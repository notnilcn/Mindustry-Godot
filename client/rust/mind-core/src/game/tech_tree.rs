// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Tech-tree **runtime** (`mindustry.content.TechTree.TechNode` research state)
//! and the research spend/unlock flow (plan 12 M2).
//!
//! The graph itself (`TechStore`/`TechNode`/`TechTreeBuilder`) is plan 02's
//! `content::tech`; this module owns `finishedRequirements`, research spending
//! (`ResearchDialog.spend`), unlock propagation (parents in MP) and
//! `Control.checkAutoUnlocks`. Single-player persistence uses the `<name>-unlocked`
//! and `req-<content>-<item>` settings keys through the plan-04
//! [`UnlockStore`]; multiplayer reads `Rules.researched` (host-authoritative).

use crate::content::ctype::UnlockFields;
use crate::content::load::ContentRegistry;
use crate::content::settings_store::UnlockStore;
use crate::content::stacks::ItemStack;
use crate::content::tech::{ObjectiveSpec, TechNode, TechNodeRef, content_research_requirements};
use crate::content::{
    BlockId, ContentRef, ContentType, ItemId, LiquidId, PlanetId, SectorId, StatusId, TeamEntryId,
    UnitTypeId, WeatherId,
};
use crate::world::modules::ItemModule;

use super::objectives::ObjectiveContext;
use super::rules::Rules;

/// Result of a research spend attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct SpendResult {
    /// Items actually removed from the inventory this call.
    pub spent: Vec<ItemStack>,
    /// Whether the node's requirements are now fully satisfied.
    pub complete: bool,
    /// Content unlocked by this call (node + parents), in unlock order.
    pub unlocked: Vec<ContentRef>,
}

/// Whether every requirement of `node` is satisfied.
pub fn requirements_complete(node: &TechNode) -> bool {
    node.requirements
        .iter()
        .zip(&node.finished_requirements)
        .all(|(requirement, finished)| finished.amount >= requirement.amount)
}

/// Effective research requirements of a node.
///
/// Returns the materialized `TechNode.requirements` when present. The generated
/// vanilla trees left the list empty (plan-02 `TechNode.requirements` gap), so
/// the content's `UnlockableContent.researchRequirements()` is derived on demand
/// and scaled by the node's inherited `research_cost_multipliers` exactly like
/// the builder scales explicit lists.
pub fn effective_requirements(registry: &ContentRegistry, node_ref: TechNodeRef) -> Vec<ItemStack> {
    let Some(node) = registry.tech().node(node_ref) else {
        return Vec::new();
    };
    if !node.requirements.is_empty() {
        return node.requirements.clone();
    }
    let Some(content) = node.content else {
        return Vec::new();
    };
    let mut out = content_research_requirements(registry, content);
    if !node.research_cost_multipliers.is_empty() {
        for stack in &mut out {
            let scale = node
                .research_cost_multipliers
                .iter()
                .find(|(item, _)| *item == stack.item)
                .map(|(_, value)| *value)
                .unwrap_or(1.0);
            stack.amount = (stack.amount as f32 * scale) as i32;
        }
    }
    out
}

/// `ResearchDialog.canSpend`: dependencies and objectives satisfied and not
/// already unlocked.
pub fn can_spend(
    registry: &ContentRegistry,
    node_ref: TechNodeRef,
    items: &ItemModule,
    ctx: &dyn ObjectiveContext,
) -> bool {
    let Some(node) = registry.tech().node(node_ref) else {
        return false;
    };
    if node
        .content
        .is_some_and(|content| content_unlocked(registry, content))
    {
        return false;
    }
    if !dependencies_unlocked(registry, node_ref) {
        return false;
    }
    if !objectives_complete(registry, &node.objectives, ctx) {
        return false;
    }
    // Requirements come from the node's materialized list, or the content's
    // `researchRequirements()` when the generated tree left it empty.
    let requirements = effective_requirements(registry, node_ref);
    requirements.iter().enumerate().all(|(index, requirement)| {
        let finished = node
            .finished_requirements
            .get(index)
            .map(|stack| stack.amount)
            .unwrap_or(0);
        finished >= requirement.amount || items.get(requirement.item) > 0
    })
}

/// `ResearchDialog.selectable`: all parents unlocked.
pub fn dependencies_unlocked(registry: &ContentRegistry, node_ref: TechNodeRef) -> bool {
    let mut current = registry.tech().node(node_ref).and_then(|node| node.parent);
    while let Some(parent) = current {
        let Some(record) = registry.tech().node(parent) else {
            return false;
        };
        if record
            .content
            .is_some_and(|content| !content_unlocked(registry, content))
        {
            return false;
        }
        current = record.parent;
    }
    true
}

/// Whether every objective in `objectives` is met under `ctx`.
pub fn objectives_complete(
    registry: &ContentRegistry,
    objectives: &[ObjectiveSpec],
    ctx: &dyn ObjectiveContext,
) -> bool {
    let _ = registry;
    objectives
        .iter()
        .all(|objective| super::objectives::Objective::from_spec(*objective).complete(ctx))
}

/// Port of `ResearchDialog.spend`: takes `min(remaining, available)` from the
/// inventory per requirement, marks progress, and unlocks when complete.
///
/// `network_client` mirrors the upstream early return (`net.client()`); clients
/// wait for the host's relayed `research`.
pub fn spend(
    registry: &mut ContentRegistry,
    node_ref: TechNodeRef,
    items: &mut ItemModule,
    store: &mut dyn UnlockStore,
    ctx: &dyn ObjectiveContext,
    network_client: bool,
) -> Result<SpendResult, SpendError> {
    if network_client {
        return Err(SpendError::ClientReadOnly);
    }
    let Some(node) = registry.tech().node(node_ref) else {
        return Err(SpendError::UnknownNode);
    };
    if !dependencies_unlocked(registry, node_ref) {
        return Err(SpendError::Locked);
    }
    if !objectives_complete(registry, &node.objectives, ctx) {
        return Err(SpendError::IncompleteObjectives);
    }

    // Materialize the content-derived requirements when the generated tree left
    // the node's list empty (plan-02 `TechNode.requirements` gap). Persisted
    // `req-` progress is loaded the same way `TechNode.setupRequirements` does
    // at build time. Nodes with an explicit list are untouched.
    if registry
        .tech()
        .node(node_ref)
        .is_some_and(|node| node.requirements.is_empty())
    {
        let effective = effective_requirements(registry, node_ref);
        if !effective.is_empty() {
            let content_name = registry
                .tech()
                .node(node_ref)
                .map(|node| node.content_name.clone())
                .unwrap_or_default();
            let finished: Vec<ItemStack> = effective
                .iter()
                .map(|stack| {
                    let item_name = registry
                        .item(stack.item)
                        .map(|item| item.name.as_str())
                        .unwrap_or("<unknown>");
                    let key = TechNode::requirement_key(&content_name, item_name);
                    ItemStack::new(stack.item, store.get_i32(&key))
                })
                .collect();
            if let Some(record) = registry.tech.node_mut(node_ref) {
                record.requirements = effective;
                record.finished_requirements = finished;
            }
        }
    }

    let requirement_count = registry
        .tech()
        .node(node_ref)
        .map(|node| node.requirements.len())
        .unwrap_or(0);
    let mut spent = Vec::new();
    let mut complete = true;

    for index in 0..requirement_count {
        let Some(node) = registry.tech.node(node_ref) else {
            return Err(SpendError::UnknownNode);
        };
        let item = node.requirements[index].item;
        let required = node.requirements[index].amount;
        let finished = node.finished_requirements[index].amount;
        let remaining = (required - finished).max(0);
        let available = items.get(item);
        let used = remaining.min(available).max(0);
        if used > 0 {
            items.remove(item, used);
            spent.push(ItemStack::new(item, used));
        }
        if let Some(node) = registry.tech.node_mut(node_ref) {
            node.finished_requirements[index].amount += used;
            if node.finished_requirements[index].amount < required {
                complete = false;
            }
        }
    }

    let mut unlocked = Vec::new();
    if complete {
        unlocked = unlock(registry, node_ref, store);
    }
    // `node.save()`: persist requirement deltas only.
    if let Some(node) = registry.tech().node(node_ref).cloned() {
        node.save(registry, store);
    }
    Ok(SpendResult {
        spent,
        complete,
        unlocked,
    })
}

/// Port of `ResearchDialog.unlock`: unlock the node content and all parents
/// (multiplayer), returning the newly-unlocked content in child→parent order.
pub fn unlock(
    registry: &mut ContentRegistry,
    node_ref: TechNodeRef,
    store: &mut dyn UnlockStore,
) -> Vec<ContentRef> {
    let mut unlocked = Vec::new();
    let mut current = Some(node_ref);
    while let Some(reference) = current {
        let content = registry
            .tech()
            .node(reference)
            .and_then(|node| node.content);
        let parent = registry.tech().node(reference).and_then(|node| node.parent);
        if let Some(content) = content {
            let name = content_name(registry, content);
            if let Some(name) = name
                && let Some(fields) = unlock_fields_mut(registry, content)
                && fields.unlock(&name, store)
            {
                unlocked.push(content);
            }
        }
        current = parent;
    }
    unlocked
}

/// `Control.checkAutoUnlocks`: unlock every parent-unlocked, zero-requirement
/// node with no incomplete objectives. Returns the newly-unlocked content.
///
/// Iterates in node order and unlocks immediately, so a child whose parent is
/// unlocked earlier in the same pass is also handled (matching upstream).
pub fn check_auto_unlocks(
    registry: &mut ContentRegistry,
    store: &mut dyn UnlockStore,
    ctx: &dyn ObjectiveContext,
) -> Vec<ContentRef> {
    let mut unlocked = Vec::new();
    for index in 0..registry.tech.nodes.len() {
        let reference = TechNodeRef(index as u32);
        let Some(node) = registry.tech.node(reference) else {
            continue;
        };
        let Some(content) = node.content else {
            continue;
        };
        if content_unlocked(registry, content) {
            continue;
        }
        let parent_unlocked = match node.parent {
            None => true,
            Some(parent) => registry
                .tech
                .node(parent)
                .and_then(|record| record.content)
                .is_some_and(|parent_content| content_unlocked(registry, parent_content)),
        };
        if !parent_unlocked || !node.requirements.is_empty() {
            continue;
        }
        let objectives = node.objectives.clone();
        if !objectives_complete(registry, &objectives, ctx) {
            continue;
        }
        unlocked.extend(unlock(registry, reference, store));
    }
    unlocked
}

/// `node.reset()` for every node (new-profile reset).
pub fn reset_all(registry: &mut ContentRegistry) {
    for node in &mut registry.tech.nodes {
        node.reset();
    }
}

/// Harness/test helper: appends a tech node with an explicit requirement list.
///
/// Used by `mind-headless campaign tech` and the unit tests to build synthetic
/// nodes that are independent of the content registry. Vanilla nodes derive
/// their costs through [`effective_requirements`].
pub fn push_node(
    registry: &mut ContentRegistry,
    depth: u32,
    parent: Option<TechNodeRef>,
    content: Option<ContentRef>,
    content_name: &str,
    requirements: Vec<ItemStack>,
) -> TechNodeRef {
    let root_node = parent.and_then(|parent_ref| {
        registry
            .tech
            .node(parent_ref)
            .map(|node| node.root_node.unwrap_or(parent_ref))
    });
    let reference = TechNodeRef(registry.tech.nodes.len() as u32);
    registry.tech.nodes.push(TechNode {
        depth,
        name: None,
        requires_unlock: false,
        parent,
        root_node,
        research_cost_multipliers: Vec::new(),
        content,
        content_name: content_name.to_owned(),
        finished_requirements: requirements
            .iter()
            .map(|stack| ItemStack::new(stack.item, 0))
            .collect(),
        requirements,
        objectives: Vec::new(),
        children: Vec::new(),
        planet: None,
        tree: None,
    });
    if let Some(parent_ref) = parent
        && let Some(record) = registry.tech.node_mut(parent_ref)
    {
        record.children.push(reference);
    }
    reference
}

/// `unlockedHost()`/`unlockedNow()`: SP reads the settings-backed
/// `UnlockFields.unlocked`; MP (`rules` present) is authoritative on
/// `Rules.researched`.
pub fn content_unlocked(registry: &ContentRegistry, content: ContentRef) -> bool {
    unlock_fields(registry, content).is_some_and(UnlockFields::unlocked)
}

/// Multiplayer-aware unlock check: `Rules.researched` wins when set.
pub fn content_unlocked_with_rules(
    registry: &ContentRegistry,
    content: ContentRef,
    rules: &Rules,
) -> bool {
    if let Some(name) = content_name(registry, content)
        && rules.researched.iter().any(|entry| entry == &name)
    {
        return true;
    }
    content_unlocked(registry, content)
}

/// Content name for a reference (via the plan-02 name table).
pub fn content_name(registry: &ContentRegistry, content: ContentRef) -> Option<String> {
    crate::content::tech::content_name_of(registry, content)
}

/// Read-only `UnlockFields` for an unlockable reference.
pub fn unlock_fields(registry: &ContentRegistry, content: ContentRef) -> Option<&UnlockFields> {
    match content.type_ {
        ContentType::Item => registry.item(ItemId::new(content.id)).map(|r| &r.unlock),
        ContentType::Block => registry.block(BlockId::new(content.id)).map(|r| &r.unlock),
        ContentType::Liquid => registry
            .liquid(LiquidId::new(content.id))
            .map(|r| &r.unlock),
        ContentType::Status => registry
            .status(StatusId::new(content.id))
            .map(|r| &r.unlock),
        ContentType::Unit => registry
            .unit(UnitTypeId::new(content.id))
            .map(|r| &r.unlock),
        ContentType::Weather => registry
            .weather(WeatherId::new(content.id))
            .map(|r| &r.unlock),
        ContentType::Sector => registry
            .sector(SectorId::new(content.id))
            .map(|r| &r.unlock),
        ContentType::Planet => registry
            .planet(PlanetId::new(content.id))
            .map(|r| &r.unlock),
        ContentType::Team => registry
            .team(TeamEntryId::new(content.id))
            .map(|r| &r.unlock),
        _ => None,
    }
}

/// Mutable `UnlockFields` for an unlockable reference, including units (the
/// plan-02 `with_unlock_fields` helper omits `ContentType::Unit`).
pub fn unlock_fields_mut(
    registry: &mut ContentRegistry,
    content: ContentRef,
) -> Option<&mut UnlockFields> {
    match content.type_ {
        ContentType::Item => registry
            .item_mut(ItemId::new(content.id))
            .map(|r| &mut r.unlock),
        ContentType::Block => registry
            .block_mut(BlockId::new(content.id))
            .map(|r| &mut r.unlock),
        ContentType::Liquid => registry
            .liquid_mut(LiquidId::new(content.id))
            .map(|r| &mut r.unlock),
        ContentType::Status => registry
            .status_mut(StatusId::new(content.id))
            .map(|r| &mut r.unlock),
        ContentType::Unit => registry
            .unit_mut(UnitTypeId::new(content.id))
            .map(|r| &mut r.unlock),
        ContentType::Weather => registry
            .weather_mut(WeatherId::new(content.id))
            .map(|r| &mut r.unlock),
        ContentType::Sector => registry
            .sector_mut(SectorId::new(content.id))
            .map(|r| &mut r.unlock),
        ContentType::Planet => registry
            .planet_mut(PlanetId::new(content.id))
            .map(|r| &mut r.unlock),
        ContentType::Team => registry
            .team_mut(TeamEntryId::new(content.id))
            .map(|r| &mut r.unlock),
        _ => None,
    }
}

/// Research errors (structured; never panics on gameplay data).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpendError {
    /// The node reference is not in the store.
    #[error("unknown tech node")]
    UnknownNode,
    /// A dependency (parent) is still locked.
    #[error("tech node is locked by an incomplete parent")]
    Locked,
    /// An objective remains incomplete.
    #[error("tech node has incomplete objectives")]
    IncompleteObjectives,
    /// Clients cannot spend locally; the host relays the result.
    #[error("research is host-authoritative in multiplayer")]
    ClientReadOnly,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::content::settings_store::MemoryUnlockStore;
    use crate::content::{MemoryBundle, create_base_content};
    use crate::world::modules::ItemModule;

    struct AllowAll;
    impl ObjectiveContext for AllowAll {
        fn is_unlocked(&self, _content: ContentRef) -> bool {
            true
        }
        fn sector_status(&self, _sector: SectorId) -> super::super::objectives::SectorStatus {
            super::super::objectives::SectorStatus::default()
        }
        fn planet_has_base(&self, _planet: PlanetId) -> bool {
            false
        }
    }

    fn registry() -> (ContentRegistry, MemoryUnlockStore) {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        let registry = create_base_content(&bundle, &store, true).unwrap();
        (registry, store)
    }

    #[test]
    fn vanilla_trees_have_nodes() {
        let (registry, _store) = registry();
        assert!(!registry.tech().nodes.is_empty());
    }

    #[test]
    fn spend_takes_items_and_persists_req_keys() {
        let (mut registry, mut store) = registry();
        let (parent_ref, node_ref, item, need) = synthetic_pair(&mut registry);
        let parent_content = node_content(&registry, parent_ref);
        unlock_fields_mut(&mut registry, parent_content)
            .unwrap()
            .unlocked = true;
        let mut items = ItemModule::with_items(registry.items().len());
        items.add(item, need + 5, 1_000);

        let result = spend(
            &mut registry,
            node_ref,
            &mut items,
            &mut store,
            &AllowAll,
            false,
        )
        .unwrap();
        assert!(result.complete, "requirements met");
        assert_eq!(result.spent.len(), 1);
        assert_eq!(result.spent[0].amount, need);
        assert_eq!(items.get(item), 5);
        let content = registry.tech().node(node_ref).unwrap().content.unwrap();
        assert!(content_unlocked(&registry, content), "unlocked after spend");
        // `req-` key saved.
        let name = content_name(&registry, content).unwrap();
        let expected = format!("req-{name}-{}", item_name(&registry, item));
        assert_eq!(store.get_i32(&expected), need);
    }

    /// Vanilla nodes carry no materialized `requirements`; spend/gating derive
    /// the real `UnlockableContent.researchRequirements()` on demand and consume
    /// exactly those items (plan-02 gap reconciliation).
    #[test]
    fn vanilla_research_uses_content_requirements() {
        let (mut registry, mut store) = registry();
        let (node_ref, parent_ref, effective) = (0..registry.tech().nodes.len())
            .find_map(|index| {
                let node_ref = TechNodeRef(index as u32);
                let node = registry.tech().node(node_ref)?;
                if node.content.is_none() || !node.requirements.is_empty() {
                    return None;
                }
                let effective = effective_requirements(&registry, node_ref);
                if effective.is_empty() {
                    return None;
                }
                Some((node_ref, node.parent?, effective))
            })
            .expect("a vanilla node with derived research requirements");
        assert!(
            registry
                .tech()
                .node(node_ref)
                .unwrap()
                .requirements
                .is_empty(),
            "generated tree did not materialize the list"
        );

        let _ = unlock(&mut registry, parent_ref, &mut store);
        let mut items = ItemModule::with_items(registry.items().len());
        assert!(
            !can_spend(&registry, node_ref, &items, &AllowAll),
            "an empty inventory cannot research a costed node"
        );

        for stack in &effective {
            items.add(stack.item, stack.amount + 5, 1_000_000);
        }
        assert!(can_spend(&registry, node_ref, &items, &AllowAll));
        let result = spend(
            &mut registry,
            node_ref,
            &mut items,
            &mut store,
            &AllowAll,
            false,
        )
        .unwrap();
        assert!(result.complete);
        for stack in &effective {
            assert_eq!(items.get(stack.item), 5, "only the requirement was spent");
        }
        assert_eq!(
            registry.tech().node(node_ref).unwrap().requirements,
            effective,
            "the runtime materialized the derived list"
        );
        let content = registry.tech().node(node_ref).unwrap().content.unwrap();
        assert!(content_unlocked(&registry, content));
    }

    #[test]
    fn locked_node_is_rejected() {
        let (mut registry, mut store) = registry();
        let (_, node_ref, item, need) = synthetic_pair(&mut registry);
        let mut items = ItemModule::with_items(registry.items().len());
        items.add(item, need, 1_000);
        // Parent not unlocked -> Locked.
        let err = spend(
            &mut registry,
            node_ref,
            &mut items,
            &mut store,
            &AllowAll,
            false,
        )
        .unwrap_err();
        assert_eq!(err, SpendError::Locked);
        // Client is read-only.
        let err = spend(
            &mut registry,
            node_ref,
            &mut items,
            &mut store,
            &AllowAll,
            true,
        )
        .unwrap_err();
        assert_eq!(err, SpendError::ClientReadOnly);
    }

    #[test]
    fn auto_unlocks_only_zero_requirement_rooted_nodes() {
        let (mut registry, mut store) = registry();
        let (_parent_ref, child_ref, _, _) =
            synthetic_pair_with_child(&mut registry, BlockId::new(60_000));
        let unlocked = check_auto_unlocks(&mut registry, &mut store, &AllowAll);
        // Every item that was auto-unlocked must have had no requirements.
        for content in &unlocked {
            let node = registry
                .tech()
                .nodes
                .iter()
                .find(|node| node.content == Some(*content))
                .unwrap();
            assert!(node.requirements.is_empty());
            assert!(content_unlocked(&registry, *content));
        }
        // The requirement-bearing child is not auto-unlocked.
        let child_content = node_content(&registry, child_ref);
        assert!(!content_unlocked(&registry, child_content));
        // Idempotent: a second pass unlocks nothing new.
        let again = check_auto_unlocks(&mut registry, &mut store, &AllowAll);
        assert!(again.is_empty());
    }

    #[test]
    fn rules_researched_overrides_host_store() {
        let (registry, _store) = registry();
        let content = registry
            .tech()
            .nodes
            .iter()
            .find_map(|node| node.content)
            .unwrap();
        let name = content_name(&registry, content).unwrap();
        let mut rules = Rules::default();
        assert!(!content_unlocked_with_rules(&registry, content, &rules));
        rules.researched.insert(name);
        assert!(content_unlocked_with_rules(&registry, content, &rules));
    }

    // --- helpers ---

    fn item_name(registry: &ContentRegistry, item: ItemId) -> String {
        registry
            .item(item)
            .map(|record| record.name.clone())
            .unwrap()
    }

    fn node_content(registry: &ContentRegistry, node_ref: TechNodeRef) -> ContentRef {
        registry.tech().node(node_ref).unwrap().content.unwrap()
    }

    /// Pushes a synthetic parent (`router`, no requirements) and child (one
    /// copper requirement) onto the tech store for spend/unlock tests.
    fn synthetic_pair(registry: &mut ContentRegistry) -> (TechNodeRef, TechNodeRef, ItemId, i32) {
        synthetic_pair_with_child(registry, registry.block_id("duo").unwrap())
    }

    fn synthetic_pair_with_child(
        registry: &mut ContentRegistry,
        child_block: BlockId,
    ) -> (TechNodeRef, TechNodeRef, ItemId, i32) {
        let copper = registry.item_id("copper").unwrap();
        let need = 30;
        let parent_content =
            ContentRef::of(ContentType::Block, registry.block_id("router").unwrap());
        let parent_ref = TechNodeRef(registry.tech.nodes.len() as u32);
        registry.tech.nodes.push(TechNode {
            depth: 0,
            name: Some("synthetic-root".to_owned()),
            requires_unlock: false,
            parent: None,
            root_node: None,
            research_cost_multipliers: Vec::new(),
            content: Some(parent_content),
            content_name: "router".to_owned(),
            requirements: Vec::new(),
            finished_requirements: Vec::new(),
            objectives: Vec::new(),
            children: Vec::new(),
            planet: None,
            tree: None,
        });

        let child_content = ContentRef::of(ContentType::Block, child_block);
        let child_ref = TechNodeRef(registry.tech.nodes.len() as u32);
        registry.tech.nodes.push(TechNode {
            depth: 1,
            name: None,
            requires_unlock: false,
            parent: Some(parent_ref),
            root_node: Some(parent_ref),
            research_cost_multipliers: Vec::new(),
            content: Some(child_content),
            content_name: "duo".to_owned(),
            requirements: vec![ItemStack::new(copper, need)],
            finished_requirements: vec![ItemStack::new(copper, 0)],
            objectives: Vec::new(),
            children: Vec::new(),
            planet: None,
            tree: None,
        });
        registry.tech.nodes[parent_ref.index()]
            .children
            .push(child_ref);
        (parent_ref, child_ref, copper, need)
    }
}

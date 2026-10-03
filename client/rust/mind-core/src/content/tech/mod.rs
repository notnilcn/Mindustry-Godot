// SPDX-License-Identifier: GPL-3.0-only

//! Tech-tree graph and builder.
//!
//! Ported from `core/src/mindustry/content/TechTree.java` (builder context,
//! `TechNode` construction, materialized `SectorComplete` objectives, research
//! cost multipliers) plus the node data of `SerpuloTechTree.java` /
//! `ErekirTechTree.java`. Runtime research state is plan 12.
//!
//! The vanilla trees are ported ahead of the block/unit registries (M3/M5):
//! nodes whose content name does not resolve yet are still created with
//! `content = None` and reported in [`TechTreeBuildReport::missing`], so the
//! same verbatim data resolves completely once those registries land.

pub mod ekir;
pub mod serpulo;

use super::ctype::UnlockFields;
use super::id::{BlockId, ItemId, PlanetId, SectorId, UnitTypeId};
use super::load::ContentRegistry;
use super::settings_store::UnlockStore;
use super::stacks::ItemStack;
use super::{ContentRef, ContentType};

/// Reference to one node in [`TechStore::nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TechNodeRef(pub u32);

impl TechNodeRef {
    /// Raw index.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Reference to one tree in [`TechStore::trees`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TreeId(pub u16);

impl TreeId {
    /// Raw index.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Materialized objective (`mindustry.game.Objectives`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectiveSpec {
    /// `Objectives.SectorComplete`.
    SectorComplete(SectorId),
    /// `Objectives.Research`.
    Research(ContentRef),
    /// `Objectives.Produce`.
    Produce(ContentRef),
    /// `Objectives.OnSector`.
    OnSector(SectorId),
    /// `Objectives.OnPlanet`.
    OnPlanet(PlanetId),
}

/// Declarative objective used by the generated tree data (resolved at build time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeObjective {
    /// `new SectorComplete(preset)`.
    SectorComplete(&'static str),
    /// `new OnSector(preset)`.
    OnSector(&'static str),
    /// `new OnPlanet(planet)`.
    OnPlanet(&'static str),
    /// `new Research(content)`.
    Research(&'static str),
    /// `nodeProduce`'s implicit `new Produce(content)`.
    Produce(&'static str),
}

/// One tech-tree node (`TechTree.TechNode`).
#[derive(Debug, Clone, PartialEq)]
pub struct TechNode {
    /// Depth in the tree.
    pub depth: u32,
    /// Root name (`nodeRoot`), used by the tree selector.
    pub name: Option<String>,
    /// For roots only: needs unlocking before being selectable.
    pub requires_unlock: bool,
    /// Parent node.
    pub parent: Option<TechNodeRef>,
    /// Root node of this tree.
    pub root_node: Option<TechNodeRef>,
    /// Per-item research cost multipliers inherited from the parent.
    pub research_cost_multipliers: Vec<(ItemId, f32)>,
    /// Researched content reference; `None` until the owning registry lands.
    pub content: Option<ContentRef>,
    /// Content name (always present; audit/debug for unresolved nodes).
    pub content_name: String,
    /// Item requirements.
    pub requirements: Vec<ItemStack>,
    /// Fulfilled requirements (same length as `requirements`).
    pub finished_requirements: Vec<ItemStack>,
    /// Extra objectives needed to research this.
    pub objectives: Vec<ObjectiveSpec>,
    /// Nodes that depend on this node.
    pub children: Vec<TechNodeRef>,
    /// Planet associated with this node (auto-detected from the root).
    pub planet: Option<PlanetId>,
    /// Owning tree (extension over upstream; `None` before finish).
    pub tree: Option<TreeId>,
}

impl TechNode {
    /// `TechNode.setupRequirements` settings key for one requirement.
    pub fn requirement_key(content_name: &str, item_name: &str) -> String {
        format!("req-{content_name}-{item_name}")
    }

    /// `TechNode.reset`: zeroes finished requirements.
    pub fn reset(&mut self) {
        for stack in &mut self.finished_requirements {
            stack.amount = 0;
        }
    }

    /// `TechNode.save`: flushes only changed finished requirements to settings.
    pub fn save(&self, registry: &ContentRegistry, store: &mut dyn UnlockStore) {
        for stack in &self.finished_requirements {
            let item_name = registry
                .item(stack.item)
                .map(|item| item.name.as_str())
                .unwrap_or("<unknown>");
            let key = Self::requirement_key(&self.content_name, item_name);
            if store.get_i32(&key) != stack.amount {
                store.set_i32(&key, stack.amount);
            }
        }
    }
}

/// One tech tree (`TechTree.roots` entry with its `nodeRoot` name).
#[derive(Debug, Clone, PartialEq)]
pub struct TechTree {
    /// Root name (`techtree.<name>` bundle key).
    pub name: String,
    /// Root node.
    pub root: TechNodeRef,
}

/// All tech nodes (`TechTree.all` + `roots`).
#[derive(Debug, Clone, Default)]
pub struct TechStore {
    /// Every node, in creation order.
    pub nodes: Vec<TechNode>,
    /// Root nodes (`TechTree.roots`).
    pub roots: Vec<TechNodeRef>,
    /// Trees in load order.
    pub trees: Vec<TechTree>,
}

impl TechStore {
    /// Node by reference.
    pub fn node(&self, node: TechNodeRef) -> Option<&TechNode> {
        self.nodes.get(node.index())
    }

    /// Mutable node by reference.
    pub fn node_mut(&mut self, node: TechNodeRef) -> Option<&mut TechNode> {
        self.nodes.get_mut(node.index())
    }

    /// Recursively visits `node` and all descendants (`TechNode.each`).
    pub fn each<F: FnMut(TechNodeRef)>(&self, node: TechNodeRef, visitor: &mut F) {
        visitor(node);
        if let Some(record) = self.node(node) {
            for child in &record.children {
                self.each(*child, visitor);
            }
        }
    }

    /// Tree by id.
    pub fn tree(&self, tree: TreeId) -> Option<&TechTree> {
        self.trees.get(tree.index())
    }
}

/// Result of building one tree (audit surface; plan 02 M2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TechTreeBuildReport {
    /// Content/objective names that could not resolve yet (M3/M5 registries).
    pub missing: Vec<String>,
    /// Nodes created with resolved content.
    pub resolved_nodes: usize,
    /// Nodes created without content (pending registry landing).
    pub unresolved_nodes: usize,
}

/// `TechTree.node`/`nodeRoot` builder.
///
/// The generated tree files call [`TechTreeBuilder::root`], then flat
/// [`TechTreeBuilder::at`] calls whose `depth` reproduces the Java closure
/// nesting; the builder maintains the parent stack internally.
pub struct TechTreeBuilder<'r> {
    registry: &'r mut ContentRegistry,
    store: &'r dyn UnlockStore,
    report: TechTreeBuildReport,
    stack: Vec<TechNodeRef>,
    root: Option<TechNodeRef>,
    tree_name: String,
    start: usize,
}

impl<'r> TechTreeBuilder<'r> {
    /// Creates a builder bound to a registry and unlock store.
    pub fn new(registry: &'r mut ContentRegistry, store: &'r dyn UnlockStore) -> Self {
        let start = registry.tech.nodes.len();
        Self {
            registry,
            store,
            report: TechTreeBuildReport::default(),
            stack: Vec::new(),
            root: None,
            tree_name: String::new(),
            start,
        }
    }

    /// Build report (missing names etc.).
    pub fn report(&self) -> &TechTreeBuildReport {
        &self.report
    }

    /// `TechTree.nodeRoot(name, content, children)`.
    pub fn root(&mut self, name: &str, content_name: &str, requires_unlock: bool) {
        let (content, _) = self.resolve_content(content_name);
        let node = self.push_node(
            0,
            content,
            content_name,
            Vec::new(),
            requires_unlock,
            Some(name),
        );
        self.root = Some(node);
        self.tree_name = name.to_owned();
    }

    /// `context().researchCostMultipliers = costMultipliers` on the root.
    pub fn set_cost_multipliers(&mut self, multipliers: &[(&str, f32)]) {
        let converted: Vec<(ItemId, f32)> = multipliers
            .iter()
            .filter_map(|(name, value)| self.registry.item_id(name).map(|id| (id, *value)))
            .collect();
        if let Some(root) = self.root
            && let Some(node) = self.registry.tech.node_mut(root)
        {
            node.research_cost_multipliers = converted;
        }
    }

    /// `TechTree.node(content, children)` with default requirements.
    pub fn at(&mut self, depth: u32, content_name: &str) -> TechNodeRef {
        self.at_full_inner(depth, content_name, None, &[])
    }

    /// `TechTree.node(content, Seq<Objective>, children)`.
    pub fn at_obj(
        &mut self,
        depth: u32,
        content_name: &str,
        objectives: &[NodeObjective],
    ) -> TechNodeRef {
        self.at_full_inner(depth, content_name, None, objectives)
    }

    /// `TechTree.node(content, ItemStack[], children)`.
    pub fn at_req(
        &mut self,
        depth: u32,
        content_name: &str,
        requirements: &[(&str, i32)],
    ) -> TechNodeRef {
        self.at_full_inner(depth, content_name, Some(requirements), &[])
    }

    /// `TechTree.node(content, ItemStack[], Seq<Objective>, children)`.
    pub fn at_full(
        &mut self,
        depth: u32,
        content_name: &str,
        requirements: &[(&str, i32)],
        objectives: &[NodeObjective],
    ) -> TechNodeRef {
        self.at_full_inner(depth, content_name, Some(requirements), objectives)
    }

    fn at_full_inner(
        &mut self,
        depth: u32,
        content_name: &str,
        requirements: Option<&[(&str, i32)]>,
        objectives: &[NodeObjective],
    ) -> TechNodeRef {
        let parent = self.parent_for(depth);
        let (content, _) = self.resolve_content(content_name);

        // Requirements: explicit list or `content.researchRequirements()`.
        let mut resolved_requirements: Vec<ItemStack> = match requirements {
            Some(list) => list
                .iter()
                .filter_map(|(name, amount)| {
                    self.registry
                        .item_id(name)
                        .map(|item| ItemStack::new(item, *amount))
                })
                .collect(),
            None => Vec::new(),
        };

        // Research cost multipliers inherit from the parent and scale requirements.
        let multipliers = parent
            .and_then(|parent| self.registry.tech.node(parent))
            .map(|parent| parent.research_cost_multipliers.clone())
            .unwrap_or_default();
        if !multipliers.is_empty() && !resolved_requirements.is_empty() {
            for stack in &mut resolved_requirements {
                let scale = multipliers
                    .iter()
                    .find(|(item, _)| *item == stack.item)
                    .map(|(_, value)| *value)
                    .unwrap_or(1.0);
                stack.amount = (stack.amount as f32 * scale) as i32;
            }
        }

        // Materialize objectives and insert the missing sector dependency.
        let mut resolved_objectives = self.resolve_objectives(objectives);
        if let Some(parent_node) = parent
            && let Some(parent_record) = self.registry.tech.node(parent_node)
            && let Some(ContentRef {
                type_: ContentType::Sector,
                id,
            }) = parent_record.content
        {
            let sector = SectorId::new(id);
            if !resolved_objectives
                .iter()
                .any(|objective| matches!(objective, ObjectiveSpec::SectorComplete(found) if *found == sector))
            {
                resolved_objectives.insert(0, ObjectiveSpec::SectorComplete(sector));
            }
        }

        let node = self.push_node(
            depth,
            content,
            content_name,
            resolved_requirements.clone(),
            false,
            None,
        );
        if let Some(record) = self.registry.tech.node_mut(node) {
            record.objectives = resolved_objectives;
            record.requirements = resolved_requirements;
            record.finished_requirements = record.requirements.clone();
        }
        // `setupRequirements`: load fulfilled amounts from settings.
        let finished: Vec<ItemStack> = {
            let record = self.registry.tech.node(node);
            record
                .map(|record| {
                    record
                        .requirements
                        .iter()
                        .map(|stack| {
                            let item_name = self
                                .registry
                                .item(stack.item)
                                .map(|item| item.name.as_str())
                                .unwrap_or("<unknown>");
                            let key = TechNode::requirement_key(content_name, item_name);
                            ItemStack::new(stack.item, self.store.get_i32(&key))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        if let Some(record) = self.registry.tech.node_mut(node) {
            record.finished_requirements = finished;
        }
        node
    }

    fn push_node(
        &mut self,
        depth: u32,
        content: Option<ContentRef>,
        content_name: &str,
        requirements: Vec<ItemStack>,
        requires_unlock: bool,
        name: Option<&str>,
    ) -> TechNodeRef {
        let parent = self.parent_for(depth);
        let root_node = match parent {
            Some(parent) => self
                .registry
                .tech
                .node(parent)
                .and_then(|record| record.root_node.or(Some(parent))),
            None => None,
        };
        // A root's own planet is assigned later via `set_planet_tech_tree`.
        let planet = parent
            .and_then(|parent| self.registry.tech.node(parent))
            .and_then(|record| record.planet);
        let node = TechNode {
            depth,
            name: name.map(str::to_owned),
            requires_unlock,
            parent,
            root_node,
            research_cost_multipliers: parent
                .and_then(|parent| self.registry.tech.node(parent))
                .map(|record| record.research_cost_multipliers.clone())
                .unwrap_or_default(),
            content,
            content_name: content_name.to_owned(),
            requirements,
            finished_requirements: Vec::new(),
            objectives: Vec::new(),
            children: Vec::new(),
            planet,
            tree: None,
        };
        let reference = TechNodeRef(self.registry.tech.nodes.len() as u32);
        self.registry.tech.nodes.push(node);
        if let Some(parent) = parent
            && let Some(record) = self.registry.tech.node_mut(parent)
        {
            record.children.push(reference);
        }
        if let Some(content) = content {
            self.registry.set_tech_node(content, reference);
            self.report.resolved_nodes += 1;
        } else {
            self.report.unresolved_nodes += 1;
        }
        // Maintain the pre-order parent stack.
        while self
            .stack
            .last()
            .and_then(|node| self.registry.tech.node(*node))
            .is_some_and(|record| record.depth >= depth)
        {
            self.stack.pop();
        }
        self.stack.push(reference);
        reference
    }

    fn parent_for(&self, depth: u32) -> Option<TechNodeRef> {
        if depth == 0 {
            return None;
        }
        self.stack
            .iter()
            .rev()
            .find(|node| {
                self.registry
                    .tech
                    .node(**node)
                    .is_some_and(|record| record.depth == depth - 1)
            })
            .copied()
    }

    fn resolve_content(&mut self, name: &str) -> (Option<ContentRef>, Option<ContentType>) {
        match self.registry.by_name(name) {
            Some(reference) => (Some(reference), Some(reference.type_)),
            None => {
                self.report.missing.push(name.to_owned());
                (None, None)
            }
        }
    }

    fn resolve_objectives(&mut self, objectives: &[NodeObjective]) -> Vec<ObjectiveSpec> {
        let mut out = Vec::with_capacity(objectives.len());
        for objective in objectives {
            match objective {
                NodeObjective::SectorComplete(name) => {
                    if let Some(sector) = self.registry.sector_by_name(name).map(|record| record.id)
                    {
                        out.push(ObjectiveSpec::SectorComplete(sector));
                    } else {
                        self.report.missing.push((*name).to_owned());
                    }
                }
                NodeObjective::OnSector(name) => {
                    if let Some(sector) = self.registry.sector_by_name(name).map(|record| record.id)
                    {
                        out.push(ObjectiveSpec::OnSector(sector));
                    } else {
                        self.report.missing.push((*name).to_owned());
                    }
                }
                NodeObjective::OnPlanet(name) => {
                    if let Some(planet) = self.registry.planet_by_name(name).map(|record| record.id)
                    {
                        out.push(ObjectiveSpec::OnPlanet(planet));
                    } else {
                        self.report.missing.push((*name).to_owned());
                    }
                }
                NodeObjective::Research(name) | NodeObjective::Produce(name) => {
                    match self.registry.by_name(name) {
                        Some(reference) => {
                            out.push(if matches!(objective, NodeObjective::Research(_)) {
                                ObjectiveSpec::Research(reference)
                            } else {
                                ObjectiveSpec::Produce(reference)
                            });
                        }
                        None => self.report.missing.push((*name).to_owned()),
                    }
                }
            }
        }
        out
    }

    /// `Planets.<x>.techTree = nodeRoot(...)`: installs the tree and returns it.
    pub fn finish(mut self) -> (TreeId, TechTreeBuildReport) {
        let tree_id = TreeId(self.registry.tech.trees.len() as u16);
        let root = self.root.unwrap_or(TechNodeRef(0));
        for index in self.start..self.registry.tech.nodes.len() {
            if let Some(record) = self.registry.tech.node_mut(TechNodeRef(index as u32)) {
                record.tree = Some(tree_id);
            }
        }
        self.registry.tech.roots.push(root);
        self.registry.tech.trees.push(TechTree {
            name: std::mem::take(&mut self.tree_name),
            root,
        });
        (tree_id, self.report)
    }
}

/// `Mathf.round(value / 10) * 10` used by block research requirements (plan 02 M3).
pub fn round_to_10(value: f32) -> i32 {
    ((value / 10.0).round() as i32) * 10
}

/// `UnlockableContent.researchRequirements()` dispatch.
///
/// Blocks use the `Block.researchRequirements()` cost formula ([`BlockDef`]);
/// units derive theirs from the block that produces them; every other content
/// type inherits `UnlockableContent`'s empty default. Plan 12's runtime
/// (`game::tech_tree`) consumes this to gate research with real costs where the
/// generated tree data did not materialize `TechNode.requirements`.
///
/// [`BlockDef`]: super::registries::blocks::BlockDef
pub fn content_research_requirements(
    registry: &ContentRegistry,
    content: ContentRef,
) -> Vec<ItemStack> {
    match content.type_ {
        ContentType::Block => registry
            .block(BlockId::new(content.id))
            .map(|block| block.research_requirements())
            .unwrap_or_default(),
        ContentType::Unit => registry
            .unit(UnitTypeId::new(content.id))
            .map(|unit| unit.research_requirements(registry))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Applies `f` to the unlock fields of a mappable, unlockable content record.
pub(crate) fn with_unlock_fields(
    registry: &mut ContentRegistry,
    content: ContentRef,
    f: impl FnOnce(&mut UnlockFields),
) {
    match content.type_ {
        ContentType::Item => {
            if let Some(record) = registry.item_mut(super::id::ItemId::new(content.id)) {
                f(&mut record.unlock);
            }
        }
        ContentType::Block => {
            if let Some(record) = registry.block_mut(super::id::BlockId::new(content.id)) {
                f(&mut record.unlock);
            }
        }
        ContentType::Liquid => {
            if let Some(record) = registry.liquid_mut(super::id::LiquidId::new(content.id)) {
                f(&mut record.unlock);
            }
        }
        ContentType::Status => {
            if let Some(record) = registry.status_mut(super::id::StatusId::new(content.id)) {
                f(&mut record.unlock);
            }
        }
        ContentType::Weather => {
            if let Some(record) = registry.weather_mut(super::id::WeatherId::new(content.id)) {
                f(&mut record.unlock);
            }
        }
        ContentType::Sector => {
            if let Some(record) = registry.sector_mut(super::id::SectorId::new(content.id)) {
                f(&mut record.unlock);
            }
        }
        ContentType::Planet => {
            if let Some(record) = registry.planet_mut(super::id::PlanetId::new(content.id)) {
                f(&mut record.unlock);
            }
        }
        ContentType::Team => {
            if let Some(record) = registry.team_mut(super::id::TeamEntryId::new(content.id)) {
                f(&mut record.unlock);
            }
        }
        _ => {}
    }
}

/// Resolution helper used by `set_tech_node` in the registry.
pub(crate) fn set_unlock_tech_node(
    registry: &mut ContentRegistry,
    content: ContentRef,
    node: TechNodeRef,
) {
    with_unlock_fields(registry, content, |fields| {
        fields.tech_node = Some(node);
        if !fields.tech_nodes.contains(&node) {
            fields.tech_nodes.push(node);
        }
    });
}

/// Content record name of a reference, for audit output.
pub fn content_name_of(registry: &ContentRegistry, content: ContentRef) -> Option<String> {
    registry
        .entries(content.type_)
        .into_iter()
        .find(|entry| entry.id == content.id)
        .and_then(|entry| entry.name.map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::super::bundle::MemoryBundle;
    use super::super::settings_store::MemoryUnlockStore;
    use super::super::{create_base_content, test_support};
    use super::*;

    /// `tech::sector_complete_insertion` (plan 02 §5 M2).
    #[test]
    fn sector_complete_insertion() {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        let mut registry = create_base_content(&bundle, &store, true).unwrap();
        registry.init().unwrap();
        registry.post_init().unwrap();

        let ground_zero = registry.sector_by_name("groundZero").unwrap().id;
        let frozen_forest = registry.sector_by_name("frozenForest").unwrap().id;

        let (tree, _report) = {
            let mut builder = TechTreeBuilder::new(&mut registry, &store);
            builder.root("synthetic", "serpulo", false);
            builder.at(1, "groundZero");
            builder.at(2, "copper");
            // Explicit duplicate must not be re-inserted.
            builder.at_obj(2, "lead", &[NodeObjective::SectorComplete("groundZero")]);
            // Explicit different objective stays as-is (parent is a sector, so the
            // sector dependency is prepended before it).
            builder.at_obj(2, "sand", &[NodeObjective::OnSector("frozenForest")]);
            builder.finish()
        };

        let root = registry.tech.tree(tree).unwrap().root;
        let ground_node = registry.tech.node(root).unwrap().children[0];
        let sector_children = registry.tech.node(ground_node).unwrap().children.clone();
        assert_eq!(sector_children.len(), 3);

        let copper = registry.tech.node(sector_children[0]).unwrap();
        assert_eq!(
            copper.objectives,
            vec![ObjectiveSpec::SectorComplete(ground_zero)]
        );

        let lead = registry.tech.node(sector_children[1]).unwrap();
        assert_eq!(
            lead.objectives,
            vec![ObjectiveSpec::SectorComplete(ground_zero)],
            "explicit duplicate not re-added"
        );

        let sand = registry.tech.node(sector_children[2]).unwrap();
        assert_eq!(
            sand.objectives,
            vec![
                ObjectiveSpec::SectorComplete(ground_zero),
                ObjectiveSpec::OnSector(frozen_forest),
            ],
            "sector dependency inserted at position 0"
        );
    }

    /// `tech::requirements_rounding` (plan 02 §5 M2): inherited research cost
    /// multipliers scale explicit requirements (`(int)(amount * mult)`), and the
    /// `round_to_10` helper matches `Mathf.round(v/10)*10`.
    #[test]
    fn requirements_rounding() {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        let mut registry = create_base_content(&bundle, &store, true).unwrap();
        registry.init().unwrap();

        let (tree, _report) = {
            let mut builder = TechTreeBuilder::new(&mut registry, &store);
            builder.root("synthetic", "copper", false);
            builder.set_cost_multipliers(&[("copper", 0.5)]);
            builder.at_req(1, "lead", &[("copper", 100), ("lead", 100)]);
            builder.finish()
        };
        let root = registry.tech.tree(tree).unwrap().root;
        let child = registry.tech.node(root).unwrap().children[0];
        let node = registry.tech.node(child).unwrap();
        let copper = registry.item_id("copper").unwrap();
        let lead = registry.item_id("lead").unwrap();
        assert_eq!(node.requirements.len(), 2);
        assert_eq!(node.requirements[0], ItemStack::new(copper, 50));
        assert_eq!(node.requirements[1], ItemStack::new(lead, 100));
        assert_eq!(node.finished_requirements.len(), 2);

        assert_eq!(round_to_10(4.9), 0);
        assert_eq!(round_to_10(5.0), 10);
        assert_eq!(round_to_10(1234.5), 1230);
    }

    /// `content_research_requirements` dispatches to the block formula and the
    /// unit derivation, and inherits the empty default for other content types.
    #[test]
    fn content_research_requirements_dispatch() {
        let registry = test_support::test_registry();
        let press = registry.block_id("graphite-press").unwrap();
        assert_eq!(
            content_research_requirements(&registry, ContentRef::of(ContentType::Block, press)),
            registry.block(press).unwrap().research_requirements()
        );
        let copper = registry.item_id("copper").unwrap();
        assert!(
            content_research_requirements(&registry, ContentRef::of(ContentType::Item, copper))
                .is_empty(),
            "items have no research cost"
        );
    }

    /// Vanilla trees are ported verbatim; unresolved names are blocks/units that
    /// land in M3/M5. This locks the node counts so later milestones can assert
    /// `missing == 0`.
    #[test]
    fn vanilla_tree_node_counts() {
        let registry = test_support::test_registry();
        let reports = registry.tech_build_reports();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].0, "serpulo");
        assert_eq!(reports[1].0, "erekir");
        // Serpulo: root + 202 node + 20 nodeProduce (223). Erekir: root + 133
        // node + 18 nodeProduce (152). `missing` counts unresolved content names
        // across nodes and objectives, so it exceeds `unresolved_nodes`.
        // M5 resolved the remaining unit names: both trees fully resolve.
        assert_eq!(reports[0].1.resolved_nodes, 223, "M5: all content resolves");
        assert_eq!(reports[0].1.unresolved_nodes, 0);
        assert_eq!(reports[0].1.missing.len(), 0);
        assert_eq!(reports[1].1.resolved_nodes, 152);
        assert_eq!(reports[1].1.unresolved_nodes, 0);
        assert_eq!(reports[1].1.missing.len(), 0);
        assert_eq!(registry.tech().trees.len(), 2);
        let serpulo_tree = registry.tech().trees[0].root;
        assert_eq!(
            registry
                .planet_by_name("serpulo")
                .unwrap()
                .tech_tree
                .map(|tree| registry.tech().tree(tree).unwrap().root),
            Some(serpulo_tree)
        );
    }
}

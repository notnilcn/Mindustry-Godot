// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The research dialog's typed tree payload (`research_tree_json`).
//! Upstream: ui/dialogs/ResearchDialog.java

use serde::{Deserialize, Serialize};

use crate::content::tech::{ObjectiveSpec, TechNode, TechNodeRef, TechStore, TechTree};
use crate::content::{ContentRef, ContentRegistry, PlanetId};
use crate::game::objectives::{CampaignObjectiveContext, Objective, ObjectiveContext};
use crate::game::rules::Rules;
use crate::game::tech_tree;
use crate::game::universe::Campaign;
use crate::ui::text::format_amount_default;
use crate::world::modules::ItemModule;

use super::{CampaignText, content_icon_name};

/// The whole `research_tree_json` payload: the selected tree's roots, the
/// global item pool and one row per node in depth-first order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchTreeView {
    /// Selected tree name (`techtree.<name>`).
    pub root: String,
    /// Localized tree title (`root_title` in the JSON).
    pub root_title: String,
    /// Content type of the tree root (`item`/`block`/…), empty when none.
    pub root_type: String,
    /// Atlas icon name of the tree root, empty when none.
    pub root_icon: String,
    /// Root selector rows (`TechTree.roots` minus locked non-preferred ones).
    pub roots: Vec<ResearchRootRow>,
    /// The global item pool rows (`rebuildItems` + `ItemsDisplay`).
    pub items: Vec<ResearchItemRow>,
    /// Tree bounds over visible nodes (`[minx, miny + nodeSize * 1.5, w, h]`).
    pub bounds: [f32; 4],
    /// Node rows in depth-first pre-order; `index` addresses this array.
    pub nodes: Vec<ResearchNodeRow>,
}

/// One root selector row (`ResearchDialog` root buttons).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchRootRow {
    /// Tree name.
    pub name: String,
    /// Localized title (`techtree.<name>`).
    pub title: String,
    /// Content type name.
    #[serde(rename = "type")]
    pub kind: String,
    /// Atlas icon name.
    pub icon: String,
    /// Currently selected tree.
    pub selected: bool,
}

/// One global item-pool row (`ItemsDisplay`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchItemRow {
    /// Item content name.
    pub item: String,
    /// Pooled amount.
    pub amount: i32,
    /// Formatted amount (`format_amount_default`).
    pub text: String,
    /// Localized item name.
    pub name: String,
}

/// One research node (`View.rebuildAll` button + the hover popup rows).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchNodeRow {
    /// Depth-first array index (the JSON `index`).
    pub index: usize,
    /// Parent array index; `-1` for the tree root.
    pub parent: i64,
    /// Child array indices, in declaration order.
    pub children: Vec<usize>,
    /// Content name of the node.
    pub content: String,
    /// Content type name, empty for a contentless node.
    #[serde(rename = "type")]
    pub kind: String,
    /// Atlas icon name.
    pub icon: String,
    /// Localized content name.
    pub name: String,
    /// Java y-up layout x.
    pub x: f32,
    /// Java y-up layout y.
    pub y: f32,
    /// `checkNodes` visibility flag.
    pub visible: bool,
    /// Locked by the host unlock store or the multiplayer `rules.researched`.
    pub locked: bool,
    /// Unlocked-or-objectives gate (`checkNodes`).
    pub selectable: bool,
    /// `canSpend`: requirements met by the current pool.
    pub can_spend: bool,
    /// Localized description.
    pub description: String,
    /// `UnlockFields.inline_description`: the hover popup shows the description
    /// inline instead of in a dialog.
    pub inline_description: bool,
    /// Progress percentage (`-1` = no bar).
    pub progress: i32,
    /// Unfinished requirement rows.
    pub reqs: Vec<ResearchReqRow>,
    /// Incomplete objective display lines.
    pub objectives: Vec<String>,
}

/// One unfinished research requirement row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchReqRow {
    /// Item content name.
    pub item: String,
    /// Localized item name.
    pub name: String,
    /// Formatted available amount (`format_amount_default`, capped at `need`).
    pub have: String,
    /// Formatted remaining amount.
    pub need: String,
    /// Any amount of the item is pooled.
    pub has: bool,
}

/// Builds the research dialog payload for `root` (empty selects the played
/// sector's tree, else serpulo's). `None` when the tree is missing or empty
/// (the facade returns `"{}"`).
///
/// `played_items` is the live default-team core inventory (`played_core_items`
/// in the facade); the campaign's own stored items come from `campaign`.
pub fn research_tree_view(
    registry: &ContentRegistry,
    campaign: &Campaign,
    text: &dyn CampaignText,
    rules: &Rules,
    sector: Option<(PlanetId, u16)>,
    played_items: &[(String, i32)],
    root: &str,
) -> Option<ResearchTreeView> {
    let tech = registry.tech();
    let tree = select_tree(registry, root, sector)?;
    let ctx = CampaignObjectiveContext::new(registry, campaign);
    let unlocked_with_rules =
        |content: ContentRef| tech_tree::content_unlocked_with_rules(registry, content, rules);

    // Depth-first pre-order from the tree root (`TechNode.each`); indices in
    // this array are the JSON `index`/`parent`/`children` values.
    let (order, indices) = tree_order(tech, tree.root);
    if order.is_empty() {
        return None;
    }
    // `checkNodes` (ResearchDialog.java:391-401): `visible` starts true for
    // every node; a locked parent hides its subtree, an unlocked parent shows
    // its direct children even when they are locked.
    let (visible, selectable) =
        tree_visibility(registry, tech, &order, &indices, &ctx, &unlocked_with_rules);

    // `treeLayout`: positions come from the Java BranchTreeLayout walk.
    let children_lists: Vec<Vec<usize>> = order
        .iter()
        .map(|node_ref| {
            tech.node(*node_ref)
                .map(|node| {
                    node.children
                        .iter()
                        .filter_map(|child| indices.get(&child.0).copied())
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    let positions = crate::ui::tree_layout::research_tree_layout(&children_lists, 0, 60.0);
    // Bounds over visible nodes (ResearchDialog.java:360-371).
    let bounds = tree_bounds(&positions, &visible);

    // The global item pool (`rebuildItems` + `ItemsDisplay`):
    // `tech_tree::research_inventory` over the root's planets.
    let planets = tech_tree::root_planets(registry, tree.root);
    let inv = tech_tree::research_inventory(registry, campaign, &planets, sector, played_items);
    let items = research_item_rows(registry, &inv.pool, text);
    // Root selector rows (`TechTree.roots` minus locked non-preferred ones).
    let roots = research_root_rows(registry, tech, tree.root, text);

    let root_node = tech.node(tree.root);
    let root_name = root_node
        .and_then(|node| node.name.clone())
        .unwrap_or_else(|| tree.name.clone());
    let root_type = root_node
        .and_then(|node| node.content)
        .map(|content| content.type_.name())
        .unwrap_or("");
    let root_icon = root_node
        .map(|node| {
            node.content
                .map(|content| content_icon_name(content.type_.name(), &node.content_name))
                .unwrap_or_default()
        })
        .unwrap_or_default();

    // Node rows (`View.rebuildAll` buttons + the hover popup table).
    let node_ctx = NodeContext {
        registry,
        indices: &indices,
        positions: &positions,
        visible: &visible,
        selectable: &selectable,
        ctx: &ctx,
        items: &inv.pool,
        rules,
        text,
    };
    let nodes = order
        .iter()
        .enumerate()
        .filter_map(|(index, node_ref)| node_row(&node_ctx, index, *node_ref))
        .collect();

    Some(ResearchTreeView {
        root: tree.name.clone(),
        root_title: text.tree_title(&root_name),
        root_type: root_type.to_owned(),
        root_icon,
        roots,
        items,
        bounds,
        nodes,
    })
}

/// The selected `ResearchDialog` tree: `root` names it, empty picks the played
/// sector's tree, else serpulo's.
fn select_tree<'a>(
    registry: &'a ContentRegistry,
    requested: &str,
    sector: Option<(PlanetId, u16)>,
) -> Option<&'a TechTree> {
    let tech = registry.tech();
    if requested.is_empty() {
        sector
            .map(|(planet, _)| planet)
            .or_else(|| registry.planet_id("serpulo"))
            .and_then(|id| registry.planet(id))
            .and_then(|def| def.tech_tree)
            .and_then(|tree_id| tech.tree(tree_id))
    } else {
        tech.trees.iter().find(|tree| tree.name == requested)
    }
}

/// The depth-first pre-order node list (`TechNode.each`) and the
/// node-ref -> array-index map the JSON `index`/`parent`/`children` values use.
fn tree_order(
    tech: &TechStore,
    root: TechNodeRef,
) -> (Vec<TechNodeRef>, std::collections::HashMap<u32, usize>) {
    let mut order: Vec<TechNodeRef> = Vec::new();
    tech.each(root, &mut |node_ref| order.push(node_ref));
    let mut indices: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (index, node_ref) in order.iter().enumerate() {
        indices.insert(node_ref.0, index);
    }
    (order, indices)
}

/// `checkNodes` (ResearchDialog.java:391-401): `visible` starts true for every
/// node; a locked parent hides its subtree, an unlocked parent shows its direct
/// children even when they are locked. `selectable` is the unlocked-or-objectives
/// gate.
fn tree_visibility(
    registry: &ContentRegistry,
    tech: &TechStore,
    order: &[TechNodeRef],
    indices: &std::collections::HashMap<u32, usize>,
    ctx: &CampaignObjectiveContext,
    unlocked_with_rules: &dyn Fn(ContentRef) -> bool,
) -> (Vec<bool>, Vec<bool>) {
    let mut visible = vec![true; order.len()];
    let mut selectable = vec![false; order.len()];
    for (index, node_ref) in order.iter().enumerate() {
        let Some(node) = tech.node(*node_ref) else {
            continue;
        };
        let node_locked = node
            .content
            .is_none_or(|content| !unlocked_with_rules(content));
        let parent_visible = node
            .parent
            .and_then(|parent| indices.get(&parent.0).copied())
            .map(|parent_index| visible[parent_index])
            .unwrap_or(true);
        if !node_locked && (node.parent.is_none() || parent_visible) {
            visible[index] = true;
        }
        // The selectable half reads the host unlock form (not the rules form):
        // `content_unlocked(registry, content)` like the replaced block.
        selectable[index] = node
            .content
            .is_some_and(|content| tech_tree::content_unlocked(registry, content))
            || tech_tree::objectives_complete(registry, &node.objectives, ctx);
        for child in &node.children {
            if let Some(child_index) = indices.get(&child.0).copied() {
                visible[child_index] = !node_locked && visible[index];
            }
        }
    }
    (visible, selectable)
}

/// Bounds over the visible nodes (ResearchDialog.java:360-371): node width goes
/// into x, `+nodeSize * 1.5` into y.
fn tree_bounds(positions: &[(f32, f32)], visible: &[bool]) -> [f32; 4] {
    const NODE_SIZE: f32 = 60.0;
    let (mut minx, mut miny, mut maxx, mut maxy) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for (index, &(x, y)) in positions.iter().enumerate() {
        if !visible.get(index).copied().unwrap_or(false) {
            continue;
        }
        minx = minx.min(x - NODE_SIZE / 2.0);
        maxx = maxx.max(x + NODE_SIZE / 2.0);
        miny = miny.min(y - NODE_SIZE / 2.0);
        maxy = maxy.max(y + NODE_SIZE / 2.0);
    }
    [minx, miny + NODE_SIZE * 1.5, maxx - minx, maxy - miny]
}

/// `ResearchDialog.java:685-705`: the progress percentage once any requirement
/// has partial progress (`min(99)`), else `-1` (no bar).
fn node_progress(
    registry: &ContentRegistry,
    node: &TechNode,
    requirements: &[crate::content::stacks::ItemStack],
) -> i32 {
    let (mut sum, mut used) = (0.0f32, 0.0f32);
    let mut has_progress = false;
    for (requirement_index, requirement) in requirements.iter().enumerate() {
        let finished = node
            .finished_requirements
            .get(requirement_index)
            .map(|stack| stack.amount)
            .unwrap_or(0);
        if finished > 0 {
            has_progress = true;
        }
        let cost = registry
            .item(requirement.item)
            .map(|item| item.cost)
            .unwrap_or(0.0);
        sum += cost * requirement.amount as f32;
        used += cost * finished as f32;
    }
    if has_progress {
        ((used / sum * 100.0) as i32).min(99)
    } else {
        -1
    }
}

/// The per-build research-tree view state: parent indices, layout positions,
/// visibility/selectable flags, the objective context, the item pool the node
/// rows price against, the rules and the facade text.
struct NodeContext<'a> {
    registry: &'a ContentRegistry,
    indices: &'a std::collections::HashMap<u32, usize>,
    positions: &'a [(f32, f32)],
    visible: &'a [bool],
    selectable: &'a [bool],
    ctx: &'a CampaignObjectiveContext,
    items: &'a ItemModule,
    rules: &'a Rules,
    text: &'a dyn CampaignText,
}

/// The global item pool rows (`rebuildItems` + `ItemsDisplay`): every nonzero
/// pool amount in item content order.
fn research_item_rows(
    registry: &ContentRegistry,
    items: &ItemModule,
    text: &dyn CampaignText,
) -> Vec<ResearchItemRow> {
    let mut rows: Vec<ResearchItemRow> = Vec::new();
    for index in 0..registry.items().len() {
        let id = crate::content::ItemId::new(index as u16);
        let amount = items.get(id);
        if amount <= 0 {
            continue;
        }
        let item_name = registry
            .item(id)
            .map(|item| item.name.clone())
            .unwrap_or_default();
        rows.push(ResearchItemRow {
            item: item_name.clone(),
            amount,
            text: format_amount_default(amount as i64),
            name: text.content_name("item", &item_name),
        });
    }
    rows
}

/// Root selector rows (`TechTree.roots` minus locked non-preferred ones).
fn research_root_rows(
    registry: &ContentRegistry,
    tech: &TechStore,
    selected_root: TechNodeRef,
    text: &dyn CampaignText,
) -> Vec<ResearchRootRow> {
    let mut rows: Vec<ResearchRootRow> = Vec::new();
    for root_ref in &tech.roots {
        let Some(node) = tech.node(*root_ref) else {
            continue;
        };
        let selected = *root_ref == selected_root;
        let unlocked = node
            .content
            .is_some_and(|content| tech_tree::content_unlocked(registry, content));
        if node.requires_unlock && !unlocked && !selected {
            continue;
        }
        let name = node.name.clone().unwrap_or_default();
        rows.push(ResearchRootRow {
            title: text.tree_title(&name),
            name,
            kind: node
                .content
                .map(|content| content.type_.name())
                .unwrap_or_default()
                .to_owned(),
            icon: node
                .content
                .map(|content| content_icon_name(content.type_.name(), &node.content_name))
                .unwrap_or_default(),
            selected,
        });
    }
    rows
}

/// One node row of the research tree (`View.rebuildAll` button + the hover
/// popup rows, `ResearchDialog.java:685-749`). `None` for an index without a
/// node (the caller skips it, like the replaced loop).
fn node_row(ctx: &NodeContext<'_>, index: usize, node_ref: TechNodeRef) -> Option<ResearchNodeRow> {
    let registry = ctx.registry;
    let tech = registry.tech();
    let node = tech.node(node_ref)?;
    let content = node.content;
    let type_name = content.map(|content| content.type_.name()).unwrap_or("");
    let icon = content
        .map(|_| content_icon_name(type_name, &node.content_name))
        .unwrap_or_default();
    let name = content
        .map(|_| ctx.text.content_name(type_name, &node.content_name))
        .unwrap_or_default();
    let description = content
        .map(|_| ctx.text.content_description(type_name, &node.content_name))
        .unwrap_or_default();
    let inline_description = content
        .and_then(|content| tech_tree::unlock_fields(registry, content))
        .is_some_and(|fields| fields.inline_description);

    // Progress line (`ResearchDialog.java:685-705`).
    let requirements = tech_tree::effective_requirements(registry, node_ref);
    let progress = node_progress(registry, node, requirements.as_slice());

    // Unfinished requirement rows (`ResearchDialog.java:707-736`).
    let mut reqs: Vec<ResearchReqRow> = Vec::new();
    for (requirement_index, requirement) in requirements.iter().enumerate() {
        let finished = node
            .finished_requirements
            .get(requirement_index)
            .map(|stack| stack.amount)
            .unwrap_or(0);
        if requirement.amount <= finished {
            continue;
        }
        let remaining = requirement.amount - finished;
        let pool = ctx.items.get(requirement.item);
        let item_name = registry
            .item(requirement.item)
            .map(|item| item.name.clone())
            .unwrap_or_default();
        reqs.push(ResearchReqRow {
            name: ctx.text.content_name("item", &item_name),
            item: item_name,
            have: format_amount_default(pool.min(remaining) as i64),
            need: format_amount_default(remaining as i64),
            has: pool > 0,
        });
    }

    // Incomplete objectives (`ResearchDialog.java:737-749`).
    let mut objectives: Vec<String> = Vec::new();
    for spec in &node.objectives {
        let (complete, text) = objective_display(registry, ctx.ctx, ctx.text, *spec);
        if !complete {
            objectives.push(text);
        }
    }

    Some(ResearchNodeRow {
        index,
        parent: node
            .parent
            .and_then(|parent| ctx.indices.get(&parent.0).copied())
            .map(|parent_index| parent_index as i64)
            .unwrap_or(-1),
        children: node
            .children
            .iter()
            .filter_map(|child| ctx.indices.get(&child.0).copied())
            .collect(),
        content: node.content_name.clone(),
        kind: type_name.to_owned(),
        icon,
        name,
        x: ctx.positions[index].0,
        y: ctx.positions[index].1,
        visible: ctx.visible[index],
        locked: content.is_none_or(|content| {
            !tech_tree::content_unlocked_with_rules(registry, content, ctx.rules)
        }),
        selectable: ctx.selectable[index],
        can_spend: tech_tree::can_spend(registry, node_ref, ctx.items, ctx.ctx),
        description,
        inline_description,
        progress,
        reqs,
        objectives,
    })
}

/// `Objective.display()` and `complete()` for the hover popup
/// (Objectives.java:26-131).
fn objective_display(
    registry: &ContentRegistry,
    ctx: &dyn ObjectiveContext,
    text: &dyn CampaignText,
    spec: ObjectiveSpec,
) -> (bool, String) {
    let complete = Objective::from_spec(spec).complete(ctx);
    let (key, argument) = match spec {
        ObjectiveSpec::SectorComplete(sector) => (
            "requirement.capture",
            text.sector_localized_name(registry, sector),
        ),
        ObjectiveSpec::Research(content) => {
            let value = if tech_tree::research_parent_unlocked(registry, content) {
                text.emoji_content_name(registry, content)
            } else {
                String::from("???")
            };
            ("requirement.research", value)
        }
        ObjectiveSpec::Produce(content) => {
            let value = if tech_tree::content_unlocked(registry, content) {
                text.emoji_content_name(registry, content)
            } else {
                String::from("???")
            };
            ("requirement.produce", value)
        }
        ObjectiveSpec::OnSector(sector) => (
            "requirement.onsector",
            text.sector_localized_name(registry, sector),
        ),
        ObjectiveSpec::OnPlanet(planet) => (
            "requirement.onplanet",
            text.planet_localized_name(registry, planet),
        ),
    };
    (complete, text.format(key, &[argument.as_str()]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::test_support::raw_registry;

    fn sample_view() -> ResearchTreeView {
        ResearchTreeView {
            root: "serpulo".to_owned(),
            root_title: "Serpulo".to_owned(),
            root_type: "block".to_owned(),
            root_icon: "block-core-shard-ui".to_owned(),
            roots: vec![ResearchRootRow {
                name: "serpulo".to_owned(),
                title: "Serpulo".to_owned(),
                kind: "block".to_owned(),
                icon: "block-core-shard-ui".to_owned(),
                selected: true,
            }],
            items: vec![ResearchItemRow {
                item: "copper".to_owned(),
                amount: 900,
                text: "900".to_owned(),
                name: "Copper".to_owned(),
            }],
            bounds: [-30.0, 60.0, 60.0, 60.0],
            nodes: vec![ResearchNodeRow {
                index: 0,
                parent: -1,
                children: vec![1],
                content: "core-shard".to_owned(),
                kind: "block".to_owned(),
                icon: "block-core-shard-ui".to_owned(),
                name: "Core: Shard".to_owned(),
                x: 0.0,
                y: 90.0,
                visible: true,
                locked: false,
                selectable: true,
                can_spend: false,
                description: "The core".to_owned(),
                inline_description: true,
                progress: -1,
                reqs: vec![ResearchReqRow {
                    item: "copper".to_owned(),
                    name: "Copper".to_owned(),
                    have: "900".to_owned(),
                    need: "42[gray]k[]".to_owned(),
                    has: true,
                }],
                objectives: vec!["Capture Ground Zero".to_owned()],
            }],
        }
    }

    #[test]
    fn research_tree_view_round_trips_serde_in_field_order() {
        let view = sample_view();
        let text = serde_json::to_string(&view).expect("serialize");
        assert!(text.starts_with("{\"root\":\"serpulo\",\"root_title\":\"Serpulo\""));
        assert!(text.contains("\"type\":\"block\""));
        let back: ResearchTreeView = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back, view);
        assert_eq!(serde_json::to_string(&back).expect("reserialize"), text);
    }

    #[test]
    fn tree_bounds_cover_only_visible_nodes() {
        let positions = [(0.0f32, 0.0f32), (60.0, 60.0)];
        assert_eq!(
            tree_bounds(&positions, &[true, false]),
            [-30.0, 60.0, 60.0, 60.0]
        );
        assert_eq!(
            tree_bounds(&positions, &[true, true]),
            [-30.0, -30.0 + 90.0, 120.0, 120.0]
        );
    }

    #[test]
    fn campaign_text_defaults_fall_back_to_the_content_name() {
        struct EmptyText;
        impl CampaignText for EmptyText {
            fn bundle(&self, _key: &str) -> Option<String> {
                None
            }
            fn format(&self, key: &str, _args: &[&str]) -> String {
                key.to_owned()
            }
            fn content_name(&self, _kind: &str, name: &str) -> String {
                name.to_owned()
            }
            fn content_description(&self, _kind: &str, _name: &str) -> String {
                String::new()
            }
            fn content_emoji(&self, _name: &str) -> String {
                String::new()
            }
        }
        let text = EmptyText;
        assert_eq!(text.tree_title("serpulo"), "serpulo");
        let registry = raw_registry();
        let preset = registry.sectors().first().expect("a preset");
        assert_eq!(
            text.sector_localized_name(&registry, preset.id),
            preset.name
        );
        assert_eq!(text.content_name("item", "copper"), "copper");
        assert_eq!(text.content_emoji("copper"), "");
        assert_eq!(
            text.format("requirement.capture", &["Ground Zero"]),
            "requirement.capture"
        );
    }
}

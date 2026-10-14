// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-block placement-line rules resolved from the content registry:
//! `Block.getReplacement` (`InputHandler.java:1952-1958`), the junction/bridge
//! half of `Block.handlePlacementLine` (Conveyor.java:94-99, Conduit.java:140-148,
//! Duct.java:123-129) and `Block.changePlacementPath` (ItemBridge.java:206-208,
//! DirectionBridge.java:90-92, PowerNode.java:179-181, BeamNode.java:108-112,
//! LightBlock.java:55-58).
//!
//! `update_line_with` consumes [`LineRules`]; [`NodeHooks`] drives the
//! `changePlacementPath` spacing inside `iterateLine`. The Java per-instance
//! fields (`junctionReplacement`, `bridgeReplacement`, `range`, `laserRange`,
//! `radius`) have no `BlockDef` counterpart yet, so the vanilla values are
//! resolved by name with kind defaults (the behaviors use the same constants;
//! `world/blocks/distribution/item_bridge.rs:118/128`,
//! `world/blocks/power/behavior.rs:1360-1366`).

use smallvec::SmallVec;

use crate::config::TILESIZE;
use crate::content::{BlockId, BlockKind, ContentRegistry};
use crate::world::TilePos;

use super::line::LineHooks;
use super::placement::{D4, PlacementWorld, calculate_nodes};

/// `Block.changePlacementPath` node spacing rule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeRule {
    /// Chebyshev tile distance
    /// (`Math.max(|dx|, |dy|) <= range`; ItemBridge/DirectionBridge).
    Chebyshev(i32),
    /// `PowerNode.overlaps`: circle at the tile centre (+`offset`) with
    /// `laserRange` tiles against the other block's `size*8` square.
    PowerLaser {
        /// `laserRange` in tiles.
        range_tiles: f32,
        /// Block size in tiles.
        size: i32,
        /// `Block.offset` (even-size blocks shift +4).
        offset: f32,
    },
    /// `BeamNode` (`Math.max(|dx|, |dy|) <= range + size - 1`, desktop only).
    Beam {
        /// `BeamNode.range` in tiles.
        range: i32,
        /// Block size in tiles.
        size: i32,
    },
    /// `LightBlock`: `point.dst2(other) <= pow(radius * 0.7f / tilesize, 2) * 3`.
    Light {
        /// `LightBlock.radius` in world pixels.
        radius: f32,
    },
}

/// Placement-line rules for the selected block.
#[derive(Debug, Clone)]
pub struct LineRules {
    /// `changePlacementPath` spacing (runs regardless of `blockreplace`).
    pub node: Option<NodeRule>,
    /// `getReplacement` junction block (`None` = never replace).
    pub junction: Option<BlockId>,
    /// Kinds whose plans satisfy `req.block instanceof Conveyor/Junction`.
    pub junction_plan_kinds: &'static [BlockKind],
    /// `handlePlacementLine` bridge `(block, range, direction bridge)`.
    pub bridge: Option<(BlockId, i32, bool)>,
    /// `avoid` kinds: `handlePlacementLine` family recognition and the
    /// `needJunction`/rotation-mismatch tests in `calculateBridges`.
    pub avoid_kinds: &'static [BlockKind],
    /// `ItemBridge.handlePlacementLine` link-config range for bridge blocks
    /// selected directly.
    pub link_bridge_range: Option<i32>,
}

impl LineRules {
    /// No line rules (non-line blocks and unknown content).
    pub const NONE: LineRules = LineRules {
        node: None,
        junction: None,
        junction_plan_kinds: &[],
        bridge: None,
        avoid_kinds: &[],
        link_bridge_range: None,
    };

    /// Whether the rules change anything beyond per-point rotations.
    pub fn is_empty(&self) -> bool {
        self.node.is_none()
            && self.junction.is_none()
            && self.bridge.is_none()
            && self.link_bridge_range.is_none()
    }
}

/// `ItemBridge.range` / `LiquidBridge` (`bridge-conduit` 4, `phase-conduit`
/// 12; `Blocks.java:2375/2387`; the port registers the same values,
/// `world/blocks/liquid/behavior.rs:500-503`).
fn liquid_bridge_range(name: &str) -> i32 {
    match name {
        "phase-conduit" => 12,
        _ => 4,
    }
}

/// `BufferedItemBridge`/`ItemBridge` range (`bridge-conveyor` 4,
/// `phase-conveyor` 12; `world/blocks/distribution/item_bridge.rs:118/128`).
fn item_bridge_range(name: &str) -> i32 {
    match name {
        "phase-conveyor" => 12,
        _ => 4,
    }
}

/// `PowerNode.laserRange` per block (`Blocks.java`: power-node 6 default,
/// large 15, surge-tower 40, beam-link 500; `power/behavior.rs:1360-1366`).
fn power_range(name: &str) -> f32 {
    match name {
        "power-node-large" => 15.0,
        "surge-tower" => 40.0,
        "beam-link" => 500.0,
        _ => 6.0,
    }
}

/// `BeamNode.range` per block (`Blocks.java:2671/2685`: beam-node 10,
/// beam-tower 23; class default 5).
pub(crate) fn beam_range(name: &str) -> i32 {
    match name {
        "beam-tower" => 23,
        _ => 10,
    }
}

/// `LightBlock.radius` (`illuminator` 140f, `Blocks.java:6816`).
fn light_radius(name: &str) -> f32 {
    match name {
        "illuminator" => 140.0,
        _ => 60.0,
    }
}

/// Resolves the placement-line rules for a block (`Block` instance fields in
/// Java; see the module docs for the per-name values).
pub fn line_rules_for(content: &ContentRegistry, block: BlockId) -> LineRules {
    use BlockKind as K;
    let Some(def) = content.block(block) else {
        return LineRules::NONE;
    };
    // Unlock state is not gated here: `replace.unlockedNow()`/`bridge.unlockedNow()`
    // have no clean mind-core seam (the live content build starts locked in
    // tests, and the build-time gate `placement_allowed` ->
    // `queue_player_place_config` still rejects locked placements). Accepted
    // deviation: a locked junction/bridge may show in the preview.
    let junction = |name: &str| -> Option<BlockId> { content.block_id(name) };
    let bridge = |name: &str, range: i32, direction: bool| -> Option<(BlockId, i32, bool)> {
        content.block_id(name).map(|id| (id, range, direction))
    };
    match def.kind {
        K::Conveyor | K::ArmoredConveyor => LineRules {
            // `Conveyor.init` (`Conveyor.java:67-68`): junction -> `junction`,
            // bridge -> `itemBridge` (bridge-conveyor).
            junction: junction("junction"),
            junction_plan_kinds: &[K::Conveyor, K::ArmoredConveyor, K::Junction],
            bridge: bridge("bridge-conveyor", 4, false),
            avoid_kinds: &[K::Conveyor, K::ArmoredConveyor],
            ..LineRules::NONE
        },
        K::Duct | K::OverflowDuct => LineRules {
            // `Duct.init` (`Duct.java:66-71`): the `ductJunction` default is
            // commented out (`junctionReplacement == null` -> `getReplacement`
            // returns `this`); bridge -> `ductBridge` (range 4).
            bridge: bridge("duct-bridge", 4, true),
            avoid_kinds: &[
                K::Duct,
                K::OverflowDuct,
                K::StackConveyor,
                K::Conveyor,
                K::ArmoredConveyor,
            ],
            ..LineRules::NONE
        },
        K::Conduit | K::ArmoredConduit => LineRules {
            // `Conduit.init` (`Conduit.java:64-66`): junction -> `liquidJunction`,
            // bridge -> `bridgeConduit`; `handlePlacementLine` passes
            // `hasJunction = true` (`Conduit.java:144-147`).
            junction: junction("liquid-junction"),
            junction_plan_kinds: &[K::Conduit, K::ArmoredConduit, K::LiquidJunction],
            bridge: bridge("bridge-conduit", 4, false),
            avoid_kinds: &[K::Conduit, K::ArmoredConduit],
            ..LineRules::NONE
        },
        K::ItemBridge | K::BufferedItemBridge => {
            let range = item_bridge_range(def.name.as_str());
            LineRules {
                node: Some(NodeRule::Chebyshev(range)),
                link_bridge_range: Some(range),
                ..LineRules::NONE
            }
        }
        K::LiquidBridge | K::DirectionLiquidBridge => {
            let range = liquid_bridge_range(def.name.as_str());
            LineRules {
                node: Some(NodeRule::Chebyshev(range)),
                link_bridge_range: Some(range),
                ..LineRules::NONE
            }
        }
        K::DuctBridge => LineRules {
            // `DuctBridge extends DirectionBridge` (default range 4).
            node: Some(NodeRule::Chebyshev(4)),
            link_bridge_range: Some(4),
            ..LineRules::NONE
        },
        K::PowerNode | K::LongPowerNode => LineRules {
            node: Some(NodeRule::PowerLaser {
                range_tiles: power_range(def.name.as_str()),
                size: def.size,
                offset: def.offset,
            }),
            ..LineRules::NONE
        },
        K::BeamNode => LineRules {
            node: Some(NodeRule::Beam {
                range: beam_range(def.name.as_str()),
                size: def.size,
            }),
            ..LineRules::NONE
        },
        K::LightBlock => LineRules {
            node: Some(NodeRule::Light {
                radius: light_radius(def.name.as_str()),
            }),
            ..LineRules::NONE
        },
        _ => LineRules::NONE,
    }
}

/// `LineHooks` applying the active [`NodeRule`] to the drag's point list.
pub struct NodeHooks<'a> {
    /// Active spacing rule.
    pub rule: NodeRule,
    /// World view for the `validPlace` filter inside `calculateNodes`.
    pub world: &'a dyn PlacementWorld,
    /// Selected block id.
    pub block: BlockId,
}

impl LineHooks for NodeHooks<'_> {
    fn change_placement_path(
        &self,
        points: &mut SmallVec<[TilePos; 128]>,
        rotation: u8,
        diagonal: bool,
    ) {
        let tile = TILESIZE as f32;
        let team = self.world.team();
        match self.rule {
            NodeRule::Chebyshev(range) => {
                calculate_nodes(
                    self.world,
                    team,
                    points,
                    self.block,
                    rotation,
                    |point, other| {
                        (point.x() as i32 - other.x() as i32)
                            .abs()
                            .max((point.y() as i32 - other.y() as i32).abs())
                            <= range
                    },
                );
            }
            NodeRule::Beam { range, size } => {
                // `BeamNode.changePlacementPath` skips the rule while the
                // diagonal key is held (`BeamNode.java:108-112`).
                if !diagonal {
                    calculate_nodes(
                        self.world,
                        team,
                        points,
                        self.block,
                        rotation,
                        |point, other| {
                            (point.x() as i32 - other.x() as i32)
                                .abs()
                                .max((point.y() as i32 - other.y() as i32).abs())
                                < range + size
                        },
                    );
                }
            }
            NodeRule::PowerLaser {
                range_tiles,
                size,
                offset,
            } => {
                calculate_nodes(
                    self.world,
                    team,
                    points,
                    self.block,
                    rotation,
                    |point, other| {
                        let cx = point.x() as f32 * tile + offset;
                        let cy = point.y() as f32 * tile + offset;
                        let radius = range_tiles * tile;
                        let ox = other.x() as f32 * tile + offset;
                        let oy = other.y() as f32 * tile + offset;
                        let side = size as f32 * tile;
                        crate::world::blocks::power::nodes::circle_rect_overlap(
                            cx,
                            cy,
                            radius,
                            [ox - side / 2.0, oy - side / 2.0, side, side],
                        )
                    },
                );
            }
            NodeRule::Light { radius } => {
                // `LightBlock.changePlacementPath` (`LightBlock.java:55-58`):
                // squared tile distance against `pow(radius * 0.7 / 8, 2) * 3`.
                let place_radius2 = (radius * 0.7 / tile).powi(2) * 3.0;
                calculate_nodes(
                    self.world,
                    team,
                    points,
                    self.block,
                    rotation,
                    |point, other| {
                        let dx = (point.x() - other.x()) as f32;
                        let dy = (point.y() - other.y()) as f32;
                        dx * dx + dy * dy <= place_radius2
                    },
                );
            }
        }
    }
}

/// `Tile.relativeTo` step for the junction front/back test.
fn step(x: i32, y: i32, rotation: i32) -> (i32, i32) {
    let (dx, dy) = D4[rotation.rem_euclid(4) as usize];
    (x + dx, y + dy)
}

/// `Conveyor.getReplacement` / `Conduit.getReplacement` (Conveyor.java:112-121,
/// Conduit.java:123-132): replace the plan with the junction block when the
/// line runs straight through an existing perpendicular member of the family.
pub fn junction_replaces(
    world: &dyn PlacementWorld,
    plans: &[super::plan::PlanCopy],
    index: usize,
    rules: &LineRules,
) -> bool {
    let Some(req) = plans.get(index).cloned() else {
        return false;
    };
    // `req.block instanceof Conveyor || req.block instanceof Junction`.
    if !world
        .block_kind(req.block)
        .is_some_and(|kind| rules.junction_plan_kinds.contains(&kind))
    {
        return false;
    }
    let rotation = req.rotation as i32;
    let front = step(req.x, req.y, rotation);
    let back = step(req.x, req.y, rotation - 2);
    let has_plan = |(x, y): (i32, i32)| {
        plans.iter().any(|plan| {
            plan.x == x
                && plan.y == y
                && world
                    .block_kind(plan.block)
                    .is_some_and(|kind| rules.junction_plan_kinds.contains(&kind))
        })
    };
    if !has_plan(front) || !has_plan(back) {
        return false;
    }
    // `req.tile().block() instanceof Conveyor` (junctions excluded).
    let tile_kind = world.block_kind(world.block_at(req.x, req.y));
    if !tile_kind.is_some_and(|kind| rules.avoid_kinds.contains(&kind)) {
        return false;
    }
    // `Mathf.mod(req.tile().build.rotation - req.rotation, 2) == 1`.
    world
        .build_rotation(req.x, req.y)
        .is_some_and(|rotation| (rotation as i32 - req.rotation as i32).rem_euclid(2) == 1)
}

/// `ItemBridge.positionsValid` / `DirectionBridge.positionsValid`: same row or
/// column within `range` (ItemBridge.java, DirectionBridge.java).
pub fn bridge_positions_valid(x1: i32, y1: i32, x2: i32, y2: i32, range: i32) -> bool {
    if x1 == x2 {
        (y1 - y2).abs() <= range
    } else if y1 == y2 {
        (x1 - x2).abs() <= range
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::input::client_input::InputState;
    use crate::input::line::{LineParams, NoLineHooks};
    use crate::world::config::ConfigValue;

    /// Grid-backed [`PlacementWorld`] with live content and existing buildings.
    struct RulesWorld {
        content: ContentRegistry,
        width: i32,
        height: i32,
        blocks: Vec<BlockId>,
        rotations: std::collections::HashMap<i32, u8>,
    }

    impl RulesWorld {
        fn new(content: ContentRegistry, width: i32, height: i32) -> Self {
            Self {
                content,
                width,
                height,
                blocks: vec![BlockId::AIR; (width * height) as usize],
                rotations: std::collections::HashMap::new(),
            }
        }

        fn set(&mut self, x: i32, y: i32, block: BlockId, rotation: u8) {
            let index = (y * self.width + x) as usize;
            self.blocks[index] = block;
            if block != BlockId::AIR {
                self.rotations
                    .insert(TilePos::new(x as i16, y as i16).pack(), rotation);
            }
        }
    }

    impl PlacementWorld for RulesWorld {
        fn in_bounds(&self, x: i32, y: i32) -> bool {
            x >= 0 && y >= 0 && x < self.width && y < self.height
        }

        fn block_at(&self, x: i32, y: i32) -> BlockId {
            if self.in_bounds(x, y) {
                self.blocks[(y * self.width + x) as usize]
            } else {
                BlockId::AIR
            }
        }

        fn floor_deep(&self, _x: i32, _y: i32) -> bool {
            false
        }

        fn always_replace(&self, x: i32, y: i32) -> bool {
            self.block_at(x, y) == BlockId::AIR
        }

        fn can_replace(&self, _target: BlockId, other: BlockId) -> bool {
            other == BlockId::AIR
        }

        /// Free air is placeable; anything else is not (walls block lines).
        fn valid_place(&self, _block: BlockId, x: i32, y: i32, _rotation: u8) -> bool {
            self.block_at(x, y) == BlockId::AIR
        }

        fn build_rotation(&self, x: i32, y: i32) -> Option<u8> {
            self.rotations
                .get(&TilePos::new(x as i16, y as i16).pack())
                .copied()
        }

        fn team(&self) -> u8 {
            1
        }

        fn block_kind(&self, block: BlockId) -> Option<BlockKind> {
            self.content.block(block).map(|def| def.kind)
        }
    }

    fn test_content() -> ContentRegistry {
        create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).expect("content")
    }

    /// Runs `update_line_with` with the block's rules (or none).
    fn drag(
        world: &RulesWorld,
        state: &mut InputState,
        block: BlockId,
        from: (i32, i32),
        to: (i32, i32),
        apply_rules: bool,
    ) {
        state.select_block(Some(block));
        let meta = world
            .content
            .block(block)
            .map(|def| crate::input::line::line_block_from_def(block, def));
        let rules = line_rules_for(&world.content, block);
        let params = LineParams::default();
        let start = TilePos::new(from.0 as i16, from.1 as i16);
        let end = TilePos::new(to.0 as i16, to.1 as i16);
        let active = apply_rules.then_some(&rules);
        match rules.node {
            Some(rule) => {
                let hooks = NodeHooks { rule, world, block };
                state.update_line_with(world, meta.as_ref(), start, end, &params, &hooks, active);
            }
            None => {
                state.update_line_with(
                    world,
                    meta.as_ref(),
                    start,
                    end,
                    &params,
                    &NoLineHooks,
                    active,
                );
            }
        }
    }

    fn plan_line(state: &InputState) -> Vec<(i32, i32, BlockId)> {
        state
            .line_plans
            .iter()
            .map(|plan| (plan.x, plan.y, plan.block))
            .collect()
    }

    #[test]
    fn conveyor_line_crossing_conveyor_becomes_junction() {
        // `Conveyor.getReplacement` (`Conveyor.java:112-121`): a straight plan
        // line crossing an existing perpendicular conveyor switches that plan
        // to `junction`.
        let content = test_content();
        let conveyor = content.block_id("conveyor").expect("conveyor");
        let junction = content.block_id("junction").expect("junction");
        let mut world = RulesWorld::new(content, 16, 16);
        world.set(5, 5, conveyor, 1);
        let mut state = InputState::new();
        drag(&world, &mut state, conveyor, (3, 5), (7, 5), true);
        let line = plan_line(&state);
        assert_eq!(line.len(), 5);
        assert_eq!(line[2], (5, 5, junction), "crossing becomes a junction");
        for (index, plan) in line.iter().enumerate() {
            if index != 2 {
                assert_eq!(plan.2, conveyor, "plan {index} stays conveyor");
            }
        }
    }

    #[test]
    fn blockreplace_off_keeps_plain_line() {
        // `InputHandler.updateLine` gates the replacement half on
        // `settings.blockreplace` (`IH:1952`).
        let content = test_content();
        let conveyor = content.block_id("conveyor").expect("conveyor");
        let mut world = RulesWorld::new(content, 16, 16);
        world.set(5, 5, conveyor, 1);
        let mut state = InputState::new();
        drag(&world, &mut state, conveyor, (3, 5), (7, 5), false);
        let line = plan_line(&state);
        assert_eq!(line.len(), 5);
        assert!(
            line.iter().all(|plan| plan.2 == conveyor),
            "no junction without blockreplace: {line:?}"
        );
    }

    #[test]
    fn conveyor_line_over_wall_bridges() {
        // `Conveyor.handlePlacementLine` -> `Placement.calculateBridges`
        // (`Conveyor.java:94-99`, `Placement.java:195-...`). Derivation for a
        // 6-plan line x=3..8 with a wall at x=5 (costs conveyor 3, junction 30,
        // bridge 200, empty penalty 5):
        //   dp[4]  = 3                    (x=4 conveyor)
        //   dp[5]  = INF (wall, no junction replacement -> skipped)
        //   dp[6]  = dp[4] + 200 = 203    (bridge x=4 -> x=6, span 2 <= 4)
        //   dp[7]  = 206, dp[8] = 209
        // Backtrack takes the x=4/x=6 bridge pair and drops the wall plan.
        let content = test_content();
        let conveyor = content.block_id("conveyor").expect("conveyor");
        let wall = content.block_id("copper-wall").expect("copper-wall");
        let bridge = content
            .block_id("bridge-conveyor")
            .expect("bridge-conveyor");
        let mut world = RulesWorld::new(content, 16, 16);
        world.set(5, 5, wall, 0);
        let mut state = InputState::new();
        drag(&world, &mut state, conveyor, (3, 5), (8, 5), true);
        let line = plan_line(&state);
        assert_eq!(
            line,
            vec![
                (3, 5, conveyor),
                (4, 5, bridge),
                (6, 5, bridge),
                (7, 5, conveyor),
                (8, 5, conveyor),
            ]
        );
        // The anchor endpoint config points at the far bridge (x=6 - x=4).
        assert_eq!(
            state.line_plans[1].config,
            ConfigValue::Point2(2, 0),
            "Point2 config on the first bridge endpoint"
        );
    }

    #[test]
    fn power_node_line_spacing() {
        // `PowerNode.changePlacementPath` (`PowerNode.java:179-181`): the
        // 1x1 node radius 6*8 circle vs the next node's 8x8 square overlaps at
        // 6 tiles (centre distance 48 <= 48+4) but not 7 (52 > 48+4).
        let content = test_content();
        let node = content.block_id("power-node").expect("power-node");
        let world = RulesWorld::new(content, 32, 4);
        let mut state = InputState::new();
        drag(&world, &mut state, node, (0, 0), (20, 0), true);
        let xs: Vec<i32> = state.line_plans.iter().map(|plan| plan.x).collect();
        assert_eq!(xs, vec![0, 6, 12, 18, 20]);
    }

    #[test]
    fn bridge_line_spacing() {
        // `ItemBridge.changePlacementPath` + `ItemBridge.handlePlacementLine`
        // (`ItemBridge.java:195-208`): Chebyshev(4) spacing then consecutive
        // Point2 link configs.
        let content = test_content();
        let bridge = content
            .block_id("bridge-conveyor")
            .expect("bridge-conveyor");
        let world = RulesWorld::new(content, 16, 4);
        let mut state = InputState::new();
        drag(&world, &mut state, bridge, (0, 0), (10, 0), true);
        let xs: Vec<i32> = state.line_plans.iter().map(|plan| plan.x).collect();
        assert_eq!(xs, vec![0, 4, 8, 10]);
        let configs: Vec<ConfigValue> = state
            .line_plans
            .iter()
            .map(|plan| plan.config.clone())
            .collect();
        assert_eq!(
            configs,
            vec![
                ConfigValue::Point2(4, 0),
                ConfigValue::Point2(4, 0),
                ConfigValue::Point2(2, 0),
                ConfigValue::None,
            ]
        );
    }

    #[test]
    fn junction_front_back_required() {
        // Short drags whose plan line does not pass through the crossing
        // (front or back missing) are not replaced (`Conveyor.java:118-119`).
        let content = test_content();
        let conveyor = content.block_id("conveyor").expect("conveyor");
        let junction = content.block_id("junction").expect("junction");
        let mut world = RulesWorld::new(content, 16, 16);
        world.set(5, 5, conveyor, 1);
        let mut state = InputState::new();
        // Drag ends at the crossing: no plan east of (5,5).
        drag(&world, &mut state, conveyor, (3, 5), (5, 5), true);
        assert!(state.line_plans.iter().all(|plan| plan.block != junction));
    }
}

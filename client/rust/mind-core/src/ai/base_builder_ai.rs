// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BaseBuilderAI` — enemy/base schematic placement (plan 11 M5).
//!
//! Ported from `core/src/mindustry/ai/BaseBuilderAI.java`. The port keeps the
//! exact timer state machine, the `pathStep = 50` enemy-core path trace over
//! `pathfinder.get_field(team, cost_ground)`, the 6-attempt `randomPosition`
//! placement, `try_place`'s `Build.valid_place` / payload-proximity / AI-path /
//! drill-resource checks, and the `BlockPlan` queueing.
//!
//! Two upstream side effects that need plan-05/07/12 services are surfaced as
//! [`BaseBuildActions`] rather than performed here: core item filling and core
//! unit spawning (`data.core().block.unitType` is plan-07 content metadata that
//! the current `BlockDef` does not carry — see the plan-11 Changelog).
//!
//! Determinism: upstream uses the global `Mathf.random`; this port takes a
//! seeded [`ArcRand`] so a scenario replays byte-for-byte (plan 11 §2.4).

use std::collections::{BTreeSet, VecDeque};

use crate::content::ContentRegistry;
use crate::content::registries::blocks::BlockKind;
use crate::game::rules::{Rules, TeamRule};
use crate::game::teams::BlockPlan;
use crate::math::ArcRand;
use crate::world::{BlockCounter, BlockTable, BuildRules, TilePos, WorldGrid};

use super::base_registry::{BasePart, BaseRegistry, BaseResource};
use super::pathfinder::Flowfield;

/// Placement attempts per step (`BaseBuilderAI.attempts`).
pub const ATTEMPTS: usize = 6;
/// Core unit multiplier (`coreUnitMultiplier`).
pub const CORE_UNIT_MULTIPLIER: i32 = 2;
/// Random empty-part chance (`emptyChance`).
pub const EMPTY_CHANCE: f64 = 0.01;
/// Placement interval lower bound in ticks (`placeIntervalMin` seconds * 60).
pub const PLACE_INTERVAL_MIN: f32 = 12.0 * 60.0;
/// Placement interval upper bound in ticks (`placeIntervalMax` seconds * 60).
pub const PLACE_INTERVAL_MAX: f32 = 2.0 * 60.0;
/// Path trace steps per update (`pathStep`).
pub const PATH_STEP: usize = 50;
/// Placement scatter radius in world pixels (`range`).
pub const PLACE_RANGE: f32 = 150.0;

const TIMER_STEP: usize = 0;
const TIMER_SPAWN: usize = 1;
const TIMER_REFRESH_PATH: usize = 2;

const D4: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const D8: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// A fixed-slot tick interval mirroring Arc `Interval` semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct AiInterval {
    timers: [f32; 4],
}

impl Default for AiInterval {
    fn default() -> Self {
        Self { timers: [0.0; 4] }
    }
}

impl AiInterval {
    /// Creates a zeroed interval.
    pub const fn new() -> Self {
        Self { timers: [0.0; 4] }
    }

    /// `Interval.reset(id, value)`.
    pub fn reset(&mut self, id: usize, value: f32) {
        self.timers[id.min(3)] = value;
    }

    /// `Interval.get(id, time)` in ticks: true once the slot reaches `time`.
    pub fn get(&mut self, id: usize, time: f32) -> bool {
        let slot = &mut self.timers[id.min(3)];
        *slot += 1.0;
        if *slot >= time {
            *slot = 0.0;
            true
        } else {
            false
        }
    }
}

/// Side effects the caller must apply (`BaseBuilderAI.update` host work).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BaseBuildActions {
    /// Fill every core with items (`core.items.set(item, maximumAccepted)`).
    pub fill_core_items: bool,
    /// Spawn one core unit at a random core (upstream `block.unitType.create`).
    pub spawn_core_unit: bool,
    /// A `BlockPlan` was queued this update.
    pub queued_plan: bool,
}

/// Read-only context + mutable plan queue for one `BaseBuilderAI.update`.
pub struct BaseBuildInput<'a> {
    /// Content registry.
    pub content: &'a ContentRegistry,
    /// Tile grid.
    pub grid: &'a WorldGrid,
    /// Block table (`Build.validPlace`).
    pub table: &'a BlockTable,
    /// Build rules (`Build.validPlace`).
    pub build_rules: &'a BuildRules,
    /// Live block counter (`Build.validPlace`).
    pub counter: &'a BlockCounter,
    /// This team's effective rules (`buildAiTier`, `aiCoreSpawn`).
    pub team_rule: &'a TeamRule,
    /// Match rules (wave-team test).
    pub rules: &'a Rules,
    /// Owning team id.
    pub team: u8,
    /// Core tile positions.
    pub cores: &'a [TilePos],
    /// Spawn tile positions.
    pub spawns: &'a [TilePos],
    /// Enemy-core flowfield (`fieldCore`), if computed.
    pub enemy_core_field: Option<&'a Flowfield>,
    /// Enemy core tile positions (path end test).
    pub enemy_cores: &'a [TilePos],
    /// Existing core-unit count (`data.countType(core.unitType)`).
    pub core_unit_count: i32,
    /// Loaded base-part catalogue (`Vars.bases`).
    pub bases: &'a BaseRegistry,
    /// Seeded RNG (replaces `Mathf.random`).
    pub rng: &'a mut ArcRand,
}

/// `BaseBuilderAI` per-`TeamData` state.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseBuilderAi {
    /// Timers.
    pub interval: AiInterval,
    /// Whether an enemy path is available (`foundPath`).
    pub found_path: bool,
    /// Flushed AI path (packed positions; blocks placement).
    pub path: BTreeSet<i32>,
    /// In-progress path (`calcPath`).
    pub calc_path: BTreeSet<i32>,
    /// Current trace tile (`calcTile`).
    pub calc_tile: Option<TilePos>,
    /// Whether the path trace is running.
    pub calculating: bool,
    /// Whether a trace has ever started.
    pub started_calculating: bool,
    /// Trace steps used this calculation (`calcCount`).
    pub calc_count: usize,
    /// Total completed traces (`totalCalcs`).
    pub total_calcs: u64,
}

impl Default for BaseBuilderAi {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseBuilderAi {
    /// Creates an idle builder AI (upstream ctor: `new Interval(4)`).
    pub fn new() -> Self {
        Self {
            interval: AiInterval::new(),
            found_path: false,
            path: BTreeSet::new(),
            calc_path: BTreeSet::new(),
            calc_tile: None,
            calculating: false,
            started_calculating: false,
            calc_count: 0,
            total_calcs: 0,
        }
    }

    /// `BaseBuilderAI.update`.
    pub fn update(
        &mut self,
        input: &mut BaseBuildInput<'_>,
        plans: &mut VecDeque<BlockPlan>,
    ) -> BaseBuildActions {
        let mut actions = BaseBuildActions {
            fill_core_items: !input.cores.is_empty(),
            spawn_core_unit: false,
            queued_plan: false,
        };

        // AI core unit spawning.
        if input.team_rule.ai_core_spawn
            && self.interval.get(TIMER_SPAWN, 60.0 * 6.0)
            && !input.cores.is_empty()
            && input.core_unit_count < input.cores.len() as i32 * CORE_UNIT_MULTIPLIER
        {
            actions.spawn_core_unit = true;
        }

        // Refresh path.
        if !self.calculating
            && (self.interval.get(TIMER_REFRESH_PATH, 3.0 * 3_600.0) || !self.started_calculating)
            && !input.cores.is_empty()
        {
            self.calculating = true;
            self.started_calculating = true;
            self.calc_path.clear();
        }

        // Didn't find a tile in time.
        let area = (input.grid.width() * input.grid.height()).max(0) as usize;
        if self.calculating && self.calc_count >= area {
            self.cancelled();
        }

        // Calculate the enemy path so schematics are not placed on it.
        if self.calculating {
            if self.calc_tile.is_none() {
                self.calc_tile = input.spawns.first().copied();
                if self.calc_tile.is_none() {
                    self.calculating = false;
                }
            } else if let Some(field) = input.enemy_core_field
                && field.done
            {
                self.trace_path(field, input.grid, input.enemy_cores);
            }
        }

        // Only schedule when there is something to build.
        let place_interval = lerp(
            PLACE_INTERVAL_MIN,
            PLACE_INTERVAL_MAX,
            input.team_rule.build_ai_tier,
        );
        if (self.found_path || !self.calculating)
            && plans.is_empty()
            && self.interval.get(TIMER_STEP, place_interval)
        {
            for _ in 0..ATTEMPTS {
                let Some(pos) = Self::random_position(input) else {
                    return actions;
                };
                let angle = input.rng.next_float() * std::f32::consts::TAU;
                let len = input.rng.random_float(PLACE_RANGE);
                let wx = pos.x() as i32 + (angle.cos() * len) as i32;
                let wy = pos.y() as i32 + (angle.sin() * len) as i32;
                let tile = input.grid.tiles.getc(wx, wy);
                let tx = tile.x as i32;
                let ty = tile.y as i32;

                // Try not to block the spawn point.
                if input
                    .spawns
                    .iter()
                    .any(|spawn| within(tx, ty, spawn, crate::config::TILESIZE as f32 * 40.0))
                {
                    continue;
                }

                let parts = if let Some(item) = tile_drop(tile, input.content) {
                    input.bases.for_resource(BaseResource::Item(item))
                } else if input.rng.chance(EMPTY_CHANCE) {
                    &input.bases.parts
                } else {
                    continue;
                };
                if parts.is_empty() {
                    continue;
                }

                let index = input.rng.next_int_bound(parts.len() as i32) as usize;
                let rotation = input.rng.next_int_bound(2);
                let part = &parts[index];
                if self.try_place(part, tx, ty, rotation, input, plans) {
                    actions.queued_plan = true;
                    break;
                }
            }
        }

        actions
    }

    /// Traces up to [`PATH_STEP`] nodes of the enemy-core flowfield.
    fn trace_path(&mut self, field: &Flowfield, grid: &WorldGrid, enemy_cores: &[TilePos]) {
        let width = grid.width();
        let height = grid.height();
        let weights = &field.complete_weights;
        let Some(mut calc) = self.calc_tile else {
            return;
        };

        for _ in 0..PATH_STEP {
            let cx = calc.x() as i32;
            let cy = calc.y() as i32;
            let mut min_cost = f32::INFINITY;
            let mut next: Option<TilePos> = None;
            for (dx, dy) in D4 {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= width || ny >= height {
                    continue;
                }
                let weight = weights[(nx + ny * width) as usize];
                if weight.is_finite() && weight < min_cost {
                    min_cost = weight;
                    next = Some(TilePos::new(nx as i16, ny as i16));
                }
            }

            let Some(next) = next else {
                self.calc_count = usize::MAX;
                break;
            };

            self.calc_path.insert(next.pack());
            for (dx, dy) in D8 {
                let px = (next.x() as i32 + dx) as i16;
                let py = (next.y() as i32 + dy) as i16;
                self.calc_path.insert(TilePos::new(px, py).pack());
            }

            if enemy_cores.contains(&next) {
                self.calculating = false;
                self.calc_count = 0;
                self.path = std::mem::take(&mut self.calc_path);
                self.calc_tile = None;
                self.total_calcs = self.total_calcs.wrapping_add(1);
                self.found_path = true;
                return;
            }

            self.calc_count += 1;
            calc = next;
        }
        self.calc_tile = Some(calc);
    }

    /// Cancels an in-progress calculation.
    fn cancelled(&mut self) {
        self.calculating = false;
        self.calc_count = 0;
        self.calc_path.clear();
        self.total_calcs = self.total_calcs.wrapping_add(1);
    }

    /// `randomPosition`: a random core (or spawn for the wave team).
    fn random_position(input: &mut BaseBuildInput<'_>) -> Option<TilePos> {
        if !input.cores.is_empty() {
            let index = input.rng.next_int_bound(input.cores.len() as i32) as usize;
            return Some(input.cores[index]);
        }
        if input.team == input.rules.wave_team && !input.spawns.is_empty() {
            let index = input.rng.next_int_bound(input.spawns.len() as i32) as usize;
            return Some(input.spawns[index]);
        }
        None
    }

    /// `tryPlace`: rotate, validate, then queue the plan.
    pub fn try_place(
        &self,
        part: &BasePart,
        x: i32,
        y: i32,
        rotation: i32,
        input: &BaseBuildInput<'_>,
        plans: &mut VecDeque<BlockPlan>,
    ) -> bool {
        let result =
            crate::game::schematics::Schematics::rotate(&part.schematic, rotation, input.content);
        let axis = (
            (part.schematic.width as f32 / 2.0) as i32,
            (part.schematic.height as f32 / 2.0) as i32,
        );
        let rotator = rotate_around(
            part.center_x as f32,
            part.center_y as f32,
            axis.0 as f32,
            axis.1 as f32,
            (rotation * 90) as f32,
        );
        let cx = x - rotator.0 as i32;
        let cy = y - rotator.1 as i32;

        for tile in &result.tiles {
            let real_x = tile.x as i32 + cx;
            let real_y = tile.y as i32 + cy;
            if !crate::world::build::valid_place(
                input.content,
                input.table,
                input.build_rules,
                input.counter,
                input.grid,
                tile.block,
                input.team,
                tile.rotation as u8,
                real_x,
                real_y,
            ) {
                return false;
            }
            let Some(def) = input.content.block(tile.block) else {
                return false;
            };
            if is_payload_block(def.kind) {
                // Near a building: mirror the upstream schematic-local `tile`
                // coordinates (parity, see plan-11 Changelog).
                for edge in crate::world::edges::edges(def.size) {
                    let ex = tile.x as i32 + edge.x() as i32;
                    let ey = tile.y as i32 + edge.y() as i32;
                    if input
                        .grid
                        .tiles
                        .getn(ex, ey)
                        .is_some_and(|t| t.build.is_some())
                    {
                        return false;
                    }
                }
            }

            // May intersect the AI path.
            if def.solid {
                for (lx, ly) in linked_tiles(real_x, real_y, def.size, def.size_offset) {
                    if self
                        .path
                        .contains(&TilePos::new(lx as i16, ly as i16).pack())
                    {
                        return false;
                    }
                }
            }
        }

        // Make sure the drill resource requirements fit.
        if let Some(BaseResource::Item(required)) = part.required {
            let mut correct = 0;
            let mut incorrect = 0;
            let mut any_drills = false;
            for tile in &result.tiles {
                let Some(def) = input.content.block(tile.block) else {
                    continue;
                };
                if def.kind != BlockKind::Drill {
                    continue;
                }
                any_drills = true;
                for (ex, ey) in taken_tiles(tile.x as i32 + cx, tile.y as i32 + cy, def.size) {
                    let drop = input
                        .grid
                        .tiles
                        .getn(ex, ey)
                        .and_then(|t| tile_drop(t, input.content));
                    if drop == Some(required) {
                        correct += 1;
                    } else if drop.is_some() {
                        incorrect += 1;
                    }
                }
            }
            if any_drills && (incorrect != 0 || correct == 0) {
                return false;
            }
        }

        for tile in &result.tiles {
            plans.push_back(BlockPlan {
                x: (cx + tile.x as i32) as i16,
                y: (cy + tile.y as i32) as i16,
                rotation: tile.rotation,
                block: tile.block,
                config: Some(tile.config.clone()),
                removed: false,
            });
        }
        true
    }
}

/// Whether `kind` is a payload block/conveyor (upstream `PayloadBlock` family).
pub fn is_payload_block(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::PayloadConveyor
            | BlockKind::PayloadRouter
            | BlockKind::PayloadMassDriver
            | BlockKind::PayloadDeconstructor
            | BlockKind::Constructor
            | BlockKind::PayloadLoader
            | BlockKind::PayloadUnloader
            | BlockKind::PayloadSource
            | BlockKind::PayloadVoid
    )
}

/// Footprint tiles of a block placed at `(x, y)` (`Tile.getLinkedTilesAs`).
fn linked_tiles(x: i32, y: i32, size: i32, size_offset: i32) -> Vec<(i32, i32)> {
    if size <= 1 {
        return vec![(x, y)];
    }
    let mut out = Vec::with_capacity((size * size) as usize);
    for dx in 0..size {
        for dy in 0..size {
            out.push((x + dx + size_offset, y + dy + size_offset));
        }
    }
    out
}

/// `Block.iterateTaken` footprint (centered `-(size-1)/2`).
fn taken_tiles(x: i32, y: i32, size: i32) -> Vec<(i32, i32)> {
    if size <= 1 {
        return vec![(x, y)];
    }
    let offset = -(size - 1) / 2;
    let mut out = Vec::with_capacity((size * size) as usize);
    for dx in 0..size {
        for dy in 0..size {
            out.push((x + dx + offset, y + dy + offset));
        }
    }
    out
}

/// `Vec2.rotateAround(axis, degrees)`.
fn rotate_around(x: f32, y: f32, axis_x: f32, axis_y: f32, degrees: f32) -> (f32, f32) {
    if degrees == 0.0 {
        return (x, y);
    }
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = x - axis_x;
    let dy = y - axis_y;
    (axis_x + dx * cos - dy * sin, axis_y + dx * sin + dy * cos)
}

/// `Mathf.lerp`.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// `Tile.drop()`: overlay ore first, then floor ore (upstream ignores `block`).
///
/// The plan-06 [`crate::world::Tile::drop`] reads the wall slot; this helper
/// mirrors upstream exactly so base building keys off floor/overlay ores.
fn tile_drop(
    tile: &crate::world::Tile,
    content: &ContentRegistry,
) -> Option<crate::content::ItemId> {
    let overlay = content.block(tile.overlay).and_then(|def| def.item_drop);
    if overlay.is_some() {
        return overlay;
    }
    content.block(tile.floor).and_then(|def| def.item_drop)
}

/// `Tile.within(tile, radius)` in world-pixel centers.
fn within(x: i32, y: i32, other: &TilePos, radius: f32) -> bool {
    let tile = crate::config::TILESIZE as f32;
    let ax = x as f32 * tile + tile / 2.0;
    let ay = y as f32 * tile + tile / 2.0;
    let bx = other.x() as f32 * tile + tile / 2.0;
    let by = other.y() as f32 * tile + tile / 2.0;
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt() <= radius
}

/// Creates the per-`TeamData` builder lazily (`TeamRule.buildAi`).
pub fn ensure_build_ai(data: &mut crate::game::teams::TeamData) -> bool {
    if data.build_ai.is_none() {
        data.build_ai = Some(BaseBuilderAi::new());
        true
    } else {
        false
    }
}

/// Runs one builder update for a `TeamData`, queueing plans into
/// `data.plans` (`BaseBuilderAI.update` per active team).
pub fn update_team_data(
    data: &mut crate::game::teams::TeamData,
    input: &mut BaseBuildInput<'_>,
) -> BaseBuildActions {
    let crate::game::teams::TeamData {
        build_ai, plans, ..
    } = data;
    match build_ai {
        Some(ai) => ai.update(input, plans),
        None => BaseBuildActions::default(),
    }
}

/// Convenience: classify a decoded schematic into a [`BasePart`] (tests).
#[cfg(test)]
fn classify_for_test(
    registry: &ContentRegistry,
    schem: crate::game::schematic::Schematic,
) -> BasePart {
    let mut bases = BaseRegistry::new();
    bases.load(registry, vec![schem]);
    bases
        .cores
        .into_iter()
        .chain(bases.parts)
        .next()
        .expect("part")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::schematic::{Schematic, Stile};
    use crate::world::BuildHarness;
    use crate::world::config::ConfigValue;
    use indexmap::IndexMap;

    fn content() -> ContentRegistry {
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        registry.init().expect("init");
        registry
    }

    fn wall_part(registry: &ContentRegistry) -> BasePart {
        let wall = registry.block_id("copper-wall").expect("wall");
        let schem = Schematic::from_tiles(
            vec![Stile::new(wall, 0, 0, ConfigValue::None, 0)],
            IndexMap::new(),
            1,
            1,
        );
        classify_for_test(registry, schem)
    }

    macro_rules! input {
        ($harness:expr, $rng:expr, $bases:expr, $cores:expr, $spawns:expr) => {
            BaseBuildInput {
                content: &$harness.content,
                grid: &$harness.grid,
                table: $harness.table(),
                build_rules: &$harness.rules,
                counter: &$harness.counter,
                team_rule: &TeamRule::default(),
                rules: &Rules::default(),
                team: 0,
                cores: $cores,
                spawns: $spawns,
                enemy_core_field: None,
                enemy_cores: &[],
                core_unit_count: 0,
                bases: $bases,
                rng: $rng,
            }
        };
    }

    #[test]
    fn interval_fires_after_accumulating_ticks() {
        let mut interval = AiInterval::new();
        assert!(!interval.get(0, 3.0));
        assert!(!interval.get(0, 3.0));
        assert!(interval.get(0, 3.0), "fires on the third tick");
        assert!(!interval.get(0, 3.0), "resets after firing");
    }

    #[test]
    fn interval_slots_are_independent() {
        let mut interval = AiInterval::new();
        assert!(!interval.get(0, 3.0));
        assert!(!interval.get(0, 3.0));
        assert!(!interval.get(1, 2.0));
        assert!(interval.get(1, 2.0), "slot 1 fires");
        assert!(interval.get(0, 3.0), "slot 0 fires independently");
    }

    #[test]
    fn try_place_queues_plan_on_flat_ground() {
        let registry = content();
        let harness = BuildHarness::new(32, 32, 7);
        let part = wall_part(&registry);
        let builder = BaseBuilderAi::new();
        let mut rng = ArcRand::new(1);
        let mut plans = VecDeque::new();
        let bases = BaseRegistry::new();
        let inputs = input!(harness, &mut rng, &bases, &[], &[]);
        assert!(builder.try_place(&part, 10, 10, 0, &inputs, &mut plans));
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].block, registry.block_id("copper-wall").unwrap());
    }

    #[test]
    fn try_place_rejects_overlapping_solid_block() {
        let registry = content();
        let mut harness = BuildHarness::new(16, 16, 7);
        let conveyor = registry.block_id("conveyor").unwrap();
        assert!(harness.place(5, 5, conveyor, 0, true));
        let part = wall_part(&registry);
        let builder = BaseBuilderAi::new();
        let mut rng = ArcRand::new(1);
        let mut plans = VecDeque::new();
        let bases = BaseRegistry::new();
        let inputs = input!(harness, &mut rng, &bases, &[], &[]);
        assert!(!builder.try_place(&part, 5, 5, 0, &inputs, &mut plans));
        assert!(plans.is_empty());
    }

    #[test]
    fn path_blocks_solid_placement() {
        let registry = content();
        let harness = BuildHarness::new(16, 16, 7);
        let part = wall_part(&registry);
        let mut builder = BaseBuilderAi::new();
        builder.path.insert(TilePos::new(8, 8).pack());
        let mut rng = ArcRand::new(1);
        let mut plans = VecDeque::new();
        let bases = BaseRegistry::new();
        let inputs = input!(harness, &mut rng, &bases, &[], &[]);
        assert!(
            !builder.try_place(&part, 8, 8, 0, &inputs, &mut plans),
            "solid placement on the AI path is rejected"
        );
    }

    #[test]
    fn update_queues_plan_when_parts_available() {
        let registry = content();
        let mut harness = BuildHarness::new(24, 24, 7);
        let wall = registry.block_id("copper-wall").unwrap();
        let source = registry.block_id("item-source").unwrap();
        let copper = registry.item_by_name("copper").unwrap().id;
        // Required part: a wall plus an item source configured for copper.
        let schem = Schematic::from_tiles(
            vec![
                Stile::new(wall, 0, 0, ConfigValue::None, 0),
                Stile::new(source, 1, 0, ConfigValue::Item(copper), 0),
            ],
            IndexMap::new(),
            2,
            1,
        );
        let mut bases = BaseRegistry::new();
        bases.load(&registry, vec![schem]);
        assert!(
            !bases.for_resource(BaseResource::Item(copper)).is_empty(),
            "required-copper part registers"
        );

        // Cover the map in copper ore so any jittered tile matches.
        let ore = registry.block_id("ore-copper").unwrap();
        for y in 0..24 {
            for x in 0..24 {
                harness.grid.tiles.get_mut(x, y).floor = ore;
            }
        }

        let mut builder = BaseBuilderAi::new();
        let mut rng = ArcRand::new(7);
        let mut plans = VecDeque::new();
        let cores = [TilePos::new(12, 12)];
        let mut inputs = input!(harness, &mut rng, &bases, &cores, &[]);
        let actions = builder.update(&mut inputs, &mut plans);
        assert!(actions.fill_core_items);
        // The placement timer lerps to `PLACE_INTERVAL_MIN` (720 ticks) at tier 0.
        for _ in 0..2500 {
            builder.update(&mut inputs, &mut plans);
            if !plans.is_empty() {
                break;
            }
        }
        assert!(!plans.is_empty(), "base builder queued a plan");
        assert_eq!(plans[0].block, wall);
    }
}

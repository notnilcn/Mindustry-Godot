// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Production block behavior (`world/blocks/production/*`).
//!
//! Plan 07 §3.12 production family. The craft loop is a faithful port of
//! `GenericCrafter.GenericCrafterBuild.updateTile`/`craft`/`getProgressIncrease`
//! and `Separator` (weighted results). Drills port the `Drill.DrillBuild` math;
//! ore counting needs the tile grid, so it is exposed as [`refresh_drill_ore`],
//! a grid-taking free function called by placement/the harness (Java runs
//! `countOre` from `onProximityUpdate`). Item/liquid output uses the module API;
//! cross-building transport is plans 08/09.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{ContentRegistry, ItemId, LiquidId};
use crate::entities::comp::{Building, CrafterState, DrillState, PumpState};
use crate::world::block::BlockTable;
use crate::world::block_kind_data::{CrafterDef, DrillDef};
use crate::world::modules::{ItemModule, LiquidModule};
use crate::world::update::{delta, edelta, get_progress_increase};
use crate::world::{BlockKindData, WorldGrid};

use super::{BuildingBehavior, BuildingReader, BuildingWriter};

/// `GenericCrafter.warmupSpeed`.
pub const CRAFTER_WARMUP_SPEED: f32 = 0.019;

/// `Mathf.approachDelta` (fixed step => `speed` per tick).
fn approach(current: f32, target: f32, speed: f32) -> f32 {
    if current < target {
        (current + speed).min(target)
    } else {
        (current - speed).max(target)
    }
}

fn instance(
    world: &World,
    e: Entity,
) -> Option<std::sync::Arc<crate::world::block::BlockInstance>> {
    let block = world.get::<Building>(e)?.block;
    world.get_resource::<BlockTable>()?.instance(block)
}

/// Generic-crafter knobs for any crafter-like block.
fn crafter_knobs(data: &BlockKindData) -> Option<CrafterDef> {
    match data {
        BlockKindData::Crafter(def) => Some(def.clone()),
        BlockKindData::AttributeCrafter(def) => Some(def.crafter.clone()),
        BlockKindData::Separator(def) => Some(CrafterDef {
            craft_time: def.craft_time,
            output_items: Vec::new(),
            output_liquids: Vec::new(),
            ignore_liquid_fullness: false,
        }),
        _ => None,
    }
}

fn produce_item(world: &mut World, e: Entity, item: ItemId, amount: i32, capacity: i32) {
    if amount > 0
        && let Some(mut module) = world.get_mut::<ItemModule>(e)
    {
        module.add(item, amount, capacity);
    }
}

fn produce_liquid(world: &mut World, e: Entity, liquid: LiquidId, amount: f32, capacity: f32) {
    if amount > 0.0
        && let Some(mut module) = world.get_mut::<LiquidModule>(e)
    {
        module.add(liquid, amount, capacity);
    }
}

fn liquid_capacity(world: &World, e: Entity) -> f32 {
    instance(world, e)
        .map(|inst| inst.def.liquid_capacity.max(0.0))
        .unwrap_or(0.0)
}

/// Deterministic 64-bit LCG step, returns a bounded value in `0..bound`.
fn next_bounded(state: &mut u64, bound: u32) -> u32 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    ((*state >> 33) as u32) % bound.max(1)
}

/// `GenericCrafter`/`Separator`/`AttributeCrafter` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct CrafterBehavior;

impl BuildingBehavior for CrafterBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<CrafterState>(e).is_some() {
            return;
        }
        let outputs = instance(world, e)
            .and_then(|inst| crafter_knobs(&inst.kind_data))
            .map(|def| def.output_items.len())
            .unwrap_or(0);
        let state = CrafterState {
            output_accumulator: smallvec::smallvec![0.0; outputs],
            ..Default::default()
        };
        world.entity_mut(e).insert(state);
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(inst) = instance(world, e) else {
            return;
        };
        let Some(crafter) = crafter_knobs(&inst.kind_data) else {
            return;
        };
        let efficiency = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        let delta = delta(world, e);
        let item_capacity = inst.def.item_capacity;

        if efficiency > 0.0 {
            let inc = get_progress_increase(world, e, crafter.craft_time.max(0.0001));
            if let Some(mut state) = world.get_mut::<CrafterState>(e) {
                state.progress += inc;
                state.warmup = approach(state.warmup, 1.0, CRAFTER_WARMUP_SPEED);
            }
            if !crafter.output_liquids.is_empty() {
                let inc = get_progress_increase(world, e, 1.0);
                let capacity = liquid_capacity(world, e);
                for (liquid, amount) in &crafter.output_liquids {
                    produce_liquid(world, e, LiquidId::new(*liquid), amount * inc, capacity);
                }
            }
        } else if let Some(mut state) = world.get_mut::<CrafterState>(e) {
            state.warmup = approach(state.warmup, 0.0, CRAFTER_WARMUP_SPEED);
        }

        if let Some(mut state) = world.get_mut::<CrafterState>(e) {
            state.total_progress += state.warmup * delta;
        }

        if world
            .get::<CrafterState>(e)
            .map(|state| state.progress)
            .unwrap_or(0.0)
            >= 1.0
        {
            craft(world, e, &crafter, item_capacity);
        }
    }

    fn efficiency_scale(&self, world: &mut World, e: Entity) -> f32 {
        let Some(inst) = instance(world, e) else {
            return 1.0;
        };
        match &inst.kind_data {
            BlockKindData::AttributeCrafter(def) => def.base_efficiency.max(0.0),
            _ => 1.0,
        }
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        if let Some(state) = world.get::<CrafterState>(e) {
            w.f(state.progress);
            w.f(state.warmup);
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, _revision: u8) {
        let progress = r.f().unwrap_or(0.0);
        let warmup = r.f().unwrap_or(0.0);
        if let Some(mut state) = world.get_mut::<CrafterState>(e) {
            state.progress = progress;
            state.warmup = warmup;
        }
    }
}

/// `GenericCrafter.craft`: consume item inputs, then emit outputs.
fn craft(world: &mut World, e: Entity, crafter: &CrafterDef, item_capacity: i32) {
    // `consume()`: remove item consumers.
    let item_consumers: Vec<Vec<(u16, i32)>> = instance(world, e)
        .map(|inst| {
            inst.consumers
                .all
                .iter()
                .filter_map(|consumer| match &consumer.kind {
                    crate::world::consumers::ConsumeInstanceKind::Items(stacks) => Some(
                        stacks
                            .iter()
                            .map(|stack| (stack.item.raw(), stack.amount))
                            .collect(),
                    ),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(mut module) = world.get_mut::<ItemModule>(e) {
        for stacks in &item_consumers {
            for (item, amount) in stacks {
                module.remove(ItemId::new(*item), *amount);
            }
        }
    }

    let separator_results = instance(world, e).and_then(|inst| match &inst.kind_data {
        BlockKindData::Separator(def) => Some(def.results.clone()),
        _ => None,
    });

    if let Some(results) = separator_results {
        let sum: i32 = results.iter().map(|(_, amount)| *amount).sum();
        if sum > 0 {
            let mut rng = world
                .get::<CrafterState>(e)
                .map(|state| state.rng)
                .unwrap_or(1);
            let pick = next_bounded(&mut rng, sum as u32) as i32;
            if let Some(mut state) = world.get_mut::<CrafterState>(e) {
                state.rng = rng;
            }
            let mut acc = 0;
            for (item, amount) in &results {
                if pick < acc + *amount {
                    produce_item(world, e, ItemId::new(*item), 1, item_capacity);
                    break;
                }
                acc += *amount;
            }
        }
    } else {
        let outputs = crafter.output_items.clone();
        let mut accumulator = world
            .get::<CrafterState>(e)
            .map(|state| state.output_accumulator.clone())
            .unwrap_or_default();
        if accumulator.len() != outputs.len() {
            accumulator = smallvec::smallvec![0.0; outputs.len()];
        }
        for (index, (item, amount)) in outputs.iter().enumerate() {
            accumulator[index] += *amount as f32;
            let floored = accumulator[index].floor();
            accumulator[index] -= floored;
            produce_item(world, e, ItemId::new(*item), floored as i32, item_capacity);
        }
        if let Some(mut state) = world.get_mut::<CrafterState>(e) {
            state.output_accumulator = accumulator;
        }
    }

    if let Some(mut state) = world.get_mut::<CrafterState>(e) {
        state.progress %= 1.0;
    }
}

/// `Drill`/`BurstDrill`/`WallCrafter` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct DrillBehavior;

impl BuildingBehavior for DrillBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<DrillState>(e).is_none() {
            world.entity_mut(e).insert(DrillState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(inst) = instance(world, e) else {
            return;
        };
        let def = match &inst.kind_data {
            BlockKindData::Drill(def) => def.clone(),
            BlockKindData::BurstDrill(def) => def.drill.clone(),
            BlockKindData::WallCrafter(def) => DrillDef {
                drill_time: def.drill_time,
                ..Default::default()
            },
            _ => return,
        };
        let (dominant, count) = world
            .get::<DrillState>(e)
            .map(|state| (state.dominant_item, state.dominant_items))
            .unwrap_or((None, 0));
        let Some(item) = dominant else {
            return;
        };
        let item_capacity = inst.def.item_capacity;
        let optional = world
            .get::<Building>(e)
            .map(|building| building.optional_efficiency)
            .unwrap_or(0.0);
        let efficiency = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        let delta = delta(world, e);
        let total = world
            .get::<ItemModule>(e)
            .map(ItemModule::total)
            .unwrap_or(0);
        let speed = (1.0 + 0.6 * optional) * efficiency;
        let delay = def.drill_time.max(0.0001);

        let (progress, ready) = {
            let Some(mut state) = world.get_mut::<DrillState>(e) else {
                return;
            };
            state.time_drilled += state.warmup * delta;
            if total < item_capacity && count > 0 && efficiency > 0.0 {
                state.last_drill_speed = (speed * count as f32 * state.warmup) / delay;
                state.warmup = approach(state.warmup, speed, 0.015);
                state.progress += delta * count as f32 * speed * state.warmup;
            } else {
                state.last_drill_speed = 0.0;
                state.warmup = approach(state.warmup, 0.0, 0.015);
                return;
            }
            (state.progress, total < item_capacity)
        };
        if count > 0 && ready && progress >= delay {
            let amount = (progress / delay) as i32;
            produce_item(world, e, item, amount.max(1), item_capacity);
            if let Some(mut state) = world.get_mut::<DrillState>(e) {
                state.progress %= delay;
            }
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        if let Some(state) = world.get::<DrillState>(e) {
            w.f(state.progress);
            w.f(state.warmup);
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8) {
        if revision >= 1 {
            let progress = r.f().unwrap_or(0.0);
            let warmup = r.f().unwrap_or(0.0);
            if let Some(mut state) = world.get_mut::<DrillState>(e) {
                state.progress = progress;
                state.warmup = warmup;
            }
        }
    }
}

/// Counts the dominant ore for a drill footprint from the tile grid
/// (`Drill.countOre`). Grid-taking so the ECS behavior stays grid-free.
pub fn refresh_drill_ore(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    e: Entity,
) {
    let Some(inst) = instance(world, e) else {
        return;
    };
    let Some(building) = world.get::<Building>(e) else {
        return;
    };
    let tile = building.tile;
    let size = inst.def.size.max(1);
    let (tier, blocked) = match &inst.kind_data {
        BlockKindData::Drill(def) => (def.tier, def.blocked_items.clone()),
        BlockKindData::BurstDrill(def) => (def.drill.tier, def.drill.blocked_items.clone()),
        BlockKindData::BeamDrill(def) => (def.tier, def.blocked_items.clone()),
        _ => return,
    };
    let (item, count) = count_ore(
        content,
        grid,
        tile.x() as i32,
        tile.y() as i32,
        size,
        tier,
        &blocked,
    );
    if let Some(mut state) = world.get_mut::<DrillState>(e) {
        state.dominant_item = item;
        state.dominant_items = count;
    }
}

/// Counts mineable floor drops over a `size x size` footprint.
pub fn count_ore(
    content: &ContentRegistry,
    grid: &WorldGrid,
    x: i32,
    y: i32,
    size: i32,
    tier: i32,
    blocked: &[u16],
) -> (Option<ItemId>, u16) {
    use indexmap::IndexMap;
    let mut counts: IndexMap<u16, u16> = IndexMap::new();
    for dx in 0..size.max(1) {
        for dy in 0..size.max(1) {
            let (tx, ty) = (x + dx, y + dy);
            if !grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            let floor = grid.tile(tx, ty).floor;
            let Some(def) = content.block(floor) else {
                continue;
            };
            let Some(drop) = def.item_drop else {
                continue;
            };
            let raw = drop.raw();
            if blocked.contains(&raw) {
                continue;
            }
            let hardness = content.item(drop).map(|item| item.hardness).unwrap_or(0);
            if hardness > tier {
                continue;
            }
            *counts.entry(raw).or_insert(0) += 1;
        }
    }
    let mut best: Option<(u16, u16)> = None;
    for (item, count) in counts {
        best = match best {
            Some((best_item, best_count))
                if count < best_count || (count == best_count && item > best_item) =>
            {
                Some((best_item, best_count))
            }
            _ => Some((item, count)),
        };
    }
    match best {
        Some((item, count)) => (Some(ItemId::new(item)), count),
        None => (None, 0),
    }
}

/// `BeamDrill` behavior (beam target selection is plan 16; mining math is shared
/// with [`DrillBehavior`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct BeamDrillBehavior;

impl BuildingBehavior for BeamDrillBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        DrillBehavior.create_state(world, e);
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        DrillBehavior.update_tile(world, e);
    }
}

/// `Pump` behavior (`Pump.PumpBuild`); floor-liquid source is plan 06/09.
#[derive(Debug, Default, Clone, Copy)]
pub struct PumpBehavior;

impl BuildingBehavior for PumpBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<PumpState>(e).is_none() {
            world.entity_mut(e).insert(PumpState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(inst) = instance(world, e) else {
            return;
        };
        let amount = match &inst.kind_data {
            BlockKindData::Pump(def) => def.pump_amount,
            _ => return,
        };
        let efficiency = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        if efficiency <= 0.0 {
            return;
        }
        let capacity = liquid_capacity(world, e);
        produce_liquid(
            world,
            e,
            LiquidId::WATER,
            amount * edelta(world, e),
            capacity,
        );
    }
}

/// `SolidPump`/`Fracker` behavior (attribute scaling is plan 09).
#[derive(Debug, Default, Clone, Copy)]
pub struct SolidPumpBehavior;

impl BuildingBehavior for SolidPumpBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<PumpState>(e).is_none() {
            world.entity_mut(e).insert(PumpState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(inst) = instance(world, e) else {
            return;
        };
        let (amount, result) = match &inst.kind_data {
            BlockKindData::SolidPump(def) => (def.pump_amount, None),
            BlockKindData::Fracker(def) => (def.pump_amount, def.result),
            _ => return,
        };
        let efficiency = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        {
            let Some(mut state) = world.get_mut::<PumpState>(e) else {
                return;
            };
            state.warmup = approach(state.warmup, if efficiency > 0.0 { 1.0 } else { 0.0 }, 0.02);
        }
        if efficiency <= 0.0 {
            return;
        }
        let capacity = liquid_capacity(world, e);
        let liquid = result.map(LiquidId::new).unwrap_or(LiquidId::WATER);
        let warmup = world.get::<PumpState>(e).map(|s| s.warmup).unwrap_or(1.0);
        produce_liquid(
            world,
            e,
            liquid,
            amount * edelta(world, e) * warmup,
            capacity,
        );
    }
}

/// `Incinerator` behavior (power-gated burn).
#[derive(Debug, Default, Clone, Copy)]
pub struct IncineratorBehavior;

impl BuildingBehavior for IncineratorBehavior {
    fn update_tile(&self, world: &mut World, e: Entity) {
        let efficiency = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        if efficiency <= 0.0 {
            return;
        }
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            let stacks: Vec<ItemId> = items.stacks().map(|(item, _)| item).collect();
            for item in stacks {
                items.remove(item, i32::MAX);
            }
        }
        if let Some(mut liquids) = world.get_mut::<LiquidModule>(e) {
            let present: Vec<LiquidId> = liquids
                .liquids
                .iter()
                .enumerate()
                .filter(|(_, amount)| **amount > 0.0)
                .map(|(index, _)| LiquidId::new(index as u16))
                .collect();
            for liquid in present {
                liquids.remove(liquid, f32::MAX);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use crate::world::modules::PowerModule;
    use crate::world::update::update_buildings;
    use bevy_ecs::world::World as EcsWorld;

    fn setup(name: &str) -> (EcsWorld, ContentRegistry, Entity) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table.get_named(name).expect(name).clone();
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        let entity = inst.spawn(
            &mut world,
            0,
            TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        inst.behavior.create_state(&mut world, entity);
        world.insert_resource(table);
        (world, content, entity)
    }

    #[test]
    fn silicon_smelter_crafts_after_craft_time() {
        let (mut world, content, entity) = setup("silicon-smelter");
        let coal = content.item_id("coal").expect("coal");
        let sand = content.item_id("sand").expect("sand");
        {
            let mut items = world.get_mut::<ItemModule>(entity).expect("items");
            items.add(coal, 100, 100);
            items.add(sand, 100, 100);
        }
        if let Some(mut power) = world.get_mut::<PowerModule>(entity) {
            power.status = 1.0;
        }
        for _ in 0..41 {
            update_buildings(&mut world);
        }
        let silicon = content.item_id("silicon").expect("silicon");
        let total = world
            .get::<ItemModule>(entity)
            .map(|items| items.get(silicon))
            .unwrap_or(0);
        assert!(total >= 1, "expected silicon output, got {total}");
    }

    #[test]
    fn separator_produces_weighted_result() {
        let (mut world, content, entity) = setup("separator");
        if let Some(mut power) = world.get_mut::<PowerModule>(entity) {
            power.status = 1.0;
        }
        // Separator has a slag liquid consumer; give it plenty.
        if let Some(mut liquids) = world.get_mut::<LiquidModule>(entity)
            && let Some(slag) = content.liquid_id("slag")
        {
            liquids.add(slag, 1000.0, 1000.0);
        }
        let total_before = world
            .get::<ItemModule>(entity)
            .map(ItemModule::total)
            .unwrap_or(0);
        for _ in 0..200 {
            if let Some(mut building) = world.get_mut::<Building>(entity) {
                building.efficiency = 1.0;
            }
            update_buildings(&mut world);
        }
        let total_after = world
            .get::<ItemModule>(entity)
            .map(ItemModule::total)
            .unwrap_or(0);
        assert!(
            total_after > total_before,
            "separator produced nothing: {total_before} -> {total_after}"
        );
    }

    #[test]
    fn drill_produces_from_assigned_ore() {
        let (mut world, content, entity) = setup("mechanical-drill");
        let copper = content.item_id("copper").expect("copper");
        if let Some(mut state) = world.get_mut::<DrillState>(entity) {
            state.dominant_item = Some(copper);
            state.dominant_items = 1;
        }
        for _ in 0..2000 {
            if let Some(mut building) = world.get_mut::<Building>(entity) {
                building.efficiency = 1.0;
            }
            update_buildings(&mut world);
        }
        let total = world
            .get::<ItemModule>(entity)
            .map(|items| items.get(copper))
            .unwrap_or(0);
        assert!(total >= 1, "expected drilled copper, got {total}");
    }

    #[test]
    fn ore_counting_prefers_dominant_drop() {
        let content = test_registry();
        let mut grid = WorldGrid::new(8, 8);
        let ore = content.block_id("ore-copper").expect("ore-copper");
        for x in 0..3 {
            for y in 0..3 {
                grid.tiles.get_mut(x, y).floor = ore;
            }
        }
        let copper = content.item_id("copper").expect("copper");
        let (item, count) = count_ore(&content, &grid, 0, 0, 3, 5, &[]);
        assert_eq!(item, Some(copper));
        assert_eq!(count, 9);
    }
}

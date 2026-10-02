// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Building spawn + per-tick update (`BuildingComp.update`/`updateConsumption`).
//!
//! Ported from `core/src/mindustry/entities/comp/BuildingComp.java:1950-2013`
//! (`updateConsumption`), `:1929-1943` (`delta`/`edelta`/multiplier) and
//! `:2270-2282` (`update`). The dispatch loop mirrors plan 07 §3.4: iterate
//! buildings in stable sequence order, decay `timeScale`, run the consumer pass,
//! then call the registered behavior.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{ItemId, LiquidId};
use crate::ecs::EntitySeq;
use crate::entities::comp::{Building, Health, Pos, TeamComp, Timers};
use crate::world::config::ConfigValue;
use crate::world::modules::{ItemModule, LiquidModule, PowerModule};

use super::TilePos;
use super::block::{BlockInstance, BlockTable};
use super::limits::BuildRules;

/// Rust-only `ProximityUpdateEvent` observable (plan 07 §3.8).
pub use super::proximity::ProximityUpdateEvent;

impl BlockInstance {
    /// Spawns a building entity for this block (`BuildingComp.create`/`init`).
    ///
    /// `seq` is the deterministic entity sequence allocated by the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &self,
        world: &mut World,
        seq: u64,
        tile: TilePos,
        team: u8,
        rot: u8,
        item_count: usize,
        liquid_count: usize,
    ) -> Entity {
        let center_x = (tile.x() as f32 + 0.5) * super::block::TILE_SIZE;
        let center_y = (tile.y() as f32 + 0.5) * super::block::TILE_SIZE;
        let mut entity = world.spawn((
            EntitySeq(seq),
            TeamComp { team },
            Pos {
                x: center_x,
                y: center_y,
            },
            Health::new(self.def.health.max(0) as f32),
            Building::new(tile, self.def.id, rot),
            Timers::default(),
        ));
        if self.def.has_items {
            entity.insert(ItemModule::with_items(item_count));
        }
        if self.def.has_liquids {
            entity.insert(LiquidModule::with_liquids(liquid_count));
        }
        if self.def.has_power {
            entity.insert(PowerModule::new());
        }
        let id = entity.id();
        // Family state components.
        self.behavior.create_state(world, id);
        self.behavior.created(world, id);
        id
    }
}

/// Iterator order key: sequence then entity index (stable, deterministic).
fn fill_building_order(world: &World, order: &mut Vec<(u64, Entity)>) {
    order.clear();
    order.extend(world.iter_entities().filter_map(|entity_ref| {
        entity_ref.get::<Building>()?;
        let seq = entity_ref
            .get::<EntitySeq>()
            .map(|seq| seq.0)
            .unwrap_or(u64::MAX);
        Some((seq, entity_ref.id()))
    }));
    order.sort_by_key(|(seq, entity)| (*seq, entity.index()));
}

/// Reusable scratch buffer so `update_buildings` allocates nothing after warmup
/// (plan 07 §7d alloc-audit).
#[derive(Debug, Default, bevy_ecs::prelude::Resource)]
pub struct BuildScratch {
    /// Building iteration order.
    pub order: Vec<(u64, Entity)>,
}

/// Sim clock tracked for building updates (`Time.time` subset, `+1.0`/tick).
///
/// Plan 05 owns the authoritative `SimClock`; this lightweight accumulating
/// resource is the value logistics time-delayed queues read as `now` during
/// `update_tile` (plan 08 L5). Hosts/harnesses insert it; systems that do not
/// advance it observe a constant `0.0` (buffers then release immediately,
/// matching a `now == 0` upstream frame).
#[derive(Debug, Clone, Copy, Default, bevy_ecs::prelude::Resource)]
pub struct BuildClock {
    /// Accumulated sim time in ticks.
    pub time: f32,
}

/// `Time.time` for logistics buffers, or `0.0` when no clock is installed.
pub fn build_time(world: &World) -> f32 {
    world
        .get_resource::<BuildClock>()
        .map(|clock| clock.time)
        .unwrap_or(0.0)
}

/// `EntitySet::UpdateBuildings` system (no-op when no [`BlockTable`] exists, so
/// the P0 `Sim` schedule/golden is untouched).
pub fn update_buildings(world: &mut World) {
    if !world.contains_resource::<BlockTable>() {
        return;
    }
    if !world.contains_resource::<BuildScratch>() {
        world.insert_resource(BuildScratch::default());
    }
    if let Some(mut clock) = world.get_resource_mut::<BuildClock>() {
        clock.time += 1.0;
    }
    let mut order = match world.get_resource_mut::<BuildScratch>() {
        Some(mut scratch) => std::mem::take(&mut scratch.order),
        None => Vec::new(),
    };
    fill_building_order(world, &mut order);
    for (_, entity) in order.iter().copied() {
        building_update(world, entity);
    }
    if let Some(mut scratch) = world.get_resource_mut::<BuildScratch>() {
        scratch.order = order;
    }
}

/// One building's `update()` (`BuildingComp.update`).
pub fn building_update(world: &mut World, entity: Entity) {
    // `(timeScaleDuration -= Time.delta) <= 0 -> timeScale = 1`.
    {
        let Some(mut building) = world.get_mut::<Building>(entity) else {
            return;
        };
        building.time_scale_duration -= 1.0;
        if building.time_scale_duration <= 0.0 {
            building.time_scale = 1.0;
        }
    }

    let Some(block_id) = world.get::<Building>(entity).map(|building| building.block) else {
        return;
    };
    let Some(inst) = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block_id))
    else {
        return;
    };

    update_consumption(world, entity, &inst);

    let (enabled, behaviour) = {
        let enabled = world
            .get::<Building>(entity)
            .is_some_and(|building| building.enabled);
        (enabled, inst.behavior.clone())
    };
    if enabled || behaviour.always_update_when_disabled() {
        behaviour.update_tile(world, entity);
    }
}

/// `BuildingComp.updateConsumption` (verbatim pass structure).
pub fn update_consumption(world: &mut World, entity: Entity, inst: &BlockInstance) {
    let cheating = world
        .get_resource::<BuildRules>()
        .is_some_and(|rules| rules.cheat);

    let Some((enabled, time_scale)) = world
        .get::<Building>(entity)
        .map(|building| (building.enabled, building.time_scale))
    else {
        return;
    };
    let scale = inst.behavior.efficiency_scale(world, entity);
    let delta = time_scale;

    if inst.consumers.is_empty() || cheating {
        let potential = if enabled { 1.0 } else { 0.0 };
        let eff = potential * scale;
        set_efficiency(world, entity, potential, eff, eff, true);
        return;
    }

    if !enabled {
        set_efficiency(world, entity, 0.0, 0.0, 0.0, false);
        return;
    }

    let consumers = &inst.consumers;
    let pass_delta = delta * scale;

    let mut min_efficiency = 1.0f32;
    let mut optional_efficiency = 1.0f32;
    let mut should_consume_power = true;

    for &index in &consumers.non_optional {
        let consumer = &consumers.all[index];
        let result = consumer_efficiency(world, entity, consumer, pass_delta);
        if Some(index) != consumers.cons_power && result <= 0.000_000_1 {
            should_consume_power = false;
        }
        min_efficiency = min_efficiency.min(result);
    }
    for &index in &consumers.optional {
        let consumer = &consumers.all[index];
        optional_efficiency =
            optional_efficiency.min(consumer_efficiency(world, entity, consumer, pass_delta));
    }

    let mut efficiency = min_efficiency;
    let mut optional = optional_efficiency.min(min_efficiency);
    let potential = efficiency;

    let update = true;
    if !update {
        efficiency = 0.0;
        optional = 0.0;
    }

    efficiency *= scale;
    optional *= scale;

    if update && efficiency > 0.0 {
        let edelta = efficiency * delta;
        for &index in &consumers.update {
            consumer_update(world, entity, &consumers.all[index], edelta);
        }
    }

    set_efficiency(
        world,
        entity,
        potential,
        efficiency,
        optional,
        should_consume_power,
    );
}

fn set_efficiency(
    world: &mut World,
    entity: Entity,
    potential: f32,
    efficiency: f32,
    optional: f32,
    should_consume_power: bool,
) {
    if let Some(mut building) = world.get_mut::<Building>(entity) {
        building.potential_efficiency = potential;
        building.efficiency = efficiency;
        building.optional_efficiency = optional;
        building.should_consume_power = should_consume_power;
    }
}

/// Consumer `efficiency(build)` for one lowered consumer.
fn consumer_efficiency(
    world: &World,
    entity: Entity,
    consumer: &super::consumers::ConsumeInstance,
    pass_delta: f32,
) -> f32 {
    use super::consumers::ConsumeInstanceKind as K;
    match &consumer.kind {
        K::Items(stacks) => {
            let Some(module) = world.get::<ItemModule>(entity) else {
                return 0.0;
            };
            for stack in stacks {
                if module.get(stack.item) < stack.amount {
                    return 0.0;
                }
            }
            1.0
        }
        K::Liquid { liquid, amount } => {
            if pass_delta <= 0.000_000_1 {
                return 0.0;
            }
            let Some(module) = world.get::<LiquidModule>(entity) else {
                return 0.0;
            };
            (module.get(*liquid) / (amount * pass_delta)).min(1.0)
        }
        K::Liquids(stacks) => {
            if pass_delta <= 0.000_000_1 {
                return 0.0;
            }
            let Some(module) = world.get::<LiquidModule>(entity) else {
                return 0.0;
            };
            let mut min = 1.0f32;
            for stack in stacks {
                min = min.min(module.get(stack.liquid) / (stack.amount * pass_delta));
            }
            min.min(1.0)
        }
        K::Power { .. } => world
            .get::<PowerModule>(entity)
            .map(|module| module.status)
            .unwrap_or(0.0),
        K::Coolant { amount, .. } => {
            if pass_delta <= 0.000_000_1 {
                return 0.0;
            }
            let Some(module) = world.get::<LiquidModule>(entity) else {
                return 0.0;
            };
            match first_available_liquid(module) {
                Some(liquid) => (module.get(liquid) / (amount * pass_delta)).min(1.0),
                None => 0.0,
            }
        }
    }
}

/// Consumer `update(build)` for one lowered consumer.
fn consumer_update(
    world: &mut World,
    entity: Entity,
    consumer: &super::consumers::ConsumeInstance,
    edelta: f32,
) {
    use super::consumers::ConsumeInstanceKind as K;
    match &consumer.kind {
        // `ConsumeItems` has no per-tick update (it triggers on craft).
        K::Items(_) | K::Power { .. } => {}
        K::Liquid { liquid, amount } => {
            if let Some(mut module) = world.get_mut::<LiquidModule>(entity) {
                module.remove(*liquid, amount * edelta);
            }
        }
        K::Liquids(stacks) => {
            if let Some(mut module) = world.get_mut::<LiquidModule>(entity) {
                for stack in stacks {
                    module.remove(stack.liquid, stack.amount * edelta);
                }
            }
        }
        K::Coolant { amount, .. } => {
            if let Some(mut module) = world.get_mut::<LiquidModule>(entity)
                && let Some(liquid) = first_available_liquid(&module)
            {
                module.remove(liquid, amount * edelta);
            }
        }
    }
}

/// First liquid present in the module (liquid-id order), matching
/// `ConsumeLiquidFilter.getConsumed`'s `current` + content-order scan.
fn first_available_liquid(module: &LiquidModule) -> Option<LiquidId> {
    module
        .liquids
        .iter()
        .enumerate()
        .find(|(_, amount)| **amount > 0.0)
        .map(|(index, _)| LiquidId::new(index as u16))
}

/// `edelta()` for a building (`efficiency * delta`).
pub fn edelta(world: &World, entity: Entity) -> f32 {
    let Some(building) = world.get::<Building>(entity) else {
        return 0.0;
    };
    building.efficiency * building.time_scale
}

/// `delta()` for a building (`Time.delta * timeScale`; fixed step => `timeScale`).
pub fn delta(world: &World, entity: Entity) -> f32 {
    world
        .get::<Building>(entity)
        .map(|building| building.time_scale)
        .unwrap_or(0.0)
}

/// `getProgressIncrease(baseTime)` (`1/baseTime * edelta`).
pub fn get_progress_increase(world: &World, entity: Entity, base_time: f32) -> f32 {
    if base_time <= 0.0 {
        0.0
    } else {
        1.0 / base_time * edelta(world, entity)
    }
}

/// `Building.timer(index, interval)`: returns `true` when the interval elapsed
/// and resets it (`Timer`/`Interval`). Timers tick once per fixed-step update.
pub fn run_timer(world: &mut World, entity: Entity, index: usize, interval: f32) -> bool {
    let Some(mut timers) = world.get_mut::<Timers>(entity) else {
        return false;
    };
    if timers.0.len() <= index {
        timers.0.resize(index + 1, 0.0);
    }
    if timers.0[index] <= 0.0 {
        timers.0[index] = interval;
        true
    } else {
        timers.0[index] -= 1.0;
        false
    }
}

/// Puts a building to sleep (`Building.sleep`): flagged and skipped while asleep.
pub fn sleep(world: &mut World, entity: Entity) {
    if let Some(mut building) = world.get_mut::<Building>(entity) {
        building.sleeping = true;
    }
}

/// Wakes a building (`Building.noSleep`).
pub fn no_sleep(world: &mut World, entity: Entity) {
    if let Some(mut building) = world.get_mut::<Building>(entity) {
        building.sleeping = false;
        building.sleep_time = 0.0;
    }
}

/// Reads the last set config value for a building's block (`Building.config`).
pub fn block_config(table: &BlockTable, entity: Entity, world: &World) -> ConfigValue {
    let Some(building) = world.get::<Building>(entity) else {
        return ConfigValue::None;
    };
    let Some(inst) = table.get(building.block) else {
        return ConfigValue::None;
    };
    inst.behavior.config(world, entity)
}

/// Scratch item list used by tests (`ItemModule` sizes).
pub fn item_module_stacks(world: &World, entity: Entity) -> Vec<(ItemId, i32)> {
    world
        .get::<ItemModule>(entity)
        .map(|module| module.stacks().collect())
        .unwrap_or_default()
}

/// Scratch liquid amount (`LiquidModule.current`).
pub fn current_liquid(world: &World, entity: Entity) -> f32 {
    world
        .get::<LiquidModule>(entity)
        .map(LiquidModule::current)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::world::block::BlockTable;
    use crate::world::{BuildRules, TilePos};

    fn setup() -> (World, std::sync::Arc<BlockInstance>, Entity) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("silicon-smelter")
            .expect("silicon-smelter")
            .clone();
        let mut world = World::new();
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
        world.insert_resource(table);
        (world, inst, entity)
    }

    #[test]
    fn building_update_decays_time_scale_and_sets_efficiency() {
        let (mut world, _inst, entity) = setup();
        // Give the smelter its item inputs (power stays at 0 => efficiency 0).
        let content = test_registry();
        let coal = content.item_id("coal").expect("coal");
        let sand = content.item_id("sand").expect("sand");
        {
            let mut module = world.get_mut::<ItemModule>(entity).expect("items");
            module.add(coal, 100, 100);
            module.add(sand, 100, 100);
        }
        update_buildings(&mut world);
        let building = world.get::<Building>(entity).expect("building");
        assert_eq!(building.efficiency, 0.0);
        // The item consumers are satisfied, so power is still requested.
        assert!(building.should_consume_power);
    }

    #[test]
    fn disabled_building_has_zero_efficiency() {
        let (mut world, _inst, entity) = setup();
        world.get_mut::<Building>(entity).expect("building").enabled = false;
        update_buildings(&mut world);
        let building = world.get::<Building>(entity).expect("building");
        assert_eq!(building.efficiency, 0.0);
        assert!(!building.should_consume_power);
    }

    #[test]
    fn edelta_scales_with_efficiency_and_time() {
        let (mut world, _inst, entity) = setup();
        {
            let mut building = world.get_mut::<Building>(entity).expect("building");
            building.efficiency = 0.5;
            building.time_scale = 2.0;
        }
        assert_eq!(edelta(&world, entity), 1.0);
        assert_eq!(delta(&world, entity), 2.0);
        assert_eq!(get_progress_increase(&world, entity, 2.0), 0.5);
    }

    #[test]
    fn liquid_consumer_efficiency_math() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        // `water-extractor` consumes no liquid; use `coal-centrifuge` (water).
        let inst = table
            .get_named("coal-centrifuge")
            .expect("coal-centrifuge")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        let entity = inst.spawn(
            &mut world,
            0,
            TilePos::new(1, 1),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        // No liquid -> efficiency 0.
        update_consumption(&mut world, entity, &inst);
        assert_eq!(world.get::<Building>(entity).expect("b").efficiency, 0.0);
    }

    #[cfg(feature = "alloc-audit")]
    #[test]
    fn update_buildings_alloc_free_after_warmup() {
        use crate::util::alloc::alloc_count;
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table.get_named("copper-wall").expect("wall").clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        for i in 0..200u64 {
            inst.spawn(
                &mut world,
                i,
                TilePos::new((i % 20) as i16, (i / 20) as i16),
                0,
                0,
                content.items().len(),
                content.liquids().len(),
            );
        }
        world.insert_resource(table);
        for _ in 0..60 {
            update_buildings(&mut world);
        }
        let before = alloc_count();
        for _ in 0..600 {
            update_buildings(&mut world);
        }
        let after = alloc_count();
        assert_eq!(after, before, "update_buildings allocated after warmup");
    }
}

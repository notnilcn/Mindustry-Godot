// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Sandbox block behavior (`world/blocks/sandbox/*`).
//!
//! PAR-0077: one behavior per sandbox block, ported from
//! `ItemSource`/`ItemVoid`/`LiquidSource`/`LiquidVoid`/`PowerSource`/`PowerVoid`.
//! `ItemSource.updateTile` moves one configured item per `60/itemsPerSecond`
//! ticks through its item module and `dump`s it; `ItemVoid` accepts everything
//! and discards it; `LiquidSource` fills its liquid module and dumps;
//! `LiquidVoid` empties on placement and accepts everything; `PowerSource`
//! reports `powerProduction` (`0` while disabled); `PowerVoid` is a pure
//! `f32::MAX` consumer whose lowered `ConsumePower` the graph drives.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{ItemId, LiquidId};
use crate::entities::comp::{Building, SandboxState};
use crate::world::BlockKindData;
use crate::world::block::BlockInstance;
use crate::world::blocks::distribution::transfer;
use crate::world::blocks::liquid::movement::dump_liquid_proximity;
use crate::world::config::{ConfigKind, ConfigValue};
use crate::world::modules::{ItemModule, LiquidModule, ModuleDims};
use crate::world::update::edelta;

use super::BuildingBehavior;

/// Upper bound on `ItemSource` emissions per tick.
///
/// `counter` grows by `edelta()` (≤ `timeScale`) per tick and the limit is
/// `60/itemsPerSecond`, so the stock configuration loops at most twice. The cap
/// only guards a pathological configured rate; it never triggers for vanilla
/// content.
const MAX_EMISSIONS_PER_TICK: u32 = 1000;

fn instance(world: &World, e: Entity) -> Option<std::sync::Arc<BlockInstance>> {
    let block = world.get::<Building>(e)?.block;
    world
        .get_resource::<crate::world::block::BlockTable>()?
        .instance(block)
}

fn enabled(world: &World, e: Entity) -> bool {
    world
        .get::<Building>(e)
        .is_some_and(|building| building.enabled)
}

fn ensure_state(world: &mut World, e: Entity) {
    if world.get::<SandboxState>(e).is_none() {
        world.entity_mut(e).insert(SandboxState::default());
    }
}

fn ensure_item_module(world: &mut World, e: Entity) {
    if world.get::<ItemModule>(e).is_some() {
        return;
    }
    if let Some(items) = world.get_resource::<ModuleDims>().map(|dims| dims.items) {
        world.entity_mut(e).insert(ItemModule::with_items(items));
    }
}

fn ensure_liquid_module(world: &mut World, e: Entity) {
    if world.get::<LiquidModule>(e).is_some() {
        return;
    }
    if let Some(liquids) = world.get_resource::<ModuleDims>().map(|dims| dims.liquids) {
        world
            .entity_mut(e)
            .insert(LiquidModule::with_liquids(liquids));
    }
}

/// Takes the state component out (Java field mutation equivalent); the caller
/// writes it back with [`put_state`].
fn take_state(world: &mut World, e: Entity) -> Option<SandboxState> {
    world
        .get_mut::<SandboxState>(e)
        .map(|mut state| std::mem::take(&mut *state))
}

fn put_state(world: &mut World, e: Entity, state: SandboxState) {
    if let Some(mut slot) = world.get_mut::<SandboxState>(e) {
        *slot = state;
    } else {
        world.entity_mut(e).insert(state);
    }
}

fn set_configured_item(world: &mut World, e: Entity, item: Option<ItemId>) {
    if let Some(mut state) = world.get_mut::<SandboxState>(e) {
        state.item = item;
    }
}

fn set_configured_liquid(world: &mut World, e: Entity, liquid: Option<LiquidId>) {
    if let Some(mut state) = world.get_mut::<SandboxState>(e) {
        state.liquid = liquid;
    }
}

fn clear_liquid_module(world: &mut World, e: Entity) {
    if let Some(mut module) = world.get_mut::<LiquidModule>(e) {
        module.liquids.fill(0.0);
        module.current_amount = 0.0;
    }
}

/// `ItemSource` (`ItemSourceBuild.updateTile`): emit one configured item per
/// `60/itemsPerSecond` ticks, `dump` it, and clear it again. An unconfigured
/// source emits nothing (Java default `outputItem = null`).
#[derive(Debug, Default, Clone, Copy)]
pub struct ItemSourceBehavior;

impl BuildingBehavior for ItemSourceBehavior {
    fn always_update_when_disabled(&self) -> bool {
        // `ItemSource.noUpdateDisabled = true`.
        true
    }

    fn create_state(&self, world: &mut World, e: Entity) {
        ensure_state(world, e);
        ensure_item_module(world, e);
    }

    fn config_kinds(&self) -> &'static [ConfigKind] {
        &[ConfigKind::Item]
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        world
            .get::<SandboxState>(e)
            .and_then(|state| state.item)
            .map(ConfigValue::Item)
            .unwrap_or(ConfigValue::None)
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        match value {
            ConfigValue::Item(item) => set_configured_item(world, e, Some(item)),
            ConfigValue::None => set_configured_item(world, e, None),
            _ => {}
        }
    }

    fn write(&self, world: &World, e: Entity, w: &mut super::BuildingWriter) {
        // `ItemSourceBuild.write`: `write.s(outputItem == null ? -1 : outputItem.id)`.
        let item = world.get::<SandboxState>(e).and_then(|state| state.item);
        w.s(item.map(|item| item.raw() as i16).unwrap_or(-1));
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut super::BuildingReader, _revision: u8) {
        // `ItemSourceBuild.read`: `outputItem = content.item(read.s())`.
        let id = r.s().unwrap_or(-1);
        let item = (id >= 0).then(|| ItemId::new(id as u16));
        ensure_state(world, e);
        set_configured_item(world, e, item);
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(inst) = instance(world, e) else {
            return;
        };
        let BlockKindData::ItemSource(def) = &inst.kind_data else {
            return;
        };
        let items_per_second = def.items_per_second.max(0.0001);
        let Some(mut state) = take_state(world, e) else {
            return;
        };
        let Some(item) = state.item else {
            put_state(world, e, state);
            return;
        };
        state.accumulator += edelta(world, e);
        let limit = 60.0 / items_per_second;

        let mut emissions = 0u32;
        while state.accumulator >= limit {
            // `items.set(outputItem, 1); dump(outputItem); produced(outputItem); items.set(outputItem, 0);`
            if let Some(mut module) = world.get_mut::<ItemModule>(e) {
                let stored = module.get(item);
                module.add(item, 1 - stored, i32::MAX);
            }
            transfer::dump(world, e, Some(item));
            self.produced(world, e, item, 1);
            if let Some(mut module) = world.get_mut::<ItemModule>(e) {
                let _ = module.remove(item, i32::MAX);
            }
            state.accumulator -= limit;
            state.emitted = state.emitted.saturating_add(1);
            emissions += 1;
            if emissions >= MAX_EMISSIONS_PER_TICK {
                // Pathological configured rate guard; vanilla rates loop <= 2x.
                state.accumulator = 0.0;
                break;
            }
        }
        put_state(world, e, state);
    }
}

/// `ItemVoid`: accepts any item while enabled and discards it. Java keeps a
/// separate `flowItems` module purely for the UI flow cache; the port's
/// per-module [`crate::world::modules::FlowWindow`] is UI-only as well, so
/// nothing is stored here.
#[derive(Debug, Default, Clone, Copy)]
pub struct ItemVoidBehavior;

impl BuildingBehavior for ItemVoidBehavior {
    fn accept_item(&self, world: &World, e: Entity, _src: Entity, _item: ItemId) -> bool {
        // `ItemVoidBuild.acceptItem` returns `enabled`.
        enabled(world, e)
    }

    fn handle_item(&self, _world: &mut World, _e: Entity, _src: Entity, _item: ItemId) {
        // Items vanish; `flowItems.handleFlow` is the UI-only flow cache.
    }

    fn accept_stack(
        &self,
        world: &World,
        e: Entity,
        _item: ItemId,
        amount: i32,
        _source: Option<Entity>,
    ) -> i32 {
        if enabled(world, e) { amount } else { 0 }
    }

    fn handle_stack(&self, _world: &mut World, _e: Entity, _item: ItemId, _amount: i32) {
        // `handleStack` forwards to `handleItem`, which discards.
    }
}

/// `LiquidSource` (`LiquidSourceBuild.updateTile`): fill the configured liquid
/// to capacity and `dumpLiquid`; clear the module when unconfigured.
#[derive(Debug, Default, Clone, Copy)]
pub struct LiquidSourceBehavior;

impl BuildingBehavior for LiquidSourceBehavior {
    fn always_update_when_disabled(&self) -> bool {
        // `LiquidSource.noUpdateDisabled = true`.
        true
    }

    fn create_state(&self, world: &mut World, e: Entity) {
        ensure_state(world, e);
        ensure_liquid_module(world, e);
    }

    fn config_kinds(&self) -> &'static [ConfigKind] {
        &[ConfigKind::Liquid]
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        world
            .get::<SandboxState>(e)
            .and_then(|state| state.liquid)
            .map(ConfigValue::Liquid)
            .unwrap_or(ConfigValue::None)
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        match value {
            ConfigValue::Liquid(liquid) => set_configured_liquid(world, e, Some(liquid)),
            ConfigValue::None => set_configured_liquid(world, e, None),
            _ => {}
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        // `LiquidSourceBuild.version() = 1` (the source id widened to a short).
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut super::BuildingWriter) {
        // `LiquidSourceBuild.write`: `write.s(source == null ? -1 : source.id)`.
        let liquid = world.get::<SandboxState>(e).and_then(|state| state.liquid);
        w.s(liquid.map(|liquid| liquid.raw() as i16).unwrap_or(-1));
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut super::BuildingReader, revision: u8) {
        // `LiquidSourceBuild.read`: revision 1 reads a short, older saves a byte.
        let id = if revision == 1 {
            r.s().map(i32::from)
        } else {
            r.b().map(i32::from)
        }
        .unwrap_or(-1);
        let liquid = (id >= 0).then(|| LiquidId::new(id as u16));
        ensure_state(world, e);
        set_configured_liquid(world, e, liquid);
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let liquid = world.get::<SandboxState>(e).and_then(|state| state.liquid);
        let Some(liquid) = liquid else {
            // `if(source == null){ liquids.clear(); }`
            clear_liquid_module(world, e);
            return;
        };
        let capacity = super::helpers::liquid_capacity(world, e);
        crate::world::blocks::power::sandbox::update_liquid_source(world, e, liquid, capacity);
        dump_liquid_proximity(world, e, liquid, 2.0, -1);
    }
}

/// `LiquidVoid`: accepts any liquid while enabled and discards it. Java's
/// `liquids.handleFlow` writes the UI-only flow cache, never storage.
#[derive(Debug, Default, Clone, Copy)]
pub struct LiquidVoidBehavior;

impl BuildingBehavior for LiquidVoidBehavior {
    fn placed(&self, world: &mut World, e: Entity) {
        // `LiquidVoidBuild.placed(){ super.placed(); liquids.clear(); }`
        clear_liquid_module(world, e);
    }

    fn accept_liquid(&self, world: &World, e: Entity, _src: Entity, _liquid: LiquidId) -> bool {
        // `LiquidVoidBuild.acceptLiquid` returns `enabled`.
        enabled(world, e)
    }

    fn handle_liquid(
        &self,
        _world: &mut World,
        _e: Entity,
        _src: Entity,
        _liquid: LiquidId,
        _amount: f32,
    ) {
        // `liquids.handleFlow(liquid, amount)` is the UI-only flow cache.
    }
}

/// `PowerSource` (`PowerSourceBuild.getPowerProduction`): report
/// `powerProduction` while enabled, `0` otherwise. The lowered placement path
/// attaches `PowerModule`; the graph merges the source into its grid.
///
/// `PowerSource extends PowerNode` (`PowerSource.java:6-12`: `maxNodes = 100`,
/// the inherited `laserRange = 6`/`autolink = true`), so the source carries the
/// node config: a placed consumer in range autolinks to it
/// (`BuildingComp.placed` -> `PowerNode.getNodeLinks`) and taps link like a node.
#[derive(Debug, Default, Clone, Copy)]
pub struct PowerSourceBehavior;

/// `PowerSource`'s inherited `PowerNode` knobs (`PowerSource.java:11`,
/// `PowerNode.java:33-35`).
const POWER_SOURCE_NODE: crate::world::blocks::power::nodes::PowerNodeBehavior =
    crate::world::blocks::power::nodes::PowerNodeBehavior {
        max_nodes: 100,
        laser_range: 6.0,
        autolink: true,
        same_block_connection: false,
    };

impl BuildingBehavior for PowerSourceBehavior {
    fn always_update_when_disabled(&self) -> bool {
        // Java reads `getPowerProduction()` every graph update; the cached
        // `PowerProduction` component must follow `enabled` the same way.
        true
    }

    fn on_configure_build_tapped(
        &self,
        world: &World,
        grid: &crate::world::WorldGrid,
        e: Entity,
        other: Entity,
    ) -> super::ConfigTapResult {
        POWER_SOURCE_NODE.on_configure_build_tapped(world, grid, e, other)
    }

    fn create_state(&self, world: &mut World, e: Entity) {
        POWER_SOURCE_NODE.create_state(world, e);
        if world
            .get::<crate::world::blocks::power::PowerProduction>(e)
            .is_none()
        {
            world
                .entity_mut(e)
                .insert(crate::world::blocks::power::PowerProduction(0.0));
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(inst) = instance(world, e) else {
            return;
        };
        let BlockKindData::PowerSource(def) = &inst.kind_data else {
            return;
        };
        let production = if enabled(world, e) {
            def.power_production
        } else {
            0.0
        };
        crate::world::blocks::power::nodes::set_power_source(world, e, production);
    }
}

/// `PowerVoid`: a `PowerBlock` with `consumePower(f32::MAX)`. The consumer is
/// lowered into `Consumers` from the block def, and the power graph's
/// per-tick distribution sets `PowerModule.status`; no `updateTile` exists.
#[derive(Debug, Default, Clone, Copy)]
pub struct PowerVoidBehavior;

impl BuildingBehavior for PowerVoidBehavior {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::entities::comp::{Building, TeamComp};
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use bevy_ecs::world::World as EcsWorld;

    fn spawn(world: &mut EcsWorld, name: &str) -> (std::sync::Arc<BlockInstance>, Entity) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table.get_named(name).expect(name).clone();
        world.insert_resource(BuildRules::default());
        world.insert_resource(crate::world::modules::ModuleDims {
            items: content.items().len(),
            liquids: content.liquids().len(),
        });
        world.insert_resource(table);
        let e = inst.spawn(
            world,
            0,
            TilePos::new(2, 2),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        (inst, e)
    }

    #[test]
    fn item_source_configure_and_emit() {
        let mut world = EcsWorld::new();
        let (inst, e) = spawn(&mut world, "item-source");
        inst.behavior.create_state(&mut world, e);
        let copper = crate::content::ItemId::new(0);
        inst.behavior
            .configured(&mut world, e, None, ConfigValue::Item(copper));
        assert_eq!(inst.behavior.config(&world, e), ConfigValue::Item(copper));
        assert!(world.get::<Building>(e).is_some());
        assert!(world.get::<TeamComp>(e).is_some());
        // Unconfigured clears back to `None`.
        inst.behavior
            .configured(&mut world, e, None, ConfigValue::None);
        assert_eq!(inst.behavior.config(&world, e), ConfigValue::None);
    }

    #[test]
    fn item_void_accepts_only_while_enabled() {
        let mut world = EcsWorld::new();
        let (inst, e) = spawn(&mut world, "item-void");
        inst.behavior.create_state(&mut world, e);
        let copper = crate::content::ItemId::new(0);
        assert!(inst.behavior.accept_item(&world, e, e, copper));
        assert_eq!(inst.behavior.accept_stack(&world, e, copper, 7, None), 7);
        world.get_mut::<Building>(e).expect("building").enabled = false;
        assert!(!inst.behavior.accept_item(&world, e, e, copper));
        assert_eq!(inst.behavior.accept_stack(&world, e, copper, 7, None), 0);
    }

    #[test]
    fn liquid_source_configure_round_trip() {
        let mut world = EcsWorld::new();
        let (inst, e) = spawn(&mut world, "liquid-source");
        inst.behavior.create_state(&mut world, e);
        let water = crate::content::LiquidId::WATER;
        inst.behavior
            .configured(&mut world, e, None, ConfigValue::Liquid(water));
        assert_eq!(inst.behavior.config(&world, e), ConfigValue::Liquid(water));
        inst.behavior
            .configured(&mut world, e, None, ConfigValue::None);
        assert_eq!(inst.behavior.config(&world, e), ConfigValue::None);
    }

    #[test]
    fn item_source_config_round_trips_through_io() {
        let mut world = EcsWorld::new();
        let (inst, e) = spawn(&mut world, "item-source");
        inst.behavior.create_state(&mut world, e);
        let silicon = crate::content::ItemId::new(3);
        inst.behavior
            .configured(&mut world, e, None, ConfigValue::Item(silicon));

        let mut bytes = Vec::new();
        {
            let mut writer = crate::io::wire::WireWriter::new(&mut bytes);
            inst.behavior.write(&world, e, &mut writer);
        }
        inst.behavior
            .configured(&mut world, e, None, ConfigValue::None);
        assert_eq!(inst.behavior.config(&world, e), ConfigValue::None);

        let mut reader = crate::io::wire::WireReader::new(&bytes);
        let revision = inst.behavior.version(&world, e);
        inst.behavior.read(&mut world, e, &mut reader, revision);
        assert_eq!(inst.behavior.config(&world, e), ConfigValue::Item(silicon));
    }

    #[test]
    fn liquid_source_config_round_trips_through_io() {
        let mut world = EcsWorld::new();
        let (inst, e) = spawn(&mut world, "liquid-source");
        inst.behavior.create_state(&mut world, e);
        let cryofluid = crate::content::LiquidId::new(1);
        inst.behavior
            .configured(&mut world, e, None, ConfigValue::Liquid(cryofluid));

        let mut bytes = Vec::new();
        {
            let mut writer = crate::io::wire::WireWriter::new(&mut bytes);
            inst.behavior.write(&world, e, &mut writer);
        }
        inst.behavior
            .configured(&mut world, e, None, ConfigValue::None);

        let mut reader = crate::io::wire::WireReader::new(&bytes);
        let revision = inst.behavior.version(&world, e);
        assert_eq!(revision, 1);
        inst.behavior.read(&mut world, e, &mut reader, revision);
        assert_eq!(
            inst.behavior.config(&world, e),
            ConfigValue::Liquid(cryofluid)
        );

        // Unconfigured sources round-trip as clear (`-1`).
        inst.behavior
            .configured(&mut world, e, None, ConfigValue::None);
        let mut bytes = Vec::new();
        {
            let mut writer = crate::io::wire::WireWriter::new(&mut bytes);
            inst.behavior.write(&world, e, &mut writer);
        }
        let mut reader = crate::io::wire::WireReader::new(&bytes);
        inst.behavior.read(&mut world, e, &mut reader, revision);
        assert_eq!(inst.behavior.config(&world, e), ConfigValue::None);
    }

    #[test]
    fn liquid_void_accepts_while_enabled_and_clears_on_placement() {
        let mut world = EcsWorld::new();
        let (inst, e) = spawn(&mut world, "liquid-void");
        inst.behavior.create_state(&mut world, e);
        let water = crate::content::LiquidId::WATER;
        if let Some(mut module) = world.get_mut::<LiquidModule>(e) {
            module.add(water, 5.0, 10.0);
        }
        inst.behavior.placed(&mut world, e);
        assert_eq!(world.get::<LiquidModule>(e).expect("module").current(), 0.0);
        assert!(inst.behavior.accept_liquid(&world, e, e, water));
        // `handle_liquid` does not store (Java's `handleFlow` is UI-only).
        inst.behavior.handle_liquid(&mut world, e, e, water, 3.0);
        assert_eq!(world.get::<LiquidModule>(e).expect("module").current(), 0.0);
    }

    #[test]
    fn power_void_lowers_an_unbounded_consumer() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table.get_named("power-void").expect("power-void");
        assert!(inst.def.consumes_power);
        let power = inst
            .consumers
            .cons_power
            .and_then(|index| inst.consumers.all.get(index))
            .expect("power consumer");
        assert!(matches!(
            power.kind,
            crate::world::consumers::ConsumeInstanceKind::Power { .. }
        ));
    }
}

// SPDX-License-Identifier: GPL-3.0-only

//! Building bars/consumption JSON for the HUD.
//! Upstream: core/src/mindustry/world/blocks/* (setBars)

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use mind_core::content::registries::blocks::BlockDef;

use super::bar_rows::{
    BarExtras, GeneratorBars, PayloadLoaderBars, PowerBars, TurretBars, UnitCapBars,
    building_bar_rows,
};
use super::*;

#[godot_api(secondary)]
impl MindSimHost {
    // ---- G03 T6 hovered-building bars (MCP/API seam) ----

    /// `Block.setBars` rows for the player-team building at `(x, y)` (G03 T6),
    /// in Java order: `[{"key","label_key","label_arg","color","value"}]`.
    /// `[]` without a player building. Label text is formatted in GDScript.
    #[func]
    pub fn building_bars_json(&self, x: i32, y: i32) -> GString {
        let Some(tile) = self.tile_at(x, y) else {
            return crate::gdutil::json::empty_array();
        };
        let Some(entity) = tile.build else {
            return crate::gdutil::json::empty_array();
        };
        // `PlacementFragment.hovered()` calls `build.display(table)` for the
        // hovered building whatever its team (Java shows enemy/derelict bars
        // too); only the *unit* test is player-team-scoped
        // (`Units.closestOverlap(player.team(), ...)`).
        let Some(content) = self.content_registry() else {
            return crate::gdutil::json::empty_array();
        };
        let Some(def) = content.block(tile.block) else {
            return crate::gdutil::json::empty_array();
        };
        let healthf = self
            .world()
            .get::<mind_core::entities::comp::health::Health>(entity)
            .map(mind_core::entities::comp::health::Health::fraction)
            .unwrap_or(1.0);
        // `Block.setBars` adds the base `power` bar only when `consPower != null`
        // (a power *consumer*); producers like generators show only their own
        // `poweroutput` row.
        let power_status = (def.consumes.iter().any(|spec| {
            matches!(
                &spec.consume,
                mind_core::content::registries::blocks::Consume::Power { .. }
            )
        }))
        .then(|| {
            self.world()
                .get::<PowerModule>(entity)
                .map(|module| module.status)
        })
        .flatten();
        let items_total = self
            .world()
            .get::<ItemModule>(entity)
            .map(|module| module.total);
        let liquids = self
            .world()
            .get::<LiquidModule>(entity)
            .map(|module| module.liquids.clone());
        let liquid_info: Vec<(String, u32)> = content
            .liquids()
            .iter()
            .map(|liquid| {
                (
                    liquid.name.clone(),
                    mind_core::render::drawf::to_bits([
                        liquid.color.r,
                        liquid.color.g,
                        liquid.color.b,
                        liquid.color.a,
                    ]),
                )
            })
            .collect();
        // `Block.setBars` family overrides read live component state (J05
        // T4): `bar_extras` collects them in `setBars` order.
        let world = self.world();
        let extras = self.bar_extras(world, content, entity, def, tile, &liquid_info);
        let rows = building_bar_rows(
            def,
            healthf,
            power_status,
            items_total,
            liquids.as_deref(),
            &liquid_info,
            &extras,
        );
        let json: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "key": row.key,
                    "label_key": row.label_key,
                    "label_args": row.label_args,
                    "color": format!("{:08x}", row.color),
                    "value": row.value,
                })
            })
            .collect();
        crate::gdutil::json::value_gstring(&serde_json::Value::Array(json))
    }

    /// `BuildingComp.displayConsumption` rows for the hovered building
    /// (`BuildingComp.java:1580,1659-1665`), shown under the bar table only for
    /// the player's team (`:1573`). Each row is one `Consume.build` element as
    /// a `MultiReqImage` list `[[{"icon","valid"}...]...]`.
    ///
    /// Ported consumer: `ItemTurret`'s ammo `ConsumeItemFilter.build`
    /// (`ItemTurret.java:72-81`): one `ReqImage(item.uiIcon)` per ammo item in
    /// content order (`filter.get(i) && i.unlockedNow()`), valid while that
    /// item is the top ammo entry (`ammo.peek()`). Other consumers' `build`
    /// rows are not ported yet; optional boosters (coolant) are skipped by
    /// Java (`cons.optional && cons.booster`).
    #[func]
    pub fn building_consumption_json(&self, x: i32, y: i32) -> GString {
        let empty = || crate::gdutil::json::empty_array();
        let Some(entity) = self.tile_at(x, y).and_then(|tile| tile.build) else {
            return empty();
        };
        let Some(content) = self.content_registry() else {
            return empty();
        };
        let world = self.world();
        let team = world
            .get::<mind_core::entities::comp::TeamComp>(entity)
            .map(|team| team.team);
        if team != Some(self.player_team()) {
            return empty();
        }
        let Some(state) =
            world.get::<mind_core::world::blocks::defense::turrets::TurretState>(entity)
        else {
            return empty();
        };
        let mind_core::world::blocks::defense::turrets::TurretAmmo::Item(ammo_types) =
            &state.config.ammo
        else {
            return empty();
        };
        let current = state.ammo.last().map(|entry| entry.item);
        let campaign = self.in_campaign();
        let images: Vec<serde_json::Value> = content
            .items()
            .iter()
            .filter(|item| ammo_types.iter().any(|ammo| ammo.item == item.id))
            .filter(|item| item.unlock.unlocked() || !campaign)
            .map(|item| {
                serde_json::json!({
                    "icon": format!("item-{}-ui", item.name),
                    "valid": current == Some(item.id),
                })
            })
            .collect();
        crate::gdutil::json::value_gstring(&serde_json::json!([images]))
    }
}

/// Family-specific live values the `Block.setBars` overrides read (J05 T4).
///
/// Split out of [`MindSimHost::building_bars_json`] one accessor per family;
/// each fills the matching [`BarExtras`] field, and `bar_extras` assigns the
/// fields in the same order the builder used to.
impl MindSimHost {
    /// Collects the [`BarExtras`] for one building.
    fn bar_extras(
        &self,
        world: &World,
        content: &ContentRegistry,
        entity: Entity,
        def: &BlockDef,
        tile: &mind_core::world::Tile,
        liquid_info: &[(String, u32)],
    ) -> BarExtras {
        let table = world.get_resource::<mind_core::world::block::BlockTable>();
        let instance = table.and_then(|table| table.instance(tile.block));
        let power_module = world.get::<PowerModule>(entity);
        let turret = world
            .get::<mind_core::world::blocks::defense::turrets::TurretState>(entity)
            .map(|state| TurretBars {
                total_ammo: state.total_ammo,
                max_ammo: state.config.max_ammo,
                heat_req: state.heat_req,
                heat_requirement: state.config.heat_requirement,
                max_heat_efficiency: state.config.max_heat_efficiency,
                activation_timer: state.activation_timer,
                activation_time: state.config.activation_time,
            });
        let power = power_module.and_then(|module| self.power_bars(world, entity, module, tile));
        let generator = Self::generator_bars(world, def, entity);
        let team = world
            .get::<mind_core::entities::comp::TeamComp>(entity)
            .map_or(0, |team| team.team);
        let factory_units = Self::factory_unit_bars(content, world, def, team, entity);
        let assembler = Self::assembler_bars(content, world, team, entity);
        let cargo_units = Self::cargo_unit_bars(content, world, team, entity);
        let campaign_state = world.get::<mind_core::entities::comp::CampaignState>(entity);
        let launch = Self::launch_bars(campaign_state, instance.as_deref());
        let variable_reactor = Self::variable_reactor_bars(world, entity);
        let overdrive = Self::overdrive_bars(world, entity);
        let shield = Self::shield_bars(world, entity);
        let pump = Self::pump_bars(world, entity);
        let target_dummy = Self::target_dummy_bars(world, entity);
        let liquids = world
            .get::<LiquidModule>(entity)
            .map(|module| module.liquids.clone());
        let output_liquids =
            Self::output_liquid_bars(instance.as_deref(), liquid_info, liquids.as_deref(), def);
        BarExtras {
            heat: world
                .get::<mind_core::world::blocks::heat::HeatState>(entity)
                .map(|state| (state.heat, state.heat_output)),
            drill: world
                .get::<mind_core::entities::comp::DrillState>(entity)
                .map(|state| (state.last_drill_speed, state.warmup)),
            // `WallCrafter.setBars` (`WallCrafter.java:68-72`) reads the build's
            // `lastEfficiency`/`warmup` and the block's `drillTime`.
            wall_crafter: match instance.as_ref().map(|instance| &instance.kind_data) {
                Some(mind_core::world::BlockKindData::WallCrafter(wall)) => world
                    .get::<mind_core::entities::comp::DrillState>(entity)
                    .map(|state| (state.last_efficiency, state.warmup, wall.drill_time)),
                _ => None,
            },
            time_scale: world
                .get::<mind_core::entities::comp::Building>(entity)
                .map(|building| building.time_scale)
                .unwrap_or(1.0),
            core_capacity: (def.kind == mind_core::content::BlockKind::CoreBlock)
                .then(|| self.core_capacity_bars(world, content, entity)),
            factory: world
                .get::<mind_core::world::blocks::units::UnitFactoryBuild>(entity)
                .map(|state| (state.progress, state.current_plan)),
            turret,
            power: power.clone(),
            generator,
            reactor_heat: world
                .get::<mind_core::world::blocks::power::behavior::NuclearReactorState>(entity)
                .map(|state| state.heat),
            variable_reactor,
            overdrive,
            shield,
            pump,
            payload_loader: matches!(
                def.kind,
                mind_core::content::BlockKind::PayloadLoader
                    | mind_core::content::BlockKind::PayloadUnloader
            )
            .then(|| Self::payload_loader_bars(world, content, entity)),
            assembler,
            factory_units,
            cargo_units,
            launch,
            landing_cooldown: campaign_state.map(|state| state.cooldown),
            accelerator: Self::accelerator_bars(power, instance.as_deref()),
            target_dummy,
            output_liquids,
        }
    }

    /// `PowerNode`/`BeamNode` power-graph bars plus the `PowerDiode` back/front
    /// graph fractions (`PowerNode.java:112-141`, `PowerDiode.java:33-52`).
    fn power_bars(
        &self,
        world: &World,
        entity: Entity,
        module: &PowerModule,
        tile: &mind_core::world::Tile,
    ) -> Option<PowerBars> {
        let graph = world
            .get_resource::<PowerGrids>()
            .and_then(|grids| grids.graph(module.graph))?;
        let diode_fraction = |dx: i32, dy: i32| {
            let target = self.tile_at(tile.x as i32 + dx, tile.y as i32 + dy)?;
            let target_entity = target.build?;
            let target_module = world.get::<PowerModule>(target_entity)?;
            let target_graph = world
                .get_resource::<PowerGrids>()
                .and_then(|grids| grids.graph(target_module.graph))?;
            let capacity = target_graph.get_total_battery_capacity(world);
            (capacity > 0.0).then(|| target_graph.get_battery_stored(world) / capacity)
        };
        let (back, front) = {
            let rotation = world
                .get::<mind_core::entities::comp::Building>(entity)
                .map(|building| building.rotation)
                .unwrap_or(0);
            let (dx, dy) = match rotation & 3 {
                0 => (0, 1),
                1 => (1, 0),
                2 => (0, -1),
                _ => (-1, 0),
            };
            (diode_fraction(-dx, -dy), diode_fraction(dx, dy))
        };
        Some(PowerBars {
            balance: graph.power_balance(),
            produced: graph.last_power_produced(),
            needed: graph.last_power_needed(),
            stored: graph.get_battery_stored(world),
            capacity: graph.get_total_battery_capacity(world),
            links: module.links.len(),
            max_nodes: world
                .get::<mind_core::world::blocks::power::PowerNodeConfig>(entity)
                .map_or(0, |config| config.max_nodes as usize),
            back,
            front,
        })
    }

    /// `PowerGenerator`/`ImpactReactor` bars (`PowerGenerator.java:88-96`,
    /// `ImpactReactor.java:47-54`).
    fn generator_bars(world: &World, def: &BlockDef, entity: Entity) -> Option<GeneratorBars> {
        world
            .get::<mind_core::world::blocks::power::generator::GeneratorConfig>(entity)
            .map(|config| GeneratorBars {
                production: config.power_production
                    * world
                        .get::<mind_core::world::blocks::power::generator::GeneratorState>(entity)
                        .map_or(0.0, |state| state.production_efficiency),
                usage: def
                    .consumes
                    .iter()
                    .find_map(|spec| match &spec.consume {
                        mind_core::content::registries::blocks::Consume::Power {
                            usage, ..
                        } => Some(*usage),
                        _ => None,
                    })
                    .unwrap_or(0.0),
                efficiency: world
                    .get::<mind_core::world::blocks::power::generator::GeneratorState>(entity)
                    .map_or(0.0, |state| state.production_efficiency),
            })
    }

    /// One factory/reconstructor/assembler/cargo-loader `units` bar
    /// (`UnitFactory.java:127-136`).
    fn unit_cap_bars(
        world: &World,
        content: &ContentRegistry,
        team: u8,
        unit: mind_core::content::UnitTypeId,
    ) -> Option<UnitCapBars> {
        let def = content.units().get(unit.index())?;
        Some(UnitCapBars {
            name: def.name.clone(),
            count: mind_core::entities::comp::unit::queries::live_type_count(world, team, unit),
            cap: mind_core::entities::comp::unit::queries::unit_cap(world, team)
                .map(|cap| cap as i32),
            use_cap: def.use_unit_cap,
            has_unit: true,
        })
    }

    /// `UnitFactory` current-plan `units` bar (`UnitFactory.java:121-136`).
    fn factory_unit_bars(
        content: &ContentRegistry,
        world: &World,
        def: &BlockDef,
        team: u8,
        entity: Entity,
    ) -> Option<UnitCapBars> {
        world
            .get::<mind_core::world::blocks::units::UnitFactoryBuild>(entity)
            .and_then(|state| {
                usize::try_from(state.current_plan)
                    .ok()
                    .and_then(|index| def.unit_plans.get(index))
                    .and_then(|plan| Self::unit_cap_bars(world, content, team, plan.unit))
            })
    }

    /// `UnitAssembler` progress + plan `units` bar
    /// (`UnitAssembler.java:128-143`).
    fn assembler_bars(
        content: &ContentRegistry,
        world: &World,
        team: u8,
        entity: Entity,
    ) -> Option<(f32, f32, Option<UnitCapBars>)> {
        world
            .get::<mind_core::world::blocks::units::UnitAssembler>(entity)
            .map(|state| {
                (
                    state.progress,
                    state.plan().map_or(0.0, |plan| plan.time),
                    state
                        .plan()
                        .and_then(|plan| Self::unit_cap_bars(world, content, team, plan.unit)),
                )
            })
    }

    /// `UnitCargoLoader` carried-unit `units` bar (`UnitCargoLoader.java:51-61`).
    fn cargo_unit_bars(
        content: &ContentRegistry,
        world: &World,
        team: u8,
        entity: Entity,
    ) -> Option<UnitCapBars> {
        world
            .get::<mind_core::world::blocks::units::CargoLoaderState>(entity)
            .and_then(|state| {
                let unit = state.unit?;
                let type_id = world
                    .get::<mind_core::entities::comp::unit::comp::UnitTypeComp>(unit)?
                    .type_id;
                Self::unit_cap_bars(world, content, team, type_id)
            })
    }

    /// `LaunchPad` launch counter/time (`LaunchPad.java:69-70`).
    fn launch_bars(
        state: Option<&mind_core::entities::comp::CampaignState>,
        instance: Option<&mind_core::world::BlockInstance>,
    ) -> Option<(f32, f32)> {
        state.and_then(|state| {
            instance.and_then(|instance| match &instance.kind_data {
                mind_core::world::BlockKindData::LaunchPad(def) => {
                    Some((state.launch_time, def.launch_time))
                }
                _ => None,
            })
        })
    }

    /// `VariableReactorBuild` instability/heat (`VariableReactor.java:57-63`).
    fn variable_reactor_bars(world: &World, entity: Entity) -> Option<(f32, f32, f32)> {
        world
            .get::<mind_core::world::blocks::power::behavior::VariableReactorState>(entity)
            .map(|state| (state.heat, 150.0, state.instability))
    }

    /// `OverdriveBuild` boost (`OverdriveProjector.java:80`).
    fn overdrive_bars(world: &World, entity: Entity) -> Option<(f32, f32)> {
        world
            .get::<mind_core::world::blocks::defense::projectors::OverdriveProjectorState>(entity)
            .map(|state| {
                let boost = state.real_boost(
                    world
                        .get::<mind_core::entities::comp::Building>(entity)
                        .map_or(0.0, |building| building.efficiency),
                );
                (boost, state.speed_boost + state.speed_boost_phase)
            })
    }

    /// `ForceBuild` shield fill (`ForceProjector.java:99`).
    fn shield_bars(world: &World, entity: Entity) -> Option<f32> {
        world
            .get::<mind_core::world::blocks::defense::shields::ForceProjectorState>(entity)
            .map(|state| {
                if state.broken {
                    0.0
                } else {
                    1.0 - state.buildup
                        / (state.shield_health + state.phase_shield_boost * state.phase_heat)
                }
            })
    }

    /// `SolidPumpBuild` pump speed (`SolidPump.java:51-55`).
    fn pump_bars(world: &World, entity: Entity) -> Option<(f32, f32, f32)> {
        world
            .get::<mind_core::entities::comp::PumpState>(entity)
            .map(|state| {
                (
                    state.amount,
                    state.warmup,
                    world
                        .get::<mind_core::entities::comp::Building>(entity)
                        .map_or(0.0, |building| building.efficiency),
                )
            })
    }

    /// `TargetDummyBuild` reset timer (`TargetDummy.java:64-73`).
    fn target_dummy_bars(world: &World, entity: Entity) -> Option<(f32, f32)> {
        world
            .get::<mind_core::world::blocks::defense::shields::TargetDummyState>(entity)
            .map(|state| (state.reset, state.reset_time))
    }

    /// `GenericCrafter.outputLiquids` output bars (`GenericCrafter.java:81-98`):
    /// `(name, rgba8888, amount, capacity)`.
    fn output_liquid_bars(
        instance: Option<&mind_core::world::BlockInstance>,
        liquid_info: &[(String, u32)],
        liquids: Option<&[f32]>,
        def: &BlockDef,
    ) -> Vec<(String, u32, f32, f32)> {
        instance
            .and_then(|instance| match &instance.kind_data {
                mind_core::world::BlockKindData::Crafter(crafter) => Some(&crafter.output_liquids),
                _ => None,
            })
            .map(|outputs| {
                outputs
                    .iter()
                    .filter_map(|(liquid, _amount)| {
                        let liquid = mind_core::content::LiquidId::new(*liquid);
                        let (name, color) = liquid_info.get(liquid.index())?;
                        Some((
                            name.clone(),
                            *color,
                            liquids
                                .and_then(|amounts| amounts.get(liquid.index()))
                                .copied()
                                .unwrap_or(0.0),
                            def.liquid_capacity,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `PayloadLoader.fraction()` (`PayloadLoader.java:219-224`): the carried
    /// build's items / liquids / battery status; 0 without a payload. The bar
    /// itself is always present (`:60-66`).
    fn payload_loader_bars(
        world: &World,
        content: &ContentRegistry,
        entity: Entity,
    ) -> PayloadLoaderBars {
        let payload_entity = world
            .get::<mind_core::world::blocks::payloads::PayloadHolder>(entity)
            .and_then(|holder| holder.payload)
            .and_then(|payload| payload.entity);
        let Some(payload_entity) = payload_entity else {
            return PayloadLoaderBars::default();
        };
        let Some(payload_def) = world
            .get::<mind_core::entities::comp::Building>(payload_entity)
            .and_then(|building| content.block(building.block))
        else {
            return PayloadLoaderBars::default();
        };
        let items = world
            .get::<ItemModule>(payload_entity)
            .map_or(0, |module| module.total);
        let progress = if payload_def.has_items {
            if payload_def.item_capacity > 0 {
                items as f32 / payload_def.item_capacity as f32
            } else {
                0.0
            }
        } else if payload_def.has_liquids {
            if payload_def.liquid_capacity > 0.0 {
                world
                    .get::<LiquidModule>(payload_entity)
                    .map_or(0.0, |module| {
                        module.current_amount / payload_def.liquid_capacity
                    })
            } else {
                0.0
            }
        } else if payload_def.has_power {
            world
                .get::<PowerModule>(payload_entity)
                .map_or(0.0, |module| module.status)
        } else {
            0.0
        };
        PayloadLoaderBars {
            progress: progress.clamp(0.0, 1.0),
            payload_has_items: payload_def.has_items,
            payload_items: items,
        }
    }

    /// `CoreBuild.storageCapacity` plus the unlocked item count
    /// (`CoreBlock.java:148-152`).
    fn core_capacity_bars(
        &self,
        world: &World,
        content: &ContentRegistry,
        entity: Entity,
    ) -> (f32, f32) {
        let unlocked = content
            .items()
            .iter()
            .filter(|item| item.unlock.unlocked() || !self.in_campaign())
            .count() as f32;
        let capacity = world
            .get::<mind_core::world::blocks::storage::CoreBuild>(entity)
            .map(|core| core.storage_capacity as f32)
            .unwrap_or(0.0);
        (capacity, unlocked)
    }

    /// `Accelerator` graph battery stored and `powerBufferRequirement`.
    fn accelerator_bars(
        power: Option<PowerBars>,
        instance: Option<&mind_core::world::BlockInstance>,
    ) -> Option<(f32, f32)> {
        power.map(|power| {
            (
                power.stored,
                instance
                    .and_then(|instance| match &instance.kind_data {
                        mind_core::world::BlockKindData::Accelerator(def) => {
                            Some(def.power_buffer_requirement)
                        }
                        _ => None,
                    })
                    .unwrap_or(0.0),
            )
        })
    }
}

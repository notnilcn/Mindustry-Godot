// SPDX-License-Identifier: GPL-3.0-only

//! Building bars/consumption JSON for the HUD.
//! Upstream: core/src/mindustry/world/blocks/* (setBars)

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
            return GString::from("[]");
        };
        let Some(entity) = tile.build else {
            return GString::from("[]");
        };
        // `PlacementFragment.hovered()` calls `build.display(table)` for the
        // hovered building whatever its team (Java shows enemy/derelict bars
        // too); only the *unit* test is player-team-scoped
        // (`Units.closestOverlap(player.team(), ...)`).
        let Some(content) = self.content_snapshot.as_ref() else {
            return GString::from("[]");
        };
        let Some(def) = content.block(tile.block) else {
            return GString::from("[]");
        };
        let healthf = self
            .sim
            .ecs
            .0
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
            self.sim
                .ecs
                .0
                .get::<PowerModule>(entity)
                .map(|module| module.status)
        })
        .flatten();
        let items_total = self
            .sim
            .ecs
            .0
            .get::<ItemModule>(entity)
            .map(|module| module.total);
        let liquids = self
            .sim
            .ecs
            .0
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
        // T4): heat, drill speed/warmup, `storageCapacity` + the unlocked item
        // count for a core, and the factory's progress.
        let world = &self.sim.ecs.0;
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
        let power = power_module.and_then(|module| {
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
        });
        let generator = world
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
            });
        let team = world
            .get::<mind_core::entities::comp::TeamComp>(entity)
            .map_or(0, |team| team.team);
        let unit_bars = |unit: mind_core::content::UnitTypeId| {
            let def = content.units().get(unit.index())?;
            Some(UnitCapBars {
                name: def.name.clone(),
                count: mind_core::entities::comp::unit::queries::live_type_count(world, team, unit),
                cap: mind_core::entities::comp::unit::queries::unit_cap(world, team)
                    .map(|cap| cap as i32),
                use_cap: def.use_unit_cap,
                has_unit: true,
            })
        };
        let factory_units = world
            .get::<mind_core::world::blocks::units::UnitFactoryBuild>(entity)
            .and_then(|state| {
                usize::try_from(state.current_plan)
                    .ok()
                    .and_then(|index| def.unit_plans.get(index))
                    .and_then(|plan| unit_bars(plan.unit))
            });
        let assembler = world
            .get::<mind_core::world::blocks::units::UnitAssembler>(entity)
            .map(|state| {
                (
                    state.progress,
                    state.plan().map_or(0.0, |plan| plan.time),
                    state.plan().and_then(|plan| unit_bars(plan.unit)),
                )
            });
        let cargo_units = world
            .get::<mind_core::world::blocks::units::CargoLoaderState>(entity)
            .and_then(|state| {
                let unit = state.unit?;
                let type_id = world
                    .get::<mind_core::entities::comp::unit::comp::UnitTypeComp>(unit)?
                    .type_id;
                unit_bars(type_id)
            });
        let campaign_state = world.get::<mind_core::entities::comp::CampaignState>(entity);
        let launch = campaign_state.and_then(|state| {
            instance
                .as_ref()
                .and_then(|instance| match &instance.kind_data {
                    mind_core::world::BlockKindData::LaunchPad(def) => {
                        Some((state.launch_time, def.launch_time))
                    }
                    _ => None,
                })
        });
        let variable_reactor = world
            .get::<mind_core::world::blocks::power::behavior::VariableReactorState>(entity)
            .map(|state| (state.heat, 150.0, state.instability));
        let overdrive = world
            .get::<mind_core::world::blocks::defense::projectors::OverdriveProjectorState>(entity)
            .map(|state| {
                let boost = state.real_boost(
                    world
                        .get::<mind_core::entities::comp::Building>(entity)
                        .map_or(0.0, |building| building.efficiency),
                );
                (boost, state.speed_boost + state.speed_boost_phase)
            });
        let shield = world
            .get::<mind_core::world::blocks::defense::shields::ForceProjectorState>(entity)
            .map(|state| {
                if state.broken {
                    0.0
                } else {
                    1.0 - state.buildup
                        / (state.shield_health + state.phase_shield_boost * state.phase_heat)
                }
            });
        let pump = world
            .get::<mind_core::entities::comp::PumpState>(entity)
            .map(|state| {
                (
                    state.amount,
                    state.warmup,
                    world
                        .get::<mind_core::entities::comp::Building>(entity)
                        .map_or(0.0, |building| building.efficiency),
                )
            });
        let target_dummy = world
            .get::<mind_core::world::blocks::defense::shields::TargetDummyState>(entity)
            .map(|state| (state.reset, state.reset_time));
        let output_liquids = instance
            .as_ref()
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
                                .as_deref()
                                .and_then(|amounts| amounts.get(liquid.index()))
                                .copied()
                                .unwrap_or(0.0),
                            def.liquid_capacity,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let extras = BarExtras {
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
            core_capacity: (def.kind == mind_core::content::BlockKind::CoreBlock).then(|| {
                let unlocked = self
                    .content_snapshot
                    .as_ref()
                    .map(|content| {
                        content
                            .items()
                            .iter()
                            .filter(|item| item.unlock.unlocked() || !self.in_campaign())
                            .count() as f32
                    })
                    .unwrap_or(0.0);
                let capacity = world
                    .get::<mind_core::world::blocks::storage::CoreBuild>(entity)
                    .map(|core| core.storage_capacity as f32)
                    .unwrap_or(0.0);
                (capacity, unlocked)
            }),
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
            .then(|| {
                // `PayloadLoader.fraction()` (`PayloadLoader.java:219-224`): the
                // carried build's items / liquids / battery status; 0 without a
                // payload. The bar itself is always present (`:60-66`).
                let payload_entity = world
                    .get::<mind_core::world::blocks::payloads::PayloadHolder>(entity)
                    .and_then(|holder| holder.payload)
                    .and_then(|payload| payload.entity);
                let Some(payload_entity) = payload_entity else {
                    return PayloadLoaderBars::default();
                };
                let Some(payload_def) = world
                    .get::<mind_core::entities::comp::Building>(payload_entity)
                    .and_then(|building| {
                        self.content_snapshot
                            .as_ref()
                            .and_then(|content| content.block(building.block))
                    })
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
            }),
            assembler,
            factory_units,
            cargo_units,
            launch,
            landing_cooldown: campaign_state.map(|state| state.cooldown),
            accelerator: power.map(|power| {
                (
                    power.stored,
                    instance
                        .as_ref()
                        .and_then(|instance| match &instance.kind_data {
                            mind_core::world::BlockKindData::Accelerator(def) => {
                                Some(def.power_buffer_requirement)
                            }
                            _ => None,
                        })
                        .unwrap_or(0.0),
                )
            }),
            target_dummy,
            output_liquids,
        };
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
        GString::from(serde_json::Value::Array(json).to_string().as_str())
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
        let empty = || GString::from("[]");
        let Some(entity) = self.tile_at(x, y).and_then(|tile| tile.build) else {
            return empty();
        };
        let Some(content) = self.content_snapshot.as_ref() else {
            return empty();
        };
        let world = &self.sim.ecs.0;
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
        GString::from(serde_json::json!([images]).to_string().as_str())
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BuilderComp.updateBuildLogic`: the per-builder construction/deconstruction
//! progression, bridge relinking and the construct-finish FX.
//!
//! Upstream: core/src/mindustry/entities/comp/BuilderComp.java (updateBuildLogic);
//! core/src/mindustry/world/blocks/ConstructBlock.java (constructFinish).

use super::checks::{
    BuildRuleView, building_team, should_skip, team_core, tile_world, validate_plans, within,
};
use super::*;

/// Result of one builder pass.
#[derive(Default)]
pub(super) struct BuilderOutcome {
    /// The grid changed (construct begun/finished/removed or rotated).
    changed: bool,
    events: Vec<SimEvent>,
    /// `Trigger.derelictBreak` (G05 T4b): the player deconstructed a team-0
    /// (derelict) building.
    derelict_break: bool,
}

/// `ItemBridge.playerPlaced` (`ItemBridge.java:224-233`; T11): the previously
/// placed bridge of the same block links itself to the newly built bridge
/// (`findLink` requires the old bridge to be still unlinked, in range and not
/// already in the new bridge's proximity).
///
/// `LiquidBridge extends ItemBridge` (`LiquidBridge.java:7,19`) and inherits
/// `playerPlaced`/`findLink`; the port's liquid bridge behavior provides the
/// matching `link_valid_between`/`configured`.
pub(super) fn chain_bridge(
    world: &mut World,
    grid: &WorldGrid,
    runtime: &SimRuntime,
    placed: Entity,
    plans: &mut BuilderPlans,
    block: BlockId,
    pos: TilePos,
) {
    let Some(def) = runtime.content().block(block) else {
        return;
    };
    if !matches!(
        def.kind,
        BlockKind::ItemBridge | BlockKind::BufferedItemBridge | BlockKind::LiquidBridge
    ) {
        return;
    }
    let previous = plans.last_bridge.filter(|(last_block, last_pos)| {
        *last_block == block
            && *last_pos != pos
            && grid.entity_at(*last_pos).is_some_and(|entity| {
                world
                    .get::<ItemBridgeBuild>(entity)
                    .is_some_and(|bridge| bridge.link == -1)
            })
    });
    if let Some((_, last_pos)) = previous
        && let Some(last_entity) = grid.entity_at(last_pos)
        && let Some(table) = world.get_resource::<BlockTable>()
        && let Some(inst) = table.instance(block)
        && inst.behavior.link_valid_between(world, placed, last_entity)
        && !world
            .get::<Building>(placed)
            .is_some_and(|building| building.proximity.contains(&last_entity))
    {
        // `link.build.configure(tile.pos())`: the old bridge points at the new
        // one (packed absolute position, the `Number` config path).
        inst.behavior.configured(
            world,
            last_entity,
            None,
            crate::world::config::ConfigValue::Number(f64::from(pos.pack())),
        );
    }
    plans.last_bridge = Some((block, pos));
}

/// `BuilderComp.updateBuildLogic` for one builder unit.
pub(super) fn update_builder(
    world: &mut World,
    grid: &mut WorldGrid,
    runtime: &mut SimRuntime,
    unit: Entity,
    plans: &mut BuilderPlans,
) -> BuilderOutcome {
    let mut out = BuilderOutcome::default();
    let Some(type_id) = world.get::<UnitTypeComp>(unit).map(|comp| comp.type_id) else {
        return out;
    };
    let Some((build_speed, build_range)) = runtime
        .content()
        .unit(type_id)
        .map(|def| (def.build_speed, def.build_range))
    else {
        return out;
    };
    if build_speed <= 0.0 {
        return out;
    }
    validate_plans(plans, world, grid, runtime.content());
    if !plans.update_building {
        // `BuilderComp.updateBuildLogic`: `if(!updateBuilding) return;`.
        return out;
    }

    let speed_multiplier = world
        .get::<StatusComp>(unit)
        .map(|status| status.build_speed_multiplier)
        .unwrap_or(1.0);
    if speed_multiplier <= 0.0 {
        return out;
    }
    let team = building_team(world, unit);
    let rules = BuildRuleView::read(world, team);
    let place_dst = if rules.state_infinite {
        f32::MAX.sqrt()
    } else {
        build_range
    };

    plans.build_counter += 1.0;
    if !plans.build_counter.is_finite() {
        plans.build_counter = 0.0;
    }
    plans.build_counter = plans.build_counter.min(10.0);

    let instant = rules.instant_build && rules.state_infinite;
    let max_per_frame = if instant { plans.plans.len() } else { 10 };
    let mut count = 0usize;

    let core = team_core(world, team);
    if core.is_none() && !rules.economy.infinite {
        return out;
    }
    let Some((ux, uy)) = world.get::<Pos>(unit).map(|pos| (pos.x, pos.y)) else {
        return out;
    };

    while (plans.build_counter >= 1.0 || instant)
        && count < max_per_frame
        && !plans.plans.is_empty()
    {
        count += 1;
        plans.build_counter -= 1.0;

        // Find the next build plan: rotate the queue to the first in-range
        // plan the core can fund (`shouldSkip`).
        if plans.plans.len() > 1 {
            let size = plans.plans.len();
            let mut total = 0usize;
            let mut best_dst = f32::MAX;
            let mut found_any = false;
            let mut best_index: Option<usize> = None;
            while total < size {
                let Some(plan) = plans.plans.front().cloned() else {
                    break;
                };
                let (tx, ty) = tile_world(plan.tile());
                let dst = (tx - ux) * (tx - ux) + (ty - uy) * (ty - uy);
                let in_range = dst <= place_dst * place_dst;
                let core_items = core.and_then(|core| world.get::<ItemModule>(core));
                if in_range && !should_skip(&plan, core_items, runtime.content(), &rules) {
                    found_any = true;
                    break;
                } else if in_range && dst < best_dst {
                    best_index = Some(total);
                    best_dst = dst;
                }
                plans.plans.rotate_left(1);
                total += 1;
            }
            if !found_any
                && let Some(best_index) = best_index.filter(|index| *index > 0)
                && let Some(head) = plans.plans.front()
                && !within(ux, uy, head.tile(), place_dst)
            {
                plans.plans.rotate_left(best_index);
            }
        }

        let Some(mut current) = plans.plans.front().cloned() else {
            break;
        };
        let pos = current.tile();
        if !within(ux, uy, pos, place_dst) {
            continue;
        }

        let allow_current = current.breaking || runtime.placement_allowed(current.block);
        let construct_at = |world: &World, grid: &WorldGrid| {
            grid.entity_at(pos)
                .filter(|entity| world.get::<ConstructState>(*entity).is_some())
        };

        match construct_at(world, grid) {
            None => {
                if !current.initialized && !current.breaking && allow_current {
                    // Quick rotation of an existing same block
                    // (`Build.beginPlace` auto-rotate branch).
                    if let Some(entity) = grid.entity_at(pos)
                        && grid.block_at(pos) == Some(current.block)
                        && building_team(world, entity) == team
                        && runtime
                            .content()
                            .block(current.block)
                            .is_some_and(|def| def.quick_rotate)
                    {
                        if let Some(mut building) = world.get_mut::<Building>(entity) {
                            building.rotation = current.rotation;
                        }
                        if let Some(mut comp) = world.get_mut::<BuildingComp>(entity) {
                            comp.rot = current.rotation;
                        }
                        runtime.refresh_proximity(world, grid, pos.x(), pos.y());
                        crate::world::update::no_sleep(world, entity);
                        plans.plans.pop_front();
                        out.changed = true;
                        continue;
                    }
                    if !runtime.can_place(
                        world,
                        grid,
                        pos.x(),
                        pos.y(),
                        current.block,
                        current.rotation,
                        team,
                        crate::world::build::PlaceFlags::PLAYER,
                    ) {
                        plans.plans.pop_front();
                        continue;
                    }
                    // `Build.checkNoUnitOverlap`: a ground unit standing on the
                    // footprint rotates the plan to the back of the queue.
                    if !no_unit_overlap(world, runtime.content(), pos, current.block) {
                        plans.plans.rotate_left(1);
                        out.changed = true;
                        continue;
                    }
                    let has_all = rules.economy.infinite
                        || runtime.content().block(current.block).is_some_and(|def| {
                            let items = core.and_then(|core| world.get::<ItemModule>(core));
                            def.requirements.iter().all(|stack| {
                                let need = ((stack.amount as f32 * rules.economy.cost_multiplier)
                                    .round() as i32)
                                    .min(1);
                                items.is_some_and(|items| items.get(stack.item) >= need)
                            })
                        });
                    if has_all {
                        if runtime.begin_place(
                            world,
                            grid,
                            pos.x(),
                            pos.y(),
                            current.block,
                            current.rotation,
                            team,
                            crate::world::build::PlaceFlags::PLAYER,
                            rules.economy.cost_multiplier,
                        ) {
                            // `BuilderComp` passes `current.config` into the
                            // construct (`Call.beginPlace` L173 / `construct`
                            // L217); `ConstructBuild.construct` stores it as
                            // `lastConfig` (`ConstructBlock.java:278-288`).
                            if !current.breaking
                                && let Some(construct) = grid.entity_at(pos)
                                && let Some(mut state) = world.get_mut::<ConstructState>(construct)
                            {
                                state.last_config = current.config.clone();
                            }
                            out.changed = true;
                        }
                    } else {
                        current.stuck = true;
                        if let Some(head) = plans.plans.front_mut() {
                            head.stuck = true;
                        }
                    }
                } else if !current.initialized
                    && current.breaking
                    && grid.entity_at(pos).is_some_and(|entity| {
                        let owner = building_team(world, entity);
                        owner == team || owner == DERELICT
                    })
                {
                    if runtime.begin_break(
                        world,
                        grid,
                        pos.x(),
                        pos.y(),
                        team,
                        rules.economy.cost_multiplier,
                    ) {
                        out.changed = true;
                    } else {
                        plans.plans.pop_front();
                        continue;
                    }
                } else {
                    plans.plans.pop_front();
                    continue;
                }
            }
            Some(entity) => {
                let owner = building_team(world, entity);
                let state_current = world
                    .get::<ConstructState>(entity)
                    .map(|state| state.current);
                let anchor = world.get::<Building>(entity).map(|b| b.tile);
                if (owner != team && owner != DERELICT)
                    || (!current.breaking
                        && (state_current != Some(current.block) || anchor != Some(pos)))
                {
                    plans.plans.pop_front();
                    continue;
                }
            }
        }

        let Some(entity) = construct_at(world, grid) else {
            continue;
        };
        if !current.initialized {
            current.initialized = true;
        }
        let Some(mut state) = world.get::<ConstructState>(entity).cloned() else {
            continue;
        };
        let requirements = runtime
            .content()
            .block(state.current)
            .map(|def| def.requirements.clone())
            .unwrap_or_default();
        let cost = if state.build_cost > 0.0 {
            state.build_cost
        } else {
            1.0
        };
        let bs = 1.0 / cost * build_speed * speed_multiplier * rules.build_speed;
        let capacity = core
            .and_then(|core| world.get::<CoreBuild>(core))
            .map(|build| build.storage_capacity)
            .unwrap_or(0);

        let finished = {
            let mut module = core.and_then(|core| world.get_mut::<ItemModule>(core));
            let inventory = module
                .as_deref_mut()
                .map(|items| CoreInventory { items, capacity });
            if current.breaking {
                state.deconstruct_step(
                    &requirements,
                    inventory,
                    bs,
                    &rules.economy,
                    rules.state_infinite,
                )
            } else if allow_current {
                state.construct_step(
                    &requirements,
                    inventory,
                    bs,
                    &rules.economy,
                    rules.state_infinite,
                )
            } else {
                false
            }
        };
        current.stuck = (current.progress - state.progress).abs() < f32::EPSILON;
        current.progress = state.progress;
        let anchor = world.get::<Building>(entity).map(|b| b.tile).unwrap_or(pos);
        let rotation = world
            .get::<Building>(entity)
            .map(|b| b.rotation)
            .unwrap_or(current.rotation);
        if !current.breaking {
            // `BuilderComp.java:217`: the builder passes its plan config on
            // every `construct` step; `ConstructBuild.construct` stores it as
            // `lastConfig` (`ConstructBlock.java:288`).
            state.last_config = current.config.clone();
        }
        world.entity_mut(entity).insert(state.clone());
        if let Some(head) = plans.plans.front_mut() {
            *head = current.clone();
        }

        if finished {
            if current.breaking {
                // `ConstructBlock.deconstructFinish` -> `tile.remove()`.
                // `Trigger.derelictBreak` (G05 T4b): the removed building's
                // team, read before removal.
                let removed_team = grid
                    .entity_at(anchor)
                    .and_then(|entity| world.get::<BuildingComp>(entity))
                    .map(|comp| comp.team.0);
                if runtime
                    .remove_block_raw(world, grid, anchor.x(), anchor.y())
                    .is_some()
                {
                    if removed_team == Some(DERELICT) {
                        out.derelict_break = true;
                    }
                    emit_construct_fx(
                        world,
                        runtime,
                        state.current,
                        (anchor.x() as i32, anchor.y() as i32),
                        true,
                    );
                    out.events
                        .push(SimEvent::BlockBrokenEvent(BlockBrokenEvent {
                            x: anchor.x(),
                            y: anchor.y(),
                            block: state.current,
                            // `deconstructFinish` (break sound).
                            destroyed: false,
                            team,
                        }));
                }
            } else {
                // `ConstructBlock.constructFinish` -> `tile.setBlock(block, team, rotation)`
                // then `tile.build.placed()` (`ConstructBlock.java:151-156`).
                runtime.set_block_raw(
                    world,
                    grid,
                    anchor.x(),
                    anchor.y(),
                    state.current,
                    rotation,
                    team,
                );
                let placed = grid.entity_at(anchor);
                if let Some(placed) = placed {
                    let grid_ref: &WorldGrid = grid;
                    runtime.placed_hooks(world, grid_ref, placed);
                    // `ConstructBlock.constructFinish` (`ConstructBlock.java:72-118`):
                    // after `tile.setBlock`, a non-null config is applied.
                    if !matches!(current.config, crate::world::config::ConfigValue::None) {
                        let _ = crate::world::config::configure(
                            world,
                            placed,
                            Some(unit),
                            current.config.clone(),
                        );
                    }
                    chain_bridge(
                        world,
                        grid_ref,
                        runtime,
                        placed,
                        plans,
                        state.current,
                        anchor,
                    );
                }
                emit_construct_fx(
                    world,
                    runtime,
                    state.current,
                    (anchor.x() as i32, anchor.y() as i32),
                    false,
                );
                // `ConstructBlock.constructFinish` calls
                // `block.placeEnded(tile, builder, rotation, config)` after the
                // place effect and before `BlockBuildEndEvent`
                // (`ConstructBlock.java:118`). GAPS #62's call site:
                // `Turret.placeEnded` resets the turret rotation and mod blocks
                // use the hook for post-place state.
                if let Some(placed) = placed {
                    let instance = world
                        .get_resource::<BlockTable>()
                        .and_then(|table| table.instance(state.current));
                    if let Some(inst) = instance {
                        inst.place_ended(world, placed, Some(unit), rotation, &current.config);
                    }
                }
                out.events
                    .push(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                        x: anchor.x(),
                        y: anchor.y(),
                        block: state.current,
                        team,
                    }));
                // `Logic` `BlockBuildEndEvent` -> `checkOverlappingPlans`
                // (`Logic.java:53-58`, `255-266`).
                check_overlapping_ghosts(
                    world,
                    runtime.content(),
                    team,
                    anchor.x() as i32,
                    anchor.y() as i32,
                    state.current,
                );
            }
            out.changed = true;
        }
    }
    out
}

impl Sim {
    /// Runs `BuilderComp.updateBuildLogic` for every builder unit with plans
    /// (today the player unit). Called once per tick from [`Sim::tick`].
    pub(crate) fn update_builders(&mut self) {
        // `Groups.unit` (G01 T10c): every unit with a non-empty plan queue
        // advances its own `BuilderComp`; iteration is `EntitySeq` order and
        // the player unit is not special-cased (its events additionally feed
        // the hint sets).
        let player = self
            .player_unit
            .filter(|unit| self.ecs.0.get_entity(*unit).is_ok());
        let mut units: Vec<Entity> = self
            .ecs
            .0
            .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
            .map(|runtime| runtime.units.clone())
            .unwrap_or_default();
        units.sort_unstable_by_key(|entity| {
            self.ecs
                .0
                .get::<crate::ecs::EntitySeq>(*entity)
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX)
        });
        for unit in units {
            if self.ecs.0.get_entity(unit).is_err() {
                continue;
            }
            let has_plans = self
                .ecs
                .0
                .get::<BuilderPlans>(unit)
                .is_some_and(|plans| !plans.plans.is_empty());
            if !has_plans {
                continue;
            }
            let Some(mut plans) = self
                .ecs
                .0
                .get_mut::<BuilderPlans>(unit)
                .map(|mut plans| std::mem::take(&mut *plans))
            else {
                continue;
            };
            let Some(mut runtime) = self.block_runtime.take() else {
                if self.player_unit == Some(unit) {
                    self.with_player_plans(unit, |slot| *slot = plans);
                } else if let Some(mut stored) = self.ecs.0.get_mut::<BuilderPlans>(unit) {
                    *stored = plans;
                }
                return;
            };
            let outcome = update_builder(
                &mut self.ecs.0,
                &mut self.grid,
                &mut runtime,
                unit,
                &mut plans,
            );
            self.block_runtime = Some(runtime);
            if self.player_unit == Some(unit) {
                self.with_player_plans(unit, |slot| *slot = plans);
            } else if let Some(mut stored) = self.ecs.0.get_mut::<BuilderPlans>(unit) {
                *stored = plans;
            }
            let is_player = player == Some(unit);
            for event in outcome.events {
                // `HintsFragment.java:74-102`: placed/break events feed the hint
                // sets before the event is dispatched (player unit only).
                if is_player {
                    match &event {
                        SimEvent::BlockPlacedEvent(placed) => {
                            self.hint_events.placed_by_player.insert(placed.block);
                        }
                        SimEvent::BlockBrokenEvent(_) => {
                            self.hint_events.events.insert("break");
                        }
                        _ => {}
                    }
                }
                self.push_event(event);
            }
            // `Trigger.derelictBreak` (G05 T4b): the player finished
            // deconstructing a team-0 (derelict) building.
            if outcome.derelict_break {
                self.hint_events.events.insert("derelictbreak");
            }
            if outcome.changed {
                self.refresh_unit_runtime();
            }
        }
    }
}

/// `ConstructBlock.constructFinish`/`deconstructFinish` FX (`ConstructBlock
/// .java:59-116`): `block.placeEffect`/`breakEffect` at the tile's draw centre
/// with the block size as the rotation. Fog is not ported, so the visibility
/// gate is skipped (documented).
pub(super) fn emit_construct_fx(
    world: &World,
    runtime: &crate::sim::runtime::SimRuntime,
    block: BlockId,
    anchor: (i32, i32),
    active_deconstruct: bool,
) {
    use crate::content::EffectId;
    use crate::content::registries::fx_meta::EffectRef;
    let Some(def) = runtime.content().block(block) else {
        return;
    };
    let Some(unit_runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>() else {
        return;
    };
    let cx = anchor.0 as f32 * 8.0 + def.offset;
    let cy = anchor.1 as f32 * 8.0 + def.offset;
    let (id, color) = if active_deconstruct {
        (
            EffectId::BREAK_BLOCK,
            def.map_color.unwrap_or(crate::content::Rgba::WHITE),
        )
    } else {
        (EffectId::PLACE_BLOCK, crate::content::Rgba::WHITE)
    };
    unit_runtime
        .fx
        .effect(&EffectRef::Named(id), cx, cy, def.size as f32, color);
}

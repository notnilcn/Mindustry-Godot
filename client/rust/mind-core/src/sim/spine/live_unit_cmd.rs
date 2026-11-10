// SPDX-License-Identifier: GPL-3.0-only

//! `SimCommand` unit-command/stance apply for live units (`CommandAI`).
//!
//! Upstream: core/src/mindustry/ai/types/CommandAI.java,
//! core/src/mindustry/input/InputHandler.java.

use crate::sim::*;

impl Sim {
    /// Applies a wire `UnitCommand` to **live** units (`id` = `EntitySeq`).
    ///
    /// Returns the number of units ordered; `0` routes the command to the
    /// relay runtime (the net/test id space). `command == 0` is the emitter's
    /// `commandUnits` marker: the wire target resolves through
    /// [`resolve_wire_target`] (a packed unit id in `x`, else a world-pixel
    /// position) and lands on the member's move target
    /// (`ControllerSlot.target`). Only default-team units whose controller is
    /// `CommandAI` accept (`isCommandable`) and only commands the unit type
    /// allows (`CommandAI.command`, CommandAI.java:57-64). `queue` appends to
    /// `CommandQueueComp` when a target already exists
    /// (`commandUnits(..., queueCommand)`; I01 T13).
    pub(in crate::sim) fn apply_live_unit_command(
        &mut self,
        ids: &[i32],
        command: u16,
        x: f32,
        y: f32,
        queue: bool,
    ) -> usize {
        let command_id = crate::content::UnitCommandId::new(command);
        let Some(runtime) = self
            .ecs
            .0
            .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        else {
            return 0;
        };
        if runtime.units.is_empty() {
            return 0;
        }
        // `CommandAI.command` ignores ids that are not registered.
        if runtime.content.unit_command(command_id).is_none() {
            return 0;
        }
        let is_move = command == MOVE_COMMAND.raw();
        // `setUnitCommand` carries the `(0, 0)` wire pair (`command_emit.rs`):
        // a command switch must not create a move target. A real move to world
        // (0, 0) is indistinguishable on this wire — accepted deviation.
        let set_only = is_move && x == 0.0 && y == 0.0;
        let default_team = self.live_default_team();
        let target = resolve_wire_target(x, y);
        // `CommandAI.commandTarget` (G01 T16): Unit/Building wire targets keep
        // the entity so the order follows moving targets.
        let attack_entity = match &target {
            crate::input::action::CommandTarget::Unit(target_id) => runtime
                .units
                .iter()
                .find(|&&entity| {
                    self.ecs
                        .0
                        .get::<crate::ecs::EntitySeq>(entity)
                        .is_some_and(|seq| seq.0 as i32 == *target_id)
                })
                .copied(),
            crate::input::action::CommandTarget::Building(pos) => self
                .grid
                .tiles
                .getn(pos.x() as i32, pos.y() as i32)
                .and_then(|tile| tile.build),
            crate::input::action::CommandTarget::Position { .. } => None,
        };
        let tile = match target {
            // `World.toTile` (rounded): tile `t` spans `t*8 ± 4` world px.
            crate::input::action::CommandTarget::Position { x, y } => Some(TilePos::new(
                crate::world::WorldGrid::to_tile(x) as i16,
                crate::world::WorldGrid::to_tile(y) as i16,
            )),
            crate::input::action::CommandTarget::Unit(target_id) => runtime
                .units
                .iter()
                .find(|&&entity| {
                    self.ecs
                        .0
                        .get::<crate::ecs::EntitySeq>(entity)
                        .is_some_and(|seq| seq.0 as i32 == target_id)
                })
                .and_then(|&entity| self.ecs.0.get::<crate::entities::comp::Pos>(entity))
                .map(|pos| {
                    TilePos::new(
                        crate::world::WorldGrid::to_tile(pos.x) as i16,
                        crate::world::WorldGrid::to_tile(pos.y) as i16,
                    )
                }),
            crate::input::action::CommandTarget::Building(pos) => Some(pos),
        };
        let mut orders: Vec<(Entity, Option<TilePos>, Option<Entity>)> = Vec::new();
        for &id in ids {
            let Some(&entity) = runtime.units.iter().find(|&&entity| {
                self.ecs
                    .0
                    .get::<crate::ecs::EntitySeq>(entity)
                    .is_some_and(|seq| seq.0 as i32 == id)
            }) else {
                continue;
            };
            let team = self
                .ecs
                .0
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|team| team.team)
                .unwrap_or(0);
            // `unit.isCommandable()` (`controller instanceof CommandAI`): the
            // player's own unit and AI-controlled units ignore orders.
            let commandable = self
                .ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(entity)
                .is_some_and(|slot| slot.kind == crate::ai::controller::AiKind::Command);
            if team != default_team || !commandable {
                continue;
            }
            // `CommandAI.command`: only unit-typed commands are accepted.
            let allowed = self
                .ecs
                .0
                .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .is_some_and(|def| {
                    crate::ai::unit_command_runtime::allows_command(def, command_id)
                });
            if !allowed {
                continue;
            }
            // Move orders need a resolvable target; other commands only switch
            // the active command. A `(0, 0)` wire pair is a pure command
            // switch (`setUnitCommand`), not a move to world (0, 0).
            let order = if is_move && !set_only {
                let Some(tile) = tile else {
                    continue;
                };
                Some(tile)
            } else {
                None
            };
            orders.push((entity, order, attack_entity));
        }
        let mut ordered = 0usize;
        for (entity, order, attack_entity) in orders {
            // `CommandAI.command`: `this.command = command` (CommandAI.java:62);
            // the move target is cleared when switching away from move, and
            // `unit.mineTile`/build plans are dropped (CommandAI.java:59-63).
            self.ecs
                .0
                .entity_mut(entity)
                .insert(crate::ai::controller::UnitCommandComp(command));
            crate::entities::comp::unit::miner::clear_mine_tile(&mut self.ecs.0, entity);
            // `commandUnits(..., queueCommand)` (`InputHandler.java:340-352`):
            // a queued order appends when a target already exists
            // (`CommandAI.commandQueue`); a direct order clears the queue and
            // replaces the target.
            let current_target = self
                .ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(entity)
                .and_then(|slot| slot.target);
            let current_attack = self
                .ecs
                .0
                .get::<crate::ai::types::command::AttackTargetComp>(entity)
                .and_then(|comp| comp.0);
            let apply_direct = !queue || (current_target.is_none() && current_attack.is_none());
            if apply_direct {
                self.ecs
                    .0
                    .entity_mut(entity)
                    .insert(crate::ai::types::command::AttackTargetComp(attack_entity));
                if let Some(mut queued) = self
                    .ecs
                    .0
                    .get_mut::<crate::ai::types::command::CommandQueueComp>(entity)
                {
                    queued.0.clear();
                }
                if let Some(mut slot) = self
                    .ecs
                    .0
                    .get_mut::<crate::ai::controller::ControllerSlot>(entity)
                {
                    slot.command = Some(command_id);
                    slot.target = order;
                    ordered += 1;
                }
            } else {
                // Queued: `CommandAI.commandQueue(location)` — append only when
                // the entry is not already present.
                let entry = match (order, attack_entity) {
                    (_, Some(target)) => {
                        if self
                            .ecs
                            .0
                            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(target)
                            .is_some()
                        {
                            self.ecs.0.get::<crate::ecs::EntitySeq>(target).map(|seq| {
                                crate::ai::types::command::CommandQueueEntry::Unit(seq.0 as i32)
                            })
                        } else {
                            self.ecs
                                .0
                                .get::<crate::entities::comp::Building>(target)
                                .map(|building| {
                                    crate::ai::types::command::CommandQueueEntry::Building(
                                        building.tile.pack(),
                                    )
                                })
                        }
                    }
                    (Some(tile), None) => {
                        Some(crate::ai::types::command::CommandQueueEntry::Position(
                            crate::world::WorldGrid::unconv(tile.x() as f32),
                            crate::world::WorldGrid::unconv(tile.y() as f32),
                        ))
                    }
                    (None, None) => None,
                };
                if let Some(entry) = entry {
                    let mut queued = self
                        .ecs
                        .0
                        .get_mut::<crate::ai::types::command::CommandQueueComp>(entity)
                        .map(|queued| queued.0.clone())
                        .unwrap_or_default();
                    if !queued.contains(&entry)
                        && queued.len() < crate::ai::types::command::MAX_COMMAND_QUEUE
                    {
                        queued.push(entry);
                        self.ecs
                            .0
                            .entity_mut(entity)
                            .insert(crate::ai::types::command::CommandQueueComp(queued));
                        ordered += 1;
                    }
                }
            }
        }
        ordered
    }

    /// Applies a wire `UnitStance` to **live** units (`id` = `EntitySeq`),
    /// mirroring [`Self::apply_live_unit_command`]'s filters: default team,
    /// `CommandAI` controller and a stance the unit type lists
    /// (`UnitType.getUnitStances`). Returns the number of units whose live
    /// stance mask was touched (`0` routes the command to the relay).
    pub(in crate::sim) fn apply_live_unit_stance(
        &mut self,
        ids: &[i32],
        stance: u16,
        enabled: bool,
    ) -> usize {
        let stance_id = crate::content::id::UnitStanceId::new(stance);
        let Some(runtime) = self
            .ecs
            .0
            .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        else {
            return 0;
        };
        if runtime.units.is_empty() || runtime.content.unit_stance(stance_id).is_none() {
            return 0;
        }
        let default_team = self.live_default_team();
        let mut accepted: Vec<Entity> = Vec::new();
        for &id in ids {
            let Some(&entity) = runtime.units.iter().find(|&&entity| {
                self.ecs
                    .0
                    .get::<crate::ecs::EntitySeq>(entity)
                    .is_some_and(|seq| seq.0 as i32 == id)
            }) else {
                continue;
            };
            let team = self
                .ecs
                .0
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|team| team.team)
                .unwrap_or(0);
            let commandable = self
                .ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(entity)
                .is_some_and(|slot| slot.kind == crate::ai::controller::AiKind::Command);
            if team != default_team || !commandable {
                continue;
            }
            let allowed = self
                .ecs
                .0
                .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .is_some_and(|def| {
                    let mut stances = Vec::new();
                    crate::ai::unit_command_runtime::get_unit_stances(
                        def,
                        &runtime.content,
                        &mut stances,
                    );
                    stances.contains(&stance_id)
                });
            if allowed {
                accepted.push(entity);
            }
        }
        let stance_def = runtime.content.unit_stance(stance_id).cloned();
        let Some(stance_def) = stance_def else {
            return 0;
        };
        let mut applied = 0usize;
        for entity in accepted {
            let mut comp = self
                .ecs
                .0
                .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(entity)
                .copied()
                .unwrap_or_default();
            if enabled {
                crate::ai::unit_stance_runtime::apply_stance(&mut comp.0, &stance_def);
            } else {
                crate::ai::unit_stance_runtime::disable_stance(&mut comp.0, stance_id);
            }
            self.ecs.0.entity_mut(entity).insert(comp);
            applied += 1;
        }
        applied
    }

    /// Live command targets for the HUD reticle (G01 T16):
    /// `(seq, world_x, world_y, is_attack)`. Move orders report their tile
    /// centre; attack orders report the tracked entity's position.
    pub fn command_targets(&self) -> Vec<(i32, f32, f32, bool)> {
        let Some(runtime) = self
            .ecs
            .0
            .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &entity in &runtime.units {
            let Some(seq) = self.ecs.0.get::<crate::ecs::EntitySeq>(entity) else {
                continue;
            };
            let attack = self
                .ecs
                .0
                .get::<crate::ai::types::command::AttackTargetComp>(entity)
                .and_then(|comp| comp.0);
            let move_tile = self
                .ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(entity)
                .and_then(|slot| slot.target);
            if attack.is_none() && move_tile.is_none() {
                continue;
            }
            let pos = attack
                .and_then(|target| self.ecs.0.get::<crate::entities::comp::Pos>(target))
                .map(|pos| (pos.x, pos.y))
                .or_else(|| {
                    move_tile.map(|tile| {
                        (
                            // `Tile.worldx()/worldy()` (Tile.java:166-171).
                            tile.x() as f32 * crate::config::TILESIZE as f32,
                            tile.y() as f32 * crate::config::TILESIZE as f32,
                        )
                    })
                })
                .unwrap_or((0.0, 0.0));
            out.push((seq.0 as i32, pos.0, pos.1, attack.is_some()));
        }
        out
    }

    /// Drains the per-tick `BuildDamageEvent` log (G01 T15): one entry per
    /// building hit by a bullet (direct or bullet-sourced splash).
    pub fn drain_build_damage_events(&mut self) -> Vec<crate::world::events::BuildDamageEvent> {
        self.ecs
            .0
            .get_resource_mut::<crate::world::events::BuildDamageLog>()
            .map(|mut log| std::mem::take(&mut log.0))
            .unwrap_or_default()
    }

    /// Drains the per-tick `WallFlashEvent` log (I02 T4): one entry per bullet
    /// collision against a `flashHit` wall (`Wall.WallBuild.collision`,
    /// `Wall.java:164-166`); the renderer owns the `hit` decay.
    pub fn drain_wall_flashes(&mut self) -> Vec<(i16, i16)> {
        self.ecs
            .0
            .get_resource_mut::<crate::world::events::WallFlashLog>()
            .map(|mut log| {
                std::mem::take(&mut log.0)
                    .into_iter()
                    .map(|event| (event.x, event.y))
                    .collect()
            })
            .unwrap_or_default()
    }
}

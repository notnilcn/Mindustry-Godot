// SPDX-License-Identifier: GPL-3.0-only

//! `Sim::command`: the canonical `SimCommand` dispatch.
//!
//! Upstream: none (port-only command dispatch).

use crate::sim::*;

impl Sim {
    /// Applies a canonical [`SimCommand`] through the same path the relay uses
    /// (plan 05 §6.4). Unsupported/unknown commands are structured errors.
    pub fn command(
        &mut self,
        command: crate::determinism::SimCommand,
    ) -> Result<(), crate::determinism::CommandError> {
        use crate::determinism::{CommandError, SimCommand};
        match &command {
            SimCommand::UnitCommand {
                units,
                command: unit_command,
                x,
                y,
                queue,
            } => {
                // Live units first (ids are `EntitySeq`; PAR-0140); the relay
                // runtime keeps the wire-only/net bookkeeping path.
                if self.apply_live_unit_command(units, *unit_command, *x, *y, *queue) == 0 {
                    self.ensure_unit_commands().apply_unit_command(
                        units,
                        *unit_command,
                        *x,
                        *y,
                        *queue,
                    );
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::UnitControl { unit } => {
                // `InputHandler.unitControl` (`InputHandler.java:775-822`):
                // possess an own unit, or clear the possession.
                match unit {
                    Some(seq) => {
                        self.possess_unit(*seq);
                    }
                    None => {
                        self.clear_unit();
                    }
                }
                self.controlled_unit = *unit;
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::UnitClear => {
                self.clear_unit();
                self.controlled_unit = None;
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::SpawnUnit { unit, x, y, team } => {
                // Host/harness spawn: only the live unit runtime can carry it
                // (`spawn_unit_def` inserts the full unit component closure).
                if self.has_unit_runtime() {
                    let ecs = &mut self.ecs.0;
                    let Some(mut runtime) = ecs.remove_non_send::<unit_runtime::UnitRuntime>()
                    else {
                        return Err(CommandError::Unsupported("spawn_unit"));
                    };
                    let id = crate::content::UnitTypeId::new(*unit);
                    let entity = runtime.spawn_by_id(ecs, id, *team, *x, *y, 0.0);
                    ecs.insert_non_send(runtime);
                    if entity.is_none() {
                        return Err(CommandError::UnknownContent(*unit));
                    }
                    self.commands_applied = self.commands_applied.wrapping_add(1);
                    return Ok(());
                }
            }
            SimCommand::BuildingControlSelect { x, y } => {
                let pos = TilePos::new(*x, *y);
                self.control_building = Some(pos);
                // `InputHandler.unitBuildingControlSelect(unit, build)`
                // (`InputHandler.java:1016-1031`) -> `CoreBlock.onControlSelect`:
                // Ctrl-click on an own core respawns the player there (I01 T12).
                if let Some(core) = self.grid.entity_at(pos)
                    && self.player_unit.is_some()
                    && self.is_own_core(core)
                {
                    let _ = self.control_select_core(core);
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::UnitStance {
                units,
                stance,
                enabled,
            } => {
                // Live units first; `0` routes to the relay runtime (net id
                // space), mirroring `UnitCommand` (G01 T11).
                if self.apply_live_unit_stance(units, *stance, *enabled) == 0 {
                    self.ensure_unit_commands()
                        .set_unit_stance(units, *stance, *enabled);
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Rotate { x, y, direction } => {
                let pos = TilePos::new(*x, *y);
                if self.grid.index(pos).is_err() {
                    return Err(CommandError::InvalidTarget);
                }
                let Some(entity) = self.grid.entity_at(pos) else {
                    return Err(CommandError::InvalidTarget);
                };
                // Upstream `InputHandler.rotateBlock`: `rotation = mod(rotation +
                // sign(direction), 4)`; `direction == true` is counter-clockwise.
                let step = if *direction { 1 } else { -1 };
                let Some(new_rot) =
                    self.ecs
                        .0
                        .get_mut::<BuildingComp>(entity)
                        .map(|mut building| {
                            building.rot = (building.rot as i32 + step).rem_euclid(4) as u8;
                            building.rot
                        })
                else {
                    return Err(CommandError::InvalidTarget);
                };
                // The live runtime reads `Building.rotation`; keep both in sync.
                if let Some(mut building) = self
                    .ecs
                    .0
                    .get_mut::<crate::entities::comp::Building>(entity)
                {
                    building.rotation = new_rot;
                }
                crate::world::update::no_sleep(&mut self.ecs.0, entity);
                // Rotation matters for the facing scans (`BeamDrill.updateFacing`
                // / `WallCrafter.getEfficiency`; PAR-0079 §5).
                if let Some(runtime) = self.block_runtime.as_ref() {
                    let content = runtime.content();
                    crate::world::behavior::production::refresh_drill_ore(
                        &mut self.ecs.0,
                        &self.grid,
                        content,
                        entity,
                    );
                }
                // `Build.beginPlace` (same-block rotate) FX (`Build.java:88,112`):
                // `Fx.rotateBlock.at(build.x, build.y, build.block.size)`.
                let placed_block = self.grid.block_at(pos).unwrap_or(BlockId::AIR);
                if let Some(def) = self
                    .block_runtime
                    .as_ref()
                    .and_then(|runtime| runtime.content().block(placed_block))
                {
                    let (offset, size) = (def.offset, def.size);
                    let cx = *x as f32 * 8.0 + offset;
                    let cy = *y as f32 * 8.0 + offset;
                    if let Some(runtime) = self.unit_runtime() {
                        use crate::content::registries::fx_meta::EffectRef;
                        runtime.fx.effect(
                            &EffectRef::Named(crate::content::EffectId::ROTATE_BLOCK),
                            cx,
                            cy,
                            size as f32,
                            crate::content::Rgba::WHITE,
                        );
                    }
                    // `Build.java:90`/`InputHandler.java:704`:
                    // `Sounds.blockRotate.at(build, 1f + Mathf.range(0.1f), 1f)`.
                    if let Some(audio) = self
                        .ecs
                        .0
                        .get_resource::<crate::audio::AudioSinkRes>()
                        .cloned()
                    {
                        let fx = self
                            .rng_streams
                            .next_float(crate::determinism::RngStream::Fx);
                        audio.emit(crate::audio::AudioEvent::at(
                            crate::audio::SoundId::BLOCK_ROTATE,
                            cx,
                            cy,
                            1.0 + (fx * 0.2 - 0.1),
                            1.0,
                            true,
                        ));
                    }
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Payload { kind, x, y, target } => {
                // `Payloadc` unit carrier (PAR-0138): pick up a unit/building or
                // drop the last carried payload. No-op without a player unit.
                let _ = self.apply_payload_command(*kind, *x, *y, *target);
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Inventory {
                kind,
                x,
                y,
                item,
                amount,
                angle,
            } => {
                // `InputHandler.requestItem`/`transferInventory`/`dropItem`.
                let _ = angle;
                let pos = TilePos::new(*x, *y);
                if self.grid.index(pos).is_err() {
                    return Err(CommandError::InvalidTarget);
                }
                if self.player_unit.is_some() {
                    // Live player unit: the transfer operates on the unit's own
                    // stack with Java's range/`interactable`/`acceptStack`
                    // checks (G08 T2a). The relay `player_item` stays for
                    // headless/relay callers.
                    self.apply_player_inventory(*kind, pos, *item, *amount);
                } else if self.block_runtime.is_some()
                    && let Some(entity) = self.grid.entity_at(pos)
                {
                    // Relay runtime: the real `ItemModule` is the storage the
                    // block behaviors read, so mutate that instead of the relay
                    // mirror.
                    let world_apply = self
                        .world_apply
                        .get_or_insert_with(world_apply::WorldApplyRuntime::new);
                    world_apply.apply_inventory_entity(
                        &mut self.ecs.0,
                        entity,
                        *kind,
                        *item,
                        *amount,
                    )?;
                } else {
                    let block = self.grid.block_at(pos).unwrap_or(BlockId::AIR);
                    self.ensure_world_apply()
                        .apply_inventory(pos, block, *kind, *item, *amount)?;
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::DeletePlans { positions } => {
                // `InputHandler.deletePlans`: the relay `BuildQueue` plus the
                // player unit's own `BuilderPlans` queue (PAR-0098 §2a).
                self.ensure_world_apply().apply_delete_plans(positions)?;
                self.delete_player_plans(positions);
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::CommandBuilding { positions, x, y } => {
                // `InputHandler.commandBuilding`: `Building.onCommand(target)`
                // for every live, `Block.commandable` building (upstream also
                // checks the player team; the client selection filters team and
                // `Sim` has no relay-side team).
                for &packed in positions.iter() {
                    let pos = TilePos::from_pack(packed);
                    if self.grid.index(pos).is_err() || self.grid.entity_at(pos).is_none() {
                        continue;
                    }
                    let Some(block) = self.grid.block_at(pos) else {
                        continue;
                    };
                    let (commandable, payload_block) = self
                        .ensure_world_apply()
                        .content()
                        .block(block)
                        .map(|def| (def.commandable, def.kind.is_payload_block()))
                        .unwrap_or((false, false));
                    if !commandable {
                        continue;
                    }
                    // `BuildingCommandEvent` with a `PayloadBlockBuild`
                    // (`HintsFragment.java:91-95`): the factoryControl hint
                    // event (`InputHandler.java:491` is the command site).
                    self.ensure_world_apply()
                        .apply_command_building(pos, *x, *y);
                    if payload_block {
                        self.hint_events.events.insert("factorycontrol");
                    }
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Place {
                x,
                y,
                block,
                rotation,
                team,
                player,
            } => {
                // The live runtime honors rotation/team end-to-end; without it the
                // P0 path below still drops rotation (no rotation-aware `Command`).
                if self.block_runtime.is_some() {
                    let id = BlockId::new(*block);
                    if self.content.name(id).is_err() {
                        return Err(CommandError::UnknownContent(*block));
                    }
                    if id == BlockId::AIR {
                        return Err(CommandError::InvalidTarget);
                    }
                    let pos = TilePos::new(*x, *y);
                    if self.grid.index(pos).is_err() {
                        return Err(CommandError::InvalidTarget);
                    }
                    let rot = u8::try_from((*rotation).rem_euclid(4)).unwrap_or(0);
                    // Player input queues a build plan on the player unit
                    // (`InputHandler.flushPlans` -> `BuilderComp.addBuild`);
                    // the builder pass constructs it over time (PAR-0098).
                    if player.is_some() && self.player_can_build() {
                        if !self.queue_player_place(*x, *y, id, rot) {
                            log::debug!("player plan rejected at ({x}, {y})");
                        }
                        self.commands_applied = self.commands_applied.wrapping_add(1);
                        return Ok(());
                    }
                    if player.is_some() && self.team_has_core(*team) {
                        // No builder: Java's `player.unit()` is `Nulls.unit`,
                        // `addBuild` returns at `if(!canBuild()) return;`
                        // (`BuilderComp.java`).
                        log::debug!(
                            "player plan at ({x}, {y}) dropped: no build-capable player unit"
                        );
                        self.commands_applied = self.commands_applied.wrapping_add(1);
                        return Ok(());
                    }
                    if self.runtime_place(
                        *x,
                        *y,
                        id,
                        rot,
                        *team,
                        crate::world::build::PlaceFlags::API,
                    ) {
                        self.push_event(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                            x: *x,
                            y: *y,
                            block: id,
                            team: *team,
                        }));
                    } else {
                        log::debug!(
                            "place command rejected at ({x}, {y}) by the live block runtime"
                        );
                    }
                    self.commands_applied = self.commands_applied.wrapping_add(1);
                    return Ok(());
                }
            }
            SimCommand::Break {
                x,
                y,
                player: Some(_),
            } if self.player_can_build() => {
                // `InputHandler.removeSelection` -> a deconstruction plan.
                let pos = TilePos::new(*x, *y);
                if self.grid.index(pos).is_err() {
                    return Err(CommandError::InvalidTarget);
                }
                if !self.queue_player_break(*x, *y) {
                    log::debug!("player break plan rejected at ({x}, {y})");
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Break {
                x,
                y,
                player: Some(_),
            } if self.team_has_core(self.player_command_team()) => {
                // No builder: `player.unit()` is `Nulls.unit`, whose
                // `addBuild` (break plan) does nothing (`BuilderComp.java`).
                log::debug!("player break at ({x}, {y}) dropped: no build-capable player unit");
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Configure { x, y, value } => {
                // `InputHandler` configure: apply the value to the target
                // building's behavior. Unsupported until a block runtime exists
                // (P0 has no behavior table).
                if self.block_runtime.is_none() {
                    return Err(CommandError::Unsupported("configure"));
                }
                let pos = TilePos::new(*x, *y);
                if self.grid.index(pos).is_err() {
                    return Err(CommandError::InvalidTarget);
                }
                let Some(entity) = self.grid.entity_at(pos) else {
                    return Err(CommandError::InvalidTarget);
                };
                let world_value = match self.block_runtime.as_ref() {
                    Some(runtime) => runtime.resolve_config(value)?,
                    None => return Err(CommandError::Unsupported("configure")),
                };
                // T2: power nodes take link configs (`PowerNode.java`
                // `config(Integer.class)` / `config(Point2[].class)`) through the
                // `PowerGrids` arena, not the behavior config path.
                if !self.apply_world_config(entity, world_value) {
                    return Err(CommandError::InvalidTarget);
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::ConfigTap {
                x,
                y,
                other_x,
                other_y,
            } => {
                // H02 T6 (PAR-0220): `config.getSelected().onConfigureBuildTapped(other)`
                // (`InputHandler.java:1993-2005`). Java computes the tap on the
                // client and sends `configure` calls; the port's input layer
                // holds no rules, so the sim computes the result and applies the
                // returned configs in order.
                if self.block_runtime.is_none() {
                    return Err(CommandError::Unsupported("config_tap"));
                }
                let Some(selected) = self.grid.entity_at(TilePos::new(*x, *y)) else {
                    return Err(CommandError::InvalidTarget);
                };
                let Some(other) = self.grid.entity_at(TilePos::new(*other_x, *other_y)) else {
                    return Err(CommandError::InvalidTarget);
                };
                let result =
                    crate::world::config_tap::config_tap(&self.ecs.0, &self.grid, selected, other);
                let (kind, values) = match result {
                    crate::world::behavior::ConfigTapResult::Consume(values) => {
                        (ConfigTapKind::Consume, values)
                    }
                    crate::world::behavior::ConfigTapResult::ConsumeAndDeselect(values) => {
                        (ConfigTapKind::Deselect, values)
                    }
                    crate::world::behavior::ConfigTapResult::SwitchAfter(values) => {
                        (ConfigTapKind::Switch, values)
                    }
                };
                for (entity, value) in values {
                    self.apply_world_config(entity, value);
                }
                self.last_config_tap = Some(kind);
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            _ => {}
        }
        let op = command.op_name();
        match command.to_p0() {
            Some(p0) => self.apply(p0).map_err(|error| match error {
                SimError::UnknownBlock(id) => CommandError::UnknownContent(id),
                SimError::World(_) | SimError::CannotPlaceAir => CommandError::InvalidTarget,
            }),
            None => Err(CommandError::Unsupported(op)),
        }
    }
}

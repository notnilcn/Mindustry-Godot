// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Player unit possession, respawn, mining, core spawning and the held-entity
//! accessors.
//! Upstream: core/src/mindustry/input/InputHandler.java
//! (requestUnitPayload/requestBuildPayload/requestDropPayload)
//! core/src/mindustry/entities/comp/PlayerComp.java

use super::*;

impl Sim {
    /// Spawns a live unit by content name and tracks it; `None` without the
    /// runtime or for an unknown unit type.
    pub fn spawn_live_unit(
        &mut self,
        name: &str,
        team: u8,
        x: f32,
        y: f32,
        rotation: f32,
    ) -> Option<Entity> {
        let ecs = &mut self.ecs.0;
        let mut runtime = ecs.remove_non_send::<UnitRuntime>()?;
        let entity = runtime.spawn(ecs, name, team, x, y, rotation);
        ecs.insert_non_send(runtime);
        entity
    }

    /// `PayloadBlockBuild.getPayload()` accessor: the payload held by a
    /// payload-block building (`PayloadHolder.payload`), independent of the
    /// block family.
    pub fn held_payload(&self, holder: Entity) -> Option<crate::world::behavior::PayloadRef> {
        self.ecs
            .0
            .get::<crate::world::blocks::payloads::PayloadHolder>(holder)
            .and_then(|state| state.payload)
    }

    /// `Payload.size()` of the payload held by `holder`, in world units
    /// (block size for build payloads, `UnitType.hitSize` for unit payloads).
    pub fn held_payload_size(&self, holder: Entity) -> Option<f32> {
        let payload = self.held_payload(holder)?;
        Some(crate::world::blocks::payloads::payload_size(
            &self.ecs.0,
            payload,
        ))
    }

    /// Uploads the player movement axis (`Core.input.axis(Binding.moveX/Y)`)
    /// for the ticks that follow; read by
    /// [`crate::ai::types::player::update_player`].
    pub fn set_player_input(&mut self, axis_x: f32, axis_y: f32) {
        let mut input = self
            .ecs
            .0
            .get_resource::<PlayerInput>()
            .copied()
            .unwrap_or_default();
        input.axis_x = axis_x;
        input.axis_y = axis_y;
        self.ecs.0.insert_resource(input);
    }

    /// Uploads the full desktop control state (`DesktopInput.updateMovement`):
    /// movement axis, `player.shooting`, the cursor aim point, the boost key
    /// and `mouseMove`. Read by [`crate::ai::types::player::update_player`].
    #[allow(clippy::too_many_arguments)] // one atomic `Player`-state upload
    pub fn set_player_control(
        &mut self,
        axis_x: f32,
        axis_y: f32,
        shooting: bool,
        aim_x: f32,
        aim_y: f32,
        boosting: bool,
        mouse_move: bool,
    ) {
        let mut input = self
            .ecs
            .0
            .get_resource::<PlayerInput>()
            .copied()
            .unwrap_or_default();
        input.axis_x = axis_x;
        input.axis_y = axis_y;
        input.shooting = shooting;
        input.aim_x = aim_x;
        input.aim_y = aim_y;
        input.boosting = boosting;
        input.mouse_move = mouse_move;
        self.ecs.0.insert_resource(input);
    }

    /// Sets or clears the player unit's mine order
    /// (`InputHandler.tryBeginMine`/`tryStopMine` set `player.unit().mineTile`).
    pub fn set_player_mine_tile(&mut self, tile: Option<TilePos>) {
        let mut input = self
            .ecs
            .0
            .get_resource::<PlayerInput>()
            .copied()
            .unwrap_or_default();
        input.mine_tile = tile;
        input.mine_timer = 0.0;
        self.ecs.0.insert_resource(input);
    }

    /// The player unit's ordered mine tile, if any.
    pub fn player_mine_tile(&self) -> Option<TilePos> {
        self.ecs
            .0
            .get_resource::<PlayerInput>()
            .and_then(|input| input.mine_tile)
    }

    /// Total ore produced by the player unit (tutorial counter evidence).
    pub fn player_mined_count(&self) -> i64 {
        self.ecs
            .0
            .get_resource::<PlayerInput>()
            .map(|input| input.mined)
            .unwrap_or(0)
    }

    /// `InputHandler.canMine` for the player unit; `false` without one.
    pub fn player_can_mine(&self, x: i16, y: i16) -> bool {
        let Some(entity) = self.player_unit else {
            return false;
        };
        let Some(runtime) = self.unit_runtime() else {
            return false;
        };
        crate::ai::types::player::can_mine_tile(
            &self.ecs.0,
            &runtime.grid,
            &runtime.content,
            entity,
            TilePos::new(x, y),
        )
    }

    /// Spawns the controlled player unit (`CoreBlock.playerSpawn` with the core
    /// type's unit) and rebinds its controller to [`AiKind::Player`]. The
    /// `comp::Player` marker puts it in the `player` group; `PlayerInput` drives
    /// it from the next tick.
    pub fn spawn_player_unit(
        &mut self,
        name: &str,
        team: u8,
        x: f32,
        y: f32,
        rotation: f32,
    ) -> Option<Entity> {
        let entity = self.spawn_live_unit(name, team, x, y, rotation)?;
        if let Some(mut slot) = self.ecs.0.get_mut::<ControllerSlot>(entity) {
            slot.kind = AiKind::Player;
            slot.target = None;
        }
        self.ecs
            .0
            .entity_mut(entity)
            .insert(crate::entities::comp::Player);
        self.player_unit = Some(entity);
        self.player_team = Some(team);
        self.player_death_timer = 0.0;
        Some(entity)
    }

    /// `PlayerComp.team` (`PlayerComp.java:44-48`): the player always has a
    /// team, so `update_player_respawn` retries the spawn every 60 ticks even
    /// when the first `spawn_player_at_best_core` failed (no supported core).
    pub fn set_player_team(&mut self, team: u8) {
        self.player_team = Some(team);
        self.player_death_timer = 0.0;
    }

    /// `PlayerComp.bestCore()` (`PlayerComp.java:121-126`): the largest core,
    /// then the closest to the last player position; supported-env cores are
    /// preferred unless there is only one core.
    pub fn best_core_for_player(&self, team: u8) -> Option<Entity> {
        use crate::world::blocks::storage::core_block::CoreBuild;
        let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
        let content = &runtime.content;
        let env = crate::game::runtime::rules_of(&self.ecs.0).map_or(1, |rules| rules.env);
        let pos = self.player_last_pos.or_else(|| self.player_unit_position());
        let mut cores: Vec<(Entity, i32, bool, f32)> = Vec::new();
        for entity_ref in self.ecs.0.iter_entities() {
            if entity_ref.get::<CoreBuild>().is_none() {
                continue;
            }
            if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) {
                continue;
            }
            let Some(build) = entity_ref.get::<Building>() else {
                continue;
            };
            let size = content.block(build.block).map_or(1, |def| def.size);
            let supported = content
                .block(build.block)
                .and_then(|def| def.core_unit)
                .and_then(|name| content.unit_by_name(name))
                .is_none_or(|unit| unit.supports_env(env));
            let dst2 = entity_ref.get::<Pos>().map_or(0.0, |p| {
                pos.map_or(0.0, |(px, py)| (p.x - px).powi(2) + (p.y - py).powi(2))
            });
            cores.push((entity_ref.id(), size, supported, dst2));
        }
        let single = cores.len() <= 1;
        cores
            .iter()
            .filter(|(_, _, supported, _)| single || *supported)
            .min_by(|a, b| {
                b.1.cmp(&a.1)
                    .then(a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
            })
            .map(|(entity, _, _, _)| *entity)
    }

    /// `CoreBlock.playerSpawn` (`CoreBlock.java:95-125`) at the best core
    /// (`PlayerComp.bestCore`). See [`Self::spawn_player_at_core`].
    pub fn spawn_player_at_best_core(&mut self, team: u8) -> Option<Entity> {
        let core = self.best_core_for_player(team)?;
        self.spawn_player_at_core(team, core)
    }

    /// `CoreBlock.requestSpawn(player)` (`CoreBlock.java:639-643`): spawn the
    /// core's `unitType` at the core with reloaded mounts, `spawnedByCore` set
    /// and the upward impulse; `None` when the core unit type does not support
    /// the sector env. Used by the best-core respawn and by
    /// `CoreBlock.onControlSelect` (I01 T12).
    pub fn spawn_player_at_core(&mut self, team: u8, core: Entity) -> Option<Entity> {
        let (x, y) = self.ecs.0.get::<Pos>(core).map(|pos| (pos.x, pos.y))?;
        let (name, env): (String, i32) = {
            let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
            let content = &runtime.content;
            let block = self.ecs.0.get::<Building>(core).map(|b| b.block)?;
            let name = content
                .block(block)
                .and_then(|def| def.core_unit)
                .unwrap_or("alpha")
                .to_string();
            let env = crate::game::runtime::rules_of(&self.ecs.0).map_or(1, |rules| rules.env);
            (name, env)
        };
        // `CoreBlock.requestSpawn`: bail when the unit type does not support
        // the sector env (the caller retries on the next death timer window).
        let supports = {
            let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
            runtime
                .content
                .unit_by_name(&name)
                .is_some_and(|def| def.supports_env(env))
        };
        if !supports {
            return None;
        }
        let entity = self.spawn_player_unit(&name, team, x, y, 90.0)?;
        // `for(mount : unit.mounts) mount.reload = mount.weapon.reload;`
        let reloads: Vec<f32> = {
            let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
            self.ecs
                .0
                .get::<UnitTypeComp>(entity)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .map(|def| def.weapons.iter().map(|weapon| weapon.reload).collect())
                .unwrap_or_default()
        };
        if let Some(mut weapons) = self.ecs.0.get_mut::<UnitWeapons>(entity) {
            for (mount, reload) in weapons.mounts.iter_mut().zip(reloads) {
                mount.reload = reload;
            }
        }
        if let Some(mut core_comp) = self.ecs.0.get_mut::<UnitCore>(entity) {
            core_comp.spawned_by_core = true;
        }
        // `unit.impulse(0, 3)` (`CoreBlock.playerSpawn`): `vel += v / mass`.
        crate::entities::comp::unit::velocity::impulse(&mut self.ecs.0, entity, 0.0, 3.0);
        // `if(state.isCampaign() && player == Vars.player){ spawnType.unlock(); }`
        // (CoreBlock.java:121-123): queue the spawned unit type for the campaign
        // unlock pipeline (H04 T2's `UnlockRef`).
        let unit_type = self
            .ecs
            .0
            .get::<UnitTypeComp>(entity)
            .map(|comp| comp.type_id);
        let is_campaign = self
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .is_some_and(|campaign| campaign.session.is_campaign());
        if is_campaign
            && let Some(id) = unit_type
            && let Some(queues) = self
                .ecs
                .0
                .get_resource::<crate::game::runtime::CampaignCoreQueues>()
            && let Ok(mut queues) = queues.0.lock()
        {
            let unlock = crate::game::runtime::UnlockRef::Unit(id);
            if !queues.unlocks.contains(&unlock) {
                queues.unlocks.push(unlock);
            }
        }
        Some(entity)
    }

    /// `PlayerComp.update` (`PlayerComp.java:214-243`): clear a dead player
    /// unit, then respawn at the best core after `deathDelay` (60 ticks) while
    /// the team still has a core.
    pub fn update_player_respawn(&mut self) {
        if let Some(entity) = self.player_unit {
            if let Some(pos) = self.ecs.0.get::<Pos>(entity) {
                self.player_last_pos = Some((pos.x, pos.y));
            }
            let gone = self.ecs.0.get_entity(entity).is_err()
                || self
                    .ecs
                    .0
                    .get::<Health>(entity)
                    .is_none_or(|health| health.dead);
            let falling = self.ecs.0.get::<Falling>(entity).is_some();
            if gone && !falling {
                self.player_unit = None;
                self.player_death_timer = 0.0;
                if self.ecs.0.get_entity(entity).is_ok() {
                    let _ = self.ecs.0.despawn(entity);
                }
            } else if gone {
                // A dead `Falling` player unit is a wreck: Java's `dead()` is
                // true (the 60-tick respawn delay runs, `PlayerComp.update`)
                // but the wreck stays in the world until it crashes and
                // `resolve_deaths` destroys it (T6). Clear the handle so the
                // respawn timer starts, do not despawn.
                self.player_unit = None;
                self.player_death_timer = 0.0;
            }
        }
        if self.player_unit.is_some() {
            self.player_death_timer = 0.0;
            return;
        }
        let Some(team) = self.player_team else {
            return;
        };
        if self.best_core_for_player(team).is_none() {
            return;
        }
        self.player_death_timer += 1.0;
        if self.player_death_timer >= 60.0 {
            self.player_death_timer = 0.0;
            let _ = self.spawn_player_at_best_core(team);
        }
    }

    /// `InputHandler.unitControl` (`InputHandler.java:775-822`): possess an own
    /// AI unit, moving the `Player` controller to it; a previous core-spawned
    /// unit is despawned without death effects and its type docked.
    pub fn possess_unit(&mut self, seq: i32) -> bool {
        use crate::entities::comp::Player;
        use crate::entities::comp::unit::comp::{DockedType, UnitCore};
        let Some(team) = self.player_team else {
            return false;
        };
        let target = self
            .ecs
            .0
            .iter_entities()
            .find(|entity| {
                entity
                    .get::<crate::ecs::EntitySeq>()
                    .is_some_and(|s| s.0 == seq as u64)
            })
            .map(|entity| entity.id());
        let Some(target) = target else {
            return false;
        };
        if self.ecs.0.get::<TeamComp>(target).map(|t| t.team) != Some(team) {
            return false;
        }
        if self
            .ecs
            .0
            .get::<Health>(target)
            .is_none_or(|health| health.dead)
        {
            return false;
        }
        let def = {
            let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
                return false;
            };
            self.ecs
                .0
                .get::<UnitTypeComp>(target)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .cloned()
        };
        let Some(def) = def else {
            return false;
        };
        if !def.player_controllable {
            return false;
        }
        if self
            .ecs
            .0
            .get::<ControllerSlot>(target)
            .is_some_and(|slot| slot.kind == AiKind::Player)
        {
            return false;
        }
        let before = self.player_unit.filter(|entity| *entity != target);
        if let Some(mut slot) = self.ecs.0.get_mut::<ControllerSlot>(target) {
            slot.kind = AiKind::Player;
            slot.target = None;
        }
        self.ecs.0.entity_mut(target).insert(Player);
        self.player_unit = Some(target);
        self.player_death_timer = 0.0;
        if let Some(before) = before {
            let core_spawned = self
                .ecs
                .0
                .get::<UnitCore>(before)
                .is_some_and(|core| core.spawned_by_core);
            // `unit.dockedType = before.type` for a core spawn; otherwise carry
            // a dockable type from the previous unit.
            let docked = if core_spawned {
                self.ecs
                    .0
                    .get::<UnitTypeComp>(before)
                    .map(|comp| comp.type_id)
            } else {
                let carried = self.ecs.0.get::<DockedType>(before).map(|docked| docked.0);
                carried.or_else(|| {
                    self.ecs
                        .0
                        .get::<UnitTypeComp>(before)
                        .map(|comp| comp.type_id)
                        .filter(|id| {
                            self.ecs
                                .0
                                .get_non_send::<UnitRuntime>()
                                .and_then(|runtime| runtime.content.unit(*id))
                                .is_some_and(|def| def.core_unit_dock)
                        })
                })
            };
            if let Some(id) = docked {
                self.ecs.0.entity_mut(target).insert(DockedType(id));
            }
            if core_spawned {
                // `Call.unitDespawn(before)`: removal without death effects.
                let _ = self.ecs.0.despawn(before);
                if let Some(mut runtime) = self.ecs.0.get_non_send_mut::<UnitRuntime>() {
                    runtime.units.retain(|entity| *entity != before);
                }
            } else {
                let before_def = {
                    let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
                        return false;
                    };
                    self.ecs
                        .0
                        .get::<UnitTypeComp>(before)
                        .and_then(|comp| runtime.content.unit(comp.type_id))
                        .cloned()
                };
                let _ = before_def;
                self.release_unit_to_ai(before, team);
            }
        }
        true
    }

    /// Releases a possessed unit back to its team AI
    /// (`InputHandler.unitClear` standard branch / `Unit.resetController`):
    /// drop the `Player` marker and hand the unit to the team controller.
    /// Also the controller-switch half of `UnitComp.afterRead` (H04 T13).
    pub(crate) fn release_unit_to_ai(&mut self, entity: Entity, team: u8) {
        use crate::entities::comp::Player;
        let def = {
            let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
                return;
            };
            self.ecs
                .0
                .get::<UnitTypeComp>(entity)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .cloned()
        };
        let kind = def.map_or(AiKind::Ground, |def| {
            crate::entities::comp::unit::lifecycle::select_team_controller(&self.ecs.0, &def, team)
        });
        self.ecs.0.entity_mut(entity).remove::<Player>();
        if let Some(mut slot) = self.ecs.0.get_mut::<ControllerSlot>(entity) {
            slot.kind = kind;
            slot.target = None;
        }
    }

    /// `InputHandler.unitClear` (`InputHandler.java:824-875`): dock into the
    /// carried core unit type when possible, otherwise release the unit and
    /// respawn at the best core immediately.
    pub fn clear_unit(&mut self) -> bool {
        use crate::entities::comp::unit::comp::{DockedType, HitboxComp, UnitCore};
        let Some(current) = self.player_unit else {
            return false;
        };
        let Some(team) = self.player_team else {
            return false;
        };
        let core_spawned = self
            .ecs
            .0
            .get::<UnitCore>(current)
            .is_some_and(|core| core.spawned_by_core);
        if !core_spawned {
            // `docked == null` -> the best core's unit type is the fallback.
            let docked = self
                .ecs
                .0
                .get::<DockedType>(current)
                .map(|docked| docked.0)
                .or_else(|| {
                    let core = self.best_core_for_player(team)?;
                    let block = self.ecs.0.get::<Building>(core).map(|build| build.block)?;
                    let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
                    let name = runtime.content.block(block).and_then(|def| def.core_unit)?;
                    runtime.content.unit_by_name(name).map(|def| def.id)
                });
            let dock_info = docked.and_then(|id| {
                let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
                let def = runtime.content.unit(id)?;
                def.core_unit_dock
                    .then(|| (def.name().to_string(), def.flying, def.hit_size))
            });

            if let Some((name, dock_flying, dock_hit)) = dock_info {
                let (px, py) = self
                    .ecs
                    .0
                    .get::<Pos>(current)
                    .map_or((0.0, 0.0), |pos| (pos.x, pos.y));
                let rotation = self
                    .ecs
                    .0
                    .get::<UnitCore>(current)
                    .map_or(0.0, |core| core.rotation);
                let (current_flying, current_hit) = {
                    let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
                        return false;
                    };
                    let flying = self
                        .ecs
                        .0
                        .get::<UnitTypeComp>(current)
                        .and_then(|comp| runtime.content.unit(comp.type_id))
                        .is_some_and(|def| def.flying);
                    let hit = self
                        .ecs
                        .0
                        .get::<HitboxComp>(current)
                        .map_or(8.0, |hitbox| hitbox.hit_size);
                    (flying, hit)
                };
                // Translate backwards so the docked unit is not stuck inside
                // the possessed one (`InputHandler.java:851-854`).
                let (sx, sy) = if current_flying && dock_flying {
                    let rad = (rotation + 180.0).to_radians();
                    let len = current_hit / 2.0 + dock_hit / 2.0;
                    (px + rad.cos() * len, py + rad.sin() * len)
                } else {
                    (px, py)
                };
                // The possessed unit must go back to the AI before the docked
                // unit takes the `Player` controller (`unit.controller(player)`
                // calls `old.resetController()`, InputHandler.java:865-872);
                // otherwise two entities keep the player controller.
                self.release_unit_to_ai(current, team);
                let Some(entity) = self.spawn_player_unit(&name, team, sx, sy, rotation) else {
                    return false;
                };
                if let Some(mut core_comp) = self.ecs.0.get_mut::<UnitCore>(entity) {
                    core_comp.spawned_by_core = true;
                }
                return true;
            }
        }
        // Standard respawn: release the unit to AI, then `player.checkSpawn()`.
        self.release_unit_to_ai(current, team);
        self.player_unit = None;
        let _ = self.spawn_player_at_best_core(team);
        true
    }

    /// `CoreBlock.canControlSelect(player)`/`onControlSelect(player)`
    /// (`CoreBlock.java:619-637`): Ctrl-click on an own core clears the player's
    /// unit and respawns it at that core (`player.clearUnit();
    /// player.deathTimer = Player.deathDelay + 1; requestSpawn(player)` ->
    /// `Call.playerSpawn(tile, player)`). I01 T12.
    pub fn control_select_core(&mut self, core: Entity) -> bool {
        let Some(team) = self.player_team else {
            return false;
        };
        // `canControlSelect(unit)` = `unit.isPlayer()`: the player must hold a
        // unit to control-select with.
        if self.player_unit.is_none() {
            return false;
        }
        if let Some(current) = self.player_unit {
            self.release_unit_to_ai(current, team);
            self.player_unit = None;
        }
        // `player.deathTimer = Player.deathDelay + 1` (60 + 1) guards the retry
        // window while `requestSpawn` spawns immediately.
        self.player_death_timer = 61.0;
        if self.spawn_player_at_core(team, core).is_some() {
            self.player_death_timer = 0.0;
            true
        } else {
            false
        }
    }

    /// `InputHandler.selectedUnit()`'s ControlBlock branch
    /// (`InputHandler.java:2159-2161`): `build instanceof ControlBlock cont &&
    /// cont.canControl() && build.team == player.team() && cont.unit() !=
    /// player.unit()` -> the detached `block` proxy unit. Turrets
    /// (`TurretBuild.unit`; I01 T12) need `playerControllable`
    /// (`Turret.java:382-384`); size-1 routers (`Router.java:46-49`) get their
    /// proxy created on first use like Java's lazy `unit()` (`:37-44`; L06
    /// T2). `None` when the tile holds no controllable block, the team
    /// differs, or the proxy is the player's own unit already.
    pub fn control_block_unit_id(&mut self, pos: TilePos) -> Option<i32> {
        let entity = self.grid.entity_at(pos)?;
        let team = self.player_team?;
        if self.ecs.0.get::<TeamComp>(entity).map(|t| t.team) != Some(team) {
            return None;
        }
        let def = self
            .ecs
            .0
            .get::<crate::entities::comp::Building>(entity)
            .and_then(|building| {
                self.ecs
                    .0
                    .get_resource::<crate::world::block::BlockTable>()
                    .and_then(|table| table.get(building.block))
                    .map(|instance| {
                        (
                            instance.def.kind,
                            instance.def.size,
                            instance.def.player_controllable,
                        )
                    })
            });
        let unit = match def {
            // `RouterBuild.canControl()` = `size == 1` (the distributor is not).
            Some((crate::content::BlockKind::Router, size, _)) => {
                if size != 1 {
                    return None;
                }
                self.ensure_router_block_unit(entity)?
            }
            Some((_, _, player_controllable)) => {
                // `TurretBuild.canControl()` = `playerControllable`.
                if !player_controllable {
                    return None;
                }
                self.ecs
                    .0
                    .get::<crate::world::blocks::defense::turrets::TurretState>(entity)?
                    .block_unit?
            }
            None => return None,
        };
        if self.player_unit == Some(unit) || self.ecs.0.get_entity(unit).is_err() {
            return None;
        }
        self.ecs.0.get::<EntitySeq>(unit).map(|seq| seq.0 as i32)
    }

    /// `RouterBuild.unit()` (`Router.java:37-44`): `UnitTypes.block.create(team)`
    /// tied to the router on first use; kept on `RouterBuild.block_unit`,
    /// never added to the unit pass.
    pub(super) fn ensure_router_block_unit(&mut self, router: Entity) -> Option<Entity> {
        use crate::world::blocks::distribution::router::RouterBuild;
        if let Some(unit) = self.ecs.0.get::<RouterBuild>(router)?.block_unit
            && self.ecs.0.get_entity(unit).is_ok()
        {
            return Some(unit);
        }
        let def = self
            .unit_runtime()?
            .content
            .unit_by_name("block")
            .cloned()?;
        let team = self.ecs.0.get::<TeamComp>(router).map_or(0, |t| t.team);
        let (x, y) = self
            .ecs
            .0
            .get::<crate::entities::comp::Pos>(router)
            .map(|pos| (pos.x, pos.y))?;
        let tile = self
            .ecs
            .0
            .get::<crate::entities::comp::Building>(router)
            .map_or(i32::MIN, |building| building.tile.pack());
        let world = &mut self.ecs.0;
        let seq = crate::world::blocks::units::behavior::next_unit_seq(world);
        let unit = crate::entities::comp::unit::lifecycle::spawn_unit_def(
            world, seq, &def, team, x, y, 0.0,
        );
        world
            .entity_mut(unit)
            .insert(crate::entities::comp::unit::comp::BlockUnitComp { tile });
        if let Some(mut state) = world.get_mut::<RouterBuild>(router) {
            state.block_unit = Some(unit);
        }
        Some(unit)
    }
}

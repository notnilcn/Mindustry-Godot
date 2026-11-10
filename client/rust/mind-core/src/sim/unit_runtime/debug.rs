// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Debug/dump seams: player unit state, debug kill/status, boss and
//! selectable-unit snapshots.
//! Upstream: core/src/mindustry/entities/comp/UnitComp.java

use super::*;

impl Sim {
    /// Live selectable-unit snapshot for the RTS input layer
    /// (`[{id,type,x,y,team,commandable}]`; `id` is the unit's `EntitySeq`).
    ///
    /// Published to `MindInput.set_selectable_units_json` every frame so
    /// drag/tap selection and command mode have a list (PAR-0140). Upstream's
    /// selectable list is `Units`-backed, so both teams are listed (the input
    /// filters by team/`commandable`); `commandable` is `Unit.isCommandable()`
    /// (`controller instanceof CommandAI`) on the live default team, so the
    /// player's own unit (`Player` controller) is never selectable.
    pub fn selectable_units_json(&self) -> String {
        let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
            return String::from("[]");
        };
        let default_team = self.live_default_team();
        let mut rows: Vec<serde_json::Value> = Vec::new();
        for &entity in &runtime.units {
            let Ok(entity_ref) = self.ecs.0.get_entity(entity) else {
                continue;
            };
            if entity_ref.get::<UnitCore>().is_some_and(|core| core.dead) {
                continue;
            }
            let (Some(seq), Some(pos), Some(team), Some(type_comp)) = (
                entity_ref.get::<EntitySeq>(),
                entity_ref.get::<Pos>(),
                entity_ref.get::<TeamComp>(),
                entity_ref.get::<UnitTypeComp>(),
            ) else {
                continue;
            };
            let commandable = team.team == default_team
                && entity_ref
                    .get::<ControllerSlot>()
                    .is_some_and(|slot| slot.kind == AiKind::Command);
            let hit_size = runtime
                .content
                .unit(type_comp.type_id)
                .map_or(0.0, |def| def.hit_size);
            // `instanceof Payloadc` (transport select) and
            // `UnitType.controlSelectGlobal` (H08 T6a consumers).
            let payload = entity_ref
                .get::<crate::entities::comp::unit::comp::UnitPayloads>()
                .is_some();
            let select_global = runtime
                .content
                .unit(type_comp.type_id)
                .is_none_or(|def| def.control_select_global);
            rows.push(serde_json::json!({
                "id": seq.0 as i32,
                "type": type_comp.type_id.get() as i32,
                "x": pos.x,
                "y": pos.y,
                "team": team.team,
                "commandable": commandable,
                "hit_size": hit_size,
                "payload": payload,
                "select_global": select_global,
            }));
        }
        serde_json::to_string(&rows).unwrap_or_else(|_| String::from("[]"))
    }

    /// `MapObjectives.CommandModeObjective.update` (`MapObjectives.java:671-675`):
    /// any live unit whose `EntitySeq` is in `ids`, on `live_default_team()`,
    /// has a `Command` controller with a live order target
    /// (`CommandAI.hasCommand()` = `targetPos != null`).
    pub fn any_unit_has_command(&self, ids: &[i32]) -> bool {
        if ids.is_empty() {
            return false;
        }
        let team = self.live_default_team();
        self.ecs.0.iter_entities().any(|entity_ref| {
            let Some(seq) = entity_ref.get::<EntitySeq>() else {
                return false;
            };
            if !ids.contains(&(seq.0 as i32)) {
                return false;
            }
            if team_of(&self.ecs.0, entity_ref.id()) != Some(team) {
                return false;
            }
            entity_ref
                .get::<ControllerSlot>()
                .is_some_and(|slot| slot.kind == AiKind::Command && slot.target.is_some())
        })
    }

    /// Debug-only live unit dump (`id`, `type`, `team`, `kind`, position,
    /// rotation, health, `can_shoot` and per-mount aim/fire state); MCP probes
    /// only, never saved or synced.
    pub fn unit_debug_json(&self) -> String {
        let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
            return String::from("[]");
        };
        let mut rows: Vec<serde_json::Value> = Vec::new();
        for &entity in &runtime.units {
            let Ok(entity_ref) = self.ecs.0.get_entity(entity) else {
                continue;
            };
            let Some(seq) = entity_ref.get::<EntitySeq>() else {
                continue;
            };
            let type_name = unit_def(&self.ecs.0, &runtime.content, entity)
                .map(|def| def.name.clone())
                .unwrap_or_default();
            let mounts: Vec<serde_json::Value> = entity_ref
                .get::<crate::weapons::UnitWeapons>()
                .map(|weapons| {
                    weapons
                        .mounts
                        .iter()
                        .map(|mount| {
                            serde_json::json!({
                                "shoot": mount.shoot,
                                "aim": [mount.aim_x, mount.aim_y],
                                "rotation": mount.rotation,
                                "target_rotation": mount.target_rotation,
                                "reload": mount.reload,
                                "warmup": mount.warmup,
                                "total_shots": mount.total_shots,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Active status names (G07 T9 readback; append-only field).
            let statuses: Vec<String> = entity_ref
                .get::<crate::entities::comp::unit::comp::StatusComp>()
                .map(|comp| {
                    comp.statuses
                        .iter()
                        .map(|status| {
                            runtime
                                .content
                                .status(status.effect)
                                .map(|def| def.name.clone())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            rows.push(serde_json::json!({
                "id": seq.0 as i32,
                "type": type_name,
                "team": team_of(&self.ecs.0, entity_ref.id()).unwrap_or(0),
                "kind": entity_ref
                    .get::<ControllerSlot>()
                    .map(|slot| slot.kind.name())
                    .unwrap_or("?"),
                "pos": entity_ref.get::<Pos>().map(|pos| [pos.x, pos.y]),
                "rotation": entity_ref.get::<UnitCore>().map(|core| core.rotation),
                // `UnitCore.elevation` (H08 T7 / PAR-0137 evidence).
                "elevation": entity_ref.get::<UnitCore>().map(|core| core.elevation),
                "health": entity_ref.get::<Health>().map(|health| health.health),
                "can_shoot": entity_ref.get::<crate::weapons::UnitState>().map(|state| state.can_shoot),
                "vel": entity_ref.get::<crate::entities::comp::Vel>().map(|vel| [vel.x, vel.y]),
                "statuses": statuses,
                "mounts": mounts,
            }));
        }
        serde_json::to_string(&rows).unwrap_or_else(|_| String::from("[]"))
    }
}

impl Sim {
    /// The controlled player unit entity, if any.
    pub fn player_unit(&self) -> Option<Entity> {
        self.player_unit
    }

    /// Debug seam (H01 T3): kill the unit with `EntitySeq == seq` by zeroing
    /// its health (`UnitComp.killed`; `environment::kill`); returns whether a
    /// live unit was found.
    pub fn debug_kill_unit(&mut self, seq: i64) -> bool {
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
        let Some(mut health) = self.ecs.0.get_mut::<Health>(target) else {
            return false;
        };
        health.health = 0.0;
        health.dead = true;
        true
    }

    /// Debug seam (G01 T19): apply a status by name to a unit selected by
    /// `EntitySeq`; returns false for unknown id/status/entity.
    pub fn debug_apply_status(&mut self, seq: i64, status: &str, ticks: f32) -> bool {
        let Some(status_id) = self
            .unit_runtime()
            .and_then(|runtime| runtime.content.status_id(status))
        else {
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
        crate::combat::damage::status::apply_status(&mut self.ecs.0, target, status_id, ticks);
        true
    }

    /// `boss_json` seam (G01 T19): `Teams.bosses`/`GameState.boss()` for the
    /// HUD boss bar (G03 T9) and the `guardian` hint (G05).
    pub fn boss_json(&self) -> String {
        use crate::content::ctype::Mappable;
        use crate::entities::comp::unit::comp::StatusComp;
        let bosses = self.bosses();
        let runtime = self.unit_runtime();
        let mut names: Vec<String> = Vec::new();
        let mut health_sum = 0.0f32;
        let mut max_sum = 0.0f32;
        let mut first: Option<serde_json::Value> = None;
        for (index, &entity) in bosses.iter().enumerate() {
            let def = runtime.and_then(|runtime| unit_def(&self.ecs.0, &runtime.content, entity));
            let name = def.map(|def| def.name().to_string()).unwrap_or_default();
            if index < 6 {
                names.push(name.clone());
            }
            let health = self.ecs.0.get::<Health>(entity).copied();
            health_sum += health.map_or(0.0, |h| h.health.max(0.0));
            max_sum += health.map_or(0.0, |h| h.max_health);
            if index == 0 {
                let armor_override = self
                    .ecs
                    .0
                    .get::<StatusComp>(entity)
                    .map(|status| status.armor_override);
                let armor = armor_override
                    .filter(|value| *value >= 0.0)
                    .unwrap_or_else(|| def.map_or(0.0, |def| def.armor));
                first = Some(serde_json::json!({
                    "id": self.ecs.0.get::<crate::ecs::EntitySeq>(entity).map_or(0, |seq| seq.0),
                    "type": name,
                    "armor": armor,
                    "flying": crate::combat::bullet::unit_is_flying(&self.ecs.0, entity),
                    "health": health.map_or(0.0, |h| h.health),
                    "max_health": health.map_or(0.0, |h| h.max_health),
                }));
            }
        }
        let value = if max_sum > 0.0 {
            health_sum / max_sum
        } else {
            0.0
        };
        serde_json::json!({
            "count": bosses.len(),
            "names": names,
            "value": value,
            "first": first,
        })
        .to_string()
    }

    /// `Teams.bosses` (`Teams.java:213-223`): live wave-team units with the
    /// `boss` status, in `EntitySeq` order.
    pub fn bosses(&self) -> Vec<Entity> {
        let Some(runtime) = self.unit_runtime() else {
            return Vec::new();
        };
        let Some(boss) = runtime.content.status_id("boss") else {
            return Vec::new();
        };
        let wave_team = crate::game::runtime::rules_of(&self.ecs.0)
            .map_or(runtime.wave_team, |rules| rules.wave_team);
        let mut bosses: Vec<(u64, Entity)> = Vec::new();
        for &entity in &runtime.units {
            if self.ecs.0.get_entity(entity).is_err() {
                continue;
            }
            if team_of(&self.ecs.0, entity) != Some(wave_team) {
                continue;
            }
            if self
                .ecs
                .0
                .get::<Health>(entity)
                .is_none_or(|health| health.dead)
            {
                continue;
            }
            let has_boss = self
                .ecs
                .0
                .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
                .is_some_and(|statuses| statuses.statuses.iter().any(|entry| entry.effect == boss));
            if !has_boss {
                continue;
            }
            let seq = self
                .ecs
                .0
                .get::<crate::ecs::EntitySeq>(entity)
                .map_or(u64::MAX, |seq| seq.0);
            bosses.push((seq, entity));
        }
        sorted_by_seq(bosses.into_iter())
    }

    /// World-pixel position of the player unit (`Player.x/y` mirror).
    pub fn player_unit_position(&self) -> Option<(f32, f32)> {
        let entity = self.player_unit?;
        self.ecs.0.get::<Pos>(entity).map(|pos| (pos.x, pos.y))
    }

    /// Whether a non-zero movement axis has reached the player unit since the
    /// last world load (`HintsFragment.desktopMove` trigger).
    pub fn player_unit_moved(&self) -> bool {
        self.ecs
            .0
            .get_resource::<PlayerInput>()
            .is_some_and(|input| input.moved)
    }
}

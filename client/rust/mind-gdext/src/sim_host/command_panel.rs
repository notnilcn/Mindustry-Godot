// SPDX-License-Identifier: GPL-3.0-only

//! Command-panel and placement-reticle view data for the HUD.
//! Upstream: core/src/mindustry/ui/fragments/PlacementFragment.java (command panel)

use super::*;

/// `PlacementFragment` unit table + command/stance rows (`PlacementFragment.java:443-676`).
///
/// `units` holds `(type id, live command, stance bits)` per selected unit.
/// Types are counted and iterated by ascending type id; `commands` is the
/// union of each type's `UnitType.commands` in that order, deduped
/// (`PlacementFragment.java:564-570`), `stances` the union of
/// `UnitType.getUnitStances`. A row is reported only when its union has more
/// than one entry (`:574`, `:590`). `active` = any unit's live command
/// (explicit, else `UnitType.defaultCommand`) equals it, / any stance bit is
/// set (`:619-630`).
pub(crate) fn command_panel(
    content: &ContentRegistry,
    units: &[(i32, Option<UnitCommandId>, StanceBits)],
) -> serde_json::Value {
    use mind_core::ai::unit_command_runtime::{default_command, get_unit_stances};
    let mut counts: std::collections::BTreeMap<i32, i32> = std::collections::BTreeMap::new();
    let mut ordered: Vec<&(i32, Option<UnitCommandId>, StanceBits)> = units.iter().collect();
    ordered.sort_by_key(|(type_id, _, _)| *type_id);
    let mut commands: Vec<UnitCommandId> = Vec::new();
    let mut stances: Vec<UnitStanceId> = Vec::new();
    let mut active_commands: Vec<UnitCommandId> = Vec::new();
    let mut active_stances: Vec<UnitStanceId> = Vec::new();
    for (type_id, _, _) in units {
        *counts.entry(*type_id).or_default() += 1;
    }
    for (type_id, current, bits) in ordered {
        let Some(def) = content.unit(UnitTypeId::new(*type_id as u16)) else {
            continue;
        };
        for command in &def.commands {
            if !commands.contains(command) {
                commands.push(*command);
            }
        }
        let current = current.or_else(|| default_command(def, content).map(|def| def.id));
        if let Some(command) = current
            && !active_commands.contains(&command)
        {
            active_commands.push(command);
        }
        let mut unit_stances = Vec::new();
        get_unit_stances(def, content, &mut unit_stances);
        for stance in unit_stances {
            if !stances.contains(&stance) {
                stances.push(stance);
            }
            if bits.get(stance) && !active_stances.contains(&stance) {
                active_stances.push(stance);
            }
        }
    }
    let type_rows: Vec<serde_json::Value> = counts
        .iter()
        .map(|(type_id, count)| {
            let name = content
                .unit(UnitTypeId::new(*type_id as u16))
                .map(|def| def.name.clone())
                .unwrap_or_default();
            serde_json::json!({"type": type_id, "name": name, "count": count})
        })
        .collect();
    let command_rows: Vec<serde_json::Value> = if commands.len() > 1 {
        commands
            .iter()
            .filter_map(|id| {
                content.unit_command(*id).map(|def| {
                    serde_json::json!({
                        "name": def.name,
                        "icon": def.icon,
                        "key": def.localized_key,
                        "active": active_commands.contains(id),
                    })
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let stance_rows: Vec<serde_json::Value> = if stances.len() > 1 {
        stances
            .iter()
            .filter_map(|id| {
                content.unit_stance(*id).map(|def| {
                    serde_json::json!({
                        "name": def.name,
                        "icon": def.icon,
                        "key": def.localized_key,
                        "active": active_stances.contains(id),
                    })
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    serde_json::json!({
        "types": type_rows,
        "commands": command_rows,
        "stances": stance_rows,
    })
}

impl MindSimHost {
    /// `MapObjectives.CommandModeObjective` input read (G08 T15): whether any
    /// selected unit holds a command order (`CommandAI.hasCommand()`).
    pub fn any_unit_has_command(&self, ids: &[i32]) -> bool {
        self.sim.any_unit_has_command(ids)
    }

    /// `ObjectiveInputState.command_mode_satisfied` (G04 T4/G08 T15).
    pub fn command_mode_satisfied(&self) -> bool {
        self.world()
            .get_resource::<mind_core::game::runtime::ObjectiveInputState>()
            .is_some_and(|state| state.command_mode_satisfied)
    }

    /// Per-unit command-panel rows: `(type id, live command, stance bits)` for
    /// the live units whose `EntitySeq` is in `ids`. The live command is the
    /// `CommandAI` command component (`UnitCommandComp`, set by
    /// `Sim::apply_live_unit_command`) falling back to `ControllerSlot.command`;
    /// `None` = the type's `defaultCommand` (`CommandAI.currentCommand`).
    pub(crate) fn command_panel_units(
        &self,
        ids: &[i32],
    ) -> Vec<(i32, Option<UnitCommandId>, StanceBits)> {
        use mind_core::ai::controller::{ControllerSlot, UnitCommandComp};
        use mind_core::ai::unit_stance_runtime::{StanceBits, UnitStancesComp};
        use mind_core::content::id::UnitCommandId;
        use mind_core::ecs::EntitySeq;
        use mind_core::entities::comp::unit::comp::UnitTypeComp;
        let Some(runtime) = self.sim.unit_runtime() else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(ids.len());
        for &id in ids {
            let Some(&entity) = runtime.units.iter().find(|&&entity| {
                self.world()
                    .get::<EntitySeq>(entity)
                    .is_some_and(|seq| seq.0 as i32 == id)
            }) else {
                continue;
            };
            let Ok(entity_ref) = self.world().get_entity(entity) else {
                continue;
            };
            let Some(type_comp) = entity_ref.get::<UnitTypeComp>() else {
                continue;
            };
            let command = entity_ref
                .get::<UnitCommandComp>()
                .map(|comp| UnitCommandId::new(comp.0))
                .or_else(|| {
                    entity_ref
                        .get::<ControllerSlot>()
                        .and_then(|slot| slot.command)
                });
            let stances: StanceBits = entity_ref
                .get::<UnitStancesComp>()
                .map(|comp| comp.0)
                .unwrap_or_default();
            out.push((type_comp.type_id.get() as i32, command, stances));
        }
        out
    }

    /// Whether the live command of unit `id` draws its target
    /// (`UnitCommand.drawTarget`; `InputHandler.java:1231`). Rust-only reticle
    /// filter (the command panel probe above reports the command but not the
    /// icon behaviour).
    pub fn command_draws_target(&self, id: i32) -> bool {
        let Some(content) = self.content_registry() else {
            return false;
        };
        self.command_panel_units(&[id])
            .first()
            .is_some_and(|(type_id, command, _)| {
                let Some(unit) = content.unit(UnitTypeId::new(*type_id as u16)) else {
                    return false;
                };
                let command = command.or_else(|| {
                    mind_core::ai::unit_command_runtime::default_command(unit, content)
                        .map(|def| def.id)
                });
                command
                    .and_then(|id| content.unit_command(id))
                    .is_some_and(|def| def.draw_target)
            })
    }

    /// Live command targets (`Sim::command_targets`; G01 T16) as
    /// `(id, world x, world y, is_attack)`. Rust-only feed for the T3 attack
    /// reticle (`InputHandler.drawCommandedTargets`).
    pub fn command_targets(&self) -> Vec<(i32, f32, f32, bool)> {
        self.sim.command_targets()
    }

    /// Drains the per-tick `BuildDamageEvent` log as `(tile x, tile y, team)`
    /// (`Control.java:68-72`; H03 T4). Rust-only: the minimap is the SINGLE
    /// drain consumer (the T3-style `#[func] drain_build_damage_json` probe is
    /// for debug reads only — two consumers would eat each other's rows).
    pub fn drain_build_damage(&mut self) -> Vec<(i32, i32, u8)> {
        self.sim
            .drain_build_damage_events()
            .into_iter()
            .map(|event| (event.x as i32, event.y as i32, event.team))
            .collect()
    }

    /// `Block.offset` of the building at `(x, y)` in world px
    /// (`((size + 1) % 2) * tilesize / 2`); `0` without a building or off-map.
    /// Minimap indicator centring (`MinimapRenderer.java:266-271`).
    pub fn block_offset(&self, x: i32, y: i32) -> f32 {
        let Some(tile) = self.tile_at(x, y) else {
            return 0.0;
        };
        if tile.build.is_none() {
            return 0.0;
        }
        self.content_snapshot
            .as_ref()
            .and_then(|content| content.block(tile.block))
            .map(|def| def.offset)
            .unwrap_or(0.0)
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Live selectable-unit snapshot (`[{id,type,x,y,team,commandable}]`, the
    /// same rows fed to `MindInput`; PAR-0140 probe).
    #[func]
    pub fn selectable_units_json(&self) -> GString {
        GString::from(self.sim.selectable_units_json().as_str())
    }

    /// Debug-only live unit dump: id, type, team, controller kind, position,
    /// rotation, health, `can_shoot` and per-mount fire state. Read-only MCP
    /// probe (`PAR-0088`/`§7`); never saved or synced.
    #[func]
    pub fn unit_debug_json(&self) -> GString {
        GString::from(self.sim.unit_debug_json().as_str())
    }

    /// G01 T16: live command targets for the HUD reticle
    /// (`[{"id":seq,"x":wx,"y":wy,"attack":bool}, ...]`).
    #[func]
    fn command_targets_json(&self) -> GString {
        let rows: Vec<serde_json::Value> = self
            .sim
            .command_targets()
            .into_iter()
            .map(|(id, x, y, attack)| {
                serde_json::json!({
                    "id": id,
                    "x": x,
                    "y": y,
                    "attack": attack,
                })
            })
            .collect();
        crate::gdutil::json::to_gstring(&rows, "[]")
    }

    /// G01 T15: drains the per-tick building-damage log as
    /// `[[tile_x, tile_y, team], ...]`; G03 filters by the player team and
    /// forwards to `MindMinimap.add_indicator`.
    #[func]
    fn drain_build_damage_json(&mut self) -> GString {
        let events = self.sim.drain_build_damage_events();
        let rows: Vec<[f64; 3]> = events
            .iter()
            .map(|event| [event.x as f64, event.y as f64, event.team as f64])
            .collect();
        crate::gdutil::json::to_gstring(&rows, "[]")
    }

    /// Verify seam: the `ObjectiveInputState.command_mode_satisfied` flag.
    #[func]
    pub fn command_mode_objective(&self) -> bool {
        self.command_mode_satisfied()
    }

    // ---- H03 T3 command panel + reticle seams ----

    /// `PlacementFragment` command table probe (`PlacementFragment.java:443-676`).
    /// `ids_json` is the selection id array (`MindInput.selection_json()`);
    /// returns the same JSON [`command_panel`] builds for the GDScript panel.
    #[func]
    pub fn command_panel_json(&self, ids_json: GString) -> GString {
        let ids: Vec<i32> = serde_json::from_str(ids_json.to_string().as_str()).unwrap_or_default();
        let Some(content) = self.content_registry() else {
            return GString::from("{\"types\":[],\"commands\":[],\"stances\":[]}");
        };
        let units = self.command_panel_units(&ids);
        crate::gdutil::json::value_gstring(&command_panel(content, &units))
    }
}

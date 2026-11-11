// SPDX-License-Identifier: GPL-3.0-only

//! Player/team helpers and drop-zone predicates.
//! Upstream: core/src/mindustry/ui/fragments/HudFragment.java (editor team), core/src/mindustry/game/SectorPreset.java (drop zone)

use super::*;

/// `HudFragment.setPlayerTeamEditor(player, team)` gate
/// (`HudFragment.java:714-718`): only in editor mode (`state.isEditor()`) and
/// only for a real `Team` id; `player != null` is the caller's live-player
/// check. Pure so the headless tests can cover the gate without a Godot node.
pub(super) fn editor_team_choice(editor: bool, team: i64, teams: usize) -> Option<u8> {
    if !editor || team < 0 || team as usize >= teams {
        return None;
    }
    Some(team as u8)
}

/// `WaveSpawner.playerNear()` (`WaveSpawner.java:58-60`): spawns exist, the
/// player is alive, a spawn tile is within `radius` of the player and the
/// player team is not the wave team. `spawns` are tiles; the player position
/// is world px.
pub fn player_near_drop_zone_predicate(
    spawns: &[(i32, i32)],
    player: Option<(f32, f32)>,
    player_team: u8,
    wave_team: u8,
    radius: f32,
) -> bool {
    if spawns.is_empty() || player_team == wave_team {
        return false;
    }
    let Some((px, py)) = player else {
        return false;
    };
    let unit = mind_core::config::TILESIZE as f32;
    spawns.iter().any(|(sx, sy)| {
        let dx = *sx as f32 * unit - px;
        let dy = *sy as f32 * unit - py;
        dx.hypot(dy) < radius
    })
}

impl MindSimHost {
    /// Binding-driven control state from the sibling `Input`:
    /// `(axis_x, axis_y, shooting, aim_x, aim_y, boosting, mouse_move)`.
    /// `MindInput.player_control` shares the `move_x`/`move_y` axis; with a
    /// player unit it drives the unit instead of the camera
    /// (`DesktopInput.updateMovement`).
    pub(super) fn player_control(&self) -> (f32, f32, bool, f32, f32, bool, bool) {
        if self.input_locked {
            // `InputHandler.locked()`: movement and firing are gated off
            // (PAR-0100).
            return (0.0, 0.0, false, 0.0, 0.0, false, false);
        }
        let Some(input) = self.input.clone() else {
            return (0.0, 0.0, false, 0.0, 0.0, false, false);
        };
        input.bind().player_control()
    }

    /// Player unit world position (`Player` mirror) for the camera follow.
    pub fn player_unit_position(&self) -> Option<(f32, f32)> {
        self.sim.player_unit_position()
    }

    /// `BuilderComp.activelyBuilding` for the player unit: the `canShoot` gate
    /// the world input checks before latching `player.shooting`.
    pub fn player_actively_building(&self) -> bool {
        self.sim.player_actively_building()
    }

    /// `player.unit().plans.size` (`BuilderComp.plans`; the `buildautopause`
    /// auto-pause/resume and the deselect-mining gate).
    pub fn player_plan_count(&self) -> usize {
        self.sim.player_plan_count()
    }

    /// `player.unit().type.canBoost` and the unit is alive
    /// (`DesktopInput.java:300-311`): the command-mode gate when `commandMode`
    /// and `boost` share one key.
    pub fn player_unit_can_boost(&self) -> bool {
        use mind_core::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
        let Some(entity) = self.sim.player_unit() else {
            return false;
        };
        if self
            .world()
            .get::<UnitCore>(entity)
            .is_some_and(|core| core.dead)
        {
            return false;
        }
        self.world()
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| {
                self.content_registry()
                    .and_then(|content| content.unit(comp.type_id))
            })
            .is_some_and(|def| def.can_boost)
    }

    /// `build.isCommandable()` own commandable buildings (`DI:346-357`).
    pub fn commandable_buildings(&self) -> Vec<(i16, i16)> {
        self.sim.commandable_buildings(self.sim.live_default_team())
    }

    /// `InputHandler.tryRepairDerelict` (H08 T11): queue a team-0 building in
    /// the rect/cursor for repair (the click and rebuild-select paths).
    pub fn try_repair_derelict(&mut self, x: i32, y: i32) -> bool {
        self.sim.try_repair_derelict(x, y)
    }

    /// `tryDropItems` deposit check (`InputHandler.java:2286-2308`):
    /// `Some(true)` when the building at the tile accepts the held stack,
    /// `None` when Java takes the `Call.dropItem` branch.
    pub fn player_can_deposit_at(&self, x: i32, y: i32) -> Option<bool> {
        let (x, y) = crate::gdutil::coords::tile_i16(x, y)?;
        (self
            .sim
            .player_deposit_accepted(mind_core::world::TilePos::new(x, y))
            > 0)
        .then_some(true)
    }

    /// Team a local placement carries (`state.rules.defaultTeam`; the live
    /// campaign session wins over the build-guard fallback).
    pub fn player_team(&self) -> u8 {
        self.campaign_runtime()
            .map(|runtime| runtime.session.rules.default_team)
            .or_else(|| {
                self.world()
                    .get_resource::<mind_core::world::limits::BuildRules>()
                    .map(|rules| rules.default_team)
            })
            .unwrap_or(mind_core::ecs::TeamId::SHARDED.0)
    }

    /// Applies a relay-assigned team to the local player (PAR-0600).
    ///
    /// Same effect as [`Self::set_player_team`] without the editor gate: the
    /// module's PvP balancer (`assign_team`, GAPS #33) assigns a team on
    /// join/start, and `PlayerComp.team(team)` (`PlayerComp.java:266-271`) must
    /// move the local player and its possessed unit. The rules `defaultTeam`
    /// mirror moves too, because [`Self::player_team`] (and
    /// `Sim::live_default_team` for the placement ghost) read it as
    /// `player.team()`.
    ///
    /// Returns `false` for an out-of-range team (no live world is rejected by
    /// the caller: a host without a sim has nothing to adopt).
    pub fn set_player_team_from_relay(&mut self, team: i64) -> bool {
        let teams = mind_core::game::team::Team::all().len();
        if team < 0 || team as usize >= teams {
            log::warn!("set_player_team_from_relay: team {team} out of range");
            return false;
        }
        let team = team as u8;
        self.sim.set_relay_player_team(team);
        if let Some(mut runtime) = self.campaign_runtime_mut() {
            runtime.session.rules.default_team = team;
        }
        if let Some(mut rules) = self
            .world_mut()
            .get_resource_mut::<mind_core::world::limits::BuildRules>()
        {
            rules.default_team = team;
        }
        log::info!("set_player_team_from_relay: relay player team {team}");
        true
    }

    /// `InputHandler.canMine` for the player unit (world-input seam).
    pub fn player_can_mine(&self, x: i16, y: i16) -> bool {
        self.sim.player_can_mine(x, y)
    }

    /// The player unit's ordered mine tile (world-input seam).
    pub fn player_mine_target(&self) -> Option<(i16, i16)> {
        self.sim.player_mine_tile().map(|tile| (tile.x(), tile.y()))
    }

    /// Sets/clears the player mine order (`tryBeginMine`/`tryStopMine`).
    pub fn set_player_mine(&mut self, tile: Option<(i16, i16)>) {
        self.sim
            .set_player_mine_tile(tile.map(|(x, y)| mind_core::world::TilePos::new(x, y)));
    }

    /// Live sector rules when available (campaign session first, then the last
    /// `load_sector` rules), else defaults.
    pub(super) fn render_rules_or_default(&self) -> Rules {
        self.live_rules().unwrap_or_default()
    }

    /// Live sector rules for the renderer's per-frame feed (G03 T12):
    /// campaign session first, else the last `load_sector` rules.
    pub fn live_rules(&self) -> Option<Rules> {
        self.campaign_runtime()
            .map(|runtime| runtime.session.rules.clone())
            .or_else(|| self.sector_rules.clone())
    }

    /// `state.isCampaign()` (`GameState.java:60-62`: `rules.sector != null`)
    /// for the config panels' `unlockedNow()` short-circuit
    /// (`UnlockableContent.java:281-283`): outside a campaign every content
    /// option is unlocked.
    pub fn in_campaign(&self) -> bool {
        self.campaign_runtime()
            .is_some_and(|runtime| runtime.session.sector.is_some())
    }

    /// `state.getPlanet()` (`GameState.java:85-87`) for the config panels'
    /// `isOnPlanet` filter: the campaign session's sector planet, else
    /// `rules.planet`'s default (`Rules.java:249`, `serpulo`).
    pub fn config_planet(&self) -> Option<mind_core::content::PlanetId> {
        if let Some(runtime) = self.campaign_runtime()
            && let Some((planet, _)) = runtime.session.sector
        {
            return Some(planet);
        }
        self.content_snapshot
            .as_ref()
            .and_then(|content| content.planet_id("serpulo"))
    }

    /// The player unit's carried stack as `(item name, amount)` (`unit.stack`).
    pub fn player_unit_stack(&self) -> Option<(String, i32)> {
        let (item, amount) = self.sim.player_unit_stack()?;
        let name = self
            .sim
            .unit_runtime()
            .and_then(|runtime| runtime.content.item(item))
            .map(|def| def.name.clone())?;
        Some((name, amount))
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// `InputHandler.locked()` push from `MindInput` (PAR-0100): while set, the
    /// player axis is forced to zero.
    #[func]
    pub fn set_input_locked(&mut self, locked: bool) {
        self.input_locked = locked;
        if locked {
            self.sim
                .set_player_control(0.0, 0.0, false, 0.0, 0.0, false, false);
        }
    }

    /// `HudFragment.setPlayerTeamEditor(player, team)`
    /// (`HudFragment.java:714-718`): `state.isEditor() && player != null` ->
    /// `player.team(team)`. That is `PlayerComp.team(team)`
    /// (`PlayerComp.java:266-271`): the player's team and the possessed unit's
    /// team. The port's local read model is [`Self::player_team`], so the
    /// editor's rules `defaultTeam` moves with it (placement/ownership checks
    /// all read `player.team()` upstream). No-op outside editor mode.
    #[func]
    pub fn set_player_team(&mut self, team: i64) -> bool {
        let editor = self
            .world()
            .get_resource::<mind_core::world::limits::BuildRules>()
            .is_some_and(|rules| rules.editor)
            || self.live_rules().is_some_and(|rules| rules.editor);
        if !editor {
            return false;
        }
        let teams = mind_core::game::team::Team::all().len();
        let Some(team) = editor_team_choice(editor, team, teams) else {
            return false;
        };
        // `PlayerComp.team(team)`: `if(unit != null) unit.team(team)`.
        if let Some(unit) = self.sim.player_unit()
            && let Some(mut comp) = self
                .world_mut()
                .get_mut::<mind_core::entities::comp::TeamComp>(unit)
        {
            comp.team = team;
        }
        // The reselect/retry-spawn team (`PlayerComp.team` field).
        self.sim.set_player_team(team);
        // `player_team()` reads the live rules' `defaultTeam`; keep the editor's
        // selected team authoritative for placement and unit ownership.
        if let Some(mut runtime) = self.campaign_runtime_mut() {
            runtime.session.rules.default_team = team;
        }
        if let Some(mut rules) = self
            .world_mut()
            .get_resource_mut::<mind_core::world::limits::BuildRules>()
        {
            rules.default_team = team;
        }
        log::info!("set_player_team: editor player team {team}");
        true
    }

    /// Spawns the campaign player unit at the best core (`CoreBlock.playerSpawn`
    /// → `UnitTypes.alpha`, rotation 90). Idempotent per world load.
    #[func]
    pub fn spawn_player_unit(&mut self) -> bool {
        if self.sim.player_unit_position().is_some() {
            return true;
        }
        // `CoreBlock.playerSpawn` spawns on `rules.defaultTeam`
        // (`Team.sharded` = 1 for the campaign, not `TeamId::SHARDED`) with
        // the best core's `unitType` (`CoreBlock.java:95-125`).
        let team = self
            .last_sector_rules()
            .map_or(1, |rules| rules.default_team);
        // `PlayerComp.team` is set before the spawn attempt so a failed first
        // spawn (no supported core yet) is retried by
        // `update_player_respawn` every 60 ticks (PlayerComp.java:44-48).
        self.sim.set_player_team(team);
        let spawned = self.sim.spawn_player_at_best_core(team).is_some();
        if spawned {
            log::info!("spawn_player_unit: core unit team {team} spawned");
        } else {
            log::warn!("spawn_player_unit: no supported core in the loaded sector");
        }
        spawned
    }

    /// Whether the player possesses a unit (`Player.unit() != null`).
    #[func]
    pub fn has_player_unit(&self) -> bool {
        self.sim.player_unit().is_some()
    }

    /// HUD status-table read model for the player unit (`player.dead()`,
    /// `unit().healthf()`, `player.icon()` type name): `{dead, healthf, type}`.
    /// `dead` is true (healthf 0) when the player has no unit.
    #[func]
    pub fn player_unit_status(&self) -> VarDictionary {
        use mind_core::entities::comp::Health;
        use mind_core::entities::comp::unit::comp::StatusComp;
        use mind_core::entities::comp::unit::comp::UnitTypeComp;
        let mut dict = VarDictionary::new();
        let entity = self.sim.player_unit();
        let world = self.world();
        let health = entity.and_then(|e| world.get::<Health>(e));
        let type_name = entity
            .and_then(|e| world.get::<UnitTypeComp>(e))
            .and_then(|t| self.content_snapshot.as_ref()?.unit(t.type_id))
            .map(|def| def.name.clone())
            .unwrap_or_default();
        let dead = health.is_none_or(|h| h.dead);
        let healthf = health
            .filter(|h| h.max_health > 0.0)
            .map_or(0.0, |h| h.health / h.max_health);
        // `HudFragment.addInfoTable` (`:1205-1231`): applied status effects
        // with their remaining duration (view-only entries, G03 T9).
        let statuses: Vec<serde_json::Value> = entity
            .and_then(|e| world.get::<StatusComp>(e))
            .map(|comp| {
                comp.statuses
                    .iter()
                    .map(|entry| {
                        let name = self
                            .content_snapshot
                            .as_ref()
                            .and_then(|content| content.status(entry.effect))
                            .map(|def| def.name.clone())
                            .unwrap_or_default();
                        serde_json::json!({
                            "name": name,
                            "duration": entry.duration,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        dict.set("dead", dead);
        dict.set("healthf", if dead { 0.0 } else { healthf });
        dict.set("type", &GString::from(type_name.as_str()));
        dict.set(
            "statuses",
            &crate::gdutil::json::value_gstring(&serde_json::Value::Array(statuses)),
        );
        dict
    }

    // ---- G03 T9 HUD seams ----

    /// Player-team cores `[{x, y, health, max}]` in row-major order (G03 T9):    /// the core-attack banner sums the healths and pans to the core whose
    /// health dropped last. Java fires `Trigger.teamCoreDamage` on
    /// `CoreBuild.damage` (`CoreBlock.java:842-848`); the port diffs these
    /// values between 5 Hz refreshes (documented deviation).
    #[func]
    pub fn player_core_healths_json(&self) -> GString {
        use mind_core::entities::comp::health::Health;
        let team = self.player_team();
        let Some(content) = self.content_registry() else {
            return crate::gdutil::json::empty_array();
        };
        let mut rows: Vec<serde_json::Value> = Vec::new();
        let mut seen = Vec::new();
        let unit = mind_core::config::TILESIZE as f32;
        for (pos, index) in self.sim.grid.iter_row_major() {
            let Some(def) = content.block(self.sim.grid.block_id_at(index)) else {
                continue;
            };
            if def.kind != BlockKind::CoreBlock {
                continue;
            }
            let Some(entity) = self.sim.grid.tile(pos.x() as i32, pos.y() as i32).build else {
                continue;
            };
            if seen.contains(&entity) {
                continue;
            }
            if self.building_team(pos.x() as i32, pos.y() as i32) != Some(team) {
                continue;
            }
            seen.push(entity);
            let anchor_x = (pos.x() as i32 - def.size_offset) as f32 * unit + def.offset;
            let anchor_y = (pos.y() as i32 - def.size_offset) as f32 * unit + def.offset;
            let (health, max) = self
                .world()
                .get::<Health>(entity)
                .map(|health| (health.health, health.max_health))
                .unwrap_or((0.0, 0.0));
            rows.push(serde_json::json!({
                "x": anchor_x,
                "y": anchor_y,
                "health": health,
                "max": max,
            }));
        }
        crate::gdutil::json::value_gstring(&serde_json::Value::Array(rows))
    }

    /// `WaveSpawner.playerNear()` (`WaveSpawner.java:58-60`, G03 T9): spawns
    /// exist, the player unit is alive, a spawn tile is within
    /// `rules.dropZoneRadius` of the player and the player team is not the
    /// wave team.
    #[func]
    pub fn player_near_drop_zone(&self) -> bool {
        let spawns: Vec<(i32, i32)> = self
            .sim
            .unit_runtime()
            .map(|runtime| {
                runtime
                    .wave_spawner
                    .spawns
                    .iter()
                    .map(|pos| (pos.x() as i32, pos.y() as i32))
                    .collect()
            })
            .unwrap_or_default();
        let rules = self.render_rules_or_default();
        player_near_drop_zone_predicate(
            &spawns,
            self.sim.player_unit_position(),
            self.player_team(),
            rules.wave_team,
            rules.drop_zone_radius,
        )
    }

    /// Debug probe: `{name, x, y, team, hp_fract}` rows for every live unit
    /// (render verification; companion to `tile_debug`).
    #[func]
    pub fn unit_debug_positions(&self) -> GString {
        let rows: Vec<serde_json::Value> = self
            .unit_draws_full()
            .iter()
            .map(|unit| {
                serde_json::json!({
                    "name": unit.name,
                    "x": unit.x,
                    "y": unit.y,
                    "team": unit.team,
                    "hp_fract": unit.healthf,
                })
            })
            .collect();
        crate::gdutil::json::value_gstring(&serde_json::Value::Array(rows))
    }

    /// Debug probe (J04 T1): the turret's resolved targeting state at `(x, y)`
    /// — `{sort, min_range, tracking_range, target_interval,
    /// new_target_interval, target}` where `target` is `{seq, name, x, y}` or
    /// null. Companion to `tile_debug`.
    #[func]
    pub fn turret_debug(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        use mind_core::combat::targeting::UnitSort;
        use mind_core::world::blocks::defense::turrets::TurretState;

        let mut d = Dictionary::new();
        let Some(tile) = self.sim.grid.tiles.getn(x, y) else {
            return d;
        };
        let Some(entity) = tile.build else {
            return d;
        };
        let Some(state) = self.world().get::<TurretState>(entity) else {
            return d;
        };
        let config = &state.config;
        d.set(
            "sort",
            &GString::from(match config.unit_sort {
                UnitSort::Closest => "closest",
                UnitSort::Farthest => "farthest",
                UnitSort::Strongest => "strongest",
                UnitSort::Weakest => "weakest",
                UnitSort::Grouped(_) => "grouped",
            })
            .to_variant(),
        );
        d.set("min_range", &config.min_range.to_variant());
        d.set("tracking_range", &config.tracking_range.to_variant());
        d.set("target_interval", &config.target_interval.to_variant());
        d.set(
            "new_target_interval",
            &config.new_target_interval.to_variant(),
        );
        d.set("total_ammo", &state.total_ammo.to_variant());
        d.set("ammo_entries", &(state.ammo.len() as i64).to_variant());
        d.set("max_ammo", &config.max_ammo.to_variant());
        let accepts_copper = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.item_id("copper"))
            .is_some_and(|item| {
                mind_core::world::blocks::defense::turrets::accept_item(self.world(), entity, item)
            });
        d.set("accepts_copper", &accepts_copper.to_variant());
        match state.target {
            Some(target) => {
                let mut row = Dictionary::<GString, Variant>::new();
                row.set(
                    "seq",
                    &(self.sim.ecs.seq_of(target).unwrap_or(u64::MAX) as i64).to_variant(),
                );
                let name = self
                    .world()
                    .get::<mind_core::entities::comp::unit::comp::UnitTypeComp>(target)
                    .and_then(|comp| {
                        self.content_snapshot
                            .as_ref()
                            .and_then(|content| content.unit(comp.type_id))
                    })
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                row.set("name", &GString::from(name.as_str()).to_variant());
                if let Some(pos) = self.world().get::<mind_core::entities::comp::Pos>(target) {
                    row.set("x", &pos.x.to_variant());
                    row.set("y", &pos.y.to_variant());
                }
                d.set("target", &row.to_variant());
            }
            None => d.set("target", &Variant::nil()),
        }
        d
    }

    /// Player unit world position as `[x, y]`, or empty when absent (render
    /// verification probe).
    #[func]
    pub fn player_unit_pos(&self) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        if let Some(entity) = self.sim.player_unit()
            && let Some(pos) = self.world().get::<mind_core::entities::comp::Pos>(entity)
        {
            out.push(pos.x);
            out.push(pos.y);
        }
        out
    }

    /// Whether the move axis reached the player unit
    /// (`HintsFragment.desktopMove` trigger).
    #[func]
    pub fn player_unit_moved(&self) -> bool {
        self.sim.player_unit_moved()
    }

    /// The player unit's ordered mine tile as `[x, y]`, or `[]` when idle.
    #[func]
    pub fn player_mine_tile(&self) -> Vec<i32> {
        self.sim
            .player_mine_tile()
            .map(|tile| vec![tile.x() as i32, tile.y() as i32])
            .unwrap_or_default()
    }

    /// Total ore produced by the player unit (tutorial counter evidence).
    #[func]
    pub fn player_mined_count(&self) -> i64 {
        self.sim.player_mined_count()
    }

    /// Playtest seam: the player unit's stack as `{"item":..,"amount":..}` or
    /// `{}` when empty.
    #[func]
    pub fn player_stack_json(&self) -> GString {
        match self.player_unit_stack() {
            Some((item, amount)) => GString::from(
                serde_json::json!({
                    "item": item,
                    "amount": amount,
                })
                .to_string()
                .as_str(),
            ),
            None => crate::gdutil::json::empty_object(),
        }
    }
}

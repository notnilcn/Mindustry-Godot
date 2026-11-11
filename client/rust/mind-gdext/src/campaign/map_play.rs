// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map play flows: custom games, playtests and joined worlds.
//! Upstream: core/src/mindustry/core/Control.java (playMap); core/src/mindustry/core/World.java (loadMap)

use super::saves::sanitize_slot_name;
use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Starts a custom map with a gamemode, applying the map's own rules
    /// (`Control.playMap`, `Control.java:400-426`).
    ///
    /// `map` is a map display name or file path (the `MindPreview` registry
    /// rows); `mode` is a `Gamemode` name; `rules_json` is empty for the map's
    /// own `applyRules(mode)` or the edited rules JSON from the Custom Rules
    /// dialog; `playtest` records the map for the editor's
    /// `resumeAfterPlaytest` path. Returns `false` (with an `@map.invalid` /
    /// `map.nospawn*` error shown) when the map is unknown, the world fails to
    /// load, or the map fails `World.loadMap`'s core checks
    /// (`World.java:366-390`).
    #[func]
    pub fn start_map_mode(
        &mut self,
        map: GString,
        mode: GString,
        playtest: bool,
        rules_json: GString,
    ) -> bool {
        let map_name = map.to_string();
        let mode_name = mode.to_string();
        let Some(header) = self.resolve_map(&map_name) else {
            return self.fail_map(&format!("start_map_mode: unknown map `{map_name}`"));
        };
        let Some(mode_id) = Gamemode::from_name(&mode_name) else {
            return self.fail_map(&format!("start_map_mode: unknown mode `{mode_name}`"));
        };
        if self.campaign.is_none() {
            return self.fail_map("start_map_mode: campaign unavailable");
        }
        let path = header.file.display().to_string();
        // `logic.reset(); world.loadMap(map, rules)` (`Control.java:405-406`).
        let loaded = self
            .host()
            .map(|mut host| host.bind_mut().load_map_world(GString::from(path.as_str())))
            .unwrap_or(false);
        if !loaded {
            self.show_error("@map.invalid");
            return false;
        }
        let rules_text = rules_json.to_string();
        let rules = if rules_text.trim().is_empty() {
            let hooks = WaveMapHooks::new();
            header.apply_rules(mode_id, &hooks)
        } else {
            match mind_core::io::json::JsonIo::read::<Rules>(&rules_text) {
                Ok(rules) => rules,
                Err(error) => {
                    return self.fail_map(&format!("start_map_mode: bad rules JSON: {error}"));
                }
            }
        };
        // `World.loadMap`'s validity checks (`World.java:366-390`). `checkRules`
        // is the requested mode set; the pvp/attack branches read
        // `state.rules`, which `Control.java:407` still holds as the map's own
        // rules at that point (the load assigned them).
        let map_rules = self
            .host()
            .and_then(|host| host.bind().last_sector_rules().cloned())
            .unwrap_or_default();
        let core_teams = self
            .host()
            .map(|host| host.bind().core_teams())
            .unwrap_or_default();
        let cores_of = |team: u8| {
            self.host()
                .map(|host| host.bind().player_core_blocks(team))
                .unwrap_or(0)
        };
        if !rules.pvp && cores_of(rules.default_team) == 0 {
            let team = mind_core::game::team::Team::get(rules.default_team).colored_name();
            return self.fail_map_key(
                &format!(
                    "start_map_mode: `{map_name}` has no core for team {}",
                    rules.default_team
                ),
                "map.nospawn",
                &[team.as_str()],
            );
        } else if rules.pvp && core_teams.len() < 2 {
            return self.fail_map_key(
                &format!("start_map_mode: `{map_name}` has fewer than two core teams"),
                "map.nospawn.pvp",
                &[],
            );
        } else if rules.attack_mode && cores_of(map_rules.wave_team) == 0 {
            let team = mind_core::game::team::Team::get(map_rules.wave_team).colored_name();
            return self.fail_map_key(
                &format!("start_map_mode: `{map_name}` has no core for the wave team"),
                "map.nospawn.attack",
                &[team.as_str()],
            );
        }
        // `rules.retainContentFields(oldRules); state.rules = rules` happens
        // inside `play_map` (invariant 6), then `logic.play()` + `Trigger.newGame`.
        if !self.start_loaded_world(rules) {
            return false;
        }
        self.current_map_name = Some(header.name().to_string());
        // `logic.reset()` (`state = new GameState()`) clears an earlier
        // `playtestingMap`; `Control.java:407` records it for a playtest only.
        self.playtesting_map = playtest.then(|| path.clone());
        if !playtest && settings_hub::row_bool("savecreate") {
            // `Control.playMap`'s `savecreate` slot (`Control.java:418-420`):
            // `map.name() + " " + new SimpleDateFormat("MMM dd h:mm")`. The
            // bare `getBool("savecreate")` reads the `checkPref("savecreate",
            // true)` default (`SettingsMenuDialog.java:404`).
            let name = format!("{} {}", header.name(), save_date_stamp());
            if !self.save_slot(GString::from(name.as_str())) {
                log::warn!("start_map_mode: savecreate slot `{name}` failed");
            } else {
                // `Saves.addSave` -> `SaveSlot.save()`: saving while a game is
                // running makes the fresh slot `current` (`Saves.java` `save()`
                // sets `current` when `state.isGame()`), and `isAutosave()`
                // defaults to true (`Saves.java:445-447`). The pause menu's
                // `@save.quit` label and `runExitSave` both read the current
                // slot (`PausedDialog.java:116`).
                let file = self.slot_file(&sanitize_slot_name(&name));
                settings_hub::with_settings(|store| {
                    let index = self.saves.set_current_by_file(store, &file);
                    if let Some(slot) = self.saves.get_save_slots().get(index) {
                        slot.set_name(store, &name);
                        slot.set_autosave(store, true);
                    }
                });
            }
        }
        true
    }

    /// The map recorded by the last playtest start (`state.playtestingMap`);
    /// the editor's `resumeAfterPlaytest` reads it. `""` when the last start
    /// was not a playtest.
    #[func]
    pub fn playtesting_map(&self) -> GString {
        GString::from(self.playtesting_map.as_deref().unwrap_or(""))
    }

    /// Starts a custom map by name (`Logic.playMap`, survival rules).
    /// Superseded by [`Self::start_map_mode`] for the map-play dialog.
    #[func]
    pub fn start_map(&mut self, name: GString) -> bool {
        self.start_map_mode(name, GString::from("survival"), false, GString::from(""))
    }

    /// Starts the session over a world loaded by `SimHost.load_map_world`
    /// (I08 T5 join): the rules come from the host snapshot header and the
    /// `Control.playMap` control half runs through [`Self::start_loaded_world`].
    #[func]
    pub fn start_joined_world(&mut self, rules_json: GString) -> bool {
        // `logic.reset()` (`state = new GameState()`): not a playtest.
        self.playtesting_map = None;
        let text = rules_json.to_string();
        if text.trim().is_empty() {
            log::warn!("start_joined_world: empty rules JSON");
            return false;
        }
        let rules = match mind_core::io::json::JsonIo::read::<Rules>(&text) {
            Ok(rules) => rules,
            Err(error) => {
                log::warn!("start_joined_world: bad rules JSON: {error}");
                return false;
            }
        };
        // A joined world is never a campaign sector; clear the custom-map name
        // so a later host uses the live identity (I08 T4).
        self.current_map_name = None;
        self.start_loaded_world(rules)
    }
}

impl MindCampaign {
    /// `start_map_mode`'s invalid-map exit: warn, show `@map.invalid`, fail.
    fn fail_map(&self, message: &str) -> bool {
        log::warn!("{message}");
        self.show_error("@map.invalid");
        false
    }

    /// `start_map_mode`'s `World.loadMap` validity exit: warn, show the
    /// `map.nospawn*` key, restore the menu, fail.
    fn fail_map_key(&self, message: &str, key: &str, args: &[&str]) -> bool {
        log::warn!("{message}");
        self.show_error_key(key, args);
        self.set_menu_visible(true);
        false
    }

    /// `Control.playMap`'s control half over an already-loaded world
    /// (`Control.java:407-426`): adopts `rules` as the session rules, runs the
    /// play flow, installs the session runtime and spawns the player. Factored
    /// so I08's host/join paths reuse it.
    pub fn start_loaded_world(&mut self, rules: Rules) -> bool {
        let Some(campaign) = self.campaign.clone() else {
            log::warn!("start_loaded_world: campaign unavailable");
            return false;
        };
        let events = play_map(&mut self.session, rules, None, |_| false, &mut self.epoch);
        if events.is_empty() {
            return false;
        }
        let mut installed = false;
        if let Some(mut host) = self.host() {
            let mut runtime = CampaignRuntime::new(self.session.clone(), campaign)
                // Same shared service (GAPS #68): a custom map match counts
                // into the one tracker too.
                .with_service(self.service.clone());
            if let Some(registry) = self.registry.take() {
                runtime.set_unlocked_content(tech_tree::unlocked_content_names(&registry));
                host.bind_mut().install_campaign_runtime(runtime);
                // A custom map has no save to resume: the launch path applies
                // the loadout (`Logic.play` starting items; `Logic.java:277-285`).
                let sync = host.bind_mut().sync_campaign_session(&registry, true);
                self.registry = Some(registry);
                if sync.is_some() {
                    let _ = with_host_runtime(self.host(), |runtime| {
                        self.session = runtime.session.clone();
                    });
                    installed = true;
                }
            }
        }
        // `Trigger.newGame` (`Control.java:198-222`): spawn the player at the
        // best core so `player.unit()` is live after the launch.
        if installed
            && let Some(mut host) = self.host()
            && !host.bind_mut().spawn_player_unit()
        {
            log::warn!("start_loaded_world: player unit spawn failed");
        }
        // The core drops in with the 160-tick landing cutscene unless
        // `skipcoreanimation` or a PvP rule set (PAR-0131).
        let new_game = events
            .iter()
            .any(|event| matches!(event, mind_core::game::play::PlayEvent::NewGame));
        if installed
            && new_game
            && !settings_hub::with_settings(|store| store.get_bool("skipcoreanimation", false))
            && !self.session.rules.pvp
            // Read the core position in its own statement: a `host.bind()`
            // temporary in the let-chain would stay borrowed across the camera
            // call below (re-entrant `bind_mut` panic).
            && let Some((core_x, core_y, core_block)) = self.host().and_then(|host| {
                let guard = host.bind();
                guard
                    .best_core_position_px()
                    .map(|(x, y)| (x, y, guard.best_core_block()))
            })
            && let Some(mut camera) = wiring::optional(&self.camera)
        {
            camera.bind_mut().show_landing(core_x, core_y, core_block);
            // G07 T7: `Trigger.newGame` audio — `coreLand.at(camera)` +
            // `Musics.land` (`Control.java:198-222`).
            if let Some(mut audio) = wiring::optional(&self.audio) {
                audio.bind_mut().play_landing();
            }
        }
        self.runtime_installed = installed;
        if let Some(host) = self.host() {
            self.last_pump_tick = host.bind().get_tick().max(0) as u64;
        }
        installed
    }

    /// Resolves a map display name or file path through the map registry rows
    /// (`MindPreview`'s `Maps.load` sources).
    fn resolve_map(&self, name: &str) -> Option<mind_core::maps::Map> {
        let maps = crate::assets::loader::load_map_registry(self.paths.root());
        maps.by_name(name).cloned().or_else(|| {
            maps.all()
                .iter()
                .find(|map| map.file.display().to_string() == name)
                .cloned()
        })
    }
}

/// `new SimpleDateFormat("MMM dd h:mm")` (`Control.playMap`'s `savecreate`
/// slot name, `Control.java:419`): short month, zero-padded day, 12-hour clock.
fn save_date_stamp() -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let dict = Time::singleton().get_datetime_dict_from_system();
    let read = |key: &str| {
        dict.get(key)
            .and_then(|value| value.try_to::<i64>().ok())
            .unwrap_or(0)
    };
    let month = read("month").clamp(1, 12) as usize;
    let day = read("day");
    let hour = read("hour");
    let minute = read("minute");
    let hour12 = match hour % 12 {
        0 => 12,
        other => other,
    };
    format!("{} {day:02} {hour12}:{minute:02}", MONTHS[month - 1])
}

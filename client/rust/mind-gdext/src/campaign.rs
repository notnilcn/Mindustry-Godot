// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindCampaign` autoload (plan 12 M8/§3.9): the GDScript-callable campaign
//! facade.
//!
//! Campaign state, play flows, objectives, tech and schematics all live in
//! `mind_core::game`; this node is the thin Godot shell the MCP oracle and the
//! plan-14 dialogs drive. It is declared in `res://scenes/game.tscn` (HLP §6.6,
//! tscn-first) rather than registered as a project autoload singleton.
//!
//! In-engine MCP verification (plan 12 §7c) is deferred to the orchestrator's
//! single-editor mutex; this class compiles and exposes the API surface today.

use godot::builtin::{PackedInt32Array, PackedVector3Array, VarDictionary, Vector3};
use godot::classes::notify::NodeNotification;
use godot::classes::{FileAccess, INode, Node, ProjectSettings};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::content::registries::planets::GeneratorKind;
use mind_core::content::{
    ContentRef, ContentRegistry, ContentType, MemoryBundle, PlanetId, create_base_content,
};
use mind_core::game::campaign_rules::CampaignRules;
use mind_core::game::objectives::CampaignObjectiveContext;
use mind_core::game::planet::EmptyNeighborhood;
use mind_core::game::play::{
    PlaySession, SectorRef, play_map, play_new_sector, play_sector, run_wave_campaign,
    sector_capture,
};
use mind_core::game::rules::Rules;
use mind_core::game::rules_event::{RulesEpoch, apply_set_rules};
use mind_core::game::runtime::CampaignRuntime;
use mind_core::game::saves::Saves;
use mind_core::game::schematics::Schematics;
use mind_core::game::tech_tree::{self, SettingsUnlockStore};
use mind_core::game::universe::{Campaign, TurnContext};
use mind_core::game::world_reloader::HostReloader;
use mind_core::io::FileSystem;
use mind_core::io::fs::{NativeFs, Paths};
use mind_core::io::save::slot::list_save_slots as list_slot_files;
use mind_core::io::settings::SettingsStore;
use mind_core::random::JavaRandom;
use mind_core::render::g3d::grid::PlanetGrid;
use mind_core::render::g3d::mesh_data::build_planet_grid;
use mind_core::world::modules::ItemModule;

use crate::sim_host::MindSimHost;

/// `PlanetRenderer.outlineRad`: the sector-grid shell scale over `radius`.
const OUTLINE_RAD: f32 = 1.17;

/// `MindCampaign` — campaign/sector/rules/tech/schematic facade for MCP + UI.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindCampaign {
    base: Base<Node>,
    session: PlaySession,
    campaign: Option<Campaign>,
    registry: Option<ContentRegistry>,
    settings: SettingsStore,
    paths: Paths,
    saves: Saves,
    epoch: RulesEpoch,
    reloader: HostReloader,
    content_error: Option<String>,
    /// A `CampaignRuntime` is installed in the live sim host.
    runtime_installed: bool,
    /// The last captured sector was the planet's last (campaign victory).
    campaign_complete: bool,
}

#[godot_api]
impl INode for MindCampaign {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            session: PlaySession::default(),
            campaign: None,
            registry: None,
            settings: SettingsStore::new(),
            paths: Paths::new(std::env::temp_dir()),
            saves: Saves::new(),
            epoch: RulesEpoch::new(),
            reloader: HostReloader::default(),
            content_error: None,
            runtime_installed: false,
            campaign_complete: false,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        self.pump_runtime();
    }

    fn exit_tree(&mut self) {
        self.persist();
    }
}

#[godot_api]
impl MindCampaign {
    /// Rebuilds the campaign content (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`). The registry and
    /// campaign are overwritten, so a re-run never duplicates them.
    fn bootstrap(&mut self) {
        // Campaign state lives in the same settings store the sim host writes
        // (`user://config/settings.bin`), so unlocks/rules survive restarts.
        let root = godot::classes::ProjectSettings::singleton()
            .globalize_path("user://")
            .to_string();
        self.paths = Paths::new(root);
        self.settings = SettingsStore::load(&NativeFs, &self.paths);

        let bundle = MemoryBundle::new();
        let store = SettingsUnlockStore::new(&mut self.settings);
        match create_base_content(&bundle, &store, true) {
            Ok(mut registry) => {
                // The facade reads database tabs and derived block/unit fields
                // (`ContentLoader.init`/`postInit` sweeps), so run the same
                // lifecycle the sim host/editor run.
                if let Err(error) = registry.init().and_then(|()| registry.post_init()) {
                    log::error!("MindCampaign content lifecycle failed: {error}");
                }
                let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
                campaign.load_all(&self.settings);
                self.saves = Saves::new();
                self.saves
                    .load(&NativeFs, &self.paths, &registry, &mut self.settings);
                // `Saves.load` assigns sector save files; reflect them on the
                // runtime sectors so launches take the resume branch (C-3).
                for planet in campaign.planets.values_mut() {
                    for sector in &mut planet.sectors {
                        let file = self.paths.sector_save(&planet.name, sector.id as u32);
                        if NativeFs.exists(&file) {
                            sector.save = Some(format!("sector-{}-{}", planet.name, sector.id));
                        }
                    }
                }
                self.registry = Some(registry);
                self.campaign = Some(campaign);
                self.runtime_installed = false;
                log::info!(
                    "MindCampaign ready ({} save slot(s))",
                    self.saves.slots.len()
                );
            }
            Err(error) => {
                self.content_error = Some(error.to_string());
                log::error!("MindCampaign content boot failed: {error}");
            }
        }
    }

    /// The sibling `MindSimHost` that owns the live sim/runtime.
    fn host(&self) -> Option<Gd<MindSimHost>> {
        self.base().try_get_node_as::<MindSimHost>("../SimHost")
    }

    /// Persists campaign/universe/unlock state to `user://config/settings.bin`.
    fn persist(&mut self) {
        if let Some(campaign) = self.campaign.as_ref()
            && let Err(error) = campaign.save_all(&mut self.settings)
        {
            log::warn!("campaign save_all failed: {error}");
        }
        if let Err(error) = self.settings.force_save(&NativeFs, &self.paths) {
            log::warn!("campaign settings flush failed: {error}");
        }
    }

    /// Pumps the live campaign runtime: drains tick events, applies capture
    /// bookkeeping (stats, auto-unlocks, capture replacements), runs due
    /// campaign turns and saves. No-op without a host/runtime.
    fn pump_runtime(&mut self) {
        let Some(mut host) = self.host() else {
            return;
        };
        let Some(mut runtime) = host.bind_mut().take_campaign_runtime() else {
            return;
        };
        let events = runtime.take_events();
        let mut dirty = false;

        let mut registry = self.registry.take();
        let replacements: Vec<(String, String)> = if events.iter().any(|event| {
            matches!(
                event,
                mind_core::game::play::PlayEvent::SectorCapture { .. }
            )
        }) {
            let planet = runtime.session.sector.map(|(planet, _)| planet);
            planet
                .and_then(|planet| {
                    registry
                        .as_ref()
                        .and_then(|registry| registry.planet(planet))
                })
                .map(|planet| planet.sector_capture_replacements.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        for event in &events {
            match event {
                mind_core::game::play::PlayEvent::SectorCapture { .. } => {
                    self.apply_capture_bookkeeping(&mut runtime, registry.as_mut());
                    dirty = true;
                }
                mind_core::game::play::PlayEvent::CampaignComplete { .. } => {
                    self.campaign_complete = true;
                    let _ = self.base_mut().emit_signal("campaign_complete", &[]);
                }
                mind_core::game::play::PlayEvent::GameOver { .. }
                | mind_core::game::play::PlayEvent::SectorLose { .. } => {
                    dirty = true;
                }
                _ => {}
            }
        }
        self.registry = registry;

        if !replacements.is_empty() {
            let replaced = host.bind_mut().apply_capture_replacements(&replacements);
            if replaced > 0 {
                dirty = true;
            }
        }

        // Auto campaign turns (`Universe.update` turnCounter).
        if runtime.turn_due {
            runtime.turn_due = false;
            if let (Some(registry), Some(campaign)) =
                (self.registry.as_ref(), Some(&mut runtime.campaign))
            {
                let turn = campaign.universe.turn;
                let mut rand = JavaRandom::new(turn as u64 + 1);
                let ctx = TurnContext {
                    current_planet: runtime.session.sector.map(|(planet, _)| planet),
                    current_sector: runtime.session.sector,
                    playing_rules: Some((
                        runtime.session.rules.waves,
                        runtime.session.rules.attack_mode,
                    )),
                    current_wave: runtime.session.wave,
                };
                let report = campaign.run_turn(registry, &mut self.settings, &ctx, &mut rand);
                log::info!(
                    "campaign turn {} ({} event(s))",
                    report.turn,
                    report.events.len()
                );
                dirty = true;
            }
        }

        // Mirror the authoritative runtime state into the facade for getters.
        // Scalars/objectives stay fresh every pump; the full session (teams)
        // and campaign clone only when an event actually landed.
        let mirrored = !events.is_empty();
        self.session.wave = runtime.session.wave;
        self.session.wavetime = runtime.session.wavetime;
        self.session.game_over = runtime.session.game_over;
        self.session.won = runtime.session.won;
        self.session.enemies = runtime.session.enemies;
        self.session.spawn_count = runtime.session.spawn_count;
        self.session.is_spawning = runtime.session.is_spawning;
        self.session.sector = runtime.session.sector;
        self.session.attack_after_waves = runtime.session.attack_after_waves;
        self.session.rules = runtime.session.rules.clone();
        self.session.objectives = runtime.session.objectives.clone();
        if mirrored {
            self.session = runtime.session.clone();
            self.campaign = Some(runtime.campaign.clone());
        }
        host.bind_mut().put_campaign_runtime(runtime);
        if dirty {
            self.persist();
        }
    }

    /// `SectorCaptureEvent` listener half: tech auto-unlocks (including
    /// `SectorComplete` objectives) and campaign-complete detection.
    fn apply_capture_bookkeeping(
        &mut self,
        runtime: &mut CampaignRuntime,
        registry: Option<&mut ContentRegistry>,
    ) {
        let Some(registry) = registry else {
            return;
        };
        let ctx = CampaignObjectiveContext::new(registry, &runtime.campaign);
        let mut store = SettingsUnlockStore::new(&mut self.settings);
        let unlocked = tech_tree::check_auto_unlocks(registry, &mut store, &ctx);
        if !unlocked.is_empty() {
            log::info!("campaign capture auto-unlocked {} content", unlocked.len());
        }
    }

    /// Emitted on the first capture of a planet's last sector (campaign
    /// victory); the UI opens `campaign_complete`.
    #[signal]
    fn campaign_complete();

    /// Starts a campaign sector (host-side launch), returning success.
    ///
    /// `loadout_json` is an optional `[{item, amount}]` launch loadout applied
    /// on the fresh-launch path (empty string = default).
    #[func]
    pub fn start_sector(&mut self, planet: GString, sector: i32) -> bool {
        self.start_sector_with_loadout(planet, sector, GString::from(""))
    }

    /// Starts a campaign sector with an explicit launch loadout (WS3 wiring).
    #[func]
    pub fn start_sector_with_loadout(
        &mut self,
        planet: GString,
        sector: i32,
        loadout_json: GString,
    ) -> bool {
        let planet_name = planet.to_string();
        if sector < 0 {
            return false;
        }
        let sector_id = sector as u16;
        let Some(registry) = self.registry.take() else {
            return false;
        };
        let Some(mut campaign) = self.campaign.take() else {
            self.registry = Some(registry);
            return false;
        };

        // C-9/GAP-12: unknown planet/sector and locked presets are rejected.
        let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
            log::warn!("start_sector: unknown planet `{planet_name}`");
            self.registry = Some(registry);
            self.campaign = Some(campaign);
            return false;
        };
        let Some(record) = campaign.sector(planet_id, sector_id) else {
            log::warn!("start_sector: sector {sector_id} out of range on `{planet_name}`");
            self.registry = Some(registry);
            self.campaign = Some(campaign);
            return false;
        };
        let always_unlocked = record.preset_always_unlocked;
        let require_unlock = record.preset_require_unlock;
        let preset_id = record.preset;
        let has_save = record.has_save();
        let tech_unlocked = preset_id.is_none_or(|preset| {
            tech_tree::content_unlocked(&registry, ContentRef::of(ContentType::Sector, preset))
        });
        if require_unlock && !always_unlocked && !tech_unlocked {
            log::warn!("start_sector: `{planet_name}:{sector_id}` is locked");
            self.registry = Some(registry);
            self.campaign = Some(campaign);
            return false;
        }

        // Optional launch loadout (GAP-10).
        if !loadout_json.to_string().trim().is_empty()
            && let Ok(stacks) = serde_json::from_str::<
                Vec<mind_core::io::json::content_serde::JsonItemStack>,
            >(&loadout_json.to_string())
        {
            self.session.rules.loadout = stacks;
        }

        self.reloader = HostReloader::default();
        let events = if has_save {
            play_sector(
                &mut self.session,
                &mut campaign,
                &registry,
                planet_id,
                sector_id,
                None,
                &mut self.epoch,
                &mut self.reloader,
            )
        } else {
            play_new_sector(
                &mut self.session,
                &mut campaign,
                &registry,
                planet_id,
                sector_id,
                None,
                &mut self.epoch,
                &mut self.reloader,
            )
        };
        // Merge the loaded map/generator rules (waves/spawns) captured by
        // `MindSimHost.load_sector` so the session and the world agree.
        if let Some(host) = self.host()
            && let Some(map_rules) = host.bind().last_sector_rules().cloned()
        {
            merge_map_rules(&mut self.session.rules, &map_rules, preset_id.is_some());
        }

        // Install the runtime and register the live cores/spawns (GAP-9).
        let mut installed = false;
        if let Some(mut host) = self.host() {
            let mut runtime = CampaignRuntime::new(self.session.clone(), campaign.clone());
            runtime.set_unlocked_content(collect_unlocked(&registry));
            host.bind_mut().install_campaign_runtime(runtime);
            let sync = host.bind_mut().sync_campaign_session(&registry, !has_save);
            if let Some(sync) = sync {
                if sync.player_cores > 0 {
                    let save_name = format!("sector-{planet_name}-{sector_id}");
                    if let Some(sector) = campaign.sector_mut(planet_id, sector_id) {
                        if sector.save.is_none() {
                            sector.save = Some(save_name);
                        }
                        sector.info.info.has_core = true;
                    }
                }
                let taken = host.bind_mut().take_campaign_runtime();
                if let Some(mut runtime) = taken {
                    self.session = runtime.session.clone();
                    // Carry the launch ownership back into the runtime copy.
                    runtime.campaign = campaign.clone();
                    host.bind_mut().put_campaign_runtime(runtime);
                }
                installed = true;
            }
        }

        self.registry = Some(registry);
        self.campaign = Some(campaign);
        self.runtime_installed = installed;
        // Persist attempts/rules; the live tile save is queued for the next IO
        // tick (`Control.playNewSector` -> `saves.saveSector`).
        self.persist();
        self.invalidate_ui_block_catalog();
        if installed
            && let Some(save_name) = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(planet_id, sector_id))
                .and_then(|sector| sector.save.clone())
            && let Some(mut host) = self.host()
        {
            let path = format!("user://saves/{save_name}.msav");
            host.bind_mut()
                .request_save(GString::from(path.as_str()), false);
        }
        self.runtime_installed && !events.is_empty()
    }

    /// Starts a custom map by name (`Logic.playMap`).
    #[func]
    pub fn start_map(&mut self, name: GString) -> bool {
        let mut rules = self.session.rules.clone();
        rules.mode_name = Some(name.to_string());
        rules.sector = None;
        let events = play_map(&mut self.session, rules, None, |_| false, &mut self.epoch);
        !events.is_empty()
    }

    /// Forces sector capture (`Logic.sectorCapture`), routing through the live
    /// runtime when one is installed so events and bookkeeping stay in sync.
    #[func]
    pub fn capture_sector(&mut self) -> bool {
        let captured = if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                let events = sector_capture(&mut runtime.session, &mut runtime.campaign);
                let ok = !events.is_empty();
                for event in events {
                    runtime.events.push(event);
                }
                host.bind_mut().put_campaign_runtime(runtime);
                ok
            } else {
                false
            }
        } else {
            false
        };
        if captured {
            self.pump_runtime();
            return true;
        }
        // Menu fallback on the facade copy.
        let Some(mut campaign) = self.campaign.take() else {
            return false;
        };
        let events = sector_capture(&mut self.session, &mut campaign);
        self.campaign = Some(campaign);
        !events.is_empty()
    }

    /// Runs one production turn; returns success. Runs on the live runtime
    /// campaign when one is installed.
    #[func]
    pub fn run_turn(&mut self) -> bool {
        let Some(registry) = self.registry.take() else {
            return false;
        };
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                let turn = runtime.campaign.universe.turn;
                let ctx = TurnContext {
                    current_planet: runtime.session.sector.map(|(planet, _)| planet),
                    current_sector: runtime.session.sector,
                    playing_rules: Some((
                        runtime.session.rules.waves,
                        runtime.session.rules.attack_mode,
                    )),
                    current_wave: runtime.session.wave,
                };
                let report = runtime.campaign.run_turn(
                    &registry,
                    &mut self.settings,
                    &ctx,
                    &mut JavaRandom::new(turn as u64 + 1),
                );
                host.bind_mut().put_campaign_runtime(runtime);
                self.registry = Some(registry);
                self.persist();
                return report.turn > 0;
            }
        }
        let Some(mut campaign) = self.campaign.take() else {
            self.registry = Some(registry);
            return false;
        };
        let turn = campaign.universe.turn;
        let report = campaign.run_turn(
            &registry,
            &mut self.settings,
            &TurnContext::default(),
            &mut JavaRandom::new(turn as u64 + 1),
        );
        self.registry = Some(registry);
        self.campaign = Some(campaign);
        self.persist();
        report.turn > 0
    }

    /// Advances the wave counter (`Logic.runWave`), mirroring into the live
    /// runtime when one is installed. The live unit runtime spawns the wave on
    /// the next `TickSet::RunWave` tick (`WaveSpawner.spawnEnemies`).
    #[func]
    pub fn run_wave(&mut self) -> bool {
        let _ = run_wave_campaign(&mut self.session);
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                let _ = run_wave_campaign(&mut runtime.session);
                let wave = runtime.session.wave;
                host.bind_mut().put_campaign_runtime(runtime);
                host.bind_mut().request_wave_spawn(wave - 1);
            }
        }
        true
    }

    /// Current rules as JSON.
    #[func]
    pub fn get_rules_json(&self) -> GString {
        GString::from(
            serde_json::to_string(&self.session.rules)
                .unwrap_or_else(|_| "{}".to_owned())
                .as_str(),
        )
    }

    /// Applies a rules JSON blob; campaign edits require `allowEditRules`.
    #[func]
    pub fn set_rules_json(&mut self, json: GString) -> bool {
        let Ok(incoming) = serde_json::from_str::<mind_core::game::rules::Rules>(&json.to_string())
        else {
            return false;
        };
        let is_campaign = self.session.is_campaign();
        apply_set_rules(
            &mut self.session.rules,
            incoming,
            is_campaign,
            None,
            &mut self.epoch,
        )
        .is_ok()
    }

    /// Researches a content name, spending the active sector's real stored items
    /// (`ResearchDialog.spend`), gated by dependencies/objectives and persisted
    /// through the settings-backed unlock store.
    #[func]
    pub fn research(&mut self, content: GString) -> bool {
        let target = content.to_string();
        let Some(mut registry) = self.registry.take() else {
            return false;
        };
        let Some(content_ref) = registry.tech().nodes.iter().find_map(|node| {
            let content = node.content?;
            (tech_tree::content_name(&registry, content).as_deref() == Some(target.as_str()))
                .then_some(content)
        }) else {
            self.registry = Some(registry);
            return false;
        };
        if tech_tree::content_unlocked_with_rules(&registry, content_ref, &self.session.rules) {
            self.registry = Some(registry);
            return true;
        }
        let Some(node_ref) = registry
            .tech()
            .nodes
            .iter()
            .position(|node| node.content == Some(content_ref))
            .map(|index| mind_core::content::tech::TechNodeRef(index as u32))
        else {
            self.registry = Some(registry);
            return false;
        };

        // Real inventory: the stored items of every owned, non-frozen sector
        // (`ResearchDialog.rebuildItems` aggregates `sector.items()`). The
        // sector being played reads its live core module instead (`Sector.items`
        // -> `state.rules.defaultTeam.items()`), so it is not double-counted.
        let default_team = self.session.default_team();
        let mut items = ItemModule::with_items(registry.items().len());
        if self.session.is_campaign()
            && let Some(host) = self.host()
        {
            for (name, amount) in host.bind().player_core_items(default_team) {
                if let Some(id) = registry.item_id(&name) {
                    items.add(id, amount.max(0), i32::MAX);
                }
            }
        }
        let played_sector = self.session.sector;
        let mut sector_entries: Vec<(SectorRef, Vec<(String, i32)>)> = Vec::new();
        if let Some(campaign) = self.campaign.as_ref() {
            for planet in campaign.planets.values() {
                for sector in &planet.sectors {
                    if !sector.has_base() || sector.is_frozen(None) {
                        continue;
                    }
                    if played_sector == Some((planet.id, sector.id)) {
                        continue;
                    }
                    let entry: Vec<(String, i32)> = sector
                        .info
                        .info
                        .items
                        .iter()
                        .map(|(name, amount)| (name.clone(), *amount))
                        .collect();
                    for (name, amount) in &entry {
                        if let Some(id) = registry.item_id(name) {
                            items.add(id, (*amount).max(0), i32::MAX);
                        }
                    }
                    sector_entries.push(((planet.id, sector.id), entry));
                }
            }
        }

        let campaign_snapshot = self.campaign.clone();
        let Some(campaign_ref) = campaign_snapshot.as_ref() else {
            self.registry = Some(registry);
            return false;
        };
        let ctx = CampaignObjectiveContext::new(&registry, campaign_ref);
        if !tech_tree::can_spend(&registry, node_ref, &items, &ctx) {
            self.registry = Some(registry);
            return false;
        }
        let mut store = SettingsUnlockStore::new(&mut self.settings);
        let outcome =
            tech_tree::spend(&mut registry, node_ref, &mut items, &mut store, &ctx, false);
        let Ok(result) = outcome else {
            self.registry = Some(registry);
            return false;
        };
        let changed = result.complete || !result.spent.is_empty();

        // Deduct the spent stacks: other sectors in order, then the active
        // sector last from its live core module (`ResearchDialog` removal
        // order; `Sector.removeItem` writes the core while playing).
        for spent in &result.spent {
            let Some(name) = registry.item(spent.item).map(|item| item.name.clone()) else {
                continue;
            };
            let mut remaining = spent.amount;
            for ((planet, sector), entry) in &mut sector_entries {
                if remaining <= 0 {
                    break;
                }
                let Some((_, amount)) =
                    entry.iter_mut().find(|(entry_name, _)| entry_name == &name)
                else {
                    continue;
                };
                let used = remaining.min((*amount).max(0));
                *amount -= used;
                remaining -= used;
                if let Some(record) = self
                    .campaign
                    .as_mut()
                    .and_then(|campaign| campaign.sector_mut(*planet, *sector))
                {
                    record.info.info.items.insert(name.clone(), *amount);
                }
            }
            if remaining > 0
                && self.session.is_campaign()
                && let Some(mut host) = self.host()
            {
                remaining -=
                    host.bind_mut()
                        .remove_player_core_items(default_team, &name, remaining);
            }
            let _ = remaining;
        }
        // Newly unlocked content may auto-unlock dependent zero-cost nodes.
        if !result.unlocked.is_empty()
            && let Some(campaign_ref) = self.campaign.as_ref()
        {
            let ctx = CampaignObjectiveContext::new(&registry, campaign_ref);
            let mut store = SettingsUnlockStore::new(&mut self.settings);
            let unlocked = tech_tree::check_auto_unlocks(&mut registry, &mut store, &ctx);
            log::info!("research auto-unlocked {} content", unlocked.len());
        }
        self.registry = Some(registry);
        // Keep the runtime copy's campaign in sync with the spent inventory.
        self.push_campaign_to_runtime();
        if changed {
            self.persist();
            self.invalidate_ui_block_catalog();
        }
        changed
    }

    /// Copies the facade campaign into the installed runtime (post-research /
    /// rule edits).
    fn push_campaign_to_runtime(&mut self) {
        let Some(campaign) = self.campaign.clone() else {
            return;
        };
        let unlocked = self
            .registry
            .as_ref()
            .map(collect_unlocked)
            .unwrap_or_default();
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                runtime.campaign = campaign;
                runtime.set_unlocked_content(unlocked);
                host.bind_mut().put_campaign_runtime(runtime);
            }
        }
    }

    /// Tech-tree summary (`nodes`, `unlocked`, error).
    #[func]
    pub fn get_tech_state(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        match &self.registry {
            Some(registry) => {
                let nodes = registry.tech().nodes.len() as i64;
                let unlocked = registry
                    .tech()
                    .nodes
                    .iter()
                    .filter(|node| {
                        node.content.is_some_and(|content| {
                            tech_tree::content_unlocked_with_rules(
                                registry,
                                content,
                                &self.session.rules,
                            )
                        })
                    })
                    .count() as i64;
                dict.set(&key("nodes"), &nodes.to_variant());
                dict.set(&key("unlocked"), &unlocked.to_variant());
            }
            None => {
                dict.set(&key("nodes"), &0i64.to_variant());
                dict.set(&key("unlocked"), &0i64.to_variant());
            }
        }
        if let Some(error) = &self.content_error {
            dict.set(&key("error"), &error.as_str().to_variant());
        }
        dict
    }

    /// Placement-palette catalog JSON (`PlacementFragment.getUnlockedByCategory`).
    ///
    /// Campaign games filter through the settings-backed unlock store: only
    /// blocks whose `<name>-unlocked` bit is set (or that `check_auto_unlocks`
    /// grants at boot) and whose `BuildVisibility` is buildable survive, and
    /// empty categories are dropped. Custom games expose the full build-menu
    /// inventory, mirroring `unlockedNowHost`'s `!state.isCampaign()` branch.
    #[func]
    pub fn block_catalog_json(&mut self) -> GString {
        let catalog = if self.session.is_campaign() {
            let store = SettingsUnlockStore::new(&mut self.settings);
            mind_core::ui::campaign::block_catalog_unlocked(&store)
        } else {
            mind_core::ui::campaign::block_catalog()
        };
        GString::from(
            serde_json::to_string(&catalog)
                .unwrap_or_else(|_| String::from("{}"))
                .as_str(),
        )
    }

    /// Drops `MindUi`'s cached placement catalog after an unlock change; its
    /// campaign-less fallback reads the persisted settings file directly.
    fn invalidate_ui_block_catalog(&self) {
        let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") else {
            return;
        };
        if ui.has_method("invalidate_block_catalog") {
            let _ = ui.call("invalidate_block_catalog", &[]);
        }
    }

    /// Completes an objective by index (`complete_objective` relay target).
    #[func]
    pub fn complete_objective(&mut self, index: i64) -> bool {
        if index < 0 {
            return false;
        }
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                let completed = runtime
                    .session
                    .objectives
                    .complete(index as usize, &mut runtime.session.rules);
                self.session.objectives = runtime.session.objectives.clone();
                host.bind_mut().put_campaign_runtime(runtime);
                return completed;
            }
        }
        self.session
            .objectives
            .complete(index as usize, &mut self.session.rules)
    }

    /// Per-objective state rows (`index`, `type`, `completed`).
    #[func]
    pub fn get_objective_state(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        for index in 0..self.session.objectives.len() {
            let mut row = VarDictionary::new();
            row.set(&GString::from("index"), &(index as i64).to_variant());
            if let Some(kind) = self.session.objectives.type_name(index) {
                row.set(&GString::from("type"), &kind.as_str().to_variant());
            }
            row.set(
                &GString::from("completed"),
                &self.session.objectives.is_completed(index).to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Current sector/rules summary.
    #[func]
    pub fn get_sector_state(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        dict.set(
            &GString::from("campaign"),
            &self.session.is_campaign().to_variant(),
        );
        dict.set(&GString::from("wave"), &self.session.wave.to_variant());
        dict.set(
            &GString::from("waves"),
            &self.session.rules.waves.to_variant(),
        );
        dict.set(
            &GString::from("attack"),
            &self.session.rules.attack_mode.to_variant(),
        );
        dict.set(
            &GString::from("hasCore"),
            &(self.session.player_core_count() > 0).to_variant(),
        );
        if let Some((planet, sector)) = self.session.sector {
            let name = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.planet(planet))
                .map(|planet| planet.name.clone())
                .unwrap_or_default();
            dict.set(&key("planet"), &name.as_str().to_variant());
            dict.set(&key("sector"), &(sector as i64).to_variant());
            let record = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(planet, sector));
            dict.set(
                &key("wasCaptured"),
                &record
                    .map(|sector| sector.info.info.was_captured)
                    .unwrap_or(false)
                    .to_variant(),
            );
            dict.set(
                &key("hasSave"),
                &record
                    .map(|sector| sector.has_save())
                    .unwrap_or(false)
                    .to_variant(),
            );
            dict.set(
                &key("isLast"),
                &record
                    .map(|sector| sector.is_last_sector())
                    .unwrap_or(false)
                    .to_variant(),
            );
            // `PausedDialog.rebuild` `showObjective`: the preset description
            // feeds the `@objective` full-text dialog.
            let preset_description = record
                .and_then(|sector| sector.preset)
                .and_then(|preset| {
                    self.registry
                        .as_ref()
                        .and_then(|registry| registry.sector(preset))
                })
                .and_then(|preset| preset.unlock.description.clone())
                .unwrap_or_default();
            dict.set(
                &key("presetDescription"),
                &preset_description.as_str().to_variant(),
            );
        }
        dict.set(
            &GString::from("campaignComplete"),
            &self.campaign_complete.to_variant(),
        );
        dict
    }

    /// Fog query placeholder (fog state is plan 12 M7 `FogControl`, not yet
    /// bound to this facade); reports the rules fog flags.
    #[func]
    pub fn fog_query(&self, team: i64, x: i32, y: i32) -> VarDictionary {
        let mut dict = VarDictionary::new();
        dict.set(&GString::from("team"), &team.to_variant());
        dict.set(&GString::from("x"), &x.to_variant());
        dict.set(&GString::from("y"), &y.to_variant());
        dict.set(&GString::from("fog"), &self.session.rules.fog.to_variant());
        dict.set(
            &GString::from("staticFog"),
            &self.session.rules.static_fog.to_variant(),
        );
        dict.set(&GString::from("discovered"), &false.to_variant());
        dict
    }

    /// Lists the decoded vanilla schematics.
    #[func]
    pub fn list_schematics(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        let Some(registry) = &self.registry else {
            return out;
        };
        let mut schematics = Schematics::new();
        schematics.load_loadouts(registry);
        for schematic in &schematics.all {
            let mut row = VarDictionary::new();
            row.set(
                &GString::from("name"),
                &schematic.name().as_str().to_variant(),
            );
            row.set(&GString::from("width"), &schematic.width.to_variant());
            row.set(&GString::from("height"), &schematic.height.to_variant());
            row.set(
                &GString::from("tiles"),
                &(schematic.tiles.len() as i64).to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Captures a schematic from the live world selection (I-5): reads the
    /// selected tile rectangle and returns its base64 `.msch` payload.
    #[func]
    pub fn write_schematic_selection(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> GString {
        let Some(host) = self.host() else {
            return GString::from("");
        };
        let Some(registry) = self.registry.as_ref() else {
            return GString::from("");
        };
        let tiles = host.bind().tile_region(x0, y0, x1, y1);
        let Some(min_x) = tiles.iter().map(|(x, _, _)| *x as i32).min() else {
            return GString::from("");
        };
        let Some(min_y) = tiles.iter().map(|(_, y, _)| *y as i32).min() else {
            return GString::from("");
        };
        let Some(max_x) = tiles.iter().map(|(x, _, _)| *x as i32).max() else {
            return GString::from("");
        };
        let Some(max_y) = tiles.iter().map(|(_, y, _)| *y as i32).max() else {
            return GString::from("");
        };
        let width = max_x - min_x + 1;
        let height = max_y - min_y + 1;
        let mut schematic = mind_core::game::schematic::Schematic {
            tiles: Vec::new(),
            labels: Vec::new(),
            tags: Default::default(),
            width,
            height,
            file: None,
            mod_name: None,
        };
        for (x, y, block) in tiles {
            if block == 0 {
                continue;
            }
            schematic.tiles.push(mind_core::game::schematic::Stile::new(
                mind_core::content::BlockId::new(block),
                (x as i32 - min_x) as i16,
                (y as i32 - min_y) as i16,
                Default::default(),
                0,
            ));
        }
        match mind_core::game::schematics::write_base64(&schematic, registry) {
            Ok(encoded) => GString::from(encoded.as_str()),
            Err(error) => {
                log::warn!("write_schematic_selection failed: {error}");
                GString::from("")
            }
        }
    }

    /// Places a base64 `.msch` schematic centered at `(x, y)` into the live
    /// world (I-5); rotation `0` only until the rotate relay command lands.
    #[func]
    pub fn place_schematic_base64(&mut self, b64: GString, x: i32, y: i32) -> bool {
        let Some(registry) = self.registry.as_ref() else {
            return false;
        };
        let Ok(schematic) = mind_core::game::schematics::read_base64(&b64.to_string(), registry)
        else {
            return false;
        };
        let Some(mut host) = self.host() else {
            return false;
        };
        let mut placed = false;
        let mut host = host.bind_mut();
        for stile in &schematic.tiles {
            if stile.block == mind_core::content::BlockId::AIR {
                continue;
            }
            let Ok(tile_x) = i16::try_from(x + stile.x as i32) else {
                continue;
            };
            let Ok(tile_y) = i16::try_from(y + stile.y as i32) else {
                continue;
            };
            placed |= host.place_block_at(tile_x, tile_y, stile.block, stile.rotation);
        }
        placed
    }

    /// Writes the live sim to a named `user://saves/<name>.msav` slot
    /// (queued at the next IO tick boundary). Returns whether the request was
    /// queued.
    #[func]
    pub fn save_slot(&mut self, name: GString) -> bool {
        let Some(mut host) = self.host() else {
            return false;
        };
        let slot = sanitize_slot_name(&name.to_string());
        if slot.is_empty() {
            return false;
        }
        host.bind_mut().request_save(
            GString::from(format!("user://saves/{slot}.msav").as_str()),
            false,
        );
        true
    }

    /// Loads a named `user://saves/<name>.msav` slot at the next IO tick
    /// boundary. Returns whether the request was queued.
    #[func]
    pub fn load_slot(&mut self, name: GString) -> bool {
        let Some(mut host) = self.host() else {
            return false;
        };
        let slot = sanitize_slot_name(&name.to_string());
        if slot.is_empty() {
            return false;
        }
        let path = format!("user://saves/{slot}.msav");
        if !FileAccess::file_exists(&path) {
            log::warn!("load_slot: `{path}` does not exist");
            return false;
        }
        host.bind_mut().request_load(GString::from(path.as_str()));
        true
    }

    /// `PlanetDialog.abandonSectorConfirm` (persisted half): clears the active
    /// sector's items/base/production, drops its save and removes the file.
    #[func]
    pub fn abandon_sector(&mut self) -> bool {
        let Some((planet, sector_id)) = self.session.sector else {
            return false;
        };
        let Some(planet_name) = self
            .registry
            .as_ref()
            .and_then(|registry| registry.planet(planet))
            .map(|record| record.name.clone())
        else {
            return false;
        };

        let mut cleared = false;
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                clear_sector_record(
                    &mut runtime.campaign,
                    &mut self.settings,
                    planet,
                    sector_id,
                    &planet_name,
                );
                host.bind_mut().put_campaign_runtime(runtime);
                cleared = true;
            }
        }
        if let Some(campaign) = self.campaign.as_mut() {
            clear_sector_record(
                campaign,
                &mut self.settings,
                planet,
                sector_id,
                &planet_name,
            );
            cleared = true;
        }
        if !cleared {
            return false;
        }

        let file = self.paths.sector_save(&planet_name, sector_id as u32);
        if let Err(error) = NativeFs.delete(&file) {
            log::warn!(
                "abandon_sector: failed to delete `{}`: {error}",
                file.display()
            );
        }
        let backup = mind_core::io::SaveIo::backup_file_for(&file);
        let _ = NativeFs.delete(&backup);
        self.persist();
        log::info!("abandoned campaign sector `{planet_name}:{sector_id}`");
        true
    }

    /// `SettingsMenuDialog.planetDataDialog` selectable planets: a generator,
    /// at least one sector and accessibility (content order).
    #[func]
    pub fn selectable_planets(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        let Some(registry) = self.registry.as_ref() else {
            return out;
        };
        for def in registry.planets() {
            if def.generator == GeneratorKind::None || def.sectors.is_empty() || !def.accessible {
                continue;
            }
            let mut row = VarDictionary::new();
            row.set(&GString::from("name"), &def.name.as_str().to_variant());
            let color = def.icon_color;
            row.set(
                &GString::from("iconColor"),
                Color::from_rgba(color.r, color.g, color.b, color.a),
            );
            out.push(&row);
        }
        out
    }

    /// `control.saves.deleteAll()` (`@settings.clearsaves`): removes every
    /// non-sector save slot.
    #[func]
    pub fn clear_saves(&mut self) -> bool {
        let Some(registry) = self.registry.as_ref() else {
            return false;
        };
        self.saves
            .load(&NativeFs, &self.paths, registry, &mut self.settings);
        match self.saves.delete_all(&NativeFs) {
            Ok(()) => {
                log::info!(
                    "cleared non-sector saves ({} slot(s) left)",
                    self.saves.slots.len()
                );
                true
            }
            Err(error) => {
                log::warn!("clear saves failed: {error}");
                false
            }
        }
    }

    /// `@settings.clearresearch`: `Universe.clearLoadoutInfo`, every tech node
    /// reset and every content unlock cleared.
    #[func]
    pub fn clear_research(&mut self) -> bool {
        let Some(mut registry) = self.registry.take() else {
            return false;
        };
        if let Some(campaign) = self.campaign.as_mut() {
            campaign.universe.clear_loadout_info(&mut self.settings);
        }
        let cleared = {
            let mut store = SettingsUnlockStore::new(&mut self.settings);
            registry.clear_unlocks(&mut store, None)
        };
        tech_tree::reset_all(&mut registry);
        self.settings.remove("unlocks");
        self.registry = Some(registry);
        self.push_campaign_to_runtime();
        self.persist();
        log::info!("cleared research ({cleared} unlock(s))");
        true
    }

    /// `@settings.clearplanetresearch`: the planet's tech tree, its database
    /// content unlocks and the launch loadout info.
    #[func]
    pub fn clear_planet_research(&mut self, planet: GString) -> bool {
        let name = planet.to_string();
        let Some(mut registry) = self.registry.take() else {
            return false;
        };
        let Some((planet_id, tree)) = registry
            .planet_by_name(&name)
            .map(|def| (def.id, def.tech_tree))
        else {
            self.registry = Some(registry);
            log::warn!("clear_planet_research: unknown planet `{name}`");
            return false;
        };
        if let Some(campaign) = self.campaign.as_mut() {
            campaign.universe.clear_loadout_info(&mut self.settings);
        }
        let cleared = {
            let mut store = SettingsUnlockStore::new(&mut self.settings);
            registry.clear_unlocks(&mut store, Some(planet_id))
        };
        if let Some(tree) = tree {
            tech_tree::reset_tree(&mut registry, tree);
        }
        self.settings.remove("unlocks");
        self.registry = Some(registry);
        self.push_campaign_to_runtime();
        self.persist();
        log::info!("cleared `{name}` research ({cleared} unlock(s))");
        true
    }

    /// `@settings.clearcampaignsaves`: every planet's stats and sector info are
    /// cleared and every sector save slot deleted.
    #[func]
    pub fn clear_campaign_saves(&mut self) -> bool {
        self.clear_campaign_saves_for(None)
    }

    /// `@settings.clearplanetcampaignsaves`: planet-scoped campaign-save clear.
    #[func]
    pub fn clear_planet_campaign_saves(&mut self, planet: GString) -> bool {
        let name = planet.to_string();
        let Some(id) = self
            .registry
            .as_ref()
            .and_then(|registry| registry.planet_by_name(&name))
            .map(|def| def.id)
        else {
            log::warn!("clear_planet_campaign_saves: unknown planet `{name}`");
            return false;
        };
        self.clear_campaign_saves_for(Some(id))
    }

    /// Shared `SettingsMenuDialog` clear-campaign-saves body (`only` scopes to
    /// one planet; `None` covers every planet).
    fn clear_campaign_saves_for(&mut self, only: Option<PlanetId>) -> bool {
        let Some(registry) = self.registry.take() else {
            return false;
        };
        let Some(mut campaign) = self.campaign.take() else {
            self.registry = Some(registry);
            return false;
        };
        let planets: Vec<(PlanetId, String)> = campaign
            .planets
            .values()
            .filter(|planet| only.is_none_or(|target| target == planet.id))
            .map(|planet| (planet.id, planet.name.clone()))
            .collect();
        if planets.is_empty() {
            self.registry = Some(registry);
            self.campaign = Some(campaign);
            return false;
        }

        for (id, name) in &planets {
            campaign.clear_stats(*id);
            let Some(planet) = campaign.planet_mut(*id) else {
                continue;
            };
            for sector in &mut planet.sectors {
                sector.save = None;
                sector.being_played = false;
                sector.clear_info(&mut self.settings, name);
                let file = self.paths.sector_save(name, sector.id as u32);
                let backup = mind_core::io::SaveIo::backup_file_for(&file);
                let _ = NativeFs.delete(&file);
                let _ = NativeFs.delete(&backup);
            }
        }
        self.campaign = Some(campaign);

        // Re-list saves (a slot may have been written this session) and drop
        // the matching sector slots.
        self.saves
            .load(&NativeFs, &self.paths, &registry, &mut self.settings);
        if let Err(error) = self.saves.delete_sectors(&NativeFs, &registry, only) {
            log::warn!("clear campaign saves: {error}");
        }
        self.registry = Some(registry);
        self.push_campaign_to_runtime();
        self.persist();
        log::info!("cleared campaign saves for {} planet(s)", planets.len());
        true
    }

    /// Lists the known save slots (`name`, `file`, `sector`, `autosave`).
    #[func]
    pub fn list_save_slots(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        // `request_save` writes through Godot's `user://`, so scan that same
        // directory instead of the bootstrap-time `Saves` snapshot: a slot saved
        // during this session must show up without a restart.
        let root = ProjectSettings::singleton()
            .globalize_path("user://")
            .to_string();
        let paths = Paths::new(root);
        for slot in list_slot_files(&NativeFs, &paths) {
            let mut row = VarDictionary::new();
            row.set(
                &GString::from("name"),
                &slot.name(&self.settings).as_str().to_variant(),
            );
            row.set(
                &GString::from("file"),
                &slot.file.to_string_lossy().as_ref().to_variant(),
            );
            row.set(&GString::from("sector"), &slot.is_sector().to_variant());
            row.set(
                &GString::from("autosave"),
                &slot.is_autosave(&self.settings).to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// The active planet's `CampaignRules` as JSON (GAP-11).
    #[func]
    pub fn get_campaign_rules_json(&self) -> GString {
        let planet = self.session.sector.map(|(planet, _)| planet);
        let rules = planet
            .and_then(|planet| {
                self.campaign
                    .as_ref()
                    .and_then(|campaign| campaign.planet(planet))
            })
            .map(|planet| planet.campaign_rules.clone())
            .unwrap_or_default();
        GString::from(
            serde_json::to_string(&rules)
                .unwrap_or_else(|_| "{}".to_owned())
                .as_str(),
        )
    }

    /// Applies a `CampaignRules` JSON blob to the active planet, folds it into
    /// the session rules and persists it (GAP-11 / C-9).
    #[func]
    pub fn set_campaign_rules_json(&mut self, json: GString) -> bool {
        let Ok(rules) = serde_json::from_str::<CampaignRules>(&json.to_string()) else {
            return false;
        };
        let Some((planet_id, _)) = self.session.sector else {
            return false;
        };
        let Some(registry) = self.registry.as_ref() else {
            return false;
        };
        let Some(campaign) = self.campaign.as_mut() else {
            return false;
        };
        let Some(planet_record) = campaign.planet_mut(planet_id) else {
            return false;
        };
        planet_record.campaign_rules = rules;
        let applied = {
            let planet_record = &*planet_record;
            planet_record.apply_rules(registry, &mut self.session.rules, false, true)
        };
        if applied.rts_swapped {
            log::info!("campaign rules RTS-AI swap (controller reset is plan-11)");
        }
        let campaign_snapshot = campaign.clone();
        // Mirror into the live runtime session.
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                runtime.session.rules = self.session.rules.clone();
                runtime.campaign = campaign_snapshot;
                host.bind_mut().put_campaign_runtime(runtime);
            }
        }
        self.persist();
        true
    }

    /// `Universe.getLaunchResources` (item-id-indexed last-launch sequence).
    #[func]
    pub fn get_launch_resources(&mut self) -> PackedInt32Array {
        let mut out = PackedInt32Array::new();
        for value in self
            .campaign
            .as_mut()
            .map(|campaign| campaign.universe.get_launch_resources(&self.settings))
            .unwrap_or_default()
        {
            out.push(*value);
        }
        out
    }

    /// `Universe.updateLaunchResources` from a JSON `[n, ...]` sequence.
    #[func]
    pub fn update_launch_resources_json(&mut self, json: GString) -> bool {
        let Ok(stacks) = serde_json::from_str::<Vec<i32>>(&json.to_string()) else {
            return false;
        };
        let Some(campaign) = self.campaign.as_mut() else {
            return false;
        };
        if campaign
            .universe
            .update_launch_resources(&mut self.settings, stacks)
            .is_err()
        {
            return false;
        }
        self.persist();
        true
    }

    /// `Universe.getLastLoadout` (selected schematic file name).
    #[func]
    pub fn get_last_loadout(&self) -> GString {
        GString::from(
            self.campaign
                .as_ref()
                .and_then(|campaign| campaign.universe.get_last_loadout())
                .unwrap_or(""),
        )
    }

    /// `Universe.updateLoadout` for a core type (empty file clears it).
    #[func]
    pub fn update_loadout(&mut self, core: GString, file: GString) -> bool {
        let Some(campaign) = self.campaign.as_mut() else {
            return false;
        };
        let file_name = file.to_string();
        let file_name = (!file_name.is_empty()).then_some(file_name.as_str());
        campaign
            .universe
            .update_loadout(&mut self.settings, &core.to_string(), file_name);
        self.persist();
        true
    }

    /// Planet globe data for the sector view (plan 16 g3d seam): the
    /// `PlanetGrid` sector topology, the render-plane grid-line mesh
    /// (`MeshBuilder.buildPlanetGrid` at `outlineRad * radius`) and the
    /// `assets/planets/<name>.json` preset → sector-index remap.
    ///
    /// Returns an empty dictionary for an unknown planet or before content
    /// boot.
    #[func]
    pub fn planet_view(&mut self, planet: GString) -> VarDictionary {
        let mut out = VarDictionary::new();
        let name = planet.to_string();
        let Some(registry) = self.registry.as_ref() else {
            return out;
        };
        let Some(def) = registry.planet_by_name(&name) else {
            return out;
        };

        let grid = PlanetGrid::create(def.sector_tiles as usize);
        let outline = OUTLINE_RAD * def.radius;

        let mut tiles = PackedVector3Array::new();
        for tile in &grid.tiles {
            tiles.push(Vector3::new(tile.v[0], tile.v[1], tile.v[2]));
        }
        let mut corners = PackedVector3Array::new();
        for corner in &grid.corners {
            corners.push(Vector3::new(corner.v[0], corner.v[1], corner.v[2]));
        }
        let mut tile_corners = Array::<PackedInt32Array>::new();
        for tile in &grid.tiles {
            let mut ids = PackedInt32Array::new();
            for corner in &tile.corners {
                ids.push(*corner as i32);
            }
            tile_corners.push(&ids);
        }

        let line_mesh = build_planet_grid(&grid, u32::MAX, outline);
        let mut lines = PackedVector3Array::new();
        for vertex in &line_mesh.vertices {
            lines.push(Vector3::new(vertex[0], vertex[1], vertex[2]));
        }

        let mut presets = VarDictionary::new();
        let path = format!(
            "{}/planets/{name}.json",
            crate::assets::loader::resolve_assets_dir()
        );
        if FileAccess::file_exists(&path) {
            let text = FileAccess::get_file_as_string(&path).to_string();
            match serde_json::from_str::<serde_json::Value>(&mind_core::io::json::quote_bare_keys(
                &text,
            )) {
                Ok(value) => {
                    if let Some(entries) = value.get("presets").and_then(|v| v.as_object()) {
                        for (preset, index) in entries {
                            if let Some(index) = index.as_u64() {
                                presets.set(preset.as_str(), index as i64);
                            }
                        }
                    }
                }
                Err(error) => log::warn!("[campaign] planet data {path}: {error}"),
            }
        }

        let icon = def.icon_color;
        out.set("name", name);
        out.set("grid_size", def.sector_tiles as i64);
        out.set("radius", def.radius);
        out.set("outline", outline);
        out.set("cam_radius", def.cam_radius);
        out.set("start_sector", def.start_sector as i64);
        out.set(
            "icon_color",
            Color::from_rgba(icon.r, icon.g, icon.b, icon.a),
        );
        out.set("tiles", &tiles);
        out.set("corners", &corners);
        out.set("tile_corners", &tile_corners);
        out.set("lines", &lines);
        out.set("presets", &presets);
        out
    }

    /// Content boot error, if any (diagnostics).
    #[func]
    pub fn content_error(&self) -> GString {
        GString::from(self.content_error.as_deref().unwrap_or(""))
    }

    /// Rebuilds settings, content and campaign state from disk
    /// (`importData`'s `settings.load()` + `state = new GameState()` pair).
    #[func]
    pub fn reload_from_disk(&mut self) -> bool {
        self.bootstrap();
        self.content_error.is_none()
    }

    /// `@settings.cleardata` host half: drops the in-memory campaign, registry,
    /// save and settings state so the exit persist cannot resurrect the data
    /// the UI is about to delete.
    #[func]
    pub fn clear_in_memory(&mut self) {
        self.settings.clear();
        self.campaign = None;
        self.registry = None;
        self.saves = Saves::new();
        self.session = PlaySession::default();
        self.runtime_installed = false;
        self.content_error = None;
    }

    /// Live HUD/status fields (`HudFragment` read surface, gap3 C-10). Appended
    /// for `MindHud`; additive and read-only. Includes the campaign session's
    /// wave/enemy/timer/goal state the other getters do not expose.
    #[func]
    pub fn get_hud_state(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        dict.set(&key("campaign"), &self.session.is_campaign().to_variant());
        dict.set(&key("waves"), &self.session.rules.waves.to_variant());
        dict.set(
            &key("waveSending"),
            &self.session.rules.wave_sending.to_variant(),
        );
        dict.set(&key("attack"), &self.session.rules.attack_mode.to_variant());
        dict.set(&key("wave"), &self.session.wave.to_variant());
        dict.set(&key("enemies"), &self.session.enemies.to_variant());
        dict.set(&key("wavetime"), &self.session.wavetime.to_variant());
        dict.set(&key("winWave"), &self.session.rules.win_wave.to_variant());
        dict.set(
            &key("waveTimer"),
            &self.session.rules.wave_timer.to_variant(),
        );
        dict.set(
            &key("waitEnemies"),
            &self.session.rules.wait_enemies.to_variant(),
        );
        dict.set(&key("gameOver"), &self.session.game_over.to_variant());
        dict.set(
            &key("afterGameOver"),
            &self.session.after_game_over.to_variant(),
        );
        dict.set(&key("won"), &self.session.won.to_variant());
        dict.set(&key("isSpawning"), &self.session.is_spawning.to_variant());
        dict.set(
            &key("hasCore"),
            &(self.session.player_core_count() > 0).to_variant(),
        );
        dict.set(
            &key("enemyCores"),
            &(self.session.wave_core_count() as i64).to_variant(),
        );
        if let Some((planet, sector)) = self.session.sector {
            let planet_name = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.planet(planet))
                .map(|planet| planet.name.clone())
                .unwrap_or_default();
            let preset_name = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(planet, sector))
                .and_then(|sector| sector.preset_name.clone());
            let was_captured = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(planet, sector))
                .map(|sector| sector.info.info.was_captured)
                .unwrap_or(false);
            dict.set(&key("planet"), &planet_name.as_str().to_variant());
            dict.set(&key("sector"), &(sector as i64).to_variant());
            dict.set(
                &key("sectorName"),
                &preset_name.unwrap_or(planet_name).as_str().to_variant(),
            );
            dict.set(&key("wasCaptured"), &was_captured.to_variant());
        }
        dict
    }
}

/// Merges the loaded map/generator's wave rules into the session rules without/// clobbering the preset/campaign fold (`World.loadSector` ordering). For a
/// preset-launched sector the preset wins; a planet-generated sector adopts the
/// generator's `generate_rules` wave/win settings.
fn merge_map_rules(session: &mut Rules, map: &Rules, has_preset: bool) {
    if !map.spawns.is_empty() {
        session.spawns = map.spawns.clone();
    }
    if has_preset {
        return;
    }
    session.waves = map.waves;
    session.win_wave = map.win_wave;
    session.attack_mode = map.attack_mode;
    session.wave_spacing = map.wave_spacing;
}

/// Unlocked content names of the registry (map objective `Research`/`Produce`).
fn collect_unlocked(registry: &ContentRegistry) -> Vec<String> {
    registry
        .tech()
        .nodes
        .iter()
        .filter_map(|node| node.content)
        .filter(|content| tech_tree::content_unlocked(registry, *content))
        .filter_map(|content| tech_tree::content_name(registry, content))
        .collect()
}

/// Sanitizes a save-slot name into a file stem (alphanumerics, `-`, `_`).
fn sanitize_slot_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect()
}

/// Clears one campaign sector's persisted base/items
/// (`PlanetDialog.abandonSectorConfirm`, non-playing branch).
fn clear_sector_record(
    campaign: &mut Campaign,
    settings: &mut SettingsStore,
    planet: mind_core::content::PlanetId,
    sector_id: u16,
    planet_name: &str,
) {
    if let Some(sector) = campaign.sector_mut(planet, sector_id) {
        sector.save = None;
        sector.being_played = false;
        sector.clear_info(settings, planet_name);
    }
}

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
use godot::classes::{FileAccess, INode, Node};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::content::{
    ContentRef, ContentRegistry, ContentType, MemoryBundle, create_base_content,
};
use mind_core::game::campaign_rules::CampaignRules;
use mind_core::game::objectives::CampaignObjectiveContext;
use mind_core::game::planet::EmptyNeighborhood;
use mind_core::game::play::{
    PlaySession, play_map, play_new_sector, play_sector, run_wave_campaign, sector_capture,
};
use mind_core::game::rules::Rules;
use mind_core::game::rules_event::{RulesEpoch, apply_set_rules};
use mind_core::game::runtime::CampaignRuntime;
use mind_core::game::saves::Saves;
use mind_core::game::schematics::Schematics;
use mind_core::game::tech_tree::{self, SettingsUnlockStore};
use mind_core::game::universe::{Campaign, TurnContext};
use mind_core::game::world_reloader::HostReloader;
use mind_core::io::fs::{NativeFs, Paths};
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
            Ok(registry) => {
                let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
                campaign.load_all(&self.settings);
                self.saves = Saves::new();
                self.saves
                    .load(&NativeFs, &self.paths, &registry, &mut self.settings);
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
        self.session = runtime.session.clone();
        self.campaign = Some(runtime.campaign.clone());
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
            merge_map_rules(&mut self.session.rules, &map_rules);
        }

        // Install the runtime and register the live cores/spawns (GAP-9).
        let mut installed = false;
        if let Some(mut host) = self.host() {
            let runtime = CampaignRuntime::new(self.session.clone(), campaign.clone());
            host.bind_mut().install_campaign_runtime(runtime);
            let sync = host.bind_mut().sync_campaign_session(&registry);
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
    /// runtime when one is installed.
    #[func]
    pub fn run_wave(&mut self) -> bool {
        let _ = run_wave_campaign(&mut self.session);
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                let _ = run_wave_campaign(&mut runtime.session);
                host.bind_mut().put_campaign_runtime(runtime);
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

        // Real inventory: the active sector's stored items (`Sector.items`).
        let mut items = ItemModule::with_items(registry.items().len());
        let sector_items: Vec<(String, i32)> = self
            .session
            .sector
            .and_then(|(planet, sector)| {
                self.campaign
                    .as_ref()
                    .and_then(|campaign| campaign.sector(planet, sector))
            })
            .map(|sector| {
                sector
                    .info
                    .info
                    .items
                    .iter()
                    .map(|(name, amount)| (name.clone(), *amount))
                    .collect()
            })
            .unwrap_or_default();
        for (name, amount) in &sector_items {
            if let Some(id) = registry.item_id(name) {
                items.add(id, *amount, i32::MAX);
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

        // Write the remaining inventory back into the sector.
        if let Some((planet, sector)) = self.session.sector
            && let Some(record) = self
                .campaign
                .as_mut()
                .and_then(|campaign| campaign.sector_mut(planet, sector))
        {
            for (name, _) in &sector_items {
                let left = registry.item_id(name).map(|id| items.get(id)).unwrap_or(0);
                record.info.info.items.insert(name.clone(), left);
            }
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
        }
        changed
    }

    /// Copies the facade campaign into the installed runtime (post-research /
    /// rule edits).
    fn push_campaign_to_runtime(&mut self) {
        let Some(campaign) = self.campaign.clone() else {
            return;
        };
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                runtime.campaign = campaign;
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

    /// Completes an objective by index (`complete_objective` relay target).
    #[func]
    pub fn complete_objective(&mut self, index: i64) -> bool {
        if index < 0 {
            return false;
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

    /// Lists the known save slots (`name`, `file`, `sector`, `autosave`).
    #[func]
    pub fn list_save_slots(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        for slot in &self.saves.slots {
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
}

/// Merges the loaded map/generator's wave rules into the session rules without
/// clobbering the preset/campaign fold (`World.loadSector` ordering).
fn merge_map_rules(session: &mut Rules, map: &Rules) {
    if map.win_wave > 0 && session.win_wave == 0 {
        session.win_wave = map.win_wave;
    }
    if !map.spawns.is_empty() {
        session.spawns = map.spawns.clone();
    }
}

/// Sanitizes a save-slot name into a file stem (alphanumerics, `-`, `_`).
fn sanitize_slot_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect()
}

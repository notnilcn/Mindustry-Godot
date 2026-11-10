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

use godot::builtin::{
    PackedInt32Array, PackedStringArray, PackedVector3Array, VarDictionary, VariantType, Vector3,
};
use godot::classes::notify::NodeNotification;
use godot::classes::{FileAccess, INode, Node, ProjectSettings, Texture2D, Time};
use godot::obj::Base;
use godot::prelude::*;

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use mind_core::assets::bundle::Bundle;
use mind_core::content::registries::planets::{GeneratorKind, PlanetDef};
use mind_core::content::settings_store::UnlockStore;
use mind_core::content::tech::{ObjectiveSpec, TechNodeRef};
use mind_core::content::{
    BlockId, ContentRef, ContentRegistry, ContentType, ItemId, LiquidId, MemoryBundle, PlanetId,
    SectorId,
};
use mind_core::event;
use mind_core::game::State;
use mind_core::game::campaign_rules::CampaignRules;
use mind_core::game::gamemode::Gamemode;
use mind_core::game::map_objectives::MapObjectivesRuntime;
use mind_core::game::objectives::{CampaignObjectiveContext, Objective, ObjectiveContext};
use mind_core::game::planet::EmptyNeighborhood;
use mind_core::game::play::{
    PlaySession, SectorRef, apply_save_load_campaign_rules, apply_world_load_campaign_rules,
    play_map, play_new_sector, play_sector, run_wave_campaign, sector_capture,
};
use mind_core::game::rules::Rules;
use mind_core::game::rules_event::{RulesEpoch, apply_set_rules};
use mind_core::game::runtime::{CampaignRuntime, UnlockRef, on_captured_sector};
use mind_core::game::saves::{
    DEFAULT_SAVE_INTERVAL_SECONDS, Saves, SavesUpdate, SavesUpdateResult,
};
use mind_core::game::schematics::Schematics;
use mind_core::game::service::{AchievementRow, ServiceTracker};
use mind_core::game::tech_tree::{self, SettingsUnlockStore};
use mind_core::game::universe::{Campaign, TurnContext, launch_plan};
use mind_core::game::waves::WaveMapHooks;
use mind_core::game::world_reloader::HostReloader;
use mind_core::io::FileSystem;
use mind_core::io::fs::{NativeFs, Paths, SAVE_EXTENSION};
use mind_core::io::json::quote_bare_keys;
use mind_core::io::json::rules::GameStats;
use mind_core::io::save::slot::{SaveSlot, list_save_slots as list_slot_files};
use mind_core::io::settings::{KEY_SAVE_INTERVAL, SettingsStore};
use mind_core::io::{SaveIo, SaveMeta};
use mind_core::random::JavaRandom;
use mind_core::render::g3d::grid::PlanetGrid;
use mind_core::render::g3d::mesh_data::build_planet_grid;
use mind_core::ui::content_stats::{Cell, StatCtx, content_stats as compute_content_stats};
use mind_core::world::modules::ItemModule;

use crate::audio::MindAudio;
use crate::camera::MindCamera2D;
use crate::editor::MindEditor;
use crate::fx::MindFx;
use crate::settings_hub;
use crate::sim_host::MindSimHost;
use crate::ui::root::MindUiRoot;
use crate::wiring;

#[allow(unused_imports)] // old `crate::campaign::FiredEvent` path
pub use events::FiredEvent;
pub use events::FiredEvents;
use accelerator::PendingAcceleratorLaunch;
use cache::DirMemo;
use events::dispatch_event_rows;
use events::event_content_of;
use events::install_event_listeners;
use saves::sanitize_slot_name;
use service::load_service;
use service::save_service;
use text::content_icon_name;

mod accelerator;
mod cache;
mod database;
mod events;
mod hud;
mod launch;
mod lifecycle;
mod objectives;
mod planet_view;
mod rules;
mod runtime;
mod saves;
mod schematics;
mod sectors;
mod service;
mod text;

#[cfg(test)]
mod test_util;

/// Off-play-sector inventory rows of `ResearchDialog.rebuildItems`
/// (`Vec<(sector, rows)>`).
type SectorInventory = Vec<(SectorRef, Vec<(String, i32)>)>;

/// `MindCampaign` — campaign/sector/rules/tech/schematic facade for MCP + UI.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindCampaign {
    base: Base<Node>,
    /// Sibling `SimHost` (`../SimHost`) — required; the live sim/runtime host.
    #[export]
    pub sim_host: Option<Gd<MindSimHost>>,
    /// Sibling `World/Camera2D` (`../World/Camera2D`) — optional; landing/
    /// accelerator cutscenes and shake.
    #[export]
    pub camera: Option<Gd<MindCamera2D>>,
    /// Sibling `MindAudio` (`../MindAudio`) — optional; landing cue and
    /// accelerator sounds.
    #[export]
    pub audio: Option<Gd<MindAudio>>,
    /// Sibling `MindFx` (`../MindFx`) — optional; accelerator effect spawns.
    #[export]
    pub fx: Option<Gd<MindFx>>,
    /// Sibling `MindEditor` (`../MindEditor`) — optional; the playtest reopen
    /// (`resumeAfterPlaytest`).
    #[export]
    pub editor: Option<Gd<MindEditor>>,
    /// `Ui/UiRoot` (`../Ui/UiRoot`) — optional; the menu restore on an invalid
    /// map.
    #[export]
    pub ui_root: Option<Gd<MindUiRoot>>,
    session: PlaySession,
    campaign: Option<Campaign>,
    registry: Option<ContentRegistry>,
    paths: Paths,
    saves: Saves,
    epoch: RulesEpoch,
    reloader: HostReloader,
    content_error: Option<String>,
    /// Live `Core.bundle` (`assets/bundles/*`); the content registry boots with
    /// an empty `MemoryBundle`, so unlock descriptions resolve from here.
    bundle: Bundle,
    /// A `CampaignRuntime` is installed in the live sim host.
    runtime_installed: bool,
    /// The last captured sector was the planet's last (campaign victory).
    campaign_complete: bool,
    /// An Interplanetary Accelerator launch waiting for its
    /// `Time.runTask(launchDuration() - 6f)` sector switch (L06 T7).
    pending_accelerator: Option<PendingAcceleratorLaunch>,
    /// Last tick seen by the runtime pump (fixed-tick delta for `Saves::update`).
    last_pump_tick: u64,
    /// Tick at which an in-flight autosave clears `Saves.saving` (3-tick task).
    saving_finish_tick: Option<u64>,
    /// Content names unlocked since the last `take_unlocked_content` (H05's
    /// `hudfrag.showUnlock` popup; `Control.java:160-174`).
    unlocked_since_last_take: Vec<String>,
    /// `state.playtestingMap` (`Control.java:409`): the map a playtest start
    /// loaded, read by the editor's `resumeAfterPlaytest`.
    playtesting_map: Option<String>,
    /// `Map.name` of the last `start_map_mode` custom game (I08 T4: the host
    /// match id / server-browser label for non-campaign worlds).
    current_map_name: Option<String>,
    /// Cached `planet_mesh` build `(planet, mesh)`. The hex mesh costs a full
    /// per-tile noise pass (Java caches it on `Planet.mesh`; `reloadMeshAsync`
    /// rebuilds on capture/loss), so repeat dialog opens reuse it until a
    /// capture/abandon invalidates the entry (T13/T14).
    planet_mesh_cache: Option<(String, VarDictionary)>,
    /// THE shared achievement/stat tracker (`Vars.service` + `GameService`;
    /// GAPS #68). Loaded from `Core.settings` at boot, injected into every
    /// live [`CampaignRuntime`] (`with_service`) so the campaign systems keep
    /// one counter graph across sector switches, mirrored back from the
    /// runtime each pump and persisted on autosave/exit.
    service: ServiceTracker,
    /// `EventType` bus (`core/src/mindustry/game/EventType.java` + Arc
    /// `Events.fire/on`; PAR-0577 residual). `pump_runtime` drains the play
    /// session's `EventRow` outbox into it, and the host-owned fire sites
    /// (research completions, relay roster changes) dispatch directly.
    events: event::Events,
    /// Rows the bus dispatched, bounded by `EVENT_QUEUE_CAPACITY`
    /// (`take_fired_events` drains it; tests read it directly).
    fired_events: Rc<RefCell<FiredEvents>>,
    /// Memoized schematic library (`schematics/` directory mtime; finding #9).
    /// Dialogs rebuild `campaign_views_json` repeatedly and every uncached call
    /// re-reads every `.msch`. Content-derived (block ids/names), so
    /// [`Self::invalidate_caches`] clears it on a content/data-root reboot.
    schematics_cache: RefCell<DirMemo<Rc<Schematics>>>,
    /// Memoized save-slot listing (`saves/` directory mtime; finding #9).
    /// `SaveIo::save` renames `<file> -> <file>-backup` and `<file>.tmp ->
    /// <file>` inside that directory, so every save/overwrite/delete bumps the
    /// key even when the slot is logically rewritten in place.
    save_slots_cache: RefCell<DirMemo<Rc<Vec<SaveSlot>>>>,
}

#[godot_api]
impl INode for MindCampaign {
    fn init(base: Base<Node>) -> Self {
        // The bus and its recorder listeners are host state, not Godot-derived:
        // `ready()` must not re-register them or every row would record twice.
        let mut events = event::Events::new();
        let fired_events = Rc::new(RefCell::new(FiredEvents::new()));
        install_event_listeners(&mut events, &fired_events);
        Self {
            base,
            sim_host: None,
            camera: None,
            audio: None,
            fx: None,
            editor: None,
            ui_root: None,
            session: PlaySession::default(),
            campaign: None,
            registry: None,
            paths: Paths::new(std::env::temp_dir()),
            saves: Saves::new(),
            epoch: RulesEpoch::new(),
            reloader: HostReloader::default(),
            content_error: None,
            bundle: Bundle::new(),
            runtime_installed: false,
            campaign_complete: false,
            pending_accelerator: None,
            last_pump_tick: 0,
            saving_finish_tick: None,
            unlocked_since_last_take: Vec::new(),
            playtesting_map: None,
            current_map_name: None,
            planet_mesh_cache: None,
            service: ServiceTracker::new(),
            events,
            fired_events,
            schematics_cache: RefCell::new(DirMemo::default()),
            save_slots_cache: RefCell::new(DirMemo::default()),
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

    fn process(&mut self, delta: f64) {
        self.pump_runtime();
        self.pump_accelerator_launch(delta);
    }

    fn exit_tree(&mut self) {
        // `Control.dispose`: save the current autosave on application dispose.
        self.run_exit_save();
        self.persist();
    }
}

#[godot_api]
impl MindCampaign {
    /// Rebuilds the campaign content (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`). The registry and
    /// campaign are overwritten, so a re-run never duplicates them.
    fn bootstrap(&mut self) {
        self.bootstrap_with(true);
    }

    /// [`Self::bootstrap`]; `run_saves_load = false` skips `Saves.load` (and
    /// its legacy sector remap) for the post-import rebuild, which only exists
    /// to stop the exit persist from writing pre-import state: Java's
    /// `importData` exits without loading saves, so the remap - and the
    /// one-time remap notice it arms - happen on the next boot.
    fn bootstrap_with(&mut self, run_saves_load: bool) {
        // Scene-wired spine deps: `game.tscn` sets them on the instance root;
        // `ready()` and `EXTENSION_RELOADED` both re-adopt here.
        self.sim_host = wiring::required("MindCampaign", "sim_host", &self.sim_host);
        self.camera = wiring::optional(&self.camera);
        self.audio = wiring::optional(&self.audio);
        self.fx = wiring::optional(&self.fx);
        self.editor = wiring::optional(&self.editor);
        self.ui_root = wiring::optional(&self.ui_root);
        // Campaign state lives in the same settings store the sim host writes
        // (`user://config/settings.bin`), so unlocks/rules survive restarts.
        let root = godot::classes::ProjectSettings::singleton()
            .globalize_path("user://")
            .to_string();
        self.paths = Paths::new(root);
        // A content (re)boot can change the block ids/names the schematic must
        // decode against, and the data root may move: drop the filesystem
        // memos before anything reads them (finding #9).
        self.invalidate_caches();
        // One shared store (`Core.settings`, PAR-0164): the `MindUi` autoload
        // initializes it before the scene; this call is the idempotent fallback.
        settings_hub::init(&self.paths);
        // Keep the live bundle for `Core.bundle`-sourced text (unlock
        // descriptions) that the empty-`MemoryBundle` registry lacks.
        self.bundle = crate::assets::bundle::load_bundle(
            &crate::assets::loader::resolve_assets_dir(),
            &crate::assets::loader::resolve_locale(),
        );

        let bundle = MemoryBundle::new();
        // I07 T4: the one content boot path (base content + enabled mods);
        // Java `ClientLauncher.java:166-189`.
        let content = settings_hub::with_settings(|settings| {
            let store = SettingsUnlockStore::new(settings);
            crate::content_boot::build_registry(&bundle, &store)
        });
        match content {
            Ok(registry) => {
                // The facade reads database tabs and derived block/unit fields
                // (`ContentLoader.init`/`postInit` sweeps), so the boot path
                // already ran the same lifecycle the sim host/editor run.
                // `Vars.init`: `control.saves.load()` runs its legacy sector
                // remap (moving `sector-serpulo-15.msav` to Ground Zero's
                // current cell and migrating `<planet>-s-<id>-info`) before the
                // planets read sector info (`Sector.loadInfo` after remap).
                self.saves = Saves::new();
                if run_saves_load {
                    settings_hub::with_settings(|settings| {
                        self.saves.load(&NativeFs, &self.paths, &registry, settings);
                    });
                }
                if self.saves.had_serpulo_remaps
                    && let Err(error) = settings_hub::save(&self.paths)
                {
                    log::warn!("sector remap settings flush failed: {error}");
                }
                let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
                settings_hub::with_settings(|settings| campaign.load_all(settings));
                // `Saves.load` assigns sector save slots from save **meta**
                // (`Saves.java:120-186`): a readable slot backs the sector its
                // rules name, so a copy under another name still resumes
                // (CG-SAVE-05). `Saves::slots` only holds files whose meta
                // decoded (main or backup; `SaveIO.isSaveValid`), so an
                // undecodable file can no longer manufacture a base that the
                // load path would silently regenerate (CG-SAVE-06).
                let assignments = self.saves.sector_save_assignments(&registry);
                let applied =
                    mind_core::game::saves::apply_sector_saves(&mut campaign, &assignments);
                log::info!(
                    "sector saves assigned from meta: {applied} of {} readable slot(s)",
                    self.saves.slots.len()
                );
                // `Planet.init` (`Planet.java:469-471`) recomputes base coverage
                // after `sector.loadInfo`; run it once more now that the stored
                // infos and save files are on the sectors, so boot threat and
                // `hasEnemyBase` match Java (H05 T1).
                for planet in campaign.planets.values_mut() {
                    planet.update_base_coverage_from_neighbors();
                }
                self.registry = Some(registry);
                self.campaign = Some(campaign);
                self.runtime_installed = false;
                // `Vars.service` boot (`DesktopLauncher`'s `GameService`
                // implementation): the persisted `achievement-*` /
                // `achievement-stat-*` keys come out of `Core.settings` before
                // the first `CampaignRuntime` can drive the listener half.
                self.service = settings_hub::with_settings(|settings| {
                    let store = SettingsUnlockStore::new(settings);
                    load_service(&store)
                });
                log::info!(
                    "MindCampaign ready ({} save slot(s), {} achievement(s))",
                    self.saves.slots.len(),
                    self.service.service().unlocked_count()
                );
            }
            Err(error) => {
                self.content_error = Some(error.to_string());
                log::error!("MindCampaign content boot failed: {error}");
            }
        }
    }

    /// Pumps the live campaign runtime: drains tick events, applies capture
    /// bookkeeping (stats, auto-unlocks, capture replacements), advances the
    /// `Saves` policy (playtime + autosave cadence), runs due campaign turns
    /// and saves. No-op without a host/runtime.
    fn pump_runtime(&mut self) {
        let Some(mut host) = self.host() else {
            // Menu/harness facade: the `capture_sector` fallback queues rows on
            // the facade session, so drain them here too (PAR-0577 residual).
            let rows = self.session.take_event_rows();
            dispatch_event_rows(&mut self.events, rows);
            return;
        };
        let Some(mut runtime) = host.bind_mut().take_campaign_runtime() else {
            let rows = self.session.take_event_rows();
            dispatch_event_rows(&mut self.events, rows);
            return;
        };
        // PAR-0577 residual: drain the play-flow `EventRow` outbox into the
        // `EventType` bus. The runtime session is authoritative while a sector
        // is live, so its outbox is the one dispatched; the facade session's
        // rows are a clone of it (the `start_*` flows install the runtime from
        // `self.session.clone()`), and dropping them keeps a later fallback
        // drain from replaying the same rows.
        let rows = runtime.session.take_event_rows();
        let _ = self.session.take_event_rows();
        dispatch_event_rows(&mut self.events, rows);
        // The runtime's tracker is authoritative while a sector is live; mirror
        // it every pump so the persist path and the UI queries see the same
        // `Vars.service` state the campaign systems just updated (GAPS #68).
        self.service = runtime.service.clone();
        let events = runtime.take_events();
        let mut dirty = false;

        // `Control.update` -> `saves.update()`: playtime accrual and the
        // autosave cadence on the fixed-tick delta (deviation 11).
        let tick = host.bind().get_tick().max(0) as u64;
        let delta_ticks = tick.saturating_sub(self.last_pump_tick);
        self.last_pump_tick = tick;
        let saves_update = SavesUpdate {
            delta_ticks,
            is_game: runtime.session.phase.is_game(),
            paused_with_dialog: host.bind().is_paused() && self.ui_has_dialog(),
            game_over: runtime.session.game_over,
            is_campaign: runtime.session.is_campaign(),
        };
        let saves_result = if delta_ticks > 0 {
            settings_hub::with_settings(|store| {
                let interval = store.get_i32(KEY_SAVE_INTERVAL, DEFAULT_SAVE_INTERVAL_SECONDS);
                self.saves.update_policy(saves_update, store, interval)
            })
        } else {
            SavesUpdateResult::default()
        };
        // `SaveVersion.writeMeta` reads `control.saves.getTotalPlaytime()`; pass
        // the host-accrued value into the sim before any save boundary.
        let total_playtime = self.saves.total_playtime;
        host.bind_mut().set_save_playtime(total_playtime);
        // `Saves.java:210-212`: campaign playtime also lands in the planet stats
        // (the runtime copy is authoritative while the sector is live).
        if saves_result.playtime_added > 0
            && saves_update.is_campaign
            && let Some((planet, _)) = runtime.session.sector
            && let Some(stats) = runtime.campaign.stats_for_mut(planet)
        {
            stats.playtime += saves_result.playtime_added;
        }
        if saves_result.autosaved
            && let Some(file) = self.saves.current_slot().map(|slot| slot.file.clone())
        {
            // `SaveVersion.writeMeta` -> `state.rules.sector.saveInfo()`: the
            // live sector record (core snapshot, items, production, wave) lands
            // in settings next to the autosave (PAR-0153). The writer takes the
            // registry explicitly (finding #1): no facade field is borrowed
            // across the `&mut self` call.
            let registry = self.registry.take();
            self.persist_sector_record_in(Some(&mut runtime), registry.as_ref());
            self.registry = registry;
            // `GameService.storeStats` at the autosave boundary: the tracker's
            // stats/achievements land in the settings store alongside the
            // sector record (the exit persist flushes the file).
            settings_hub::with_settings(|settings| {
                let mut store = SettingsUnlockStore::new(settings);
                save_service(&mut self.service, &mut store);
            });
            // `Saves.SaveSlot.save` -> `savePreview()` (autosave path).
            self.write_live_slot_preview(&file);
            host.bind_mut()
                .request_save(GString::from(file.to_string_lossy().as_ref()), false);
            self.saving_finish_tick = Some(tick.saturating_add(3));
            dirty = true;
            log::info!("autosave queued (`{}`)", file.display());
        }
        if self.saving_finish_tick.is_some_and(|finish| tick >= finish) {
            self.saves.finish_saving();
            self.saving_finish_tick = None;
        }

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
                    // `Logic.sectorCapture` (`Logic.java:398-401`):
                    // `control.saves.saveSector(state.rules.sector)` — the
                    // captured world is saved immediately, like the GameOver arm.
                    if let Some(file) = self.saves.current_slot().map(|slot| slot.file.clone()) {
                        self.write_live_slot_preview(&file);
                        host.bind_mut()
                            .request_save(GString::from(file.to_string_lossy().as_ref()), false);
                        self.saving_finish_tick = Some(tick.saturating_add(3));
                        log::info!("capture save queued (`{}`)", file.display());
                    }
                    self.persist_sector_record_in(Some(&mut runtime), registry.as_ref());
                }
                mind_core::game::play::PlayEvent::CampaignComplete { .. } => {
                    self.campaign_complete = true;
                    let _ = self.base_mut().emit_signal("campaign_complete", &[]);
                }
                mind_core::game::play::PlayEvent::GameOver { .. }
                | mind_core::game::play::PlayEvent::SectorLose { .. } => {
                    // `Sector.isBeingPlayed` (`Sector.java:148-151`) is false
                    // once `state.gameOver` is set; clear it on the runtime copy
                    // so the sector produces offline on later turns (T1).
                    if let Some((planet, sector_id)) = runtime.session.sector
                        && let Some(sector) = runtime.campaign.sector_mut(planet, sector_id)
                    {
                        sector.being_played = false;
                    }
                    // `Control` GameOverEvent (`Control.java:188-195`): save the
                    // game-over state immediately, so a later re-attack loads
                    // the destroyed base (PAR-0084).
                    if let Some(file) = self.saves.current_slot().map(|slot| slot.file.clone()) {
                        self.write_live_slot_preview(&file);
                        host.bind_mut()
                            .request_save(GString::from(file.to_string_lossy().as_ref()), false);
                        self.saving_finish_tick = Some(tick.saturating_add(3));
                        log::info!("game-over save queued (`{}`)", file.display());
                    }
                    // `SaveVersion.writeMeta` persists the sector record next to
                    // the save (spawn position + prepared core state), which the
                    // lost-sector replay reads on the re-attack.
                    self.persist_sector_record_in(Some(&mut runtime), registry.as_ref());
                    dirty = true;
                }
                _ => {}
            }
        }

        // T2: produced/dumped content unlocks (`UnlockableContent.unlock()` ->
        // `UnlockEvent` -> `Control.java:160-174`). Resolve the queued refs on
        // the registry, persist the unlock, then run `checkAutoUnlocks()`.
        let mut unlocked_block = false;
        let mut unlocked_content: Vec<ContentRef> = Vec::new();
        if !runtime.pending_unlocks.is_empty()
            && let Some(registry) = registry.as_mut()
        {
            let mut newly: Vec<String> = Vec::new();
            for unlock in std::mem::take(&mut runtime.pending_unlocks) {
                let (content, name) = match unlock {
                    UnlockRef::Item(id) => (
                        ContentRef::of(ContentType::Item, id),
                        registry.item(id).map(|c| c.name.clone()),
                    ),
                    UnlockRef::Liquid(id) => (
                        ContentRef::of(ContentType::Liquid, id),
                        registry.liquid(id).map(|c| c.name.clone()),
                    ),
                    UnlockRef::Unit(id) => (
                        ContentRef::of(ContentType::Unit, id),
                        registry.unit(id).map(|c| c.name.clone()),
                    ),
                };
                let Some(name) = name else {
                    continue;
                };
                let unlocked = settings_hub::with_settings(|settings| {
                    let mut store = SettingsUnlockStore::new(settings);
                    if let Some(fields) = tech_tree::unlock_fields_mut(registry, content)
                        && fields.unlock(&name, &mut store)
                    {
                        true
                    } else {
                        false
                    }
                });
                if unlocked {
                    unlocked_content.push(content);
                    newly.push(name);
                }
            }
            if !newly.is_empty() {
                let auto = settings_hub::with_settings(|settings| {
                    let ctx = CampaignObjectiveContext::new(registry, &runtime.campaign);
                    let mut store = SettingsUnlockStore::new(settings);
                    tech_tree::check_auto_unlocks(registry, &mut store, &ctx)
                });
                log::info!(
                    "campaign unlock: {} produced, {} auto-unlocked",
                    newly.len(),
                    auto.len()
                );
                for content in auto {
                    if content.type_ == ContentType::Block {
                        unlocked_block = true;
                    }
                    unlocked_content.push(content);
                    if let Some(name) = tech_tree::content_name(registry, content) {
                        newly.push(name);
                    }
                }
                self.unlocked_since_last_take.extend(newly);
                let unlocked = collect_unlocked(registry);
                runtime.set_unlocked_content(unlocked);
                dirty = true;
            }
        }
        // H04 T6: legacy launch pads queue destination records for the host to
        // flush (`Sector.saveInfo` after `addItems`).
        if !runtime.pending_info_saves.is_empty() {
            for (planet, sector_id) in std::mem::take(&mut runtime.pending_info_saves) {
                let planet_name = runtime
                    .campaign
                    .planet(planet)
                    .map(|planet| planet.name.clone())
                    .unwrap_or_default();
                if let Some(sector) = runtime.campaign.sector(planet, sector_id) {
                    settings_hub::with_settings(|settings| {
                        let _ = sector.save_info(settings, &planet_name);
                    });
                }
            }
            dirty = true;
        }
        self.registry = registry;

        if unlocked_block {
            // `UnlockEvent` with a block reached the facade: the placement
            // palette rebuilds (`PlacementFragment.java:92-96`).
            let _ = self.base_mut().emit_signal("block_unlocked", &[]);
        }
        if let Some(registry) = self.registry.as_ref() {
            // `Control.java:160-165`: each drained unlock shows the toast when
            // `showUnlock()` is true.
            for content in &unlocked_content {
                self.push_unlock_toast(registry, *content);
            }
        }

        if !replacements.is_empty() {
            let replaced = host.bind_mut().apply_capture_replacements(&replacements);
            if replaced > 0 {
                dirty = true;
            }
        }

        // Auto campaign turns (`Universe.update` turnCounter).
        if runtime.turn_due {
            runtime.turn_due = false;
            let mut turn_events = Vec::new();
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
                let report = settings_hub::with_settings(|settings| {
                    campaign.run_turn(registry, settings, &ctx, &mut rand)
                });
                log::info!(
                    "campaign turn {} ({} event(s))",
                    report.turn,
                    report.events.len()
                );
                turn_events = report.events;
                dirty = true;
            }
            // Played-sector invasions restart the live waves (T4).
            mind_core::game::runtime::apply_played_invasions(&mut runtime, &turn_events);
            // T7: toasts for off-play sectors (`HudFragment.java:261-275`).
            if !turn_events.is_empty() {
                self.toast_campaign_events(&turn_events);
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
        // H09 T10: `game_stats_json` reads the live counters (small clone).
        self.session.stats = runtime.session.stats.clone();
        if mirrored {
            self.session = runtime.session.clone();
        }
        // T1: while `dirty`, the facade must carry the runtime's campaign
        // before `persist()` runs (the runtime is authoritative; the facade is
        // only a view once this pump returns the runtime to the host).
        if mirrored || dirty {
            self.campaign = Some(runtime.campaign.clone());
        }
        host.bind_mut().put_campaign_runtime(runtime);
        // The facade campaign copy feeds the views/persist; keep its playtime in
        // step with the runtime accrual (a mirrored clone already carries it).
        if !mirrored
            && saves_result.playtime_added > 0
            && saves_update.is_campaign
            && let Some((planet, _)) = self.session.sector
            && let Some(stats) = self
                .campaign
                .as_mut()
                .and_then(|campaign| campaign.stats_for_mut(planet))
        {
            stats.playtime += saves_result.playtime_added;
        }
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
        let unlocked = settings_hub::with_settings(|settings| {
            let ctx = CampaignObjectiveContext::new(registry, &runtime.campaign);
            let mut store = SettingsUnlockStore::new(settings);
            tech_tree::check_auto_unlocks(registry, &mut store, &ctx)
        });
        if !unlocked.is_empty() {
            log::info!("campaign capture auto-unlocked {} content", unlocked.len());
        }
        for content in &unlocked {
            // `Control.java:160-165` (`UnlockEvent`) half.
            self.push_unlock_toast(registry, *content);
        }
        if unlocked
            .iter()
            .any(|content| content.type_ == ContentType::Block)
        {
            // `UnlockEvent` with a block: the palette rebuilds on capture
            // unlocks (`PlacementFragment.java:92-96`).
            let _ = self.base_mut().emit_signal("block_unlocked", &[]);
        }
    }

    /// Emitted on the first capture of a planet's last sector (campaign
    /// victory); the UI opens `campaign_complete`.
    #[signal]
    fn campaign_complete();

    /// Emitted when an unlock performed during play includes a block
    /// (`UnlockEvent`; `J/ui/fragments/PlacementFragment.java:92-96` rebuilds
    /// the palette). Never emitted while a host bind is held.
    #[signal]
    fn block_unlocked();

    /// The file backing a sector's save association (`Sector.save` stores the
    /// slot's **stem**, `Saves.SaveSlot.index`): the listed slot with that
    /// stem, else the canonical sector file when the stem names it (a save
    /// queued earlier this session), else `saves/<stem>.msav`.
    ///
    /// Never re-derives the path from the sector id alone: the association
    /// came from save meta, so a copy under another name resolves to its own
    /// file (CG-SAVE-05).
    fn sector_save_file(&self, planet_name: &str, sector: u16, stem: &str) -> std::path::PathBuf {
        if let Some(slot) = self.saves.slot_for_stem(stem) {
            return slot.file.clone();
        }
        let canonical = self.paths.sector_save(planet_name, sector as u32);
        if canonical
            .file_stem()
            .is_some_and(|name| name.to_string_lossy() == stem)
        {
            canonical
        } else {
            self.paths.saves().join(format!("{stem}.{SAVE_EXTENSION}"))
        }
    }

    /// Primes the host's sector-generation extras from the campaign record
    /// (`Sector.threat`/`hasEnemyBase`/`hasBase`) so the next `load_sector` of
    /// a numbered sector generates with the live campaign state (H05 T2).
    /// Call before `MindSimHost.load_sector` for the same planet+sector.
    ///
    /// Also hands the host the sector's associated save file
    /// (`Control.playSector` -> `Sector.save`, `Control.java:450-456`): the
    /// resume source is the slot the meta association picked, not a guessed
    /// file name (CG-SAVE-05).
    #[func]
    pub fn prime_sector_load(&mut self, planet: GString, sector: i32) -> bool {
        let planet_name = planet.to_string();
        if sector < 0 {
            return false;
        }
        let Some(campaign) = self.campaign.as_ref() else {
            return false;
        };
        let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
            return false;
        };
        let Some(record) = campaign.sector(planet_id, sector as u16) else {
            return false;
        };
        let extras = mind_core::maps::planet::SectorGenExtras {
            threat: record.threat,
            has_enemy_base: record.has_enemy_base(),
            has_base: record.has_base(),
        };
        let save_file = record
            .save
            .as_deref()
            .map(|stem| self.sector_save_file(&planet_name, sector as u16, stem));
        let Some(mut host) = self.host() else {
            return false;
        };
        host.bind_mut()
            .set_sector_gen_extras_rs(&planet_name, sector, extras);
        host.bind_mut().set_sector_resume_file(GString::from(
            save_file
                .map(|file| file.to_string_lossy().into_owned())
                .unwrap_or_default()
                .as_str(),
        ));
        true
    }

    /// Starts a campaign sector with the default (empty) launch loadout,
    /// returning success. Delegates to [`Self::start_sector_with_loadout`] with
    /// an empty payload; use that method to pass an explicit
    /// `[{item, amount}]` loadout.
    #[func]
    pub fn start_sector(&mut self, planet: GString, sector: i32) -> bool {
        self.start_sector_with_loadout(planet, sector, GString::from(""))
    }

    /// Starts a campaign sector with an explicit launch loadout (WS3 wiring):
    /// `loadout_json` is the optional `[{item, amount}]` payload applied on the
    /// fresh-launch path (empty string = default).
    #[func]
    pub fn start_sector_with_loadout(
        &mut self,
        planet: GString,
        sector: i32,
        loadout_json: GString,
    ) -> bool {
        self.current_map_name = None;
        // `logic.reset()` (`state = new GameState()`): not a playtest.
        self.playtesting_map = None;
        let planet_name = planet.to_string();
        if sector < 0 {
            return false;
        }
        // `Control.playSector` (`Control.java:437-441`): a launch that replaces
        // a live match writes the outgoing slot first (CG-SECTOR-02). The
        // caller must have done this BEFORE the destination world load (the
        // planet dialog does it via `save_outgoing_sector`; the accelerator in
        // `pump_accelerator_launch`), because by now the host already holds the
        // destination world.
        let sector_id = sector as u16;
        let Some(mut registry) = self.registry.take() else {
            return false;
        };
        // T1: a launch while a sector is live starts from the runtime's state
        // (the old runtime is replaced below by `install_campaign_runtime`).
        self.mirror_runtime_campaign();
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
        let mut has_save = record.has_save();
        let spawn_position = record.info.info.spawn_position;
        let had_no_core = !record.info.info.has_core;
        let tech_unlocked = preset_id.is_none_or(|preset| {
            tech_tree::content_unlocked(&registry, ContentRef::of(ContentType::Sector, preset))
        });
        if require_unlock && !always_unlocked && !tech_unlocked {
            log::warn!("start_sector: `{planet_name}:{sector_id}` is locked");
            self.registry = Some(registry);
            self.campaign = Some(campaign);
            return false;
        }
        // `Control.playSector`'s `catch(SaveException)` (`Control.java:555-561`):
        // the host found the associated resume save unreadable between listing
        // and load. Java clears `sector.save`, deletes the slot, reports
        // `@save.corrupted` and replays the sector; it never resumes a
        // regenerated world under the stale record (CG-SAVE-06). The load path
        // only flags it — the record is cleared here, before the resume branch
        // is chosen.
        let corrupt_resume = self
            .host()
            .and_then(|mut host| host.bind_mut().take_corrupt_sector_save());
        if let Some(path) = corrupt_resume {
            let file = std::path::PathBuf::from(&path);
            if let Some(sector) = campaign.sector_mut(planet_id, sector_id) {
                sector.save = None;
            }
            has_save = false;
            // `SaveSlot.delete()`: the file and its backup, then the list entry
            // (`Slot.delete` removes itself from `saves`).
            let mut deleted = false;
            if let Some(slot) = self.saves.slots.iter_mut().find(|slot| slot.file == file) {
                deleted = slot.delete(&NativeFs).is_ok();
            }
            if !deleted {
                let _ = NativeFs.delete(&file);
                let _ = NativeFs.delete(&mind_core::io::SaveIo::backup_file_for(&file));
            }
            self.saves.forget_file(&file);
            // `Time.runTask(10f, () -> ui.showErrorMessage("@save.corrupted"))`.
            self.show_error("@save.corrupted");
            log::warn!(
                "start_sector: `{planet_name}:{sector_id}` resume save `{path}` was corrupt; \
                 deleted the slot and replaying the sector (`@save.corrupted`)"
            );
        }

        // `Control.playSector` (`Control.java:437-448`): quiet-unlock the preset
        // (no unlock event) and record the planet's last sector.
        let preset_name =
            preset_id.and_then(|preset| registry.sector(preset).map(|def| def.name.clone()));
        if let Some(preset) = preset_id
            && let Some(name) = preset_name.as_deref()
        {
            settings_hub::with_settings(|settings| {
                if let Some(fields) = tech_tree::unlock_fields_mut(
                    &mut registry,
                    ContentRef::of(ContentType::Sector, preset),
                ) {
                    fields.quiet_unlock(name, &mut SettingsUnlockStore::new(settings));
                }
            });
        }
        if let Some(planet) = campaign.planet_mut(planet_id) {
            settings_hub::with_settings(|settings| planet.set_last_sector(settings, sector_id));
        }

        // `Control.playSector`'s branch selection (`Control.java:449-470`): the
        // normal load path is a save with a live core; a save whose sector was
        // lost (`no_live_core || had_no_core`) takes the no-core branch, which
        // replays only when `!clearSave && spawnPosition != 0 &&
        // sectorDataMatches` (PAR-0084).
        let default_team = self.session.rules.default_team;
        let no_live_core = self
            .host()
            .map(|host| host.bind().player_core_blocks(default_team) == 0)
            .unwrap_or(false);
        let no_core_branch = no_live_core || had_no_core;
        let clear_save = campaign
            .planet(planet_id)
            .is_some_and(|planet| planet.campaign_rules.clear_sector_on_lose);
        let data_matches = campaign.sector(planet_id, sector_id).is_some_and(|sector| {
            // Deviation: the preset def carries no map dimensions, so the
            // record's own size is passed; the effective test is the preset
            // name (`SectorInfo.sectorDataMatches`, `SectorInfo.java:98-103`).
            sector.sector_data_matches(
                preset_name.as_deref(),
                sector.info.info.last_width,
                sector.info.info.last_height,
            )
        });
        let lost_replay =
            has_save && no_core_branch && !clear_save && spawn_position != 0 && data_matches;
        // Java's load branch: a save that is not in the no-core branch.
        let resume = has_save && !no_core_branch;
        let (cores, core_teams) = self
            .host()
            .map(|host| {
                let guard = host.bind();
                (guard.player_core_blocks(default_team), guard.core_teams())
            })
            .unwrap_or_default();
        log::info!(
            "start_sector branch: sector={planet_name}:{sector_id} has_save={has_save} \
             had_no_core={had_no_core} no_live_core={no_live_core} default_team={default_team} \
             clear_save={clear_save} data_matches={data_matches} lost_replay={lost_replay} \
             resume={resume} cores={cores:?} teams={core_teams:?}"
        );
        let mut restored_survivors = Vec::new();
        if lost_replay {
            // H05 T2: the replay regenerates the world through the host, so
            // prime the generator extras from the live record first.
            if let Some(record) = campaign.sector(planet_id, sector_id)
                && let Some(mut host) = self.host()
            {
                host.bind_mut().set_sector_gen_extras_rs(
                    &planet_name,
                    sector,
                    mind_core::maps::planet::SectorGenExtras {
                        threat: record.threat,
                        has_enemy_base: record.has_enemy_base(),
                        has_base: record.has_base(),
                    },
                );
            }
            let (spawn_x, spawn_y) = mind_core::world::pos::unpack(spawn_position);
            restored_survivors = self
                .host()
                .map(|mut host| {
                    host.bind_mut().replay_lost_sector(
                        planet.clone(),
                        sector,
                        i32::from(spawn_x),
                        i32::from(spawn_y),
                    )
                })
                .unwrap_or_default();
            if let Some(sector_record) = campaign.sector_mut(planet_id, sector_id) {
                sector_record.info.info.has_core = true;
            }
            log::info!(
                "start_sector: lost-sector replay for `{planet_name}:{sector_id}` restored {} survivor(s)",
                restored_survivors.len()
            );
        }

        // Optional launch loadout (`GAP-10`/PAR-0085). A campaign launch without
        // an explicit loadout carries an EMPTY loadout: the `Rules` default
        // (`copper x100`) is the custom-game default and must not stock a
        // campaign core. Java's reference Ground Zero resume save stores
        // `loadout:[]` (runs/20261009-065416-ref-ground-zero/checkpoints/
        // r01-sector-serpulo-170.msav, decompressed meta) and starts with an
        // empty core; `FileMapGenerator`/`Logic.play` then clear the core and
        // add nothing (PAR-0056).
        let loadout_payload = loadout_json.to_string();
        if loadout_payload.trim().is_empty() {
            self.session.rules.loadout.clear();
        } else if let Some(stacks) = parse_loadout_stacks(&loadout_payload) {
            self.session.rules.loadout = stacks;
        }
        let (launch_schematic, launch_origin, launch_stacks_json) =
            parse_launch_payload(&loadout_payload);
        // The accelerator envelope's `launch_core` (`Universe.updateLoadout(
        // CoreBlock)`, L06 T7).
        let launch_core: Option<String> =
            serde_json::from_str::<serde_json::Value>(&loadout_payload)
                .ok()
                .and_then(|value| value.get("launch_core")?.as_str().map(str::to_owned));
        // `Control.playNewSector` (`Control.java:573-578`): the launcher sector
        // id from the dialog becomes `origin`/`destination`.
        let origin = launch_origin.map(|origin| (planet_id, origin));

        // `Sector.isBeingPlayed` (`Sector.java:148-151`): only the sector being
        // played reads true; a launch clears every previous flag first (T1).
        for planet in campaign.planets.values_mut() {
            for sector in &mut planet.sectors {
                sector.being_played = false;
            }
        }

        self.reloader = HostReloader::default();
        let events = if resume {
            play_sector(
                &mut self.session,
                &mut campaign,
                &registry,
                planet_id,
                sector_id,
                origin,
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
                origin,
                &mut self.epoch,
                &mut self.reloader,
            )
        };
        // T2: `readMeta`'s resume fields — `wavetime`/`stats`/markers — from the
        // save `MindSimHost.load_sector` just decoded. Java restores them inside
        // `SaveIO.load` (`SaveVersion.readMeta`) before the `SaveLoadEvent`
        // rule merge; the host also resumed the saved `tick`. The
        // `static-fog-data` chunk is applied after the rules adoption below
        // (it needs the live `FogControl` created by `reset_fog`).
        let mut resume_static_fog: Option<Vec<u8>> = None;
        if resume
            && let Some(meta) = self
                .host()
                .and_then(|mut host| host.bind_mut().take_sector_resume())
        {
            self.session.wavetime = meta.wavetime.unwrap_or(self.session.rules.wave_spacing);
            if let Some(stats) = meta.stats {
                self.session.stats = stats;
            }
            self.session.markers = meta.markers;
            resume_static_fog = meta.static_fog;
        }
        // Adopt the loaded map/save rules wholesale (T5:
        // `SaveVersion.readRules` + `World.setSectorRules`) and re-derive the
        // world-side state (map-area limit rect + border darkness).
        if let Some(map_rules) = self
            .host()
            .and_then(|host| host.bind().last_sector_rules().cloned())
            && let Some(planet) = campaign.planet(planet_id)
        {
            let preset_no_lighting = campaign
                .sector(planet_id, sector_id)
                .is_some_and(|sector| sector.preset_no_lighting);
            adopt_map_rules(
                &mut self.session,
                &map_rules,
                planet,
                &registry,
                preset_id.is_some(),
                preset_no_lighting,
                resume,
            );
            if let Some(mut host) = self.host() {
                host.bind_mut().apply_world_rules(&self.session.rules);
            }
        }
        // `World.loadSector` -> `setSectorRules` -> `planet.generator.addWeather`
        // (World.java:292,303): fresh generations get terrain weather, replacing
        // the preset map's entries; resumed saves keep theirs.
        let fresh = !resume;
        if fresh
            && mind_core::maps::generators::planet::planet_adds_weather(&planet_name)
            && let Some(host) = self.host()
        {
            let host = host.bind();
            if let Some(content) = host.content_registry() {
                self.session.rules.weather = mind_core::maps::generators::planet::add_weather(
                    host.grid(),
                    content,
                    &self.session.rules,
                );
            }
        }
        // `Logic.play` installs the map objective executor from the final rules;
        // the play-flow events ran before the map merge above, so rebuild the
        // runtime here or the tutorial's first objective never qualifies
        // (PAR-0037: resumed Ground Zero showed no objective state).
        self.session.objectives = MapObjectivesRuntime::from_rules(&self.session.rules);

        // `apply_team_plans`: the resume save's build plans (and the replay's
        // restored survivors) land in the live session's team data before the
        // runtime snapshot is taken (PAR-0084).
        if (has_save || lost_replay)
            && let Some(mut host) = self.host()
        {
            let carried = host.bind_mut().take_sector_plans();
            mind_core::io::sim_io::apply_team_plans(&mut self.session.teams, carried);
        }
        if lost_replay && !restored_survivors.is_empty() {
            let data = self.session.teams.get(mind_core::ecs::TeamId(default_team));
            for entry in &restored_survivors {
                data.plans.push_back(mind_core::game::teams::BlockPlan {
                    x: entry.x,
                    y: entry.y,
                    rotation: entry.rot as i8,
                    block: entry.block,
                    config: None,
                    removed: false,
                });
            }
        }

        // Install the runtime and register the live cores/spawns (GAP-9).
        let mut installed = false;
        if let Some(mut host) = self.host() {
            let mut runtime = CampaignRuntime::new(self.session.clone(), campaign.clone())
                // One shared `Vars.service` (GAPS #68): the facade's boot-loaded
                // tracker goes into the live runtime, so the campaign systems
                // keep counting into the same achievement/stat graph.
                .with_service(self.service.clone());
            runtime.set_unlocked_content(collect_unlocked(&registry));
            host.bind_mut().install_campaign_runtime(runtime);
            // `Schematics.placeLaunchLoadout` (PAR-0085 step 5): a launcher
            // launch replaces the map's core with the loadout schematic's core
            // before the session sync registers and stocks it. Fresh launches
            // only (`playNewSector`); a resume keeps the saved world unchanged.
            // H05 T4: a generated sector already placed its loadout during the
            // world load (the generator runs `placeLaunchLoadout`).
            if !has_save
                && self.session.allow_launch_loadout
                && let Some(index) = launch_schematic
                && !host.bind().generated_loadout_placed()
            {
                host.bind_mut()
                    .place_launch_loadout(index, GString::from(launch_stacks_json.as_str()));
            }
            // L06 T7: the accelerator's `updateLoadout(CoreBlock)` single-core
            // loadout on a preset map (`FileMapGenerator` placeLaunchLoadout).
            if !has_save
                && self.session.allow_launch_loadout
                && launch_schematic.is_none()
                && let Some(core) = launch_core.as_deref()
                && !host.bind().generated_loadout_placed()
            {
                host.bind_mut().place_launch_core(
                    GString::from(core),
                    GString::from(launch_stacks_json.as_str()),
                );
            }
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
                let mut core_items: Option<indexmap::IndexMap<String, i32>> = None;
                if let Some(mut runtime) = taken {
                    // Carry the launch ownership back into the runtime copy.
                    runtime.campaign = campaign.clone();
                    if resume {
                        // `Logic` `SaveLoadEvent` (`Logic.java:70-88`):
                        // `info.write()` pushes the stored record into the live
                        // session, then `planet.applyRules` re-derives the
                        // campaign rules over it (H04 T3).
                        let wave_core = runtime.session.wave_core_count() > 0;
                        let allow_waves = campaign
                            .planet(planet_id)
                            .is_some_and(|planet| planet.allow_waves);
                        if let Some(record) = runtime.campaign.sector_mut(planet_id, sector_id) {
                            let capture_wave = record.preset_capture_wave;
                            let capacity = record.info.info.storage_capacity;
                            let items = record.info.write_to_state(
                                wave_core,
                                allow_waves,
                                capture_wave,
                                &mut runtime.session,
                            );
                            record.info.info.has_core = true;
                            settings_hub::with_settings(|settings| {
                                let _ = record.save_info(settings, &planet_name);
                            });
                            // Keep the session team inventory equal to the core
                            // module, like a fresh launch (`sync_session_with_sim`).
                            let item_count = registry.items().len();
                            let inventory = runtime
                                .session
                                .teams
                                .inventory(mind_core::ecs::TeamId(default_team), item_count);
                            inventory.clear();
                            for (name, amount) in &items {
                                if let Some(item) = registry.item_id(name) {
                                    inventory.add(item, (*amount).clamp(0, capacity), i32::MAX);
                                }
                            }
                            core_items = Some(items);
                        }
                        if let Some(planet) = campaign.planet(planet_id) {
                            planet.apply_rules(&registry, &mut runtime.session.rules, false, true);
                        }
                    }
                    // `Trigger.newGame` (`Control.java:212-216`):
                    // `if(info.importRateCache != null) info.refreshImportRates(planet)`
                    // so the played sector's landing pads see its importers (T7).
                    runtime
                        .campaign
                        .refresh_sector_import_rates(&registry, planet_id, sector_id);
                    self.session = runtime.session.clone();
                    host.bind_mut().put_campaign_runtime(runtime);
                    // H04 T13: `UnitComp.afterRead` (`UnitComp.java:585-593`).
                    // The world load spawned the saved units before the session
                    // rules existed, so a player-team unit came back as
                    // `GroundAI`; re-derive the controllers now that the
                    // runtime is installed. A fresh launch has no restored
                    // units, so the call is a no-op there.
                    host.bind_mut().reset_loaded_unit_controllers();
                }
                if let Some(items) = core_items
                    && let Some(mut host) = self.host()
                {
                    host.bind_mut()
                        .write_record_items_to_core(default_team, &items);
                }
                installed = true;
            }
        }

        // `static-fog-data`: applied after the campaign runtime is installed
        // (only then can `reset_fog` build the `FogControl` from `rules.fog`).
        if installed
            && let Some(bytes) = resume_static_fog.take()
            && let Some(mut host) = self.host()
        {
            host.bind_mut().apply_static_fog(bytes);
        }

        // `CoreBlock.playerSpawn` (upstream `Trigger.newGame` → `player.set(core)`):
        // spawn the player mech at the best core so `player.unit()` is live after
        // the launch and WASD drives it instead of the camera.
        if installed
            && let Some(mut host) = self.host()
            && !host.bind_mut().spawn_player_unit()
        {
            log::warn!("start_sector: player unit spawn failed");
        }

        // `Trigger.newGame` (`Control.java:198-222`): the core drops in with the
        // 160-tick landing cutscene unless `skipcoreanimation` or a PvP rule set
        // (PAR-0131). The scaled core sprite/fade/announce ride the camera's
        // `get_land_time_in` from the UI; the core block name is unused until
        // that draw lands.
        // Java fires `Trigger.newGame` only from `playNewSector`/`playMap`
        // (Control.java:415,589); a plain resume (`slot.load`) has no landing.
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

        self.registry = Some(registry);
        self.campaign = Some(campaign);
        self.runtime_installed = installed;
        if let Some(host) = self.host() {
            self.last_pump_tick = host.bind().get_tick().max(0) as u64;
        }
        self.invalidate_ui_block_catalog();
        if installed
            && let Some(save_name) = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(planet_id, sector_id))
                .and_then(|sector| sector.save.clone())
        {
            // `Sector.save` is the slot's stem; resolve it through the listed
            // slots so a resumed copy under another name keeps writing back to
            // its own file (`Saves.saveSector` reuses `sector.save`,
            // `Saves.java:257-259`) (CG-SAVE-05).
            let file = self.sector_save_file(&planet_name, sector_id, &save_name);
            if has_save {
                // `World.loadSector` -> `SaveSlot.load`: the resumed slot is
                // current, so the save policy pumps against it.
                settings_hub::with_settings(|settings| {
                    self.saves.set_current_by_file(settings, &file);
                });
            } else {
                // `Control.playNewSector` -> `Saves.saveSector`: create the
                // sector's autosave slot and make it current; the world write is
                // queued at the IO seam.
                settings_hub::with_settings(|settings| {
                    self.saves
                        .prepare_sector_slot(settings, &self.paths, &planet_name, sector_id);
                });
                // `SaveVersion.writeMeta` (`SaveVersion.java:117-118`) runs
                // `state.rules.sector.info.prepare(sector)` + `saveInfo()` on
                // EVERY save, including the launch save. Without it the stored
                // record stays at the default (empty items, `wave: 1`), so a
                // crash in the first session resumes an empty core at wave 1
                // while the tile save has everything (CG-SAVE-01).
                self.persist_sector_record();
                if let Some(mut host) = self.host() {
                    let path = format!("user://saves/{save_name}.msav");
                    // `Saves.saveSector` -> `SaveSlot.save` -> `savePreview()`.
                    self.write_live_slot_preview(std::path::Path::new(&save_name));
                    host.bind_mut()
                        .request_save(GString::from(path.as_str()), false);
                }
            }
        }
        // Persist attempts/rules and the launch slot bookkeeping (name,
        // autosave flag, `last-sector-save`); the live tile save is queued for
        // the next IO tick (`Control.playNewSector` -> `saves.saveSector`).
        self.persist();
        self.runtime_installed && !events.is_empty()
    }

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
            log::warn!("start_map_mode: unknown map `{map_name}`");
            self.show_error("@map.invalid");
            return false;
        };
        let Some(mode_id) = Gamemode::from_name(&mode_name) else {
            log::warn!("start_map_mode: unknown mode `{mode_name}`");
            self.show_error("@map.invalid");
            return false;
        };
        if self.campaign.is_none() {
            log::warn!("start_map_mode: campaign unavailable");
            self.show_error("@map.invalid");
            return false;
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
                    log::warn!("start_map_mode: bad rules JSON: {error}");
                    self.show_error("@map.invalid");
                    return false;
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
            log::warn!(
                "start_map_mode: `{map_name}` has no core for team {}",
                rules.default_team
            );
            self.show_error_key("map.nospawn", &[team.as_str()]);
            self.set_menu_visible(true);
            return false;
        } else if rules.pvp && core_teams.len() < 2 {
            log::warn!("start_map_mode: `{map_name}` has fewer than two core teams");
            self.show_error("@map.nospawn.pvp");
            self.set_menu_visible(true);
            return false;
        } else if rules.attack_mode && cores_of(map_rules.wave_team) == 0 {
            let team = mind_core::game::team::Team::get(map_rules.wave_team).colored_name();
            log::warn!("start_map_mode: `{map_name}` has no core for the wave team");
            self.show_error_key("map.nospawn.attack", &[team.as_str()]);
            self.set_menu_visible(true);
            return false;
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
                let file = self.paths.saves().join(format!(
                    "{}.{}",
                    sanitize_slot_name(&name),
                    mind_core::io::fs::SAVE_EXTENSION
                ));
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
                runtime.set_unlocked_content(collect_unlocked(&registry));
                host.bind_mut().install_campaign_runtime(runtime);
                // A custom map has no save to resume: the launch path applies
                // the loadout (`Logic.play` starting items; `Logic.java:277-285`).
                let sync = host.bind_mut().sync_campaign_session(&registry, true);
                self.registry = Some(registry);
                if sync.is_some() {
                    let taken = host.bind_mut().take_campaign_runtime();
                    if let Some(runtime) = taken {
                        self.session = runtime.session.clone();
                        host.bind_mut().put_campaign_runtime(runtime);
                    }
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

    /// The map recorded by the last playtest start (`state.playtestingMap`);
    /// the editor's `resumeAfterPlaytest` reads it. `""` when the last start
    /// was not a playtest.
    #[func]
    pub fn playtesting_map(&self) -> GString {
        GString::from(self.playtesting_map.as_deref().unwrap_or(""))
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

    /// Forces sector capture (`Logic.sectorCapture`), routing through the live
    /// runtime when one is installed so events and bookkeeping stay in sync.
    #[func]
    pub fn capture_sector(&mut self) -> bool {
        let captured = if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                // `SectorCaptureEvent` listener inputs: Java fires the event
                // inside `Logic.sectorCapture` before `rules.attackMode =
                // false`, so snapshot the attacking flag and the pre-capture
                // loss counter first (finding #2).
                let was_attack_mode = runtime.session.rules.attack_mode;
                let buildings_destroyed = runtime.session.stats.buildings_destroyed;
                let events = sector_capture(&mut runtime.session, &mut runtime.campaign);
                let ok = !events.is_empty();
                for event in events {
                    // Shared with `game_state_tick_system` (GAPS #68 /
                    // finding #2): the `GameService` capture hooks
                    // (`on_sector_capture`) and the `Logic.java:117-121`
                    // derelict-wave-team queue now run on this path too, so a
                    // capture here counts for achievements.
                    if let mind_core::game::play::PlayEvent::SectorCapture { planet, .. } = &event {
                        on_captured_sector(
                            &mut runtime,
                            *planet,
                            was_attack_mode,
                            buildings_destroyed,
                        );
                    }
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
            // `Logic.sectorCapture`'s "save, just in case": the capture
            // bookkeeping must be on disk before a restart (PAR-0081).
            self.persist_sector_record();
            // `onSectorCaptured` -> `reloadMeshAsync` (T14).
            self.planet_mesh_cache = None;
            return true;
        }
        // Menu fallback on the facade copy.
        let Some(mut campaign) = self.campaign.take() else {
            return false;
        };
        let events = sector_capture(&mut self.session, &mut campaign);
        self.campaign = Some(campaign);
        self.planet_mesh_cache = None;
        !events.is_empty()
    }

    /// `HudFragment.showToast` for `CampaignEvent`s (`HudFragment.java:261-275`):
    /// `sector.attacked` with the warning icon on a `SectorInvasion`. The
    /// off-play `sector.capture`/`sector.lost` toasts land when the
    /// `CampaignEvent` variants exist (seam §6). The label itself is suppressed
    /// in the menu by `ui_root.gd _on_toast` (`HudFragment.java:753-754`).
    fn toast_campaign_events(&self, events: &[mind_core::game::universe::CampaignEvent]) {
        use mind_core::game::universe::CampaignEvent;
        for event in events {
            let CampaignEvent::SectorInvasion { planet, sector, .. } = event else {
                continue;
            };
            let planet_record = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.planet(*planet));
            let planet_name = planet_record
                .map(|planet| planet.name.clone())
                .unwrap_or_default();
            // `Sector.name()` (`Sector.java:153-164`) through the one shared
            // implementation; the record can be missing only if the event's
            // sector is out of range (the port's defensive fallback is the bare
            // grid index, never Java's `<planet>-<id>`).
            let name = planet_record
                .zip(
                    self.campaign
                        .as_ref()
                        .and_then(|campaign| campaign.sector(*planet, *sector)),
                )
                .map(|(planet, record)| record.display_name(planet))
                .unwrap_or_else(|| sector.to_string());
            log::info!("campaign event: `{planet_name}:{sector}` invaded (toast sector.attacked)");
            let Some(mut hud) = self.base().get_node_or_null("/root/MindHud") else {
                continue;
            };
            let text = self
                .base()
                .get_node_or_null("/root/MindAssets")
                .and_then(|mut assets| {
                    assets
                        .call(
                            "bundle_format",
                            &[
                                GString::from("sector.attacked").to_variant(),
                                PackedStringArray::from(vec![GString::from(name.as_str())])
                                    .to_variant(),
                            ],
                        )
                        .try_to::<GString>()
                        .ok()
                })
                .unwrap_or_else(|| GString::from(format!("Sector {name} under attack!").as_str()));
            let _ = hud.call(
                "push_toast",
                &[text.to_variant(), GString::from("warning").to_variant()],
            );
        }
    }

    /// Runs one production turn; returns success. Runs on the live runtime
    /// campaign when one is installed (T1: one authoritative campaign).
    #[func]
    pub fn run_turn(&mut self) -> bool {
        let Some(registry) = self.registry.take() else {
            return false;
        };
        // `ctx` reads the live session (runtime copy while a sector runs).
        let ctx = if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            let ctx = taken.as_ref().map(|runtime| TurnContext {
                current_planet: runtime.session.sector.map(|(planet, _)| planet),
                current_sector: runtime.session.sector,
                playing_rules: Some((
                    runtime.session.rules.waves,
                    runtime.session.rules.attack_mode,
                )),
                current_wave: runtime.session.wave,
            });
            if let Some(runtime) = taken {
                host.bind_mut().put_campaign_runtime(runtime);
            }
            ctx
        } else {
            None
        }
        .unwrap_or_default();
        let Some(report) = self.with_campaign_mut(|campaign, settings| {
            let turn = campaign.universe.turn;
            campaign.run_turn(
                &registry,
                settings,
                &ctx,
                &mut JavaRandom::new(turn as u64 + 1),
            )
        }) else {
            self.registry = Some(registry);
            return false;
        };
        // Played-sector invasions hit the live session rules (T4: `Universe.java:550-555`).
        if report.events.iter().any(|event| {
            matches!(
                event,
                mind_core::game::universe::CampaignEvent::SectorInvasion { played: true, .. }
            )
        }) && let Some(mut host) = self.host()
        {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                mind_core::game::runtime::apply_played_invasions(&mut runtime, &report.events);
                self.session.rules = runtime.session.rules.clone();
                host.bind_mut().put_campaign_runtime(runtime);
            }
        }
        // T7: toasts for off-play sectors (`HudFragment.java:261-275`).
        self.toast_campaign_events(&report.events);
        self.registry = Some(registry);
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
                let (_, spawn_index) = run_wave_campaign(&mut runtime.session);
                host.bind_mut().put_campaign_runtime(runtime);
                host.bind_mut().request_wave_spawn(spawn_index);
            }
        }
        true
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

        // Real inventory: `ResearchDialog.rebuildItems` aggregates the stored
        // items of the tree root's planets; the sector being played reads its
        // live core module instead (`Sector.items` -> default-team items), so
        // it is not double-counted.
        let default_team = self.session.default_team();
        let root_ref = registry
            .tech()
            .node(node_ref)
            .and_then(|node| node.root_node)
            .unwrap_or(node_ref);
        let planets = root_planets(&registry, root_ref);
        // T1: reads (`research_inventory`/`ctx`) must see the live campaign.
        self.mirror_runtime_campaign();
        let (mut items, mut sector_entries, mut played_items) =
            self.research_inventory(&registry, &planets);

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
        let outcome = settings_hub::with_settings(|settings| {
            let mut store = SettingsUnlockStore::new(settings);
            tech_tree::spend(&mut registry, node_ref, &mut items, &mut store, &ctx, false)
        });
        let Ok(result) = outcome else {
            self.registry = Some(registry);
            return false;
        };
        let changed = result.complete || !result.spent.is_empty();

        // `ResearchDialog.complete` (`ResearchDialog.java:624`):
        // `Events.fire(new ResearchEvent(node.content))` once the node is
        // finished (host-owned fire site; PAR-0577 residual).
        if result.complete
            && let Some(payload) = event_content_of(content_ref)
        {
            self.events
                .dispatch_game(event::GameEvent::ResearchEvent(event::ResearchEvent {
                    content: payload,
                }));
        }

        // Deduct the spent stacks exactly like `ResearchDialog.rebuildItems`'s
        // `ItemSeq.add(item, -amount)`: percentage removal from the other
        // sectors in order, then the remainder from the active sector last
        // (its live core module while playing; `Sector.removeItem`).
        let campaign_sector_active = self.session.is_campaign() && self.session.sector.is_some();
        // T1: sector-record removals are applied to the live campaign after the
        // loop (one `with_campaign_mut`), with the Java `saveInfo` flush.
        let mut record_updates: Vec<(PlanetId, u16, String, i32)> = Vec::new();
        for spent in &result.spent {
            let Some(name) = registry.item(spent.item).map(|item| item.name.clone()) else {
                continue;
            };
            let others: Vec<i32> = sector_entries
                .iter()
                .map(|(_, entry)| {
                    entry
                        .iter()
                        .find(|(entry_name, _)| entry_name == &name)
                        .map(|(_, amount)| *amount)
                        .unwrap_or(0)
                })
                .collect();
            let current = campaign_sector_active.then(|| {
                played_items
                    .iter()
                    .find(|(entry_name, _)| entry_name == &name)
                    .map(|(_, amount)| *amount)
                    .unwrap_or(0)
            });
            let (removals, current_removal) =
                tech_tree::distribute_removal(spent.amount, current, &others);
            for (index, to_remove) in removals.iter().enumerate() {
                if *to_remove <= 0 {
                    continue;
                }
                let ((planet, sector), entry) = &mut sector_entries[index];
                let (planet, sector) = (*planet, *sector);
                if let Some((_, amount)) =
                    entry.iter_mut().find(|(entry_name, _)| entry_name == &name)
                {
                    *amount -= *to_remove;
                }
                let updated = entry
                    .iter()
                    .find(|(entry_name, _)| entry_name == &name)
                    .map(|(_, amount)| *amount)
                    .unwrap_or(0);
                record_updates.push((planet, sector, name.clone(), updated));
            }
            if current_removal > 0
                && let Some(mut host) = self.host()
            {
                host.bind_mut()
                    .remove_player_core_items(default_team, &name, current_removal);
                if let Some((_, amount)) = played_items
                    .iter_mut()
                    .find(|(entry_name, _)| entry_name == &name)
                {
                    *amount -= current_removal;
                }
            }
        }
        // Apply the record removals on the live campaign (`Sector.removeItems`
        // -> `addItems` -> `saveInfo()`; `Sector.java:207-231`).
        if !record_updates.is_empty() {
            self.with_campaign_mut(|campaign, settings| {
                for (planet, sector_id, name, updated) in &record_updates {
                    let planet_name = campaign
                        .planet(*planet)
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    if let Some(record) = campaign.sector_mut(*planet, *sector_id) {
                        record.info.info.items.insert(name.clone(), *updated);
                        let _ = record.save_info(settings, &planet_name);
                    }
                }
            });
        }
        // Newly unlocked content may auto-unlock dependent zero-cost nodes.
        let mut unlocked_content: Vec<ContentRef> = result.unlocked.clone();
        if !result.unlocked.is_empty()
            && let Some(campaign_ref) = self.campaign.as_ref()
        {
            let unlocked = settings_hub::with_settings(|settings| {
                let ctx = CampaignObjectiveContext::new(&registry, campaign_ref);
                let mut store = SettingsUnlockStore::new(settings);
                tech_tree::check_auto_unlocks(&mut registry, &mut store, &ctx)
            });
            unlocked_content.extend(unlocked.iter().copied());
            log::info!("research auto-unlocked {} content", unlocked.len());
        }
        self.registry = Some(registry);
        // Refresh the runtime copy's unlocked-content cache (the campaign
        // mutations above already went through `with_campaign_mut`).
        self.push_campaign_to_runtime();
        if changed {
            self.persist();
            self.invalidate_ui_block_catalog();
        }
        if let Some(registry) = self.registry.as_ref() {
            // Each unlock fires an `UnlockEvent` (`Control.java:160-165`):
            // toast for `showUnlock()` content, palette rebuild for blocks.
            for content in &unlocked_content {
                self.push_unlock_toast(registry, *content);
            }
            if unlocked_content
                .iter()
                .any(|content| content.type_ == ContentType::Block)
            {
                // No host bind is held here.
                let _ = self.base_mut().emit_signal("block_unlocked", &[]);
            }
        }
        changed
    }

    /// `ResearchDialog.rebuildItems` inventory for `planets` (Java
    /// `rootPlanets`): the stored items of every based, non-frozen sector plus
    /// the active sector's live core module. Returns the pool, the per-other-
    /// sector rows (planet order, then sector id) and the active sector's core
    /// rows.
    fn research_inventory(
        &self,
        registry: &ContentRegistry,
        planets: &[PlanetId],
    ) -> (ItemModule, SectorInventory, Vec<(String, i32)>) {
        let mut items = ItemModule::with_items(registry.items().len());
        let mut played_items: Vec<(String, i32)> = Vec::new();
        if self.session.is_campaign()
            && let Some(host) = self.host()
        {
            for (name, amount) in host.bind().player_core_items(self.session.default_team()) {
                if let Some(id) = registry.item_id(&name) {
                    items.add(id, amount.max(0), i32::MAX);
                }
                played_items.push((name, amount.max(0)));
            }
        }
        let played_sector = self.session.sector;
        let mut sector_entries: SectorInventory = Vec::new();
        if let Some(campaign) = self.campaign.as_ref() {
            for planet in campaign.planets.values() {
                if !planets.contains(&planet.id) {
                    continue;
                }
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
        (items, sector_entries, played_items)
    }

    /// `ResearchDialog` tree data (`treeLayout` positions + `checkNodes` state
    /// + the hover popup rows): the selected tree's nodes in depth-first order
    /// with Java y-up positions, lock/selectable state, requirement rows, the
    /// incomplete objectives and the global item pool.
    ///
    /// `root` selects the tree by name; empty picks the played sector's tree,
    /// else serpulo. Cheap: no world access (`campaign_views_json` is the
    /// heavy one).
    #[func]
    pub fn research_tree_json(&self, root: GString) -> GString {
        let Some(registry) = self.registry.as_ref() else {
            return GString::from("{}");
        };
        let Some(campaign) = self.campaign.as_ref() else {
            return GString::from("{}");
        };
        let tech = registry.tech();
        let requested = root.to_string();
        let tree = if requested.is_empty() {
            self.session
                .sector
                .map(|(planet, _)| planet)
                .or_else(|| registry.planet_id("serpulo"))
                .and_then(|id| registry.planet(id))
                .and_then(|def| def.tech_tree)
                .and_then(|tree_id| tech.tree(tree_id))
        } else {
            tech.trees.iter().find(|tree| tree.name == requested)
        };
        let Some(tree) = tree else {
            return GString::from("{}");
        };

        let ctx = CampaignObjectiveContext::new(registry, campaign);
        let unlocked_with_rules = |content: ContentRef| {
            tech_tree::content_unlocked_with_rules(registry, content, &self.session.rules)
        };

        // Depth-first pre-order from the tree root (`TechNode.each`); indices
        // in this array are the JSON `index`/`parent`/`children` values.
        let mut order: Vec<TechNodeRef> = Vec::new();
        tech.each(tree.root, &mut |node_ref| order.push(node_ref));
        if order.is_empty() {
            return GString::from("{}");
        }
        let mut indices: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        for (index, node_ref) in order.iter().enumerate() {
            indices.insert(node_ref.0, index);
        }

        // `checkNodes` (ResearchDialog.java:391-401): `visible` starts true for
        // every node; a locked parent hides its subtree, an unlocked parent
        // shows its direct children even when they are locked.
        let mut visible = vec![true; order.len()];
        let mut selectable = vec![false; order.len()];
        for (index, node_ref) in order.iter().enumerate() {
            let Some(node) = tech.node(*node_ref) else {
                continue;
            };
            let node_locked = node
                .content
                .is_none_or(|content| !unlocked_with_rules(content));
            let parent_visible = node
                .parent
                .and_then(|parent| indices.get(&parent.0).copied())
                .map(|parent_index| visible[parent_index])
                .unwrap_or(true);
            if !node_locked && (node.parent.is_none() || parent_visible) {
                visible[index] = true;
            }
            selectable[index] = node
                .content
                .is_some_and(|content| tech_tree::content_unlocked(registry, content))
                || tech_tree::objectives_complete(registry, &node.objectives, &ctx);
            for child in &node.children {
                if let Some(child_index) = indices.get(&child.0).copied() {
                    visible[child_index] = !node_locked && visible[index];
                }
            }
        }

        // `treeLayout`: positions come from the Java BranchTreeLayout walk.
        let children_lists: Vec<Vec<usize>> = order
            .iter()
            .map(|node_ref| {
                tech.node(*node_ref)
                    .map(|node| {
                        node.children
                            .iter()
                            .filter_map(|child| indices.get(&child.0).copied())
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect();
        let positions = mind_core::ui::tree_layout::research_tree_layout(&children_lists, 0, 60.0);

        // Bounds over visible nodes (ResearchDialog.java:360-371): node width
        // goes into x, +`nodeSize * 1.5` into y.
        const NODE_SIZE: f32 = 60.0;
        let (mut minx, mut miny, mut maxx, mut maxy) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for (index, &(x, y)) in positions.iter().enumerate() {
            if !visible.get(index).copied().unwrap_or(false) {
                continue;
            }
            minx = minx.min(x - NODE_SIZE / 2.0);
            maxx = maxx.max(x + NODE_SIZE / 2.0);
            miny = miny.min(y - NODE_SIZE / 2.0);
            maxy = maxy.max(y + NODE_SIZE / 2.0);
        }
        let bounds = [minx, miny + NODE_SIZE * 1.5, maxx - minx, maxy - miny];

        // The global item pool (`rebuildItems` + `ItemsDisplay`).
        let planets = root_planets(registry, tree.root);
        let (items, _, _) = self.research_inventory(registry, &planets);
        let mut items_json: Vec<serde_json::Value> = Vec::new();
        for index in 0..registry.items().len() {
            let id = ItemId::new(index as u16);
            let amount = items.get(id);
            if amount <= 0 {
                continue;
            }
            let item_name = registry
                .item(id)
                .map(|item| item.name.clone())
                .unwrap_or_default();
            items_json.push(serde_json::json!({
                "item": item_name,
                "amount": amount,
                "text": mind_core::ui::text::format_amount_default(amount as i64),
                "name": self.localized_content_name("item", &item_name),
            }));
        }

        // Root selector rows (`TechTree.roots` minus locked non-preferred ones).
        let mut roots_json: Vec<serde_json::Value> = Vec::new();
        for root_ref in &tech.roots {
            let Some(node) = tech.node(*root_ref) else {
                continue;
            };
            let selected = *root_ref == tree.root;
            let unlocked = node
                .content
                .is_some_and(|content| tech_tree::content_unlocked(registry, content));
            if node.requires_unlock && !unlocked && !selected {
                continue;
            }
            let name = node.name.clone().unwrap_or_default();
            roots_json.push(serde_json::json!({
                "name": name,
                "title": self.tree_title(&name),
                "type": node
                    .content
                    .map(|content| content.type_.name())
                    .unwrap_or_default(),
                "icon": node
                    .content
                    .map(|content| content_icon_name(content.type_.name(), &node.content_name))
                    .unwrap_or_default(),
                "selected": selected,
            }));
        }
        let root_node = tech.node(tree.root);
        let root_name = root_node
            .and_then(|node| node.name.clone())
            .unwrap_or_else(|| tree.name.clone());
        let root_type = root_node
            .and_then(|node| node.content)
            .map(|content| content.type_.name())
            .unwrap_or("");
        let root_icon = root_node
            .map(|node| {
                node.content
                    .map(|content| content_icon_name(content.type_.name(), &node.content_name))
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        // Node rows (`View.rebuildAll` buttons + the hover popup table).
        let mut nodes_json: Vec<serde_json::Value> = Vec::with_capacity(order.len());
        for (index, node_ref) in order.iter().enumerate() {
            let Some(node) = tech.node(*node_ref) else {
                continue;
            };
            let content = node.content;
            let type_name = content.map(|content| content.type_.name()).unwrap_or("");
            let icon = content
                .map(|_| content_icon_name(type_name, &node.content_name))
                .unwrap_or_default();
            let name = content
                .map(|_| self.localized_content_name(type_name, &node.content_name))
                .unwrap_or_default();
            let description = content
                .map(|_| self.localized_content_description(type_name, &node.content_name))
                .unwrap_or_default();
            let inline_description = content
                .and_then(|content| tech_tree::unlock_fields(registry, content))
                .is_some_and(|fields| fields.inline_description);

            // Progress line (`ResearchDialog.java:685-705`).
            let requirements = tech_tree::effective_requirements(registry, *node_ref);
            let (mut sum, mut used) = (0.0f32, 0.0f32);
            let mut has_progress = false;
            for (requirement_index, requirement) in requirements.iter().enumerate() {
                let finished = node
                    .finished_requirements
                    .get(requirement_index)
                    .map(|stack| stack.amount)
                    .unwrap_or(0);
                if finished > 0 {
                    has_progress = true;
                }
                let cost = registry
                    .item(requirement.item)
                    .map(|item| item.cost)
                    .unwrap_or(0.0);
                sum += cost * requirement.amount as f32;
                used += cost * finished as f32;
            }
            let progress = if has_progress {
                ((used / sum * 100.0) as i32).min(99)
            } else {
                -1
            };

            // Unfinished requirement rows (`ResearchDialog.java:707-736`).
            let mut reqs_json: Vec<serde_json::Value> = Vec::new();
            for (requirement_index, requirement) in requirements.iter().enumerate() {
                let finished = node
                    .finished_requirements
                    .get(requirement_index)
                    .map(|stack| stack.amount)
                    .unwrap_or(0);
                if requirement.amount <= finished {
                    continue;
                }
                let remaining = requirement.amount - finished;
                let pool = items.get(requirement.item);
                let item_name = registry
                    .item(requirement.item)
                    .map(|item| item.name.clone())
                    .unwrap_or_default();
                reqs_json.push(serde_json::json!({
                    "item": item_name,
                    "name": self.localized_content_name("item", &item_name),
                    "have": mind_core::ui::text::format_amount_default(pool.min(remaining) as i64),
                    "need": mind_core::ui::text::format_amount_default(remaining as i64),
                    "has": pool > 0,
                }));
            }

            // Incomplete objectives (`ResearchDialog.java:737-749`).
            let mut objectives_json: Vec<String> = Vec::new();
            for spec in &node.objectives {
                let (complete, text) = self.objective_display(registry, &ctx, *spec);
                if !complete {
                    objectives_json.push(text);
                }
            }

            nodes_json.push(serde_json::json!({
                "index": index,
                "parent": node
                    .parent
                    .and_then(|parent| indices.get(&parent.0).copied())
                    .map(|parent_index| parent_index as i64)
                    .unwrap_or(-1),
                "children": node
                    .children
                    .iter()
                    .filter_map(|child| indices.get(&child.0).copied())
                    .collect::<Vec<usize>>(),
                "content": node.content_name,
                "type": type_name,
                "icon": icon,
                "name": name,
                "x": positions[index].0,
                "y": positions[index].1,
                "visible": visible[index],
                "locked": content.is_none_or(|content| !unlocked_with_rules(content)),
                "selectable": selectable[index],
                "can_spend": tech_tree::can_spend(registry, *node_ref, &items, &ctx),
                "description": description,
                "inline_description": inline_description,
                "progress": progress,
                "reqs": reqs_json,
                "objectives": objectives_json,
            }));
        }

        let view = serde_json::json!({
            "root": tree.name,
            "root_title": self.tree_title(&root_name),
            "root_type": root_type,
            "root_icon": root_icon,
            "roots": roots_json,
            "items": items_json,
            "bounds": bounds,
            "nodes": nodes_json,
        });
        GString::from(view.to_string().as_str())
    }

    /// `bundle("techtree." + name, name)` for a tree/root name.
    fn tree_title(&self, name: &str) -> String {
        let key = format!("techtree.{name}");
        let value = self.bundle.get(&key);
        if value == key {
            name.to_owned()
        } else {
            value.to_owned()
        }
    }

    /// `Objectives.Research.display()` half: `content.techNode == null ||
    /// parent == null || parent.content.unlockedHost()`. The parent comes from
    /// `UnlockFields.tech_node` (the last-created node), matching Java's
    /// `UnlockableContent.techNode` (PAR-0500).
    fn research_parent_unlocked(&self, registry: &ContentRegistry, content: ContentRef) -> bool {
        tech_tree::research_parent_unlocked(registry, content)
    }

    /// `Objective.display()` and `complete()` for the hover popup
    /// (Objectives.java:26-131).
    fn objective_display(
        &self,
        registry: &ContentRegistry,
        ctx: &dyn ObjectiveContext,
        spec: ObjectiveSpec,
    ) -> (bool, String) {
        let complete = Objective::from_spec(spec).complete(ctx);
        let (key, argument) = match spec {
            ObjectiveSpec::SectorComplete(sector) => (
                "requirement.capture",
                self.sector_localized_name(registry, sector),
            ),
            ObjectiveSpec::Research(content) => {
                let value = if self.research_parent_unlocked(registry, content) {
                    self.emoji_content_name(registry, content)
                } else {
                    String::from("???")
                };
                ("requirement.research", value)
            }
            ObjectiveSpec::Produce(content) => {
                let value = if tech_tree::content_unlocked(registry, content) {
                    self.emoji_content_name(registry, content)
                } else {
                    String::from("???")
                };
                ("requirement.produce", value)
            }
            ObjectiveSpec::OnSector(sector) => (
                "requirement.onsector",
                self.sector_localized_name(registry, sector),
            ),
            ObjectiveSpec::OnPlanet(planet) => (
                "requirement.onplanet",
                self.planet_localized_name(registry, planet),
            ),
        };
        (complete, self.bundle.format(key, &[argument.as_str()]))
    }

    /// `UnlockableContent.unlockedHost()` by content name (G05 T4b hint gates);
    /// `false` for unknown names or before content boot.
    #[func]
    pub fn content_unlocked(&self, name: GString) -> bool {
        let Some(registry) = self.registry.as_ref() else {
            return false;
        };
        let name = name.to_string();
        let Some(content) = registry.by_name(&name) else {
            return false;
        };
        tech_tree::content_unlocked(registry, content)
    }

    /// Content names unlocked since the last call (`Control.java:160-174`:
    /// `hudfrag.showUnlock` popup; H05 consumes). Drained here.
    #[func]
    pub fn take_unlocked_content(&mut self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for name in self.unlocked_since_last_take.drain(..) {
            out.push(&GString::from(name.as_str()));
        }
        out
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
}

/// `SaveVersion.readRules` + `World.setSectorRules` (T5): the loaded
/// preset/generated map (or sector save) rules replace the session rules
/// wholesale, then the planet `ruleSetter`, the preset overlays and the
/// campaign load overrides are re-applied.
///
/// `has_preset` restores the preset's capture win wave on a fresh launch
/// (`SectorPreset.rules` = `winWave = captureWave`); `resumed` keeps the wave
/// record `SectorInfo.write` copied in `play_sector` and re-applies the
/// `SaveLoadEvent` overrides (`Logic.java:73-91`).
fn adopt_map_rules(
    session: &mut PlaySession,
    map: &Rules,
    planet: &mind_core::game::planet::Planet,
    registry: &ContentRegistry,
    has_preset: bool,
    preset_no_lighting: bool,
    resumed: bool,
) {
    let kept = session.rules.clone();
    let mut rules = map.clone();
    // Empty spawns fall back to the generated/default waves.
    if rules.spawns.is_empty() {
        rules.spawns = kept.spawns.clone();
    }
    rules.sector = kept.sector.clone();
    rules.loadout = kept.loadout.clone();
    planet.apply_rules(registry, &mut rules, false, true);
    if has_preset && !resumed {
        rules.win_wave = kept.win_wave;
        rules.waves = kept.waves;
        rules.attack_mode = kept.attack_mode;
    }
    if resumed {
        rules.waves = kept.waves;
        rules.wave_spacing = kept.wave_spacing;
        rules.win_wave = kept.win_wave;
        rules.attack_mode = kept.attack_mode;
    }
    // Preset `noLighting` (PAR-0059) survives the wholesale copy.
    if !kept.lighting && preset_no_lighting {
        rules.lighting = false;
    }
    apply_world_load_campaign_rules(&mut rules, planet);
    if resumed {
        apply_save_load_campaign_rules(&mut rules);
    }
    session.rules = rules;
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

/// Java `rootPlanets` for a tree root (`ResearchDialog.rebuildItems`): planets
/// whose `techTree == lastNode` or `lastNode.planet == planet`; vanilla trees
/// carry the planet on the root, fallback serpulo.
fn root_planets(registry: &ContentRegistry, root_ref: TechNodeRef) -> Vec<PlanetId> {
    let mut planets: Vec<PlanetId> = registry
        .tech()
        .node(root_ref)
        .and_then(|node| node.planet)
        .into_iter()
        .collect();
    if planets.is_empty()
        && let Some(serpulo) = registry.planet_id("serpulo")
    {
        planets.push(serpulo);
    }
    planets
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

/// Parses a launch-loadout payload: the dialog envelope
/// `{"core": ..., "stacks": [...]}` (`PlanetDialog.playSelected`) or the legacy
/// bare `[{item, amount}]` array.
fn parse_loadout_stacks(
    json: &str,
) -> Option<Vec<mind_core::io::json::content_serde::JsonItemStack>> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let stacks = value.get("stacks").cloned().unwrap_or(value);
    serde_json::from_value(stacks).ok()
}

/// Parses the launcher launch payload (`PlanetDialog.playSelected`):
/// `(schematic index, origin sector id, stacks JSON)`.
///
/// The schematic index indexes `Schematics.all` (`None` for the items-only
/// fallback); the `stacks` array JSON is forwarded to
/// `MindSimHost.place_launch_loadout`.
fn parse_launch_payload(json: &str) -> (Option<i32>, Option<u16>, String) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return (None, None, String::from("[]"));
    };
    let schematic = value
        .get("schematic")
        .and_then(serde_json::Value::as_i64)
        .map(|index| index as i32);
    // `Control.playNewSector` (`Control.java:573-578`): `info.origin =
    // info.destination = origin`; absent -> no origin (a free launch).
    let origin = value
        .get("origin")
        .and_then(serde_json::Value::as_i64)
        .and_then(|origin| u16::try_from(origin).ok());
    let stacks = value.get("stacks").cloned().unwrap_or(value);
    (schematic, origin, stacks.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::content::MemoryUnlockStore;
    use mind_core::content::create_base_content;

    /// PAR-0037/PAR-0030 + T5: a preset map's rules replace `state.rules`
    /// wholesale in Java (`FileMapGenerator.generate` -> `SaveIO.load(map.file)`
    /// -> `SaveVersion.readRules`), so the adopted rules must carry the
    /// objective graph, its flags and the `waveTimer` gate into the session or
    /// the tutorial launches as a wave sector.
    #[test]
    fn preset_map_rules_carry_objectives_and_wave_timer() {
        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        let objectives = match mind_core::io::json::objectives::MapObjectives::from_json(
            r#"[{"class":"Item","amount":15,"markers":[{"class":"Text","text":"@gz.mine"}]}]"#,
        ) {
            Ok(objectives) => objectives,
            Err(error) => panic!("fixture objectives must parse: {error}"),
        };
        let map = Rules {
            wave_timer: false,
            wave_sending: false,
            objectives,
            objective_flags: std::iter::once("spawnWave".to_owned()).collect(),
            ..Rules::default()
        };
        let planet_def = match registry.planet_by_name("serpulo") {
            Some(planet) => planet,
            None => panic!("serpulo planet missing"),
        };
        let planet = mind_core::game::planet::Planet::from_def(&registry, planet_def, 0.0);

        let mut session = PlaySession::new(Rules::default());
        adopt_map_rules(&mut session, &map, &planet, &registry, true, false, false);

        assert!(!session.rules.wave_timer, "map waveTimer:false must win");
        assert!(
            !session.rules.wave_sending,
            "map waveSending:false must win (canSkipWave gate - PAR-0030)"
        );
        assert_eq!(session.rules.objectives.len(), 1);
        assert!(session.rules.objective_flags.contains("spawnWave"));
        // The serpulo ruleSetter re-applied after the wholesale copy.
        assert_eq!(session.rules.wave_team, 2);
        assert!(session.rules.core_destroy_clear);
    }
}

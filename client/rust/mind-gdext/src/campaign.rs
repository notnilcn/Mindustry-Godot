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
use cache::dir_mtime_key;
use events::dispatch_event_rows;
use events::event_content_of;
use events::install_event_listeners;
use service::load_service;
use service::save_service;
use text::content_icon_name;
use text::resolve_preset_description;

mod accelerator;
mod cache;
mod database;
mod events;
mod hud;
mod objectives;
mod planet_view;
mod rules;
mod runtime;
mod schematics;
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

    /// `Saves.SaveSlot.savePreview` on the live world (`Saves.java:343-358`):
    /// renders `<user>/previews/save_slot_<stem>.png` from the current grid
    /// before the queued save lands at the IO boundary. Java runs it for every
    /// slot save (manual, autosave, sector, capture, game over).
    fn write_live_slot_preview(&self, file: &std::path::Path) {
        let Some(stem) = file.file_stem().and_then(|stem| stem.to_str()) else {
            return;
        };
        if let Some(mut host) = self.host() {
            host.bind_mut().write_save_preview(GString::from(stem));
        }
    }

    /// `SaveSlot.savePreview` for a slot with no live world: renders the preview
    /// PNG from the slot file's own tiles. Used on import and when listing a
    /// slot that has no preview (`Saves.java:343-358`).
    fn render_slot_preview_from_file(&mut self, file: &std::path::Path) -> bool {
        let paths = self.save_paths();
        let Some(mut registry) = self.registry.take() else {
            log::warn!("save preview: content registry unavailable");
            return false;
        };
        let mut palette = mind_core::io::map::BlockPalette::of(&registry);
        palette.apply_block_colors(&crate::render::minimap::load_block_colors());
        let mut slot = mind_core::io::save::slot::SaveSlot::new(file.to_path_buf());
        let result = slot.write_preview_from_file(&NativeFs, &paths, &mut registry, palette);
        self.registry = Some(registry);
        match result {
            Ok(_) => true,
            Err(error) => {
                log::warn!("save preview `{}` render failed: {error}", file.display());
                false
            }
        }
    }

    /// Clears the filesystem-derived memos (finding #9). A content (re)boot can
    /// change block ids/names the `Schematics` decode resolves, and
    /// `bootstrap_with` re-points `self.paths`; both invalidate every entry.
    fn invalidate_caches(&self) {
        self.schematics_cache.borrow_mut().clear();
        self.save_slots_cache.borrow_mut().clear();
    }

    /// `item.isOnPlanet(planet)` (`UnlockableContent.shownPlanets`).
    fn item_on_planet(registry: &ContentRegistry, name: &str, planet: PlanetId) -> bool {
        registry
            .item_by_name(name)
            .is_some_and(|item| item.unlock.shown_planets.contains(&planet))
    }

    /// `destination.preset.generator.map.rules().loadout` for the disallowed
    /// branch (`LaunchLoadoutDialog.java:66-76`), filtered to items on the
    /// launcher's planet; the map header is read through `SaveIo.getMeta`.
    fn preset_launch_loadout(
        registry: &ContentRegistry,
        planet: PlanetId,
        preset_id: SectorId,
    ) -> indexmap::IndexMap<String, i32> {
        let mut out = indexmap::IndexMap::new();
        let Some(preset) = registry.sector(preset_id) else {
            return out;
        };
        let owner = registry
            .planet(preset.planet)
            .map(|planet| planet.name.clone())
            .unwrap_or_default();
        let map = preset
            .file_name
            .clone()
            .unwrap_or_else(|| preset.name.clone());
        let path = std::path::PathBuf::from(crate::assets::loader::resolve_assets_dir())
            .join("maps")
            .join(owner)
            .join(format!("{map}.msav"));
        let Ok(meta) = SaveIo::get_meta(&NativeFs, &path) else {
            return out;
        };
        let Ok(rules) = serde_json::from_str::<Rules>(&quote_bare_keys(&meta.rules_json)) else {
            return out;
        };
        for stack in &rules.loadout {
            let Some(name) = stack.item.as_deref() else {
                continue;
            };
            if Self::item_on_planet(registry, name, planet) {
                *out.entry(name.to_owned()).or_insert(0) += stack.amount;
            }
        }
        out
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

    /// The live record of one sector as JSON (T1 seam): the `SectorInfo` serde
    /// (camelCase) plus `"beingPlayed"` and `"hasSave"`; `"{}"` when unknown.
    /// Reads the runtime copy while a sector is installed, else the facade.
    #[func]
    pub fn sector_record_json(&self, planet: GString, sector: i64) -> GString {
        if !(0..=i64::from(u16::MAX)).contains(&sector) {
            return GString::from("{}");
        }
        let planet_name = planet.to_string();
        let sector_id = sector as u16;
        let mut value = self
            .campaign
            .as_ref()
            .and_then(|campaign| Self::sector_record_value(campaign, &planet_name, sector_id));
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(runtime) = &taken {
                value = Self::sector_record_value(&runtime.campaign, &planet_name, sector_id);
            }
            if let Some(runtime) = taken {
                host.bind_mut().put_campaign_runtime(runtime);
            }
        }
        match value {
            Some(value) => GString::from(value.to_string().as_str()),
            None => GString::from("{}"),
        }
    }

    fn sector_record_value(
        campaign: &Campaign,
        planet_name: &str,
        sector_id: u16,
    ) -> Option<serde_json::Value> {
        let planet_id = campaign.planet_id_by_name(planet_name)?;
        let sector = campaign.sector(planet_id, sector_id)?;
        let mut value = serde_json::to_value(&sector.info.info).ok()?;
        if let serde_json::Value::Object(ref mut map) = value {
            map.insert(
                "beingPlayed".to_owned(),
                serde_json::Value::Bool(sector.being_played),
            );
            map.insert(
                "hasSave".to_owned(),
                serde_json::Value::Bool(sector.save.is_some()),
            );
        }
        Some(value)
    }

    /// `LaunchPad.buildConfiguration` -> `ui.planet.showSelect` result
    /// (`LaunchPad.java:185-203`): sets the played sector's export destination
    /// to `planet:sector`; `-1` clears it. H05 wires the pad config button.
    #[func]
    pub fn set_launch_destination(&mut self, planet: GString, sector: i64) -> bool {
        let planet_name = planet.to_string();
        let destination = if sector < 0 {
            None
        } else if sector <= i64::from(u16::MAX) {
            Some(mind_core::io::json::content_serde::SectorKey::format(
                &planet_name,
                sector as u16,
            ))
        } else {
            return false;
        };
        let Some((planet_id, sector_id)) = self.session.sector else {
            return false;
        };
        let Some(registry) = self.registry.take() else {
            return false;
        };
        let updated = self.with_campaign_mut(|campaign, settings| {
            let previous = campaign
                .sector(planet_id, sector_id)
                .and_then(|sector| sector.info.info.destination.clone());
            let Some(record) = campaign.sector_mut(planet_id, sector_id) else {
                return false;
            };
            record.info.info.destination = destination.clone();
            let _ = record.save_info(settings, &planet_name);
            // `if(prev != null) prev.info.refreshImportRates(planet)`.
            if let Some(previous) = previous
                && let Some(prev_planet) = previous.planet()
                && let Some(prev_sector) = previous.sector()
                && let Some(prev_planet_id) = campaign.planet_id_by_name(prev_planet)
            {
                campaign.refresh_sector_import_rates(&registry, prev_planet_id, prev_sector);
            }
            true
        });
        self.registry = Some(registry);
        if updated != Some(true) {
            return false;
        }
        self.persist();
        true
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

    /// M5 campaign dialog read models as live JSON (`CampaignViews`).
    ///
    /// The planet-14 dialogs prefer this endpoint over
    /// `MindUi.campaign_views()`'s deterministic fixture: sector
    /// owned/captured/locked flags, stats and planet rows are projected from
    /// the running `Campaign` every call, so a capture (`capture_sector`) is
    /// visible on the next refresh. `planet` is a content name; empty selects
    /// the planet being played, else the default starting planet.
    #[func]
    pub fn campaign_views_json(&self, planet: GString) -> GString {
        let Some(campaign) = self.campaign.as_ref() else {
            return GString::from("{}");
        };
        let Some(registry) = self.registry.as_ref() else {
            return GString::from("{}");
        };
        let name = planet.to_string();
        let active = if name.is_empty() {
            self.session
                .sector
                .map(|(planet, _)| planet)
                .or_else(|| campaign.planet_id_by_name("serpulo"))
                .or_else(|| {
                    campaign
                        .order
                        .first()
                        .and_then(|id| campaign.planets.get(id))
                        .map(|planet| planet.id)
                })
        } else {
            campaign.planet_id_by_name(&name)
        };
        let Some(active) = active else {
            return GString::from("{}");
        };
        let schematics = self.load_schematics(registry);
        // The `findLauncher` actual launch sector: the played sector, cleared
        // after game over (`PlanetDialog`: `gameOver ? null : state.getSector()`).
        let played = if self.session.game_over {
            None
        } else {
            self.session.sector
        };
        // `Sector.isAttacked()`/`isCaptured()` read the live rules while the
        // sector is being played (`Sector.java:129-131,181-184`), so the panel
        // shows `Under attack!` and the stats dialog its `Threat:` row
        // (PAR-0503). `gameOver` clears the "being played" state in Java too.
        let playing_rules = if self.session.game_over {
            None
        } else {
            self.session
                .sector
                .map(|_| (self.session.rules.waves, self.session.rules.attack_mode))
        };
        // `loadout_of` reads the preset map header for the disallowed-loadout
        // branch, filtered to the destination planet's items.
        let loadout_of = |sector_id: u16| -> Vec<(String, i32)> {
            let Some(sector) = campaign.sector(active, sector_id) else {
                return Vec::new();
            };
            let Some(preset_id) = sector.preset else {
                return Vec::new();
            };
            Self::preset_launch_loadout(registry, active, preset_id)
                .into_iter()
                .collect()
        };
        let mut views = mind_core::ui::campaign::CampaignViews::from_campaign(
            campaign,
            registry,
            &schematics,
            active,
            played,
            // No cheap core-size seam for the being-played `allowNumberedLaunch`
            // clause (G05 T3 deviation; `0` = unknown).
            0,
            playing_rules,
            &loadout_of,
        );
        // T10: `SaveSlot.getPlayTime` (`Saves.java:385-386`) — the stats dialog
        // reads `meta.timePlayed` from the sector's save slot (one scan; the
        // live slot's in-memory accumulation is not added, deviation).
        let stems: std::collections::HashMap<u16, String> = campaign
            .planet(active)
            .map(|planet| {
                planet
                    .sectors
                    .iter()
                    .filter_map(|sector| sector.save.clone().map(|save| (sector.id, save)))
                    .collect()
            })
            .unwrap_or_default();
        if !stems.is_empty() {
            // Memoized on the saves-directory mtime (finding #9): this listing
            // reads every slot header on each dialog rebuild otherwise.
            let saved = self.cached_save_slots();
            for view in &mut views.sectors {
                if let Some(stem) = stems.get(&view.id)
                    && let Some(slot) = saved.iter().find(|slot| slot.index() == *stem)
                    && let Some(meta) = slot.meta.as_ref()
                {
                    view.stats.playtime_ms = meta.time_played;
                }
            }
        }
        // The live registry boots with the empty `MemoryBundle`, so the
        // builder's `SectorPreset.isHidden()` proxy (`unlock.description`) is
        // always true here. Java reads the description from `Core.bundle`
        // (`UnlockableContent.java:91`); resolve the stats description key the
        // same way `localized_content_description` does so a bundle-less
        // preset keeps no row.
        for view in &mut views.sectors {
            if view.stats.preset_description_key.is_some() {
                continue;
            }
            let Some(preset) = view.preset.as_deref() else {
                continue;
            };
            let key = format!("sector.{preset}.description");
            if self.bundle.get(&key) != key {
                view.stats.preset_description_key = Some(key);
            }
        }
        // The map-registry dialogs read `maps` from the same endpoint; keep the
        // built-in rows the fixture path feeds them.
        views.maps = mind_core::ui::campaign::default_map_entries();
        GString::from(
            serde_json::to_string(&views)
                .unwrap_or_else(|_| String::from("{}"))
                .as_str(),
        )
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
        // `GameState.isEditor` (`rules.editor`): the pause dialog's desktop
        // `!state.isEditor()` gate for the `@savegame`/`@loadgame` entries
        // (`PausedDialog.java:89-93`).
        dict.set(
            &GString::from("editor"),
            &self.session.rules.editor.to_variant(),
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
        // I08 T4: the live custom map's name (the host match id for
        // non-campaign worlds); empty for campaign sectors and the menu.
        dict.set(
            &GString::from("mapName"),
            &self
                .current_map_name
                .clone()
                .unwrap_or_default()
                .to_variant(),
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
            // `state.getSector().preset` name (hint predicates: `isTutorial`,
            // `presetLaunch`), empty for a generated sector.
            let preset_name = record
                .and_then(|sector| sector.preset)
                .and_then(|preset| {
                    self.registry
                        .as_ref()
                        .and_then(|registry| registry.sector(preset))
                        .map(|def| def.name.clone())
                })
                .unwrap_or_default();
            dict.set(&key("preset"), &preset_name.as_str().to_variant());
            // The landing announce gate (CoreBlock.java:414): campaign +
            // `Vars.showSectorLandInfo` + `preset.showSectorLandInfo`.
            let preset_def = record.and_then(|sector| sector.preset).and_then(|preset| {
                self.registry
                    .as_ref()
                    .and_then(|registry| registry.sector(preset))
            });
            dict.set(
                &key("showLandInfo"),
                &mind_core::content::registries::sectors::shows_sector_land_info(
                    self.session.is_campaign(),
                    preset_def,
                )
                .to_variant(),
            );
            dict.set(
                &key("captured"),
                &record
                    .map(|sector| sector.is_captured(None))
                    .unwrap_or(false)
                    .to_variant(),
            );
            // `PausedDialog.rebuild` `showObjective`: the preset description
            // feeds the `@objective` full-text dialog (`UnlockableContent`
            // resolves it from `Core.bundle`).
            let preset_description = resolve_preset_description(
                self.registry.as_ref(),
                &self.bundle,
                record.and_then(|sector| sector.preset),
            );
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
        let total_playtime = self.saves.total_playtime;
        host.bind_mut().set_save_playtime(total_playtime);
        // `Saves.addSave` (`Saves.java:265-271`) -> `SaveSlot.save`
        // (`:343-358`): the slot's preview PNG is written from the live grid
        // before the queued save lands at the IO boundary, the slot is
        // registered and it becomes current while in game (`slot.save()`'s
        // `if(state.isGame()) current = this`).
        host.bind_mut()
            .write_save_preview(GString::from(slot.as_str()));
        let file = self
            .save_paths()
            .saves()
            .join(format!("{slot}.{SAVE_EXTENSION}"));
        if self.session.phase.is_game() {
            settings_hub::with_settings(|store| {
                self.saves.set_current_by_file(store, &file);
            });
        }
        host.bind_mut().request_save(
            GString::from(format!("user://saves/{slot}.msav").as_str()),
            false,
        );
        // `control.saves.addSave(text)` writes at once (`SaveDialog.java:26-31`
        // inside `ui.loadAnd`), and the pause menu that offers it has the sim
        // paused: with the pump halted no IO boundary would ever run the
        // queued save (PAR-0438). Flush it now, like the exit save.
        if host.bind().is_paused() {
            host.bind_mut().flush_io();
        }
        true
    }

    /// `Saves.SaveSlot.previewTexture()`: decodes
    /// `<user>/previews/save_slot_<name>.png` (`Saves.java:359-370`); null when
    /// the file is missing (the dialog keeps its placeholder).
    #[func]
    pub fn save_preview_texture(&mut self, name: GString) -> Option<Gd<Texture2D>> {
        use mind_core::io::map::decode_png;

        let paths = Paths::new(crate::assets::loader::user_data_root());
        let file = paths.preview_for_save(name.to_string().as_str());
        let bytes = NativeFs.read(&file).ok()?;
        let image = decode_png(&bytes).ok()?;
        crate::editor::preview::bind_texture(&image)
    }

    /// Loads a named `user://saves/<name>.msav` slot at the next IO tick
    /// boundary. Returns whether the request was queued.
    #[func]
    pub fn load_slot(&mut self, name: GString) -> bool {
        // `logic.reset()` (`state = new GameState()`): not a playtest.
        self.playtesting_map = None;
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
        // `SaveSlot.load` (`Saves.java:314-328`): apply the saved meta to the
        // live session (rules/wave/timer/stats) and resume its playtime before
        // the world swap lands at the IO boundary.
        let native = ProjectSettings::singleton().globalize_path(&path);
        if let Ok(bytes) = NativeFs.read(std::path::Path::new(native.to_string().as_str()))
            && let Ok(meta) = SaveIo::get_meta_bytes(&bytes)
        {
            apply_loaded_meta(&mut self.session, &meta);
            self.saves.total_playtime = meta.time_played;
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                apply_loaded_meta(&mut runtime.session, &meta);
                host.bind_mut().put_campaign_runtime(runtime);
            }
        } else {
            log::warn!("load_slot: `{path}` meta unavailable; session rules not applied");
        }
        // `SaveSlot.load` (`Saves.java:319-324`): `current = this`. Without it
        // the loaded save is never the current slot, so the autosave cadence,
        // the playtime accrual and the exit save all stay inert and the pause
        // menu keeps `@quit` (CG-SAVE-02).
        let file = self.paths.saves().join(format!("{slot}.{SAVE_EXTENSION}"));
        let time_played = self.saves.total_playtime;
        settings_hub::with_settings(|settings| {
            adopt_loaded_slot(&mut self.saves, settings, &file, time_played);
        });
        host.bind_mut().request_load(GString::from(path.as_str()));
        true
    }

    /// `Saves.getTotalPlaytime` (session playtime in milliseconds).
    #[func]
    pub fn get_total_playtime(&self) -> i64 {
        self.saves.get_total_playtime()
    }

    /// Whether the current save slot is an autosave
    /// (`control.saves.getCurrent().isAutosave()`, used by the pause menu's
    /// `@save.quit` label, `PausedDialog.java:116`).
    #[func]
    pub fn current_save_is_autosave(&self) -> bool {
        self.saves
            .get_current()
            .is_some_and(|slot| settings_hub::with_settings(|store| slot.is_autosave(store)))
    }

    /// `Control.playSector`'s pre-launch save (`Control.java:437-441`): when a
    /// match is live and a slot is current, the outgoing sector is written
    /// before the launch replaces the world, so the player's construction since
    /// the last autosave survives the switch (CG-SECTOR-02). The destination
    /// launch re-points `Saves.current` afterwards (`prepare_sector_slot` on a
    /// fresh launch, `set_current_by_file` on a resume), so Java's
    /// `resetSave()` has no observable equivalent here.
    ///
    /// MUST run while the host still holds the OUTGOING world: the planet
    /// dialog pre-loads the destination (`MindSimHost.load_sector`) before
    /// `start_sector`, so the UI calls this first
    /// ([`Self::save_outgoing_sector`], `PlanetDialog.playSelected`'s
    /// "save before launch"); the accelerator path calls it from
    /// `pump_accelerator_launch` before its own load. Returns whether a save
    /// was queued.
    fn save_outgoing_slot(&mut self) -> bool {
        if !self.session.phase.is_game() {
            return false;
        }
        let Some(file) = self.saves.get_current().map(|slot| slot.file.clone()) else {
            return false;
        };
        // `SaveVersion.writeMeta` (`SaveVersion.java:117-118`): prepare/save the
        // sector record as part of the save, like every other save path.
        self.persist_sector_record();
        // `GameService.storeStats` on a sector switch: the live runtime (and
        // its tracker) is about to be replaced by the destination world, so
        // mirror the tracker now and write it through the settings store.
        self.sync_service();
        settings_hub::with_settings(|settings| {
            let mut store = SettingsUnlockStore::new(settings);
            save_service(&mut self.service, &mut store);
        });
        self.write_live_slot_preview(&file);
        let Some(mut host) = self.host() else {
            return false;
        };
        host.bind_mut()
            .request_save(GString::from(file.to_string_lossy().as_ref()), false);
        host.bind_mut().flush_io();
        log::info!("saved outgoing sector before launch (`{}`)", file.display());
        true
    }

    /// Rust-only seam for the planet dialog's "save before launch"
    /// (`PlanetDialog.playSelected`, `PlanetDialog.java:1380-1386`): writes the
    /// live sector before `MindSimHost.load_sector` replaces the world
    /// (CG-SECTOR-02). Returns whether a save was queued.
    #[func]
    pub fn save_outgoing_sector(&mut self) -> bool {
        self.save_outgoing_slot()
    }

    /// `PausedDialog.runExitSave` / `Control.dispose`: writes the current
    /// autosave slot through the IO seam, then resets the match to the menu
    /// (`logic.reset`'s saves/session half). Returns whether a write was queued.
    #[func]
    pub fn run_exit_save(&mut self) -> bool {
        let current = self.saves.get_current().cloned();
        let should_save = current
            .as_ref()
            .is_some_and(|slot| settings_hub::with_settings(|store| slot.is_autosave(store)))
            && !self.session.game_over
            && self.session.phase != State::Menu;
        let mut queued = false;
        if should_save
            && let (Some(file), Some(mut host)) = (current.map(|slot| slot.file), self.host())
        {
            // `Saves.SaveSlot.save` -> `savePreview()` (`@save.quit` path).
            self.write_live_slot_preview(&file);
            host.bind_mut()
                .request_save(GString::from(file.to_string_lossy().as_ref()), false);
            host.bind_mut().flush_io();
            queued = true;
            log::info!("saved on exit (`{}`)", file.display());
        }
        self.reset_to_menu();
        queued
    }

    /// `PausedDialog.checkPlaytest` (`PausedDialog.java:174-183`), reached from
    /// `runExitSave` (pause-menu quit, `:189-197`) and the game-over `@menu`
    /// button (`GameOverDialog.java:134-139`): when the match is a playtest
    /// (`state.playtestingMap != null`), reset it WITHOUT the exit save and
    /// reopen the editor on the playtested map
    /// (`ui.editor.resumeAfterPlaytest(map)` -> `beginEditMap(map.file)`,
    /// `MapEditorDialog.java:370-372` / `:508-519`).
    ///
    /// Returns `{"playtest": bool, "resumed": bool}`: `playtest` is false (and
    /// nothing happens) for a normal match; `resumed` is false when the map
    /// could not be reopened (`@editor.errorload` is shown, like Java's
    /// `ui.showException`). The caller shows the editor shell on `resumed`.
    #[func]
    pub fn check_playtest(&mut self) -> VarDictionary {
        let status = |playtest: bool, resumed: bool| {
            let mut out = VarDictionary::new();
            out.set(&GString::from("playtest"), &playtest.to_variant());
            out.set(&GString::from("resumed"), &resumed.to_variant());
            out
        };
        let Some(path) = self.playtesting_map.take() else {
            return status(false, false);
        };
        // "no exit save here"; `logic.reset()`.
        self.reset_to_menu();
        let resumed = wiring::optional(&self.editor).is_some_and(|mut editor| {
            editor
                .bind_mut()
                .resume_after_playtest(GString::from(path.as_str()))
        });
        if !resumed {
            log::warn!("check_playtest: could not reopen `{path}` in the editor");
            self.show_error("@editor.errorload");
        }
        status(true, resumed)
    }

    /// `logic.reset()` (`Logic.java:299-311`): THE return-to-menu path (L02
    /// T2). [`Self::run_exit_save`] and [`Self::check_playtest`] end in it,
    /// and the UI calls it for the game-over `@menu` button
    /// (`GameOverDialog.java:134-138`), the editor exit
    /// (`MapEditorDialog.java:789-795`) and the abandon/quit fallbacks
    /// (`UiRoot.return_to_menu`). Saves/session half: persists the sector
    /// record, drops the session runtime and returns to the menu phase
    /// (`state = new GameState()`, so `playtestingMap` is cleared too); world
    /// half: `MindSimHost.reset_to_menu` empties the sim and sets its phase to
    /// menu. Idempotent.
    #[func]
    pub fn reset_to_menu(&mut self) {
        self.playtesting_map = None;
        // `SectorInfo.write()`/`saveInfo` on exit: the live wave/attack bookkeeping
        // lands in the stored record before the runtime is dropped (PAR-0081).
        self.persist_sector_record();
        // T1: `Sector.isBeingPlayed` (`Sector.java:148-151`) is false once the
        // game state leaves `isGame()`; clear it on the live campaign so the
        // sector produces offline from the next turn on.
        if let Some((planet, sector_id)) = self.session.sector {
            self.with_campaign_mut(|campaign, _settings| {
                if let Some(sector) = campaign.sector_mut(planet, sector_id) {
                    sector.being_played = false;
                }
            });
        }
        // `StateChangeEvent(menu)`: clears current/playtime (`logic.reset`).
        self.saves.on_state_change(State::Menu);
        self.saving_finish_tick = None;
        self.session.reset_world();
        self.runtime_installed = false;
        // T1: mirror the runtime campaign before dropping it, or the turn
        // clock and every other sector's turn production are lost.
        self.mirror_runtime_campaign();
        if let Some(mut host) = self.host() {
            host.bind_mut().clear_campaign_runtime();
            // `Groups.clear(); world.tiles = new Tiles(0, 0); state = new
            // GameState()` (`Logic.java:300-307`).
            host.bind_mut().reset_to_menu();
        }
        self.persist();
    }

    /// `run_exit_save` / capture bookkeeping: writes the live sector's stored
    /// record (`SectorInfo.prepare` + `Sector.saveInfo`) so captures, wave
    /// progress, item pools and the production windows survive a client restart
    /// (PAR-0081/PAR-0082).
    fn persist_sector_record(&mut self) {
        // The record writer takes the registry explicitly (finding #1); take the
        // facade copy for the call and restore it afterwards so `&mut self` is
        // free for the campaign/settings half.
        let registry = self.registry.take();
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                self.persist_sector_record_in(Some(&mut runtime), registry.as_ref());
                host.bind_mut().put_campaign_runtime(runtime);
                self.registry = registry;
                return;
            }
        }
        self.persist_sector_record_in(None, registry.as_ref());
        self.registry = registry;
    }

    /// `persist_sector_record` body. `runtime` is the already-taken live
    /// runtime when the caller holds it (`pump_runtime` events); with `None`
    /// the host runtime (when installed) is taken for the read and put back.
    ///
    /// `registry` is the explicit content registry (finding #1): `pump_runtime`
    /// has `self.registry` `take()`n for its whole event window, so reading the
    /// facade field here silently skipped the capture/game-over record write.
    /// Every caller owns the borrow (the `&mut self` below cannot hold one).
    fn persist_sector_record_in(
        &mut self,
        mut runtime: Option<&mut CampaignRuntime>,
        registry: Option<&ContentRegistry>,
    ) {
        let Some((planet, sector_id)) = self.session.sector else {
            return;
        };
        let Some(planet_name) = registry
            .and_then(|registry| registry.planet(planet))
            .map(|record| record.name.clone())
        else {
            return;
        };
        let wave = self.session.wave;
        let rules = self.session.rules.clone();
        // `SectorInfo.prepare(sector)` (`SaveVersion.writeMeta`): the live core
        // snapshot lands in the record before it is written (PAR-0082).
        let default_team = self.session.rules.default_team;
        let snapshot = self
            .host()
            .and_then(|host| host.bind().campaign_core_snapshot(default_team));
        // The live runtime owns the authoritative record (the campaign tick
        // folds the core/production deltas into it); the facade clone only
        // mirrors on events, so read the runtime's copy when the caller holds
        // it (or the host does).
        let source = match &runtime {
            Some(live) => live
                .campaign
                .sector(planet, sector_id)
                .map(|sector| sector.info.info.clone()),
            None => self.runtime_or_facade_record(planet, sector_id),
        };
        let Some(mut record) = source else {
            return;
        };
        if let Some(snapshot) = snapshot {
            // `SectorInfo.prepare` `lastPresetName` (`SectorInfo.java:226-228`).
            let preset = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(planet, sector_id))
                .and_then(|sector| sector.preset);
            let preset_name = preset.and_then(|preset| {
                registry
                    .and_then(|registry| registry.sector(preset))
                    .map(|def| def.name.clone())
            });
            let mut prepared = mind_core::game::sector::SectorInfoState {
                info: record,
                ..Default::default()
            };
            prepared.prepare(&snapshot, true, preset_name.as_deref());
            record = prepared.info;
        }
        record.wave = wave;
        record.waves = rules.waves;
        record.wave_spacing = rules.wave_spacing;
        record.win_wave = rules.win_wave;
        record.attack = rules.attack_mode;
        // Java has one object graph (`Vars.universe`): the prepared record is
        // THE live record. Without writing it back, the next runtime->facade
        // mirror overwrites the facade with the unprepared copy and the
        // lost-sector replay's `sectorDataMatches` (`SectorInfo.java:98-103`)
        // fails on an in-process re-attack (H04 V1).
        if let Some(live) = &mut runtime
            && let Some(sector) = live.campaign.sector_mut(planet, sector_id)
        {
            sector.info.info = record.clone();
        }
        // Keep the facade copy's record in step for the dialog views.
        if let Some(campaign) = self.campaign.as_mut()
            && let Some(sector) = campaign.sector_mut(planet, sector_id)
        {
            sector.info.info = record.clone();
        }
        let key = mind_core::game::sector::info_key(&planet_name, sector_id);
        if let Err(error) = settings_hub::with_settings(|store| store.put_json(&key, &record)) {
            log::warn!("sector record persist failed: {error}");
        }
        // `put_json` only marks the store dirty; flush so the record is on disk
        // before any restart (the capture path calls this after `pump_runtime`).
        if let Err(error) = settings_hub::save(&self.paths) {
            log::warn!("sector record flush failed: {error}");
        }
    }

    /// `Vars.hadSerpuloRemaps && settings.getBoolOnce("serpulo-remaps-notice")`:
    /// Java shows the one-time "Campaign Changes Notice" dialog after a Serpulo
    /// remap (`PlanetDialog.java:216-222`). `had_serpulo_remaps` is only true in
    /// the boot that performed the remap (same as Java's static flag), so this
    /// consumes the notice flag once and reports whether to show it.
    #[func]
    pub fn take_serpulo_remaps_notice(&mut self) -> bool {
        let noticed =
            settings_hub::with_settings(|store| store.get_bool("serpulo-remaps-notice", false));
        if !self.saves.had_serpulo_remaps || noticed {
            return false;
        }
        settings_hub::with_settings(|store| store.put_bool("serpulo-remaps-notice", true));
        if let Err(error) = settings_hub::save(&self.paths) {
            log::warn!("remap notice flush failed: {error}");
        }
        true
    }

    /// The live runtime's sector record when a host is attached, else the
    /// facade copy's record.
    fn runtime_or_facade_record(
        &self,
        planet: mind_core::content::PlanetId,
        sector_id: u16,
    ) -> Option<mind_core::io::json::rules::SectorInfo> {
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            let record = taken
                .as_ref()
                .and_then(|runtime| runtime.campaign.sector(planet, sector_id))
                .map(|sector| sector.info.info.clone());
            if let Some(runtime) = taken {
                host.bind_mut().put_campaign_runtime(runtime);
            }
            if record.is_some() {
                return record;
            }
        }
        self.campaign
            .as_ref()
            .and_then(|campaign| campaign.sector(planet, sector_id))
            .map(|sector| sector.info.info.clone())
    }

    /// `PlanetDialog.abandonSectorConfirm` for the played sector
    /// (PlanetDialog.java:1153-1161): the caller hides the planet page and
    /// 7 ticks later every player-team core dies, so the campaign game-over
    /// path (`Logic.checkGameState` -> `GameOverEvent` -> sector loss) plays.
    /// Without a live sim runtime (no sector running) it falls back to the
    /// off-play record clear.
    #[func]
    pub fn abandon_sector(&mut self) -> bool {
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                let played = runtime.session.is_campaign();
                runtime.abandon_played_sector();
                host.bind_mut().put_campaign_runtime(runtime);
                if played {
                    log::info!("abandon: player cores die in 7 ticks (game-over path)");
                    return true;
                }
            }
        }
        self.clear_played_sector_record()
    }

    /// The persisted half of an abandon without a running sector: clears the
    /// session sector's items/base/production, drops its save and removes the
    /// file.
    fn clear_played_sector_record(&mut self) -> bool {
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

        // T1: clear the record on the single live campaign (runtime while a
        // sector runs), then mirror into the facade via `with_campaign_mut`.
        let cleared = self
            .with_campaign_mut(|campaign, settings| {
                clear_sector_record(campaign, settings, planet, sector_id, &planet_name);
            })
            .is_some();
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
        self.planet_mesh_cache = None;
        true
    }

    /// `PlanetDialog.abandonSectorConfirm`: an off-play sector loses its items,
    /// base and production in the record (`saveInfo` + persist); the played
    /// sector routes through the existing played-sector clear.
    #[func]
    pub fn abandon_sector_at(&mut self, planet: GString, sector: i32) -> bool {
        if sector < 0 {
            return false;
        }
        let planet_name = planet.to_string();
        let sector_id = sector as u16;
        let Some(planet_id) = self
            .registry
            .as_ref()
            .and_then(|registry| registry.planet_by_name(&planet_name))
            .map(|def| def.id)
        else {
            return false;
        };
        if self.session.sector == Some((planet_id, sector_id)) {
            return self.abandon_sector();
        }
        let Some(true) = self.with_campaign_mut(|campaign, settings| {
            let Some(record) = campaign.sector_mut(planet_id, sector_id) else {
                return false;
            };
            record.info.info.items.clear();
            record.info.info.has_core = false;
            record.info.info.production.clear();
            let _ = record.save_info(settings, &planet_name);
            true
        }) else {
            return false;
        };
        self.persist();
        self.planet_mesh_cache = None;
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
        settings_hub::with_settings(|settings| {
            self.saves.load(&NativeFs, &self.paths, registry, settings);
        });
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
        self.with_campaign_mut(|campaign, settings| {
            campaign.universe.clear_loadout_info(settings);
        });
        let cleared = settings_hub::with_settings(|settings| {
            let mut store = SettingsUnlockStore::new(settings);
            registry.clear_unlocks(&mut store, None)
        });
        tech_tree::reset_all(&mut registry);
        settings_hub::with_settings(|store| {
            store.remove("unlocks");
        });
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
        self.with_campaign_mut(|campaign, settings| {
            campaign.universe.clear_loadout_info(settings);
        });
        let cleared = settings_hub::with_settings(|settings| {
            let mut store = SettingsUnlockStore::new(settings);
            registry.clear_unlocks(&mut store, Some(planet_id))
        });
        if let Some(tree) = tree {
            tech_tree::reset_tree(&mut registry, tree);
        }
        settings_hub::with_settings(|store| {
            store.remove("unlocks");
        });
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
        // T1: read the planet list from the mirrored (live) campaign; the
        // clearing runs on the live campaign through `with_campaign_mut`.
        self.mirror_runtime_campaign();
        let planets: Vec<(PlanetId, String)> = self
            .campaign
            .as_ref()
            .map(|campaign| {
                campaign
                    .planets
                    .values()
                    .filter(|planet| only.is_none_or(|target| target == planet.id))
                    .map(|planet| (planet.id, planet.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        if planets.is_empty() {
            self.registry = Some(registry);
            return false;
        }
        let paths = self.paths.clone();
        let Some(()) = self.with_campaign_mut(|campaign, settings| {
            for (id, name) in &planets {
                campaign.clear_stats(*id);
                let Some(planet) = campaign.planet_mut(*id) else {
                    continue;
                };
                for sector in &mut planet.sectors {
                    sector.save = None;
                    sector.being_played = false;
                    sector.clear_info(settings, name);
                    let file = paths.sector_save(name, sector.id as u32);
                    let backup = mind_core::io::SaveIo::backup_file_for(&file);
                    let _ = NativeFs.delete(&file);
                    let _ = NativeFs.delete(&backup);
                }
            }
        }) else {
            self.registry = Some(registry);
            return false;
        };

        // Re-list saves (a slot may have been written this session) and drop
        // the matching sector slots.
        settings_hub::with_settings(|settings| {
            self.saves.load(&NativeFs, &self.paths, &registry, settings);
        });
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
    pub fn list_save_slots(&mut self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        // `request_save` writes through Godot's `user://`, so scan that same
        // directory instead of the bootstrap-time `Saves` snapshot: a slot saved
        // during this session must show up without a restart. The slot listing
        // is memoized on the directory mtime (finding #9); the rows are rebuilt
        // on every call.
        let paths = self.save_paths();
        let slots = self.cached_save_slots();
        for slot in slots.iter() {
            // `Saves.SaveSlot.previewTexture()` shows the saved preview PNG; a
            // slot that has none (imported saves, pre-fix saves) gets one from
            // its own tiles here (`Saves.java:343-358`).
            let preview = slot.preview_path(&paths);
            if !NativeFs.exists(&preview) {
                let file = slot.file.clone();
                self.render_slot_preview_from_file(&file);
            }
            let mut row = VarDictionary::new();
            row.set(
                &GString::from("name"),
                &settings_hub::with_settings(|store| slot.name(store))
                    .as_str()
                    .to_variant(),
            );
            row.set(
                &GString::from("file"),
                &slot.file.to_string_lossy().as_ref().to_variant(),
            );
            row.set(&GString::from("sector"), &slot.is_sector().to_variant());
            row.set(
                &GString::from("autosave"),
                &settings_hub::with_settings(|store| slot.is_autosave(store)).to_variant(),
            );
            // `SaveSlot.hasExternalAssets` (`@save.export.needsload` guard).
            row.set(
                &GString::from("external_assets"),
                &slot.has_external_assets().to_variant(),
            );
            // `SaveSlot.getMap/mode/getWave/getPlayTime/getDate` meta reads;
            // the dialog resolves `map_name` against the map registry
            // (`SaveMeta`: `maps.all().find(m -> m.name().equals(map))`).
            if let Some(meta) = slot.meta.as_ref() {
                row.set(
                    &GString::from("map_name"),
                    &meta.map_name.as_str().to_variant(),
                );
                row.set(&GString::from("wave"), &i64::from(meta.wave).to_variant());
                row.set(
                    &GString::from("playtime_ms"),
                    &meta.time_played.to_variant(),
                );
                row.set(&GString::from("timestamp_ms"), &meta.timestamp.to_variant());
                let mode = serde_json::from_str::<Rules>(&quote_bare_keys(&meta.rules_json))
                    .map(|rules| rules.mode().name())
                    .unwrap_or("survival");
                row.set(&GString::from("mode"), &mode.to_variant());
            }
            out.push(&row);
        }
        out
    }

    /// The `user://` `Paths` the save slots live in (same root as
    /// `list_save_slots`).
    fn save_paths(&self) -> Paths {
        let root = ProjectSettings::singleton()
            .globalize_path("user://")
            .to_string();
        Paths::new(root)
    }

    /// The `user://saves` slot listing, memoized on the saves directory's mtime
    /// (finding #9): [`list_slot_files`] reads and parses every slot header, and
    /// the campaign dialogs ask for the listing on every rebuild.
    ///
    /// Safe against in-place rewrites: every save path in the port goes through
    /// `SaveIo::save`, which renames `<file> -> <file>-backup` and then
    /// `<file>.tmp -> <file>` inside this directory, so even a logical
    /// overwrite of the same slot bumps the directory mtime; imports and
    /// deletes change the directory too. An unreadable directory bypasses the
    /// memo rather than pinning an empty listing.
    fn cached_save_slots(&self) -> Rc<Vec<SaveSlot>> {
        let paths = self.save_paths();
        let key = dir_mtime_key(&paths.saves());
        if let Some(cached) = self.save_slots_cache.borrow().get(key) {
            return Rc::clone(cached);
        }
        let slots = Rc::new(list_slot_files(&NativeFs, &paths));
        self.save_slots_cache
            .borrow_mut()
            .store(key, Rc::clone(&slots));
        slots
    }

    /// One `user://saves/<stem>.msav` slot, re-read from disk.
    fn save_slot_by_name(&self, stem: &str) -> Option<mind_core::io::save::slot::SaveSlot> {
        let paths = self.save_paths();
        list_slot_files(&NativeFs, &paths)
            .into_iter()
            .find(|slot| slot.index() == stem)
    }

    /// `LoadDialog.addSetup`: imports one `.msav` into `user://saves`
    /// (`SaveIO.isSaveValid` -> `SaveIO.getMeta` sector check ->
    /// `Saves.importSave`, `LoadDialog.java:198-216`). Returns `""` on success
    /// or `"invalid"` / `"nocampaign"` / `"fail:<message>"` for the dialog.
    #[func]
    pub fn import_save(&mut self, path: GString) -> GString {
        let native = ProjectSettings::singleton().globalize_path(&path);
        let from = std::path::PathBuf::from(native.to_string());
        if !SaveIo::is_save_valid(&NativeFs, &from) {
            return GString::from("invalid");
        }
        // `meta.rules.sector != null` -> `@save.nocampaign`: campaign saves are
        // not importable.
        if let Ok(meta) = SaveIo::get_meta(&NativeFs, &from) {
            let has_sector = serde_json::from_str::<serde_json::Value>(&meta.rules_json)
                .ok()
                .and_then(|value| value.get("sector").cloned())
                .is_some_and(|sector| !sector.is_null());
            if has_sector {
                return GString::from("nocampaign");
            }
        }
        let paths = self.save_paths();
        let result = settings_hub::with_settings(|store| {
            self.saves.import_save(&NativeFs, &paths, store, &from)
        });
        match result {
            Ok(index) => {
                // `importData` ships no `previews/` entries (`SettingsMenuDialog.java:575-608`),
                // so the imported slot renders its preview from its own tiles
                // (`Saves.SaveSlot.savePreview`, `Saves.java:343-358`).
                let slot = self
                    .saves
                    .get_save_slots()
                    .get(index)
                    .map(|slot| (slot.index(), slot.file.clone(), slot.preview_path(&paths)));
                if let Some((_stem, file, preview)) = slot
                    && !NativeFs.exists(&preview)
                {
                    self.render_slot_preview_from_file(&file);
                }
                GString::new()
            }
            Err(error) => GString::from(format!("fail:{error}").as_str()),
        }
    }

    /// `LoadDialog.modifyButton` `Icon.trash`: `slot.delete()` plus the preview
    /// PNG and the `save-<n>-name`/`save-<n>-autosave` settings keys
    /// (`Saves.java:245-255`). Returns whether the slot file is gone.
    #[func]
    pub fn delete_save_slot(&mut self, name: GString) -> bool {
        let stem = name.to_string();
        let Some(mut slot) = self.save_slot_by_name(&stem) else {
            return false;
        };
        let paths = self.save_paths();
        let preview = slot.preview_path(&paths);
        if NativeFs.exists(&preview) {
            let _ = NativeFs.delete(&preview);
        }
        settings_hub::with_settings(|store| {
            store.remove(&mind_core::io::settings::slot_name_key(&stem));
            store.remove(&mind_core::io::settings::slot_autosave_key(&stem));
        });
        if let Err(error) = slot.delete(&NativeFs) {
            log::warn!("delete_save_slot `{stem}`: {error}");
            return false;
        }
        if let Err(error) = settings_hub::save(&self.paths) {
            log::warn!("delete_save_slot: settings save failed: {error}");
        }
        true
    }

    /// `LoadDialog.modifyButton` `Icon.pencil`: `slot.setName(text)`.
    #[func]
    pub fn rename_save_slot(&mut self, name: GString, new_name: GString) -> bool {
        let stem = name.to_string();
        let renamed = new_name.to_string();
        if renamed.trim().is_empty() {
            return false;
        }
        let Some(slot) = self.save_slot_by_name(&stem) else {
            return false;
        };
        settings_hub::with_settings(|store| slot.set_name(store, &renamed));
        if let Err(error) = settings_hub::save(&self.paths) {
            log::warn!("rename_save_slot: settings save failed: {error}");
        }
        true
    }

    /// `Saves.getCurrent().isAutosave()` (`PausedDialog.java:116,154-155`):
    /// whether the live current save slot autosaves (`save-<index>-autosave`,
    /// default true). `false` when no slot is current.
    #[func]
    pub fn current_save_autosave(&self) -> bool {
        self.saves
            .current_slot()
            .is_some_and(|slot| settings_hub::with_settings(|store| slot.is_autosave(store)))
    }

    /// `LoadDialog.modifyButton` `Icon.save`: `slot.setAutosave(on)`.
    #[func]
    pub fn set_slot_autosave(&mut self, name: GString, on: bool) -> bool {
        let stem = name.to_string();
        let Some(slot) = self.save_slot_by_name(&stem) else {
            return false;
        };
        settings_hub::with_settings(|store| slot.set_autosave(store, on));
        if let Err(error) = settings_hub::save(&self.paths) {
            log::warn!("set_slot_autosave: settings save failed: {error}");
        }
        true
    }

    /// `LoadDialog.modifyButton` `Icon.export`: `slot.exportFile(to)`
    /// (`FileChooser.export`).
    #[func]
    pub fn export_save_slot(&mut self, name: GString, path: GString) -> bool {
        let stem = name.to_string();
        let Some(mut slot) = self.save_slot_by_name(&stem) else {
            return false;
        };
        let native = ProjectSettings::singleton().globalize_path(&path);
        let to = std::path::PathBuf::from(native.to_string());
        match slot.export_file(&NativeFs, &to, None) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("export_save_slot `{stem}`: {error}");
                false
            }
        }
    }

    /// `Universe.getLaunchResources` (item-id-indexed last-launch sequence).
    #[func]
    pub fn get_launch_resources(&mut self) -> PackedInt32Array {
        let mut out = PackedInt32Array::new();
        for value in self
            .campaign
            .as_mut()
            .map(|campaign| {
                settings_hub::with_settings(|settings| {
                    campaign.universe.get_launch_resources(settings)
                })
            })
            .unwrap_or_default()
        {
            out.push(*value);
        }
        out
    }

    /// `Universe.getLaunchResources` as a `{item name: amount}` JSON object for
    /// the launch dialogs (`LaunchLoadoutDialog` merges it with the schematic
    /// requirements; PAR-0085). Only nonzero entries are reported.
    #[func]
    pub fn get_launch_resource_stacks(&mut self) -> GString {
        let resources = self
            .campaign
            .as_mut()
            .map(|campaign| {
                settings_hub::with_settings(|settings| {
                    campaign.universe.get_launch_resources(settings).to_vec()
                })
            })
            .unwrap_or_default();
        let Some(registry) = self.registry.as_ref() else {
            return GString::from("{}");
        };
        let mut out = serde_json::Map::new();
        for (index, item) in registry.items().iter().enumerate() {
            let Some(amount) = resources.get(index).copied() else {
                break;
            };
            if amount > 0 {
                out.insert(item.name.clone(), serde_json::json!(amount));
            }
        }
        GString::from(serde_json::Value::Object(out).to_string().as_str())
    }

    /// `Universe.updateLaunchResources` from the loadout editor's
    /// `{item: amount}` map: the `LoadoutDialog` hider callback
    /// (`LaunchLoadoutDialog.java:138-141`) writes the edited `ItemSeq` back
    /// before the parent rows refresh. Items are placed in the dense
    /// item-id-indexed sequence the launch flow charges and stocks.
    #[func]
    pub fn set_launch_resources(&mut self, amounts: VarDictionary) -> bool {
        let stacks = {
            let Some(registry) = self.registry.as_ref() else {
                return false;
            };
            let mut amounts_by_name: Vec<(String, i64)> = Vec::new();
            for key in amounts.keys_array().iter_shared() {
                let Ok(name) = key.try_to::<GString>() else {
                    continue;
                };
                // The editor stores amounts as GDScript `int` variants
                // (`amount.to_variant()`); a numeric read must accept INT as
                // well as FLOAT, because `try_to::<f64>` rejects INT and
                // silently zeroed every edited amount (PAR-0488).
                let amount = amounts
                    .get(&key)
                    .map(|value| variant_amount(&value))
                    .unwrap_or(0);
                amounts_by_name.push((name.to_string(), amount));
            }
            mind_core::ui::campaign::launch_resource_sequence(registry, &amounts_by_name)
        };
        let Some(result) = self.with_campaign_mut(|campaign, settings| {
            campaign.universe.update_launch_resources(settings, stacks)
        }) else {
            return false;
        };
        if result.is_err() {
            return false;
        }
        self.persist();
        true
    }

    /// `Universe.updateLaunchResources` from a JSON `[n, ...]` sequence.
    #[func]
    pub fn update_launch_resources_json(&mut self, json: GString) -> bool {
        let Ok(stacks) = serde_json::from_str::<Vec<i32>>(&json.to_string()) else {
            return false;
        };
        let Some(result) = self.with_campaign_mut(|campaign, settings| {
            campaign.universe.update_launch_resources(settings, stacks)
        }) else {
            return false;
        };
        if result.is_err() {
            return false;
        }
        self.persist();
        true
    }

    /// `LaunchLoadoutDialog.update` (`LaunchLoadoutDialog.java:58-90`): the
    /// Configure Loadout dialog's inputs and outputs as JSON.
    ///
    /// `launcher`/`destination` are sector ids on `planet`; `loadout_index` is a
    /// `Schematics.all` index (`-1`/out of range selects the launcher core's
    /// default loadout). Output: `{"from", "from_preset", "capacity",
    /// "allow_launch_loadout", "stored", "requirements", "resources", "max",
    /// "valid", "total", "preset_loadout"}`. `resources` is the post-`max`
    /// staged sequence (persisted like Java while `max` is on), so the launcher
    /// branch of `PlanetDialog.playSelected` charges/stocks exactly it.
    #[func]
    pub fn launch_dialog_json(
        &mut self,
        planet: GString,
        launcher: i32,
        destination: i32,
        loadout_index: i32,
    ) -> GString {
        if launcher < 0 || destination < 0 {
            return GString::from("{}");
        }
        let planet_name = planet.to_string();
        let Some(registry) = self.registry.as_ref() else {
            return GString::from("{}");
        };
        let Some(planet_id) = self
            .campaign
            .as_ref()
            .and_then(|campaign| campaign.planet_id_by_name(&planet_name))
        else {
            return GString::from("{}");
        };

        // Snapshot the sector reads before the `universe` mutable borrow.
        let (
            sitems,
            destination_allow,
            destination_allow_schematics,
            destination_preset,
            launcher_core_name,
            planet_default_core,
            from_name,
            from_preset,
        ) = {
            let Some(campaign) = self.campaign.as_ref() else {
                return GString::from("{}");
            };
            let Some(planet_record) = campaign.planet(planet_id) else {
                return GString::from("{}");
            };
            let Some(planet_def) = registry.planet(planet_id) else {
                return GString::from("{}");
            };
            let (Some(launcher_sector), Some(destination_sector)) = (
                planet_record.sector(launcher as u16),
                planet_record.sector(destination as u16),
            ) else {
                return GString::from("{}");
            };
            // `Core.bundle.format("launch.from", sector.name())`
            // (`LaunchLoadoutDialog.java:165`): one display-name implementation.
            let from_name = launcher_sector.display_name(planet_record);
            (
                launcher_sector.items().clone(),
                destination_sector.allow_launch_loadout(planet_def.allow_launch_loadout),
                destination_sector.allow_launch_schematics(planet_def.allow_launch_schematics),
                destination_sector.preset,
                launcher_sector.info.info.best_core_type.clone(),
                planet_def.default_core.clone().unwrap_or_default(),
                from_name,
                launcher_sector.preset_name.clone(),
            )
        };

        let schematics = self.load_schematics(registry);
        // `PlanetDialog.java:1405`: the picker's core is the launcher's best
        // core when the *destination* allows launch schematics, and the
        // planet's default core otherwise (`LaunchLoadoutDialog.java:162`
        // then seeds the selection from that core's last-used loadout).
        let core_name = launch_core_name(
            registry,
            &planet_default_core,
            &launcher_core_name,
            destination_allow_schematics,
        );
        let core = registry.block_id(&core_name);
        // `universe.getLoadout(core)` (`LaunchLoadoutDialog.java:162`): the
        // last loadout used for the launcher's core type, persisted as
        // `lastloadout-<core>`; Java falls back to `core-shard`'s first
        // loadout (`:163`).
        let seeded_index = self.campaign.as_ref().and_then(|campaign| {
            settings_hub::with_settings(|settings| {
                campaign
                    .universe
                    .get_loadout_index(settings, &schematics, registry, &core_name)
            })
        });
        let default_index = seeded_index.or_else(|| {
            registry
                .block_id("core-shard")
                .and_then(|shard| schematics.get_loadouts(shard).first().copied())
        });
        let selected_index = usize::try_from(loadout_index)
            .ok()
            .filter(|index| *index < schematics.all.len())
            .or(default_index)
            .unwrap_or(0);
        let Some(selected) = schematics.all.get(selected_index) else {
            return GString::from("{}");
        };

        // `cap = (int)(sector.planet.launchCapacityMultiplier *
        // selected.findCore().itemCapacity)`.
        let capacity = selected
            .find_core(registry)
            .and_then(|core| registry.block(core))
            .map(|block| {
                (registry
                    .planet(planet_id)
                    .map(|planet| planet.launch_capacity_multiplier)
                    .unwrap_or(0.0)
                    * block.item_capacity as f32) as i32
            })
            .unwrap_or(0);

        // `ItemSeq schems = selected.requirements()`.
        let requirements = launch_requirements(selected, registry);

        // `universe.getLaunchResources()`, with the `show()` on-planet filter
        // (`launch.set(item, 0)` for items not on the launcher's planet).
        let mut resources: indexmap::IndexMap<String, i32> = indexmap::IndexMap::new();
        {
            let Some(campaign) = self.campaign.as_mut() else {
                return GString::from("{}");
            };
            let launch_seq = settings_hub::with_settings(|settings| {
                campaign.universe.get_launch_resources(settings)
            });
            for (index, item) in registry.items().iter().enumerate() {
                if registry
                    .planet(planet_id)
                    .is_some_and(|planet| planet.allow_launch_loadout)
                    && !Self::item_on_planet(registry, &item.name, planet_id)
                {
                    continue;
                }
                let amount = launch_seq.get(index).copied().unwrap_or(0);
                if amount != 0 {
                    resources.insert(item.name.clone(), amount);
                }
            }
        }

        let preset_loadout = destination_preset
            .map(|preset| Self::preset_launch_loadout(registry, planet_id, preset))
            .unwrap_or_default();
        let max = settings_hub::with_settings(|store| store.get_bool("maxresources", true));
        let all_items: Vec<String> = registry
            .items()
            .iter()
            .map(|item| item.name.clone())
            .collect();
        let (planned, valid) = launch_plan(
            &sitems,
            &requirements,
            &resources,
            capacity,
            max,
            destination_allow,
            &preset_loadout,
            &all_items,
        );

        // `universe.updateLaunchResources(resources)`: persist the recomputed
        // sequence so the launch path reads the same staged amounts (T1: on the
        // live campaign). `registry` borrows `self.registry` here, so this
        // cannot route through `with_campaign_mut` (which borrows all of
        // `self`); take/put the runtime directly instead.
        let mut persist_resources = false;
        if destination_allow && max {
            let mut dense = vec![0i32; registry.items().len()];
            for (name, amount) in &planned {
                if let Some(index) = registry.item_id(name)
                    && let Some(slot) = dense.get_mut(index.index())
                {
                    *slot = *amount;
                }
            }
            let mut updated = false;
            if let Some(mut host) = self.host() {
                let taken = host.bind_mut().take_campaign_runtime();
                if let Some(mut runtime) = taken {
                    persist_resources = settings_hub::with_settings(|settings| {
                        runtime
                            .campaign
                            .universe
                            .update_launch_resources(settings, std::mem::take(&mut dense))
                            .is_ok()
                    });
                    host.bind_mut().put_campaign_runtime(runtime);
                    updated = true;
                }
            }
            if !updated && let Some(campaign) = self.campaign.as_mut() {
                persist_resources = settings_hub::with_settings(|settings| {
                    campaign
                        .universe
                        .update_launch_resources(settings, dense)
                        .is_ok()
                });
            }
        }

        // `total = selected.requirements() + universe.getLaunchResources()`
        // (`LaunchLoadoutDialog.java:79-84`): the launcher pays exactly this
        // at `PlanetDialog.playSelected` (`PlanetDialog.java:1409-1410`).
        let total = launch_total(&requirements, &planned);
        // `LaunchLoadoutDialog.java:167-196`: the schematic grid only renders
        // when the destination allows launch schematics, offers every loadout
        // whose core is no larger than the picker's core, and skips loadouts
        // with a block that cannot be built on the launcher's planet.
        let core_size = core
            .and_then(|core| registry.block(core))
            .map(|block| block.size)
            .unwrap_or(3);
        let env = registry
            .planet(planet_id)
            .map(|planet| mind_core::game::planet::env_mask(&planet.default_env))
            .unwrap_or(0);
        let sun = registry.planet_by_name("sun").map(|planet| planet.id);
        let mut loadout_rows: Vec<serde_json::Value> = Vec::new();
        if destination_allow_schematics {
            for index in 0..schematics.all.len() {
                let Some(loadout) = schematics.all.get(index) else {
                    continue;
                };
                let Some(loadout_core) = loadout.find_core(registry) else {
                    continue;
                };
                let Some(def) = registry.block(loadout_core) else {
                    continue;
                };
                if def.size > core_size {
                    continue;
                }
                if !schematic_placeable_on_planet(loadout, registry, planet_id, env, sun) {
                    continue;
                }
                loadout_rows.push(serde_json::json!({
                    "index": index,
                    "name": loadout.name(),
                    "core": def.name,
                    "is_default": schematics.get_default_loadout(loadout_core) == Some(index),
                    "file": loadout.file,
                }));
            }
        }
        let nonzero = |map: &indexmap::IndexMap<String, i32>| {
            let mut out = serde_json::Map::new();
            for (item, amount) in map {
                if *amount != 0 {
                    out.insert(item.clone(), serde_json::json!(amount));
                }
            }
            out
        };
        // `from.removeItems(loadout.requirements())` +
        // `from.removeItems(universe.getLaunchResources())` as one stack list
        // (`PlanetDialog.java:1409-1410`); `planet_launch` charges this.
        let launch_cost: Vec<serde_json::Value> = total
            .iter()
            .filter(|(_, amount)| **amount != 0)
            .map(|(item, amount)| serde_json::json!({"item": item, "amount": amount}))
            .collect();
        // `LaunchLoadoutDialog.java:204-210`: a destination that forbids launch
        // schematics shows the preset description pane instead of the grid.
        let preset_description =
            resolve_preset_description(Some(registry), &self.bundle, destination_preset);
        let value = serde_json::json!({
            "from": from_name,
            "from_preset": from_preset,
            "capacity": capacity,
            "allow_launch_loadout": destination_allow,
            "allow_launch_schematics": destination_allow_schematics,
            "preset_description": preset_description,
            "core": core_name,
            "stored": nonzero(&sitems),
            "requirements": nonzero(&requirements),
            "resources": nonzero(&planned),
            "max": max,
            "valid": valid,
            "total": nonzero(&total),
            "launch_cost": launch_cost,
            "preset_loadout": nonzero(&preset_loadout),
            "loadouts": loadout_rows,
            "selected": selected_index,
            "selected_file": selected.file,
        });
        if persist_resources {
            self.persist();
        }
        GString::from(value.to_string().as_str())
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
        let core_name = core.to_string();
        let file_name = file.to_string();
        let file_name = (!file_name.is_empty()).then_some(file_name.as_str());
        let Some(()) = self.with_campaign_mut(|campaign, settings| {
            campaign
                .universe
                .update_loadout(settings, &core_name, file_name);
        }) else {
            return false;
        };
        self.persist();
        true
    }

    /// `Universe.clearLoadoutInfo`: a free launch (no launcher sector) drops
    /// the stored loadout + launch resources (`PlanetDialog.playSelected`).
    #[func]
    pub fn clear_loadout_info(&mut self) -> bool {
        let Some(()) = self.with_campaign_mut(|campaign, settings| {
            campaign.universe.clear_loadout_info(settings);
        }) else {
            return false;
        };
        self.persist();
        true
    }

    /// `Sector.removeItems`: subtracts `[{item, amount}]` stacks from a
    /// sector's stored items (clamped at 0) and persists (PAR-0085 launch
    /// cost, applied to the launcher sector before the launch).
    #[func]
    pub fn remove_sector_items(
        &mut self,
        planet: GString,
        sector: i32,
        stacks_json: GString,
    ) -> bool {
        if sector < 0 {
            return false;
        }
        let Ok(stacks) = serde_json::from_str::<
            Vec<mind_core::io::json::content_serde::JsonItemStack>,
        >(&stacks_json.to_string()) else {
            return false;
        };
        let planet_name = planet.to_string();
        let stacks: Vec<(String, i32)> = stacks
            .iter()
            .filter_map(|stack| stack.item.clone().map(|item| (item, stack.amount)))
            .collect();
        // T1: mutate the single live campaign; `Sector.removeItems` ->
        // `addItems` -> `saveInfo()` (`Sector.java:207-231`) persists the
        // record right away (the PAR-0085 launch cost must stick even while
        // a sector runs).
        let Some(true) = self.with_campaign_mut(|campaign, settings| {
            let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
                return false;
            };
            let Some(record) = campaign.sector_mut(planet_id, sector as u16) else {
                return false;
            };
            record.remove_items(stacks.iter().cloned());
            let _ = record.save_info(settings, &planet_name);
            true
        }) else {
            return false;
        };
        self.persist();
        true
    }

    /// The planet-globe reveal list (`PlanetDialog.java:270-288`): collects
    /// this planet's unlocked, still-hidden presets, marks `info.shown` and
    /// saves each sector record (`Sector.saveInfo`); the caller appends the
    /// last sector, reverses and replays the reveal.
    #[func]
    pub fn take_new_presets(&mut self, planet: GString) -> PackedInt32Array {
        let planet_name = planet.to_string();
        let mut out = PackedInt32Array::new();
        let Some(registry) = self.registry.take() else {
            return out;
        };
        let Some(ids) = self.with_campaign_mut(|campaign, settings| {
            let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
                return Vec::new();
            };
            let Some(planet) = campaign.planet_mut(planet_id) else {
                return Vec::new();
            };
            let ids = planet.new_presets(&registry);
            for id in &ids {
                if let Some(sector) = planet.sectors.get(*id as usize) {
                    let _ = sector.save_info(settings, &planet_name);
                }
            }
            ids
        }) else {
            self.registry = Some(registry);
            return out;
        };
        self.registry = Some(registry);
        for id in ids {
            out.push(id as i32);
        }
        if !out.is_empty() {
            self.persist();
        }
        out
    }

    /// `Sector.setName` (`Sector.java:164-167`): stores the display name in the
    /// sector's info record and persists (`saveInfo`); an empty name clears it.
    #[func]
    pub fn set_sector_name(&mut self, planet: GString, sector: i32, name: GString) -> bool {
        if sector < 0 {
            return false;
        }
        let planet_name = planet.to_string();
        let display = name.to_string();
        let display = (!display.is_empty()).then_some(display);
        let Some(true) = self.with_campaign_mut(|campaign, settings| {
            let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
                return false;
            };
            let Some(record) = campaign.sector_mut(planet_id, sector as u16) else {
                return false;
            };
            record.info.info.name = display.clone();
            let _ = record.save_info(settings, &planet_name);
            true
        }) else {
            return false;
        };
        self.persist();
        true
    }

    /// Sector marker icon (`PlanetDialog.java:1223-1283`): an accessible
    /// `info.icon` (`Iconc` field name) or an `info.contentIcon` content name;
    /// the unused half is stored as `None` (both empty = `Icon.none`).
    #[func]
    pub fn set_sector_icon(
        &mut self,
        planet: GString,
        sector: i32,
        icon: GString,
        content_icon: GString,
    ) -> bool {
        if sector < 0 {
            return false;
        }
        let planet_name = planet.to_string();
        let icon = icon.to_string();
        let content_icon = content_icon.to_string();
        let Some(true) = self.with_campaign_mut(|campaign, settings| {
            let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
                return false;
            };
            let Some(record) = campaign.sector_mut(planet_id, sector as u16) else {
                return false;
            };
            record.info.info.icon = (!icon.is_empty()).then_some(icon.clone());
            record.info.info.content_icon =
                (!content_icon.is_empty()).then_some(content_icon.clone());
            let _ = record.save_info(settings, &planet_name);
            true
        }) else {
            return false;
        };
        self.persist();
        true
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
        self.bootstrap_with(false);
        self.content_error.is_none()
    }

    /// `@settings.cleardata` host half: drops the in-memory campaign, registry,
    /// save and settings state so the exit persist cannot resurrect the data
    /// the UI is about to delete.
    #[func]
    pub fn clear_in_memory(&mut self) {
        settings_hub::with_settings(|store| store.clear());
        self.campaign = None;
        self.registry = None;
        // The memos hold content-derived data (schematic decode) and a
        // filesystem listing under the root the data clear is about to wipe.
        self.invalidate_caches();
        self.saves = Saves::new();
        self.session = PlaySession::default();
        self.runtime_installed = false;
        self.content_error = None;
    }

    /// `SaveSlot.getPlayTime()` for the current slot (`GameOverDialog`'s
    /// "Time Played" row, `GameOverDialog.java:91-97`): the live accumulated
    /// playtime, else the slot's saved meta. `-1` when no slot is current
    /// (Java hides the row then).
    #[func]
    pub fn current_save_playtime_ms(&self) -> i64 {
        let Some(slot) = self.saves.get_current() else {
            return -1;
        };
        if self.saves.total_playtime > 0 {
            return self.saves.total_playtime;
        }
        slot.meta
            .as_ref()
            .map(|meta| meta.time_played)
            .unwrap_or_default()
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

/// `SaveSlot.load` meta half (`Saves.java:314-328`): the saved rules/wave,
/// wave timer and stats replace the live session's, and the loaded match is
/// playing (`readMeta`/`readRules`).
fn apply_loaded_meta(session: &mut PlaySession, meta: &SaveMeta) {
    if let Ok(rules) = serde_json::from_str::<Rules>(&quote_bare_keys(&meta.rules_json)) {
        session.rules = rules;
    } else {
        log::warn!("load: rules JSON did not parse; session rules kept");
    }
    session.wave = meta.wave;
    if let Some(wavetime) = meta
        .tags
        .get("wavetime")
        .and_then(|value| value.parse::<f32>().ok())
    {
        session.wavetime = wavetime;
    }
    if let Some(stats) = meta
        .tags
        .get("stats")
        .and_then(|value| serde_json::from_str::<GameStats>(value).ok())
    {
        session.stats = stats;
    }
    session.phase = State::Playing;
}

/// Sanitizes a save-slot name into a file stem (alphanumerics, `-`, `_`).
///
/// **Deviation (J01 T6, kept deliberately).** Java slots are numeric files:
/// `Saves.getNextSlotFile` builds `saves/<index>.msav` and the display name
/// lives in the `save-<index>-name` setting / the meta (`Saves.java:265-271,
/// 283-290`), previews as `previews/save_slot_<index>.png`. The port keys the
/// file, the `save-<stem>-name`/`-autosave` settings and the preview PNG by
/// this sanitized name — the round-I I06 flow (`save_slot("t5-preview")` ->
/// `save_slot_t5-preview.png`) built the load/save/import/rename/export
/// surface on stems, and `Saves::import_save` keeps imported Java saves on
/// their numeric stems. A Java data export/import therefore round-trips by
/// slot identity, not by file name; switching to numeric indices would churn
/// the save/preview/settings ABI for no behavioural gain (the slot name is
/// user data in both designs).
fn sanitize_slot_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect()
}

/// `SaveSlot.load`'s policy half (`Saves.java:319-324`): the loaded slot
/// becomes `Saves.current` and its saved playtime seeds the accumulator, so the
/// autosave cadence, the playtime accrual and the exit save all key on the slot
/// the player loaded (`MindCampaign.load_slot`, CG-SAVE-02). Returns the slot
/// index — the entry is created when the file is not listed yet.
fn adopt_loaded_slot(
    saves: &mut Saves,
    settings: &mut SettingsStore,
    file: &std::path::Path,
    time_played: i64,
) -> usize {
    saves.total_playtime = time_played;
    saves.set_current_by_file(settings, file)
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

/// Reads a numeric `Variant` as an integer (`LoadoutDialog`'s amounts are
/// GDScript `int`s; JSON-derived values are `float`s).
fn variant_amount(value: &Variant) -> i64 {
    match value.get_type() {
        VariantType::INT => value.try_to::<i64>().unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number as i64)
            .unwrap_or_default(),
        _ => 0,
    }
}

/// `PlanetDialog.java:1405`: the core whose loadout list the launch picker
/// offers. With `destination.allowLaunchSchematics()` the launcher's best core
/// wins (`from.info.bestCoreType instanceof CoreBlock`), otherwise the planet's
/// default core is forced (PAR-0509). A missing/`air` best core or default
/// falls back to `core-shard`.
fn launch_core_name(
    registry: &ContentRegistry,
    planet_default_core: &str,
    launcher_best_core: &str,
    allow_schematics: bool,
) -> String {
    let is_core_block = |name: &str| {
        registry
            .block_id(name)
            .and_then(|id| registry.block(id))
            .is_some_and(|def| def.kind == mind_core::content::BlockKind::CoreBlock)
    };
    let fallback = if is_core_block(planet_default_core) {
        planet_default_core.to_owned()
    } else {
        "core-shard".to_owned()
    };
    if allow_schematics && is_core_block(launcher_best_core) {
        launcher_best_core.to_owned()
    } else {
        fallback
    }
}

/// `Schematic.requirements()` as a non-zero `item name -> amount` map, in item
/// content order (the `ItemSeq` the launch charge is read from).
fn launch_requirements(
    schematic: &mind_core::game::schematic::Schematic,
    registry: &ContentRegistry,
) -> indexmap::IndexMap<String, i32> {
    let requirements = schematic.requirements(registry);
    registry
        .items()
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let amount = requirements.get(ItemId::new(index as u16));
            (amount != 0).then(|| (item.name.clone(), amount))
        })
        .collect()
}

/// `LaunchLoadoutDialog.update`'s `total` (`LaunchLoadoutDialog.java:79-84`):
/// the selected schematic's requirements plus the staged launch resources.
/// `PlanetDialog.playSelected` removes exactly this from the launcher
/// (`PlanetDialog.java:1409-1410`), so it must be priced from the schematic
/// the picker actually selected rather than the planet-default core's loadout
/// list (PAR-0489).
fn launch_total(
    requirements: &indexmap::IndexMap<String, i32>,
    resources: &indexmap::IndexMap<String, i32>,
) -> indexmap::IndexMap<String, i32> {
    let mut total = requirements.clone();
    for (item, amount) in resources {
        if *amount != 0 {
            *total.entry(item.clone()).or_insert(0) += *amount;
        }
    }
    total
}

/// `LaunchLoadoutDialog.java:171-176`: a loadout is offered only when every
/// tile's block supports the launcher planet's default environment and can be
/// built there (`Block.supportsEnv`/`Block.isOnPlanet`).
fn schematic_placeable_on_planet(
    schematic: &mind_core::game::schematic::Schematic,
    registry: &ContentRegistry,
    planet: PlanetId,
    env: i32,
    sun: Option<PlanetId>,
) -> bool {
    schematic.tiles.iter().all(|tile| {
        registry.block(tile.block).is_some_and(|def| {
            mind_core::world::build::supports_env_bits(def, env)
                && mind_core::world::build::environment_buildable(def, Some(planet), sun)
        })
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::content::MemoryUnlockStore;
    use mind_core::content::create_base_content;

    /// Test-only `expect`: the workspace denies `clippy::expect_used`.
    fn require<T>(value: Option<T>, what: &str) -> T {
        match value {
            Some(value) => value,
            None => panic!("missing {what}"),
        }
    }

    /// Test-only `expect` for fallible setup (`clippy::expect_used` is denied).
    fn require_ok<T, E: std::fmt::Display>(value: Result<T, E>, what: &str) -> T {
        match value {
            Ok(value) => value,
            Err(error) => panic!("{what}: {error}"),
        }
    }

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

    /// CG-SAVE-02: `SaveSlot.load` (`Saves.java:319-324`) makes the loaded slot
    /// `Saves.current` and seeds the playtime from its meta, so the autosave
    /// cadence, the exit save and the pause label key on the slot the player
    /// loaded. Before the fix `load_slot` left `current` at `None`, so this
    /// policy was inert for the whole session.
    #[test]
    fn loaded_slot_becomes_current_and_autosaves() {
        let mut settings = SettingsStore::new();
        let mut saves = Saves::new();
        let file = std::path::PathBuf::from("/data/saves/t5-load.msav");
        assert!(saves.get_current().is_none(), "no slot is current at boot");

        let index = adopt_loaded_slot(&mut saves, &mut settings, &file, 12_345);
        assert_eq!(saves.current, Some(index), "the loaded slot is current");
        assert_eq!(saves.get_total_playtime(), 12_345);
        assert_eq!(
            saves.get_current().map(|slot| slot.file.clone()),
            Some(file)
        );
        // `save-<stem>-autosave` defaults to true (`SaveSlot.isAutosave`), so a
        // loaded slot takes part in the autosave cadence like Java's.
        assert!(
            saves
                .get_current()
                .is_some_and(|slot| slot.is_autosave(&settings))
        );
        let update = SavesUpdate {
            delta_ticks: 61 * 60,
            is_game: true,
            paused_with_dialog: false,
            game_over: false,
            is_campaign: false,
        };
        // 61 s crosses the default 60 s interval; the same call seeds the
        // playtime timestamp, so the next one accrues on the loaded slot.
        assert!(
            saves
                .update_policy(update, &settings, DEFAULT_SAVE_INTERVAL_SECONDS)
                .autosaved
        );
        let accrued = saves.update_policy(update, &settings, DEFAULT_SAVE_INTERVAL_SECONDS);
        assert_eq!(accrued.playtime_added, 61_000);
        assert!(saves.get_total_playtime() > 12_345);
    }

    /// PAR-0489: the launch charge is the *selected* schematic's requirements
    /// plus the staged launch resources (`PlanetDialog.java:1409-1410`), not
    /// the planet-default core's loadout list. The old path looked the chosen
    /// index up in `campaign_section("loadouts")` (built from
    /// `planet.default_core`), which never contains a larger-core schematic,
    /// so `core-nucleus`+ loadouts launched for free.
    #[test]
    fn launch_cost_prices_the_selected_schematic_not_the_default_core_list() {
        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        let mut schematics = Schematics::new();
        schematics.load_loadouts(&registry);

        // `basicNucleus` is the vanilla `core-nucleus` loadout.
        let nucleus_core = require(registry.block_id("core-nucleus"), "core-nucleus");
        let nucleus_index = require(
            schematics.get_default_loadout(nucleus_core),
            "basicNucleus loadout",
        );
        let nucleus = &schematics.all[nucleus_index];
        let requirements = launch_requirements(nucleus, &registry);
        assert!(
            !requirements.is_empty(),
            "basicNucleus carries its blocks' build costs"
        );

        // The old cost source (the planet-default core's loadout rows) never
        // lists the nucleus schematic, which is why its requirements were lost.
        let default_rows =
            mind_core::ui::campaign::loadout_views(&schematics, &registry, "core-shard");
        assert!(!default_rows.is_empty(), "core-shard has loadouts");
        assert!(
            default_rows.iter().all(|row| row.index != nucleus_index),
            "the default-core list cannot price a nucleus loadout (PAR-0489)"
        );

        // `total = selected.requirements() + universe.getLaunchResources()`.
        let resources = indexmap::IndexMap::from([("copper".to_owned(), 100)]);
        let total = launch_total(&requirements, &resources);
        let copper = requirements.get("copper").copied().unwrap_or(0);
        assert_eq!(
            total.get("copper").copied(),
            Some(copper + 100),
            "schematic copper requirement + staged resources"
        );
        for (item, amount) in &requirements {
            if item != "copper" {
                assert_eq!(
                    total.get(item),
                    Some(amount),
                    "`{item}` requirement charged"
                );
            }
        }
    }

    /// PAR-0509: `PlanetDialog.java:1405` picks the launcher's best core only
    /// when the destination allows launch schematics; otherwise the planet
    /// default core is forced (`LaunchLoadoutDialog` then renders no grid).
    /// Serpulo allows schematics (`Planets.java:138`), Erekir does not.
    #[test]
    fn launch_core_follows_destination_allow_launch_schematics() {
        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        assert!(
            require(registry.planet_by_name("serpulo"), "serpulo").allow_launch_schematics,
            "Serpulo allows launch schematics"
        );
        assert!(
            !require(registry.planet_by_name("erekir"), "erekir").allow_launch_schematics,
            "Erekir keeps the default `allowLaunchSchematics = false`"
        );
        // Schematics allowed: the launcher's `bestCoreType` wins.
        assert_eq!(
            launch_core_name(&registry, "core-shard", "core-nucleus", true),
            "core-nucleus"
        );
        // Destination forbids schematics: always the planet default core, even
        // when the launcher's best core is larger.
        assert_eq!(
            launch_core_name(&registry, "core-bastion", "core-nucleus", false),
            "core-bastion"
        );
        // `SectorInfo.bestCoreType` defaults to a non-core block ("air");
        // Java's `instanceof CoreBlock` check falls back to the default core.
        assert_eq!(
            launch_core_name(&registry, "core-shard", "air", true),
            "core-shard"
        );
        assert_eq!(
            launch_core_name(&registry, "core-bastion", "", true),
            "core-bastion"
        );
    }

    /// `LaunchLoadoutDialog.java:171-176`: a loadout whose blocks cannot be
    /// built on the launcher's planet is not offered. Uses a Serpulo tech-tree
    /// block (`copper-wall` -> `shownPlanets = [serpulo]`): buildable on
    /// Serpulo, not on Erekir (`Block.isOnPlanet`).
    #[test]
    fn launch_grid_skips_blocks_not_buildable_on_the_planet() {
        let mut registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        // `shownPlanets` is assigned by the `init`/`postInit` sweeps
        // (`Planet.init` -> `TechTree.addPlanet`, `Block.postInit`), which the
        // live client boot runs after `create_base_content`.
        require_ok(registry.init(), "content init");
        require_ok(registry.post_init(), "content postInit");
        let copper_wall = require(registry.block_id("copper-wall"), "copper-wall");
        let serpulo_id = require(registry.planet_by_name("serpulo"), "serpulo").id;
        assert!(
            registry
                .block(copper_wall)
                .is_some_and(|def| def.unlock.shown_planets.contains(&serpulo_id)),
            "copper-wall is registered to Serpulo's tech tree"
        );
        let schematic = mind_core::game::schematic::Schematic {
            tiles: vec![mind_core::game::schematic::Stile::new(
                copper_wall,
                0,
                0,
                Default::default(),
                0,
            )],
            labels: Vec::new(),
            tags: Default::default(),
            width: 1,
            height: 1,
            file: None,
            mod_name: None,
        };
        let serpulo = require(registry.planet_by_name("serpulo"), "serpulo");
        let erekir = require(registry.planet_by_name("erekir"), "erekir");
        let sun = registry.planet_by_name("sun").map(|planet| planet.id);
        let serpulo_env = mind_core::game::planet::env_mask(&serpulo.default_env);
        let erekir_env = mind_core::game::planet::env_mask(&erekir.default_env);
        assert!(
            schematic_placeable_on_planet(&schematic, &registry, serpulo.id, serpulo_env, sun),
            "a Serpulo block is offered on Serpulo"
        );
        assert!(
            !schematic_placeable_on_planet(&schematic, &registry, erekir.id, erekir_env, sun),
            "a Serpulo-only block is not offered on Erekir"
        );
    }
}

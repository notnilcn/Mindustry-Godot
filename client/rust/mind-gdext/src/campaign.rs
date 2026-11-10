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
use events::install_event_listeners;
use service::load_service;

mod accelerator;
mod cache;
mod database;
mod events;
mod hud;
mod launch;
mod lifecycle;
mod map_play;
mod objectives;
mod planet_view;
mod pump;
mod research;
mod rules;
mod runtime;
mod saves;
mod schematics;
mod sector_launch;
mod sectors;
mod service;
mod text;

#[cfg(test)]
mod test_util;

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

    /// Emitted on the first capture of a planet's last sector (campaign
    /// victory); the UI opens `campaign_complete`.
    #[signal]
    fn campaign_complete();

    /// Emitted when an unlock performed during play includes a block
    /// (`UnlockEvent`; `J/ui/fragments/PlacementFragment.java:92-96` rebuilds
    /// the palette). Never emitted while a host bind is held.
    #[signal]
    fn block_unlocked();
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

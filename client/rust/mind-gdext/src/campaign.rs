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
    PackedInt32Array, PackedStringArray, PackedVector3Array, VarDictionary, Vector3,
};
use godot::classes::notify::NodeNotification;
use godot::classes::{FileAccess, INode, Node, ProjectSettings, Texture2D, Time};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::assets::bundle::Bundle;
use mind_core::content::registries::planets::GeneratorKind;
use mind_core::content::tech::{ObjectiveSpec, TechNodeRef};
use mind_core::content::{
    BlockId, ContentRef, ContentRegistry, ContentType, ItemId, LiquidId, MemoryBundle, PlanetId,
    SectorId,
};
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
use mind_core::game::runtime::{CampaignRuntime, UnlockRef};
use mind_core::game::saves::{
    DEFAULT_SAVE_INTERVAL_SECONDS, Saves, SavesUpdate, SavesUpdateResult,
};
use mind_core::game::schematics::Schematics;
use mind_core::game::tech_tree::{self, SettingsUnlockStore};
use mind_core::game::universe::{Campaign, TurnContext, launch_plan};
use mind_core::game::waves::WaveMapHooks;
use mind_core::game::world_reloader::HostReloader;
use mind_core::io::FileSystem;
use mind_core::io::fs::{NativeFs, Paths};
use mind_core::io::json::quote_bare_keys;
use mind_core::io::json::rules::GameStats;
use mind_core::io::save::slot::list_save_slots as list_slot_files;
use mind_core::io::settings::{KEY_SAVE_INTERVAL, SettingsStore};
use mind_core::io::{SaveIo, SaveMeta};
use mind_core::random::JavaRandom;
use mind_core::render::g3d::grid::PlanetGrid;
use mind_core::render::g3d::mesh_data::build_planet_grid;
use mind_core::ui::content_stats::{Cell, StatCtx, content_stats as compute_content_stats};
use mind_core::world::modules::ItemModule;

use crate::settings_hub;
use crate::sim_host::MindSimHost;

/// `PlanetRenderer.outlineRad`: the sector-grid shell scale over `radius`.
const OUTLINE_RAD: f32 = 1.17;

/// Off-play-sector inventory rows of `ResearchDialog.rebuildItems`
/// (`Vec<(sector, rows)>`).
type SectorInventory = Vec<(SectorRef, Vec<(String, i32)>)>;

/// `MindCampaign` — campaign/sector/rules/tech/schematic facade for MCP + UI.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindCampaign {
    base: Base<Node>,
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
}

#[godot_api]
impl INode for MindCampaign {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
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
            last_pump_tick: 0,
            saving_finish_tick: None,
            unlocked_since_last_take: Vec::new(),
            playtesting_map: None,
            current_map_name: None,
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
        // `Control.dispose`: save the current autosave on application dispose.
        self.run_exit_save();
        self.persist();
    }
}

/// `state.rules.sector.preset.description` for the paused dialog's
/// `@objective` entry (`PausedDialog.java:76-80`).
///
/// The campaign registry boots with an empty `MemoryBundle`, so
/// `UnlockableContent.description` is usually unset; fall back to the live
/// bundle's `sector.<preset>.description` key (`UnlockableContent.java:91`
/// loads it through `Core.bundle`).
fn resolve_preset_description(
    registry: Option<&ContentRegistry>,
    bundle: &Bundle,
    preset: Option<SectorId>,
) -> String {
    let Some(def) = preset.and_then(|preset| registry.and_then(|registry| registry.sector(preset)))
    else {
        return String::new();
    };
    def.unlock
        .description
        .clone()
        .filter(|description| !description.is_empty())
        .or_else(|| {
            bundle
                .get_or_null(&format!("sector.{}.description", def.name))
                .map(str::to_owned)
        })
        .unwrap_or_default()
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
        // Campaign state lives in the same settings store the sim host writes
        // (`user://config/settings.bin`), so unlocks/rules survive restarts.
        let root = godot::classes::ProjectSettings::singleton()
            .globalize_path("user://")
            .to_string();
        self.paths = Paths::new(root);
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

    /// `Core.scene.hasDialog()` for the `MindUi` dialog stack.
    fn ui_has_dialog(&self) -> bool {
        let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") else {
            return false;
        };
        if !ui.has_method("has_dialog") {
            return false;
        }
        ui.call("has_dialog", &[]).try_to::<bool>().unwrap_or(false)
    }

    /// Loads the schematic library (`Schematics.load`: vanilla loadouts plus
    /// every `.msch` under the data dir's `schematics/`; Java's
    /// `schematicDirectory` is `dataDirectory/schematics`, which the profile
    /// import extracts).
    fn load_schematics(&self, registry: &ContentRegistry) -> Schematics {
        let mut schematics = Schematics::new();
        let dir = self.paths.root().join("schematics");
        if let Err(error) = schematics.load(&NativeFs, &dir, registry) {
            log::warn!(
                "failed to load schematics from `{}`: {error}",
                dir.display()
            );
        }
        schematics
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

    /// Persists campaign/universe/unlock state to `user://config/settings.bin`.
    ///
    /// Mirrors the live runtime campaign first: while a sector runs the runtime
    /// copy is authoritative, so writing the facade's last-mirrored copy would
    /// reset the turn counter and clock (Java has one object graph:
    /// `Vars.universe`/`planet.sectors`; `Universe.save()` `Universe.java:293`).
    fn persist(&mut self) {
        self.mirror_runtime_campaign();
        if let Some(campaign) = self.campaign.as_mut()
            && let Err(error) = settings_hub::with_settings(|settings| campaign.save_all(settings))
        {
            log::warn!("campaign save_all failed: {error}");
        }
        if let Err(error) = settings_hub::save(&self.paths) {
            log::warn!("campaign settings flush failed: {error}");
        }
    }

    /// Runs `f` on THE live campaign: the runtime's copy while a sector runs
    /// (then mirrors it into the facade), else the facade. Java has one
    /// object graph (`Vars.universe` / `planet.sectors`); the facade is a view
    /// that catches up, never a second copy (H04 T1).
    ///
    /// `f` runs with the runtime taken out of the host, so it must not call
    /// back into Godot (H-environment §3: no held `bind_mut` across calls).
    /// It receives the one shared settings store (`Core.settings`, PAR-0164);
    /// the closure must not call back into the hub (the lock is not re-entrant).
    fn with_campaign_mut<R>(
        &mut self,
        f: impl FnOnce(&mut Campaign, &mut SettingsStore) -> R,
    ) -> Option<R> {
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                let out =
                    settings_hub::with_settings(|settings| f(&mut runtime.campaign, settings));
                self.campaign = Some(runtime.campaign.clone());
                host.bind_mut().put_campaign_runtime(runtime);
                return Some(out);
            }
        }
        let mut campaign = self.campaign.take()?;
        let out = settings_hub::with_settings(|settings| f(&mut campaign, settings));
        self.campaign = Some(campaign);
        Some(out)
    }

    /// Copies the runtime campaign into the facade (no-op without a runtime).
    /// Facade -> runtime copies never happen (T1: the runtime stays
    /// authoritative until it is dropped).
    fn mirror_runtime_campaign(&mut self) {
        let Some(mut host) = self.host() else {
            return;
        };
        let taken = host.bind_mut().take_campaign_runtime();
        if let Some(runtime) = taken {
            self.campaign = Some(runtime.campaign.clone());
            host.bind_mut().put_campaign_runtime(runtime);
        }
    }

    /// Pumps the live campaign runtime: drains tick events, applies capture
    /// bookkeeping (stats, auto-unlocks, capture replacements), advances the
    /// `Saves` policy (playtime + autosave cadence), runs due campaign turns
    /// and saves. No-op without a host/runtime.
    fn pump_runtime(&mut self) {
        let Some(mut host) = self.host() else {
            return;
        };
        let Some(mut runtime) = host.bind_mut().take_campaign_runtime() else {
            return;
        };
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
            // in settings next to the autosave (PAR-0153).
            self.persist_sector_record_in(Some(&mut runtime));
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
                        host.bind_mut()
                            .request_save(GString::from(file.to_string_lossy().as_ref()), false);
                        self.saving_finish_tick = Some(tick.saturating_add(3));
                        log::info!("capture save queued (`{}`)", file.display());
                    }
                    self.persist_sector_record_in(Some(&mut runtime));
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
                        host.bind_mut()
                            .request_save(GString::from(file.to_string_lossy().as_ref()), false);
                        self.saving_finish_tick = Some(tick.saturating_add(3));
                        log::info!("game-over save queued (`{}`)", file.display());
                    }
                    // `SaveVersion.writeMeta` persists the sector record next to
                    // the save (spawn position + prepared core state), which the
                    // lost-sector replay reads on the re-attack.
                    self.persist_sector_record_in(Some(&mut runtime));
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

    /// Primes the host's sector-generation extras from the campaign record
    /// (`Sector.threat`/`hasEnemyBase`/`hasBase`) so the next `load_sector` of
    /// a numbered sector generates with the live campaign state (H05 T2).
    /// Call before `MindSimHost.load_sector` for the same planet+sector.
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
        let Some(mut host) = self.host() else {
            return false;
        };
        host.bind_mut()
            .set_sector_gen_extras_rs(&planet_name, sector, extras);
        true
    }

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
        self.current_map_name = None;
        let planet_name = planet.to_string();
        if sector < 0 {
            return false;
        }
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
        let has_save = record.has_save();
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
            let mut runtime = CampaignRuntime::new(self.session.clone(), campaign.clone());
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
            && let Some(mut camera) = self.base().get_node_or_null("/root/Spine/World/Camera2D")
        {
            camera.call(
                "show_landing",
                &[
                    core_x.to_variant(),
                    core_y.to_variant(),
                    core_block.to_variant(),
                ],
            );
            // G07 T7: `Trigger.newGame` audio — `coreLand.at(camera)` +
            // `Musics.land` (`Control.java:198-222`).
            if let Some(mut audio) = self.base().get_node_or_null("/root/Spine/MindAudio") {
                audio.call("play_landing", &[]);
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
            let file = self.paths.sector_save(&planet_name, sector_id as u32);
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
                if let Some(mut host) = self.host() {
                    let path = format!("user://saves/{save_name}.msav");
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
        if playtest {
            self.playtesting_map = Some(path);
        } else if settings_hub::with_settings(|store| store.get_bool("savecreate", false)) {
            // `Control.playMap`'s `savecreate` slot (`Control.java:418-420`):
            // `map.name() + " " + new SimpleDateFormat("MMM dd h:mm")`.
            let name = format!("{} {}", header.name(), save_date_stamp());
            if !self.save_slot(GString::from(name.as_str())) {
                log::warn!("start_map_mode: savecreate slot `{name}` failed");
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
            let mut runtime = CampaignRuntime::new(self.session.clone(), campaign);
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
            && let Some(mut camera) = self.base().get_node_or_null("/root/Spine/World/Camera2D")
        {
            camera.call(
                "show_landing",
                &[
                    core_x.to_variant(),
                    core_y.to_variant(),
                    core_block.to_variant(),
                ],
            );
            // G07 T7: `Trigger.newGame` audio — `coreLand.at(camera)` +
            // `Musics.land` (`Control.java:198-222`).
            if let Some(mut audio) = self.base().get_node_or_null("/root/Spine/MindAudio") {
                audio.call("play_landing", &[]);
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

    /// `Planet.applyRules(rules, true)` over a rules JSON (`CustomRulesDialog`'s
    /// planet buttons, `CustomRulesDialog.java:287-306` / `Planet.java:286-306`).
    /// Returns the updated rules JSON (the input unchanged on failure).
    #[func]
    pub fn apply_planet_rules(&mut self, planet: GString, rules_json: GString) -> GString {
        let text = rules_json.to_string();
        let Ok(mut rules) = serde_json::from_str::<Rules>(&text) else {
            return rules_json;
        };
        let Some(registry) = self.registry.as_ref() else {
            return rules_json;
        };
        let Some(def) = registry.planet_by_name(&planet.to_string()) else {
            return rules_json;
        };
        let record = mind_core::game::planet::Planet::from_def(registry, def, 0.0);
        let is_game = self.session.phase.is_game();
        record.apply_rules(registry, &mut rules, true, is_game);
        match serde_json::to_string(&rules) {
            Ok(json) => GString::from(json.as_str()),
            Err(_) => rules_json,
        }
    }

    /// `Planets.<name>.defaultEnv` as a bitmask (`CustomRulesDialog`'s Erekir
    /// checks). `0` for an unknown planet.
    #[func]
    pub fn planet_default_env(&self, planet: GString) -> i64 {
        let Some(registry) = self.registry.as_ref() else {
            return 0;
        };
        registry
            .planet_by_name(&planet.to_string())
            .map(|def| {
                i64::from(mind_core::game::planet::Planet::from_def(registry, def, 0.0).default_env)
            })
            .unwrap_or(0)
    }

    /// `Vars.defaultEnv` (`Vars.java:65`): `terrestrial | spores | groundOil |
    /// groundWater | oxygen` = 233. Used by `@rules.anyenv`.
    #[func]
    pub fn default_env(&self) -> i64 {
        use mind_core::content::EnvFlag;
        i64::from(
            mind_core::game::planet::env_bit(EnvFlag::Terrestrial)
                | mind_core::game::planet::env_bit(EnvFlag::Spores)
                | mind_core::game::planet::env_bit(EnvFlag::GroundOil)
                | mind_core::game::planet::env_bit(EnvFlag::GroundWater)
                | mind_core::game::planet::env_bit(EnvFlag::Oxygen),
        )
    }

    /// Planets the rules dialog may switch to (`content.planets().select(p ->
    /// p.accessible && p.visible && p.isLandable())`,
    /// `CustomRulesDialog.java:287`): `[{name}]` in content order.
    #[func]
    pub fn rules_planets(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        let Some(registry) = self.registry.as_ref() else {
            return out;
        };
        for def in registry.planets() {
            if !def.accessible || !def.visible || def.sectors.is_empty() {
                continue;
            }
            let mut row = VarDictionary::new();
            row.set(&GString::from("name"), &def.name.as_str().to_variant());
            out.push(&row);
        }
        out
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

    /// `UI.showErrorMessage(text)` (`UI.java:495-508`): a modal error prompt.
    /// The port has no dedicated error dialog; `show_text` with the
    /// `@error.title` header is the same shape. The argument is a bundle key
    /// (`@`-prefixed like the Java call sites).
    fn show_error(&self, message: &str) {
        let text = self.bundle.get(message.trim_start_matches('@')).to_owned();
        self.show_error_text(&text);
    }

    /// [`Self::show_error`] with a bundle key formatted against `args`
    /// (`Core.bundle.format`).
    fn show_error_key(&self, key: &str, args: &[&str]) {
        let text = self.bundle.format(key.trim_start_matches('@'), args);
        self.show_error_text(&text);
    }

    fn show_error_text(&self, text: &str) {
        let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") else {
            log::warn!("error shown with no UI available: {text}");
            return;
        };
        let title = self.bundle.get("error.title").to_owned();
        // `Dialog.add(text)` parses markup upstream; the prompt labels take
        // BBCode, so render both through `UI.formatIcons` first.
        let render = |raw: &str| -> String {
            self.base()
                .get_node_or_null("/root/MindAssets")
                .and_then(|mut assets| {
                    assets
                        .call("markup_text", &[GString::from(raw).to_variant()])
                        .try_to::<GString>()
                        .ok()
                })
                .map(|rendered| rendered.to_string())
                .unwrap_or_else(|| raw.to_owned())
        };
        let title = render(&title);
        let text = render(text);
        ui.call(
            "show_text",
            &[
                GString::from(title.as_str()).to_variant(),
                GString::from(text.as_str()).to_variant(),
            ],
        );
    }

    /// `Core.scene` menu restore for an invalid map (`World.loadMap` posts
    /// `state.set(State.menu)`, `World.java:391`).
    fn set_menu_visible(&self, visible: bool) {
        let Some(mut ui_root) = self.base().get_node_or_null("/root/Spine/Ui/UiRoot") else {
            return;
        };
        ui_root.call("set_menu_visible", &[visible.to_variant()]);
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
                let events = sector_capture(&mut runtime.session, &mut runtime.campaign);
                let ok = !events.is_empty();
                for event in events {
                    // `Logic`'s `SectorCaptureEvent` listener (`Logic.java:117-121`):
                    // the captured sector's wave team turns derelict on the next
                    // tick (`Sim::apply_campaign_world_requests`). The debug
                    // capture path must queue it like the game-state path.
                    if matches!(
                        event,
                        mind_core::game::play::PlayEvent::SectorCapture { .. }
                    ) {
                        runtime.pending_derelict_team = Some(runtime.session.rules.wave_team);
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
            let planet_name = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.planet(*planet).map(|planet| planet.name.clone()))
                .unwrap_or_default();
            let name = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(*planet, *sector))
                .map(|record| sector_toast_name(record, &planet_name, *sector))
                .unwrap_or_else(|| format!("{planet_name}-{sector}"));
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

    /// Live `{turn, seconds}` of the universe (T1 seam; `Universe.java:293-296`).
    ///
    /// Reads the runtime copy while a sector is installed, else the facade
    /// (`"{}"` with no campaign).
    #[func]
    pub fn universe_json(&self) -> GString {
        let mut value = self.campaign.as_ref().map(|campaign| {
            serde_json::json!({
                "turn": campaign.universe.turn,
                "seconds": campaign.universe.seconds,
            })
        });
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(runtime) = &taken {
                value = Some(serde_json::json!({
                    "turn": runtime.campaign.universe.turn,
                    "seconds": runtime.campaign.universe.seconds,
                }));
            }
            if let Some(runtime) = taken {
                host.bind_mut().put_campaign_runtime(runtime);
            }
        }
        GString::from(
            value
                .map(|value| value.to_string())
                .unwrap_or_else(|| "{}".to_owned())
                .as_str(),
        )
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
    ///
    /// `PausedDialog`'s live edit (`PausedDialog.java:20-37`):
    /// `rulesDialog.hidden(() -> { Vars.state.rules = toEdit; Call.setRules(toEdit); })`.
    /// While a match runs the runtime session is authoritative (`pump_runtime`
    /// mirrors it back), so the accepted rules are written into the live
    /// `CampaignRuntime` and its world state is re-derived
    /// (`CampaignRuntime.rules_world_dirty`).
    #[func]
    pub fn set_rules_json(&mut self, json: GString) -> bool {
        // `JsonIo::read` coerces whole floats (`1.0` -> `1`), so a rules blob
        // round-tripped through GDScript's JSON parses like Arc's reader.
        let incoming = match mind_core::io::json::JsonIo::read::<Rules>(&json.to_string()) {
            Ok(rules) => rules,
            Err(error) => {
                log::warn!("set_rules_json: rules JSON did not parse: {error}");
                return false;
            }
        };
        let is_campaign = self.session.is_campaign();
        if let Err(error) = apply_set_rules(
            &mut self.session.rules,
            incoming.clone(),
            is_campaign,
            None,
            &mut self.epoch,
        ) {
            log::warn!("set_rules_json: rejected: {error}");
            return false;
        }
        // Mirror into the runtime copy (`Call.setRules`); the facade session is
        // refreshed by the next pump.
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                let runtime_is_campaign = runtime.session.is_campaign();
                let accepted = match apply_set_rules(
                    &mut runtime.session.rules,
                    incoming,
                    runtime_is_campaign,
                    None,
                    &mut self.epoch,
                ) {
                    Ok(_) => true,
                    Err(error) => {
                        log::warn!("set_rules_json: runtime rejected: {error}");
                        false
                    }
                };
                if accepted {
                    runtime.rules_world_dirty = true;
                }
                host.bind_mut().put_campaign_runtime(runtime);
                return accepted;
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

    /// Emits an `unlock` for the toast when the content `showUnlock()`s.
    ///
    /// `Control.java:160-165` (`UnlockEvent` -> `hudfrag.showUnlock`); the
    /// default is true (`UnlockableContent.java:242`) and `StatusEffect` is the
    /// only `false` override (`StatusEffect.java:140`), so status unlocks are
    /// skipped. No HUD present (tests) is a no-op.
    fn push_unlock_toast(&self, registry: &ContentRegistry, content: ContentRef) {
        if content.type_ == ContentType::Status {
            return;
        }
        let Some(name) = tech_tree::content_name(registry, content) else {
            return;
        };
        let Some(mut hud) = self.base().get_node_or_null("/root/MindHud") else {
            return;
        };
        if hud.has_method("push_unlock") {
            hud.call("push_unlock", &[GString::from(name.as_str()).to_variant()]);
        }
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
                    .map(|content| format!("{}-{}-ui", content.type_.name(), node.content_name))
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
                    .map(|content| format!("{}-{}-ui", content.type_.name(), node.content_name))
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
                .map(|_| format!("{type_name}-{}-ui", node.content_name))
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

    /// `<type>.<name>.name` through the live bundle, falling back to `name`.
    fn localized_content_name(&self, type_name: &str, name: &str) -> String {
        let key = format!("{type_name}.{name}.name");
        let value = self.bundle.get(&key);
        if value == key {
            name.to_owned()
        } else {
            value.to_owned()
        }
    }

    /// `<type>.<name>.description` (empty when the bundle has no entry).
    fn localized_content_description(&self, type_name: &str, name: &str) -> String {
        self.localized_content_field(type_name, name, "description")
    }

    /// `<type>.<name>.<field>` through the live bundle (empty when missing).
    fn localized_content_field(&self, type_name: &str, name: &str, field: &str) -> String {
        let key = format!("{type_name}.{name}.{field}");
        let value = self.bundle.get(&key);
        if value == key {
            String::new()
        } else {
            value.to_owned()
        }
    }

    /// `UnlockableContent.emoji()` through the `MindAssets` `Iconc` table
    /// (`Fonts.getUnicodeStr`); empty when the content has no icon.
    fn content_emoji(&self, name: &str) -> String {
        let Some(mut assets) = self.base().get_node_or_null("/root/MindAssets") else {
            return String::new();
        };
        assets
            .call("unicode_str", &[GString::from(name).to_variant()])
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    /// `Objectives.Research.display()` half: `content.techNode == null ||
    /// parent == null || parent.content.unlockedHost()`.
    fn research_parent_unlocked(&self, registry: &ContentRegistry, content: ContentRef) -> bool {
        let Some(node) = registry
            .tech()
            .nodes
            .iter()
            .find(|node| node.content == Some(content))
        else {
            return true;
        };
        let Some(parent_ref) = node.parent else {
            return true;
        };
        registry
            .tech()
            .node(parent_ref)
            .and_then(|parent| parent.content)
            .is_none_or(|parent_content| tech_tree::content_unlocked(registry, parent_content))
    }

    /// `Content.emoji() + " " + Content.localizedName` (no leading space when
    /// the emoji is unknown).
    fn emoji_content_name(&self, registry: &ContentRegistry, content: ContentRef) -> String {
        let name = tech_tree::content_name(registry, content).unwrap_or_default();
        let emoji = self.content_emoji(&name);
        let localized = self.localized_content_name(content.type_.name(), &name);
        if emoji.is_empty() {
            localized
        } else {
            format!("{emoji} {localized}")
        }
    }

    /// `SectorPreset.localizedName`.
    fn sector_localized_name(&self, registry: &ContentRegistry, id: SectorId) -> String {
        let name = registry
            .sector(id)
            .map(|record| record.name.clone())
            .unwrap_or_default();
        self.localized_content_name("sector", &name)
    }

    /// Localized preset name for a planet-local sector index (H03 T17: Java's
    /// sector-lost toast passes `state.getSector().name()`, `Control.java`).
    /// Falls back to the internal preset name / an empty string.
    #[func]
    pub fn sector_name_localized(&self, planet: GString, sector: i64) -> GString {
        let Some(registry) = self.registry.as_ref() else {
            return GString::new();
        };
        let Some(campaign) = self.campaign.as_ref() else {
            return GString::new();
        };
        let Ok(sector) = u16::try_from(sector) else {
            return GString::new();
        };
        let Some(planet_id) = registry.planet_id(&planet.to_string()) else {
            return GString::new();
        };
        let Some(cell) = campaign.sector(planet_id, sector) else {
            return GString::new();
        };
        let name = match cell.preset {
            Some(preset) => self.sector_localized_name(registry, preset),
            None => cell.preset_name.clone().unwrap_or_default(),
        };
        GString::from(name.as_str())
    }

    /// `Planet.localizedName`.
    fn planet_localized_name(&self, registry: &ContentRegistry, id: PlanetId) -> String {
        let name = registry
            .planet(id)
            .map(|record| record.name.clone())
            .unwrap_or_default();
        self.localized_content_name("planet", &name)
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

    /// Copies the facade campaign into the installed runtime (post-research /
    /// rule edits).
    /// Refreshes the live runtime's unlocked-content cache from the registry
    /// after research/clear flows. The campaign itself is never copied
    /// facade -> runtime (T1: the runtime copy is authoritative while a sector
    /// runs; mutations go through [`Self::with_campaign_mut`]).
    fn push_campaign_to_runtime(&mut self) {
        let unlocked = self
            .registry
            .as_ref()
            .map(collect_unlocked)
            .unwrap_or_default();
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                runtime.set_unlocked_content(unlocked);
                host.bind_mut().put_campaign_runtime(runtime);
            }
        }
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

    /// `ContentInfoDialog.show(content)`: name/description/details plus the
    /// computed stat categories for the `?` dialog (`UnlockableContent
    /// .computeStats` -> `world/meta/Stats`). Unknown names return `{}` so the
    /// caller can keep its bundle-only fallback.
    #[func]
    pub fn content_info_json(&self, name: GString) -> GString {
        let Some(registry) = self.registry.as_ref() else {
            return GString::from("{}");
        };
        let name = name.to_string();
        let Some(content) = registry.by_name(&name) else {
            return GString::from("{}");
        };
        let type_name = content.type_.name();
        // The family stats read the runtime block table (kind data + behavior
        // knobs). It is rebuilt per call — cheap next to dialog construction and
        // never stale across a registry reload (H05 T10b).
        let block_table = mind_core::world::block::BlockTable::build_default(registry).ok();
        let ctx = StatCtx {
            registry,
            bundle: &self.bundle,
            blocks: block_table.as_ref(),
            in_game: self.session.phase.is_game(),
            planet: Some(self.session.rules.planet.as_str()),
            campaign: self.session.is_campaign(),
        };
        let stats = compute_content_stats(&ctx, content);
        let categories: Vec<serde_json::Value> = stats
            .to_map()
            .into_iter()
            .filter_map(|(cat, rows)| {
                let stat_rows: Vec<serde_json::Value> = rows
                    .into_iter()
                    .map(|(stat, values)| {
                        let rendered: Vec<serde_json::Value> = values
                            .iter()
                            .map(|cells| render_stat_cells(cells))
                            .collect();
                        serde_json::json!({
                            "name": stat.name(),
                            "label": ctx.stat_label(stat),
                            "tooltip": ctx.stat_tooltip(stat),
                            "values": rendered,
                        })
                    })
                    .collect();
                (!stat_rows.is_empty()).then(|| {
                    serde_json::json!({
                        "name": cat.name(),
                        "label": ctx.category_label(cat),
                        "stats": stat_rows,
                    })
                })
            })
            .collect();
        let hide_details = match content.type_ {
            ContentType::Block => registry
                .block(BlockId::new(content.id))
                .is_some_and(|def| def.unlock.hide_details),
            ContentType::Item => registry
                .item(ItemId::new(content.id))
                .is_some_and(|def| def.unlock.hide_details),
            ContentType::Liquid => registry
                .liquid(LiquidId::new(content.id))
                .is_some_and(|def| def.unlock.hide_details),
            _ => false,
        };
        let view = serde_json::json!({
            "content": name,
            "type": type_name,
            "localized": self.localized_content_name(type_name, &name),
            "icon": format!("{type_name}-{name}-ui"),
            "description": self.localized_content_description(type_name, &name),
            "details": self.localized_content_field(type_name, &name, "details"),
            "credit": self.localized_content_field(type_name, &name, "credit"),
            "use_categories": stats.use_categories,
            "locked": !tech_tree::content_unlocked_with_rules(
                registry,
                content,
                &self.session.rules
            ),
            "hide_details": hide_details,
            "categories": categories,
        });
        GString::from(view.to_string().as_str())
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
    /// Built over the live session registry — the one content boot filled,
    /// mods included — because Java reads the global `Vars.content.blocks()`
    /// (`PlacementFragment.java:771-781`); the base-content helpers are only
    /// the no-session fallback. Campaign games filter through the
    /// settings-backed unlock store: only blocks whose `<name>-unlocked` bit is
    /// set (or that `check_auto_unlocks` grants at boot) and whose
    /// `BuildVisibility` is buildable survive, empty categories are dropped,
    /// and other-planet content is hidden (`Block.environmentBuildable` reads
    /// the active planet's `shownPlanets`). Custom games expose the full
    /// build-menu inventory, mirroring `unlockedNowHost`'s
    /// `!state.is_campaign()` branch, with the same planet filter.
    ///
    /// The campaign planet comes from the LOADED world's rules when one exists:
    /// Java's `state.rules` is the loaded world's from `World.loadSector` on,
    /// while the facade session only flips in `start_sector` — which the UI can
    /// call a frame after the world load, so `WorldLoadEvent`'s posted rebuild
    /// (`PlacementFragment.java:78-84`) would otherwise read the old planet.
    #[func]
    pub fn block_catalog_json(&mut self) -> GString {
        // Read the loaded rules into one owned string; never hold the host
        // bind across the catalog build below.
        let loaded_planet = self.host().and_then(|host| {
            host.bind()
                .last_sector_rules()
                .map(|rules| rules.planet.clone())
        });
        let planet = loaded_planet.unwrap_or_else(|| self.session.rules.planet.clone());
        let is_campaign = self.session.is_campaign();
        let catalog = match self.registry.as_ref() {
            Some(registry) => {
                if is_campaign {
                    settings_hub::with_settings(|settings| {
                        let store = SettingsUnlockStore::new(settings);
                        mind_core::ui::campaign::block_catalog_from(
                            registry,
                            Some(&store),
                            Some(planet.as_str()),
                        )
                    })
                } else {
                    mind_core::ui::campaign::block_catalog_from(
                        registry,
                        None,
                        Some(planet.as_str()),
                    )
                }
            }
            None => {
                if is_campaign {
                    settings_hub::with_settings(|settings| {
                        let store = SettingsUnlockStore::new(settings);
                        mind_core::ui::campaign::block_catalog_unlocked_for(&store, planet.as_str())
                    })
                } else {
                    mind_core::ui::campaign::block_catalog_for(planet.as_str())
                }
            }
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

    /// Core Database view JSON (`DatabaseDialog` planet tabs + content grid).
    ///
    /// Built from the live campaign registry so the menu and campaign show the
    /// settings-backed locked/unlocked entries; a custom (non-campaign,
    /// non-menu) game forces everything unlocked (`DatabaseDialog.unlocked`).
    #[func]
    pub fn database_json(&self) -> GString {
        let Some(registry) = self.registry.as_ref() else {
            return GString::from("{}");
        };
        let unlocked_override = !self.session.is_campaign() && self.session.phase.is_game();
        // `Block.isBanned()`: `state.isGame() && rules.bannedBlocks.contains`
        // (`DatabaseDialog.rebuild` draws the cross and sorts banned last).
        let banned = self
            .session
            .phase
            .is_game()
            .then_some(&self.session.rules.banned_blocks);
        let view = mind_core::ui::database::database_view(registry, unlocked_override, banned);
        GString::from(
            serde_json::to_string(&view)
                .unwrap_or_else(|_| String::from("{}"))
                .as_str(),
        )
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
            let saved = list_slot_files(&NativeFs, &self.save_paths());
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

    /// Per-objective state rows (`index`, `type`, `completed`, `countup`).
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
            row.set(
                &GString::from("countup"),
                &self.session.objectives.timer_countup(index).to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Debug seam: `{completed, countup}` straight from the live runtime
    /// executor (the facade mirror is stale while the frame pump is paused).
    #[func]
    pub fn objective_runtime_debug(&mut self, index: i64) -> GString {
        if index < 0 {
            return GString::from("null");
        }
        let index = index as usize;
        let Some(mut host) = self.host() else {
            return GString::from("null");
        };
        let Some(runtime) = host.bind_mut().take_campaign_runtime() else {
            return GString::from("null");
        };
        let completed = runtime.session.objectives.is_completed(index);
        let countup = runtime.session.objectives.timer_countup(index);
        host.bind_mut().put_campaign_runtime(runtime);
        GString::from(
            serde_json::json!({"completed": completed, "countup": countup})
                .to_string()
                .as_str(),
        )
    }

    /// `MapObjectives` world-marker draw list for the running objectives
    /// (view-only; `MindObjectiveMarkers` projects the positions through the
    /// camera). `ShapeText` markers with the `world` flag off are omitted.
    #[func]
    pub fn objective_markers_json(&self) -> GString {
        let mut rows: Vec<serde_json::Value> = Vec::new();
        for index in self.session.objectives.running_indices() {
            let Some(objective) = self.session.objectives.get(index) else {
                continue;
            };
            for marker in &objective.common().markers {
                let mind_core::io::json::objectives::ObjectiveMarker::ShapeText(marker) = marker
                else {
                    continue;
                };
                if marker.world == 0 {
                    continue;
                }
                rows.push(serde_json::json!({
                    "class": "ShapeText",
                    "x": marker.pos.x,
                    "y": marker.pos.y,
                    "text": self.localized_marker_text(&marker.text),
                    "radius": marker.radius,
                    "rotation": marker.rotation,
                    "sides": marker.sides,
                    "color": [
                        marker.color.0.r,
                        marker.color.0.g,
                        marker.color.0.b,
                        marker.color.0.a,
                    ],
                    "textHeight": marker.text_height,
                    "fontSize": marker.font_size,
                }));
            }
        }
        GString::from(serde_json::Value::Array(rows).to_string().as_str())
    }

    /// `ShapeTextMarker.fetchText`: a leading `@` resolves through the live
    /// bundle; Arc markup and `:icon:` tokens are stripped for the plain world
    /// label (atlas-backed icons arrive with the label pass follow-up).
    fn localized_marker_text(&self, raw: &str) -> String {
        let resolved = match raw.trim().strip_prefix('@') {
            Some(key) => {
                let value = self.bundle.get(key);
                if value.is_empty() || value == key {
                    raw.to_owned()
                } else {
                    value.to_owned()
                }
            }
            None => raw.to_owned(),
        };
        // Arc markup and `:icon:` tokens stay in: the world-label pass draws
        // them as coloured runs with content-icon glyphs (`WorldLabel`).
        resolved
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

    /// Fog query placeholder (fog state is plan 12 M7 `FogControl`, not yet
    /// bound to this facade); reports the rules fog flags. The live probe is
    /// `MindSimHost.fog_debug_json` (this facade does not own the `Sim`).
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

    /// The `index`-th loaded schematic (`list_schematics` order) for
    /// `MindInput.use_schematic` (G08 T6b, consumed by G05 T10b).
    pub fn schematic_at(&self, index: i32) -> Option<mind_core::game::schematic::Schematic> {
        let registry = self.registry.as_ref()?;
        self.load_schematics(registry)
            .all
            .get(index.max(0) as usize)
            .cloned()
    }

    /// Lists the decoded schematics (vanilla loadouts + `schematics/*.msch`).
    #[func]
    pub fn list_schematics(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        let Some(registry) = &self.registry else {
            return out;
        };
        let schematics = self.load_schematics(registry);
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
        // With a live player unit the tiles queue as construction plans
        // (`InputHandler` placement path); otherwise they place instantly
        // (editor/preview worlds, T18).
        let player_plans = host.player_can_build();
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
            placed |= if player_plans {
                host.queue_player_place_at(tile_x, tile_y, stile.block, stile.rotation)
            } else {
                host.place_block_at(tile_x, tile_y, stile.block, stile.rotation)
            };
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
        let total_playtime = self.saves.total_playtime;
        host.bind_mut().set_save_playtime(total_playtime);
        // `Saves.SaveSlot.save` -> `savePreview()`: the slot's preview PNG,
        // written from the live grid before the queued save lands at the IO
        // boundary (`Saves.java:343-375`).
        host.bind_mut()
            .write_save_preview(GString::from(slot.as_str()));
        host.bind_mut().request_save(
            GString::from(format!("user://saves/{slot}.msav").as_str()),
            false,
        );
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
        host.bind_mut().request_load(GString::from(path.as_str()));
        true
    }

    /// `Saves.getTotalPlaytime` (session playtime in milliseconds).
    #[func]
    pub fn get_total_playtime(&self) -> i64 {
        self.saves.get_total_playtime()
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
            host.bind_mut()
                .request_save(GString::from(file.to_string_lossy().as_ref()), false);
            host.bind_mut().flush_io();
            queued = true;
            log::info!("saved on exit (`{}`)", file.display());
        }
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
        }
        self.persist();
        queued
    }

    /// `run_exit_save` / capture bookkeeping: writes the live sector's stored
    /// record (`SectorInfo.prepare` + `Sector.saveInfo`) so captures, wave
    /// progress, item pools and the production windows survive a client restart
    /// (PAR-0081/PAR-0082).
    fn persist_sector_record(&mut self) {
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                self.persist_sector_record_in(Some(&mut runtime));
                host.bind_mut().put_campaign_runtime(runtime);
                return;
            }
        }
        self.persist_sector_record_in(None);
    }

    /// `persist_sector_record` body. `runtime` is the already-taken live
    /// runtime when the caller holds it (`pump_runtime` events); with `None`
    /// the host runtime (when installed) is taken for the read and put back.
    fn persist_sector_record_in(&mut self, mut runtime: Option<&mut CampaignRuntime>) {
        let Some((planet, sector_id)) = self.session.sector else {
            return;
        };
        let Some(planet_name) = self
            .registry
            .as_ref()
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
                self.registry
                    .as_ref()
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
            Ok(_) => GString::new(),
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

    /// Writes one `CampaignRules` field for a planet and persists it
    /// (`campaign_rules_dialog.gd`'s `set_campaign_rule(planet, key, value)`;
    /// Java `PlanetDialog.rules`). Re-applies the planet rules on the live
    /// session when the edited planet is the one being played.
    #[func]
    pub fn set_campaign_rule(&mut self, planet: GString, key: GString, value: Variant) -> bool {
        let planet_name = planet.to_string();
        let key = key.to_string();
        let Some(campaign) = self.campaign.as_ref() else {
            return false;
        };
        let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
            return false;
        };
        let Some(record) = campaign.planet(planet_id) else {
            return false;
        };
        let Ok(mut json) = serde_json::to_value(&record.campaign_rules) else {
            return false;
        };
        let Some(object) = json.as_object_mut() else {
            return false;
        };
        let value = if let Ok(flag) = value.try_to::<bool>() {
            serde_json::Value::Bool(flag)
        } else if let Ok(number) = value.try_to::<f64>() {
            serde_json::Value::from(number)
        } else if let Ok(text) = value.try_to::<GString>() {
            serde_json::Value::String(text.to_string())
        } else {
            log::warn!("set_campaign_rule: unsupported value type for `{key}`");
            return false;
        };
        object.insert(key.clone(), value);
        let Ok(rules) = serde_json::from_value::<CampaignRules>(json) else {
            return false;
        };
        let Some(updated) = self.with_campaign_mut(|campaign, _settings| {
            let Some(record) = campaign.planet_mut(planet_id) else {
                return false;
            };
            record.campaign_rules = rules.clone();
            true
        }) else {
            return false;
        };
        if !updated {
            return false;
        }
        // Re-apply on the played planet (`Planet.applyRules(state.rules)`).
        if self
            .session
            .sector
            .is_some_and(|(played, _)| played == planet_id)
            && let Some(registry) = self.registry.take()
        {
            let applied = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.planet(planet_id))
                .map(|record| record.apply_rules(&registry, &mut self.session.rules, false, true));
            if let Some(applied) = applied {
                if applied.rts_swapped {
                    log::info!("campaign rule `{key}` triggered the RTS-AI swap");
                }
                if let Some(mut host) = self.host() {
                    let taken = host.bind_mut().take_campaign_runtime();
                    if let Some(mut runtime) = taken {
                        runtime.session.rules = self.session.rules.clone();
                        host.bind_mut().put_campaign_runtime(runtime);
                    }
                }
            }
            self.registry = Some(registry);
        }
        self.persist();
        true
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
    ///
    /// T1: the planet record is written on the live campaign; the runtime's
    /// session rules are updated in a separate statement. The runtime campaign
    /// is never replaced by the facade snapshot.
    #[func]
    pub fn set_campaign_rules_json(&mut self, json: GString) -> bool {
        let Ok(rules) = serde_json::from_str::<CampaignRules>(&json.to_string()) else {
            return false;
        };
        let Some((planet_id, _)) = self.session.sector else {
            return false;
        };
        let Some(registry) = self.registry.take() else {
            return false;
        };
        let Some(found) = self.with_campaign_mut(|campaign, _settings| {
            let Some(planet_record) = campaign.planet_mut(planet_id) else {
                return false;
            };
            planet_record.campaign_rules = rules;
            true
        }) else {
            self.registry = Some(registry);
            return false;
        };
        if !found {
            self.registry = Some(registry);
            return false;
        }
        // `PlanetDialog.rules` -> `Planet.applyRules(state.rules)`
        // (`Planet.java:308-309`); the facade mirrors the live campaign now.
        let applied = self
            .campaign
            .as_ref()
            .and_then(|campaign| campaign.planet(planet_id))
            .map(|planet_record| {
                planet_record.apply_rules(&registry, &mut self.session.rules, false, true)
            });
        let Some(applied) = applied else {
            self.registry = Some(registry);
            return false;
        };
        if applied.rts_swapped {
            log::info!("campaign rules RTS-AI swap (controller reset is plan-11)");
        }
        // Mirror into the live runtime session; `runtime.campaign` stays as-is.
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                runtime.session.rules = self.session.rules.clone();
                host.bind_mut().put_campaign_runtime(runtime);
            }
        }
        self.registry = Some(registry);
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
            destination_preset,
            launcher_core_name,
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
            let from_name = launcher_sector
                .info
                .info
                .name
                .clone()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| {
                    launcher_sector.preset_name.clone().unwrap_or_else(|| {
                        if planet_record.sectors.len() == 1 {
                            planet_record.name.clone()
                        } else {
                            launcher_sector.id.to_string()
                        }
                    })
                });
            (
                launcher_sector.items().clone(),
                destination_sector.allow_launch_loadout(planet_def.allow_launch_loadout),
                destination_sector.preset,
                launcher_sector.info.info.best_core_type.clone(),
                from_name,
                launcher_sector.preset_name.clone(),
            )
        };

        let schematics = self.load_schematics(registry);
        let core = registry
            .block_id(&launcher_core_name)
            .or_else(|| registry.block_id("core-shard"));
        let default_index = core
            .and_then(|core| schematics.get_default_loadout(core))
            .or_else(|| core.and_then(|core| schematics.get_loadouts(core).first().copied()));
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
        let requirement_seq = selected.requirements(registry);
        let requirements: indexmap::IndexMap<String, i32> = registry
            .items()
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let amount = requirement_seq.get(ItemId::new(index as u16));
                (amount != 0).then(|| (item.name.clone(), amount))
            })
            .collect();

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

        let mut total = requirements.clone();
        for (item, amount) in &planned {
            *total.entry(item.clone()).or_insert(0) += *amount;
        }
        // `LaunchLoadoutDialog` schematic grid (`entry.key.size <= core.size`):
        // every loadout whose core is no larger than the launcher's core, in
        // `Schematics.all` order (the dialog renders these as row buttons).
        let core_size = core
            .and_then(|core| registry.block(core))
            .map(|block| block.size)
            .unwrap_or(3);
        let mut loadout_rows: Vec<serde_json::Value> = Vec::new();
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
            loadout_rows.push(serde_json::json!({
                "index": index,
                "name": loadout.name(),
                "core": def.name,
                "is_default": schematics.get_default_loadout(loadout_core) == Some(index),
                "file": loadout.file,
            }));
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
        let value = serde_json::json!({
            "from": from_name,
            "from_preset": from_preset,
            "capacity": capacity,
            "allow_launch_loadout": destination_allow,
            "stored": nonzero(&sitems),
            "requirements": nonzero(&requirements),
            "resources": nonzero(&planned),
            "max": max,
            "valid": valid,
            "total": nonzero(&total),
            "preset_loadout": nonzero(&preset_loadout),
            "loadouts": loadout_rows,
            "selected": selected_index,
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
        // `Planet.maxZoom = 2f` (`Planet.java:56`); `Planet.getLastSector`
        // (`Planet.java:312-317`) reads `<name>-last-sector` through
        // `Core.settings`, clamped to the grid.
        out.set("min_zoom", def.min_zoom);
        out.set("max_zoom", 2.0f32);
        let last_sector = settings_hub::with_settings(|settings| {
            settings
                .get_i32(
                    &format!("{}-last-sector", def.name),
                    def.start_sector as i32,
                )
                .clamp(0, def.sector_count.saturating_sub(1) as i32)
        });
        out.set("last_sector", last_sector as i64);
        out.set(
            "icon_color",
            Color::from_rgba(icon.r, icon.g, icon.b, icon.a),
        );
        // `PlanetRenderer.drawAtmosphere` uniforms: radii are
        // `planet.radius + atmosphereRad{In,Out}`.
        let atmosphere = def.atmosphere_color;
        let mut atmos = VarDictionary::new();
        atmos.set("has", def.has_atmosphere);
        atmos.set(
            "color",
            Color::from_rgba(atmosphere.r, atmosphere.g, atmosphere.b, atmosphere.a),
        );
        atmos.set("rad_in", def.atmosphere_rad_in);
        atmos.set("rad_out", def.atmosphere_rad_out);
        out.set("atmosphere", &atmos);
        out.set("tiles", &tiles);
        out.set("corners", &corners);
        out.set("tile_corners", &tile_corners);
        out.set("lines", &lines);
        out.set("presets", &presets);
        out
    }

    /// Hex planet mesh for the globe (`HexMesh.java:11`:
    /// `MeshBuilder.buildHex(planet.generator, 6, planet.radius, 0.2f)`):
    /// vertices, flat normals, per-tile colors (`Block.mapColor` with
    /// `1 - albedo` alpha) and the emissive channel (`CUSTOM0`, `isEmissive()`
    /// planets only). Empty for planets without a hex generator.
    #[func]
    pub fn planet_mesh(&self, planet: GString) -> VarDictionary {
        use mind_core::maps::generators::planet::HexMesher;

        let mut out = VarDictionary::new();
        let name = planet.to_string();
        let Some(registry) = self.registry.as_ref() else {
            return out;
        };
        let Some(def) = registry.planet_by_name(&name) else {
            return out;
        };
        let mut mesher: Box<dyn HexMesher> = match name.as_str() {
            "serpulo" => {
                let mut generator = mind_core::maps::planet::SerpuloPlanetGenerator::new();
                generator.load_content(registry, &crate::render::minimap::load_block_colors());
                Box::new(generator)
            }
            "erekir" => {
                let mut generator = mind_core::maps::planet::ErekirPlanetGenerator::new();
                generator.load_content(registry, &crate::render::minimap::load_block_colors());
                Box::new(generator)
            }
            _ => return out,
        };
        let emissive = mesher.is_emissive();
        let mesh =
            mind_core::render::g3d::mesh_data::build_hex(mesher.as_mut(), 6, def.radius, 0.2);
        let mut vertices = PackedVector3Array::new();
        for vertex in &mesh.vertices {
            vertices.push(Vector3::new(vertex[0], vertex[1], vertex[2]));
        }
        let mut normals = PackedVector3Array::new();
        for normal in &mesh.normals {
            normals.push(Vector3::new(normal[0], normal[1], normal[2]));
        }
        let color_of = |packed: u32| {
            Color::from_rgba8(
                (packed >> 24) as u8,
                (packed >> 16) as u8,
                (packed >> 8) as u8,
                packed as u8,
            )
        };
        let mut colors = PackedColorArray::new();
        for color in &mesh.colors {
            colors.push(color_of(*color));
        }
        let mut emissive_colors = PackedColorArray::new();
        for color in &mesh.emissive {
            emissive_colors.push(color_of(*color));
        }
        let mut indices = PackedInt32Array::new();
        for index in &mesh.indices {
            indices.push(*index as i32);
        }
        out.set("indexed", mesh.indexed);
        out.set("emissive", emissive);
        out.set("vertices", &vertices);
        out.set("normals", &normals);
        out.set("colors", &colors);
        out.set("emissive_colors", &emissive_colors);
        out.set("indices", &indices);
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
        self.saves = Saves::new();
        self.session = PlaySession::default();
        self.runtime_installed = false;
        self.content_error = None;
    }

    /// `GameOverDialog` stats rows (`state.stats`, G03 T16):
    /// `{"wave", "unitsCreated", "enemiesDestroyed", "built", "destroyed",
    /// "deconstructed", "waves"}` in Java label order.
    #[func]
    pub fn game_stats_json(&self) -> GString {
        let stats = &self.session.stats;
        let value = serde_json::json!({
            "wave": stats.waves_lasted,
            "unitsCreated": stats.units_created,
            "enemiesDestroyed": stats.enemy_units_destroyed,
            "built": stats.buildings_built,
            "destroyed": stats.buildings_destroyed,
            "deconstructed": stats.buildings_deconstructed,
            "waves": self.session.rules.waves,
            // `stats.coreItemCount` keyed by item name (`CoreItemObjective.text()`).
            "coreItems": stats.core_item_count,
        });
        GString::from(value.to_string().as_str())
    }

    /// `state.isCampaign()`: a live campaign sector is loaded (the same
    /// condition `get_hud_state` reports as `campaign`). The J/N toggles read
    /// it (`DesktopInput.java:446-461`).
    #[func]
    pub fn campaign_active(&self) -> bool {
        self.session.is_campaign()
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
        // `Logic.isWaitingWave` (`Logic.java:621-624`); G03 reads it for
        // `wave.waveInProgress`.
        dict.set(
            &key("waitingWave"),
            &mind_core::game::play::is_waiting_wave(&self.session).to_variant(),
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
fn sanitize_slot_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect()
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

/// Renders one stat value's cells for the `content_info_json` payload: Text
/// cells pass through `UI.formatIcons` (`render_markup`), every other cell
/// serializes as its tagged JSON form (`Cell::k`).
fn render_stat_cells(cells: &[Cell]) -> serde_json::Value {
    let rendered: Vec<serde_json::Value> = cells
        .iter()
        .map(|cell| match cell {
            Cell::Text { t } => serde_json::json!({ "k": "text", "t": t }),
            other => serde_json::to_value(other).unwrap_or(serde_json::Value::Null),
        })
        .collect();
    serde_json::Value::Array(rendered)
}

/// `Sector.name()` (`Sector.java:243-245`): the stored display name, else the
/// preset name, else `<planet>-<id>`.
fn sector_toast_name(
    record: &mind_core::game::sector::Sector,
    planet_name: &str,
    id: u16,
) -> String {
    if let Some(name) = &record.info.info.name {
        return name.clone();
    }
    if let Some(preset) = &record.preset_name {
        return preset.clone();
    }
    format!("{planet_name}-{id}")
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

/// Launch payload split: the chosen schematic index (`Schematics.all`, `None`
/// for the items-only fallback) and the `stacks` array JSON for
/// `MindSimHost.place_launch_loadout`.
/// Parses the launcher launch payload (`PlanetDialog` `playSelected`):
/// `(schematic index, origin sector id, stacks JSON)`.
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
    use mind_core::assets::bundle::parse_properties;
    use mind_core::content::{MemoryUnlockStore, create_base_content};

    /// EV-0063: the registry boots with an empty `MemoryBundle`, so the paused
    /// dialog's `@objective` gate must resolve the description from the live
    /// `Core.bundle` key (`sector.<preset>.description`).
    #[test]
    fn preset_description_falls_back_to_the_live_bundle() {
        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        let Some(preset) = registry.sector_by_name("groundZero") else {
            panic!("groundZero preset missing");
        };
        assert!(
            preset.unlock.description.is_none(),
            "empty MemoryBundle must leave the registry description unset"
        );
        let properties = parse_properties("sector.groundZero.description = Defend the sector.");
        let bundle = Bundle::from_layers(vec![properties]);
        assert_eq!(
            resolve_preset_description(Some(&registry), &bundle, Some(preset.id)),
            "Defend the sector."
        );
    }

    /// A registry built with the description keeps precedence over the bundle.
    #[test]
    fn preset_description_prefers_the_registry_value() {
        let content =
            MemoryBundle::with_pairs([("sector.groundZero.description", "Registry text")]);
        let registry = match create_base_content(&content, &MemoryUnlockStore::new(), true) {
            Ok(registry) => registry,
            Err(error) => panic!("base content: {error}"),
        };
        let Some(preset) = registry.sector_by_name("groundZero") else {
            panic!("groundZero preset missing");
        };
        let bundle = Bundle::from_layers(vec![parse_properties(
            "sector.groundZero.description = Bundle text",
        )]);
        assert_eq!(
            resolve_preset_description(Some(&registry), &bundle, Some(preset.id)),
            "Registry text"
        );
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
}

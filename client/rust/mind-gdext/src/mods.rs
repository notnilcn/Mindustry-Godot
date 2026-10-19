// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindMods` autoload (plan 20 M6/§3.9): the GDScript-callable mod service.
//!
//! Discovery/metadata/dependencies live in `mind_core::mods`; this node is the
//! thin Godot shell that scans `user://mods`, persists enable flags and exposes
//! the `list/import/set_enabled/remove/requires_reload/errors/mod_strings` API
//! plus `mods_changed`/`mod_error` signals for the plan-14 dialogs.
//!
//! The `ModsDialog`/`ModBrowserDialog` widgets themselves are plan 14 (not
//! landed); the browser's HTTP + `.zip` download stay in GDScript. In-engine
//! MCP verification is deferred to the orchestrator's single-editor mutex.

use std::path::PathBuf;

use godot::classes::notify::NodeNotification;
use godot::classes::{INode, Json, Node as GdNode, ProjectSettings};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::io::{NativeFs, Paths};
use mind_core::mods::assets::{DataAsset, DataAssetData, DataAssetType, ModDataManager};
use mind_core::mods::provider::{ModBootContent, ModContentFailure};
use mind_core::mods::{ModListEntry, ModListing, Mods};
use mind_core::version::BuildInfo;

use crate::settings_hub;

/// Mods directory in Godot user space (`<data>/mods/`).
const MODS_PATH: &str = "user://mods";

/// The mod service autoload.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindMods {
    base: Base<GdNode>,
    mods: Mods,
    /// `GameState.data` (`core/GameState.java:47`, PAR-0394): the single owner
    /// of map/mod data assets (patches, content, bundles, images, audio).
    data: ModDataManager,
    /// `bootstrap()` ran at least once (autoloads `_ready` after scene init).
    bootstrapped: bool,
}

#[godot_api]
impl INode for MindMods {
    fn init(base: Base<GdNode>) -> Self {
        MindMods {
            base,
            mods: Mods::new(false, PathBuf::new()),
            data: ModDataManager::new(),
            bootstrapped: false,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }
}

#[godot_api]
impl MindMods {
    /// Rebuilds the mod registry (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`). The `Mods`
    /// instance is replaced before loading, so a re-run never duplicates.
    fn bootstrap(&mut self) {
        let dir = mods_dir();
        if let Err(error) = std::fs::create_dir_all(&dir) {
            log::warn!("[mods] could not create `{}`: {error}", dir.display());
        }
        self.mods = Mods::new(false, dir.clone());
        // `Vars.checkLaunch` -> `Mods.failedToLaunch`: a crashed previous launch
        // disables mod loading for this boot (`Mods.java:509-511`, applied in
        // `loadMod` at `:1222-1227`).
        self.mods
            .set_failed_to_launch(crate::platform::failed_to_launch());
        let load =
            settings_hub::with_settings(|settings| self.mods.load(&NativeFs, &dir, settings));
        match load {
            Ok(report) => {
                log::info!("mods loaded: {} mod(s)", report.mods.len());
            }
            Err(error) => log::warn!("[mods] load failed: {error}"),
        }
        // `ClientLauncher.java:170`: `mods.loadScripts()` runs before content
        // creation. The port's `NoScriptHost` (OD1) marks mods with `scripts/`
        // unsupported instead of running them (`Mods.java:803-834`).
        self.mods.load_scripts(&NativeFs);
        // I07 T4/T5: the assets autoload entered first (project.godot order), so
        // its atlas is loaded; merge mod bundles/sprites/sounds now.
        self.push_asset_overlay();
        // PAR-0394: (re)start the data-asset owner for this boot. Map data is
        // replaced whenever a map loads (`load_map_data`).
        self.data = ModDataManager::new();
        self.publish_data_assets();
        self.bootstrapped = true;
    }

    /// Emitted whenever the mod set or its enable flags change.
    #[signal]
    fn mods_changed();

    /// Emitted when a mod reports content errors / unsupported code.
    #[signal]
    fn mod_error(name: GString, details: GString);

    /// Emitted around an import (`path`, `ratio` 0.0→1.0). The Rust import is
    /// synchronous (`Mods::import_mod`); the two endpoints are the honest
    /// progress a local import can report. The browser's streaming download
    /// progress lives in `mod_browser_dialog.gd` (`HTTPRequest`).
    #[signal]
    fn mod_import_progress(path: GString, ratio: f32);

    /// Serialized mod rows (`ModListEntry`), sorted like the dialog list.
    /// `failed` carries `mod-<name>-failed` for the ModsDialog Restore button.
    #[func]
    pub fn list(&self) -> Array<VarDictionary> {
        let failed: Vec<String> = settings_hub::with_settings(|settings| {
            self.mods
                .list()
                .iter()
                .filter(|mod_| mod_.failed(settings))
                .map(|mod_| mod_.name.clone())
                .collect()
        });
        let mut out = Array::<VarDictionary>::new();
        for entry in &self.mods.report().mods {
            let mut row = entry_to_dict(entry);
            row.set(
                &"failed".to_variant(),
                &failed.contains(&entry.name).to_variant(),
            );
            if let Some(mod_) = self.mods.list().iter().find(|m| m.name == entry.name) {
                // `ScriptsUnsupported` / outdated reason text for the dialog row
                // (`ModsDialog.getStateDetails` reads the state reason).
                let reason = mod_
                    .unsupported_reason
                    .as_ref()
                    .map(|reason| reason.message())
                    .unwrap_or("");
                if !reason.is_empty() {
                    row.set(&"reason".to_variant(), &reason.to_variant());
                }
            }
            out.push(&row);
        }
        out
    }

    /// One mod row by internal name (`{}` when absent).
    ///
    /// Named `mod_info` rather than `get`: `get` shadows `Object::get` and
    /// becomes a registration error in godot-rust 0.6.
    #[func]
    pub fn mod_info(&self, name: GString) -> VarDictionary {
        let target = name.to_string();
        self.mods
            .report()
            .mods
            .iter()
            .find(|entry| entry.name == target)
            .map(entry_to_dict)
            .unwrap_or_default()
    }

    /// Sets `mod-<name>-enabled`; sets `requiresReload` for non-disabled mods.
    #[func]
    pub fn set_enabled(&mut self, name: GString, enabled: bool) {
        let target = name.to_string();
        settings_hub::with_settings(|settings| {
            self.mods.set_enabled(&target, enabled, settings);
        });
        self.persist_settings();
        let _ = self.base_mut().emit_signal("mods_changed", &[]);
    }

    /// Imports a `.zip`/`.jar`/folder into the mods directory.
    ///
    /// The browser hands in a `user://modcache/...` path; `NativeFs` only speaks
    /// native paths, so localized Godot paths are globalized first (a native
    /// path from the file chooser passes through unchanged).
    #[func]
    pub fn import_mod(&mut self, path: GString) -> VarDictionary {
        let global = ProjectSettings::singleton().globalize_path(path.to_string().as_str());
        let source = PathBuf::from(global.to_string());
        let _ = self.base_mut().emit_signal(
            "mod_import_progress",
            &[path.to_variant(), 0.0f32.to_variant()],
        );
        let result = settings_hub::with_settings(|settings| {
            self.mods.import_mod(&NativeFs, &source, false, settings)
        });
        self.persist_settings();
        let _ = self.base_mut().emit_signal(
            "mod_import_progress",
            &[path.to_variant(), 1.0f32.to_variant()],
        );
        let mut out = VarDictionary::new();
        match result {
            Ok(_name) => {
                out.set(&"ok".to_variant(), &true.to_variant());
                out.set(
                    &"requiresReload".to_variant(),
                    &self.mods.requires_reload().to_variant(),
                );
                let _ = self.base_mut().emit_signal("mods_changed", &[]);
            }
            Err(error) => {
                out.set(&"ok".to_variant(), &false.to_variant());
                out.set(&"error".to_variant(), &error.to_string().to_variant());
                let _ = self.base_mut().emit_signal(
                    "mod_error",
                    &[path.to_variant(), error.to_string().to_variant()],
                );
            }
        }
        out
    }

    /// Removes a mod from the mods directory.
    #[func]
    pub fn remove_mod(&mut self, name: GString) -> VarDictionary {
        let target = name.to_string();
        let mut out = VarDictionary::new();
        match self.mods.remove_mod(&NativeFs, &target) {
            Ok(()) => {
                out.set(&"ok".to_variant(), &true.to_variant());
                out.set(
                    &"requiresReload".to_variant(),
                    &self.mods.requires_reload().to_variant(),
                );
                let _ = self.base_mut().emit_signal("mods_changed", &[]);
            }
            Err(error) => {
                out.set(&"ok".to_variant(), &false.to_variant());
                out.set(&"error".to_variant(), &error.to_string().to_variant());
            }
        }
        out
    }

    /// Whether a restart is required (`@mod.reloadrequired`).
    #[func]
    pub fn requires_reload(&self) -> bool {
        self.mods.requires_reload()
    }

    /// `Mods.skipModLoading()` (`Mods.java:509-511`): this boot disabled mod
    /// loading because the previous launch failed (`@mods.initfailed`).
    #[func]
    pub fn skipped_mod_loading(&self) -> bool {
        settings_hub::with_settings(|settings| self.mods.skip_mod_loading(settings))
    }

    /// Whether [`MindMods::bootstrap`] ran (`false` while the main scene is
    /// instantiated before the autoloads' `_ready`).
    pub fn is_bootstrapped(&self) -> bool {
        self.bootstrapped
    }

    /// Collects the enabled mods' boot content: JSON content files + patch sets
    /// (`Mods.buildFiles`/`loadModPatches`). The caller caches the result for
    /// the session (mod changes require a restart; Java exits to reload).
    pub fn collect_boot_content(&mut self) -> ModBootContent {
        mind_core::content::boot::collect_boot_content(&mut self.mods, &NativeFs)
    }

    /// Builds the enabled mods' asset overlay (`Mods.packSprites` +
    /// `DataBundleLoader` + sound overrides) and hands it to `MindAssets`
    /// (I07 T4 bundles / T5 sprites+sounds). No-op until the asset autoload has
    /// loaded; `MindSimHost` retries after content boot when autoload order put
    /// `MindAssets` second.
    pub fn push_asset_overlay(&mut self) {
        let Some(mut assets) = crate::content_boot::assets_node() else {
            return;
        };
        if !assets.bind().can_accept_mod_overlay() {
            return;
        }
        let overlay = {
            let handle = &assets;
            let atlas_has = |name: &str| handle.bind().has_region(GString::from(name));
            self.mods.build_overlay(&NativeFs, &atlas_has)
        };
        for warning in &overlay.warnings {
            log::warn!("[mods] {warning}");
        }
        assets.bind_mut().apply_mod_overlay(&overlay);
    }

    // --- data assets (PAR-0394, Java `state.data`) ---

    /// Emitted after the data-asset set (or its applied state) changes.
    #[signal]
    fn data_changed();

    /// The owning `ModDataManager` (`GameState.data`).
    pub fn data(&self) -> &ModDataManager {
        &self.data
    }

    /// Mutable access for the editor facade (mutations must be followed by
    /// [`MindMods::publish_data_assets`]).
    pub fn data_mut(&mut self) -> &mut ModDataManager {
        &mut self.data
    }

    /// Applies a map's `patches` region records (`SaveVersion.readDataPatches`
    /// → `state.data.load(assets)`) and refreshes the live decorators.
    pub fn load_map_data(
        &mut self,
        assets: Vec<DataAsset>,
        registry: &mut mind_core::content::ContentRegistry,
    ) -> Result<(), String> {
        self.data
            .load_assets(assets, registry)
            .map_err(|error| error.to_string())?;
        self.publish_data_assets();
        Ok(())
    }

    /// Pushes the manager's records into the live asset/audio decorators and
    /// notifies the dialogs (`DataManager` apply half, PAR-0394).
    pub fn publish_data_assets(&mut self) {
        if let Some(mut assets) = crate::content_boot::assets_node() {
            let mut assets = assets.bind_mut();
            assets.apply_data_images(&self.data);
            assets.apply_data_bundles(&self.data);
            assets.apply_data_audio(&self.data);
        }
        if let Some(mut audio) = self
            .base()
            .get_node_or_null("/root/Spine/MindAudio")
            .and_then(|node| node.try_cast::<crate::audio::MindAudio>().ok())
        {
            audio.bind_mut().apply_data_audio(&self.data);
        }
        let _ = self.base_mut().emit_signal("data_changed", &[]);
    }

    /// Data-asset rows for `MapAssetsDialog` (all types, manager order).
    ///
    /// Fields per row: `kind`, `type` (folder), `name`, `path`, `embedded`,
    /// `size`, `data` (patch/content JSON or bundle properties text),
    /// `index` (patch order), `contentType`, `region`, `found`, `width`,
    /// `height`, `generated`, `id`, `streaming`.
    #[func]
    pub fn data_assets(&self) -> Array<VarDictionary> {
        let assets = crate::content_boot::assets_node();
        let assets = assets.as_ref().map(|node| node.bind());
        let probe = |name: &str| -> Option<(bool, i64, i64)> {
            let assets = assets.as_ref()?;
            let payload = assets.probe(GString::from(name));
            let found = payload
                .get("found")
                .and_then(|value| value.try_to::<bool>().ok())
                .unwrap_or(false);
            let width = payload
                .get("w")
                .and_then(|value| value.try_to::<i64>().ok())
                .unwrap_or(0);
            let height = payload
                .get("h")
                .and_then(|value| value.try_to::<i64>().ok())
                .unwrap_or(0);
            Some((found, width, height))
        };
        let mut out = Array::<VarDictionary>::new();
        for type_ in DataAssetType::ALL {
            let records: Vec<&DataAsset> = if type_ == DataAssetType::Patch {
                // Patch rows follow `patch_records()` order: the dialog's
                // replace/remove actions are index-based.
                let patches = self.data.patch_records();
                patches
                    .iter()
                    .filter_map(|patch| {
                        self.data
                            .all()
                            .iter()
                            .find(|asset| asset.type_ == type_ && asset.path == patch.name)
                    })
                    .collect()
            } else {
                self.data.assets_of(type_)
            };
            for (index, asset) in records.into_iter().enumerate() {
                out.push(&data_row(&self.data, asset, index, &probe));
            }
        }
        out
    }

    /// Content parse/load errors from the last data reload.
    #[func]
    pub fn data_errors(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for error in self.data.content_errors() {
            out.push(&GString::from(error.as_str()));
        }
        out
    }

    /// Number of `dp-`/`net-` regions the decorator registered (probe).
    #[func]
    pub fn data_region_count(&self) -> i64 {
        crate::content_boot::assets_node()
            .map(|node| node.bind().data_region_count())
            .unwrap_or(0)
    }

    /// Records per-file content failures on the owning mods
    /// (`Mods.handleContentError` -> `LoadedMod.erroredContent`) and logs them.
    pub fn record_content_failures(&mut self, failures: &[ModContentFailure]) {
        for failure in failures {
            log::warn!(
                "[mods] {}: {} ({})",
                failure.mod_name,
                failure.message,
                failure.path
            );
        }
        let mut counts: Vec<(String, usize)> = Vec::new();
        for failure in failures {
            match counts
                .iter_mut()
                .find(|(name, _)| name == &failure.mod_name)
            {
                Some((_, count)) => *count += 1,
                None => counts.push((failure.mod_name.clone(), 1)),
            }
        }
        for (name, count) in counts {
            self.mods.note_content_failures(&name, count);
        }
    }

    /// Whether any enabled mod ships scripts (OD1: surfaced, not run).
    #[func]
    pub fn has_scripts(&self) -> bool {
        self.mods.list().iter().any(|mod_| mod_.has_scripts)
    }

    /// Content/unsupported errors as `{name, details}` rows.
    #[func]
    pub fn errors(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        for mod_ in self.mods.list() {
            let mut details = Vec::new();
            if let Some(reason) = &mod_.unsupported_reason {
                details.push(reason.message().to_owned());
            }
            if !mod_.errored_content.is_empty() {
                details.push(format!("{} content error(s)", mod_.errored_content.len()));
            }
            if details.is_empty() {
                continue;
            }
            let mut row = VarDictionary::new();
            row.set(&"name".to_variant(), &mod_.name.to_variant());
            row.set(&"details".to_variant(), &details.join("\n").to_variant());
            out.push(&row);
        }
        out
    }

    /// `name:version` strings for the plan-21 join handshake.
    #[func]
    pub fn mod_strings(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for value in self.mods.get_mod_strings() {
            out.push(&GString::from(value.as_str()));
        }
        out
    }

    /// Rescans the mods directory (still requires a reload for content).
    /// `Mods.reload` (`Mods.java:787-796`): when a reload is required the client
    /// shows `@mods.reloadexit` and exits; the Godot port quits the tree.
    #[func]
    pub fn reload(&mut self) {
        if !self.mods.requires_reload() {
            return;
        }
        if let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") {
            let _ = ui.call(
                "show_info",
                &[GString::from("@mods.reloadexit").to_variant()],
            );
        }
        log::info!("[mods] exiting to reload mods");
        // Java waits for the info dialog to be hidden (`showInfoOnHidden`);
        // the port's `show_info` is a toast, so quit on the next frame.
        self.base_mut().call_deferred("quit_for_reload", &[]);
    }

    /// Deferred `Core.app.exit()` half of [`MindMods::reload`].
    #[func]
    fn quit_for_reload(&mut self) {
        let mut tree = self.base().get_tree();
        tree.quit();
    }

    /// Globalized mods directory (for the file chooser/import UI).
    #[func]
    pub fn mods_dir(&self) -> GString {
        GString::from(mods_dir().display().to_string().as_str())
    }

    /// Full detail row for the ModsDialog details pane (`showMod`).
    #[func]
    pub fn details(&self, name: GString) -> VarDictionary {
        let target = name.to_string();
        let report = self.mods.report();
        let Some(entry) = report.mods.iter().find(|entry| entry.name == target) else {
            return VarDictionary::new();
        };
        let mut out = entry_to_dict(entry);
        if let Some(mod_) = self.mods.list().iter().find(|mod_| mod_.name == target) {
            out.set(
                &"description".to_variant(),
                &mod_
                    .meta
                    .description
                    .clone()
                    .unwrap_or_default()
                    .to_variant(),
            );
            out.set(
                &"subtitle".to_variant(),
                &mod_.meta.subtitle.clone().unwrap_or_default().to_variant(),
            );
            out.set(
                &"config_folder".to_variant(),
                &mod_
                    .config_folder(self.mods.mod_directory())
                    .display()
                    .to_string()
                    .to_variant(),
            );
            out.set(
                &"file".to_variant(),
                &mod_.file.display().to_string().to_variant(),
            );
            out.set(
                &"repo".to_variant(),
                &mod_.meta.repo.clone().unwrap_or_default().to_variant(),
            );
            out.set(
                &"failed".to_variant(),
                &settings_hub::with_settings(|settings| mod_.failed(settings)).to_variant(),
            );
            let reason = mod_
                .unsupported_reason
                .as_ref()
                .map(|reason| reason.message().to_owned())
                .unwrap_or_default();
            out.set(&"reason".to_variant(), &reason.to_variant());
            out.set(
                &"errored_content".to_variant(),
                &(mod_.errored_content.len() as i64).to_variant(),
            );
            out.set(
                &"soft_dependencies".to_variant(),
                &string_array(&mod_.soft_dependencies),
            );
            out.set(
                &"missing_soft_dependencies".to_variant(),
                &string_array(&mod_.missing_soft_dependencies),
            );
        }
        out
    }

    /// Parses a GitHub mod listing JSON (`ModListing`) into a dictionary.
    ///
    /// The browser fetches the JSON with `HTTPRequest`; parsing/version matching
    /// stay in `mind-core` so the dialog does not re-implement `parseVersion`/
    /// `matchesGameVersion`.
    #[func]
    pub fn parse_listing(&self, json_text: GString) -> VarDictionary {
        let mut out = VarDictionary::new();
        match serde_json::from_str::<ModListing>(&json_text.to_string()) {
            Ok(listing) => {
                out.set(&"ok".to_variant(), &true.to_variant());
                let value = serde_json::to_value(&listing).unwrap_or(serde_json::Value::Null);
                out.set(&"listing".to_variant(), &json_to_dict(&value));
            }
            Err(error) => {
                out.set(&"ok".to_variant(), &false.to_variant());
                out.set(&"error".to_variant(), &error.to_string().to_variant());
            }
        }
        out
    }

    /// `ModListing.getMatchingRelease`: the release matching `build`/`revision`.
    ///
    /// Returns `{found, id, version}`; `found = false` means "use `/latest`".
    #[func]
    pub fn matching_release(
        &self,
        listing_json: GString,
        build: i32,
        revision: i32,
    ) -> VarDictionary {
        let mut out = VarDictionary::new();
        let Ok(listing) = serde_json::from_str::<ModListing>(&listing_json.to_string()) else {
            out.set(&"found".to_variant(), &false.to_variant());
            return out;
        };
        match listing.get_matching_release(build, revision) {
            Some(release) => {
                out.set(&"found".to_variant(), &true.to_variant());
                out.set(&"id".to_variant(), &release.id.to_variant());
                out.set(&"version".to_variant(), &release.version.to_variant());
            }
            None => {
                out.set(&"found".to_variant(), &false.to_variant());
            }
        }
        out
    }

    /// The embedded game build (`Version.build`; `-1` for a custom build).
    ///
    /// The browser passes this to `matching_release` instead of guessing.
    #[func]
    pub fn game_build(&self) -> i32 {
        BuildInfo::embedded().build
    }

    /// The embedded game revision (`Version.revision`).
    #[func]
    pub fn game_revision(&self) -> i32 {
        BuildInfo::embedded().revision as i32
    }

    /// `BuildInfo` fields for the dialog footer / version-mismatch checks.
    #[func]
    pub fn version_info(&self) -> VarDictionary {
        let info = BuildInfo::embedded();
        let mut out = VarDictionary::new();
        out.set(&"build".to_variant(), &info.build.to_variant());
        out.set(
            &"revision".to_variant(),
            &(info.revision as i64).to_variant(),
        );
        out.set(
            &"buildString".to_variant(),
            &info.build_string().to_variant(),
        );
        out.set(&"combined".to_variant(), &info.combined().to_variant());
        out
    }

    /// `Mods.getConfigFolder` path for a mod (`user://mods/<internal-name>`).
    #[func]
    pub fn config_folder(&self, name: GString) -> GString {
        let target = name.to_string();
        self.mods
            .list()
            .iter()
            .find(|mod_| mod_.name == target)
            .map(|mod_| {
                GString::from(
                    mod_.config_folder(self.mods.mod_directory())
                        .display()
                        .to_string()
                        .as_str(),
                )
            })
            .unwrap_or_default()
    }

    /// Flushes the shared settings store after an enable/import write
    /// (`Core.settings` `mod-<name>-enabled` flags; I05 T1).
    fn persist_settings(&self) {
        if let Err(error) = settings_hub::save(&Paths::resolve(None)) {
            log::warn!("[mods] settings save failed: {error}");
        }
    }
}

/// `(found, width, height)` probe result for a `dp-` atlas region.
type RegionProbe<'a> = &'a dyn Fn(&str) -> Option<(bool, i64, i64)>;

/// One `MapAssetsDialog` row for a data asset (`AssetView.build` input).
fn data_row(
    manager: &ModDataManager,
    asset: &DataAsset,
    index: usize,
    probe: RegionProbe<'_>,
) -> VarDictionary {
    let mut row = VarDictionary::new();
    let key = |name: &str| GString::from(name);
    row.set(
        &key("kind"),
        &GString::from(asset.type_.kind()).to_variant(),
    );
    row.set(
        &key("type"),
        &GString::from(asset.type_.folder()).to_variant(),
    );
    row.set(
        &key("name"),
        &GString::from(asset.name.as_str()).to_variant(),
    );
    row.set(
        &key("path"),
        &GString::from(asset.path.as_str()).to_variant(),
    );
    row.set(&key("embedded"), &asset.embedded.to_variant());
    row.set(&key("index"), &(index as i64).to_variant());
    let (data, size) = match &asset.data {
        DataAssetData::Patch(text) => (text.clone(), text.len()),
        DataAssetData::Content(record) => (record.json.clone(), record.json.len()),
        DataAssetData::Bundle(map) => (mind_core::mods::assets::render_properties(map), map.len()),
        DataAssetData::Blob(bytes) => (String::new(), bytes.len()),
    };
    row.set(&key("data"), &GString::from(data.as_str()).to_variant());
    row.set(&key("size"), &(size as i64).to_variant());
    if let DataAssetData::Content(record) = &asset.data {
        row.set(
            &key("contentType"),
            &GString::from(record.type_.name()).to_variant(),
        );
    }
    let mut region = String::new();
    match asset.type_ {
        DataAssetType::Image => {
            if let Some(entry) = manager
                .image_applier()
                .entries()
                .iter()
                .find(|entry| entry.path == asset.path)
            {
                region = entry.name.clone();
                row.set(&key("generated"), &entry.generated.to_variant());
                row.set(&key("scale"), &(entry.scale as f64).to_variant());
            }
        }
        DataAssetType::Sound | DataAssetType::Music => {
            if let Some(entry) = manager
                .audio_applier()
                .entries()
                .iter()
                .find(|entry| entry.path == asset.path)
            {
                row.set(&key("id"), &entry.id.to_variant());
                row.set(&key("streaming"), &entry.streaming.to_variant());
            }
        }
        _ => {}
    }
    if !region.is_empty() {
        row.set(&key("region"), &GString::from(region.as_str()).to_variant());
        let (found, width, height) = probe(&region).unwrap_or((false, 0, 0));
        row.set(&key("found"), &found.to_variant());
        row.set(&key("width"), &width.to_variant());
        row.set(&key("height"), &height.to_variant());
    }
    row
}

/// Packs a `Vec<String>` into a Godot string array.
fn string_array(values: &[String]) -> PackedStringArray {
    let mut out = PackedStringArray::new();
    for value in values {
        out.push(&GString::from(value.as_str()));
    }
    out
}

/// Converts a `serde_json::Value` to a Godot `VarDictionary`.
fn json_to_dict(value: &serde_json::Value) -> VarDictionary {
    let text = serde_json::to_string(value).unwrap_or_else(|_| String::from("{}"));
    Json::parse_string(text.as_str())
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// Converts a `ModListEntry` to a Godot dictionary via its serde JSON form.
fn entry_to_dict(entry: &ModListEntry) -> VarDictionary {
    json_to_dict(&serde_json::to_value(entry).unwrap_or(serde_json::Value::Null))
}

/// Globalizes `user://mods` to a native path.
fn mods_dir() -> PathBuf {
    PathBuf::from(
        ProjectSettings::singleton()
            .globalize_path(MODS_PATH)
            .to_string(),
    )
}

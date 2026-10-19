// SPDX-License-Identifier: GPL-3.0-only

//! Live-client content boot (I07 T4, PAR-0115 part 1): the one registry builder
//! every facade calls, so content ids agree across `MindSimHost`,
//! `MindCampaign` and `MindEditor`.
//!
//! Java order (`ClientLauncher.java:166-189`): `createBaseContent` ->
//! `mods.loadScripts` (already done by `MindMods`) -> `createModContent` ->
//! `content.init()`/`load()` -> `mods.loadModPatches`. The pure sequencing lives
//! in [`mind_core::content::boot`]; this wrapper resolves the enabled mods from
//! the `MindMods` autoload once per session (mod changes require a restart:
//! Java `Mods.reload` exits the game) and routes failures back to it.

use std::sync::OnceLock;

use godot::classes::{Engine, SceneTree};
use godot::obj::Singleton;
use godot::prelude::*;

use mind_core::content::ContentRegistry;
use mind_core::content::boot::{ModBootReport, build_registry_with_mods};
use mind_core::content::bundle::BundleView;
use mind_core::content::settings_store::UnlockStore;
use mind_core::mods::provider::ModBootContent;

use crate::mods::MindMods;

/// Session cache of the enabled mods' boot data (`Mods` never changes without a
/// restart; Java exits to reload).
fn session_boot() -> ModBootContent {
    static BOOT: OnceLock<ModBootContent> = OnceLock::new();
    if let Some(cached) = BOOT.get() {
        return cached.clone();
    }
    let Some(mut mods) = mods_node() else {
        log::warn!("[content] MindMods autoload missing; booting base content only");
        return ModBootContent::default();
    };
    if !mods.bind().is_bootstrapped() {
        // The scene can be instantiated before the autoloads' `_ready`; never
        // cache that empty result (the later callers get the real mod set).
        log::warn!("[content] MindMods not bootstrapped yet; booting base content only");
        return ModBootContent::default();
    }
    let boot = mods.bind_mut().collect_boot_content();
    log::info!(
        "[content] mod boot: {} content file(s), {} patch set(s)",
        boot.files.len(),
        boot.patches.len()
    );
    let _ = BOOT.set(boot.clone());
    boot
}

/// The `MindMods` autoload (`/root/MindMods`).
pub fn mods_node() -> Option<Gd<MindMods>> {
    let main_loop = Engine::singleton().get_main_loop()?;
    let tree = main_loop.try_cast::<SceneTree>().ok()?;
    let root = tree.get_root();
    root.try_get_node_as::<MindMods>("MindMods")
}

/// The `MindAssets` autoload (`/root/MindAssets`).
pub fn assets_node() -> Option<Gd<crate::assets::MindAssets>> {
    let main_loop = Engine::singleton().get_main_loop()?;
    let tree = main_loop.try_cast::<SceneTree>().ok()?;
    let root = tree.get_root();
    root.try_get_node_as::<crate::assets::MindAssets>("MindAssets")
}

/// Pushes the mods' asset overlay to `MindAssets` (I07 T4/T5). Safe to call
/// from any node that does not hold a `MindAssets` bind; caller contexts that
/// do (e.g. `MindMods` inside its own bootstrap) call
/// [`crate::mods::MindMods::push_asset_overlay`] directly.
pub fn push_asset_overlay() {
    if let Some(mut mods) = mods_node() {
        mods.bind_mut().push_asset_overlay();
    }
}

/// Builds a registry: base content, enabled mods' content, `init`/`postInit`,
/// then mod patches, in Java's order. `headless` stays the flag the client
/// sites already passed to `create_base_content` (skips icon/load sweeps only).
pub fn build_registry(
    bundle: &dyn BundleView,
    unlocks: &dyn UnlockStore,
) -> Result<ContentRegistry, String> {
    // `content.loadColors()` (`ClientLauncher.java:168`): every client
    // registry carries the real `Block.mapColor`s (floors included).
    let (registry, report) = build_registry_with_mods(
        bundle,
        unlocks,
        true,
        &session_boot(),
        &crate::render::minimap::load_block_colors(),
    )?;
    apply_report(&report);
    Ok(registry)
}

/// Routes patch warnings to the log and content failures to `MindMods`
/// (`LoadedMod.erroredContent`, the dialog's `@mod.errors`).
fn apply_report(report: &ModBootReport) {
    for warning in &report.patch_warnings {
        log::warn!("[content] {warning}");
    }
    if report.failures.is_empty() {
        return;
    }
    if let Some(mut mods) = mods_node() {
        mods.bind_mut().record_content_failures(&report.failures);
    }
}

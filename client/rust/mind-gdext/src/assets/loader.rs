// SPDX-License-Identifier: GPL-3.0-only

//! Runtime asset location (plan 03 §3.2/§6.1).
//!
//! The packed `assets/` tree lives at the repo root (outside the Godot project
//! `client/`), so `res://` cannot address it directly. [`resolve_assets_dir`]
//! probes `res://assets` first (export/native layout) and falls back to
//! `<project>/../assets` (editor / dev layout). Plan 22 owns shipping the tree
//! into exports; the project setting [`ASSETS_DIR_SETTING`] overrides both.

use godot::classes::{FileAccess, Os, ProjectSettings};
use godot::obj::Singleton;
use godot::prelude::*;

use mind_core::io::fs::Paths;
use mind_core::io::settings::KEY_LOCALE;

/// `ProjectSettings` key overriding the asset root (absolute or `res://`).
pub const ASSETS_DIR_SETTING: &str = "mindustry/assets_dir";

/// `ProjectSettings` key selecting the locale (`en`, `pt_BR`, `default`).
pub const LOCALE_SETTING: &str = "mindustry/locale";

/// Resolves the active locale.
///
/// Ported from `Vars.java:536-555` + `LanguageDialog.java:91-128`: the stored
/// `locale` setting (`Core.settings.getString("locale")`) wins; the registered
/// default `"default"` resolves through the OS locale against the
/// `assets/locales` list (exact match, then same language, else `en`) and the
/// result is stored (`findClosestLocale`). The `mindustry/locale` project
/// setting stays as a test-only override.
pub fn resolve_locale() -> String {
    if let Some(value) = project_override() {
        return value;
    }

    let paths = Paths::resolve(None);
    let stored =
        crate::settings_hub::with_settings(|store| store.get_string(KEY_LOCALE, "default"));
    if stored != "default" && !stored.is_empty() {
        return stored;
    }

    let available = crate::assets::bundle::load_locales(&resolve_assets_dir());
    let os_locale = Os::singleton().get_locale().to_string();
    let closest = mind_core::assets::locale::find_closest_locale(&os_locale, &available);
    crate::settings_hub::with_settings(|store| store.put_string(KEY_LOCALE, &closest));
    if let Err(error) = crate::settings_hub::save(&paths) {
        log::warn!("[assets] locale flush failed: {error}");
    }
    closest
}

/// The test-only `mindustry/locale` override (empty/`default` = no override).
fn project_override() -> Option<String> {
    ProjectSettings::singleton()
        .get_setting(LOCALE_SETTING)
        .try_to::<GString>()
        .ok()
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty() && value != "default")
}

/// Resolves the directory containing `sprites/sprites.atlas.json`.
pub fn resolve_assets_dir() -> String {
    let configured = ProjectSettings::singleton()
        .get_setting(ASSETS_DIR_SETTING)
        .try_to::<GString>()
        .ok()
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty());
    if let Some(dir) = configured
        && atlas_exists(&dir)
    {
        return dir;
    }

    if atlas_exists("res://assets") {
        return String::from("res://assets");
    }

    let project = ProjectSettings::singleton()
        .globalize_path("res://")
        .to_string();
    let dev = format!("{project}../assets");
    if atlas_exists(&dev) {
        return dev;
    }
    // Even when the pack output is absent, return the dev path so the loader
    // logs a precise missing-manifest error (no panic).
    dev
}

fn atlas_exists(dir: &str) -> bool {
    FileAccess::file_exists(&format!("{dir}/sprites/sprites.atlas.json"))
}

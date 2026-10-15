// SPDX-License-Identifier: GPL-3.0-only

//! Runtime asset location (plan 03 §3.2/§6.1).
//!
//! The packed `assets/` tree lives at the repo root (outside the Godot project
//! `client/`), so `res://` cannot address it directly. [`resolve_assets_dir`]
//! probes `res://assets` first (export/native layout) and falls back to
//! `<project>/../assets` (editor / dev layout). Plan 22 owns shipping the tree
//! into exports; the project setting [`ASSETS_DIR_SETTING`] overrides both.

use godot::classes::{FileAccess, ProjectSettings};
use godot::prelude::*;

/// `ProjectSettings` key overriding the asset root (absolute or `res://`).
pub const ASSETS_DIR_SETTING: &str = "mindustry/assets_dir";

/// `ProjectSettings` key selecting the locale (`en`, `pt_BR`, `default`).
pub const LOCALE_SETTING: &str = "mindustry/locale";

/// Resolves the active locale (`mindustry/locale`, default `en`).
pub fn resolve_locale() -> String {
    ProjectSettings::singleton()
        .get_setting(LOCALE_SETTING)
        .try_to::<GString>()
        .ok()
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty() && value != "default")
        .unwrap_or_else(|| String::from("en"))
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

/// Resolves a `res://`/`user://` virtual path to an OS path (absolute strings
/// pass through `ProjectSettings.globalize_path` unchanged).
pub fn globalize_path(path: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(
        ProjectSettings::singleton()
            .globalize_path(path)
            .to_string(),
    )
}

/// The built-in map directory (`<assets>/maps`) as an OS path
/// (`Maps.defaultMapDirectory`).
pub fn assets_maps_dir() -> std::path::PathBuf {
    globalize_path(&resolve_assets_dir()).join("maps")
}

/// The session data root (`user://` globalized) that holds `maps/`,
/// `previews/`, `saves/` and `config/`.
pub fn user_data_root() -> std::path::PathBuf {
    globalize_path("user://")
}

/// Loads the map registry from the built-in folder plus the data root's
/// `maps/` (`Maps.load`, `Maps.java:200-260`).
pub fn load_map_registry(root: &std::path::Path) -> mind_core::maps::Maps {
    use mind_core::io::fs::{FileSystem, NativeFs, Paths};

    let mut maps = mind_core::maps::Maps::new();
    let custom = Paths::new(root).maps();
    let sources = mind_core::maps::MapSources {
        builtin_dir: Some(assets_maps_dir()),
        custom_dir: NativeFs.exists(&custom).then_some(custom),
        use_default_folder: true,
    };
    maps.load(&NativeFs, &sources);
    maps
}

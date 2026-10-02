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

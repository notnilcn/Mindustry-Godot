// SPDX-License-Identifier: GPL-3.0-only

//! Minimal P0 client settings (`user://settings.json`, plan §6.5): selected
//! block only. Plan 04 replaces this with the full `Settings` port; the file
//! shape is `{ "format": 1, "selected_block": "..." }`. Camera zoom is not
//! persisted: upstream `Renderer.targetscale` resets to `Scl.scl(4)` every boot.

use std::path::PathBuf;

use godot::builtin::{GString, VarDictionary};
use godot::classes::{Json, ProjectSettings};
use godot::meta::ToGodot;
use godot::obj::Singleton;

/// Settings file path in Godot user space.
const SETTINGS_PATH: &str = "user://settings.json";
/// P0 settings format (`§6.5`).
const SETTINGS_FORMAT: i64 = 1;

/// Values read back from `settings.json` (all optional; defaults live in code).
#[derive(Debug, Default, Clone)]
pub struct ClientSettings {
    /// Last selected block name.
    pub selected_block: Option<String>,
}

/// Reads `user://settings.json`; `None` when absent or unreadable.
pub fn read() -> Option<ClientSettings> {
    let native = native_path();
    let text = std::fs::read_to_string(&native).ok()?;
    let variant = Json::parse_string(text.as_str());
    let dict = variant.try_to::<VarDictionary>().ok()?;

    let block_key = "selected_block".to_variant();
    let selected_block = dict
        .get(&block_key)
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|name| name.to_string());
    Some(ClientSettings { selected_block })
}

/// Writes `user://settings.json`; `false` when the file cannot be written.
pub fn write(selected_block: &str) -> bool {
    let mut dict = VarDictionary::new();
    let format_key = "format".to_variant();
    let block_key = "selected_block".to_variant();
    dict.set(&format_key, &SETTINGS_FORMAT.to_variant());
    dict.set(&block_key, &selected_block.to_variant());
    let text = Json::stringify(&dict.to_variant());

    let native = native_path();
    if let Some(parent) = native.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return false;
    }
    std::fs::write(native, text.to_string()).is_ok()
}

/// Globalizes the `user://` settings path to a native path.
fn native_path() -> PathBuf {
    PathBuf::from(
        ProjectSettings::singleton()
            .globalize_path(SETTINGS_PATH)
            .to_string(),
    )
}

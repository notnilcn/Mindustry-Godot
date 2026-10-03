// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only
//
//! `BindingState` ↔ plan-04 `SettingsStore` persistence + `user://keybinds.json`
//! (plan 15 §3.1/§6.1). All parity names/defaults live in `mind_core::input`.

use std::path::PathBuf;

use godot::classes::ProjectSettings;
use godot::obj::Singleton;

use mind_core::input::{BindingState, KeyBindTable};
use mind_core::io::SettingsStore;

/// File format version.
const KEYBINDS_FORMAT: u32 = 1;
/// Godot user-space path.
const KEYBINDS_PATH: &str = "user://keybinds.json";

/// Persists `state` into a plan-04 [`SettingsStore`] (the parity path).
pub fn to_settings(state: &BindingState) -> SettingsStore {
    let mut store = SettingsStore::new();
    state.save_to_settings(&mut store);
    store
}

/// Loads a [`BindingState`] from a plan-04 [`SettingsStore`] (defaults survive).
pub fn from_settings(store: &SettingsStore) -> BindingState {
    let mut state = BindingState::new();
    state.load_from_settings(store);
    state
}

/// Writes `user://keybinds.json` via the plan-04 settings values.
pub fn save(state: &BindingState) -> bool {
    let store = to_settings(state);
    let mut bindings = serde_json::Map::new();
    for bind in KeyBindTable::all() {
        let key = format!("keybind.{}", bind.name);
        bindings.insert(
            bind.name.to_owned(),
            serde_json::Value::String(store.get_string(&key, "")),
        );
    }
    let mut root = serde_json::Map::new();
    root.insert(
        "format".to_owned(),
        serde_json::Value::Number(serde_json::Number::from(KEYBINDS_FORMAT)),
    );
    root.insert("bindings".to_owned(), serde_json::Value::Object(bindings));
    let text = serde_json::Value::Object(root).to_string();
    let native = native_path();
    if let Some(parent) = native.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return false;
    }
    std::fs::write(native, text).is_ok()
}

/// Reads `user://keybinds.json`; missing/corrupt files yield upstream defaults.
pub fn load() -> BindingState {
    let native = native_path();
    let Ok(text) = std::fs::read_to_string(&native) else {
        return BindingState::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        log::warn!("keybinds file is corrupt; falling back to defaults");
        return BindingState::new();
    };
    if value.get("format").and_then(serde_json::Value::as_u64) != Some(KEYBINDS_FORMAT as u64) {
        log::warn!("keybinds format unsupported; falling back to defaults");
        return BindingState::new();
    }
    let mut store = SettingsStore::new();
    if let Some(bindings) = value.get("bindings").and_then(|v| v.as_object()) {
        for (name, entry) in bindings {
            store.put_string(
                &format!("keybind.{name}"),
                entry.as_str().unwrap_or_default(),
            );
        }
    }
    from_settings(&store)
}

/// Globalizes `user://keybinds.json` to a native path.
fn native_path() -> PathBuf {
    PathBuf::from(
        ProjectSettings::singleton()
            .globalize_path(KEYBINDS_PATH)
            .to_string(),
    )
}

/// Rebinds a single-key binding by registry name; returns whether it applied.
pub fn rebind(state: &mut BindingState, name: &str, code: &str) -> bool {
    let Some(id) = KeyBindTable::all()
        .iter()
        .position(|entry| entry.name == name)
        .map(|index| index as u16)
    else {
        return false;
    };
    let bind = KeyBindTable::get(id);
    match bind.kind {
        mind_core::input::KeyKind::Key => state.set_key(id, code),
        mind_core::input::KeyKind::Axis => state.set_axis(id, None, Some(code)),
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let mut state = BindingState::new();
        state.set_key(mind_core::input::ids::SELECT, "q");
        let store = to_settings(&state);
        let loaded = from_settings(&store);
        assert_eq!(loaded.name(mind_core::input::ids::SELECT), Some("q"));
        assert_eq!(loaded.name(mind_core::input::ids::BOOST), Some("shiftLeft"));
    }

    #[test]
    fn rebind_by_name() {
        let mut state = BindingState::new();
        assert!(rebind(&mut state, "select", "q"));
        assert!(!rebind(&mut state, "does_not_exist", "q"));
        assert_eq!(state.name(mind_core::input::ids::SELECT), Some("q"));
        assert!(rebind(&mut state, "move_x", "left"));
        assert_eq!(state.name(mind_core::input::ids::MOVE_X), Some("left"));
    }
}

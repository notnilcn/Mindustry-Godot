// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Free helpers shared by the desktop bridge files.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

/// `Math.round(float)` (`floor(x + 0.5)`; Java rounds halves toward +inf).
fn java_round(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

/// `pollInputPlayer` L657-665 plan-drag tile: Java keeps
/// `buildPlanMouseOffsetX = splan.x * tilesize - mouseWorld.x` and each frame
/// sets `x = Math.round((mouseWorld.x + offset) / tilesize)`, which reduces to
/// `origin + round((mouseWorld - grabWorld) / tilesize)`.
pub(super) fn plan_drag_tile(
    origin: (i32, i32),
    grab_world: (f32, f32),
    world: (f32, f32),
) -> (i32, i32) {
    let dx = java_round((world.0 - grab_world.0) / 8.0);
    let dy = java_round((world.1 - grab_world.1) / 8.0);
    (origin.0 + dx, origin.1 + dy)
}

/// `PlanConfigFragment` spec JSON from the block's plan-config options.
///
/// `{title, options: [{name, label}], value}`; the label is the bundle key of
/// the content (`item.<name>.name`, `PlanConfigOption::label`). `None` when the
/// block offers no options (`if(options.isEmpty()) return;`,
/// `PlanConfigFragment.java:58`).
pub(super) fn plan_config_spec_json(
    title: &str,
    options: &[mind_core::world::block::PlanConfigOption],
    value: Option<&str>,
) -> Option<String> {
    if options.is_empty() {
        return None;
    }
    let options: Vec<serde_json::Value> = options
        .iter()
        .map(|option| {
            serde_json::json!({
                "name": option.name,
                "label": option.label(),
            })
        })
        .collect();
    let mut spec = serde_json::json!({
        "title": title,
        "options": options,
    });
    if let Some(value) = value {
        spec["value"] = serde_json::Value::from(value);
    }
    Some(spec.to_string())
}

/// `settings.getBool("console")` (default false, `SettingsMenuDialog.java:566`):
/// the F8 console toggle is gated on it (`ConsoleFragment.java:43`).
pub(super) fn console_setting_enabled() -> bool {
    crate::settings_hub::with_settings(|store| store.get_bool("console", false))
}

/// Parses a selectable-unit snapshot (`[{id,type,x,y,team,commandable}]`).
pub(super) fn parse_selectable_units(json: &str) -> Vec<SelectableUnit> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(list) = value.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| {
            Some(SelectableUnit {
                id: entry.get("id")?.as_i64()? as i32,
                type_id: entry.get("type")?.as_i64()? as i32,
                x: entry.get("x")?.as_f64()? as f32,
                y: entry.get("y")?.as_f64()? as f32,
                team: entry.get("team")?.as_u64()? as u8,
                commandable: entry
                    .get("commandable")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true),
                hit_size: entry
                    .get("hit_size")
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0) as f32,
                select_global: entry
                    .get("select_global")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true),
                payload: entry
                    .get("payload")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
            })
        })
        .collect()
}

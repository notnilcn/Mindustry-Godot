// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/TraceDialog.java.

//! The trace dialog controller: the copyable identity rows and the IP/name
//! history panes.
//! Upstream: ui/dialogs/TraceDialog.java

use godot::classes::control::SizeFlags;
use godot::classes::{DisplayServer, HBoxContainer, VBoxContainer};
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::net::roster::dict_text;
use crate::ui::dialogs::net::util_misc::variant_text;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindTable};

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "traces" => Some(Box::new(TraceDialog)),
        _ => None,
    }
}

/// `Table table = new Table(Tex.clear); table.margin(14)` (`TraceDialog.java:21`).
const TRACE_MARGIN: f32 = 14.0;
/// `float s = 28f` (`TraceDialog.java:29`): the copy-button cell.
const TRACE_COPY_SIZE: f32 = 28.0;
/// `table.pane(...).padLeft(20f)` for the IP/name history lists.
const TRACE_LIST_INDENT: f32 = 20.0;

/// One `TraceDialog` row (`TraceDialog.java:31-63`).
#[derive(Debug, Default, PartialEq, Eq)]
struct TraceFields {
    player: String,
    ip: String,
    locale: String,
    uuid: String,
    modded: bool,
    mobile: bool,
    joined: i64,
    kicked: i64,
    ips: Vec<String>,
    names: Vec<String>,
}

impl TraceFields {
    /// The Java `PlayerInfo` context dictionary (`TraceDialog.show(player, info)`).
    fn from_context(player: &str, info: &VarDictionary) -> Self {
        Self {
            player: player.to_owned(),
            ip: dict_text(info, &["ip"]),
            locale: dict_text(info, &["locale"]),
            uuid: dict_text(info, &["uuid"]),
            modded: bool_value(info.get("modded")),
            mobile: bool_value(info.get("mobile")),
            joined: count_value(info.get("timesJoined")),
            kicked: count_value(info.get("timesKicked")),
            ips: list_strings(info.get("ips")),
            names: list_strings(info.get("names")),
        }
    }
}

/// `MindUi.player_trace_json` (`mind-stdb` `TraceInfo.to_json`) -> dialog rows.
///
/// `PlayerListFragment`'s Trace button reads the roster aggregation instead of
/// asking the server (`AdminAction.trace`); the relay schema has no IP, locale
/// or custom-client columns (roster deviation D2), so those Java rows keep
/// their empty/false defaults. `sessions`/`kicks` feed the joined/kicked
/// counts and `names` the `@trace.names` pane.
fn trace_fields_from_roster(player: &str, trace_json: &str) -> Option<TraceFields> {
    let value = serde_json::from_str::<serde_json::Value>(trace_json).ok()?;
    let object = value.as_object()?;
    let identity = object
        .get("identity")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let sessions = object.get("sessions").and_then(serde_json::Value::as_i64);
    let names: Vec<String> = object
        .get("names")
        .and_then(serde_json::Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(|name| name.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if identity.is_empty() && sessions.is_none() && names.is_empty() {
        return None;
    }
    Some(TraceFields {
        player: player.to_owned(),
        uuid: identity.to_owned(),
        joined: sessions.unwrap_or(0),
        kicked: object
            .get("kicks")
            .and_then(serde_json::Value::as_array)
            .map(|kicks| kicks.len() as i64)
            .unwrap_or(0),
        names,
        ..TraceFields::default()
    })
}

/// `MindUi.open_dialog("traces", ctx)` context: the roster trace payload is
/// passed verbatim (as a string) so [`TraceDialog`] owns the field mapping.
pub(crate) fn trace_context_json(player: &str, trace_json: &str) -> String {
    serde_json::json!({ "player": player, "trace": trace_json }).to_string()
}

/// Player trace info (`TraceDialog`): the copyable player/IP/language/ID rows,
/// the formatted scalar rows and the IP/name history lists.
#[derive(Default)]
pub struct TraceDialog;

impl DialogController for TraceDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@trace");
        dialog.should_pause = false;
        dialog.clear_buttons();
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        let mut table = dialog.content_table();
        table.bind_mut().margin(TRACE_MARGIN);
        let player = dialog.context_string("player").to_string();
        // `PlayerListFragment`'s Trace button passes the relay roster trace
        // (`MindUi.player_trace_json`); a direct Java-style caller passes the
        // `info` `PlayerInfo` dictionary instead.
        let trace_text = dialog.context_string("trace").to_string();
        let fields = match trace_fields_from_roster(&player, &trace_text) {
            Some(fields) => fields,
            None => {
                let info = dialog
                    .context_value("info")
                    .and_then(|value| value.try_to::<VarDictionary>().ok())
                    .unwrap_or_default();
                TraceFields::from_context(&player, &info)
            }
        };
        // `TraceDialog.java:31-45`: one copyable `bundle.format` line each.
        for (key, value) in
            trace_field_rows(&fields.player, &fields.ip, &fields.locale, &fields.uuid)
        {
            trace_copy_row(&mut table, key, &value);
        }
        // `TraceDialog.java:47-50`: the four scalar lines.
        for (key, value) in
            trace_scalar_rows(fields.modded, fields.mobile, fields.joined, fields.kicked)
        {
            let text = kit::tm_format(key, &[value.as_str()]).to_string();
            let label = factory::label(&text);
            table.bind_mut().add(&label).pad(1.0).left();
            table.bind_mut().row();
        }
        // `TraceDialog.java:52-63`: `@trace.ips` / `@trace.names` panes.
        for (key, values) in trace_list_rows(fields.ips, fields.names) {
            let header = factory::label(&kit::t(key).to_string());
            table.bind_mut().add(&header).pad(1.0).left();
            table.bind_mut().row();
            // code-instantiated: the IP/name history is trace data.
            let mut column = VBoxContainer::new_alloc();
            for value in values {
                let label = factory::label(&format!("[lightgray]{value}"));
                column.add_child(&label);
            }
            table
                .bind_mut()
                .add(&column)
                .pad(1.0)
                .pad_left(TRACE_LIST_INDENT)
                .left();
            table.bind_mut().row();
        }
    }
}

/// One `TraceDialog` copy row: `Icon.copySmall` 28x28 then the formatted text.
fn trace_copy_row(table: &mut Gd<MindTable>, key: &str, value: &str) {
    // code-instantiated: trace fields are data-driven from the trace info.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 4);
    let mut button = factory::glyph_button("copy", "@copy", "emptyi");
    button.set_custom_minimum_size(Vector2::new(TRACE_COPY_SIZE, TRACE_COPY_SIZE));
    let value_for_copy = GString::from(value);
    let callable = Callable::from_fn("copy_trace", move |_args: &[&Variant]| {
        DisplayServer::singleton().clipboard_set(&value_for_copy);
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("show_info_fade", &[kit::t("@copied").to_variant()]);
        }
    });
    button.connect("pressed", &callable);
    row.add_child(&button);
    let text = kit::tm_format(key, &[value]).to_string();
    let mut label = factory::label(&text);
    label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    row.add_child(&label);
    table.bind_mut().add(&row).pad(1.0).left();
    table.bind_mut().row();
}

/// The copyable trace lines in Java order (`TraceDialog.java:31-45`):
/// `trace.playername`, `trace.ip`, `trace.language`, `trace.id`.
fn trace_field_rows(
    player: &str,
    ip: &str,
    locale: &str,
    uuid: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("@trace.playername", player.to_string()),
        ("@trace.ip", ip.to_string()),
        ("@trace.language", locale.to_string()),
        ("@trace.id", uuid.to_string()),
    ]
}

/// The scalar trace lines in Java order (`TraceDialog.java:47-50`).
fn trace_scalar_rows(
    modded: bool,
    mobile: bool,
    joined: i64,
    kicked: i64,
) -> Vec<(&'static str, String)> {
    vec![
        ("@trace.modclient", modded.to_string()),
        ("@trace.mobile", mobile.to_string()),
        ("@trace.times.joined", joined.to_string()),
        ("@trace.times.kicked", kicked.to_string()),
    ]
}

/// The IP/name history lists in Java order (`TraceDialog.java:52-63`).
fn trace_list_rows(ips: Vec<String>, names: Vec<String>) -> Vec<(&'static str, Vec<String>)> {
    vec![("@trace.ips", ips), ("@trace.names", names)]
}

/// A `String[]` trace field as text rows.
fn list_strings(value: Option<Variant>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Ok(array) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    array
        .iter_shared()
        .map(|entry| variant_text(&entry))
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// A Java `boolean` trace field (`false` when absent).
fn bool_value(value: Option<Variant>) -> bool {
    match value {
        Some(value) => {
            if let Ok(flag) = value.try_to::<bool>() {
                flag
            } else if let Ok(number) = value.try_to::<i64>() {
                number != 0
            } else {
                variant_text(&value) == "true"
            }
        }
        None => false,
    }
}

/// A Java `int` trace field (`0` when absent).
fn count_value(value: Option<Variant>) -> i64 {
    match value {
        Some(value) => {
            if let Ok(number) = value.try_to::<i64>() {
                number
            } else if let Ok(number) = value.try_to::<f64>() {
                number as i64
            } else {
                0
            }
        }
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn trace_rows_follow_the_java_order() {
        let fields = trace_field_rows("Alice", "1.2.3.4", "en", "u-1");
        let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
        assert_eq!(
            keys,
            [
                "@trace.playername",
                "@trace.ip",
                "@trace.language",
                "@trace.id"
            ]
        );
        assert_eq!(fields[0].1, "Alice");
        assert_eq!(fields[1].1, "1.2.3.4");
        assert_eq!(fields[2].1, "en");
        assert_eq!(fields[3].1, "u-1");
        let scalars = trace_scalar_rows(false, true, 3, 1);
        let values: Vec<&str> = scalars.iter().map(|(_, value)| value.as_str()).collect();
        assert_eq!(values, ["false", "true", "3", "1"]);
    }

    #[test]
    fn trace_lists_read_ips_and_names() {
        let lists = trace_list_rows(
            vec!["1.1.1.1".to_string(), "2.2.2.2".to_string()],
            vec!["Alice".to_string()],
        );
        assert_eq!(lists[0].0, "@trace.ips");
        assert_eq!(lists[0].1, ["1.1.1.1", "2.2.2.2"]);
        assert_eq!(lists[1].0, "@trace.names");
        assert_eq!(lists[1].1, ["Alice"]);
    }

    #[test]
    fn trace_scalars_default_to_java_primitive_values() {
        let scalars = trace_scalar_rows(false, false, 0, 0);
        let values: Vec<&str> = scalars.iter().map(|(_, value)| value.as_str()).collect();
        assert_eq!(values, ["false", "false", "0", "0"]);
    }

    #[test]
    fn roster_trace_maps_into_the_trace_dialog_rows() {
        let trace = r#"{
            "identity": "ab12",
            "names": ["Alice", "alice_old"],
            "sessions": 4,
            "connected": true,
            "lastJoin": 123,
            "banned": true,
            "whitelisted": false,
            "kicks": [{"reason": "grief", "by": "cd", "at": 9}],
            "actions": [{"kind": "kick", "detail": "", "at": 9}]
        }"#;
        let Some(fields) = trace_fields_from_roster("Alice", trace) else {
            panic!("roster trace should map");
        };
        assert_eq!(fields.player, "Alice");
        assert_eq!(fields.uuid, "ab12");
        assert_eq!(fields.joined, 4);
        assert_eq!(fields.kicked, 1);
        assert_eq!(fields.names, ["Alice", "alice_old"]);
        // The relay schema has no IP/locale/custom-client columns (D2).
        assert_eq!(fields.ip, "");
        assert_eq!(fields.locale, "");
        assert!(!fields.modded);
        assert!(!fields.mobile);
        assert!(fields.ips.is_empty());
        // The Java order is preserved when rendered.
        assert_eq!(
            trace_field_rows(&fields.player, &fields.ip, &fields.locale, &fields.uuid)[3],
            ("@trace.id", String::from("ab12"))
        );
    }

    #[test]
    fn malformed_or_empty_trace_falls_back_to_the_java_context() {
        assert!(trace_fields_from_roster("Alice", "not-json").is_none());
        assert!(trace_fields_from_roster("Alice", "{}").is_none());
        assert!(trace_fields_from_roster("Alice", "").is_none());
    }

    #[test]
    fn trace_context_carries_the_player_and_payload() {
        let context = trace_context_json("Alice", r#"{"identity":"ab"}"#);
        let value: serde_json::Value =
            serde_json::from_str(&context).unwrap_or(serde_json::Value::Null);
        assert_eq!(value["player"], "Alice");
        assert_eq!(value["trace"], r#"{"identity":"ab"}"#);
    }
}

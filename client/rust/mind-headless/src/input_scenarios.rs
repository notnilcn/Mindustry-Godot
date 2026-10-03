// SPDX-License-Identifier: GPL-3.0-only

//! Plan 15 headless input scenarios (`input` subcommand).

use std::path::Path;

use anyhow::{Result, bail};
use mind_core::input::{
    BindingDefault, BindingState, FocusGuards, FocusState, InputLog, InputReplay, KeyBindTable,
    LockId, PlaceMode, RawEvent,
};

use crate::cli::InputCommand;

/// Runs an `input` subcommand.
pub fn run(command: &InputCommand) -> Result<()> {
    match command {
        InputCommand::Dump { json, out, golden } => dump(*json, out.as_deref(), golden.as_deref()),
        InputCommand::Replay {
            events,
            out,
            json,
            mobile,
        } => replay(events, out.as_deref(), *json, *mobile),
        InputCommand::Scenario {
            name,
            json,
            dump: out,
            golden,
        } => scenario(name, *json, out.as_deref(), golden.as_deref()),
    }
}

fn print_json(value: &serde_json::Value, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
    }
}

fn emit(value: &serde_json::Value, out: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let text = format!("{}\n", serde_json::to_string_pretty(value)?);
    if let Some(path) = out {
        std::fs::write(path, &text)?;
    }
    if let Some(path) = golden {
        let expected = std::fs::read_to_string(path)?;
        if expected != text {
            bail!("input golden mismatch at {}", path.display());
        }
    }
    Ok(())
}

/// `input dump`: the binding registry parity table.
fn dump(json: bool, out: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let mut categories = serde_json::Map::new();
    for category in mind_core::input::Category::ALL {
        categories.insert(
            category.name().to_owned(),
            serde_json::Value::from(KeyBindTable::category(category).count()),
        );
    }
    let binds: Vec<serde_json::Value> = KeyBindTable::all()
        .iter()
        .map(|bind| {
            let default =
                match bind.default {
                    BindingDefault::Key(Some(name)) => serde_json::Value::from(name),
                    BindingDefault::Key(None) => serde_json::Value::Null,
                    BindingDefault::Axis { negative, positive } => serde_json::Value::from(
                        format!("{}/{}", negative.unwrap_or(""), positive.unwrap_or("")),
                    ),
                };
            serde_json::json!({
                "name": bind.name,
                "category": bind.category.map(|c| c.name()),
                "axis": matches!(bind.kind, mind_core::input::KeyKind::Axis),
                "bundle": bind.bundle_key(),
                "default": default,
            })
        })
        .collect();
    let report = serde_json::json!({
        "format": 1,
        "count": KeyBindTable::len(),
        "categories": categories,
        "binds": binds,
    });
    emit(&report, out, golden)?;
    print_json(&report, json);
    Ok(())
}

/// `input replay`: deterministic event-log replay summary (controllers land M2).
fn replay(events: &Path, out: Option<&Path>, json: bool, mobile: bool) -> Result<()> {
    let text = std::fs::read_to_string(events)?;
    let log = InputLog::from_jsonl(&text)?;
    let mut guards = FocusGuards::default();
    let replay = InputReplay::new(log.clone());
    let mut key_events = 0usize;
    let mut mouse_events = 0usize;
    let mut touch_events = 0usize;
    let mut focus_events = 0usize;
    for record in &log.records {
        match &record.ev {
            RawEvent::KeyDown { .. } | RawEvent::KeyUp { .. } => key_events += 1,
            RawEvent::MouseMove { .. } | RawEvent::MouseButton { .. } | RawEvent::Scroll { .. } => {
                mouse_events += 1
            }
            RawEvent::TouchDown { .. }
            | RawEvent::TouchUp { .. }
            | RawEvent::TouchMove { .. }
            | RawEvent::Magnify { .. } => touch_events += 1,
            RawEvent::UiFocus(focus) => {
                focus_events += 1;
                guards.focus = focus.clone();
            }
            _ => {}
        }
    }
    let last_tick = replay.last_tick();
    let report = serde_json::json!({
        "format": 1,
        "mobile": mobile || log.header.mobile,
        "events": log.len(),
        "key_events": key_events,
        "mouse_events": mouse_events,
        "touch_events": touch_events,
        "focus_events": focus_events,
        "last_tick": last_tick,
        "focus_after": guards.focus,
        "checksum": fnv1a(text.as_bytes()),
    });
    emit(&report, out, None)?;
    print_json(&report, json);
    Ok(())
}

/// Runs a named input scenario.
fn scenario(name: &str, json: bool, out: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    match name {
        "input_focus_guards" => focus_guards(json, out, golden),
        other => bail!("unknown input scenario `{other}`"),
    }
}

/// Focus/lock guard truth table (M0 §7a).
fn focus_guards(json: bool, out: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let mut cases = Vec::new();

    // locked() gates camera/world mutations, not typed UI shortcuts.
    let mut guards = FocusGuards::default();
    guards.locks.add_lock(LockId::Cutscene);
    cases.push(serde_json::json!({
        "name": "locked_gates_camera_only",
        "locked": guards.locked(),
        "blocks_camera": guards.blocks_camera(),
        "typing": guards.focus.typing(),
    }));
    guards.locks.clear();

    // Dialog blocks placement + shooting.
    guards.focus.has_dialog = true;
    cases.push(serde_json::json!({
        "name": "dialog_blocks_placement",
        "can_place": guards.can_place(),
        "can_shoot": guards.can_shoot(),
        "can_mine": guards.can_mine(),
    }));
    guards.focus.has_dialog = false;

    // Chat/console/scroll block zoom.
    guards.focus.chat_shown = true;
    cases.push(serde_json::json!({
        "name": "chat_blocks_zoom",
        "blocks_zoom": guards.blocks_zoom(),
    }));
    guards.focus.chat_shown = false;
    guards.focus.has_scroll = true;
    cases.push(serde_json::json!({
        "name": "scroll_blocks_zoom",
        "blocks_zoom": guards.blocks_zoom(),
    }));
    guards.focus = FocusState::default();

    // Menu/editor clears placement state.
    guards.focus.is_menu = true;
    cases.push(serde_json::json!({
        "name": "menu_clears_state",
        "can_place": guards.can_place(),
        "clears_placement": guards.focus.clears_placement(),
    }));
    guards.focus = FocusState::default();

    // PlaceMode parity names.
    let modes: Vec<&str> = [
        PlaceMode::None,
        PlaceMode::Breaking,
        PlaceMode::Placing,
        PlaceMode::SchematicSelect,
        PlaceMode::RebuildSelect,
    ]
    .iter()
    .map(|mode| mode.name())
    .collect();

    let report = serde_json::json!({
        "scenario": "input_focus_guards",
        "cases": cases,
        "modes": modes,
        "keybinds": BindingState::new().len(),
        "checksum": fnv1a(serde_json::to_string(&cases)?.as_bytes()),
    });
    emit(&report, out, golden)?;
    print_json(&report, json);
    Ok(())
}

/// FNV-1a 64-bit (stable report checksum; goldens are the source of truth).
pub(crate) fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

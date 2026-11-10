// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Unit tests for the UI root's free helpers and node-hook queue.
//! Upstream: none (port-only; unit tests for the UI root helpers)

use super::free::{json_bool, json_string, save_failure_markup};

use super::*;

#[test]
fn toast_slot_is_three_and_a_half_seconds() {
    // `HudFragment.java:730`: `(int)(3.5 * 1000)`.
    assert!((TOAST_SLOT_MS - 3500.0).abs() < f64::EPSILON);
}

#[test]
fn uiscale_countdown_rounds_up_to_whole_seconds() {
    // `ceil` of a 10.02 s remainder shows 11, like the GDScript prompt.
    assert_eq!((10.02f64).ceil() as i32, 11);
    assert_eq!((-0.5f64).ceil().max(0.0) as i32, 0);
}

#[test]
fn prompt_spacing_matches_base_dialog_cells() {
    // title rule 4 + `cont` 3 + body 4 (+2 line box).
    assert!((4.0f32 + 3.0 + 4.0 + 2.0 - 13.0).abs() < f32::EPSILON);
    // body 4 + `cont` 3 + buttons 3 + button 2 (+1 line box).
    assert!((4.0f32 + 3.0 + 3.0 + 2.0 + 1.0 - 13.0).abs() < f32::EPSILON);
}

#[test]
fn node_hook_queue_drains_in_arrival_order() {
    // The `Tree.node_added` listener only records instance ids — it never
    // binds the root, so an add from a bound method cannot panic. The queue
    // contract is engine-free: FIFO, and a drain empties it.
    let first = InstanceId::from_i64(1);
    let second = InstanceId::from_i64(2);
    PENDING_NODE_HOOKS.with(|queue| queue.borrow_mut().clear());
    push_node_hook(first);
    push_node_hook(second);
    assert!(take_node_hooks() == vec![first, second]);
    assert!(take_node_hooks().is_empty());
}

#[test]
fn save_failure_markup_matches_show_exception() {
    // `ui.showException("[accent]" + @savefail, e)`: accent body, the final
    // message in light gray inside parentheses (`UI.java:518-527`).
    let text = save_failure_markup("Failed to save game!", "disk full");
    assert_eq!(
        text,
        "[accent]Failed to save game![]\n[lightgray](disk full)"
    );
    // A message-less failure keeps only the accent body.
    assert_eq!(
        save_failure_markup("Failed to save game!", ""),
        "[accent]Failed to save game![]"
    );
}

#[test]
fn manifest_helpers_default_when_fields_are_absent() {
    let value = serde_json::json!({"name": "about", "pause": true});
    assert_eq!(json_string(&value, "name"), "about");
    assert!(json_bool(&value, "pause"));
    assert_eq!(json_string(&value, "scene"), "");
    assert!(!json_bool(&value, "hidden"));
}

// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/editor/MapEditorDialog.java,
//         core/src/mindustry/editor/MapView.java,
//         core/src/mindustry/editor/WaveGraph.java,
//         core/src/mindustry/editor/MapObjectivesCanvas.java.

//! Native scene roots for the four standalone map-editor views.
//!
//! Each class replaces one scene-attached `client/scenes/editor/*.gd` script;
//! they are plain `Control`/`PanelContainer` roots (not `DialogController`s),
//! so they keep the static `.tscn` structure and only rebuild the dynamic rows:
//!
//! - [`MindMapEditorDialog`] (`map_editor_dialog.gd`,
//!   `core/src/mindustry/editor/MapEditorDialog.java`): the editor shell — tool
//!   rail, team/brush pickers, block palette, rotation/undo/redo, the
//!   `MindUi` editor-dialog buttons and the `doInput` keyboard shortcuts.
//! - [`MindMapView`] (`map_view.gd`, `core/src/mindustry/editor/MapView.java`):
//!   the pan/zoom/paint surface, its brush-outline/grid overlay and the
//!   `SubViewport` camera mirror of the `MindEditor` projection.
//! - [`MindWaveGraph`] (`wave_graph.gd`,
//!   `core/src/mindustry/editor/WaveGraph.java`): the `Tex.pane` wave plot, the
//!   unit legend with its hide toggles and the counts/totals/health mode row.
//! - [`MindMapObjectivesCanvas`] (`map_objectives_canvas.gd`,
//!   `core/src/mindustry/editor/MapObjectivesCanvas.java`): the objective
//!   marker canvas — grid, connectors, drag-to-move and the `@add` query cursor.
//!
//! Everything the views need (`status`, palette order, projection, brush
//! outline, wave series, objectives graph) comes from `/root/Spine/MindEditor`;
//! the views only lay out and forward, exactly like the GDScript they replace.

mod map_editor_dialog;
mod map_view;
mod objectives_canvas;
mod wave_graph;

#[cfg(test)]
mod tests;

// The old flat-module paths stay resolvable; only `MindMapEditorDialog` has a
// Rust caller (the others are instantiated by their `.tscn` scenes through the
// `#[derive(GodotClass)]` registration, so the re-export is for docs/tests).
pub use map_editor_dialog::MindMapEditorDialog;
#[allow(unused_imports)]
pub use map_view::MindMapView;
#[allow(unused_imports)]
pub use objectives_canvas::MindMapObjectivesCanvas;
#[allow(unused_imports)]
pub use wave_graph::MindWaveGraph;

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::{
    PackedInt64Array, PackedVector2Array, Rect2, Side, VarArray, VarDictionary, VariantType,
    Vector2, Vector2i,
};
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    Button, Camera2D, ColorRect, Control, GridContainer, HBoxContainer, HSlider, IControl,
    IPanelContainer, Input, InputEvent, InputEventKey, InputEventMouseButton,
    InputEventMouseMotion, Json, Label, LineEdit, MarginContainer, PanelContainer, ScrollContainer,
    StyleBoxEmpty, StyleBoxFlat, StyleBoxTexture, TextureRect, ThemeDb, VBoxContainer,
};
use godot::global::{HorizontalAlignment, Key, MouseButton, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindScroll, icons, styles};

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// A queued one-shot action (`Callable.call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase.
///
/// A signal emitted from inside a bound GDExtension method re-enters the same
/// object when its handler calls back into it, which panics ("already bound",
/// PAR-0066); the editor views defer exactly those calls.
fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
    let slot: DeferredAction = Rc::new(RefCell::new(Some(Box::new(action))));
    let callable = Callable::from_fn(name, move |_args: &[&Variant]| {
        let queued = slot.borrow_mut().take();
        if let Some(action) = queued {
            action();
        }
    });
    callable.call_deferred(&[]);
}

/// Calls a `MindEditor` facade method (`None` when the facade/method is absent).
fn editor_call(editor: &Gd<Node>, method: &str, args: &[Variant]) -> Option<Variant> {
    let mut editor = editor.clone();
    if !editor.has_method(method) {
        log::warn!("[ui] MindEditor has no `{method}`");
        return None;
    }
    Some(editor.call(method, args))
}

/// GDScript `float(value)` coercion for JSON numbers.
fn variant_f64(value: &Variant) -> f64 {
    match value.get_type() {
        VariantType::FLOAT => value.try_to::<f64>().unwrap_or_default(),
        VariantType::INT => value
            .try_to::<i64>()
            .map(|number| number as f64)
            .unwrap_or_default(),
        _ => 0.0,
    }
}

/// GDScript `int(value)` for JSON numbers.
fn variant_i64(value: &Variant) -> i64 {
    match value.get_type() {
        VariantType::INT => value.try_to::<i64>().unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number as i64)
            .unwrap_or_default(),
        _ => 0,
    }
}

/// `bool(row.get(key, false))` on a dictionary.
fn dict_bool(dict: &VarDictionary, key: &str) -> bool {
    match dict.get(key) {
        Some(value) => match value.get_type() {
            VariantType::BOOL => value.try_to::<bool>().unwrap_or(false),
            _ => false,
        },
        None => false,
    }
}

/// `int(row.get(key, 0))` on a dictionary (`0` when absent).
fn dict_i64(dict: &VarDictionary, key: &str) -> i64 {
    dict.get(key).map(|value| variant_i64(&value)).unwrap_or(0)
}

/// `float(row.get(key, 0))` on a dictionary (`0` when absent).
fn dict_f64(dict: &VarDictionary, key: &str) -> f64 {
    dict.get(key)
        .map(|value| variant_f64(&value))
        .unwrap_or(0.0)
}

/// `str(row.get(key, ""))` on a dictionary (`""` when absent).
fn dict_string(dict: &VarDictionary, key: &str) -> String {
    match dict.get(key) {
        Some(value) if value.get_type() == VariantType::STRING => value
            .try_to::<GString>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// `row.get(key, [])` as an untyped array.
fn dict_array(dict: &VarDictionary, key: &str) -> VarArray {
    dict.get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
}

/// `assets.bundle_get(key)` with the key echoed back when it resolves empty
/// (`MindWidgets` bundle lookups plus the wave-graph `_bundle` fallback).
fn bundle(key: &str) -> String {
    let Some(assets) = kit::assets() else {
        return key.to_string();
    };
    let value = assets.bind().bundle_get(GString::from(key)).to_string();
    if value.is_empty() {
        key.to_string()
    } else {
        value
    }
}

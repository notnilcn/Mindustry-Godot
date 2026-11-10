// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The effects dialog (`effects`): the `LogicFx` cell grid and its shared live-preview host.
//! Upstream: core/src/mindustry/ui/dialogs/EffectsDialog.java

use godot::builtin::Side;
use godot::classes::control::{FocusMode, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{Button, Control, GridContainer, Label, StyleBoxEmpty, Time, Timer};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindScroll};

use crate::ui::dialogs::settings::util::*;

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "effects" => Some(Box::new(EffectsController)),
        _ => None,
    }
}

/// `EffectsDialog.EffectCell` edge and grid gap (`EffectsDialog.setup`).
const EFFECTS_CELL_SIZE: f32 = 280.0;
const EFFECTS_CELL_GAP: f32 = 12.0;
/// The shared preview host ticks at 20 Hz (`EffectCell.act` is per frame; the
/// port re-renders the one hovered effect rather than 35 cell batches).
const EFFECTS_PREVIEW_TICK_SECONDS: f64 = 0.05;
/// `EffectCell` respawns when its lifetime elapses; the port re-emits the
/// hovered effect every 8 ticks (0.4 s).
const EFFECTS_PREVIEW_RESPAWN_TICKS: u32 = 8;
/// `EffectCell` scale clamp (`width / calculateSize` explodes for tiny bounds).
const EFFECTS_PREVIEW_MIN_SCALE: f32 = 0.1;
const EFFECTS_PREVIEW_MAX_SCALE: f32 = 2.0;

/// The shared live-preview host of the effects dialog.
///
/// Java's `EffectsDialog.EffectCell` renders its own effect through
/// `Core.batch`. The native port has no per-cell batch, so one [`MindFx`] node
/// is parented over the dialog and plays the hovered cell's `LogicFx` entry
/// (`EffectCell.draw` + `act`: the effect loops until the pointer leaves). This
/// is a documented deviation: only the hovered cell animates, not all 35.
#[derive(Clone)]
struct EffectsPreview {
    fx: Gd<crate::fx::MindFx>,
    /// `(Fx field name, canvas position, scale)` while a cell is hovered.
    current: Option<(String, Vector2, f32)>,
    tick: u32,
}

/// The `Fx` field a `LogicFx` entry plays (`LogicFx.EffectEntry.effect`).
fn effects_fx_name(effect_name: &str) -> Option<&'static str> {
    mind_core::logic::fx::get(effect_name)
        .map(|entry| entry.fx)
        .filter(|fx| !fx.is_empty())
}

/// `width / calculateSize(effect)` (`EffectsDialog.java:211`), clamped so a
/// tiny cached bound cannot blow the preview up past the cell.
fn effects_preview_scale(bounds: f32) -> f32 {
    let bounds = if bounds > 0.0 {
        bounds
    } else {
        EFFECTS_CELL_SIZE
    };
    (EFFECTS_CELL_SIZE / bounds).clamp(EFFECTS_PREVIEW_MIN_SCALE, EFFECTS_PREVIEW_MAX_SCALE)
}

/// `ui.effects` — the `LogicFx` picker used by `EffectStatement`.
///
/// A scrolling grid of 280 px cells over the alpha checker with the effect's
/// `name.token.<name>` label. Hovering a cell plays its `Fx` in a shared
/// [`MindFx`] preview host; selecting a cell emits `effect_selected` and closes
/// the dialog.
#[derive(Default)]
pub struct EffectsController;

impl DialogController for EffectsController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        // `makeButtonOverlay()` (`EffectsDialog.java:34`): padded title cell.
        dialog.apply_button_overlay(true);
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_literal("Effects");
        effects_build(dialog);
    }
}

/// `MindLogic.effect_entries()` rows.
fn effects_entries() -> Vec<VarDictionary> {
    let Some(mut logic) = kit::logic() else {
        return Vec::new();
    };
    if !logic.has_method("effect_entries") {
        return Vec::new();
    }
    let rows = logic.call("effect_entries", &[]).to::<VarArray>();
    rows.iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// `EffectsDialog.setup`: `cols = max(1, width / (size + 12))`, a scrollable
/// grid of 280 px cells.
fn effects_build(dialog: &mut MindDialog) {
    dialog.clear_content();
    // The shared live-preview host over the cells (`EffectCell.draw`).
    let preview = effects_preview_host(dialog);
    let width = viewport_width(dialog);
    let columns = ((width / (EFFECTS_CELL_SIZE + EFFECTS_CELL_GAP)) as i32).max(1);
    // code-instantiated: the effect grid is data-driven (`LogicFx.entries()`).
    let mut scroll = MindScroll::new_alloc();
    scroll.bind_mut().set_scrolling_disabled(true, false);
    scroll.set_custom_minimum_size(Vector2::new((width * 0.9).min(1200.0), 520.0));
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    // code-instantiated: one cell per LogicFx entry.
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(columns);
    grid.add_theme_constant_override("h_separation", EFFECTS_CELL_GAP as i32);
    grid.add_theme_constant_override("v_separation", EFFECTS_CELL_GAP as i32);
    {
        let control: Gd<Control> = grid.clone().upcast();
        scroll.add_child(&control);
    }
    if let Some(mut content) = dialog.content() {
        let control: Gd<Control> = scroll.clone().upcast();
        content.add_child(&control);
    }
    let entries = effects_entries();
    if entries.is_empty() {
        grid.add_child(&factory::label(&kit::t("@none.found").to_string()));
        return;
    }
    for entry in entries {
        let cell = effects_cell(dialog, preview.as_ref(), &dict_text(&entry, "name"));
        grid.add_child(&cell);
    }
}

/// The shared [`MindFx`] preview host over the effects dialog.
///
/// Java's `EffectCell` renders into its own clipped transform; the port parents
/// one `MindFx` Node2D over the dialog, moves it to the hovered cell and loops
/// that cell's effect. Returns `None` when the FX host cannot be built (the
/// cell keeps its HSV-wash fallback).
fn effects_preview_host(dialog: &mut MindDialog) -> Option<Rc<RefCell<EffectsPreview>>> {
    for name in ["EffectPreviewFx", "EffectPreviewTick"] {
        if let Some(mut old) = dialog.base().get_node_or_null(name) {
            if let Some(mut parent) = old.get_parent() {
                parent.remove_child(&old);
            }
            old.queue_free();
        }
    }
    let mut fx = crate::fx::MindFx::new_alloc();
    // Silence the required-camera wiring warning when the game camera exists
    // (the preview never emits shake events, so it only satisfies the export).
    let camera = kit::node("/root/Spine/World/Camera2D")
        .and_then(|node| node.try_cast::<crate::camera::MindCamera2D>().ok());
    fx.bind_mut().camera = camera;
    let mut fx_node = fx.clone();
    fx_node.set_name("EffectPreviewFx");
    fx_node.set_z_index(100);
    let preview = Rc::new(RefCell::new(EffectsPreview {
        fx: fx.clone(),
        current: None,
        tick: 0,
    }));
    {
        let mut root: Gd<Control> = dialog.to_gd().upcast();
        let node: Gd<Node> = fx.upcast();
        root.add_child(&node);
    }
    // code-instantiated: the shared 20 Hz preview ticker (there is one per open
    // dialog, not one per cell).
    let mut timer = Timer::new_alloc();
    timer.set_name("EffectPreviewTick");
    timer.set_wait_time(EFFECTS_PREVIEW_TICK_SECONDS);
    timer.set_one_shot(false);
    timer.set_autostart(true);
    let preview_for_tick = preview.clone();
    let dialog_for_tick = dialog.to_gd();
    let callable = Callable::from_fn("effects_preview_tick", move |_args: &[&Variant]| {
        if !dialog_for_tick.is_instance_valid() || !dialog_for_tick.is_visible_in_tree() {
            return Variant::nil();
        }
        let mut state = preview_for_tick.borrow_mut();
        state.tick = state.tick.wrapping_add(1);
        let Some((fx_name, position, scale)) = state.current.clone() else {
            // Still advance the pool so the last effect finishes its lifetime.
            state.fx.clone().bind_mut().tick_view();
            return Variant::nil();
        };
        {
            let mut fx = state.fx.clone();
            fx.set_position(position);
            fx.set_scale(Vector2::splat(scale));
            fx.bind_mut().tick_view();
        }
        if state.tick.is_multiple_of(EFFECTS_PREVIEW_RESPAWN_TICKS) {
            state.fx.clone().bind_mut().spawn_effect(
                GString::from(fx_name.as_str()),
                0.0,
                0.0,
                0.0,
            );
        }
        Variant::nil()
    });
    timer.connect("timeout", &callable);
    {
        let mut root: Gd<Control> = dialog.to_gd().upcast();
        let node: Gd<Node> = timer.upcast();
        root.add_child(&node);
    }
    Some(preview)
}

/// One `EffectsDialog.EffectCell`: a clickable 280 px stack with the preview
/// and the bottom-left outlined name.
fn effects_cell(
    dialog: &mut MindDialog,
    preview: Option<&Rc<RefCell<EffectsPreview>>>,
    effect_name: &str,
) -> Gd<Button> {
    // code-instantiated: one preview cell per LogicFx entry.
    let mut button = Button::new_alloc();
    button.set_custom_minimum_size(Vector2::new(EFFECTS_CELL_SIZE, EFFECTS_CELL_SIZE));
    button.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    button.set_v_size_flags(SizeFlags::SHRINK_BEGIN);
    button.set_focus_mode(FocusMode::NONE);
    for state in [
        "normal",
        "hover",
        "pressed",
        "disabled",
        "focus",
        "hover_pressed",
    ] {
        button.add_theme_stylebox_override(state, &StyleBoxEmpty::new_gd());
    }
    let mut preview_control = effects_preview(effect_name);
    preview_control.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    preview_control.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&preview_control);
    let mut label = effects_outline_label(effect_name);
    label.set_anchors_and_offsets_preset(LayoutPreset::BOTTOM_LEFT);
    label.set_offset(Side::LEFT, 6.0);
    label.set_offset(Side::BOTTOM, -4.0);
    label.set_offset(Side::TOP, -28.0);
    label.set_offset(Side::RIGHT, EFFECTS_CELL_SIZE - 6.0);
    label.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&label);
    button.set_tooltip_text(&GString::from(effect_name));
    let target = dialog.to_gd();
    let name_for_pick = effect_name.to_string();
    let callable = Callable::from_fn("effects_pick", move |_args: &[&Variant]| {
        if !target.is_instance_valid() {
            return;
        }
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        effects_pick(&mut bound, &name_for_pick);
    });
    button.connect("pressed", &callable);
    // Live preview (`EffectCell.draw`): the shared host moves to this cell and
    // loops its `Fx` while hovered. The HSV wash is the no-host fallback.
    if let Some(preview) = preview
        && let Some(fx_name) = effects_fx_name(effect_name)
    {
        let bounds = mind_core::logic::fx::get(effect_name)
            .map(|entry| entry.bounds)
            .unwrap_or(-1.0);
        let scale = effects_preview_scale(bounds);
        let cell_id = button.instance_id();
        let preview_in = preview.clone();
        let dialog_for = dialog.to_gd();
        let fx_name = fx_name.to_string();
        let enter = Callable::from_fn("effects_preview_enter", move |_args: &[&Variant]| {
            let Ok(cell) = Gd::<Control>::try_from_instance_id(cell_id) else {
                return Variant::nil();
            };
            if !dialog_for.is_instance_valid() {
                return Variant::nil();
            }
            let mut state = preview_in.borrow_mut();
            let centre = cell.get_global_rect().center();
            {
                let mut fx = state.fx.clone();
                fx.set_position(centre);
                fx.set_scale(Vector2::splat(scale));
                fx.bind_mut().tick_view();
            }
            state.current = Some((fx_name.clone(), centre, scale));
            state.tick = EFFECTS_PREVIEW_RESPAWN_TICKS - 1;
            Variant::nil()
        });
        button.connect("mouse_entered", &enter);
        let preview_out = preview.clone();
        let leave = Callable::from_fn("effects_preview_leave", move |_args: &[&Variant]| {
            preview_out.borrow_mut().current = None;
            Variant::nil()
        });
        button.connect("mouse_exited", &leave);
    }
    button
}

/// `LStatement.bundle(entry.name)` (`name.token.<name>` fallback: the name).
fn effects_outline_label(effect_name: &str) -> Gd<Label> {
    // code-instantiated: one label per effect cell.
    let mut label = Label::new_alloc();
    let key = format!("name.token.{effect_name}");
    let text = kit::t(&key).to_string();
    let label_text = if text == key {
        effect_name.to_string()
    } else {
        text
    };
    label.set_text(&GString::from(label_text.as_str()));
    label.add_theme_color_override("font_color", Color::WHITE);
    label.add_theme_color_override("font_outline_color", Color::BLACK);
    label.add_theme_constant_override("outline_size", 4);
    label
}

/// `EffectsDialog.EffectCell` background: the alpha checker and the border;
/// the live `Fx` itself is drawn by the shared preview host above the cell.
/// Cells without a resolvable `Fx` keep the animated HSV wash as a fallback.
fn effects_preview(effect_name: &str) -> Gd<Control> {
    // code-instantiated: preview layer of one effect cell.
    let mut preview = Control::new_alloc();
    preview.set_mouse_filter(MouseFilter::IGNORE);
    let with_wash = effects_fx_name(effect_name).is_none();
    let preview_for_draw = preview.clone();
    let callable = Callable::from_fn("effects_preview_draw", move |_args: &[&Variant]| {
        if !preview_for_draw.is_instance_valid() {
            return;
        }
        effects_draw_preview(&preview_for_draw, with_wash);
    });
    preview.connect("draw", &callable);
    if with_wash {
        // code-instantiated: the fallback animation ticker of one preview cell.
        let mut timer = Timer::new_alloc();
        timer.set_wait_time(1.0 / 60.0);
        timer.set_one_shot(false);
        timer.set_autostart(true);
        let preview_for_tick = preview.clone();
        let callable = Callable::from_fn("effects_preview_redraw", move |_args: &[&Variant]| {
            if !preview_for_tick.is_instance_valid() {
                return;
            }
            let mut node = preview_for_tick.clone();
            node.queue_redraw();
        });
        timer.connect("timeout", &callable);
        preview.add_child(&timer);
    }
    preview
}

/// `EffectsDialog.EffectCell.draw`: the checker, the optional HSV wash and the
/// border.
fn effects_draw_preview(preview: &Gd<Control>, with_wash: bool) {
    let mut canvas: Gd<Control> = preview.clone();
    let size = canvas.get_size();
    // `Tex.alphaBg`: a 16 px dark/light checker.
    let cell = 16.0f32;
    let mut y = 0.0f32;
    let mut row = 0i32;
    while y < size.y {
        let mut x = 0.0f32;
        let mut column = row;
        while x < size.x {
            let tint = if column % 2 == 0 {
                Color::from_rgba(0.35, 0.35, 0.35, 0.55)
            } else {
                Color::from_rgba(0.22, 0.22, 0.22, 0.55)
            };
            canvas.draw_rect(
                Rect2::new(Vector2::new(x, y), Vector2::new(cell, cell)),
                tint,
            );
            x += cell;
            column += 1;
        }
        y += cell;
        row += 1;
    }
    if with_wash {
        // `color.fromHsv((Time.globalTime * 2f) % 360f, 1f, 1f)`.
        let hue = (Time::singleton().get_ticks_msec() as f64 * 0.002) % 1.0;
        let mut wash = Color::from_hsv(hue, 1.0, 1.0);
        wash.a = 0.85;
        let centre = size * 0.5;
        let radius = size.x.min(size.y) * 0.28;
        canvas.draw_circle(centre, radius, wash);
        canvas.draw_circle(centre, radius * 0.55, Color::from_rgba(1.0, 1.0, 1.0, 0.35));
    }
    canvas
        .draw_rect_ex(Rect2::new(Vector2::ZERO, size), Color::BLACK)
        .filled(false)
        .width(3.0)
        .done();
}

/// `_pick`: emit `effect_selected(name)` and close.
fn effects_pick(dialog: &mut MindDialog, effect_name: &str) {
    dialog.base_mut().emit_signal(
        "effect_selected",
        &[GString::from(effect_name).to_variant()],
    );
    if let Some(mut ui) = kit::ui() {
        let closed = ui
            .call("close_dialog", &[GString::from("effects").to_variant()])
            .try_to::<bool>()
            .unwrap_or(false);
        if closed {
            return;
        }
    }
    dialog.hide_dialog();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_preview_resolves_logic_fx_to_its_fx_field() {
        // `LogicFx.EffectEntry.effect`: the preview spawns the Fx by field name.
        assert_eq!(effects_fx_name("breakBlock"), Some("breakBlock"));
        assert_eq!(effects_fx_name("explosion"), Some("dynamicExplosion"));
        assert_eq!(effects_fx_name("bubble"), Some("airBubble"));
        assert_eq!(effects_fx_name("not-an-effect"), None);
    }

    #[test]
    fn effects_preview_scale_fits_the_cell() {
        // `width / calculateSize`; no cached bounds falls back to the cell size.
        assert_eq!(effects_preview_scale(EFFECTS_CELL_SIZE), 1.0);
        assert_eq!(effects_preview_scale(-1.0), 1.0);
        assert_eq!(effects_preview_scale(100.0), EFFECTS_PREVIEW_MAX_SCALE);
        assert_eq!(effects_preview_scale(10_000.0), EFFECTS_PREVIEW_MIN_SCALE);
    }
}

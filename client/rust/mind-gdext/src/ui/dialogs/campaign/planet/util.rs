// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared row readers, settings accessors and widget helpers for the planet dialog.
//! Upstream: none (port-only; shared UI helpers of the old batch_c).

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::{VarArray, VarDictionary, VariantType};
use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::{
    Button, Control, Json, Label, Node, PanelContainer, RichTextLabel, StyleBoxFlat, Texture2D,
    TextureRect,
};
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::editor::MindPreview;
use crate::ui::dialog::MindDialog;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindLabel};

// ---------------------------------------------------------------------------
// Shared helpers
//
// Row readers and settings accessors shared by the campaign dialogs of this
// batch. The planet page does not call every one of them yet, so the unused
// ones carry `allow(dead_code)` instead of being dropped from the port.
// ---------------------------------------------------------------------------

/// `Pal.gray` / `Tex.pane` outline (#454545) used by the pane buttons.
pub(super) const OUTLINE_GRAY: Color = Color::from_rgba(0.27, 0.27, 0.27, 1.0);
/// `Pal.lightGray` sampled from the reference frames (sector stats cells).
#[allow(dead_code)]
pub(super) const LIGHT_GRAY: Color = Color::from_rgba(0.62, 0.62, 0.62, 1.0);
/// `Color(0.75, 0.75, 0.75)` (research requirement/objective text).
pub(super) const TEXT_GRAY: Color = Color::from_rgba(0.75, 0.75, 0.75, 1.0);
/// `Pal.remove` (`Color(1.0, 0.204, 0.11)`), the requirement-red.
pub(super) const PAL_REMOVE: Color = Color::from_rgba(1.0, 0.204, 0.11, 1.0);
/// The live planet view (`ui/planet.rs`).
pub(super) const PLANET_VIEW_PATH: &str = "/root/Spine/Planet/PlanetView";
/// The live map registry (`editor/preview.rs`).
pub(super) const PREVIEW_PATH: &str = "/root/Spine/MindPreview";
/// Stage layers hidden while the planet page covers the screen.
pub(super) const MENU_GROUP_PATH: &str = "/root/Spine/Ui/UiRoot/MenuGroup";
pub(super) const MENU_BACKGROUND_PATH: &str = "/root/Spine/Ui/MenuBackground";
pub(super) const HUD_GROUP_PATH: &str = "/root/Spine/Ui/UiRoot/HudGroup";
pub(super) const WORLD_PATH: &str = "/root/Spine/World";

/// A rich label's engine handle (text/color/alignment live on `RichTextLabel`).
pub(super) fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

/// Reads a JSON string field (`""` when missing or null).
pub(super) fn row_str(row: &VarDictionary, key: &str) -> String {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// Reads a JSON string field (`None` when missing or null).
#[allow(dead_code)]
pub(super) fn row_opt_str(row: &VarDictionary, key: &str) -> Option<String> {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
}

/// Reads a JSON boolean field (`false` when missing).
pub(super) fn row_bool(row: &VarDictionary, key: &str) -> bool {
    row_bool_or(row, key, false)
}

/// Reads a JSON boolean field with a default.
pub(super) fn row_bool_or(row: &VarDictionary, key: &str, default: bool) -> bool {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(default)
}

/// Reads a JSON integer field (`0` when missing; floats truncate).
pub(super) fn row_i64(row: &VarDictionary, key: &str) -> i64 {
    row_i64_or(row, key, 0)
}

/// Reads a JSON integer field with a default.
pub(super) fn row_i64_or(row: &VarDictionary, key: &str, default: i64) -> i64 {
    let Some(value) = row
        .get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
    else {
        return default;
    };
    variant_i64(&value)
}

/// Reads a JSON-derived number as an integer: `JSON.parse_string` yields
/// `Variant::FLOAT` for every number, and `try_to` only converts an exact type
/// match, so a bare `try_to::<i64>` silently zeroes launch amounts (PAR-0488).
pub(super) fn variant_i64(value: &Variant) -> i64 {
    if let Ok(integer) = value.try_to::<i64>() {
        return integer;
    }
    value
        .try_to::<f64>()
        .map(|number| number as i64)
        .unwrap_or(0)
}

/// Reads a JSON float field (`0.0` when missing).
#[allow(dead_code)]
pub(super) fn row_f64(row: &VarDictionary, key: &str) -> f64 {
    let Some(value) = row
        .get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
    else {
        return 0.0;
    };
    if let Ok(number) = value.try_to::<f64>() {
        return number;
    }
    value
        .try_to::<i64>()
        .map(|value| value as f64)
        .unwrap_or(0.0)
}

/// Reads a JSON array field (empty when missing).
pub(super) fn row_arr(row: &VarDictionary, key: &str) -> VarArray {
    row.get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
}

/// Reads a JSON object field (empty when missing).
pub(super) fn row_dict(row: &VarDictionary, key: &str) -> VarDictionary {
    row.get(key)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// Reads the row dictionaries of a JSON array field.
pub(super) fn row_rows(row: &VarDictionary, key: &str) -> Vec<VarDictionary> {
    row_arr(row, key)
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// Parses a JSON object (`{}` on any other JSON shape).
pub(super) fn parse_object(text: &str) -> VarDictionary {
    if text.is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(&GString::from(text))
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// Calls a `MindUi` method when both the autoload and the method exist.
pub(super) fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// Calls a `MindUi` method returning a bool (`false` when absent).
pub(super) fn ui_bool(method: &str, args: &[Variant]) -> bool {
    ui_call(method, args)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(false)
}

/// `Core.settings.getBool(key)` through the `MindUi` store.
pub(super) fn setting_flag(key: &str) -> bool {
    ui_bool("settings_get_flag", &[GString::from(key).to_variant()])
}

/// `Core.settings.getString(key, default)` through the `MindUi` store.
pub(super) fn setting_text(key: &str, default: &str) -> String {
    ui_call(
        "settings_get_text",
        &[
            GString::from(key).to_variant(),
            GString::from(default).to_variant(),
        ],
    )
    .and_then(|value| value.try_to::<GString>().ok())
    .map(|value| value.to_string())
    .unwrap_or_else(|| default.to_string())
}

/// `Core.settings.getInt(key, default)` through the `MindUi` store.
#[allow(dead_code)]
pub(super) fn setting_get_int(key: &str, default: i64) -> i64 {
    ui_call(
        "settings_get_int",
        &[GString::from(key).to_variant(), default.to_variant()],
    )
    .and_then(|value| value.try_to::<i64>().ok())
    .unwrap_or(default)
}

/// `Core.settings.put(key, value)` (bool) through the `MindUi` store.
pub(super) fn setting_put_flag(key: &str, value: bool) {
    let _ = ui_call(
        "settings_put_flag",
        &[GString::from(key).to_variant(), value.to_variant()],
    );
}

/// `Core.settings.put(key, value)` (string) through the `MindUi` store.
pub(super) fn setting_put_text(key: &str, value: &str) {
    let _ = ui_call(
        "settings_put_text",
        &[
            GString::from(key).to_variant(),
            GString::from(value).to_variant(),
        ],
    );
}

/// The live map registry node (`None` outside the game scene).
#[allow(dead_code)]
pub(super) fn preview_node() -> Option<Gd<MindPreview>> {
    kit::node(PREVIEW_PATH)?.try_cast::<MindPreview>().ok()
}

/// The `UiRoot` shell (`/root/Spine/Ui/UiRoot`).
pub(super) fn ui_root() -> Option<Gd<Node>> {
    kit::node("/root/Spine/Ui/UiRoot")
}

/// One manifest dialog node through `UiRoot.dialog(name)`.
pub(super) fn dialog_node(name: &str) -> Option<Gd<Node>> {
    let mut root = ui_root()?;
    if !root.has_method("dialog") {
        return None;
    }
    root.call("dialog", &[GString::from(name).to_variant()])
        .try_to::<Gd<Node>>()
        .ok()
}

/// Shows a planed pane style (`_pane_box`: black fill, 4 px border, 2 px radius).
pub(super) fn pane_box(fill: Color, border: Color, width: i32, radius: i32) -> Gd<StyleBoxFlat> {
    MindDialog::pane_box(fill, border, width as f32, radius as f32, -1.0)
}

/// Forces a themed toggle's visual state without running its listeners.
pub(super) fn set_toggle(button: &Gd<Button>, pressed: bool) {
    let mut button = button.clone();
    button.set_pressed_no_signal(pressed);
}

/// A sizing-only spacer cell (`Control()` with an expand flag).
pub(super) fn spacer(expand: bool, width: f32, height: f32) -> Gd<Control> {
    // code-instantiated: layout-only spacer, no static scene owns it.
    let mut control = Control::new_alloc();
    control.set_custom_minimum_size(Vector2::new(width, height));
    if expand {
        control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    }
    control.set_mouse_filter(MouseFilter::IGNORE);
    control
}

/// An icon-font glyph control sized `extent` (`MindWidgets.glyph`).
pub(super) fn glyph_control(name: &str, extent: f32) -> Gd<Label> {
    factory::glyph_label(name, extent as i32, Color::WHITE)
}

/// `UnlockableContent.uiIcon` region chain (`item-<name>-ui` first).
pub(super) fn content_texture(content_name: &str) -> Option<Gd<Texture2D>> {
    for prefix in ["item", "liquid", "block", "unit", "status"] {
        if let Some(texture) = kit::icon_texture(&format!("{prefix}-{content_name}-ui")) {
            return Some(texture.upcast());
        }
    }
    None
}

/// A fitted `TextureRect` for an atlas region (`res.image(uiIcon).size(n)`).
pub(super) fn icon_rect(texture: &Gd<Texture2D>, extent: f32) -> Gd<TextureRect> {
    // code-instantiated: one content icon per data row.
    let mut image = TextureRect::new_alloc();
    image.set_texture(texture);
    image.set_custom_minimum_size(Vector2::new(extent, extent));
    image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    image.set_mouse_filter(MouseFilter::IGNORE);
    image
}

/// A `Picture`/`BorderImage` preview: fitted texture with a `thickness` border.
#[allow(dead_code)]
pub(super) fn border_image(
    texture: &Gd<Texture2D>,
    extent: f32,
    thickness: f32,
) -> Gd<PanelContainer> {
    // code-instantiated: map preview textures come from the map registry.
    let mut image = TextureRect::new_alloc();
    image.set_texture(texture);
    image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    image.set_custom_minimum_size(Vector2::new(extent, extent));
    image.set_texture_filter(godot::classes::canvas_item::TextureFilter::NEAREST);
    image.set_mouse_filter(MouseFilter::IGNORE);
    let mut frame = panel_with_box(pane_box(
        Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        OUTLINE_GRAY,
        thickness as i32,
        0,
    ));
    frame.add_child(&image);
    frame
}

/// A `PanelContainer` carrying a stylebox override.
pub(super) fn panel_with_box(style: Gd<StyleBoxFlat>) -> Gd<PanelContainer> {
    // code-instantiated: themed frame built per data row.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &style);
    panel
}

/// Connects `callable` to `signal` once per dialog (`is_connected` cannot match
/// a `Callable::from_fn`, so the dialog node carries the latch).
pub(super) fn connect_once(
    dialog: &mut MindDialog,
    key: &str,
    target: &Gd<Node>,
    signal: &str,
    callable: &Callable,
) {
    let meta = format!("_batch_c_{key}");
    if dialog.base().has_meta(&meta) {
        return;
    }
    if !target.has_signal(signal) {
        return;
    }
    let mut target = target.clone();
    let _ = target.connect(signal, callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

/// Runs `action` on the next frame (the deferred seams).
///
/// The queue is drained from a `SceneTree.process_frame` connection installed
/// by [`install_frame_tick`]; `MindDialog` owns no `_process` of its own.
// A boxed one-shot closure; naming the alias would not make the type simpler.
#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub(super) struct Deferred(Rc<RefCell<Vec<Box<dyn FnMut()>>>>);

impl Deferred {
    pub(super) fn push(&self, action: impl FnMut() + 'static) {
        self.0.borrow_mut().push(Box::new(action));
    }

    pub(super) fn drain(&self) {
        let queued: Vec<Box<dyn FnMut()>> = self.0.borrow_mut().drain(..).collect();
        for mut action in queued {
            action();
        }
    }
}

/// Installs the per-frame hook once (deferred queue drain + `tick`).
pub(super) fn install_frame_tick(
    dialog: &mut MindDialog,
    key: &str,
    deferred: Deferred,
    mut tick: impl FnMut() + 'static,
) {
    let mut tree = dialog.base().get_tree();
    let meta = format!("_batch_c_frame_{key}");
    if dialog.base().has_meta(&meta) {
        return;
    }
    // The connection outlives a dialog freed while the tree is alive (editor
    // dialogs, teardown), so it must never touch the node again: the guard runs
    // before the queued actions and the tick, and drops the connection.
    let dialog_gd = dialog.to_gd();
    let tree_for = tree.clone();
    let callable_slot: Rc<RefCell<Option<Callable>>> = Rc::new(RefCell::new(None));
    let slot_for = callable_slot.clone();
    let callable = Callable::from_fn("batch_c_frame", move |_args: &[&Variant]| {
        if !dialog_gd.is_instance_valid() {
            if let Some(entry) = slot_for.borrow_mut().take() {
                let mut tree = tree_for.clone();
                if tree.is_connected("process_frame", &entry) {
                    tree.disconnect("process_frame", &entry);
                }
            }
            return Variant::nil();
        }
        deferred.drain();
        tick();
        Variant::nil()
    });
    *callable_slot.borrow_mut() = Some(callable.clone());
    let _ = tree.connect("process_frame", &callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

/// `_localized_item`/`_localized_planet` fallback rule: bundle value when the
/// key resolves, else the caller's fallback.
pub(super) fn bundle_or(key: &str, fallback: &str) -> String {
    let value = kit::t(&format!("@{key}")).to_string();
    if value.is_empty() || value == key {
        fallback.to_string()
    } else {
        value
    }
}

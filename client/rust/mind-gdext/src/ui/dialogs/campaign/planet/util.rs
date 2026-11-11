// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared row readers, settings accessors and widget helpers for the planet dialog.
//! Upstream: none (port-only; shared UI helpers of the old batch_c).
//!
//! The campaign shared set lives in [`super::super::util`]; every helper whose
//! body is identical is re-exported from there instead of copied. This file
//! keeps the planet-specific readers plus the ones whose semantics differ
//! (`row_str`/`row_opt_str` accept only `GString` values, `row_i64_or` reads
//! numbers only) and the `_batch_c_` ABI bindings.

use godot::builtin::{VarDictionary, VariantType};
use godot::classes::control::MouseFilter;
use godot::classes::{Label, Node, PanelContainer, StyleBoxFlat, Texture2D, TextureRect};
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::editor::MindPreview;
use crate::gdutil::variant::to_i64_int_or_float;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::widgets::factory;

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

// ---------------------------------------------------------------------------
// Re-exported shared helpers (identical bodies live in `dialogs/campaign/util.rs`)
// ---------------------------------------------------------------------------

pub(super) use super::super::util::{
    bundle_or, campaign_launch_stacks, campaign_sector_is_playing, campaign_views_of,
    content_texture, dialog_node, empty_sector_view, gs, icon_rect, json_stringify,
    open_dialog_json, panel_with_box, set_toggle, setting_put_flag, setting_put_text, setting_text,
    spacer, ui_bool, ui_root,
};

/// `Core.settings.getInt(key, default)` through the `MindUi` store.
#[allow(unused_imports)] // ported surface; no planet caller yet
pub(super) use super::super::util::setting_get_int;

/// A rich label's engine handle (text/color/alignment live on `RichTextLabel`).
pub(super) use crate::ui::dialogs::common::facade::rich;
/// Calls a `MindUi` method when both the autoload and the method exist.
pub(super) use crate::ui::dialogs::common::facade::ui_call;

// ---------------------------------------------------------------------------
// Planet-specific helpers
// ---------------------------------------------------------------------------

/// Reads a JSON integer field with a default (`i64`, then `f64` truncated).
///
/// Unlike the campaign copy this never parses string/bool variants (the planet
/// read model only carries numbers; `PAR-0488`).
pub(super) fn row_i64_or(row: &VarDictionary, key: &str, default: i64) -> i64 {
    let Some(value) = row
        .get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
    else {
        return default;
    };
    to_i64_int_or_float(&value)
}

/// `Core.settings.getBool(key)` through the `MindUi` store.
pub(super) fn setting_flag(key: &str) -> bool {
    ui_bool("settings_get_flag", &[GString::from(key).to_variant()])
}

/// The live map registry node (`None` outside the game scene).
#[allow(dead_code)]
pub(super) fn preview_node() -> Option<Gd<MindPreview>> {
    kit::node(PREVIEW_PATH)?.try_cast::<MindPreview>().ok()
}

/// Shows a planed pane style (`_pane_box`: black fill, 4 px border, 2 px radius).
pub(super) fn pane_box(fill: Color, border: Color, width: i32, radius: i32) -> Gd<StyleBoxFlat> {
    MindDialog::pane_box(fill, border, width as f32, radius as f32, -1.0)
}

/// An icon-font glyph control sized `extent` (`MindWidgets.glyph`).
pub(super) fn glyph_control(name: &str, extent: f32) -> Gd<Label> {
    factory::glyph_label(name, extent as i32, Color::WHITE)
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

/// Connects `callable` to `signal` once per dialog (`is_connected` cannot match
/// a `Callable::from_fn`, so the dialog node carries the latch).
///
/// Keeps the planet batch's original `_batch_c_<key>` meta key; the shared body
/// is [`connect_once_prefixed`](super::super::util::connect_once_prefixed).
pub(super) fn connect_once(
    dialog: &mut MindDialog,
    key: &str,
    target: &Gd<Node>,
    signal: &str,
    callable: &Callable,
) {
    super::super::util::connect_once_prefixed(dialog, "_batch_c_", key, target, signal, callable);
}

/// Runs `action` on the next frame (the deferred seams).
///
/// The queue is drained from a `SceneTree.process_frame` connection installed
/// by [`install_frame_tick`]; `MindDialog` owns no `_process` of its own.
pub(super) use crate::ui::dialogs::common::defer::Deferred;

/// Installs the per-frame hook once (deferred queue drain + `tick`).
///
/// Keeps the planet batch's original meta key (`_batch_c_frame_<key>`) and
/// callable name (`batch_c_frame`); both are part of the UI ABI.
pub(super) fn install_frame_tick(
    dialog: &mut MindDialog,
    key: &str,
    deferred: Deferred,
    tick: impl FnMut() + 'static,
) {
    crate::ui::dialogs::common::defer::install_frame_tick(
        dialog,
        "_batch_c_frame_",
        "batch_c_frame",
        key,
        deferred,
        tick,
    );
}

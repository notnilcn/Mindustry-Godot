// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/CustomRulesDialog.java,
//         core/src/mindustry/ui/dialogs/EditorMapsDialog.java,
//         core/src/mindustry/ui/dialogs/CanvasEditDialog.java,
//         core/src/mindustry/ui/dialogs/MapListDialog.java (`displayType`,
//         `showMap` override), core/src/mindustry/logic/CanvasBlock.java
//         (LSB-first pixel packing).

//! The custom-rules editor, the editor map browser and the logic-canvas painter.
//!
//! * `custom_rules` mirrors `CustomRulesDialog`: the category list (`waves`,
//!   `resourcesbuilding`, `unit`, `enemy`, `environment`, `light`, `music`,
//!   `planet`, `teams`), the `check`/`number`/`numberi`/`text`/`team` row
//!   builders, the per-row enable conditions (`update(a -> a.setDisabled(...))`),
//!   the search filter, the team collapsers, the in-dialog `_overlay` panels
//!   (`@edit` clipboard menu, `@configure` loadout, `@rules.weather`, the
//!   banned-block/unit pickers and the `@rules.ambientlight` colour picker) and
//!   the hide-time `rules_edited` hand-off.
//! * `editor_maps` mirrors `EditorMapsDialog extends MapListDialog`
//!   (`displayType = true`): the shared `maplist_*` base in `batch_campaign2`
//!   supplies the search row, filters, grid and card, and this controller adds
//!   `@editor.newmap`/`@editor.importmap`, the New Map text prompt, the
//!   file-chosen import route with the overwrite confirm and the per-map
//!   `editor_map` page.
//! * `canvas_edit` mirrors `CanvasEditDialog`: the paint surface over the
//!   block's palette (left-click paint, drag Bresenham line, middle-click pick,
//!   `fill`/`grid` toggles), `@import`/`@export` PNG and the 2-second auto-save
//!   that pushes LSB-first packed pixels through
//!   `MindInput.configure_building_bytes`.
//!
//! Behaviour (rules JSON, editor map state, the canvas model) stays in the Rust
//! facades; these controllers only read their projections and call the same
//! dynamic method names the GDScript did.

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::{
    PackedByteArray, PackedInt64Array, PackedStringArray, VarArray, VarDictionary, VariantType,
};
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::file_dialog::{Access, FileMode};
use godot::classes::image::{Format, Interpolation};
use godot::classes::{
    Button, CenterContainer, ColorRect, Control, DisplayServer, FileDialog, HBoxContainer,
    IControl, Image, ImageTexture, InputEvent, InputEventMouseButton, InputEventMouseMotion, Json,
    LineEdit, Node, PanelContainer, RichTextLabel, StyleBoxEmpty, VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton, MouseButtonMask};
use godot::obj::{EngineBitfield, NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::check::MindCheck;
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindTable};
use super::batch_campaign2::{
use crate::ui::dialogs::rules::util::*;
    Deferred, MapListState, install_frame_tick, maplist_add_pane_button, maplist_build,
    maplist_load_settings, maplist_maps, maplist_rebuild, maplist_refresh_registry,
};

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "canvas_edit" => Some(Box::new(CanvasEditDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// CanvasEditDialog (`canvas_edit`)
// ---------------------------------------------------------------------------

/// `CanvasEditDialog.java:185`: 480 px paint surface on desktop.
const CANVAS_EXTENT: f32 = 480.0;
/// `refreshTime = 60 * 2` (`CanvasEditDialog.java:24`): auto-save accumulator.
const CANVAS_SAVE_TIME: f64 = 120.0;
/// The default canvas size (`CanvasBlock.canvasSize`) before the row arrives.
const CANVAS_DEFAULT_SIZE: i32 = 12;
/// The default bits per pixel (`CanvasBlock.defaultBpp`) before the row arrives.
const CANVAS_DEFAULT_BPP: i64 = 3;

/// Display-side paint state shared with [`MindCanvasSurface`] (`_pixels`,
/// `_palette`, `_texture`, `_size`, `_grid`).
#[derive(Default)]
pub(crate) struct CanvasSurfaceState {
    size: i32,
    /// `_palette`: packed RGBA8888 colours.
    palette: Vec<u32>,
    /// `_pixels`: one palette index per pixel (`pos = y * size + x`, row 0 = top).
    pixels: Vec<u8>,
    texture: Option<Gd<ImageTexture>>,
    grid: bool,
}

/// The paint surface (`CanvasEditDialog`'s `cont.table(Tex.pane, body -> ...)`).
///
/// A dedicated `Control` subclass implements the custom `_draw` (pixel texture
/// plus the grid overlay); the controller owns the state it renders.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindCanvasSurface {
    base: Base<Control>,
    shared: Rc<RefCell<CanvasSurfaceState>>,
}

#[godot_api]
impl IControl for MindCanvasSurface {
    fn draw(&mut self) {
        let (texture, size, grid) = {
            let state = self.shared.borrow();
            (state.texture.clone(), state.size, state.grid)
        };
        let Some(texture) = texture else {
            return;
        };
        let texture: Gd<godot::classes::Texture2D> = texture.upcast();
        let rect = self.base().get_size();
        self.base_mut()
            .draw_texture_rect(&texture, Rect2::new(Vector2::ZERO, rect), false);
        if !grid || size <= 0 {
            return;
        }
        // `CanvasEditDialog`'s grid (`:152-170`): a line every `jump` cells.
        let cell = Vector2::new(
            (rect.x / size as f32).max(1.0),
            (rect.y / size as f32).max(1.0),
        );
        let min_space = 10.0f32;
        let jump_x = ((min_space.max(cell.x) / cell.x) as i32).max(1);
        let jump_y = ((min_space.max(cell.y) / cell.y) as i32).max(1);
        let line = Color::from_rgba(0.0, 0.0, 0.0, 0.35);
        let mut column = 0i32;
        while column <= size {
            self.base_mut().draw_rect(
                Rect2::new(
                    Vector2::new(column as f32 * cell.x - 1.0, 0.0),
                    Vector2::new(2.0, rect.y),
                ),
                line,
            );
            column += jump_x;
        }
        let mut row = 0i32;
        while row <= size {
            self.base_mut().draw_rect(
                Rect2::new(
                    Vector2::new(0.0, row as f32 * cell.y - 1.0),
                    Vector2::new(rect.x, 2.0),
                ),
                line,
            );
            row += jump_y;
        }
    }
}

impl MindCanvasSurface {
    /// Installs the controller-owned paint state.
    fn install(&mut self, shared: Rc<RefCell<CanvasSurfaceState>>) {
        self.shared = shared;
    }
}

/// `CanvasEditDialog` view state.
struct CanvasEditState {
    x: i64,
    y: i64,
    bpp: i64,
    cur_color: i64,
    fill: bool,
    modified: bool,
    save_timer: f64,
    surface: Option<Gd<MindCanvasSurface>>,
    shared: Rc<RefCell<CanvasSurfaceState>>,
    palette_row: Option<Gd<HBoxContainer>>,
    swatches: Vec<Gd<Button>>,
    /// `_last_cell`: the last painted cell (`(-1, -1)` = none).
    last_cell: (i32, i32),
}

impl Default for CanvasEditState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            bpp: CANVAS_DEFAULT_BPP,
            cur_color: 0,
            fill: false,
            modified: false,
            save_timer: 0.0,
            surface: None,
            shared: Rc::new(RefCell::new(CanvasSurfaceState {
                size: CANVAS_DEFAULT_SIZE,
                grid: true,
                ..CanvasSurfaceState::default()
            })),
            palette_row: None,
            swatches: Vec::new(),
            last_cell: (-1, -1),
        }
    }
}

/// The logic-block canvas painter (`CanvasEditDialog`).
pub struct CanvasEditDialog {
    state: Rc<RefCell<CanvasEditState>>,
}

impl Default for CanvasEditDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(CanvasEditState::default())),
        }
    }
}

impl DialogController for CanvasEditDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `CanvasEditDialog` removes the title table (`:36`).
        dialog.should_pause = false;
        dialog.set_title_literal("");
        build_canvas(dialog, &self.state);
        let target = dialog.to_gd();
        let state = self.state.clone();
        install_frame_tick(dialog, "canvas_edit", Deferred::default(), move || {
            canvas_tick(&state, &target);
        });
    }

    /// `shown()`: seed the surface from `SimHost.logic_canvas_sync`.
    fn shown(&mut self, dialog: &mut MindDialog) {
        let x = dialog.context_int("x");
        let y = dialog.context_int("y");
        canvas_load(dialog, &self.state, x, y);
    }

    /// `hidden()`: apply the packed pixmap (`CanvasEditDialog.hidden`).
    fn hidden(&mut self, _dialog: &mut MindDialog) {
        canvas_save(&self.state);
    }
}

/// `_build()`: the paint surface, the grid/fill toggles, the palette swatches
/// and the `@import`/`@export`/`@back` button row.
fn build_canvas(dialog: &mut MindDialog, state: &Rc<RefCell<CanvasEditState>>) {
    let mut table = dialog.content_table();
    let shared = state.borrow().shared.clone();
    // code-instantiated: the paint surface size is the block's canvas size; it
    // has no scene-authorable shape.
    let mut surface = MindCanvasSurface::new_alloc();
    surface.bind_mut().install(shared);
    surface.set_custom_minimum_size(Vector2::new(CANVAS_EXTENT, CANVAS_EXTENT));
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("canvas_edit_input", move |args: &[&Variant]| {
            let Some(event) = args
                .first()
                .and_then(|value| value.try_to::<Gd<InputEvent>>().ok())
            else {
                return;
            };
            canvas_input(&state_for, &event);
        });
        surface.connect("gui_input", &callable);
    }
    table.bind_mut().add(&surface);
    table.bind_mut().row();
    state.borrow_mut().surface = Some(surface.clone());

    let mut controls = HBoxContainer::new_alloc();
    table.bind_mut().add(&controls);
    let mut grid_button = factory::glyph_button("grid", "grid", "clearNoneTogglei");
    grid_button.set_toggle_mode(true);
    grid_button.set_pressed_no_signal(true);
    grid_button.set_custom_minimum_size(Vector2::new(44.0, 44.0));
    grid_button.set_tooltip_text(&kit::t("@grid"));
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("canvas_edit_grid", move |args: &[&Variant]| {
            let pressed = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            state_for.borrow().shared.borrow_mut().grid = pressed;
            canvas_redraw(&state_for);
        });
        grid_button.connect("toggled", &callable);
    }
    controls.add_child(&grid_button);

    let palette_row = HBoxContainer::new_alloc();
    controls.add_child(&palette_row);
    state.borrow_mut().palette_row = Some(palette_row.clone());

    // `Icon.fill` has no icon-font alias yet; `glyph_button` falls back to text.
    let mut fill_button = factory::glyph_button("fill", "fill", "clearNoneTogglei");
    fill_button.set_toggle_mode(true);
    fill_button.set_custom_minimum_size(Vector2::new(44.0, 44.0));
    fill_button.set_tooltip_text(&kit::t("@fill"));
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("canvas_edit_fill", move |args: &[&Variant]| {
            let pressed = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            state_for.borrow_mut().fill = pressed;
        });
        fill_button.connect("toggled", &callable);
    }
    controls.add_child(&fill_button);

    dialog.add_close_button(160.0);
    let target = dialog.to_gd();
    let import_target = target.clone();
    let state_import = state.clone();
    let callable = Callable::from_fn("canvas_edit_import", move |_args: &[&Variant]| {
        canvas_import_png(&import_target, &state_import);
    });
    let text = dialog.t("@import").to_string();
    let mut import_button = dialog.add_button(&text, Some(callable), "download", 160.0);
    let mut minimum = import_button.get_custom_minimum_size();
    minimum.y = 64.0;
    import_button.set_custom_minimum_size(minimum);
    let export_target = target;
    let state_export = state.clone();
    let callable = Callable::from_fn("canvas_edit_export", move |_args: &[&Variant]| {
        canvas_export_png(&export_target, &state_export);
    });
    let text = dialog.t("@export").to_string();
    let mut export_button = dialog.add_button(&text, Some(callable), "upload", 160.0);
    let mut minimum = export_button.get_custom_minimum_size();
    minimum.y = 64.0;
    export_button.set_custom_minimum_size(minimum);
}

/// `CanvasEditDialog.update`: `if(refreshTime >= 60*2) save()`.
fn canvas_tick(state: &Rc<RefCell<CanvasEditState>>, dialog: &Gd<MindDialog>) {
    if !dialog.is_instance_valid() || !dialog.is_visible() || !state.borrow().modified {
        return;
    }
    let delta = dialog.get_process_delta_time();
    let should_save = {
        let mut st = state.borrow_mut();
        st.save_timer += delta * 60.0;
        if st.save_timer >= CANVAS_SAVE_TIME {
            st.save_timer = 0.0;
            true
        } else {
            false
        }
    };
    if should_save {
        canvas_save(state);
    }
}

/// Seeds the surface from `SimHost.logic_canvas_sync` (`CanvasEditDialog.update`).
fn canvas_load(dialog: &mut MindDialog, state: &Rc<RefCell<CanvasEditState>>, x: i64, y: i64) {
    let _ = dialog;
    {
        let mut st = state.borrow_mut();
        st.x = x;
        st.y = y;
    }
    let Some(mut host) = kit::sim_host() else {
        return;
    };
    let mut size = CANVAS_DEFAULT_SIZE;
    let mut bpp = CANVAS_DEFAULT_BPP;
    let mut palette: Vec<u32> = Vec::new();
    if host.has_method("logic_canvas_sync") {
        let rows = host.call(
            "logic_canvas_sync",
            &[
                (-1.0e9f32).to_variant(),
                (-1.0e9f32).to_variant(),
                1.0e9f32.to_variant(),
                1.0e9f32.to_variant(),
            ],
        );
        if let Ok(rows) = rows.try_to::<VarArray>() {
            for row in rows.iter_shared() {
                let Ok(row) = row.try_to::<VarDictionary>() else {
                    continue;
                };
                if dict_i64(&row, "x", -1) != x || dict_i64(&row, "y", -1) != y {
                    continue;
                }
                size = dict_i64(&row, "pixels", i64::from(size)).max(1) as i32;
                bpp = dict_i64(&row, "bpp", bpp).max(1);
                if let Some(value) = row.get("palette")
                    && let Ok(packed) = value.try_to::<PackedInt64Array>()
                {
                    palette = packed
                        .as_slice()
                        .iter()
                        .map(|value| *value as u32)
                        .collect();
                }
                break;
            }
        }
    }
    let mut pixels: Vec<u8> = Vec::new();
    if host.has_method("logic_canvas_pixels")
        && let Ok(data) = host
            .call("logic_canvas_pixels", &[x.to_variant(), y.to_variant()])
            .try_to::<PackedByteArray>()
        && data.len() == (size * size) as usize
    {
        pixels = data.as_slice().to_vec();
    }
    if pixels.is_empty() {
        pixels = vec![0; (size * size) as usize];
    }
    {
        let mut st = state.borrow_mut();
        st.bpp = bpp;
        st.modified = false;
        st.save_timer = 0.0;
        st.last_cell = (-1, -1);
        st.cur_color = st.cur_color.clamp(0, (palette.len() as i64 - 1).max(0));
        let mut shared = st.shared.borrow_mut();
        shared.size = size;
        shared.palette = palette;
        shared.pixels = pixels;
    }
    refresh_texture(state);
    rebuild_swatches(state);
    canvas_redraw(state);
}

/// `_rebuild_swatches`: one tinted toggle per palette colour
/// (`CanvasEditDialog.java:194-207`).
fn rebuild_swatches(state: &Rc<RefCell<CanvasEditState>>) {
    let Some(mut palette_row) = state.borrow().palette_row.clone() else {
        return;
    };
    for child in palette_row.get_children().iter_shared() {
        let mut child = child.clone();
        child.queue_free();
    }
    state.borrow_mut().swatches.clear();
    let (palette, cur_color) = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        (shared.palette.clone(), st.cur_color)
    };
    for (index, packed) in palette.iter().enumerate() {
        // code-instantiated: one palette swatch per canvas palette colour.
        let mut button = Button::new_alloc();
        button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        button.set_toggle_mode(true);
        button.set_theme_type_variation("squareTogglei");
        button.set_pressed_no_signal(index as i64 == cur_color);
        button.set_modulate(rgba8_color(i64::from(*packed)));
        {
            let state_for = state.clone();
            let callable = Callable::from_fn("canvas_edit_swatch", move |_args: &[&Variant]| {
                select_color(&state_for, index as i64);
            });
            button.connect("pressed", &callable);
        }
        palette_row.add_child(&button);
        state.borrow_mut().swatches.push(button);
    }
}

/// `_select_color`.
fn select_color(state: &Rc<RefCell<CanvasEditState>>, index: i64) {
    state.borrow_mut().cur_color = index;
    refresh_swatches(state);
}

/// `_refresh_swatches`.
fn refresh_swatches(state: &Rc<RefCell<CanvasEditState>>) {
    let (swatches, cur_color) = {
        let st = state.borrow();
        (st.swatches.clone(), st.cur_color)
    };
    for (index, button) in swatches.iter().enumerate() {
        let mut button = button.clone();
        button.set_pressed_no_signal(index as i64 == cur_color);
    }
}

/// `packPixmap`: LSB-first `bpp`-bit packing (`CanvasBlock.java:216-227`).
fn pack_pixels(state: &Rc<RefCell<CanvasEditState>>) -> PackedByteArray {
    let (size, pixels, bpp) = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        (shared.size, shared.pixels.clone(), st.bpp)
    };
    let count = (size * size).max(0) as usize;
    let bytes = ((size * size) as i64 * bpp + 7) / 8;
    let mut packed = PackedByteArray::new();
    packed.resize(bytes.max(0) as usize);
    let bpp = bpp.max(1);
    let bytes_len = packed.len();
    for pos in 0..count {
        let value = i64::from(pixels.get(pos).copied().unwrap_or(0));
        let bit_offset = pos as i64 * bpp;
        for bit in 0..bpp {
            let word = ((bit_offset + bit) >> 3) as usize;
            let mask = 1i64 << ((bit_offset + bit) & 7);
            if word >= bytes_len {
                continue;
            }
            let current = i64::from(packed.as_slice()[word]);
            let updated = if (value >> bit) & 1 != 0 {
                current | mask
            } else {
                current & !mask
            };
            packed.as_mut_slice()[word] = updated as u8;
        }
    }
    packed
}

/// `CanvasEditDialog.save`: `canvas.configure(canvas.packPixmap(pix))`.
fn canvas_save(state: &Rc<RefCell<CanvasEditState>>) {
    if !state.borrow().modified {
        return;
    }
    let (x, y) = {
        let st = state.borrow();
        (st.x, st.y)
    };
    let Some(mut input) = kit::node("/root/Spine/Input") else {
        return;
    };
    if !input.has_method("configure_building_bytes") {
        return;
    }
    let packed = pack_pixels(state);
    let _ = input.call(
        "configure_building_bytes",
        &[x.to_variant(), y.to_variant(), packed.to_variant()],
    );
    state.borrow_mut().modified = false;
}

/// `@export`: `file.writePng(pix)` (`CanvasEditDialog.java:216-222`).
fn canvas_export_png(dialog: &Gd<MindDialog>, state: &Rc<RefCell<CanvasEditState>>) {
    // code-instantiated: `@export` opens a runtime native save dialog.
    let mut file_dialog = FileDialog::new_alloc();
    file_dialog.set_file_mode(FileMode::SAVE_FILE);
    file_dialog.set_access(Access::FILESYSTEM);
    file_dialog.add_filter(&GString::from("*.png"));
    let state_for = state.clone();
    let target = file_dialog.clone();
    let callable = Callable::from_fn("canvas_export_selected", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut target = target.clone();
        target.queue_free();
        if let Some(image) = pixels_image(&state_for) {
            let error = image.save_png(&GString::from(path.as_str()));
            if error != godot::global::Error::OK {
                log::warn!("[ui] canvas export `{path}`: {error:?}");
            }
        }
    });
    file_dialog.connect("file_selected", &callable);
    let canceled_target = file_dialog.clone();
    let callable = Callable::from_fn("canvas_export_canceled", move |_args: &[&Variant]| {
        let mut target = canceled_target.clone();
        target.queue_free();
    });
    file_dialog.connect("canceled", &callable);
    let mut node: Gd<Node> = dialog.clone().upcast();
    node.add_child(&file_dialog);
    file_dialog.popup_centered();
}

/// `@import`: opens the native file browser, then [`canvas_import_image`].
fn canvas_import_png(dialog: &Gd<MindDialog>, state: &Rc<RefCell<CanvasEditState>>) {
    // code-instantiated: `@import` opens a runtime native open dialog.
    let mut file_dialog = FileDialog::new_alloc();
    file_dialog.set_file_mode(FileMode::OPEN_FILE);
    file_dialog.set_access(Access::FILESYSTEM);
    file_dialog.add_filter(&GString::from("*.png,*.jpg,*.jpeg"));
    let state_for = state.clone();
    let target = file_dialog.clone();
    let callable = Callable::from_fn("canvas_import_selected", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut target = target.clone();
        target.queue_free();
        canvas_import_image(&state_for, &path);
    });
    file_dialog.connect("file_selected", &callable);
    let canceled_target = file_dialog.clone();
    let callable = Callable::from_fn("canvas_import_canceled", move |_args: &[&Variant]| {
        let mut target = canceled_target.clone();
        target.queue_free();
    });
    file_dialog.connect("canceled", &callable);
    let mut node: Gd<Node> = dialog.clone().upcast();
    node.add_child(&file_dialog);
    file_dialog.popup_centered();
}

/// `_import_image`: decode, scale/centre and map every pixel to the nearest
/// palette entry (`CanvasEditDialog.java:224-274`).
fn canvas_import_image(state: &Rc<RefCell<CanvasEditState>>, path: &str) {
    let Some(mut image) = Image::load_from_file(&GString::from(path)) else {
        return;
    };
    image.convert(Format::RGBA8);
    let (target, palette) = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        (shared.size, shared.palette.clone())
    };
    if target <= 0 {
        return;
    }
    // Scale preserving aspect when larger, else fill with palette[0] and centre.
    let base = palette
        .first()
        .map(|packed| rgba8_color(i64::from(*packed)))
        .unwrap_or(Color::BLACK);
    let source_width = image.get_width().max(1);
    let source_height = image.get_height().max(1);
    let mut scaled_width = source_width;
    let mut scaled_height = source_height;
    if source_width > target || source_height > target {
        let ratio = (target as f64 / source_width as f64).min(target as f64 / source_height as f64);
        scaled_width = ((source_width as f64 * ratio).round() as i32).max(1);
        scaled_height = ((source_height as f64 * ratio).round() as i32).max(1);
        image
            .resize_ex(scaled_width, scaled_height)
            .interpolation(Interpolation::BILINEAR)
            .done();
    }
    let origin_x = (target - scaled_width) / 2;
    let origin_y = (target - scaled_height) / 2;
    let mut pixels = vec![0u8; (target * target) as usize];
    for y in 0..target {
        for x in 0..target {
            let source_x = x - origin_x;
            let source_y = y - origin_y;
            let mut color = base;
            if source_x >= 0 && source_y >= 0 && source_x < scaled_width && source_y < scaled_height
            {
                let pixel = image.get_pixel(source_x, source_y);
                // `findClosest`: blend the source alpha over palette[0].
                color = if pixel.a < 1.0 {
                    base.blend(pixel)
                } else {
                    pixel
                };
            }
            pixels[(y * target + x) as usize] = nearest_palette(&palette, color) as u8;
        }
    }
    state.borrow().shared.borrow_mut().pixels = pixels;
    state.borrow_mut().modified = true;
    refresh_texture(state);
    canvas_redraw(state);
}

/// `_pixels_image`: the palette-mapped RGBA8 image.
fn pixels_image(state: &Rc<RefCell<CanvasEditState>>) -> Option<Gd<Image>> {
    let (size, palette, pixels) = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        (shared.size, shared.palette.clone(), shared.pixels.clone())
    };
    if size <= 0 {
        return None;
    }
    let mut image = Image::create(size, size, false, Format::RGBA8)?;
    for y in 0..size {
        for x in 0..size {
            let index = (y * size + x) as usize;
            let palette_index = pixels.get(index).copied().unwrap_or(0) as usize;
            let color = palette
                .get(palette_index)
                .map(|packed| rgba8_color(i64::from(*packed)))
                .unwrap_or(Color::BLACK);
            image.set_pixel(x, y, color);
        }
    }
    Some(image)
}

/// `_nearest_palette`: the closest palette entry by squared RGB distance.
fn nearest_palette(palette: &[u32], color: Color) -> i64 {
    let mut best = 0i64;
    let mut best_distance = f64::INFINITY;
    for (index, packed) in palette.iter().enumerate() {
        let candidate = rgba8_color(i64::from(*packed));
        let distance = f64::from(color.r - candidate.r).powi(2)
            + f64::from(color.g - candidate.g).powi(2)
            + f64::from(color.b - candidate.b).powi(2);
        if distance < best_distance {
            best_distance = distance;
            best = index as i64;
        }
    }
    best
}

/// `_refresh_texture`: rebuilds the pixel texture from `_pixels` (top row first).
fn refresh_texture(state: &Rc<RefCell<CanvasEditState>>) {
    let Some(image) = pixels_image(state) else {
        return;
    };
    let texture = ImageTexture::create_from_image(&image);
    state.borrow().shared.borrow_mut().texture = texture;
    canvas_redraw(state);
}

/// `_set_pixel_texture`: one pixel of the live texture.
fn set_pixel_texture(state: &Rc<RefCell<CanvasEditState>>, x: i32, y: i32) {
    let (texture, palette, pixels, size) = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        (
            shared.texture.clone(),
            shared.palette.clone(),
            shared.pixels.clone(),
            shared.size,
        )
    };
    let Some(mut texture) = texture else {
        return;
    };
    let Some(mut image) = texture.get_image() else {
        return;
    };
    let index = (y * size + x) as usize;
    let palette_index = pixels.get(index).copied().unwrap_or(0) as usize;
    let color = palette
        .get(palette_index)
        .map(|packed| rgba8_color(i64::from(*packed)))
        .unwrap_or(Color::BLACK);
    image.set_pixel(x, y, color);
    texture.update(&image);
    canvas_redraw(state);
}

/// `_cell_size`: the screen size of one canvas cell.
fn cell_size(shared: &CanvasSurfaceState, rect: Vector2) -> Vector2 {
    if shared.size <= 0 {
        return Vector2::ONE;
    }
    Vector2::new(
        (rect.x / shared.size as f32).max(1.0),
        (rect.y / shared.size as f32).max(1.0),
    )
}

/// `convertX`/`convertY` (`:78-84`): screen position -> canvas cell (row 0 = top).
fn cell_at(shared: &CanvasSurfaceState, rect: Vector2, position: Vector2) -> (i32, i32) {
    let cell = cell_size(shared, rect);
    ((position.x / cell.x) as i32, (position.y / cell.y) as i32)
}

/// `_index_at`.
fn index_at(size: i32, cell: (i32, i32)) -> Option<usize> {
    if cell.0 < 0 || cell.1 < 0 || cell.0 >= size || cell.1 >= size {
        return None;
    }
    Some((cell.1 * size + cell.0) as usize)
}

/// `draw(x, y)`: set when in bounds and different (`:138-144`).
fn canvas_paint(state: &Rc<RefCell<CanvasEditState>>, cell: (i32, i32)) {
    let (size, cur_color) = {
        let st = state.borrow();
        (st.shared.borrow().size, st.cur_color)
    };
    let Some(index) = index_at(size, cell) else {
        return;
    };
    {
        let st = state.borrow();
        let mut shared = st.shared.borrow_mut();
        if index >= shared.pixels.len() || i64::from(shared.pixels[index]) == cur_color {
            return;
        }
        shared.pixels[index] = cur_color as u8;
    }
    state.borrow_mut().modified = true;
    set_pixel_texture(state, cell.0, cell.1);
}

/// `fill` mode flood fill (`:94-114`).
fn canvas_flood_fill(state: &Rc<RefCell<CanvasEditState>>, cell: (i32, i32)) {
    let (size, cur_color) = {
        let st = state.borrow();
        (st.shared.borrow().size, st.cur_color)
    };
    let Some(start) = index_at(size, cell) else {
        return;
    };
    let mut stack = vec![start];
    {
        let st = state.borrow();
        let mut shared = st.shared.borrow_mut();
        let Some(&target) = shared.pixels.get(start) else {
            return;
        };
        if i64::from(target) == cur_color {
            return;
        }
        while let Some(pos) = stack.pop() {
            if pos >= shared.pixels.len() || shared.pixels[pos] != target {
                continue;
            }
            shared.pixels[pos] = cur_color as u8;
            let cx = (pos as i32) % size;
            let cy = (pos as i32) / size;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= size || ny >= size {
                    continue;
                }
                stack.push((ny * size + nx) as usize);
            }
        }
    }
    state.borrow_mut().modified = true;
    refresh_texture(state);
}

/// `Bresenham2.line`: the cells between two canvas cells.
fn line_cells(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    let mut x = from.0;
    let mut y = from.1;
    let dx = (to.0 - x).abs();
    let dy = (to.1 - y).abs();
    let sx = if x < to.0 { 1 } else { -1 };
    let sy = if y < to.1 { 1 } else { -1 };
    let mut err = dx - dy;
    loop {
        cells.push((x, y));
        if x == to.0 && y == to.1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
    cells
}

/// Paint surface input (`CanvasEditDialog`'s `InputListener`; `:87-135`).
fn canvas_input(state: &Rc<RefCell<CanvasEditState>>, event: &Gd<InputEvent>) {
    let Some(rect) = state
        .borrow()
        .surface
        .clone()
        .map(|surface| surface.get_rect().size)
    else {
        return;
    };
    if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
        let cell = {
            let st = state.borrow();
            let shared = st.shared.borrow();
            cell_at(&shared, rect, button.get_position())
        };
        let index = button.get_button_index();
        let pressed = button.is_pressed();
        if index == MouseButton::LEFT && pressed {
            let fill = state.borrow().fill;
            if fill {
                canvas_flood_fill(state, cell);
            } else {
                canvas_paint(state, cell);
                state.borrow_mut().last_cell = cell;
            }
            canvas_redraw(state);
        } else if index == MouseButton::MIDDLE && pressed {
            let (size, pixels) = {
                let st = state.borrow();
                let shared = st.shared.borrow();
                (shared.size, shared.pixels.clone())
            };
            if let Some(position) = index_at(size, cell)
                && let Some(value) = pixels.get(position)
            {
                state.borrow_mut().cur_color = i64::from(*value);
                refresh_swatches(state);
            }
        }
        return;
    }
    let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() else {
        return;
    };
    let last_cell = state.borrow().last_cell;
    if last_cell.0 < 0 {
        return;
    }
    if motion.get_button_mask().ord() & MouseButtonMask::LEFT.ord() == 0 {
        return;
    }
    if state.borrow().fill {
        return;
    }
    let target = {
        let st = state.borrow();
        let shared = st.shared.borrow();
        cell_at(&shared, rect, motion.get_position())
    };
    // `Bresenham2.line(lastX, lastY, cx, cy, this::draw)`.
    for cell in line_cells(last_cell, target) {
        canvas_paint(state, cell);
    }
    state.borrow_mut().last_cell = target;
    canvas_redraw(state);
}

/// Queues a redraw of the paint surface.
fn canvas_redraw(state: &Rc<RefCell<CanvasEditState>>) {
    let surface = state.borrow().surface.clone();
    if let Some(mut surface) = surface {
        surface.queue_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CanvasEditDialog.java:185`: the desktop paint surface is 480 px (the
    /// port used 300 before L07 T5).
    #[test]
    fn canvas_surface_is_480_px() {
        assert_eq!(CANVAS_EXTENT, 480.0);
        // `CanvasEditDialog.update`: `if(time >= refreshTime)` with
        // `refreshTime = 60 * 2` ticks -> a 2 s auto-save at 60 Hz.
        assert_eq!(CANVAS_SAVE_TIME, 120.0);
        // `Blocks.canvas` (`Blocks.java:6957`) sets `canvasSize = 12`; the
        // port's fallback must match the flushed sync payload.
        assert_eq!(CANVAS_DEFAULT_SIZE, 12);
        assert_eq!(CANVAS_DEFAULT_BPP, 3);
    }

    /// `_nearest_palette`: the closest palette entry by squared RGB distance
    /// (`CanvasEditDialog.java:224-274`'s `findClosest`).
    #[test]
    fn nearest_palette_picks_exact_and_closest_entries() {
        // Packed RGBA8888: red, green, blue.
        let palette = [0xff00_00ffu32, 0x00ff_00ff, 0x0000_ffff];
        let red = Color::from_rgba8(0xff, 0, 0, 0xff);
        let green = Color::from_rgba8(0, 0xff, 0, 0xff);
        let blue = Color::from_rgba8(0, 0, 0xff, 0xff);
        assert_eq!(nearest_palette(&palette, red), 0);
        assert_eq!(nearest_palette(&palette, green), 1);
        assert_eq!(nearest_palette(&palette, blue), 2);
        // A dark red is closer to red than to green/blue.
        assert_eq!(
            nearest_palette(&palette, Color::from_rgba8(0x80, 0, 0, 0xff)),
            0
        );
        // Alpha is ignored (squared RGB distance only).
        assert_eq!(
            nearest_palette(&palette, Color::from_rgba8(0, 0xff, 0, 0x00)),
            1
        );
        // An empty palette falls back to 0 (never panics).
        assert_eq!(nearest_palette(&[], red), 0);
    }

    /// `convertX`/`convertY` (`CanvasEditDialog.java:78-84`): a screen point
    /// maps to a cell by the surface/size ratio, and `_index_at` clamps cells
    /// outside the canvas.
    #[test]
    fn canvas_cell_and_index_math_matches_java() {
        let shared = CanvasSurfaceState {
            size: 12,
            ..CanvasSurfaceState::default()
        };
        let rect = Vector2::new(480.0, 480.0);
        assert_eq!(cell_at(&shared, rect, Vector2::new(0.0, 0.0)), (0, 0));
        assert_eq!(cell_at(&shared, rect, Vector2::new(479.0, 479.0)), (11, 11));
        assert_eq!(cell_at(&shared, rect, Vector2::new(40.0, 80.0)), (1, 2));
        // Java's `(int)` cast truncates toward zero.
        assert_eq!(cell_at(&shared, rect, Vector2::new(-1.0, 0.0)).0, 0);
        assert_eq!(cell_at(&shared, rect, Vector2::new(-41.0, 0.0)).0, -1);
        assert_eq!(index_at(12, (0, 0)), Some(0));
        assert_eq!(index_at(12, (11, 0)), Some(11));
        assert_eq!(index_at(12, (0, 1)), Some(12));
        assert_eq!(index_at(12, (12, 0)), None);
        assert_eq!(index_at(12, (0, 12)), None);
        assert_eq!(index_at(12, (-1, 0)), None);
    }
}

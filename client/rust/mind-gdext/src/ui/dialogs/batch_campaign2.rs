// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/ResearchDialog.java,
//         core/src/mindustry/ui/dialogs/SchematicsDialog.java,
//         core/src/mindustry/ui/dialogs/LoadoutDialog.java,
//         core/src/mindustry/ui/dialogs/LaunchLoadoutDialog.java,
//         core/src/mindustry/ui/dialogs/PlanetDialog.java (`showStats`),
//         core/src/mindustry/ui/dialogs/CustomGameDialog.java,
//         core/src/mindustry/ui/dialogs/MapListDialog.java,
//         core/src/mindustry/ui/dialogs/MapPlayDialog.java.
//
//! Campaign dialog controllers, batch 2: research, schematics, loadout,
//! launch loadout, sector stats, custom game and map play.
//!
//! Each controller binds the data-driven part of one `client/ui/dialogs/*.gd`
//! script to the shared [`MindDialog`] shell. Static structure lives in the
//! dialog scenes; every row here is rebuilt from the `MindCampaign` read models
//! (`campaign_views_json`, `research_tree_json`, `launch_dialog_json`) and the
//! `MindPreview` map registry, so the controllers stay UI-only. The tech-tree
//! canvas, the `@globalitems` pool and the schematic previews are the native
//! kit widgets (`MindResearchView`, `MindResearchItemsDisplay`,
//! `MindSchematicImage`).

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use godot::builtin::{Side, VarArray, VarDictionary, VariantType};
use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::{AutowrapMode, OverrunBehavior};
use godot::classes::{
    Button, CanvasItem, ColorRect, Control, DisplayServer, Font, FontVariation, HBoxContainer,
    Json, Label, LineEdit, MarginContainer, Node, PanelContainer, RichTextLabel, StyleBox,
    StyleBoxEmpty, StyleBoxFlat, StyleBoxTexture, Texture2D, TextureRect, VBoxContainer,
};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::widgets::research_items_display::MindResearchItemsDisplay;
use super::super::kit::widgets::research_view::MindResearchView;
use super::super::kit::widgets::schematic_image::MindSchematicImage;
use super::super::kit::{self, MindLabel, MindScroll, MindTable, styles};
use crate::editor::MindPreview;

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "research" => Some(Box::new(ResearchDialog::default())),
        "schematics" => Some(Box::new(SchematicsDialog::default())),
        "loadout" => Some(Box::new(LoadoutDialog::default())),
        "launch_loadout" => Some(Box::new(LaunchLoadoutDialog::default())),
        "sector_stats" => Some(Box::new(SectorStatsDialog::default())),
        "custom" => Some(Box::new(CustomGameDialog::default())),
        "map_play" => Some(Box::new(MapPlayDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// `Pal.gray` / `Tex.pane` outline (#454545) used by the pane buttons.
const OUTLINE_GRAY: Color = Color::from_rgba(0.27, 0.27, 0.27, 1.0);
/// `Pal.lightGray` sampled from the reference frames (sector stats cells).
const LIGHT_GRAY: Color = Color::from_rgba(0.62, 0.62, 0.62, 1.0);
/// `Color(0.75, 0.75, 0.75)` (research requirement text).
const TEXT_GRAY: Color = Color::from_rgba(0.75, 0.75, 0.75, 1.0);
/// `Pal.remove` (`Color(1.0, 0.204, 0.11)`).
const PAL_REMOVE: Color = Color::from_rgba(1.0, 0.204, 0.11, 1.0);
/// `Pal.gray` (#454545) drawable colour.
const PAL_GRAY: Color = Color::from_rgba(0.271, 0.271, 0.271, 1.0);
/// `Pal.darkestGray` (`Styles.grayPanel`, the `grayt` up state).
const PAL_DARKEST_GRAY: Color = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
/// The live map registry (`editor/preview.rs`).
const PREVIEW_PATH: &str = "/root/Spine/MindPreview";
/// The `UiRoot` shell (`/root/Spine/Ui/UiRoot`).
const UI_ROOT_PATH: &str = "/root/Spine/Ui/UiRoot";

/// A rich label's engine handle (text/color/alignment live on `RichTextLabel`).
fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

/// Reads a JSON string field (`""` when missing or null).
fn row_str(row: &VarDictionary, key: &str) -> String {
    row_opt_str(row, key).unwrap_or_default()
}

/// Reads a JSON string field (`None` when missing or null).
fn row_opt_str(row: &VarDictionary, key: &str) -> Option<String> {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .map(|value| text_of(&value))
}

/// Reads a JSON boolean field (`false` when missing).
fn row_bool(row: &VarDictionary, key: &str) -> bool {
    row_bool_or(row, key, false)
}

/// Reads a JSON boolean field with a default.
fn row_bool_or(row: &VarDictionary, key: &str, default: bool) -> bool {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(default)
}

/// Reads a JSON integer field (`0` when missing; floats truncate).
fn row_i64(row: &VarDictionary, key: &str) -> i64 {
    row_i64_or(row, key, 0)
}

/// Reads a JSON integer field with a default.
fn row_i64_or(row: &VarDictionary, key: &str, default: i64) -> i64 {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .map(|value| int_of(&value))
        .unwrap_or(default)
}

/// Reads a JSON array field (empty when missing).
fn row_arr(row: &VarDictionary, key: &str) -> VarArray {
    row.get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
}

/// Reads a JSON object field (empty when missing).
fn row_dict(row: &VarDictionary, key: &str) -> VarDictionary {
    row.get(key)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// Reads the row dictionaries of a JSON array field.
fn row_rows(row: &VarDictionary, key: &str) -> Vec<VarDictionary> {
    row_arr(row, key)
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// Parses a JSON object (`{}` on any other JSON shape).
fn parse_object(text: &str) -> VarDictionary {
    if text.is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(&gs(text))
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// Parses a JSON array (`[]` on any other JSON shape).
fn parse_array(text: &str) -> VarArray {
    if text.is_empty() {
        return VarArray::new();
    }
    Json::parse_string(&gs(text))
        .try_to::<VarArray>()
        .unwrap_or_default()
}

/// `JSON.stringify(object)`.
fn json_stringify(object: &VarDictionary) -> String {
    Json::stringify(&object.to_variant()).to_string()
}

/// `JSON.stringify(array)`.
fn json_stringify_array(array: &VarArray) -> String {
    Json::stringify(&array.to_variant()).to_string()
}

/// `GString` from arbitrary text (gdext's `From` impl only accepts `&str`).
fn gs(text: impl AsRef<str>) -> GString {
    GString::from(text.as_ref())
}

/// GDScript `str(value)`.
fn text_of(value: &Variant) -> String {
    match value.get_type() {
        VariantType::NIL => String::new(),
        VariantType::BOOL => {
            if value.to::<bool>() {
                String::from("true")
            } else {
                String::from("false")
            }
        }
        VariantType::STRING => value
            .try_to::<GString>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        VariantType::STRING_NAME => value
            .try_to::<StringName>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        VariantType::INT => value
            .try_to::<i64>()
            .map(|number| number.to_string())
            .unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(format_godot_float)
            .unwrap_or_default(),
        _ => format!("{value}"),
    }
}

/// GDScript `str(float)`: whole numbers keep the `.0` suffix.
fn format_godot_float(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

/// GDScript `int(value)`: numbers, bools and numeric strings convert.
fn int_of(value: &Variant) -> i64 {
    match value.get_type() {
        VariantType::INT => value.try_to::<i64>().unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number as i64)
            .unwrap_or_default(),
        VariantType::BOOL => {
            if value.to::<bool>() {
                1
            } else {
                0
            }
        }
        VariantType::STRING | VariantType::STRING_NAME => {
            text_of(value).trim().parse().unwrap_or_default()
        }
        _ => 0,
    }
}

/// Calls a `MindUi` method when both the autoload and the method exist.
fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// Calls a `MindUi` method returning a bool (`false` when absent).
fn ui_bool(method: &str, args: &[Variant]) -> bool {
    ui_call(method, args)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(false)
}

/// `Core.settings.getString(key, default)` through the `MindUi` store.
fn setting_text(key: &str, default: &str) -> String {
    ui_call(
        "settings_get_text",
        &[gs(key).to_variant(), gs(default).to_variant()],
    )
    .and_then(|value| value.try_to::<GString>().ok())
    .map(|value| value.to_string())
    .unwrap_or_else(|| default.to_string())
}

/// `Core.settings.getInt(key, default)` through the `MindUi` store.
fn setting_get_int(key: &str, default: i64) -> i64 {
    ui_call(
        "settings_get_int",
        &[gs(key).to_variant(), default.to_variant()],
    )
    .and_then(|value| value.try_to::<i64>().ok())
    .unwrap_or(default)
}

/// `Core.settings.put(key, value)` (bool) through the `MindUi` store.
fn setting_put_flag(key: &str, value: bool) {
    let _ = ui_call(
        "settings_put_flag",
        &[gs(key).to_variant(), value.to_variant()],
    );
}

/// `Core.settings.put(key, value)` (string) through the `MindUi` store.
fn setting_put_text(key: &str, value: &str) {
    let _ = ui_call(
        "settings_put_text",
        &[gs(key).to_variant(), gs(value).to_variant()],
    );
}

/// `MindDialog.open_content_info`: opens the `content` dialog for a name.
fn open_content_info(content: &str) {
    if content.is_empty() {
        return;
    }
    let mut context = VarDictionary::new();
    context.set("content", &gs(content));
    let _ = ui_call(
        "open_dialog",
        &[
            gs("content").to_variant(),
            gs(json_stringify(&context)).to_variant(),
        ],
    );
}

/// Emit-only UI sound (`MindUi.play_ui_sound`).
fn play_ui_sound(name: &str) {
    let _ = ui_call("play_ui_sound", &[gs(name).to_variant()]);
}

/// Closes a registered dialog through `MindUi`.
fn close_dialog(name: &str) {
    let _ = ui_call("close_dialog", &[gs(name).to_variant()]);
}

/// Opens a registered dialog through `MindUi` with a JSON context.
fn open_dialog(name: &str, context: &VarDictionary) {
    let _ = ui_call(
        "open_dialog",
        &[
            gs(name).to_variant(),
            gs(json_stringify(context)).to_variant(),
        ],
    );
}

/// The live map registry node (`None` outside the game scene).
fn preview_node() -> Option<Gd<MindPreview>> {
    kit::node(PREVIEW_PATH)?.try_cast::<MindPreview>().ok()
}

/// The `UiRoot` shell (`/root/Spine/Ui/UiRoot`).
fn ui_root() -> Option<Gd<Node>> {
    kit::node(UI_ROOT_PATH)
}

/// One manifest dialog node through `UiRoot.dialog(name)`.
fn dialog_node(name: &str) -> Option<Gd<Node>> {
    let mut root = ui_root()?;
    if !root.has_method("dialog") {
        return None;
    }
    root.call("dialog", &[gs(name).to_variant()])
        .try_to::<Gd<Node>>()
        .ok()
}

/// `state.isGame()`: the dialog is open in a running match (`custom_rules`
/// uses the same `!= "menu"` phase test).
fn in_game() -> bool {
    let Some(mut host) = kit::sim_host() else {
        return false;
    };
    if !host.has_method("get_state") {
        return false;
    }
    host.call("get_state", &[]).to::<GString>() != "menu"
}

/// `control.input.useSchematic(s)` through `MindInput` (G08 T6b).
fn use_schematic(index: i64) -> bool {
    let Some(mut input) = kit::node("/root/Spine/Input") else {
        return false;
    };
    if !input.has_method("use_schematic") {
        return false;
    }
    input
        .call("use_schematic", &[index.to_variant()])
        .try_to::<bool>()
        .unwrap_or(false)
}

/// A sizing-only spacer cell (`Control()` with an expand flag).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn spacer(expand: bool, width: f32, height: f32) -> Gd<Control> {
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
fn glyph_control(name: &str, extent: f32, color: Color) -> Gd<Label> {
    factory::glyph_label(name, extent as i32, color)
}

/// `UnlockableContent.uiIcon` region chain (`item-<name>-ui` first).
fn content_texture(content_name: &str) -> Option<Gd<Texture2D>> {
    for prefix in ["item", "liquid", "block", "unit", "status"] {
        if let Some(texture) = kit::icon_texture(&format!("{prefix}-{content_name}-ui")) {
            return Some(texture.upcast());
        }
    }
    None
}

/// A fitted `TextureRect` for an atlas region (`res.image(uiIcon).size(n)`).
fn icon_rect(texture: &Gd<Texture2D>, extent: f32) -> Gd<TextureRect> {
    // code-instantiated: one content icon per data row.
    let mut image = TextureRect::new_alloc();
    image.set_texture(texture);
    image.set_custom_minimum_size(Vector2::new(extent, extent));
    image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    image.set_mouse_filter(MouseFilter::IGNORE);
    image
}

/// A `PanelContainer` carrying a stylebox override.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn panel_with_box(style: Gd<StyleBoxFlat>) -> Gd<PanelContainer> {
    // code-instantiated: themed frame built per data row.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &style);
    panel
}

/// `Strings.stripColors(text)` (`[#rrggbb]`/`[name]` tags removed) lowercased.
fn strip_color_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let characters: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    while index < characters.len() {
        let character = characters[index];
        if character != '[' {
            out.push(character);
            index += 1;
            continue;
        }
        let Some(close_offset) = characters[index..].iter().position(|value| *value == ']') else {
            out.push(character);
            index += 1;
            continue;
        };
        let inner: String = characters[index + 1..index + close_offset].iter().collect();
        let color_tag = inner
            .strip_prefix('#')
            .map(|hex| {
                (3..=8).contains(&hex.len()) && hex.chars().all(|value| value.is_ascii_hexdigit())
            })
            .unwrap_or(false);
        let name_tag = !inner.is_empty() && inner.chars().all(|value| value.is_ascii_lowercase());
        if color_tag || name_tag {
            index += close_offset + 1;
            continue;
        }
        out.push(character);
        index += 1;
    }
    out.to_lowercase()
}

/// Connects `callable` to `signal` once per dialog (`is_connected` cannot match
/// a `Callable::from_fn`, so the dialog node carries the latch).
fn connect_once(
    dialog: &mut MindDialog,
    key: &str,
    target: &Gd<Node>,
    signal: &str,
    callable: &Callable,
) {
    let meta = format!("_campaign2_{key}");
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

/// Runs `action` on the next frame (the GDScript `call_deferred` seams).
///
/// The queue is drained from a `SceneTree.process_frame` connection installed
/// by [`install_frame_tick`]; `MindDialog` owns no `_process` of its own.
// A boxed one-shot closure; naming the alias would not make the type simpler.
#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub(crate) struct Deferred(Rc<RefCell<Vec<Box<dyn FnMut()>>>>);

impl Deferred {
    fn push(&self, action: impl FnMut() + 'static) {
        self.0.borrow_mut().push(Box::new(action));
    }

    fn drain(&self) {
        let queued: Vec<Box<dyn FnMut()>> = self.0.borrow_mut().drain(..).collect();
        for mut action in queued {
            action();
        }
    }
}

/// Installs the per-frame hook once (deferred queue drain + `tick`).
pub(crate) fn install_frame_tick(
    dialog: &mut MindDialog,
    key: &str,
    deferred: Deferred,
    mut tick: impl FnMut() + 'static,
) {
    let mut tree = dialog.base().get_tree();
    let meta = format!("_campaign2_frame_{key}");
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
    let callable = Callable::from_fn("campaign2_frame", move |_args: &[&Variant]| {
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

/// `_t("@key")` with a fallback when the bundle has no entry.
fn bundle_or(key: &str, fallback: &str) -> String {
    let value = kit::t(&format!("@{key}")).to_string();
    if value.is_empty() || value == key {
        fallback.to_string()
    } else {
        value
    }
}

/// A `minimap`-style solid `StyleBoxFlat` (`_solid`).
fn solid_box(color: Color) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(color);
    box_
}

/// A map preview with the Arc `BorderImage` stroke (`MindBorderImage`).
fn border_preview(texture: &Gd<Texture2D>, extent: Vector2, thickness: f32) -> Gd<Control> {
    use super::super::kit::widgets::border_image::MindBorderImage;
    // code-instantiated: the preview texture comes from the map registry.
    let mut image = MindBorderImage::new_alloc();
    {
        let mut bound = image.bind_mut();
        bound.thickness = thickness;
        // `map.safeTexture()` is an Arc `Texture` with the default
        // `TextureFilter.nearest`: crisp pixels.
        bound.force_nearest = true;
    }
    let mut rect: Gd<TextureRect> = image.clone().upcast();
    rect.set_texture(texture);
    rect.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    rect.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    rect.set_custom_minimum_size(extent);
    let control: Gd<Control> = image.upcast();
    control
}

/// `MindWidgets.texture_for_name` through the live registry.
fn preview_texture(map_name: &str) -> Option<Gd<Texture2D>> {
    let mut preview = preview_node()?;
    preview.bind_mut().texture_for_name(gs(map_name))
}

/// `MindIcons.glyph` for a label's markup (`UI.formatIcons` icon tokens).
fn icon_glyph_markup(markup: &str) -> String {
    let characters: Vec<char> = markup.chars().collect();
    let mut out = String::with_capacity(markup.len());
    let mut cursor = 0usize;
    while cursor < characters.len() {
        let rest: String = characters[cursor..].iter().collect();
        let Some(start) = rest.find("[icon name=\"") else {
            out.extend(characters[cursor..].iter());
            break;
        };
        let token_start = cursor + start;
        let name_start = token_start + "[icon name=\"".len();
        let Some(name_len) = markup[name_start..].find('"') else {
            out.extend(characters[cursor..].iter());
            break;
        };
        let name_end = name_start + name_len;
        let Some(close_offset) = markup[name_end..].find("[/icon]") else {
            out.extend(characters[cursor..].iter());
            break;
        };
        let token_end = name_end + close_offset + "[/icon]".len();
        out.extend(characters[cursor..token_start].iter());
        let glyph = kit::glyph(&markup[name_start..name_end]);
        if glyph.is_empty() {
            out.push_str(&markup[token_start..token_end]);
        } else {
            out.push_str(&glyph.to_string());
        }
        cursor = token_end;
    }
    out
}

/// Gives a label the `MindAssets.icon_font()` as a fallback of its text font so
/// icon code points render inline (Arc injects the glyphs into every UI font).
fn apply_icon_fallback(label: &Gd<RichTextLabel>) {
    let Some(assets) = kit::assets() else {
        return;
    };
    let Some(icon_font) = assets.bind().icon_font() else {
        return;
    };
    if !label.is_inside_tree() {
        return;
    }
    let Some(base) = label.get_theme_font("normal_font") else {
        return;
    };
    // code-instantiated: per-label font wrapper adding the icon-font fallback.
    let mut variation = FontVariation::new_gd();
    variation.set_base_font(&base);
    let icon: Gd<Font> = icon_font.upcast();
    let mut fallbacks: Array<Gd<Font>> = Array::new();
    fallbacks.push(&icon);
    variation.set_fallbacks(&fallbacks);
    let mut label = label.clone();
    label.add_theme_font_override("normal_font", &variation);
}

/// `Strings.formatMillis` (`ARC/.../Strings.java:813`): `h:mm:ss`, hours
/// unpadded, minutes/seconds two digits (`SaveSlot.getPlayTime`).
fn format_millis(millis: i64) -> String {
    let sign_prefix = if millis < 0 { "-" } else { "" };
    let millis = millis.abs();
    format!(
        "{}{}:{:02}:{:02}",
        sign_prefix,
        millis / 3_600_000,
        (millis % 3_600_000) / 60_000,
        (millis % 60_000) / 1000
    )
}

/// `Styles.grayt` (`Styles.java:157-165`): `up = grayPanel`,
/// `over`/`down = flatOver` (#454545).
fn apply_grayt(button: &mut Gd<Button>) {
    button.add_theme_stylebox_override("normal", &solid_box(PAL_DARKEST_GRAY));
    button.add_theme_stylebox_override("hover", &solid_box(PAL_GRAY));
    button.add_theme_stylebox_override("pressed", &solid_box(PAL_GRAY));
    button.add_theme_stylebox_override("hover_pressed", &solid_box(PAL_GRAY));
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
}

/// `Styles.flatToggleMenut` (`Styles.java:210-219`).
fn apply_flat_toggle_menu(button: &mut Gd<Button>) {
    let mut over = StyleBoxFlat::new_gd();
    over.set_bg_color(Color::from_rgba(0.271, 0.271, 0.271, 1.0));
    let mut checked = StyleBoxFlat::new_gd();
    checked.set_bg_color(Color::BLACK);
    checked.set_border_color(styles::ACCENT);
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        checked.set_border_width(side, 4);
    }
    for corner in [
        godot::builtin::Corner::TOP_LEFT,
        godot::builtin::Corner::TOP_RIGHT,
        godot::builtin::Corner::BOTTOM_LEFT,
        godot::builtin::Corner::BOTTOM_RIGHT,
    ] {
        checked.set_corner_radius(corner, 2);
    }
    let mut disabled = StyleBoxFlat::new_gd();
    disabled.set_bg_color(Color::BLACK);
    button.add_theme_stylebox_override("normal", &StyleBoxEmpty::new_gd());
    button.add_theme_stylebox_override("hover", &over);
    button.add_theme_stylebox_override("pressed", &checked);
    button.add_theme_stylebox_override("hover_pressed", &checked);
    button.add_theme_stylebox_override("disabled", &disabled);
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
    button.add_theme_color_override("font_color", Color::WHITE);
    button.add_theme_color_override("font_hover_color", Color::WHITE);
    button.add_theme_color_override("font_pressed_color", Color::WHITE);
    button.add_theme_color_override("font_hover_pressed_color", Color::WHITE);
    button.add_theme_color_override("font_disabled_color", styles::ARC_GRAY);
}

/// Forces a themed toggle's visual state without running its listeners.
fn set_toggle(button: &Gd<Button>, pressed: bool) {
    let mut button = button.clone();
    button.set_pressed_no_signal(pressed);
}

// ---------------------------------------------------------------------------
// ResearchDialog
// ---------------------------------------------------------------------------

/// `iconMed` (`Scl.scl(32f)`): title icon, root rows and the balancing spacer.
const RESEARCH_ICON_SIZE: f32 = 32.0;

/// Controller state for the reused research dialog node.
#[derive(Default)]
struct ResearchState {
    view: Option<Gd<MindResearchView>>,
    title_button: Option<Gd<Button>>,
    title_icon_holder: Option<Gd<Control>>,
    title_label: Option<Gd<MindLabel>>,
    items_display: Option<Gd<MindResearchItemsDisplay>>,
    popup: Option<Gd<PanelContainer>>,
    hovered: i64,
    tree: VarDictionary,
    last_root: String,
    deferred: Deferred,
}

impl ResearchState {
    fn new() -> Self {
        Self {
            hovered: -1,
            ..Self::default()
        }
    }
}

/// The Java graph view over the `research_tree_json` read model.
pub struct ResearchDialog {
    state: Rc<RefCell<ResearchState>>,
}

impl Default for ResearchDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(ResearchState::new())),
        }
    }
}

impl DialogController for ResearchDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        research_ready(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        research_shown(dialog, &self.state);
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        research_hidden(dialog);
    }
}

fn research_ready(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    // Java: `super("")` + `titleTable.remove()`.
    dialog.title_text = GString::new();
    dialog.should_pause = true;
    dialog.refresh_title();
    // Java `BaseDialog`: `setFillParent(true)` + `margin(0f).marginBottom(8)`.
    dialog.fill_panel();
    // The shared panel's `defaultDialog` style would paint an opaque slate over
    // the dimmed planet page, so clear it and keep the 15 px bottom inset
    // (margin + the button cells' pad(3)).
    if let Some(panel_node) = dialog.base().get_node_or_null("Center/Panel")
        && let Ok(mut panel) = panel_node.try_cast::<Control>()
    {
        let mut empty = StyleBoxEmpty::new_gd();
        empty.set_content_margin(Side::BOTTOM, 15.0);
        panel.add_theme_stylebox_override("panel", &empty);
    }
    research_build(dialog, state);
    dialog.add_close_button(210.0);
    // Java: hide research, show the database (`Icon.book`).
    let target = dialog.to_gd();
    let callable = Callable::from_fn("research_database", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        research_open_database(&mut bound);
    });
    let text = kit::t("@database").to_string();
    dialog.add_button(&text, Some(callable), "book", 210.0);
    research_install_tick(dialog, state);
}

fn research_build(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    let Some(mut layout) = dialog
        .base()
        .get_node_or_null("Center/Panel/Layout")
        .and_then(|node| node.try_cast::<VBoxContainer>().ok())
    else {
        return;
    };
    // code-instantiated: the tree canvas replaces the generic scroll body
    // (Java `cont.stack(titleTable, view, itemDisplay)`).
    let mut view = MindResearchView::new_alloc();
    view.set_h_size_flags(SizeFlags::EXPAND_FILL);
    view.set_v_size_flags(SizeFlags::EXPAND_FILL);
    let view_control: Gd<Control> = view.clone().upcast();
    layout.add_child(&view_control);
    let mut view_node: Gd<Node> = view.clone().upcast();
    {
        let target = dialog.to_gd();
        let state_for_press = state.clone();
        let callable = Callable::from_fn("research_node_pressed", move |args: &[&Variant]| {
            let index = args.first().map(|value| (*value).to::<i64>()).unwrap_or(-1);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            research_on_node_pressed(&mut bound, &state_for_press, index);
        });
        view_node.connect("node_pressed", &callable);
    }
    {
        let target = dialog.to_gd();
        let state_for_hover = state.clone();
        let callable = Callable::from_fn("research_node_hovered", move |args: &[&Variant]| {
            let index = args.first().map(|value| (*value).to::<i64>()).unwrap_or(-1);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            research_on_node_hovered(&mut bound, &state_for_hover, index);
        });
        view_node.connect("node_hovered", &callable);
    }
    {
        let target = dialog.to_gd();
        let state_for_exit = state.clone();
        let callable = Callable::from_fn("research_node_exited", move |args: &[&Variant]| {
            let index = args.first().map(|value| (*value).to::<i64>()).unwrap_or(-1);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            research_on_node_exited(&mut bound, &state_for_exit, index);
        });
        view_node.connect("node_exited", &callable);
    }
    // code-instantiated: root-selector title button (Java `titleTable`, a stack
    // layer over the canvas; `titleTable.toFront()` keeps it on top).
    let mut title_button = Button::new_alloc();
    title_button.set_theme_type_variation("defaultt");
    title_button.set_visible(false);
    {
        let target = dialog.to_gd();
        let state_for_selector = state.clone();
        let callable = Callable::from_fn("research_root_selector", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            research_open_root_selector(&mut bound, &state_for_selector);
        });
        title_button.connect("pressed", &callable);
    }
    view.add_child(&title_button);
    title_button.set_anchors_preset(LayoutPreset::CENTER_TOP);
    title_button.set_offset(Side::LEFT, -150.0);
    title_button.set_offset(Side::RIGHT, 150.0);
    title_button.set_offset(Side::TOP, 4.0);
    title_button.set_offset(Side::BOTTOM, 64.0);
    research_build_title_row(dialog, state, &mut title_button);
    // code-instantiated: the global item pool overlays the canvas top-left.
    let mut items = MindResearchItemsDisplay::new_alloc();
    // The stack sits in `cont` under the Dialog's and cont's `pad(3)` cells:
    // the items box is at (6, 6).
    items.set_position(Vector2::new(6.0, 6.0));
    view.add_child(&items);
    if let Some(mut body) = dialog.body() {
        // The canvas is not a scroll body; keep it out of the ScrollContainer
        // (it must receive drags) and above the hidden Body in the VBox.
        let body_slot = body.get_index();
        layout.move_child(&view_node, body_slot);
        body.set_visible(false);
    }
    {
        let mut st = state.borrow_mut();
        st.view = Some(view);
        st.title_button = Some(title_button);
        st.items_display = Some(items);
    }
}

/// `titleTable.button` content: 32 px root icon, `padRight(8)`, a grow spacer,
/// the accent label, another grow spacer and a 32 px spacer so the label sits
/// centred; built once and retargeted per tree.
fn research_build_title_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    button: &mut Gd<Button>,
) {
    let _ = dialog;
    // code-instantiated: title row content is tree-driven.
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    row.add_theme_constant_override("separation", 0);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);
    {
        // The button's `Tex.button` background pads its cells (Arc
        // `Table.setBackground`), so the icon sits inside the border.
        let mut button_control: Gd<Control> = button.clone().upcast();
        let mut row_control: Gd<Control> = row.clone().upcast();
        factory::inset_to_content(&mut button_control, &mut row_control);
    }
    let button_for_theme = button.clone();
    let row_for_theme = row.clone();
    let callable = Callable::from_fn("research_title_inset", move |_args: &[&Variant]| {
        let mut button_control: Gd<Control> = button_for_theme.clone().upcast();
        let mut row_control: Gd<Control> = row_for_theme.clone().upcast();
        factory::inset_to_content(&mut button_control, &mut row_control);
    });
    button.connect("theme_changed", &callable);
    // code-instantiated: root icon follows the tree data (atlas region or the
    // `terrain` glyph for a sector root).
    let mut holder = Control::new_alloc();
    holder.set_custom_minimum_size(Vector2::new(RESEARCH_ICON_SIZE, RESEARCH_ICON_SIZE));
    holder.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    holder.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&holder);
    row.add_child(&research_title_spacer(8.0));
    row.add_child(&research_title_spacer(-1.0));
    let label = factory::label("");
    rich(&label).add_theme_color_override("default_color", styles::ACCENT);
    rich(&label).set_v_size_flags(SizeFlags::SHRINK_CENTER);
    rich(&label).set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&label);
    row.add_child(&research_title_spacer(-1.0));
    row.add_child(&research_title_spacer(RESEARCH_ICON_SIZE));
    let mut st = state.borrow_mut();
    st.title_icon_holder = Some(holder);
    st.title_label = Some(label);
}

/// Spacer cell for the title button row (`b.add().growX()` = -1).
fn research_title_spacer(size: f32) -> Gd<Control> {
    // code-instantiated: title row is data-driven (Java `b.add()` cells).
    let mut spacer = Control::new_alloc();
    spacer.set_mouse_filter(MouseFilter::IGNORE);
    if size < 0.0 {
        spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    } else {
        spacer.set_custom_minimum_size(Vector2::new(size, 0.0));
    }
    spacer
}

/// Retargets the title button on the displayed tree.
fn research_update_title_button(state: &Rc<RefCell<ResearchState>>) {
    let st = state.borrow();
    let (Some(mut holder), Some(label)) = (st.title_icon_holder.clone(), st.title_label.clone())
    else {
        return;
    };
    for child in holder.get_children().iter_shared() {
        let mut child = child;
        child.queue_free();
    }
    let icon_region = row_str(&st.tree, "root_icon");
    let type_name = row_str(&st.tree, "root_type");
    if let Some(mut icon) = factory::content_icon(&icon_region, &type_name, RESEARCH_ICON_SIZE) {
        icon.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        icon.set_mouse_filter(MouseFilter::IGNORE);
        holder.add_child(&icon);
    }
    rich(&label).set_text(&gs(row_str(&st.tree, "root_title")));
}

/// Java `titleTable.button` click: the `@techtree.select` picker.
fn research_open_root_selector(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    let (roots, selected) = {
        let st = state.borrow();
        (row_arr(&st.tree, "roots"), row_str(&st.tree, "root"))
    };
    if roots.len() <= 1 {
        return;
    }
    let Some(selector) = dialog_node("research_root_select") else {
        return;
    };
    if selector.has_signal("root_selected") {
        let target = dialog.to_gd();
        let state_for_pick = state.clone();
        let callable = Callable::from_fn("research_select_root", move |args: &[&Variant]| {
            let name = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            research_select_root(&mut bound, &state_for_pick, &name);
        });
        connect_once(
            dialog,
            "research_root_selected",
            &selector,
            "root_selected",
            &callable,
        );
    }
    let mut context = VarDictionary::new();
    context.set("roots", &roots.to_variant());
    context.set("selected", &gs(selected));
    open_dialog("research_root_select", &context);
}

/// `rebuildTree(node)`: switch tree, reset the view.
fn research_select_root(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    root_name: &str,
) {
    if root_name.is_empty() {
        return;
    }
    let (last_root, view) = {
        let st = state.borrow();
        (st.last_root.clone(), st.view.clone())
    };
    if root_name == last_root {
        research_update_title_button(state);
        return;
    }
    close_dialog("research_root_select");
    let tree = dialog.campaign_json("research_tree_json", &[gs(root_name).to_variant()]);
    if tree.is_empty() {
        return;
    }
    let Some(mut view) = view else {
        return;
    };
    view.bind_mut().set_tree(tree.clone());
    view.bind_mut().reset_view();
    {
        let mut st = state.borrow_mut();
        st.last_root = row_opt_str(&tree, "root").unwrap_or_else(|| root_name.to_string());
        st.tree = tree;
    }
    research_update_title_button(state);
}

/// `ResearchDialog.java:463-487`: `if(canSpend && locked) spend`.
fn research_on_node_pressed(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    index: i64,
) {
    let view = {
        let st = state.borrow();
        st.view.clone()
    };
    let Some(mut view) = view else {
        return;
    };
    let node = view.bind().node_at(index);
    if node.is_empty() {
        return;
    }
    let content = row_str(&node, "content");
    if content.is_empty() {
        return;
    }
    if !row_bool(&node, "locked") || !row_bool(&node, "can_spend") {
        return;
    }
    let Some(mut campaign) = dialog.campaign_node() else {
        return;
    };
    if !campaign.has_method("research") {
        return;
    }
    if !campaign
        .call("research", &[gs(content.as_str()).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false)
    {
        return;
    }
    // Requirement amounts that changed flash (`ResearchDialog.spend` shine).
    let mut previous_have = VarDictionary::new();
    for req_variant in row_arr(&node, "reqs").iter_shared() {
        let Ok(req) = req_variant.try_to::<VarDictionary>() else {
            continue;
        };
        previous_have.set(
            &gs(row_str(&req, "item")),
            &gs(row_text(&req, "have")).to_variant(),
        );
    }
    let was_locked = row_bool_or(&node, "locked", true);
    let last_root = {
        let st = state.borrow();
        st.last_root.clone()
    };
    // Lock state and layout positions change after a spend (Java `treeLayout`
    // runs from `unlock`); re-fetch the same tree.
    let tree = dialog.campaign_json("research_tree_json", &[gs(last_root).to_variant()]);
    if tree.is_empty() {
        return;
    }
    let mut shine = VarDictionary::new();
    let mut unlocked_now = false;
    for node_variant in row_arr(&tree, "nodes").iter_shared() {
        let Ok(refreshed) = node_variant.try_to::<VarDictionary>() else {
            continue;
        };
        if row_str(&refreshed, "content") != content {
            continue;
        }
        unlocked_now = !row_bool_or(&refreshed, "locked", true);
        for req_variant in row_arr(&refreshed, "reqs").iter_shared() {
            let Ok(req) = req_variant.try_to::<VarDictionary>() else {
                continue;
            };
            let item_name = row_str(&req, "item");
            let before = previous_have
                .get(&gs(item_name.as_str()))
                .map(|value| text_of(&value))
                .unwrap_or_default();
            if previous_have.contains_key(&gs(item_name.as_str()))
                && before != row_text(&req, "have")
            {
                shine.set(&gs(item_name), &true.to_variant());
            }
        }
        break;
    }
    view.bind_mut().set_tree(tree.clone());
    research_raise_overlays(state);
    {
        let st = state.borrow();
        if let Some(mut items) = st.items_display.clone() {
            items
                .bind_mut()
                .rebuild(row_arr(&tree, "items"), shine.clone());
        }
    }
    {
        let mut st = state.borrow_mut();
        st.tree = tree;
    }
    research_update_title_button(state);
    let hovered = {
        let st = state.borrow();
        st.hovered
    };
    if hovered >= 0 {
        research_rebuild_popup(dialog, state, &shine);
    }
    if was_locked && unlocked_now {
        // Java `unlock`: `Sounds.uiUnlock.play()`.
        play_ui_sound("uiUnlock");
    }
}

/// `ResearchDialog.java:490-501`: hover shows the info table for visible nodes.
fn research_on_node_hovered(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    index: i64,
) {
    let view = {
        let st = state.borrow();
        st.view.clone()
    };
    let Some(view) = view else {
        return;
    };
    let node = view.bind().node_at(index);
    if node.is_empty() || !row_bool_or(&node, "visible", true) {
        return;
    }
    state.borrow_mut().hovered = index;
    research_rebuild_popup(dialog, state, &VarDictionary::new());
}

fn research_on_node_exited(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    index: i64,
) {
    let hovered = {
        let st = state.borrow();
        st.hovered
    };
    if hovered != index {
        return;
    }
    let deferred = {
        let st = state.borrow();
        st.deferred.clone()
    };
    let target = dialog.to_gd();
    let state_for_clear = state.clone();
    deferred.push(move || {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        research_maybe_clear_popup(&mut bound, &state_for_clear);
    });
}

/// Keep the popup while the mouse is over it or the node button; else remove.
fn research_maybe_clear_popup(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    let hovered = {
        let st = state.borrow();
        st.hovered
    };
    if hovered < 0 {
        return;
    }
    let mouse = dialog
        .base()
        .get_viewport()
        .map(|viewport| viewport.get_mouse_position())
        .unwrap_or(Vector2::ZERO);
    let popup = {
        let st = state.borrow();
        st.popup.clone()
    };
    if let Some(popup) = popup {
        let popup_control: Gd<Control> = popup.upcast();
        if popup_control.get_global_rect().contains_point(mouse) {
            return;
        }
    }
    let view = {
        let st = state.borrow();
        st.view.clone()
    };
    if let Some(view) = view
        && view.bind().button_rect(hovered).contains_point(mouse)
    {
        return;
    }
    research_clear_popup(state);
}

fn research_clear_popup(state: &Rc<RefCell<ResearchState>>) {
    state.borrow_mut().hovered = -1;
    research_free_popup(state);
}

fn research_free_popup(state: &Rc<RefCell<ResearchState>>) {
    let popup = {
        let mut st = state.borrow_mut();
        st.popup.take()
    };
    if let Some(popup) = popup {
        let mut node: Gd<Node> = popup.upcast();
        node.queue_free();
    }
}

/// `View.rebuild` (`ResearchDialog.java:632-784`): the hover info table.
fn research_rebuild_popup(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    shine: &VarDictionary,
) {
    research_free_popup(state);
    let (hovered, view) = {
        let st = state.borrow();
        (st.hovered, st.view.clone())
    };
    if hovered < 0 {
        return;
    }
    let Some(view) = view else {
        return;
    };
    let node = view.bind().node_at(hovered);
    if node.is_empty() {
        return;
    }
    let selectable = row_bool(&node, "selectable");
    let locked = row_bool_or(&node, "locked", true);
    // code-instantiated: hover info table (Java `infoTable`).
    let mut popup = PanelContainer::new_alloc();
    let mut box_ = StyleBoxTexture::new_gd();
    if let Some(texture) = kit::icon_texture("button") {
        box_.set_texture(&texture);
    }
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        box_.set_texture_margin(side, 8.0);
    }
    let box_gd: Gd<StyleBox> = box_.upcast();
    popup.add_theme_stylebox_override("panel", &box_gd);
    popup.set_mouse_filter(MouseFilter::STOP);
    {
        let target = dialog.to_gd();
        let state_for_clear = state.clone();
        let callable = Callable::from_fn("research_popup_exited", move |_args: &[&Variant]| {
            let deferred = {
                let st = state_for_clear.borrow();
                st.deferred.clone()
            };
            let target_inner = target.clone();
            let state_inner = state_for_clear.clone();
            deferred.push(move || {
                let mut node = target_inner.clone();
                let mut bound = node.bind_mut();
                research_maybe_clear_popup(&mut bound, &state_inner);
            });
        });
        popup.connect("mouse_exited", &callable);
    }
    let mut view_node: Gd<Node> = view.upcast();
    view_node.add_child(&popup);
    state.borrow_mut().popup = Some(popup.clone());

    // code-instantiated: info row + description column (Java `infoTable.table`).
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 4);
    popup.add_child(&column);
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 6);
    column.add_child(&row);
    if selectable {
        // code-instantiated: `Icon.info` button (`ui.content.show`).
        let mut info = factory::glyph_button("info", "i", "flati");
        info.set_custom_minimum_size(Vector2::new(50.0, 0.0));
        let content = row_str(&node, "content");
        let callable = Callable::from_fn("research_info", move |_args: &[&Variant]| {
            open_content_info(&content);
        });
        info.connect("pressed", &callable);
        row.add_child(&info);
    }
    // code-instantiated: description column (`rebuild(shine)` desc table).
    let mut desc = VBoxContainer::new_alloc();
    desc.add_theme_constant_override("separation", 2);
    row.add_child(&desc);
    let name_text = if selectable {
        row_str(&node, "name")
    } else {
        String::from("???")
    };
    let name_label = factory::label(&name_text);
    if !selectable {
        rich(&name_label).add_theme_color_override("default_color", styles::ACCENT);
    }
    desc.add_child(&name_label);
    if locked {
        if selectable {
            research_build_requirement_rows(&mut desc, &node, shine);
        } else if !row_arr(&node, "objectives").is_empty() {
            // code-instantiated: incomplete objective rows (`@complete` block).
            let complete = factory::label(&kit::tm("@complete").to_string());
            desc.add_child(&complete);
            for objective in row_arr(&node, "objectives").iter_shared() {
                let mut objective_row = HBoxContainer::new_alloc();
                objective_row.add_theme_constant_override("separation", 3);
                let objective_label = factory::label(&format!("> {}", text_of(&objective)));
                rich(&objective_label).add_theme_color_override("default_color", TEXT_GRAY);
                objective_row.add_child(&objective_label);
                let glyph = glyph_control("cancel", 18.0, PAL_REMOVE);
                objective_row.add_child(&glyph);
                desc.add_child(&objective_row);
            }
        }
    } else {
        let completed = factory::label(&kit::tm("@completed").to_string());
        desc.add_child(&completed);
    }
    // `rebuild`: description wrap 270 px when longer than 20 chars.
    let description = row_str(&node, "description");
    if selectable && row_bool_or(&node, "inline_description", false) && !description.is_empty() {
        let description_label = factory::label(&description);
        let mut description_rich = rich(&description_label);
        description_rich.add_theme_color_override("default_color", TEXT_GRAY);
        if description.chars().count() > 20 {
            let mut minimum = description_rich.get_custom_minimum_size();
            minimum.x = 270.0;
            description_rich.set_custom_minimum_size(minimum);
            description_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
        }
        column.add_child(&description_label);
    }
    research_update_popup_position(state);
}

/// `ResearchDialog.java:682-736`: progress line + one row per unfinished stack.
fn research_build_requirement_rows(
    desc: &mut Gd<VBoxContainer>,
    node: &VarDictionary,
    shine: &VarDictionary,
) {
    let progress = row_i64_or(node, "progress", -1);
    if progress >= 0 {
        let text = kit::tm_format("@research.progress", &[progress.to_string().as_str()]);
        let progress_label = factory::label(&text.to_string());
        rich(&progress_label).add_theme_color_override("default_color", TEXT_GRAY);
        desc.add_child(&progress_label);
    }
    for req_variant in row_arr(node, "reqs").iter_shared() {
        let Ok(req) = req_variant.try_to::<VarDictionary>() else {
            continue;
        };
        let item_name = row_str(&req, "item");
        let has = row_bool_or(&req, "has", false);
        let target = if has { TEXT_GRAY } else { PAL_REMOVE };
        // code-instantiated: one requirement row (`list.image` + labels).
        let mut req_row = HBoxContainer::new_alloc();
        req_row.add_theme_constant_override("separation", 3);
        let mut icon = TextureRect::new_alloc();
        if let Some(texture) = kit::icon_texture(&format!("item-{item_name}-ui")) {
            icon.set_texture(&texture);
        }
        icon.set_custom_minimum_size(Vector2::new(24.0, 24.0));
        icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        req_row.add_child(&icon);
        let item_label_text = row_opt_str(&req, "name").unwrap_or_else(|| item_name.clone());
        let item_label = factory::label(&item_label_text);
        rich(&item_label).add_theme_color_override("default_color", TEXT_GRAY);
        req_row.add_child(&item_label);
        let have = row_opt_str(&req, "have").unwrap_or_else(|| String::from("0"));
        let need = row_opt_str(&req, "need").unwrap_or_else(|| String::from("0"));
        let amount_label = factory::label(&format!(" {have} / {need}"));
        rich(&amount_label).add_theme_color_override("default_color", target);
        if shine.contains_key(item_name.as_str()) {
            // `Actions.color(Pal.accent -> targetColor, 0.75s)`.
            let mut amount_canvas: Gd<CanvasItem> = amount_label.clone().upcast();
            amount_canvas.set_modulate(styles::ACCENT);
            let mut tween = amount_canvas.create_tween();
            tween.tween_property(&amount_canvas, "modulate", &Color::WHITE.to_variant(), 0.75);
        }
        req_row.add_child(&amount_label);
        desc.add_child(&req_row);
    }
}

fn research_update_popup_position(state: &Rc<RefCell<ResearchState>>) {
    let (view, popup, hovered) = {
        let st = state.borrow();
        (st.view.clone(), st.popup.clone(), st.hovered)
    };
    let (Some(view), Some(mut popup)) = (view, popup) else {
        return;
    };
    if hovered < 0 {
        return;
    }
    // Java places it at the button's top-right (`Align.topLeft`, y-up): Godot
    // top-right corner = (right, top). Controls are unrotated/unscaled here, so
    // view-local = global - view.global_position.
    let rect = view.bind().button_rect(hovered);
    let corner = Vector2::new(rect.position.x + rect.size.x, rect.position.y);
    let view_control: Gd<Control> = view.upcast();
    popup.set_position(corner - view_control.get_global_position());
}

/// The `@globalitems` pool and the title button draw above the node buttons
/// (`view.move_child(_items_display, -1)` / `titleTable.toFront()`).
fn research_raise_overlays(state: &Rc<RefCell<ResearchState>>) {
    let st = state.borrow();
    let Some(view) = st.view.clone() else {
        return;
    };
    let mut view_node: Gd<Node> = view.upcast();
    if let Some(items) = st.items_display.clone() {
        let items_node: Gd<Node> = items.upcast();
        view_node.move_child(&items_node, -1);
    }
    if let Some(button) = st.title_button.clone() {
        let button_node: Gd<Node> = button.upcast();
        view_node.move_child(&button_node, -1);
    }
}

fn research_install_tick(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    let deferred = {
        let st = state.borrow();
        st.deferred.clone()
    };
    let state_for_tick = state.clone();
    install_frame_tick(dialog, "research", deferred, move || {
        let has_popup = {
            let st = state_for_tick.borrow();
            st.popup.is_some()
        };
        if !has_popup {
            return;
        }
        research_update_popup_position(&state_for_tick);
    });
}

fn research_shown(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    // Java `shown`: the planet page's planet, else the played sector's planet.
    let planet = dialog.context_string("planet").to_string();
    let tree = dialog.campaign_json("research_tree_json", &[gs(planet).to_variant()]);
    if tree.is_empty() {
        return;
    }
    let view = {
        let st = state.borrow();
        st.view.clone()
    };
    let Some(mut view) = view else {
        return;
    };
    view.bind_mut().set_tree(tree.clone());
    research_raise_overlays(state);
    let view_control: Gd<Control> = view.clone().upcast();
    let view_height = view_control.get_size().y;
    {
        let st = state.borrow();
        if let Some(mut items) = st.items_display.clone() {
            items.bind_mut().max_height = 60.0f32.max(view_height - 120.0);
            items.bind_mut().rebuild_items(row_arr(&tree, "items"));
        }
    }
    let root_name = row_str(&tree, "root");
    let last_root = {
        let st = state.borrow();
        st.last_root.clone()
    };
    if root_name != last_root {
        // Java keeps pan between opens of the same tree, resets on a switch.
        view.bind_mut().reset_view();
        state.borrow_mut().last_root = root_name;
    }
    {
        let mut st = state.borrow_mut();
        st.tree = tree.clone();
    }
    research_update_title_button(state);
    let visible = !row_str(&tree, "root").is_empty();
    if let Some(mut button) = state.borrow().title_button.clone() {
        button.set_visible(visible);
    }
}

fn research_open_database(dialog: &mut MindDialog) {
    let _ = dialog;
    // Java: hide research, show the database.
    close_dialog("research");
    open_dialog("database", &VarDictionary::new());
}

fn research_hidden(dialog: &mut MindDialog) {
    let _ = dialog;
    // Java `hidden(ui.planet::setup)`: refresh the planet page underneath.
    if !ui_bool("is_dialog_shown", &[gs("planet").to_variant()]) {
        return;
    }
    let Some(mut planet) = dialog_node("planet") else {
        return;
    };
    if planet.has_method("refresh_page") {
        planet.call_deferred("refresh_page", &[]);
    }
}

/// `row.get(key, "")` coerced to a string (GDScript `str(...)`).
fn row_text(row: &VarDictionary, key: &str) -> String {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .map(|value| text_of(&value))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// SchematicsDialog
// ---------------------------------------------------------------------------

/// `SchematicsDialog.ignoreSymbols` (the regex symbol class).
const IGNORE_SYMBOLS: &str = "`~!@#$%^&*()-_=+{}|;:'\",<.>/?";
/// `SchematicsDialog.tagh` (tag button height).
const TAG_HEIGHT: f32 = 42.0;
/// `rebuildPane`: `cols = max((int)(screenW / 230), 1)`.
const SCHEMATIC_COLUMN_WIDTH: f32 = 230.0;
/// The fixed card action row (`SchematicsDialog.java:150-205`).
const SCHEMATIC_ACTIONS: [(&str, &str, &str); 4] = [
    ("info", "@info.title", "info"),
    ("upload", "@editor.export", "export"),
    ("pencil", "@schematic.edit", "edit"),
    ("trash", "@save.delete", "delete"),
];

/// Controller state for the reused schematics dialog node.
#[derive(Default)]
struct SchematicState {
    field: Option<Gd<LineEdit>>,
    list: Option<Gd<MindTable>>,
    tags_row: Option<Gd<HBoxContainer>>,
    tags: Vec<String>,
    selected_tags: Vec<String>,
    /// First visible schematic's view index (`firstSchematic`; Enter uses it).
    first_index: i64,
    pending_delete: i64,
    pending_rename: i64,
    pending_icon: i64,
    /// True while the tag text prompt is open (`showAllTags` pencil half).
    pending_tag: bool,
}

impl SchematicState {
    fn new() -> Self {
        Self {
            first_index: -1,
            pending_delete: -1,
            pending_rename: -1,
            pending_icon: -1,
            ..Self::default()
        }
    }
}

/// The schematic library (`SchematicsDialog`).
pub struct SchematicsDialog {
    state: Rc<RefCell<SchematicState>>,
}

impl Default for SchematicsDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(SchematicState::new())),
        }
    }
}

impl DialogController for SchematicsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        schematics_ready(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        schematics_shown(dialog, &self.state);
    }
}

fn schematics_ready(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    // `cont.top()` (`SchematicsDialog.java:78`): the body sticks to the top.
    dialog.apply_cont_top(true);
    dialog.set_title_key("@schematics");
    dialog.should_pause = true;
    dialog.enable_full_dialog(true);
    // `makeButtonOverlay()` (`SchematicsDialog.java:64`).
    dialog.apply_button_overlay(true);
    schematics_build(dialog, state);
    dialog.add_close_button(210.0);
    // `buttons.button("@schematic.import", Icon.download, this::showImport)`:
    // the import button follows Back in the bottom row.
    let mut import = factory::button("");
    import.set_custom_minimum_size(Vector2::new(200.0, 54.0));
    dialog.apply_pane_button(&mut import, OUTLINE_GRAY, 4.0);
    let glyph: Gd<Control> = glyph_control("download", 20.0, Color::WHITE).upcast();
    let label = kit::t("@schematic.import").to_string();
    dialog.build_button_content(&mut import, Some(glyph), &label);
    let target = dialog.to_gd();
    let state_for_import = state.clone();
    let callable = Callable::from_fn("schematic_import", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        schematics_import(&mut bound, &state_for_import);
    });
    import.connect("pressed", &callable);
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&import);
    }
    if let Some(ui) = kit::ui() {
        let target = dialog.to_gd();
        let state_for_confirm = state.clone();
        let callable = Callable::from_fn("schematic_confirm", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_on_confirm(&mut bound, &state_for_confirm, confirmed);
        });
        connect_once(
            dialog,
            "schematic_confirm",
            &ui,
            "confirm_result",
            &callable,
        );
        let target = dialog.to_gd();
        let state_for_input = state.clone();
        let callable = Callable::from_fn("schematic_text_input", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_on_text_input(&mut bound, &state_for_input, &text);
        });
        connect_once(
            dialog,
            "schematic_text_input",
            &ui,
            "text_input_result",
            &callable,
        );
    }
}

fn schematics_build(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let mut root = dialog.content_table();
    // `cont.table(s -> { s.image(Icon.zoom); s.add(searchField).growX() })`.
    // code-instantiated: the search row is a fixed composite (zoom glyph +
    // field) that filters the runtime schematic rows.
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 4);
    let zoom = glyph_control("zoom", 22.0, Color::WHITE);
    search_row.add_child(&zoom);
    let mut field = factory::field(&kit::t("@schematic.search").to_string());
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        // `searchField.changed(() -> { search = searchField.getText(); rebuildPane.run(); })`.
        let target = dialog.to_gd();
        let state_for_filter = state.clone();
        let callable = Callable::from_fn("schematic_search", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_rebuild_list(&mut bound, &state_for_filter);
        });
        field.connect("text_changed", &callable);
        // `Binding.chat` with the field focused -> use/inspect the first match.
        let target = dialog.to_gd();
        let state_for_submit = state.clone();
        let callable = Callable::from_fn("schematic_search_submit", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_search_submit(&mut bound, &state_for_submit);
        });
        field.connect("text_submitted", &callable);
    }
    search_row.add_child(&field);
    root.bind_mut().add(&search_row).grow_x().pad(4.0);
    root.bind_mut().row();
    // `cont.table(in -> { in.left(); in.add("@schematic.tags"); pane(toggles);
    // button(Icon.pencilSmall, showAllTags) })`.
    let mut tags_row = HBoxContainer::new_alloc();
    tags_row.add_theme_constant_override("separation", 2);
    root.bind_mut().add(&tags_row).grow_x().pad(2.0);
    root.bind_mut().row();
    // `cont.pane(t -> ...)` with `cols = max(screenW / 230, 1)`.
    let mut list = MindTable::new_alloc();
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    root.bind_mut().add(&list).grow_x().grow_y();
    root.bind_mut().row();
    {
        let mut st = state.borrow_mut();
        st.field = Some(field);
        st.tags_row = Some(tags_row);
        st.list = Some(list);
    }
    schematics_rebuild_tags(dialog, state);
    schematics_rebuild_list(dialog, state);
}

fn schematics_shown(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    schematics_load_tags(dialog, state);
    // `shown(() -> { searchField.setText(search = ""); setup(); })`.
    if let Some(mut field) = state.borrow().field.clone() {
        field.set_text("");
    }
    schematics_rebuild_tags(dialog, state);
    schematics_rebuild_list(dialog, state);
    // `SchematicsDialog.show`: desktop focuses the search field.
    if let Some(mut field) = state.borrow().field.clone() {
        field.call_deferred("grab_focus", &[]);
    }
}

/// `SchematicsDialog` tags (`Core.settings` JSON `schematic-tags`).
fn schematics_load_tags(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let _ = dialog;
    let mut tags: Vec<String> = Vec::new();
    let text = setting_text("schematic-tags", "[]");
    for tag in parse_array(&text).iter_shared() {
        let value = text_of(&tag);
        if !value.is_empty() && !tags.contains(&value) {
            tags.push(value);
        }
    }
    state.borrow_mut().tags = tags;
    schematics_check_tags(dialog, state);
}

/// `checkTags()` (`SchematicsDialog.java:367-379`): every label of every
/// schematic joins the global tag list in first-seen order.
fn schematics_check_tags(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let schematics = dialog.campaign_section("schematics");
    let mut st = state.borrow_mut();
    for row_variant in schematics.iter_shared() {
        let Ok(row) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        for label in row_arr(&row, "labels").iter_shared() {
            let tag = text_of(&label);
            if !tag.is_empty() && !st.tags.contains(&tag) {
                st.tags.push(tag);
            }
        }
    }
}

fn schematics_save_tags(state: &Rc<RefCell<SchematicState>>) {
    let tags = {
        let st = state.borrow();
        st.tags.clone()
    };
    let mut array = VarArray::new();
    for tag in &tags {
        array.push(&gs(tag.as_str()).to_variant());
    }
    setting_put_text("schematic-tags", &json_stringify_array(&array));
}

/// `rebuildTags` (`SchematicsDialog.java:86-116`): `@schematic.tags` label, one
/// toggle per saved tag (checked while selected) and the edit pencil.
fn schematics_rebuild_tags(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let Some(mut row) = state.borrow().tags_row.clone() else {
        return;
    };
    for child in row.get_children().iter_shared() {
        let mut child = child;
        child.queue_free();
    }
    let (tags, selected) = {
        let st = state.borrow();
        (st.tags.clone(), st.selected_tags.clone())
    };
    // code-instantiated: the tag label/toggles follow the settings tag list.
    let mut tags_label = factory::label(&kit::t("@schematic.tags").to_string());
    tags_label.set_custom_minimum_size(Vector2::new(0.0, TAG_HEIGHT));
    rich(&tags_label).set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    row.add_child(&tags_label);
    // `in.add("@schematic.tags").padRight(4)`.
    row.add_child(&factory::space(2.0, 0.0));
    // `in.pane(Styles.noBarPane, t -> ...).fillX().height(tagh).scrollY(false)`:
    // the toggles scroll horizontally without a visible bar.
    // code-instantiated: rebuilt with the tag list (`rebuildTags`).
    let mut pane = MindScroll::new_alloc();
    pane.set_h_size_flags(SizeFlags::EXPAND_FILL);
    pane.set_custom_minimum_size(Vector2::new(0.0, TAG_HEIGHT + 4.0));
    pane.set_horizontal_scroll_mode(ScrollMode::SHOW_NEVER);
    pane.set_vertical_scroll_mode(ScrollMode::DISABLED);
    let mut toggles = HBoxContainer::new_alloc();
    toggles.add_theme_constant_override("separation", 4);
    pane.add_child(&toggles);
    row.add_child(&pane);
    for tag in &tags {
        let mut button = factory::button(&schematics_tag_plain(tag));
        // Tag labels carry Arc color markup (`[magenta]advanced-logic`): tint
        // the button text with that color.
        if let Some(tint) = schematics_tag_color(tag) {
            for key in [
                "font_color",
                "font_hover_color",
                "font_pressed_color",
                "font_focus_color",
            ] {
                button.add_theme_color_override(key, tint);
            }
        }
        button.set_theme_type_variation("togglet");
        button.set_toggle_mode(true);
        button.set_pressed(selected.contains(tag));
        button.set_custom_minimum_size(Vector2::new(0.0, TAG_HEIGHT));
        // `.with(c -> c.getLabel().setWrap(false))`.
        button.set_autowrap_mode(AutowrapMode::OFF);
        let target = dialog.to_gd();
        let state_for_tag = state.clone();
        let tag_name = tag.clone();
        let callable = Callable::from_fn("schematic_toggle_tag", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_toggle_tag(&mut bound, &state_for_tag, &tag_name);
        });
        button.connect("pressed", &callable);
        toggles.add_child(&button);
    }
    let mut pencil = factory::glyph_button(
        "pencil",
        &kit::t("@schematic.edittags").to_string(),
        "emptyi",
    );
    pencil.set_custom_minimum_size(Vector2::new(TAG_HEIGHT, TAG_HEIGHT));
    let target = dialog.to_gd();
    let state_for_edit_tags = state.clone();
    let callable = Callable::from_fn("schematic_edit_tags", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        schematics_edit_tags(&mut bound, &state_for_edit_tags);
    });
    pencil.connect("pressed", &callable);
    row.add_child(&pencil);
}

/// A tag with its Arc color markup (`[magenta]`, `[#ff00ff]`, `[]`) removed.
fn schematics_tag_plain(tag: &str) -> String {
    let characters: Vec<char> = tag.chars().collect();
    let mut out = String::with_capacity(tag.len());
    let mut index = 0usize;
    while index < characters.len() {
        if characters[index] == '['
            && let Some(close) = characters[index..].iter().position(|value| *value == ']')
        {
            index += close + 1;
            continue;
        }
        out.push(characters[index]);
        index += 1;
    }
    out
}

/// The first color the tag's markup selects (`None` for plain tags).
fn schematics_tag_color(tag: &str) -> Option<Color> {
    let rest = tag.strip_prefix('[')?;
    let close = rest.find(']')?;
    let inner = &rest[..close];
    if inner.is_empty() {
        return None;
    }
    let parsed = Color::from_string(&gs(inner))?;
    // `Color.from_string(s, Color(0, 0, 0, 0))` + sentinel compare.
    if parsed == Color::from_rgba(0.0, 0.0, 0.0, 0.0) {
        return None;
    }
    Some(parsed)
}

fn schematics_toggle_tag(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>, tag: &str) {
    {
        let mut st = state.borrow_mut();
        if let Some(position) = st.selected_tags.iter().position(|value| value == tag) {
            st.selected_tags.remove(position);
        } else {
            st.selected_tags.push(tag.to_string());
        }
    }
    schematics_rebuild_list(dialog, state);
}

/// `showAllTags` pencil: the port adds a tag through the shared text prompt.
fn schematics_edit_tags(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let _ = dialog;
    state.borrow_mut().pending_tag = true;
    let _ = ui_call(
        "show_text_input",
        &[
            kit::t("@schematic.edittags").to_variant(),
            kit::t("@schematic.tags").to_variant(),
            24i64.to_variant(),
            GString::new().to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `rebuildPane` (`SchematicsDialog.java:118-224`).
fn schematics_rebuild_list(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let Some(mut list) = state.borrow().list.clone() else {
        return;
    };
    list.bind_mut().clear();
    let search = state
        .borrow()
        .field
        .clone()
        .map(|field| field.get_text().to_string())
        .unwrap_or_default();
    let needle = schematics_strip_symbols(&search.to_lowercase());
    let schematics = dialog.campaign_section("schematics");
    // `cols = max((int)(screenW / 230), 1)`.
    let cols = ((dialog.base().get_viewport_rect().size.x / SCHEMATIC_COLUMN_WIDTH) as i64).max(1);
    let mut shown_count = 0i64;
    let mut first_index = -1i64;
    let selected_tags = {
        let st = state.borrow();
        st.selected_tags.clone()
    };
    for row_variant in schematics.iter_shared() {
        let Ok(schematic) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        // `selectedTags.any() && !s.labels.containsAll(selectedTags)`.
        let labels = row_arr(&schematic, "labels");
        let mut tagged = true;
        for tag in &selected_tags {
            if !labels.iter_shared().any(|value| text_of(&value) == *tag) {
                tagged = false;
                break;
            }
        }
        if !tagged {
            continue;
        }
        // `!search.isEmpty() && !ignoreSymbols(name).contains(searchString)`.
        let name = row_str(&schematic, "name");
        if !needle.is_empty() && !schematics_strip_symbols(&name.to_lowercase()).contains(&needle) {
            continue;
        }
        if first_index < 0 {
            first_index = row_i64_or(&schematic, "index", -1);
        }
        let card = schematics_build_card(dialog, state, &schematic);
        list.bind_mut().add(&card).pad(4.0);
        shown_count += 1;
        if shown_count % cols == 0 {
            list.bind_mut().row();
        }
    }
    state.borrow_mut().first_index = first_index;
    if shown_count == 0 {
        // `@none.found` when filtering, plain `@none` (light gray) otherwise.
        if !needle.is_empty() || !selected_tags.is_empty() {
            let label = factory::label(&kit::t("@none.found").to_string());
            list.bind_mut().add(&label);
        } else {
            let label = factory::label(&kit::t("@none").to_string());
            let mut canvas: Gd<CanvasItem> = label.clone().upcast();
            canvas.set_modulate(styles::ARC_LIGHT_GRAY);
            list.bind_mut().add(&label);
        }
        list.bind_mut().row();
    }
    list.bind_mut().sort_now();
}

/// One card (`SchematicsDialog.java:150-205`): a flat button with the `pane`
/// background holding the 50 px action row and the 200 px image stack.
fn schematics_build_card(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    schematic: &VarDictionary,
) -> Gd<Button> {
    let index = row_i64_or(schematic, "index", -1);
    // code-instantiated: one card per filtered schematic row.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("flati");
    button.set_custom_minimum_size(Vector2::new(200.0, 250.0));
    let pane = MindDialog::pane_box(Color::BLACK, OUTLINE_GRAY, 4.0, 2.0, -1.0);
    for style_name in ["normal", "hover", "pressed"] {
        button.add_theme_stylebox_override(style_name, &pane);
    }
    {
        let target = dialog.to_gd();
        let state_for_press = state.clone();
        let callable = Callable::from_fn("schematic_card_pressed", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_on_card_pressed(&mut bound, &state_for_press, index);
        });
        button.connect("pressed", &callable);
    }
    let mut column = VBoxContainer::new_alloc();
    column.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    column.add_theme_constant_override("separation", 0);
    column.set_mouse_filter(MouseFilter::IGNORE);
    // `b.table(buttons -> { buttons.left(); buttons.defaults().size(50f); ... })
    // .growX().height(50f)`.
    let mut actions = HBoxContainer::new_alloc();
    actions.add_theme_constant_override("separation", 0);
    actions.set_mouse_filter(MouseFilter::IGNORE);
    for (icon, tooltip, action) in SCHEMATIC_ACTIONS {
        // code-instantiated: fixed upstream action row (four card buttons).
        let mut action_button = factory::glyph_button(icon, &kit::t(tooltip).to_string(), "emptyi");
        action_button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
        let target = dialog.to_gd();
        let state_for_action = state.clone();
        let action_name = action.to_string();
        let callable = Callable::from_fn("schematic_card_action", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_on_card_action(&mut bound, &state_for_action, index, &action_name);
        });
        action_button.connect("pressed", &callable);
        actions.add_child(&action_button);
    }
    column.add_child(&actions);
    // `b.stack(new SchematicImage(s), new Table(n -> { n.top(); black3 badge }))
    // .size(200f)`.
    let mut stack = Control::new_alloc();
    stack.set_custom_minimum_size(Vector2::new(200.0, 200.0));
    stack.set_mouse_filter(MouseFilter::IGNORE);
    let mut image = MindSchematicImage::new_alloc();
    {
        let mut image_control: Gd<Control> = image.clone().upcast();
        image_control.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    }
    image.bind_mut().setup(schematic.clone());
    stack.add_child(&image);
    let mut badge = PanelContainer::new_alloc();
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(styles::BLACK_3);
    box_.set_content_margin(Side::LEFT, 4.0);
    box_.set_content_margin(Side::RIGHT, 4.0);
    badge.add_theme_stylebox_override("panel", &box_);
    badge.set_anchors_preset(LayoutPreset::TOP_WIDE);
    badge.set_mouse_filter(MouseFilter::IGNORE);
    // `label.setEllipsis(true)`, `maxWidth(200f - 8f)`, centred, top.
    let mut name_label = Label::new_alloc();
    name_label.set_text(&gs(row_str(schematic, "name")));
    name_label.set_custom_minimum_size(Vector2::new(192.0, 0.0));
    name_label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
    name_label.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
    name_label.set_clip_text(true);
    name_label.set_mouse_filter(MouseFilter::IGNORE);
    badge.add_child(&name_label);
    stack.add_child(&badge);
    column.add_child(&stack);
    button.add_child(&column);
    {
        let image_for_enter = image.clone();
        let callable = Callable::from_fn("schematic_card_enter", move |_args: &[&Variant]| {
            let mut image = image_for_enter.clone();
            image.bind_mut().set_hovered(true);
        });
        button.connect("mouse_entered", &callable);
        let image_for_exit = image.clone();
        let callable = Callable::from_fn("schematic_card_exit", move |_args: &[&Variant]| {
            let mut image = image_for_exit.clone();
            image.bind_mut().set_hovered(false);
        });
        button.connect("mouse_exited", &callable);
    }
    button
}

/// Card press (`SchematicsDialog.java:190-205`): menu -> info; in game ->
/// `control.input.useSchematic(s)` + hide (or `@schematic.disabled`).
fn schematics_on_card_pressed(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
) {
    let _ = state;
    if index < 0 {
        return;
    }
    if !in_game() {
        schematics_open_info(index);
        return;
    }
    if !use_schematic(index) {
        let text = kit::t("@schematic.disabled").to_string();
        dialog.show_toast(&text);
        return;
    }
    close_dialog("schematics");
}

/// Card action row press: never falls through as a card press (the buttons stop
/// the event; Java `sel[0].childrenPressed()`).
fn schematics_on_card_action(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
    action: &str,
) {
    match action {
        "info" => schematics_open_info(index),
        "export" => schematics_export(dialog, index),
        "edit" => schematics_edit(dialog, state, index),
        "delete" => schematics_delete(dialog, state, index),
        _ => {}
    }
    dialog.base_mut().emit_signal(
        "schematic_action",
        &[index.to_variant(), gs(action).to_variant()],
    );
}

/// Enter in the search field (`Binding.chat`).
fn schematics_search_submit(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let first_index = {
        let st = state.borrow();
        st.first_index
    };
    if first_index < 0 {
        return;
    }
    if !in_game() {
        schematics_open_info(first_index);
        return;
    }
    if !use_schematic(first_index) {
        let text = kit::t("@schematic.disabled").to_string();
        dialog.show_toast(&text);
        return;
    }
    close_dialog("schematics");
}

/// `showInfo(schematic)` -> the `schematic_info` dialog.
fn schematics_open_info(index: i64) {
    let mut context = VarDictionary::new();
    context.set("index", &index.to_variant());
    open_dialog("schematic_info", &context);
}

/// `SchematicsDialog.showExport` -> `@copy.clipboard` half.
fn schematics_export(dialog: &mut MindDialog, index: i64) {
    let result = dialog.campaign_call("export_schematic", &[index.to_variant()]);
    let text = result
        .filter(|value| value.get_type() != VariantType::NIL)
        .map(|value| text_of(&value))
        .unwrap_or_default();
    if text.is_empty() {
        let none = kit::t("@none").to_string();
        dialog.show_toast(&none);
        return;
    }
    DisplayServer::singleton().clipboard_set(&gs(text.as_str()));
    let copied = kit::t("@copied").to_string();
    dialog.show_toast(&copied);
}

/// `SchematicsDialog.showImport` clipboard half.
fn schematics_import(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let text = DisplayServer::singleton().clipboard_get();
    if text.is_empty() {
        let none = kit::t("@none").to_string();
        dialog.show_toast(&none);
        return;
    }
    let result = dialog.campaign_call("import_schematic", &[text.to_variant()]);
    let imported = result.map(|value| int_of(&value)).unwrap_or(-1);
    if imported < 0 {
        let none = kit::t("@none").to_string();
        dialog.show_toast(&none);
        return;
    }
    // The read models are live (`campaign_views_json`), so `refresh_campaign_views`
    // needs no cache update here.
    schematics_rebuild_list(dialog, state);
}

/// `@schematic.delete.confirm`; mod-provided schematics show the removal note
/// (`mod.item.remove`) instead of the confirm.
fn schematics_delete(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>, index: i64) {
    let schematics = dialog.campaign_section("schematics");
    let mut mod_name = String::new();
    for row_variant in schematics.iter_shared() {
        let Ok(row) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        if row_i64_or(&row, "index", -1) == index {
            mod_name = row_str(&row, "mod_name");
            break;
        }
    }
    if !mod_name.is_empty() {
        let text = kit::tm_format("@mod.item.remove", &[mod_name.as_str()]).to_string();
        dialog.show_toast(&text);
        return;
    }
    state.borrow_mut().pending_delete = index;
    let _ = ui_call(
        "show_confirm",
        &[kit::t("@schematic.delete.confirm").to_variant()],
    );
}

fn schematics_edit(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>, index: i64) {
    state.borrow_mut().pending_rename = index;
    let mut schematic_name = String::new();
    let schematics = dialog.campaign_section("schematics");
    for row_variant in schematics.iter_shared() {
        let Ok(row) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        if row_i64_or(&row, "index", -1) == index {
            schematic_name = row_str(&row, "name");
            break;
        }
    }
    let _ = ui_call(
        "show_text_input",
        &[
            kit::t("@schematic.edit").to_variant(),
            kit::t("@name").to_variant(),
            64i64.to_variant(),
            gs(schematic_name.as_str()).to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `SchematicsDialog` icon tag picker (`IconSelectDialog`).
#[allow(dead_code)]
fn schematics_open_icon_select(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
) {
    state.borrow_mut().pending_icon = index;
    if let Some(selector) = dialog_node("icon_select")
        && selector.has_signal("icon_selected")
    {
        let target = dialog.to_gd();
        let state_for_pick = state.clone();
        let callable = Callable::from_fn("schematic_icon_selected", move |args: &[&Variant]| {
            let region = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_on_icon_selected(&mut bound, &state_for_pick, &region);
        });
        connect_once(
            dialog,
            "schematic_icon_selected",
            &selector,
            "icon_selected",
            &callable,
        );
    }
    let mut context = VarDictionary::new();
    context.set("index", &index.to_variant());
    open_dialog("icon_select", &context);
}

fn schematics_on_confirm(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    confirmed: bool,
) {
    let pending = {
        let st = state.borrow();
        st.pending_delete
    };
    if !confirmed || pending < 0 {
        state.borrow_mut().pending_delete = -1;
        return;
    }
    state.borrow_mut().pending_delete = -1;
    let deleted = dialog
        .campaign_call("delete_schematic", &[pending.to_variant()])
        .map(|value| value.try_to::<bool>().unwrap_or(false))
        .unwrap_or(false);
    if deleted {
        schematics_rebuild_list(dialog, state);
    }
}

fn schematics_on_text_input(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    text: &str,
) {
    let tag_pending = {
        let st = state.borrow();
        st.pending_tag
    };
    if tag_pending {
        let added = {
            let mut st = state.borrow_mut();
            st.pending_tag = false;
            if !text.is_empty() && !st.tags.iter().any(|tag| tag == text) {
                st.tags.push(text.to_string());
                true
            } else {
                false
            }
        };
        if added {
            schematics_save_tags(state);
            schematics_rebuild_tags(dialog, state);
            schematics_rebuild_list(dialog, state);
        }
        return;
    }
    let pending = {
        let st = state.borrow();
        st.pending_rename
    };
    if pending < 0 {
        return;
    }
    if text.is_empty() {
        state.borrow_mut().pending_rename = -1;
        return;
    }
    state.borrow_mut().pending_rename = -1;
    let renamed = dialog
        .campaign_call(
            "rename_schematic",
            &[pending.to_variant(), gs(text).to_variant()],
        )
        .map(|value| value.try_to::<bool>().unwrap_or(false))
        .unwrap_or(false);
    if renamed {
        schematics_rebuild_list(dialog, state);
    }
}

fn schematics_on_icon_selected(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    region_name: &str,
) {
    let pending = {
        let st = state.borrow();
        st.pending_icon
    };
    if pending < 0 {
        return;
    }
    state.borrow_mut().pending_icon = -1;
    close_dialog("icon_select");
    let applied = dialog
        .campaign_call(
            "set_schematic_icon",
            &[pending.to_variant(), gs(region_name).to_variant()],
        )
        .map(|value| value.try_to::<bool>().unwrap_or(false))
        .unwrap_or(false);
    if applied {
        schematics_rebuild_list(dialog, state);
    }
}

/// `SchematicsDialog.ignoreSymbols`: strip the symbol class from a name.
fn schematics_strip_symbols(value: &str) -> String {
    value
        .chars()
        .filter(|character| !IGNORE_SYMBOLS.contains(*character))
        .collect()
}

// ---------------------------------------------------------------------------
// LoadoutDialog
// ---------------------------------------------------------------------------

/// Controller state for the core loadout capacity editor.
#[derive(Default)]
struct LoadoutState {
    summary: Option<Gd<MindTable>>,
    capacity: i64,
    index: i64,
    amounts: VarDictionary,
    seed_amounts: VarDictionary,
    pending_edit: String,
}

impl LoadoutState {
    fn new() -> Self {
        Self {
            index: -1,
            ..Self::default()
        }
    }
}

/// Core loadout capacity editor (`LoadoutDialog`).
pub struct LoadoutDialog {
    state: Rc<RefCell<LoadoutState>>,
}

impl Default for LoadoutDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(LoadoutState::new())),
        }
    }
}

impl DialogController for LoadoutDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@configure");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        let table = dialog.content_table();
        self.state.borrow_mut().summary = Some(table);
        if let Some(ui) = kit::ui() {
            let target = dialog.to_gd();
            let state_for_input = self.state.clone();
            let callable = Callable::from_fn("loadout_text_input", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| (*value).to::<GString>().to_string())
                    .unwrap_or_default();
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                loadout_on_text_input(&mut bound, &state_for_input, &text);
            });
            connect_once(
                dialog,
                "loadout_text_input",
                &ui,
                "text_input_result",
                &callable,
            );
        }
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let planet = dialog.context_string("planet").to_string();
        let views = dialog.campaign_json("campaign_views_json", &[gs(planet).to_variant()]);
        let capacity = match dialog.context_value("capacity") {
            Some(value) => int_of(&value),
            None => row_i64_or(&views, "launch_capacity", 0),
        };
        let index = dialog
            .context_value("index")
            .map(|value| int_of(&value))
            .unwrap_or(-1);
        {
            let mut st = self.state.borrow_mut();
            st.capacity = capacity;
            st.index = index;
        }
        loadout_seed(dialog, &self.state);
        loadout_rebuild(dialog, &self.state);
    }
}

/// Seeds the editor from the live launch resources, unioned with every loadout
/// requirement item so all items can be stepped at least once.
fn loadout_seed(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>) {
    let mut seed_amounts = VarDictionary::new();
    // `Universe.getLaunchResources()`: the name-keyed stacks endpoint is the
    // live facade reader (`get_launch_resources` returns the dense sequence).
    let mut live = dialog.campaign_json("get_launch_resources", &[]);
    if live.is_empty() {
        live = dialog.campaign_json("get_launch_resource_stacks", &[]);
    }
    let index = {
        let st = state.borrow();
        st.index
    };
    if !live.is_empty() {
        for item in live.keys_array().iter_shared() {
            let name = text_of(&item);
            let amount = live
                .get(&gs(name.as_str()))
                .map(|value| int_of(&value))
                .unwrap_or(0);
            seed_amounts.set(&gs(name.as_str()), &amount.to_variant());
        }
    } else {
        for loadout in dialog.campaign_section("loadouts").iter_shared() {
            let Ok(loadout) = loadout.try_to::<VarDictionary>() else {
                continue;
            };
            if index >= 0 && row_i64_or(&loadout, "index", -1) != index {
                continue;
            }
            for requirement in row_arr(&loadout, "requirements").iter_shared() {
                let Ok(requirement) = requirement.try_to::<VarArray>() else {
                    continue;
                };
                let mut values = requirement.iter_shared();
                let item = values
                    .next()
                    .map(|value| text_of(&value))
                    .unwrap_or_default();
                let amount = values.next().map(|value| int_of(&value)).unwrap_or(0);
                seed_amounts.set(&gs(item), &amount.to_variant());
            }
        }
    }
    for loadout in dialog.campaign_section("loadouts").iter_shared() {
        let Ok(loadout) = loadout.try_to::<VarDictionary>() else {
            continue;
        };
        for requirement in row_arr(&loadout, "requirements").iter_shared() {
            let Ok(requirement) = requirement.try_to::<VarArray>() else {
                continue;
            };
            let Some(item) = requirement
                .iter_shared()
                .next()
                .map(|value| text_of(&value))
            else {
                continue;
            };
            if !seed_amounts.contains_key(&gs(item.as_str())) {
                seed_amounts.set(&gs(item), &0i64.to_variant());
            }
        }
    }
    let mut st = state.borrow_mut();
    st.amounts = seed_amounts.duplicate_shallow();
    st.seed_amounts = seed_amounts;
}

fn loadout_rebuild(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>) {
    let Some(mut summary) = state.borrow().summary.clone() else {
        return;
    };
    summary.bind_mut().clear();
    let (capacity, amounts) = {
        let st = state.borrow();
        (st.capacity, st.amounts.clone())
    };
    if capacity > 0 {
        let text = kit::tm_format("@launch.capacity", &[capacity.to_string().as_str()]).to_string();
        let label = factory::label(&text);
        summary.bind_mut().add(&label).grow_x().pad(4.0);
        summary.bind_mut().row();
    }
    let mut keys: Vec<String> = amounts
        .keys_array()
        .iter_shared()
        .map(|value| text_of(&value))
        .collect();
    keys.sort();
    let mut pending: Option<Gd<HBoxContainer>> = None;
    for (position, item) in keys.iter().enumerate() {
        if position % 2 == 0 {
            // code-instantiated: item rows are the live launch-resource ItemSeq.
            let mut row = HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 6);
            summary.bind_mut().add(&row).grow_x().pad(2.0);
            summary.bind_mut().row();
            pending = Some(row);
        }
        let Some(mut row) = pending.clone() else {
            continue;
        };
        let item_row = loadout_item_row(dialog, state, item);
        row.add_child(&item_row);
    }
    dialog.clear_buttons();
    let target = dialog.to_gd();
    let state_for_max = state.clone();
    let callable = Callable::from_fn("loadout_max", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        loadout_max_items(&mut bound, &state_for_max);
    });
    let max_text = kit::t("@max").to_string();
    dialog.add_button(&max_text, Some(callable), "add", 200.0);
    let target = dialog.to_gd();
    let state_for_reset = state.clone();
    let callable = Callable::from_fn("loadout_reset", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        loadout_reset(&mut bound, &state_for_reset);
    });
    let reset_text = kit::t("@settings.reset").to_string();
    dialog.add_button(&reset_text, Some(callable), "refresh", 200.0);
    dialog.add_close_button(210.0);
}

fn loadout_item_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LoadoutState>>,
    item: &str,
) -> Gd<Control> {
    // code-instantiated: item rows are the live launch-resource ItemSeq.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 2);
    let mut minus = factory::button("-");
    minus.set_custom_minimum_size(Vector2::new(36.0, 36.0));
    let target = dialog.to_gd();
    let state_for_minus = state.clone();
    let item_for_minus = item.to_string();
    let callable = Callable::from_fn("loadout_minus", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        loadout_step(&mut bound, &state_for_minus, &item_for_minus, -1);
    });
    minus.connect("pressed", &callable);
    row.add_child(&minus);
    let mut plus = factory::button("+");
    plus.set_custom_minimum_size(Vector2::new(36.0, 36.0));
    let target = dialog.to_gd();
    let state_for_plus = state.clone();
    let item_for_plus = item.to_string();
    let callable = Callable::from_fn("loadout_plus", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        loadout_step(&mut bound, &state_for_plus, &item_for_plus, 1);
    });
    plus.connect("pressed", &callable);
    row.add_child(&plus);
    let mut pencil = factory::image_button("pencil", "flati");
    pencil.set_custom_minimum_size(Vector2::new(36.0, 36.0));
    let target = dialog.to_gd();
    let state_for_edit = state.clone();
    let item_for_edit = item.to_string();
    let callable = Callable::from_fn("loadout_edit", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        loadout_edit(&mut bound, &state_for_edit, &item_for_edit);
    });
    pencil.connect("pressed", &callable);
    row.add_child(&pencil);
    // `res.image(item.uiIcon)`: the packed item region (the GDScript passed the
    // bare content name, which has no atlas region).
    let icon = match content_texture(item) {
        Some(texture) => icon_rect(&texture, 28.0),
        None => factory::image(&format!("item-{item}-ui")),
    };
    let mut icon_control: Gd<Control> = icon.upcast();
    icon_control.set_custom_minimum_size(Vector2::new(28.0, 28.0));
    icon_control.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&icon_control);
    let amount = {
        let st = state.borrow();
        st.amounts
            .get(&gs(item))
            .map(|value| int_of(&value))
            .unwrap_or(0)
    };
    let label = factory::label(&format!("{}  {}", loadout_localized_item(item), amount));
    row.add_child(&label);
    let control: Gd<Control> = row.upcast();
    control
}

/// Bundle label for an item (`item.<name>.name`), falling back to the id.
fn loadout_localized_item(item: &str) -> String {
    let key = format!("item.{item}.name");
    bundle_or(&key, item)
}

/// `LoadoutDialog.step`: 100/200/500/1000 depending on the current amount.
fn loadout_step_size(amount: i64) -> i64 {
    if amount < 1000 {
        100
    } else if amount < 2000 {
        200
    } else if amount < 5000 {
        500
    } else {
        1000
    }
}

fn loadout_step(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LoadoutState>>,
    item: &str,
    direction: i64,
) {
    let capacity = {
        let st = state.borrow();
        st.capacity
    };
    {
        let mut st = state.borrow_mut();
        let current = st
            .amounts
            .get(&gs(item))
            .map(|value| int_of(&value))
            .unwrap_or(0);
        let mut amount = (current + direction * loadout_step_size(current)).max(0);
        if capacity > 0 {
            amount = amount.min(capacity);
        }
        st.amounts.set(&gs(item), &amount.to_variant());
    }
    loadout_apply(dialog, state);
    loadout_rebuild(dialog, state);
}

/// `LoadoutDialog.maxItems`: every stack to the launch capacity.
fn loadout_max_items(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>) {
    let capacity = {
        let st = state.borrow();
        st.capacity
    };
    if capacity <= 0 {
        return;
    }
    {
        let mut st = state.borrow_mut();
        let keys: Vec<String> = st
            .amounts
            .keys_array()
            .iter_shared()
            .map(|value| text_of(&value))
            .collect();
        for item in keys {
            st.amounts.set(&gs(item.as_str()), &capacity.to_variant());
        }
    }
    loadout_apply(dialog, state);
    loadout_rebuild(dialog, state);
}

/// `LoadoutDialog.@settings.reset`: re-seed from the live state.
fn loadout_reset(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>) {
    loadout_seed(dialog, state);
    loadout_apply(dialog, state);
    loadout_rebuild(dialog, state);
}

/// Inline amount entry through the shared `showTextInput` prompt pipeline.
fn loadout_edit(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>, item: &str) {
    state.borrow_mut().pending_edit = item.to_string();
    let capacity = {
        let st = state.borrow();
        st.capacity
    };
    let current = {
        let st = state.borrow();
        st.amounts
            .get(&gs(item))
            .map(|value| int_of(&value))
            .unwrap_or(0)
    };
    let limit = if capacity > 0 { capacity } else { 9999 };
    let message = kit::tm_format("@configure.invalid", &[limit.to_string().as_str()]).to_string();
    let _ = dialog;
    let _ = ui_call(
        "show_text_input",
        &[
            kit::t("@configure").to_variant(),
            gs(message.as_str()).to_variant(),
            10i64.to_variant(),
            gs(current.to_string().as_str()).to_variant(),
            true.to_variant(),
            true.to_variant(),
        ],
    );
}

fn loadout_on_text_input(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>, text: &str) {
    let pending = {
        let st = state.borrow();
        st.pending_edit.clone()
    };
    if pending.is_empty() || !is_valid_int(text) {
        state.borrow_mut().pending_edit = String::new();
        return;
    }
    let capacity = {
        let st = state.borrow();
        st.capacity
    };
    let mut amount = text.trim().parse::<i64>().unwrap_or(0).max(0);
    if capacity > 0 {
        amount = amount.min(capacity);
    }
    {
        let mut st = state.borrow_mut();
        st.amounts.set(&gs(pending.as_str()), &amount.to_variant());
        st.pending_edit = String::new();
    }
    loadout_apply(dialog, state);
    loadout_rebuild(dialog, state);
}

/// GDScript `String.is_valid_int()`: an optional sign followed by digits.
fn is_valid_int(text: &str) -> bool {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix(['-', '+']).unwrap_or(trimmed);
    !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit())
}

/// Persists the edited amounts through the campaign facade and emits the change.
fn loadout_apply(dialog: &mut MindDialog, state: &Rc<RefCell<LoadoutState>>) {
    let (amounts, index) = {
        let st = state.borrow();
        (st.amounts.clone(), st.index)
    };
    if let Some(mut campaign) = dialog.campaign_node()
        && campaign.has_method("set_launch_resources")
    {
        let _ = campaign.call("set_launch_resources", &[amounts.to_variant()]);
    }
    if index >= 0 {
        dialog
            .base_mut()
            .emit_signal("loadout_changed", &[index.to_variant()]);
    }
}

// ---------------------------------------------------------------------------
// LaunchLoadoutDialog
// ---------------------------------------------------------------------------

/// Controller state for the launch-loadout picker.
#[derive(Default)]
struct LaunchLoadoutState {
    list: Option<Gd<MindTable>>,
    selected: i64,
    plan: VarDictionary,
}

impl LaunchLoadoutState {
    fn new() -> Self {
        Self {
            selected: -1,
            ..Self::default()
        }
    }
}

/// Launch-loadout picker (`LaunchLoadoutDialog`).
pub struct LaunchLoadoutDialog {
    state: Rc<RefCell<LaunchLoadoutState>>,
}

impl Default for LaunchLoadoutDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(LaunchLoadoutState::new())),
        }
    }
}

impl DialogController for LaunchLoadoutDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@configure");
        dialog.should_pause = false;
        let list = dialog.content_table();
        self.state.borrow_mut().list = Some(list);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        launch_rebuild(dialog, &self.state);
    }
}

/// The `launch_dialog_json` plan for the current context/selection.
fn launch_plan(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) -> VarDictionary {
    let planet = dialog.context_string("planet").to_string();
    let launcher = dialog
        .context_value("launcher")
        .map(|value| int_of(&value))
        .unwrap_or(-1);
    let sector = dialog
        .context_value("sector")
        .map(|value| int_of(&value))
        .unwrap_or(-1);
    let selected = {
        let st = state.borrow();
        st.selected
    };
    dialog.campaign_json(
        "launch_dialog_json",
        &[
            gs(planet).to_variant(),
            launcher.to_variant(),
            sector.to_variant(),
            selected.to_variant(),
        ],
    )
}

/// `schematics.getLoadouts()` filtered by the launcher core.
fn launch_rows(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LaunchLoadoutState>>,
) -> Vec<VarDictionary> {
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    let rows = row_rows(&plan, "loadouts");
    if !rows.is_empty() {
        return rows;
    }
    dialog
        .campaign_section("loadouts")
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// The default core of the selected planet, read from the live loadout rows.
fn launch_selected_core(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LaunchLoadoutState>>,
) -> String {
    for loadout in launch_rows(dialog, state) {
        let core = row_str(&loadout, "core");
        if !core.is_empty() {
            return core;
        }
    }
    String::new()
}

/// The `SchematicView` row for a schematic index.
fn launch_schematic_row(dialog: &mut MindDialog, index: i64) -> VarDictionary {
    for row_variant in dialog.campaign_section("schematics").iter_shared() {
        let Ok(row) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        if row_i64_or(&row, "index", -1) == index {
            return row;
        }
    }
    VarDictionary::new()
}

/// The launcher's localized name (`Sector.name()` + the `sector.<preset>.name`
/// rule).
fn launch_localized_from(plan: &VarDictionary) -> String {
    let from_name = row_str(plan, "from");
    let preset = row_str(plan, "from_preset");
    if !preset.is_empty() {
        let key = format!("sector.{preset}.name");
        let localized = kit::t(&format!("@{key}")).to_string();
        if !localized.is_empty() && localized != key {
            return localized;
        }
    }
    from_name
}

fn launch_rebuild(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let Some(mut list) = state.borrow().list.clone() else {
        return;
    };
    list.bind_mut().clear();
    let plan = launch_plan(dialog, state);
    state.borrow_mut().plan = plan;
    // Select a row the launcher's plan offers (default row preferred); the plan
    // carries the requirements/capacity for the *selected* schematic, so a
    // selection change rebuilds it.
    let offered: Vec<i64> = launch_rows(dialog, state)
        .iter()
        .map(|loadout| row_i64_or(loadout, "index", -1))
        .collect();
    let mut selected = {
        let st = state.borrow();
        st.selected
    };
    if !offered.contains(&selected) {
        let plan = {
            let st = state.borrow();
            st.plan.clone()
        };
        selected = row_i64_or(&plan, "selected", -1);
        if !offered.contains(&selected) && !offered.is_empty() {
            selected = offered[0];
        }
        state.borrow_mut().selected = selected;
        let plan = launch_plan(dialog, state);
        state.borrow_mut().plan = plan;
    }
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    let loadouts = launch_rows(dialog, state);
    // `launch.from`: the launcher sector's name (`LaunchLoadoutDialog.java:165`).
    if !plan.is_empty() {
        let text = kit::tm_format("@launch.from", &[launch_localized_from(&plan).as_str()]);
        let label = factory::label(&text.to_string());
        list.bind_mut().add(&label).grow_x().pad(4.0);
        list.bind_mut().row();
    }
    if loadouts.is_empty() {
        let label = factory::label(&kit::t("@none").to_string());
        list.bind_mut().add(&label).pad(8.0);
        list.bind_mut().row();
    } else {
        // `LaunchLoadoutDialog.java:160-190`: `cols = max(screenW / 230, 1)`
        // image toggles (`Styles.togglet` + `SchematicImage`, 200 px, pad 4).
        let cols =
            ((dialog.base().get_viewport_rect().size.x / SCHEMATIC_COLUMN_WIDTH) as i64).max(1);
        for (position, loadout) in loadouts.iter().enumerate() {
            let schematic_index = row_i64_or(loadout, "index", -1);
            // code-instantiated: one SchematicImage toggle per allowed loadout.
            let mut button = Button::new_alloc();
            button.set_theme_type_variation("togglet");
            button.set_toggle_mode(true);
            button.set_pressed(schematic_index == selected);
            button.set_custom_minimum_size(Vector2::new(200.0, 200.0));
            let target = dialog.to_gd();
            let state_for_select = state.clone();
            let callable = Callable::from_fn("launch_select", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                state_for_select.borrow_mut().selected = schematic_index;
                launch_rebuild(&mut bound, &state_for_select);
            });
            button.connect("pressed", &callable);
            let mut image = MindSchematicImage::new_alloc();
            {
                let mut image_control: Gd<Control> = image.clone().upcast();
                image_control.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            }
            let schematic = launch_schematic_row(dialog, schematic_index);
            image.bind_mut().setup(schematic);
            button.add_child(&image);
            {
                let image_for_enter = image.clone();
                let callable = Callable::from_fn("launch_enter", move |_args: &[&Variant]| {
                    let mut image = image_for_enter.clone();
                    image.bind_mut().set_hovered(true);
                });
                button.connect("mouse_entered", &callable);
                let image_for_exit = image.clone();
                let callable = Callable::from_fn("launch_exit", move |_args: &[&Variant]| {
                    let mut image = image_for_exit.clone();
                    image.bind_mut().set_hovered(false);
                });
                button.connect("mouse_exited", &callable);
            }
            list.bind_mut().add(&button).pad(4.0);
            if (position as i64 + 1) % cols == 0 {
                list.bind_mut().row();
            }
        }
    }
    launch_rebuild_items(dialog, state);
    launch_rebuild_buttons(dialog, state);
}

/// Capacity label + item rows (`LaunchLoadoutDialog.rebuild`).
fn launch_rebuild_items(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let Some(mut list) = state.borrow().list.clone() else {
        return;
    };
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    if plan.is_empty() {
        let resource_totals = launch_resource_stacks(dialog);
        if !resource_totals.is_empty() {
            let mut parts: Vec<String> = Vec::new();
            for item in resource_totals.keys_array().iter_shared() {
                let name = text_of(&item);
                let amount = resource_totals
                    .get(&gs(name.as_str()))
                    .map(|value| int_of(&value))
                    .unwrap_or(0);
                parts.push(format!("{name} x{amount}"));
            }
            let label = factory::label(&format!("{} {}", kit::t("@resources"), parts.join(", ")));
            list.bind_mut().add(&label).grow_x().pad(2.0);
            list.bind_mut().row();
        }
        return;
    }
    let capacity = row_i64_or(&plan, "capacity", 0);
    if capacity > 0 {
        let text = kit::tm_format("@launch.capacity", &[capacity.to_string().as_str()]).to_string();
        let label = factory::label(&text);
        list.bind_mut().add(&label).grow_x().pad(4.0);
        list.bind_mut().row();
    }
    let allow = row_bool(&plan, "allow_launch_loadout");
    let total = row_dict(&plan, "total");
    let requirements = row_dict(&plan, "requirements");
    let resources = row_dict(&plan, "resources");
    let stored = row_dict(&plan, "stored");
    let mut count = 0i64;
    // code-instantiated: item rows are the item-id-ordered `total` map.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 4);
    for item in total.keys_array().iter_shared() {
        let name = text_of(&item);
        let amount = total
            .get(&gs(name.as_str()))
            .map(|value| int_of(&value))
            .unwrap_or(0);
        if amount == 0 {
            continue;
        }
        let icon = factory::image(&format!("item-{name}-ui"));
        row.add_child(&icon);
        let as_amount = requirements
            .get(&gs(name.as_str()))
            .map(|value| int_of(&value))
            .unwrap_or(0);
        let al_amount = resources
            .get(&gs(name.as_str()))
            .map(|value| int_of(&value))
            .unwrap_or(0);
        let mut amount_text = amount.to_string();
        if allow {
            amount_text = format!("{amount_text}[gray] ({al_amount} + {as_amount})");
        }
        let stored_amount = stored
            .get(&gs(name.as_str()))
            .map(|value| int_of(&value))
            .unwrap_or(0);
        if stored_amount < amount {
            amount_text = format!(
                "[scarlet]{}[lightgray]/{}",
                stored_amount.min(amount),
                amount_text
            );
        }
        let label = factory::label(&kit::markup_text(&amount_text).to_string());
        row.add_child(&label);
        count += 1;
        if count % 4 == 0 {
            list.bind_mut().add(&row).grow_x().pad(2.0);
            list.bind_mut().row();
            // code-instantiated: grid row continuation for the item list.
            row = HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 4);
        }
    }
    if count % 4 != 0 {
        list.bind_mut().add(&row).grow_x().pad(2.0);
        list.bind_mut().row();
    }
    if !row_bool(&plan, "valid") {
        let label = factory::label(&kit::tm("@sector.missingresources").to_string());
        list.bind_mut().add(&label).grow_x().pad(4.0);
        list.bind_mut().row();
    }
    let _ = state;
}

/// `buttons.defaults().size(160f, 64f)`; Java order: `@back`,
/// (`@resources.max` toggle), `@resources`, `@launch.text`.
fn launch_rebuild_buttons(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let plan = {
        let st = state.borrow();
        st.plan.clone()
    };
    dialog.clear_buttons();
    let back = dialog.base().callable("hide_dialog");
    let back_text = kit::t("@back").to_string();
    dialog.add_button(&back_text, Some(back), "left", 160.0);
    let allow = row_bool_or(&plan, "allow_launch_loadout", true);
    let max_on = row_bool_or(&plan, "max", true);
    if allow {
        let target = dialog.to_gd();
        let state_for_max = state.clone();
        let callable = Callable::from_fn("launch_toggle_max", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            launch_toggle_max(&mut bound, &state_for_max);
        });
        let max_text = kit::t("@resources.max").to_string();
        let mut max_button = dialog.add_button(&max_text, Some(callable), "add", 160.0);
        max_button.set_toggle_mode(true);
        max_button.set_pressed(max_on);
        let target = dialog.to_gd();
        let state_for_open = state.clone();
        let callable = Callable::from_fn("launch_open_loadout", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            launch_open_loadout(&mut bound, &state_for_open);
        });
        let resources_text = kit::t("@resources").to_string();
        let mut resources = dialog.add_button(&resources_text, Some(callable), "edit", 160.0);
        resources.set_disabled(max_on);
    }
    let target = dialog.to_gd();
    let state_for_confirm = state.clone();
    let callable = Callable::from_fn("launch_confirm", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        launch_confirm(&mut bound, &state_for_confirm);
    });
    let launch_text = kit::t("@launch.text").to_string();
    let mut launch = dialog.add_button(&launch_text, Some(callable), "ok", 160.0);
    launch.set_disabled(!plan.is_empty() && !row_bool(&plan, "valid"));
}

/// `setMax`: writes the `maxresources` flag through the live settings store.
fn launch_toggle_max(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let max_on = {
        let st = state.borrow();
        row_bool_or(&st.plan, "max", true)
    };
    setting_put_flag("maxresources", !max_on);
    state.borrow_mut().plan = VarDictionary::new();
    launch_rebuild(dialog, state);
}

/// Live `universe.getLaunchResources` as `{item: amount}`.
fn launch_resource_stacks(dialog: &mut MindDialog) -> VarDictionary {
    let Some(mut campaign) = dialog.campaign_node() else {
        return VarDictionary::new();
    };
    if !campaign.has_method("get_launch_resource_stacks") {
        return VarDictionary::new();
    }
    let text = campaign
        .call("get_launch_resource_stacks", &[])
        .to::<GString>();
    parse_object(&text.to_string())
}

/// The selected loadout row, or `{}`.
fn launch_find_selected_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LaunchLoadoutState>>,
) -> VarDictionary {
    let selected = {
        let st = state.borrow();
        st.selected
    };
    for loadout in launch_rows(dialog, state) {
        if row_i64_or(&loadout, "index", -1) == selected {
            return loadout;
        }
    }
    VarDictionary::new()
}

/// `LaunchLoadoutDialog.@launch.text`: `universe.updateLoadout(core, selected);
/// confirm.run(); hide()`.
fn launch_confirm(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let selected = {
        let st = state.borrow();
        st.selected
    };
    if selected < 0 {
        return;
    }
    // `Universe.updateLoadout(core, selected)` (`LaunchLoadoutDialog.java:150-152`):
    // record the chosen schematic's file for the core before the planet dialog
    // reads it back for the launch. A file-less schematic records the empty
    // string (Java `schem.file == null ? ""`).
    let row = launch_find_selected_row(dialog, state);
    if let Some(mut campaign) = dialog.campaign_node()
        && campaign.has_method("update_loadout")
    {
        let file_name = row_str(&row, "file");
        let _ = campaign.call(
            "update_loadout",
            &[
                gs(row_str(&row, "core")).to_variant(),
                gs(file_name).to_variant(),
            ],
        );
    }
    dialog
        .base_mut()
        .emit_signal("loadout_chosen", &[selected.to_variant()]);
}

/// `LaunchLoadoutDialog.@resources`: opens the capacity editor seeded with the
/// live launch resources.
fn launch_open_loadout(dialog: &mut MindDialog, state: &Rc<RefCell<LaunchLoadoutState>>) {
    let planet = dialog.context_string("planet").to_string();
    let sector = dialog
        .context_value("sector")
        .map(|value| int_of(&value))
        .unwrap_or(-1);
    let capacity = {
        let st = state.borrow();
        row_i64_or(&st.plan, "capacity", 0)
    };
    let selected = {
        let st = state.borrow();
        st.selected
    };
    let core = launch_selected_core(dialog, state);
    let mut context = VarDictionary::new();
    context.set("planet", &gs(planet));
    context.set("sector", &sector.to_variant());
    context.set("capacity", &capacity.to_variant());
    context.set("index", &selected.to_variant());
    context.set("core", &gs(core));
    open_dialog("loadout", &context);
}

// ---------------------------------------------------------------------------
// SectorStatsDialog
// ---------------------------------------------------------------------------

/// `Pal.accent` hex used in the `name [accent]value` stat lines.
const STATS_ACCENT: &str = "#ffd37f";

/// Controller state for the reused sector stats dialog node.
#[derive(Default)]
struct SectorStatsState {
    planet: String,
    sector_id: i64,
    row: VarDictionary,
    root: Option<Gd<MindTable>>,
    pending_abandon: bool,
}

impl SectorStatsState {
    fn new() -> Self {
        Self {
            sector_id: -1,
            ..Self::default()
        }
    }
}

/// The planet page's Stats window (`PlanetDialog.java:1026-1171`).
pub struct SectorStatsDialog {
    state: Rc<RefCell<SectorStatsState>>,
}

impl Default for SectorStatsDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(SectorStatsState::new())),
        }
    }
}

impl DialogController for SectorStatsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        // `new BaseDialog(sector.name())` fills the viewport.
        dialog.fill_panel();
        if let Some(ui) = kit::ui() {
            let target = dialog.to_gd();
            let state_for_confirm = self.state.clone();
            let callable = Callable::from_fn("sector_stats_confirm", move |args: &[&Variant]| {
                let confirmed = args
                    .first()
                    .map(|value| (*value).to::<bool>())
                    .unwrap_or(false);
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                sector_stats_on_confirm(&mut bound, &state_for_confirm, confirmed);
            });
            connect_once(
                dialog,
                "sector_stats_confirm",
                &ui,
                "confirm_result",
                &callable,
            );
        }
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let planet = dialog.context_string("planet").to_string();
        let sector_id = dialog
            .context_value("sector")
            .map(|value| int_of(&value))
            .unwrap_or(-1);
        let row = sector_stats_find_row(dialog, &planet, sector_id);
        {
            let mut st = self.state.borrow_mut();
            st.planet = planet;
            st.sector_id = sector_id;
            st.row = row;
            st.pending_abandon = false;
        }
        sector_stats_build(dialog, &self.state);
    }
}

/// `PlanetDialog.showStats`: description, time/attempts/wave/threat, resources,
/// lockdown, rate sections and the stored pool, in this order.
fn sector_stats_build(dialog: &mut MindDialog, state: &Rc<RefCell<SectorStatsState>>) {
    let mut root = {
        let st = state.borrow();
        st.root.clone()
    };
    let mut root = match root.take() {
        Some(table) => table,
        None => {
            // `dialog.cont.pane(c -> ...)`: the dialog body already scrolls, so
            // the stats table sits directly in it, centred at its natural width.
            let table = dialog.content_table();
            {
                let mut control: Gd<Control> = table.clone().upcast();
                control.set_h_size_flags(SizeFlags::SHRINK_CENTER);
            }
            state.borrow_mut().root = Some(table.clone());
            table
        }
    };
    root.bind_mut().clear();
    dialog.clear_buttons();
    let title = sector_stats_localized_name(state);
    dialog.set_title_literal(&title);
    let (data, planet, sector_id) = {
        let st = state.borrow();
        (st.row.clone(), st.planet.clone(), st.sector_id)
    };
    let _ = (planet, sector_id);
    let stats = row_dict(&data, "stats");
    let description_key = row_opt_str(&stats, "preset_description_key").unwrap_or_default();
    if !description_key.is_empty() {
        let description = factory::label(&kit::tm(&format!("@{description_key}")).to_string());
        let mut description_rich = rich(&description);
        description_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
        let mut minimum = description_rich.get_custom_minimum_size();
        minimum.x = 420.0;
        description_rich.set_custom_minimum_size(minimum);
        root.bind_mut().add(&description).grow_x().pad_bottom(5.0);
        root.bind_mut().row();
    }
    let playtime = row_i64_or(&stats, "playtime_ms", 0);
    sector_stats_value_line(&mut root, "@sectors.time", &format_millis(playtime));
    let attempts = row_i64_or(&stats, "attempts", 0);
    if attempts > 0 {
        sector_stats_value_line(&mut root, "@sectors.attempts", &attempts.to_string());
    }
    let wave = row_i64_or(&stats, "wave", 0);
    if wave > 0 {
        sector_stats_value_line(&mut root, "@sectors.wave", &wave.to_string());
    }
    if row_bool(&data, "attacked") || !row_bool(&data, "has_base") {
        let band = row_opt_str(&data, "threat_band").unwrap_or_else(|| String::from("low"));
        let text = kit::t(&format!("@threat.{band}")).to_string();
        sector_stats_value_line(&mut root, "@sectors.threat", &text);
    }
    let resources = row_arr(&stats, "resources");
    if row_bool(&data, "has_save") && !resources.is_empty() {
        let label = factory::label(&kit::t("@sectors.resources").to_string());
        root.bind_mut().add(&label).left();
        root.bind_mut().row();
        let mut resource_row = HBoxContainer::new_alloc();
        resource_row.add_theme_constant_override("separation", 3);
        for resource in resources.iter_shared() {
            let name = text_of(&resource);
            let Some(texture) = content_texture(&name) else {
                continue;
            };
            // code-instantiated: one 24 px icon per known resource.
            resource_row.add_child(&icon_rect(&texture, 24.0));
        }
        root.bind_mut().add(&resource_row).pad_left(10.0).left();
        root.bind_mut().row();
    }
    if row_bool(&stats, "lockdown") {
        // `c.add(UI.formatIcons(bundle.get("sector.lockdown"))).wrap().fillX()`.
        let lockdown = factory::label(&icon_glyph_markup(&kit::tm("@sector.lockdown").to_string()));
        let mut lockdown_rich = rich(&lockdown);
        lockdown_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
        lockdown_rich.set_h_size_flags(SizeFlags::EXPAND_FILL);
        root.bind_mut().add(&lockdown).grow_x().pad_bottom(10.0);
        root.bind_mut().row();
        apply_icon_fallback(&lockdown_rich);
    }
    sector_stats_stat_section(
        &mut root,
        "@sectors.production",
        &row_arr(&stats, "production"),
        &VarArray::new(),
    );
    let mut export_trailing = VarArray::new();
    if let Some(destination) = stats.get("destination")
        && destination.get_type() == VariantType::DICTIONARY
    {
        export_trailing.push(&destination);
    }
    sector_stats_stat_section(
        &mut root,
        "@sectors.export",
        &row_arr(&stats, "export"),
        &export_trailing,
    );
    if row_bool(&data, "has_base") {
        sector_stats_stat_section(
            &mut root,
            "@sectors.import",
            &row_arr(&stats, "import"),
            &row_arr(&stats, "import_sources"),
        );
    }
    let stored = row_arr(&stats, "stored");
    if row_bool(&data, "has_base") && !stored.is_empty() {
        let label = factory::label(&kit::t("@sectors.stored").to_string());
        root.bind_mut().add(&label).left();
        root.bind_mut().row();
        // code-instantiated: the stored pool is a runtime-sized 4-column grid.
        let mut stored_grid = MindTable::new_alloc();
        let mut index = 0i64;
        for entry in stored.iter_shared() {
            let Ok(entry) = entry.try_to::<VarArray>() else {
                continue;
            };
            if entry.len() < 2 {
                continue;
            }
            let mut values = entry.iter_shared();
            let content_name = values
                .next()
                .map(|value| text_of(&value))
                .unwrap_or_default();
            let amount = values.next().map(|value| int_of(&value)).unwrap_or(0);
            let cell = sector_stats_stored_cell(&content_name, amount);
            stored_grid.bind_mut().add(&cell);
            index += 1;
            if index % 4 == 0 {
                stored_grid.bind_mut().row();
            }
        }
        root.bind_mut().add(&stored_grid).pad_left(10.0).left();
        root.bind_mut().row();
    }
    dialog.add_close_button(210.0);
    // `abandonSectorConfirm` button, shown for sectors with a base.
    if row_bool(&data, "has_base") {
        let mut abandon = factory::icon_button(
            "cancel",
            &kit::t("@sector.abandon").to_string(),
            "cleart",
            godot::classes::box_container::AlignmentMode::BEGIN,
        );
        let target = dialog.to_gd();
        let state_for_abandon = state.clone();
        let callable = Callable::from_fn("sector_stats_abandon", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            sector_stats_confirm_abandon(&mut bound, &state_for_abandon);
        });
        abandon.connect("pressed", &callable);
        if let Some(mut row) = dialog.buttons_row() {
            row.add_child(&abandon);
        }
    }
}

/// `name [accent]value` line (`c.add(Core.bundle.get(...) + " [accent]" + ...)`).
fn sector_stats_value_line(root: &mut Gd<MindTable>, label_key: &str, value: &str) {
    let label = factory::label(&format!(
        "{} [color={STATS_ACCENT}]{}[/color]",
        kit::t(label_key),
        value
    ));
    root.bind_mut().add(&label).left().pad_bottom(5.0);
    root.bind_mut().row();
}

/// `displayItems(c, stats, name, builder)`: 3 `icon total unit.perminute` cells
/// per row behind a `lightGray` label, plus the trailing destination/source
/// lines (`Iconc.rightOpen` + sector name).
fn sector_stats_stat_section(
    root: &mut Gd<MindTable>,
    label_key: &str,
    rows: &VarArray,
    trailing: &VarArray,
) {
    let mut cells: Vec<VarArray> = Vec::new();
    for entry in rows.iter_shared() {
        let Ok(entry) = entry.try_to::<VarArray>() else {
            continue;
        };
        if entry.len() >= 2 {
            cells.push(entry);
        }
    }
    if cells.is_empty() && trailing.is_empty() {
        return;
    }
    let label = factory::label(&kit::t(label_key).to_string());
    root.bind_mut().add(&label).left();
    root.bind_mut().row();
    if !cells.is_empty() {
        // code-instantiated: runtime-sized 3-column rate grid.
        let mut grid = MindTable::new_alloc();
        let mut index = 0i64;
        for cell in &cells {
            let mut cell_row = HBoxContainer::new_alloc();
            cell_row.add_theme_constant_override("separation", 3);
            let content_name = cell
                .iter_shared()
                .next()
                .map(|value| text_of(&value))
                .unwrap_or_default();
            let amount = cell
                .iter_shared()
                .nth(1)
                .map(|value| int_of(&value))
                .unwrap_or(0);
            if let Some(texture) = content_texture(&content_name) {
                // code-instantiated: one icon per rate row.
                cell_row.add_child(&icon_rect(&texture, 24.0));
            }
            let label = factory::label(&format!("{} {}", amount, kit::t("@unit.perminute")));
            rich(&label).add_theme_color_override("default_color", LIGHT_GRAY);
            cell_row.add_child(&label);
            grid.bind_mut().add(&cell_row);
            index += 1;
            if index % 3 == 0 {
                grid.bind_mut().row();
            }
        }
        root.bind_mut().add(&grid).pad_left(10.0).left();
        root.bind_mut().row();
    }
    for source_variant in trailing.iter_shared() {
        let Ok(source) = source_variant.try_to::<VarDictionary>() else {
            continue;
        };
        let prefix = kit::glyph("right-open");
        let content_icon = row_opt_str(&source, "content_icon").unwrap_or_default();
        let icon_name = row_opt_str(&source, "icon").unwrap_or_default();
        let icon_text = if !content_icon.is_empty() {
            String::new()
        } else if !icon_name.is_empty() {
            kit::glyph(&icon_name).to_string()
        } else {
            String::new()
        };
        let spacing = if icon_text.is_empty() { "" } else { " " };
        let line = format!(
            "{} {}{}{}",
            prefix,
            icon_text,
            spacing,
            row_str(&source, "name")
        );
        let label = factory::label(&line);
        root.bind_mut().add(&label).pad_left(10.0).left();
        root.bind_mut().row();
    }
}

/// One stored cell (`res.image(uiIcon).padRight(3); res.add(formatAmount)`).
fn sector_stats_stored_cell(content_name: &str, amount: i64) -> Gd<Control> {
    // code-instantiated: one per stored item row.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 3);
    if let Some(texture) = content_texture(content_name) {
        row.add_child(&icon_rect(&texture, 24.0));
    }
    let label = factory::label(&amount.max(0).to_string());
    rich(&label).add_theme_color_override("default_color", LIGHT_GRAY);
    row.add_child(&label);
    let control: Gd<Control> = row.upcast();
    control
}

/// `BaseDialog(sector.name())`: custom name wins, then the localized preset.
fn sector_stats_localized_name(state: &Rc<RefCell<SectorStatsState>>) -> String {
    let st = state.borrow();
    if row_bool(&st.row, "named") {
        return row_str(&st.row, "name");
    }
    let preset = row_str(&st.row, "preset");
    if !preset.is_empty() {
        let key = format!("sector.{preset}.name");
        let localized = kit::t(&format!("@{key}")).to_string();
        if !localized.is_empty() && localized != key {
            return localized;
        }
    }
    row_str(&st.row, "name")
}

/// The live `campaign_views` sector row this dialog was opened for.
fn sector_stats_find_row(dialog: &mut MindDialog, planet: &str, sector_id: i64) -> VarDictionary {
    let views = dialog.campaign_json("campaign_views_json", &[gs(planet).to_variant()]);
    for row_variant in row_arr(&views, "sectors").iter_shared() {
        let Ok(row) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        if row_i64_or(&row, "id", -1) == sector_id && row_str(&row, "planet") == planet {
            return row;
        }
    }
    VarDictionary::new()
}

/// `abandonSectorConfirm`: shared confirm prompt, then the off-play record
/// clear (`MindCampaign.abandon_sector_at`) and the planet page refresh.
fn sector_stats_confirm_abandon(dialog: &mut MindDialog, state: &Rc<RefCell<SectorStatsState>>) {
    let _ = dialog;
    state.borrow_mut().pending_abandon = true;
    let _ = ui_call(
        "show_confirm",
        &[kit::t("@sector.abandon.confirm").to_variant()],
    );
}

fn sector_stats_on_confirm(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SectorStatsState>>,
    confirmed: bool,
) {
    let pending = {
        let mut st = state.borrow_mut();
        let pending = st.pending_abandon;
        st.pending_abandon = false;
        pending
    };
    if !pending {
        return;
    }
    if !confirmed {
        return;
    }
    let (planet, sector_id) = {
        let st = state.borrow();
        (st.planet.clone(), st.sector_id)
    };
    let played = sector_stats_is_played(dialog, state);
    if let Some(mut campaign) = dialog.campaign_node()
        && campaign.has_method("abandon_sector_at")
    {
        let _ = campaign.call(
            "abandon_sector_at",
            &[gs(planet).to_variant(), sector_id.to_variant()],
        );
    }
    if kit::ui().is_some() {
        close_dialog("sector_stats");
        // `sector.isBeingPlayed()` -> `hide()` (PlanetDialog.java:1153-1154):
        // the cores die 7 ticks later and the game-over path plays.
        if played {
            close_dialog("planet");
            return;
        }
    }
    if let Some(mut planet_dialog) = dialog_node("planet")
        && planet_dialog.has_method("refresh_page")
    {
        planet_dialog.call("refresh_page", &[]);
    }
}

/// `Sector.isBeingPlayed` for this dialog's sector (`get_sector_state`).
fn sector_stats_is_played(dialog: &mut MindDialog, state: &Rc<RefCell<SectorStatsState>>) -> bool {
    let (planet, sector_id) = {
        let st = state.borrow();
        (st.planet.clone(), st.sector_id)
    };
    let Some(mut campaign) = dialog.campaign_node() else {
        return false;
    };
    if !campaign.has_method("get_sector_state") {
        return false;
    }
    let value = campaign.call("get_sector_state", &[]);
    let Ok(sector_state) = value.try_to::<VarDictionary>() else {
        return false;
    };
    row_bool(&sector_state, "campaign")
        && row_str(&sector_state, "planet") == planet
        && row_i64_or(&sector_state, "sector", -1) == sector_id
}

// ---------------------------------------------------------------------------
// CustomGameDialog (MapListDialog base, display_type = false)
// ---------------------------------------------------------------------------

/// `MapListDialog.settings` persistence key (`Core.settings.getJson`).
const MAPLIST_SETTINGS_KEY: &str = "editormapviewsettings";
/// `rebuildMaps`: `float mapsize = 200f`.
pub(crate) const MAP_SIZE: f32 = 200.0;
/// `Core.graphics.getWidth() / Scl.scl(230)` columns.
pub(crate) const MAP_COLUMN_WIDTH: f32 = 230.0;
/// `button.margin(9)`.
pub(crate) const MAP_CARD_MARGIN: i32 = 9;
/// `Gamemode.all` in declaration order (`Gamemode.java:9-43`).
const GAMEMODES: [&str; 5] = ["survival", "sandbox", "attack", "pvp", "editor"];
/// Gamemodes whose `mode<Name>` icon exists in the atlas.
const MODE_ICONS: [(&str, &str); 3] = [
    ("survival", "mode-survival"),
    ("attack", "mode-attack"),
    ("pvp", "mode-pvp"),
];
/// `MapPriority.all` in declaration order with the filter-dialog icons.
const PRIORITIES: [&str; 4] = ["name", "custom", "builtin", "recent"];
const PRIORITY_ICONS: [&str; 4] = ["file-text", "players", "hammer", "play"];

/// `MapViewSettings` defaults (`MapListDialog.java:58-75`).
pub(crate) fn maplist_default_settings() -> VarDictionary {
    let mut settings = VarDictionary::new();
    settings.set("showBuiltIn", &true.to_variant());
    settings.set("showCustom", &true.to_variant());
    settings.set("showModded", &true.to_variant());
    settings.set("searchAuthor", &false.to_variant());
    settings.set("searchDescription", &false.to_variant());
    settings.set("searchModname", &false.to_variant());
    settings.set("priority", &gs("recent").to_variant());
    settings
}

/// `showMap(map)` override hook (`MapListDialog._show_map` is virtual).
pub(crate) type MapShowHook = Rc<dyn Fn(&mut MindDialog, &VarDictionary)>;

/// Controller state for the reused map-browser dialog node (`CustomGameDialog`
/// and `EditorMapsDialog` share this node type).
#[derive(Default)]
pub(crate) struct MapListState {
    pub(crate) settings: VarDictionary,
    /// `MapListDialog.modes`: the gamemode filter (not persisted).
    pub(crate) mode_filter: Vec<String>,
    /// `searchString` (lowercased; empty = null).
    pub(crate) search: String,
    pub(crate) field: Option<Gd<LineEdit>>,
    pub(crate) grid: Option<Gd<MindTable>>,
    pub(crate) filter_panel: Option<Gd<Control>>,
    pub(crate) priority_buttons: Vec<Gd<Button>>,
    /// `MapViewSettings` flag -> its filter toggle (re-synced on reload).
    pub(crate) setting_buttons: Vec<(String, Gd<Button>)>,
    /// Per-map `Gamemode.valid` cache (`MindPreview.map_modes`), keyed by path.
    pub(crate) modes_cache: Vec<(String, VarDictionary)>,
    /// Coalesces `previews_changed` batches into one grid rebuild per frame.
    pub(crate) preview_refresh_queued: bool,
    /// `MapListDialog.displayType`: `EditorMapsDialog` turns it on to show the
    /// custom/built-in/workshop source label under each card.
    pub(crate) display_type: bool,
    /// `showMap(map)` override: `EditorMapsDialog` opens the per-map `editor_map`
    /// page, `CustomGameDialog` (the default, `None`) emits `map_chosen` and
    /// opens `map_play` (`MapListDialog._show_map` virtual).
    pub(crate) show_map: Option<MapShowHook>,
    pub(crate) deferred: Deferred,
}

impl MapListState {
    pub(crate) fn new() -> Self {
        Self {
            settings: maplist_default_settings(),
            ..Self::default()
        }
    }
}

/// `CustomGameDialog extends MapListDialog` with `displayType = false`.
pub struct CustomGameDialog {
    state: Rc<RefCell<MapListState>>,
}

impl Default for CustomGameDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(MapListState::new())),
        }
    }
}

impl DialogController for CustomGameDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@customgame");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        // `makeButtonOverlay()` (`MapListDialog.java:103`).
        dialog.apply_button_overlay(true);
        maplist_load_settings(dialog, &self.state);
        maplist_build(dialog, &self.state);
        let deferred = {
            let st = self.state.borrow();
            st.deferred.clone()
        };
        install_frame_tick(dialog, "custom", deferred, || {});
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        maplist_load_settings(dialog, &self.state);
        maplist_refresh_registry(dialog, &self.state);
        // `setup`: `searchString = null` and a fresh (empty) search field.
        self.state.borrow_mut().search = String::new();
        if let Some(mut field) = self.state.borrow().field.clone() {
            field.set_text("");
        }
        maplist_rebuild(dialog, &self.state);
        // `MapListDialog.show`: desktop focuses the search field.
        if let Some(mut field) = self.state.borrow().field.clone() {
            field.call_deferred("grab_focus", &[]);
        }
    }
}

pub(crate) fn maplist_build(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    maplist_build_search(dialog, state);
    // code-instantiated: tile count and layout are driven by the Maps registry.
    let mut grid = MindTable::new_alloc();
    grid.set_mouse_filter(MouseFilter::PASS);
    let mut root = dialog.content_table();
    // `cont.add(pane).padLeft(28f)` + `mapTable.marginRight(12f)`; the table is
    // centred in the pane (`Table` default alignment), so expand (not fill).
    root.bind_mut()
        .add(&grid)
        .expand_x()
        .pad_left(28.0)
        .pad_right(12.0);
    state.borrow_mut().grid = Some(grid);
    dialog.clear_buttons();
    // `setup`: `buttons.button("@back", Icon.left, this::hide).size(210f, 64f)`.
    let close = dialog.base().callable("close_pressed");
    maplist_add_pane_button(dialog, "@back", "left", Some(close));
    maplist_rebuild(dialog, state);
}

/// `buttons.button(text, icon, listener).size(210f, 64f)` over the default
/// `Tex.button` pane (`BaseDialog.addCloseButton` look).
pub(crate) fn maplist_add_pane_button(
    dialog: &mut MindDialog,
    text_key: &str,
    icon_name: &str,
    listener: Option<Callable>,
) -> Gd<Button> {
    // code-instantiated: the dialog's button row is data-driven per dialog.
    let mut button = factory::button("");
    button.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    dialog.apply_pane_button(&mut button, OUTLINE_GRAY, 4.0);
    let glyph: Gd<Control> = glyph_control(icon_name, 24.0, Color::WHITE).upcast();
    dialog.build_button_content(&mut button, Some(glyph), &kit::t(text_key).to_string());
    if let Some(listener) = listener
        && listener.is_valid()
    {
        button.connect("pressed", &listener);
    }
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&button);
    }
    button
}

/// `search.image(Icon.zoom)`, `search.field(...).maxTextLength(50).growX()`,
/// `search.button(Icon.filter, Styles.emptyi, this::showMapFilters)`.
pub(crate) fn maplist_build_search(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    // code-instantiated: the search row is a fixed composite (zoom glyph + field
    // + filter) over the runtime map grid.
    let mut search = HBoxContainer::new_alloc();
    search.set_name("Search");
    search.add_theme_constant_override("separation", 6);
    let zoom = glyph_control("zoom", 24.0, Color::WHITE);
    search.add_child(&zoom);
    let mut field = factory::field(&kit::t("@editor.search").to_string());
    field.set_max_length(50);
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let target = dialog.to_gd();
        let state_for_search = state.clone();
        let callable = Callable::from_fn("maplist_search", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            state_for_search.borrow_mut().search = text.to_lowercase();
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            maplist_rebuild(&mut bound, &state_for_search);
        });
        field.connect("text_changed", &callable);
    }
    search.add_child(&field);
    let mut filter_button = factory::icon_button(
        "filter",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    filter_button.set_tooltip_text(&kit::t("@editor.filters"));
    filter_button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
    let target = dialog.to_gd();
    let state_for_filters = state.clone();
    let callable = Callable::from_fn("maplist_toggle_filters", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        maplist_toggle_filters(&mut bound, &state_for_filters);
    });
    filter_button.connect("pressed", &callable);
    search.add_child(&filter_button);
    let mut filter_panel = maplist_build_filter_panel(dialog, state);
    filter_panel.set_visible(false);
    let layout = dialog
        .base()
        .get_node_or_null("Center/Panel/Layout")
        .and_then(|node| node.try_cast::<VBoxContainer>().ok());
    match (layout, dialog.body()) {
        (Some(mut layout), Some(body)) => {
            // `cont.add(search)` takes `cont.defaults().pad(3)` above it.
            // code-instantiated: wraps the code-built search row.
            let mut search_pad = MarginContainer::new_alloc();
            search_pad.set_name("SearchPad");
            search_pad.add_theme_constant_override("margin_top", 3);
            search_pad.add_child(&search);
            layout.add_child(&search_pad);
            let search_node: Gd<Node> = search_pad.clone().upcast();
            layout.move_child(&search_node, body.get_index());
            layout.add_child(&filter_panel);
            let panel_node: Gd<Node> = filter_panel.clone().upcast();
            layout.move_child(&panel_node, body.get_index());
        }
        _ => {
            let mut root = dialog.content_table();
            root.bind_mut().add(&search).grow_x();
            root.bind_mut().row();
            root.bind_mut().add(&filter_panel).grow_x();
        }
    }
    let mut st = state.borrow_mut();
    st.field = Some(field);
    let control: Gd<Control> = filter_panel.clone().upcast();
    st.filter_panel = Some(control);
}

/// `showMapFilters`: gamemode + priority toggles, the map type toggles and the
/// search-in toggles (upstream opens them in a `BaseDialog`; the port keeps them
/// in an inline panel under the search row).
pub(crate) fn maplist_build_filter_panel(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
) -> Gd<PanelContainer> {
    // code-instantiated: the filter panel is a fixed upstream toggle set.
    let mut panel = PanelContainer::new_alloc();
    panel.set_name("Filters");
    panel.set_theme_type_variation("smallPane");
    let mut menu = VBoxContainer::new_alloc();
    menu.add_theme_constant_override("separation", 6);
    panel.add_child(&menu);
    let mut tabs = HBoxContainer::new_alloc();
    tabs.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    tabs.add_theme_constant_override("separation", 10);
    menu.add_child(&tabs);
    let tabs_node: Gd<Node> = tabs.clone().upcast();
    // `t.add("@editor.filters.mode")` + one `Styles.emptyTogglei` 60 px toggle
    // per gamemode with an icon.
    let mut mode_box = maplist_filter_box(&tabs_node, "@editor.filters.mode");
    for mode in GAMEMODES {
        let Some((_, icon)) = MODE_ICONS.iter().find(|(name, _)| *name == mode) else {
            continue;
        };
        let mut mode_button = factory::glyph_button(
            icon,
            &kit::t(&format!("@mode.{mode}.name")).to_string(),
            "emptyTogglei",
        );
        mode_button.set_toggle_mode(true);
        mode_button.set_custom_minimum_size(Vector2::new(60.0, 60.0));
        mode_button.set_pressed(state.borrow().mode_filter.iter().any(|value| value == mode));
        mode_button.set_tooltip_text(&kit::t(&format!("@mode.{mode}.name")));
        let target = dialog.to_gd();
        let state_for_mode = state.clone();
        let mode_name = mode.to_string();
        let callable = Callable::from_fn("maplist_mode_toggle", move |args: &[&Variant]| {
            let pressed = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            {
                let mut st = state_for_mode.borrow_mut();
                let present = st.mode_filter.contains(&mode_name);
                if pressed && !present {
                    st.mode_filter.push(mode_name.clone());
                } else if !pressed && present {
                    st.mode_filter.retain(|value| *value != mode_name);
                }
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            maplist_rebuild(&mut bound, &state_for_mode);
        });
        mode_button.connect("toggled", &callable);
        mode_box.add_child(&mode_button);
    }
    // `t.add("@editor.filters.priorities")`: one checked-when-current toggle per
    // `MapPriority`.
    let mut priority_box = maplist_filter_box(&tabs_node, "@editor.filters.priorities");
    state.borrow_mut().priority_buttons.clear();
    for (index, priority) in PRIORITIES.iter().enumerate() {
        let mut priority_button =
            factory::glyph_button(PRIORITY_ICONS[index], priority, "emptyTogglei");
        priority_button.set_toggle_mode(true);
        priority_button.set_custom_minimum_size(Vector2::new(60.0, 60.0));
        priority_button
            .set_tooltip_text(&kit::t(&format!("@editor.filters.prioritize.{priority}")));
        let target = dialog.to_gd();
        let state_for_priority = state.clone();
        let priority_name = priority.to_string();
        let callable = Callable::from_fn("maplist_priority", move |_args: &[&Variant]| {
            {
                let mut st = state_for_priority.borrow_mut();
                st.settings.set("priority", &gs(priority_name.as_str()));
            }
            maplist_sync_priority_buttons(&state_for_priority);
            maplist_save_settings(&state_for_priority);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            maplist_rebuild(&mut bound, &state_for_priority);
        });
        priority_button.connect("pressed", &callable);
        priority_box.add_child(&priority_button);
        state.borrow_mut().priority_buttons.push(priority_button);
    }
    maplist_sync_priority_buttons(state);
    // `menu.add("@editor.filters.type")` + custom/builtin/modded (150x60).
    let menu_node: Gd<Node> = menu.clone().upcast();
    let mut type_box = maplist_filter_box(&menu_node, "@editor.filters.type");
    maplist_add_setting_toggle(dialog, state, &mut type_box, "@custom", "showCustom");
    maplist_add_setting_toggle(dialog, state, &mut type_box, "@builtin", "showBuiltIn");
    maplist_add_setting_toggle(dialog, state, &mut type_box, "@modded", "showModded");
    // `menu.add("@editor.filters.search")` + author/description/modname.
    let mut search_box = maplist_filter_box(&menu_node, "@editor.filters.search");
    maplist_add_setting_toggle(
        dialog,
        state,
        &mut search_box,
        "@editor.filters.author",
        "searchAuthor",
    );
    maplist_add_setting_toggle(
        dialog,
        state,
        &mut search_box,
        "@editor.filters.description",
        "searchDescription",
    );
    maplist_add_setting_toggle(
        dialog,
        state,
        &mut search_box,
        "@editor.filters.modname",
        "searchModname",
    );
    panel
}

/// A titled `Tex.button` box (`t.add(title).row(); t.table(Tex.button, ...)`);
/// returns the row the toggles go into.
pub(crate) fn maplist_filter_box(parent: &Gd<Node>, title_key: &str) -> Gd<HBoxContainer> {
    // code-instantiated: filter group boxes are part of the fixed filter panel.
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 6);
    let label = factory::label(&kit::t(title_key).to_string());
    column.add_child(&label);
    let mut box_ = PanelContainer::new_alloc();
    let pane = MindDialog::pane_box(Color::BLACK, OUTLINE_GRAY, 4.0, 2.0, -1.0);
    box_.add_theme_stylebox_override("panel", &pane);
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 0);
    box_.add_child(&row);
    column.add_child(&box_);
    let mut parent = parent.clone();
    parent.add_child(&column);
    row
}

/// A `Styles.flatTogglet` 150x60 toggle bound to one `MapViewSettings` flag.
pub(crate) fn maplist_add_setting_toggle(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    row: &mut Gd<HBoxContainer>,
    key: &str,
    setting: &str,
) {
    // code-instantiated: filter toggles are data-driven (one per settings flag).
    let mut button = factory::button(&kit::t(key).to_string());
    button.set_theme_type_variation("flatTogglet");
    button.set_toggle_mode(true);
    button.set_custom_minimum_size(Vector2::new(150.0, 60.0));
    button.set_pressed(row_bool(&state.borrow().settings, setting));
    let target = dialog.to_gd();
    let state_for_toggle = state.clone();
    let setting_name = setting.to_string();
    let callable = Callable::from_fn("maplist_setting_toggle", move |args: &[&Variant]| {
        let pressed = args
            .first()
            .map(|value| (*value).to::<bool>())
            .unwrap_or(false);
        {
            let mut st = state_for_toggle.borrow_mut();
            st.settings
                .set(&gs(setting_name.as_str()), &pressed.to_variant());
        }
        maplist_save_settings(&state_for_toggle);
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        maplist_rebuild(&mut bound, &state_for_toggle);
    });
    button.connect("toggled", &callable);
    row.add_child(&button);
    state
        .borrow_mut()
        .setting_buttons
        .push((setting.to_string(), button));
}

pub(crate) fn maplist_sync_priority_buttons(state: &Rc<RefCell<MapListState>>) {
    let (buttons, priority) = {
        let st = state.borrow();
        (
            st.priority_buttons.clone(),
            row_str(&st.settings, "priority"),
        )
    };
    for (index, button) in buttons.iter().enumerate() {
        let Some(priority_name) = PRIORITIES.get(index) else {
            continue;
        };
        set_toggle(button, *priority_name == priority);
    }
}

pub(crate) fn maplist_toggle_filters(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    let _ = dialog;
    let panel = {
        let st = state.borrow();
        st.filter_panel.clone()
    };
    let Some(mut panel) = panel else {
        return;
    };
    let visible = panel.is_visible();
    panel.set_visible(!visible);
}

/// `Core.settings.getJson("editormapviewsettings", ...)`: shared by both map
/// dialogs, so it is re-read on every `setup`.
pub(crate) fn maplist_load_settings(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    let _ = dialog;
    let text = setting_text(MAPLIST_SETTINGS_KEY, "");
    if text.trim().is_empty() {
        return;
    }
    let stored = parse_object(&text);
    let keys: Vec<String> = state
        .borrow()
        .settings
        .keys_array()
        .iter_shared()
        .map(|value| text_of(&value))
        .collect();
    {
        let mut st = state.borrow_mut();
        for key in keys {
            if let Some(value) = stored.get(&gs(key.as_str())) {
                st.settings.set(&gs(key.as_str()), &value);
            }
        }
        let priority = row_str(&st.settings, "priority");
        if !PRIORITIES.contains(&priority.as_str()) {
            st.settings.set("priority", &gs("recent"));
        }
    }
    maplist_sync_priority_buttons(state);
    let buttons = state.borrow().setting_buttons.clone();
    for (setting, button) in buttons {
        let pressed = row_bool(&state.borrow().settings, &setting);
        set_toggle(&button, pressed);
    }
}

/// `MapViewSettings.save` (`activeDialog.hidden(settings::save)`).
pub(crate) fn maplist_save_settings(state: &Rc<RefCell<MapListState>>) {
    let settings = {
        let st = state.borrow();
        st.settings.clone()
    };
    setting_put_text(MAPLIST_SETTINGS_KEY, &json_stringify(&settings));
}

/// Reloads the live `MindPreview` registry and asks it for the missing previews
/// (`Maps.loadPreviews` + `createAllPreviews`); batches land through
/// `previews_changed`.
pub(crate) fn maplist_refresh_registry(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    state.borrow_mut().modes_cache.clear();
    let Some(preview) = preview_node() else {
        return;
    };
    let mut node: Gd<Node> = preview.upcast();
    if node.has_method("refresh") {
        node.call("refresh", &[]);
    }
    let target = dialog.to_gd();
    let state_for_preview = state.clone();
    let callable = Callable::from_fn("maplist_previews_changed", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        maplist_on_previews_changed(&mut bound, &state_for_preview);
    });
    connect_once(
        dialog,
        "maplist_previews",
        &node,
        "previews_changed",
        &callable,
    );
    if node.has_method("generate_previews") {
        node.call("generate_previews", &[]);
    }
}

/// A generated preview batch landed (`Maps.createAllPreviews`); its scan facts
/// (`Map.spawns`/`teams`) now exist, so drop the mode cache and refresh the
/// tiles once per frame while batches keep arriving.
pub(crate) fn maplist_on_previews_changed(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
) {
    if state.borrow().preview_refresh_queued {
        return;
    }
    let deferred = {
        let st = state.borrow();
        st.deferred.clone()
    };
    let target = dialog.to_gd();
    let state_for_rebuild = state.clone();
    state.borrow_mut().preview_refresh_queued = true;
    deferred.push(move || {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        maplist_rebuild_after_previews(&mut bound, &state_for_rebuild);
    });
}

pub(crate) fn maplist_rebuild_after_previews(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
) {
    state.borrow_mut().preview_refresh_queued = false;
    state.borrow_mut().modes_cache.clear();
    maplist_rebuild(dialog, state);
}

/// The live registry rows (`MindPreview.maps_list`), else the `campaign_views`
/// `maps` fallback for an empty registry.
pub(crate) fn maplist_maps(dialog: &mut MindDialog) -> Vec<VarDictionary> {
    let mut rows: Vec<VarDictionary> = Vec::new();
    if let Some(preview) = preview_node() {
        for row in preview.bind().maps_list().iter_shared() {
            if !row_str(&row, "name").is_empty() {
                rows.push(row);
            }
        }
    }
    if rows.is_empty() {
        for row_variant in dialog.campaign_section("maps").iter_shared() {
            let Ok(row) = row_variant.try_to::<VarDictionary>() else {
                continue;
            };
            if !row_str(&row, "name").is_empty() {
                rows.push(row);
            }
        }
    }
    rows
}

/// One sortable map row (`MapListDialog._sorted_maps` keyed entry).
struct MapSortKey {
    row: VarDictionary,
    index: usize,
    name: String,
    custom: bool,
    modified: i64,
}

/// `maps.customMaps()` + `defaultMaps()` + `moddedMaps()` (each list filtered
/// by its `MapViewSettings` flag), `distinct()`, then
/// `sort(settings.priority.comparator)`.
pub(crate) fn maplist_sorted_maps(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
) -> Vec<VarDictionary> {
    let mut customs: Vec<VarDictionary> = Vec::new();
    let mut builtins: Vec<VarDictionary> = Vec::new();
    let mut modded: Vec<VarDictionary> = Vec::new();
    for row in maplist_maps(dialog) {
        if row_bool(&row, "custom") {
            customs.push(row);
        } else if row_bool(&row, "mod") {
            modded.push(row);
        } else {
            builtins.push(row);
        }
    }
    let settings = {
        let st = state.borrow();
        st.settings.clone()
    };
    let mut entries: Vec<VarDictionary> = Vec::new();
    if row_bool_or(&settings, "showCustom", true) {
        entries.extend(customs);
    }
    if row_bool_or(&settings, "showBuiltIn", true) {
        entries.extend(builtins);
    }
    if row_bool_or(&settings, "showModded", true) {
        entries.extend(modded);
    }
    // `Seq.sort` is stable (TimSort); carry the list index as the final key.
    let mut keyed: Vec<MapSortKey> = entries
        .iter()
        .enumerate()
        .map(|(index, row)| MapSortKey {
            row: row.clone(),
            index,
            name: row_str(row, "name"),
            custom: row_bool(row, "custom"),
            modified: row_i64(row, "lastModified"),
        })
        .collect();
    let priority = row_opt_str(&settings, "priority").unwrap_or_else(|| String::from("recent"));
    keyed.sort_by(|a, b| maplist_compare(&priority, a, b));
    keyed.into_iter().map(|entry| entry.row).collect()
}

/// `MapPriority` comparators (`MapListDialog.java:44-50`); a negative result
/// orders `a` first. `String.compareTo` is a code-point compare.
fn maplist_compare(priority: &str, a: &MapSortKey, b: &MapSortKey) -> Ordering {
    let mut result = match priority {
        "name" => a.name.cmp(&b.name),
        "custom" => {
            // `comparingBool(m -> !m.custom)` then `comparingLong(-lastModified)`.
            let first = (if a.custom { 0 } else { 1 }).cmp(&(if b.custom { 0 } else { 1 }));
            if first == Ordering::Equal {
                (-a.modified).cmp(&(-b.modified))
            } else {
                first
            }
        }
        "builtin" => {
            let first = (if a.custom { 1 } else { 0 }).cmp(&(if b.custom { 1 } else { 0 }));
            if first == Ordering::Equal {
                (-a.modified).cmp(&(-b.modified))
            } else {
                first
            }
        }
        _ => {
            // `recent`: `comparingLong(-lastModified)` then `comparing(Map::name)`.
            let first = (-a.modified).cmp(&(-b.modified));
            if first == Ordering::Equal {
                a.name.cmp(&b.name)
            } else {
                first
            }
        }
    };
    if result == Ordering::Equal {
        result = (a.index as i64).cmp(&(b.index as i64));
    }
    result
}

/// `Gamemode.valid(map)` (`Gamemode.java`): survival needs `spawns > 0`,
/// attack/pvp need `teams.size > 1`, sandbox/editor are always valid.
pub(crate) fn maplist_map_modes(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
) -> VarDictionary {
    let _ = dialog;
    let key = row_opt_str(map, "path").unwrap_or_else(|| row_str(map, "name"));
    if let Some((_, cached)) = state
        .borrow()
        .modes_cache
        .iter()
        .find(|(cached_key, _)| *cached_key == key)
    {
        return cached.clone();
    }
    let mut modes = VarDictionary::new();
    if row_bool_or(map, "preview", true)
        && let Some(mut preview) = preview_node()
    {
        modes = preview.bind_mut().map_modes(gs(row_str(map, "name")));
    }
    state.borrow_mut().modes_cache.push((key, modes.clone()));
    modes
}

pub(crate) fn maplist_mode_valid(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
    mode: &str,
) -> bool {
    if mode == "sandbox" || mode == "editor" {
        return true;
    }
    row_bool_or(&maplist_map_modes(dialog, state, map), mode, false)
}

/// The gamemode filter (`invalid |= !mode.valid(map)`) and the search over
/// `plainName` (+ author/description/mod name per `MapViewSettings`).
pub(crate) fn maplist_passes_filter(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
) -> bool {
    let (mode_filter, search, settings) = {
        let st = state.borrow();
        (
            st.mode_filter.clone(),
            st.search.clone(),
            st.settings.clone(),
        )
    };
    for mode in &mode_filter {
        if !maplist_mode_valid(dialog, state, map, mode) {
            return false;
        }
    }
    if search.is_empty() {
        return true;
    }
    let needle = search.as_str();
    if strip_color_tags(&row_str(map, "name")).contains(needle) {
        return true;
    }
    if row_bool(&settings, "searchAuthor")
        && strip_color_tags(&row_str(map, "author")).contains(needle)
    {
        return true;
    }
    if row_bool(&settings, "searchDescription")
        && strip_color_tags(&row_str(map, "description")).contains(needle)
    {
        return true;
    }
    if row_bool(&settings, "searchModname")
        && strip_color_tags(&row_str(map, "mod_name")).contains(needle)
    {
        return true;
    }
    false
}

/// `MapListDialog.rebuildMaps`: one card per filtered map, `maxwidth` per row.
pub(crate) fn maplist_rebuild(dialog: &mut MindDialog, state: &Rc<RefCell<MapListState>>) {
    let Some(mut grid) = state.borrow().grid.clone() else {
        return;
    };
    grid.bind_mut().clear();
    let maxwidth = ((dialog.base().get_viewport_rect().size.x / MAP_COLUMN_WIDTH) as i64).max(1);
    let mut index = 0i64;
    let mut shown_any = false;
    for map in maplist_sorted_maps(dialog, state) {
        if !maplist_passes_filter(dialog, state, &map) {
            continue;
        }
        shown_any = true;
        if index > 0 && index % maxwidth == 0 {
            grid.bind_mut().row();
        }
        // `mapTable.button("", Styles.grayt, ...).width(mapsize).bottom().pad(8)`.
        let tile = maplist_build_map_tile(dialog, state, &map);
        grid.bind_mut().add(&tile).width(MAP_SIZE).bottom().pad(8.0);
        index += 1;
    }
    if !shown_any {
        let label = factory::label(&kit::tm("@maps.none").to_string());
        grid.bind_mut().add(&label).pad(8.0);
    }
}

/// One `rebuildMaps` card (`MapListDialog.java:165-205`).
pub(crate) fn maplist_build_map_tile(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
) -> Gd<Button> {
    let map_name = row_str(map, "name");
    // code-instantiated: map tiles are data-driven from the Maps registry.
    let mut tile = factory::button("");
    tile.set_theme_type_variation("grayt");
    apply_grayt(&mut tile);
    tile.set_custom_minimum_size(Vector2::new(MAP_SIZE, 0.0));
    tile.set_clip_contents(true);
    {
        let target = dialog.to_gd();
        let state_for_pick = state.clone();
        let map_for_pick = map.clone();
        let callable = Callable::from_fn("maplist_choose_map", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            maplist_show_map_for(&mut bound, &state_for_pick, &map_for_pick);
        });
        tile.connect("pressed", &callable);
    }
    // code-instantiated: `button.margin(9)` inset around the card column.
    let mut inset = MarginContainer::new_alloc();
    inset.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    inset.set_mouse_filter(MouseFilter::IGNORE);
    for side in ["left", "right", "top", "bottom"] {
        inset.add_theme_constant_override(&format!("margin_{side}"), MAP_CARD_MARGIN);
    }
    tile.add_child(&inset);
    // code-instantiated: the card column (`button.bottom()`: content at the
    // bottom).
    let mut column = VBoxContainer::new_alloc();
    column.set_alignment(godot::classes::box_container::AlignmentMode::END);
    column.set_mouse_filter(MouseFilter::IGNORE);
    column.add_theme_constant_override("separation", 0);
    inset.add_child(&column);
    let mode_row = maplist_build_mode_row(dialog, state, map);
    column.add_child(&mode_row);
    // `button.add(map.name()).width(mapsize - 18f).center().get().setEllipsis(true)`.
    // code-instantiated: the name needs native ellipsis trimming the rich label
    // lacks.
    let mut name_label = Label::new_alloc();
    name_label.set_text(&gs(map_name.clone()));
    name_label.set_clip_text(true);
    name_label.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
    name_label.set_custom_minimum_size(Vector2::new(MAP_SIZE - 18.0, 0.0));
    name_label.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    name_label.set_mouse_filter(MouseFilter::IGNORE);
    column.add_child(&name_label);
    // `button.image().growX().pad(4).color(Pal.gray)`: the 3 px `whiteui` rule.
    // code-instantiated: divider between the map name and the preview.
    let mut rule_pad = MarginContainer::new_alloc();
    rule_pad.set_mouse_filter(MouseFilter::IGNORE);
    for side in ["left", "right", "top", "bottom"] {
        rule_pad.add_theme_constant_override(&format!("margin_{side}"), 4);
    }
    let mut rule = ColorRect::new_alloc();
    rule.set_color(PAL_GRAY);
    rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    rule.set_mouse_filter(MouseFilter::IGNORE);
    rule_pad.add_child(&rule);
    column.add_child(&rule_pad);
    // `button.stack(new Image(map.safeTexture()).setScaling(Scaling.fit),
    // new BorderImage(map.safeTexture()).setScaling(Scaling.fit)).size(mapsize - 20f)`.
    let preview = maplist_build_preview(&map_name, MAP_SIZE - 20.0, 4.0);
    column.add_child(&preview);
    if state.borrow().display_type {
        // `button.add(map.custom ? "@custom" : map.workshop ? "@workshop" :
        // map.mod != null ? "[lightgray]" + modName : "@builtin")
        // .color(Color.gray).padTop(3f)`: the source label row.
        // code-instantiated: the source label row is data-driven per map.
        let mut type_label = Label::new_alloc();
        type_label.set_text(&maplist_source_label(map));
        type_label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        type_label.add_theme_color_override("font_color", styles::ARC_GRAY);
        type_label.set_mouse_filter(MouseFilter::IGNORE);
        // code-instantiated: `padTop(3f)` spacer above the source label.
        let mut spacer = Control::new_alloc();
        spacer.set_custom_minimum_size(Vector2::new(0.0, 3.0));
        spacer.set_mouse_filter(MouseFilter::IGNORE);
        column.add_child(&spacer);
        column.add_child(&type_label);
    }
    // A `Button` is not a container: size the card to its column once the theme
    // fonts resolve (the card height is the content height, like Arc's
    // `TextButton` pref size).
    let inset_for_ready = inset.clone();
    let tile_for_ready = tile.clone();
    let callable = Callable::from_fn("maplist_tile_ready", move |_args: &[&Variant]| {
        let inset = inset_for_ready.clone();
        let mut tile = tile_for_ready.clone();
        let height = inset.get_combined_minimum_size().y;
        let mut minimum = tile.get_custom_minimum_size();
        minimum.y = height;
        tile.set_custom_minimum_size(minimum);
    });
    tile.connect("ready", &callable);
    tile
}

/// `button.table(t -> { t.left(); for(Gamemode mode : Gamemode.all) ... })`:
/// a 16 px icon (pad 4) per valid mode whose icon exists, or one empty 16 px
/// cell when none.
pub(crate) fn maplist_build_mode_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
) -> Gd<Control> {
    // code-instantiated: the mode-icon row reflects the map's valid Gamemodes.
    let mut pad = MarginContainer::new_alloc();
    pad.set_mouse_filter(MouseFilter::IGNORE);
    pad.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    for side in ["left", "right", "top", "bottom"] {
        pad.add_theme_constant_override(&format!("margin_{side}"), 4);
    }
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    pad.add_child(&row);
    for mode in GAMEMODES {
        let Some((_, icon)) = MODE_ICONS.iter().find(|(name, _)| *name == mode) else {
            continue;
        };
        if !maplist_mode_valid(dialog, state, map, mode) {
            continue;
        }
        row.add_child(&maplist_icon_cell(icon, 16.0));
    }
    if row.get_child_count() == 0 {
        // code-instantiated: `t.add().size(16f).pad(4f)` placeholder cell.
        let mut empty = Control::new_alloc();
        empty.set_custom_minimum_size(Vector2::new(16.0, 16.0));
        empty.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&empty);
    }
    let control: Gd<Control> = pad.upcast();
    control
}

/// A fixed-size icon cell (`t.image(icon).size(16f)`) holding an icon glyph.
pub(crate) fn maplist_icon_cell(icon_name: &str, extent: f32) -> Gd<Control> {
    // code-instantiated: icon cells are data-driven (one per valid gamemode).
    let mut cell = Control::new_alloc();
    cell.set_custom_minimum_size(Vector2::new(extent, extent));
    cell.set_mouse_filter(MouseFilter::IGNORE);
    let mut glyph = glyph_control(icon_name, extent, Color::WHITE);
    glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    glyph.set_h_grow_direction(GrowDirection::BOTH);
    glyph.set_v_grow_direction(GrowDirection::BOTH);
    glyph.set_mouse_filter(MouseFilter::IGNORE);
    cell.add_child(&glyph);
    let control: Gd<Control> = cell.upcast();
    control
}

/// The fitted map preview (`Scaling.fit` inside a `size x size` cell with a
/// `BorderImage` stroke), or the `nomap` placeholder until the batched
/// generation lands (`map.safeTexture()`).
pub(crate) fn maplist_build_preview(map_name: &str, extent: f32, thickness: f32) -> Gd<Control> {
    if let Some(texture) = preview_texture(map_name) {
        // code-instantiated: the preview texture comes from the map registry.
        let mut image = border_preview(&texture, Vector2::new(extent, extent), thickness);
        image.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        image.set_mouse_filter(MouseFilter::IGNORE);
        return image;
    }
    // code-instantiated: the `nomap` placeholder until map textures exist.
    let mut placeholder = PanelContainer::new_alloc();
    placeholder.set_theme_type_variation("smallPane");
    placeholder.set_custom_minimum_size(Vector2::new(extent, extent));
    placeholder.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    placeholder.set_mouse_filter(MouseFilter::IGNORE);
    let glyph = glyph_control("map", 48.0, Color::WHITE);
    placeholder.add_child(&glyph);
    let control: Gd<Control> = placeholder.upcast();
    control
}

/// `_source_label(map)`: `@custom` / `@workshop` / the mod name / `@builtin`.
pub(crate) fn maplist_source_label(map: &VarDictionary) -> GString {
    if row_bool(map, "custom") {
        return kit::t("@custom");
    }
    if row_bool(map, "workshop") {
        return kit::t("@workshop");
    }
    if row_bool(map, "mod") {
        return gs(row_str(map, "mod_name"));
    }
    kit::t("@builtin")
}

/// `showMap(map)`: the per-controller override
/// ([`MapListState::show_map`]) or `CustomGameDialog`'s route.
pub(crate) fn maplist_show_map_for(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapListState>>,
    map: &VarDictionary,
) {
    let hook = state.borrow().show_map.clone();
    if let Some(hook) = hook {
        hook(dialog, map);
        return;
    }
    maplist_show_map(dialog, map);
}

/// `showMap(map)`: `CustomGameDialog` emits `map_chosen` then opens `map_play`.
pub(crate) fn maplist_show_map(dialog: &mut MindDialog, map: &VarDictionary) {
    let map_name = row_str(map, "name");
    dialog
        .base_mut()
        .emit_signal("map_chosen", &[gs(map_name.as_str()).to_variant()]);
    let mut context = VarDictionary::new();
    context.set("map", &gs(map_name));
    open_dialog("map_play", &context);
}

// ---------------------------------------------------------------------------
// MapPlayDialog
// ---------------------------------------------------------------------------

/// Visible `Gamemode` names in declaration order (`Gamemode.all` minus editor).
const MAP_PLAY_MODES: [&str; 4] = ["survival", "sandbox", "attack", "pvp"];
/// `MapPlayDialog` mode-button size (`size(140f, mobile ? 44f : 54f)`).
const MODE_BUTTON: Vector2 = Vector2::new(140.0, 54.0);
/// `size(mobile && !portrait ? 150f : 250f)`.
const PREVIEW_SIZE: Vector2 = Vector2::new(250.0, 250.0);
/// `buttons.button(...).size(210f, 64f)` (`addCloseButton` + `@play`).
const MAP_PLAY_ACTION_BUTTON: Vector2 = Vector2::new(210.0, 64.0);

/// Controller state for the reused gamemode-selection dialog node.
#[derive(Default)]
struct MapPlayState {
    map_name: String,
    mode: String,
    /// Working rules JSON (`MapPlayDialog.rules`); empty = the map's own rules.
    rules_json: String,
    /// `MapPlayDialog.show(map, playtesting)`: the editor playtest route.
    playtest: bool,
    mode_buttons: Vec<Gd<Button>>,
    button_modes: Vec<String>,
}

impl MapPlayState {
    fn new() -> Self {
        Self {
            mode: String::from("survival"),
            ..Self::default()
        }
    }
}

/// Gamemode selection, the mode help list, Customize Rules, the map preview and
/// the high score (`MapPlayDialog`).
pub struct MapPlayDialog {
    state: Rc<RefCell<MapPlayState>>,
}

impl Default for MapPlayDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(MapPlayState::new())),
        }
    }
}

impl DialogController for MapPlayDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        map_play_floating_layout(dialog);
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let map_name = dialog.context_string("map").to_string();
        let requested = dialog.context_string("mode").to_string();
        let rules = dialog.context_string("rules").to_string();
        let playtest = dialog
            .context_value("playtest")
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        let mut st = self.state.borrow_mut();
        st.map_name = map_name;
        if MAP_PLAY_MODES.contains(&requested.as_str()) {
            st.mode = requested;
        }
        st.rules_json = rules;
        st.playtest = playtest;
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let mut st = self.state.borrow_mut();
        if st.map_name.is_empty() {
            st.map_name = dialog.context_string("map").to_string();
        }
        drop(st);
        map_play_build(dialog, &self.state);
    }
}

/// `super(""); setFillParent(false)` with `Styles.defaultDialog`
/// (`background = windowEmpty`, `stageBackground = black9`): the content floats
/// centred over the dim with no boxed panel and no scrolling body.
fn map_play_floating_layout(dialog: &mut MindDialog) {
    if let Some(mut panel) = dialog.panel() {
        panel.set_theme_type_variation("none");
        panel.set_custom_minimum_size(Vector2::ZERO);
        // The floating table still carries `windowEmpty`'s 4 px inset above the
        // (unpadded) title row, and under the buttons the button's own `pad(3)`
        // plus the `buttons` cell's `pad(3)` (`Dialog.java:208-214`).
        let mut inset = StyleBoxEmpty::new_gd();
        inset.set_content_margin(Side::TOP, 4.0);
        inset.set_content_margin(Side::BOTTOM, 4.0 + 3.0 + 3.0);
        panel.add_theme_stylebox_override("panel", &inset);
    }
    if let Some(mut body) = dialog.body() {
        // The body reports its content height so nothing is clipped/scrolled.
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
        body.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    }
    if let Some(mut buttons) = dialog.buttons_row() {
        buttons.add_theme_constant_override("separation", 8);
    }
    dialog.title_color = styles::ACCENT;
}

fn map_play_build(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>) {
    let (map_name, mut mode, mut rules_json) = {
        let st = state.borrow();
        (st.map_name.clone(), st.mode.clone(), st.rules_json.clone())
    };
    let title = if map_name.is_empty() {
        kit::t("@map.play").to_string()
    } else {
        map_name.clone()
    };
    dialog.set_title_literal(&title);
    // `cont.clearChildren()` (`MapPlayDialog.java:45`): the body is rebuilt per
    // shown map.
    dialog.clear_content();
    let modes = map_play_mode_validity(dialog, &map_name);
    // `if(!selectedGamemode.valid(map)) selectedGamemode = first valid || survival`.
    if !row_bool_or(&modes, &mode, true) {
        let mut picked = String::new();
        for candidate in MAP_PLAY_MODES {
            if row_bool_or(&modes, candidate, false) {
                picked = candidate.to_string();
                break;
            }
        }
        mode = if picked.is_empty() {
            String::from("survival")
        } else {
            picked
        };
    }
    if rules_json.is_empty() {
        rules_json = map_play_mode_rules(dialog, &map_name, &mode);
    }
    {
        let mut st = state.borrow_mut();
        st.mode = mode.clone();
        st.rules_json = rules_json;
    }
    let mut root = dialog.content_table();
    // `Table selmode`: `add("@level.mode").colspan(2)`, then the `Tex.button`
    // mode table and the `?` button on one row (`MapPlayDialog.java:56-71`).
    let mut selmode = factory::table();
    let mode_label = factory::label(&kit::t("@level.mode").to_string());
    selmode.bind_mut().add(&mode_label).colspan(2);
    selmode.bind_mut().row();
    // code-instantiated: `selmode.table(Tex.button, modes -> ...)` frame.
    let mut mode_frame = PanelContainer::new_alloc();
    let mut frame_box = MindDialog::pane_box(Color::BLACK, OUTLINE_GRAY, 4.0, 2.0, -1.0);
    // `Tex.button` pads its content 12 px (the 9-patch splits): the Java mode
    // box is 304x132 around the 2x2 140x54 buttons (java/mapplay-caldera.png).
    for side in [Side::LEFT, Side::RIGHT, Side::TOP, Side::BOTTOM] {
        frame_box.set_content_margin(side, 12.0);
    }
    mode_frame.add_theme_stylebox_override("panel", &frame_box);
    let mut mode_box = factory::table();
    mode_frame.add_child(&mode_box);
    state.borrow_mut().mode_buttons.clear();
    state.borrow_mut().button_modes.clear();
    for (index, candidate) in MAP_PLAY_MODES.iter().enumerate() {
        // code-instantiated: one toggle per visible Gamemode (`Gamemode.all`).
        let mut button = factory::button(&kit::t(&format!("@mode.{candidate}.name")).to_string());
        button.set_theme_type_variation("flatToggleMenut");
        apply_flat_toggle_menu(&mut button);
        button.set_toggle_mode(true);
        button.set_pressed(*candidate == mode);
        button.set_disabled(!row_bool_or(&modes, candidate, true));
        button.set_custom_minimum_size(MODE_BUTTON);
        let target = dialog.to_gd();
        let state_for_mode = state.clone();
        let mode_name = candidate.to_string();
        let callable = Callable::from_fn("map_play_select_mode", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            map_play_select_mode(&mut bound, &state_for_mode, &mode_name);
        });
        button.connect("pressed", &callable);
        mode_box
            .bind_mut()
            .add(&button)
            .width(MODE_BUTTON.x)
            .height(MODE_BUTTON.y);
        if index % 2 == 1 {
            mode_box.bind_mut().row();
        }
        state.borrow_mut().mode_buttons.push(button);
        state.borrow_mut().button_modes.push(candidate.to_string());
    }
    selmode.bind_mut().add(&mode_frame);
    // `selmode.button("?", this::displayGameModeHelp).width(50f).fillY().padLeft(18f)`.
    let mut help = factory::button("?");
    dialog.apply_pane_button(&mut help, OUTLINE_GRAY, 4.0);
    help.set_custom_minimum_size(Vector2::new(50.0, 0.0));
    let target = dialog.to_gd();
    let callable = Callable::from_fn("map_play_help", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        map_play_show_mode_help(&mut bound);
    });
    help.connect("pressed", &callable);
    selmode
        .bind_mut()
        .add(&help)
        .width(50.0)
        .fill_y()
        .pad_left(18.0);
    root.bind_mut().add(&selmode).expand_x();
    root.bind_mut().row();
    // `cont.button("@customize", Icon.settings, ...).height(50f).width(230)`.
    let mut customize = factory::button("");
    dialog.apply_pane_button(&mut customize, OUTLINE_GRAY, 4.0);
    let glyph: Gd<Control> = glyph_control("settings", 24.0, Color::WHITE).upcast();
    let customize_text = kit::t("@customize").to_string();
    dialog.build_button_content(&mut customize, Some(glyph), &customize_text);
    customize.set_custom_minimum_size(Vector2::new(230.0, 50.0));
    let target = dialog.to_gd();
    let state_for_rules = state.clone();
    let callable = Callable::from_fn("map_play_open_rules", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        map_play_open_rules(&mut bound, &state_for_rules);
    });
    customize.connect("pressed", &callable);
    root.bind_mut().add(&customize).width(230.0).height(50.0);
    root.bind_mut().row();
    // `cont.add(new BorderImage(map.safeTexture(), 3f)).size(250f)
    // .get().setScaling(Scaling.fit)`: the whole preview, fitted.
    if let Some(texture) = preview_texture(&map_name) {
        // code-instantiated: the preview texture comes from the map registry.
        let mut image = border_preview(&texture, PREVIEW_SIZE, 3.0);
        image.set_mouse_filter(MouseFilter::IGNORE);
        root.bind_mut()
            .add(&image)
            .width(PREVIEW_SIZE.x)
            .height(PREVIEW_SIZE.y);
    }
    // `if(Gamemode.survival.valid(map)) cont.label(level.highscore).pad(3f)`.
    if row_bool_or(&modes, "survival", false) {
        root.bind_mut().row();
        let label = factory::label(&map_play_high_score(&map_name));
        root.bind_mut().add(&label).pad(3.0);
    }
    dialog.clear_buttons();
    // `addCloseButton()` (210x64) then `buttons.button("@play", Icon.play, ...)
    // .size(210f, 64f)`.
    let close = dialog.base().callable("close_pressed");
    let back_text = kit::t("@back").to_string();
    map_play_action_button(dialog, &back_text, "left", close);
    let target = dialog.to_gd();
    let state_for_play = state.clone();
    let callable = Callable::from_fn("map_play_start", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        map_play_start(&mut bound, &state_for_play);
    });
    let play_text = kit::t("@play").to_string();
    map_play_action_button(dialog, &play_text, "play", callable);
}

/// A 210x64 `Tex.button` action button with an icon (`BaseDialog` buttons row).
fn map_play_action_button(
    dialog: &mut MindDialog,
    text_value: &str,
    icon_name: &str,
    listener: Callable,
) {
    // code-instantiated: the button row is rebuilt per shown map.
    let mut button = factory::button("");
    button.set_custom_minimum_size(MAP_PLAY_ACTION_BUTTON);
    dialog.apply_pane_button(&mut button, OUTLINE_GRAY, 4.0);
    let glyph: Gd<Control> = glyph_control(icon_name, 24.0, Color::WHITE).upcast();
    dialog.build_button_content(&mut button, Some(glyph), text_value);
    if listener.is_valid() {
        button.connect("pressed", &listener);
    }
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&button);
    }
}

/// `Gamemode.valid(map)` per visible mode (`MindPreview.map_modes`).
fn map_play_mode_validity(dialog: &mut MindDialog, map_name: &str) -> VarDictionary {
    let _ = dialog;
    let Some(mut preview) = preview_node() else {
        return VarDictionary::new();
    };
    preview.bind_mut().map_modes(gs(map_name))
}

/// `map.applyRules(mode)` serialized (`MindPreview.map_rules_json`).
fn map_play_mode_rules(dialog: &mut MindDialog, map_name: &str, mode: &str) -> String {
    let _ = dialog;
    let Some(mut preview) = preview_node() else {
        return String::new();
    };
    preview
        .bind_mut()
        .map_rules_json(gs(map_name), gs(mode))
        .to_string()
}

/// `mode.toString()` + the selected index, refreshing the rules.
fn map_play_select_mode(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>, mode: &str) {
    let (buttons, modes, map_name) = {
        let st = state.borrow();
        (
            st.mode_buttons.clone(),
            st.button_modes.clone(),
            st.map_name.clone(),
        )
    };
    for (index, button) in buttons.iter().enumerate() {
        let selected = modes
            .get(index)
            .map(|candidate| candidate == mode)
            .unwrap_or(false);
        set_toggle(button, selected);
    }
    // `rules = map.applyRules(mode)` upstream.
    let rules = map_play_mode_rules(dialog, &map_name, mode);
    let mut st = state.borrow_mut();
    st.mode = mode.to_string();
    st.rules_json = rules;
}

/// `displayGameModeHelp` (`MapPlayDialog.java:91-112`): every visible mode as
/// `[accent]<name>:[] [lightgray]<description>`, width 400, `@ok` 110x50.
fn map_play_show_mode_help(dialog: &mut MindDialog) {
    let _ = dialog;
    let mut lines: Vec<String> = Vec::new();
    for mode in MAP_PLAY_MODES {
        lines.push(format!(
            "[accent]{}:[] [lightgray]{}",
            kit::t(&format!("@mode.{mode}.name")),
            kit::t(&format!("@mode.{mode}.description"))
        ));
    }
    let body = lines.join("\n");
    let _ = ui_call(
        "show_text",
        &[
            kit::t("@mode.help.title").to_variant(),
            kit::markup_text(&body).to_variant(),
        ],
    );
}

/// `CustomRulesDialog(true).show(rules, () -> rules = map.applyRules(mode))`:
/// the dialog edits the working rules; `rules_edited` replaces them.
fn map_play_open_rules(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>) {
    let (rules_json, mode, map_name) = {
        let st = state.borrow();
        (st.rules_json.clone(), st.mode.clone(), st.map_name.clone())
    };
    let current = parse_object(&rules_json);
    let reset = parse_object(&map_play_mode_rules(dialog, &map_name, &mode));
    let mut context = VarDictionary::new();
    context.set("rules", &current.to_variant());
    context.set("reset", &reset.to_variant());
    context.set("show_rule_edit", &false.to_variant());
    open_dialog("custom_rules", &context);
    let Some(rules_dialog) = dialog_node("custom_rules") else {
        return;
    };
    if !rules_dialog.has_signal("rules_edited") {
        return;
    }
    let state_for_rules = state.clone();
    let callable = Callable::from_fn("map_play_rules_edited", move |args: &[&Variant]| {
        let json = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        state_for_rules.borrow_mut().rules_json = json;
    });
    connect_once(
        dialog,
        "map_play_rules",
        &rules_dialog,
        "rules_edited",
        &callable,
    );
}

/// `Core.bundle.format("level.highscore", map.getHighScore())`.
fn map_play_high_score(map_name: &str) -> String {
    let default = kit::t("@level.highscore").to_string();
    let Some(preview) = preview_node() else {
        return default;
    };
    let key = preview.bind().map_high_score_key(gs(map_name));
    if key.is_empty() {
        return default;
    }
    let score = setting_get_int(&key.to_string(), 0);
    kit::markup_text(&default.replace("{0}", &score.to_string())).to_string()
}

/// `control.playMap(map, rules, playtesting)` + `ui.custom.hide()`.
fn map_play_start(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>) {
    let (map_name, mode, playtest, rules_json) = {
        let st = state.borrow();
        (
            st.map_name.clone(),
            st.mode.clone(),
            st.playtest,
            st.rules_json.clone(),
        )
    };
    let mut started = false;
    if !map_name.is_empty()
        && let Some(mut campaign) = dialog.campaign_node()
        && campaign.has_method("start_map_mode")
    {
        // `start_map_mode` loads the world, applies the gamemode + custom rules
        // and starts the session in one call (`Control.playMap`).
        started = campaign
            .call(
                "start_map_mode",
                &[
                    gs(map_name).to_variant(),
                    gs(mode).to_variant(),
                    playtest.to_variant(),
                    gs(rules_json).to_variant(),
                ],
            )
            .try_to::<bool>()
            .unwrap_or(false);
    }
    map_play_close_stack(dialog);
    if !started && playtest {
        // `Control.playMap`: "booted out of map, resume editing"
        // (`Control.java:422-428`): the editor still holds the map the playtest
        // just saved, so re-show it over the error.
        if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
            && shell.has_method("show_dialog")
        {
            shell.call("show_dialog", &[]);
        }
        return;
    }
    if started {
        // `MapEditorDialog.playtest`'s `playListener = this::hide`: a playtest
        // returns to the editor, so hide the shell while the map plays.
        if playtest
            && let Some(mut editor) = kit::node("/root/Spine/Ui/EditorDialog")
            && editor.has_method("hide_dialog")
        {
            editor.call("hide_dialog", &[]);
        }
        if let Some(mut root) = ui_root()
            && root.has_method("set_menu_visible")
        {
            root.call("set_menu_visible", &[false.to_variant()]);
        }
    }
}

fn map_play_close_stack(dialog: &mut MindDialog) {
    if kit::ui().is_none() {
        dialog.hide_dialog();
        return;
    }
    close_dialog("map_play");
    close_dialog("custom");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millis_use_h_mm_ss_like_strings_format_millis() {
        assert_eq!(format_millis(0), "0:00:00");
        assert_eq!(format_millis(61_000), "0:01:01");
        assert_eq!(format_millis(3_661_000), "1:01:01");
        assert_eq!(format_millis(-1_000), "-0:00:01");
    }

    #[test]
    fn step_sizes_match_loadout_dialog() {
        assert_eq!(loadout_step_size(0), 100);
        assert_eq!(loadout_step_size(999), 100);
        assert_eq!(loadout_step_size(1000), 200);
        assert_eq!(loadout_step_size(2000), 500);
        assert_eq!(loadout_step_size(5000), 1000);
    }

    #[test]
    fn symbol_strip_matches_ignore_symbols_class() {
        assert_eq!(schematics_strip_symbols("Copper Wall!"), "Copper Wall");
        assert_eq!(schematics_strip_symbols("a-b_c"), "abc");
        assert_eq!(schematics_strip_symbols("keep spaces"), "keep spaces");
    }

    #[test]
    fn tag_markup_is_split_into_plain_text_and_color() {
        assert_eq!(
            schematics_tag_plain("[magenta]advanced-logic"),
            "advanced-logic"
        );
        assert_eq!(schematics_tag_plain("plain"), "plain");
        assert_eq!(
            schematics_tag_color("[#ff00ff]x"),
            Some(Color::from_rgba(1.0, 0.0, 1.0, 1.0))
        );
        assert_eq!(schematics_tag_color("plain"), None);
    }

    #[test]
    fn color_tags_strip_like_strings_strip_colors() {
        assert_eq!(strip_color_tags("[scarlet]Wall[white]"), "wall");
        assert_eq!(strip_color_tags("[#ff0000]Copper"), "copper");
        assert_eq!(strip_color_tags("[not a color]"), "[not a color]");
    }

    #[test]
    fn valid_int_matches_gdscript_is_valid_int() {
        assert!(is_valid_int("42"));
        assert!(is_valid_int("-7"));
        assert!(is_valid_int(" 12 "));
        assert!(!is_valid_int(""));
        assert!(!is_valid_int("4.2"));
        assert!(!is_valid_int("abc"));
    }

    #[test]
    fn icon_markup_replaces_known_tokens() {
        let markup = kit::glyph("warn").to_string();
        assert_eq!(
            icon_glyph_markup("[icon name=\"warn\"][/icon]!"),
            format!("{markup}!")
        );
        assert_eq!(
            icon_glyph_markup("[icon name=\"nope\"][/icon]"),
            "[icon name=\"nope\"][/icon]"
        );
    }

    #[test]
    fn map_sort_keys_follow_map_priority() {
        let a = MapSortKey {
            row: VarDictionary::new(),
            index: 1,
            name: String::from("alpha"),
            custom: true,
            modified: 10,
        };
        let b = MapSortKey {
            row: VarDictionary::new(),
            index: 0,
            name: String::from("beta"),
            custom: false,
            modified: 20,
        };
        assert_eq!(maplist_compare("name", &a, &b), Ordering::Less);
        assert_eq!(maplist_compare("builtin", &a, &b), Ordering::Greater);
        // `recent` prefers the newer (larger) `lastModified`.
        assert_eq!(maplist_compare("recent", &a, &b), Ordering::Greater);
        // Equal-key rows fall back to the registry order.
        assert_eq!(maplist_compare("name", &a, &a), Ordering::Equal);
    }

    #[test]
    fn dispatch_covers_the_campaign_dialogs() {
        // A registry contract: every batch id resolves to a controller.
        for id in [
            "research",
            "schematics",
            "loadout",
            "launch_loadout",
            "sector_stats",
            "custom",
            "map_play",
        ] {
            assert!(dispatch(id).is_some(), "missing controller for `{id}`");
        }
        assert!(dispatch("nope").is_none());
    }
}

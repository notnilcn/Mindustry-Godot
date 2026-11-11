// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers for the campaign dialogs. Filled by S03 (docs/PLANS).
//! Upstream: none (port-only; shared helpers of the old batch_campaign2)

use crate::MindPreview;
use crate::gdutil::variant::{to_i64_typed, to_text_godot};
use crate::ui::dialog::MindDialog;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, styles};
use godot::builtin::{Side, VarArray, VarDictionary, VariantType};
use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::{
    Button, Control, Font, FontVariation, Json, Label, PanelContainer, RichTextLabel,
    StyleBoxEmpty, StyleBoxFlat, Texture2D, TextureRect,
};
use godot::prelude::*;
use mind_core::ui::campaign::{
    CampaignViews, ResearchTreeView, SchematicView, SectorStatsView, SectorView,
};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// `Pal.gray` / `Tex.pane` outline (#454545) used by the pane buttons.
pub(in crate::ui::dialogs) const OUTLINE_GRAY: Color = Color::from_rgba(0.27, 0.27, 0.27, 1.0);
/// `Pal.lightGray` sampled from the reference frames (sector stats cells).
pub(in crate::ui::dialogs) const LIGHT_GRAY: Color = Color::from_rgba(0.62, 0.62, 0.62, 1.0);
/// `Color(0.75, 0.75, 0.75)` (research requirement text).
pub(in crate::ui::dialogs) const TEXT_GRAY: Color = Color::from_rgba(0.75, 0.75, 0.75, 1.0);
/// `Pal.remove` (`Color(1.0, 0.204, 0.11)`).
pub(in crate::ui::dialogs) const PAL_REMOVE: Color = Color::from_rgba(1.0, 0.204, 0.11, 1.0);
/// `Pal.gray` (#454545) drawable colour.
pub(in crate::ui::dialogs) const PAL_GRAY: Color = Color::from_rgba(0.271, 0.271, 0.271, 1.0);
/// `Pal.darkestGray` (`Styles.grayPanel`, the `grayt` up state).
pub(in crate::ui::dialogs) const PAL_DARKEST_GRAY: Color = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
/// The live map registry (`editor/preview.rs`).
pub(in crate::ui::dialogs) const PREVIEW_PATH: &str = "/root/Spine/MindPreview";
/// The `UiRoot` shell (`/root/Spine/Ui/UiRoot`).
pub(in crate::ui::dialogs) const UI_ROOT_PATH: &str = "/root/Spine/Ui/UiRoot";

/// A rich label's engine handle (text/color/alignment live on `RichTextLabel`).
pub(in crate::ui::dialogs) use crate::ui::dialogs::common::facade::rich;

// ---------------------------------------------------------------------------
// Typed campaign read models (T02)
// ---------------------------------------------------------------------------

/// The live typed campaign projection for `planet` (empty selects the played
/// planet, else the default starting planet). `None` when the campaign is not
/// booted, which keeps the dialogs' empty-default behaviour.
///
/// The `bind()` guard is a single statement so it is dropped before the caller
/// touches the UI (a `bind()` here would panic on re-entrancy).
pub(in crate::ui::dialogs) fn campaign_views_of(
    dialog: &MindDialog,
    planet: &str,
) -> Option<CampaignViews> {
    let campaign = dialog.campaign()?;
    campaign.bind().campaign_views(planet)
}

/// [`campaign_views_of`] for the played planet (`campaign_views()`).
pub(in crate::ui::dialogs) fn campaign_views_live(dialog: &MindDialog) -> Option<CampaignViews> {
    campaign_views_of(dialog, "")
}

/// The typed schematic library (`CampaignViews.schematics`), empty when the
/// campaign facade is absent.
pub(in crate::ui::dialogs) fn schematic_library(dialog: &MindDialog) -> Vec<SchematicView> {
    campaign_views_of(dialog, "")
        .map(|views| views.schematics)
        .unwrap_or_default()
}

/// The research dialog's typed tree payload (`campaign_research_tree`): the
/// `research_tree(root)` producer, with `root` empty for the played sector's
/// tree. `None` when the campaign facade is absent or the tree is missing.
pub(in crate::ui::dialogs) fn campaign_research_tree(
    dialog: &MindDialog,
    root: &str,
) -> Option<ResearchTreeView> {
    let campaign = dialog.campaign()?;
    campaign.bind().research_tree(root)
}

/// The `SectorView` for `planet`/`sector_id` in a projection (`Sector.isBeingPlayed`
/// lookups, the stats dialog and the planet page).
pub(in crate::ui::dialogs) fn campaign_sector_of(
    views: &CampaignViews,
    planet: &str,
    sector_id: i64,
) -> Option<SectorView> {
    views
        .sectors
        .iter()
        .find(|sector| i64::from(sector.id) == sector_id && sector.planet == planet)
        .cloned()
}

/// The view a missing sector/section reads as: every string empty, every flag
/// false, every number zero. `threat_band` stays `low` like the row readers'
/// fallback (the old `row_opt_str(...).unwrap_or("low")`).
pub(in crate::ui) fn empty_sector_stats() -> SectorStatsView {
    SectorStatsView {
        attempts: 0,
        wave: 0,
        production: Vec::new(),
        export: Vec::new(),
        import: Vec::new(),
        stored: Vec::new(),
        resources: Vec::new(),
        playtime_ms: 0,
        destination: None,
        import_sources: Vec::new(),
        lockdown: false,
        preset_description_key: None,
    }
}

/// A `SectorView` with every field at its empty default (see
/// [`empty_sector_stats`]). The planet page's synthesized start-sector row and
/// the stats dialog's missing-sector fallback use it.
pub(in crate::ui) fn empty_sector_view(id: u16, planet: &str, name: &str) -> SectorView {
    SectorView {
        id,
        planet: planet.to_owned(),
        preset: None,
        name: name.to_owned(),
        named: false,
        selectable: false,
        accelerator_selectable: false,
        require_unlock: false,
        preset_unlocked: false,
        lock_icon: false,
        icon: None,
        content_icon: None,
        difficulty: 0.0,
        threat: 0.0,
        threat_band: "low",
        unlocked: false,
        locked: false,
        has_base: false,
        has_save: false,
        captured: false,
        attacked: false,
        frozen: false,
        shielded: false,
        has_enemy_base: false,
        is_last: false,
        capture_wave: 0,
        stats: empty_sector_stats(),
        launch_from: None,
        no_candidate: false,
        show_action: false,
        objectives_locked: false,
        objectives: Vec::new(),
        vulnerable: false,
        threat_color: String::new(),
        hover_lock: None,
        hover_allowed: false,
        being_played: false,
        shield_arc: None,
        attack_sources: Vec::new(),
        export_destination: None,
        any_exports: false,
    }
}

/// The `SchematicImage` widget's row shape (`width`/`height`/`tiles`) built from
/// the typed schematic view.
///
/// The widget is outside T02's owned paths and keeps its untyped
/// `#[func] setup(VarDictionary)` surface, so the dialog translates at that
/// boundary. Follow-up for C01: a typed `setup_view(&SchematicView)` on
/// `kit/widgets/schematic_image.rs` would drop this adapter.
pub(in crate::ui::dialogs) fn schematic_image_row(view: &SchematicView) -> VarDictionary {
    let mut row = VarDictionary::new();
    row.set("width", &i64::from(view.width).to_variant());
    row.set("height", &i64::from(view.height).to_variant());
    let mut tiles = VarArray::new();
    for tile in &view.tiles {
        let mut tile_row = VarDictionary::new();
        tile_row.set("block", &gs(tile.block.as_str()).to_variant());
        tile_row.set("x", &i64::from(tile.x).to_variant());
        tile_row.set("y", &i64::from(tile.y).to_variant());
        tile_row.set("size", &i64::from(tile.size).to_variant());
        tile_row.set("rotation", &i64::from(tile.rotation).to_variant());
        tile_row.set("rotate", &tile.rotate.to_variant());
        if !tile.lasers.is_empty() {
            let mut lasers = VarArray::new();
            for index in &tile.lasers {
                lasers.push(&(*index as i64).to_variant());
            }
            tile_row.set("lasers", &lasers.to_variant());
        }
        tiles.push(&tile_row.to_variant());
    }
    row.set("tiles", &tiles.to_variant());
    row
}

/// Reads a JSON string field (`""` when missing or null).
pub(in crate::ui::dialogs) fn row_str(row: &VarDictionary, key: &str) -> String {
    row_opt_str(row, key).unwrap_or_default()
}

/// Reads a JSON string field (`None` when missing or null).
pub(in crate::ui::dialogs) fn row_opt_str(row: &VarDictionary, key: &str) -> Option<String> {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .map(|value| to_text_godot(&value))
}

/// Reads a JSON boolean field (`false` when missing).
pub(in crate::ui::dialogs) fn row_bool(row: &VarDictionary, key: &str) -> bool {
    row_bool_or(row, key, false)
}

/// Reads a JSON boolean field with a default.
pub(in crate::ui::dialogs) fn row_bool_or(row: &VarDictionary, key: &str, default: bool) -> bool {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(default)
}

/// Reads a JSON integer field (`0` when missing; floats truncate).
pub(in crate::ui::dialogs) fn row_i64(row: &VarDictionary, key: &str) -> i64 {
    row_i64_or(row, key, 0)
}

/// Reads a JSON integer field with a default.
pub(in crate::ui::dialogs) fn row_i64_or(row: &VarDictionary, key: &str, default: i64) -> i64 {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .map(|value| to_i64_typed(&value))
        .unwrap_or(default)
}

/// `JSON.stringify(object)`.
pub(in crate::ui::dialogs) fn json_stringify(object: &VarDictionary) -> String {
    Json::stringify(&object.to_variant()).to_string()
}

/// `JSON.stringify(array)`.
pub(in crate::ui::dialogs) fn json_stringify_array(array: &VarArray) -> String {
    Json::stringify(&array.to_variant()).to_string()
}

/// `GString` from arbitrary text (gdext's `From` impl only accepts `&str`).
pub(in crate::ui::dialogs) fn gs(text: impl AsRef<str>) -> GString {
    GString::from(text.as_ref())
}

/// Calls a `MindUi` method when both the autoload and the method exist.
pub(in crate::ui::dialogs) use crate::ui::dialogs::common::facade::ui_call;
/// Opens a registered dialog through `MindUi` with an already-serialized JSON
/// context (`serde_json` output, so typed rows keep their exact field names).
pub(in crate::ui::dialogs) fn open_dialog_json(name: &str, context_json: &str) {
    let _ = ui_call(
        "open_dialog",
        &[gs(name).to_variant(), gs(context_json).to_variant()],
    );
}

/// Calls a `MindUi` method returning a bool (`false` when absent).
pub(in crate::ui::dialogs) fn ui_bool(method: &str, args: &[Variant]) -> bool {
    ui_call(method, args)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(false)
}

/// `Core.settings.getString(key, default)` through the `MindUi` store.
pub(in crate::ui::dialogs) fn setting_text(key: &str, default: &str) -> String {
    ui_call(
        "settings_get_text",
        &[gs(key).to_variant(), gs(default).to_variant()],
    )
    .and_then(|value| value.try_to::<GString>().ok())
    .map(|value| value.to_string())
    .unwrap_or_else(|| default.to_string())
}

/// `Core.settings.getInt(key, default)` through the `MindUi` store.
pub(in crate::ui::dialogs) fn setting_get_int(key: &str, default: i64) -> i64 {
    ui_call(
        "settings_get_int",
        &[gs(key).to_variant(), default.to_variant()],
    )
    .and_then(|value| value.try_to::<i64>().ok())
    .unwrap_or(default)
}

/// `Core.settings.put(key, value)` (bool) through the `MindUi` store.
pub(in crate::ui::dialogs) fn setting_put_flag(key: &str, value: bool) {
    let _ = ui_call(
        "settings_put_flag",
        &[gs(key).to_variant(), value.to_variant()],
    );
}

/// `Core.settings.put(key, value)` (string) through the `MindUi` store.
pub(in crate::ui::dialogs) fn setting_put_text(key: &str, value: &str) {
    let _ = ui_call(
        "settings_put_text",
        &[gs(key).to_variant(), gs(value).to_variant()],
    );
}

/// Emit-only UI sound (`MindUi.play_ui_sound`).
pub(in crate::ui::dialogs) fn play_ui_sound(name: &str) {
    let _ = ui_call("play_ui_sound", &[gs(name).to_variant()]);
}

/// Closes a registered dialog through `MindUi`.
pub(in crate::ui::dialogs) fn close_dialog(name: &str) {
    let _ = ui_call("close_dialog", &[gs(name).to_variant()]);
}

/// Opens a registered dialog through `MindUi` with a JSON context.
pub(in crate::ui::dialogs) fn open_dialog(name: &str, context: &VarDictionary) {
    let _ = ui_call(
        "open_dialog",
        &[
            gs(name).to_variant(),
            gs(json_stringify(context)).to_variant(),
        ],
    );
}

/// Live `universe.getLaunchResources()` as `(item name, amount)` rows.
///
/// Reads the typed `MindCampaign.launch_resource_stacks` producer (T01); empty
/// when the campaign facade is absent.
pub(in crate::ui::dialogs) fn campaign_launch_stacks(dialog: &MindDialog) -> Vec<(String, i32)> {
    let Some(mut campaign) = dialog.campaign() else {
        return Vec::new();
    };
    campaign.bind_mut().launch_resource_stacks()
}

/// The live map registry node (`None` outside the game scene).
pub(in crate::ui::dialogs) fn preview_node() -> Option<Gd<MindPreview>> {
    kit::node(PREVIEW_PATH)?.try_cast::<MindPreview>().ok()
}

/// The `UiRoot` shell (`/root/Spine/Ui/UiRoot`).
pub(in crate::ui::dialogs) fn ui_root() -> Option<Gd<Node>> {
    kit::node(UI_ROOT_PATH)
}

/// One manifest dialog node through `UiRoot.dialog(name)`.
pub(in crate::ui::dialogs) fn dialog_node(name: &str) -> Option<Gd<Node>> {
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
pub(in crate::ui::dialogs) fn in_game() -> bool {
    let Some(mut host) = kit::sim_host() else {
        return false;
    };
    if !host.has_method("get_state") {
        return false;
    }
    host.call("get_state", &[]).to::<GString>() != "menu"
}

/// `Sector.isBeingPlayed`: the campaign's live sector is `planet_name`/`sector_id`.
///
/// Reads the typed `MindCampaign.sector_state()` view (`-1` = no played sector,
/// the old missing-`sector`-key default) and compares the campaign flag plus
/// the planet/sector pair (`PlanetDialog` showStats / `playSelected` paths).
pub(in crate::ui::dialogs) fn campaign_sector_is_playing(
    dialog: &MindDialog,
    planet_name: &str,
    sector_id: i64,
) -> bool {
    let Some(campaign) = dialog.campaign() else {
        return false;
    };
    let state = campaign.bind().sector_state();
    state.campaign && state.planet == planet_name && state.sector == sector_id
}

/// `control.input.useSchematic(s)` through `MindInput` (G08 T6b).
pub(in crate::ui::dialogs) fn use_schematic(index: i64) -> bool {
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
pub(in crate::ui::dialogs) fn spacer(expand: bool, width: f32, height: f32) -> Gd<Control> {
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
pub(in crate::ui::dialogs) fn glyph_control(name: &str, extent: f32, color: Color) -> Gd<Label> {
    factory::glyph_label(name, extent as i32, color)
}

/// `UnlockableContent.uiIcon` region chain (`item-<name>-ui` first).
pub(in crate::ui::dialogs) fn content_texture(content_name: &str) -> Option<Gd<Texture2D>> {
    for prefix in ["item", "liquid", "block", "unit", "status"] {
        if let Some(texture) = kit::icon_texture(&format!("{prefix}-{content_name}-ui")) {
            return Some(texture.upcast());
        }
    }
    None
}

/// A fitted `TextureRect` for an atlas region (`res.image(uiIcon).size(n)`).
pub(in crate::ui::dialogs) fn icon_rect(texture: &Gd<Texture2D>, extent: f32) -> Gd<TextureRect> {
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
pub(in crate::ui::dialogs) fn panel_with_box(style: Gd<StyleBoxFlat>) -> Gd<PanelContainer> {
    // code-instantiated: themed frame built per data row.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &style);
    panel
}

/// Connects `callable` to `signal` once per dialog (`is_connected` cannot match
/// a `Callable::from_fn`, so the dialog node carries the latch).
pub(in crate::ui::dialogs) fn connect_once(
    dialog: &mut MindDialog,
    key: &str,
    target: &Gd<Node>,
    signal: &str,
    callable: &Callable,
) {
    connect_once_prefixed(dialog, "_campaign2_", key, target, signal, callable);
}

/// [`connect_once`] with the dialog family's original meta-key prefix.
///
/// The prefix is part of each batch's UI ABI (`_campaign2_` here, `_batch_c_`
/// for the planet page), so it stays a parameter instead of being unified.
pub(in crate::ui::dialogs) fn connect_once_prefixed(
    dialog: &mut MindDialog,
    meta_prefix: &str,
    key: &str,
    target: &Gd<Node>,
    signal: &str,
    callable: &Callable,
) {
    let meta = format!("{meta_prefix}{key}");
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
pub(crate) use crate::ui::dialogs::common::defer::Deferred;

/// Installs the per-frame hook once (deferred queue drain + `tick`).
///
/// Keeps the campaign batch's original meta key (`_campaign2_frame_<key>`) and
/// callable name (`campaign2_frame`). The editor/canvas dialogs install the same
/// pair through [`crate::ui::dialogs::common::defer::install_frame_tick`].
pub(crate) fn install_frame_tick(
    dialog: &mut MindDialog,
    key: &str,
    deferred: Deferred,
    tick: impl FnMut() + 'static,
) {
    crate::ui::dialogs::common::defer::install_frame_tick(
        dialog,
        "_campaign2_frame_",
        "campaign2_frame",
        key,
        deferred,
        tick,
    );
}

/// `_t("@key")` with a fallback when the bundle has no entry.
pub(in crate::ui::dialogs) fn bundle_or(key: &str, fallback: &str) -> String {
    let value = kit::t(&format!("@{key}")).to_string();
    if value.is_empty() || value == key {
        fallback.to_string()
    } else {
        value
    }
}

/// A `minimap`-style solid `StyleBoxFlat` (`_solid`).
pub(in crate::ui::dialogs) fn solid_box(color: Color) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(color);
    box_
}

/// A map preview with the Arc `BorderImage` stroke (`MindBorderImage`).
pub(in crate::ui::dialogs) fn border_preview(
    texture: &Gd<Texture2D>,
    extent: Vector2,
    thickness: f32,
) -> Gd<Control> {
    use crate::ui::kit::widgets::border_image::MindBorderImage;
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
pub(in crate::ui::dialogs) fn preview_texture(map_name: &str) -> Option<Gd<Texture2D>> {
    let mut preview = preview_node()?;
    preview.bind_mut().texture_for_name(gs(map_name))
}

/// `MindIcons.glyph` for a label's markup (`UI.formatIcons` icon tokens).
pub(in crate::ui::dialogs) fn icon_glyph_markup(markup: &str) -> String {
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
pub(in crate::ui::dialogs) fn apply_icon_fallback(label: &Gd<RichTextLabel>) {
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
pub(in crate::ui::dialogs) fn format_millis(millis: i64) -> String {
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
pub(in crate::ui::dialogs) fn apply_grayt(button: &mut Gd<Button>) {
    button.add_theme_stylebox_override("normal", &solid_box(PAL_DARKEST_GRAY));
    button.add_theme_stylebox_override("hover", &solid_box(PAL_GRAY));
    button.add_theme_stylebox_override("pressed", &solid_box(PAL_GRAY));
    button.add_theme_stylebox_override("hover_pressed", &solid_box(PAL_GRAY));
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
}

/// `Styles.flatToggleMenut` (`Styles.java:210-219`).
pub(in crate::ui::dialogs) fn apply_flat_toggle_menu(button: &mut Gd<Button>) {
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
pub(in crate::ui::dialogs) fn set_toggle(button: &Gd<Button>, pressed: bool) {
    let mut button = button.clone();
    button.set_pressed_no_signal(pressed);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use mind_core::util::strip_colors;

    #[test]
    fn millis_use_h_mm_ss_like_strings_format_millis() {
        assert_eq!(format_millis(0), "0:00:00");
        assert_eq!(format_millis(61_000), "0:01:01");
        assert_eq!(format_millis(3_661_000), "1:01:01");
        assert_eq!(format_millis(-1_000), "-0:00:01");
    }

    #[test]
    fn color_tags_strip_like_strings_strip_colors() {
        // `Map.plainName()` + `toLowerCase()`: the shared Java-faithful strip
        // then the search's case fold.
        assert_eq!(strip_colors("[scarlet]Wall[white]").to_lowercase(), "wall");
        assert_eq!(strip_colors("[#ff0000]Copper").to_lowercase(), "copper");
        assert_eq!(
            strip_colors("[not a color]").to_lowercase(),
            "[not a color]"
        );
        assert_eq!(strip_colors("[NotATag]").to_lowercase(), "[notatag]");
    }

    #[test]
    fn icon_markup_replaces_known_tokens() {
        // Godot-FFI-backed lookups/types need the engine; plain `cargo test`
        // runs without it (the headless oracles cover the engine path).
        if !godot::sys::is_godot_initialized() {
            return;
        }
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
    fn dispatch_covers_the_campaign_dialogs() {
        // Godot-FFI-backed lookups/types need the engine; plain `cargo test`
        // runs without it (the headless oracles cover the engine path).
        if !godot::sys::is_godot_initialized() {
            return;
        }
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
            assert!(
                crate::ui::dialogs::controller_for(id).is_some(),
                "missing controller for `{id}`"
            );
        }
        assert!(crate::ui::dialogs::controller_for("nope").is_none());
    }
}

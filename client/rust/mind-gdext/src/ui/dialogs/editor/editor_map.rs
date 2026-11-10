// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The per-map preview page of the map browser.
//! Upstream: ui/dialogs/EditorMapsDialog.java (showMap)

use godot::classes::Button;
use godot::classes::Control;
use godot::classes::MarginContainer;
use godot::classes::PanelContainer;
use godot::classes::ScrollContainer;
use godot::classes::StyleBoxFlat;
use godot::classes::Texture2D;
use godot::classes::VBoxContainer;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::MouseFilter;
use godot::classes::control::SizeFlags;
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::StretchMode;
use godot::obj::NewAlloc;
use godot::obj::NewGd;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::border_image::MindBorderImage;
use crate::ui::kit::widgets::factory;

use super::util_d::*;

// ---------------------------------------------------------------------------
// EditorMapDialog
// ---------------------------------------------------------------------------

/// `showMap`: `Core.graphics.isPortrait() ? 160f : 300f` (the port hardcodes
/// the desktop size in the GDScript).
const MAP_PREVIEW_SIZE: f32 = 300.0;
/// `mind_core::maps::UNKNOWN`: the `Map.tag` fallback that `maps_list` puts in
/// a row whose tag is missing/blank.
const MAP_MISSING_TAG: &str = "unknown";
/// `t.margin(6)` around the info rows (`EditorMapsDialog.java:113`).
const MAP_INFO_MARGIN: i32 = 6;
/// `t.defaults().padTop(10)` between caption rows.
const MAP_INFO_ROW_PAD: f32 = 10.0;
/// `.padTop(2)` above each value.
const MAP_INFO_VALUE_PAD: f32 = 2.0;
/// Wrapped-line pitch: Arc's `Fonts.def` lines are 20 px apart, `MindLabel`'s
/// 19 px (java/17b-capital.png: description lines at y=263, 283, 303, ...).
const MAP_ARC_LINE_EXTRA: i32 = 1;
/// An Arc label's box is one more pixel than its line pitch.
const MAP_ARC_LABEL_TAIL: f32 = 1.0;
/// Arc's cap offset inside a label box minus `MindLabel`'s (5 px vs 3 px).
const MAP_ARC_CAP_LEAD: f32 = 2.0;
/// `table.button(...).fillX().height(54f)` (`EditorMapsDialog.java:149,163`).
const MAP_BUTTON_HEIGHT: f32 = 54.0;

/// `@editor_map` (`client/ui/dialogs/editor_map_dialog.gd`,
/// `EditorMapsDialog.showMap` `:100-163`): the map preview beside the
/// name/author/description column, then `@editor.openin` and `@delete`
/// (disabled for built-in maps).
#[derive(Default)]
pub struct EditorMapDialog {
    name: String,
}

impl DialogController for EditorMapDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.mapinfo");
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
        connect_confirm_prompt(dialog, "editor_map", |target, confirmed| {
            editor_map_on_confirm(target, confirmed);
        });
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        self.name = dialog.context_string("map").to_string();
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        if self.name.is_empty() {
            self.name = dialog.context_string("map").to_string();
        }
        self.build(dialog);
    }
}

impl EditorMapDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        // Java builds a fresh dialog per `showMap` (`EditorMapsDialog.java:101`);
        // this node is reused, so drop the previous open's body first.
        dialog.clear_content();
        let mut root = dialog.content_table();
        let row = editor_map_row(&self.name);
        let name = self.name.clone();
        {
            let mut node = dialog.to_gd();
            node.set_meta(
                EDITOR_MAP_NAME_META,
                &GString::from(name.as_str()).to_variant(),
            );
        }

        // `EditorMapsDialog.showMap` (`:100-163`): a 2x2 grid — the preview and
        // the `Styles.black` info pane are each `mapsize` (300) squares, and the
        // two action buttons `fillX()` their own column.
        if let Some(texture) = map_preview_texture(&name) {
            // code-instantiated: the map preview texture comes from the registry.
            let mut image = MindBorderImage::new_alloc();
            image.set_texture(&texture);
            // `new BorderImage(map.safeTexture())`: the default `thickness = 4f`
            // (`BorderImage.java:16`, `EditorMapsDialog.java:108`) and the
            // default nearest filter (`map.safeTexture()`).
            image.bind_mut().thickness = 4.0;
            image.bind_mut().force_nearest = true;
            image.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            root.bind_mut()
                .add(&image)
                .width(MAP_PREVIEW_SIZE)
                .height(MAP_PREVIEW_SIZE);
        } else {
            // Keep the grid's first column even without a preview texture.
            // code-instantiated: layout placeholder for the missing preview.
            let placeholder = Control::new_alloc();
            root.bind_mut()
                .add(&placeholder)
                .width(MAP_PREVIEW_SIZE)
                .height(MAP_PREVIEW_SIZE);
        }

        // `table.table(Styles.black, desc -> { desc.top(); ...
        // desc.add(new ScrollPane(t)).grow(); ... }).height(mapsize)
        // .width(mapsize)` (`EditorMapsDialog.java:110-134`): the rows scroll
        // inside the fixed 300x300 pane.
        // code-instantiated: the info pane's rows come from the map's tags.
        let mut info = PanelContainer::new_alloc();
        let mut black = StyleBoxFlat::new_gd();
        black.set_bg_color(Color::BLACK);
        info.add_theme_stylebox_override("panel", &black);
        info.set_clip_contents(true);
        // code-instantiated: `new ScrollPane(t)` (default pane style, no X scroll).
        let mut pane = ScrollContainer::new_alloc();
        pane.set_horizontal_scroll_mode(ScrollMode::DISABLED);
        pane.set_h_size_flags(SizeFlags::EXPAND_FILL);
        pane.set_v_size_flags(SizeFlags::EXPAND_FILL);
        info.add_child(&pane);
        // code-instantiated: `t.margin(6)`.
        let mut margin = MarginContainer::new_alloc();
        margin.set_h_size_flags(SizeFlags::EXPAND_FILL);
        for side in ["left", "top", "bottom"] {
            margin.add_theme_constant_override(
                &StringName::from(&format!("margin_{side}")),
                MAP_INFO_MARGIN,
            );
        }
        // Hand the right margin to the text box: Godot's line breaker counts the
        // space after a word, Arc's `GlyphLayout` does not.
        margin.add_theme_constant_override("margin_right", 0);
        pane.add_child(&margin);
        let mut info_layout = VBoxContainer::new_alloc();
        info_layout.add_theme_constant_override("separation", 0);
        info_layout.set_alignment(AlignmentMode::BEGIN);
        info_layout.set_h_size_flags(SizeFlags::EXPAND_FILL);
        margin.add_child(&info_layout);
        // Every Arc label draws its cap 2 px lower in its box than a `MindLabel`
        // (java/17b-capital.png: pane top 122, "Map Name:" cap 133), so the whole
        // column starts `MAP_ARC_CAP_LEAD` lower.
        add_info_gap(&mut info_layout, MAP_ARC_CAP_LEAD);
        add_info_row(&mut info_layout, "@editor.mapname", &name, 0.0);
        let mut author = map_tag(&row, "author");
        if author.is_empty() {
            // `showMap`: built-ins with no author tag show "Anuke"; custom maps
            // show `map.author()` = `Map.tag` -> `@unknown`.
            author = if dict_bool(&row, "custom", false) {
                kit::t("@unknown").to_string()
            } else {
                "Anuke".to_string()
            };
        }
        add_info_row(
            &mut info_layout,
            "@editor.author",
            &author,
            MAP_INFO_ROW_PAD,
        );
        // `if(!map.tags.get("description", "").isEmpty())`: no description row at
        // all for a map without one.
        let description = map_tag(&row, "description");
        if !description.is_empty() {
            add_info_row(
                &mut info_layout,
                "@editor.description",
                &description,
                MAP_INFO_ROW_PAD,
            );
        }
        root.bind_mut()
            .add(&info)
            .width(MAP_PREVIEW_SIZE)
            .height(MAP_PREVIEW_SIZE);
        root.bind_mut().row();

        // `table.button("@editor.openin", Icon.export, ...).fillX().height(54f)`.
        let open_path = dict_string(&row, "path");
        let mut open_button = factory::icon_button(
            "export",
            &kit::t("@editor.openin").to_string(),
            "defaultt",
            AlignmentMode::BEGIN,
        );
        fix_map_button_height(&open_button);
        let callable = {
            let target = dialog.to_gd();
            Callable::from_fn("editor_map_open", move |_args: &[&Variant]| {
                editor_map_open_in_editor(&target, &open_path);
            })
        };
        open_button.connect("pressed", &callable);
        root.bind_mut()
            .add(&open_button)
            .width(MAP_PREVIEW_SIZE)
            .height(MAP_BUTTON_HEIGHT);

        // `table.button("@delete", Icon.trash, ...).disabled(!map.custom)`.
        let mut delete_button = factory::icon_button(
            "trash",
            &kit::t("@delete").to_string(),
            "defaultt",
            AlignmentMode::BEGIN,
        );
        fix_map_button_height(&delete_button);
        delete_button.set_disabled(!dict_bool(&row, "custom", false));
        let callable = {
            let target = dialog.to_gd();
            Callable::from_fn("editor_map_delete", move |_args: &[&Variant]| {
                editor_map_confirm_delete(&target);
            })
        };
        delete_button.connect("pressed", &callable);
        root.bind_mut()
            .add(&delete_button)
            .width(MAP_PREVIEW_SIZE)
            .height(MAP_BUTTON_HEIGHT);
        root.bind_mut().row();
    }
}

/// `.height(54f)`: Arc's cell size wins over the icon row's pref height
/// (`MindWidgets.icon_button` grows the button to its 31 px glyph + 24 px
/// padding on `ready`; this handler runs after it).
fn fix_map_button_height(button: &Gd<Button>) {
    let mut button = button.clone();
    let mut min = button.get_custom_minimum_size();
    min.y = MAP_BUTTON_HEIGHT;
    button.set_custom_minimum_size(min);
    let button_for_ready = button.clone();
    let callable = Callable::from_fn("editor_map_button_height", move |_args: &[&Variant]| {
        let mut button = button_for_ready.clone();
        let mut min = button.get_custom_minimum_size();
        min.y = MAP_BUTTON_HEIGHT;
        button.set_custom_minimum_size(min);
    });
    button.connect("ready", &callable);
}

/// A row tag as the raw `map.tags.get(name, "")` (blank when missing).
fn map_tag(row: &VarDictionary, tag_name: &str) -> String {
    let value = dict_string(row, tag_name).trim().to_string();
    if value == MAP_MISSING_TAG {
        String::new()
    } else {
        value
    }
}

/// One caption + value pair: `t.add(key).padRight(10).color(Color.gray)` then
/// `t.add(value).growX().wrap().padTop(2)` (`EditorMapsDialog.java:121-133`).
fn add_info_row(parent: &mut Gd<VBoxContainer>, label_key: &str, value: &str, pad_top: f32) {
    add_info_gap(parent, pad_top);
    let label = factory::label(&kit::t(label_key).to_string());
    let mut label_rich = rich(&label);
    label_rich.add_theme_color_override("default_color", styles::ARC_GRAY);
    label_rich.add_theme_constant_override("line_separation", MAP_ARC_LINE_EXTRA);
    parent.add_child(&label);
    add_info_gap(parent, MAP_ARC_LABEL_TAIL + MAP_INFO_VALUE_PAD);
    let text = factory::label(value);
    let mut text_rich = rich(&text);
    text_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
    text_rich.add_theme_constant_override("line_separation", MAP_ARC_LINE_EXTRA);
    parent.add_child(&text);
    add_info_gap(parent, MAP_ARC_LABEL_TAIL);
}

/// A cell `padTop` between two runtime-built rows.
fn add_info_gap(parent: &mut Gd<VBoxContainer>, height: f32) {
    if height <= 0.0 {
        return;
    }
    // code-instantiated: a cell `padTop` between two runtime-built rows.
    let mut gap = Control::new_alloc();
    gap.set_custom_minimum_size(Vector2::new(0.0, height));
    gap.set_mouse_filter(MouseFilter::IGNORE);
    parent.add_child(&gap);
}

/// The registry row for a map name (`MindPreview.maps_list`).
fn editor_map_row(map_name: &str) -> VarDictionary {
    preview_maps_list()
        .into_iter()
        .find(|row| dict_string(row, "name") == map_name)
        .unwrap_or_default()
}

/// The live registry rows (`MindPreview.maps_list`), hidden entries dropped.
fn preview_maps_list() -> Vec<VarDictionary> {
    let Some(mut preview) = kit::node("/root/Spine/MindPreview") else {
        return Vec::new();
    };
    if !preview.has_method("maps_list") {
        return Vec::new();
    }
    preview
        .call("maps_list", &[])
        .try_to::<VarArray>()
        .unwrap_or_default()
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .filter(|row| !dict_string(row, "name").is_empty())
        .collect()
}

/// `map.safeTexture()` for a display name (`MindPreview.texture_for_name`).
fn map_preview_texture(map_name: &str) -> Option<Gd<Texture2D>> {
    let mut preview = kit::node("/root/Spine/MindPreview")?;
    if !preview.has_method("texture_for_name") {
        return None;
    }
    preview
        .call("texture_for_name", &[GString::from(map_name).to_variant()])
        .try_to::<Gd<Texture2D>>()
        .ok()
}

/// `showMap`'s `@editor.openin` handler (delegated by `editor_map`).
///
/// `Vars.ui.editor.beginEditMap(map.file); dialog.hide(); hide();`: the maps
/// dialog loads the file, hides itself and shows the editor shell. A missing
/// maps dialog is a no-op (the GDScript null check), so the per-map page just
/// closes.
fn editor_map_open_in_editor(target: &Gd<MindDialog>, path: &str) {
    if dialog_node("editor_maps").is_some() {
        editor_load_map_into_editor(path);
    }
    close_node(target);
}

/// `EditorMapsDialog._open_in_editor`: load the map and show the shell.
fn editor_load_map_into_editor(path: &str) {
    if path.is_empty() {
        notify_error_key("@editor.errorload");
        return;
    }
    let Some(mut editor) = kit::editor() else {
        notify_error_key("@editor.errorload");
        return;
    };
    let opened = editor
        .call("begin_edit_map", &[GString::from(path).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false);
    if !opened {
        notify_error_key("@editor.errorload");
        return;
    }
    close_named_dialog("editor_maps");
    if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
        && shell.has_method("show_dialog")
    {
        let _ = shell.call("show_dialog", &[]);
    }
}

/// `_confirm_delete`: `ui.showConfirm("@confirm", map.delete(map.name()), ...)`.
fn editor_map_confirm_delete(target: &Gd<MindDialog>) {
    let mut node = target.clone();
    node.set_meta(EDITOR_MAP_DELETE_META, &true.to_variant());
    let name = meta_string(&node, EDITOR_MAP_NAME_META);
    let text = fmt_markup("@map.delete", &[name.as_str()]);
    let _ = ui_call("show_confirm", &[text.to_variant()]);
}

/// `_on_confirm`: deletes the map, refreshes the previews and the map browser.
fn editor_map_on_confirm(target: &Gd<MindDialog>, confirmed: bool) {
    let mut node = target.clone();
    if !take_meta_bool(&mut node, EDITOR_MAP_DELETE_META) {
        return;
    }
    if !confirmed {
        return;
    }
    let name = meta_string(&node, EDITOR_MAP_NAME_META);
    let Some(mut editor) = kit::editor() else {
        notify_error_key("@editor.errorload");
        return;
    };
    let deleted = editor
        .call("delete_map", &[GString::from(name.as_str()).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false);
    if !deleted {
        notify_error_key("@editor.errorload");
        return;
    }
    if let Some(mut preview) = kit::node("/root/Spine/MindPreview")
        && preview.has_method("refresh")
    {
        let _ = preview.call("refresh", &[]);
    }
    editor_maps_request_rebuild();
    close_node(target);
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "editor_map" => Some(Box::new(EditorMapDialog::default())),
        _ => None,
    }
}

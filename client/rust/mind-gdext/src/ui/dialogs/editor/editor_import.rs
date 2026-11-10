// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map editor import dialog controller.
//! Upstream: editor/MapEditorDialog.java

use godot::classes::HBoxContainer;
use godot::classes::VBoxContainer;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::LayoutPreset;
use godot::classes::control::MouseFilter;
use godot::classes::control::SizeFlags;
use godot::classes::text_server::AutowrapMode;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindTable;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;

use super::util::*;

// ---------------------------------------------------------------------------
// EditorImportDialog
// ---------------------------------------------------------------------------

/// `@editor.import`: the three import routes — the in-game map list
/// (`map_load`), a `.msav` file and a PNG (`client/ui/dialogs/
/// editor_import_dialog.gd`, `MapEditorDialog.createDialog("@editor.import")`).
pub struct EditorImportDialog;

/// One `createDialog` entry is a 360x90 left-aligned button.
const IMPORT_BUTTON_SIZE: Vector2 = Vector2::new(360.0, 90.0);

impl DialogController for EditorImportDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.import");
        dialog.should_pause = false;
        dialog.add_default_close_button();
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.build(dialog);
    }
}

impl EditorImportDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        // A reused dialog node: drop the previous open's body before rebuilding.
        dialog.clear_content();
        let mut root = dialog.content_table();
        let target = dialog.to_gd();

        // `loadDialog.show()`: the in-game editor map list.
        let open_map_load = {
            let target = target.clone();
            Callable::from_fn("editor_import_map_load", move |_args: &[&Variant]| {
                open_named_dialog("map_load", "");
                close_node(&target);
            })
        };
        add_chooser_entry(
            &mut root,
            "@editor.importmap",
            "@editor.importmap.description",
            "download",
            open_map_load,
        );

        // `FileChooser.open(extension)` -> `beginEdit(Map|Pixmap)`.
        let chooser = chooser_context(true, &["msav"]);
        let open_file = {
            let target = target.clone();
            Callable::from_fn("editor_import_file", move |_args: &[&Variant]| {
                open_chooser_route(&target, "importfile", &chooser, |dialog, path| {
                    editor_import_file_chosen(dialog, path);
                });
            })
        };
        add_chooser_entry(
            &mut root,
            "@editor.importfile",
            "@editor.importfile.description",
            "file-text",
            open_file,
        );

        let chooser = chooser_context(true, &["png"]);
        let open_image = {
            let target = target.clone();
            Callable::from_fn("editor_import_image", move |_args: &[&Variant]| {
                open_chooser_route(&target, "importimage", &chooser, |dialog, path| {
                    editor_import_file_chosen(dialog, path);
                });
            })
        };
        add_chooser_entry(
            &mut root,
            "@editor.importimage",
            "@editor.importimage.description",
            "image",
            open_image,
        );
    }
}

/// One `createDialog` row: icon + wrapped name + grey description.
pub(super) fn add_chooser_entry(
    root: &mut Gd<MindTable>,
    name_key: &str,
    desc_key: &str,
    icon: &str,
    handler: Callable,
) {
    // code-instantiated: the import/export entries are data-driven (fixed rows).
    let mut button = factory::button("");
    button.set_custom_minimum_size(IMPORT_BUTTON_SIZE);
    button.connect("pressed", &handler);
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.add_theme_constant_override("separation", 10);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);
    let mut glyph = factory::glyph_label(icon, 26, Color::WHITE);
    glyph.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    glyph.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&glyph);
    let mut text = VBoxContainer::new_alloc();
    text.set_h_size_flags(SizeFlags::EXPAND_FILL);
    text.set_alignment(AlignmentMode::CENTER);
    text.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&text);
    let mut name_label = factory::label(&kit::t(name_key).to_string());
    name_label.set_mouse_filter(MouseFilter::IGNORE);
    text.add_child(&name_label);
    let mut description = factory::label(&kit::t(desc_key).to_string());
    let mut description_rich = rich(&description);
    description_rich.add_theme_color_override("default_color", styles::ARC_GRAY);
    description_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
    description.set_mouse_filter(MouseFilter::IGNORE);
    text.add_child(&description);
    root.bind_mut()
        .add(&button)
        .width(IMPORT_BUTTON_SIZE.x)
        .height(IMPORT_BUTTON_SIZE.y)
        .pad(2.0);
    root.bind_mut().row();
}

/// `_on_file_chosen`: `beginEdit(Map|Pixmap)` for the pending route.
fn editor_import_file_chosen(target: &mut Gd<MindDialog>, path: &str) {
    let action = take_meta_string(target, CHOOSER_ACTION_META);
    close_named_dialog("file_chooser");
    let Some(mut editor) = kit::editor() else {
        return;
    };
    match action.as_str() {
        "importfile" => {
            // `MapIO.isImage(file)` -> `@editor.errorimage`.
            let extension = file_extension(path).to_lowercase();
            if extension == "png" || extension == "jpg" || extension == "jpeg" {
                notify_error_key("@editor.errorimage");
                return;
            }
            let opened = editor
                .call("begin_edit_map", &[GString::from(path).to_variant()])
                .try_to::<bool>()
                .unwrap_or(false);
            if opened {
                close_node(target);
            } else {
                notify_error_key("@editor.errorload");
            }
        }
        "importimage" => {
            let opened = editor
                .call("begin_edit_image", &[GString::from(path).to_variant()])
                .try_to::<bool>()
                .unwrap_or(false);
            if opened {
                close_node(target);
            } else {
                notify_error_key("@editor.errorload");
            }
        }
        _ => {}
    }
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "editor_import" => Some(Box::new(EditorImportDialog)),
        _ => None,
    }
}

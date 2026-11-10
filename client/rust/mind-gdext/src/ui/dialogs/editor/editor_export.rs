// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map editor export dialog controller.
//! Upstream: editor/MapEditorDialog.java

use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;

use super::util::*;

use super::editor_import::add_chooser_entry;

// ---------------------------------------------------------------------------
// EditorExportDialog
// ---------------------------------------------------------------------------

/// `@editor.export`: the two export routes — the `.msav` (`MapIO.writeMap`) and
/// a PNG (`MapIO.writeImage`) (`client/ui/dialogs/editor_export_dialog.gd`).
pub struct EditorExportDialog;

impl DialogController for EditorExportDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.export");
        dialog.should_pause = false;
        dialog.add_default_close_button();
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.build(dialog);
    }
}

impl EditorExportDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        let mut root = dialog.content_table();
        let target = dialog.to_gd();

        let chooser = chooser_context(false, &["msav"]);
        let export_file = {
            let target = target.clone();
            Callable::from_fn("editor_export_file", move |_args: &[&Variant]| {
                open_chooser_route(&target, "exportfile", &chooser, |dialog, path| {
                    editor_export_file_chosen(dialog, path);
                });
            })
        };
        add_chooser_entry(
            &mut root,
            "@editor.exportfile",
            "@editor.exportfile.description",
            "file-text",
            export_file,
        );

        let chooser = chooser_context(false, &["png"]);
        let export_image = {
            let target = target.clone();
            Callable::from_fn("editor_export_image", move |_args: &[&Variant]| {
                open_chooser_route(&target, "exportimage", &chooser, |dialog, path| {
                    editor_export_file_chosen(dialog, path);
                });
            })
        };
        add_chooser_entry(
            &mut root,
            "@editor.exportimage",
            "@editor.exportimage.description",
            "image",
            export_image,
        );
    }
}

/// `_on_file_chosen`: `MapIO.writeMap`/`writeImage` for the pending route.
fn editor_export_file_chosen(target: &mut Gd<MindDialog>, path: &str) {
    let action = take_meta_string(target, CHOOSER_ACTION_META);
    close_named_dialog("file_chooser");
    let Some(mut editor) = kit::editor() else {
        return;
    };
    let ok = match action.as_str() {
        "exportfile" => editor
            .call("export_map", &[GString::from(path).to_variant()])
            .try_to::<bool>()
            .unwrap_or(false),
        "exportimage" => editor
            .call("export_image", &[GString::from(path).to_variant()])
            .try_to::<bool>()
            .unwrap_or(false),
        _ => false,
    };
    if action.is_empty() {
        return;
    }
    if ok {
        close_node(target);
    } else {
        notify_error_key("@editor.errorsave");
    }
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "editor_export" => Some(Box::new(EditorExportDialog)),
        _ => None,
    }
}

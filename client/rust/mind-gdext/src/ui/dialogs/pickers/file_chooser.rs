// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The fallback file chooser dialog controller.
//! Upstream: ui/dialogs/FileChooserDialog.java

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::DirAccess;
use godot::classes::LineEdit;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindTable;
use crate::ui::kit::widgets::factory;

use crate::ui::dialogs::editor::util::*;

// ---------------------------------------------------------------------------
// FileChooserDialog
// ---------------------------------------------------------------------------

/// Fallback file browser (`@file_chooser`, `client/ui/dialogs/
/// file_chooser_dialog.gd`, `FileChooserDialog.java`): a path field over the
/// directory listing, an extension filter and the one-shot `file_chosen`
/// signal. The native `DisplayServer.file_dialog_show` path is plan 22's.
pub struct FileChooserDialog {
    state: Rc<RefCell<FileChooserState>>,
}

/// `_open`/`_extensions`/`_directory` plus the built field and list.
struct FileChooserState {
    is_open: bool,
    extensions: Vec<String>,
    directory: String,
    field: Option<Gd<LineEdit>>,
    list: Option<Gd<MindTable>>,
}

impl Default for FileChooserState {
    fn default() -> Self {
        Self {
            is_open: true,
            extensions: Vec::new(),
            directory: "user://".to_string(),
            field: None,
            list: None,
        }
    }
}

impl Default for FileChooserDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(FileChooserState::default())),
        }
    }
}

impl DialogController for FileChooserDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        dialog.set_title_key("@open");
        self.build(dialog);
        dialog.add_default_close_button();
        self.rebuild(dialog);
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let is_open = dialog
            .context_value("open")
            .map(|value| value.to::<bool>())
            .unwrap_or(true);
        let extensions = string_array(dialog.context_value("extensions"));
        {
            let mut state = self.state.borrow_mut();
            state.is_open = is_open;
            if dialog.context_value("extensions").is_some() {
                state.extensions = extensions;
            }
        }
        dialog.set_title_key(if is_open { "@open" } else { "@save" });
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        // `FileChooserDialog.open` filters by its extensions; the GDScript only
        // rebuilt in `_ready`, so a re-open kept the previous listing (and the
        // new extension list never reached the filter).
        self.rebuild(dialog);
    }
}

impl FileChooserDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        let mut root = dialog.content_table();
        let mut field = factory::field(&kit::t("@filechooser.path").to_string());
        let target = dialog.to_gd();
        let state = self.state.clone();
        let callable = Callable::from_fn("file_chooser_navigate", move |args: &[&Variant]| {
            let path = args
                .first()
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_default();
            file_chooser_navigate(&target, &state, &path.to_string());
        });
        field.connect("text_submitted", &callable);
        root.bind_mut().add(&field).grow_x().pad(4.0);
        root.bind_mut().row();
        let list = factory::table();
        root.bind_mut().add(&list).grow_x().grow_y();
        {
            let mut state = self.state.borrow_mut();
            state.field = Some(field);
            state.list = Some(list);
        }
    }

    fn rebuild(&mut self, dialog: &mut MindDialog) {
        let target = dialog.to_gd();
        file_chooser_rebuild(&target, &self.state);
    }
}

/// `_navigate`: sets the directory and re-lists it.
fn file_chooser_navigate(
    target: &Gd<MindDialog>,
    state: &Rc<RefCell<FileChooserState>>,
    path: &str,
) {
    {
        let mut state = state.borrow_mut();
        if !path.is_empty() {
            state.directory = path.to_string();
        }
    }
    file_chooser_rebuild(target, state);
}

/// `_rebuild`: `..`, the matching files, then the subdirectories.
fn file_chooser_rebuild(target: &Gd<MindDialog>, state: &Rc<RefCell<FileChooserState>>) {
    let (list, field, directory, extensions) = {
        let state = state.borrow();
        (
            state.list.clone(),
            state.field.clone(),
            state.directory.clone(),
            state.extensions.clone(),
        )
    };
    let (Some(mut list), Some(mut field)) = (list, field) else {
        return;
    };
    list.bind_mut().clear();
    field.set_text(&GString::from(directory.as_str()));
    let Some(dir) = DirAccess::open(&GString::from(directory.as_str())) else {
        // The GDScript handed the raw key to `label` (`@filechooser.unreadable`
        // was shown literally); resolve it here.
        let label = factory::label(&kit::t("@filechooser.unreadable").to_string());
        list.bind_mut().add(&label).pad(4.0);
        return;
    };
    // code-instantiated: directory entries are runtime filesystem data.
    if directory != "user://" && directory != "/" {
        let mut up = factory::button("..");
        let target_for_up = target.clone();
        let state_for_up = state.clone();
        let callable = Callable::from_fn("file_chooser_up", move |_args: &[&Variant]| {
            let current = state_for_up.borrow().directory.clone();
            let path = simplify_path(&path_join(&current, ".."));
            file_chooser_navigate(&target_for_up, &state_for_up, &path);
        });
        up.connect("pressed", &callable);
        list.bind_mut().add(&up).grow_x().pad(1.0);
        list.bind_mut().row();
    }
    for file_name in dir.get_files().as_slice() {
        let name = file_name.to_string();
        if !extensions.is_empty() && !file_chooser_matches(&name, &extensions) {
            continue;
        }
        let mut file = factory::button(&name);
        let target_for_file = target.clone();
        let state_for_file = state.clone();
        let callable = Callable::from_fn("file_chooser_pick", move |_args: &[&Variant]| {
            let current = state_for_file.borrow().directory.clone();
            let path = path_join(&current, &name);
            file_chooser_choose(&target_for_file, &path);
        });
        file.connect("pressed", &callable);
        list.bind_mut().add(&file).grow_x().pad(1.0);
        list.bind_mut().row();
    }
    for dir_name in dir.get_directories().as_slice() {
        let name = dir_name.to_string();
        let mut folder = factory::button(&format!("{name}/"));
        let target_for_folder = target.clone();
        let state_for_folder = state.clone();
        let callable = Callable::from_fn("file_chooser_enter", move |_args: &[&Variant]| {
            let current = state_for_folder.borrow().directory.clone();
            let path = path_join(&current, &name);
            file_chooser_navigate(&target_for_folder, &state_for_folder, &path);
        });
        folder.connect("pressed", &callable);
        list.bind_mut().add(&folder).grow_x().pad(1.0);
        list.bind_mut().row();
    }
}

/// `_matches`: the lowercased extension is one of the allowed ones.
fn file_chooser_matches(file_name: &str, extensions: &[String]) -> bool {
    let extension = file_extension(file_name).to_lowercase();
    extensions
        .iter()
        .any(|allowed| extension == allowed.to_lowercase())
}

/// `_choose`: emit `file_chosen(path)` and hide.
fn file_chooser_choose(target: &Gd<MindDialog>, path: &str) {
    if !target.is_instance_valid() {
        return;
    }
    emit_dialog_signal(target, "file_chosen", &[GString::from(path).to_variant()]);
    hide_node(target);
}

/// Emits one `MindDialog` signal from an engine callback.
pub(super) fn emit_dialog_signal(target: &Gd<MindDialog>, signal: &str, args: &[Variant]) {
    if !target.is_instance_valid() {
        return;
    }
    let mut node = target.clone();
    let _ = node.emit_signal(signal, args);
}

/// `BaseDialog.hide_dialog()` from an engine callback.
pub(super) fn hide_node(target: &Gd<MindDialog>) {
    if !target.is_instance_valid() {
        return;
    }
    let mut node = target.clone();
    let _ = node.call("hide_dialog", &[]);
}

/// A context/read-model value as a list of strings (`PackedStringArray` or
/// `Array`; empty when absent).
fn string_array(value: Option<Variant>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    if let Ok(array) = value.try_to::<VarArray>() {
        return array
            .iter_shared()
            .map(|entry| entry.to::<GString>().to_string())
            .collect();
    }
    value
        .try_to::<PackedStringArray>()
        .unwrap_or_default()
        .as_slice()
        .iter()
        .map(|entry| entry.to_string())
        .collect()
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "file_chooser" => Some(Box::new(FileChooserDialog::default())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_extension_matches_godot() {
        assert_eq!(file_extension("user://maps/a.msav"), "msav");
        assert_eq!(file_extension("a.png"), "png");
        assert_eq!(file_extension("noext"), "");
        assert_eq!(file_extension("dir.d/file"), "");
    }

    #[test]
    fn path_join_keeps_schemes() {
        assert_eq!(path_join("user://", "a.msav"), "user://a.msav");
        assert_eq!(path_join("user://maps", "a.msav"), "user://maps/a.msav");
        assert_eq!(path_join("user://maps/", "a.msav"), "user://maps/a.msav");
        assert_eq!(path_join("", "a.msav"), "a.msav");
        assert_eq!(path_join("user://maps", "user://b"), "user://b");
    }

    #[test]
    fn simplify_path_collapses_segments() {
        assert_eq!(simplify_path("user://a/../"), "user://");
        assert_eq!(simplify_path("user://a/b/.."), "user://a");
        assert_eq!(simplify_path("/a/./b//c"), "/a/b/c");
        assert_eq!(simplify_path("user://"), "user://");
    }
}

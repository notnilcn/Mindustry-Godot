// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Native file-dialog bridge (plan 22 §3.5).
//!
//! Bridges plan 14's `FileChooserParams` onto Godot's
//! `DisplayServer.file_dialog_show` (plan 14 owns the dialog fallback). The
//! callback receives `(status, selected_paths, selected_filter_index)`, matching
//! the Godot 4.7 Callable API; the caller posts the result back to the main loop.

use godot::builtin::{Callable, PackedStringArray};
use godot::classes::DisplayServer;
use godot::classes::display_server::{Feature, FileDialogMode};
use godot::obj::Singleton;

/// Maps the plan-14 chooser flags to the native dialog mode.
pub fn native_mode(save: bool, multiple: bool, directory: bool) -> FileDialogMode {
    if directory {
        FileDialogMode::OPEN_DIR
    } else if save {
        FileDialogMode::SAVE_FILE
    } else if multiple {
        FileDialogMode::OPEN_FILES
    } else {
        FileDialogMode::OPEN_FILE
    }
}

/// Whether the current display server implements native file dialogs.
pub fn has_native_dialogs() -> bool {
    DisplayServer::singleton().has_feature(Feature::NATIVE_DIALOG_FILE)
}

/// Shows a native file dialog; returns `false` when the backend is unavailable
/// so the caller can use plan 14's `FileChooserDialog` fallback.
///
/// `filters` uses Godot's `*.ext;Description;mime/type` pattern strings. On save
/// the caller appends the extension when missing (plan 14 fix).
#[allow(clippy::too_many_arguments)]
pub fn show_file_dialog(
    title: &str,
    current_dir: &str,
    file_name: &str,
    show_hidden: bool,
    save: bool,
    multiple: bool,
    directory: bool,
    filters: &PackedStringArray,
    callback: &Callable,
) -> bool {
    let mut display = DisplayServer::singleton();
    if !display.has_feature(Feature::NATIVE_DIALOG_FILE) {
        return false;
    }
    display.file_dialog_show(
        title,
        current_dir,
        file_name,
        show_hidden,
        native_mode(save, multiple, directory),
        filters,
        callback,
    ) == godot::global::Error::OK
}

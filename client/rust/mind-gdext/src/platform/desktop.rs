// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Desktop window/launch seam (plan 22 §3.4/§6.1).
//!
//! `Vars.checkLaunch`/`finishLaunch` are ported as the `launchid.dat` failed-launch
//! detector; the data-root resolver applies the §6.1 precedence; dropped/passed
//! files are classified with the plan-04 extension/meta rules. The actual import
//! UI hand-off belongs to plans 04/06/12/14, so this module only routes and logs.

use std::path::{Path, PathBuf};

use godot::classes::DisplayServer;
use godot::classes::display_server::WindowMode;
use godot::obj::Singleton;

use mind_core::platform::args as core_args;
use mind_core::platform::file_import::{self, ImportKind};

/// Failed-launch marker written at boot and removed after a successful load.
pub const LAUNCH_ID: &str = "launchid.dat";

/// Resolves the client data root per plan 22 §6.1 (arg wins, then env, then the
/// platform default). `launch_data_dir` is `LaunchArgs::data_dir`.
pub fn data_root(launch_data_dir: Option<&Path>) -> PathBuf {
    core_args::data_root(launch_data_dir, |key| std::env::var(key).ok())
        .unwrap_or_else(mind_core::config::default_data_dir)
}

/// `Vars.checkLaunch`: creates `<root>/launchid.dat`.
///
/// Returns `true` when a marker already existed, meaning the previous launch may
/// have crashed (upstream sets `failedToLaunch`; the crash report is the
/// actionable surface).
pub fn check_launch(root: &Path) -> bool {
    let marker = root.join(LAUNCH_ID);
    let previous = marker.exists();
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, b"launching\n");
    previous
}

/// `Vars.finishLaunch`: removes the failed-launch marker.
pub fn finish_launch(root: &Path) {
    let _ = std::fs::remove_file(root.join(LAUNCH_ID));
}

/// Classifies a dropped/passed path (`ClientLauncher.fileDropped`).
pub fn classify(path: &Path) -> ImportKind {
    file_import::from_extension(path)
}

/// Routes a dropped/passed file, logging the hand-off the UI layer performs.
///
/// Returns `true` when the extension is accepted (the file enters the import
/// flow), matching `ClientLauncher.fileDropped`.
pub fn handle_file_import(path: &Path) -> bool {
    let kind = classify(path);
    match kind {
        ImportKind::Schematic => {
            log::info!(
                "[platform] importing schematic {} (plan-14 schematic flow)",
                path.display()
            );
            true
        }
        ImportKind::Save => {
            log::info!(
                "[platform] importing save {} (plan-04 slot + plan-14 load flow)",
                path.display()
            );
            true
        }
        ImportKind::Unsupported => false,
        ImportKind::Map | ImportKind::CampaignSave => {
            // These require the save-meta refinement owned by plan 04 before the
            // correct flow can be chosen; log and accept for now.
            log::warn!(
                "[platform] {} requires save-meta routing (plan 04); accepted",
                path.display()
            );
            true
        }
    }
}

/// Applies `-maximized`/`-width`/`-height` to the live window (plan 22 §3.4).
pub fn apply_window_args(maximized: Option<bool>, width: Option<u32>, height: Option<u32>) -> bool {
    let mut display = DisplayServer::singleton();
    if let (Some(width), Some(height)) = (width, height)
        && width > 0
        && height > 0
    {
        display.window_set_size(godot::builtin::Vector2i::new(width as i32, height as i32));
    }
    if let Some(maximized) = maximized {
        let mode = if maximized {
            WindowMode::MAXIMIZED
        } else {
            WindowMode::WINDOWED
        };
        display.window_set_mode(mode);
    }
    true
}

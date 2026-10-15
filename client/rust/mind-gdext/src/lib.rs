// SPDX-License-Identifier: GPL-3.0-only

//! `mind-gdext` — the Godot 4.7 GDExtension cdylib.
//!
//! Owns view drivers, input, the fixed-step frame pump and autoloads. Contains no
//! game rules (D1/HLP §2.2). The M4 spine classes are `MindSimHost`
//! (sim owner + fixed 60 Hz pump + input/API), `MindCamera2D` (pan/zoom,
//! screen↔tile) and `MindTileGrid` (`_draw` grid/quads) per plan §3.5.

mod assets;
mod audio;
mod camera;
mod campaign;
mod content_boot;
mod editor;
mod fx;
mod hello;
mod input;
mod log_bridge;
mod logic;
mod mods;
mod net;
pub mod platform;
mod render;
mod settings;
mod settings_hub;
mod sim_host;
mod stdb;
mod tile_grid;
mod ui;

pub use assets::MindAssets;
pub use audio::MindAudio;
pub use camera::MindCamera2D;
pub use campaign::MindCampaign;
pub use editor::{MindEditor, MindEditorRenderer, MindPreview};
pub use fx::MindFx;
pub use input::MindInput;
pub use logic::MindLogic;
pub use mods::MindMods;
pub use net::MindNet;
pub use platform::MindPlatform;
pub use render::{MindMinimap, MindRender, MindWorldRenderer};
pub use sim_host::MindSimHost;
pub use stdb::{StdbBinder, StdbConnector};
pub use tile_grid::MindTileGrid;
pub use ui::{MindHud, MindUi};

use godot::prelude::*;

/// Version of the `mind-gdext` crate, taken from `Cargo.toml`.
pub const MIND_VERSION: &str = env!("CARGO_PKG_VERSION");

struct MindExtension;

#[gdextension]
unsafe impl ExtensionLibrary for MindExtension {
    fn on_stage_init(stage: InitStage) {
        // Scene is this library's default `min_level()` and runs before the main
        // scene is instantiated, so the bridge is live for the first `_ready`.
        if stage == InitStage::Scene {
            // One data root for every facade: settings, saves and imports all
            // resolve under `user://` like the campaign store.
            mind_core::config::set_default_data_dir(
                godot::classes::ProjectSettings::singleton()
                    .globalize_path("user://")
                    .to_string(),
            );
            // `Vars.checkLaunch` (Vars.java:398-408) runs in `ClientLauncher.main`
            // before anything reads `failedToLaunch`: the `MindMods` autoload
            // bootstraps (and skips mod loading) during autoload `_ready`, which
            // precedes `MindPlatform`'s scene insert. The Godot editor also
            // instantiates the scene and never clears the marker, so only the
            // game process runs the check.
            log_bridge::install();
            // Chains the log bridge's hook so a panic flushes `last_log.txt`
            // before writing its `crashes/` report (plan 22 §3.4).
            platform::crash::install();
            if !godot::classes::Engine::singleton().is_editor_hint() {
                let launch = platform::args::parse_process_args();
                platform::note_launch(&platform::desktop::data_root(launch.data_dir.as_deref()));
            }
        }
    }
}

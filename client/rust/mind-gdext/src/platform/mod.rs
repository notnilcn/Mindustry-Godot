// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindPlatform` — the Godot-facing platform capability node (plan 22 §3.3).
//!
//! Owns the capability snapshot, the `#[func]` MCP probes and the host glue for
//! window/args, native dialogs, crash handling, URI routing and the service
//! registry. Scene insertion at `/root/Spine/MindPlatform` is declared in
//! `client/scenes/game.tscn` (plan 22 §3.3).

pub mod args;
pub mod crash;
pub mod desktop;
pub mod dialogs;
pub mod discord;
pub mod service;
pub mod update;
pub mod uri;
pub mod workshop;

use godot::classes::{DisplayServer, INode, Node as GdNode, Os};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::platform::caps::{PerformanceTier, PlatformCaps, PlatformKind};
use mind_core::version::BuildInfo;

use service::ServiceRegistry;

/// Platform capability node (appended to the spine by `game.tscn`).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindPlatform {
    base: Base<GdNode>,
    caps: PlatformCaps,
    test_mobile: bool,
    services: ServiceRegistry,
}

#[godot_api]
impl INode for MindPlatform {
    fn init(base: Base<GdNode>) -> Self {
        Self {
            base,
            caps: PlatformCaps::desktop("en"),
            test_mobile: false,
            services: ServiceRegistry::default(),
        }
    }

    fn ready(&mut self) {
        let os = Os::singleton();
        let locale = os.get_locale().to_string();
        let cores = os.get_processor_count().max(1) as u32;
        let launch = args::parse_process_args();
        self.test_mobile = launch.test_mobile;
        self.caps = PlatformCaps {
            kind: PlatformKind::Desktop,
            headless: false,
            mobile: false,
            test_mobile: launch.test_mobile,
            locale: locale.clone(),
            locale_default: locale,
            tier: PerformanceTier::from_processor_count(PlatformKind::Desktop, cores),
            steam: false,
        };
        if launch.debug {
            log::info!("[platform] launch args: {launch:?}");
        }

        // Window/mode/args plumbing (plan 22 §3.4).
        desktop::apply_window_args(launch.maximized, launch.width, launch.height);
        let root = desktop::data_root(launch.data_dir.as_deref());
        if desktop::check_launch(&root) {
            log::warn!(
                "[platform] previous launch may have crashed (leftover {}); check {}/crashes/",
                desktop::LAUNCH_ID,
                root.display()
            );
        }

        log::info!(
            "[platform] MindPlatform ready: locale={} cores={} tier={:?} testMobile={} dataRoot={}",
            self.caps.locale,
            cores,
            self.caps.tier,
            self.caps.test_mobile,
            root.display()
        );
    }
}

#[godot_api]
impl MindPlatform {
    /// The embedded `Version.java` build report (plan 22 §3.2).
    #[func]
    pub fn get_build_info(&self) -> VarDictionary {
        let info = BuildInfo::embedded();
        let mut dict = VarDictionary::new();
        dict.set("type", info.r#type.clone());
        dict.set("modifier", info.modifier.clone());
        dict.set("commitHash", info.commit_hash.clone());
        dict.set("buildDate", info.build_date.clone());
        dict.set("number", info.number as i64);
        dict.set("build", info.build as i64);
        dict.set("revision", info.revision as i64);
        dict.set("isSteam", info.is_steam);
        dict.set("buildString", info.build_string());
        dict.set("combined", info.combined());
        dict
    }

    /// The current platform capability snapshot (plan 22 §3.3).
    #[func]
    pub fn get_platform_caps(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        dict.set("kind", format!("{:?}", self.caps.kind));
        dict.set("headless", self.caps.headless);
        dict.set("mobile", self.caps.mobile);
        dict.set("testMobile", self.caps.test_mobile);
        dict.set("locale", self.caps.locale.clone());
        dict.set("tier", format!("{:?}", self.caps.tier));
        dict.set("steam", self.caps.steam);
        dict
    }

    /// Window state (M1 applies launch args; M0 reports the live viewport).
    #[func]
    pub fn window_state(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let display = DisplayServer::singleton();
        let size = display.window_get_size();
        dict.set("width", size.x as i64);
        dict.set("height", size.y as i64);
        dict.set("mode", format!("{:?}", display.window_get_mode()));
        dict
    }

    /// Sets the window fullscreen/windowed mode (plan 22 §3.4).
    #[func]
    pub fn set_fullscreen(&mut self, enabled: bool) {
        use godot::classes::display_server::WindowMode;
        let mut display = DisplayServer::singleton();
        display.window_set_mode(if enabled {
            WindowMode::FULLSCREEN
        } else {
            WindowMode::WINDOWED
        });
        log::info!("[platform] set_fullscreen({enabled})");
    }

    /// Whether the no-op `GameService` is active (always `false` until OD4).
    #[func]
    pub fn service_enabled(&self) -> bool {
        self.services.service().enabled()
    }

    /// Whether the display server offers native file dialogs (plan 22 §3.5).
    #[func]
    pub fn has_native_dialogs(&self) -> bool {
        dialogs::has_native_dialogs()
    }

    /// Debug hook: routes a dropped/passed file through the import classifier.
    #[func]
    pub fn dev_file_drop(&mut self, path: GString) -> bool {
        desktop::handle_file_import(std::path::Path::new(&path.to_string()))
    }

    /// Debug hook: marks a launch successful (`Vars.finishLaunch`).
    #[func]
    pub fn finish_launch(&mut self) {
        let root = desktop::data_root(None);
        desktop::finish_launch(&root);
    }

    /// Debug hook: opens a URI (`OS.shell_open`).
    #[func]
    pub fn dev_open_uri(&mut self, uri: GString) -> bool {
        uri::open_uri(&uri.to_string())
    }

    /// Debug hook (debug builds only): writes a crash report for the given cause.
    #[func]
    pub fn dev_crash(&mut self, reason: GString) -> GString {
        match crash::write_report(&reason.to_string(), &[]) {
            Some(path) => GString::from(path.to_string_lossy().as_ref()),
            None => GString::new(),
        }
    }

    /// Debug hook: toggles the `--mobile-preview` branch (`Vars.testMobile`).
    #[func]
    pub fn dev_set_test_mobile(&mut self, enabled: bool) -> bool {
        self.test_mobile = enabled;
        self.caps.test_mobile = enabled;
        true
    }
}

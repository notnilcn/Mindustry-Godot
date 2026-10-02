// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindPlatform` — the Godot-facing platform capability node (plan 22 M0 §3.3).
//!
//! M0 lands the class, its capability seam and the `#[func]` MCP probes. Window
//! control, native dialogs, crash handling, services and URI routing land in M1+
//! (they need plan 14/15/16 or the export templates). Scene insertion at
//! `/root/Spine/MindPlatform` is deferred to the orchestrator (shared
//! `spine.tscn`), per plan 22 §3.3.

pub mod args;

use godot::classes::{DisplayServer, INode, Node as GdNode, Os};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::platform::caps::{PerformanceTier, PlatformCaps, PlatformKind};
use mind_core::version::BuildInfo;

/// Platform capability node (appended to the spine by the orchestrator).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindPlatform {
    base: Base<GdNode>,
    caps: PlatformCaps,
    test_mobile: bool,
}

#[godot_api]
impl INode for MindPlatform {
    fn init(base: Base<GdNode>) -> Self {
        Self {
            base,
            caps: PlatformCaps::desktop("en"),
            test_mobile: false,
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
        log::info!(
            "[platform] MindPlatform ready: locale={} cores={} tier={:?} testMobile={}",
            self.caps.locale,
            cores,
            self.caps.tier,
            self.caps.test_mobile
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

    /// Sets the window fullscreen mode (M1 wires the launch args; M0 logs).
    #[func]
    pub fn set_fullscreen(&mut self, enabled: bool) {
        log::info!("[platform] set_fullscreen({enabled}) — applied at M1");
    }

    /// Debug hook: routes a dropped/passed file (M1 wires the import router).
    #[func]
    pub fn dev_file_drop(&mut self, path: GString) -> bool {
        log::warn!("[platform] dev_file_drop({path}) — not wired until M1");
        false
    }

    /// Debug hook: opens a URI (M1 wires `OS.shell_open`).
    #[func]
    pub fn dev_open_uri(&mut self, uri: GString) -> bool {
        log::warn!("[platform] dev_open_uri({uri}) — not wired until M1");
        false
    }

    /// Debug hook: toggles the `--mobile-preview` branch (`Vars.testMobile`).
    #[func]
    pub fn dev_set_test_mobile(&mut self, enabled: bool) -> bool {
        self.test_mobile = enabled;
        self.caps.test_mobile = enabled;
        true
    }
}

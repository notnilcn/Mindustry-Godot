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

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use godot::classes::notify::NodeNotification;
use godot::classes::{DisplayServer, INode, Node as GdNode, Os};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::io::fs::Paths;
use mind_core::platform::caps::{PerformanceTier, PlatformCaps, PlatformKind};
use mind_core::version::BuildInfo;

use service::ServiceRegistry;

/// `Vars.failedToLaunch`: `launchid.dat` existed at boot (`Vars.java:398-408`).
/// Recorded once per process by [`note_launch`].
static FAILED_TO_LAUNCH: OnceLock<bool> = OnceLock::new();

/// The data root the marker was checked against (`note_launch`), so
/// [`finish_launch`] deletes the same file even before the platform boot ran.
static LAUNCH_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// The main menu called [`finish_launch`] before the platform booted; retried
/// from [`note_launch`] so the boot never re-creates a stale marker.
static FINISH_PENDING: AtomicBool = AtomicBool::new(false);

/// `Vars.failedToLaunch` (`Vars.java:401-408`). `false` until [`note_launch`].
pub fn failed_to_launch() -> bool {
    FAILED_TO_LAUNCH.get().copied().unwrap_or(false)
}

/// `Vars.checkLaunch` once per process (`Vars.java:398-408`), memoized so a
/// GDExtension hot reload never treats its own marker as a new crash. Also
/// drains a [`finish_launch`] that ran before boot. Returns `failedToLaunch`.
pub fn note_launch(root: &Path) -> bool {
    if FAILED_TO_LAUNCH.get().is_none() {
        let failed = desktop::check_launch(root);
        let _ = FAILED_TO_LAUNCH.set(failed);
        let _ = LAUNCH_ROOT.set(root.to_path_buf());
        if failed {
            log::warn!(
                "[platform] previous launch may have crashed (leftover {}); check {}/crashes/",
                desktop::LAUNCH_ID,
                root.display()
            );
        }
        if FINISH_PENDING.swap(false, Ordering::SeqCst) {
            let _ = finish_launch(&Paths::resolve(None));
        }
    }
    failed_to_launch()
}

/// `Vars.finishLaunch` (`Vars.java:411-418`): records `lastBuild` /
/// `lastBuildString` and deletes `launchid.dat`. Idempotent; when
/// [`note_launch`] has not run yet the call is deferred so the later
/// `checkLaunch` cannot re-create the marker.
pub fn finish_launch(paths: &Paths) -> bool {
    let Some(root) = LAUNCH_ROOT.get() else {
        FINISH_PENDING.store(true, Ordering::SeqCst);
        return false;
    };
    desktop::finish_launch(root);
    let info = BuildInfo::embedded();
    crate::settings_hub::with_settings(|store| {
        store.put_i32("lastBuild", info.build);
        store.put_string("lastBuildString", &info.build_string());
    });
    if let Err(error) = crate::settings_hub::save(paths) {
        log::warn!("[platform] finishLaunch settings save failed: {error}");
    }
    true
}

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
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }
}

#[godot_api]
impl MindPlatform {
    /// Rebuilds the platform capability snapshot (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`). Every field write
    /// overwrites and the window args are idempotent.
    fn bootstrap(&mut self) {
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
        // The editor's scene instantiation must not write the crash marker
        // (extension init already checked it for the game process).
        if !godot::classes::Engine::singleton().is_editor_hint() {
            note_launch(&root);
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

    /// `Vars.finishLaunch`: marks the launch successful (`ClientLauncher.java:253`)
    /// — writes `lastBuild`/`lastBuildString` and deletes `launchid.dat`. The
    /// main menu calls this at first show; idempotent.
    #[func]
    pub fn finish_launch(&mut self) -> bool {
        self::finish_launch(&Paths::resolve(None))
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

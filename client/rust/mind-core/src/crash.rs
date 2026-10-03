// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Crash report builder — port of `core/src/mindustry/net/CrashHandler.java`.
//!
//! Ports `createReport`, `getModCause`/`getMatches`, the `crashes/` report
//! naming (`crash-report-<MM_dd_yyyy_HH_mm_ss>.txt`) and the Android-style
//! `crash_<millis>.txt` path. Every host fact (date, OS, GPU, memory, cores) is
//! injected by the caller so `mind-core` neither reads the wall clock nor the
//! OS; the Godot and headless hosts gather them. `last_log.txt` is owned by
//! [`crate::log`] and is never touched here (the crash report is a separate
//! `crashes/` artifact), so a crash can never lose the running log.

use std::path::{Path, PathBuf};

use crate::mods::loaded::LoadedMod;
use crate::version::BuildInfo;

/// Directory under the data root holding crash reports (`crashes/`).
pub const CRASH_DIR: &str = "crashes";

/// Prefix for the timestamped report file (`CrashHandler.handle`).
pub const REPORT_PREFIX: &str = "crash-report-";

/// Prefix for the millisecond legacy file (`CrashHandler.log`).
pub const LEGACY_PREFIX: &str = "crash_";

/// One decoded stack frame (class/function path + optional source file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackFrame {
    /// Fully-qualified symbol/class path (`element.getClassName()`).
    pub class_name: String,
    /// Source file name (`element.getFileName()`), without a directory.
    pub file_name: Option<String>,
}

impl StackFrame {
    /// A frame with a symbol path and no source file.
    pub fn new(class_name: impl Into<String>) -> Self {
        Self {
            class_name: class_name.into(),
            file_name: None,
        }
    }

    /// A frame with a symbol path and source file name.
    pub fn with_file(class_name: impl Into<String>, file_name: impl Into<String>) -> Self {
        Self {
            class_name: class_name.into(),
            file_name: Some(file_name.into()),
        }
    }
}

/// A loaded mod as needed by likely-cause detection and the report mod list.
///
/// The plan-20 [`LoadedMod`] converts into this so the report builder has no
/// dependency on the mod loader's internals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashMod {
    /// Internal mod name (`LoadedMod.name`).
    pub name: String,
    /// Human-readable name (`LoadedMod.meta.displayName` or the internal name).
    pub display_name: String,
    /// Mod version string (`""` when unknown).
    pub version: String,
    /// Java/script entry class (`LoadedMod.meta.main`), if any.
    pub main: Option<String>,
}

impl CrashMod {
    /// Builds a mod record for tests/hosts without a full [`LoadedMod`].
    pub fn new(
        name: impl Into<String>,
        display_name: impl Into<String>,
        version: impl Into<String>,
        main: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            display_name: display_name.into(),
            version: version.into(),
            main,
        }
    }
}

impl From<&LoadedMod> for CrashMod {
    fn from(mod_: &LoadedMod) -> Self {
        Self {
            name: mod_.name.clone(),
            display_name: mod_
                .meta
                .display_name
                .clone()
                .unwrap_or_else(|| mod_.name.clone()),
            version: mod_.meta.version().to_owned(),
            main: mod_.meta.main.clone(),
        }
    }
}

/// Host facts captured at crash time (all injected: no OS/wall-clock reads).
#[derive(Debug, Clone, Default)]
pub struct CrashContext {
    /// OS name (`OS.osName`).
    pub os_name: String,
    /// CPU architecture (`OS.osArch`).
    pub os_arch: String,
    /// Architecture bit width (`OS.osArchBits`), if known.
    pub os_arch_bits: Option<u32>,
    /// Graphics backend string, if available.
    pub gl_version: Option<String>,
    /// Runtime/engine version line (replaces upstream `Java Version`).
    pub runtime_version: String,
    /// Maximum available memory in MB (`maxMemory / 1024 / 1024`).
    pub max_memory_mb: u64,
    /// Logical processor count (`OS.cores`).
    pub cores: u32,
    /// `Vars.headless` (adds the `(Server)` suffix).
    pub headless: bool,
    /// Issue-tracker URL printed for vanilla crashes on stamped builds.
    pub report_issue_url: String,
}

/// Creates the crash report text (port of `CrashHandler.createReport`).
///
/// `date` is preformatted by the host (`"MMMM d, yyyy HH:mm:ss a"` upstream);
/// `exception` is the rendered backtrace/panic text.
pub fn create_report(
    exception: &str,
    frames: &[StackFrame],
    mods: &[CrashMod],
    build: &BuildInfo,
    date: &str,
    context: &CrashContext,
) -> String {
    let cause = get_mod_cause(frames, mods);

    let mut report = match cause {
        Some(index) => format!(
            "The mod '{}' ({}) has caused Mindustry to crash.\n",
            mods[index].display_name, mods[index].name
        ),
        None => "Mindustry has crashed. How unfortunate.\n".to_owned(),
    };
    if mods.is_empty() && build.build != -1 {
        report.push_str(&format!("Report this at {}\n\n", context.report_issue_url));
    }

    report.push_str(&format!("Version: {}", build.combined()));
    if build.build_date != "unknown" {
        report.push_str(&format!(" (Built {})", build.build_date));
    }
    if context.headless {
        report.push_str(" (Server)");
    }
    report.push('\n');
    report.push_str(&format!("Date: {date}\n"));

    report.push_str(&format!("OS: {}", context.os_name));
    if let Some(bits) = context.os_arch_bits {
        report.push_str(&format!(" x{bits}"));
    }
    report.push_str(&format!(" ({})\n", context.os_arch));
    if let Some(gl) = &context.gl_version {
        report.push_str(&format!("GL Version: {gl}\n"));
    }
    report.push_str(&format!("Runtime Version: {}\n", context.runtime_version));
    report.push_str(&format!(
        "Runtime Available Memory: {}mb\n",
        context.max_memory_mb
    ));
    report.push_str(&format!("Cores: {}\n", context.cores));

    if let Some(index) = cause {
        report.push_str(&format!(
            "Likely Cause: {} ({} v{})\n",
            mods[index].display_name, mods[index].name, mods[index].version
        ));
    }
    if mods.is_empty() {
        report.push_str("Mods: none (vanilla)\n");
    } else {
        let list = mods
            .iter()
            .map(|mod_| format!("{}:{}", mod_.name, mod_.version))
            .collect::<Vec<_>>()
            .join(", ");
        report.push_str(&format!("Mods: {list}\n"));
    }

    report.push_str("\n\n");
    report.push_str(exception);
    report
}

/// Returns the index of the mod likely to have caused `frames`
/// (port of `CrashHandler.getModCause`). Vanilla frames are skipped.
pub fn get_mod_cause(frames: &[StackFrame], mods: &[CrashMod]) -> Option<usize> {
    for frame in frames {
        let name = &frame.class_name;
        if is_vanilla(name) {
            continue;
        }
        for (index, mod_) in mods.iter().enumerate() {
            if let Some(main) = &mod_.main
                && get_matches(main, name) > 0
            {
                return Some(index);
            }
            if let Some(file) = &frame.file_name
                && file.ends_with(".js")
                && file.starts_with(&format!("{}/", mod_.name))
            {
                return Some(index);
            }
        }
    }
    None
}

/// Port of `CrashHandler.getMatches`: counts the common leading dotted segments,
/// ignoring common domain prefixes (`net|org|com|io`).
pub fn get_matches(name1: &str, name2: &str) -> usize {
    let arr1: Vec<&str> = name1.split('.').collect();
    let arr2: Vec<&str> = name2.split('.').collect();
    let mut matches = 0usize;
    for index in 0..arr1.len().min(arr2.len()) {
        if arr1[index] != arr2[index] {
            return index;
        } else if !matches!(arr1[index], "net" | "org" | "com" | "io") {
            matches += 1;
        }
    }
    matches
}

/// Engine/framework symbol prefixes that are never attributed to a mod.
///
/// Upstream checks `(mindustry|arc|java|javax|sun|jdk)`; the Rust port checks the
/// engine/system crates instead (plan 22 §3.4).
pub const VANILLA_PREFIXES: [&str; 5] = ["mindcore", "mind_", "std", "core", "alloc"];

/// Whether `class_name` names an engine/system symbol (not a mod symbol).
pub fn is_vanilla(class_name: &str) -> bool {
    VANILLA_PREFIXES
        .iter()
        .any(|prefix| class_name.starts_with(prefix))
}

/// Writes the timestamped report under `<root>/crashes/` (port of
/// `CrashHandler.handle`). `timestamp` is preformatted
/// `MM_dd_yyyy_HH_mm_ss`.
pub fn write_crash_report(
    root: impl AsRef<Path>,
    report: &str,
    timestamp: &str,
) -> std::io::Result<PathBuf> {
    let dir = root.as_ref().join(CRASH_DIR);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{REPORT_PREFIX}{timestamp}.txt"));
    std::fs::write(&path, report)?;
    Ok(path)
}

/// Writes the Android-style millisecond report under `<root>/crashes/`
/// (port of `CrashHandler.log`).
pub fn write_legacy_crash(
    root: impl AsRef<Path>,
    report: &str,
    millis: u128,
) -> std::io::Result<PathBuf> {
    let dir = root.as_ref().join(CRASH_DIR);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{LEGACY_PREFIX}{millis}.txt"));
    std::fs::write(&path, report)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build() -> BuildInfo {
        BuildInfo {
            r#type: "official".to_owned(),
            modifier: "release".to_owned(),
            commit_hash: "deadbee".to_owned(),
            build_date: "2026-10-03".to_owned(),
            number: 4,
            build: 43,
            revision: 2,
            is_steam: false,
            enabled: true,
        }
    }

    fn context(headless: bool) -> CrashContext {
        CrashContext {
            os_name: "Linux".to_owned(),
            os_arch: "amd64".to_owned(),
            os_arch_bits: Some(64),
            gl_version: Some("4.6".to_owned()),
            runtime_version: "Godot 4.7.2".to_owned(),
            max_memory_mb: 4096,
            cores: 8,
            headless,
            report_issue_url: "https://example.invalid/issues".to_owned(),
        }
    }

    #[test]
    fn report_contains_build_os_mods() {
        let mods = vec![CrashMod::new(
            "test-mod",
            "Test Mod",
            "1.2",
            Some("org.example.testmod.Main".to_owned()),
        )];
        let frames = vec![StackFrame::new("org.example.testmod.Main.update")];
        let report = create_report(
            "stack trace here",
            &frames,
            &mods,
            &build(),
            "October 3, 2026 5:00:00 PM",
            &context(false),
        );
        assert!(report.contains("The mod 'Test Mod' (test-mod) has caused Mindustry to crash."));
        assert!(report.contains("Version: release build 43.2 (deadbee)"));
        assert!(report.contains("(Built 2026-10-03)"));
        assert!(report.contains("Date: October 3, 2026 5:00:00 PM"));
        assert!(report.contains("OS: Linux x64 (amd64)"));
        assert!(report.contains("GL Version: 4.6"));
        assert!(report.contains("Runtime Available Memory: 4096mb"));
        assert!(report.contains("Cores: 8"));
        assert!(report.contains("Likely Cause: Test Mod (test-mod v1.2)"));
        assert!(report.contains("Mods: test-mod:1.2"));
        assert!(report.contains("stack trace here"));
        assert!(!report.contains("none (vanilla)"));
    }

    #[test]
    fn vanilla_report_has_server_suffix_and_issue_url() {
        let report = create_report(
            "boom",
            &[StackFrame::new("mind_core::sim::tick")],
            &[],
            &build(),
            "date",
            &context(true),
        );
        assert!(report.contains("Mindustry has crashed. How unfortunate."));
        assert!(report.contains("(Server)"));
        assert!(report.contains("Mods: none (vanilla)"));
        assert!(report.contains("Report this at https://example.invalid/issues"));
        assert!(!report.contains("Likely Cause:"));
    }

    #[test]
    fn likely_mod_from_frame() {
        let mods = vec![
            CrashMod::new("alpha", "Alpha", "1", Some("com.alpha.Main".to_owned())),
            CrashMod::new("beta", "Beta", "2", None),
        ];
        // Vanilla frames are ignored.
        assert_eq!(
            get_mod_cause(&[StackFrame::new("mind_core::world::step")], &mods),
            None
        );
        // A frame under the mod's main class is attributed to it.
        assert_eq!(
            get_mod_cause(&[StackFrame::new("com.alpha.Main.render")], &mods),
            Some(0)
        );
        // A script frame under the mod's folder is attributed to it.
        let script = vec![StackFrame::with_file("script.Handler", "beta/main.js")];
        assert_eq!(get_mod_cause(&script, &mods), Some(1));
    }

    #[test]
    fn get_matches_port() {
        assert_eq!(get_matches("com.alpha.Main", "com.alpha.Other"), 2);
        assert_eq!(get_matches("net.example.Foo", "net.example.Foo"), 2);
        assert_eq!(get_matches("com.alpha.Main", "org.alpha.Main"), 0);
        assert_eq!(get_matches("a.b.c", "a.b"), 2);
    }

    #[test]
    fn is_vanilla_prefixes() {
        assert!(is_vanilla("mind_core::sim::tick"));
        assert!(is_vanilla("std::panicking::begin_panic"));
        assert!(is_vanilla("core::fmt::write"));
        assert!(!is_vanilla("com.example.mod.Main"));
        assert!(!is_vanilla("org.example.mod.Main"));
    }

    #[test]
    fn write_report_creates_dir_and_preserves_last_log() {
        let root = std::env::temp_dir().join(format!("mind-crash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create");
        // The running log lives at the data root; the crash report must not clobber it.
        std::fs::write(root.join("last_log.txt"), "[I] still here\n").expect("log");

        let path = write_crash_report(&root, "report body", "10_03_2026_17_00_00").expect("report");
        assert!(path.ends_with("crashes/crash-report-10_03_2026_17_00_00.txt"));
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "report body");
        assert_eq!(
            std::fs::read_to_string(root.join("last_log.txt")).expect("log"),
            "[I] still here\n"
        );

        let legacy = write_legacy_crash(&root, "legacy", 123).expect("legacy");
        assert!(legacy.ends_with("crashes/crash_123.txt"));
        let _ = std::fs::remove_dir_all(&root);
    }
}

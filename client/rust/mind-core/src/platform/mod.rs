// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Platform capability trait (the `Vars` host seam).
//!
//! Ported from `core/src/mindustry/Vars.java` host fields (`headless`,
//! `dataDirectory`, `app.exit`) and `core/src/mindustry/core/Logic.java`. Sim
//! systems never read platform state to make simulation decisions; the trait is
//! for logging/data-dir/settings only (plan 05 §3.3 invariant 3).

pub mod args;
pub mod assoc;
pub mod caps;
pub mod file_import;
pub mod hooks;
pub mod uri;

use std::path::{Path, PathBuf};

pub use args::{LaunchArgs, data_root, parse as parse_launch_args};
pub use caps::{PerformanceTier, PlatformCaps, PlatformKind};
pub use hooks::{ClientHooks, ClientHooksHandle, NoopClientHooks};

/// Log severity (mirrors Arc `Log.LogLevel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Verbose diagnostics.
    Debug,
    /// Informational.
    Info,
    /// Recoverable problem.
    Warn,
    /// Failure.
    Error,
}

/// Host capability seam. `mind-core` only ever calls these; `mind-gdext`
/// supplies a Godot-backed implementation, headless supplies [`HeadlessPlatform`].
pub trait Platform: Send {
    /// Whether the process is headless (`Vars.headless`).
    fn headless(&self) -> bool;
    /// Data directory (`Vars.dataDirectory`).
    fn data_dir(&self) -> &Path;
    /// Writes a log line.
    fn log(&self, level: LogLevel, msg: &str);
    /// Requests application exit (`Core.app.exit`).
    fn exit(&self);
    /// Wall-clock milliseconds since the Unix epoch (`Time.millis` =
    /// `System.currentTimeMillis()`, `Time.java:122-124`; written into the save
    /// meta `saved` tag, `SaveVersion.java:130`). Never used by sim scheduling
    /// (D8), only by view/UI and save metadata.
    fn millis(&self) -> u64;
}

/// The default headless host: stdout/file logging and a workspace data dir.
#[derive(Debug)]
pub struct HeadlessPlatform {
    data_dir: PathBuf,
}

impl HeadlessPlatform {
    /// Creates a headless platform rooted at `data_dir`.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    /// Creates a headless platform at the default data directory.
    pub fn with_default_dir() -> Self {
        Self::new(crate::config::default_data_dir())
    }
}

impl Default for HeadlessPlatform {
    fn default() -> Self {
        Self::with_default_dir()
    }
}

impl Platform for HeadlessPlatform {
    fn headless(&self) -> bool {
        true
    }

    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn log(&self, level: LogLevel, msg: &str) {
        match level {
            LogLevel::Debug => log::debug!("{msg}"),
            LogLevel::Info => log::info!("{msg}"),
            LogLevel::Warn => log::warn!("{msg}"),
            LogLevel::Error => log::error!("{msg}"),
        }
    }

    fn exit(&self) {}

    fn millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_platform_reports_headless_and_data_dir() {
        let platform = HeadlessPlatform::new("/tmp/mind-test");
        assert!(platform.headless());
        assert_eq!(platform.data_dir(), Path::new("/tmp/mind-test"));
        platform.log(LogLevel::Info, "hello");
        let _ = platform.millis();
        platform.exit();
    }

    /// T5: `Time.millis` is wall-clock epoch milliseconds (`Time.java:122-124`),
    /// the value `SaveVersion.writeMeta` stores as `saved` — not a monotonic
    /// host-start counter (which made every new save sort to 1970).
    #[test]
    fn platform_millis_is_epoch_milliseconds() {
        let floor = 1_700_000_000_000u64; // 2023-11-14; any real clock is past it.
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_millis() as u64;
        let platform = HeadlessPlatform::new("/tmp/mind-test");
        let value = platform.millis();
        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_millis() as u64;
        assert!(
            value >= before.max(floor) && value <= after,
            "millis {value} is wall-clock epoch time (before {before}, after {after})"
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-only

//! Logging facade.
//!
//! Ported from `arc.util.Log` plus `core/src/mindustry/Vars.java`
//! (`loadLogger`/`loadFileLogger`): `[D]/[I]/[W]/[E]` prefixes, console output
//! plus an optional `last_log.txt`. Installation is idempotent (`OnceLock`).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// Logger installation settings.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Maximum level that is emitted (`Error..=Trace`).
    pub level: LevelFilter,
    /// Optional log file (`<data-dir>/last_log.txt` by default in hosts).
    pub file: Option<PathBuf>,
    /// Whether to emit ANSI colors (hosts set this only for TTYs).
    pub color: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: LevelFilter::Info,
            file: None,
            color: false,
        }
    }
}

/// Errors raised while installing the logger.
#[derive(thiserror::Error, Debug)]
pub enum LogError {
    /// The log file could not be opened/created.
    #[error("failed to open log file `{path}`: {source}")]
    File {
        /// Path that failed.
        path: PathBuf,
        /// Underlying IO error.
        #[source]
        source: std::io::Error,
    },
    /// A logger from another crate was installed first.
    #[error("a logger is already installed by another crate")]
    AlreadySet,
}

struct MindLogger {
    level: LevelFilter,
    color: bool,
    file: Mutex<Option<File>>,
}

impl Log for MindLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let tag = level_tag(record.level());
        let message = record.args().to_string();
        let rendered = if self.color {
            let code = level_color(record.level());
            format!("\u{1b}[{code}m[{tag}]\u{1b}[0m {message}")
        } else {
            format!("[{tag}] {message}")
        };

        {
            let stderr = std::io::stderr();
            let mut lock = stderr.lock();
            let _ = writeln!(lock, "{rendered}");
        }

        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
        {
            let _ = writeln!(file, "[{tag}] {message}");
        }
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
        {
            let _ = file.flush();
        }
    }
}

fn level_tag(level: Level) -> &'static str {
    match level {
        Level::Error => "E",
        Level::Warn => "W",
        Level::Info => "I",
        Level::Debug | Level::Trace => "D",
    }
}

fn level_color(level: Level) -> &'static str {
    // Ported from Vars.loadLogger tags: [green][D], [royal][I], [yellow][W], [scarlet][E].
    match level {
        Level::Error => "31",
        Level::Warn => "33",
        Level::Info => "34",
        Level::Debug | Level::Trace => "32",
    }
}

static LOGGER: OnceLock<MindLogger> = OnceLock::new();

/// Installs [`MindLogger`] exactly once. Later calls are no-ops and return `Ok`.
///
/// Ported from `Vars.loadLogger`/`loadFileLogger` (guarded by `loadedLogger`).
pub fn init(config: LogConfig) -> Result<(), LogError> {
    if LOGGER.get().is_some() {
        return Ok(());
    }

    let file = match &config.file {
        Some(path) => Some(open_log_file(path)?),
        None => None,
    };
    let logger = MindLogger {
        level: config.level,
        color: config.color,
        file: Mutex::new(file),
    };

    if LOGGER.set(logger).is_err() {
        return Ok(());
    }
    let installed = LOGGER.get().ok_or(LogError::AlreadySet)?;
    log::set_logger(installed).map_err(|_| LogError::AlreadySet)?;
    log::set_max_level(config.level);
    Ok(())
}

/// Opens (creating parents if needed) a log file in append mode.
pub fn open_log_file(path: &Path) -> Result<File, LogError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|source| LogError::File {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| LogError::File {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_are_idempotent_and_file_receives_lines() {
        let path =
            std::env::temp_dir().join(format!("mind-core-log-test-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);

        // First install wins; a later call must stay a no-op even with a different config.
        init(LogConfig {
            level: LevelFilter::Info,
            file: Some(path.clone()),
            color: false,
        })
        .unwrap();
        init(LogConfig::default()).unwrap();

        log::info!("log facade test line");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains("[I] log facade test line"),
            "unexpected log contents: {contents}"
        );
        let _ = std::fs::remove_file(&path);
    }
}

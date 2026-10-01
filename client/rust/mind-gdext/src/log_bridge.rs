// SPDX-License-Identifier: GPL-3.0-only

//! `log` → Godot console + `user://last_log.txt` bridge and panic hook for the
//! GDExtension.
//!
//! Records are forwarded to Godot's console (`print`/`warn`/`error`, which also
//! reach stdout/stderr) with the Mindustry `[D]/[I]/[W]/[E]` prefixes, and
//! appended to `user://last_log.txt` (plan §3.6; the file half deferred at M1).
//! Console and file share the same line format; colors stay off (not a TTY).

use std::fs::File;
use std::panic::{self, PanicHookInfo};
use std::sync::{Mutex, Once};

use godot::classes::ProjectSettings;
use godot::global::{godot_error, godot_print, godot_warn};
use godot::obj::Singleton;
use log::{Level, LevelFilter, Log, Metadata, Record};

/// In-engine log file, relative to Godot's `user://` (plan §3.6/§6.5).
const LOG_PATH: &str = "user://last_log.txt";

struct GodotLogger {
    file: Mutex<Option<File>>,
}

impl Log for GodotLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let tag = match record.level() {
            Level::Error => "E",
            Level::Warn => "W",
            Level::Info => "I",
            Level::Debug | Level::Trace => "D",
        };
        let message = format!("[{tag}] {}", record.args());

        match record.level() {
            Level::Error => godot_error!("{message}"),
            Level::Warn => godot_warn!("{message}"),
            _ => godot_print!("{message}"),
        }

        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
        {
            use std::io::Write as _;
            let _ = writeln!(file, "{message}");
            let _ = file.flush();
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
        {
            use std::io::Write as _;
            let _ = file.flush();
        }
    }
}

static INSTALL: Once = Once::new();

/// Installs the logger and panic hook exactly once per process (hot-reload safe).
pub fn install() {
    INSTALL.call_once(|| {
        // `user://` resolves through the engine; the M1 console bridge stays the
        // source of truth, the file is the deferred §3.6 half.
        let file = {
            let path = ProjectSettings::singleton()
                .globalize_path(LOG_PATH)
                .to_string();
            if path.is_empty() {
                None
            } else {
                mind_core::log::open_log_file(std::path::Path::new(&path)).ok()
            }
        };
        if log::set_boxed_logger(Box::new(GodotLogger {
            file: Mutex::new(file),
        }))
        .is_ok()
        {
            log::set_max_level(LevelFilter::Debug);
        }

        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
            log::error!("panic before unwinding into Godot: {info}");
            previous(info);
        }));
    });
}

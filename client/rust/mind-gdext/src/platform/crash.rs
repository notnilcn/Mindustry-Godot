// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Crash-handler host glue (plan 22 §3.4).
//!
//! The report builder, likely-mod detection and `crashes/` naming live in
//! `mind-core` (Godot-free, unit-tested). This module gathers the Godot host
//! facts (OS, engine version, memory, cores), writes the report under the data
//! root, and chains the panic hook so a crash never loses `last_log.txt`.

use std::path::PathBuf;
use std::sync::Once;

use godot::classes::{Engine, Os, Time};
use godot::obj::Singleton;
use godot::prelude::GString;
use mind_core::crash::{self as core_crash, CrashContext, CrashMod};
use mind_core::version::BuildInfo;

/// Issue tracker printed on vanilla crashes of stamped builds (`Vars.reportIssueURL`).
pub const REPORT_ISSUE_URL: &str = "https://github.com/Anuken/Mindustry/issues";

static INSTALL: Once = Once::new();

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn dict_i64(dict: &godot::builtin::VarDictionary, key: &str) -> i64 {
    dict.get(key)
        .and_then(|value| value.try_to::<i64>().ok())
        .unwrap_or(0)
}

/// Computes `(file_stamp, display_date)` from the system clock.
///
/// `file_stamp` is `MM_dd_yyyy_HH_mm_ss` (report file name) and `display_date`
/// mirrors upstream's `"MMMM d, yyyy HH:mm:ss a"` report line.
fn datetime() -> (String, String) {
    let dict = Time::singleton().get_datetime_dict_from_system();
    let year = dict_i64(&dict, "year");
    let month = dict_i64(&dict, "month").clamp(1, 12) as usize;
    let day = dict_i64(&dict, "day");
    let hour = dict_i64(&dict, "hour");
    let minute = dict_i64(&dict, "minute");
    let second = dict_i64(&dict, "second");
    let suffix = if hour < 12 { "AM" } else { "PM" };
    let hour12 = match hour % 12 {
        0 => 12,
        other => other,
    };
    (
        format!("{month:02}_{day:02}_{year:04}_{hour:02}_{minute:02}_{second:02}"),
        format!(
            "{} {day}, {year:04} {hour12:02}:{minute:02}:{second:02} {suffix}",
            MONTHS[month - 1]
        ),
    )
}

/// Gathers the host report context.
fn context() -> CrashContext {
    let os = Os::singleton();
    let engine = Engine::singleton();
    let version = engine.get_version_info();
    let runtime_version = version
        .get("string")
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .unwrap_or_else(|| "Godot".to_owned());
    let memory = os.get_memory_info();
    let max_memory_mb = (dict_i64(&memory, "available").max(0) as u64) / 1024 / 1024;
    CrashContext {
        os_name: os.get_name().to_string(),
        os_arch: std::env::consts::ARCH.to_owned(),
        os_arch_bits: Some((std::mem::size_of::<usize>() * 8) as u32),
        gl_version: None,
        runtime_version,
        max_memory_mb,
        cores: os.get_processor_count().max(0) as u32,
        headless: os.has_feature("server"),
        report_issue_url: REPORT_ISSUE_URL.to_owned(),
    }
}

/// Writes a crash report for `exception` with `mods` and returns its path.
///
/// Plan 20 supplies `mods` (converted from its `LoadedMod` registry) so
/// likely-mod detection can run; an empty slice is valid.
pub fn write_report(exception: &str, mods: &[CrashMod]) -> Option<PathBuf> {
    let (stamp, date) = datetime();
    let report = core_crash::create_report(
        exception,
        &[],
        mods,
        BuildInfo::embedded(),
        &date,
        &context(),
    );
    let root = super::desktop::data_root(None);
    match core_crash::write_crash_report(&root, &report, &stamp) {
        Ok(path) => {
            log::error!("[crash] report written to {}", path.display());
            Some(path)
        }
        Err(error) => {
            log::error!("[crash] could not write crash report: {error}");
            None
        }
    }
}

/// Installs a panic hook that writes a crash report, chaining the previous hook
/// (the log bridge) so `last_log.txt` is flushed first.
pub fn install() {
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                write_report(&info.to_string(), &[]);
            }));
            previous(info);
        }));
    });
}

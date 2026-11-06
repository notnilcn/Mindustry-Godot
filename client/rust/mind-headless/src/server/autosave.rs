// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server autosave naming and rotation (plan 22 §3.6/§6.5).
//!
//! Port of `ServerControl`'s autosave block: `auto_<map>_<MM-dd-yyyy_HH-mm-ss>.msav`,
//! keep the newest `autosaveAmount`, spacing in seconds (`* 60` ticks at 60 Hz).

use std::path::{Path, PathBuf};

/// Autosave filename prefix (`f.name().startsWith("auto_")`).
pub const AUTOSAVE_PREFIX: &str = "auto_";

/// `state.map.file.nameWithoutExtension().replace(" ", "_")`, hardened to strip
/// path separators and other unsafe filename characters.
pub fn sanitize_map(map: &str) -> String {
    let mut out = String::with_capacity(map.len());
    for c in map.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "unknown".to_owned()
    } else {
        out
    }
}

/// `auto_<map>_<timestamp>.msav`.
pub fn autosave_name(map: &str, timestamp: &str) -> String {
    format!("{AUTOSAVE_PREFIX}{}_{timestamp}.msav", sanitize_map(map))
}

/// `<saves_dir>/auto_<map>_<timestamp>.msav`.
pub fn autosave_path(saves_dir: &Path, map: &str, timestamp: &str) -> PathBuf {
    saves_dir.join(autosave_name(map, timestamp))
}

/// Seconds since the Unix epoch (system clock).
pub fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// Formats a Unix time as Java's `MM-dd-yyyy_HH-mm-ss`.
///
/// The dedicated server has no timezone database (and no chrono dependency);
/// the timestamp is UTC so two autosaves can never reorder across DST. This is
/// a documented deviation from `LocalDateTime.now()` (Java server local time).
pub fn timestamp_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;
    format!("{month:02}-{day:02}-{year:04}_{hour:02}-{minute:02}-{second:02}")
}

/// The current autosave timestamp (`autosaveDate.format(LocalDateTime.now())`).
pub fn timestamp_now() -> String {
    timestamp_from_unix(now_unix_secs())
}

/// Howard Hinnant's `civil_from_days` (proleptic Gregorian, days since epoch).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = i64::from(yoe) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year + i64::from(month <= 2);
    (year, month, day)
}

/// Seconds between autosaves → ticks at the fixed 60 Hz rate.
pub fn spacing_ticks(seconds: u64) -> u64 {
    seconds.saturating_mul(crate::server::TICKS_PER_SECOND)
}

/// Whether an autosave is due (`Interval.get(spacing)`).
pub fn should_autosave(now_tick: u64, last_tick: u64, spacing_ticks: u64) -> bool {
    spacing_ticks > 0 && now_tick.saturating_sub(last_tick) >= spacing_ticks
}

/// Parses the trailing `MM-dd-yyyy_HH-mm-ss` timestamp of an autosave name.
pub fn parse_timestamp(name: &str) -> Option<(i32, u32, u32, u32, u32, u32)> {
    let stem = name.strip_suffix(".msav").unwrap_or(name);
    // Format: `<map>_<MM-dd-yyyy>_<HH-mm-ss>`.
    let parts: Vec<&str> = stem.rsplitn(3, '_').collect();
    if parts.len() < 3 {
        return None;
    }
    let time = parts[0];
    let date = parts[1];
    let date_parts: Vec<&str> = date.split('-').collect();
    let time_parts: Vec<&str> = time.split('-').collect();
    if date_parts.len() != 3 || time_parts.len() != 3 {
        return None;
    }
    Some((
        date_parts[2].parse().ok()?,
        date_parts[0].parse().ok()?,
        date_parts[1].parse().ok()?,
        time_parts[0].parse().ok()?,
        time_parts[1].parse().ok()?,
        time_parts[2].parse().ok()?,
    ))
}

/// Autosave files in `dir`, oldest first (by parsed timestamp, then mtime, then
/// name for a deterministic tie-break).
pub fn rotation_order(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(AUTOSAVE_PREFIX))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort_by(|a, b| {
        let name_a = a.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let name_b = b.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let mtime_a = modified(a);
        let mtime_b = modified(b);
        parse_timestamp(name_a)
            .cmp(&parse_timestamp(name_b))
            .then_with(|| mtime_a.cmp(&mtime_b))
            .then_with(|| name_a.cmp(name_b))
    });
    files
}

/// File modification time (Java `Fi.lastModified`; epoch 0 when unavailable).
fn modified(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

/// Deletes the oldest autosaves until at most `keep` remain; returns the
/// deleted paths (oldest first).
pub fn rotate(dir: &Path, keep: usize) -> Vec<PathBuf> {
    let files = rotation_order(dir);
    let mut deleted = Vec::new();
    if files.len() > keep {
        for path in &files[..files.len() - keep] {
            if std::fs::remove_file(path).is_ok() {
                deleted.push(path.clone());
            }
        }
    }
    deleted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mind-srv-auto-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create");
        dir
    }

    fn write_autosave(dir: &Path, map: &str, timestamp: &str, age_secs: u64) -> PathBuf {
        let path = autosave_path(dir, map, timestamp);
        std::fs::write(&path, b"x").expect("write");
        if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path) {
            let modified = SystemTime::now() - Duration::from_secs(age_secs);
            let times = std::fs::FileTimes::new().set_modified(modified);
            let _ = file.set_times(times);
        }
        path
    }

    #[test]
    fn filename_sanitizes_map() {
        assert_eq!(sanitize_map("ground Zero"), "ground_Zero");
        assert_eq!(sanitize_map("../evil/name"), ".._evil_name");
        assert_eq!(sanitize_map(""), "unknown");
        assert_eq!(
            autosave_name("ground Zero", "10-02-2026_12-00-00"),
            "auto_ground_Zero_10-02-2026_12-00-00.msav"
        );
        assert_eq!(
            parse_timestamp("auto_ground_Zero_10-02-2026_12-00-00.msav"),
            Some((2026, 10, 2, 12, 0, 0))
        );
    }

    #[test]
    fn keep_amount_rotates_oldest() {
        let dir = temp_dir("rotate");
        // Distinct mtimes: a oldest, d newest.
        let a = write_autosave(&dir, "map", "01-01-2026_00-00-00", 300);
        let b = write_autosave(&dir, "map", "01-01-2026_00-00-01", 200);
        let c = write_autosave(&dir, "map", "01-01-2026_00-00-02", 100);
        let d = write_autosave(&dir, "map", "01-01-2026_00-00-03", 0);

        let deleted = rotate(&dir, 2);
        assert_eq!(deleted, vec![a, b]);
        let remaining = rotation_order(&dir);
        assert_eq!(remaining, vec![c, d]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timestamp_matches_the_java_pattern() {
        // 2026-10-02T12:00:00Z.
        assert_eq!(timestamp_from_unix(1_790_942_400), "10-02-2026_12-00-00");
        assert_eq!(timestamp_from_unix(0), "01-01-1970_00-00-00");
        assert_eq!(timestamp_from_unix(1_790_942_459), "10-02-2026_12-00-59");
        // Leap day.
        assert_eq!(timestamp_from_unix(1_709_164_800), "02-29-2024_00-00-00");
        let name = autosave_name("ground Zero", &timestamp_from_unix(1_790_942_400));
        assert_eq!(name, "auto_ground_Zero_10-02-2026_12-00-00.msav");
        assert!(parse_timestamp(&name).is_some());
    }

    #[test]
    fn rotation_order_uses_mtime_for_equal_timestamps() {
        let dir = temp_dir("mtime");
        let older = write_autosave(&dir, "map", "01-01-2026_00-00-00", 200);
        let newer = write_autosave(&dir, "other", "01-01-2026_00-00-00", 10);
        assert_eq!(rotation_order(&dir), vec![older.clone(), newer]);
        assert_eq!(rotate(&dir, 1), vec![older.clone()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spacing_ticks_math() {
        assert_eq!(spacing_ticks(300), 18_000);
        assert!(should_autosave(18_000, 0, 18_000));
        assert!(!should_autosave(17_999, 0, 18_000));
    }
}

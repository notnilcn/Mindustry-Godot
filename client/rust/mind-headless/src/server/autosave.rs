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
        parse_timestamp(name_a)
            .cmp(&parse_timestamp(name_b))
            .then_with(|| name_a.cmp(name_b))
    });
    files
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
    fn spacing_ticks_math() {
        assert_eq!(spacing_ticks(300), 18_000);
        assert!(should_autosave(18_000, 0, 18_000));
        assert!(!should_autosave(17_999, 0, 18_000));
    }
}

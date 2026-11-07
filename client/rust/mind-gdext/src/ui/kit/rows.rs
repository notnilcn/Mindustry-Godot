// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/service/Achievement.java,
//         core/src/mindustry/service/SStat.java,
//         desktop/src/mindustry/desktop/DesktopLauncher.java (the anonymous
//         `GameService` the achievement dialog queried).

//! Achievement/statistics rows for the Database dialog (GAPS #68 UI half).
//!
//! Upstream removed the achievement dialog; only the Steam overlay showed the
//! 90 `Achievement`/21 `SStat` entries. This port adds two Database tabs
//! (`achievements`, `statistics`) over the same tables:
//!
//! - **live rows** come from `MindCampaign.database_rows_json(kind)` (the gdext
//!   query API, guarded by `has_method`) or from the `achievements`/
//!   `statistics` arrays of `MindCampaign.database_json()`;
//! - **fallback rows** are read from the persisted settings store with
//!   [`LocalGameService`] (`achievement-<name>`/`achievement-stat-<name>`), the
//!   same keys the campaign runtime saves. That is real state, never a fixture:
//!   an untouched profile shows every achievement locked with zero progress.
//!
//! Row shape (append-only, the dialog and any future platform overlay read it):
//!
//! ```json
//! {"name":"unitsDestroyed","stat":"unitsDestroyed","goal":10,
//!  "progress":3,"unlocked":false,"description":"achievement.x.description"}
//! {"name":"unitsDestroyed","value":3,"max":false}
//! ```

use mind_core::game::achievements::SStat;
use mind_core::game::service::LocalGameService;

/// One achievement row for the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AchievementRow {
    /// Upstream enum name (`Achievement.name()`).
    pub name: String,
    /// Driving stat name, or `None` for a pure event achievement.
    pub stat: Option<String>,
    /// Goal value (`Achievement.statGoal`; `0` for event-only rows).
    pub goal: i32,
    /// Stat reading clamped to the goal (`Achievement.progress`).
    pub progress: i32,
    /// Whether the achievement is unlocked.
    pub unlocked: bool,
}

/// One statistics-table row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatRow {
    /// Upstream enum name (`SStat.name()`).
    pub name: String,
    /// Current value.
    pub value: i32,
    /// `SStat.isMax()` (a high-water mark, not a counter).
    pub max: bool,
}

/// Achievements from the local service, in declaration order.
pub fn achievement_rows_from_service(service: &LocalGameService) -> Vec<serde_json::Value> {
    service
        .achievement_rows()
        .into_iter()
        .map(|row| {
            let stat = row.achievement.stat();
            serde_json::json!({
                "name": row.achievement.name(),
                "stat": stat.map(SStat::name),
                "goal": row.achievement.goal(),
                "progress": row.progress,
                "unlocked": row.unlocked,
                "description": row.achievement.description_key(),
            })
        })
        .collect()
}

/// Statistics from the local service, in declaration order.
pub fn stat_rows_from_service(service: &LocalGameService) -> Vec<serde_json::Value> {
    service
        .stat_rows()
        .into_iter()
        .map(|(stat, value)| {
            serde_json::json!({
                "name": stat.name(),
                "value": value,
                "max": stat.is_max(),
            })
        })
        .collect()
}

/// Parses achievement rows (live or service shape) into the display model.
pub fn parse_achievements(json: &str) -> Vec<AchievementRow> {
    let Ok(serde_json::Value::Array(rows)) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let name = row.get("name")?.as_str()?;
            if name.is_empty() {
                return None;
            }
            Some(AchievementRow {
                name: name.to_owned(),
                stat: row
                    .get("stat")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                goal: row
                    .get("goal")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0) as i32,
                progress: row
                    .get("progress")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0) as i32,
                unlocked: row
                    .get("unlocked")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

/// Parses statistics rows (live or service shape) into the display model.
pub fn parse_stats(json: &str) -> Vec<StatRow> {
    let Ok(serde_json::Value::Array(rows)) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let name = row.get("name")?.as_str()?;
            if name.is_empty() {
                return None;
            }
            Some(StatRow {
                name: name.to_owned(),
                value: row
                    .get("value")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0) as i32,
                max: row
                    .get("max")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

/// `(unlocked, total)` achievement counts for the tab progress line
/// (`LocalGameService::completion`).
pub fn completion(rows: &[AchievementRow]) -> (usize, usize) {
    (rows.iter().filter(|row| row.unlocked).count(), rows.len())
}

/// The progress text: `"unlocked"`, `"3/10"`, or `"—"` for event-only rows.
pub fn progress_text(row: &AchievementRow) -> String {
    if row.unlocked {
        return String::from("unlocked");
    }
    match row.stat {
        Some(_) if row.goal > 0 => format!("{}/{}", row.progress.max(0), row.goal),
        _ => String::from("—"),
    }
}

/// The row label colour tag: `accent` when unlocked, `gray` when locked
/// (Java's `DatabaseDialog` locked-content `Pal.gray` treatment).
pub fn row_color(row: &AchievementRow) -> &'static str {
    if row.unlocked { "accent" } else { "gray" }
}

/// The statistics-table value text (`SStat` high-water marks are labelled).
pub fn stat_value_text(row: &StatRow) -> String {
    if row.max {
        format!("{} (max)", row.value)
    } else {
        row.value.to_string()
    }
}

/// Resolves the upstream table rows for a database tab kind
/// (`"achievements"`/`"statistics"`).
pub fn rows_for_kind(service: &LocalGameService, kind: &str) -> Option<Vec<serde_json::Value>> {
    match kind {
        "achievements" | "achievement" => Some(achievement_rows_from_service(service)),
        "statistics" | "stats" | "stat" => Some(stat_rows_from_service(service)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use mind_core::content::settings_store::{MemoryUnlockStore, UnlockStore};
    use mind_core::game::achievements::Achievement;
    use mind_core::game::achievements::achievement_key;

    fn json(rows: &[serde_json::Value]) -> String {
        serde_json::to_string(rows).unwrap_or_else(|_| String::from("[]"))
    }

    #[test]
    fn service_rows_cover_every_upstream_entry_in_order() {
        let service = LocalGameService::new();
        let achievements = achievement_rows_from_service(&service);
        assert_eq!(achievements.len(), Achievement::COUNT);
        assert_eq!(achievements[0]["name"], Achievement::ALL[0].name());
        let stats = stat_rows_from_service(&service);
        assert_eq!(stats.len(), SStat::COUNT);
        assert_eq!(stats[0]["name"], SStat::ALL[0].name());
        // A fresh profile is locked with zero progress — never a fixture.
        let parsed = parse_achievements(&json(&achievements));
        assert_eq!(parsed.len(), Achievement::COUNT);
        assert!(parsed.iter().all(|row| !row.unlocked));
        assert_eq!(completion(&parsed), (0, Achievement::COUNT));

        // Persisted unlocks/stat values surface through the same read.
        let mut store = MemoryUnlockStore::new();
        let mut service = LocalGameService::new();
        service.unlock_by_name(Achievement::ALL[0].name());
        let stat = SStat::ALL[0];
        service.write_stat(stat, 7);
        service.save(&mut store);
        assert!(store.get_bool(&achievement_key(Achievement::ALL[0].name())));
        let mut loaded = LocalGameService::new();
        loaded.load(&store);
        let parsed = parse_achievements(&json(&achievement_rows_from_service(&loaded)));
        assert!(parsed[0].unlocked);
        assert_eq!(completion(&parsed).0, 1);
        let stats = parse_stats(&json(&stat_rows_from_service(&loaded)));
        assert_eq!(stats[0].value, 7);
    }

    #[test]
    fn parses_live_and_service_shapes_and_skips_malformed_rows() {
        let rows = r#"[
            {"name":"unitsDestroyed","stat":"unitsDestroyed","goal":10,"progress":3,"unlocked":false},
            {"name":"attacksWon","unlocked":true},
            {"name":"","unlocked":true},
            {"goal":4}
        ]"#;
        let parsed = parse_achievements(rows);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].stat.as_deref(), Some("unitsDestroyed"));
        assert_eq!(parsed[0].goal, 10);
        assert_eq!(parsed[0].progress, 3);
        assert!(parsed[1].unlocked);
        assert_eq!(parsed[1].goal, 0);
        assert!(parse_achievements("{}").is_empty());
        assert!(parse_achievements("nope").is_empty());

        let stats = parse_stats(
            r#"[{"name":"timesLaunched","value":4,"max":false},
                {"name":"maxUnitActive","value":9,"max":true},
                {"value":2}]"#,
        );
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[1].name, "maxUnitActive");
        assert!(stats[1].max);
    }

    #[test]
    fn progress_and_colour_follow_the_unlock_state() {
        let locked = AchievementRow {
            name: String::from("unitsDestroyed"),
            stat: Some(String::from("unitsDestroyed")),
            goal: 10,
            progress: 3,
            unlocked: false,
        };
        assert_eq!(progress_text(&locked), "3/10");
        assert_eq!(row_color(&locked), "gray");
        let done = AchievementRow {
            unlocked: true,
            ..locked.clone()
        };
        assert_eq!(progress_text(&done), "unlocked");
        assert_eq!(row_color(&done), "accent");
        // Event-only achievements have no stat predicate.
        let event = AchievementRow {
            stat: None,
            goal: 0,
            ..locked
        };
        assert_eq!(progress_text(&event), "—");
        // Progress never renders negative after a settings reset.
        let reset = AchievementRow {
            progress: -1,
            ..event
        };
        assert_eq!(progress_text(&reset), "—");
    }

    #[test]
    fn stat_values_label_high_water_marks() {
        let counter = StatRow {
            name: String::from("unitsDestroyed"),
            value: 12,
            max: false,
        };
        assert_eq!(stat_value_text(&counter), "12");
        let peak = StatRow {
            name: String::from("maxUnitActive"),
            value: 9,
            max: true,
        };
        assert_eq!(stat_value_text(&peak), "9 (max)");
    }

    #[test]
    fn rows_for_kind_covers_both_tabs() {
        let service = LocalGameService::new();
        assert_eq!(
            rows_for_kind(&service, "achievements")
                .expect("achievements")
                .len(),
            Achievement::COUNT
        );
        assert_eq!(
            rows_for_kind(&service, "statistics")
                .expect("statistics")
                .len(),
            SStat::COUNT
        );
        assert!(rows_for_kind(&service, "planets").is_none());
    }
}

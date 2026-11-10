// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Vars.service` facade: the shared achievement/stat tracker and its JSON rows.
//! Upstream: core/src/mindustry/service/GameService.java; game/Achievement.java; game/SStat.java

use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// The achievement rows as JSON (`achievement_rows`), for the achievements
    /// table. Reads the live runtime's tracker when a sector is installed.
    #[func]
    pub fn service_achievement_rows_json(&mut self) -> GString {
        self.sync_service();
        GString::from(service_achievement_rows_json(self.service.service()).as_str())
    }

    /// The `SStat` rows as JSON (`stat_rows`), for the stats table.
    #[func]
    pub fn service_stat_rows_json(&mut self) -> GString {
        self.sync_service();
        GString::from(service_stat_rows_json(self.service.service()).as_str())
    }

    /// `(unlocked, total)` achievement completion for a UI progress line
    /// (`LocalGameService::completion`), as `{"unlocked": n, "total": m}`.
    #[func]
    pub fn service_completion_json(&mut self) -> GString {
        self.sync_service();
        let (unlocked, total) = self.service.service().completion();
        value_gstring(&serde_json::json!({ "unlocked": unlocked, "total": total }))
    }

    /// The unlocked achievement names as JSON (`KEY_ACHIEVEMENTS` payload;
    /// `LocalGameService::unlocked_json`).
    #[func]
    pub fn service_unlocked_json(&mut self) -> GString {
        self.sync_service();
        value_gstring(&self.service.service().unlocked_json())
    }
}

impl MindCampaign {
    /// Pulls the live runtime's achievement/stat tracker back into the facade.
    ///
    /// The runtime copy is authoritative while a sector runs (Java has one
    /// global `Vars.service`; the port's `CampaignRuntime` owns the listener
    /// state the campaign systems mutate), so every facade read/persist
    /// mirrors it first. No-op without an installed runtime.
    pub(super) fn sync_service(&mut self) {
        let Some(host) = self.host() else {
            return;
        };
        let guard = host.bind();
        if let Some(tracker) = guard.campaign_service() {
            self.service = tracker.clone();
        }
    }

    /// Writes the shared tracker through `Core.settings` (`GameService.storeStats`
    /// half: the values land in the settings store; the caller flushes it).
    pub(super) fn persist_service(&mut self) {
        self.sync_service();
        self.store_service();
    }

    /// The store half of [`Self::persist_service`]: the tracker's stats and
    /// achievements land in the shared settings store (no runtime sync, no
    /// flush — the caller owns both).
    pub(super) fn store_service(&mut self) {
        settings_hub::with_settings(|settings| {
            let mut store = SettingsUnlockStore::new(settings);
            save_service(&mut self.service, &mut store);
        });
    }
}

/// Loads the shared achievement/stat tracker from a settings-backed store.
///
/// `DesktopLauncher`'s anonymous `GameService` keeps `Vars.service` alive for
/// the whole process and reads the persisted `achievement-<name>` /
/// `achievement-stat-<name>` keys on boot ([`ServiceTracker::load`]). The
/// facade owns that one tracker and injects it into every live
/// [`CampaignRuntime`], so sector switches never restart the counters.
pub(super) fn load_service(store: &dyn UnlockStore) -> ServiceTracker {
    let mut tracker = ServiceTracker::new();
    tracker.load(store);
    tracker
}

/// Writes the shared tracker's stats/achievements through a settings-backed
/// store (`GameService.storeStats`). The caller flushes `Core.settings`.
pub(super) fn save_service(tracker: &mut ServiceTracker, store: &mut dyn UnlockStore) {
    tracker.save(store);
}

/// `Achievement` rows as JSON for the achievements table: declaration order,
/// `{name, unlocked, progress, goal, stat}` (`LocalGameService::achievement_rows`;
/// the extra two fields are the stat/goal the row's progress is clamped to).
fn service_achievement_rows_json(service: &mind_core::game::service::LocalGameService) -> String {
    let rows: Vec<serde_json::Value> = service
        .achievement_rows()
        .into_iter()
        .map(|row: AchievementRow| {
            serde_json::json!({
                "name": row.achievement.name(),
                "unlocked": row.unlocked,
                "progress": row.progress,
                "goal": row.achievement.goal(),
                "stat": row.achievement.stat().map(|stat| stat.name()),
            })
        })
        .collect();
    serde_json::Value::Array(rows).to_string()
}

/// `SStat` rows as JSON for the stats table: declaration order,
/// `{name, value}` (`LocalGameService::stat_rows`).
fn service_stat_rows_json(service: &mind_core::game::service::LocalGameService) -> String {
    let rows: Vec<serde_json::Value> = service
        .stat_rows()
        .into_iter()
        .map(|(stat, value)| serde_json::json!({ "name": stat.name(), "value": value }))
        .collect();
    serde_json::Value::Array(rows).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::content::MemoryUnlockStore;
    use mind_core::content::create_base_content;
    use mind_core::game::achievements::Achievement;
    use mind_core::game::achievements::SStat;

    /// GAPS #68 host wiring: the shared `Vars.service` tracker (a) boots from
    /// the persisted settings keys, (b) is the exact tracker a fresh
    /// `CampaignRuntime` runs on (`with_service`), and (c) round-trips the
    /// values the campaign systems wrote through the save/load helpers the
    /// autosave/exit paths call.
    #[test]
    fn service_injection_roundtrips_through_the_unlock_store() {
        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);

        // A previous session persisted a stat and an achievement under the
        // module ABI keys (`achievement-*` / `achievement-stat-*`).
        let mut boot_store = MemoryUnlockStore::new();
        boot_store.set_i32("achievement-stat-unitsDestroyed", 1_234);
        boot_store.set_bool("achievement-buildT5", true);

        // Boot: `load_service` restores the tracker from `Core.settings`.
        let boot = load_service(&boot_store);
        assert_eq!(boot.service().get(SStat::UnitsDestroyed), 1_234);
        assert!(boot.service().unlocked(Achievement::BuildT5));

        // A default runtime starts empty; the facade injects the shared one.
        let untouched = CampaignRuntime::new(PlaySession::default(), campaign.clone())
            .with_service(ServiceTracker::new());
        assert_eq!(untouched.service.service().get(SStat::UnitsDestroyed), 0);
        let mut runtime =
            CampaignRuntime::new(PlaySession::default(), campaign).with_service(boot.clone());
        assert_eq!(runtime.service.service().get(SStat::UnitsDestroyed), 1_234);
        assert!(runtime.service.service().unlocked(Achievement::BuildT5));

        // The campaign listener half keeps counting on the injected tracker
        // (the runtime systems call these hooks; the direct call is the same
        // seam without a sim tick).
        runtime.service.on_sector_launch();
        assert_eq!(runtime.service.service().get(SStat::TimesLaunched), 1);
        // The facade mirrors the runtime's copy every pump/persist.
        let mirrored = runtime.service.clone();
        assert_eq!(mirrored.service().get(SStat::TimesLaunched), 1);

        // Autosave/exit: the mirrored tracker saves through the store...
        let mut saved = MemoryUnlockStore::new();
        let mut tracker = mirrored;
        save_service(&mut tracker, &mut saved);
        assert_eq!(saved.get_i32("achievement-stat-timesLaunched"), 1);
        assert_eq!(saved.get_i32("achievement-stat-unitsDestroyed"), 1_234);
        assert!(saved.get_bool("achievement-buildT5"));

        // ...and the next boot sees the incremented stat plus the old values.
        let reloaded = load_service(&saved);
        assert_eq!(reloaded.service().get(SStat::TimesLaunched), 1);
        assert_eq!(reloaded.service().get(SStat::UnitsDestroyed), 1_234);
        assert!(reloaded.service().unlocked(Achievement::BuildT5));
    }

    /// The `#[func]` row payloads (`service_stat_rows_json` /
    /// `service_achievement_rows_json`) keep Java declaration order and expose
    /// the values/goals the UI tables render.
    #[test]
    fn service_rows_json_cover_every_stat_and_achievement() {
        let mut tracker = ServiceTracker::new();
        tracker.service_mut().set(SStat::BlocksBuilt, 2_500);
        tracker.service_mut().unlock(Achievement::Place10kBlocks);

        let stats: serde_json::Value =
            match serde_json::from_str(&service_stat_rows_json(tracker.service())) {
                Ok(value) => value,
                Err(error) => panic!("stat rows JSON: {error}"),
            };
        let rows = stats.as_array().map(Vec::as_slice).unwrap_or_default();
        assert_eq!(rows.len(), SStat::COUNT);
        assert_eq!(rows[0]["name"], "unitsDestroyed");
        let blocks = rows
            .iter()
            .find(|row| row["name"] == "blocksBuilt")
            .unwrap_or_else(|| panic!("blocksBuilt row"));
        assert_eq!(blocks["value"], 2_500);

        let achievements: serde_json::Value =
            match serde_json::from_str(&service_achievement_rows_json(tracker.service())) {
                Ok(value) => value,
                Err(error) => panic!("achievement rows JSON: {error}"),
            };
        let rows = achievements
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        assert_eq!(rows.len(), Achievement::COUNT);
        let placed = rows
            .iter()
            .find(|row| row["name"] == "place10kBlocks")
            .unwrap_or_else(|| panic!("place10kBlocks row"));
        assert_eq!(placed["unlocked"], serde_json::json!(true));
        assert_eq!(placed["progress"], 2_500);
        assert_eq!(placed["goal"], 10_000);
        assert_eq!(placed["stat"], "blocksBuilt");
        // Event-only achievements report a `null` stat and zero goal.
        let event_only = rows
            .iter()
            .find(|row| row["name"] == "defeatAttack5Waves")
            .unwrap_or_else(|| panic!("defeatAttack5Waves row"));
        assert!(event_only["stat"].is_null());
        assert_eq!(event_only["goal"], 0);
    }
}

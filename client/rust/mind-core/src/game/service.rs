// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Local achievement/stat service (GAPS #68, the #70 `NullService` seam).
//!
//! Ported from `core/src/mindustry/service/GameService.java` (the listener
//! half) and the anonymous `GameService` in
//! `desktop/src/mindustry/desktop/DesktopLauncher.java`. Upstream has two
//! layers: the platform service (`completeAchievement`/`isAchieved`/
//! `getStat`/`setStat`/`storeStats`) and `GameService`, the event listener that
//! drives `SStat` increments and achievement completion. This module ports both
//! without Steam or the network:
//!
//! - [`LocalGameService`] is the platform half. It stores stats/achievements
//!   in memory and persists through any [`UnlockStore`] (`Core.settings` in the
//!   game, [`MemoryUnlockStore`] in tests). It implements
//!   [`crate::service::GameService`], so the existing platform seam keeps
//!   working; [`crate::service::NullService`] remains the inert default.
//! - [`ServiceTracker`] is the listener half. It owns the `blocks-built`/
//!   `units-built` sets, maps gameplay hooks onto `SStat` increments and
//!   achievement unlocks, and is installed on [`CampaignRuntime`] so the
//!   campaign tick/game-state systems drive it headlessly.
//!
//! Deliberate headless deviations (documented, not silently dropped):
//!
//! - `GameService.checkUpdate`'s power-graph checks (`negative10kPower`/
//!   `positive100kPower`/`store1milPower`) and `active10Polys` need live
//!   graph/type data the campaign tick does not own; the host drives them
//!   through [`ServiceTracker::check_power`] and
//!   [`ServiceTracker::on_poly_active`].
//! - UI-owned events (`openWiki`, `openConsole`, `enablePixelation`,
//!   `routerLanguage`, `useAnimdustryEmoji`, `joinCommunityServer`,
//!   `installMod`) and screenshot-style checks have no headless trigger; the
//!   query API is exposed for the host to call [`LocalGameService::unlock`].
//! - `blocksBuilt` counts every default-team placement (the event bus does not
//!   carry the placing unit), and `unitsBuilt` counts `GameStats.units_created`
//!   (any default-team spawn). Java gates both on `e.unit.team == defaultTeam`.
//! - The `survive10WavesNoBlocks` sector-263 workaround and the console-shown
//!   gate in `Achievement.complete()` are skipped (UI/sector-content state).
//!
//! [`CampaignRuntime`]: super::runtime::CampaignRuntime

use std::collections::BTreeSet;

use indexmap::IndexMap;

use crate::content::PlanetId;
use crate::content::settings_store::UnlockStore;

use super::achievements::{Achievement, SStat, T5_UNITS, achievement_key, stat_key};
use super::play::PlaySession;
use super::universe::Campaign;

/// The local platform service (`GameService` in `DesktopLauncher`).
///
/// Holds the 21 [`SStat`] values and the 90 [`Achievement`] flags. Unknown names
/// passed through the platform trait (mods, later DLC) are kept in overflow
/// maps so [`crate::service::GameService`] behaves like a real backend for
/// names the enum tables do not know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalGameService {
    stats: [i32; SStat::COUNT],
    achieved: [bool; Achievement::COUNT],
    extra_stats: IndexMap<String, i32>,
    extra_achievements: BTreeSet<String>,
    /// Number of `storeStats` requests (`SStat.set`/`unlock` call it).
    store_count: u64,
    /// Pending persistence: a host flush (`save`) clears it.
    dirty: bool,
}

impl Default for LocalGameService {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalGameService {
    /// Empty service (every stat `0`, every achievement locked).
    pub fn new() -> Self {
        Self {
            stats: [0; SStat::COUNT],
            achieved: [false; Achievement::COUNT],
            extra_stats: IndexMap::new(),
            extra_achievements: BTreeSet::new(),
            store_count: 0,
            dirty: false,
        }
    }

    /// Reads a stat (`SStat.get`, missing = 0).
    pub fn get(&self, stat: SStat) -> i32 {
        self.stats[stat as usize]
    }

    /// Raw platform write (`GameService.setStat`): no store request and no
    /// achievement re-check. [`LocalGameService::set`] is the `SStat.set` flow.
    pub fn write_stat(&mut self, stat: SStat, amount: i32) {
        self.stats[stat as usize] = amount;
    }

    /// `SStat.set`: write, request a flush, then re-check every achievement.
    ///
    /// Returns the achievements this call unlocked (Java's `checkCompletion`
    /// loop has no return value; the list is the headless observability seam).
    pub fn set(&mut self, stat: SStat, amount: i32) -> Vec<Achievement> {
        self.write_stat(stat, amount);
        self.store_stats();
        self.check_achievements()
    }

    /// `SStat.max`: raise to `amount` only when larger.
    pub fn max(&mut self, stat: SStat, amount: i32) -> Vec<Achievement> {
        if amount > self.get(stat) {
            self.set(stat, amount)
        } else {
            Vec::new()
        }
    }

    /// `SStat.add(int)`.
    pub fn add(&mut self, stat: SStat, amount: i32) -> Vec<Achievement> {
        let next = self.get(stat).saturating_add(amount);
        self.set(stat, next)
    }

    /// `SStat.add()`.
    pub fn add_one(&mut self, stat: SStat) -> Vec<Achievement> {
        self.add(stat, 1)
    }

    /// `Achievement.isAchieved()`.
    pub fn unlocked(&self, achievement: Achievement) -> bool {
        self.achieved[achievement as usize]
    }

    /// `Achievement.complete()`: idempotent unlock plus a store request.
    ///
    /// Returns `true` only for the call that actually unlocked it.
    pub fn unlock(&mut self, achievement: Achievement) -> bool {
        if self.achieved[achievement as usize] {
            return false;
        }
        self.achieved[achievement as usize] = true;
        self.store_stats();
        true
    }

    /// Name-based unlock used by the platform trait.
    pub fn unlock_by_name(&mut self, name: &str) -> bool {
        match Achievement::from_name(name) {
            Some(achievement) => self.unlock(achievement),
            None => {
                let newly = self.extra_achievements.insert(name.to_owned());
                if newly {
                    self.store_stats();
                }
                newly
            }
        }
    }

    /// `Achievement.uncomplete()`: idempotent relock plus a store request.
    pub fn clear(&mut self, achievement: Achievement) -> bool {
        if !self.achieved[achievement as usize] {
            return false;
        }
        self.achieved[achievement as usize] = false;
        self.store_stats();
        true
    }

    /// Name-based relock used by the platform trait.
    pub fn clear_by_name(&mut self, name: &str) -> bool {
        match Achievement::from_name(name) {
            Some(achievement) => self.clear(achievement),
            None => {
                let removed = self.extra_achievements.remove(name);
                if removed {
                    self.store_stats();
                }
                removed
            }
        }
    }

    /// `SStat.set`'s completion sweep: checks every achievement in declaration
    /// order and returns the newly unlocked ones.
    pub fn check_achievements(&mut self) -> Vec<Achievement> {
        let mut unlocked = Vec::new();
        for achievement in Achievement::ALL {
            if achievement.check_completion(self) {
                unlocked.push(*achievement);
            }
        }
        unlocked
    }

    /// `GameService.storeStats`: records a pending flush. The host persists with
    /// [`LocalGameService::save`] (settings write / Steam callback).
    pub fn store_stats(&mut self) {
        self.store_count = self.store_count.saturating_add(1);
        self.dirty = true;
    }

    /// Number of `storeStats` requests since creation (test/telemetry probe).
    pub fn store_count(&self) -> u64 {
        self.store_count
    }

    /// Whether anything changed since the last [`LocalGameService::save`].
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Loads every known stat and achievement from a settings-backed store.
    ///
    /// Overwrites the in-memory tables (call on a fresh service). Unknown names
    /// are not enumerable through [`UnlockStore`], so mod-provided stats are
    /// session-only.
    pub fn load(&mut self, store: &dyn UnlockStore) {
        for stat in SStat::ALL {
            self.stats[*stat as usize] = store.get_i32(&stat_key(stat.name()));
        }
        for achievement in Achievement::ALL {
            self.achieved[*achievement as usize] =
                store.get_bool(&achievement_key(achievement.name()));
        }
        self.dirty = false;
    }

    /// Writes every known stat and achievement through a settings-backed store.
    ///
    /// The keys are the module ABI (`achievement-<name>` /
    /// `achievement-stat-<name>`); the host flushes its settings afterwards.
    pub fn save(&mut self, store: &mut dyn UnlockStore) {
        for stat in SStat::ALL {
            store.set_i32(&stat_key(stat.name()), self.get(*stat));
        }
        for achievement in Achievement::ALL {
            store.set_bool(
                &achievement_key(achievement.name()),
                self.unlocked(*achievement),
            );
        }
        self.dirty = false;
    }

    /// Every unlocked achievement in declaration order.
    pub fn unlocked_achievements(&self) -> Vec<Achievement> {
        Achievement::ALL
            .iter()
            .copied()
            .filter(|achievement| self.unlocked(*achievement))
            .collect()
    }

    /// Number of unlocked achievements.
    pub fn unlocked_count(&self) -> usize {
        self.achieved.iter().filter(|done| **done).count()
    }

    /// `(unlocked, total)` achievement counts for a UI progress line.
    pub fn completion(&self) -> (usize, usize) {
        (self.unlocked_count(), Achievement::COUNT)
    }

    /// Stat rows in declaration order (`[(stat, value)]`) for a UI table.
    pub fn stat_rows(&self) -> Vec<(SStat, i32)> {
        SStat::ALL
            .iter()
            .map(|stat| (*stat, self.get(*stat)))
            .collect()
    }

    /// Achievement rows in declaration order for a UI table.
    pub fn achievement_rows(&self) -> Vec<AchievementRow> {
        Achievement::ALL
            .iter()
            .map(|achievement| AchievementRow {
                achievement: *achievement,
                unlocked: self.unlocked(*achievement),
                progress: achievement.progress(self),
            })
            .collect()
    }

    /// The unlocked-name list as JSON (`KEY_ACHIEVEMENTS` payload for a UI).
    pub fn unlocked_json(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.unlocked_achievements()
                .into_iter()
                .map(|achievement| serde_json::Value::String(achievement.name().to_owned()))
                .collect(),
        )
    }

    /// Reads an arbitrary platform stat name (`GameService.getStat`).
    pub fn get_stat_by_name(&self, name: &str, default: i32) -> i32 {
        match SStat::from_name(name) {
            Some(stat) => self.get(stat),
            None => self.extra_stats.get(name).copied().unwrap_or(default),
        }
    }

    /// Writes an arbitrary platform stat name (`GameService.setStat`).
    pub fn set_stat_by_name(&mut self, name: &str, amount: i32) {
        match SStat::from_name(name) {
            Some(stat) => self.write_stat(stat, amount),
            None => {
                self.extra_stats.insert(name.to_owned(), amount);
            }
        }
    }

    /// Whether an arbitrary platform achievement name is unlocked.
    pub fn is_achieved_by_name(&self, name: &str) -> bool {
        match Achievement::from_name(name) {
            Some(achievement) => self.unlocked(achievement),
            None => self.extra_achievements.contains(name),
        }
    }
}

/// One row of [`LocalGameService::achievement_rows`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AchievementRow {
    /// The achievement.
    pub achievement: Achievement,
    /// Whether it is unlocked.
    pub unlocked: bool,
    /// Stat progress clamped to the goal (`0` for event-only achievements).
    pub progress: i32,
}

impl crate::service::GameService for LocalGameService {
    fn enabled(&self) -> bool {
        true
    }

    fn complete_achievement(&mut self, name: &str) {
        self.unlock_by_name(name);
    }

    fn clear_achievement(&mut self, name: &str) {
        self.clear_by_name(name);
    }

    fn is_achieved(&self, name: &str) -> bool {
        self.is_achieved_by_name(name)
    }

    fn get_stat(&self, name: &str, default: i32) -> i32 {
        self.get_stat_by_name(name, default)
    }

    fn set_stat(&mut self, name: &str, amount: i32) {
        self.set_stat_by_name(name, amount);
    }

    fn store_stats(&mut self) {
        self.store_stats();
    }
}

/// The counter watermark [`ServiceTracker::poll`] diffs each tick.
///
/// `GameStats`/`CampaignStats` are the port's existing stat aggregation points
/// (`sim::Sim::flush_events` and `game::stats::notify_unit_created` write them
/// before the next campaign tick), so diffing them recovers the per-event
/// increments without a second listener on the event bus. A counter that goes
/// backwards means the session reset; the watermark follows it without adding a
/// negative delta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StatsWatermark {
    buildings_built: i32,
    enemy_units_destroyed: i32,
    units_created: i32,
    destroyed_blocks: i32,
}

/// Diffs one monotonic counter, absorbing session resets.
fn counter_delta(current: i32, mark: &mut i32) -> i32 {
    if current < *mark {
        *mark = current;
        return 0;
    }
    let delta = current - *mark;
    *mark = current;
    delta
}

/// The `GameService` listener half: maps gameplay hooks onto stat increments.
///
/// Java's `registerEvents` reacts to `EventType` fires; the port exposes one
/// method per hook and installs the tracker on [`CampaignRuntime`], where the
/// campaign systems (`campaign_tick_system`, `game_state_tick_system`,
/// `run_wave_now`) call it. Hooks Java drives from UI/map/editor events are
/// exposed as public methods for the host.
///
/// [`CampaignRuntime`]: super::runtime::CampaignRuntime
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceTracker {
    service: LocalGameService,
    /// `blocksBuilt`: every default-team block name ever placed.
    blocks_built: BTreeSet<String>,
    /// `unitsBuilt`: every unit name ever produced (campaign stats).
    units_built: BTreeSet<String>,
    /// Counter watermark (`poll`).
    watermark: StatsWatermark,
}

impl ServiceTracker {
    /// Empty tracker with an empty local service.
    pub fn new() -> Self {
        Self::default()
    }

    /// Wraps a pre-loaded service (host boot path).
    pub fn with_service(service: LocalGameService) -> Self {
        Self {
            service,
            ..Self::default()
        }
    }

    /// The platform service (`Vars.service`).
    pub fn service(&self) -> &LocalGameService {
        &self.service
    }

    /// Mutable platform service.
    pub fn service_mut(&mut self) -> &mut LocalGameService {
        &mut self.service
    }

    /// Removes the service from the tracker (host teardown/persistence).
    pub fn take_service(&mut self) -> LocalGameService {
        std::mem::take(&mut self.service)
    }

    /// Loads the persisted service state (host boot).
    pub fn load(&mut self, store: &dyn UnlockStore) {
        self.service.load(store);
    }

    /// Persists the service state (host save/flush).
    pub fn save(&mut self, store: &mut dyn UnlockStore) {
        self.service.save(store);
    }

    /// Block names seen in the `blocks-built` set (`GameService.blocksBuilt`).
    pub fn blocks_built(&self) -> &BTreeSet<String> {
        &self.blocks_built
    }

    /// Unit names seen in the `units-built` set (`GameService.unitsBuilt`).
    pub fn units_built(&self) -> &BTreeSet<String> {
        &self.units_built
    }

    /// `Events.on(WaveEvent)`: `maxWavesSurvived.max(wave)` plus the
    /// `survive10WavesNoBlocks` check.
    ///
    /// Java skips sector 263 of Serpulo (a uniquely broken map); the port keeps
    /// that guard because it is observable in saves.
    pub fn on_wave(&mut self, session: &PlaySession, campaign: &Campaign) {
        if !session.is_campaign() {
            return;
        }
        self.service.max(SStat::MaxWavesSurvived, session.wave);
        let broken_map = campaign.planet_id_by_name("serpulo") == session.sector.map(|s| s.0)
            && session.sector.is_some_and(|s| s.1 == 263);
        if session.stats.buildings_built == 0 && session.wave >= 10 && !broken_map {
            self.service.unlock(Achievement::Survive10WavesNoBlocks);
        }
    }

    /// `Events.on(SectorCaptureEvent)`: attack victories, no-loss capture and
    /// the controlled/captured-all sector checks.
    ///
    /// `attack_mode`/`buildings_destroyed`/`wave` are the pre-capture session
    /// values (Java fires the event before `Logic.sectorCapture` clears
    /// `attackMode`, so the listener still sees them).
    #[allow(clippy::too_many_arguments)]
    pub fn on_sector_capture(
        &mut self,
        session: &PlaySession,
        campaign: &Campaign,
        planet: PlanetId,
        was_attack_mode: bool,
        buildings_destroyed: i32,
    ) {
        if was_attack_mode {
            self.service.add_one(SStat::AttacksWon);
            if session.wave <= 5 {
                self.service.unlock(Achievement::DefeatAttack5Waves);
            }
        }
        if buildings_destroyed == 0 {
            self.service.unlock(Achievement::CaptureNoBlocksBroken);
        }
        if campaign.planet_id_by_name("serpulo") == Some(planet) {
            self.service.set(
                SStat::SectorsControlled,
                sectors_with_base(campaign, planet),
            );
        }
        if planet_fully_based(campaign, planet) {
            self.service.unlock(Achievement::CaptureAllSectors);
        }
    }

    /// `Control`'s last-sector victory (`CampaignComplete`): the planet
    /// completion achievements.
    pub fn on_campaign_complete(&mut self, campaign: &Campaign, planet: PlanetId) {
        let achievement = if campaign.planet_id_by_name("erekir") == Some(planet) {
            Achievement::CompleteErekir
        } else if campaign.planet_id_by_name("serpulo") == Some(planet) {
            Achievement::CompleteSerpulo
        } else {
            return;
        };
        self.service.unlock(achievement);
    }

    /// `Events.on(WinEvent)`: PvP victory.
    ///
    /// Upstream fires `WinEvent` from `GameOverDialog`; the headless analog is
    /// a game over the default team won under `rules.pvp`.
    pub fn on_game_over(&mut self, session: &PlaySession, winner: u8) {
        if session.rules.pvp && winner == session.rules.default_team {
            self.service.add_one(SStat::PvpsWon);
        }
    }

    /// `Events.on(LaunchItemEvent)`: launched items count and the launch-pad
    /// achievement (Java gates `launchItemPad` on `campaign()`).
    pub fn on_item_launched(&mut self, amount: i32, campaign: bool) {
        self.service.add(SStat::ItemsLaunched, amount);
        if campaign {
            self.service.unlock(Achievement::LaunchItemPad);
        }
    }

    /// `Events.on(SectorLaunchEvent)`: `timesLaunched.add()`.
    pub fn on_sector_launch(&mut self) {
        self.service.add_one(SStat::TimesLaunched);
    }

    /// `Events.on(SchematicCreateEvent)`: `schematicsCreated.add()`.
    pub fn on_schematic_created(&mut self) {
        self.service.add_one(SStat::SchematicsCreated);
    }

    /// `Events.on(MapMakeEvent)`.
    pub fn on_map_made(&mut self) {
        self.service.add_one(SStat::MapsMade);
    }

    /// `Events.on(MapPublishEvent)`.
    pub fn on_map_published(&mut self) {
        self.service.add_one(SStat::MapsPublished);
    }

    /// `Events.run(Trigger.thoriumReactorOverheat)`.
    pub fn on_reactor_overheat(&mut self) {
        self.service.add_one(SStat::ReactorsOverheated);
    }

    /// `Events.on(UnitDestroyEvent)`'s boss branch (`unit.isBoss()`).
    pub fn on_boss_defeated(&mut self) {
        self.service.add_one(SStat::BossesDefeated);
    }

    /// `BlockBuildEndEvent`'s `breakSound == rockBreak` branch
    /// (`bouldersDeconstructed`).
    pub fn on_boulder_deconstructed(&mut self, amount: i32) {
        self.service.add(SStat::BouldersDeconstructed, amount);
    }

    /// `Events.on(UnlockEvent)`: `obtainThorium`/`obtainTitanium` and the
    /// research checks that upstream re-runs on every unlock.
    pub fn on_unlock(&mut self, content_name: &str) {
        match content_name {
            "thorium" => {
                self.service.unlock(Achievement::ObtainThorium);
            }
            "titanium" => {
                self.service.unlock(Achievement::ObtainTitanium);
            }
            "router" => {
                self.service.unlock(Achievement::ResearchRouter);
            }
            "microProcessor" => {
                self.service.unlock(Achievement::ResearchLogic);
            }
            _ => {}
        }
    }

    /// `checkUpdate`'s power-graph checks (`balance` per minute, `stored`).
    pub fn check_power(&mut self, balance: f32, stored: f32) {
        if balance < -10_000.0 {
            self.service.unlock(Achievement::Negative10kPower);
        }
        if balance > 100_000.0 {
            self.service.unlock(Achievement::Positive100kPower);
        }
        if stored > 1_000_000.0 {
            self.service.unlock(Achievement::Store1milPower);
        }
    }

    /// `checkUpdate`'s `active10Polys`: at least ten team polys alive.
    pub fn on_poly_active(&mut self, count: i32) {
        if count >= 10 {
            self.service.unlock(Achievement::Active10Polys);
        }
    }

    /// `Timer.schedule(this::checkUpdate, 2f, 2f)`: the periodic checks.
    ///
    /// `active_units` is the live default-team unit count
    /// (`Groups.unit.count(t -> t.team == player.team())`).
    pub fn check_update(&mut self, session: &PlaySession, campaign: &Campaign, active_units: i32) {
        if !session.is_campaign() {
            return;
        }
        self.service.max(SStat::MaxUnitActive, active_units);
        if active_units >= 100 {
            self.service.unlock(Achievement::Active100Units);
        }
        self.service
            .max(SStat::MaxProduction, production_per_minute(campaign));
        self.service
            .max(SStat::TotalCampaignItems, campaign_item_total(campaign));
    }

    /// Diffs `GameStats`/`CampaignStats` into the [`SStat`] table.
    ///
    /// Drives `blocksBuilt`, `blocksDestroyed`, `unitsBuilt`, `unitsDestroyed`,
    /// `unitTypesBuilt` and the block/unit-name achievements. Safe to call every
    /// tick: counters are only read when unchanged, and the name scans stop at
    /// the first new name per poll.
    pub fn poll(&mut self, session: &PlaySession, campaign: &Campaign) {
        // Block placements (`Logic`'s `BlockBuildEndEvent` -> GameStats).
        let placed = counter_delta(
            session.stats.buildings_built,
            &mut self.watermark.buildings_built,
        );
        if placed > 0 {
            self.service.add(SStat::BlocksBuilt, placed);
        }
        // Enemy blocks destroyed (`BlockDestroyEvent` -> destroyed map).
        let destroyed: i32 = session.stats.destroyed_block_count.values().sum();
        let destroyed = counter_delta(destroyed, &mut self.watermark.destroyed_blocks);
        if destroyed > 0 {
            self.service.add(SStat::BlocksDestroyed, destroyed);
        }
        // Friendly units created (`UnitCreateEvent` -> unitsCreated).
        let created = counter_delta(
            session.stats.units_created,
            &mut self.watermark.units_created,
        );
        if created > 0 {
            self.service.add(SStat::UnitsBuilt, created);
        }
        // Enemy unit kills (`UnitDestroyEvent` -> enemyUnitsDestroyed).
        let killed = counter_delta(
            session.stats.enemy_units_destroyed,
            &mut self.watermark.enemy_units_destroyed,
        );
        if killed > 0 && session.is_campaign() {
            self.service.add(SStat::UnitsDestroyed, killed);
        }

        // `blocksBuilt.add(name)` (persisted across sessions upstream).
        for name in session.stats.placed_block_count.keys() {
            if self.blocks_built.insert(name.clone()) {
                self.on_block_built(name, session, campaign);
            }
        }

        // `unitsBuilt.add(type.name)` + `unitTypesBuilt.max(...)`.
        let mut new_units = false;
        for (_, planet) in &campaign.planets {
            if !owns_stats(planet) {
                continue;
            }
            for name in planet.stats.units_produced.keys() {
                if self.units_built.insert(name.clone()) {
                    new_units = true;
                    if T5_UNITS.contains(&name.as_str()) {
                        self.service.unlock(Achievement::BuildT5);
                    }
                }
            }
        }
        if new_units {
            let count = self.units_built.len() as i32;
            self.service.max(SStat::UnitTypesBuilt, count);
        }
    }

    /// `BlockBuildEndEvent`'s per-name checks (`!breaking`).
    fn on_block_built(&mut self, name: &str, session: &PlaySession, campaign: &Campaign) {
        match name {
            "groundFactory" => {
                self.service.unlock(Achievement::BuildGroundFactory);
            }
            "mendProjector" => {
                self.service.unlock(Achievement::BuildMendProjector);
            }
            "overdriveProjector" => {
                self.service.unlock(Achievement::BuildOverdriveProjector);
            }
            "coreNucleus" => {
                // `state.rules.sector == SectorPresets.groundZero.sector`.
                let in_ground_zero = session.sector.is_some_and(|(planet, id)| {
                    campaign
                        .sector(planet, id)
                        .is_some_and(|sector| sector.preset_name.as_deref() == Some("groundZero"))
                });
                if in_ground_zero {
                    self.service.unlock(Achievement::NucleusGroundZero);
                }
            }
            _ => {}
        }
        if ["meltdown", "spectre", "foreshadow"]
            .iter()
            .all(|required| self.blocks_built.contains(*required))
        {
            self.service.unlock(Achievement::BuildMeltdownSpectre);
        }
    }
}

/// `Planet.hasOwnStats` for the `statParent` delegation
/// (`Campaign::stats_for`).
fn owns_stats(planet: &super::planet::Planet) -> bool {
    planet.stat_parent.is_none_or(|parent| parent == planet.id)
}

/// `state.rules.sector.planet.sectors.count(Sector::hasBase)`.
fn sectors_with_base(campaign: &Campaign, planet: PlanetId) -> i32 {
    campaign
        .planet(planet)
        .map(|planet| {
            planet
                .sectors
                .iter()
                .filter(|sector| sector.has_base())
                .count() as i32
        })
        .unwrap_or(0)
}

/// `!planet.sectors.contains(s -> !s.hasBase())` with at least one sector.
fn planet_fully_based(campaign: &Campaign, planet: PlanetId) -> bool {
    campaign.planet(planet).is_some_and(|planet| {
        !planet.sectors.is_empty() && planet.sectors.iter().all(|sector| sector.has_base())
    })
}

/// `TurnEvent`'s `maxProduction`: the sum of base production means per minute.
fn production_per_minute(campaign: &Campaign) -> i32 {
    let mut total = 0.0f32;
    for planet in campaign.planets.values() {
        for sector in &planet.sectors {
            if !sector.has_base() {
                continue;
            }
            for stat in sector.info.info.production.values() {
                if stat.mean > 0.0 {
                    total += stat.mean * 60.0;
                }
            }
        }
    }
    total.round() as i32
}

/// `TurnEvent`'s `totalCampaignItems`: items stored in every base sector.
fn campaign_item_total(campaign: &Campaign) -> i32 {
    let mut total = 0i32;
    for planet in campaign.planets.values() {
        for sector in &planet.sectors {
            if sector.has_base() {
                total = total.saturating_add(sector.info.info.items.values().sum::<i32>());
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::PlanetId;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::rules::Rules;

    fn setup() -> (Campaign, PlaySession) {
        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content");
        let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let planet = campaign.planet_id_by_name("serpulo").expect("serpulo");
        let sector = campaign
            .planet(planet)
            .and_then(|planet| planet.sectors.first())
            .map(|sector| sector.id)
            .expect("sector");
        let mut session = PlaySession::new(Rules {
            waves: true,
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session.sector = Some((planet, sector));
        session.phase = State::Playing;
        (campaign, session)
    }

    fn planet_of(session: &PlaySession) -> PlanetId {
        session.sector.expect("sector").0
    }

    #[test]
    fn stat_accumulation_from_representative_events() {
        let (campaign, mut session) = setup();
        let mut tracker = ServiceTracker::new();

        // One wave, one block placed, one friendly unit, two kills, one enemy
        // block destroyed.
        session.wave = 1;
        tracker.on_wave(&session, &campaign);
        assert_eq!(tracker.service().get(SStat::MaxWavesSurvived), 1);

        session.stats.buildings_built = 1;
        session.stats.placed_block_count.insert("router".into(), 1);
        session.stats.units_created = 1;
        session.stats.enemy_units_destroyed = 2;
        session
            .stats
            .destroyed_block_count
            .insert("conveyor".into(), 1);
        tracker.poll(&session, &campaign);

        let service = tracker.service();
        assert_eq!(service.get(SStat::BlocksBuilt), 1);
        assert_eq!(service.get(SStat::BlocksDestroyed), 1);
        assert_eq!(service.get(SStat::UnitsBuilt), 1);
        assert_eq!(service.get(SStat::UnitsDestroyed), 2);
        assert!(tracker.blocks_built().contains("router"));

        // A second poll with unchanged counters adds nothing.
        tracker.poll(&session, &campaign);
        assert_eq!(tracker.service().get(SStat::BlocksBuilt), 1);

        // Item launches and sector launches are direct hooks.
        tracker.on_item_launched(25, true);
        tracker.on_sector_launch();
        assert_eq!(tracker.service().get(SStat::ItemsLaunched), 25);
        assert_eq!(tracker.service().get(SStat::TimesLaunched), 1);
        assert!(Achievement::LaunchItemPad.unlocked(tracker.service()));

        // Reactor overheat + boss kill + boulders.
        tracker.on_reactor_overheat();
        tracker.on_boss_defeated();
        tracker.on_boulder_deconstructed(3);
        assert_eq!(tracker.service().get(SStat::ReactorsOverheated), 1);
        assert_eq!(tracker.service().get(SStat::BossesDefeated), 1);
        assert_eq!(tracker.service().get(SStat::BouldersDeconstructed), 3);
        assert!(Achievement::OverheatReactor.unlocked(tracker.service()));
    }

    #[test]
    fn max_semantics_and_threshold_achievements() {
        let (_campaign, _session) = setup();
        let mut service = LocalGameService::new();

        service.max(SStat::MaxWavesSurvived, 5);
        service.max(SStat::MaxWavesSurvived, 3);
        assert_eq!(service.get(SStat::MaxWavesSurvived), 5);

        // 10k blocks unlocks exactly at the goal.
        service.add(SStat::BlocksBuilt, 9_999);
        assert!(!Achievement::Place10kBlocks.unlocked(&service));
        let unlocked = service.add_one(SStat::BlocksBuilt);
        assert_eq!(unlocked, vec![Achievement::Place10kBlocks]);
        assert!(Achievement::Place10kBlocks.unlocked(&service));

        // Idempotent unlock: the second call is a no-op and does not request a
        // flush.
        let before = service.store_count();
        assert!(!Achievement::Place10kBlocks.unlock(&mut service));
        assert_eq!(service.store_count(), before);

        // `SStat.max` also runs the completion sweep.
        service.max(SStat::MaxWavesSurvived, 100);
        assert!(Achievement::Survive100Waves.unlocked(&service));
    }

    #[test]
    fn persistence_roundtrip_through_unlock_store() {
        let (_campaign, _session) = setup();
        let mut service = LocalGameService::new();
        service.set(SStat::UnitsDestroyed, 1_234);
        service.add(SStat::BlocksBuilt, 50);
        service.unlock(Achievement::BuildT5);
        assert!(service.is_dirty());

        let mut store = MemoryUnlockStore::new();
        service.save(&mut store);
        assert!(!service.is_dirty());
        assert_eq!(store.get_i32("achievement-stat-unitsDestroyed"), 1_234);
        assert!(store.get_bool("achievement-buildT5"));
        assert!(!store.get_bool("achievement-place10kBlocks"));

        let mut reloaded = LocalGameService::new();
        reloaded.load(&store);
        assert_eq!(reloaded.get(SStat::UnitsDestroyed), 1_234);
        assert_eq!(reloaded.get(SStat::BlocksBuilt), 50);
        assert!(Achievement::BuildT5.unlocked(&reloaded));
        // `SStat.set` ran the completion sweep: 1234 kills crossed the
        // `Kill1kEnemies` goal, so two achievements are persisted.
        assert!(Achievement::Kill1kEnemies.unlocked(&reloaded));
        assert_eq!(reloaded.completion(), (2, Achievement::COUNT));
    }

    #[test]
    fn platform_trait_matches_the_steam_service_surface() {
        let (_campaign, _session) = setup();
        let mut service = LocalGameService::new();
        assert!(crate::service::GameService::enabled(&service));

        // Platform `setStat` is a raw write (upstream's Steam `setStatI`), so it
        // does not run the `SStat.set` completion sweep.
        crate::service::GameService::set_stat(&mut service, "blocksBuilt", 10_000);
        assert_eq!(service.get(SStat::BlocksBuilt), 10_000);
        assert!(!Achievement::Place10kBlocks.unlocked(&service));
        // The typed `SStat.set` path does.
        service.set(SStat::BlocksBuilt, 10_000);
        assert!(Achievement::Place10kBlocks.unlocked(&service));

        // Unknown names (mods) round-trip through the overflow maps.
        crate::service::GameService::set_stat(&mut service, "modStat", 7);
        assert_eq!(
            crate::service::GameService::get_stat(&service, "modStat", 0),
            7
        );
        assert_eq!(
            crate::service::GameService::get_stat(&service, "missing", 42),
            42
        );
        crate::service::GameService::complete_achievement(&mut service, "modAchievement");
        assert!(crate::service::GameService::is_achieved(
            &service,
            "modAchievement"
        ));
        crate::service::GameService::complete_achievement(&mut service, "buildT5");
        assert!(crate::service::GameService::is_achieved(
            &service, "buildT5"
        ));
        crate::service::GameService::store_stats(&mut service);
        assert!(service.store_count() > 0);
    }

    #[test]
    fn poll_absorbs_session_resets() {
        let (campaign, mut session) = setup();
        let mut tracker = ServiceTracker::new();
        session.stats.buildings_built = 4;
        tracker.poll(&session, &campaign);
        assert_eq!(tracker.service().get(SStat::BlocksBuilt), 4);

        // New game: counters reset. No negative delta.
        session.stats.buildings_built = 0;
        tracker.poll(&session, &campaign);
        assert_eq!(tracker.service().get(SStat::BlocksBuilt), 4);

        session.stats.buildings_built = 2;
        tracker.poll(&session, &campaign);
        assert_eq!(tracker.service().get(SStat::BlocksBuilt), 6);
    }

    #[test]
    fn sector_capture_and_unit_type_tracking() {
        let (mut campaign, mut session) = setup();
        let planet = planet_of(&session);

        // Mark two sectors as based so control/capture checks have something to
        // count.
        let sectors: Vec<u16> = campaign
            .planet(planet)
            .map(|planet| {
                planet
                    .sectors
                    .iter()
                    .take(2)
                    .map(|sector| sector.id)
                    .collect()
            })
            .unwrap_or_default();
        for id in &sectors {
            if let Some(sector) = campaign.sector_mut(planet, *id) {
                sector.save = Some("sector-save".into());
                sector.info.info.has_core = true;
            }
        }
        let expected_based = sectors_with_base(&campaign, planet);
        assert_eq!(expected_based, 2);

        let mut tracker = ServiceTracker::new();
        // Attack-mode capture within five waves, no buildings lost.
        session.rules.attack_mode = true;
        session.wave = 3;
        tracker.on_sector_capture(&session, &campaign, planet, true, 0);
        let service = tracker.service();
        assert_eq!(service.get(SStat::AttacksWon), 1);
        assert_eq!(service.get(SStat::SectorsControlled), 2);
        assert!(Achievement::DefeatAttack5Waves.unlocked(service));
        assert!(Achievement::CaptureNoBlocksBroken.unlocked(service));

        // Late-wave capture with losses does not award either.
        let mut tracker = ServiceTracker::new();
        session.wave = 12;
        tracker.on_sector_capture(&session, &campaign, planet, false, 4);
        assert_eq!(tracker.service().get(SStat::AttacksWon), 0);
        assert!(!Achievement::DefeatAttack5Waves.unlocked(tracker.service()));
        assert!(!Achievement::CaptureNoBlocksBroken.unlocked(tracker.service()));

        // Unit-type tracking: two produced units, one of them a T5.
        if let Some(stats) = campaign.stats_for_mut(planet) {
            stats.add_unit_produced("dagger", 2);
            stats.add_unit_produced("eclipse", 1);
        }
        let mut tracker = ServiceTracker::new();
        tracker.poll(&session, &campaign);
        assert_eq!(tracker.service().get(SStat::UnitTypesBuilt), 2);
        assert!(tracker.units_built().contains("eclipse"));
        assert!(Achievement::BuildT5.unlocked(tracker.service()));

        // Polling again does not grow the distinct-type count.
        tracker.poll(&session, &campaign);
        assert_eq!(tracker.service().get(SStat::UnitTypesBuilt), 2);
    }

    #[test]
    fn turn_and_update_checks_track_peaks() {
        let (mut campaign, mut session) = setup();
        let planet = planet_of(&session);
        let sector_id = session.sector.expect("sector").1;
        if let Some(sector) = campaign.sector_mut(planet, sector_id) {
            sector.save = Some("sector-save".into());
            sector.info.info.has_core = true;
            sector.info.info.items.insert("copper".into(), 900);
            sector.info.info.production.insert(
                "copper".into(),
                crate::io::json::rules::ExportStat {
                    mean: 2.0,
                    counter: 0.0,
                },
            );
        }
        let mut tracker = ServiceTracker::new();
        tracker.check_update(&session, &campaign, 42);
        assert_eq!(tracker.service().get(SStat::MaxUnitActive), 42);
        assert_eq!(tracker.service().get(SStat::MaxProduction), 120);
        assert_eq!(tracker.service().get(SStat::TotalCampaignItems), 900);

        // Peaks never decrease.
        session.wave = 1;
        tracker.check_update(&session, &campaign, 3);
        assert_eq!(tracker.service().get(SStat::MaxUnitActive), 42);

        // 100 active units + full completion of the capture graph.
        tracker.check_update(&session, &campaign, 100);
        assert!(Achievement::Active100Units.unlocked(tracker.service()));
        tracker.on_campaign_complete(&campaign, planet);
        assert!(Achievement::CompleteSerpulo.unlocked(tracker.service()));

        // PvP win.
        session.rules.pvp = true;
        tracker.on_game_over(&session, session.rules.default_team);
        assert_eq!(tracker.service().get(SStat::PvpsWon), 1);
        tracker.on_game_over(&session, 0);
        assert_eq!(tracker.service().get(SStat::PvpsWon), 1);

        // Unlock hook.
        tracker.on_unlock("thorium");
        tracker.on_unlock("microProcessor");
        assert!(Achievement::ObtainThorium.unlocked(tracker.service()));
        assert!(Achievement::ResearchLogic.unlocked(tracker.service()));

        // Power and poly checks.
        tracker.check_power(-20_000.0, 2_000_000.0);
        tracker.on_poly_active(10);
        assert!(Achievement::Negative10kPower.unlocked(tracker.service()));
        assert!(Achievement::Store1milPower.unlocked(tracker.service()));
        assert!(Achievement::Active10Polys.unlocked(tracker.service()));
    }

    #[test]
    fn query_api_rows_and_json() {
        let mut service = LocalGameService::new();
        service.add(SStat::BlocksBuilt, 2_500);
        service.unlock(Achievement::Place10kBlocks);
        let rows = service.achievement_rows();
        assert_eq!(rows.len(), Achievement::COUNT);
        let row = rows
            .iter()
            .find(|row| row.achievement == Achievement::Place10kBlocks)
            .expect("row");
        assert!(row.unlocked);
        assert_eq!(row.progress, 2_500);
        assert_eq!(service.stat_rows().len(), SStat::COUNT);
        let json = service.unlocked_json();
        assert_eq!(json[0], "place10kBlocks");
        assert_eq!(service.completion(), (1, Achievement::COUNT));
    }
}

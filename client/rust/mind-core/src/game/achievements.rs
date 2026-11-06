// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Steam-style achievement/stat tables (GAPS #68).
//!
//! Ported from `core/src/mindustry/service/Achievement.java` and
//! `core/src/mindustry/service/SStat.java` at upstream HEAD: 90 achievements
//! and 21 stats, in Java declaration order (order is the persistence/UI order
//! and mirrors the Steam API's names).
//!
//! Upstream `Achievement` carries only a driving [`SStat`] and a goal; the
//! display half (description text, icon) never lived in the enum — the
//! achievement dialog was removed and Steam supplies descriptions through the
//! app configuration. This port therefore exposes the parity `name()`, the
//! stat predicate ([`Achievement::stat`]/[`Achievement::goal`]) and the
//! conventional bundle key ([`Achievement::description_key`]); there is no
//! `hidden`/`icon` field upstream at this commit, and none is invented here.
//!
//! Persistence keys (settings ABI used by [`LocalGameService`]):
//!
//! - `achievement-<name>` — boolean, the achievement was unlocked;
//! - `achievement-stat-<name>` — integer, the stat value.
//!
//! [`LocalGameService`]: super::service::LocalGameService

use super::service::LocalGameService;

/// The aggregate settings key a UI may write the unlocked-name JSON list to
/// (the per-achievement `achievement-<name>` booleans stay canonical).
pub const KEY_ACHIEVEMENTS: &str = "achievements";

/// Prefix of the per-achievement persistence key (`achievement-<name>`).
pub const ACHIEVEMENT_KEY_PREFIX: &str = "achievement-";

/// Prefix of the per-stat persistence key (`achievement-stat-<name>`).
pub const STAT_KEY_PREFIX: &str = "achievement-stat-";

/// `T5` unit names (`GameService.registerEvents`' `t5s` set).
///
/// Java hard-codes the six tier-5 units; the port keys the set by unit name so
/// it can live next to the content-name ABI.
pub const T5_UNITS: &[&str] = &["omura", "reign", "toxopid", "eclipse", "oct", "corvus"];

/// The persistence key for an achievement (`achievement-<name>`).
pub fn achievement_key(name: &str) -> String {
    format!("{ACHIEVEMENT_KEY_PREFIX}{name}")
}

/// The persistence key for a stat (`achievement-stat-<name>`).
pub fn stat_key(name: &str) -> String {
    format!("{STAT_KEY_PREFIX}{name}")
}

/// One Steam stat (`SStat`): a named integer the achievement predicates read.
///
/// Java's `get`/`set`/`max`/`add` helpers are on [`LocalGameService`] because
/// upstream reads the global `Vars.service`; the port threads the service
/// explicitly so the host owns the platform seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum SStat {
    /// Enemy units destroyed in campaign games (`unitsDestroyed`).
    UnitsDestroyed,
    /// Attack-mode sectors won (`attacksWon`).
    AttacksWon,
    /// PvP games won (`pvpsWon`).
    PvpsWon,
    /// Sector launches (`timesLaunched`).
    TimesLaunched,
    /// Enemy blocks destroyed (`blocksDestroyed`).
    BlocksDestroyed,
    /// Items sent through launch pads (`itemsLaunched`).
    ItemsLaunched,
    /// Reactor overheat events (`reactorsOverheated`).
    ReactorsOverheated,
    /// Maximum simultaneously active player-team units (`maxUnitActive`).
    MaxUnitActive,
    /// Distinct unit types produced (`unitTypesBuilt`).
    UnitTypesBuilt,
    /// Units produced (`unitsBuilt`).
    UnitsBuilt,
    /// Bosses defeated (`bossesDefeated`).
    BossesDefeated,
    /// Maximum players on a hosted server (`maxPlayersServer`).
    MaxPlayersServer,
    /// Maps made in the editor (`mapsMade`).
    MapsMade,
    /// Maps published to the workshop (`mapsPublished`).
    MapsPublished,
    /// Highest wave survived (`maxWavesSurvived`).
    MaxWavesSurvived,
    /// Player-team blocks built (`blocksBuilt`).
    BlocksBuilt,
    /// Peak production per minute (`maxProduction`).
    MaxProduction,
    /// Serpulo sectors with a base (`sectorsControlled`).
    SectorsControlled,
    /// Schematics created (`schematicsCreated`).
    SchematicsCreated,
    /// Boulders deconstructed (`bouldersDeconstructed`).
    BouldersDeconstructed,
    /// Peak campaign items across sectors (`totalCampaignItems`).
    TotalCampaignItems,
}

impl SStat {
    /// Every stat in Java declaration order.
    pub const ALL: &'static [SStat] = &[
        SStat::UnitsDestroyed,
        SStat::AttacksWon,
        SStat::PvpsWon,
        SStat::TimesLaunched,
        SStat::BlocksDestroyed,
        SStat::ItemsLaunched,
        SStat::ReactorsOverheated,
        SStat::MaxUnitActive,
        SStat::UnitTypesBuilt,
        SStat::UnitsBuilt,
        SStat::BossesDefeated,
        SStat::MaxPlayersServer,
        SStat::MapsMade,
        SStat::MapsPublished,
        SStat::MaxWavesSurvived,
        SStat::BlocksBuilt,
        SStat::MaxProduction,
        SStat::SectorsControlled,
        SStat::SchematicsCreated,
        SStat::BouldersDeconstructed,
        SStat::TotalCampaignItems,
    ];

    /// Number of stats (`SStat.values().length`).
    pub const COUNT: usize = SStat::ALL.len();

    /// The upstream enum name; also the Steam stat name.
    pub const fn name(self) -> &'static str {
        match self {
            SStat::UnitsDestroyed => "unitsDestroyed",
            SStat::AttacksWon => "attacksWon",
            SStat::PvpsWon => "pvpsWon",
            SStat::TimesLaunched => "timesLaunched",
            SStat::BlocksDestroyed => "blocksDestroyed",
            SStat::ItemsLaunched => "itemsLaunched",
            SStat::ReactorsOverheated => "reactorsOverheated",
            SStat::MaxUnitActive => "maxUnitActive",
            SStat::UnitTypesBuilt => "unitTypesBuilt",
            SStat::UnitsBuilt => "unitsBuilt",
            SStat::BossesDefeated => "bossesDefeated",
            SStat::MaxPlayersServer => "maxPlayersServer",
            SStat::MapsMade => "mapsMade",
            SStat::MapsPublished => "mapsPublished",
            SStat::MaxWavesSurvived => "maxWavesSurvived",
            SStat::BlocksBuilt => "blocksBuilt",
            SStat::MaxProduction => "maxProduction",
            SStat::SectorsControlled => "sectorsControlled",
            SStat::SchematicsCreated => "schematicsCreated",
            SStat::BouldersDeconstructed => "bouldersDeconstructed",
            SStat::TotalCampaignItems => "totalCampaignItems",
        }
    }

    /// Resolves an upstream stat name (`service.getStat(name, ...)`).
    pub fn from_name(name: &str) -> Option<SStat> {
        SStat::ALL.iter().copied().find(|stat| stat.name() == name)
    }

    /// Reserves the stats whose semantics are "highest value wins"
    /// (`SStat.max`): `maxProduction`, `maxUnitActive`, `unitTypesBuilt`,
    /// `maxPlayersServer`, `maxWavesSurvived` and `totalCampaignItems`.
    pub const fn is_max(self) -> bool {
        matches!(
            self,
            SStat::MaxProduction
                | SStat::MaxUnitActive
                | SStat::UnitTypesBuilt
                | SStat::MaxPlayersServer
                | SStat::MaxWavesSurvived
                | SStat::TotalCampaignItems
        )
    }

    /// Reads the current value (`SStat.get`).
    pub fn get(self, service: &LocalGameService) -> i32 {
        service.get(self)
    }

    /// Writes and re-checks every achievement (`SStat.set`).
    pub fn set(self, service: &mut LocalGameService, amount: i32) -> Vec<Achievement> {
        service.set(self, amount)
    }

    /// Raises to `amount` when larger (`SStat.max`).
    pub fn max(self, service: &mut LocalGameService, amount: i32) -> Vec<Achievement> {
        service.max(self, amount)
    }

    /// Adds `amount` (`SStat.add(int)`).
    pub fn add(self, service: &mut LocalGameService, amount: i32) -> Vec<Achievement> {
        service.add(self, amount)
    }

    /// Adds one (`SStat.add()`).
    pub fn add_one(self, service: &mut LocalGameService) -> Vec<Achievement> {
        service.add(self, 1)
    }
}

/// Declares the [`Achievement`] enum, its [`Achievement::ALL`] table and the
/// `name`/`stat`/`goal` accessors from one Java-ordered list.
macro_rules! define_achievements {
    ($(
        $(#[$meta:meta])*
        $variant:ident => $name:literal, $stat:expr, $goal:literal;
    )*) => {
        /// One Steam achievement (`Achievement`).
        ///
        /// Variant order is the upstream declaration order; `name()` is the
        /// exact Java enum name (`Achievement.name()`), which is also the Steam
        /// achievement API name and the persistence-key suffix.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Achievement {
            $(
                $(#[$meta])*
                $variant,
            )*
        }

        impl Achievement {
            /// Every achievement in Java declaration order (`Achievement.all`).
            pub const ALL: &'static [Achievement] = &[
                $(Achievement::$variant,)*
            ];

            /// The upstream enum name (`Achievement.name()`).
            pub const fn name(self) -> &'static str {
                match self {
                    $(Achievement::$variant => $name,)*
                }
            }

            /// The driving stat, if the achievement is stat-gated
            /// (`Achievement.stat`; `None` for event-only achievements).
            pub const fn stat(self) -> Option<SStat> {
                match self {
                    $(Achievement::$variant => $stat,)*
                }
            }

            /// The goal value of [`Achievement::stat`] (`Achievement.statGoal`;
            /// `0` for event-only achievements).
            pub const fn goal(self) -> i32 {
                match self {
                    $(Achievement::$variant => $goal,)*
                }
            }
        }
    };
}

define_achievements! {
    Kill1kEnemies => "kill1kEnemies", Some(SStat::UnitsDestroyed), 1_000;
    Kill100kEnemies => "kill100kEnemies", Some(SStat::UnitsDestroyed), 100_000;
    Launch100kItems => "launch100kItems", Some(SStat::ItemsLaunched), 100_000;
    Produce5kMin => "produce5kMin", Some(SStat::MaxProduction), 5_000;
    Produce50kMin => "produce50kMin", Some(SStat::MaxProduction), 50_000;
    Win10Attack => "win10Attack", Some(SStat::AttacksWon), 10;
    Win10PvP => "win10PvP", Some(SStat::PvpsWon), 10;
    DefeatAttack5Waves => "defeatAttack5Waves", None, 0;
    Launch30Times => "launch30Times", Some(SStat::TimesLaunched), 30;
    CaptureBackground => "captureBackground", None, 0;
    Survive100Waves => "survive100Waves", Some(SStat::MaxWavesSurvived), 100;
    ResearchAll => "researchAll", None, 0;
    ShockWetEnemy => "shockWetEnemy", None, 0;
    KillEnemyPhaseWall => "killEnemyPhaseWall", None, 0;
    ResearchRouter => "researchRouter", None, 0;
    Place10kBlocks => "place10kBlocks", Some(SStat::BlocksBuilt), 10_000;
    Destroy1kBlocks => "destroy1kBlocks", Some(SStat::BlocksDestroyed), 1_000;
    OverheatReactor => "overheatReactor", Some(SStat::ReactorsOverheated), 1;
    Make10maps => "make10maps", Some(SStat::MapsMade), 10;
    DownloadMapWorkshop => "downloadMapWorkshop", None, 0;
    PublishMap => "publishMap", Some(SStat::MapsPublished), 1;
    DefeatBoss => "defeatBoss", Some(SStat::BossesDefeated), 1;
    CaptureAllSectors => "captureAllSectors", None, 0;
    Control10Sectors => "control10Sectors", Some(SStat::SectorsControlled), 10;
    Drop10kitems => "drop10kitems", None, 0;
    PowerupImpactReactor => "powerupImpactReactor", None, 0;
    ObtainThorium => "obtainThorium", None, 0;
    ObtainTitanium => "obtainTitanium", None, 0;
    SuicideBomb => "suicideBomb", None, 0;
    BuildGroundFactory => "buildGroundFactory", None, 0;
    IssueAttackCommand => "issueAttackCommand", None, 0;
    Active100Units => "active100Units", Some(SStat::MaxUnitActive), 100;
    Build1000Units => "build1000Units", Some(SStat::UnitsBuilt), 1_000;
    BuildAllUnits => "buildAllUnits", Some(SStat::UnitTypesBuilt), 50;
    BuildT5 => "buildT5", None, 0;
    PickupT5 => "pickupT5", None, 0;
    Active10Polys => "active10Polys", None, 0;
    DieExclusion => "dieExclusion", None, 0;
    Drown => "drown", None, 0;
    FillCoreAllCampaign => "fillCoreAllCampaign", None, 0;
    HostServer10 => "hostServer10", Some(SStat::MaxPlayersServer), 10;
    BuildMeltdownSpectre => "buildMeltdownSpectre", None, 0;
    LaunchItemPad => "launchItemPad", None, 0;
    ChainRouters => "chainRouters", None, 0;
    CircleConveyor => "circleConveyor", None, 0;
    BecomeRouter => "becomeRouter", None, 0;
    Create20Schematics => "create20Schematics", Some(SStat::SchematicsCreated), 20;
    Create500Schematics => "create500Schematics", Some(SStat::SchematicsCreated), 500;
    Survive10WavesNoBlocks => "survive10WavesNoBlocks", None, 0;
    CaptureNoBlocksBroken => "captureNoBlocksBroken", None, 0;
    UseFlameAmmo => "useFlameAmmo", None, 0;
    CoolTurret => "coolTurret", None, 0;
    EnablePixelation => "enablePixelation", None, 0;
    OpenWiki => "openWiki", None, 0;
    AllTransportOneMap => "allTransportOneMap", None, 0;
    BuildOverdriveProjector => "buildOverdriveProjector", None, 0;
    BuildMendProjector => "buildMendProjector", None, 0;
    BuildWexWater => "buildWexWater", None, 0;
    Have10mItems => "have10mItems", Some(SStat::TotalCampaignItems), 10_000_000;
    KillEclipseDuo => "killEclipseDuo", None, 0;
    KillMassDriver => "killMassDriver", None, 0;
    CompleteErekir => "completeErekir", None, 0;
    CompleteSerpulo => "completeSerpulo", None, 0;
    LaunchCoreSchematic => "launchCoreSchematic", None, 0;
    NucleusGroundZero => "nucleusGroundZero", None, 0;
    NeoplasmWater => "neoplasmWater", None, 0;
    BlastFrozenUnit => "blastFrozenUnit", None, 0;
    AllBlocksSerpulo => "allBlocksSerpulo", None, 0;
    AllBlocksErekir => "allBlocksErekir", None, 0;
    BreakForceProjector => "breakForceProjector", None, 0;
    ResearchLogic => "researchLogic", None, 0;
    Negative10kPower => "negative10kPower", None, 0;
    Positive100kPower => "positive100kPower", None, 0;
    Store1milPower => "store1milPower", None, 0;
    BlastGenerator => "blastGenerator", None, 0;
    NeoplasiaExplosion => "neoplasiaExplosion", None, 0;
    InstallMod => "installMod", None, 0;
    RouterLanguage => "routerLanguage", None, 0;
    JoinCommunityServer => "joinCommunityServer", None, 0;
    OpenConsole => "openConsole", None, 0;
    ControlTurret => "controlTurret", None, 0;
    DropUnitsCoreZone => "dropUnitsCoreZone", None, 0;
    DestroyScatterFlare => "destroyScatterFlare", None, 0;
    BoostUnit => "boostUnit", None, 0;
    BoostBuildingFloor => "boostBuildingFloor", None, 0;
    HoverUnitLiquid => "hoverUnitLiquid", None, 0;
    Break100Boulders => "break100Boulders", Some(SStat::BouldersDeconstructed), 100;
    Break10000Boulders => "break10000Boulders", Some(SStat::BouldersDeconstructed), 10_000;
    ShockwaveTowerUse => "shockwaveTowerUse", None, 0;
    UseAnimdustryEmoji => "useAnimdustryEmoji", None, 0;
}

impl Achievement {
    /// Number of achievements (`Achievement.values().length`).
    pub const COUNT: usize = Achievement::ALL.len();

    /// Resolves an upstream enum name (`service.isAchieved(name)`).
    pub fn from_name(name: &str) -> Option<Achievement> {
        Achievement::ALL
            .iter()
            .copied()
            .find(|achievement| achievement.name() == name)
    }

    /// Whether the achievement is gated by a stat rather than a gameplay event
    /// (`Achievement.stat != null`).
    pub const fn is_stat_based(self) -> bool {
        self.stat().is_some()
    }

    /// The persistence key (`achievement-<name>`).
    pub fn key(self) -> String {
        achievement_key(self.name())
    }

    /// Conventional bundle key for the description text
    /// (`achievement.<name>.description`).
    pub fn description_key(self) -> String {
        format!("achievement.{}.description", self.name())
    }

    /// The current predicate value: the stat reading clamped to the goal, or
    /// `0` for event-only achievements (`SStat.get`).
    pub fn progress(self, service: &LocalGameService) -> i32 {
        let Some(stat) = self.stat() else {
            return 0;
        };
        service.get(stat).max(0).min(self.goal().max(0))
    }

    /// Whether the stat predicate is satisfied (`stat.get() >= statGoal`).
    pub fn goal_reached(self, service: &LocalGameService) -> bool {
        self.stat()
            .is_some_and(|stat| service.get(stat) >= self.goal())
    }

    /// `Achievement.isAchieved()`.
    pub fn unlocked(self, service: &LocalGameService) -> bool {
        service.unlocked(self)
    }

    /// `Achievement.complete()`: unlock unless already unlocked.
    ///
    /// Upstream's dev-console gate is UI state and is not part of the headless
    /// port. Returns `true` when this call unlocked the achievement.
    pub fn unlock(self, service: &mut LocalGameService) -> bool {
        service.unlock(self)
    }

    /// `Achievement.uncomplete()`: relock.
    pub fn uncomplete(self, service: &mut LocalGameService) -> bool {
        service.clear(self)
    }

    /// `Achievement.checkCompletion()`: unlock when the stat predicate is met.
    ///
    /// Returns `true` when this call unlocked the achievement.
    pub fn check_completion(self, service: &mut LocalGameService) -> bool {
        if !self.unlocked(service) && self.goal_reached(service) {
            self.unlock(service)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::MemoryUnlockStore;
    use crate::content::settings_store::UnlockStore;

    #[test]
    fn tables_match_upstream_java() {
        // `Achievement.values().length` / `SStat.values().length` at the
        // upstream commit this port tracks.
        assert_eq!(Achievement::COUNT, 90);
        assert_eq!(SStat::COUNT, 21);

        // Declaration order is the parity order.
        assert_eq!(Achievement::ALL[0].name(), "kill1kEnemies");
        assert_eq!(Achievement::ALL[7].name(), "defeatAttack5Waves");
        assert_eq!(Achievement::ALL[89].name(), "useAnimdustryEmoji");
        assert_eq!(SStat::ALL[0].name(), "unitsDestroyed");
        assert_eq!(SStat::ALL[20].name(), "totalCampaignItems");
    }

    #[test]
    fn names_are_unique_and_roundtrip() {
        let mut names = std::collections::BTreeSet::new();
        for achievement in Achievement::ALL {
            assert!(
                names.insert(achievement.name()),
                "duplicate achievement name {}",
                achievement.name()
            );
            assert_eq!(
                Achievement::from_name(achievement.name()),
                Some(*achievement)
            );
        }
        let mut stat_names = std::collections::BTreeSet::new();
        for stat in SStat::ALL {
            assert!(
                stat_names.insert(stat.name()),
                "duplicate stat {}",
                stat.name()
            );
            assert_eq!(SStat::from_name(stat.name()), Some(*stat));
        }
        assert_eq!(Achievement::from_name("nope"), None);
        assert_eq!(SStat::from_name("nope"), None);
    }

    #[test]
    fn predicates_match_the_java_table() {
        let service = LocalGameService::new();
        let place = Achievement::Place10kBlocks;
        assert_eq!(place.stat(), Some(SStat::BlocksBuilt));
        assert_eq!(place.goal(), 10_000);
        assert_eq!(place.progress(&service), 0);
        assert!(!place.goal_reached(&service));

        // Manual (event-only) achievements have no stat predicate.
        assert!(!Achievement::DefeatAttack5Waves.is_stat_based());
        assert!(!Achievement::DefeatAttack5Waves.goal_reached(&service));
        assert_eq!(Achievement::DefeatAttack5Waves.goal(), 0);
        assert_eq!(Achievement::DefeatAttack5Waves.progress(&service), 0);

        // Stat goals are positive; manual goals are zero.
        for achievement in Achievement::ALL {
            match achievement.stat() {
                Some(_) => assert!(
                    achievement.goal() > 0,
                    "{} has a non-positive goal",
                    achievement.name()
                ),
                None => assert_eq!(achievement.goal(), 0, "{}", achievement.name()),
            }
        }
    }

    #[test]
    fn unlock_is_idempotent_and_persistent() {
        let mut service = LocalGameService::new();
        assert!(!Achievement::Place10kBlocks.unlocked(&service));

        // Below the goal the predicate does not fire.
        service.set(SStat::BlocksBuilt, 9_999);
        assert!(!Achievement::Place10kBlocks.unlocked(&service));

        // Reaching the goal unlocks through `SStat.set` -> `checkCompletion`.
        service.set(SStat::BlocksBuilt, 10_000);
        assert!(Achievement::Place10kBlocks.unlocked(&service));
        assert!(!Achievement::Place10kBlocks.check_completion(&mut service));
        assert!(!Achievement::Place10kBlocks.unlock(&mut service));

        // Persistence round-trip through the settings-backed unlock store.
        let mut store = MemoryUnlockStore::new();
        service.save(&mut store);
        assert!(store.get_bool("achievement-place10kBlocks"));
        assert_eq!(store.get_i32("achievement-stat-blocksBuilt"), 10_000);

        let mut reloaded = LocalGameService::new();
        reloaded.load(&store);
        assert!(Achievement::Place10kBlocks.unlocked(&reloaded));
        assert_eq!(reloaded.get(SStat::BlocksBuilt), 10_000);

        assert!(Achievement::Place10kBlocks.uncomplete(&mut reloaded));
        assert!(!Achievement::Place10kBlocks.unlocked(&reloaded));
        assert!(!Achievement::Place10kBlocks.uncomplete(&mut reloaded));
    }

    #[test]
    fn persistence_keys_are_the_documented_abi() {
        assert_eq!(achievement_key("buildT5"), "achievement-buildT5");
        assert_eq!(stat_key("unitsBuilt"), "achievement-stat-unitsBuilt");
        assert_eq!(Achievement::BuildT5.key(), "achievement-buildT5");
        assert_eq!(
            Achievement::BuildT5.description_key(),
            "achievement.buildT5.description"
        );
        assert_eq!(KEY_ACHIEVEMENTS, "achievements");
        assert_eq!(T5_UNITS.len(), 6);
        assert!(T5_UNITS.contains(&"eclipse"));
    }
}

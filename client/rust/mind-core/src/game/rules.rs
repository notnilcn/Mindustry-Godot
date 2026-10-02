// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Runtime half of `mindustry.game.Rules` (plan 12 M0).
//!
//! The persistence shape (every field + camelCase JSON ABI + `serde(default)`)
//! already lives in [`crate::io::json::rules`] (plan 04 M6, §2.3 boundary).
//! Plan 12 owns the *behavior*: `copy`, `retainContentFields`, `mode`, the
//! per-team multiplier accessors, `hasEnv`, `isBanned`, the checksum contribution
//! and the campaign constants. Rust only allows one inherent `impl` per type per
//! crate, so the accessors are added here on the plan-04 struct and the type is
//! re-exported under the plan-12 `game::rules` path.

use std::sync::OnceLock;

use crate::determinism::checksum::{Checksum, Hasher};

pub use crate::io::json::rules::{
    Attributes, DEFAULT_ENV, ExportStat, GameStats, MapLocales, NEVER, NEVER_F32, PlanetParams,
    Rules, SectorInfo, SpawnGroup, TIME_TO_MINUTES, TeamRule, TeamRules, WeatherEntry,
};

/// `Vars.turnDuration` (`2 * Time.toMinutes`) in fixed 60 Hz ticks.
pub const TURN_DURATION_TICKS: u64 = 7_200;
/// `Vars.invasionGracePeriod` (seconds).
pub const INVASION_GRACE_PERIOD: f32 = 20.0;
/// `Vars.baseInvasionChance` (per turn).
pub const BASE_INVASION_CHANCE: f64 = 0.01;
/// `Vars.maxSchematicSize` (tiles per side; config-overridable upstream).
pub const MAX_SCHEMATIC_SIZE: i32 = 64;
/// `Vars.maxSchematicDimension` (hard cap on `.msch` width/height).
pub const MAX_SCHEMATIC_DIMENSION: i32 = 128;
/// `Vars.maxLoadoutSchematicPad` (extra tiles around the core for loadouts).
pub const MAX_LOADOUT_SCHEMATIC_PAD: i32 = 5;
/// `Vars.maxPreviewsMobile` (plan 19).
pub const MAX_PREVIEWS_MOBILE: usize = 32;
/// `FogControl.DYNAMIC_UPDATE_INTERVAL` quantized to fixed ticks (25 Hz at 60 tps).
pub const DYNAMIC_UPDATE_INTERVAL_TICKS: u64 = 40;
/// `AttackIndicators.LIFETIME` (15 s at 60 Hz).
pub const ATTACK_INDICATOR_LIFETIME_TICKS: u64 = 900;
/// `TeamData.clusterChunkSize` (world pixels).
pub const CLUSTER_CHUNK_SIZE: f32 = 70.0;
/// Native save extension (`Vars.saveExtension`).
pub const SAVE_EXTENSION: &str = "msav";
/// Schematic extension (`Vars.schematicExtension`).
pub const SCHEMATIC_EXTENSION: &str = "msch";

// The campaign constants above are plan 12's (`HIGH_LEVEL_PLAN` §6.6); they live
// here rather than `mind_core::constants` to keep the shared plan-05 file untouched.

/// A team's effective rules, falling back to the upstream lazy default.
fn fallback_team_rule(team: u8) -> &'static TeamRule {
    static DERELICT: OnceLock<TeamRule> = OnceLock::new();
    static DEFAULT: OnceLock<TeamRule> = OnceLock::new();
    if team == crate::game::team::DERELICT.0 {
        DERELICT.get_or_init(|| TeamRule {
            protect_cores: false,
            check_placement: false,
            ..TeamRule::default()
        })
    } else {
        DEFAULT.get_or_init(TeamRule::default)
    }
}

impl Rules {
    /// `Rules.copy()`: plan 04 deviation 8 — a plain clone, not a JSON round trip.
    pub fn copy(&self) -> Rules {
        self.clone()
    }

    /// `TeamRules.get(team)` with the upstream lazy default inserted on first
    /// use (derelict defaults `protectCores=false`, `checkPlacement=false`).
    pub fn team_rule(&self, team: u8) -> &TeamRule {
        self.teams
            .get(team)
            .unwrap_or_else(|| fallback_team_rule(team))
    }

    /// Inserts a team's rule if absent, returning a mutable reference
    /// (`TeamRules.get` write path).
    pub fn team_rule_mut(&mut self, team: u8) -> &mut TeamRule {
        self.teams.0.entry(team).or_default()
    }

    /// `Rules.retainContentFields(source)`.
    ///
    /// The three content-defining fields are always overwritten; banned
    /// blocks/units and the loadout are overwritten only when they contain
    /// patched (mod) content. Patched-ness is supplied by the caller because
    /// `mind-core` stores content by name rather than by pointer.
    pub fn retain_content_fields(
        &mut self,
        source: &Rules,
        is_patch_content: impl Fn(&str) -> bool,
    ) {
        self.spawns = source.spawns.clone();
        self.objectives = source.objectives.clone();
        self.weather = source.weather.clone();

        if source
            .banned_blocks
            .iter()
            .any(|name| is_patch_content(name))
        {
            self.banned_blocks = source.banned_blocks.clone();
        }
        if source
            .banned_units
            .iter()
            .any(|name| is_patch_content(name))
        {
            self.banned_units = source.banned_units.clone();
        }
        if source
            .loadout
            .iter()
            .any(|stack| stack.item.as_deref().is_some_and(&is_patch_content))
        {
            self.loadout = source.loadout.clone();
        }
    }

    /// `Rules.mode()`.
    pub fn mode(&self) -> super::gamemode::Gamemode {
        use super::gamemode::Gamemode;
        if self.pvp {
            Gamemode::Pvp
        } else if self.editor {
            Gamemode::Editor
        } else if self.attack_mode {
            Gamemode::Attack
        } else if self.infinite_resources {
            Gamemode::Sandbox
        } else {
            Gamemode::Survival
        }
    }

    /// `Rules.hasEnv(env)`.
    pub const fn has_env(&self, env: i32) -> bool {
        (self.env & env) != 0
    }

    /// `Rules.buildRadius(team)` (world pixels).
    pub fn build_radius(&self, team: u8) -> f32 {
        let rule = self.team_rule(team);
        if !rule.protect_cores {
            0.0
        } else {
            self.enemy_core_build_radius + rule.extra_core_build_radius
        }
    }

    /// `Rules.unitBuildSpeed(team)`.
    pub fn unit_build_speed(&self, team: u8) -> f32 {
        self.unit_build_speed_multiplier * self.team_rule(team).unit_build_speed_multiplier
    }

    /// `Rules.unitCost(team)`.
    pub fn unit_cost(&self, team: u8) -> f32 {
        self.unit_cost_multiplier * self.team_rule(team).unit_cost_multiplier
    }

    /// `Rules.unitDamage(team)`.
    pub fn unit_damage(&self, team: u8) -> f32 {
        self.unit_damage_multiplier * self.team_rule(team).unit_damage_multiplier
    }

    /// `Rules.unitHealth(team)`; clamped to a tiny positive value like upstream.
    pub fn unit_health(&self, team: u8) -> f32 {
        (self.unit_health_multiplier * self.team_rule(team).unit_health_multiplier).max(0.000_001)
    }

    /// `Rules.unitCrashDamage(team)`.
    pub fn unit_crash_damage(&self, team: u8) -> f32 {
        self.unit_damage(team)
            * self.unit_crash_damage_multiplier
            * self.team_rule(team).unit_crash_damage_multiplier
    }

    /// `Rules.unitMineSpeed(team)`.
    pub fn unit_mine_speed(&self, team: u8) -> f32 {
        self.unit_mine_speed_multiplier * self.team_rule(team).unit_mine_speed_multiplier
    }

    /// `Rules.blockHealth(team)`.
    pub fn block_health(&self, team: u8) -> f32 {
        self.block_health_multiplier * self.team_rule(team).block_health_multiplier
    }

    /// `Rules.blockDamage(team)`.
    pub fn block_damage(&self, team: u8) -> f32 {
        self.block_damage_multiplier * self.team_rule(team).block_damage_multiplier
    }

    /// `Rules.buildSpeed(team)`.
    pub fn build_speed(&self, team: u8) -> f32 {
        self.build_speed_multiplier * self.team_rule(team).build_speed_multiplier
    }

    /// `Rules.unitActivationDelay(team)` (ticks).
    pub fn unit_activation_delay(&self, team: u8) -> f32 {
        self.unit_factory_activation_delay + self.team_rule(team).unit_factory_activation_delay
    }

    /// `Rules.isInfiniteResources(team)` (also true for a cheating team, which
    /// never consumes resources — matching upstream `cheat` semantics).
    pub fn is_infinite_resources(&self, team: u8) -> bool {
        self.infinite_resources
            || self.team_rule(team).infinite_resources
            || self.team_rule(team).cheat
    }

    /// `Rules.isBanned(Block)` by content name.
    pub fn is_banned_block(&self, name: &str) -> bool {
        self.block_whitelist != self.banned_blocks.contains(name)
    }

    /// `Rules.isBanned(UnitType)` by content name.
    pub fn is_banned_unit(&self, name: &str) -> bool {
        self.unit_whitelist != self.banned_units.contains(name)
    }

    /// Canonical, sorted-field checksum contribution (plan 12 deviation 10).
    ///
    /// Upstream has no rules checksum. This is **not** wired into
    /// [`crate::sim::Sim::checksum`] yet: doing so requires the joint
    /// `CHECKSUM_VERSION` bump tracked as plan-12 R5. The hash is stable and
    /// usable by plan 21/23 state comparison today.
    pub fn checksum_part(&self) -> Checksum {
        let mut hasher = Hasher::new();
        // Canonicalize object keys by sorting recursively, because
        // `serde_json` is built with `preserve_order` in this workspace.
        let value = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        let mut canonical = String::new();
        write_canonical(&value, &mut canonical);
        hasher.write(canonical.as_bytes());
        hasher.finish()
    }
}

/// Appends `value` to `out` with object keys sorted recursively.
fn write_canonical(value: &serde_json::Value, out: &mut String) {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap_or_default());
                out.push(':');
                if let Some(inner) = map.get(*key) {
                    write_canonical(inner, out);
                }
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;

    #[test]
    fn mode_matches_presets() {
        use super::super::gamemode::Gamemode;

        let mut rules = Rules::default();
        assert_eq!(rules.mode(), Gamemode::Survival);

        rules.infinite_resources = true;
        assert_eq!(rules.mode(), Gamemode::Sandbox);

        rules.attack_mode = true;
        assert_eq!(rules.mode(), Gamemode::Attack);

        rules.editor = true;
        assert_eq!(rules.mode(), Gamemode::Editor);

        rules.pvp = true;
        assert_eq!(rules.mode(), Gamemode::Pvp, "pvp wins over editor/attack");
    }

    #[test]
    fn copy_is_independent_clone() {
        let mut rules = Rules::default();
        rules.win_wave = 42;
        let copy = rules.copy();
        rules.win_wave = 7;
        assert_eq!(copy.win_wave, 42);
        assert_eq!(rules.win_wave, 7);
    }

    #[test]
    fn team_multiplier_accessors_compound_with_global() {
        let mut rules = Rules {
            unit_build_speed_multiplier: 2.0,
            unit_damage_multiplier: 3.0,
            block_health_multiplier: 4.0,
            build_speed_multiplier: 5.0,
            unit_factory_activation_delay: 10.0,
            ..Rules::default()
        };
        rules.team_rule_mut(2).unit_build_speed_multiplier = 1.5;
        rules.team_rule_mut(2).unit_damage_multiplier = 2.0;
        rules.team_rule_mut(2).block_health_multiplier = 0.5;
        rules.team_rule_mut(2).build_speed_multiplier = 0.25;
        rules.team_rule_mut(2).unit_factory_activation_delay = 5.0;

        assert_eq!(rules.unit_build_speed(2), 3.0);
        assert_eq!(rules.unit_damage(2), 6.0);
        assert_eq!(rules.block_health(2), 2.0);
        assert_eq!(rules.build_speed(2), 1.25);
        assert_eq!(rules.unit_activation_delay(2), 15.0);
        // Unknown team uses the standard defaults.
        assert_eq!(rules.unit_build_speed(200), 2.0);
    }

    #[test]
    fn derelict_defaults_are_unprotected() {
        let rules = Rules::default();
        let derelict = crate::game::team::DERELICT.0;
        assert!(!rules.team_rule(derelict).protect_cores);
        assert!(!rules.team_rule(derelict).check_placement);
        assert_eq!(rules.build_radius(derelict), 0.0);
        // A normal team keeps the enemy core build radius.
        assert_eq!(rules.build_radius(crate::game::team::SHARDED.0), 400.0);
    }

    #[test]
    fn bans_respect_whitelist_flag() {
        let mut rules = Rules::default();
        rules.banned_blocks.insert("conveyor".to_owned());
        assert!(rules.is_banned_block("conveyor"));
        assert!(!rules.is_banned_block("router"));
        rules.block_whitelist = true;
        assert!(!rules.is_banned_block("conveyor"));
        assert!(rules.is_banned_block("router"));

        rules.banned_units.insert("dagger".to_owned());
        assert!(rules.is_banned_unit("dagger"));
        rules.unit_whitelist = true;
        assert!(!rules.is_banned_unit("dagger"));
    }

    #[test]
    fn has_env_masks_bits() {
        let mut rules = Rules::default();
        rules.env = 0b1010;
        assert!(rules.has_env(0b0010));
        assert!(rules.has_env(0b1000));
        assert!(!rules.has_env(0b0100));
    }

    #[test]
    fn retain_content_fields_overwrites_forced_and_patched() {
        let mut source = Rules::default();
        source.spawns.push(SpawnGroup {
            type_: "dagger".to_owned(),
            ..SpawnGroup::default()
        });
        source.objective_flags.insert("captured".to_owned());
        source.banned_blocks.insert("mod-processor".to_owned());
        source.banned_units.insert("vanilla-dagger".to_owned());
        source.loadout = vec![crate::io::json::content_serde::JsonItemStack {
            item: Some("mod-ore".to_owned()),
            amount: 5,
        }];

        let mut target = Rules::default();
        target.banned_blocks.insert("keep-me".to_owned());
        target.banned_units.insert("keep-unit".to_owned());
        target.loadout.clear();

        target.retain_content_fields(&source, |name| name.starts_with("mod-"));

        assert_eq!(target.spawns.len(), 1);
        // Patched ban content came across wholesale.
        assert!(target.banned_blocks.contains("mod-processor"));
        assert!(!target.banned_blocks.contains("keep-me"));
        // A vanilla unit inside bannedUnits is *not* patch content, so no copy.
        assert!(target.banned_units.contains("keep-unit"));
        // Loadout contains patched content -> copied.
        assert_eq!(target.loadout.len(), 1);
    }

    #[test]
    fn checksum_part_is_stable_and_sensitive() {
        let rules = Rules::default();
        assert_eq!(rules.checksum_part(), rules.checksum_part());

        let mut changed = Rules::default();
        changed.win_wave = rules.win_wave + 1;
        assert_ne!(rules.checksum_part(), changed.checksum_part());

        // Team order must not matter: the JSON map keying is canonical.
        let mut a = Rules::default();
        a.team_rule_mut(3).unit_damage_multiplier = 2.0;
        a.team_rule_mut(1).unit_damage_multiplier = 4.0;
        let mut b = Rules::default();
        b.team_rule_mut(1).unit_damage_multiplier = 4.0;
        b.team_rule_mut(3).unit_damage_multiplier = 2.0;
        assert_eq!(a.checksum_part(), b.checksum_part());
    }
}

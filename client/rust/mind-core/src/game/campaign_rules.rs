// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `CampaignRules`/`Difficulty` (plan 12 M3).
//!
//! Ported from `core/src/mindustry/game/CampaignRules.java` and
//! `core/src/mindustry/game/Difficulty.java`. `CampaignRules` is a persisted
//! per-planet modifier set (`<planet>-campaign-rules`); `apply` folds it into a
//! match [`Rules`] exactly like upstream, including the RTS-AI toggle on the
//! wave team. The controller reset that upstream performs when the toggle flips
//! during a live game is plan 11's hook: [`CampaignRules::apply`] returns
//! whether the swap happened so the caller can fire the team event.

use serde::{Deserialize, Serialize};

use super::rules::Rules;
use crate::content::BundleView;
use crate::content::registries::planets::PlanetDef;

/// Campaign difficulty preset (`Difficulty`).
///
/// Field order and values match the Java enum constructor
/// `(enemyHealthMultiplier, enemySpawnMultiplier, waveTimeMultiplier)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Difficulty {
    /// `casual(0.5, 0.5, 2)`.
    Casual,
    /// `easy(1, 0.75, 1.5)`.
    Easy,
    /// `normal(1, 1, 1)`.
    #[default]
    Normal,
    /// `hard(1.25, 1.5, 0.8)`.
    Hard,
    /// `eradication(1.5, 2, 0.6)`.
    Eradication,
}

/// Every difficulty in declaration order (`Difficulty.all`).
pub const ALL_DIFFICULTIES: [Difficulty; 5] = [
    Difficulty::Casual,
    Difficulty::Easy,
    Difficulty::Normal,
    Difficulty::Hard,
    Difficulty::Eradication,
];

impl Difficulty {
    /// Lowercase enum name (bundle suffix and JSON value).
    pub const fn name(self) -> &'static str {
        match self {
            Difficulty::Casual => "casual",
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
            Difficulty::Eradication => "eradication",
        }
    }

    /// `Difficulty.enemyHealthMultiplier`.
    pub const fn enemy_health_multiplier(self) -> f32 {
        match self {
            Difficulty::Casual => 0.5,
            Difficulty::Easy => 1.0,
            Difficulty::Normal => 1.0,
            Difficulty::Hard => 1.25,
            Difficulty::Eradication => 1.5,
        }
    }

    /// `Difficulty.enemySpawnMultiplier`.
    pub const fn enemy_spawn_multiplier(self) -> f32 {
        match self {
            Difficulty::Casual => 0.5,
            Difficulty::Easy => 0.75,
            Difficulty::Normal => 1.0,
            Difficulty::Hard => 1.5,
            Difficulty::Eradication => 2.0,
        }
    }

    /// `Difficulty.waveTimeMultiplier`.
    pub const fn wave_time_multiplier(self) -> f32 {
        match self {
            Difficulty::Casual => 2.0,
            Difficulty::Easy => 1.5,
            Difficulty::Normal => 1.0,
            Difficulty::Hard => 0.8,
            Difficulty::Eradication => 0.6,
        }
    }

    /// `Difficulty.percentStat`: `[stat]/[negstat]` colored delta string.
    pub fn percent_stat(value: f32) -> String {
        let delta = (value * 100.0 - 100.0) as i32;
        let tag = if delta > 0 { "[negstat]" } else { "[stat]" };
        format!("{tag}{delta:+}%[]")
    }

    /// `Difficulty.percentStatNeg`: inverted-color delta string.
    pub fn percent_stat_neg(value: f32) -> String {
        let delta = (value * 100.0 - 100.0) as i32;
        let tag = if delta > 0 { "[stat]" } else { "[negstat]" };
        format!("{tag}{delta:+}%[]")
    }

    /// `Difficulty.localized`: `difficulty.<name>` bundle key, falling back to
    /// the enum name when the bundle is absent.
    pub fn localized_with(self, bundle: &dyn BundleView) -> String {
        let key = format!("difficulty.{}", self.name());
        bundle.get_or(&key, self.name())
    }

    /// `Difficulty.localized()`.
    pub fn localized(self) -> String {
        self.localized_with(&crate::content::MemoryBundle::new())
    }

    /// `Difficulty.info()`: newline-joined modifier lines, or the
    /// `difficulty.nomodifiers` marker when nothing is modified.
    pub fn info_with(self, bundle: &dyn BundleView) -> String {
        let mut out = String::new();
        let mut push = |key: &str, value: f32, neg: bool| {
            if value == 1.0 {
                return;
            }
            let delta = if neg {
                Difficulty::percent_stat_neg(value)
            } else {
                Difficulty::percent_stat(value)
            };
            let line = bundle.get_or(key, key).replace('@', &delta);
            out.push_str(&line);
            out.push('\n');
        };
        push(
            "difficulty.enemyHealthMultiplier",
            self.enemy_health_multiplier(),
            false,
        );
        push(
            "difficulty.enemySpawnMultiplier",
            self.enemy_spawn_multiplier(),
            false,
        );
        push(
            "difficulty.waveTimeMultiplier",
            self.wave_time_multiplier(),
            true,
        );
        if out.is_empty() {
            bundle.get_or("difficulty.nomodifiers", "difficulty.nomodifiers")
        } else {
            out
        }
    }

    /// `Difficulty.info()` with the empty bundle.
    pub fn info(self) -> String {
        self.info_with(&crate::content::MemoryBundle::new())
    }
}

/// Persisted per-planet campaign rules (`CampaignRules`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CampaignRules {
    /// Difficulty preset.
    pub difficulty: Difficulty,
    /// Static + dynamic fog of war.
    pub fog: bool,
    /// Enemy spawn points are hidden.
    pub hide_spawns: bool,
    /// Enemy bases may launch invasions.
    pub sector_invasion: bool,
    /// Enemy AI targets randomly.
    pub random_wave_ai: bool,
    /// Legacy launch-pad sector import/export is simulated.
    pub legacy_launch_pads: bool,
    /// Enemy team runs the RTS base AI while in attack mode.
    pub rts_ai: bool,
    /// Loss clears the sector save.
    pub clear_sector_on_lose: bool,
    /// Pause is disabled in singleplayer.
    pub pause_disabled: bool,
}

impl Default for CampaignRules {
    fn default() -> Self {
        Self {
            difficulty: Difficulty::Normal,
            fog: false,
            hide_spawns: false,
            sector_invasion: false,
            random_wave_ai: false,
            legacy_launch_pads: false,
            rts_ai: false,
            clear_sector_on_lose: false,
            pause_disabled: false,
        }
    }
}

/// Result of [`CampaignRules::apply`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CampaignApply {
    /// The wave team's `rtsAi` flag actually changed.
    pub rts_swapped: bool,
    /// The RTS-AI value now written to the wave team (only meaningful when
    /// `planet.showRtsAIRule`).
    pub rts_enabled: bool,
}

impl CampaignRules {
    /// `CampaignRules.apply(planet, rules)`.
    ///
    /// `is_game` mirrors `Vars.state.isGame()` for the controller-reset guard;
    /// upstream resets every non-player unit controller of the wave team when
    /// `rts_ai` toggles during a live game. That side effect is plan 11's; this
    /// method reports the swap through [`CampaignApply::rts_swapped`].
    pub fn apply(&self, planet: &PlanetDef, rules: &mut Rules, is_game: bool) -> CampaignApply {
        rules.static_fog = self.fog;
        rules.fog = self.fog;
        rules.hide_spawns = self.hide_spawns;
        rules.random_wave_ai = self.random_wave_ai;
        rules.pause_disabled = self.pause_disabled;
        rules.objective_timer_multiplier = self.difficulty.wave_time_multiplier();

        let mut result = CampaignApply::default();
        if planet.show_rts_ai_rule {
            // Enabling waves forces attack mode off, which disables RTS AI.
            let enabled = self.rts_ai && rules.attack_mode;
            let wave_team = rules.wave_team;
            let swapped = rules.team_rule(wave_team).rts_ai != enabled;
            {
                let rule = rules.team_rule_mut(wave_team);
                rule.rts_ai = enabled;
                rule.rts_max_squad = 15;
            }
            result.rts_enabled = enabled;
            result.rts_swapped = swapped && is_game;
        }

        let wave_team = rules.wave_team;
        let health = self.difficulty.enemy_health_multiplier();
        let spawn = self.difficulty.enemy_spawn_multiplier();
        let rule = rules.team_rule_mut(wave_team);
        rule.block_health_multiplier = health;
        rule.unit_health_multiplier = health;
        rule.unit_cost_multiplier = 1.0 / spawn;
        rule.unit_build_speed_multiplier = spawn;

        result
    }

    /// Loads `<planet>-campaign-rules` from a settings store
    /// (`JsonIO`/`Settings.getJson` shape).
    pub fn read(settings: &crate::io::settings::SettingsStore, planet_name: &str) -> CampaignRules {
        settings.get_json_or(
            &format!("{planet_name}-campaign-rules"),
            CampaignRules::default,
        )
    }

    /// Writes `<planet>-campaign-rules` (`Planet.saveRules`).
    pub fn write(
        &self,
        settings: &mut crate::io::settings::SettingsStore,
        planet_name: &str,
    ) -> Result<(), crate::io::IoError> {
        settings.put_json(&format!("{planet_name}-campaign-rules"), self)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::content::registries::planets::PlanetDef;
    use crate::content::{MemoryBundle, MemoryUnlockStore};

    fn planet(name: &str, show_rts: bool) -> PlanetDef {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        let mut planet = PlanetDef::new(name, None, 1.0, &bundle, &store);
        planet.show_rts_ai_rule = show_rts;
        planet
    }

    #[test]
    fn difficulty_table_matches_java() {
        assert_eq!(Difficulty::Casual.enemy_health_multiplier(), 0.5);
        assert_eq!(Difficulty::Easy.enemy_spawn_multiplier(), 0.75);
        assert_eq!(Difficulty::Normal.wave_time_multiplier(), 1.0);
        assert_eq!(Difficulty::Hard.enemy_health_multiplier(), 1.25);
        assert_eq!(Difficulty::Eradication.enemy_spawn_multiplier(), 2.0);
        assert_eq!(ALL_DIFFICULTIES.len(), 5);
        assert_eq!(Difficulty::default(), Difficulty::Normal);
    }

    #[test]
    fn percent_stat_formats_delta() {
        assert_eq!(Difficulty::percent_stat(1.25), "[negstat]+25%[]");
        assert_eq!(Difficulty::percent_stat(0.5), "[stat]-50%[]");
        // f32 math matches Java's `(int)(val * 100 - 100)` truncation.
        assert_eq!(Difficulty::percent_stat_neg(0.6), "[negstat]-39%[]");
        assert_eq!(Difficulty::percent_stat_neg(2.0), "[stat]+100%[]");
    }

    #[test]
    fn apply_sets_global_and_team_fields() {
        let mut rules = Rules {
            attack_mode: true,
            ..Rules::default()
        };
        rules.wave_team = 2;
        let mut campaign = CampaignRules {
            fog: true,
            hide_spawns: true,
            random_wave_ai: true,
            pause_disabled: true,
            difficulty: Difficulty::Hard,
            rts_ai: true,
            ..CampaignRules::default()
        };
        let planet = planet("serpulo", true);
        let result = campaign.apply(&planet, &mut rules, false);

        assert!(rules.fog);
        assert!(rules.static_fog);
        assert!(rules.hide_spawns);
        assert!(rules.random_wave_ai);
        assert!(rules.pause_disabled);
        assert_eq!(rules.objective_timer_multiplier, 0.8);
        assert!(rules.team_rule(2).rts_ai);
        assert_eq!(rules.team_rule(2).rts_max_squad, 15);
        assert_eq!(
            rules.team_rule(2).unit_health_multiplier,
            Difficulty::Hard.enemy_health_multiplier()
        );
        assert_eq!(
            rules.team_rule(2).unit_cost_multiplier,
            1.0 / Difficulty::Hard.enemy_spawn_multiplier()
        );
        // Not a live game: swap is not reported even though the flag changed.
        assert!(!result.rts_swapped);
        assert!(result.rts_enabled);

        // Live game + flag change -> swap reported (plan 11 resets controllers).
        campaign.rts_ai = false;
        let result = campaign.apply(&planet, &mut rules, true);
        assert!(result.rts_swapped);
        assert!(!result.rts_enabled);
    }

    #[test]
    fn rts_ai_is_disabled_without_attack_mode() {
        let mut rules = Rules::default();
        rules.attack_mode = false;
        let campaign = CampaignRules {
            rts_ai: true,
            ..CampaignRules::default()
        };
        let planet = planet("erekir", true);
        let result = campaign.apply(&planet, &mut rules, false);
        assert!(!rules.team_rule(rules.wave_team).rts_ai);
        assert!(!result.rts_enabled);
    }

    #[test]
    fn roundtrip_and_defaults() {
        let campaign = CampaignRules {
            difficulty: Difficulty::Eradication,
            fog: true,
            ..CampaignRules::default()
        };
        let json = serde_json::to_string(&campaign).unwrap();
        assert!(json.contains("\"difficulty\":\"eradication\""));
        let back: CampaignRules = serde_json::from_str(&json).unwrap();
        assert_eq!(back, campaign);
        // Missing fields default.
        let empty: CampaignRules = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.difficulty, Difficulty::Normal);
        assert!(!empty.fog);
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `mindustry.game.Gamemode` presets (plan 12 M0).
//!
//! The rule mutations are the exact `Gamemode.apply(Rules)` table; `valid` is
//! expressed over spawn/team counts (the map registry supplies them via
//! [`MapView`]) because `mind-core::maps::Map` is owned by plan 06.

use super::rules::{Rules, TIME_TO_MINUTES};

/// A preset ruleset (`Gamemode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gamemode {
    /// Campaign / classic waves (`survival`).
    Survival,
    /// Infinite-resource sandbox.
    Sandbox,
    /// Attack mode against AI bases.
    Attack,
    /// Player-vs-player.
    Pvp,
    /// Map editor (hidden from the menu).
    Editor,
}

/// Map metadata the validators read (`Map.spawns`/`Map.teams`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MapView {
    /// Number of spawn points (`Map.spawns`).
    pub spawns: usize,
    /// Number of teams present in the map (`Map.teams.size`).
    pub teams: usize,
}

impl MapView {
    /// Builds a view from raw counts.
    pub const fn new(spawns: usize, teams: usize) -> Self {
        Self { spawns, teams }
    }
}

impl Gamemode {
    /// Every preset in declaration order (`Gamemode.all`).
    pub const ALL: [Gamemode; 5] = [
        Gamemode::Survival,
        Gamemode::Sandbox,
        Gamemode::Attack,
        Gamemode::Pvp,
        Gamemode::Editor,
    ];

    /// Stable lowercase name (also the bundle key stem `mode.<name>.name`).
    pub const fn name(self) -> &'static str {
        match self {
            Gamemode::Survival => "survival",
            Gamemode::Sandbox => "sandbox",
            Gamemode::Attack => "attack",
            Gamemode::Pvp => "pvp",
            Gamemode::Editor => "editor",
        }
    }

    /// `Gamemode.hidden` — editor is withheld from the normal mode list.
    pub const fn hidden(self) -> bool {
        matches!(self, Gamemode::Editor)
    }

    /// `Gamemode.apply(Rules)` — the exact upstream preset table.
    pub fn apply(self, rules: &mut Rules) {
        match self {
            Gamemode::Survival => {
                rules.wave_timer = true;
                rules.waves = true;
            }
            Gamemode::Sandbox => {
                rules.infinite_resources = true;
                rules.allow_edit_rules = true;
                rules.waves = true;
                rules.wave_timer = false;
            }
            Gamemode::Attack => {
                rules.attack_mode = true;
                rules.wave_timer = true;
                rules.wave_spacing = 2.0 * TIME_TO_MINUTES;
                let wave_team = rules.wave_team;
                rules.team_rule_mut(wave_team).infinite_resources = true;
            }
            Gamemode::Pvp => {
                rules.pvp = true;
                rules.enemy_core_build_radius = 600.0;
                rules.build_cost_multiplier = 1.0;
                rules.build_speed_multiplier = 1.0;
                rules.unit_build_speed_multiplier = 2.0;
                rules.attack_mode = true;
            }
            Gamemode::Editor => {
                rules.infinite_resources = true;
                rules.instant_build = true;
                rules.editor = true;
                rules.waves = false;
                rules.wave_timer = false;
            }
        }
    }

    /// `Gamemode.valid(Map)`.
    pub fn valid(self, map: MapView) -> bool {
        match self {
            Gamemode::Survival => map.spawns > 0,
            Gamemode::Attack | Gamemode::Pvp => map.teams > 1,
            Gamemode::Sandbox | Gamemode::Editor => true,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;

    #[test]
    fn survival_enables_waves() {
        let mut rules = Rules::default();
        Gamemode::Survival.apply(&mut rules);
        assert!(rules.wave_timer);
        assert!(rules.waves);
    }

    #[test]
    fn sandbox_disables_timer_but_enables_waves_and_edit() {
        let mut rules = Rules::default();
        Gamemode::Sandbox.apply(&mut rules);
        assert!(rules.infinite_resources);
        assert!(rules.allow_edit_rules);
        assert!(rules.waves);
        assert!(!rules.wave_timer);
    }

    #[test]
    fn attack_sets_spacing_and_wave_team_infinite() {
        let mut rules = Rules::default();
        rules.wave_team = 2;
        Gamemode::Attack.apply(&mut rules);
        assert!(rules.attack_mode);
        assert!(rules.wave_timer);
        assert_eq!(rules.wave_spacing, 2.0 * TIME_TO_MINUTES);
        assert!(rules.team_rule(2).infinite_resources);
    }

    #[test]
    fn pvp_table_matches_upstream() {
        let mut rules = Rules::default();
        Gamemode::Pvp.apply(&mut rules);
        assert!(rules.pvp);
        assert_eq!(rules.enemy_core_build_radius, 600.0);
        assert_eq!(rules.build_cost_multiplier, 1.0);
        assert_eq!(rules.build_speed_multiplier, 1.0);
        assert_eq!(rules.unit_build_speed_multiplier, 2.0);
        assert!(rules.attack_mode);
    }

    #[test]
    fn editor_is_hidden_and_instant() {
        let mut rules = Rules::default();
        Gamemode::Editor.apply(&mut rules);
        assert!(Gamemode::Editor.hidden());
        assert!(rules.editor);
        assert!(rules.infinite_resources);
        assert!(rules.instant_build);
        assert!(!rules.waves);
        assert!(!rules.wave_timer);
    }

    #[test]
    fn validators_gate_on_map_metadata() {
        let empty = MapView::new(0, 1);
        let single = MapView::new(2, 1);
        let multi = MapView::new(2, 3);
        assert!(!Gamemode::Survival.valid(empty));
        assert!(Gamemode::Survival.valid(single));
        assert!(!Gamemode::Attack.valid(single));
        assert!(Gamemode::Attack.valid(multi));
        assert!(Gamemode::Sandbox.valid(empty));
        assert_eq!(Gamemode::ALL.len(), 5);
    }

    #[test]
    fn mode_round_trips_through_rules() {
        for mode in Gamemode::ALL {
            let mut rules = Rules::default();
            mode.apply(&mut rules);
            if mode == Gamemode::Editor {
                // pvp/attack/editor ordering: editor flags do not set pvp/attack.
                assert_eq!(rules.mode(), Gamemode::Editor);
            } else if mode == Gamemode::Attack {
                assert_eq!(rules.mode(), Gamemode::Attack);
            } else if mode == Gamemode::Sandbox {
                assert_eq!(rules.mode(), Gamemode::Sandbox);
            } else if mode == Gamemode::Pvp {
                assert_eq!(rules.mode(), Gamemode::Pvp);
            } else {
                assert_eq!(rules.mode(), Gamemode::Survival);
            }
        }
    }
}

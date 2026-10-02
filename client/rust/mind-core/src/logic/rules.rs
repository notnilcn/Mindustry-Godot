// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LogicRule` application.
//!
//! Ported from `LExecutor.SetRuleI.run` (`core/src/mindustry/logic/LExecutor.java`).
//! State fields (`state.wave`/`state.wavetime`), map-area edits, content bans and
//! the ambient-light color need plan-05 state, plan-06 world and plan-12 content
//! sinks; those return `false` (unapplied) until the owning plans land.

use crate::game::rules::Rules;
use crate::logic::statement::LogicRule;
use crate::logic::value::LVar;

/// Applies one `setrule` entry. Returns whether the rule was applied here.
pub fn apply_to_rules(
    rules: &mut Rules,
    rule: LogicRule,
    value: &LVar,
    p1: &LVar,
    _p2: &LVar,
    _p3: &LVar,
    _p4: &LVar,
) -> bool {
    let numf = value.numf();
    match rule {
        LogicRule::WaveTimer => rules.wave_timer = value.as_bool(),
        LogicRule::Waves => rules.waves = value.as_bool(),
        LogicRule::WaveSending => rules.wave_sending = value.as_bool(),
        LogicRule::AttackMode => rules.attack_mode = value.as_bool(),
        LogicRule::CanGameOver => rules.can_game_over = value.as_bool(),
        LogicRule::PauseDisabled => rules.pause_disabled = value.as_bool(),
        LogicRule::WaveSpacing => rules.wave_spacing = numf * 60.0,
        LogicRule::EnemyCoreBuildRadius => rules.enemy_core_build_radius = numf * 8.0,
        LogicRule::DropZoneRadius => rules.drop_zone_radius = numf * 8.0,
        LogicRule::UnitCap => rules.unit_cap = value.numi().max(0),
        LogicRule::SolarMultiplier => rules.solar_multiplier = numf.max(0.0),
        LogicRule::DragMultiplier => rules.drag_multiplier = numf.max(0.0),
        LogicRule::BuildSpeed
        | LogicRule::UnitHealth
        | LogicRule::UnitBuildSpeed
        | LogicRule::UnitMineSpeed
        | LogicRule::UnitCost
        | LogicRule::UnitDamage
        | LogicRule::BlockHealth
        | LogicRule::BlockDamage
        | LogicRule::RtsMinWeight
        | LogicRule::RtsMinSquad => {
            let Some(team) = p1.team() else {
                return false;
            };
            let team_rule = rules.team_rule_mut(team);
            match rule {
                LogicRule::BuildSpeed => team_rule.build_speed_multiplier = numf.clamp(0.001, 50.0),
                LogicRule::UnitHealth => team_rule.unit_health_multiplier = numf.max(0.001),
                LogicRule::UnitBuildSpeed => {
                    team_rule.unit_build_speed_multiplier = numf.clamp(0.0, 50.0)
                }
                LogicRule::UnitMineSpeed => team_rule.unit_mine_speed_multiplier = numf.max(0.0),
                LogicRule::UnitCost => team_rule.unit_cost_multiplier = numf.max(0.0),
                LogicRule::UnitDamage => team_rule.unit_damage_multiplier = numf.max(0.0),
                LogicRule::BlockHealth => team_rule.block_health_multiplier = numf.max(0.001),
                LogicRule::BlockDamage => team_rule.block_damage_multiplier = numf.max(0.0),
                LogicRule::RtsMinWeight => team_rule.rts_min_weight = numf,
                LogicRule::RtsMinSquad => team_rule.rts_min_squad = numf as i32,
                _ => unreachable!(),
            }
        }
        // state/world/content/color rule sinks are owned by plans 05/06/12.
        LogicRule::CurrentWaveTime
        | LogicRule::Wave
        | LogicRule::MapArea
        | LogicRule::Lighting
        | LogicRule::AmbientLight
        | LogicRule::UnitLight
        | LogicRule::MusicVolume
        | LogicRule::Ban
        | LogicRule::Unban => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::value::LVar;

    fn n(v: f64) -> LVar {
        LVar::num_const("___", v)
    }

    #[test]
    fn applies_numeric_and_bool_rules() {
        let mut rules = Rules::default();
        assert!(apply_to_rules(
            &mut rules,
            LogicRule::WaveSpacing,
            &n(10.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        ));
        assert_eq!(rules.wave_spacing, 600.0);
        assert!(apply_to_rules(
            &mut rules,
            LogicRule::Waves,
            &n(1.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        ));
        assert!(rules.waves);
    }

    #[test]
    fn state_gated_rules_are_not_applied() {
        let mut rules = Rules::default();
        assert!(!apply_to_rules(
            &mut rules,
            LogicRule::Wave,
            &n(3.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        ));
    }
}

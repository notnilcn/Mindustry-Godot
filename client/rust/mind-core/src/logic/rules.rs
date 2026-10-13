// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LogicRule` application.
//!
//! Ported from `LExecutor.SetRuleI.run` (`core/src/mindustry/logic/LExecutor.java`
//! l.1779-1865) and `checkMapArea` (l.1866-1910). The caller owns the live
//! `state.rules`/`state.wave`/`state.wavetime` halves (campaign session records
//! or the standalone [`LogicWorldState`] copy); this module is pure.

use crate::content::ContentType;
use crate::game::rules::Rules;
use crate::logic::globals::LogicContentInfo;
use crate::logic::statement::LogicRule;
use crate::logic::value::{LVar, LogicObject};
use crate::logic::world::{LogicWorldState, color_from_double};

/// Applies one `setrule` entry against the live rule/state halves.
///
/// `wave`/`wavetime` are the live `state.wave`/`state.wavetime` (ticks);
/// `map_w`/`map_h` the world size for `checkMapArea`; `content` the
/// [`LogicContentInfo`] name table used to resolve `ban`/`unban` content ids
/// to the name sets the port's `Rules` stores. Returns whether the rule was
/// applied (false = no source for the target, e.g. `ban` without content info).
#[allow(clippy::too_many_arguments)]
pub fn apply_rule(
    rules: &mut Rules,
    wave: &mut i32,
    wavetime: &mut f32,
    map_w: i32,
    map_h: i32,
    content: Option<&LogicContentInfo>,
    rule: LogicRule,
    value: &LVar,
    p1: &LVar,
    p2: &LVar,
    _p3: &LVar,
    _p4: &LVar,
) -> bool {
    let numf = value.numf();
    match rule {
        LogicRule::WaveTimer => rules.wave_timer = value.as_bool(),
        LogicRule::Wave => *wave = value.numi().max(1),
        LogicRule::CurrentWaveTime => *wavetime = (numf * 60.0).max(0.0),
        LogicRule::Waves => rules.waves = value.as_bool(),
        LogicRule::WaveSending => rules.wave_sending = value.as_bool(),
        LogicRule::AttackMode => rules.attack_mode = value.as_bool(),
        LogicRule::WaveSpacing => rules.wave_spacing = numf * 60.0,
        LogicRule::EnemyCoreBuildRadius => rules.enemy_core_build_radius = numf * 8.0,
        LogicRule::DropZoneRadius => rules.drop_zone_radius = numf * 8.0,
        LogicRule::UnitCap => rules.unit_cap = value.numi().max(0),
        LogicRule::Lighting => rules.lighting = value.as_bool(),
        LogicRule::CanGameOver => rules.can_game_over = value.as_bool(),
        LogicRule::PauseDisabled => rules.pause_disabled = value.as_bool(),
        LogicRule::MusicVolume => rules.music_volume = numf.clamp(0.0, 1.0),
        LogicRule::MapArea => {
            check_map_area(
                rules,
                map_w,
                map_h,
                p1.numi(),
                p2.numi(),
                _p3.numi(),
                _p4.numi(),
            );
            return true;
        }
        LogicRule::AmbientLight => rules.ambient_light = color_from_double(value.num()),
        LogicRule::UnitLight => rules.unit_light = value.as_bool(),
        LogicRule::SolarMultiplier => rules.solar_multiplier = numf.max(0.0),
        LogicRule::DragMultiplier => rules.drag_multiplier = numf.max(0.0),
        LogicRule::Ban | LogicRule::Unban => {
            let Some(info) = content else {
                return false;
            };
            let Some(LogicObject::Content(c)) = value.value_obj() else {
                return false;
            };
            let Some(name) = info.name(c.type_, c.id).map(str::to_owned) else {
                return false;
            };
            match c.type_ {
                ContentType::Block => {
                    if rule == LogicRule::Ban {
                        rules.banned_blocks.insert(name);
                    } else {
                        rules.banned_blocks.shift_remove(&name);
                    }
                }
                ContentType::Unit => {
                    if rule == LogicRule::Ban {
                        rules.banned_units.insert(name);
                    } else {
                        rules.banned_units.shift_remove(&name);
                    }
                }
                _ => return false,
            }
        }
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
    }
    true
}

/// Back-compat wrapper over the standalone [`LogicWorldState`] copy (tests and
/// the harness path before T4's live accessors).
pub fn apply_to_state(
    state: &mut LogicWorldState,
    rule: LogicRule,
    value: &LVar,
    p1: &LVar,
    p2: &LVar,
    p3: &LVar,
    p4: &LVar,
) -> bool {
    let (map_w, map_h) = (state.map_width, state.map_height);
    let mut wave = state.wave;
    let mut wavetime = state.wavetime;
    let applied = apply_rule(
        &mut state.rules,
        &mut wave,
        &mut wavetime,
        map_w,
        map_h,
        None,
        rule,
        value,
        p1,
        p2,
        p3,
        p4,
    );
    state.wave = wave;
    state.wavetime = wavetime;
    applied
}

/// `checkMapArea(...)` with `set = true` (host/offline apply); returns whether
/// the area was already set to this value (`Logic.java`/`LExecutor.java:1866`).
fn check_map_area(rules: &mut Rules, map_w: i32, map_h: i32, x: i32, y: i32, w: i32, h: i32) {
    let x = x.max(0);
    let y = y.max(0);
    let w = if map_w > 0 { w.min(map_w) } else { w };
    let h = if map_h > 0 { h.min(map_h) } else { h };
    let full = x == 0 && y == 0 && (map_w <= 0 || w == map_w) && (map_h <= 0 || h == map_h);

    if rules.limit_map_area {
        if rules.limit_x == x
            && rules.limit_y == y
            && rules.limit_width == w
            && rules.limit_height == h
        {
            return;
        }
        if full {
            // Covers the whole map: disable the rule.
            rules.limit_map_area = false;
            return;
        }
    } else if full {
        // Already disabled, nothing to change.
        return;
    }

    rules.limit_map_area = true;
    rules.limit_x = x;
    rules.limit_y = y;
    rules.limit_width = w;
    rules.limit_height = h;
}

/// Back-compat wrapper over the pure `Rules` half (no live session/state).
pub fn apply_to_rules(
    rules: &mut Rules,
    rule: LogicRule,
    value: &LVar,
    p1: &LVar,
    p2: &LVar,
    p3: &LVar,
    p4: &LVar,
) -> bool {
    let mut wave = 1;
    let mut wavetime = 0.0;
    apply_rule(
        rules,
        &mut wave,
        &mut wavetime,
        0,
        0,
        None,
        rule,
        value,
        p1,
        p2,
        p3,
        p4,
    )
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
    fn state_backed_rules_apply() {
        let mut state = LogicWorldState::new();
        assert!(apply_to_state(
            &mut state,
            LogicRule::Wave,
            &n(7.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        ));
        assert_eq!(state.wave, 7);
        assert!(apply_to_state(
            &mut state,
            LogicRule::CurrentWaveTime,
            &n(2.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        ));
        assert_eq!(state.wavetime, 120.0);
        assert!(apply_to_state(
            &mut state,
            LogicRule::Lighting,
            &n(1.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        ));
        assert!(state.rules.lighting);
        // `ban`/`unban` key on content names and stay unapplied.
        assert!(!apply_to_state(
            &mut state,
            LogicRule::Ban,
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        ));
    }

    #[test]
    fn state_gated_rules_apply_to_live_halves() {
        let mut rules = Rules::default();
        let (mut wave, mut wavetime) = (1, 0.0);
        assert!(apply_rule(
            &mut rules,
            &mut wave,
            &mut wavetime,
            0,
            0,
            None,
            LogicRule::Wave,
            &n(3.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        ));
        assert_eq!(wave, 3);
        assert!(apply_rule(
            &mut rules,
            &mut wave,
            &mut wavetime,
            0,
            0,
            None,
            LogicRule::CurrentWaveTime,
            &n(2.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        ));
        assert_eq!(wavetime, 120.0);
        // `mapArea` needs the map size; a 300x300 world clamps w/h and enables.
        assert!(apply_rule(
            &mut rules,
            &mut wave,
            &mut wavetime,
            300,
            300,
            None,
            LogicRule::MapArea,
            &n(0.0),
            &n(10.0),
            &n(0.0),
            &n(256.0),
            &n(256.0),
        ));
        assert!(rules.limit_map_area);
        assert_eq!(rules.limit_width, 256);
        // Whole map disables the limit again.
        assert!(apply_rule(
            &mut rules,
            &mut wave,
            &mut wavetime,
            300,
            300,
            None,
            LogicRule::MapArea,
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(300.0),
            &n(300.0),
        ));
        assert!(!rules.limit_map_area);
    }
}

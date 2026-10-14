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
/// Result of one `setrule` application (`SetRuleI.run`,
/// `J/logic/LExecutor.java:1779-1858`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RuleChange {
    /// The rule had a valid target (the pre-T5 `apply_rule` return).
    pub applied: bool,
    /// The value actually changed (`state.rules.X != value`).
    pub changed: bool,
    /// Previous `mapArea` rect when the area changed
    /// (`(-1, -1, 0, 0)` when the area was not limited before).
    pub map_area: Option<(i32, i32, i32, i32)>,
}

/// `if(*slot != v){ *slot = v; true } else { false }` — per-arm change
/// tracking for [`apply_rule`].
fn set<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot != value {
        *slot = value;
        true
    } else {
        false
    }
}

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
    p3: &LVar,
    p4: &LVar,
) -> RuleChange {
    let numf = value.numf();
    let not_applied = RuleChange {
        applied: false,
        changed: false,
        map_area: None,
    };
    let changed = match rule {
        LogicRule::WaveTimer => set(&mut rules.wave_timer, value.as_bool()),
        LogicRule::Wave => set(wave, value.numi().max(1)),
        LogicRule::CurrentWaveTime => set(wavetime, (numf * 60.0).max(0.0)),
        LogicRule::Waves => set(&mut rules.waves, value.as_bool()),
        LogicRule::WaveSending => set(&mut rules.wave_sending, value.as_bool()),
        LogicRule::AttackMode => set(&mut rules.attack_mode, value.as_bool()),
        LogicRule::WaveSpacing => set(&mut rules.wave_spacing, numf * 60.0),
        LogicRule::EnemyCoreBuildRadius => set(&mut rules.enemy_core_build_radius, numf * 8.0),
        LogicRule::DropZoneRadius => set(&mut rules.drop_zone_radius, numf * 8.0),
        LogicRule::UnitCap => set(&mut rules.unit_cap, value.numi().max(0)),
        LogicRule::Lighting => set(&mut rules.lighting, value.as_bool()),
        LogicRule::CanGameOver => set(&mut rules.can_game_over, value.as_bool()),
        LogicRule::PauseDisabled => set(&mut rules.pause_disabled, value.as_bool()),
        LogicRule::MusicVolume => set(&mut rules.music_volume, numf.clamp(0.0, 1.0)),
        LogicRule::MapArea => {
            let map_area = check_map_area(
                rules,
                map_w,
                map_h,
                p1.numi(),
                p2.numi(),
                p3.numi(),
                p4.numi(),
            );
            return RuleChange {
                applied: true,
                changed: map_area.is_some(),
                map_area,
            };
        }
        LogicRule::AmbientLight => set(&mut rules.ambient_light, color_from_double(value.num())),
        LogicRule::UnitLight => set(&mut rules.unit_light, value.as_bool()),
        LogicRule::SolarMultiplier => set(&mut rules.solar_multiplier, numf.max(0.0)),
        LogicRule::DragMultiplier => set(&mut rules.drag_multiplier, numf.max(0.0)),
        LogicRule::Ban | LogicRule::Unban => {
            let Some(info) = content else {
                return not_applied;
            };
            let Some(LogicObject::Content(c)) = value.value_obj() else {
                return not_applied;
            };
            let Some(name) = info.name(c.type_, c.id).map(str::to_owned) else {
                return not_applied;
            };
            match c.type_ {
                ContentType::Block => {
                    if rule == LogicRule::Ban {
                        rules.banned_blocks.insert(name)
                    } else {
                        rules.banned_blocks.shift_remove(&name)
                    }
                }
                ContentType::Unit => {
                    if rule == LogicRule::Ban {
                        rules.banned_units.insert(name)
                    } else {
                        rules.banned_units.shift_remove(&name)
                    }
                }
                _ => return not_applied,
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
                return not_applied;
            };
            let team_rule = rules.team_rule_mut(team);
            match rule {
                LogicRule::BuildSpeed => set(
                    &mut team_rule.build_speed_multiplier,
                    numf.clamp(0.001, 50.0),
                ),
                LogicRule::UnitHealth => {
                    set(&mut team_rule.unit_health_multiplier, numf.max(0.001))
                }
                LogicRule::UnitBuildSpeed => set(
                    &mut team_rule.unit_build_speed_multiplier,
                    numf.clamp(0.0, 50.0),
                ),
                LogicRule::UnitMineSpeed => {
                    set(&mut team_rule.unit_mine_speed_multiplier, numf.max(0.0))
                }
                LogicRule::UnitCost => set(&mut team_rule.unit_cost_multiplier, numf.max(0.0)),
                LogicRule::UnitDamage => set(&mut team_rule.unit_damage_multiplier, numf.max(0.0)),
                LogicRule::BlockHealth => {
                    set(&mut team_rule.block_health_multiplier, numf.max(0.001))
                }
                LogicRule::BlockDamage => {
                    set(&mut team_rule.block_damage_multiplier, numf.max(0.0))
                }
                LogicRule::RtsMinWeight => set(&mut team_rule.rts_min_weight, numf),
                LogicRule::RtsMinSquad => set(&mut team_rule.rts_min_squad, numf as i32),
                _ => unreachable!(),
            }
        }
    };
    RuleChange {
        applied: true,
        changed,
        map_area: None,
    }
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
    let change = apply_rule(
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
    change.applied
}

/// `checkMapArea(x, y, w, h, true)` (`LExecutor.java:1866-1910`): applies the
/// clamped rect; returns the PREVIOUS rect when the area changed
/// (`(-1, -1, 0, 0)` when the area was not limited), else `None`.
fn check_map_area(
    rules: &mut Rules,
    map_w: i32,
    map_h: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Option<(i32, i32, i32, i32)> {
    let x = x.max(0);
    let y = y.max(0);
    let w = if map_w > 0 { w.min(map_w) } else { w };
    let h = if map_h > 0 { h.min(map_h) } else { h };
    let full = x == 0 && y == 0 && (map_w <= 0 || w == map_w) && (map_h <= 0 || h == map_h);
    let prev = if rules.limit_map_area {
        (
            rules.limit_x,
            rules.limit_y,
            rules.limit_width,
            rules.limit_height,
        )
    } else {
        (-1, -1, 0, 0)
    };

    if rules.limit_map_area {
        if rules.limit_x == x
            && rules.limit_y == y
            && rules.limit_width == w
            && rules.limit_height == h
        {
            return None;
        }
        if full {
            // Covers the whole map: disable the rule.
            rules.limit_map_area = false;
            return Some(prev);
        }
    } else if full {
        // Already disabled, nothing to change.
        return None;
    }

    rules.limit_map_area = true;
    rules.limit_x = x;
    rules.limit_y = y;
    rules.limit_width = w;
    rules.limit_height = h;
    Some(prev)
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
    .applied
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
        assert!(
            apply_rule(
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
            )
            .applied
        );
        assert_eq!(wave, 3);
        assert!(
            apply_rule(
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
            )
            .applied
        );
        assert_eq!(wavetime, 120.0);
        // `mapArea` needs the map size; a 300x300 world clamps w/h and enables.
        assert!(
            apply_rule(
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
            )
            .applied
        );
        assert!(rules.limit_map_area);
        assert_eq!(rules.limit_width, 256);
        // Whole map disables the limit again.
        assert!(
            apply_rule(
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
            )
            .applied
        );
        assert!(!rules.limit_map_area);
    }

    /// H06 T5: re-setting the same value reports `changed = false`
    /// (frozenForest/fungalPass run `setrule waveTimer x` every tick).
    #[test]
    fn unchanged_rule_reports_not_changed() {
        let mut rules = Rules {
            wave_timer: true,
            ..Default::default()
        };
        let (mut wave, mut wavetime) = (1, 0.0);
        let change = apply_rule(
            &mut rules,
            &mut wave,
            &mut wavetime,
            0,
            0,
            None,
            LogicRule::WaveTimer,
            &n(1.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        );
        assert!(change.applied);
        assert!(!change.changed, "same value is not a change");
        assert!(change.map_area.is_none());
        let change = apply_rule(
            &mut rules,
            &mut wave,
            &mut wavetime,
            0,
            0,
            None,
            LogicRule::WaveTimer,
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
            &n(0.0),
        );
        assert!(change.changed, "true -> false is a change");
    }

    /// H06 T5: `mapArea` reports the previous rect (`(-1,-1,0,0)` before the
    /// first limit) so the host can run `World.checkMapArea(prev)`.
    #[test]
    fn map_area_reports_previous_rect() {
        let mut rules = Rules::default();
        let (mut wave, mut wavetime) = (1, 0.0);
        let change = apply_rule(
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
            &n(256.0),
            &n(256.0),
        );
        assert!(change.changed);
        assert_eq!(change.map_area, Some((-1, -1, 0, 0)));
        // The same rect again: no change, no previous rect.
        let change = apply_rule(
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
            &n(256.0),
            &n(256.0),
        );
        assert!(!change.changed);
        assert_eq!(change.map_area, None);
        // Whole map: disables the limit and reports the old rect.
        let change = apply_rule(
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
        );
        assert!(change.changed);
        assert_eq!(change.map_area, Some((0, 0, 256, 256)));
    }
}

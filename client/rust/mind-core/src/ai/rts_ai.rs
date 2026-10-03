// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RtsAI` squad assignment (plan 11 §3.9).
//!
//! Ported from `core/src/mindustry/ai/RtsAI.java`. This module owns the
//! deterministic squad grouping and target-selection halves (grouping idle
//! commanded units by `flag`, nearest-unassigned target pick). The per-tick
//! `TeamData` wiring, defense triggers and `estimateStats` weighting are plan 12
//! (`RtsAI.update` is called per active `TeamData`).

use bevy_ecs::entity::Entity;

use crate::world::TilePos;

/// One candidate unit for squad BFS (`RtsAI.assignSquads` inputs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RtsUnit {
    /// Entity handle.
    pub entity: Entity,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Mobilization flag (`unit.flag`).
    pub flag: i32,
    /// Whether the unit has no command and is not attacking (`CommandAI` idle).
    pub idle: bool,
    /// Unit hit size (BFS radius term).
    pub hit_size: f32,
}

/// One enemy building the AI may defend against or attack (`RtsAI.targets`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RtsBuilding {
    /// Entity handle (identity for `assignedTargets`).
    pub entity: Entity,
    /// Tile position (identity for `invalidTarget`).
    pub pos: TilePos,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Whether this is a core (`CoreBuild`).
    pub in_core: bool,
}

/// A nearby enemy combatant for `estimateStats` (turret or unit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnemyStat {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Effective health.
    pub health: f32,
    /// Estimated DPS.
    pub dps: f32,
    /// Engagement range in world pixels.
    pub range: f32,
    /// Can hit air (`targetAir`).
    pub target_air: bool,
    /// Can hit ground (`targetGround`).
    pub target_ground: bool,
    /// Whether the *attacking* squad is air (selects the eligibility flag).
    pub air: bool,
}

/// Aggregate squad stats (`RtsAI.handleSquad` loop).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SquadStats {
    /// Average x.
    pub x: f32,
    /// Average y.
    pub y: f32,
    /// Total health.
    pub health: f32,
    /// Total DPS.
    pub dps: f32,
    /// All members can hit air.
    pub target_air: bool,
    /// All members can hit ground.
    pub target_ground: bool,
    /// First member is naval (`WaterMovec`).
    pub naval: bool,
    /// First member is flying.
    pub flying: bool,
    /// First member flag is non-zero.
    pub numbered: bool,
}

impl SquadStats {
    /// Aggregates per-unit stat rows (`handleSquad` loop).
    pub fn aggregate(units: &[SquadMember]) -> Self {
        let mut stats = SquadStats {
            x: 0.0,
            y: 0.0,
            health: 0.0,
            dps: 0.0,
            target_air: true,
            target_ground: true,
            naval: false,
            flying: false,
            numbered: false,
        };
        if let Some(first) = units.first() {
            stats.naval = first.naval;
            stats.flying = first.flying;
            stats.numbered = first.flag != 0;
        }
        for unit in units {
            if !unit.target_air {
                stats.target_air = false;
            }
            if !unit.target_ground {
                stats.target_ground = false;
            }
            stats.x += unit.x;
            stats.y += unit.y;
            stats.health += unit.health;
            stats.dps += unit.dps;
        }
        let len = units.len().max(1) as f32;
        stats.x /= len;
        stats.y /= len;
        stats
    }
}

/// One unit row for `SquadStats::aggregate`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SquadMember {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Health.
    pub health: f32,
    /// Estimated DPS.
    pub dps: f32,
    /// Can target air.
    pub target_air: bool,
    /// Can target ground.
    pub target_ground: bool,
    /// Water unit.
    pub naval: bool,
    /// Flying unit.
    pub flying: bool,
    /// Mobilization flag.
    pub flag: i32,
}

/// One RTS squad (`RtsAI.Squad`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Squad {
    /// Member units, in stable order.
    pub units: Vec<Entity>,
    /// Mobilization flag (`unit.flag`).
    pub flag: i32,
    /// Current target tile.
    pub target: Option<TilePos>,
}

/// `RtsAI` state for one team.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RtsAi {
    /// Assigned squads.
    pub squads: Vec<Squad>,
    /// Timer until the next squad re-assignment, in ticks (`timer`).
    pub timer: f32,
    /// Monotonic assignment counter (deterministic target shuffle seed).
    pub assignment_id: u64,
}

impl RtsAi {
    /// Creates an empty RTS AI.
    pub fn new() -> Self {
        Self::default()
    }

    /// `assignSquads`: groups idle units by `flag` (`(unit, flag)` pairs).
    ///
    /// Upstream BFS-clusters units within `squad_radius`; the port groups by
    /// flag in stable list order. Deterministic: input order is preserved and
    /// squad order is first-seen flag order.
    pub fn assign_squads(&mut self, units: &[(Entity, i32)], _squad_radius: f32) {
        self.squads.clear();
        for &(entity, flag) in units {
            if let Some(squad) = self.squads.iter_mut().find(|squad| squad.flag == flag) {
                squad.units.push(entity);
            } else {
                self.squads.push(Squad {
                    units: vec![entity],
                    flag,
                    target: None,
                });
            }
        }
        self.assignment_id = self.assignment_id.wrapping_add(1);
    }

    /// `findTarget`: nearest candidate tile not already assigned, given `(x, y)`.
    pub fn find_target(
        squads: &[Squad],
        candidates: &[TilePos],
        x: f32,
        y: f32,
    ) -> Option<TilePos> {
        let mut best: Option<(f32, TilePos)> = None;
        for &candidate in candidates {
            if squads.iter().any(|squad| squad.target == Some(candidate)) {
                continue;
            }
            let ts = crate::config::TILESIZE as f32;
            let cx = (candidate.x() as f32 + 0.5) * ts;
            let cy = (candidate.y() as f32 + 0.5) * ts;
            let dx = cx - x;
            let dy = cy - y;
            let dist2 = dx * dx + dy * dy;
            match best {
                Some((best_dist, best_tile))
                    if best_dist < dist2 || (best_dist == dist2 && best_tile <= candidate) => {}
                _ => best = Some((dist2, candidate)),
            }
        }
        best.map(|(_, tile)| tile)
    }

    /// Updates the squad re-assignment timer; returns `true` when it fires.
    pub fn update_timer(&mut self, interval: f32) -> bool {
        self.timer -= 1.0;
        if self.timer <= 0.0 {
            self.timer = interval;
            true
        } else {
            false
        }
    }

    /// `assignSquads`: BFS-clusters idle `CommandAI` units by square proximity.
    ///
    /// Upstream walks the team unit quadtree; this port consumes a stable
    /// pre-sorted [`RtsUnit`] slice so the output is deterministic (plan 11
    /// §2.4). `squad_radius = 60`.
    pub fn assign_squads_bfs(&mut self, units: &[RtsUnit], squad_radius: f32) {
        self.squads.clear();
        let mut used = vec![false; units.len()];
        for i in 0..units.len() {
            if !units[i].idle || used[i] {
                continue;
            }
            used[i] = true;
            let flag_zero = units[i].flag == 0;
            let rad = squad_radius + units[i].hit_size * 1.5;
            let mut squad = vec![units[i].entity];
            let mut stack = vec![units[i]];
            while let Some(next) = stack.pop() {
                let half = rad * 0.5;
                for (j, other) in units.iter().enumerate() {
                    if used[j] || !other.idle {
                        continue;
                    }
                    if (other.flag == 0) != flag_zero {
                        continue;
                    }
                    if (other.x - next.x).abs() <= half && (other.y - next.y).abs() <= half {
                        used[j] = true;
                        squad.push(other.entity);
                        stack.push(*other);
                    }
                }
            }
            self.squads.push(Squad {
                units: squad,
                flag: i32::from(!flag_zero),
                target: None,
            });
        }
        self.assignment_id = self.assignment_id.wrapping_add(1);
    }

    /// `RtsAI.handleSquad` defense selection.
    ///
    /// Returns `(defend_building, defending_core)` when the squad should
    /// respond to damage, exactly matching the upstream condition chain.
    pub fn plan_defend(
        stats: &SquadStats,
        size: usize,
        damaged: &[RtsBuilding],
        min_squad: i32,
    ) -> Option<(RtsBuilding, bool)> {
        if stats.naval || damaged.is_empty() {
            return None;
        }
        let mut best: Option<&RtsBuilding> = None;
        let mut best_key = i64::MAX;
        for build in damaged {
            // Cores rush immediately (`return -999999f`).
            let key = if build.in_core {
                i64::MIN
            } else {
                dist2(build.x, build.y, stats.x, stats.y) as i64
            };
            if key < best_key {
                best_key = key;
                best = Some(build);
            }
        }
        let build = best?;
        let respond = build.in_core
            || size >= min_squad as usize
            || stats.numbered
            || dist2(build.x, build.y, stats.x, stats.y) <= 1000.0 * 1000.0;
        respond.then_some((*build, build.in_core))
    }

    /// `findTarget`: pick the best weighted unassigned enemy building.
    ///
    /// The upstream shuffle of candidates is replaced by stable input order so
    /// the choice is deterministic; the comparator chain
    /// `((1 - weight) + dst/10000, dst2)` is ported exactly.
    #[allow(clippy::too_many_arguments)]
    pub fn find_target_weighted(
        candidates: &[RtsBuilding],
        x: f32,
        y: f32,
        total: i32,
        self_dps: f32,
        self_health: f32,
        check_weight: bool,
        air: bool,
        min_squad: i32,
        min_weight: f32,
        max_squad: i32,
        unit_cap: i32,
        assigned: &mut std::collections::BTreeSet<Entity>,
        invalid: &std::collections::BTreeSet<TilePos>,
        enemy_stats: &[EnemyStat],
    ) -> Option<RtsBuilding> {
        if total < min_squad {
            return None;
        }
        let available: Vec<&RtsBuilding> = candidates
            .iter()
            .filter(|c| !assigned.contains(&c.entity) && !invalid.contains(&c.pos))
            .collect();
        if available.is_empty() {
            return None;
        }

        let mut best: Option<(&RtsBuilding, f32, f32, f32)> = None;
        for candidate in &available {
            let weight = estimate_stats(
                x,
                y,
                candidate.x,
                candidate.y,
                self_dps,
                self_health,
                air,
                enemy_stats,
            );
            let dst = dist2(candidate.x, candidate.y, x, y);
            let primary = (1.0 - weight) + dst / 10_000.0;
            match best {
                Some((_, best_primary, best_dst, _)) => {
                    if primary < best_primary || (primary == best_primary && dst < best_dst) {
                        best = Some((candidate, primary, dst, weight));
                    }
                }
                None => best = Some((candidate, primary, dst, weight)),
            }
        }

        let (result, _, _, weight) = best?;
        if check_weight && weight < min_weight && total < max_squad && total < unit_cap {
            return None;
        }
        assigned.insert(result.entity);
        Some(*result)
    }

    /// `checkBuilding`: core-unit cap test (`coreUnits < cores.size`).
    ///
    /// The actual `block.unitType.create(team)` spawn is host work; this returns
    /// whether it should happen.
    pub fn check_building(
        ai_core_spawn: bool,
        timer_fired: bool,
        core_units: i32,
        cores: i32,
    ) -> bool {
        ai_core_spawn && timer_fired && cores > 0 && core_units < cores
    }
}

/// `RtsAI.estimateStats`: combat-worth score in `[0, 100000]`.
///
/// `100000` means the squad can destroy the target with no losses; `0` means it
/// cannot win. Matches the upstream infinite/zero guard order exactly.
#[allow(clippy::too_many_arguments)]
pub fn estimate_stats(
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    self_dps: f32,
    self_health: f32,
    air: bool,
    enemy_stats: &[EnemyStat],
) -> f32 {
    let mut health = 0.0f32;
    let mut dps = 0.0f32;
    let extra_radius = 50.0f32;
    for stat in enemy_stats {
        let targets = (stat.target_air && air) || (stat.target_ground && !air);
        if targets
            && distance_segment_point(from_x, from_y, to_x, to_y, stat.x, stat.y)
                <= stat.range + extra_radius
        {
            health += stat.health;
            dps += stat.dps;
        }
    }

    let time_destroy_other = if self_dps.abs() <= 0.001 {
        f32::INFINITY
    } else {
        health / self_dps
    };
    let time_destroy_self = if dps.abs() <= f32::EPSILON {
        f32::INFINITY
    } else {
        self_health / dps
    };

    if time_destroy_other.is_infinite() || time_destroy_self == 0.0 {
        return 0.0;
    }
    if time_destroy_self.is_infinite() || time_destroy_other == 0.0 {
        return 100_000.0;
    }
    time_destroy_self / time_destroy_other
}

/// Squared distance.
fn dist2(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    (ax - bx).powi(2) + (ay - by).powi(2)
}

/// `Intersector.distanceSegmentPoint` (segment `a`-`b`, point `p`).
fn distance_segment_point(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    let abx = bx - ax;
    let aby = by - ay;
    let apx = px - ax;
    let apy = py - ay;
    let ab_len2 = abx * abx + aby * aby;
    let t = if ab_len2 <= f32::EPSILON {
        0.0
    } else {
        ((apx * abx + apy * aby) / ab_len2).clamp(0.0, 1.0)
    };
    let cx = ax + abx * t;
    let cy = ay + aby * t;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

/// Creates the per-`TeamData` RTS AI lazily (`TeamRule.rtsAi`).
pub fn ensure_rts_ai(data: &mut crate::game::teams::TeamData) -> bool {
    if data.rts_ai.is_none() {
        data.rts_ai = Some(RtsAi::new());
        true
    } else {
        false
    }
}

/// `RtsAI.update`: fires the 2 s (120-tick) squad re-assignment timer.
pub fn update(rts: &mut RtsAi) -> bool {
    rts.update_timer(120.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_stats_rewards_strength() {
        let enemy = [EnemyStat {
            x: 50.0,
            y: 0.0,
            health: 100.0,
            dps: 10.0,
            range: 40.0,
            target_air: false,
            target_ground: true,
            air: false,
        }];
        // self can kill the target fast, takes little damage -> high score.
        let strong = estimate_stats(0.0, 0.0, 100.0, 0.0, 100.0, 1000.0, false, &enemy);
        let weak = estimate_stats(0.0, 0.0, 100.0, 0.0, 1.0, 100.0, false, &enemy);
        assert!(strong > weak, "stronger squad scores higher");
        // No enemy in range -> self is invincible -> max score.
        let none = estimate_stats(0.0, 0.0, 100.0, 0.0, 10.0, 10.0, false, &[]);
        assert_eq!(none, 100_000.0);
    }

    #[test]
    fn estimate_stats_zero_self_dps_is_unwinnable() {
        let enemy = [EnemyStat {
            x: 0.0,
            y: 0.0,
            health: 100.0,
            dps: 10.0,
            range: 1000.0,
            target_air: false,
            target_ground: true,
            air: false,
        }];
        assert_eq!(
            estimate_stats(0.0, 0.0, 10.0, 0.0, 0.0, 100.0, false, &enemy),
            0.0
        );
    }

    #[test]
    fn find_target_respects_min_squad_and_assignment() {
        let a = RtsBuilding {
            entity: Entity::PLACEHOLDER,
            pos: TilePos::new(5, 5),
            x: 5.0,
            y: 5.0,
            in_core: false,
        };
        let mut assigned = std::collections::BTreeSet::new();
        let invalid = std::collections::BTreeSet::new();
        // Below min squad -> no target.
        assert!(
            RtsAi::find_target_weighted(
                &[a],
                0.0,
                0.0,
                1,
                10.0,
                100.0,
                false,
                false,
                5,
                0.0,
                15,
                100,
                &mut assigned,
                &invalid,
                &[]
            )
            .is_none()
        );
        let picked = RtsAi::find_target_weighted(
            &[a],
            0.0,
            0.0,
            10,
            10.0,
            100.0,
            false,
            false,
            5,
            0.0,
            15,
            100,
            &mut assigned,
            &invalid,
            &[],
        );
        assert_eq!(picked, Some(a));
        // Now assigned -> excluded.
        assert!(
            RtsAi::find_target_weighted(
                &[a],
                0.0,
                0.0,
                10,
                10.0,
                100.0,
                false,
                false,
                5,
                0.0,
                15,
                100,
                &mut assigned,
                &invalid,
                &[]
            )
            .is_none()
        );
    }

    #[test]
    fn plan_defend_rushes_cores() {
        let stats = SquadStats {
            x: 0.0,
            y: 0.0,
            health: 100.0,
            dps: 10.0,
            target_air: true,
            target_ground: true,
            naval: false,
            flying: false,
            numbered: false,
        };
        let core = RtsBuilding {
            entity: Entity::PLACEHOLDER,
            pos: TilePos::new(1, 1),
            x: 10000.0,
            y: 10000.0,
            in_core: true,
        };
        let far = RtsBuilding {
            pos: TilePos::new(2, 2),
            x: 5000.0,
            y: 5000.0,
            ..core
        };
        let order = RtsAi::plan_defend(&stats, 1, &[far, core], 15).expect("defend");
        assert!(order.1, "core defend always fires");
        assert!(order.0.in_core);
    }

    #[test]
    fn bfs_clusters_nearby_same_flag_units() {
        let units = [
            RtsUnit {
                entity: Entity::PLACEHOLDER,
                x: 0.0,
                y: 0.0,
                flag: 0,
                idle: true,
                hit_size: 8.0,
            },
            RtsUnit {
                entity: Entity::PLACEHOLDER,
                x: 5.0,
                y: 0.0,
                flag: 0,
                idle: true,
                hit_size: 8.0,
            },
            RtsUnit {
                entity: Entity::PLACEHOLDER,
                x: 1000.0,
                y: 0.0,
                flag: 0,
                idle: true,
                hit_size: 8.0,
            },
        ];
        let mut rts = RtsAi::new();
        rts.assign_squads_bfs(&units, 60.0);
        assert_eq!(rts.squads.len(), 2, "two distant clusters");
        assert_eq!(rts.squads[0].units.len(), 2);
        assert_eq!(rts.squads[1].units.len(), 1);
    }

    #[test]
    fn check_building_core_unit_cap() {
        assert!(RtsAi::check_building(true, true, 0, 2));
        assert!(!RtsAi::check_building(true, true, 2, 2));
        assert!(!RtsAi::check_building(false, true, 0, 2));
    }

    #[test]
    fn rts_assigns_squads_by_flag() {
        let mut rts = RtsAi::new();
        let a = Entity::PLACEHOLDER;
        let units = [(a, 0), (a, 1), (a, 0), (a, 1)];
        rts.assign_squads(&units, 60.0);
        assert_eq!(rts.squads.len(), 2, "one squad per distinct flag");
        assert_eq!(rts.squads[0].units.len(), 2);
        assert_eq!(rts.squads[1].units.len(), 2);
        assert_eq!(rts.squads[0].flag, 0);
        // Deterministic across runs.
        let mut again = RtsAi::new();
        again.assign_squads(&units, 60.0);
        assert_eq!(rts.squads, again.squads);
    }

    #[test]
    fn target_skips_assigned() {
        let mut rts = RtsAi::new();
        let a = Entity::PLACEHOLDER;
        rts.assign_squads(&[(a, 0), (a, 1)], 60.0);
        rts.squads[0].target = Some(TilePos::new(5, 5));
        let candidates = [TilePos::new(5, 5), TilePos::new(9, 9)];
        let target = RtsAi::find_target(&rts.squads, &candidates, 0.0, 0.0);
        assert_eq!(target, Some(TilePos::new(9, 9)));
    }
}

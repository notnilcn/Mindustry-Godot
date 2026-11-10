// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RtsAI.estimateStats` rows: enemy unit rows plus the ammo-based turret DPS.
//! Upstream: core/src/mindustry/ai/RtsAI.java (estimateStats),
//! core/src/mindustry/world/blocks/defense/turrets/Turret.java

use super::super::*;
use super::{TeamAiBuildingRow, TeamAiUnitRow, team_ai_enemy_stats};
use crate::ai::rts_ai::EnemyStat;
use crate::content::ContentRegistry;
use crate::entities::comp::Health;

/// `estimateStats` enemy rows for one team: unit rows plus turret rows.
pub(super) fn enemy_stats_for(
    world: &World,
    units: &[TeamAiUnitRow],
    buildings: &[TeamAiBuildingRow],
    team: u8,
) -> Vec<EnemyStat> {
    let mut stats = team_ai_enemy_stats(units, team);
    if let Some(unit_runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>() {
        stats.extend(turret_enemy_stats(
            world,
            &unit_runtime.content,
            buildings,
            team,
        ));
    }
    stats
}

/// `RtsAI.estimateStats` turret rows: every live enemy turret of an active
/// non-derelict team (`indexer.getEnemy(team, BlockFlag.turret)`,
/// `RtsAI.java:329-335`), with the per-kind `estimateDps` of
/// `Turret.java:335-338`, `ContinuousTurret.java:43-46` and
/// `TractorBeamTurret.java:150-153`.
pub(in crate::game::runtime) fn turret_enemy_stats(
    world: &World,
    content: &ContentRegistry,
    buildings: &[TeamAiBuildingRow],
    team: u8,
) -> Vec<EnemyStat> {
    use crate::world::blocks::defense::turrets::{TurretKind, TurretState, has_ammo, peek_ammo};
    let mut rows: Vec<(u64, EnemyStat)> = buildings
        .iter()
        .filter(|row| row.team != team && row.team != 0)
        .filter_map(|row| {
            let state = world.get::<TurretState>(row.entity)?;
            let config = &state.config;
            let (potential_efficiency, time_scale, efficiency) = world
                .get::<crate::entities::comp::Building>(row.entity)
                .map_or((1.0, 1.0, 1.0), |building| {
                    (
                        building.potential_efficiency,
                        building.time_scale,
                        building.efficiency,
                    )
                });
            let dps = match config.kind {
                // `ContinuousTurret.estimateDps` (`ContinuousTurret.java:43`).
                TurretKind::Continuous => {
                    if has_ammo(world, row.entity) {
                        let bullet = peek_ammo(world, row.entity).and_then(|id| content.bullet(id));
                        let damage = bullet.map_or(0.0, |bullet| bullet.damage);
                        let interval = bullet.map_or(5.0, |bullet| bullet.damage_interval);
                        damage * 60.0 / if interval > 0.0 { interval } else { 5.0 }
                    } else {
                        0.0
                    }
                }
                // `TractorBeamTurret.estimateDps` (`TractorBeamTurret.java:150`).
                TurretKind::TractorBeam => {
                    if !state.any || config.beam_damage <= 0.0 {
                        0.0
                    } else {
                        config.beam_damage * 60.0 * efficiency * config.coolant_multiplier
                    }
                }
                // `Turret.estimateDps` (`Turret.java:335-338`) for the item /
                // liquid / power / laser / payload-ammo families.
                TurretKind::Item
                | TurretKind::Liquid
                | TurretKind::Power
                | TurretKind::Laser
                | TurretKind::PayloadAmmo => {
                    if has_ammo(world, row.entity) {
                        let ammo_dps = peek_ammo(world, row.entity)
                            .and_then(|id| content.bullet(id))
                            .map_or(0.0, |bullet| bullet.estimate_dps);
                        config.shoot.shots as f32 / config.reload
                            * 60.0
                            * ammo_dps
                            * potential_efficiency
                            * time_scale
                    } else {
                        0.0
                    }
                }
                // `BaseTurretBuild.estimateDps()` = 0 (`BaseTurret.java:138-140`).
                TurretKind::PointDefense | TurretKind::Build => 0.0,
            };
            let health = world
                .get::<Health>(row.entity)
                .map_or(0.0, |health| health.health);
            Some((
                row.seq,
                EnemyStat {
                    x: row.x,
                    y: row.y,
                    health,
                    dps,
                    // Java `t.range()` adds the loaded ammo's `rangeChange`
                    // (`Turret.java:348-353`).
                    range: crate::world::blocks::defense::turrets::turret_range(world, row.entity),
                    target_air: config.target_air,
                    target_ground: config.target_ground,
                    air: false,
                    is_turret: true,
                },
            ))
        })
        .collect();
    // Stable `EntitySeq` order so the summed health/DPS is deterministic.
    rows.sort_by_key(|(seq, _)| *seq);
    rows.into_iter().map(|(_, stat)| stat).collect()
}

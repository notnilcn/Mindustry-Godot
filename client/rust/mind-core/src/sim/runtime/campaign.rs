// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign world-request drain, dirty-rule application, map materialization and
//! lost-sector capture/replay payload.
//!
//! Upstream: none (port-only campaign glue).

use super::*;

impl Sim {
    /// Drains `CampaignRuntime` world-request queues that cannot mutate the
    /// world from inside a system: the capture → derelict conversion (T7) and
    /// `coreDestroyClear` `timeDestroy` posts.
    pub(crate) fn apply_campaign_world_requests(&mut self) {
        let (derelict_team, time_destroys, abandon_kill) = {
            let Some(mut runtime) = self.ecs.0.get_resource_mut::<CampaignRuntime>() else {
                return;
            };
            // `abandonSectorConfirm`'s `Time.runTask(7f)` (PlanetDialog.java:
            // 1156-1161): count down, then kill `player.team().cores()`.
            let abandon_kill = match runtime.pending_abandon_kill {
                Some(ticks) if ticks <= 1 => {
                    runtime.pending_abandon_kill = None;
                    let rules = runtime.session.rules.clone();
                    runtime.session.teams.player_cores(&rules).to_vec()
                }
                Some(ticks) => {
                    runtime.pending_abandon_kill = Some(ticks - 1);
                    Vec::new()
                }
                None => Vec::new(),
            };
            (
                runtime.pending_derelict_team.take(),
                std::mem::take(&mut runtime.pending_time_destroy),
                abandon_kill,
            )
        };
        // `core.kill()`: the next death pass destroys them and the campaign
        // `checkGameState` sees no player core.
        for core in abandon_kill {
            self.kill_building(core);
        }
        if derelict_team.is_none() && time_destroys.is_empty() {
            return;
        }
        // Pass the sim's stream through a swap: the methods borrow `self`.
        let mut rng = std::mem::replace(&mut self.rng_streams, crate::determinism::SimRng::new(0));
        if let Some(team) = derelict_team {
            self.destroy_team_to_derelict(team, &mut rng);
        }
        for (team, x, y, range) in time_destroys {
            self.time_destroy(team, x, y, range, &mut rng);
        }
        self.rng_streams = rng;
    }

    /// Drains `CampaignRuntime.rules_world_dirty` into
    /// [`Self::apply_rules_to_world`] (T5: G06 `setrule`, relay `setRules`,
    /// universe invasion of the played sector).
    pub(crate) fn apply_dirty_rules_to_world(&mut self) {
        let rules = {
            let Some(mut runtime) = self.ecs.0.get_resource_mut::<CampaignRuntime>() else {
                return;
            };
            if !runtime.rules_world_dirty {
                return;
            }
            runtime.rules_world_dirty = false;
            runtime.session.rules.clone()
        };
        self.apply_rules_to_world(&rules);
    }

    /// Materializes the ECS buildings for a loaded grid (map/save reader's
    /// decoded payload queue). No-op without the live block runtime; returns the
    /// number of entities spawned.
    pub fn materialize_map_buildings(&mut self, pending: &[(usize, DecodedBase)]) -> usize {
        let Sim {
            grid,
            ecs,
            block_runtime,
            ..
        } = self;
        let Some(runtime) = block_runtime.as_mut() else {
            return 0;
        };
        let spawned = runtime.materialize_buildings(&mut ecs.0, grid, pending);
        // Keep the unit runtime's terrain/path snapshot in sync with the newly
        // linked map buildings.
        self.refresh_unit_runtime();
        spawned
    }

    /// `Control.playSector`'s lost-sector half: `SectorDamage.apply(1f)` over the
    /// loaded (lost) world destroying the marked buildings, then the surviving
    /// default-team buildings and derelicts as the replay payload
    /// (`Control.java:482-537`).
    pub fn capture_lost_sector(
        &mut self,
        default_team: u8,
        wave_team: u8,
    ) -> (Vec<ReplayBuilding>, Vec<ReplayBuilding>) {
        let Sim {
            grid,
            ecs,
            block_runtime,
            ..
        } = self;
        let Some(runtime) = block_runtime.as_mut() else {
            return (Vec::new(), Vec::new());
        };

        // `SectorDamage.apply` state: spawn overlays + every live building.
        let mut spawns: Vec<(i32, i32)> = Vec::new();
        for tile in grid.tiles.array() {
            if runtime
                .content
                .block(tile.overlay)
                .is_some_and(|def| def.kind == crate::content::BlockKind::SpawnBlock)
            {
                spawns.push((i32::from(tile.x), i32::from(tile.y)));
            }
        }
        let mut buildings: Vec<crate::maps::sector_damage::DamageBuilding> = Vec::new();
        let mut owners: Vec<Entity> = Vec::new();
        for (_seq, entity, comp) in ecs.entities_by_seq() {
            let Some(def) = runtime.content.block(comp.block) else {
                continue;
            };
            let health = ecs
                .0
                .get::<crate::entities::comp::health::Health>(entity)
                .map(|entry| entry.health)
                .unwrap_or(def.health as f32);
            buildings.push(crate::maps::sector_damage::DamageBuilding::new(
                i32::from(comp.pos.x()),
                i32::from(comp.pos.y()),
                comp.team.0,
                health,
                def.size,
                def.kind == crate::content::BlockKind::CoreBlock,
            ));
            owners.push(entity);
        }
        let mut state = crate::maps::sector_damage::SectorDamageState {
            default_team,
            wave_team,
            spawns,
            buildings,
        };
        let mut rng = crate::determinism::SimRng::new(u64::from(default_team).wrapping_add(1));
        crate::maps::sector_damage::apply(
            &grid.tiles,
            &runtime.content,
            &mut state,
            1.0,
            &mut rng,
            &crate::maps::sector_damage::NoopDamageFx,
        );
        // Remove the buildings the damage marked (`Control.java:482`'s pass).
        let mut log = crate::world::ops::WorldEventLog::default();
        for (index, building) in state.buildings.iter().enumerate() {
            if !building.removed {
                continue;
            }
            let Some(entity) = owners.get(index).copied() else {
                continue;
            };
            if ecs.0.get_entity(entity).is_err() {
                continue;
            }
            let mut ctx = crate::world::ops::WorldCtx {
                grid: &mut *grid,
                content: &runtime.content,
                ecs: &mut ecs.0,
                hooks: &runtime.hooks,
                render: &runtime.render,
                log: &mut log,
            };
            ctx.clear_building(entity);
        }

        // Survivor payload: default-team buildings (cores excluded) + derelicts.
        let mut survivors = Vec::new();
        let mut derelicts = Vec::new();
        for (_seq, entity, comp) in ecs.entities_by_seq() {
            let Some(def) = runtime.content.block(comp.block) else {
                continue;
            };
            if def.kind == crate::content::BlockKind::CoreBlock {
                continue;
            }
            let health = ecs
                .0
                .get::<crate::entities::comp::health::Health>(entity)
                .map(|entry| entry.health)
                .unwrap_or(def.health as f32);
            let entry = ReplayBuilding {
                x: comp.pos.x(),
                y: comp.pos.y(),
                block: comp.block,
                rot: comp.rot,
                health,
            };
            if comp.team.0 == default_team {
                survivors.push(entry);
            } else if comp.team.0 == crate::game::team::DERELICT.0 {
                derelicts.push(entry);
            }
        }
        (survivors, derelicts)
    }
}

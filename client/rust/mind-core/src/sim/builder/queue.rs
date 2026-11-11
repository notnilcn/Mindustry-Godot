// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The `impl Sim` player build-queue API: queue place/break, repair plans, plan
//! queries and the team ghost-plan list.
//!
//! Upstream: core/src/mindustry/input/InputHandler.java (flushPlans, update).

use super::checks::team_core;
use super::*;

use crate::sim::query::unit_def;

impl Sim {
    /// Whether `SimCommand::Place`/`Break` from player input should queue on
    /// the player unit (`InputHandler.flushPlans`) rather than apply
    /// instantly: a live block runtime and a player unit that can build.
    pub fn player_can_build(&self) -> bool {
        let (Some(unit), Some(runtime)) = (self.player_unit, self.block_runtime.as_ref()) else {
            return false;
        };
        unit_def(&self.ecs.0, runtime.content(), unit).is_some_and(|def| def.build_speed > 0.0)
    }

    /// Whether `team` owns a core. Player plans with no build-capable unit
    /// are dropped in any world where the player's team has a core (Java:
    /// `Nulls.unit.addBuild` is a no-op, `BuilderComp.java` `addBuild`
    /// `if(!canBuild()) return;`). Core-less dev worlds keep the port's
    /// instant placement (no Java counterpart; spine/determinism scenarios).
    pub(crate) fn team_has_core(&self, team: u8) -> bool {
        team_core(&self.ecs.0, team).is_some()
    }

    /// Team of player-issued commands that carry no team (`Break`): the
    /// session `rules.defaultTeam` (`Vars.player.team()` in campaign).
    pub(crate) fn player_command_team(&self) -> u8 {
        crate::game::runtime::rules_of(&self.ecs.0).map_or(1, |rules| rules.default_team)
    }

    /// Queues a placement on the player unit (`InputHandler.flushPlans` ->
    /// `player.unit().addBuild(plan)`); invalid placements are dropped.
    pub fn queue_player_place(&mut self, x: i16, y: i16, block: BlockId, rotation: u8) -> bool {
        self.queue_player_place_config(
            x,
            y,
            block,
            rotation,
            crate::world::config::ConfigValue::None,
        )
    }

    /// Queues a placement carrying a config (`InputHandler.updateLine` plan
    /// configs, schematic copies, rebuild ghosts; the value is applied when the
    /// construct completes).
    pub fn queue_player_place_config(
        &mut self,
        x: i16,
        y: i16,
        block: BlockId,
        rotation: u8,
        config: crate::world::config::ConfigValue,
    ) -> bool {
        let Some(unit) = self.player_unit else {
            return false;
        };
        let team = self.live_default_team();
        let Some(runtime) = self.block_runtime.as_ref() else {
            return false;
        };
        if !runtime.can_place(
            &self.ecs.0,
            &self.grid,
            x,
            y,
            block,
            rotation,
            team,
            crate::world::build::PlaceFlags::PLAYER,
        ) {
            return false;
        }
        let mut plan = UnitBuildPlan::place(x, y, rotation, block);
        plan.config = config;
        // `addBuild`: carry the progress of an in-flight construct block.
        if let Some(entity) = self.grid.entity_at(TilePos::new(x, y))
            && let Some(state) = self.ecs.0.get::<ConstructState>(entity)
        {
            plan.progress = state.progress;
        }
        self.with_player_plans(unit, |plans| plans.add_build(plan, true));
        true
    }

    /// `InputHandler.rebuildArea` (`InputHandler.java:2318-2330`): re-queues
    /// every live team ghost plan whose block bounds overlap the selection
    /// rect. Returns the queued count. (`tryRepairDerelict` over the rect,
    /// `InputHandler.java:2331-2341`, is deferred: it needs the G02 T8
    /// derelict-repair branches in `Build.validPlace`.)
    pub fn queue_team_rebuild(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> usize {
        let team = self.live_default_team();
        let r = crate::input::placement::normalize_area(x1, y1, x2, y2, 0, false, 999_999_999);
        let rect = (
            (r.x * 8) as f32,
            (r.y * 8) as f32,
            ((r.x2 - r.x) * 8) as f32,
            ((r.y2 - r.y) * 8) as f32,
        );
        let content = self.block_runtime.as_ref().map(|runtime| runtime.content());
        // `for(BlockPlan plan : player.team().data().plans)` (front to back).
        let ghosts: Vec<crate::game::teams::BlockPlan> = self
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .and_then(|campaign| campaign.session.teams.get_or_null(crate::ecs::TeamId(team)))
            .map(|data| {
                data.plans
                    .iter()
                    .filter(|plan| {
                        rects_overlap(
                            block_bounds(content, plan.x as i32, plan.y as i32, plan.block),
                            rect,
                        )
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut queued = 0usize;
        for plan in ghosts {
            let rotation = plan.rotation.rem_euclid(4) as u8;
            if self.queue_player_place_config(
                plan.x,
                plan.y,
                plan.block,
                rotation,
                plan.config.clone().unwrap_or_default(),
            ) {
                queued += 1;
            }
        }
        // `for(x..x2) for(y..y2) world.tileBuilding(x, y)` -> `tryRepairDerelict`
        // deduped by building origin (`intSet.add(tile.pos())`,
        // InputHandler.java:2331-2341).
        let mut seen: std::collections::HashSet<(i16, i16)> = std::collections::HashSet::new();
        for x in r.x..=r.x2 {
            for y in r.y..=r.y2 {
                let Some(entity) = self.grid.tiles.getn(x, y).and_then(|tile| tile.build) else {
                    continue;
                };
                let origin = self
                    .ecs
                    .0
                    .get::<crate::ecs::BuildingComp>(entity)
                    .map(|building| (building.pos.x(), building.pos.y()))
                    .unwrap_or((x as i16, y as i16));
                if !seen.insert(origin) {
                    continue;
                }
                if self.try_repair_derelict(x, y) {
                    queued += 1;
                }
            }
        }
        queued
    }

    /// `InputHandler.canRepairDerelict` (`InputHandler.java:2076-2081`): the
    /// derelict (team-0) building at `(x, y)` can be rebuilt by the player
    /// team when `Build.validPlace` accepts the same block/rotation there.
    ///
    /// `block.unlockedNowHost()` is a no-op here: the live content registry
    /// carries the full vanilla set (the campaign unlock gate runs in the
    /// research UI, not on the repair plan).
    pub fn derelict_repair_plan(
        &self,
        x: i32,
        y: i32,
    ) -> Option<(i16, i16, BlockId, u8, crate::world::config::ConfigValue)> {
        if self.player_unit.is_none() || self.live_default_team() == 0 {
            return None;
        }
        let entity = self.grid.tiles.getn(x, y).and_then(|tile| tile.build)?;
        let building = self.ecs.0.get::<crate::ecs::BuildingComp>(entity)?;
        // `tile.build.team == Team.derelict`.
        if building.team.0 != 0 {
            return None;
        }
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let block = building.block;
        let rotation = building.rot;
        if !self.valid_place_player(tx, ty, block, rotation) {
            return None;
        }
        // `build.config()` (`BuildPlan(..., selected.block(), build.config())`).
        let config = crate::world::config::read_config(&self.ecs.0, entity);
        Some((tx, ty, block, rotation, config))
    }

    /// `InputHandler.tryRepairDerelict`: queue the derelict building's repair
    /// into the player build queue.
    pub fn try_repair_derelict(&mut self, x: i32, y: i32) -> bool {
        let Some((tx, ty, block, rotation, config)) = self.derelict_repair_plan(x, y) else {
            return false;
        };
        self.queue_player_place_config(tx, ty, block, rotation, config)
    }

    /// `InputHandler.removeSelection`'s plan half (`InputHandler.java:1905-1935`):
    /// remove non-breaking player plans whose bounds overlap the clamped rect,
    /// and drop team ghost plans overlapping it (`plan.removed = true`).
    /// Runs even when no building sits in the rect.
    pub fn remove_plans_in(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, max_length: i32) {
        let r = crate::input::placement::normalize_area(x1, y1, x2, y2, 0, false, max_length);
        let rect = (
            (r.x * 8) as f32,
            (r.y * 8) as f32,
            ((r.x2 - r.x) * 8) as f32,
            ((r.y2 - r.y) * 8) as f32,
        );
        // Collect first (content borrows), then mutate the queue.
        let plan_positions: Vec<(i16, i16)> = if let Some(unit) = self.player_unit {
            let content = self.block_runtime.as_ref().map(|runtime| runtime.content());
            self.ecs
                .0
                .get::<BuilderPlans>(unit)
                .map(|plans| {
                    plans
                        .plans
                        .iter()
                        .filter(|plan| !plan.breaking)
                        .filter(|plan| {
                            rects_overlap(
                                block_bounds(content, plan.x as i32, plan.y as i32, plan.block),
                                rect,
                            )
                        })
                        .map(|plan| (plan.x, plan.y))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(unit) = self.player_unit {
            self.with_player_plans(unit, |plans| {
                plans
                    .plans
                    .retain(|plan| !plan_positions.contains(&(plan.x, plan.y)));
            });
        }
        if let Some(mut campaign) = self
            .ecs
            .0
            .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
        {
            let content = self.block_runtime.as_ref().map(|runtime| runtime.content());
            let team = campaign.session.rules.default_team;
            let data = campaign.session.teams.get(crate::ecs::TeamId(team));
            for plan in data.plans.iter_mut() {
                if rects_overlap(
                    block_bounds(content, plan.x as i32, plan.y as i32, plan.block),
                    rect,
                ) {
                    plan.removed = true;
                }
            }
            data.plans.retain(|plan| !plan.removed);
        }
    }

    /// `BuilderComp.activelyBuilding` for the live player unit
    /// (`BuilderComp.java:301-309`): the `canShoot` gate for left presses.
    pub fn player_actively_building(&self) -> bool {
        let Some(unit) = self.player_unit else {
            return false;
        };
        let Some(pos) = self.ecs.0.get::<Pos>(unit).map(|pos| (pos.x, pos.y)) else {
            return false;
        };
        let Some(runtime) = self.unit_runtime() else {
            return false;
        };
        crate::ai::types::player::player_actively_building(&self.ecs.0, &runtime.content, unit, pos)
            .is_some()
    }

    /// `InputHandler.validPlace(x, y, block, rot)` for the local team: the
    /// placement-preview validity check, no side effects and nothing queued
    /// (`InputHandler.java:2029-2032` calls `Build.validPlace`).
    pub fn preview_can_place(&self, x: i16, y: i16, block: BlockId, rotation: u8) -> bool {
        self.valid_place_player(x, y, block, rotation)
    }

    /// Queues a deconstruction on the player unit
    /// (`InputHandler.removeSelection`); a queued placement covering the tile
    /// is removed instead.
    pub fn queue_player_break(&mut self, x: i16, y: i16) -> bool {
        let Some(unit) = self.player_unit else {
            return false;
        };
        let content_sizes: Option<Vec<(i16, i16, i32)>> = {
            let runtime = self.block_runtime.as_ref();
            self.ecs.0.get::<BuilderPlans>(unit).map(|plans| {
                plans
                    .plans
                    .iter()
                    .filter(|plan| !plan.breaking)
                    .map(|plan| {
                        let size = runtime
                            .and_then(|runtime| runtime.content().block(plan.block))
                            .map(|def| def.size)
                            .unwrap_or(1);
                        (plan.x, plan.y, size)
                    })
                    .collect()
            })
        };
        if let Some(sizes) = content_sizes {
            let hit = sizes.into_iter().find(|(px, py, size)| {
                let lo = -((size - 1) / 2);
                let hi = size / 2;
                let (dx, dy) = (i32::from(x - px), i32::from(y - py));
                dx >= lo && dx <= hi && dy >= lo && dy <= hi
            });
            if let Some((px, py, _)) = hit {
                self.with_player_plans(unit, |plans| {
                    plans
                        .plans
                        .retain(|plan| plan.breaking || plan.x != px || plan.y != py);
                });
                return true;
            }
        }
        let pos = TilePos::new(x, y);
        if self.grid.index(pos).is_err() {
            return false;
        }
        let Some(entity) = self.grid.entity_at(pos) else {
            return false;
        };
        let anchor = self
            .ecs
            .0
            .get::<Building>(entity)
            .map(|b| b.tile)
            .unwrap_or(pos);
        let team = self.live_default_team();
        let rules = self
            .ecs
            .0
            .get_resource::<crate::world::limits::BuildRules>()
            .cloned()
            .unwrap_or_default();
        let Some(runtime) = self.block_runtime.as_ref() else {
            return false;
        };
        if !crate::world::build::valid_break(
            runtime.content(),
            &self.grid,
            Some(&self.ecs.0),
            &rules,
            team,
            i32::from(anchor.x()),
            i32::from(anchor.y()),
        ) {
            return false;
        }
        self.with_player_plans(unit, |plans| {
            plans.add_build(UnitBuildPlan::break_plan(anchor.x(), anchor.y()), true);
        });
        true
    }

    pub(super) fn with_player_plans(&mut self, unit: Entity, f: impl FnOnce(&mut BuilderPlans)) {
        let Ok(mut entity) = self.ecs.0.get_entity_mut(unit) else {
            return;
        };
        match entity.get_mut::<BuilderPlans>() {
            Some(mut plans) => f(&mut plans),
            None => {
                let mut plans = BuilderPlans::default();
                f(&mut plans);
                entity.insert(plans);
            }
        }
    }

    /// The player unit's queued plans in queue order (head = active).
    pub fn player_build_plans(&self) -> Vec<UnitBuildPlan> {
        self.player_unit
            .and_then(|unit| self.ecs.0.get::<BuilderPlans>(unit))
            .map(|plans| plans.plans.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// `player.unit().plans.size` (`BuilderComp.plans`; the `buildautopause`
    /// auto-pause and deselect-mining gates, `DesktopInput.java:593/609`)
    /// without cloning the queue.
    pub fn player_plan_count(&self) -> usize {
        self.player_unit
            .and_then(|unit| self.ecs.0.get::<BuilderPlans>(unit))
            .map(|plans| plans.plans.len())
            .unwrap_or(0)
    }

    /// `build.isCommandable()` own buildings (`DesktopInput.java:346-357`):
    /// cores, unit factories, reconstructors, assemblers and payload sources,
    /// in `EntitySeq` order (`(x, y)` anchor tiles).
    pub fn commandable_buildings(&self, team: u8) -> Vec<(i16, i16)> {
        use crate::content::BlockKind;
        let Some(runtime) = self.block_runtime.as_ref() else {
            return Vec::new();
        };
        let content = runtime.content();
        self.ecs
            .entities_by_seq()
            .into_iter()
            .filter(|(_, _, comp)| {
                comp.team.0 == team
                    && content.block(comp.block).is_some_and(|def| {
                        matches!(
                            def.kind,
                            BlockKind::CoreBlock
                                | BlockKind::UnitFactory
                                | BlockKind::Reconstructor
                                | BlockKind::UnitAssembler
                                | BlockKind::PayloadSource
                        )
                    })
            })
            .map(|(_, _, comp)| (comp.pos.x(), comp.pos.y()))
            .collect()
    }

    /// Clears the player unit's queue (`BuilderComp.clearBuilding`).
    pub fn clear_player_build_plans(&mut self) {
        if let Some(unit) = self.player_unit
            && let Some(mut plans) = self.ecs.0.get_mut::<BuilderPlans>(unit)
        {
            plans.plans.clear();
        }
    }

    /// `BuilderComp.removeBuild`-style removal for the delete-plans command:
    /// drops every queued plan whose [`TilePos::pack`] is in `positions`
    /// (`InputHandler.deletePlans` over the player's own queue).
    pub fn delete_player_plans(&mut self, positions: &[i32]) {
        if let Some(unit) = self.player_unit
            && let Some(mut plans) = self.ecs.0.get_mut::<BuilderPlans>(unit)
        {
            plans
                .plans
                .retain(|plan| !positions.contains(&TilePos::new(plan.x, plan.y).pack()));
        }
    }

    /// `BuilderComp.updateBuilding` (`InputHandler.update`:
    /// `player.unit().updateBuilding = isBuilding`; PAR-0098 §2b).
    pub fn set_player_building_paused(&mut self, paused: bool) {
        if let Some(unit) = self.player_unit {
            // The component is inserted lazily by the first queued plan, so the
            // pause flag must create it when absent.
            self.with_player_plans(unit, |plans| plans.update_building = !paused);
        }
    }

    /// Construct progress at `(x, y)` as `(current block, progress,
    /// deconstructing)`, if the tile holds a construct block.
    pub fn construct_progress(&self, x: i16, y: i16) -> Option<(BlockId, f32, bool)> {
        let entity = self.grid.entity_at(TilePos::new(x, y))?;
        let state = self.ecs.0.get::<ConstructState>(entity)?;
        Some((state.current, state.progress, state.active_deconstruct))
    }
}

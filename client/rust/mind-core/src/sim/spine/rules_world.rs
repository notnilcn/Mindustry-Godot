// SPDX-License-Identifier: GPL-3.0-only

//! `Rules` application to the live world plus the map-area/debug team helpers.
//!
//! Upstream: core/src/mindustry/core/World.java.

use crate::sim::*;

impl Sim {
    /// Re-derives the world state that mirrors `Rules`: the map-area limit rect
    /// and border darkness (`World.isInMapArea`/`getDarkness`,
    /// `World.java:337-339, 483-503`), plus the build guard (G02 T8's
    /// `sync_build_rules`, called from here once it exists). Call after any
    /// rules change: sector start (`MindCampaign`), or mid-match through
    /// `CampaignRuntime.rules_world_dirty` (G06 `setrule`) / the gdext
    /// `apply_world_rules` wrapper.
    pub fn apply_rules_to_world(&mut self, rules: &crate::game::rules::Rules) {
        let new_limit = rules.limit_map_area.then_some((
            rules.limit_x,
            rules.limit_y,
            rules.limit_width,
            rules.limit_height,
        ));
        // `World.checkMapArea(prev)` (`World.java:469-481`): a shrunken area
        // re-enables disabled `autoResetEnabled` buildings outside the previous
        // rect before the new one is checked.
        let prev = self.grid.limit_area;
        if let Some(prev) = prev
            && Some(prev) != new_limit
        {
            self.reset_map_area(prev);
        }
        self.grid.limit_area = new_limit;
        self.grid.border_darkness = rules.border_darkness;
        // `World.setRules` + every `Build.validPlace` read of `state.rules`
        // (H02 T2 `sync_build_rules`); `is_campaign` comes from the live
        // campaign runtime when present.
        let is_campaign = self
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .map(|runtime| runtime.session.is_campaign())
            .unwrap_or(false);
        crate::world::limits::sync_build_rules(&mut self.ecs.0, rules, is_campaign);
        self.refresh_unit_runtime();
        // `WorldLoadEvent` half (`World.java:44-52`): every building re-checks
        // `allowUpdate` against the new rules.
        self.check_all_allow_update();
        // `WorldLoadEvent` fog half (`FogControl.java:46-62`): (re)size the fog
        // whenever the flags change or the resource is missing. Serpulo
        // (`!rules.fog`) removes the resource and pays nothing per tick.
        let flags = (rules.fog, rules.static_fog);
        let missing = flags.0
            && self
                .ecs
                .0
                .get_resource::<crate::game::fog::FogControl>()
                .is_none();
        if self.last_fog_flags != Some(flags) || missing {
            self.last_fog_flags = Some(flags);
            self.reset_fog();
        }
    }

    /// `World.checkAllowUpdate` for every live building (`World.java:44-52`).
    /// Editor worlds skip it (`EditorTile` is always allowed).
    pub fn check_all_allow_update(&mut self) {
        let (env, limit_area, disable_outside_area, editor) =
            match crate::game::runtime::rules_of(&self.ecs.0) {
                Some(rules) => (
                    rules.env,
                    rules.limit_map_area.then_some((
                        rules.limit_x,
                        rules.limit_y,
                        rules.limit_width,
                        rules.limit_height,
                    )),
                    rules.disable_outside_area,
                    rules.editor,
                ),
                None => (crate::game::rules::DEFAULT_ENV, None, true, false),
            };
        if editor {
            return;
        }
        let mut buildings: Vec<bevy_ecs::entity::Entity> = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| entity.contains::<crate::entities::comp::Building>())
            .map(|entity| entity.id())
            .collect();
        buildings.sort_by_key(|entity| entity.index());
        for entity in buildings {
            crate::world::allow_update::check_allow_update(
                &mut self.ecs.0,
                entity,
                env,
                limit_area,
                disable_outside_area,
            );
        }
    }

    /// `Tile.setTeam` for the building at `(x, y)` (`BuildingComp.changeTeam`
    /// -> `checkAllowUpdate`, `BuildingComp.java:1842`).
    pub fn debug_set_building_team(&mut self, x: i32, y: i32, team: u8) -> bool {
        let Some(entity) = self
            .grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.grid.tile(x, y).build)
            .flatten()
        else {
            return false;
        };
        let Some(mut comp) = self
            .ecs
            .0
            .get_mut::<crate::entities::comp::TeamComp>(entity)
        else {
            return false;
        };
        comp.team = team;
        let (env, limit_area, disable_outside_area) =
            match crate::game::runtime::rules_of(&self.ecs.0) {
                Some(rules) => (
                    rules.env,
                    rules.limit_map_area.then_some((
                        rules.limit_x,
                        rules.limit_y,
                        rules.limit_width,
                        rules.limit_height,
                    )),
                    rules.disable_outside_area,
                ),
                None => (crate::game::rules::DEFAULT_ENV, None, true),
            };
        crate::world::allow_update::check_allow_update(
            &mut self.ecs.0,
            entity,
            env,
            limit_area,
            disable_outside_area,
        );
        true
    }

    /// `World.checkMapArea(prevX, prevY, prevW, prevH)`'s re-enable half
    /// (`World.java:472-476`).
    pub(in crate::sim) fn reset_map_area(&mut self, prev: (i32, i32, i32, i32)) {
        let mut buildings: Vec<bevy_ecs::entity::Entity> = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| entity.contains::<crate::entities::comp::Building>())
            .map(|entity| entity.id())
            .collect();
        buildings.sort_by_key(|entity| entity.index());
        let (x, y, w, h) = prev;
        for entity in buildings {
            let (tile, auto_reset, enabled) = {
                let Some(building) = self.ecs.0.get::<crate::entities::comp::Building>(entity)
                else {
                    continue;
                };
                let Some(auto_reset) = self
                    .ecs
                    .0
                    .get_resource::<crate::world::BlockTable>()
                    .and_then(|table| table.instance(building.block))
                    .map(|instance| instance.def.auto_reset_enabled)
                else {
                    continue;
                };
                (building.tile, auto_reset, building.enabled)
            };
            let (tx, ty) = (tile.x() as i32, tile.y() as i32);
            if !enabled
                && auto_reset
                && !(tx >= x && ty >= y && tx < x + w && ty < y + h)
                && let Some(mut building) = self
                    .ecs
                    .0
                    .get_mut::<crate::entities::comp::Building>(entity)
            {
                building.enabled = true;
            }
        }
    }
}

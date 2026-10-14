// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BuildingComp.allowUpdate`/`checkAllowUpdate` and `World.checkMapArea`
//! (`BuildingComp.java:627-632,2261-2265`, `World.java:44-52,469-481`):
//! derelict, env-unsupported and outside-map-area buildings stop updating.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::BlockDef;
use crate::entities::comp::{Building, TeamComp};
use crate::world::TilePos;
use crate::world::build::supports_env_bits;

/// `BuildingComp.allowUpdate()` (`BuildingComp.java:627-632`).
///
/// `EditorTile` is the editor's always-allowed tile; callers skip this whole
/// pass when `rules.editor`.
pub fn allow_update(
    def: &BlockDef,
    team: u8,
    tile: TilePos,
    env: i32,
    limit_area: Option<(i32, i32, i32, i32)>,
    disable_outside_area: bool,
) -> bool {
    team != crate::game::team::DERELICT.0
        && supports_env_bits(def, env)
        && (def.privileged
            || limit_area.is_none()
            || !disable_outside_area
            || limit_area.is_some_and(|(x, y, w, h)| {
                let (tx, ty) = (tile.x() as i32, tile.y() as i32);
                tx >= x && ty >= y && tx < x + w && ty < y + h
            }))
}

/// `BuildingComp.checkAllowUpdate()` (`BuildingComp.java:2261-2265`): disable
/// the building when [`allow_update`] is false. Never re-enables.
pub fn check_allow_update(
    world: &mut World,
    e: Entity,
    env: i32,
    limit_area: Option<(i32, i32, i32, i32)>,
    disable_outside_area: bool,
) {
    let (tile, team, def) = {
        let Some(building) = world.get::<Building>(e) else {
            return;
        };
        let Some(team) = world.get::<TeamComp>(e).map(|team| team.team) else {
            return;
        };
        let Some(instance) = world
            .get_resource::<crate::world::BlockTable>()
            .and_then(|table| table.instance(building.block))
        else {
            return;
        };
        (building.tile, team, instance.def.clone())
    };
    if !allow_update(&def, team, tile, env, limit_area, disable_outside_area)
        && let Some(mut building) = world.get_mut::<Building>(e)
    {
        building.enabled = false;
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Fog gates for targeting and placement: `UnitComp.inFogTo`
//! (`UnitComp.java:232-248`), `BuildingComp.isDiscovered`/`inFogTo`
//! (`BuildingComp.java:315-333,2231-2249`) and the `ForceProjector`/
//! `BaseShield` "always visible" overrides, reading the live `FogControl`
//! resource (T1).
//!
//! No `CampaignRuntime`, no `rules.fog`, no `FogControl` or an AI viewer are
//! all no-ops, so non-campaign sims keep their current behaviour (combat
//! goldens unchanged).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::entities::comp::unit::comp::HitboxComp;
use crate::entities::comp::{Building, TeamComp};
use crate::game::fog::FogControl;
use crate::game::runtime::{CampaignRuntime, rules_of};
use crate::world::WorldGrid;
use crate::world::blocks::defense::projectors::BaseShieldState;
use crate::world::blocks::defense::shields::ForceProjectorState;
use crate::world::darkness::{border_darkness, get_wall_darkness};

/// `Geometry.d8` (arc `Geometry.java`): the eight tile-neighbour directions.
const D8: [(f32, f32); 8] = [
    (1.0, 0.0),
    (1.0, 1.0),
    (0.0, 1.0),
    (-1.0, 1.0),
    (-1.0, 0.0),
    (-1.0, -1.0),
    (0.0, -1.0),
    (1.0, -1.0),
];

/// `CampaignRuntime.session.is_campaign()` (single-player `state.isCampaign()`).
fn is_campaign(world: &World) -> bool {
    world
        .get_resource::<CampaignRuntime>()
        .is_some_and(|runtime| runtime.session.is_campaign())
}

/// `UnitComp.inFogTo(viewer)` (`UnitComp.java:232-248`).
///
/// A unit whose centre tile is not visible is hidden; large units
/// (`hitSize > 16`) probe a `d8` ring at `hitSize / 2` and are hidden only when
/// none of the probes is visible.
pub fn unit_in_fog_to(world: &World, unit: Entity, viewer: u8) -> bool {
    let Some(rules) = rules_of(world) else {
        return false;
    };
    if !rules.fog {
        return false;
    }
    if world.get::<TeamComp>(unit).map(|team| team.team) == Some(viewer) {
        return false;
    }
    let is_ai = rules.team_is_ai(viewer, is_campaign(world));
    let Some(pos) = world.get::<crate::entities::comp::Pos>(unit) else {
        return false;
    };
    let Some(fog) = world.get_resource::<FogControl>() else {
        // `fogControl.isVisible` with no team data returns false -> hidden.
        return true;
    };
    let hit_size = world
        .get::<HitboxComp>(unit)
        .map_or(0.0, |hitbox| hitbox.hit_size);
    if hit_size <= 16.0 {
        return !fog.is_visible(viewer, pos.x, pos.y, rules.fog, is_ai);
    }
    let trns = hit_size / 2.0;
    for (px, py) in D8 {
        if fog.is_visible(
            viewer,
            pos.x + px * trns,
            pos.y + py * trns,
            rules.fog,
            is_ai,
        ) {
            return false;
        }
    }
    true
}

/// `BuildingComp.isDiscovered(viewer)` (`BuildingComp.java:315-333`).
///
/// `content` resolves the block size and the darkness term (footprint tiles of
/// blocks larger than 2x2 also probe their corners).
pub fn building_discovered(
    world: &World,
    content: &ContentRegistry,
    grid: Option<&WorldGrid>,
    build: Entity,
    viewer: u8,
) -> bool {
    let size = world
        .get::<Building>(build)
        .and_then(|building| content.block(building.block))
        .map(|def| def.size);
    building_discovered_size(world, Some(content), grid, build, size, viewer)
}

/// [`building_discovered`] for callers without a content borrow
/// (`BlockIndexer.find_enemy_building`): the block size comes from the world's
/// `BlockTable` and the darkness term is skipped (`grid` is normally `None`).
pub fn building_discovered_table(
    world: &World,
    grid: Option<&WorldGrid>,
    build: Entity,
    viewer: u8,
) -> bool {
    let size = world.get::<Building>(build).and_then(|building| {
        world
            .get_resource::<crate::world::BlockTable>()
            .and_then(|table| table.instance(building.block))
            .map(|instance| instance.def.size)
    });
    building_discovered_size(world, None, grid, build, size, viewer)
}

/// Shared body of `BuildingComp.isDiscovered` with the block size pre-resolved
/// (`None` = unresolvable def, treated as discovered).
fn building_discovered_size(
    world: &World,
    content: Option<&ContentRegistry>,
    grid: Option<&WorldGrid>,
    build: Entity,
    size: Option<i32>,
    viewer: u8,
) -> bool {
    let Some(rules) = rules_of(world) else {
        return true;
    };
    let Some(building) = world.get::<Building>(build) else {
        return true;
    };
    let (tx, ty) = (building.tile.x() as i32, building.tile.y() as i32);

    // `state.rules.limitMapArea && world.getDarkness(tile.x, tile.y) >= 3`.
    if let Some(grid) = grid
        && grid.limit_area.is_some()
        && let Some(content) = content
    {
        let dark = (get_wall_darkness(grid, content, tx, ty) as f32).max(border_darkness(
            grid,
            grid.border_darkness,
            grid.limit_area,
            tx,
            ty,
        ));
        if dark >= 3.0 {
            return false;
        }
    }

    if !rules.static_fog || !rules.fog {
        return true;
    }
    let Some(size) = size else {
        return true;
    };
    let is_ai = rules.team_is_ai(viewer, is_campaign(world));
    let Some(fog) = world.get_resource::<FogControl>() else {
        // `getDiscovered` null -> undiscovered (BuildingComp.java:318-321).
        return false;
    };
    if size <= 2 {
        return fog.is_discovered(viewer, tx, ty, rules.static_fog, rules.fog, is_ai);
    }
    let s = size / 2;
    fog.is_discovered(viewer, tx, ty, rules.static_fog, rules.fog, is_ai)
        || fog.is_discovered(viewer, tx - s, ty - s, rules.static_fog, rules.fog, is_ai)
        || fog.is_discovered(viewer, tx - s, ty + s, rules.static_fog, rules.fog, is_ai)
        || fog.is_discovered(viewer, tx + s, ty + s, rules.static_fog, rules.fog, is_ai)
        || fog.is_discovered(viewer, tx + s, ty - s, rules.static_fog, rules.fog, is_ai)
}

/// `BuildingComp.inFogTo(viewer)` (`BuildingComp.java:2231-2249`) plus the
/// `ForceProjector.inFogTo`/`BaseShield.inFogTo` overrides
/// (`ForceProjector.java:202`, `BaseShield.java:121`): shields are always
/// visible.
pub fn building_in_fog_to(
    world: &World,
    content: &ContentRegistry,
    grid: Option<&WorldGrid>,
    build: Entity,
    viewer: u8,
) -> bool {
    let Some(rules) = rules_of(world) else {
        return false;
    };
    if !rules.fog {
        return false;
    }
    let Some(team) = world.get::<TeamComp>(build).map(|team| team.team) else {
        return false;
    };
    if team == viewer {
        return false;
    }
    if world.get::<ForceProjectorState>(build).is_some()
        || world.get::<BaseShieldState>(build).is_some()
    {
        return false;
    }
    if !building_discovered(world, content, grid, build, viewer) {
        return true;
    }
    let Some(building) = world.get::<Building>(build) else {
        return false;
    };
    let Some(def) = content.block(building.block) else {
        return true;
    };
    let is_ai = rules.team_is_ai(viewer, is_campaign(world));
    let (tx, ty) = (building.tile.x() as i32, building.tile.y() as i32);
    let (size, offset) = (def.size.max(1), def.size_offset);
    for x in 0..size {
        for y in 0..size {
            if fog_visible_tile(
                world,
                viewer,
                tx + x + offset,
                ty + y + offset,
                rules.fog,
                is_ai,
            ) {
                return false;
            }
        }
    }
    true
}

/// `FogControl.isVisibleTile` through the live resource (false without data).
fn fog_visible_tile(
    world: &World,
    team: u8,
    x: i32,
    y: i32,
    fog_enabled: bool,
    is_ai: bool,
) -> bool {
    world
        .get_resource::<FogControl>()
        .is_some_and(|fog| fog.is_visible_tile(team, x, y, fog_enabled, is_ai))
}

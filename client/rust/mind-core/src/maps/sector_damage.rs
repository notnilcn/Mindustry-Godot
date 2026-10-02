// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SectorDamage` (plan 06 §3.10). Ported from `maps/SectorDamage.java`.
//!
//! Buildings are plan 07's ECS runtime; this module operates on an explicit
//! [`DamageBuilding`] list supplied by the caller (plan 12's sector loader) and
//! the `Tiles` grid for pathing/floor data. The `Effect::rubble` call is a
//! plan-17 hook ([`DamageFx`]); the default implementation is empty.

use std::collections::VecDeque;

use crate::content::ContentRegistry;
use crate::determinism::{RngStream, SimRng};
use crate::maps::filters::block_info;
use crate::maps::generators::astar;
use crate::world::Tile;
use crate::world::tiles::Tiles;

/// One building relevant to sector damage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageBuilding {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Owning team.
    pub team: u8,
    /// Current health (`Building.health`).
    pub health: f32,
    /// Tile size (`Block.size`).
    pub size: i32,
    /// Whether this is a core (`Block instanceof CoreBlock`).
    pub core: bool,
    /// Set when the building has been destroyed.
    pub removed: bool,
}

impl DamageBuilding {
    /// Creates a live building.
    pub fn new(x: i32, y: i32, team: u8, health: f32, size: i32, core: bool) -> Self {
        Self {
            x,
            y,
            team,
            health,
            size: size.max(1),
            core,
            removed: false,
        }
    }
}

/// `Effect::rubble` hook (plan 17); the core default is empty.
pub trait DamageFx: Send + Sync {
    /// Spawns rubble at a destroyed building (`Effect.rubble`).
    fn rubble(&self, _x: f32, _y: f32, _size: i32) {}
}

/// An empty [`DamageFx`] (headless default).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopDamageFx;

impl DamageFx for NoopDamageFx {}

/// Inputs/state for [`apply`].
#[derive(Debug, Clone, Default)]
pub struct SectorDamageState {
    /// Player team (`state.rules.defaultTeam`).
    pub default_team: u8,
    /// Enemy team (`state.rules.waveTeam`).
    pub wave_team: u8,
    /// Spawnpoint tiles (`overlay == Blocks.spawn` or wave-team cores).
    pub spawns: Vec<(i32, i32)>,
    /// All buildings.
    pub buildings: Vec<DamageBuilding>,
}

impl SectorDamageState {
    /// Index of a live building at `(x, y)`, if any.
    pub fn building_at(&self, x: i32, y: i32) -> Option<usize> {
        self.buildings
            .iter()
            .position(|building| !building.removed && building.x == x && building.y == y)
    }
}

/// `SectorDamage.apply(fraction)`.
///
/// Returns the number of buildings destroyed. `rng` seeds the rubble chance
/// (deviation §2.3.5; upstream used `Mathf.chance`).
pub fn apply(
    tiles: &Tiles,
    content: &ContentRegistry,
    state: &mut SectorDamageState,
    fraction: f32,
    rng: &mut SimRng,
    fx: &dyn DamageFx,
) -> usize {
    let width = tiles.width;
    let height = tiles.height;
    if width <= 0 || height <= 0 {
        return 0;
    }

    // Phase one: collect spawnpoints, including wave-team cores.
    let mut frontier: VecDeque<(i32, i32)> = VecDeque::new();
    let mut values = vec![0.0f32; (width * height) as usize];
    for (x, y) in state.spawns.clone() {
        frontier.push_back((x, y));
        values[(x + y * width) as usize] = fraction * 24.0;
    }
    for building in &state.buildings {
        if building.core && building.team == state.wave_team && !building.removed {
            frontier.push_back((building.x, building.y));
            values[(building.x + building.y * width) as usize] = fraction * 24.0;
        }
    }

    let core_index = state.buildings.iter().position(|building| {
        building.core && building.team == state.default_team && !building.removed
    });

    let mut destroyed = 0usize;

    if let Some(core_index) = core_index
        && !frontier.is_empty()
    {
        let core = (state.buildings[core_index].x, state.buildings[core_index].y);
        for spawner in frontier.clone() {
            let path = astar::pathfind(
                tiles,
                spawner,
                core,
                |_, to| cost(content, state, to),
                |to| !(is_static(content, to) && to.solid(content)),
            );
            if path.is_empty() {
                continue;
            }

            let radius = 3;
            let total_health = if fraction >= 1.0 {
                1.0
            } else {
                path.iter()
                    .map(|(tx, ty)| {
                        neighborhood(
                            content,
                            state,
                            *tx,
                            *ty,
                            radius,
                            width,
                            height,
                            state.default_team,
                        )
                        .iter()
                        .map(|(_, health, size)| health / (size * size) as f32)
                        .sum::<f32>()
                    })
                    .sum()
            };
            let target_health = total_health * fraction;
            let mut health_count = 0.0f32;
            let mut removal: Vec<usize> = Vec::new();

            'path: for (tx, ty) in &path {
                for (index, health, size) in neighborhood(
                    content,
                    state,
                    *tx,
                    *ty,
                    radius,
                    width,
                    height,
                    state.default_team,
                ) {
                    let building = &state.buildings[index];
                    if building.core {
                        continue;
                    }
                    let floor = tiles.getc(building.x, building.y).floor;
                    if !floor_solid(content, floor)
                        && !floor_liquid(content, floor)
                        && rng.chance(RngStream::MapGen, 0.4)
                    {
                        fx.rubble(building.x as f32, building.y as f32, size);
                    }
                    health_count += health;
                    removal.push(index);
                    if health_count >= target_health && fraction < 0.999 {
                        break 'path;
                    }
                }
            }

            for index in removal {
                if !state.buildings[index].removed {
                    state.buildings[index].removed = true;
                    destroyed += 1;
                }
            }
        }
    }

    // Kill every core if damage is maximum.
    if fraction >= 1.0 {
        for building in &mut state.buildings {
            if building.core && building.team == state.default_team && !building.removed {
                building.removed = true;
                destroyed += 1;
            }
        }
    }

    // Phase two: propagate damage.
    if fraction > 0.15 {
        let falloff = fraction / ((width.max(height) as f32) * std::f32::consts::SQRT_2);
        while let Some((x, y)) = frontier.pop_front() {
            let curr_damage = values[(x + y * width) as usize] - falloff;
            for (dx, dy) in [(-1, 0), (0, -1), (1, 0), (0, 1)] {
                let cx = x + dx;
                let cy = y + dy;
                if cx < 0 || cy < 0 || cx >= width || cy >= height {
                    continue;
                }
                let cindex = (cx + cy * width) as usize;
                if values[cindex] >= curr_damage {
                    continue;
                }
                let mut result_damage = curr_damage;
                if let Some(index) = state.building_at(cx, cy)
                    && state.buildings[index].team == state.default_team
                {
                    let building = &mut state.buildings[index];
                    result_damage -= building.health;
                    building.health -= curr_damage;
                    if building.core {
                        building.health = building.health.max(1.0);
                    }
                    if building.health < 0.0 {
                        let floor = tiles.getc(cx, cy).floor;
                        if !floor_solid(content, floor)
                            && !floor_liquid(content, floor)
                            && rng.chance(RngStream::MapGen, 0.4)
                        {
                            fx.rubble(cx as f32, cy as f32, building.size);
                        }
                        building.removed = true;
                        destroyed += 1;
                    }
                } else if tiles
                    .getn(cx, cy)
                    .is_some_and(|tile| tile.solid(content) && !synthetic(content, tile.block))
                {
                    continue;
                }

                if result_damage > 0.0 && values[cindex] < result_damage {
                    frontier.push_back((cx, cy));
                    values[cindex] = result_damage;
                }
            }
        }
    }

    destroyed
}

/// `SectorDamage.cost`.
fn cost(content: &ContentRegistry, state: &SectorDamageState, tile: &Tile) -> f32 {
    let mut value = 1.0;
    if is_static(content, tile) && tile.solid(content) {
        value += 200.0;
    }
    if let Some(index) = state.building_at(tile.x as i32, tile.y as i32) {
        let building = &state.buildings[index];
        value += building.health / (building.size * building.size) as f32 / 20.0;
    }
    if floor_liquid(content, tile.floor) {
        value += 10.0;
    }
    value
}

/// Player-team, non-core buildings in a radius-`radius` disc around `(cx, cy)`.
#[allow(clippy::too_many_arguments)]
fn neighborhood(
    content: &ContentRegistry,
    state: &SectorDamageState,
    cx: i32,
    cy: i32,
    radius: i32,
    width: i32,
    height: i32,
    team: u8,
) -> Vec<(usize, f32, i32)> {
    let mut out = Vec::new();
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            let wx = dx + cx;
            let wy = dy + cy;
            if wx < 0
                || wy < 0
                || wx >= width
                || wy >= height
                || dx * dx + dy * dy > radius * radius
            {
                continue;
            }
            if let Some(index) = state.building_at(wx, wy) {
                let building = &state.buildings[index];
                if building.team == team && !building.core {
                    let _ = content;
                    out.push((index, building.health, building.size));
                }
            }
        }
    }
    out
}

fn is_static(content: &ContentRegistry, tile: &Tile) -> bool {
    content.block(tile.block).is_some_and(block_info::is_static)
}

fn synthetic(content: &ContentRegistry, block: crate::content::BlockId) -> bool {
    content.block(block).is_some_and(block_info::synthetic)
}

fn floor_liquid(content: &ContentRegistry, floor: crate::content::BlockId) -> bool {
    content.block(floor).is_some_and(block_info::is_liquid)
}

fn floor_solid(content: &ContentRegistry, floor: crate::content::BlockId) -> bool {
    content.block(floor).is_some_and(|def| def.solid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::determinism::SimRng;

    fn registry() -> ContentRegistry {
        crate::content::test_support::test_registry()
    }

    fn flat(content: &ContentRegistry, size: i32) -> Tiles {
        let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
        let mut tiles = Tiles::new(size, size);
        for index in 0..tiles.len() {
            tiles.geti_mut(index).floor = stone;
        }
        tiles
    }

    /// Partial damage removes some player buildings on the path to the core,
    /// but never the core itself.
    #[test]
    fn frontier_damage() {
        let content = registry();
        let tiles = flat(&content, 16);
        // A wall of player buildings across x = 7: any spawn->core path must
        // pass through one of them (buildings do not block the A* itself).
        let mut buildings = vec![
            DamageBuilding::new(0, 0, 2, 100.0, 3, true),
            DamageBuilding::new(15, 15, 1, 1000.0, 3, true),
        ];
        for y in 0..16 {
            buildings.push(DamageBuilding::new(7, y, 1, 100.0, 1, false));
        }
        let mut state = SectorDamageState {
            default_team: 1,
            wave_team: 2,
            spawns: vec![(0, 0)],
            buildings,
        };
        let mut rng = SimRng::new(1);
        let destroyed = apply(&tiles, &content, &mut state, 0.5, &mut rng, &NoopDamageFx);
        assert!(destroyed > 0, "some buildings should be destroyed");
        assert!(
            !state.buildings[1].removed,
            "the player core must survive partial damage"
        );
    }

    /// `fraction >= 1` kills every player core.
    #[test]
    fn full_fraction_kills_cores() {
        let content = registry();
        let tiles = flat(&content, 16);
        let mut state = SectorDamageState {
            default_team: 1,
            wave_team: 2,
            spawns: vec![(0, 0)],
            buildings: vec![
                DamageBuilding::new(0, 0, 2, 100.0, 3, true),
                DamageBuilding::new(15, 15, 1, 1000.0, 3, true),
                DamageBuilding::new(8, 8, 1, 100.0, 1, false),
            ],
        };
        let mut rng = SimRng::new(7);
        let destroyed = apply(&tiles, &content, &mut state, 1.0, &mut rng, &NoopDamageFx);
        assert!(destroyed > 0);
        assert!(
            state.buildings[1].removed,
            "full damage must destroy the player core"
        );
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! World/render hook seams (plan 06 §3.2, risk R2/R9).
//!
//! Plan 06 needs `newBuilding`, `onRemoved`, proximity, `blockChanged`,
//! `floorChanged` and `legacy remove` before plan 07 exists. They are traits
//! with no-op defaults here; plan 07 replaces the implementation without
//! changing the `world::ops` call sites.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::BlockId;
use crate::io::json::SpawnGroup;
use crate::maps::{GameMode, Map};

/// Request to create a building entity for a placed multiblock/single block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewBuilding {
    /// Block being placed.
    pub block: BlockId,
    /// Center tile x.
    pub x: i16,
    /// Center tile y.
    pub y: i16,
    /// Owning team.
    pub team: u8,
    /// Rotation.
    pub rot: u8,
}

/// Building-lifecycle hooks (`WorldHooks`).
///
/// The default `new_building` returns `None`, so plan 06's core stays free of
/// plan 07's building runtime; tests and plan 07 inject an implementation that
/// spawns a `BuildingComp` entity.
pub trait WorldHooks: Send + Sync {
    /// Creates the building entity for a placed block.
    fn new_building(&self, world: &mut World, request: NewBuilding) -> Option<Entity> {
        let _ = (world, request);
        None
    }

    /// Called before a building is removed (`Building.onRemoved`).
    fn on_removed(&self, world: &mut World, entity: Entity) {
        let _ = (world, entity);
    }

    /// Recomputes proximity for a building (`Building.updateProximity`).
    fn update_proximity(&self, world: &mut World, entity: Entity) {
        let _ = (world, entity);
    }

    /// A block changed on a tile (`Block.blockChanged`).
    fn block_changed(&self, _block: BlockId, _x: i16, _y: i16) {}

    /// A floor changed on a tile (`Floor.floorChanged`).
    fn floor_changed(&self, _floor: BlockId, _x: i16, _y: i16) {}

    /// Removes legacy blocks when loading an old world
    /// (`Block.legacyRemoveSelf`); plan 07/02 body.
    fn legacy_remove_self(&self, world: &mut World, _block: BlockId, _x: i16, _y: i16) {
        let _ = world;
    }

    /// Re-runs the allow-update check for a building (`WorldLoadEvent` body).
    fn check_allow_update(&self, world: &mut World, entity: Entity) {
        let _ = (world, entity);
    }

    /// Power-graph reflow after `setTeam` (plan 09 hook).
    fn team_changed(&self, _x: i16, _y: i16, _team: u8) {}
}

/// Render invalidation hooks (`RenderHooks`); implementation is plan 16.
pub trait RenderHooks: Send + Sync {
    /// Recache one tile's static sprites.
    fn recache_tile(&self, _x: i16, _y: i16) {}
    /// Recache one wall tile.
    fn recache_wall(&self, _x: i16, _y: i16) {}
    /// Add a tile to the floor index (plan 11/16).
    fn add_floor_index(&self, _x: i16, _y: i16) {}
    /// Remove a tile from the floor index.
    fn remove_floor_index(&self, _x: i16, _y: i16) {}
    /// Invalidates a tile's cached draw.
    fn invalidate_tile(&self, _x: i16, _y: i16) {}
    /// Marks the minimap dirty.
    fn minimap_update(&self, _x: i16, _y: i16) {}
}

/// A no-op [`WorldHooks`] (core default).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopWorldHooks;

impl WorldHooks for NoopWorldHooks {}

/// A no-op [`RenderHooks`] (core default).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopRenderHooks;

impl RenderHooks for NoopRenderHooks {}

/// Map-registry/generation hooks (plan 06 §3.9/§3.10, risk R3).
///
/// Plan 11 supplies the default wave groups (`Waves.get()`) and plan 12 owns
/// gamemode validity (`Gamemode.valid`). The defaults keep 06 runnable before
/// those plans land (no waves; the upstream `ShuffleMode.valid` rule).
pub trait MapGenHooks: Send + Sync {
    /// Default wave groups for maps without explicit spawns (`Waves.get`).
    fn wave_groups(&self) -> Vec<SpawnGroup> {
        Vec::new()
    }

    /// Whether `map` is valid for `mode` (`ShuffleMode.valid`).
    fn valid_for_mode(&self, mode: GameMode, map: &Map) -> bool {
        crate::maps::shuffle::default_valid(mode, map)
    }
}

/// A no-op [`MapGenHooks`] (core default).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopMapGenHooks;

impl MapGenHooks for NoopMapGenHooks {}

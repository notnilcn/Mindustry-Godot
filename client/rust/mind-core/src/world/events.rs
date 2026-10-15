// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! World tile-event payloads (plan 06 §3.4).
//!
//! Deviation §2.3.3: Java reuses static event instances carrying `Tile` refs;
//! these are by-value structs. Plan 05's `EventBus` registers them append-only;
//! until that registration lands the world layer tracks `tile_changes` /
//! `floor_changes` counters directly (observably identical — plan 06 §3.4).

use crate::content::BlockId;

/// A tile content change (`TileChangeEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileChangeEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
}

/// A tile is about to change (`TilePreChangeEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TilePreChangeEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
}

/// A floor change (`TileFloorChangeEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileFloorChangeEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Previous floor id.
    pub prev: BlockId,
    /// New floor id.
    pub next: BlockId,
}

/// An overlay change (`TileOverlayChangeEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileOverlayChangeEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Previous overlay id.
    pub prev: BlockId,
    /// New overlay id.
    pub next: BlockId,
}

/// Fired before a world load begins (`WorldLoadBeginEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorldLoadBeginEvent;

/// Fired after a world load's tiles are read (`WorldLoadEndEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorldLoadEndEvent;

/// Fired when a world load fully completes (`WorldLoadEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorldLoadEvent;

/// A build/deconstruct action began (`BlockBuildBeginEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockBuildBeginEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Team performing the action.
    pub team: u8,
    /// Whether this is a deconstruction.
    pub breaking: bool,
}

/// A build/deconstruct action ended (`BlockBuildEndEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockBuildEndEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Team performing the action.
    pub team: u8,
    /// Whether this was a deconstruction.
    pub breaking: bool,
    /// Config carried through the finish, if any.
    pub has_config: bool,
}

/// A building rotated (`BuildRotateEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildRotateEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Previous rotation.
    pub previous: u8,
    /// New rotation.
    pub rotation: u8,
}

/// A building's team changed (`BuildTeamChangeEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildTeamChangeEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Previous team.
    pub previous: u8,
    /// New team.
    pub team: u8,
}

/// A building took damage (`BuildDamageEvent`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildDamageEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Damage amount.
    pub amount: f32,
    /// Team of the damaged building (`Control.java:68-72` filters by it).
    pub team: u8,
}

/// Undrained `BuildDamageEvent`s for the current tick (G01 T15). Bounded at
/// [`MAX_BUILD_DAMAGE_LOG`] entries.
#[derive(Debug, Default, bevy_ecs::prelude::Resource)]
pub struct BuildDamageLog(pub Vec<BuildDamageEvent>);

/// Cap for [`BuildDamageLog`] when nobody drains it (plan G01 T15).
pub const MAX_BUILD_DAMAGE_LOG: usize = 4096;

/// One bullet collision against a `flashHit` wall (`Wall.WallBuild.collision`
/// sets `hit = 1`, `Wall.java:164-166`). Carries the wall's tile; the renderer
/// owns the `hit` decay (`draw()`), so nothing about the flash is sim state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallFlashEvent {
    /// Wall anchor tile x.
    pub x: i16,
    /// Wall anchor tile y.
    pub y: i16,
}

/// Undrained `WallFlashEvent`s for the current tick (I02 T4). Bounded at
/// [`MAX_WALL_FLASH_LOG`] entries.
#[derive(Debug, Default, bevy_ecs::prelude::Resource)]
pub struct WallFlashLog(pub Vec<WallFlashEvent>);

/// Cap for [`WallFlashLog`] when nobody drains it (I02 T4).
pub const MAX_WALL_FLASH_LOG: usize = 1024;

/// Pushes one wall-flash event (no-op when absent or full).
pub fn push_wall_flash(world: &mut bevy_ecs::world::World, x: i16, y: i16) {
    if let Some(mut log) = world.get_resource_mut::<WallFlashLog>()
        && log.0.len() < MAX_WALL_FLASH_LOG
    {
        log.0.push(WallFlashEvent { x, y });
    }
}

/// Pushes one event into the log (no-op when absent or full).
pub fn push_build_damage(world: &mut bevy_ecs::world::World, event: BuildDamageEvent) {
    if let Some(mut log) = world.get_resource_mut::<BuildDamageLog>()
        && log.0.len() < MAX_BUILD_DAMAGE_LOG
    {
        log.0.push(event);
    }
}

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

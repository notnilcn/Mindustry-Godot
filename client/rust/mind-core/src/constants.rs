// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Simulation constants shared by plans 02/04/05/06+ (plan 05 §6.6).
//!
//! Ported from `core/src/mindustry/Vars.java` (`tilesize`, `maxBlockSize`,
//! `finalWorldBounds`) and the fixed-step loop (`Logic.java`).

pub use crate::config::{MAX_BLOCK_SIZE, TILESIZE};

/// Fixed simulation rate (D8).
pub const TICKS_PER_SECOND: u32 = 60;

/// Fixed step in seconds.
pub const FIXED_HZ: f64 = 60.0;

/// Maximum fixed steps simulated per rendered frame (`Vars.maxDeltaClient/Server`).
pub const MAX_TICKS_PER_FRAME: u32 = 4;

/// Canonical checksum stream version (plan 05 §6.5; parity/checksum_registry.json).
pub const CHECKSUM_VERSION: u32 = 1;

/// Half-extent of the final playable world bounds (`Vars.finalWorldBounds`).
pub const FINAL_WORLD_BOUNDS: f32 = 250.0;

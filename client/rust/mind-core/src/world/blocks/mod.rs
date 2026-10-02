// SPDX-License-Identifier: GPL-3.0-only

//! The `blocks/` subtree of `world` (upstream `world/blocks/` mirror).
//!
//! Plan 06 owns the tile-grid implementation; plan 03 contributes
//! [`tile_bitmask`] (the 47-slice autotile table).

pub mod liquid;
pub mod power;
pub mod tile_bitmask;

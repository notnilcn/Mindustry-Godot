// SPDX-License-Identifier: GPL-3.0-only

//! The `blocks/` subtree of `world` (upstream `world/blocks/` mirror).
//!
//! Plan 03 contributes [`tile_bitmask`] (the 47-slice autotile table). Plan 08
//! owns [`autotiler`] (blend state), the [`distribution`] transport family and
//! the [`storage`] family.

pub mod autotiler;
pub mod distribution;
pub mod heat;
pub mod liquid;
pub mod power;
pub mod storage;
pub mod tile_bitmask;

use crate::content::ContentRegistry;
use crate::world::behavior::BehaviorRegistry;

/// Builds the default behavior registry (plan 07 fallbacks + plan 08 families).
pub fn default_registry(content: &ContentRegistry) -> BehaviorRegistry {
    let mut registry = BehaviorRegistry::new();
    distribution::register(&mut registry, content);
    storage::register(&mut registry, content);
    heat::register(&mut registry, content);
    power::register(&mut registry, content);
    registry
}

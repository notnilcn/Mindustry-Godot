// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `StorageBlock`/`CoreBlock`/`Unloader` family (`world/blocks/storage/`) — plan 08.
//!
//! M0 provides the plain [`StorageBuild`] deposit path (`vault`/`container`);
//! `linkedCore` forwarding, the core inventory algorithm and the full unloader
//! comparator land in milestones M4/M5.

pub mod storage_block;
pub mod unloader;

use std::sync::Arc;

use crate::content::ContentRegistry;
use crate::world::behavior::BehaviorRegistry;

pub use storage_block::{StorageBehavior, StorageBuild};
pub use unloader::{ContainerStat, UnloaderBehavior, UnloaderBuild};

/// Registers the storage-family behaviors available at this milestone.
pub fn register(registry: &mut BehaviorRegistry, _content: &ContentRegistry) {
    for name in [
        "container",
        "vault",
        "reinforced-container",
        "reinforced-vault",
    ] {
        registry.register_named(name, Arc::new(StorageBehavior));
    }
    registry.register_named("unloader", Arc::new(UnloaderBehavior::VANILLA));
}

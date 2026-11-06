// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `StorageBlock`/`CoreBlock`/`Unloader` family (`world/blocks/storage/`) — plan 08.
//!
//! M0 provides the plain [`StorageBuild`] deposit path (`vault`/`container`);
//! `linkedCore` forwarding, the core inventory algorithm and the full unloader
//! comparator land in milestones M4/M5.

pub mod core_block;
pub mod storage_block;
pub mod unloader;

use std::sync::Arc;

use crate::content::{BlockKind, ContentRegistry};
use crate::world::behavior::{BehaviorRegistry, BuildingBehavior};

pub use core_block::{
    CoreBehavior, CoreBuild, CoreCampaignHooks, CoreHooks, NoopCoreCampaignHooks,
};
pub use storage_block::{StorageBehavior, StorageBuild};
pub use unloader::{ContainerStat, UnloaderBehavior, UnloaderBuild};

/// Class-default behavior for a storage [`BlockKind`] (GAPS #55).
///
/// `StorageBlock`/`CoreBlock`/`Unloader` dispatch by Java class upstream, so a
/// mod block of either class gets the family loop instead of `NoopBehavior`.
pub fn class_default(kind: BlockKind) -> Option<Arc<dyn BuildingBehavior>> {
    Some(match kind {
        BlockKind::StorageBlock => Arc::new(StorageBehavior),
        BlockKind::CoreBlock => Arc::new(CoreBehavior),
        BlockKind::Unloader => Arc::new(UnloaderBehavior::VANILLA),
        _ => return None,
    })
}

/// Registers the storage-family behaviors available at this milestone.
pub fn register(registry: &mut BehaviorRegistry, _content: &ContentRegistry) {
    for kind in [
        BlockKind::StorageBlock,
        BlockKind::CoreBlock,
        BlockKind::Unloader,
    ] {
        if let Some(behavior) = class_default(kind) {
            registry.register_kind(kind, behavior);
        }
    }
    for name in [
        "container",
        "vault",
        "reinforced-container",
        "reinforced-vault",
    ] {
        registry.register_named(name, Arc::new(StorageBehavior));
    }
    registry.register_named("unloader", Arc::new(UnloaderBehavior::VANILLA));
    for name in [
        "core-shard",
        "core-foundation",
        "core-nucleus",
        "core-bastion",
        "core-citadel",
        "core-acropolis",
    ] {
        registry.register_named(name, Arc::new(CoreBehavior));
    }
}

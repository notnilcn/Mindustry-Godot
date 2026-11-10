// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Opt-in live simulation runtime: the plan-07 building stack on `Sim`.
//!
//! The P0 `Sim` path stores a bare `BuildingComp` and a grid cell per placed
//! block, so every building is inert (gap5 GAP-1, inventory R0-2). This module
//! provisions the real runtime — [`BlockTable`], `BuildRules`, `BuildClock`,
//! `TileBuilds`, `ModuleDims` and [`PowerGrids`] — into the `Sim` ECS and routes
//! `Place`/`Break` through the plan-06 [`WorldCtx`] tile ops so entities carry
//! the full plan-07 component set and the schedule's `UpdateBuildings`/
//! `UpdatePowerGraph` systems have something to run.
//!
//! Ported from `core/src/mindustry/core/Logic.java` (`updateEntities`),
//! `entities/comp/BuildingComp.java` (`update`/`updateConsumption`,
//! `onProximityAdded`/`onProximityRemoved`) and `world/Tile.java` (`setBlock`).
//!
//! The runtime is explicitly opt-in: a `Sim` that never calls
//! [`Sim::install_block_runtime`] is byte-identical to the P0 spine (no ECS
//! resources, no extra checksum parts), so the plan-05 checkpoint goldens stay
//! frozen. Placement uses the harness instant-build convention (there is no
//! builder-unit progress path in the live spine yet).

mod block_runtime;
mod campaign;
mod dynamic_consume;
mod hooks;
mod install;
mod load;
mod materialize;
mod passes;
mod teams;
#[cfg(test)]
mod tests;
mod tile_ops;

use hooks::SimHooks;

pub use block_runtime::SimRuntime;
pub use dynamic_consume::SimDynamicConsume;

use std::sync::atomic::{AtomicU64, Ordering};

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{
    Attribute, BlockId, BlockKind, BuildVisibility, ContentError, ContentRegistry, ContentType,
    ItemId, LiquidId, MemoryBundle, MemoryUnlockStore, UnitTypeId, create_base_content,
};
use crate::determinism::{Checksum, Checksummer, CommandError};
use crate::ecs::{BuildingComp, TeamId};
use crate::game::runtime::CampaignRuntime;
use crate::world::block::{BlockError, BlockTable};
use crate::world::blocks::power::{
    PowerGrids, PowerNodeConfig, power_graph_removed, read_power_info, update_power_graph,
};
use crate::world::building_io::{self, DecodedBase};
use crate::world::config::ConfigValue as WorldConfigValue;
use crate::world::limits::{BlockCounter, BuildRules};
use crate::world::modules::{ItemModule, LiquidModule, LiquidTraits, ModuleDims, PowerModule};
use crate::world::ops::{WorldCtx, WorldEventLog};
use crate::world::proximity::{remove_from_proximity, update_proximity};
use crate::world::update::BuildClock;
use crate::world::{NewBuilding, NoopRenderHooks, TileBuilds, TilePos, WorldGrid, WorldHooks};

use super::Sim;

/// Errors raised while installing the live runtime.
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum RuntimeError {
    /// Content bootstrap failed.
    #[error("live runtime content boot failed: {0}")]
    Content(#[from] ContentError),
    /// The block behavior table could not be built.
    #[error("live runtime block table failed: {0}")]
    BlockTable(#[from] BlockError),
}

/// Live-runtime configuration (all gating is default-off).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SimRuntimeConfig {
    /// Gate placement on `BuildVisibility`/unlock state (`Block.canBeBuilt`,
    /// `UnlockableContent.unlockedNow`; gap inventory K-9). Off by default so a
    /// vanilla custom game can build everything.
    pub placement_gate: bool,
    /// Whether the match is a campaign (`state.isCampaign()`); when set (with
    /// `placement_gate`), locked content cannot be placed.
    pub campaign: bool,
}

impl SimRuntimeConfig {
    /// Enables the placement gate.
    pub const fn with_placement_gate(mut self, placement_gate: bool) -> Self {
        self.placement_gate = placement_gate;
        self
    }

    /// Marks the match as a campaign (locked content is unplaceable).
    pub const fn campaign(mut self, campaign: bool) -> Self {
        self.campaign = campaign;
        self
    }
}

/// Content name → dense-id index for scheduled systems that resolve content
/// names over the ECS (`ObjectiveEnv.team_has_item`/`placed_block_count`);
/// built once by the runtime install from its content registry.
#[derive(bevy_ecs::prelude::Resource, Debug, Default)]
pub struct ContentIndex {
    /// Item name → dense id (`ContentRegistry::item_id`).
    pub item_ids: std::collections::HashMap<String, u16>,
    /// Dense item id → name (id-indexed; `ContentRegistry::items` order).
    pub item_names: Vec<String>,
    /// Dense item id → `Item.buildable` (`CoreBlock.incinerateNonBuildable`).
    pub item_buildable: Vec<bool>,
    /// Block name → dense id (`ContentRegistry::block_id`).
    pub block_ids: std::collections::HashMap<String, u16>,
    /// Unit name → dense id (`ContentRegistry::unit_id`; `UnitCountObjective`).
    pub unit_ids: std::collections::HashMap<String, u16>,
    /// Dense unit id → name (id-indexed; `ContentRegistry::units` order).
    pub unit_names: Vec<String>,
}

/// Boots the vanilla content registry the way the harness does.
pub(crate) fn load_vanilla_content() -> Result<ContentRegistry, RuntimeError> {
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)?;
    content.init()?;
    content.post_init()?;
    // `Blocks.java` turret `ammo(...)` tables: the anonymous ammo bullets join
    // the registry so placed turrets resolve a live `TurretBehavior` (PAR-0086).
    crate::world::blocks::defense::turrets::install_turret_ammo(&mut content);
    Ok(content)
}

/// One building captured for the lost-sector replay (`Control.playSector`'s
/// `previousBuildings`/`previousDerelicts` payload).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplayBuilding {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Block content id.
    pub block: crate::content::BlockId,
    /// Rotation.
    pub rot: u8,
    /// Current health.
    pub health: f32,
}

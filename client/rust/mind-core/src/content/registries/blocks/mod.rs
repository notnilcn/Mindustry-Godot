// SPDX-License-Identifier: GPL-3.0-only

//! Block metadata registry (plan 02 M3/M4).
//!
//! Ported from `core/src/mindustry/content/Blocks.java` (metadata half) and
//! `core/src/mindustry/world/Block.java` (`init`/`postInit`/`researchRequirements`
//! metadata derivations). Behavior (`update`, `acceptItem`, placement,
//! consumers execution, multiblocks) is owned by plan 07.
//!
//! ## Layout
//!
//! * [`BlockDef`] — the full metadata record; derived fields are filled by
//!   [`Content::init_self`] exactly like `Block.init()`.
//! * [`BlockSpec`] — the generated wave input (name-based references, class
//!   defaults from [`BlockSpec::for_kind`]).
//! * [`Blocks`] — the sim-facing name/id table (`Sim.content`); it is a thin
//!   consumer of the same generated waves so scenario block names stay valid.
//! * waves [`environment`], [`ore`], [`crafting`], [`defense`] (M3) and the
//!   B3–B6 modules (M4) — generated from `Blocks.java` in upstream order.

pub mod campaign;
pub mod crafting;
pub mod defense;
pub mod distribution;
pub mod drawer;
pub mod environment;
pub mod legacy;
pub mod liquid;
pub mod logic;
pub mod ore;
pub mod payloads;
pub mod power;
pub mod production;
pub mod sandbox;
pub mod storage;
pub mod turret_parts;
pub mod turrets;
pub mod units;
pub mod units_erekir;

mod consume;
mod def;
mod kind;
mod loader;
mod spec;
mod traits;
mod types;
#[cfg(test)]
mod tests;

pub use consume::*;
pub use def::*;
pub use kind::*;
pub use loader::*;
pub use spec::*;
pub use traits::*;
pub use types::*;
pub use super::super::registries::planets::EnvFlag;

/// Base tile size constant (`Block.tilesize`).
pub const TILE_SIZE: f32 = 8.0;

/// `Block.unitCapModifier` base (upstream `Block` default is 0, but cores set 1+).
pub const BASE_UNIT_CAP_MODIFIER: i32 = 0;

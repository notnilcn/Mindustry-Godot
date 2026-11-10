// SPDX-License-Identifier: GPL-3.0-only

//! The `Sim` spine: the `impl Sim` bodies that used to live in `sim/mod.rs`, plus the
//! unit-ability drain system registered by [`Sim::new`].
//!
//! Upstream: core/src/mindustry/core/Logic.java (`update`, `updateEntities`), and the
//! per-file citations in each child module.

mod accessors;
mod apply;
mod checksum;
mod command_dispatch;
mod construct;
mod event_flush;
mod live_unit_cmd;
mod logic_bridge;
mod placement;
mod rules_world;
mod tick;
mod world_config;

mod abilities;

pub use abilities::KIND_BUILDING;
pub(crate) use abilities::{drain_ability_queues_system, spawn_logic_bullet};

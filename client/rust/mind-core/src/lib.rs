// SPDX-License-Identifier: GPL-3.0-only

//! `mind-core` — the Godot-free, tokio-free Mindustry simulation core.
//!
//! All simulation, content, world, IO and campaign logic lives here and must stay
//! testable with plain `cargo test -p mind-core` (no Godot, no network).
//!
//! P0 module set is defined by `00_FOUNDATION_IMPLEMENTATION_PLAN.md` §3.5; later
//! plans extend these modules in place.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

// Lets `mind_derive` output reference `::mind_core` inside this crate itself.
extern crate self as mind_core;

pub mod assets;
pub mod command;
pub mod config;
pub mod content;
pub mod ecs;
pub mod event;
pub mod game;
pub mod io;
pub mod log;
pub mod random;
pub mod scenario;
pub mod schedule;
pub mod sim;
pub mod time;
pub mod version;
pub mod world;

/// Version of the `mind-core` crate, taken from `Cargo.toml`.
pub use version::MIND_VERSION;

/// The canonical JSON state-dump types (also reachable as `mind_core::sim::dump`).
pub use sim::dump;

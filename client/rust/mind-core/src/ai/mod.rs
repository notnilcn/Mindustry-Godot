// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit AI, pathfinding and the headless unit harness (plan 11).
//!
//! M0 ships the movement/pathfinding core: the controller framework, `GroundAI`,
//! `Pathfinder` cost-ground flowfields, and [`UnitHarness`]. RTS (`CommandAI`),
//! team AI and waves land with M4–M6.

pub mod controller;
pub mod harness;
pub mod pathfinder;
pub mod types;

pub use controller::{AiKind, ControllerSlot, UnitController, select_ai};
pub use harness::UnitHarness;
pub use pathfinder::{Cost, Flowfield, PathTile, Pathfinder};

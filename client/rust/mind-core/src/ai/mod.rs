// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit AI, pathfinding and the headless unit harness (plan 11).
//!
//! M0 ships the movement/pathfinding core: the controller framework, `GroundAI`,
//! `Pathfinder` cost-ground flowfields, and [`UnitHarness`]. M3 adds `Astar` and
//! the `ControlPathfinder` bit packing; M4 adds `UnitCommand`/`UnitStance`
//! runtime helpers and `UnitGroup` formation. RTS `CommandAI`, team AI and waves
//! land with M4–M6.

pub mod astar;
pub mod control_structs;
pub mod controller;
pub mod harness;
pub mod pathfinder;
pub mod types;
pub mod unit_command_runtime;
pub mod unit_group;
pub mod unit_stance_runtime;

pub use astar::{AstarScratch, DistanceHeuristic, TileHeuristic, manhattan};
pub use control_structs::{FieldIndex, IntraEdge, NodeIndex};
pub use controller::{AiKind, ControllerSlot, UnitController, select_ai};
pub use harness::UnitHarness;
pub use pathfinder::{Cost, Flowfield, PathTile, Pathfinder};
pub use types::command::{AttackTarget, CommandAiState, CommandQueueEntry};
pub use unit_command_runtime::{
    allows_command, command_controller, default_command, extra_stances, get_unit_stances,
};
pub use unit_group::UnitGroup;
pub use unit_stance_runtime::{
    StanceBits, apply_stance, disable_stance, set_stance, set_stance_enabled,
};

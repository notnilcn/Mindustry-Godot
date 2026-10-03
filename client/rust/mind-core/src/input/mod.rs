// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Input, placement & RTS (plan 15).
//!
//! The pure input state machine, placement algorithms, plan mirror and RTS
//! selection live here (Godot-free/tokio-free, deviation I1). Godot event
//! translation, node plumbing and settings persistence live in
//! `mind-gdext::input`. None of this registers an ECS system and none of it
//! enters the checksum (I2/I3).

pub mod action;
pub mod binding;
pub mod caps;
pub mod client_input;
pub mod command_emit;
pub mod desktop;
pub mod focus;
pub mod input_log;
pub mod line;
pub mod place_mode;
pub mod placement;
pub mod plan;
pub mod queue;
pub mod replay;
pub mod rts;

pub use action::{CommandTarget, InventoryKind, PayloadAction, RemoteAction};
pub use binding::{
    BINDS, BindingDefault, BindingState, BindingValue, Category, KeyBind, KeyBindId, KeyBindTable,
    KeyKind, ids,
};
pub use caps::{InputCaps, TestCaps};
pub use client_input::{CONTROL_GROUP_DOUBLE_TAP_MS, CONTROL_GROUPS, InputState};
pub use command_emit::{
    ActionBatch, ActionBatcher, ActionError, COMMAND_CHUNK, DELETE_POSITION_CAP,
};
pub use desktop::{DesktopController, ScrollOutcome};
pub use focus::{FocusGuards, FocusState, InputLocks, LockId};
pub use input_log::{
    INPUT_LOG_FORMAT, InputHeader, InputLog, InputLogError, InputRecord, InputReplay, RawEvent,
};
pub use line::{PlaceLine, flip_plans, iterate_line, rotate_plans};
pub use place_mode::{MobileMode, PlaceMode};
pub use placement::{
    ASTAR_NODE_LIMIT, BridgePlacer, DirectionBridgePlacer, MAX_LENGTH, NormalizeDrawResult,
    NormalizeResult, PlacementWorld, RelativeBridgePlacer, astar, calculate_bridges,
    calculate_nodes, is_side_place, normalize_area, normalize_draw_area, normalize_line,
    normalize_rectangle, pathfind_line, upgrade_line,
};
pub use plan::{ClientPlan, PlanCopy, PlanMirror, PlanTree, PreviewState};
pub use queue::{AddOutcome, BuildQueue};
pub use replay::ReplayHarness;
pub use rts::{
    SelectRect, SelectableUnit, enemy_unit_at, select_buildings_rect, select_typed_units,
    select_unit_tap, select_units_rect,
};

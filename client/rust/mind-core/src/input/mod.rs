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
pub mod camera_state;
pub mod caps;
pub mod client_input;
pub mod command_emit;
pub mod cursor;
pub mod desktop;
pub mod focus;
pub mod input_log;
pub mod line;
pub mod mobile;
pub mod place_mode;
pub mod placement;
pub mod plan;
pub mod queue;
pub mod replay;
pub mod rts;
pub mod sync;

pub use action::{CommandTarget, InventoryKind, PayloadAction, RemoteAction};
pub use binding::{
    BINDS, BindingDefault, BindingState, BindingValue, Category, KeyBind, KeyBindId, KeyBindTable,
    KeyKind, ids,
};
pub use camera_state::{
    CAMERA_LERP, CAMERA_SNAP, CameraState, CameraView, MAX_ZOOM, MAX_ZOOM_IN_GAME, MIN_ZOOM,
    MIN_ZOOM_IN_GAME, MOBILE_CAM_SPEED, MinimapRegion, PAN_SCALE, PAN_SPEED, SMOOTH_CAMERA_LERP,
    ZOOM_STEP_DIVISOR,
};
pub use caps::{InputCaps, TestCaps};
pub use client_input::{CONTROL_GROUP_DOUBLE_TAP_MS, CONTROL_GROUPS, InputState};
pub use command_emit::{
    ActionBatch, ActionBatcher, ActionError, COMMAND_CHUNK, DELETE_POSITION_CAP,
};
pub use cursor::{CursorContext, CursorKind, resolve_cursor};
pub use desktop::{DesktopController, ScrollOutcome};
pub use focus::{FocusGuards, FocusState, InputLocks, LockId};
pub use input_log::{
    INPUT_LOG_FORMAT, InputHeader, InputLog, InputLogError, InputRecord, InputReplay, RawEvent,
};
pub use line::{PlaceLine, flip_plans, iterate_line, rotate_plans};
pub use mobile::{
    AREA_BREAK_START_TILES, EDGE_PAN, GestureDetector, GestureEvent, LINE_MODE_START_TILES,
    LONG_PRESS_SECONDS, MAX_FLING_DELAY_SECONDS, MAX_PAN_SPEED, MINE_DOUBLE_TAP_MS,
    MobileController, PayloadTarget, TAP_COUNT_INTERVAL_SECONDS, TAP_SQUARE_HALF,
    should_begin_mine,
};
pub use place_mode::{MobileMode, PlaceMode};
pub use placement::{
    ASTAR_NODE_LIMIT, BridgePlacer, DirectionBridgePlacer, MAX_LENGTH, NormalizeDrawResult,
    NormalizeResult, PlacementWorld, RelativeBridgePlacer, astar, calculate_bridges,
    calculate_nodes, is_side_place, normalize_area, normalize_draw_area, normalize_line,
    normalize_rectangle, pathfind_line, upgrade_line,
};
pub use plan::{ClientPlan, PlanCopy, PlanMirror, PlanTree, PreviewState};
pub use queue::{AddOutcome, BuildQueue};
pub use replay::{MobileReplayHarness, ReplayHarness};
pub use rts::{
    SelectRect, SelectableBuilding, SelectableUnit, command_units_apply, enemy_unit_at,
    select_buildings_rect, select_typed_units, select_unit_tap, select_units_rect,
};
pub use sync::{
    MAX_PLAYER_PREVIEW_PLANS, PLAN_SNAPSHOT_CHUNK, PLAN_SNAPSHOT_INTERVAL_TICKS, PlanSnapshot,
    PlanSnapshotTimer, PlanWire, PlayerInputSync, WireConfig, build_plan_snapshots,
    get_synced_plans,
};

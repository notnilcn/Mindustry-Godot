// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MobileInput` + Arc `GestureDetector` port (plan 15 §3.1/§3.5/§3.9).
//!
//! Platform-neutral: the Godot touch translation and 14's on-screen buttons
//! live in `mind-gdext::input::mobile`; this module owns the gesture state
//! machine, the mobile decision tree and the confirm/commit path. Everything is
//! deterministic and testable headlessly (deviation I1).

use smallvec::SmallVec;

use crate::config::TILESIZE;
use crate::content::BlockId;
use crate::world::TilePos;

use super::action::RemoteAction;
use super::caps::InputCaps;
use super::client_input::InputState;
use super::line::{LineBlock, LineParams};
use super::place_mode::{MobileMode, PlaceMode};
use super::placement::PlacementWorld;
use super::plan::ClientPlan;
use super::queue::BuildQueue;
use super::rts::SelectableUnit;

// Arc `GestureDetector` thresholds. Mindustry constructs the detector at
// `InputHandler.add()` with `new GestureDetector(20, 0.5f, 0.3f, 0.15f, this)`;
// the Arc signature is `(halfTapSquareSize, tapCountInterval, longPressDuration,
// maxFlingDelay)`, so `tapCountInterval = 0.5 s` and `longPressDuration = 0.3 s`.
// (The plan §6.6 shorthand had these two swapped; corrected to upstream here.)

/// Half-width/height of the tap square (`halfTapSquareSize`).
pub const TAP_SQUARE_HALF: f32 = 20.0;
/// Consecutive-tap window (`tapCountInterval`).
pub const TAP_COUNT_INTERVAL_SECONDS: f64 = 0.5;
/// Hold duration for a long press (`longPressDuration`).
pub const LONG_PRESS_SECONDS: f64 = 0.3;
/// Maximum delay after the last drag for a fling (`maxFlingDelay`).
pub const MAX_FLING_DELAY_SECONDS: f64 = 0.15;

/// Screen-edge distance that starts automatic panning (`MobileInput.edgePan`).
pub const EDGE_PAN: f32 = 60.0;
/// Maximum auto-pan speed (`MobileInput.maxPanSpeed`).
pub const MAX_PAN_SPEED: f32 = 1.3;
/// Drag distance (tiles) that promotes a placement drag to line mode.
pub const LINE_MODE_START_TILES: f32 = 3.0;
/// Drag distance (tiles) that promotes a break drag to area mode.
pub const AREA_BREAK_START_TILES: f32 = 2.0;
/// Double-tap window for double-tap mining (`500 ms`).
pub const MINE_DOUBLE_TAP_MS: u64 = 500;
/// Keyboard-less mobile camera move speed (`MobileInput.update`).
pub const CAM_SPEED: f32 = 6.0;
/// Closest enemy radius for target acquisition (`checkTargets`).
pub const TARGET_RANGE: f32 = 20.0;

/// One gesture emitted by [`GestureDetector`] (Arc `GestureListener` calls).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GestureEvent {
    /// A pointer went down.
    TouchDown {
        /// X.
        x: f32,
        /// Y.
        y: f32,
        /// Pointer index.
        pointer: i32,
    },
    /// A tap occurred (`count` is the consecutive-tap count).
    Tap {
        /// X.
        x: f32,
        /// Y.
        y: f32,
        /// Tap count.
        count: i32,
    },
    /// A long press fired.
    LongPress {
        /// X.
        x: f32,
        /// Y.
        y: f32,
    },
    /// A drag left the tap square.
    Pan {
        /// X.
        x: f32,
        /// Y.
        y: f32,
        /// Delta x since the last update.
        delta_x: f32,
        /// Delta y since the last update.
        delta_y: f32,
    },
    /// Panning ended.
    PanStop,
    /// A fling was released.
    Fling {
        /// Velocity x (px/s).
        velocity_x: f32,
        /// Velocity y (px/s).
        velocity_y: f32,
    },
    /// A pinch zoom progressed.
    Zoom {
        /// Distance between fingers when the gesture started.
        initial_distance: f32,
        /// Current distance between fingers.
        distance: f32,
    },
    /// Pinching ended.
    PinchStop,
}

impl GestureEvent {
    /// Stable parity name (gesture JSON / replay logs).
    pub const fn name(&self) -> &'static str {
        match self {
            GestureEvent::TouchDown { .. } => "touch_down",
            GestureEvent::Tap { .. } => "tap",
            GestureEvent::LongPress { .. } => "long_press",
            GestureEvent::Pan { .. } => "pan",
            GestureEvent::PanStop => "pan_stop",
            GestureEvent::Fling { .. } => "fling",
            GestureEvent::Zoom { .. } => "zoom",
            GestureEvent::PinchStop => "pinch_stop",
        }
    }
}

/// Arc `GestureDetector.VelocityTracker` (10-sample mean velocity).
#[derive(Debug, Clone)]
struct VelocityTracker {
    last_x: f32,
    last_y: f32,
    delta_x: f32,
    delta_y: f32,
    last_time: f64,
    sample: usize,
    mean_x: [f32; 10],
    mean_y: [f32; 10],
    mean_time: [f64; 10],
}

impl Default for VelocityTracker {
    fn default() -> Self {
        Self {
            last_x: 0.0,
            last_y: 0.0,
            delta_x: 0.0,
            delta_y: 0.0,
            last_time: 0.0,
            sample: 0,
            mean_x: [0.0; 10],
            mean_y: [0.0; 10],
            mean_time: [0.0; 10],
        }
    }
}

impl VelocityTracker {
    fn start(&mut self, x: f32, y: f32, time: f64) {
        self.last_x = x;
        self.last_y = y;
        self.delta_x = 0.0;
        self.delta_y = 0.0;
        self.sample = 0;
        self.mean_x = [0.0; 10];
        self.mean_y = [0.0; 10];
        self.mean_time = [0.0; 10];
        self.last_time = time;
    }

    fn update(&mut self, x: f32, y: f32, time: f64) {
        self.delta_x = x - self.last_x;
        self.delta_y = y - self.last_y;
        self.last_x = x;
        self.last_y = y;
        let delta_time = time - self.last_time;
        self.last_time = time;
        let index = self.sample % 10;
        self.mean_x[index] = self.delta_x;
        self.mean_y[index] = self.delta_y;
        self.mean_time[index] = delta_time;
        self.sample += 1;
    }

    fn mean(values: &[f64]) -> f64 {
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    }

    fn velocity_x(&self) -> f32 {
        let count = self.sample.min(10);
        if count == 0 {
            return 0.0;
        }
        let mean_x = self.mean_x[..count].iter().sum::<f32>() / count as f32;
        let mean_time = Self::mean(&self.mean_time[..count]);
        if mean_time == 0.0 {
            0.0
        } else {
            (mean_x as f64 / mean_time) as f32
        }
    }

    fn velocity_y(&self) -> f32 {
        let count = self.sample.min(10);
        if count == 0 {
            return 0.0;
        }
        let mean_y = self.mean_y[..count].iter().sum::<f32>() / count as f32;
        let mean_time = Self::mean(&self.mean_time[..count]);
        if mean_time == 0.0 {
            0.0
        } else {
            (mean_y as f64 / mean_time) as f32
        }
    }
}

/// Pure port of `arc.input.GestureDetector` (time in seconds).
#[derive(Debug, Clone)]
pub struct GestureDetector {
    tap_rect_w: f32,
    tap_rect_h: f32,
    tap_count_interval: f64,
    long_press_seconds: f64,
    max_fling_delay: f64,
    tracker: VelocityTracker,
    pointer1: (f32, f32),
    pointer2: (f32, f32),
    initial_pointer1: (f32, f32),
    initial_pointer2: (f32, f32),
    long_press_fired: bool,
    in_tap_rect: bool,
    panning: bool,
    pinching: bool,
    tap_count: i32,
    last_tap_time: f64,
    last_tap_x: f32,
    last_tap_y: f32,
    last_tap_pointer: i32,
    tap_center: (f32, f32),
    gesture_start_time: f64,
    touch_active: bool,
}

impl Default for GestureDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureDetector {
    /// Creates a detector with the `InputHandler.add()` thresholds.
    pub fn new() -> Self {
        Self {
            tap_rect_w: TAP_SQUARE_HALF,
            tap_rect_h: TAP_SQUARE_HALF,
            tap_count_interval: TAP_COUNT_INTERVAL_SECONDS,
            long_press_seconds: LONG_PRESS_SECONDS,
            max_fling_delay: MAX_FLING_DELAY_SECONDS,
            tracker: VelocityTracker::default(),
            pointer1: (0.0, 0.0),
            pointer2: (0.0, 0.0),
            initial_pointer1: (0.0, 0.0),
            initial_pointer2: (0.0, 0.0),
            long_press_fired: false,
            in_tap_rect: false,
            panning: false,
            pinching: false,
            tap_count: 0,
            last_tap_time: f64::NEG_INFINITY,
            last_tap_x: 0.0,
            last_tap_y: 0.0,
            last_tap_pointer: -1,
            tap_center: (0.0, 0.0),
            gesture_start_time: 0.0,
            touch_active: false,
        }
    }

    /// Whether a drag is currently panning.
    pub fn is_panning(&self) -> bool {
        self.panning
    }

    /// Whether the active touch has been held for a long press.
    pub fn is_long_pressed(&self, time: f64) -> bool {
        self.touch_active && time - self.gesture_start_time > self.long_press_seconds
    }

    /// Timer-driven long press (Arc `longPressTask`).
    pub fn update(&mut self, time: f64) -> Option<GestureEvent> {
        if self.in_tap_rect && !self.long_press_fired && !self.panning && self.is_long_pressed(time)
        {
            self.long_press_fired = true;
            return Some(GestureEvent::LongPress {
                x: self.pointer1.0,
                y: self.pointer1.1,
            });
        }
        None
    }

    /// `touchDown`.
    pub fn touch_down(
        &mut self,
        time: f64,
        x: f32,
        y: f32,
        pointer: i32,
    ) -> SmallVec<[GestureEvent; 2]> {
        let mut out = SmallVec::new();
        if pointer > 1 {
            return out;
        }
        if pointer == 0 {
            self.pointer1 = (x, y);
            self.gesture_start_time = time;
            self.touch_active = true;
            self.tracker.start(x, y, time);
            if self.pinching {
                // A second finger was already down: start pinch.
                self.in_tap_rect = false;
                self.initial_pointer1 = self.pointer1;
                self.initial_pointer2 = self.pointer2;
            } else {
                self.in_tap_rect = true;
                self.pinching = false;
                self.long_press_fired = false;
                self.tap_center = (x, y);
            }
        } else {
            self.pointer2 = (x, y);
            self.in_tap_rect = false;
            self.pinching = true;
            self.initial_pointer1 = self.pointer1;
            self.initial_pointer2 = self.pointer2;
        }
        out.push(GestureEvent::TouchDown { x, y, pointer });
        out
    }

    /// `touchDragged`.
    pub fn touch_dragged(
        &mut self,
        time: f64,
        x: f32,
        y: f32,
        pointer: i32,
    ) -> SmallVec<[GestureEvent; 2]> {
        let mut out = SmallVec::new();
        if pointer > 1 || self.long_press_fired {
            return out;
        }
        if pointer == 0 {
            self.pointer1 = (x, y);
        } else {
            self.pointer2 = (x, y);
        }

        if self.pinching {
            let initial = distance(self.initial_pointer1, self.initial_pointer2);
            let current = distance(self.pointer1, self.pointer2);
            out.push(GestureEvent::Zoom {
                initial_distance: initial,
                distance: current,
            });
            return out;
        }

        self.tracker.update(x, y, time);
        if self.in_tap_rect && !self.within_tap_rect(x, y, self.tap_center.0, self.tap_center.1) {
            self.in_tap_rect = false;
        }
        if !self.in_tap_rect {
            self.panning = true;
            out.push(GestureEvent::Pan {
                x,
                y,
                delta_x: self.tracker.delta_x,
                delta_y: self.tracker.delta_y,
            });
        }
        out
    }

    /// `touchUp`.
    pub fn touch_up(
        &mut self,
        time: f64,
        x: f32,
        y: f32,
        pointer: i32,
    ) -> SmallVec<[GestureEvent; 2]> {
        let mut out = SmallVec::new();
        if pointer > 1 {
            return out;
        }
        if pointer == 0 {
            self.touch_active = false;
        }
        if self.in_tap_rect && !self.within_tap_rect(x, y, self.tap_center.0, self.tap_center.1) {
            self.in_tap_rect = false;
        }
        let was_panning = self.panning;
        self.panning = false;
        if self.long_press_fired {
            return out;
        }

        if self.in_tap_rect {
            if self.last_tap_pointer != pointer
                || time - self.last_tap_time > self.tap_count_interval
                || !self.within_tap_rect(x, y, self.last_tap_x, self.last_tap_y)
            {
                self.tap_count = 0;
            }
            self.tap_count += 1;
            self.last_tap_time = time;
            self.last_tap_x = x;
            self.last_tap_y = y;
            self.last_tap_pointer = pointer;
            out.push(GestureEvent::Tap {
                x,
                y,
                count: self.tap_count,
            });
            return out;
        }

        if self.pinching {
            self.pinching = false;
            out.push(GestureEvent::PinchStop);
            self.panning = true;
            let start = if pointer == 0 {
                self.pointer2
            } else {
                self.pointer1
            };
            self.tracker.start(start.0, start.1, time);
            return out;
        }

        if was_panning {
            out.push(GestureEvent::PanStop);
        }
        if time - self.tracker.last_time < self.max_fling_delay {
            self.tracker.update(x, y, time);
            out.push(GestureEvent::Fling {
                velocity_x: self.tracker.velocity_x(),
                velocity_y: self.tracker.velocity_y(),
            });
        }
        out
    }

    /// No further gestures fire for the current touch.
    pub fn cancel(&mut self) {
        self.long_press_fired = true;
    }

    /// Resets transient state.
    pub fn reset(&mut self) {
        self.gesture_start_time = 0.0;
        self.panning = false;
        self.in_tap_rect = false;
        self.touch_active = false;
        self.tracker.last_time = 0.0;
    }

    fn within_tap_rect(&self, x: f32, y: f32, center_x: f32, center_y: f32) -> bool {
        (x - center_x).abs() < self.tap_rect_w && (y - center_y).abs() < self.tap_rect_h
    }
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Long-press payload resolution (`MobileInput.longPress`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PayloadTarget {
    /// No target.
    None,
    /// A friendly unit to pick up.
    Unit(i32),
    /// A friendly building to pick up/drop into.
    Building(TilePos),
    /// A ground position to drop the current payload.
    Position(f32, f32),
}

/// Whether a tap should begin mining (`DesktopInput`/`MobileInput` double-tap
/// gate). `double_tap_mine` mirrors the `doubletapmine` setting.
pub fn should_begin_mine(
    selected: TilePos,
    prev_selected: Option<TilePos>,
    time_since_ms: u64,
    double_tap_mine: bool,
) -> bool {
    if !double_tap_mine {
        true
    } else {
        prev_selected == Some(selected) && time_since_ms < MINE_DOUBLE_TAP_MS
    }
}

/// `MobileInput` controller (platform-neutral decision tree).
#[derive(Debug, Clone)]
pub struct MobileController {
    /// Client input state.
    pub state: InputState,
    /// Build queue seam (plan 11).
    pub queue: BuildQueue,
    /// Mobile-only mode flags.
    pub mode: MobileMode,
    /// Gesture detector.
    pub detector: GestureDetector,
    /// `lineStartX/Y` (line-mode anchor).
    pub line_start: Option<TilePos>,
    /// `lastLineX/Y`.
    pub last_line: (i32, i32),
    /// `lineScale` (preview animation).
    pub line_scale: f32,
    /// Accumulated plan-shift delta (`shiftDeltaX/Y`).
    pub shift_delta: (f32, f32),
    /// Whether a plan is selected and being shifted.
    pub selecting: bool,
    /// `down` (a pointer is down).
    pub down: bool,
    /// Whether manual shooting (point with finger) is enabled.
    pub manual_shooting: bool,
    /// Payload target being moved to.
    pub payload_target: PayloadTarget,
    /// Current thing being shot at (unit id).
    pub target: Option<i32>,
    /// Unit last tapped.
    pub unit_tapped: Option<i32>,
    /// Control building last tapped.
    pub building_tapped: Option<TilePos>,
    /// Auto-pan vector handed to the camera.
    pub vector: (f32, f32),
    /// Last renderer scale seen by `zoom` (`lastZoom`).
    pub last_zoom: f32,
    /// Emitted actions (relay/21).
    pub emitted: SmallVec<[RemoteAction; 4]>,
    /// Monotonic render clock (ms).
    pub clock_ms: u64,
    /// `settings.getBool("keyboard")`: mobile keyboard mode.
    pub keyboard: bool,
    /// `lastBlock` (mode-transition tracking, `MobileInput.update`).
    pub last_block: Option<BlockId>,
    /// `lastPlaced` (last selection plan added).
    pub last_placed: Option<ClientPlan>,
    /// Deconstruction plans fading out (`MobileInput.removals`).
    pub removals: Vec<ClientPlan>,
    /// Last building resolved by autotarget (`MobileInput.target`).
    pub target_building: Option<TilePos>,
}

impl Default for MobileController {
    fn default() -> Self {
        Self::new()
    }
}

impl MobileController {
    /// Creates a mobile controller.
    pub fn new() -> Self {
        Self {
            state: InputState::new(),
            queue: BuildQueue::new(),
            mode: MobileMode::default(),
            detector: GestureDetector::new(),
            line_start: None,
            last_line: (0, 0),
            line_scale: 0.0,
            shift_delta: (0.0, 0.0),
            selecting: false,
            down: false,
            manual_shooting: false,
            payload_target: PayloadTarget::None,
            target: None,
            unit_tapped: None,
            building_tapped: None,
            vector: (0.0, 0.0),
            last_zoom: -1.0,
            emitted: SmallVec::new(),
            clock_ms: 0,
            keyboard: false,
            last_block: None,
            last_placed: None,
            removals: Vec::new(),
            target_building: None,
        }
    }

    /// Starts line mode from a tile (`MobileInput.longPress` placing/breaking).
    pub fn begin_line(&mut self, tile: TilePos) {
        self.line_start = Some(tile);
        self.last_line = (tile.x() as i32, tile.y() as i32);
        self.mode.line_mode = true;
        self.update_mode_from_place_mode();
    }

    /// `isLinePlacing`.
    pub fn is_line_placing(&self, world_x: f32, world_y: f32) -> bool {
        let Some(start) = self.line_start else {
            return false;
        };
        let d = distance(
            (
                start.x() as f32 * TILESIZE as f32,
                start.y() as f32 * TILESIZE as f32,
            ),
            (world_x, world_y),
        );
        self.mode.line_mode
            && self.state.place_mode.is_placing()
            && d >= LINE_MODE_START_TILES * TILESIZE as f32
    }

    /// `isAreaBreaking`.
    pub fn is_area_breaking(&self, world_x: f32, world_y: f32) -> bool {
        let Some(start) = self.line_start else {
            return false;
        };
        let d = distance(
            (
                start.x() as f32 * TILESIZE as f32,
                start.y() as f32 * TILESIZE as f32,
            ),
            (world_x, world_y),
        );
        self.mode.line_mode
            && self.state.place_mode.is_breaking()
            && d >= AREA_BREAK_START_TILES * TILESIZE as f32
    }

    /// `updateLine` for the active drag.
    pub fn drag_to(&mut self, world: &dyn PlacementWorld, block: Option<&LineBlock>, end: TilePos) {
        let Some(start) = self.line_start else {
            return;
        };
        self.last_line = (end.x() as i32, end.y() as i32);
        let params = LineParams {
            mobile: true,
            swap_diagonal: false,
            rotation: self.state.rotation,
            override_line_rotation: self.state.override_line_rotation,
            ..LineParams::default()
        };
        self.state.update_line(world, block, start, end, &params);
        self.update_mode_from_place_mode();
    }

    fn update_mode_from_place_mode(&mut self) {
        self.mode.line_mode =
            self.state.place_mode.is_placing() || self.state.place_mode.is_breaking();
        self.mode.schematic_mode = self.state.place_mode.is_schematic_selecting();
        self.mode.rebuild_mode = self.state.place_mode.is_rebuild_selecting();
    }

    /// Confirm-button commit: valid selection plans enter the build queue,
    /// breaking plans are dropped (upstream `buildPlacementUI` confirm).
    pub fn confirm_plans(&mut self, world: &dyn PlacementWorld) -> usize {
        let plans = std::mem::take(&mut self.state.select_plans);
        let mut committed = 0usize;
        for plan in plans {
            if plan.breaking {
                continue;
            }
            if world.valid_place(plan.block, plan.x, plan.y, plan.rotation) {
                self.queue.add_build(plan, false);
                committed += 1;
            }
        }
        self.mode.confirm_pending = false;
        self.selecting = false;
        committed
    }

    /// `flushSelectPlans(linePlans)` on touch-up: commits the active drag line
    /// to the build queue (upstream `MobileInput.touchUp` placing branch).
    pub fn confirm_line(&mut self, world: &dyn PlacementWorld) -> usize {
        let plans = std::mem::take(&mut self.state.line_plans);
        let mut committed = 0usize;
        for plan in plans {
            if plan.breaking || plan.block == BlockId::AIR {
                continue;
            }
            if world.valid_place(plan.block, plan.x, plan.y, plan.rotation) {
                self.queue.add_build(plan, false);
                committed += 1;
            }
        }
        self.state.line.clear();
        self.state.place_mode = PlaceMode::None;
        self.mode.line_mode = false;
        self.line_start = None;
        committed
    }

    /// Queues a placement plan on a tap (`selectPlans.add`).
    pub fn add_select_plan(&mut self, plan: ClientPlan) {
        self.last_placed = Some(plan.clone());
        self.state.select_plans.push(plan);
        self.mode.confirm_pending = true;
    }

    /// `isRebuildSelecting`.
    pub fn is_rebuild_selecting(&self) -> bool {
        self.mode.rebuild_mode
    }

    /// `hasSchematic` (a schematic is loaded for placement).
    pub fn has_schematic(&self) -> bool {
        self.state.has_schematic
    }

    /// `MobileInput.schemOriginX/Y`: the centroid of `selectPlans` in tiles.
    ///
    /// Deviation: uses plan centers (no `drawx`/`drawy` block offset), which is
    /// exact for the 1x1 fixture set and within a tile otherwise.
    pub fn schem_origin(&self) -> (i32, i32) {
        if self.state.select_plans.is_empty() {
            return (0, 0);
        }
        let sum_x: i64 = self.state.select_plans.iter().map(|p| p.x as i64).sum();
        let sum_y: i64 = self.state.select_plans.iter().map(|p| p.y as i64).sum();
        let count = self.state.select_plans.len() as i64;
        ((sum_x / count) as i32, (sum_y / count) as i32)
    }

    /// `useSchematic`: replace `selectPlans` with a schematic's plans.
    pub fn use_schematic(&mut self, plans: Vec<ClientPlan>) {
        self.state.select_plans = plans;
        self.state.has_schematic = true;
        self.mode.schematic_mode = true;
        self.mode.confirm_pending = !self.state.select_plans.is_empty();
        self.state.place_mode = PlaceMode::SchematicSelect;
    }

    /// `MobileInput.touchUp` schematic branch: place the created schematic and
    /// leave select mode (plans stay queued for the confirm button).
    pub fn confirm_schematic(&mut self, plans: Vec<ClientPlan>) -> usize {
        self.use_schematic(plans);
        self.mode.schematic_mode = false;
        self.state.place_mode = PlaceMode::None;
        self.state.select_plans.len()
    }

    /// `getPlan`: the selection plan at `tile`.
    ///
    /// Deviation: exact-tile match; upstream compares block footprints, which
    /// needs plan 07's `BlockView.size` (not yet threaded into `ClientPlan`).
    pub fn get_plan(&self, tile: TilePos) -> Option<&ClientPlan> {
        self.state
            .select_plans
            .iter()
            .find(|plan| plan.x == tile.x() as i32 && plan.y == tile.y() as i32)
    }

    /// `hasPlan`.
    pub fn has_plan(&self, tile: TilePos) -> bool {
        self.get_plan(tile).is_some()
    }

    /// `removePlan`: drop the plan and queue a deconstruction fade.
    pub fn remove_plan(&mut self, plan: &ClientPlan) -> bool {
        let before = self.state.select_plans.len();
        self.state
            .select_plans
            .retain(|p| p.x != plan.x || p.y != plan.y);
        if self.state.select_plans.len() == before {
            return false;
        }
        if !plan.breaking {
            self.removals.push(plan.clone());
        }
        true
    }

    /// `checkOverlapPlacement`: would a `block_size` footprint at `(x, y)`
    /// overlap an existing selection plan? Breaking plans block their own tile.
    pub fn check_overlap_placement(&self, x: i32, y: i32, block_size: i32) -> bool {
        let half = (block_size - 1) / 2;
        for plan in &self.state.select_plans {
            let dx = (plan.x - x).abs();
            let dy = (plan.y - y).abs();
            if dx <= half && dy <= half {
                return true;
            }
        }
        false
    }

    /// `checkTargets`: acquire the closest enemy within [`TARGET_RANGE`], else a
    /// world building. Sets [`Self::target`]/[`Self::target_building`].
    pub fn check_targets(
        &mut self,
        world: &dyn PlacementWorld,
        units: &[SelectableUnit],
        player_team: u8,
        x: f32,
        y: f32,
    ) -> Option<i32> {
        let mut best: Option<(f32, i32)> = None;
        for unit in units {
            if unit.team == player_team {
                continue;
            }
            let dist = (unit.x - x).hypot(unit.y - y);
            if dist <= TARGET_RANGE && best.is_none_or(|(best_dist, _)| dist < best_dist) {
                best = Some((dist, unit.id));
            }
        }
        if let Some((_, id)) = best {
            self.target = Some(id);
            self.target_building = None;
            return Some(id);
        }
        let tile = TilePos::new(
            (x / TILESIZE as f32).floor() as i16,
            (y / TILESIZE as f32).floor() as i16,
        );
        if world.block_at(tile.x() as i32, tile.y() as i32) != BlockId::AIR {
            self.target_building = Some(tile);
        }
        None
    }

    /// `rebuildArea`: queue replacement plans for derelicts in the rect. The
    /// caller supplies the plan-07 `getReplacement` result per tile.
    pub fn rebuild_area(
        &mut self,
        world: &dyn PlacementWorld,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        replacement: impl Fn(TilePos, BlockId) -> Option<ClientPlan>,
    ) -> usize {
        let (min_x, max_x) = (x1.min(x2), x1.max(x2));
        let (min_y, max_y) = (y1.min(y2), y1.max(y2));
        let mut queued = 0usize;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let block = world.block_at(x, y);
                if block == BlockId::AIR {
                    continue;
                }
                if let Some(plan) = replacement(TilePos::new(x as i16, y as i16), block) {
                    self.add_select_plan(plan);
                    queued += 1;
                }
            }
        }
        self.mode.rebuild_mode = false;
        self.state.place_mode = PlaceMode::None;
        queued
    }

    /// `update()` state transitions (command/placing/line/schematic/rebuild).
    pub fn update_transitions(&mut self) {
        if !self.state.command_mode {
            self.mode.queue_command_mode = false;
        } else {
            self.state.place_mode = PlaceMode::None;
            self.mode.schematic_mode = false;
        }

        if self.state.block.is_some() {
            self.mode.rebuild_mode = false;
            self.mode.schematic_mode = false;
            self.state.command_mode = false;
        }

        if !self.state.command_mode {
            self.state.command_buildings.clear();
            self.state.selected_units.clear();
        }

        if self.state.place_mode == PlaceMode::None {
            self.mode.line_mode = false;
        }
        if self.mode.line_mode && self.state.place_mode.is_placing() && self.state.block.is_none() {
            self.mode.line_mode = false;
        }

        if self.state.block.is_some() && self.state.place_mode == PlaceMode::None {
            self.state.place_mode = PlaceMode::Placing;
        }
        if self.state.block.is_none() && self.state.place_mode.is_placing() {
            self.state.place_mode = PlaceMode::None;
        }

        if !self.mode.schematic_mode
            && (self.state.place_mode.is_schematic_selecting()
                || self.state.place_mode.is_rebuild_selecting())
        {
            self.state.place_mode = PlaceMode::None;
        }
        if !self.mode.rebuild_mode && self.state.place_mode.is_rebuild_selecting() {
            self.state.place_mode = PlaceMode::None;
        }

        if self.state.block != self.last_block
            && self.state.place_mode.is_breaking()
            && self.state.block.is_some()
        {
            self.state.place_mode = PlaceMode::Placing;
            self.last_block = self.state.block;
        }
        if self.state.block.is_none() {
            self.last_block = None;
        }
    }

    /// `MobileInput.update` keyboard-less camera move (returns the world delta).
    pub fn camera_move(&mut self, axis_x: f32, axis_y: f32, delta: f32) -> (f32, f32) {
        if self.keyboard {
            return (0.0, 0.0);
        }
        let length = (axis_x * axis_x + axis_y * axis_y).sqrt();
        if length <= 0.0 {
            return (0.0, 0.0);
        }
        let speed = delta * CAM_SPEED / length;
        (axis_x * speed, axis_y * speed)
    }

    /// `settings.getBool("keyboard")` branch.
    pub fn keyboard(&self) -> bool {
        self.keyboard
    }

    /// Sets the mobile keyboard mode (`settings` push).
    pub fn set_keyboard(&mut self, value: bool) {
        self.keyboard = value;
    }

    /// Pan handling (upstream `MobileInput.pan`). Shifts `select_plans` while a
    /// plan is selected; otherwise returns the camera delta the caller applies.
    pub fn pan(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        camera_width: f32,
        viewport_width: f32,
    ) -> (f32, f32) {
        // `MobileInput.pan` guards: keyboard mode, dialog, manual shooting and
        // schematic placement block touch panning.
        if self.keyboard || self.manual_shooting || !self.down || self.mode.schematic_mode {
            return (0.0, 0.0);
        }
        let scale = if viewport_width > 0.0 {
            camera_width / viewport_width
        } else {
            1.0
        };
        let dx = delta_x * scale;
        let dy = delta_y * scale;
        if self.selecting {
            self.shift_delta.0 += dx;
            self.shift_delta.1 += dy;
            let shifted_x = (self.shift_delta.0 / TILESIZE as f32) as i32;
            let shifted_y = (self.shift_delta.1 / TILESIZE as f32) as i32;
            if shifted_x != 0 || shifted_y != 0 {
                for plan in &mut self.state.select_plans {
                    if plan.breaking {
                        continue;
                    }
                    plan.x += shifted_x;
                    plan.y += shifted_y;
                }
                self.shift_delta.0 %= TILESIZE as f32;
                self.shift_delta.1 %= TILESIZE as f32;
            }
            (0.0, 0.0)
        } else {
            (-dx, -dy)
        }
    }

    /// `panStop`.
    pub fn pan_stop(&mut self) {
        self.shift_delta = (0.0, 0.0);
    }

    /// Pinch zoom (`MobileInput.zoom`): returns the new target scale.
    ///
    /// Keyboard mode disables touch zoom (`MobileInput.zoom` early return).
    pub fn zoom(&mut self, initial_distance: f32, distance: f32, base_scale: f32) -> f32 {
        if self.keyboard {
            return base_scale;
        }
        if self.last_zoom < 0.0 {
            self.last_zoom = base_scale;
        }
        let scale = if initial_distance > 0.0 {
            distance / initial_distance * self.last_zoom
        } else {
            self.last_zoom
        };
        self.last_zoom = scale;
        scale
    }

    /// `autoPan`: screen-edge camera pan, clamped to `maxPanSpeed`.
    pub fn auto_pan(
        &mut self,
        screen_x: f32,
        screen_y: f32,
        viewport_w: f32,
        viewport_h: f32,
        camera_width: f32,
    ) -> (f32, f32) {
        let mut pan_x = 0.0f32;
        let mut pan_y = 0.0f32;
        if screen_x <= EDGE_PAN {
            pan_x = -(EDGE_PAN - screen_x);
        }
        if screen_x >= viewport_w - EDGE_PAN {
            pan_x = (screen_x - viewport_w) + EDGE_PAN;
        }
        if screen_y <= EDGE_PAN {
            pan_y = -(EDGE_PAN - screen_y);
        }
        if screen_y >= viewport_h - EDGE_PAN {
            pan_y = (screen_y - viewport_h) + EDGE_PAN;
        }
        let scale = if viewport_w > 0.0 {
            camera_width / viewport_w
        } else {
            1.0
        };
        let mut vx = pan_x * scale;
        let mut vy = pan_y * scale;
        let len = (vx * vx + vy * vy).sqrt();
        if len > MAX_PAN_SPEED {
            vx = vx / len * MAX_PAN_SPEED;
            vy = vy / len * MAX_PAN_SPEED;
        }
        self.vector = (vx, vy);
        self.vector
    }

    /// `longPress` payload targeting. `unit_candidates` are friendly, AI,
    /// grounded, pickable units; `building_candidates` are friendly pickable
    /// buildings; `has_payload` mirrors `pay.hasPayload()`.
    pub fn resolve_payload_target(
        &mut self,
        pos: (f32, f32),
        unit_candidates: &[(i32, f32, f32)],
        building_candidates: &[TilePos],
        has_payload: bool,
    ) -> PayloadTarget {
        // Closest friendly unit within 8 px (`Units.closest(..., 8f)`).
        let mut best: Option<(f32, i32)> = None;
        for (id, x, y) in unit_candidates {
            let d = distance((*x, *y), pos);
            if d <= 8.0 && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, *id));
            }
        }
        if let Some((_, id)) = best {
            self.payload_target = PayloadTarget::Unit(id);
            return self.payload_target;
        }
        if let Some(build) = building_candidates.first().copied() {
            self.payload_target = PayloadTarget::Building(build);
            return self.payload_target;
        }
        if has_payload {
            self.payload_target = PayloadTarget::Position(pos.0, pos.1);
        } else {
            self.payload_target = PayloadTarget::None;
            self.manual_shooting = true;
            self.target = None;
        }
        self.payload_target
    }

    /// Dispatches one gesture event into the controller (upstream callbacks).
    pub fn handle_gesture(
        &mut self,
        event: GestureEvent,
        world: &dyn PlacementWorld,
        block: Option<&LineBlock>,
        caps: &dyn InputCaps,
    ) -> SmallVec<[RemoteAction; 4]> {
        self.emitted.clear();
        match event {
            GestureEvent::TouchDown { x, y, pointer } => {
                if caps.is_menu() || caps.focus().is_menu || caps.is_cutscene() {
                    return SmallVec::new();
                }
                self.keyboard = caps.mobile_keyboard();
                self.down = true;
                if pointer == 0 {
                    let tile = TilePos::new(
                        (x / TILESIZE as f32).floor() as i16,
                        (y / TILESIZE as f32).floor() as i16,
                    );
                    // Selecting begins only on an existing plan (`hasPlan`).
                    self.selecting = self.has_plan(tile) && !self.command_mode_enabled();
                    if self.mode.schematic_mode && self.state.block.is_none() {
                        self.state.place_mode = if self.mode.rebuild_mode {
                            PlaceMode::RebuildSelect
                        } else {
                            PlaceMode::SchematicSelect
                        };
                        self.begin_line(tile);
                    } else if !self.selecting && self.keyboard {
                        // Keyboard mode shoots on touch down (`MobileInput.touchDown`).
                        self.manual_shooting = true;
                    }
                }
            }
            GestureEvent::LongPress { x, y } => {
                if caps.is_menu() || caps.focus().has_mouse || self.mode.schematic_mode {
                    return SmallVec::new();
                }
                if self.state.place_mode == PlaceMode::None {
                    if self.command_mode_enabled() {
                        self.state.command_rect = Some((x, y, 0.0, 0.0));
                    } else {
                        self.manual_shooting = true;
                        self.target = None;
                    }
                } else {
                    let tile = TilePos::new(
                        (x / TILESIZE as f32).floor() as i16,
                        (y / TILESIZE as f32).floor() as i16,
                    );
                    self.begin_line(tile);
                    if self.state.place_mode.is_placing() {
                        self.drag_to(world, block, tile);
                    }
                }
            }
            GestureEvent::Tap { x, y, count } => {
                if self.mode.line_mode || caps.focus().is_menu {
                    return SmallVec::new();
                }
                let tile = TilePos::new(
                    (x / TILESIZE as f32).floor() as i16,
                    (y / TILESIZE as f32).floor() as i16,
                );
                let block_size = block.map(|b| b.size).unwrap_or(1);
                // Remove an existing plan on tap before queueing a new one.
                let existing = if !self.command_mode_enabled() {
                    self.get_plan(tile).cloned()
                } else {
                    None
                };
                if let Some(plan) = existing {
                    self.remove_plan(&plan);
                } else if let Some(block_id) = self.state.block
                    && self.state.place_mode.is_placing()
                {
                    if world.valid_place(
                        block_id,
                        tile.x() as i32,
                        tile.y() as i32,
                        self.state.rotation,
                    ) && !self.check_overlap_placement(
                        tile.x() as i32,
                        tile.y() as i32,
                        block_size,
                    ) {
                        self.add_select_plan(ClientPlan::place(
                            tile.x() as i32,
                            tile.y() as i32,
                            self.state.rotation,
                            block_id,
                        ));
                    }
                } else if self.state.place_mode.is_breaking()
                    && world.block_at(tile.x() as i32, tile.y() as i32) != BlockId::AIR
                    && !self.has_plan(tile)
                {
                    self.add_select_plan(ClientPlan::break_plan(tile.x() as i32, tile.y() as i32));
                } else if count == 2 {
                    self.payload_target = PayloadTarget::None;
                    self.manual_shooting = false;
                }
            }
            GestureEvent::Pan {
                delta_x, delta_y, ..
            } => {
                let _ = self.pan(delta_x, delta_y, 1.0, 1.0);
            }
            GestureEvent::PanStop => self.pan_stop(),
            GestureEvent::Fling { .. } | GestureEvent::Zoom { .. } | GestureEvent::PinchStop => {}
        }
        self.emitted.clone()
    }

    fn command_mode_enabled(&self) -> bool {
        self.state.command_mode
    }

    /// `updateState()` menu branch.
    pub fn reset(&mut self) {
        self.state.reset();
        self.manual_shooting = false;
        self.down = false;
        self.payload_target = PayloadTarget::None;
        self.mode = MobileMode::default();
        self.line_start = None;
        self.selecting = false;
        self.removals.clear();
        self.target = None;
        self.target_building = None;
        self.last_placed = None;
        self.last_block = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::input::caps::TestCaps;
    use crate::input::replay::ReplayWorld;

    #[test]
    fn longpress_line() {
        let mut detector = GestureDetector::new();
        detector.touch_down(0.0, 32.0, 32.0, 0);
        // Not yet held long enough.
        assert!(detector.update(0.1).is_none());
        // After the 0.3 s threshold the long press fires once.
        let event = detector.update(0.31).expect("long press");
        assert!(matches!(event, GestureEvent::LongPress { .. }));
        assert!(detector.update(0.5).is_none(), "fires once");

        let mut controller = MobileController::new();
        controller.state.select_block(Some(BlockId::STONE_WALL));
        controller.state.begin_place();
        let world = ReplayWorld::new();
        controller.handle_gesture(event, &world, None, &TestCaps::default());
        assert!(controller.mode.line_mode);
        assert_eq!(controller.line_start, Some(TilePos::new(4, 4)));
    }

    #[test]
    fn tap_commit() {
        let world = ReplayWorld::new();
        let mut controller = MobileController::new();
        controller.add_select_plan(ClientPlan::place(1, 1, 0, BlockId::STONE_WALL));
        controller.add_select_plan(ClientPlan::place(2, 1, 0, BlockId::STONE_WALL));
        controller.add_select_plan(ClientPlan::break_plan(3, 1));
        assert_eq!(controller.state.select_plans.len(), 3);
        assert_eq!(controller.confirm_plans(&world), 2);
        assert!(controller.state.select_plans.is_empty());
        assert_eq!(controller.queue.len(), 2);
        assert!(controller.queue.iter().all(|plan| !plan.breaking));
    }

    #[test]
    fn pan_shifts_plans() {
        let mut controller = MobileController::new();
        controller.down = true;
        controller.selecting = true;
        controller.add_select_plan(ClientPlan::place(5, 5, 0, BlockId::STONE_WALL));
        // One tile of drag pans the selected plans by exactly one tile.
        let (cam_x, cam_y) = controller.pan(TILESIZE as f32, 0.0, 800.0, 800.0);
        assert_eq!((cam_x, cam_y), (0.0, 0.0));
        assert_eq!(controller.state.select_plans[0].x, 6);
        assert_eq!(controller.state.select_plans[0].y, 5);
    }

    #[test]
    fn zoom_gesture() {
        let mut detector = GestureDetector::new();
        detector.touch_down(0.0, 100.0, 100.0, 0);
        detector.touch_down(0.0, 200.0, 100.0, 1);
        let events = detector.touch_dragged(0.1, 300.0, 100.0, 1);
        let zoom = events
            .iter()
            .find_map(|event| match event {
                GestureEvent::Zoom {
                    initial_distance,
                    distance,
                } => Some((*initial_distance, *distance)),
                _ => None,
            })
            .expect("zoom");
        assert!((zoom.0 - 100.0).abs() < 0.001);
        assert!((zoom.1 - 200.0).abs() < 0.001);

        let mut controller = MobileController::new();
        let scale = controller.zoom(zoom.0, zoom.1, 4.0);
        assert!((scale - 8.0).abs() < 0.001);
        assert!((controller.last_zoom - 8.0).abs() < 0.001);
    }

    #[test]
    fn edge_pan_clamp() {
        let mut controller = MobileController::new();
        // Far outside the left edge: pan magnitude clamps to maxPanSpeed.
        let (vx, vy) = controller.auto_pan(-1000.0, 540.0, 1920.0, 1080.0, 1920.0);
        assert!(vx < 0.0 && vy.abs() < 0.001);
        assert!((vx * vx + vy * vy).sqrt() <= MAX_PAN_SPEED + 0.0001);
        // Inside the viewport: no pan.
        let (vx, vy) = controller.auto_pan(960.0, 540.0, 1920.0, 1080.0, 1920.0);
        assert_eq!((vx, vy), (0.0, 0.0));
    }

    #[test]
    fn doubletap_mine() {
        let tile = TilePos::new(3, 3);
        // Default (`doubletapmine` off): the first tap mines immediately.
        assert!(should_begin_mine(tile, None, 0, false));
        // With double-tap mining on, a single tap does not mine...
        assert!(!should_begin_mine(tile, None, 0, true));
        // ...but a same-tile second tap within 500 ms does.
        assert!(should_begin_mine(tile, Some(tile), 200, true));
        assert!(!should_begin_mine(tile, Some(tile), 600, true));
    }

    #[test]
    fn autotarget() {
        let world = ReplayWorld::new();
        let mut controller = MobileController::new();
        let units = [
            SelectableUnit {
                id: 5,
                type_id: 0,
                x: 30.0,
                y: 30.0,
                team: 1,
                commandable: true,
            },
            SelectableUnit {
                id: 6,
                type_id: 0,
                x: 32.0,
                y: 30.0,
                team: 0,
                commandable: true,
            },
        ];
        // Closest enemy within 20 px is targeted; own units are ignored.
        assert_eq!(
            controller.check_targets(&world, &units, 0, 31.0, 30.0),
            Some(5)
        );
        assert_eq!(controller.target, Some(5));
        // No enemy in range: fall through to building/position resolution.
        assert_eq!(
            controller.check_targets(&world, &units, 0, 400.0, 400.0),
            None
        );
    }

    #[test]
    fn schematic_use_and_origin() {
        let mut controller = MobileController::new();
        let plans = vec![
            ClientPlan::place(4, 4, 0, BlockId::STONE_WALL),
            ClientPlan::place(6, 8, 0, BlockId::STONE_WALL),
        ];
        assert_eq!(controller.schem_origin(), (0, 0));
        controller.use_schematic(plans);
        assert!(controller.has_schematic());
        assert!(controller.mode.schematic_mode);
        assert_eq!(controller.state.place_mode, PlaceMode::SchematicSelect);
        assert_eq!(controller.schem_origin(), (5, 6));
        assert_eq!(controller.confirm_schematic(Vec::new()), 0);
        assert!(!controller.mode.schematic_mode);
        assert_eq!(controller.state.place_mode, PlaceMode::None);
    }

    #[test]
    fn plan_remove_and_overlap() {
        let mut controller = MobileController::new();
        controller.add_select_plan(ClientPlan::place(2, 2, 0, BlockId::STONE_WALL));
        assert!(controller.has_plan(TilePos::new(2, 2)));
        assert_eq!(
            controller.get_plan(TilePos::new(2, 2)).map(|p| p.x),
            Some(2)
        );
        // A 1x1 footprint at the same tile overlaps.
        assert!(controller.check_overlap_placement(2, 2, 1));
        assert!(!controller.check_overlap_placement(9, 9, 1));
        let plan = controller
            .get_plan(TilePos::new(2, 2))
            .cloned()
            .expect("plan");
        assert!(controller.remove_plan(&plan));
        assert!(!controller.remove_plan(&plan));
        assert_eq!(controller.removals.len(), 1);
        assert!(!controller.has_plan(TilePos::new(2, 2)));
    }

    #[test]
    fn rebuild_area_queues_replacements() {
        let mut world = ReplayWorld::new();
        world.place(3, 3, BlockId::STONE_WALL);
        world.place(4, 3, BlockId::STONE_WALL);
        let mut controller = MobileController::new();
        controller.mode.rebuild_mode = true;
        controller.state.place_mode = PlaceMode::RebuildSelect;
        let queued = controller.rebuild_area(&world, 3, 3, 4, 3, |pos, _block| {
            Some(ClientPlan::place(
                pos.x() as i32,
                pos.y() as i32,
                0,
                BlockId::STONE_WALL,
            ))
        });
        assert_eq!(queued, 2);
        assert_eq!(controller.state.select_plans.len(), 2);
        assert_eq!(controller.state.place_mode, PlaceMode::None);
        assert!(!controller.mode.rebuild_mode);
    }

    #[test]
    fn keyboard_gates_pan_zoom() {
        let mut controller = MobileController::new();
        controller.set_keyboard(true);
        assert!(controller.keyboard());
        controller.down = true;
        controller.selecting = true;
        controller.add_select_plan(ClientPlan::place(5, 5, 0, BlockId::STONE_WALL));
        // Pan is disabled in keyboard mode: plans do not shift.
        assert_eq!(
            controller.pan(TILESIZE as f32, 0.0, 800.0, 800.0),
            (0.0, 0.0)
        );
        assert_eq!(controller.state.select_plans[0].x, 5);
        // Zoom is disabled in keyboard mode: the base scale is returned.
        assert_eq!(controller.zoom(100.0, 200.0, 4.0), 4.0);
        // Keyboard camera move is disabled too.
        assert_eq!(controller.camera_move(1.0, 0.0, 0.1), (0.0, 0.0));

        controller.set_keyboard(false);
        let (dx, _dy) = controller.camera_move(1.0, 0.0, 1.0);
        assert!((dx - CAM_SPEED).abs() < 0.001);
        assert!((controller.zoom(100.0, 200.0, 4.0) - 8.0).abs() < 0.001);
    }

    #[test]
    fn touch_down_selects_existing_plan() {
        let world = ReplayWorld::new();
        let mut controller = MobileController::new();
        controller.add_select_plan(ClientPlan::place(4, 4, 0, BlockId::STONE_WALL));
        let caps = TestCaps {
            mobile: true,
            ..TestCaps::default()
        };
        controller.handle_gesture(
            GestureEvent::TouchDown {
                x: 4.0 * TILESIZE as f32,
                y: 4.0 * TILESIZE as f32,
                pointer: 0,
            },
            &world,
            None,
            &caps,
        );
        assert!(controller.selecting);
        assert!(controller.down);
    }

    #[test]
    fn tap_removes_existing_plan() {
        let world = ReplayWorld::new();
        let mut controller = MobileController::new();
        controller.add_select_plan(ClientPlan::place(4, 4, 0, BlockId::STONE_WALL));
        let caps = TestCaps::default();
        controller.handle_gesture(
            GestureEvent::Tap {
                x: 4.0 * TILESIZE as f32,
                y: 4.0 * TILESIZE as f32,
                count: 1,
            },
            &world,
            None,
            &caps,
        );
        assert!(controller.state.select_plans.is_empty());
        assert_eq!(controller.removals.len(), 1);
    }
}

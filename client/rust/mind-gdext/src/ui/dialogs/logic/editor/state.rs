// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `state` support code.
//! Upstream: logic/LogicDialog.java

use super::*;

// ---------------------------------------------------------------------------
// editor state (`LogicDialog` fields)
// ---------------------------------------------------------------------------

/// A `JumpCurve`'s layout data while `DragLayout.setJumpHeights` runs.
#[derive(Clone, Default)]
pub(super) struct CurveData {
    pub(super) jump: i64,
    pub(super) begin: i64,
    pub(super) end: i64,
    pub(super) flipped: bool,
    pub(super) pred: i64,
    pub(super) done: bool,
}

/// The shared scratch buffers of the recursive height prediction (Java's static
/// `tmpOccupiers2`/`tmpBits2` aliased across recursion levels).
#[derive(Default)]
pub(super) struct HeightScratch {
    pub(super) occupiers: Vec<usize>,
    pub(super) bits: BTreeSet<i64>,
}

/// One content picker entry (`showSelectTable` row).
#[derive(Clone, Default)]
pub(super) struct AddEntry {
    pub(super) name: String,
    pub(super) category: String,
    pub(super) color: Color,
    pub(super) label: String,
    pub(super) type_name: String,
    pub(super) tooltip: String,
}

/// One sound browser row (`PlaySoundStatement.showSoundSelect`).
#[derive(Clone, Default)]
pub(super) struct SoundEntry {
    pub(super) name: String,
    pub(super) id: String,
    pub(super) category: String,
}

/// The dialog's mutable model + live node handles (the GDScript's `_` fields).
pub(super) struct LogicState {
    pub(super) x: i64,
    pub(super) y: i64,
    pub(super) privileged: bool,
    pub(super) code: String,
    pub(super) editor_mode: bool,
    pub(super) filter_mode: bool,
    pub(super) force_restart: bool,
    pub(super) statements: Vec<StatementRow>,
    pub(super) list: Option<Gd<VBoxContainer>>,
    pub(super) scroll: Option<Gd<ScrollContainer>>,
    pub(super) modified: bool,
    pub(super) variables_button: Option<Gd<Button>>,
    pub(super) add_button: Option<Gd<Button>>,
    pub(super) vars_popup: Option<Gd<PopupPanel>>,
    pub(super) vars_rows: Vec<(String, Gd<Label>)>,
    pub(super) vars_timer: i64,
    pub(super) row_nodes: Vec<Gd<Control>>,
    pub(super) jump_buttons: BTreeMap<i64, Gd<Button>>,
    pub(super) jump_over: i64,
    pub(super) overlay: Option<Gd<Control>>,
    pub(super) pred_heights: BTreeMap<i64, i64>,
    pub(super) curve_heights: BTreeMap<i64, f32>,
    pub(super) drag_index: i64,
    pub(super) drag_active: bool,
    pub(super) drag_start: Vector2,
    pub(super) drop_index: i64,
    pub(super) drag_row: Option<Gd<Control>>,
    pub(super) drag_grab: Vector2,
    pub(super) jump_selecting: i64,
    pub(super) jump_hover: i64,
    pub(super) jump_pointer: Vector2,
    pub(super) effect_target: (i64, i64),
    pub(super) effect_connected: bool,
    pub(super) confirm_connected: bool,
    pub(super) sub_dialog: Option<Gd<Control>>,
    pub(super) pending_clear: bool,
    pub(super) meta_cache: HashMap<String, VarDictionary>,
    pub(super) region_cache: HashMap<String, Option<Gd<Texture2D>>>,
    pub(super) node_texture: Option<Gd<Texture2D>>,
}

impl Default for LogicState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            privileged: false,
            code: String::new(),
            editor_mode: false,
            filter_mode: false,
            force_restart: false,
            statements: Vec::new(),
            list: None,
            scroll: None,
            modified: false,
            variables_button: None,
            add_button: None,
            vars_popup: None,
            vars_rows: Vec::new(),
            vars_timer: 0,
            row_nodes: Vec::new(),
            jump_buttons: BTreeMap::new(),
            jump_over: -1,
            overlay: None,
            pred_heights: BTreeMap::new(),
            curve_heights: BTreeMap::new(),
            drag_index: -1,
            drag_active: false,
            drag_start: Vector2::ZERO,
            drop_index: -1,
            drag_row: None,
            drag_grab: Vector2::ZERO,
            jump_selecting: -1,
            jump_hover: -1,
            jump_pointer: Vector2::ZERO,
            effect_target: (-1, -1),
            effect_connected: false,
            confirm_connected: false,
            sub_dialog: None,
            pending_clear: false,
            meta_cache: HashMap::new(),
            region_cache: HashMap::new(),
            node_texture: None,
        }
    }
}

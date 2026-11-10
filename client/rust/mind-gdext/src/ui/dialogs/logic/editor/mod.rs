// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor dialog controller; this directory holds its submodules. Filled by S09 and S10 (docs/PLANS).
//! Upstream: logic/LogicDialog.java, LCanvas.java, LStatement.java
//!
//! The mlog logic editor (`LogicDialog` / `LCanvas`).
//!
//! The dialog is a full-screen statement column: every statement is an
//! `LCanvas.StatementElem` — a category-tinted `Tex.whitePane` frame with a
//! category-coloured header (`localizedName()`, address, `+`/copy/delete) over
//! the statement's `LStatement.build(Table)` field row. The column is
//! `getTargetWidth()` wide and centred; jump curves are `JumpCurve` trapezoids
//! driven by `DragLayout.setJumpHeights` lane prediction, drawn every frame by
//! the [`MindLogicOverlay`] child stacked above the statement list. The header
//! drag reorders statements (`DragLayout.insertPosition`), the jump node's
//! press-drag-release picks a `JumpStatement.dest`.
//!
//! `LogicDialog.setup`'s `@back`/`@edit`/`@variables`/`@add` row (160x64) sits in
//! the shared button row; `@add` opens the nested `showAddDialog` menu and
//! `@edit` the `@editor.export` menu (`@clear`/`@copy.clipboard`/
//! `@load.clipboard`/`@logic.restart`). The `@variables` button swaps between
//! the executor's variable panel and `@logic.globals`
//! (`GlobalVarsDialog`/`shouldShowVariables`). Content pencils open the
//! `showSelectTable` pickers (units, ores, statuses, weather, markers,
//! setprop/sensor tabs and the sound browser).
//!
//! Load/save is `MindLogic.parse_statements`/`write_statements` around
//! `SimHost.logic_get_state`/`logic_get_code`/`logic_configure_code` (the editor
//! tile uses `MindEditor`'s `logic_*` pair). The standalone
//! `show(code, null, true, consumer)` mode (`MapGenerateDialog`'s `LogicFilter`)
//! echoes the edited program through [`MindDialog`]'s `code_edited` signal
//! instead of writing a tile.

mod add_menu;
mod canvas;
mod consts;
mod drag;
mod edit_menu;
mod fields;
mod io;
mod jumps;
mod lanes;
mod model;
mod overlay;
mod pickers;
mod queries;
mod select;
mod shell;
mod signals;
mod state;
mod style;
mod text;
mod util;
mod variables;

#[cfg(test)]
mod tests;

use add_menu::*;
use canvas::*;
use consts::*;
use drag::*;
use edit_menu::*;
use fields::*;
use io::*;
use jumps::*;
use lanes::*;
use model::*;
use overlay::*;
use pickers::*;
use queries::*;
use select::*;
use shell::*;
use signals::*;
use state::*;
use style::*;
use text::*;
use util::*;
use variables::*;
// `save` is also exported by `godot::prelude::*`; the old single-module
// file shadowed that glob with the local `io::save`, so name it explicitly.
use io::save;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{CursorShape, FocusMode, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, CenterContainer, ColorPickerButton, ColorRect, Container, Control, DisplayServer,
    GridContainer, HBoxContainer, HFlowContainer, IControl, InputEvent, InputEventMouseButton,
    InputEventMouseMotion, Json, Label, LineEdit, MarginContainer, Node, PanelContainer,
    PopupPanel, ScrollContainer, StyleBox, StyleBoxEmpty, StyleBoxFlat, StyleBoxTexture, Texture2D,
    TextureRect, VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, styles};

// ---------------------------------------------------------------------------
// `LogicDialog` controller
// ---------------------------------------------------------------------------

/// `@logic` — the mlog logic editor.
pub struct LogicDialog {
    state: Rc<RefCell<LogicState>>,
}

impl Default for LogicDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(LogicState::default())),
        }
    }
}

impl DialogController for LogicDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        let state = self.state.clone();
        dialog.set_title_literal("");
        dialog.enable_full_dialog(true);
        dialog.should_pause = true;
        dialog.clear_content();
        dialog.clear_buttons();
        build(dialog, &state);
        connect_resize(dialog, &state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let state = self.state.clone();
        show_loaded(dialog, &state);
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        let state = self.state.clone();
        close_sub_dialog(&state, false);
        state.borrow_mut().pending_clear = false;
        let needs_save = {
            let st = state.borrow();
            st.modified || st.force_restart
        };
        if !needs_save {
            return;
        }
        save(dialog, &state);
    }

    fn blocks_escape(&self) -> bool {
        self.state.borrow().sub_dialog.is_some()
    }
}

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "logic" => Some(Box::new(LogicDialog::default())),
        _ => None,
    }
}

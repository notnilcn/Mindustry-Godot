// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The custom game dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/CustomGameDialog.java

use super::map_list::*;
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::campaign::util::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "custom" => Some(Box::new(CustomGameDialog::default())),
        _ => None,
    }
}

/// `CustomGameDialog extends MapListDialog` with `displayType = false`.
pub struct CustomGameDialog {
    state: Rc<RefCell<MapListState>>,
}

impl Default for CustomGameDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(MapListState::new())),
        }
    }
}

impl DialogController for CustomGameDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@customgame");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        // `makeButtonOverlay()` (`MapListDialog.java:103`).
        dialog.apply_button_overlay(true);
        maplist_load_settings(dialog, &self.state);
        maplist_build(dialog, &self.state);
        let deferred = {
            let st = self.state.borrow();
            st.deferred.clone()
        };
        install_frame_tick(dialog, "custom", deferred, || {});
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        maplist_load_settings(dialog, &self.state);
        maplist_refresh_registry(dialog, &self.state);
        // `setup`: `searchString = null` and a fresh (empty) search field.
        self.state.borrow_mut().search = String::new();
        if let Some(mut field) = self.state.borrow().field.clone() {
            field.set_text("");
        }
        maplist_rebuild(dialog, &self.state);
        // `MapListDialog.show`: desktop focuses the search field.
        if let Some(mut field) = self.state.borrow().field.clone() {
            field.call_deferred("grab_focus", &[]);
        }
    }
}

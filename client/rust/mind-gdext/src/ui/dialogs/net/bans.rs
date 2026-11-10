// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/BansDialog.java.

//! The bans dialog controller: the ban list with one unban action per row.
//! Upstream: ui/dialogs/BansDialog.java

use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::net::roster::{RosterKind, RosterState, roster_ready, roster_shown};

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "bans" => Some(Box::new(BansDialog::default())),
        _ => None,
    }
}

/// `BansDialog`: the ban list with one `Icon.cancel` unban button per row.
#[derive(Default)]
pub struct BansDialog {
    state: Rc<RefCell<RosterState>>,
}

impl DialogController for BansDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        roster_ready(dialog, &self.state, RosterKind::Bans);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        roster_shown(dialog, &self.state, RosterKind::Bans);
    }
}

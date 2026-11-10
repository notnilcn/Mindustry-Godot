// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/AdminsDialog.java.

//! The admins dialog controller: the admin roster with one unadmin action per row.
//! Upstream: ui/dialogs/AdminsDialog.java

use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::net::roster::{RosterKind, RosterState, roster_ready, roster_shown};

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "admins" => Some(Box::new(AdminsDialog::default())),
        _ => None,
    }
}

/// `AdminsDialog`: the admin roster with one `Icon.cancel` unadmin button.
#[derive(Default)]
pub struct AdminsDialog {
    state: Rc<RefCell<RosterState>>,
}

impl DialogController for AdminsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        roster_ready(dialog, &self.state, RosterKind::Admins);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        roster_shown(dialog, &self.state, RosterKind::Admins);
    }
}

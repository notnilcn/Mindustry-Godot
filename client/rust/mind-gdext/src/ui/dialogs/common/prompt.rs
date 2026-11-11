// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! `MindUi` prompt-signal wiring shared by the dialog controllers.
//! Upstream: none (port-only; the prompt seams duplicated in the old batch files)

use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::kit;

/// Connects a `MindUi` prompt signal once per dialog node.
///
/// `meta_prefix` is the owning group's original "already connected" latch key
/// (`_batch_f_`, …): node metadata is observable, so each group keeps the key
/// its batch used instead of a shared one.
pub(crate) fn connect_prompt_once(
    dialog: &mut MindDialog,
    meta_prefix: &str,
    key: &str,
    signal: &str,
    callable: &Callable,
) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    let meta = format!("{meta_prefix}{key}");
    if dialog.base().has_meta(&meta) || !ui.has_signal(signal) {
        return;
    }
    let _ = ui.connect(signal, callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

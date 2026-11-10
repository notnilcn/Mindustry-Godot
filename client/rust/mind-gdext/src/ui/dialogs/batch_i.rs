// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/CustomRulesDialog.java,
//         core/src/mindustry/ui/dialogs/EditorMapsDialog.java,
//         core/src/mindustry/ui/dialogs/CanvasEditDialog.java,
//         core/src/mindustry/ui/dialogs/MapListDialog.java (`displayType`,
//         `showMap` override), core/src/mindustry/logic/CanvasBlock.java
//         (LSB-first pixel packing).

//! Moved to rules/, editor/editor_maps.rs and logic/canvas_edit.rs. Deleted in W5 (C01).

use crate::ui::dialog::DialogController;

/// The dialog controllers of the old batch; every id moved to a group leaf.
pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
    None
}

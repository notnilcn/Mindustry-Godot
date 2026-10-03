// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Focus and input-lock guards (`InputHandler.locked()` + `Core.scene` probes).
//!
//! Plan 15 §3.4. Input state is client-local and never checksummed; `mind-core`
//! never queries Godot (`UiFocus` is pushed by plan 14 / the MCP test double).

use serde::{Deserialize, Serialize};

/// A named lock blocking world mutations (`InputHandler.inputLocks`/cutscene).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockId {
    /// `renderer.isCutscene()`.
    Cutscene,
    /// `logicCutscene` (logic-controlled camera).
    LogicCutscene,
    /// An open dialog owns the world.
    Dialog,
    /// A mod/UI-added lock (`addLock`).
    Custom(u32),
}

impl LockId {
    /// Parity name used in the `input_dump` JSON (§6.3 `locks`).
    pub fn name(self) -> &'static str {
        match self {
            LockId::Cutscene => "cutscene",
            LockId::LogicCutscene => "logic_cutscene",
            LockId::Dialog => "dialog",
            LockId::Custom(_) => "custom",
        }
    }
}

/// `InputHandler.inputLocks` plus the renderer cutscene flags.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputLocks {
    /// Dialog/renderer-driven lock flag.
    pub dialog: bool,
    /// `renderer.isCutscene()`.
    pub cutscene: bool,
    /// `logicCutscene`.
    pub logic_cutscene: bool,
    /// `addLock` reference-counted custom ids.
    pub custom: Vec<u32>,
}

impl InputLocks {
    /// Empty lock set.
    pub fn new() -> Self {
        Self::default()
    }

    /// `InputHandler.addLock`.
    pub fn add_lock(&mut self, id: LockId) {
        match id {
            LockId::Cutscene => self.cutscene = true,
            LockId::LogicCutscene => self.logic_cutscene = true,
            LockId::Dialog => self.dialog = true,
            LockId::Custom(value) => {
                if !self.custom.contains(&value) {
                    self.custom.push(value);
                }
            }
        }
    }

    /// Removes a lock (`removeLock`); returns whether it had been present.
    pub fn remove_lock(&mut self, id: LockId) -> bool {
        match id {
            LockId::Cutscene => std::mem::replace(&mut self.cutscene, false),
            LockId::LogicCutscene => std::mem::replace(&mut self.logic_cutscene, false),
            LockId::Dialog => std::mem::replace(&mut self.dialog, false),
            LockId::Custom(value) => {
                let before = self.custom.len();
                self.custom.retain(|entry| *entry != value);
                self.custom.len() != before
            }
        }
    }

    /// `InputHandler.locked()`.
    pub fn is_locked(&self) -> bool {
        self.cutscene || self.logic_cutscene || self.dialog || !self.custom.is_empty()
    }

    /// Clears every lock (menu transition / world reset).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Parity list for the state dump.
    pub fn names(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.cutscene {
            out.push("cutscene");
        }
        if self.logic_cutscene {
            out.push("logic_cutscene");
        }
        if self.dialog {
            out.push("dialog");
        }
        if !self.custom.is_empty() {
            out.push("custom");
        }
        out
    }
}

/// UI focus flags pushed by plan 14 (`Core.scene` probes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FocusState {
    /// A text field owns keyboard focus (`hasKeyboard`).
    pub has_keyboard: bool,
    /// Pointer is over a UI `Control` (`hasMouse`).
    pub has_mouse: bool,
    /// A dialog is open (`hasDialog`).
    pub has_dialog: bool,
    /// Any focusable text/IME widget is active (`hasField`).
    pub has_field: bool,
    /// Pointer is over a scroll container (`hasScroll`).
    pub has_scroll: bool,
    /// Chat fragment shown.
    pub chat_shown: bool,
    /// Console fragment shown.
    pub console_shown: bool,
    /// HUD fragment shown.
    pub hud_shown: bool,
    /// `state.isMenu()`.
    pub is_menu: bool,
    /// `state.isEditor()`.
    pub is_editor: bool,
    /// Editor blocks palette shown (`editor-blocks-shown` setting branch).
    pub editor_blocks_shown: bool,
}

impl FocusState {
    /// Any text/IME/chat/console owns input (blocks shooting/shortcuts).
    pub fn typing(&self) -> bool {
        self.has_keyboard
            || self.has_field
            || self.chat_shown
            || self.console_shown
            || self.has_dialog
    }

    /// Whether the pointer is over blocking UI.
    pub fn over_ui(&self) -> bool {
        self.has_mouse || self.has_dialog || self.has_scroll
    }

    /// Whether zoom must be suppressed (scroll/chat/console/dialog).
    pub fn blocks_zoom(&self) -> bool {
        self.has_scroll || self.chat_shown || self.console_shown || self.has_dialog
    }

    /// Menu/editor state clears the placement interaction.
    pub fn clears_placement(&self) -> bool {
        self.is_menu || self.is_editor && !self.editor_blocks_shown
    }
}

/// Combined focus + lock guard ported from `InputHandler`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FocusGuards {
    /// Input locks.
    pub locks: InputLocks,
    /// UI focus flags.
    pub focus: FocusState,
}

impl FocusGuards {
    /// `locked()` (gates camera/world mutations, not UI shortcuts).
    pub fn locked(&self) -> bool {
        self.locks.is_locked()
    }

    /// Whether placement/break gestures may start.
    pub fn can_place(&self) -> bool {
        !self.locked() && !self.focus.is_menu && !self.focus.has_dialog && !self.focus.has_field
    }

    /// Whether the player can shoot (no typing/chat/console/dialog).
    pub fn can_shoot(&self) -> bool {
        !self.focus.typing() && !self.locked()
    }

    /// Mining shares the shooting guards (`canShoot`).
    pub fn can_mine(&self) -> bool {
        self.can_shoot()
    }

    /// Whether a wheel event zooms rather than rotates a placement.
    pub fn blocks_zoom(&self) -> bool {
        self.focus.blocks_zoom()
    }

    /// Whether camera pan input is ignored.
    pub fn blocks_camera(&self) -> bool {
        self.locked() || self.focus.typing()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_gates_camera_only() {
        let mut guards = FocusGuards::default();
        guards.locks.add_lock(LockId::Cutscene);
        assert!(guards.locked());
        assert!(guards.blocks_camera());
        // locked() does not, by itself, disable UI-only shortcuts.
        assert!(!guards.focus.typing());
        assert!(guards.locks.remove_lock(LockId::Cutscene));
        assert!(!guards.locked());
        assert!(!guards.locks.remove_lock(LockId::Cutscene));
    }

    #[test]
    fn dialog_blocks_placement() {
        let mut guards = FocusGuards::default();
        assert!(guards.can_place());
        guards.focus.has_dialog = true;
        assert!(!guards.can_place());
        assert!(!guards.can_shoot());
    }

    #[test]
    fn chat_blocks_zoom() {
        let mut guards = FocusGuards::default();
        assert!(!guards.blocks_zoom());
        guards.focus.chat_shown = true;
        assert!(guards.blocks_zoom());
        guards.focus.chat_shown = false;
        guards.focus.has_scroll = true;
        assert!(guards.blocks_zoom());
    }

    #[test]
    fn menu_clears_state() {
        let mut guards = FocusGuards::default();
        guards.focus.is_menu = true;
        assert!(!guards.can_place());
        // updateState() still runs and clears placement when entering a menu.
        guards.focus.is_menu = false;
        guards.focus.is_editor = true;
        guards.focus.editor_blocks_shown = false;
        assert!(guards.focus.clears_placement());
        guards.focus.editor_blocks_shown = true;
        assert!(!guards.focus.clears_placement());
    }
}

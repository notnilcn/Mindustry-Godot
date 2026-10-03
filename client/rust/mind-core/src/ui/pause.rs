// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/BaseDialog.java (`shouldPause`
//         `shown`/`hidden` callbacks), core/src/mindustry/core/UI.java.

//! Godot-free pause-governor model (plan 14 §3.2/§3.4).
//!
//! Upstream `BaseDialog` pauses the sim while a `shouldPause` dialog is shown,
//! recording `wasPaused` so a dialog opened over an already-paused game does not
//! resume it on close. The Godot runtime governor lives in `mind-gdext::ui`, but
//! the state machine is pure and the manifest declares each dialog's flag, so it
//! is fully testable headlessly — including "every `pause: true` dialog freezes
//! the sim".

use crate::ui::manifest::DialogsManifest;

/// A sim-pause transition the governor asks the host to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseAction {
    /// Nothing to do (already in the target state / unguarded).
    None,
    /// Set `state.paused`.
    Pause,
    /// Restore `state.playing`.
    Resume,
}

impl PauseAction {
    /// The `set_paused` argument, when the action is a transition.
    pub fn paused_value(self) -> Option<bool> {
        match self {
            PauseAction::None => None,
            PauseAction::Pause => Some(true),
            PauseAction::Resume => Some(false),
        }
    }
}

/// `BaseDialog` guard: the pause governor only acts in-game and offline
/// (`state.isGame() && !net.active()`).
pub fn should_govern(state_is_game: bool, net_active: bool) -> bool {
    state_is_game && !net_active
}

/// Reference-counted pause state machine (plan §3.2).
///
/// The first acquire records whether the game was already paused; the last
/// release resumes it only when the governor caused the pause.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PauseGovernor {
    depth: u32,
    was_paused: bool,
}

impl PauseGovernor {
    /// Fresh governor (nothing paused).
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of currently-open pause dialogs.
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// Whether the governor itself paused the sim.
    pub fn owns_pause(&self) -> bool {
        self.depth > 0 && !self.was_paused
    }

    /// `BaseDialog.shown`: acquire one pause claim.
    pub fn acquire(&mut self, sim_paused: bool) -> PauseAction {
        if self.depth == 0 {
            self.was_paused = sim_paused;
            self.depth = 1;
            if sim_paused {
                PauseAction::None
            } else {
                PauseAction::Pause
            }
        } else {
            self.depth += 1;
            PauseAction::None
        }
    }

    /// `BaseDialog.hidden`: release one pause claim.
    pub fn release(&mut self) -> PauseAction {
        if self.depth == 0 {
            return PauseAction::None;
        }
        self.depth -= 1;
        if self.depth == 0 && !self.was_paused {
            PauseAction::Resume
        } else {
            PauseAction::None
        }
    }

    /// Full `shown` path: applies the `BaseDialog` guard before acquiring.
    pub fn on_dialog_shown(
        &mut self,
        should_pause: bool,
        state_is_game: bool,
        net_active: bool,
        sim_paused: bool,
    ) -> PauseAction {
        if !should_pause || !should_govern(state_is_game, net_active) {
            return PauseAction::None;
        }
        self.acquire(sim_paused)
    }

    /// Full `hidden` path: applies the `BaseDialog` guard before releasing.
    pub fn on_dialog_hidden(
        &mut self,
        should_pause: bool,
        state_is_game: bool,
        net_active: bool,
    ) -> PauseAction {
        if !should_pause || !should_govern(state_is_game, net_active) {
            return PauseAction::None;
        }
        self.release()
    }
}

/// Iterates the manifest's `pause: true` dialogs (open order irrelevant).
pub fn pause_dialogs(manifest: &DialogsManifest) -> impl Iterator<Item = &str> {
    manifest
        .dialogs
        .iter()
        .filter(|entry| entry.pause)
        .map(|entry| entry.name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Reads the committed registry (same file `ui manifest` validates).
    fn committed_manifest() -> DialogsManifest {
        let client = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = client.join("ui/dialogs_manifest.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        DialogsManifest::from_json(&text).expect("parse dialogs_manifest.json")
    }

    #[test]
    fn guard_matches_base_dialog() {
        assert!(should_govern(true, false));
        assert!(!should_govern(true, true));
        assert!(!should_govern(false, false));
        assert!(!should_govern(false, true));
        assert_eq!(PauseAction::Pause.paused_value(), Some(true));
        assert_eq!(PauseAction::Resume.paused_value(), Some(false));
        assert_eq!(PauseAction::None.paused_value(), None);
    }

    #[test]
    fn pause_dialog_freezes_then_restores() {
        let mut governor = PauseGovernor::new();
        assert_eq!(
            governor.on_dialog_shown(true, true, false, false),
            PauseAction::Pause
        );
        assert_eq!(governor.depth(), 1);
        assert!(governor.owns_pause());
        assert_eq!(
            governor.on_dialog_hidden(true, true, false),
            PauseAction::Resume
        );
        assert_eq!(governor.depth(), 0);
        assert!(!governor.owns_pause());
    }

    #[test]
    fn pre_paused_game_is_not_resumed() {
        let mut governor = PauseGovernor::new();
        assert_eq!(
            governor.on_dialog_shown(true, true, false, true),
            PauseAction::None
        );
        assert_eq!(governor.depth(), 1);
        assert!(!governor.owns_pause());
        assert_eq!(
            governor.on_dialog_hidden(true, true, false),
            PauseAction::None
        );
        assert_eq!(governor.depth(), 0);
    }

    #[test]
    fn network_active_is_a_noop() {
        let mut governor = PauseGovernor::new();
        assert_eq!(
            governor.on_dialog_shown(true, true, true, false),
            PauseAction::None
        );
        assert_eq!(governor.depth(), 0);
        assert_eq!(
            governor.on_dialog_hidden(true, true, true),
            PauseAction::None
        );
    }

    #[test]
    fn non_pause_dialog_never_governs() {
        let mut governor = PauseGovernor::new();
        assert_eq!(
            governor.on_dialog_shown(false, true, false, false),
            PauseAction::None
        );
        assert_eq!(governor.depth(), 0);
    }

    #[test]
    fn nested_pause_dialogs_resume_once() {
        let mut governor = PauseGovernor::new();
        assert_eq!(governor.acquire(false), PauseAction::Pause);
        assert_eq!(governor.acquire(false), PauseAction::None);
        assert_eq!(governor.depth(), 2);
        assert_eq!(governor.release(), PauseAction::None);
        assert_eq!(governor.release(), PauseAction::Resume);
        assert_eq!(governor.release(), PauseAction::None);
        assert_eq!(governor.depth(), 0);
    }

    /// Every `pause: true` entry in the committed manifest must freeze an
    /// unpaused in-game sim and restore it on close.
    #[test]
    fn every_pause_dialog_in_manifest_freezes() {
        let manifest = committed_manifest();
        let names: Vec<&str> = pause_dialogs(&manifest).collect();
        assert!(
            !names.is_empty(),
            "manifest must declare at least one pause dialog"
        );
        let expected = [
            "settings",
            "database",
            "planet",
            "research",
            "schematics",
            "campaign_complete",
            "full_text",
        ];
        for name in expected {
            assert!(
                names.contains(&name),
                "pause dialog '{name}' missing from committed manifest"
            );
        }
        for &name in &names {
            let entry = manifest.dialog(name).expect("pause dialog entry");
            assert!(entry.pause, "pause iterator returned a non-pause dialog");
            let mut governor = PauseGovernor::new();
            assert_eq!(
                governor.on_dialog_shown(true, true, false, false),
                PauseAction::Pause,
                "dialog '{name}' must freeze the sim"
            );
            assert_eq!(
                governor.on_dialog_hidden(true, true, false),
                PauseAction::Resume,
                "dialog '{name}' must restore the sim"
            );
        }
        // And the non-pause dialogs must not be in the freeze set.
        for entry in &manifest.dialogs {
            if !entry.pause {
                assert!(
                    !names.contains(&entry.name.as_str()),
                    "dialog '{}' must not pause",
                    entry.name
                );
            }
        }
    }
}

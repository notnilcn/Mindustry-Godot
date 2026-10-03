// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map-editor playtest lifecycle (`MapEditorDialog`/`MapEditor.playtest`,
//! plan 19 §3.11 / M7).
//!
//! Godot-free state machine over the now-landed plan-12 [`PlaySession`] +
//! [`RulesEpoch`] and plan-07 building runtime. The editor owns the working
//! `rules`; `edit_in_game` snapshots them, applies the hidden
//! [`Gamemode::Editor`] preset and starts a synthetic play session, while
//! `playtest`/`resume_editing`/`resume_after_playtest`/`try_exit` mirror the
//! upstream dialog transitions. The in-engine `MindEditor` facade
//! (`mind-gdext`) delegates here; the `SubViewport` mount/unmount step remains
//! the orchestrator's single-editor MCP mutex (documented deferred).
//!
//! Ported from `core/src/mindustry/editor/MapEditorDialog.java`
//! (`editInGame`/`playtest`/`resumeEditing`/`resumeAfterPlaytest`/`tryExit`).

use std::path::PathBuf;

use crate::game::State;
use crate::game::gamemode::{Gamemode, MapView};
use crate::game::play::{PlayEvent, PlaySession, logic_play, play_map};
use crate::game::rules::Rules;
use crate::game::rules_event::RulesEpoch;
use crate::maps::Map;
use crate::world::MapGenHooks;

use super::MapEditor;

/// Result of [`EditorPlayState::playtest`].
#[derive(Debug, Clone, PartialEq)]
pub enum PlaytestOutcome {
    /// Shift was not held: the caller opens the plan-14 `MapPlayDialog`
    /// (`play_listener` = hide, `show(map, true)`).
    Dialog,
    /// Shift playtest started; an auto-picked gamemode was applied.
    Playing {
        /// Auto-picked gamemode (`survival` → `attack` → `sandbox`).
        mode: Gamemode,
        /// Ordered play events from `control.playMap`.
        events: Vec<PlayEvent>,
    },
}

/// `MapEditorDialog` playtest state (`rules`/`last_saved_rules`/`playtesting`).
#[derive(Debug, Clone)]
pub struct EditorPlayState {
    /// The dialog's working rules (`MapEditorDialog.rules`).
    pub rules: Rules,
    /// Snapshot taken by `edit_in_game`; restored by `resume_editing`.
    pub last_saved_rules: Option<Rules>,
    /// `playtesting` shift flag.
    pub playtesting: bool,
}

impl Default for EditorPlayState {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorPlayState {
    /// A fresh state with default rules.
    pub fn new() -> Self {
        Self {
            rules: Rules::default(),
            last_saved_rules: None,
            playtesting: false,
        }
    }

    /// A state seeded with the given rules (`begin_edit_map` load).
    pub fn with_rules(rules: Rules) -> Self {
        Self {
            rules,
            last_saved_rules: None,
            playtesting: false,
        }
    }

    /// `MapEditorDialog.editInGame()`: snapshot the rules, apply the hidden
    /// `editor` gamemode and start a synthetic play session.
    ///
    /// The synthetic map / camera-follow / core-spawn steps are world concerns
    /// owned by plan 06/07 + the gdext facade; this function owns the
    /// deterministic rules/phase transitions.
    pub fn edit_in_game(
        &mut self,
        editor: &mut MapEditor,
        session: &mut PlaySession,
    ) -> Vec<PlayEvent> {
        self.last_saved_rules = Some(self.rules.clone());
        let mut rules = self.rules.clone();
        Gamemode::Editor.apply(&mut rules);
        rules.limit_map_area = false;
        rules.sector = None;
        rules.fog = false;

        session.reset_world();
        session.rules = rules;
        session.sector = None;
        editor.shown_with_map = true;
        logic_play(session)
    }

    /// `MapEditorDialog.playtest()`: with Shift held, auto-pick a valid gamemode
    /// and `control.playMap(map, map.applyRules(mode), playtesting=true)`.
    ///
    /// Without Shift this returns [`PlaytestOutcome::Dialog`] (the caller owns
    /// the map-picker dialog and the save() step).
    pub fn playtest(
        &mut self,
        editor: &mut MapEditor,
        session: &mut PlaySession,
        map: &Map,
        hooks: &dyn MapGenHooks,
        epoch: &mut RulesEpoch,
        shift: bool,
    ) -> PlaytestOutcome {
        if !shift {
            return PlaytestOutcome::Dialog;
        }
        self.playtesting = true;
        editor.shown_with_map = true;

        let base = map.rules(Rules::default(), hooks);
        let view = MapView::new(map.spawns as usize, map.teams.len());
        let mode = if Gamemode::Survival.valid(view) {
            Gamemode::Survival
        } else if Gamemode::Attack.valid(view) {
            Gamemode::Attack
        } else {
            Gamemode::Sandbox
        };
        let mut applied = base.clone();
        mode.apply(&mut applied);
        let events = play_map(session, applied, Some(&base), |_| false, epoch);
        PlaytestOutcome::Playing { mode, events }
    }

    /// `MapEditorDialog.resumeEditing()`: back to the menu phase with the
    /// pre-playtest rules restored.
    pub fn resume_editing(&mut self, editor: &mut MapEditor, session: &mut PlaySession) {
        session.phase = State::Menu;
        self.rules = self.last_saved_rules.take().unwrap_or_default();
        editor.saved = false;
        editor.shown_with_map = true;
    }

    /// `MapEditorDialog.resumeAfterPlaytest(map)`: the file to re-open through
    /// `begin_edit_map`.
    pub fn resume_after_playtest(&mut self, map: &Map) -> PathBuf {
        map.file.clone()
    }

    /// `MapEditorDialog.tryExit()`: always shows the unsaved-changes confirm,
    /// regardless of the `saved` flag. Returns `true` (a confirmation is
    /// required); the caller runs `logic.reset()` + `hide()` on confirm.
    pub fn try_exit(&mut self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::world::NoopMapGenHooks;

    #[test]
    fn edit_in_game_snapshots_and_applies_editor_mode() {
        let mut editor = MapEditor::new();
        let mut session = PlaySession::new(Rules::default());
        let mut play = EditorPlayState::new();
        play.rules.waves = true;

        let events = play.edit_in_game(&mut editor, &mut session);
        assert_eq!(events, vec![PlayEvent::Play]);
        assert_eq!(session.phase, State::Playing);
        assert!(session.rules.editor);
        assert!(session.rules.infinite_resources && session.rules.instant_build);
        assert!(!session.rules.waves);
        assert!(!session.rules.limit_map_area);
        assert_eq!(
            play.last_saved_rules.as_ref().map(|rules| rules.waves),
            Some(true),
            "pre-playtest rules snapshot preserved"
        );
        assert!(editor.shown_with_map);
    }

    #[test]
    fn resume_editing_restores_snapshot() {
        let mut editor = MapEditor::new();
        let mut session = PlaySession::new(Rules::default());
        let mut play = EditorPlayState::new();
        play.rules.waves = true;
        play.rules.wave_spacing = 42.0;
        play.edit_in_game(&mut editor, &mut session);

        play.resume_editing(&mut editor, &mut session);
        assert_eq!(session.phase, State::Menu);
        assert!(!play.rules.editor);
        assert!(play.rules.waves);
        assert_eq!(play.rules.wave_spacing, 42.0);
        assert!(!editor.saved);
    }

    #[test]
    fn playtest_without_shift_needs_dialog() {
        let mut editor = MapEditor::new();
        let mut session = PlaySession::new(Rules::default());
        let mut play = EditorPlayState::new();
        let mut epoch = RulesEpoch::new();
        let map = crate::maps::Maps::map_for_file(
            PathBuf::from("/maps/playtest.msav"),
            16,
            16,
            "Playtest",
            true,
        );
        let outcome = play.playtest(
            &mut editor,
            &mut session,
            &map,
            &NoopMapGenHooks,
            &mut epoch,
            false,
        );
        assert_eq!(outcome, PlaytestOutcome::Dialog);
        assert_eq!(session.phase, State::Menu);
    }

    #[test]
    fn shift_playtest_picks_sandbox_without_spawns() {
        let mut editor = MapEditor::new();
        let mut session = PlaySession::new(Rules::default());
        let mut play = EditorPlayState::new();
        let mut epoch = RulesEpoch::new();
        let map = crate::maps::Maps::map_for_file(
            PathBuf::from("/maps/playtest.msav"),
            16,
            16,
            "Playtest",
            true,
        );
        let outcome = play.playtest(
            &mut editor,
            &mut session,
            &map,
            &NoopMapGenHooks,
            &mut epoch,
            true,
        );
        match outcome {
            PlaytestOutcome::Playing { mode, events } => {
                assert_eq!(mode, Gamemode::Sandbox, "no spawns -> sandbox fallback");
                assert!(events.contains(&PlayEvent::Play));
                assert!(events.contains(&PlayEvent::NewGame));
            }
            PlaytestOutcome::Dialog => panic!("shift playtest must start"),
        }
        assert_eq!(session.phase, State::Playing);
        assert!(play.playtesting);
    }

    #[test]
    fn try_exit_always_confirms() {
        let mut play = EditorPlayState::new();
        assert!(play.try_exit());
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Injected capabilities (`InputCaps`) replacing direct `ui.*`/`renderer.*`/
//! `Core.settings` reads in the core input controllers (plan 15 §4).
//!
//! Keeping these as a trait makes the platform-neutral decision trees testable
//! headlessly and keeps `mind-core` Godot-free (D1/I1). The real implementation
//! lives in `mind-gdext`; [`TestCaps`] is the headless double.

use super::focus::FocusState;

/// Settings + scene state the input controllers consume.
pub trait InputCaps {
    /// Current UI focus flags (`Core.scene`).
    fn focus(&self) -> FocusState {
        FocusState::default()
    }

    /// `renderer.isCutscene()`.
    fn is_cutscene(&self) -> bool {
        false
    }

    /// `state.isMenu()`.
    fn is_menu(&self) -> bool {
        false
    }

    /// `state.isEditor()`.
    fn is_editor(&self) -> bool {
        false
    }

    /// `settings.getBool("conveyorpathfinding", true)`.
    fn conveyor_pathfinding(&self) -> bool {
        true
    }

    /// `settings.getBool("blockreplace", true)`.
    fn block_replace(&self) -> bool {
        true
    }

    /// `settings.getBool("buildautopause")`.
    fn build_autopause(&self) -> bool {
        false
    }

    /// `settings.getBool("doubletapmine")`.
    fn double_tap_mine(&self) -> bool {
        false
    }

    /// `settings.getBool("commandmodehold", true)`.
    fn command_mode_hold(&self) -> bool {
        true
    }

    /// `settings.getBool("distinctcontrolgroups", true)`.
    fn distinct_control_groups(&self) -> bool {
        true
    }

    /// `settings.getBool("smoothcamera", true)`.
    fn smooth_camera(&self) -> bool {
        true
    }

    /// `settings.getBool("detach-camera")`.
    fn detach_camera(&self) -> bool {
        false
    }

    /// `settings.getBool("swapdiagonal")` (mobile default UI-checked).
    fn swap_diagonal(&self) -> bool {
        false
    }

    /// Whether the active handler is the mobile one.
    fn mobile(&self) -> bool {
        false
    }

    /// `settings.getBool("keyboard")`: mobile keyboard mode disables touch
    /// pan/zoom and enables shoot-on-touch (`MobileInput`).
    fn mobile_keyboard(&self) -> bool {
        false
    }

    /// Player select radius (`11` desktop / `17` mobile).
    fn player_select_range(&self) -> f32 {
        if self.mobile() { 17.0 } else { 11.0 }
    }
}

/// Headless test double with every flag overridable.
#[derive(Debug, Clone, Default)]
pub struct TestCaps {
    /// Focus flags.
    pub focus: FocusState,
    /// Cutscene flag.
    pub cutscene: bool,
    /// Menu flag.
    pub menu: bool,
    /// Editor flag.
    pub editor: bool,
    /// `conveyorpathfinding`.
    pub conveyor_pathfinding: Option<bool>,
    /// `blockreplace`.
    pub block_replace: Option<bool>,
    /// `commandmodehold`.
    pub command_mode_hold: Option<bool>,
    /// `distinctcontrolgroups`.
    pub distinct_control_groups: Option<bool>,
    /// `swapdiagonal`.
    pub swap_diagonal: Option<bool>,
    /// Mobile handler flag.
    pub mobile: bool,
    /// `keyboard` mobile setting.
    pub mobile_keyboard: Option<bool>,
}

impl InputCaps for TestCaps {
    fn focus(&self) -> FocusState {
        self.focus.clone()
    }

    fn is_cutscene(&self) -> bool {
        self.cutscene
    }

    fn is_menu(&self) -> bool {
        self.menu
    }

    fn is_editor(&self) -> bool {
        self.editor
    }

    fn conveyor_pathfinding(&self) -> bool {
        self.conveyor_pathfinding.unwrap_or(true)
    }

    fn block_replace(&self) -> bool {
        self.block_replace.unwrap_or(true)
    }

    fn command_mode_hold(&self) -> bool {
        self.command_mode_hold.unwrap_or(true)
    }

    fn distinct_control_groups(&self) -> bool {
        self.distinct_control_groups.unwrap_or(true)
    }

    fn swap_diagonal(&self) -> bool {
        self.swap_diagonal.unwrap_or(false)
    }

    fn mobile(&self) -> bool {
        self.mobile
    }

    fn mobile_keyboard(&self) -> bool {
        self.mobile_keyboard.unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caps_defaults_match_upstream() {
        let caps = TestCaps::default();
        assert!(caps.conveyor_pathfinding());
        assert!(caps.block_replace());
        assert!(caps.command_mode_hold());
        assert!(caps.distinct_control_groups());
        assert!(!caps.swap_diagonal());
        assert_eq!(caps.player_select_range(), 11.0);
        let mobile = TestCaps {
            mobile: true,
            ..TestCaps::default()
        };
        assert_eq!(mobile.player_select_range(), 17.0);
    }

    #[test]
    fn test_caps_overrides() {
        let caps = TestCaps {
            conveyor_pathfinding: Some(false),
            block_replace: Some(false),
            ..TestCaps::default()
        };
        assert!(!caps.conveyor_pathfinding());
        assert!(!caps.block_replace());
    }
}

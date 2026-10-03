// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PlaceMode` and the mobile-only mode flags (`core/src/mindustry/input/PlaceMode.java`).

/// Mobile-only interaction flags (`MobileInput` mode booleans).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MobileMode {
    /// Line drag mode (`mode == placing` with a start tap).
    pub line_mode: bool,
    /// Schematic placement mode.
    pub schematic_mode: bool,
    /// Derelict rebuild-select mode.
    pub rebuild_mode: bool,
    /// Queued unit command mode.
    pub queue_command_mode: bool,
    /// Select-then-confirm placement pending.
    pub confirm_pending: bool,
}

/// Upstream `PlaceMode` (the active build interaction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaceMode {
    /// No active interaction.
    #[default]
    None,
    /// Breaking blocks.
    Breaking,
    /// Placing/validating a plan.
    Placing,
    /// Selecting an area for a schematic.
    SchematicSelect,
    /// Selecting derelicts to rebuild.
    RebuildSelect,
}

impl PlaceMode {
    /// Parity string used in the `input_dump` state JSON (§6.3).
    pub const fn name(self) -> &'static str {
        match self {
            PlaceMode::None => "none",
            PlaceMode::Breaking => "breaking",
            PlaceMode::Placing => "placing",
            PlaceMode::SchematicSelect => "schematic_select",
            PlaceMode::RebuildSelect => "rebuild_select",
        }
    }

    /// Whether placement plans are being accumulated.
    pub const fn is_placing(self) -> bool {
        matches!(self, PlaceMode::Placing)
    }

    /// Whether a break is active.
    pub const fn is_breaking(self) -> bool {
        matches!(self, PlaceMode::Breaking)
    }

    /// Whether a schematic area is being selected.
    pub const fn is_schematic_selecting(self) -> bool {
        matches!(self, PlaceMode::SchematicSelect)
    }

    /// Whether derelict rebuild selection is active.
    pub const fn is_rebuild_selecting(self) -> bool {
        matches!(self, PlaceMode::RebuildSelect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_and_predicates() {
        assert_eq!(PlaceMode::None.name(), "none");
        assert_eq!(PlaceMode::Placing.name(), "placing");
        assert!(PlaceMode::Placing.is_placing());
        assert!(PlaceMode::Breaking.is_breaking());
        assert!(PlaceMode::SchematicSelect.is_schematic_selecting());
        assert!(PlaceMode::RebuildSelect.is_rebuild_selecting());
        assert!(!PlaceMode::None.is_placing());
    }

    #[test]
    fn mobile_mode_flags() {
        let mut mode = MobileMode::default();
        assert!(!mode.line_mode);
        mode.line_mode = true;
        mode.queue_command_mode = true;
        assert!(mode.line_mode);
        assert!(mode.queue_command_mode);
        assert!(!mode.schematic_mode);
        mode.line_mode = false;
        assert!(!mode.line_mode);
    }
}

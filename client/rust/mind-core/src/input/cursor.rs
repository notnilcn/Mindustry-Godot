// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Cursor resolution (plan 15 §3.8, `DesktopInput.update` cursor block).
//!
//! Pure data: the core names the cursor, `mind-gdext`/plan 16 map it to a
//! `SystemCursor`/Godot `CursorShape`. No drawing and no Godot types here (I6).

/// A system cursor name (`SystemCursor` subset used by placement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorKind {
    /// `SystemCursor.arrow`.
    #[default]
    Arrow,
    /// `SystemCursor.hand` (placing / select plans / movable plan).
    Hand,
    /// `ui.targetCursor` (RTS attack hover).
    Target,
    /// `ui.drillCursor` (mining).
    Drill,
    /// `ui.unloadCursor` (tap own player).
    Unload,
    /// `ui.repairCursor` (repairable derelict).
    Repair,
    /// `SystemCursor.ibeam` (text field).
    Ibeam,
}

impl CursorKind {
    /// Parity name used by the state dump / Godot cursor map.
    pub const fn name(self) -> &'static str {
        match self {
            CursorKind::Arrow => "arrow",
            CursorKind::Hand => "hand",
            CursorKind::Target => "target",
            CursorKind::Drill => "drill",
            CursorKind::Unload => "unload",
            CursorKind::Repair => "repair",
            CursorKind::Ibeam => "ibeam",
        }
    }
}

/// The hover facts `DesktopInput.update` consults, in upstream precedence order
/// (later matches override earlier ones).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CursorContext {
    /// A text field owns focus (`SystemCursor.ibeam`).
    pub has_keyboard: bool,
    /// Hovering an interactable building (`cursor.build.getCursor()`).
    pub hover_interactable: bool,
    /// `canRepairDerelict(cursor)`.
    pub can_repair_derelict: bool,
    /// `isPlacing() || !selectPlans.isEmpty()`.
    pub placing_or_select: bool,
    /// `!isPlacing() && canMine(cursor)`.
    pub can_mine: bool,
    /// RTS command-mode attack hover (`ui.targetCursor`).
    pub command_attack: bool,
    /// A movable plan / existing plan under the cursor.
    pub movable_plan: bool,
    /// `canTapPlayer(...)` (`ui.unloadCursor`).
    pub can_tap_player: bool,
}

/// Resolves the cursor exactly like the `DesktopInput.update` if-chain
/// (last match wins).
pub fn resolve_cursor(context: &CursorContext) -> CursorKind {
    if context.has_keyboard {
        return CursorKind::Ibeam;
    }
    let mut cursor = CursorKind::Arrow;
    if context.hover_interactable {
        cursor = CursorKind::Hand;
    }
    if context.can_repair_derelict {
        cursor = CursorKind::Repair;
    }
    if context.placing_or_select {
        cursor = CursorKind::Hand;
    }
    if context.can_mine {
        cursor = CursorKind::Drill;
    }
    if context.command_attack {
        cursor = CursorKind::Target;
    }
    if context.movable_plan {
        cursor = CursorKind::Hand;
    }
    if context.can_tap_player {
        cursor = CursorKind::Unload;
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_precedence_matches_upstream_order() {
        let mut ctx = CursorContext::default();
        assert_eq!(resolve_cursor(&ctx), CursorKind::Arrow);

        ctx.hover_interactable = true;
        assert_eq!(resolve_cursor(&ctx), CursorKind::Hand);

        ctx.can_repair_derelict = true;
        assert_eq!(resolve_cursor(&ctx), CursorKind::Repair);

        // Placing overrides repair (upstream order).
        ctx.placing_or_select = true;
        assert_eq!(resolve_cursor(&ctx), CursorKind::Hand);

        ctx.can_mine = true;
        assert_eq!(resolve_cursor(&ctx), CursorKind::Drill);

        ctx.command_attack = true;
        assert_eq!(resolve_cursor(&ctx), CursorKind::Target);

        ctx.movable_plan = true;
        assert_eq!(resolve_cursor(&ctx), CursorKind::Hand);

        ctx.can_tap_player = true;
        assert_eq!(resolve_cursor(&ctx), CursorKind::Unload);

        // A keyboard field always wins.
        ctx.has_keyboard = true;
        assert_eq!(resolve_cursor(&ctx), CursorKind::Ibeam);
    }
}

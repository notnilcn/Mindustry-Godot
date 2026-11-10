// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `consts` support code.
//! Upstream: logic/LogicDialog.java, logic/LCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// constants (`LCategory` / `LogicDialog` / `LCanvas` / `LExecutor`)
// ---------------------------------------------------------------------------

/// `LExecutor.maxInstructions`.
pub(super) const MAX_INSTRUCTIONS: usize = 1000;
/// `DragLayout.space`.
pub(super) const ELEMENT_SPACE: i32 = 10;
/// `JumpButton` cell size (`JumpStatement.build`: `.size(30)`).
pub(super) const JUMP_BUTTON_SIZE: f32 = 30.0;
/// `Pal.place` (jump curves/buttons while hovered).
pub(super) const PLACE: Color = Color::from_rgba8(0x63, 0x35, 0xf8, 0xff);
/// `Pal.darkishGray` (Add dialog category headers).
pub(super) const DARKISH_GRAY: Color = Color::from_rgba(0.3, 0.3, 0.3, 1.0);
/// Meta key marking the one `size_changed` hook (`LogicDialog.onResize`).
pub(super) const RESIZE_META: &str = "mind_logic_resize";

/// `LCategory` colours (`Pal`; `LCategory.java:16-22`).
pub(super) fn category_color(category: &str) -> Color {
    match category {
        "io" => Color::from_rgba8(0xa0, 0x8a, 0x8a, 0xff),
        "block" => Color::from_rgba8(0xd4, 0x81, 0x6b, 0xff),
        "operation" => Color::from_rgba8(0x87, 0x7b, 0xad, 0xff),
        "control" => Color::from_rgba8(0x6b, 0xb2, 0xb2, 0xff),
        "unit" => Color::from_rgba8(0xc7, 0xb5, 0x9d, 0xff),
        "world" => Color::from_rgba8(0x6b, 0x84, 0xd4, 0xff),
        _ => Color::from_rgba8(0x4d, 0x4d, 0x4d, 0xff),
    }
}

/// `LCategory` icons (`Icon.logicSmall`, `effectSmall`, `settingsSmall`,
/// `rotateSmall`, `unitsSmall`, `terrainSmall`; unknown has none).
pub(super) fn category_icon(category: &str) -> &'static str {
    match category {
        "io" => "logic",
        "block" => "effect",
        "operation" => "settings",
        "control" => "rotate",
        "unit" => "units",
        "world" => "terrain",
        _ => "",
    }
}

/// `LStatement.showSelect` content pickers (Java `showSelectTable` grids,
/// `LStatements.java`): `name -> (field index, picker kind)`.
pub(super) fn content_picker(statement: &str, field_index: i64) -> &'static str {
    let table: &[(&str, i64, &str)] = &[
        ("ubind", 0, "unit"),
        ("playsound", 1, "sound"),
        ("status", 1, "status"),
        ("weathersense", 1, "weather"),
        ("weatherset", 0, "weather"),
        ("setprop", 0, "prop"),
        ("sensor", 2, "sensor"),
        ("setmarker", 2, "layer"),
        ("ulocate", 3, "ore"),
    ];
    table
        .iter()
        .find(|(name, index, _)| *name == statement && *index == field_index)
        .map(|(_, _, kind)| *kind)
        .unwrap_or("")
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawI` packing and the `DisplayCmd` bit layout (plan 13 M4 §6.6).
//!
//! Ported from `core/src/mindustry/logic/LExecutor.java` (`DrawI`) and
//! `world/blocks/logic/LogicDisplay.java` (`DisplayCmd`/`GraphicsType`). The
//! command queue is sim state; the GPU `FrameBuffer`/`processCommands` is plan
//! 16.

use crate::logic::executor::{Executor, MAX_GRAPHICS_BUFFER};
use crate::logic::value::{LogicObject, VarRef};

/// `LogicDisplay.commandClear`.
pub const COMMAND_CLEAR: u8 = 0;
/// `LogicDisplay.commandColor`.
pub const COMMAND_COLOR: u8 = 1;
/// `LogicDisplay.commandColorPack` (virtual, expanded here).
pub const COMMAND_COLOR_PACK: u8 = 2;
/// `LogicDisplay.commandStroke`.
pub const COMMAND_STROKE: u8 = 3;
/// `LogicDisplay.commandLine`.
pub const COMMAND_LINE: u8 = 4;
/// `LogicDisplay.commandRect`.
pub const COMMAND_RECT: u8 = 5;
/// `LogicDisplay.commandLineRect`.
pub const COMMAND_LINE_RECT: u8 = 6;
/// `LogicDisplay.commandPoly`.
pub const COMMAND_POLY: u8 = 7;
/// `LogicDisplay.commandLinePoly`.
pub const COMMAND_LINE_POLY: u8 = 8;
/// `LogicDisplay.commandTriangle`.
pub const COMMAND_TRIANGLE: u8 = 9;
/// `LogicDisplay.commandImage`.
pub const COMMAND_IMAGE: u8 = 10;
/// `LogicDisplay.commandPrint` (virtual, expanded here).
pub const COMMAND_PRINT: u8 = 11;
/// `LogicDisplay.commandTranslate`.
pub const COMMAND_TRANSLATE: u8 = 12;
/// `LogicDisplay.commandScale`.
pub const COMMAND_SCALE: u8 = 13;
/// `LogicDisplay.commandRotate`.
pub const COMMAND_ROTATE: u8 = 14;
/// `LogicDisplay.commandResetTransform`.
pub const COMMAND_RESET_TRANSFORM: u8 = 15;
/// `LogicDisplay.displayDrawType`.
pub const DISPLAY_DRAW_TYPE: i32 = 30;
/// `LogicDisplay.scaleStep`.
pub const SCALE_STEP: f32 = 0.05;

/// `DrawI.pack` (9-bit unsigned magnitude, `0b0111111111`).
pub fn pack(value: i32) -> i32 {
    value & 0x1FF
}

/// `DrawI.packSign` (9-bit magnitude + sign bit, `0b1000000000`).
pub fn pack_sign(value: i32) -> i32 {
    (value.abs() & 0x1FF) | if value < 0 { 0x200 } else { 0 }
}

/// `LogicDisplay.unpackSign`.
pub fn unpack_sign(value: i32) -> i32 {
    (value & 0x1FF) * if value & 0x200 != 0 { -1 } else { 1 }
}

/// `DisplayCmd.get(type, x, y, p1, p2, p3, p4)` (4 + 6×10 bit little-endian).
pub fn pack_cmd(type_: u8, x: i32, y: i32, p1: i32, p2: i32, p3: i32, p4: i32) -> u64 {
    (type_ as u64 & 0xF)
        | ((x as u64 & 0x3FF) << 4)
        | ((y as u64 & 0x3FF) << 14)
        | ((p1 as u64 & 0x3FF) << 24)
        | ((p2 as u64 & 0x3FF) << 34)
        | ((p3 as u64 & 0x3FF) << 44)
        | ((p4 as u64 & 0x3FF) << 54)
}

/// `DisplayCmd.type`.
pub fn cmd_type(cmd: u64) -> u8 {
    (cmd & 0xF) as u8
}

/// `DisplayCmd.x`.
pub fn cmd_x(cmd: u64) -> i32 {
    ((cmd >> 4) & 0x3FF) as i32
}

/// `DisplayCmd.y`.
pub fn cmd_y(cmd: u64) -> i32 {
    ((cmd >> 14) & 0x3FF) as i32
}

/// `DisplayCmd.p1`.
pub fn cmd_p1(cmd: u64) -> i32 {
    ((cmd >> 24) & 0x3FF) as i32
}

/// `DisplayCmd.p2`.
pub fn cmd_p2(cmd: u64) -> i32 {
    ((cmd >> 34) & 0x3FF) as i32
}

/// `DisplayCmd.p3`.
pub fn cmd_p3(cmd: u64) -> i32 {
    ((cmd >> 44) & 0x3FF) as i32
}

/// `DisplayCmd.p4`.
pub fn cmd_p4(cmd: u64) -> i32 {
    ((cmd >> 54) & 0x3FF) as i32
}

/// `LogicFontMetrics` seam (plan 13 §8 R4); plan 16 supplies real atlas metrics.
pub trait LogicFontMetrics {
    /// Whether the glyph exists (`Fonts.logic.getData().hasGlyph`).
    fn has_glyph(&self, c: char) -> bool;
    /// `spaceXadvance`.
    fn space_xadvance(&self) -> f32;
    /// `lineHeight`.
    fn line_height(&self) -> f32;
}

/// Committed fallback metrics captured from the upstream logic font.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultLogicFontMetrics;

impl LogicFontMetrics for DefaultLogicFontMetrics {
    fn has_glyph(&self, c: char) -> bool {
        // ASCII coverage is what the ported scenarios exercise; plan 16 replaces
        // this with the real glyph table.
        c.is_ascii()
    }
    fn space_xadvance(&self) -> f32 {
        8.0
    }
    fn line_height(&self) -> f32 {
        14.0
    }
}

/// `Color.whiteFloatBits` fallback for a fresh display.
pub fn white_float_bits() -> f32 {
    f32::from_bits(super::pack_color(1.0, 1.0, 1.0, 1.0) as u32)
}

fn num_of(exec: &Executor, v: VarRef) -> f64 {
    exec.arena.get(v.id()).num()
}

fn numi_of(exec: &Executor, v: VarRef) -> i32 {
    exec.arena.get(v.id()).numi()
}

/// `DrawI` head: packs one command into the executor's graphics buffer.
#[allow(clippy::too_many_arguments)]
pub fn pack_draw(
    exec: &mut Executor,
    type_: u8,
    x: VarRef,
    y: VarRef,
    p1: VarRef,
    p2: VarRef,
    p3: VarRef,
    p4: VarRef,
) {
    // Deviation 7: headless draw packing is skipped unless capture is enabled.
    if exec.skip_draw_pack || exec.graphics_buffer.len() >= MAX_GRAPHICS_BUFFER {
        return;
    }

    if type_ == COMMAND_COLOR_PACK {
        let packed = num_of(exec, x);
        let value = packed.to_bits() as u32;
        let r = ((value & 0xff00_0000) >> 24) as i32;
        let g = ((value & 0x00ff_0000) >> 16) as i32;
        let b = ((value & 0x0000_ff00) >> 8) as i32;
        let a = (value & 0x0000_00ff) as i32;
        let cmd = pack_cmd(COMMAND_COLOR, pack(r), pack(g), pack(b), pack(a), 0, 0);
        exec.graphics_buffer.push(cmd);
        return;
    }

    if type_ == COMMAND_PRINT {
        let text = std::mem::take(&mut exec.text_buffer);
        if text.is_empty() {
            return;
        }
        let metrics = DefaultLogicFontMetrics;
        let advance = metrics.space_xadvance() as i32;
        let line_height = metrics.line_height() as i32;
        let align = numi_of(exec, p1);

        let mut max_width = 0;
        let mut lines = 1;
        let mut line_width = 0;
        for next in text.chars() {
            if next == '\n' {
                max_width = max_width.max(line_width);
                line_width = 0;
                lines += 1;
            } else {
                line_width += 1;
            }
        }
        max_width = max_width.max(line_width);

        let width = max_width * advance;
        let height = lines * line_height;
        let ha = align_horizontal(align);
        let va = align_vertical(align);
        let x_offset = -((width as f32 * ha) as i32);
        let y_offset = -((height as f32 * va) as i32) + (lines - 1) * line_height;

        let mut cur_x = numi_of(exec, x);
        let mut cur_y = numi_of(exec, y);
        for next in text.chars() {
            if next == '\n' {
                cur_y -= line_height;
                cur_x = numi_of(exec, x);
                continue;
            }
            if metrics.has_glyph(next) {
                exec.graphics_buffer.push(pack_cmd(
                    COMMAND_PRINT,
                    pack_sign(cur_x + x_offset),
                    pack_sign(cur_y + y_offset),
                    next as i32,
                    0,
                    0,
                    0,
                ));
            }
            cur_x += advance;
            if exec.graphics_buffer.len() >= MAX_GRAPHICS_BUFFER {
                break;
            }
        }
        return;
    }

    let mut x_val = pack_sign(numi_of(exec, x));
    let mut y_val = pack_sign(numi_of(exec, y));
    let mut num1 = pack_sign(numi_of(exec, p1));
    let num4 = pack_sign(numi_of(exec, p4));

    if type_ == COMMAND_IMAGE {
        let mut packed = -1;
        match exec.arena.get(p1.id()).value_obj() {
            Some(LogicObject::Content(content)) => {
                packed = ((content.id as i32) << 5) | (content.type_.ordinal() as i32 & 31);
            }
            Some(LogicObject::Building(_)) => {
                // `LogicDisplayBuild` source: `(rootDisplay.index << 5) | 30`.
                // The display arena index is resolved at flush time (plan 16);
                // pack the sentinel type here.
                packed = DISPLAY_DRAW_TYPE;
            }
            _ => {}
        }
        num1 = packed & 0x3FF;
        // `num4` is recomputed from `packed >> 10` (not `p4`).
        let num4 = packed >> 10;
        let p2v = pack_sign(numi_of(exec, p2));
        let p3v = pack_sign(numi_of(exec, p3));
        exec.graphics_buffer
            .push(pack_cmd(type_, x_val, y_val, num1, p2v, p3v, num4));
        return;
    }

    if type_ == COMMAND_SCALE {
        x_val = pack_sign((num_of(exec, x) as f32 / SCALE_STEP) as i32);
        y_val = pack_sign((num_of(exec, y) as f32 / SCALE_STEP) as i32);
    }

    let p2v = pack_sign(numi_of(exec, p2));
    let p3v = pack_sign(numi_of(exec, p3));
    exec.graphics_buffer
        .push(pack_cmd(type_, x_val, y_val, num1, p2v, p3v, num4));
}

fn align_horizontal(align: i32) -> f32 {
    // Arc `Align`: top=1, bottom=2, left=4, right=8, center=16.
    let is_left = align & 4 != 0;
    let is_right = align & 8 != 0;
    ((if is_left { -1.0 } else { 0.0 }) + 1.0 + (if is_right { 1.0 } else { 0.0 })) / 2.0
}

fn align_vertical(align: i32) -> f32 {
    let is_bottom = align & 2 != 0;
    let is_top = align & 1 != 0;
    ((if is_bottom { -1.0 } else { 0.0 }) + 1.0 + (if is_top { 1.0 } else { 0.0 })) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        for value in [-300, -1, 0, 1, 300, 511] {
            assert_eq!(unpack_sign(pack_sign(value)), value);
        }
        let cmd = pack_cmd(
            COMMAND_LINE,
            pack_sign(-5),
            pack_sign(7),
            pack_sign(3),
            0,
            pack_sign(-2),
            pack_sign(1),
        );
        assert_eq!(cmd_type(cmd), COMMAND_LINE);
        assert_eq!(unpack_sign(cmd_x(cmd)), -5);
        assert_eq!(unpack_sign(cmd_y(cmd)), 7);
        assert_eq!(unpack_sign(cmd_p1(cmd)), 3);
        assert_eq!(unpack_sign(cmd_p3(cmd)), -2);
    }

    #[test]
    fn color_pack_expands_to_color_command() {
        let mut exec = Executor::new();
        let x = exec.arena.put_var("x");
        exec.arena
            .get_mut(x)
            .set_num(super::super::pack_color(1.0, 0.0, 0.0, 1.0));
        let y = exec.arena.put_var("y");
        let p1 = exec.arena.put_var("p1");
        let p2 = exec.arena.put_var("p2");
        let p3 = exec.arena.put_var("p3");
        let p4 = exec.arena.put_var("p4");
        pack_draw(
            &mut exec,
            COMMAND_COLOR_PACK,
            VarRef::Local(x),
            VarRef::Local(y),
            VarRef::Local(p1),
            VarRef::Local(p2),
            VarRef::Local(p3),
            VarRef::Local(p4),
        );
        assert_eq!(exec.graphics_buffer.len(), 1);
        let cmd = exec.graphics_buffer[0];
        assert_eq!(cmd_type(cmd), COMMAND_COLOR);
        // `commandColor` spreads r/g/b/a across x/y/p1/p2.
        assert_eq!(cmd_x(cmd), 255);
        assert_eq!(cmd_y(cmd), 0);
        assert_eq!(cmd_p1(cmd), 0);
        assert_eq!(cmd_p2(cmd), 255);
    }
}

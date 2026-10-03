// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LAssembler` — statements to instructions plus the variable table.
//!
//! Ported from `core/src/mindustry/logic/LAssembler.java`: `assemble`/`read`/
//! `write`, `var`/`putVar`/`putConst`/`getVar`, numeric parsing
//! (decimal/scientific/hex/binary with 64-bit wrap, `%rrggbb[aa]`, `%[name]`)
//! and string unescaping.

use crate::logic::executor::{Instruction, build_statement};
use crate::logic::globals::{GlobalVars, insert_into, named_color, rgba_to_double_bits};
use crate::logic::parser::{ParseError, Parser};
use crate::logic::statement::Statement;
use crate::logic::value::{LVar, LogicObject, VarArena, VarRef};

/// "Compiles" statements into instructions.
#[derive(Debug)]
pub struct Assembler {
    /// Privileged (world processor) assemble.
    pub privileged: bool,
    /// Variable table (insertion-ordered).
    pub arena: VarArena,
    /// Compiled instruction list.
    pub instructions: Vec<Instruction>,
    /// Global constants.
    pub globals: GlobalVars,
    /// Status-effect names for the `status` legacy rewrite (content hook).
    pub status_names: Vec<String>,
}

impl Default for Assembler {
    fn default() -> Self {
        Self::new()
    }
}

impl Assembler {
    /// Creates an assembler with the `LAssembler` bootstrap variables.
    pub fn new() -> Self {
        let mut arena = VarArena::new();
        // @counter is a numeric variable
        let counter = arena.put_var("@counter");
        arena.get_mut(counter).is_obj = false;
        // @unit / @this are null object constants
        arena.put_obj_const("@unit", None);
        arena.put_obj_const("@this", None);
        Self {
            privileged: false,
            arena,
            instructions: Vec::new(),
            globals: GlobalVars::new(),
            status_names: Vec::new(),
        }
    }

    /// `LAssembler.assemble`.
    pub fn assemble(text: &str, privileged: bool) -> Result<Assembler, ParseError> {
        Self::assemble_with(text, privileged, GlobalVars::new())
    }

    /// `LAssembler.assemble` against a caller-supplied global arena.
    ///
    /// The globals carry content/sound constants; executors receive a clone of
    /// this arena at `load` and observe live values through
    /// [`GlobalVars::update`](crate::logic::globals::GlobalVars::update).
    pub fn assemble_with(
        text: &str,
        privileged: bool,
        globals: GlobalVars,
    ) -> Result<Assembler, ParseError> {
        let statements = Self::read(text, privileged)?;
        let mut asm = Assembler::new();
        asm.privileged = privileged;
        asm.globals = globals;
        asm.instructions = statements
            .iter()
            .filter_map(|statement| build_statement(statement, &mut asm))
            .collect();
        Ok(asm)
    }

    /// `LAssembler.read`.
    pub fn read(text: &str, privileged: bool) -> Result<Vec<Statement>, ParseError> {
        if text.is_empty() {
            return Ok(Vec::new());
        }
        Parser::new(text, privileged).parse()
    }

    /// `LAssembler.write`.
    pub fn write(statements: &[Statement]) -> String {
        let mut out = String::new();
        for statement in statements {
            statement.write(&mut out);
            out.push('\n');
        }
        out
    }

    /// `LAssembler.var`.
    ///
    /// Global constants resolve to [`VarRef::Global`] and stay live (deviation
    /// 2); string/number constants and free variables stay executor-local.
    pub fn var(&mut self, symbol: &str) -> VarRef {
        if let Some(global) = self.globals.get_ref(symbol, self.privileged) {
            return global;
        }

        // string literal
        if symbol.len() > 1 && symbol.starts_with('"') && symbol.ends_with('"') {
            let inner = &symbol[1..symbol.len() - 1];
            let value = unescape(inner);
            let key = format!("___{symbol}");
            let id = self.arena.put_str_const(&key, value);
            return VarRef::Local(id);
        }

        let value = parse_double(symbol);
        if value.is_nan() {
            VarRef::Local(self.arena.put_var(symbol))
        } else {
            let value = if value.is_infinite() { 0.0 } else { value };
            let key = format!("___{value}");
            VarRef::Local(self.arena.put_num_const(&key, value))
        }
    }

    /// `LAssembler.putVar`.
    pub fn put_var(&mut self, name: &str) -> VarRef {
        VarRef::Local(self.arena.put_var(name))
    }

    /// `LAssembler.putConst(name, number)`.
    pub fn put_num_const(&mut self, name: &str, value: f64) -> VarRef {
        VarRef::Local(self.arena.put_num_const(name, value))
    }

    /// `LAssembler.putConst(name, object)`.
    pub fn put_obj_const(&mut self, name: &str, value: Option<LogicObject>) -> VarRef {
        VarRef::Local(self.arena.put_obj_const(name, value))
    }

    /// `LAssembler.putConst(name, value)` matching `Object`/`Number` semantics.
    pub fn put_const(&mut self, name: &str, mut value: LVar) -> VarRef {
        value.name = name.to_owned();
        VarRef::Local(insert_into(&mut self.arena, &value))
    }

    /// `LAssembler.getVar`.
    pub fn get_var(&self, name: &str) -> Option<VarRef> {
        self.arena.get_id(name).map(VarRef::Local)
    }
}

/// `LAssembler.unescape`.
pub fn unescape(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_owned();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == 'n' {
                out.push('\n');
                i += 2;
                continue;
            } else if next == '"' || next == '\\' {
                out.push(next);
                i += 2;
                continue;
            } else if next == 'u' && i + 5 < chars.len() {
                let hex: String = chars[i + 2..i + 6].iter().collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
                i += 6;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// `LAssembler.isNumStart`.
fn is_num_start(c: char) -> bool {
    c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == '%'
}

/// `LAssembler.parseDouble`.
pub fn parse_double(symbol: &str) -> f64 {
    let mut chars = symbol.chars();
    match chars.next() {
        None => return f64::NAN,
        Some(first) if !is_num_start(first) => return f64::NAN,
        _ => {}
    }

    if let Some(rest) = symbol.strip_prefix("0b") {
        return parse_hex_or_bin(false, rest, true);
    }
    if let Some(rest) = symbol.strip_prefix("+0b") {
        return parse_hex_or_bin(false, rest, true);
    }
    if let Some(rest) = symbol.strip_prefix("-0b") {
        return parse_hex_or_bin(true, rest, true);
    }
    if let Some(rest) = symbol.strip_prefix("0x") {
        return parse_hex_or_bin(false, rest, false);
    }
    if let Some(rest) = symbol.strip_prefix("+0x") {
        return parse_hex_or_bin(false, rest, false);
    }
    if let Some(rest) = symbol.strip_prefix("-0x") {
        return parse_hex_or_bin(true, rest, false);
    }
    if symbol.starts_with("%[") && symbol.ends_with(']') && symbol.len() > 3 {
        return parse_named_color(symbol);
    }
    if symbol.starts_with('%') && (symbol.len() == 7 || symbol.len() == 9) {
        return parse_color(symbol);
    }

    symbol.parse::<f64>().unwrap_or(f64::NAN)
}

/// `LAssembler.parseHexOrBin` (unsigned accumulate with 64-bit wrap).
fn parse_hex_or_bin(negative: bool, s: &str, binary: bool) -> f64 {
    if s.is_empty() {
        return f64::NAN;
    }
    let bytes: Vec<char> = s.chars().collect();
    let end = bytes.len();
    let mut pos = 0;
    while pos < end && bytes[pos] == '0' {
        pos += 1;
    }
    let shift = if binary { 1 } else { 4 };
    if end - pos > 64 / shift {
        return f64::NAN;
    }
    let radix = 1u32 << shift;
    let mut acc: i64 = 0;
    while pos < end {
        let digit = bytes[pos].to_digit(radix);
        match digit {
            Some(d) => acc = acc.wrapping_shl(shift as u32) | d as i64,
            None => return f64::NAN,
        }
        pos += 1;
    }
    if negative { -(acc as f64) } else { acc as f64 }
}

/// `LAssembler.parseColor`.
fn parse_color(symbol: &str) -> f64 {
    let bytes: Vec<char> = symbol.chars().collect();
    let r = parse_hex_range(&bytes, 1, 3);
    let g = parse_hex_range(&bytes, 3, 5);
    let b = parse_hex_range(&bytes, 5, 7);
    let a = if bytes.len() == 9 {
        parse_hex_range(&bytes, 7, 9)
    } else {
        255
    };
    rgba_to_double_bits((r as u8, g as u8, b as u8, a as u8))
}

fn parse_hex_range(chars: &[char], start: usize, end: usize) -> i32 {
    chars[start..end]
        .iter()
        .fold(0i32, |acc, c| acc * 16 + c.to_digit(16).unwrap_or(0) as i32)
}

/// `LAssembler.parseNamedColor`.
fn parse_named_color(symbol: &str) -> f64 {
    let name = &symbol[2..symbol.len() - 1];
    match named_color(name) {
        Some(color) => rgba_to_double_bits(color),
        None => f64::NAN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::globals::NAMED_COLORS;

    fn arena_var(asm: &mut Assembler, symbol: &str) -> LVar {
        match asm.var(symbol) {
            VarRef::Local(id) => asm.arena.get(id).clone(),
            VarRef::Global(id) => asm
                .globals
                .global_cell(id)
                .cloned()
                .unwrap_or_else(|| LVar::new(symbol)),
        }
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn parse_decimal_hex_binary() {
        assert_eq!(parse_double("0"), 0.0);
        assert_eq!(parse_double("42"), 42.0);
        assert_eq!(parse_double("-42"), -42.0);
        assert_eq!(parse_double("+42"), 42.0);
        assert_eq!(parse_double("3.14"), 3.14);
        assert_eq!(parse_double("1e3"), 1000.0);
        assert_eq!(parse_double("1.5e-2"), 0.015);
        assert_eq!(parse_double("0xff"), 255.0);
        assert_eq!(parse_double("0xFFFFFFFFFFFFFFFF"), -1.0);
        assert_eq!(parse_double("0x8000000000000000"), i64::MIN as f64);
        assert_eq!(parse_double("-0xFFFFFFFFFFFFFFFF"), 1.0);
        assert_eq!(parse_double("0b1010"), 10.0);
        assert_eq!(parse_double("0b0"), 0.0);
    }

    #[test]
    fn invalid_numbers_stay_nan() {
        for symbol in [
            "",
            "abc",
            "e10",
            "NaN",
            "Infinity",
            "0x",
            "0b",
            "+0x",
            "-0b",
            "0xG",
            "0x12G4",
            "0b2",
            "0b102",
            "0x-1",
            "0x1.5",
            "%fff",
            "%[nosuchcolor]",
        ] {
            assert!(parse_double(symbol).is_nan(), "should be NaN: {symbol}");
        }
        assert!(parse_double("0x10000000000000000").is_nan());
        assert!(
            parse_double("0b10000000000000000000000000000000000000000000000000000000000000000")
                .is_nan()
        );
    }

    #[test]
    fn parse_colors() {
        let a = f64::from_bits(0xff_ff_ff_ff);
        assert_eq!(parse_double("%ffffff"), a);
        let b = f64::from_bits(0xff_00_00_80);
        assert_eq!(parse_double("%ff000080"), b);
        let red = rgba_to_double_bits(NAMED_COLORS.iter().find(|(n, _)| *n == "red").unwrap().1);
        assert_eq!(parse_double("%[red]"), red);
    }

    #[test]
    fn var_resolution_and_unescape() {
        let mut asm = Assembler::new();
        let null = arena_var(&mut asm, "null");
        assert!(null.is_obj && null.obj.is_none());
        let number = arena_var(&mut asm, "5");
        assert!(!number.is_obj && number.num == 5.0);
        let s = arena_var(&mut asm, "\"line1\\nline2\"");
        assert_eq!(s.obj, Some(LogicObject::Str("line1\nline2".to_owned())));
        let v = arena_var(&mut asm, "abc");
        assert!(v.is_obj && v.obj.is_none());
    }

    #[test]
    fn empty_string_const() {
        let mut asm = Assembler::new();
        let v = arena_var(&mut asm, "\"\"");
        assert!(v.is_obj);
        assert_eq!(v.obj, Some(LogicObject::Str(String::new())));
    }

    #[test]
    fn assemble_set_op_jump() {
        let asm = Assembler::assemble("set result 5\nop add result result 2\n", true).unwrap();
        assert_eq!(asm.instructions.len(), 2);
        assert!(matches!(asm.instructions[0], Instruction::Set { .. }));
        assert!(matches!(asm.instructions[1], Instruction::Op { .. }));
    }
}

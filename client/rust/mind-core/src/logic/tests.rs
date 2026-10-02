// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Ported `Mindustry/tests/src/test/java/LogicTests.java` groups.
//!
//! These run on a minimal `mind-core` registry fixture (no Godot/JVM):
//! `GlobalVars` static constants + the parser/assembler/executor.

use crate::logic::assembler::Assembler;
use crate::logic::executor::Executor;
use crate::logic::statement::Statement;
use crate::logic::value::{LogicObject, VarRef};

/// Assembles and loads a program, mirroring the `LogicBlock` usage pattern.
fn load(code: &str) -> Executor {
    let mut exec = Executor::new();
    exec.privileged = true;
    exec.load(Assembler::assemble(code, true).expect("assemble"));
    exec
}

/// Runs a single `set result <value>` and returns the decoded `from` value.
fn set_from_value(code: &str) -> LogicObject {
    let exec = load(code);
    assert!(
        !exec.instructions.is_empty(),
        "expected at least one instruction from: {code}"
    );
    match &exec.instructions[0] {
        crate::logic::executor::Instruction::Set { from, .. } => {
            let var = exec.arena.get(from.id());
            assert!(var.is_obj, "expected an object value from: {code}");
            var.obj.clone().expect("object present")
        }
        other => panic!("expected a set instruction, got {other:?}"),
    }
}

fn var(asm: &mut Assembler, symbol: &str) -> crate::logic::value::LVar {
    let VarRef::Local(id) = asm.var(symbol);
    asm.arena.get(id).clone()
}

#[test]
fn string_escapes() {
    let cases: &[(&str, &str)] = &[
        (r#"set result "asdf""#, "asdf"),
        (r#"set result "line1\nline2""#, "line1\nline2"),
        (
            r#"set result "the entity said \"hi\"""#,
            "the entity said \"hi\"",
        ),
        (r#"set result "a\\b""#, "a\\b"),
        (r#"set result "a\\nb""#, "a\\nb"),
        (r#"set result "tab\ttab""#, "tab\\ttab"),
        (
            r#"set result "start\nmid\"quoted\"\\end""#,
            "start\nmid\"quoted\"\\end",
        ),
        (r#"set result """#, ""),
        (r#"set result "end\\""#, "end\\"),
        (r#"set result "\u0041""#, "A"),
        (r#"set result "\uf8ff\uf8ff""#, "\u{f8ff}\u{f8ff}"),
        (r#"set result "\u0041\n\u0042\\end""#, "A\nB\\end"),
    ];
    for (code, expected) in cases {
        match set_from_value(code) {
            LogicObject::Str(s) => assert_eq!(&s, expected, "case: {code}"),
            other => panic!("expected string, got {other:?} for {code}"),
        }
    }
}

#[test]
fn plain_number_is_not_a_string() {
    let exec = load("set result 5");
    match &exec.instructions[0] {
        crate::logic::executor::Instruction::Set { from, .. } => {
            let v = exec.arena.get(from.id());
            assert!(!v.is_obj);
            assert_eq!(v.num, 5.0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn sanitized_quoted_values_roundtrip() {
    let cases: &[(&str, &str)] = &[
        ("\"hello\"", "hello"),
        ("\"a\"b\"", "a\"b"),
        ("\"C:\\Users\"", "C:\\Users"),
        ("\"line1\nline2\"", "line1\nline2"),
        ("\"a\\nb\\nc\"", "a\nb\nc"),
        ("\"a\\b\"c\nd\"", "a\\b\"c\nd"),
        ("\"say \\\"hi\\\"\"", "say \"hi\""),
        ("\"a\\\\b\"", "a\\b"),
        ("\"C:\\Users\"", "C:\\Users"),
        ("\"end\\\"", "end\\"),
    ];
    for (input, expected) in cases {
        let sanitized = Statement::sanitize(input);
        assert!(
            sanitized.len() >= 2 && sanitized.starts_with('"') && sanitized.ends_with('"'),
            "expected quoted value, got {sanitized}"
        );
        let decoded = set_from_value(&format!("set result {sanitized}"));
        match decoded {
            LogicObject::Str(s) => assert_eq!(&s, expected, "input: {input}"),
            other => panic!("expected string, got {other:?} for {input}"),
        }
        let text = format!("set result {sanitized}\n");
        let stmts = Assembler::read(&text, true).unwrap();
        assert_eq!(Assembler::write(&stmts), text);
    }
}

#[test]
#[allow(clippy::approx_constant)]
fn parse_var_values() {
    let numeric: &[(&str, f64)] = &[
        ("0", 0.0),
        ("42", 42.0),
        ("-42", -42.0),
        ("+42", 42.0),
        ("3.14", 3.14),
        ("-3.14", -3.14),
        (".5", 0.5),
        ("123456789012", 123456789012.0),
        ("1e3", 1000.0),
        ("1.5e-2", 0.015),
        ("0x0", 0.0),
        ("0xFF", 255.0),
        ("0xff", 255.0),
        ("0xDeadBeef", 3735928559.0),
        ("+0xFF", 255.0),
        ("-0xFF", -255.0),
        ("0x0000000000000000000000FF", 255.0),
        ("0x7FFFFFFFFFFFFFFF", i64::MAX as f64),
        ("0x8000000000000000", i64::MIN as f64),
        ("0xFFFFFFFFFFFFFFFF", -1.0),
        ("0xFFFFFFFFFFFFFFFE", -2.0),
        ("-0xFFFFFFFFFFFFFFFF", 1.0),
        ("-0x7FFFFFFFFFFFFFFF", -(i64::MAX as f64)),
        ("0b0", 0.0),
        ("0b1", 1.0),
        ("0b1010", 10.0),
        ("+0b1010", 10.0),
        ("-0b1010", -10.0),
    ];
    let mut asm = Assembler::new();
    for (symbol, expected) in numeric {
        let v = var(&mut asm, symbol);
        assert!(!v.is_obj, "should be numeric: {symbol}");
        assert!(
            (v.num - expected).abs() < 0.00001,
            "{symbol}: got {}, expected {expected}",
            v.num
        );
    }

    let v = var(&mut asm, "null");
    assert!(v.is_obj);
    assert!(v.obj.is_none());
    // many leading zeros are not overflow
    let v = var(&mut asm, "0x0000000000000000000000FF");
    assert_eq!(v.num, 255.0);
    let v = var(
        &mut asm,
        "0b00000000000000000000000000000000000000000000000000000000000000000000001",
    );
    assert_eq!(v.num, 1.0);
}

#[test]
fn parse_color_values() {
    let mut asm = Assembler::new();
    let expected_white = crate::logic::globals::rgba_to_double_bits((255, 255, 255, 255));
    let expected_red = crate::logic::globals::rgba_to_double_bits((229, 84, 84, 255));
    assert_eq!(var(&mut asm, "%ffffff").num, expected_white);
    assert_eq!(
        var(&mut asm, "%ff000080").num,
        crate::logic::globals::rgba_to_double_bits((255, 0, 0, 128))
    );
    assert_eq!(var(&mut asm, "%[red]").num, expected_red);
}

#[test]
fn parse_invalid_numbers() {
    let cases = [
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
    ];
    let mut asm = Assembler::new();
    for symbol in cases {
        let v = var(&mut asm, symbol);
        assert!(v.is_obj, "should not parse as number: {symbol}");
        assert!(v.obj.is_none());
    }
    for symbol in [
        "0x10000000000000000",
        "-0x10000000000000000",
        "0xFFFFFFFFFFFFFFFFF",
        "0b10000000000000000000000000000000000000000000000000000000000000000",
        "0b11111111111111111111111111111111111111111111111111111111111111111",
    ] {
        let v = var(&mut asm, symbol);
        assert!(v.is_obj, "overflow should not parse: {symbol}");
    }
}

#[test]
fn unterminated_strings_throw() {
    for code in [
        "set result \"asd\\\"",
        "set result \"asd\\",
        "set result \"asd\\\"; set bar 5",
        "set result \"asd",
        "set result \"asd\nset bar 5",
        "set result \"\\u12zz\"",
        "set result \"\\u1h23\"",
        "set result \"\\uhh23\"",
        "set result \"\\u12\"",
    ] {
        assert!(
            Assembler::assemble(code, true).is_err(),
            "expected parse error: {code}"
        );
    }
}

#[test]
fn quote_in_variable_name_throws() {
    assert!(Assembler::assemble("set result abc\"def", true).is_err());
    assert!(Assembler::assemble("set result abcdef\"", true).is_err());
}

#[test]
fn crlf_does_not_corrupt_tokens() {
    let exec = load("set a bar\r\nset b bar\n");
    let first = match &exec.instructions[0] {
        crate::logic::executor::Instruction::Set { from, .. } => {
            exec.arena.get(from.id()).name.clone()
        }
        other => panic!("{other:?}"),
    };
    let second = match &exec.instructions[1] {
        crate::logic::executor::Instruction::Set { from, .. } => {
            exec.arena.get(from.id()).name.clone()
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(first, second);
    assert_eq!(
        set_from_value("set result \"bar\"\r\n"),
        LogicObject::Str("bar".into())
    );
    let exec = load("set result 1\rset result2 2\r");
    assert_eq!(exec.instructions.len(), 2);
    assert!(Assembler::assemble("loop:\r\njump loop always\r\n", true).is_ok());
}

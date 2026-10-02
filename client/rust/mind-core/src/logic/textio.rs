// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Text (de)serialization equivalent of generated `mindustry.gen.LogicIO`.
//!
//! The annotation processor is replaced by a hand-written field-order table
//! (deviation 8, plan 13 §2.4). Field order is ABI: one token per field after
//! the statement name, enums by Java `name()`, bools as `true`/`false`.

pub use crate::logic::assembler::Assembler;
pub use crate::logic::statement::{Statement, StatementMeta, all_statements, read_statement};

/// Maximum statements a canvas/assembler accepts (`LExecutor.maxInstructions`).
pub const MAX_INSTRUCTIONS: usize = crate::logic::executor::MAX_INSTRUCTIONS;

/// Writes one statement (name + fields, no trailing newline).
pub fn write_statement(statement: &Statement, out: &mut String) {
    statement.write(out);
}

/// Reads one statement from whitespace-split tokens.
pub fn read_statement_tokens(tokens: &[&str]) -> Option<Statement> {
    read_statement(tokens)
}

/// All registered statement metadata (plan-14 add dialog).
pub fn statements() -> Vec<StatementMeta> {
    all_statements()
}

/// `LCanvas.save`: statements → text.
pub fn canvas_save(statements: &[Statement]) -> String {
    Assembler::write(statements)
}

/// `LCanvas.load`: text → statements, truncated to `MAX_INSTRUCTIONS`.
pub fn canvas_load(
    text: &str,
    privileged: bool,
) -> Result<Vec<Statement>, crate::logic::parser::ParseError> {
    let mut statements = Assembler::read(text, privileged)?;
    statements.truncate(MAX_INSTRUCTIONS);
    Ok(statements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_statements_roundtrip() {
        for meta in all_statements() {
            let st = crate::logic::statement::default_statement(meta.registered_name)
                .unwrap_or_else(|| panic!("missing default for {}", meta.registered_name));

            let mut first = String::new();
            write_statement(&st, &mut first);

            let tokens: Vec<&str> = first.split(' ').collect();
            let read =
                read_statement(&tokens).unwrap_or_else(|| panic!("failed to read back: {first}"));
            assert_eq!(read, st, "roundtrip mismatch for {}", meta.registered_name);

            let mut second = String::new();
            write_statement(&read, &mut second);
            assert_eq!(
                first, second,
                "write/read/write mismatch for {}",
                meta.registered_name
            );
        }
    }

    #[test]
    fn missing_trailing_fields_keep_defaults() {
        let st = read_statement(&["read", "out"]).unwrap();
        assert_eq!(
            st,
            Statement::Read {
                output: "out".to_owned(),
                target: "cell1".to_owned(),
                address: "0".to_owned()
            }
        );
    }

    #[test]
    fn extra_tokens_are_ignored() {
        let st = read_statement(&["set", "a", "1", "extra", "more"]).unwrap();
        assert_eq!(
            st,
            Statement::Set {
                to: "a".to_owned(),
                from: "1".to_owned()
            }
        );
    }

    #[test]
    fn unknown_name_returns_none() {
        assert!(read_statement(&["not-a-statement"]).is_none());
    }

    #[test]
    fn draw_after_read_applies_alpha_and_align_fixups() {
        let st = read_statement(&["draw", "color", "0", "0", "0", "0", "0", "0"]).unwrap();
        match st {
            Statement::Draw { p2, .. } => assert_eq!(p2, "255"),
            other => panic!("{other:?}"),
        }
        let st = read_statement(&["draw", "print", "0", "0", "top", "0", "0", "0"]).unwrap();
        match st {
            Statement::Draw { p1, .. } => assert_eq!(p1, "@top"),
            other => panic!("{other:?}"),
        }
    }
}

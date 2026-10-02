// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Text-language lexer/parser.
//!
//! Ported from `core/src/mindustry/logic/LParser.java`: CR/CRLF normalization,
//! the 16-token line grammar, `#` comments, raw string literals with validated
//! `\uXXXX` escapes, `;` separators, jump labels, legacy op renames, `status`
//! effect prefixing, `@configure`/`configure` rewrites and the 1000-statement cap.

use crate::logic::statement::{Statement, read_statement};

/// Maximum tokens on a single line (`LParser.tokens.length`).
pub const MAX_TOKENS: usize = 16;
/// Maximum jump labels (`LParser.maxJumps`).
pub const MAX_JUMPS: usize = 500;
/// Maximum parsed statements (`LExecutor.maxInstructions`).
pub const MAX_INSTRUCTIONS: usize = 1000;

/// Parser error (`RuntimeException("Invalid code. " + message)`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Invalid code. {0}")]
pub struct ParseError(pub String);

/// `LParser` port.
pub struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    privileged: bool,
    /// Optional status-effect existence check (upstream `Vars.content.statusEffect`).
    has_status: Option<&'a dyn Fn(&str) -> bool>,
    statements: Vec<Statement>,
    labels: Vec<(String, usize)>,
    jumps: Vec<(usize, String)>,
}

impl<'a> Parser<'a> {
    /// Creates a parser over `text`.
    pub fn new(text: &str, privileged: bool) -> Self {
        let chars: Vec<char> = text
            .chars()
            .map(|c| if c == '\r' { '\n' } else { c })
            .collect();
        Self {
            chars,
            pos: 0,
            line: 0,
            privileged,
            has_status: None,
            statements: Vec::new(),
            labels: Vec::new(),
            jumps: Vec::new(),
        }
    }

    /// Attaches a status-effect lookup (used by the `status` legacy rewrite).
    pub fn with_status_lookup(mut self, lookup: &'a dyn Fn(&str) -> bool) -> Self {
        self.has_status = Some(lookup);
        self
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError(message.into()))
    }

    fn comment(&mut self) {
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            self.pos += 1;
            if c == '\n' {
                break;
            }
        }
    }

    fn utf16_size(c: i32) -> usize {
        if c != 0 && c < 0x80 {
            1
        } else if c < 0x800 {
            2
        } else {
            3
        }
    }

    /// `LParser.string`: returns the raw literal **including** both quotes.
    fn string(&mut self) -> Result<String, ParseError> {
        let from = self.pos;
        let mut utflen = 0usize;

        loop {
            self.pos += 1;
            if self.pos >= self.chars.len() {
                break;
            }
            let c = self.chars[self.pos];

            if c == '\\' && self.pos + 1 < self.chars.len() {
                let next = self.chars[self.pos + 1];
                if next == 'n' || next == '"' || next == '\\' {
                    utflen += Self::utf16_size(next as i32);
                    self.pos += 1;
                    continue;
                }
                if next == 'u' {
                    if self.pos + 5 >= self.chars.len() {
                        return self.error("Invalid \\u escape; expected 4 hex digits.");
                    }
                    let mut value: i32 = 0;
                    for j in self.pos + 2..=self.pos + 5 {
                        let digit = self.chars[j].to_digit(16).map(|d| d as i32).unwrap_or(-1);
                        value = value << 4 | digit;
                    }
                    if value < 0 {
                        return self.error("Invalid \\u escape; expected 4 hex digits.");
                    }
                    utflen += Self::utf16_size(value);
                    self.pos += 5;
                    continue;
                }
            }

            if c == '\n' {
                return self.error("Missing closing quote \" before end of line.");
            } else if c == '"' {
                break;
            }

            utflen += Self::utf16_size(c as i32);
        }

        if self.pos >= self.chars.len() || self.chars[self.pos] != '"' {
            return self.error("Missing closing quote \" before end of file.");
        }
        if utflen > 65535 {
            return self.error("String value too long.");
        }

        self.pos += 1;
        Ok(self.chars[from..self.pos].iter().collect())
    }

    fn token(&mut self) -> String {
        let from = self.pos;
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            if matches!(c, '\n' | ' ' | '#' | '\t' | ';' | '"') {
                break;
            }
            self.pos += 1;
        }
        self.chars[from..self.pos].iter().collect()
    }

    /// `LParser.checkRead`.
    fn check_read(&self, tokens: &mut [String]) {
        if tokens[0] == "op" && tokens.len() > 1 {
            tokens[1] = match tokens[1].as_str() {
                "atan2" => "angle".to_owned(),
                "dst" => "len".to_owned(),
                other => other.to_owned(),
            };
        }
        if tokens[0] == "status" && tokens.len() > 1 {
            let is_status = self.has_status.map(|f| f(&tokens[1])).unwrap_or(false);
            if is_status {
                tokens[1] = format!("@status-{}", tokens[1]);
            }
        }
    }

    /// `LParser.statement`.
    fn statement(&mut self) -> Result<(), ParseError> {
        let mut tokens: Vec<String> = Vec::with_capacity(MAX_TOKENS);
        let mut expect_next = false;

        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];

            if tokens.len() >= MAX_TOKENS {
                return self.error(format!(
                    "Line too long; may only contain {MAX_TOKENS} tokens"
                ));
            }
            if c == '\n' || c == ';' {
                break;
            }
            if expect_next && c != ' ' && c != '#' && c != '\t' {
                return self.error("Expected space after string/token.");
            }
            expect_next = false;

            if c == '#' {
                self.comment();
                break;
            } else if c == '"' {
                tokens.push(self.string()?);
                expect_next = true;
            } else if c != ' ' && c != '\t' {
                tokens.push(self.token());
                expect_next = true;
            } else {
                self.pos += 1;
            }
        }

        if tokens.is_empty() {
            return Ok(());
        }

        self.check_read(&mut tokens);

        // Jump location, always ends with a colon.
        if tokens.len() == 1 && tokens[0].ends_with(':') {
            if self.labels.len() >= MAX_JUMPS {
                return self.error(format!("Too many jump locations. Max jumps: {MAX_JUMPS}"));
            }
            let label = tokens[0][..tokens[0].len() - 1].to_owned();
            if self.labels.iter().any(|(name, _)| *name == label) {
                return self.error(format!("Jump label already defined: \"{label}\"."));
            }
            self.labels.push((label, self.line));
            return Ok(());
        }

        let mut was_jump = false;
        let mut jump_loc: Option<String> = None;
        if tokens[0] == "jump" && tokens.len() > 1 && !can_parse_int(&tokens[1]) {
            was_jump = true;
            jump_loc = Some(tokens[1].clone());
            tokens[1] = "-1".to_owned();
        }

        for token in tokens.iter_mut().skip(1) {
            if token == "@configure" {
                *token = "@config".to_owned();
            } else if token == "configure" {
                *token = "config".to_owned();
            }
        }

        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        let mut st = read_statement(&refs).unwrap_or(Statement::Invalid {});

        if !self.privileged && st.privileged() {
            st = Statement::Invalid {};
        }

        if was_jump {
            let index = self.statements.len();
            self.jumps.push((index, jump_loc.unwrap_or_default()));
        }

        self.statements.push(st);
        self.line += 1;
        Ok(())
    }

    /// Parses the whole program.
    pub fn parse(mut self) -> Result<Vec<Statement>, ParseError> {
        while self.pos < self.chars.len() && self.line < MAX_INSTRUCTIONS {
            match self.chars[self.pos] {
                '\n' | ';' | ' ' => self.pos += 1,
                _ => self.statement()?,
            }
        }

        for (index, location) in &self.jumps {
            let dest = self
                .labels
                .iter()
                .find(|(name, _)| name == location)
                .map(|(_, line)| *line as i32)
                .ok_or_else(|| {
                    ParseError(format!(
                        "Undefined jump location: \"{location}\". Make sure the jump label exists and is typed correctly."
                    ))
                })?;
            if let Statement::Jump { dest_index, .. } = &mut self.statements[*index] {
                *dest_index = dest;
            }
        }

        Ok(self.statements)
    }
}

/// `Strings.canParseInt`.
fn can_parse_int(s: &str) -> bool {
    !s.is_empty() && s.parse::<i32>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Vec<Statement> {
        Parser::new(text, true).parse().unwrap()
    }

    #[test]
    fn normalizes_crlf_and_lone_cr() {
        assert_eq!(parse("set a bar\r\nset b bar\n").len(), 2);
        assert_eq!(parse("set result 1\rset result2 2\r").len(), 2);
        assert_eq!(parse("loop:\r\njump loop always\r\n").len(), 1);
    }

    #[test]
    fn jump_label_resolves() {
        let st = parse("loop:\njump loop always\n");
        match &st[0] {
            Statement::Jump { dest_index, .. } => assert_eq!(*dest_index, 0),
            other => panic!("expected jump, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_and_undefined_labels_error() {
        assert!(Parser::new("a:\na:\n", true).parse().is_err());
        assert!(Parser::new("jump nope always\n", true).parse().is_err());
    }

    #[test]
    fn string_keeps_raw_escapes_and_strips_quotes_only_via_var() {
        let st = parse("set result \"a\\nb\"");
        match &st[0] {
            Statement::Set { from, .. } => assert_eq!(from, "\"a\\nb\""),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unterminated_strings_error() {
        for code in [
            "set result \"asd\\\"",
            "set result \"asd\\",
            "set result \"asd",
            "set result \"asd\nset bar 5",
            "set result \"\\u12zz\"",
        ] {
            assert!(
                Parser::new(code, true).parse().is_err(),
                "should fail: {code}"
            );
        }
    }

    #[test]
    fn quote_inside_token_errors() {
        assert!(Parser::new("set result abc\"def", true).parse().is_err());
        assert!(Parser::new("set result abcdef\"", true).parse().is_err());
    }

    #[test]
    fn op_legacy_rewrite() {
        let st = parse("op atan2 result a b");
        match &st[0] {
            Statement::Operation { op, .. } => assert_eq!(op.name(), "angle"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn privileged_filtering() {
        assert!(matches!(
            parse("setblock block @air 0 0")[0],
            Statement::SetBlock { .. }
        ));
        let non = Parser::new("setblock block @air 0 0", false)
            .parse()
            .unwrap();
        assert_eq!(non[0], Statement::Invalid {});
    }
}

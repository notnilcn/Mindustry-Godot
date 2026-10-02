// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `pack.json` semantics from Arc (Apache-2.0) `TexturePacker.Settings` +
// `TexturePackerFileProcessor` (inheritance + `combineSubdirectories`).

//! `pack.json` loading and inheritance (plan 03 §6.2).
//!
//! The upstream files are lenient JSON (unquoted keys); this module parses the
//! subset used by `assets-raw/sprites/**/pack.json` and resolves the
//! closest-parent inheritance chain.

use crate::error::{AtlasError, Result};

/// Packer settings with Arc `TexturePacker.Settings` defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct PackSettings {
    /// Pad each region on all sides (upstream default true via pack.json).
    pub duplicate_padding: bool,
    /// Subfolders without their own `pack.json` merge into the parent pass.
    pub combine_subdirectories: bool,
    /// Region names have no directories; duplicates are an error.
    pub flatten_paths: bool,
    /// Page caps.
    pub max_width: i32,
    /// Page caps.
    pub max_height: i32,
    /// `fast` packer heuristic (ported deterministically).
    pub fast: bool,
    /// Keep the region rectangle centered when stripping whitespace.
    pub strip_whitespace_center: bool,
    /// Strip whitespace (X axis) — off in every upstream config.
    pub strip_whitespace_x: bool,
    /// Strip whitespace (Y axis) — off in every upstream config.
    pub strip_whitespace_y: bool,
    /// Paths excluded from center stripping (`["effects/"]` at root).
    pub ignored_whitespace_strings: Vec<String>,
    /// Alpha bleed on finished pages.
    pub bleed: bool,
    /// Bleed iterations (Arc default 2).
    pub bleed_iterations: usize,
    /// Inter-region padding.
    pub padding_x: i32,
    /// Inter-region padding.
    pub padding_y: i32,
    /// Pad page edges.
    pub edge_padding: bool,
    /// Power-of-two page sizes.
    pub pot: bool,
    /// Minimum page size.
    pub min_width: i32,
    /// Minimum page size.
    pub min_height: i32,
    /// Deduplicate identical images as aliases.
    pub alias: bool,
    /// Drop fully-transparent images.
    pub ignore_blank_images: bool,
    /// Allow 90° rotation (off everywhere upstream).
    pub rotation: bool,
    /// Alpha threshold for whitespace stripping.
    pub alpha_threshold: i32,
    /// Force page sizes to multiples of four.
    pub multiple_of_four: bool,
    /// Square pages only.
    pub square: bool,
    /// Skip this directory entirely.
    pub ignore: bool,
}

impl Default for PackSettings {
    fn default() -> Self {
        Self {
            duplicate_padding: false,
            combine_subdirectories: false,
            flatten_paths: false,
            max_width: 1024,
            max_height: 1024,
            fast: true,
            strip_whitespace_center: false,
            strip_whitespace_x: false,
            strip_whitespace_y: false,
            ignored_whitespace_strings: Vec::new(),
            bleed: true,
            bleed_iterations: 2,
            padding_x: 2,
            padding_y: 2,
            edge_padding: true,
            pot: true,
            min_width: 16,
            min_height: 16,
            alias: true,
            ignore_blank_images: true,
            rotation: false,
            alpha_threshold: 0,
            multiple_of_four: false,
            square: false,
            ignore: false,
        }
    }
}

/// Lenient JSON value (the `pack.json` subset).
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Object(Vec<(String, Value)>),
    Array(Vec<Value>),
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

impl PackSettings {
    /// Parses a `pack.json` file (lenient: unquoted keys, `:` or `=`).
    pub fn parse(text: &str) -> Result<(PackSettings, Vec<String>)> {
        let mut parser = Parser {
            chars: text.chars().collect(),
            pos: 0,
        };
        let value = parser.parse_value()?;
        parser.skip_ws();
        if parser.pos != parser.chars.len() {
            return Err(AtlasError::Invalid(format!(
                "trailing content in pack.json at byte {}",
                parser.pos
            )));
        }
        let mut settings = PackSettings::default();
        let mut warnings = Vec::new();
        if let Value::Object(fields) = value {
            for (key, value) in fields {
                settings.apply(&key, &value, &mut warnings)?;
            }
        } else {
            return Err(AtlasError::Invalid(
                "pack.json root must be an object".into(),
            ));
        }
        Ok((settings, warnings))
    }

    /// Applies one key (Arc `Json.readFields` semantics: unknown keys ignored
    /// with a warning).
    fn apply(&mut self, key: &str, value: &Value, warnings: &mut Vec<String>) -> Result<()> {
        fn boolean(value: &Value, key: &str) -> Result<bool> {
            match value {
                Value::Bool(b) => Ok(*b),
                _ => Err(AtlasError::Invalid(format!(
                    "pack.json `{key}` must be a bool"
                ))),
            }
        }
        fn number(value: &Value, key: &str) -> Result<i32> {
            match value {
                Value::Number(n) => Ok(*n as i32),
                _ => Err(AtlasError::Invalid(format!(
                    "pack.json `{key}` must be a number"
                ))),
            }
        }
        match key {
            "duplicatePadding" => self.duplicate_padding = boolean(value, key)?,
            "combineSubdirectories" => self.combine_subdirectories = boolean(value, key)?,
            "flattenPaths" => self.flatten_paths = boolean(value, key)?,
            "maxWidth" => self.max_width = number(value, key)?,
            "maxHeight" => self.max_height = number(value, key)?,
            "fast" => self.fast = boolean(value, key)?,
            "stripWhitespaceCenter" => self.strip_whitespace_center = boolean(value, key)?,
            "stripWhitespaceX" => self.strip_whitespace_x = boolean(value, key)?,
            "stripWhitespaceY" => self.strip_whitespace_y = boolean(value, key)?,
            "bleed" => self.bleed = boolean(value, key)?,
            "bleedIterations" => self.bleed_iterations = number(value, key)? as usize,
            "paddingX" => self.padding_x = number(value, key)?,
            "paddingY" => self.padding_y = number(value, key)?,
            "edgePadding" => self.edge_padding = boolean(value, key)?,
            "pot" => self.pot = boolean(value, key)?,
            "minWidth" => self.min_width = number(value, key)?,
            "minHeight" => self.min_height = number(value, key)?,
            "alias" => self.alias = boolean(value, key)?,
            "ignoreBlankImages" => self.ignore_blank_images = boolean(value, key)?,
            "rotation" => self.rotation = boolean(value, key)?,
            "alphaThreshold" => self.alpha_threshold = number(value, key)?,
            "multipleOfFour" => self.multiple_of_four = boolean(value, key)?,
            "square" => self.square = boolean(value, key)?,
            "ignore" => self.ignore = boolean(value, key)?,
            "ignoredWhitespaceStrings" => match value {
                Value::Array(items) => {
                    self.ignored_whitespace_strings = items
                        .iter()
                        .map(|item| match item {
                            Value::String(s) => Ok(s.clone()),
                            _ => Err(AtlasError::Invalid(
                                "pack.json `ignoredWhitespaceStrings` must be strings".into(),
                            )),
                        })
                        .collect::<Result<Vec<_>>>()?;
                }
                _ => {
                    return Err(AtlasError::Invalid(
                        "pack.json `ignoredWhitespaceStrings` must be an array".into(),
                    ));
                }
            },
            // Arc-only keys accepted and ignored (single-scale, Godot-owned
            // filtering, or replaced by the .atlas.json manifest per A1).
            "scale" | "scaleSuffix" | "scaleResampling" | "atlasExtension" | "filterMin"
            | "filterMag" | "wrapX" | "wrapY" | "grid" | "silent" | "printAliases" => {}
            other => warnings.push(format!("pack.json: unknown key `{other}` ignored")),
        }
        Ok(())
    }

    /// Merges another parsed config over `self` (child overrides parent),
    /// Arc `Settings.copy()` + `readFields` semantics: only keys present in
    /// `text` change values.
    pub fn merged_with(&self, text: &str) -> Result<(PackSettings, Vec<String>)> {
        let value = parse_object(text)?;
        let mut merged = self.clone();
        let mut warnings = Vec::new();
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    merged.apply(&key, &value, &mut warnings)?;
                }
            }
            _ => {
                return Err(AtlasError::Invalid(
                    "pack.json root must be an object".into(),
                ));
            }
        }
        Ok((merged, warnings))
    }
}

fn parse_object(text: &str) -> Result<Value> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        pos: 0,
    };
    parser.parse_value()
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() {
            match self.chars[self.pos] {
                c if c.is_whitespace() => self.pos += 1,
                '/' if self.chars.get(self.pos + 1) == Some(&'/') => {
                    while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn parse_value(&mut self) -> Result<Value> {
        self.skip_ws();
        match self.peek() {
            Some('{') => {
                self.pos += 1;
                let mut fields = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some('}') {
                        self.pos += 1;
                        break;
                    }
                    let key = self.parse_key()?;
                    self.skip_ws();
                    match self.peek() {
                        Some(':') | Some('=') => self.pos += 1,
                        other => {
                            return Err(AtlasError::Invalid(format!(
                                "pack.json: expected `:` after key, got {other:?}"
                            )));
                        }
                    }
                    let value = self.parse_value()?;
                    fields.push((key, value));
                    self.skip_ws();
                    match self.peek() {
                        Some(',') => self.pos += 1,
                        Some('}') => {
                            self.pos += 1;
                            break;
                        }
                        other => {
                            return Err(AtlasError::Invalid(format!(
                                "pack.json: expected `,` or `}}`, got {other:?}"
                            )));
                        }
                    }
                }
                Ok(Value::Object(fields))
            }
            Some('[') => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some(']') {
                        self.pos += 1;
                        break;
                    }
                    items.push(self.parse_value()?);
                    self.skip_ws();
                    match self.peek() {
                        Some(',') => self.pos += 1,
                        Some(']') => {
                            self.pos += 1;
                            break;
                        }
                        other => {
                            return Err(AtlasError::Invalid(format!(
                                "pack.json: expected `,` or `]`, got {other:?}"
                            )));
                        }
                    }
                }
                Ok(Value::Array(items))
            }
            Some('"') => {
                self.pos += 1;
                let mut out = String::new();
                while let Some(c) = self.peek() {
                    self.pos += 1;
                    match c {
                        '"' => break,
                        '\\' => {
                            if let Some(escaped) = self.peek() {
                                out.push(escaped);
                                self.pos += 1;
                            }
                        }
                        other => out.push(other),
                    }
                }
                Ok(Value::String(out))
            }
            Some(c) if c.is_ascii_digit() || c == '-' || c == '.' => {
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E') {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                let text: String = self.chars[start..self.pos].iter().collect();
                text.parse::<f64>()
                    .map(Value::Number)
                    .map_err(|error| AtlasError::Invalid(format!("pack.json number: {error}")))
            }
            Some(_) => {
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if c.is_alphabetic() {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                let text: String = self.chars[start..self.pos].iter().collect();
                match text.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "null" => Ok(Value::Null),
                    other => Ok(Value::String(other.to_owned())),
                }
            }
            None => Err(AtlasError::Invalid("pack.json: unexpected eof".into())),
        }
    }

    fn parse_key(&mut self) -> Result<String> {
        self.skip_ws();
        match self.peek() {
            Some('"') => match self.parse_value()? {
                Value::String(s) => Ok(s),
                _ => unreachable!(),
            },
            Some(c) if c.is_alphabetic() || c == '_' => {
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                Ok(self.chars[start..self.pos].iter().collect())
            }
            other => Err(AtlasError::Invalid(format!(
                "pack.json: expected key, got {other:?}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = r#"{
  duplicatePadding: true,
  combineSubdirectories: true,
  flattenPaths: true,
  maxWidth: 4096,
  maxHeight: 4096,
  fast: true,
  stripWhitespaceCenter: true,
  ignoredWhitespaceStrings: ["effects/"]
}"#;

    #[test]
    fn parses_upstream_root_pack_json() {
        let (settings, warnings) = PackSettings::parse(ROOT).unwrap();
        assert!(warnings.is_empty());
        assert!(settings.duplicate_padding);
        assert!(settings.combine_subdirectories);
        assert!(settings.flatten_paths);
        assert_eq!(settings.max_width, 4096);
        assert!(settings.fast);
        assert!(settings.strip_whitespace_center);
        assert_eq!(settings.ignored_whitespace_strings, vec!["effects/"]);
        // Arc defaults survive for unset keys.
        assert!(settings.pot);
        assert!(settings.alias);
        assert!(!settings.rotation);
    }

    #[test]
    fn child_overrides_parent() {
        let (root, _) = PackSettings::parse(ROOT).unwrap();
        let (child, _) = root
            .merged_with("{ maxWidth: 2048, maxHeight: 2048, stripWhitespaceCenter: false }")
            .unwrap();
        assert_eq!(child.max_width, 2048);
        assert!(!child.strip_whitespace_center);
        assert!(child.duplicate_padding);
        assert_eq!(child.ignored_whitespace_strings, vec!["effects/"]);
    }
}

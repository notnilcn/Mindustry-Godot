// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: `core/assets/icons/icons.properties` (`<code>=<content>|<texture>`),
//         `core/assets-raw/fontgen/config.json` (`icon_codes.json`),
//         `core/src/mindustry/ui/Fonts.java` (`registerIcon`/`getUnicode`/
//         `getUnicodeStr` equivalents), `core/src/mindustry/ui/Icon.java`.

//! `Iconc`/`Icon` code tables (plan 03 §3.3/M7).
//!
//! The append-only `icons.properties` codes are the parity ABI. `Iconc` maps
//! content name → PUA char (and back) plus the generated UI texture name;
//! `IconCodes` maps the fontgen glyph names → chars. Font loading itself lives
//! in `mind-gdext`; this module is pure data (no filesystem, no Godot).

use std::collections::HashMap;

use serde::Deserialize;

/// `icons.properties` code table (`Iconc` equivalent).
#[derive(Debug, Clone, Default)]
pub struct Iconc {
    by_name: HashMap<String, u32>,
    by_code: HashMap<u32, String>,
    textures: HashMap<String, String>,
    all: String,
}

impl Iconc {
    /// Parses `icons.properties` (`<code>=<contentName>|<textureName>`).
    pub fn from_properties(text: &str) -> Self {
        let mut table = Iconc::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((code, rest)) = line.split_once('=') else {
                continue;
            };
            let Ok(code) = code.trim().parse::<u32>() else {
                continue;
            };
            let (name, texture) = rest.split_once('|').unwrap_or((rest, ""));
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            table.by_name.insert(name.to_owned(), code);
            table.by_code.insert(code, name.to_owned());
            table
                .textures
                .insert(name.to_owned(), texture.trim().to_owned());
            if let Some(ch) = char::from_u32(code) {
                table.all.push(ch);
            }
        }
        table
    }

    /// `Iconc` content name → char.
    pub fn get(&self, name: &str) -> Option<char> {
        self.by_name
            .get(name)
            .and_then(|code| char::from_u32(*code))
    }

    /// Raw PUA code for a content name.
    pub fn code(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    /// Reverse lookup: PUA code → content name.
    pub fn name(&self, code: u32) -> Option<&str> {
        self.by_code.get(&code).map(String::as_str)
    }

    /// Generated UI texture name (`<type>-<name>-ui`).
    pub fn texture(&self, name: &str) -> Option<&str> {
        self.textures.get(name).map(String::as_str)
    }

    /// `Fonts.getUnicodeStr(name)`: the char as a string.
    pub fn unicode_str(&self, name: &str) -> Option<String> {
        self.get(name).map(|ch| ch.to_string())
    }

    /// All icon chars in file order (the `Iconc.all` string upstream builds).
    pub fn all(&self) -> &str {
        &self.all
    }

    /// Number of icons.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

/// One `fontgen` glyph (`icon_codes.json`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct IconGlyph {
    /// Fontello glyph name (`Add`, `Battery1`).
    pub name: String,
    /// Unicode code point.
    pub code: u32,
    /// CSS class suffix (`add`).
    pub css: String,
}

impl IconGlyph {
    /// The glyph's char, when the code point is a valid scalar value.
    pub fn ch(&self) -> Option<char> {
        char::from_u32(self.code)
    }
}

/// `assets/icons/icon_codes.json` (generated from `fontgen/config.json`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct IconCodes {
    /// Format version.
    pub format: u32,
    /// Glyphs.
    pub glyphs: Vec<IconGlyph>,
}

impl IconCodes {
    /// Parses `icon_codes.json`.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Glyph by name.
    pub fn glyph(&self, name: &str) -> Option<&IconGlyph> {
        self.glyphs.iter().find(|glyph| glyph.name == name)
    }

    /// Glyph char by name.
    pub fn get(&self, name: &str) -> Option<char> {
        self.glyph(name).and_then(IconGlyph::ch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROPERTIES: &str = "63743=spawn|block-spawn-ui\n63742=deepwater|block-deepwater-ui\n";

    #[test]
    fn iconc_round_trips_names_and_codes() {
        let table = Iconc::from_properties(PROPERTIES);
        assert_eq!(table.len(), 2);
        assert_eq!(table.code("spawn"), Some(63743));
        assert_eq!(table.get("spawn").unwrap() as u32, 63743);
        assert_eq!(table.name(63742), Some("deepwater"));
        assert_eq!(table.texture("spawn"), Some("block-spawn-ui"));
        assert_eq!(table.unicode_str("spawn").unwrap().chars().count(), 1);
        assert_eq!(table.all().chars().count(), 2);
        assert_eq!(table.get("missing"), None);
    }

    #[test]
    fn icon_codes_parse() {
        let text = r#"{"format":1,"glyphs":[{"name":"Add","code":59411,"css":"add"}]}"#;
        let codes = IconCodes::from_json(text).unwrap();
        assert_eq!(codes.format, 1);
        assert_eq!(codes.get("Add").unwrap() as u32, 59411);
        assert_eq!(codes.glyph("Add").unwrap().css, "add");
        assert_eq!(codes.get("Nope"), None);
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/ImagePacker.java` (icons.properties writer) and
// `annotations/src/main/java/mindustry/annotations/impl/AssetsProcess.java`
// (Tex/Icon/Iconc code tables).

//! Generated id tables (plan 03 §3.5 stage 8, §5 M3/M4).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use mind_core::content::load::ContentRegistry;
use mind_core::content::registries::blocks::BlockKind;
use serde::Serialize;

/// Highest valid PUA code (`0xF8FF`), the allocation start.
pub const ICON_CODE_START: u32 = 0xF8FF;

/// Result of an `icons.properties` sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconsSyncReport {
    /// Total entries after the sync.
    pub entries: usize,
    /// Newly allocated icons.
    pub added: usize,
    /// Lowest allocated code after the sync (`min(existing)-1`).
    pub next_code: u32,
}

/// Loads `icons.properties` into an ordered `code -> "content|texture"` map.
pub fn load_icons_properties(path: &Path) -> Result<IndexMap<u32, String>> {
    let mut map = IndexMap::new();
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(map),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if let Ok(code) = key.trim().parse::<u32>() {
            map.insert(code, value.to_owned());
        }
    }
    Ok(map)
}

/// `ImagePacker` icons.properties writer: preserves existing lines in order
/// and allocates new content codes descending from `0xF8FF`.
pub fn sync_icons_properties(root: &Path, registry: &ContentRegistry) -> Result<IconsSyncReport> {
    let path = root.join("assets/icons/icons.properties");
    let mut map = load_icons_properties(&path)?;
    let known: Vec<String> = map
        .values()
        .map(|value| value.split('|').next().unwrap_or_default().to_owned())
        .collect();

    let mut min_id = ICON_CODE_START;
    for code in map.keys() {
        min_id = min_id.min(code.saturating_sub(1));
    }

    // Upstream order: blocks, items, liquids, units, statuses. Units are
    // deferred with plan 02 M5 (no unit registry yet); their codes will be
    // appended below the status codes once it lands.
    let blocks = registry
        .blocks()
        .iter()
        .filter(|block| block.kind != BlockKind::ConstructBlock && block.name != "air")
        .map(|block| ("block", block.name.as_str()));
    let items = registry
        .items()
        .iter()
        .map(|item| ("item", item.name.as_str()));
    let liquids = registry
        .liquids()
        .iter()
        .map(|liquid| ("liquid", liquid.name.as_str()));
    let statuses = registry
        .statuses()
        .iter()
        .map(|status| ("status", status.name.as_str()));

    let mut added = 0usize;
    for (type_name, name) in blocks.chain(items).chain(liquids).chain(statuses) {
        if known.iter().any(|known| known == name) {
            continue;
        }
        map.insert(min_id, format!("{name}|{type_name}-{name}-ui"));
        min_id = min_id.saturating_sub(1);
        added += 1;
    }

    let mut out = String::new();
    for (code, value) in &map {
        out.push_str(&format!("{code}={value}\n"));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, out).with_context(|| format!("writing {}", path.display()))?;
    Ok(IconsSyncReport {
        entries: map.len(),
        added,
        next_code: min_id,
    })
}

/// One `Icon` glyph from `fontgen/config.json`.
#[derive(Debug, Clone, Serialize)]
pub struct IconGlyph {
    /// Capitalized CSS name (`AssetsProcess.capitalize`).
    pub name: String,
    /// Unicode code point.
    pub code: u32,
    /// Raw CSS name from the config.
    pub css: String,
}

/// Parses `assets-raw/fontgen/config.json` into `Icon` glyphs (selected,
/// unique names, declaration order).
pub fn load_fontgen_glyphs(root: &Path) -> Result<Vec<IconGlyph>> {
    let path = root.join("assets-raw/fontgen/config.json");
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let glyphs = value
        .get("glyphs")
        .and_then(serde_json::Value::as_array)
        .context("fontgen config missing `glyphs`")?;

    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for glyph in glyphs {
        let selected = glyph
            .get("selected")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        if !selected {
            continue;
        }
        let css = glyph
            .get("css")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let name = capitalize(css);
        if !seen.insert(name.clone()) {
            continue;
        }
        let code = glyph
            .get("code")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        out.push(IconGlyph {
            name,
            code,
            css: css.to_owned(),
        });
    }
    Ok(out)
}

/// `AssetsProcess.capitalize`: uppercase the character after `-`/`_`, which
/// are dropped.
fn capitalize(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut upper_next = false;
    for (i, c) in value.chars().enumerate() {
        if c == '_' || c == '-' {
            upper_next = true;
            continue;
        }
        if upper_next || (i == 0 && c.is_ascii_lowercase()) {
            result.extend(c.to_uppercase());
        } else {
            result.push(c);
        }
        upper_next = false;
    }
    result
}

/// Writes `assets/icons/icon_codes.json` from the fontgen config.
pub fn write_icon_codes(root: &Path) -> Result<usize> {
    #[derive(Serialize)]
    struct CodeTable {
        format: u32,
        glyphs: Vec<IconGlyph>,
    }
    let mut glyphs = load_fontgen_glyphs(root)?;
    glyphs.sort_by(|a, b| a.name.cmp(&b.name));
    let table = CodeTable { format: 1, glyphs };
    let path = root.join("assets/icons/icon_codes.json");
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&table)?),
    )
    .with_context(|| format!("writing {}", path.display()))?;
    Ok(table.glyphs.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base_content;

    #[test]
    fn capitalize_matches_assets_process() {
        assert_eq!(capitalize("file-text"), "FileText");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("map"), "Map");
        assert_eq!(capitalize("zoom-in"), "ZoomIn");
    }

    #[test]
    fn icons_properties_append_only() {
        let root = std::env::temp_dir().join(format!("icons-sync-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(root.join("assets/icons")).unwrap();
        // A synthetic existing entry with a higher code than the PUA start.
        fs::write(
            root.join("assets/icons/icons.properties"),
            "63743=spawn|block-spawn-ui\n",
        )
        .unwrap();

        let registry = base_content().unwrap();
        let first = sync_icons_properties(&root, &registry).unwrap();
        assert!(first.added > 0);
        // Existing line preserved byte-for-byte at the head.
        let text = fs::read_to_string(root.join("assets/icons/icons.properties")).unwrap();
        assert!(text.starts_with("63743=spawn|block-spawn-ui\n"));
        // New codes allocate downward from the existing minimum.
        assert_eq!(
            first.next_code,
            63743u32
                .saturating_sub(1)
                .saturating_sub(first.added as u32)
        );

        let second = sync_icons_properties(&root, &registry).unwrap();
        assert_eq!(second.added, 0);
        let text2 = fs::read_to_string(root.join("assets/icons/icons.properties")).unwrap();
        assert_eq!(text, text2);

        fs::remove_dir_all(&root).unwrap();
    }
}

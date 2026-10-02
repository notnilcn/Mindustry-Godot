// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Arc `TextureAtlas`/`AtlasRegion` runtime semantics over the plan-03
// `sprites.atlas.json` manifest (deviation A1).

//! Runtime atlas region index (plan 03 §3.3).
//!
//! Loads a `sprites.atlas.json` manifest (written by `mind-tools pack`) and
//! answers region lookups with Arc `Core.atlas.find` semantics: a missing name
//! is **not** silently replaced with the `error` region — [`AtlasIndex::find`]
//! returns `None`; only an explicit [`AtlasIndex::find_or`] bridges to a
//! fallback. This makes accidental missing regions a load error (§3.3).

use std::collections::HashMap;

use serde::Deserialize;

use crate::content::ContentType;

/// One atlas region (Arc `AtlasRegion` fields used by the game).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    /// Region name — parity ABI, byte-identical, never path-qualified.
    pub name: String,
    /// Page index into [`AtlasIndex::pages`].
    pub page: usize,
    /// X on the page (y-down pixels).
    pub x: i32,
    /// Y on the page (y-down pixels).
    pub y: i32,
    /// Region width.
    pub w: i32,
    /// Region height.
    pub h: i32,
    /// Ninepatch splits `[left, right, top, bottom]` (`None` when absent, Arc
    /// `splits == null` parity).
    pub splits: Option<[i32; 4]>,
    /// Ninepatch pads `[left, right, top, bottom]` (`None` when absent).
    pub pads: Option<[i32; 4]>,
    /// Whitespace-strip offsets `[offsetX, originalHeight - regionHeight - offsetY]`.
    pub offsets: [i32; 2],
    /// Page type (`main`/`environment`/`ui`/`rubble`).
    pub page_type: PageType,
}

/// `MultiPacker.PageType` (mirrors `mind_atlas::page::PageType` without the
/// dependency; names are the ABI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageType {
    /// `main`.
    Main,
    /// `environment`.
    Environment,
    /// `ui`.
    Ui,
    /// `rubble`.
    Rubble,
}

impl PageType {
    /// Logical name (`PageType.name()`).
    pub const fn name(self) -> &'static str {
        match self {
            PageType::Main => "main",
            PageType::Environment => "environment",
            PageType::Ui => "ui",
            PageType::Rubble => "rubble",
        }
    }

    /// Parses a manifest page-type name.
    pub fn parse(name: &str) -> Option<PageType> {
        match name {
            "main" => Some(PageType::Main),
            "environment" => Some(PageType::Environment),
            "ui" => Some(PageType::Ui),
            "rubble" => Some(PageType::Rubble),
            _ => None,
        }
    }
}

/// One page entry (image file metadata; pixels are loaded by `mind-gdext`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Page index.
    pub index: usize,
    /// Page type.
    pub type_: PageType,
    /// PNG file name relative to the manifest directory.
    pub file: String,
    /// Image width.
    pub width: i32,
    /// Image height: i32.
    pub height: i32,
    /// Expected sha256 of the PNG (lowercase hex).
    pub sha256: String,
}

/// Errors from manifest loading.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum AtlasLoadError {
    /// JSON parse failure.
    #[error("atlas manifest parse: {0}")]
    Parse(String),
    /// Unsupported format version.
    #[error("atlas manifest format {0} unsupported (expected 1)")]
    Format(u32),
    /// A region references a page that does not exist.
    #[error("region `{0}` references missing page {1}")]
    BadPage(String, usize),
    /// Unknown page-type name.
    #[error("unknown page type `{0}`")]
    BadPageType(String),
    /// Duplicate region name in the manifest.
    #[error("duplicate region `{0}`")]
    DuplicateRegion(String),
}

/// The loaded region index (`Core.atlas` equivalent, metadata only).
#[derive(Debug, Clone, Default)]
pub struct AtlasIndex {
    by_name: HashMap<String, usize>,
    regions: Vec<Region>,
    pages: Vec<Page>,
    /// Whether this is the 2048 fallback atlas.
    pub fallback: bool,
    /// `inputsHash` from the manifest.
    pub inputs_hash: String,
}

impl AtlasIndex {
    /// Parses a `sprites.atlas.json` manifest.
    pub fn from_manifest_json(text: &str) -> Result<AtlasIndex, AtlasLoadError> {
        let raw: RawManifest =
            serde_json::from_str(text).map_err(|error| AtlasLoadError::Parse(error.to_string()))?;
        if raw.format != 1 {
            return Err(AtlasLoadError::Format(raw.format));
        }
        let mut pages = Vec::with_capacity(raw.pages.len());
        for page in &raw.pages {
            pages.push(Page {
                index: page.index,
                type_: PageType::parse(&page.type_)
                    .ok_or_else(|| AtlasLoadError::BadPageType(page.type_.clone()))?,
                file: page.file.clone(),
                width: page.width,
                height: page.height,
                sha256: page.sha256.clone(),
            });
        }
        let mut by_name = HashMap::with_capacity(raw.regions.len());
        let mut regions = Vec::with_capacity(raw.regions.len());
        for entry in &raw.regions {
            if entry.page >= pages.len() {
                return Err(AtlasLoadError::BadPage(entry.name.clone(), entry.page));
            }
            let page_type = PageType::parse(&entry.page_type)
                .ok_or_else(|| AtlasLoadError::BadPageType(entry.page_type.clone()))?;
            let index = regions.len();
            if by_name.insert(entry.name.clone(), index).is_some() {
                return Err(AtlasLoadError::DuplicateRegion(entry.name.clone()));
            }
            regions.push(Region {
                name: entry.name.clone(),
                page: entry.page,
                x: entry.x,
                y: entry.y,
                w: entry.w,
                h: entry.h,
                splits: entry.splits,
                pads: entry.pads,
                offsets: entry.offsets,
                page_type,
            });
        }
        Ok(AtlasIndex {
            by_name,
            regions,
            pages,
            fallback: raw.fallback,
            inputs_hash: raw.inputs_hash,
        })
    }

    /// `Core.atlas.find(name)` — `None` means "not found" (Arc `found() == false`).
    pub fn find(&self, name: &str) -> Option<&Region> {
        self.by_name.get(name).map(|index| &self.regions[*index])
    }

    /// `Core.atlas.find(name, fallback)` — resolves `fallback` when `name` is
    /// missing; `None` only when the fallback is missing too.
    pub fn find_or(&self, name: &str, fallback: &str) -> Option<&Region> {
        self.find(name).or_else(|| self.find(fallback))
    }

    /// `Core.atlas.has(name)`.
    pub fn has(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Number of regions.
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// All regions in manifest order (sorted by name at pack time).
    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// All pages.
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// Region names that appear more than once — always empty for a validated
    /// index (kept for the MCP duplicate assertion).
    pub fn duplicates(&self) -> Vec<&str> {
        Vec::new()
    }

    /// `UnlockableContent.loadIcon()` full-icon chain (plan 03 §4):
    /// `fullOverride` → `<type>-<name>-full` → `<name>-full` → `<name>` →
    /// `<type>-<name>` → `<name>1`.
    pub fn find_full_icon(
        &self,
        type_: ContentType,
        name: &str,
        full_override: &str,
    ) -> Option<&Region> {
        if !full_override.is_empty()
            && let Some(region) = self.find(full_override)
        {
            return Some(region);
        }
        let type_name = type_.name();
        self.find(&format!("{type_name}-{name}-full"))
            .or_else(|| self.find(&format!("{name}-full")))
            .or_else(|| self.find(name))
            .or_else(|| self.find(&format!("{type_name}-{name}")))
            .or_else(|| self.find(&format!("{name}1")))
    }

    /// `UnlockableContent.loadIcon()` ui-icon lookup: `<type>-<name>-ui` else
    /// the full icon.
    pub fn find_ui_icon<'a>(
        &'a self,
        type_: ContentType,
        name: &str,
        full_icon: Option<&'a Region>,
    ) -> Option<&'a Region> {
        self.find(&format!("{}-{name}-ui", type_.name()))
            .or(full_icon)
    }
}

#[derive(Deserialize)]
struct RawManifest {
    format: u32,
    #[serde(default)]
    fallback: bool,
    #[serde(rename = "inputsHash", default)]
    inputs_hash: String,
    pages: Vec<RawPage>,
    regions: Vec<RawRegion>,
}

#[derive(Deserialize)]
struct RawPage {
    index: usize,
    #[serde(rename = "type")]
    type_: String,
    file: String,
    width: i32,
    height: i32,
    #[serde(default)]
    sha256: String,
}

#[derive(Deserialize)]
struct RawRegion {
    name: String,
    page: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    #[serde(default)]
    splits: Option<[i32; 4]>,
    #[serde(default)]
    pads: Option<[i32; 4]>,
    #[serde(default)]
    offsets: [i32; 2],
    #[serde(rename = "pageType")]
    page_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
  "format": 1,
  "generator": "mind-tools 0.1.0",
  "pageCap": 4096,
  "fallback": false,
  "inputsHash": "sha256:abc",
  "pages": [
    {"index": 0, "type": "main", "file": "sprites.png", "width": 64, "height": 64, "sha256": "00"}
  ],
  "regions": [
    {"name": "copper-wall", "page": 0, "x": 1, "y": 1, "w": 32, "h": 32,
     "offsets": [0, 0], "original": [32, 32], "pageType": "main"},
    {"name": "block-copper-wall-full", "page": 0, "x": 1, "y": 34, "w": 32, "h": 32,
     "offsets": [0, 0], "original": [32, 32], "pageType": "main"},
    {"name": "block-copper-wall-ui", "page": 0, "x": 34, "y": 1, "w": 32, "h": 32,
     "offsets": [0, 0], "original": [32, 32], "pageType": "ui"},
    {"name": "bar", "page": 0, "x": 34, "y": 34, "w": 24, "h": 12, "splits": [1, 3, 1, 3],
     "offsets": [0, 0], "original": [24, 12], "pageType": "ui"},
    {"name": "error", "page": 0, "x": 60, "y": 1, "w": 3, "h": 3,
     "offsets": [0, 0], "original": [3, 3], "pageType": "main"}
  ]
}"#;

    #[test]
    fn loads_manifest_and_finds_regions() {
        let index = AtlasIndex::from_manifest_json(FIXTURE).unwrap();
        assert_eq!(index.len(), 5);
        let region = index.find("copper-wall").unwrap();
        assert_eq!((region.x, region.y, region.w, region.h), (1, 1, 32, 32));
        assert!(index.has("error"));
        assert!(!index.has("missing"));
        assert!(index.find("missing").is_none());
        // find_or bridges to the fallback only.
        assert_eq!(index.find_or("missing", "error").unwrap().name, "error");
        assert!(index.find_or("missing", "also-missing").is_none());
        // Ninepatch splits parse.
        assert_eq!(index.find("bar").unwrap().splits, Some([1, 3, 1, 3]));
        assert_eq!(index.find("copper-wall").unwrap().splits, None);
        assert_eq!(index.find("copper-wall").unwrap().page_type, PageType::Main);
        assert_eq!(
            index.find("block-copper-wall-ui").unwrap().page_type,
            PageType::Ui
        );
    }

    #[test]
    fn duplicate_region_rejected() {
        let text = FIXTURE.replace(r#""name": "error""#, r#""name": "copper-wall""#);
        assert!(matches!(
            AtlasIndex::from_manifest_json(&text),
            Err(AtlasLoadError::DuplicateRegion(name)) if name == "copper-wall"
        ));
    }

    #[test]
    fn load_icon_chain() {
        let index = AtlasIndex::from_manifest_json(FIXTURE).unwrap();
        let full = index
            .find_full_icon(ContentType::Block, "copper-wall", "")
            .unwrap();
        assert_eq!(full.name, "block-copper-wall-full");
        // Falls through to `<name>` when no generated icon exists.
        let plain = index
            .find_full_icon(ContentType::Block, "error", "")
            .unwrap();
        assert_eq!(plain.name, "error");
        // fullOverride wins.
        let over = index
            .find_full_icon(ContentType::Block, "copper-wall", "bar")
            .unwrap();
        assert_eq!(over.name, "bar");
        let ui = index
            .find_ui_icon(ContentType::Block, "copper-wall", Some(full))
            .unwrap();
        assert_eq!(ui.name, "block-copper-wall-ui");
        // ui falls back to the full icon.
        let ui_fallback = index
            .find_ui_icon(ContentType::Block, "error", Some(plain))
            .unwrap();
        assert_eq!(ui_fallback.name, "error");
    }
}

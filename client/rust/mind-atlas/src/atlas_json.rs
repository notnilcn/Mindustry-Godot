// SPDX-License-Identifier: GPL-3.0-only

//! `sprites.atlas.json` manifest format (plan 03 §6.3, deviation A1).
//!
//! Writer side of the runtime atlas manifest: regions are stored as a flat
//! array sorted byte-wise by `name`; coordinates are y-down page pixels,
//! directly consumable as Godot `AtlasTexture` rects. Page layout is **not**
//! ABI — only region names and presence are.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::pack::{PackedPage, PackedRegion};
use crate::page::PageType;

/// Manifest root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtlasManifest {
    /// Format version (1).
    pub format: u32,
    /// Generator tag (`mind-tools <version>`).
    pub generator: String,
    /// Root page cap used for this pack (4096 normal, 2048 fallback).
    #[serde(rename = "pageCap")]
    pub page_cap: i32,
    /// Whether this is the 2048 fallback atlas.
    pub fallback: bool,
    /// sha256 over sorted `(relative path, sha256)` of every staged source
    /// after generation but before packing.
    #[serde(rename = "inputsHash")]
    pub inputs_hash: String,
    /// Page files in index order.
    pub pages: Vec<PageEntry>,
    /// Every region, sorted byte-wise by name.
    pub regions: Vec<RegionEntry>,
}

/// One page entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageEntry {
    /// Page index (region `page` references this).
    pub index: usize,
    /// Page type name (`main`/`environment`/`ui`/`rubble`).
    #[serde(rename = "type")]
    pub type_: String,
    /// PNG file name relative to the manifest directory.
    pub file: String,
    /// Image width.
    pub width: i32,
    /// Image height.
    pub height: i32,
    /// Lowercase hex sha256 of the PNG bytes.
    pub sha256: String,
}

/// One region entry (coordinates are y-down page pixels).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionEntry {
    /// Region name (parity ABI; never path-qualified).
    pub name: String,
    /// Page index into `pages`.
    pub page: usize,
    /// X on the page.
    pub x: i32,
    /// Y on the page.
    pub y: i32,
    /// Region width.
    pub w: i32,
    /// Region height.
    pub h: i32,
    /// Ninepatch splits `[left, right, top, bottom]`, or `null` when absent
    /// (Arc `write.bool(splits != null)` parity).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub splits: Option<[i32; 4]>,
    /// Ninepatch pads `[left, right, top, bottom]`, or `null` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pads: Option<[i32; 4]>,
    /// Whitespace-strip offsets `[offsetX, originalHeight - regionHeight - offsetY]`.
    pub offsets: [i32; 2],
    /// Original (unstripped) size.
    pub original: [i32; 2],
    /// Page type name (`main`/`environment`/`ui`/`rubble`).
    #[serde(rename = "pageType")]
    pub page_type: String,
}

impl AtlasManifest {
    /// Builds a manifest from packer output.
    pub fn from_pack(
        pages: &[PackedPage],
        regions: &[PackedRegion],
        page_types: &[PageType],
        page_cap: i32,
        fallback: bool,
        inputs_hash: String,
        pngs: &[Vec<u8>],
    ) -> AtlasManifest {
        let page_entries = pages
            .iter()
            .enumerate()
            .map(|(index, page)| PageEntry {
                index,
                type_: page_types
                    .get(index)
                    .copied()
                    .unwrap_or(PageType::Main)
                    .name()
                    .to_owned(),
                file: page_file_name(index),
                width: page.width,
                height: page.height,
                sha256: crate::manifest::sha256_hex(&pngs[index]),
            })
            .collect();
        let region_entries = regions
            .iter()
            .map(|region| RegionEntry {
                name: region.name.clone(),
                page: region.page,
                x: region.x,
                y: region.y,
                w: region.w,
                h: region.h,
                splits: region.splits,
                pads: region.pads,
                offsets: region.offsets,
                original: region.original,
                page_type: page_types
                    .get(region.page)
                    .copied()
                    .unwrap_or(PageType::Main)
                    .name()
                    .to_owned(),
            })
            .collect();
        AtlasManifest {
            format: 1,
            generator: format!("mind-tools {}", env!("CARGO_PKG_VERSION")),
            page_cap,
            fallback,
            inputs_hash,
            pages: page_entries,
            regions: region_entries,
        }
    }

    /// Serializes as pretty JSON with a trailing newline.
    pub fn to_json(&self) -> Result<String> {
        Ok(format!("{}\n", serde_json::to_string_pretty(self)?))
    }

    /// Writes the manifest.
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }

    /// Reads a manifest.
    pub fn read(path: &Path) -> Result<AtlasManifest> {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Parses a manifest from a string.
    pub fn parse(text: &str) -> Result<AtlasManifest> {
        Ok(serde_json::from_str(text)?)
    }
}

/// Page PNG file name for an index (`sprites.png`, `sprites2.png`, ...).
pub fn page_file_name(index: usize) -> String {
    if index == 0 {
        String::from("sprites.png")
    } else {
        format!("sprites{}.png", index + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_names_match_upstream() {
        assert_eq!(page_file_name(0), "sprites.png");
        assert_eq!(page_file_name(1), "sprites2.png");
        assert_eq!(page_file_name(4), "sprites5.png");
    }

    #[test]
    fn manifest_round_trip() {
        let json = r#"{
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
     "offsets": [0, 0], "original": [32, 32], "pageType": "main"}
  ]
}"#;
        let manifest = AtlasManifest::parse(json).unwrap();
        assert_eq!(manifest.regions.len(), 1);
        assert_eq!(manifest.regions[0].name, "copper-wall");
        assert_eq!(manifest.regions[0].splits, None);
        let out = manifest.to_json().unwrap();
        let reparsed = AtlasManifest::parse(&out).unwrap();
        assert_eq!(manifest, reparsed);
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! World-processor list for `MapProcessorsDialog` (plan 19 M6; plan 13 owns
//! `LogicBuild`).
//!
//! Upstream scans every tile for a `world-processor` center and reads the
//! `LogicBuild` tag/icon. The port keeps the scan Godot-free; the tag/icon come
//! from a caller resolver over the ECS `LogicBlockState`, so headless files with
//! no entities simply report processors with empty tags.

use crate::content::{BlockId, ContentRegistry};
use crate::logic::blocks::logic_block::MAX_NAME_LENGTH;
use crate::world::WorldGrid;

/// One processor row (`MapProcessorsDialog.rebuild`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorEntry {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// `LogicBuild.tag` (world-processor name).
    pub tag: Option<String>,
    /// `LogicBuild.iconTag`.
    pub icon_tag: char,
}

impl ProcessorEntry {
    /// The display label upstream builds (`[accent]tag [[x, y]]`).
    pub fn label(&self) -> String {
        match &self.tag {
            Some(tag) => format!("[accent]{tag}\n[lightgray][[{}, {}]", self.x, self.y),
            None => format!("<no name>\n[lightgray][[{}, {}]", self.x, self.y),
        }
    }

    /// Whether a search string matches the row's tag.
    pub fn matches(&self, search: &str) -> bool {
        search.is_empty()
            || self
                .tag
                .as_deref()
                .is_some_and(|tag| tag.to_lowercase().contains(&search.to_lowercase()))
    }
}

/// Collector variant for a resolver that has no ECS access.
pub fn processor_entries<F>(
    grid: &WorldGrid,
    content: &ContentRegistry,
    mut resolve: F,
) -> Vec<ProcessorEntry>
where
    F: FnMut(i32, i32) -> (Option<String>, char),
{
    let Some(processor) = content.block_id("world-processor") else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for index in 0..grid.tiles.len() {
        let tile = grid.tiles.geti(index);
        if tile.block != processor {
            continue;
        }
        // Only multiblock centers are listed (`tile.isCenter()`); a tile with no
        // ECS entity is trivially a center.
        if tile.build.is_some() {
            continue;
        }
        let (x, y) = (tile.x as i32, tile.y as i32);
        let (tag, icon_tag) = resolve(x, y);
        entries.push(ProcessorEntry {
            x,
            y,
            tag,
            icon_tag,
        });
    }
    entries.sort_by_key(|entry| (entry.y, entry.x));
    entries
}

/// `MapProcessorsDialog` Add button: first non-synthetic tile becomes a
/// world-processor. Returns the chosen tile and whether one was found.
pub fn add_processor(grid: &mut WorldGrid, content: &ContentRegistry) -> Option<(i32, i32)> {
    let processor = content.block_id("world-processor")?;
    for index in 0..grid.tiles.len() {
        let tile = grid.tiles.geti(index);
        let synthetic = content
            .block(tile.block)
            .is_some_and(crate::maps::filters::block_info::synthetic);
        if !synthetic {
            let (x, y) = (tile.x as i32, tile.y as i32);
            grid.tiles.get_mut(x, y).block = processor;
            return Some((x, y));
        }
    }
    None
}

/// Truncates a candidate tag to `LogicBlock.maxNameLength`.
pub fn clamp_tag(tag: &str) -> String {
    tag.chars().take(MAX_NAME_LENGTH).collect()
}

/// Removes a processor: a surrounded tile becomes a static wall, else air
/// (`MapProcessorsDialog` delete confirm).
pub fn remove_processor(
    grid: &mut WorldGrid,
    content: &ContentRegistry,
    x: i32,
    y: i32,
    surrounded: bool,
) {
    let air = content.block_id("air").unwrap_or(BlockId::AIR);
    let replacement = if surrounded {
        // `tile.floor().wall` is not tracked; `stone-wall` is the upstream
        // default (`Blocks.stoneWall`).
        content.block_id("stone-wall").unwrap_or(air)
    } else {
        air
    };
    if grid.tiles.in_bounds(x, y) {
        grid.tiles.get_mut(x, y).block = replacement;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    #[test]
    fn scan_lists_processors_with_resolver() {
        let content = test_registry();
        let processor = content.block_id("world-processor").unwrap();
        let mut grid = WorldGrid::new(8, 8);
        grid.tiles.get_mut(3, 4).block = processor;
        let entries = processor_entries(&grid, &content, |_, _| (Some("main".to_owned()), 'A'));
        assert_eq!(entries.len(), 1);
        assert_eq!((entries[0].x, entries[0].y), (3, 4));
        assert!(entries[0].matches("mai"));
        assert!(!entries[0].matches("other"));
        assert_eq!(clamp_tag(&"x".repeat(50)).len(), MAX_NAME_LENGTH);
    }

    #[test]
    fn add_and_remove_processor() {
        let content = test_registry();
        let mut grid = WorldGrid::new(4, 4);
        let position = add_processor(&mut grid, &content);
        assert!(position.is_some());
        let entries = processor_entries(&grid, &content, |_, _| (None, '\0'));
        assert_eq!(entries.len(), 1);
        remove_processor(
            &mut grid,
            &content,
            position.unwrap().0,
            position.unwrap().1,
            false,
        );
        assert!(processor_entries(&grid, &content, |_, _| (None, '\0')).is_empty());
    }
}

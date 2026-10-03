// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `EditorTool` (`editor/EditorTool.java`, plan 19 §3.5).
//!
//! The enum plus its per-tool alternate-mode metadata. `key()` upstream returns
//! an Arc `KeyCode`; this port returns the lowercase key name string because
//! `mind-core` is Godot/input-free (D1) — `mind-gdext` maps the name to a
//! `KeyCode`. `touched`/`touched_line` dispatch lives on [`MapEditor`].

use super::EditorGrid;
use super::tile_op::{OP_DATA, OP_DATA_EXTRA, TileOp, TileOpData};
use super::{D8, EditorBlockInfo, MapEditor};
use crate::content::{BlockDef, BlockId, ContentRegistry};

/// The editor tool enum (`EditorTool`), declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditorTool {
    /// Pan/zoom only (upstream default).
    #[default]
    Zoom,
    /// Pick the block/overlay/floor under the cursor.
    Pick,
    /// Drag a line of blocks.
    Line,
    /// Freehand brush.
    Pencil,
    /// Erase blocks/overlays.
    Eraser,
    /// Flood fill.
    Fill,
    /// Random spray.
    Spray,
}

impl EditorTool {
    /// Number of tools (`EditorTool.all.length`).
    pub const COUNT: usize = 7;
    /// All tools in declaration order (`EditorTool.all`).
    pub const ALL: [EditorTool; Self::COUNT] = [
        EditorTool::Zoom,
        EditorTool::Pick,
        EditorTool::Line,
        EditorTool::Pencil,
        EditorTool::Eraser,
        EditorTool::Fill,
        EditorTool::Spray,
    ];

    /// The enum order index (used by `tool_modes`).
    pub fn index(self) -> usize {
        match self {
            EditorTool::Zoom => 0,
            EditorTool::Pick => 1,
            EditorTool::Line => 2,
            EditorTool::Pencil => 3,
            EditorTool::Eraser => 4,
            EditorTool::Fill => 5,
            EditorTool::Spray => 6,
        }
    }

    /// Activation key (`EditorTool.key`): v/i/l/b/e/g/r.
    pub fn key(self) -> &'static str {
        match self {
            EditorTool::Zoom => "v",
            EditorTool::Pick => "i",
            EditorTool::Line => "l",
            EditorTool::Pencil => "b",
            EditorTool::Eraser => "e",
            EditorTool::Fill => "g",
            EditorTool::Spray => "r",
        }
    }

    /// Parses a tool by name (`EditorTool.valueOf`, case-insensitive).
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|tool| tool.name().eq_ignore_ascii_case(name))
    }

    /// Lowercase tool name.
    pub fn name(self) -> &'static str {
        match self {
            EditorTool::Zoom => "zoom",
            EditorTool::Pick => "pick",
            EditorTool::Line => "line",
            EditorTool::Pencil => "pencil",
            EditorTool::Eraser => "eraser",
            EditorTool::Fill => "fill",
            EditorTool::Spray => "spray",
        }
    }

    /// Alternate placement modes (`EditorTool.altModes`).
    pub fn alt_modes(self) -> &'static [&'static str] {
        match self {
            EditorTool::Line => &["replace", "orthogonal"],
            EditorTool::Pencil => &["replace", "square", "drawteams", "underliquid"],
            EditorTool::Eraser => &["eraseores"],
            EditorTool::Fill => &[
                "replaceall",
                "fillteams",
                "fillerase",
                "fillcliffs",
                "fillunderliquid",
            ],
            EditorTool::Spray => &["replace"],
            EditorTool::Zoom | EditorTool::Pick => &[],
        }
    }

    /// Whether the tool mutates tiles (`EditorTool.edit`).
    pub fn edit(self) -> bool {
        matches!(
            self,
            EditorTool::Pencil | EditorTool::Eraser | EditorTool::Fill | EditorTool::Spray
        )
    }

    /// Whether the tool drags across tiles (`EditorTool.draggable`).
    pub fn draggable(self) -> bool {
        matches!(
            self,
            EditorTool::Pencil | EditorTool::Eraser | EditorTool::Spray
        )
    }
}

/// `Block instanceof Floor`.
pub fn is_floor(def: Option<&BlockDef>) -> bool {
    def.is_some_and(crate::maps::filters::block_info::is_floor)
}

/// `OverlayFloor` classification.
pub fn is_overlay(def: Option<&BlockDef>) -> bool {
    def.is_some_and(crate::maps::filters::block_info::is_overlay)
}

/// `Floor.isLiquid` classification.
pub fn is_liquid(def: Option<&BlockDef>) -> bool {
    def.is_some_and(crate::maps::filters::block_info::is_liquid)
}

/// Cursor dispatch for one tile (`EditorTool.touched`); the active mode comes
/// from `editor.tool_modes[tool.index()]` (`-1` = standard).
pub fn touched(
    editor: &mut MapEditor,
    tool: EditorTool,
    world: &mut dyn EditorGrid,
    content: &ContentRegistry,
    x: i32,
    y: i32,
) {
    let mode = editor.tool_modes[tool.index()];
    match tool {
        EditorTool::Pick => {
            if !world.in_bounds(x, y) {
                return;
            }
            let block = world.block_id(x, y);
            let overlay = world.overlay_id(x, y);
            let floor = world.floor_id(x, y);
            let block_editable = content.block(block).is_some_and(|def| def.in_editor);
            editor.draw_block = if block == BlockId::AIR || !block_editable {
                if overlay == BlockId::AIR {
                    floor
                } else {
                    overlay
                }
            } else {
                block
            };
        }
        EditorTool::Line => { /* line drawing happens in touched_line */ }
        EditorTool::Pencil => match mode {
            0 => editor.draw_blocks_replace(world, content, x, y),
            1 => editor.draw_blocks_square(world, content, x, y, false),
            2 => {
                let team = editor.draw_team;
                editor.draw_circle(world, x, y, &mut |w, tx, ty| w.set_team(tx, ty, team));
            }
            3 => {
                let info = editor.draw_info(content);
                if !info.is_liquid {
                    editor.draw_blocks_where(world, content, x, y, false, true, &|w, c, tx, ty| {
                        is_liquid(c.block(w.floor_id(tx, ty)))
                    });
                }
            }
            _ => editor.draw_blocks(world, content, x, y),
        },
        EditorTool::Eraser => {
            editor.draw_circle(world, x, y, &mut |w, tx, ty| {
                if mode == 0 {
                    w.set_overlay(tx, ty, BlockId::AIR);
                } else {
                    w.set_block(tx, ty, BlockId::AIR, 0, 0);
                }
            });
        }
        EditorTool::Fill => fill_touched(editor, mode, world, content, x, y),
        EditorTool::Spray => {
            let info = editor.draw_info(content);
            let block = editor.draw_block;
            let brush = editor.brush_size;
            if info.is_floor {
                editor.draw_circle(world, x, y, &mut |w, tx, ty| {
                    if spray_chance(brush, tx, ty) {
                        w.set_floor(tx, ty, block);
                    }
                });
            } else if mode == 0 {
                editor.draw_blocks_where(world, content, x, y, false, false, &|w, _c, tx, ty| {
                    w.block_id(tx, ty) != BlockId::AIR
                });
            } else {
                editor.draw_blocks(world, content, x, y);
            }
        }
        EditorTool::Zoom => {}
    }
}

/// Records the two data pre-ops (`EditorTool.fill` `saveData` wrapper).
fn record_data_pre_ops(editor: &mut MapEditor, world: &dyn EditorGrid, x: i32, y: i32) {
    let (data, floor_data, overlay_data, extra) = world.tile_data(x, y);
    editor.add_tile_op(TileOp::get(
        x,
        y,
        OP_DATA,
        TileOpData::get(data, floor_data, overlay_data),
    ));
    editor.add_tile_op(TileOp::get(x, y, OP_DATA_EXTRA, extra));
}

/// `EditorTool.fill.touched` (upstream `EditorTool.java:104-234`): six modes with
/// the multiblock guard, team fill, erase, cliff fill and under-liquid fill.
fn fill_touched(
    editor: &mut MapEditor,
    mode: i32,
    world: &mut dyn EditorGrid,
    content: &ContentRegistry,
    x: i32,
    y: i32,
) {
    if !world.in_bounds(x, y) {
        return;
    }
    let draw_block = editor.draw_block;
    let draw_team = editor.draw_team;
    let rotation = editor.rotation;
    let draw_info = editor.draw_info(content);
    let tile_block = world.block_id(x, y);
    let tile_info = EditorBlockInfo::from_def(content.block(tile_block));

    // Don't fill multiblocks with a multiblock draw block; behave like the pencil.
    if draw_info.is_multiblock && (mode == 0 || mode == -1) {
        editor.draw_blocks(world, content, x, y);
        return;
    }

    if mode == 0 || mode == -1 {
        // Can't fill parts of multiblocks.
        if tile_info.is_multiblock {
            return;
        }
        let replace = mode == 0;
        let save_data = draw_info.save_data;
        let supports_overlay = draw_info.supports_overlay;
        if draw_info.is_overlay {
            let dest = world.overlay_id(x, y);
            if dest == draw_block {
                return;
            }
            let tester = move |w: &dyn EditorGrid, c: &ContentRegistry, tx: i32, ty: i32| {
                w.overlay_id(tx, ty) == dest
                    && EditorBlockInfo::from_def(c.block(w.floor_id(tx, ty))).supports_overlay
            };
            let mut setter = move |ed: &mut MapEditor, w: &mut dyn EditorGrid, tx: i32, ty: i32| {
                if save_data {
                    record_data_pre_ops(ed, &*w, tx, ty);
                }
                w.set_overlay(tx, ty, draw_block);
            };
            editor.fill(world, content, x, y, replace, &tester, &mut setter);
        } else if draw_info.is_floor {
            let dest = world.floor_id(x, y);
            if dest == draw_block {
                return;
            }
            let content_ref = content;
            let tester = move |w: &dyn EditorGrid, _c: &ContentRegistry, tx: i32, ty: i32| {
                w.floor_id(tx, ty) == dest
            };
            let mut setter = move |ed: &mut MapEditor, w: &mut dyn EditorGrid, tx: i32, ty: i32| {
                if save_data {
                    record_data_pre_ops(ed, &*w, tx, ty);
                }
                w.set_floor(tx, ty, draw_block);
                let overlay = w.overlay_id(tx, ty);
                if !is_overlay(content_ref.block(overlay)) && !supports_overlay {
                    w.set_overlay(tx, ty, BlockId::AIR);
                }
            };
            editor.fill(world, content, x, y, replace, &tester, &mut setter);
        } else {
            let dest = world.block_id(x, y);
            if dest == draw_block {
                return;
            }
            let tester = move |w: &dyn EditorGrid, _c: &ContentRegistry, tx: i32, ty: i32| {
                w.block_id(tx, ty) == dest
            };
            let mut setter = move |ed: &mut MapEditor, w: &mut dyn EditorGrid, tx: i32, ty: i32| {
                if save_data {
                    record_data_pre_ops(ed, &*w, tx, ty);
                }
                w.set_block(tx, ty, draw_block, draw_team, rotation);
            };
            editor.fill(world, content, x, y, replace, &tester, &mut setter);
        }
    } else if mode == 1 {
        // Team fill: only synthetic tiles are meaningful.
        if tile_info.synthetic {
            let dest = world.team_id(x, y);
            if dest == draw_team {
                return;
            }
            let tester = move |w: &dyn EditorGrid, c: &ContentRegistry, tx: i32, ty: i32| {
                w.team_id(tx, ty) == dest
                    && EditorBlockInfo::from_def(c.block(w.block_id(tx, ty))).synthetic
            };
            let mut setter =
                move |_ed: &mut MapEditor, w: &mut dyn EditorGrid, tx: i32, ty: i32| {
                    w.set_team(tx, ty, draw_team);
                };
            editor.fill(world, content, x, y, true, &tester, &mut setter);
        }
    } else if mode == 2 {
        // Erase: blocks first, then overlays; floors are not erased.
        let overlay = world.overlay_id(x, y);
        if tile_block != BlockId::AIR {
            let tester = move |w: &dyn EditorGrid, _c: &ContentRegistry, tx: i32, ty: i32| {
                w.block_id(tx, ty) == tile_block
            };
            let mut setter =
                move |_ed: &mut MapEditor, w: &mut dyn EditorGrid, tx: i32, ty: i32| {
                    w.set_block(tx, ty, BlockId::AIR, 0, 0);
                };
            editor.fill(world, content, x, y, false, &tester, &mut setter);
        } else if overlay != BlockId::AIR {
            let tester = move |w: &dyn EditorGrid, _c: &ContentRegistry, tx: i32, ty: i32| {
                w.overlay_id(tx, ty) == overlay
            };
            let mut setter =
                move |_ed: &mut MapEditor, w: &mut dyn EditorGrid, tx: i32, ty: i32| {
                    w.set_overlay(tx, ty, BlockId::AIR);
                };
            editor.fill(world, content, x, y, false, &tester, &mut setter);
        }
    } else if mode == 3 {
        // Cliff fill: static blocks become autotiled cliffs.
        let cliff = content.block_id("cliff").unwrap_or(BlockId::AIR);
        if !tile_info.is_static || tile_block == cliff {
            return;
        }
        let air = BlockId::AIR;
        let content_ref = content;
        let mut was_static = vec![false; (world.width() * world.height()).max(0) as usize];
        let tester = move |w: &dyn EditorGrid, c: &ContentRegistry, tx: i32, ty: i32| {
            let block = w.block_id(tx, ty);
            EditorBlockInfo::from_def(c.block(block)).is_static && block != cliff
        };
        let mut setter = move |_ed: &mut MapEditor, w: &mut dyn EditorGrid, tx: i32, ty: i32| {
            let width = w.width();
            let mut rotation = 0i32;
            for (i, (dx, dy)) in D8.iter().enumerate() {
                let (ox, oy) = (tx + dx, ty + dy);
                if w.in_bounds(ox, oy) {
                    let other = w.block_id(ox, oy);
                    let index = (ox + oy * width).max(0) as usize;
                    if !EditorBlockInfo::from_def(content_ref.block(other)).is_static
                        && !was_static.get(index).copied().unwrap_or(false)
                    {
                        rotation |= 1 << i;
                    }
                }
            }
            let index = (tx + ty * width).max(0) as usize;
            if rotation != 0 {
                w.set_block(tx, ty, cliff, w.team_id(tx, ty), w.rotation(tx, ty));
            } else {
                w.set_block(tx, ty, air, 0, 0);
                if let Some(slot) = was_static.get_mut(index) {
                    *slot = true;
                }
            }
            let (_, floor_data, overlay_data, _) = w.tile_data(tx, ty);
            w.set_data(tx, ty, rotation as i8, floor_data, overlay_data);
        };
        editor.fill(world, content, x, y, false, &tester, &mut setter);
    } else if mode == 4 && draw_info.is_floor && !draw_info.is_liquid {
        // Fill under liquid: overlay liquid tiles, matching floor + overlay.
        if !EditorBlockInfo::from_def(content.block(world.floor_id(x, y))).is_liquid {
            return;
        }
        let dest = world.floor_id(x, y);
        let dest_overlay = world.overlay_id(x, y);
        let tester = move |w: &dyn EditorGrid, _c: &ContentRegistry, tx: i32, ty: i32| {
            w.floor_id(tx, ty) == dest && w.overlay_id(tx, ty) == dest_overlay
        };
        let mut setter = move |_ed: &mut MapEditor, w: &mut dyn EditorGrid, tx: i32, ty: i32| {
            if w.overlay_id(tx, ty) != draw_block {
                w.set_overlay(tx, ty, draw_block);
            }
        };
        editor.fill(world, content, x, y, false, &tester, &mut setter);
    }
}

/// `EditorTool.line.touchedLine`: Bresenham line with an optional dominant-axis
/// snap (`mode == 1`), then flush (`MapView` release).
#[allow(clippy::too_many_arguments)]
pub fn touched_line(
    editor: &mut MapEditor,
    tool: EditorTool,
    world: &mut dyn EditorGrid,
    content: &ContentRegistry,
    x1: i32,
    y1: i32,
    mut x2: i32,
    mut y2: i32,
) {
    if tool != EditorTool::Line {
        return;
    }
    let mode = editor.tool_modes[tool.index()];
    if mode == 1 {
        if (x2 - x1).abs() > (y2 - y1).abs() {
            y2 = y1;
        } else {
            x2 = x1;
        }
    }
    let mut points = std::mem::take(&mut editor.line_scratch);
    points.clear();
    crate::world::raycast::raycast_each(x1, y1, x2, y2, |x, y| {
        points.push((x, y));
        false
    });
    for &(x, y) in &points {
        if mode == 0 {
            editor.draw_blocks_replace(world, content, x, y);
        } else {
            editor.draw_blocks(world, content, x, y);
        }
    }
    editor.line_scratch = points;
    editor.flush_op();
}

/// Deterministic spray gate (`Mathf.chance(0.012)` in `EditorTool.spray`).
///
/// M0 uses a position hash so the headless oracle is deterministic and
/// allocation-free; the per-frame RNG wiring lands with the input plan (M3).
pub fn spray_chance(brush: f32, x: i32, y: i32) -> bool {
    let h = (x as u32)
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add((y as u32).wrapping_mul(0x85EB_CA6B))
        .wrapping_add(brush.to_bits().wrapping_mul(0xC2B2_AE35));
    ((h >> 8) as f64) / (1u64 << 24) as f64 <= 0.012
}

/// Resolves a block's editor-relevant fields (`EditorBlockInfo` accessor).
pub fn info_of(content: &ContentRegistry, block: BlockId) -> EditorBlockInfo {
    EditorBlockInfo::from_def(content.block(block))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_metadata_matches_upstream() {
        assert_eq!(EditorTool::ALL.len(), EditorTool::COUNT);
        assert_eq!(EditorTool::Line.alt_modes(), ["replace", "orthogonal"]);
        assert_eq!(EditorTool::Fill.alt_modes().len(), 5);
        assert_eq!(EditorTool::Pencil.key(), "b");
        assert!(EditorTool::Eraser.edit());
        assert!(EditorTool::Spray.draggable());
        assert!(!EditorTool::Line.draggable());
        assert!(!EditorTool::Pick.edit());
        assert_eq!(EditorTool::from_name("PENCIL"), Some(EditorTool::Pencil));
        assert_eq!(EditorTool::from_name("nope"), None);
        assert_eq!(EditorTool::default(), EditorTool::Zoom);
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! One world tile (`world/Tile.java`, plan 06 §3.3/§3.4).
//!
//! Rust adaptation (deviation §2.3.1): the building link is an ECS `Entity`
//! handle instead of a `Building` pointer; team/rotation live on the building
//! entity (`Tile.team()`/`Tile.rotation()` delegate upstream), so a tile with no
//! building reports team `0`/rotation `0`. All mutation goes through
//! `world::ops` (plan 06 §3.13).

use bevy_ecs::entity::Entity;

use crate::content::{BlockDef, BlockId, BlockKind, ContentRegistry};

/// Byte layout of the private packed tile data (`Tile.getPackedData`).
///
/// This is **not** a wire format; the native save stores the four fields
/// separately (plan 04 §6.1).
pub const PACK_DATA_LAYOUT: &str =
    "extra_data:i32 | data:u8 | floor_data:u8 | overlay_data:u8 (private to MGRS)";

/// A world tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    /// Tile x (`short` upstream; never widen — plan 06 §3.13/HLP §9).
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Block save data **or** edge darkness for static walls (upstream overload).
    pub data: i8,
    /// Floor save data.
    pub floor_data: i8,
    /// Overlay save data.
    pub overlay_data: i8,
    /// Extra (int) save data.
    pub extra_data: i32,
    /// Wall/block content id (`air` when empty).
    pub block: BlockId,
    /// Floor content id.
    pub floor: BlockId,
    /// Overlay content id.
    pub overlay: BlockId,
    /// Building entity; on a multiblock proxy this points at the center entity.
    pub build: Option<Entity>,
    /// Guard against re-entrant change events (`Tile.changing`).
    pub changing: bool,
}

impl Default for Tile {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            data: 0,
            floor_data: 0,
            overlay_data: 0,
            extra_data: 0,
            block: BlockId::AIR,
            floor: BlockId::AIR,
            overlay: BlockId::AIR,
            build: None,
            changing: false,
        }
    }
}

impl Tile {
    /// An all-`air` tile at `(x, y)` (upstream `Tile(int, int)`).
    pub fn new(x: i16, y: i16) -> Self {
        Self {
            x,
            y,
            ..Self::default()
        }
    }

    /// Packed tile position (`Tile.pos()`).
    pub fn pos(&self) -> crate::world::TilePos {
        crate::world::TilePos::new(self.x, self.y)
    }

    /// Whether this tile is a multiblock center (`Tile.isCenter`).
    ///
    /// The center is the tile whose building entity reports this position; a
    /// tile with no building is trivially a center.
    pub fn is_center_with(&self, center: crate::world::TilePos) -> bool {
        self.build.is_none() || center == self.pos()
    }

    /// Flammability of the floor/block (`Tile.getFlammability`).
    ///
    /// Plan 10 §3.10: for an empty tile this is the floor's `liquidDrop`
    /// flammability (`tar` → oil `1.2`); a tile with a building contributes its
    /// item/liquid flammability upstream, which requires plan-07/08 module
    /// inventories not reachable from a bare `Tile` — the fire system adds the
    /// puddle contribution separately, and the build half stays `0` until the
    /// inventories are threaded through.
    pub fn get_flammability(&self, content: &ContentRegistry) -> f32 {
        if self.block == BlockId::AIR {
            return content
                .block(self.floor)
                .and_then(|def| def.liquid_drop)
                .and_then(|liquid| content.liquid(liquid))
                .map(|def| def.flammability)
                .unwrap_or(0.0);
        }
        0.0
    }

    /// Whether the block is solid (`Block.solid`).
    pub fn solid(&self, content: &ContentRegistry) -> bool {
        content.block(self.block).is_some_and(|def| def.solid)
    }

    /// Whether the tile can be walked through (P0 approximation of
    /// `Tile.passable(Team)`; team ownership checks arrive with 07).
    pub fn passable(&self, content: &ContentRegistry) -> bool {
        !self.solid(content)
    }

    /// Whether the block can be broken (`Tile.breakable()`:
    /// `block.destructible || block.breakable || block.update`; the port does
    /// not model upstream's separate `Block.breakable` flag).
    pub fn breakable(&self, content: &ContentRegistry) -> bool {
        content
            .block(self.block)
            .is_some_and(|def| (def.destructible || def.update) && def.kind != BlockKind::AirBlock)
    }

    /// Whether the tile damages entities (floor hazard; floor data in 02).
    pub fn dangerous(&self, _content: &ContentRegistry) -> bool {
        false
    }

    /// Whether legs can stand on the block (`Block.legSolid`).
    pub fn leg_solid(&self, _content: &ContentRegistry) -> bool {
        false
    }

    /// Whether the block contributes static darkness (`Tile.staticDarkness`:
    /// `block.solid && block.fillsTile && !block.synthetic()`).
    pub fn static_darkness(&self, content: &ContentRegistry) -> bool {
        self.build.is_none()
            && content
                .block(self.block)
                .is_some_and(|def| def.solid && def.fills_tile)
    }

    /// The item dropped when this tile is mined (`Block.drop`).
    pub fn drop(&self, content: &ContentRegistry) -> Option<crate::content::ItemId> {
        content.block(self.block).and_then(|def| def.item_drop)
    }

    /// The item dropped when this wall is mined (`Block.wallDrop`).
    pub fn wall_drop(&self, content: &ContentRegistry) -> Option<crate::content::ItemId> {
        content
            .block(self.block)
            .and_then(|def| def.wall_ore.then_some(def.item_drop).flatten())
    }

    /// Pixel hitbox `[x, y, w, h]` (`Tile.getHitbox`).
    pub fn get_hitbox(&self, content: &ContentRegistry) -> [f32; 4] {
        let size = self.block_size(content);
        let offset = -(size as f32 - 1.0) / 2.0 * 8.0;
        [
            self.x as f32 * 8.0 + offset,
            self.y as f32 * 8.0 + offset,
            size as f32 * 8.0,
            size as f32 * 8.0,
        ]
    }

    /// Multiblock edge size in tiles (`Block.size`).
    pub fn block_size(&self, content: &ContentRegistry) -> i32 {
        content
            .block(self.block)
            .map(|def| def.size)
            .unwrap_or(1)
            .max(1)
    }

    /// `Tile.shouldSaveData`: `floor.saveData || overlay.saveData || block.saveData`.
    pub fn should_save_data(&self, content: &ContentRegistry) -> bool {
        let save = |id: BlockId| content.block(id).is_some_and(|def| def.save_data);
        save(self.floor) || save(self.overlay) || save(self.block)
    }

    /// The private packed data (`Tile.getPackedData`; not a wire format).
    pub fn get_packed_data(&self) -> i64 {
        (self.extra_data as i64 & 0xffff_ffff)
            | ((self.data as u8 as i64) << 32)
            | ((self.floor_data as u8 as i64) << 40)
            | ((self.overlay_data as u8 as i64) << 48)
    }

    /// Restores the private packed data (`Tile.setPackedData`).
    pub fn set_packed_data(&mut self, value: i64) {
        self.extra_data = value as i32;
        self.data = ((value >> 32) & 0xff) as u8 as i8;
        self.floor_data = ((value >> 40) & 0xff) as u8 as i8;
        self.overlay_data = ((value >> 48) & 0xff) as u8 as i8;
    }

    /// Human-readable block name (debug).
    pub fn block_name<'a>(&self, content: &'a ContentRegistry) -> &'a str {
        content
            .block(self.block)
            .map(|def| def.name.as_str())
            .unwrap_or("air")
    }

    /// Raw block definition, if registered.
    pub fn block_def<'a>(&self, content: &'a ContentRegistry) -> Option<&'a BlockDef> {
        content.block(self.block)
    }
}

/// Whether a kind is treated as static (darkens neighbours) upstream.
pub fn is_static_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::StaticWall
            | BlockKind::StaticProp
            | BlockKind::StaticTree
            | BlockKind::Cliff
            | BlockKind::Prop
            | BlockKind::TallBlock
            | BlockKind::SeaBush
            | BlockKind::Seaweed
            | BlockKind::ColoredWall
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tile_is_air_center() {
        let tile = Tile::new(2, 3);
        assert_eq!(tile.block, BlockId::AIR);
        assert_eq!(tile.floor, BlockId::AIR);
        assert_eq!(tile.overlay, BlockId::AIR);
        assert!(tile.build.is_none());
        assert!(tile.is_center_with(crate::world::TilePos::new(9, 9)));
        assert_eq!(tile.pos(), crate::world::TilePos::new(2, 3));
    }

    #[test]
    fn packed_data_roundtrip() {
        let mut tile = Tile::new(0, 0);
        tile.data = -1;
        tile.floor_data = 0x11;
        tile.overlay_data = 0x22u8 as i8;
        tile.extra_data = 0x0BAD_F00D_u32 as i32;
        let packed = tile.get_packed_data();
        let mut other = Tile::new(0, 0);
        other.set_packed_data(packed);
        assert_eq!(other.data, tile.data);
        assert_eq!(other.floor_data, tile.floor_data);
        assert_eq!(other.overlay_data, tile.overlay_data);
        assert_eq!(other.extra_data, tile.extra_data);
    }
}

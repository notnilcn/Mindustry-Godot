// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `CanvasBlock`/`CanvasBuild` (plan 13 M4, PAR-0225).
//!
//! Ported from `core/src/mindustry/world/blocks/logic/CanvasBlock.java`. The
//! canvas stores LSB-first bit-packed palette indices; `getPixel(pos)` returns
//! NaN out of range and `setPixel(pos, index)` validates the palette index
//! (`:143-155`). The pixmap/texture half lives in the T13 render pass.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::io::entity::{EntityReader, EntityWriter};
use crate::world::behavior::BuildingBehavior;
use crate::world::config::ConfigValue;

use super::canvas_def_of;

/// `CanvasBlock.palette` (RGBA8888, `Blocks.java:6955-6957`).
pub const PALETTE: [u32; 8] = [
    0x3629_44ff,
    0xc45d_9fff,
    0xe39a_acff,
    0xf0da_b1ff,
    0x6461_c2ff,
    0x2ba9_b4ff,
    0x93d4_b5ff,
    0xf0f6_e8ff,
];

/// Runtime canvas pixels (`CanvasBuild`).
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct CanvasBlockState {
    /// Bit-packed palette indices (`CanvasBuild.data`).
    pub data: Vec<u8>,
    /// 4-bit same-block neighbour mask (`CanvasBuild.blending`).
    pub blending: u8,
    /// Bits per pixel (`log2(nextPowerOfTwo(palette.length))`).
    pub bits_per_pixel: u8,
    /// Canvas width/height in pixels.
    pub canvas_size: i32,
    /// `CanvasBuild.invalidated` (the view rebuilds its texture).
    pub invalidated: bool,
}

impl CanvasBlockState {
    /// Creates an empty canvas for a `canvasSize`/`bpp`.
    pub fn new(canvas_size: i32, bits_per_pixel: u8) -> Self {
        let pixels = (canvas_size.max(0) as usize) * (canvas_size.max(0) as usize);
        let bytes = (pixels * bits_per_pixel as usize).div_ceil(8);
        Self {
            data: vec![0u8; bytes],
            blending: 0,
            bits_per_pixel,
            canvas_size,
            invalidated: true,
        }
    }

    /// `CanvasBuild.getPixel(pos)`: NaN out of range (`:150-155`).
    pub fn get_pixel(&self, pos: i32) -> f64 {
        let pixels = self.canvas_size.max(0) * self.canvas_size.max(0);
        if pos < 0 || pos >= pixels {
            return f64::NAN;
        }
        self.get_byte(pos * i32::from(self.bits_per_pixel)) as f64
    }

    /// `CanvasBuild.setPixel(pos, index)` (`:143-149`).
    pub fn set_pixel(&mut self, pos: i32, index: i32) {
        let pixels = self.canvas_size.max(0) * self.canvas_size.max(0);
        if pos < 0 || pos >= pixels || index < 0 || index >= PALETTE.len() as i32 {
            return;
        }
        self.set_byte(pos * i32::from(self.bits_per_pixel), index);
        self.invalidated = true;
    }

    /// `CanvasBuild.getByte(data, bitOffset)`: LSB-first bit reads (`:186-195`).
    pub fn get_byte(&self, bit_offset: i32) -> i32 {
        let mut result = 0i32;
        for i in 0..i32::from(self.bits_per_pixel) {
            let word = (i + bit_offset) >> 3;
            let bit = 1 << ((i + bit_offset) & 7);
            if self.data.get(word.max(0) as usize).copied().unwrap_or(0) & bit as u8 != 0 {
                result |= 1 << i;
            }
        }
        result
    }

    /// `CanvasBuild.setByte(bytes, bitOffset, value)` (`:216-227`).
    pub fn set_byte(&mut self, bit_offset: i32, value: i32) {
        for i in 0..i32::from(self.bits_per_pixel) {
            let word = ((i + bit_offset) >> 3).max(0) as usize;
            let bit = 1u8 << ((i + bit_offset) & 7);
            if let Some(byte) = self.data.get_mut(word) {
                if (value >> i) & 1 == 0 {
                    *byte &= !bit;
                } else {
                    *byte |= bit;
                }
            }
        }
    }

    /// `CanvasBlock.makePixmap`: pixel `i` as RGBA8888 (row 0 = top).
    pub fn pixel_rgba(&self, pos: i32) -> u32 {
        let index = self.get_byte(pos * i32::from(self.bits_per_pixel)).max(0) as usize;
        PALETTE
            .get(index.min(PALETTE.len() - 1))
            .copied()
            .unwrap_or(0)
    }
}

/// One visible canvas row for the render pass (T14).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasDrawRow {
    /// Canvas tile x (texture key).
    pub x: i16,
    /// Canvas tile y (texture key).
    pub y: i16,
    /// Draw centre x (world pixels).
    pub cx: f32,
    /// Draw centre y (world pixels).
    pub cy: f32,
    /// Draw size (`size * 8 - pad`, Java `Draw.rect(Tmp.tr1, x, y, size*tilesize - pad, ...)`).
    pub size: f32,
    /// 4-bit same-block neighbour mask (`CanvasBuild.blending`).
    pub blending: u8,
    /// Canvas pixels per axis (`canvasSize`).
    pub pixels: i32,
    /// Whether the canvas was invalidated since the last texture upload.
    pub invalidated: bool,
}

/// `CanvasBuild.draw` selection for one camera rect (T14).
pub fn canvas_rows(
    world: &World,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
) -> Vec<CanvasDrawRow> {
    let mut canvases: Vec<(u64, Entity)> = world
        .iter_entities()
        .filter(|entity| entity.contains::<CanvasBlockState>())
        .map(|entity| {
            let seq = entity
                .get::<crate::ecs::EntitySeq>()
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX);
            (seq, entity.id())
        })
        .collect();
    canvases.sort_by_key(|(seq, e)| (*seq, e.index().index()));
    let mut rows = Vec::new();
    for (_, e) in canvases {
        let Some(building) = world.get::<crate::entities::comp::Building>(e) else {
            continue;
        };
        let Some(def) = canvas_def_of(world, e) else {
            continue;
        };
        let block_size = crate::logic::blocks::block_size_of(world, e).unwrap_or(2);
        let centre_offset = ((block_size + 1) % 2) as f32 * 4.0;
        let cx = f32::from(building.tile.x()) * 8.0 + centre_offset;
        let cy = f32::from(building.tile.y()) * 8.0 + centre_offset;
        if cx < min_x || cx > max_x || cy < min_y || cy > max_y {
            continue;
        }
        let blend = world
            .get::<CanvasBlockState>(e)
            .map(|state| state.blending)
            .unwrap_or(0);
        // `pad = blending == 0 ? padding : 0` (`CanvasBlock.java:271`).
        let pad = if blend == 0 {
            def.padding.max(0.0)
        } else {
            0.0
        };
        rows.push(CanvasDrawRow {
            x: building.tile.x(),
            y: building.tile.y(),
            cx,
            cy,
            size: block_size as f32 * 8.0 - pad,
            blending: blend,
            pixels: def.canvas_size.max(0),
            invalidated: world
                .get::<CanvasBlockState>(e)
                .is_some_and(|state| state.invalidated),
        });
    }
    rows
}

/// `CanvasBlock.CanvasBuild` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct CanvasBehavior;

impl BuildingBehavior for CanvasBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        let def = canvas_def_of(world, e).unwrap_or_default();
        world
            .entity_mut(e)
            .insert(CanvasBlockState::new(def.canvas_size, def.bits_per_pixel));
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        // `config(byte[].class)` (`CanvasBlock.java:48-53`): copy when the
        // lengths match and mark the texture invalid.
        if let ConfigValue::Bytes(data) = value
            && let Some(mut state) = world.get_mut::<CanvasBlockState>(e)
            && state.data.len() == data.len()
        {
            state.data = data.to_vec();
            state.invalidated = true;
        }
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        // `CanvasBuild.config()` returns `data` (`CanvasBlock.java:327-329`).
        world
            .get::<CanvasBlockState>(e)
            .map(|state| ConfigValue::Bytes(state.data.clone().into()))
            .unwrap_or(ConfigValue::None)
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        0
    }

    fn write(&self, world: &World, e: Entity, w: &mut EntityWriter) {
        let data = world
            .get::<CanvasBlockState>(e)
            .map(|state| state.data.clone())
            .unwrap_or_default();
        w.i(data.len() as i32);
        w.bytes(&data);
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut EntityReader, _revision: u8) {
        let Ok(len) = r.i() else {
            return;
        };
        let len = len.max(0) as usize;
        let Ok(bytes) = r.bytes(len) else {
            return;
        };
        let bytes = bytes.to_vec();
        if let Some(mut state) = world.get_mut::<CanvasBlockState>(e)
            && state.data.len() == len
        {
            state.data = bytes;
            state.invalidated = true;
        }
    }
}

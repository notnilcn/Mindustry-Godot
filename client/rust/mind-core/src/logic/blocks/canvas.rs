// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `CanvasBlock`/`CanvasBuild` (plan 13 M4).
//!
//! Ported from `core/src/mindustry/world/blocks/logic/CanvasBlock.java`. The
//! canvas stores bit-packed palette indices; `getPixel` returns NaN out of
//! range and `setPixel` validates the palette index. Pixmap/texture is plan 16.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::io::entity::{EntityReader, EntityWriter};
use crate::world::behavior::BuildingBehavior;
use crate::world::config::ConfigValue;

use super::canvas_def_of;

/// Runtime canvas pixels (`CanvasBuild`).
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct CanvasBlockState {
    /// Bit-packed pixel data.
    pub data: Vec<u8>,
    /// Blending mode byte.
    pub blending: u8,
    /// Bits per pixel (2^palette size).
    pub bits_per_pixel: u8,
    /// Canvas width/height in pixels.
    pub canvas_size: i32,
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
        }
    }

    /// `CanvasBuild.getPixel(x, y)`: NaN out of range.
    pub fn get_pixel(&self, x: i32, y: i32) -> f64 {
        if x < 0 || y < 0 || x >= self.canvas_size || y >= self.canvas_size {
            return f64::NAN;
        }
        let index = (y * self.canvas_size + x) as usize;
        let bit = index * self.bits_per_pixel as usize;
        let byte = bit / 8;
        let offset = bit % 8;
        let mask = (1u16 << self.bits_per_pixel) - 1;
        let value = ((self.data.get(byte).copied().unwrap_or(0) as u16)
            | ((self.data.get(byte + 1).copied().unwrap_or(0) as u16) << 8))
            >> offset
            & mask;
        value as f64
    }

    /// `CanvasBuild.setPixel(x, y, color)`: bounds/palette validated.
    pub fn set_pixel(&mut self, x: i32, y: i32, color: u8) {
        if x < 0 || y < 0 || x >= self.canvas_size || y >= self.canvas_size {
            return;
        }
        let max_index = 1u16.checked_shl(self.bits_per_pixel as u32).unwrap_or(0) as u8;
        if color >= max_index.max(1) {
            return;
        }
        let index = (y * self.canvas_size + x) as usize;
        let bit = index * self.bits_per_pixel as usize;
        let byte = bit / 8;
        let offset = bit % 8;
        let mask = ((1u16 << self.bits_per_pixel) - 1) as u8;
        if let Some(value) = self.data.get_mut(byte) {
            *value = (*value & !(mask << offset)) | (color << offset);
        }
        if offset + self.bits_per_pixel as usize > 8
            && let Some(value) = self.data.get_mut(byte + 1)
        {
            *value &= !(mask >> (8 - offset));
        }
    }
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
        let def = canvas_def_of(world, e).unwrap_or_default();
        if !super::accessible(false, &super::DefaultLogicRules) {
            return;
        }
        if let ConfigValue::Bytes(data) = value
            && let Some(mut state) = world.get_mut::<CanvasBlockState>(e)
        {
            let expected = ((def.canvas_size.max(0) as usize).pow(2) * def.bits_per_pixel as usize)
                .div_ceil(8);
            if data.len() == expected {
                state.data = data.to_vec();
            }
        }
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
        }
    }
}

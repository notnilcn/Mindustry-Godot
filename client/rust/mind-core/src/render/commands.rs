// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Reusable draw-command buffer (plan 16 §6.2).
//!
//! This is the render-side POD payload the `MindWorldRenderer` replays through
//! Godot bands. Plan 07 owns the `DrawBlock`/`DrawSpec` descriptors that emit
//! into it; plans 10/11/17 push external draw state at their named layers. The
//! buffer is reused across frames via [`CommandBuffer::clear`] and must not grow
//! in steady state (plan 16 §7.4 alloc audit).

use smallvec::SmallVec;

use crate::render::ids::RegionId;

/// Blend mode, mapped to `CanvasItemMaterial.blend_mode` by the Godot layer
/// (plan 16 §3.4 / D16-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Blend {
    /// `BlendMode::MIX` (normal alpha).
    #[default]
    Normal,
    /// `BlendMode::ADD`.
    Additive,
    /// `BlendMode::MUL` (multiply).
    Multiply,
    /// Blending disabled (opaque write).
    Disabled,
}

/// Shape primitive kinds (`Drawf`/`Lines`/`Fill`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    /// Filled circle.
    Circle,
    /// Filled rectangle.
    Rect,
    /// Filled polygon.
    Poly,
    /// Circle outline.
    CircleOutline,
    /// Rect outline.
    RectOutline,
    /// Line strip.
    Line,
}

/// Line primitive kinds (`LineKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineKind {
    /// A single segment.
    Line,
    /// A polyline.
    Poly,
    /// A dashed line.
    Dash,
    /// An arc.
    Arc,
}

/// Fill primitive kinds (`Fill`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FillKind {
    /// A filled rectangle.
    Rect,
    /// A filled polygon.
    Poly,
    /// A filled triangle.
    Triangle,
    /// A filled circle.
    Circle,
}

/// One render command (plan 16 §6.2). POD only; no strings on the hot path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrawCmd {
    /// Set the current z.
    SetZ(f32),
    /// Set the current blend.
    SetBlend(Blend),
    /// Set the current color (RGBA8888).
    SetColor(u32),
    /// Set a mix color and strength (`Draw.mixcol`).
    SetMixColor(u32, f32),
    /// Set the current alpha multiplier.
    SetAlpha(f32),
    /// A sprite quad.
    Sprite {
        /// Atlas region.
        region: RegionId,
        /// Center x in world pixels.
        x: f32,
        /// Center y in world pixels.
        y: f32,
        /// Quad width.
        w: f32,
        /// Quad height.
        h: f32,
        /// Origin x in `[0, 1]`.
        ox: f32,
        /// Origin y in `[0, 1]`.
        oy: f32,
        /// Rotation in degrees.
        rot: f32,
        /// Tint (RGBA8888).
        color: u32,
        /// Blend mode.
        blend: Blend,
    },
    /// A shape primitive.
    Shape {
        /// Kind.
        kind: ShapeKind,
        /// Up to 8 parameters (`x, y, w, h, rot, …`).
        params: [f32; 8],
        /// Color (RGBA8888).
        color: u32,
    },
    /// A line primitive.
    Lines {
        /// Kind.
        kind: LineKind,
        /// Up to 8 parameters.
        params: [f32; 8],
        /// Color (RGBA8888).
        color: u32,
        /// Stroke width.
        stroke: f32,
    },
    /// A filled primitive.
    Fill {
        /// Kind.
        kind: FillKind,
        /// Up to 6 parameters.
        params: [f32; 6],
        /// Color (RGBA8888).
        color: u32,
    },
}

/// Reusable per-frame command buffer.
#[derive(Debug, Default)]
pub struct CommandBuffer {
    /// Emitted commands, in emission order.
    pub commands: Vec<DrawCmd>,
    /// Current z.
    pub z: f32,
    /// Current blend.
    pub blend: Blend,
    /// Current tint (RGBA8888).
    pub color: u32,
}

impl CommandBuffer {
    /// Empty white/opaque state.
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
            z: 0.0,
            blend: Blend::Normal,
            color: 0xffff_ffff,
        }
    }

    /// Clears for the next frame without freeing capacity.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.z = 0.0;
        self.blend = Blend::Normal;
        self.color = 0xffff_ffff;
    }

    /// Sets the current z and pushes a `SetZ`.
    pub fn set_z(&mut self, z: f32) {
        self.z = z;
        self.commands.push(DrawCmd::SetZ(z));
    }

    /// Sets the current blend and pushes a `SetBlend`.
    pub fn set_blend(&mut self, blend: Blend) {
        self.blend = blend;
        self.commands.push(DrawCmd::SetBlend(blend));
    }

    /// Sets the current color and pushes a `SetColor`.
    pub fn set_color(&mut self, color: u32) {
        self.color = color;
        self.commands.push(DrawCmd::SetColor(color));
    }

    /// Pushes a sprite at the current z/blend/color.
    #[allow(clippy::too_many_arguments)]
    pub fn sprite(
        &mut self,
        region: RegionId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rot: f32,
        color: u32,
    ) {
        self.commands.push(DrawCmd::Sprite {
            region,
            x,
            y,
            w,
            h,
            ox: 0.5,
            oy: 0.5,
            rot,
            color,
            blend: self.blend,
        });
    }

    /// Number of commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Reserves capacity for `additional` more commands so steady-state frames
    /// never reallocate (alloc audit).
    pub fn warm(&mut self, additional: usize) {
        self.commands.reserve(additional);
    }
}

/// A `SmallVec` of sprite commands, used by the floor/block bakers before they
/// are pushed into a [`CommandBuffer`].
pub type SpriteBatch = SmallVec<[DrawCmd; 16]>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_alloc_after_warmup() {
        let mut buffer = CommandBuffer::new();
        buffer.warm(64);
        let capacity = buffer.commands.capacity();
        for i in 0..64 {
            buffer.sprite(RegionId(i), 0.0, 0.0, 8.0, 8.0, 0.0, 0xffff_ffff);
        }
        assert_eq!(buffer.commands.capacity(), capacity);
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.commands.capacity(), capacity);
    }
}

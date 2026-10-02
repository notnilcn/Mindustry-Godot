// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawPrim`/`DrawProgram` — the plan-16↔plan-17 FX draw contract.
//!
//! Java effects call Arc `Draw`/`Fill`/`Lines` immediately. The port instead
//! compiles each effect into a flat, sortable [`DrawProgram`] of geometry that
//! `mind-gdext` executes and batches. This keeps effect geometry testable
//! headlessly (golden programs) and lets plan 16 batch by `(z, blend, region)`.
//! Godot-free; never feeds simulation state.

use smallvec::SmallVec;

use crate::content::Rgba;
use crate::render::layer::Layer;

/// Blend mode (mirrors plan 16 [`crate::render::commands::Blend`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Blending {
    /// Normal alpha blending.
    #[default]
    Normal,
    /// Additive blending (`Drawf.additive`).
    Additive,
    /// Multiply blending.
    Multiply,
}

/// An atlas region name (plan 03 ABI). Static names cover the vanilla
/// catalogue; dynamic names are interned by the resolver at the gdext boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegionKey(pub &'static str);

impl RegionKey {
    /// The missing-region sentinel (`find_or(name, "error")`).
    pub const ERROR: RegionKey = RegionKey("error");
    /// The default particle region (`ParticleEffect.region`).
    pub const CIRCLE: RegionKey = RegionKey("circle");
}

impl Default for RegionKey {
    fn default() -> Self {
        RegionKey::ERROR
    }
}

/// A loose (non-atlas) texture reference (`sprites/distortAlpha.png`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct TextureKey(pub &'static str);

/// A shader reference (`caustics`, …) executed by plan 16.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ShaderKey(pub &'static str);

/// One FX geometry primitive (plan 17 §3.6). POD; no allocation beyond the
/// small point buffers.
#[derive(Clone, Debug, PartialEq)]
pub enum PrimKind {
    /// A textured quad.
    Region {
        /// Atlas region.
        region: RegionKey,
        /// Center x (world pixels).
        x: f32,
        /// Center y (world pixels).
        y: f32,
        /// Quad width.
        w: f32,
        /// Quad height.
        h: f32,
        /// Rotation in degrees.
        rotation_deg: f32,
        /// Origin in `[0, 1]`.
        origin: (f32, f32),
        /// Tint.
        color: Rgba,
        /// Optional mix color (`Draw.mixcol`).
        mix: Option<Rgba>,
        /// Whether the region wraps (`TextureRegion.scroll`).
        wrap: bool,
    },
    /// `Fill.rect`.
    Rect {
        /// Center x.
        x: f32,
        /// Center y.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
        /// Tint.
        color: Rgba,
    },
    /// `Fill.circle` / `Lines.circle`.
    Circle {
        /// Center x.
        x: f32,
        /// Center y.
        y: f32,
        /// Radius.
        r: f32,
        /// Fill (`true`) or stroke (`false`).
        fill: bool,
        /// Stroke width when `fill == false`.
        stroke: f32,
        /// Tint.
        color: Rgba,
    },
    /// `Fill.poly` / `Lines.poly`.
    Poly {
        /// Center x.
        x: f32,
        /// Center y.
        y: f32,
        /// Side count.
        sides: u16,
        /// Radius.
        r: f32,
        /// Rotation in degrees.
        rotation_deg: f32,
        /// Fill (`true`) or stroke (`false`).
        fill: bool,
        /// Stroke width when `fill == false`.
        stroke: f32,
        /// Tint.
        color: Rgba,
    },
    /// `Fill.poly` with explicit points.
    Polygon {
        /// Point list.
        points: SmallVec<[(f32, f32); 12]>,
        /// Fill (`true`) or stroke (`false`).
        fill: bool,
        /// Stroke width when `fill == false`.
        stroke: f32,
        /// Tint.
        color: Rgba,
    },
    /// `Drawf.tri`.
    Tri {
        /// Center x.
        x: f32,
        /// Center y.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
        /// Rotation in degrees.
        rotation_deg: f32,
        /// Tint.
        color: Rgba,
    },
    /// `Lines.line` / `Lines.lineAngle`.
    Line {
        /// Start x.
        x1: f32,
        /// Start y.
        y1: f32,
        /// End x.
        x2: f32,
        /// End y.
        y2: f32,
        /// Stroke width.
        stroke: f32,
        /// Tint.
        color: Rgba,
        /// Whether to draw round caps.
        cap: bool,
    },
    /// `Lines` polyline.
    Polyline {
        /// Point list.
        points: SmallVec<[(f32, f32); 12]>,
        /// Stroke width.
        stroke: f32,
        /// Tint.
        color: Rgba,
    },
    /// `Lines.arc`.
    Arc {
        /// Center x.
        x: f32,
        /// Center y.
        y: f32,
        /// Radius.
        r: f32,
        /// Start angle in degrees.
        start_deg: f32,
        /// Sweep in degrees.
        sweep_deg: f32,
        /// Stroke width.
        stroke: f32,
        /// Tint.
        color: Rgba,
    },
    /// `NoiseEffect.drawNoise` layer (executed by a native shader blit).
    NoiseLayer {
        /// Loose noise texture.
        texture: TextureKey,
        /// Screen rect `[x, y, w, h]`.
        rect: [f32; 4],
        /// Tint.
        tint: Rgba,
        /// Opacity.
        opacity: f32,
        /// Scroll `[x, y]`.
        scroll: [f32; 2],
        /// Layer offset.
        offset: f32,
    },
    /// A generic shader blit (caustics, env) executed by plan 16.
    ShaderBlit {
        /// Shader key.
        shader: ShaderKey,
    },
    /// `Drawf.light` (routed to 16's `LightRenderer`, never drawn as geometry).
    Light {
        /// Center x.
        x: f32,
        /// Center y.
        y: f32,
        /// Radius.
        radius: f32,
        /// Tint.
        color: Rgba,
        /// Opacity.
        opacity: f32,
    },
}

impl PrimKind {
    /// The blend mode implied by this primitive (`Drawf.additive` sets it).
    pub fn default_blending(&self) -> Blending {
        Blending::Normal
    }
}

/// One sorted draw primitive.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawPrim {
    /// Z layer (`Draw.z`).
    pub z: f32,
    /// Blend mode.
    pub blend: Blending,
    /// Geometry.
    pub kind: PrimKind,
}

impl DrawPrim {
    /// Builds a primitive at a layer with normal blending.
    pub fn at(z: f32, kind: PrimKind) -> Self {
        Self {
            z,
            blend: kind.default_blending(),
            kind,
        }
    }

    /// Sets additive blending (`Drawf.additive`).
    pub fn additive(mut self) -> Self {
        self.blend = Blending::Additive;
        self
    }

    /// Folds this primitive into a deterministic FNV-1a hash.
    fn hash_into(&self, hash: &mut u64) {
        mix_f32_(hash, self.z);
        mix_u64_(hash, blend_code(self.blend) as u64);
        hash_kind(&self.kind, hash);
    }
}

/// FNV-1a prime.
const FNV_PRIME: u64 = 0x1000_0000_01b3;

fn mix_u64_(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn mix_f32_(hash: &mut u64, value: f32) {
    for b in value.to_bits().to_le_bytes() {
        mix_u64_(hash, b as u64);
    }
}

fn mix_color_(hash: &mut u64, color: Rgba) {
    mix_f32_(hash, color.r);
    mix_f32_(hash, color.g);
    mix_f32_(hash, color.b);
    mix_f32_(hash, color.a);
}

macro_rules! mixv {
    ($hash:expr, $value:expr) => {
        mix_f32_($hash, $value)
    };
}

macro_rules! mixc {
    ($hash:expr, $value:expr) => {
        mix_color_($hash, $value)
    };
}

fn blend_code(blend: Blending) -> u8 {
    match blend {
        Blending::Normal => 0,
        Blending::Additive => 1,
        Blending::Multiply => 2,
    }
}

fn hash_str(hash: &mut u64, value: &str) {
    for &b in value.as_bytes() {
        *hash ^= b as u64;
        *hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    *hash ^= 0xff;
    *hash = hash.wrapping_mul(0x1000_0000_01b3);
}

fn hash_kind(kind: &PrimKind, hash: &mut u64) {
    match kind {
        PrimKind::Region {
            region,
            x,
            y,
            w,
            h,
            rotation_deg,
            origin,
            color,
            mix,
            wrap,
        } => {
            *hash ^= 1;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            hash_str(hash, region.0);
            mixv!(hash, *x);
            mixv!(hash, *y);
            mixv!(hash, *w);
            mixv!(hash, *h);
            mixv!(hash, *rotation_deg);
            mixv!(hash, origin.0);
            mixv!(hash, origin.1);
            mixc!(hash, *color);
            if let Some(m) = mix {
                *hash ^= 1;
                *hash = hash.wrapping_mul(0x1000_0000_01b3);
                mixc!(hash, *m);
            }
            *hash ^= u64::from(*wrap);
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        PrimKind::Rect { x, y, w, h, color } => {
            *hash ^= 2;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            mixv!(hash, *x);
            mixv!(hash, *y);
            mixv!(hash, *w);
            mixv!(hash, *h);
            mixc!(hash, *color);
        }
        PrimKind::Circle {
            x,
            y,
            r,
            fill,
            stroke,
            color,
        } => {
            *hash ^= 3;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            mixv!(hash, *x);
            mixv!(hash, *y);
            mixv!(hash, *r);
            mixv!(hash, *stroke);
            *hash ^= u64::from(*fill);
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            mixc!(hash, *color);
        }
        PrimKind::Poly {
            x,
            y,
            sides,
            r,
            rotation_deg,
            fill,
            stroke,
            color,
        } => {
            *hash ^= 4;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            mixv!(hash, *x);
            mixv!(hash, *y);
            mixv!(hash, *r);
            mixv!(hash, *rotation_deg);
            mixv!(hash, *stroke);
            *hash ^= *sides as u64;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            *hash ^= u64::from(*fill);
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            mixc!(hash, *color);
        }
        PrimKind::Polygon {
            points,
            fill,
            stroke,
            color,
        } => {
            *hash ^= 5;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            for p in points {
                mixv!(hash, p.0);
                mixv!(hash, p.1);
            }
            mixv!(hash, *stroke);
            *hash ^= u64::from(*fill);
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            mixc!(hash, *color);
        }
        PrimKind::Tri {
            x,
            y,
            w,
            h,
            rotation_deg,
            color,
        } => {
            *hash ^= 6;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            mixv!(hash, *x);
            mixv!(hash, *y);
            mixv!(hash, *w);
            mixv!(hash, *h);
            mixv!(hash, *rotation_deg);
            mixc!(hash, *color);
        }
        PrimKind::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            color,
            cap,
        } => {
            *hash ^= 7;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            mixv!(hash, *x1);
            mixv!(hash, *y1);
            mixv!(hash, *x2);
            mixv!(hash, *y2);
            mixv!(hash, *stroke);
            mixc!(hash, *color);
            *hash ^= u64::from(*cap);
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        PrimKind::Polyline {
            points,
            stroke,
            color,
        } => {
            *hash ^= 8;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            for p in points {
                mixv!(hash, p.0);
                mixv!(hash, p.1);
            }
            mixv!(hash, *stroke);
            mixc!(hash, *color);
        }
        PrimKind::Arc {
            x,
            y,
            r,
            start_deg,
            sweep_deg,
            stroke,
            color,
        } => {
            *hash ^= 9;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            mixv!(hash, *x);
            mixv!(hash, *y);
            mixv!(hash, *r);
            mixv!(hash, *start_deg);
            mixv!(hash, *sweep_deg);
            mixv!(hash, *stroke);
            mixc!(hash, *color);
        }
        PrimKind::NoiseLayer {
            texture,
            rect,
            tint,
            opacity,
            scroll,
            offset,
        } => {
            *hash ^= 10;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            hash_str(hash, texture.0);
            for v in rect {
                mixv!(hash, *v);
            }
            for v in scroll {
                mixv!(hash, *v);
            }
            mixv!(hash, *opacity);
            mixv!(hash, *offset);
            mixc!(hash, *tint);
        }
        PrimKind::ShaderBlit { shader } => {
            *hash ^= 11;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            hash_str(hash, shader.0);
        }
        PrimKind::Light {
            x,
            y,
            radius,
            color,
            opacity,
        } => {
            *hash ^= 12;
            *hash = hash.wrapping_mul(0x1000_0000_01b3);
            mixv!(hash, *x);
            mixv!(hash, *y);
            mixv!(hash, *radius);
            mixv!(hash, *opacity);
            mixc!(hash, *color);
        }
    }
}

/// One effect's compiled geometry for a rendered frame.
#[derive(Clone, Debug, Default)]
pub struct DrawProgram {
    /// Emission order; sorted by `(z, insertion)` in [`DrawProgram::sort`].
    pub prims: Vec<DrawPrim>,
    /// `Effect.render` may return a new lifetime (`trailFade`); `None` = keep.
    pub lifetime_override: Option<f32>,
    /// The effect's clip radius.
    pub clip: f32,
}

impl DrawProgram {
    /// Empty program.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears for reuse without freeing capacity.
    pub fn clear(&mut self) {
        self.prims.clear();
        self.lifetime_override = None;
        self.clip = 0.0;
    }

    /// Pushes a primitive in emission order.
    pub fn push(&mut self, prim: DrawPrim) {
        self.prims.push(prim);
    }

    /// Number of primitives.
    pub fn len(&self) -> usize {
        self.prims.len()
    }

    /// Whether the program is empty.
    pub fn is_empty(&self) -> bool {
        self.prims.is_empty()
    }

    /// Stable-sorts by z (`(z, insertion index)`).
    pub fn sort(&mut self) {
        self.prims
            .sort_by(|a, b| a.z.partial_cmp(&b.z).unwrap_or(std::cmp::Ordering::Equal));
    }

    /// Deterministic FNV-1a hash of the (assumed sorted) program.
    pub fn hash(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        hash ^= self.prims.len() as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
        for prim in &self.prims {
            prim.hash_into(&mut hash);
        }
        if let Some(lifetime) = self.lifetime_override {
            for b in lifetime.to_bits().to_le_bytes() {
                hash ^= b as u64;
                hash = hash.wrapping_mul(0x1000_0000_01b3);
            }
        }
        hash
    }

    /// The set of distinct z values used (batch key helper for tests).
    pub fn layers(&self) -> Vec<f32> {
        let mut out: Vec<f32> = Vec::new();
        for prim in &self.prims {
            if out.last() != Some(&prim.z) && !out.contains(&prim.z) {
                out.push(prim.z);
            }
        }
        out
    }
}

/// The default FX z when an [`EffectDef`](crate::fx::def::EffectDef) has no layer.
pub const DEFAULT_LAYER: f32 = Layer::Effect.z();

/// `MultiMesh2D` instance threshold (plan 17 §3.14).
pub const MULTIMESH_THRESHOLD: u32 = 256;
/// `GPUParticles2D` instance threshold (plan 17 §3.14).
pub const GPUPARTICLES_THRESHOLD: u32 = 4000;
/// Draw-call target at the `mid` profile (plan 17 §7d).
pub const MAX_DRAW_CALLS_TARGET: u32 = 200;

#[cfg(test)]
mod tests {
    use super::*;

    fn circ(x: f32, z: f32) -> DrawPrim {
        DrawPrim::at(
            z,
            PrimKind::Circle {
                x,
                y: 0.0,
                r: 1.0,
                fill: true,
                stroke: 0.0,
                color: Rgba::WHITE,
            },
        )
    }

    #[test]
    fn sort_is_stable_by_z() {
        let mut program = DrawProgram::new();
        program.push(circ(0.0, 20.0));
        program.push(circ(1.0, 10.0));
        program.push(circ(2.0, 10.0));
        program.push(circ(3.0, 30.0));
        program.sort();
        let xs: Vec<f32> = program
            .prims
            .iter()
            .map(|p| match &p.kind {
                PrimKind::Circle { x, .. } => *x,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(xs, vec![1.0, 2.0, 0.0, 3.0]);
    }

    #[test]
    fn hash_is_deterministic_and_order_sensitive() {
        let mut a = DrawProgram::new();
        a.push(circ(1.0, 10.0));
        a.push(circ(2.0, 10.0));
        let mut b = DrawProgram::new();
        b.push(circ(1.0, 10.0));
        b.push(circ(2.0, 10.0));
        assert_eq!(a.hash(), b.hash());
        let mut c = DrawProgram::new();
        c.push(circ(2.0, 10.0));
        c.push(circ(1.0, 10.0));
        assert_ne!(a.hash(), c.hash());
    }

    #[test]
    fn clear_retains_capacity() {
        let mut program = DrawProgram::new();
        for i in 0..32 {
            program.push(circ(i as f32, 10.0));
        }
        let cap = program.prims.capacity();
        program.clear();
        assert!(program.is_empty());
        assert_eq!(program.prims.capacity(), cap);
    }
}

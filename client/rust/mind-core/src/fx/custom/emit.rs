// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! [`FxEmit`]: the Arc-like primitive vocabulary custom bodies write to.
//!
//! Keeps a small `Draw`-style state (`color`, `mix`, `stroke`, additive) and
//! pushes [`DrawPrim`]s at a fixed z. No allocation except the program buffer.

use crate::content::Rgba;
use crate::render::draw::{Blending, DrawPrim, DrawProgram, PrimKind, RegionKey};

use super::super::angles::{trnsx, trnsy};

/// Emitter handed to a [`super::CustomBody`].
pub struct FxEmit<'a> {
    /// The program being built.
    pub program: &'a mut DrawProgram,
    /// Layer z for every emitted primitive.
    pub z: f32,
    current: Rgba,
    mix: Option<Rgba>,
    stroke: f32,
    blend: Blending,
}

impl<'a> FxEmit<'a> {
    /// Builds an emitter for a program at `z`.
    pub fn new(program: &'a mut DrawProgram, z: f32) -> Self {
        Self {
            program,
            z,
            current: Rgba::WHITE,
            mix: None,
            stroke: 1.0,
            blend: Blending::Normal,
        }
    }

    /// `Draw.color(Color)`.
    pub fn color(&mut self, color: impl Into<Rgba>) {
        self.current = color.into();
    }

    /// `Draw.color(from, to, f)`.
    pub fn color_lerp(&mut self, from: impl Into<Rgba>, to: impl Into<Rgba>, f: f32) {
        let from = from.into();
        let to = to.into();
        self.current = Rgba::new(
            from.r + (to.r - from.r) * f,
            from.g + (to.g - from.g) * f,
            from.b + (to.b - from.b) * f,
            from.a + (to.a - from.a) * f,
        );
    }

    /// Three-stop color lerp (`Draw.color(a, b, c, f)`).
    pub fn color_lerp3(
        &mut self,
        a: impl Into<Rgba>,
        b: impl Into<Rgba>,
        c: impl Into<Rgba>,
        f: f32,
    ) {
        let a = a.into();
        let b = b.into();
        let c = c.into();
        if f <= 0.5 {
            self.color_lerp(a, b, f * 2.0);
        } else {
            self.color_lerp(b, c, (f - 0.5) * 2.0);
        }
    }

    /// `Draw.alpha(a)`.
    pub fn alpha(&mut self, alpha: f32) {
        self.current.a = alpha.clamp(0.0, 1.0);
    }

    /// `Draw.getColorAlpha()`.
    pub fn alpha_value(&self) -> f32 {
        self.current.a
    }

    /// `Draw.getColor()`.
    pub fn get_color(&self) -> Rgba {
        self.current
    }

    /// `Draw.mixcol(color, strength)` (best-effort: strength 1 replaces).
    pub fn mixcol(&mut self, color: impl Into<Rgba>, strength: f32) {
        if strength >= 1.0 {
            self.mix = Some(color.into());
        }
    }

    /// `Lines.stroke(width)`.
    pub fn stroke(&mut self, width: f32) {
        self.stroke = width;
    }

    /// `Lines.getStroke()`.
    pub fn get_stroke(&self) -> f32 {
        self.stroke
    }

    /// `Drawf.additive(...)`: subsequent prims blend additively.
    pub fn additive(&mut self, r: f32, g: f32, b: f32, alpha: f32) {
        self.blend = Blending::Additive;
        self.current = Rgba::new(r, g, b, alpha);
    }

    /// `Draw.reset()`.
    pub fn reset(&mut self) {
        self.current = Rgba::WHITE;
        self.mix = None;
        self.stroke = 1.0;
        self.blend = Blending::Normal;
    }

    /// Pushes a raw primitive at the emitter z.
    pub fn push(&mut self, kind: PrimKind) {
        self.program.push(DrawPrim {
            z: self.z,
            blend: self.blend,
            kind,
        });
    }

    /// `Draw.rect(region, x, y, w, h, rotation)`.
    #[allow(clippy::too_many_arguments)]
    pub fn rect(&mut self, region: RegionKey, x: f32, y: f32, w: f32, h: f32, rotation_deg: f32) {
        self.push(PrimKind::Region {
            region,
            x,
            y,
            w,
            h,
            rotation_deg,
            origin: (0.5, 0.5),
            color: self.current,
            mix: self.mix,
            wrap: false,
        });
    }

    /// `Draw.rect(region, x, y, rotation)` at native tile size.
    pub fn rect_native(&mut self, region: RegionKey, x: f32, y: f32, rotation_deg: f32) {
        self.rect(region, x, y, 32.0, 32.0, rotation_deg);
    }

    /// `Fill.rect(x, y, w, h, rotation)` as a rotated quad.
    pub fn rect_fill(&mut self, x: f32, y: f32, w: f32, h: f32, rotation_deg: f32) {
        let (s, c) = rotation_deg.to_radians().sin_cos();
        let hw = w / 2.0;
        let hh = h / 2.0;
        let corner = |dx: f32, dy: f32| (x + dx * c - dy * s, y + dx * s + dy * c);
        let mut points: smallvec::SmallVec<[(f32, f32); 12]> = smallvec::SmallVec::new();
        points.push(corner(-hw, -hh));
        points.push(corner(hw, -hh));
        points.push(corner(hw, hh));
        points.push(corner(-hw, hh));
        self.push(PrimKind::Polygon {
            points,
            fill: true,
            stroke: 0.0,
            color: self.current,
        });
    }

    /// `Fill.circle(x, y, radius)`.
    pub fn circle(&mut self, x: f32, y: f32, r: f32) {
        self.push(PrimKind::Circle {
            x,
            y,
            r,
            fill: true,
            stroke: 0.0,
            color: self.current,
        });
    }

    /// `Lines.circle(x, y, radius)`.
    pub fn circle_line(&mut self, x: f32, y: f32, r: f32) {
        self.push(PrimKind::Circle {
            x,
            y,
            r,
            fill: false,
            stroke: self.stroke,
            color: self.current,
        });
    }

    /// `Fill.square(x, y, radius, rotation)` (`Arc Fill.java:361-363`):
    /// `Draw.rect(atlas.white(), x, y, radius*2, radius*2, rotation)` — a
    /// square of side `radius * 2` rotated `rotation` degrees, i.e. a 4-gon of
    /// circumradius `radius * sqrt(2)` with its corners at `rotation + 45`.
    pub fn square(&mut self, x: f32, y: f32, r: f32, rotation_deg: f32) {
        self.push(PrimKind::Poly {
            x,
            y,
            sides: 4,
            r: r * std::f32::consts::SQRT_2,
            rotation_deg: rotation_deg + 45.0,
            fill: true,
            stroke: 0.0,
            color: self.current,
        });
    }

    /// `Lines.square(x, y, radius, rotation)` (`Arc Lines.java:507-509`):
    /// `poly(x, y, 4, radius, rotation - 45)` — a 4-gon of circumradius
    /// `radius` with its corners at `rotation`.
    pub fn square_line(&mut self, x: f32, y: f32, r: f32, rotation_deg: f32) {
        self.push(PrimKind::Poly {
            x,
            y,
            sides: 4,
            r,
            rotation_deg: rotation_deg - 45.0,
            fill: false,
            stroke: self.stroke,
            color: self.current,
        });
    }

    /// `Fill.poly(x, y, sides, radius, rotation)`.
    pub fn poly(&mut self, x: f32, y: f32, sides: u16, r: f32, rotation_deg: f32) {
        self.push(PrimKind::Poly {
            x,
            y,
            sides,
            r,
            rotation_deg,
            fill: true,
            stroke: 0.0,
            color: self.current,
        });
    }

    /// `Lines.poly(x, y, sides, radius, rotation)`.
    pub fn poly_line(&mut self, x: f32, y: f32, sides: u16, r: f32, rotation_deg: f32) {
        self.push(PrimKind::Poly {
            x,
            y,
            sides,
            r,
            rotation_deg,
            fill: false,
            stroke: self.stroke,
            color: self.current,
        });
    }

    /// `Drawf.tri(x, y, w, h, rotation)`.
    pub fn tri(&mut self, x: f32, y: f32, w: f32, h: f32, rotation_deg: f32) {
        self.push(PrimKind::Tri {
            x,
            y,
            w,
            h,
            rotation_deg,
            color: self.current,
        });
    }

    /// `Lines.line(x1, y1, x2, y2)`.
    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, cap: bool) {
        self.push(PrimKind::Line {
            x1,
            y1,
            x2,
            y2,
            stroke: self.stroke,
            color: self.current,
            cap,
        });
    }

    /// `Lines.lineAngle(x, y, angle, length)`.
    pub fn line_angle(&mut self, x: f32, y: f32, angle: f32, length: f32) {
        let dx = trnsx(angle, length);
        let dy = trnsy(angle, length);
        self.line(x, y, x + dx, y + dy, true);
    }

    /// `Lines.lineAngle(x, y, angle, length, offset)` — the segment starts
    /// `offset` along the direction and extends `length` beyond it.
    pub fn line_angle_offset(&mut self, x: f32, y: f32, angle: f32, length: f32, offset: f32) {
        let ox = trnsx(angle, offset);
        let oy = trnsy(angle, offset);
        let ex = trnsx(angle, length + offset);
        let ey = trnsy(angle, length + offset);
        self.line(x + ox, y + oy, x + ex, y + ey, false);
    }

    /// `Lines.spikes(x, y, radius, length, spikes)`.
    pub fn spikes(&mut self, x: f32, y: f32, radius: f32, length: f32, spikes: i32) {
        let step = 360.0 / spikes as f32;
        for i in 0..spikes {
            let a = i as f32 * step;
            let x1 = trnsx(a, radius);
            let y1 = trnsy(a, radius);
            let x2 = trnsx(a, radius + length);
            let y2 = trnsy(a, radius + length);
            self.line(x + x1, y + y1, x + x2, y + y2, false);
        }
    }

    /// `Drawf.light(x, y, radius, color, opacity)`.
    pub fn light(&mut self, x: f32, y: f32, radius: f32, color: impl Into<Rgba>, opacity: f32) {
        self.push(PrimKind::Light {
            x,
            y,
            radius,
            color: color.into().with_alpha(self.current.a),
            opacity,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_produces_prims_at_z() {
        let mut program = DrawProgram::new();
        {
            let mut emit = FxEmit::new(&mut program, 110.0);
            emit.color(Rgba::WHITE);
            emit.circle(1.0, 2.0, 3.0);
            emit.additive(1.0, 0.0, 0.0, 0.5);
            emit.tri(0.0, 0.0, 4.0, 5.0, 0.0);
        }
        assert_eq!(program.len(), 2);
        assert_eq!(program.prims[0].z, 110.0);
        assert_eq!(program.prims[0].blend, Blending::Normal);
        assert_eq!(program.prims[1].blend, Blending::Additive);
    }

    #[test]
    fn line_angle_uses_trig() {
        let mut program = DrawProgram::new();
        {
            let mut emit = FxEmit::new(&mut program, 0.0);
            emit.stroke(2.0);
            emit.line_angle(0.0, 0.0, 0.0, 10.0);
        }
        if let PrimKind::Line { x2, y2, stroke, .. } = &program.prims[0].kind {
            assert!((x2 - 10.0).abs() < 1e-4);
            assert!(y2.abs() < 1e-4);
            assert_eq!(*stroke, 2.0);
        } else {
            panic!("expected line");
        }
    }

    /// J03 T7/PAR-0348: Arc's `Fill.square(x, y, radius, rotation)` is
    /// `Draw.rect(white, x, y, radius*2, radius*2, rotation)`
    /// (`Fill.java:361-363`) — a square of side `2*radius` rotated, i.e. a
    /// 4-gon of circumradius `radius*sqrt(2)` with corners at
    /// `rotation + 45`; `Lines.square` is `poly(4, radius, rotation - 45)`
    /// (`Lines.java:507-509`).
    #[test]
    fn arc_square_conventions() {
        let mut program = DrawProgram::new();
        {
            let mut emit = FxEmit::new(&mut program, 0.0);
            emit.square(1.0, 2.0, 5.0, 45.0);
            emit.square_line(3.0, 4.0, 5.0, 45.0);
        }
        match &program.prims[0].kind {
            PrimKind::Poly {
                sides,
                r,
                rotation_deg,
                fill,
                ..
            } => {
                assert_eq!(*sides, 4);
                assert!((r - 5.0 * std::f32::consts::SQRT_2).abs() < 1e-4, "r {r}");
                // `rotation + 45` with corners at the axes for rotation=45:
                // the Diamond the Java `Fill.square(…, 45)` call sites expect.
                assert!((rotation_deg - 90.0).abs() < 1e-4);
                assert!(*fill);
                // Corner positions: radius*sqrt2 at 90/180/270/0 degrees.
                let points = crate::render::drawf::poly_ring(1.0, 2.0, 4, *r, *rotation_deg);
                for (x, y) in points {
                    let dx = x - 1.0;
                    let dy = y - 2.0;
                    assert!(
                        dx.abs() < 1e-3 || dy.abs() < 1e-3,
                        "corner on an axis ({x}, {y})"
                    );
                }
            }
            other => panic!("expected poly: {other:?}"),
        }
        match &program.prims[1].kind {
            PrimKind::Poly {
                r,
                rotation_deg,
                fill,
                ..
            } => {
                assert!((r - 5.0).abs() < 1e-4);
                assert!((rotation_deg - 0.0).abs() < 1e-4);
                assert!(!fill);
            }
            other => panic!("expected poly: {other:?}"),
        }
    }
}

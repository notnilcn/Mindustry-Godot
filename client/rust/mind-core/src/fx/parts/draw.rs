// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawPart` draw emission (plan 17 M4).
//!
//! Compiles the plan-02 part data (`content::registries::units::parts`) into
//! [`DrawProgram`] primitives. Godot-free: `mind-gdext` executes the program.
//! Ported 1:1 from `entities/part/{DrawPart,RegionPart,ShapePart,HaloPart,
//! HoverPart,FlarePart,EffectSpawnerPart}.java`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::content::Rgba;
use crate::content::registries::units::parts::{
    BlendingKind, DrawPartKind, DrawPartSpec, PartProgressSpec as ContentProgress,
};
use crate::math::{ArcRand, Interp};
use crate::render::draw::{Blending, DrawPrim, DrawProgram, PrimKind, RegionKey};
use crate::render::layer::Layer;

use super::params::PartParams;
use super::progress::{PartFunc, PartProgressSpec};
use super::{EffectSpawnerPartSpec, HaloPartSpec};

/// A resolved atlas region (plan 03 boundary): name + pixel size + found flag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionInfo {
    /// Interned atlas region key.
    pub key: RegionKey,
    /// Native region width in pixels.
    pub w: f32,
    /// Native region height in pixels.
    pub h: f32,
    /// `region.found()`.
    pub found: bool,
}

impl RegionInfo {
    /// A missing region (drawn as nothing).
    pub fn missing() -> Self {
        Self {
            key: RegionKey::ERROR,
            w: 32.0,
            h: 32.0,
            found: false,
        }
    }
}

impl Default for RegionInfo {
    fn default() -> Self {
        Self::missing()
    }
}

/// Region name → [`RegionInfo`] (plan 03's `AtlasIndex` implements this in gdext).
pub trait RegionLookup {
    /// Looks up `name`, returning a missing info when absent.
    fn region(&self, name: &str) -> RegionInfo;
}

/// Interns a resolved (content-derived) region name into a `RegionKey`.
///
/// Content-derived names are bounded; each distinct name is leaked exactly once
/// (the same pattern plan 03/16 uses at the atlas boundary, plan 17 §3.15).
pub fn intern_region(name: &str) -> RegionKey {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(leaked) = guard.get(name) {
        return RegionKey(leaked);
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    guard.insert(name.to_owned(), leaked);
    RegionKey(leaked)
}

/// A headless lookup that treats every non-empty name as a found region of a
/// fixed size (real found-ness comes from plan 03's atlas).
pub struct AllRegions {
    /// Region size in pixels for both axes.
    pub size: f32,
}

impl AllRegions {
    /// A lookup with a square region size.
    pub fn new(size: f32) -> Self {
        Self { size }
    }
}

impl Default for AllRegions {
    fn default() -> Self {
        Self { size: 32.0 }
    }
}

impl RegionLookup for AllRegions {
    fn region(&self, name: &str) -> RegionInfo {
        if name.is_empty() {
            return RegionInfo::missing();
        }
        RegionInfo {
            key: intern_region(name),
            w: self.size,
            h: self.size,
            found: true,
        }
    }
}

/// A deterministic lookup for tests: only registered names are `found`.
#[derive(Default)]
pub struct MapRegions {
    entries: Vec<(String, f32, f32)>,
}

impl MapRegions {
    /// A new empty lookup.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a region with a pixel size.
    pub fn add(&mut self, name: &str, w: f32, h: f32) -> &mut Self {
        self.entries.push((name.to_owned(), w, h));
        self
    }
}

impl RegionLookup for MapRegions {
    fn region(&self, name: &str) -> RegionInfo {
        match self.entries.iter().find(|(n, _, _)| n == name) {
            Some((_, w, h)) => RegionInfo {
                key: intern_region(name),
                w: *w,
                h: *h,
                found: true,
            },
            None => RegionInfo::missing(),
        }
    }
}

/// Emission context/state for one part tree (plan 17 §3.7).
pub struct PartEmit<'a> {
    /// Program being built.
    pub program: &'a mut DrawProgram,
    /// Region resolver.
    pub lookup: &'a dyn RegionLookup,
    /// Fixed view tick (`Time.time`, deviation #7).
    pub tick: f32,
    pub(crate) z: f32,
    pub(crate) xscl: f32,
    pub(crate) yscl: f32,
    pub(crate) color: Rgba,
    pub(crate) mix: Option<Rgba>,
    pub(crate) blend: Blending,
}

impl<'a> PartEmit<'a> {
    /// Builds an emitter at layer `z`.
    pub fn new(program: &'a mut DrawProgram, lookup: &'a dyn RegionLookup, tick: f32) -> Self {
        Self {
            program,
            lookup,
            tick,
            z: Layer::Effect.z(),
            xscl: 1.0,
            yscl: 1.0,
            color: Rgba::WHITE,
            mix: None,
            blend: Blending::Normal,
        }
    }

    /// The current z.
    pub fn z(&self) -> f32 {
        self.z
    }

    pub(crate) fn push(&mut self, kind: PrimKind) {
        self.program.push(DrawPrim {
            z: self.z,
            blend: self.blend,
            kind,
        });
    }

    /// Pushes a primitive at an explicit layer (does not change `self.z`).
    pub fn push_at(&mut self, z: f32, kind: PrimKind, blend: Blending) {
        self.program.push(DrawPrim { z, blend, kind });
    }

    /// Draws a named region centered at `(x, y)` if found; returns found-ness.
    pub fn region(&mut self, name: &str, x: f32, y: f32, rot: f32) -> bool {
        let info = self.lookup.region(name);
        if info.found {
            self.emit_region(info, x, y, rot, 0.0, 0.0);
            true
        } else {
            false
        }
    }

    /// Looks up region info without emitting.
    pub fn lookup(&self, name: &str) -> RegionInfo {
        self.lookup.region(name)
    }

    fn emit_region(&mut self, info: RegionInfo, x: f32, y: f32, rot: f32, ox: f32, oy: f32) {
        let w = info.w * self.xscl;
        let h = info.h * self.yscl;
        let px = w / 2.0 + ox * self.xscl;
        let py = h / 2.0 + oy * self.yscl;
        let onx = if w.abs() > 1e-9 { px / w } else { 0.5 };
        let ony = if h.abs() > 1e-9 { py / h } else { 0.5 };
        self.push(PrimKind::Region {
            region: info.key,
            x,
            y,
            w,
            h,
            rotation_deg: rot,
            origin: (onx, ony),
            color: self.color,
            mix: self.mix,
            wrap: false,
        });
    }

    /// Draws one part tree.
    pub fn draw_part(&mut self, part: &DrawPartSpec, params: &PartParams, content_name: &str) {
        match part.kind {
            DrawPartKind::RegionPart => self.draw_region(part, params, content_name),
            DrawPartKind::ShapePart => self.draw_shape(part, params),
            DrawPartKind::HoverPart => self.draw_hover(part, params),
            DrawPartKind::FlarePart => self.draw_flare(part, params),
        }
    }

    /// `RegionPart.draw`.
    fn draw_region(&mut self, p: &DrawPartSpec, params: &PartParams, content_name: &str) {
        let z = self.z;
        if p.layer > 0.0 {
            self.z = p.layer;
        }
        if p.under && p.turret_shading {
            self.z = z - 0.0001;
        }
        self.z += p.layer_offset;
        let prev_z = self.z;

        let clamp = p.clamp_progress;
        let prog = eval_progress(&p.progress, params, self.tick, clamp);
        let scl_prog = eval_progress(&p.grow_progress, params, self.tick, clamp);

        let mut mx = p.move_x * prog;
        let mut my = p.move_y * prog;
        let mut mr = p.move_rot * prog + p.rotation;
        let mut gx = p.grow_x * scl_prog;
        let mut gy = p.grow_y * scl_prog;
        for m in &p.moves {
            let mp = eval_progress(&m.progress, params, self.tick, clamp);
            mx += m.x * mp;
            my += m.y * mp;
            mr += m.rot * mp;
            gx += m.gx * mp;
            gy += m.gy * mp;
        }

        let len = if p.mirror && params.side_override == -1 {
            2
        } else {
            1
        };
        let pre_xscl = self.xscl;
        let pre_yscl = self.yscl;
        self.xscl *= p.x_scl + gx;
        self.yscl *= p.y_scl + gy;
        let prev_col = self.color;
        let prev_mix = self.mix;

        let names = RegionNames::resolve(p, content_name);
        for s in 0..len {
            let i = if params.side_override == -1 {
                s
            } else {
                params.side_override
            };
            let sign = (if i == 0 { 1.0 } else { -1.0 }) * params.side_multiplier as f32;
            let (mut vx, mut vy) = rotate((p.x + mx) * sign, p.y + my, params.rotation - 90.0);
            self.xscl *= sign;
            if p.origin_x != 0.0 || p.origin_y != 0.0 {
                let (wx, wy) = rotate(
                    -p.origin_x * self.xscl,
                    -p.origin_y * self.yscl,
                    params.rotation - 90.0,
                );
                vx -= wx + p.origin_x * self.xscl;
                vy -= wy + p.origin_y * self.yscl;
            }
            let rx = params.x + vx;
            let ry = params.y + vy;
            let rot = mr * sign + params.rotation - 90.0;

            let region = self.lookup.region(names.region(max_i(i)));
            let outline = self.lookup.region(names.outline(max_i(i)));
            let heat = self.lookup.region(&names.heat);
            let light = self.lookup.region(&names.light);

            if p.outline && p.draw_region {
                self.z = prev_z + p.outline_layer_offset;
                self.emit_region(outline, rx, ry, rot, p.origin_x, p.origin_y);
                self.z = prev_z;
            }

            if p.draw_region && region.found {
                self.color = match (p.color, p.color_to) {
                    (Some(a), Some(b)) => lerp_color(a, b, prog),
                    (Some(a), None) => a,
                    _ => self.color,
                };
                self.mix = match (p.mix_color, p.mix_color_to) {
                    (Some(a), Some(b)) => Some(lerp_color(a, b, prog)),
                    (Some(a), None) => Some(a.with_alpha(a.a)),
                    _ => None,
                };
                self.blend = if p.blending == BlendingKind::Additive {
                    Blending::Additive
                } else {
                    Blending::Normal
                };
                self.emit_region(region, rx, ry, rot, p.origin_x, p.origin_y);
                self.blend = Blending::Normal;
                if p.color.is_some() {
                    self.color = Rgba::WHITE;
                }
            }

            if heat.found {
                let hprog = eval_progress(&p.heat_progress, params, self.tick, clamp);
                let mut hc = p.heat_color.unwrap_or(Rgba::WHITE);
                hc.a *= hprog;
                let hz = if p.turret_heat_layer {
                    Layer::TurretHeat.z()
                } else {
                    self.z + p.heat_layer_offset
                };
                let saved_z = self.z;
                let saved_color = self.color;
                let saved_mix = self.mix;
                self.z = hz;
                self.color = hc;
                self.mix = None;
                self.blend = Blending::Additive;
                self.emit_region(heat, rx, ry, rot, p.origin_x, p.origin_y);
                self.z = saved_z;
                self.color = saved_color;
                self.mix = saved_mix;
                self.blend = Blending::Normal;
                if p.heat_light {
                    let info = if light.found { light } else { heat };
                    self.push_light(rx, ry, info.w, hc, p.heat_light_opacity * hprog);
                }
            }

            self.xscl *= sign;
        }

        self.color = prev_col;
        self.mix = prev_mix;
        self.z = z;

        if !p.children.is_empty() {
            for s in 0..len {
                let i = if params.side_override == -1 {
                    s
                } else {
                    params.side_override
                };
                let sign = (if i == 1 { -1.0 } else { 1.0 }) * params.side_multiplier as f32;
                let (vx, vy) = rotate((p.x + mx) * sign, p.y + my, params.rotation - 90.0);
                let mut child = PartParams::default();
                child.set(
                    params.warmup,
                    params.reload,
                    params.smooth_reload,
                    params.heat,
                    params.recoil,
                    params.charge,
                    params.x + vx,
                    params.y + vy,
                    mr * sign + params.rotation,
                );
                child.side_multiplier = params.side_multiplier;
                child.life = params.life;
                child.side_override = i;
                for c in &p.children {
                    self.draw_part(c, &child, content_name);
                }
            }
        }

        self.xscl = pre_xscl;
        self.yscl = pre_yscl;
    }

    fn push_light(&mut self, x: f32, y: f32, radius: f32, color: Rgba, opacity: f32) {
        self.push(PrimKind::Light {
            x,
            y,
            radius,
            color,
            opacity,
        });
    }

    /// `ShapePart.draw`.
    fn draw_shape(&mut self, p: &DrawPartSpec, params: &PartParams) {
        let z = self.z;
        if p.layer > 0.0 {
            self.z = p.layer;
        }
        if p.under && p.turret_shading {
            self.z = z - 0.0001;
        }
        self.z += p.layer_offset;

        let clamp = p.clamp_progress;
        let prog = eval_progress(&p.progress, params, self.tick, clamp);
        let base_rot = self.tick * p.rotate_speed;
        let rad = if p.radius_to < 0.0 {
            p.radius
        } else {
            lerp(p.radius, p.radius_to, prog)
        };
        let str_w = if p.stroke_to < 0.0 {
            p.stroke
        } else {
            lerp(p.stroke, p.stroke_to, prog)
        };

        let len = if p.mirror && params.side_override == -1 {
            2
        } else {
            1
        };
        let color = p.color.unwrap_or(Rgba::WHITE);
        for s in 0..len {
            let i = if params.side_override == -1 {
                s
            } else {
                params.side_override
            };
            let sign = (if i == 0 { 1.0 } else { -1.0 }) * params.side_multiplier as f32;
            let (vx, vy) = rotate(
                (p.x + p.move_x * prog) * sign,
                p.y + p.move_y * prog,
                params.rotation - 90.0,
            );
            let rx = params.x + vx;
            let ry = params.y + vy;
            let rot = p.move_rot * prog * sign + params.rotation - 90.0 * sign
                + p.rotation * sign
                + base_rot * sign;
            let fill = p
                .color_to
                .map(|to| lerp_color(color, to, prog))
                .unwrap_or(color);
            let saved = self.color;
            self.color = fill;
            if !p.hollow {
                if !p.circle {
                    self.push(PrimKind::Poly {
                        x: rx,
                        y: ry,
                        sides: p.sides.max(0) as u16,
                        r: rad,
                        rotation_deg: rot,
                        fill: true,
                        stroke: 0.0,
                        color: fill,
                    });
                } else {
                    self.push(PrimKind::Circle {
                        x: rx,
                        y: ry,
                        r: rad,
                        fill: true,
                        stroke: 0.0,
                        color: fill,
                    });
                }
            } else if str_w > 0.0001 {
                if !p.circle {
                    self.push(PrimKind::Poly {
                        x: rx,
                        y: ry,
                        sides: p.sides.max(0) as u16,
                        r: rad,
                        rotation_deg: rot,
                        fill: false,
                        stroke: str_w,
                        color: fill,
                    });
                } else {
                    self.push(PrimKind::Circle {
                        x: rx,
                        y: ry,
                        r: rad,
                        fill: false,
                        stroke: str_w,
                        color: fill,
                    });
                }
            }
            self.color = saved;
        }
        self.z = z;
    }

    /// `HoverPart.draw`.
    fn draw_hover(&mut self, p: &DrawPartSpec, params: &PartParams) {
        let z = self.z;
        if p.layer > 0.0 {
            self.z = p.layer;
        }
        if p.under && p.turret_shading {
            self.z = z - 0.0001;
        }
        self.z += p.layer_offset;

        let len = if p.mirror && params.side_override == -1 {
            2
        } else {
            1
        };
        let color = p.color.unwrap_or(Rgba::WHITE);
        self.color = color;
        let circles = p.circles.max(1);
        for c in 0..circles {
            let fin = (self.tick / p.phase + c as f32 / circles as f32).rem_euclid(1.0);
            let stroke = (1.0 - fin) * p.stroke + p.min_stroke;
            for s in 0..len {
                let i = if params.side_override == -1 {
                    s
                } else {
                    params.side_override
                };
                let sign = (if i == 0 { 1.0 } else { -1.0 }) * params.side_multiplier as f32;
                let (vx, vy) = rotate(p.x * sign, p.y, params.rotation - 90.0);
                let rx = params.x + vx;
                let ry = params.y + vy;
                self.push(PrimKind::Poly {
                    x: rx,
                    y: ry,
                    sides: p.sides.max(0) as u16,
                    r: p.radius * fin,
                    rotation_deg: params.rotation + p.rotation * sign,
                    fill: false,
                    stroke,
                    color,
                });
            }
        }
        self.color = Rgba::WHITE;
        self.z = z;
    }

    /// `FlarePart.draw`.
    fn draw_flare(&mut self, p: &DrawPartSpec, params: &PartParams) {
        let z = self.z;
        if p.layer > 0.0 {
            self.z = p.layer;
        }
        let prog = eval_progress(&p.progress, params, self.tick, p.clamp_progress);
        let i = if params.side_override == -1 {
            0
        } else {
            params.side_override
        };
        let sign = (if i == 0 { 1.0 } else { -1.0 }) * params.side_multiplier as f32;
        let (vx, vy) = rotate(p.x * sign, p.y, params.rotation - 90.0);
        let rx = params.x + vx;
        let ry = params.y + vy;
        let rot = (if p.follow_rotation {
            params.rotation
        } else {
            0.0
        }) + p.rot_move * prog
            + p.rotation
            + self.tick * p.spin_speed;
        let rad = if p.radius_to < 0.0 {
            p.radius
        } else {
            lerp(p.radius, p.radius_to, prog)
        };
        let sides = p.sides.max(1);
        for j in 0..sides {
            self.color = p.color1;
            self.push(PrimKind::Tri {
                x: rx,
                y: ry,
                w: p.stroke,
                h: rad,
                rotation_deg: j as f32 * 360.0 / sides as f32 + rot,
                color: p.color1,
            });
        }
        for j in 0..sides {
            self.color = p.color2;
            self.push(PrimKind::Tri {
                x: rx,
                y: ry,
                w: p.stroke * p.inner_scl,
                h: rad * p.inner_rad_scl,
                rotation_deg: j as f32 * 360.0 / sides as f32 + rot,
                color: p.color2,
            });
        }
        self.color = Rgba::WHITE;
        self.z = z;
    }

    /// `HaloPart.draw` (fx spec; not used by vanilla units).
    pub fn draw_halo(&mut self, p: &HaloPartSpec, params: &PartParams) {
        let z = self.z;
        if p.layer > 0.0 {
            self.z = p.layer;
        }
        self.z += p.layer_offset;
        let prog = if p.clamp_progress {
            p.progress.get_clamp(params, self.tick)
        } else {
            p.progress.get(params, self.tick)
        };
        let base_rot = self.tick * p.rotate_speed;
        let rad = if p.radius_to < 0.0 {
            p.radius
        } else {
            lerp(p.radius, p.radius_to, prog)
        };
        let tri_len = if p.tri_length_to < 0.0 {
            p.tri_length
        } else {
            lerp(p.tri_length, p.tri_length_to, prog)
        };
        let str_w = if p.stroke_to < 0.0 {
            p.stroke
        } else {
            lerp(p.stroke, p.stroke_to, prog)
        };
        let halo_rad = if p.halo_radius_to < 0.0 {
            p.halo_radius
        } else {
            lerp(p.halo_radius, p.halo_radius_to, prog)
        };
        let len = if p.mirror && params.side_override == -1 {
            2
        } else {
            1
        };
        let color = match p.color_to {
            Some(to) => lerp_color(p.color, to, prog),
            None => p.color,
        };
        self.color = color;
        for s in 0..len {
            let i = if params.side_override == -1 {
                s
            } else {
                params.side_override
            };
            let sign = (if i == 0 { 1.0 } else { -1.0 }) * params.side_multiplier as f32;
            let (vx, vy) = rotate(
                (p.x + p.move_x * prog) * sign,
                p.y + p.move_y * prog,
                params.rotation - 90.0,
            );
            let rx = params.x + vx;
            let ry = params.y + vy;
            let halo_rot = (p.halo_rotation + p.halo_rotate_speed * self.tick) * sign;
            let shapes = p.shapes.max(0);
            for v in 0..shapes {
                let rot = halo_rot + v as f32 * 360.0 / shapes as f32 + params.rotation;
                let shape_x = crate::fx::angles::trnsx(rot, halo_rad) + rx;
                let shape_y = crate::fx::angles::trnsy(rot, halo_rad) + ry;
                let point_rot = rot
                    + p.shape_move_rot * prog * sign
                    + p.shape_rotation * sign
                    + base_rot * sign;
                if p.tri {
                    if rad > 0.001 && tri_len > 0.001 {
                        self.push(PrimKind::Tri {
                            x: shape_x,
                            y: shape_y,
                            w: rad,
                            h: tri_len,
                            rotation_deg: point_rot,
                            color,
                        });
                    }
                } else if !p.hollow {
                    if rad > 0.001 {
                        self.push(PrimKind::Poly {
                            x: shape_x,
                            y: shape_y,
                            sides: p.sides.max(0) as u16,
                            r: rad,
                            rotation_deg: point_rot,
                            fill: true,
                            stroke: 0.0,
                            color,
                        });
                    }
                } else if str_w > 0.001 {
                    self.push(PrimKind::Poly {
                        x: shape_x,
                        y: shape_y,
                        sides: p.sides.max(0) as u16,
                        r: rad,
                        rotation_deg: point_rot,
                        fill: false,
                        stroke: str_w,
                        color,
                    });
                }
            }
        }
        self.color = Rgba::WHITE;
        self.z = z;
    }
}

/// Clamps a possibly-`i32` side index into a name-vector index (Java `Math.min`).
fn max_i(i: i32) -> usize {
    i.max(0) as usize
}

/// Resolved region names for a `RegionPart` (`load()` output).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionNames {
    /// Base region names (1 or 2 for mirror).
    pub regions: Vec<String>,
    /// Outline region names.
    pub outlines: Vec<String>,
    /// Heat region name.
    pub heat: String,
    /// Light region name.
    pub light: String,
}

impl RegionNames {
    /// Computes the names for `part` loaded under `content_name`.
    pub fn resolve(part: &DrawPartSpec, content_name: &str) -> Self {
        let real = format!("{}{}", content_name, part.suffix);
        let (regions, outlines) = if part.draw_region {
            if part.mirror && part.turret_shading {
                (
                    vec![format!("{real}-r"), format!("{real}-l")],
                    vec![format!("{real}-r-outline"), format!("{real}-l-outline")],
                )
            } else {
                (vec![real.clone()], vec![format!("{real}-outline")])
            }
        } else {
            (Vec::new(), Vec::new())
        };
        Self {
            regions,
            outlines,
            heat: format!("{real}-heat"),
            light: format!("{real}-light"),
        }
    }

    /// Base region name at side index `i` (Java `Math.min(i, len-1)`), or empty.
    pub fn region(&self, i: usize) -> &str {
        if self.regions.is_empty() {
            ""
        } else {
            &self.regions[i.min(self.regions.len() - 1)]
        }
    }

    /// Outline region name at side index `i`, or empty.
    pub fn outline(&self, i: usize) -> &str {
        if self.outlines.is_empty() {
            ""
        } else {
            &self.outlines[i.min(self.outlines.len() - 1)]
        }
    }
}

/// `DrawPart.getOutlines`: the base region names for outline generation (plan 03).
pub fn get_outlines(parts: &[DrawPartSpec], content_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    collect_outlines(parts, content_name, &mut out);
    out
}

fn collect_outlines(parts: &[DrawPartSpec], content_name: &str, out: &mut Vec<String>) {
    for part in parts {
        if part.kind == DrawPartKind::RegionPart && part.outline && part.draw_region {
            let names = RegionNames::resolve(part, content_name);
            out.extend(names.regions.iter().cloned());
        }
        collect_outlines(&part.children, content_name, out);
    }
}

/// Draws a list of parts.
pub fn draw_parts(
    program: &mut DrawProgram,
    parts: &[DrawPartSpec],
    params: &PartParams,
    lookup: &dyn RegionLookup,
    tick: f32,
) {
    let mut emit = PartEmit::new(program, lookup, tick);
    for part in parts {
        emit.draw_part(part, params, "");
    }
}

/// Draws a list of parts under a content region name.
pub fn draw_named_parts(
    program: &mut DrawProgram,
    parts: &[DrawPartSpec],
    params: &PartParams,
    lookup: &dyn RegionLookup,
    tick: f32,
    content_name: &str,
) {
    let mut emit = PartEmit::new(program, lookup, tick);
    for part in parts {
        emit.draw_part(part, params, content_name);
    }
}

/// Per-instance `EffectSpawnerPart` state (`effectIntervalState`).
#[derive(Clone, Debug)]
pub struct SpawnerState {
    /// Accumulated interval ticks.
    pub interval_state: f32,
    rng: ArcRand,
}

impl Default for SpawnerState {
    fn default() -> Self {
        Self {
            interval_state: 0.0,
            rng: ArcRand::new(0),
        }
    }
}

impl SpawnerState {
    /// A state seeded deterministically by `seed`.
    pub fn seeded(seed: u64) -> Self {
        Self {
            interval_state: 0.0,
            rng: ArcRand::new(seed),
        }
    }
}

/// Spawns effects for an `EffectSpawnerPart`; `spawn(effect, x, y, rotation, color)`.
pub fn draw_spawner(
    emit: &mut PartEmit,
    spec: &EffectSpawnerPartSpec,
    params: &PartParams,
    state: &mut SpawnerState,
    delta: f32,
    paused: bool,
    mut spawn: impl FnMut(crate::content::EffectId, f32, f32, f32, Rgba),
) {
    if spec.debug_draw {
        for i in 0..(if spec.mirror { 2 } else { 1 }) {
            let sign = if i == 0 { 1.0 } else { -1.0 };
            let rot = params.rotation + spec.rotation * sign;
            let (mut vx, mut vy) = rotate(spec.x * sign, spec.y, params.rotation - 90.0);
            vx += params.x;
            vy += params.y;
            let _ = (rot, vx, vy);
        }
    }
    if paused {
        return;
    }
    let prog = spec.progress.get_clamp(params, emit.tick);
    let real_interval = if spec.effect_interval_from > 0.0 {
        lerp(spec.effect_interval_from, spec.effect_interval, prog)
    } else {
        spec.effect_interval
    };
    for i in 0..(if spec.mirror { 2 } else { 1 }) {
        let fire = if real_interval > 0.0 {
            state.interval_state += delta;
            state.interval_state >= real_interval
        } else {
            let chance = spec.effect_chance * if spec.use_progress { prog } else { 1.0 };
            state.rng.next_float() < chance * delta
        };
        if !fire {
            continue;
        }
        let sign = if i == 0 { 1.0 } else { -1.0 };
        let rot = params.rotation + spec.rotation * sign;
        let (mut vx, mut vy) = rotate(spec.x * sign, spec.y, params.rotation - 90.0);
        vx += params.x;
        vy += params.y;
        let (jx, jy) = rotate(
            state
                .rng
                .random_range_float(-spec.height * 0.5, spec.height * 0.5),
            state
                .rng
                .random_range_float(-spec.width * 0.5, spec.width * 0.5),
            rot,
        );
        vx += jx;
        vy += jy;
        let eff_rot = rot
            + spec.effect_rot * sign
            + state
                .rng
                .random_range_float(-spec.effect_rand_rot, spec.effect_rand_rot);
        spawn(spec.effect, vx, vy, eff_rot, spec.effect_color);
        if real_interval > 0.0 {
            state.interval_state %= real_interval;
        }
    }
}

/// `Vec2.rotate` (degrees, counter-clockwise).
#[inline]
fn rotate(x: f32, y: f32, deg: f32) -> (f32, f32) {
    let (sin, cos) = deg.to_radians().sin_cos();
    (x * cos - y * sin, x * sin + y * cos)
}

#[inline]
fn lerp(a: f32, b: f32, f: f32) -> f32 {
    a + (b - a) * f
}

#[inline]
fn lerp_color(a: Rgba, b: Rgba, f: f32) -> Rgba {
    Rgba::new(
        lerp(a.r, b.r, f),
        lerp(a.g, b.g, f),
        lerp(a.b, b.b, f),
        lerp(a.a, b.a, f),
    )
}

fn eval_progress(spec: &ContentProgress, p: &PartParams, time: f32, clamp: bool) -> f32 {
    let converted = convert_progress(spec);
    if clamp {
        converted.get_clamp(p, time)
    } else {
        converted.get(p, time)
    }
}

/// Converts a plan-02 metadata progress tree into the evaluator tree.
pub fn convert_progress(spec: &ContentProgress) -> PartProgressSpec {
    use ContentProgress as C;
    use PartProgressSpec as P;
    let b = |x: &ContentProgress| Box::new(convert_progress(x));
    match spec {
        C::Reload => P::Reload,
        C::SmoothReload => P::SmoothReload,
        C::Warmup => P::Warmup,
        C::Charge => P::Charge,
        C::Recoil => P::Recoil,
        C::Heat => P::Heat,
        C::Life => P::Life,
        C::Time => P::Time,
        C::Constant(v) => P::Constant(*v),
        C::Inv(a) => P::Inv(b(a)),
        C::Slope(a) => P::Slope(b(a)),
        C::Clamp(a) => P::Clamp(b(a)),
        C::Add(a, v) => P::AddConst(b(a), *v),
        C::AddProgress(a, c) => P::Add(b(a), b(c)),
        C::Delay(a, v) => P::Delay(b(a), *v),
        C::CurveRange(a, o, d) => P::Curve(b(a), *o, *d),
        C::CurveInterp(a, i) => P::CurveInterp(b(a), convert_interp(*i)),
        C::Sustain(a, o, g, s) => P::Sustain(b(a), *o, *g, *s),
        C::Shorten(a, v) => P::Shorten(b(a), *v),
        C::Compress(a, s, e) => P::Compress(b(a), *s, *e),
        C::Blend(a, c, v) => P::Blend(b(a), b(c), *v),
        C::MulProgress(a, c) => P::Mul(b(a), b(c)),
        C::Mul(a, v) => P::MulConst(b(a), *v),
        C::Min(a, c) => P::Min(b(a), b(c)),
        C::Sin(a, o, s, m) => P::SinTime(b(a), *o, *s, *m),
        C::Absin(a, s, m) => P::Absin(b(a), *s, *m),
        C::Mod(a, v) => P::Mod(b(a), *v),
        C::Loop(a, t) => P::Loop(b(a), *t),
        C::AbsinTime { offset, scl, mag } => {
            P::AbsinTime(Box::new(P::Constant(0.0)), *offset, *scl, *mag)
        }
    }
}

fn convert_interp(kind: crate::content::registries::units::parts::InterpKind) -> Interp {
    use crate::content::registries::units::parts::InterpKind as K;
    match kind {
        K::Linear => Interp::Linear,
        K::One => Interp::One,
        K::Slope => Interp::Slope,
        K::Pow2In => Interp::Pow2In,
        K::Pow5In => Interp::Pow5In,
    }
}

/// Unused-parameter helper for `PartFunc` (kept for API completeness).
#[allow(dead_code)]
fn _part_func_type(_: PartFunc) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::registries::units::parts::DrawPartSpec;

    fn params() -> PartParams {
        PartParams {
            warmup: 0.5,
            reload: 0.25,
            smooth_reload: 0.25,
            heat: 0.75,
            recoil: 0.0,
            charge: 0.0,
            x: 100.0,
            y: 200.0,
            rotation: 0.0,
            side_override: -1,
            side_multiplier: 1,
            life: 0.0,
        }
    }

    fn prim_count(program: &DrawProgram) -> usize {
        program.len()
    }

    #[test]
    fn region_suffix_resolution() {
        let mut p = DrawPartSpec::region("-glow");
        p.layer = 110.0;
        let names = RegionNames::resolve(&p, "flare");
        assert_eq!(names.regions, vec!["flare-glow"]);
        assert_eq!(names.heat, "flare-glow-heat");
        assert_eq!(names.light, "flare-glow-light");

        p.mirror = true;
        p.turret_shading = true;
        let names = RegionNames::resolve(&p, "duo");
        assert_eq!(names.regions, vec!["duo-glow-r", "duo-glow-l"]);
        assert_eq!(
            names.outlines,
            vec!["duo-glow-r-outline", "duo-glow-l-outline"]
        );
    }

    #[test]
    fn mirror_side_override_picks_side() {
        let mut lookup = MapRegions::new();
        lookup
            .add("turret-glow-r", 32.0, 32.0)
            .add("turret-glow-l", 32.0, 32.0);
        let mut p = DrawPartSpec::region("-glow");
        p.layer = 110.0;
        p.mirror = true;
        p.turret_shading = true;
        p.outline = false;

        let mut program = DrawProgram::new();
        draw_named_parts(
            &mut program,
            std::slice::from_ref(&p),
            &params(),
            &lookup,
            0.0,
            "turret",
        );
        assert_eq!(prim_count(&program), 2);
        let regions: Vec<&str> = program
            .prims
            .iter()
            .map(|prim| match &prim.kind {
                PrimKind::Region { region, .. } => region.0,
                _ => panic!("expected region"),
            })
            .collect();
        assert!(regions.contains(&"turret-glow-r"));
        assert!(regions.contains(&"turret-glow-l"));

        // sideOverride -1 with mirror => 1 prim (only one side).
        let mut one = params();
        one.side_override = 1;
        one.side_multiplier = -1;
        let mut program2 = DrawProgram::new();
        draw_named_parts(
            &mut program2,
            std::slice::from_ref(&p),
            &one,
            &lookup,
            0.0,
            "turret",
        );
        assert_eq!(prim_count(&program2), 1);
        match &program2.prims[0].kind {
            PrimKind::Region { region, .. } => assert_eq!(region.0, "turret-glow-l"),
            _ => panic!("expected region"),
        }
    }

    #[test]
    fn region_progress_moves_and_rotates() {
        let mut lookup = MapRegions::new();
        lookup.add("turret-part", 32.0, 32.0);
        let mut p = DrawPartSpec::region("-part");
        p.layer = 110.0;
        p.draw_region = false; // skip region, exercise move via heat only
        p.x = 8.0;
        p.move_x = 4.0;
        p.progress = ContentProgress::Warmup;
        let mut program = DrawProgram::new();
        draw_named_parts(
            &mut program,
            std::slice::from_ref(&p),
            &params(),
            &lookup,
            0.0,
            "turret",
        );
        // draw_region false => no region prim (heat/light names resolve missing).
        assert_eq!(prim_count(&program), 0);
    }

    #[test]
    fn shape_part_emits_poly() {
        let lookup = MapRegions::new();
        let mut p = DrawPartSpec::shape();
        p.layer = 110.0;
        p.circle = false;
        p.sides = 3;
        p.radius = 5.0;
        p.color = Some(Rgba::WHITE);
        let mut program = DrawProgram::new();
        draw_named_parts(
            &mut program,
            std::slice::from_ref(&p),
            &params(),
            &lookup,
            0.0,
            "",
        );
        assert_eq!(prim_count(&program), 1);
        match &program.prims[0].kind {
            PrimKind::Poly { sides, r, fill, .. } => {
                assert_eq!(*sides, 3);
                assert_eq!(*r, 5.0);
                assert!(*fill);
            }
            _ => panic!("expected poly"),
        }
    }

    #[test]
    fn hover_part_emits_circle_strokes() {
        let lookup = MapRegions::new();
        let mut p = DrawPartSpec::hover();
        p.layer = 110.0;
        p.circles = 2;
        let mut program = DrawProgram::new();
        draw_named_parts(
            &mut program,
            std::slice::from_ref(&p),
            &params(),
            &lookup,
            5.0,
            "",
        );
        assert_eq!(prim_count(&program), 2);
        assert!(
            program
                .prims
                .iter()
                .all(|prim| matches!(prim.kind, PrimKind::Poly { fill: false, .. }))
        );
    }

    #[test]
    fn flare_part_emits_tri_fans() {
        let lookup = MapRegions::new();
        let mut p = DrawPartSpec::flare();
        p.sides = 4;
        let mut program = DrawProgram::new();
        draw_named_parts(
            &mut program,
            std::slice::from_ref(&p),
            &params(),
            &lookup,
            0.0,
            "",
        );
        assert_eq!(prim_count(&program), 8);
        assert!(
            program
                .prims
                .iter()
                .all(|prim| matches!(prim.kind, PrimKind::Tri { .. }))
        );
        assert_eq!(program.prims[0].z, 110.0);
    }

    #[test]
    fn children_inherit_params_and_side() {
        let mut lookup = MapRegions::new();
        lookup.add("turret-child", 16.0, 16.0);
        let mut child = DrawPartSpec::region("-child");
        child.layer = 110.0;
        child.draw_region = true;
        child.outline = false;
        let mut parent = DrawPartSpec::region("-parent");
        parent.layer = 110.0;
        parent.suffix = "-parent".to_owned();
        parent.draw_region = false;
        parent.children.push(child);
        let mut program = DrawProgram::new();
        draw_named_parts(
            &mut program,
            std::slice::from_ref(&parent),
            &params(),
            &lookup,
            0.0,
            "turret",
        );
        assert_eq!(prim_count(&program), 1);
    }

    #[test]
    fn outlines_list_walks_children() {
        let mut child = DrawPartSpec::region("-fin");
        child.outline = true;
        let mut parent = DrawPartSpec::region("-blade");
        parent.outline = true;
        parent.children.push(child);
        let names = get_outlines(std::slice::from_ref(&parent), "wraith");
        assert_eq!(names, vec!["wraith-blade", "wraith-fin"]);
    }

    #[test]
    fn progress_combinators_convert() {
        use ContentProgress as C;
        let p = params();
        let spec = C::CurveInterp(
            Box::new(C::Warmup),
            crate::content::registries::units::parts::InterpKind::Pow2In,
        );
        let ev = convert_progress(&spec);
        assert_eq!(ev.get(&p, 0.0), Interp::Pow2In.apply(0.5));
        let spec = C::AbsinTime {
            offset: 0.0,
            scl: 1.0,
            mag: 1.0,
        };
        let ev = convert_progress(&spec);
        let expected = (f32::sin(10.0 / 2.0) * 1.0 + 1.0) / 2.0;
        assert!((ev.get(&p, 10.0) - expected).abs() < 1e-5);
    }

    #[test]
    fn spawner_interval_fires_deterministically() {
        let lookup = MapRegions::new();
        let spec = EffectSpawnerPartSpec {
            effect: crate::content::EffectId::SMOKE,
            effect_interval: 5.0,
            width: 8.0,
            height: 8.0,
            ..Default::default()
        };
        let mut state = SpawnerState::seeded(7);
        let mut program = DrawProgram::new();
        let mut emit = PartEmit::new(&mut program, &lookup, 0.0);
        let mut spawned = 0;
        for _ in 0..20 {
            draw_spawner(
                &mut emit,
                &spec,
                &params(),
                &mut state,
                1.0,
                false,
                |_, _, _, _, _| {
                    spawned += 1;
                },
            );
        }
        assert_eq!(spawned, 4);
    }
}

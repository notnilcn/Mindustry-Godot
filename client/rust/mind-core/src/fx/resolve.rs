// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Effect → [`DrawProgram`] resolution (plan 17 §3.6).
//!
//! Godot-free: `mind-gdext` calls [`build_program`] each rendered frame and
//! executes/sorts the result. Also the core of `mind-headless fx program`
//! golden tests.

use crate::content::Rgba;
use crate::math::{ArcRand, curve, curve_offset};
use crate::render::draw::{DrawPrim, DrawProgram, PrimKind, RegionKey};
use crate::render::layer::Layer;

use super::angles::{angle, rand_len_vectors, trnsx, trnsx_vec, trnsy, trnsy_vec};
use super::container::EffectContainer;
use super::custom::{FxEmit, dispatch};
use super::data::ViewSnapshot;
use super::def::{
    EffectDef, EffectKind, ExplosionParams, NoiseParams, ParticleParams, TriangleParams, WaveParams,
};
use super::pool::EffectState;

/// Resolves a live state into a sorted draw program.
pub fn build_program(
    def: &EffectDef,
    state: &EffectState,
    snapshot: &dyn ViewSnapshot,
    program: &mut DrawProgram,
) {
    program.clear();
    program.clip = def.clip;
    build_program_into(def, state, snapshot, program);
    program.sort();
}

/// Appends a state's primitives to `program` without clearing or sorting.
pub fn build_program_into(
    def: &EffectDef,
    state: &EffectState,
    snapshot: &dyn ViewSnapshot,
    program: &mut DrawProgram,
) {
    build_program_into_lod(def, state, snapshot, program, false);
}

/// LOD-aware [`build_program_into`] (plan 17 §3.14): at `l2`, particle counts
/// halve (floor 1). Quality-only; gameplay is unaffected.
pub fn build_program_into_lod(
    def: &EffectDef,
    state: &EffectState,
    snapshot: &dyn ViewSnapshot,
    program: &mut DrawProgram,
    l2: bool,
) {
    let container = EffectContainer {
        id: def.id,
        x: state.x,
        y: state.y,
        time: state.time,
        lifetime: state.lifetime,
        rotation: state.rotation,
        color: state.color,
        data: state.data.clone(),
        inner: None,
    };
    render_def(def, &container, snapshot, program, l2);
}

fn render_def(
    def: &EffectDef,
    e: &EffectContainer,
    snapshot: &dyn ViewSnapshot,
    program: &mut DrawProgram,
    l2: bool,
) {
    match &def.kind {
        EffectKind::None | EffectKind::Multi(_) | EffectKind::Radial(_) | EffectKind::Wrap(_) => {}
        EffectKind::Unported => {}
        EffectKind::Custom(id, _) => {
            let mut emit = FxEmit::new(program, def.layer);
            dispatch(*id)(&mut emit, e, snapshot);
        }
        EffectKind::Particle(params) => particle(params, e, program, def.layer, l2),
        EffectKind::Explosion(params) => explosion(params, e, program, def.layer, l2),
        EffectKind::Wave(params) => wave(params, e, program, def.layer),
        EffectKind::Triangle(params) => triangle(params, e, program, def.layer),
        EffectKind::Noise(params) => noise(params, e, program),
        EffectKind::Sound(params) => {
            // Sound playback is a one-way sink call (plan 18); the render half
            // is the child effect.
            let child = super::def::registry().get(params.effect);
            render_def(child, e, snapshot, program, l2);
        }
        EffectKind::Seq(children) => seq(children, e, snapshot, program, l2),
    }
}

/// `SeqEffect.render`: find the active child and render within its window.
fn seq(
    children: &smallvec::SmallVec<[crate::content::EffectId; 4]>,
    e: &EffectContainer,
    snapshot: &dyn ViewSnapshot,
    program: &mut DrawProgram,
    l2: bool,
) {
    let life = e.time;
    let mut sum = 0.0f32;
    for &child_id in children.iter() {
        let child = super::def::registry().get(child_id);
        if life <= child.lifetime + sum {
            let mut inner = e.clone();
            inner.id = child_id;
            inner.time = life - sum;
            inner.lifetime = child.lifetime;
            inner.inner = None;
            render_def(child, &inner, snapshot, program, l2);
            return;
        }
        sum += child.lifetime;
    }
}

fn particle(
    params: &ParticleParams,
    e: &EffectContainer,
    program: &mut DrawProgram,
    layer: f32,
    l2: bool,
) {
    let real_rotation = if params.use_rotation {
        if params.casing_flip {
            e.rotation.abs()
        } else {
            e.rotation
        }
    } else {
        0.0
    };
    let flip = if params.casing_flip {
        -crate::fx::angles::sign(e.rotation) as f32
    } else {
        1.0
    };
    let rawfin = e.fin();
    let fin = e.fin_with(params.interp);
    let col_fin = e.fin_with(params.color_interp);
    let size_curve = curve(
        rawfin,
        if e.lifetime == 0.0 {
            0.0
        } else {
            params.size_change_start / e.lifetime
        },
        1.0,
    );
    let rad = params
        .size_interp
        .apply_range(params.size_from, params.size_to, size_curve)
        * 2.0;
    let width = rad
        * params.width_interp.apply_range(
            params.width_from,
            params.width_to,
            curve(
                rawfin,
                if e.lifetime == 0.0 {
                    0.0
                } else {
                    params.width_change_start / e.lifetime
                },
                1.0,
            ),
        );
    let height = rad
        * params.height_interp.apply_range(
            params.height_from,
            params.height_to,
            curve(
                rawfin,
                if e.lifetime == 0.0 {
                    0.0
                } else {
                    params.height_change_start / e.lifetime
                },
                1.0,
            ),
        );
    let ox = e.x + trnsx_vec(real_rotation, params.offset_x * flip, params.offset_y);
    let oy = e.y + trnsy_vec(real_rotation, params.offset_x * flip, params.offset_y);

    let mut color = Rgba::new(
        params.color_from.r + (params.color_to.r - params.color_from.r) * col_fin,
        params.color_from.g + (params.color_to.g - params.color_from.g) * col_fin,
        params.color_from.b + (params.color_to.b - params.color_from.b) * col_fin,
        params.color_from.a + (params.color_to.a - params.color_from.a) * col_fin,
    );
    color.a *= e.color.a;
    let light_color = params.light_color.unwrap_or(color);

    let mut rand = ArcRand::new(e.id.raw() as u64);
    let particles = super::batch::lod_particle_count(params.particles, l2);
    if params.line {
        let stroke = params
            .size_interp
            .apply_range(params.stroke_from, params.stroke_to, rawfin);
        let len = params
            .size_interp
            .apply_range(params.len_from, params.len_to, rawfin);
        for _ in 0..particles {
            let l = params.length * fin + params.base_length;
            let a = real_rotation + rand.range_float(params.cone);
            let dist = if params.rand_length {
                rand.random_float(l)
            } else {
                l
            };
            let x = trnsx(a, dist);
            let y = trnsy(a, dist);
            program.push(DrawPrim::at(
                layer,
                PrimKind::Line {
                    x1: ox + x,
                    y1: oy + y,
                    x2: ox + x + trnsx(angle(x, y), len),
                    y2: oy + y + trnsy(angle(x, y), len),
                    stroke,
                    color,
                    cap: params.cap,
                },
            ));
            program.push(DrawPrim::at(
                layer,
                PrimKind::Light {
                    x: ox + x,
                    y: oy + y,
                    radius: len * params.light_scl,
                    color: light_color,
                    opacity: params.light_opacity * color.a,
                },
            ));
        }
    } else {
        for _ in 0..particles {
            let l = params.length * fin + params.base_length;
            let a = real_rotation + rand.range_float(params.cone);
            let dist = if params.rand_length {
                rand.random_float(l)
            } else {
                l
            };
            let x = trnsx(a, dist);
            let y = trnsy(a, dist);
            program.push(DrawPrim::at(
                layer,
                PrimKind::Region {
                    region: RegionKey(params.region),
                    x: ox + x,
                    y: oy + y,
                    w: width,
                    h: height,
                    rotation_deg: real_rotation + params.offset + e.time * params.spin,
                    origin: (0.5, 0.5),
                    color,
                    mix: None,
                    wrap: false,
                },
            ));
            program.push(DrawPrim::at(
                layer,
                PrimKind::Light {
                    x: ox + x,
                    y: oy + y,
                    radius: rad * params.light_scl,
                    color: light_color,
                    opacity: params.light_opacity * color.a,
                },
            ));
        }
    }
}

fn explosion(
    params: &ExplosionParams,
    e: &EffectContainer,
    program: &mut DrawProgram,
    layer: f32,
    l2: bool,
) {
    let mut emit = FxEmit::new(program, layer);
    emit.color(params.wave_color);
    e.scaled_view(params.wave_life, |s| {
        emit.stroke(params.wave_stroke * s.fout());
        emit.circle_line(e.x, e.y, params.wave_rad_base + s.fin() * params.wave_rad);
    });
    emit.color(params.smoke_color);
    if params.smoke_size > 0.0 {
        rand_len_vectors(
            e.id.raw() as u64,
            super::batch::lod_particle_count(params.smokes, l2),
            2.0 + params.smoke_rad * e.finpow(),
            |x, y| {
                emit.circle(
                    e.x + x,
                    e.y + y,
                    e.fout() * params.smoke_size + params.smoke_size_base,
                );
            },
        );
    }
    emit.color(params.spark_color);
    emit.stroke(e.fout() * params.spark_stroke);
    rand_len_vectors(
        e.id.raw() as u64 + 1,
        super::batch::lod_particle_count(params.sparks, l2),
        1.0 + params.spark_rad * e.finpow(),
        |x, y| {
            emit.line_angle(
                e.x + x,
                e.y + y,
                angle(x, y),
                1.0 + e.fout() * params.spark_len,
            );
            emit.light(
                e.x + x,
                e.y + y,
                e.fout() * params.spark_len * 4.0,
                params.spark_color,
                0.7,
            );
        },
    );
}

fn wave(params: &WaveParams, e: &EffectContainer, program: &mut DrawProgram, layer: f32) {
    let fin = e.fin();
    let ifin = e.fin_with(params.interp);
    let ox = e.x + trnsx_vec(e.rotation, params.offset_x, params.offset_y);
    let oy = e.y + trnsy_vec(e.rotation, params.offset_x, params.offset_y);
    let mut emit = FxEmit::new(program, layer);
    emit.color_lerp(params.color_from, params.color_to, ifin);
    emit.stroke(
        params
            .interp
            .apply_range(params.stroke_from, params.stroke_to, fin),
    );
    let rad = params
        .interp
        .apply_range(params.size_from, params.size_to, fin);
    let sides = if params.sides <= 0 {
        circle_vertices(rad)
    } else {
        params.sides as u16
    };
    emit.poly_line(ox, oy, sides, rad, params.rotation + e.rotation);
    let light = params.light_color.unwrap_or(emit.get_color());
    emit.light(
        ox,
        oy,
        rad * params.light_scl,
        light,
        params.light_opacity * e.fin_with(params.light_interp),
    );
}

fn triangle(params: &TriangleParams, e: &EffectContainer, program: &mut DrawProgram, layer: f32) {
    let real_rotation = if params.use_rotation {
        if params.flippable {
            e.rotation.abs()
        } else {
            e.rotation
        }
    } else {
        0.0
    };
    let rawfin = e.fin();
    let col_fin = e.fin_with(params.interp);
    let width = params.width_interp.apply_range(
        params.width_from,
        params.width_to,
        curve(rawfin, 0.0, 1.0),
    );
    let height = params.height_interp.apply_range(
        params.height_from,
        params.height_to,
        curve(rawfin, 0.0, 1.0),
    );
    let light_opac = params.color_interp.apply_range(
        params.light_opacity_from,
        params.light_opacity_to,
        curve(rawfin, 0.0, 1.0),
    );
    let cx = params
        .interp
        .apply_range(params.start_x, params.end_x, curve(rawfin, 0.0, 1.0));
    let cy = params
        .interp
        .apply_range(params.start_y, params.end_y, curve(rawfin, 0.0, 1.0));
    let (sin, cos) = real_rotation.to_radians().sin_cos();
    let px = cx * cos - cy * sin;
    let py = cx * sin + cy * cos;
    let mut emit = FxEmit::new(program, layer);
    emit.color_lerp(params.color_from, params.color_to, col_fin);
    let light = params.light_color.unwrap_or(emit.get_color());
    emit.tri(
        px + e.x,
        py + e.y,
        width,
        height,
        real_rotation + params.offset + e.time * params.spin as f32,
    );
    emit.light(
        px + e.x,
        py + e.y,
        params.light_scl,
        light,
        light_opac * e.color.a,
    );
}

fn noise(params: &NoiseParams, e: &EffectContainer, program: &mut DrawProgram) {
    // `NoiseEffect` never clips (plan 17 §2.4 #9); the layer scroll math is
    // computed here and executed as a shader blit by plan 16.
    let mut sspeed = 1.0f32;
    let mut salpha = 1.0f32;
    let mut offset = 0.0f32;
    let base = params.color.unwrap_or(e.color);
    let mut col = Rgba::new(base.r, base.g, base.b, base.a * e.fout());
    for _ in 0..params.layers {
        let speed = sspeed * params.base_speed * params.intensity;
        let scroll = [-(params.wind_x * speed), -(params.wind_y * speed)];
        program.push(DrawPrim::at(
            Layer::Weather.z(),
            PrimKind::NoiseLayer {
                texture: crate::render::draw::TextureKey(params.noise_path),
                rect: [0.0, 0.0, 0.0, 0.0],
                tint: col,
                opacity: salpha * params.opacity,
                scroll,
                offset,
            },
        ));
        sspeed *= params.layer_speed_mul;
        salpha *= params.layer_alpha_mul;
        offset += 0.29;
        col.r *= params.layer_color_mul;
        col.g *= params.layer_color_mul;
        col.b *= params.layer_color_mul;
    }
}

/// `Lines.circleVertices(rad)`.
fn circle_vertices(rad: f32) -> u16 {
    (11.0 + rad).max(3.0) as u16
}

/// Unused helper retained for parity tests of `curve_offset`.
#[allow(dead_code)]
pub(crate) fn particle_start_ratio(start: f32, lifetime: f32) -> f32 {
    curve_offset(start, lifetime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{EffectId, Rgba};
    use crate::fx::data::{EffectData, EmptySnapshot};
    use crate::fx::def::registry;
    use crate::fx::pool::EffectState;

    fn state(id: EffectId) -> EffectState {
        let def = registry().get(id);
        EffectState {
            def: id,
            x: 100.0,
            y: 100.0,
            rotation: 0.0,
            color: Rgba::WHITE,
            time: 7.0,
            lifetime: def.lifetime,
            data: EffectData::None,
            parent: None,
            rot_with_parent: false,
            offset_x: 0.0,
            offset_y: 0.0,
            offset_pos: 0.0,
            offset_rot: 0.0,
            lifetime_override: None,
            alive: true,
        }
    }

    #[test]
    fn every_ported_custom_body_emits_deterministically() {
        let reg = registry();
        let mut ported = 0;
        for def in reg.iter() {
            if !matches!(def.kind, EffectKind::Custom(..)) {
                continue;
            }
            ported += 1;
            let mut a = DrawProgram::new();
            let id = def.id;
            let mut st = state(id);
            st.time = (def.lifetime * 0.5).max(0.0);
            build_program(def, &st, &EmptySnapshot, &mut a);
            let mut b = DrawProgram::new();
            build_program(def, &st, &EmptySnapshot, &mut b);
            assert_eq!(
                a.hash(),
                b.hash(),
                "non-deterministic body for {}",
                def.name
            );
        }
        assert!(
            ported >= 80,
            "expected >=80 ported custom bodies, got {ported}"
        );
    }

    #[test]
    fn first_effect_program() {
        let def = registry().get(EffectId::HIT_BULLET_SMALL);
        let mut program = DrawProgram::new();
        build_program(
            def,
            &state(EffectId::HIT_BULLET_SMALL),
            &EmptySnapshot,
            &mut program,
        );
        assert!(!program.is_empty());
        assert!(program.prims.iter().all(|p| p.z == def.layer));
        // Deterministic hash.
        let mut other = DrawProgram::new();
        build_program(
            def,
            &state(EffectId::HIT_BULLET_SMALL),
            &EmptySnapshot,
            &mut other,
        );
        assert_eq!(program.hash(), other.hash());
    }

    #[test]
    fn explosion_program_has_circles_and_lines() {
        let def = registry().get(EffectId::EXPLOSION);
        let mut program = DrawProgram::new();
        build_program(
            def,
            &state(EffectId::EXPLOSION),
            &EmptySnapshot,
            &mut program,
        );
        assert!(
            program
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Circle { .. }))
        );
        assert!(
            program
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Line { .. }))
        );
    }

    #[test]
    fn smoke_program_has_one_circle() {
        let def = registry().get(EffectId::SMOKE);
        let mut program = DrawProgram::new();
        build_program(def, &state(EffectId::SMOKE), &EmptySnapshot, &mut program);
        assert_eq!(program.len(), 1);
    }

    fn decl_def(kind: EffectKind) -> EffectDef {
        let mut def = EffectDef::blank(EffectId(1), "test", 30.0, 100.0);
        def.kind = kind;
        def
    }

    fn state_of(def: &EffectDef, time: f32) -> EffectState {
        EffectState {
            def: def.id,
            x: 50.0,
            y: 50.0,
            rotation: 0.0,
            color: Rgba::WHITE,
            time,
            lifetime: def.lifetime,
            data: EffectData::None,
            parent: None,
            rot_with_parent: false,
            offset_x: 0.0,
            offset_y: 0.0,
            offset_pos: 0.0,
            offset_rot: 0.0,
            lifetime_override: None,
            alive: true,
        }
    }

    #[test]
    fn declarative_particle_emits_regions_and_lights() {
        let params = crate::fx::def::ParticleParams {
            particles: 3,
            length: 10.0,
            light_opacity: 0.5,
            ..Default::default()
        };
        let def = decl_def(EffectKind::Particle(params));
        let mut program = DrawProgram::new();
        build_program(&def, &state_of(&def, 5.0), &EmptySnapshot, &mut program);
        assert_eq!(
            program
                .prims
                .iter()
                .filter(|p| matches!(p.kind, PrimKind::Region { .. }))
                .count(),
            3
        );
        assert_eq!(
            program
                .prims
                .iter()
                .filter(|p| matches!(p.kind, PrimKind::Light { .. }))
                .count(),
            3
        );
    }

    #[test]
    fn declarative_wave_emits_poly_line_and_light() {
        let params = crate::fx::def::WaveParams {
            size_to: 40.0,
            ..Default::default()
        };
        let def = decl_def(EffectKind::Wave(params));
        let mut program = DrawProgram::new();
        build_program(&def, &state_of(&def, 10.0), &EmptySnapshot, &mut program);
        assert!(
            program
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Poly { .. }))
        );
        assert!(
            program
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Light { .. }))
        );
    }

    #[test]
    fn declarative_triangle_emits_tri_and_light() {
        let def = decl_def(EffectKind::Triangle(Default::default()));
        let mut program = DrawProgram::new();
        build_program(&def, &state_of(&def, 10.0), &EmptySnapshot, &mut program);
        assert!(
            program
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Tri { .. }))
        );
        assert!(
            program
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Light { .. }))
        );
    }

    #[test]
    fn lod_halves_particle_prims() {
        let params = crate::fx::def::ParticleParams {
            particles: 8,
            length: 10.0,
            ..Default::default()
        };
        let def = decl_def(EffectKind::Particle(params));
        let state = state_of(&def, 5.0);

        let mut full = DrawProgram::new();
        build_program_into_lod(&def, &state, &EmptySnapshot, &mut full, false);
        let mut l2 = DrawProgram::new();
        build_program_into_lod(&def, &state, &EmptySnapshot, &mut l2, true);
        assert_eq!(full.len(), 16); // 8 regions + 8 lights
        assert_eq!(l2.len(), 8); // 4 regions + 4 lights
    }

    #[test]
    fn declarative_noise_emits_layers() {
        let params = crate::fx::def::NoiseParams {
            layers: 3,
            ..Default::default()
        };
        let def = decl_def(EffectKind::Noise(params));
        let mut program = DrawProgram::new();
        build_program(&def, &state_of(&def, 10.0), &EmptySnapshot, &mut program);
        assert_eq!(program.len(), 3);
        assert!(
            program
                .prims
                .iter()
                .all(|p| matches!(p.kind, PrimKind::NoiseLayer { .. }))
        );
    }
}

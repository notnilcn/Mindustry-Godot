// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! FX batching policy and budgets (plan 17 §3.14 / §7d).
//!
//! Pure planning over an already-sorted [`DrawProgram`]: adjacent region prims
//! sharing `(z, blend, region)` merge into one draw; large runs select the
//! `MultiMesh2D`/`GPUParticles2D` paths. The gdext executor consumes this
//! without allocating per frame.

use crate::render::draw::{Blending, DrawPrim, PrimKind, RegionKey};
use crate::render::draw::{GPUPARTICLES_THRESHOLD, MULTIMESH_THRESHOLD};

/// Where a batch is submitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchBackend {
    /// One immediate `draw_texture_rect` per instance.
    Single,
    /// One `MultiMesh2D` draw for the whole run.
    MultiMesh,
    /// One `GPUParticles2D` batch (very high instance counts).
    GpuParticles,
}

/// Chooses a backend for `instances` per plan 17 §3.14.
pub fn choose_backend(instances: usize) -> BatchBackend {
    if instances as u32 >= GPUPARTICLES_THRESHOLD {
        BatchBackend::GpuParticles
    } else if instances as u32 >= MULTIMESH_THRESHOLD {
        BatchBackend::MultiMesh
    } else {
        BatchBackend::Single
    }
}

/// A merged run of adjacent region prims with the same batch key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionBatch {
    /// Layer.
    pub z: f32,
    /// Blend mode.
    pub blend: Blending,
    /// Atlas region.
    pub region: RegionKey,
    /// Index of the first prim in the program.
    pub start: usize,
    /// Number of instances.
    pub count: usize,
    /// Chosen backend.
    pub backend: BatchBackend,
}

/// Groups adjacent `Region` prims with a shared `(z, blend, region)` key.
pub fn batching_runs(prims: &[DrawPrim]) -> Vec<RegionBatch> {
    let mut out: Vec<RegionBatch> = Vec::new();
    for (i, prim) in prims.iter().enumerate() {
        let PrimKind::Region { region, .. } = &prim.kind else {
            continue;
        };
        if let Some(last) = out.last_mut()
            && last.start + last.count == i
            && last.z == prim.z
            && last.blend == prim.blend
            && last.region == *region
        {
            last.count += 1;
            last.backend = choose_backend(last.count);
            continue;
        }
        out.push(RegionBatch {
            z: prim.z,
            blend: prim.blend,
            region: *region,
            start: i,
            count: 1,
            backend: choose_backend(1),
        });
    }
    out
}

/// Number of draw calls the batching executor would submit for `prims`.
///
/// Adjacent same-key regions count as one call regardless of backend; every
/// non-region prim is one call.
pub fn draw_call_count(prims: &[DrawPrim]) -> usize {
    let mut calls = 0usize;
    let mut i = 0usize;
    while i < prims.len() {
        match &prims[i].kind {
            PrimKind::Region { region, .. } => {
                let mut j = i + 1;
                while j < prims.len()
                    && prims[j].z == prims[i].z
                    && prims[j].blend == prims[i].blend
                    && matches!(&prims[j].kind, PrimKind::Region { region: r, .. } if r == region)
                {
                    j += 1;
                }
                calls += 1;
                i = j;
            }
            _ => {
                calls += 1;
                i += 1;
            }
        }
    }
    calls
}

/// Stable rank for a [`Blending`] key.
fn blend_rank(blend: Blending) -> u8 {
    match blend {
        Blending::Normal => 0,
        Blending::Additive => 1,
        Blending::Multiply => 2,
    }
}

/// Discriminant tag for a batched shape prim (per-instance geometry varies but
/// the mesh/shader is shared). `None` for prims that are inherently one draw
/// each (`NoiseLayer`/`ShaderBlit`).
fn shape_tag(kind: &PrimKind) -> Option<u32> {
    Some(match kind {
        PrimKind::Rect { .. } => 0,
        PrimKind::Circle { fill, .. } => 1 + u32::from(*fill),
        PrimKind::Poly { sides, fill, .. } => 3 + u32::from(*fill) + (*sides as u32) * 2,
        PrimKind::Polygon { fill, .. } => 1000 + u32::from(*fill),
        PrimKind::Tri { .. } => 1002,
        PrimKind::Line { cap, .. } => 1003 + u32::from(*cap),
        PrimKind::Polyline { .. } => 1005,
        PrimKind::Arc { .. } => 1006,
        // `Drawf.light` accumulates into plan 16's single `LightRenderer` pass.
        PrimKind::Light { .. } => 2000,
        PrimKind::Region { .. } | PrimKind::NoiseLayer { .. } | PrimKind::ShaderBlit { .. } => {
            return None;
        }
    })
}

/// Draw calls a fully-batched executor submits for `prims` (plan 16 §7.4 /
/// plan 17 §7d): every `(z, blend, region)` group is **one** vertex/MultiMesh
/// bank regardless of adjacency, every shape kind at a `(z, blend)` is one
/// instanced draw, and `NoiseLayer`/`ShaderBlit` stay one draw each. Unlike
/// [`draw_call_count`] (adjacent runs), this models the material/MultiMesh
/// collapse that makes the `mid` FX profile fit the ≤200 target.
pub fn batched_draw_call_count(prims: &[DrawPrim]) -> usize {
    use std::collections::BTreeMap;
    let mut region_groups: BTreeMap<(u32, u8, &'static str), u32> = BTreeMap::new();
    let mut shape_groups: BTreeMap<(u32, u8, u32), u32> = BTreeMap::new();
    let mut uniques = 0usize;
    for prim in prims {
        match &prim.kind {
            PrimKind::Region { region, .. } => {
                *region_groups
                    .entry((prim.z.to_bits(), blend_rank(prim.blend), region.0))
                    .or_insert(0) += 1;
            }
            kind => match shape_tag(kind) {
                Some(tag) => {
                    *shape_groups
                        .entry((prim.z.to_bits(), blend_rank(prim.blend), tag))
                        .or_insert(0) += 1;
                }
                None => uniques += 1,
            },
        }
    }
    region_groups.len() + shape_groups.len() + uniques
}

/// LOD particle-count policy (plan 17 §3.14): at `Lod::l2`, halve (floor 1).
pub fn lod_particle_count(base: i32, l2: bool) -> i32 {
    if l2 { (base / 2).max(1) } else { base }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Rgba;

    fn region(x: f32, z: f32, blend: Blending, name: &'static str) -> DrawPrim {
        DrawPrim {
            z,
            blend,
            kind: PrimKind::Region {
                region: RegionKey(name),
                x,
                y: 0.0,
                w: 1.0,
                h: 1.0,
                rotation_deg: 0.0,
                origin: (0.5, 0.5),
                color: Rgba::WHITE,
                mix: None,
                wrap: false,
            },
        }
    }

    #[test]
    fn adjacent_same_key_regions_merge() {
        let prims = vec![
            region(0.0, 10.0, Blending::Normal, "a"),
            region(1.0, 10.0, Blending::Normal, "a"),
            region(2.0, 10.0, Blending::Normal, "a"),
        ];
        let runs = batching_runs(&prims);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].count, 3);
        assert_eq!(draw_call_count(&prims), 1);
    }

    #[test]
    fn key_change_splits_runs() {
        let prims = vec![
            region(0.0, 10.0, Blending::Normal, "a"),
            region(1.0, 10.0, Blending::Normal, "b"),
            region(2.0, 11.0, Blending::Normal, "a"),
            region(3.0, 10.0, Blending::Additive, "a"),
        ];
        let runs = batching_runs(&prims);
        assert_eq!(runs.len(), 4);
        assert_eq!(draw_call_count(&prims), 4);
    }

    #[test]
    fn thresholds_select_backends() {
        assert_eq!(choose_backend(1), BatchBackend::Single);
        assert_eq!(
            choose_backend(MULTIMESH_THRESHOLD as usize),
            BatchBackend::MultiMesh
        );
        assert_eq!(
            choose_backend(GPUPARTICLES_THRESHOLD as usize),
            BatchBackend::GpuParticles
        );
    }

    #[test]
    fn batched_executor_collapses_non_adjacent_keys() {
        // a, b, a at the same z/blend: the adjacent-run count is 3, but the
        // batched executor groups the two `a` regions into one bank -> 2.
        let prims = vec![
            region(0.0, 10.0, Blending::Normal, "a"),
            region(1.0, 10.0, Blending::Normal, "b"),
            region(2.0, 10.0, Blending::Normal, "a"),
        ];
        assert_eq!(draw_call_count(&prims), 3);
        assert_eq!(batched_draw_call_count(&prims), 2);
        // Different z stays a distinct bank.
        let split = vec![
            region(0.0, 10.0, Blending::Normal, "a"),
            region(1.0, 11.0, Blending::Normal, "a"),
        ];
        assert_eq!(batched_draw_call_count(&split), 2);
        // Same-kind shapes at one `(z, blend)` collapse into one instanced draw.
        let shape = |x: f32| DrawPrim {
            z: 10.0,
            blend: Blending::Normal,
            kind: PrimKind::Circle {
                x,
                y: 0.0,
                r: 1.0,
                fill: true,
                stroke: 0.0,
                color: Rgba::WHITE,
            },
        };
        assert_eq!(batched_draw_call_count(&[shape(0.0), shape(1.0)]), 1);
    }

    #[test]
    fn large_run_merges_to_one_multimesh_call() {
        let mut prims = Vec::new();
        for i in 0..1000 {
            prims.push(region(i as f32, 10.0, Blending::Normal, "a"));
        }
        let runs = batching_runs(&prims);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].backend, BatchBackend::MultiMesh);
        assert_eq!(draw_call_count(&prims), 1);
    }

    #[test]
    fn lod_halves_min_one() {
        assert_eq!(lod_particle_count(10, false), 10);
        assert_eq!(lod_particle_count(10, true), 5);
        assert_eq!(lod_particle_count(1, true), 1);
    }

    #[test]
    fn program_reuse_is_allocation_free_after_warmup() {
        use crate::content::{EffectId, Rgba};
        use crate::fx::data::{EffectData, EmptySnapshot};
        use crate::fx::def::registry;
        use crate::fx::pool::EffectState;
        use crate::fx::resolve::build_program;
        use crate::render::draw::DrawProgram;

        let ids = [
            EffectId::HIT_BULLET_SMALL,
            EffectId::EXPLOSION,
            EffectId::SMOKE,
            EffectId::SHOCKWAVE,
        ];
        let mut program = DrawProgram::new();
        let build = |program: &mut DrawProgram, frame: usize| {
            for i in 0..2000usize {
                let def = registry().get(ids[i % ids.len()]);
                let state = EffectState {
                    def: def.id,
                    x: (i % 64) as f32,
                    y: (i % 32) as f32,
                    rotation: 45.0,
                    color: Rgba::WHITE,
                    time: (frame % 7) as f32,
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
                };
                build_program(def, &state, &EmptySnapshot, program);
            }
        };
        // Warm up to the high-water capacity.
        for frame in 0..4 {
            build(&mut program, frame);
        }
        let capacity = program.prims.capacity();
        for frame in 0..32 {
            build(&mut program, frame);
            assert_eq!(
                program.prims.capacity(),
                capacity,
                "program capacity grew on frame {frame}"
            );
        }
    }
}

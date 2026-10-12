// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `UnitRenderer` — live unit sprites (plan 16 unit bands).
//!
//! Rebuilds one rotated-quad `ArrayMesh` per (unit band, atlas page) from
//! `MindSimHost::unit_draws()` every frame; units move each tick, so the pass
//! has no early-out and hides pooled nodes when a bucket empties. Upstream:
//! the unit draw in `DrawDefault`/`UnitDraw` into `Layer.groundUnit`,
//! `Layer.flyingUnitLow` and `Layer.flyingUnit` (`renderer/Layer.java`).

use std::collections::{BTreeMap, HashMap, HashSet};

use godot::classes::mesh::PrimitiveType;
use godot::classes::{ArrayMesh, Mesh, MeshInstance2D, Node2D, SurfaceTool, Texture2D, Time};
use godot::obj::{NewGd, Singleton};
use godot::prelude::*;

use crate::sim_host::MindSimHost;

use super::atlas_bind::{RegionResolver, color_from};

/// Unit sprite counters forwarded to the renderer stats.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnitStats {
    /// Unit sprites emitted this frame.
    pub sprites: i64,
    /// Regions that fell back to `error`/placeholder.
    pub missing_regions: u64,
}

/// One rotated unit quad.
#[derive(Clone, Copy)]
pub(crate) struct UnitQuad {
    pub(crate) cx: f32,
    pub(crate) cy: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    /// Sprite rotation in degrees (upstream `unit.rotation`).
    pub(crate) rot: f32,
    pub(crate) uv: [f32; 4],
    pub(crate) color: u32,
}

/// One `(band, atlas page)` bucket: its texture and quads.
type UnitBucket = (Option<Gd<Texture2D>>, Vec<UnitQuad>);

/// Live unit draw pass, one bucket per unit layer.
pub struct UnitRenderer {
    host: Gd<MindSimHost>,
    resolver: RegionResolver,
    /// `Draw.scl` equivalent: the port's atlas ships the 4x sprite set
    /// (`scale_marker` width 4), so region pixels are divided by the marker
    /// width to get world pixels (`Control.java:330`).
    sprite_scale: f32,
    /// `[ground, flyingUnitLow, flyingUnit]` band nodes (index = bucket;
    /// `None` when a band is absent).
    bands: Vec<Option<Gd<Node2D>>>,
    /// One pooled `MeshInstance2D` per `(bucket, atlas page)`.
    nodes: HashMap<(usize, i32), Gd<MeshInstance2D>>,
    stats: UnitStats,
}

impl UnitRenderer {
    /// Builds the pass over the unit band nodes in `[ground, low, flying]`
    /// order (`None` for an absent band).
    pub fn new(
        host: Gd<MindSimHost>,
        assets: Option<Gd<crate::assets::MindAssets>>,
        bands: Vec<Option<Gd<Node2D>>>,
    ) -> Self {
        let mut resolver = RegionResolver::new(assets);
        let marker = resolver.resolve("scale_marker");
        let sprite_scale = marker.map(|region| 1.0 / region.w.max(1.0)).unwrap_or(1.0);
        Self {
            host,
            resolver,
            sprite_scale,
            bands,
            nodes: HashMap::new(),
            stats: UnitStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> UnitStats {
        self.stats
    }

    /// Rebuilds the per-bucket unit meshes; called once per frame.
    ///
    /// Composition follows `UnitType.draw` (`:1471-1560`): `drawSoftShadow`,
    /// `drawMech` legs, outline, body, `drawCell`, `drawWeapons`.
    pub fn update(&mut self) {
        let draws = self.host.clone().bind().unit_draws_full();
        let time = Time::singleton().get_ticks_msec() as f32 / 1000.0;
        let mut groups: BTreeMap<(usize, i32), UnitBucket> = BTreeMap::new();
        let mut sprites = 0i64;
        for unit in draws {
            let bucket = if unit.low_altitude {
                1
            } else if unit.flying {
                2
            } else {
                0
            };
            let Some(body) = self.resolver.resolve(&unit.name) else {
                continue;
            };
            // `drawSoftShadow`: `max(region.w, region.h) * region.scl() *
            // softShadowScl * 1.6`, colour `(0,0,0,0.4)`.
            if let Some(shadow) = self.resolver.find("circle-shadow") {
                let size = body.w.max(body.h) * self.sprite_scale;
                let entry = groups
                    .entry((bucket, shadow.page))
                    .or_insert((shadow.texture.clone(), Vec::new()));
                entry.1.push(UnitQuad {
                    cx: unit.x,
                    cy: unit.y,
                    w: size * 1.6,
                    h: size * 1.6,
                    rot: 0.0,
                    uv: shadow.uv,
                    color: 0x0000_0066,
                });
                sprites += 1;
            }
            // `drawMech`: two mirrored `<name>-leg` quads (`Mathf.signs`).
            if let Some(mech) = &unit.mech
                && let Some(leg) = self.resolver.find(&format!("{}-leg", unit.name))
            {
                let transforms = mind_core::render::mech_leg_transforms(
                    mech.base_rotation,
                    mech.walk_time,
                    mech.stride,
                    unit.elevation,
                );
                let extension =
                    mind_core::render::mech_walk_extend(mech.walk_time, mech.stride, false)
                        * (1.0 - unit.elevation);
                for (index, (dx, dy, sx, sy, rot)) in transforms.iter().enumerate() {
                    let sign = if index == 0 { -1.0 } else { 1.0 };
                    let tint = mind_core::render::to_bits(mind_core::render::pal::DARK_METAL);
                    let alpha = (sign * extension / mech.stride.max(0.001)).clamp(0.0, 1.0);
                    let [u0, v0, u1, v1] = leg.uv;
                    let uv = if *sx < 0.0 {
                        [u1, v0, u0, v1]
                    } else {
                        [u0, v0, u1, v1]
                    };
                    let entry = groups
                        .entry((bucket, leg.page))
                        .or_insert((leg.texture.clone(), Vec::new()));
                    entry.1.push(UnitQuad {
                        cx: unit.x + dx,
                        cy: unit.y + dy,
                        w: leg.w * self.sprite_scale,
                        h: leg.h * self.sprite_scale * *sy,
                        rot: *rot,
                        uv,
                        color: mix_rgba(0xffff_ffff, tint, alpha),
                    });
                    sprites += 1;
                }
            }
            // `drawBody` (`rotation - 90`; the port draws trimmed sizes, which
            // is what Arc's `Draw.rect(region, x, y)` does for centered trims).
            {
                let entry = groups
                    .entry((bucket, body.page))
                    .or_insert((body.texture.clone(), Vec::new()));
                entry.1.push(UnitQuad {
                    cx: unit.x,
                    cy: unit.y,
                    w: body.w * self.sprite_scale,
                    h: body.h * self.sprite_scale,
                    rot: unit.rotation - 90.0,
                    uv: body.uv,
                    color: 0xffff_ffff,
                });
                sprites += 1;
            }
            // `drawCell`: `<name>-cell` else `power-cell`, tinted by
            // `UnitType.cellColor`.
            let cell = self
                .resolver
                .find(&format!("{}-cell", unit.name))
                .or_else(|| self.resolver.find("power-cell"));
            if let Some(cell) = cell {
                let team = mind_core::game::team::Team::get(unit.team)
                    .color
                    .to_rgba8888();
                let entry = groups
                    .entry((bucket, cell.page))
                    .or_insert((cell.texture.clone(), Vec::new()));
                entry.1.push(UnitQuad {
                    cx: unit.x,
                    cy: unit.y,
                    w: cell.w * self.sprite_scale,
                    h: cell.h * self.sprite_scale,
                    rot: unit.rotation - 90.0,
                    uv: cell.uv,
                    color: mind_core::render::cell_color(team, unit.healthf, time),
                });
                sprites += 1;
            }
            // `drawWeapons` (`Weapon.draw` mount transform + flip mirror).
            for weapon in &unit.weapons {
                let Some(region) = self.resolver.find(&weapon.name) else {
                    continue;
                };
                let (wx, wy, wr) = mind_core::render::unit_draw::weapon_mount_transform(
                    unit.x,
                    unit.y,
                    unit.rotation,
                    weapon.x,
                    weapon.y,
                    weapon.rotate,
                    weapon.mount_rotation,
                    weapon.base_rotation,
                    weapon.recoil,
                );
                let [u0, v0, u1, v1] = region.uv;
                let uv = if weapon.flip {
                    [u1, v0, u0, v1]
                } else {
                    [u0, v0, u1, v1]
                };
                let entry = groups
                    .entry((bucket, region.page))
                    .or_insert((region.texture.clone(), Vec::new()));
                entry.1.push(UnitQuad {
                    cx: wx,
                    cy: wy,
                    w: region.w * self.sprite_scale,
                    h: region.h * self.sprite_scale,
                    rot: wr,
                    uv,
                    color: 0xffff_ffff,
                });
                sprites += 1;
            }
        }

        let live: HashSet<(usize, i32)> = groups.keys().copied().collect();
        for (key, node) in self.nodes.iter_mut() {
            if !live.contains(key) {
                node.set_visible(false);
            }
        }

        for (key, (texture, quads)) in groups {
            let Some(mesh) = build_unit_mesh(&quads) else {
                continue;
            };
            let node = if let Some(node) = self.nodes.get(&key) {
                node.clone()
            } else {
                let Some(mut band) = self.bands.get(key.0).and_then(|band| band.clone()) else {
                    continue;
                };
                let node = MeshInstance2D::new_alloc();
                // code-instantiated: one pooled node per (unit layer, atlas
                // page); the count is page-driven (plan 16 §6.6).
                band.add_child(&node);
                self.nodes.insert(key, node.clone());
                node
            };
            let mut node = node;
            node.set_mesh(&mesh.upcast::<Mesh>());
            if let Some(texture) = texture {
                node.set_texture(&texture);
            }
            node.set_visible(true);
        }

        self.stats.sprites = sprites;
        self.stats.missing_regions = self.resolver.missing();
    }
}

/// `Draw.mixcol(color, alpha)`: lerp an RGBA8888 toward `target`'s RGB.
fn mix_rgba(base: u32, target: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let from = ((base >> shift) & 0xff) as f32;
        let to = ((target >> shift) & 0xff) as f32;
        (from + (to - from) * t) as u32
    };
    (channel(24) << 24) | (channel(16) << 16) | (channel(8) << 8) | 0xff
}

/// Builds an `ArrayMesh` from rotated quads (two triangles each).
pub(crate) fn build_unit_mesh(quads: &[UnitQuad]) -> Option<Gd<ArrayMesh>> {
    let mut tool = SurfaceTool::new_gd();
    tool.begin(PrimitiveType::TRIANGLES);
    for quad in quads {
        let color = color_from(quad.color);
        let [u0, v0, u1, v1] = quad.uv;
        let radians = quad.rot.to_radians();
        let (sin, cos) = (radians.sin(), radians.cos());
        let hw = quad.w / 2.0;
        let hh = quad.h / 2.0;
        // y-flip camera compensation (round 18): unit sprites are top-row-first.
        let corners = [
            (-hw, -hh, u0, v1),
            (hw, -hh, u1, v1),
            (hw, hh, u1, v0),
            (-hw, hh, u0, v0),
        ];
        for index in [0usize, 1, 2, 0, 2, 3] {
            let (lx, ly, u, v) = corners[index];
            let vx = quad.cx + lx * cos - ly * sin;
            let vy = quad.cy + lx * sin + ly * cos;
            tool.set_color(color);
            tool.set_uv(Vector2::new(u, v));
            tool.add_vertex(Vector3::new(vx, vy, 0.0));
        }
    }
    tool.commit()
}

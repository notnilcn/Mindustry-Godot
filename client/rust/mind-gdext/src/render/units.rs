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

use godot::classes::canvas_item_material::BlendMode;
use godot::classes::mesh::PrimitiveType;
use godot::classes::{
    ArrayMesh, CanvasItemMaterial, Mesh, MeshInstance2D, Node2D, SurfaceTool, Texture2D, Time,
};
use godot::obj::{NewGd, Singleton};
use godot::prelude::*;

use mind_core::content::registries::fx_meta::{EffectId, effect_by_name};
use mind_core::render::unit_draw::{self, UnitFxInput, UnitViewState, ViewEffect};

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
    /// Additive hit-flash nodes (`CanvasItemMaterial` BLEND_MODE_ADD), keyed
    /// `(bucket, page)`; parented to the same band, after the normal nodes.
    flash_nodes: HashMap<(usize, i32), Gd<MeshInstance2D>>,
    /// Shared additive material for every flash node.
    flash_material: Gd<CanvasItemMaterial>,
    /// Per-unit view state (hit flash from health drops; T5).
    view: HashMap<u64, UnitViewState>,
    /// `Fx.unitLand` id (resolved once; H03 T10).
    unit_land: EffectId,
    /// `Fx.ripple` id (`Floor.walkEffect` on liquid floors; H03 T10).
    ripple: EffectId,
    /// Tick clock of the last `update` (delta ticks for the view lerps).
    last_tick: f32,
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
        let mut flash_material = CanvasItemMaterial::new_gd();
        flash_material.set_blend_mode(BlendMode::ADD);
        Self {
            host,
            resolver,
            sprite_scale,
            bands,
            nodes: HashMap::new(),
            flash_nodes: HashMap::new(),
            flash_material,
            view: HashMap::new(),
            unit_land: effect_by_name("unitLand").unwrap_or(EffectId::NONE),
            ripple: effect_by_name("ripple").unwrap_or(EffectId::NONE),
            last_tick: 0.0,
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
    /// `drawMech` legs, outline, body, `drawCell`, `drawWeapons`. Returns the
    /// view-side effect spawns (H03 T10: status particles, landing dust, walk
    /// ripples) for the caller to forward to `MindFx`.
    pub fn update(&mut self) -> Vec<ViewEffect> {
        let draws = self.host.clone().bind().unit_draws_full();
        let ticks = Time::singleton().get_ticks_msec() as f32 * 0.06;
        let delta_ticks = (ticks - self.last_tick).max(0.0);
        self.last_tick = ticks;
        let time = ticks / 60.0;
        let mut effects: Vec<ViewEffect> = Vec::new();
        let mut groups: BTreeMap<(usize, i32), UnitBucket> = BTreeMap::new();
        let mut flash_groups: BTreeMap<(usize, i32), UnitBucket> = BTreeMap::new();
        let mut live_units = HashSet::new();
        let mut sprites = 0i64;
        for unit in draws {
            // `HealthComp` hit flash (`hitTime = 1` on a health drop, decay
            // `delta / 9`; view-derived, the sim keeps no `hitTime`).
            let view = self.view.entry(unit.id).or_default();
            unit_draw::advance_view(view, unit.health, unit.item.is_some(), delta_ticks);
            let hit_time = view.hit_time;
            // `StatusEffect.update` + `UnitComp.update` view half (H03 T10):
            // status particles, the landing dust and walk ripples.
            let fx_input = UnitFxInput {
                x: unit.x,
                y: unit.y,
                elevation: unit.elevation,
                hit_size: unit.hit_size,
                hovering: unit.hovering,
                emit_walk_effect: unit.emit_walk_effect,
                floor_is_liquid: unit.floor_is_liquid,
                floor_color: unit.floor_color,
                unit_land: self.unit_land,
                ripple: self.ripple,
            };
            let mut rand = || godot::global::randf() as f32;
            unit_draw::advance_unit_fx(
                view,
                &fx_input,
                &unit.statuses,
                delta_ticks,
                &mut rand,
                &mut effects,
            );
            live_units.insert(unit.id);
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
            // `UnitType.drawShadow` (`UnitType.java:1494-1497,1652-1664`):
            // flying units (and any unit with a positive `shadowElevation`)
            // draw a `Pal.shadow`-tinted silhouette of their full icon offset
            // down-left, at `min(Layer.darkness, z - 1)` = Layer.darkness.
            if unit.flying || unit.elevation > 0.0 {
                let shadow = self
                    .resolver
                    .find(&format!("unit-{}-full", unit.name))
                    .or_else(|| self.resolver.find(&unit.name));
                if let Some(shadow) = shadow {
                    let (ox, oy) =
                        mind_core::render::unit_draw::flying_shadow_offset(unit.elevation);
                    let entry = groups
                        .entry((3, shadow.page))
                        .or_insert((shadow.texture.clone(), Vec::new()));
                    entry.1.push(UnitQuad {
                        cx: unit.x + ox,
                        cy: unit.y + oy,
                        w: shadow.w * self.sprite_scale,
                        h: shadow.h * self.sprite_scale,
                        rot: unit.rotation - 90.0,
                        uv: shadow.uv,
                        color: 0x0000_0038,
                    });
                    sprites += 1;
                }
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
            // `drawTank` (`UnitType.java:1806-1824`): the full `-treads` base
            // at `rotation - 90`, then the animated `-treads<i>-<frame>` slices
            // per `treadRects` for both sides (`Mathf.signs`), offset by
            // `-(rect.x + w/2)`/`-(rect.y + h/2)` image px (÷4 → world units).
            if let Some(treads) = &unit.treads
                && let Some(base_tread) = self.resolver.find(&format!("{}-treads", unit.name))
            {
                let entry = groups
                    .entry((bucket, base_tread.page))
                    .or_insert((base_tread.texture.clone(), Vec::new()));
                entry.1.push(UnitQuad {
                    cx: unit.x,
                    cy: unit.y,
                    w: base_tread.w * self.sprite_scale,
                    h: base_tread.h * self.sprite_scale,
                    rot: unit.rotation - 90.0,
                    uv: base_tread.uv,
                    color: 0xffff_ffff,
                });
                sprites += 1;
                let frame = (treads.tread_time as i32).rem_euclid(treads.frames.max(1));
                for (index, rect) in treads.rects.iter().enumerate() {
                    let Some(region) = self
                        .resolver
                        .find(&format!("{}-treads{}-{}", unit.name, index, frame))
                    else {
                        continue;
                    };
                    let x_offset = -(rect.x + rect.width / 2.0);
                    let y_offset = -(rect.y + rect.height / 2.0);
                    for side in [-1.0f32, 1.0] {
                        let angle = (unit.rotation - 90.0).to_radians();
                        let (sin, cos) = angle.sin_cos();
                        let (vx, vy) = (
                            (x_offset * side) * cos - y_offset * sin,
                            (x_offset * side) * sin + y_offset * cos,
                        );
                        let entry = groups
                            .entry((bucket, region.page))
                            .or_insert((region.texture.clone(), Vec::new()));
                        entry.1.push(UnitQuad {
                            cx: unit.x + vx / 4.0,
                            cy: unit.y + vy / 4.0,
                            w: rect.width / 4.0,
                            h: region.h * self.sprite_scale,
                            rot: unit.rotation - 90.0,
                            uv: region.uv,
                            color: 0xffff_ffff,
                        });
                        sprites += 1;
                    }
                }
            }
            // `drawLegs` (`UnitType.java:1826-1904`): the plan-17 leg op list
            // (front-first feet/joints, upper+lower leg lines, base joints,
            // base) emitted at `z - 0.02`, under the soft shadow and body.
            if let Some(legs_draw) = &unit.legs {
                let foot = self.resolver.find(&format!("{}-foot", unit.name));
                let leg_region = self.resolver.find(&format!("{}-leg", unit.name));
                let leg_base = self
                    .resolver
                    .find(&format!("{}-leg-base", unit.name))
                    .or_else(|| leg_region.clone());
                let joint = self.resolver.find(&format!("{}-joint", unit.name));
                let base_joint = self.resolver.find(&format!("{}-joint-base", unit.name));
                let base = self.resolver.find(&format!("{}-base", unit.name));
                let found = mind_core::render::unit_draw::LegRegions {
                    foot: foot.is_some(),
                    leg: leg_region.is_some(),
                    leg_base: leg_base.is_some(),
                    joint: joint.is_some(),
                    base_joint: base_joint.is_some(),
                    base: base.is_some(),
                };
                let params = mind_core::render::unit_draw::LegsDrawParams {
                    x: unit.x,
                    y: unit.y,
                    base_rotation: legs_draw.base_rotation,
                    inv_drown: legs_draw.inv_drown,
                    legs: legs_draw.legs.clone(),
                    leg_base_offset: legs_draw.leg_base_offset,
                    leg_straightness: legs_draw.leg_straightness,
                    base_leg_straightness: legs_draw.base_leg_straightness,
                    leg_straight_length: legs_draw.leg_straight_length,
                    leg_extension: legs_draw.leg_extension,
                    leg_base_under: legs_draw.leg_base_under,
                    shadow_elevation: legs_draw.shadow_elevation,
                    shadow_tx: -12.0,
                    shadow_ty: -13.0,
                    sprite_scale: self.sprite_scale,
                    foot_w: foot.as_ref().map(|r| r.w).unwrap_or(0.0),
                    leg_h: leg_region.as_ref().map(|r| r.h).unwrap_or(0.0),
                    leg_base_h: leg_base.as_ref().map(|r| r.h).unwrap_or(0.0),
                    found,
                };
                for op in mind_core::render::unit_draw::leg_draw_ops(&params) {
                    use mind_core::render::unit_draw::{LegOp, LegSprite};
                    let (region, quad) = match &op {
                        LegOp::FootShadow {
                            x,
                            y,
                            radius,
                            alpha,
                        } => {
                            let Some(region) = self.resolver.find("circle-shadow") else {
                                continue;
                            };
                            (
                                region.clone(),
                                UnitQuad {
                                    cx: *x,
                                    cy: *y,
                                    w: *radius,
                                    h: *radius,
                                    rot: 0.0,
                                    uv: region.uv,
                                    // `Pal.shadow` color at `0.4 * invDrown`.
                                    color: (0.4 * alpha * 255.0) as u32,
                                },
                            )
                        }
                        LegOp::Rect { sprite, x, y, rot } => {
                            let region = match sprite {
                                LegSprite::Foot => foot.clone(),
                                LegSprite::Leg => leg_region.clone(),
                                LegSprite::LegBase => leg_base.clone(),
                                LegSprite::Joint => joint.clone(),
                                LegSprite::BaseJoint => base_joint.clone(),
                                LegSprite::Base => base.clone(),
                            };
                            let Some(region) = region else {
                                continue;
                            };
                            (
                                region.clone(),
                                UnitQuad {
                                    cx: *x,
                                    cy: *y,
                                    w: region.w * self.sprite_scale,
                                    h: region.h * self.sprite_scale,
                                    rot: *rot,
                                    uv: region.uv,
                                    color: 0xffff_ffff,
                                },
                            )
                        }
                        LegOp::Line {
                            sprite,
                            x1,
                            y1,
                            x2,
                            y2,
                            stroke,
                        } => {
                            let region = match sprite {
                                LegSprite::Leg => leg_region.clone(),
                                LegSprite::LegBase => leg_base.clone(),
                                _ => None,
                            };
                            let Some(region) = region else {
                                continue;
                            };
                            let len = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
                            if len <= 1e-6 {
                                continue;
                            }
                            // `Lines.line` maps the region's width along the
                            // segment and its height across it; a negative
                            // stroke mirrors the sprite (`Lines.java:108-131`).
                            let [u0, v0, u1, v1] = region.uv;
                            let uv = if *stroke < 0.0 {
                                [u0, v1, u1, v0]
                            } else {
                                [u0, v0, u1, v1]
                            };
                            (
                                region.clone(),
                                UnitQuad {
                                    cx: (x1 + x2) / 2.0,
                                    cy: (y1 + y2) / 2.0,
                                    w: len,
                                    h: stroke.abs(),
                                    rot: (y2 - y1).atan2(x2 - x1).to_degrees(),
                                    uv,
                                    color: 0xffff_ffff,
                                },
                            )
                        }
                        LegOp::Base { x, y, rot } => {
                            let Some(region) = base.clone() else {
                                continue;
                            };
                            (
                                region.clone(),
                                UnitQuad {
                                    cx: *x,
                                    cy: *y,
                                    w: region.w * self.sprite_scale,
                                    h: region.h * self.sprite_scale,
                                    rot: *rot,
                                    uv: region.uv,
                                    color: 0xffff_ffff,
                                },
                            )
                        }
                    };
                    let entry = groups
                        .entry((bucket, region.page))
                        .or_insert((region.texture.clone(), Vec::new()));
                    entry.1.push(quad);
                    sprites += 1;
                }
            }
            // `drawSoftShadow`: `max(region.w, region.h) * region.scl() *
            // softShadowScl * 1.6`, colour `(0,0,0,0.4)`. Java draws it at
            // `z - 0.01`, *over* the legs (`z - 0.02`) and under the body.
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
            // `drawOutline`/`drawWeaponOutlines` (`UnitType.java:1537-1539`):
            // the unit's own `<name>-outline` (present for modded units with
            // `alwaysCreateOutline`; vanilla bodies bake the outline into the
            // region in place) and each non-top weapon's `<weapon>-outline`
            // sprite (`Weapon.drawOutline`, same mount transform as the shot).
            if let Some(outline) = self.resolver.find(&format!("{}-outline", unit.name)) {
                let entry = groups
                    .entry((bucket, outline.page))
                    .or_insert((outline.texture.clone(), Vec::new()));
                entry.1.push(UnitQuad {
                    cx: unit.x,
                    cy: unit.y,
                    w: outline.w * self.sprite_scale,
                    h: outline.h * self.sprite_scale,
                    rot: unit.rotation - 90.0,
                    uv: outline.uv,
                    color: 0xffff_ffff,
                });
                sprites += 1;
            }
            for weapon in &unit.weapons {
                if weapon.top {
                    continue;
                }
                let Some(outline) = self.resolver.find(&format!("{}-outline", weapon.name)) else {
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
                let [u0, v0, u1, v1] = outline.uv;
                let uv = if weapon.flip {
                    [u1, v0, u0, v1]
                } else {
                    [u0, v0, u1, v1]
                };
                let entry = groups
                    .entry((bucket, outline.page))
                    .or_insert((outline.texture.clone(), Vec::new()));
                entry.1.push(UnitQuad {
                    cx: wx,
                    cy: wy,
                    w: outline.w * self.sprite_scale,
                    h: outline.h * self.sprite_scale,
                    rot: wr,
                    uv,
                    color: 0xffff_ffff,
                });
                sprites += 1;
            }
            // `drawBody` (`rotation - 90`; the port draws trimmed sizes, which
            // is what Arc's `Draw.rect(region, x, y)` does for centered trims).
            if unit.draw_body {
                let quad = UnitQuad {
                    cx: unit.x,
                    cy: unit.y,
                    w: body.w * self.sprite_scale,
                    h: body.h * self.sprite_scale,
                    rot: unit.rotation - 90.0,
                    uv: body.uv,
                    color: 0xffff_ffff,
                };
                let entry = groups
                    .entry((bucket, body.page))
                    .or_insert((body.texture.clone(), Vec::new()));
                entry.1.push(quad);
                sprites += 1;
                // `UnitType.applyColor`: `Draw.mixcol(white, hitTime)` — the
                // port draws the flash as an additive duplicate body quad.
                if hit_time > 0.01 {
                    let alpha = (hit_time.clamp(0.0, 1.0) * 255.0) as u32;
                    let entry = flash_groups
                        .entry((bucket, body.page))
                        .or_insert((body.texture.clone(), Vec::new()));
                    entry.1.push(UnitQuad {
                        color: 0xffff_ff00 | alpha,
                        ..quad
                    });
                }
            }
            // `drawCell`: `<name>-cell` else `power-cell`, tinted by
            // `UnitType.cellColor`.
            let cell = self
                .resolver
                .find(&format!("{}-cell", unit.name))
                .or_else(|| self.resolver.find("power-cell"));
            if unit.draw_cell
                && let Some(cell) = cell
            {
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
                // `Weapon.draw` (`Weapon.java:261-265`): `<weapon>-cell` tinted
                // by `UnitType.cellColor`, same mount transform as the region.
                if let Some(cell) = self.resolver.find(&format!("{}-cell", weapon.name)) {
                    let team = mind_core::game::team::Team::get(unit.team)
                        .color
                        .to_rgba8888();
                    let [c0, d0, c1, d1] = cell.uv;
                    let uv = if weapon.flip {
                        [c1, d0, c0, d1]
                    } else {
                        [c0, d0, c1, d1]
                    };
                    let entry = groups
                        .entry((bucket, cell.page))
                        .or_insert((cell.texture.clone(), Vec::new()));
                    entry.1.push(UnitQuad {
                        cx: wx,
                        cy: wy,
                        w: cell.w * self.sprite_scale,
                        h: cell.h * self.sprite_scale,
                        rot: wr,
                        uv,
                        color: mind_core::render::cell_color(team, unit.healthf, time),
                    });
                    sprites += 1;
                }
            }
        }

        self.view.retain(|id, _| live_units.contains(id));

        let live: HashSet<(usize, i32)> = groups.keys().copied().collect();
        for (key, node) in self.nodes.iter_mut() {
            if !live.contains(key) {
                node.set_visible(false);
            }
        }
        let live_flash: HashSet<(usize, i32)> = flash_groups.keys().copied().collect();
        for (key, node) in self.flash_nodes.iter_mut() {
            if !live_flash.contains(key) {
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

        for (key, (texture, quads)) in flash_groups {
            let Some(mesh) = build_unit_mesh(&quads) else {
                continue;
            };
            let node = if let Some(node) = self.flash_nodes.get(&key) {
                node.clone()
            } else {
                let Some(mut band) = self.bands.get(key.0).and_then(|band| band.clone()) else {
                    continue;
                };
                let mut node = MeshInstance2D::new_alloc();
                node.set_material(
                    &self
                        .flash_material
                        .clone()
                        .upcast::<godot::classes::Material>(),
                );
                // code-instantiated: one pooled additive node per (unit layer,
                // atlas page); the count is page-driven (G03 T5).
                band.add_child(&node);
                self.flash_nodes.insert(key, node.clone());
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
        effects
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
    // I02 T2: an empty slice gets no mesh (see `atlas_bind::has_mesh_quads`).
    if !super::atlas_bind::has_mesh_quads(quads) {
        return None;
    }
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

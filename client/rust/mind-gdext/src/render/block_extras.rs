// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live `DrawBlock` chains for crafter/generator animations (H03 T8).
//!
//! `core/render/draw_desc.rs` ports each vanilla `world/draw/*` body. The base
//! block sprite is drawn by `blocks.rs` (dynamic) or the building cache; this
//! pass runs `execute_extras` for every visible building whose
//! `vanilla_chain(name)` is non-empty and interprets the emitted primitives
//! (`DrawFlame` circles, `DrawRegion -rotator` spins, `DrawPistons`, heat
//! regions, liquid tops, ...). Two nodes share the work: one for
//! `Blend::Normal` and one under a `CanvasItemMaterial` `BLEND_MODE_ADD` for
//! `Blend::Additive` commands.

use std::collections::HashSet;

use godot::builtin::PackedVector2Array;
use godot::classes::canvas_item_material::BlendMode;
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{CanvasItemMaterial, INode2D, Node2D, Texture2D};
use godot::obj::{Base, NewGd};
use godot::prelude::*;

use mind_core::render::commands::{Blend, DrawCmd, FillKind, LineKind, ShapeKind};
use mind_core::render::draw_desc::{
    DescDraw, execute_extras, split_chain_at_default, vanilla_chain,
};
use mind_core::render::drawf;
use mind_core::render::ids::RegionIdTable;
use mind_core::render::scan::{CameraView, visible_blocks};

use crate::sim_host::MindSimHost;

use super::canvas::{self, RegionCache};

/// One interpreted `DrawBlock` primitive, ready for `draw()`.
enum Cmd {
    Region {
        texture: Gd<Texture2D>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rot: f32,
        color: Color,
    },
    Circle {
        x: f32,
        y: f32,
        r: f32,
        color: Color,
    },
    PolyFill {
        x: f32,
        y: f32,
        sides: i32,
        r: f32,
        rot: f32,
        color: Color,
    },
    CircleOutline {
        x: f32,
        y: f32,
        r: f32,
        stroke: f32,
        color: Color,
    },
    PolyLine {
        x: f32,
        y: f32,
        sides: i32,
        r: f32,
        rot: f32,
        stroke: f32,
        color: Color,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        stroke: f32,
        color: Color,
    },
}

/// `DrawBlock` extras pass (`Node2D` under the `Layer.block` band, above the
/// base block meshes).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct BlockExtras {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    node: Option<Gd<Node2D>>,
    regions: RegionCache,
    ids: RegionIdTable,
    /// `true` on the additive-blend node (`BLEND_MODE_ADD`); it handles only
    /// `Blend::Additive` commands, the normal node every other blend.
    pub(crate) additive: bool,
    /// `true` on the under-sprite node (`z_index = -1`): it runs only the chain
    /// parts *before* `DrawDefault` (`-bottom`, `DrawLiquidTile`; I02 T7).
    pub(crate) under: bool,
}

#[godot_api]
impl INode2D for BlockExtras {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            node: None,
            regions: RegionCache::new(),
            ids: RegionIdTable::new(),
            additive: false,
            under: false,
        }
    }

    fn ready(&mut self) {
        if self.additive {
            let mut material = CanvasItemMaterial::new_gd();
            material.set_blend_mode(BlendMode::ADD);
            self.base_mut().set_material(&material);
        }
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.host.is_some() {
            // Warmup/progress/time animate every frame.
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        // Collect every input first: `base_mut()` borrows `self` for the whole
        // draw, so no `self.*` read may follow it.
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(node) = self.node.clone() else {
            return;
        };
        let Some(rect) = self.visible_rect() else {
            return;
        };
        let add = self.additive;
        let mut cmds: Vec<Cmd> = Vec::new();
        {
            let host = host.bind();
            let Some(content) = host.content_registry() else {
                return;
            };
            let world = host.grid();
            let view = CameraView {
                x: rect.0 + rect.2 / 2.0,
                y: rect.1 + rect.3 / 2.0,
                w: rect.2 + 32.0,
                h: rect.3 + 32.0,
                zoom: 1.0,
                team: 0,
            };
            let mut seen = HashSet::new();
            for tile in visible_blocks(world, content, &view) {
                let Some(def) = content.block(tile.block) else {
                    continue;
                };
                // Multiblock: draw once per center entity (as `blocks.rs` does).
                if let Some(entity) = world.tile(tile.x, tile.y).build
                    && !seen.insert(entity)
                {
                    continue;
                }
                let Some(chain) = vanilla_chain(&def.name) else {
                    continue;
                };
                let Some(state) = host.block_draw_state(tile.x, tile.y) else {
                    continue;
                };
                // `DrawMulti` order (`DrawMulti.java`): parts before
                // `DrawDefault` (`-bottom`, `DrawLiquidTile`) draw *under* the
                // base sprite, the rest over it (I02 T7).
                let (under_chain, over_chain) = split_chain_at_default(&chain);
                let bucket_src = if self.under {
                    &under_chain
                } else {
                    &over_chain
                };
                let mut descs: Vec<DescDraw> = Vec::new();
                for desc in bucket_src {
                    execute_extras(desc, &def.name, &state, &mut self.ids, &mut descs);
                }
                let (normal, additive) = bucket_by_blend(descs);
                let bucket = if add { additive } else { normal };
                for desc in bucket {
                    if let Some(cmd) = interpret(&desc, &self.ids, &node, &mut self.regions) {
                        cmds.push(cmd);
                    }
                }
            }
        }
        let mut base = self.base_mut();
        for cmd in cmds {
            match cmd {
                Cmd::Region {
                    texture,
                    x,
                    y,
                    w,
                    h,
                    rot,
                    color,
                } => canvas::draw_region(&mut base, &texture, x, y, w, h, rot, color),
                Cmd::Circle { x, y, r, color } => canvas::fill_circle(&mut base, x, y, r, color),
                Cmd::PolyFill {
                    x,
                    y,
                    sides,
                    r,
                    rot,
                    color,
                } => {
                    let ring = drawf::poly_ring(x, y, sides.max(3) as u32, r, rot);
                    let packed: PackedVector2Array = ring
                        .iter()
                        .map(|(px, py)| Vector2::new(*px, *py))
                        .collect::<Vec<_>>()
                        .into_iter()
                        .collect();
                    base.draw_colored_polygon(&packed, color);
                }
                Cmd::CircleOutline {
                    x,
                    y,
                    r,
                    stroke,
                    color,
                } => canvas::circle_stroke(&mut base, x, y, r, stroke, color),
                Cmd::PolyLine {
                    x,
                    y,
                    sides,
                    r,
                    rot,
                    stroke,
                    color,
                } => {
                    let mut ring = drawf::poly_ring(x, y, sides.max(3) as u32, r, rot);
                    if let Some(first) = ring.first().copied() {
                        ring.push(first);
                    }
                    canvas::poly_line(&mut base, &ring, stroke, color);
                }
                Cmd::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    color,
                } => {
                    base.draw_line_ex(Vector2::new(x1, y1), Vector2::new(x2, y2), color)
                        .width(stroke.max(1.0))
                        .done();
                }
            }
        }
    }
}

impl BlockExtras {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.node = Some(self.to_gd().upcast::<Node2D>());
        self.base_mut().queue_redraw();
    }

    /// World-space visible rect `(min x, min y, w, h)` from the camera
    /// transform.
    fn visible_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let camera = self
            .base()
            .try_get_node_as::<godot::classes::Camera2D>("/root/Spine/World/Camera2D")?;
        let viewport = self.base().get_viewport()?;
        let rect = viewport.get_visible_rect();
        let inverse = camera.get_canvas_transform().affine_inverse();
        let corners = [
            inverse * rect.position,
            inverse * (rect.position + Vector2::new(rect.size.x, 0.0)),
            inverse * rect.position + rect.size,
            inverse * (rect.position + Vector2::new(0.0, rect.size.y)),
        ];
        let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let min_y = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_x = corners
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = corners
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        Some((min_x, min_y, max_x - min_x, max_y - min_y))
    }
}

/// Splits a chain's draws into `(normal, additive)` buckets, each stable-sorted
/// by z (`RenderQueue` ordering; `BlockExtras` keeps one node per blend).
pub(crate) fn bucket_by_blend(descs: Vec<DescDraw>) -> (Vec<DescDraw>, Vec<DescDraw>) {
    let mut normal = Vec::new();
    let mut additive = Vec::new();
    for desc in descs {
        if desc.blend == Blend::Additive {
            additive.push(desc);
        } else {
            normal.push(desc);
        }
    }
    normal.sort_by(|a, b| a.z.total_cmp(&b.z));
    additive.sort_by(|a, b| a.z.total_cmp(&b.z));
    (normal, additive)
}

/// Interprets one `DescDraw` into a canvas command (textures resolved through
/// the shared region cache). `None` for unknown regions/commands.
fn interpret(
    desc: &DescDraw,
    ids: &RegionIdTable,
    node: &Gd<Node2D>,
    regions: &mut RegionCache,
) -> Option<Cmd> {
    let color = |rgba: u32| canvas::color_u32(rgba);
    match &desc.cmd {
        DrawCmd::Sprite {
            region,
            x,
            y,
            w,
            h,
            rot,
            color: rgba,
            ..
        } => {
            let name = ids.name(*region)?;
            let texture = regions.get(node, name)?;
            Some(Cmd::Region {
                texture,
                x: *x,
                y: *y,
                w: *w,
                h: *h,
                rot: *rot,
                color: color(*rgba),
            })
        }
        DrawCmd::Fill {
            kind,
            params,
            color: rgba,
        } => match kind {
            FillKind::Circle => Some(Cmd::Circle {
                x: params[0],
                y: params[1],
                r: params[2],
                color: color(*rgba),
            }),
            FillKind::Poly => Some(Cmd::PolyFill {
                x: params[0],
                y: params[1],
                sides: params[2] as i32,
                r: params[3],
                rot: params[4],
                color: color(*rgba),
            }),
            // Not emitted by any ported `DrawBlock` chain yet.
            FillKind::Rect | FillKind::Triangle => None,
        },
        DrawCmd::Shape {
            kind,
            params,
            color: rgba,
        } => match kind {
            ShapeKind::CircleOutline => Some(Cmd::CircleOutline {
                x: params[0],
                y: params[1],
                r: params[2],
                stroke: params[3],
                color: color(*rgba),
            }),
            ShapeKind::Poly => Some(Cmd::PolyLine {
                x: params[0],
                y: params[1],
                sides: params[2] as i32,
                r: params[3],
                rot: params[4],
                stroke: params[5],
                color: color(*rgba),
            }),
            _ => None,
        },
        DrawCmd::Lines {
            kind: LineKind::Line,
            params,
            color: rgba,
            stroke,
        } => Some(Cmd::Line {
            x1: params[0],
            y1: params[1],
            x2: params[2],
            y2: params[3],
            stroke: *stroke,
            color: color(*rgba),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::render::draw_desc::DescDraw;
    use mind_core::render::ids::RegionIdTable;
    use mind_core::render::layer::Layer;

    fn circle(z: f32, blend: Blend) -> DescDraw {
        DescDraw {
            z,
            blend,
            cmd: DrawCmd::Fill {
                kind: FillKind::Circle,
                params: [0.0, 0.0, 4.0, 0.0, 0.0, 0.0],
                color: 0xffff_ffff,
            },
        }
    }

    /// H03 T8: a chain's draws split by blend into two z-sorted buckets
    /// (one node each).
    #[test]
    fn bucket_split_is_by_blend_and_z() {
        let mut ids = RegionIdTable::new();
        let region = ids.intern("white");
        let sprite = DescDraw {
            z: Layer::Block.z() + 0.01,
            blend: Blend::Additive,
            cmd: DrawCmd::Sprite {
                region,
                x: 0.0,
                y: 0.0,
                w: 8.0,
                h: 8.0,
                ox: 0.5,
                oy: 0.5,
                rot: 0.0,
                color: 0xffff_ffff,
                blend: Blend::Additive,
            },
        };
        let (normal, additive) = bucket_by_blend(vec![
            sprite,
            circle(Layer::Block.z(), Blend::Normal),
            circle(Layer::Block.z() - 1.0, Blend::Additive),
        ]);
        assert_eq!(normal.len(), 1);
        assert_eq!(additive.len(), 2);
        // Stable z order inside the additive bucket.
        assert!(additive[0].z < additive[1].z);
        assert_eq!(normal[0].blend, Blend::Normal);
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit factory / reconstructor build preview (H03 T9).
//!
//! `UnitFactory.draw` (`UnitFactory.java:380-397`) and `Reconstructor.draw`
//! (`Reconstructor.java:287-317`) draw the unit being built through
//! `Drawf.construct` (`Drawf.java:586-603`): the unit's `fullIcon` through
//! `Shaders.build` (`client/shaders/unitbuild.gdshader`) with the scan-stripe
//! progress, then a sweeping accent line, then the factory's `-top` region at
//! `Layer.blockOver + 0.1`. The reconstructor additionally draws the incoming
//! payload icon at `1 - progress/constructTime` before the preview.
//!
//! The base/`-in`/`-out` sprites stay in `blocks.rs` (the `Layer.block` band);
//! this pass owns the `Layer.blockOver` band node and derives everything from
//! the `MindSimHost::unit_build_draw` rows, with per-factory view state
//! (`time`, `speedScl`) mirrored from Java `updateTile`.

use std::collections::{HashMap, HashSet};

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Material, Mesh, MeshInstance2D, Node2D, Shader, ShaderMaterial};
use godot::obj::{Base, NewGd};
use godot::prelude::*;

use mind_core::content::BlockKind;
use mind_core::render::scan::{CameraView, visible_blocks};

use crate::sim_host::MindSimHost;

use super::atlas_bind::{RegionResolver, ResolvedRegion};
use super::units::{UnitQuad, build_unit_mesh};

/// `Pal.accent` (`Drawf.construct` line/preview tint).
const ACCENT: u32 = 0xffd3_7fff;
/// Java `Lines.getStroke()` default 1f (the construct sweep line).
const LINE_STROKE: f32 = 1.0;
/// View-animation dt clamp (ticks): a long pause/resume must not jump.
const MAX_DT: i64 = 30;

/// Per-building view animation state (`UnitFactoryBuild.time/speedScl`,
/// view-only; H03 T9).
#[derive(Default)]
struct BuildView {
    /// `UnitFactoryBuild.time` (only the construct sweep uses it).
    time: f32,
    /// `UnitFactoryBuild.speedScl` (`lerpDelta(.., valid ? 1 : 0, 0.05)`).
    speed_scl: f32,
}

/// One visible factory/reconstructor row collected under the host bind.
struct BuildRow {
    /// Entity index (view-state key; Java uses the entity id for seeding).
    id: i32,
    cx: f32,
    cy: f32,
    size: i32,
    rotation: u8,
    /// Content name (`<name>-top` candidates).
    name: String,
    /// `unit-<name>-full` of the unit being constructed.
    unit_region: Option<String>,
    /// `unit-<name>-full` of the reconstructor payload.
    payload_region: Option<String>,
    fraction: f32,
    working: bool,
}

/// Unit build-preview pass (`Node2D` under the `Layer.blockOver` band).
///
/// One construct node (through the `unitbuild` shader), one payload icon, one
/// accent-line quad and one top-region quad per visible factory/reconstructor,
/// pooled and hidden when unused.
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct UnitBuildRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    shader: Option<Gd<Shader>>,
    resolver: RegionResolver,
    materials: Vec<Gd<ShaderMaterial>>,
    nodes: Vec<Gd<MeshInstance2D>>,
    payload_nodes: Vec<Gd<MeshInstance2D>>,
    line_nodes: Vec<Gd<MeshInstance2D>>,
    top_nodes: Vec<Gd<MeshInstance2D>>,
    view: HashMap<i32, BuildView>,
    /// Sim tick of the previous `draw` (view dt; 0 = first frame).
    last_tick: i64,
}

#[godot_api]
impl INode2D for UnitBuildRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            shader: None,
            resolver: RegionResolver::new(None),
            materials: Vec::new(),
            nodes: Vec::new(),
            payload_nodes: Vec::new(),
            line_nodes: Vec::new(),
            top_nodes: Vec::new(),
            view: HashMap::new(),
            last_tick: 0,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.host.is_some() {
            // Build the row meshes every frame (`speedScl`/`time` animate and
            // factory progress moves without a world-revision bump). All
            // visuals are child `MeshInstance2D`s, so no `draw()` is needed.
            self.update();
        }
    }
}

impl UnitBuildRenderer {
    /// Collects the visible factory rows and drives the pooled child nodes.
    fn update(&mut self) {
        // Collect every input first: no `self.*` mutation may race a `bind`.
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(rect) = self.visible_rect() else {
            return;
        };
        let tick = host.bind().get_tick();
        let mut rows: Vec<BuildRow> = Vec::new();
        {
            let bound = host.bind();
            let Some(content) = bound.content_registry() else {
                return;
            };
            let world = bound.grid();
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
                if !matches!(def.kind, BlockKind::UnitFactory | BlockKind::Reconstructor) {
                    continue;
                }
                // Multiblock: one row per centre entity (as `blocks.rs` does).
                let Some(entity) = world.tile(tile.x, tile.y).build else {
                    continue;
                };
                if !seen.insert(entity) {
                    continue;
                }
                let Some(row) = bound.unit_build_draw(tile.x, tile.y) else {
                    continue;
                };
                let unit_region = row
                    .unit
                    .and_then(|id| content.unit(id))
                    .map(|def| format!("unit-{}-full", def.name));
                let payload_region = row
                    .payload
                    .and_then(|id| content.unit(id))
                    .map(|def| format!("unit-{}-full", def.name));
                rows.push(BuildRow {
                    id: entity.index_u32() as i32,
                    cx: row.cx,
                    cy: row.cy,
                    size: row.size,
                    rotation: row.rotation,
                    name: def.name.clone(),
                    unit_region,
                    payload_region,
                    fraction: row.fraction,
                    working: row.working,
                });
            }
        }

        // View animation in sim ticks (`lerpDelta(from, to, 0.05)`:
        // `1 - 0.95^delta`; `time += delta * speedScl`, view-only).
        let dt = if self.last_tick == 0 {
            0.0
        } else {
            (tick - self.last_tick).clamp(0, MAX_DT) as f32
        };
        self.last_tick = tick;
        // Drop view state for buildings that left the view/revision.
        let live: HashSet<i32> = rows.iter().map(|row| row.id).collect();
        self.view.retain(|id, _| live.contains(id));

        // Pool growth: four nodes per row (payload under construct, then the
        // sweep line, then the top region), created together so tree order is
        // per-factory. code-instantiated: count driven by the visible
        // factories/reconstructors.
        while self.nodes.len() < rows.len() {
            let mut payload = MeshInstance2D::new_alloc();
            payload.set_name("UnitBuildPayload");
            payload.set_visible(false);
            let mut construct = MeshInstance2D::new_alloc();
            construct.set_name("UnitBuildConstruct");
            construct.set_visible(false);
            let mut material = ShaderMaterial::new_gd();
            if let Some(shader) = self.shader.clone() {
                material.set_shader(&shader);
            }
            construct.set_material(&material.clone().upcast::<Material>());
            let mut line = MeshInstance2D::new_alloc();
            line.set_name("UnitBuildLine");
            line.set_visible(false);
            let mut top = MeshInstance2D::new_alloc();
            top.set_name("UnitBuildTop");
            top.set_visible(false);
            // `add_child` emits NOTIFICATION_CHILD_ORDER_CHANGED on self;
            // defer it so `on_notification` does not re-enter while the
            // `&mut self` borrow is held.
            for node in [&payload, &construct, &line, &top] {
                self.base_mut()
                    .call_deferred("add_child", &[node.to_variant()]);
            }
            self.payload_nodes.push(payload);
            self.nodes.push(construct);
            self.materials.push(material);
            self.line_nodes.push(line);
            self.top_nodes.push(top);
        }

        if rows.is_empty() {
            for node in self.nodes.iter_mut().chain(self.payload_nodes.iter_mut()) {
                node.set_visible(false);
            }
            for node in self.line_nodes.iter_mut().chain(self.top_nodes.iter_mut()) {
                node.set_visible(false);
            }
            return;
        }

        for (index, row) in rows.iter().enumerate() {
            let state = self.view.entry(row.id).or_default();
            let target = if row.working { 1.0 } else { 0.0 };
            state.speed_scl += (target - state.speed_scl) * (1.0 - 0.95f32.powf(dt));
            state.time += dt * state.speed_scl;
            let time = state.time;
            let speed = state.speed_scl;

            let construct_node = self.nodes[index].clone();
            let payload_node = self.payload_nodes[index].clone();
            let line_node = self.line_nodes[index].clone();
            let top_node = self.top_nodes[index].clone();

            // `Drawf.construct(.., rotdeg() - 90, progress/time, speedScl, time)`.
            let unit_region = row
                .unit_region
                .as_ref()
                .and_then(|name| self.resolver.find(name));
            match unit_region {
                Some(region) => {
                    let quad = region_quad(
                        &region,
                        row.cx,
                        row.cy,
                        f32::from(row.rotation) * 90.0 - 90.0,
                        color_alpha(ACCENT, speed),
                    );
                    match (build_unit_mesh(&[quad]), region.texture.clone()) {
                        (Some(mesh), Some(texture)) => {
                            let mut node = construct_node;
                            node.set_mesh(&mesh.upcast::<Mesh>());
                            node.set_visible(true);
                            let mut material = self.materials[index].clone();
                            let (page_w, page_h) = (
                                texture.get_width().max(1) as f32,
                                texture.get_height().max(1) as f32,
                            );
                            let [u0, v0, u1, v1] = region.uv;
                            material.set_shader_parameter("u_texture", &texture.to_variant());
                            material.set_shader_parameter(
                                "u_texsize",
                                &Vector2::new(page_w, page_h).to_variant(),
                            );
                            material
                                .set_shader_parameter("u_uv", &Vector2::new(u0, v0).to_variant());
                            material
                                .set_shader_parameter("u_uv2", &Vector2::new(u1, v1).to_variant());
                            material.set_shader_parameter(
                                "u_progress",
                                &row.fraction.clamp(0.0, 1.0).to_variant(),
                            );
                            // `Shaders.build.time = -time / 20f`.
                            material.set_shader_parameter("u_time", &(-time / 20.0).to_variant());
                            // `Shaders.build.color = Pal.accent` with alpha
                            // `speedScl` (`Drawf.java:589-592`).
                            material.set_shader_parameter(
                                "u_color",
                                &color_alpha(ACCENT, speed).to_variant(),
                            );
                        }
                        _ => construct_node.clone().set_visible(false),
                    }
                }
                None => construct_node.clone().set_visible(false),
            }

            // Reconstructor payload icon at `1 - progress/constructTime`
            // (`Reconstructor.draw`; rotation is not tracked by the port).
            let payload_region = row
                .payload_region
                .as_ref()
                .and_then(|name| self.resolver.find(name));
            match payload_region {
                Some(region) => {
                    let alpha = (1.0 - row.fraction).clamp(0.0, 1.0);
                    let quad = region_quad(
                        &region,
                        row.cx,
                        row.cy,
                        0.0,
                        color_alpha(0xffff_ffff, alpha),
                    );
                    match (build_unit_mesh(&[quad]), region.texture.clone()) {
                        (Some(mesh), Some(texture)) => {
                            let mut node = payload_node;
                            node.set_mesh(&mesh.upcast::<Mesh>());
                            node.set_texture(&texture);
                            node.set_visible(true);
                        }
                        _ => payload_node.clone().set_visible(false),
                    }
                }
                None => payload_node.clone().set_visible(false),
            }

            // `Draw.color(Pal.accent); Draw.alpha(speedScl); Lines.lineAngleCenter(
            // x + sin(time, 20, size/2), y, 90, size)` with
            // `size = block.size * tilesize - 4`.
            let line_size = row.size as f32 * 8.0 - 4.0;
            let line_x = mind_core::render::drawf::construct_line_x(row.cx, time, line_size);
            let quad = UnitQuad {
                cx: line_x,
                cy: row.cy,
                w: LINE_STROKE,
                h: line_size,
                rot: 0.0,
                uv: [0.0, 0.0, 1.0, 1.0],
                color: color_bits(color_alpha(ACCENT, speed)),
            };
            match build_unit_mesh(&[quad]) {
                Some(mesh) => {
                    let mut node = line_node;
                    node.set_mesh(&mesh.upcast::<godot::classes::Mesh>());
                    node.set_visible(true);
                }
                None => line_node.clone().set_visible(false),
            }

            // `Draw.rect(topRegion, x, y)` at `Layer.blockOver + 0.1`
            // (`UnitFactory.java:395-396`); `<name>-top` else `factory-top-<size>`.
            let top_region = self
                .resolver
                .find(&format!("{}-top", row.name))
                .or_else(|| self.resolver.find(&format!("factory-top-{}", row.size)));
            match top_region {
                Some(region) => {
                    let quad = region_quad(&region, row.cx, row.cy, 0.0, Color::WHITE);
                    match (build_unit_mesh(&[quad]), region.texture.clone()) {
                        (Some(mesh), Some(texture)) => {
                            let mut node = top_node;
                            node.set_mesh(&mesh.upcast::<godot::classes::Mesh>());
                            node.set_texture(&texture);
                            node.set_visible(true);
                        }
                        _ => top_node.clone().set_visible(false),
                    }
                }
                None => top_node.clone().set_visible(false),
            }
        }

        // Hide pooled spares.
        for node in self.nodes.iter_mut().skip(rows.len()) {
            node.set_visible(false);
        }
        for node in self.payload_nodes.iter_mut().skip(rows.len()) {
            node.set_visible(false);
        }
        for node in self.line_nodes.iter_mut().skip(rows.len()) {
            node.set_visible(false);
        }
        for node in self.top_nodes.iter_mut().skip(rows.len()) {
            node.set_visible(false);
        }
    }
}

impl UnitBuildRenderer {
    /// Installs the `unitbuild` shader (called from
    /// `MindWorldRenderer::build_floor`; a hot reload recreates the node).
    pub(crate) fn set_shader(&mut self, shader: Option<Gd<Shader>>) {
        self.shader = shader;
    }

    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.resolver = RegionResolver::new(
            self.base()
                .try_get_node_as::<crate::assets::MindAssets>("/root/MindAssets"),
        );
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

/// A natural-size quad for a resolved region (`Draw.rect(region, x, y)`:
/// `pixels * 0.25` world units).
fn region_quad(region: &ResolvedRegion, cx: f32, cy: f32, rot: f32, color: Color) -> UnitQuad {
    UnitQuad {
        cx,
        cy,
        w: region.w * 0.25,
        h: region.h * 0.25,
        rot,
        uv: region.uv,
        color: color_bits(color),
    }
}

/// `Color` -> RGBA8888 (`UnitQuad.color`).
fn color_bits(color: Color) -> u32 {
    ((color.r * 255.0).round() as u32) << 24
        | ((color.g * 255.0).round() as u32) << 16
        | ((color.b * 255.0).round() as u32) << 8
        | (color.a * 255.0).round() as u32
}

/// `Pal` RGBA8888 with a multiplied alpha.
fn color_alpha(rgba: u32, alpha: f32) -> Color {
    Color::from_rgba8(
        ((rgba >> 24) & 0xff) as u8,
        ((rgba >> 16) & 0xff) as u8,
        ((rgba >> 8) & 0xff) as u8,
        ((rgba & 0xff) as f32 * alpha.clamp(0.0, 1.0)) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::{color_alpha, color_bits};

    #[test]
    fn color_bits_round_trips_rgba8888() {
        assert_eq!(color_bits(color_alpha(0xffd3_7fff, 1.0)), 0xffd3_7fff);
    }

    #[test]
    fn color_alpha_scales_the_alpha_channel() {
        assert_eq!(color_bits(color_alpha(0xffd3_7fff, 0.5)), 0xffd3_7f7f);
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BlockRenderer` — dynamic block draws (plan 16 §3.6, M2 slice).
//!
//! The M2 slice rebuilds a per-atlas-page `ArrayMesh` of the visible dynamic
//! (non-static) blocks and assigns it to a pooled `MeshInstance2D` under the
//! `Layer.block` band. Cached buildings (`drawCached`), cracks, team overlays,
//! status bars, shadows and darkness land in M3/M4. The deterministic visible
//! set comes from `mind_core::render::scan::visible_blocks`; this module owns
//! only the Godot mesh build and the `processBlocks`-style early-out.

use std::collections::{BTreeMap, HashSet};

use godot::classes::{Material, Mesh, MeshInstance2D, Node2D, Shader, ShaderMaterial, Texture2D};
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::content::{BlockDef, BlockKind};
use mind_core::render::scan::{CameraKey, CameraView, visible_blocks};

use crate::sim_host::MindSimHost;

use super::atlas_bind::{RegionResolver, ResolvedRegion};
use super::shaders::ShaderRegistry;
use super::units::{UnitQuad, build_unit_mesh};

/// `Pal.accent` (construct-site tint, `Draw.color(Pal.accent, ...)`).
const ACCENT: u32 = 0xffd3_7fff;
/// `Pal.remove` (deconstruct tint).
const REMOVE: u32 = 0xe554_54ff;
/// `Drill.rotateSpeed` (`Drill.java:59`): rotator degrees per `timeDrilled`.
const DRILL_ROTATE_SPEED: f32 = 2.0;
/// `Turret.heatColor = Pal.turretHeat` (`ab3400`); the alpha carries heat.
const HEAT_COLOR: u32 = 0xab34_00ff;
/// `Vars.itemSize` — belt item icons draw at 5 world px (`Conveyor.draw`).
const ITEM_SIZE: f32 = 5.0;

/// Block-renderer counters mirrored into `MindWorldRenderer`'s stats.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockStats {
    /// Dynamic sprites emitted this frame.
    pub dynamic_sprites: i64,
    /// Mesh rebuilds since boot.
    pub mesh_rebuilds: u64,
    /// Regions that fell back to `error`/placeholder.
    pub missing_regions: u64,
}

/// Dynamic block draw pass.
pub struct BlockRenderer {
    host: Gd<MindSimHost>,
    band_node: Gd<Node2D>,
    /// `Layer.blockBuilding` band: one node per construct site (each carries
    /// its own `blockbuild` uniforms).
    building_band: Gd<Node2D>,
    /// `Shaders.blockbuild` (`ConstructBlock.draw`).
    construct_shader: Option<Gd<Shader>>,
    resolver: RegionResolver,
    /// Pooled per-page nodes (index → node); reused across rebuilds.
    nodes: Vec<Gd<MeshInstance2D>>,
    /// Pooled construct-site nodes (index → node); reused across rebuilds.
    construct_nodes: Vec<Gd<MeshInstance2D>>,
    last_key: Option<CameraKey>,
    last_revision: i64,
    /// Construct sites were drawn last frame: their progress animates without
    /// a world-revision bump, so keep rebuilding while any are visible.
    animating: bool,
    stats: BlockStats,
}

/// One visible construct site (`ConstructBuild.draw`).
struct ConstructSite {
    region: ResolvedRegion,
    x: f32,
    y: f32,
    size: f32,
    rot: f32,
    progress: f32,
    color: u32,
}

impl BlockRenderer {
    /// Builds the renderer over the `Layer.block` band node.
    pub fn new(
        host: Gd<MindSimHost>,
        assets: Option<Gd<crate::assets::MindAssets>>,
        band_node: Gd<Node2D>,
        building_band: Gd<Node2D>,
        shaders: &ShaderRegistry,
    ) -> Self {
        Self {
            host,
            band_node,
            building_band,
            construct_shader: shaders.get("blockbuild"),
            resolver: RegionResolver::new(assets),
            nodes: Vec::new(),
            construct_nodes: Vec::new(),
            last_key: None,
            last_revision: i64::MIN,
            animating: false,
            stats: BlockStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> BlockStats {
        self.stats
    }

    /// Whether the visible set or the world changed (early-out, §3.6).
    fn needs_rebuild(&mut self, view: &CameraView) -> bool {
        let revision = self.host.clone().bind().world_revision();
        let key = CameraKey::from_view(view, TILESIZE as f32);
        if !self.animating && self.last_key == Some(key) && self.last_revision == revision {
            return false;
        }
        self.last_key = Some(key);
        self.last_revision = revision;
        true
    }

    /// Rebuilds the dynamic block meshes when the visible set changed.
    pub fn update(&mut self, view: &CameraView) {
        if !self.needs_rebuild(view) {
            return;
        }

        let mut groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<UnitQuad>)> = BTreeMap::new();
        // Belt items are collected separately and appended to their page after
        // every block quad is in place, so a later belt can never cover an item
        // drawn earlier: `Conveyor.draw` runs its item loop at
        // `Layer.block - 0.1`, above all belts (drawn at `Layer.block - 0.2`).
        let mut item_groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<UnitQuad>)> =
            BTreeMap::new();
        // Construct sites draw through the `blockbuild` shader on their own
        // pooled nodes (T9).
        let mut sites: Vec<ConstructSite> = Vec::new();
        let mut seen = HashSet::new();
        let mut dynamic = 0i64;
        let mut animating = false;
        {
            let host = self.host.clone();
            let host = host.bind();
            let world = host.grid();
            let Some(content) = host.content_registry() else {
                return;
            };
            let size = TILESIZE as f32;
            for tile in visible_blocks(world, content, view) {
                let Some(def) = content.block(tile.block) else {
                    continue;
                };
                // Multiblock: draw once per center entity (first row-major tile).
                if let Some(entity) = world.tile(tile.x, tile.y).build
                    && !seen.insert(entity)
                {
                    continue;
                }
                let w = def.size.max(1) as f32 * size;
                // Java anchors a building sprite at its footprint centre
                // (`Building.x = tile.drawx() + Block.offset`, `Tile.drawx =
                // x*tilesize`); the scan yields the first row-major tile, so odd
                // sizes shift to the centre tile and `Block.offset` covers the
                // even-size half-tile.
                let anchor_x = (tile.x + (def.size - 1) / 2) as f32 * size + def.offset;
                let anchor_y = (tile.y + (def.size - 1) / 2) as f32 * size + def.offset;
                let rotation = host.building_rotation(tile.x, tile.y);
                let mut push = |region: &ResolvedRegion,
                                uv: [f32; 4],
                                cx: f32,
                                cy: f32,
                                quad_w: f32,
                                quad_h: f32,
                                rot: f32,
                                color: u32,
                                frac: f32| {
                    let entry = groups
                        .entry(region.page)
                        .or_insert((region.texture.clone(), Vec::new()));
                    let [u0, v0, u1, v1] = uv;
                    // `frac < 1`: bottom-up reveal (world y-up; v1 is the
                    // sprite's bottom row through the y-flip camera).
                    let h = quad_h * frac;
                    entry.1.push(UnitQuad {
                        cx,
                        cy: cy - (quad_h - h) / 2.0,
                        w: quad_w,
                        h,
                        rot,
                        uv: [u0, v1 - (v1 - v0) * frac, u1, v1],
                        color,
                    });
                };

                // `ConstructBuild.draw`: the target block's icon through the
                // `blockbuild` shader (outline scan + diamond reveal), tinted
                // `Pal.accent`/`Pal.remove`; one pooled node per site (T9).
                if def.kind == BlockKind::ConstructBlock {
                    if let Some((current, progress)) = host.construct_draw(tile.x, tile.y)
                        && let Some(target) = content.block(current)
                        && let Some(region) = self.block_region(target)
                    {
                        let rot = if rotates_sprite(target) {
                            f32::from(rotation) * 90.0
                        } else {
                            0.0
                        };
                        let deconstructing = host
                            .construct_at(tile.x, tile.y)
                            .map(|(_, _, deconstructing)| deconstructing)
                            .unwrap_or(false);
                        sites.push(ConstructSite {
                            region,
                            x: anchor_x,
                            y: anchor_y,
                            size: w,
                            rot,
                            progress: progress.clamp(0.0, 1.0),
                            color: if deconstructing { REMOVE } else { ACCENT },
                        });
                        animating = true;
                        dynamic += 1;
                    }
                    continue;
                }

                let rot = if rotates_sprite(def) {
                    f32::from(rotation) * 90.0
                } else {
                    0.0
                };
                let is_belt = matches!(def.kind, BlockKind::Conveyor | BlockKind::ArmoredConveyor);
                if is_belt {
                    // `ConveyorBuild.draw` runs every frame: a visible belt
                    // keeps the pass rebuilding (items + the frame scroll).
                    animating = true;
                }
                // `Drill.DrillBuild.draw` (T10): base `region` + spinning
                // `-rotator` (`timeDrilled * rotateSpeed`) + `-top`, instead of
                // the composed `-full` icon.
                if def.kind == BlockKind::Drill {
                    if let Some(time_drilled) = host.drill_draw(tile.x, tile.y) {
                        if let Some(base) = self.resolver.find(&def.region) {
                            push(
                                &base,
                                base.uv,
                                anchor_x,
                                anchor_y,
                                w,
                                w,
                                0.0,
                                0xffff_ffff,
                                1.0,
                            );
                        }
                        if let Some(rotator) = self.resolver.find(&format!("{}-rotator", def.name))
                        {
                            push(
                                &rotator,
                                rotator.uv,
                                anchor_x,
                                anchor_y,
                                w,
                                w,
                                time_drilled * DRILL_ROTATE_SPEED,
                                0xffff_ffff,
                                1.0,
                            );
                        }
                        if let Some(top) = self.resolver.find(&format!("{}-top", def.name)) {
                            push(
                                &top,
                                top.uv,
                                anchor_x,
                                anchor_y,
                                w,
                                w,
                                0.0,
                                0xffff_ffff,
                                1.0,
                            );
                        }
                        dynamic += 1;
                        animating = true;
                    }
                    continue;
                }
                // `DrawTurret.draw` (T10): `<basePrefix>block-<size>` base, then
                // the barrel `region` and `-top` at the recoil offset, rotation
                // `drawrot() = rotation - 90` (heat overlays when hot).
                if matches!(
                    def.kind,
                    BlockKind::ItemTurret | BlockKind::PowerTurret | BlockKind::LiquidTurret
                ) {
                    if let Some((rotation, recoil_x, recoil_y, heat)) =
                        host.turret_draw(tile.x, tile.y)
                    {
                        if let Some(base) = self.resolver.find(&format!("block-{}", def.size)) {
                            push(
                                &base,
                                base.uv,
                                anchor_x,
                                anchor_y,
                                w,
                                w,
                                0.0,
                                0xffff_ffff,
                                1.0,
                            );
                        }
                        let rot = rotation - 90.0;
                        let bx = anchor_x + recoil_x;
                        let by = anchor_y + recoil_y;
                        if let Some(barrel) = self.resolver.find(&def.region) {
                            push(&barrel, barrel.uv, bx, by, w, w, rot, 0xffff_ffff, 1.0);
                        }
                        if let Some(top) = self.resolver.find(&format!("{}-top", def.name)) {
                            push(&top, top.uv, bx, by, w, w, rot, 0xffff_ffff, 1.0);
                        }
                        if heat > 0.00001
                            && let Some(heat_region) =
                                self.resolver.find(&format!("{}-heat", def.name))
                        {
                            push(
                                &heat_region,
                                heat_region.uv,
                                bx,
                                by,
                                w,
                                w,
                                rot,
                                (HEAT_COLOR & !0xff) | ((heat.clamp(0.0, 1.0) * 255.0) as u32),
                                1.0,
                            );
                        }
                        dynamic += 1;
                        animating = true;
                    }
                    continue;
                }
                // `ConveyorBuild.draw`: `regions[blendbits][frame]` with the
                // autotile scales from `Autotiler.buildBlending` (cached by
                // the sim on proximity updates). A negative scale mirrors the
                // sprite: swap the UVs like the unit weapon flips do.
                let belt = if is_belt {
                    host.belt_draw(tile.x, tile.y)
                } else {
                    None
                };
                let resolved = match belt {
                    Some((blend_bits, sclx, scly, frame)) => self
                        .resolver
                        .find(&format!("{}-{}-{}", def.name, blend_bits, frame))
                        .map(|region| {
                            let [mut u0, v0, mut u1, v1] = region.uv;
                            if sclx < 0.0 {
                                std::mem::swap(&mut u0, &mut u1);
                            }
                            let (v0, v1) = if scly < 0.0 { (v1, v0) } else { (v0, v1) };
                            (region, [u0, v0, u1, v1])
                        }),
                    None => self.block_region(def).map(|region| {
                        let uv = region.uv;
                        (region, uv)
                    }),
                };
                match resolved {
                    Some((region, uv)) => {
                        push(&region, uv, anchor_x, anchor_y, w, w, rot, 0xffff_ffff, 1.0);
                        if is_belt
                            && let Some((belt_rot, items)) = host.conveyor_items(tile.x, tile.y)
                            && !items.is_empty()
                        {
                            let xs: Vec<f32> = items.iter().map(|(_, xs, _)| *xs).collect();
                            let ys: Vec<f32> = items.iter().map(|(_, _, ys)| *ys).collect();
                            let positions = mind_core::render::belt_item_positions(
                                anchor_x, anchor_y, belt_rot, &xs, &ys,
                            );
                            for ((item, _, _), (ix, iy)) in items.iter().zip(positions) {
                                let Some(item_def) = content.item(*item) else {
                                    continue;
                                };
                                let Some(region) =
                                    self.resolver.find(&format!("item-{}", item_def.name))
                                else {
                                    continue;
                                };
                                let entry = item_groups
                                    .entry(region.page)
                                    .or_insert((region.texture.clone(), Vec::new()));
                                let [u0, v0, u1, v1] = region.uv;
                                entry.1.push(UnitQuad {
                                    cx: ix,
                                    cy: iy,
                                    w: ITEM_SIZE,
                                    h: ITEM_SIZE,
                                    rot: 0.0,
                                    uv: [u0, v0, u1, v1],
                                    color: 0xffff_ffff,
                                });
                            }
                            // Items move without a world-revision bump: keep
                            // rebuilding while any visible belt carries some.
                            animating = true;
                        }
                    }
                    None => {
                        // All three candidates missed (no `-full`, no
                        // `def.region`, no `<name>-0-0`): fall back to the
                        // `error` placeholder; the resolver logs the miss.
                        if let Some(region) = self.resolver.resolve(&def.region) {
                            push(
                                &region,
                                region.uv,
                                anchor_x,
                                anchor_y,
                                w,
                                w,
                                rot,
                                0xffff_ffff,
                                1.0,
                            );
                        }
                    }
                }
                // `Building.draw() -> drawTeamTop()`: the per-team region over
                // the base sprite, only when the block actually loads one
                // (`Block.teamRegion.found()`): the pre-coloured
                // `<name>-team-<team>` region when present, else the
                // `<name>-team` mask tinted with the team colour. Blocks with
                // no team region (conveyors, turrets) draw nothing — never the
                // `error` fallback that `resolve` would hand back.
                if def.draw_team_overlay
                    && let Some(team) = host.building_team(tile.x, tile.y)
                {
                    let team_name = mind_core::game::team::Team::get(team).name.clone();
                    if let Some(region) = self
                        .resolver
                        .find(&format!("{}-team-{}", def.name, team_name))
                    {
                        push(
                            &region,
                            region.uv,
                            anchor_x,
                            anchor_y,
                            w,
                            w,
                            0.0,
                            0xffff_ffff,
                            1.0,
                        );
                    } else if let Some(region) = self.resolver.find(&format!("{}-team", def.name)) {
                        push(
                            &region,
                            region.uv,
                            anchor_x,
                            anchor_y,
                            w,
                            w,
                            0.0,
                            mind_core::game::team::Team::get(team).color.to_rgba8888(),
                            1.0,
                        );
                    }
                }
                dynamic += 1;
            }
        }
        self.animating = animating;

        // Items on top of every block quad of their page.
        for (page, (texture, quads)) in item_groups {
            let entry = groups.entry(page).or_insert((texture, Vec::new()));
            entry.1.extend(quads);
        }

        self.apply_groups(groups);
        self.apply_construct_sites(sites);
        self.stats.dynamic_sprites = dynamic;
        self.stats.mesh_rebuilds += 1;
        self.stats.missing_regions = self.resolver.missing();
    }

    /// Assigns the grouped page meshes to the pooled nodes, hiding spares.
    /// The in-world sprite for a block: the composed `block-<name>-full`
    /// generated icon (base + top layers, e.g. turret base + barrel, drill
    /// rotator/top) when the generator emitted one, else `Block.region`, else
    /// the `Conveyor.regions[0][0]` frame. `None` when nothing is packed.
    /// The candidate chain itself lives in `mind_core::render`.
    fn block_region(&mut self, def: &BlockDef) -> Option<ResolvedRegion> {
        mind_core::render::block_region_candidates(&def.name, &def.region)
            .into_iter()
            .find_map(|name| self.resolver.find(&name))
    }

    /// Draws each visible construct site through its own `blockbuild` node
    /// (`ConstructBlock.draw`: per-region shader uniforms, `Pal.accent` lerped
    /// to `Pal.remove`), hiding pooled spares.
    fn apply_construct_sites(&mut self, sites: Vec<ConstructSite>) {
        let Some(shader) = self.construct_shader.clone() else {
            return;
        };
        for (index, site) in sites.iter().enumerate() {
            let node = if index < self.construct_nodes.len() {
                self.construct_nodes[index].clone()
            } else {
                let node = MeshInstance2D::new_alloc();
                // code-instantiated: one node per visible construct site (each
                // carries its own u_uv/progress/time uniform set).
                self.building_band.clone().add_child(&node);
                self.construct_nodes.push(node.clone());
                node
            };
            let Some(texture) = site.region.texture.clone() else {
                continue;
            };
            let Some(mesh) = build_unit_mesh(&[UnitQuad {
                cx: site.x,
                cy: site.y,
                w: site.size,
                h: site.size,
                rot: site.rot,
                uv: site.region.uv,
                color: 0xffff_ffff,
            }]) else {
                continue;
            };
            // `u_texsize` is the atlas *page* size: `ResolvedRegion.texture` is
            // the page texture (`Shaders.blockbuild` sets it from
            // `region.texture` in Java).
            let (page_w, page_h) = (
                texture.get_width().max(1) as f32,
                texture.get_height().max(1) as f32,
            );
            let [u0, v0, u1, v1] = site.region.uv;
            let mut material = ShaderMaterial::new_gd();
            material.set_shader(&shader);
            material.set_shader_parameter("u_texture", &texture.to_variant());
            material.set_shader_parameter("u_texsize", &Vector2::new(page_w, page_h).to_variant());
            material.set_shader_parameter("u_uv", &Vector2::new(u0, v0).to_variant());
            material.set_shader_parameter("u_uv2", &Vector2::new(u1, v1).to_variant());
            material.set_shader_parameter("u_progress", &site.progress.to_variant());
            material.set_shader_parameter(
                "u_time",
                &(self.host.clone().bind().get_tick() as f32).to_variant(),
            );
            material.set_shader_parameter("u_alpha", &1.0f32.to_variant());
            let mut node = node;
            node.set_mesh(&mesh.upcast::<Mesh>());
            node.set_material(&material.upcast::<Material>());
            node.set_modulate(Color::from_rgba8(
                ((site.color >> 24) & 0xff) as u8,
                ((site.color >> 16) & 0xff) as u8,
                ((site.color >> 8) & 0xff) as u8,
                (site.color & 0xff) as u8,
            ));
            node.set_visible(true);
        }
        for node in self.construct_nodes.iter_mut().skip(sites.len()) {
            node.set_visible(false);
        }
    }

    fn apply_groups(&mut self, groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<UnitQuad>)>) {
        let mut index = 0usize;
        for (_page, (texture, quads)) in groups {
            if quads.is_empty() {
                continue;
            }
            let Some(mesh) = build_unit_mesh(&quads) else {
                continue;
            };
            let node = if index < self.nodes.len() {
                self.nodes[index].clone()
            } else {
                let node = MeshInstance2D::new_alloc();
                // code-instantiated: one pooled node per atlas page; count is
                // page-driven (plan 16 §6.6).
                self.band_node.clone().add_child(&node);
                self.nodes.push(node.clone());
                node
            };
            let mut node = node;
            node.set_mesh(&mesh.upcast::<Mesh>());
            if let Some(texture) = texture {
                node.set_texture(&texture);
            }
            node.set_visible(true);
            index += 1;
        }
        for node in self.nodes.iter_mut().skip(index) {
            node.set_visible(false);
        }
    }
}

/// Whether the block's sprite follows `Building.rotation` (`rotate` blocks
/// that draw their region with `rotdeg()`: belts, conduits, ducts).
pub(crate) fn rotates_sprite(def: &BlockDef) -> bool {
    matches!(
        def.kind,
        BlockKind::Conveyor
            | BlockKind::ArmoredConveyor
            | BlockKind::StackConveyor
            | BlockKind::Conduit
            | BlockKind::ArmoredConduit
            | BlockKind::Duct
            | BlockKind::PayloadConveyor
    )
}

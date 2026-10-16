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

use std::collections::{BTreeMap, HashMap, HashSet};

use godot::classes::canvas_item_material::BlendMode;
use godot::classes::{
    CanvasItemMaterial, Image, ImageTexture, Material, Mesh, MeshInstance2D, Node2D, Shader,
    ShaderMaterial, Texture2D,
};
use godot::obj::NewGd;
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::content::{BlockDef, BlockKind};
use mind_core::render::block_sprite::{crack_index, crack_rotation};
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
/// `Vars.itemSize` — belt item icons draw at 5 world px (`Conveyor.draw`).
const ITEM_SIZE: f32 = 5.0;
/// `Wall.WallBuild.hit` decays by `Time.delta / 10` per frame
/// (`Wall.java:158-159`): a full flash lasts 10 ticks.
const WALL_FLASH_TICKS: f64 = 10.0;

/// One `Wall.WallBuild.draw` decay step (`Wall.java:158-159`):
/// `hit = Mathf.clamp(hit - Time.delta / 10f)` with `dt` in sim ticks.
fn decay_wall_hit(hit: f32, dt: f32) -> f32 {
    (hit - dt / WALL_FLASH_TICKS as f32).clamp(0.0, 1.0)
}

/// Blocks whose `DrawBlock` chain (H03 T8, `render/block_extras.rs`) animates
/// parts that the composed `block-<name>-full` icon already bakes at rest:
/// draw the plain `Block.region` as the base so the part is not doubled.
/// Decided by diffing each chain block's `-full` icon against its region
/// (identical or absent icons keep the current chain; listed blocks differ in
/// the animated area). One comment per entry names the part.
const ANIMATED_CHAIN_BASE: &[&str] = &[
    // `DrawWeave` weaves over the icon's baked centre.
    "phase-weaver",
    // `DrawLiquidTile` re-draws the icon's baked liquid centre.
    "cryofluid-mixer",
    "melter",
    "separator",
    "disassembler",
    // `DrawPistons` pumps over the icon's baked pistons.
    "spore-press",
    // `DrawCultivator` plants over the icon's baked plants.
    "cultivator",
];

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
    /// `Wall.WallBuild.hit` per `flashHit` wall anchor tile (`Wall.java:29,
    /// 146-161`; I02 T4). Set from the sim's wall-collision log, decayed here
    /// (`hit = clamp(hit - delta/10)`, only while the sim runs).
    wall_hits: HashMap<(i32, i32), f32>,
    /// Additive white `Fill.rect` flash overlay node (no texture: the quads
    /// carry the colour).
    flash_node: Gd<MeshInstance2D>,
    /// Tick of the last `update` (flash-decay delta).
    last_tick: f64,
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
        // `Wall.WallBuild.draw` (`Wall.java:146-161`): `Fill.rect` in
        // `flashColor` (white) with `Blending.additive` over the wall sprite.
        let mut flash_node = MeshInstance2D::new_alloc();
        flash_node.set_name("BlockWallFlash");
        let mut flash_material = CanvasItemMaterial::new_gd();
        flash_material.set_blend_mode(BlendMode::ADD);
        flash_node.set_material(&flash_material.upcast::<Material>());
        // A `MeshInstance2D` without a texture is skipped by this project's
        // canvas renderer (found in-engine, I02 T4), so the solid `Fill.rect`
        // overlay carries a 1x1 white texel: the vertex colour with alpha
        // `hit * 0.5` still does the tint.
        if let Some(texture) = white_texel() {
            flash_node.set_texture(&texture);
        }
        // code-instantiated: one pooled additive mesh for every flashing wall;
        // the quad count is data-driven (I02 T2 hides it without a mesh).
        band_node.clone().add_child(&flash_node);
        flash_node.set_visible(false);
        // Above the pooled block-sprite meshes in the same band.
        flash_node.set_z_index(1);
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
            wall_hits: HashMap::new(),
            flash_node,
            last_tick: 0.0,
            stats: BlockStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> BlockStats {
        self.stats
    }

    /// `DrawTurret.load` base chain (`DrawTurret.java:146-170`): `<name>-base`,
    /// else `<basePrefix>block-<size>` (`reinforced-` for the Erekir turrets).
    fn turret_base_region(&mut self, def: &BlockDef) -> Option<ResolvedRegion> {
        for candidate in
            mind_core::content::registries::blocks::turret_parts::base_region_candidates(
                &def.name, def.size,
            )
        {
            if let Some(region) = self.resolver.find(&candidate) {
                return Some(region);
            }
        }
        None
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
        // I02 T4: `Wall.WallBuild.hit` is view state. Drain the sim's
        // wall-collision log and decay every active hit first, so a flash keeps
        // redrawing without a world-revision bump (`needs_rebuild` early-out).
        let tick = self.host.clone().bind().get_tick() as f64;
        // `Wall.java:158-159` decays `hit` per frame with `Time.delta / 10`;
        // `Time.delta` and `dt` are both in sim ticks (1/60 s), so a flash
        // lasts 10 ticks. A gap longer than the whole flash only has to zero it.
        let dt = if self.last_tick > 0.0 {
            (tick - self.last_tick).clamp(0.0, WALL_FLASH_TICKS) as f32
        } else {
            0.0
        };
        self.last_tick = tick;
        let flashes = self.host.clone().bind_mut().drain_wall_flashes();
        for (x, y) in flashes {
            self.wall_hits.insert((i32::from(x), i32::from(y)), 1.0);
        }
        if !self.wall_hits.is_empty() {
            // `Wall.java:158-159`: `hit = clamp(hit - Time.delta / 10)` while
            // not paused (a paused sim does not advance `tick`, so `dt == 0`).
            self.wall_hits.retain(|_, hit| {
                *hit = decay_wall_hit(*hit, dt);
                *hit >= 0.001
            });
        }
        let flashing = !self.wall_hits.is_empty();
        let changed = self.needs_rebuild(view);
        if !changed && !flashing {
            return;
        }

        let mut groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<UnitQuad>)> = BTreeMap::new();
        // Belt items are collected separately and appended to their page after
        // every block quad is in place, so a later belt can never cover an item
        // drawn earlier: `Conveyor.draw` runs its item loop at
        // `Layer.block - 0.1`, above all belts (drawn at `Layer.block - 0.2`).
        let mut item_groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<UnitQuad>)> =
            BTreeMap::new();
        // Cracks + enemy team marks draw above block items (T8).
        let mut crack_groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<UnitQuad>)> =
            BTreeMap::new();
        // Construct sites draw through the `blockbuild` shader on their own
        // pooled nodes (T9).
        let mut sites: Vec<ConstructSite> = Vec::new();
        let mut seen = HashSet::new();
        let mut dynamic = 0i64;
        let mut animating = false;
        // `Wall.WallBuild.draw` flash quads (`Wall.java:146-161`; I02 T4).
        let mut flash_quads: Vec<UnitQuad> = Vec::new();
        {
            let host = self.host.clone();
            let host = host.bind();
            let world = host.grid();
            let Some(content) = host.content_registry() else {
                return;
            };
            let player_team = host.player_team();
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
                // `DrawTurret.draw` (`DrawTurret.java:73-88`): the turret
                // **base** draws at the block layer; the head/shadow/heat and
                // every part are the live turret pass (`render/turret.rs`).
                // `DrawTurret.load` (`:146-170`): `<name>-base`, else the mod's
                // `<prefix>block-<size>`, else `<basePrefix>block-<size>`
                // (`basePrefix = "reinforced-"` for the Erekir turrets).
                if matches!(
                    def.kind,
                    BlockKind::ItemTurret
                        | BlockKind::PowerTurret
                        | BlockKind::LiquidTurret
                        | BlockKind::ContinuousTurret
                        | BlockKind::ContinuousLiquidTurret
                        | BlockKind::LaserTurret
                        | BlockKind::PointDefenseTurret
                        | BlockKind::TractorBeamTurret
                        | BlockKind::BuildTurret
                ) {
                    if let Some(base) = self.turret_base_region(def) {
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
                    dynamic += 1;
                    continue;
                }
                // `UnitFactory.draw` / `Reconstructor.draw` (H03 T9): base
                // `region`, the reconstructor input sides (`-in`; the Java
                // neighbour-blend test is approximated by the rotation
                // fallback) and the output arrow at `rotdeg()`. The construct
                // preview + top region draw in `unit_build.rs` at
                // `Layer.blockOver` (`UnitFactory.java:380-397`,
                // `Reconstructor.java:287-317`).
                if matches!(def.kind, BlockKind::UnitFactory | BlockKind::Reconstructor) {
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
                    let factory_size = def.size.max(1);
                    if def.kind == BlockKind::Reconstructor
                        && let Some(in_region) = self
                            .resolver
                            .find(&format!("{}-in", def.name))
                            .or_else(|| self.resolver.find(&format!("factory-in-{factory_size}")))
                    {
                        push(
                            &in_region,
                            in_region.uv,
                            anchor_x,
                            anchor_y,
                            w,
                            w,
                            f32::from(rotation) * 90.0,
                            0xffff_ffff,
                            1.0,
                        );
                    }
                    if let Some(out_region) = self
                        .resolver
                        .find(&format!("{}-out", def.name))
                        .or_else(|| self.resolver.find(&format!("factory-out-{factory_size}")))
                    {
                        push(
                            &out_region,
                            out_region.uv,
                            anchor_x,
                            anchor_y,
                            w,
                            w,
                            f32::from(rotation) * 90.0,
                            0xffff_ffff,
                            1.0,
                        );
                    }
                    dynamic += 1;
                    continue;
                }
                // Door/switch/sorter/item-source/unloader state sprites belong
                // to the static building cache: those kinds are `drawCached`
                // and `!drawDynamic`, so `visible_blocks` never yields them.
                // The shared sprite chain lives in
                // `mind_core::render::block_sprite::cached_bake_sprites`
                // (H03 T18) and is consumed by `building_cache.rs`.
                let state = host.block_state_draw(tile.x, tile.y);
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
                    None => self.base_region(def).map(|region| {
                        let uv = region.uv;
                        (region, uv)
                    }),
                };
                match resolved {
                    Some((region, uv)) => {
                        push(&region, uv, anchor_x, anchor_y, w, w, rot, 0xffff_ffff, 1.0);
                        // `Wall.WallBuild.draw` (`Wall.java:146-161`): while
                        // `hit >= 0.001`, a white (`flashColor`) additive
                        // `Fill.rect(x, y, size*tilesize, size*tilesize)` with
                        // alpha `hit * 0.5` covers the wall. The pass owns the
                        // decay (`wall_hits`), the sim only logs collisions.
                        if let Some(hit) = self.wall_hits.get(&(tile.x, tile.y)) {
                            let alpha = (hit.clamp(0.0, 1.0) * 0.5 * 255.0) as u32;
                            flash_quads.push(UnitQuad {
                                cx: anchor_x,
                                cy: anchor_y,
                                w,
                                h: w,
                                rot: 0.0,
                                uv: [0.0, 0.0, 1.0, 1.0],
                                color: 0xffff_ff00 | alpha,
                            });
                            animating = true;
                        }
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
                // `BuildingComp.drawCracks` (`:1236-1244`): `cracks-<size>-<i>`
                // at `Layer.blockCracks` (above belt items), grey 0.2 with
                // alpha `0.1 + (1 - healthf) * 0.6`, rotation from the tile y.
                if let Some(state) = state.as_ref()
                    && state.damaged
                    && def.size <= 7
                    && let Some(region) = self.resolver.find(&format!(
                        "cracks-{}-{}",
                        def.size,
                        crack_index(state.healthf)
                    ))
                {
                    let alpha =
                        ((0.1 + (1.0 - state.healthf) * 0.6).clamp(0.0, 1.0) * 255.0) as u32;
                    let entry = crack_groups
                        .entry(region.page)
                        .or_insert((region.texture.clone(), Vec::new()));
                    let [u0, v0, u1, v1] = region.uv;
                    entry.1.push(UnitQuad {
                        cx: anchor_x,
                        cy: anchor_y,
                        w,
                        h: w,
                        rot: crack_rotation(tile.y),
                        uv: [u0, v0, u1, v1],
                        color: 0x3333_3300 | alpha,
                    });
                    // Health changes do not bump the world revision.
                    animating = true;
                }
                // `BuildingComp.drawTeam` (enemy/derelict corner mark,
                // `BlockRenderer.java:752-757`).
                if def.draw_team_overlay
                    && let Some(team) = host.building_team(tile.x, tile.y)
                    && team != player_team
                    && let Some(border) = self.resolver.find("block-border")
                {
                    let half = def.size as f32 * 4.0;
                    let entry = crack_groups
                        .entry(border.page)
                        .or_insert((border.texture.clone(), Vec::new()));
                    let [u0, v0, u1, v1] = border.uv;
                    entry.1.push(UnitQuad {
                        cx: anchor_x - half + 4.0,
                        cy: anchor_y - half + 4.0,
                        w: 8.0,
                        h: 8.0,
                        rot: 0.0,
                        uv: [u0, v0, u1, v1],
                        color: mind_core::game::team::Team::get(team).color.to_rgba8888(),
                    });
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
        // Cracks + team marks above the items (Layer.blockCracks 30.1).
        for (page, (texture, quads)) in crack_groups {
            let entry = groups.entry(page).or_insert((texture, Vec::new()));
            entry.1.extend(quads);
        }

        self.apply_groups(groups);
        self.apply_construct_sites(sites);
        // I02 T4: the additive wall-flash overlay (a solid white quad per
        // flashing wall, no texture: the vertex colour carries it).
        match build_unit_mesh(&flash_quads) {
            Some(mesh) => {
                self.flash_node.set_mesh(&mesh.upcast::<Mesh>());
                self.flash_node.set_visible(true);
            }
            None => self.flash_node.set_visible(false),
        }
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

    /// The base sprite actually drawn by the dynamic pass: for
    /// [`ANIMATED_CHAIN_BASE`] blocks the plain `Block.region` wins (their
    /// `DrawBlock` extras animate the part the composed icon bakes), else the
    /// usual [`Self::block_region`] chain.
    fn base_region(&mut self, def: &BlockDef) -> Option<ResolvedRegion> {
        if ANIMATED_CHAIN_BASE.contains(&def.name.as_str()) {
            self.resolver.find(&def.region)
        } else {
            None
        }
        .or_else(|| self.block_region(def))
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
                // I02 T2: hidden until this site's mesh lands (a visible mesh
                // node without a mesh errors in `mesh_get_surface_count`).
                node.clone().set_visible(false);
                self.construct_nodes.push(node.clone());
                node
            };
            let Some(texture) = site.region.texture.clone() else {
                node.clone().set_visible(false);
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
                node.clone().set_visible(false);
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

/// A 1x1 white `ImageTexture` for the wall-flash overlay (I02 T4). A
/// `MeshInstance2D` without a texture is skipped by this project's canvas
/// renderer (verified in-engine), so the solid `Fill.rect` overlay samples this
/// texel and keeps its alpha in the vertex colour.
pub(crate) fn white_texel() -> Option<Gd<Texture2D>> {
    let data = PackedByteArray::from([255u8, 255, 255, 255]);
    let image = Image::create_from_data(1, 1, false, godot::classes::image::Format::RGBA8, &data)?;
    ImageTexture::create_from_image(&image).map(|texture| texture.upcast::<Texture2D>())
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

#[cfg(test)]
mod tests {
    use super::decay_wall_hit;

    /// `Wall.java:158-159`: `hit -= Time.delta / 10` with `Time.delta` in
    /// ticks, so a full flash (`hit = 1`) is gone after 10 ticks, not 600.
    #[test]
    fn wall_flash_decays_over_ten_ticks() {
        let mut hit = 1.0f32;
        for _ in 0..5 {
            hit = decay_wall_hit(hit, 1.0);
        }
        assert!((hit - 0.5).abs() < 1e-5, "half gone after 5 ticks: {hit}");
        for _ in 0..5 {
            hit = decay_wall_hit(hit, 1.0);
        }
        assert!(hit < 1e-5, "fully decayed after 10 ticks: {hit}");
        assert_eq!(decay_wall_hit(1.0, 0.0), 1.0, "paused (dt 0) holds");
        assert_eq!(decay_wall_hit(0.3, 600.0), 0.0, "clamped at zero");
    }
}

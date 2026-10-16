// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindEditorRenderer` — the map-editor world pass (plan 19 §3.5,
//! `J/editor/EditorRenderer.java`).
//!
//! Java bakes the editor world into 60x60-tile `EditorSpriteCache` chunks: the
//! shared floor renderer draws the floor bands (`drawFloor` + the `walls` cache
//! layer when `showTerrain`), then every non-air `CacheLayer.normal` centre
//! tile's `fullIcon` plus the team-tinted `block-border` corner (`recacheChunk`).
//! The port bakes the same tiles per chunk with the shared
//! [`crate::render::floor`] quad builder and atlas resolver, and re-bakes a
//! chunk when an edit bumps `MindEditor::tiles_revision`.
//!
//! The node lives in `scenes/editor/map_view.tscn`'s `EditorWorldView` slot;
//! `map_view.gd` drives the `SubViewport` camera and uploads the visible
//! canvas rect (`set_view_rect`) so only in-view dirty chunks are baked.
//!
//! ## Coordinate convention (PAR-0109)
//!
//! Chunks are baked in Java world space (tile `(x, y)` centred at
//! `(x * 8, y * 8)`, y-up), exactly like the live world floor pass. The
//! `SubViewport` camera of `map_view.gd` is a plain y-down `Camera2D` whose
//! canvas maps `(0, 0)..(w * 8, h * 8)` onto `MapView`'s letterboxed rect
//! with `(0, 0)` at the rect's top-left. The node therefore carries the
//! transform `EditorRenderer.draw` builds into its camera matrix
//! (`setToTranslation(tx, ty).scale(...).translate(4, 4)` on a y-up batch):
//! a y mirror (tile row 0 at the bottom of the canvas) plus the half-tile
//! `translate(4, 4)`, i.e. local `(x, y)` -> canvas
//! `(x + 4, h * 8 - 4 - y)` ([`canvas_transform`]). The y-flip matches the
//! world renderer's negative-y camera, so sprites use the same v-swap rules
//! as the world passes (floors per `tile_quads`, building icons flipped like
//! `building_cache`).

use std::collections::{BTreeMap, BTreeSet};

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, MeshInstance2D, Node2D, Texture2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::content::{BlockId, BlockKind, ContentRegistry};
use mind_core::ecs::BuildingComp;
use mind_core::io::map::team_color;
use mind_core::render::layer::CacheLayerId;
use mind_core::world::WorldGrid;

use crate::assets::MindAssets;
use crate::editor::MindEditor;
use crate::render::floor::{
    PendingQuad, Quad, RegionAtlas, build_mesh, chunk_used_layers_sized, resolved_uv, tile_quads,
};

/// Upstream `EditorRenderer.chunkSize` (60; the live world floor cache uses 30).
const CHUNK: i32 = 60;
/// `tilesize` in world pixels.
const TILE: f32 = mind_core::config::TILESIZE as f32;
/// Dirty chunks baked per frame (Java processes its recache set every other frame).
const BAKE_BUDGET: usize = 3;
/// `EditorRenderer.packPad` grow for the wall band (`FloorRenderer` walls grow).
const WALL_GROW: f32 = 0.04;
/// `EditorRenderer.recacheChunk` team corner region.
const TEAM_REGION: &str = "block-border";
/// Atlas sprites are packed at 4× (`Arc` `TextureRegion.scl()` == 1/4,
/// `mind-tools pack` scale): a 1-tile sprite is 32 atlas px = 8 world px.
const SPRITE_SCALE: f32 = 4.0;

/// One chunk's floor/wall quads grouped by `(cache layer id, atlas page)`.
type LayerGroups = BTreeMap<(u8, i32), (Option<Gd<Texture2D>>, Vec<Quad>)>;

/// One building icon of a chunk (`fullIcon` quad + its team corner).
struct BlockIcon {
    /// Centre x in world pixels (`x*8 + block.offset`).
    cx: f32,
    /// Centre y in world pixels.
    cy: f32,
    /// `block.fullIcon` region name.
    region: String,
    /// Owning team (`-1` = no build; the border is skipped).
    team: i32,
}

/// Baked nodes of one chunk (floors/walls + building icons).
#[derive(Default)]
struct ChunkNodes {
    floors: Vec<Gd<MeshInstance2D>>,
    blocks: Vec<Gd<MeshInstance2D>>,
}

/// `MindEditorRenderer` — the editor map preview pass.
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct MindEditorRenderer {
    base: Base<Node2D>,
    editor: Option<Gd<MindEditor>>,
    /// Floor/wall band children (baked meshes parent here).
    floors: Option<Gd<Node2D>>,
    /// Building icon + team border children (`Layer.block`).
    blocks: Option<Gd<Node2D>>,
    atlas: Option<RegionAtlas>,
    chunks: BTreeMap<(i32, i32), ChunkNodes>,
    dirty: BTreeSet<(i32, i32)>,
    dims: (i32, i32),
    revision: i64,
    show: (bool, bool, bool),
    /// Visible canvas rect `[x, y, w, h]` in the parent's y-down space
    /// (`set_view_rect`; see [`canvas_rect_to_local`]).
    view_rect: [f32; 4],
}

#[godot_api]
impl INode2D for MindEditorRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            editor: None,
            floors: None,
            blocks: None,
            atlas: None,
            chunks: BTreeMap::new(),
            dirty: BTreeSet::new(),
            dims: (0, 0),
            revision: i64::MIN,
            show: (true, true, true),
            view_rect: [0.0, 0.0, 1.0, 1.0],
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn process(&mut self, _delta: f64) {
        self.pump();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }
}

impl MindEditorRenderer {
    /// Builds the renderer's Godot-derived state (`ready`/`EXTENSION_RELOADED`).
    fn bootstrap(&mut self) {
        if self.floors.is_none() {
            // code-instantiated: the floor/wall band parent (world renderer's
            // band-node pattern; one child holding per-layer meshes).
            let mut floors = Node2D::new_alloc();
            floors.set_name("EditorFloors");
            self.base_mut().add_child(&floors);
            self.floors = Some(floors);
        }
        if self.blocks.is_none() {
            // code-instantiated: building icons + team borders draw above floors.
            let mut blocks = Node2D::new_alloc();
            blocks.set_name("EditorBlocks");
            blocks.set_z_index(10);
            self.base_mut().add_child(&blocks);
            self.blocks = Some(blocks);
        }
        if self.atlas.is_none() {
            let mut assets = self
                .base()
                .try_get_node_as::<MindAssets>("/root/MindAssets");
            if let Some(assets) = assets.as_mut()
                && !assets.bind_mut().load_assets()
            {
                log::warn!("MindEditorRenderer: atlas manifest unavailable");
            }
            self.atlas = Some(RegionAtlas::new(assets));
        }
        self.editor = self
            .base()
            .try_get_node_as::<MindEditor>("/root/Spine/MindEditor");
        // Force a full re-bake against the (possibly reloaded) editor.
        self.revision = i64::MIN;
        self.dims = (0, 0);
    }

    /// One frame: poll the editor, invalidate on edit/size/show changes, bake
    /// up to [`BAKE_BUDGET`] dirty in-view chunks.
    fn pump(&mut self) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        let (width, height, revision, show) = {
            let editor = editor.bind();
            (
                editor.grid_ref().width(),
                editor.grid_ref().height(),
                editor.tiles_revision(),
                (
                    editor.show_floor(),
                    editor.show_terrain(),
                    editor.show_buildings(),
                ),
            )
        };
        if (width, height) != self.dims || revision != self.revision || show != self.show {
            if (width, height) != self.dims {
                self.clear_chunks();
                self.dims = (width, height);
                let (a, b, origin) = canvas_transform(height);
                self.base_mut().set_transform(Transform2D::from_cols(
                    Vector2::new(a[0], a[1]),
                    Vector2::new(b[0], b[1]),
                    Vector2::new(origin[0], origin[1]),
                ));
            }
            self.revision = revision;
            self.show = show;
            self.mark_all_dirty(width, height);
        }
        self.bake_budget();
    }

    /// Queues every chunk of the current map for a re-bake.
    fn mark_all_dirty(&mut self, width: i32, height: i32) {
        self.dirty.extend(all_chunks(width, height));
    }

    /// Frees every baked chunk (`MapEditorDialog` reopen / resize).
    fn clear_chunks(&mut self) {
        for (_, nodes) in std::mem::take(&mut self.chunks) {
            free_nodes(nodes.floors);
            free_nodes(nodes.blocks);
        }
        self.dirty.clear();
    }

    /// Bakes up to [`BAKE_BUDGET`] dirty chunks overlapping the visible rect.
    fn bake_budget(&mut self) {
        if self.dirty.is_empty() {
            return;
        }
        let view = canvas_rect_to_local(self.view_rect, self.dims.1);
        for (cx, cy) in next_bake_batch(&self.dirty, view, BAKE_BUDGET) {
            self.dirty.remove(&(cx, cy));
            self.bake_chunk(cx, cy);
        }
    }

    /// `EditorRenderer.recacheChunk`: re-bakes one chunk's floors/walls and
    /// building icons.
    fn bake_chunk(&mut self, cx: i32, cy: i32) {
        if let Some(nodes) = self.chunks.remove(&(cx, cy)) {
            free_nodes(nodes.floors);
            free_nodes(nodes.blocks);
        }

        let Some(editor) = self.editor.clone() else {
            return;
        };
        let (show_floor, show_terrain, show_buildings) = self.show;
        // (quad, tile x, tile y, cache layer id) — resolved after the bind drops.
        let mut floor_quads: Vec<(PendingQuad, i32, i32, u8)> = Vec::new();
        let mut icons: Vec<BlockIcon> = Vec::new();
        {
            let editor = editor.bind();
            let world = editor.grid_ref();
            let Some(content) = editor.content_ref() else {
                // No registry yet: keep the chunk queued instead of leaving
                // it permanently empty until the next edit.
                self.dirty.insert((cx, cy));
                return;
            };
            // The used-layer scan must cover the editor's 60-tile chunk, not
            // the world cache's 30-tile one (PAR-0109 dark chunks).
            for layer in chunk_used_layers_sized(world, content, cx, cy, CHUNK) {
                match layer {
                    CacheLayerId::Walls if !show_terrain => continue,
                    CacheLayerId::Walls => {}
                    _ if !show_floor => continue,
                    _ => {}
                }
                let [x0, y0, x1, y1] = chunk_tile_bounds(cx, cy, world.width(), world.height());
                for ty in y0..y1 {
                    for tx in x0..x1 {
                        for quad in tile_quads(world, content, world.tile(tx, ty), layer) {
                            floor_quads.push((quad, tx, ty, layer.id()));
                        }
                    }
                }
            }
            if show_buildings {
                collect_buildings(world, content, editor.ecs_ref(), cx, cy, &mut icons);
            }
        }

        let mut floors = Vec::new();
        // Java layer order: floor bands first, the `walls` cache layer above
        // (z = `CacheLayerId.id()`; walls = 8).
        let mut groups: LayerGroups = BTreeMap::new();
        if let Some(atlas) = self.atlas.as_mut() {
            for (quad, tx, ty, layer_id) in floor_quads {
                let resolved = if quad.variant {
                    atlas.resolve_variant(&quad.region, tx, ty)
                } else {
                    atlas.resolve(&quad.region)
                };
                let (page, texture, uv) = resolved_uv(&resolved);
                let entry = groups
                    .entry((layer_id, page))
                    .or_insert((texture, Vec::new()));
                entry.1.push(Quad {
                    cx: quad.cx,
                    cy: quad.cy,
                    w: quad.w,
                    h: quad.h,
                    u0: uv[0],
                    v0: uv[1],
                    u1: uv[2],
                    v1: uv[3],
                    color: quad.color,
                    v_flip: quad.v_flip,
                });
            }
        }
        for ((layer_id, _page), (texture, quads)) in groups {
            if quads.is_empty() {
                continue;
            }
            let Some(floors_node) = self.floors.clone() else {
                break;
            };
            if let Some(mesh) = build_mesh(&quads, WALL_GROW) {
                floors.push(spawn_mesh(floors_node, &mesh, texture, layer_id as i32));
            }
        }

        let mut blocks = Vec::new();
        if let Some(atlas) = self.atlas.as_mut() {
            // `EditorRenderer.recacheChunk`: `fullIcon` centred at
            // `x*8 + block.offset`, then the team-tinted `block-border` with
            // its own size anchored at the icon's lower-left corner
            // (`x*8 + offset - width/2`, origin `0, 0`). Local space is y-up
            // (see the module docs), so sprites take the world pass's v swap
            // (`building_cache`: `flip_v = true`) to stay upright.
            let border = atlas.resolve(TEAM_REGION);
            for icon in &icons {
                let icon_region = atlas.resolve(&icon.region);
                let (icon_w, icon_h) = icon_region.as_ref().map_or((0.0, 0.0), |region| {
                    (region.w / SPRITE_SCALE, region.h / SPRITE_SCALE)
                });
                if let Some(region) = icon_region {
                    let (page, texture, uv) = resolved_uv(&Some(region.clone()));
                    let quad = Quad {
                        cx: icon.cx,
                        cy: icon.cy,
                        w: icon_w,
                        h: icon_h,
                        u0: uv[0],
                        v0: uv[1],
                        u1: uv[2],
                        v1: uv[3],
                        color: 0xffff_ffff,
                        v_flip: true,
                    };
                    if let Some(blocks_node) = self.blocks.clone()
                        && let Some(mesh) = build_mesh(&[quad], 0.0)
                    {
                        blocks.push(spawn_mesh(blocks_node, &mesh, texture, page));
                    }
                }
                if icon.team >= 0
                    && let Some(region) = &border
                {
                    let (page, texture, uv) = resolved_uv(&Some(region.clone()));
                    let (border_w, border_h) = (region.w / SPRITE_SCALE, region.h / SPRITE_SCALE);
                    let (cx, cy) = corner_anchored_center(
                        (icon.cx, icon.cy),
                        (icon_w, icon_h),
                        (border_w, border_h),
                    );
                    let quad = Quad {
                        cx,
                        cy,
                        w: border_w,
                        h: border_h,
                        u0: uv[0],
                        v0: uv[1],
                        u1: uv[2],
                        v1: uv[3],
                        color: team_color(icon.team as u8),
                        v_flip: true,
                    };
                    if let Some(blocks_node) = self.blocks.clone()
                        && let Some(mesh) = build_mesh(&[quad], 0.0)
                    {
                        blocks.push(spawn_mesh(blocks_node, &mesh, texture, page));
                    }
                }
            }
        }

        self.chunks.insert((cx, cy), ChunkNodes { floors, blocks });
    }
}

#[godot_api]
impl MindEditorRenderer {
    /// Uploads the visible canvas rect (`map_view.gd`, every frame): the
    /// parent `SubViewport` space, y-down, `(0, 0)` = the canvas's top-left.
    #[func]
    pub fn set_view_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.view_rect = [x, y, w.max(1.0), h.max(1.0)];
    }

    /// Number of chunks baked (`EditorRenderer.chunks` probe).
    #[func]
    pub fn baked_chunks(&self) -> i64 {
        self.chunks.len() as i64
    }

    /// Dirty chunks still queued for a re-bake.
    #[func]
    pub fn dirty_chunks(&self) -> i64 {
        self.dirty.len() as i64
    }
}

/// Collects the chunk's building icons (`EditorRenderer.recacheChunk`:
/// every non-air `CacheLayer.normal` centre tile).
fn collect_buildings(
    world: &WorldGrid,
    content: &ContentRegistry,
    ecs: &mind_core::ecs::MindWorld,
    cx: i32,
    cy: i32,
    out: &mut Vec<BlockIcon>,
) {
    let [x0, y0, x1, y1] = chunk_tile_bounds(cx, cy, world.width(), world.height());
    for ty in y0..y1 {
        for tx in x0..x1 {
            let tile = world.tile(tx, ty);
            if tile.block == BlockId::AIR {
                continue;
            }
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            if def.cache_layer != CacheLayerId::Normal || def.kind == BlockKind::AirBlock {
                continue;
            }
            // `tile.isCenter()`: a multiblock proxy points at the centre entity.
            let (center, team) = match tile.build {
                Some(entity) => ecs
                    .0
                    .get::<BuildingComp>(entity)
                    .map(|comp| {
                        (
                            comp.pos.x() == tx as i16 && comp.pos.y() == ty as i16,
                            i32::from(comp.team.0),
                        )
                    })
                    .unwrap_or((true, -1)),
                None => (true, -1),
            };
            if !center {
                continue;
            }
            out.push(BlockIcon {
                cx: tx as f32 * TILE + def.offset,
                cy: ty as f32 * TILE + def.offset,
                region: def.region.clone(),
                team,
            });
        }
    }
}

/// The node transform (columns `a`, `b`, origin) mapping local Java world
/// space (y-up, tile `(x, y)` centred at `(x*8, y*8)`) onto the y-down canvas
/// `(0, 0)..(w*8, h*8)`: `EditorRenderer.draw`'s `translate(4, 4)` plus the
/// y mirror that puts tile row 0 at the canvas bottom.
fn canvas_transform(height: i32) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let half = TILE / 2.0;
    (
        [1.0, 0.0],
        [0.0, -1.0],
        [half, height.max(0) as f32 * TILE - half],
    )
}

/// A local (y-up world) point on the canvas (y-down), per [`canvas_transform`].
#[cfg(test)]
fn local_to_canvas(point: [f32; 2], height: i32) -> [f32; 2] {
    let (a, b, origin) = canvas_transform(height);
    [
        origin[0] + a[0] * point[0] + b[0] * point[1],
        origin[1] + a[1] * point[0] + b[1] * point[1],
    ]
}

/// The inverse of [`canvas_transform`] for an axis-aligned canvas rect
/// `[x, y, w, h]` -> the local AABB `[x, y, w, h]`.
fn canvas_rect_to_local(rect: [f32; 4], height: i32) -> [f32; 4] {
    let half = TILE / 2.0;
    let top = height.max(0) as f32 * TILE - half;
    [rect[0] - half, top - (rect[1] + rect[3]), rect[2], rect[3]]
}

/// Every chunk of a `width` x `height` map (`EditorRenderer.resize`:
/// `ceil(width / chunkSize) x ceil(height / chunkSize)`).
fn all_chunks(width: i32, height: i32) -> impl Iterator<Item = (i32, i32)> {
    let chunks_x = (width.max(1) + CHUNK - 1) / CHUNK;
    let chunks_y = (height.max(1) + CHUNK - 1) / CHUNK;
    (0..chunks_y).flat_map(move |cy| (0..chunks_x).map(move |cx| (cx, cy)))
}

/// A chunk's tile bounds `[x0, y0, x1, y1)` clipped to the map.
fn chunk_tile_bounds(cx: i32, cy: i32, width: i32, height: i32) -> [i32; 4] {
    [
        (cx * CHUNK).max(0),
        (cy * CHUNK).max(0),
        ((cx + 1) * CHUNK).min(width),
        ((cy + 1) * CHUNK).min(height),
    ]
}

/// A chunk's local (y-up world) AABB `[x, y, w, h]`: tile spans are
/// `x*8 - 4 .. x*8 + 4`.
fn chunk_local_aabb(cx: i32, cy: i32) -> [f32; 4] {
    let half = TILE / 2.0;
    let size = CHUNK as f32 * TILE;
    [
        (cx * CHUNK) as f32 * TILE - half,
        (cy * CHUNK) as f32 * TILE - half,
        size,
        size,
    ]
}

/// Strict AABB overlap of two `[x, y, w, h]` rects.
fn rects_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

/// Up to `budget` dirty chunks overlapping the local view AABB, in order.
fn next_bake_batch(dirty: &BTreeSet<(i32, i32)>, view: [f32; 4], budget: usize) -> Vec<(i32, i32)> {
    dirty
        .iter()
        .copied()
        .filter(|(cx, cy)| rects_overlap(chunk_local_aabb(*cx, *cy), view))
        .take(budget)
        .collect()
}

/// The centre of a `size` sprite whose lower-left corner (`origin 0, 0` in
/// `cache.draw`) is the lower-left corner of the `icon` sprite at `center`.
fn corner_anchored_center(center: (f32, f32), icon: (f32, f32), size: (f32, f32)) -> (f32, f32) {
    (
        center.0 - icon.0 / 2.0 + size.0 / 2.0,
        center.1 - icon.1 / 2.0 + size.1 / 2.0,
    )
}

/// Adds one baked mesh node under `parent` and returns it.
fn spawn_mesh(
    mut parent: Gd<Node2D>,
    mesh: &Gd<godot::classes::ArrayMesh>,
    texture: Option<Gd<Texture2D>>,
    z: i32,
) -> Gd<MeshInstance2D> {
    let mut node = MeshInstance2D::new_alloc();
    node.set_mesh(&mesh.clone().upcast::<godot::classes::Mesh>());
    if let Some(texture) = texture {
        node.set_texture(&texture);
    }
    node.set_z_index(z);
    parent.add_child(&node);
    node
}

/// Queues floor/block mesh nodes for deletion (`queue_free`).
fn free_nodes(nodes: Vec<Gd<MeshInstance2D>>) {
    for mut node in nodes {
        node.queue_free();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use mind_core::render::list::{block_cache_layer, floor_cache_layer};
    use mind_core::world::tiles::Tiles;

    #[test]
    fn chunk_constants_match_java() {
        assert_eq!(CHUNK, 60);
        assert_eq!(SPRITE_SCALE, 4.0);
    }

    /// PAR-0109: tile row 0 is the bottom of the canvas (Java y-up), and the
    /// half-tile `translate(4, 4)` puts tile (0, 0)'s lower-left corner on the
    /// canvas's bottom-left corner.
    #[test]
    fn tiles_map_to_canvas_with_row_zero_at_the_bottom() {
        let (w, h) = (100, 50);
        let canvas_h = h as f32 * TILE;
        // Tile centres.
        assert_eq!(local_to_canvas([0.0, 0.0], h), [4.0, canvas_h - 4.0]);
        assert_eq!(local_to_canvas([0.0, (h - 1) as f32 * TILE], h), [4.0, 4.0]);
        assert_eq!(
            local_to_canvas([(w - 1) as f32 * TILE, 0.0], h),
            [w as f32 * TILE - 4.0, canvas_h - 4.0]
        );
        // Map corners: Java's lower-left `(-4, -4)` is the canvas bottom-left,
        // the upper-right corner the canvas top-right.
        assert_eq!(local_to_canvas([-4.0, -4.0], h), [0.0, canvas_h]);
        assert_eq!(
            local_to_canvas([w as f32 * TILE - 4.0, canvas_h - 4.0], h),
            [w as f32 * TILE, 0.0]
        );
        // A higher row is higher on screen (smaller canvas y).
        assert!(local_to_canvas([0.0, 8.0], h)[1] < local_to_canvas([0.0, 0.0], h)[1]);
    }

    #[test]
    fn canvas_rect_to_local_inverts_the_node_transform() {
        let h = 70;
        let rect = [33.0, 120.0, 250.0, 90.0];
        let local = canvas_rect_to_local(rect, h);
        let a = local_to_canvas([local[0], local[1]], h);
        let b = local_to_canvas([local[0] + local[2], local[1] + local[3]], h);
        // The local min corner is the canvas bottom-left, the max the top-right.
        assert_eq!(a, [rect[0], rect[1] + rect[3]]);
        assert_eq!(b, [rect[0] + rect[2], rect[1]]);
    }

    /// Every tile belongs to exactly one chunk, and a full update with the
    /// whole canvas in view bakes every chunk.
    #[test]
    fn full_update_bakes_every_chunk() {
        for (w, h) in [(1, 1), (50, 50), (60, 60), (61, 59), (200, 130), (500, 500)] {
            let chunks: BTreeSet<(i32, i32)> = all_chunks(w, h).collect();
            let expected = ((w + CHUNK - 1) / CHUNK) * ((h + CHUNK - 1) / CHUNK);
            assert_eq!(chunks.len() as i32, expected, "{w}x{h}");

            let mut covered = vec![0u8; (w * h) as usize];
            for &(cx, cy) in &chunks {
                let [x0, y0, x1, y1] = chunk_tile_bounds(cx, cy, w, h);
                assert!(x0 < x1 && y0 < y1, "{w}x{h} chunk ({cx},{cy}) is empty");
                for y in y0..y1 {
                    for x in x0..x1 {
                        covered[(y * w + x) as usize] += 1;
                    }
                }
            }
            assert!(covered.iter().all(|count| *count == 1), "{w}x{h}");

            let view = canvas_rect_to_local([0.0, 0.0, w as f32 * TILE, h as f32 * TILE], h);
            let mut dirty = chunks.clone();
            let mut baked = BTreeSet::new();
            for _ in 0..=chunks.len() {
                let batch = next_bake_batch(&dirty, view, BAKE_BUDGET);
                if batch.is_empty() {
                    break;
                }
                for chunk in batch {
                    dirty.remove(&chunk);
                    baked.insert(chunk);
                }
            }
            assert!(dirty.is_empty(), "{w}x{h}: unbaked {dirty:?}");
            assert_eq!(baked, chunks, "{w}x{h}");
        }
    }

    /// The canvas's bottom-left corner shows chunk (0, 0); the top-left shows
    /// the top chunk row.
    #[test]
    fn view_culling_follows_the_flipped_rows() {
        let (w, h) = (200, 200);
        let dirty: BTreeSet<(i32, i32)> = all_chunks(w, h).collect();
        let canvas_h = h as f32 * TILE;
        let bottom_left = canvas_rect_to_local([0.0, canvas_h - 100.0, 100.0, 100.0], h);
        assert_eq!(next_bake_batch(&dirty, bottom_left, 16), vec![(0, 0)]);
        let top_left = canvas_rect_to_local([0.0, 0.0, 100.0, 100.0], h);
        assert_eq!(next_bake_batch(&dirty, top_left, 16), vec![(0, 3)]);
    }

    #[test]
    fn team_border_is_anchored_at_the_icon_lower_left_corner() {
        // A 3x3 block icon (24 px) at (100, 60); an 8 px border sprite.
        let (cx, cy) = corner_anchored_center((100.0, 60.0), (24.0, 24.0), (8.0, 8.0));
        assert_eq!((cx - 4.0, cy - 4.0), (100.0 - 12.0, 60.0 - 12.0));
    }

    fn base_registry() -> ContentRegistry {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        let mut registry = match create_base_content(&bundle, &store, true) {
            Ok(registry) => registry,
            Err(error) => panic!("content boot failed: {error}"),
        };
        if let Err(error) = registry.init() {
            panic!("registry init failed: {error}");
        }
        if let Err(error) = registry.post_init() {
            panic!("registry post_init failed: {error}");
        }
        registry
    }

    /// PAR-0109 dark chunks: the used-layer scan of an editor chunk must
    /// cover its 60 tiles, not the world cache's 30-tile chunk of the same
    /// index. A map whose lower-left 32x32 tiles are solid wall made the old
    /// scan of editor chunk (0, 0) see only the `walls` layer, so the whole
    /// chunk's open floor was never baked (a flat background rectangle).
    #[test]
    fn used_layers_cover_the_whole_editor_chunk() {
        let content = base_registry();
        let (Some(stone), Some(wall)) = (content.block_id("stone"), content.block_id("stone-wall"))
        else {
            panic!("stone / stone-wall missing");
        };
        let (Some(stone_def), Some(wall_def)) = (content.block(stone), content.block(wall)) else {
            panic!("stone / stone-wall defs missing");
        };
        let floor_layer = floor_cache_layer(stone_def);
        assert_eq!(block_cache_layer(wall_def), CacheLayerId::Walls);
        assert_ne!(floor_layer, CacheLayerId::Walls);

        let (w, h) = (130, 70);
        let mut tiles = Tiles::new(w, h);
        tiles.fill(stone, BlockId::AIR);
        for y in 0..32 {
            for x in 0..32 {
                tiles.get_mut(x, y).block = wall;
            }
        }
        let world = WorldGrid::from_tiles(tiles);

        // Every open tile's floor layer is baked by the chunk holding it.
        for (cx, cy) in all_chunks(w, h) {
            let used = chunk_used_layers_sized(&world, &content, cx, cy, CHUNK);
            let [x0, y0, x1, y1] = chunk_tile_bounds(cx, cy, w, h);
            for y in y0..y1 {
                for x in x0..x1 {
                    let layer = if world.tile(x, y).block == wall {
                        CacheLayerId::Walls
                    } else {
                        floor_layer
                    };
                    assert!(
                        used.contains(&layer),
                        "chunk ({cx},{cy}) misses {layer:?} of tile ({x},{y})"
                    );
                }
            }
        }
        // The world-cache (30-tile) scan of index (0, 0) sees only walls.
        let world_scan = crate::render::floor::chunk_used_layers(&world, &content, 0, 0);
        assert!(!world_scan.contains(&floor_layer));
    }
}

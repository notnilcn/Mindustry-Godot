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
//! `map_view.gd` drives the `SubViewport` camera and uploads the visible world
//! rect (`set_view_rect`) so only in-view dirty chunks are baked.

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
    PendingQuad, Quad, RegionAtlas, build_mesh, chunk_used_layers, resolved_uv, tile_quads,
};

/// Upstream `EditorRenderer.chunkSize` (60; the live world floor cache uses 30).
const CHUNK: i32 = 60;
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
    /// Visible world-pixel rect (`set_view_rect`).
    view_rect: Rect2,
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
            view_rect: Rect2::new(Vector2::ZERO, Vector2::new(1.0, 1.0)),
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
            }
            self.revision = revision;
            self.show = show;
            self.mark_all_dirty(width, height);
        }
        self.bake_budget();
    }

    /// Queues every chunk of the current map for a re-bake.
    fn mark_all_dirty(&mut self, width: i32, height: i32) {
        let chunks_x = (width.max(1) + CHUNK - 1) / CHUNK;
        let chunks_y = (height.max(1) + CHUNK - 1) / CHUNK;
        for cy in 0..chunks_y {
            for cx in 0..chunks_x {
                self.dirty.insert((cx, cy));
            }
        }
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
        let visible: Vec<(i32, i32)> = self
            .dirty
            .iter()
            .copied()
            .filter(|(cx, cy)| self.chunk_visible(*cx, *cy))
            .take(BAKE_BUDGET)
            .collect();
        for (cx, cy) in visible {
            self.dirty.remove(&(cx, cy));
            self.bake_chunk(cx, cy);
        }
    }

    /// Whether a chunk's world-pixel AABB intersects the visible rect.
    fn chunk_visible(&self, cx: i32, cy: i32) -> bool {
        let size = mind_core::config::TILESIZE as f32;
        let min = Vector2::new((cx * CHUNK) as f32 * size, (cy * CHUNK) as f32 * size);
        let max = Vector2::new(
            ((cx + 1) * CHUNK) as f32 * size,
            ((cy + 1) * CHUNK) as f32 * size,
        );
        self.view_rect.intersects(Rect2::new(min, max - min))
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
                return;
            };
            for layer in chunk_used_layers(world, content, cx, cy) {
                match layer {
                    CacheLayerId::Walls if !show_terrain => continue,
                    CacheLayerId::Walls => {}
                    _ if !show_floor => continue,
                    _ => {}
                }
                for ty in cy * CHUNK..(cy + 1) * CHUNK {
                    for tx in cx * CHUNK..(cx + 1) * CHUNK {
                        if tx < 0 || ty < 0 || tx >= world.width() || ty >= world.height() {
                            continue;
                        }
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
            // `EditorRenderer.recacheChunk`: `fullIcon` at
            // `x*8 + block.offset - w/2`, then the team-tinted `block-border`
            // corner at the same position.
            let border = atlas.resolve(TEAM_REGION);
            for icon in &icons {
                if let Some(region) = atlas.resolve(&icon.region) {
                    let (page, texture, uv) = resolved_uv(&Some(region.clone()));
                    let quad = Quad {
                        cx: icon.cx,
                        cy: icon.cy,
                        w: region.w / SPRITE_SCALE,
                        h: region.h / SPRITE_SCALE,
                        u0: uv[0],
                        v0: uv[1],
                        u1: uv[2],
                        v1: uv[3],
                        color: 0xffff_ffff,
                        v_flip: false,
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
                    let quad = Quad {
                        cx: icon.cx,
                        cy: icon.cy,
                        w: region.w / SPRITE_SCALE,
                        h: region.h / SPRITE_SCALE,
                        u0: uv[0],
                        v0: uv[1],
                        u1: uv[2],
                        v1: uv[3],
                        color: team_color(icon.team as u8),
                        v_flip: false,
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
    /// Uploads the visible world-pixel rect (`map_view.gd`, every frame).
    #[func]
    pub fn set_view_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.view_rect = Rect2::new(Vector2::new(x, y), Vector2::new(w.max(1.0), h.max(1.0)));
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
    let size = mind_core::config::TILESIZE as f32;
    for ty in cy * CHUNK..(cy + 1) * CHUNK {
        for tx in cx * CHUNK..(cx + 1) * CHUNK {
            if tx < 0 || ty < 0 || tx >= world.width() || ty >= world.height() {
                continue;
            }
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
                cx: tx as f32 * size + def.offset,
                cy: ty as f32 * size + def.offset,
                region: def.region.clone(),
                team,
            });
        }
    }
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

    #[test]
    fn chunk_constants_match_java() {
        assert_eq!(CHUNK, 60);
        assert_eq!(SPRITE_SCALE, 4.0);
    }
}

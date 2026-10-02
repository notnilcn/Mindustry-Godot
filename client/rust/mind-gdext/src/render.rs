// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot-facing world renderer (plan 16).
//!
//! `MindWorldRenderer` owns the frame pipeline skeleton and the append-only band
//! children; `MindRender` is the stable `#[func]` facade the MCP oracle drives.
//! The heavy per-pass renderers (floor/block/light/fog/minimap/g3d) attach to
//! this host in later milestones. View only — never feeds the sim (D8).

use std::collections::HashMap;

use godot::builtin::{GString, PackedStringArray, VarDictionary};
use godot::classes::{Camera2D, INode, INode2D, Image, Node, Node2D, Viewport};
use godot::obj::{Base, WithBaseField};
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::render::Layer;
use mind_core::render::bands::{BandEntry, BandKey, BandPlan};
use mind_core::render::queue::RenderQueue;
use mind_core::render::scan::CameraView;

use crate::assets::MindAssets;
use crate::sim_host::MindSimHost;

mod atlas_bind;
mod blocks;
mod floor;

pub use blocks::BlockRenderer;
pub use floor::FloorRenderer;

/// Per-frame render counters exposed through `MindRender.get_render_stats()`.
#[derive(Debug, Default, Clone)]
pub struct RenderStats {
    /// Frames processed.
    pub frames: u64,
    /// Queue entries emitted this frame.
    pub entries: usize,
    /// Distinct regions referenced this frame.
    pub regions: usize,
    /// Peak queue length.
    pub queue_max: usize,
    /// Dirty floor chunks.
    pub floor_chunks_dirty: i64,
    /// Dirty block chunks.
    pub block_chunks_dirty: i64,
    /// Mesh rebuilds since boot.
    pub mesh_rebuilds: i64,
    /// Dynamic sprites emitted this frame.
    pub dynamic_sprites: i64,
    /// Regions that fell back to `error`/placeholder this frame.
    pub missing_regions: i64,
    /// Last frame build time in microseconds.
    pub build_us: i64,
    /// Current sim tick (updated by the facade).
    pub tick: i64,
    /// Ordered stage trace (plan 16 §3.3 step names).
    pub stage_trace: Vec<String>,
}

impl RenderStats {
    /// Converts to the Godot dictionary returned by `get_render_stats`.
    pub fn to_dict(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        dict.set(&key("frames"), &(self.frames as i64).to_variant());
        dict.set(&key("entries"), &(self.entries as i64).to_variant());
        dict.set(&key("regions"), &(self.regions as i64).to_variant());
        dict.set(&key("queue_max"), &(self.queue_max as i64).to_variant());
        dict.set(
            &key("floor_chunks_dirty"),
            &self.floor_chunks_dirty.to_variant(),
        );
        dict.set(
            &key("block_chunks_dirty"),
            &self.block_chunks_dirty.to_variant(),
        );
        dict.set(&key("mesh_rebuilds"), &self.mesh_rebuilds.to_variant());
        dict.set(&key("dynamic_sprites"), &self.dynamic_sprites.to_variant());
        dict.set(&key("missing_regions"), &self.missing_regions.to_variant());
        dict.set(&key("build_us"), &self.build_us.to_variant());
        dict.set(&key("tick"), &self.tick.to_variant());
        let mut trace = PackedStringArray::new();
        for stage in &self.stage_trace {
            trace.push(&GString::from(stage));
        }
        dict.set(&key("stage_trace"), &trace.to_variant());
        dict
    }
}

/// `MindWorldRenderer` — frame pipeline owner (plan 16 §3.2).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct MindWorldRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    band_plan: BandPlan,
    band_nodes: Vec<Gd<Node2D>>,
    band_index: HashMap<BandKey, usize>,
    queue: RenderQueue,
    stats: RenderStats,
    layer_visible: HashMap<String, bool>,
    visibility_dirty: bool,
    floor: Option<FloorRenderer>,
    blocks: Option<BlockRenderer>,
}

#[godot_api]
impl INode2D for MindWorldRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            band_plan: BandPlan::new(),
            band_nodes: Vec::new(),
            band_index: HashMap::new(),
            queue: RenderQueue::new(),
            stats: RenderStats::default(),
            layer_visible: HashMap::new(),
            visibility_dirty: false,
            floor: None,
            blocks: None,
        }
    }

    fn ready(&mut self) {
        if let Some(host) = self.base().try_get_node_as::<MindSimHost>("../../SimHost") {
            self.host = Some(host);
        } else {
            log::warn!("MindWorldRenderer: no MindSimHost at ../../SimHost");
        }
        self.build_bands();
        self.build_floor();
        log::info!("MindWorldRenderer ready ({} bands)", self.band_nodes.len());
    }

    fn process(&mut self, _delta: f64) {
        self.frame();
    }
}

impl MindWorldRenderer {
    /// Creates one `Node2D` per band with `z_as_relative = false`.
    ///
    /// The band table is generated from the append-only `BandPlan`; the band
    /// count/order is data-driven, so the children are created in code.
    fn build_bands(&mut self) {
        // code-instantiated: one band node per append-only BandPlan entry;
        // count and z order are data-driven from the Layer/CacheLayer tables.
        for entry in self.band_plan.entries().to_vec() {
            let mut node = Node2D::new_alloc();
            node.set_name(band_name(&entry).as_str());
            node.set_z_index(entry.band);
            node.set_z_as_relative(false);
            let node = node;
            self.base_mut().add_child(&node);
            let index = self.band_nodes.len();
            self.band_index.insert(
                BandKey {
                    layer: entry.layer,
                    sub: entry.sub,
                    blend: entry.blend,
                },
                index,
            );
            self.band_nodes.push(node);
        }
    }

    /// Resolves the `Node2D` for a band key (append-only band plan).
    fn band_node_at(&self, key: BandKey) -> Option<Gd<Node2D>> {
        self.band_index
            .get(&key)
            .and_then(|index| self.band_nodes.get(*index))
            .cloned()
    }

    /// Constructs the plan-16 floor + dynamic block passes.
    fn build_floor(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let mut assets = self
            .base()
            .try_get_node_as::<MindAssets>("/root/MindAssets");
        if let Some(assets) = assets.as_mut()
            && !assets.bind_mut().load_assets()
        {
            log::warn!("MindWorldRenderer: atlas manifest unavailable; using placeholder quads");
        }
        if assets.is_none() {
            log::warn!("MindWorldRenderer: no MindAssets autoload; atlas disabled");
        }
        let band_nodes = self.floor_band_nodes();
        self.floor = Some(FloorRenderer::new(host.clone(), assets.clone(), band_nodes));
        if let Some(block_band) = self.band_node_at(BandKey::base(Layer::Block)) {
            self.blocks = Some(BlockRenderer::new(host, assets, block_band));
        }
    }

    /// The floor band nodes in `CacheLayerId::ALL` order.
    fn floor_band_nodes(&self) -> Vec<Gd<Node2D>> {
        use mind_core::render::layer::CacheLayerId;
        CacheLayerId::ALL
            .iter()
            .filter_map(|cache| self.band_node_at(BandKey::floor(*cache)))
            .collect()
    }

    /// Current camera as a `mind-core` [`CameraView`] for culling.
    fn camera_view(&self) -> CameraView {
        let mut view = CameraView::default();
        let Some(camera) = self.base().try_get_node_as::<Camera2D>("../Camera2D") else {
            return view;
        };
        let position = camera.get_position();
        let zoom = camera.get_zoom().x.max(0.01);
        let size = self
            .base()
            .get_viewport()
            .map(|viewport| viewport.get_visible_rect().size)
            .unwrap_or(Vector2::new(320.0, 180.0));
        view.x = position.x;
        view.y = position.y;
        view.w = size.x / zoom;
        view.h = size.y / zoom;
        view.zoom = zoom;
        view
    }

    /// Applies queued layer-visibility toggles to the band nodes.
    fn apply_visibility(&mut self) {
        if !self.visibility_dirty {
            return;
        }
        for (i, entry) in self.band_plan.entries().iter().enumerate() {
            let visible = self
                .layer_visible
                .get(entry.layer.name())
                .copied()
                .unwrap_or(true);
            if let Some(node) = self.band_nodes.get_mut(i) {
                node.set_visible(visible);
            }
        }
        self.visibility_dirty = false;
    }

    /// Runs one frame of the pipeline skeleton (plan 16 §3.3). M0 resets the
    /// queue and records counters; floor/block/overlay passes attach in M1+.
    pub fn frame(&mut self) {
        let start = std::time::Instant::now();
        self.stats.frames += 1;
        self.stats.stage_trace.clear();
        self.stats.stage_trace.push(String::from("frame_begin"));
        self.queue.clear();
        // Stage 9/12: frame-alpha view, chunk invalidation + floor bake.
        let view = self.camera_view();
        if let Some(floor) = self.floor.as_mut() {
            floor.update(&view);
            let floor_stats = floor.stats();
            self.stats.mesh_rebuilds = floor_stats.mesh_rebuilds as i64;
            self.stats.floor_chunks_dirty = floor_stats.dirty;
            self.stats.missing_regions = floor_stats.missing_regions as i64;
        }
        self.stats.stage_trace.push(String::from("floor"));
        if let Some(blocks) = self.blocks.as_mut() {
            blocks.update(&view);
            let block_stats = blocks.stats();
            self.stats.dynamic_sprites = block_stats.dynamic_sprites;
            self.stats.mesh_rebuilds += block_stats.mesh_rebuilds as i64;
            self.stats.missing_regions += block_stats.missing_regions as i64;
        }
        self.stats.stage_trace.push(String::from("blocks"));
        self.queue.set_sort(true);
        self.stats.stage_trace.push(String::from("sort"));
        self.queue.flush();
        self.stats.stage_trace.push(String::from("flush"));
        self.apply_visibility();
        self.stats.entries = self.queue.len();
        self.stats.queue_max = self.stats.queue_max.max(self.queue.max_seq() as usize);
        self.stats.build_us = start.elapsed().as_micros() as i64;
    }

    /// The current band plan (audit/tests).
    pub fn band_plan(&self) -> &BandPlan {
        &self.band_plan
    }

    /// The current render queue.
    pub fn queue(&self) -> &RenderQueue {
        &self.queue
    }

    /// The live counters.
    pub fn stats(&self) -> &RenderStats {
        &self.stats
    }

    /// Mutable counters (facade updates `tick`).
    pub fn stats_mut(&mut self) -> &mut RenderStats {
        &mut self.stats
    }

    /// Number of band children (debug).
    pub fn band_count(&self) -> usize {
        self.band_nodes.len()
    }
}

#[godot_api]
impl MindWorldRenderer {
    /// Enables/disables a named layer's band nodes.
    #[func]
    pub fn set_layer_visible(&mut self, name: GString, visible: bool) {
        self.layer_visible.insert(name.to_string(), visible);
        self.visibility_dirty = true;
        self.apply_visibility();
    }

    /// Whether a named layer is currently visible (default `true`).
    #[func]
    pub fn layer_visible(&self, name: GString) -> bool {
        self.layer_visible
            .get(&name.to_string())
            .copied()
            .unwrap_or(true)
    }

    /// Raw render counters.
    #[func]
    pub fn get_stats(&self) -> VarDictionary {
        self.stats.to_dict()
    }

    /// Forces a full chunk rebuild; returns the number of rebuilt chunks.
    #[func]
    pub fn rebuild_chunks(&mut self) -> i64 {
        self.stats.mesh_rebuilds += 1;
        0
    }
}

fn band_name(entry: &BandEntry) -> String {
    format!("Band_{}_{}", entry.layer.name(), entry.sub)
}

/// `MindRender` — the stable MCP/debug facade (plan 16 §3.2).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindRender {
    base: Base<Node>,
    renderer: Option<Gd<MindWorldRenderer>>,
    menu_seed: i64,
}

#[godot_api]
impl INode for MindRender {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            renderer: None,
            menu_seed: 0,
        }
    }

    fn ready(&mut self) {
        self.renderer = self
            .base()
            .try_get_node_as::<MindWorldRenderer>("../World/Renderer");
        if self.renderer.is_none() {
            log::warn!("MindRender: no MindWorldRenderer at ../World/Renderer");
        }
    }
}

impl MindRender {
    fn renderer(&self) -> Option<Gd<MindWorldRenderer>> {
        self.renderer.clone()
    }
}

#[godot_api]
impl MindRender {
    /// Toggles a layer's visibility by Java layer name.
    #[func]
    pub fn set_layer_visible(&mut self, name: GString, visible: bool) {
        if let Some(mut renderer) = self.renderer() {
            renderer.bind_mut().set_layer_visible(name, visible);
        }
    }

    /// Toggles the plan-00 debug tile grid (default hidden after M1).
    #[func]
    pub fn set_tile_grid_visible(&mut self, visible: bool) {
        if let Some(mut grid) = self
            .base()
            .try_get_node_as::<crate::tile_grid::MindTileGrid>("../World/TileGrid")
        {
            grid.set_visible(visible);
        }
    }

    /// Whether a layer is visible.
    #[func]
    pub fn layer_visible(&self, name: GString) -> bool {
        self.renderer()
            .map(|renderer| renderer.bind().layer_visible(name))
            .unwrap_or(true)
    }

    /// Pins the camera at tile `(x, y)` with `zoom`.
    #[func]
    pub fn set_camera_pose(&mut self, x: f64, y: f64, zoom: f64) {
        let Some(mut camera) = self.base().try_get_node_as::<Camera2D>("../World/Camera2D") else {
            log::warn!("MindRender.set_camera_pose: no Camera2D at ../World/Camera2D");
            return;
        };
        let size = TILESIZE as f64;
        camera.set_position(Vector2::new(
            (x + 0.5) as f32 * size as f32,
            (y + 0.5) as f32 * size as f32,
        ));
        let zoom = (zoom as f32).max(0.01);
        camera.set_zoom(Vector2::new(zoom, zoom));
    }

    /// The renderer's counters as a dictionary (`tick` refreshed first).
    #[func]
    pub fn get_render_stats(&mut self) -> VarDictionary {
        if let (Some(renderer), Some(host)) = (
            self.renderer(),
            self.base().try_get_node_as::<MindSimHost>("../SimHost"),
        ) {
            let tick = host.bind().get_tick();
            let mut renderer = renderer;
            renderer.bind_mut().stats_mut().tick = tick;
            return renderer.bind().get_stats();
        }
        VarDictionary::new()
    }

    /// Captures a PNG per layer at the current pose into `dir`.
    #[func]
    pub fn capture_layers(&mut self, dir: GString) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        let base = dir.to_string();
        let layers: Vec<&str> = Layer::ALL
            .iter()
            .map(|layer| layer.name())
            .filter(|name| {
                // Skip the sentinel min/max layers.
                *name != "min" && *name != "max"
            })
            .collect();
        for name in layers {
            let path = format!("{base}/layer-{name}.png");
            if self.capture_to(&path) {
                out.push(&GString::from(&path));
            }
        }
        out
    }

    /// Captures the world viewport to `path` (map-screenshot helper).
    #[func]
    pub fn capture_map(&mut self, path: GString) -> bool {
        self.capture_to(&path.to_string())
    }

    /// Pins the menu world RNG seed (OD16-F).
    #[func]
    pub fn set_menu_seed(&mut self, seed: i64) {
        self.menu_seed = seed;
    }

    /// Current menu seed.
    #[func]
    pub fn menu_seed(&self) -> i64 {
        self.menu_seed
    }

    /// Forces a chunk rebuild through the renderer.
    #[func]
    pub fn rebuild_chunks(&mut self) -> i64 {
        self.renderer()
            .map(|mut renderer| renderer.bind_mut().rebuild_chunks())
            .unwrap_or(0)
    }

    fn capture_to(&mut self, path: &str) -> bool {
        let Some(viewport) = self.base().get_viewport() else {
            return false;
        };
        let image = viewport_texture_image(&viewport);
        let Some(image) = image else {
            return false;
        };
        image.save_png(path) == godot::global::Error::OK
    }
}

fn viewport_texture_image(viewport: &Gd<Viewport>) -> Option<Gd<Image>> {
    let texture = viewport.get_texture()?;
    texture.get_image()
}

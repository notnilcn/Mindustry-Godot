// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot-facing world renderer (plan 16).
//!
//! `MindWorldRenderer` owns the frame pipeline skeleton and the append-only band
//! children; `MindRender` is the stable `#[func]` facade the MCP oracle drives.
//! The heavy per-pass renderers (floor/block/light/fog/minimap/g3d) attach to
//! this host in later milestones. View only — never feeds the sim (D8).

use std::collections::HashMap;

use godot::builtin::{
    Color, GString, PackedByteArray, PackedFloat32Array, PackedStringArray, Rect2i, VarDictionary,
    Vector2i,
};
use godot::classes::notify::{CanvasItemNotification, NodeNotification};
use godot::classes::{Camera2D, INode, INode2D, Image, ImageTexture, Node, Node2D, Viewport};
use godot::obj::{Base, WithBaseField};
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::content::ContentRegistry;
use mind_core::render::Layer;
use mind_core::render::bands::{BandEntry, BandKey, BandPlan};
use mind_core::render::layer::CacheLayerId;
use mind_core::render::lod::Lod;
use mind_core::render::menu::MenuWorld;
use mind_core::render::queue::RenderQueue;
use mind_core::render::rules::RulesRenderView;
use mind_core::render::scan::CameraView;

use crate::assets::MindAssets;
use crate::sim_host::MindSimHost;

mod atlas_bind;
mod blocks;
mod building_cache;
mod debug;
mod env;
mod floor;
mod fog;
mod g3d;
mod light;
mod load;
mod menu;
mod minimap;
mod overlays;
mod pixelate;
mod shaders;
mod shadow;

pub use blocks::BlockRenderer;
pub use building_cache::BuildingCacheRenderer;
pub use debug::DebugCollisionRenderer;
pub use env::EnvRenderer;
pub use floor::FloorRenderer;
pub use fog::FogRenderer;
pub use g3d::PlanetRenderer;
pub use light::LightRenderer;
pub use load::LoadRenderer;
pub use menu::MenuRenderer;
pub use minimap::MindMinimap;
pub use overlays::OverlayRenderer;
pub use pixelate::Pixelator;
pub use shaders::ShaderRegistry;
pub use shadow::ShadowRenderer;

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
    /// Cached-building sprites emitted this frame.
    pub cached_sprites: i64,
    /// Regions that fell back to `error`/placeholder this frame.
    pub missing_regions: i64,
    /// Shaders loaded by the `ShaderRegistry` (plan 16 M8).
    pub shaders_loaded: i64,
    /// Missing-shader neutral substitutions (plan 16 M8).
    pub shader_substitutions: i64,
    /// Shadow-map rebuilds since boot (plan 16 M3).
    pub shadow_rebuilds: i64,
    /// Darkness-map rebuilds since boot (plan 16 M3).
    pub darkness_rebuilds: i64,
    /// Shadow events recorded in the last rebuild.
    pub shadow_events: i64,
    /// Light-map rebuilds since boot (plan 16 M5).
    pub light_rebuilds: i64,
    /// Live circle lights in the last light rebuild.
    pub light_circles: i64,
    /// Debug hitbox quads in the last rebuild (plan 16 M6).
    pub debug_hitboxes: i64,
    /// Whether the env pass drew this frame (plan 16 M6).
    pub env_underwater: bool,
    /// Whether the fog composite drew this frame (plan 16 M6).
    pub fog_active: bool,
    /// Queued fog events (plan 16 M6).
    pub fog_events: i64,
    /// Displayed core-protection edges (plan 16 M5).
    pub overlay_edges: i64,
    /// Whether the pixelator is enabled (plan 16 M6).
    pub pixelate: bool,
    /// Low-res target width (plan 16 M6).
    pub pixel_w: i64,
    /// Low-res target height (plan 16 M6).
    pub pixel_h: i64,
    /// Whether the menu world is generated (plan 16 M7).
    pub menu_ready: bool,
    /// Menu tiles (plan 16 M7).
    pub menu_tiles: i64,
    /// Menu flyers (plan 16 M7).
    pub menu_flyers: i64,
    /// Planet sector count at the loading subdivision (plan 16 M7).
    pub planet_sectors: i64,
    /// `Lod.l1` (plan 16 M6).
    pub lod_l1: bool,
    /// `Lod.l2` (plan 16 M6).
    pub lod_l2: bool,
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
        dict.set(&key("cached_sprites"), &self.cached_sprites.to_variant());
        dict.set(&key("missing_regions"), &self.missing_regions.to_variant());
        dict.set(&key("shaders_loaded"), &self.shaders_loaded.to_variant());
        dict.set(
            &key("shader_substitutions"),
            &self.shader_substitutions.to_variant(),
        );
        dict.set(&key("shadow_rebuilds"), &self.shadow_rebuilds.to_variant());
        dict.set(
            &key("darkness_rebuilds"),
            &self.darkness_rebuilds.to_variant(),
        );
        dict.set(&key("shadow_events"), &self.shadow_events.to_variant());
        dict.set(&key("light_rebuilds"), &self.light_rebuilds.to_variant());
        dict.set(&key("light_circles"), &self.light_circles.to_variant());
        dict.set(&key("debug_hitboxes"), &self.debug_hitboxes.to_variant());
        dict.set(&key("env_underwater"), &self.env_underwater.to_variant());
        dict.set(&key("fog_active"), &self.fog_active.to_variant());
        dict.set(&key("fog_events"), &self.fog_events.to_variant());
        dict.set(&key("overlay_edges"), &self.overlay_edges.to_variant());
        dict.set(&key("pixelate"), &self.pixelate.to_variant());
        dict.set(&key("pixel_w"), &self.pixel_w.to_variant());
        dict.set(&key("pixel_h"), &self.pixel_h.to_variant());
        dict.set(&key("menu_ready"), &self.menu_ready.to_variant());
        dict.set(&key("menu_tiles"), &self.menu_tiles.to_variant());
        dict.set(&key("menu_flyers"), &self.menu_flyers.to_variant());
        dict.set(&key("planet_sectors"), &self.planet_sectors.to_variant());
        dict.set(&key("lod_l1"), &self.lod_l1.to_variant());
        dict.set(&key("lod_l2"), &self.lod_l2.to_variant());
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
    /// Atlas autoload captured at boot for the menu-background bake.
    assets: Option<Gd<MindAssets>>,
    band_plan: BandPlan,
    band_nodes: Vec<Gd<Node2D>>,
    band_index: HashMap<BandKey, usize>,
    queue: RenderQueue,
    stats: RenderStats,
    layer_visible: HashMap<String, bool>,
    visibility_dirty: bool,
    floor: Option<FloorRenderer>,
    blocks: Option<BlockRenderer>,
    building_cache: Option<BuildingCacheRenderer>,
    shadow: Option<ShadowRenderer>,
    light: Option<LightRenderer>,
    debug: Option<DebugCollisionRenderer>,
    env: Option<EnvRenderer>,
    fog: Option<FogRenderer>,
    overlays: Option<OverlayRenderer>,
    pixelator: Pixelator,
    planet: PlanetRenderer,
    menu: MenuRenderer,
    load: LoadRenderer,
    rules: RulesRenderView,
    lod: Lod,
    draw_light: bool,
    draw_hitboxes: bool,
    shaders: ShaderRegistry,
    /// Whether the plan-19 editor view is mounted (suspends the main bands).
    editor_view_active: bool,
}

#[godot_api]
impl INode2D for MindWorldRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            assets: None,
            band_plan: BandPlan::new(),
            band_nodes: Vec::new(),
            band_index: HashMap::new(),
            queue: RenderQueue::new(),
            stats: RenderStats::default(),
            layer_visible: HashMap::new(),
            visibility_dirty: false,
            floor: None,
            blocks: None,
            building_cache: None,
            shadow: None,
            light: None,
            debug: None,
            env: None,
            fog: None,
            overlays: None,
            pixelator: Pixelator::new(),
            planet: PlanetRenderer::new(),
            menu: MenuRenderer::new(),
            load: LoadRenderer::new(),
            rules: RulesRenderView::default(),
            lod: Lod::default(),
            draw_light: true,
            draw_hitboxes: false,
            shaders: ShaderRegistry::new(),
            editor_view_active: false,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        // `ready()` is one-shot; rebuild the Godot-derived children on reload.
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        self.frame();
    }
}

impl MindWorldRenderer {
    /// Rebuilds the Godot-derived pipeline state (adopts persisted band
    /// children) after `ready()` or a hot reload.
    fn bootstrap(&mut self) {
        if let Some(host) = self.base().try_get_node_as::<MindSimHost>("../../SimHost") {
            self.host = Some(host);
        } else {
            log::warn!("MindWorldRenderer: no MindSimHost at ../../SimHost");
        }
        self.build_bands();
        self.build_floor();
        log::info!("MindWorldRenderer ready ({} bands)", self.band_nodes.len());
    }

    /// Creates one `Node2D` per band with `z_as_relative = false`.
    ///
    /// The band table is generated from the append-only `BandPlan`; the band
    /// count/order is data-driven, so the children are created in code.
    fn build_bands(&mut self) {
        // code-instantiated: one band node per append-only BandPlan entry;
        // count and z order are data-driven from the Layer/CacheLayer tables.
        //
        // Upstream draws `CacheLayer.walls` at `Layer.block - 0.09` (after the
        // shadow composite at `Layer.block - 1`), not with the floor bands; the
        // integer band table has no slot between `blockUnder` and `block`, so
        // the walls node shares the block band's z and is created before it
        // (tree order draws it under building sprites, over shadows). The band
        // number in the plan is unchanged (append-only ABI).
        let walls_z = self.band_plan.band(BandKey::base(Layer::Block));
        //
        // Hot reload persists the previous instance's band nodes as plain
        // engine children while `band_nodes`/`band_index` reset, so adopt a
        // child with the same name instead of duplicating it, and drop the
        // stale render-bank children it still carries (the rebuilt passes
        // re-attach fresh ones).
        for entry in self.band_plan.entries().to_vec() {
            let name = band_name(&entry);
            let adopted = self
                .base()
                .get_node_or_null(name.as_str())
                .and_then(|node| node.try_cast::<Node2D>().ok());
            let mut node = if let Some(mut node) = adopted {
                while let Some(mut child) = node.get_child(0) {
                    node.remove_child(&child);
                    child.queue_free();
                }
                node
            } else {
                let mut node = Node2D::new_alloc();
                node.set_name(name.as_str());
                self.base_mut().add_child(&node);
                node
            };
            let walls = entry.layer == Layer::Floor && entry.sub == CacheLayerId::Walls.id();
            node.set_z_index(if walls { walls_z } else { entry.band });
            node.set_z_as_relative(false);
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
        self.assets = assets.clone();
        let assets_dir = assets
            .as_ref()
            .map(|assets| assets.bind().assets_dir().to_string());
        let manifest_loaded = self.shaders.load(assets_dir.as_deref());
        self.stats.shaders_loaded = self.shaders.loaded() as i64;
        self.stats.shader_substitutions = self.shaders.substitutions();
        log::info!(
            "[render] shaders loaded={} substitutions={} manifest={}",
            self.stats.shaders_loaded,
            self.stats.shader_substitutions,
            manifest_loaded
        );
        let band_nodes = self.floor_band_nodes();
        self.floor = Some(FloorRenderer::new(host.clone(), assets.clone(), band_nodes));
        if let (Some(shadow_band), Some(dark_band)) = (
            self.band_node_at(BandKey::base(Layer::BlockUnder)),
            self.band_node_at(BandKey::base(Layer::Darkness)),
        ) {
            self.shadow = Some(ShadowRenderer::new(
                host.clone(),
                shadow_band,
                dark_band,
                &self.shaders,
            ));
        }
        if let (Some(under_band), Some(block_band)) = (
            self.band_node_at(BandKey::base(Layer::BlockUnder)),
            self.band_node_at(BandKey::base(Layer::Block)),
        ) {
            self.building_cache = Some(BuildingCacheRenderer::new(
                host.clone(),
                assets.clone(),
                under_band,
                block_band,
            ));
        }
        if let Some(block_band) = self.band_node_at(BandKey::base(Layer::Block)) {
            self.blocks = Some(BlockRenderer::new(host.clone(), assets, block_band));
        }
        if let Some(light_band) = self.band_node_at(BandKey::base(Layer::Light)) {
            self.light = Some(LightRenderer::new(host.clone(), light_band, &self.shaders));
        }
        if let Some(light_band) = self.band_node_at(BandKey::base(Layer::Light)) {
            self.env = Some(EnvRenderer::new(light_band));
        }
        if let Some(fog_band) = self.band_node_at(BandKey::base(Layer::FogOfWar)) {
            self.fog = Some(FogRenderer::new(fog_band));
        }
        if let Some(overlay_band) = self.band_node_at(BandKey::base(Layer::OverlayUi)) {
            self.overlays = Some(OverlayRenderer::new(overlay_band.clone()));
        }
        if let Some(overlay_band) = self.band_node_at(BandKey::base(Layer::OverlayUi)) {
            self.debug = Some(DebugCollisionRenderer::new(host, overlay_band));
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
        self.stats.stage_trace.push(String::from("pre_draw"));
        self.stats.stage_trace.push(String::from("cutscene"));
        self.queue.clear();
        // Stage 9/12: frame-alpha view, chunk invalidation + floor bake.
        let view = self.camera_view();
        // Stage 4: `Lod.update()` — `scale = screen_width / camera.width`.
        self.lod.update(view.w * view.zoom, view.w);
        self.stats.lod_l1 = self.lod.l1;
        self.stats.lod_l2 = self.lod.l2;
        self.stats.stage_trace.push(String::from("lod"));
        self.stats.stage_trace.push(String::from("process_blocks"));
        // `mesh_rebuilds` is cumulative per pass; each pass reports its own boot
        // total, so the frame total is the sum (never an accumulation).
        let mut mesh_rebuilds = 0i64;
        if let Some(floor) = self.floor.as_mut() {
            floor.update(&view);
            let floor_stats = floor.stats();
            mesh_rebuilds += floor_stats.mesh_rebuilds as i64;
            self.stats.floor_chunks_dirty = floor_stats.dirty;
            self.stats.missing_regions = floor_stats.missing_regions as i64;
        }
        self.stats.stage_trace.push(String::from("floor"));
        if let Some(cache) = self.building_cache.as_mut() {
            cache.update(&view);
            let cache_stats = cache.stats();
            self.stats.cached_sprites = cache_stats.sprites;
            mesh_rebuilds += cache_stats.mesh_rebuilds as i64;
            self.stats.missing_regions += cache_stats.missing_regions as i64;
        }
        self.stats.stage_trace.push(String::from("building_cache"));
        if let Some(blocks) = self.blocks.as_mut() {
            blocks.update(&view);
            let block_stats = blocks.stats();
            self.stats.dynamic_sprites = block_stats.dynamic_sprites;
            mesh_rebuilds += block_stats.mesh_rebuilds as i64;
            self.stats.missing_regions += block_stats.missing_regions as i64;
        }
        self.stats.mesh_rebuilds = mesh_rebuilds;
        self.stats.stage_trace.push(String::from("blocks"));
        if let Some(shadow) = self.shadow.as_mut() {
            shadow.update(false);
            let shadow_stats = shadow.stats();
            self.stats.shadow_rebuilds = shadow_stats.shadow_rebuilds;
            self.stats.darkness_rebuilds = shadow_stats.darkness_rebuilds;
            self.stats.shadow_events = shadow_stats.shadow_events;
        }
        self.stats.stage_trace.push(String::from("shadows"));
        self.stats.stage_trace.push(String::from("blockbuild"));
        if let Some(env) = self.env.as_mut() {
            env.set_rules_env(self.rules.env);
            env.update(&view);
            self.stats.env_underwater = env.stats().underwater;
        }
        self.stats.stage_trace.push(String::from("env"));
        self.stats.stage_trace.push(String::from("markers"));
        let draw_light = self.draw_light;
        if let Some(light) = self.light.as_mut() {
            // `Renderer.draw`: the light composite only runs when
            // `Rules.lighting` is set; the ambient alpha comes from the
            // `Rules` `ambientLight` alpha (never a hardcoded near-black).
            light.update(
                &view,
                self.rules.lighting,
                self.rules.ambient_light[3],
                draw_light,
            );
            let light_stats = light.stats();
            self.stats.light_rebuilds = light_stats.rebuilds;
            self.stats.light_circles = light_stats.lights;
        }
        self.stats.stage_trace.push(String::from("light"));
        self.stats.stage_trace.push(String::from("darkness"));
        self.stats.stage_trace.push(String::from("bloom"));
        let draw_hitboxes = self.draw_hitboxes;
        if let Some(debug) = self.debug.as_mut() {
            debug.update(&view, draw_hitboxes);
            self.stats.debug_hitboxes = debug.stats().hitboxes;
        }
        self.stats.stage_trace.push(String::from("debug"));
        self.stats.stage_trace.push(String::from("plans"));
        self.stats.stage_trace.push(String::from("overlay_ui"));
        if let Some(overlays) = self.overlays.as_mut() {
            overlays.update(&view);
            self.stats.overlay_edges = overlays.stats().edges;
        }
        self.stats.stage_trace.push(String::from("overlays"));
        if let Some(fog) = self.fog.as_mut() {
            fog.update(&view, &self.rules);
            let fog_stats = fog.stats();
            self.stats.fog_active = fog_stats.active;
            self.stats.fog_events = fog_stats.events;
        }
        self.stats.stage_trace.push(String::from("fog"));
        self.stats.stage_trace.push(String::from("space"));
        self.stats.stage_trace.push(String::from("draw_over"));
        self.menu.update(1.0 / 60.0);
        self.stats.menu_ready = self.menu.is_ready();
        self.stats.menu_tiles = self.menu.tile_count() as i64;
        self.stats.menu_flyers = self.menu.flyers() as i64;
        self.stats.planet_sectors = self.planet.sector_counts(2).0 as i64;
        self.stats.stage_trace.push(String::from("menu_planet"));
        let screen = self
            .base()
            .get_viewport()
            .map(|viewport| viewport.get_visible_rect().size)
            .unwrap_or(Vector2::new(1280.0, 720.0));
        self.pixelator.update(
            view.zoom,
            view.w,
            view.h,
            screen.x as i32,
            screen.y as i32,
            false,
            1.0,
        );
        self.stats.pixelate = self.pixelator.enabled();
        self.stats.pixel_w = self.pixelator.size().0 as i64;
        self.stats.pixel_h = self.pixelator.size().1 as i64;
        self.queue.set_sort(true);
        self.stats.stage_trace.push(String::from("sort"));
        self.queue.flush();
        self.stats.stage_trace.push(String::from("flush"));
        self.stats.stage_trace.push(String::from("post_draw"));
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

    /// Whether the nullable `shield` shader resolved (plan 16 §3.9,
    /// `Renderer.java:403`; `false` skips the shield band bracket).
    #[func]
    pub fn shield_available(&self) -> bool {
        self.shaders.get("shield").is_some()
    }

    /// Shader registry status (plan 16 M8 oracle).
    #[func]
    pub fn shader_status(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        dict.set(
            &GString::from("manifest_loaded"),
            &self.shaders.manifest_loaded().to_variant(),
        );
        dict.set(
            &GString::from("loaded"),
            &(self.shaders.loaded() as i64).to_variant(),
        );
        dict.set(
            &GString::from("substitutions"),
            &self.shaders.substitutions().to_variant(),
        );
        let mut missing = PackedStringArray::new();
        for name in self.shaders.missing() {
            missing.push(&GString::from(name));
        }
        dict.set(&GString::from("missing"), &missing.to_variant());
        dict
    }

    /// Enables/disables the `LightRenderer` composite (plan 16 M5).
    #[func]
    pub fn set_draw_light(&mut self, enabled: bool) {
        self.draw_light = enabled;
    }

    /// Enables/disables the `DebugCollisionRenderer` overlay (plan 16 M6).
    #[func]
    pub fn set_draw_hitboxes(&mut self, enabled: bool) {
        self.draw_hitboxes = enabled;
    }

    /// Enables/disables the pixelator (plan 16 M6).
    #[func]
    pub fn set_pixelate(&mut self, enabled: bool) {
        self.pixelator.set_enabled(enabled);
    }

    /// Enables/disables the fog composite (plan 16 M6 / plan 12 `Rules.fog`).
    #[func]
    pub fn set_fog_enabled(&mut self, enabled: bool) {
        self.rules.fog = enabled;
    }

    /// Sets `Rules.staticFog`.
    #[func]
    pub fn set_static_fog(&mut self, enabled: bool) {
        self.rules.static_fog = enabled;
    }

    /// Sets the active `Rules.env` mask (plan 16 §3.12).
    #[func]
    pub fn set_rules_env(&mut self, mask: i64) {
        self.rules.env = mask as u32;
    }

    /// Sets `Rules.lighting` and `Rules.ambientLight` for the light composite
    /// (plan 16 M5). The light pass stays hidden while `lighting` is `false`.
    #[func]
    pub fn set_rules_lighting(
        &mut self,
        lighting: bool,
        ambient_r: f64,
        ambient_g: f64,
        ambient_b: f64,
        ambient_a: f64,
    ) {
        self.rules.lighting = lighting;
        self.rules.ambient_light = [
            ambient_r as f32,
            ambient_g as f32,
            ambient_b as f32,
            ambient_a as f32,
        ];
    }

    /// Queues a packed fog event (plan 12 `ClientHooks::fog_handle_event`).
    #[func]
    pub fn push_fog_event(&mut self, x: i32, y: i32, radius: i32, team: i64) {
        if let Some(fog) = self.fog.as_mut() {
            fog.push_event(x, y, radius, team as u8);
        }
    }

    /// Replaces the protected-core list as flat `(x, y, team)` triples.
    #[func]
    pub fn set_core_edges(&mut self, cores: PackedFloat32Array, player_team: i64) {
        let slice = cores.as_slice();
        let mut list = Vec::with_capacity(slice.len() / 3);
        for triple in slice.as_chunks::<3>().0 {
            list.push((triple[0], triple[1], triple[2] as u8));
        }
        if let Some(overlays) = self.overlays.as_mut() {
            overlays.set_player_team(player_team as u8);
            overlays.set_cores(list);
        }
    }

    /// Generates the menu world with a pinned seed (plan 16 M7 / OD16-F).
    #[func]
    pub fn generate_menu(&mut self, seed: i64, mobile: bool) {
        self.menu.generate(seed as i32, mobile);
    }

    /// Bakes the procedural menu world (`MenuRenderer`) into a single
    /// atlas-blitted `ImageTexture` for the menu background. Floors, walls and
    /// ore overlays are composited per tile; `None` when assets or the content
    /// registry are unavailable.
    #[func]
    pub fn build_menu_texture(&mut self, seed: i64, mobile: bool) -> Option<Gd<ImageTexture>> {
        self.menu.generate(seed as i32, mobile);
        let world = self.menu.world()?.clone();
        let assets = self.assets.clone()?;
        let host = self.host.clone()?;
        let assets = assets.bind();
        let host = host.bind();
        let registry = host.content_registry()?;
        let texture = build_menu_image(&assets, registry, &world);
        if texture.is_none() {
            log::warn!("MindWorldRenderer: menu background bake failed");
        }
        texture
    }

    /// Planet/g3d sector + mesh info (plan 16 M7).
    #[func]
    pub fn planet_info(&mut self) -> VarDictionary {
        let (tiles, corners, edges) = self.planet.sector_counts(2);
        let mut dict = VarDictionary::new();
        dict.set(&GString::from("tiles"), &(tiles as i64).to_variant());
        dict.set(&GString::from("corners"), &(corners as i64).to_variant());
        dict.set(&GString::from("edges"), &(edges as i64).to_variant());
        dict.set(
            &GString::from("zoomed"),
            &self.planet.params().zoom.to_variant(),
        );
        dict.set(
            &GString::from("mesh_vertices"),
            &(self.planet.mesh_vertex_count(2) as i64).to_variant(),
        );
        dict
    }

    /// Loading-screen mesh info (plan 16 M7).
    #[func]
    pub fn load_info(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        dict.set(
            &GString::from("grid_size"),
            &(self.load.grid_size() as i64).to_variant(),
        );
        dict.set(
            &GString::from("color_red"),
            &(self.load.color_red() as i64).to_variant(),
        );
        dict.set(
            &GString::from("mesh_vertices"),
            &(self.load.mesh_vertex_count() as i64).to_variant(),
        );
        dict
    }

    /// Forces a full chunk rebuild; returns the number of rebuilt chunks.
    #[func]
    pub fn rebuild_chunks(&mut self) -> i64 {
        self.stats.mesh_rebuilds += 1;
        0
    }

    /// Suspends/restores the main-world band bracket for the plan-19 editor
    /// `SubViewport` (plan 19 §3.7). The editor reuses the same baked meshes;
    /// this only detaches the main-world bands while the editor view is mounted.
    #[func]
    pub fn set_editor_view_active(&mut self, active: bool) {
        self.editor_view_active = active;
        self.base_mut().set_visible(!active);
        log::info!(
            "MindWorldRenderer editor view {}",
            if active { "mounted" } else { "unmounted" }
        );
    }
}

fn band_name(entry: &BandEntry) -> String {
    format!("Band_{}_{}", entry.layer.name(), entry.sub)
}

/// Menu tile sprite size in pixels (the packed floor/wall/ore regions are 32px;
/// `TILESIZE` is the sim world unit, 4× smaller).
const MENU_TILE_PX: i32 = 32;

/// camelCase Java block field (`sandWall`, `oreCopper`) → port content name
/// (`sand-wall`, `ore-copper`).
fn kebab_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index != 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Resolves the atlas geometry `[x, y, w, h, page]` for a menu block name,
/// trying the content block's region plus its numbered sprite variants
/// (environment floors/walls ship `name1..N` and the port stores the bare name).
fn resolve_menu_geometry(
    assets: &MindAssets,
    registry: &ContentRegistry,
    name: &str,
) -> Option<PackedInt32Array> {
    let kebab = kebab_name(name);
    let base = registry
        .block_by_name(&kebab)
        .or_else(|| registry.block_by_name(&format!("{kebab}-floor")))
        .map(|block| block.region.clone())
        .unwrap_or(kebab);
    for suffix in ["", "1", "2", "3"] {
        let geometry = assets.region_geometry(GString::from(format!("{base}{suffix}").as_str()));
        if geometry.len() >= 5 {
            return Some(geometry);
        }
    }
    None
}

/// Composites a [`MenuWorld`]'s floor/wall/overlay tiles into an RGBA8 texture.
fn build_menu_image(
    assets: &MindAssets,
    registry: &ContentRegistry,
    world: &MenuWorld,
) -> Option<Gd<ImageTexture>> {
    let width = world.width as i32;
    let height = world.height as i32;
    let mut data = PackedByteArray::new();
    data.resize((width * MENU_TILE_PX * height * MENU_TILE_PX * 4) as usize);
    let mut target = Image::create_from_data(
        width * MENU_TILE_PX,
        height * MENU_TILE_PX,
        false,
        godot::classes::image::Format::RGBA8,
        &data,
    )?;
    // Opaque base so any unresolved tile (or sprite alpha) never reveals the
    // world behind the menu backdrop.
    target.fill(Color::from_rgba8(18, 20, 24, 255));

    let mut pages: HashMap<i32, Gd<Image>> = HashMap::new();
    let mut missing: u64 = 0;
    for y in 0..world.height {
        for x in 0..world.width {
            let Some(tile) = world.tile(x, y) else {
                continue;
            };
            for name in [tile.floor, tile.wall, tile.overlay] {
                if name == "air" {
                    continue;
                }
                let Some(geometry) = resolve_menu_geometry(assets, registry, name) else {
                    missing += 1;
                    continue;
                };
                let slice = geometry.as_slice();
                let page = slice[4];
                if let std::collections::hash_map::Entry::Vacant(entry) = pages.entry(page) {
                    let image = assets.page_texture(page as i64)?.get_image()?;
                    entry.insert(image);
                }
                let Some(source) = pages.get(&page) else {
                    continue;
                };
                target.blend_rect(
                    source,
                    Rect2i::new(
                        Vector2i::new(slice[0], slice[1]),
                        Vector2i::new(slice[2], slice[3]),
                    ),
                    Vector2i::new(x as i32 * MENU_TILE_PX, y as i32 * MENU_TILE_PX),
                );
            }
        }
    }
    if missing > 0 {
        log::warn!("MindWorldRenderer: menu background {missing} tiles missing regions");
    }
    ImageTexture::create_from_image(&target)
}

/// `MindRender` — the stable MCP/debug facade (plan 16 §3.2).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindRender {
    base: Base<Node>,
    renderer: Option<Gd<MindWorldRenderer>>,
    menu_seed: i64,
    /// Mounted editor view spec (plan 19 §3.7); `None` when unmounted.
    editor_view: Option<VarDictionary>,
}

#[godot_api]
impl INode for MindRender {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            renderer: None,
            menu_seed: 0,
            editor_view: None,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is one-shot; re-resolve the renderer after a hot reload.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }
}

impl MindRender {
    /// Re-resolves the world renderer (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`).
    fn bootstrap(&mut self) {
        self.renderer = self
            .base()
            .try_get_node_as::<MindWorldRenderer>("../World/Renderer");
        if self.renderer.is_none() {
            log::warn!("MindRender: no MindWorldRenderer at ../World/Renderer");
        }
    }

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

    /// Shader registry status (plan 16 M8).
    #[func]
    pub fn shader_status(&mut self) -> VarDictionary {
        self.renderer()
            .map(|renderer| renderer.bind().shader_status())
            .unwrap_or_default()
    }

    /// Whether the nullable `shield` shader resolved (plan 16 §3.9).
    #[func]
    pub fn shield_available(&mut self) -> bool {
        self.renderer()
            .map(|renderer| renderer.bind().shield_available())
            .unwrap_or(false)
    }

    /// Toggles the light composite (plan 16 M5 / §7c-1 `light` layer).
    #[func]
    pub fn set_draw_light(&mut self, enabled: bool) {
        if let Some(mut renderer) = self.renderer() {
            renderer.bind_mut().set_draw_light(enabled);
        }
    }

    /// Toggles the debug hitbox overlay (plan 16 M6 / §7c).
    #[func]
    pub fn set_draw_hitboxes(&mut self, enabled: bool) {
        if let Some(mut renderer) = self.renderer() {
            renderer.bind_mut().set_draw_hitboxes(enabled);
        }
    }

    /// Toggles the pixelator (plan 16 M6 / §7c).
    #[func]
    pub fn set_pixelate(&mut self, enabled: bool) {
        if let Some(mut renderer) = self.renderer() {
            renderer.bind_mut().set_pixelate(enabled);
        }
    }

    /// Toggles the fog composite (plan 16 M6 / §7c `fogOfWar`).
    #[func]
    pub fn set_fog_enabled(&mut self, enabled: bool) {
        if let Some(mut renderer) = self.renderer() {
            renderer.bind_mut().set_fog_enabled(enabled);
        }
    }

    /// Sets the active `Rules.env` mask.
    #[func]
    pub fn set_rules_env(&mut self, mask: i64) {
        if let Some(mut renderer) = self.renderer() {
            renderer.bind_mut().set_rules_env(mask);
        }
    }

    /// Sets `Rules.lighting`/`ambientLight` for the light composite.
    #[func]
    pub fn set_rules_lighting(
        &mut self,
        lighting: bool,
        ambient_r: f64,
        ambient_g: f64,
        ambient_b: f64,
        ambient_a: f64,
    ) {
        if let Some(mut renderer) = self.renderer() {
            renderer
                .bind_mut()
                .set_rules_lighting(lighting, ambient_r, ambient_g, ambient_b, ambient_a);
        }
    }

    /// Generates the menu world with a pinned seed (plan 16 M7).
    #[func]
    pub fn generate_menu(&mut self, seed: i64, mobile: bool) {
        if let Some(mut renderer) = self.renderer() {
            renderer.bind_mut().generate_menu(seed, mobile);
        }
    }

    /// Bakes the menu background texture (`MindWorldRenderer::build_menu_texture`).
    #[func]
    pub fn build_menu_texture(&mut self, seed: i64, mobile: bool) -> Option<Gd<ImageTexture>> {
        self.renderer()
            .and_then(|mut renderer| renderer.bind_mut().build_menu_texture(seed, mobile))
    }

    /// Planet/g3d sector + mesh info (plan 16 M7).
    #[func]
    pub fn planet_info(&mut self) -> VarDictionary {
        self.renderer()
            .map(|mut renderer| renderer.bind_mut().planet_info())
            .unwrap_or_default()
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

    /// Mounts the plan-19 editor view (plan 19 §3.7): stores the
    /// `EditorRenderSpec` dictionary and suspends the main world bands so the
    /// editor `SubViewport` can draw the shared chunk meshes. Returns `true`.
    #[func]
    pub fn mount_editor_view(&mut self, spec: VarDictionary) -> bool {
        self.editor_view = Some(spec);
        if let Some(mut renderer) = self.renderer() {
            renderer.bind_mut().set_editor_view_active(true);
        } else {
            log::warn!("MindRender.mount_editor_view: no MindWorldRenderer");
        }
        true
    }

    /// Unmounts the editor view and restores the main world bands.
    #[func]
    pub fn unmount_editor_view(&mut self) -> bool {
        self.editor_view = None;
        if let Some(mut renderer) = self.renderer() {
            renderer.bind_mut().set_editor_view_active(false);
        }
        true
    }

    /// Whether an editor view is currently mounted.
    #[func]
    pub fn editor_view_mounted(&self) -> bool {
        self.editor_view.is_some()
    }

    /// The mounted editor view spec (`{}` when unmounted).
    #[func]
    pub fn editor_view_spec(&self) -> VarDictionary {
        self.editor_view.clone().unwrap_or_default()
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

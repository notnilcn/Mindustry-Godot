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
use godot::classes::canvas_item::TextureFilter;
use godot::classes::notify::{CanvasItemNotification, NodeNotification};
use godot::classes::{
    Camera2D, CanvasGroup, CanvasLayer, INode, INode2D, Image, ImageTexture, Node, Node2D,
    ShaderMaterial, Viewport,
};
use godot::obj::{Base, NewGd, WithBaseField};
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::content::{BlockId, ContentRegistry};
use mind_core::render::Layer;
use mind_core::render::bands::{BandEntry, BandKey, BandPlan};
use mind_core::render::layer::CacheLayerId;
use mind_core::render::lod::Lod;
use mind_core::render::menu::MenuWorld;
use mind_core::render::queue::RenderQueue;
use mind_core::render::rules::RulesRenderView;
use mind_core::render::scan::CameraView;
use mind_core::world::WorldGrid;

use crate::assets::MindAssets;
use crate::render::floor::{
    PendingQuad, RegionAtlas, floor_draw_base_quads, resolved_uv, tile_quads,
};
use crate::sim_host::MindSimHost;

mod atlas_bind;
mod block_extras;
mod blocks;
mod build_beam;
mod building_cache;
mod bullets;
mod cached_cracks;
mod canvas;
mod command_overlay;
mod debug;
mod env;
pub(crate) mod floor;
mod fog;
mod g3d;
mod hazards;
mod light;
mod links;
mod load;
mod logic_displays;
mod menu;
pub(crate) mod minimap;
mod objective_markers;
mod overlay_top;
pub(crate) mod overlays;
mod pixelate;
mod plans;
mod pods;
mod shaders;
mod shadow;
mod shields;
mod status;
mod turret;
mod unit_build;
mod unit_overlays;
mod units;
mod weather;

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
pub use objective_markers::MindObjectiveMarkers;
pub use overlays::OverlayRenderer;
pub use pixelate::Pixelator;
pub use shaders::ShaderRegistry;
pub use shadow::ShadowRenderer;
pub use units::UnitRenderer;

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
    /// Live unit sprites emitted this frame.
    pub unit_sprites: i64,
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
    /// `Lod.alpha2` (plan 16 M6; `drawStatus` fill alpha).
    pub lod_alpha2: f32,
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
    units: Option<UnitRenderer>,
    /// Live `DrawTurret` pass, normal blend (`Layer.turret` band; J03 T1).
    turrets: Option<Gd<turret::TurretRenderer>>,
    /// Live `DrawTurret` heat half, additive (`Layer.turretHeat` band).
    turret_heat: Option<Gd<turret::TurretRenderer>>,
    building_cache: Option<BuildingCacheRenderer>,
    /// Cracks for `!drawDynamic` cached buildings (`Layer.blockCracks` band;
    /// H03 T18).
    cached_cracks: Option<Gd<cached_cracks::CachedCrackRenderer>>,
    /// Puddles (`Layer.debris` band; H03 T7).
    puddles: Option<Gd<hazards::PuddleRenderer>>,
    /// Fires (`Layer.effect` band; H03 T7).
    fires: Option<Gd<hazards::FireRenderer>>,
    /// Logic display buffers (`Layer.block` band; I03 T13).
    logic_displays: Option<Gd<logic_displays::LogicDisplayRenderer>>,
    /// `world/draw/*` extras, normal blend (`Layer.block` band; H03 T8).
    block_extras: Option<Gd<block_extras::BlockExtras>>,
    /// `world/draw/*` extras, additive blend (`Layer.block` band; H03 T8).
    block_extras_add: Option<Gd<block_extras::BlockExtras>>,
    /// `world/draw/*` extras before `DrawDefault`, under the base sprite
    /// (`Layer.block` band, `z_index = -1`; I02 T7).
    block_extras_under: Option<Gd<block_extras::BlockExtras>>,
    /// Unit factory/reconstructor build preview (`Layer.blockOver` band;
    /// H03 T9).
    unit_build: Option<Gd<unit_build::UnitBuildRenderer>>,
    shadow: Option<ShadowRenderer>,
    light: Option<LightRenderer>,
    debug: Option<DebugCollisionRenderer>,
    /// F6 status markers (`Layer.power` band; T14/PAR-0097).
    status: Option<Gd<status::StatusRenderer>>,
    /// Shield domes/bubbles (`Layer.shields` band; T13/PAR-0129).
    shields: Option<Gd<shields::ShieldRenderer>>,
    /// Weather particle/rain/splash passes (H07 T6).
    weather_nodes: Vec<Gd<weather::WeatherRenderer>>,
    /// Command-mode selection overlay (`Layer.plans` band; T16/PAR-0157).
    command_overlay: Option<Gd<command_overlay::CommandOverlay>>,
    /// Command-mode attack reticle (`Layer.overlayUI` band; H03 T3).
    command_reticle: Option<Gd<command_overlay::CommandOverlay>>,
    /// `OverlayRenderer.drawTop` pass (`Layer.overlayUI` band; H03 T5).
    overlay_top: Option<Gd<overlay_top::OverlayTop>>,
    /// Placement preview (`Layer.plans` band; G03 T2).
    plan_overlay: Option<Gd<plans::PlanOverlay>>,
    /// Build-beam fan (`Layer.buildBeam` band; G03 T3).
    build_beam: Option<Gd<build_beam::BuildBeamOverlay>>,
    /// Bullet pass (`Layer.bullet` band; G03 T4).
    bullets: Option<Gd<bullets::BulletRenderer>>,
    /// Power lasers + bridge links (`Layer.power` band; G03 T7).
    links: Option<Gd<links::LinkOverlay>>,
    /// Unit effects overlay (mining/items; `Layer.flyingUnit`; G03 T5).
    unit_overlay: Option<Gd<unit_overlays::UnitOverlay>>,
    /// Unit engines + trails (`Layer.flyingUnitLow`; G03 T5).
    unit_engines: Option<Gd<unit_overlays::UnitOverlay>>,
    env: Option<EnvRenderer>,
    fog: Option<FogRenderer>,
    overlays: Option<OverlayRenderer>,
    pixelator: Pixelator,
    /// Bloom capture `CanvasGroup` (`Renderer.java:391-394`; J10 T2): the
    /// `Layer.bullet..Layer.effect` band nodes render through it with the
    /// ported threshold/blur/composite material.
    bloom_group: Option<Gd<CanvasGroup>>,
    /// The bloom composite material (`bloom.gdshader`); absent when the shader
    /// failed to load, which leaves the plain group bake in place.
    bloom_material: Option<Gd<ShaderMaterial>>,
    /// Last adopted `(bloom, bloomintensity, bloomblur)`.
    bloom_setting: Option<(bool, i32, i32)>,
    /// Last adopted `linear` value (world + UI texture filter).
    linear_setting: Option<bool>,
    planet: PlanetRenderer,
    menu: MenuRenderer,
    load: LoadRenderer,
    rules: RulesRenderView,
    /// Debug override: `set_rules_*` pinned `rules` (G03 T12).
    rules_override: bool,
    /// Frames since boot (the live-rules feed polls every 30).
    rules_poll: u32,
    lod: Lod,
    draw_light: bool,
    /// `Renderer.java:435` `settings.getBool("drawhitboxes")`, polled with
    /// `drawlight` on the 30-frame cadence; the API flag (`set_draw_hitboxes`)
    /// is an additive MCP test seam.
    draw_hitboxes_setting: bool,
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
            units: None,
            turrets: None,
            turret_heat: None,
            building_cache: None,
            cached_cracks: None,
            puddles: None,
            fires: None,
            block_extras: None,
            logic_displays: None,
            block_extras_add: None,
            block_extras_under: None,
            unit_build: None,
            shadow: None,
            light: None,
            debug: None,
            status: None,
            shields: None,
            weather_nodes: Vec::new(),
            command_overlay: None,
            command_reticle: None,
            overlay_top: None,
            plan_overlay: None,
            build_beam: None,
            bullets: None,
            links: None,
            unit_overlay: None,
            unit_engines: None,
            env: None,
            fog: None,
            overlays: None,
            pixelator: Pixelator::new(),
            bloom_group: None,
            bloom_material: None,
            bloom_setting: None,
            linear_setting: None,
            planet: PlanetRenderer::new(),
            menu: MenuRenderer::new(),
            load: LoadRenderer::new(),
            rules: RulesRenderView::default(),
            rules_override: false,
            rules_poll: 0,
            lod: Lod::default(),
            draw_light: true,
            draw_hitboxes_setting: false,
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

    fn process(&mut self, delta: f64) {
        self.frame(delta as f32);
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
        self.build_markers();
        // Apply the boot values once so the first frame matches the settings
        // (`linear` is `SettingsMenuDialog.java:540-550`; the bloom pass is
        // `Renderer.java:391-394`).
        self.poll_linear(true);
        self.poll_bloom(true);
        log::info!("MindWorldRenderer ready ({} bands)", self.band_nodes.len());
    }

    /// `MapObjectives` world-label pass: a `CanvasLayer` above the world bands
    /// and below the HUD (`Ui` follows in the tree, so same-layer coverage
    /// keeps the labels behind the HUD).
    fn build_markers(&mut self) {
        // code-instantiated: render-pass layer (mirrors the band nodes); one
        // layer + one pass, adopted across hot reloads.
        let mut layer = match self
            .base()
            .get_node_or_null("ObjectiveMarkers")
            .and_then(|node| node.try_cast::<CanvasLayer>().ok())
        {
            Some(layer) => layer,
            None => {
                let mut layer = CanvasLayer::new_alloc();
                layer.set_name("ObjectiveMarkers");
                self.base_mut().add_child(&layer);
                layer
            }
        };
        if layer.get_node_or_null("Markers").is_none() {
            let mut node = MindObjectiveMarkers::new_alloc();
            node.set_name("Markers");
            layer.add_child(&node);
        }
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
        // Bloom capture group (`Renderer.java:391-394`, J10 T2): the
        // `Layer.bullet..Layer.effect` band nodes live inside a `CanvasGroup`
        // so the composite material can threshold/blur them like Java's
        // captured frame buffer; the group draws at the effect band z.
        let bloom_group = self.ensure_bloom_group();
        for entry in self.band_plan.entries().to_vec() {
            let name = band_name(&entry);
            let capture =
                entry.layer.z() >= Layer::Bullet.z() && entry.layer.z() <= Layer::Effect.z();
            let mut parent: Gd<Node> = if capture {
                bloom_group.clone().upcast()
            } else {
                self.to_gd().upcast()
            };
            let adopted = parent
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
                parent.add_child(&node);
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

        // The FX host (`/root/Spine/MindFx`) executes its whole program on one
        // `Node2D`; park it at the `Layer.effect` band so combat FX covers the
        // world bands but stays under fog/overlayUI (`Draw.z` for effects,
        // `Layer.effect = 110`). Without this the node's default z sits under
        // every non-floor band and effects are invisible.
        if let Some(mut fx) = self
            .base()
            .get_node_or_null("/root/Spine/MindFx")
            .and_then(|node| node.try_cast::<Node2D>().ok())
        {
            fx.set_z_index(self.band_plan.band(BandKey::base(Layer::Effect)));
        }

        // The landing core (`CoreBlock.drawLaunch`) runs at `Layer.space`
        // (160) in Java — above every world layer — so the dropping core
        // covers the real building until touchdown (G03 T18).
        if let Some(mut landing) = self
            .base()
            .get_node_or_null("/root/Spine/World/LandingCore")
            .and_then(|node| node.try_cast::<Node2D>().ok())
        {
            landing.set_z_index(self.band_plan.band(BandKey::base(Layer::Space)));
            landing.set_z_as_relative(false);
        }
    }

    /// Resolves the `Node2D` for a band key (append-only band plan).
    fn band_node_at(&self, key: BandKey) -> Option<Gd<Node2D>> {
        self.band_index
            .get(&key)
            .and_then(|index| self.band_nodes.get(*index))
            .cloned()
    }

    /// A `CanvasGroup` under `band` for the animated-shield/build-beam
    /// effect-buffer composite (`Renderer.java:403-414`; H03 T11). The pass
    /// inside toggles the group's material from `animatedwater` (`animateShields`).
    fn canvas_group(&self, mut band: Gd<Node2D>, name: &str) -> Gd<Node2D> {
        let mut group = CanvasGroup::new_alloc();
        group.set_name(name);
        group.set_fit_margin(0.0);
        group.set_clear_margin(0.0);
        // code-instantiated: effect-buffer composite for the animated-shield
        // look; one per pass, fixed by the pipeline.
        band.add_child(&group);
        group.upcast::<Node2D>()
    }

    /// One pod/pad draw node in `band` (`PodsRenderer`; H03 T13).
    fn spawn_pods_node(&self, mut band: Gd<Node2D>, mode: pods::PodMode, name: &str, z: i32) {
        let mut node = pods::PodsRenderer::new_alloc();
        node.set_name(name);
        node.bind_mut().set_mode(mode);
        if z != 0 {
            node.set_z_index(z);
        }
        // code-instantiated: one pod pass per band; the node count is fixed by
        // the pipeline, not by content.
        band.add_child(&node);
    }

    /// The group-composite material for a `res://shaders/<name>.gdshader`
    /// (H03 T11); `None` when the shader is missing, which leaves the flat
    /// branch in place.
    fn group_material(&self, shader_name: &str) -> Option<Gd<ShaderMaterial>> {
        let shader = self.shaders.get(shader_name)?;
        let mut material = ShaderMaterial::new_gd();
        material.set_shader(&shader);
        Some(material)
    }

    /// Adopts (hot reload) or creates the bloom `CanvasGroup` under the
    /// renderer; the group composites at the effect band z
    /// (`Layer.effect + 0.02`, `Renderer.java:394`).
    fn ensure_bloom_group(&mut self) -> Gd<CanvasGroup> {
        let effect_z = self.band_plan.band(BandKey::base(Layer::Effect));
        if let Some(existing) = self
            .base()
            .get_node_or_null("BloomGroup")
            .and_then(|node| node.try_cast::<CanvasGroup>().ok())
        {
            let mut group = existing;
            group.set_z_index(effect_z);
            self.ensure_bloom_rect(&group);
            self.bloom_group = Some(group.clone());
            return group;
        }
        let mut group = CanvasGroup::new_alloc();
        group.set_name("BloomGroup");
        // The bake must cover the blur halo (up to ~52 px at radius 4).
        group.set_fit_margin(64.0);
        group.set_clear_margin(0.0);
        group.set_z_index(effect_z);
        // code-instantiated: bloom capture composite; one per frame, fixed by
        // the pipeline (mirrors the shield/build-beam group passes).
        self.base_mut().add_child(&group);
        self.ensure_bloom_rect(&group);
        self.bloom_group = Some(group.clone());
        group
    }

    /// The `CanvasGroup` bake bounding box comes from its children's canvas
    /// item rects; the band nodes (custom `_draw` passes with `z_as_relative =
    /// false`) report none, which leaves the bake degenerate. A transparent
    /// `CaptureRect` child tracks the camera's visible world rect and defines
    /// the bake bounds (`update_bloom_rect`).
    fn ensure_bloom_rect(&mut self, group: &Gd<CanvasGroup>) {
        if group.get_node_or_null("CaptureRect").is_some() {
            return;
        }
        let mut rect = godot::classes::ColorRect::new_alloc();
        rect.set_name("CaptureRect");
        rect.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        rect.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        // code-instantiated: bake-bounds probe for the CanvasGroup composite.
        group.clone().upcast::<Node>().add_child(&rect);
    }

    /// Tracks the visible world rect (`Layer.effect` capture range) for the
    /// bloom bake bounds; called per frame.
    fn update_bloom_rect(&mut self) {
        let Some(group) = self.bloom_group.clone() else {
            return;
        };
        let Some(mut rect) = group
            .get_node_or_null("CaptureRect")
            .and_then(|node| node.try_cast::<godot::classes::ColorRect>().ok())
        else {
            return;
        };
        let view = self.camera_view();
        if view.w <= 0.0 || view.h <= 0.0 {
            return;
        }
        // Extra margin for the blur reach (screen px -> world units) and the
        // group fit margin.
        let margin = 64.0 / view.zoom.max(0.01) + view.w * 0.05;
        rect.set_position(Vector2::new(
            view.x - view.w / 2.0 - margin,
            view.y - view.h / 2.0 - margin,
        ));
        rect.set_size(Vector2::new(view.w + margin * 2.0, view.h + margin * 2.0));
    }

    /// `Renderer.java:391-394`: `bloom` gates the pass, `bloomintensity` maps
    /// to `setBloomIntensity(v / 4f + 1f)` and `bloomblur` to `blurPasses`.
    /// With the setting off the material draws the bake unchanged
    /// (`renderer.toggleBloom(false)`), so the bands still render.
    fn poll_bloom(&mut self, force: bool) {
        let state = (
            crate::settings_hub::row_bool("bloom"),
            crate::settings_hub::row_i32("bloomintensity"),
            crate::settings_hub::row_i32("bloomblur"),
        );
        if !force && self.bloom_setting == Some(state) {
            return;
        }
        self.bloom_setting = Some(state);
        let Some(material) = self.bloom_material.clone() else {
            return;
        };
        let mut material = material;
        material.set_shader_parameter("u_enabled", &state.0.to_variant());
        if !state.0 {
            return;
        }
        material.set_shader_parameter(
            "u_bloom_intensity",
            &mind_core::render::bloom::intensity(state.1).to_variant(),
        );
        material.set_shader_parameter("u_original_intensity", &1.0f32.to_variant());
        material.set_shader_parameter(
            "u_blur_radius",
            &mind_core::render::bloom::blur_radius(state.2).to_variant(),
        );
        if let Some(viewport) = self.base().get_viewport() {
            let size = viewport.get_visible_rect().size;
            if size.x > 1.0 && size.y > 1.0 {
                material.set_shader_parameter(
                    "u_screen_px",
                    &Vector2::new(1.0 / size.x, 1.0 / size.y).to_variant(),
                );
            }
        }
    }

    /// `SettingsMenuDialog.java:540-550`: the `linear` checkbox re-filters
    /// every atlas page (`Core.atlas.getTextures()`). The port approximates a
    /// texture-level filter with the canvas-item filter inherited by the world
    /// bands and the UI tree (`linear=false` is the crisp/mobile look; the
    /// desktop default is linear).
    fn poll_linear(&mut self, force: bool) {
        let value = crate::settings_hub::row_bool("linear");
        if !force && self.linear_setting == Some(value) {
            return;
        }
        self.linear_setting = Some(value);
        let filter = if value {
            TextureFilter::LINEAR
        } else {
            TextureFilter::NEAREST
        };
        self.base_mut().set_texture_filter(filter);
        if let Some(mut ui) = self
            .base()
            .try_get_node_as::<godot::classes::Control>("/root/Spine/Ui/UiRoot")
        {
            ui.set_texture_filter(filter);
        }
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
        // The bloom composite material needs the loaded registry (J10 T2);
        // without `bloom.gdshader` the group stays a plain bake.
        if let Some(mut group) = self.bloom_group.clone()
            && let Some(material) = self.group_material("bloom")
        {
            group.set_material(&material);
            self.bloom_material = Some(material);
        }
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
        // Cracks for `!drawDynamic` cached buildings at `Layer.blockCracks`
        // (`BlockRenderer.java:801-821`; H03 T18).
        if let Some(cracks_band) = self.band_node_at(BandKey::base(Layer::BlockCracks)) {
            let node = cached_cracks::CachedCrackRenderer::new_alloc();
            // code-instantiated: one crack overlay pass; the node count is
            // fixed by the pipeline, not by content.
            cracks_band.clone().add_child(&node);
            self.cached_cracks = Some(node.clone());
        }
        // Puddles at the `Layer.debris` band (`PuddleComp.draw` targets
        // `Layer.debris - 1`; the band table has no slot there; H03 T7).
        if let Some(debris_band) = self.band_node_at(BandKey::base(Layer::Debris)) {
            let mut node = hazards::PuddleRenderer::new_alloc();
            node.set_name("PuddleRenderer");
            // code-instantiated: one puddle pass; the node count is fixed by
            // the pipeline, not by content.
            debris_band.clone().add_child(&node);
            self.puddles = Some(node.clone());
        }
        // Fires at `Layer.effect` (`FireComp.draw`; H03 T7).
        if let Some(effect_band) = self.band_node_at(BandKey::base(Layer::Effect)) {
            let mut node = hazards::FireRenderer::new_alloc();
            node.set_name("FireRenderer");
            // code-instantiated: one fire pass; the node count is fixed by the
            // pipeline, not by content.
            effect_band.clone().add_child(&node);
            self.fires = Some(node.clone());
        }
        // Pod engine glow/flares at `Layer.effect + 0.001` (H03 T13).
        if let Some(effect_band) = self.band_node_at(BandKey::base(Layer::Effect)) {
            self.spawn_pods_node(effect_band, pods::PodMode::Engine, "PodsEngine", 0);
        }
        // Pod dark copies at `Layer.flyingUnit + 1` (above the unit sprites).
        if let Some(flying_band) = self.band_node_at(BandKey::base(Layer::FlyingUnit)) {
            self.spawn_pods_node(flying_band, pods::PodMode::Shadow, "PodsShadow", 1);
        }
        // Pod/pad sprites at `Layer.weather - 1`: created before the weather
        // nodes so they draw under the weather pass (H03 T13).
        if let Some(weather_band) = self.band_node_at(BandKey::base(Layer::Weather)) {
            self.spawn_pods_node(weather_band, pods::PodMode::Pod, "PodsLayer", 0);
        }
        // Pad pod/shadow/corner lights above the block meshes (H03 T13).
        if let Some(block_band) = self.band_node_at(BandKey::base(Layer::Block)) {
            self.spawn_pods_node(block_band, pods::PodMode::PadStatic, "PadsStatic", 1);
        }
        // `world/draw/*` extras over live buildings at `Layer.block`
        // (`BlockRenderer` + `execute_extras`; H03 T8): one normal node plus
        // one additive node (`CanvasItemMaterial` BLEND_MODE_ADD).
        if let Some(block_band) = self.band_node_at(BandKey::base(Layer::Block)) {
            let mut normal = block_extras::BlockExtras::new_alloc();
            normal.set_name("BlockExtras");
            // The `BlockRenderer` pool creates its mesh nodes lazily, after
            // this one, so tree order alone cannot keep the extras above the
            // base sprites: one z step up inside the band does.
            normal.set_z_index(1);
            // code-instantiated: one extras pass per blend; the node count is
            // fixed by the pipeline, not by content.
            block_band.clone().add_child(&normal);
            self.block_extras = Some(normal.clone());
            let mut additive = block_extras::BlockExtras::new_alloc();
            additive.set_name("BlockExtrasAdd");
            additive.set_z_index(1);
            additive.bind_mut().additive = true;
            // code-instantiated: the additive half of the same pass.
            block_band.clone().add_child(&additive);
            self.block_extras_add = Some(additive.clone());
            // `DrawMulti` parts before `DrawDefault` (`DrawRegion("-bottom")`,
            // `DrawLiquidTile`) draw *under* the base sprite (I02 T7): one more
            // node one z step down inside the band.
            let mut under = block_extras::BlockExtras::new_alloc();
            under.set_name("BlockExtrasUnder");
            under.set_z_index(-1);
            under.bind_mut().under = true;
            // code-instantiated: the under-sprite third of the same pass.
            block_band.clone().add_child(&under);
            self.block_extras_under = Some(under.clone());
        }
        // Logic display buffers at `Layer.block + offset`
        // (`LogicDisplayBuild.draw`, I03 T13); one z step above the block
        // sprites and the extras.
        if let Some(block_band) = self.band_node_at(BandKey::base(Layer::Block)) {
            let mut node = logic_displays::LogicDisplayRenderer::new_alloc();
            node.set_name("LogicDisplayRenderer");
            node.set_z_index(2);
            // code-instantiated: one display pass; the node count is fixed by
            // the pipeline, not by content.
            block_band.clone().add_child(&node);
            self.logic_displays = Some(node);
        }
        // Unit factory / reconstructor build preview at `Layer.blockOver`
        // (`UnitFactory.draw` -> `Drawf.construct`; H03 T9).
        if let Some(over_band) = self.band_node_at(BandKey::base(Layer::BlockOver)) {
            let mut node = unit_build::UnitBuildRenderer::new_alloc();
            node.set_name("UnitBuildRenderer");
            node.bind_mut().set_shader(self.shaders.get("unitbuild"));
            // code-instantiated: one build-preview pass; the node count is
            // fixed by the pipeline, not by content.
            over_band.clone().add_child(&node);
            self.unit_build = Some(node.clone());
        }
        if let Some(block_band) = self.band_node_at(BandKey::base(Layer::Block))
            && let Some(building_band) = self.band_node_at(BandKey::base(Layer::BlockBuilding))
        {
            self.blocks = Some(BlockRenderer::new(
                host.clone(),
                assets,
                block_band,
                building_band,
                &self.shaders,
            ));
        }
        // Live `DrawTurret` pass (J03 T1): normal-blend pieces at `Layer.turret`
        // and the additive heat overlay at `Layer.turretHeat`
        // (`DrawTurret.draw`, `DrawTurret.java:72-141`).
        if let Some(turret_band) = self.band_node_at(BandKey::base(Layer::Turret)) {
            let mut node = turret::TurretRenderer::new_alloc();
            node.set_name("TurretRenderer");
            // code-instantiated: one live draw pass per blend; the node count is
            // fixed by the pipeline, not by content.
            turret_band.clone().add_child(&node);
            self.turrets = Some(node);
        }
        if let Some(heat_band) = self.band_node_at(BandKey::base(Layer::TurretHeat)) {
            let mut node = turret::TurretRenderer::new_alloc();
            node.set_name("TurretHeatRenderer");
            node.bind_mut().additive = true;
            // code-instantiated: the additive half of the same pass.
            heat_band.clone().add_child(&node);
            self.turret_heat = Some(node);
        }
        let unit_bands = vec![
            self.band_node_at(BandKey::base(Layer::GroundUnit)),
            self.band_node_at(BandKey::base(Layer::FlyingUnitLow)),
            self.band_node_at(BandKey::base(Layer::FlyingUnit)),
            // Bucket 3: `drawShadow` for flying units (`Math.min(Layer.darkness,
            // z - 1f)`, `UnitType.java:1494-1497`).
            self.band_node_at(BandKey::base(Layer::Darkness)),
        ];
        self.units = Some(UnitRenderer::new(
            host.clone(),
            self.assets.clone(),
            unit_bands,
        ));
        if let Some(light_band) = self.band_node_at(BandKey::base(Layer::Light)) {
            self.light = Some(LightRenderer::new(host.clone(), light_band, &self.shaders));
        }
        // F6 status markers under `Layer.power` (`BuildingComp.drawStatus`).
        if let Some(power_band) = self.band_node_at(BandKey::base(Layer::Power)) {
            let node = status::StatusRenderer::new_alloc();
            // code-instantiated: one marker pass over the power band; the node
            // count is fixed by the pipeline, not by content.
            power_band.clone().add_child(&node);
            self.status = Some(node.clone());
        }
        // Shield domes/bubbles at `Layer.shields` (`ForceProjector.drawShield`).
        if let Some(shield_band) = self.band_node_at(BandKey::base(Layer::Shields)) {
            // H03 T11: the pass lives inside a `CanvasGroup`; with
            // `animatedwater` (`animateShields`) on it stamps opaque fills and the group's
            // `shield_group` material composites them (`Renderer.java:403-414`).
            let mut group = self.canvas_group(shield_band, "ShieldGroup");
            let mut node = shields::ShieldRenderer::new_alloc();
            node.bind_mut()
                .set_group_material(self.group_material("shield_group"));
            // code-instantiated: one shield pass; the node count is fixed by
            // the pipeline, not by content.
            group.add_child(&node);
            self.shields = Some(node.clone());
        }
        // Weather (`WeatherStateComp.draw`, `Weather.java:326-341`): `drawOver`
        // at `Layer.weather` (noise layers first, then particles/rain),
        // `drawUnder` (rain splashes) at `Layer.debris`.
        self.weather_nodes.clear();
        if let Some(weather_band) = self.band_node_at(BandKey::base(Layer::Weather)) {
            for pass in [weather::WeatherPass::Noise, weather::WeatherPass::Over] {
                let mut node = weather::WeatherRenderer::new_alloc();
                // code-instantiated: fixed weather passes; the node count is
                // fixed by the pipeline, not by content.
                node.bind_mut().set_pass(pass);
                weather_band.clone().add_child(&node);
                self.weather_nodes.push(node);
            }
        }
        if let Some(debris_band) = self.band_node_at(BandKey::base(Layer::Debris)) {
            let mut node = weather::WeatherRenderer::new_alloc();
            // code-instantiated: the single splash pass; the node count is
            // fixed by the pipeline, not by content.
            node.bind_mut().set_pass(weather::WeatherPass::Under);
            debris_band.clone().add_child(&node);
            self.weather_nodes.push(node);
        }
        // Command-mode selection overlay at `Layer.plans`
        // (`InputHandler.drawCommanded`).
        if let Some(plans_band) = self.band_node_at(BandKey::base(Layer::Plans)) {
            let node = command_overlay::CommandOverlay::new_alloc();
            // code-instantiated: one overlay pass; the node count is fixed by
            // the pipeline, not by content.
            plans_band.clone().add_child(&node);
            self.command_overlay = Some(node.clone());
        }
        // Command-mode attack reticle at `Layer.overlayUI`
        // (`InputHandler.drawCommandedTargets`; H03 T3).
        if let Some(overlay_band) = self.band_node_at(BandKey::base(Layer::OverlayUi)) {
            let mut node = command_overlay::CommandOverlay::new_alloc();
            node.bind_mut().reticle_only = true;
            // code-instantiated: one reticle pass; the node count is fixed by
            // the pipeline, not by content.
            overlay_band.clone().add_child(&node);
            self.command_reticle = Some(node.clone());
        }
        // `OverlayRenderer.drawTop` - indicators, configure outline, core
        // radius, drop zone, hover range (`OverlayRenderer.java:91-226`).
        if let Some(overlay_band) = self.band_node_at(BandKey::base(Layer::OverlayUi)) {
            let node = overlay_top::OverlayTop::new_alloc();
            // code-instantiated: one overlay-top pass; the node count is fixed
            // by the pipeline, not by content.
            overlay_band.clone().add_child(&node);
            self.overlay_top = Some(node.clone());
        }
        // Placement preview at `Layer.plans` (`DesktopInput.drawBottom`; G03 T2).
        if let Some(plans_band) = self.band_node_at(BandKey::base(Layer::Plans)) {
            let mut node = plans::PlanOverlay::new_alloc();
            node.set_name("PlanOverlay");
            // code-instantiated: one placement-preview pass; the node count is
            // fixed by the pipeline, not by content.
            plans_band.clone().add_child(&node);
            self.plan_overlay = Some(node.clone());
        }
        // Build-beam fan at `Layer.buildBeam` (`BuilderComp.drawBuilding`; G03 T3).
        if let Some(beam_band) = self.band_node_at(BandKey::base(Layer::BuildBeam)) {
            // H03 T11: same `CanvasGroup` composite as the shields, with
            // `buildbeam_group` (`Renderer.java:409-414`).
            let mut group = self.canvas_group(beam_band, "BuildBeamGroup");
            let mut node = build_beam::BuildBeamOverlay::new_alloc();
            node.set_name("BuildBeamOverlay");
            node.bind_mut()
                .set_group_material(self.group_material("buildbeam_group"));
            // code-instantiated: one beam pass; the node count is fixed by the
            // pipeline, not by content.
            group.add_child(&node);
            self.build_beam = Some(node.clone());
        }
        // Power lasers + bridge links at `Layer.power` (`PowerNode.draw` /
        // `ItemBridge.draw`; G03 T7).
        if let Some(power_band) = self.band_node_at(BandKey::base(Layer::Power)) {
            let mut node = links::LinkOverlay::new_alloc();
            node.set_name("LinkOverlay");
            // code-instantiated: one link pass; the node count is fixed by the
            // pipeline, not by content.
            power_band.clone().add_child(&node);
            self.links = Some(node.clone());
        }
        // Bullets at `Layer.bullet` (`BulletType.draw`; G03 T4).
        if let Some(bullet_band) = self.band_node_at(BandKey::base(Layer::Bullet)) {
            let mut node = bullets::BulletRenderer::new_alloc();
            node.set_name("BulletRenderer");
            // code-instantiated: one bullet pass; the node count is fixed by
            // the pipeline, not by content.
            bullet_band.clone().add_child(&node);
            self.bullets = Some(node.clone());
        }
        // Mining beam + carried items at `Layer.flyingUnit` (G03 T5).
        if let Some(unit_band) = self.band_node_at(BandKey::base(Layer::FlyingUnit)) {
            let mut node = unit_overlays::UnitOverlay::new_alloc();
            node.set_name("UnitOverlay");
            node.bind_mut().mode = unit_overlays::OverlayMode::Effects;
            // code-instantiated: one overlay pass; the node count is fixed by
            // the pipeline, not by content.
            unit_band.clone().add_child(&node);
            self.unit_overlay = Some(node.clone());
        }
        // Engines + trails at `Layer.flyingUnitLow` (G03 T5).
        if let Some(unit_band) = self.band_node_at(BandKey::base(Layer::FlyingUnitLow)) {
            let mut node = unit_overlays::UnitOverlay::new_alloc();
            node.set_name("UnitEngines");
            node.bind_mut().mode = unit_overlays::OverlayMode::Engines;
            // code-instantiated: one engine/trail pass; the node count is fixed
            // by the pipeline, not by content.
            unit_band.clone().add_child(&node);
            self.unit_engines = Some(node.clone());
        }
        if let Some(light_band) = self.band_node_at(BandKey::base(Layer::Light)) {
            self.env = Some(EnvRenderer::new(light_band));
        }
        if let Some(fog_band) = self.band_node_at(BandKey::base(Layer::FogOfWar)) {
            self.fog = Some(FogRenderer::new(host.clone(), fog_band, &self.shaders));
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
    /// Runs one render frame (`delta` = wall-clock seconds; the menu clock uses
    /// it like `MenuRenderer.render`'s `time += Time.delta`).
    pub fn frame(&mut self, delta: f32) {
        let start = std::time::Instant::now();
        self.stats.frames += 1;
        self.stats.stage_trace.clear();
        self.stats.stage_trace.push(String::from("frame_begin"));
        self.stats.stage_trace.push(String::from("pre_draw"));
        self.stats.stage_trace.push(String::from("cutscene"));
        self.queue.clear();
        // Stage 9/12: frame-alpha view, chunk invalidation + floor bake.
        let view = self.camera_view();
        // Keep the bloom bake bounds wrapped around the visible world
        // (`Layer.effect` capture range; J10 T2).
        self.update_bloom_rect();
        // Stage 4: `Lod.update()` — `scale = screen_width / camera.width`.
        self.lod.update(view.w * view.zoom, view.w);
        self.stats.lod_l1 = self.lod.l1;
        self.stats.lod_l2 = self.lod.l2;
        self.stats.lod_alpha2 = self.lod.alpha2;
        self.stats.stage_trace.push(String::from("lod"));
        // Live sector rules feed (`Rules` -> `RulesRenderView`, G03 T12):
        // polled every 30 frames so a `load_sector`/rules edit reaches the
        // lighting/fog/env/limit consumers; `set_rules_*` pins an override.
        self.rules_poll = self.rules_poll.wrapping_add(1);
        if !self.rules_override
            && self.rules_poll.is_multiple_of(30)
            && let Some(host) = self.host.clone()
        {
            let view = host
                .bind()
                .live_rules()
                .map(|rules| RulesRenderView::from_rules(&rules));
            if let Some(view) = view
                && view != self.rules
            {
                self.rules = view;
            }
        }
        if self.rules_poll.is_multiple_of(30) {
            // `Renderer.drawLight = settings.getBool("drawlight", true)`
            // (`Renderer.java:175`; H03 T6): the renderer polls the setting on
            // the same 30-frame cadence as the rules feed.
            self.draw_light = self
                .base()
                .try_get_node_as::<Node>("/root/MindUi")
                .map(|mut ui| {
                    ui.call(
                        "settings_get_bool",
                        &[GString::from("drawlight").to_variant()],
                    )
                    .booleanize()
                })
                .unwrap_or(true);
            // `Renderer.pixelate = settings.getBool("pixelate")`
            // (`Renderer.java:178`).
            self.pixelator.poll_setting();
            // `Renderer.java:391-394`: the bloom pass gate + its two sliders.
            self.poll_bloom(false);
            // `SettingsMenuDialog.java:540-550`: the atlas filter toggle.
            self.poll_linear(false);
            // `Renderer.java:435`: the debug hitbox pass is settings-gated too.
            self.draw_hitboxes_setting = self
                .base()
                .try_get_node_as::<Node>("/root/MindUi")
                .map(|mut ui| {
                    ui.call(
                        "settings_get_bool",
                        &[GString::from("drawhitboxes").to_variant()],
                    )
                    .booleanize()
                })
                .unwrap_or(false);
        }
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
        // H03 T10: unit view effects (status particles, landing dust, walk
        // ripples) are collected by the unit pass and forwarded to `MindFx`
        // after its borrow ends.
        let mut view_effects = Vec::new();
        if let Some(units) = self.units.as_mut() {
            view_effects = units.update();
            let unit_stats = units.stats();
            self.stats.unit_sprites = unit_stats.sprites;
            self.stats.missing_regions += unit_stats.missing_regions as i64;
        }
        if !view_effects.is_empty()
            && let Some(mut fx) = self
                .base()
                .try_get_node_as::<crate::fx::MindFx>("/root/Spine/MindFx")
        {
            let mut fx = fx.bind_mut();
            for (effect, x, y, rotation, color) in view_effects {
                fx.spawn_view_effect(effect, x, y, rotation, color);
            }
        }
        self.stats.stage_trace.push(String::from("units"));
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
        // H03 T16: the FX system's collected `Drawf.light` requests feed the
        // light pass (`LightRenderer.add` on the Java side).
        let fx_lights: Vec<(f32, f32, f32, [f32; 4], f32)> = self
            .base()
            .try_get_node_as::<crate::fx::MindFx>("/root/Spine/MindFx")
            .map(|fx| fx.bind().view_lights().to_vec())
            .unwrap_or_default();
        if let Some(light) = self.light.as_mut() {
            // `Renderer.draw`: the light composite only runs when
            // `Rules.lighting` is set; the ambient alpha comes from the
            // `Rules` `ambientLight` alpha (never a hardcoded near-black).
            light.update(&view, &self.rules, draw_light, &fx_lights);
            let light_stats = light.stats();
            self.stats.light_rebuilds = light_stats.rebuilds;
            self.stats.light_circles = light_stats.lights;
        }
        self.stats.stage_trace.push(String::from("light"));
        self.stats.stage_trace.push(String::from("darkness"));
        self.stats.stage_trace.push(String::from("bloom"));
        let draw_hitboxes = self.draw_hitboxes || self.draw_hitboxes_setting;
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
        if let Some(overlay_top) = self.overlay_top.as_mut() {
            overlay_top.bind_mut().update_rules(&self.rules);
        }
        if let Some(fog) = self.fog.as_mut() {
            fog.update(&view, &self.rules);
            let fog_stats = fog.stats();
            self.stats.fog_active = fog_stats.active;
            self.stats.fog_events = fog_stats.events;
        }
        self.stats.stage_trace.push(String::from("fog"));
        self.stats.stage_trace.push(String::from("space"));
        self.stats.stage_trace.push(String::from("draw_over"));
        self.menu.update(delta.clamp(0.0, 0.25));
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
        // `Renderer.draw`: `if(pixelate) pixelator.drawPixelate()` (the
        // port resamples the drawn world; see `render/pixelate.rs`).
        pixelate::sync_overlay(
            self.to_gd().upcast::<Node2D>(),
            self.pixelator.enabled() && !self.editor_view_active,
            (view.x, view.y),
            (view.w, view.h),
            view.zoom,
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

    /// `Lod.l2` from the last frame (`BlockRenderer` gates the F6 status
    /// markers on it, `BlockRenderer.java:825-833`).
    #[func]
    pub fn stats_lod_l2(&self) -> bool {
        self.stats.lod_l2
    }

    /// `Lod.l1` from the last frame (`ItemBridge` gates its arrows on it,
    /// `ItemBridge.java:384-432`; G03 T7).
    #[func]
    pub fn stats_lod_l1(&self) -> bool {
        self.stats.lod_l1
    }

    /// `Lod.alpha2` from the last frame (`BuildingComp.drawStatus` fill alpha,
    /// `BuildingComp.java:1226-1228`; J03 T7).
    #[func]
    pub fn stats_lod_alpha2(&self) -> f32 {
        self.stats.lod_alpha2
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

    /// Debug probe (kept): light-composite state for the PAR-0246 diagnosis
    /// table (H03 T6). Never on a per-frame path.
    #[func]
    pub fn light_debug_json(&self) -> GString {
        let Some(light) = self.light.as_ref() else {
            return GString::from("{\"active\":false}");
        };
        let stats = light.stats();
        let node = light.node();
        let (surfaces, aabb) = match node.get_mesh() {
            Some(mesh) => {
                let aabb = mesh.get_aabb();
                (
                    mesh.get_surface_count(),
                    [aabb.position.x, aabb.position.y, aabb.size.x, aabb.size.y],
                )
            }
            None => (0, [0.0; 4]),
        };
        let (shader_head, u_ambient) = match light.material() {
            Some(material) => {
                let head = material
                    .get_shader()
                    .map(|shader| shader.get_code().to_string())
                    .unwrap_or_default();
                let head: String = head.chars().take(120).collect();
                (
                    head,
                    format!("{:?}", material.get_shader_parameter("u_ambient")),
                )
            }
            None => (String::new(), String::from("null")),
        };
        let texture_size = node
            .get_texture()
            .map(|texture| [texture.get_width(), texture.get_height()])
            .unwrap_or([0, 0]);
        let band_visible = node
            .get_parent()
            .and_then(|parent| parent.try_cast::<godot::classes::CanvasItem>().ok())
            .map(|parent| parent.is_visible_in_tree())
            .unwrap_or(false);
        let value = serde_json::json!({
            "active": stats.visible,
            "rebuilds": stats.rebuilds,
            "lights": stats.lights,
            "node_visible": node.is_visible(),
            "visible_in_tree": node.is_visible_in_tree(),
            "band_visible": band_visible,
            "mesh_surfaces": surfaces,
            "mesh_aabb": aabb,
            "has_material": light.material().is_some(),
            "shader_head": shader_head,
            "u_ambient": u_ambient,
            "texture_size": texture_size,
        });
        GString::from(value.to_string().as_str())
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
        self.rules_override = true;
    }

    /// Sets `Rules.staticFog`.
    #[func]
    pub fn set_static_fog(&mut self, enabled: bool) {
        self.rules.static_fog = enabled;
        self.rules_override = true;
    }

    /// Sets the active `Rules.env` mask (plan 16 §3.12).
    #[func]
    pub fn set_rules_env(&mut self, mask: i64) {
        self.rules.env = mask as u32;
        self.rules_override = true;
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
        self.rules_override = true;
    }

    /// Resumes the live `Rules` feed after a debug `set_rules_*` override
    /// (G03 T12).
    #[func]
    pub fn clear_rules_override(&mut self) {
        self.rules_override = false;
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
    /// atlas-blitted `ImageTexture` for the menu background. Floors, edges,
    /// overlays, the wall shadow layer and walls are composited per tile;
    /// `None` when assets or the content registry are unavailable.
    #[func]
    pub fn build_menu_texture(&mut self, seed: i64, mobile: bool) -> Option<Gd<ImageTexture>> {
        self.menu.generate(seed as i32, mobile);
        let world = self.menu.world()?.clone();
        let assets = self.assets.clone()?;
        let host = self.host.clone()?;
        let texture = {
            let host = host.bind();
            let registry = host.content_registry()?;
            build_menu_image(&assets, registry, &world)
        };
        if texture.is_none() {
            log::warn!("MindWorldRenderer: menu background bake failed");
        }
        texture
    }

    /// `MenuRenderer.drawFlyers()` (`graphics/MenuRenderer.java:241-262`): the
    /// current flyer sprites for the baked menu texture, as JSON.
    ///
    /// Returns `{"width", "height", "rot", "type", "rows": [[x_tex, y_tex,
    /// region], ...]}`. `x_tex`/`y_tex` are in the same texture-pixel space as
    /// `build_menu_image` (32 px per tile); the world's y-up motion is mapped to
    /// screen-up, so the squadron crosses the background like Java's. The caller
    /// resolves `region` through `MindAssets` and rotates it by `rot`.
    #[func]
    pub fn menu_flyers_json(&mut self) -> GString {
        let Some(world) = self.menu.world().cloned() else {
            return GString::from("{\"rows\":[]}");
        };
        let speed = match self.host.clone() {
            Some(host) => {
                let host = host.bind();
                host.content_registry()
                    .and_then(|registry| registry.unit_by_name(world.flyer_type))
                    .map(|unit| unit.speed)
                    .unwrap_or(1.0)
            }
            None => 1.0,
        };
        let scale = MENU_TILE_PX as f32 / TILESIZE as f32;
        let tex_height = world.height as f32 * MENU_TILE_PX as f32;
        let time = self.menu.time();
        let rot = self.menu.flyer_rot();
        let rows: Vec<String> = (0..world.flyers)
            .map(|i| {
                let [x, y] = world.flyer_position(i, time, rot, speed);
                format!(
                    "[{:.2},{:.2},\"unit-{}-full\"]",
                    x * scale,
                    tex_height - y * scale,
                    world.flyer_type
                )
            })
            .collect();
        GString::from(
            format!(
                "{{\"width\":{},\"height\":{},\"rot\":{},\"type\":\"{}\",\"rows\":[{}]}}",
                world.width as i32 * MENU_TILE_PX,
                tex_height as i32,
                rot,
                world.flyer_type,
                rows.join(",")
            )
            .as_str(),
        )
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

/// Resolves a menu block name (`sandWall`, `sand`) to a registered block id;
/// the Java floor fields use the region names (`sand` is the `sand-floor`
/// block).
fn menu_block_id(registry: &ContentRegistry, name: &str) -> Option<BlockId> {
    let kebab = kebab_name(name);
    registry
        .block_id(&kebab)
        .or_else(|| registry.block_id(&format!("{kebab}-floor")))
}

/// Blends one floor/wall/edge quad of the menu bake into the target image.
///
/// The bake is a direct y-down pixel copy (row `y` = world tile `y`), so unlike
/// the camera pass it uses the region's *authored* orientation — exactly what
/// Arc's `Draw.rect` shows (`v_flip` is ignored here, `crop` is honoured).
#[allow(clippy::too_many_arguments)]
fn blend_menu_quad(
    target: &mut Gd<Image>,
    pages: &mut HashMap<i32, Gd<Image>>,
    assets: &Gd<MindAssets>,
    atlas: &mut RegionAtlas,
    quad: &PendingQuad,
    tx: i32,
    ty: i32,
    scale: f32,
) {
    let resolved = if quad.variant {
        atlas.resolve_variant(&quad.region, tx, ty)
    } else {
        atlas.resolve(&quad.region)
    };
    let (page, _texture, uv) = resolved_uv(&resolved);
    let geometry = match pages.get(&page) {
        Some(image) => image.clone(),
        None => {
            let bound = assets.bind();
            let Some(image) = bound
                .page_texture(page as i64)
                .and_then(|page| page.get_image())
            else {
                return;
            };
            drop(bound);
            pages.insert(page, image.clone());
            image
        }
    };
    let (tw, th) = (geometry.get_width() as f32, geometry.get_height() as f32);
    let mut sx = (uv[0] * tw).round() as i32;
    let mut sy = (uv[1] * th).round() as i32;
    let mut sw = ((uv[2] - uv[0]) * tw).round() as i32;
    let mut sh = ((uv[3] - uv[1]) * th).round() as i32;
    // `Floor.edge`: one cell of the region's 3×3 split, rows from the image top.
    if let Some((cols, rows, col, row)) = quad.crop {
        let cw = sw / i32::from(cols.max(1));
        let ch = sh / i32::from(rows.max(1));
        sx += i32::from(col) * cw;
        sy += i32::from(row) * ch;
        sw = cw;
        sh = ch;
    }
    let dw = (quad.w * scale).round() as i32;
    let dh = (quad.h * scale).round() as i32;
    let dx = (quad.cx * scale).round() as i32 - dw / 2;
    let dy = (quad.cy * scale).round() as i32 - dh / 2;
    target.blend_rect(
        &geometry,
        Rect2i::new(Vector2i::new(sx, sy), Vector2i::new(sw, sh)),
        Vector2i::new(dx, dy),
    );
}

/// Composites a [`MenuWorld`] the way `MenuRenderer.cache` + `render` do
/// (`graphics/MenuRenderer.java:170-239`): floors with `drawBase` (so J02 T1
/// edges), overlays, the black per-wall shadow framebuffer offset `(-4,-4)`
/// world units, then the walls.
fn build_menu_image(
    assets: &Gd<MindAssets>,
    registry: &ContentRegistry,
    world: &MenuWorld,
) -> Option<Gd<ImageTexture>> {
    let width = world.width as i32;
    let height = world.height as i32;
    // World units (TILESIZE = 8) → texture px (32 px per tile, the pack scale).
    let scale = MENU_TILE_PX as f32 / TILESIZE as f32;
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

    // `MenuRenderer.generate` world → a real tile grid so the shared floor path
    // (`tile_quads`/`floor_draw_base_quads`) does base + edges + overlays.
    let mut grid = WorldGrid::new(width, height);
    grid.generating = true;
    for y in 0..world.height {
        for x in 0..world.width {
            let Some(tile) = world.tile(x, y) else {
                continue;
            };
            let grid_tile = grid.tiles.get_mut(x as i32, y as i32);
            grid_tile.floor = menu_block_id(registry, tile.floor).unwrap_or(BlockId::AIR);
            grid_tile.block = menu_block_id(registry, tile.wall).unwrap_or(BlockId::AIR);
            grid_tile.overlay = menu_block_id(registry, tile.overlay).unwrap_or(BlockId::AIR);
        }
    }

    let mut atlas = RegionAtlas::new(Some(assets.clone()));
    let mut pages: HashMap<i32, Gd<Image>> = HashMap::new();

    // Floors + edges + overlays (`tile.floor().drawBase(tile)`,
    // `MenuRenderer.java:191-197` — no accessibility gate in the menu cache).
    for y in 0..height {
        for x in 0..width {
            let tile = grid.tile(x, y);
            for quad in floor_draw_base_quads(&grid, registry, tile, CacheLayerId::Normal) {
                blend_menu_quad(
                    &mut target,
                    &mut pages,
                    assets,
                    &mut atlas,
                    &quad,
                    x,
                    y,
                    scale,
                );
            }
        }
    }

    // Shadow framebuffer (`MenuRenderer.java:172-184`): one *pixel* per occupied
    // block (the menu FBO is `width`×`height` px), upscaled 8× with bilinear
    // filtering when drawn (`:226-228`) — that is what softens Java's shadows.
    // Drawn offset `(-4,-4)` world units = `(-16,+16)` texture px in this
    // y-down bake.
    let mut shadow_data = PackedByteArray::new();
    shadow_data.resize((width * height * 4) as usize);
    let mut shadows = Image::create_from_data(
        width,
        height,
        false,
        godot::classes::image::Format::RGBA8,
        &shadow_data,
    )?;
    shadows.fill(Color::from_rgba8(0, 0, 0, 0));
    for y in 0..height {
        for x in 0..width {
            if grid.tile(x, y).block != BlockId::AIR {
                shadows.set_pixel(x, y, Color::from_rgba8(0, 0, 0, 255));
            }
        }
    }
    shadows
        .resize_ex(width * MENU_TILE_PX, height * MENU_TILE_PX)
        .interpolation(godot::classes::image::Interpolation::BILINEAR)
        .done();
    let shadow = (MENU_TILE_PX as f32 * 4.0 / TILESIZE as f32).round() as i32;
    // Shifted `(-shadow, +shadow)`: blend the in-bounds part
    // (`MenuRenderer.java` clips the same way at the framebuffer edge).
    target.blend_rect(
        &shadows,
        Rect2i::new(
            Vector2i::new(shadow, 0),
            Vector2i::new(
                width * MENU_TILE_PX - shadow,
                height * MENU_TILE_PX - shadow,
            ),
        ),
        Vector2i::new(0, shadow),
    );

    // Walls (`tile.block().drawBase(tile)`, `MenuRenderer.java:202-204`).
    for y in 0..height {
        for x in 0..width {
            let tile = grid.tile(x, y);
            for quad in tile_quads(&grid, registry, tile, CacheLayerId::Walls) {
                blend_menu_quad(
                    &mut target,
                    &mut pages,
                    assets,
                    &mut atlas,
                    &quad,
                    x,
                    y,
                    scale,
                );
            }
        }
    }
    if atlas.missing > 0 {
        log::warn!(
            "MindWorldRenderer: menu background {} tiles missing regions",
            atlas.missing
        );
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

    /// `MenuRenderer.drawFlyers()` rows (`MindWorldRenderer::menu_flyers_json`).
    #[func]
    pub fn menu_flyers_json(&mut self) -> GString {
        self.renderer()
            .map(|mut renderer| renderer.bind_mut().menu_flyers_json())
            .unwrap_or_else(|| GString::from("{\"rows\":[]}"))
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
        // Same Java y-up projection as `MindCamera2D::process`: Godot 2D is
        // y-down, so the rendered y axis is negated.
        camera.set_zoom(Vector2::new(zoom, -zoom));
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

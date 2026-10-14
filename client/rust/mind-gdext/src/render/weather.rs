// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live weather rendering (`WeatherStateComp.draw`, `Weather.java:326-341`;
//! H07 T6/T7): particles and rain at `Layer.weather` (130), rain splashes at
//! `Layer.debris` (20), noise layers (T7).
//!
//! Java draws the particle fields through the shared `Weather` builders with a
//! per-frame `renderer.weatherAlpha` fade; the port keeps the same split: the
//! core builders (`mind_core::fx::weather_fx`) produce geometry, this node
//! executes it through Godot's `CanvasItem` draw calls. `Time.time` is the
//! sim tick (frozen while paused, like Java).

use std::collections::HashMap;

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, ImageTexture, Node2D, Texture2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::content::Rgba;
use mind_core::fx::weather_fx::{
    SplashGround, WeatherKind, WeatherStateView, WeatherView, splash_visible, weather_fx_for,
};
use mind_core::render::draw::{DrawPrim, PrimKind, RegionKey};

use crate::render::minimap::load_block_colors;
use crate::sim_host::MindSimHost;

/// `RainWeather.splashes`: `splash-0 .. splash-11` (12 frames, 32 px each).
const SPLASH_REGIONS: [RegionKey; 12] = [
    RegionKey("splash-0"),
    RegionKey("splash-1"),
    RegionKey("splash-2"),
    RegionKey("splash-3"),
    RegionKey("splash-4"),
    RegionKey("splash-5"),
    RegionKey("splash-6"),
    RegionKey("splash-7"),
    RegionKey("splash-8"),
    RegionKey("splash-9"),
    RegionKey("splash-10"),
    RegionKey("splash-11"),
];

/// `Draw.scl` during the splash draw: `1 / scale_marker.width` = `1/4`, and
/// the atlas `splash-*` regions are 32 px -> 8x8 world units
/// (`assets/sprites/sprites.atlas.json`, checked 2026-10-14).
const SPLASH_SIZE: f32 = 32.0 * 0.25;

/// The three weather draw passes (`WeatherStateComp.draw`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeatherPass {
    /// `ParticleWeather.drawOver` noise loop (T7).
    Noise,
    /// `ParticleWeather`/`RainWeather.drawOver` at `Layer.weather`.
    Over,
    /// `RainWeather.drawUnder` splashes at `Layer.debris`.
    Under,
}

/// One weather pass (`Node2D` under the weather/debris band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct WeatherRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    pass: WeatherPass,
    /// Atlas region lookup cache (`ParticleWeather.region`, splash frames).
    regions: HashMap<&'static str, Option<Gd<Texture2D>>>,
    /// Raw noise textures (T7).
    #[allow(dead_code, reason = "T7 noise pass reads this cache")]
    noise_textures: HashMap<String, Option<Gd<ImageTexture>>>,
    /// Packed minimap colours per block id (`Weather.drawSplashes` floor tint).
    block_colors: Vec<u32>,
    views: Vec<WeatherStateView>,
    prims: Vec<DrawPrim>,
    weather_alpha: f32,
    show_weather: bool,
    frames: u64,
}

#[godot_api]
impl INode2D for WeatherRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            pass: WeatherPass::Over,
            regions: HashMap::new(),
            noise_textures: HashMap::new(),
            block_colors: Vec::new(),
            views: Vec::new(),
            prims: Vec::new(),
            weather_alpha: 0.0,
            show_weather: true,
            frames: 0,
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

    fn process(&mut self, delta: f64) {
        if self.host.is_none() {
            return;
        }
        self.frames = self.frames.wrapping_add(1);
        // `Renderer.weatherAlpha` (`Renderer.java:180-191`): 0 during the
        // landing cutscene, else `lerpDelta(weatherAlpha, 1, 0.08)` per frame.
        if self.camera_cutscene() {
            self.weather_alpha = 0.0;
        } else {
            let dt = (delta as f32 * 60.0).max(0.0);
            self.weather_alpha += (1.0 - self.weather_alpha) * (1.0 - (1.0 - 0.08f32).powf(dt));
        }
        // `settings.getBool("showweather")` (default true), polled at the
        // settings cadence rather than per frame.
        if self.frames.is_multiple_of(30) {
            self.show_weather = self.ui_show_weather();
        }
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        if self.weather_alpha <= 0.0001 || !self.show_weather {
            return;
        }
        let Some(host) = self.host.clone() else {
            return;
        };
        let host = host.bind();
        // Camera bounds in world units (y-up through the flipped camera).
        let (cx, cy, zoom, viewport) = self.camera_frame();
        if zoom <= 0.0 || viewport.x <= 0.0 || viewport.y <= 0.0 {
            return;
        }
        let cam = WeatherView {
            x: cx - viewport.x / zoom / 2.0,
            y: cy - viewport.y / zoom / 2.0,
            w: viewport.x / zoom,
            h: viewport.y / zoom,
        };
        // `renderer.minScale()` = `Scl.scl(minZoomInGame)` = 1.5 on desktop.
        let field_w = viewport.x / 1.5;
        let field_h = viewport.y / 1.5;
        // Java `Time.time`; the sim tick freezes while paused like Java.
        let tick = host.get_tick() as f32;
        host.weather_views(&mut self.views);
        let mut prims = std::mem::take(&mut self.prims);
        prims.clear();
        if let Some(content) = host.content_registry() {
            let grid = host.grid();
            for index in 0..self.views.len() {
                let view = self.views[index];
                let Some(def) = content.weather(view.weather) else {
                    continue;
                };
                let Some(fx) = weather_fx_for(def) else {
                    continue;
                };
                match (self.pass, fx.kind) {
                    (WeatherPass::Over, WeatherKind::Particle) => {
                        let Some(particle) = def.particle.as_ref() else {
                            continue;
                        };
                        // `useWindVector` rule lives in `ParticleWeather.drawOver`.
                        let wind = if particle.use_wind_vector {
                            let speed = particle.base_speed * view.intensity;
                            (view.wind_vector.0 * speed, view.wind_vector.1 * speed)
                        } else {
                            (particle.xspeed, particle.yspeed)
                        };
                        let field = WeatherView::centered(
                            cx,
                            cy,
                            field_w,
                            field_h,
                            particle.size_max * 1.5,
                        );
                        fx.build_particles(&view, field, cam, tick, wind, &mut prims);
                    }
                    (WeatherPass::Over, WeatherKind::Rain) => {
                        let Some(rain) = def.rain.as_ref() else {
                            continue;
                        };
                        let field =
                            WeatherView::centered(cx, cy, field_w, field_h, rain.size_max * 0.9);
                        let alpha = self.weather_alpha * view.opacity * def.opacity_multiplier;
                        fx.build_rain(&view, field, cam, tick, alpha, &mut prims);
                    }
                    (WeatherPass::Under, WeatherKind::Rain) => {
                        let Some(rain) = def.rain.as_ref() else {
                            continue;
                        };
                        let field = WeatherView::centered(cx, cy, field_w, field_h, rain.size_max);
                        let water = content.liquid_id("water");
                        let colors = &self.block_colors;
                        let ground = |x: f32, y: f32| -> SplashGround {
                            // `World.tileWorld` = `Math.round(x / tilesize)`.
                            let tx = (x / 8.0).round() as i32;
                            let ty = (y / 8.0).round() as i32;
                            if !grid.tiles.in_bounds(tx, ty) {
                                return SplashGround::None;
                            }
                            let floor = grid.tiles.get(tx, ty).floor;
                            let Some(def) = content.block(floor) else {
                                return SplashGround::None;
                            };
                            if water.is_some() && def.liquid_drop == water {
                                // `tile.floor().mapColor.mul(1.5f)`: packed
                                // `rgba8888` (r << 24 | g << 16 | b << 8 | a).
                                let packed =
                                    colors.get(floor.raw() as usize).copied().unwrap_or(0xff);
                                let r = (((packed >> 24) & 0xff) as f32 / 255.0 * 1.5).min(1.0);
                                let g = (((packed >> 16) & 0xff) as f32 / 255.0 * 1.5).min(1.0);
                                let b = (((packed >> 8) & 0xff) as f32 / 255.0 * 1.5).min(1.0);
                                return SplashGround::Liquid(Rgba::new(r, g, b, 1.0));
                            }
                            if def.liquid_drop.is_none() && !def.solid {
                                SplashGround::Clear
                            } else {
                                SplashGround::Solid
                            }
                        };
                        if !splash_visible(&view) {
                            continue;
                        }
                        fx.build_splashes(
                            &view,
                            field,
                            cam,
                            tick,
                            rain.density,
                            rain.splash_time_scale,
                            rain.stroke,
                            &SPLASH_REGIONS,
                            SPLASH_SIZE,
                            ground,
                            &mut prims,
                        );
                    }
                    // The noise pass is T7; other pass/kind combinations draw
                    // nothing (`fog` has no particles).
                    (WeatherPass::Noise, _) | (WeatherPass::Under, WeatherKind::Particle) => {}
                }
            }
        }
        for prim in &prims {
            self.draw_prim(prim);
        }
        self.prims = prims;
    }
}

impl WeatherRenderer {
    /// Rust-only: selects the pass this node renders (called after
    /// `new_alloc`; the count is fixed by `WeatherStateComp.draw`).
    pub fn set_pass(&mut self, pass: WeatherPass) {
        self.pass = pass;
    }

    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.block_colors = load_block_colors();
        self.show_weather = self.ui_show_weather();
        self.base_mut().queue_redraw();
    }

    /// `Camera2D` world frame + viewport size (screen pixels).
    fn camera_frame(&self) -> (f32, f32, f32, Vector2) {
        let viewport = self
            .base()
            .get_viewport()
            .map(|viewport| viewport.get_visible_rect().size)
            .unwrap_or(Vector2::new(1.0, 1.0));
        let camera = self
            .base()
            .get_node_or_null("/root/Spine/World/Camera2D")
            .and_then(|node| node.try_cast::<godot::classes::Camera2D>().ok());
        match camera {
            Some(camera) => (
                camera.get_global_position().x,
                camera.get_global_position().y,
                camera.get_zoom().x.abs().max(0.01),
                viewport,
            ),
            None => (0.0, 0.0, 1.0, viewport),
        }
    }

    /// `Camera2D.is_cutscene()` (the renderer freezes `weatherAlpha` during the
    /// landing cutscene).
    fn camera_cutscene(&self) -> bool {
        self.base()
            .get_node_or_null("/root/Spine/World/Camera2D")
            .and_then(|node| node.try_cast::<crate::camera::MindCamera2D>().ok())
            .is_some_and(|camera| camera.bind().is_cutscene())
    }

    /// `MindUi.settings_get_bool("showweather")` (default true).
    fn ui_show_weather(&self) -> bool {
        let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") else {
            return true;
        };
        if !ui.has_method("settings_get_bool") {
            return true;
        }
        ui.call(
            "settings_get_bool",
            &[GString::from("showweather").to_variant()],
        )
        .try_to::<bool>()
        .unwrap_or(true)
    }

    /// Resolves `RegionKey.0` through the `MindAssets` atlas (`find_region`).
    fn region_texture(&mut self, name: &'static str) -> Option<Gd<Texture2D>> {
        if let Some(cached) = self.regions.get(name) {
            return cached.clone();
        }
        let texture = self
            .base()
            .try_get_node_as::<crate::assets::MindAssets>("/root/MindAssets")
            .and_then(|mut assets| {
                assets
                    .bind_mut()
                    .find_region(GString::from(name))
                    .map(|texture| texture.upcast::<Texture2D>())
            });
        self.regions.insert(name, texture.clone());
        texture
    }

    /// Executes one prim (region/line only; the noise pass executes its own
    /// textures in T7).
    fn draw_prim(&mut self, prim: &DrawPrim) {
        match &prim.kind {
            PrimKind::Region {
                region,
                x,
                y,
                w,
                h,
                rotation_deg,
                origin,
                color,
                ..
            } => {
                let texture = self.region_texture(region.0);
                let mut base = self.base_mut();
                match texture {
                    Some(texture) => {
                        // Same arm as `gdext/fx.rs` `draw_prim`: world space is
                        // y-up through the flipped camera, so the texture rect
                        // uses a negative height and the position the negated
                        // origin fractions.
                        base.draw_set_transform_ex(Vector2::new(*x, *y))
                            .rotation(rotation_deg.to_radians())
                            .scale(Vector2::ONE)
                            .done();
                        base.draw_texture_rect_ex(
                            &texture,
                            Rect2::new(
                                Vector2::new(-w * origin.0, -h * origin.1),
                                Vector2::new(*w, -h),
                            ),
                            false,
                        )
                        .modulate(to_color(*color))
                        .done();
                        base.draw_set_transform(Vector2::ZERO);
                    }
                    None => {
                        // Missing region: debug quad so the pass is visible.
                        let rect = Rect2::new(
                            Vector2::new(x - w / 2.0, y - h / 2.0),
                            Vector2::new(*w, *h),
                        );
                        base.draw_rect_ex(rect, to_color(*color))
                            .filled(true)
                            .done();
                    }
                }
            }
            PrimKind::Line {
                x1,
                y1,
                x2,
                y2,
                stroke,
                color,
                ..
            } => {
                let mut base = self.base_mut();
                // No width clamp: rain stroke is 0.75 world units.
                base.draw_line_ex(
                    Vector2::new(*x1, *y1),
                    Vector2::new(*x2, *y2),
                    to_color(*color),
                )
                .width(*stroke)
                .done();
            }
            _ => {}
        }
    }
}

fn to_color(color: Rgba) -> Color {
    Color::from_rgba(color.r, color.g, color.b, color.a)
}

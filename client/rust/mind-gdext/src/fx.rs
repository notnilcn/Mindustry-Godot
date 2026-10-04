// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindFx` — the in-engine FX host (plan 17 §3.13).
//!
//! Owns the plan-17 view pools (`FxPool`/`DecalPool`/`TrailRegistry`), drains
//! the [`FxBus`] after each fixed sim tick, compiles effect geometry into a
//! [`DrawProgram`] and executes it in `_draw`. Godot-only; no game rules.
//!
//! The catalogue/lifecycle/program math all lives in `mind-core::fx`; this file
//! is the thin execution + probe facade the MCP oracle drives.

use godot::builtin::{Color, GString, PackedVector2Array, Rect2, VarDictionary, Vector2};
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D};
use godot::obj::{Base, WithBaseField};
use godot::prelude::*;

use smallvec::SmallVec;

use mind_core::combat::view::TurretDrawState;
use mind_core::content::{EffectId, Rgba, WeatherId};
use mind_core::fx::{AllRegions, Decal, DecalPool, ShakeState, TrailRegistry};
use mind_core::fx::{
    DrawProgram, EffectData, EffectKind, EmptySnapshot, FxBus, FxEvent, FxPool, FxSettings,
    WeatherFx, WeatherKind, WeatherStateView, WeatherView, build_program_into, registry,
};

pub mod draw_turret;
pub mod env;

use draw_turret::TurretDrawInput;
use env::EnvFxInput;
use mind_core::render::draw::PrimKind;
use mind_core::render::layer::Layer;

/// Converts a core `Rgba` to a Godot `Color`.
fn to_color(color: Rgba) -> Color {
    Color::from_rgba(color.r, color.g, color.b, color.a)
}

/// `MindFx` — effect/decal/trail/shake pool host + MCP probes.
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct MindFx {
    base: Base<Node2D>,
    bus: FxBus,
    pool: FxPool,
    decals: DecalPool,
    trails: TrailRegistry,
    shake: ShakeState,
    settings: FxSettings,
    program: DrawProgram,
    view_tick: u64,
    draw_calls: i64,
    missed_events: i64,
    weather: Option<(WeatherFx, WeatherStateView)>,
    view_rect: WeatherView,
    /// Owned turret draw snapshots (plan 17 M5 `DrawTurret` adapter).
    turrets: Vec<TurretDrawInput>,
    next_turret_handle: i64,
    /// Active env pass (plan 17 M6 `EnvRenderers` bodies).
    env: Option<EnvFxInput>,
}

#[godot_api]
impl INode2D for MindFx {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            bus: FxBus::new(),
            pool: FxPool::default(),
            decals: DecalPool::default(),
            trails: TrailRegistry::new(),
            shake: ShakeState::default(),
            settings: FxSettings::default(),
            program: DrawProgram::new(),
            view_tick: 0,
            draw_calls: 0,
            missed_events: 0,
            weather: None,
            view_rect: WeatherView::centered(0.0, 0.0, 1.0, 1.0, 0.0),
            turrets: Vec::new(),
            next_turret_handle: 1,
            env: None,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn draw(&mut self) {
        let mut program = std::mem::take(&mut self.program);
        program.clear();
        let snapshot = EmptySnapshot;
        for state in self.pool.iter_live() {
            let def = registry().get(state.def);
            build_program_into(def, state, &snapshot, &mut program);
        }
        self.append_view_prims(&mut program);
        self.append_turret_prims(&mut program);
        if let Some(input) = self.env {
            env::build_env_prims(&mut program, self.view_tick as f32, &input);
        }
        program.sort();

        let mut calls = 0i64;
        for prim in &program.prims {
            if self.draw_prim(prim) {
                calls += 1;
            } else {
                self.missed_events += 1;
            }
        }
        self.draw_calls = calls;
        self.program = program;
    }
}

impl MindFx {
    /// Rebuilds the node's Godot-derived state (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`).
    fn bootstrap(&mut self) {
        self.base_mut().queue_redraw();
    }

    /// Executes one primitive; returns whether it was drawn.
    fn draw_prim(&mut self, prim: &mind_core::render::draw::DrawPrim) -> bool {
        let mut base = self.base_mut();
        match &prim.kind {
            PrimKind::Circle {
                x,
                y,
                r,
                fill,
                stroke,
                color,
            } => {
                if *fill {
                    base.draw_circle(Vector2::new(*x, *y), *r, to_color(*color));
                } else {
                    let _ = stroke;
                    base.draw_arc(
                        Vector2::new(*x, *y),
                        *r,
                        0.0,
                        std::f32::consts::TAU,
                        24,
                        to_color(*color),
                    );
                }
                true
            }
            PrimKind::Rect { x, y, w, h, color } => {
                let rect = Rect2::new(Vector2::new(x - w / 2.0, y - h / 2.0), Vector2::new(*w, *h));
                base.draw_rect_ex(rect, to_color(*color))
                    .filled(true)
                    .done();
                true
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
                base.draw_line_ex(
                    Vector2::new(*x1, *y1),
                    Vector2::new(*x2, *y2),
                    to_color(*color),
                )
                .width(stroke.max(1.0))
                .done();
                true
            }
            PrimKind::Poly {
                x,
                y,
                sides,
                r,
                rotation_deg,
                fill,
                stroke,
                color,
            } => {
                let points = polygon_points(*sides, *r, *x, *y, *rotation_deg);
                if *fill {
                    base.draw_colored_polygon(&points, to_color(*color));
                } else {
                    base.draw_polyline_ex(&points, to_color(*color))
                        .width(stroke.max(1.0))
                        .done();
                }
                true
            }
            PrimKind::Polygon {
                points,
                fill,
                stroke,
                color,
            } => {
                let packed: PackedVector2Array = points
                    .iter()
                    .map(|p| Vector2::new(p.0, p.1))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .collect();
                if *fill {
                    base.draw_colored_polygon(&packed, to_color(*color));
                } else {
                    base.draw_polyline_ex(&packed, to_color(*color))
                        .width(stroke.max(1.0))
                        .done();
                }
                true
            }
            PrimKind::Tri {
                x,
                y,
                w,
                h,
                rotation_deg,
                color,
            } => {
                let points = triangle_points(*x, *y, *w, *h, *rotation_deg);
                base.draw_colored_polygon(&points, to_color(*color));
                true
            }
            PrimKind::Arc {
                x,
                y,
                r,
                start_deg,
                sweep_deg,
                stroke,
                color,
            } => {
                let _ = stroke;
                base.draw_arc(
                    Vector2::new(*x, *y),
                    *r,
                    start_deg.to_radians(),
                    (start_deg + sweep_deg).to_radians(),
                    24,
                    to_color(*color),
                );
                true
            }
            PrimKind::Light {
                x,
                y,
                radius,
                color,
                opacity,
            } => {
                // Plan 16 owns the additive light FBO; draw a faint debug disc.
                let mut c = to_color(*color);
                c.a *= opacity.clamp(0.0, 1.0);
                base.draw_circle(Vector2::new(*x, *y), *radius, c);
                true
            }
            PrimKind::Region {
                x, y, w, h, color, ..
            } => {
                // Region binding is plan 03/16; draw a debug quad so the FX pass
                // is visible in-engine until the atlas resolver is wired.
                let rect = Rect2::new(Vector2::new(x - w / 2.0, y - h / 2.0), Vector2::new(*w, *h));
                base.draw_rect_ex(rect, to_color(*color))
                    .filled(true)
                    .done();
                true
            }
            // Executed by plan 16's pipeline (no CPU geometry).
            PrimKind::NoiseLayer { .. }
            | PrimKind::ShaderBlit { .. }
            | PrimKind::Polyline { .. } => false,
        }
    }

    /// Appends decal/trail/weather view prims to the frame program (plan 17 M6).
    fn append_view_prims(&self, program: &mut DrawProgram) {
        // Decals (`Layer.scorch`, fade `1 - curve(fin, 0.98)`).
        for decal in self.decals.iter_live() {
            let alpha = decal.alpha();
            if alpha <= 0.0 {
                continue;
            }
            program.push(mind_core::render::draw::DrawPrim::at(
                Decal::LAYER,
                PrimKind::Region {
                    region: decal.region,
                    x: decal.x,
                    y: decal.y,
                    w: 32.0,
                    h: 32.0,
                    rotation_deg: decal.rotation,
                    origin: (0.5, 0.5),
                    color: decal.color.with_alpha(decal.color.a * alpha),
                    mix: None,
                    wrap: false,
                },
            ));
        }

        // Trails (ribbon quads + cap).
        let mut quads: Vec<[f32; 8]> = Vec::new();
        for (_, trail, color) in self.trails.iter_live_colored() {
            quads.clear();
            trail.draw(1.0, &mut quads);
            for q in &quads {
                let mut points: SmallVec<[(f32, f32); 12]> = SmallVec::new();
                points.push((q[0], q[1]));
                points.push((q[2], q[3]));
                points.push((q[4], q[5]));
                points.push((q[6], q[7]));
                program.push(mind_core::render::draw::DrawPrim::at(
                    Layer::Effect.z(),
                    PrimKind::Polygon {
                        points,
                        fill: true,
                        stroke: 0.0,
                        color,
                    },
                ));
            }
            if let Some((cx, cy, w, _rot)) = trail.draw_cap(1.0) {
                program.push(mind_core::render::draw::DrawPrim::at(
                    Layer::Effect.z(),
                    PrimKind::Circle {
                        x: cx,
                        y: cy,
                        r: w / 2.0,
                        fill: true,
                        stroke: 0.0,
                        color,
                    },
                ));
            }
        }

        // Weather particle/rain field (state frozen by plan 12).
        if let Some((fx, state)) = &self.weather {
            match fx.kind {
                WeatherKind::Particle => fx.build_particles(
                    state,
                    self.view_rect,
                    self.view_rect,
                    self.view_tick as f32,
                    &mut program.prims,
                ),
                WeatherKind::Rain => fx.build_rain(
                    state,
                    self.view_rect,
                    self.view_rect,
                    self.view_tick as f32,
                    &mut program.prims,
                ),
            }
        }
    }

    /// Appends owned turret draw snapshots at the block band (plan 17 M5).
    ///
    /// Region binding is plan 03/16; `AllRegions` treats every name as a found
    /// 32-px region so the draw order/layers are visible in-engine until the
    /// atlas executor lands (same interim policy as `draw_prim`).
    fn append_turret_prims(&self, program: &mut DrawProgram) {
        if self.turrets.is_empty() {
            return;
        }
        let lookup = AllRegions::new(32.0);
        for input in &self.turrets {
            draw_turret::build_turret(
                program,
                &lookup,
                self.view_tick as f32,
                Layer::Block.z(),
                input,
            );
        }
    }
}

fn polygon_points(sides: u16, r: f32, x: f32, y: f32, rotation_deg: f32) -> PackedVector2Array {
    let count = sides.max(3);
    let mut out: Vec<Vector2> = Vec::with_capacity(count as usize);
    for i in 0..count {
        let a = rotation_deg.to_radians() + i as f32 * std::f32::consts::TAU / count as f32;
        out.push(Vector2::new(x + a.cos() * r, y + a.sin() * r));
    }
    out.into_iter().collect()
}

fn triangle_points(x: f32, y: f32, w: f32, h: f32, rotation_deg: f32) -> PackedVector2Array {
    // `Drawf.tri`: apex forward (+y local), base behind.
    let local = [(-w / 2.0, 0.0), (w / 2.0, 0.0), (0.0, h)];
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let mut out: Vec<Vector2> = Vec::with_capacity(3);
    for (lx, ly) in local {
        let rx = lx * cos - ly * sin;
        let ry = lx * sin + ly * cos;
        out.push(Vector2::new(x + rx, y + ry));
    }
    out.into_iter().collect()
}

#[godot_api]
impl MindFx {
    /// The shared effect bus `MindSimHost` writes to (set at boot).
    pub fn bus(&self) -> &FxBus {
        &self.bus
    }

    /// Drains the bus and advances the view pools by one tick.
    #[func]
    pub fn tick_view(&mut self) {
        self.view_tick += 1;
        let snapshot = EmptySnapshot;
        for event in self.bus.drain() {
            match event {
                FxEvent::Effect {
                    effect,
                    x,
                    y,
                    rotation,
                    color,
                } => {
                    let id = match &effect {
                        mind_core::content::registries::fx_meta::EffectRef::Named(id) => *id,
                        mind_core::content::registries::fx_meta::EffectRef::Inline(_) => {
                            EffectId::NONE
                        }
                    };
                    if !self.settings.effects || id == EffectId::NONE {
                        continue;
                    }
                    let def = registry().get(id);
                    self.pool
                        .spawn(def, x, y, rotation, color, EffectData::None, &snapshot);
                }
                FxEvent::Shake {
                    intensity,
                    duration,
                } => {
                    self.shake.add(intensity, duration.max(0.0));
                }
                FxEvent::Decal {
                    region,
                    x,
                    y,
                    rotation,
                    lifetime,
                    color,
                } => {
                    let mut decal = mind_core::fx::Decal::new(region, x, y, rotation, color);
                    decal.lifetime = lifetime;
                    self.decals.spawn(decal);
                }
                FxEvent::Trail { .. } | FxEvent::Light { .. } | FxEvent::Sound { .. } => {
                    // Trails/lights/sounds are owned by plans 10/16/18 sinks.
                }
            }
        }
        self.pool.advance(&snapshot);
        self.decals.advance();
        self.base_mut().queue_redraw();
    }

    /// Live effect-state count.
    #[func]
    pub fn live_effect_count(&self) -> i64 {
        self.pool.live_count() as i64
    }

    /// Live decal count.
    #[func]
    pub fn live_decal_count(&self) -> i64 {
        self.decals.live_count() as i64
    }

    /// Live trail channel count.
    #[func]
    pub fn live_trail_count(&self) -> i64 {
        self.trails.count() as i64
    }

    /// Draw calls executed on the last frame.
    #[func]
    pub fn draw_call_count(&self) -> i64 {
        self.draw_calls
    }

    /// Size of the last compiled program.
    #[func]
    pub fn last_program_size(&self) -> i64 {
        self.program.len() as i64
    }

    /// Histogram of live effect behaviour classes.
    #[func]
    pub fn effect_kind_counts(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let mut custom = 0i64;
        let mut declarative = 0i64;
        let mut composite = 0i64;
        let mut unported = 0i64;
        let mut none = 0i64;
        for state in self.pool.iter_live() {
            match &registry().get(state.def).kind {
                EffectKind::None => none += 1,
                EffectKind::Custom(..) => custom += 1,
                EffectKind::Particle(_)
                | EffectKind::Explosion(_)
                | EffectKind::Wave(_)
                | EffectKind::Triangle(_)
                | EffectKind::Noise(_) => declarative += 1,
                EffectKind::Multi(_)
                | EffectKind::Seq(_)
                | EffectKind::Radial(_)
                | EffectKind::Wrap(_)
                | EffectKind::Sound(_) => composite += 1,
                EffectKind::Unported => unported += 1,
            }
        }
        let key = |name: &str| GString::from(name);
        dict.set(&key("none"), &none.to_variant());
        dict.set(&key("custom"), &custom.to_variant());
        dict.set(&key("declarative"), &declarative.to_variant());
        dict.set(&key("composite"), &composite.to_variant());
        dict.set(&key("unported"), &unported.to_variant());
        dict
    }

    /// JSON list of live effect states.
    #[func]
    pub fn dump_effects(&self) -> GString {
        let mut out = String::from("[");
        for (i, state) in self.pool.iter_live().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"id\":{},\"name\":\"{}\",\"x\":{},\"y\":{},\"rotation\":{},\"time\":{},\"lifetime\":{},\"layer\":{}}}",
                state.def.raw(),
                registry().get(state.def).name,
                state.x,
                state.y,
                state.rotation,
                state.time,
                state.lifetime,
                registry().get(state.def).layer,
            ));
        }
        out.push(']');
        GString::from(out.as_str())
    }

    /// Dev control: spawn a named catalogue effect.
    #[func]
    pub fn spawn_effect(&mut self, name: GString, x: f32, y: f32, rotation: f32) -> i64 {
        let name_str = name.to_string();
        let Some(id) = mind_core::content::effect_by_name(&name_str) else {
            return -1;
        };
        let def = registry().get(id);
        if self
            .pool
            .spawn(
                def,
                x,
                y,
                rotation,
                Rgba::WHITE,
                EffectData::None,
                &EmptySnapshot,
            )
            .is_some()
        {
            self.base_mut().queue_redraw();
            self.pool.live_count() as i64
        } else {
            -1
        }
    }

    /// Dev control: clears all effects.
    #[func]
    pub fn clear_effects(&mut self) {
        self.pool.clear();
        self.program.clear();
        self.base_mut().queue_redraw();
    }

    /// Dev control: enables/disables effect spawns.
    #[func]
    pub fn set_fx_enabled(&mut self, enabled: bool) {
        self.settings.effects = enabled;
        if !enabled {
            self.clear_effects();
        }
    }

    /// Primitives deferred to plan 16's pipeline (noise/shader/polyline).
    #[func]
    pub fn deferred_prim_count(&self) -> i64 {
        self.missed_events
    }

    /// Dev control: sets the weather view (`rain` or any other name = particles).
    /// Weather state is owned by plan 12; this is the plan-17 draw probe.
    #[allow(clippy::too_many_arguments)]
    #[func]
    pub fn set_weather(
        &mut self,
        kind: GString,
        intensity: f32,
        opacity: f32,
        wind_x: f32,
        wind_y: f32,
        life: f32,
        view_x: f32,
        view_y: f32,
        view_w: f32,
        view_h: f32,
    ) {
        let mut fx = WeatherFx::default();
        if kind.to_string().eq_ignore_ascii_case("rain") {
            fx.kind = WeatherKind::Rain;
        }
        self.weather = Some((
            fx,
            WeatherStateView {
                weather: WeatherId::new(0),
                intensity,
                opacity,
                wind_vector: (wind_x, wind_y),
                life,
            },
        ));
        self.view_rect = WeatherView {
            x: view_x,
            y: view_y,
            w: view_w.max(1.0),
            h: view_h.max(1.0),
        };
        self.base_mut().queue_redraw();
    }

    /// Clears the weather view probe.
    #[func]
    pub fn clear_weather(&mut self) {
        self.weather = None;
        self.base_mut().queue_redraw();
    }

    /// Whether a weather view is active.
    #[func]
    pub fn weather_active(&self) -> bool {
        self.weather.is_some()
    }

    /// Dev control: sets the active env pass (`Rules.env` + camera/world bounds).
    /// Bodies are plan 17; registration/selection is plan 16.
    #[allow(clippy::too_many_arguments)]
    #[func]
    pub fn set_env(
        &mut self,
        rules_env: i64,
        cam_x: f32,
        cam_y: f32,
        cam_w: f32,
        cam_h: f32,
        world_w: f32,
        world_h: f32,
        ray_w: f32,
        ray_h: f32,
        fog: bool,
    ) {
        self.env = Some(EnvFxInput {
            rules_env: rules_env as u32,
            cam: (cam_x, cam_y, cam_w, cam_h),
            world: (world_w, world_h),
            ray_size: (ray_w, ray_h),
            fog,
        });
        self.base_mut().queue_redraw();
    }

    /// Dev control: clears the active env pass.
    #[func]
    pub fn clear_env(&mut self) {
        self.env = None;
        self.base_mut().queue_redraw();
    }

    /// Whether an env pass is active.
    #[func]
    pub fn env_active(&self) -> bool {
        self.env.is_some()
    }

    /// Whether effects are enabled.
    #[func]
    pub fn fx_enabled(&self) -> bool {
        self.settings.effects
    }

    /// Part animation probe (plan 17 M4/M5). Returns the [`PartParams`] the
    /// turret snapshot with `unit_id` would evaluate; `status` is `missing`
    /// when no such snapshot exists (MCP §7c step 7).
    #[func]
    pub fn sample_part(&self, unit_id: i64, weapon_index: i64, part_index: i64) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        dict.set(&key("unit"), &unit_id.to_variant());
        dict.set(&key("weapon"), &weapon_index.to_variant());
        dict.set(&key("part"), &part_index.to_variant());
        match self.turrets.iter().find(|t| t.handle == unit_id) {
            Some(turret) => {
                let p = turret.part_params();
                dict.set(&key("warmup"), &p.warmup.to_variant());
                dict.set(&key("reload"), &p.reload.to_variant());
                dict.set(&key("smooth_reload"), &p.smooth_reload.to_variant());
                dict.set(&key("heat"), &p.heat.to_variant());
                dict.set(&key("recoil"), &p.recoil.to_variant());
                dict.set(&key("charge"), &p.charge.to_variant());
                dict.set(&key("status"), &GString::from("ok").to_variant());
            }
            None => {
                dict.set(&key("status"), &GString::from("missing").to_variant());
            }
        }
        dict
    }

    /// Dev/MCP probe: registers an owned turret draw snapshot from plan-10 draw
    /// state and returns its handle (`<= 0` rejected). Region execution is
    /// plan 03/16; this exercises the DrawTurret adapter in-engine.
    #[allow(clippy::too_many_arguments)]
    #[func]
    pub fn spawn_turret_draw(
        &mut self,
        name: GString,
        x: f32,
        y: f32,
        rotation: f32,
        heat: f32,
        warmup: f32,
        recoil: f32,
        charge: f32,
        ammo_fraction: f32,
        progress: f32,
    ) -> i64 {
        let handle = self.next_turret_handle;
        self.next_turret_handle += 1;
        let state = TurretDrawState {
            rotation,
            recoil,
            heat,
            warmup,
            charge,
            ammo_fraction,
        };
        self.turrets.push(TurretDrawInput::from_state(
            handle,
            name.to_string(),
            x,
            y,
            state,
            progress,
            0.0,
            Rgba::WHITE,
            Rgba::WHITE,
        ));
        self.base_mut().queue_redraw();
        handle
    }

    /// Dev/MCP probe: removes every registered turret draw snapshot.
    #[func]
    pub fn clear_turrets(&mut self) {
        self.turrets.clear();
        self.base_mut().queue_redraw();
    }

    /// Live turret draw-snapshot count.
    #[func]
    pub fn live_turret_count(&self) -> i64 {
        self.turrets.len() as i64
    }
}

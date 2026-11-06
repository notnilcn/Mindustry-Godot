// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/PlanetDialog.java +
//         core/src/mindustry/graphics/g3d/PlanetRenderer.java (plan 16 §3.10);
//         native port of client/ui/planet_view.gd (the GDScript this replaces
//         1:1, including its `PlanetGrid`/`HexMesh`/`HexSkyMesh` comments).

//! The campaign sector-select globe (`RustPlanetView`).
//!
//! A `SubViewport` 3D scene owned by `scenes/ui/planet_view.tscn`: the vendored
//! planet art (or the generated hex mesh) as the surface, the `PlanetGrid` line
//! mesh drawn with the ported `planetgrid` shader, and the selected sector
//! highlighted from the grid corner geometry. All data comes from
//! `MindCampaign.planet_view` (the Godot-free `mind_core::render::g3d`
//! topology), `planet_light`, `planet_mesh` and `planet_clouds`; `MindUi`
//! supplies the `atmosphere` graphics setting.
//!
//! The class has no signals: the GDScript base it ports defines none, and the
//! driving `PlanetDialog` controller polls the read-only properties instead.
//! Registered as `RustPlanetView` while the GDScript `class_name PlanetView`
//! still exists (PORT-STATUS.md §"Temporary registration names").

use std::f32::consts::{PI, TAU};

use godot::classes::base_material_3d::{
    CullMode, DepthDrawMode, Flags, ShadingMode, TextureFilter, TextureParam, Transparency,
};
use godot::classes::mesh::{ArrayCustomFormat, ArrayFormat, ArrayType, PrimitiveType};
use godot::classes::{
    ArrayMesh, BoxMesh, Camera3D, Control, Cubemap, FileAccess, Font, IControl, Image, Label3D,
    Mesh, MeshInstance3D, Node, Node3D, QuadMesh, ResourceLoader, RichTextLabel, Shader,
    ShaderMaterial, SphereMesh, Sprite3D, StandardMaterial3D, SubViewport, Texture2D, TextureRect,
    Time,
};
use godot::prelude::*;

use crate::ui::kit::{self, styles};

/// `PlanetRenderer.camLength`; camera distance is `(radius + camRadius) * this`.
const CAM_LENGTH: f32 = 4.0;
/// Vendored planet-art quad size; the sprite's bright disc is ~0.37 of its width.
const BODY_SIZE: f32 = 2.7;
/// Sector icon size inside the selected hex (`renderProjections`
/// `iw = 64f/4f`, ~21 px on screen at the near-surface scale).
const SECTOR_ICON_SIZE: f32 = 0.106;
/// `PlanetRenderer.outlineColor`/`hoverColor` are Pal.accent (#ffd37f).
const ACCENT: Color = Color::from_rgba(1.0, 0.827, 0.498, 1.0);
/// `PlanetShader.lightDir` initial value (`Shaders.java:109`), used until
/// `MindCampaign.planet_light` answers.
const DEFAULT_LIGHT: Vector3 = Vector3::new(0.57735, 0.57735, 0.57735);
/// The 2D `$Space` `STRETCH_KEEP_ASPECT_COVERED` reference framing
/// (1152x648) that `_build_background` mirrors in 3D.
const BACKDROP_ASPECT: f32 = 1152.0 / 648.0;

/// Transparent draw order, mirroring `PlanetRenderer.render`/`renderTransparent`
/// (`PlanetRenderer.java:105-127,147-168`): clouds (-7/-6) -> `renderSectors`
/// (grid 4, rings/fills/hover 5, selection/outline 6) -> `drawAtmosphere` ->
/// `renderOrbit` -> `renderProjections` (sector icons).
const ATMOSPHERE_PRIORITY: i32 = 7;
const ORBIT_PRIORITY: i32 = 8;
const ICON_PRIORITY: i32 = 9;

/// `Pal.gray` (`Pal.java:82`, `454545`) — the orbit line colour.
const ORBIT_GRAY: Color = Color::from_rgba(0.271, 0.271, 0.271, 1.0);
/// `Team.sharded/crux/derelict.color`.
const SHARDED: Color = Color::from_rgba(1.0, 0.827, 0.498, 1.0);
const CRUX: Color = Color::from_rgba(0.949, 0.333, 0.333, 1.0);
const DERELICT: Color = Color::from_rgba(0.302, 0.306, 0.345, 1.0);
/// `PlanetRenderer.shadowColor`.
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.7);
/// `Pal.place` (`Pal.java:85`, `6335f8`) — the export-arc start colour.
const PLACE: Color = Color::from_rgba(0.388, 0.208, 0.973, 1.0);
/// Arc `Color.gray` (`Color.java:14`, `7f7f7fff`) — the import-arc start colour.
const ARC_GRAY: Color = Color::from_rgba(0.498, 0.498, 0.498, 1.0);
/// `PlanetDialog.java:506-508`: the launch/sector arcs use 25 curve points.
const ARC_POINTS: i32 = 25;

const PLANET_SHADER: &str = "res://shaders/planet.gdshader";
const CLOUDS_SHADER: &str = "res://shaders/clouds.gdshader";
const GRID_SHADER: &str = "res://shaders/planetgrid.gdshader";
const ATMOSPHERE_SHADER: &str = "res://shaders/atmosphere.gdshader";
/// `Shaders.unlit` (`unlit.vert/.frag`) — `SunMesh`'s material
/// (`SunMesh.java:23`), ported as `unlit.gdshader`.
const UNLIT_SHADER: &str = "res://shaders/unlit.gdshader";
/// `CubemapMesh`'s `shaders/cubemap.vert`/`.frag` (`CubemapMesh.java:64`).
const CUBEMAP_SHADER: &str = "res://shaders/cubemap.gdshader";
/// Arc `Cubemap(String base)` face order (`Cubemap.java:29-38`), which is also
/// Godot's `Cubemap.create_from_images` order (+X, -X, +Y, -Y, +Z, -Z).
const STAR_FACES: [&str; 6] = ["right", "left", "top", "bottom", "front", "back"];
/// `CubemapMesh`'s `Cubemap("cubemaps/stars/")` (`PlanetRenderer.java:36`).
const STAR_CUBEMAP_DIR: &str = "cubemaps/stars";

const SUBVIEWPORT_PATH: &str = "ViewportContainer/SubViewport";
const CAMERA_PATH: &str = "ViewportContainer/SubViewport/Camera";
const BACKGROUND_PATH: &str = "ViewportContainer/SubViewport/Background";
const BODY_PATH: &str = "ViewportContainer/SubViewport/Body";
const ATMOSPHERE_PATH: &str = "ViewportContainer/SubViewport/Atmosphere";
const GRID_PATH: &str = "ViewportContainer/SubViewport/Grid";
const SELECTION_PATH: &str = "ViewportContainer/SubViewport/Selection";
const HOVER_PATH: &str = "ViewportContainer/SubViewport/Hover";
const OUTLINE_PATH: &str = "ViewportContainer/SubViewport/Outline";

/// One `Planet.drawArc`/`Planet.drawArcLine` curve
/// (`Planet.java:641-690`) between two sector tiles.
#[derive(Debug, Clone)]
struct ArcSpec {
    /// Start/end grid tile indices (`sector.tile.v`).
    a: i32,
    b: i32,
    /// Arc gradient start/end (`Color.lerp` along the curve, animated).
    from: Color,
    to: Color,
    /// Curve height factor (`length`) and colour-cycle period in ticks.
    length: f32,
    time_scale: f32,
    /// Ribbon half-width (`drawArcLine` `stroke`); `0` = the 1 px
    /// `Gl.lineStrip` variant, rendered as a thin ribbon here.
    stroke: f32,
}

/// `PlanetView` — the campaign globe layer.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindPlanetView {
    base: Base<Control>,

    /// `@onready` scene handles; resolved once in `ready()` like the GDScript
    /// `$Path` members.
    space: Option<Gd<TextureRect>>,
    sector_label: Option<Gd<RichTextLabel>>,
    camera: Option<Gd<Camera3D>>,
    background: Option<Gd<MeshInstance3D>>,
    /// `PlanetRenderer.skybox` (`CubemapMesh`, `PlanetRenderer.java:36,107-121`):
    /// the star cubemap cube parented to the camera (Java zeroes `cam.position`
    /// for the skybox draw). `None` when the cubemap assets or shader are
    /// missing — the 2D `$Space` billboard stays as the fallback.
    skybox: Option<Gd<MeshInstance3D>>,
    body: Option<Gd<MeshInstance3D>>,
    atmosphere: Option<Gd<MeshInstance3D>>,
    grid: Option<Gd<MeshInstance3D>>,
    selection: Option<Gd<MeshInstance3D>>,
    hover_fill: Option<Gd<MeshInstance3D>>,
    outline: Option<Gd<MeshInstance3D>>,

    /// `planet_view` payload (grid topology, preset remap, render params).
    #[var]
    pub data: VarDictionary,
    /// Active planet content name. The property uses custom accessors because
    /// gdext's generated `#[var]` setter would be named `set_planet`, which
    /// collides with the dialog API method of the same name.
    #[var(get = get_planet, set = set_planet_value)]
    pub planet: GString,
    /// Selected grid tile index (-1 = none).
    #[var]
    pub selected_tile: i32,
    /// Hovered grid tile index (-1 = none); drives the grid `u_mouse` and the
    /// label (`PlanetDialog.hovered`).
    #[var]
    pub hover_tile: i32,
    /// `PlanetDialog.state.camPos`: the direction from the planet centre to the
    /// camera (planet space). Only direction matters — `_process` scales it by
    /// the camera distance (`PlanetRenderer.java:64-66`); drag rotates it.
    #[var]
    pub cam_pos: Vector3,
    /// Lerped camera zoom (`PlanetDialog.state.zoom`); `zoom_target` is the
    /// value the wheel and the reveal set (`PlanetDialog.zoom`).
    #[var]
    pub zoom: f32,
    #[var]
    pub zoom_target: f32,
    /// `PlanetDialog.state.uiAlpha`: the sector overlays fade when the zoom
    /// passes 1.9 (`lerpDelta(uiAlpha, zoom < 1.9 ? 1 : 0, 0.1)`).
    #[var]
    pub ui_alpha: f32,

    /// Whether the shown planet has an atmosphere billboard
    /// (`planet.hasAtmosphere`).
    has_atmosphere: bool,
    /// Values baked into the last marker rebuild (throttles the per-frame
    /// updates).
    built_ui_alpha: f32,
    built_pulse: f32,

    tiles: PackedVector3Array,
    corners: PackedVector3Array,
    tile_corners: VarArray,
    grid_material: Option<Gd<ShaderMaterial>>,
    body_material: Option<Gd<ShaderMaterial>>,
    /// `HexMesh.preRender` light in the planet mesh frame
    /// (`MindCampaign.planet_light`).
    light_dir: Vector3,
    ambient: Color,
    sector_label_text: GString,
    /// `sector_label_text` already is final Godot BBCode
    /// (`set_sector_label_markup`, the `PlanetDialog.draw` hover text) instead
    /// of a plain sector name to wrap in the `[accent][[ name ]` frame.
    sector_label_markup: bool,
    /// Preset whose icon is drawn inside the selected hex
    /// (`renderProjections`).
    sector_icon_preset: GString,
    sector_icon: Option<Gd<MeshInstance3D>>,
    /// Per-sector status layer (`renderSectors` fills/rings +
    /// `renderProjections` icons), rebuilt from the sector rows.
    markers: Option<Gd<Node3D>>,
    /// `renderOverProjections` arc layer (shield/attack/import/export arcs),
    /// a single depth-tested mesh re-filled every frame because the arc colour
    /// phase animates with `Time.globalTime`.
    arcs: Option<Gd<MeshInstance3D>>,
    /// Arc geometry resolved from the sector rows in the last marker rebuild.
    arc_specs: Vec<ArcSpec>,
    sectors: VarArray,
    /// `PlanetRenderer.renderOrbit` loop meshes (one per system planet with a
    /// parent); the shared material's alpha tracks `uiAlpha`.
    orbits: Option<Gd<Node3D>>,
    orbit_material: Option<Gd<StandardMaterial3D>>,
    /// `Planet.cloudMesh` layers (`HexSkyMesh` shells) and their `speed`s.
    clouds: Vec<Gd<MeshInstance3D>>,
    cloud_speeds: Vec<f32>,
    /// Arc `Time.globalTime` in ticks (advances while paused;
    /// `HexSkyMesh.relRot`).
    global_time: f64,
}

#[godot_api]
impl IControl for MindPlanetView {
    // A custom `init` only seeds plain fields: the base object is not bound yet
    // (PORTING.md), so every scene handle is resolved in `ready`.
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            space: None,
            sector_label: None,
            camera: None,
            background: None,
            skybox: None,
            body: None,
            atmosphere: None,
            grid: None,
            selection: None,
            hover_fill: None,
            outline: None,
            data: VarDictionary::new(),
            planet: GString::new(),
            selected_tile: -1,
            hover_tile: -1,
            cam_pos: Vector3::new(0.0, CAM_LENGTH, 0.1),
            zoom: 1.0,
            zoom_target: 1.0,
            ui_alpha: 1.0,
            has_atmosphere: false,
            built_ui_alpha: -1.0,
            built_pulse: -1.0,
            tiles: PackedVector3Array::new(),
            corners: PackedVector3Array::new(),
            tile_corners: VarArray::new(),
            grid_material: None,
            body_material: None,
            light_dir: DEFAULT_LIGHT,
            ambient: Color::from_rgba(1.0, 1.0, 1.0, 1.0),
            sector_label_text: GString::new(),
            sector_label_markup: false,
            sector_icon_preset: GString::new(),
            sector_icon: None,
            markers: None,
            arcs: None,
            arc_specs: Vec::new(),
            sectors: VarArray::new(),
            orbits: None,
            orbit_material: None,
            clouds: Vec::new(),
            cloud_speeds: Vec::new(),
            global_time: 0.0,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_visible(false);
        self.base_mut().set_process(true);

        self.space = self.child::<TextureRect>("Space");
        self.sector_label = self.child::<RichTextLabel>("SectorLabel");
        self.camera = self.child::<Camera3D>(CAMERA_PATH);
        self.background = self.child::<MeshInstance3D>(BACKGROUND_PATH);
        self.body = self.child::<MeshInstance3D>(BODY_PATH);
        self.atmosphere = self.child::<MeshInstance3D>(ATMOSPHERE_PATH);
        self.grid = self.child::<MeshInstance3D>(GRID_PATH);
        self.selection = self.child::<MeshInstance3D>(SELECTION_PATH);
        self.hover_fill = self.child::<MeshInstance3D>(HOVER_PATH);
        self.outline = self.child::<MeshInstance3D>(OUTLINE_PATH);

        let space_texture = self.load_loose("space", false);
        if let Some(mut space) = self.space.clone() {
            match &space_texture {
                Some(texture) => space.set_texture(texture),
                None => space.set_texture(Gd::<Texture2D>::null_arg()),
            }
        }
        self.build_background(space_texture);
        self.build_skybox();
    }

    /// `PlanetRenderer.java:64-66`: the camera sits at
    /// `(radius + camRadius) * camLength + (zoom - 1) * (radius + camRadius) * 2`
    /// along `cam_pos`, looking at the planet centre; `state.zoom` lerps toward
    /// the target every frame (`PlanetDialog.java:1022`).
    fn process(&mut self, delta: f64) {
        if self.data.is_empty() {
            return;
        }
        // Arc `Time.delta` is tick-scaled (1.0 at 60 fps), so Java's
        // `lerpDelta(state.zoom, zoom, 0.4f)` is a 0.4 per-frame factor; raise
        // it to `delta * 60` so the speed is framerate-independent.
        let frame = delta as f32 * 60.0;
        self.zoom += (self.zoom_target - self.zoom) * (1.0 - (1.0 - 0.4f32).powf(frame));
        let radius = row_f32(&self.data, "radius", 1.0);
        let cam_radius = row_f32(&self.data, "cam_radius", 0.0);
        let distance =
            (radius + cam_radius) * CAM_LENGTH + (self.zoom - 1.0) * (radius + cam_radius) * 2.0;
        let camera_position = self.cam_pos.normalized() * distance;
        if let Some(mut camera) = self.camera.clone() {
            camera.set_position(camera_position);
            camera.look_at(Vector3::ZERO);
        }
        // `HexMesh.preRender` runs every frame; the sun direction moves with
        // `universe.secondsf()` (orbit + spin), so re-read it.
        self.refresh_light();
        self.update_atmosphere_camera();
        // `PlanetDialog.draw`: `uiAlpha = lerpDelta(uiAlpha, zoom < 1.9 ? 1 : 0, 0.1)`
        // and `absin(Time.time, 10, 1)` are baked into the marker rebuilds (Java
        // redraws every frame; the port rebuilds when either moves).
        let target = if self.zoom < 1.9 { 1.0 } else { 0.0 };
        self.ui_alpha += (target - self.ui_alpha) * (1.0 - (1.0 - 0.1f32).powf(frame));
        // `renderOrbit`'s `alpha = params.uiAlpha` (skipped at <= 0.02).
        if let Some(mut material) = self.orbit_material.clone() {
            material.set_albedo(Color::from_rgba(
                ORBIT_GRAY.r,
                ORBIT_GRAY.g,
                ORBIT_GRAY.b,
                self.ui_alpha,
            ));
        }
        if let Some(mut orbits) = self.orbits.clone() {
            orbits.set_visible(self.ui_alpha > 0.02);
        }
        if (self.ui_alpha - self.built_ui_alpha).abs() > 0.02
            || (self.preset_pulse() - self.built_pulse).abs() > 0.04
        {
            self.rebuild_markers();
            // `PlanetDialog.renderSectors` fades the hover fill and the selected
            // sector's band/borders by `uiAlpha` too (`:489-496`), so those
            // overlays must refresh when the zoom-driven alpha moves.
            self.rebuild_hover_fill();
            self.rebuild_highlight();
        }
        self.global_time += delta * 60.0;
        // `Planet.drawArc*` cycles the gradient with `Time.globalTime`
        // (`Planet.java:655,677`); Java redraws the arcs every frame.
        if !self.arc_specs.is_empty() {
            self.rebuild_arcs();
        }
        self.update_clouds();
    }
}

#[godot_api]
impl MindPlanetView {
    // -----------------------------------------------------------------------
    // Public API (the `PlanetView` surface the dialog drives)
    // -----------------------------------------------------------------------

    /// Shows/hides the whole globe layer (the dialog owns the mode).
    #[func]
    pub fn set_active(&mut self, active: bool) {
        self.base_mut().set_visible(active);
    }

    /// Loads the planet's grid + params and builds the body/grid meshes.
    #[func]
    pub fn set_planet(&mut self, planet_name: GString) -> bool {
        let Some(mut campaign) = kit::campaign() else {
            return false;
        };
        if !campaign.has_method("planet_view") {
            return false;
        }
        let payload = campaign.call("planet_view", &[planet_name.to_variant()]);
        let Ok(data) = payload.try_to::<VarDictionary>() else {
            return false;
        };
        self.data = data;
        if self.data.is_empty() {
            return false;
        }
        self.planet = planet_name;
        self.sectors = VarArray::new();
        self.tiles = row_packed_vector3(&self.data, "tiles");
        self.corners = row_packed_vector3(&self.data, "corners");
        self.tile_corners = row_array(&self.data, "tile_corners");
        self.refresh_light();
        self.build_body();
        self.build_clouds();
        self.build_grid();
        self.build_atmosphere();
        self.build_orbits();
        self.build_universe();
        self.clear_highlight();
        true
    }

    /// `PlanetView.planet` property getter (custom accessor: gdext's generated
    /// setter would collide with the [`Self::set_planet`] dialog API method).
    #[func]
    pub fn get_planet(&self) -> GString {
        self.planet.clone()
    }

    /// `PlanetView.planet` property setter (see [`Self::get_planet`]).
    #[func]
    pub fn set_planet_value(&mut self, planet_name: GString) {
        self.planet = planet_name;
    }

    /// `PlanetDialog.renderSectors` + `renderProjections` for every sector but
    /// the selected one.
    #[func]
    pub fn set_sectors(&mut self, sectors: VarArray) {
        self.sectors = sectors;
        self.rebuild_markers();
    }

    /// Centers the camera on a tile and highlights its hex
    /// (`PlanetDialog.lookAt`). `u_mouse` stays at the origin (the planet is
    /// not hovered), so the sector grid fades out exactly like
    /// `Planet.renderSectors` with a ray that misses.
    #[func]
    pub fn focus_tile(&mut self, tile: i32) {
        self.selected_tile = tile;
        if !self.sectors.is_empty() {
            self.rebuild_markers();
        }
        if let Some(mut material) = self.grid_material.clone() {
            set_uniform(&mut material, "u_mouse", &Vector3::ZERO.to_variant());
        }
        self.set_hover(-1);
        self.rebuild_highlight();
    }

    /// `PlanetDialog.lookAt(sector)` (exact, alpha 1) / `lookAt(sector, alpha)`
    /// (slerp): turns `cam_pos` toward the sector's grid tile, keeping the
    /// length (`Planet.java:692-694`, `PlanetDialog.java:395-408`).
    #[func]
    pub fn look_at_tile(&mut self, tile: i32, #[opt(default = 1.0_f32)] alpha: f32) {
        if tile < 0 || tile as usize >= self.tiles.len() {
            return;
        }
        let target = self.tile_vec(tile).normalized();
        self.cam_pos = if alpha >= 1.0 {
            target
        } else {
            self.cam_pos.slerp(target, alpha)
        };
    }

    /// `PlanetDialog.java:114-138` (`dragged`): orbit the camera by a
    /// screen-space drag delta. `relative` is Godot's mouse-motion delta in the
    /// same y-down screen basis as Arc's `mx - lastX`/`my - lastY`;
    /// `cx / 9 * speed` around the camera up axis, `cy / 10` degrees of vertical
    /// travel clamped to 1..179.
    #[func]
    pub fn drag(&mut self, relative: Vector2) {
        let Some(camera) = self.camera.clone() else {
            return;
        };
        let up_v = self.cam_pos.angle_to(Vector3::UP).to_degrees();
        let speed = 1.0 - (up_v - 90.0).abs() / 90.0;
        let basis = camera.get_basis();
        let cam_up = basis.col_b().normalized();
        let cam_dir = -basis.col_c();
        self.cam_pos = self
            .cam_pos
            .rotated(cam_up, (relative.x / 9.0 * speed).to_radians());
        let mut amount = relative.y / 10.0;
        amount = (up_v + amount).clamp(1.0, 179.0) - up_v;
        let axis = cam_up.rotated(cam_dir, PI * 0.5);
        self.cam_pos = self.cam_pos.rotated(axis.normalized(), amount.to_radians());
    }

    /// `PlanetDialog.java:140-147` (`scrolled`): the wheel moves the target zoom
    /// within `planet.minZoom`/`planet.maxZoom`.
    #[func]
    pub fn scroll(&mut self, amount_y: f32) {
        let min_zoom = row_f32(&self.data, "min_zoom", 0.5);
        let max_zoom = row_f32(&self.data, "max_zoom", 2.0);
        self.zoom_target = (self.zoom_target + amount_y / 10.0).clamp(min_zoom, max_zoom);
    }

    /// Clears the selected sector highlight/icon without moving the camera
    /// (`PlanetDialog.viewPlanet` keeps the camera and unsets `selected`).
    #[func]
    pub fn clear_selection(&mut self) {
        self.selected_tile = -1;
        self.sector_icon_preset = GString::new();
        self.clear_highlight();
        self.rebuild_sector_icon();
        if !self.sectors.is_empty() {
            self.rebuild_markers();
        }
    }

    /// Sets the hovered tile (`PlanetDialog.hovered`): the grid fades in around
    /// it, the hex takes the accent hover fill and the dialog draws the name
    /// label.
    #[func]
    pub fn set_hover(&mut self, tile: i32) {
        self.hover_tile = tile;
        if let Some(mut material) = self.grid_material.clone() {
            let mut mouse = Vector3::ZERO;
            if tile >= 0 && (tile as usize) < self.tiles.len() {
                mouse = self.tile_vec(tile).normalized() * row_f32(&self.data, "outline", 1.17);
            }
            set_uniform(&mut material, "u_mouse", &mouse.to_variant());
        }
        self.rebuild_hover_fill();
        self.update_label();
    }

    /// Sets the preset whose icon is drawn at the selected sector
    /// (`PlanetDialog.renderProjections`: `sec.preset.uiIcon`); empty clears it.
    #[func]
    pub fn set_sector_icon(&mut self, preset: GString) {
        self.sector_icon_preset = preset;
        self.rebuild_sector_icon();
    }

    /// The in-world hover label (`PlanetDialog.hoverLabel`): `[ name ]` under the
    /// hovered sector only.
    #[func]
    pub fn set_sector_label(&mut self, text_value: GString) {
        self.sector_label_text = text_value;
        self.sector_label_markup = false;
        self.update_label();
    }

    /// Final `PlanetDialog.draw` hover text as Godot BBCode
    /// (`PlanetDialog.java:956-977`): the dialog composes the selectable name
    /// frame or the locked reason (icons included), the view only positions it.
    #[func]
    pub fn set_sector_label_markup(&mut self, text_value: GString) {
        self.sector_label_text = text_value;
        self.sector_label_markup = true;
        self.update_label();
    }

    /// Nearest `PlanetGrid` tile under a viewport point, or -1
    /// (`Planet.getSector`).
    #[func]
    pub fn pick(&self, viewport_position: Vector2) -> i32 {
        if self.tiles.is_empty() {
            return -1;
        }
        let Some(camera) = self.camera.as_ref() else {
            return -1;
        };
        let radius = row_f32(&self.data, "outline", 1.17);
        let origin = camera.project_ray_origin(viewport_position);
        let direction = camera.project_ray_normal(viewport_position);
        let b = origin.dot(direction);
        let c = origin.length_squared() - radius * radius;
        let discriminant = b * b - c;
        if discriminant < 0.0 {
            return -1;
        }
        let hit = (origin + direction * (-b - discriminant.sqrt())).normalized();
        let mut best = -1;
        let mut best_distance = 2.0;
        for index in 0..self.tiles.len() {
            let Some(tile) = self.tiles.get(index) else {
                continue;
            };
            let distance = hit.distance_squared_to(tile);
            if distance < best_distance {
                best_distance = distance;
                best = index as i32;
            }
        }
        best
    }

    /// Tile position on the render shell (world units).
    #[func]
    pub fn tile_position(&self, tile: i32) -> Vector3 {
        if tile < 0 || tile as usize >= self.tiles.len() {
            return Vector3::ZERO;
        }
        self.tile_vec(tile).normalized() * row_f32(&self.data, "outline", 1.17)
    }

    /// Projects a tile to viewport coordinates (`Planet.project`), or null
    /// offscreen.
    #[func]
    pub fn tile_to_screen(&self, tile: i32) -> Variant {
        match self.tile_to_screen_point(tile) {
            Some(point) => point.to_variant(),
            None => Variant::nil(),
        }
    }

    /// Remapped view tile for a `campaign_views` sector row
    /// (`Planet.getData().presets`).
    #[func]
    pub fn view_tile(&self, sector: VarDictionary) -> i32 {
        let presets = row_dict(&self.data, "presets");
        let preset = row_str(&sector, "preset");
        if !preset.is_empty()
            && let Some(value) = presets.get(preset.as_str())
            && let Some(index) = variant_i64(&value)
        {
            return index as i32;
        }
        row_i64(&sector, "id", -1) as i32
    }

    // -----------------------------------------------------------------------
    // Scene access
    // -----------------------------------------------------------------------

    fn child<T>(&self, path: &str) -> Option<Gd<T>>
    where
        T: GodotClass + Inherits<Node>,
    {
        self.base().get_node_or_null(path)?.try_cast::<T>().ok()
    }

    fn subviewport(&self) -> Option<Gd<SubViewport>> {
        self.child::<SubViewport>(SUBVIEWPORT_PATH)
    }

    // -----------------------------------------------------------------------
    // Background / body / grid / atmosphere / orbits / clouds
    // -----------------------------------------------------------------------

    /// The starfield as a camera-facing 3D quad inside the viewport so the
    /// additive atmosphere glow composites over it (`PlanetRenderer`'s skybox
    /// slot); the UV crop mirrors the 2D `$Space`
    /// `STRETCH_KEEP_ASPECT_COVERED` framing.
    fn build_background(&mut self, texture: Option<Gd<Texture2D>>) {
        let Some(mut background) = self.background.clone() else {
            return;
        };
        background.set_visible(false);
        let Some(texture) = texture else {
            return;
        };
        let fov = self
            .camera
            .as_ref()
            .map(|camera| camera.get_fov())
            .unwrap_or(60.0);
        let half_fov = (fov * 0.5).to_radians();
        let height = 2.0 * CAM_LENGTH * half_fov.tan();
        let width = height * BACKDROP_ASPECT;
        // code-instantiated: one backdrop billboard for the whole planet layer.
        let mut quad = QuadMesh::new_gd();
        quad.set_size(Vector2::new(width, height));
        background.set_mesh(&quad);
        let texture_size = texture.get_size();
        let cover = (width / texture_size.x).max(height / texture_size.y);
        let mut backdrop = StandardMaterial3D::new_gd();
        backdrop.set_shading_mode(ShadingMode::UNSHADED);
        backdrop.set_texture(TextureParam::ALBEDO, &texture);
        backdrop.set_texture_filter(TextureFilter::LINEAR);
        backdrop.set_cull_mode(CullMode::DISABLED);
        backdrop.set_depth_draw_mode(DepthDrawMode::DISABLED);
        backdrop.set_render_priority(-10);
        let uv_scale = Vector2::new(
            width / cover / texture_size.x,
            height / cover / texture_size.y,
        );
        backdrop.set_uv1_scale(Vector3::new(uv_scale.x, uv_scale.y, 1.0));
        backdrop.set_uv1_offset(Vector3::new(
            (1.0 - uv_scale.x) * 0.5,
            (1.0 - uv_scale.y) * 0.5,
            0.0,
        ));
        background.set_material_override(&backdrop);
        background.set_visible(true);
    }

    /// `PlanetRenderer`'s skybox (`PlanetRenderer.java:107-121`): when
    /// `params.drawSkybox` Java renders `CubemapMesh` — a unit cube scaled by
    /// `SCALE = 50` (`cubemap.vert`) whose vertex position is the cube-map
    /// direction, with `cam.position` temporarily zeroed so the cube is centred
    /// on the eye (`CubemapMesh.java:16-77`).
    ///
    /// The cube is parented to the `Camera3D` (the port's way of centring it on
    /// the eye each frame) and drawn with the ported `cubemap.gdshader`; the 2D
    /// `$Space` billboard and the 3D backdrop quad are hidden while it is live
    /// (`drawSkybox` replaces both). Asset/shader failures leave the billboard,
    /// so the view still renders (`Cubemap` faces are the same six star PNGs).
    fn build_skybox(&mut self) {
        let Some(shader) = load_shader(CUBEMAP_SHADER) else {
            return;
        };
        let Some(cubemap) = Self::load_star_cubemap() else {
            return;
        };
        let Some(mut camera) = self.camera.clone() else {
            return;
        };
        let mut material = ShaderMaterial::new_gd();
        material.set_shader(&shader);
        // `shader.setUniformi("u_cubemap", 0)` (`CubemapMesh.java:73`): the
        // samplerCube parameter; Godot binds it directly (no texture unit).
        set_uniform(&mut material, "u_cubemap", &cubemap.to_variant());
        // code-instantiated: one skybox cube for the whole planet layer
        // (Java `CubemapMesh` owns a fixed 36-vertex cube).
        let mut node = MeshInstance3D::new_alloc();
        node.clone().set_name("Skybox");
        let mut cube = BoxMesh::new_gd();
        // `CubemapMesh`'s vertices are the unit cube (`±1`); the shader applies
        // `SCALE`. Godot's `BoxMesh` is `size`-sized, so `size = 2` matches.
        cube.set_size(Vector3::new(2.0, 2.0, 2.0));
        node.clone().set_mesh(&cube);
        node.clone().set_material_override(&material);
        // The cube surrounds the eye; both faces are visible from inside via
        // the shader's `cull_disabled`.
        node.set_position(Vector3::ZERO);
        camera.add_child(&node);
        // Java draws the skybox before the planets and with the depth mask off
        // (`Gl.depthMask(false)`), so nothing occludes it and it writes no
        // depth; the shader's `depth_draw_never` is the same.
        self.skybox = Some(node);
        self.apply_skybox_visibility();
    }

    /// Hides the billboard backdrop while the cubemap skybox renders
    /// (`params.drawSkybox` off would restore it; the port always draws the
    /// skybox when the assets exist).
    fn apply_skybox_visibility(&mut self) {
        let live = self.skybox.is_some();
        let has_backdrop = self
            .background
            .as_ref()
            .map(|background| background.get_mesh().is_some())
            .unwrap_or(false);
        if let Some(mut background) = self.background.clone() {
            background.set_visible(!live && has_backdrop);
        }
        if let Some(mut space) = self.space.clone() {
            space.set_visible(!live);
        }
    }

    /// `Cubemap("cubemaps/stars/")` (`PlanetRenderer.java:36`): the six star
    /// faces, converted to one RGBA8 format (Godot requires every layer to
    /// match) and uploaded as a `Cubemap`.
    fn load_star_cubemap() -> Option<Gd<Cubemap>> {
        let dir = crate::assets::loader::resolve_assets_dir();
        let mut images = Array::<Gd<Image>>::new();
        for face in STAR_FACES {
            let path = format!("{dir}/{STAR_CUBEMAP_DIR}/{face}.png");
            if !FileAccess::file_exists(&path) {
                return None;
            }
            let bytes = FileAccess::get_file_as_bytes(&path);
            let mut image = Image::new_gd();
            if image.load_png_from_buffer(&bytes) != godot::global::Error::OK {
                return None;
            }
            // The star faces ship as RGB8; the conversion also keeps a stray
            // RGBA face from breaking `create_from_images`.
            image.convert(godot::classes::image::Format::RGBA8);
            images.push(&image);
        }
        let mut cubemap = Cubemap::new_gd();
        if cubemap.create_from_images(&images) != godot::global::Error::OK {
            return None;
        }
        Some(cubemap)
    }

    /// `PlanetRenderer.renderPlanet`/`renderTransparent`
    /// (`PlanetRenderer.java:127-168`): the solar system is drawn by recursing
    /// from `params.planet.solarSystem` over its children, so the **sun** and
    /// every other planet/moon in the system appear behind the viewed globe.
    ///
    /// The port needs the body list from the campaign facade: `planet_view`
    /// only carries the viewed planet plus its orbit rows (a radius and an
    /// orbit centre per body but no identity), so the port reads
    /// `planet_bodies(planet) -> Array[VarDictionary]` when the facade has it
    /// (see the report: `campaign.rs` must add it). Row shape:
    ///
    /// ```text
    /// { "name": "sun", "kind": "sun"|"planet",
    ///   "position": Vector3,   // body centre in the viewed planet's mesh frame
    ///   "radius": 4.0,
    ///   "rotation": 0.0 }      // Planet.getRotation() in degrees
    /// ```
    ///
    /// The sun gets its `SunMesh` (`SunMesh.java:9-25`: a `HexMesh` at
    /// divisions 4 with the unlit shader and a simplex-noise colour ramp);
    /// every other body uses the vendored `planets/<name>` art as a
    /// camera-facing billboard, the same fallback `build_body` uses for planets
    /// without a hex generator. Bodies are placed once per planet load: vanilla
    /// orbit periods are hundreds of thousands of seconds
    /// (`Planet.orbitTime = pow(radius, 1.5) * 1000`), so the Java per-frame
    /// transform update is visually static here.
    fn build_universe(&mut self) {
        let Some(mut campaign) = kit::campaign() else {
            return;
        };
        if !campaign.has_method("planet_bodies") {
            return;
        }
        let payload = campaign.call("planet_bodies", &[self.planet.to_variant()]);
        let Ok(bodies) = payload.try_to::<VarArray>() else {
            return;
        };
        let Some(mut parent) = self.subviewport() else {
            return;
        };
        for index in 0..bodies.len() {
            let Some(row) = bodies
                .get(index)
                .and_then(|value| value.try_to::<VarDictionary>().ok())
            else {
                continue;
            };
            let name = row_str(&row, "name");
            let radius = row_f32(&row, "radius", 1.0);
            let position = row_vec3(&row, "position", Vector3::ZERO);
            let kind = row_str(&row, "kind");
            let node = MeshInstance3D::new_alloc();
            node.clone().set_name(&format!("SystemBody{index}"));
            if kind == "sun" || name == "sun" {
                // `SunMesh`: `HexMesh(planet, 4, Shaders.unlit)` with the
                // noise-ramp mesher (`SunMesh.java:9-25`).
                let mesh_data = sun_mesh_data(radius);
                let Some(mesh) = upload_mesh(&mesh_data) else {
                    continue;
                };
                node.clone().set_mesh(&mesh);
                let mut material = ShaderMaterial::new_gd();
                if let Some(shader) = load_shader(UNLIT_SHADER) {
                    material.set_shader(&shader);
                }
                // Vertex colours already carry the ramp; `Shaders.unlit`
                // (`unlit.gdshader`) ignores light and ambient.
                node.clone().set_material_override(&material);
            } else {
                // Other planets/moons: vendored art billboard (the port's
                // stand-in for `HexMesh`, same as `build_body`'s fallback).
                let texture = self.load_loose(&format!("planets/{name}"), true);
                let Some(texture) = texture else {
                    continue;
                };
                let mut quad = QuadMesh::new_gd();
                // `BODY_SIZE` is the art quad for a radius-1 planet.
                quad.set_size(Vector2::new(BODY_SIZE * radius, BODY_SIZE * radius));
                node.clone().set_mesh(&quad);
                let mut billboard = StandardMaterial3D::new_gd();
                billboard.set_shading_mode(ShadingMode::UNSHADED);
                billboard.set_transparency(Transparency::ALPHA);
                billboard.set_texture(TextureParam::ALBEDO, &texture);
                billboard.set_texture_filter(TextureFilter::LINEAR);
                billboard
                    .set_billboard_mode(godot::classes::base_material_3d::BillboardMode::ENABLED);
                billboard.set_depth_draw_mode(DepthDrawMode::DISABLED);
                billboard.set_render_priority(-9);
                node.clone().set_material_override(&billboard);
            }
            node.clone().set_position(position);
            parent.add_child(&node);
        }
    }

    /// `HexMesh(planet, 6)`: the shaded hex mesh (`MeshBuilder.buildHex` with
    /// the generator colours) replaces the vendored-art billboard; the billboard
    /// stays as the fallback for planets without a hex generator (T13/PAR-0142).
    fn build_body(&mut self) {
        let mut mesh_data = VarDictionary::new();
        if let Some(mut campaign) = kit::campaign()
            && campaign.has_method("planet_mesh")
        {
            let payload = campaign.call("planet_mesh", &[self.planet.to_variant()]);
            if let Ok(value) = payload.try_to::<VarDictionary>() {
                mesh_data = value;
            }
        }
        if mesh_data.is_empty() {
            self.build_body_billboard();
            return;
        }
        let mut arrays = new_arrays();
        arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &row_packed_vector3(&mesh_data, "vertices").to_variant(),
        );
        arrays.set(
            ArrayType::NORMAL.ord() as usize,
            &row_packed_vector3(&mesh_data, "normals").to_variant(),
        );
        arrays.set(
            ArrayType::COLOR.ord() as usize,
            &row_packed_color(&mesh_data, "colors").to_variant(),
        );
        let indices = Self::godot_winding(&row_packed_i32(&mesh_data, "indices"));
        arrays.set(ArrayType::INDEX.ord() as usize, &indices.to_variant());
        let emissive = row_packed_color(&mesh_data, "emissive_colors");
        let mut array_mesh = ArrayMesh::new_gd();
        if emissive.is_empty() {
            array_mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        } else {
            // Godot wants `ARRAY_CUSTOM0` as a float/byte array, not a colour
            // array.
            let mut floats = Vec::with_capacity(emissive.len() * 4);
            for index in 0..emissive.len() {
                let Some(color) = emissive.get(index) else {
                    continue;
                };
                floats.push(color.r);
                floats.push(color.g);
                floats.push(color.b);
                floats.push(color.a);
            }
            let packed = PackedFloat32Array::from(floats);
            arrays.set(ArrayType::CUSTOM0.ord() as usize, &packed.to_variant());
            let format = u64::try_from(ArrayCustomFormat::RGBA_FLOAT.ord()).unwrap_or(0)
                << ArrayFormat::CUSTOM0_SHIFT.ord();
            let flags = ArrayFormat::CUSTOM0 | ArrayFormat::from_ord(format);
            array_mesh
                .add_surface_from_arrays_ex(PrimitiveType::TRIANGLES, &arrays)
                .flags(flags)
                .done();
        }
        let mut planet_material = ShaderMaterial::new_gd();
        if let Some(shader) = load_shader(PLANET_SHADER) {
            planet_material.set_shader(&shader);
        }
        planet_material.set_render_priority(-8);
        set_uniform(
            &mut planet_material,
            "u_lightdir",
            &self.light_dir.to_variant(),
        );
        set_uniform(
            &mut planet_material,
            "u_ambientColor",
            &vec3(self.ambient).to_variant(),
        );
        let emissive_flag = if row_bool(&mesh_data, "emissive") {
            1.0f32
        } else {
            0.0f32
        };
        set_uniform(
            &mut planet_material,
            "u_emissive",
            &emissive_flag.to_variant(),
        );
        let Some(mut body) = self.body.clone() else {
            return;
        };
        body.set_mesh(&array_mesh);
        body.set_material_override(&planet_material);
        self.body_material = Some(planet_material);
        self.update_body_camera();
        body.set_visible(true);
    }

    /// `MeshBuilder.buildHex` emits OpenGL counter-clockwise front faces and
    /// `PlanetRenderer.render` culls `Gl.back` (`PlanetRenderer.java:53-54`).
    /// Godot's front faces are clockwise, so the same index order culled the
    /// near hemisphere and the globe showed the inside of its far side. Swap the
    /// last two indices of every triangle.
    fn godot_winding(indices: &PackedInt32Array) -> PackedInt32Array {
        let source = indices.to_vec();
        let mut out = Vec::with_capacity(source.len());
        let mut start = 0usize;
        while start + 2 < source.len() {
            out.push(source[start]);
            out.push(source[start + 2]);
            out.push(source[start + 1]);
            start += 3;
        }
        out.extend_from_slice(&source[start..]);
        PackedInt32Array::from(out)
    }

    /// `Planet.drawClouds` (`PlanetRenderer.renderTransparent`,
    /// `PlanetRenderer.java:149`): each `HexSkyMesh` layer of `cloudMesh`
    /// (`MindCampaign.planet_clouds`) after the body and before the atmosphere
    /// (`render_priority` -7, -6; see ATMOSPHERE_PRIORITY).
    fn build_clouds(&mut self) {
        for node in self.clouds.drain(..) {
            let mut node = node;
            node.queue_free();
        }
        self.cloud_speeds.clear();
        let Some(mut campaign) = kit::campaign() else {
            return;
        };
        if !campaign.has_method("planet_clouds") {
            return;
        }
        let payload = campaign.call("planet_clouds", &[self.planet.to_variant()]);
        let Ok(layers) = payload.try_to::<VarArray>() else {
            return;
        };
        let shader = load_shader(CLOUDS_SHADER);
        for layer in layers.iter_shared() {
            let Ok(layer) = layer.try_to::<VarDictionary>() else {
                continue;
            };
            let mut arrays = new_arrays();
            arrays.set(
                ArrayType::VERTEX.ord() as usize,
                &row_packed_vector3(&layer, "vertices").to_variant(),
            );
            arrays.set(
                ArrayType::NORMAL.ord() as usize,
                &row_packed_vector3(&layer, "normals").to_variant(),
            );
            arrays.set(
                ArrayType::COLOR.ord() as usize,
                &row_packed_color(&layer, "colors").to_variant(),
            );
            arrays.set(
                ArrayType::INDEX.ord() as usize,
                &row_packed_i32(&layer, "indices").to_variant(),
            );
            let mut mesh = ArrayMesh::new_gd();
            mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
            let mut cloud_material = ShaderMaterial::new_gd();
            if let Some(shader) = shader.clone() {
                cloud_material.set_shader(&shader);
            }
            cloud_material.set_render_priority(-7 + self.clouds.len() as i32);
            // code-instantiated: one shell per data-driven `HexSkyMesh` layer
            // (`Planet.cloudMeshLoader`'s `MultiMesh`, Planets.java:49-52,127-130).
            let mut instance = MeshInstance3D::new_alloc();
            instance.set_name(format!("Clouds{}", self.clouds.len()).as_str());
            instance.set_mesh(&mesh);
            instance.set_material_override(&cloud_material);
            let Some(mut parent) = self.body.clone().and_then(|body| body.get_parent()) else {
                continue;
            };
            parent.add_child(&instance);
            self.clouds.push(instance);
            self.cloud_speeds.push(row_f32(&layer, "speed", 0.0));
        }
        self.update_clouds();
    }

    /// `HexSkyMesh.render`/`preRender` (`HexSkyMesh.java:45-63`): hidden while
    /// `Mathf.zero(1 - uiAlpha, 0.01)`, `u_alpha = 1 - uiAlpha` on the viewed
    /// planet, spun by `relRot = Time.globalTime * speed / 40` degrees on top of
    /// the body frame, with the light direction rotated by the same angle.
    fn update_clouds(&mut self) {
        let alpha = 1.0 - self.ui_alpha;
        let hidden = alpha.abs() <= 0.01;
        let light_dir = self.light_dir;
        let ambient = self.ambient;
        for index in 0..self.clouds.len() {
            let Some(node) = self.clouds.get(index).cloned() else {
                continue;
            };
            let mut node = node;
            node.set_visible(!hidden);
            if hidden {
                continue;
            }
            let speed = self.cloud_speeds.get(index).copied().unwrap_or(0.0);
            let rel = (self.global_time as f32 * speed / 40.0).to_radians();
            node.set_rotation(Vector3::new(0.0, rel, 0.0));
            let Some(material) = node.get_material_override() else {
                continue;
            };
            let Ok(mut material) = material.try_cast::<ShaderMaterial>() else {
                continue;
            };
            set_uniform(&mut material, "u_alpha", &alpha.to_variant());
            set_uniform(
                &mut material,
                "u_lightdir",
                &light_dir.rotated(Vector3::UP, rel).to_variant(),
            );
            set_uniform(
                &mut material,
                "u_ambientColor",
                &Vector3::new(ambient.r, ambient.g, ambient.b).to_variant(),
            );
        }
    }

    /// Vendored-art fallback (no hex generator data).
    fn build_body_billboard(&mut self) {
        let texture = self.load_loose(&format!("planets/{}", self.planet), true);
        let Some(mut body) = self.body.clone() else {
            return;
        };
        // code-instantiated: the globe quad is a data-driven billboard whose
        // scale depends on the planet radius; no static scene owns per-planet
        // meshes.
        let mut quad = QuadMesh::new_gd();
        quad.set_size(Vector2::new(BODY_SIZE, BODY_SIZE));
        body.set_mesh(&quad);
        body.set_visible(texture.is_some());
        let Some(texture) = texture else {
            return;
        };
        let mut billboard = StandardMaterial3D::new_gd();
        billboard.set_shading_mode(ShadingMode::UNSHADED);
        billboard.set_transparency(Transparency::ALPHA);
        billboard.set_texture(TextureParam::ALBEDO, &texture);
        billboard.set_texture_filter(TextureFilter::LINEAR);
        billboard.set_billboard_mode(godot::classes::base_material_3d::BillboardMode::ENABLED);
        // The opaque background is cut out by luminance alpha, so only the disc
        // writes depth and occludes the far half of the grid.
        billboard.set_depth_draw_mode(DepthDrawMode::ALWAYS);
        billboard.set_render_priority(-8);
        body.set_material_override(&billboard);
    }

    fn build_grid(&mut self) {
        let lines = row_packed_vector3(&self.data, "lines");
        if lines.is_empty() {
            return;
        }
        // `Planet.gridMeshLoader` builds the grid with `outlineColor`
        // (Pal.accent).
        let colors = PackedColorArray::from(vec![ACCENT; lines.len()]);
        let mut arrays = new_arrays();
        arrays.set(ArrayType::VERTEX.ord() as usize, &lines.to_variant());
        arrays.set(ArrayType::COLOR.ord() as usize, &colors.to_variant());
        let mut mesh = ArrayMesh::new_gd();
        mesh.add_surface_from_arrays(PrimitiveType::LINES, &arrays);
        let mut material = ShaderMaterial::new_gd();
        if let Some(shader) = load_shader(GRID_SHADER) {
            material.set_shader(&shader);
        }
        material.set_render_priority(4);
        if let Some(mut grid) = self.grid.clone() {
            grid.set_mesh(&mesh);
            grid.set_material_override(&material);
        }
        self.grid_material = Some(material);
    }

    /// `PlanetRenderer.drawAtmosphere`: additive atmospheric scattering over
    /// `MeshBuilder.buildHex(Color.white, 2, 1.5f)` — a sphere of radius 1.5
    /// that covers the `radius + atmosphereRad{In,Out}` shell. Drawn in Java's
    /// `renderTransparent` order, so the additive veil tints the limb and the
    /// rings near it; orbit lines and sector icons stay untinted.
    fn build_atmosphere(&mut self) {
        if let Some(mut atmosphere) = self.atmosphere.clone() {
            atmosphere.set_visible(false);
        }
        self.has_atmosphere = false;
        let atmos = row_dict(&self.data, "atmosphere");
        if atmos.is_empty() || !row_bool(&atmos, "has") {
            return;
        }
        // code-instantiated: one atmosphere sphere per planet view. Deviation: a
        // smooth sphere instead of the 2-subdivision hex sphere (rotationally
        // symmetric, so the shader's per-fragment ray is the same surface ray).
        let mut sphere = SphereMesh::new_gd();
        sphere.set_radius(1.5);
        sphere.set_height(3.0);
        sphere.set_radial_segments(48);
        sphere.set_rings(24);
        let mut material = ShaderMaterial::new_gd();
        if let Some(shader) = load_shader(ATMOSPHERE_SHADER) {
            material.set_shader(&shader);
        }
        material.set_render_priority(ATMOSPHERE_PRIORITY);
        let radius = row_f32(&self.data, "radius", 1.0);
        set_uniform(
            &mut material,
            "u_innerRadius",
            &(radius + row_f32(&atmos, "rad_in", 0.0)).to_variant(),
        );
        set_uniform(
            &mut material,
            "u_outerRadius",
            &(radius + row_f32(&atmos, "rad_out", 0.3)).to_variant(),
        );
        let color = row_color(&atmos, "color", Color::from_rgba(0.3, 0.7, 1.0, 1.0));
        set_uniform(&mut material, "u_color", &vec3(color).to_variant());
        // `AtmosphereShader.apply` passes `planet.getLightNormal()` against a
        // world-space camera; the port's camera lives in the planet mesh frame,
        // so the same sun direction is used in that frame.
        set_uniform(&mut material, "u_light", &self.light_dir.to_variant());
        if let Some(mut atmosphere) = self.atmosphere.clone() {
            atmosphere.set_mesh(&sphere);
            atmosphere.set_material_override(&material);
        }
        self.has_atmosphere = true;
        let visible = self.atmosphere_setting();
        if let Some(mut atmosphere) = self.atmosphere.clone() {
            atmosphere.set_visible(visible);
        }
        self.update_atmosphere_camera();
    }

    /// `PlanetRenderer.renderOrbit` (`PlanetRenderer.java:171-179`): a
    /// `lineLoop` of `(int)(radius * 10)` points around each parent position at
    /// `orbitRadius`, colour `Pal.gray` with `alpha = params.uiAlpha` (hidden
    /// below 0.02). The payload pre-computes each centre in the view frame
    /// (planet at the origin).
    fn build_orbits(&mut self) {
        if self.orbits.is_none() {
            // code-instantiated: one loop mesh per solar-system planet with a
            // parent; the count is data-driven from the system layout.
            let mut orbits = Node3D::new_alloc();
            orbits.set_name("Orbits");
            if let Some(mut viewport) = self.subviewport() {
                viewport.add_child(&orbits);
            }
            self.orbits = Some(orbits);
        }
        if let Some(mut orbits) = self.orbits.clone() {
            for child in orbits.get_children().iter_shared() {
                let mut child = child;
                orbits.remove_child(&child);
                child.queue_free();
            }
        }
        let mut material = StandardMaterial3D::new_gd();
        material.set_shading_mode(ShadingMode::UNSHADED);
        material.set_transparency(Transparency::ALPHA);
        material.set_albedo(Color::from_rgba(
            ORBIT_GRAY.r,
            ORBIT_GRAY.g,
            ORBIT_GRAY.b,
            self.ui_alpha,
        ));
        material.set_cull_mode(CullMode::DISABLED);
        material.set_render_priority(ORBIT_PRIORITY);
        let rows = row_array(&self.data, "orbits");
        for row in rows.iter_shared() {
            let Ok(row) = row.try_to::<VarDictionary>() else {
                continue;
            };
            let radius = row_f32(&row, "radius", 0.0);
            let points = (radius * 10.0) as i32;
            if points < 3 {
                continue;
            }
            let center = row_vec3(&row, "center", Vector3::ZERO);
            // `Angles.circleVectors(points, radius, ...)` with a `Gl.lineLoop`
            // (the strip repeats the first point to close).
            let mut verts = Vec::with_capacity(points as usize + 1);
            for i in 0..=points {
                let angle = TAU * (i % points) as f32 / points as f32;
                verts.push(center + Vector3::new(angle.cos() * radius, 0.0, angle.sin() * radius));
            }
            let mut arrays = new_arrays();
            arrays.set(
                ArrayType::VERTEX.ord() as usize,
                &PackedVector3Array::from(verts).to_variant(),
            );
            let mut mesh = ArrayMesh::new_gd();
            mesh.add_surface_from_arrays(PrimitiveType::LINE_STRIP, &arrays);
            let mut node = MeshInstance3D::new_alloc();
            node.set_mesh(&mesh);
            node.set_material_override(&material);
            if let Some(mut orbits) = self.orbits.clone() {
                orbits.add_child(&node);
            }
        }
        if let Some(mut orbits) = self.orbits.clone() {
            orbits.set_visible(self.ui_alpha > 0.02);
        }
        self.orbit_material = Some(material);
    }

    /// `PlanetRenderer.renderTransparent` (`PlanetRenderer.java:156`): the
    /// atmosphere draws only while `Core.settings.getBool("atmosphere")`
    /// (Graphics row, default true); read every frame like Java.
    fn atmosphere_setting(&self) -> bool {
        let Some(mut ui) = kit::ui() else {
            return true;
        };
        if !ui.has_method("settings_get_bool") {
            return true;
        }
        ui.call(
            "settings_get_bool",
            &[GString::from("atmosphere").to_variant()],
        )
        .try_to::<bool>()
        .unwrap_or(true)
    }

    fn atmosphere_material(&self) -> Option<Gd<ShaderMaterial>> {
        let atmosphere = self.atmosphere.as_ref()?;
        let material = atmosphere.get_material_override()?;
        material.try_cast::<ShaderMaterial>().ok()
    }

    /// `AtmosphereShader.apply`: `u_campos` is the camera in planet space and
    /// `u_rcampos` the camera relative to the planet (the port keeps the planet
    /// at the viewport origin, so both are the camera position). The sphere is
    /// rotation-symmetric, so no billboard rotation is needed; the space
    /// backdrop still follows the camera.
    fn update_atmosphere_camera(&mut self) {
        let visible = self.has_atmosphere && self.atmosphere_setting();
        if let Some(mut atmosphere) = self.atmosphere.clone() {
            atmosphere.set_visible(visible);
        }
        let camera = self.camera.clone();
        if let (Some(mut background), Some(camera)) = (self.background.clone(), camera.clone()) {
            background.set_rotation(camera.get_rotation());
        }
        if let (Some(mut material), Some(camera)) = (self.atmosphere_material(), camera) {
            let position = camera.get_position();
            set_uniform(&mut material, "u_campos", &position.to_variant());
            set_uniform(&mut material, "u_rcampos", &position.to_variant());
        }
        self.update_body_camera();
    }

    /// `HexMesh.preRender` (`HexMesh.java:22-27`): the sun direction in the
    /// planet mesh frame and the sun's light colour, pushed to the body and
    /// atmosphere.
    fn refresh_light(&mut self) {
        if self.planet.is_empty() {
            return;
        }
        let Some(mut campaign) = kit::campaign() else {
            return;
        };
        if !campaign.has_method("planet_light") {
            return;
        }
        let payload = campaign.call("planet_light", &[self.planet.to_variant()]);
        let Ok(light) = payload.try_to::<VarDictionary>() else {
            return;
        };
        if light.is_empty() {
            return;
        }
        self.light_dir = row_vec3(&light, "light_dir", DEFAULT_LIGHT);
        self.ambient = row_color(&light, "ambient", Color::from_rgba(1.0, 1.0, 1.0, 1.0));
        if let Some(mut material) = self.body_material.clone() {
            set_uniform(&mut material, "u_lightdir", &self.light_dir.to_variant());
            set_uniform(
                &mut material,
                "u_ambientColor",
                &vec3(self.ambient).to_variant(),
            );
        }
        if let Some(mut material) = self.atmosphere_material() {
            set_uniform(&mut material, "u_light", &self.light_dir.to_variant());
        }
    }

    /// `Shaders.PlanetShader.apply`: `u_campos` is the camera and `u_camdir` its
    /// direction. The port keeps the camera in the planet mesh frame (Java's
    /// world camera rotated by `getRotation()`), matching `u_lightdir`.
    fn update_body_camera(&mut self) {
        let Some(camera) = self.camera.clone() else {
            return;
        };
        let Some(mut material) = self.body_material.clone() else {
            return;
        };
        set_uniform(
            &mut material,
            "u_campos",
            &camera.get_global_position().to_variant(),
        );
        set_uniform(
            &mut material,
            "u_camdir",
            &(-camera.get_global_basis().col_c()).to_variant(),
        );
    }

    // -----------------------------------------------------------------------
    // Sector markers (`renderSectors` + `renderProjections`)
    // -----------------------------------------------------------------------

    fn rebuild_markers(&mut self) {
        if self.markers.is_none() {
            // code-instantiated: marker layer content is driven by the sector
            // rows.
            let mut markers = Node3D::new_alloc();
            markers.set_name("SectorMarkers");
            if let Some(mut viewport) = self.subviewport() {
                viewport.add_child(&markers);
            }
            self.markers = Some(markers);
        }
        if let Some(markers) = self.markers.clone() {
            for child in markers.get_children().iter_shared() {
                let mut child = child;
                child.queue_free();
            }
        }
        if self.tile_corners.is_empty() {
            return;
        }
        self.built_ui_alpha = self.ui_alpha;
        self.built_pulse = self.preset_pulse();
        let arad = row_f32(&self.data, "outline", 1.17) + 0.001;
        let sectors = self.sector_rows();
        let ui_alpha = self.ui_alpha;
        let mut verts: Vec<Vector3> = Vec::new();
        let mut colors: Vec<Color> = Vec::new();
        for sector in &sectors {
            let tile = self.view_tile(sector.clone());
            if tile < 0 || tile as usize >= self.tile_corners.len() || tile == self.selected_tile {
                continue;
            }
            let color = self.status_color(sector);
            if row_bool(sector, "selectable") || row_bool(sector, "unlocked") {
                if let Some(color) = color {
                    let ring =
                        Color::from_rgba(color.r * 0.8, color.g * 0.8, color.b * 0.8, ui_alpha);
                    self.append_ring(&mut verts, &mut colors, tile, arad, 0.026, ring);
                }
            } else {
                let shadow = Color::from_rgba(
                    SHADOW.r,
                    SHADOW.g,
                    SHADOW.b,
                    gamma_shadow_alpha(SHADOW.a * ui_alpha),
                );
                self.append_fill(&mut verts, &mut colors, tile, arad, shadow);
            }
            self.add_icon(sector, tile);
        }
        // `PlanetDialog.java:481-486`: the sector being played is filled with
        // `hoverColor` (`Pal.accent` at 50%) at `-0.001f`.
        let current_fill =
            Color::from_rgba(ACCENT.r, ACCENT.g, ACCENT.b, faded_alpha(0.5, ui_alpha));
        for sector in &sectors {
            if !row_bool(sector, "being_played") {
                continue;
            }
            let tile = self.view_tile(sector.clone());
            if tile < 0 || tile as usize >= self.tile_corners.len() {
                continue;
            }
            self.append_fill(&mut verts, &mut colors, tile, arad - 0.002, current_fill);
        }
        self.arc_specs = self.build_arc_specs(&sectors, ui_alpha);
        if !verts.is_empty() {
            let mut arrays = new_arrays();
            arrays.set(
                ArrayType::VERTEX.ord() as usize,
                &PackedVector3Array::from(verts).to_variant(),
            );
            arrays.set(
                ArrayType::COLOR.ord() as usize,
                &PackedColorArray::from(colors).to_variant(),
            );
            let mut mesh = ArrayMesh::new_gd();
            mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
            let mut instance = MeshInstance3D::new_alloc();
            instance.set_mesh(&mesh);
            let mut fill_material = overlay_material();
            // Depth-tested so the globe disc hides the far hemisphere.
            fill_material.set_flag(Flags::DISABLE_DEPTH_TEST, false);
            fill_material.set_render_priority(5);
            instance.set_material_override(&fill_material);
            if let Some(mut markers) = self.markers.clone() {
                markers.add_child(&instance);
            }
        }
        self.rebuild_arcs();
    }

    /// `PlanetDialog.renderOverProjections` (`PlanetDialog.java:519-543`): the
    /// shield arc, the `sectorInvasion` attack arcs and the selected sector's
    /// import/export arcs, all in the mesh frame the port renders in.
    fn build_arc_specs(&self, sectors: &[VarDictionary], ui_alpha: f32) -> Vec<ArcSpec> {
        let mut specs = Vec::new();
        if ui_alpha <= 0.001 {
            return specs;
        }
        let tile_of = |id: i64| -> Option<i32> {
            let row = sectors.iter().find(|row| row_i64(row, "id", -1) == id)?;
            let tile = self.view_tile(row.clone());
            (tile >= 0).then_some(tile)
        };
        let selected_id = sectors
            .iter()
            .find(|row| self.view_tile((*row).clone()) == self.selected_tile)
            .map(|row| row_i64(row, "id", -1))
            .filter(|id| *id >= 0);
        let selected_row =
            selected_id.and_then(|id| sectors.iter().find(|row| row_i64(row, "id", -1) == id));
        let selected_has_base = selected_row.is_some_and(|row| row_bool(row, "has_base"));
        let selected_destination =
            selected_row.and_then(|row| row_opt_i64(row, "export_destination"));
        let selected_exports = selected_row.is_some_and(|row| row_bool(row, "any_exports"));
        for row in sectors {
            let id = row_i64(row, "id", -1);
            let Some(tile) = tile_of(id) else {
                continue;
            };
            // `PlanetDialog.java:519-521`: `Team.crux.color` fading to half
            // alpha, height 0.3, 110-tick cycle, stroke 0.006.
            if let Some(target) = row_opt_i64(row, "shield_arc")
                && let Some(target_tile) = tile_of(target)
            {
                specs.push(ArcSpec {
                    a: tile,
                    b: target_tile,
                    from: with_alpha(CRUX, ui_alpha),
                    to: with_alpha(CRUX, ui_alpha * 0.5),
                    length: 0.3,
                    time_scale: 110.0,
                    stroke: 0.006,
                });
            }
            if !row_bool(row, "has_base") {
                continue;
            }
            // `PlanetDialog.java:525-531`: crux to clear, height 0.24, stroke
            // 0.005.
            for source in row_i64_array(row, "attack_sources") {
                let Some(source_tile) = tile_of(source) else {
                    continue;
                };
                specs.push(ArcSpec {
                    a: source_tile,
                    b: tile,
                    from: with_alpha(CRUX, ui_alpha),
                    to: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
                    length: 0.24,
                    time_scale: 110.0,
                    stroke: 0.005,
                });
            }
            // `PlanetDialog.java:534-542`: imports into the selected sector
            // (`Color.gray` -> accent) and exports from it (`Pal.place` ->
            // accent), height 0.4, 90-tick cycle.
            let Some(selected) = selected_id.filter(|selected| *selected != id) else {
                continue;
            };
            if !selected_has_base {
                continue;
            }
            if row_bool(row, "any_exports")
                && row_opt_i64(row, "export_destination") == Some(selected)
                && let Some(selected_tile) = tile_of(selected)
            {
                specs.push(ArcSpec {
                    a: tile,
                    b: selected_tile,
                    from: with_alpha(ARC_GRAY, ui_alpha),
                    to: with_alpha(ACCENT, ui_alpha),
                    length: 0.4,
                    time_scale: 90.0,
                    stroke: 0.0,
                });
            }
            if selected_exports
                && selected_destination == Some(id)
                && let Some(selected_tile) = tile_of(selected)
            {
                specs.push(ArcSpec {
                    a: selected_tile,
                    b: tile,
                    from: with_alpha(PLACE, ui_alpha),
                    to: with_alpha(ACCENT, ui_alpha),
                    length: 0.4,
                    time_scale: 90.0,
                    stroke: 0.0,
                });
            }
        }
        specs
    }

    /// Rebuilds the arc mesh with the current `Time.globalTime` phase
    /// (`Planet.drawArc*` animates the gradient every frame).
    fn rebuild_arcs(&mut self) {
        let Some(mut instance) = self.ensure_arcs() else {
            return;
        };
        if self.arc_specs.is_empty() {
            instance.set_mesh(Gd::<Mesh>::null_arg());
            return;
        }
        let time = self.global_time as f32;
        let mut verts: Vec<Vector3> = Vec::new();
        let mut colors: Vec<Color> = Vec::new();
        for spec in &self.arc_specs {
            self.append_arc(&mut verts, &mut colors, spec, time);
        }
        if verts.is_empty() {
            instance.set_mesh(Gd::<Mesh>::null_arg());
            return;
        }
        let mut arrays = new_arrays();
        arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(verts).to_variant(),
        );
        arrays.set(
            ArrayType::COLOR.ord() as usize,
            &PackedColorArray::from(colors).to_variant(),
        );
        let mut mesh = ArrayMesh::new_gd();
        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        let mut material = overlay_material();
        // Depth-tested so the far hemisphere/behind-the-globe arcs are hidden.
        material.set_flag(Flags::DISABLE_DEPTH_TEST, false);
        material.set_render_priority(6);
        instance.set_mesh(&mesh);
        instance.set_material_override(&material);
    }

    fn ensure_arcs(&mut self) -> Option<Gd<MeshInstance3D>> {
        if self.arcs.is_none() {
            // code-instantiated: one dynamic mesh for the projection arcs.
            let mut instance = MeshInstance3D::new_alloc();
            instance.set_name("SectorArcs");
            if let Some(mut viewport) = self.subviewport() {
                viewport.add_child(&instance);
            }
            self.arcs = Some(instance);
        }
        self.arcs.clone()
    }

    /// One quadratic-Bezier ribbon (`Planet.drawArcLine`, `Planet.java:663-690`)
    /// between two sector tiles; the geometry/colour walk lives in
    /// [`arc_ribbon`] so it is unit-testable without a Godot instance.
    fn append_arc(
        &self,
        verts: &mut Vec<Vector3>,
        colors: &mut Vec<Color>,
        spec: &ArcSpec,
        time: f32,
    ) {
        if spec.a < 0 || spec.b < 0 {
            return;
        }
        let a = self.tile_vec(spec.a).normalized();
        let b = self.tile_vec(spec.b).normalized();
        if a.length_squared() < 1e-8 || b.length_squared() < 1e-8 {
            return;
        }
        // The port renders the unit sphere, so `outlineRad * radius` is just
        // the outline shell radius.
        let outline = row_f32(&self.data, "outline", 1.17);
        let (ribbon, rainbow) = arc_ribbon(
            [a.x, a.y, a.z],
            [b.x, b.y, b.z],
            outline,
            spec.length,
            spec.time_scale,
            spec.stroke,
            [spec.from.r, spec.from.g, spec.from.b, spec.from.a],
            [spec.to.r, spec.to.g, spec.to.b, spec.to.a],
            time,
        );
        verts.extend(ribbon.iter().map(|v| Vector3::new(v[0], v[1], v[2])));
        colors.extend(
            rainbow
                .iter()
                .map(|c| Color::from_rgba(c[0], c[1], c[2], c[3])),
        );
    }

    /// `renderSectors` ring color (None = no ring).
    fn status_color(&self, sector: &VarDictionary) -> Option<Color> {
        if row_bool(sector, "has_base") {
            let mix = if row_bool(sector, "has_enemy_base") {
                0.5
            } else {
                0.0
            };
            return Some(SHARDED.lerp(CRUX, mix));
        }
        let preset = row_str(sector, "preset");
        if !preset.is_empty() && row_bool(sector, "require_unlock") {
            // `derelict.lerp(white, absin(time, 10, 1))` (`PlanetDialog.java:463`).
            return Some(if row_bool(sector, "preset_unlocked") {
                DERELICT.lerp(Color::WHITE, f64::from(self.preset_pulse()))
            } else {
                styles::ARC_GRAY
            });
        }
        if row_bool(sector, "has_enemy_base") {
            return Some(CRUX);
        }
        None
    }

    fn tile_outer(&self, tile: i32, arad: f32) -> Vec<Vector3> {
        self.tile_ids(tile)
            .iter()
            .map(|id| self.corner(*id) * arad)
            .collect()
    }

    fn append_fill(
        &self,
        verts: &mut Vec<Vector3>,
        colors: &mut Vec<Color>,
        tile: i32,
        arad: f32,
        color: Color,
    ) {
        let outers = self.tile_outer(tile, arad);
        let center = self.tile_vec(tile) * arad;
        for index in 0..outers.len() {
            verts.push(center);
            verts.push(outers[index]);
            verts.push(outers[(index + 1) % outers.len()]);
            for _ in 0..3 {
                colors.push(color);
            }
        }
    }

    fn append_ring(
        &self,
        verts: &mut Vec<Vector3>,
        colors: &mut Vec<Color>,
        tile: i32,
        arad: f32,
        stroke: f32,
        color: Color,
    ) {
        let outers = self.tile_outer(tile, arad);
        let center = self.tile_vec(tile) * arad;
        let mut inners = Vec::with_capacity(outers.len());
        for outer in &outers {
            let offset = *outer - center;
            inners.push(center + offset.normalized() * (offset.length() - stroke));
        }
        for index in 0..outers.len() {
            let next = (index + 1) % outers.len();
            verts.push(outers[index]);
            verts.push(outers[next]);
            verts.push(inners[next]);
            verts.push(outers[index]);
            verts.push(inners[next]);
            verts.push(inners[index]);
            for _ in 0..6 {
                colors.push(color);
            }
        }
    }

    /// `renderProjections` icon for one sector.
    ///
    /// Java has no facing cull here: each icon lies flat on the sector's plane
    /// (`planets.drawPlane` -> `Planet.setPlane`), so icons near the limb are
    /// seen edge-on and those behind the globe fail the depth test.
    fn add_icon(&mut self, sector: &VarDictionary, tile: i32) {
        let preset = row_str(sector, "preset");
        let has_base = row_bool(sector, "has_base");
        let mut glyph = String::new();
        let mut region = String::new();
        let mut color = Color::WHITE;
        if row_bool(sector, "attacked") {
            glyph = String::from("warn");
            color = SHARDED;
        } else if !has_base
            && !preset.is_empty()
            && row_bool(sector, "require_unlock")
            && row_bool(sector, "preset_unlocked")
            && row_str(sector, "icon").is_empty()
            && row_str(sector, "content_icon").is_empty()
        {
            region = format!("sector-{preset}");
        } else if row_bool(sector, "lock_icon") {
            glyph = String::from("lock");
        } else {
            // `Sector.icon()`: contentIcon, info.icon, else an unlocked preset's
            // icon.
            let content_icon = row_str(sector, "content_icon");
            let icon = row_str(sector, "icon");
            if !content_icon.is_empty() {
                region = content_icon;
            } else if !icon.is_empty() {
                glyph = icon;
            } else if !preset.is_empty()
                && row_bool(sector, "require_unlock")
                && row_bool(sector, "preset_unlocked")
            {
                region = format!("sector-{preset}");
            }
        }
        let plane = self.sector_plane(tile);
        let ui_alpha = self.ui_alpha;
        if !region.is_empty() {
            let mut texture = kit::icon_texture(&region);
            if texture.is_none() && !region.starts_with("sector-") {
                for prefix in ["item", "block", "unit", "liquid"] {
                    texture = kit::icon_texture(&format!("{prefix}-{region}-ui"));
                    if texture.is_some() {
                        break;
                    }
                }
            }
            let Some(texture) = texture else {
                return;
            };
            // code-instantiated: one sector-plane sprite per marked sector.
            let mut sprite = Sprite3D::new_alloc();
            sprite.set_texture(&texture);
            let width = (texture.get_width() as f32).max(1.0);
            sprite.set_pixel_size(SECTOR_ICON_SIZE / width);
            sprite.set_texture_filter(TextureFilter::LINEAR);
            sprite.set_render_priority(ICON_PRIORITY);
            sprite.set_transform(plane);
            sprite.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, ui_alpha));
            if let Some(mut markers) = self.markers.clone() {
                markers.add_child(&sprite);
            }
            return;
        }
        let text = kit::glyph(&glyph);
        if text.is_empty() {
            return;
        }
        // code-instantiated: one icon-font glyph per marked sector.
        let mut label = Label3D::new_alloc();
        label.set_text(&text);
        if let Some(assets) = kit::assets()
            && let Some(font) = assets.bind().icon_font()
        {
            let font: Gd<Font> = font.upcast();
            label.set_font(&font);
        }
        label.set_font_size(64);
        label.set_outline_size(0);
        label.set_pixel_size(SECTOR_ICON_SIZE / 64.0);
        label.set_modulate(Color::from_rgba(
            color.r,
            color.g,
            color.b,
            color.a * ui_alpha,
        ));
        label.set_render_priority(ICON_PRIORITY);
        label.set_transform(plane);
        if let Some(mut markers) = self.markers.clone() {
            markers.add_child(&label);
        }
    }

    /// `Planet.setPlane` (`Planet.java:700-712`) in the mesh frame the port
    /// renders in (Java's `rotate(Y, -getRotation())` and `+ position` cancel
    /// out): origin `tile.v * (outlineRad + 0.01) * radius`; up = `Vec3.Y`
    /// projected onto the sector plane (`Sector.plane` through corners 0/2/4,
    /// `Sector.java:271`) minus `tile.v * radius`; right = up turned by Arc's
    /// `rotate(tile.v, 90)` (a clockwise quarter turn seen from outside). The
    /// local +Z faces outward, so a Sprite3D/Label3D/QuadMesh drawn in its XY
    /// plane matches `PlaneBatch3D`.
    fn sector_plane(&self, tile: i32) -> Transform3D {
        let radius = row_f32(&self.data, "radius", 1.0);
        let v = self.tile_vec(tile);
        let origin = v.normalized() * (row_f32(&self.data, "outline", 1.17) + 0.01 * radius);
        let mut up = Vector3::UP;
        let ids = self.tile_ids(tile);
        if ids.len() >= 5 {
            let p1 = self.corner(ids[0]);
            let p2 = self.corner(ids[2]);
            let p3 = self.corner(ids[4]);
            let normal = (p1 - p2).cross(p2 - p3).normalized();
            let mut top = v + Vector3::UP;
            top -= normal * (normal.dot(top) - p1.dot(normal));
            up = top - v * radius;
        }
        if up.length_squared() < 1e-10 {
            up = Vector3::UP;
        }
        let up = up.normalized();
        // Arc `Vec3.rotate(axis, 90)` is Godot's `rotated(axis, -PI / 2)`.
        let right = up.rotated(v.normalized(), -PI * 0.5).normalized();
        let basis = Basis::from_cols(right, up, right.cross(up).normalized());
        Transform3D::new(basis, origin)
    }

    // -----------------------------------------------------------------------
    // Hover fill / selection highlight / sector icon
    // -----------------------------------------------------------------------

    /// `PlanetDialog.draw`: `planets.fill(hovered, hoverColor.write().a(0.5f),
    /// -0.003f)` paints the hovered hex with the accent at 50% just under the
    /// shell.
    fn rebuild_hover_fill(&mut self) {
        if let Some(mut hover) = self.hover_fill.clone() {
            hover.set_mesh(Gd::<Mesh>::null_arg());
        }
        if self.hover_tile < 0 || self.hover_tile as usize >= self.tile_corners.len() {
            return;
        }
        let ids = self.tile_ids(self.hover_tile);
        let arad = row_f32(&self.data, "outline", 1.17) - 0.003;
        let center = if (self.hover_tile as usize) < self.tiles.len() {
            self.tile_vec(self.hover_tile) * arad
        } else {
            Vector3::ZERO
        };
        let corners: Vec<Vector3> = ids.iter().map(|id| self.corner(*id) * arad).collect();
        // `hoverColor = Pal.accent.a(0.5f)` (`PlanetRenderer.java:19`) faded by
        // `mulA(state.uiAlpha)` (`PlanetDialog.java:489`).
        let accent_fill = Color::from_rgba(
            ACCENT.r,
            ACCENT.g,
            ACCENT.b,
            faded_alpha(0.5, self.ui_alpha),
        );
        let mut fan: Vec<Vector3> = Vec::new();
        let mut colors: Vec<Color> = Vec::new();
        for index in 0..corners.len() {
            fan.push(center);
            fan.push(corners[index]);
            fan.push(corners[(index + 1) % corners.len()]);
            for _ in 0..3 {
                colors.push(accent_fill);
            }
        }
        let mut arrays = new_arrays();
        arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(fan).to_variant(),
        );
        arrays.set(
            ArrayType::COLOR.ord() as usize,
            &PackedColorArray::from(colors).to_variant(),
        );
        let mut mesh = ArrayMesh::new_gd();
        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        let mut material = overlay_material();
        material.set_render_priority(5);
        if let Some(mut hover) = self.hover_fill.clone() {
            hover.set_mesh(&mesh);
            hover.set_material_override(&material);
        }
    }

    fn rebuild_highlight(&mut self) {
        self.clear_highlight();
        if self.selected_tile < 0 || self.selected_tile as usize >= self.tile_corners.len() {
            return;
        }
        let ids = self.tile_ids(self.selected_tile);
        // `PlanetRenderer.drawSelection(selected, uiAlpha)` draws a stroke band
        // `(outlineRad + 0.001) * radius` wide `0.04` toward the tile center, in
        // Pal.accent; `drawBorders` re-draws the hex outline in accent at 30%.
        let arad = row_f32(&self.data, "outline", 1.17) + 0.001;
        let center = if (self.selected_tile as usize) < self.tiles.len() {
            self.tile_vec(self.selected_tile) * arad
        } else {
            Vector3::ZERO
        };
        let stroke = 0.04;
        let mut outers: Vec<Vector3> = Vec::new();
        let mut inners: Vec<Vector3> = Vec::new();
        for id in &ids {
            let outer = self.corner(*id) * arad;
            outers.push(outer);
            let offset = outer - center;
            inners.push(center + offset.normalized() * (offset.length() - stroke));
        }

        // Band ring (`Planet.drawSelection`: quad(outer_i, outer_i+1,
        // inner_i+1, inner_i)).
        let mut band: Vec<Vector3> = Vec::new();
        let mut band_colors: Vec<Color> = Vec::new();
        // `drawSelection(selected, state.uiAlpha)` (`PlanetDialog.java:495`)
        // tints the band with `Pal.accent` at alpha `uiAlpha`.
        let band_color = Color::from_rgba(
            ACCENT.r,
            ACCENT.g,
            ACCENT.b,
            faded_alpha(1.0, self.ui_alpha),
        );
        for index in 0..outers.len() {
            let next = (index + 1) % outers.len();
            band.push(outers[index]);
            band.push(outers[next]);
            band.push(inners[next]);
            band.push(outers[index]);
            band.push(inners[next]);
            band.push(inners[index]);
            band_colors.extend(std::iter::repeat_n(band_color, 6));
        }
        let mut band_arrays = new_arrays();
        band_arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(band).to_variant(),
        );
        band_arrays.set(
            ArrayType::COLOR.ord() as usize,
            &PackedColorArray::from(band_colors).to_variant(),
        );
        let mut band_mesh = ArrayMesh::new_gd();
        band_mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &band_arrays);
        if let Some(mut selection) = self.selection.clone() {
            selection.set_mesh(&band_mesh);
            selection.set_material_override(&overlay_material());
        }
        self.rebuild_sector_icon();

        // Outline loop (`drawBorders`, `borderColor` = accent at 30%),
        // `borderColor.write.cpy().a((base.a + 0.3f) * alpha)` with
        // `alpha = state.uiAlpha` (`Planet.java:563-564`, `PlanetDialog.java:496`).
        let mut outline_loop: Vec<Vector3> = Vec::new();
        let mut loop_colors: Vec<Color> = Vec::new();
        let edge = Color::from_rgba(
            ACCENT.r,
            ACCENT.g,
            ACCENT.b,
            faded_alpha(0.3, self.ui_alpha),
        );
        for index in 0..outers.len() {
            outline_loop.push(outers[index]);
            outline_loop.push(outers[(index + 1) % outers.len()]);
            for _ in 0..2 {
                loop_colors.push(edge);
            }
        }
        let mut loop_arrays = new_arrays();
        loop_arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(outline_loop).to_variant(),
        );
        loop_arrays.set(
            ArrayType::COLOR.ord() as usize,
            &PackedColorArray::from(loop_colors).to_variant(),
        );
        let mut loop_mesh = ArrayMesh::new_gd();
        loop_mesh.add_surface_from_arrays(PrimitiveType::LINES, &loop_arrays);
        if let Some(mut outline) = self.outline.clone() {
            outline.set_mesh(&loop_mesh);
            outline.set_material_override(&overlay_material());
        }
    }

    fn clear_highlight(&mut self) {
        if let Some(mut selection) = self.selection.clone() {
            selection.set_mesh(Gd::<Mesh>::null_arg());
        }
        if let Some(mut outline) = self.outline.clone() {
            outline.set_mesh(Gd::<Mesh>::null_arg());
        }
        if let Some(mut icon) = self.sector_icon.clone() {
            icon.set_mesh(Gd::<Mesh>::null_arg());
        }
    }

    /// The selected sector's icon (`PlanetDialog.renderProjections`: every
    /// visible sector draws its preset icon; the port draws the selected one).
    fn rebuild_sector_icon(&mut self) {
        if self.sector_icon.is_none() {
            // code-instantiated: one data-driven sector-plane quad for the
            // selected sector.
            let mut icon = MeshInstance3D::new_alloc();
            icon.set_name("SectorIcon");
            icon.set_mesh(Gd::<Mesh>::null_arg());
            if let Some(mut viewport) = self.subviewport() {
                viewport.add_child(&icon);
            }
            self.sector_icon = Some(icon);
        }
        let mut texture: Option<Gd<Texture2D>> = None;
        if !self.sector_icon_preset.is_empty() {
            texture = kit::icon_texture(&format!("sector-{}", self.sector_icon_preset));
        }
        let missing = texture.is_none()
            || self.selected_tile < 0
            || self.selected_tile as usize >= self.tiles.len();
        if missing {
            if let Some(mut icon) = self.sector_icon.clone() {
                icon.set_mesh(Gd::<Mesh>::null_arg());
            }
            return;
        }
        let Some(texture) = texture else {
            return;
        };
        // `renderProjections` `iw = 64f/4f` at the `PlaneBatch3D` scaling
        // (1/150), flat on the sector plane (`Planet.setPlane`) like every other
        // icon.
        let mut quad = QuadMesh::new_gd();
        quad.set_size(Vector2::new(SECTOR_ICON_SIZE, SECTOR_ICON_SIZE));
        let plane = self.sector_plane(self.selected_tile);
        let Some(mut icon) = self.sector_icon.clone() else {
            return;
        };
        icon.set_mesh(&quad);
        icon.set_transform(plane);
        let mut material = StandardMaterial3D::new_gd();
        material.set_shading_mode(ShadingMode::UNSHADED);
        material.set_transparency(Transparency::ALPHA);
        material.set_texture(TextureParam::ALBEDO, &texture);
        material.set_texture_filter(TextureFilter::LINEAR);
        material.set_cull_mode(CullMode::DISABLED);
        material.set_render_priority(ICON_PRIORITY);
        icon.set_material_override(&material);
    }

    // -----------------------------------------------------------------------
    // Label / tile geometry
    // -----------------------------------------------------------------------

    /// `Mathf.absin(Time.time, 10f, 1f)` (`PlanetDialog.java:463`): a 10 s
    /// `|sin|` pulse for unlocked, still-locked presets.
    fn preset_pulse(&self) -> f32 {
        let seconds = Time::singleton().get_ticks_msec() as f32 / 1000.0;
        (seconds / 10.0 * TAU).sin().abs()
    }

    fn update_label(&mut self) {
        let Some(mut label) = self.sector_label.clone() else {
            return;
        };
        // `PlanetDialog.draw`: the label exists only for `hovered != null`; the
        // selected sector alone draws no name.
        let point = if self.hover_tile >= 0 && !self.sector_label_text.is_empty() {
            self.tile_to_screen_point(self.hover_tile)
        } else {
            None
        };
        let Some(point) = point else {
            label.set_visible(false);
            return;
        };
        // `PlanetDialog.draw`: `[accent][lb] [white]<name>[accent] ]` — gold
        // brackets with the name in white (`Styles.outlineLabel` carries the
        // black outline via the scene's theme overrides). The dialog's
        // `set_sector_label_markup` hands over the finished hover text instead.
        let text = if self.sector_label_markup {
            self.sector_label_text.to_string()
        } else {
            format!(
                "[color=#ffd37f][lb] [color=white]{}[color=#ffd37f] ]",
                self.sector_label_text
            )
        };
        label.set_text(&GString::from(text.as_str()));
        label.set_visible(true);
        label.reset_size();
        let size = label.get_size();
        label.set_position(point - size * 0.5);
    }

    fn tile_to_screen_point(&self, tile: i32) -> Option<Vector2> {
        if tile < 0 || tile as usize >= self.tiles.len() {
            return None;
        }
        let camera = self.camera.as_ref()?;
        let point = camera.unproject_position(self.tile_position(tile));
        let viewport_size = camera.get_viewport()?.get_visible_rect().size;
        if point.x < 0.0 || point.y < 0.0 || point.x > viewport_size.x || point.y > viewport_size.y
        {
            return None;
        }
        Some(point)
    }

    fn sector_rows(&self) -> Vec<VarDictionary> {
        let mut out = Vec::new();
        for row in self.sectors.iter_shared() {
            if let Ok(dict) = row.try_to::<VarDictionary>() {
                out.push(dict);
            }
        }
        out
    }

    fn tile_vec(&self, tile: i32) -> Vector3 {
        if tile < 0 {
            return Vector3::ZERO;
        }
        self.tiles.get(tile as usize).unwrap_or(Vector3::ZERO)
    }

    fn corner(&self, id: i32) -> Vector3 {
        if id < 0 {
            return Vector3::ZERO;
        }
        self.corners.get(id as usize).unwrap_or(Vector3::ZERO)
    }

    fn tile_ids(&self, tile: i32) -> Vec<i32> {
        if tile < 0 {
            return Vec::new();
        }
        let Some(value) = self.tile_corners.get(tile as usize) else {
            return Vec::new();
        };
        value
            .try_to::<PackedInt32Array>()
            .map(|ids| ids.to_vec())
            .unwrap_or_default()
    }

    fn load_loose(&self, key: &str, cutout: bool) -> Option<Gd<Texture2D>> {
        let mut assets = kit::assets()?;
        let mut guard = assets.bind_mut();
        let texture = if cutout {
            guard.find_loose_cutout(GString::from(key))?
        } else {
            guard.find_loose(GString::from(key))?
        };
        Some(texture.upcast())
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// `SunMesh.java:9-25`: a `HexMesh` whose mesher keeps a flat sphere
/// (`getHeight` 0) and colours each tile from a simplex-noise ramp
/// (`Math.pow(Simplex.noise3d(0, 5, 0.3, 1.7, x, y, z), 1.2) * 1`, indexed
/// into the six Java colours and scaled by `colorScale` 1.1).
pub struct SunMesher {
    /// `SunMesh`'s six ramp colours (`Planets.java:29-43`).
    colors: [[f32; 4]; 6],
    /// `SunMesh`'s `colorScale` (`Planets.java:37`).
    color_scale: f32,
}

impl Default for SunMesher {
    fn default() -> Self {
        Self {
            colors: [
                color_hex(0xff7a38),
                color_hex(0xff9638),
                color_hex(0xffc64c),
                color_hex(0xffc64c),
                color_hex(0xffe371),
                color_hex(0xf4ee8e),
            ],
            color_scale: 1.1,
        }
    }
}

impl mind_core::maps::generators::planet::HexMesher for SunMesher {
    /// `getColor`: `height = pow(Simplex.noise3d(0, 5, 0.3, 1.7, pos), 1.2)`,
    /// then `colors[clamp((int)(height * len), 0, len - 1)] * colorScale`.
    fn get_color(&self, position: [f32; 3]) -> [f32; 4] {
        let noise = mind_core::math::noise::noise3d(
            0,
            5,
            0.3,
            1.7,
            position[0] as f64,
            position[1] as f64,
            position[2] as f64,
        );
        let height = (f64::from(noise).powf(1.2) * 1.0) as f32;
        let index = ((height * self.colors.len() as f32) as i32)
            .clamp(0, self.colors.len() as i32 - 1) as usize;
        let color = self.colors[index];
        [
            (color[0] * self.color_scale).min(1.0),
            (color[1] * self.color_scale).min(1.0),
            (color[2] * self.color_scale).min(1.0),
            1.0,
        ]
    }
}

/// Builds the `SunMesh` geometry: `MeshBuilder.buildHex(SunMesh's mesher, 4,
/// radius, 1)` (`HexMesh` divisions 4, `SunMesh` constructor).
pub fn sun_mesh_data(radius: f32) -> mind_core::render::g3d::MeshData {
    let mut mesher = SunMesher::default();
    mind_core::render::g3d::mesh_data::build_hex(&mut mesher, 4, radius, 1.0)
}

/// Uploads a `MeshData` (`MeshBuilder` arrays) as an `ArrayMesh`.
fn upload_mesh(mesh_data: &mind_core::render::g3d::MeshData) -> Option<Gd<ArrayMesh>> {
    let mut arrays = new_arrays();
    let mut vertices = PackedVector3Array::new();
    for vertex in &mesh_data.vertices {
        vertices.push(Vector3::new(vertex[0], vertex[1], vertex[2]));
    }
    let mut normals = PackedVector3Array::new();
    for normal in &mesh_data.normals {
        normals.push(Vector3::new(normal[0], normal[1], normal[2]));
    }
    let mut colors = PackedColorArray::new();
    for packed in &mesh_data.colors {
        colors.push(Color::from_rgba8(
            (packed >> 24) as u8,
            (packed >> 16) as u8,
            (packed >> 8) as u8,
            *packed as u8,
        ));
    }
    arrays.set(ArrayType::VERTEX.ord() as usize, &vertices.to_variant());
    arrays.set(ArrayType::NORMAL.ord() as usize, &normals.to_variant());
    arrays.set(ArrayType::COLOR.ord() as usize, &colors.to_variant());
    if mesh_data.indexed && !mesh_data.indices.is_empty() {
        let mut indices = PackedInt32Array::new();
        for index in &mesh_data.indices {
            indices.push(*index as i32);
        }
        arrays.set(ArrayType::INDEX.ord() as usize, &indices.to_variant());
    }
    let mut mesh = ArrayMesh::new_gd();
    mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
    Some(mesh)
}

/// `Color.valueOf(hex)` (Arc `Color.valueOf`), opaque.
fn color_hex(hex: u32) -> [f32; 4] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    ]
}

/// Allocates a `Mesh.ARRAY_MAX`-sized mesh-array list.
fn new_arrays() -> VarArray {
    let mut arrays = VarArray::new();
    arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
    arrays
}

/// Sets one shader uniform (uniform names are pass-through from the ported
/// GDScript).
fn set_uniform(material: &mut Gd<ShaderMaterial>, name: &str, value: &Variant) {
    material.set_shader_parameter(&StringName::from(name), value);
}

fn load_shader(path: &str) -> Option<Gd<Shader>> {
    let mut loader = ResourceLoader::singleton();
    let resource = loader.load(&GString::from(path))?;
    resource.try_cast::<Shader>().ok()
}

/// `Planet.drawArcLine`'s ribbon (`Planet.java:663-690`): quadratic Bezier
/// `[b*r, midpoint, a*r]` with the midpoint height raised by
/// `1 + length + dot*1.35` in the port's unit-sphere mesh frame, a per-point
/// colour phase `(f + Time.globalTime / timeScale) % 1`, and a `±stroke`
/// ribbon. Godot-free (plain arrays) so the walk is unit-testable without an
/// engine instance and the hot loop never crosses the FFI for `Color::lerp`.
#[allow(clippy::too_many_arguments)]
fn arc_ribbon(
    a: [f32; 3],
    b: [f32; 3],
    outline: f32,
    length: f32,
    time_scale: f32,
    stroke: f32,
    from: [f32; 4],
    to: [f32; 4],
    time: f32,
) -> (Vec<[f32; 3]>, Vec<[f32; 4]>) {
    let mut verts: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let dot3 = |u: [f32; 3], v: [f32; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let len = |v: [f32; 3]| dot3(v, v).sqrt();
    let normalize = |v: [f32; 3]| {
        let l = len(v);
        if l > 1e-8 {
            [v[0] / l, v[1] / l, v[2] / l]
        } else {
            [0.0, 0.0, 0.0]
        }
    };
    let a = normalize(a);
    let b = normalize(b);
    if len(a) < 1e-8 || len(b) < 1e-8 {
        return (verts, colors);
    }
    let dot = 1.0 - (dot3(a, b) + 1.0) * 0.5;
    let mut avg = [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ];
    if len(avg) < 1e-6 {
        // Antipodal tiles: Arc's `setLength(0)` yields a degenerate midpoint;
        // pick any perpendicular so the curve stays defined.
        let up = [0.0, 1.0, 0.0];
        avg = [
            a[1] * up[2] - a[2] * up[1],
            a[2] * up[0] - a[0] * up[2],
            a[0] * up[1] - a[1] * up[0],
        ];
        if len(avg) < 1e-8 {
            let right = [1.0, 0.0, 0.0];
            avg = [
                a[1] * right[2] - a[2] * right[1],
                a[2] * right[0] - a[0] * right[2],
                a[0] * right[1] - a[1] * right[0],
            ];
        }
    }
    let p0 = [b[0] * outline, b[1] * outline, b[2] * outline];
    let height = 1.0 + length + dot * 1.35;
    let mid = normalize(avg);
    let p1 = [mid[0] * height, mid[1] * height, mid[2] * height];
    let p2 = [a[0] * outline, a[1] * outline, a[2] * outline];
    let u = [p0[0] - p1[0], p0[1] - p1[1], p0[2] - p1[2]];
    let v = [p1[0] - p2[0], p1[1] - p2[1], p1[2] - p2[2]];
    let normal = normalize([
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]);
    if len(normal) < 1e-8 {
        return (verts, colors);
    }
    // `drawArc` is a 1 px `Gl.lineStrip`; render it as a thin ribbon.
    let stroke = stroke.max(0.0015);
    let count = ARC_POINTS.max(1);
    let lerp_color = |f: f32| {
        let phase = (f + time / time_scale.max(1.0)) % 1.0;
        [
            from[0] + (to[0] - from[0]) * phase,
            from[1] + (to[1] - from[1]) * phase,
            from[2] + (to[2] - from[2]) * phase,
            from[3] + (to[3] - from[3]) * phase,
        ]
    };
    let mut prev_top = [0.0, 0.0, 0.0];
    let mut prev_bottom = [0.0, 0.0, 0.0];
    let mut prev_color = [0.0, 0.0, 0.0, 0.0];
    for index in 0..=count {
        let f = index as f32 / count as f32;
        let t = 1.0 - f;
        let point = [
            p0[0] * (t * t) + p1[0] * (2.0 * t * f) + p2[0] * (f * f),
            p0[1] * (t * t) + p1[1] * (2.0 * t * f) + p2[1] * (f * f),
            p0[2] * (t * t) + p1[2] * (2.0 * t * f) + p2[2] * (f * f),
        ];
        let color = lerp_color(f);
        let top = [
            point[0] + normal[0] * stroke,
            point[1] + normal[1] * stroke,
            point[2] + normal[2] * stroke,
        ];
        let bottom = [
            point[0] - normal[0] * stroke,
            point[1] - normal[1] * stroke,
            point[2] - normal[2] * stroke,
        ];
        if index > 0 {
            verts.push(prev_top);
            verts.push(prev_bottom);
            verts.push(top);
            colors.push(prev_color);
            colors.push(prev_color);
            colors.push(color);
            verts.push(top);
            verts.push(prev_bottom);
            verts.push(bottom);
            colors.push(color);
            colors.push(prev_color);
            colors.push(color);
        }
        prev_top = top;
        prev_bottom = bottom;
        prev_color = color;
    }
    (verts, colors)
}

/// `PlanetView._overlay_material()`: unshaded vertex-colour alpha overlay.
fn overlay_material() -> Gd<StandardMaterial3D> {
    let mut overlay = StandardMaterial3D::new_gd();
    overlay.set_shading_mode(ShadingMode::UNSHADED);
    overlay.set_flag(Flags::ALBEDO_FROM_VERTEX_COLOR, true);
    overlay.set_transparency(Transparency::ALPHA);
    overlay.set_cull_mode(CullMode::DISABLED);
    overlay.set_flag(Flags::DISABLE_DEPTH_TEST, true);
    overlay.set_render_priority(6);
    overlay
}

/// A `Color` handed to a plain `vec3` uniform is converted sRGB -> linear by
/// Godot, which darkened Java's raw `setUniformf(r, g, b)` values (the
/// atmosphere lost most of its red/green and turned a dim blue). Pass the
/// components verbatim.
fn vec3(color: Color) -> Vector3 {
    Vector3::new(color.r, color.g, color.b)
}

/// `PlanetView._gamma_shadow_alpha`.
fn gamma_shadow_alpha(alpha: f32) -> f32 {
    1.0 - (1.0 - alpha.clamp(0.0, 1.0)).powf(2.2)
}

// ---------------------------------------------------------------------------
// Read-model field accessors (`planet_view`/`campaign_views` dictionaries)
// ---------------------------------------------------------------------------

/// A non-null field of a read-model row.
fn value(row: &VarDictionary, key: &str) -> Option<Variant> {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
}

fn variant_string(value: &Variant) -> Option<String> {
    if let Ok(text) = value.try_to::<GString>() {
        return Some(text.to_string());
    }
    if let Ok(name) = value.try_to::<StringName>() {
        return Some(name.to_string());
    }
    None
}

fn variant_f32(value: &Variant) -> Option<f32> {
    if let Ok(number) = value.try_to::<f32>() {
        return Some(number);
    }
    if let Ok(number) = value.try_to::<f64>() {
        return Some(number as f32);
    }
    value.try_to::<i64>().ok().map(|number| number as f32)
}

fn variant_i64(value: &Variant) -> Option<i64> {
    if let Ok(number) = value.try_to::<i64>() {
        return Some(number);
    }
    variant_f32(value).map(|number| number as i64)
}

/// Optional string field of a read-model row (`null` -> "").
fn row_str(row: &VarDictionary, key: &str) -> String {
    value(row, key)
        .and_then(|value| variant_string(&value))
        .unwrap_or_default()
}

fn row_bool(row: &VarDictionary, key: &str) -> bool {
    value(row, key)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(false)
}

fn row_i64(row: &VarDictionary, key: &str, default: i64) -> i64 {
    value(row, key)
        .and_then(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// Optional id field of a read-model row (`null`/missing -> `None`).
fn row_opt_i64(row: &VarDictionary, key: &str) -> Option<i64> {
    value(row, key).and_then(|value| variant_i64(&value))
}

/// Integer-array field of a read-model row (`attack_sources`).
fn row_i64_array(row: &VarDictionary, key: &str) -> Vec<i64> {
    row_array(row, key)
        .iter_shared()
        .filter_map(|value| variant_i64(&value))
        .collect()
}

/// `color` with its alpha replaced (`Color.write().a(alpha)`).
fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::from_rgba(color.r, color.g, color.b, alpha.clamp(0.0, 1.0))
}

fn row_f32(row: &VarDictionary, key: &str, default: f32) -> f32 {
    value(row, key)
        .and_then(|value| variant_f32(&value))
        .unwrap_or(default)
}

fn row_vec3(row: &VarDictionary, key: &str, default: Vector3) -> Vector3 {
    value(row, key)
        .and_then(|value| value.try_to::<Vector3>().ok())
        .unwrap_or(default)
}

fn row_color(row: &VarDictionary, key: &str, default: Color) -> Color {
    value(row, key)
        .and_then(|value| value.try_to::<Color>().ok())
        .unwrap_or(default)
}

fn row_dict(row: &VarDictionary, key: &str) -> VarDictionary {
    value(row, key)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// A non-null array field of a read-model row.
///
/// The campaign read-model producers hand GDScript-shaped **typed** arrays
/// (`Array[PackedInt32Array]` for `tile_corners`, `Array[Dictionary]` for
/// `orbits`), while JSON-born rows are untyped. gdext 0.5.5 refuses to convert
/// a typed array to `Array<Variant>` (`ElementType::is_compatible_with`), which
/// silently left `PlanetView.tile_corners` empty and dead-ended every overlay
/// build (`rebuild_highlight`/`rebuild_hover_fill`/`rebuild_markers`) on the
/// bounds check — PAR-0018. Read the runtime array type through `AnyArray` and
/// rebuild an untyped array element by element; this keeps both producer shapes
/// working without changing the UI ABI.
fn row_array(row: &VarDictionary, key: &str) -> VarArray {
    let Some(value) = value(row, key) else {
        return VarArray::new();
    };
    if let Ok(array) = value.try_to::<VarArray>() {
        return array;
    }
    let Ok(array) = value.try_to::<AnyArray>() else {
        return VarArray::new();
    };
    let mut out = VarArray::new();
    for element in array.iter_shared() {
        out.push(&element);
    }
    out
}

fn row_packed_vector3(row: &VarDictionary, key: &str) -> PackedVector3Array {
    value(row, key)
        .and_then(|value| value.try_to::<PackedVector3Array>().ok())
        .unwrap_or_default()
}

fn row_packed_color(row: &VarDictionary, key: &str) -> PackedColorArray {
    value(row, key)
        .and_then(|value| value.try_to::<PackedColorArray>().ok())
        .unwrap_or_default()
}

fn row_packed_i32(row: &VarDictionary, key: &str) -> PackedInt32Array {
    value(row, key)
        .and_then(|value| value.try_to::<PackedInt32Array>().ok())
        .unwrap_or_default()
}

/// `Color.a * state.uiAlpha` for the sector overlays the dialog fades while
/// zooming (`PlanetDialog.renderSectors`: `hoverColor.mulA(uiAlpha)` at
/// `PlanetDialog.java:489`, `drawSelection(selected, uiAlpha)` at `:495`,
/// `drawBorders(..., uiAlpha)` at `:490/:496`). Clamped like a colour channel.
fn faded_alpha(base: f32, ui_alpha: f32) -> f32 {
    (base * ui_alpha).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{ARC_POINTS, STAR_FACES, SunMesher, arc_ribbon, faded_alpha, sun_mesh_data};
    use mind_core::maps::generators::planet::HexMesher;

    /// Java's overlay alphas: `hoverColor = Pal.accent.a(0.5f)` and
    /// `borderColor = Pal.accent.a(0.3f)` (`PlanetRenderer.java:19`), the
    /// selection band `Pal.accent` at alpha `uiAlpha`
    /// (`PlanetRenderer.drawSelection`, `:228-230`), each multiplied by the
    /// zoom-driven `state.uiAlpha` (`PlanetDialog.java:489-496`).
    #[test]
    fn sector_overlay_alpha_scales_with_ui_alpha() {
        assert_eq!(faded_alpha(0.5, 1.0), 0.5);
        assert_eq!(faded_alpha(0.5, 0.0), 0.0);
        assert_eq!(faded_alpha(0.3, 0.5), 0.15);
        assert_eq!(faded_alpha(1.0, 0.25), 0.25);
        assert_eq!(faded_alpha(1.0, 2.0), 1.0);
        assert_eq!(faded_alpha(1.0, -1.0), 0.0);
    }

    /// PAR-0493: `Planet.drawArcLine`'s quadratic Bezier ribbon — ARC_POINTS
    /// strips of 6 vertices, endpoints on the outline shell, the midpoint
    /// raised by `1 + length + dot*1.35`, and the animated from/to gradient.
    #[test]
    fn arc_ribbon_walks_the_bezier() {
        let from = [1.0, 0.0, 0.0, 1.0];
        let to = [0.0, 0.0, 1.0, 0.0];
        let (verts, colors) = arc_ribbon(
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            1.17,
            0.3,
            110.0,
            0.006,
            from,
            to,
            0.0,
        );
        assert_eq!(verts.len(), ARC_POINTS as usize * 6);
        assert_eq!(colors.len(), verts.len());
        let mean = verts.iter().fold([0.0_f32; 3], |acc, vertex| {
            [acc[0] + vertex[0], acc[1] + vertex[1], acc[2] + vertex[2]]
        });
        let mean = [
            mean[0] / verts.len() as f32,
            mean[1] / verts.len() as f32,
            mean[2] / verts.len() as f32,
        ];
        let mean_len = (mean[0] * mean[0] + mean[1] * mean[1] + mean[2] * mean[2]).sqrt();
        assert!(mean_len > 1.17, "the curve arcs above the outline shell");
        // a and b are symmetric, so the apex sits along their average direction.
        assert!(
            (mean[0] - mean[1]).abs() < 0.05,
            "the apex lies between a and b"
        );
        let max_len = verts
            .iter()
            .map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt())
            .fold(0.0_f32, f32::max);
        assert!(
            max_len > 1.17 * 1.15,
            "the midpoint control point lifts the curve above the shell"
        );
        assert_eq!(colors[0], from, "f=0 starts the gradient at `from`");
        assert!(colors[3][2] > 0.0, "the phase lerps toward the end colour");
        // Antipodal endpoints keep a finite ribbon (the degenerate-midpoint guard).
        let (antipodal, antipodal_colors) = arc_ribbon(
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            1.17,
            0.3,
            110.0,
            0.006,
            from,
            to,
            0.0,
        );
        assert_eq!(antipodal.len(), ARC_POINTS as usize * 6);
        assert_eq!(antipodal_colors.len(), antipodal.len());
        assert!(
            antipodal
                .iter()
                .all(|vertex| vertex.iter().all(|value| value.is_finite()))
        );
    }

    /// `SunMesh` (`SunMesh.java:9-25`): a `HexMesh` at divisions 4 — a flat
    /// sphere with the noise-ramp colours from `Planets.java:29-43`.
    #[test]
    fn sun_mesh_is_a_flat_noise_coloured_sphere() {
        let mesh = sun_mesh_data(4.0);
        assert!(!mesh.vertices.is_empty(), "sun mesh vertices");
        assert!(!mesh.indices.is_empty(), "sun mesh indices");
        assert!(mesh.indexed);
        assert!(mesh.emissive.is_empty(), "SunMesh is not emissive");
        assert_eq!(mesh.vertices.len(), mesh.colors.len());
        // `getHeight` is 0, so every vertex sits exactly on the radius-4
        // sphere.
        for vertex in &mesh.vertices {
            let length =
                (vertex[0] * vertex[0] + vertex[1] * vertex[1] + vertex[2] * vertex[2]).sqrt();
            assert!((length - 4.0).abs() < 1e-3, "{vertex:?} off the sphere");
        }
        // Every colour is one of the six ramp colours scaled by `colorScale`
        // 1.1 (clamped at 1) — never black and never out of range.
        for packed in &mesh.colors {
            let r = (packed >> 24) as u8;
            let g = (packed >> 16) as u8;
            assert!(r > 100 && g > 50, "warm ramp colour {packed:08x}");
        }
    }

    /// `SunMesh`'s mesher: `getColor` indexes the ramp by
    /// `pow(noise, 1.2) * colors.length` and multiplies by `colorScale`.
    #[test]
    fn sun_mesher_colors_stay_on_the_ramp() {
        let mesher = SunMesher::default();
        for position in [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.577, 0.577, 0.577],
        ] {
            let color = mesher.get_color(position);
            for channel in &color[0..3] {
                assert!((0.0..=1.0).contains(channel), "{color:?}");
            }
            assert_eq!(color[3], 1.0);
        }
    }

    /// `Cubemap(String base)` loads `right, left, top, bottom, front, back`
    /// (`Cubemap.java:29-38`), which is Godot's
    /// `Cubemap.create_from_images` order (+X, -X, +Y, -Y, +Z, -Z) — the
    /// skybox face order must not be reordered accidentally.
    #[test]
    fn star_cubemap_faces_are_in_arc_order() {
        assert_eq!(
            STAR_FACES,
            ["right", "left", "top", "bottom", "front", "back"]
        );
    }
}

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
    ArrayMesh, Camera3D, Control, Font, IControl, Label3D, Mesh, MeshInstance3D, Node, Node3D,
    QuadMesh, ResourceLoader, RichTextLabel, Shader, ShaderMaterial, SphereMesh, Sprite3D,
    StandardMaterial3D, SubViewport, Texture2D, TextureRect, Time,
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

const PLANET_SHADER: &str = "res://shaders/planet.gdshader";
const CLOUDS_SHADER: &str = "res://shaders/clouds.gdshader";
const GRID_SHADER: &str = "res://shaders/planetgrid.gdshader";
const ATMOSPHERE_SHADER: &str = "res://shaders/atmosphere.gdshader";

const SUBVIEWPORT_PATH: &str = "ViewportContainer/SubViewport";
const CAMERA_PATH: &str = "ViewportContainer/SubViewport/Camera";
const BACKGROUND_PATH: &str = "ViewportContainer/SubViewport/Background";
const BODY_PATH: &str = "ViewportContainer/SubViewport/Body";
const ATMOSPHERE_PATH: &str = "ViewportContainer/SubViewport/Atmosphere";
const GRID_PATH: &str = "ViewportContainer/SubViewport/Grid";
const SELECTION_PATH: &str = "ViewportContainer/SubViewport/Selection";
const HOVER_PATH: &str = "ViewportContainer/SubViewport/Hover";
const OUTLINE_PATH: &str = "ViewportContainer/SubViewport/Outline";

/// `PlanetView` — the campaign globe layer.
#[derive(GodotClass)]
#[class(base=Control, rename = RustPlanetView)]
pub struct MindPlanetView {
    base: Base<Control>,

    /// `@onready` scene handles; resolved once in `ready()` like the GDScript
    /// `$Path` members.
    space: Option<Gd<TextureRect>>,
    sector_label: Option<Gd<RichTextLabel>>,
    camera: Option<Gd<Camera3D>>,
    background: Option<Gd<MeshInstance3D>>,
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
    /// Preset whose icon is drawn inside the selected hex
    /// (`renderProjections`).
    sector_icon_preset: GString,
    sector_icon: Option<Gd<MeshInstance3D>>,
    /// Per-sector status layer (`renderSectors` fills/rings +
    /// `renderProjections` icons), rebuilt from the sector rows.
    markers: Option<Gd<Node3D>>,
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
            sector_icon_preset: GString::new(),
            sector_icon: None,
            markers: None,
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
        }
        self.global_time += delta * 60.0;
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
        if verts.is_empty() {
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
        let accent_fill = Color::from_rgba(ACCENT.r, ACCENT.g, ACCENT.b, 0.5);
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
        for index in 0..outers.len() {
            let next = (index + 1) % outers.len();
            band.push(outers[index]);
            band.push(outers[next]);
            band.push(inners[next]);
            band.push(outers[index]);
            band.push(inners[next]);
            band.push(inners[index]);
            band_colors.extend(std::iter::repeat_n(ACCENT, 6));
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

        // Outline loop (`drawBorders`, `borderColor` = accent at 30%).
        let mut outline_loop: Vec<Vector3> = Vec::new();
        let mut loop_colors: Vec<Color> = Vec::new();
        let edge = Color::from_rgba(ACCENT.r, ACCENT.g, ACCENT.b, 0.3);
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
        // black outline via the scene's theme overrides).
        let text = format!(
            "[color=#ffd37f][lb] [color=white]{}[color=#ffd37f] ]",
            self.sector_label_text
        );
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

fn row_array(row: &VarDictionary, key: &str) -> VarArray {
    value(row, key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
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

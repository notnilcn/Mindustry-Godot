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
//! topology) and the typed `CampaignViews.sectors` markers pushed through
//! `set_sector_views`; `planet_light`, `planet_mesh` and `planet_clouds` come
//! from the same facade, and `MindUi` supplies the `atmosphere` graphics
//! setting.
//!
//! The class has no signals: the GDScript base it ports defines none, and the
//! driving `PlanetDialog` controller polls the read-only properties instead.
//! Registered as `RustPlanetView` while the GDScript `class_name PlanetView`
//! still exists (PORT-STATUS.md §"Temporary registration names").

mod mesh;
mod overlays;
mod rows;
mod scene;
#[cfg(test)]
mod tests;

use std::f32::consts::PI;

use godot::classes::{
    Camera3D, Control, IControl, MeshInstance3D, Node3D, RichTextLabel, ShaderMaterial,
    StandardMaterial3D, Texture2D, TextureRect,
};
use godot::prelude::*;

use crate::ui::dialogs::campaign::util::empty_sector_view;
use crate::ui::kit;
use mind_core::ui::campaign::SectorView;

use self::mesh::set_uniform;
use self::rows::{
    row_array, row_bool, row_dict, row_f32, row_i64, row_i64_array, row_opt_i64,
    row_packed_vector3, row_str, variant_i64,
};

// The old `planet::SunMesher`/`planet::sun_mesh_data` paths stay valid for C01.
#[allow(unused_imports)]
pub use self::mesh::{SunMesher, sun_mesh_data};

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
    /// The typed `SectorView` rows driving the overlay layers.
    sector_views: Vec<SectorView>,
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
            sector_views: Vec::new(),
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
        self.sector_views = Vec::new();
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
    ///
    /// # ABI shim
    ///
    /// The untyped `#[func]` form is the MCP/test surface; the campaign dialog
    /// pushes [`Self::set_sector_views`] instead. Each row is projected into the
    /// typed view with the fields the overlay layers read.
    #[func]
    pub fn set_sectors(&mut self, sectors: VarArray) {
        let views: Vec<SectorView> = sectors
            .iter_shared()
            .filter_map(|row| row.try_to::<VarDictionary>().ok())
            .map(|row| sector_view_from_row(&row))
            .collect();
        self.set_sector_views(&views);
    }

    /// Binds the typed `CampaignViews.sectors` markers (T01/T02 seam).
    pub fn set_sector_views(&mut self, sectors: &[SectorView]) {
        self.sector_views = sectors.to_vec();
        self.rebuild_markers();
    }

    /// Centers the camera on a tile and highlights its hex
    /// (`PlanetDialog.lookAt`). `u_mouse` stays at the origin (the planet is
    /// not hovered), so the sector grid fades out exactly like
    /// `Planet.renderSectors` with a ray that misses.
    #[func]
    pub fn focus_tile(&mut self, tile: i32) {
        self.selected_tile = tile;
        if !self.sector_views.is_empty() {
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
        if !self.sector_views.is_empty() {
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
    ///
    /// # ABI shim
    ///
    /// The untyped `#[func]` form is the MCP/test surface; the dialog calls
    /// [`Self::view_tile_index`] with the typed view's fields.
    #[func]
    pub fn view_tile(&self, sector: VarDictionary) -> i32 {
        let preset = row_str(&sector, "preset");
        let id = row_i64(&sector, "id", -1);
        self.view_tile_index(id, &preset)
    }

    /// The remapped grid tile for a typed sector marker.
    pub fn view_tile_index(&self, id: i64, preset: &str) -> i32 {
        let presets = row_dict(&self.data, "presets");
        if !preset.is_empty()
            && let Some(value) = presets.get(preset)
            && let Some(index) = variant_i64(&value)
        {
            return index as i32;
        }
        id as i32
    }
}

/// Projects one untyped `campaign_views_json` sector row into [`SectorView`]
/// (the `#[func] set_sectors` ABI path). Only the fields the overlay layers read
/// are taken from the row; the rest keep [`empty_sector_view`]'s defaults.
fn sector_view_from_row(row: &VarDictionary) -> SectorView {
    let planet = row_str(row, "planet");
    let name = row_str(row, "name");
    let id = u16::try_from(row_i64(row, "id", -1)).unwrap_or(0);
    let mut view = empty_sector_view(id, &planet, &name);
    view.preset = Some(row_str(row, "preset")).filter(|preset| !preset.is_empty());
    view.selectable = row_bool(row, "selectable");
    view.unlocked = row_bool(row, "unlocked");
    view.locked = row_bool(row, "locked");
    view.require_unlock = row_bool(row, "require_unlock");
    view.preset_unlocked = row_bool(row, "preset_unlocked");
    view.lock_icon = row_bool(row, "lock_icon");
    view.icon = Some(row_str(row, "icon")).filter(|icon| !icon.is_empty());
    view.content_icon = Some(row_str(row, "content_icon")).filter(|icon| !icon.is_empty());
    view.has_base = row_bool(row, "has_base");
    view.has_enemy_base = row_bool(row, "has_enemy_base");
    view.attacked = row_bool(row, "attacked");
    view.being_played = row_bool(row, "being_played");
    view.any_exports = row_bool(row, "any_exports");
    view.export_destination =
        row_opt_i64(row, "export_destination").and_then(|id| u16::try_from(id).ok());
    view.shield_arc = row_opt_i64(row, "shield_arc").and_then(|id| u16::try_from(id).ok());
    view.attack_sources = row_i64_array(row, "attack_sources")
        .into_iter()
        .filter_map(|id| u16::try_from(id).ok())
        .collect();
    view
}

// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Scene build for the planet view: background, skybox, universe, body, clouds, grid, atmosphere and light.
//! Upstream: core/src/mindustry/graphics/g3d/PlanetRenderer.java

use std::f32::consts::TAU;

use godot::classes::base_material_3d::{
    CullMode, DepthDrawMode, ShadingMode, TextureFilter, TextureParam, Transparency,
};
use godot::classes::mesh::{ArrayCustomFormat, ArrayFormat, ArrayType, PrimitiveType};
use godot::classes::{
    ArrayMesh, BoxMesh, Cubemap, FileAccess, Image, MeshInstance3D, Node, Node3D, QuadMesh,
    ShaderMaterial, SphereMesh, StandardMaterial3D, SubViewport, Texture2D,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use super::mesh::{load_shader, new_arrays, set_uniform, sun_mesh_data, upload_mesh, vec3};
use super::rows::{
    row_array, row_bool, row_color, row_dict, row_f32, row_packed_color, row_packed_i32,
    row_packed_vector3, row_str, row_vec3,
};
use super::*;

impl MindPlanetView {
    // -----------------------------------------------------------------------
    // Scene access
    // -----------------------------------------------------------------------

    pub(super) fn child<T>(&self, path: &str) -> Option<Gd<T>>
    where
        T: GodotClass + Inherits<Node>,
    {
        self.base().get_node_or_null(path)?.try_cast::<T>().ok()
    }

    pub(super) fn subviewport(&self) -> Option<Gd<SubViewport>> {
        self.child::<SubViewport>(SUBVIEWPORT_PATH)
    }

    // -----------------------------------------------------------------------
    // Background / body / grid / atmosphere / orbits / clouds
    // -----------------------------------------------------------------------

    /// The starfield as a camera-facing 3D quad inside the viewport so the
    /// additive atmosphere glow composites over it (`PlanetRenderer`'s skybox
    /// slot); the UV crop mirrors the 2D `$Space`
    /// `STRETCH_KEEP_ASPECT_COVERED` framing.
    pub(super) fn build_background(&mut self, texture: Option<Gd<Texture2D>>) {
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
    pub(super) fn build_skybox(&mut self) {
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
    pub(super) fn apply_skybox_visibility(&mut self) {
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
    pub(super) fn load_star_cubemap() -> Option<Gd<Cubemap>> {
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
    pub(super) fn build_universe(&mut self) {
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
    pub(super) fn build_body(&mut self) {
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
    pub(super) fn godot_winding(indices: &PackedInt32Array) -> PackedInt32Array {
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
    pub(super) fn build_clouds(&mut self) {
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
    pub(super) fn update_clouds(&mut self) {
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
    pub(super) fn build_body_billboard(&mut self) {
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

    pub(super) fn build_grid(&mut self) {
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
    pub(super) fn build_atmosphere(&mut self) {
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
    pub(super) fn build_orbits(&mut self) {
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
    pub(super) fn atmosphere_setting(&self) -> bool {
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

    pub(super) fn atmosphere_material(&self) -> Option<Gd<ShaderMaterial>> {
        let atmosphere = self.atmosphere.as_ref()?;
        let material = atmosphere.get_material_override()?;
        material.try_cast::<ShaderMaterial>().ok()
    }

    /// `AtmosphereShader.apply`: `u_campos` is the camera in planet space and
    /// `u_rcampos` the camera relative to the planet (the port keeps the planet
    /// at the viewport origin, so both are the camera position). The sphere is
    /// rotation-symmetric, so no billboard rotation is needed; the space
    /// backdrop still follows the camera.
    pub(super) fn update_atmosphere_camera(&mut self) {
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
    pub(super) fn refresh_light(&mut self) {
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
    pub(super) fn update_body_camera(&mut self) {
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
}

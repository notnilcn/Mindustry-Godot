// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Planet view data: the solar-system bodies, the light/orbit read-outs and the
//! hex mesh payload.
//! Upstream: graphics/g3d/PlanetRenderer.java; graphics/g3d/HexMesh.java; graphics/g3d/HexSkyMesh.java; game/Universe.java

use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Live `{turn, seconds}` of the universe (T1 seam; `Universe.java:293-296`).
    ///
    /// Reads the runtime copy while a sector is installed, else the facade
    /// (`"{}"` with no campaign).
    #[func]
    pub fn universe_json(&self) -> GString {
        // The runtime copy while a sector is installed, else the facade copy.
        self.read_live_campaign(|campaign| {
            serde_json::json!({
                "turn": campaign.universe.turn,
                "seconds": campaign.universe.seconds,
            })
        })
        .map_or_else(empty_object, |value| value_gstring(&value))
    }

    /// `Shaders.planet` lighting for the globe (`HexMesh.preRender`,
    /// `HexMesh.java:22-27`): `light_dir` is
    /// `(solarSystem.position - position).rotate(Y, getRotation()).nor()` in
    /// the planet's mesh frame at the current `universe.secondsf()`, and
    /// `ambient` is `solarSystem.lightColor`. Empty for an unknown planet.
    #[func]
    pub fn planet_light(&self, planet: GString) -> VarDictionary {
        let mut out = VarDictionary::new();
        let Some(registry) = self.registry.as_ref() else {
            return out;
        };
        let Some(def) = planet_def(registry, &planet.to_string()) else {
            return out;
        };
        let index = def.id.index();
        let light = mind_core::render::g3d::planet_light(
            registry.planets(),
            index,
            self.universe_secondsf(),
        );
        let [x, y, z] = light.light_dir;
        let [r, g, b] = light.ambient;
        out.set("light_dir", Vector3::new(x, y, z));
        out.set("ambient", Color::from_rgb(r, g, b));
        out.set("rotation", light.rotation);
        out
    }

    /// The other bodies of the viewed planet's solar system for the planet
    /// view (`PlanetRenderer.renderPlanet`, `PlanetRenderer.java:139-153`):
    /// the sun and every other visible planet/moon, positioned in the viewed
    /// planet's mesh frame (the frame `planet_mesh`/`planet_view` use, where
    /// the viewed globe sits at the origin).
    ///
    /// Rows: `{name, kind: "sun"|"planet", position: Vector3, radius,
    /// rotation}`. The viewed planet itself is excluded (its globe is drawn
    /// separately), and bodies of other solar systems are skipped
    /// (`solarSystem`, `Planet.java:220`). Empty for an unknown planet or
    /// before content boot.
    #[func]
    pub fn planet_bodies(&mut self, planet: GString) -> VarArray {
        let mut out = VarArray::new();
        let seconds = self.universe_secondsf();
        let Some(registry) = self.registry.as_ref() else {
            return out;
        };
        let Some(def) = planet_def(registry, &planet.to_string()) else {
            return out;
        };
        for row in planet_body_rows(registry.planets(), def.id.index(), seconds) {
            let mut dict = VarDictionary::new();
            dict.set("name", row.name.as_str());
            dict.set("kind", if row.sun { "sun" } else { "planet" });
            dict.set(
                "position",
                Vector3::new(row.position[0], row.position[1], row.position[2]),
            );
            dict.set("radius", row.radius);
            dict.set("rotation", row.rotation);
            out.push(&dict.to_variant());
        }
        out
    }

    /// Planet globe data for the sector view (plan 16 g3d seam): the
    /// `PlanetGrid` sector topology, the render-plane grid-line mesh
    /// (`MeshBuilder.buildPlanetGrid` at `outlineRad * radius`) and the
    /// `assets/planets/<name>.json` preset → sector-index remap.
    ///
    /// Returns an empty dictionary for an unknown planet or before content
    /// boot.
    #[func]
    pub fn planet_view(&mut self, planet: GString) -> VarDictionary {
        let mut out = VarDictionary::new();
        let name = planet.to_string();
        let Some(registry) = self.registry.as_ref() else {
            return out;
        };
        let Some(def) = planet_def(registry, &name) else {
            return out;
        };

        let grid = PlanetGrid::create(def.sector_tiles as usize);
        let outline = OUTLINE_RAD * def.radius;

        let tile_points: Vec<[f32; 3]> = grid.tiles.iter().map(|tile| tile.v).collect();
        let tiles = vec3_array(&tile_points);
        let corner_points: Vec<[f32; 3]> = grid.corners.iter().map(|corner| corner.v).collect();
        let corners = vec3_array(&corner_points);
        let mut tile_corners = Array::<PackedInt32Array>::new();
        for tile in &grid.tiles {
            let mut ids = PackedInt32Array::new();
            for corner in &tile.corners {
                ids.push(*corner as i32);
            }
            tile_corners.push(&ids);
        }

        let line_mesh = build_planet_grid(&grid, u32::MAX, outline);
        let lines = vec3_array(&line_mesh.vertices);

        let mut presets = VarDictionary::new();
        let path = format!(
            "{}/planets/{name}.json",
            crate::assets::loader::resolve_assets_dir()
        );
        if FileAccess::file_exists(&path) {
            let text = FileAccess::get_file_as_string(&path).to_string();
            match serde_json::from_str::<serde_json::Value>(&mind_core::io::json::quote_bare_keys(
                &text,
            )) {
                Ok(value) => {
                    if let Some(entries) = value.get("presets").and_then(|v| v.as_object()) {
                        for (preset, index) in entries {
                            if let Some(index) = index.as_u64() {
                                presets.set(preset.as_str(), index as i64);
                            }
                        }
                    }
                }
                Err(error) => log::warn!("[campaign] planet data {path}: {error}"),
            }
        }

        let icon = def.icon_color;
        out.set("name", name);
        out.set("grid_size", def.sector_tiles as i64);
        out.set("radius", def.radius);
        out.set("outline", outline);
        out.set("cam_radius", def.cam_radius);
        out.set("start_sector", def.start_sector as i64);
        // `Planet.maxZoom = 2f` (`Planet.java:56`); `Planet.getLastSector`
        // (`Planet.java:312-317`) reads `<name>-last-sector` through
        // `Core.settings`, clamped to the grid.
        out.set("min_zoom", def.min_zoom);
        out.set("max_zoom", 2.0f32);
        let last_sector = settings_hub::with_settings(|settings| {
            settings
                .get_i32(
                    &format!("{}-last-sector", def.name),
                    def.start_sector as i32,
                )
                .clamp(0, def.sector_count.saturating_sub(1) as i32)
        });
        out.set("last_sector", last_sector as i64);
        out.set(
            "icon_color",
            Color::from_rgba(icon.r, icon.g, icon.b, icon.a),
        );
        // `PlanetRenderer.drawAtmosphere` uniforms: radii are
        // `planet.radius + atmosphereRad{In,Out}`.
        let atmosphere = def.atmosphere_color;
        let mut atmos = VarDictionary::new();
        atmos.set("has", def.has_atmosphere);
        atmos.set(
            "color",
            Color::from_rgba(atmosphere.r, atmosphere.g, atmosphere.b, atmosphere.a),
        );
        atmos.set("rad_in", def.atmosphere_rad_in);
        atmos.set("rad_out", def.atmosphere_rad_out);
        out.set("atmosphere", &atmos);
        // `PlanetRenderer.renderOrbit` (`PlanetRenderer.java:171-179`): every
        // visible planet with a parent draws a `lineLoop` of `radius * 10`
        // points around its parent's position at `orbitRadius`, colour
        // `Pal.gray` with alpha = `params.uiAlpha`, skipped when
        // `!planet.drawOrbit`. The port's view frame keeps the current planet
        // at the origin in its mesh frame, so each centre is
        // `(parent.position - planet.position).rotate(Y, getRotation())`.
        let planets = registry.planets();
        let orbit = mind_core::render::g3d::orbit::orbit_radii(planets);
        let seconds = self.universe_secondsf();
        let system = mind_core::render::g3d::orbit::solar_system(planets, def.id.index());
        let mut orbits = Array::<VarDictionary>::new();
        for (index, body) in planets.iter().enumerate() {
            let Some(parent) = body.parent else {
                continue;
            };
            if !body.visible || !body.draw_orbit {
                continue;
            }
            if mind_core::render::g3d::orbit::solar_system(planets, index) != system {
                continue;
            }
            let radius = orbit.get(index).copied().unwrap_or(0.0);
            if radius <= 0.0 {
                continue;
            }
            // In the viewed planet's mesh frame (the frame the port renders the
            // globe and places the camera in), not raw world space: Java's
            // planet + camera are rotated by `getRotation()` while the orbit
            // stays in world space (L03 T2, PAR-0420).
            let center = mind_core::render::g3d::orbit::orbit_center_in_planet_frame(
                planets,
                &orbit,
                parent.index(),
                def.id.index(),
                seconds,
            );
            let mut row = VarDictionary::new();
            row.set("radius", radius);
            row.set("center", Vector3::new(center[0], center[1], center[2]));
            orbits.push(&row);
        }
        out.set("orbits", &orbits);
        out.set("tiles", &tiles);
        out.set("corners", &corners);
        out.set("tile_corners", &tile_corners);
        out.set("lines", &lines);
        out.set("presets", &presets);
        out
    }

    /// `Planet.cloudMesh` (`MultiMesh(HexSkyMesh ..)`, Planets.java:49-52,
    /// 127-130): one dictionary per layer with the hex shell arrays
    /// (`vertices`/`normals`/`colors`/`indices`) and `speed` (`relRot =
    /// Time.globalTime * speed / 40` degrees, HexSkyMesh.java:40-42). Empty
    /// for planets without clouds. `Planet.drawClouds` (PlanetRenderer.java:149)
    /// is the planet view's per-frame draw.
    #[func]
    pub fn planet_clouds(&self, planet: GString) -> VarArray {
        let mut out = VarArray::new();
        let Some(def) = self
            .registry
            .as_ref()
            .and_then(|registry| planet_def(registry, &planet.to_string()))
        else {
            return out;
        };
        for layer in &def.cloud_layers {
            let mesh = layer.build(def.radius);
            let vertices = vec3_array(&mesh.vertices);
            let normals = vec3_array(&mesh.normals);
            let mut colors = PackedColorArray::new();
            for packed in &mesh.colors {
                colors.push(rgba8(*packed));
            }
            let mut indices = PackedInt32Array::new();
            for index in &mesh.indices {
                indices.push(*index as i32);
            }
            let mut entry = VarDictionary::new();
            entry.set("vertices", &vertices);
            entry.set("normals", &normals);
            entry.set("colors", &colors);
            entry.set("indices", &indices);
            entry.set("speed", layer.speed);
            out.push(&entry.to_variant());
        }
        out
    }

    /// Hex planet mesh for the globe (`HexMesh.java:11`:
    /// `MeshBuilder.buildHex(planet.generator, 6, planet.radius, 0.2f)`):
    /// vertices, flat normals, per-tile colors (`Block.mapColor` with
    /// `1 - albedo` alpha) and the emissive channel (`CUSTOM0`, `isEmissive()`
    /// planets only). Empty for planets without a hex generator.
    #[func]
    pub fn planet_mesh(&mut self, planet: GString) -> VarDictionary {
        use mind_core::maps::generators::planet::HexMesher;

        let mut out = VarDictionary::new();
        let name = planet.to_string();
        if let Some((cached_name, cached)) = &self.planet_mesh_cache
            && cached_name == &name
        {
            return cached.clone();
        }
        let Some(registry) = self.registry.as_ref() else {
            return out;
        };
        let Some(def) = planet_def(registry, &name) else {
            return out;
        };
        let mut mesher: Box<dyn HexMesher> = match name.as_str() {
            "serpulo" => {
                let mut generator = mind_core::maps::planet::SerpuloPlanetGenerator::new();
                generator.load_content(registry, &crate::render::minimap::load_block_colors());
                // `getEmissiveColor` + the megabase patch read the live sector
                // states (`Planets.serpulo.sectors`); project them from the
                // campaign with the same grid the globe uses.
                let grid = PlanetGrid::create(def.sector_tiles as usize);
                let mut glow = Vec::new();
                if let Some(planet) = self
                    .campaign
                    .as_ref()
                    .and_then(|campaign| campaign.planet(def.id))
                {
                    for sector in &planet.sectors {
                        // Only sectors that can contribute a glow (the per-tile
                        // loop runs over the whole mesh).
                        let captured = sector.is_captured(None);
                        if !((sector.has_enemy_base() && !captured) || sector.has_base()) {
                            continue;
                        }
                        let Some(tile) = grid.tiles.get(sector.id as usize) else {
                            continue;
                        };
                        glow.push(mind_core::maps::planet::SectorGlow {
                            v: tile.v,
                            has_enemy_base: sector.has_enemy_base(),
                            captured,
                            has_base: sector.has_base(),
                            has_preset: sector.preset.is_some(),
                            difficulty: sector.preset_difficulty,
                            light_coverage: sector.info.info.light_coverage,
                        });
                    }
                }
                generator.set_sector_glow(glow);
                Box::new(generator)
            }
            "erekir" => {
                let mut generator = mind_core::maps::planet::ErekirPlanetGenerator::new();
                generator.load_content(registry, &crate::render::minimap::load_block_colors());
                Box::new(generator)
            }
            _ => return out,
        };
        let emissive = mesher.is_emissive();
        let mesh =
            mind_core::render::g3d::mesh_data::build_hex(mesher.as_mut(), 6, def.radius, 0.2);
        let vertices = vec3_array(&mesh.vertices);
        let normals = vec3_array(&mesh.normals);
        let mut colors = PackedColorArray::new();
        for color in &mesh.colors {
            colors.push(rgba8(*color));
        }
        let mut emissive_colors = PackedColorArray::new();
        for color in &mesh.emissive {
            emissive_colors.push(rgba8(*color));
        }
        let mut indices = PackedInt32Array::new();
        for index in &mesh.indices {
            indices.push(*index as i32);
        }
        out.set("indexed", mesh.indexed);
        out.set("emissive", emissive);
        out.set(
            "emissive_count",
            mesh.emissive
                .iter()
                .filter(|&&color| (color & 0xff) > 0)
                .count() as i64,
        );
        out.set("vertices", &vertices);
        out.set("normals", &normals);
        out.set("colors", &colors);
        out.set("emissive_colors", &emissive_colors);
        out.set("indices", &indices);
        self.planet_mesh_cache = Some((name, out.clone()));
        out
    }
}

impl MindCampaign {
    /// `universe.secondsf()` (`Universe.java:289-291`): the runtime copy while
    /// a sector is installed, else the facade; `0` with no campaign.
    fn universe_secondsf(&self) -> f32 {
        let facade = self
            .campaign
            .as_ref()
            .map_or(0.0, |campaign| campaign.universe.secondsf(false));
        self.host()
            .and_then(|host| host.bind().campaign_universe_secondsf())
            .unwrap_or(facade)
    }
}

/// A `PackedVector3Array` from `[x, y, z]` points (the `push(Vector3::new(..))`
/// loops every planet-view mesh used to repeat).
fn vec3_array(points: &[[f32; 3]]) -> PackedVector3Array {
    let mut out = PackedVector3Array::new();
    for point in points {
        out.push(Vector3::new(point[0], point[1], point[2]));
    }
    out
}

/// `Color.from_rgba8` from a packed `0xRRGGBBAA` mesh color.
fn rgba8(packed: u32) -> Color {
    Color::from_rgba8(
        (packed >> 24) as u8,
        (packed >> 16) as u8,
        (packed >> 8) as u8,
        packed as u8,
    )
}

/// `PlanetRenderer.outlineRad`: the sector-grid shell scale over `radius`.
const OUTLINE_RAD: f32 = 1.17;

/// One body row of [`planet_body_rows`].
struct PlanetBodyRow {
    /// `Planet.name`.
    name: String,
    /// `true` for the solar system root (`SunMesh`, `Planets.java:25-40`).
    sun: bool,
    /// Body centre in the viewed planet's mesh frame.
    position: [f32; 3],
    /// `Planet.radius` (the mesh scale).
    radius: f32,
    /// `Planet.getRotation()` in degrees at this time.
    rotation: f32,
}

/// `PlanetRenderer.renderPlanet` (`PlanetRenderer.java:139-153`): every visible
/// body of the viewed planet's solar system except the viewed planet itself.
///
/// Java walks the `Planet.children` tree from `params.planet.solarSystem` and
/// draws each planet at its world position; the port renders the viewed globe
/// at the origin in its own rotated mesh frame (`planet_light`/
/// `orbit_center_in_planet_frame`), so a body's row is
/// `(body.position - viewed.position).rotate(Y, viewed.getRotation())` — the
/// `Planet.getTransform` rotation applied to the world-space offset. The
/// viewed planet is excluded because its hex mesh already *is* the globe.
fn planet_body_rows(planets: &[PlanetDef], viewed: usize, seconds: f32) -> Vec<PlanetBodyRow> {
    use mind_core::render::g3d::orbit::{
        orbit_radii, rotate_y, rotation, solar_system, world_position,
    };
    let Some(viewed_def) = planets.get(viewed) else {
        return Vec::new();
    };
    let orbit = orbit_radii(planets);
    let system = solar_system(planets, viewed);
    let viewed_pos = world_position(planets, &orbit, viewed, seconds);
    let viewed_rot = rotation(
        viewed_def,
        orbit.get(viewed).copied().unwrap_or(0.0),
        seconds,
    );
    let mut out = Vec::new();
    for (index, body) in planets.iter().enumerate() {
        // `if(!planet.visible()) return;` (and its children with it, since the
        // Java recursion returns before descending).
        if !body.visible || index == viewed || solar_system(planets, index) != system {
            continue;
        }
        let mut ancestors_visible = true;
        let mut ancestor = body.parent;
        while let Some(parent) = ancestor {
            match planets.get(parent.index()) {
                Some(def) if def.visible => ancestor = def.parent,
                _ => {
                    ancestors_visible = false;
                    break;
                }
            }
        }
        if !ancestors_visible {
            continue;
        }
        let pos = world_position(planets, &orbit, index, seconds);
        let position = rotate_y(
            [
                pos[0] - viewed_pos[0],
                pos[1] - viewed_pos[1],
                pos[2] - viewed_pos[2],
            ],
            viewed_rot,
        );
        out.push(PlanetBodyRow {
            name: body.name.clone(),
            sun: index == system,
            position,
            radius: body.radius,
            rotation: rotation(body, orbit.get(index).copied().unwrap_or(0.0), seconds),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;
    use mind_core::content::MemoryUnlockStore;
    use mind_core::content::create_base_content;

    /// GAPS #50 follow-up: `planet_bodies` derives the sun/planet rows from the
    /// campaign orbit helpers in the viewed planet's mesh frame. The sun and
    /// its sibling planets are the same solar system, the viewed planet is
    /// excluded, and the world-space distances survive the frame rotation.
    #[test]
    fn planet_bodies_derive_the_solar_system_in_the_viewed_frame() {
        use mind_core::render::g3d::orbit::{orbit_radii, rotation, world_position};

        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        let planets = registry.planets();
        let serpulo = require(registry.planet_by_name("serpulo"), "serpulo");
        let erekir = require(registry.planet_by_name("erekir"), "erekir");
        let sun = require(registry.planet_by_name("sun"), "sun");
        let orbit = orbit_radii(planets);
        let (serpulo_at, erekir_at, sun_at) =
            (serpulo.id.index(), erekir.id.index(), sun.id.index());
        assert_eq!(
            mind_core::render::g3d::orbit::solar_system(planets, erekir_at),
            sun_at,
            "erekir orbits the sun"
        );

        let seconds = 1_234.5f32;
        let rows = planet_body_rows(planets, serpulo_at, seconds);
        // The viewed globe is drawn by `planet_mesh`, not as a system body.
        assert!(
            rows.iter().all(|row| row.name != "serpulo"),
            "the viewed planet must not be duplicated"
        );
        let sun_row = rows
            .iter()
            .find(|row| row.name == "sun")
            .unwrap_or_else(|| panic!("sun row"));
        assert!(sun_row.sun, "the system root is the sun");
        assert_eq!(sun_row.radius, sun.radius);
        let erekir_row = rows
            .iter()
            .find(|row| row.name == "erekir")
            .unwrap_or_else(|| panic!("erekir row"));
        assert!(!erekir_row.sun);
        // `Planet.getRotation()` is row-local (the billboard/globe spin).
        assert_eq!(
            erekir_row.rotation,
            rotation(erekir, orbit[erekir_at], seconds)
        );

        // The row offset is the world offset turned by the viewed planet's
        // rotation (`Planet.getTransform`): rotation preserves distance, so
        // the sun->erekir separation is exactly `orbitRadius` in both frames.
        let world = |at: usize| world_position(planets, &orbit, at, seconds);
        let dist = |a: [f32; 3], b: [f32; 3]| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        let row_sep = dist(sun_row.position, erekir_row.position);
        let world_sep = dist(world(sun_at), world(erekir_at));
        assert!(
            (row_sep - world_sep).abs() < 1e-3,
            "{row_sep} != {world_sep}"
        );
        assert!((world_sep - orbit[erekir_at]).abs() < 1e-3);
        // A body at the viewed origin keeps its zero offset (the sun itself,
        // seen from the sun: every child sits exactly on its orbit radius).
        let from_sun = planet_body_rows(planets, sun_at, seconds);
        let erekir_from_sun = from_sun
            .iter()
            .find(|row| row.name == "erekir")
            .unwrap_or_else(|| panic!("erekir row from the sun"));
        let radius = (erekir_from_sun.position[0] * erekir_from_sun.position[0]
            + erekir_from_sun.position[2] * erekir_from_sun.position[2])
            .sqrt();
        assert!((radius - orbit[erekir_at]).abs() < 1e-3);
        // The viewed planet is a row when another body is the viewpoint.
        assert!(from_sun.iter().any(|row| row.name == "serpulo"));
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Solar-system layout and planet lighting (`type/Planet.java` constructor,
//! `getOrbitAngle`, `getRotation`, `addParentOffset`/`getWorldPosition`, and
//! `graphics/g3d/HexMesh.preRender`).
//!
//! The port renders a planet in its own (rotating) mesh frame, so the light
//! direction is what `HexMesh.preRender` hands `Shaders.planet`: the sun
//! position minus the planet position, rotated into the mesh frame by
//! `planet.getRotation()`.

use crate::content::registries::planets::PlanetDef;
use crate::fx::angles::{random_seed, trnsx, trnsy};

/// `Planet.rotateTime` default (`Planet.java:77`): seconds per full turn.
pub const ROTATE_TIME: f32 = 24.0 * 60.0;

/// `Planet.orbitRadius` for every planet (indexed like `planets`), replaying
/// the `Planet(String, Planet, float)` constructor in content order
/// (`Planet.java:197-221`): a child's orbit is
/// `parent.totalRadius + parent.orbitSpacing + totalRadius` with the child's
/// `totalRadius` still equal to its `radius`, then the parent runs
/// `updateTotalRadius()` (`Planet.java:337-343`). Values set in an
/// initializer block (`totalRadius += 2.6f`, `orbitSpacing = 2f`) land after
/// the constructor, so they come from the finished definitions.
pub fn orbit_radii(planets: &[PlanetDef]) -> Vec<f32> {
    let count = planets.len();
    let mut orbit = vec![0.0f32; count];
    let mut total = vec![0.0f32; count];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (index, def) in planets.iter().enumerate() {
        total[index] = def.radius;
        if let Some(parent) = def.parent.map(|id| id.index())
            && parent < index
        {
            orbit[index] = total[parent] + planets[parent].orbit_spacing + total[index];
            children[parent].push(index);
            // `updateTotalRadius`: the newest child still has its constructor
            // `totalRadius` (= radius).
            let mut parent_total = planets[parent].radius;
            for &child in &children[parent] {
                parent_total = parent_total.max(orbit[child] + total[child]);
            }
            total[parent] = parent_total;
        }
        // Initializer-block adjustments (`totalRadius += …`) apply once the
        // constructor returns; a later child's `updateTotalRadius` overwrites it.
        total[index] = def.total_radius;
    }
    orbit
}

/// `Planet.orbitTime = pow(orbitRadius, 1.5) * 1000` (`Planet.java:211`).
pub fn orbit_time(orbit_radius: f32) -> f32 {
    orbit_radius.powf(1.5) * 1000.0
}

/// `Planet.getOrbitAngle()` (`Planet.java:349-352`); `orbitOffset` is
/// `Mathf.randomSeed(id + 1, 360)` (`Planet.java:202`).
pub fn orbit_angle(def: &PlanetDef, orbit_radius: f32, seconds: f32) -> f32 {
    let orbit_offset = random_seed(i64::from(def.id.get()) + 1, 360.0);
    (orbit_offset + seconds / (orbit_time(orbit_radius) / 360.0)) % 360.0
}

/// `Planet.getRotation()` (`Planet.java:355-363`): tidally locked planets face
/// their parent, others spin once per `rotateTime` seconds from a seeded offset.
pub fn rotation(def: &PlanetDef, orbit_radius: f32, seconds: f32) -> f32 {
    if def.tidal_lock {
        return -orbit_angle(def, orbit_radius, seconds) + 90.0;
    }
    let offset = random_seed(i64::from(def.id.get()) + 1, 360.0);
    (offset + seconds / (ROTATE_TIME / 360.0)) % 360.0
}

/// `Planet.getWorldPosition` (`Planet.java:376-383`): the sum of every
/// `addParentOffset` up the parent chain.
pub fn world_position(
    planets: &[PlanetDef],
    orbit: &[f32],
    index: usize,
    seconds: f32,
) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    let mut current = Some(index);
    // Bounded walk (a malformed parent cycle cannot hang the view).
    let mut guard = planets.len();
    while let Some(at) = current {
        let Some(def) = planets.get(at) else {
            break;
        };
        let radius = orbit.get(at).copied().unwrap_or(0.0);
        // `addParentOffset`: root planets and zero orbits add nothing.
        if def.parent.is_some() && radius.abs() > 1e-6 {
            let angle = orbit_angle(def, radius, seconds);
            out[0] += trnsx(angle, radius);
            out[2] += trnsy(angle, radius);
        }
        current = def.parent.map(|id| id.index());
        guard = guard.saturating_sub(1);
        if guard == 0 {
            break;
        }
    }
    out
}

/// `Planet.solarSystem`: the root of the parent chain (`Planet.java:220`).
pub fn solar_system(planets: &[PlanetDef], index: usize) -> usize {
    let mut at = index;
    for _ in 0..planets.len() {
        match planets.get(at).and_then(|def| def.parent) {
            Some(parent) if parent.index() < planets.len() => at = parent.index(),
            _ => break,
        }
    }
    at
}

/// `Vec3.rotate(Vec3.Y, degrees)` (Arc `Mat.setToRotation` + `Vec3.mul`):
/// `x' = x cos - z sin`, `z' = x sin + z cos`.
pub fn rotate_y(v: [f32; 3], degrees: f32) -> [f32; 3] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [v[0] * cos - v[2] * sin, v[1], v[0] * sin + v[2] * cos]
}

fn normalized(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len <= f32::EPSILON {
        // `Vec3.nor()` leaves a zero vector unchanged; the port falls back to
        // `PlanetShader.lightDir`'s initial `(1, 1, 1).nor()`.
        let n = 1.0 / 3.0f32.sqrt();
        return [n, n, n];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

/// `HexMesh.preRender` (`HexMesh.java:22-27`):
/// `lightDir = (solarSystem.position - position).rotate(Y, getRotation()).nor()`
/// in the planet's mesh frame, and `ambientColor = solarSystem.lightColor`.
pub fn planet_light(planets: &[PlanetDef], index: usize, seconds: f32) -> PlanetLight {
    let orbit = orbit_radii(planets);
    let sun = solar_system(planets, index);
    let sun_pos = world_position(planets, &orbit, sun, seconds);
    let pos = world_position(planets, &orbit, index, seconds);
    let rot = planets
        .get(index)
        .map(|def| rotation(def, orbit.get(index).copied().unwrap_or(0.0), seconds))
        .unwrap_or(0.0);
    let to_sun = [
        sun_pos[0] - pos[0],
        sun_pos[1] - pos[1],
        sun_pos[2] - pos[2],
    ];
    let light = planets
        .get(sun)
        .map(|def| def.light_color)
        .unwrap_or(crate::content::Rgba::WHITE);
    PlanetLight {
        light_dir: normalized(rotate_y(to_sun, rot)),
        ambient: [light.r, light.g, light.b],
        rotation: rot,
    }
}

/// An orbit loop's centre (`PlanetRenderer.renderOrbit`, `PlanetRenderer.java:
/// 171-179`: `parent.position` in world space) expressed in the viewed
/// planet's mesh frame, where the port keeps the planet at the origin and the
/// camera on the sector's *unrotated* tile vector. Java rotates both the mesh
/// (`Planet.getTransform`: `rotate(Y, getRotation())`, `Planet.java:407`) and the
/// camera target (`Planet.lookAt`: `tile.rotate(Y, -getRotation())`, `:692-694`)
/// while the orbit stays in world space, so the world offset must be turned by
/// `+getRotation()` like `planet_light`'s sun direction. The XZ circle itself is
/// invariant under a Y rotation; only its centre moves (L03 T2, PAR-0420).
pub fn orbit_center_in_planet_frame(
    planets: &[PlanetDef],
    orbit: &[f32],
    parent: usize,
    viewed: usize,
    seconds: f32,
) -> [f32; 3] {
    let parent_pos = world_position(planets, orbit, parent, seconds);
    let pos = world_position(planets, orbit, viewed, seconds);
    let rot = planets
        .get(viewed)
        .map(|def| rotation(def, orbit.get(viewed).copied().unwrap_or(0.0), seconds))
        .unwrap_or(0.0);
    rotate_y(
        [
            parent_pos[0] - pos[0],
            parent_pos[1] - pos[1],
            parent_pos[2] - pos[2],
        ],
        rot,
    )
}

/// Uniforms `Shaders.PlanetShader.apply` receives for one planet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetLight {
    /// `u_lightdir` in the planet mesh frame.
    pub light_dir: [f32; 3],
    /// `u_ambientColor` (`solarSystem.lightColor`).
    pub ambient: [f32; 3],
    /// `planet.getRotation()` in degrees.
    pub rotation: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content};

    fn registry() -> ContentRegistry {
        create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content boot")
    }

    fn index(registry: &ContentRegistry, name: &str) -> usize {
        registry.planet_by_name(name).expect(name).id.index()
    }

    /// The constructor replay of `Planets.load()` (sun 4, erekir 1 with
    /// `orbitSpacing = 2`/`totalRadius += 2.6`, gier/notva/verilus 0.12,
    /// tantros/serpulo 1, sun `orbitSpacing = 12`).
    #[test]
    fn vanilla_orbit_radii_match_java_constructor_order() {
        let registry = registry();
        let planets = registry.planets();
        let orbit = orbit_radii(planets);
        let expect = [
            ("sun", 0.0),
            ("erekir", 17.0),
            ("gier", 5.72),
            ("notva", 30.12),
            ("tantros", 43.24),
            ("serpulo", 57.24),
            ("verilus", 70.36),
        ];
        for (name, radius) in expect {
            let got = orbit[index(&registry, name)];
            assert!((got - radius).abs() < 1e-3, "{name}: {got} != {radius}");
        }
    }

    /// L03 T2: Serpulo's own orbit loop is centred on the sun. In the mesh
    /// frame the port renders in, that centre must lie along the mesh-frame
    /// light direction (`HexMesh.preRender` turns the same world offset by the
    /// same rotation) at the orbit radius. Before the fix the centre was the raw
    /// world offset, off by the planet's spin (the orbit drew in the wrong place).
    #[test]
    fn serpulo_orbit_center_is_the_sun_in_mesh_frame() {
        let registry = registry();
        let planets = registry.planets();
        let serpulo = index(&registry, "serpulo");
        let sun = index(&registry, "sun");
        let orbit = orbit_radii(planets);
        for seconds in [0.0f32, 777.0, 12_345.0] {
            let center = orbit_center_in_planet_frame(planets, &orbit, sun, serpulo, seconds);
            let light = planet_light(planets, serpulo, seconds);
            let len = (center[0] * center[0] + center[2] * center[2]).sqrt();
            assert!((len - orbit[serpulo]).abs() < 1e-2, "radius {len}");
            assert!(center[1].abs() < 1e-6);
            let dot = (center[0] * light.light_dir[0] + center[2] * light.light_dir[2]) / len;
            assert!(
                dot > 0.9999,
                "t={seconds}: centre {center:?} vs light {:?}",
                light.light_dir
            );
        }
    }

    #[test]
    fn rotate_y_matches_arc_matrix() {
        // Arc: x' = x cos - z sin, z' = x sin + z cos; 90° maps +X to +Z.
        let v = rotate_y([1.0, 0.0, 0.0], 90.0);
        assert!(v[0].abs() < 1e-6 && (v[2] - 1.0).abs() < 1e-6, "{v:?}");
    }

    /// `HexMesh.preRender`: the light points from the planet towards the sun,
    /// rotated into the mesh frame, unit length, ambient = sun light colour.
    #[test]
    fn serpulo_light_points_at_the_sun_in_mesh_frame() {
        let registry = registry();
        let planets = registry.planets();
        let serpulo = index(&registry, "serpulo");
        let seconds = 12_345.0;
        let light = planet_light(planets, serpulo, seconds);
        let len = light.light_dir.iter().map(|c| c * c).sum::<f32>().sqrt();
        assert!((len - 1.0).abs() < 1e-5);
        assert_eq!(light.ambient, [1.0, 1.0, 1.0]);
        // Undo the rotation and compare with -position (the sun sits at 0).
        let orbit = orbit_radii(planets);
        let pos = world_position(planets, &orbit, serpulo, seconds);
        let world = rotate_y(light.light_dir, -light.rotation);
        let plen = (pos[0] * pos[0] + pos[2] * pos[2]).sqrt();
        assert!((world[0] + pos[0] / plen).abs() < 1e-4, "{world:?} {pos:?}");
        assert!((world[2] + pos[2] / plen).abs() < 1e-4, "{world:?} {pos:?}");
        assert!(world[1].abs() < 1e-6);
    }

    /// Tidal lock (`Planet.java:357-359`): erekir's rotation follows its
    /// orbit, so its lit side stays fixed in the mesh frame.
    #[test]
    fn tidally_locked_light_is_time_invariant() {
        let registry = registry();
        let planets = registry.planets();
        let erekir = index(&registry, "erekir");
        let a = planet_light(planets, erekir, 0.0).light_dir;
        let b = planet_light(planets, erekir, 50_000.0).light_dir;
        for axis in 0..3 {
            assert!((a[axis] - b[axis]).abs() < 1e-3, "{a:?} vs {b:?}");
        }
    }
}

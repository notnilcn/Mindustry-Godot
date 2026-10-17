// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Planet cloud layers: `graphics/g3d/HexSkyMesh.java` (the `MultiMesh` of
//! `Planet.cloudMeshLoader`, `content/Planets.java:49-52,127-130`) and the
//! `Planet.drawClouds` gates (`Planet.java:555-559`, `PlanetRenderer.java:149`).

use super::mesh_data::{MeshData, build_hex};
use crate::content::Rgba;
use crate::maps::generators::planet::HexMesher;

/// One `HexSkyMesh(planet, seed, speed, radius, divisions, color, octaves,
/// persistence, scl, thresh)` layer (`HexSkyMesh.java:16`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HexSkyLayer {
    /// Noise seed (`Simplex.noise3d(7 + seed, ...)`).
    pub seed: i32,
    /// Spin speed: `relRot = Time.globalTime * speed / 40` degrees.
    pub speed: f32,
    /// Shell height over the body (`MeshBuilder.buildHex` intensity).
    pub radius: f32,
    /// `PlanetGrid` subdivisions.
    pub divisions: usize,
    /// Flat layer colour (alpha included).
    pub color: Rgba,
    /// Noise octaves.
    pub octaves: i32,
    /// Noise persistence.
    pub persistence: f32,
    /// Noise scale.
    pub scl: f32,
    /// Tiles with noise `>= thresh` are skipped (no cloud).
    pub thresh: f32,
}

impl HexSkyLayer {
    /// Builds the layer's hex shell: `MeshBuilder.buildHex(mesher, divisions,
    /// planet.radius, radius)` with `getHeight = 1` (HexSkyMesh.java:17-32).
    pub fn build(&self, planet_radius: f32) -> MeshData {
        let mut mesher = HexSkyMesher { layer: *self };
        build_hex(&mut mesher, self.divisions, planet_radius, self.radius)
    }

    /// `HexSkyMesh.skip` (HexSkyMesh.java:29-31): `Simplex.noise3d(7 + seed,
    /// octaves, persistence, scl, x, y * 3f, z) >= thresh` (float `y * 3f`,
    /// then widened like the Java call).
    pub fn skip(&self, position: [f32; 3]) -> bool {
        crate::math::noise::noise3d(
            7 + self.seed,
            self.octaves,
            f64::from(self.persistence),
            f64::from(self.scl),
            f64::from(position[0]),
            f64::from(position[1] * 3.0),
            f64::from(position[2]),
        ) >= self.thresh
    }

    /// `HexSkyMesh.relRot` (HexSkyMesh.java:40-42), degrees about +Y on top of
    /// `planet.getRotation()`; `global_time` is Arc `Time.globalTime` (ticks).
    pub fn rel_rot(&self, global_time: f32) -> f32 {
        global_time * self.speed / 40.0
    }
}

struct HexSkyMesher {
    layer: HexSkyLayer,
}

impl HexMesher for HexSkyMesher {
    fn get_height(&self, _position: [f32; 3]) -> f32 {
        1.0
    }

    fn get_color(&self, _position: [f32; 3]) -> [f32; 4] {
        let color = self.layer.color;
        [color.r, color.g, color.b, color.a]
    }

    fn skip(&self, position: [f32; 3]) -> bool {
        self.layer.skip(position)
    }
}

/// `HexSkyMesh.preRender` alpha (HexSkyMesh.java:62): `1 - uiAlpha` on the
/// viewed planet, 1 elsewhere.
pub fn cloud_alpha(viewed_planet: bool, ui_alpha: f32) -> f32 {
    if viewed_planet { 1.0 - ui_alpha } else { 1.0 }
}

/// `HexSkyMesh.render`'s early return (HexSkyMesh.java:47): the viewed
/// planet's clouds are skipped while `Mathf.zero(1 - uiAlpha, 0.01)`.
pub fn clouds_hidden(viewed_planet: bool, ui_alpha: f32) -> bool {
    viewed_planet && (1.0 - ui_alpha).abs() <= 0.01
}

/// `Color.set(c).mul(f)` (Arc `Color.mul(float)` scales all four channels).
fn mul(color: Rgba, factor: f32) -> Rgba {
    Rgba::new(
        (color.r * factor).clamp(0.0, 1.0),
        (color.g * factor).clamp(0.0, 1.0),
        (color.b * factor).clamp(0.0, 1.0),
        (color.a * factor).clamp(0.0, 1.0),
    )
}

/// Arc `Color.lerp(target, t)` over all four channels.
fn lerp(from: Rgba, to: Rgba, t: f32) -> Rgba {
    Rgba::new(
        from.r + (to.r - from.r) * t,
        from.g + (to.g - from.g) * t,
        from.b + (to.b - from.b) * t,
        from.a + (to.a - from.a) * t,
    )
}

/// `Pal.spore` (Pal.java:27).
fn spore() -> Rgba {
    Rgba::from_hex("7457ce").unwrap_or(Rgba::WHITE)
}

/// Serpulo's `cloudMeshLoader` (Planets.java:127-130).
pub fn serpulo_clouds() -> Vec<HexSkyLayer> {
    vec![
        HexSkyLayer {
            seed: 11,
            speed: 0.15,
            radius: 0.13,
            divisions: 5,
            color: mul(spore(), 0.9).with_alpha(0.75),
            octaves: 2,
            persistence: 0.45,
            scl: 0.9,
            thresh: 0.38,
        },
        HexSkyLayer {
            seed: 1,
            speed: 0.6,
            radius: 0.16,
            divisions: 5,
            color: lerp(Rgba::WHITE, spore(), 0.55).with_alpha(0.75),
            octaves: 2,
            persistence: 0.45,
            scl: 1.0,
            thresh: 0.41,
        },
    ]
}

/// Erekir's `cloudMeshLoader` (Planets.java:49-52).
pub fn erekir_clouds() -> Vec<HexSkyLayer> {
    let hex = |value: &str| Rgba::from_hex(value).unwrap_or(Rgba::WHITE);
    vec![
        HexSkyLayer {
            seed: 2,
            speed: 0.15,
            radius: 0.14,
            divisions: 5,
            color: hex("eba768").with_alpha(0.75),
            octaves: 2,
            persistence: 0.42,
            scl: 1.0,
            thresh: 0.43,
        },
        HexSkyLayer {
            seed: 3,
            speed: 0.6,
            radius: 0.15,
            divisions: 5,
            color: hex("eea293").with_alpha(0.75),
            octaves: 2,
            persistence: 0.42,
            scl: 1.2,
            thresh: 0.45,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::g3d::grid::PlanetGrid;

    /// Kept tiles + the `hash = hash * 31 + id` over them, probed from the
    /// pinned jar (`runs/20261018-s3-L06-T6.5/probe/CloudProbe.java`:
    /// `PlanetGrid.create(5)` + `HexSkyMesh.skip`).
    fn kept(layer: &HexSkyLayer) -> (usize, i64) {
        let grid = PlanetGrid::create(layer.divisions);
        let mut kept = 0;
        let mut hash: i64 = 0;
        for tile in &grid.tiles {
            if !layer.skip(tile.v) {
                kept += 1;
                hash = hash.wrapping_mul(31).wrapping_add(tile.id as i64);
            }
        }
        (kept, hash)
    }

    #[test]
    fn cloud_layers_skip_like_java() {
        let serpulo = serpulo_clouds();
        let erekir = erekir_clouds();
        assert_eq!(kept(&serpulo[0]), (567, 6421037042009277813));
        assert_eq!(kept(&serpulo[1]), (724, -1318847851189910102));
        assert_eq!(kept(&erekir[0]), (840, -5064370176780491322));
        assert_eq!(kept(&erekir[1]), (862, 7573480947416043693));
    }

    #[test]
    fn cloud_shell_height_colour_and_gates() {
        let layer = serpulo_clouds()[0];
        let mesh = layer.build(1.0);
        // 567 kept tiles, indexed (2432 * 6 < 65535): 5 or 6 corners each.
        assert!(mesh.indexed);
        assert!(mesh.vertices.len() >= 567 * 5 && mesh.vertices.len() <= 567 * 6);
        for vertex in mesh.vertices.iter().take(32) {
            let length =
                (vertex[0] * vertex[0] + vertex[1] * vertex[1] + vertex[2] * vertex[2]).sqrt();
            assert!(
                (length - 1.13).abs() < 1e-4,
                "shell at (1 + 0.13) * radius: {length}"
            );
        }
        // `Pal.spore.mul(0.9).a(0.75)`: 7457ce * 0.9, alpha 0.75 (0xbf).
        assert_eq!(mesh.colors[0] & 0xff, 0xbf);
        assert!((layer.rel_rot(400.0) - 1.5).abs() < 1e-6);
        assert_eq!(cloud_alpha(true, 0.25), 0.75);
        assert_eq!(cloud_alpha(false, 0.25), 1.0);
        assert!(clouds_hidden(true, 0.995));
        assert!(!clouds_hidden(true, 0.9));
        assert!(!clouds_hidden(false, 1.0));
    }
}

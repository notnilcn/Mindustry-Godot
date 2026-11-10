// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Tests for the planet view.
//! Upstream: none (port-only; tests for `ui::planet`)

use super::mesh::{SunMesher, arc_ribbon, sun_mesh_data};
use super::rows::faded_alpha;
use super::{ARC_POINTS, STAR_FACES};
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
        let length = (vertex[0] * vertex[0] + vertex[1] * vertex[1] + vertex[2] * vertex[2]).sqrt();
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

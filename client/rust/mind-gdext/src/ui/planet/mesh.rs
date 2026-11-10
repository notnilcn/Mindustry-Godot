// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Mesh and shader helpers for the planet view (`SunMesher`, the arc ribbon and the upload/uniform seams).
//! Upstream: core/src/mindustry/graphics/g3d/SunMesh.java + core/src/mindustry/ui/dialogs/PlanetDialog.java (arcs)

use godot::classes::base_material_3d::{CullMode, Flags, ShadingMode, Transparency};
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::{ArrayMesh, ResourceLoader, Shader, ShaderMaterial, StandardMaterial3D};
use godot::obj::NewGd;
use godot::prelude::*;

use super::*;

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
pub(super) fn upload_mesh(mesh_data: &mind_core::render::g3d::MeshData) -> Option<Gd<ArrayMesh>> {
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
pub(super) fn color_hex(hex: u32) -> [f32; 4] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    ]
}

/// Allocates a `Mesh.ARRAY_MAX`-sized mesh-array list.
pub(super) fn new_arrays() -> VarArray {
    let mut arrays = VarArray::new();
    arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
    arrays
}

/// Sets one shader uniform (uniform names are pass-through from the ported
/// GDScript).
pub(super) fn set_uniform(material: &mut Gd<ShaderMaterial>, name: &str, value: &Variant) {
    material.set_shader_parameter(&StringName::from(name), value);
}

pub(super) fn load_shader(path: &str) -> Option<Gd<Shader>> {
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
pub(super) fn arc_ribbon(
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
pub(super) fn overlay_material() -> Gd<StandardMaterial3D> {
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
pub(super) fn vec3(color: Color) -> Vector3 {
    Vector3::new(color.r, color.g, color.b)
}

/// `PlanetView._gamma_shadow_alpha`.
pub(super) fn gamma_shadow_alpha(alpha: f32) -> f32 {
    1.0 - (1.0 - alpha.clamp(0.0, 1.0)).powf(2.2)
}

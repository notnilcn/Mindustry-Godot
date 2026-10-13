// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Pal` color constants and small `Drawf` color helpers (`graphics/Pal.java`,
//! `graphics/Drawf.java`, plan 16 §3.1/M5). Exact `Color.valueOf` hex values,
//! stored as `[f32; 4]` RGBA. Used by plans 14/16/17.

/// `Color.valueOf(hex)` with alpha `1.0`.
const fn c(hex: u32) -> [f32; 4] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    ]
}

/// `Color.valueOf(hex).a(alpha)`.
const fn ca(hex: u32, alpha: f32) -> [f32; 4] {
    let mut color = c(hex);
    color[3] = alpha;
    color
}

/// RGBA `[f32;4]` → packed RGBA8888.
pub fn to_bits(color: [f32; 4]) -> u32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (q(color[0]) << 24) | (q(color[1]) << 16) | (q(color[2]) << 8) | q(color[3])
}

/// `Color.a(alpha)` (copy with a new alpha).
pub const fn with_alpha(color: [f32; 4], alpha: f32) -> [f32; 4] {
    [color[0], color[1], color[2], alpha]
}

/// `Color.lerp(other, f)`.
pub fn lerp(a: [f32; 4], b: [f32; 4], f: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
        a[3] + (b[3] - a[3]) * f,
    ]
}

/// `Color.mul(factor)`.
pub const fn mul(color: [f32; 4], factor: f32) -> [f32; 4] {
    [
        color[0] * factor,
        color[1] * factor,
        color[2] * factor,
        color[3],
    ]
}

/// `Pal` global color constants (`graphics/Pal.java`).
pub mod pal {
    use super::{c, ca};

    macro_rules! colors {
        ($($name:ident = $hex:expr),* $(,)?) => {
            $(pub const $name: [f32; 4] = c($hex);)*
        };
    }

    colors!(
        WATER = 0x596ab8,
        DARK_OUTLINE = 0x2d2f39,
        THORIUM_PINK = 0xf9a3c7,
        COAL_BLACK = 0x272727,
        ITEMS = 0x2ea756,
        COMMAND = 0xeab678,
        SAP = 0x665c9f,
        SAP_BULLET = 0xbf92f9,
        SAP_BULLET_BACK = 0x6d56bf,
        REGEN = 0xd1efff,
        REACTOR_PURPLE = 0xbf92f9,
        REACTOR_PURPLE2 = 0x8a73c6,
        SPORE = 0x7457ce,
        BULLET_YELLOW = 0xfff8e8,
        BULLET_YELLOW_BACK = 0xf9c27a,
        DARK_METAL = 0x6e7080,
        DARKER_METAL = 0x565666,
        DARKEST_METAL = 0x38393f,
        MISSILE_YELLOW = 0xffd2ae,
        MISSILE_YELLOW_BACK = 0xe58956,
        MELTDOWN_HIT = 0xffb98b,
        PLASTANIUM_BACK = 0xd8d97f,
        PLASTANIUM_FRONT = 0xfffac6,
        LIGHT_FLAME = 0xffdd55,
        DARK_FLAME = 0xdb401c,
        LIGHT_PYRA_FLAME = 0xffb855,
        DARK_PYRA_FLAME = 0xdb661c,
        TURRET_HEAT = 0xab3400,
        LIGHT_ORANGE = 0xf68021,
        LIGHTISH_ORANGE = 0xf8ad42,
        LIGHTER_ORANGE = 0xf6e096,
        LIGHTISH_GRAY = 0xa2a2a2,
        AMMO = 0xff8947,
        RUBBLE = 0x1c1817,
        BOOST_TO = 0xffad4d,
        BOOST_FROM = 0xff7f57,
        LANCER_LASER = 0xa9d8ff,
        STONE_GRAY = 0x8f8f8f,
        ENGINE = 0xffbb64,
        YELLOW_BOLT_FRONT = 0xffd27e,
        HEALTH = 0xff341c,
        HEAL = 0x98ffa9,
        ACCENT = 0xffd37f,
        STAT = 0xffd37f,
        NEGATIVE_STAT = 0xe55454,
        GRAY = 0x454545,
        METAL_GRAY_DARK = 0x6e7080,
        ACCENT_BACK = 0xd4816b,
        PLACE = 0x6335f8,
        REMOVE = 0xe55454,
        NOPLACE = 0xffa697,
        REMOVE_BACK = 0xa73e3e,
        BREAK_INVALID = 0xd44b3d,
        RANGE = 0xf4ba6e,
        POWER = 0xfbad67,
        POWER_BAR = 0xec7b4c,
        POWER_LIGHT = 0xfbd367,
        UNIT_FRONT = 0xffa665,
        UNIT_BACK = 0xd06b53,
        LIGHT_TRAIL = 0xffe2a9,
        SURGE = 0xf3e979,
        PLASTANIUM = 0xa1b46e,
        RED_SPARK = 0xfbb97f,
        ORANGE_SPARK = 0xd2b29c,
        RED_DUST = 0xffa480,
        REDDER_DUST = 0xff7b69,
        PLASTIC_SMOKE = 0xf1e479,
        ADMIN_CHAT = 0xff4000,
        NEOPLASM_OUTLINE = 0x2e191d,
        NEOPLASM1 = 0xf98f4a,
        NEOPLASM_MID = 0xe05438,
        NEOPLASM2 = 0x9e172c,
        NEOPLASM_ACID = 0x8ead44,
        NEOPLASM_ACID_GLOW = 0x68e43e,
        LOGIC_BLOCKS = 0xd4816b,
        LOGIC_CONTROL = 0x6bb2b2,
        LOGIC_OPERATIONS = 0x877bad,
        LOGIC_IO = 0xa08a8a,
        LOGIC_UNITS = 0xc7b59d,
        LOGIC_WORLD = 0x6b84d4,
        BERYL_SHOT = 0xb1dd7e,
        TUNGSTEN_SHOT = 0x768a9a,
        PLASTIC_BURN = 0xe9ead3,
        MUDDY = 0x432722,
        RED_LIGHT = 0xfeb380,
        SLAG_ORANGE = 0xffa166,
        TECH_BLUE = 0x8ca9e8,
        VENT = 0x6b4e4e,
        VENT2 = 0x3b2a2a,
        COPPER_AMMO_FRONT = 0xeac1a8,
        COPPER_AMMO_BACK = 0xd39169,
        GRAPHITE_AMMO_BACK = 0x7d89d8,
        GRAPHITE_AMMO_FRONT = 0xdae1ee,
        SILICON_AMMO_BACK = 0x707594,
        SILICON_AMMO_FRONT = 0x999ba0,
        GLASS_AMMO_BACK = 0xb9c9df,
        GLASS_AMMO_FRONT = 0xffffff,
        SCRAP_AMMO_FRONT = 0xf5e0cc,
        SCRAP_AMMO_BACK = 0xd8887e,
        BLAST_AMMO_BACK = 0xe9665b,
        BLAST_AMMO_FRONT = 0xeeab89,
        THORIUM_AMMO_BACK = 0xf595be,
        THORIUM_AMMO_FRONT = 0xffffff,
    );

    /// `Pal.suppress` = `Pal.sap.mul(1.6f)`.
    pub const SUPPRESS: [f32; 4] = super::mul(c(0x665c9f), 1.6);

    /// `Pal.shield` = `Color.valueOf("ffd37f").a(0.7f)`.
    pub const SHIELD: [f32; 4] = ca(0xffd37f, 0.7);

    /// `Pal.placeRotate` = `Pal.accent`.
    pub const PLACE_ROTATE: [f32; 4] = ACCENT;
    /// `Pal.placing` = `Pal.accent`.
    pub const PLACING: [f32; 4] = ACCENT;

    /// `Color.darkGray`.
    pub const DARK_GRAY: [f32; 4] = [0.25, 0.25, 0.25, 1.0];
    /// `Color.lightGray`.
    pub const LIGHT_GRAY: [f32; 4] = [0.75, 0.75, 0.75, 1.0];
    /// `Color.darkishGray` (`0.3`).
    pub const DARKISH_GRAY: [f32; 4] = [0.3, 0.3, 0.3, 1.0];
    /// `Pal.shadow` (`0, 0, 0, 0.22`).
    pub const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.22];
}

// ---- Arc `Mathf`/`Lines` and `Drawf` geometry (G03 T1, view-only) ----

/// `Mathf.sin(in, scl, mag) = sin(in / scl) * mag` (`Mathf.java:90-92`).
pub fn sin_scl(x: f32, scl: f32, mag: f32) -> f32 {
    (x / scl).sin() * mag
}

/// `Mathf.absin(in, scl, mag) = (sin(in, scl * 2, mag) + mag) / 2`
/// (`Mathf.java:78-80`). Note the `scl * 2` factor: the full period is
/// `4 * PI * scl` ticks (`Drawf.java:343` uses it for the placement pulse).
pub fn absin(x: f32, scl: f32, mag: f32) -> f32 {
    ((x / (scl * 2.0)).sin() * mag + mag) / 2.0
}

/// `Angles.trnsx/trnsy(angle, len)` pair: `(cos * len, sin * len)`.
pub fn trns(angle_deg: f32, len: f32) -> (f32, f32) {
    let r = angle_deg.to_radians();
    (r.cos() * len, r.sin() * len)
}

/// `Lines.dashCircle(x, y, radius)` (`Lines.java:354-370`): `sides = 10 +
/// (int)(radius * 0.6)`, rounded up to even, then a segment every two sides,
/// the first starting at angle 90°. Returns `[x1, y1, x2, y2]` per segment.
pub fn dash_circle_segments(x: f32, y: f32, radius: f32) -> Vec<[f32; 4]> {
    let mut sides = 10 + (radius * 0.6) as i32;
    if sides % 2 == 1 {
        sides += 1;
    }
    let step = 360.0 / sides as f32;
    let mut out = Vec::with_capacity((sides / 2) as usize);
    let mut i = 0;
    while i < sides {
        let (x1, y1) = trns(step * i as f32 + 90.0, radius);
        let (x2, y2) = trns(step * (i + 1) as f32 + 90.0, radius);
        out.push([x + x1, y + y1, x + x2, y + y2]);
        i += 2;
    }
    out
}

/// `Lines.poly(x, y, sides, radius, angle)` (`Lines.java:414-419`): the closed
/// vertex ring starting at `angle_deg` (first == last for a connectable line).
pub fn poly_ring(x: f32, y: f32, sides: u32, radius: f32, angle_deg: f32) -> Vec<(f32, f32)> {
    if sides == 0 {
        return Vec::new();
    }
    let step = 360.0 / sides as f32;
    let mut out = Vec::with_capacity(sides as usize + 1);
    for i in 0..sides {
        let (dx, dy) = trns(angle_deg + step * i as f32, radius);
        out.push((x + dx, y + dy));
    }
    out.push(out[0]);
    out
}

/// `Lines.spikes(x, y, radius, length, spikes, rot)` (`Lines.java:372-383`):
/// `[x1, y1, x2, y2]` per spike, from the inner to the outer end.
pub fn spikes(x: f32, y: f32, radius: f32, length: f32, count: u32, rot_deg: f32) -> Vec<[f32; 4]> {
    let step = 360.0 / count as f32;
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let angle = i as f32 * step + rot_deg;
        let (x1, y1) = trns(angle, radius);
        let (x2, y2) = trns(angle, radius + length);
        out.push([x + x1, y + y1, x + x2, y + y2]);
    }
    out
}

/// `Drawf.tri(x, y, width, length, rotation)` (`Drawf.java:544-555`):
/// `[(x + rx, y + ry), (x + fx, y + fy), (x - rx, y - ry)]` with `f = trns(
/// rotation, length)` and `r = trns(rotation - 90, width / 2)`.
pub fn tri(x: f32, y: f32, width: f32, length: f32, rotation_deg: f32) -> [(f32, f32); 3] {
    let (fx, fy) = trns(rotation_deg, length);
    let (rx, ry) = trns(rotation_deg - 90.0, width / 2.0);
    [(x + rx, y + ry), (x + fx, y + fy), (x - rx, y - ry)]
}

/// `Geometry.d8edge` order used by `Drawf.selected` (`Drawf.java:340`).
pub const D8EDGE: [(i32, i32); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];

/// `Drawf.selected(x, y, block, color)` corner centres (`Drawf.java:338-348`):
/// per `d8edge` corner the region centre and rotation `i * 90`. `offset` is the
/// block's `Block.offset` (`((size + 1) % 2) * 8 / 2`, `Block.java:774`).
pub fn selected_corners(x: i32, y: i32, size: i32, offset: f32) -> [(f32, f32, f32); 4] {
    let corner = -(size - 1).max(0) as f32 / 2.0 * 8.0;
    let mut out = [(0.0, 0.0, 0.0); 4];
    for (i, (dx, dy)) in D8EDGE.iter().enumerate() {
        out[i] = (
            x as f32 * 8.0 + offset + corner * *dx as f32,
            y as f32 * 8.0 + offset + corner * *dy as f32,
            i as f32 * 90.0,
        );
    }
    out
}

/// `Drawf.buildBeam(x, y, tx, ty, radius)` (`Drawf.java:131-157`): the fan
/// triangles from the emitter to the target square's corner pair. `animate`
/// is Java's `renderer.animateShields` (the port always draws the filled
/// branch, `settings animatedshields` row pending in T17).
pub fn build_beam_tris(x: f32, y: f32, tx: f32, ty: f32, radius: f32) -> Vec<[(f32, f32); 3]> {
    let axis = (ty - y).atan2(tx - x).to_degrees();
    let angle = |p: (f32, f32)| (p.1 - y).atan2(p.0 - x).to_degrees();
    let dist = |p: (f32, f32)| {
        let d = (angle(p) - axis).abs() % 360.0;
        if d > 180.0 { 360.0 - d } else { d }
    };
    let mut corners = [
        (tx - radius, ty - radius),
        (tx + radius, ty - radius),
        (tx - radius, ty + radius),
        (tx + radius, ty + radius),
    ];
    // `Structs.comparingFloat(vec -> -angleDist(..))`: descending distance,
    // stable (ties keep the declaration order, Arc `Arrays.sort`).
    corners.sort_by(|a, b| {
        dist(*b)
            .partial_cmp(&dist(*a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // `Geometry.findClosest`: first strict minimum.
    let mut close = corners[0];
    let mut best = (close.0 - x).powi(2) + (close.1 - y).powi(2);
    for corner in &corners[1..] {
        let d = (corner.0 - x).powi(2) + (corner.1 - y).powi(2);
        if d < best {
            best = d;
            close = *corner;
        }
    }
    let (x1, y1) = corners[0];
    let (x3, y3) = corners[1];
    if close != corners[0] && close != corners[1] {
        vec![
            [(x, y), (x1, y1), (close.0, close.1)],
            [(x, y), (x3, y3), (close.0, close.1)],
        ]
    } else {
        vec![[(x, y), (x1, y1), (x3, y3)]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pal_hex_values_are_exact() {
        assert!((pal::ACCENT[0] - 1.0).abs() < 1e-6);
        assert!((pal::ACCENT[1] - 0.827_451).abs() < 1e-4);
        assert!((pal::ACCENT[2] - 0.498_039).abs() < 1e-4);
        assert!((pal::ACCENT[3] - 1.0).abs() < 1e-6);
        let bits = to_bits(pal::ACCENT);
        assert_eq!(bits, 0xffd37fff);
        assert_eq!(to_bits([1.0, 1.0, 1.0, 1.0]), 0xffff_ffff);
    }

    #[test]
    fn shield_has_point_seven_alpha() {
        assert!((pal::SHIELD[3] - 0.7).abs() < 1e-6);
        // 0.7 * 255 = 178.5 rounds to 179.
        assert_eq!(to_bits(pal::SHIELD) & 0xff, 179);
    }

    #[test]
    fn lerp_and_mul_are_linear() {
        let black = [0.0, 0.0, 0.0, 1.0];
        let white = [1.0, 1.0, 1.0, 1.0];
        assert_eq!(lerp(black, white, 0.5), [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(mul(white, 0.0), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn dash_circle_segments_matches_arc() {
        // radius 80 -> sides = 10 + 48 = 58 (even) -> 29 segments.
        let segs = dash_circle_segments(10.0, 20.0, 80.0);
        assert_eq!(segs.len(), 29);
        let first = segs[0];
        assert!((first[0] - 10.0).abs() < 1e-4);
        assert!((first[1] - 100.0).abs() < 1e-4);
        // Ends at angle 90 + 360/58 = 96.2069°.
        let end = trns(90.0 + 360.0 / 58.0, 80.0);
        assert!((first[2] - (10.0 + end.0)).abs() < 1e-4);
        assert!((first[3] - (20.0 + end.1)).abs() < 1e-4);

        // radius 5 -> 10 + 3 = 13 -> odd -> 14 -> 7 segments.
        assert_eq!(dash_circle_segments(0.0, 0.0, 5.0).len(), 7);
    }

    #[test]
    fn selected_corners_size_two() {
        let corners = selected_corners(10, 10, 2, 4.0);
        // offset -max(1,0)/2*8 = -4, block.offset 4.
        assert!((corners[0].0 - 80.0).abs() < 1e-4);
        assert!((corners[0].1 - 80.0).abs() < 1e-4);
        assert!((corners[0].2 - 0.0).abs() < 1e-4);
        assert!((corners[2].0 - 88.0).abs() < 1e-4);
        assert!((corners[2].1 - 88.0).abs() < 1e-4);
        assert!((corners[2].2 - 180.0).abs() < 1e-4);
    }

    #[test]
    fn tri_points() {
        let t = tri(0.0, 0.0, 2.0, 10.0, 0.0);
        assert!((t[0].0 - 0.0).abs() < 1e-4 && (t[0].1 + 1.0).abs() < 1e-4);
        assert!((t[1].0 - 10.0).abs() < 1e-4 && (t[1].1 - 0.0).abs() < 1e-4);
        assert!((t[2].0 - 0.0).abs() < 1e-4 && (t[2].1 - 1.0).abs() < 1e-4);
    }

    #[test]
    fn absin_range() {
        // Java `absin(in, scl, mag)`: `(sin(in / (scl * 2)) * mag + mag) / 2`
        // (`Mathf.java:78-80`); rising through `mag / 2` at 0, peak `mag` at
        // `PI * scl`, trough 0 at `-PI * scl`.
        assert!((absin(0.0, 1.0, 2.0) - 1.0).abs() < 1e-4);
        assert!((absin(std::f32::consts::PI, 1.0, 2.0) - 2.0).abs() < 1e-4);
        assert!((absin(-std::f32::consts::PI, 1.0, 2.0) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn poly_ring_is_closed_and_starts_at_angle() {
        let ring = poly_ring(1.0, 2.0, 4, 10.0, 0.0);
        assert_eq!(ring.len(), 5);
        assert!((ring[0].0 - 11.0).abs() < 1e-4);
        assert!((ring[0].1 - 2.0).abs() < 1e-4);
        assert_eq!(ring[0], ring[4]);
    }

    #[test]
    fn build_beam_tris_axis_aligned() {
        // Emitter (0,0), target (20,0), radius 4: the two x=16 corners are the
        // farthest from the beam axis, and the closest corner is also one of
        // them, so `Drawf.buildBeam` draws one triangle.
        let tris = build_beam_tris(0.0, 0.0, 20.0, 0.0, 4.0);
        assert_eq!(tris.len(), 1);
        let tri = tris[0];
        assert!((tri[0].0 - 0.0).abs() < 1e-4 && (tri[0].1 - 0.0).abs() < 1e-4);
        assert!((tri[1].0 - 16.0).abs() < 1e-4);
        assert!((tri[2].0 - 16.0).abs() < 1e-4);
        assert!((tri[1].1 + 4.0).abs() < 1e-4);
        assert!((tri[2].1 - 4.0).abs() < 1e-4);
    }
}

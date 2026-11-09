// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Pure bullet draw math (G03 T4): the size curves of `BasicBulletType.draw`
//! and the per-colour layer sizes of `LaserBulletType.draw`.

/// `BasicBulletType.draw` (`BasicBulletType.java:48-67`): the shrink factor
/// scales width/height by the `shrinkX`/`shrinkY` fractions.
pub fn basic_size(
    width: f32,
    height: f32,
    shrink_x: f32,
    shrink_y: f32,
    shrink: f32,
) -> (f32, f32) {
    (
        width * ((1.0 - shrink_x) + shrink_x * shrink),
        height * ((1.0 - shrink_y) + shrink_y * shrink),
    )
}

/// `LaserBulletType.draw` (`LaserBulletType.java:76-104`), one entry per
/// colour: `(stroke, tri length, circle radius, side length)`, with
/// `lengthFalloff = 0.5` (`LaserBulletType.java:9`).
pub fn laser_layers(
    width: f32,
    falloff: f32,
    fout: f32,
    colors_len: usize,
) -> Vec<(f32, f32, f32, f32)> {
    let mut cwidth = width;
    let mut compound = 1.0;
    let mut out = Vec::with_capacity(colors_len);
    for _ in 0..colors_len {
        cwidth *= falloff;
        let stroke = cwidth * fout;
        out.push((stroke, cwidth * 2.0 + width / 2.0, cwidth, 29.0 * compound));
        compound *= falloff;
    }
    out
}

/// `ContinuousLaserBulletType.draw`'s fade factor
/// (`ContinuousLaserBulletType.java:49-50`):
/// `clamp(b.time > b.lifetime - fadeTime ? 1 - (b.time - (lifetime - fadeTime))
/// / fadeTime : 1, 0, 1)`. A non-positive `fade_time` is the identity (the
/// shared def keeps 0 for kinds that do not declare the field).
pub fn continuous_laser_fout(time: f32, lifetime: f32, fade_time: f32) -> f32 {
    if fade_time <= 0.0 || time <= lifetime - fade_time {
        return 1.0;
    }
    (1.0 - (time - (lifetime - fade_time)) / fade_time).clamp(0.0, 1.0)
}

/// One colour layer of `ContinuousLaserBulletType.draw`
/// (`ContinuousLaserBulletType.java:59-64`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContinuousLaserLayer {
    /// `stroke = (width + Mathf.absin(Time.time, oscScl, oscMag)) * fout *
    /// lerp(strokeFrom, strokeTo, colorFin)`.
    pub stroke: f32,
    /// `ellipseLenScl = Mathf.lerp(1 - i / colors.length, 1, pointyScaling)`.
    pub ellipse_len_scl: f32,
}

/// Per-colour `ContinuousLaserBulletType.draw` layers (`:55-66`).
pub fn continuous_laser_layers(
    width: f32,
    stroke_from: f32,
    stroke_to: f32,
    pointy_scaling: f32,
    osc_scl: f32,
    osc_mag: f32,
    time: f32,
    fout: f32,
    colors_len: usize,
) -> Vec<ContinuousLaserLayer> {
    let oscillation = crate::render::drawf::absin(time, osc_scl, osc_mag);
    let base = width + oscillation;
    let mut out = Vec::with_capacity(colors_len);
    for index in 0..colors_len {
        let color_fin = if colors_len > 1 {
            index as f32 / (colors_len - 1) as f32
        } else {
            0.0
        };
        let base_stroke = lerp(stroke_from, stroke_to, color_fin);
        let ellipse_len_scl = lerp(1.0 - index as f32 / colors_len as f32, 1.0, pointy_scaling);
        out.push(ContinuousLaserLayer {
            stroke: base * fout * base_stroke,
            ellipse_len_scl,
        });
    }
    out
}

/// `Mathf.sin(in, scl, mag) = sin(in / scl) * mag` (`Mathf.java:90-92`), used
/// by both continuous draws for the width/angle oscillation.
pub fn continuous_osc(time: f32, scl: f32, mag: f32) -> f32 {
    if scl == 0.0 {
        return 0.0;
    }
    (time / scl).sin() * mag
}

/// One colour layer of `ContinuousFlameBulletType.draw`
/// (`ContinuousFlameBulletType.java:51-59`): the `Drawf.flame` arguments
/// `(length, width, pan)` derived from the interleaved `lengthWidthPans` array
/// (`:25-31`).
pub fn continuous_flame_layer(
    index: usize,
    length: f32,
    width: f32,
    mult: f32,
    sin: f32,
    length_width_pans: &[f32],
) -> Option<(f32, f32, f32)> {
    let base = index.checked_mul(3)?;
    let region_length = *length_width_pans.get(base)?;
    let region_width = *length_width_pans.get(base + 1)?;
    let pan = *length_width_pans.get(base + 2)?;
    Some((
        length * region_length * (1.0 - sin),
        width * region_width * mult * (1.0 + sin),
        pan,
    ))
}

/// `ContinuousFlameBulletType.draw`'s per-colour tint
/// (`ContinuousFlameBulletType.java:54-55`):
/// `Color.mul(0.9f).mul(1f + Mathf.absin(Time.time, 1f, 0.1f))`.
pub fn continuous_flame_color(color: [f32; 4], time: f32) -> [f32; 4] {
    let factor = 0.9 * (1.0 + crate::render::drawf::absin(time, 1.0, 0.1));
    crate::render::drawf::mul(color, factor)
}

/// `ContinuousFlameBulletType.draw`'s flare triangle angles
/// (`ContinuousFlameBulletType.java:63-77`):
/// `angle = Time.time * flareRotSpeed + (rotateFlare ? b.rotation() : 0f)` and
/// one triangle every 90° starting at 45°.
pub fn continuous_flame_flare_angles(
    rotation: f32,
    time: f32,
    rot_speed: f32,
    rotate_flare: bool,
) -> [f32; 4] {
    let angle = time * rot_speed + if rotate_flare { rotation } else { 0.0 };
    [45.0 + angle, 135.0 + angle, 225.0 + angle, 315.0 + angle]
}

/// `ContinuousFlameBulletType.currentLength`'s `b.fin(lengthInterp)` with the
/// class default `Interp.slope` (`:20`, `:96-98`): `1 - |fin - 0.5| * 2`.
pub fn continuous_flame_length_mult(fin: f32) -> f32 {
    1.0 - (fin.clamp(0.0, 1.0) - 0.5).abs() * 2.0
}

/// `Mathf.lerp(a, b, t)`.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_size_halves_height_at_end() {
        // shrink 0: full size; shrink 1: shrinkY 0.5 halves the height.
        assert_eq!(basic_size(5.0, 7.0, 0.0, 0.5, 0.0), (5.0, 3.5));
        assert_eq!(basic_size(5.0, 7.0, 0.0, 0.5, 1.0), (5.0, 7.0));
    }

    #[test]
    fn laser_layers_lancer_start() {
        let layers = laser_layers(15.0, 0.5, 1.0, 3);
        let strokes: Vec<f32> = layers.iter().map(|l| l.0).collect();
        assert_eq!(strokes, vec![7.5, 3.75, 1.875]);
        let tris: Vec<f32> = layers.iter().map(|l| l.1).collect();
        assert_eq!(tris, vec![22.5, 15.0, 11.25]);
        let circles: Vec<f32> = layers.iter().map(|l| l.2).collect();
        assert_eq!(circles, vec![7.5, 3.75, 1.875]);
        let sides: Vec<f32> = layers.iter().map(|l| l.3).collect();
        assert_eq!(sides, vec![29.0, 14.5, 7.25]);
    }

    /// PAR-0615 consumer: `ContinuousLaserBulletType.draw`'s 16-tick fade
    /// (`ContinuousLaserBulletType.java:49-50`).
    #[test]
    fn continuous_laser_fout_fades_over_the_last_fade_time() {
        assert_eq!(continuous_laser_fout(0.0, 16.0, 16.0), 1.0);
        assert!((continuous_laser_fout(8.0, 16.0, 16.0) - 0.5).abs() < 1e-6);
        assert_eq!(continuous_laser_fout(16.0, 16.0, 16.0), 0.0);
        // Kinds without a fade keep the beam at full length.
        assert_eq!(continuous_laser_fout(8.0, 16.0, 0.0), 1.0);
        // Clamped past the lifetime.
        assert_eq!(continuous_laser_fout(20.0, 16.0, 16.0), 0.0);
    }

    /// `ContinuousLaserBulletType.draw` per-colour stroke/ellipse math
    /// (`:55-66`) with the vanilla initializer values
    /// (`ContinuousLaserBulletType.java:16-21`).
    #[test]
    fn continuous_laser_layers_match_the_java_formula() {
        // `Mathf.absin(0, 0.8, 1.5)` = ((sin(0) * 1.5) + 1.5) / 2 = 0.75.
        let layers = continuous_laser_layers(9.0, 2.0, 0.5, 0.75, 0.8, 1.5, 0.0, 1.0, 4);
        assert_eq!(layers.len(), 4);
        let base = 9.75;
        // i = 0: colorFin 0 -> baseStroke 2; ellipse = lerp(1, 1, .75) = 1.
        assert!((layers[0].stroke - base * 2.0).abs() < 1e-5);
        assert!((layers[0].ellipse_len_scl - 1.0).abs() < 1e-6);
        // i = 1: colorFin 1/3 -> strokeScl 1.5; ellipse lerp(0.75, 1, .75).
        assert!((layers[1].stroke - base * 1.5).abs() < 1e-5);
        assert!((layers[1].ellipse_len_scl - 0.9375).abs() < 1e-6);
        // i = 3: colorFin 1 -> baseStroke 0.5; ellipse lerp(0.25, 1, .75).
        assert!((layers[3].stroke - base * 0.5).abs() < 1e-5);
        assert!((layers[3].ellipse_len_scl - 0.8125).abs() < 1e-6);
        // The fade scales every stroke (`fout = 0.5`).
        let faded = continuous_laser_layers(9.0, 2.0, 0.5, 0.75, 0.8, 1.5, 0.0, 0.5, 4);
        assert!((faded[0].stroke - layers[0].stroke * 0.5).abs() < 1e-5);
    }

    /// `ContinuousFlameBulletType.draw` (`:51-59`) reads the interleaved
    /// `lengthWidthPans` array (`:25-31`).
    #[test]
    fn continuous_flame_layers_read_the_pan_array() {
        const PANS: [f32; 6] = [1.12, 1.3, 0.32, 1.0, 1.0, 0.3];
        let (length, width, pan) =
            continuous_flame_layer(0, 120.0, 3.7, 1.0, 0.0, &PANS).expect("layer");
        assert!((length - 134.4).abs() < 1e-4);
        assert!((width - 4.81).abs() < 1e-4);
        assert!((pan - 0.32).abs() < 1e-6);
        // `sin` pinches the length and widens the quad (`(1 - sin)` / `(1 + sin)`).
        let (length, width, _) =
            continuous_flame_layer(1, 120.0, 3.7, 1.0, 0.5, &PANS).expect("layer");
        assert!((length - 60.0).abs() < 1e-4);
        assert!((width - 5.55).abs() < 1e-4);
        // Missing pan entries = no layer.
        assert!(continuous_flame_layer(2, 120.0, 3.7, 1.0, 0.0, &PANS).is_none());
    }

    /// `ContinuousFlameBulletType.draw` tint pulse (`:54-55`) and `Interp.slope`
    /// length multiplier (`:96-98`).
    #[test]
    fn continuous_flame_color_and_length_pulse() {
        // absin(0, 1, 0.1) = 0.05 -> 0.9 * 1.05 = 0.945.
        let color = continuous_flame_color([1.0, 0.5, 0.25, 0.8], 0.0);
        assert!((color[0] - 0.945).abs() < 1e-5);
        assert!((color[1] - 0.4725).abs() < 1e-5);
        assert!((color[2] - 0.23625).abs() < 1e-5);
        assert_eq!(color[3], 0.8);
        // `Mathf.sin(Time.time, 1.2, 0.02)` around zero.
        assert!((continuous_osc(0.0, 1.2, 0.02)).abs() < 1e-6);
        assert!((continuous_osc(std::f32::consts::FRAC_PI_2 * 1.2, 1.2, 0.02) - 0.02).abs() < 1e-6);
        // Interp.slope: zero at both ends, one at the midpoint.
        assert_eq!(continuous_flame_length_mult(0.0), 0.0);
        assert_eq!(continuous_flame_length_mult(0.5), 1.0);
        assert_eq!(continuous_flame_length_mult(1.0), 0.0);
    }

    /// `ContinuousFlameBulletType.draw`'s flare triangles (`:63-77`).
    #[test]
    fn continuous_flame_flare_angles_are_quarter_turns() {
        assert_eq!(
            continuous_flame_flare_angles(0.0, 0.0, 1.2, true),
            [45.0, 135.0, 225.0, 315.0]
        );
        // `rotateFlare` adds the bullet rotation; `Time.time * flareRotSpeed`
        // is the spin term.
        let angles = continuous_flame_flare_angles(90.0, 10.0, 1.2, true);
        assert_eq!(
            angles,
            [
                45.0 + 12.0 + 90.0,
                135.0 + 12.0 + 90.0,
                225.0 + 12.0 + 90.0,
                315.0 + 12.0 + 90.0
            ]
        );
        let fixed = continuous_flame_flare_angles(90.0, 10.0, 1.2, false);
        assert_eq!(fixed[0], 45.0 + 12.0);
    }
}

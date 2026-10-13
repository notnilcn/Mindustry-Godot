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
}

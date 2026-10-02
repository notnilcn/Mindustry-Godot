// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Trail` ribbon state machine (`graphics/Trail.java`, plan 16 §3.1/M5).
//!
//! Owns the point list, the `update`/`shorten` timing and the quad emitter
//! (`draw`/`drawCap`). Plan 11 drives `update` from unit/weapon components;
//! plan 17 owns FX lifetimes. View-only, allocation-free once warmed.

/// The angle conversion Arc uses (`Mathf.radDeg`).
const RAD_DEG: f32 = 180.0 / std::f32::consts::PI;

/// `Angles.angleRad(x1, y1, x2, y2)`.
pub fn angle_rad(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    (y2 - y1).atan2(x2 - x1)
}

/// A ribbon trail (`Trail`). `points` is a flat `[x, y, width]` triple list.
#[derive(Clone, Debug)]
pub struct Trail {
    /// Maximum number of points (`Trail.length`).
    pub length: usize,
    points: Vec<f32>,
    last_x: f32,
    last_y: f32,
    last_angle: f32,
    counter: f32,
    last_w: f32,
}

impl Trail {
    /// `new Trail(length)`.
    pub fn new(length: usize) -> Self {
        Self {
            length,
            points: Vec::with_capacity(length * 3),
            last_x: -1.0,
            last_y: -1.0,
            last_angle: -1.0,
            counter: 0.0,
            last_w: 0.0,
        }
    }

    /// `Trail.copy`.
    pub fn copy(&self) -> Self {
        let mut out = Trail::new(self.length);
        out.points = self.points.clone();
        out.last_x = self.last_x;
        out.last_y = self.last_y;
        out.last_angle = self.last_angle;
        out
    }

    /// `Trail.width`.
    pub fn width(&self) -> f32 {
        self.last_w
    }

    /// `Trail.clear`.
    pub fn clear(&mut self) {
        self.points.clear();
    }

    /// `Trail.size` (number of points).
    pub fn size(&self) -> usize {
        self.points.len() / 3
    }

    /// The raw point triples (`[x, y, width]`), for tests/draw.
    pub fn points(&self) -> &[f32] {
        &self.points
    }

    /// `Trail.drawCap`: the end-cap circle (`hcircle`), if the trail has points.
    /// Returns `(x, y, w, rotation_degrees)`.
    pub fn draw_cap(&self, width: f32) -> Option<(f32, f32, f32, f32)> {
        if self.points.is_empty() {
            return None;
        }
        let i = self.points.len() - 3;
        let x1 = self.points[i];
        let y1 = self.points[i + 1];
        let w1 = self.points[i + 2];
        if w1 <= 0.001 {
            return None;
        }
        let count = (self.points.len() / 3) as f32;
        let w = w1 * width / count * (i as f32 / 3.0) * 2.0;
        Some((x1, y1, w, -RAD_DEG * self.last_angle + 180.0))
    }

    /// `Trail.draw`: appends one quad `[x1,y1, x2,y2, x3,y3, x4,y4]` per segment.
    pub fn draw(&self, width: f32, out: &mut Vec<[f32; 8]>) {
        let size = self.points.len();
        if size == 0 {
            return;
        }
        let seg = width / (size as f32 / 3.0);
        let mut last_angle = self.last_angle;
        for i in (0..size).step_by(3) {
            let mut x1 = self.points[i];
            let mut y1 = self.points[i + 1];
            let mut w1 = self.points[i + 2];
            let (x2, y2, w2);
            if i < size - 3 {
                x2 = self.points[i + 3];
                y2 = self.points[i + 4];
                w2 = self.points[i + 5];
                if i == 0 && size >= (self.length.saturating_sub(1)) * 3 {
                    x1 = lerp(x1, x2, self.counter);
                    y1 = lerp(y1, y2, self.counter);
                    w1 = lerp(w1, w2, self.counter);
                }
            } else {
                x2 = self.last_x;
                y2 = self.last_y;
                w2 = self.last_w;
            }
            let z2 = -angle_rad(x1, y1, x2, y2);
            let z1 = if i == 0 { z2 } else { last_angle };
            if w1 <= 0.001 || w2 <= 0.001 {
                continue;
            }
            let fi = i as f32 / 3.0;
            let cx = z1.sin() * fi * seg * w1;
            let cy = z1.cos() * fi * seg * w1;
            let nx = z2.sin() * (fi + 1.0) * seg * w2;
            let ny = z2.cos() * (fi + 1.0) * seg * w2;
            out.push([
                x1 - cx,
                y1 - cy,
                x1 + cx,
                y1 + cy,
                x2 + nx,
                y2 + ny,
                x2 - nx,
                y2 - ny,
            ]);
            last_angle = z2;
        }
    }

    /// `Trail.shorten`: removes points at the `counter` interval.
    pub fn shorten(&mut self, delta: f32) {
        let count = (self.counter + delta) as i32;
        self.counter += delta;
        self.counter -= count as f32;
        if count > 0 && !self.points.is_empty() {
            let last = (count * 3 - 1).min(self.points.len() as i32 - 1).max(0);
            self.points.drain(0..=last as usize);
        }
    }

    /// `Trail.update(x, y)`.
    pub fn update(&mut self, x: f32, y: f32, delta: f32) {
        self.update_width(x, y, 1.0, delta);
    }

    /// `Trail.update(x, y, width)`.
    pub fn update_width(&mut self, x: f32, y: f32, width: f32, delta: f32) {
        let count = {
            let c = (self.counter + delta) as i32;
            self.counter += delta;
            self.counter -= c as f32;
            c
        };
        if count > 0 {
            let to_remove = self.points.len() as i32 + (count - 1 - self.length as i32) * 3;
            if to_remove > 0 && !self.points.is_empty() {
                let last = (to_remove - 1).min(self.points.len() as i32 - 1).max(0);
                self.points.drain(0..=last as usize);
            }
            if count == 1 || self.last_x == -1.0 {
                self.points.extend_from_slice(&[x, y, width]);
            } else {
                for i in 0..count {
                    let f = (i as f32 + 1.0) / count as f32;
                    self.points.push(lerp(self.last_x, x, f));
                    self.points.push(lerp(self.last_y, y, f));
                    self.points.push(lerp(self.last_w, width, f));
                }
            }
        }
        self.last_angle = -angle_rad(x, y, self.last_x, self.last_y);
        self.last_x = x;
        self.last_y = y;
        self.last_w = width;
    }
}

fn lerp(a: f32, b: f32, f: f32) -> f32 {
    a + (b - a) * f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_adds_points_at_interval() {
        let mut trail = Trail::new(10);
        // First update with delta 1 => count 1, lastX == -1 => single point.
        trail.update(5.0, 5.0, 1.0);
        assert_eq!(trail.size(), 1);
        // Next update with delta 1 adds one more interpolated point.
        trail.update(6.0, 5.0, 1.0);
        assert_eq!(trail.size(), 2);
        assert_eq!(trail.width(), 1.0);
    }

    #[test]
    fn points_are_capped_to_length() {
        let mut trail = Trail::new(3);
        for i in 0..20 {
            trail.update(i as f32, 0.0, 1.0);
        }
        assert!(trail.size() <= 4, "size {}", trail.size());
    }

    #[test]
    fn draw_emits_a_quad_per_segment() {
        let mut trail = Trail::new(10);
        trail.update(0.0, 0.0, 1.0);
        trail.update(8.0, 0.0, 1.0);
        let mut out = Vec::new();
        trail.draw(2.0, &mut out);
        assert!(!out.is_empty());
        // Cap returns a finite rotation.
        let cap = trail.draw_cap(2.0);
        assert!(cap.is_some());
    }

    #[test]
    fn shorten_removes_points() {
        let mut trail = Trail::new(20);
        for i in 0..5 {
            trail.update(i as f32, 0.0, 1.0);
        }
        let before = trail.size();
        trail.shorten(1.0);
        assert!(trail.size() < before);
    }
}

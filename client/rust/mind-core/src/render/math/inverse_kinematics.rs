// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Two-segment inverse-kinematics solver (`graphics/InverseKinematics.java`,
//! plan 16 §3.7/M7). Used by plans 11/17 to place a joint between the origin
//! `(0, 0)` and `end` such that `|result| == lengthA` and
//! `|result - end| == lengthB`. Deterministic, allocation-free, view-only.

/// Rotates `v` by `degrees` counter-clockwise (Arc `Vec2.rotate`).
pub fn rotate(v: [f32; 2], degrees: f32) -> [f32; 2] {
    let r = degrees.to_radians();
    let (sin, cos) = r.sin_cos();
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos]
}

/// Arc `Vec2.setLength`: normalizes then scales; a near-zero vector stays zero.
pub fn set_length(v: [f32; 2], len: f32) -> [f32; 2] {
    let m = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if m < 1e-9 {
        return [0.0, 0.0];
    }
    [v[0] / m * len, v[1] / m * len]
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

/// `InverseKinematics.solve(lengthA, lengthB, end, side)` convenience overload:
/// picks the attractor automatically and delegates to [`solve`].
pub fn solve_side(length_a: f32, length_b: f32, end: [f32; 2], side: bool) -> (bool, [f32; 2]) {
    let dir = if side { 1.0 } else { -1.0 };
    let at1 = {
        let r = rotate(end, dir);
        let l = set_length(r, length_a + length_b);
        [l[0] + end[0] / 2.0, l[1] + end[1] / 2.0]
    };
    solve(length_a, length_b, end, at1)
}

/// `InverseKinematics.solve(lengthA, lengthB, end, attractor)`: returns the
/// joint position and whether IK succeeded (`dist > 0 && dist < lengthA`).
pub fn solve(length_a: f32, length_b: f32, end: [f32; 2], attractor: [f32; 2]) -> (bool, [f32; 2]) {
    let axis = set_length(end, 1.0);
    let projected = [
        axis[0] * dot(attractor, axis),
        axis[1] * dot(attractor, axis),
    ];
    let perp = set_length(
        [attractor[0] - projected[0], attractor[1] - projected[1]],
        1.0,
    );

    let ex = dot(axis, end);
    let ey = dot(perp, end);
    let len = (ex * ex + ey * ey).sqrt();
    if len < 1e-9 {
        return (false, [0.0, 0.0]);
    }
    let raw = (len + (length_a * length_a - length_b * length_b) / len) / 2.0;
    let dist = raw.clamp(0.0, length_a);
    let h = (length_a * length_a - dist * dist).max(0.0).sqrt();
    let result = [axis[0] * dist + perp[0] * h, axis[1] * dist + perp[1] * h];
    (dist > 0.0 && dist < length_a, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_is_counter_clockwise() {
        let v = rotate([1.0, 0.0], 90.0);
        assert!((v[0] - 0.0).abs() < 1e-5);
        assert!((v[1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn solve_reaches_endpoint_with_exact_segment_lengths() {
        // Two equal segments, reachable straight-line endpoint.
        let end = [3.0, 0.0];
        let (ok, joint) = solve(2.0, 2.0, end, [0.0, 1.0]);
        assert!(ok);
        let a = (joint[0] * joint[0] + joint[1] * joint[1]).sqrt();
        let b = ((end[0] - joint[0]).powi(2) + (end[1] - joint[1]).powi(2)).sqrt();
        assert!((a - 2.0).abs() < 1e-3, "first segment {a}");
        assert!((b - 2.0).abs() < 1e-3, "second segment {b}");
    }

    #[test]
    fn unreachable_endpoint_fails_cleanly() {
        // Endpoint farther than lengthA + lengthB.
        let (ok, _) = solve(1.0, 1.0, [10.0, 0.0], [0.0, 1.0]);
        assert!(!ok);
    }

    #[test]
    fn side_overload_solves_each_mirror() {
        let end = [2.0, 1.0];
        let (ok, left) = solve_side(2.0, 2.0, end, true);
        let (ok2, right) = solve_side(2.0, 2.0, end, false);
        assert!(ok && ok2);
        // The two solutions are on opposite sides of the origin-end axis.
        assert!((left[1] - right[1]).abs() > 1e-3 || (left[0] - right[0]).abs() > 1e-3);
    }
}

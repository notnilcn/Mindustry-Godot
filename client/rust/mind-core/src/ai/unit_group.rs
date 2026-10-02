// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `UnitGroup` formation (plan 11 §3.8, deviation 5).
//!
//! Ported from `core/src/mindustry/ai/UnitGroup.java`. The upstream algorithm
//! squeezes colliders toward the group center, then runs a bounded physics
//! relaxation. The port is **synchronous and join-free** (deviation 5): squads
//! are ≤ 50 units and the whole computation runs inside the tick that triggers
//! it, so there is no `mainExecutor` submission or `volatile` state. The
//! quadtree broad-phase becomes an `O(n²)` pair scan (still ≤ 50 units); the
//! per-unit `raycastFastAvoid` clamp is a plan-11 M3 seam and is marked below.

use std::f32::consts::PI;

/// `Vars.unitCollisionRadiusScale`.
pub const UNIT_COLLISION_RADIUS_SCALE: f32 = 0.6;

/// One unit's contribution to a formation (`x`, `y`, `hitSize`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormationUnit {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Hitbox side (`Unit.hitSize`).
    pub hit_size: f32,
}

impl FormationUnit {
    /// Creates a unit contribution.
    pub const fn new(x: f32, y: f32, hit_size: f32) -> Self {
        Self { x, y, hit_size }
    }
}

/// Formation state for a squad (`UnitGroup`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitGroup {
    /// Group members with their original world positions.
    pub units: Vec<FormationUnit>,
    /// Physics collision layer (`PhysicsProcess.layerFlying` skips raycast).
    pub collision_layer: i32,
    /// Formation offsets **relative to the destination** (upstream `positions`).
    pub positions: Vec<(f32, f32)>,
    /// Offsets before raycast clamping (`originalPositions`).
    pub original_positions: Vec<(f32, f32)>,
    /// Whether the formation finished (`valid`).
    pub valid: bool,
}

impl UnitGroup {
    /// Creates an empty group.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the members.
    pub fn set_units(&mut self, units: Vec<FormationUnit>) {
        self.units = units;
        self.positions.clear();
        self.original_positions.clear();
        self.valid = false;
    }

    /// Whether `collision_layer` is the flying layer (`PhysicsProcess.layerFlying`).
    ///
    /// TODO(plan 05): read the real `PhysicsProcess.layerFlying` constant once
    /// plan 05's physics process lands; it is not on this branch.
    pub fn is_flying_layer(&self) -> bool {
        // Upstream `layerFlying = 1`; keep as a local constant until plan 05 owns it.
        self.collision_layer == 1
    }

    /// Computes the formation offsets for the current members.
    ///
    /// Reproduces `UnitGroup.calculateFormation` exactly except for the
    /// quadtree broad-phase (O(n²) here) and the per-unit pathfinder raycast
    /// (a plan-11 M3 `ControlPathfinder` seam; [`Self::original_positions`]
    /// stays equal to [`Self::positions`] until M3 wires it).
    pub fn calculate_formation(&mut self, collision_layer: i32) {
        self.collision_layer = collision_layer;
        let count = self.units.len();
        self.positions = vec![(0.0, 0.0); count];
        self.original_positions.clear();
        if count == 0 {
            self.valid = true;
            return;
        }

        let mut cx = 0.0f32;
        let mut cy = 0.0f32;
        for unit in &self.units {
            cx += unit.x;
            cy += unit.y;
        }
        cx /= count as f32;
        cy /= count as f32;

        for (index, unit) in self.units.iter().enumerate() {
            self.positions[index] = (unit.x - cx, unit.y - cy);
        }

        // Gather the radii once; the physics loop reads them by index.
        let radii: Vec<f32> = self.units.iter().map(|u| u.hit_size).collect();
        let positions = &mut self.positions;

        let max_space_usage = 0.7f32;
        let mut compress = true;
        let mut compression_iterations = 0;
        let mut physics_iterations = 0;
        let mut total_iterations = 0;
        let max_physics_iterations = ((1.0 + (count as f32).powf(0.65) / 10.0) as i32).min(6);

        while total_iterations < 40 && physics_iterations < max_physics_iterations {
            total_iterations += 1;
            let mut space_used = 0.0f32;

            if compress {
                compression_iterations += 1;
                let mut max_dst = 1.0f32;
                let mut total_area = 0.0f32;
                for a in 0..count {
                    // `v1.set(pos).lerp(zero, 0.3)` == scale by 0.7.
                    positions[a].0 *= 0.7;
                    positions[a].1 *= 0.7;
                    let rad = radii[a] * UNIT_COLLISION_RADIUS_SCALE;
                    let dst =
                        (positions[a].0 * positions[a].0 + positions[a].1 * positions[a].1).sqrt();
                    max_dst = max_dst.max(dst + rad);
                    total_area += PI * rad * rad;
                }
                let bounding_area = PI * max_dst * max_dst;
                space_used = total_area / bounding_area;
                compress = space_used <= max_space_usage && compression_iterations < 20;
            }

            if !compress || space_used > 0.5 {
                physics_iterations += 1;
                for a in 0..count {
                    let mut x = positions[a].0;
                    let mut y = positions[a].1;
                    let radius = radii[a] / 2.0;
                    for b in 0..count {
                        if a == b {
                            continue;
                        }
                        let ox = positions[b].0;
                        let oy = positions[b].1;
                        let rs = (radius + radii[b] / 2.0) * 1.2;
                        let dx = x - ox;
                        let dy = y - oy;
                        let dst = (dx * dx + dy * dy).sqrt();
                        if dst < rs {
                            let (vx, vy) = if dst > 0.0 {
                                let scale = (rs - dst) / dst;
                                (dx * scale, dy * scale)
                            } else {
                                (0.0, 0.0)
                            };
                            let mass1 = radii[a];
                            let mass2 = radii[b];
                            let ms = mass1 + mass2;
                            let m1 = mass2 / ms;
                            let m2 = mass1 / ms;

                            positions[a].0 += vx * m1;
                            positions[a].1 += vy * m1;
                            positions[b].0 -= vx * m2;
                            positions[b].1 -= vy * m2;
                            x = positions[a].0;
                            y = positions[a].1;
                        }
                    }
                }
            }
        }

        self.original_positions = self.positions.clone();
        // TODO(plan 11 M3): per-unit `ControlPathfinder::raycast_fast_avoid`
        // clamp from each offset toward `dest`. No `ControlPathfinder` on this
        // branch yet; offsets are used unclamped, matching a corridor-free map.
        self.valid = true;
    }

    /// World-space position of member `index` for a destination `(x, y)`.
    pub fn world_position(&self, index: usize, dest_x: f32, dest_y: f32) -> Option<(f32, f32)> {
        self.positions
            .get(index)
            .map(|(ox, oy)| (dest_x + ox, dest_y + oy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squad(count: usize, hit_size: f32) -> UnitGroup {
        let mut group = UnitGroup::new();
        // A tight cluster (1 px spacing) so the relaxation has a non-zero
        // separation vector to work with, like real unit positions.
        group.set_units(
            (0..count)
                .map(|i| FormationUnit::new(100.0 + i as f32, 100.0 + (i % 3) as f32, hit_size))
                .collect(),
        );
        group
    }

    #[test]
    fn clustered_units_spread_apart() {
        let mut group = squad(9, 8.0);
        group.calculate_formation(0);
        assert!(group.valid);
        assert_eq!(group.positions.len(), 9);
        // Center of mass of the offsets stays near zero.
        let sx: f32 = group.positions.iter().map(|p| p.0).sum();
        let sy: f32 = group.positions.iter().map(|p| p.1).sum();
        assert!(sx.abs() < 1.0 && sy.abs() < 1.0);
        // The offsets are not all identical (relaxation separated them).
        assert!(group.positions.windows(2).any(|w| w[0] != w[1]));
    }

    #[test]
    fn formation_is_deterministic() {
        let mut first = squad(12, 10.0);
        first.calculate_formation(0);
        let mut second = squad(12, 10.0);
        second.calculate_formation(0);
        assert_eq!(first.positions, second.positions);
        assert_eq!(first.original_positions, second.original_positions);
        assert_eq!(
            first.world_position(0, 500.0, 400.0),
            second.world_position(0, 500.0, 400.0)
        );
    }

    #[test]
    fn empty_group_is_valid() {
        let mut group = UnitGroup::new();
        group.calculate_formation(0);
        assert!(group.valid);
        assert!(group.positions.is_empty());
    }
}

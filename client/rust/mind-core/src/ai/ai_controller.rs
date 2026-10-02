// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `AIController` base helpers (plan 11 §3.5).
//!
//! Ported from `core/src/mindustry/entities/units/AIController.java`. Upstream
//! stores these as instance methods on the abstract `AIController`; the port
//! expresses them as free functions over [`AiCtx`] (plan 11 deviation 3: no
//! `Prov`/closure controllers). The pieces that need plan-12 `TeamData`, the
//! plan-13 `LogicAI` radar cache or the plan-08 payload runtime are marked with
//! an explicit `TODO(plan NN)` and kept inert until those land.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::entities::comp::unit::comp::{PhysicsComp, UnitCore, UnitTypeComp};
use crate::entities::comp::unit::queries;
use crate::entities::comp::unit::weapon_mount::WeaponsComp;
use crate::entities::comp::{Pos, TeamComp, Vel};
use crate::world::{TilePos, WorldGrid};

use super::pathfinder::{Cost, Pathfinder};
use super::unit_stance_runtime::StanceBits;

/// Per-tick AI scratch carrying the world, terrain, content and pathfinder.
///
/// Borrows are disjoint so a controller can mutate the unit while reading the
/// grid/content (mirrors upstream's `unit` + `Vars.world` split).
pub struct AiCtx<'a> {
    /// ECS world (unit components live here).
    pub world: &'a mut World,
    /// Terrain grid (`Vars.world.tiles`).
    pub grid: &'a WorldGrid,
    /// Content registry (`Vars.content`).
    pub content: &'a ContentRegistry,
    /// Per-team flowfield pathfinder.
    pub pathfinder: &'a mut Pathfinder,
    /// Team the pathfields were built for (`Pathfinder` is per-team upstream).
    pub team: u8,
}

impl AiCtx<'_> {
    /// Whether `unit` can fly (`Unit.type.flying` mirrored on [`PhysicsComp`]).
    pub fn is_flying(&self, unit: Entity) -> bool {
        self.world
            .get::<PhysicsComp>(unit)
            .map(|physics| physics.flying)
            .unwrap_or(false)
    }

    /// `prefSpeed`: `type.speed * (isBoosting ? boostMultiplier : 1)` cleared of
    /// statuses (status speed multiplier lands with plan 10's status runtime).
    pub fn pref_speed(&self, unit: Entity) -> f32 {
        let Some(physics) = self.world.get::<PhysicsComp>(unit) else {
            return 1.0;
        };
        let boosting = self
            .world
            .get::<UnitCore>(unit)
            .map(|core| core.boosting)
            .unwrap_or(false);
        if boosting && physics.can_boost {
            physics.speed * physics.boost_multiplier
        } else {
            physics.speed
        }
    }

    /// `faceMovement(dx, dy)`: point the body along a movement vector.
    ///
    /// Uses `atan2` like upstream `Unit.rotate`/`Angles.angle`.
    pub fn face_movement(&mut self, unit: Entity, dx: f32, dy: f32) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let angle = dy.atan2(dx).to_degrees();
        if let Some(mut core) = self.world.get_mut::<UnitCore>(unit) {
            core.rotation = normalize_angle(angle);
        }
    }

    /// `faceTarget(target)`: point the body at a world position.
    pub fn face_target(&mut self, unit: Entity, tx: f32, ty: f32) {
        let Some(pos) = self.world.get::<Pos>(unit).copied() else {
            return;
        };
        self.face_movement(unit, tx - pos.x, ty - pos.y);
    }

    /// `target(entity)`: mark an attack target, face it and gate the mounts.
    ///
    /// Returns the aim point so the caller can pass it to the plan-10 weapon
    /// pass. `mount.shoot` is cleared when the target is `None`.
    pub fn target(&mut self, unit: Entity, target: Option<Entity>) -> Option<(f32, f32)> {
        let aim = target.and_then(|target| self.world.get::<Pos>(target).map(|pos| (pos.x, pos.y)));
        if let Some((x, y)) = aim {
            self.face_target(unit, x, y);
        }
        if let Some(mut weapons) = self.world.get_mut::<WeaponsComp>(unit) {
            for mount in &mut weapons.mounts {
                mount.target = target;
                mount.shoot = target.is_some();
                if let Some((x, y)) = aim {
                    mount.aim_x = x;
                    mount.aim_y = y;
                }
            }
        }
        aim
    }

    /// `stopShooting`: clear every mount's shoot flag.
    pub fn stop_shooting(&mut self, unit: Entity) {
        if let Some(mut weapons) = self.world.get_mut::<WeaponsComp>(unit) {
            for mount in &mut weapons.mounts {
                mount.shoot = false;
            }
        }
    }

    /// `findTarget`: nearest hostile unit within `range`, respecting air/ground
    /// targeting rules and team. `target_air`/`target_ground` mirror
    /// `UnitType.targetAir`/`targetGround`; ties break by entity index.
    pub fn find_target(
        &mut self,
        unit: Entity,
        range: f32,
        target_air: bool,
        target_ground: bool,
    ) -> Option<Entity> {
        let pos = self.world.get::<Pos>(unit).copied()?;
        let team = self.world.get::<TeamComp>(unit).map(|t| t.team)?;
        let candidates = queries::in_radius(self.world, pos.x, pos.y, range, None);
        let mut best: Option<(f32, Entity)> = None;
        for candidate in candidates {
            if candidate == unit {
                continue;
            }
            let Some(other_team) = self.world.get::<TeamComp>(candidate).map(|t| t.team) else {
                continue;
            };
            if other_team == team {
                continue;
            }
            let flying = self
                .world
                .get::<PhysicsComp>(candidate)
                .map(|physics| physics.flying)
                .unwrap_or(false);
            if (flying && !target_air) || (!flying && !target_ground) {
                continue;
            }
            let Some(other_pos) = self.world.get::<Pos>(candidate) else {
                continue;
            };
            let dx = other_pos.x - pos.x;
            let dy = other_pos.y - pos.y;
            let dist2 = dx * dx + dy * dy;
            match best {
                Some((best_dist, best_entity))
                    if best_dist < dist2
                        || (best_dist == dist2 && best_entity.index() <= candidate.index()) => {}
                _ => best = Some((dist2, candidate)),
            }
        }
        best.map(|(_, entity)| entity)
    }

    /// `keepDistance(target, min)`: push away from a target that has closed in.
    ///
    /// Returns the applied velocity (already written to [`Vel`]).
    pub fn keep_distance(&mut self, unit: Entity, target: Entity, min: f32) -> (f32, f32) {
        let (Some(pos), Some(tpos)) = (
            self.world.get::<Pos>(unit).copied(),
            self.world.get::<Pos>(target).copied(),
        ) else {
            return (0.0, 0.0);
        };
        let dx = pos.x - tpos.x;
        let dy = pos.y - tpos.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist >= min || dist <= 0.0001 {
            return (0.0, 0.0);
        }
        let speed = self.pref_speed(unit);
        let (vx, vy) = (dx / dist * speed, dy / dist * speed);
        if let Some(mut vel) = self.world.get_mut::<Vel>(unit) {
            vel.x = vx;
            vel.y = vy;
        }
        (vx, vy)
    }

    /// `avoid`: steering vector away from nearby allied units (separation).
    ///
    /// Deterministic: candidates are visited in entity order, ties sum.
    pub fn avoid(&mut self, unit: Entity, sense: f32) -> (f32, f32) {
        let Some(pos) = self.world.get::<Pos>(unit).copied() else {
            return (0.0, 0.0);
        };
        let candidates = queries::in_radius(self.world, pos.x, pos.y, sense, None);
        let mut ax = 0.0f32;
        let mut ay = 0.0f32;
        for other in candidates {
            if other == unit {
                continue;
            }
            let Some(opos) = self.world.get::<Pos>(other) else {
                continue;
            };
            let dx = pos.x - opos.x;
            let dy = pos.y - opos.y;
            let dist2 = dx * dx + dy * dy;
            if dist2 <= 0.0001 || dist2 > sense * sense {
                continue;
            }
            ax += dx / dist2;
            ay += dy / dist2;
        }
        (ax, ay)
    }

    /// `retreat(core)`: steer toward a friendly core (flee behavior).
    pub fn retreat(&mut self, unit: Entity, core_x: f32, core_y: f32) -> bool {
        self.face_target(unit, core_x, core_y);
        self.move_direct(unit, core_x, core_y, 4.0)
    }

    /// `unreachable`: upstream returns `true` when a path rebuild has failed and
    /// the controller should stop issuing orders. Without the M3 async request
    /// state this is the conservative `false` (target stays reachable).
    pub fn unreachable(&self, _unit: Entity) -> bool {
        false
    }

    /// `hasStance`: bit test against the unit's active stances.
    pub fn has_stance(stances: &StanceBits, stance: crate::content::id::UnitStanceId) -> bool {
        stances.get(stance)
    }

    /// `stanceChanged`: hook invoked after a stance toggle. The command
    /// controller reads this in `CommandAI`; the base AI has nothing to do.
    pub fn stance_changed(&self, _unit: Entity, _stance: crate::content::id::UnitStanceId) {}

    /// Direct straight-line move (flying units and `SuicideAI`).
    pub fn move_direct(&mut self, unit: Entity, dest_x: f32, dest_y: f32, arrive: f32) -> bool {
        let speed = self.pref_speed(unit);
        let Some(mut pos) = self.world.get::<Pos>(unit).copied() else {
            return false;
        };
        let mut vel = self
            .world
            .get::<Vel>(unit)
            .copied()
            .unwrap_or(Vel { x: 0.0, y: 0.0 });
        let mut core = self
            .world
            .get::<UnitCore>(unit)
            .copied()
            .unwrap_or(UnitCore::new(0.0));
        let arrived = super::types::ground::approach(
            &mut pos, &mut vel, &mut core, speed, dest_x, dest_y, arrive,
        );
        if let Some(mut stored) = self.world.get_mut::<Pos>(unit) {
            *stored = pos;
        }
        if let Some(mut stored) = self.world.get_mut::<Vel>(unit) {
            *stored = vel;
        }
        if let Some(mut stored) = self.world.get_mut::<UnitCore>(unit) {
            *stored = core;
        }
        arrived
    }

    /// `pathfind(dest)`: move toward a destination tile, respecting terrain for
    /// ground units and going straight for flyers. Returns `true` on arrival.
    ///
    /// Port of the `AIController.pathfind`/`moveTo` base: the flowfield (or a
    /// direct line) supplies the next waypoint; the body then steers at
    /// `prefSpeed`. `stop_at_target` halts within `arrive`.
    pub fn pathfind(&mut self, unit: Entity, dest: TilePos, arrive: f32) -> bool {
        let (cx, cy) = ground::tile_center(dest.x() as i32, dest.y() as i32);
        if self.is_flying(unit) {
            return self.move_direct(unit, cx, cy, arrive);
        }
        let Some(pos) = self.world.get::<Pos>(unit).copied() else {
            return false;
        };
        let current = TilePos::new(
            WorldGrid::to_tile(pos.x) as i16,
            WorldGrid::to_tile(pos.y) as i16,
        );
        if current == dest || self.pathfinder.width <= 0 {
            return self.move_direct(unit, cx, cy, arrive);
        }
        let width = self.pathfinder.width;
        let height = self.pathfinder.height;
        let index = current.x() as usize + current.y() as usize * width as usize;
        let next = self
            .pathfinder
            .get_field(Cost::Ground, self.team, &[dest])
            .get_target_tile(index, width, height, true);
        match next {
            Some(next) => {
                let (nx, ny) = ground::tile_center((next as i32) % width, (next as i32) / width);
                self.move_direct(unit, nx, ny, arrive)
            }
            None => self.move_direct(unit, cx, cy, arrive),
        }
    }

    /// `circleAttack`: orbit a target at `radius` while facing it.
    ///
    /// Approximated as a tangential direct move; the exact upstream orbiting
    /// uses `unit.vel` accumulation and lands with the M4 `CommandAI` pass.
    pub fn circle_attack(&mut self, unit: Entity, target: Entity, radius: f32) -> bool {
        let (Some(pos), Some(tpos)) = (
            self.world.get::<Pos>(unit).copied(),
            self.world.get::<Pos>(target).copied(),
        ) else {
            return false;
        };
        let dx = pos.x - tpos.x;
        let dy = pos.y - tpos.y;
        let dist = (dx * dx + dy * dy).sqrt().max(0.0001);
        // Tangential direction (counter-clockwise), scaled to stay on the ring.
        let tangent_x = -dy / dist;
        let tangent_y = dx / dist;
        let radial = (dist - radius) / dist;
        let dest_x = pos.x + tangent_x * 8.0 + dx * radial;
        let dest_y = pos.y + tangent_y * 8.0 + dy * radial;
        self.move_direct(unit, dest_x, dest_y, 4.0)
    }

    /// `updateVisuals`: upstream writes walk/boost/leg draw state. Headless is a
    /// no-op; the view reads component state directly (plan 16/17).
    pub fn update_visuals(&self, _unit: Entity) {}

    /// Content type id of a unit (`Unit.type`).
    pub fn unit_type(&self, unit: Entity) -> Option<crate::content::UnitTypeId> {
        self.world
            .get::<UnitTypeComp>(unit)
            .map(|comp| comp.type_id)
    }
}

/// Normalizes degrees into `[0, 360)`.
pub fn normalize_angle(degrees: f32) -> f32 {
    let mut angle = degrees % 360.0;
    if angle < 0.0 {
        angle += 360.0;
    }
    angle
}

use super::types::ground;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;

    fn ctx<'a>(harness: &'a mut UnitHarness, team: u8) -> AiCtx<'a> {
        AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team,
        }
    }

    #[test]
    fn find_target_respects_team_and_air_flags() {
        let mut harness = UnitHarness::new(64, 64, 9);
        let a = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("a");
        let _enemy_ground = harness.spawn("dagger", 1, 100.0, 64.0, 0.0).expect("eg");
        let mut ctx = ctx(&mut harness, 0);
        // Ground targeting finds the hostile mech.
        let found = ctx.find_target(a, 200.0, false, true).expect("target");
        // Air-only targeting finds nothing (no flyers).
        assert!(ctx.find_target(a, 200.0, true, false).is_none());
        assert_ne!(found, a);
    }

    #[test]
    fn target_faces_and_gates_mounts() {
        let mut harness = UnitHarness::new(64, 64, 9);
        let a = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("a");
        let b = harness.spawn("dagger", 1, 128.0, 64.0, 0.0).expect("b");
        let aim = {
            let mut ctx = ctx(&mut harness, 0);
            ctx.target(a, Some(b)).expect("aim")
        };
        assert!(aim.0 > 64.0);
        let rotation = harness.build.world.get::<UnitCore>(a).unwrap().rotation;
        assert!((rotation - 0.0).abs() < 1.0);
        {
            let mut ctx = ctx(&mut harness, 0);
            ctx.target(a, None);
        }
        let weapons = harness.build.world.get::<WeaponsComp>(a).unwrap();
        assert!(weapons.mounts.iter().all(|m| !m.shoot));
    }

    #[test]
    fn keep_distance_pushes_away() {
        let mut harness = UnitHarness::new(64, 64, 9);
        let a = harness.spawn("dagger", 0, 100.0, 100.0, 0.0).expect("a");
        let b = harness.spawn("dagger", 1, 104.0, 100.0, 0.0).expect("b");
        let mut ctx = ctx(&mut harness, 0);
        let (vx, _vy) = ctx.keep_distance(a, b, 32.0);
        assert!(vx < 0.0, "moves left away from target");
    }

    #[test]
    fn pathfind_reaches_target_tile() {
        let mut harness = UnitHarness::new(64, 64, 9);
        let a = harness.spawn("dagger", 0, 44.0, 44.0, 0.0).expect("a");
        let mut arrive_tick = None;
        for tick in 0..1500 {
            {
                let mut ctx = ctx(&mut harness, 0);
                if ctx.pathfind(a, TilePos::new(60, 60), 8.0) {
                    arrive_tick = Some(tick);
                    break;
                }
            }
        }
        assert!(arrive_tick.is_some(), "pathfinder helper arrived");
    }
}

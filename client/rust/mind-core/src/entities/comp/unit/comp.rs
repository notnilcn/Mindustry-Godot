// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit-side components (plan 11 §3.6).
//!
//! Plan 05 owns `Pos`/`Vel`/`TeamComp` and plan 07 owns `Health`; units reuse
//! them. This module adds the unit core, kind markers and derived movement
//! state. Components are plain `bevy_ecs::Component`s — the plan-11 metadata
//! vocabulary lives in [`super::defs`] and does not mutate plan 05's frozen
//! `meta entities` golden.

use bevy_ecs::component::Component;

use crate::content::UnitTypeId;

/// Core unit state (`UnitComp` fields not owned by a base component).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct UnitCore {
    /// Facing angle in degrees (`rotation`).
    pub rotation: f32,
    /// Whether the unit is dead (`dead`).
    pub dead: bool,
    /// Flight elevation in `0..=1` (`elevation`).
    pub elevation: f32,
    /// Boost active (`isBoosting`).
    pub boosting: bool,
    /// Rail/boost flag mirror (`wasPlayer`-adjacent; transient).
    pub spawned_by_core: bool,
}

impl UnitCore {
    /// Defaults for a freshly spawned unit facing `rotation`.
    pub const fn new(rotation: f32) -> Self {
        Self {
            rotation,
            dead: false,
            elevation: 0.0,
            boosting: false,
            spawned_by_core: false,
        }
    }
}

/// Content unit-type handle (`Unit.type`, a `UnitTypeId`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct UnitTypeComp {
    /// Registered unit content id.
    pub type_id: UnitTypeId,
}

/// Movement physics derived from `UnitType`.
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct PhysicsComp {
    /// Ground speed (world units/tick).
    pub speed: f32,
    /// Boost speed multiplier.
    pub boost_multiplier: f32,
    /// Movement drag fraction.
    pub drag: f32,
    /// Acceleration fraction.
    pub accel: f32,
    /// Whether the unit flies.
    pub flying: bool,
    /// Whether the unit can boost.
    pub can_boost: bool,
}

impl Default for PhysicsComp {
    fn default() -> Self {
        Self {
            speed: 1.0,
            boost_multiplier: 1.0,
            drag: 0.5,
            accel: 0.1,
            flying: false,
            can_boost: false,
        }
    }
}

/// Square hitbox side (`HitboxComp.hitSize`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct HitboxComp {
    /// Hitbox side length in world units.
    pub hit_size: f32,
}

/// Mech-specific state (`MechComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct MechComp {
    /// Base rotation decoupled from body rotation (`baseRotation`).
    pub base_rotation: f32,
    /// Walk animation timer (`walkTime`).
    pub walk_time: f32,
    /// Whether the mech stepped this tick (`walked`).
    pub walked: bool,
}

impl MechComp {
    /// Defaults for a freshly spawned mech.
    pub const fn new(rotation: f32) -> Self {
        Self {
            base_rotation: rotation,
            walk_time: 0.0,
            walked: false,
        }
    }
}

/// Legs state (`LegsComp`; the `Leg[]` IK array lands with M1/M3 work).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct LegsComp {
    /// Number of legs (`legs.length`).
    pub leg_count: u8,
}

/// Hover/elevation movement marker (`ElevationMoveComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct ElevationMoveComp {
    /// Whether the unit is in the air (`flying`).
    pub flying: bool,
}

/// Tank treads (`TankComp`; tread rects/timers land with M1).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct TankComp {
    /// Tread dust timer.
    pub tread_time: f32,
}

/// Naval water movement (`WaterMoveComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct WaterMoveComp {
    /// Wave trail timer.
    pub trail_time: f32,
}

/// Crawl movement (`CrawlComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct CrawlComp {
    /// Segment rotation in degrees (`segmentRot`).
    pub segment_rot: f32,
}

/// Payload carrier state (`PayloadComp`); payload entities are plan 08's.
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct PayloadComp {
    /// Payload capacity in world units² (`payloadCapacity`).
    pub capacity: f32,
}

/// Proxy unit over a building tile (`BlockUnitComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct BlockUnitComp {
    /// Packed tile position of the proxied building (`TilePos::pack`).
    pub tile: i32,
}

/// Building tether (`BuildingTetherComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct BuildingTetherComp {
    /// Tethered building entity, if resolved.
    pub building: Option<bevy_ecs::entity::Entity>,
}

/// Timed-kill missile lifetime (`TimedKillComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct TimedKillComp {
    /// Remaining lifetime in ticks (`time`).
    pub time: f32,
    /// Initial lifetime (`lifetime`).
    pub lifetime: f32,
}

/// Generic countdown (`TimedComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct TimedComp {
    /// Remaining time (`time`).
    pub time: f32,
}

/// Target dummy proxy (`TargetDummyComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct TargetDummyComp;

/// Marks the unit as carrying the segment head/child chain role.
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct SegmentComp {
    /// Parent entity for non-head segments.
    pub parent: Option<bevy_ecs::entity::Entity>,
    /// Segment index (`0` = head).
    pub index: u8,
}

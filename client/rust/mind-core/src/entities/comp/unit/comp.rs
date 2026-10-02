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
use bevy_ecs::entity::Entity;

use crate::content::{ItemId, StatusId, UnitTypeId};
use crate::world::TilePos;

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

/// Audio call-site data copied from `UnitTypeDef` at spawn (plan 18 §2.3).
///
/// `UnitComp.kill` reads this to emit `deathSound`/`wreckSound` through the
/// installed [`crate::audio::AudioSinkRes`] without needing the content registry
/// in the `&mut World` kill path.
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct UnitAudioComp {
    /// `UnitType.deathSound` (`none`/`unset` = silent).
    pub death_sound: crate::content::registries::sound_meta::SoundId,
    /// `UnitType.deathSoundVolume`.
    pub death_volume: f32,
    /// `UnitType.wreckSound` (`none`/`unset` = silent).
    pub wreck_sound: crate::content::registries::sound_meta::SoundId,
    /// `UnitType.wreckSoundVolume`.
    pub wreck_volume: f32,
}

impl Default for UnitAudioComp {
    fn default() -> Self {
        UnitAudioComp {
            death_sound: crate::content::registries::sound_meta::SoundId::UNSET,
            death_volume: 1.0,
            wreck_sound: crate::content::registries::sound_meta::SoundId::UNSET,
            wreck_volume: 1.0,
        }
    }
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

/// One leg (`entities/Leg.java`; transient IK state, not serialized).
///
/// Positions are world-space; `base` is the hip, `joint` the knee, `foot` the
/// planted endpoint. `LegsComp::reset_legs` initializes them and
/// [`super::legs::update_legs`] steps them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Leg {
    /// Hip x.
    pub base_x: f32,
    /// Hip y.
    pub base_y: f32,
    /// Knee x (`InverseKinematics` joint).
    pub joint_x: f32,
    /// Knee y.
    pub joint_y: f32,
    /// Foot x (planted endpoint).
    pub foot_x: f32,
    /// Foot y.
    pub foot_y: f32,
    /// Leg index.
    pub index: u8,
    /// Whether this leg is mid-step.
    pub moving: bool,
    /// Step interpolation `0..1`.
    pub step: f32,
    /// Whether the leg is on the right side (`flip_leg_side`).
    pub side: bool,
    /// Leg group index (`legGroupSize`).
    pub group: u8,
    /// World-space angle of the hip from the body center, in degrees.
    pub angle: f32,
}

/// Legs state (`LegsComp`): the `Leg[]` IK array plus walk animation.
#[derive(Debug, Clone, PartialEq, Component)]
pub struct LegsComp {
    /// Number of legs (`legs.length`).
    pub leg_count: u8,
    /// Leg IK state, in leg order.
    pub legs: Vec<Leg>,
    /// Base rotation decoupled from the body (`baseRotation`, legs variant).
    pub base_rotation: f32,
    /// Walk animation timer.
    pub walk_time: f32,
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
    pub parent: Option<Entity>,
    /// Segment index (`0` = head).
    pub index: u8,
}

/// Item inventory (`ItemsComp.stack`). A unit carries one item type at a time.
#[derive(Debug, Clone, Copy, PartialEq, Component, Default)]
pub struct ItemsComp {
    /// Held item + amount (`ItemStack`).
    pub item: Option<(ItemId, i32)>,
}

impl ItemsComp {
    /// Adds `amount` of `item` (`UnitComp.addItem`).
    pub fn add_item(&mut self, item: ItemId, amount: i32) {
        if amount <= 0 {
            return;
        }
        match self.item {
            Some((existing, current)) if existing == item => {
                self.item = Some((item, current.saturating_add(amount)));
            }
            _ => self.item = Some((item, amount)),
        }
    }

    /// Clears the inventory (`UnitComp.clearItem`).
    pub fn clear_item(&mut self) {
        self.item = None;
    }
}

/// Shield pool (`ShieldComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component, Default)]
pub struct ShieldComp {
    /// Current shield points (`shield`).
    pub shield: f32,
}

/// One active status effect (`StatusEntry`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusEntry {
    /// Effect content id.
    pub effect: StatusId,
    /// Remaining duration in ticks (`time`).
    pub duration: f32,
}

/// Status effect list (`StatusComp`); application/extend/cancel from plan 10.
#[derive(Debug, Clone, PartialEq, Component, Default)]
pub struct StatusComp {
    /// Active entries, in application order.
    pub statuses: Vec<StatusEntry>,
}

impl StatusComp {
    /// Applies `entry`, extending an existing same-effect duration (`StatusComp.apply`).
    pub fn apply(&mut self, entry: StatusEntry) {
        if let Some(existing) = self
            .statuses
            .iter_mut()
            .find(|status| status.effect == entry.effect)
        {
            existing.duration = existing.duration.max(entry.duration);
        } else {
            self.statuses.push(entry);
        }
    }

    /// Whether any status is active.
    pub fn has_effect(&self) -> bool {
        !self.statuses.is_empty()
    }

    /// Cancels every entry for `effect`.
    pub fn remove(&mut self, effect: StatusId) {
        self.statuses.retain(|status| status.effect != effect);
    }
}

/// Mining state (`MinerComp.mineTile`); mining logic lands with M2's `MinerAI`.
#[derive(Debug, Clone, Copy, PartialEq, Component, Default)]
pub struct MinerComp {
    /// Tile being mined (`mineTile`).
    pub mine_tile: Option<TilePos>,
    /// Whether the unit is actively mining this tick (`mining`).
    pub mining: bool,
}

/// Build plan queue size (`BuilderComp`); plans are plan 11 M2/M5.
#[derive(Debug, Clone, Copy, PartialEq, Component, Default)]
pub struct BuilderComp {
    /// Number of queued build plans (`plans.size`).
    pub plan_count: u32,
}

/// Spawner tether (`UnitTetherComp.spawnerUnit`).
#[derive(Debug, Clone, Copy, PartialEq, Component, Default)]
pub struct UnitTetherComp {
    /// Spawning building entity, if resolved.
    pub spawner: Option<Entity>,
}

/// Parent link for segmented/child units (`ChildComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component, Default)]
pub struct ChildComp {
    /// Parent entity.
    pub parent: Option<Entity>,
}

/// Ownership link (`OwnerComp.owner`).
#[derive(Debug, Clone, Copy, PartialEq, Component, Default)]
pub struct OwnerComp {
    /// Owning entity, if any.
    pub owner: Option<Entity>,
}

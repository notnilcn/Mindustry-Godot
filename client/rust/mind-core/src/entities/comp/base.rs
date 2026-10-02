// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Base components (`@BaseComponent` equivalent).
//!
//! Ported from `core/src/mindustry/entities/comp/EntityComp.java` and
//! `PosTeamDef.java`. `self()`/`as()` disappear: Bevy provides the entity
//! handle and `world.get::<T>()`.

use bevy_ecs::component::Component;
use mind_macros::SimComponent;

use crate::entities::meta::EntityDefId;

/// Marks an added entity (`@BaseComponent` lifetime flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, SimComponent)]
#[sim(component, base)]
pub struct BaseEntity {
    /// Whether `add()` completed for this entity.
    #[sim(transient)]
    pub added: bool,
}

/// Monotonic simulation entity id (`id` field, allocated by `EntityIds`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Component, SimComponent)]
#[sim(component, base)]
pub struct SimId {
    /// Entity id (save/network ABI, monotonic, never reused).
    #[sim(transient)]
    pub id: i32,
}

/// Internal entity-def index (`classId()` predecessor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, SimComponent)]
#[sim(component, base)]
pub struct DefId {
    /// Registry def id.
    #[sim(transient)]
    pub def: EntityDefId,
}

/// Marker: entity is locally simulated (`isLocal()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, SimComponent)]
#[sim(component, base)]
pub struct Local;

/// Marker: entity is controlled by a remote player (`isRemote()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, SimComponent)]
#[sim(component, base)]
pub struct Remote;

/// World position in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Component, SimComponent)]
#[sim(component, base)]
pub struct Pos {
    /// X in world units.
    pub x: f32,
    /// Y in world units.
    pub y: f32,
}

/// Velocity in pixels/tick.
#[derive(Debug, Clone, Copy, PartialEq, Component, SimComponent)]
#[sim(component, base)]
pub struct Vel {
    /// X velocity.
    pub x: f32,
    /// Y velocity.
    pub y: f32,
}

/// Owning team id (`Team.get(id) & 0xff` semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, SimComponent)]
#[sim(component, base)]
pub struct TeamComp {
    /// Team id.
    pub team: u8,
}

impl BaseEntity {
    /// Creates an unadded base component.
    pub const fn new() -> Self {
        Self { added: false }
    }
}

impl Default for BaseEntity {
    fn default() -> Self {
        Self::new()
    }
}

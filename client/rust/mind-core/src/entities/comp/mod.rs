// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Ported entity components. Plan 05 owns the base set; later plans add the
//! `Unit`/`Building`/`Bullet`/`Player` and behavior components in place.

pub mod base;
pub mod building;
pub mod health;
pub mod markers;

pub use base::{BaseEntity, DefId, Local, Pos, Remote, SimId, TeamComp, Vel};
pub use building::{
    BeamDrillState, Building, CrafterState, DoorState, DrillState, PumpState, RadarState,
    SandboxState, Timers, WallState,
};
pub use health::{Health, hp};
pub use markers::{Bullet, Draw, EffectState, Player, PowerGraphUpdater, Unit, WeatherState};

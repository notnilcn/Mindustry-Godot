// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Ported entity components. Plan 05 owns the base set; later plans add the
//! `Unit`/`Building`/`Bullet`/`Player` and behavior components in place.

pub mod base;
pub mod markers;

pub use base::{BaseEntity, DefId, Local, Pos, Remote, SimId, TeamComp, Vel};
pub use markers::{
    Building, Bullet, Draw, EffectState, Player, PowerGraphUpdater, Unit, WeatherState,
};

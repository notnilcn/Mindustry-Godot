// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Group-defining marker components (the `@Component` kinds in `GroupDefs`).
//!
//! Ported from `core/src/mindustry/entities/comp/*Comp.java` class tags and the
//! `GroupDefs.java` component references. These are the sole membership keys for
//! [`crate::entities::groups::GROUP_DEFS`]; behavior lands in plans 07/10/11.

use bevy_ecs::component::Component;
use mind_macros::SimComponent;

/// `Groups.unit` and `Groups.all` exclusion key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, SimComponent)]
#[sim(component, base)]
pub struct Unit;

// NOTE: `Building` (the `Groups.build` membership key) is plan 07's data
// component in `entities/comp/building.rs`; it replaced the plan-05 marker so
// the archetype key and the building runtime state are the same component.

/// `Groups.bullet` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, SimComponent)]
#[sim(component, base)]
pub struct Bullet;

/// `Groups.player` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, SimComponent)]
#[sim(component, base)]
pub struct Player;

/// `Groups.effect` key (`EffectStateComp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, SimComponent)]
#[sim(component, base)]
pub struct EffectState;

/// `Groups.weather` key (`WeatherStateComp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, SimComponent)]
#[sim(component, base)]
pub struct WeatherState;

/// `Groups.powerGraph` key (`PowerGraphUpdaterComp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, SimComponent)]
#[sim(component, base)]
pub struct PowerGraphUpdater;

/// `Groups.draw` key (`DrawComp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, SimComponent)]
#[sim(component, base)]
pub struct Draw;

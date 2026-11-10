// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret behavior (`core/src/mindustry/world/blocks/defense/turrets/*.java`).
//!
//! Plan 10 §3.8. This milestone (M5) ports the `BaseTurret` → `ReloadTurret` →
//! `Turret` state machine plus the three core ammo families (`ItemTurret`,
//! `LiquidTurret`, `PowerTurret`): targeting, reload/coolant, ammo stacks,
//! warmup/recoil/heat and the shoot pipeline. The advanced turret classes
//! (`ContinuousTurret`, `LaserTurret`, `PointDefenseTurret`, …) are M6 and live
//! in sibling modules as they land.
//!
//! ## Integration note (plan 10 §3.2 fallback, R-10-1)
//!
//! Plan 07's [`crate::world::behavior::BuildingBehavior::update_tile`] receives
//! only `&mut World`, but turret shooting resolves bullet metadata from the
//! [`crate::content::ContentRegistry`]. Rather than duplicate the content
//! registry into every building behavior (or take a hard dependency on plan 11's
//! `TargetQueries`, which does not exist yet), this M5 slice drives turrets
//! through the plan-10 [`crate::combat::CombatHarness`], which owns the content
//! registry and the deterministic bullet pass. Plan 11/07 reconciliation will
//! hoist this into `BuildingBehavior` once the unit/target queries and a
//! building-side content handle land. The item/liquid hooks here are the same
//! code logistics will call through `accepted_item`/`handle_liquid`.

pub mod advanced;
pub mod behavior;
pub mod save;

mod ammo;
mod build_turret;
mod config;
#[cfg(test)]
mod find_target_tests;
mod math;
mod targeting;
#[cfg(test)]
mod tests;
mod types;
mod update;

pub use ammo::*;
pub use build_turret::*;
pub use config::*;
pub use math::*;
pub use targeting::*;
pub use types::*;
pub use update::*;

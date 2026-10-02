// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Damage system (`core/src/mindustry/entities/Damage.java`).
//!
//! Ported so far: armor math ([`armor`]) and area/complete damage ([`area`]).
//! Laser/line/pierce/explosion/status submodules land in later plan-10
//! milestones; public names mirror `Damage.java` overloads.

pub mod area;
pub mod armor;

pub use area::{DamageOptions, apply_health, complete_damage, damage_area};
pub use armor::{MIN_ARMOR_DAMAGE, apply_armor, apply_armor_opt};

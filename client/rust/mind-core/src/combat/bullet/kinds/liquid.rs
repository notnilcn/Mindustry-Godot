// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LiquidBulletType`/`SpaceLiquidBulletType` behavior
//! (`entities/bullet/{LiquidBulletType,SpaceLiquidBulletType}.java`).
//!
//! Puddle deposit and fire extinguish depend on plan 10 M3's `Puddles`/`Fires`;
//! until then the bullet travels and despawns without a deposit.

use super::super::behavior::BulletBehavior;

/// `LiquidBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct LiquidBehavior;

impl BulletBehavior for LiquidBehavior {}

/// Static liquid-behavior instance.
pub static LIQUID: LiquidBehavior = LiquidBehavior;

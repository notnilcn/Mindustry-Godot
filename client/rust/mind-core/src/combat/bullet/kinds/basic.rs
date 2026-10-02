// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BasicBulletType` behavior (`entities/bullet/BasicBulletType.java`).
//!
//! Vanilla bullets are field-configured only, so gameplay behavior equals the
//! base `BulletType`; the subclass contributes sprite/shrink/spin draw data,
//! which plan 16 reads from [`crate::content::BulletDef`].

use super::super::behavior::BulletBehavior;

/// `BasicBulletType` behavior (drawing-only differences).
#[derive(Debug, Default, Clone, Copy)]
pub struct BasicBehavior;

impl BulletBehavior for BasicBehavior {}

/// Static basic-behavior instance.
pub static BASIC: BasicBehavior = BasicBehavior;

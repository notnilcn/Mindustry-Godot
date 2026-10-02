// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `EmptyBulletType` behavior (`entities/bullet/EmptyBulletType.java`): no-op,
//! used by purely visual weapons.

use super::super::behavior::BulletBehavior;

/// `EmptyBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyBehavior;

impl BulletBehavior for EmptyBehavior {}

/// Static empty-behavior instance.
pub static EMPTY: EmptyBehavior = EmptyBehavior;

// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Production-family behavior registration (`world/blocks/production/*`).
//!
//! PAR-0079: the Erekir wall miners need content lookups at build time — the
//! plasma bores scan wall ore in front of the block and gate on `Item.hardness`,
//! so their behaviors capture the hardness table here instead of the
//! content-free [`crate::world::behavior::default_behavior`] fallback.
//! The cliff crushers (`WallCrafter`) only read `BlockDef.attributes` and stay
//! in `default_behavior`.

use crate::content::ContentRegistry;
use crate::world::behavior::BehaviorRegistry;

/// Registers the content-dependent production behaviors.
pub fn register(registry: &mut BehaviorRegistry, content: &ContentRegistry) {
    for name in ["plasma-bore", "large-plasma-bore"] {
        registry.register_named(
            name,
            std::sync::Arc::new(
                crate::world::behavior::production::BeamDrillBehavior::from_content(content),
            ),
        );
    }
}

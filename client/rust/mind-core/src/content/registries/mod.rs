// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla registry modules.
//!
//! Ported from `core/src/mindustry/content/*.java`. `create_base_content` follows
//! the upstream `ContentLoader.createBaseContent()` order; M1 ports
//! items/statuses/liquids/bullets, M2 the small registries + tech trees, M3/M4
//! blocks, M5 units.

pub mod blocks;
pub mod bullets;
pub mod commands;
pub mod fx_meta;
pub mod items;
pub mod liquids;
pub mod loadouts;
pub mod pal;
pub mod planets;
pub mod sectors;
pub mod sound_meta;
pub mod stances;
pub mod statuses;
pub mod teams;
pub mod units;
pub mod weathers;

use super::ContentError;
use super::bundle::BundleView;
use super::load::ContentRegistry;
use super::settings_store::UnlockStore;
use super::tech::{TechTreeBuilder, ekir, serpulo};

/// Creates all base (vanilla) content.
///
/// Upstream order: `UnitCommand → TeamEntries → Items → UnitStance →
/// StatusEffects → Liquids → Bullets → UnitTypes → Blocks → Loadouts → Weathers
/// → Planets → SectorPresets → SerpuloTechTree → ErekirTechTree`. M3/M5 slot the
/// blocks/units registries between bullets and loadouts (plan 02 §3.4).
pub fn create_base_content(
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
    headless: bool,
) -> Result<ContentRegistry, ContentError> {
    let mut registry = ContentRegistry::new(headless);
    commands::load(&mut registry)?;
    teams::load(&mut registry, bundle, store)?;
    items::load(&mut registry, bundle, store)?;
    stances::load(&mut registry)?;
    statuses::load(&mut registry, bundle, store)?;
    liquids::load(&mut registry, bundle, store)?;
    bullets::load(&mut registry)?;
    units::load_into(&mut registry, bundle, store)?;
    blocks::load_into(&mut registry, bundle, store)?;
    loadouts::load(&mut registry)?;
    weathers::load(&mut registry, bundle, store)?;
    planets::load(&mut registry, bundle, store)?;
    sectors::load(&mut registry, bundle, store, &sectors::IdentityRemap)?;

    // SerpuloTechTree.load
    let serpulo_planet = registry.planet_id("serpulo");
    let (serpulo_tree, serpulo_report) = {
        let mut builder = TechTreeBuilder::new(&mut registry, store);
        serpulo::load(&mut builder);
        builder.finish()
    };
    if let Some(planet) = serpulo_planet {
        registry.set_planet_tech_tree(planet, serpulo_tree);
    }
    registry.push_tech_report("serpulo", serpulo_report);

    // ErekirTechTree.load: `rebalance()` runs first.
    ekir::rebalance(&mut registry);
    let erekir_planet = registry.planet_id("erekir");
    let (erekir_tree, erekir_report) = {
        let mut builder = TechTreeBuilder::new(&mut registry, store);
        ekir::load(&mut builder);
        builder.finish()
    };
    if let Some(planet) = erekir_planet {
        registry.set_planet_tech_tree(planet, erekir_tree);
    }
    registry.push_tech_report("erekir", erekir_report);

    Ok(registry)
}

/// Negative load-order scenario (plan 02 §7b/§7c): boots `Liquids` before
/// `StatusEffects`, which must fail because liquids reference statuses
/// (`UnknownName("wet")`). Used by `mind-headless content load-order-bad`.
#[doc(hidden)]
pub fn create_base_content_bad_order(
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
    headless: bool,
) -> Result<ContentRegistry, ContentError> {
    let mut registry = ContentRegistry::new(headless);
    commands::load(&mut registry)?;
    items::load(&mut registry, bundle, store)?;
    // Deliberately wrong: breaks the `StatusEffects -> Liquids` dependency.
    liquids::load(&mut registry, bundle, store)?;
    statuses::load(&mut registry, bundle, store)?;
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::super::{ContentError, MemoryBundle, MemoryUnlockStore};

    /// `content::registries::tests::load_order_bad` — the negative scenario the
    /// harness exposes as `content load-order-bad`.
    #[test]
    fn load_order_bad() {
        match super::create_base_content_bad_order(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        ) {
            Err(error) => assert_eq!(error, ContentError::UnknownName(String::from("wet"))),
            Ok(_) => panic!("liquids before statuses must fail"),
        }
    }
}

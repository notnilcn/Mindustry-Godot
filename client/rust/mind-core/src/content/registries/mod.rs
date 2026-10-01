// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla registry modules.
//!
//! Ported from `core/src/mindustry/content/*.java`. `create_base_content` follows
//! the upstream `ContentLoader.createBaseContent()` order; milestones land the
//! registries in relative order (M1: items/statuses/liquids/bullets; M2 adds the
//! small registries + tech trees; M3/M5 add blocks/units).

pub mod blocks;
pub mod bullets;
pub mod fx_meta;
pub mod items;
pub mod liquids;
pub mod statuses;

use super::ContentError;
use super::bundle::BundleView;
use super::load::ContentRegistry;
use super::settings_store::UnlockStore;

/// Creates all base (vanilla) content.
///
/// Upstream order: `UnitCommand → TeamEntries → Items → UnitStance →
/// StatusEffects → Liquids → Bullets → UnitTypes → Blocks → Loadouts → Weathers
/// → Planets → SectorPresets → SerpuloTechTree → ErekirTechTree`. M1 ships the
/// middle slice; the omitted loaders land in later milestones at the same
/// relative positions (plan 02 §3.4).
pub fn create_base_content(
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
    headless: bool,
) -> Result<ContentRegistry, ContentError> {
    let mut registry = ContentRegistry::new(headless);
    items::load(&mut registry, bundle, store)?;
    statuses::load(&mut registry, bundle, store)?;
    liquids::load(&mut registry, bundle, store)?;
    bullets::load(&mut registry)?;
    Ok(registry)
}

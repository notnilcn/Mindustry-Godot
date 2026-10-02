// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla entity archetypes (`@EntityDef` set) and the boot registry.
//!
//! Ported from `annotations/.../entity/EntityProcess.java` `@EntityDef` lists and
//! `entities/GroupDefs.java`. Concrete `UnitType`/`Block` instances share these
//! archetypes (plan 05 §3.5.1); per-content defs are bound during content init.

use std::collections::BTreeMap;

use mind_macros::entity_def;

use super::comp::{self, base::*};
use super::mapping::EntityMapping;
use super::meta::{EntityRegistry, RegistryError, SimComponentMeta};

// Def names follow the upstream `@EntityDef` class names so `classids.properties`
// resolves (`BuildingComp=6`, `BulletComp=7`, ...). `Unit` is per-type upstream
// (e.g. `mace=4`) and has no single class id; its id falls back to the mod range.
entity_def! {
    Unit = [BaseEntity, SimId, DefId, Pos, Vel, TeamComp, comp::Unit];
    BuildingComp = [BaseEntity, SimId, DefId, Pos, TeamComp, Health, Building, Timers];
    BulletComp = [BaseEntity, SimId, DefId, Pos, Vel, comp::Bullet];
    PlayerComp = [BaseEntity, SimId, DefId, Pos, TeamComp, comp::Player];
    EffectStateComp = [BaseEntity, SimId, DefId, Pos, comp::EffectState];
    WeatherStateComp = [BaseEntity, SimId, DefId, comp::WeatherState];
    PowerGraphUpdaterComp = [BaseEntity, SimId, DefId, comp::PowerGraphUpdater];
    DrawComp = [BaseEntity, SimId, DefId, comp::Draw];
}

/// Registers the base + marker components.
pub fn register_base_components(registry: &mut EntityRegistry) {
    registry.register_component(BaseEntity::component_meta());
    registry.register_component(SimId::component_meta());
    registry.register_component(DefId::component_meta());
    registry.register_component(Pos::component_meta());
    registry.register_component(Vel::component_meta());
    registry.register_component(TeamComp::component_meta());
    registry.register_component(comp::Health::component_meta());
    registry.register_component(comp::Timers::component_meta());
    registry.register_component(comp::Local::component_meta());
    registry.register_component(comp::Remote::component_meta());
    registry.register_component(comp::Unit::component_meta());
    registry.register_component(comp::Building::component_meta());
    registry.register_component(comp::Bullet::component_meta());
    registry.register_component(comp::Player::component_meta());
    registry.register_component(comp::EffectState::component_meta());
    registry.register_component(comp::WeatherState::component_meta());
    registry.register_component(comp::PowerGraphUpdater::component_meta());
    registry.register_component(comp::Draw::component_meta());
}

/// Builds the vanilla registry from [`ENTITY_DEF_SPECS`] and the committed
/// `classids.properties` class-id ABI.
pub fn vanilla_registry() -> Result<EntityRegistry, RegistryError> {
    let mut registry = EntityRegistry::new();
    register_base_components(&mut registry);
    let class_ids: BTreeMap<String, u16> = EntityMapping::load_default().class_id_map();
    registry.build_from_specs(ENTITY_DEF_SPECS, &class_ids)?;
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::groups::GroupKind;

    #[test]
    fn vanilla_defs_resolve_with_groups() {
        let registry = vanilla_registry().expect("vanilla registry builds");
        assert_eq!(registry.defs_len(), 8);
        let unit = registry.def_by_name("Unit").expect("unit");
        assert!(unit.groups.contains(GroupKind::Unit));
        assert!(!unit.groups.contains(GroupKind::All));
        let effect = registry.def_by_name("EffectStateComp").expect("effect");
        // Effects update in their own slot, so they are excluded from `all`.
        assert!(effect.groups.contains(GroupKind::Effect));
        assert!(!effect.groups.contains(GroupKind::All));
        let building = registry.def_by_name("BuildingComp").expect("building");
        assert_eq!(
            building.class_id, 6,
            "class id comes from classids.properties"
        );
    }

    #[test]
    fn metadata_json_is_stable() {
        let registry = vanilla_registry().expect("registry");
        let json = registry.metadata_json();
        assert_eq!(json["format"], 1);
        assert_eq!(json["defs"].as_array().map(Vec::len), Some(8));
        let unit = json["defs"]
            .as_array()
            .and_then(|defs| defs.iter().find(|def| def["name"] == "Unit"))
            .expect("unit def");
        // Aggregated fields include the base set in component order.
        assert!(unit["fields"].as_array().is_some_and(|f| !f.is_empty()));
        assert!(unit["groups"].as_array().is_some());
    }
}

// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/ErekirTechTree.java
//
//! Vanilla tech-tree node data (flat pre-order; depth tracks Java nesting).
//!
//! Generated once from the upstream source with a mechanical converter; node
//! order, parents, objectives and produce nodes match `TechTree.node` nesting.
//! Names for content whose registries land in M3/M5 stay unresolved until then
//! (`TechTreeBuildReport::missing`), the same data resolving completely afterwards.

use super::super::id::BulletId;
use super::super::load::ContentRegistry;
use super::{NodeObjective as obj, TechTreeBuilder};

/// Loads the tree into `t`.
pub fn load(t: &mut TechTreeBuilder<'_>) {
    t.root("erekir", "core-bastion", true);
    t.set_cost_multipliers(&[
        ("copper", 0.9),
        ("lead", 0.9),
        ("metaglass", 0.9),
        ("graphite", 0.9),
        ("sand", 0.9),
        ("coal", 0.9),
        ("titanium", 0.9),
        ("thorium", 0.9),
        ("scrap", 0.9),
        ("silicon", 0.9),
        ("plastanium", 0.9),
        ("phase-fabric", 0.9),
        ("surge-alloy", 0.9),
        ("spore-pod", 0.9),
        ("blast-compound", 0.9),
        ("pyratite", 0.9),
        ("beryllium", 0.9),
        ("tungsten", 0.9),
        ("oxide", 0.9),
        ("carbide", 0.9),
        ("fissile-matter", 0.9),
        ("dormant-cyst", 0.9),
        ("oxide", 0.5),
        ("surge-alloy", 0.7),
        ("carbide", 0.3),
        ("phase-fabric", 0.2),
    ]);
    t.at_obj(1, "duct", &[obj::OnPlanet("erekir")]);
    t.at(2, "duct-router");
    t.at(3, "duct-bridge");
    t.at(4, "armored-duct");
    t.at(5, "surge-conveyor");
    t.at(6, "surge-router");
    t.at(4, "unit-cargo-loader");
    t.at(5, "unit-cargo-unload-point");
    t.at_obj(3, "overflow-duct", &[obj::OnSector("aegis")]);
    t.at(4, "underflow-duct");
    t.at(4, "reinforced-container");
    t.at(5, "duct-unloader");
    t.at(5, "reinforced-vault");
    t.at_obj(3, "reinforced-message", &[obj::OnSector("aegis")]);
    t.at(4, "canvas");
    t.at(5, "large-canvas");
    t.at_obj(2, "reinforced-payload-conveyor", &[obj::OnSector("atlas")]);
    t.at_obj(
        3,
        "payload-mass-driver",
        &[obj::Research("silicon-arc-furnace"), obj::OnSector("split")],
    );
    t.at(4, "payload-loader");
    t.at(5, "payload-unloader");
    t.at(6, "large-payload-mass-driver");
    t.at_obj(4, "constructor", &[obj::OnSector("split")]);
    t.at_obj(5, "small-deconstructor", &[obj::OnSector("peaks")]);
    t.at_obj(6, "large-constructor", &[obj::OnSector("siege")]);
    t.at_obj(6, "deconstructor", &[obj::OnSector("siege")]);
    t.at(3, "reinforced-payload-router");
    t.at(1, "plasma-bore");
    t.at_obj(2, "impact-drill", &[obj::OnSector("aegis")]);
    t.at_obj(3, "large-plasma-bore", &[obj::OnSector("caldera-erekir")]);
    t.at_obj(4, "eruption-drill", &[obj::OnSector("stronghold")]);
    t.at_obj(4, "large-cliff-crusher", &[obj::OnSector("stronghold")]);
    t.at(1, "turbine-condenser");
    t.at(2, "beam-node");
    t.at_obj(3, "vent-condenser", &[obj::OnSector("aegis")]);
    t.at_obj(4, "chemical-combustion-chamber", &[obj::OnSector("basin")]);
    t.at_obj(5, "pyrolysis-generator", &[obj::OnSector("crevice")]);
    t.at_obj(
        6,
        "flux-reactor",
        &[
            obj::OnSector("crossroads"),
            obj::Research("cyanogen-synthesizer"),
        ],
    );
    t.at_obj(7, "neoplasia-reactor", &[obj::OnSector("karst")]);
    t.at_obj(3, "beam-tower", &[obj::OnSector("peaks")]);
    t.at_obj(4, "beam-link", &[obj::OnSector("crossroads")]);
    t.at_obj(3, "regen-projector", &[obj::OnSector("peaks")]);
    t.at_obj(4, "build-tower", &[obj::OnSector("stronghold")]);
    t.at_obj(5, "shockwave-tower", &[obj::OnSector("siege")]);
    t.at_obj(2, "reinforced-conduit", &[obj::OnSector("aegis")]);
    t.at_obj(3, "reinforced-pump", &[obj::OnSector("basin")]);
    t.at(3, "reinforced-liquid-junction");
    t.at(4, "reinforced-bridge-conduit");
    t.at(4, "reinforced-liquid-router");
    t.at(5, "reinforced-liquid-container");
    t.at_obj(
        6,
        "reinforced-liquid-tank",
        &[obj::SectorComplete("intersect")],
    );
    t.at(2, "cliff-crusher");
    t.at(3, "silicon-arc-furnace");
    t.at_obj(4, "electrolyzer", &[obj::OnSector("atlas")]);
    t.at_obj(
        5,
        "oxidation-chamber",
        &[obj::Research("tank-refabricator"), obj::OnSector("marsh")],
    );
    t.at_obj(6, "surge-crucible", &[obj::OnSector("ravine")]);
    t.at_obj(6, "heat-redirector", &[obj::OnSector("ravine")]);
    t.at_obj(
        7,
        "electric-heater",
        &[obj::OnSector("ravine"), obj::Research("afflict")],
    );
    t.at_obj(8, "slag-heater", &[obj::OnSector("caldera-erekir")]);
    t.at_obj(
        8,
        "atmospheric-concentrator",
        &[obj::OnSector("caldera-erekir")],
    );
    t.at_obj(9, "cyanogen-synthesizer", &[obj::OnSector("siege")]);
    t.at_obj(8, "carbide-crucible", &[obj::OnSector("crevice")]);
    t.at_obj(9, "phase-synthesizer", &[obj::OnSector("karst")]);
    t.at_obj(10, "phase-heater", &[obj::Research("phase-synthesizer")]);
    t.at(8, "heat-router");
    t.at(9, "small-heat-redirector");
    t.at_obj(5, "slag-incinerator", &[obj::OnSector("basin")]);
    t.at_obj(
        1,
        "breach",
        &[
            obj::Research("silicon-arc-furnace"),
            obj::Research("tank-fabricator"),
        ],
    );
    t.at(2, "beryllium-wall");
    t.at(3, "beryllium-wall-large");
    t.at(3, "tungsten-wall");
    t.at(4, "tungsten-wall-large");
    t.at(5, "blast-door");
    t.at(4, "reinforced-surge-wall");
    t.at(5, "reinforced-surge-wall-large");
    t.at(6, "shielded-wall");
    t.at(4, "carbide-wall");
    t.at(5, "carbide-wall-large");
    t.at_obj(2, "diffuse", &[obj::OnSector("lake")]);
    t.at_obj(3, "sublimate", &[obj::OnSector("marsh")]);
    t.at_obj(4, "afflict", &[obj::OnSector("ravine")]);
    t.at_obj(5, "titan", &[obj::OnSector("stronghold")]);
    t.at_obj(6, "lustre", &[obj::OnSector("crevice")]);
    t.at_obj(7, "smite", &[obj::OnSector("karst")]);
    t.at_obj(3, "disperse", &[obj::OnSector("stronghold")]);
    t.at_obj(4, "scathe", &[obj::OnSector("siege")]);
    t.at_obj(5, "malign", &[obj::SectorComplete("karst")]);
    t.at_obj(
        2,
        "radar",
        &[
            obj::Research("beam-node"),
            obj::Research("turbine-condenser"),
            obj::Research("tank-fabricator"),
            obj::OnSector("aegis"),
        ],
    );
    t.at_obj(1, "core-citadel", &[obj::SectorComplete("peaks")]);
    t.at_obj(2, "core-acropolis", &[obj::SectorComplete("siege")]);
    t.at_obj(
        1,
        "tank-fabricator",
        &[
            obj::Research("silicon-arc-furnace"),
            obj::Research("plasma-bore"),
            obj::Research("turbine-condenser"),
        ],
    );
    t.at(2, "stell");
    t.at_obj(
        2,
        "unit-repair-tower",
        &[obj::OnSector("ravine"), obj::Research("mech-refabricator")],
    );
    t.at_obj(2, "ship-fabricator", &[obj::OnSector("lake")]);
    t.at(3, "elude");
    t.at_obj(3, "mech-fabricator", &[obj::OnSector("intersect")]);
    t.at(4, "merui");
    t.at_obj(4, "tank-refabricator", &[obj::OnSector("atlas")]);
    t.at(5, "locus");
    t.at_obj(5, "mech-refabricator", &[obj::OnSector("basin")]);
    t.at(6, "cleroi");
    t.at_obj(6, "ship-refabricator", &[obj::OnSector("peaks")]);
    t.at(7, "avert");
    t.at_obj(7, "prime-refabricator", &[obj::OnSector("stronghold")]);
    t.at(8, "precept");
    t.at(8, "anthicus");
    t.at(8, "obviate");
    t.at_obj(
        7,
        "tank-assembler",
        &[
            obj::OnSector("siege"),
            obj::Research("constructor"),
            obj::Research("atmospheric-concentrator"),
        ],
    );
    t.at(8, "vanquish");
    t.at_obj(9, "conquer", &[obj::OnSector("karst")]);
    t.at_obj(8, "ship-assembler", &[obj::OnSector("crossroads")]);
    t.at(9, "quell");
    t.at_obj(10, "disrupt", &[obj::OnSector("karst")]);
    t.at_obj(8, "mech-assembler", &[obj::OnSector("crossroads")]);
    t.at(9, "tecta");
    t.at_obj(10, "collaris", &[obj::OnSector("karst")]);
    t.at_obj(8, "basic-assembler-module", &[obj::SectorComplete("karst")]);
    t.at(1, "onset");
    t.at_obj(
        2,
        "aegis",
        &[
            obj::SectorComplete("onset"),
            obj::Research("duct-router"),
            obj::Research("duct-bridge"),
        ],
    );
    t.at_obj(3, "lake", &[obj::SectorComplete("aegis")]);
    t.at_obj(
        3,
        "intersect",
        &[
            obj::SectorComplete("aegis"),
            obj::SectorComplete("lake"),
            obj::Research("vent-condenser"),
            obj::Research("ship-fabricator"),
        ],
    );
    t.at_obj(
        4,
        "atlas",
        &[
            obj::SectorComplete("intersect"),
            obj::Research("mech-fabricator"),
        ],
    );
    t.at_obj(
        5,
        "split",
        &[
            obj::SectorComplete("atlas"),
            obj::Research("reinforced-payload-conveyor"),
            obj::Research("reinforced-container"),
        ],
    );
    t.at_obj(5, "basin", &[obj::SectorComplete("atlas")]);
    t.at_obj(6, "marsh", &[obj::SectorComplete("basin")]);
    t.at_obj(
        7,
        "ravine",
        &[obj::SectorComplete("marsh"), obj::Research("slag")],
    );
    t.at_obj(
        8,
        "caldera-erekir",
        &[
            obj::SectorComplete("peaks"),
            obj::SectorComplete("ravine"),
            obj::Research("heat-redirector"),
        ],
    );
    t.at_obj(
        9,
        "stronghold",
        &[
            obj::SectorComplete("caldera-erekir"),
            obj::Research("core-citadel"),
        ],
    );
    t.at_obj(10, "crevice", &[obj::SectorComplete("stronghold")]);
    t.at_obj(11, "siege", &[obj::SectorComplete("crevice")]);
    t.at_obj(12, "crossroads", &[obj::SectorComplete("siege")]);
    t.at_obj(
        13,
        "karst",
        &[
            obj::SectorComplete("crossroads"),
            obj::Research("core-acropolis"),
        ],
    );
    t.at_obj(
        14,
        "origin",
        &[
            obj::SectorComplete("karst"),
            obj::Research("core-acropolis"),
            obj::Research("vanquish"),
            obj::Research("disrupt"),
            obj::Research("collaris"),
            obj::Research("malign"),
            obj::Research("basic-assembler-module"),
            obj::Research("neoplasia-reactor"),
        ],
    );
    t.at_obj(
        7,
        "peaks",
        &[obj::SectorComplete("marsh"), obj::SectorComplete("split")],
    );
    t.at_obj(1, "beryllium", &[obj::Produce("beryllium")]);
    t.at_obj(2, "sand", &[obj::Produce("sand")]);
    t.at_obj(3, "silicon", &[obj::Produce("silicon")]);
    t.at_obj(4, "oxide", &[obj::Produce("oxide")]);
    t.at_obj(2, "water", &[obj::Produce("water")]);
    t.at_obj(3, "ozone", &[obj::Produce("ozone")]);
    t.at_obj(4, "hydrogen", &[obj::Produce("hydrogen")]);
    t.at_obj(5, "nitrogen", &[obj::Produce("nitrogen")]);
    t.at_obj(5, "cyanogen", &[obj::Produce("cyanogen")]);
    t.at_obj(6, "neoplasm", &[obj::Produce("neoplasm")]);
    t.at_obj(2, "graphite", &[obj::Produce("graphite")]);
    t.at_obj(3, "tungsten", &[obj::Produce("tungsten")]);
    t.at_obj(4, "slag", &[obj::Produce("slag")]);
    t.at_obj(4, "arkycite", &[obj::Produce("arkycite")]);
    t.at_obj(4, "thorium", &[obj::Produce("thorium")]);
    t.at_obj(5, "carbide", &[obj::Produce("carbide")]);
    t.at_obj(4, "surge-alloy", &[obj::Produce("surge-alloy")]);
    t.at_obj(5, "phase-fabric", &[obj::Produce("phase-fabric")]);
}

/// Scale-once guard (`ErekirTechTree.balanced` / `IntSet`).
#[derive(Debug, Default)]
pub struct RebalanceSet {
    balanced: std::collections::BTreeSet<u16>,
}

impl RebalanceSet {
    /// Empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks `bullet` balanced, returning true the first time only.
    fn add(&mut self, bullet: BulletId) -> bool {
        self.balanced.insert(bullet.raw())
    }

    /// Number of scaled bullets.
    pub fn len(&self) -> usize {
        self.balanced.len()
    }

    /// Whether no bullet was scaled.
    pub fn is_empty(&self) -> bool {
        self.balanced.is_empty()
    }
}

/// `rebalanceBullet(bullet)`: multiplies damage by 0.75 once per bullet id.
pub fn rebalance_bullet(
    registry: &mut ContentRegistry,
    bullet: BulletId,
    balanced: &mut RebalanceSet,
) {
    if balanced.add(bullet)
        && let Some(def) = registry.bullet_mut(bullet)
    {
        def.damage *= 0.75;
    }
}

/// `ErekirTechTree.rebalance()` (`ErekirTechTree.java:26-48`).
///
/// Scales `damage *= 0.75` once per bullet id for (a) every weapon bullet of
/// `ErekirUnitType` units (which includes `TankUnitType` by inheritance, but
/// not `MissileUnitType`/`NeoplasmUnitType`), and (b) turret ammo bullets of
/// turrets with non-Serpulo requirements. Part (b) stays a documented no-op
/// until turret ammo lands with plan 10 (`Blocks.java` inline ammo bullets are
/// not registered yet; see the plan-02 M5 changelog).
pub fn rebalance(registry: &mut ContentRegistry) {
    let mut balanced = RebalanceSet::new();
    let mut bullets: Vec<BulletId> = Vec::new();
    for unit in registry.units() {
        if !matches!(
            unit.kind,
            super::super::registries::units::UnitKind::ErekirUnitType
                | super::super::registries::units::UnitKind::TankUnitType
        ) {
            continue;
        }
        for weapon in &unit.weapons {
            bullets.push(weapon.bullet.id);
        }
    }
    for bullet in bullets {
        rebalance_bullet(registry, bullet, &mut balanced);
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::load::ContentRegistry;
    use super::super::super::registries::bullets::{BulletDef, BulletKind};
    use super::*;

    /// `erekir::tests::rebalance_applies_once` (plan 02 §5 M2).
    #[test]
    fn rebalance_applies_once() {
        let mut registry = ContentRegistry::new(true);
        let shared = registry
            .add_bullet(BulletDef::new(BulletKind::Plain))
            .unwrap();
        let other = registry
            .add_bullet(BulletDef::new(BulletKind::Plain))
            .unwrap();
        assert_eq!(registry.bullet(shared).unwrap().damage, 1.0);

        let mut balanced = RebalanceSet::new();
        rebalance_bullet(&mut registry, shared, &mut balanced);
        rebalance_bullet(&mut registry, shared, &mut balanced);
        assert_eq!(
            registry.bullet(shared).unwrap().damage,
            0.75,
            "shared bullet scales once"
        );

        rebalance_bullet(&mut registry, other, &mut balanced);
        assert_eq!(registry.bullet(other).unwrap().damage, 0.75);
        assert_eq!(balanced.len(), 2);

        // `rebalance()` itself is a no-op until units/blocks land (M5/M3).
        let damage = registry.bullet(shared).unwrap().damage;
        rebalance(&mut registry);
        assert_eq!(registry.bullet(shared).unwrap().damage, damage);
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan-11 entity-def vocabulary (`ComponentKind`, the 12 unit `@EntityDef`
//! groups) ported from `annotations/.../entity/EntityProcess.java` and
//! `content/UnitTypes.java`.
//!
//! Plan 02 owns the *content* metadata half ([`crate::content::registries::units::UnitTypeDef`]
//! / its `EntityDefSpec` with `UnitComponent` tags). This module owns the
//! **runtime** vocabulary: the concrete [`ComponentKind`] enum, the canonical
//! 12-def table ([`UNIT_DEFS`]) used by the unit bundle builders, and the bridge
//! from plan 02's content tags.
//!
//! Reconciliation note: plan 05's [`crate::entities::meta::EntityDefSpec`] is a
//! name-only spec used by the frozen `meta entities` golden. Plan 11 does **not**
//! mutate that registry (it would change the plan-05 parity artifact); it keeps
//! its own constant table here, per the plan §3.2 boundary.

use crate::content::registries::units::UnitComponent;

/// Runtime component vocabulary: upstream component class names minus `Comp`.
///
/// Variant names are the ABI (mod JSON + audit dumps); the numeric order is
/// frozen for on-disk/audit dumps but only names are load-bearing (plan §6.1).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ComponentKind {
    // --- unit base closure (`UnitComp` implemented interfaces) ---
    /// `PosComp`.
    Pos = 0,
    /// `RotComp`.
    Rot = 1,
    /// `VelComp`.
    Vel = 2,
    /// `TeamComp`.
    Team = 3,
    /// `HealthComp`.
    Health = 4,
    /// `PhysicsComp`.
    Physics = 5,
    /// `HitboxComp`.
    Hitbox = 6,
    /// `StatusComp`.
    Status = 7,
    /// `ItemsComp`.
    Items = 8,
    /// `WeaponsComp`.
    Weapons = 9,
    /// `DrawComp`.
    Draw = 10,
    /// `SyncComp`.
    Sync = 11,
    /// `ShieldComp`.
    Shield = 12,
    /// `MinerComp`.
    Miner = 13,
    /// `BuilderComp`.
    Builder = 14,
    /// `OwnerComp`.
    Owner = 15,
    /// `DamageComp`.
    Damage = 16,
    /// `TimerComp`.
    Timer = 17,
    // --- optional per-def kinds ---
    /// `MechComp`.
    Mech = 18,
    /// `LegsComp`.
    Legs = 19,
    /// `TankComp`.
    Tank = 20,
    /// `WaterMoveComp`.
    WaterMove = 21,
    /// `WaterCrawlComp`.
    WaterCrawl = 22,
    /// `UnderwaterMoveComp`.
    UnderwaterMove = 23,
    /// `CrawlComp`.
    Crawl = 24,
    /// `SegmentComp`.
    Segment = 25,
    /// `ElevationMoveComp`.
    ElevationMove = 26,
    /// `PayloadComp`.
    Payload = 27,
    /// `BlockUnitComp`.
    BlockUnit = 28,
    /// `BuildingTetherComp`.
    BuildingTether = 29,
    /// `UnitTetherComp`.
    UnitTether = 30,
    /// `ChildComp`.
    Child = 31,
    /// `TimedComp`.
    Timed = 32,
    /// `TimedKillComp`.
    TimedKill = 33,
    /// `TargetDummyComp`.
    TargetDummy = 34,
    /// `ShielderComp`.
    Shielder = 35,
    /// `PlayerComp` bridge (plan 15 possession).
    PlayerBridge = 36,
}

/// Number of [`ComponentKind`] variants (frozen).
pub const KIND_COUNT: usize = 37;

impl ComponentKind {
    /// ABI/audit name (Rust variant name; matches plan §6.1).
    pub const fn name(self) -> &'static str {
        match self {
            ComponentKind::Pos => "Pos",
            ComponentKind::Rot => "Rot",
            ComponentKind::Vel => "Vel",
            ComponentKind::Team => "Team",
            ComponentKind::Health => "Health",
            ComponentKind::Physics => "Physics",
            ComponentKind::Hitbox => "Hitbox",
            ComponentKind::Status => "Status",
            ComponentKind::Items => "Items",
            ComponentKind::Weapons => "Weapons",
            ComponentKind::Draw => "Draw",
            ComponentKind::Sync => "Sync",
            ComponentKind::Shield => "Shield",
            ComponentKind::Miner => "Miner",
            ComponentKind::Builder => "Builder",
            ComponentKind::Owner => "Owner",
            ComponentKind::Damage => "Damage",
            ComponentKind::Timer => "Timer",
            ComponentKind::Mech => "Mech",
            ComponentKind::Legs => "Legs",
            ComponentKind::Tank => "Tank",
            ComponentKind::WaterMove => "WaterMove",
            ComponentKind::WaterCrawl => "WaterCrawl",
            ComponentKind::UnderwaterMove => "UnderwaterMove",
            ComponentKind::Crawl => "Crawl",
            ComponentKind::Segment => "Segment",
            ComponentKind::ElevationMove => "ElevationMove",
            ComponentKind::Payload => "Payload",
            ComponentKind::BlockUnit => "BlockUnit",
            ComponentKind::BuildingTether => "BuildingTether",
            ComponentKind::UnitTether => "UnitTether",
            ComponentKind::Child => "Child",
            ComponentKind::Timed => "Timed",
            ComponentKind::TimedKill => "TimedKill",
            ComponentKind::TargetDummy => "TargetDummy",
            ComponentKind::Shielder => "Shielder",
            ComponentKind::PlayerBridge => "PlayerBridge",
        }
    }

    /// Parses a [`ComponentKind`] from its ABI name.
    pub fn from_name(name: &str) -> Option<ComponentKind> {
        ALL_KINDS.iter().copied().find(|kind| kind.name() == name)
    }
}

/// Every [`ComponentKind`] in numeric order.
pub static ALL_KINDS: [ComponentKind; KIND_COUNT] = [
    ComponentKind::Pos,
    ComponentKind::Rot,
    ComponentKind::Vel,
    ComponentKind::Team,
    ComponentKind::Health,
    ComponentKind::Physics,
    ComponentKind::Hitbox,
    ComponentKind::Status,
    ComponentKind::Items,
    ComponentKind::Weapons,
    ComponentKind::Draw,
    ComponentKind::Sync,
    ComponentKind::Shield,
    ComponentKind::Miner,
    ComponentKind::Builder,
    ComponentKind::Owner,
    ComponentKind::Damage,
    ComponentKind::Timer,
    ComponentKind::Mech,
    ComponentKind::Legs,
    ComponentKind::Tank,
    ComponentKind::WaterMove,
    ComponentKind::WaterCrawl,
    ComponentKind::UnderwaterMove,
    ComponentKind::Crawl,
    ComponentKind::Segment,
    ComponentKind::ElevationMove,
    ComponentKind::Payload,
    ComponentKind::BlockUnit,
    ComponentKind::BuildingTether,
    ComponentKind::UnitTether,
    ComponentKind::Child,
    ComponentKind::Timed,
    ComponentKind::TimedKill,
    ComponentKind::TargetDummy,
    ComponentKind::Shielder,
    ComponentKind::PlayerBridge,
];

/// The implicit `UnitComp` closure (plan §3.2 closure rule).
pub static BASE_CLOSURE: [ComponentKind; 18] = [
    ComponentKind::Pos,
    ComponentKind::Rot,
    ComponentKind::Vel,
    ComponentKind::Team,
    ComponentKind::Health,
    ComponentKind::Physics,
    ComponentKind::Hitbox,
    ComponentKind::Status,
    ComponentKind::Items,
    ComponentKind::Weapons,
    ComponentKind::Draw,
    ComponentKind::Sync,
    ComponentKind::Shield,
    ComponentKind::Miner,
    ComponentKind::Builder,
    ComponentKind::Owner,
    ComponentKind::Damage,
    ComponentKind::Timer,
];

/// One unit `@EntityDef` group (plan §3.2).
///
/// `components()` is `BASE_CLOSURE` followed by `extras`, in canonical order.
#[derive(Debug, Clone, Copy)]
pub struct UnitDefSpec {
    /// Entity-def element name used by revisions/classids (`"alpha"`, `"mace"`).
    pub def_name: &'static str,
    /// Display/group name of the merged class (`"Unit"`, `"MechUnit"`).
    pub class_name: &'static str,
    /// Components beyond the base closure, in canonical order.
    pub extras: &'static [ComponentKind],
    /// Upstream `@EntityDef(legacy = true)`.
    pub legacy: bool,
}

impl UnitDefSpec {
    /// Full component closure: base followed by extras.
    pub fn components(&self) -> Vec<ComponentKind> {
        let mut out = Vec::with_capacity(BASE_CLOSURE.len() + self.extras.len());
        out.extend_from_slice(&BASE_CLOSURE);
        out.extend_from_slice(self.extras);
        out
    }

    /// Whether `kind` is part of this def's closure.
    pub fn has(&self, kind: ComponentKind) -> bool {
        BASE_CLOSURE.contains(&kind) || self.extras.contains(&kind)
    }
}

/// The 12 canonical unit defs (plan §3.2 table), one per `@EntityDef` group.
pub static UNIT_DEFS: [UnitDefSpec; 12] = [
    UnitDefSpec {
        def_name: "alpha",
        class_name: "Unit",
        extras: &[],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "mace",
        class_name: "MechUnit",
        extras: &[ComponentKind::Mech, ComponentKind::ElevationMove],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "corvus",
        class_name: "LegsUnit",
        extras: &[ComponentKind::Legs],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "stell",
        class_name: "TankUnit",
        extras: &[ComponentKind::Tank, ComponentKind::ElevationMove],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "risso",
        class_name: "WaterMoveUnit",
        extras: &[ComponentKind::WaterMove],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "latum",
        class_name: "CrawlUnit",
        extras: &[ComponentKind::Crawl],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "elude",
        class_name: "ElevationMoveUnit",
        extras: &[ComponentKind::ElevationMove],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "mega",
        class_name: "PayloadUnit",
        extras: &[ComponentKind::Payload],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "manifold",
        class_name: "BuildingTetherUnit",
        extras: &[ComponentKind::BuildingTether, ComponentKind::Payload],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "missile",
        class_name: "TimedKillUnit",
        extras: &[ComponentKind::TimedKill, ComponentKind::Timed],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "block",
        class_name: "BlockUnit",
        extras: &[ComponentKind::BlockUnit],
        legacy: false,
    },
    UnitDefSpec {
        def_name: "dummy",
        class_name: "TargetDummyUnit",
        extras: &[ComponentKind::TargetDummy],
        legacy: false,
    },
];

/// Looks a def up by its `def_name`.
pub fn def_by_name(name: &str) -> Option<&'static UnitDefSpec> {
    UNIT_DEFS.iter().find(|def| def.def_name == name)
}

/// Bridges a plan-02 content [`UnitComponent`] tag into [`ComponentKind`].
pub fn kind_of(component: UnitComponent) -> ComponentKind {
    match component {
        UnitComponent::Unit => ComponentKind::Pos, // `Unitc` itself is the closure marker
        UnitComponent::Mech => ComponentKind::Mech,
        UnitComponent::Legs => ComponentKind::Legs,
        UnitComponent::ElevationMove => ComponentKind::ElevationMove,
        UnitComponent::WaterMove => ComponentKind::WaterMove,
        UnitComponent::Payload => ComponentKind::Payload,
        UnitComponent::BlockUnit => ComponentKind::BlockUnit,
        UnitComponent::BuildingTether => ComponentKind::BuildingTether,
        UnitComponent::TargetDummy => ComponentKind::TargetDummy,
        UnitComponent::Tank => ComponentKind::Tank,
        UnitComponent::TimedKill => ComponentKind::TimedKill,
        UnitComponent::Crawl => ComponentKind::Crawl,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_are_unique_and_round_trip() {
        assert_eq!(ALL_KINDS.len(), KIND_COUNT);
        for (i, kind) in ALL_KINDS.iter().enumerate() {
            assert_eq!(*kind as usize, i, "numeric order is frozen");
            assert_eq!(ComponentKind::from_name(kind.name()), Some(*kind));
        }
    }

    #[test]
    fn twelve_defs_match_upstream_shapes() {
        assert_eq!(UNIT_DEFS.len(), 12);
        let mace = def_by_name("mace").expect("mace");
        assert_eq!(mace.class_name, "MechUnit");
        assert!(mace.has(ComponentKind::Mech));
        assert!(mace.has(ComponentKind::ElevationMove));
        assert!(mace.has(ComponentKind::Health));
        assert!(!mace.has(ComponentKind::Legs));

        let manifold = def_by_name("manifold").expect("manifold");
        assert!(manifold.has(ComponentKind::BuildingTether));
        assert!(manifold.has(ComponentKind::Payload));

        for def in &UNIT_DEFS {
            assert_eq!(
                def.components().len(),
                BASE_CLOSURE.len() + def.extras.len()
            );
        }
    }
}

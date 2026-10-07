// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit blocks (plan 11 §3.11).
//!
//! Ported from `core/src/mindustry/world/blocks/units/*.java`: `UnitBlock`/
//! `UnitFactory`, `Reconstructor`, `UnitAssembler`/`UnitAssemblerModule`,
//! `RepairTower`, `RepairTurret`, `UnitCargoLoader`, `UnitCargoUnloadPoint`.
//!
//! This module owns the block **configuration and progress kernels** (the parts
//! that are pure and deterministic): factory plan requirement math and item
//! consumption, reconstructor upgrade validation, assembler tier/module rules,
//! repair refresh intervals and unload-point staleness. The plan-07
//! `BuildingBehavior` registration (`update_tile` item IO, revision manifests,
//! payload output) and plan-08 payload hand-off are the wiring half and are
//! recorded as deferred-with-owner (plan 07/08/11 reconciliation) in the plan
//! Changelog.

use crate::content::{ItemId, UnitTypeId};

pub mod behavior;

/// `UnitBlock` spawn handshake (subset; plan-08 payload wiring is the rest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitBlockKind {
    /// `UnitFactory` (ground/air/naval tiers).
    #[default]
    Factory,
    /// `Reconstructor`.
    Reconstructor,
    /// `UnitAssembler`.
    Assembler,
    /// `UnitAssemblerModule`.
    AssemblerModule,
    /// `RepairTower`.
    RepairTower,
    /// `RepairTurret`.
    RepairTurret,
    /// `UnitCargoLoader`.
    CargoLoader,
    /// `UnitCargoUnloadPoint`.
    CargoUnloadPoint,
    /// `UnitBlock` base (pads).
    UnitBlock,
}

impl UnitBlockKind {
    /// Java class-ish name (audit ABI).
    pub const fn name(self) -> &'static str {
        match self {
            UnitBlockKind::Factory => "UnitFactory",
            UnitBlockKind::Reconstructor => "Reconstructor",
            UnitBlockKind::Assembler => "UnitAssembler",
            UnitBlockKind::AssemblerModule => "UnitAssemblerModule",
            UnitBlockKind::RepairTower => "RepairTower",
            UnitBlockKind::RepairTurret => "RepairTurret",
            UnitBlockKind::CargoLoader => "UnitCargoLoader",
            UnitBlockKind::CargoUnloadPoint => "UnitCargoUnloadPoint",
            UnitBlockKind::UnitBlock => "UnitBlock",
        }
    }
}

/// One factory plan (`UnitFactory.UnitPlan`).
#[derive(Debug, Clone, PartialEq)]
pub struct UnitPlan {
    /// Unit produced.
    pub unit: UnitTypeId,
    /// Item requirements.
    pub requirements: Vec<(ItemId, i32)>,
    /// Construct time in ticks.
    pub time: f32,
}

impl UnitPlan {
    /// Creates a plan.
    pub fn new(unit: UnitTypeId, requirements: Vec<(ItemId, i32)>, time: f32) -> Self {
        Self {
            unit,
            requirements,
            time,
        }
    }

    /// Requirement amount for `item` (`0` if absent).
    pub fn requirement(&self, item: ItemId) -> i32 {
        self.requirements
            .iter()
            .find(|(candidate, _)| *candidate == item)
            .map(|(_, amount)| *amount)
            .unwrap_or(0)
    }
}

/// `UnitFactory` configuration (`plans` + derived `capacities`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitFactory {
    /// Production plans, in config order.
    pub plans: Vec<UnitPlan>,
    /// Indexed item capacity (`capacities[item index]`; two× max requirement).
    pub capacities: Vec<i32>,
}

impl UnitFactory {
    /// Builds a factory and derives capacities (`UnitFactory.init`).
    pub fn new(plans: Vec<UnitPlan>, item_count: usize) -> Self {
        let mut capacities = vec![0i32; item_count];
        for plan in &plans {
            for (item, amount) in &plan.requirements {
                let slot = &mut capacities[item.index()];
                let needed = (amount * 2).max(10);
                *slot = (*slot).max(needed);
            }
        }
        Self { plans, capacities }
    }

    /// Item capacity for `item` (`getMaximumAccepted`).
    pub fn capacity(&self, item: ItemId) -> i32 {
        self.capacities.get(item.index()).copied().unwrap_or(0)
    }

    /// Whether any plan needs `item` (`accepts`).
    pub fn accepts(&self, item: ItemId) -> bool {
        self.plans.iter().any(|plan| plan.requirement(item) > 0)
    }

    /// Whether the factory should consume items (`shouldConsume`).
    pub fn should_consume(&self, items: &[i32], plan_index: usize) -> bool {
        let Some(plan) = self.plans.get(plan_index) else {
            return false;
        };
        plan.requirements
            .iter()
            .all(|(item, amount)| items.get(item.index()).copied().unwrap_or(0) >= *amount)
    }
}

/// `UnitFactoryBuild` progress state (the mutable half).
#[derive(Debug, Clone, Default, PartialEq, bevy_ecs::component::Component)]
pub struct UnitFactoryBuild {
    /// Selected plan index.
    pub current_plan: usize,
    /// Accumulated build time.
    pub progress: f32,
    /// Stored item counts indexed by item id.
    pub items: Vec<i32>,
}

impl UnitFactoryBuild {
    /// Creates a build for `plan_count` plans and `item_count` items.
    pub fn new(_plan_count: usize, item_count: usize) -> Self {
        Self {
            current_plan: 0,
            progress: 0.0,
            items: vec![0; item_count],
        }
    }

    /// `UnitFactoryBuild.updateTile`: advance progress and produce a unit when
    /// the plan completes and its requirements are held.
    ///
    /// Returns the produced unit and resets progress.
    pub fn update(&mut self, factory: &UnitFactory, time_scale: f32) -> Option<UnitTypeId> {
        let plan = factory.plans.get(self.current_plan)?;
        if !factory.should_consume(&self.items, self.current_plan) {
            // Requirements not met: hold progress (upstream blocks consumption).
            return None;
        }
        self.progress += time_scale;
        if self.progress < plan.time {
            return None;
        }
        for (item, amount) in &plan.requirements {
            if let Some(slot) = self.items.get_mut(item.index()) {
                *slot -= *amount;
            }
        }
        self.progress = 0.0;
        Some(plan.unit)
    }
}

/// `Reconstructor` configuration (`upgrades`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reconstructor {
    /// `(from, to)` upgrade pairs.
    pub upgrades: Vec<(UnitTypeId, UnitTypeId)>,
    /// Construct time in ticks.
    pub construct_time: f32,
}

impl Reconstructor {
    /// Creates a reconstructor.
    pub fn new(upgrades: Vec<(UnitTypeId, UnitTypeId)>, construct_time: f32) -> Self {
        Self {
            upgrades,
            construct_time,
        }
    }

    /// `ReconstructorBuild.payload` validation: whether `unit` can upgrade.
    pub fn can_upgrade(&self, unit: UnitTypeId) -> Option<UnitTypeId> {
        self.upgrades
            .iter()
            .find(|(from, _)| *from == unit)
            .map(|(_, to)| *to)
    }
}

/// `ReconstructorBuild` progress state.
#[derive(Debug, Clone, Default, PartialEq, bevy_ecs::component::Component)]
pub struct ReconstructorBuild {
    /// Payload unit currently held (`None` = empty).
    pub payload: Option<UnitTypeId>,
    /// Accumulated construct time.
    pub progress: f32,
}

impl ReconstructorBuild {
    /// Accepts a payload unit only when the configured upgrade is valid.
    pub fn accept_payload(&mut self, reconstructor: &Reconstructor, unit: UnitTypeId) -> bool {
        if self.payload.is_some() {
            return false;
        }
        if reconstructor.can_upgrade(unit).is_none() {
            return false;
        }
        self.payload = Some(unit);
        self.progress = 0.0;
        true
    }

    /// `ReconstructorBuild.updateTile`: advance the upgrade; on completion returns
    /// the upgraded unit type.
    pub fn update(&mut self, reconstructor: &Reconstructor, time_scale: f32) -> Option<UnitTypeId> {
        let current = self.payload?;
        let target = reconstructor.can_upgrade(current)?;
        self.progress += time_scale;
        if self.progress < reconstructor.construct_time {
            return None;
        }
        self.payload = None;
        self.progress = 0.0;
        Some(target)
    }
}

/// One assembler plan payload requirement (`AssemblerUnitPlan.requirements`;
/// upstream a `PayloadStack` of a unit or block).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssemblerPayload {
    /// Whether the payload is a building (else a unit).
    pub is_block: bool,
    /// Raw block/unit content id.
    pub content: u16,
    /// Required count.
    pub amount: i32,
}

/// One assembler plan (`UnitAssembler.AssemblerUnitPlan`).
#[derive(Debug, Clone, PartialEq)]
pub struct AssemblerUnitPlan {
    /// Unit produced.
    pub unit: UnitTypeId,
    /// Payload requirements (`requirements`).
    pub payloads: Vec<AssemblerPayload>,
    /// Construct time in ticks.
    pub time: f32,
    /// Drones required.
    pub drones: i32,
}

/// `UnitAssembler` configuration/state.
#[derive(Debug, Clone, PartialEq, bevy_ecs::component::Component)]
pub struct UnitAssembler {
    /// Assembler square side (`areaSize`).
    pub area_size: i32,
    /// Plans, in tier order.
    pub plans: Vec<AssemblerUnitPlan>,
    /// Module tier registered by an adjacent module (`0` = none).
    pub module_tier: i32,
    /// How many drones were created (`dronesCreated`).
    pub drones_created: i32,
    /// Drone construct time.
    pub drone_construct_time: f32,
    /// Accumulated build progress (`progress`).
    pub progress: f32,
    /// Stored payloads (`blocks`/`payloads` sequences), keyed by plan entry.
    pub stored: Vec<(AssemblerPayload, i32)>,
}

/// `UnitAssembler.areaSize` default.
pub const ASSEMBLER_AREA_SIZE: i32 = 11;

/// Whether two plan payload entries denote the same content (the stored
/// counter keys ignore the requirement amount).
fn same_payload(a: AssemblerPayload, b: AssemblerPayload) -> bool {
    a.is_block == b.is_block && a.content == b.content
}

impl UnitAssembler {
    /// Creates an assembler with the upstream 11×11 area.
    pub fn new(plans: Vec<AssemblerUnitPlan>, drone_construct_time: f32) -> Self {
        Self {
            area_size: ASSEMBLER_AREA_SIZE,
            plans,
            module_tier: 0,
            drones_created: 0,
            drone_construct_time,
            progress: 0.0,
            stored: Vec::new(),
        }
    }

    /// `getRect` half-extent (`areaSize / 2`).
    pub fn side(&self) -> i32 {
        self.area_size / 2
    }

    /// Whether a module of `tier` is accepted (next unused tier).
    pub fn accepts_module(&self, tier: i32) -> bool {
        tier == self.module_tier + 1
    }

    /// Registers an accepted module tier.
    pub fn add_module(&mut self, tier: i32) -> bool {
        if self.accepts_module(tier) {
            self.module_tier = tier;
            true
        } else {
            false
        }
    }

    /// `UnitAssemblerBuild.moduleTier`-gated active plan (`plan()`): the plan
    /// index is `min(moduleTier, plans.len - 1)`.
    pub fn plan(&self) -> Option<&AssemblerUnitPlan> {
        if self.plans.is_empty() {
            return None;
        }
        let index = (self.module_tier.max(0) as usize).min(self.plans.len() - 1);
        self.plans.get(index)
    }

    /// Stored count for `payload`.
    pub fn stored(&self, payload: AssemblerPayload) -> i32 {
        self.stored
            .iter()
            .find(|(candidate, _)| same_payload(*candidate, payload))
            .map(|(_, amount)| *amount)
            .unwrap_or(0)
    }

    /// Whether the active plan accepts one more `payload`
    /// (`UnitAssemblerBuild.acceptPayload`; the plan is the module-tier one).
    pub fn accepts_payload(&self, payload: AssemblerPayload) -> bool {
        let Some(plan) = self.plan() else {
            return false;
        };
        plan.payloads.iter().any(|requirement| {
            same_payload(*requirement, payload) && self.stored(*requirement) < requirement.amount
        })
    }

    /// Stores one accepted payload (`moveInPayload` into `blocks`/`payloads`).
    pub fn store_payload(&mut self, payload: AssemblerPayload) -> bool {
        if !self.accepts_payload(payload) {
            return false;
        }
        match self
            .stored
            .iter_mut()
            .find(|(candidate, _)| same_payload(*candidate, payload))
        {
            Some((_, amount)) => *amount += 1,
            None => self.stored.push((payload, 1)),
        }
        true
    }

    /// Whether every active-plan requirement is satisfied
    /// (`shouldConsume` payload half).
    pub fn has_requirements(&self) -> bool {
        let Some(plan) = self.plan() else {
            return false;
        };
        plan.payloads
            .iter()
            .all(|requirement| self.stored(*requirement) >= requirement.amount)
    }

    /// Consumes the active plan's stored payloads (`consume()`).
    pub fn consume_requirements(&mut self) {
        let Some(plan) = self.plan() else {
            return;
        };
        let payloads = plan.payloads.clone();
        for requirement in &payloads {
            if let Some((_, amount)) = self
                .stored
                .iter_mut()
                .find(|(candidate, _)| same_payload(*candidate, *requirement))
            {
                *amount = (*amount - requirement.amount).max(0);
            }
        }
        self.stored.retain(|(_, amount)| *amount > 0);
    }

    /// `UnitAssemblerBuild.updateTile` progress half: advance when the plan's
    /// requirements are met and return the unit on completion.
    pub fn update(&mut self, time_scale: f32) -> Option<UnitTypeId> {
        let plan = self.plan()?;
        let (unit, time) = (plan.unit, plan.time.max(0.0001));
        if !self.has_requirements() {
            return None;
        }
        self.progress += time_scale / time;
        if self.progress < 1.0 {
            return None;
        }
        self.consume_requirements();
        self.progress = 0.0;
        Some(unit)
    }
}

/// `UnitAssemblerModule` configuration.
#[derive(Debug, Clone, Default, PartialEq, bevy_ecs::component::Component)]
pub struct UnitAssemblerModule {
    /// Module tier (`1..=4`).
    pub tier: i32,
    /// Owning assembler tile (`None` = unlinked).
    pub assembler: Option<i32>,
}

/// `RepairTower` configuration/state.
#[derive(Debug, Clone, PartialEq, bevy_ecs::component::Component)]
pub struct RepairTower {
    /// Heal range in world units.
    pub range: f32,
    /// Heal per tick.
    pub heal_amount: f32,
    /// Refresh countdown in ticks (`6`-tick upstream block interval).
    pub refresh_time: f32,
    /// Suppression countdown (`healSuppression`).
    pub suppression: f32,
}

impl Default for RepairTower {
    fn default() -> Self {
        Self {
            range: 80.0,
            heal_amount: 1.0,
            refresh_time: 6.0,
            suppression: 0.0,
        }
    }
}

impl RepairTower {
    /// `RepairTower.updateTile`: refresh the target list every 6 ticks.
    ///
    /// Returns `true` on a refresh tick.
    pub fn update_refresh(&mut self) -> bool {
        self.refresh_time -= 1.0;
        if self.refresh_time <= 0.0 {
            self.refresh_time = 6.0;
            true
        } else {
            false
        }
    }
}

/// `RepairTurret` configuration/state.
#[derive(Debug, Clone, PartialEq, bevy_ecs::component::Component)]
pub struct RepairTurret {
    /// Repair radius in world units (`repairRadius`).
    pub repair_radius: f32,
    /// Repair per second (`repairSpeed`).
    pub repair_speed: f32,
    /// Target acquisition interval (`60` ticks upstream).
    pub target_interval: f32,
}

impl Default for RepairTurret {
    fn default() -> Self {
        Self {
            repair_radius: 60.0,
            repair_speed: 0.5,
            target_interval: 60.0,
        }
    }
}

impl RepairTurret {
    /// `RepairPointBuild.updateTile`: acquire a target every 60 ticks.
    pub fn update_target(&mut self) -> bool {
        self.target_interval -= 1.0;
        if self.target_interval <= 0.0 {
            self.target_interval = 60.0;
            true
        } else {
            false
        }
    }
}

/// `UnitCargoLoader` configuration.
#[derive(Debug, Clone, PartialEq, bevy_ecs::component::Component)]
pub struct UnitCargoLoader {
    /// Spawned unit type (`manifold`).
    pub unit_type: UnitTypeId,
    /// Build time in ticks (`unitBuildTime`).
    pub unit_build_time: f32,
}

/// `UnitTransportSourceBuild` runtime state (`buildProgress`/`unit`).
#[derive(Debug, Clone, Default, PartialEq, bevy_ecs::component::Component)]
pub struct CargoLoaderState {
    /// Accumulated build progress (`buildProgress`).
    pub build_progress: f32,
    /// Currently tethered unit (`unit`; `None` = ready to build).
    pub unit: Option<bevy_ecs::entity::Entity>,
}

impl CargoLoaderState {
    /// `UnitTransportSourceBuild.updateTile` progress half: advance while no
    /// unit is tethered and return the spawned unit on completion.
    pub fn update(
        &mut self,
        time_scale: f32,
        unit_build_time: f32,
        unit_type: UnitTypeId,
        unit_alive: bool,
    ) -> Option<UnitTypeId> {
        if self.unit.is_some() {
            if !unit_alive {
                self.unit = None;
            } else {
                return None;
            }
        }
        self.build_progress += time_scale / unit_build_time.max(0.0001);
        if self.build_progress >= 1.0 {
            self.build_progress = 0.0;
            return Some(unit_type);
        }
        None
    }
}

/// `UnitCargoUnloadPoint` configuration/state.
#[derive(Debug, Clone, PartialEq, bevy_ecs::component::Component)]
pub struct UnitCargoUnloadPoint {
    /// Item configured for unloading.
    pub item: Option<ItemId>,
    /// Stale duration in ticks (`staleTimeDuration = 360`).
    pub stale_time_duration: f32,
    /// Remaining stale time.
    pub stale_time: f32,
}

impl Default for UnitCargoUnloadPoint {
    fn default() -> Self {
        Self {
            item: None,
            stale_time_duration: 360.0,
            stale_time: 0.0,
        }
    }
}

impl UnitCargoUnloadPoint {
    /// `UnitCargoUnloadPoint.updateTile`: decrement the stale timer while empty.
    ///
    /// Returns `true` while the point is stale (`staleTime <= 0`).
    pub fn update_stale(&mut self, has_items: bool) -> bool {
        if has_items {
            self.stale_time = self.stale_time_duration;
        } else {
            self.stale_time -= 1.0;
        }
        self.stale_time <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore};

    fn content() -> ContentRegistry {
        let mut registry = crate::content::create_base_content(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        registry
    }

    #[test]
    fn factory_progress_consumes_and_produces() {
        let content = content();
        let dagger = content.unit_by_name("dagger").expect("dagger").id;
        let copper = content.item_by_name("copper").expect("copper").id;
        let plan = UnitPlan::new(dagger, vec![(copper, 10)], 60.0);
        let factory = UnitFactory::new(vec![plan], content.items().len());
        assert_eq!(factory.capacity(copper), 20);
        assert!(factory.accepts(copper));

        let mut build = UnitFactoryBuild::new(1, factory.capacities.len());
        build.items[copper.index()] = 20;
        // Not enough time yet.
        assert_eq!(build.update(&factory, 30.0), None);
        assert_eq!(build.update(&factory, 30.0), Some(dagger));
        assert_eq!(build.items[copper.index()], 10, "requirements consumed");
    }

    #[test]
    fn reconstructor_upgrade_matrix() {
        let content = content();
        let dagger = content.unit_by_name("dagger").expect("dagger").id;
        let mace = content.unit_by_name("mace").expect("mace").id;
        let recon = Reconstructor::new(vec![(dagger, mace)], 120.0);
        assert_eq!(recon.can_upgrade(dagger), Some(mace));
        assert_eq!(recon.can_upgrade(mace), None);

        let mut build = ReconstructorBuild::default();
        assert!(
            !build.accept_payload(&recon, mace),
            "invalid upgrade rejected"
        );
        assert!(build.accept_payload(&recon, dagger));
        assert!(!build.accept_payload(&recon, dagger), "already occupied");
        assert_eq!(build.update(&recon, 60.0), None);
        assert_eq!(build.update(&recon, 60.0), Some(mace));
    }

    #[test]
    fn assembler_tiers_and_modules() {
        let content = content();
        let dagger = content.unit_by_name("dagger").expect("dagger").id;
        let plan = AssemblerUnitPlan {
            unit: dagger,
            payloads: Vec::new(),
            time: 60.0,
            drones: 4,
        };
        let mut assembler = UnitAssembler::new(vec![plan], 30.0);
        assert_eq!(assembler.side(), 5);
        assert!(assembler.accepts_module(1));
        assert!(!assembler.accepts_module(2));
        assert!(assembler.add_module(1));
        assert!(assembler.accepts_module(2));
        assert_eq!(assembler.module_tier, 1);
    }

    #[test]
    fn unload_point_stale_flips() {
        let mut point = UnitCargoUnloadPoint::default();
        assert!(!point.update_stale(true), "fresh when receiving items");
        for _ in 0..360 {
            point.update_stale(false);
        }
        assert!(point.update_stale(false), "stale after 360 empty ticks");
        point.update_stale(true);
        assert!(!point.update_stale(true), "reset by items");
    }

    #[test]
    fn repair_intervals_match_upstream() {
        let mut tower = RepairTower::default();
        for _ in 0..5 {
            assert!(!tower.update_refresh());
        }
        assert!(tower.update_refresh(), "tower refreshes every 6 ticks");
        let mut turret = RepairTurret::default();
        for _ in 0..59 {
            assert!(!turret.update_target());
        }
        assert!(turret.update_target(), "turret reacquires every 60 ticks");
    }

    #[test]
    fn unit_block_kind_names_unique() {
        let kinds = [
            UnitBlockKind::Factory,
            UnitBlockKind::Reconstructor,
            UnitBlockKind::Assembler,
            UnitBlockKind::AssemblerModule,
            UnitBlockKind::RepairTower,
            UnitBlockKind::RepairTurret,
            UnitBlockKind::CargoLoader,
            UnitBlockKind::CargoUnloadPoint,
            UnitBlockKind::UnitBlock,
        ];
        for (i, a) in kinds.iter().enumerate() {
            for b in &kinds[i + 1..] {
                assert_ne!(a.name(), b.name());
            }
        }
    }
}

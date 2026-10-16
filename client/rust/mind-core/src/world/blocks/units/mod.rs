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

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{ContentRef, ContentType, ItemId, PayloadSeq, UnitTypeId};
use crate::entities::comp::{Building, Health, Pos};
use crate::world::modules::ItemModule;

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

    /// Whether the plan's requirements are held (`ConsumeItemDynamic` trigger
    /// gate; `UnitFactory.java:84`).
    pub fn should_consume(&self, items: &ItemModule, plan_index: usize) -> bool {
        let Some(plan) = self.plans.get(plan_index) else {
            return false;
        };
        plan.requirements
            .iter()
            .all(|(item, amount)| items.get(*item) >= *amount)
    }
}

/// `UnitFactoryBuild` progress state (the mutable half).
#[derive(Debug, Clone, Default, PartialEq, bevy_ecs::component::Component)]
pub struct UnitFactoryBuild {
    /// Selected plan index (`-1` = none; Java `UnitFactoryBuild.currentPlan`).
    pub current_plan: i32,
    /// Accumulated build time.
    pub progress: f32,
    /// Queued unit's command override (`UnitFactoryBuild.command`,
    /// `UnitFactory.java:227`); `None` keeps the unit's `defaultCommand`
    /// (`UnitFactory.java:437`). Set by the command row (`:292-340`) and
    /// cleared by `configClear` (`:82`) or when a new plan's unit lacks it
    /// (`:64-66`, `:76-78`).
    pub command: Option<crate::content::UnitCommandId>,
}

impl UnitFactoryBuild {
    /// Creates an unconfigured build (`currentPlan = -1`, `progress = 0`).
    pub fn new() -> Self {
        Self {
            current_plan: -1,
            progress: 0.0,
            command: None,
        }
    }

    /// `UnitFactoryBuild.updateTile` progress half: advance progress by
    /// `delta_progress` (`edelta() * unitBuildSpeed`; the caller already applied
    /// `efficiency > 0`) and return the produced unit when the plan completes
    /// and its requirements are held in `items`.
    ///
    /// On completion `progress %= 1` (the caller consumes the requirements from
    /// `ItemModule`); with no plan or missing requirements `progress` is held
    /// clamped to `0..=plan.time` (`UnitFactory.java:406-450`).
    pub fn update(
        &mut self,
        factory: &UnitFactory,
        delta_progress: f32,
        items: &ItemModule,
    ) -> Option<UnitTypeId> {
        let Some(index) = usize::try_from(self.current_plan).ok() else {
            // `currentPlan == -1` -> the Java else branch resets progress.
            self.progress = 0.0;
            return None;
        };
        let Some(plan) = factory.plans.get(index) else {
            self.progress = 0.0;
            return None;
        };
        if delta_progress <= 0.0 || !factory.should_consume(items, index) {
            // Requirements not met (or idle): hold progress, clamped.
            self.progress = self.progress.clamp(0.0, plan.time.max(0.0));
            return None;
        }
        self.progress += delta_progress;
        if self.progress < plan.time {
            self.progress = self.progress.clamp(0.0, plan.time.max(0.0));
            return None;
        }
        self.progress %= 1.0;
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
    /// `Reconstructor.capacities`: per-item max, `2x` the `ConsumeItems` stack
    /// (`Reconstructor.initCapacities`).
    pub capacities: Vec<i32>,
}

impl Reconstructor {
    /// Creates a reconstructor.
    pub fn new(upgrades: Vec<(UnitTypeId, UnitTypeId)>, construct_time: f32) -> Self {
        Self {
            upgrades,
            construct_time,
            capacities: Vec::new(),
        }
    }

    /// `Reconstructor.capacities[item.id]` (`getMaximumAccepted` base value).
    pub fn capacity(&self, item: ItemId) -> i32 {
        self.capacities.get(item.index()).copied().unwrap_or(0)
    }

    /// Whether the configured `ConsumeItems` stacks include `item`
    /// (`Block.consumesItem`).
    pub fn accepts(&self, item: ItemId) -> bool {
        self.capacity(item) > 0
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
    /// Configured output command (`ReconstructorBuild.command`).
    pub command: Option<u16>,
    /// Payload travel to the centre, `0..1` (`PayloadBlock.moveInPayload`).
    ///
    /// The port approximates `payloadSpeed = 0.7` px/tick over ~14 px with
    /// `+edelta/20` per tick (about 20 ticks). Transient: not serialized.
    pub move_in: f32,
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
        self.move_in = 0.0;
        true
    }

    /// `ReconstructorBuild.updateTile`: advance the payload travel, then the
    /// upgrade; on completion returns the upgraded unit type and keeps
    /// `progress %= 1f` (Java does not reset).
    ///
    /// `progress` only counts once `move_in` reaches 1
    /// (`Reconstructor.updateTile` -> `moveInPayload()` gate).
    pub fn update(
        &mut self,
        reconstructor: &Reconstructor,
        delta_progress: f32,
        edelta: f32,
    ) -> Option<UnitTypeId> {
        let current = self.payload?;
        let target = reconstructor.can_upgrade(current)?;
        if self.move_in < 1.0 {
            self.move_in = (self.move_in + edelta / 20.0).min(1.0);
            return None;
        }
        self.progress += delta_progress;
        if self.progress < reconstructor.construct_time {
            return None;
        }
        self.payload = None;
        self.progress %= 1.0;
        Some(target)
    }

    /// `ReconstructorBuild.constructing()`: a payload with a valid upgrade.
    pub fn constructing(&self, reconstructor: &Reconstructor) -> bool {
        self.payload
            .is_some_and(|payload| reconstructor.can_upgrade(payload).is_some())
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
    /// Module tier registered by an adjacent module (`currentTier`; `0` = none).
    pub module_tier: i32,
    /// How many drones to keep alive (`dronesCreated`, Java default 4).
    pub drones_created: i32,
    /// Drone construct time.
    pub drone_construct_time: f32,
    /// Accumulated unit build progress (`progress`).
    pub progress: f32,
    /// Accumulated drone build progress (`droneProgress`; transient).
    pub drone_progress: f32,
    /// Tier seen last tick (`lastTier`; transient, Java starts at `-2`).
    pub last_tier: i32,
    /// Stored payloads (Java `blocks` `PayloadSeq`).
    pub stored: PayloadSeq,
    /// Live assembly drones (Java `units`).
    pub units: Vec<Entity>,
    /// Drones ids read from the save body (`readUnits`), resolved on update.
    pub read_unit_ids: Vec<i32>,
    /// Command position handed to produced units (`commandPos`).
    pub command_pos: Option<(f32, f32)>,
}

/// `UnitAssembler.areaSize` default.
pub const ASSEMBLER_AREA_SIZE: i32 = 11;

/// Content reference of one stored payload (`PayloadStack.item`).
pub fn payload_content_ref(payload: AssemblerPayload) -> ContentRef {
    ContentRef::new(
        if payload.is_block {
            ContentType::Block
        } else {
            ContentType::Unit
        },
        payload.content,
    )
}

impl UnitAssembler {
    /// Creates an assembler with the upstream 11×11 area.
    pub fn new(plans: Vec<AssemblerUnitPlan>, drone_construct_time: f32) -> Self {
        Self {
            area_size: ASSEMBLER_AREA_SIZE,
            plans,
            module_tier: 0,
            // `UnitAssembler.dronesCreated = 4`.
            drones_created: 4,
            drone_construct_time,
            progress: 0.0,
            drone_progress: 0.0,
            last_tier: -2,
            stored: PayloadSeq::default(),
            units: Vec::new(),
            read_unit_ids: Vec::new(),
            command_pos: None,
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

    /// Stored count for `payload` (`blocks.get`).
    pub fn stored(&self, payload: AssemblerPayload) -> i32 {
        self.stored.get(payload_content_ref(payload))
    }

    /// Whether the active plan accepts one more `payload`
    /// (`UnitAssemblerBuild.acceptPayload`; the plan is the module-tier one).
    pub fn accepts_payload(&self, payload: AssemblerPayload) -> bool {
        let Some(plan) = self.plan() else {
            return false;
        };
        let reference = payload_content_ref(payload);
        plan.payloads.iter().any(|requirement| {
            payload_content_ref(*requirement) == reference
                && self.stored(*requirement) < requirement.amount
        })
    }

    /// Stores one accepted payload (`moveInPayload` into `blocks`).
    pub fn store_payload(&mut self, payload: AssemblerPayload) -> bool {
        if !self.accepts_payload(payload) {
            return false;
        }
        self.stored.add(payload_content_ref(payload), 1);
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

    /// Consumes the active plan's stored payloads (`ConsumePayloadDynamic`).
    pub fn consume_requirements(&mut self) {
        let Some(plan) = self.plan() else {
            return;
        };
        let payloads = plan.payloads.clone();
        for requirement in &payloads {
            self.stored
                .remove(payload_content_ref(*requirement), requirement.amount);
        }
    }

    /// `UnitAssemblerBuild.updateTile` product half: advance with the `eff`
    /// (in-position drones) term and return the unit on completion.
    pub fn update(&mut self, time_scale: f32, eff: f32) -> Option<UnitTypeId> {
        let plan = self.plan()?;
        let (unit, time) = (plan.unit, plan.time.max(0.0001));
        if !self.has_requirements() {
            return None;
        }
        self.progress += time_scale * eff / time;
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
    /// Current target unit (`target`).
    pub target: Option<Entity>,
    /// Turret rotation in degrees (`rotation`; init 90).
    pub rotation: f32,
    /// `RepairTurretBuild.strength`.
    pub strength: f32,
    /// `acceptCoolant` (repair-turret only).
    pub accept_coolant: bool,
    /// `coolantMultiplier`.
    pub coolant_multiplier: f32,
}

impl Default for RepairTurret {
    fn default() -> Self {
        Self {
            repair_radius: 60.0,
            repair_speed: 0.5,
            target: None,
            rotation: 90.0,
            strength: 0.0,
            accept_coolant: false,
            coolant_multiplier: 0.0,
        }
    }
}

/// `RepairTurretBuild.updateTile` (`RepairTurret.java:186-223`, T12).
///
/// `Units.closest(team, x, y, repairRadius, Unit::damaged)` runs on the 20-tick
/// `timerTarget`; the target is dropped when dead/out of range/full; healing
/// happens while the aim is within 30 degrees and scales with `strength`,
/// `edelta` and the coolant multiplier (repair-turret).
pub fn update_repair_turret(world: &mut World, e: Entity, turret: &mut RepairTurret) {
    let (efficiency, optional_efficiency, time_scale) = world
        .get::<Building>(e)
        .map(|building| {
            (
                building.efficiency,
                building.optional_efficiency,
                building.time_scale,
            )
        })
        .unwrap_or((0.0, 0.0, 1.0));

    let multiplier = if turret.accept_coolant {
        // `RepairTurret.java:186-188`: `liquids.current().heatCapacity`
        // (with no coolant `optionalEfficiency` is 0, so the multiplier is 1).
        let heat_capacity = world
            .get::<crate::world::modules::LiquidModule>(e)
            .map(|module| module.current_liquid())
            .and_then(|liquid| {
                world
                    .get_resource::<crate::world::modules::LiquidTraits>()
                    .and_then(|traits| traits.heat_capacity.get(liquid.index()).copied())
            })
            .unwrap_or(0.0);
        1.0 + heat_capacity * turret.coolant_multiplier * optional_efficiency
    } else {
        1.0
    };

    // Drop a dead/out-of-range/full target.
    if let Some(target) = turret.target {
        let pos = world.get::<Pos>(e).copied();
        let target_pos = world.get::<Pos>(target).copied();
        let hit_size = world
            .get::<crate::entities::comp::unit::comp::HitboxComp>(target)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        let dead = world
            .get::<crate::entities::comp::unit::comp::UnitCore>(target)
            .is_some_and(|core| core.dead);
        let full = world
            .get::<Health>(target)
            .is_some_and(|health| health.health >= health.max_health);
        let dst = match (pos, target_pos) {
            (Some(a), Some(b)) => ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt(),
            _ => f32::MAX,
        };
        if dead || full || dst - hit_size / 2.0 > turret.repair_radius {
            turret.target = None;
        }
    }

    let mut healed = false;
    if let Some(target) = turret.target
        && efficiency > 0.0
    {
        let (Some(from), Some(to)) = (
            world.get::<Pos>(e).copied(),
            world.get::<Pos>(target).copied(),
        ) else {
            return;
        };
        let angle = (to.y - from.y).atan2(to.x - from.x).to_degrees();
        if crate::weapons::angle_dist(angle, turret.rotation) < 30.0 {
            healed = true;
            crate::combat::damage::area::heal_health(
                world,
                target,
                turret.repair_speed * turret.strength * efficiency * time_scale * multiplier,
            );
        }
        turret.rotation = slerp_delta(turret.rotation, angle, 0.5 * efficiency * time_scale);
    }

    turret.strength += ((if healed { 1.0 } else { 0.0 }) - turret.strength) * 0.08;

    if crate::world::update::run_timer(world, e, 0, 20.0) {
        let Some(pos) = world.get::<Pos>(e).copied() else {
            return;
        };
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|team| team.team);
        let mut best: Option<(f32, Entity)> = None;
        for entity_ref in world.iter_entities() {
            let Some(core) = entity_ref.get::<crate::entities::comp::unit::comp::UnitCore>() else {
                continue;
            };
            if core.dead {
                continue;
            }
            if entity_ref
                .get::<crate::entities::comp::TeamComp>()
                .map(|target_team| target_team.team)
                != team
            {
                continue;
            }
            let Some(health) = entity_ref.get::<Health>() else {
                continue;
            };
            if !health.damaged() {
                continue;
            }
            let Some(target_pos) = entity_ref.get::<Pos>() else {
                continue;
            };
            let hit_size = entity_ref
                .get::<crate::entities::comp::unit::comp::HitboxComp>()
                .map(|hitbox| hitbox.hit_size)
                .unwrap_or(0.0);
            let dst2 = (target_pos.x - pos.x).powi(2) + (target_pos.y - pos.y).powi(2);
            let reach = turret.repair_radius + hit_size / 2.0;
            if dst2 <= reach * reach && best.is_none_or(|(best_dst2, _)| dst2 < best_dst2) {
                best = Some((dst2, entity_ref.id()));
            }
        }
        turret.target = best.map(|(_, entity)| entity);
    }
}

/// `Mathf.slerpDelta` for degrees (`delta == 1`).
fn slerp_delta(from: f32, to: f32, progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    let delta = ((to - from + 360.0 + 180.0) % 360.0) - 180.0;
    (from + delta * progress + 360.0) % 360.0
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
#[derive(Debug, Clone, PartialEq, bevy_ecs::component::Component)]
pub struct CargoLoaderState {
    /// Accumulated build progress (`buildProgress`).
    pub build_progress: f32,
    /// Currently tethered unit (`unit`; `None` = ready to build).
    pub unit: Option<bevy_ecs::entity::Entity>,
    /// Unit id read from the save body, resolved on the next `updateTile`
    /// (`readUnitId`; `UnitCargoLoader.java:86,98-103`). `-1` = none.
    pub read_unit_id: i32,
}

impl Default for CargoLoaderState {
    fn default() -> Self {
        Self {
            build_progress: 0.0,
            unit: None,
            // Java initializes `readUnitId = -1` (`UnitCargoLoader.java:86`).
            read_unit_id: -1,
        }
    }
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
    /// Ticks the point has stayed full (`staleTimer`).
    pub stale_timer: f32,
    /// `stale`: the point was full for `staleTimeDuration` ticks, so cargo
    /// stops flying here.
    pub stale: bool,
}

impl Default for UnitCargoUnloadPoint {
    fn default() -> Self {
        Self {
            item: None,
            stale_time_duration: 360.0,
            stale_timer: 0.0,
            stale: false,
        }
    }
}

impl UnitCargoUnloadPoint {
    /// `UnitCargoUnloadPointBuild.updateTile` full/stale half
    /// (`UnitCargoUnloadPoint.java:60-73`): the point marks itself stale after
    /// `staleTimeDuration` full ticks; dropping below capacity resets it.
    /// Java's `dumpAccumulate()` reset is not modelled (the port's unload point
    /// has no dump path).
    ///
    /// Returns `true` while the point is stale.
    pub fn update_stale(&mut self, total: i32, capacity: i32, delta: f32) -> bool {
        if total < capacity {
            self.stale_timer = 0.0;
            self.stale = false;
        }
        if total >= capacity {
            self.stale_timer += delta;
            if self.stale_timer >= self.stale_time_duration {
                self.stale = true;
            }
        }
        self.stale
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

        let mut items = ItemModule::with_items(content.items().len());
        items.add(copper, 20, 20);
        let mut build = UnitFactoryBuild::new();
        build.current_plan = 0;
        // Not enough time yet; requirements gate the progress.
        assert_eq!(build.update(&factory, 30.0, &items), None);
        assert_eq!(build.update(&factory, 30.0, &items), Some(dagger));
        // The caller consumes the requirements (`UnitFactory.updateTile`).
        assert_eq!(items.remove(copper, 10), 10, "requirements consumed");
        assert_eq!(items.get(copper), 10);
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
        // `moveInPayload()`: the first tick only advances the payload travel.
        assert_eq!(build.update(&recon, 0.0, 100.0), None);
        assert_eq!(build.update(&recon, 60.0, 1.0), None);
        assert_eq!(build.update(&recon, 60.0, 1.0), Some(mace));
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
        assert!(!point.update_stale(0, 10, 1.0), "fresh while not full");
        for _ in 0..359 {
            assert!(!point.update_stale(10, 10, 1.0));
        }
        assert!(
            point.update_stale(10, 10, 1.0),
            "stale after 360 full ticks"
        );
        assert!(!point.update_stale(0, 10, 1.0), "reset below capacity");
    }

    #[test]
    fn repair_intervals_match_upstream() {
        let mut tower = RepairTower::default();
        for _ in 0..5 {
            assert!(!tower.update_refresh());
        }
        assert!(tower.update_refresh(), "tower refreshes every 6 ticks");
        // The repair turret's target timer is now the Java `timer(timerTarget,
        // 20)` inside `update_repair_turret` (T12); the default state keeps the
        // Java initial rotation.
        assert_eq!(RepairTurret::default().rotation, 90.0);
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

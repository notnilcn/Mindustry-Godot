// SPDX-License-Identifier: GPL-3.0-only

//! Spec helper types shared by the generated block waves (`BuildVisibility` through `BarSpec`).
//! Upstream: core/src/mindustry/world/meta/{BlockGroup,BlockFlag,Env}.java, core/src/mindustry/world/Block.java

use crate::content::id::{LiquidId, UnitTypeId};
use crate::content::registries::planets::EnvFlag;
use crate::content::stacks::{ItemStack, LiquidStack, PayloadStack};

/// `BuildVisibility` variant tag (plan 02 §6.1; no runtime predicates in metadata).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BuildVisibility {
    /// `BuildVisibility.hidden`.
    #[default]
    Hidden,
    /// `BuildVisibility.shown`.
    Shown,
    /// `BuildVisibility.debugOnly`.
    DebugOnly,
    /// `BuildVisibility.editorOnly`.
    EditorOnly,
    /// `BuildVisibility.coreZoneOnly`.
    CoreZoneOnly,
    /// `BuildVisibility.worldProcessorOnly`.
    WorldProcessorOnly,
    /// `BuildVisibility.sandboxOnly`.
    SandboxOnly,
    /// `BuildVisibility.campaignOnly`.
    CampaignOnly,
    /// `BuildVisibility.legacyLaunchPadOnly`.
    LegacyLaunchPadOnly,
    /// `BuildVisibility.notLegacyLaunchPadOnly`.
    NotLegacyLaunchPadOnly,
    /// `BuildVisibility.lightingOnly`.
    LightingOnly,
    /// `BuildVisibility.fogOnly`.
    FogOnly,
}

impl BuildVisibility {
    /// Java field name (audit/debug).
    pub const fn name(self) -> &'static str {
        match self {
            BuildVisibility::Hidden => "hidden",
            BuildVisibility::Shown => "shown",
            BuildVisibility::DebugOnly => "debugOnly",
            BuildVisibility::EditorOnly => "editorOnly",
            BuildVisibility::CoreZoneOnly => "coreZoneOnly",
            BuildVisibility::WorldProcessorOnly => "worldProcessorOnly",
            BuildVisibility::SandboxOnly => "sandboxOnly",
            BuildVisibility::CampaignOnly => "campaignOnly",
            BuildVisibility::LegacyLaunchPadOnly => "legacyLaunchPadOnly",
            BuildVisibility::NotLegacyLaunchPadOnly => "notLegacyLaunchPadOnly",
            BuildVisibility::LightingOnly => "lightingOnly",
            BuildVisibility::FogOnly => "fogOnly",
        }
    }
}

/// `BlockGroup` (`world/meta/BlockGroup.java`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlockGroup {
    /// `none`.
    #[default]
    None,
    /// `walls`.
    Walls,
    /// `projectors`.
    Projectors,
    /// `turrets`.
    Turrets,
    /// `transportation`.
    Transportation,
    /// `power`.
    Power,
    /// `liquids`.
    Liquids,
    /// `drills`.
    Drills,
    /// `units`.
    Units,
    /// `logic`.
    Logic,
    /// `payloads`.
    Payloads,
    /// `heat`.
    Heat,
}

impl BlockGroup {
    /// Java enum identifier.
    pub const fn name(self) -> &'static str {
        match self {
            BlockGroup::None => "none",
            BlockGroup::Walls => "walls",
            BlockGroup::Projectors => "projectors",
            BlockGroup::Turrets => "turrets",
            BlockGroup::Transportation => "transportation",
            BlockGroup::Power => "power",
            BlockGroup::Liquids => "liquids",
            BlockGroup::Drills => "drills",
            BlockGroup::Units => "units",
            BlockGroup::Logic => "logic",
            BlockGroup::Payloads => "payloads",
            BlockGroup::Heat => "heat",
        }
    }

    /// `BlockGroup.anyReplace` — two blocks in such a group replace each other.
    pub const fn any_replace(self) -> bool {
        matches!(
            self,
            BlockGroup::Walls
                | BlockGroup::Projectors
                | BlockGroup::Turrets
                | BlockGroup::Transportation
                | BlockGroup::Liquids
                | BlockGroup::Logic
                | BlockGroup::Payloads
                | BlockGroup::Heat
        )
    }
}

/// `BlockFlag` (`world/meta/BlockFlag.java`), declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockFlag {
    /// Enemy core; primary target for all units.
    Core,
    /// Vault/container/etc.
    Storage,
    /// Something that generates power.
    Generator,
    /// Any turret.
    Turret,
    /// A block that transforms resources.
    Factory,
    /// Repair point.
    Repair,
    /// Block that stored power for resupply.
    Battery,
    /// Any reactor block.
    Reactor,
    /// Blocks that extinguish fires.
    Extinguisher,
    /// Is a drill.
    Drill,
    /// Force projector block.
    Shield,
    /// Launch pad.
    LaunchPad,
    /// Unit cargo unload point.
    UnitCargoUnloadPoint,
    /// Unit assembler.
    UnitAssembler,
    /// Has a fog radius.
    HasFogRadius,
    /// Steam vent.
    SteamVent,
    /// Block repair (projectors).
    BlockRepair,
    /// Synced network entity.
    Synced,
}

impl BlockFlag {
    /// Java enum identifier.
    pub const fn name(self) -> &'static str {
        match self {
            BlockFlag::Core => "core",
            BlockFlag::Storage => "storage",
            BlockFlag::Generator => "generator",
            BlockFlag::Turret => "turret",
            BlockFlag::Factory => "factory",
            BlockFlag::Repair => "repair",
            BlockFlag::Battery => "battery",
            BlockFlag::Reactor => "reactor",
            BlockFlag::Extinguisher => "extinguisher",
            BlockFlag::Drill => "drill",
            BlockFlag::Shield => "shield",
            BlockFlag::LaunchPad => "launchPad",
            BlockFlag::UnitCargoUnloadPoint => "unitCargoUnloadPoint",
            BlockFlag::UnitAssembler => "unitAssembler",
            BlockFlag::HasFogRadius => "hasFogRadius",
            BlockFlag::SteamVent => "steamVent",
            BlockFlag::BlockRepair => "blockRepair",
            BlockFlag::Synced => "synced",
        }
    }
}

/// `Env.any`-capable environment mask (`Env` bits).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvMask {
    /// `Env.any` — matches every environment.
    pub any: bool,
    /// Explicit flags.
    pub flags: Vec<EnvFlag>,
}

impl EnvMask {
    /// Empty mask.
    pub fn none() -> Self {
        Self::default()
    }

    /// `Env.any`.
    pub fn any() -> Self {
        Self {
            any: true,
            flags: Vec::new(),
        }
    }

    /// Builds a mask from flags.
    pub fn of(flags: Vec<EnvFlag>) -> Self {
        Self { any: false, flags }
    }

    /// Whether the mask is empty.
    pub fn is_empty(&self) -> bool {
        !self.any && self.flags.is_empty()
    }

    /// Whether `flag` is present (any-mask includes everything).
    pub fn contains(&self, flag: EnvFlag) -> bool {
        self.any || self.flags.contains(&flag)
    }
}

/// Input stack (item name-based) used by generated block waves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackSpec {
    /// Item name (parity ABI).
    pub item: &'static str,
    /// Amount.
    pub amount: i32,
}

/// Builds an item stack spec.
pub const fn stack(item: &'static str, amount: i32) -> StackSpec {
    StackSpec { item, amount }
}

/// Generated unit-factory plan input (`new UnitPlan(unit, time, requirements)`).
#[derive(Debug, Clone, PartialEq)]
pub struct UnitPlanSpec {
    /// Produced unit name.
    pub unit: &'static str,
    /// Build time in ticks.
    pub time: f32,
    /// Item requirements.
    pub requirements: Vec<StackSpec>,
}

/// Builds a unit plan spec.
pub fn unit_plan(unit: &'static str, time: f32, requirements: Vec<StackSpec>) -> UnitPlanSpec {
    UnitPlanSpec {
        unit,
        time,
        requirements,
    }
}

/// Payload stack input (`PayloadStack.list(...)`; block or unit name).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PayloadStackSpec {
    /// Content name (parity ABI).
    pub name: &'static str,
    /// Whether the payload is a block (else a unit).
    pub block: bool,
    /// Amount.
    pub amount: i32,
}

/// Builds a payload stack spec for a block.
pub const fn payload_block(name: &'static str, amount: i32) -> PayloadStackSpec {
    PayloadStackSpec {
        name,
        block: true,
        amount,
    }
}

/// Builds a payload stack spec for a unit.
pub const fn payload_unit(name: &'static str, amount: i32) -> PayloadStackSpec {
    PayloadStackSpec {
        name,
        block: false,
        amount,
    }
}

/// Generated assembler plan input (`new AssemblerUnitPlan(unit, time, payloads)`).
#[derive(Debug, Clone, PartialEq)]
pub struct AssemblerUnitPlanSpec {
    /// Produced unit name.
    pub unit: &'static str,
    /// Build time in ticks.
    pub time: f32,
    /// Payload requirements.
    pub payloads: Vec<PayloadStackSpec>,
}

/// Builds an assembler plan spec.
pub fn assembler_plan(
    unit: &'static str,
    time: f32,
    payloads: Vec<PayloadStackSpec>,
) -> AssemblerUnitPlanSpec {
    AssemblerUnitPlanSpec {
        unit,
        time,
        payloads,
    }
}

/// `UnitFactory.UnitPlan` resolved to content ids.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitPlanDef {
    /// Produced unit.
    pub unit: UnitTypeId,
    /// Build time in ticks.
    pub time: f32,
    /// Item requirements.
    pub requirements: Vec<ItemStack>,
}

/// `UnitAssembler.AssemblerUnitPlan` resolved to content ids.
#[derive(Debug, Clone, PartialEq)]
pub struct AssemblerUnitPlanDef {
    /// Produced unit.
    pub unit: UnitTypeId,
    /// Build time in ticks.
    pub time: f32,
    /// Payload requirements.
    pub payloads: Vec<PayloadStack>,
}

/// Input liquid stack (liquid name-based) used by generated block waves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiquidStackSpec {
    /// Liquid name (parity ABI).
    pub liquid: &'static str,
    /// Amount per tick.
    pub amount: f32,
}

/// Builds a liquid stack spec.
pub const fn liquid_stack(liquid: &'static str, amount: f32) -> LiquidStackSpec {
    LiquidStackSpec { liquid, amount }
}

/// Consumer spec input (name-based) emitted by generated block waves.
///
/// Flags mirror `Consume` fields set through `.optional()`/`.ignore()` and the
/// class-level `update` flag (plan 02 §6.1).
#[derive(Debug, Clone, PartialEq)]
pub enum ConsumeDef {
    /// `consumeItem`/`consumeItems`.
    Items {
        /// Required stacks.
        stacks: Vec<StackSpec>,
        /// `ConsumeItems.optional`.
        optional: bool,
        /// `ConsumeItems.ignore`.
        ignore: bool,
        /// `Consume.update` (`.update(false)` skips the per-tick update pass).
        update: bool,
    },
    /// `consumeLiquid`.
    Liquid {
        /// Liquid name.
        liquid: &'static str,
        /// Amount per tick (60/s display unit).
        amount: f32,
        /// `ConsumeLiquid.optional`.
        optional: bool,
        /// `Consume.update` (`.update(false)` skips the per-tick update pass).
        update: bool,
    },
    /// `consumeLiquids`.
    Liquids {
        /// Required stacks.
        stacks: Vec<LiquidStackSpec>,
        /// `ConsumeLiquids.optional`.
        optional: bool,
        /// `Consume.update` (`.update(false)` skips the per-tick update pass).
        update: bool,
    },
    /// `consumePower`.
    Power {
        /// Power per tick.
        usage: f32,
        /// `consumePowerBuffered` capacity (>0 when buffered).
        buffered: f32,
        /// `Consume.update` (`.update(false)` skips the per-tick update pass).
        update: bool,
    },
    /// `consumeCoolant`.
    Coolant {
        /// Amount per tick.
        amount: f32,
        /// Whether liquid coolants are allowed.
        allow_liquid: bool,
        /// Whether gaseous coolants are allowed.
        allow_gas: bool,
        /// `ConsumeCoolant.optional`.
        optional: bool,
        /// `Consume.update` (`.update(false)` skips the per-tick update pass).
        update: bool,
    },
}

impl ConsumeDef {
    /// `Consume.update` (Java `.update(false)`): excludes this consumer from the
    /// per-tick `updateConsumption` pass while keeping it in the efficiency and
    /// optional partitions (plan 02 §6.1).
    pub fn update(mut self, on: bool) -> Self {
        match &mut self {
            ConsumeDef::Items { update, .. }
            | ConsumeDef::Liquid { update, .. }
            | ConsumeDef::Liquids { update, .. }
            | ConsumeDef::Power { update, .. }
            | ConsumeDef::Coolant { update, .. } => *update = on,
        }
        self
    }
}

/// Final consumer record resolved to IDs (plan 02 §6.1 `ConsumeSpec`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsumeSpec {
    /// Consumer payload.
    pub consume: Consume,
    /// `Consume.optional`.
    pub optional: bool,
    /// `Consume.update`.
    pub update: bool,
    /// `Consume.ignore` (consumer skipped by the update/optional partitions).
    pub ignore: bool,
}

/// Consumer payload with resolved content IDs.
#[derive(Debug, Clone, PartialEq)]
pub enum Consume {
    /// Required items.
    Items(Vec<ItemStack>),
    /// A single required/optional liquid.
    Liquid {
        /// Liquid reference.
        liquid: LiquidId,
        /// Amount per tick.
        amount: f32,
    },
    /// Multiple required/optional liquids.
    Liquids(Vec<LiquidStack>),
    /// Power consumer (`buffered > 0` mirrors `consumePowerBuffered`).
    Power {
        /// Power per tick.
        usage: f32,
        /// Buffered capacity.
        buffered: f32,
    },
    /// Coolant consumer.
    Coolant {
        /// Amount per tick.
        amount: f32,
        /// Liquid allowed.
        allow_liquid: bool,
        /// Gas allowed.
        allow_gas: bool,
    },
}

/// Stat display spec (`setStats` data; bodies in plans 14/16).
///
/// Plan 02 ports the data model; vanilla `setStats` closure bodies are display
/// logic and are owned by the UI plans (see the plan Changelog).
#[derive(Debug, Clone, PartialEq)]
pub enum StatSpec {
    /// Boolean stat.
    Bool {
        /// Stat name.
        name: &'static str,
        /// Value.
        value: bool,
    },
    /// Numeric stat.
    Number {
        /// Stat name.
        name: &'static str,
        /// Value.
        value: f32,
        /// Display unit.
        unit: &'static str,
    },
    /// Percent stat.
    Percent {
        /// Stat name.
        name: &'static str,
        /// Value (0..1).
        value: f32,
        /// Whether to divide by 60.
        per_second: bool,
    },
    /// Item stack list stat.
    Items {
        /// Stat name.
        name: &'static str,
        /// Stacks.
        stacks: Vec<ItemStack>,
    },
    /// Liquid stat.
    Liquid {
        /// Stat name.
        name: &'static str,
        /// Liquid.
        liquid: LiquidId,
        /// Amount (per second when `per_second`).
        amount: f32,
        /// Whether amount is per second.
        per_second: bool,
    },
}

/// Bar display spec (`setBars` data; renderers in plans 14/16).
#[derive(Debug, Clone, PartialEq)]
pub enum BarSpec {
    /// Health bar (`Block.setBars` default).
    Health,
    /// Power bar.
    Power,
    /// Item capacity bar.
    Items,
    /// Liquid capacity bar.
    Liquid,
    /// Heat bar.
    Heat,
    /// Shield bar.
    Shield,
    /// Custom bar with a bundle key.
    Custom {
        /// Bar key suffix (bundle `bar.<key>`).
        key: &'static str,
    },
}

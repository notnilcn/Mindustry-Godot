// SPDX-License-Identifier: GPL-3.0-only

//! Block metadata registry (plan 02 M3/M4).
//!
//! Ported from `core/src/mindustry/content/Blocks.java` (metadata half) and
//! `core/src/mindustry/world/Block.java` (`init`/`postInit`/`researchRequirements`
//! metadata derivations). Behavior (`update`, `acceptItem`, placement,
//! consumers execution, multiblocks) is owned by plan 07.
//!
//! ## Layout
//!
//! * [`BlockDef`] — the full metadata record; derived fields are filled by
//!   [`Content::init_self`] exactly like `Block.init()`.
//! * [`BlockSpec`] — the generated wave input (name-based references, class
//!   defaults from [`BlockSpec::for_kind`]).
//! * [`Blocks`] — the sim-facing name/id table (`Sim.content`); it is a thin
//!   consumer of the same generated waves so scenario block names stay valid.
//! * waves [`environment`], [`ore`], [`crafting`], [`defense`] (M3) and the
//!   B3–B6 modules (M4) — generated from `Blocks.java` in upstream order.

pub mod campaign;
pub mod crafting;
pub mod defense;
pub mod distribution;
pub mod environment;
pub mod legacy;
pub mod liquid;
pub mod logic;
pub mod ore;
pub mod payloads;
pub mod power;
pub mod production;
pub mod sandbox;
pub mod storage;
pub mod turrets;
pub mod units;
pub mod units_erekir;

use indexmap::IndexMap;

use super::super::bundle::BundleView;
use super::super::category::Category;
use super::super::color::Rgba;
use super::super::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use super::super::id::{BlockId, ItemId, LiquidId, PlanetId, UnitTypeId};
pub use super::super::registries::planets::EnvFlag;
use super::super::settings_store::UnlockStore;
use super::super::stacks::{ItemStack, LiquidStack, PayloadStack, round_to_i32};
use super::super::{ContentError, ContentRef, ContentType};
use super::ContentRegistry;
use crate::render::layer::{BuildingCacheLayer, CacheLayerId};

/// Base tile size constant (`Block.tilesize`).
pub const TILE_SIZE: f32 = 8.0;

/// `Block.unitCapModifier` base (upstream `Block` default is 0, but cores set 1+).
pub const BASE_UNIT_CAP_MODIFIER: i32 = 0;

/// Java class tag of a vanilla block (`BlockKind` content ABI; append-only).
///
/// Variant names are the exact upstream class identifiers; `name()` returns the
/// Java class name for audit/dump output (plan 02 §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum BlockKind {
    /// `mindustry.world.Block` (direct instantiations; fallback tag).
    #[default]
    Block = 0,
    /// `mindustry.world.blocks.environment.AirBlock`.
    AirBlock = 1,
    /// `mindustry.world.blocks.environment.SpawnBlock`.
    SpawnBlock = 2,
    /// `mindustry.world.blocks.environment.RemoveWall`.
    RemoveWall = 3,
    /// `mindustry.world.blocks.environment.RemoveOre`.
    RemoveOre = 4,
    /// `mindustry.world.blocks.environment.Cliff`.
    Cliff = 5,
    /// `mindustry.world.blocks.ConstructBlock`.
    ConstructBlock = 6,
    /// `mindustry.world.blocks.environment.Floor`.
    Floor = 7,
    /// `mindustry.world.blocks.environment.EmptyFloor`.
    EmptyFloor = 8,
    /// `mindustry.world.blocks.environment.OverlayFloor`.
    OverlayFloor = 9,
    /// `mindustry.world.blocks.environment.OreBlock`.
    OreBlock = 10,
    /// `mindustry.world.blocks.environment.StaticWall`.
    StaticWall = 11,
    /// `mindustry.world.blocks.environment.StaticProp`.
    StaticProp = 12,
    /// `mindustry.world.blocks.environment.StaticTree`.
    StaticTree = 13,
    /// `mindustry.world.blocks.environment.Prop`.
    Prop = 14,
    /// `mindustry.world.blocks.environment.TreeBlock`.
    TreeBlock = 15,
    /// `mindustry.world.blocks.environment.TallBlock`.
    TallBlock = 16,
    /// `mindustry.world.blocks.environment.SeaBush`.
    SeaBush = 17,
    /// `mindustry.world.blocks.environment.Seaweed`.
    Seaweed = 18,
    /// `mindustry.world.blocks.environment.SteamVent`.
    SteamVent = 19,
    /// `mindustry.world.blocks.environment.ShallowLiquid`.
    ShallowLiquid = 20,
    /// `mindustry.world.blocks.environment.CharacterOverlay`.
    CharacterOverlay = 21,
    /// `mindustry.world.blocks.environment.RuneOverlay`.
    RuneOverlay = 22,
    /// `mindustry.world.blocks.environment.ColoredFloor`.
    ColoredFloor = 23,
    /// `mindustry.world.blocks.environment.ColoredWall`.
    ColoredWall = 24,
    /// `mindustry.world.blocks.production.GenericCrafter`.
    GenericCrafter = 25,
    /// `mindustry.world.blocks.production.AttributeCrafter`.
    AttributeCrafter = 26,
    /// `mindustry.world.blocks.heat.HeatCrafter`.
    HeatCrafter = 27,
    /// `mindustry.world.blocks.production.Separator`.
    Separator = 28,
    /// `mindustry.world.blocks.production.Incinerator`.
    Incinerator = 29,
    /// `mindustry.world.blocks.production.ItemIncinerator`.
    ItemIncinerator = 30,
    /// `mindustry.world.blocks.heat.HeatProducer`.
    HeatProducer = 31,
    /// `mindustry.world.blocks.heat.HeatConductor`.
    HeatConductor = 32,
    /// `mindustry.world.blocks.defense.Wall`.
    Wall = 33,
    /// `mindustry.world.blocks.defense.ShieldWall`.
    ShieldWall = 34,
    /// `mindustry.world.blocks.defense.Door`.
    Door = 35,
    /// `mindustry.world.blocks.defense.AutoDoor`.
    AutoDoor = 36,
    /// `mindustry.world.blocks.defense.MendProjector`.
    MendProjector = 37,
    /// `mindustry.world.blocks.defense.OverdriveProjector`.
    OverdriveProjector = 38,
    /// `mindustry.world.blocks.defense.ForceProjector`.
    ForceProjector = 39,
    /// `mindustry.world.blocks.defense.BaseShield`.
    BaseShield = 40,
    /// `mindustry.world.blocks.defense.ShockMine`.
    ShockMine = 41,
    /// `mindustry.world.blocks.defense.Radar`.
    Radar = 42,
    /// `mindustry.world.blocks.defense.turrets.BaseTurret` subclasses
    /// (`BuildTurret`).
    BuildTurret = 43,
    /// `mindustry.world.blocks.defense.RegenProjector`.
    RegenProjector = 44,
    /// `mindustry.world.blocks.defense.ShockwaveTower`.
    ShockwaveTower = 45,
    /// `mindustry.world.blocks.defense.Thruster`.
    Thruster = 46,
    /// `mindustry.world.blocks.*` (Conveyor).
    Conveyor = 47,
    /// `mindustry.world.blocks.*` (StackConveyor).
    StackConveyor = 48,
    /// `mindustry.world.blocks.*` (ArmoredConveyor).
    ArmoredConveyor = 49,
    /// `mindustry.world.blocks.*` (Junction).
    Junction = 50,
    /// `mindustry.world.blocks.*` (BufferedItemBridge).
    BufferedItemBridge = 51,
    /// `mindustry.world.blocks.*` (ItemBridge).
    ItemBridge = 52,
    /// `mindustry.world.blocks.*` (Sorter).
    Sorter = 53,
    /// `mindustry.world.blocks.*` (Router).
    Router = 54,
    /// `mindustry.world.blocks.*` (OverflowGate).
    OverflowGate = 55,
    /// `mindustry.world.blocks.*` (Unloader).
    Unloader = 56,
    /// `mindustry.world.blocks.*` (MassDriver).
    MassDriver = 57,
    /// `mindustry.world.blocks.*` (Duct).
    Duct = 58,
    /// `mindustry.world.blocks.*` (DuctRouter).
    DuctRouter = 59,
    /// `mindustry.world.blocks.*` (OverflowDuct).
    OverflowDuct = 60,
    /// `mindustry.world.blocks.*` (DuctBridge).
    DuctBridge = 61,
    /// `mindustry.world.blocks.*` (DirectionalUnloader).
    DirectionalUnloader = 62,
    /// `mindustry.world.blocks.*` (StackRouter).
    StackRouter = 63,
    /// `mindustry.world.blocks.*` (UnitCargoLoader).
    UnitCargoLoader = 64,
    /// `mindustry.world.blocks.*` (UnitCargoUnloadPoint).
    UnitCargoUnloadPoint = 65,
    /// `mindustry.world.blocks.*` (Pump).
    Pump = 66,
    /// `mindustry.world.blocks.*` (Conduit).
    Conduit = 67,
    /// `mindustry.world.blocks.*` (ArmoredConduit).
    ArmoredConduit = 68,
    /// `mindustry.world.blocks.*` (LiquidRouter).
    LiquidRouter = 69,
    /// `mindustry.world.blocks.*` (LiquidJunction).
    LiquidJunction = 70,
    /// `mindustry.world.blocks.*` (LiquidBridge).
    LiquidBridge = 71,
    /// `mindustry.world.blocks.*` (DirectionLiquidBridge).
    DirectionLiquidBridge = 72,
    /// `mindustry.world.blocks.*` (PowerNode).
    PowerNode = 73,
    /// `mindustry.world.blocks.*` (PowerDiode).
    PowerDiode = 74,
    /// `mindustry.world.blocks.*` (Battery).
    Battery = 75,
    /// `mindustry.world.blocks.*` (ConsumeGenerator).
    ConsumeGenerator = 76,
    /// `mindustry.world.blocks.*` (ThermalGenerator).
    ThermalGenerator = 77,
    /// `mindustry.world.blocks.*` (SolarGenerator).
    SolarGenerator = 78,
    /// `mindustry.world.blocks.*` (NuclearReactor).
    NuclearReactor = 79,
    /// `mindustry.world.blocks.*` (ImpactReactor).
    ImpactReactor = 80,
    /// `mindustry.world.blocks.*` (BeamNode).
    BeamNode = 81,
    /// `mindustry.world.blocks.*` (LongPowerNode).
    LongPowerNode = 82,
    /// `mindustry.world.blocks.*` (VariableReactor).
    VariableReactor = 83,
    /// `mindustry.world.blocks.*` (HeaterGenerator).
    HeaterGenerator = 84,
    /// `mindustry.world.blocks.*` (Drill).
    Drill = 85,
    /// `mindustry.world.blocks.*` (SolidPump).
    SolidPump = 86,
    /// `mindustry.world.blocks.*` (Fracker).
    Fracker = 87,
    /// `mindustry.world.blocks.*` (WallCrafter).
    WallCrafter = 88,
    /// `mindustry.world.blocks.*` (BeamDrill).
    BeamDrill = 89,
    /// `mindustry.world.blocks.*` (BurstDrill).
    BurstDrill = 90,
    /// `mindustry.world.blocks.*` (CoreBlock).
    CoreBlock = 91,
    /// `mindustry.world.blocks.*` (StorageBlock).
    StorageBlock = 92,
    /// `mindustry.world.blocks.*` (ItemTurret).
    ItemTurret = 93,
    /// `mindustry.world.blocks.*` (LiquidTurret).
    LiquidTurret = 94,
    /// `mindustry.world.blocks.*` (PowerTurret).
    PowerTurret = 95,
    /// `mindustry.world.blocks.*` (TractorBeamTurret).
    TractorBeamTurret = 96,
    /// `mindustry.world.blocks.*` (PointDefenseTurret).
    PointDefenseTurret = 97,
    /// `mindustry.world.blocks.*` (LaserTurret).
    LaserTurret = 98,
    /// `mindustry.world.blocks.*` (ContinuousLiquidTurret).
    ContinuousLiquidTurret = 99,
    /// `mindustry.world.blocks.*` (ContinuousTurret).
    ContinuousTurret = 100,
    /// `mindustry.world.blocks.*` (UnitFactory).
    UnitFactory = 101,
    /// `mindustry.world.blocks.*` (Reconstructor).
    Reconstructor = 102,
    /// `mindustry.world.blocks.*` (RepairTurret).
    RepairTurret = 103,
    /// `mindustry.world.blocks.*` (UnitAssembler).
    UnitAssembler = 104,
    /// `mindustry.world.blocks.*` (UnitAssemblerModule).
    UnitAssemblerModule = 105,
    /// `mindustry.world.blocks.*` (RepairTower).
    RepairTower = 106,
    /// `mindustry.world.blocks.*` (PayloadConveyor).
    PayloadConveyor = 107,
    /// `mindustry.world.blocks.*` (PayloadRouter).
    PayloadRouter = 108,
    /// `mindustry.world.blocks.*` (PayloadMassDriver).
    PayloadMassDriver = 109,
    /// `mindustry.world.blocks.*` (PayloadDeconstructor).
    PayloadDeconstructor = 110,
    /// `mindustry.world.blocks.*` (Constructor).
    Constructor = 111,
    /// `mindustry.world.blocks.*` (PayloadLoader).
    PayloadLoader = 112,
    /// `mindustry.world.blocks.*` (PayloadUnloader).
    PayloadUnloader = 113,
    /// `mindustry.world.blocks.*` (PowerSource).
    PowerSource = 114,
    /// `mindustry.world.blocks.*` (PowerVoid).
    PowerVoid = 115,
    /// `mindustry.world.blocks.*` (ItemSource).
    ItemSource = 116,
    /// `mindustry.world.blocks.*` (ItemVoid).
    ItemVoid = 117,
    /// `mindustry.world.blocks.*` (LiquidSource).
    LiquidSource = 118,
    /// `mindustry.world.blocks.*` (LiquidVoid).
    LiquidVoid = 119,
    /// `mindustry.world.blocks.*` (PayloadSource).
    PayloadSource = 120,
    /// `mindustry.world.blocks.*` (PayloadVoid).
    PayloadVoid = 121,
    /// `mindustry.world.blocks.*` (TargetDummy).
    TargetDummy = 122,
    /// `mindustry.world.blocks.*` (LightBlock).
    LightBlock = 123,
    /// `mindustry.world.blocks.*` (LaunchPad).
    LaunchPad = 124,
    /// `mindustry.world.blocks.*` (LandingPad).
    LandingPad = 125,
    /// `mindustry.world.blocks.*` (Accelerator).
    Accelerator = 126,
    /// `mindustry.world.blocks.*` (MessageBlock).
    MessageBlock = 127,
    /// `mindustry.world.blocks.*` (SwitchBlock).
    SwitchBlock = 128,
    /// `mindustry.world.blocks.*` (LogicBlock).
    LogicBlock = 129,
    /// `mindustry.world.blocks.*` (MemoryBlock).
    MemoryBlock = 130,
    /// `mindustry.world.blocks.*` (LogicDisplay).
    LogicDisplay = 131,
    /// `mindustry.world.blocks.*` (TileableLogicDisplay).
    TileableLogicDisplay = 132,
    /// `mindustry.world.blocks.*` (CanvasBlock).
    CanvasBlock = 133,
    /// `mindustry.world.blocks.campaign.LegacyMechPad` (content name `legacy-mech-pad`).
    LegacyMechPad = 134,
    /// `mindustry.world.blocks.units.LegacyUnitFactory` (3 legacy variants).
    LegacyUnitFactory = 135,
    /// `mindustry.world.blocks.campaign.LegacyCommandCenter` (`command-center`).
    LegacyCommandCenter = 136,
}

impl BlockKind {
    /// All variants in declaration order.
    pub const ALL: &'static [BlockKind] = &[
        BlockKind::Block,
        BlockKind::AirBlock,
        BlockKind::SpawnBlock,
        BlockKind::RemoveWall,
        BlockKind::RemoveOre,
        BlockKind::Cliff,
        BlockKind::ConstructBlock,
        BlockKind::Floor,
        BlockKind::EmptyFloor,
        BlockKind::OverlayFloor,
        BlockKind::OreBlock,
        BlockKind::StaticWall,
        BlockKind::StaticProp,
        BlockKind::StaticTree,
        BlockKind::Prop,
        BlockKind::TreeBlock,
        BlockKind::TallBlock,
        BlockKind::SeaBush,
        BlockKind::Seaweed,
        BlockKind::SteamVent,
        BlockKind::ShallowLiquid,
        BlockKind::CharacterOverlay,
        BlockKind::RuneOverlay,
        BlockKind::ColoredFloor,
        BlockKind::ColoredWall,
        BlockKind::GenericCrafter,
        BlockKind::AttributeCrafter,
        BlockKind::HeatCrafter,
        BlockKind::Separator,
        BlockKind::Incinerator,
        BlockKind::ItemIncinerator,
        BlockKind::HeatProducer,
        BlockKind::HeatConductor,
        BlockKind::Wall,
        BlockKind::ShieldWall,
        BlockKind::Door,
        BlockKind::AutoDoor,
        BlockKind::MendProjector,
        BlockKind::OverdriveProjector,
        BlockKind::ForceProjector,
        BlockKind::BaseShield,
        BlockKind::ShockMine,
        BlockKind::Radar,
        BlockKind::BuildTurret,
        BlockKind::RegenProjector,
        BlockKind::ShockwaveTower,
        BlockKind::Thruster,
        BlockKind::Conveyor,
        BlockKind::StackConveyor,
        BlockKind::ArmoredConveyor,
        BlockKind::Junction,
        BlockKind::BufferedItemBridge,
        BlockKind::ItemBridge,
        BlockKind::Sorter,
        BlockKind::Router,
        BlockKind::OverflowGate,
        BlockKind::Unloader,
        BlockKind::MassDriver,
        BlockKind::Duct,
        BlockKind::DuctRouter,
        BlockKind::OverflowDuct,
        BlockKind::DuctBridge,
        BlockKind::DirectionalUnloader,
        BlockKind::StackRouter,
        BlockKind::UnitCargoLoader,
        BlockKind::UnitCargoUnloadPoint,
        BlockKind::Pump,
        BlockKind::Conduit,
        BlockKind::ArmoredConduit,
        BlockKind::LiquidRouter,
        BlockKind::LiquidJunction,
        BlockKind::LiquidBridge,
        BlockKind::DirectionLiquidBridge,
        BlockKind::PowerNode,
        BlockKind::PowerDiode,
        BlockKind::Battery,
        BlockKind::ConsumeGenerator,
        BlockKind::ThermalGenerator,
        BlockKind::SolarGenerator,
        BlockKind::NuclearReactor,
        BlockKind::ImpactReactor,
        BlockKind::BeamNode,
        BlockKind::LongPowerNode,
        BlockKind::VariableReactor,
        BlockKind::HeaterGenerator,
        BlockKind::Drill,
        BlockKind::SolidPump,
        BlockKind::Fracker,
        BlockKind::WallCrafter,
        BlockKind::BeamDrill,
        BlockKind::BurstDrill,
        BlockKind::CoreBlock,
        BlockKind::StorageBlock,
        BlockKind::ItemTurret,
        BlockKind::LiquidTurret,
        BlockKind::PowerTurret,
        BlockKind::TractorBeamTurret,
        BlockKind::PointDefenseTurret,
        BlockKind::LaserTurret,
        BlockKind::ContinuousLiquidTurret,
        BlockKind::ContinuousTurret,
        BlockKind::UnitFactory,
        BlockKind::Reconstructor,
        BlockKind::RepairTurret,
        BlockKind::UnitAssembler,
        BlockKind::UnitAssemblerModule,
        BlockKind::RepairTower,
        BlockKind::PayloadConveyor,
        BlockKind::PayloadRouter,
        BlockKind::PayloadMassDriver,
        BlockKind::PayloadDeconstructor,
        BlockKind::Constructor,
        BlockKind::PayloadLoader,
        BlockKind::PayloadUnloader,
        BlockKind::PowerSource,
        BlockKind::PowerVoid,
        BlockKind::ItemSource,
        BlockKind::ItemVoid,
        BlockKind::LiquidSource,
        BlockKind::LiquidVoid,
        BlockKind::PayloadSource,
        BlockKind::PayloadVoid,
        BlockKind::TargetDummy,
        BlockKind::LightBlock,
        BlockKind::LaunchPad,
        BlockKind::LandingPad,
        BlockKind::Accelerator,
        BlockKind::MessageBlock,
        BlockKind::SwitchBlock,
        BlockKind::LogicBlock,
        BlockKind::MemoryBlock,
        BlockKind::LogicDisplay,
        BlockKind::TileableLogicDisplay,
        BlockKind::CanvasBlock,
        BlockKind::LegacyMechPad,
        BlockKind::LegacyUnitFactory,
        BlockKind::LegacyCommandCenter,
    ];

    /// Stable ordinal (content/audit ABI).
    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// Java class name for dumps/ledgers (`BlockKind` content ABI).
    pub const fn name(self) -> &'static str {
        match self {
            BlockKind::Block => "Block",
            BlockKind::AirBlock => "AirBlock",
            BlockKind::SpawnBlock => "SpawnBlock",
            BlockKind::RemoveWall => "RemoveWall",
            BlockKind::RemoveOre => "RemoveOre",
            BlockKind::Cliff => "Cliff",
            BlockKind::ConstructBlock => "ConstructBlock",
            BlockKind::Floor => "Floor",
            BlockKind::EmptyFloor => "EmptyFloor",
            BlockKind::OverlayFloor => "OverlayFloor",
            BlockKind::OreBlock => "OreBlock",
            BlockKind::StaticWall => "StaticWall",
            BlockKind::StaticProp => "StaticProp",
            BlockKind::StaticTree => "StaticTree",
            BlockKind::Prop => "Prop",
            BlockKind::TreeBlock => "TreeBlock",
            BlockKind::TallBlock => "TallBlock",
            BlockKind::SeaBush => "SeaBush",
            BlockKind::Seaweed => "Seaweed",
            BlockKind::SteamVent => "SteamVent",
            BlockKind::ShallowLiquid => "ShallowLiquid",
            BlockKind::CharacterOverlay => "CharacterOverlay",
            BlockKind::RuneOverlay => "RuneOverlay",
            BlockKind::ColoredFloor => "ColoredFloor",
            BlockKind::ColoredWall => "ColoredWall",
            BlockKind::GenericCrafter => "GenericCrafter",
            BlockKind::AttributeCrafter => "AttributeCrafter",
            BlockKind::HeatCrafter => "HeatCrafter",
            BlockKind::Separator => "Separator",
            BlockKind::Incinerator => "Incinerator",
            BlockKind::ItemIncinerator => "ItemIncinerator",
            BlockKind::HeatProducer => "HeatProducer",
            BlockKind::HeatConductor => "HeatConductor",
            BlockKind::Wall => "Wall",
            BlockKind::ShieldWall => "ShieldWall",
            BlockKind::Door => "Door",
            BlockKind::AutoDoor => "AutoDoor",
            BlockKind::MendProjector => "MendProjector",
            BlockKind::OverdriveProjector => "OverdriveProjector",
            BlockKind::ForceProjector => "ForceProjector",
            BlockKind::BaseShield => "BaseShield",
            BlockKind::ShockMine => "ShockMine",
            BlockKind::Radar => "Radar",
            BlockKind::BuildTurret => "BuildTurret",
            BlockKind::RegenProjector => "RegenProjector",
            BlockKind::ShockwaveTower => "ShockwaveTower",
            BlockKind::Thruster => "Thruster",
            BlockKind::Conveyor => "Conveyor",
            BlockKind::StackConveyor => "StackConveyor",
            BlockKind::ArmoredConveyor => "ArmoredConveyor",
            BlockKind::Junction => "Junction",
            BlockKind::BufferedItemBridge => "BufferedItemBridge",
            BlockKind::ItemBridge => "ItemBridge",
            BlockKind::Sorter => "Sorter",
            BlockKind::Router => "Router",
            BlockKind::OverflowGate => "OverflowGate",
            BlockKind::Unloader => "Unloader",
            BlockKind::MassDriver => "MassDriver",
            BlockKind::Duct => "Duct",
            BlockKind::DuctRouter => "DuctRouter",
            BlockKind::OverflowDuct => "OverflowDuct",
            BlockKind::DuctBridge => "DuctBridge",
            BlockKind::DirectionalUnloader => "DirectionalUnloader",
            BlockKind::StackRouter => "StackRouter",
            BlockKind::UnitCargoLoader => "UnitCargoLoader",
            BlockKind::UnitCargoUnloadPoint => "UnitCargoUnloadPoint",
            BlockKind::Pump => "Pump",
            BlockKind::Conduit => "Conduit",
            BlockKind::ArmoredConduit => "ArmoredConduit",
            BlockKind::LiquidRouter => "LiquidRouter",
            BlockKind::LiquidJunction => "LiquidJunction",
            BlockKind::LiquidBridge => "LiquidBridge",
            BlockKind::DirectionLiquidBridge => "DirectionLiquidBridge",
            BlockKind::PowerNode => "PowerNode",
            BlockKind::PowerDiode => "PowerDiode",
            BlockKind::Battery => "Battery",
            BlockKind::ConsumeGenerator => "ConsumeGenerator",
            BlockKind::ThermalGenerator => "ThermalGenerator",
            BlockKind::SolarGenerator => "SolarGenerator",
            BlockKind::NuclearReactor => "NuclearReactor",
            BlockKind::ImpactReactor => "ImpactReactor",
            BlockKind::BeamNode => "BeamNode",
            BlockKind::LongPowerNode => "LongPowerNode",
            BlockKind::VariableReactor => "VariableReactor",
            BlockKind::HeaterGenerator => "HeaterGenerator",
            BlockKind::Drill => "Drill",
            BlockKind::SolidPump => "SolidPump",
            BlockKind::Fracker => "Fracker",
            BlockKind::WallCrafter => "WallCrafter",
            BlockKind::BeamDrill => "BeamDrill",
            BlockKind::BurstDrill => "BurstDrill",
            BlockKind::CoreBlock => "CoreBlock",
            BlockKind::StorageBlock => "StorageBlock",
            BlockKind::ItemTurret => "ItemTurret",
            BlockKind::LiquidTurret => "LiquidTurret",
            BlockKind::PowerTurret => "PowerTurret",
            BlockKind::TractorBeamTurret => "TractorBeamTurret",
            BlockKind::PointDefenseTurret => "PointDefenseTurret",
            BlockKind::LaserTurret => "LaserTurret",
            BlockKind::ContinuousLiquidTurret => "ContinuousLiquidTurret",
            BlockKind::ContinuousTurret => "ContinuousTurret",
            BlockKind::UnitFactory => "UnitFactory",
            BlockKind::Reconstructor => "Reconstructor",
            BlockKind::RepairTurret => "RepairTurret",
            BlockKind::UnitAssembler => "UnitAssembler",
            BlockKind::UnitAssemblerModule => "UnitAssemblerModule",
            BlockKind::RepairTower => "RepairTower",
            BlockKind::PayloadConveyor => "PayloadConveyor",
            BlockKind::PayloadRouter => "PayloadRouter",
            BlockKind::PayloadMassDriver => "PayloadMassDriver",
            BlockKind::PayloadDeconstructor => "PayloadDeconstructor",
            BlockKind::Constructor => "Constructor",
            BlockKind::PayloadLoader => "PayloadLoader",
            BlockKind::PayloadUnloader => "PayloadUnloader",
            BlockKind::PowerSource => "PowerSource",
            BlockKind::PowerVoid => "PowerVoid",
            BlockKind::ItemSource => "ItemSource",
            BlockKind::ItemVoid => "ItemVoid",
            BlockKind::LiquidSource => "LiquidSource",
            BlockKind::LiquidVoid => "LiquidVoid",
            BlockKind::PayloadSource => "PayloadSource",
            BlockKind::PayloadVoid => "PayloadVoid",
            BlockKind::TargetDummy => "TargetDummy",
            BlockKind::LightBlock => "LightBlock",
            BlockKind::LaunchPad => "LaunchPad",
            BlockKind::LandingPad => "LandingPad",
            BlockKind::Accelerator => "Accelerator",
            BlockKind::MessageBlock => "MessageBlock",
            BlockKind::SwitchBlock => "SwitchBlock",
            BlockKind::LogicBlock => "LogicBlock",
            BlockKind::MemoryBlock => "MemoryBlock",
            BlockKind::LogicDisplay => "LogicDisplay",
            BlockKind::TileableLogicDisplay => "TileableLogicDisplay",
            BlockKind::CanvasBlock => "CanvasBlock",
            BlockKind::LegacyMechPad => "LegacyMechPad",
            BlockKind::LegacyUnitFactory => "LegacyUnitFactory",
            BlockKind::LegacyCommandCenter => "LegacyCommandCenter",
        }
    }
}

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

/// Generated block input record (name-based references).
///
/// Fields default to the Java `Block` base defaults; per-class defaults come
/// from [`BlockSpec::for_kind`]. `None`/empty means "class default".
#[derive(Debug, Clone)]
pub struct BlockSpec {
    /// Content name (parity ABI).
    pub name: &'static str,
    /// Java class tag.
    pub kind: BlockKind,
    /// Multiblock size in tiles.
    pub size: Option<i32>,
    /// Explicit health (`-1` = derive from size + item health scaling).
    pub health: Option<i32>,
    /// Health per tile (`<0` = derive to 40 scaled by item health scaling).
    pub scaled_health: Option<f32>,
    /// Damage absorption.
    pub armor: Option<f32>,
    /// Place category (set by `requirements(...)`).
    pub category: Option<Category>,
    /// Build requirements.
    pub requirements: Vec<StackSpec>,
    /// Explicit research cost.
    pub research_cost: Option<Vec<StackSpec>>,
    /// Research cost multiplier.
    pub research_cost_multiplier: Option<f32>,
    /// Per-item research cost multipliers (item names).
    pub research_cost_multipliers: Vec<(&'static str, f32)>,
    /// Build time multiplier.
    pub build_cost_multiplier: Option<f32>,
    /// Explicit build time (`<0` = derived from requirements).
    pub build_time: Option<f32>,
    /// Replace group.
    pub group: Option<BlockGroup>,
    /// Targeting priority (`TargetPriority` constant).
    pub priority: Option<f32>,
    /// Unit cap contribution.
    pub unit_cap_modifier: Option<i32>,
    /// Special flags (effective: class defaults + body overrides).
    pub flags: Vec<BlockFlag>,
    /// Consumers.
    pub consumes: Vec<ConsumeDef>,
    /// Unit factory plans (`UnitFactory.plans`).
    pub unit_plans: Vec<UnitPlanSpec>,
    /// Reconstructor upgrade pairs (`Reconstructor.upgrades`).
    pub upgrades: Vec<(&'static str, &'static str)>,
    /// Assembler plans (`UnitAssembler.plans`).
    pub assembler_plans: Vec<AssemblerUnitPlanSpec>,
    /// Item capacity.
    pub item_capacity: Option<i32>,
    /// Liquid capacity.
    pub liquid_capacity: Option<f32>,
    /// Has an item module.
    pub has_items: Option<bool>,
    /// Has a liquid module.
    pub has_liquids: Option<bool>,
    /// Has a power module.
    pub has_power: Option<bool>,
    /// Outputs power.
    pub outputs_power: Option<bool>,
    /// Consumes power (base true; nodes may set false).
    pub consumes_power: Option<bool>,
    /// Conducts power like a cable.
    pub conductive_power: Option<bool>,
    /// Outputs liquids somewhere.
    pub outputs_liquid: Option<bool>,
    /// Build-menu visibility.
    pub build_visibility: Option<BuildVisibility>,
    /// Required environment.
    pub env_required: Option<EnvMask>,
    /// Enabled environments.
    pub env_enabled: Option<EnvMask>,
    /// Disabled environments.
    pub env_disabled: Option<EnvMask>,
    /// Solid.
    pub solid: Option<bool>,
    /// Can float on liquids.
    pub floating: Option<bool>,
    /// Updates each tick.
    pub update: Option<bool>,
    /// Destructible.
    pub destructible: Option<bool>,
    /// Saves tile data.
    pub save_data: Option<bool>,
    /// Saves placement config.
    pub save_config: Option<bool>,
    /// Configurable.
    pub configurable: Option<bool>,
    /// Visible in the editor.
    pub in_editor: Option<bool>,
    /// Player-placeable.
    pub placeable_player: Option<bool>,
    /// Placeable on liquids.
    pub placeable_liquid: Option<bool>,
    /// Floors can be placed on this.
    pub placeable_on: Option<bool>,
    /// Insulated.
    pub insulated: Option<bool>,
    /// Absorbs lasers.
    pub absorb_lasers: Option<bool>,
    /// Core zone placement allowed.
    pub allow_core_placement: Option<bool>,
    /// Cannot be mined by players.
    pub player_unmineable: Option<bool>,
    /// Wall ore block.
    pub wall_ore: Option<bool>,
    /// Dropped item (drills/ore).
    pub item_drop: Option<&'static str>,
    /// Default ore flag.
    pub ore_default: Option<bool>,
    /// Ore threshold.
    pub ore_threshold: Option<f32>,
    /// Ore scale.
    pub ore_scale: Option<f32>,
    /// Fog radius (`-1` = none).
    pub fog_radius: Option<i32>,
    /// Region override.
    pub region: Option<&'static str>,
    /// Disable icon generation.
    pub generate_icons: Option<bool>,
    /// Map color.
    pub map_color: Option<Rgba>,
    /// Use this block's color in the minimap.
    pub has_color: Option<bool>,
    /// Square sprite.
    pub square_sprite: Option<bool>,
    /// `Block.cacheLayer` (default `normal`; walls set `walls`, liquids their layer).
    pub cache_layer: Option<CacheLayerId>,
    /// `Block.drawCached` (`drawCached()` is baked into the building cache).
    pub draw_cached: Option<bool>,
    /// `Block.drawDynamic` (`draw()` runs every frame).
    pub draw_dynamic: Option<bool>,
    /// `Block.buildingCacheLayer` (`BuildingCacheLayer.under`/`normal`).
    pub building_cache_layer: Option<f32>,
    /// `Block.fillsTile` (floor is hidden under this block; darkness owner).
    pub fills_tile: Option<bool>,
    /// `Block.hasShadow`.
    pub has_shadow: Option<bool>,
    /// `Block.customShadow` (implies `hasShadow = false` unless re-enabled).
    pub custom_shadow: Option<bool>,
    /// `Block.obstructsLight` (excluded from light quads; `drawNonLayer`).
    pub obstructs_light: Option<bool>,
    /// `Block.emitLight` (`drawLight()` is called).
    pub emit_light: Option<bool>,
    /// `Block.drawTeamOverlay` (team corner is drawn).
    pub draw_team_overlay: Option<bool>,
    /// `Floor.liquidDrop` (the liquid this floor yields / reacts with; plan 10
    /// §3.10 needs it for `Tile.getFlammability` and floor-drop reactions).
    /// Append-only addendum: plan 02 originally omitted the field.
    pub liquid_drop: Option<&'static str>,
    /// `Floor.speedMultiplier` (unit velocity multiplier; plan 11 §3.6).
    pub speed_multiplier: Option<f32>,
    /// `Floor.isLiquid` (naval solidity/`onLiquid`; plan 11 §3.6).
    pub is_liquid: Option<bool>,
    /// `Floor.shallow` (`ShallowLiquid.set` sets true; plan 11 §3.6).
    pub shallow: Option<bool>,
    /// `Floor.drownTime` (ticks to drown; plan 11 `isDeep`/drown; plan 10).
    pub drown_time: Option<f32>,
    /// `Floor.liquidMultiplier` (deep-water pump yield; plan 10/11).
    pub liquid_multiplier: Option<f32>,
    /// `Block.crushFragile` (instantly destroyed by crushing tanks; plan 11).
    pub crush_fragile: Option<bool>,
    /// `Block.crushDamageMultiplier` (tank-crush damage scaling; plan 11).
    pub crush_damage_multiplier: Option<f32>,
    /// `Block.unitMoveBreakable` (deconstructed by moving units; plan 11).
    pub unit_move_breakable: Option<bool>,
    /// `Block.alwaysReplace` (plan 07 `Block.canReplace`).
    pub always_replace: Option<bool>,
    /// `Block.privileged` (world processors/cells; plan 07/13).
    pub privileged: Option<bool>,
    /// `Block.replaceable` (`canReplace`; plan 07).
    pub replaceable: Option<bool>,
    /// `Block.quickRotate` (`canReplace` self-replacement; plan 07).
    pub quick_rotate: Option<bool>,
    /// Java `Block.subclass` proxy; plan 07 uses `kind` equality when unset.
    pub subclass: Option<BlockKind>,
    /// `GenericCrafter.craftTime` (ticks); plan 07 R2 reconciliation.
    pub craft_time: Option<f32>,
    /// `Block.commandable` (unit command center; plan 11/15 `commandBuilding`).
    pub commandable: Option<bool>,
    /// Floor/environment attributes (`Floor.attributes`; plan 09 R3 steam vents).
    pub attributes: Vec<(&'static str, f32)>,
}

impl Default for BlockSpec {
    fn default() -> Self {
        Self {
            name: "",
            kind: BlockKind::Block,
            size: None,
            health: None,
            scaled_health: None,
            armor: None,
            category: None,
            requirements: Vec::new(),
            research_cost: None,
            research_cost_multiplier: None,
            research_cost_multipliers: Vec::new(),
            build_cost_multiplier: None,
            build_time: None,
            group: None,
            priority: None,
            unit_cap_modifier: None,
            flags: Vec::new(),
            consumes: Vec::new(),
            unit_plans: Vec::new(),
            upgrades: Vec::new(),
            assembler_plans: Vec::new(),
            item_capacity: None,
            liquid_capacity: None,
            has_items: None,
            has_liquids: None,
            has_power: None,
            outputs_power: None,
            consumes_power: None,
            conductive_power: None,
            outputs_liquid: None,
            build_visibility: None,
            env_required: None,
            env_enabled: None,
            env_disabled: None,
            solid: None,
            floating: None,
            update: None,
            destructible: None,
            save_data: None,
            save_config: None,
            configurable: None,
            in_editor: None,
            placeable_player: None,
            placeable_liquid: None,
            placeable_on: None,
            insulated: None,
            absorb_lasers: None,
            allow_core_placement: None,
            player_unmineable: None,
            wall_ore: None,
            item_drop: None,
            ore_default: None,
            ore_threshold: None,
            ore_scale: None,
            fog_radius: None,
            region: None,
            generate_icons: None,
            map_color: None,
            has_color: None,
            square_sprite: None,
            cache_layer: None,
            draw_cached: None,
            draw_dynamic: None,
            building_cache_layer: None,
            fills_tile: None,
            has_shadow: None,
            custom_shadow: None,
            obstructs_light: None,
            emit_light: None,
            draw_team_overlay: None,
            liquid_drop: None,
            speed_multiplier: None,
            is_liquid: None,
            shallow: None,
            drown_time: None,
            liquid_multiplier: None,
            crush_fragile: None,
            crush_damage_multiplier: None,
            unit_move_breakable: None,
            always_replace: None,
            privileged: None,
            replaceable: None,
            quick_rotate: None,
            subclass: None,
            craft_time: None,
            commandable: None,
            attributes: Vec::new(),
        }
    }
}

impl BlockSpec {
    /// Class-default spec for `kind`, transcribed from the upstream constructors
    /// (`world/blocks/**`). Fields not listed keep the Java `Block` base defaults.
    pub fn for_kind(kind: BlockKind) -> Self {
        let mut spec = Self {
            kind,
            ..Self::default()
        };
        // Draw-flag class defaults (plan-02 reconciliation; plan 16 §3.13). Base
        // `Block` values: `cacheLayer=normal`, `drawDynamic=true`,
        // `drawCached=false`, `buildingCacheLayer=normal`, `fillsTile=true`,
        // `hasShadow=true`, `customShadow=false`, `obstructsLight=true`,
        // `emitLight=false`, `drawTeamOverlay=true`.
        match kind {
            BlockKind::StaticProp => {
                spec.draw_cached = Some(true);
                spec.draw_dynamic = Some(false);
            }
            BlockKind::StorageBlock
            | BlockKind::Unloader
            | BlockKind::LogicBlock
            | BlockKind::MemoryBlock
            | BlockKind::SwitchBlock
            | BlockKind::LogicDisplay
            | BlockKind::TileableLogicDisplay
            | BlockKind::CanvasBlock
            | BlockKind::MessageBlock
            | BlockKind::Router
            | BlockKind::Sorter
            | BlockKind::Junction
            | BlockKind::OverflowGate
            | BlockKind::OverflowDuct
            | BlockKind::DuctRouter
            | BlockKind::DirectionalUnloader
            | BlockKind::ItemSource
            | BlockKind::LiquidJunction => {
                spec.draw_cached = Some(true);
                spec.draw_dynamic = Some(false);
            }
            BlockKind::Conduit
            | BlockKind::ArmoredConduit
            | BlockKind::Duct
            | BlockKind::StackConveyor => {
                spec.draw_cached = Some(true);
                spec.building_cache_layer = Some(BuildingCacheLayer::UNDER);
            }
            BlockKind::PowerNode
            | BlockKind::LongPowerNode
            | BlockKind::ItemBridge
            | BlockKind::BufferedItemBridge
            | BlockKind::DuctBridge
            | BlockKind::LiquidBridge
            | BlockKind::DirectionLiquidBridge => {
                spec.draw_cached = Some(true);
            }
            BlockKind::CoreBlock
            | BlockKind::ConsumeGenerator
            | BlockKind::ThermalGenerator
            | BlockKind::NuclearReactor
            | BlockKind::ImpactReactor
            | BlockKind::LightBlock
            | BlockKind::LandingPad
            | BlockKind::Accelerator => {
                spec.emit_light = Some(true);
            }
            _ => {}
        }
        match kind {
            BlockKind::Block => {}
            BlockKind::UnitFactory
            | BlockKind::Reconstructor
            | BlockKind::UnitAssembler
            | BlockKind::CoreBlock
            | BlockKind::PayloadSource => {
                // Those constructors set `commandable = true` (unit command
                // centers; plan 11/15 `InputHandler.commandBuilding` gate).
                spec.commandable = Some(true);
            }
            BlockKind::AirBlock => {
                // extends Floor
                spec.placeable_liquid = Some(true);
                spec.generate_icons = Some(false);
                spec.has_shadow = Some(false);
                spec.draw_cached = Some(false);
                spec.draw_dynamic = Some(false);
                // `AirBlock` constructor: `alwaysReplace = true`.
                spec.always_replace = Some(true);
            }
            BlockKind::SpawnBlock => spec.placeable_liquid = Some(true),
            BlockKind::RemoveWall => {
                spec.placeable_liquid = Some(true);
                spec.in_editor = Some(false);
            }
            BlockKind::RemoveOre => {
                spec.placeable_liquid = Some(true);
                spec.in_editor = Some(false);
            }
            BlockKind::Cliff => {
                spec.solid = Some(true);
                spec.cache_layer = Some(CacheLayerId::Walls);
                spec.fills_tile = Some(false);
                spec.has_shadow = Some(false);
            }
            BlockKind::ConstructBlock => {
                spec.update = Some(true);
                spec.health = Some(10);
                spec.in_editor = Some(false);
                spec.generate_icons = Some(false);
            }
            BlockKind::Floor => {
                spec.placeable_liquid = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
            }
            BlockKind::EmptyFloor => {
                spec.placeable_liquid = Some(true);
                spec.placeable_on = Some(false);
                spec.solid = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
            }
            BlockKind::OverlayFloor => {
                spec.placeable_liquid = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
            }
            BlockKind::OreBlock => {
                spec.placeable_liquid = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
            }
            BlockKind::StaticWall => {
                spec.solid = Some(true);
                spec.placeable_liquid = Some(true);
                spec.cache_layer = Some(CacheLayerId::Walls);
            }
            BlockKind::StaticProp => {
                spec.draw_cached = Some(true);
                spec.draw_dynamic = Some(false);
                // `Prop` constructor: `alwaysReplace = true`, `unitMoveBreakable = true`
                // (inherited); `StaticProp` only overrides the draw flags.
                spec.always_replace = Some(true);
                spec.unit_move_breakable = Some(true);
            }
            BlockKind::StaticTree => {
                spec.solid = Some(true);
                spec.placeable_liquid = Some(true);
                spec.cache_layer = Some(CacheLayerId::Walls);
                spec.always_replace = Some(true);
            }
            BlockKind::Prop => {
                // `Prop` constructor: `alwaysReplace = true`, `unitMoveBreakable = true`.
                spec.always_replace = Some(true);
                spec.unit_move_breakable = Some(true);
            }
            BlockKind::TreeBlock => {
                spec.solid = Some(true);
                spec.custom_shadow = Some(true);
                spec.always_replace = Some(true);
            }
            BlockKind::TallBlock => {
                spec.solid = Some(true);
                spec.custom_shadow = Some(true);
                spec.has_shadow = Some(true);
                spec.always_replace = Some(true);
            }
            BlockKind::SeaBush => {
                spec.obstructs_light = Some(false);
                spec.always_replace = Some(true);
            }
            BlockKind::Seaweed => {
                spec.obstructs_light = Some(false);
                spec.always_replace = Some(true);
            }
            BlockKind::SteamVent => {
                spec.placeable_liquid = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
                spec.flags.push(BlockFlag::SteamVent);
            }
            BlockKind::ShallowLiquid => {
                spec.placeable_liquid = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
                // `ShallowLiquid` copies the liquid base's cache layer; the
                // generated waves carry water-family liquids, default water.
                spec.cache_layer = Some(CacheLayerId::Water);
            }
            BlockKind::CharacterOverlay => {
                spec.placeable_liquid = Some(true);
                spec.save_data = Some(true);
                spec.save_config = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
            }
            BlockKind::RuneOverlay => {
                spec.placeable_liquid = Some(true);
                spec.save_data = Some(true);
                spec.save_config = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
            }
            BlockKind::ColoredFloor => {
                spec.placeable_liquid = Some(true);
                spec.save_data = Some(true);
                spec.save_config = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
            }
            BlockKind::ColoredWall => {
                spec.solid = Some(true);
                spec.placeable_liquid = Some(true);
                spec.save_data = Some(true);
                spec.save_config = Some(true);
                spec.cache_layer = Some(CacheLayerId::Walls);
            }
            BlockKind::GenericCrafter => {
                spec.update = Some(true);
                spec.solid = Some(true);
                spec.has_items = Some(true);
                spec.flags.push(BlockFlag::Factory);
            }
            BlockKind::AttributeCrafter | BlockKind::HeatCrafter | BlockKind::HeatProducer => {
                spec.update = Some(true);
                spec.solid = Some(true);
                spec.has_items = Some(true);
                spec.flags.push(BlockFlag::Factory);
            }
            BlockKind::Separator => {
                spec.update = Some(true);
                spec.solid = Some(true);
                spec.has_items = Some(true);
                spec.has_liquids = Some(true);
            }
            BlockKind::Incinerator => {
                spec.update = Some(true);
                spec.solid = Some(true);
                spec.has_power = Some(true);
                spec.has_liquids = Some(true);
            }
            BlockKind::ItemIncinerator => {
                spec.update = Some(true);
                spec.solid = Some(true);
            }
            BlockKind::HeatConductor => {
                spec.update = Some(true);
                spec.solid = Some(true);
            }
            BlockKind::Wall => {
                spec.solid = Some(true);
                spec.destructible = Some(true);
                spec.group = Some(BlockGroup::Walls);
                spec.build_cost_multiplier = Some(6.0);
                spec.priority = Some(TARGET_PRIORITY_WALL);
                spec.env_enabled = Some(EnvMask::any());
                // `Wall.java:45`: `crushDamageMultiplier = 5f`.
                spec.crush_damage_multiplier = Some(5.0);
                // `Wall.init()`: drawCached = true; drawDynamic = false unless flashHit.
                spec.draw_cached = Some(true);
                spec.draw_dynamic = Some(false);
            }
            BlockKind::ShieldWall => {
                spec.solid = Some(true);
                spec.destructible = Some(true);
                spec.update = Some(true);
                spec.group = Some(BlockGroup::Walls);
                spec.build_cost_multiplier = Some(6.0);
                spec.priority = Some(TARGET_PRIORITY_WALL);
                spec.env_enabled = Some(EnvMask::any());
                // `ShieldWall extends Wall`: inherits `crushDamageMultiplier = 5`.
                spec.crush_damage_multiplier = Some(5.0);
                // `ShieldWall.init()` re-enables dynamic drawing for the shield glow.
                spec.draw_cached = Some(true);
                spec.draw_dynamic = Some(true);
            }
            BlockKind::Door => {
                spec.solid = Some(false);
                spec.destructible = Some(true);
                spec.group = Some(BlockGroup::Walls);
                spec.build_cost_multiplier = Some(6.0);
                spec.priority = Some(TARGET_PRIORITY_WALL);
                spec.env_enabled = Some(EnvMask::any());
                spec.draw_cached = Some(true);
                spec.draw_dynamic = Some(false);
            }
            BlockKind::AutoDoor => {
                spec.solid = Some(false);
                spec.destructible = Some(true);
                spec.update = Some(true);
                spec.group = Some(BlockGroup::Walls);
                spec.build_cost_multiplier = Some(6.0);
                spec.priority = Some(TARGET_PRIORITY_WALL);
                spec.env_enabled = Some(EnvMask::any());
                spec.draw_cached = Some(true);
                spec.draw_dynamic = Some(false);
            }
            BlockKind::MendProjector => {
                spec.solid = Some(true);
                spec.update = Some(true);
                spec.group = Some(BlockGroup::Projectors);
                spec.has_power = Some(true);
                spec.has_items = Some(true);
                spec.flags.push(BlockFlag::BlockRepair);
                spec.emit_light = Some(true);
                spec.draw_cached = Some(true);
            }
            BlockKind::OverdriveProjector => {
                spec.solid = Some(true);
                spec.update = Some(true);
                spec.group = Some(BlockGroup::Projectors);
                spec.has_power = Some(true);
                spec.has_items = Some(true);
                spec.emit_light = Some(true);
            }
            BlockKind::ForceProjector => {
                spec.solid = Some(true);
                spec.update = Some(true);
                spec.group = Some(BlockGroup::Projectors);
                spec.has_power = Some(true);
                spec.has_liquids = Some(true);
                spec.has_items = Some(true);
                spec.flags.push(BlockFlag::Shield);
            }
            BlockKind::BaseShield => {
                spec.solid = Some(true);
                spec.update = Some(true);
                spec.has_power = Some(true);
            }
            BlockKind::ShockMine => {
                spec.solid = Some(false);
                spec.destructible = Some(true);
            }
            BlockKind::Radar => {
                spec.solid = Some(true);
                spec.update = Some(true);
                spec.flags.push(BlockFlag::HasFogRadius);
                spec.fog_radius = Some(10);
            }
            BlockKind::BuildTurret => {
                spec.solid = Some(true);
                spec.update = Some(true);
                spec.group = Some(BlockGroup::Turrets);
                spec.priority = Some(TARGET_PRIORITY_TURRET);
                spec.flags.push(BlockFlag::Turret);
            }
            BlockKind::RegenProjector => {
                spec.solid = Some(true);
                spec.update = Some(true);
                spec.group = Some(BlockGroup::Projectors);
                spec.has_power = Some(true);
                spec.has_items = Some(true);
                spec.flags.push(BlockFlag::BlockRepair);
                spec.emit_light = Some(true);
            }
            BlockKind::ShockwaveTower => {
                spec.solid = Some(true);
                spec.update = Some(true);
            }
            BlockKind::Thruster => {
                spec.solid = Some(true);
            }
            // B3-B6 kinds carry their class defaults in the generated waves.
            _ => {}
        }
        spec
    }
}

/// `TargetPriority.wall`.
pub const TARGET_PRIORITY_WALL: f32 = -3.0;
/// `TargetPriority.under`.
pub const TARGET_PRIORITY_UNDER: f32 = -2.0;
/// `TargetPriority.transport`.
pub const TARGET_PRIORITY_TRANSPORT: f32 = -1.0;
/// `TargetPriority.base`.
pub const TARGET_PRIORITY_BASE: f32 = 0.0;
/// `TargetPriority.turret`.
pub const TARGET_PRIORITY_TURRET: f32 = 1.0;
/// `TargetPriority.core`.
pub const TARGET_PRIORITY_CORE: f32 = 2.0;

/// Builds a spec with the class defaults for `kind` (`BlockSpec::for_kind`).
pub fn spec(name: &'static str, kind: BlockKind) -> BlockSpec {
    let mut spec = BlockSpec::for_kind(kind);
    spec.name = name;
    spec
}

/// `consumeItem`/`consumeItems` helper for generated waves.
pub fn consume_item(item: &'static str, amount: i32) -> ConsumeDef {
    ConsumeDef::Items {
        stacks: vec![StackSpec { item, amount }],
        optional: false,
        ignore: false,
        update: true,
    }
}

/// `consumeItems` helper for generated waves.
pub fn consume_items(stacks: Vec<StackSpec>) -> ConsumeDef {
    ConsumeDef::Items {
        stacks,
        optional: false,
        ignore: false,
        update: true,
    }
}

/// `consumeLiquid` helper for generated waves.
pub fn consume_liquid(liquid: &'static str, amount: f32) -> ConsumeDef {
    ConsumeDef::Liquid {
        liquid,
        amount,
        optional: false,
        update: true,
    }
}

/// `consumeLiquids` helper for generated waves.
pub fn consume_liquids(stacks: Vec<LiquidStackSpec>) -> ConsumeDef {
    ConsumeDef::Liquids {
        stacks,
        optional: false,
        update: true,
    }
}

/// `consumePower` helper for generated waves.
pub fn consume_power(usage: f32) -> ConsumeDef {
    ConsumeDef::Power {
        usage,
        buffered: 0.0,
        update: true,
    }
}

/// `consumePowerBuffered` helper for generated waves.
pub fn consume_power_buffered(capacity: f32) -> ConsumeDef {
    ConsumeDef::Power {
        usage: 0.0,
        buffered: capacity,
        update: true,
    }
}

/// `consumeCoolant` helper for generated waves.
pub fn consume_coolant(amount: f32, allow_liquid: bool, allow_gas: bool) -> ConsumeDef {
    ConsumeDef::Coolant {
        amount,
        allow_liquid,
        allow_gas,
        optional: false,
        update: true,
    }
}

/// `consume*` optional wrapper (`.optional(...)`/`.boost()`).
pub fn consume_optional(mut consume: ConsumeDef) -> ConsumeDef {
    match &mut consume {
        ConsumeDef::Items { optional, .. }
        | ConsumeDef::Liquid { optional, .. }
        | ConsumeDef::Liquids { optional, .. }
        | ConsumeDef::Coolant { optional, .. } => *optional = true,
        ConsumeDef::Power { .. } => {}
    }
    consume
}

/// `Color.valueOf(hex)` helper for generated waves.
pub fn rgba_hex(hex: &str) -> Rgba {
    Rgba::from_hex(hex).unwrap_or(Rgba::WHITE)
}

/// Block metadata record (plan 02 §6.1).
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDef {
    /// Dense id in the block content space.
    pub id: BlockId,
    /// Content name (parity ABI; already mod-prefixed).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Unlock/database fields.
    pub unlock: UnlockFields,
    /// Java class tag.
    pub kind: BlockKind,
    /// Multiblock size in tiles.
    pub size: i32,
    /// Building health (`Block.health`).
    pub health: i32,
    /// Health per tile before `health` derivation (`Block.scaledHealth`).
    pub scaled_health: f32,
    /// Damage absorption.
    pub armor: f32,
    /// Build requirements.
    pub requirements: Vec<ItemStack>,
    /// Explicit research cost (`researchCost`); `None` = derived.
    pub research_cost: Option<Vec<ItemStack>>,
    /// Research cost multiplier.
    pub research_cost_multiplier: f32,
    /// Per-item research cost multipliers.
    pub research_cost_multipliers: Vec<(ItemId, f32)>,
    /// Build time multiplier.
    pub build_cost_multiplier: f32,
    /// Build time in ticks (`Block.buildTime`, derived in `init`).
    pub build_time: f32,
    /// Place category.
    pub category: Category,
    /// Replace group.
    pub group: BlockGroup,
    /// Targeting priority.
    pub priority: f32,
    /// Unit cap contribution.
    pub unit_cap_modifier: i32,
    /// Special flags.
    pub flags: Vec<BlockFlag>,
    /// Consumers in declaration order.
    pub consumes: Vec<ConsumeSpec>,
    /// Unit factory plans (`UnitFactory.plans`).
    pub unit_plans: Vec<UnitPlanDef>,
    /// Reconstructor upgrade pairs (`Reconstructor.upgrades`).
    pub reconstructor_upgrades: Vec<(UnitTypeId, UnitTypeId)>,
    /// Assembler plans (`UnitAssembler.plans`).
    pub assembler_plans: Vec<AssemblerUnitPlanDef>,
    /// Indices of optional, non-ignored consumers.
    pub optional_consumers: Vec<usize>,
    /// Indices of non-optional, non-ignored consumers.
    pub non_optional_consumers: Vec<usize>,
    /// Indices of updating, non-ignored consumers.
    pub update_consumers: Vec<usize>,
    /// Index of the power consumer, if any.
    pub cons_power: Option<usize>,
    /// Item capacity.
    pub item_capacity: i32,
    /// Liquid capacity (`<0` = derived).
    pub liquid_capacity: f32,
    /// Has an item module.
    pub has_items: bool,
    /// Has a liquid module.
    pub has_liquids: bool,
    /// Has a power module.
    pub has_power: bool,
    /// Outputs power.
    pub outputs_power: bool,
    /// Consumes power.
    pub consumes_power: bool,
    /// Conducts power.
    pub conductive_power: bool,
    /// Outputs liquids.
    pub outputs_liquid: bool,
    /// Accepts items (`acceptsItems`; derived for transportation/distribution).
    pub accepts_items: bool,
    /// Build-menu visibility.
    pub build_visibility: BuildVisibility,
    /// Required environment.
    pub env_required: EnvMask,
    /// Enabled environments.
    pub env_enabled: EnvMask,
    /// Disabled environments.
    pub env_disabled: EnvMask,
    /// Solid.
    pub solid: bool,
    /// Floats on liquids.
    pub floating: bool,
    /// Updates each tick.
    pub update: bool,
    /// Destructible.
    pub destructible: bool,
    /// Saves tile data.
    pub save_data: bool,
    /// Saves placement config.
    pub save_config: bool,
    /// Configurable.
    pub configurable: bool,
    /// Visible in the editor.
    pub in_editor: bool,
    /// Player-placeable.
    pub placeable_player: bool,
    /// Placeable on liquids.
    pub placeable_liquid: bool,
    /// Floors can be placed on this.
    pub placeable_on: bool,
    /// Insulated.
    pub insulated: bool,
    /// Absorbs lasers.
    pub absorb_lasers: bool,
    /// Core zone placement allowed.
    pub allow_core_placement: bool,
    /// Cannot be mined by players.
    pub player_unmineable: bool,
    /// Wall ore block.
    pub wall_ore: bool,
    /// Dropped item.
    pub item_drop: Option<ItemId>,
    /// Default ore flag.
    pub ore_default: bool,
    /// Ore threshold.
    pub ore_threshold: f32,
    /// Ore scale.
    pub ore_scale: f32,
    /// Fog radius (`-1` = none).
    pub fog_radius: i32,
    /// Sprite region name (`Block.region`).
    pub region: String,
    /// Multiblock draw offset (`Block.offset`).
    pub offset: f32,
    /// Centered iteration offset (`Block.sizeOffset`).
    pub size_offset: i32,
    /// Stat data (`setStats`).
    pub stats: Vec<StatSpec>,
    /// Bar data (`setBars`).
    pub bars: Vec<BarSpec>,
    /// Map color.
    pub map_color: Option<Rgba>,
    /// Use the block color in the minimap.
    pub has_color: bool,
    /// Square sprite.
    pub square_sprite: bool,
    /// `Block.cacheLayer` (floor/wall draw band; `normal` = dynamic block pass).
    pub cache_layer: CacheLayerId,
    /// `Block.drawCached` (baked into the per-chunk building cache).
    pub draw_cached: bool,
    /// `Block.drawDynamic` (`draw()` runs every frame).
    pub draw_dynamic: bool,
    /// `Block.buildingCacheLayer` (`Layer.block - 0.5` / `Layer.block`).
    pub building_cache_layer: f32,
    /// `Block.fillsTile` (floor hidden under this block; darkness owner).
    pub fills_tile: bool,
    /// `Block.hasShadow`.
    pub has_shadow: bool,
    /// `Block.customShadow`.
    pub custom_shadow: bool,
    /// `Block.displayShadow(tile)` (base: `hasShadow`).
    pub display_shadow: bool,
    /// `Block.obstructsLight` (excluded from the light quadtree).
    pub obstructs_light: bool,
    /// `Block.emitLight` (`drawLight()` is called).
    pub emit_light: bool,
    /// `Block.drawTeamOverlay` (team corner is drawn for enemy buildings).
    pub draw_team_overlay: bool,
    /// Item base costs indexed by item id (`Item.cost`; used by `init`).
    pub(crate) item_costs: Vec<f32>,
    /// Item health scaling indexed by item id (`Item.healthScaling`; used by `init`).
    pub(crate) item_health_scaling: Vec<f32>,
    /// Floor/environment attributes keyed by attribute name (`Floor.attributes`;
    /// the name-keyed half mirrors upstream's dynamic `Attribute` registry; plan
    /// 20 M3b patches this via `block.<name>.attributes.<attr>`).
    pub attributes: Vec<(String, f32)>,
    /// `Drill.drillMultipliers` (`ObjectFloatMap<Item>`); plan 20 M3b patches this
    /// via `block.<name>.drillMultipliers`.
    pub drill_multipliers: Vec<(ItemId, f32)>,
    /// `Floor.liquidDrop` (`Option<Liquid>`; plan 10 §3.10 append-only addendum).
    pub liquid_drop: Option<LiquidId>,
    /// `Floor.speedMultiplier` (plan 11 §3.6 unit velocity multiplier).
    pub speed_multiplier: f32,
    /// `Floor.isLiquid` (plan 11 §3.6 naval predicate).
    pub is_liquid: bool,
    /// `Floor.shallow` (plan 11 §3.6 shallow-speed rule).
    pub shallow: bool,
    /// `Floor.isDeep()` (`drownTime > 0`; plan 11 §3.6 deep-floor dispatch).
    pub is_deep: bool,
    /// `Floor.drownTime` (plan 10/11 drowning).
    pub drown_time: f32,
    /// `Floor.liquidMultiplier` (plan 10 pump yield).
    pub liquid_multiplier: f32,
    /// `Block.crushFragile` (plan 11 tank crush).
    pub crush_fragile: bool,
    /// `Block.crushDamageMultiplier` (plan 11 tank crush damage; `Wall` = 5).
    pub crush_damage_multiplier: f32,
    /// `Block.unitMoveBreakable` (plan 11 unit deconstruct).
    pub unit_move_breakable: bool,
    /// `Block.alwaysReplace` (plan 07 `can_replace`).
    pub always_replace: bool,
    /// `Block.privileged` (plan 07/13 world processors).
    pub privileged: bool,
    /// `Block.replaceable` (plan 07 `can_replace`).
    pub replaceable: bool,
    /// `Block.quickRotate` (plan 07 `can_replace` self-replacement).
    pub quick_rotate: bool,
    /// Java `Block.subclass` proxy (plan 07 uses `kind` equality when `kind`).
    pub subclass: BlockKind,
    /// `GenericCrafter.craftTime` (ticks); plan 07 R2 reconciliation.
    pub craft_time: f32,
    /// `Block.commandable` (unit command center; plan 11/15 `commandBuilding`).
    pub commandable: bool,
}

impl BlockDef {
    /// Resolves a generated [`BlockSpec`] into a record
    /// (`ContentLoader`-adjacent construction; names resolve at load time like
    /// upstream object references).
    pub fn from_spec(
        spec: BlockSpec,
        registry: &ContentRegistry,
        bundle: &dyn BundleView,
        store: &dyn UnlockStore,
    ) -> Result<Self, ContentError> {
        let item_costs: Vec<f32> = registry.items().iter().map(|item| item.cost).collect();
        let item_health_scaling: Vec<f32> = registry
            .items()
            .iter()
            .map(|item| item.health_scaling)
            .collect();
        let generate_icons = spec.generate_icons;
        let mut requirements = Vec::with_capacity(spec.requirements.len());
        for stack in &spec.requirements {
            requirements.push(resolve_item(registry, stack)?);
        }
        let research_cost = match &spec.research_cost {
            Some(stacks) => {
                let mut out = Vec::with_capacity(stacks.len());
                for stack in stacks {
                    out.push(resolve_item(registry, stack)?);
                }
                Some(out)
            }
            None => None,
        };
        let mut consumes = Vec::with_capacity(spec.consumes.len());
        for consume in spec.consumes {
            consumes.push(resolve_consume(registry, consume)?);
        }
        let mut research_cost_multipliers =
            Vec::with_capacity(spec.research_cost_multipliers.len());
        for (name, multiplier) in &spec.research_cost_multipliers {
            let item = registry
                .item_id(name)
                .ok_or_else(|| ContentError::UnknownName((*name).to_owned()))?;
            research_cost_multipliers.push((item, *multiplier));
        }
        let item_drop = match spec.item_drop {
            Some(name) => Some(
                registry
                    .item_id(name)
                    .ok_or_else(|| ContentError::UnknownName(name.to_owned()))?,
            ),
            None => None,
        };
        let liquid_drop = match spec.liquid_drop {
            Some(name) => Some(
                registry
                    .liquid_id(name)
                    .ok_or_else(|| ContentError::UnknownName(name.to_owned()))?,
            ),
            None => None,
        };
        // `Floor.isDeep()` = `drownTime > 0` (`Floor.java:339`).
        let drown_time = spec.drown_time.unwrap_or(0.0);
        let is_deep = drown_time > 0.0;
        let subclass = spec.subclass.unwrap_or(spec.kind);
        let mut unit_plans = Vec::with_capacity(spec.unit_plans.len());
        for plan in &spec.unit_plans {
            let unit = registry
                .unit_id(plan.unit)
                .ok_or_else(|| ContentError::UnknownName(plan.unit.to_owned()))?;
            let mut requirements = Vec::with_capacity(plan.requirements.len());
            for stack in &plan.requirements {
                requirements.push(resolve_item(registry, stack)?);
            }
            unit_plans.push(UnitPlanDef {
                unit,
                time: plan.time,
                requirements,
            });
        }
        let mut reconstructor_upgrades = Vec::with_capacity(spec.upgrades.len());
        for (from, to) in &spec.upgrades {
            let from = registry
                .unit_id(from)
                .ok_or_else(|| ContentError::UnknownName((*from).to_owned()))?;
            let to = registry
                .unit_id(to)
                .ok_or_else(|| ContentError::UnknownName((*to).to_owned()))?;
            reconstructor_upgrades.push((from, to));
        }
        let mut assembler_plans = Vec::with_capacity(spec.assembler_plans.len());
        for plan in &spec.assembler_plans {
            let unit = registry
                .unit_id(plan.unit)
                .ok_or_else(|| ContentError::UnknownName(plan.unit.to_owned()))?;
            let mut payloads = Vec::with_capacity(plan.payloads.len());
            for payload in &plan.payloads {
                let item = if payload.block {
                    let id = registry
                        .block_id(payload.name)
                        .ok_or_else(|| ContentError::UnknownName(payload.name.to_owned()))?;
                    ContentRef::block(id)
                } else {
                    let id = registry
                        .unit_id(payload.name)
                        .ok_or_else(|| ContentError::UnknownName(payload.name.to_owned()))?;
                    ContentRef::of(ContentType::Unit, id)
                };
                payloads.push(PayloadStack::new(item, payload.amount));
            }
            assembler_plans.push(AssemblerUnitPlanDef {
                unit,
                time: plan.time,
                payloads,
            });
        }
        let region = spec
            .region
            .map(str::to_owned)
            .unwrap_or_else(|| spec.name.to_owned());
        // `Block.init()`: `customShadow` disables the standard shadow unless the
        // wave explicitly re-enables `hasShadow` (e.g. `TallBlock`).
        let custom_shadow = spec.custom_shadow.unwrap_or(false);
        let has_shadow = if custom_shadow && spec.has_shadow.is_none() {
            false
        } else {
            spec.has_shadow.unwrap_or(true)
        };
        let mut def = Self {
            id: BlockId::new(0),
            name: spec.name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            unlock: UnlockFields::new(ContentType::Block, spec.name, bundle, store),
            kind: spec.kind,
            size: spec.size.unwrap_or(1),
            health: spec.health.unwrap_or(-1),
            scaled_health: spec.scaled_health.unwrap_or(-1.0),
            armor: spec.armor.unwrap_or(0.0),
            requirements,
            research_cost,
            research_cost_multiplier: spec.research_cost_multiplier.unwrap_or(1.0),
            research_cost_multipliers,
            build_cost_multiplier: spec.build_cost_multiplier.unwrap_or(1.0),
            build_time: spec.build_time.unwrap_or(-1.0),
            category: spec.category.unwrap_or(Category::Distribution),
            group: spec.group.unwrap_or(BlockGroup::None),
            priority: spec.priority.unwrap_or(TARGET_PRIORITY_BASE),
            unit_cap_modifier: spec.unit_cap_modifier.unwrap_or(BASE_UNIT_CAP_MODIFIER),
            flags: spec.flags,
            consumes,
            unit_plans,
            reconstructor_upgrades,
            assembler_plans,
            optional_consumers: Vec::new(),
            non_optional_consumers: Vec::new(),
            update_consumers: Vec::new(),
            cons_power: None,
            item_capacity: spec.item_capacity.unwrap_or(10),
            liquid_capacity: spec.liquid_capacity.unwrap_or(-1.0),
            has_items: spec.has_items.unwrap_or(false),
            has_liquids: spec.has_liquids.unwrap_or(false),
            has_power: spec.has_power.unwrap_or(false),
            outputs_power: spec.outputs_power.unwrap_or(false),
            consumes_power: spec.consumes_power.unwrap_or(true),
            conductive_power: spec.conductive_power.unwrap_or(false),
            outputs_liquid: spec.outputs_liquid.unwrap_or(false),
            accepts_items: false,
            build_visibility: spec.build_visibility.unwrap_or(BuildVisibility::Hidden),
            env_required: spec.env_required.unwrap_or_default(),
            env_enabled: spec
                .env_enabled
                .unwrap_or_else(|| EnvMask::of(vec![EnvFlag::Terrestrial])),
            env_disabled: spec.env_disabled.unwrap_or_default(),
            solid: spec.solid.unwrap_or(false),
            floating: spec.floating.unwrap_or(false),
            update: spec.update.unwrap_or(false),
            destructible: spec.destructible.unwrap_or(false),
            save_data: spec.save_data.unwrap_or(false),
            save_config: spec.save_config.unwrap_or(false),
            configurable: spec.configurable.unwrap_or(false),
            in_editor: spec.in_editor.unwrap_or(true),
            placeable_player: spec.placeable_player.unwrap_or(true),
            placeable_liquid: spec.placeable_liquid.unwrap_or(false),
            placeable_on: spec.placeable_on.unwrap_or(true),
            insulated: spec.insulated.unwrap_or(false),
            absorb_lasers: spec.absorb_lasers.unwrap_or(false),
            allow_core_placement: spec.allow_core_placement.unwrap_or(false),
            player_unmineable: spec.player_unmineable.unwrap_or(false),
            wall_ore: spec.wall_ore.unwrap_or(false),
            item_drop,
            ore_default: spec.ore_default.unwrap_or(false),
            ore_threshold: spec.ore_threshold.unwrap_or(0.828),
            ore_scale: spec.ore_scale.unwrap_or(24.0),
            fog_radius: spec.fog_radius.unwrap_or(-1),
            region,
            offset: 0.0,
            size_offset: 0,
            stats: Vec::new(),
            bars: Vec::new(),
            map_color: spec.map_color,
            has_color: spec.has_color.unwrap_or(false),
            square_sprite: spec.square_sprite.unwrap_or(true),
            cache_layer: spec.cache_layer.unwrap_or(CacheLayerId::Normal),
            draw_cached: spec.draw_cached.unwrap_or(false),
            draw_dynamic: spec.draw_dynamic.unwrap_or(true),
            building_cache_layer: spec
                .building_cache_layer
                .unwrap_or(BuildingCacheLayer::NORMAL),
            fills_tile: spec.fills_tile.unwrap_or(true),
            has_shadow,
            custom_shadow,
            // `displayShadow` is derived from `hasShadow`.
            display_shadow: has_shadow,
            obstructs_light: spec.obstructs_light.unwrap_or(true),
            emit_light: spec.emit_light.unwrap_or(false),
            draw_team_overlay: spec.draw_team_overlay.unwrap_or(true),
            item_costs,
            item_health_scaling,
            attributes: spec
                .attributes
                .iter()
                .map(|(name, value)| ((*name).to_string(), *value))
                .collect(),
            drill_multipliers: Vec::new(),
            liquid_drop,
            speed_multiplier: spec.speed_multiplier.unwrap_or(1.0),
            is_liquid: spec.is_liquid.unwrap_or(false),
            shallow: spec.shallow.unwrap_or(false),
            is_deep,
            drown_time,
            liquid_multiplier: spec.liquid_multiplier.unwrap_or(1.0),
            crush_fragile: spec.crush_fragile.unwrap_or(false),
            crush_damage_multiplier: spec.crush_damage_multiplier.unwrap_or(1.0),
            unit_move_breakable: spec.unit_move_breakable.unwrap_or(false),
            always_replace: spec.always_replace.unwrap_or(false),
            privileged: spec.privileged.unwrap_or(false),
            replaceable: spec.replaceable.unwrap_or(true),
            quick_rotate: spec.quick_rotate.unwrap_or(true),
            subclass,
            craft_time: spec.craft_time.unwrap_or(0.0),
            commandable: spec.commandable.unwrap_or(false),
        };
        if let Some(generate) = generate_icons {
            def.unlock.generate_icons = generate;
        }
        Ok(def)
    }

    /// Whether a building can have health (`Block.canBeBuilt`-adjacent metadata).
    pub fn has_health(&self) -> bool {
        self.health > 0
    }

    /// `Block.researchRequirements()` — ported formula (`Block.java:1284-1295`).
    pub fn research_requirements(&self) -> Vec<ItemStack> {
        if let Some(cost) = &self.research_cost {
            return cost.clone();
        }
        if self.research_cost_multiplier <= 0.0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(self.requirements.len());
        for stack in &self.requirements {
            let per_item = self
                .research_cost_multipliers
                .iter()
                .find(|(item, _)| *item == stack.item)
                .map(|(_, multiplier)| *multiplier)
                .unwrap_or(1.0);
            let amount = stack.amount as f64;
            let raw = 60.0 * self.research_cost_multiplier as f64
                + amount.powf(1.11) * 20.0 * self.research_cost_multiplier as f64 * per_item as f64;
            let quantity = round_to(raw as f32, 10.0);
            out.push(ItemStack::new(
                stack.item,
                ui_round_amount(round_to_i32(quantity)),
            ));
        }
        out
    }

    /// `Block.getDependencies()` — required items plus non-optional item inputs.
    pub fn get_dependencies(&self) -> Vec<ItemId> {
        let mut out: Vec<ItemId> = self.requirements.iter().map(|stack| stack.item).collect();
        for consumer in &self.consumes {
            if consumer.optional {
                continue;
            }
            if let Consume::Items(stacks) = &consumer.consume {
                for stack in stacks {
                    if !out.contains(&stack.item) {
                        out.push(stack.item);
                    }
                }
            }
        }
        out
    }

    /// Whether this block is placeable in the current build (metadata half).
    pub fn is_placeable(&self) -> bool {
        self.placeable_player && self.in_editor
    }

    /// `Block.isHidden()` metadata approximation (plan 14 owns the runtime reveal).
    pub fn is_hidden(&self) -> bool {
        !matches!(self.build_visibility, BuildVisibility::Shown)
    }

    /// Derived `has_items`/`has_liquids` consistency (plan 07 consumes this).
    pub fn consumer_partitions_consistent(&self) -> bool {
        let optional: Vec<usize> = self
            .consumes
            .iter()
            .enumerate()
            .filter(|(_, spec)| spec.optional && !spec.ignore)
            .map(|(index, _)| index)
            .collect();
        let non_optional: Vec<usize> = self
            .consumes
            .iter()
            .enumerate()
            .filter(|(_, spec)| !spec.optional && !spec.ignore)
            .map(|(index, _)| index)
            .collect();
        let update: Vec<usize> = self
            .consumes
            .iter()
            .enumerate()
            .filter(|(_, spec)| spec.update && !spec.ignore)
            .map(|(index, _)| index)
            .collect();
        optional == self.optional_consumers
            && non_optional == self.non_optional_consumers
            && update == self.update_consumers
    }
}

/// Resolves a generated item stack spec.
fn resolve_item(registry: &ContentRegistry, spec: &StackSpec) -> Result<ItemStack, ContentError> {
    let item = registry
        .item_id(spec.item)
        .ok_or_else(|| ContentError::UnknownName(spec.item.to_owned()))?;
    Ok(ItemStack::new(item, spec.amount))
}

/// Resolves a generated consumer spec.
fn resolve_consume(
    registry: &ContentRegistry,
    def: ConsumeDef,
) -> Result<ConsumeSpec, ContentError> {
    match def {
        ConsumeDef::Items {
            stacks,
            optional,
            ignore,
            update,
        } => {
            let mut out = Vec::with_capacity(stacks.len());
            for stack in &stacks {
                out.push(resolve_item(registry, stack)?);
            }
            Ok(ConsumeSpec {
                consume: Consume::Items(out),
                optional,
                update,
                ignore,
            })
        }
        ConsumeDef::Liquid {
            liquid,
            amount,
            optional,
            update,
        } => {
            let liquid = registry
                .liquid_id(liquid)
                .ok_or_else(|| ContentError::UnknownName(liquid.to_owned()))?;
            Ok(ConsumeSpec {
                consume: Consume::Liquid { liquid, amount },
                optional,
                update,
                ignore: false,
            })
        }
        ConsumeDef::Liquids {
            stacks,
            optional,
            update,
        } => {
            let mut out = Vec::with_capacity(stacks.len());
            for stack in &stacks {
                let liquid = registry
                    .liquid_id(stack.liquid)
                    .ok_or_else(|| ContentError::UnknownName(stack.liquid.to_owned()))?;
                out.push(LiquidStack::new(liquid, stack.amount));
            }
            Ok(ConsumeSpec {
                consume: Consume::Liquids(out),
                optional,
                update,
                ignore: false,
            })
        }
        ConsumeDef::Power {
            usage,
            buffered,
            update,
        } => Ok(ConsumeSpec {
            consume: Consume::Power { usage, buffered },
            optional: false,
            update,
            ignore: false,
        }),
        ConsumeDef::Coolant {
            amount,
            allow_liquid,
            allow_gas,
            optional,
            update,
        } => Ok(ConsumeSpec {
            consume: Consume::Coolant {
                amount,
                allow_liquid,
                allow_gas,
            },
            optional,
            update,
            ignore: false,
        }),
    }
}

impl Content for BlockDef {
    const TYPE: ContentType = ContentType::Block;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = BlockId::new(id);
    }

    fn minfo(&self) -> &ModContentInfo {
        &self.minfo
    }

    fn minfo_mut(&mut self) -> &mut ModContentInfo {
        &mut self.minfo
    }

    fn removed(&self) -> bool {
        self.removed
    }

    fn set_removed(&mut self, removed: bool) {
        self.removed = removed;
    }

    fn kind_name(&self) -> &'static str {
        self.kind.name()
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn unlock_fields(&self) -> Option<&UnlockFields> {
        Some(&self.unlock)
    }

    /// `Block.init()` metadata derivations (`world/Block.java:1357+`).
    fn init_self(&mut self) -> Result<(), ContentError> {
        // `Block.displayShadow(tile)` is `hasShadow` for every vanilla block
        // (no overrides), kept as a field for the render/metadata ABI.
        self.display_shadow = self.has_shadow;

        if self.fog_radius > 0 && !self.flags.contains(&BlockFlag::HasFogRadius) {
            self.flags.push(BlockFlag::HasFogRadius);
        }

        if self.health == -1 {
            let mut round = false;
            if self.scaled_health < 0.0 {
                self.scaled_health = 40.0;
                let mut scaling = 1.0f32;
                for stack in &self.requirements {
                    scaling += self
                        .item_health_scaling
                        .get(stack.item.index())
                        .copied()
                        .unwrap_or(0.0);
                }
                self.scaled_health *= scaling;
                round = true;
            }
            if round {
                let size = self.size as f32;
                self.health = round_to(size * size * self.scaled_health, 5.0) as i32;
            } else {
                let size = self.size as f32;
                self.health = (size * size * self.scaled_health) as i32;
            }
        }

        self.offset = ((self.size + 1) % 2) as f32 * TILE_SIZE / 2.0;
        self.size_offset = -((self.size - 1) / 2);

        if !self.requirements.is_empty() && self.build_time < 0.0 {
            self.build_time = 0.0;
            for stack in &self.requirements {
                let cost = self
                    .item_costs
                    .get(stack.item.index())
                    .copied()
                    .unwrap_or(1.0);
                self.build_time += stack.amount as f32 * cost;
            }
        }
        if self.build_time < 0.0 {
            self.build_time = 20.0;
        }
        self.build_time *= self.build_cost_multiplier;

        if self.liquid_capacity < 0.0 {
            let mut consume_amount = 1.0f32;
            for spec in &self.consumes {
                match &spec.consume {
                    Consume::Liquid { amount, .. } => {
                        consume_amount = consume_amount.max(amount * 60.0);
                    }
                    Consume::Liquids(stacks) => {
                        for stack in stacks {
                            consume_amount = consume_amount.max(stack.amount * 60.0);
                        }
                    }
                    Consume::Coolant { amount, .. } => {
                        consume_amount = consume_amount.max(amount * 60.0);
                    }
                    _ => {}
                }
            }
            self.liquid_capacity = round_to(10.0 * consume_amount, 1.0);
        }

        if self.group == BlockGroup::Transportation || self.category == Category::Distribution {
            self.accepts_items = true;
        }

        self.optional_consumers = self
            .consumes
            .iter()
            .enumerate()
            .filter(|(_, spec)| spec.optional && !spec.ignore)
            .map(|(index, _)| index)
            .collect();
        self.non_optional_consumers = self
            .consumes
            .iter()
            .enumerate()
            .filter(|(_, spec)| !spec.optional && !spec.ignore)
            .map(|(index, _)| index)
            .collect();
        self.update_consumers = self
            .consumes
            .iter()
            .enumerate()
            .filter(|(_, spec)| spec.update && !spec.ignore)
            .map(|(index, _)| index)
            .collect();
        self.cons_power = self
            .consumes
            .iter()
            .position(|spec| matches!(spec.consume, Consume::Power { .. }));

        // `Consume.apply(Block)` flags: liquid consumers imply `hasLiquids`
        // (`ConsumeLiquidBase`/`ConsumeLiquids`/`ConsumeLiquidFilter`) and
        // `ConsumePower` implies `hasPower`.
        for spec in &self.consumes {
            match &spec.consume {
                Consume::Liquid { .. } | Consume::Liquids(_) | Consume::Coolant { .. } => {
                    self.has_liquids = true;
                }
                Consume::Power { .. } => {
                    self.has_power = true;
                }
                _ => {}
            }
        }

        // `Block.init()`: every liquid block with `drawLiquidLight` emits light.
        if self.has_liquids {
            self.emit_light = true;
        }
        Ok(())
    }

    /// `UnlockableContent.postInit` + `Block.postInit` database defaults.
    fn post_init(&mut self) -> Result<(), ContentError> {
        if self
            .unlock
            .database_tag
            .as_deref()
            .is_none_or(str::is_empty)
        {
            self.unlock.database_tag = Some(self.category.name().to_owned());
        }
        self.unlock.post_init();
        Ok(())
    }

    fn after_patch(&mut self) -> Result<(), ContentError> {
        self.init_self()
    }
}

impl Mappable for BlockDef {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for BlockDef {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

/// `UI.roundAmount` (`core/src/mindustry/core/UI.java:736`).
///
/// The `>= 1000` and `>= 100` branches are byte-identical upstream; kept as-is
/// for source fidelity.
#[allow(clippy::if_same_then_else)]
pub fn ui_round_amount(number: i32) -> i32 {
    if number >= 1_000_000_000 {
        round_to(number as f32, 100_000_000.0) as i32
    } else if number >= 1_000_000 {
        round_to(number as f32, 100_000.0) as i32
    } else if number >= 10_000 {
        round_to(number as f32, 1000.0) as i32
    } else if number >= 1000 {
        round_to(number as f32, 100.0) as i32
    } else if number >= 100 {
        round_to(number as f32, 100.0) as i32
    } else if number >= 10 {
        round_to(number as f32, 10.0) as i32
    } else {
        number
    }
}

/// `Mathf.round(value, step)` — Arc truncates toward zero
/// (`(int)(value / step) * step`), it does **not** round to nearest.
pub fn round_to(value: f32, step: f32) -> f32 {
    ((value / step) as i32) as f32 * step
}

/// Sink for generated block waves.
pub trait BlockSink {
    /// Adds one generated block spec.
    fn push(&mut self, spec: BlockSpec) -> Result<(), ContentError>;
}

/// Sink that registers block names into the sim-facing [`Blocks`] table.
struct NamesSink<'a> {
    blocks: &'a mut Blocks,
}

impl BlockSink for NamesSink<'_> {
    fn push(&mut self, spec: BlockSpec) -> Result<(), ContentError> {
        self.blocks.register(spec.name)?;
        Ok(())
    }
}

/// Sink that builds `BlockDef` records inside a [`ContentRegistry`].
struct RegistrySink<'a> {
    registry: &'a mut ContentRegistry,
    bundle: &'a dyn BundleView,
    store: &'a dyn UnlockStore,
}

impl BlockSink for RegistrySink<'_> {
    fn push(&mut self, spec: BlockSpec) -> Result<(), ContentError> {
        let def = BlockDef::from_spec(spec, self.registry, self.bundle, self.store)?;
        self.registry.add_block(def)?;
        Ok(())
    }
}

/// Sim-facing block name/id table (`Sim.content`).
///
/// Built from the same generated waves as [`ContentRegistry`], so scenario block
/// names stay valid. Kept as a name table (not full records) so the P0 sim and
/// `mind-gdext` bindings do not depend on the metadata record layout.
#[derive(Debug, Clone, Default)]
pub struct Blocks {
    names: Vec<String>,
    by_name: IndexMap<String, u16>,
}

impl Blocks {
    /// Creates the vanilla table (all ported waves, upstream order).
    pub fn new() -> Self {
        let mut blocks = Self::default();
        load_names(&mut blocks);
        debug_assert!(blocks.assert_invariants().is_ok());
        blocks
    }

    /// Number of registered blocks.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the registry has no blocks (never true for `Blocks::new`).
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Name for an id (parity ABI).
    pub fn name(&self, id: BlockId) -> Result<&str, ContentError> {
        self.names
            .get(id.index())
            .map(String::as_str)
            .ok_or(ContentError::UnknownId(id.raw()))
    }

    /// Id for a name (parity ABI).
    pub fn id(&self, name: &str) -> Result<BlockId, ContentError> {
        self.by_name
            .get(name)
            .map(|id| BlockId::new(*id))
            .ok_or_else(|| ContentError::UnknownName(name.to_owned()))
    }

    /// Whether a name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Appends a block, returning its id. The name is the parity ABI and is
    /// rejected on a second registration.
    pub fn register(&mut self, name: &str) -> Result<BlockId, ContentError> {
        if self.by_name.contains_key(name) {
            return Err(ContentError::DuplicateName(name.to_owned()));
        }
        let raw = u16::try_from(self.names.len()).map_err(|_| ContentError::IdSpaceExhausted)?;
        self.names.push(name.to_owned());
        self.by_name.insert(name.to_owned(), raw);
        Ok(BlockId::new(raw))
    }

    /// Asserts the append-only invariants of the block table.
    pub fn assert_invariants(&self) -> Result<(), ContentError> {
        if self.name(BlockId::AIR)? != "air" {
            return Err(ContentError::UnknownName(String::from(
                "air must stay block id 0",
            )));
        }
        if self.name(BlockId::STONE_WALL)? != "stone-wall" {
            return Err(ContentError::UnknownName(String::from(
                "stone-wall must keep its upstream id",
            )));
        }
        if self.len() != self.by_name.len() {
            return Err(ContentError::DuplicateName(String::from(
                "registry has duplicate names",
            )));
        }
        for (idx, name) in self.names.iter().enumerate() {
            if self.by_name.get(name).copied() != Some(idx as u16) {
                return Err(ContentError::UnknownName(format!(
                    "name `{name}` maps to an inconsistent id"
                )));
            }
        }
        Ok(())
    }
}

/// Runs every ported block wave into `sink` in upstream order.
///
/// M3 loads `environment`/`ore`/`crafting`/`defense`; M4 appends the remaining
/// regions (`Blocks.java` region order).
pub fn load(sink: &mut dyn BlockSink) -> Result<(), ContentError> {
    environment::load(sink)?;
    ore::load(sink)?;
    crafting::load(sink)?;
    defense::load(sink)?;
    distribution::load(sink)?;
    liquid::load(sink)?;
    power::load(sink)?;
    production::load(sink)?;
    storage::load(sink)?;
    turrets::load(sink)?;
    units::load(sink)?;
    units_erekir::load(sink)?;
    payloads::load(sink)?;
    sandbox::load(sink)?;
    legacy::load(sink)?;
    campaign::load(sink)?;
    logic::load(sink)?;
    Ok(())
}

/// Builds the sim-facing name table from the generated waves.
fn load_names(blocks: &mut Blocks) {
    let mut sink = NamesSink { blocks };
    // Every wave is static vanilla data; registration cannot fail.
    let _ = load(&mut sink);
}

/// Loads blocks into a [`ContentRegistry`] (called by `create_base_content`).
pub(crate) fn load_into(
    registry: &mut ContentRegistry,
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
) -> Result<(), ContentError> {
    let mut sink = RegistrySink {
        registry,
        bundle,
        store,
    };
    load(&mut sink)
}

/// `Block.postInit` auto `shownPlanets` (requires the planets registry).
pub(crate) fn post_init_link(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    let planets: Vec<PlanetId> = registry
        .planets()
        .iter()
        .filter(|planet| !planet.sectors.is_empty())
        .map(|planet| planet.id)
        .collect();

    let mut assignments: Vec<(usize, Vec<PlanetId>)> = Vec::new();
    for (index, block) in registry.blocks().iter().enumerate() {
        if block.requirements.is_empty() || !block.unlock.shown_planets.is_empty() {
            continue;
        }
        let mut shown = Vec::new();
        for planet in &planets {
            let all = block.requirements.iter().all(|stack| {
                registry
                    .item(stack.item)
                    .map(|item| {
                        item.unlock.shown_planets.is_empty()
                            || item.unlock.shown_planets.contains(planet)
                    })
                    .unwrap_or(false)
            });
            if all {
                shown.push(*planet);
            }
        }
        if !shown.is_empty() {
            assignments.push((index, shown));
        }
    }

    for (index, shown) in assignments {
        if let Some(block) = registry.blocks_mut().get_mut(index) {
            for planet in shown {
                if !block.unlock.shown_planets.contains(&planet) {
                    block.unlock.shown_planets.push(planet);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    /// Plan 02 §7a: `blocks::construct_block_present` (`build2` exists).
    ///
    /// Ported from `tests/src/test/java/ApplicationTests.java` bootstrap
    /// assertion that `Blocks.build2` is registered.
    #[test]
    fn construct_block_present() {
        let registry = test_registry();
        let build2 = registry
            .block_by_name("build2")
            .expect("build2 must be registered");
        assert_eq!(build2.kind, BlockKind::ConstructBlock);
        assert_eq!(build2.size, 2);
        assert_eq!(build2.health, 10);
        assert!(!build2.in_editor);
        assert_eq!(build2.id, BlockId::new(6));
    }

    /// `stone-wall` keeps its upstream identity (`air = 0`, `stone-wall = 79`).
    #[test]
    fn stone_wall_keeps_upstream_id() {
        let registry = test_registry();
        let stone_wall = registry
            .block(BlockId::STONE_WALL)
            .expect("stone-wall id must resolve");
        assert_eq!(stone_wall.name, "stone-wall");
        assert_eq!(stone_wall.kind, BlockKind::StaticWall);
        assert!(stone_wall.solid);

        let blocks = Blocks::new();
        assert_eq!(blocks.id("stone-wall").unwrap(), BlockId::STONE_WALL);
        assert_eq!(blocks.name(BlockId::STONE_WALL).unwrap(), "stone-wall");
        assert_eq!(blocks.len(), 447, "M4 block count + M7 JVM-golden fix");
    }

    /// Plan 02 §7a: `blocks::health_and_buildtime_derivation` — `Block.init()`
    /// health/offset/build-time derivations (`world/Block.java:1357+`).
    #[test]
    fn health_and_buildtime_derivation() {
        let registry = test_registry();

        let press = registry.block_by_name("graphite-press").unwrap();
        // size 2, no healthScaling items -> round(2*2*40, 5) = 160.
        assert_eq!(press.size, 2);
        assert_eq!(press.health, 160);
        assert_eq!(press.offset, 4.0);
        assert_eq!(press.size_offset, 0);
        // buildTime = 75 * copper.cost + 30 * lead.cost (no multiplier).
        let copper = registry.item(registry.item_id("copper").unwrap()).unwrap();
        let lead = registry.item(registry.item_id("lead").unwrap()).unwrap();
        assert_eq!(press.build_time, 75.0 * copper.cost + 30.0 * lead.cost);
        assert_eq!(press.build_time, 58.5);

        // size 3 + carbide(1.1)/thorium(0.2)/tungsten(0.8) health scaling
        // -> 40 * 3.1 = 124 per tile -> round(9 * 124, 5) = 1115.
        let synth = registry.block_by_name("phase-synthesizer").unwrap();
        assert_eq!(synth.size, 3);
        assert!((synth.scaled_health - 124.0).abs() < 0.001);
        assert_eq!(synth.health, 1115);

        // Walls have explicit health and a 6x build time multiplier.
        let wall = registry.block_by_name("copper-wall").unwrap();
        assert_eq!(wall.health, 320);
        assert_eq!(wall.build_cost_multiplier, 6.0);
        assert_eq!(wall.build_time, 6.0 * copper.cost * 6.0);
        assert_eq!(wall.offset, 0.0);
        assert_eq!(wall.size_offset, 0);
    }

    /// Plan 02 §7a: `blocks::research_requirements_formula`
    /// (`Block.researchRequirements()`, `Block.java:1284-1295`).
    #[test]
    fn research_requirements_formula() {
        let registry = test_registry();
        let copper = registry.item_id("copper").unwrap();
        let lead = registry.item_id("lead").unwrap();

        let press = registry.block_by_name("graphite-press").unwrap();
        // Arc `Mathf.round` truncates: round(60 + 75^1.11*20, 10) = 2470,
        // then `UI.roundAmount` truncates to 2400 (JVM golden confirms).
        // round(60 + 30^1.11*20, 10) = 930 -> `roundAmount` 900.
        assert_eq!(
            press.research_requirements(),
            vec![ItemStack::new(copper, 2400), ItemStack::new(lead, 900)]
        );

        // Explicit `researchCost` overrides the formula (radar).
        let radar = registry.block_by_name("radar").unwrap();
        let silicon = registry.item_id("silicon").unwrap();
        let graphite = registry.item_id("graphite").unwrap();
        assert_eq!(
            radar.research_requirements(),
            vec![ItemStack::new(silicon, 70), ItemStack::new(graphite, 70)]
        );

        // `researchCostMultiplier <= 0` yields an empty list.
        let mut zeroed = press.clone();
        zeroed.research_cost = None;
        zeroed.research_cost_multiplier = 0.0;
        assert!(zeroed.research_requirements().is_empty());
    }

    /// Plan 02 §7a (M4 metadata half, exercised for M3 coverage):
    /// every ported block has valid metadata.
    #[test]
    fn all_metadata_valid() {
        let registry = test_registry();
        assert_eq!(registry.blocks().len(), 447);
        for block in registry.blocks() {
            assert!(block.health > 0, "{} has no health", block.name);
            assert!(
                (1..=16).contains(&block.size),
                "{} has invalid size {}",
                block.name,
                block.size
            );
            assert_eq!(
                block.offset,
                ((block.size + 1) % 2) as f32 * TILE_SIZE / 2.0,
                "{} offset",
                block.name
            );
            assert_eq!(block.size_offset, -((block.size - 1) / 2), "{}", block.name);
            assert!(block.build_time > 0.0, "{} build time", block.name);
            assert!(
                block.consumer_partitions_consistent(),
                "{} consumer partitions",
                block.name
            );
            assert!(!block.region.is_empty(), "{} region", block.name);
            assert!(!block.name.is_empty());
        }
    }

    /// Plan 02 §7a: `blocks::ergonomic_flags_match_golden` — behavior-facing
    /// flags/groups/priorities for representative B1–B6 blocks match the
    /// upstream constructors (source-derived golden; JVM golden pending NUD-10).
    #[test]
    fn ergonomic_flags_match_golden() {
        let registry = test_registry();
        let get = |name: &str| registry.block_by_name(name).expect("block present");

        let conveyor = get("conveyor");
        assert_eq!(conveyor.kind, BlockKind::Conveyor);
        assert_eq!(conveyor.group, BlockGroup::Transportation);
        assert!(conveyor.update && conveyor.has_items && !conveyor.solid);
        assert!(conveyor.accepts_items);
        assert_eq!(conveyor.priority, TARGET_PRIORITY_TRANSPORT);

        let router = get("router");
        assert_eq!(router.group, BlockGroup::Transportation);
        assert!(router.update && router.has_items && !router.solid);
        assert_eq!(router.item_capacity, 1);

        let sorter = get("sorter");
        assert!(!sorter.update && sorter.destructible && sorter.configurable && sorter.save_config);
        assert_eq!(sorter.group, BlockGroup::Transportation);

        let drill = get("mechanical-drill");
        assert!(drill.flags.contains(&BlockFlag::Drill));
        assert!(drill.update && drill.solid && drill.has_items && drill.has_liquids);
        assert_eq!(drill.group, BlockGroup::Drills);
        assert_eq!(drill.category, Category::Production);

        let generator = get("combustion-generator");
        assert!(generator.outputs_power && !generator.consumes_power && generator.has_power);
        assert!(generator.update && generator.solid);
        assert_eq!(generator.group, BlockGroup::Power);

        let battery = get("battery");
        assert!(battery.flags.contains(&BlockFlag::Battery));
        assert!(battery.outputs_power && battery.consumes_power && battery.has_power);
        assert!(battery.destructible && !battery.update);

        let core = get("core-shard");
        assert!(core.flags.contains(&BlockFlag::Core));
        assert_eq!(core.priority, TARGET_PRIORITY_CORE);
        assert_eq!(core.unit_cap_modifier, 8);
        assert!(core.solid && core.update && core.has_items);

        let duo = get("duo");
        assert!(duo.flags.contains(&BlockFlag::Turret));
        assert_eq!(duo.group, BlockGroup::Turrets);
        assert_eq!(duo.priority, TARGET_PRIORITY_TURRET);
        assert!(duo.has_items && duo.solid && duo.update);

        let factory = get("ground-factory");
        assert_eq!(factory.group, BlockGroup::Units);
        assert!(factory.update && factory.solid && factory.has_items && factory.has_power);
        assert!(factory.configurable);

        let payload = get("payload-conveyor");
        assert_eq!(payload.group, BlockGroup::Payloads);
        assert_eq!(payload.size, 3);
        assert!(payload.update);
        assert_eq!(payload.priority, TARGET_PRIORITY_TRANSPORT);

        let power_source = get("power-source");
        assert!(power_source.outputs_power && !power_source.consumes_power);
        assert_eq!(power_source.build_visibility, BuildVisibility::SandboxOnly);
        assert!(power_source.destructible && power_source.configurable);

        let processor = get("world-processor");
        assert_eq!(processor.group, BlockGroup::Logic);
        assert_eq!(
            processor.build_visibility,
            BuildVisibility::WorldProcessorOnly
        );
        assert!(processor.update && processor.solid && processor.configurable);

        let launch = get("launch-pad");
        assert!(launch.flags.contains(&BlockFlag::LaunchPad));
        assert!(launch.has_items && launch.solid && launch.update && launch.configurable);

        let message = get("message");
        assert_eq!(message.group, BlockGroup::Logic);
        assert!(message.solid && message.destructible && message.configurable);
    }

    /// Generated item references resolve and `get_dependencies` covers inputs.
    #[test]
    fn dependencies_and_items_resolve() {
        let registry = test_registry();
        let press = registry.block_by_name("graphite-press").unwrap();
        let deps = press.get_dependencies();
        assert!(deps.contains(&registry.item_id("copper").unwrap()));
        assert!(deps.contains(&registry.item_id("lead").unwrap()));
        assert!(deps.contains(&registry.item_id("coal").unwrap()));

        // Every requirement/drop/consume reference resolves to a real item.
        for block in registry.blocks() {
            for stack in &block.requirements {
                assert!(
                    registry.item(stack.item).is_some(),
                    "{} requirement",
                    block.name
                );
            }
            if let Some(drop) = block.item_drop {
                assert!(registry.item(drop).is_some(), "{} drop", block.name);
            }
        }

        // Ore blocks carry `itemDrop` and minable metadata (`OreBlock` ctor).
        let ore = registry.block_by_name("ore-copper").unwrap();
        assert_eq!(ore.kind, BlockKind::OreBlock);
        assert_eq!(ore.item_drop, registry.item_id("copper"));
        assert_eq!(ore.region, "ore-copper");
    }
}

// SPDX-License-Identifier: GPL-3.0-only

//! `BlockKind`: Java class tags for vanilla blocks and their classification predicates.
//! Upstream: core/src/mindustry/content/Blocks.java, core/src/mindustry/world/meta/ClassMap.java

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
    /// `mindustry.world.blocks.environment.TiledFloor` (`TiledFloor.java:14`):
    /// a `Floor` drawn in packed square tiles sized by `maxSize` (default 3).
    /// Mod-only class (`ClassMap.java:287`); no vanilla block instantiates it.
    TiledFloor = 137,
    /// `mindustry.world.blocks.environment.TiledWall` (`TiledWall.java:14`):
    /// a `StaticWall` drawn in packed square tiles sized by `maxSize` (default
    /// 3). Mod-only class (`ClassMap.java:288`).
    TiledWall = 138,
    /// `mindustry.world.blocks.environment.WobbleProp` (`WobbleProp.java:8`):
    /// a `Prop` whose region vertices wobble (`wscl`/`wmag`/`wtscl`/`wmag2`).
    /// Mod-only class (`ClassMap.java:290`).
    WobbleProp = 139,
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
        BlockKind::TiledFloor,
        BlockKind::TiledWall,
        BlockKind::WobbleProp,
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
            BlockKind::TiledFloor => "TiledFloor",
            BlockKind::TiledWall => "TiledWall",
            BlockKind::WobbleProp => "WobbleProp",
        }
    }

    /// Whether the block's Java class extends `PayloadBlock`
    /// (`J/world/blocks/payloads/PayloadBlock.java:17`): `UnitBlock` (unit
    /// factories and reconstructors), `UnitAssembler`(+`Module`), the payload
    /// mass driver, deconstructor, constructor, loader, unloader, source and
    /// void. `PayloadConveyor`/`PayloadRouter` extend `Block`, not
    /// `PayloadBlock`, so they are excluded. Read by the `factorycontrol` hint
    /// (`HintsFragment.java:91-95`, `BuildingCommandEvent`).
    pub const fn is_payload_block(self) -> bool {
        matches!(
            self,
            BlockKind::UnitFactory
                | BlockKind::Reconstructor
                | BlockKind::UnitAssembler
                | BlockKind::UnitAssemblerModule
                | BlockKind::PayloadMassDriver
                | BlockKind::PayloadDeconstructor
                | BlockKind::Constructor
                | BlockKind::PayloadLoader
                | BlockKind::PayloadUnloader
                | BlockKind::PayloadSource
                | BlockKind::PayloadVoid
        )
    }
}

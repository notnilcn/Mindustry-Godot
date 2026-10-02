// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-family block definition data (`BlockKindData`).
//!
//! Ported from the per-class configuration fields of
//! `core/src/mindustry/world/blocks/**` (`GenericCrafter.craftTime`,
//! `Drill.drillTime`, `Wall.buildCostMultiplier`, …). Plan 02 owns the JSON
//! metadata record ([`super::super::content::BlockDef`]); this enum carries the
//! typed family knobs plan 07's behavior code consumes.
//!
//! ## Ownership note (plan 07 §8 R2)
//!
//! Plan 07 §6.1 sketches `BlockKindData` as an additive field on
//! plan-02's `BlockDef`. Plan 02's `BlockSpec`/`BlockDef` does not yet expose the
//! family knob fields, so this pass derives a `BlockKindData` from the existing
//! metadata via [`BlockKindData::from_def`]. When plan 02 grows the
//! registration-wave knob fields the derivation is replaced by a copy and no
//! behavior code changes. The enum shape itself is the frozen interface.

use crate::content::{BlockDef, BlockKind};

/// Coarse family grouping used for behavior dispatch and dumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlockFamily {
    /// `Floor`/`OverlayFloor`/`OreBlock`/`StaticWall`/`Prop`/`Cliff`/… — no building.
    Environment,
    /// `Wall`/`Door`/`AutoDoor`/`Radar`/`Thruster`/`TargetDummy`.
    Defense,
    /// `GenericCrafter`/`Drill`/`Pump`/`Separator`/… .
    Production,
    /// `ItemSource`/`ItemVoid`/`LiquidSource`/`LiquidVoid`/`PowerSource`/`PowerVoid`.
    Sandbox,
    /// `Accelerator`/`LandingPad`/`LaunchPad` (building half; campaign wiring is 12).
    Campaign,
    /// `LegacyBlock` + subclasses (removed at `World.endMapLoad`).
    Legacy,
    /// `ConstructBlock` size singletons.
    Construct,
    /// Logic family (`LogicBlock`/`MemoryBlock`/`SwitchBlock`; plan 13).
    Logic,
    /// Families owned by other plans (08/09/10/11) or a plain `Block`.
    #[default]
    Other,
}

/// `GenericCrafter` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CrafterDef {
    /// `GenericCrafter.craftTime` (ticks).
    pub craft_time: f32,
    /// `outputItems` stacks (`(item, amount)`).
    pub output_items: Vec<(u16, i32)>,
    /// `outputLiquids` stacks (`(liquid, amount)`).
    pub output_liquids: Vec<(u16, f32)>,
    /// `ignoreLiquidFullness`.
    pub ignore_liquid_fullness: bool,
}

/// `AttributeCrafter` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AttributeCrafterDef {
    /// Base `GenericCrafter` knobs (`AttributeCrafter extends GenericCrafter`).
    pub crafter: CrafterDef,
    /// Attribute index boosted, `-1` = none.
    pub attribute: i16,
    /// `baseEfficiency`.
    pub base_efficiency: f32,
    /// `boostScale`.
    pub boost_scale: f32,
    /// `minEfficiency`/`maxBoost`.
    pub min_efficiency: f32,
    /// `maxBoost`.
    pub max_boost: f32,
}

/// `Drill` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrillDef {
    /// `Drill.drillTime` (ticks per item).
    pub drill_time: f32,
    /// Hardness tier required.
    pub tier: i32,
    /// `hardnessDrillMultiplier`.
    pub hardness_drill_multiplier: f32,
    /// Blocked items (`blockedItems`).
    pub blocked_items: Vec<u16>,
    /// Whether this is a wall drill (`Drill.wallDrill`).
    pub wall_drill: bool,
}

/// `BurstDrill` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BurstDrillDef {
    /// Base drill knobs.
    pub drill: DrillDef,
    /// `BurstDrill.burstTime`.
    pub burst_time: f32,
    /// `BurstDrill.invertedTime`.
    pub inverted_time: f32,
    /// `BurstDrill.drillMultiplier`.
    pub drill_multiplier: f32,
}

/// `BeamDrill` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BeamDrillDef {
    /// `BeamDrill.range`.
    pub range: i32,
    /// `BeamDrill.drillTime`.
    pub drill_time: f32,
    /// `BeamDrill.tier`.
    pub tier: i32,
    /// `BeamDrill.blockedItems`.
    pub blocked_items: Vec<u16>,
}

/// `Pump` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PumpDef {
    /// `Pump.pumpAmount`.
    pub pump_amount: f32,
}

/// `SolidPump` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SolidPumpDef {
    /// `SolidPump.pumpAmount`.
    pub pump_amount: f32,
    /// `SolidPump.attribute` index, `-1` = none.
    pub attribute: i16,
    /// `SolidPump.baseEfficiency`.
    pub base_efficiency: f32,
}

/// `Fracker` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrackerDef {
    /// `Fracker.itemUseTime`.
    pub item_use_time: f32,
    /// `SolidPump.pumpAmount`.
    pub pump_amount: f32,
    /// Resulting liquid, if any.
    pub result: Option<u16>,
    /// `Fracker.attribute` index, `-1` = none.
    pub attribute: i16,
}

/// `WallCrafter` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WallCrafterDef {
    /// `WallCrafter.drillTime`.
    pub drill_time: f32,
    /// `WallCrafter.attribute` index, `-1` = none.
    pub attribute: i16,
    /// Item output (`WallCrafter.output`).
    pub output: Option<u16>,
}

/// `Separator` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeparatorDef {
    /// `Separator.craftTime`.
    pub craft_time: f32,
    /// `Separator.results` (`(item, amount)`).
    pub results: Vec<(u16, i32)>,
}

/// `Incinerator` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IncineratorDef {
    /// Power usage (0 = none).
    pub power_usage: f32,
}

/// `Wall` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WallDef {
    /// `Wall.autotile`.
    pub autotile: bool,
    /// `Wall.baseExplosiveness`/`lightningChance` etc. are 10-hook data.
    pub lightning_chance: f32,
    /// `Wall.lightningDamage`.
    pub lightning_damage: f32,
    /// `Wall.lightningLength`.
    pub lightning_length: i32,
}

/// `Door`/`AutoDoor` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DoorDef {
    /// `Door.openTime`.
    pub open_time: f32,
    /// `Door.closeTime`.
    pub close_time: f32,
}

/// `Radar` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RadarDef {
    /// `Radar.discoveryTime`.
    pub discovery_time: f32,
}

/// `Thruster` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ThrusterDef {
    /// `Thruster.force`.
    pub force: f32,
}

/// `TargetDummy` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TargetDummyDef {
    /// `TargetDummy.cooldown`.
    pub cooldown: f32,
}

/// `Floor` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FloorDef {
    /// Blend group index (`Floor.blendGroup`).
    pub blend_group: i32,
    /// Whether this is a deep liquid (`Floor.isDeep`).
    pub is_deep: bool,
}

/// `OverlayFloor` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OverlayDef {
    /// Whether the overlay can be placed on walls (`OverlayFloor.wallOre`).
    pub wall_ore: bool,
}

/// `OreBlock` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OreDef {
    /// Dropped item.
    pub item_drop: Option<u16>,
    /// Ore display scale.
    pub ore_scale: f32,
}

/// `StaticWall` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StaticWallDef {
    /// Whether the 2×2 large variant applies.
    pub large: bool,
}

/// `Prop`/`StaticProp` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PropDef {
    /// Draw layer (`Prop.layer`).
    pub layer: f32,
}

/// `Cliff` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CliffDef {
    /// Editor variant count.
    pub variants: u8,
}

/// `ShallowLiquid` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShallowLiquidDef {
    /// Whether the liquid is shallow (always true for this class).
    pub shallow: bool,
}

/// `SteamVent` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SteamVentDef {
    /// `SteamVent.effectInterval`.
    pub effect_interval: f32,
}

/// `SpawnBlock` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpawnDef {
    /// Team the spawn belongs to (`-1` = neutral).
    pub team: i16,
}

/// Sandbox item-source/sink knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SandboxItemDef {
    /// Items per second (`ItemSource.itemsPerSecond`).
    pub items_per_second: f32,
}

/// Sandbox liquid-source/sink knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SandboxLiquidDef {
    /// `LiquidSource.sourceAmount`.
    pub source_amount: f32,
}

/// Sandbox power-source/sink knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SandboxPowerDef {
    /// `PowerSource.powerProduction`.
    pub power_production: f32,
}

/// `Accelerator` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AcceleratorDef {
    /// `Accelerator.heatTime`.
    pub heat_time: f32,
}

/// `LandingPad` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LandingPadDef {
    /// Item capacity for the landing inventory.
    pub item_capacity: i32,
}

/// `LaunchPad` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LaunchPadDef {
    /// `LaunchPad.launchTime`.
    pub launch_time: f32,
}

/// `LegacyBlock` marker data (replacement content id, when defined).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LegacyDef {
    /// Replacement block id, if any.
    pub replacement: Option<u16>,
}

/// `LogicBlock` family knobs (plan 13 §3.6).
#[derive(Debug, Clone, PartialEq)]
pub struct LogicBlockDef {
    /// `LogicBlock.instructionsPerTick`.
    pub ipt: i32,
    /// `LogicBlock.maxInstructionsPerTick` (privileged only).
    pub max_ipt: i32,
    /// `LogicBlock.range` (world units).
    pub range: f32,
    /// `LogicBlock.privileged` (world processor).
    pub privileged: bool,
}

impl Default for LogicBlockDef {
    fn default() -> Self {
        Self {
            ipt: 1,
            max_ipt: 40,
            range: 8.0 * 10.0,
            privileged: false,
        }
    }
}

/// `MemoryBlock` family knobs (`MemoryBlock.memoryCapacity`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MemoryDef {
    /// `MemoryBlock.memoryCapacity`.
    pub capacity: i32,
    /// Whether this is the privileged `world-cell`.
    pub privileged: bool,
}

/// `SwitchBlock` family data (privileged flag for `world-switch`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SwitchDef {
    /// Whether this is the privileged `world-switch`.
    pub privileged: bool,
}

/// `MessageBlock` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MessageDef {
    /// `MessageBlock.maxTextLength`.
    pub max_text: i32,
    /// `MessageBlock.maxNewlines`.
    pub max_newlines: i32,
    /// Whether this is the privileged `world-message`.
    pub privileged: bool,
}

/// `LogicDisplay` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisplayDef {
    /// `LogicDisplay.displaySize`.
    pub display_size: i32,
    /// `LogicDisplay.scaleFactor`.
    pub scale_factor: f32,
}

/// `TileableLogicDisplay` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TileableDisplayDef {
    /// Per-tile `displaySize`.
    pub display_size: i32,
    /// `TileableLogicDisplay.frameSize`.
    pub frame_size: i32,
    /// `TileableLogicDisplay.maxDisplayDimensions`.
    pub max_dimensions: i32,
}

/// `CanvasBlock` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CanvasDef {
    /// `CanvasBlock.canvasSize`.
    pub canvas_size: i32,
    /// `CanvasBlock.padding`.
    pub padding: f32,
    /// `CanvasBlock.bitsPerPixel`.
    pub bits_per_pixel: u8,
}

/// Typed family data for one block (plan 07 §6.1).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum BlockKindData {
    /// `ConstructBlock` size singleton.
    Construct {
        /// Size in tiles.
        size: u8,
    },
    /// `Floor`.
    Floor(FloorDef),
    /// `OverlayFloor`.
    OverlayFloor(OverlayDef),
    /// `OreBlock`.
    Ore(OreDef),
    /// `StaticWall`.
    StaticWall(StaticWallDef),
    /// `Prop`.
    Prop(PropDef),
    /// `Cliff`.
    Cliff(CliffDef),
    /// `ShallowLiquid`.
    ShallowLiquid(ShallowLiquidDef),
    /// `SteamVent`.
    SteamVent(SteamVentDef),
    /// `SpawnBlock`.
    Spawn(SpawnDef),
    /// `Wall`.
    Wall(WallDef),
    /// `Door`/`AutoDoor`.
    Door(DoorDef),
    /// `Radar`.
    Radar(RadarDef),
    /// `Thruster`.
    Thruster(ThrusterDef),
    /// `TargetDummy`.
    TargetDummy(TargetDummyDef),
    /// `GenericCrafter`/`HeatCrafter`.
    Crafter(CrafterDef),
    /// `AttributeCrafter`.
    AttributeCrafter(AttributeCrafterDef),
    /// `Drill`.
    Drill(DrillDef),
    /// `BurstDrill`.
    BurstDrill(BurstDrillDef),
    /// `BeamDrill`.
    BeamDrill(BeamDrillDef),
    /// `Pump`.
    Pump(PumpDef),
    /// `SolidPump`.
    SolidPump(SolidPumpDef),
    /// `Fracker`.
    Fracker(FrackerDef),
    /// `WallCrafter`.
    WallCrafter(WallCrafterDef),
    /// `Separator`.
    Separator(SeparatorDef),
    /// `Incinerator`/`ItemIncinerator`.
    Incinerator(IncineratorDef),
    /// `ItemSource`.
    ItemSource(SandboxItemDef),
    /// `ItemVoid`.
    ItemVoid,
    /// `LiquidSource`.
    LiquidSource(SandboxLiquidDef),
    /// `LiquidVoid`.
    LiquidVoid,
    /// `PowerSource`.
    PowerSource(SandboxPowerDef),
    /// `PowerVoid`.
    PowerVoid,
    /// `Accelerator`.
    Accelerator(AcceleratorDef),
    /// `LandingPad`.
    LandingPad(LandingPadDef),
    /// `LaunchPad`.
    LaunchPad(LaunchPadDef),
    /// `LegacyBlock` + subclasses.
    Legacy(LegacyDef),
    /// `LogicBlock` (plan 13).
    Logic(LogicBlockDef),
    /// `MemoryBlock` (plan 13).
    Memory(MemoryDef),
    /// `SwitchBlock` (plan 13).
    Switch(SwitchDef),
    /// `MessageBlock` (plan 13).
    Message(MessageDef),
    /// `LogicDisplay` (plan 13).
    Display(DisplayDef),
    /// `TileableLogicDisplay` (plan 13).
    TileableDisplay(TileableDisplayDef),
    /// `CanvasBlock` (plan 13).
    Canvas(CanvasDef),
    /// A plain `Block`/family owned by another plan.
    #[default]
    None,
}

impl BlockKindData {
    /// Derives the typed data from an existing metadata record.
    ///
    /// Plan 02 does not yet expose per-family knob fields (see the module note),
    /// so family defaults are produced here. Values sourced from `BlockDef`
    /// (e.g. `item_drop`) are copied where the metadata already carries them.
    pub fn from_def(def: &BlockDef) -> Self {
        match def.kind {
            BlockKind::ConstructBlock => BlockKindData::Construct {
                size: def.size.clamp(1, 16) as u8,
            },
            BlockKind::Floor | BlockKind::EmptyFloor | BlockKind::ColoredFloor => {
                BlockKindData::Floor(FloorDef::default())
            }
            BlockKind::OverlayFloor | BlockKind::CharacterOverlay | BlockKind::RuneOverlay => {
                BlockKindData::OverlayFloor(OverlayDef {
                    wall_ore: def.wall_ore,
                })
            }
            BlockKind::OreBlock => BlockKindData::Ore(OreDef {
                item_drop: def.item_drop.map(|item| item.raw()),
                ore_scale: def.ore_scale,
            }),
            BlockKind::StaticWall | BlockKind::ColoredWall => {
                BlockKindData::StaticWall(StaticWallDef {
                    large: def.size >= 2,
                })
            }
            BlockKind::Prop
            | BlockKind::StaticProp
            | BlockKind::StaticTree
            | BlockKind::TreeBlock => BlockKindData::Prop(PropDef::default()),
            BlockKind::Cliff | BlockKind::TallBlock => BlockKindData::Cliff(CliffDef::default()),
            BlockKind::ShallowLiquid => {
                BlockKindData::ShallowLiquid(ShallowLiquidDef { shallow: true })
            }
            BlockKind::SteamVent => BlockKindData::SteamVent(SteamVentDef::default()),
            BlockKind::SpawnBlock => BlockKindData::Spawn(SpawnDef::default()),
            BlockKind::Wall | BlockKind::ShieldWall => BlockKindData::Wall(WallDef::default()),
            BlockKind::Door | BlockKind::AutoDoor => BlockKindData::Door(DoorDef::default()),
            BlockKind::Radar => BlockKindData::Radar(RadarDef::default()),
            BlockKind::Thruster => BlockKindData::Thruster(ThrusterDef::default()),
            BlockKind::TargetDummy => BlockKindData::TargetDummy(TargetDummyDef::default()),
            BlockKind::GenericCrafter | BlockKind::HeatCrafter => {
                BlockKindData::Crafter(CrafterDef::default())
            }
            BlockKind::AttributeCrafter => {
                BlockKindData::AttributeCrafter(AttributeCrafterDef::default())
            }
            BlockKind::Drill => BlockKindData::Drill(DrillDef::default()),
            BlockKind::BurstDrill => BlockKindData::BurstDrill(BurstDrillDef::default()),
            BlockKind::BeamDrill => BlockKindData::BeamDrill(BeamDrillDef::default()),
            BlockKind::Pump => BlockKindData::Pump(PumpDef::default()),
            BlockKind::SolidPump => BlockKindData::SolidPump(SolidPumpDef::default()),
            BlockKind::Fracker => BlockKindData::Fracker(FrackerDef::default()),
            BlockKind::WallCrafter => BlockKindData::WallCrafter(WallCrafterDef::default()),
            BlockKind::Separator => BlockKindData::Separator(SeparatorDef::default()),
            BlockKind::Incinerator | BlockKind::ItemIncinerator => {
                BlockKindData::Incinerator(IncineratorDef::default())
            }
            BlockKind::ItemSource => BlockKindData::ItemSource(SandboxItemDef::default()),
            BlockKind::ItemVoid => BlockKindData::ItemVoid,
            BlockKind::LiquidSource => BlockKindData::LiquidSource(SandboxLiquidDef::default()),
            BlockKind::LiquidVoid => BlockKindData::LiquidVoid,
            BlockKind::PowerSource => BlockKindData::PowerSource(SandboxPowerDef::default()),
            BlockKind::PowerVoid => BlockKindData::PowerVoid,
            BlockKind::Accelerator => BlockKindData::Accelerator(AcceleratorDef::default()),
            BlockKind::LandingPad => BlockKindData::LandingPad(LandingPadDef::default()),
            BlockKind::LaunchPad => BlockKindData::LaunchPad(LaunchPadDef::default()),
            BlockKind::LegacyMechPad
            | BlockKind::LegacyUnitFactory
            | BlockKind::LegacyCommandCenter => BlockKindData::Legacy(LegacyDef::default()),
            BlockKind::LogicBlock => BlockKindData::Logic(LogicBlockDef::default()),
            BlockKind::MemoryBlock => BlockKindData::Memory(MemoryDef::default()),
            BlockKind::SwitchBlock => BlockKindData::Switch(SwitchDef::default()),
            BlockKind::MessageBlock => BlockKindData::Message(MessageDef::default()),
            BlockKind::LogicDisplay => BlockKindData::Display(DisplayDef::default()),
            BlockKind::TileableLogicDisplay => {
                BlockKindData::TileableDisplay(TileableDisplayDef::default())
            }
            BlockKind::CanvasBlock => BlockKindData::Canvas(CanvasDef::default()),
            _ => BlockKindData::None,
        }
    }

    /// Applies 07-owned interim vanilla family knobs.
    ///
    /// Plan 02's generated `BlockSpec`/`BlockDef` does not yet carry the
    /// per-family knob fields (plan 07 §8 R2, orchestrator action). Until it
    /// does, this table supplies the numeric knobs for the vanilla blocks this
    /// plan's behaviors and scenarios exercise. It is **07-owned** and does not
    /// mutate plan-02 metadata; when `BlockDef` grows the fields, this becomes a
    /// no-op and behavior code is unchanged.
    pub fn apply_vanilla_knobs(&mut self, content: &crate::content::ContentRegistry, name: &str) {
        let item = |n: &str| content.item_id(n).map(|id| id.raw()).unwrap_or(0);
        let liquid = |n: &str| content.liquid_id(n).map(|id| id.raw()).unwrap_or(0);
        let craft = |craft_time: f32,
                     items: Vec<(u16, i32)>,
                     liquids: Vec<(u16, f32)>,
                     ignore: bool| CrafterDef {
            craft_time,
            output_items: items,
            output_liquids: liquids,
            ignore_liquid_fullness: ignore,
        };
        match name {
            "silicon-smelter" => {
                self.set_crafter(craft(40.0, vec![(item("silicon"), 1)], vec![], false))
            }
            "surge-smelter" => {
                self.set_crafter(craft(75.0, vec![(item("surge-alloy"), 1)], vec![], false))
            }
            "spore-press" => self.set_crafter(craft(
                20.0,
                vec![],
                vec![(liquid("oil"), 18.0 / 60.0)],
                false,
            )),
            "coal-centrifuge" => {
                self.set_crafter(craft(30.0, vec![(item("coal"), 1)], vec![], false))
            }
            "kiln" => self.set_crafter(craft(30.0, vec![(item("metaglass"), 1)], vec![], false)),
            "melter" => self.set_crafter(craft(
                10.0,
                vec![],
                vec![(liquid("slag"), 12.0 / 60.0)],
                false,
            )),
            "pulverizer" => self.set_crafter(craft(40.0, vec![(item("sand"), 1)], vec![], false)),
            "separator" => {
                if let BlockKindData::Separator(def) = self {
                    def.craft_time = 35.0;
                    def.results = vec![
                        (item("copper"), 5),
                        (item("lead"), 3),
                        (item("graphite"), 2),
                        (item("titanium"), 2),
                    ];
                }
            }
            "disassembler" => {
                if let BlockKindData::Separator(def) = self {
                    def.craft_time = 15.0;
                    def.results = vec![
                        (item("sand"), 2),
                        (item("graphite"), 1),
                        (item("titanium"), 1),
                        (item("thorium"), 1),
                    ];
                }
            }
            "mechanical-drill" => self.set_drill(600.0, 2, None),
            "pneumatic-drill" => self.set_drill(400.0, 3, None),
            "laser-drill" => self.set_drill(280.0, 4, None),
            "blast-drill" => self.set_drill(280.0, 5, None),
            "impact-drill" => {
                if let BlockKindData::BurstDrill(def) = self {
                    def.drill.drill_time = 720.0;
                    def.drill.tier = 6;
                    def.drill.blocked_items = vec![item("thorium")];
                    def.drill_multiplier = 1.0;
                    def.burst_time = 60.0 * 5.0;
                }
            }
            "plasma-bore" => {
                if let BlockKindData::BeamDrill(def) = self {
                    def.range = 5;
                    def.drill_time = 160.0;
                    def.tier = 3;
                }
            }
            "large-plasma-bore" => {
                if let BlockKindData::BeamDrill(def) = self {
                    def.range = 6;
                    def.drill_time = 100.0;
                    def.tier = 5;
                }
            }
            "water-extractor" => {
                if let BlockKindData::SolidPump(def) = self {
                    def.pump_amount = 0.11;
                    def.attribute = -1;
                    def.base_efficiency = 1.0;
                }
            }
            "oil-extractor" => {
                if let BlockKindData::Fracker(def) = self {
                    def.pump_amount = 0.25;
                    def.item_use_time = 60.0;
                    def.result = Some(liquid("oil"));
                    def.attribute = -1;
                }
            }
            "cultivator" => {
                if let BlockKindData::AttributeCrafter(def) = self {
                    def.crafter = craft(100.0, vec![(item("spore-pod"), 1)], vec![], false);
                    def.attribute = -1;
                    def.base_efficiency = 0.0;
                    def.max_boost = 2.0;
                    def.min_efficiency = 0.0;
                }
            }
            "vent-condenser" => {
                if let BlockKindData::AttributeCrafter(def) = self {
                    def.crafter = craft(120.0, vec![], vec![(liquid("water"), 30.0 / 60.0)], false);
                    def.attribute = -1;
                    def.base_efficiency = 0.0;
                    def.min_efficiency = 9.0 - 0.0001;
                    def.boost_scale = 1.0 / 9.0;
                }
            }
            "cliff-crusher" => self.set_wall_crafter(110.0, Some(item("sand"))),
            "large-cliff-crusher" => self.set_wall_crafter(48.0, Some(item("sand"))),
            "incinerator" => {
                if let BlockKindData::Incinerator(def) = self {
                    def.power_usage = 0.5;
                }
            }
            // Plan 13 §3.6: `Blocks.java` logic region values.
            "micro-processor" => self.set_logic(2, 40, 8.0 * 10.0, false),
            "logic-processor" => self.set_logic(8, 40, 8.0 * 22.0, false),
            "hyper-processor" => self.set_logic(25, 40, 8.0 * 42.0, false),
            "world-processor" => self.set_logic(8, 1000, f32::MAX, true),
            "memory-cell" => self.set_memory(64, false),
            "memory-bank" => self.set_memory(512, false),
            "world-cell" => self.set_memory(512, true),
            "world-switch" => {
                if let BlockKindData::Switch(def) = self {
                    def.privileged = true;
                }
            }
            "message" | "reinforced-message" => self.set_message(400, 24, false),
            "world-message" => self.set_message(400, 24, true),
            "logic-display" => self.set_display(80, 1.0),
            "large-logic-display" => self.set_display(176, 1.0),
            "tile-logic-display" => {
                if let BlockKindData::TileableDisplay(def) = self {
                    def.display_size = 32;
                    def.frame_size = 6;
                    def.max_dimensions = 16;
                }
            }
            "canvas" => self.set_canvas(12, 3.5, 3),
            "large-canvas" => self.set_canvas(24, 3.5, 4),
            _ => {}
        }
    }

    fn set_crafter(&mut self, crafter: CrafterDef) {
        match self {
            BlockKindData::Crafter(def) => *def = crafter,
            BlockKindData::AttributeCrafter(def) => def.crafter = crafter,
            _ => {}
        }
    }

    fn set_drill(&mut self, drill_time: f32, tier: i32, blocked: Option<u16>) {
        let apply = |def: &mut DrillDef| {
            def.drill_time = drill_time;
            def.tier = tier;
            if let Some(item) = blocked {
                def.blocked_items.push(item);
            }
        };
        match self {
            BlockKindData::Drill(def) => apply(def),
            BlockKindData::BurstDrill(def) => apply(&mut def.drill),
            _ => {}
        }
    }

    fn set_wall_crafter(&mut self, drill_time: f32, output: Option<u16>) {
        if let BlockKindData::WallCrafter(def) = self {
            def.drill_time = drill_time;
            def.output = output;
            def.attribute = -1;
        }
    }

    fn set_logic(&mut self, ipt: i32, max_ipt: i32, range: f32, privileged: bool) {
        if let BlockKindData::Logic(def) = self {
            def.ipt = ipt;
            def.max_ipt = max_ipt;
            def.range = range;
            def.privileged = privileged;
        }
    }

    fn set_memory(&mut self, capacity: i32, privileged: bool) {
        if let BlockKindData::Memory(def) = self {
            def.capacity = capacity;
            def.privileged = privileged;
        }
    }

    fn set_message(&mut self, max_text: i32, max_newlines: i32, privileged: bool) {
        if let BlockKindData::Message(def) = self {
            def.max_text = max_text;
            def.max_newlines = max_newlines;
            def.privileged = privileged;
        }
    }

    fn set_display(&mut self, display_size: i32, scale_factor: f32) {
        if let BlockKindData::Display(def) = self {
            def.display_size = display_size;
            def.scale_factor = scale_factor;
        }
    }

    fn set_canvas(&mut self, canvas_size: i32, padding: f32, bits_per_pixel: u8) {
        if let BlockKindData::Canvas(def) = self {
            def.canvas_size = canvas_size;
            def.padding = padding;
            def.bits_per_pixel = bits_per_pixel;
        }
    }

    /// Coarse family for dispatch/dumps.
    pub fn family(&self) -> BlockFamily {
        match self {
            BlockKindData::Construct { .. } => BlockFamily::Construct,
            BlockKindData::Floor(_)
            | BlockKindData::OverlayFloor(_)
            | BlockKindData::Ore(_)
            | BlockKindData::StaticWall(_)
            | BlockKindData::Prop(_)
            | BlockKindData::Cliff(_)
            | BlockKindData::ShallowLiquid(_)
            | BlockKindData::SteamVent(_)
            | BlockKindData::Spawn(_) => BlockFamily::Environment,
            BlockKindData::Wall(_)
            | BlockKindData::Door(_)
            | BlockKindData::Radar(_)
            | BlockKindData::Thruster(_)
            | BlockKindData::TargetDummy(_) => BlockFamily::Defense,
            BlockKindData::Crafter(_)
            | BlockKindData::AttributeCrafter(_)
            | BlockKindData::Drill(_)
            | BlockKindData::BurstDrill(_)
            | BlockKindData::BeamDrill(_)
            | BlockKindData::Pump(_)
            | BlockKindData::SolidPump(_)
            | BlockKindData::Fracker(_)
            | BlockKindData::WallCrafter(_)
            | BlockKindData::Separator(_)
            | BlockKindData::Incinerator(_) => BlockFamily::Production,
            BlockKindData::ItemSource(_)
            | BlockKindData::ItemVoid
            | BlockKindData::LiquidSource(_)
            | BlockKindData::LiquidVoid
            | BlockKindData::PowerSource(_)
            | BlockKindData::PowerVoid => BlockFamily::Sandbox,
            BlockKindData::Accelerator(_)
            | BlockKindData::LandingPad(_)
            | BlockKindData::LaunchPad(_) => BlockFamily::Campaign,
            BlockKindData::Legacy(_) => BlockFamily::Legacy,
            BlockKindData::Logic(_)
            | BlockKindData::Memory(_)
            | BlockKindData::Switch(_)
            | BlockKindData::Message(_)
            | BlockKindData::Display(_)
            | BlockKindData::TileableDisplay(_)
            | BlockKindData::Canvas(_) => BlockFamily::Logic,
            BlockKindData::None => BlockFamily::Other,
        }
    }

    /// Parity family tag for the `blocks audit` dump.
    pub fn family_name(&self) -> &'static str {
        match self.family() {
            BlockFamily::Environment => "environment",
            BlockFamily::Defense => "defense",
            BlockFamily::Production => "production",
            BlockFamily::Sandbox => "sandbox",
            BlockFamily::Campaign => "campaign",
            BlockFamily::Legacy => "legacy",
            BlockFamily::Construct => "construct",
            BlockFamily::Logic => "logic",
            BlockFamily::Other => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    #[test]
    fn family_mapping_matches_owned_kinds() {
        let registry = test_registry();
        let copper_wall = registry
            .block_id("copper-wall")
            .and_then(|id| registry.block(id))
            .expect("copper-wall");
        assert_eq!(
            BlockKindData::from_def(copper_wall).family(),
            BlockFamily::Defense
        );
        let stone = registry
            .block_id("stone")
            .and_then(|id| registry.block(id))
            .expect("stone");
        assert_eq!(
            BlockKindData::from_def(stone).family(),
            BlockFamily::Environment
        );
        // Construct singletons are registered as `build1`..`build16`.
        let build2 = registry
            .block_id("build2")
            .and_then(|id| registry.block(id))
            .expect("build2");
        assert_eq!(
            BlockKindData::from_def(build2),
            BlockKindData::Construct { size: 2 }
        );
    }
}

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
            _ => BlockKindData::None,
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

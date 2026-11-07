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
//! plan-02's `BlockDef`. Plan 02's `BlockSpec`/`BlockDef` now exposes the
//! floor/environment and placement/crush knob fields (plan 02 §6.1 addendum,
//! `lane/f27-floor-meta`), so [`BlockKindData::from_def`] copies those real
//! fields. The production family knobs (crafter outputs, drill time/tier/
//! hardness/blocked items/boosts, pump amount/item time/result/attribute,
//! wall-crafter boosts, attribute-crafter scaling) and the
//! `TiledFloor`/`TiledWall`/`WobbleProp` environment knobs are content fields
//! too (GAPS #56 follow-up), so [`BlockKindData::from_def`] reads them and only
//! falls back to the upstream class field initializer when the content left
//! them unset. The 07-owned [`BlockKindData::apply_vanilla_knobs`] overlay is
//! now the migration-era fallback for `Blocks.java` per-block recipe values
//! that the generated vanilla waves have not moved onto `BlockSpec` yet.
//! The enum shape itself is the frozen interface.

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
#[derive(Debug, Clone, PartialEq)]
pub struct CrafterDef {
    /// `GenericCrafter.craftTime` (ticks).
    pub craft_time: f32,
    /// `outputItems` stacks (`(item, amount)`).
    pub output_items: Vec<(u16, i32)>,
    /// `outputLiquids` stacks (`(liquid, amount)`).
    pub output_liquids: Vec<(u16, f32)>,
    /// `GenericCrafter.liquidOutputDirections` (`GenericCrafter.java:35`):
    /// per-output relative directions (`-1` = all sides; electrolyzer `{1, 3}`,
    /// `Blocks.java:1397`).
    pub liquid_output_directions: Vec<i8>,
    /// `ignoreLiquidFullness`.
    pub ignore_liquid_fullness: bool,
}

impl Default for CrafterDef {
    fn default() -> Self {
        // `GenericCrafter.java:35-39` field initializers (the class defaults a
        // mod block inherits when its JSON omits them).
        Self {
            craft_time: 80.0,
            output_items: Vec::new(),
            output_liquids: Vec::new(),
            liquid_output_directions: vec![-1],
            ignore_liquid_fullness: false,
        }
    }
}

/// `AttributeCrafter` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeCrafterDef {
    /// Base `GenericCrafter` knobs (`AttributeCrafter extends GenericCrafter`).
    pub crafter: CrafterDef,
    /// Attribute index boosted, `-1` = none.
    pub attribute: i16,
    /// `baseEfficiency`.
    pub base_efficiency: f32,
    /// `boostScale`.
    pub boost_scale: f32,
    /// `minEfficiency`.
    pub min_efficiency: f32,
    /// `maxBoost`.
    pub max_boost: f32,
    /// `outputScale` (`0` = disabled; silicon-crucible `0.15`).
    pub output_scale: f32,
    /// `scaleLiquidConsumption` (efficiency also scales liquid input).
    pub scale_liquid_consumption: bool,
    /// `displayEfficiency` (`AttributeCrafter.setBars`/`setStats` gate).
    pub display_efficiency: bool,
    /// `displayScaledOutput` (`setBars`/`setStats` yield gate).
    pub display_scaled_output: bool,
}

impl Default for AttributeCrafterDef {
    /// `AttributeCrafter.java:15-31` field defaults: `attribute = Attribute.heat`
    /// (index 0), `baseEfficiency = 1`, `maxBoost = 1`, `minEfficiency = -1`,
    /// `boostScale = 1`, `displayEfficiency = displayScaledOutput = true`,
    /// `outputScale = 0`, `scaleLiquidConsumption = false`. The derived
    /// `Default` (all zero) silently removed the boost for every block that does
    /// not patch `boostScale` (cultivator).
    fn default() -> Self {
        Self {
            crafter: CrafterDef::default(),
            attribute: 0,
            base_efficiency: 1.0,
            boost_scale: 1.0,
            min_efficiency: -1.0,
            max_boost: 1.0,
            output_scale: 0.0,
            scale_liquid_consumption: false,
            display_efficiency: true,
            display_scaled_output: true,
        }
    }
}

/// `Drill` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct DrillDef {
    /// `Drill.drillTime` (ticks per item).
    pub drill_time: f32,
    /// Hardness tier required.
    pub tier: i32,
    /// `hardnessDrillMultiplier`.
    pub hardness_drill_multiplier: f32,
    /// `drillMultipliers` per item id (`(item, multiplier)`).
    pub drill_multipliers: Vec<(u16, f32)>,
    /// Blocked items (`blockedItems`).
    pub blocked_items: Vec<u16>,
    /// Whether this is a wall drill (`Drill.wallDrill`).
    pub wall_drill: bool,
    /// `Drill.liquidBoostIntensity` (`Drill.java:38`; blast 1.8, impact 1.75).
    pub liquid_boost_intensity: f32,
    /// `Drill.warmupSpeed` (`Drill.java:40`; blast drill 0.01).
    pub warmup_speed: f32,
}

impl Default for DrillDef {
    fn default() -> Self {
        // `Drill.java:38,40`: `liquidBoostIntensity = 1.6f`,
        // `warmupSpeed = 0.015f`.
        Self {
            // `Drill.java:28,34,36`: `hardnessDrillMultiplier = 50f`,
            // `tier = 0`, `drillTime = 300f`.
            drill_time: 300.0,
            tier: 0,
            hardness_drill_multiplier: 50.0,
            drill_multipliers: Vec::new(),
            blocked_items: Vec::new(),
            wall_drill: false,
            liquid_boost_intensity: 1.6,
            warmup_speed: 0.015,
        }
    }
}

/// `BurstDrill` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct BurstDrillDef {
    /// Base drill knobs.
    pub drill: DrillDef,
    /// `BurstDrill.burstTime`.
    pub burst_time: f32,
    /// `BurstDrill.invertedTime`.
    pub inverted_time: f32,
    /// `BurstDrill.drillMultiplier`.
    pub drill_multiplier: f32,
    /// `BurstDrill.shake` (`BurstDrill.java:15`, default 2): the burst's
    /// `Effect.shake(shake, shake, this)` intensity.
    pub shake: f32,
    /// `BurstDrill.drillEffect` (`BurstDrill.java:26` constructor default
    /// `Fx.shockwave`): the effect played at the burst. The two vanilla drills
    /// replace it with a `MultiEffect(...)` (`Blocks.java:3095,3121-3124`).
    pub drill_effect: crate::content::registries::fx_meta::EffectRef,
    /// `BurstDrill.drillEffectRnd` (`BurstDrill.java:26` constructor
    /// `drillEffectRnd = 0f`, overriding `Drill.drillEffectRnd = -1`): the
    /// `Mathf.range(...)` x/y offset radius.
    pub drill_effect_rnd: f32,
}

impl Default for BurstDrillDef {
    fn default() -> Self {
        // `BurstDrill.java:32-54` constructor: it "does not drill in the
        // traditional sense", so `hardnessDrillMultiplier = 0`; `invertedTime`
        // keeps the `BurstDrill.java:26` field initializer 200; `shake = 2`
        // (`:15`), `drillEffect = Fx.shockwave` + `drillEffectRnd = 0f` (`:26`).
        Self {
            drill: DrillDef {
                hardness_drill_multiplier: 0.0,
                ..DrillDef::default()
            },
            burst_time: 0.0,
            inverted_time: 200.0,
            drill_multiplier: 1.0,
            shake: 2.0,
            drill_effect: crate::content::registries::fx_meta::EffectRef::Named(
                crate::content::EffectId::SHOCKWAVE,
            ),
            drill_effect_rnd: 0.0,
        }
    }
}

/// `BeamDrill` family knobs.
#[derive(Debug, Clone, PartialEq)]
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

impl Default for BeamDrillDef {
    fn default() -> Self {
        // `BeamDrill.java:38-41` field initializers.
        Self {
            range: 5,
            drill_time: 200.0,
            tier: 1,
            blocked_items: Vec::new(),
        }
    }
}

/// `Pump` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct PumpDef {
    /// `Pump.pumpAmount`.
    pub pump_amount: f32,
}

impl Default for PumpDef {
    fn default() -> Self {
        // `Pump.java:20`: `pumpAmount = 0.2f`.
        Self { pump_amount: 0.2 }
    }
}

/// `SolidPump` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct SolidPumpDef {
    /// `SolidPump.pumpAmount`.
    pub pump_amount: f32,
    /// `SolidPump.attribute` name (`""` = none).
    pub attribute: String,
    /// `SolidPump.baseEfficiency`.
    pub base_efficiency: f32,
    /// `SolidPump.result` (`SolidPump.java:22`, default `Liquids.water`; the
    /// content `pumpLiquid` field). `None` = the class default.
    pub result: Option<u16>,
}

impl Default for SolidPumpDef {
    fn default() -> Self {
        // `Pump.java:20` (`pumpAmount = 0.2f`, inherited) and
        // `SolidPump.java:26` (`baseEfficiency = 1f`; `attribute` null).
        Self {
            pump_amount: 0.2,
            attribute: String::new(),
            base_efficiency: 1.0,
            result: Some(crate::content::LiquidId::WATER.raw()),
        }
    }
}

/// `Fracker` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct FrackerDef {
    /// `Fracker.itemUseTime`.
    pub item_use_time: f32,
    /// `SolidPump.pumpAmount`.
    pub pump_amount: f32,
    /// Resulting liquid, if any.
    pub result: Option<u16>,
    /// `SolidPump.attribute` name (`""` = none).
    pub attribute: String,
    /// `SolidPump.baseEfficiency`.
    pub base_efficiency: f32,
}

impl Default for FrackerDef {
    fn default() -> Self {
        // `Fracker.java:8` (`itemUseTime = 100f`) on top of the `SolidPump`
        // defaults; `result` inherits `Liquids.water` (`SolidPump.java:22`).
        Self {
            item_use_time: 100.0,
            pump_amount: 0.2,
            result: Some(crate::content::LiquidId::WATER.raw()),
            attribute: String::new(),
            base_efficiency: 1.0,
        }
    }
}

/// `WallCrafter` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct WallCrafterDef {
    /// `WallCrafter.drillTime`.
    pub drill_time: f32,
    /// `WallCrafter.attribute` name (Java `Attribute.sand`).
    pub attribute: String,
    /// Item output (`WallCrafter.output`).
    pub output: Option<u16>,
    /// `WallCrafter.liquidBoostIntensity` (`1.6f`).
    pub liquid_boost_intensity: f32,
    /// `WallCrafter.itemBoostIntensity` (`1.6f`).
    pub item_boost_intensity: f32,
    /// `WallCrafter.boostItemUseTime` (`120f`; the item-boost consumer timer).
    pub boost_item_use_time: f32,
    /// `WallCrafter.hasLiquidBooster`, set in `init()` from
    /// `findConsumer(f -> f instanceof ConsumeLiquidBase && f.booster) != null`.
    /// `false` for both vanilla cliff crushers: `large-cliff-crusher`'s hydrogen
    /// consumer is required, not `.boost()`ed.
    pub has_liquid_booster: bool,
}

impl Default for WallCrafterDef {
    fn default() -> Self {
        // `WallCrafter.java:28-49` field defaults (`attribute = Attribute.sand`).
        Self {
            drill_time: 150.0,
            attribute: String::from("sand"),
            output: None,
            liquid_boost_intensity: 1.6,
            item_boost_intensity: 1.6,
            boost_item_use_time: 120.0,
            has_liquid_booster: false,
        }
    }
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
#[derive(Debug, Clone, PartialEq)]
pub struct WallDef {
    /// `Wall.autotile`.
    pub autotile: bool,
    /// `Wall.lightningChance` (`-1` = disabled).
    pub lightning_chance: f32,
    /// `Wall.lightningDamage`.
    pub lightning_damage: f32,
    /// `Wall.lightningLength`.
    pub lightning_length: i32,
    /// `Wall.chanceDeflect` (`-1` = disabled).
    pub chance_deflect: f32,
    /// `Wall.flashHit`: the wall draws a white additive overlay while `hit`
    /// decays after a bullet collision (`Wall.java:29,146-161`; I02 T4).
    pub flash_hit: bool,
}

impl Default for WallDef {
    fn default() -> Self {
        // `Wall.java:21-31` field defaults.
        Self {
            autotile: false,
            lightning_chance: -1.0,
            lightning_damage: 20.0,
            lightning_length: 17,
            chance_deflect: -1.0,
            flash_hit: false,
        }
    }
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
#[derive(Debug, Clone, PartialEq)]
pub struct RadarDef {
    /// `Radar.discoveryTime`.
    pub discovery_time: f32,
}

impl Default for RadarDef {
    fn default() -> Self {
        // `Radar.java:18`: `discoveryTime = 60f * 10f`.
        Self {
            discovery_time: 600.0,
        }
    }
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
    /// Whether this is a deep liquid (`Floor.isDeep`, derived from `drownTime`).
    pub is_deep: bool,
    /// Whether this floor is a liquid (`Floor.isLiquid`).
    pub is_liquid: bool,
    /// `ShallowLiquid`/generation shallow flag (`Floor.shallow`).
    pub shallow: bool,
    /// Unit velocity multiplier (`Floor.speedMultiplier`).
    pub speed_multiplier: f32,
    /// Ticks to drown (`Floor.drownTime`; plan 10/11).
    pub drown_time: f32,
    /// `TiledFloor.maxSize` (`TiledFloor.java:17`; `0` = not a tiled floor):
    /// largest packed square the floor renderer may assemble. Tiled floors are
    /// still `Floor` family data; the `max_size` value selects the packed-tile
    /// draw path.
    pub max_size: i32,
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
    /// `TiledWall.maxSize` (`TiledWall.java:16`; `0` = not a tiled wall):
    /// largest packed square the wall renderer may assemble.
    pub max_size: i32,
}

/// `Prop`/`StaticProp` family knobs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PropDef {
    /// Draw layer (`Prop.layer`).
    pub layer: f32,
    /// `WobbleProp.wscl` (`WobbleProp.java:9`, class default 25).
    pub wobble_scale: f32,
    /// `WobbleProp.wmag` (class default 0.4).
    pub wobble_magnitude: f32,
    /// `WobbleProp.wtscl` (class default 1).
    pub wobble_time_scale: f32,
    /// `WobbleProp.wmag2` (class default 1).
    pub wobble_magnitude2: f32,
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
    /// Whether this floor is a liquid (`Floor.isLiquid`; always true here).
    pub is_liquid: bool,
    /// `Floor.speedMultiplier`.
    pub speed_multiplier: f32,
    /// `Floor.drownTime` (`ShallowLiquid` never sets one).
    pub drown_time: f32,
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

/// `Accelerator` family knobs (`world/blocks/campaign/Accelerator.java:37-73`).
#[derive(Debug, Clone, PartialEq)]
pub struct AcceleratorDef {
    /// `launchBlock` name (default `Blocks.coreNucleus`, :39).
    pub launch_block: String,
    /// `launchBlock.itemCapacity`: the extra capacity once the core is built
    /// (`getMaximumAccepted`, :293-295); core-nucleus = 13000 (Blocks.java:3177).
    pub launch_block_item_capacity: i32,
    /// `powerBufferRequirement` (:40; the vanilla block 1_000_000,
    /// Blocks.java:6876).
    pub power_buffer_requirement: f32,
    /// `launchDuration` (:51).
    pub launch_duration: f32,
    /// `chargeDuration` (:52).
    pub charge_duration: f32,
    /// `buildDuration` (:53): core construction time once powered and full.
    pub build_duration: f32,
}

impl Default for AcceleratorDef {
    fn default() -> Self {
        Self {
            launch_block: "core-nucleus".to_owned(),
            launch_block_item_capacity: 13000,
            power_buffer_requirement: 0.0,
            launch_duration: 120.0,
            charge_duration: 220.0,
            build_duration: 120.0,
        }
    }
}

/// `LandingPad` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct LandingPadDef {
    /// Item capacity for the landing inventory.
    pub item_capacity: i32,
    /// `LandingPad.arrivalDuration` (ticks).
    pub arrival_duration: f32,
    /// `LandingPad.cooldownTime` (ticks).
    pub cooldown_time: f32,
    /// `LandingPad.consumeLiquidAmount` (water per pod).
    pub consume_liquid_amount: f32,
    /// `LandingPad.consumeLiquid` (`Liquids.water`).
    pub consume_liquid: crate::content::LiquidId,
}

impl Default for LandingPadDef {
    fn default() -> Self {
        Self {
            item_capacity: 0,
            // `Blocks.java:6862-6871`: `arrivalDuration` 150, `cooldownTime`
            // 150, `consumeLiquidAmount` 1500, `consumeLiquid` water.
            arrival_duration: 150.0,
            cooldown_time: 150.0,
            consume_liquid_amount: 1500.0,
            consume_liquid: crate::content::LiquidId::WATER,
        }
    }
}

/// `LaunchPad` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchPadDef {
    /// `LaunchPad.launchTime` (ticks).
    pub launch_time: f32,
    /// `LaunchPad.acceptMultipleItems` (false: one item type at a time).
    pub accept_multiple_items: bool,
}

impl Default for LaunchPadDef {
    fn default() -> Self {
        // `LaunchPad.java:32,40`: `launchTime = 1f`, `acceptMultipleItems =
        // false` (the vanilla 1200/1800-tick pads override it in `Blocks.java`).
        Self {
            launch_time: 1.0,
            accept_multiple_items: false,
        }
    }
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
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryDef {
    /// `MemoryBlock.memoryCapacity`.
    pub capacity: i32,
    /// Whether this is the privileged `world-cell`.
    pub privileged: bool,
}

impl Default for MemoryDef {
    fn default() -> Self {
        // `MemoryBlock.java:17`: `memoryCapacity = 32`.
        Self {
            capacity: 32,
            privileged: false,
        }
    }
}

/// `SwitchBlock` family data (privileged flag for `world-switch`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SwitchDef {
    /// Whether this is the privileged `world-switch`.
    pub privileged: bool,
}

/// `MessageBlock` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageDef {
    /// `MessageBlock.maxTextLength`.
    pub max_text: i32,
    /// `MessageBlock.maxNewlines`.
    pub max_newlines: i32,
    /// Whether this is the privileged `world-message`.
    pub privileged: bool,
}

impl Default for MessageDef {
    fn default() -> Self {
        // `MessageBlock.java:29-30`.
        Self {
            max_text: 400,
            max_newlines: 24,
            privileged: false,
        }
    }
}

/// `LogicDisplay` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayDef {
    /// `LogicDisplay.displaySize`.
    pub display_size: i32,
    /// `LogicDisplay.scaleFactor`.
    pub scale_factor: f32,
}

impl Default for DisplayDef {
    fn default() -> Self {
        // `LogicDisplay.java:53-54`: `displaySize = 64`, `scaleFactor = 1`.
        Self {
            display_size: 64,
            scale_factor: 1.0,
        }
    }
}

/// `TileableLogicDisplay` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct TileableDisplayDef {
    /// Per-tile `displaySize`.
    pub display_size: i32,
    /// `TileableLogicDisplay.frameSize`.
    pub frame_size: i32,
    /// `TileableLogicDisplay.maxDisplayDimensions`.
    pub max_dimensions: i32,
}

impl Default for TileableDisplayDef {
    fn default() -> Self {
        // `TileableLogicDisplay.java:27-37`: `maxDisplayDimensions = 16`,
        // `frameSize = 6`, `displaySize = 32` (constructor).
        Self {
            display_size: 32,
            frame_size: 6,
            max_dimensions: 16,
        }
    }
}

/// `CanvasBlock` family knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasDef {
    /// `CanvasBlock.canvasSize`.
    pub canvas_size: i32,
    /// `CanvasBlock.padding`.
    pub padding: f32,
    /// `CanvasBlock.bitsPerPixel`.
    pub bits_per_pixel: u8,
    /// `CanvasBlock.palette` (RGBA8888; 8 entries for `canvas`, the 16-colour
    /// woodspark palette for `large-canvas`, `Blocks.java:6965-6983`).
    pub palette: Vec<u32>,
}

impl Default for CanvasDef {
    fn default() -> Self {
        // `CanvasBlock.java:22-26` field initializers (the 8-colour default
        // palette; `bitsPerPixel = 0` until a colour count is set).
        Self {
            canvas_size: 8,
            padding: 0.0,
            bits_per_pixel: 0,
            palette: vec![
                0x3629_44ff,
                0xc45d_9fff,
                0xe39a_acff,
                0xf0da_b1ff,
                0x6461_c2ff,
                0x2ba9_b4ff,
                0x93d4_b5ff,
                0xf0f6_e8ff,
            ],
        }
    }
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

/// Resolves the `GenericCrafter` content knobs onto the class field
/// initializers (`GenericCrafter.java:22-39`). Content wins; unset stays the
/// Java default.
fn crafter_from_def(def: &BlockDef) -> CrafterDef {
    let defaults = CrafterDef::default();
    CrafterDef {
        // A zero `craft_time` means the content did not override it, so the
        // class default applies (mod blocks/patches inherit it).
        craft_time: if def.craft_time > 0.0 {
            def.craft_time
        } else {
            defaults.craft_time
        },
        output_items: def
            .output_items
            .iter()
            .map(|stack| (stack.item.raw(), stack.amount))
            .collect(),
        output_liquids: def
            .output_liquids
            .iter()
            .map(|(liquid, amount)| (liquid.raw(), *amount))
            .collect(),
        liquid_output_directions: if def.liquid_output_directions.is_empty() {
            defaults.liquid_output_directions
        } else {
            def.liquid_output_directions.clone()
        },
        ignore_liquid_fullness: def.ignore_liquid_fullness,
    }
}

/// Resolves the `Drill` content knobs (`Drill.java:26-42`) with the family's
/// class defaults passed in (a burst drill's constructor zeroes
/// `hardnessDrillMultiplier`, `BurstDrill.java:35-44`).
fn drill_from_def(def: &BlockDef, defaults: DrillDef) -> DrillDef {
    DrillDef {
        drill_time: def.drill_time.unwrap_or(defaults.drill_time),
        tier: def.drill_tier.unwrap_or(defaults.tier),
        hardness_drill_multiplier: def
            .hardness_drill_multiplier
            .unwrap_or(defaults.hardness_drill_multiplier),
        drill_multipliers: def
            .drill_multipliers
            .iter()
            .map(|(item, multiplier)| (item.raw(), *multiplier))
            .collect(),
        blocked_items: def.blocked_items.iter().map(|item| item.raw()).collect(),
        wall_drill: defaults.wall_drill,
        liquid_boost_intensity: def
            .liquid_boost_intensity
            .unwrap_or(defaults.liquid_boost_intensity),
        warmup_speed: def.warmup_speed.unwrap_or(defaults.warmup_speed),
    }
}

impl BlockKindData {
    /// Derives the typed data from an existing metadata record.
    ///
    /// Per-family knobs are copied from [`BlockDef`] when the content set them;
    /// otherwise the upstream class field initializer applies (mod blocks and
    /// patches inherit it). `apply_vanilla_knobs` remains the fallback for the
    /// `Blocks.java` per-block recipe values the vanilla waves have not moved
    /// onto `BlockSpec` yet.
    pub fn from_def(def: &BlockDef) -> Self {
        match def.kind {
            BlockKind::ConstructBlock => BlockKindData::Construct {
                size: def.size.clamp(1, 16) as u8,
            },
            BlockKind::Floor
            | BlockKind::EmptyFloor
            | BlockKind::ColoredFloor
            | BlockKind::TiledFloor => {
                BlockKindData::Floor(FloorDef {
                    blend_group: 0,
                    is_deep: def.is_deep,
                    is_liquid: def.is_liquid,
                    shallow: def.shallow,
                    speed_multiplier: def.speed_multiplier,
                    drown_time: def.drown_time,
                    // `TiledFloor.maxSize = 3` (`TiledFloor.java:17`).
                    max_size: if def.kind == BlockKind::TiledFloor {
                        if def.max_size > 0 { def.max_size } else { 3 }
                    } else {
                        def.max_size
                    },
                })
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
            BlockKind::StaticWall | BlockKind::ColoredWall | BlockKind::TiledWall => {
                BlockKindData::StaticWall(StaticWallDef {
                    large: def.size >= 2,
                    // `TiledWall.maxSize = 3` (`TiledWall.java:16`).
                    max_size: if def.kind == BlockKind::TiledWall {
                        if def.max_size > 0 { def.max_size } else { 3 }
                    } else {
                        def.max_size
                    },
                })
            }
            BlockKind::Prop
            | BlockKind::StaticProp
            | BlockKind::StaticTree
            | BlockKind::TreeBlock => BlockKindData::Prop(PropDef::default()),
            // `WobbleProp extends Prop` (`WobbleProp.java:8`); the class field
            // initializers are `wscl = 25f`, `wmag = 0.4f`, `wtscl = 1f`,
            // `wmag2 = 1f` (`WobbleProp.java:9`).
            BlockKind::WobbleProp => BlockKindData::Prop(PropDef {
                layer: 0.0,
                wobble_scale: def.wobble_scale.unwrap_or(25.0),
                wobble_magnitude: def.wobble_magnitude.unwrap_or(0.4),
                wobble_time_scale: def.wobble_time_scale.unwrap_or(1.0),
                wobble_magnitude2: def.wobble_magnitude2.unwrap_or(1.0),
            }),
            BlockKind::Cliff | BlockKind::TallBlock => BlockKindData::Cliff(CliffDef::default()),
            BlockKind::ShallowLiquid => BlockKindData::ShallowLiquid(ShallowLiquidDef {
                shallow: true,
                is_liquid: def.is_liquid,
                speed_multiplier: def.speed_multiplier,
                drown_time: def.drown_time,
            }),
            BlockKind::SteamVent => BlockKindData::SteamVent(SteamVentDef::default()),
            BlockKind::SpawnBlock => BlockKindData::Spawn(SpawnDef::default()),
            BlockKind::Wall | BlockKind::ShieldWall => BlockKindData::Wall(WallDef::default()),
            BlockKind::Door | BlockKind::AutoDoor => BlockKindData::Door(DoorDef::default()),
            BlockKind::Radar => BlockKindData::Radar(RadarDef::default()),
            BlockKind::Thruster => BlockKindData::Thruster(ThrusterDef::default()),
            BlockKind::TargetDummy => BlockKindData::TargetDummy(TargetDummyDef::default()),
            BlockKind::GenericCrafter | BlockKind::HeatCrafter | BlockKind::HeatProducer => {
                // `HeatProducer` extends `GenericCrafter` (`HeatProducer.java:1`);
                // the kind data carries its craft loop exactly like a heat
                // crafter (T1: oxidation-chamber's oxide output).
                BlockKindData::Crafter(crafter_from_def(def))
            }
            BlockKind::AttributeCrafter => {
                let defaults = AttributeCrafterDef::default();
                // `AttributeCrafter.attribute` is an `Attribute` enum; the
                // content field is its name (`AttributeCrafter.java:15`).
                let attribute = def
                    .attribute
                    .as_deref()
                    .and_then(crate::world::attributes::attribute_from_name)
                    .map_or(defaults.attribute, |attribute| {
                        crate::world::attributes::attribute_index(attribute) as i16
                    });
                BlockKindData::AttributeCrafter(AttributeCrafterDef {
                    crafter: crafter_from_def(def),
                    attribute,
                    base_efficiency: def.base_efficiency.unwrap_or(defaults.base_efficiency),
                    boost_scale: def.boost_scale.unwrap_or(defaults.boost_scale),
                    min_efficiency: def.min_efficiency.unwrap_or(defaults.min_efficiency),
                    max_boost: def.max_boost.unwrap_or(defaults.max_boost),
                    output_scale: def.output_scale.unwrap_or(defaults.output_scale),
                    scale_liquid_consumption: def
                        .scale_liquid_consumption
                        .unwrap_or(defaults.scale_liquid_consumption),
                    display_efficiency: def
                        .display_efficiency
                        .unwrap_or(defaults.display_efficiency),
                    display_scaled_output: def
                        .display_scaled_output
                        .unwrap_or(defaults.display_scaled_output),
                })
            }
            BlockKind::Drill => BlockKindData::Drill(drill_from_def(def, DrillDef::default())),
            BlockKind::BurstDrill => {
                // `BurstDrill`'s constructor overrides the drill defaults
                // (`hardnessDrillMultiplier = 0`, `BurstDrill.java:35-44`);
                // `invertedTime` keeps its own field initializer. `shake` /
                // `drillEffect` default to the class values (`:15,26`) and the
                // two vanilla drills are patched by `apply_vanilla_knobs`.
                let defaults = BurstDrillDef::default();
                BlockKindData::BurstDrill(BurstDrillDef {
                    drill: drill_from_def(def, defaults.drill.clone()),
                    burst_time: defaults.burst_time,
                    inverted_time: def.inverted_time.unwrap_or(defaults.inverted_time),
                    drill_multiplier: defaults.drill_multiplier,
                    shake: def.burst_shake.unwrap_or(defaults.shake),
                    drill_effect: def
                        .burst_drill_effect
                        .clone()
                        .unwrap_or(defaults.drill_effect),
                    drill_effect_rnd: defaults.drill_effect_rnd,
                })
            }
            BlockKind::BeamDrill => {
                let defaults = BeamDrillDef::default();
                BlockKindData::BeamDrill(BeamDrillDef {
                    range: def.beam_range.unwrap_or(defaults.range),
                    drill_time: def.drill_time.unwrap_or(defaults.drill_time),
                    tier: def.drill_tier.unwrap_or(defaults.tier),
                    blocked_items: def.blocked_items.iter().map(|item| item.raw()).collect(),
                })
            }
            BlockKind::Pump => BlockKindData::Pump(PumpDef {
                pump_amount: def
                    .pump_amount
                    .unwrap_or_else(|| PumpDef::default().pump_amount),
            }),
            BlockKind::SolidPump => {
                let defaults = SolidPumpDef::default();
                BlockKindData::SolidPump(SolidPumpDef {
                    pump_amount: def.pump_amount.unwrap_or(defaults.pump_amount),
                    attribute: def.attribute.clone().unwrap_or(defaults.attribute),
                    base_efficiency: def.base_efficiency.unwrap_or(defaults.base_efficiency),
                    result: def
                        .pump_liquid
                        .map(|liquid| liquid.raw())
                        .or(defaults.result),
                })
            }
            BlockKind::Fracker => {
                let defaults = FrackerDef::default();
                BlockKindData::Fracker(FrackerDef {
                    item_use_time: def.item_use_time.unwrap_or(defaults.item_use_time),
                    pump_amount: def.pump_amount.unwrap_or(defaults.pump_amount),
                    result: def
                        .pump_liquid
                        .map(|liquid| liquid.raw())
                        .or(defaults.result),
                    attribute: def.attribute.clone().unwrap_or(defaults.attribute),
                    base_efficiency: def.base_efficiency.unwrap_or(defaults.base_efficiency),
                })
            }
            BlockKind::WallCrafter => {
                let defaults = WallCrafterDef::default();
                BlockKindData::WallCrafter(WallCrafterDef {
                    drill_time: def.drill_time.unwrap_or(defaults.drill_time),
                    attribute: def.attribute.clone().unwrap_or(defaults.attribute),
                    output: def.output_item.map(|item| item.raw()).or(defaults.output),
                    liquid_boost_intensity: def
                        .liquid_boost_intensity
                        .unwrap_or(defaults.liquid_boost_intensity),
                    item_boost_intensity: def
                        .item_boost_intensity
                        .unwrap_or(defaults.item_boost_intensity),
                    boost_item_use_time: def
                        .boost_item_use_time
                        .unwrap_or(defaults.boost_item_use_time),
                    has_liquid_booster: defaults.has_liquid_booster,
                })
            }
            BlockKind::Separator => BlockKindData::Separator(SeparatorDef {
                // `Separator extends Block` (not `GenericCrafter`), so its
                // `craftTime` is content data, not a class default.
                craft_time: def.craft_time,
                results: if def.separator_results.is_empty() {
                    def.output_items
                        .iter()
                        .map(|stack| (stack.item.raw(), stack.amount))
                        .collect()
                } else {
                    def.separator_results
                        .iter()
                        .map(|stack| (stack.item.raw(), stack.amount))
                        .collect()
                },
            }),
            BlockKind::Incinerator | BlockKind::ItemIncinerator => {
                BlockKindData::Incinerator(IncineratorDef::default())
            }
            // `ItemSource.itemsPerSecond = 100`.
            BlockKind::ItemSource => BlockKindData::ItemSource(SandboxItemDef {
                items_per_second: 100.0,
            }),
            BlockKind::ItemVoid => BlockKindData::ItemVoid,
            BlockKind::LiquidSource => BlockKindData::LiquidSource(SandboxLiquidDef::default()),
            BlockKind::LiquidVoid => BlockKindData::LiquidVoid,
            // `Blocks.powerSource.powerProduction = 1000000f / 60f`.
            BlockKind::PowerSource => BlockKindData::PowerSource(SandboxPowerDef {
                power_production: 1_000_000.0 / 60.0,
            }),
            BlockKind::PowerVoid => BlockKindData::PowerVoid,
            BlockKind::Accelerator => BlockKindData::Accelerator(AcceleratorDef {
                // `interplanetaryAccelerator.powerBufferRequirement`
                // (Blocks.java:6876).
                power_buffer_requirement: if def.name == "interplanetary-accelerator" {
                    1_000_000.0
                } else {
                    0.0
                },
                ..AcceleratorDef::default()
            }),
            BlockKind::LandingPad => BlockKindData::LandingPad(LandingPadDef {
                item_capacity: def.item_capacity,
                ..LandingPadDef::default()
            }),
            BlockKind::LaunchPad => BlockKindData::LaunchPad(match def.name.as_str() {
                // `Blocks.java:6844,6846` (`launch-pad` 1200 ticks,
                // `acceptMultipleItems = true`) and `:6854,6856`
                // (`advanced-launch-pad` 1800 ticks, one item type).
                "launch-pad" => LaunchPadDef {
                    launch_time: 1200.0,
                    accept_multiple_items: true,
                },
                "advanced-launch-pad" => LaunchPadDef {
                    launch_time: 1800.0,
                    accept_multiple_items: false,
                },
                _ => LaunchPadDef::default(),
            }),
            BlockKind::LegacyMechPad
            | BlockKind::LegacyUnitFactory
            | BlockKind::LegacyCommandCenter => BlockKindData::Legacy(LegacyDef::default()),
            BlockKind::LogicBlock => BlockKindData::Logic(LogicBlockDef::default()),
            BlockKind::MemoryBlock => BlockKindData::Memory(MemoryDef::default()),
            BlockKind::SwitchBlock => BlockKindData::Switch(SwitchDef::default()),
            // `MessageBlock.maxTextLength = 400`, `maxNewlines = 24`.
            BlockKind::MessageBlock => BlockKindData::Message(MessageDef {
                max_text: 400,
                max_newlines: 24,
                privileged: false,
            }),
            BlockKind::LogicDisplay => BlockKindData::Display(DisplayDef::default()),
            BlockKind::TileableLogicDisplay => {
                BlockKindData::TileableDisplay(TileableDisplayDef::default())
            }
            BlockKind::CanvasBlock => BlockKindData::Canvas(CanvasDef::default()),
            _ => BlockKindData::None,
        }
    }

    /// Applies the legacy `Blocks.java` per-block vanilla recipe overlay.
    ///
    /// The family knobs are content fields on [`BlockDef`] now (GAPS #56
    /// follow-up), and [`Self::from_def`] copies them; this table remains the
    /// fallback for the `Blocks.java` per-block values the generated vanilla
    /// waves have not moved onto `BlockSpec` yet. Mod blocks never match a
    /// vanilla name, so their content values always win. It is **07-owned** and
    /// does not mutate plan-02 metadata.
    pub fn apply_vanilla_knobs(&mut self, content: &crate::content::ContentRegistry, name: &str) {
        let item = |n: &str| content.item_id(n).map(|id| id.raw()).unwrap_or(0);
        let liquid = |n: &str| content.liquid_id(n).map(|id| id.raw()).unwrap_or(0);
        // `craft_time` is now copied from the real plan-02 `BlockDef.craft_time`
        // field (`from_def`); this overlay only supplies the output stacks that
        // plan-02 does not yet carry.
        let craft = |items: Vec<(u16, i32)>, liquids: Vec<(u16, f32)>, ignore: bool| CrafterDef {
            craft_time: 0.0,
            output_items: items,
            output_liquids: liquids,
            liquid_output_directions: Vec::new(),
            ignore_liquid_fullness: ignore,
        };
        // `Blocks.java` factories whose `craftTime` is set explicitly upstream
        // (`GenericCrafter.craftTime` defaults to 80).
        let craft_t = |items: Vec<(u16, i32)>,
                       liquids: Vec<(u16, f32)>,
                       craft_time: f32,
                       ignore: bool| CrafterDef {
            craft_time,
            output_items: items,
            output_liquids: liquids,
            liquid_output_directions: Vec::new(),
            ignore_liquid_fullness: ignore,
        };
        match name {
            // `Blocks.java:1759-1772`: phase walls deflect bullets
            // (`chanceDeflect = 10f; flashHit = true`).
            "phase-wall" | "phase-wall-large" => {
                if let BlockKindData::Wall(def) = self {
                    def.chance_deflect = 10.0;
                    def.flash_hit = true;
                }
            }
            // `Blocks.java:1774-1785`: surge walls arc lightning
            // (`lightningChance = 0.05f`; damage/length are the class defaults).
            "surge-wall" | "surge-wall-large" => {
                if let BlockKindData::Wall(def) = self {
                    def.lightning_chance = 0.05;
                }
            }
            // `Blocks.java:1873-1888`: Erekir surge walls arc for 30 damage.
            "reinforced-surge-wall" | "reinforced-surge-wall-large" => {
                if let BlockKindData::Wall(def) = self {
                    def.lightning_chance = 0.05;
                    def.lightning_damage = 30.0;
                }
            }
            "silicon-smelter" => self.set_crafter(craft(vec![(item("silicon"), 1)], vec![], false)),
            // `Blocks.java:1041 graphite-press` — graphite x1, craftTime 90.
            "graphite-press" => {
                self.set_crafter(craft_t(vec![(item("graphite"), 1)], vec![], 90.0, false))
            }
            // `Blocks.java:1053 multi-press` — graphite x2, craftTime 30.
            "multi-press" => {
                self.set_crafter(craft_t(vec![(item("graphite"), 2)], vec![], 30.0, false))
            }
            // `Blocks.java:1086 silicon-crucible` — silicon x8, craftTime 90,
            // `boostScale`/`outputScale` 0.15, `baseEfficiency` 1.
            "silicon-crucible" => {
                self.set_crafter(craft_t(vec![(item("silicon"), 8)], vec![], 90.0, false));
                if let BlockKindData::AttributeCrafter(def) = self {
                    def.attribute = 0;
                    def.base_efficiency = 1.0;
                    def.boost_scale = 0.15;
                    def.max_boost = 1.0;
                    def.min_efficiency = -1.0;
                    // `Blocks.java:1090` `outputScale = 0.15f`.
                    def.output_scale = 0.15;
                }
            }
            // `Blocks.java:1120 plastanium-compressor` — plastanium x1, craftTime 60.
            "plastanium-compressor" => {
                self.set_crafter(craft_t(vec![(item("plastanium"), 1)], vec![], 60.0, false))
            }
            // `Blocks.java:1138 phase-weaver` — phase-fabric x1, craftTime 120.
            "phase-weaver" => self.set_crafter(craft_t(
                vec![(item("phase-fabric"), 1)],
                vec![],
                120.0,
                false,
            )),
            // `Blocks.java:1170 cryofluid-mixer` — cryofluid 12/60, craftTime 120.
            "cryofluid-mixer" => self.set_crafter(craft_t(
                vec![],
                vec![(liquid("cryofluid"), 12.0 / 60.0)],
                120.0,
                false,
            )),
            // `Blocks.java:1191 pyratite-mixer` — pyratite x1, default craftTime 80.
            "pyratite-mixer" => {
                self.set_crafter(craft_t(vec![(item("pyratite"), 1)], vec![], 80.0, false))
            }
            // `Blocks.java:1206 blast-mixer` — blast-compound x1, default craftTime 80.
            "blast-mixer" => self.set_crafter(craft_t(
                vec![(item("blast-compound"), 1)],
                vec![],
                80.0,
                false,
            )),
            // `Blocks.java:1335 silicon-arc-furnace` — silicon x4, craftTime 50.
            "silicon-arc-furnace" => {
                self.set_crafter(craft_t(vec![(item("silicon"), 4)], vec![], 50.0, false))
            }
            // `Blocks.java:1356 electrolyzer` — ozone 4/60 + hydrogen 6/60, craftTime 10.
            "electrolyzer" => {
                let mut crafter = craft_t(
                    vec![],
                    vec![
                        (liquid("ozone"), 4.0 / 60.0),
                        (liquid("hydrogen"), 6.0 / 60.0),
                    ],
                    10.0,
                    false,
                );
                // `Blocks.java:1397`: `liquidOutputDirections = new int[]{1, 3}`
                // (relative to rotation; the only vanilla crafter with them).
                crafter.liquid_output_directions = vec![1, 3];
                self.set_crafter(crafter);
            }
            // `Blocks.java:1400 atmospheric-concentrator` (HeatCrafter) —
            // nitrogen 16/60, default craftTime 80.
            "atmospheric-concentrator" => self.set_crafter(craft_t(
                vec![],
                vec![(liquid("nitrogen"), 16.0 / 60.0)],
                80.0,
                false,
            )),
            // `Blocks.java:1429 oxidation-chamber` (HeatProducer, which extends
            // GenericCrafter) — oxide x1, craftTime 60 * 2, liquidCapacity 30.
            "oxidation-chamber" => {
                self.set_crafter(craft_t(vec![(item("oxide"), 1)], vec![], 60.0 * 2.0, false))
            }
            // `Blocks.java:1537 carbide-crucible` (HeatCrafter) — carbide x1,
            // craftTime 60 * 2.25/4 = 33.75.
            "carbide-crucible" => self.set_crafter(craft_t(
                vec![(item("carbide"), 1)],
                vec![],
                60.0 * 2.25 / 4.0,
                false,
            )),
            // `Blocks.java:1556 slag-centrifuge` — gallium 1/60, craftTime 120.
            "slag-centrifuge" => self.set_crafter(craft_t(
                vec![],
                vec![(liquid("gallium"), 1.0 / 60.0)],
                120.0,
                false,
            )),
            // `Blocks.java:1587 surge-crucible` (HeatCrafter) — surge-alloy x1,
            // craftTime 60 * 3/4 = 45.
            "surge-crucible" => self.set_crafter(craft_t(
                vec![(item("surge-alloy"), 1)],
                vec![],
                60.0 * 3.0 / 4.0,
                false,
            )),
            // `Blocks.java:1623 cyanogen-synthesizer` (HeatCrafter) — cyanogen
            // 12/60, craftTime 80/4 = 20.
            "cyanogen-synthesizer" => self.set_crafter(craft_t(
                vec![],
                vec![(liquid("cyanogen"), 12.0 / 60.0)],
                80.0 / 4.0,
                false,
            )),
            // `Blocks.java:1655 phase-synthesizer` (HeatCrafter) — phase-fabric
            // x1, craftTime 60 * 2/4 = 30.
            "phase-synthesizer" => self.set_crafter(craft_t(
                vec![(item("phase-fabric"), 1)],
                vec![],
                60.0 * 2.0 / 4.0,
                false,
            )),
            "surge-smelter" => {
                self.set_crafter(craft(vec![(item("surge-alloy"), 1)], vec![], false))
            }
            "spore-press" => {
                self.set_crafter(craft(vec![], vec![(liquid("oil"), 18.0 / 60.0)], false))
            }
            "coal-centrifuge" => self.set_crafter(craft(vec![(item("coal"), 1)], vec![], false)),
            "kiln" => self.set_crafter(craft(vec![(item("metaglass"), 1)], vec![], false)),
            "melter" => self.set_crafter(craft(vec![], vec![(liquid("slag"), 12.0 / 60.0)], false)),
            "pulverizer" => self.set_crafter(craft(vec![(item("sand"), 1)], vec![], false)),
            "separator" => {
                if let BlockKindData::Separator(def) = self {
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
            "blast-drill" => {
                self.set_drill(280.0, 5, None);
                // `Blocks.java:2929,2933`: warmupSpeed 0.01,
                // liquidBoostIntensity 1.8.
                self.set_drill_boost(1.8, 0.01);
            }
            // `Blocks.java:2299 mechanical-pump` — `pumpAmount = 7/60`.
            "mechanical-pump" => self.set_pump_amount(7.0 / 60.0),
            // `Blocks.java:2305 rotary-pump` — `pumpAmount = 0.2`.
            "rotary-pump" => self.set_pump_amount(0.2),
            // `Blocks.java:2314 impulse-pump` — `pumpAmount = 0.22`.
            "impulse-pump" => self.set_pump_amount(0.22),
            // `Blocks.java:2404 reinforced-pump` — `pumpAmount = 80/60/4`.
            "reinforced-pump" => self.set_pump_amount(80.0 / 60.0 / 4.0),
            "impact-drill" => {
                if let BlockKindData::BurstDrill(def) = self {
                    def.drill.drill_time = 720.0;
                    def.drill.tier = 6;
                    def.drill.blocked_items = vec![item("thorium")];
                    // `drillMultipliers.put(Items.beryllium, 2f)` (Blocks.java:3102).
                    def.drill.drill_multipliers = vec![(item("beryllium"), 2.0)];
                    def.drill_multiplier = 1.0;
                    def.burst_time = 60.0 * 5.0;
                    // `BurstDrill.invertedTime` default 200 (BurstDrill.java:27).
                    def.inverted_time = 200.0;
                    // `Blocks.java:3095-3096`: `drillEffect = new
                    // MultiEffect(Fx.mineImpact, Fx.drillSteam,
                    // Fx.mineImpactWave.wrap(Pal.redLight, 40f)); shake = 4f`.
                    def.shake = 4.0;
                    def.drill_effect = crate::fx::inline::multi(vec![
                        crate::fx::inline::named(crate::content::EffectId::MINE_IMPACT),
                        crate::fx::inline::named(crate::content::EffectId::DRILL_STEAM),
                        crate::fx::inline::wrap(
                            crate::fx::inline::named(crate::content::EffectId::MINE_IMPACT_WAVE),
                            crate::content::registries::pal::RED_LIGHT,
                            40.0,
                        ),
                    ]);
                }
                // `Blocks.java:3103`: liquidBoostIntensity 1.75 (burst warmup
                // stays the `BurstDrill.java:92,101` literal 0.01).
                self.set_drill_boost(1.75, 0.01);
            }
            // `Blocks.java:3112 eruption-drill` — `drillTime = 281.25`, tier 7,
            // `BurstDrill.invertedTime` default 200.
            "eruption-drill" => {
                if let BlockKindData::BurstDrill(def) = self {
                    def.drill.drill_time = 281.25;
                    def.drill.tier = 7;
                    // `drillMultipliers.put(Items.beryllium, 2f)` (Blocks.java:3133).
                    def.drill.drill_multipliers = vec![(item("beryllium"), 2.0)];
                    def.drill_multiplier = 1.0;
                    def.inverted_time = 200.0;
                    // `Blocks.java:3121-3125`: `drillEffect = new MultiEffect(
                    // Fx.mineImpact, Fx.drillSteam,
                    // Fx.dynamicSpikes.wrap(Liquids.hydrogen.color, 30f),
                    // Fx.mineImpactWave.wrap(Liquids.hydrogen.color, 45f));
                    // shake = 4f`.
                    let hydrogen = content
                        .liquid_by_name("hydrogen")
                        .map_or(crate::content::Rgba::from_rgba8888(0x9eabf7ff), |l| l.color);
                    def.shake = 4.0;
                    def.drill_effect = crate::fx::inline::multi(vec![
                        crate::fx::inline::named(crate::content::EffectId::MINE_IMPACT),
                        crate::fx::inline::named(crate::content::EffectId::DRILL_STEAM),
                        crate::fx::inline::wrap(
                            crate::fx::inline::named(crate::content::EffectId::DYNAMIC_SPIKES),
                            hydrogen,
                            30.0,
                        ),
                        crate::fx::inline::wrap(
                            crate::fx::inline::named(crate::content::EffectId::MINE_IMPACT_WAVE),
                            hydrogen,
                            45.0,
                        ),
                    ]);
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
                    // `Blocks.java:2941-2948`: `attribute = Attribute.water`,
                    // `baseEfficiency = 1f` (the `SolidPump` field default).
                    def.attribute = String::from("water");
                    def.base_efficiency = 1.0;
                }
            }
            "oil-extractor" => {
                if let BlockKindData::Fracker(def) = self {
                    def.pump_amount = 0.25;
                    def.item_use_time = 60.0;
                    def.result = Some(liquid("oil"));
                    // `Blocks.java:2985-2992`: `attribute = Attribute.oil`,
                    // `baseEfficiency = 0f`.
                    def.attribute = String::from("oil");
                    def.base_efficiency = 0.0;
                }
            }
            "cultivator" => {
                if let BlockKindData::AttributeCrafter(def) = self {
                    let craft_time = def.crafter.craft_time;
                    def.crafter = craft(vec![(item("spore-pod"), 1)], vec![], false);
                    def.crafter.craft_time = craft_time;
                    // `Blocks.java:2952-2972`: `attribute = Attribute.spores`
                    // (index 1 in `world::attributes::ATTRIBUTE_NAMES`),
                    // `baseEfficiency`/`boostScale`/`minEfficiency` keep the
                    // `AttributeCrafter` defaults (1/1/-1), `maxBoost = 2`.
                    def.attribute = 1;
                    def.base_efficiency = 1.0;
                    def.max_boost = 2.0;
                    def.min_efficiency = -1.0;
                }
            }
            "vent-condenser" => {
                if let BlockKindData::AttributeCrafter(def) = self {
                    let craft_time = def.crafter.craft_time;
                    def.crafter = craft(vec![], vec![(liquid("water"), 30.0 / 60.0)], false);
                    def.crafter.craft_time = craft_time;
                    // `Blocks.java:3000-3006`: `attribute = Attribute.steam`
                    // (index 4).
                    def.attribute = 4;
                    def.base_efficiency = 0.0;
                    def.min_efficiency = 9.0 - 0.0001;
                    def.boost_scale = 1.0 / 9.0;
                    // `Blocks.java:3005` `displayEfficiency = false`.
                    def.display_efficiency = false;
                }
            }
            "cliff-crusher" => self.set_wall_crafter(110.0, Some(item("sand"))),
            "large-cliff-crusher" => {
                self.set_wall_crafter(48.0, Some(item("sand")));
                // `Blocks.java:3051`: `boostItemUseTime = 60f / 0.75f`.
                if let BlockKindData::WallCrafter(def) = self {
                    def.boost_item_use_time = 60.0 / 0.75;
                }
            }
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
            "world-message" => self.set_message(400, 24, true),
            "logic-display" => self.set_display(80, 1.0),
            "large-logic-display" => self.set_display(176, 1.0),
            "canvas" => self.set_canvas(12, 3.5, 3, &crate::logic::blocks::canvas::PALETTE),
            // `Blocks.java:6965-6983`: the woodspark 16-colour palette
            // (`setPaletteFromString`).
            "large-canvas" => self.set_canvas(
                24,
                3.5,
                4,
                &[
                    0x452b_3fff,
                    0x8a58_65ff,
                    0xe08d_51ff,
                    0xfabf_61ff,
                    0xf5ee_b0ff,
                    0x2c5e_3bff,
                    0x609c_4fff,
                    0xc6cc_54ff,
                    0x78c2_d6ff,
                    0x5479_b0ff,
                    0x5654_6eff,
                    0x839f_a6ff,
                    0xe0d3_c8ff,
                    0xf05b_5bff,
                    0x8f32_5fff,
                    0xeb6c_98ff,
                ],
            ),
            _ => {}
        }
    }

    fn set_crafter(&mut self, mut crafter: CrafterDef) {
        let target = match self {
            BlockKindData::Crafter(def) => def,
            BlockKindData::AttributeCrafter(def) => &mut def.crafter,
            _ => return,
        };
        // An explicit overlay `craft_time` wins; the `craft()` helper's `0.0`
        // means "keep the class default / content value".
        if crafter.craft_time <= 0.0 {
            crafter.craft_time = target.craft_time;
        }
        *target = crafter;
    }

    /// `Blocks.java` per-block drill boost/warmup overrides (blast drill,
    /// impact drill).
    fn set_drill_boost(&mut self, liquid_boost_intensity: f32, warmup_speed: f32) {
        let def = match self {
            BlockKindData::Drill(def) => def,
            BlockKindData::BurstDrill(def) => &mut def.drill,
            _ => return,
        };
        def.liquid_boost_intensity = liquid_boost_intensity;
        def.warmup_speed = warmup_speed;
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
            BlockKindData::Drill(def) => {
                // `Drill.hardnessDrillMultiplier = 50f` (`Drill.java:28`).
                def.hardness_drill_multiplier = 50.0;
                apply(def);
            }
            BlockKindData::BurstDrill(def) => {
                // `hardnessDrillMultiplier = 0f` (`BurstDrill.java:35-44`).
                def.drill.hardness_drill_multiplier = 0.0;
                apply(&mut def.drill);
            }
            _ => {}
        }
    }

    /// Sets `Pump.pumpAmount` for a plain (`Pump`) family block.
    fn set_pump_amount(&mut self, amount: f32) {
        if let BlockKindData::Pump(def) = self {
            def.pump_amount = amount;
        }
    }

    fn set_wall_crafter(&mut self, drill_time: f32, output: Option<u16>) {
        if let BlockKindData::WallCrafter(def) = self {
            def.drill_time = drill_time;
            def.output = output;
            // `WallCrafter.attribute = Attribute.sand` (the name-keyed form the
            // port's `BlockDef.attributes` uses).
            def.attribute = "sand".to_owned();
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

    fn set_canvas(&mut self, canvas_size: i32, padding: f32, bits_per_pixel: u8, palette: &[u32]) {
        if let BlockKindData::Canvas(def) = self {
            def.canvas_size = canvas_size;
            def.padding = padding;
            def.bits_per_pixel = bits_per_pixel;
            def.palette = palette.to_vec();
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
    use crate::world::block::BlockTable;

    fn crafter_of(inst: &crate::world::block::BlockInstance) -> &CrafterDef {
        match &inst.kind_data {
            BlockKindData::Crafter(def) => def,
            BlockKindData::AttributeCrafter(def) => &def.crafter,
            other => panic!("not a crafter: {other:?}"),
        }
    }

    /// #56: a mod block of a vanilla class inherits the class field
    /// initializers (`X.java`), so it no longer needs a name-keyed
    /// `apply_vanilla_knobs` entry to be functional.
    #[test]
    fn class_defaults_apply_to_mod_blocks() {
        let content = test_registry();
        let base = content.block_by_name("router").expect("router").clone();
        let kind_data = |kind: BlockKind| {
            let mut def = base.clone();
            def.name = String::from("mod-test-block");
            def.kind = kind;
            def.craft_time = 0.0;
            BlockKindData::from_def(&def)
        };

        match kind_data(BlockKind::GenericCrafter) {
            BlockKindData::Crafter(def) => {
                assert_eq!(def.craft_time, 80.0, "GenericCrafter.craftTime");
                assert_eq!(def.liquid_output_directions, vec![-1]);
            }
            other => panic!("expected crafter, got {other:?}"),
        }
        match kind_data(BlockKind::AttributeCrafter) {
            BlockKindData::AttributeCrafter(def) => {
                assert_eq!(def.crafter.craft_time, 80.0);
                assert_eq!(def.base_efficiency, 1.0);
                assert_eq!(def.boost_scale, 1.0);
                assert_eq!(def.min_efficiency, -1.0);
                assert_eq!(def.max_boost, 1.0);
            }
            other => panic!("expected attribute crafter, got {other:?}"),
        }
        match kind_data(BlockKind::Drill) {
            BlockKindData::Drill(def) => {
                assert_eq!(def.drill_time, 300.0, "Drill.drillTime");
                assert_eq!(def.hardness_drill_multiplier, 50.0);
                assert_eq!(def.liquid_boost_intensity, 1.6);
            }
            other => panic!("expected drill, got {other:?}"),
        }
        match kind_data(BlockKind::BurstDrill) {
            BlockKindData::BurstDrill(def) => {
                assert_eq!(def.drill.hardness_drill_multiplier, 0.0);
                assert_eq!(def.inverted_time, 200.0);
            }
            other => panic!("expected burst drill, got {other:?}"),
        }
        match kind_data(BlockKind::BeamDrill) {
            BlockKindData::BeamDrill(def) => {
                assert_eq!((def.range, def.drill_time, def.tier), (5, 200.0, 1));
            }
            other => panic!("expected beam drill, got {other:?}"),
        }
        match kind_data(BlockKind::Pump) {
            BlockKindData::Pump(def) => assert_eq!(def.pump_amount, 0.2),
            other => panic!("expected pump, got {other:?}"),
        }
        match kind_data(BlockKind::Fracker) {
            BlockKindData::Fracker(def) => {
                assert_eq!(def.item_use_time, 100.0);
                assert_eq!(def.pump_amount, 0.2);
                assert_eq!(def.base_efficiency, 1.0);
                assert_eq!(def.result, Some(crate::content::LiquidId::WATER.raw()));
            }
            other => panic!("expected fracker, got {other:?}"),
        }
        match kind_data(BlockKind::Radar) {
            BlockKindData::Radar(def) => assert_eq!(def.discovery_time, 600.0),
            other => panic!("expected radar, got {other:?}"),
        }
        match kind_data(BlockKind::MemoryBlock) {
            BlockKindData::Memory(def) => assert_eq!(def.capacity, 32),
            other => panic!("expected memory, got {other:?}"),
        }
        match kind_data(BlockKind::MessageBlock) {
            BlockKindData::Message(def) => {
                assert_eq!((def.max_text, def.max_newlines), (400, 24));
            }
            other => panic!("expected message, got {other:?}"),
        }
        match kind_data(BlockKind::LogicDisplay) {
            BlockKindData::Display(def) => {
                assert_eq!((def.display_size, def.scale_factor), (64, 1.0));
            }
            other => panic!("expected display, got {other:?}"),
        }
        match kind_data(BlockKind::TileableLogicDisplay) {
            BlockKindData::TileableDisplay(def) => {
                assert_eq!(
                    (def.display_size, def.frame_size, def.max_dimensions),
                    (32, 6, 16)
                );
            }
            other => panic!("expected tileable display, got {other:?}"),
        }
        match kind_data(BlockKind::CanvasBlock) {
            BlockKindData::Canvas(def) => {
                assert_eq!((def.canvas_size, def.bits_per_pixel), (8, 0));
                assert_eq!(def.palette.len(), 8);
            }
            other => panic!("expected canvas, got {other:?}"),
        }
        match kind_data(BlockKind::LaunchPad) {
            BlockKindData::LaunchPad(def) => {
                assert_eq!(def.launch_time, 1.0);
                assert!(!def.accept_multiple_items);
            }
            other => panic!("expected launch pad, got {other:?}"),
        }
    }

    /// gap5 GAP-3 / K-1: every vanilla factory carries its upstream output
    /// stacks and craft time (`Blocks.java` crafting regions).
    #[test]
    fn factory_recipes_match_upstream() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let item = |name: &str| content.item_id(name).map(|id| id.raw()).unwrap_or(u16::MAX);
        let liquid = |name: &str| {
            content
                .liquid_id(name)
                .map(|id| id.raw())
                .unwrap_or(u16::MAX)
        };

        type RecipeCase = (&'static str, Vec<(u16, i32)>, Vec<(u16, f32)>, f32);
        let cases: Vec<RecipeCase> = vec![
            ("graphite-press", vec![(item("graphite"), 1)], vec![], 90.0),
            ("multi-press", vec![(item("graphite"), 2)], vec![], 30.0),
            ("silicon-crucible", vec![(item("silicon"), 8)], vec![], 90.0),
            (
                "plastanium-compressor",
                vec![(item("plastanium"), 1)],
                vec![],
                60.0,
            ),
            (
                "phase-weaver",
                vec![(item("phase-fabric"), 1)],
                vec![],
                120.0,
            ),
            (
                "cryofluid-mixer",
                vec![],
                vec![(liquid("cryofluid"), 12.0 / 60.0)],
                120.0,
            ),
            ("pyratite-mixer", vec![(item("pyratite"), 1)], vec![], 80.0),
            (
                "blast-mixer",
                vec![(item("blast-compound"), 1)],
                vec![],
                80.0,
            ),
            (
                "silicon-arc-furnace",
                vec![(item("silicon"), 4)],
                vec![],
                50.0,
            ),
            (
                "electrolyzer",
                vec![],
                vec![
                    (liquid("ozone"), 4.0 / 60.0),
                    (liquid("hydrogen"), 6.0 / 60.0),
                ],
                10.0,
            ),
            (
                "atmospheric-concentrator",
                vec![],
                vec![(liquid("nitrogen"), 16.0 / 60.0)],
                80.0,
            ),
            (
                "carbide-crucible",
                vec![(item("carbide"), 1)],
                vec![],
                60.0 * 2.25 / 4.0,
            ),
            (
                "slag-centrifuge",
                vec![],
                vec![(liquid("gallium"), 1.0 / 60.0)],
                120.0,
            ),
            (
                "surge-crucible",
                vec![(item("surge-alloy"), 1)],
                vec![],
                45.0,
            ),
            (
                "cyanogen-synthesizer",
                vec![],
                vec![(liquid("cyanogen"), 12.0 / 60.0)],
                20.0,
            ),
            (
                "phase-synthesizer",
                vec![(item("phase-fabric"), 1)],
                vec![],
                30.0,
            ),
        ];

        for (name, items, liquids, craft_time) in cases {
            let inst = table.get_named(name).unwrap_or_else(|| panic!("{name}"));
            let crafter = crafter_of(inst);
            assert_eq!(crafter.output_items, items, "outputs for {name}");
            assert_eq!(crafter.output_liquids, liquids, "liquid outputs for {name}");
            assert_eq!(crafter.craft_time, craft_time, "craft time for {name}");
        }

        // `silicon-crucible` is an `AttributeCrafter` with `baseEfficiency` 1,
        // `boostScale`/`outputScale` 0.15 (`Blocks.java:1086`).
        let crucible = table.get_named("silicon-crucible").expect("crucible");
        match &crucible.kind_data {
            BlockKindData::AttributeCrafter(def) => {
                assert_eq!(def.base_efficiency, 1.0);
                assert_eq!(def.boost_scale, 0.15);
                assert_eq!(def.attribute, 0);
            }
            other => panic!("expected AttributeCrafter, got {other:?}"),
        }
    }

    /// gap5 GAP-9 / K-5: pump amounts and the eruption-drill time are the
    /// upstream `Blocks.java` values (not zero).
    #[test]
    fn pumps_and_eruption_drill_match_upstream() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        for (name, amount) in [
            ("mechanical-pump", 7.0f32 / 60.0),
            ("rotary-pump", 0.2),
            ("impulse-pump", 0.22),
            ("reinforced-pump", 80.0 / 60.0 / 4.0),
        ] {
            let inst = table.get_named(name).unwrap_or_else(|| panic!("{name}"));
            match &inst.kind_data {
                BlockKindData::Pump(def) => {
                    assert_eq!(def.pump_amount, amount, "pump amount for {name}")
                }
                other => panic!("expected Pump for {name}, got {other:?}"),
            }
        }
        let drill = table.get_named("eruption-drill").expect("eruption-drill");
        match &drill.kind_data {
            BlockKindData::BurstDrill(def) => {
                assert_eq!(def.drill.drill_time, 281.25);
                assert_eq!(def.drill.tier, 7);
                assert_eq!(def.inverted_time, 200.0);
            }
            other => panic!("expected BurstDrill, got {other:?}"),
        }
    }

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

    /// GAPS #56 follow-up: the production family knobs are content fields now
    /// (`BlockDef`), so a mod block of a vanilla class carries the JSON values
    /// into `BlockKindData` instead of the class defaults.
    #[test]
    fn content_family_knobs_populate_kind_data() {
        let content = test_registry();
        let item = |name: &str| content.item_id(name).expect("item");
        let liquid = |name: &str| content.liquid_id(name).expect("liquid");
        let base = |name: &str| content.block_by_name(name).expect(name).clone();

        // `GenericCrafter` outputs (`GenericCrafter.java:22-39`).
        let mut crafter = base("graphite-press");
        crafter.name = String::from("mod-test-crafter");
        crafter.craft_time = 0.0;
        crafter.output_items = vec![crate::content::ItemStack::new(item("graphite"), 7)];
        crafter.output_liquids = vec![(liquid("water"), 0.5)];
        crafter.liquid_output_directions = vec![1, 3];
        crafter.ignore_liquid_fullness = true;
        let kind = BlockKindData::from_def(&crafter);
        match &kind {
            BlockKindData::Crafter(def) => {
                assert_eq!(def.craft_time, 80.0, "class default craftTime");
                assert_eq!(def.output_items, vec![(item("graphite").raw(), 7)]);
                assert_eq!(def.output_liquids, vec![(liquid("water").raw(), 0.5)]);
                assert_eq!(def.liquid_output_directions, vec![1, 3]);
                assert!(def.ignore_liquid_fullness);
            }
            other => panic!("expected Crafter, got {other:?}"),
        }
        // Vanilla names keep the `Blocks.java` overlay values, but a mod name is
        // untouched by it (content wins).
        let mut overlaid = kind.clone();
        overlaid.apply_vanilla_knobs(&content, "mod-test-crafter");
        assert_eq!(overlaid, kind);

        // `Drill` (`Drill.java:26-42`).
        let mut drill = base("mechanical-drill");
        drill.name = String::from("mod-test-drill");
        drill.drill_time = Some(111.0);
        drill.drill_tier = Some(4);
        drill.hardness_drill_multiplier = Some(0.0);
        drill.blocked_items = vec![item("thorium")];
        drill.drill_multipliers = vec![(item("beryllium"), 2.0)];
        drill.liquid_boost_intensity = Some(1.9);
        drill.warmup_speed = Some(0.02);
        match BlockKindData::from_def(&drill) {
            BlockKindData::Drill(def) => {
                assert_eq!(def.drill_time, 111.0);
                assert_eq!(def.tier, 4);
                assert_eq!(def.hardness_drill_multiplier, 0.0);
                assert_eq!(def.blocked_items, vec![item("thorium").raw()]);
                assert_eq!(def.drill_multipliers, vec![(item("beryllium").raw(), 2.0)]);
                assert_eq!(def.liquid_boost_intensity, 1.9);
                assert_eq!(def.warmup_speed, 0.02);
            }
            other => panic!("expected Drill, got {other:?}"),
        }

        // `BurstDrill` (`BurstDrill.java:26,35-44`).
        let mut burst = base("impact-drill");
        burst.name = String::from("mod-test-burst");
        burst.drill_time = Some(640.0);
        burst.drill_tier = Some(6);
        burst.inverted_time = Some(120.0);
        match BlockKindData::from_def(&burst) {
            BlockKindData::BurstDrill(def) => {
                assert_eq!(def.drill.drill_time, 640.0);
                assert_eq!(def.drill.tier, 6);
                assert_eq!(def.drill.hardness_drill_multiplier, 0.0);
                assert_eq!(def.inverted_time, 120.0);
            }
            other => panic!("expected BurstDrill, got {other:?}"),
        }

        // `BeamDrill` (`BeamDrill.java:38-41`).
        let mut beam = base("plasma-bore");
        beam.name = String::from("mod-test-beam");
        beam.beam_range = Some(9);
        beam.drill_time = Some(90.0);
        beam.drill_tier = Some(8);
        beam.blocked_items = vec![item("sand")];
        match BlockKindData::from_def(&beam) {
            BlockKindData::BeamDrill(def) => {
                assert_eq!(def.range, 9);
                assert_eq!(def.drill_time, 90.0);
                assert_eq!(def.tier, 8);
                assert_eq!(def.blocked_items, vec![item("sand").raw()]);
            }
            other => panic!("expected BeamDrill, got {other:?}"),
        }

        // `Pump`/`SolidPump`/`Fracker` (`Pump.java:20`, `SolidPump.java:22-26`,
        // `Fracker.java:8`).
        let mut pump = base("rotary-pump");
        pump.name = String::from("mod-test-pump");
        pump.pump_amount = Some(0.37);
        assert_eq!(
            BlockKindData::from_def(&pump),
            BlockKindData::Pump(PumpDef { pump_amount: 0.37 })
        );

        let mut solid = base("water-extractor");
        solid.name = String::from("mod-test-solid-pump");
        solid.pump_amount = Some(0.42);
        solid.attribute = Some(String::from("oil"));
        solid.base_efficiency = Some(0.25);
        solid.pump_liquid = Some(liquid("oil"));
        match BlockKindData::from_def(&solid) {
            BlockKindData::SolidPump(def) => {
                assert_eq!(def.pump_amount, 0.42);
                assert_eq!(def.attribute, "oil");
                assert_eq!(def.base_efficiency, 0.25);
                assert_eq!(def.result, Some(liquid("oil").raw()));
            }
            other => panic!("expected SolidPump, got {other:?}"),
        }

        let mut fracker = base("oil-extractor");
        fracker.name = String::from("mod-test-fracker");
        fracker.item_use_time = Some(45.0);
        fracker.pump_liquid = Some(liquid("slag"));
        match BlockKindData::from_def(&fracker) {
            BlockKindData::Fracker(def) => {
                assert_eq!(def.item_use_time, 45.0);
                assert_eq!(def.result, Some(liquid("slag").raw()));
            }
            other => panic!("expected Fracker, got {other:?}"),
        }

        // `WallCrafter` (`WallCrafter.java:28-49`).
        let mut wall = base("cliff-crusher");
        wall.name = String::from("mod-test-wall-crafter");
        wall.drill_time = Some(80.0);
        wall.attribute = Some(String::from("sand"));
        wall.output_item = Some(item("metaglass"));
        wall.liquid_boost_intensity = Some(1.4);
        wall.item_boost_intensity = Some(1.3);
        wall.boost_item_use_time = Some(70.0);
        match BlockKindData::from_def(&wall) {
            BlockKindData::WallCrafter(def) => {
                assert_eq!(def.drill_time, 80.0);
                assert_eq!(def.attribute, "sand");
                assert_eq!(def.output, Some(item("metaglass").raw()));
                assert_eq!(def.liquid_boost_intensity, 1.4);
                assert_eq!(def.item_boost_intensity, 1.3);
                assert_eq!(def.boost_item_use_time, 70.0);
            }
            other => panic!("expected WallCrafter, got {other:?}"),
        }

        // `AttributeCrafter` (`AttributeCrafter.java:15-31`).
        let mut attribute = base("cultivator");
        attribute.name = String::from("mod-test-attribute");
        attribute.attribute = Some(String::from("steam"));
        attribute.base_efficiency = Some(0.3);
        attribute.boost_scale = Some(0.25);
        attribute.min_efficiency = Some(-0.5);
        attribute.max_boost = Some(3.0);
        attribute.output_scale = Some(0.2);
        attribute.scale_liquid_consumption = Some(true);
        attribute.display_efficiency = Some(false);
        attribute.display_scaled_output = Some(false);
        match BlockKindData::from_def(&attribute) {
            BlockKindData::AttributeCrafter(def) => {
                // `steam` is index 4 in `world::attributes::ATTRIBUTE_NAMES`.
                assert_eq!(def.attribute, 4);
                assert_eq!(def.base_efficiency, 0.3);
                assert_eq!(def.boost_scale, 0.25);
                assert_eq!(def.min_efficiency, -0.5);
                assert_eq!(def.max_boost, 3.0);
                assert_eq!(def.output_scale, 0.2);
                assert!(def.scale_liquid_consumption);
                assert!(!def.display_efficiency);
                assert!(!def.display_scaled_output);
            }
            other => panic!("expected AttributeCrafter, got {other:?}"),
        }

        // `Separator.results` (`Separator.java:20`).
        let mut separator = base("separator");
        separator.name = String::from("mod-test-separator");
        separator.separator_results = vec![crate::content::ItemStack::new(item("metaglass"), 1)];
        match BlockKindData::from_def(&separator) {
            BlockKindData::Separator(def) => {
                assert_eq!(def.results, vec![(item("metaglass").raw(), 1)]);
            }
            other => panic!("expected Separator, got {other:?}"),
        }
    }

    /// PAR-0565 (`Blocks.java:3095-3096,3121-3125`): both vanilla burst drills
    /// override `BurstDrill`'s class defaults — `shake = 4` and a
    /// `MultiEffect(...)` drill effect (impact: red-light mine wave at 40°;
    /// eruption: hydrogen-tinted spikes at 30° plus a hydrogen-tinted wave at
    /// 45°).
    #[test]
    fn vanilla_burst_drills_override_shake_and_drill_effect() {
        use crate::content::EffectId;
        use crate::content::registries::fx_meta::{EffectRef, InlineEffectKind};
        use crate::fx::inline;

        let content = test_registry();
        let burst = |name: &str| {
            let def = content
                .block_by_name(name)
                .unwrap_or_else(|| panic!("{name}"));
            let mut kind = BlockKindData::from_def(def);
            kind.apply_vanilla_knobs(&content, name);
            match kind {
                BlockKindData::BurstDrill(def) => def,
                other => panic!("expected BurstDrill, got {other:?}"),
            }
        };

        // Class defaults (`BurstDrill.java:15,26`): shake 2, shockwave, rnd 0.
        let defaults = BurstDrillDef::default();
        assert_eq!(defaults.shake, 2.0);
        assert_eq!(defaults.drill_effect, EffectRef::Named(EffectId::SHOCKWAVE));
        assert_eq!(defaults.drill_effect_rnd, 0.0);

        let impact = burst("impact-drill");
        assert_eq!(impact.shake, 4.0);
        assert_eq!(impact.drill_effect_rnd, 0.0);
        let children = match &impact.drill_effect {
            EffectRef::Inline(spec) => {
                assert_eq!(spec.kind, InlineEffectKind::MultiEffect);
                spec.children.clone()
            }
            other => panic!("impact-drill drillEffect is a MultiEffect, got {other:?}"),
        };
        assert_eq!(
            children,
            vec![
                inline::named(EffectId::MINE_IMPACT),
                inline::named(EffectId::DRILL_STEAM),
                inline::wrap(
                    inline::named(EffectId::MINE_IMPACT_WAVE),
                    crate::content::registries::pal::RED_LIGHT,
                    40.0,
                ),
            ]
        );

        let hydrogen = content.liquid_by_name("hydrogen").expect("hydrogen").color;
        let eruption = burst("eruption-drill");
        assert_eq!(eruption.shake, 4.0);
        let children = match &eruption.drill_effect {
            EffectRef::Inline(spec) => {
                assert_eq!(spec.kind, InlineEffectKind::MultiEffect);
                spec.children.clone()
            }
            other => panic!("eruption-drill drillEffect is a MultiEffect, got {other:?}"),
        };
        assert_eq!(
            children,
            vec![
                inline::named(EffectId::MINE_IMPACT),
                inline::named(EffectId::DRILL_STEAM),
                inline::wrap(inline::named(EffectId::DYNAMIC_SPIKES), hydrogen, 30.0),
                inline::wrap(inline::named(EffectId::MINE_IMPACT_WAVE), hydrogen, 45.0),
            ]
        );
    }

    /// GAPS #56 follow-up: `TiledFloor`/`TiledWall`/`WobbleProp` carry their
    /// class-specific knobs (`TiledFloor.java:17`, `TiledWall.java:16`,
    /// `WobbleProp.java:9`) and classify as the environment families they
    /// extend.
    #[test]
    fn tiled_and_wobble_kinds_get_class_defaults() {
        let content = test_registry();
        let base = content.block_by_name("router").expect("router").clone();
        let with_kind = |kind: BlockKind| {
            let mut def = base.clone();
            def.name = String::from("mod-env-block");
            def.kind = kind;
            def.max_size = 0;
            def.wobble_scale = None;
            def.wobble_magnitude = None;
            def.wobble_time_scale = None;
            def.wobble_magnitude2 = None;
            def
        };

        // `maxSize = 3` class default.
        match BlockKindData::from_def(&with_kind(BlockKind::TiledFloor)) {
            BlockKindData::Floor(def) => assert_eq!(def.max_size, 3),
            other => panic!("expected Floor, got {other:?}"),
        }
        match BlockKindData::from_def(&with_kind(BlockKind::TiledWall)) {
            BlockKindData::StaticWall(def) => assert_eq!(def.max_size, 3),
            other => panic!("expected StaticWall, got {other:?}"),
        }
        match BlockKindData::from_def(&with_kind(BlockKind::WobbleProp)) {
            BlockKindData::Prop(def) => {
                assert_eq!(def.wobble_scale, 25.0, "WobbleProp.wscl");
                assert_eq!(def.wobble_magnitude, 0.4, "WobbleProp.wmag");
                assert_eq!(def.wobble_time_scale, 1.0, "WobbleProp.wtscl");
                assert_eq!(def.wobble_magnitude2, 1.0, "WobbleProp.wmag2");
            }
            other => panic!("expected Prop, got {other:?}"),
        }

        // Content values win over the class default.
        let mut tiled = with_kind(BlockKind::TiledFloor);
        tiled.max_size = 4;
        match BlockKindData::from_def(&tiled) {
            BlockKindData::Floor(def) => assert_eq!(def.max_size, 4),
            other => panic!("expected Floor, got {other:?}"),
        }
        let mut wobble = with_kind(BlockKind::WobbleProp);
        wobble.wobble_scale = Some(40.0);
        wobble.wobble_magnitude = Some(0.9);
        wobble.wobble_time_scale = Some(2.5);
        wobble.wobble_magnitude2 = Some(0.0);
        match BlockKindData::from_def(&wobble) {
            BlockKindData::Prop(def) => {
                assert_eq!(def.wobble_scale, 40.0);
                assert_eq!(def.wobble_magnitude, 0.9);
                assert_eq!(def.wobble_time_scale, 2.5);
                assert_eq!(def.wobble_magnitude2, 0.0);
            }
            other => panic!("expected Prop, got {other:?}"),
        }

        // Family dispatch stays environment for all three.
        for kind in [
            BlockKind::TiledFloor,
            BlockKind::TiledWall,
            BlockKind::WobbleProp,
        ] {
            assert_eq!(
                BlockKindData::from_def(&with_kind(kind)).family(),
                BlockFamily::Environment,
                "{} family",
                kind.name()
            );
        }
        // `is_floor_kind` is the renderer/placement gate: only `TiledFloor`
        // extends `Floor`.
        assert!(crate::content::is_floor_kind(BlockKind::TiledFloor));
        assert!(!crate::content::is_floor_kind(BlockKind::TiledWall));
        assert!(!crate::content::is_floor_kind(BlockKind::WobbleProp));
    }
}

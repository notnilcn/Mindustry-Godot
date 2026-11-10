// SPDX-License-Identifier: GPL-3.0-only

//! `BlockSpec`: the generated per-block input record and its class defaults.
//! Upstream: core/src/mindustry/content/Blocks.java, core/src/mindustry/world/Block.java

use super::*;

use crate::content::category::Category;
use crate::content::color::Rgba;
use crate::render::layer::{BuildingCacheLayer, CacheLayerId};

use super::drawer::DrawerSpec;

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
    /// Core unit type spawned on player spawn/respawn (`CoreBlock.unitType`).
    pub core_unit: Option<&'static str>,
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
    /// `Block.drawer` (`DrawBlock` tree; plan 17 executes it).
    pub drawer: Option<DrawerSpec>,
    /// `Drill.drillMultipliers` (item name -> multiplier).
    pub drill_multipliers: Vec<(&'static str, f32)>,
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
    /// `Block.solidifes` (sic, `Block.java:116`): blocks placement under units
    /// like a solid block does (`Build.checkNoUnitOverlap`). True for doors.
    pub solidifies: Option<bool>,
    /// Can float on liquids.
    pub floating: Option<bool>,
    /// `CoreBlock.incinerateNonBuildable`: drop non-buildable items on intake
    /// (Erekir cores). Default false.
    pub incinerate_non_buildable: Option<bool>,
    /// `CoreBlock.requiresCoreZone`. Default false.
    pub requires_core_zone: Option<bool>,
    /// `Block.ignoreBuildDarkness`. Default false.
    pub ignore_build_darkness: Option<bool>,
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
    /// `Block.canOverdrive` (`Block.java:314`; default true).
    pub can_overdrive: Option<bool>,
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
    /// `UnlockableContent.alwaysUnlocked` (bypasses the stored unlock flag).
    pub always_unlocked: Option<bool>,
    /// Map color.
    pub map_color: Option<Rgba>,
    /// `Block.albedo`: the mesh alpha is `1 - albedo` (planet globe tint).
    pub albedo: Option<f32>,
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
    /// `Block.lightRadius` (`Block.java:350`; default 60, class/`Blocks.java`
    /// overrides applied by [`java_light_defaults`]).
    pub light_radius: Option<f32>,
    /// `Block.lightColor` (`Block.java:344`; default white; the alpha is the
    /// environment light's opacity, `drawEnvironmentLight`).
    pub light_color: Option<Rgba>,
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
    /// `Floor.autotile` (J02 T2): the 8-neighbour `TileBitmask` slice pick.
    pub autotile: Option<bool>,
    /// `Floor.autotileVariants` (`TileBitmask.loadVariants`; J02 T2).
    pub autotile_variants: Option<i32>,
    /// `Floor.autotileMidVariants` (the bit-13 mid slices; J02 T2).
    pub autotile_mid_variants: Option<i32>,
    /// `Floor.tilingVariants` (`<name>-tile<i+1>` layers; J02 T2).
    pub tiling_variants: Option<i32>,
    /// `Floor.overlayAlpha` (default 0.65; the liquid redraw runs at
    /// `1 - overlayAlpha`, `Floor.java:257-261`; J02 T2).
    pub overlay_alpha: Option<f32>,
    /// `Floor.shallow` (`ShallowLiquid.set` sets true; plan 11 §3.6).
    pub shallow: Option<bool>,
    /// `Floor.drownTime` (ticks to drown; plan 11 `isDeep`/drown; plan 10).
    pub drown_time: Option<f32>,
    /// `Floor.status` (status name; plan G01 T13).
    pub floor_status: Option<&'static str>,
    /// `Floor.statusDuration` (default 60; G01 T13).
    pub floor_status_duration: Option<f32>,
    /// `Floor.dragMultiplier` (default 1; G01 T13).
    pub floor_drag_multiplier: Option<f32>,
    /// `Floor.damageTaken` (default 0; G01 T13).
    pub floor_damage_taken: Option<f32>,
    /// `Floor.liquidMultiplier` (deep-water pump yield; plan 10/11).
    pub liquid_multiplier: Option<f32>,
    /// `Block.crushFragile` (instantly destroyed by crushing tanks; plan 11).
    pub crush_fragile: Option<bool>,
    /// `Block.crushDamageMultiplier` (tank-crush damage scaling; plan 11).
    pub crush_damage_multiplier: Option<f32>,
    /// `Block.unitMoveBreakable` (deconstructed by moving units; plan 11).
    pub unit_move_breakable: Option<bool>,
    /// `Block.depositCooldown` (`-1` = use the global rule; J04 T5).
    pub deposit_cooldown: Option<f32>,
    /// `Block.teamPassable` (`AutoDoor`; pathfinder `teamPassable` bit).
    pub team_passable: Option<bool>,
    /// `Block.alwaysReplace` (plan 07 `Block.canReplace`).
    pub always_replace: Option<bool>,
    /// `Block.privileged` (world processors/cells; plan 07/13).
    pub privileged: Option<bool>,
    /// `Block.replaceable` (`canReplace`; plan 07).
    pub replaceable: Option<bool>,
    /// `Block.quickRotate` (`canReplace` self-replacement; plan 07).
    pub quick_rotate: Option<bool>,
    /// `Block.rotate` (`Block.java:124`): whether the block is rotatable.
    /// `None` derives it from the block kind (`kind_rotates`).
    pub rotate: Option<bool>,
    /// `Block.rotateDraw` (`Block.java:126`): rotate the region while drawing.
    pub rotate_draw: Option<bool>,
    /// `Block.rotateDrawEditor` (`Block.java:128`): rotate the region in the map
    /// editor.
    pub rotate_draw_editor: Option<bool>,
    /// `Block.visualRotationOffset` (`Block.java:130`): degrees added to the
    /// drawn rotation (broken-plan rendering).
    pub visual_rotation_offset: Option<f32>,
    /// Java `Block.subclass` proxy; plan 07 uses `kind` equality when unset.
    pub subclass: Option<BlockKind>,
    /// `GenericCrafter.craftTime` (ticks); plan 07 R2 reconciliation.
    pub craft_time: Option<f32>,
    /// `Reconstructor.constructTime` (ticks).
    pub construct_time: Option<f32>,
    /// `Block.commandable` (unit command center; plan 11/15 `commandBuilding`).
    pub commandable: Option<bool>,
    /// Floor/environment attributes (`Floor.attributes`; plan 09 R3 steam vents).
    pub attributes: Vec<(&'static str, f32)>,
    /// `Floor.blendGroup` content name (`Floor.java:62`; omitted = the block
    /// itself). J02 T1: the `-edge` texture group and the blend-id source.
    pub blend_group: Option<&'static str>,
    /// `Floor.drawEdgeIn` (`Floor.java:92`; Java default true).
    pub draw_edge_in: Option<bool>,
    /// `Floor.drawEdgeOut` (`Floor.java:94`; Java default true).
    pub draw_edge_out: Option<bool>,
    /// `Block.useColor` (`Block.java:183`; Java default true, `OverlayFloor`
    /// and `AirBlock` clear it, `OreBlock` re-enables it). J02 T3:
    /// `MapIO.colorFor` uses the overlay's value.
    pub use_color: Option<bool>,
    /// Floors whose `Floor.decoration` is this block. Java assigns it from the
    /// prop's initialiser (`Blocks.java:705-822`, e.g. `boulder`:
    /// `stone.asFloor().decoration = craters.asFloor().decoration =
    /// charr.asFloor().decoration = this`); the floors register first, so the
    /// registry sink writes the back-reference when this block is pushed
    /// (L02 T1).
    pub decorates: Vec<&'static str>,
    /// `Turret.playerControllable` (`Turret.java:103`, default true; mod knob).
    pub player_controllable: Option<bool>,

    // ---- production family knobs (append-only; GAPS #56 follow-up) ----
    //
    // The per-family numbers below were previously reachable only through the
    // name-keyed `BlockKindData::apply_vanilla_knobs` overlay. They are content
    // data upstream (`Blocks.java` / mod JSON `type` classes), so they live on
    // the record now: `BlockKindData::from_def` copies them and falls back to
    // the Java class default ([`crate::world::block_kind_data`]) when `None`.
    /// `GenericCrafter.outputItems`/`Separator.results`-adjacent item outputs
    /// (`GenericCrafter.java:22`; the `outputItem(...)`/`outputItems(...)`
    /// builders). Empty = no item output.
    pub output_items: Vec<StackSpec>,
    /// `GenericCrafter.outputLiquids` (`GenericCrafter.java:23`): amounts are
    /// per tick (Java stores `amount / 60f` in the builder).
    pub output_liquids: Vec<LiquidStackSpec>,
    /// `GenericCrafter.liquidOutputDirections` (`GenericCrafter.java:35`): one
    /// relative direction per liquid output; empty = Java's `{-1}` (all sides).
    pub liquid_output_directions: Vec<i8>,
    /// `GenericCrafter.ignoreLiquidFullness` (`GenericCrafter.java:37`).
    pub ignore_liquid_fullness: Option<bool>,
    /// `Separator.results` (`Separator.java:20`, an `ItemStack[]`; the class
    /// derives `hasItems`/`hasLiquids` from it in `init`).
    pub separator_results: Vec<StackSpec>,
    /// `Drill.drillTime` (frames per ore; `Drill.java:34`). Also read by
    /// `BurstDrill` (inherits) and `BeamDrill.drillTime`
    /// (`BeamDrill.java:38`) and `WallCrafter.drillTime` (`WallCrafter.java:30`).
    pub drill_time: Option<f32>,
    /// `Drill.tier` (`Drill.java:30`): maximum ore hardness tier. Also
    /// `BeamDrill.tier` (`BeamDrill.java:40`).
    pub drill_tier: Option<i32>,
    /// `Drill.hardnessDrillMultiplier` (`Drill.java:26`, default 50; `0` for
    /// `BurstDrill`, `BurstDrill.java:35-44`).
    pub hardness_drill_multiplier: Option<f32>,
    /// `Drill.blockedItems` (`Drill.java:42`; the `blockedItem` single-item
    /// form is folded in by the caller). Also `BeamDrill.blockedItems`.
    pub blocked_items: Vec<&'static str>,
    /// `Drill.liquidBoostIntensity` (`Drill.java:36`, default 1.6). Also
    /// `WallCrafter.liquidBoostIntensity` (`WallCrafter.java:32`).
    pub liquid_boost_intensity: Option<f32>,
    /// `Drill.warmupSpeed` (`Drill.java:38`, default 0.015).
    pub warmup_speed: Option<f32>,
    /// `BurstDrill.invertedTime` (`BurstDrill.java:26`, default 200).
    pub inverted_time: Option<f32>,
    /// `BurstDrill.shake` (`BurstDrill.java:15`, default 2): the burst's
    /// `Effect.shake` intensity (`Blocks.java:3096,3125` set 4 for both vanilla
    /// burst drills).
    pub burst_shake: Option<f32>,
    /// `BurstDrill.drillEffect` (`BurstDrill.java:26`, default `Fx.shockwave`);
    /// the vanilla drills install a `MultiEffect(...)` (`Blocks.java:3095,
    /// 3121-3124`).
    pub burst_drill_effect: Option<crate::content::registries::fx_meta::EffectRef>,
    /// `BeamDrill.range` (`BeamDrill.java:39`, default 5): drill reach in tiles.
    pub beam_range: Option<i32>,
    /// `Pump.pumpAmount` (`Pump.java:20`, default 0.2): per-tile liquid yield.
    /// Also `SolidPump`/`Fracker` (inherited).
    pub pump_amount: Option<f32>,
    /// `Fracker.itemUseTime` (`Fracker.java:8`, default 100): ticks between
    /// item consumptions.
    pub item_use_time: Option<f32>,
    /// `SolidPump.result` (`SolidPump.java:22`, default `Liquids.water`): the
    /// liquid the pump produces. `Fracker` inherits it.
    pub pump_liquid: Option<&'static str>,
    /// `SolidPump.baseEfficiency` (`SolidPump.java:26`, default 1). Also the
    /// `AttributeCrafter.baseEfficiency` field (`AttributeCrafter.java:17`).
    pub base_efficiency: Option<f32>,
    /// `SolidPump.attribute`/`Fracker`/`WallCrafter.attribute`/
    /// `AttributeCrafter.attribute` (`Attribute` name, e.g. `water`, `sand`,
    /// `spores`, `steam`); `None` = Java `null` (no attribute check).
    pub attribute: Option<&'static str>,
    /// `WallCrafter.output` (`WallCrafter.java:40`, default `Items.sand`).
    pub output_item: Option<&'static str>,
    /// `WallCrafter.itemBoostIntensity` (`WallCrafter.java:46`, default 1.6).
    pub item_boost_intensity: Option<f32>,
    /// `WallCrafter.boostItemUseTime` (`WallCrafter.java:44`, default 120):
    /// ticks between item-booster consumptions.
    pub boost_item_use_time: Option<f32>,
    /// `AttributeCrafter.boostScale` (`AttributeCrafter.java:19`, default 1).
    pub boost_scale: Option<f32>,
    /// `AttributeCrafter.minEfficiency` (`AttributeCrafter.java:21`, default
    /// -1).
    pub min_efficiency: Option<f32>,
    /// `AttributeCrafter.maxBoost` (`AttributeCrafter.java:23`, default 1).
    pub max_boost: Option<f32>,
    /// `AttributeCrafter.outputScale` (`AttributeCrafter.java:25`; `0` = the
    /// output is not scaled by efficiency).
    pub output_scale: Option<f32>,
    /// `AttributeCrafter.scaleLiquidConsumption` (`AttributeCrafter.java:27`).
    pub scale_liquid_consumption: Option<bool>,
    /// `AttributeCrafter.displayEfficiency` (`AttributeCrafter.java:29`).
    pub display_efficiency: Option<bool>,
    /// `AttributeCrafter.displayScaledOutput` (`AttributeCrafter.java:31`).
    pub display_scaled_output: Option<bool>,

    // ---- environment family knobs (TiledFloor/TiledWall/WobbleProp) ----
    /// `Block.variants` (`Block.java:136`, default 0): sprite variant count
    /// (`variantRegions`). `TiledFloor(name, variants, maxSize)` and
    /// `TiledWall(name, variants, maxSize)` set it (`TiledFloor.java:21`).
    pub variants: Option<i32>,
    /// `TiledFloor.maxSize`/`TiledWall.maxSize` (`TiledFloor.java:17`,
    /// `TiledWall.java:16`, default 3): largest packed tile block the renderer
    /// may assemble.
    pub max_size: Option<i32>,
    /// `WobbleProp.wscl` (`WobbleProp.java:9`, default 25).
    pub wobble_scale: Option<f32>,
    /// `WobbleProp.wmag` (`WobbleProp.java:9`, default 0.4).
    pub wobble_magnitude: Option<f32>,
    /// `WobbleProp.wtscl` (`WobbleProp.java:9`, default 1).
    pub wobble_time_scale: Option<f32>,
    /// `WobbleProp.wmag2` (`WobbleProp.java:9`, default 1).
    pub wobble_magnitude2: Option<f32>,
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
            core_unit: None,
            flags: Vec::new(),
            consumes: Vec::new(),
            unit_plans: Vec::new(),
            upgrades: Vec::new(),
            assembler_plans: Vec::new(),
            drawer: None,
            drill_multipliers: Vec::new(),
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
            solidifies: None,
            floating: None,
            incinerate_non_buildable: None,
            requires_core_zone: None,
            ignore_build_darkness: None,
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
            can_overdrive: None,
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
            always_unlocked: None,
            map_color: None,
            albedo: None,
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
            light_radius: None,
            light_color: None,
            draw_team_overlay: None,
            liquid_drop: None,
            speed_multiplier: None,
            is_liquid: None,
            autotile: None,
            autotile_variants: None,
            autotile_mid_variants: None,
            tiling_variants: None,
            overlay_alpha: None,
            shallow: None,
            drown_time: None,
            floor_status: None,
            floor_status_duration: None,
            floor_drag_multiplier: None,
            floor_damage_taken: None,
            liquid_multiplier: None,
            crush_fragile: None,
            crush_damage_multiplier: None,
            unit_move_breakable: None,
            deposit_cooldown: None,
            team_passable: None,
            always_replace: None,
            privileged: None,
            replaceable: None,
            quick_rotate: None,
            rotate: None,
            rotate_draw: None,
            rotate_draw_editor: None,
            visual_rotation_offset: None,
            subclass: None,
            craft_time: None,
            construct_time: None,
            commandable: None,
            attributes: Vec::new(),
            blend_group: None,
            draw_edge_in: None,
            draw_edge_out: None,
            use_color: None,
            decorates: Vec::new(),
            player_controllable: None,
            // Production family knobs (class defaults live in
            // `world::block_kind_data`, applied when these stay unset).
            output_items: Vec::new(),
            output_liquids: Vec::new(),
            liquid_output_directions: Vec::new(),
            ignore_liquid_fullness: None,
            separator_results: Vec::new(),
            drill_time: None,
            drill_tier: None,
            hardness_drill_multiplier: None,
            blocked_items: Vec::new(),
            liquid_boost_intensity: None,
            warmup_speed: None,
            inverted_time: None,
            burst_shake: None,
            burst_drill_effect: None,
            beam_range: None,
            pump_amount: None,
            item_use_time: None,
            pump_liquid: None,
            base_efficiency: None,
            attribute: None,
            output_item: None,
            item_boost_intensity: None,
            boost_item_use_time: None,
            boost_scale: None,
            min_efficiency: None,
            max_boost: None,
            output_scale: None,
            scale_liquid_consumption: None,
            display_efficiency: None,
            display_scaled_output: None,
            variants: None,
            max_size: None,
            wobble_scale: None,
            wobble_magnitude: None,
            wobble_time_scale: None,
            wobble_magnitude2: None,
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
        // `Floor.java:117`: every `Floor` subclass clears `obstructsLight` in
        // its constructor (so floors and their overlays can light the tile,
        // `BlockRenderer.java:846-855`); walls/props keep the base `true`.
        if matches!(
            kind,
            BlockKind::Floor
                | BlockKind::AirBlock
                | BlockKind::EmptyFloor
                | BlockKind::OverlayFloor
                | BlockKind::OreBlock
                | BlockKind::RemoveOre
                | BlockKind::SpawnBlock
                | BlockKind::CharacterOverlay
                | BlockKind::RuneOverlay
                | BlockKind::ColoredFloor
                | BlockKind::ShallowLiquid
                | BlockKind::SteamVent
        ) {
            spec.obstructs_light = Some(false);
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
                // `RemoveWall.java:19`.
                spec.ignore_build_darkness = Some(true);
            }
            BlockKind::RemoveOre => {
                spec.placeable_liquid = Some(true);
                spec.in_editor = Some(false);
                // `RemoveOre.java:19` (`OverlayFloor` already sets it).
                spec.ignore_build_darkness = Some(true);
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
                // `Floor.java:116`: `ignoreBuildDarkness = true` (every floor).
                spec.ignore_build_darkness = Some(true);
            }
            BlockKind::EmptyFloor => {
                spec.placeable_liquid = Some(true);
                spec.placeable_on = Some(false);
                spec.solid = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
                // `EmptyFloor extends Floor`; `EmptyFloor.java:15`: `drawEdgeOut = false`.
                spec.draw_edge_out = Some(false);
                spec.ignore_build_darkness = Some(true);
            }
            BlockKind::OverlayFloor => {
                spec.placeable_liquid = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
                // `OverlayFloor extends Floor`.
                spec.ignore_build_darkness = Some(true);
            }
            BlockKind::OreBlock => {
                spec.placeable_liquid = Some(true);
                spec.ore_threshold = Some(0.828);
                spec.ore_scale = Some(24.0);
                // `OreBlock extends OverlayFloor extends Floor`.
                spec.ignore_build_darkness = Some(true);
            }
            BlockKind::StaticWall => {
                spec.solid = Some(true);
                spec.placeable_liquid = Some(true);
                spec.cache_layer = Some(CacheLayerId::Walls);
                // `StaticWall.java:35`.
                spec.ignore_build_darkness = Some(true);
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
                // `Door.java:35`: `solidifes = true`.
                spec.solidifies = Some(true);
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
                // `AutoDoor extends Door`: inherits `solidifes = true`.
                spec.solidifies = Some(true);
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

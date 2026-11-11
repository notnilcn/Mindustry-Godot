// SPDX-License-Identifier: GPL-3.0-only

//! `BlockDef`: the resolved block metadata record and its spec resolution.
//! Upstream: core/src/mindustry/world/Block.java

use super::*;

use crate::content::bundle::BundleView;
use crate::content::category::Category;
use crate::content::color::Rgba;
use crate::content::ctype::{ModContentInfo, UnlockFields};
use crate::content::id::{BlockId, ItemId, LiquidId, UnitTypeId};
use crate::content::registries::ContentRegistry;
use crate::content::registries::planets::EnvFlag;
use crate::content::settings_store::UnlockStore;
use crate::content::stacks::{ItemStack, LiquidStack, PayloadStack, round_to_i32};
use crate::content::{ContentError, ContentRef, ContentType};
use crate::render::layer::{BuildingCacheLayer, CacheLayerId};

use super::BASE_UNIT_CAP_MODIFIER;
use super::drawer::DrawerSpec;
use super::{BlockKind, BlockSpec, Consume, ConsumeDef, ConsumeSpec};

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
    /// `Block.underBullets`: bullets fly over this block unless aimed at it
    /// (`Block.java:120`, `BulletComp.checkUnderBuild`).
    pub under_bullets: bool,
    /// `Block.rebuildable`: dead blocks leave a rebuild ghost
    /// (`Block.java:148`, `Logic.java:42-50`).
    pub rebuildable: bool,
    /// `Block.deconstructDropAllLiquid` (`Block.java:181`, default false; no
    /// vanilla block sets it): drops every stored liquid on deconstruct even
    /// when incinerable (`BuildingComp.onDeconstructed`,
    /// `BuildingComp.java:1480-1487`).
    pub deconstruct_drop_all_liquid: bool,
    /// `CoreBlock.incinerateNonBuildable` (`CoreBlock.java:54`; true for the
    /// three Erekir cores, `Blocks.java:3197,3216,3234`): drops non-buildable
    /// items on intake instead of storing them.
    pub incinerate_non_buildable: bool,
    /// `CoreBlock.requiresCoreZone` (`CoreBlock.java:53`; true for the three
    /// Erekir cores, `Blocks.java:3198,3218,3236`): the upgrade branch of
    /// `canPlaceOn` requires every footprint floor to allow core placement.
    pub requires_core_zone: bool,
    /// `Block.ignoreBuildDarkness` (`Block.java:377`; true for floors, static
    /// walls, remove-wall/remove-ore, `Floor.java:116`, `StaticWall.java:35`,
    /// `RemoveWall.java:19`, `RemoveOre.java:19`): skips the darkness gate and
    /// the solid-floor gate of `Build.validPlace` (`Build.java:229,257`).
    pub ignore_build_darkness: bool,
    /// `Block.ignoreResizeConfig` (`Block.java:264-265`): `MapEditor.resize`
    /// skips the `BuildPlan.pointConfig` shift for this block
    /// (`MapEditor.java:342-345`). True for the classes whose constructor sets
    /// it — `LogicBlock` (`LogicBlock.java:58`), `ItemBridge`
    /// (`ItemBridge.java:61`) and `PowerNode` (`PowerNode.java:45`) — with
    /// subclasses inheriting it (`kind_ignores_resize_config`).
    pub ignore_resize_config: bool,
    /// Unit cap contribution.
    pub unit_cap_modifier: i32,
    /// Core unit type spawned on player spawn/respawn (`CoreBlock.unitType`).
    pub core_unit: Option<&'static str>,
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
    /// `Block.drawer` (`DrawBlock` tree; plan 17 executes it).
    pub drawer: DrawerSpec,
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
    /// `Block.solidifes` (`Block.java:116`): placement is refused when a ground
    /// unit overlaps the footprint (`Build.checkNoUnitOverlap`). True for the
    /// doors (`Door.java:35`).
    pub solidifies: bool,
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
    /// `Block.canOverdrive` (`Block.java:314`).
    pub can_overdrive: bool,
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
    /// `Block.rotate`: this block is rotatable.
    pub rotate: bool,
    /// `Block.rotateDraw`: the region rotates with the block (default true).
    pub rotate_draw: bool,
    /// `Block.rotateDrawEditor`: the region rotates in the editor (default true).
    pub rotate_draw_editor: bool,
    /// `Block.visualRotationOffset`: degrees added when drawing.
    pub visual_rotation_offset: f32,
    /// Centered iteration offset (`Block.sizeOffset`).
    pub size_offset: i32,
    /// Stat data (`setStats`).
    pub stats: Vec<StatSpec>,
    /// Bar data (`setBars`).
    pub bars: Vec<BarSpec>,
    /// Map color.
    pub map_color: Option<Rgba>,
    /// `Block.albedo` (`1f - albedo` is the planet-mesh alpha; default 0).
    pub albedo: f32,
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
    /// `Block.lightRadius` (`Block.java:350`): the environment light radius
    /// emitted by `drawEnvironmentLight` (`Block.java:493-495`).
    pub light_radius: f32,
    /// `Block.lightColor` (`Block.java:344`): the environment light colour; the
    /// alpha is the light opacity (`drawEnvironmentLight`'s `lightColor.a`).
    pub light_color: Rgba,
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
    /// `Floor.autotile` (`Floor.drawMain`'s bitmask branch; J02 T2).
    pub autotile: bool,
    /// `Floor.autotileVariants` (`TileBitmask.loadVariants` rows; J02 T2).
    pub autotile_variants: i32,
    /// `Floor.autotileMidVariants` (the bit-13 mid slices; J02 T2).
    pub autotile_mid_variants: i32,
    /// `Floor.tilingVariants` (`<name>-tile<i+1>` layers; J02 T2).
    pub tiling_variants: i32,
    /// `Floor.overlayAlpha` (default 0.65; J02 T2).
    pub overlay_alpha: f32,
    /// `Floor.shallow` (plan 11 §3.6 shallow-speed rule).
    pub shallow: bool,
    /// `Floor.isDeep()` (`drownTime > 0`; plan 11 §3.6 deep-floor dispatch).
    pub is_deep: bool,
    /// `Floor.drownTime` (plan 10/11 drowning).
    pub drown_time: f32,
    /// `Floor.status` (G01 T13 floor statuses).
    pub floor_status: Option<crate::content::StatusId>,
    /// `Floor.statusDuration` (Java default 60).
    pub floor_status_duration: f32,
    /// `Floor.dragMultiplier` (1 unless ice; G01 T13).
    pub floor_drag_multiplier: f32,
    /// `Floor.damageTaken` (0 for vanilla; G01 T13).
    pub floor_damage_taken: f32,
    /// `Floor.liquidMultiplier` (plan 10 pump yield).
    pub liquid_multiplier: f32,
    /// `Block.crushFragile` (plan 11 tank crush).
    pub crush_fragile: bool,
    /// `Block.crushDamageMultiplier` (plan 11 tank crush damage; `Wall` = 5).
    pub crush_damage_multiplier: f32,
    /// `Block.unitMoveBreakable` (plan 11 unit deconstruct).
    pub unit_move_breakable: bool,
    /// `Block.depositCooldown` (`-1` = use `Rules.itemDepositCooldown`;
    /// `InputHandler.canDepositItem`, J04 T5).
    pub deposit_cooldown: f32,
    /// `Block.teamPassable` (H01 T15 pathfinder bit; `AutoDoor.java:38`).
    pub team_passable: bool,
    /// `Block.alwaysReplace` (plan 07 `can_replace`).
    pub always_replace: bool,
    /// `Block.privileged` (plan 07/13 world processors).
    pub privileged: bool,
    /// `Block.autoResetEnabled` (`Block.java:170`; false for switches,
    /// `SwitchBlock.java:23`): re-enabled by `World.checkMapArea` when a map
    /// area shrinks (`World.java:469-481`).
    pub auto_reset_enabled: bool,
    /// `Block.baseExplosiveness` (`Block.java:197`; `PowerGenerator` 5,
    /// batteries 1/5 — H09 T7).
    pub base_explosiveness: f32,
    /// `Block.explosivenessScale` (`Block.java:199`; the conduit family
    /// fractions from `Blocks.java:2327-2458`).
    pub explosiveness_scale: f32,
    /// `Block.flammabilityScale` (`Block.java:200`).
    pub flammability_scale: f32,
    /// `Block.replaceable` (plan 07 `can_replace`).
    pub replaceable: bool,
    /// `Block.quickRotate` (plan 07 `can_replace` self-replacement).
    pub quick_rotate: bool,
    /// Java `Block.subclass` proxy (plan 07 uses `kind` equality when `kind`).
    pub subclass: BlockKind,
    /// `GenericCrafter.craftTime` (ticks); plan 07 R2 reconciliation.
    pub craft_time: f32,
    /// `Reconstructor.constructTime` (ticks).
    pub construct_time: f32,
    /// `Block.commandable` (unit command center; plan 11/15 `commandBuilding`).
    pub commandable: bool,
    /// `Floor.blendGroup` (`Floor.java:62`) resolved to an id; `None` = the
    /// block itself (`Floor.init`: `blendId = blendGroup.id`, `Floor.java:190`).
    pub blend_group: Option<BlockId>,
    /// `Floor.drawEdgeIn` (`Floor.java:92`): this floor draws other floors'
    /// edges on itself (`Floor.drawBase`).
    pub draw_edge_in: bool,
    /// `Floor.drawEdgeOut` (`Floor.java:94`): this floor's edges draw onto
    /// other floors (`Floor.drawEdges` / `drawNonLayer`).
    pub draw_edge_out: bool,
    /// Whether a `<name>-edge` region exists (`Floor.load`, `Floor.java:170`).
    /// The upstream `edges` generator makes one for every non-overlay floor
    /// with `blendGroup == this && drawEdgeOut` (`Generators.java:827-847`).
    pub has_edge: bool,
    /// `Block.useColor` (`Block.java:183`): whether `MapIO.colorFor` uses this
    /// block's `mapColor` for an overlay (J02 T3).
    pub use_color: bool,
    /// `Floor.decoration` (`Floor.java:70`, default `Blocks.air`): the prop the
    /// generators scatter on this floor (`BasicGenerator.decoration`, the
    /// default `ScatterFilter`s of `Maps.readFilters`, `SerpuloPlanetGenerator`).
    /// Set by the decorating prop (`BlockSpec::decorates`) or a mod's
    /// `decoration` field; `AIR` = none (L02 T1).
    pub decoration: BlockId,
    /// `Turret.playerControllable` (`Turret.java:103`; `canControl()`,
    /// `:382-384`): whether the player may possess this turret's proxy unit.
    /// Default true; no vanilla turret clears it (mods may). L06 T2.
    pub player_controllable: bool,

    // ---- production family knobs (append-only; GAPS #56 follow-up) ----
    //
    // Resolved content values with the Java *content* semantics: `None`/empty
    // means "the JSON/mod content did not set it", and
    // [`crate::world::block_kind_data::BlockKindData::from_def`] then applies
    // the upstream class field initializer. They replace the name-keyed
    // `apply_vanilla_knobs` overlay as the source of truth.
    /// `GenericCrafter.outputItems` (`GenericCrafter.java:22`).
    pub output_items: Vec<ItemStack>,
    /// `GenericCrafter.outputLiquids` (`GenericCrafter.java:23`): per-tick
    /// amounts (`(liquid, amount / 60f)` upstream).
    pub output_liquids: Vec<(LiquidId, f32)>,
    /// `GenericCrafter.liquidOutputDirections` (`GenericCrafter.java:35`); empty
    /// = the Java default `{-1}` (dump/behaviour code applies the fallback).
    pub liquid_output_directions: Vec<i8>,
    /// `GenericCrafter.ignoreLiquidFullness` (`GenericCrafter.java:37`).
    pub ignore_liquid_fullness: bool,
    /// `Separator.results` (`Separator.java:20`).
    pub separator_results: Vec<ItemStack>,
    /// `Drill.drillTime` (also `BurstDrill`, `BeamDrill.drillTime`,
    /// `WallCrafter.drillTime`); `None` = class default.
    pub drill_time: Option<f32>,
    /// `Drill.tier` (also `BeamDrill.tier`); `None` = class default.
    pub drill_tier: Option<i32>,
    /// `Drill.hardnessDrillMultiplier`; `None` = class default (50, 0 for
    /// burst drills).
    pub hardness_drill_multiplier: Option<f32>,
    /// `Drill.blockedItems` (`blockedItem` folded in).
    pub blocked_items: Vec<ItemId>,
    /// `Drill.liquidBoostIntensity` / `WallCrafter.liquidBoostIntensity`.
    pub liquid_boost_intensity: Option<f32>,
    /// `Drill.warmupSpeed`.
    pub warmup_speed: Option<f32>,
    /// `BurstDrill.invertedTime`.
    pub inverted_time: Option<f32>,
    /// `BurstDrill.shake`.
    pub burst_shake: Option<f32>,
    /// `BurstDrill.drillEffect` (a named `Fx` id or an inline `MultiEffect`).
    pub burst_drill_effect: Option<crate::content::registries::fx_meta::EffectRef>,
    /// `BeamDrill.range`.
    pub beam_range: Option<i32>,
    /// `Pump.pumpAmount` (also `SolidPump`/`Fracker`).
    pub pump_amount: Option<f32>,
    /// `Fracker.itemUseTime`.
    pub item_use_time: Option<f32>,
    /// `SolidPump.result` (`Fracker` inherits it).
    pub pump_liquid: Option<LiquidId>,
    /// `SolidPump.baseEfficiency` / `AttributeCrafter.baseEfficiency`.
    pub base_efficiency: Option<f32>,
    /// Family `attribute` (`Attribute` name; `SolidPump`/`Fracker`/
    /// `WallCrafter`/`AttributeCrafter`).
    pub attribute: Option<String>,
    /// `WallCrafter.output`.
    pub output_item: Option<ItemId>,
    /// `WallCrafter.itemBoostIntensity`.
    pub item_boost_intensity: Option<f32>,
    /// `WallCrafter.boostItemUseTime`.
    pub boost_item_use_time: Option<f32>,
    /// `AttributeCrafter.boostScale`.
    pub boost_scale: Option<f32>,
    /// `AttributeCrafter.minEfficiency`.
    pub min_efficiency: Option<f32>,
    /// `AttributeCrafter.maxBoost`.
    pub max_boost: Option<f32>,
    /// `AttributeCrafter.outputScale`.
    pub output_scale: Option<f32>,
    /// `AttributeCrafter.scaleLiquidConsumption`.
    pub scale_liquid_consumption: Option<bool>,
    /// `AttributeCrafter.displayEfficiency`.
    pub display_efficiency: Option<bool>,
    /// `AttributeCrafter.displayScaledOutput`.
    pub display_scaled_output: Option<bool>,

    // ---- environment family knobs (TiledFloor/TiledWall/WobbleProp) ----
    /// `Block.variants` (`Block.java:136`): sprite variant count (`0` = the
    /// plain region).
    pub variants: i32,
    /// `TiledFloor.maxSize`/`TiledWall.maxSize` (`0` = not a tiled class; the
    /// class default 3 applies for tiled kinds).
    pub max_size: i32,
    /// `WobbleProp.wscl` (`None` = the class default 25).
    pub wobble_scale: Option<f32>,
    /// `WobbleProp.wmag` (`None` = the class default 0.4).
    pub wobble_magnitude: Option<f32>,
    /// `WobbleProp.wtscl` (`None` = the class default 1).
    pub wobble_time_scale: Option<f32>,
    /// `WobbleProp.wmag2` (`None` = the class default 1).
    pub wobble_magnitude2: Option<f32>,
}

/// Whether the kind extends `Floor` (`world/blocks/environment/*`): base and
/// empty floors, every overlay kind, shallow liquids, steam vents and colored
/// floors. `AirBlock` extends `Floor` too (`AirBlock.java`) but draws nothing
/// (`AirBlock.drawBase` is empty).
pub fn is_floor_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Floor
            | BlockKind::AirBlock
            | BlockKind::EmptyFloor
            | BlockKind::OverlayFloor
            | BlockKind::OreBlock
            | BlockKind::ShallowLiquid
            | BlockKind::ColoredFloor
            | BlockKind::CharacterOverlay
            | BlockKind::RuneOverlay
            | BlockKind::SpawnBlock
            | BlockKind::RemoveOre
            | BlockKind::SteamVent
            // `TiledFloor extends Floor` (`TiledFloor.java:14`); its `drawMain`
            // is the tiled variant of `Floor.drawMain`.
            | BlockKind::TiledFloor
    )
}

/// Whether the kind's `drawBase` is `OverlayFloor.drawBase` — the variant
/// region only, no edges/overlay recursion (`OverlayFloor.java:22-24`). Covers
/// `OreBlock`, `SpawnBlock`, `CharacterOverlay`, `RuneOverlay` and `RemoveOre`,
/// which all extend `OverlayFloor`.
pub fn is_overlay_floor_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::OverlayFloor
            | BlockKind::OreBlock
            | BlockKind::SpawnBlock
            | BlockKind::RemoveOre
            | BlockKind::CharacterOverlay
            | BlockKind::RuneOverlay
    )
}

/// `Block.ignoreResizeConfig` (`Block.java:264-265`, default false) by class:
/// the constructors that set it are `LogicBlock` (`LogicBlock.java:58`),
/// `ItemBridge` (`ItemBridge.java:61`) and `PowerNode` (`PowerNode.java:45`).
/// Subclasses inherit the flag (`BufferedItemBridge`/`LiquidBridge`,
/// `LongPowerNode`), while `DirectionBridge`/`BeamNode` do not extend those
/// classes and keep it false.
pub fn kind_ignores_resize_config(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::LogicBlock
            | BlockKind::ItemBridge
            | BlockKind::BufferedItemBridge
            | BlockKind::LiquidBridge
            | BlockKind::PowerNode
            | BlockKind::LongPowerNode
    )
}

impl BlockDef {
    /// `Floor.init()`: `blendId = blendGroup.id` (`Floor.java:190`).
    pub fn blend_group_id(&self) -> BlockId {
        self.blend_group.unwrap_or(self.id)
    }

    /// Resolves a generated [`BlockSpec`] into a record
    /// (`ContentLoader`-adjacent construction; names resolve at load time like
    /// upstream object references).
    pub fn from_spec(
        spec: BlockSpec,
        registry: &ContentRegistry,
        bundle: &dyn BundleView,
        store: &dyn UnlockStore,
    ) -> Result<Self, ContentError> {
        // `Block.lightRadius`/`lightColor` (`Block.java:344,350` + the class and
        // `Blocks.java` overrides in [`java_light_defaults`]); resolved before
        // `spec.consumes` moves below.
        let (default_light_radius, default_light_color) = java_light_defaults(&spec);
        let floor_emit_light = java_floor_light(spec.name).is_some();
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
            let item = need(registry.item_id(name), name)?;
            research_cost_multipliers.push((item, *multiplier));
        }
        let item_drop = match spec.item_drop {
            Some(name) => Some(need(registry.item_id(name), name)?),
            None => None,
        };
        let liquid_drop = match spec.liquid_drop {
            Some(name) => Some(need(registry.liquid_id(name), name)?),
            None => None,
        };
        // `OreBlock.java:22,40` (`setup`): an ore's `mapColor` is its item's
        // colour. The generated specs already carry the same literals; this
        // covers mod content and any future drift.
        let map_color = match (spec.kind, spec.map_color, item_drop) {
            (BlockKind::OreBlock, None, Some(item)) => registry.item(item).map(|def| def.color),
            (_, color, _) => color,
        };
        // `Floor.isDeep()` = `drownTime > 0` (`Floor.java:339`).
        let drown_time = spec.drown_time.unwrap_or(0.0);
        let is_deep = drown_time > 0.0;
        // `Floor.status`/`statusDuration`/`dragMultiplier`/`damageTaken`
        // (G01 T13).
        let floor_status = match spec.floor_status {
            Some(name) => Some(need(
                registry.status_by_name(name).map(|record| record.id),
                name,
            )?),
            None => None,
        };
        let floor_status_duration = spec.floor_status_duration.unwrap_or(60.0);
        let floor_drag_multiplier = spec.floor_drag_multiplier.unwrap_or(1.0);
        let floor_damage_taken = spec.floor_damage_taken.unwrap_or(0.0);
        let subclass = spec.subclass.unwrap_or(spec.kind);
        // `Floor.blendGroup` (`Floor.java:62`); `None` resolves to this block in
        // `init_self` (the id is assigned after `from_spec`).
        let blend_group = match spec.blend_group {
            Some(name) => Some(need(registry.block_id(name), name)?),
            None => None,
        };
        let mut unit_plans = Vec::with_capacity(spec.unit_plans.len());
        for plan in &spec.unit_plans {
            let unit = need(registry.unit_id(plan.unit), plan.unit)?;
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
            let from = need(registry.unit_id(from), from)?;
            let to = need(registry.unit_id(to), to)?;
            reconstructor_upgrades.push((from, to));
        }
        let mut assembler_plans = Vec::with_capacity(spec.assembler_plans.len());
        for plan in &spec.assembler_plans {
            let unit = need(registry.unit_id(plan.unit), plan.unit)?;
            let mut payloads = Vec::with_capacity(plan.payloads.len());
            for payload in &plan.payloads {
                let item = if payload.block {
                    let id = need(registry.block_id(payload.name), payload.name)?;
                    ContentRef::block(id)
                } else {
                    let id = need(registry.unit_id(payload.name), payload.name)?;
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
        // `Block.requirements(Category, ItemStack...)` defaults the visibility
        // to `BuildVisibility.shown` (`Block.java:1231-1233`); the generated
        // specs omit the field for those calls. Requirement-bearing blocks are
        // therefore shown build-menu content, while requirement-less world
        // content (floors/walls/props/ores/editor blocks) stays hidden.
        let build_visibility = spec.build_visibility.unwrap_or({
            if requirements.is_empty() {
                BuildVisibility::Hidden
            } else {
                BuildVisibility::Shown
            }
        });
        // Production family knobs (GAPS #56 follow-up). Every list resolves the
        // same way the consumer/output stacks do, so a JSON mod's item/liquid
        // names are validated at load time like `ContentParser`'s reflective
        // field read. `None`/empty stays unset; `BlockKindData::from_def` then
        // applies the Java class default.
        let mut output_items = Vec::with_capacity(spec.output_items.len());
        for stack in &spec.output_items {
            output_items.push(resolve_item(registry, stack)?);
        }
        let mut output_liquids = Vec::with_capacity(spec.output_liquids.len());
        for stack in &spec.output_liquids {
            let liquid = need(registry.liquid_id(stack.liquid), stack.liquid)?;
            output_liquids.push((liquid, stack.amount));
        }
        let mut separator_results = Vec::with_capacity(spec.separator_results.len());
        for stack in &spec.separator_results {
            separator_results.push(resolve_item(registry, stack)?);
        }
        let mut blocked_items = Vec::with_capacity(spec.blocked_items.len());
        for name in &spec.blocked_items {
            blocked_items.push(need(registry.item_id(name), name)?);
        }
        let pump_liquid = match spec.pump_liquid {
            Some(name) => Some(need(registry.liquid_id(name), name)?),
            None => None,
        };
        let output_item = match spec.output_item {
            Some(name) => Some(need(registry.item_id(name), name)?),
            None => None,
        };
        // `Block.lightRadius`/`lightColor` (`Block.java:344,350` + the class and
        // `Blocks.java` overrides in [`java_light_defaults`]).
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
            under_bullets: false,
            // `rebuildable = false` classes: `BaseShield` (BaseShield.java:64),
            // `NuclearReactor` (:60), `VariableReactor` (VariableReactor.java:39).
            rebuildable: !matches!(
                spec.kind,
                BlockKind::BaseShield | BlockKind::NuclearReactor | BlockKind::VariableReactor
            ),
            deconstruct_drop_all_liquid: false,
            incinerate_non_buildable: spec.incinerate_non_buildable.unwrap_or(false),
            requires_core_zone: spec.requires_core_zone.unwrap_or(false),
            ignore_build_darkness: spec.ignore_build_darkness.unwrap_or(false),
            ignore_resize_config: kind_ignores_resize_config(spec.kind),
            unit_cap_modifier: spec.unit_cap_modifier.unwrap_or(BASE_UNIT_CAP_MODIFIER),
            core_unit: spec.core_unit,
            flags: spec.flags,
            consumes,
            unit_plans,
            reconstructor_upgrades,
            assembler_plans,
            drawer: spec.drawer.unwrap_or_default(),
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
            build_visibility,
            env_required: spec.env_required.unwrap_or_default(),
            env_enabled: spec
                .env_enabled
                .unwrap_or_else(|| EnvMask::of(vec![EnvFlag::Terrestrial])),
            env_disabled: spec.env_disabled.unwrap_or_default(),
            solid: spec.solid.unwrap_or(false),
            solidifies: spec.solidifies.unwrap_or(false),
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
            // `Block.canOverdrive` (`Block.java:314`); `LandingPad` clears it
            // in its constructor (`LandingPad.java:69`).
            can_overdrive: spec
                .can_overdrive
                .unwrap_or(!matches!(spec.kind, BlockKind::LandingPad)),
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
            rotate: spec.rotate.unwrap_or_else(|| kind_rotates(spec.kind)),
            rotate_draw: spec.rotate_draw.unwrap_or(true),
            rotate_draw_editor: spec.rotate_draw_editor.unwrap_or(true),
            visual_rotation_offset: spec.visual_rotation_offset.unwrap_or(0.0),
            size_offset: 0,
            stats: Vec::new(),
            bars: Vec::new(),
            map_color,
            albedo: spec.albedo.unwrap_or(0.0),
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
            emit_light: spec.emit_light.unwrap_or(floor_emit_light),
            light_radius: spec.light_radius.unwrap_or(default_light_radius),
            light_color: spec.light_color.unwrap_or(default_light_color),
            draw_team_overlay: spec.draw_team_overlay.unwrap_or(true),
            item_costs,
            item_health_scaling,
            attributes: spec
                .attributes
                .iter()
                .map(|(name, value)| ((*name).to_string(), *value))
                .collect(),
            drill_multipliers: spec
                .drill_multipliers
                .iter()
                .filter_map(|(name, value)| {
                    registry.item_by_name(name).map(|item| (item.id, *value))
                })
                .collect(),
            liquid_drop,
            speed_multiplier: spec.speed_multiplier.unwrap_or(1.0),
            is_liquid: spec.is_liquid.unwrap_or(false),
            // `Floor.autotile`/`autotileVariants`/`autotileMidVariants` and
            // `tilingVariants` (`Floor.java:82-90`); `overlayAlpha` default
            // 0.65 (`Floor.java:54`).
            autotile: spec.autotile.unwrap_or(false),
            autotile_variants: spec.autotile_variants.unwrap_or(1),
            autotile_mid_variants: spec.autotile_mid_variants.unwrap_or(1),
            tiling_variants: spec.tiling_variants.unwrap_or(0),
            overlay_alpha: spec.overlay_alpha.unwrap_or(0.65),
            shallow: spec.shallow.unwrap_or(false),
            is_deep,
            drown_time,
            floor_status,
            floor_status_duration,
            floor_drag_multiplier,
            floor_damage_taken,
            liquid_multiplier: spec.liquid_multiplier.unwrap_or(1.0),
            crush_fragile: spec.crush_fragile.unwrap_or(false),
            crush_damage_multiplier: spec.crush_damage_multiplier.unwrap_or(1.0),
            unit_move_breakable: spec.unit_move_breakable.unwrap_or(false),
            deposit_cooldown: spec.deposit_cooldown.unwrap_or(-1.0),
            team_passable: spec
                .team_passable
                .unwrap_or(spec.kind == BlockKind::AutoDoor),
            always_replace: spec.always_replace.unwrap_or(false),
            privileged: spec.privileged.unwrap_or(false),
            // `SwitchBlock.java:23`: `autoResetEnabled = false` (switch and
            // world-switch are the only vanilla overrides).
            auto_reset_enabled: spec.kind != BlockKind::SwitchBlock,
            // Actual values (generators/batteries/conduits) are set in
            // `init_self` (`Block.java:197-201`).
            base_explosiveness: 0.0,
            explosiveness_scale: 1.0,
            flammability_scale: 1.0,
            replaceable: spec.replaceable.unwrap_or(true),
            quick_rotate: spec.quick_rotate.unwrap_or(true),
            subclass,
            craft_time: spec.craft_time.unwrap_or(0.0),
            construct_time: spec.construct_time.unwrap_or(0.0),
            commandable: spec.commandable.unwrap_or(false),
            blend_group,
            draw_edge_in: spec.draw_edge_in.unwrap_or(true),
            draw_edge_out: spec.draw_edge_out.unwrap_or(true),
            // Computed in `init_self` (the block id is assigned on registration).
            has_edge: false,
            // `Floor.decoration = Blocks.air` (`Floor.java:70`); the decorating
            // prop writes it on registration (`RegistrySink::push`).
            decoration: BlockId::AIR,
            player_controllable: spec.player_controllable.unwrap_or(true),
            // `Block.useColor` (`Block.java:183`); `OverlayFloor.java:13` and
            // `AirBlock.java:14` clear it, `OreBlock.java:23` re-enables it.
            use_color: spec.use_color.unwrap_or(!matches!(
                spec.kind,
                BlockKind::OverlayFloor
                    | BlockKind::CharacterOverlay
                    | BlockKind::RuneOverlay
                    | BlockKind::SpawnBlock
                    | BlockKind::RemoveOre
                    | BlockKind::AirBlock
            )),
            // Production family knobs (GAPS #56 follow-up).
            output_items,
            output_liquids,
            liquid_output_directions: spec.liquid_output_directions,
            ignore_liquid_fullness: spec.ignore_liquid_fullness.unwrap_or(false),
            separator_results,
            drill_time: spec.drill_time,
            drill_tier: spec.drill_tier,
            hardness_drill_multiplier: spec.hardness_drill_multiplier,
            blocked_items,
            liquid_boost_intensity: spec.liquid_boost_intensity,
            warmup_speed: spec.warmup_speed,
            inverted_time: spec.inverted_time,
            burst_shake: spec.burst_shake,
            burst_drill_effect: spec.burst_drill_effect,
            beam_range: spec.beam_range,
            pump_amount: spec.pump_amount,
            item_use_time: spec.item_use_time,
            pump_liquid,
            base_efficiency: spec.base_efficiency,
            attribute: spec.attribute.map(str::to_owned),
            output_item,
            item_boost_intensity: spec.item_boost_intensity,
            boost_item_use_time: spec.boost_item_use_time,
            boost_scale: spec.boost_scale,
            min_efficiency: spec.min_efficiency,
            max_boost: spec.max_boost,
            output_scale: spec.output_scale,
            scale_liquid_consumption: spec.scale_liquid_consumption,
            display_efficiency: spec.display_efficiency,
            display_scaled_output: spec.display_scaled_output,
            // Environment family knobs (TiledFloor/TiledWall/WobbleProp).
            variants: spec.variants.unwrap_or(0),
            max_size: spec.max_size.unwrap_or(0),
            wobble_scale: spec.wobble_scale,
            wobble_magnitude: spec.wobble_magnitude,
            wobble_time_scale: spec.wobble_time_scale,
            wobble_magnitude2: spec.wobble_magnitude2,
        };
        // `Block.init`: `if(underBullets){ priority = TargetPriority.under; }`
        // (Block.java:1386-1387) — unconditional, so conveyors/conduits (which
        // set the transport priority in their initializers) end up at `under`.
        def.under_bullets = block_under_bullets(spec.kind, spec.name);
        if def.under_bullets {
            def.priority = TARGET_PRIORITY_UNDER;
        }
        if let Some(generate) = generate_icons {
            def.unlock.generate_icons = generate;
        }
        if let Some(always_unlocked) = spec.always_unlocked {
            def.unlock.always_unlocked = always_unlocked;
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

/// Resolves a name-based content reference, mapping a miss to
/// [`ContentError::UnknownName`] (`ContentParser`'s "no content named …").
pub(super) fn need<T>(value: Option<T>, name: &str) -> Result<T, ContentError> {
    value.ok_or_else(|| ContentError::UnknownName(name.to_owned()))
}

/// Resolves a generated item stack spec.
pub(super) fn resolve_item(
    registry: &ContentRegistry,
    spec: &StackSpec,
) -> Result<ItemStack, ContentError> {
    let item = need(registry.item_id(spec.item), spec.item)?;
    Ok(ItemStack::new(item, spec.amount))
}

/// Resolves a generated consumer spec.
pub(super) fn resolve_consume(
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
            let liquid = need(registry.liquid_id(liquid), liquid)?;
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
                let liquid = need(registry.liquid_id(stack.liquid), stack.liquid)?;
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

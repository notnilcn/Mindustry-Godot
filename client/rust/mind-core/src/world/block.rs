// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Block behavior side table (`Block` runtime half).
//!
//! Ported from `core/src/mindustry/world/Block.java` (`init`, `afterPatch`,
//! `createIcons`, placement flags) and the `BlockView` read API of plan 07
//! §3.2. Plan 02 owns [`crate::content::BlockDef`] metadata; this module adds the
//! per-[`BlockId`] runtime instance (behavior, consumers, bars, draw/sound
//! descriptors) and the derived-value validation pass.

use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::content::{
    BarSpec, BlockDef, BlockFlag, BlockGroup, BlockId, ContentRegistry, ContentType, EnvMask,
    PlanetId, Rgba,
};
use crate::game::rules::{DEFAULT_ENV, Rules};

use super::behavior::{BehaviorRegistry, BuildingBehavior, BuildingKind, resolve_behavior};
use super::block_kind_data::BlockKindData;
use super::config::{ConfigHandlers, ConfigKind, ConfigValue};
use super::consumers::Consumers;
use super::draw::DrawSpec;

/// `Block.tilesize`.
pub const TILE_SIZE: f32 = crate::content::TILE_SIZE;

/// Fields `@NoPatch` rejects in plan 20's `DataPatcher` (plan 07 §8 R11).
pub const PATCH_DENIED: &[&str] = &[
    "size",
    "consumes",
    "buildType",
    "itemFilter",
    "liquidFilter",
    "kind",
    "region",
    "draw",
    "bars",
];

/// Sound handles for a block's lifecycle (`Block.placeSound` etc.; plan 18).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockSounds {
    /// Placement sound.
    pub place: Option<String>,
    /// Break-in-progress sound (`breakSound`).
    pub break_sound: Option<String>,
    /// Destruction sound (`destroySound`).
    pub destroy: Option<String>,
    /// Ambient loop.
    pub ambient: Option<String>,
}

/// One block's runtime behavior instance (plan 07 §3.2).
pub struct BlockInstance {
    /// Content id.
    pub id: BlockId,
    /// Content name (parity ABI).
    pub name: String,
    /// Owned metadata snapshot (so behaviors never need the full registry).
    pub def: Arc<BlockDef>,
    /// Building family tag (grouping/IO/revision).
    pub building: BuildingKind,
    /// Registered behavior.
    pub behavior: Arc<dyn BuildingBehavior>,
    /// Typed family data.
    pub kind_data: BlockKindData,
    /// Lowered consumers.
    pub consumers: Consumers,
    /// Config handlers.
    pub configs: ConfigHandlers,
    /// Static bar descriptors.
    pub bars: SmallVec<[BarSpec; 4]>,
    /// Draw descriptor.
    pub draw: DrawSpec,
    /// Lifecycle sounds.
    pub sounds: BlockSounds,
    /// Icon region descriptors (03 consumes).
    pub icon_descriptors: SmallVec<[String; 4]>,
    /// Derived rotation capability (`Block.rotate`; plan-04 plan predicate).
    pub rotate: bool,
}

impl std::fmt::Debug for BlockInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockInstance")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("building", &self.building)
            .field("family", &self.kind_data.family_name())
            .field("consumers", &self.consumers.len())
            .field("rotate", &self.rotate)
            .finish()
    }
}

impl BlockInstance {
    /// Builds the runtime instance from metadata + an optional behavior override.
    pub fn from_def(def: &BlockDef, registry: &BehaviorRegistry) -> Result<Self, BlockError> {
        let mut kind_data = BlockKindData::from_def(def);
        let building = building_kind(&kind_data);
        let behavior = resolve_behavior(def, registry);
        let consumers = Consumers::build(&def.consumes);
        // `WallCrafter.init()` (`WallCrafter.java:103-107`): `hasLiquidBooster =
        // findConsumer(f -> f instanceof ConsumeLiquidBase && f.booster) != null`.
        // The port has no `Consume.booster` flag; `optional` stands in for
        // `.boost()` (the only way vanilla declares an optional consumer).
        // `ConsumeCoolant` also extends `ConsumeLiquidBase`.
        if let BlockKindData::WallCrafter(wall) = &mut kind_data {
            wall.has_liquid_booster = consumers.optional.iter().any(|&index| {
                consumers.all.get(index).is_some_and(|consumer| {
                    matches!(
                        consumer.kind,
                        crate::world::consumers::ConsumeInstanceKind::Liquid { .. }
                            | crate::world::consumers::ConsumeInstanceKind::Liquids(_)
                            | crate::world::consumers::ConsumeInstanceKind::Coolant { .. }
                    )
                })
            });
        }
        assert_derivations(def)?;
        let mut bars: SmallVec<[BarSpec; 4]> = SmallVec::new();
        if def.health > 0 {
            bars.push(BarSpec::Health);
        }
        if def.has_power {
            bars.push(BarSpec::Power);
        }
        if def.has_items {
            bars.push(BarSpec::Items);
        }
        if def.has_liquids {
            bars.push(BarSpec::Liquid);
        }
        for bar in &def.bars {
            if !bars.contains(bar) {
                bars.push(bar.clone());
            }
        }
        let sounds = default_sounds(def);
        let mut icon_descriptors = SmallVec::new();
        if !def.region.is_empty() {
            icon_descriptors.push(def.region.clone());
        }
        Ok(Self {
            id: def.id,
            name: def.name.clone(),
            def: Arc::new(def.clone()),
            building,
            behavior,
            kind_data,
            consumers,
            configs: ConfigHandlers::default(),
            bars,
            draw: DrawSpec::Default,
            sounds,
            icon_descriptors,
            rotate: match def.name.as_str() {
                // `Blocks.java:1362` electrolyzer sets `rotate = true`;
                // `Blocks.java:1177` cryofluidMixer sets `rotate = false`.
                "electrolyzer" => true,
                "cryofluid-mixer" => false,
                _ => kind_rotates(def.kind),
            },
        })
    }

    /// Whether the block has a building entity (`Block.hasBuilding`).
    pub fn has_building(&self) -> bool {
        !matches!(
            self.kind_data.family(),
            super::block_kind_data::BlockFamily::Environment
        )
    }

    /// Config value kinds the block's behavior accepts
    /// (`Block.configurations` keys; `Block.config(...)`).
    pub fn config_kinds(&self) -> &'static [ConfigKind] {
        self.behavior.config_kinds()
    }

    /// Consumer count.
    pub fn consumer_count(&self) -> usize {
        self.consumers.len()
    }

    /// `Block.planRotation` (`Block.java:1653-1655`): `!rotate && lockRotation ?
    /// 0 : rot` (`lockRotation` defaults true and no ported block flips it).
    pub fn plan_rotation(&self, rotation: u8) -> u8 {
        if self.rotate { rotation } else { 0 }
    }

    /// `Block.invertFlip` (`Block.java:144`; only `electrolyzer` sets it,
    /// `Blocks.java:1363`).
    pub fn invert_flip(&self) -> bool {
        self.name == "electrolyzer"
    }

    /// `Block.flipRotation(req, x)` (`Block.java:1657-1661`): flips the plan
    /// rotation by two when the flip axis matches the block's orientation.
    pub fn flip_rotation(&self, rotation: u8, x: bool) -> u8 {
        let even = rotation.is_multiple_of(2);
        if (x == even) != self.invert_flip() {
            self.plan_rotation((rotation + 2) % 4)
        } else {
            rotation
        }
    }

    /// Config value kinds the block declares (`Block.configurations` keys).
    ///
    /// The runtime `configs` table is only populated by a behavior that also
    /// installs handlers; the declared kinds on the behavior are the same set
    /// (`config(Class, ...)` registers both), so they are unioned.
    pub fn declared_config_kinds(&self) -> impl Iterator<Item = ConfigKind> + '_ {
        self.behavior
            .config_kinds()
            .iter()
            .copied()
            .chain(self.configs.by_kind.iter().map(|(kind, _)| *kind))
    }

    /// `Block.getPlanConfigs(options)` (`Block.java:1663-1671`) plus the
    /// `UnitFactory` (`UnitFactory.java:203-209`), `Constructor`
    /// (`Constructor.java:50-53`), `PayloadSource` (`PayloadSource.java:71-75`)
    /// and `PayloadRouter` (`PayloadRouter.java:49-53`) overrides: the content
    /// options a plan's point-config popup lists.
    ///
    /// The default lists every item/liquid when the block declares an
    /// `Item.class`/`Liquid.class` config (`configurations.containsKey`,
    /// not the item/liquid container flags). `rules` supplies the ban and
    /// environment filters; `None` keeps the rules-free options.
    pub fn plan_configs(
        &self,
        content: &ContentRegistry,
        rules: Option<&Rules>,
    ) -> Vec<PlanConfigOption> {
        let ctx = PlanConfigCtx::new(content, rules);
        let mut options = Vec::new();
        match self.def.kind {
            crate::content::BlockKind::UnitFactory => {
                self.unit_factory_plan_configs(&ctx, &mut options)
            }
            crate::content::BlockKind::Constructor => {
                self.constructor_plan_configs(&ctx, &mut options)
            }
            crate::content::BlockKind::PayloadSource => {
                self.payload_source_plan_configs(&ctx, &mut options)
            }
            crate::content::BlockKind::PayloadRouter => {
                self.payload_router_plan_configs(&ctx, &mut options)
            }
            _ => self.default_plan_configs(&ctx, &mut options),
        }
        options
    }

    /// `Block.getPlanConfigs` base (`Block.java:1664-1673`).
    fn default_plan_configs(&self, ctx: &PlanConfigCtx<'_>, out: &mut Vec<PlanConfigOption>) {
        let declares = |kind: ConfigKind| self.declared_config_kinds().any(|value| value == kind);
        if declares(ConfigKind::Item) {
            for item in ctx.content.items() {
                out.push(PlanConfigOption::new(
                    ContentType::Item,
                    item.id.raw(),
                    &item.name,
                ));
            }
        }
        if declares(ConfigKind::Liquid) {
            for liquid in ctx.content.liquids() {
                out.push(PlanConfigOption::new(
                    ContentType::Liquid,
                    liquid.id.raw(),
                    &liquid.name,
                ));
            }
        }
    }

    /// `UnitFactory.getPlanConfigs` (`UnitFactory.java:203-209`): every plan
    /// unit that is not banned.
    fn unit_factory_plan_configs(&self, ctx: &PlanConfigCtx<'_>, out: &mut Vec<PlanConfigOption>) {
        for plan in &self.def.unit_plans {
            let Some(unit) = ctx.content.unit(plan.unit) else {
                continue;
            };
            if ctx.banned_unit(&unit.name) {
                continue;
            }
            out.push(PlanConfigOption::new(
                ContentType::Unit,
                unit.id.raw(),
                &unit.name,
            ));
        }
    }

    /// `Constructor.canProduce` size range: the behavior's `minBlockSize`/
    /// `maxBlockSize` knobs (class defaults `1..=2`, `Constructor.java:25`).
    fn constructor_sizes(&self) -> (i32, i32) {
        use crate::world::blocks::payloads::constructor::ConstructorBehavior;
        let behavior = self.behavior.as_ref() as &dyn std::any::Any;
        behavior
            .downcast_ref::<ConstructorBehavior>()
            .map(|behavior| (behavior.min_block_size, behavior.max_block_size))
            .unwrap_or((1, 2))
    }

    /// `Constructor.getPlanConfigs` (`Constructor.java:49-52`) via
    /// `canProduce` (`Constructor.java:54-56`).
    fn constructor_plan_configs(&self, ctx: &PlanConfigCtx<'_>, out: &mut Vec<PlanConfigOption>) {
        let (min, max) = self.constructor_sizes();
        for block in ctx.content.blocks() {
            if !ctx.constructor_can_produce(block, min, max) {
                continue;
            }
            out.push(PlanConfigOption::new(
                ContentType::Block,
                block.id.raw(),
                &block.name,
            ));
        }
    }

    /// `PayloadSource.getPlanConfigs` (`PayloadSource.java:71-75`): producible
    /// blocks then producible units.
    fn payload_source_plan_configs(
        &self,
        ctx: &PlanConfigCtx<'_>,
        out: &mut Vec<PlanConfigOption>,
    ) {
        for block in ctx.content.blocks() {
            if !ctx.payload_source_can_produce_block(block, self.def.size) {
                continue;
            }
            out.push(PlanConfigOption::new(
                ContentType::Block,
                block.id.raw(),
                &block.name,
            ));
        }
        for unit in ctx.content.units() {
            if !ctx.payload_source_can_produce_unit(unit) {
                continue;
            }
            out.push(PlanConfigOption::new(
                ContentType::Unit,
                unit.id.raw(),
                &unit.name,
            ));
        }
    }

    /// `PayloadRouter.getPlanConfigs` (`PayloadRouter.java:49-53`): sortable
    /// blocks then sortable units.
    fn payload_router_plan_configs(
        &self,
        ctx: &PlanConfigCtx<'_>,
        out: &mut Vec<PlanConfigOption>,
    ) {
        for block in ctx.content.blocks() {
            if !ctx.payload_router_can_sort_block(block, self.def.size) {
                continue;
            }
            out.push(PlanConfigOption::new(
                ContentType::Block,
                block.id.raw(),
                &block.name,
            ));
        }
        for unit in ctx.content.units() {
            if !ctx.payload_router_can_sort_unit(unit) {
                continue;
            }
            out.push(PlanConfigOption::new(
                ContentType::Unit,
                unit.id.raw(),
                &unit.name,
            ));
        }
    }

    /// `Block.placeEnded(tile, builder, rotation, config)` (`Block.java:1083-1085`):
    /// dispatches the completion hook to the block's behavior.
    pub fn place_ended(
        &self,
        world: &mut World,
        e: Entity,
        builder: Option<Entity>,
        rotation: u8,
        config: &ConfigValue,
    ) {
        self.behavior
            .place_ended(world, e, builder, rotation, config);
    }
}

/// One selectable content of a plan's point-config popup
/// (`Block.getPlanConfigs`'s `Seq<UnlockableContent>` entry).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlanConfigOption {
    /// Content type (`item`/`liquid`/`block`/`unit`).
    pub kind: ContentType,
    /// Content name (parity ABI; already mod-prefixed).
    pub name: String,
    /// Dense content id in the type's id space.
    pub id: u16,
}

impl PlanConfigOption {
    /// Builds an option from a typed content record.
    pub fn new(kind: ContentType, id: u16, name: &str) -> Self {
        Self {
            kind,
            name: name.to_owned(),
            id,
        }
    }

    /// Bundle key of the option label (`item.<name>.name`, the key the
    /// `PlanConfigFragment` popup resolves).
    pub fn label(&self) -> String {
        format!("{}.{}.name", self.kind.name(), self.name)
    }
}

/// The `state.rules` + `state.getPlanet()` subset the `getPlanConfigs`
/// overrides filter on.
#[derive(Clone, Copy)]
struct PlanConfigCtx<'a> {
    /// Content registry.
    content: &'a ContentRegistry,
    /// Live game rules (`None` = rules-free).
    rules: Option<&'a Rules>,
    /// `state.getPlanet()`.
    planet: Option<PlanetId>,
    /// `Planets.sun` (matches every block, `UnlockableContent.isOnPlanet`).
    sun: Option<PlanetId>,
}

impl<'a> PlanConfigCtx<'a> {
    fn new(content: &'a ContentRegistry, rules: Option<&'a Rules>) -> Self {
        let planet = rules.and_then(|rules| Self::planet_of(content, rules));
        let sun = content.planet_id("sun");
        Self {
            content,
            rules,
            planet,
            sun,
        }
    }

    /// `GameState.getPlanet()` (`GameState.java:85-87`):
    /// `rules.sector != null ? rules.sector.planet : rules.planet`.
    fn planet_of(content: &ContentRegistry, rules: &Rules) -> Option<PlanetId> {
        if let Some(sector) = &rules.sector
            && let Some((planet, _)) = sector.0.rsplit_once('-')
            && let Some(planet) = content.planet_id(planet)
        {
            return Some(planet);
        }
        content.planet_id(&rules.planet)
    }

    fn editor(&self) -> bool {
        self.rules.is_some_and(|rules| rules.editor)
    }

    /// `state.rules.env` (the default environment when rules are absent).
    fn env(&self) -> i32 {
        self.rules.map(|rules| rules.env).unwrap_or(DEFAULT_ENV)
    }

    /// `rules.isBanned(block)` by content name.
    fn banned_block(&self, name: &str) -> bool {
        self.rules.is_some_and(|rules| rules.is_banned_block(name))
    }

    /// `rules.isBanned(unit)` by content name.
    fn banned_unit(&self, name: &str) -> bool {
        self.rules.is_some_and(|rules| rules.is_banned_unit(name))
    }

    /// `Block.environmentBuildable()` (`Block.java:1117-1119`).
    fn environment_buildable(&self, def: &BlockDef) -> bool {
        super::build::environment_buildable(def, self.planet, self.sun)
    }

    /// `Block.isVisible()` (`Block.java:1031-1033`): `!isHidden() && (editor ||
    /// !hideBannedBlocks || !isBanned())`, with `isHidden()` being
    /// `!BuildVisibility.visible() && !revealedBlocks.contains(this)`
    /// (`Block.java:1553-1555`, `BuildVisibility.java:10-21`).
    fn block_visible(&self, def: &BlockDef) -> bool {
        use crate::content::registries::blocks::BuildVisibility as V;
        let editor = self.editor();
        let rules = self.rules;
        let visible = match def.build_visibility {
            V::Hidden | V::DebugOnly => false,
            V::Shown => true,
            V::EditorOnly => editor,
            V::WorldProcessorOnly => {
                editor || rules.is_some_and(|rules| rules.allow_edit_world_processors)
            }
            V::SandboxOnly => rules.is_none_or(|rules| rules.infinite_resources),
            V::CampaignOnly => rules.is_none_or(|rules| rules.sector.is_some()),
            // `Vars.indexer.isBlockPresent(Blocks.coreZone) || !state.isGame()`:
            // a live game has the core-zone floor registered (the port has no
            // content-presence index at this seam).
            V::CoreZoneOnly => true,
            V::LightingOnly => rules.is_none_or(|rules| {
                rules.lighting || rules.infinite_resources || rules.sector.is_some()
            }),
            V::FogOnly => rules.is_none_or(|rules| rules.fog || rules.editor),
            // Campaign launch-pad rules (`Planet.campaignRules`) are not a port
            // field at this seam; treat the block as visible.
            V::LegacyLaunchPadOnly | V::NotLegacyLaunchPadOnly => true,
        };
        let revealed = rules.is_some_and(|rules| rules.revealed_blocks.contains(def.name.as_str()));
        let hidden = !visible && !revealed;
        !hidden
            && (editor
                || !rules.is_some_and(|rules| {
                    rules.hide_banned_blocks && rules.is_banned_block(&def.name)
                }))
    }

    /// `Constructor.canProduce(Block)` (`Constructor.java:54-56`):
    /// `b.isVisible() && b.size >= minBlockSize && b.size <= maxBlockSize &&
    /// !(b instanceof CoreBlock) && !state.rules.isBanned(b) &&
    /// b.environmentBuildable() && (filter.isEmpty() || filter.contains(b))`.
    ///
    /// The port's `BlockDef` has no `filter` field (no vanilla constructor sets
    /// one; `Blocks.java:6710` leaves it empty), so the filter half is trivially
    /// true.
    fn constructor_can_produce(&self, block: &BlockDef, min: i32, max: i32) -> bool {
        self.block_visible(block)
            && block.size >= min
            && block.size <= max
            && block.kind != crate::content::BlockKind::CoreBlock
            && !self.banned_block(&block.name)
            && self.environment_buildable(block)
    }

    /// `PayloadSource.canProduce(Block)` (`PayloadSource.java:89-91`):
    /// `!b.removed && b.isVisible() && b.size < size && b.allowedInPayloads &&
    /// !isBanned(b) && b.environmentBuildable()`.
    ///
    /// `Block.allowedInPayloads` is not a ported `BlockDef` field and no vanilla
    /// block clears it (default true), so that clause is treated as true.
    fn payload_source_can_produce_block(&self, block: &BlockDef, source_size: i32) -> bool {
        !block.removed
            && self.block_visible(block)
            && block.size < source_size
            && !self.banned_block(&block.name)
            && self.environment_buildable(block)
    }

    /// `PayloadSource.canProduce(UnitType)` (`PayloadSource.java:93-95`):
    /// `!t.removed && !t.isHidden() && !t.isBanned() && t.supportsEnv(rules.env)`.
    fn payload_source_can_produce_unit(
        &self,
        unit: &crate::content::registries::units::UnitTypeDef,
    ) -> bool {
        !unit.removed
            && !unit.hidden
            && !self.banned_unit(&unit.name)
            && unit.supports_env(self.env())
    }

    /// `PayloadRouter.canSort(Block)` (`PayloadRouter.java:62-64`):
    /// `b.isVisible() && b.size <= size && !(b instanceof CoreBlock) &&
    /// !isBanned(b) && b.environmentBuildable()`.
    fn payload_router_can_sort_block(&self, block: &BlockDef, router_size: i32) -> bool {
        self.block_visible(block)
            && block.size <= router_size
            && block.kind != crate::content::BlockKind::CoreBlock
            && !self.banned_block(&block.name)
            && self.environment_buildable(block)
    }

    /// `PayloadRouter.canSort(UnitType)` (`PayloadRouter.java:66-68`):
    /// `!t.isHidden() && !t.isBanned() && t.supportsEnv(rules.env)`.
    fn payload_router_can_sort_unit(
        &self,
        unit: &crate::content::registries::units::UnitTypeDef,
    ) -> bool {
        !unit.hidden && !self.banned_unit(&unit.name) && unit.supports_env(self.env())
    }
}

/// Errors raised while building the block table.
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum BlockError {
    /// A derived field did not match plan 02's value.
    #[error("block `{name}` derivation mismatch: {field} expected {expected}, found {found}")]
    Derivation {
        /// Block name.
        name: String,
        /// Field.
        field: &'static str,
        /// Expected value.
        expected: f32,
        /// Actual value.
        found: f32,
    },
    /// The registry had no blocks.
    #[error("content registry has no blocks")]
    Empty,
}

/// Per-block runtime table (`Blocks` resource; plan 07 §3.2).
///
/// Instances are stored behind `Arc` so behavior code can hold block data across
/// a mutable ECS borrow without cloning per tick.
#[derive(Resource, Clone)]
pub struct BlockTable {
    instances: Vec<Option<Arc<BlockInstance>>>,
    by_name: indexmap::IndexMap<String, BlockId>,
    count: usize,
}

impl std::fmt::Debug for BlockTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockTable")
            .field("count", &self.count)
            .finish()
    }
}

/// Back-compat alias matching the plan's `Blocks` resource name.
pub type Blocks = BlockTable;

impl BlockTable {
    /// Builds the table from a content registry and behavior overrides.
    pub fn build(
        content: &ContentRegistry,
        registry: &BehaviorRegistry,
    ) -> Result<Self, BlockError> {
        let defs = content.blocks();
        if defs.is_empty() {
            return Err(BlockError::Empty);
        }
        // Dense id order: `BlockDef.id.index()` is the raw content id.
        let max = defs.iter().map(|def| def.id.index()).max().unwrap_or(0);
        let mut instances: Vec<Option<Arc<BlockInstance>>> = (0..=max).map(|_| None).collect();
        let mut by_name = indexmap::IndexMap::new();
        let mut count = 0;
        for def in defs {
            let mut instance = BlockInstance::from_def(def, registry)?;
            instance.kind_data.apply_vanilla_knobs(content, &def.name);
            let instance = Arc::new(instance);
            by_name.insert(def.name.clone(), def.id);
            instances[def.id.index()] = Some(instance);
            count += 1;
        }
        Ok(Self {
            instances,
            by_name,
            count,
        })
    }

    /// Builds with the default plan-07/08 behavior registry (logistics families).
    pub fn build_default(content: &ContentRegistry) -> Result<Self, BlockError> {
        Self::build(content, &super::blocks::default_registry(content))
    }

    /// Instance by id.
    pub fn get(&self, id: BlockId) -> Option<&Arc<BlockInstance>> {
        self.instances.get(id.index()).and_then(Option::as_ref)
    }

    /// Cloned instance handle (cheap `Arc` clone) for use across mutable borrows.
    pub fn instance(&self, id: BlockId) -> Option<Arc<BlockInstance>> {
        self.get(id).cloned()
    }

    /// Resolves a content name to an id.
    pub fn id_of(&self, name: &str) -> Option<BlockId> {
        self.by_name.get(name).copied()
    }

    /// Instance by content name.
    pub fn get_named(&self, name: &str) -> Option<&Arc<BlockInstance>> {
        let id = self.id_of(name)?;
        self.get(id)
    }

    /// Number of blocks in the table.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the table has no blocks (never for a real registry).
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Iterates instances in id order.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<BlockInstance>> + '_ {
        self.instances.iter().filter_map(Option::as_ref)
    }

    /// Whether a block rotates (`Block.rotate` predicate for plan 04's
    /// `write_client_plans`).
    pub fn rotate(&self, id: BlockId) -> bool {
        self.get(id).is_some_and(|instance| instance.rotate)
    }

    /// Returns a [`BlockView`] combining metadata + runtime.
    pub fn view(&self, id: BlockId) -> Option<BlockView<'_>> {
        let inst = self.get(id)?;
        Some(BlockView {
            def: inst.def.as_ref(),
            inst,
        })
    }
}

/// Combined metadata + runtime view (`BlockView`).
#[derive(Debug, Clone, Copy)]
pub struct BlockView<'a> {
    /// Metadata record.
    pub def: &'a BlockDef,
    /// Runtime instance.
    pub inst: &'a BlockInstance,
}

impl BlockView<'_> {
    /// Content id.
    pub fn id(&self) -> BlockId {
        self.def.id
    }

    /// Content name.
    pub fn name(&self) -> &str {
        &self.def.name
    }

    /// Size in tiles.
    pub fn size(&self) -> i32 {
        self.def.size
    }

    /// `Block.offset`.
    pub fn offset(&self) -> f32 {
        self.def.offset
    }

    /// `Block.sizeOffset`.
    pub fn size_offset(&self) -> i32 {
        self.def.size_offset
    }

    /// Health.
    pub fn health(&self) -> i32 {
        self.def.health
    }

    /// Build time.
    pub fn build_time(&self) -> f32 {
        self.def.build_time
    }

    /// Item capacity.
    pub fn item_capacity(&self) -> i32 {
        self.def.item_capacity
    }

    /// Liquid capacity.
    pub fn liquid_capacity(&self) -> f32 {
        self.def.liquid_capacity
    }

    /// `Block.hasItems`.
    pub fn has_items(&self) -> bool {
        self.def.has_items
    }

    /// `Block.hasLiquids`.
    pub fn has_liquids(&self) -> bool {
        self.def.has_liquids
    }

    /// `Block.hasPower`.
    pub fn has_power(&self) -> bool {
        self.def.has_power
    }

    /// `Block.outputsPower`.
    pub fn outputs_power(&self) -> bool {
        self.def.outputs_power
    }

    /// `Block.consumesPower`.
    pub fn consumes_power(&self) -> bool {
        self.def.consumes_power
    }

    /// `Block.solid`.
    pub fn solid(&self) -> bool {
        self.def.solid
    }

    /// `Block.update`.
    pub fn update(&self) -> bool {
        self.def.update
    }

    /// Whether the block is "active" for status purposes (plan 07 status).
    pub fn is_active(&self) -> bool {
        self.def.update && (self.def.has_items || self.def.has_liquids || self.def.has_power)
    }

    /// `Block.rotate`.
    pub fn rotate(&self) -> bool {
        self.inst.rotate
    }

    /// `Block.configurable`.
    pub fn configurable(&self) -> bool {
        self.def.configurable
    }

    /// Replace group.
    pub fn group(&self) -> BlockGroup {
        self.def.group
    }

    /// Block flags.
    pub fn flags(&self) -> &[BlockFlag] {
        &self.def.flags
    }

    /// Whether the block has a flag.
    pub fn has_flag(&self, flag: BlockFlag) -> bool {
        self.def.flags.contains(&flag)
    }

    /// Required environment mask.
    pub fn env_required(&self) -> &EnvMask {
        &self.def.env_required
    }

    /// Enabled environment mask.
    pub fn env_enabled(&self) -> &EnvMask {
        &self.def.env_enabled
    }

    /// Disabled environment mask.
    pub fn env_disabled(&self) -> &EnvMask {
        &self.def.env_disabled
    }

    /// `Block.placeablePlayer`.
    pub fn placeable_player(&self) -> bool {
        self.def.placeable_player
    }

    /// `Block.placeableLiquid`.
    pub fn placeable_liquid(&self) -> bool {
        self.def.placeable_liquid
    }

    /// `Block.placeableOn`.
    pub fn placeable_on(&self) -> bool {
        self.def.placeable_on
    }

    /// `Block.destructible`.
    pub fn destructible(&self) -> bool {
        self.def.destructible
    }

    /// Barriers/limits (`Block.canBeBuilt` metadata half).
    pub fn can_be_built(&self) -> bool {
        !matches!(
            self.def.build_visibility,
            crate::content::BuildVisibility::Hidden | crate::content::BuildVisibility::DebugOnly
        )
    }

    /// Consumers.
    pub fn consumers(&self) -> &Consumers {
        &self.inst.consumers
    }

    /// Typed family data.
    pub fn kind_data(&self) -> &BlockKindData {
        &self.inst.kind_data
    }

    /// Building family tag.
    pub fn building(&self) -> BuildingKind {
        self.inst.building
    }

    /// Registered behavior.
    pub fn behavior(&self) -> &Arc<dyn BuildingBehavior> {
        &self.inst.behavior
    }

    /// Bar descriptors.
    pub fn bars(&self) -> &[BarSpec] {
        &self.inst.bars
    }

    /// Map color, when set.
    pub fn map_color(&self) -> Option<Rgba> {
        self.def.map_color
    }
}

/// Maps typed family data to the building family tag.
pub fn building_kind(data: &BlockKindData) -> BuildingKind {
    use BlockKindData as D;
    match data {
        D::Construct { .. } => BuildingKind::CONSTRUCT,
        D::Crafter(_) | D::AttributeCrafter(_) | D::Incinerator(_) => BuildingKind::CRAFTER,
        D::Drill(_) | D::BurstDrill(_) | D::BeamDrill(_) | D::WallCrafter(_) => BuildingKind::DRILL,
        D::Separator(_) => BuildingKind::SEPARATOR,
        D::LiquidSource(_) => BuildingKind::LIQUID_SOURCE,
        D::Accelerator(_) => BuildingKind::ACCELERATOR,
        D::LaunchPad(_) => BuildingKind::LAUNCH_PAD,
        D::Wall(_) => BuildingKind::WALL,
        D::Door(_) => BuildingKind::DOOR,
        D::Logic(_) => BuildingKind::LOGIC,
        D::Memory(_) => BuildingKind::MEMORY,
        D::Switch(_) => BuildingKind::SWITCH,
        D::Message(_) => BuildingKind::MESSAGE,
        D::Display(_) => BuildingKind::DISPLAY,
        D::TileableDisplay(_) => BuildingKind::TILEABLE_DISPLAY,
        D::Canvas(_) => BuildingKind::CANVAS,
        _ => BuildingKind::GENERIC,
    }
}

/// Whether a block kind rotates by default (`Block.rotate` derived default).
///
/// Plan 02 owns `BlockDef.rotate`; until that field exists this is the derived
/// predicate the plan-04 `write_client_plans` seam consumes. The list is the
/// rotation-capable set of this plan's families plus the transport/turret kinds
/// owned by plans 08/10.
pub fn kind_rotates(kind: crate::content::BlockKind) -> bool {
    // The canonical list lives with the block content (`rotate = true` in
    // `J/world/blocks` plus the `Blocks.java` per-block overrides; `Block.java`
    // defaults `rotate` to false). Kept as a thin alias so the sim's placement
    // code and `BlockDef.rotate` cannot drift.
    crate::content::registries::blocks::kind_rotates(kind)
}

/// Default lifecycle sounds by size (`Block.init`).
pub fn default_sounds(def: &BlockDef) -> BlockSounds {
    let area = def.size * def.size;
    BlockSounds {
        place: Some(String::from("place")),
        break_sound: Some(String::from("break")),
        destroy: Some(if area >= 4 {
            String::from("explosion")
        } else {
            String::from("break")
        }),
        ambient: None,
    }
}

/// Asserts plan 02's derived equations (`Block.init`) and returns a structured
/// error on drift (plan 07 §7a `blocks_derivation_matches`).
pub fn assert_derivations(def: &BlockDef) -> Result<(), BlockError> {
    let size = def.size as f32;
    let expected_offset = ((def.size + 1) % 2) as f32 * TILE_SIZE / 2.0;
    if (def.offset - expected_offset).abs() > f32::EPSILON {
        return Err(BlockError::Derivation {
            name: def.name.clone(),
            field: "offset",
            expected: expected_offset,
            found: def.offset,
        });
    }
    let expected_size_offset = -((def.size - 1) / 2) as f32;
    if (def.size_offset as f32 - expected_size_offset).abs() > f32::EPSILON {
        return Err(BlockError::Derivation {
            name: def.name.clone(),
            field: "size_offset",
            expected: expected_size_offset,
            found: def.size_offset as f32,
        });
    }
    // Health formula guard: positive blocks must have a computed health.
    if def.health < 0 && size > 0.0 {
        return Err(BlockError::Derivation {
            name: def.name.clone(),
            field: "health",
            expected: 1.0,
            found: def.health as f32,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    fn table() -> (ContentRegistry, BlockTable) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        (content, table)
    }

    #[test]
    fn table_covers_registry_and_view_reads() {
        let (content, table) = table();
        assert_eq!(table.len(), content.blocks().len());
        let wall = content.block_id("copper-wall").expect("copper-wall");
        let view = table.view(wall).expect("view");
        assert_eq!(view.size(), 1);
        assert!(!view.has_items());
        assert!(view.solid());
        assert_eq!(view.health(), 320);
        assert!(!view.rotate());
        assert_eq!(view.offset(), 0.0);
    }

    /// GAPS #62: `Block.flipRotation`/`planRotation`/`invertFlip` and
    /// `getPlanConfigs` on the runtime instance.
    #[test]
    fn block_plan_helpers_match_java() {
        use crate::content::BlockKind;
        let (content, table) = table();
        // A rotating block: a horizontal flip of an even rotation flips it.
        let belt = table
            .instance(content.block_id("conveyor").unwrap())
            .unwrap();
        assert!(belt.rotate);
        assert_eq!(belt.plan_rotation(3), 3);
        // `(x == (rotation % 2 == 0)) != invertFlip` (`Block.java:1658`).
        assert_eq!(belt.flip_rotation(1, true), 1);
        assert_eq!(belt.flip_rotation(1, false), 3);
        assert_eq!(belt.flip_rotation(2, true), 0);
        assert_eq!(belt.flip_rotation(2, false), 2);
        // Walls do not rotate: `planRotation` locks the rotation to 0.
        let wall = table
            .instance(content.block_id("copper-wall").unwrap())
            .unwrap();
        assert!(!wall.rotate);
        assert_eq!(wall.plan_rotation(2), 0);
        // `electrolyzer` is the only `invertFlip` block (`Blocks.java:1363`).
        let electrolyzer = table
            .instance(content.block_id("electrolyzer").unwrap())
            .unwrap();
        assert!(electrolyzer.invert_flip());
        assert_eq!(electrolyzer.flip_rotation(1, true), 3);
        assert_eq!(electrolyzer.flip_rotation(1, false), 1);
        // Plan config classes come from the behavior's `configurations`.
        let sorter = table.instance(content.block_id("sorter").unwrap()).unwrap();
        let sorter_options = sorter.plan_configs(&content, None);
        assert!(
            sorter_options
                .iter()
                .any(|option| option.kind == ContentType::Item)
        );
        assert!(
            !sorter_options
                .iter()
                .any(|option| option.kind == ContentType::Liquid)
        );
        let wall = table
            .instance(content.block_id("copper-wall").unwrap())
            .unwrap();
        assert!(wall.plan_configs(&content, None).is_empty());
        let _ = BlockKind::Wall;
    }

    /// PAR-0555 remainder: `Block.getPlanConfigs` overrides —
    /// `Block.java:1664-1673` default, `UnitFactory.java:203-209`,
    /// `Constructor.java:50-53`, `PayloadSource.java:71-75`,
    /// `PayloadRouter.java:49-53`.
    #[test]
    fn plan_configs_cover_the_java_overrides() {
        let (content, table) = table();
        let options = |name: &str, rules: Option<&Rules>| {
            table
                .get_named(name)
                .unwrap_or_else(|| panic!("{name}"))
                .plan_configs(&content, rules)
        };
        // Default: a `config(Item.class, ...)` block lists every item, a
        // `config(Liquid.class, ...)` block lists every liquid.
        let sorter = options("sorter", None);
        assert_eq!(sorter.len(), content.items().len());
        assert!(sorter.iter().all(|option| option.kind == ContentType::Item));
        // `Block.getPlanConfigs` does not fall back to the item/liquid container
        // flags: a block with `hasItems` but no `Item.class` config has none.
        let conveyor = table.get_named("conveyor").unwrap();
        assert!(conveyor.def.has_items, "conveyors carry items");
        assert!(conveyor.plan_configs(&content, None).is_empty());
        // `UnitFactory.getPlanConfigs`: its plan units only.
        let factory = options("ground-factory", None);
        assert!(!factory.is_empty());
        assert!(
            factory
                .iter()
                .all(|option| option.kind == ContentType::Unit)
        );
        let unit_plans: Vec<String> = table
            .get_named("ground-factory")
            .unwrap()
            .def
            .unit_plans
            .iter()
            .filter_map(|plan| content.unit(plan.unit).map(|unit| unit.name.clone()))
            .collect();
        let listed: Vec<String> = factory.iter().map(|option| option.name.clone()).collect();
        assert_eq!(listed, unit_plans);
        // `Constructor.canProduce`: blocks within `minBlockSize..=maxBlockSize`,
        // never cores, never hidden/banned blocks.
        let constructor = options("constructor", None);
        assert!(!constructor.is_empty());
        assert!(
            constructor
                .iter()
                .all(|option| option.kind == ContentType::Block)
        );
        let sizes: Vec<i32> = constructor
            .iter()
            .map(|option| {
                content
                    .block_id(&option.name)
                    .and_then(|id| content.block(id))
                    .unwrap()
                    .size
            })
            .collect();
        assert!(sizes.iter().all(|size| (1..=2).contains(size)), "{sizes:?}");
        assert!(
            !constructor
                .iter()
                .any(|option| option.name == "core-shard" || option.name == "air"),
            "cores and hidden blocks are not producible"
        );
        // `large-constructor` uses size 3..=4 (`Blocks.java:6719-6720`).
        assert!(options("large-constructor", None).iter().all(|option| {
            content
                .block_id(&option.name)
                .and_then(|id| content.block(id))
                .is_some_and(|def| (3..=4).contains(&def.size))
        }));
        // `PayloadSource.canProduce`: blocks smaller than the source (size 5,
        // `Blocks.java:6782`) plus non-hidden, env-supporting units.
        let source = options("payload-source", None);
        assert!(
            source
                .iter()
                .any(|option| option.kind == ContentType::Block)
        );
        assert!(source.iter().any(|option| option.kind == ContentType::Unit));
        assert!(
            !source
                .iter()
                .filter(|option| option.kind == ContentType::Block)
                .any(|option| content
                    .block_id(&option.name)
                    .and_then(|id| content.block(id))
                    .is_some_and(|def| def.size >= 5)),
            "a payload source only produces blocks smaller than itself"
        );
        assert!(
            source
                .iter()
                .filter(|option| option.kind == ContentType::Block)
                .any(|option| content
                    .block_id(&option.name)
                    .and_then(|id| content.block(id))
                    .is_some_and(|def| def.size == 4)),
            "size-4 blocks fit inside the size-5 source"
        );
        // `PayloadRouter.canSort`: blocks up to the router size (3) plus units.
        let router = options("payload-router", None);
        assert!(
            router
                .iter()
                .any(|option| option.kind == ContentType::Block)
        );
        assert!(router.iter().any(|option| option.kind == ContentType::Unit));
        assert!(
            !router
                .iter()
                .filter(|option| option.kind == ContentType::Block)
                .any(|option| option.name == "core-shard"),
            "a core cannot be sorted"
        );
        // Rules filters: a banned unit disappears from a unit-factory plan list,
        // a banned block from the constructor list.
        let mut rules = Rules::default();
        rules.banned_units.insert(unit_plans[0].clone());
        let filtered = options("ground-factory", Some(&rules));
        assert_eq!(filtered.len(), factory.len() - 1);
        let mut rules = Rules::default();
        rules.banned_blocks.insert(constructor[0].name.clone());
        let filtered = options("constructor", Some(&rules));
        assert!(
            filtered
                .iter()
                .all(|option| option.name != constructor[0].name),
            "banned blocks are dropped from Constructor.getPlanConfigs"
        );
        // Label bundle keys follow the content type.
        assert_eq!(sorter[0].label(), format!("item.{}.name", sorter[0].name));
    }

    #[test]
    fn derivation_matches_def() {
        let (content, _table) = table();
        for def in content.blocks() {
            assert_derivations(def).unwrap_or_else(|error| panic!("{error}"));
        }
    }

    #[test]
    fn rotate_predicate_wires_plan04_seam() {
        let (content, table) = table();
        // `Conveyor.java:40` sets `rotate = true`; `Door`/`Wall` keep the
        // `Block.java:122` default.
        let conveyor = content.block_id("conveyor").expect("conveyor");
        assert!(table.rotate(conveyor));
        let door = content.block_id("door").expect("door");
        assert!(!table.rotate(door));
        let wall = content.block_id("copper-wall").expect("copper-wall");
        assert!(!table.rotate(wall));
    }

    #[test]
    fn consumers_are_lowered_in_declaration_order() {
        let (content, table) = table();
        let smelter = content
            .block_id("silicon-smelter")
            .expect("silicon-smelter");
        let view = table.view(smelter).expect("view");
        assert!(!view.consumers().is_empty());
        assert!(view.consumers().cons_power.is_some());
    }
}

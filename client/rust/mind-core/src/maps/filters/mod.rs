// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Generation filters (`maps/filters/*`, plan 06 §3.8).
//!
//! Ported from `maps/filters/GenerateFilter.java`, `FilterOption.java` and the
//! 16 filter subclasses. Deviation §2.3.4: `GenerateInput.tile()` reads a local
//! packed-state buffer, and `apply_tiles` maintains Java's exact read-visibility
//! rules (unbuffered = write-through; buffered = pre-pass snapshot). The engine
//! is a single implementation here; per-filter `apply` bodies live in
//! [`builtin`].

pub mod builtin;
pub mod option;

pub use option::{BlockPredicate, FilterOption};

use bevy_ecs::prelude::Resource;

use crate::content::{BlockDef, BlockId, BlockKind, ContentRegistry};
use crate::determinism::RngStream;
use crate::determinism::SimRng;
use crate::math::{noise, ridged};
use crate::world::tiles::Tiles;

/// Maximum execution budget for `LogicFilter` (`maxInstructionsExecution`).
pub const MAX_LOGIC_INSTRUCTIONS: i32 = 500 * 500 * 25;

/// Errors raised while (de)serializing or building filters.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum FilterError {
    /// The `class` tag is missing or unknown (skipped by the caller).
    #[error("unknown filter class `{0}`")]
    UnknownClass(String),
    /// A filter JSON payload was structurally invalid.
    #[error("invalid filter json: {0}")]
    Json(String),
    /// A logic filter needs plan 13's script runner (deferred).
    #[error("logic filter execution is a plan-13 hook")]
    LogicUnsupported,
}

impl From<serde_json::Error> for FilterError {
    fn from(error: serde_json::Error) -> Self {
        FilterError::Json(error.to_string())
    }
}

/// Shared block classification helpers (`Floor`/`OverlayFloor`/`synthetic`).
pub mod block_info {
    use super::*;

    /// Liquid floors (`Floor.isLiquid`; plan 02 does not carry the field).
    pub const LIQUID_FLOORS: &[&str] = &[
        "deep-water",
        "shallow-water",
        "tainted-water",
        "deep-tainted-water",
        "darksand-tainted-water",
        "sand-water",
        "darksand-water",
        "tar",
        "pooled-cryofluid",
        "molten-slag",
        "arkycite-floor",
    ];

    /// `Block instanceof Floor` (overlay floors included).
    pub fn is_floor(def: &BlockDef) -> bool {
        matches!(
            def.kind,
            BlockKind::Floor
                | BlockKind::EmptyFloor
                | BlockKind::OverlayFloor
                | BlockKind::OreBlock
                | BlockKind::ShallowLiquid
                | BlockKind::ColoredFloor
                | BlockKind::CharacterOverlay
                | BlockKind::RuneOverlay
                | BlockKind::SteamVent
        )
    }

    /// `Block instanceof OverlayFloor`.
    pub fn is_overlay(def: &BlockDef) -> bool {
        matches!(
            def.kind,
            BlockKind::OverlayFloor
                | BlockKind::OreBlock
                | BlockKind::CharacterOverlay
                | BlockKind::RuneOverlay
        )
    }

    /// `floorsOnly`: a floor that is not an overlay floor.
    pub fn floors_only(def: &BlockDef) -> bool {
        is_floor(def) && !is_overlay(def)
    }

    /// `wallsOnly`: a non-synthetic, non-floor block.
    pub fn walls_only(def: &BlockDef) -> bool {
        !synthetic(def) && !is_floor(def)
    }

    /// `Block.synthetic()` == `update || destructible`.
    pub fn synthetic(def: &BlockDef) -> bool {
        def.update || def.destructible
    }

    /// `Block.isStatic()` (`cacheLayer == walls`; kind approximation).
    pub fn is_static(def: &BlockDef) -> bool {
        matches!(
            def.kind,
            BlockKind::StaticWall
                | BlockKind::Cliff
                | BlockKind::StaticTree
                | BlockKind::StaticProp
                | BlockKind::ColoredWall
        )
    }

    /// `Floor.isLiquid` (name table; see [`LIQUID_FLOORS`]).
    pub fn is_liquid(def: &BlockDef) -> bool {
        LIQUID_FLOORS.contains(&def.name.as_str())
    }

    /// `Floor.hasSurface()` == `!isLiquid && !solid`.
    pub fn has_surface(def: &BlockDef) -> bool {
        !is_liquid(def) && !def.solid
    }

    /// `Floor.isDeep()` (`drownTime > 0`; name approximation).
    pub fn is_deep(def: &BlockDef) -> bool {
        def.name.starts_with("deep-") || matches!(def.name.as_str(), "tar" | "molten-slag")
    }

    /// `OverlayFloor.needsSurface` (true for ores; other overlays default true).
    pub fn needs_surface(def: &BlockDef) -> bool {
        !matches!(def.kind, BlockKind::SpawnBlock)
    }

    /// The default decoration block (`<name>-boulder`) or `None`.
    pub fn decoration(content: &ContentRegistry, def: &BlockDef) -> Option<BlockId> {
        content
            .block_by_name(&format!("{}-boulder", def.name))
            .map(|block| block.id)
    }

    /// The default wall block (`<name>-wall`, with the dune special case).
    pub fn wall(content: &ContentRegistry, def: &BlockDef) -> Option<BlockId> {
        let name = def.name.clone();
        content
            .block_by_name(&format!("{name}-wall"))
            .or_else(|| {
                name.contains("darksand")
                    .then(|| {
                        content.block_by_name(&format!("{}-wall", name.replace("darksand", "dune")))
                    })
                    .flatten()
            })
            .map(|block| block.id)
    }

    /// Whether `block` is solid (`Block.solid`; floors only).
    pub fn solid(registry: &ContentRegistry, block: BlockId) -> bool {
        registry.block(block).is_some_and(|def| def.solid)
    }
}

/// One tile's generation read/write state (`GenerateInput` fields).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedState {
    /// Block id.
    pub block: BlockId,
    /// Floor id.
    pub floor: BlockId,
    /// Overlay id.
    pub overlay: BlockId,
    /// Packed tile data (`Tile.getPackedData`).
    pub packed_data: i64,
}

impl Default for PackedState {
    fn default() -> Self {
        Self {
            block: BlockId::AIR,
            floor: BlockId::AIR,
            overlay: BlockId::AIR,
            packed_data: 0,
        }
    }
}

/// The generation read/write input (`GenerateInput`).
///
/// `states` is the local packed buffer `tile()` reads; the engine keeps it in
/// sync per the buffered/unbuffered rule (§2.3.4). `world` carries the
/// persistent generation ECS resources (`WorldGrid`/`GlobalVars`/
/// `LogicContentIndex`/`LogicWorldState`) for post filters that run privileged
/// logic scripts; `None` means the stack was applied without a booted world.
#[derive(Default)]
pub struct GenerateInput<'a> {
    /// Current tile x.
    pub x: i32,
    /// Current tile y.
    pub y: i32,
    /// Grid width.
    pub width: i32,
    /// Grid height.
    pub height: i32,
    /// Current output floor.
    pub floor: BlockId,
    /// Current output block.
    pub block: BlockId,
    /// Current output overlay.
    pub overlay: BlockId,
    /// Current packed data.
    pub packed_data: i64,
    /// Content registry for block classification (`Block.synthetic()` etc.).
    pub content: Option<&'a ContentRegistry>,
    /// Read buffer (flat `x + y*width`).
    pub states: Vec<PackedState>,
    /// Buffered output slot.
    pub output: Vec<PackedState>,
    /// Persistent generation ECS world (see the type-level doc).
    pub world: Option<&'a mut bevy_ecs::world::World>,
}

impl<'a> GenerateInput<'a> {
    /// `GenerateInput.begin(width, height)`.
    pub fn begin(&mut self, width: i32, height: i32) {
        self.width = width;
        self.height = height;
        self.x = 0;
        self.y = 0;
        self.floor = BlockId::AIR;
        self.block = BlockId::AIR;
        self.overlay = BlockId::AIR;
        self.packed_data = 0;
        self.states.clear();
        self.output.clear();
    }

    /// `GenerateInput.set`: load one tile's fields.
    pub fn set_state(&mut self, state: PackedState, x: i32, y: i32) {
        self.floor = state.floor;
        self.block = state.block;
        self.overlay = state.overlay;
        self.packed_data = state.packed_data;
        self.x = x;
        self.y = y;
    }

    /// Reads a neighbouring tile, clamped (`GenerateInput.tile`).
    pub fn tile(&self, x: f32, y: f32) -> PackedState {
        if self.states.is_empty() || self.width <= 0 || self.height <= 0 {
            return PackedState::default();
        }
        let cx = (x as i32).clamp(0, self.width - 1);
        let cy = (y as i32).clamp(0, self.height - 1);
        let index = (cx + cy * self.width) as usize;
        self.states.get(index).copied().unwrap_or_default()
    }

    /// The output state built from the current fields.
    pub fn out_state(&self) -> PackedState {
        PackedState {
            block: self.block,
            floor: self.floor,
            overlay: self.overlay,
            packed_data: self.packed_data,
        }
    }

    fn def(&self, id: BlockId) -> Option<&BlockDef> {
        self.content.and_then(|content| content.block(id))
    }

    /// `in.block.synthetic()`.
    pub fn block_synthetic(&self, id: BlockId) -> bool {
        self.def(id).is_some_and(block_info::synthetic)
    }

    /// `block.isOverlay()`.
    pub fn block_is_overlay(&self, id: BlockId) -> bool {
        self.def(id).is_some_and(block_info::is_overlay)
    }

    /// `block.isFloor()`.
    pub fn block_is_floor(&self, id: BlockId) -> bool {
        self.def(id).is_some_and(block_info::is_floor)
    }

    /// `block.isStatic()`.
    pub fn block_is_static(&self, id: BlockId) -> bool {
        self.def(id).is_some_and(block_info::is_static)
    }

    /// `block.solid`.
    pub fn block_solid(&self, id: BlockId) -> bool {
        self.def(id).is_some_and(|def| def.solid)
    }

    /// `in.floor.asFloor().hasSurface()`.
    pub fn floor_has_surface(&self, id: BlockId) -> bool {
        self.def(id).is_some_and(block_info::has_surface)
    }

    /// `in.floor.asFloor().isDeep()`.
    pub fn floor_is_deep(&self, id: BlockId) -> bool {
        self.def(id).is_some_and(block_info::is_deep)
    }

    /// `in.floor.asFloor().needsSurface`.
    pub fn floor_needs_surface(&self, id: BlockId) -> bool {
        self.def(id).is_some_and(block_info::needs_surface)
    }
}

/// Filter helper: `GenerateFilter.noise(in, ...)` (seed-relative).
pub fn filter_noise(inp: &GenerateInput, seed: i32, scl: f32, mag: f32) -> f32 {
    noise::noise2d(
        seed,
        1,
        0.0,
        1.0 / scl as f64,
        (inp.x + 10) as f64,
        (inp.y + 10) as f64,
    ) * mag
}

/// Filter helper: `GenerateFilter.noise(in, scl, mag, octaves, persistence)`.
pub fn filter_noise_oct(
    inp: &GenerateInput,
    seed: i32,
    scl: f32,
    mag: f32,
    octaves: f32,
    persistence: f32,
) -> f32 {
    noise::noise2d(
        seed,
        octaves as i32,
        persistence as f64,
        1.0 / scl as f64,
        (inp.x + 10) as f64,
        (inp.y + 10) as f64,
    ) * mag
}

/// Filter helper: `GenerateFilter.noise(x, y, scl, mag, octaves, persistence)`.
pub fn filter_noise_xy(
    seed: i32,
    x: f32,
    y: f32,
    scl: f32,
    mag: f32,
    octaves: f32,
    persistence: f32,
) -> f32 {
    noise::noise2d(
        seed,
        octaves as i32,
        persistence as f64,
        1.0 / scl as f64,
        (x + 10.0) as f64,
        (y + 10.0) as f64,
    ) * mag
}

/// Filter helper: `GenerateFilter.rnoise(x, y, scl, mag)` (ridged, seed+1).
pub fn filter_rnoise(seed: i32, x: f32, y: f32, scl: f32, mag: f32) -> f32 {
    ridged::noise2d(seed + 1, x as f64, y as f64, 1, 0.5, 1.0 / scl as f64) * mag
}

/// Filter helper: `GenerateFilter.rnoise(x, y, octaves, scl, falloff, mag)`.
pub fn filter_rnoise_oct(
    seed: i32,
    x: f32,
    y: f32,
    octaves: i32,
    scl: f32,
    falloff: f32,
    mag: f32,
) -> f32 {
    ridged::noise2d(
        seed + 1,
        x as f64,
        y as f64,
        octaves,
        falloff as f64,
        1.0 / scl as f64,
    ) * mag
}

/// `GenerateFilter.chance(x, y)`: deterministic per-coordinate hash in `[0,1]`.
pub fn filter_chance(x: i32, y: i32, seed: i32) -> f32 {
    // Arc `Mathf.randomSeed(Pack.longInt(x, y + seed))`: a 32-bit hash → [0,1).
    let packed = ((x as i64 & 0xffff_ffff) | (((y + seed) as i64 & 0xffff_ffff) << 32)) as u64;
    let mut z = packed.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9_u64);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB_u64);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u32 << 24) as f32
}

/// A generation filter (`GenerateFilter`). Object-safe.
pub trait GenerateFilter: Send + Sync {
    /// Configurable option descriptors (plan 14 renders).
    fn options(&self) -> Vec<FilterOption> {
        Vec::new()
    }

    /// The per-tile filter body (`GenerateFilter.apply(GenerateInput)`).
    fn apply(&mut self, _input: &mut GenerateInput<'_>) {}

    /// Whether the filter needs a snapshot buffer (`GenerateFilter.isBuffered`).
    fn is_buffered(&self) -> bool {
        false
    }

    /// Whether the filter can only run during generation (`isPost`).
    fn is_post(&self) -> bool {
        false
    }

    /// Editor icon (`Iconc`; `'\0'` when none).
    fn icon(&self) -> char {
        '\0'
    }

    /// The block whose `uiIcon` the editor icon stands for (`Iconc.block*`),
    /// named for the Godot atlas (`block-<name>-ui`). `None` when `icon()` is
    /// `'\0'`.
    fn icon_block(&self) -> Option<&'static str> {
        None
    }

    /// The filter seed (`GenerateFilter.seed`).
    fn seed(&self) -> i32 {
        0
    }

    /// Sets the filter seed.
    fn set_seed(&mut self, _seed: i32) {}

    /// Randomized seed (`GenerateFilter.randomize`); deterministic `MapGen` draw.
    fn randomize(&mut self, rng: &mut SimRng) {
        let seed = rng.random(RngStream::MapGen, 999_999_999);
        self.set_seed(seed);
    }

    /// Lowercased class name minus `Filter` (`GenerateFilter.simpleName`).
    fn simple_name(&self) -> &'static str;

    /// The JSON class tag (camelized class name minus `Filter`).
    fn class_tag(&self) -> &'static str;

    /// Serializes this filter (`class` + public fields + `seed`).
    fn to_json(&self, registry: &ContentRegistry) -> serde_json::Value;

    /// Runs the filter over the whole grid (`GenerateFilter.apply(Tiles, in)`).
    ///
    /// Default implementation: the buffered/unbuffered engine. Post filters
    /// (core/enemy spawn, spawn path, logic, random item) override this.
    fn apply_tiles(
        &mut self,
        tiles: &mut Tiles,
        input: &mut GenerateInput<'_>,
        content: &ContentRegistry,
        rng: &mut SimRng,
    ) {
        apply_default_engine(self, tiles, input, content, rng);
    }
}

/// The default buffered/unbuffered engine (deviation §2.3.4).
fn apply_default_engine<F: GenerateFilter + ?Sized>(
    filter: &mut F,
    tiles: &mut Tiles,
    input: &mut GenerateInput<'_>,
    content: &ContentRegistry,
    _rng: &mut SimRng,
) {
    let width = tiles.width;
    let height = tiles.height;
    let n = (width * height) as usize;
    input.begin(width, height);
    input.states.reserve(n);
    for i in 0..n {
        let tile = tiles.geti(i);
        input.states.push(PackedState {
            block: tile.block,
            floor: tile.floor,
            overlay: tile.overlay,
            packed_data: tile.get_packed_data(),
        });
    }

    let buffered = filter.is_buffered();
    if buffered {
        input.output = vec![PackedState::default(); n];
        for i in 0..n {
            let (x, y) = ((i as i32) % width, (i as i32) / width);
            input.set_state(input.states[i], x, y);
            filter.apply(input);
            let mut out = input.out_state();
            // Buffered filters never transfer packed data (Java `PackTile`).
            out.packed_data = input.states[i].packed_data;
            input.output[i] = out;
        }
        for i in 0..n {
            apply_result(tiles, content, i as i32, input.output[i], true);
        }
    } else {
        for i in 0..n {
            let (x, y) = ((i as i32) % width, (i as i32) / width);
            input.set_state(input.states[i], x, y);
            filter.apply(input);
            let out = input.out_state();
            input.states[i] = out;
            apply_result(tiles, content, i as i32, out, false);
        }
    }
}

/// Writes one filter result to a tile with Java's post-steps.
fn apply_result(
    tiles: &mut Tiles,
    content: &ContentRegistry,
    index: i32,
    state: PackedState,
    buffered: bool,
) {
    let index = index as usize;
    let old_block = tiles.geti(index).block;
    let floor_def = content.block(state.floor);
    let overlay_def = content.block(state.overlay);

    // Java: `!floor.hasSurface() && overlay.asFloor().needsSurface && overlay instanceof OreBlock`.
    let overlay = match (floor_def, overlay_def) {
        (Some(floor), Some(overlay))
            if !block_info::has_surface(floor)
                && block_info::needs_surface(overlay)
                && overlay.kind == BlockKind::OreBlock =>
        {
            BlockId::AIR
        }
        _ => state.overlay,
    };

    let new_block = content.block(state.block);
    let tile = tiles.geti_mut(index);
    tile.floor = state.floor;
    tile.overlay = overlay;
    if !buffered {
        // Unbuffered transfers packed data.
        tile.set_packed_data(state.packed_data);
    }
    let old_synthetic = content.block(old_block).is_some_and(block_info::synthetic);
    let new_synthetic = new_block.is_some_and(block_info::synthetic);
    if !old_synthetic && !new_synthetic {
        tile.block = state.block;
    }
}

/// Applies a filter stack (`World.FilterContext.applyFilters`).
///
/// Filters are randomized from the caller's `MapGen` stream (OD6-A). Logic
/// filters need the privileged VM's persistent resources, so when the stack
/// contains one this boots a scratch generation world
/// ([`crate::world::generation`]); callers that already own a live generation
/// world should use [`apply_stack_in`] instead.
pub fn apply_stack(
    tiles: &mut Tiles,
    stack: &mut [Box<dyn GenerateFilter>],
    content: &ContentRegistry,
    rng: &mut SimRng,
) {
    if stack.iter().any(|filter| filter.class_tag() == "logic") {
        let mut ecs = bevy_ecs::world::World::new();
        crate::world::generation::ensure_generation_resources(
            &mut ecs,
            content,
            tiles.width,
            tiles.height,
        );
        apply_stack_in(tiles, stack, content, rng, Some(&mut ecs));
    } else {
        apply_stack_in(tiles, stack, content, rng, None);
    }
}

/// Applies a filter stack against an optional persistent generation ECS world.
///
/// `ecs` must be the world booted by
/// [`crate::world::generation::install_generation_resources`] (or one for which
/// [`crate::world::generation::ensure_generation_resources`] has run), so
/// `LogicFilter` reads/mutates the live resources rather than building new ones.
pub fn apply_stack_in(
    tiles: &mut Tiles,
    stack: &mut [Box<dyn GenerateFilter>],
    content: &ContentRegistry,
    rng: &mut SimRng,
    ecs: Option<&mut bevy_ecs::world::World>,
) {
    let mut input: GenerateInput<'_> = GenerateInput {
        content: Some(content),
        world: ecs,
        ..GenerateInput::default()
    };
    for filter in stack.iter_mut() {
        filter.randomize(rng);
        filter.apply_tiles(tiles, &mut input, content, rng);
    }
}

/// Filter class constructor (`Prov<GenerateFilter>`).
type FilterCtor = fn() -> Box<dyn GenerateFilter>;

/// The registry of filter class tags (`Maps.allFilterTypes`).
#[derive(Resource, Clone)]
pub struct FilterRegistry {
    types: Vec<(&'static str, FilterCtor)>,
}

impl Default for FilterRegistry {
    fn default() -> Self {
        Self::vanilla()
    }
}

impl FilterRegistry {
    /// Empty registry.
    pub fn empty() -> Self {
        Self { types: Vec::new() }
    }

    /// The 15 registered vanilla filters, in `Maps.allFilterTypes` order.
    pub fn vanilla() -> Self {
        let mut registry = Self::empty();
        registry.register("noise", || Box::new(builtin::NoiseFilter::default()));
        registry.register("scatter", || Box::new(builtin::ScatterFilter::default()));
        registry.register("terrain", || Box::new(builtin::TerrainFilter::default()));
        registry.register("distort", || Box::new(builtin::DistortFilter::default()));
        registry.register("riverNoise", || {
            Box::new(builtin::RiverNoiseFilter::default())
        });
        registry.register("ore", || Box::new(builtin::OreFilter::default()));
        registry.register(
            "oreMedian",
            || Box::new(builtin::OreMedianFilter::default()),
        );
        registry.register("median", || Box::new(builtin::MedianFilter::default()));
        registry.register("blend", || Box::new(builtin::BlendFilter::default()));
        registry.register("mirror", || Box::new(builtin::MirrorFilter::default()));
        registry.register("clear", || Box::new(builtin::ClearFilter::default()));
        registry.register(
            "coreSpawn",
            || Box::new(builtin::CoreSpawnFilter::default()),
        );
        registry.register("enemySpawn", || {
            Box::new(builtin::EnemySpawnFilter::default())
        });
        registry.register(
            "spawnPath",
            || Box::new(builtin::SpawnPathFilter::default()),
        );
        registry.register("logic", || Box::new(builtin::LogicFilter::default()));
        registry
    }

    /// Registers a filter type (last registration wins for a tag).
    pub fn register(&mut self, tag: &'static str, ctor: FilterCtor) {
        if let Some(entry) = self.types.iter_mut().find(|(name, _)| *name == tag) {
            entry.1 = ctor;
        } else {
            self.types.push((tag, ctor));
        }
    }

    /// Whether a class tag is registered.
    pub fn contains(&self, tag: &str) -> bool {
        self.types.iter().any(|(name, _)| *name == tag)
    }

    /// The registered class tags in order.
    pub fn tags(&self) -> Vec<&'static str> {
        self.types.iter().map(|(name, _)| *name).collect()
    }

    /// Constructs a default filter by class tag.
    pub fn create(&self, tag: &str) -> Option<Box<dyn GenerateFilter>> {
        self.types
            .iter()
            .find(|(name, _)| *name == tag)
            .map(|(_, ctor)| ctor())
    }

    /// Builds a filter from a `genfilters` JSON object.
    ///
    /// Missing fields fall back to the class defaults (upstream `JsonIO.read`
    /// instantiates then overlays); unknown fields are ignored.
    pub fn from_json(
        &self,
        registry: &ContentRegistry,
        value: &serde_json::Value,
    ) -> Result<Box<dyn GenerateFilter>, FilterError> {
        let class = value
            .get("class")
            .and_then(|class| class.as_str())
            .ok_or_else(|| FilterError::UnknownClass("<missing>".to_owned()))?;
        if !self.contains(class) {
            return Err(FilterError::UnknownClass(class.to_owned()));
        }
        let base = builtin::FilterJson::default_for(class)
            .ok_or_else(|| FilterError::UnknownClass(class.to_owned()))?;
        let mut merged = serde_json::to_value(&base)?;
        merge_json(&mut merged, value.clone());
        let parsed: builtin::FilterJson = serde_json::from_value(merged)?;
        parsed.into_filter(registry)
    }
}

/// Deep object merge (`JsonIO.readInto`): `overlay` fields win.
fn merge_json(base: &mut serde_json::Value, overlay: serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base), serde_json::Value::Object(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(&key) {
                    Some(existing) if existing.is_object() && value.is_object() => {
                        merge_json(existing, value);
                    }
                    _ => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

/// Reads a `genfilters` JSON string (`Maps.readFilters`).
///
/// Empty/parse errors fall back to the default stack (upstream behavior).
pub fn read_filters(content: &ContentRegistry, json: &str) -> Vec<Box<dyn GenerateFilter>> {
    if json.trim().is_empty() {
        return default_filter_stack(content);
    }
    match parse_filters(content, json, &FilterRegistry::vanilla()) {
        Ok(filters) => filters,
        Err(error) => {
            log::error!("failed to read generation filters: {error}");
            default_filter_stack(content)
        }
    }
}

/// Rewrites every integral float in a `genfilters` array as an integer.
///
/// GDScript's `JSON.parse_string` turns every JSON number into a `float`, so a
/// dialog round-trip (`JsonIO.write`-shaped JSON -> Dictionary -> stringify)
/// would write `"seed": 0.0`, which `serde` rejects for the `i32` fields
/// (`angle`/`radius`/`amount`/`seed`). Upstream `JsonIO.write` keeps `int`
/// fields integral; normalizing at the write boundary restores that.
/// Non-integral values (and parse failures) pass through unchanged.
pub fn normalize_filter_numbers(json: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(mut value) => {
            normalize_numbers(&mut value);
            serde_json::to_string(&value).unwrap_or_else(|_| json.to_owned())
        }
        Err(_) => json.to_owned(),
    }
}

/// Recursive integral-float rewrite used by [`normalize_filter_numbers`].
fn normalize_numbers(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Number(number) => {
            if let Some(float) = number.as_f64()
                && number.is_f64()
                && float.fract() == 0.0
                && float.abs() <= 9_007_199_254_740_992.0
            {
                *number = serde_json::Number::from(float as i64);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                normalize_numbers(item);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values_mut() {
                normalize_numbers(item);
            }
        }
        _ => {}
    }
}

/// Parses a `genfilters` array; unknown classes are warned + skipped.
pub fn parse_filters(
    content: &ContentRegistry,
    json: &str,
    registry: &FilterRegistry,
) -> Result<Vec<Box<dyn GenerateFilter>>, FilterError> {
    let values: Vec<serde_json::Value> = serde_json::from_str(json)?;
    let mut filters = Vec::with_capacity(values.len());
    for value in &values {
        match registry.from_json(content, value) {
            Ok(filter) => filters.push(filter),
            Err(FilterError::UnknownClass(class)) => {
                log::warn!("unknown generation filter class `{class}`; skipping");
            }
            Err(error) => return Err(error),
        }
    }
    Ok(filters)
}

/// Writes a filter stack back to `genfilters` JSON (`JsonIO.write`).
pub fn write_filters(content: &ContentRegistry, stack: &[Box<dyn GenerateFilter>]) -> String {
    let values: Vec<serde_json::Value> =
        stack.iter().map(|filter| filter.to_json(content)).collect();
    serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_owned())
}

/// Serializes the current stack's `FilterOption` descriptors + values for the
/// Godot `MapGenerateDialog` (`MapGenerateDialog.rebuildFilters` walks
/// `filter.options()` and each option's live getter).
///
/// Rows are in tag order. `values` is the filter's serialized JSON (class
/// defaults merged with the tag's fields), so the dialog renders concrete
/// defaults for partial `genfilters` entries without knowing any schema.
pub fn filter_options_json(content: &ContentRegistry, json: &str) -> String {
    let registry = FilterRegistry::vanilla();
    let filters = match parse_filters(content, json, &registry) {
        Ok(filters) => filters,
        Err(_) => default_filter_stack(content),
    };
    let rows: Vec<serde_json::Value> = filters
        .iter()
        .map(|filter| {
            let options: Vec<serde_json::Value> =
                filter.options().iter().map(FilterOption::to_json).collect();
            serde_json::json!({
                "class": filter.class_tag(),
                "name": filter.simple_name(),
                "icon": filter.icon_block().unwrap_or(""),
                "post": filter.is_post(),
                "options": options,
                "values": filter.to_json(content),
            })
        })
        .collect();
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_owned())
}

/// Serializes `Maps.allFilterTypes` for the dialog's `@add` picker: class tag,
/// `simpleName` (bundle `filter.<name>`) and `isPost` (post filters are hidden
/// in applied mode).
pub fn filter_types_json() -> String {
    let registry = FilterRegistry::vanilla();
    let rows: Vec<serde_json::Value> = registry
        .types
        .iter()
        .map(|(tag, ctor)| {
            let filter = ctor();
            serde_json::json!({
                "class": tag,
                "name": filter.simple_name(),
                "icon": filter.icon_block().unwrap_or(""),
                "post": filter.is_post(),
            })
        })
        .collect();
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_owned())
}

/// `Maps.addDefaultOres`: one `OreFilter` per `oreDefault` overlay
/// (`Maps.java:354-363`), in content-id order.
pub fn default_ore_filters(content: &ContentRegistry) -> Vec<Box<dyn GenerateFilter>> {
    let mut filters: Vec<Box<dyn GenerateFilter>> = Vec::new();
    for def in content.blocks() {
        if block_info::is_overlay(def) && def.ore_default {
            filters.push(Box::new(builtin::OreFilter {
                seed: 0,
                scl: def.ore_scale,
                threshold: def.ore_threshold,
                octaves: 2.0,
                falloff: 0.3,
                tilt: 0.0,
                ore: def.id,
                target: BlockId::AIR,
            }));
        }
    }
    filters
}

/// The default stack (`Maps.readFilters("")`): per-floor scatter + default ores.
pub fn default_filter_stack(content: &ContentRegistry) -> Vec<Box<dyn GenerateFilter>> {
    let mut filters: Vec<Box<dyn GenerateFilter>> = Vec::new();

    for def in content.blocks() {
        if block_info::floors_only(def)
            && def.in_editor
            && let Some(decoration) = block_info::decoration(content, def)
            && decoration != BlockId::AIR
        {
            filters.push(Box::new(builtin::ScatterFilter {
                seed: 0,
                chance: 0.013,
                flooronto: def.id,
                floor: BlockId::AIR,
                block: decoration,
            }));
        }
    }

    filters.extend(default_ore_filters(content));

    filters
}

/// Lowercases the first character (Arc `Strings.camelize`).
pub fn camelize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    /// `maps::filters::tests::class_tags_roundtrip`.
    #[test]
    fn class_tags_roundtrip() {
        let registry = FilterRegistry::vanilla();
        let expected = [
            "noise",
            "scatter",
            "terrain",
            "distort",
            "riverNoise",
            "ore",
            "oreMedian",
            "median",
            "blend",
            "mirror",
            "clear",
            "coreSpawn",
            "enemySpawn",
            "spawnPath",
            "logic",
        ];
        assert_eq!(registry.tags(), expected);
        // `RandomItemFilter` is not registered (upstream parity).
        assert!(!registry.contains("randomItem"));
        assert!(registry.create("noise").is_some());
        assert!(registry.create("unknown").is_none());
        assert_eq!(camelize("RiverNoise"), "riverNoise");
    }

    #[test]
    fn default_stack_order_matches_content() {
        let content = test_registry();
        let stack = default_filter_stack(&content);
        assert!(!stack.is_empty());
        // Scatter filters come before ore filters; scatter targets are floors.
        let first_ore = stack.iter().position(|f| f.class_tag() == "ore");
        let last_scatter = stack.iter().rposition(|f| f.class_tag() == "scatter");
        if let (Some(ore), Some(scatter)) = (first_ore, last_scatter) {
            assert!(scatter < ore, "scatter filters precede ore filters");
        }
    }

    /// Test filter: copies the left neighbour's floor (to observe read state).
    struct EchoLeft {
        seed: i32,
        buffered: bool,
    }

    impl GenerateFilter for EchoLeft {
        fn simple_name(&self) -> &'static str {
            "echoleft"
        }
        fn class_tag(&self) -> &'static str {
            "echoLeft"
        }
        fn is_buffered(&self) -> bool {
            self.buffered
        }
        fn seed(&self) -> i32 {
            self.seed
        }
        fn set_seed(&mut self, seed: i32) {
            self.seed = seed;
        }
        fn apply(&mut self, input: &mut GenerateInput<'_>) {
            let left = input.tile((input.x - 1) as f32, input.y as f32);
            input.floor = left.floor;
        }
        fn to_json(&self, _registry: &ContentRegistry) -> serde_json::Value {
            serde_json::Value::Null
        }
    }

    fn echo_grid(content: &ContentRegistry) -> (crate::world::tiles::Tiles, [BlockId; 4]) {
        let ids = [
            content.block_id("stone").unwrap_or(BlockId::AIR),
            content.block_id("sand-floor").unwrap_or(BlockId::AIR),
            content.block_id("ice").unwrap_or(BlockId::AIR),
            content.block_id("moss").unwrap_or(BlockId::AIR),
        ];
        let mut tiles = crate::world::tiles::Tiles::new(4, 1);
        for (index, id) in ids.iter().enumerate() {
            tiles.geti_mut(index).floor = *id;
        }
        (tiles, ids)
    }

    fn run_echo(content: &ContentRegistry, buffered: bool) -> Vec<BlockId> {
        let (mut tiles, _) = echo_grid(content);
        let mut stack: Vec<Box<dyn GenerateFilter>> =
            vec![Box::new(EchoLeft { seed: 0, buffered })];
        let mut rng = SimRng::new(1);
        apply_stack(&mut tiles, &mut stack, content, &mut rng);
        (0..4).map(|index| tiles.geti(index).floor).collect()
    }

    /// `maps::filters::tests::buffered_reads_pre_state`.
    #[test]
    fn buffered_reads_pre_state() {
        let content = test_registry();
        let (_, ids) = echo_grid(&content);
        assert_eq!(
            run_echo(&content, true),
            vec![ids[0], ids[0], ids[1], ids[2]]
        );
    }

    /// `maps::filters::tests::unbuffered_reads_write_through`.
    #[test]
    fn unbuffered_reads_write_through() {
        let content = test_registry();
        let (_, ids) = echo_grid(&content);
        assert_eq!(
            run_echo(&content, false),
            vec![ids[0], ids[0], ids[0], ids[0]]
        );
    }

    #[test]
    fn json_roundtrip_preserves_fields() {
        let content = test_registry();
        let original: Vec<Box<dyn GenerateFilter>> = vec![
            Box::new(builtin::ScatterFilter {
                seed: 123,
                chance: 0.25,
                flooronto: content.block_id("moss").unwrap_or(BlockId::AIR),
                floor: content.block_id("sand-floor").unwrap_or(BlockId::AIR),
                block: content.block_id("stone-wall").unwrap_or(BlockId::AIR),
            }),
            Box::new(builtin::OreFilter {
                seed: 7,
                scl: 30.0,
                threshold: 0.7,
                octaves: 3.0,
                falloff: 0.4,
                tilt: 0.25,
                ore: content.block_id("ore-copper").unwrap_or(BlockId::AIR),
                target: BlockId::AIR,
            }),
        ];
        let json = write_filters(&content, &original);
        let parsed = parse_filters(&content, &json, &FilterRegistry::vanilla()).unwrap();
        assert_eq!(parsed.len(), original.len());
        // Re-serialize and compare JSON (seed/fields stable).
        let json2 = write_filters(&content, &parsed);
        let a: serde_json::Value = serde_json::from_str(&json).unwrap();
        let b: serde_json::Value = serde_json::from_str(&json2).unwrap();
        assert_eq!(a, b);
        // A default is applied for missing fields.
        let partial = r#"[{"class":"scatter","seed":5}]"#;
        let filters = parse_filters(&content, partial, &FilterRegistry::vanilla()).unwrap();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].seed(), 5);
    }

    /// Plan-13 M7/plan-06 hook: a `LogicFilter` script mutates the generation
    /// tiles through `getblock`/`setblock` on the temporary `WorldGrid`.
    #[test]
    fn logic_filter_applies_generated_script() {
        let content = test_registry();
        let wall = content.block_id("copper-wall").expect("copper-wall");
        let sand = content.block_id("sand-floor").expect("sand-floor");
        let mut tiles = crate::world::tiles::Tiles::new(3, 3);
        for index in 0..9 {
            tiles.geti_mut(index).floor = BlockId::AIR;
        }
        let code = "setblock block @copper-wall 1 1 0 0\n\
            setblock floor @sand-floor 0 0 0 0\n\
            end\n"
            .to_owned();
        let mut stack: Vec<Box<dyn GenerateFilter>> = vec![Box::new(builtin::LogicFilter {
            seed: 0,
            code: Some(code),
            loop_enabled: false,
        })];
        let mut rng = SimRng::new(1);
        apply_stack(&mut tiles, &mut stack, &content, &mut rng);
        assert_eq!(tiles.geti(4).block, wall, "setblock block applied");
        assert_eq!(tiles.geti(0).floor, sand, "setblock floor applied");
    }

    /// `MapGenerateDialog.rebuildFilters` option rows: names/fields match the
    /// Java `options()` declarations (`NoiseFilter.java:39-49`,
    /// `MirrorFilter.java:42-47`), including the int sliders.
    #[test]
    fn options_follow_java_declarations() {
        let registry = FilterRegistry::vanilla();
        let noise = registry.create("noise").expect("noise");
        let names: Vec<&str> = noise.options().iter().map(FilterOption::name).collect();
        assert_eq!(
            names,
            vec![
                "scale",
                "threshold",
                "octaves",
                "falloff",
                "tilt",
                "target",
                "floor",
                "wall"
            ]
        );
        let fields: Vec<&str> = noise.options().iter().map(FilterOption::field).collect();
        assert_eq!(
            fields,
            vec![
                "scl",
                "threshold",
                "octaves",
                "falloff",
                "tilt",
                "target",
                "floor",
                "block"
            ]
        );
        let mirror = registry.create("mirror").expect("mirror");
        match &mirror.options()[0] {
            FilterOption::Slider {
                min,
                max,
                step,
                integer,
                ..
            } => {
                assert_eq!((*min, *max, *step), (0.0, 360.0, 15.0));
                assert!(integer, "angle casts to int");
            }
            other => panic!("expected slider, got {other:?}"),
        }
        assert!(
            registry
                .create("coreSpawn")
                .expect("coreSpawn")
                .options()
                .is_empty(),
            "CoreSpawnFilter.options() is disabled upstream"
        );
    }

    /// The Godot dialog payload: class defaults merged into `values`, options
    /// and the block icon in registry order (`MapGenerateDialog.showAdd`).
    #[test]
    fn filter_options_json_merges_defaults() {
        let content = test_registry();
        let rows: serde_json::Value =
            serde_json::from_str(&filter_options_json(&content, r#"[{"class":"noise"}]"#)).unwrap();
        let rows = rows.as_array().expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["class"], "noise");
        assert_eq!(rows[0]["icon"], "pebbles");
        assert_eq!(rows[0]["values"]["scl"], 40.0);
        assert_eq!(rows[0]["values"]["threshold"], 0.5);
        assert_eq!(rows[0]["options"][0]["field"], "scl");
        assert_eq!(rows[0]["options"][5]["predicate"], "anyOptional");

        // An empty tag falls back to the default stack (scatter + ores).
        let defaults = filter_options_json(&content, "");
        let parsed: serde_json::Value = serde_json::from_str(&defaults).unwrap();
        assert!(parsed.as_array().is_some_and(|rows| !rows.is_empty()));

        // The add-picker payload: 15 vanilla types, post flags set.
        let types: serde_json::Value = serde_json::from_str(&filter_types_json()).unwrap();
        let types = types.as_array().expect("types");
        assert_eq!(types.len(), 15);
        assert_eq!(types[0]["class"], "noise");
        assert_eq!(types[0]["name"], "noise");
        assert_eq!(
            types
                .iter()
                .find(|row| row["class"] == "coreSpawn")
                .map(|row| row["post"].clone()),
            Some(serde_json::Value::Bool(true))
        );
    }

    /// `Maps.addDefaultOres` is the tail of the default stack.
    #[test]
    fn default_ores_match_stack_tail() {
        let content = test_registry();
        let ores = default_ore_filters(&content);
        assert!(
            !ores.is_empty(),
            "the base registry defines `oreDefault` overlays"
        );
        let stack = default_filter_stack(&content);
        let tail = &stack[stack.len() - ores.len()..];
        assert_eq!(
            write_filters(&content, tail),
            write_filters(&content, &ores),
            "default stack ends with the default ores"
        );
        // Every ore filter targets an `oreDefault` overlay.
        for filter in &ores {
            assert_eq!(filter.class_tag(), "ore");
        }
    }

    /// The Godot dialog writes its tag through GDScript's JSON (all numbers
    /// become floats); normalization keeps the `i32` fields parsable.
    #[test]
    fn normalize_filter_numbers_restores_integer_fields() {
        let content = test_registry();
        let raw = r#"[{"class":"scatter","seed":0.0,"chance":0.013},{"class":"mirror","seed":123.0,"angle":45.0,"rotate":false,"axisX":0.5,"axisY":0.5}]"#;
        assert!(
            parse_filters(&content, raw, &FilterRegistry::vanilla()).is_err(),
            "float seeds fail the strict parse"
        );
        let fixed = normalize_filter_numbers(raw);
        let filters = parse_filters(&content, &fixed, &FilterRegistry::vanilla()).unwrap();
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[1].seed(), 123);
        // Non-integral values stay floats; integral floats inside strings/keys
        // are untouched.
        assert!(fixed.contains("0.013"));
        let parsed: serde_json::Value = serde_json::from_str(&fixed).unwrap();
        assert!(parsed[0]["seed"].is_i64());
        assert!(parsed[1]["angle"].is_i64());
        assert!(parsed[1]["axisX"].is_f64());
        // Invalid JSON passes through unchanged.
        assert_eq!(normalize_filter_numbers("not json"), "not json");
    }
}

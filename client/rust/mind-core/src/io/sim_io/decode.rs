// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DecodedSave` and the save/map byte decoders.
//! Upstream: core/src/mindustry/io/SaveIO.java, core/src/mindustry/io/SaveVersion.java

use crate::content::ContentRegistry;
use crate::game::map_markers::MapMarkers;
use crate::io::save::state::TeamPlan;
use crate::io::save::{SaveIo, SaveReadState};
use crate::io::{IoError, StringMap};
use crate::world::WorldGrid;

use super::entities::LiveEntitySink;

/// One decoded save (`SaveIO.load` output before it touches the world).
pub struct DecodedSave {
    /// The decoded tile grid.
    pub grid: WorldGrid,
    /// Decoded building payloads awaiting ECS spawn.
    pub pending: Vec<(usize, crate::world::building_io::DecodedBase)>,
    /// Team build plans read from the `entities` region.
    pub plans: Vec<(i32, Vec<TeamPlan>)>,
    /// Meta tags as read (`SaveReadState.tags`).
    pub tags: StringMap,
    /// Parsed rules (`SaveVersion.readRules`).
    pub rules: Option<crate::io::json::Rules>,
    /// Per-game stats from the `meta` region.
    pub stats: Option<crate::io::json::GameStats>,
    /// Map markers (`MapMarkers::read`).
    pub markers: MapMarkers,
    /// Decoded world entities (units/fires/puddles).
    pub(super) entities: LiveEntitySink,
    /// `static-fog-data` raw chunk bytes (`FogControl.write`), applied after
    /// the world rebuild (`SaveVersion.readCustomChunks`).
    pub static_fog: Option<Vec<u8>>,
    /// The generation seed when this decode ran the `World.FilterContext`
    /// filter step ([`decode_map_bytes`]); `None` for plain save loads.
    pub map_gen_seed: Option<u64>,
}

/// Decodes one save/map container into [`DecodedSave`] without touching the
/// live sim (`SaveIO.load`'s world half + `World.endMapLoad`).
///
/// Used by both the Load-Game path and the sector-resume path so a resumed
/// sector restores exactly what a loaded save does (`readMeta`, `readMap`,
/// `readEntities`, `readMarkers`). Game loads never re-run generation filters
/// (`World.loadMap` vs plain `SaveIO.load`).
pub fn decode_save_bytes(
    bytes: &[u8],
    registry: &mut ContentRegistry,
) -> Result<DecodedSave, IoError> {
    decode_bytes_inner(bytes, registry, None)
}

/// Decodes a map container the way `World.loadMap(Map)` does: the same regions
/// as [`decode_save_bytes`], then the `FilterContext` step — every entry of the
/// map's resolved `genfilters` stack (`Map.filters()`) is randomized and
/// applied to the tiles before `endMapLoad` (`World.java:341-357,661-695`).
///
/// `seed` is the recorded generation seed; it feeds the `MapGen` stream so the
/// filtered world is reproducible per seed. Save loads must keep using
/// [`decode_save_bytes`] (`SaveIO.load(file)` uses the plain world context).
pub fn decode_map_bytes(
    bytes: &[u8],
    registry: &mut ContentRegistry,
    seed: u64,
) -> Result<DecodedSave, IoError> {
    decode_bytes_inner(bytes, registry, Some(seed))
}

/// Shared decode body; `filter_seed` selects the map-load `FilterContext` step.
pub(super) fn decode_bytes_inner(
    bytes: &[u8],
    registry: &mut ContentRegistry,
    filter_seed: Option<u64>,
) -> Result<DecodedSave, IoError> {
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let mut entities = LiveEntitySink::default();
    let mut markers = MapMarkers::new();
    // `static-fog-data` (`FogControl.java:97`): registered so the reader's
    // custom-chunk pass collects it instead of skipping it.
    let static_fog_chunk = std::sync::Arc::new(crate::game::fog::StaticFogChunk(
        std::sync::Mutex::new(None),
    ));
    let mut custom_chunks: indexmap::IndexMap<
        String,
        std::sync::Arc<dyn crate::io::save::state::CustomChunk>,
    > = indexmap::IndexMap::new();
    custom_chunks.insert(
        "static-fog-data".to_owned(),
        static_fog_chunk.clone() as std::sync::Arc<dyn crate::io::save::state::CustomChunk>,
    );
    let (result, tags, rules, stats, plans, pending) = {
        let mut context = crate::editor::context::EditorContext::new(&mut grid, registry);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(registry),
            entities: Some(&mut entities),
            markers: Some(&mut markers),
            custom_chunks: Some(&custom_chunks),
            ..SaveReadState::default()
        };
        let result = SaveIo::load_bytes(bytes, &mut state);
        (
            result,
            state.tags.clone(),
            state.rules.take(),
            state.stats.take(),
            std::mem::take(&mut state.team_plans),
            context.take_pending_buildings(),
        )
    };
    result?;
    // `World.FilterContext.end()`: `applyFilters()` runs before the normal
    // end-map-load step (`World.java:666-676`) — the filters see the fully
    // loaded tile grid and their ore/boulders land before `endMapLoad`'s
    // darkness pass. The stack is `Map.filters()` over the decoded meta tags
    // (`Map.java:120-125`).
    let map_gen_seed = filter_seed.inspect(|&seed| {
        let map = crate::maps::Map::new(
            std::path::PathBuf::new(),
            grid.width(),
            grid.height(),
            tags.clone(),
            false,
            0,
            -1,
        );
        let mut filters = map.filters(registry);
        if !filters.is_empty() {
            crate::world::context::apply_filters(&mut grid.tiles, &mut filters, registry, seed);
        }
    });
    grid.end_map_load(registry);
    let static_fog = static_fog_chunk
        .0
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    Ok(DecodedSave {
        grid,
        pending,
        plans,
        tags,
        rules,
        stats,
        markers,
        entities,
        static_fog,
        map_gen_seed,
    })
}

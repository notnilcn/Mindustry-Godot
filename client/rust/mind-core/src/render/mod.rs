// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Render-side, Godot-free data and scan logic (plan 16).
//!
//! `mind-core::render` owns everything the world renderer needs that does not
//! touch Godot: the [`Layer`] constant table, [`CacheLayerId`] ordering, the
//! append-only [`BandPlan`] band allocator, the stable [`RenderQueue`], region
//! id interning, floor/block chunk bookkeeping and the visible-set scan. It
//! never feeds simulation state (HLP §2.4 / D8); the Godot-facing half lives in
//! `mind-gdext::render`.

pub mod bands;
pub mod block_cache;
pub mod bloom;
pub mod commands;
pub mod cutscene;
pub mod draw;
pub mod draw_meta;
pub mod drawf;
pub mod env;
pub mod floor_cache;
pub mod fog;
pub mod g3d;
pub mod hooks;
pub mod ids;
pub mod layer;
pub mod light;
pub mod list;
pub mod lod;
pub mod math;
pub mod menu;
pub mod minimap;
pub mod overlay;
pub mod pixelate;
pub mod queue;
pub mod rules;
pub mod scan;
pub mod shaders;
pub mod shadow;
pub mod snapshot;
pub mod trail;

pub use bands::{BandEntry, BandKey, BandPlan};
pub use block_cache::BuildingCacheGrid;
pub use bloom::{capture_z as bloom_capture_z, render_z as bloom_render_z};
pub use commands::{Blend, CommandBuffer, DrawCmd, FillKind, LineKind, ShapeKind};
pub use cutscene::{Cutscene, LaunchAnimator};
pub use draw::{
    Blending, DrawPrim, DrawProgram, GPUPARTICLES_THRESHOLD, MAX_DRAW_CALLS_TARGET,
    MULTIMESH_THRESHOLD, PrimKind, RegionKey, ShaderKey, TextureKey,
};
pub use draw_meta::BlockDrawMeta;
pub use drawf::{lerp as lerp_color, pal, to_bits, with_alpha};
pub use env::{
    ANY as ENV_ANY, EnvRegistry, EnvRendererSpec, NONE as ENV_NONE, matches as env_matches,
};
pub use floor_cache::{CHUNK_SIZE, CHUNK_UNITS, FloorChunkGrid};
pub use g3d::{MeshData, PlanetGrid, PlanetParams, build_planet_grid, hex_is_indexed};
pub use hooks::RenderInvalidation;
pub use ids::{RegionId, RegionIdTable};
pub use layer::{BuildingCacheLayer, CacheLayerId, Layer};
pub use list::{
    FORMAT, RenderEntry, RenderList, block_cache_layer, build_entries, floor_cache_layer,
    is_accessible, sort_entries,
};
pub use lod::Lod;
pub use menu::{MenuTile, MenuWorld, generate as generate_menu};
pub use overlay::{CoreEdge, build_core_edges, displayed as core_edge_displayed};
pub use queue::{QueueEntry, RenderQueue};
pub use rules::RulesRenderView;
pub use scan::{
    CameraView, ChunkSet, ProcessBlocksOut, ScanError, VisibleBlock, floor_layers_in_view,
    process_blocks, visible_blocks, visible_cached_blocks,
};
pub use shaders::{ExpectedShader, ShaderStage, Uniforms};
pub use shadow::{
    BLEND_SHADOW_COLOR, darkness_value, in_limited_rect, shadow_tile_color, wall_data_update,
};
pub use snapshot::{ScreenshotPlan, plan as plan_screenshot};
pub use trail::Trail;

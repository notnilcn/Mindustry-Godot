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
pub mod commands;
pub mod draw_meta;
pub mod floor_cache;
pub mod fog;
pub mod hooks;
pub mod ids;
pub mod layer;
pub mod light;
pub mod list;
pub mod lod;
pub mod minimap;
pub mod pixelate;
pub mod queue;
pub mod scan;
pub mod shaders;
pub mod shadow;

pub use bands::{BandEntry, BandKey, BandPlan};
pub use block_cache::BuildingCacheGrid;
pub use commands::{Blend, CommandBuffer, DrawCmd, FillKind, LineKind, ShapeKind};
pub use draw_meta::BlockDrawMeta;
pub use floor_cache::{CHUNK_SIZE, CHUNK_UNITS, FloorChunkGrid};
pub use hooks::RenderInvalidation;
pub use ids::{RegionId, RegionIdTable};
pub use layer::{BuildingCacheLayer, CacheLayerId, Layer};
pub use list::{
    FORMAT, RenderEntry, RenderList, block_cache_layer, build_entries, floor_cache_layer,
    is_accessible, sort_entries,
};
pub use lod::Lod;
pub use queue::{QueueEntry, RenderQueue};
pub use scan::{
    CameraView, ChunkSet, ProcessBlocksOut, ScanError, VisibleBlock, floor_layers_in_view,
    process_blocks, visible_blocks, visible_cached_blocks,
};
pub use shaders::{ExpectedShader, ShaderStage, Uniforms};
pub use shadow::{
    BLEND_SHADOW_COLOR, darkness_value, in_limited_rect, shadow_tile_color, wall_data_update,
};

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Block draw metadata seam (plan 16 §3.6 / M4).
//!
//! Upstream these predicates live on `Block` (`drawCached`, `drawDynamic`,
//! `hasShadow`, `fillsTile`, `obstructsLight`, `emitLight`, `drawTeamOverlay`,
//! `cacheLayer`). Plan 02's `BlockDef` does not carry them yet, so this module
//! centralizes the **derivation from `BlockKind`** that M3/M4 consume and that
//! plan 02 replaces with the real fields once they land. The defaults are
//! chosen to preserve the M2 behavior: everything non-static draws dynamically,
//! static walls fill/shadow/obstruct.

use crate::content::BlockDef;
use crate::render::layer::CacheLayerId;

/// The draw-relevant `Block` flags (plan 16 §3.13, plan 02 §3.6).
///
/// These are read straight from the plan-02 [`BlockDef`] fields (reconciled
/// 2026-10-02): the earlier `is_static_kind` derivation is gone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockDrawMeta {
    /// `Block.cacheLayer`.
    pub cache_layer: CacheLayerId,
    /// `Block.drawCached` (eligible for the building cache).
    pub draw_cached: bool,
    /// `Block.drawDynamic` (must redraw every frame).
    pub draw_dynamic: bool,
    /// `Block.hasShadow` / `Block.displayShadow(tile)`.
    pub display_shadow: bool,
    /// `Block.fillsTile` (overloaded `tile.data` darkness owner).
    pub fills_tile: bool,
    /// `Block.obstructsLight` (excluded from light quadtrees).
    pub obstructs_light: bool,
    /// `Block.emitLight`.
    pub emit_light: bool,
    /// `Block.drawTeamOverlay`.
    pub draw_team_overlay: bool,
    /// `Block.buildingCacheLayer`.
    pub building_cache_layer: f32,
}

impl BlockDrawMeta {
    /// Reads the metadata from a [`BlockDef`] (plan-02 fields).
    pub fn from_def(def: &BlockDef) -> Self {
        Self {
            cache_layer: def.cache_layer,
            draw_cached: def.draw_cached,
            draw_dynamic: def.draw_dynamic,
            display_shadow: def.display_shadow,
            fills_tile: def.fills_tile,
            obstructs_light: def.obstructs_light,
            emit_light: def.emit_light,
            draw_team_overlay: def.draw_team_overlay,
            building_cache_layer: def.building_cache_layer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::render::layer::BuildingCacheLayer;

    fn registry() -> crate::content::ContentRegistry {
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap();
        registry.init().unwrap();
        registry.post_init().unwrap();
        registry.load().unwrap();
        registry
    }

    #[test]
    fn wall_is_cached_wall_layer_and_shadowcasting() {
        let content = registry();
        let wall = content.block_id("stone-wall").expect("stone-wall");
        let def = content.block(wall).expect("def");
        let meta = BlockDrawMeta::from_def(def);
        assert!(meta.display_shadow);
        assert!(meta.fills_tile);
        assert!(meta.obstructs_light);
        assert_eq!(meta.cache_layer, CacheLayerId::Walls);
    }

    #[test]
    fn router_is_cached_and_hidden_from_dynamic_pass() {
        let content = registry();
        let router = content.block_id("router").expect("router");
        let def = content.block(router).expect("def");
        let meta = BlockDrawMeta::from_def(def);
        assert!(!meta.draw_dynamic);
        assert!(meta.draw_cached);
        assert_eq!(meta.cache_layer, CacheLayerId::Normal);
        assert_eq!(meta.building_cache_layer, BuildingCacheLayer::NORMAL);
    }

    #[test]
    fn container_is_cached() {
        let content = registry();
        let container = content.block_id("container").expect("container");
        let def = content.block(container).expect("def");
        let meta = BlockDrawMeta::from_def(def);
        assert!(meta.draw_cached);
        assert!(!meta.draw_dynamic);
    }

    #[test]
    fn emitter_is_flagged() {
        let content = registry();
        let light = content.block_id("illuminator").expect("illuminator");
        let def = content.block(light).expect("def");
        assert!(BlockDrawMeta::from_def(def).emit_light);
    }

    #[test]
    fn duct_uses_under_cache_layer() {
        let content = registry();
        let duct = content.block_id("duct").expect("duct");
        let def = content.block(duct).expect("def");
        let meta = BlockDrawMeta::from_def(def);
        assert!(meta.draw_cached);
        assert_eq!(meta.building_cache_layer, BuildingCacheLayer::UNDER);
    }
}

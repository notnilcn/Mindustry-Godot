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
use crate::render::layer::{BuildingCacheLayer, CacheLayerId};
use crate::render::list::block_cache_layer;
use crate::world::tile::is_static_kind;

/// The draw-relevant `Block` flags (plan 16 §3.13, plan 02 §3.6).
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
    /// Derives the metadata from a `BlockDef`.
    ///
    /// **Plan 02 gap:** `draw_cached`/`emit_light`/`draw_team_overlay` default
    /// to `false` and `draw_dynamic` to `!is_static`, matching the M2 dynamic
    /// pass; flip these when `BlockDef` carries the upstream fields.
    pub fn from_def(def: &BlockDef) -> Self {
        let static_kind = is_static_kind(def.kind);
        Self {
            cache_layer: block_cache_layer(def),
            draw_cached: false,
            draw_dynamic: !static_kind,
            display_shadow: static_kind,
            fills_tile: static_kind,
            obstructs_light: static_kind,
            emit_light: false,
            draw_team_overlay: false,
            building_cache_layer: BuildingCacheLayer::NORMAL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

    fn registry() -> crate::content::ContentRegistry {
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap();
        registry.init().unwrap();
        registry.post_init().unwrap();
        registry.load().unwrap();
        registry
    }

    #[test]
    fn wall_is_static_and_shadowcasting() {
        let content = registry();
        let wall = content.block_id("stone-wall").expect("stone-wall");
        let def = content.block(wall).expect("def");
        let meta = BlockDrawMeta::from_def(def);
        assert!(meta.display_shadow);
        assert!(meta.fills_tile);
        assert!(meta.obstructs_light);
        assert!(!meta.draw_dynamic);
        assert_eq!(meta.cache_layer, CacheLayerId::Walls);
    }

    #[test]
    fn router_is_dynamic_by_default() {
        let content = registry();
        let router = content.block_id("router").expect("router");
        let def = content.block(router).expect("def");
        let meta = BlockDrawMeta::from_def(def);
        assert!(meta.draw_dynamic);
        assert!(!meta.display_shadow);
        assert_eq!(meta.cache_layer, CacheLayerId::Normal);
    }
}

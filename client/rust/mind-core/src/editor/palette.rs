// SPDX-License-Identifier: GPL-3.0-only
//! Editor block palette order (`MapEditorDialog.rebuildBlockSelection`,
//! `editor/MapEditorDialog.java:841-870`).
//!
//! Java sorts every block, then skips the ones the palette never shows:
//! - sort: cores first, then `!synthetic()` before `synthetic()`, then
//!   non-`OverlayFloor` before `OverlayFloor`, then `id`
//!   (`MapEditorDialog.java:846-855`);
//! - skip: `!block.inEditor`, `BuildVisibility.debugOnly`
//!   (`MapEditorDialog.java:863-864`).
//!
//! The remaining two filters are UI-side and live in the GDScript shell
//! (`map_editor_dialog.gd`): `!Core.atlas.isFound(block.uiIcon)` and the
//! `localizedName` search.

use crate::content::registries::blocks::is_overlay_floor_kind;
use crate::content::{BlockDef, BlockKind, BuildVisibility};
use crate::maps::filters::block_info;

/// `block instanceof OverlayFloor`: ores, overlay floors, `SpawnBlock`,
/// `RemoveOre`, `CharacterOverlay` and `RuneOverlay` (`SpawnBlock.java:6`,
/// `RemoveOre.java:11`).
pub fn is_overlay_floor(def: &BlockDef) -> bool {
    is_overlay_floor_kind(def.kind)
}

/// Whether the palette lists `def` at all (`MapEditorDialog.java:863-864`).
pub fn in_palette(def: &BlockDef) -> bool {
    def.in_editor && def.build_visibility != BuildVisibility::DebugOnly
}

/// The palette blocks in Java's order, before the UI-side icon and search
/// filters.
pub fn palette_order(blocks: &[BlockDef]) -> Vec<&BlockDef> {
    let mut rows: Vec<&BlockDef> = blocks.iter().filter(|def| in_palette(def)).collect();
    rows.sort_by(|a, b| {
        let a_core = a.kind == BlockKind::CoreBlock;
        let b_core = b.kind == BlockKind::CoreBlock;
        // `-Boolean.compare(b1 instanceof CoreBlock, b2 instanceof CoreBlock)`.
        b_core
            .cmp(&a_core)
            .then_with(|| block_info::synthetic(a).cmp(&block_info::synthetic(b)))
            .then_with(|| is_overlay_floor(a).cmp(&is_overlay_floor(b)))
            .then_with(|| a.id.raw().cmp(&b.id.raw()))
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    /// The first 40 palette blocks, derived from Java source with the exact
    /// comparator (`MapEditorDialog.java:846-855`) over the `Blocks.java`
    /// declaration order (`Blocks.java:191-...`: content ids follow it) and
    /// the `inEditor`/`debugOnly` filters. `air` is listed here and dropped by
    /// the UI-side icon filter (Java has no `air` sprite, so
    /// `atlas.isFound(air.uiIcon)` is false). The round-L Java frame
    /// (`runs/20261017-L-twin/java/31-editor-side.png`) shows the same rows:
    /// six cores, then `deep-water`.., then `stone` starting row 4.
    const JAVA_FIRST_40: [&str; 40] = [
        "core-shard",
        "core-foundation",
        "core-nucleus",
        "core-bastion",
        "core-citadel",
        "core-acropolis",
        "air",
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
        "space",
        "empty",
        "stone",
        "crater-stone",
        "char",
        "basalt",
        "hotrock",
        "magmarock",
        "sand-floor",
        "darksand",
        "dirt",
        "mud",
        "dacite",
        "rhyolite",
        "rhyolite-crater",
        "rough-rhyolite",
        "regolith",
        "yellow-stone",
        "carbon-stone",
        "ferric-stone",
        "ferric-craters",
        "beryllic-stone",
        "crystalline-stone",
    ];

    fn names(content: &crate::content::ContentRegistry) -> Vec<&str> {
        palette_order(content.blocks())
            .into_iter()
            .map(|d| d.name.as_str())
            .collect()
    }

    #[test]
    fn palette_first_40_match_java() {
        let content = test_registry();
        assert_eq!(names(&content)[..40], JAVA_FIRST_40);
    }

    /// `spawn` is an `OverlayFloor` (`SpawnBlock.java:6`): it sorts with the
    /// ores after every natural non-overlay block, never in row 2.
    #[test]
    fn spawn_sorts_with_the_ores_and_buildings_after() {
        let content = test_registry();
        let order = palette_order(content.blocks());
        let at = |name: &str| order.iter().position(|d| d.name == name).unwrap();
        let spawn = at("spawn");
        assert!(order[..spawn].iter().all(|d| d.kind == BlockKind::CoreBlock
            || (!block_info::synthetic(d) && !is_overlay_floor(d))));
        assert!(spawn < at("ore-copper") && at("ore-copper") < at("graphite-press"));
        // `update = true` blocks are synthetic (TargetDummy.java:38,
        // UnitAssembler.java:62, UnitCargoUnloadPoint.java:26).
        for name in ["target-dummy", "tank-assembler", "unit-cargo-unload-point"] {
            assert!(at(name) > at("ore-copper"), "{name}");
        }
        assert!(
            !order
                .iter()
                .any(|d| d.name == "cliff" || d.name == "remove-ore")
        );
    }
}

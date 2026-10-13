// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Block place/break/destroy sound resolution.
//!
//! `ConstructBlock.constructFinish`/`deconstructFinish` play
//! `block.placeSound`/`block.breakSound` (`ConstructBlock.java:58-120`);
//! `BuildingComp.playDestroySound` plays `block.destroySound`
//! (`BuildingComp.java:1524-1543`), with the `Wall`/`CoreBlock`/`Prop`
//! overrides transcribed below.

use super::ids::SoundId;
use crate::content::registries::blocks::{BlockDef, BlockKind};

/// `Block.init` default place sound by size (`Block.java:1367-1372`).
pub fn place_sound(def: &BlockDef) -> SoundId {
    super::sim::block_place_sound(def.size)
}

/// `Block.init` default break sound by size, plus the `Prop` family overrides
/// (`Prop.java:21`; `StaticProp`/`StaticWall`/`StaticTree` all extend `Prop`)
/// and the `spore-cluster` override (`Blocks.java:699`).
pub fn break_sound(def: &BlockDef) -> SoundId {
    match def.kind {
        BlockKind::Prop if def.name == "spore-cluster" => SoundId::PLANT_BREAK,
        BlockKind::Prop | BlockKind::StaticProp | BlockKind::StaticWall | BlockKind::StaticTree => {
            SoundId::ROCK_BREAK
        }
        _ => super::sim::block_break_sound(def.size),
    }
}

/// `(candidates, volume)` for `Block.destroySound` (`Block.java:1360-1364`),
/// with the `Wall` size-2 and `CoreBlock` overrides (`Wall.java:74`,
/// `CoreBlock.java:90-91`).
pub fn destroy_sound(def: &BlockDef) -> (&'static [SoundId], f32) {
    match def.kind {
        BlockKind::CoreBlock => (&[SoundId::EXPLOSION_CORE], 1.6),
        BlockKind::Wall if def.size == 2 => (&[SoundId::BLOCK_EXPLODE_WALL], 1.0),
        _ => (super::sim::block_destroy_alternates(def.size), 1.0),
    }
}

/// `Block.offset` world centre for a tile (`tile.drawx()/drawy()`).
pub fn world_center(def: &BlockDef, x: i16, y: i16) -> (f32, f32) {
    (
        f32::from(x) * 8.0 + def.offset,
        f32::from(y) * 8.0 + def.offset,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> crate::content::ContentRegistry {
        crate::world::harness::BuildHarness::load_content()
    }

    fn def(content: &crate::content::ContentRegistry, name: &str) -> BlockDef {
        content
            .block_id(name)
            .and_then(|id| content.block(id))
            .cloned()
            .unwrap_or_else(|| panic!("block `{name}`"))
    }

    #[test]
    fn destroy_sound_overrides() {
        let content = content();
        assert_eq!(
            destroy_sound(&def(&content, "core-shard")),
            (&[SoundId::EXPLOSION_CORE][..], 1.6)
        );
        assert_eq!(
            destroy_sound(&def(&content, "copper-wall-large")),
            (&[SoundId::BLOCK_EXPLODE_WALL][..], 1.0)
        );
        assert_eq!(
            destroy_sound(&def(&content, "copper-wall")),
            (
                &[SoundId::BLOCK_EXPLODE1, SoundId::BLOCK_EXPLODE1_ALT][..],
                1.0
            )
        );
    }

    #[test]
    fn prop_break_sounds() {
        let content = content();
        // `Blocks.java:699`: spore-cluster -> plantBreak.
        assert_eq!(
            break_sound(&def(&content, "spore-cluster")),
            SoundId::PLANT_BREAK
        );
        // `Prop.java:21`: every prop -> rockBreak. The port maps Java's
        // `StaticProp`/`StaticWall`/`StaticTree` (all `Prop` subclasses) to
        // their own BlockKind, so boulder is a `StaticProp`.
        assert_eq!(break_sound(&def(&content, "boulder")), SoundId::ROCK_BREAK);
        // Walls keep the size default.
        assert_eq!(
            break_sound(&def(&content, "copper-wall")),
            SoundId::BLOCK_BREAK1
        );
        assert_eq!(
            break_sound(&def(&content, "copper-wall-large")),
            SoundId::BLOCK_BREAK2
        );
    }

    #[test]
    fn world_center_uses_block_offset() {
        let content = content();
        let wall = def(&content, "copper-wall");
        assert_eq!(world_center(&wall, 5, 7), (40.0, 56.0));
        let large = def(&content, "copper-wall-large");
        assert_eq!(
            world_center(&large, 8, 8),
            (8.0 * 8.0 + large.offset, 64.0 + large.offset)
        );
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Placed-block sprite region chain (plan 16; parity plan §3.1 PAR-0152).
//!
//! Java resolves a placed block's world sprite from several sources:
//!
//! * `Block.load()`: `region = Core.atlas.find(name)` (`Block.java:1527`);
//! * autotiled drawers use the generated slices
//!   (`<name>-<blendbits>-<frame>`, `Conveyor.regions`, `Conveyor.java:144`);
//! * the packer emits the composed `block-<name>-full` icon (`Generators` +
//!   `Block.createIcons`): one unstripped region holding the base plus every
//!   `DrawPart` at rest, e.g. a turret's base + barrel.
//!
//! The port draws one quad per placed building (per-part animation lands with
//! the unit/FX work), so it prefers the composed `-full` region, then the
//! block's own `region`, then the first conveyor frame. The chain is pure
//! string logic so it is unit-tested here; the atlas lookup stays in
//! `mind-gdext::render`.

/// Candidate atlas region names for a placed block's world sprite, in draw
/// order (the first region present in the atlas wins).
///
/// `name` is the content name (`Block.name`) and `region` the block's loaded
/// region name (`Block.region`, normally equal to `name`).
pub fn block_region_candidates(name: &str, region: &str) -> [String; 3] {
    [
        format!("block-{name}-full"),
        region.to_owned(),
        format!("{name}-0-0"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duo_prefers_the_composed_full_icon() {
        assert_eq!(
            block_region_candidates("duo", "duo"),
            ["block-duo-full", "duo", "duo-0-0"]
        );
    }

    #[test]
    fn conveyor_chain_covers_the_autotile_frame() {
        let candidates = block_region_candidates("conveyor", "conveyor");
        assert_eq!(candidates[0], "block-conveyor-full");
        assert!(
            candidates.contains(&"conveyor-0-0".to_owned()),
            "belt frames are `<name>-<blendbits>-<frame>`; the chain must reach `conveyor-0-0`: {candidates:?}"
        );
    }

    #[test]
    fn custom_region_name_is_kept() {
        assert_eq!(
            block_region_candidates("snow-boulder", "boulder-region"),
            [
                "block-snow-boulder-full",
                "boulder-region",
                "snow-boulder-0-0"
            ]
        );
    }
}

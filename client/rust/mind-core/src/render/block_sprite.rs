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

use crate::content::{BlockKind, ItemId};

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

/// `BuildingComp.drawCracks` (`BuildingComp.java:1236-1244`):
/// `index = clamp((int)((1 - healthf) * 8), 0, 7)` for the `cracks-<size>-<i>`
/// region.
pub fn crack_index(healthf: f32) -> usize {
    (((1.0 - healthf) * 8.0) as i32).clamp(0, 7) as usize
}

/// `drawCached` state a static building-cache bake depends on (H03 T18).
///
/// Java keeps these on the live building; the port reads them through
/// `MindSimHost::block_state_draw` and re-bakes a chunk when the folded state
/// hash changes (the equivalent of `BuildingComp.recache()`,
/// `BuildingComp.java:1290-1292`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CachedBlockState {
    /// `DoorBuild.open` (`Door.java:133`).
    pub door_open: bool,
    /// `Building.enabled` (the switch state, `SwitchBlock.java:67-69`).
    pub enabled: bool,
    /// Item drawn by a sorter / item source / unloader (`None` = unset).
    pub item: Option<ItemId>,
    /// `damaged()` (`BuildingComp.java:1234`) — hash only.
    pub damaged: bool,
    /// `crack_index(healthf)` — hash only.
    pub crack: usize,
}

impl Default for CachedBlockState {
    /// `Building.enabled` defaults to true (`BuildingComp.java`), unlike the
    /// derive default.
    fn default() -> Self {
        Self {
            door_open: false,
            enabled: true,
            item: None,
            damaged: false,
            crack: 0,
        }
    }
}

/// One sprite a `drawCached` building bakes into the static building cache
/// (`BuildingCacheRenderer::bake_chunk`; H03 T18).
#[derive(Clone, Debug, PartialEq)]
pub struct CachedBakeSprite {
    /// Atlas region candidates in resolution order (first present wins).
    pub candidates: Vec<String>,
    /// `Some(item)`: tint the quad with the item colour
    /// (`Draw.color(sortItem.color)`, `Sorter.java:82`).
    pub item_tint: Option<ItemId>,
    /// Quad edge in world units; `None` = the region's natural size
    /// (`pixels * 0.25`, Java `Draw.rect(region, x, y)`).
    pub size: Option<f32>,
}

/// Java base sprite for the state-driven `drawCached` kinds: their `draw()`
/// overrides call `super.draw()`, i.e. `Draw.rect(region, x, y)` on the plain
/// block region — never the composed `block-<name>-full` icon (that one fills
/// the sorter's transparent centre, hiding the item square).
fn cached_base_candidates(kind: BlockKind, name: &str, region: &str) -> Vec<String> {
    match kind {
        BlockKind::Door
        | BlockKind::AutoDoor
        | BlockKind::SwitchBlock
        | BlockKind::Sorter
        | BlockKind::ItemSource
        | BlockKind::Unloader => vec![
            region.to_owned(),
            format!("{name}-0-0"),
            format!("block-{name}-full"),
        ],
        _ => block_region_candidates(name, region).to_vec(),
    }
}

/// The sprites a `drawCached` building bakes, in Java draw order (bottom
/// sprite first):
///
/// * `Door.draw` (`Door.java:132-135`): an open door replaces the base with
///   `<name>-open`.
/// * `SwitchBlock.draw` (`SwitchBlock.java:64-71`): base, then `<name>-on`
///   while enabled.
/// * `Sorter.draw` (`Sorter.java:76-88`) / `ItemSource.draw`
///   (`ItemSource.java:71-80`): the item-colour square (or the `cross-full`
///   cross when unset) first, then `super.draw()`'s base.
/// * `Unloader.draw` (`Unloader.java:263-269`): base, then the item-tinted
///   `<name>-center` (Java draws a fully transparent rect when unset; the
///   port skips it).
pub fn cached_bake_sprites(
    kind: BlockKind,
    name: &str,
    region: &str,
    size: i32,
    state: &CachedBlockState,
) -> Vec<CachedBakeSprite> {
    let unit = crate::config::TILESIZE as f32;
    let footprint = size.max(1) as f32 * unit;
    let base = |candidates: Vec<String>| CachedBakeSprite {
        candidates,
        item_tint: None,
        size: Some(footprint),
    };
    // Overlays draw at their natural region size (Java `Draw.rect(region)` /
    // `Draw.rect(centerRegion)`); the item square is a `Fill.square` of
    // `tilesize`, drawn through the 1x1 `white` region.
    let overlay =
        |candidates: Vec<String>, item_tint: Option<ItemId>, size: Option<f32>| CachedBakeSprite {
            candidates,
            item_tint,
            size,
        };
    match kind {
        BlockKind::Door | BlockKind::AutoDoor => {
            if state.door_open {
                vec![base(vec![format!("{name}-open")])]
            } else {
                vec![base(cached_base_candidates(kind, name, region))]
            }
        }
        BlockKind::SwitchBlock => {
            let mut out = vec![base(cached_base_candidates(kind, name, region))];
            if state.enabled {
                out.push(overlay(vec![format!("{name}-on")], None, None));
            }
            out
        }
        BlockKind::Sorter => {
            let first = match state.item {
                Some(item) => overlay(vec![String::from("white")], Some(item), Some(unit)),
                None => overlay(
                    vec![format!("{name}-cross"), String::from("cross-full")],
                    None,
                    None,
                ),
            };
            vec![first, base(cached_base_candidates(kind, name, region))]
        }
        BlockKind::ItemSource => {
            let first = match state.item {
                Some(item) => overlay(vec![String::from("white")], Some(item), Some(unit)),
                None => overlay(vec![String::from("cross-full")], None, None),
            };
            vec![first, base(cached_base_candidates(kind, name, region))]
        }
        BlockKind::Unloader => {
            let mut out = vec![base(cached_base_candidates(kind, name, region))];
            if let Some(item) = state.item {
                out.push(overlay(
                    vec![format!("{name}-center"), String::from("unloader-center")],
                    Some(item),
                    None,
                ));
            }
            out
        }
        _ => vec![base(cached_base_candidates(kind, name, region))],
    }
}

/// Cheap hash of the state that changes a `drawCached` bake (H03 T18): config
/// (door open, switch enabled, sorter/source/unloader item) and damage (crack
/// index, `damaged()`). `BuildingCacheRenderer` folds it per chunk per frame;
/// a changed fold re-bakes that chunk.
pub fn cached_state_hash(state: &CachedBlockState) -> u64 {
    fn mix(h: u64, value: u64) -> u64 {
        (h ^ value).wrapping_mul(0x0000_0100_0000_01b3)
    }
    let mut h = 0xcbf2_9ce4_8422_2325;
    h = mix(h, u64::from(state.door_open));
    h = mix(h, u64::from(state.enabled));
    h = mix(h, state.item.map_or(u64::MAX, |item| u64::from(item.raw())));
    h = mix(h, u64::from(state.damaged));
    mix(h, state.crack as u64)
}

/// `BuildingComp.drawCracks` rotation: `(pos % 4) * 90` with
/// `pos = Point2.pack(x, y)` (`(x << 16) | y`), i.e. the tile y's low bits.
pub fn crack_rotation(tile_y: i32) -> f32 {
    (tile_y.rem_euclid(4)) as f32 * 90.0
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

    #[test]
    fn crack_index_clamps_to_eight_steps() {
        assert_eq!(crack_index(1.0), 0);
        assert_eq!(crack_index(0.3), 5);
        assert_eq!(crack_index(0.0), 7);
    }

    #[test]
    fn crack_rotation_uses_tile_y_low_bits() {
        assert_eq!(crack_rotation(6), 180.0);
        assert_eq!(crack_rotation(4), 0.0);
        assert_eq!(crack_rotation(-1), 270.0);
    }

    #[test]
    fn door_state_region_closed_and_open() {
        let closed = cached_bake_sprites(
            BlockKind::Door,
            "door",
            "door",
            1,
            &CachedBlockState::default(),
        );
        assert_eq!(closed.len(), 1);
        // Java `super.draw()` uses the plain region, not the composed icon
        // (the icon fills the sorter's hole; H03 T18).
        assert_eq!(
            closed[0].candidates,
            ["door", "door-0-0", "block-door-full"]
        );
        assert_eq!(closed[0].size, Some(crate::config::TILESIZE as f32));
        let open = cached_bake_sprites(
            BlockKind::Door,
            "door",
            "door",
            1,
            &CachedBlockState {
                door_open: true,
                ..CachedBlockState::default()
            },
        );
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].candidates, ["door-open"]);
    }

    #[test]
    fn switch_bakes_the_on_sprite_only_when_enabled() {
        let on = cached_bake_sprites(
            BlockKind::SwitchBlock,
            "switch",
            "switch",
            1,
            &CachedBlockState {
                enabled: true,
                ..CachedBlockState::default()
            },
        );
        assert_eq!(on.len(), 2);
        assert_eq!(on[1].candidates, ["switch-on"]);
        // `switch-on` is a 12x12 sub-region: natural size, not tilesize.
        assert_eq!(on[1].size, None);
        let off = cached_bake_sprites(
            BlockKind::SwitchBlock,
            "switch",
            "switch",
            1,
            &CachedBlockState {
                enabled: false,
                ..CachedBlockState::default()
            },
        );
        assert_eq!(off.len(), 1);
    }

    #[test]
    fn sorter_bake_cmds() {
        // Unconfigured sorter: the cross under the base (`Sorter.java:78-79`).
        let none = cached_bake_sprites(
            BlockKind::Sorter,
            "sorter",
            "sorter",
            1,
            &CachedBlockState::default(),
        );
        assert_eq!(none.len(), 2);
        assert_eq!(none[0].candidates, ["sorter-cross", "cross-full"]);
        assert_eq!(none[0].item_tint, None);
        assert_eq!(none[0].size, None);
        // The crossed sorter keeps its transparent centre (plain `sorter`
        // region, not the opaque composed icon).
        assert_eq!(none[1].candidates[0], "sorter");
        // Configured: the white item square tinted with the item colour
        // (`Draw.color(sortItem.color); Fill.square(..)`, `Sorter.java:81-84`).
        let copper = ItemId::COPPER;
        let configured = cached_bake_sprites(
            BlockKind::Sorter,
            "sorter",
            "sorter",
            1,
            &CachedBlockState {
                item: Some(copper),
                ..CachedBlockState::default()
            },
        );
        assert_eq!(configured[0].candidates, ["white"]);
        assert_eq!(configured[0].item_tint, Some(copper));
        assert_eq!(configured[0].size, Some(crate::config::TILESIZE as f32));
    }

    #[test]
    fn unloader_bakes_center_only_with_an_item() {
        let empty = cached_bake_sprites(
            BlockKind::Unloader,
            "duct-unloader",
            "duct-unloader",
            1,
            &CachedBlockState::default(),
        );
        assert_eq!(empty.len(), 1);
        let filled = cached_bake_sprites(
            BlockKind::Unloader,
            "duct-unloader",
            "duct-unloader",
            1,
            &CachedBlockState {
                item: Some(ItemId::COPPER),
                ..CachedBlockState::default()
            },
        );
        assert_eq!(filled.len(), 2);
        assert_eq!(
            filled[1].candidates,
            ["duct-unloader-center", "unloader-center"]
        );
        assert_eq!(filled[1].item_tint, Some(ItemId::COPPER));
        // `unloader-center` is a sub-region: natural size.
        assert_eq!(filled[1].size, None);
    }

    #[test]
    fn cached_state_hash_changes_on_config_and_damage() {
        let base = CachedBlockState::default();
        assert_ne!(
            cached_state_hash(&base),
            cached_state_hash(&CachedBlockState {
                door_open: true,
                ..base
            })
        );
        assert_ne!(
            cached_state_hash(&base),
            cached_state_hash(&CachedBlockState {
                enabled: false,
                ..base
            })
        );
        assert_ne!(
            cached_state_hash(&base),
            cached_state_hash(&CachedBlockState {
                item: Some(ItemId::COPPER),
                ..base
            })
        );
        assert_ne!(
            cached_state_hash(&base),
            cached_state_hash(&CachedBlockState {
                damaged: true,
                crack: 5,
                ..base
            })
        );
        // Stable for the same state.
        assert_eq!(cached_state_hash(&base), cached_state_hash(&base));
    }
}

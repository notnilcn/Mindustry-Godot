// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Environment-prop draw math (`world/blocks/environment/`): the `SeaBush`,
//! `TreeBlock`, `TallBlock`, `Cliff`, `RuneOverlay`, `CharacterOverlay` and
//! `SteamVent` draw bodies, plus their deterministic per-tile seeds. The
//! `WobbleProp` warp lives in [`super::tiled::Wobble`].
//!
//! View-only: these functions take the tile seed and `Time.time` and return
//! quads/regions/colours; the Godot passes (`mind-gdext::render::{blocks,floor}`)
//! resolve the atlas regions and bake the meshes. Java cites are per body.

use crate::content::{BlockId, Rgba};
use crate::math::rand_arc::{ArcRand, pack_point2, random_seed_int};
use crate::render::drawf::{sin_scl, trns};

/// `Mathf.sin`-style `sin(in / scl) * mag` for the `cos` terms.
fn cos_scl(x: f32, scl: f32, mag: f32) -> f32 {
    (x / scl).cos() * mag
}

/// `Angles.angleDist(a, b)`: the unsigned smallest angle between two angles
/// (`Angles.java:angleDist`), in degrees `[0, 180]`.
pub fn angle_dist(a: f32, b: f32) -> f32 {
    let d = (b - a) % 360.0;
    let d = if d < -180.0 {
        d + 360.0
    } else if d > 180.0 {
        d - 360.0
    } else {
        d
    };
    d.abs()
}

/// `SeaBush` field defaults (`SeaBush.java:10-15`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeaBushParams {
    /// `lobesMin` (`:10`, 7).
    pub lobes_min: i32,
    /// `lobesMax` (`:10`, 7).
    pub lobes_max: i32,
    /// `botAngle` (`:11`, 60).
    pub bot_angle: f32,
    /// `origin` (`:11`, 0.1).
    pub origin: f32,
    /// `sclMin` (`:12`, 30).
    pub scl_min: f32,
    /// `sclMax` (`:12`, 50).
    pub scl_max: f32,
    /// `magMin` (`:12`, 5).
    pub mag_min: f32,
    /// `magMax` (`:12`, 15).
    pub mag_max: f32,
    /// `timeRange` (`:12`, 40).
    pub time_range: f32,
    /// `spread` (`:12`, 0).
    pub spread: f32,
}

impl Default for SeaBushParams {
    fn default() -> Self {
        // `SeaBush.java:10-15` field initializers.
        Self {
            lobes_min: 7,
            lobes_max: 7,
            bot_angle: 60.0,
            origin: 0.1,
            scl_min: 30.0,
            scl_max: 50.0,
            mag_min: 5.0,
            mag_max: 15.0,
            time_range: 40.0,
            spread: 0.0,
        }
    }
}

/// One `SeaBush` lobe (`SeaBush.drawBase`, `SeaBush.java:26-45`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeaBushLobe {
    /// Final draw angle in degrees (`ba + Mathf.sin(...)`).
    pub rotation: f32,
    /// Quad centre (world px).
    pub cx: f32,
    /// Quad centre (world px).
    pub cy: f32,
    /// Quad width (`region.width * region.scl()`; scale is 1 in world draws).
    pub w: f32,
    /// Quad height.
    pub h: f32,
    /// `Angles.angleDist(ba, 225f) <= botAngle`: use the `-bot` region.
    pub bot: bool,
}

/// `SeaBush.drawBase` lobes (`SeaBush.java:25-46`).
///
/// The per-tile `Rand` is seeded with `tile.pos()`; the draw then consumes
/// `random(180)`, `random(lobesMin, lobesMax)` and, per lobe, `range(spread)`,
/// `random(0, timeRange)`, `random(sclMin, sclMax)` and `random(magMin,
/// magMax)` in Java argument order. `region_w`/`region_h` are the main
/// region's natural pixel size (the bot region has the same size).
pub fn sea_bush_lobes(
    tile_x: i32,
    tile_y: i32,
    time: f32,
    region_w: f32,
    region_h: f32,
    params: &SeaBushParams,
) -> Vec<SeaBushLobe> {
    let mut rand = ArcRand::new(pack_point2(tile_x, tile_y));
    let offset = rand.random_float(180.0);
    let lobes = rand
        .random_range_int(params.lobes_min, params.lobes_max)
        .max(0);
    let world_x = tile_x as f32 * crate::config::TILESIZE as f32;
    let world_y = tile_y as f32 * crate::config::TILESIZE as f32;
    let mut out = Vec::with_capacity(lobes as usize);
    for i in 0..lobes {
        let ba = i as f32 / lobes as f32 * 360.0 + offset + rand.range_float(params.spread);
        // `Mathf.sin(Time.time + rand.random(0, timeRange), rand.random(sclMin,
        // sclMax), rand.random(magMin, magMax))` — argument order is the
        // `rand.random` draw order.
        let phase = time + rand.random_range_float(0.0, params.time_range);
        let scl = rand.random_range_float(params.scl_min, params.scl_max);
        let mag = rand.random_range_float(params.mag_min, params.mag_max);
        let rotation = ba + sin_scl(phase, scl, mag);
        let (ox, oy) = trns(rotation, params.origin);
        // `Draw.rect(region, x - trnsx + w*0.5, y - trnsy, w, h, origin*4,
        // h/2, angle)`: the pivot is `(origin*4, h/2)` **pixels**, so the
        // drawn centre shifts by the rotated `(w/2 - origin*4, 0)`.
        let (dx, dy) = trns(rotation, region_w / 2.0 - params.origin * 4.0);
        out.push(SeaBushLobe {
            rotation,
            cx: world_x - ox + region_w * 0.5 + dx,
            cy: world_y - oy + dy,
            w: region_w,
            h: region_h,
            bot: angle_dist(ba, 225.0) <= params.bot_angle,
        });
    }
    out
}

/// `TreeBlock.shadowOffset` (`TreeBlock.java:12`, -4).
pub const TREE_SHADOW_OFFSET: f32 = -4.0;

/// `TreeBlock.drawBase` rotation (`TreeBlock.java:24-27`):
/// `randomSeed(pos, 0, 4) * 90 + sin(Time.time + x, 50, 0.5) +
/// sin(Time.time - y, 65, 0.9) + sin(Time.time + y - x, 85, 0.9)`.
pub fn tree_rotation(tile_x: i32, tile_y: i32, time: f32) -> f32 {
    let x = tile_x as f32 * crate::config::TILESIZE as f32;
    let y = tile_y as f32 * crate::config::TILESIZE as f32;
    random_seed_int(pack_point2(tile_x, tile_y), 0, 4) as f32 * 90.0
        + sin_scl(time + x, 50.0, 0.5)
        + sin_scl(time - y, 65.0, 0.9)
        + sin_scl(time + y - x, 85.0, 0.9)
}

/// `TreeBlock.drawBase` warped quad (`TreeBlock.java:32-38`): the `w`×`h`
/// sprite rotated by `rotation` and then displaced per vertex by
/// `(sin(y*3 + t, 30, 0.2) + sin(x*3 - t, 70, 0.8), cos(x*3 + t + 8, 36,
/// 0.22) + sin(y*3 - t, 50, 0.2))` — `scl = 30f, mag = 0.2f` (`:29`), with
/// **world-space** vertex coordinates like `Draw.rectv`'s callback.
pub fn tree_quad(tile_x: i32, tile_y: i32, time: f32, w: f32, h: f32) -> [[f32; 2]; 4] {
    let x = tile_x as f32 * crate::config::TILESIZE as f32;
    let y = tile_y as f32 * crate::config::TILESIZE as f32;
    let rotation = tree_rotation(tile_x, tile_y, time);
    let (hw, hh) = (w / 2.0, h / 2.0);
    let (sin, cos) = rotation.to_radians().sin_cos();
    [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|(lx, ly)| {
        let vx = x + lx * cos - ly * sin;
        let vy = y + lx * sin + ly * cos;
        let dx = sin_scl(vy * 3.0 + time, 30.0, 0.2) + sin_scl(vx * 3.0 - time, 70.0, 0.8);
        let dy = cos_scl(vx * 3.0 + time + 8.0, 36.0, 0.22) + sin_scl(vy * 3.0 - time, 50.0, 0.2);
        [vx + dx, vy + dy]
    })
}

/// `TallBlock.shadowOffset` (`TallBlock.java:10`, -3).
pub const TALL_BLOCK_SHADOW_OFFSET: f32 = -3.0;
/// `TallBlock.shadowAlpha` (`TallBlock.java:13`, 0.6).
pub const TALL_BLOCK_SHADOW_ALPHA: f32 = 0.6;

/// `TallBlock.drawBase` rotation (`TallBlock.java:25`):
/// `Mathf.randomSeedRange(tile.pos() + 1, rotationRand)`.
pub fn tall_block_rotation(tile_x: i32, tile_y: i32) -> f32 {
    random_seed_range(pack_point2(tile_x, tile_y).wrapping_add(1), 20.0)
}

/// Exact `Mathf.randomSeedRange(seed, range)` (`Mathf.java:321-324`):
/// `rand.setSeed(seed * 99999); range * (rand.nextFloat() - 0.5) * 2`.
pub fn random_seed_range(seed: u64, range: f32) -> f32 {
    let mut rand = ArcRand::new(seed.wrapping_mul(99999));
    range * (rand.next_float() - 0.5) * 2.0
}

/// `Cliff.drawBase` region index: `tile.data & 0xff` (`Cliff.java:26-29`).
pub fn cliff_region_index(data: i8) -> usize {
    (data as u8) as usize
}

/// `Cliff.drawBase` region name (`cliffs[tile.data & 0xff]`, loaded by
/// `@Load(value = "cliffmask#", length = 256)`).
pub fn cliff_region_name(data: i8) -> String {
    format!("cliffmask{}", cliff_region_index(data))
}

/// `Cliff.drawBase` tint (`Cliff.java:28`):
/// `Color.set(tile.floor().mapColor).mul(1.6f)`.
pub fn cliff_tint(map_color: Rgba) -> Rgba {
    Rgba::new(
        map_color.r * 1.6,
        map_color.g * 1.6,
        map_color.b * 1.6,
        map_color.a,
    )
}

/// `Cliff.minimapColor` (`Cliff.java:32-34`): `mapColor.mul(1.2f)`.
pub fn cliff_minimap_color(map_color: Rgba) -> Rgba {
    Rgba::new(
        map_color.r * 1.2,
        map_color.g * 1.2,
        map_color.b * 1.2,
        map_color.a,
    )
}

/// `RuneOverlay.characters` (`RuneOverlay.java:15`, 109).
pub const RUNE_CHARACTERS: usize = 109;

/// `RuneOverlay.drawBase` letter region (`RuneOverlay.java:31-36`):
/// `letterRegions[tile.overlayData & 0xff]` (loaded as `<name><index>` when the
/// name matches `@#`), skipped when the index is out of range.
pub fn rune_region_name(name: &str, overlay_data: i8) -> Option<String> {
    let index = (overlay_data as u8) as usize;
    (index < RUNE_CHARACTERS).then(|| format!("{name}{index}"))
}

/// `CharacterOverlay.chars` (`CharacterOverlay.java:17`): the reduced
/// 6-bit character set used by the editor.
pub const CHARACTER_CHARS: &str =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890\"!?.,;:()[]{}<>|/@\\^-%+=#_&~";

/// `CharacterOverlay`'s letter-prefix annotation is a fixed string, not the
/// block name: `@Load(value = "character-overlay#", length = 64)`
/// (`CharacterOverlay.java:20-21`), so the white variant shares the grey
/// sprite sheet.
pub const CHARACTER_OVERLAY_PREFIX: &str = "character-overlay";
/// `CharacterOverlay` letter count (`CharacterOverlay.java:20`, 64).
pub const CHARACTER_LETTERS: usize = 64;

/// `CharOverlayData.character` (`CharacterOverlay.java:130-135`): the low six
/// bits of `tile.overlayData`.
pub fn character_index(overlay_data: i8) -> usize {
    (overlay_data as u8 & 0x3f) as usize
}

/// `CharOverlayData.rotation` (`CharacterOverlay.java:136-141`): the top two
/// bits, drawn as `rotation * 90f` (`:31`).
pub fn character_rotation(overlay_data: i8) -> f32 {
    ((overlay_data as u8 >> 6) & 0x3) as f32 * 90.0
}

/// `CharacterOverlay.drawBase` letter region (`:29-32`).
pub fn character_region_name(name: &str, overlay_data: i8) -> String {
    format!("{name}{}", character_index(overlay_data))
}

/// Per-block overlay letter tint (`RuneOverlay.color`/`CharacterOverlay.color`
/// initializers, `Blocks.java:955-968`): `rune-overlay` is
/// `Color.valueOf("3f4049")`, `character-overlay` is `Pal.metalGrayDark`
/// (`6e7080`) and every other overlay (`character-overlay-white`,
/// `rune-overlay-crux`) keeps `Color.white`.
pub fn overlay_color(name: &str) -> Rgba {
    match name {
        "rune-overlay" => Rgba::from_rgba8888(0x3f4049ff),
        "character-overlay" => {
            let color = crate::render::drawf::pal::METAL_GRAY_DARK;
            Rgba::new(color[0], color[1], color[2], color[3])
        }
        _ => Rgba::WHITE,
    }
}

/// `SteamVent.offsets` after the static `p.sub(1, 1)` (`SteamVent.java:17-34`):
/// the nine neighbour offsets checked by `checkAdjacent`.
pub const STEAM_VENT_OFFSETS: [[i32; 2]; 9] = [
    [-1, -1],
    [0, -1],
    [0, 0],
    [-1, 0],
    [-2, 0],
    [-2, -1],
    [-2, -2],
    [-1, -2],
    [0, -2],
];

/// `SteamVent.drawMain`'s `checkAdjacent` (`SteamVent.java:108-116`): every one
/// of the nine offsets around `(x, y)` must be the same vent floor.
pub fn steam_vent_adjacent(
    x: i32,
    y: i32,
    floor: BlockId,
    floor_at: impl Fn(i32, i32) -> Option<BlockId>,
) -> bool {
    STEAM_VENT_OFFSETS
        .iter()
        .all(|[dx, dy]| floor_at(x + dx, y + dy) == Some(floor))
}

/// `SteamVent.drawMain` overlay origin (`SteamVent.java:77-82`): when `(x, y)`
/// is the top-right tile of a complete 3×3 vent, the 96×96 variant sprite is
/// drawn at `world(x - 1, y - 1)` (the 3×3 centre), so the caller tiling the
/// quad gets the same layout.
pub fn steam_vent_overlay_tile(
    x: i32,
    y: i32,
    floor: BlockId,
    floor_at: impl Fn(i32, i32) -> Option<BlockId>,
) -> Option<(i32, i32)> {
    steam_vent_adjacent(x, y, floor, floor_at).then_some((x - 1, y - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_dist_wraps() {
        assert_eq!(angle_dist(10.0, 10.0), 0.0);
        assert!((angle_dist(350.0, 10.0) - 20.0).abs() < 1e-5);
        assert!((angle_dist(0.0, 180.0) - 180.0).abs() < 1e-5);
        assert!((angle_dist(225.0, 45.0) - 180.0).abs() < 1e-5);
    }

    /// `SeaBush.drawBase` (`SeaBush.java:25-46`): seven lobes for the default
    /// range, the pivot shift matches the `origin*4` pixel draw, and the
    /// `angleDist(ba, 225) <= botAngle` split picks the bottom region.
    #[test]
    fn sea_bush_lobes_follow_the_java_stream() {
        let params = SeaBushParams::default();
        let lobes = sea_bush_lobes(12, 7, 0.0, 32.0, 22.0, &params);
        assert_eq!(lobes.len(), 7);
        // Deterministic per tile.
        assert_eq!(lobes, sea_bush_lobes(12, 7, 0.0, 32.0, 22.0, &params));
        assert_ne!(lobes, sea_bush_lobes(13, 7, 0.0, 32.0, 22.0, &params));
        // Angles are `offset + i/lobes * 360 + bob`; one lobe per
        // `360 / lobes` step (`bob` is bounded by `magMax = 15`), and every
        // lobe keeps its natural sprite size.
        for (i, lobe) in lobes.iter().enumerate() {
            assert!((lobe.w - 32.0).abs() < 1e-5);
            assert!((lobe.h - 22.0).abs() < 1e-5);
            let next = lobes[(i + 1) % lobes.len()];
            let step = angle_dist(lobe.rotation, next.rotation);
            assert!(
                (360.0 / 7.0 - 30.0..=360.0 / 7.0 + 30.0).contains(&step),
                "{i}: step {step}"
            );
            // Center is near the tile (origin + sprite half-size shift).
            assert!((lobe.cx - 96.0).abs() < 40.0, "{i}: {}", lobe.cx);
            assert!((lobe.cy - 56.0).abs() < 40.0, "{i}: {}", lobe.cy);
        }
        // The bottom-region split picks at least one lobe among seven
        // (`angleDist(ba, 225) <= 60` spans 120°), and its fill is stable.
        assert!(lobes.iter().any(|lobe| lobe.bot));
        // The time term moves the lobes (wobble).
        let later = sea_bush_lobes(12, 7, 25.0, 32.0, 22.0, &params);
        assert!(
            lobes
                .iter()
                .zip(later.iter())
                .any(|(a, b)| a.rotation != b.rotation)
        );
    }

    /// `Cliff.drawBase` (`Cliff.java:26-34`): `tile.data & 0xff` picks the
    /// `cliffmask` region and the floor `mapColor * 1.6` is the tint.
    #[test]
    fn cliff_uses_the_mask_and_map_color() {
        assert_eq!(cliff_region_index(0), 0);
        assert_eq!(cliff_region_index(0x22), 0x22);
        assert_eq!(cliff_region_index(-1), 255);
        assert_eq!(cliff_region_name(3), "cliffmask3");
        let tint = cliff_tint(Rgba::new(0.5, 0.25, 0.125, 1.0));
        assert!((tint.r - 0.8).abs() < 1e-6);
        assert!((tint.g - 0.4).abs() < 1e-6);
        assert!((tint.b - 0.2).abs() < 1e-6);
        assert_eq!(tint.a, 1.0);
        let minimap = cliff_minimap_color(Rgba::new(0.5, 0.25, 0.125, 1.0));
        assert!((minimap.r - 0.6).abs() < 1e-6);
        // Over-bright map colours are clamped only at the byte pack.
        let bright = cliff_tint(Rgba::new(1.0, 1.0, 1.0, 1.0));
        assert!((bright.r - 1.6).abs() < 1e-6);
    }

    /// `TallBlock.drawBase` (`TallBlock.java:25`): `randomSeedRange` stays in
    /// `[-20, 20]` and is deterministic per tile.
    #[test]
    fn tall_block_rotation_is_seeded() {
        for x in 0..10 {
            let rot = tall_block_rotation(x, 3);
            assert!(rot.abs() <= 20.0 + 1e-4, "{x}: {rot}");
            assert_eq!(rot, tall_block_rotation(x, 3));
        }
        assert_ne!(tall_block_rotation(1, 3), tall_block_rotation(2, 3));
        // Exact Arc stream: seed = (pos + 1) * 99999.
        let mut rand = ArcRand::new((pack_point2(2, 3) + 1).wrapping_mul(99999));
        let expected = 20.0 * (rand.next_float() - 0.5) * 2.0;
        assert_eq!(tall_block_rotation(2, 3), expected);
    }

    /// `TreeBlock.drawBase` (`TreeBlock.java:19-38`): the sprite rotates in
    /// 90° seeded steps plus the three sine terms, and the warp keeps the
    /// quad near its natural size.
    #[test]
    fn tree_quad_rotates_and_warps() {
        let corners = tree_quad(10, 10, 0.0, 310.0, 298.0);
        assert_eq!(corners.len(), 4);
        let rot = tree_rotation(10, 10, 0.0);
        // `randomSeed(pos, 0, 4) * 90` dominates.
        assert!(rot.abs() <= 360.0 + 4.0);
        let quad_width = (corners[1][0] - corners[0][0]).abs();
        assert!(quad_width > 1.0 && quad_width < 600.0);
        // The warp is time-dependent but bounded (scl 30/70, mag .2/.8).
        let later = tree_quad(10, 10, 12.0, 310.0, 298.0);
        assert!(corners.iter().zip(later.iter()).any(|(a, b)| a != b));
        for corner in &later {
            assert!(corner[0].is_finite() && corner[1].is_finite());
        }
        // Shadow offset (`TreeBlock.java:12`).
        assert_eq!(TREE_SHADOW_OFFSET, -4.0);
    }

    /// `RuneOverlay` / `CharacterOverlay` letter selection
    /// (`RuneOverlay.java:31-36`, `CharacterOverlay.java:29-32,130-141`).
    #[test]
    fn overlay_letters_follow_the_packed_data() {
        assert_eq!(
            rune_region_name("rune-overlay", 0).as_deref(),
            Some("rune-overlay0")
        );
        assert_eq!(
            rune_region_name("rune-overlay", 108).as_deref(),
            Some("rune-overlay108")
        );
        assert_eq!(rune_region_name("rune-overlay", 109), None);
        assert_eq!(rune_region_name("rune-overlay", -1), None);
        assert_eq!(
            rune_region_name("rune-overlay-crux", 3).as_deref(),
            Some("rune-overlay-crux3")
        );

        // Low 6 bits = letter, top 2 bits = rotation quarter-turns.
        assert_eq!(character_index(0), 0);
        assert_eq!(character_index(0x3f), 63);
        assert_eq!(character_rotation(0), 0.0);
        assert_eq!(character_rotation(0x40), 90.0);
        assert_eq!(character_rotation(0x80u8 as i8), 180.0);
        assert_eq!(character_rotation(0xc0u8 as i8), 270.0);
        // The index is independent of the rotation bits.
        assert_eq!(character_index(0x45), 5);
        assert_eq!(character_rotation(0x45), 90.0);
        assert_eq!(
            character_region_name(CHARACTER_OVERLAY_PREFIX, 0x45),
            "character-overlay5"
        );
        assert_eq!(CHARACTER_CHARS.chars().count(), 64);
        assert_eq!(CHARACTER_LETTERS, 64);
    }

    /// Overlay letter tints (`Blocks.java:955-968`).
    #[test]
    fn overlay_colors_match_the_content_initializers() {
        assert_eq!(
            overlay_color("rune-overlay"),
            Rgba::from_rgba8888(0x3f4049ff)
        );
        assert_eq!(
            overlay_color("character-overlay"),
            Rgba::from_rgba8888(0x6e7080ff)
        );
        assert_eq!(overlay_color("character-overlay-white"), Rgba::WHITE);
        assert_eq!(overlay_color("rune-overlay-crux"), Rgba::WHITE);
    }

    /// `SteamVent` (`SteamVent.java:17-34,77-82,108-116`): only the top-right
    /// tile of a complete 3×3 vent satisfies `checkAdjacent`, and the 96×96
    /// overlay is drawn at the 3×3 centre.
    #[test]
    fn steam_vent_requires_the_full_three_by_three() {
        let vent = BlockId::new(42);
        // A full 3×3 vent spanning (4..=6, 4..=6).
        let full = |x: i32, y: i32| ((4..=6).contains(&x) && (4..=6).contains(&y)).then_some(vent);
        // Only the top-right tile checks out.
        assert_eq!(steam_vent_overlay_tile(6, 6, vent, full), Some((5, 5)));
        assert!(steam_vent_overlay_tile(5, 6, vent, full).is_none());
        assert!(steam_vent_overlay_tile(6, 5, vent, full).is_none());
        assert!(steam_vent_overlay_tile(6, 6, BlockId::new(7), full).is_none());
        // A missing corner breaks the adjacency.
        let broken = |x: i32, y: i32| {
            ((4..=6).contains(&x) && (4..=6).contains(&y) && !(x == 4 && y == 4)).then_some(vent)
        };
        assert!(steam_vent_overlay_tile(6, 6, vent, broken).is_none());
    }
}

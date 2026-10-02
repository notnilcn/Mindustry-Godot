// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/Generators.java` (`block-icons` pass) and
// `tools/src/mindustry/tools/ImagePacker.java` (`saveScaled`/`drawScaledFit`,
// `get` copy semantics, icon composition).
//
//! The `block-icons` pass (plan 03 §5 M3): outlines, team recoloring,
//! `block-<name>-full`, `ui/block-<name>-ui` and `block_colors.png`.

use anyhow::Result;
use mind_atlas::pixmaps::{self, Pixmap};
use mind_core::content::load::ContentRegistry;
use mind_core::content::registries::blocks::BlockKind;

use crate::generate::GenCtx;
use crate::generate::MAX_UI_ICON;
use crate::generate::metadata::{self, IconCtx};

/// Whether the block is skipped by `Generators.block-icons`
/// (`isAir() || ConstructBlock || OreBlock || LegacyBlock`).
fn is_excluded(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::AirBlock | BlockKind::ConstructBlock | BlockKind::OreBlock
    )
}

/// Runs the pass over all content blocks in ID order.
pub fn run(ctx: &mut GenCtx, registry: &ContentRegistry) -> Result<()> {
    let block_count = registry.blocks().len();
    let mut colors = Pixmap::new(block_count, 1);

    for (index, block) in registry.blocks().iter().enumerate() {
        if is_excluded(block.kind) {
            continue;
        }
        let meta = metadata::block_meta(&block.name, block.kind);
        let (icons, to_outline, body_outlined, outline_index) = {
            let ctx_icons = IconCtx {
                name: &block.name,
                region: &block.region,
                kind: block.kind,
                size: block.size,
                variants: meta.variants,
                has: &|name: &str| ctx.atlas.has(name),
            };
            let icons = metadata::generated_icons(&ctx_icons);
            let outline_index = metadata::outline_icon_index(block.kind, icons.len());
            let to_outline = metadata::make_icon_regions(&ctx_icons)
                .into_iter()
                .chain(metadata::regions_to_outline(&ctx_icons))
                .collect::<Vec<_>>();
            (
                icons,
                to_outline,
                metadata::turret_body_outlined(&ctx_icons),
                outline_index,
            )
        };
        let outline_color = metadata::outline_color(&block.name);
        let outline_radius = metadata::DEFAULT_OUTLINE_RADIUS;

        // Team recolor (`Block.createIcons`): palette teams only.
        let team_region = format!("{}-team", block.name);
        let mut shard_team_top: Option<Pixmap> = None;
        if ctx.atlas.has(&team_region) {
            let teamr = ctx.atlas.get(&team_region)?.as_ref().clone();
            for team in metadata::TEAMS.iter().filter(|team| team.has_palette) {
                let mut out = Pixmap::new(teamr.width, teamr.height);
                for y in 0..teamr.height {
                    for x in 0..teamr.width {
                        let color = teamr.get_raw(x, y);
                        let palette_index = match color {
                            0xffff_ffff => Some(0),
                            0xdcc6_c6ff | 0xdbc5_c5ff => Some(1),
                            0x9d7f_7fff | 0x9e80_80ff => Some(2),
                            _ => None,
                        };
                        out.set_raw(x, y, palette_index.map_or(color, |i| team.palette[i]));
                    }
                }
                ctx.atlas
                    .save(&out, &format!("{}-team-{}", block.name, team.name))?;
                if team.id == metadata::SHARDED_ID {
                    shard_team_top = Some(out);
                }
            }
        }

        // `makeIconRegions` (empty in vanilla) + `getRegionsToOutline`.
        for region in &to_outline {
            let pixmap = ctx.atlas.get(region)?.as_ref().clone();
            let outlined = pixmap.outline(outline_color, outline_radius);
            ctx.atlas.save(&outlined, &format!("{region}-outline"))?;
        }
        if body_outlined {
            let pixmap = ctx.atlas.get(&block.region)?.as_ref().clone();
            let outlined = pixmap.outline(outline_color, outline_radius);
            ctx.atlas
                .save(&outlined, &format!("{}-outline", block.region))?;
        }

        if icons.is_empty() {
            continue;
        }

        // `outlineIcon`: outline the chosen icon region (padded, 1px, saved
        // under the region's own name) and remember the unpadded outline.
        let mut last: Option<Pixmap> = None;
        if let Some(outline_index) = outline_index
            && ctx.atlas.has(&icons[outline_index])
        {
            let region = icons[outline_index].clone();
            let base = ctx.atlas.get(&region)?.as_ref().clone();
            let mut out = base.outline(outline_color, outline_radius);
            let outlined_index = metadata::outlined_icon(block.kind);
            if outlined_index >= 0 {
                for extra in icons.iter().skip(outlined_index as usize + 1) {
                    let pixmap = ctx.atlas.get(extra)?;
                    out.draw_blended(&pixmap, 0, 0);
                }
            }
            last = Some(out);

            let mut padded = Pixmap::new(base.width + 2, base.height + 2);
            padded.draw(&base, 1, 1);
            let padded = padded.outline(outline_color, outline_radius);
            // `region.path.delete(); save(padded, region.name)`.
            ctx.atlas.replace(&region, &region, &padded)?;
        }

        // Compose the full icon.
        let mut image: Option<Pixmap> = None;
        if ctx.atlas.has(&icons[0]) {
            let mut composed = ctx.atlas.get(&icons[0])?.as_ref().clone();
            let sharded = format!("{}-team-sharded", block.name);
            for (i, region) in icons.iter().enumerate() {
                if i + 1 != icons.len() || last.is_none() {
                    let pixmap = ctx.atlas.get(region)?;
                    composed.draw_blended(&pixmap, 0, 0);
                } else if let Some(outline) = &last {
                    composed.draw_blended(outline, 0, 0);
                }
                if *region == sharded
                    && let Some(top) = &shard_team_top
                {
                    composed.draw_blended(top, 0, 0);
                }
            }
            if !(icons.len() == 1 && icons[0] == block.region && shard_team_top.is_none()) {
                ctx.atlas
                    .save(&composed, &format!("block-{}-full", block.name))?;
            }
            let size = composed.width.min(MAX_UI_ICON);
            let scaled = save_scaled(&composed, size);
            ctx.atlas
                .save(&scaled, &format!("ui/block-{}-ui", block.name))?;
            image = Some(composed);
        } else if let Some(shallow) = ctx.gens.get(&block.name) {
            image = Some(shallow.as_ref().clone());
        }

        let Some(image) = image else {
            continue;
        };

        // Map color for `block_colors.png` (alpha encodes square-sprite).
        let mut has_empty = false;
        let mut asum = 0f32;
        let mut average = [0f32; 3];
        for y in 0..image.height {
            for x in 0..image.width {
                let color = image.get_raw(x, y);
                let a = pixmaps::ai(color) as f32 / 255.0;
                average[0] += pixmaps::ri(color) as f32 / 255.0 * a;
                average[1] += pixmaps::gi(color) as f32 / 255.0 * a;
                average[2] += pixmaps::bi(color) as f32 / 255.0 * a;
                asum += a;
                if a < 0.9 {
                    has_empty = true;
                }
            }
        }
        let factor = if asum > 0.0 { 1.0 / asum } else { 0.0 };
        let tint = if metadata::is_floor_kind(block.kind) && !meta.wall_ore {
            0.77
        } else {
            1.1
        };
        let alpha = if has_empty { 0.1 } else { 1.0 };
        let packed = pixmaps::rgba8888f(
            average[0] * factor * tint,
            average[1] * factor * tint,
            average[2] * factor * tint,
            alpha,
        );
        colors.set_raw(index, 0, packed);
    }

    ctx.extras.insert(String::from("block_colors"), colors);
    Ok(())
}

/// `ImagePacker.saveScaled`: `<size>×<size>` nearest... upstream uses
/// filtering=true (bilinear) with blending.
fn save_scaled(pixmap: &Pixmap, size: usize) -> Pixmap {
    let mut scaled = Pixmap::new(size, size);
    scaled.draw_filtered(
        pixmap,
        0,
        0,
        pixmap.width,
        pixmap.height,
        0,
        0,
        size,
        size,
        true,
        true,
    );
    scaled
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_atlas::pixmaps::rgba8888;

    #[test]
    fn save_scaled_keeps_output_size() {
        let mut pix = Pixmap::new(7, 3);
        pix.fill(rgba8888(255, 0, 0, 255));
        let scaled = save_scaled(&pix, 32);
        assert_eq!(scaled.width, 32);
        assert_eq!(scaled.height, 32);
    }

    #[test]
    fn excluded_kinds() {
        assert!(is_excluded(BlockKind::AirBlock));
        assert!(is_excluded(BlockKind::ConstructBlock));
        assert!(is_excluded(BlockKind::OreBlock));
        assert!(!is_excluded(BlockKind::Wall));
    }
}

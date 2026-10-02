// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/Generators.java` (`ore-icons` pass).

//! The `ore-icons` pass (plan 03 §5 M3): variant shadows, environment
//! replacement, `block-<name>-full`, `ui/block-<name>-ui`.

use anyhow::Result;
use mind_atlas::pixmaps::{self};
use mind_core::content::load::ContentRegistry;
use mind_core::content::registries::blocks::BlockKind;

use crate::generate::GenCtx;
use crate::generate::metadata::{self, TILE_SIZE};

/// `Generators.ore-icons`.
pub fn run(ctx: &mut GenCtx, registry: &ContentRegistry) -> Result<()> {
    let shadow_color = pixmaps::rgba8888f(0.0, 0.0, 0.0, 0.3);
    for block in registry.blocks().iter() {
        if block.kind != BlockKind::OreBlock {
            continue;
        }
        let meta = metadata::block_meta(&block.name, block.kind);
        let variants = meta.variants.max(1);
        // `Block.load`: variantRegions[i] = find(name + (i + 1)).
        let variant_region = |i: u32| format!("{}{}", block.name, i + 1);

        for i in 0..variants {
            let region = variant_region(i);
            let base = ctx.atlas.get(&region)?.as_ref().clone();
            let mut image = base.copy();

            let offset = (image.width / TILE_SIZE as usize).saturating_sub(1);
            for x in 0..image.width {
                for y in offset..image.height {
                    // Draw a semi-transparent background where the source is
                    // opaque one `offset` above.
                    if base.get_a(x, y - offset) != 0 {
                        image.set_raw(x, y, pixmaps::blend(shadow_color, base.get_raw(x, y)));
                    }
                }
            }
            image.draw_blended(&base, 0, 0);

            // `replace(ore.variantRegions[i], image)` + `save(..., name + (i+1))`
            // resolve to the same flattened region; write once in place.
            ctx.atlas
                .save(&image, &format!("blocks/environment/{region}"))?;

            ctx.atlas
                .save(&image, &format!("block-{}-full", block.name))?;
            ctx.atlas
                .save(&image, &format!("ui/block-{}-ui", block.name))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilesize_is_vars_value() {
        assert_eq!(TILE_SIZE, 8);
    }

    #[test]
    fn shadow_blend_semantics() {
        // 30% black over opaque red darkens; over transparent yields the
        // shadow color itself.
        let red = pixmaps::rgba8888(255, 0, 0, 255);
        let shadow = pixmaps::rgba8888f(0.0, 0.0, 0.0, 0.3);
        assert_eq!(pixmaps::ri(pixmaps::blend(shadow, red)), 179);
        assert_eq!(pixmaps::ai(pixmaps::blend(shadow, red)), 255);
        let muted = pixmaps::blend(shadow, pixmaps::CLEAR);
        assert_eq!(pixmaps::ai(muted), 76);
    }
}

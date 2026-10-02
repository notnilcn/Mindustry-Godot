// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/Generators.java` (`autotiles` pass) +
// `tools/src/mindustry/tools/ImageTileGenerator.java`.

//! The `autotiles` pass (plan 03 M2).
//!
//! Upstream iterates content blocks with `autotile`/`autotileVariants`;
//! until the plan-02 metadata contract lands (M3) this pass is filename-driven:
//! every staged `blocks/environment/<name>-autotile[N].png` produces the 47
//! slices `<name>[-N]-<i>.png`, deletes its source, and writes the bottom-right
//! 32×32 preview as `<name>.png` when absent. M3 cross-checks this set against
//! the content metadata (see plan Changelog).

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use mind_atlas::autotile;

use crate::generate::GenCtx;

/// Runs the autotile pass over the staging tree.
pub fn run(ctx: &mut GenCtx) -> Result<()> {
    // Discover sources: name -> variants (sorted).
    let mut sources: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for name in ctx.atlas.names().map(str::to_owned).collect::<Vec<_>>() {
        if let Some((base, variant)) = parse_autotile_name(&name) {
            sources.entry(base).or_default().push(variant);
        }
    }
    for variants in sources.values_mut() {
        variants.sort_unstable();
    }

    for (base, variants) in sources {
        let multi = variants.len() > 1;
        for variant in variants {
            let source_name = if multi && variant > 0 {
                format!("{base}-autotile{variant}")
            } else {
                format!("{base}-autotile")
            };
            if !ctx.atlas.has(&source_name) {
                continue;
            }
            let image = ctx
                .atlas
                .get(&source_name)
                .with_context(|| format!("loading autotile source {source_name}"))?;
            let out_base = if multi {
                format!("{base}-{variant}")
            } else {
                base.clone()
            };
            let slices = autotile::generate_slices(&image, &out_base)
                .with_context(|| format!("autotiling {source_name}"))?;
            for (i, slice) in slices.iter().enumerate() {
                ctx.atlas
                    .save(slice, &format!("blocks/environment/{out_base}-{i}"))?;
            }
            // The raw autotile source is never packed.
            ctx.atlas.delete(&source_name)?;

            if variant <= 1 {
                // Bottom-right 32x32 crop is the preview/main sprite.
                let preview = image.crop(32, 32, 32, 32);
                if !ctx.atlas.has(&base) {
                    ctx.atlas
                        .save(&preview, &format!("blocks/environment/{base}"))?;
                }
                ctx.gens.insert(base.clone(), std::rc::Rc::new(preview));
            }
        }
    }
    Ok(())
}

/// `<name>-autotile[N]` → `(name, N)` (N = 0 when unsuffixed).
fn parse_autotile_name(name: &str) -> Option<(String, u32)> {
    if let Some(base) = name.strip_suffix("-autotile") {
        return (!base.is_empty()).then(|| (base.to_owned(), 0));
    }
    let (base, digits) = name.rsplit_once("-autotile")?;
    if base.is_empty() || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((base.to_owned(), digits.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_autotile_names() {
        assert_eq!(
            parse_autotile_name("dirt-autotile"),
            Some(("dirt".to_owned(), 0))
        );
        assert_eq!(
            parse_autotile_name("metal-tiles-11-autotile3"),
            Some(("metal-tiles-11".to_owned(), 3))
        );
        assert_eq!(parse_autotile_name("copper-wall"), None);
        assert_eq!(parse_autotile_name("conveyor-0-0"), None);
    }
}

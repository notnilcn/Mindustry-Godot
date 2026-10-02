// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/Generators.java` (`item-icons`, `sector-icons`,
// `team-icons` passes).

//! Content icon passes (plan 03 §5 M3).

use anyhow::Result;
use mind_atlas::pixmaps::{self, Pixmap};
use mind_core::content::Rgba;
use mind_core::content::load::ContentRegistry;

use crate::generate::GenCtx;
use crate::generate::metadata;

/// `Generators.item-icons`: item/liquid/status UI icons. Statuses get the
/// effect tint, a 6px container and a `Pal.gray` outline.
pub fn item_icons(ctx: &mut GenCtx, registry: &ContentRegistry) -> Result<()> {
    for item in registry.items() {
        item_icon(ctx, "item", &item.name, None)?;
    }
    for liquid in registry.liquids() {
        item_icon(ctx, "liquid", &liquid.name, None)?;
    }
    for status in registry.statuses() {
        if !ctx.atlas.has(&format!("status-{}", status.name)) {
            continue;
        }
        item_icon(ctx, "status", &status.name, Some(rgba8888(status.color)))?;
    }
    Ok(())
}

/// One `item-icons` entry; statuses get the tint/container/outline treatment.
fn item_icon(ctx: &mut GenCtx, type_name: &str, name: &str, tint: Option<u32>) -> Result<()> {
    let region = format!("{type_name}-{name}");
    let mut base = ctx.atlas.get(&region)?.as_ref().clone();
    if let Some(tint) = tint {
        base.replace(|color| pixmaps::muli(color, tint));
        let mut container = Pixmap::new(base.width + 6, base.height + 6);
        container.draw_blended(&base, 3, 3);
        base = container.outline(metadata::PAL_GRAY, 3);
    }
    ctx.atlas
        .save(&base, &format!("ui/{type_name}-{name}-ui"))?;
    Ok(())
}

/// `Generators.sector-icons`: 10px container + `Pal.darkerGray` outline,
/// moved to `ui/sector-<name>`.
pub fn sector_icons(ctx: &mut GenCtx, registry: &ContentRegistry) -> Result<()> {
    for sector in registry.sectors() {
        let region = format!("sector-{}", sector.name);
        if !ctx.atlas.has(&region) {
            continue;
        }
        let base = ctx.atlas.get(&region)?.as_ref().clone();
        let mut container = Pixmap::new(base.width + 10, base.height + 10);
        container.draw_blended(&base, 5, 5);
        let outlined = container.outline(metadata::PAL_DARKER_GRAY, 5);
        // `replace("../ui/sector-" + name, "sector-" + name, ...)`.
        ctx.atlas
            .replace(&format!("ui/sector-{}", sector.name), &region, &outlined)?;
    }
    Ok(())
}

/// `Generators.team-icons`: tint (`derelict` = `b7b8c9`), delete source,
/// outline `Pal.gray`, save as `ui/team-<name>`.
pub fn team_icons(ctx: &mut GenCtx, _registry: &ContentRegistry) -> Result<()> {
    for team in metadata::TEAMS {
        let region = format!("team-{}", team.name);
        if !ctx.atlas.has(&region) {
            continue;
        }
        let tint = if team.name == "derelict" {
            metadata::DERELICT_ICON_COLOR
        } else {
            team.color
        };
        let mut base = ctx.atlas.get(&region)?.as_ref().clone();
        base.replace(|color| pixmaps::muli(color, tint));
        ctx.atlas.delete(&region)?;
        let outlined = base.outline(metadata::PAL_GRAY, 3);
        ctx.atlas
            .save(&outlined, &format!("ui/team-{}", team.name))?;
    }
    Ok(())
}

/// `Color.rgba()` for a content color.
fn rgba8888(color: Rgba) -> u32 {
    pixmaps::rgba8888f(color.r, color.g, color.b, color.a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_pack_matches_channel_math() {
        // `Color.rgba()` truncates (`(int)(0.5 * 255)` == 127).
        let color = Rgba::new(1.0, 0.5, 0.0, 1.0);
        assert_eq!(rgba8888(color), 0xff7f_00ff);
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/Generators.java` (`unit-icons` pass),
// `core/src/mindustry/type/UnitType.java` (`load`/`getRegionsToOutline`/
// `needsBodyOutline`/`createIcons` region chain),
// `core/src/mindustry/type/Weapon.java` (`load`),
// `core/src/mindustry/entities/part/RegionPart.java` (`load`/`getOutlines`),
// `tools/src/mindustry/tools/ImagePacker.java`
// (`drawCenter`/`drawScaledFit`/`saveScaled`, `Draw.scl` setup).

//! The `unit-icons` pass (plan 03 §5 M3/M5): unit outlines, weapon composition,
//! tank tread animation slices, crawl segment outlines, `unit-<name>-full`,
//! deterministic wrecks and the fit-scaled `ui/unit-<name>-ui` icon.
//!
//! Pixel output follows the plan's A3 deviation: algorithms are reimplemented
//! in `mind-atlas` and are Rust↔Rust deterministic only (wreck RNG is seeded
//! from the FNV-1a hash of the unit name, never Java `String.hashCode`).

use std::collections::BTreeSet;

use anyhow::Result;
use mind_atlas::hash::fnv1a;
use mind_atlas::mathf;
use mind_atlas::noise::{Rand, ridged_noise2d, simplex_raw2d};
use mind_atlas::pixmaps::Pixmap;
use mind_core::content::load::ContentRegistry;
use mind_core::content::registries::units::UnitComponent;
use mind_core::content::registries::units::UnitTypeDef;
use mind_core::content::registries::units::weapon::WeaponDef;

use crate::generate::GenCtx;
use crate::generate::MAX_UI_ICON;
use crate::generate::metadata;
use crate::pack_atlas::PackAtlas;

/// Runs the pass over every generator-eligible unit in ID order
/// (`type.internal && !type.internalGenerateSprites` skips hidden units).
pub fn run(ctx: &mut GenCtx, registry: &ContentRegistry) -> Result<()> {
    // `ImagePacker.main`: `Draw.scl = 1f / Core.atlas.find("scale_marker").width`.
    // Keep `scl` as `1 / Draw.scl` (the multiply factor) to avoid division noise.
    let scl_factor = ctx
        .atlas
        .get("scale_marker")
        .map(|pixmap| pixmap.width as f32)
        .unwrap_or(4.0);

    for unit in registry.units().iter() {
        if unit.internal && !unit.internal_generate_sprites {
            continue;
        }
        if let Err(error) = generate_unit(ctx, unit, scl_factor) {
            // Upstream wraps each unit in try/catch and logs a warning.
            eprintln!("mind-tools: warning: skipping unit {}: {error}", unit.name);
        }
    }
    Ok(())
}

/// Cloned, decoded pixels (the generators keep no long-lived `Rc` borrows).
fn get(atlas: &mut PackAtlas, name: &str) -> Result<Pixmap> {
    Ok(atlas.get(name)?.as_ref().clone())
}

/// `ImagePacker.drawCenter` (`Pixmap.draw(other, centered, true)`).
fn draw_center(image: &mut Pixmap, other: &Pixmap) {
    let dx = image.width as i32 / 2 - other.width as i32 / 2;
    let dy = image.height as i32 / 2 - other.height as i32 / 2;
    image.draw_blended(other, dx, dy);
}

/// `ImagePacker.drawScaledFit` (Arc `Scaling.fit` preserves aspect ratio).
fn draw_scaled_fit(base: &mut Pixmap, image: &Pixmap) {
    if image.width == 0 || image.height == 0 {
        return;
    }
    let scl =
        (base.width as f32 / image.width as f32).min(base.height as f32 / image.height as f32);
    let dst_w = (image.width as f32 * scl) as usize;
    let dst_h = (image.height as f32 * scl) as usize;
    let dx = base.width as i32 / 2 - dst_w as i32 / 2;
    let dy = base.height as i32 / 2 - dst_h as i32 / 2;
    base.draw_filtered(
        image,
        0,
        0,
        image.width,
        image.height,
        dx,
        dy,
        dst_w,
        dst_h,
        true,
        true,
    );
}

/// `Color.muli(rgba, factor)` — multiply every channel including alpha.
fn darken(color: u32, factor: f32) -> u32 {
    let r = (((color >> 24) & 0xff) as f32 * factor) as u32 & 0xff;
    let g = (((color >> 16) & 0xff) as f32 * factor) as u32 & 0xff;
    let b = (((color >> 8) & 0xff) as f32 * factor) as u32 & 0xff;
    let a = ((color & 0xff) as f32 * factor) as u32 & 0xff;
    (r << 24) | (g << 16) | (b << 8) | a
}

/// Unit/weapon `-cell` recolor (`Generators.unit-icons` cell `replace`).
fn recolor_cell(color: u32) -> u32 {
    if color == metadata::UNIT_CELL_WHITE {
        metadata::UNIT_CELL_ORANGE
    } else if metadata::UNIT_CELL_GRAY.contains(&color) {
        metadata::UNIT_CELL_DARK_ORANGE
    } else {
        0
    }
}

/// `weaponRegion(weapon)` — `<name>-preview` when present, else `<name>`.
fn weapon_region(atlas: &mut PackAtlas, weapon: &WeaponDef) -> Result<Pixmap> {
    let preview = format!("{}-preview", weapon.name);
    if atlas.has(&preview) {
        get(atlas, &preview)
    } else {
        get(atlas, &weapon.name)
    }
}

/// `Cons2<Weapon, Pixmap> drawWeapon` compositor.
fn draw_weapon(
    image: &mut Pixmap,
    pixmap: &Pixmap,
    weapon: &WeaponDef,
    region_w: usize,
    region_h: usize,
    scl_factor: f32,
) {
    let flipped;
    let src = if weapon.flip_sprite {
        flipped = pixmap.flip_x();
        &flipped
    } else {
        pixmap
    };
    let dx = (weapon.x * scl_factor + image.width as f32 / 2.0 - region_w as f32 / 2.0) as i32;
    let dy = (-weapon.y * scl_factor + image.height as f32 / 2.0 - region_h as f32 / 2.0) as i32;
    image.draw_blended(src, dx, dy);
}

/// Replaces a region's staging file with its outlined version under the same
/// name (`ImagePacker.replace(region, outline(get(region)))`).
fn replace_outlined(atlas: &mut PackAtlas, name: &str, color: u32, radius: i32) -> Result<()> {
    if !atlas.has(name) {
        return Ok(());
    }
    let pixmap = get(atlas, name)?;
    let outlined = pixmap.outline(color, radius);
    atlas.replace_in_place(name, &outlined)
}

/// Generates every derived region for one unit.
fn generate_unit(ctx: &mut GenCtx, unit: &UnitTypeDef, scl_factor: f32) -> Result<()> {
    let outline_color = unit.outline_color.to_rgba8888();
    let outline_radius = unit.outline_radius;
    let atlas = &mut ctx.atlas;

    let regions = metadata::unit_regions(unit, &|name: &str| atlas.has(name));
    // Segment units (latum/renale) have no body sprite; their first segment is
    // the body seed. Everything else needs the body/preview region.
    if unit.segments > 0 {
        if !atlas.has(&regions.segments[0]) {
            return Ok(());
        }
    } else if !atlas.has(&regions.region) {
        return Ok(());
    }

    let components = unit.entity_def.components;
    let is_mech = components.contains(&UnitComponent::Mech);
    let is_legs = components.contains(&UnitComponent::Legs);
    let is_crawl = components.contains(&UnitComponent::Crawl);
    let is_tank = components.contains(&UnitComponent::Tank);

    // 1. `type.getRegionsToOutline(toOutline)` -> `<region>-outline`.
    let mut to_outline = metadata::part_outline_regions(&unit.parts, &unit.name);
    for weapon in &unit.weapons {
        if atlas.has(&weapon.name) {
            to_outline.extend(metadata::part_outline_regions(&weapon.parts, &weapon.name));
        }
    }
    for region in &to_outline {
        if !atlas.has(region) {
            continue;
        }
        let pixmap = get(atlas, region)?;
        let outlined = pixmap.outline(outline_color, outline_radius);
        atlas.save(&outlined, &format!("{region}-outline"))?;
    }

    // 3. Weapon outlines (`weapons.each(Weapon::load)` + outline loop).
    let weapons: Vec<&WeaponDef> = unit
        .weapons
        .iter()
        .filter(|weapon| atlas.has(&weapon.name))
        .collect();
    let mut outlined: BTreeSet<String> = BTreeSet::new();
    for weapon in &weapons {
        if !outlined.insert(weapon.name.clone()) || !atlas.has(&weapon.name) {
            continue;
        }
        if !weapon.top || weapon.parts.iter().any(|part| part.under) {
            let pixmap = get(atlas, &weapon.name)?;
            let outlined_pix = pixmap.outline(outline_color, outline_radius);
            atlas.save(&outlined_pix, &format!("{}-outline", weapon.name))?;
        } else {
            let pixmap = get(atlas, &weapon.name)?;
            let outlined_pix = pixmap.outline(outline_color, outline_radius);
            atlas.replace_in_place(&weapon.name, &outlined_pix)?;
        }
    }

    // 4. Tank tread animation slices (`sample instanceof Tankc`).
    if is_tank
        && let Some(tread_name) = &regions.tread
        && atlas.has(tread_name)
    {
        let tread = get(atlas, tread_name)?;
        for (r, rect) in unit.tread_rects.iter().enumerate() {
            let sx = (rect.x + tread.width as f32 / 2.0) as i32;
            let sy = (rect.y + tread.height as f32 / 2.0) as i32;
            let slice = tread.crop(sx, sy, 1, rect.height as usize);
            for i in 0..unit.tread_frames {
                let pull = unit.tread_pull_offset;
                let mut frame = Pixmap::new(1, slice.height);
                for y in 0..slice.height {
                    let mut idx = y as i32 + i;
                    if idx >= slice.height as i32 {
                        idx -= slice.height as i32;
                        idx += pull;
                        idx = idx.rem_euclid(slice.height as i32);
                    }
                    frame.set_raw(0, y, slice.get_raw(0, idx as usize));
                }
                atlas.save(&frame, &format!("{}-treads{r}-{i}", unit.name))?;
            }
        }
    }

    // 5. In-place outlines for the shared body parts.
    for name in [
        regions.joint.as_deref(),
        regions.foot.as_deref(),
        regions.leg_base.as_deref(),
        regions.base_joint.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        replace_outlined(atlas, name, outline_color, outline_radius)?;
    }
    if is_legs && let Some(leg) = &regions.leg {
        replace_outlined(atlas, leg, outline_color, outline_radius)?;
    }
    if is_tank && let Some(tread) = &regions.tread {
        replace_outlined(atlas, tread, outline_color, outline_radius)?;
    }

    // 6. Body image seed.
    let mut image = if unit.segments > 0 && atlas.has(&regions.segments[0]) {
        get(atlas, &regions.segments[0])?
    } else if unit.draw_body {
        let preview = get(atlas, &regions.preview)?;
        preview.outline(outline_color, outline_radius)
    } else {
        Pixmap::new(1, 1)
    };

    // 7. Crawl segments (outline saved, composite body).
    if is_crawl {
        for i in 0..unit.segments as usize {
            let Some(segment) = regions.segments.get(i) else {
                continue;
            };
            if !atlas.has(segment) {
                continue;
            }
            let pixmap = get(atlas, segment)?;
            let outlined = pixmap.outline(outline_color, outline_radius);
            atlas.save(&outlined, &format!("{}-segment-outline{i}", unit.name))?;
            if i > 0 {
                let added = get(atlas, segment)?;
                draw_center(&mut image, &added);
            }
        }
        atlas.save(&image, &unit.name)?;
    }

    // 8. `needsBodyOutline()` / body outline replacement.
    if unit.always_create_outline {
        atlas.save(&image, &format!("{}-outline", unit.name))?;
    } else if unit.segments == 0 && unit.draw_body {
        let body = get(atlas, &regions.region)?;
        let outlined = body.outline(outline_color, outline_radius);
        atlas.replace_in_place(&unit.name, &outlined)?;
    }

    // 9. Weapons under the base, then mask the base back over them.
    let mut any_under = false;
    for weapon in weapons.iter().filter(|weapon| weapon.layer_offset < 0.0) {
        let region = weapon_region(atlas, weapon)?;
        let outlined = region.outline(outline_color, outline_radius);
        let (width, height) = weapon_dims(atlas, weapon);
        draw_weapon(&mut image, &outlined, weapon, width, height, scl_factor);
        any_under = true;
    }
    if any_under {
        let preview = get(atlas, &regions.preview)?;
        let outlined = preview.outline(outline_color, outline_radius);
        draw_center(&mut image, &outlined);
    }

    // 10. Tank treads + body, mech base/legs + body.
    if is_tank
        && let Some(tread_name) = &regions.tread
        && atlas.has(tread_name)
    {
        let treads = get(atlas, tread_name)?;
        let outlined = treads.outline(outline_color, outline_radius);
        let dx = image.width as i32 / 2 - outlined.width as i32 / 2;
        let dy = image.height as i32 / 2 - outlined.height as i32 / 2;
        image.draw_blended(&outlined, dx, dy);
        let preview = get(atlas, &regions.preview)?;
        draw_center(&mut image, &preview);
    }
    if is_mech {
        if let Some(base) = &regions.base {
            let pixmap = get(atlas, base)?;
            draw_center(&mut image, &pixmap);
        }
        if let Some(leg) = &regions.leg {
            let pixmap = get(atlas, leg)?;
            draw_center(&mut image, &pixmap);
            let flipped = pixmap.flip_x();
            draw_center(&mut image, &flipped);
        }
        let preview = get(atlas, &regions.preview)?;
        draw_center(&mut image, &preview);
    }

    // 11. Weapon outlines on the base.
    for weapon in &weapons {
        if weapon.layer_offset < 0.0 {
            continue;
        }
        let region = weapon_region(atlas, weapon)?;
        let outlined = region.outline(outline_color, outline_radius);
        let (width, height) = weapon_dims(atlas, weapon);
        draw_weapon(&mut image, &outlined, weapon, width, height, scl_factor);
    }

    // 12/13. Body cell mask + recolored team cell.
    if unit.draw_cell {
        let preview = get(atlas, &regions.preview)?;
        draw_center(&mut image, &preview);
        if let Some(cell) = &regions.cell
            && atlas.has(cell)
        {
            let mut pixmap = get(atlas, cell)?;
            pixmap.replace(recolor_cell);
            draw_center(&mut image, &pixmap);
        }
    }

    // 14. Final weapon pass (top regions outlined, cells recolored).
    for weapon in &weapons {
        if weapon.layer_offset < 0.0 {
            continue;
        }
        let region = weapon_region(atlas, weapon)?;
        let pixmap = if weapon.top {
            region.outline(outline_color, outline_radius)
        } else {
            region
        };
        let (width, height) = weapon_dims(atlas, weapon);
        draw_weapon(&mut image, &pixmap, weapon, width, height, scl_factor);

        let cell_name = format!("{}-cell", weapon.name);
        if atlas.has(&cell_name) {
            let mut cell = get(atlas, &cell_name)?;
            cell.replace(recolor_cell);
            draw_weapon(&mut image, &cell, weapon, width, height, scl_factor);
        }
    }

    // 15. Full icon.
    if unit.generate_full_icon {
        atlas.save(&image, &format!("unit-{}-full", unit.name))?;
    }

    generate_wrecks(atlas, unit, &image)?;

    // 17. Fit-scaled UI icon.
    let maxd = image.width.max(image.height).min(MAX_UI_ICON);
    let mut fit = Pixmap::new(maxd, maxd);
    draw_scaled_fit(&mut fit, &image);
    atlas.save(&fit, &format!("ui/unit-{}-ui", unit.name))?;

    Ok(())
}

/// Weapon region dimensions used for the `drawWeapon` offsets.
fn weapon_dims(atlas: &mut PackAtlas, weapon: &WeaponDef) -> (usize, usize) {
    atlas
        .get(&weapon.name)
        .map(|pixmap| (pixmap.width, pixmap.height))
        .unwrap_or((0, 0))
}

/// Deterministic wreck generation (`Generators.unit-icons` wreck block).
///
/// Voronoi/`Noise.rawNoise` are approximated with `simplex_raw2d` (A3: wreck
/// pixels are not a parity ABI; only the region names and determinism are).
fn generate_wrecks(atlas: &mut PackAtlas, unit: &UnitTypeDef, image: &Pixmap) -> Result<()> {
    let seed = fnv1a(&unit.name) as i64;
    let mut rand = Rand::new(seed);
    let degrees = rand.random(360.0);
    let offset_range = image.width.max(image.height) as f32 * 0.15;
    // `new Vec2(1,1).rotate(a)` has direction `45 + a`.
    let dir = 45.0 + rand.random(360.0);
    let len = rand.random(offset_range);
    let ox = mathf::cos_deg(dir) * len + image.width as f32 / 2.0;
    let oy = mathf::sin_deg(dir) * len + image.height as f32 / 2.0;

    const SPLITS: usize = 3;
    let mut wrecks: [Pixmap; SPLITS] = [
        Pixmap::new(image.width, image.height),
        Pixmap::new(image.width, image.height),
        Pixmap::new(image.width, image.height),
    ];
    let crack_freq = 1.0 / (20.0 + image.width as f64 / 8.0);
    let voronoi_scale = 1.0 / (14.0 + image.width as f64 / 40.0);
    let noise_scale = 1.0 / (9.0 + image.width as f64 / 70.0);
    let noise_mag = 60.0 + image.width as f32 / 30.0;

    image.each(|x, y| {
        let rvalue = ridged_noise2d(1, x as f64, y as f64, 3, 0.5, crack_freq) > 0.16;
        let vval = simplex_raw2d(
            seed as i32,
            x as f64 * voronoi_scale,
            y as f64 * voronoi_scale,
        ) > 0.47;
        if vval {
            return;
        }
        let color = image.get_raw(x, y);
        let dst = ((x as f32 - ox).powi(2) + (y as f32 - oy).powi(2)).sqrt() as f64;
        let noise = simplex_raw2d(
            seed as i32 ^ 0x5bd1_e995,
            dst * noise_scale,
            dst * noise_scale,
        ) as f32
            * noise_mag;
        let angle = mathf::atan2(x as f32 - ox, y as f32 - oy);
        let section = ((((angle + noise + degrees) % 360.0) + 360.0) % 360.0 / 360.0
            * SPLITS as f32)
            .clamp(0.0, (SPLITS - 1) as f32) as usize;
        let value = if rvalue { darken(color, 0.7) } else { color };
        wrecks[section].set_raw(x, y, value);
    });

    for (index, wreck) in wrecks.iter().enumerate() {
        atlas.save(wreck, &format!("rubble/{}-wreck{index}", unit.name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recolors_unit_cell_magic_colors() {
        assert_eq!(
            recolor_cell(metadata::UNIT_CELL_WHITE),
            metadata::UNIT_CELL_ORANGE
        );
        assert_eq!(recolor_cell(0xdcc6_c6ff), metadata::UNIT_CELL_DARK_ORANGE);
        assert_eq!(recolor_cell(0xdcc5_c5ff), metadata::UNIT_CELL_DARK_ORANGE);
        assert_eq!(recolor_cell(0x1234_5678), 0);
    }

    #[test]
    fn scaled_fit_preserves_aspect() {
        let mut base = Pixmap::new(32, 32);
        let image = Pixmap::new(64, 16);
        draw_scaled_fit(&mut base, &image);
        assert_eq!(base.width, 32);
        assert_eq!(base.height, 32);
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/Generators.java` (`splashes`, `bubbles`,
// `cliffs`, `cracks`, `scorches`, `edges` passes + `ScorchGenerator`).

//! Environment generator passes (plan 03 M2).

use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Context, Result};
use mind_atlas::mathf;
use mind_atlas::noise::{self, Rand};
use mind_atlas::pixmaps::{self, Pixmap, WHITE, ai, blend, rgba8888f};
use mind_core::content::load::ContentRegistry;
use mind_core::content::registries::blocks::BlockKind;

use crate::generate::metadata::{self, IconCtx};
use crate::generate::{CRACK_REGIONS, GenCtx, MAX_CRACK_SIZE};

fn dst(x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    (dx * dx + dy * dy).sqrt()
}

/// `Generators.splashes` — 12 frames of 32px ring strokes.
pub fn splashes(ctx: &mut GenCtx) -> Result<()> {
    let frames = 12;
    let size = 32;
    for i in 0..frames {
        let fin = i as f32 / frames as f32;
        let fout = 1.0 - fin;
        let stroke = 3.5 * fout;
        let radius = (size as f32 / 2.0) * fin;

        let mut pixmap = Pixmap::new(size, size);
        for y in 0..size {
            for x in 0..size {
                let d = dst(x as f32, y as f32, size as f32 / 2.0, size as f32 / 2.0);
                if (d - radius).abs() <= stroke {
                    pixmap.set_raw(x, y, WHITE);
                }
            }
        }
        ctx.atlas.save(&pixmap, &format!("effects/splash-{i}"))?;
    }
    Ok(())
}

/// `Generators.bubbles` — 16 frames of 40px rings with a shine dot.
pub fn bubbles(ctx: &mut GenCtx) -> Result<()> {
    let frames = 16;
    let size = 40;
    for i in 0..frames {
        let fin = i as f32 / frames as f32;
        let fout = 1.0 - fin;
        let stroke = 3.5 * fout;
        let radius = (size as f32 / 2.0) * fin;
        let shinelen = radius / 2.5;
        let shinerad = stroke * 1.5 + 0.3;
        let shinex = size as f32 / 2.0 + shinelen / std::f32::consts::SQRT_2;
        let shiney = size as f32 / 2.0 - shinelen / std::f32::consts::SQRT_2;

        let mut pixmap = Pixmap::new(size, size);
        for y in 0..size {
            for x in 0..size {
                let d = dst(x as f32, y as f32, size as f32 / 2.0, size as f32 / 2.0);
                if (d - radius).abs() <= stroke
                    || dst(x as f32, y as f32, shinex, shiney) <= shinerad
                {
                    pixmap.set_raw(x, y, WHITE);
                }
            }
        }
        ctx.atlas.save(&pixmap, &format!("effects/bubble-{i}"))?;
    }
    Ok(())
}

/// `Generators.cliffs` — the 256 64×64 cliff masks from `cliff0..7` sources.
pub fn cliffs(ctx: &mut GenCtx) -> Result<()> {
    let size = 64usize;
    // new Color(0.5, 0.5, 0.6, 1).mul(0.98)
    let dark = rgba8888f(0.5 * 0.98, 0.5 * 0.98, 0.6 * 0.98, 1.0);
    // Color.lightGray (0xbfbfbfff)
    let mid = 0xbfbf_bfff;

    let mut images: Vec<Arc<Pixmap>> = Vec::with_capacity(8);
    for i in 0..8 {
        let pixmap = ctx
            .atlas
            .get(&format!("cliff{i}"))
            .with_context(|| format!("cliffs: `cliff{i}` source missing"))?;
        images.push(Arc::from(pixmap.as_ref().clone()));
    }

    // (mask_index, pixels) computed in parallel; written in index order.
    let mut masks: Vec<Option<Pixmap>> = Vec::new();
    masks.resize_with(256, || None);
    {
        let images = &images;
        let masks = &mut masks;
        std::thread::scope(|scope| {
            let chunk = 256usize.div_ceil(8);
            for shard in masks.chunks_mut(chunk).enumerate() {
                let (start, shard) = (shard.0 * chunk, shard.1);
                scope.spawn(move || {
                    for (offset, slot) in shard.iter_mut().enumerate() {
                        let i = (start + offset) as i32;
                        *slot = Some(cliff_mask(i, size, dark, mid, images));
                    }
                });
            }
        });
    }

    for (i, mask) in masks.into_iter().enumerate() {
        let mask = mask.with_context(|| format!("cliffs: missing mask {i}"))?;
        ctx.atlas
            .save(&mask, &format!("blocks/environment/cliffmask{i}"))?;
    }
    Ok(())
}

/// One cliff mask (the `Generators.cliffs` body for a single byte value).
fn cliff_mask(i: i32, size: usize, dark: u32, mid: u32, images: &[Arc<Pixmap>]) -> Pixmap {
    let mut mask = vec![0u8; size * size];
    // `(bi & 0xff) == 128 ? 128 : (byte)bi` — i is the loop value in
    // -128..=127 shifted to 0..=255 by the caller; keep the byte semantics.
    let val = if (i & 0xff) == 128 {
        128
    } else {
        i as i8 as i32
    };

    for (j, image) in images.iter().enumerate() {
        if (val & (1 << j)) != 0 {
            if j % 2 == 1 && (((val & (1 << (j + 1))) != 0) != ((val & (1 << (j - 1))) != 0)) {
                continue;
            }
            for y in 0..size {
                for x in 0..size {
                    let raw = image.get_raw(x, y);
                    if ai(raw) as f32 / 255.0 > 0.1 {
                        // white -> bit 1 -> top; black -> bit 2 -> bottom
                        let r = ((raw >> 24) & 0xff) as f32 / 255.0;
                        mask[x + y * size] |= if r > 0.5 { 1 } else { 2 };
                    }
                }
            }
        }
    }

    let mut result = Pixmap::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let m = mask[x + y * size];
            if m == 0 {
                continue;
            }
            let mut m = m;
            if m == 3 {
                // Mid: find the nearest non-mid color, expanding the search.
                let mut best = 0u8;
                let mut best_dst = 0.0f32;
                let mut found = false;
                let mut rad = 9;
                while rad < 64 {
                    let cx0 = (x as i32 - rad).max(0) as usize;
                    let cx1 = ((x as i32 + rad) as usize).min(size - 1);
                    let cy0 = (y as i32 - rad).max(0) as usize;
                    let cy1 = ((y as i32 + rad) as usize).min(size - 1);
                    for cx in cx0..=cx1 {
                        for cy in cy0..=cy1 {
                            let nval = mask[cx + cy * size];
                            if nval == 1 || nval == 2 {
                                let dx = cx as f32 - x as f32;
                                let dy = cy as f32 - y as f32;
                                let dst2 = dx * dx + dy * dy;
                                if dst2 <= (rad * rad) as f32 && (!found || dst2 < best_dst) {
                                    best = nval;
                                    best_dst = dst2;
                                    found = true;
                                }
                            }
                        }
                    }
                    rad += 7;
                }
                if found {
                    m = best;
                }
            }
            result.set_raw(
                x,
                y,
                if m == 1 {
                    WHITE
                } else if m == 2 {
                    dark
                } else {
                    mid
                },
            );
        }
    }
    result
}

/// `Generators.cracks` — `cracks-<size>-<i>` via Ridged noise + median filter.
pub fn cracks(ctx: &mut GenCtx) -> Result<()> {
    for size in 1..=MAX_CRACK_SIZE {
        let dim = size * 32;
        for i in 0..CRACK_REGIONS {
            let fract = i as f32 / CRACK_REGIONS as f32;
            let mut image = Pixmap::new(dim, dim);
            for y in 0..dim {
                for x in 0..dim {
                    let d = dst(x as f32 / dim as f32, y as f32 / dim as f32, 0.5, 0.5) * 2.0;
                    if d < 1.2
                        && noise::ridged_noise2d(1, x as f64, y as f64, 3, 0.5, 1.0 / 40.0) as f32
                            - d * (1.0 - fract)
                            > 0.16
                    {
                        image.set_raw(x, y, WHITE);
                    }
                }
            }

            // Median filter (Generators' inline variant, 6x6 clamp window).
            let mut output = Pixmap::new(dim, dim);
            let rad = 3i32;
            for y in 0..dim {
                for x in 0..dim {
                    let mut whites = 0;
                    let mut clears = 0;
                    for cx in -rad..rad {
                        for cy in -rad..rad {
                            let wx = (x as i32 + cx).clamp(0, dim as i32 - 1) as usize;
                            let wy = (y as i32 + cy).clamp(0, dim as i32 - 1) as usize;
                            if ai(image.get_raw(wx, wy)) > 127 {
                                whites += 1;
                            } else {
                                clears += 1;
                            }
                        }
                    }
                    output.set_raw(
                        x,
                        y,
                        if whites >= clears {
                            WHITE
                        } else {
                            pixmaps::CLEAR
                        },
                    );
                }
            }
            ctx.atlas
                .save(&output, &format!("rubble/cracks-{size}-{i}"))?;
        }
    }
    Ok(())
}

/// `Generators.scorches` — 10 sizes × 3 via `ScorchGenerator` + median.
///
/// Upstream draws from a time-seeded shared RNG; seeds here are pinned as
/// FNV-1a of the output name (plan 03 §3.5/A3), so output is deterministic.
pub fn scorches(ctx: &mut GenCtx) -> Result<()> {
    for size in 0..10usize {
        for i in 0..3usize {
            let name = format!("scorch-{size}-{i}");
            let mut rand = Rand::new(mind_atlas::hash::seed(&name));
            let mut scorch = ScorchGenerator::default();
            let multiplier = 30.0f64;
            let ss = size as f64 * multiplier / 20.0;

            scorch.seed = rand.random_int_inclusive(100000);
            scorch.size += (size as f64 * multiplier) as i32;
            scorch.scale = scorch.size as f64 / 80.0 * 18.0;
            scorch.octaves += ss / 3.0;
            scorch.pers += ss / 10.0 / 5.0;

            scorch.scale += rand.range(3.0) as f64;
            scorch.scale -= ss * 2.0;
            scorch.nscl -= rand.random(1.0) as f64;

            let out = scorch.generate();
            let median = pixmaps::median(&out, 2, 0.75);
            ctx.atlas.save(&median, &format!("rubble/{name}"))?;
        }
    }
    Ok(())
}

/// `Generators.ScorchGenerator`.
pub struct ScorchGenerator {
    /// Output size in px.
    pub size: i32,
    /// Noise seed.
    pub seed: i32,
    /// Pixel color.
    pub color: u32,
    /// Noise scale.
    pub scale: f64,
    /// Noise exponent.
    pub pow: f64,
    /// Octaves.
    pub octaves: f64,
    /// Persistence.
    pub pers: f64,
    /// Base distance offset.
    pub add: f64,
    /// Angular noise scale.
    pub nscl: f64,
}

impl Default for ScorchGenerator {
    fn default() -> Self {
        Self {
            size: 80,
            seed: 0,
            color: WHITE,
            scale: 18.0,
            pow: 2.0,
            octaves: 4.0,
            pers: 0.4,
            add: 2.0,
            nscl: 4.5,
        }
    }
}

impl ScorchGenerator {
    /// Generates the raw scorch (pre-median).
    pub fn generate(&self) -> Pixmap {
        let size = self.size as usize;
        let mut pix = Pixmap::new(size, size);
        let half = size as f32 / 2.0;
        for y in 0..size {
            for x in 0..size {
                let d = dst(x as f32, y as f32, half, half) / half;
                let mut scaled = ((d - 0.5).abs() * 5.0 + self.add as f32) as f64;
                let ang = mathf::angle(x as f32, y as f32, half, half);
                scaled -= self.noise(ang) * self.nscl;
                if scaled < 1.5 {
                    pix.set_raw(x, y, self.color);
                }
            }
        }
        pix
    }

    fn noise(&self, angle: f32) -> f64 {
        let half = self.size as f32 / 2.0;
        let x = mathf::trnsx(angle, half) + half;
        let y = mathf::trnsy(angle, half) + half;
        f64::from(noise::simplex_noise2d(
            self.seed,
            self.octaves,
            self.pers,
            1.0 / self.scale,
            x as f64,
            y as f64,
        ))
        .powf(self.pow)
    }
}

/// `Generators.shallows`: blend the `liquidBase` over every `floorBase`
/// variant into `<shallow>1..N` and register the result in `gens`.
pub fn shallows(ctx: &mut GenCtx, registry: &ContentRegistry) -> Result<()> {
    for block in registry.blocks().iter() {
        if block.kind != BlockKind::ShallowLiquid {
            continue;
        }
        let meta = metadata::block_meta(&block.name, block.kind);
        let (Some(liquid), Some(floor)) = (meta.shallow_liquid_base, meta.shallow_floor_base)
        else {
            continue;
        };
        let overlay = ctx.atlas.get(liquid)?.as_ref().clone();

        // `floorBase.variantRegions()`: `<floor>1..N`, or `[floor]` when 0.
        let floor_variants = registry
            .block_by_name(floor)
            .map(|def| metadata::block_meta(&def.name, def.kind).variants)
            .unwrap_or(0);
        let regions: Vec<String> = if floor_variants > 0 {
            (1..=floor_variants)
                .map(|i| format!("{floor}{i}"))
                .collect()
        } else {
            vec![floor.to_owned()]
        };

        for (offset, region) in regions.iter().enumerate() {
            let mut res = ctx.atlas.get(region)?.as_ref().clone();
            for y in 0..res.height {
                for x in 0..res.width {
                    let overlay_pixel = (overlay.get_raw(x % overlay.width, y % overlay.height)
                        & 0xffff_ff00)
                        | (meta.shallow_liquid_opacity * 255.0) as u32;
                    res.set_raw(x, y, blend(overlay_pixel, res.get_raw(x, y)));
                }
            }
            let name = format!("{}{}", block.name, offset + 1);
            ctx.atlas
                .save(&res, &format!("blocks/environment/{name}"))?;
            ctx.gens.insert(block.name.clone(), Rc::new(res));
        }
    }
    Ok(())
}

/// `Generators.edges` (content-driven): every `Floor` that is not an
/// overlay, has no explicit edge, no blend group and `drawEdgeOut` gets
/// `<name>-edge` from the `edge-stencil` × its first generated icon (or the
/// `gens` preview).
pub fn edges(ctx: &mut GenCtx, registry: &ContentRegistry) -> Result<()> {
    let edge = ctx
        .atlas
        .get("edge-stencil")
        .context("edges: `edge-stencil` source missing")?;
    for block in registry.blocks().iter() {
        if !metadata::is_floor_kind(block.kind)
            || matches!(
                block.kind,
                BlockKind::OverlayFloor
                    | BlockKind::OreBlock
                    | BlockKind::SpawnBlock
                    | BlockKind::RemoveOre
                    | BlockKind::CharacterOverlay
                    | BlockKind::RuneOverlay
                    | BlockKind::AirBlock
            )
        {
            continue;
        }
        let meta = metadata::block_meta(&block.name, block.kind);
        if !meta.blend_group.is_empty() || !meta.draw_edge_out {
            continue;
        }
        let out_name = format!("{}-edge", block.name);
        if ctx.atlas.has(&out_name) {
            continue;
        }

        let image = if let Some(preview) = ctx.gens.get(&block.name) {
            preview.clone()
        } else {
            let first = {
                let ctx_icons = IconCtx {
                    name: &block.name,
                    region: &block.region,
                    kind: block.kind,
                    size: block.size,
                    variants: meta.variants,
                    has: &|name: &str| ctx.atlas.has(name),
                };
                metadata::generated_icons(&ctx_icons).into_iter().next()
            };
            let Some(first) = first else { continue };
            if !ctx.atlas.has(&first) {
                continue;
            }
            ctx.atlas.get(&first)?
        };

        let mut result = Pixmap::new(edge.width, edge.height);
        for y in 0..edge.height {
            for x in 0..edge.width {
                let stencil = edge.get_raw(x, y);
                let base = image.get_raw(x % image.width, y % image.height);
                result.set_raw(x, y, pixmaps::muli(stencil, base));
            }
        }
        ctx.atlas
            .save(&result, &format!("blocks/environment/{out_name}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_shapes() {
        let ctx_atlas_dir =
            std::env::temp_dir().join(format!("splash-test-{}", std::process::id()));
        std::fs::create_dir_all(&ctx_atlas_dir).unwrap();
        let mut ctx = GenCtx::new(&ctx_atlas_dir).unwrap();
        splashes(&mut ctx).unwrap();
        // Frame 0: radius 0, stroke 3.5 — a small filled disc at the center.
        let first = ctx.atlas.get("splash-0").unwrap();
        assert_eq!(first.get_raw(16, 16) & 0xff, 0xff);
        assert_eq!(first.get_raw(0, 0) & 0xff, 0);
        // Later frames form a ring (center clear again, ring pixels set).
        let ring = ctx.atlas.get("splash-8").unwrap();
        assert_eq!(ring.get_raw(16, 16) & 0xff, 0);
        assert!(ring.pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 0));
        std::fs::remove_dir_all(&ctx_atlas_dir).unwrap();
    }

    #[test]
    fn scorch_generator_deterministic() {
        let scorch = ScorchGenerator {
            seed: 7,
            ..ScorchGenerator::default()
        };
        let a = scorch.generate();
        let b = scorch.generate();
        assert_eq!(a, b);
    }
}

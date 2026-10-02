// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/Generators.java` (`run` driver order) and
// `core/src/mindustry/graphics/BlockRenderer.java` (crack constants),
// `core/src/mindustry/type/Liquid.java` (`animationFrames`).

//! Stage 3 (`generate`) — the `Generators.run()` port (plan 03 §3.5).
//!
//! Pass order matches upstream exactly. Filename-only passes land in M2;
//! content-driven passes (M3) are driven by the metadata contract in
//! [`crate::generate::metadata`]. Unit passes consume the plan 02 M5 metadata
//! (see [`units`]).

use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Instant;

use anyhow::Result;
use mind_atlas::pixmaps::Pixmap;
use mind_core::content::load::ContentRegistry;

use crate::pack_atlas::PackAtlas;

pub mod autotile;
pub mod blocks;
pub mod environment;
pub mod fx;
pub mod icons;
pub mod inventory;
pub mod metadata;
pub mod ore;
pub mod units;

/// `BlockRenderer.maxCrackSize`.
pub const MAX_CRACK_SIZE: usize = 7;
/// `BlockRenderer.crackRegions`.
pub const CRACK_REGIONS: usize = 8;
/// `Liquid.animationFrames`.
pub const ANIMATION_FRAMES: usize = 50;
/// UI icon size cap (`Generators.maxUiIcon`).
pub const MAX_UI_ICON: usize = 128;

/// Shared generator context: the fake atlas plus the in-memory floor-preview
/// map (`Generators.gens`).
pub struct GenCtx {
    /// The staging atlas.
    pub atlas: PackAtlas,
    /// Floor preview pixmaps for the `edges` pass (`Generators.gens`), keyed
    /// by block name.
    pub gens: BTreeMap<String, Rc<Pixmap>>,
    /// Generated pixmaps that live outside the staging tree (written straight
    /// to the output dir by `pack`, e.g. `block_colors`).
    pub extras: BTreeMap<String, Pixmap>,
}

impl GenCtx {
    /// Enumerates the staging tree.
    pub fn new(staging: &std::path::Path) -> Result<GenCtx> {
        Ok(GenCtx {
            atlas: PackAtlas::enumerate(staging)?,
            gens: BTreeMap::new(),
            extras: BTreeMap::new(),
        })
    }
}

/// Generator pass names in upstream `Generators.run()` order.
pub const PASS_ORDER: &[&str] = &[
    "autotiles",
    "splashes",
    "bubbles",
    "gas-frames",
    "cliffs",
    "cracks",
    "block-icons",
    "shallows",
    "item-icons",
    "sector-icons",
    "team-icons",
    "unit-icons",
    "ore-icons",
    "edges",
    "scorches",
];

/// Runs every generator pass in upstream `Generators.run()` order. Returns
/// per-pass timings (seconds).
pub fn run_passes(
    ctx: &mut GenCtx,
    registry: &ContentRegistry,
    log: &mut impl FnMut(&str, f64),
) -> Result<BTreeMap<String, f64>> {
    let mut timings = BTreeMap::new();
    macro_rules! content_pass {
        ($name:literal, $pass:path) => {{
            let start = Instant::now();
            $pass(ctx, registry)?;
            let elapsed = start.elapsed().as_secs_f64();
            timings.insert($name.to_owned(), elapsed);
            log($name, elapsed);
        }};
    }
    macro_rules! plain_pass {
        ($name:literal, $pass:path) => {{
            let start = Instant::now();
            $pass(ctx)?;
            let elapsed = start.elapsed().as_secs_f64();
            timings.insert($name.to_owned(), elapsed);
            log($name, elapsed);
        }};
    }

    plain_pass!("autotiles", autotile::run);
    plain_pass!("splashes", environment::splashes);
    plain_pass!("bubbles", environment::bubbles);
    plain_pass!("gas-frames", fx::gas_frames);
    plain_pass!("cliffs", environment::cliffs);
    plain_pass!("cracks", environment::cracks);
    content_pass!("block-icons", blocks::run);
    content_pass!("shallows", environment::shallows);
    content_pass!("item-icons", icons::item_icons);
    content_pass!("sector-icons", icons::sector_icons);
    content_pass!("team-icons", icons::team_icons);
    content_pass!("unit-icons", units::run);
    content_pass!("ore-icons", ore::run);
    content_pass!("edges", environment::edges);
    plain_pass!("scorches", environment::scorches);
    Ok(timings)
}

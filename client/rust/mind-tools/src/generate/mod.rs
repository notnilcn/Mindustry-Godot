// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/Generators.java` (`run` driver order) and
// `core/src/mindustry/graphics/BlockRenderer.java` (crack constants),
// `core/src/mindustry/type/Liquid.java` (`animationFrames`).

//! Stage 3 (`generate`) — the `Generators.run()` port (plan 03 §3.5).
//!
//! Pass order matches upstream exactly. Filename-only passes land in M2;
//! content-driven passes (M3) are driven by the metadata contract in
//! [`crate::generate::metadata`]. Unit passes are deferred until plan 02 M5
//! metadata merges (lane note, recorded in the plan changelog).

use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Instant;

use anyhow::Result;
use mind_atlas::pixmaps::Pixmap;

use crate::pack_atlas::PackAtlas;

pub mod autotile;
pub mod environment;
pub mod fx;

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
}

impl GenCtx {
    /// Enumerates the staging tree.
    pub fn new(staging: &std::path::Path) -> Result<GenCtx> {
        Ok(GenCtx {
            atlas: PackAtlas::enumerate(staging)?,
            gens: BTreeMap::new(),
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

/// Runs the filename-only passes (M2). Returns per-pass timings (seconds).
pub fn run_filename_passes(
    ctx: &mut GenCtx,
    log: &mut impl FnMut(&str, f64),
) -> Result<BTreeMap<String, f64>> {
    let mut timings = BTreeMap::new();
    let run = |name: &str,
               ctx: &mut GenCtx,
               pass: fn(&mut GenCtx) -> Result<()>,
               timings: &mut BTreeMap<String, f64>,
               log: &mut dyn FnMut(&str, f64)|
     -> Result<()> {
        let start = Instant::now();
        pass(ctx)?;
        let elapsed = start.elapsed().as_secs_f64();
        timings.insert(name.to_owned(), elapsed);
        log(name, elapsed);
        Ok(())
    };
    run("autotiles", ctx, autotile::run, &mut timings, log)?;
    run("splashes", ctx, environment::splashes, &mut timings, log)?;
    run("bubbles", ctx, environment::bubbles, &mut timings, log)?;
    run("gas-frames", ctx, fx::gas_frames, &mut timings, log)?;
    run("cliffs", ctx, environment::cliffs, &mut timings, log)?;
    run("cracks", ctx, environment::cracks, &mut timings, log)?;
    run("edges", ctx, environment::edges, &mut timings, log)?;
    run("scorches", ctx, environment::scorches, &mut timings, log)?;
    Ok(timings)
}

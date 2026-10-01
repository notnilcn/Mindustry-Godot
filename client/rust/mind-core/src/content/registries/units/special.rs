// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/UnitTypes.java (wave `special`).
//
//! Generated unit metadata (`UnitTypes.java`, wave `special`).
//! Regenerate with `parity/tools/gen_units.py` after upstream content changes.

use super::UnitSink;
use crate::content::ContentError;

/// Loads the `special` wave in upstream order.
pub fn load(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    let _ = sink;
    Ok(())
}

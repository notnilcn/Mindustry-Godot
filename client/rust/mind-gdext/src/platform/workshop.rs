// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Steam Workshop seam (plan 22 §3.9, OD4).
//!
//! Steam is deferred (NUD-04/51=A), so every query returns empty. Plans 06/16/19/20
//! compile against these functions; adopting GodotSteam later replaces the bodies
//! without touching call sites. `STEAM_APP_ID` (`SVars.appId`) is kept for the
//! parity documentation.

use std::path::PathBuf;

pub use mind_core::service::STEAM_APP_ID;

/// Whether a Workshop backend is available (always `false` until OD4).
pub fn workshop_available() -> bool {
    false
}

/// Published Workshop maps (`SWorkshop::getMaps`); empty until OD4.
pub fn workshop_maps() -> Vec<PathBuf> {
    Vec::new()
}

/// Published Workshop schematics (`SWorkshop::getSchematics`); empty until OD4.
pub fn workshop_schematics() -> Vec<PathBuf> {
    Vec::new()
}

/// Published Workshop mods (`SWorkshop::getMods`); empty until OD4.
pub fn workshop_mods() -> Vec<PathBuf> {
    Vec::new()
}

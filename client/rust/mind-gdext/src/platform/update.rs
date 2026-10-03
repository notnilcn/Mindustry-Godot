// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Updater disposition (plan 22 §3.10, P22-3).
//!
//! The upstream `BeControl` bleeding-edge updater (GitHub releases, self-replace)
//! is **not ported** (no JVM artifacts). `config autoUpdate` is retained but only
//! logs the notice below; no network call is ever made at boot.

/// The single notice printed when `autoUpdate` is requested.
pub const NOT_AVAILABLE: &str = "updater not available in this build";

/// Whether an update is available (always `false`: no update feed is consulted).
pub fn update_available() -> bool {
    false
}

/// Whether automatic self-updating is supported (always `false`).
pub fn auto_update_supported() -> bool {
    false
}

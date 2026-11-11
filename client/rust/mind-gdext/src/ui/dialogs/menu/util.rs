// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared helpers for the standalone-menu dialogs (moved from the old simple.rs).
//! Upstream: none (port-only; shared helper of the old simple)

/// A rich label's engine handle (for text/color/alignment).
///
/// The local body was byte-identical to `dialogs/common/facade::rich`; the W2
/// H05a adoption re-exports the shared copy instead of keeping a second one.
pub(super) use crate::ui::dialogs::common::facade::rich;

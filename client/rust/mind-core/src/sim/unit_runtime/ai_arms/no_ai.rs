// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `NoAI` dispatch arm.
//! Upstream: core/src/mindustry/ai/types/NoAI.java

/// `NoAI.updateUnit(){}`: the dummy never runs a controller (the
/// kinematics/environment tail still runs, as in Java `Unit.update`).
pub(in crate::sim::unit_runtime) fn no_ai_arm() {}

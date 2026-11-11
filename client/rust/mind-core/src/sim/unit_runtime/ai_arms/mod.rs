// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-AI-kind dispatch arms of [`super::update::unit_update_system`]: one
//! module per `ControllerSlot` kind, each holding the arm body that used to be
//! inline in the dispatch chain.
//!
//! The arms keep the exact statement order (and therefore RNG/ECS ordering) of
//! `UnitComp.update`'s controller call. The chain itself, the kind kinematics,
//! the weapons pass and the shock mines stay in `update.rs`.
//! Upstream: core/src/mindustry/entities/comp/UnitComp.java (update)

use super::*;

pub(super) mod no_ai;
pub(super) mod player;

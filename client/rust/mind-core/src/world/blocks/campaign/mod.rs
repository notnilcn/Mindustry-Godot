// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign block support (`world/blocks/campaign/*`). The per-building state
//! machines live in [`crate::world::behavior::campaign`]; this subtree owns the
//! Interplanetary Accelerator's launch-cost and launch-sequence data shared by
//! the sim launch step, the host pump and the render pass.

pub mod accelerator;

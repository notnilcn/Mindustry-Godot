// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! AI controllers (`ai/types/*`). M0 ships `GroundAI`; M4 adds `CommandAI`; the
//! remaining controllers land with M2/M4.

pub mod command;
pub mod flying;
pub mod ground;

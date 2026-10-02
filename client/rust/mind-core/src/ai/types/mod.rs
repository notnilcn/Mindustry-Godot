// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! AI controllers (`ai/types/*`). M0 shipped `GroundAI`; M4 added `CommandAI`;
//! this milestone completes the remaining 15 controllers plus the `Player`
//! bridge. Controllers that need plan-08/12/13 runtime (payload transfer,
//! schematics, `LUnitControl`) implement the parts available now and mark the
//! rest with an explicit `TODO(plan NN)`.

pub mod assembler;
pub mod boost;
pub mod builder;
pub mod cargo;
pub mod command;
pub mod defender;
pub mod flying;
pub mod flying_follow;
pub mod ground;
pub mod hug;
pub mod logic;
pub mod miner;
pub mod missile;
pub mod no_ai;
pub mod player_bridge;
pub mod prebuild;
pub mod repair;
pub mod suicide;

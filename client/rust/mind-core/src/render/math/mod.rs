// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot-free render math helpers (plan 16 §3.1/§3.7/M7).
//!
//! `voronoi` backs the overlay core-protection edges (`Voronoi.java`);
//! `inverse_kinematics` is the two-segment leg solver used by unit drawing
//! (`InverseKinematics.java`, consumed by plans 11/17). View-only.

pub mod inverse_kinematics;
pub mod voronoi;

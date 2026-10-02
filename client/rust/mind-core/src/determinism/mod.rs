// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Determinism primitives: RNG streams, command log, canonical checksum.
//!
//! Plan 05 §3.11/§6.4/§6.5. The checksum module is the canonical
//! `Checksum` (HLP §12 C2) that replaces the P0 xxh3 dump hash at M8.

pub mod rng;

pub use rng::{ALL_STREAMS, RngStream, SimRng};

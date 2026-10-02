// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Scratch/pooling utilities (plan 05 §3.9).

pub mod alloc;
pub mod pools;
pub mod strings;
pub mod tmp;

pub use alloc::{alloc_bytes, alloc_count};
pub use pools::VecPool;
pub use strings::{sanitize_filename, strip_colors};
pub use tmp::{TempVec, Tmp, with_temp_vec};

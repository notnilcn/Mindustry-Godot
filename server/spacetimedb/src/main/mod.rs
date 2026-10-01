// SPDX-License-Identifier: GPL-3.0-only

//! Module-wide skeleton (plan 01 §3.7): constants, seeds, audit and lifecycle.
//! Subsystem modules (`identity/`, later `relay/`) own their own tables,
//! reducers and views.

pub mod audit;
pub mod global;
pub mod lifecycle;
pub mod seeds;
pub mod tables;

// SPDX-License-Identifier: GPL-3.0-only

//! `mind-tools` — offline asset pipeline (plan 03 §3.5). Library half so the
//! CLI and integration tests share the stage implementations.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod antialias;
pub mod generate;
pub mod migrate;
pub mod pack_atlas;
pub mod pack_pipeline;
pub mod staging;

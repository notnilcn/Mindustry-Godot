// SPDX-License-Identifier: GPL-3.0-only

//! `mind-atlas` — pure-Rust sprite atlas packing library (plan 03 §3.1).
//!
//! Ports the Arc `Pixmap`/`Pixmaps`/`TexturePacker` semantics used by the
//! Mindustry `tools:pack` pipeline into a dependency-light, Godot-free crate.
//! See `03_ASSETS_IMPLEMENTATION_PLAN.md` §3.5 for the stage list.
//!
//! Attribution: pixmap/packer behavior is a behavioral reimplementation of
//! Arc (<https://github.com/Anuken/Arc>, Apache-2.0) revision `7445105cd2`
//! (pinned in `Mindustry/gradle.properties`); the GPL-3.0 Mindustry
//! `ImagePacker`/`Generators` drivers live in `mind-tools`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod atlas_json;
pub mod autotile;
pub mod error;
pub mod hash;
pub mod manifest;
pub mod mathf;
pub mod migrate;
pub mod ninepatch;
pub mod noise;
pub mod pack;
pub mod pack_json;
pub mod page;
pub mod pixmaps;
pub mod png_io;
pub mod vector_table;
pub mod whitespace;

pub use error::AtlasError;

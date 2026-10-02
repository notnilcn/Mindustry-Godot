// SPDX-License-Identifier: GPL-3.0-only

//! `mind-tools` — offline asset pipeline (plan 03 §3.5). Library half so the
//! CLI and integration tests share the stage implementations.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod antialias;
pub mod generate;
pub mod generated_assets;
pub mod migrate;
pub mod pack_atlas;
pub mod pack_pipeline;
pub mod shaders;
pub mod sounds;
pub mod staging;

use anyhow::Result;
use mind_core::content::load::ContentRegistry;
use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

/// Builds the headless vanilla content registry used by the generators
/// (`Vars.content.createBaseContent()` + `init()`), with a memory bundle.
pub fn base_content() -> Result<ContentRegistry> {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true)?;
    registry.init()?;
    registry.post_init()?;
    Ok(registry)
}

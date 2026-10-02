// SPDX-License-Identifier: GPL-3.0-only

//! Godot runtime asset layer (plan 03 §3.6).
//!
//! `MindAssets` (the `res://scenes/autoloads/mind_assets.tscn` autoload) loads
//! the packed atlas manifest + page PNGs, caches `AtlasTexture`s per region,
//! loads loose textures and exposes the `probe(name)` MCP debug API plus the
//! inspector fixture helpers. Stage-machine boot, bundles/fonts/cursors land in
//! later milestones (M7/M8) — see `03_ASSETS_IMPLEMENTATION_PLAN.md`.

mod atlas;
mod audio;
mod bundle;
mod fonts;
mod loader;

pub use atlas::MindAssets;

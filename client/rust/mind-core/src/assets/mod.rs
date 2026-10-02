// SPDX-License-Identifier: GPL-3.0-only

//! Asset runtime layer (plan 03): `FileTree`, atlas index, bundles, icons and
//! sound/music registries — the Godot-free halves of `Core.atlas`/`Vars.tree`.
//!
//! Milestone map (`03_ASSETS_IMPLEMENTATION_PLAN.md` §3.3):
//!
//! * `atlas` (M1) — `Region`/`AtlasIndex`, manifest loading, `find` semantics.
//! * `file_tree` (M5) — mods-first virtual FS (`core/src/mindustry/core/FileTree.java`).
//! * `bundle` (M7) — `.properties` chain + `IntFormat`.
//! * `icons` (M7) — `Iconc`/`Icon` code tables.
//! * `regions` (M6) — `@Load` equivalent (`LoadRegionProcessor`).
//! * `sounds` (M8) — `Sounds`/`Musics` name+id registry.
//!
//! Invariants (§3.9): region names, bundle keys, sound names and icon codes are
//! parity ABI — never reordered, auto-cased or path-qualified. This module tree
//! never reads the filesystem outside an injected `FileTree` and stays
//! Godot-free and tokio-free (HLP §2.2).

pub mod atlas;
pub mod bundle;
pub mod file_tree;
pub mod generated;
pub mod icons;
pub mod regions;
pub mod sounds;

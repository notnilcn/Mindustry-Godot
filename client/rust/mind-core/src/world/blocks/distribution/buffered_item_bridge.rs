// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BufferedItemBridge` (`world/blocks/distribution/BufferedItemBridge.java`) —
//! plan 08 M3. The buffered transport path lives on [`ItemBridgeBuild`]'s
//! optional `buffer` field so one behavior covers both bridge classes; this
//! module exposes the shared type under its upstream build name.

pub use super::item_bridge::{
    ItemBridgeBehavior as BufferedItemBridgeBehavior, ItemBridgeBuild as BufferedItemBridgeBuild,
};

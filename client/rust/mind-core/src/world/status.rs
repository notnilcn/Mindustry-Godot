// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Block status computation (`core/src/mindustry/world/meta/BlockStatus.java`).
//!
//! `BlockStatus` drives the "active" pulse on building icons. The pulse uses
//! `state.tick`; this module keeps the computation Godot-free.

use crate::world::block::BlockView;

/// Display status of a placed block (`BlockStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockStatus {
    /// No status.
    None,
    /// Active (pulsing).
    Active,
    /// No input (red).
    NoInput,
    /// No output (yellow).
    NoOutput,
}

impl BlockStatus {
    /// Selection color for the status (`BlockStatus.color`); 14 reads it.
    pub const fn color(self) -> u32 {
        match self {
            BlockStatus::None => 0xffff_ffff,
            BlockStatus::Active => 0xffff_ffff,
            BlockStatus::NoInput => 0xff1d1d,
            BlockStatus::NoOutput => 0xffd37f,
        }
    }
}

/// Average of the active/valid draw branches (`BlockStatus.forBlock`).
pub fn block_status(
    block: &BlockView<'_>,
    enabled: bool,
    has_items: bool,
    has_liquids: bool,
    power_ok: bool,
) -> BlockStatus {
    if !enabled {
        return BlockStatus::None;
    }
    // `BlockStatus.forBlock` checks item/liquid/power availability.
    if block.has_items() && !has_items {
        return BlockStatus::NoInput;
    }
    if block.has_liquids() && !has_liquids {
        return BlockStatus::NoInput;
    }
    if block.has_power() && !power_ok {
        return BlockStatus::NoInput;
    }
    if block.is_active() {
        BlockStatus::Active
    } else {
        BlockStatus::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_colors_match_upstream() {
        assert_eq!(BlockStatus::NoInput.color(), 0xff1d1d);
        assert_eq!(BlockStatus::NoOutput.color(), 0xffd37f);
    }
}

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
    /// Disabled by logic (`logicDisable`).
    LogicDisable,
    /// Inactive (`inactive`).
    Inactive,
}

impl BlockStatus {
    /// Selection color for the status (`BlockStatus.color`); 14 reads it.
    pub const fn color(self) -> u32 {
        match self {
            BlockStatus::None => 0xffff_ffff,
            BlockStatus::Active => 0xffff_ffff,
            BlockStatus::NoInput => 0xff1d1d,
            BlockStatus::NoOutput => 0xffd37f,
            BlockStatus::LogicDisable => 0x8a73c6,
            BlockStatus::Inactive => 0xbfbfbf,
        }
    }

    /// The Java `BlockStatus` F6-marker colours (`world/meta/BlockStatus.java`)
    /// as `0xRRGGBBAA`: active `#5ce677`, noOutput `Color.orange` `#ffa500`,
    /// noInput `Pal.remove` `#e55454`, logicDisable `#8a73c6`, inactive and
    /// absent status `Color.lightGray` `#bfbfbf`.
    pub const fn java_color(self) -> u32 {
        match self {
            BlockStatus::Active => 0x5ce6_77ff,
            BlockStatus::NoOutput => 0xffa5_00ff,
            BlockStatus::NoInput => 0xe554_54ff,
            BlockStatus::LogicDisable => 0x8a73_c6ff,
            BlockStatus::Inactive | BlockStatus::None => 0xbfbf_bfff,
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

    /// The F6 marker colours are the Java `BlockStatus` values (T14/PAR-0097).
    #[test]
    fn java_marker_colors_match_blockstatus() {
        assert_eq!(BlockStatus::Active.java_color(), 0x5ce6_77ff);
        assert_eq!(BlockStatus::NoOutput.java_color(), 0xffa5_00ff);
        assert_eq!(BlockStatus::NoInput.java_color(), 0xe554_54ff);
        assert_eq!(BlockStatus::LogicDisable.java_color(), 0x8a73_c6ff);
        assert_eq!(BlockStatus::Inactive.java_color(), 0xbfbf_bfff);
        assert_eq!(BlockStatus::None.java_color(), 0xbfbf_bfff);
    }
}

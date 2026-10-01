// SPDX-License-Identifier: GPL-3.0-only

//! Stack value types and ID-indexed count containers.
//!
//! Ported from `core/src/mindustry/type/{ItemStack,LiquidStack,PayloadStack,ItemSeq,PayloadSeq}.java`.
//! `ItemSeq`/`PayloadSeq` arrays grow against `ContentRegistry::arr_epoch` when
//! content is appended (plan 04 calls [`ItemSeq::resize`] after mod content);
//! `add` drops out-of-range items exactly like upstream (`ItemSeq.java:126-130`).

use serde::{Deserialize, Serialize};

use super::{ContentRef, ContentType, ItemId, LiquidId};

/// Item amount pair (`ItemStack`). Defaults to `Items.copper` for deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ItemStack {
    /// Item reference.
    pub item: ItemId,
    /// Amount (may be negative for deltas).
    pub amount: i32,
}

impl Default for ItemStack {
    fn default() -> Self {
        Self {
            item: ItemId::COPPER,
            amount: 0,
        }
    }
}

impl ItemStack {
    /// Builds a stack.
    pub const fn new(item: ItemId, amount: i32) -> Self {
        Self { item, amount }
    }

    /// `ItemStack.set`.
    pub fn set(&mut self, item: ItemId, amount: i32) -> &mut Self {
        self.item = item;
        self.amount = amount;
        self
    }

    /// `ItemStack.copy`.
    pub const fn copy(self) -> Self {
        self
    }

    /// `ItemStack.mult` over a slice (rounds like `Mathf.round`).
    pub fn mult(stacks: &[ItemStack], amount: f32) -> Vec<ItemStack> {
        stacks
            .iter()
            .map(|stack| ItemStack::new(stack.item, round_to_i32(stack.amount as f32 * amount)))
            .collect()
    }
}

/// Liquid amount pair (`LiquidStack`). Defaults to `Liquids.water`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LiquidStack {
    /// Liquid reference.
    pub liquid: LiquidId,
    /// Amount.
    pub amount: f32,
}

impl Default for LiquidStack {
    fn default() -> Self {
        Self {
            liquid: LiquidId::WATER,
            amount: 0.0,
        }
    }
}

impl LiquidStack {
    /// Builds a stack.
    pub const fn new(liquid: LiquidId, amount: f32) -> Self {
        Self { liquid, amount }
    }

    /// `LiquidStack.set`.
    pub fn set(&mut self, liquid: LiquidId, amount: f32) -> &mut Self {
        self.liquid = liquid;
        self.amount = amount;
        self
    }

    /// `LiquidStack.copy`.
    pub const fn copy(self) -> Self {
        self
    }
}

/// Payload amount pair (`PayloadStack`); `item` is a block or unit reference.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PayloadStack {
    /// Block or unit content reference.
    pub item: ContentRef,
    /// Amount.
    pub amount: i32,
}

impl PayloadStack {
    /// Builds a stack, asserting the payload kind at runtime via `Result` at
    /// call sites (constructors accept any block/unit reference).
    pub const fn new(item: ContentRef, amount: i32) -> Self {
        Self { item, amount }
    }

    /// Whether the referenced content is a valid payload kind.
    pub fn is_valid(&self) -> bool {
        matches!(self.item.type_, ContentType::Block | ContentType::Unit)
    }
}

/// `ItemSeq`: dense id-indexed item counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemSeq {
    counts: Vec<i32>,
}

impl ItemSeq {
    /// Empty sequence.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sequence with `len` zeroed slots (content count when constructed).
    pub fn with_len(len: usize) -> Self {
        Self {
            counts: vec![0; len],
        }
    }

    /// Number of slots.
    pub fn len(&self) -> usize {
        self.counts.len()
    }

    /// Whether there are no slots.
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Resizes to hold `len` items, preserving existing counts
    /// (`DataPatcher.fixContentArrayCapacity` calls this via plan 04).
    pub fn resize(&mut self, len: usize) {
        self.counts.resize(len, 0);
    }

    /// Count for an item (out-of-range = 0).
    pub fn get(&self, item: ItemId) -> i32 {
        self.counts.get(item.index()).copied().unwrap_or(0)
    }

    /// `ItemSeq.add`: out-of-range items are ignored (`ItemSeq.java:126-130`).
    pub fn add(&mut self, item: ItemId, amount: i32) {
        if let Some(slot) = self.counts.get_mut(item.index()) {
            *slot = slot.saturating_add(amount);
        }
    }

    /// `ItemSeq.set`.
    pub fn set(&mut self, item: ItemId, amount: i32) {
        if let Some(slot) = self.counts.get_mut(item.index()) {
            *slot = amount;
        }
    }

    /// Sum of all counts.
    pub fn total(&self) -> i64 {
        self.counts.iter().map(|value| i64::from(*value)).sum()
    }

    /// `ItemSeq.add(ItemStack[])`: adds every stack.
    pub fn add_stacks(&mut self, stacks: &[ItemStack]) {
        for stack in stacks {
            self.add(stack.item, stack.amount);
        }
    }

    /// `ItemSeq.toArray`: nonzero counts as stacks, in id order.
    pub fn to_array(&self) -> Vec<ItemStack> {
        self.counts
            .iter()
            .enumerate()
            .filter(|(_, amount)| **amount > 0)
            .map(|(index, amount)| ItemStack::new(ItemId::new(index as u16), *amount))
            .collect()
    }
}

/// `PayloadSeq`: dense id-indexed payload counts (per block/unit space).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PayloadSeq {
    counts: Vec<i32>,
}

impl PayloadSeq {
    /// Empty sequence.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sequence with `len` zeroed slots.
    pub fn with_len(len: usize) -> Self {
        Self {
            counts: vec![0; len],
        }
    }

    /// Number of slots.
    pub fn len(&self) -> usize {
        self.counts.len()
    }

    /// Whether there are no slots.
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Resizes to hold `len` payloads.
    pub fn resize(&mut self, len: usize) {
        self.counts.resize(len, 0);
    }

    /// Count for a raw index.
    pub fn get_index(&self, index: usize) -> i32 {
        self.counts.get(index).copied().unwrap_or(0)
    }

    /// Adds to a raw index; out-of-range payloads are ignored.
    pub fn add_index(&mut self, index: usize, amount: i32) {
        if let Some(slot) = self.counts.get_mut(index) {
            *slot = slot.saturating_add(amount);
        }
    }
}

/// `Mathf.round` (half-up for positive values, half-away-from-zero otherwise).
pub fn round_to_i32(value: f32) -> i32 {
    if value >= 0.0 {
        (value + 0.5) as i32
    } else {
        (value - 0.5) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_seq_ignores_out_of_range() {
        let mut seq = ItemSeq::with_len(3);
        seq.add(ItemId::new(0), 5);
        seq.add(ItemId::new(2), 2);
        seq.add(ItemId::new(9), 100);
        assert_eq!(seq.get(ItemId::new(0)), 5);
        assert_eq!(seq.get(ItemId::new(1)), 0);
        assert_eq!(seq.get(ItemId::new(2)), 2);
        assert_eq!(seq.get(ItemId::new(9)), 0);
        assert_eq!(seq.total(), 7);
        // Growth preserves existing counts.
        seq.resize(5);
        assert_eq!(seq.get(ItemId::new(0)), 5);
        assert_eq!(seq.len(), 5);
    }

    #[test]
    fn stack_helpers_round_like_mathf() {
        let stacks = [ItemStack::new(ItemId::new(0), 3)];
        assert_eq!(ItemStack::mult(&stacks, 1.5)[0].amount, 5);
        assert_eq!(ItemStack::mult(&stacks, 0.5)[0].amount, 2);
        assert_eq!(round_to_i32(-1.5), -2);
    }
}

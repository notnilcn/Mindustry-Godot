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

/// `PayloadSeq` (Java `mindustry.type.PayloadSeq`): an insertion-ordered
/// content→count container for block/unit payloads, with Java's save format
/// (negated entry count, then per entry `b type ordinal`, `s id`, `i amount`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PayloadSeq {
    entries: indexmap::IndexMap<ContentRef, i32>,
    total: i32,
}

impl PayloadSeq {
    /// Empty sequence.
    pub fn new() -> Self {
        Self::default()
    }

    /// `PayloadSeq.isEmpty`.
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// `PayloadSeq.any`.
    pub fn any(&self) -> bool {
        self.total > 0
    }

    /// `PayloadSeq.total`.
    pub fn total(&self) -> i32 {
        self.total
    }

    /// `PayloadSeq.add`: increments the entry and the total (`null` content is
    /// a no-op upstream; the port never passes one).
    pub fn add(&mut self, content: ContentRef, amount: i32) {
        *self.entries.entry(content).or_insert(0) += amount;
        self.total += amount;
    }

    /// `PayloadSeq.remove`.
    pub fn remove(&mut self, content: ContentRef, amount: i32) {
        self.add(content, -amount);
    }

    /// `PayloadSeq.clear`.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.total = 0;
    }

    /// `PayloadSeq.get`.
    pub fn get(&self, content: ContentRef) -> i32 {
        self.entries.get(&content).copied().unwrap_or(0)
    }

    /// `PayloadSeq.contains(block, amount)`.
    pub fn contains(&self, content: ContentRef, amount: i32) -> bool {
        self.get(content) >= amount
    }

    /// Serialized (nonzero or not) entries in insertion order.
    pub fn entries(&self) -> impl Iterator<Item = (ContentRef, i32)> + '_ {
        self.entries
            .iter()
            .map(|(content, amount)| (*content, *amount))
    }

    /// `PayloadSeq.write`: `s -size`, then `b contentType.ordinal()`,
    /// `s id`, `i amount` per entry. The negated count marks the new format
    /// (the `>= 0` arm is the legacy block-only list).
    pub fn write(&self, w: &mut crate::io::wire::WireWriter) {
        w.s(-(self.entries.len() as i16));
        for (content, amount) in &self.entries {
            w.b(content.type_.ordinal() as i8);
            w.s(content.id as i16);
            w.i(*amount);
        }
    }

    /// `PayloadSeq.read`: replaces the sequence. The legacy (`>= 0`) arm holds
    /// block ids only; the new format carries the content-type ordinal. Ids are
    /// kept raw like the rest of the port's save readers (mod id mapping is the
    /// temporary mapper's job elsewhere).
    pub fn read(&mut self, r: &mut crate::io::wire::WireReader) -> crate::io::IoResult<()> {
        self.clear();
        let amount = r.s()?;
        if amount >= 0 {
            for _ in 0..amount {
                let id = r.s()? as u16;
                let count = r.i()?;
                self.add(ContentRef::new(ContentType::Block, id), count);
            }
        } else {
            for _ in 0..(-i32::from(amount)) {
                let ordinal = r.ub()? as usize;
                let id = r.s()? as u16;
                let count = r.i()?;
                if let Some(type_) = ContentType::ALL.get(ordinal).copied() {
                    self.add(ContentRef::new(type_, id), count);
                }
            }
        }
        Ok(())
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

    /// I01 T11: `PayloadSeq` round-trips through the negated-count format and
    /// still reads the legacy block-only list (`PayloadSeq.java:70-95`).
    #[test]
    fn payload_seq_round_trips_and_reads_the_legacy_format() {
        use crate::io::wire::{WireReader, WireWriter};

        let mut seq = PayloadSeq::new();
        let unit = ContentRef::new(ContentType::Unit, 7);
        let block = ContentRef::new(ContentType::Block, 3);
        seq.add(unit, 4);
        seq.add(block, 2);
        seq.add(block, 1);
        assert_eq!(seq.get(unit), 4);
        assert_eq!(seq.get(block), 3);
        assert_eq!(seq.total(), 7);

        let mut bytes = Vec::new();
        {
            let mut w = WireWriter::new(&mut bytes);
            seq.write(&mut w);
        }
        // Header: the negated entry count (`s -size`).
        assert_eq!(i16::from_be_bytes([bytes[0], bytes[1]]), -2);
        let mut r = WireReader::new(&bytes);
        let mut decoded = PayloadSeq::new();
        decoded.read(&mut r).expect("read");
        assert_eq!(decoded, seq);

        // Legacy format: `s amount >= 0`, then block id + amount pairs.
        let mut legacy = Vec::new();
        {
            let mut w = WireWriter::new(&mut legacy);
            w.s(1);
            w.s(3);
            w.i(5);
        }
        let mut r = WireReader::new(&legacy);
        let mut decoded = PayloadSeq::new();
        decoded.read(&mut r).expect("read legacy");
        assert_eq!(decoded.get(ContentRef::new(ContentType::Block, 3)), 5);
        assert_eq!(decoded.total(), 5);
    }
}

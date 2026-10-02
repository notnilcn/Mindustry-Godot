// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Time-delayed item queues (`world/ItemBuffer.java`,
//! `world/DirectionalItemBuffer.java`) — plan 08 §3.5.
//!
//! Ported exactly: the packed `@Struct` bit layouts (plan 08 L4), the f32
//! `Time.time` comparison (plan 08 L5) and the legacy 1-byte item reads.
//! Capacities are preallocated once at building creation and retained across
//! pooling (plan 08 L3); the hot `poll`/`accept`/`remove` paths never allocate.

use crate::content::ItemId;
use crate::io::IoError;
use crate::io::entity::{EntityReader, EntityWriter};

/// Packed `BufferItem { short item; float time; }` (bits 0..16, 16..48).
pub mod packed {
    use crate::content::ItemId;

    /// `BufferItem.get(item, time)`.
    pub fn buffer_item(item: ItemId, time: f32) -> u64 {
        item.raw() as u64 | ((time.to_bits() as u64) << 16)
    }

    /// `BufferItem.item`.
    pub fn buffer_item_item(value: u64) -> ItemId {
        ItemId::new((value & 0xffff) as u16)
    }

    /// `BufferItem.time`.
    pub fn buffer_item_time(value: u64) -> f32 {
        f32::from_bits(((value >> 16) & 0xffff_ffff) as u32)
    }

    /// `TimeItem.get(data, item, time)` (bits 0..16, 16..32, 32..64).
    pub fn time_item(data: i16, item: ItemId, time: f32) -> u64 {
        (data as u16 as u64) | ((item.raw() as u64) << 16) | ((time.to_bits() as u64) << 32)
    }

    /// `TimeItem.data`.
    pub fn time_item_data(value: u64) -> i16 {
        (value & 0xffff) as u16 as i16
    }

    /// `TimeItem.item`.
    pub fn time_item_item(value: u64) -> ItemId {
        ItemId::new(((value >> 16) & 0xffff) as u16)
    }

    /// `TimeItem.time`.
    pub fn time_item_time(value: u64) -> f32 {
        f32::from_bits(((value >> 32) & 0xffff_ffff) as u32)
    }

    /// Converts a legacy `BufferItemLegacy { byte item; float time; }` value into
    /// the current 2-byte `BufferItem` layout (`DirectionalItemBuffer.read`).
    pub fn legacy_buffer_item(value: u64) -> u64 {
        let item = ItemId::new((value & 0xff) as u16);
        let time = f32::from_bits(((value >> 8) & 0xffff_ffff) as u32);
        buffer_item(item, time)
    }
}

/// `ItemBuffer` — an insertion-ordered queue of `(item, time)` entries with a
/// head index (no memmove on `poll`; `remove` shifts only on consumption).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemBuffer {
    buffer: Vec<u64>,
    index: usize,
}

impl ItemBuffer {
    /// `new ItemBuffer(capacity)`.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0; capacity],
            index: 0,
        }
    }

    /// `accepts()`.
    pub fn accepts(&self) -> bool {
        self.index < self.buffer.len()
    }

    /// Number of queued entries (`index`).
    pub fn len(&self) -> usize {
        self.index
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.index == 0
    }

    /// Allocated capacity (`buffer.length`).
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// `accept(Item, short)`; the `data` field is only read by the legacy
    /// `OverflowGate` path.
    pub fn accept(&mut self, item: ItemId, data: i16, now: f32) {
        if !self.accepts() {
            return;
        }
        self.buffer[self.index] = packed::time_item(data, item, now);
        self.index += 1;
    }

    /// `poll(speed)` — releases the head once `now >= time + speed` or on the
    /// f32 wraparound branch (`now < time`).
    pub fn poll(&self, speed: f32, now: f32) -> Option<ItemId> {
        if self.index > 0 {
            let value = self.buffer[0];
            let time = packed::time_item_time(value);
            if now >= time + speed || now < time {
                return Some(packed::time_item_item(value));
            }
        }
        None
    }

    /// `remove()` — drops the head and shifts the queue down.
    pub fn remove(&mut self) {
        if self.index == 0 {
            return;
        }
        self.buffer.copy_within(1..self.index, 0);
        self.index -= 1;
    }

    /// Clears the queue without dropping capacity (`reset`/pool reuse).
    pub fn clear(&mut self) {
        self.index = 0;
    }

    /// `write(Writes)` — writes the head index, the allocation length and every
    /// slot (allocation state is part of the save ABI).
    pub fn write(&self, w: &mut EntityWriter) {
        w.b(self.index as i8);
        w.b(self.buffer.len() as i8);
        for value in &self.buffer {
            w.l(*value as i64);
        }
    }

    /// `read(Reads)`.
    pub fn read(&mut self, r: &mut EntityReader) -> Result<(), IoError> {
        let index = r.b()? as i32;
        let length = r.b()? as i32;
        for i in 0..length {
            let value = r.l()? as u64;
            if let Some(slot) = self.buffer.get_mut(i as usize) {
                *slot = value;
            }
        }
        self.index = index.min(length - 1).max(0) as usize;
        Ok(())
    }
}

/// `DirectionalItemBuffer` — four independent `ItemBuffer` lanes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DirectionalItemBuffer {
    /// Per-direction packed queues (indexed by rotation `0..4`).
    pub buffers: [Vec<u64>; 4],
    /// Per-direction queue length.
    pub indexes: [u8; 4],
}

impl DirectionalItemBuffer {
    /// `new DirectionalItemBuffer(capacity)`.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffers: std::array::from_fn(|_| vec![0; capacity]),
            indexes: [0; 4],
        }
    }

    /// `accepts(buffer)`.
    pub fn accepts(&self, buffer: usize) -> bool {
        (self.indexes[buffer] as usize) < self.buffers[buffer].len()
    }

    /// `accept(buffer, Item)`.
    pub fn accept(&mut self, buffer: usize, item: ItemId, now: f32) {
        if !self.accepts(buffer) {
            return;
        }
        let index = self.indexes[buffer] as usize;
        self.buffers[buffer][index] = packed::buffer_item(item, now);
        self.indexes[buffer] += 1;
    }

    /// `poll(buffer, speed)`.
    pub fn poll(&self, buffer: usize, speed: f32, now: f32) -> Option<ItemId> {
        if self.indexes[buffer] > 0 {
            let value = self.buffers[buffer][0];
            let time = packed::buffer_item_time(value);
            if now >= time + speed || now < time {
                return Some(packed::buffer_item_item(value));
            }
        }
        None
    }

    /// `remove(buffer)`.
    pub fn remove(&mut self, buffer: usize) {
        let index = self.indexes[buffer] as usize;
        if index == 0 {
            return;
        }
        self.buffers[buffer].copy_within(1..index, 0);
        self.indexes[buffer] -= 1;
    }

    /// `write(Writes)`.
    pub fn write(&self, w: &mut EntityWriter) {
        for i in 0..4 {
            w.b(self.indexes[i] as i8);
            w.b(self.buffers[i].len() as i8);
            for value in &self.buffers[i] {
                w.l(*value as i64);
            }
        }
    }

    /// `read(Reads)`.
    pub fn read(&mut self, r: &mut EntityReader) -> Result<(), IoError> {
        self.read_inner(r, false)
    }

    /// `read(Reads, legacy)` — converts 1-byte-item entries to the 2-byte layout.
    pub fn read_legacy(&mut self, r: &mut EntityReader) -> Result<(), IoError> {
        self.read_inner(r, true)
    }

    fn read_inner(&mut self, r: &mut EntityReader, legacy: bool) -> Result<(), IoError> {
        for i in 0..4 {
            self.indexes[i] = r.b()? as u8;
            let length = r.b()? as i32;
            for j in 0..length {
                let mut value = r.l()? as u64;
                if legacy {
                    value = packed::legacy_buffer_item(value);
                }
                if let Some(slot) = self.buffers[i].get_mut(j as usize) {
                    *slot = value;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::wire::{WireReader, WireWriter};

    #[test]
    fn pack_literals_match_java_structs() {
        // BufferItem: item bits 0..16, time bits 16..48.
        assert_eq!(
            packed::buffer_item(ItemId::new(5), 2.0),
            0x0000_4000_0000_0005
        );
        assert_eq!(
            packed::buffer_item_item(0x0000_4000_0000_0005),
            ItemId::new(5)
        );
        assert_eq!(packed::buffer_item_time(0x0000_4000_0000_0005), 2.0);

        // TimeItem: data 0..16, item 16..32, time 32..64.
        let value = packed::time_item(-1, ItemId::new(3), 1.0);
        assert_eq!(value, 0x3f80_0000_0003_ffff);
        assert_eq!(packed::time_item_data(value), -1);
        assert_eq!(packed::time_item_item(value), ItemId::new(3));
        assert_eq!(packed::time_item_time(value), 1.0);

        // Legacy: byte item 0..8, time 8..40 -> upgraded to BufferItem.
        assert_eq!(
            packed::legacy_buffer_item(0x40_0000_0005),
            0x0000_4000_0000_0005
        );
    }

    #[test]
    fn poll_latency_and_wraparound() {
        let mut buffer = ItemBuffer::new(2);
        let copper = ItemId::new(1);
        assert!(buffer.accepts());
        buffer.accept(copper, -1, 10.0);
        buffer.accept(ItemId::new(2), -1, 10.0);
        assert_eq!(buffer.len(), 2);
        assert!(!buffer.accepts());

        // Not yet released at now == time.
        assert_eq!(buffer.poll(26.0, 10.0), None);
        // Released once now >= time + speed.
        assert_eq!(buffer.poll(26.0, 36.0), Some(copper));
        // Wraparound branch: now < time.
        buffer.remove();
        assert_eq!(buffer.poll(26.0, 5.0), Some(ItemId::new(2)));
    }

    #[test]
    fn buffer_write_read_roundtrip() {
        let mut buffer = ItemBuffer::new(4);
        buffer.accept(ItemId::new(7), 42, 3.5);
        buffer.accept(ItemId::new(9), -1, 4.0);
        let mut bytes = Vec::new();
        {
            let mut writer = WireWriter::new(&mut bytes);
            buffer.write(&mut writer);
        }
        let mut other = ItemBuffer::new(4);
        let mut reader = WireReader::new(&bytes);
        other.read(&mut reader).expect("read");
        assert_eq!(other.len(), 2);
        assert_eq!(other.buffer[0], packed::time_item(42, ItemId::new(7), 3.5));
        assert_eq!(other.buffer[1], packed::time_item(-1, ItemId::new(9), 4.0));
    }

    #[test]
    fn directional_legacy_read_upgrades_items() {
        // Write a legacy-format blob by hand: one lane with one BufferItemLegacy.
        let legacy = 0x40_0000_0005u64;
        let mut bytes = Vec::new();
        {
            let mut writer = WireWriter::new(&mut bytes);
            // lane 0: index 0, length 1, value
            writer.b(0);
            writer.b(1);
            writer.l(legacy as i64);
            // lanes 1..3 empty
            for _ in 1..4 {
                writer.b(0);
                writer.b(0);
            }
        }
        let mut buffer = DirectionalItemBuffer::new(4);
        let mut reader = WireReader::new(&bytes);
        buffer.read_legacy(&mut reader).expect("legacy read");
        assert_eq!(buffer.indexes[0], 0);
        assert_eq!(
            buffer.buffers[0][0],
            packed::buffer_item(ItemId::new(5), 2.0)
        );
    }

    #[test]
    fn directional_accept_poll_remove() {
        let mut buffer = DirectionalItemBuffer::new(2);
        buffer.accept(1, ItemId::new(1), 0.0);
        buffer.accept(1, ItemId::new(2), 0.0);
        assert!(!buffer.accepts(1));
        assert_eq!(buffer.poll(1, 26.0, 0.0), None);
        assert_eq!(buffer.poll(1, 26.0, 26.0), Some(ItemId::new(1)));
        buffer.remove(1);
        assert_eq!(buffer.indexes[1], 1);
        assert_eq!(
            buffer.buffers[1][0],
            packed::buffer_item(ItemId::new(2), 0.0)
        );
    }
}

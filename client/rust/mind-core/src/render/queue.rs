// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Stable `Draw` queue (plan 16 §3.3 step 25 / §6.2).
//!
//! Mirrors Arc's `Draw` queue semantics: commands are emitted in order with
//! their `z`; when `sort` is enabled the queue is **stable**-sorted by `z` only,
//! so equal-`z` commands keep emission order. `Draw.drawRange` is represented by
//! explicit range brackets so the Godot renderer can bind a band material for
//! `blockbuild`/shields/buildBeam.

use crate::render::commands::DrawCmd;

/// A queued item: a command, or a shader-range bracket.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum QueueItem {
    /// A draw command.
    Cmd(DrawCmd),
    /// `Draw.drawRange(z, begin, …)` start.
    RangeBegin,
    /// `Draw.drawRange(…, end)` end.
    RangeEnd,
}

/// One queue entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QueueEntry {
    /// Layer z.
    pub z: f32,
    /// Monotonic emission sequence (tie-break within equal `z`).
    pub seq: u32,
    /// Payload.
    pub item: QueueItem,
}

/// The stable render queue.
#[derive(Debug, Default)]
pub struct RenderQueue {
    entries: Vec<QueueEntry>,
    next_seq: u32,
    sort: bool,
    dirty: bool,
}

impl RenderQueue {
    /// Empty queue with sorting disabled (`Draw.sort(false)`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears for the next frame (`Draw.reset`).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.next_seq = 0;
        self.dirty = false;
    }

    /// Enables/disables z sorting (`Draw.sort`). Enabling marks the queue dirty.
    pub fn set_sort(&mut self, sort: bool) {
        if self.sort != sort {
            self.sort = sort;
            self.dirty = true;
        }
    }

    /// Whether sorting is enabled.
    pub fn sort_enabled(&self) -> bool {
        self.sort
    }

    /// Pushes a command at `z`.
    pub fn push(&mut self, z: f32, cmd: DrawCmd) {
        self.push_item(z, QueueItem::Cmd(cmd));
    }

    /// Pushes an arbitrary item.
    pub fn push_item(&mut self, z: f32, item: QueueItem) {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        self.entries.push(QueueEntry { z, seq, item });
    }

    /// `Draw.drawRange(z, begin)`: starts a material bracket.
    pub fn range_begin(&mut self, z: f32) {
        self.push_item(z, QueueItem::RangeBegin);
    }

    /// `Draw.drawRange(z, end)`: ends a material bracket.
    pub fn range_end(&mut self, z: f32) {
        self.push_item(z, QueueItem::RangeEnd);
    }

    /// Sorts in place when enabled. Stable, so equal-`z` emission order holds.
    pub fn flush(&mut self) {
        if self.sort && self.dirty {
            self.entries.sort_by(|a, b| {
                a.z.partial_cmp(&b.z)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.seq.cmp(&b.seq))
            });
            self.dirty = false;
        }
    }

    /// The (possibly sorted) entries. Call [`flush`](Self::flush) first.
    pub fn entries(&self) -> &[QueueEntry] {
        &self.entries
    }

    /// Number of queued items.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Highest emission sequence used this frame (`queue_max` stat).
    pub fn max_seq(&self) -> u32 {
        self.next_seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::commands::Blend;

    fn z(cmd: &DrawCmd) -> f32 {
        match cmd {
            DrawCmd::SetZ(z) => *z,
            _ => f32::NAN,
        }
    }

    #[test]
    fn stable_within_equal_z() {
        let mut queue = RenderQueue::new();
        queue.set_sort(true);
        // Two commands at z=1 (A then B) and one at z=0.
        queue.push(1.0, DrawCmd::SetBlend(Blend::Normal));
        queue.push(0.0, DrawCmd::SetBlend(Blend::Additive));
        queue.push(1.0, DrawCmd::SetBlend(Blend::Multiply));
        queue.flush();
        let items: Vec<Blend> = queue
            .entries()
            .iter()
            .map(|e| match e.item {
                QueueItem::Cmd(DrawCmd::SetBlend(b)) => b,
                _ => Blend::Disabled,
            })
            .collect();
        // z=0 first, then the two z=1 in emission order.
        assert_eq!(items, [Blend::Additive, Blend::Normal, Blend::Multiply]);
    }

    #[test]
    fn sort_disabled_preserves_emission() {
        let mut queue = RenderQueue::new();
        queue.push(5.0, DrawCmd::SetZ(5.0));
        queue.push(1.0, DrawCmd::SetZ(1.0));
        queue.push(3.0, DrawCmd::SetZ(3.0));
        queue.flush();
        let zs: Vec<f32> = queue
            .entries()
            .iter()
            .map(|e| match e.item {
                QueueItem::Cmd(cmd) => z(&cmd),
                _ => f32::NAN,
            })
            .collect();
        assert_eq!(zs, [5.0, 1.0, 3.0]);
    }

    #[test]
    fn range_shader_brackets() {
        let mut queue = RenderQueue::new();
        queue.range_begin(40.0);
        queue.push(40.0, DrawCmd::SetBlend(Blend::Normal));
        queue.range_end(40.0);
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.entries()[0].item, QueueItem::RangeBegin);
        assert_eq!(
            queue.entries()[2].item,
            QueueItem::RangeEnd,
            "range brackets stay in emission order"
        );
    }

    #[test]
    fn seq_is_monotonic() {
        let mut queue = RenderQueue::new();
        queue.push(0.0, DrawCmd::SetAlpha(1.0));
        queue.push(0.0, DrawCmd::SetAlpha(0.5));
        assert_eq!(queue.entries()[0].seq, 0);
        assert_eq!(queue.entries()[1].seq, 1);
        assert_eq!(queue.max_seq(), 2);
    }
}

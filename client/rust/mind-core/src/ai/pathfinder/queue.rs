// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PathfindQueue`: exact int-node/float-weight binary heap (plan 11 §3.7).
//!
//! Ported from `core/src/mindustry/ai/PathfindQueue.java`. Ties are broken by
//! node index so the pop order is deterministic (`f32::total_cmp` avoids NaN
//! ordering surprises).

use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// One heap entry (min-heap by weight, then node).
#[derive(Debug, Clone, Copy)]
struct Entry {
    weight: f32,
    node: usize,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Entry {}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        // `BinaryHeap` is a max-heap; invert so the smallest weight pops first.
        other
            .weight
            .total_cmp(&self.weight)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Deterministic binary heap of `(node, weight)`.
#[derive(Debug, Clone, Default)]
pub struct PathfindQueue {
    heap: BinaryHeap<Entry>,
}

impl PathfindQueue {
    /// Creates an empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node with its path weight.
    pub fn add(&mut self, node: usize, weight: f32) {
        self.heap.push(Entry { weight, node });
    }

    /// Peeks the lowest-weight node without removing it.
    pub fn peek(&self) -> Option<(usize, f32)> {
        self.heap.peek().map(|entry| (entry.node, entry.weight))
    }

    /// Removes and returns the lowest-weight node.
    pub fn poll(&mut self) -> Option<(usize, f32)> {
        self.heap.pop().map(|entry| (entry.node, entry.weight))
    }

    /// Empties the queue.
    pub fn clear(&mut self) {
        self.heap.clear();
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Number of queued entries.
    pub fn len(&self) -> usize {
        self.heap.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pops_lowest_weight_first_with_stable_ties() {
        let mut queue = PathfindQueue::new();
        queue.add(5, 3.0);
        queue.add(1, 1.0);
        queue.add(2, 1.0);
        queue.add(7, 0.5);
        assert_eq!(queue.poll(), Some((7, 0.5)));
        assert_eq!(queue.poll(), Some((1, 1.0)), "lower node wins a tie");
        assert_eq!(queue.poll(), Some((2, 1.0)));
        assert_eq!(queue.poll(), Some((5, 3.0)));
        assert_eq!(queue.poll(), None);
        assert!(queue.is_empty());
    }
}

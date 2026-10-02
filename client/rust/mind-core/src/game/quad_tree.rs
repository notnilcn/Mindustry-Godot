// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic spatial index fallback for `TeamData` trees (plan 12 R3).
//!
//! Plan 09 was expected to ship `world::spatial::quad_tree` (Arc `QuadTree`);
//! it did not, so plan 12 provides this insertion-stable, allocation-friendly
//! fallback behind the same role. It has no comparator objects and no hash
//! iteration: queries scan the insertion-ordered slab, so results are
//! deterministic regardless of platform. Query order is insertion order (stable
//! tie-break: first inserted wins `get_closest`).

/// A deterministic, insertion-ordered point index.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuadTree<T> {
    items: Vec<(f32, f32, T)>,
}

impl<T: Copy> QuadTree<T> {
    /// Creates an empty index.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Removes every item, retaining capacity (`QuadTree.clear`).
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Number of indexed items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Inserts an item at `(x, y)` (`QuadTree.insert`).
    pub fn insert(&mut self, x: f32, y: f32, item: T) {
        self.items.push((x, y, item));
    }

    /// Collects every item in insertion order (`QuadTree.getObjects`).
    pub fn get_objects(&self) -> Vec<T> {
        self.items.iter().map(|(_, _, item)| *item).collect()
    }

    /// Nearest item within `max_distance` (squared distances compared), or
    /// `None`. Ties keep the first-inserted item.
    pub fn get_closest(&self, x: f32, y: f32, max_distance: f32) -> Option<T> {
        let max2 = max_distance * max_distance;
        let mut best: Option<(f32, T)> = None;
        for (ix, iy, item) in &self.items {
            let dx = ix - x;
            let dy = iy - y;
            let dist2 = dx * dx + dy * dy;
            if dist2 <= max2 && best.is_none_or(|(best_dist, _)| dist2 < best_dist) {
                best = Some((dist2, *item));
            }
        }
        best.map(|(_, item)| item)
    }

    /// Items within `radius` of `(x, y)`, in insertion order.
    pub fn within(&self, x: f32, y: f32, radius: f32) -> Vec<T> {
        let r2 = radius * radius;
        self.items
            .iter()
            .filter(|(ix, iy, _)| {
                let dx = ix - x;
                let dy = iy - y;
                dx * dx + dy * dy <= r2
            })
            .map(|(_, _, item)| *item)
            .collect()
    }

    /// Items whose point falls inside the axis-aligned rectangle
    /// `(x, y, width, height)` (`QuadTree.intersect`), in insertion order.
    pub fn intersect(&self, x: f32, y: f32, width: f32, height: f32) -> Vec<T> {
        self.items
            .iter()
            .filter(|(ix, iy, _)| *ix >= x && *ix <= x + width && *iy >= y && *iy <= y + height)
            .map(|(_, _, item)| *item)
            .collect()
    }

    /// Read-only view of the slots, for serialization/debug.
    pub fn entries(&self) -> &[(f32, f32, T)] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_are_insertion_stable() {
        let mut tree = QuadTree::new();
        tree.insert(0.0, 0.0, 1u32);
        tree.insert(5.0, 5.0, 2u32);
        tree.insert(0.5, 0.5, 3u32);
        assert_eq!(tree.len(), 3);
        assert_eq!(tree.get_objects(), vec![1, 2, 3]);
        // Closest: tie between 1 and 3 (dist 0.5 vs 0.707) -> 3.
        assert_eq!(tree.get_closest(0.6, 0.6, 100.0), Some(3));
        // Out of range.
        assert_eq!(tree.get_closest(100.0, 100.0, 1.0), None);
        assert_eq!(tree.within(0.0, 0.0, 1.0), vec![1, 3]);
        assert_eq!(tree.intersect(4.0, 4.0, 2.0, 2.0), vec![2]);
        tree.clear();
        assert!(tree.is_empty());
    }
}

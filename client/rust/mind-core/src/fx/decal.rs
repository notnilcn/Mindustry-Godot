// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Decal pool (plan 17 §3.10). Ported from `entities/Effect.java`'s
//! `decal`/`scorch`/`rubble` and `entities/comp/DecalComp.java`.
//!
//! Deliberate deviation #6: decals are capped (oldest dropped) to bound draw
//! batches; upstream is pooled/unbounded.

use crate::content::Rgba;
use crate::math::curve;
use crate::render::draw::RegionKey;
use crate::render::layer::Layer;

use super::pool_spec::{DECAL_CAPACITY, DECAL_FADE_START, DECAL_LIFETIME};

/// One decal instance (`DecalComp`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decal {
    /// Region.
    pub region: RegionKey,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Lifetime in ticks (default 3600).
    pub lifetime: f32,
    /// Elapsed ticks.
    pub time: f32,
    /// Tint.
    pub color: Rgba,
    /// Whether the slot is alive.
    pub alive: bool,
}

impl Decal {
    /// `DecalComp` default.
    pub fn new(region: RegionKey, x: f32, y: f32, rotation: f32, color: Rgba) -> Self {
        Self {
            region,
            x,
            y,
            rotation,
            lifetime: DECAL_LIFETIME,
            time: 0.0,
            color,
            alive: true,
        }
    }

    /// `DecalComp.draw`: `alpha(1 - curve(fin, 0.98))`.
    pub fn alpha(&self) -> f32 {
        if self.lifetime == 0.0 {
            0.0
        } else {
            1.0 - curve(self.time / self.lifetime, DECAL_FADE_START, 1.0)
        }
    }

    /// The `Layer::Scorch` z.
    pub const LAYER: f32 = Layer::Scorch.z();
}

/// Fixed-capacity oldest-first decal pool.
#[derive(Debug)]
pub struct DecalPool {
    slots: Vec<Decal>,
    free: Vec<usize>,
    live: std::collections::VecDeque<usize>,
    capacity: usize,
    /// Number dropped by the cap.
    pub dropped: u64,
    /// Total spawned.
    pub spawned: u64,
}

impl Default for DecalPool {
    fn default() -> Self {
        Self::new(DECAL_CAPACITY)
    }
}

impl DecalPool {
    /// Builds a pool with the given capacity.
    pub fn new(capacity: usize) -> Self {
        let mut slots = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots.push(Decal::new(RegionKey::ERROR, 0.0, 0.0, 0.0, Rgba::WHITE));
        }
        Self {
            slots,
            free: (0..capacity).rev().collect(),
            live: std::collections::VecDeque::with_capacity(capacity),
            capacity,
            dropped: 0,
            spawned: 0,
        }
    }

    /// Number of live decals.
    pub fn live_count(&self) -> usize {
        self.live.len()
    }

    /// Capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Iterates live decals in insertion order.
    pub fn iter_live(&self) -> impl Iterator<Item = &Decal> {
        self.live.iter().map(move |&i| &self.slots[i])
    }

    /// Adds a decal, dropping the oldest when full.
    #[allow(clippy::collapsible_if)]
    pub fn spawn(&mut self, decal: Decal) -> usize {
        self.spawned += 1;
        // Drop oldest when the pool or free list is exhausted.
        if self.free.is_empty() {
            if let Some(oldest) = self.live.pop_front() {
                self.slots[oldest].alive = false;
                self.free.push(oldest);
            }
        }
        let slot = match self.free.pop() {
            Some(slot) => slot,
            None => {
                self.dropped += 1;
                return usize::MAX;
            }
        };
        self.slots[slot] = decal;
        self.live.push_back(slot);
        if self.live.len() > self.capacity {
            if let Some(oldest) = self.live.pop_front() {
                self.slots[oldest].alive = false;
                self.free.push(oldest);
                self.dropped += 1;
            }
        }
        slot
    }

    /// Advances one view tick and expires decals. Returns expired count.
    pub fn advance(&mut self) -> usize {
        let mut expired = 0;
        let len = self.live.len();
        for _ in 0..len {
            let slot = self.live.pop_front().unwrap_or(0);
            let decal = &mut self.slots[slot];
            decal.time += 1.0;
            if decal.time >= decal.lifetime {
                decal.alive = false;
                self.free.push(slot);
                expired += 1;
            } else {
                self.live.push_back(slot);
            }
        }
        expired
    }

    /// Clears all decals.
    pub fn clear(&mut self) {
        while let Some(slot) = self.live.pop_front() {
            self.slots[slot].alive = false;
            self.free.push(slot);
        }
    }
}

/// `Effect.scorch` region name: `scorch-<size>-<0|1>`.
pub fn scorch_region(size: i32, variant: i32) -> RegionKey {
    match (size.clamp(0, 9), variant.clamp(0, 1)) {
        (0, 0) => RegionKey("scorch-0-0"),
        (0, _) => RegionKey("scorch-0-1"),
        (1, 0) => RegionKey("scorch-1-0"),
        (1, _) => RegionKey("scorch-1-1"),
        (2, 0) => RegionKey("scorch-2-0"),
        (2, _) => RegionKey("scorch-2-1"),
        (3, 0) => RegionKey("scorch-3-0"),
        (3, _) => RegionKey("scorch-3-1"),
        (4, 0) => RegionKey("scorch-4-0"),
        (4, _) => RegionKey("scorch-4-1"),
        (5, 0) => RegionKey("scorch-5-0"),
        (5, _) => RegionKey("scorch-5-1"),
        (6, 0) => RegionKey("scorch-6-0"),
        (6, _) => RegionKey("scorch-6-1"),
        (7, 0) => RegionKey("scorch-7-0"),
        (7, _) => RegionKey("scorch-7-1"),
        (8, 0) => RegionKey("scorch-8-0"),
        (8, _) => RegionKey("scorch-8-1"),
        (_, 0) => RegionKey("scorch-9-0"),
        (_, _) => RegionKey("scorch-9-1"),
    }
}

/// `Effect.rubble` region name: `rubble-<n>-<0|1>`.
pub fn rubble_region(block_size: i32, variant: i32) -> RegionKey {
    match (block_size, variant) {
        (1, 1) => RegionKey("rubble-1-1"),
        (2, 1) => RegionKey("rubble-2-1"),
        (3, 1) => RegionKey("rubble-3-1"),
        (1, _) => RegionKey("rubble-1-0"),
        (2, _) => RegionKey("rubble-2-0"),
        _ => RegionKey("rubble-3-0"),
    }
}

/// `Effect.scorch` rotation `random(4) * 90`.
pub fn scorch_rotation(random4: i32) -> f32 {
    (random4 & 3) as f32 * 90.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_and_cap() {
        let mut pool = DecalPool::new(3);
        for i in 0..4 {
            let mut d = Decal::new(RegionKey("scorch-0-0"), i as f32, 0.0, 0.0, Rgba::WHITE);
            d.lifetime = 100.0;
            pool.spawn(d);
        }
        assert_eq!(pool.live_count(), 3);
        assert_eq!(pool.dropped, 0);
        // The oldest (x=0) was dropped.
        assert!(pool.iter_live().all(|d| d.x >= 1.0));
    }

    #[test]
    fn alpha_fades_at_end() {
        let mut d = Decal::new(RegionKey("scorch-0-0"), 0.0, 0.0, 0.0, Rgba::WHITE);
        d.lifetime = 100.0;
        d.time = 0.0;
        assert_eq!(d.alpha(), 1.0);
        d.time = 98.0;
        assert!((d.alpha() - 1.0).abs() < 1e-5);
        d.time = 99.0;
        assert!((d.alpha() - 0.5).abs() < 1e-4);
    }

    #[test]
    fn expiry_advances() {
        let mut pool = DecalPool::new(4);
        let mut d = Decal::new(RegionKey("scorch-1-0"), 0.0, 0.0, 0.0, Rgba::WHITE);
        d.lifetime = 2.0;
        pool.spawn(d);
        assert_eq!(pool.advance(), 0);
        assert_eq!(pool.advance(), 1);
        assert_eq!(pool.live_count(), 0);
    }

    #[test]
    fn region_names_match_upstream() {
        assert_eq!(scorch_region(3, 1), RegionKey("scorch-3-1"));
        assert_eq!(rubble_region(2, 0), RegionKey("rubble-2-0"));
        assert_eq!(scorch_rotation(5), 90.0);
    }
}

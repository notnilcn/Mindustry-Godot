// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Trail channels and detach/fade hand-off (plan 17 §3.9).
//!
//! The ribbon math itself is [`crate::render::trail::Trail`] (owned here after
//! the plan-16 seed); this module adds the per-bullet/engine channel registry
//! and the `Fx.trailFade` detach used by plans 10/11.

pub use crate::render::trail::Trail;

use crate::content::Rgba;

use super::data::TrailChannelId;

/// Fixed-capacity trail channel registry.
#[derive(Debug, Default)]
pub struct TrailRegistry {
    channels: Vec<Option<Trail>>,
    /// Per-channel ribbon tint (`BulletType.trailColor`/`UnitType.trailColor`).
    colors: Vec<Rgba>,
    free: Vec<u32>,
}

impl TrailRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates a channel with a trail of `length` points (white tint).
    pub fn create(&mut self, length: usize) -> TrailChannelId {
        self.create_colored(length, Rgba::WHITE)
    }

    /// Allocates a channel with a trail of `length` points and a ribbon tint.
    pub fn create_colored(&mut self, length: usize, color: Rgba) -> TrailChannelId {
        let index = if let Some(index) = self.free.pop() {
            self.channels[index as usize] = Some(Trail::new(length));
            self.colors[index as usize] = color;
            index
        } else {
            let index = self.channels.len() as u32;
            self.channels.push(Some(Trail::new(length)));
            self.colors.push(color);
            index
        };
        TrailChannelId(index)
    }

    /// The ribbon tint for a channel (`white` when unknown).
    pub fn color(&self, id: TrailChannelId) -> Rgba {
        self.colors
            .get(id.0 as usize)
            .copied()
            .unwrap_or(Rgba::WHITE)
    }

    /// Number of live channels.
    pub fn count(&self) -> usize {
        self.channels.iter().filter(|c| c.is_some()).count()
    }

    /// Iterates live channels in slot order (additive helper for plan 17 M6).
    pub fn iter_live(&self) -> impl Iterator<Item = (TrailChannelId, &Trail)> {
        self.channels
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|t| (TrailChannelId(i as u32), t)))
    }

    /// Iterates live channels with their ribbon tint, in slot order.
    pub fn iter_live_colored(&self) -> impl Iterator<Item = (TrailChannelId, &Trail, Rgba)> {
        self.channels.iter().enumerate().filter_map(|(i, c)| {
            c.as_ref().map(|t| {
                (
                    TrailChannelId(i as u32),
                    t,
                    self.color(TrailChannelId(i as u32)),
                )
            })
        })
    }

    /// Trait for a channel.
    pub fn get(&self, id: TrailChannelId) -> Option<&Trail> {
        self.channels.get(id.0 as usize).and_then(|c| c.as_ref())
    }

    /// Mutable trail for a channel.
    pub fn get_mut(&mut self, id: TrailChannelId) -> Option<&mut Trail> {
        self.channels
            .get_mut(id.0 as usize)
            .and_then(|c| c.as_mut())
    }

    /// `Trail.update`.
    pub fn update(&mut self, id: TrailChannelId, x: f32, y: f32, width: f32, delta: f32) {
        if let Some(trail) = self.get_mut(id) {
            trail.update_width(x, y, width, delta);
        }
    }

    /// `Trail.shorten`.
    pub fn shorten(&mut self, id: TrailChannelId, delta: f32) {
        if let Some(trail) = self.get_mut(id) {
            trail.shorten(delta);
        }
    }

    /// Detaches a trail (for `Fx.trailFade`).
    pub fn detach(&mut self, id: TrailChannelId) -> Option<Trail> {
        let index = id.0 as usize;
        if index >= self.channels.len() {
            return None;
        }
        let trail = self.channels[index].take();
        if trail.is_some() {
            self.free.push(id.0);
        }
        trail
    }

    /// Removes a channel without returning the trail.
    pub fn remove(&mut self, id: TrailChannelId) {
        let _ = self.detach(id);
    }

    /// Clears all channels.
    pub fn clear(&mut self) {
        self.channels.clear();
        self.colors.clear();
        self.free.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_reuse_slots() {
        let mut reg = TrailRegistry::new();
        let a = reg.create(10);
        let _b = reg.create(10);
        assert_eq!(reg.count(), 2);
        reg.remove(a);
        assert_eq!(reg.count(), 1);
        let c = reg.create(10);
        assert_eq!(c, a);
    }

    #[test]
    fn detach_returns_trail_for_fade() {
        let mut reg = TrailRegistry::new();
        let id = reg.create(10);
        reg.update(id, 0.0, 0.0, 1.0, 1.0);
        reg.update(id, 5.0, 0.0, 1.0, 1.0);
        let trail = reg.detach(id).unwrap();
        assert!(trail.size() >= 1);
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn channels_carry_ribbon_color() {
        let mut reg = TrailRegistry::new();
        let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
        let id = reg.create_colored(8, red);
        assert_eq!(reg.color(id), red);
        let seen: Vec<Rgba> = reg.iter_live_colored().map(|(_, _, c)| c).collect();
        assert_eq!(seen, vec![red]);
        assert_eq!(reg.color(TrailChannelId(999)), Rgba::WHITE);
    }

    #[test]
    fn update_shorten_vertices_golden_shape() {
        let mut trail = Trail::new(10);
        trail.update(0.0, 0.0, 1.0);
        trail.update(8.0, 0.0, 1.0);
        let mut quads = Vec::new();
        trail.draw(2.0, &mut quads);
        assert!(!quads.is_empty());
    }
}

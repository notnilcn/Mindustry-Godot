// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! View-only effect-state pool (plan 17 §2.5/§3.5).
//!
//! Godot-free so the lifecycle is testable headlessly; `mind-gdext` owns the
//! instance and ticks it after each `Sim::tick()`. `EffectState` mirrors
//! `EffectStateComp` + `ChildComp` (`serialize = false`, `pooled = true`).
//! Nothing here may be read by simulation systems or checksums.

use crate::content::{EffectId, Rgba};
use crate::render::scan::CameraView;

use super::data::{EffectData, ViewEntityId, ViewSnapshot};
use super::def::EffectDef;
use super::pool_spec::{DELAYED_SPAWN_CAPACITY, EFFECT_POOL_CAPACITY};

/// One live effect instance.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectState {
    /// Effect id.
    pub def: EffectId,
    /// World x (parent-followed while alive).
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Tint.
    pub color: Rgba,
    /// Elapsed ticks (`TimedComp.time`).
    pub time: f32,
    /// Total ticks (mutable: `trailFade`).
    pub lifetime: f32,
    /// Typed payload.
    pub data: EffectData,
    /// Bound parent (from `followParent`).
    pub parent: Option<ViewEntityId>,
    /// Whether rotation follows the parent.
    pub rot_with_parent: bool,
    /// Child offset x (`ChildComp.offsetX`).
    pub offset_x: f32,
    /// Child offset y.
    pub offset_y: f32,
    /// Child offset position (`ChildComp.offsetPos`).
    pub offset_pos: f32,
    /// Child offset rotation (`ChildComp.offsetRot`).
    pub offset_rot: f32,
    /// Render-set lifetime override (applied next tick).
    pub lifetime_override: Option<f32>,
    /// Whether the state is alive.
    pub alive: bool,
}

impl EffectState {
    fn blank() -> Self {
        Self {
            def: EffectId::NONE,
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            color: Rgba::WHITE,
            time: 0.0,
            lifetime: 0.0,
            data: EffectData::None,
            parent: None,
            rot_with_parent: false,
            offset_x: 0.0,
            offset_y: 0.0,
            offset_pos: 0.0,
            offset_rot: 0.0,
            lifetime_override: None,
            alive: false,
        }
    }
}

/// A deferred spawn (`Effect.startDelay` / `Time.run`).
#[derive(Clone, Debug, PartialEq)]
pub struct PendingSpawn {
    /// Effect to spawn.
    pub def: EffectId,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Tint.
    pub color: Rgba,
    /// Payload.
    pub data: EffectData,
    /// Ticks until spawn.
    pub ticks_left: u32,
}

/// `Effect.shouldCreate` gate (plan 17 §3.5).
#[derive(Clone, Copy, Debug)]
pub struct FxGate {
    /// Not the dedicated server/headless render path.
    pub headless: bool,
    /// `Renderer.enableEffects`.
    pub effects: bool,
}

impl Default for FxGate {
    fn default() -> Self {
        Self {
            headless: false,
            effects: true,
        }
    }
}

impl FxGate {
    /// `shouldCreate`: not headless, effects enabled, not `any`.
    pub fn should_create(&self, def: &EffectDef) -> bool {
        !self.headless && self.effects && !def.is_none()
    }
}

/// `Core.camera.bounds().overlaps(setCentered(x, y, clip))`.
pub fn in_camera(camera: &CameraView, x: f32, y: f32, clip: f32) -> bool {
    let hw = (camera.w + clip) / 2.0;
    let hh = (camera.h + clip) / 2.0;
    (x - camera.x).abs() <= hw && (y - camera.y).abs() <= hh
}

/// Fixed-capacity insertion-ordered effect pool.
#[derive(Debug)]
pub struct FxPool {
    slots: Vec<EffectState>,
    free: Vec<usize>,
    live: Vec<usize>,
    pending: Vec<PendingSpawn>,
    capacity: usize,
    tick: u64,
    /// Total effects spawned.
    pub spawned: u64,
    /// Overflow drops (newest dropped when full).
    pub dropped: u64,
    /// Pending-ring overflow drops.
    pub pending_dropped: u64,
}

impl Default for FxPool {
    fn default() -> Self {
        Self::new(EFFECT_POOL_CAPACITY)
    }
}

impl FxPool {
    /// Builds a pool with a fixed capacity (allocates once).
    pub fn new(capacity: usize) -> Self {
        let mut slots = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots.push(EffectState::blank());
        }
        let free = (0..capacity).rev().collect();
        Self {
            slots,
            free,
            live: Vec::with_capacity(capacity),
            pending: Vec::with_capacity(DELAYED_SPAWN_CAPACITY),
            capacity,
            tick: 0,
            spawned: 0,
            dropped: 0,
            pending_dropped: 0,
        }
    }

    /// Current view tick.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Number of live states.
    pub fn live_count(&self) -> usize {
        self.live.len()
    }

    /// Number of pending (delayed) spawns.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Iterates live states in insertion order.
    pub fn iter_live(&self) -> impl Iterator<Item = &EffectState> {
        self.live.iter().map(move |&i| &self.slots[i])
    }

    /// A live state by index into the live list.
    pub fn live_at(&self, index: usize) -> Option<&EffectState> {
        self.live.get(index).map(|&i| &self.slots[i])
    }

    /// A mutable live state by index into the live list.
    pub fn live_at_mut(&mut self, index: usize) -> Option<&mut EffectState> {
        let slot = *self.live.get(index)?;
        self.slots.get_mut(slot)
    }

    /// Clears every state and pending spawn (does not touch counters).
    pub fn clear(&mut self) {
        for &i in &self.live {
            self.slots[i].alive = false;
        }
        self.live.clear();
        self.pending.clear();
    }

    /// Spawns a state directly (no gate). Returns its live index, or `None` on
    /// overflow or when the def is a spawn-time composite (which expands into
    /// child spawns instead of owning a state).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &mut self,
        def: &EffectDef,
        x: f32,
        y: f32,
        rotation: f32,
        color: Rgba,
        data: EffectData,
        snapshot: &dyn ViewSnapshot,
    ) -> Option<usize> {
        // Java `MultiEffect`/`RadialEffect`/`WrapEffect`/`SoundEffect` override
        // `create` and never add their own state.
        match &def.kind {
            super::def::EffectKind::Multi(children) => {
                for &child in children.iter() {
                    let child_def = super::def::registry().get(child);
                    self.spawn(child_def, x, y, rotation, color, data.clone(), snapshot);
                }
                return None;
            }
            super::def::EffectKind::Radial(params) => {
                let mut rot = rotation + params.rotation_offset;
                for _ in 0..params.amount {
                    let child_def = super::def::registry().get(params.effect);
                    self.spawn(
                        child_def,
                        x + super::angles::trnsx(rot, params.length_offset),
                        y + super::angles::trnsy(rot, params.length_offset),
                        rot + params.effect_rotation_offset,
                        color,
                        data.clone(),
                        snapshot,
                    );
                    rot += params.rotation_spacing;
                }
                return None;
            }
            super::def::EffectKind::Wrap(params) => {
                let child_def = super::def::registry().get(params.effect);
                return self.spawn(
                    child_def,
                    x,
                    y,
                    params.rotation,
                    params.color,
                    data,
                    snapshot,
                );
            }
            super::def::EffectKind::Sound(params) => {
                let child_def = super::def::registry().get(params.effect);
                return self.spawn(child_def, x, y, rotation, color, data, snapshot);
            }
            _ => {}
        }

        if def.start_delay > 0.0 {
            if self.pending.len() >= DELAYED_SPAWN_CAPACITY {
                self.pending_dropped += 1;
                return None;
            }
            self.pending.push(PendingSpawn {
                def: def.id,
                x,
                y,
                rotation,
                color,
                data,
                ticks_left: def.start_delay.ceil() as u32,
            });
            return None;
        }
        self.spawn_now(def.id, x, y, rotation, color, data, snapshot)
    }

    #[allow(clippy::too_many_arguments, clippy::collapsible_if)]
    fn spawn_now(
        &mut self,
        id: EffectId,
        x: f32,
        y: f32,
        rotation: f32,
        color: Rgba,
        data: EffectData,
        snapshot: &dyn ViewSnapshot,
    ) -> Option<usize> {
        let Some(slot_index) = self.free.pop() else {
            self.dropped += 1;
            return None;
        };
        let def = super::def::registry().get(id);
        let mut state = EffectState::blank();
        state.def = id;
        state.x = x;
        state.y = y;
        state.rotation = def.base_rotation + rotation;
        state.color = color;
        state.lifetime = def.lifetime;
        state.data = data.clone();
        state.alive = true;

        // ChildComp.add: bind parent and compute offsets.
        if def.follow_parent {
            if let EffectData::Unit { id: entity, .. } = &data {
                state.parent = Some(*entity);
                state.rot_with_parent = def.rot_with_parent;
                let pose = snapshot.unit_pose(*entity);
                if let Some(pose) = pose {
                    state.offset_x = x - pose.x;
                    state.offset_y = y - pose.y;
                    if def.rot_with_parent {
                        state.offset_pos = -pose.rotation;
                        state.offset_rot = state.rotation - pose.rotation;
                    }
                }
            }
        }

        self.slots[slot_index] = state;
        self.live.push(slot_index);
        self.spawned += 1;
        Some(self.live.len() - 1)
    }

    /// Applies a parent follow (`ChildComp.update`).
    fn follow(state: &mut EffectState, snapshot: &dyn ViewSnapshot) {
        let Some(parent) = state.parent else {
            return;
        };
        let Some(pose) = snapshot.unit_pose(parent) else {
            // Despawned parent freezes at the last pose (R-17-6).
            return;
        };
        if state.rot_with_parent {
            let ang = pose.rotation + state.offset_pos;
            let (sin, cos) = ang.to_radians().sin_cos();
            state.x = pose.x + state.offset_x * cos - state.offset_y * sin;
            state.y = pose.y + state.offset_x * sin + state.offset_y * cos;
            state.rotation = pose.rotation + state.offset_rot;
        } else {
            state.x = pose.x + state.offset_x;
            state.y = pose.y + state.offset_y;
        }
    }

    /// Advances one view tick: delayed spawns fire, live states follow/age and
    /// expire. Returns the number of states expired this tick.
    pub fn advance(&mut self, snapshot: &dyn ViewSnapshot) -> usize {
        self.tick += 1;

        // 1. Fire due delayed spawns (insertion order).
        if !self.pending.is_empty() {
            let mut i = 0;
            while i < self.pending.len() {
                self.pending[i].ticks_left = self.pending[i].ticks_left.saturating_sub(1);
                if self.pending[i].ticks_left == 0 {
                    let p = self.pending.remove(i);
                    self.spawn_now(p.def, p.x, p.y, p.rotation, p.color, p.data, snapshot);
                } else {
                    i += 1;
                }
            }
        }

        // 2. Follow parents, age, expire. Compaction keeps insertion order and
        //    never allocates in steady state.
        let mut write = 0usize;
        let mut expired = 0usize;
        let live_len = self.live.len();
        for read in 0..live_len {
            let slot_index = self.live[read];
            let state = &mut self.slots[slot_index];
            if !state.alive {
                continue;
            }
            Self::follow(state, snapshot);
            if let Some(override_life) = state.lifetime_override.take() {
                state.lifetime = override_life;
            }
            state.time = (state.time + 1.0).min(state.lifetime);
            if state.time >= state.lifetime {
                state.alive = false;
                self.free.push(slot_index);
                expired += 1;
            } else {
                self.live[write] = slot_index;
                write += 1;
            }
        }
        self.live.truncate(write);
        expired
    }

    /// Removes a live state by its slot (used by `Fx.trailFade` hand-off).
    pub fn despawn_at(&mut self, live_index: usize) -> Option<EffectState> {
        if live_index >= self.live.len() {
            return None;
        }
        let slot_index = self.live.remove(live_index);
        self.slots[slot_index].alive = false;
        self.free.push(slot_index);
        Some(self.slots[slot_index].clone())
    }

    /// Applies a program lifetime override to the state at `live_index`.
    pub fn set_lifetime_override(&mut self, live_index: usize, lifetime: f32) {
        if let Some(&slot_index) = self.live.get(live_index) {
            self.slots[slot_index].lifetime_override = Some(lifetime);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::EffectId;
    use crate::fx::data::{EmptySnapshot, Pose};
    use crate::fx::def::registry;

    fn spawn_id(pool: &mut FxPool, id: EffectId, snap: &dyn ViewSnapshot) {
        let def = registry().get(id);
        pool.spawn(def, 0.0, 0.0, 0.0, Rgba::WHITE, EffectData::None, snap);
    }

    #[test]
    fn lifecycle_spawn_expire_order() {
        let snap = EmptySnapshot;
        let mut pool = FxPool::new(16);
        let smoke = EffectId::SMOKE; // lifetime 100
        let hit = EffectId::HIT_BULLET_SMALL; // lifetime 14
        spawn_id(&mut pool, smoke, &snap);
        spawn_id(&mut pool, hit, &snap);
        assert_eq!(pool.live_count(), 2);
        assert_eq!(pool.live_at(0).unwrap().def, smoke);
        assert_eq!(pool.live_at(1).unwrap().def, hit);

        for _ in 0..13 {
            pool.advance(&snap);
        }
        // hit time 13 < 14, still alive.
        assert_eq!(pool.live_count(), 2);
        pool.advance(&snap);
        // hit expires at time 14, insertion order preserved for smoke.
        assert_eq!(pool.live_count(), 1);
        assert_eq!(pool.live_at(0).unwrap().def, smoke);
    }

    #[test]
    fn start_delay_deferred() {
        let snap = EmptySnapshot;
        let mut pool = FxPool::new(8);
        let mut def = registry().get(EffectId::SMOKE).clone();
        def.start_delay = 5.0;
        pool.spawn(&def, 1.0, 2.0, 0.0, Rgba::WHITE, EffectData::None, &snap);
        assert_eq!(pool.live_count(), 0);
        assert_eq!(pool.pending_count(), 1);
        for _ in 0..4 {
            pool.advance(&snap);
        }
        assert_eq!(pool.live_count(), 0);
        pool.advance(&snap);
        assert_eq!(pool.live_count(), 1);
        // The state is added during the tick and `TimedComp.update` runs in the
        // same frame, so it has already aged one tick.
        assert_eq!(pool.live_at(0).unwrap().time, 1.0);
    }

    #[test]
    fn follow_parent_tracks_snapshot() {
        struct Snap(Pose);
        impl ViewSnapshot for Snap {
            fn unit_pose(&self, _id: ViewEntityId) -> Option<Pose> {
                Some(self.0)
            }
        }
        let snap = Snap(Pose {
            x: 10.0,
            y: 20.0,
            rotation: 90.0,
        });
        let mut pool = FxPool::new(8);
        let def = registry().get(EffectId::UNIT_CONTROL).clone();
        let data = EffectData::Unit {
            id: ViewEntityId(1),
            kind: crate::content::UnitTypeId::new(0),
        };
        pool.spawn(&def, 10.0, 20.0, 0.0, Rgba::WHITE, data, &snap);
        // Follow uses base_rotation folded into rotation; rot_with_parent default false.
        pool.advance(&Snap(Pose {
            x: 15.0,
            y: 25.0,
            rotation: 90.0,
        }));
        let state = pool.live_at(0).unwrap();
        assert!((state.x - 15.0).abs() < 1e-5);
        assert!((state.y - 25.0).abs() < 1e-5);
    }

    #[test]
    fn overflow_drops_newest() {
        let snap = EmptySnapshot;
        let mut pool = FxPool::new(2);
        spawn_id(&mut pool, EffectId::SMOKE, &snap);
        spawn_id(&mut pool, EffectId::SMOKE, &snap);
        spawn_id(&mut pool, EffectId::SMOKE, &snap);
        assert_eq!(pool.live_count(), 2);
        assert_eq!(pool.dropped, 1);
    }

    #[test]
    fn composites_expand_at_spawn() {
        use super::super::def::{EffectDef, EffectKind, RadialParams, WrapParams};
        use smallvec::smallvec;
        let snap = EmptySnapshot;

        // Multi => one state per child.
        let mut multi = EffectDef::blank(EffectId(200), "multi", 10.0, 0.0);
        multi.kind = EffectKind::Multi(smallvec![EffectId::SMOKE, EffectId::HIT_BULLET_SMALL]);
        let mut pool = FxPool::new(8);
        pool.spawn(&multi, 0.0, 0.0, 0.0, Rgba::WHITE, EffectData::None, &snap);
        assert_eq!(pool.live_count(), 2);

        // Radial => `amount` states.
        let mut radial = EffectDef::blank(EffectId(201), "radial", 10.0, 0.0);
        radial.kind = EffectKind::Radial(RadialParams {
            effect: EffectId::SMOKE,
            amount: 4,
            ..Default::default()
        });
        let mut pool = FxPool::new(8);
        pool.spawn(&radial, 0.0, 0.0, 0.0, Rgba::WHITE, EffectData::None, &snap);
        assert_eq!(pool.live_count(), 4);

        // Wrap => one recolored child.
        let mut wrap = EffectDef::blank(EffectId(202), "wrap", 10.0, 0.0);
        wrap.kind = EffectKind::Wrap(WrapParams {
            effect: EffectId::SMOKE,
            color: Rgba::BLACK,
            rotation: 45.0,
        });
        let mut pool = FxPool::new(8);
        pool.spawn(&wrap, 0.0, 0.0, 0.0, Rgba::WHITE, EffectData::None, &snap);
        assert_eq!(pool.live_count(), 1);
        let state = pool.live_at(0).unwrap();
        assert_eq!(state.color, Rgba::BLACK);
        assert!((state.rotation - 45.0).abs() < 1e-5);
    }
}

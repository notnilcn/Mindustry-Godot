// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-frame/phase time counters — port of
//! `core/src/mindustry/core/PerfCounter.java` (GAPS #70).
//!
//! Java's `PerfCounter` is an enum of static counters whose `begin`/`end` call
//! `Time.nanos()` and record `Core.graphics.getFrameId()`. This port keeps the
//! same window/refresh semantics (a 120-sample [`WindowedMean`] and a 100 ms
//! refresh for the displayed value) but injects the clock through
//! [`PerfClock`], so `mind-core` stays Godot-free and the tests run on a manual
//! clock. `ClientLauncher.update`'s "everything that is not a displayed counter
//! is `other`" aggregation is [`PerfCounters::finish_frame`].
//!
//! Java exposes the counters as static enum instances; here [`PerfCounters`]
//! owns the 14 slots and the host keeps one instance (the `MindPlatform` node in
//! `mind-gdext`). Nothing in the simulation reads these values.

use crate::math::WindowedMean;

/// Samples retained per counter (`PerfCounter.meanWindow`).
pub const MEAN_WINDOW: usize = 120;

/// Displayed-value refresh interval in milliseconds (`refreshTimeMillis`).
pub const REFRESH_TIME_MS: u64 = 100;

/// Nanoseconds per millisecond (`arc.util.Time.nanosPerMilli`).
pub const NANOS_PER_MILLI: f32 = 1_000_000.0;

/// The clock the counters read (`Time.nanos`/`Time.millis`/`Core.graphics.getFrameId`).
///
/// Hosts implement this over their engine clock; tests use a manual clock.
pub trait PerfClock {
    /// Monotonic nanoseconds (`Time.nanos()`).
    fn nanos(&self) -> u64;
    /// Monotonic milliseconds (`Time.millis()`).
    fn millis(&self) -> u64;
    /// Monotonic frame id (`Core.graphics.getFrameId()`).
    fn frame_id(&self) -> u64;
}

/// One upstream counter (`PerfCounter.frame`, `PerfCounter.update`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(usize)]
pub enum PerfCounterKind {
    /// Whole update loop (`PerfCounter.frame`).
    Frame = 0,
    /// Top-level client update (`PerfCounter.update`).
    Update,
    /// Everything not covered by a displayed counter (`PerfCounter.other`).
    Other,
    /// Game-state update (`PerfCounter.stateUpdate`).
    StateUpdate,
    /// Misc entity work (`PerfCounter.entityMisc`).
    EntityMisc,
    /// Entity update wrapper (`PerfCounter.entityUpdate`).
    EntityUpdate,
    /// Building update (`PerfCounter.buildingUpdate`).
    BuildingUpdate,
    /// Power graph update (`PerfCounter.powerUpdate`).
    PowerUpdate,
    /// Unit update (`PerfCounter.unitUpdate`).
    UnitUpdate,
    /// Async physics enqueue (`PerfCounter.unitPhysicsAsync`).
    UnitPhysicsAsync,
    /// Async physics wait (`PerfCounter.unitPhysicsWait`).
    UnitPhysicsWait,
    /// Bullet update (`PerfCounter.bulletUpdate`).
    BulletUpdate,
    /// UI update (`PerfCounter.ui`).
    Ui,
    /// Renderer update (`PerfCounter.render`).
    Render,
}

impl PerfCounterKind {
    /// Number of counters (`PerfCounter.values().length`).
    pub const COUNT: usize = 14;

    /// Every counter in declaration order (`PerfCounter.all`).
    pub const ALL: [PerfCounterKind; Self::COUNT] = [
        PerfCounterKind::Frame,
        PerfCounterKind::Update,
        PerfCounterKind::Other,
        PerfCounterKind::StateUpdate,
        PerfCounterKind::EntityMisc,
        PerfCounterKind::EntityUpdate,
        PerfCounterKind::BuildingUpdate,
        PerfCounterKind::PowerUpdate,
        PerfCounterKind::UnitUpdate,
        PerfCounterKind::UnitPhysicsAsync,
        PerfCounterKind::UnitPhysicsWait,
        PerfCounterKind::BulletUpdate,
        PerfCounterKind::Ui,
        PerfCounterKind::Render,
    ];

    /// The subset shown by the debug overlay (`PerfCounter.displayedCounters`),
    /// in the upstream order.
    pub const DISPLAYED: [PerfCounterKind; 9] = [
        PerfCounterKind::PowerUpdate,
        PerfCounterKind::BuildingUpdate,
        PerfCounterKind::EntityMisc,
        PerfCounterKind::BulletUpdate,
        PerfCounterKind::UnitUpdate,
        PerfCounterKind::Render,
        PerfCounterKind::Ui,
        PerfCounterKind::StateUpdate,
        PerfCounterKind::Other,
    ];

    /// `0..COUNT` index of this counter.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The upstream enum constant name (probe/telemetry key).
    pub fn name(self) -> &'static str {
        match self {
            PerfCounterKind::Frame => "frame",
            PerfCounterKind::Update => "update",
            PerfCounterKind::Other => "other",
            PerfCounterKind::StateUpdate => "stateUpdate",
            PerfCounterKind::EntityMisc => "entityMisc",
            PerfCounterKind::EntityUpdate => "entityUpdate",
            PerfCounterKind::BuildingUpdate => "buildingUpdate",
            PerfCounterKind::PowerUpdate => "powerUpdate",
            PerfCounterKind::UnitUpdate => "unitUpdate",
            PerfCounterKind::UnitPhysicsAsync => "unitPhysicsAsync",
            PerfCounterKind::UnitPhysicsWait => "unitPhysicsWait",
            PerfCounterKind::BulletUpdate => "bulletUpdate",
            PerfCounterKind::Ui => "ui",
            PerfCounterKind::Render => "render",
        }
    }

    /// Parses an upstream enum constant name.
    pub fn from_name(name: &str) -> Option<Self> {
        PerfCounterKind::ALL
            .iter()
            .copied()
            .find(|counter| counter.name() == name)
    }
}

/// Timing state of one counter (the Java enum instance fields).
#[derive(Debug, Clone, PartialEq)]
pub struct PerfCounter {
    mean: WindowedMean,
    /// `lastUpdateFrame`; `-1` before the first sample, as in Java.
    last_update_frame: i64,
    value_refresh_time: u64,
    refresh_value: f32,
    begin_time: u64,
    part_value: i64,
    began: bool,
}

impl Default for PerfCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl PerfCounter {
    /// An empty counter (all samples zero, `lastUpdateFrame = -1`).
    pub fn new() -> Self {
        Self {
            mean: WindowedMean::new(MEAN_WINDOW),
            last_update_frame: -1,
            value_refresh_time: 0,
            refresh_value: 0.0,
            begin_time: 0,
            part_value: 0,
            began: false,
        }
    }

    /// Records a raw nanosecond sample (`add`).
    pub fn add(&mut self, nanos: i64, clock: &dyn PerfClock) {
        self.mean.add(nanos as f32);
        self.last_update_frame = clock.frame_id() as i64;
    }

    /// Starts timing (`begin`).
    pub fn begin(&mut self, clock: &dyn PerfClock) {
        self.last_update_frame = clock.frame_id() as i64;
        self.began = true;
        self.begin_time = clock.nanos();
    }

    /// Stops timing and records the elapsed time (`end`).
    pub fn end(&mut self, clock: &dyn PerfClock) {
        self.end_subtract(clock, 0);
    }

    /// Stops timing and records `elapsed - subtract_ns` (`end(long)`).
    ///
    /// The subtraction removes nested intervals (for example `stateUpdate`
    /// excludes `entityUpdate`). A counter that never began is ignored.
    pub fn end_subtract(&mut self, clock: &dyn PerfClock, subtract_ns: u64) {
        if !self.began {
            return;
        }
        self.last_update_frame = clock.frame_id() as i64;
        self.began = false;
        let elapsed = clock.nanos().saturating_sub(self.begin_time);
        self.mean.add(elapsed.saturating_sub(subtract_ns) as f32);
    }

    /// Records a zero sample when no update happened this frame (`checkUpdate`).
    ///
    /// `lastUpdateFrame` is deliberately not refreshed, exactly like upstream:
    /// an idle counter keeps reporting `0` until the next `begin`/`add`.
    pub fn check_update(&mut self, clock: &dyn PerfClock) {
        if self.last_update_frame < clock.frame_id() as i64 {
            self.mean.add(0.0);
        }
    }

    /// Starts a partial interval (`beginPart`).
    pub fn begin_part(&mut self, clock: &dyn PerfClock) {
        self.began = true;
        self.begin_time = clock.nanos();
    }

    /// Ends a partial interval and accumulates it (`endPart`).
    pub fn end_part(&mut self, clock: &dyn PerfClock) {
        if !self.began {
            return;
        }
        self.began = false;
        self.part_value = self
            .part_value
            .saturating_add(clock.nanos().saturating_sub(self.begin_time) as i64);
    }

    /// Flushes accumulated partial intervals as one sample (`finishParts`).
    pub fn finish_parts(&mut self, clock: &dyn PerfClock) {
        self.last_update_frame = clock.frame_id() as i64;
        self.mean.add(self.part_value as f32);
        self.part_value = 0;
    }

    /// Displayed value in milliseconds (`valueMs`), refreshed at most every
    /// [`REFRESH_TIME_MS`] so the UI does not jitter.
    pub fn value_ms(&mut self, clock: &dyn PerfClock) -> f32 {
        if clock.millis().saturating_sub(self.value_refresh_time) > REFRESH_TIME_MS {
            self.refresh_value = self.raw_value_ms();
            self.value_refresh_time = clock.millis();
        }
        self.refresh_value
    }

    /// The most recent sample in nanoseconds (`latestValueNs`).
    pub fn latest_value_ns(&self) -> i64 {
        self.mean.latest() as i64
    }

    /// Raw mean in milliseconds, before the refresh throttle (`rawValueMs`).
    pub fn raw_value_ms(&self) -> f32 {
        self.mean.raw_mean() / NANOS_PER_MILLI
    }

    /// Raw mean in nanoseconds (`rawValueNs`).
    pub fn raw_value_ns(&self) -> i64 {
        self.mean.raw_mean() as i64
    }

    /// Number of samples currently in the window (test/telemetry probe).
    pub fn sample_count(&self) -> usize {
        self.mean.get_count()
    }
}

/// The 14 upstream counters owned together (`PerfCounter.all`).
#[derive(Debug, Clone, PartialEq)]
pub struct PerfCounters {
    counters: [PerfCounter; PerfCounterKind::COUNT],
}

impl Default for PerfCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl PerfCounters {
    /// All counters empty.
    pub fn new() -> Self {
        Self {
            counters: std::array::from_fn(|_| PerfCounter::new()),
        }
    }

    /// Shared access to one counter.
    pub fn get(&self, counter: PerfCounterKind) -> &PerfCounter {
        &self.counters[counter.index()]
    }

    /// Mutable access to one counter.
    pub fn get_mut(&mut self, counter: PerfCounterKind) -> &mut PerfCounter {
        &mut self.counters[counter.index()]
    }

    /// [`PerfCounter::add`] for one slot.
    pub fn add(&mut self, counter: PerfCounterKind, nanos: i64, clock: &dyn PerfClock) {
        self.get_mut(counter).add(nanos, clock);
    }

    /// [`PerfCounter::begin`] for one slot.
    pub fn begin(&mut self, counter: PerfCounterKind, clock: &dyn PerfClock) {
        self.get_mut(counter).begin(clock);
    }

    /// [`PerfCounter::end`] for one slot.
    pub fn end(&mut self, counter: PerfCounterKind, clock: &dyn PerfClock) {
        self.get_mut(counter).end(clock);
    }

    /// [`PerfCounter::end_subtract`] for one slot.
    pub fn end_subtract(
        &mut self,
        counter: PerfCounterKind,
        clock: &dyn PerfClock,
        subtract_ns: u64,
    ) {
        self.get_mut(counter).end_subtract(clock, subtract_ns);
    }

    /// [`PerfCounter::begin_part`] for one slot.
    pub fn begin_part(&mut self, counter: PerfCounterKind, clock: &dyn PerfClock) {
        self.get_mut(counter).begin_part(clock);
    }

    /// [`PerfCounter::end_part`] for one slot.
    pub fn end_part(&mut self, counter: PerfCounterKind, clock: &dyn PerfClock) {
        self.get_mut(counter).end_part(clock);
    }

    /// [`PerfCounter::finish_parts`] for one slot.
    pub fn finish_parts(&mut self, counter: PerfCounterKind, clock: &dyn PerfClock) {
        self.get_mut(counter).finish_parts(clock);
    }

    /// [`PerfCounter::check_update`] for every counter (`ClientLauncher.update`).
    pub fn check_update(&mut self, clock: &dyn PerfClock) {
        for counter in PerfCounterKind::ALL {
            self.get_mut(counter).check_update(clock);
        }
    }

    /// `ClientLauncher.update`'s end-of-frame aggregation: take the `update`
    /// counter, subtract every other displayed counter, and add the remainder
    /// to `other`; then zero-fill idle counters.
    pub fn finish_frame(&mut self, clock: &dyn PerfClock) {
        let mut other = self.get(PerfCounterKind::Update).latest_value_ns();
        for counter in PerfCounterKind::DISPLAYED {
            if counter != PerfCounterKind::Other {
                other -= self.get(counter).latest_value_ns();
            }
        }
        self.get_mut(PerfCounterKind::Other).add(other, clock);
        self.check_update(clock);
    }

    /// `(counter, value_ms)` for the displayed counters, in upstream order.
    ///
    /// Uses the 100 ms refresh throttle ([`PerfCounter::value_ms`]).
    pub fn displayed_ms(&mut self, clock: &dyn PerfClock) -> Vec<(PerfCounterKind, f32)> {
        PerfCounterKind::DISPLAYED
            .iter()
            .map(|counter| (*counter, self.get_mut(*counter).value_ms(clock)))
            .collect()
    }

    /// `(counter, raw_value_ms)` for every counter, in declaration order.
    ///
    /// Unthrottled: the host/probe path that wants stable numbers should use
    /// [`PerfCounters::displayed_ms`] for the displayed subset.
    pub fn all_raw_ms(&self) -> Vec<(PerfCounterKind, f32)> {
        PerfCounterKind::ALL
            .iter()
            .map(|counter| (*counter, self.get(*counter).raw_value_ms()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic clock for tests.
    #[derive(Debug, Default)]
    struct ManualClock {
        nanos: u64,
        millis: u64,
        frame: u64,
    }

    impl PerfClock for ManualClock {
        fn nanos(&self) -> u64 {
            self.nanos
        }
        fn millis(&self) -> u64 {
            self.millis
        }
        fn frame_id(&self) -> u64 {
            self.frame
        }
    }

    #[test]
    fn declaration_order_matches_upstream_enum() {
        assert_eq!(PerfCounterKind::COUNT, 14);
        assert_eq!(PerfCounterKind::ALL[0], PerfCounterKind::Frame);
        assert_eq!(PerfCounterKind::ALL[13], PerfCounterKind::Render);
        assert_eq!(PerfCounterKind::Other.index(), 2);

        let displayed: Vec<&str> = PerfCounterKind::DISPLAYED
            .iter()
            .map(|c| c.name())
            .collect();
        assert_eq!(
            displayed,
            [
                "powerUpdate",
                "buildingUpdate",
                "entityMisc",
                "bulletUpdate",
                "unitUpdate",
                "render",
                "ui",
                "stateUpdate",
                "other",
            ]
        );
        for counter in PerfCounterKind::ALL {
            assert_eq!(PerfCounterKind::from_name(counter.name()), Some(counter));
        }
        assert_eq!(PerfCounterKind::from_name("frameMs"), None);
    }

    #[test]
    fn begin_end_records_elapsed_ns() {
        let mut clock = ManualClock {
            nanos: 1_000,
            millis: 5,
            frame: 7,
        };
        let mut counter = PerfCounter::new();
        counter.begin(&clock);
        clock.nanos = 3_500;
        counter.end(&clock);

        assert_eq!(counter.sample_count(), 1);
        assert_eq!(counter.raw_value_ns(), 2_500);
        assert_eq!(counter.latest_value_ns(), 2_500);
        assert!((counter.raw_value_ms() - 0.0025).abs() < 1e-9);
    }

    #[test]
    fn end_subtract_removes_nested_interval() {
        let mut clock = ManualClock {
            nanos: 10_000,
            ..ManualClock::default()
        };
        let mut counter = PerfCounter::new();
        counter.begin(&clock);
        clock.nanos = 14_000;
        counter.end_subtract(&clock, 1_500);
        assert_eq!(counter.latest_value_ns(), 2_500);
    }

    #[test]
    fn end_without_begin_is_ignored() {
        let clock = ManualClock::default();
        let mut counter = PerfCounter::new();
        counter.end(&clock);
        assert_eq!(counter.sample_count(), 0);
    }

    #[test]
    fn check_update_zero_fills_idle_frames() {
        let mut clock = ManualClock {
            nanos: 0,
            millis: 0,
            frame: 1,
        };
        let mut counter = PerfCounter::new();
        counter.add(2_000, &clock);
        assert_eq!(counter.latest_value_ns(), 2_000);

        clock.frame = 2;
        counter.check_update(&clock);
        assert_eq!(counter.latest_value_ns(), 0);
        assert_eq!(counter.sample_count(), 2);

        // Upstream `checkUpdate` deliberately does not refresh
        // `lastUpdateFrame`, so a second call in the same frame adds another
        // zero; the real caller invokes it exactly once per frame.
        counter.check_update(&clock);
        assert_eq!(counter.sample_count(), 3);
    }

    #[test]
    fn partial_intervals_accumulate_into_one_sample() {
        let mut clock = ManualClock::default();
        let mut counter = PerfCounter::new();
        clock.nanos = 1_000;
        counter.begin_part(&clock);
        clock.nanos = 2_000;
        counter.end_part(&clock);
        clock.nanos = 5_000;
        counter.begin_part(&clock);
        clock.nanos = 6_000;
        counter.end_part(&clock);
        counter.finish_parts(&clock);
        assert_eq!(counter.sample_count(), 1);
        assert_eq!(counter.latest_value_ns(), 2_000);

        // An endPart without beginPart is ignored.
        clock.nanos = 9_000;
        counter.end_part(&clock);
        assert_eq!(counter.sample_count(), 1);
    }

    #[test]
    fn value_ms_is_refreshed_every_100ms() {
        let mut clock = ManualClock {
            nanos: 0,
            millis: 1_000,
            frame: 1,
        };
        let mut counter = PerfCounter::new();
        counter.add(2_000_000, &clock);
        assert!((counter.value_ms(&clock) - 2.0).abs() < 1e-6);

        // New sample inside the refresh window: displayed value is cached.
        clock.nanos = 1_000_000_000;
        clock.millis = 1_050;
        counter.add(10_000_000, &clock);
        assert!((counter.value_ms(&clock) - 2.0).abs() < 1e-6);

        // Past the window: refresh to the new mean.
        clock.millis = 1_150;
        assert!((counter.value_ms(&clock) - 6.0).abs() < 1e-6);
    }

    #[test]
    fn finish_frame_attributes_the_remainder_to_other() {
        let clock = ManualClock {
            nanos: 0,
            millis: 1_000,
            frame: 4,
        };
        let mut counters = PerfCounters::new();
        counters.add(PerfCounterKind::Update, 10_000, &clock);
        counters.add(PerfCounterKind::PowerUpdate, 1_000, &clock);
        counters.add(PerfCounterKind::BuildingUpdate, 2_000, &clock);
        counters.add(PerfCounterKind::Render, 3_000, &clock);

        counters.finish_frame(&clock);

        // 10_000 - (1_000 + 2_000 + 3_000) = 4_000; the other slots are 0.
        assert_eq!(
            counters.get(PerfCounterKind::Other).latest_value_ns(),
            4_000
        );
        let displayed = counters.displayed_ms(&clock);
        assert_eq!(displayed.len(), PerfCounterKind::DISPLAYED.len());
        assert_eq!(displayed[0].0, PerfCounterKind::PowerUpdate);
        assert!((displayed[0].1 - 0.001).abs() < 1e-6);
    }

    #[test]
    fn all_raw_ms_covers_every_counter() {
        let counters = PerfCounters::new();
        let rows = counters.all_raw_ms();
        assert_eq!(rows.len(), PerfCounterKind::COUNT);
        assert!(rows.iter().all(|(_, ms)| *ms == 0.0));
    }
}

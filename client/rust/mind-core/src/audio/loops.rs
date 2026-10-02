// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Positional loop aggregation, `SoundLoop`, weather loops and ambience
//! (plan 18 §3.7/§3.8).
//!
//! Pure port of `SoundControl.loop`/`updateLoops`, `SoundLoop`, and
//! `SoundControl.AudioThread.doLoop`. Godot voices are supplied through
//! [`LoopOutput`]; the 20 Hz poller consumes a pre-built [`AmbientSnapshot`]
//! (never reads the ECS off-thread — plan 18 §2.4 deviation 2).

use indexmap::IndexMap;

use super::events::AudioEvent;
use super::ids::{SoundId, VoiceKey};
use super::math::{
    LOOP_LERP, LOOP_START_VOLUME, LOOP_STOP_VOLUME, Listener, MIN_LOOP_VOLUME, SOUND_LOOP_FADE,
    SOUND_LOOP_STOP_VOLUME, calc_falloff, calc_pan, clamp01, is_zero, lerp_delta,
};

/// Per-sound loop accumulator (`SoundControl.SoundData`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoopData {
    /// Accumulated volume (`volume`).
    pub volume: f32,
    /// Accumulated weighted pitch (`pitch`).
    pub pitch: f32,
    /// Sum of base falloff (`total`).
    pub total: f32,
    /// Sum of weighted volume (`totalVolume`).
    pub total_volume: f32,
    /// Weighted x centroid (`sumX`).
    pub sum_x: f32,
    /// Weighted y centroid (`sumY`).
    pub sum_y: f32,
    /// Faded output volume (`curVolume`).
    pub cur_volume: f32,
    /// Live voice id (`soundID`; `-1`/`None` when silent).
    pub voice: Option<u64>,
}

impl LoopData {
    /// Resets the per-frame accumulators.
    pub fn reset_accum(&mut self) {
        self.pitch = 0.0;
        self.volume = 0.0;
        self.total = 0.0;
        self.total_volume = 0.0;
        self.sum_x = 0.0;
        self.sum_y = 0.0;
    }
}

/// Voice operations required by [`LoopMixer`].
pub trait LoopOutput {
    /// Starts a loop voice, returning its id.
    fn start_loop(&mut self, sound: SoundId, volume: f32, pitch: f32, pan: f32) -> u64;
    /// Updates a loop voice.
    fn update_loop(&mut self, voice: u64, volume: f32, pitch: f32, pan: f32);
    /// Stops a loop voice.
    fn stop_loop(&mut self, voice: u64);
    /// Whether a voice is currently playing.
    fn is_playing(&self, voice: u64) -> bool;
}

/// `SoundControl.loop` accumulation + `updateLoops` lifecycle.
#[derive(Debug, Clone, Default)]
pub struct LoopMixer {
    data: IndexMap<SoundId, LoopData>,
}

impl LoopMixer {
    /// Creates an empty mixer (reserves the upstream-scale capacity).
    pub fn new() -> Self {
        LoopMixer {
            data: IndexMap::with_capacity(64),
        }
    }

    /// Number of tracked sounds.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether no sound is tracked.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// `SoundControl.loop(sound, pos, volume, pitch)` accumulation.
    pub fn accumulate(
        &mut self,
        sound: SoundId,
        x: f32,
        y: f32,
        volume: f32,
        pitch: f32,
        listener: Listener,
    ) {
        if sound.is_none_or_unset() || volume <= MIN_LOOP_VOLUME {
            return;
        }
        let base_vol = calc_falloff(x, y, listener, 0.0);
        let vol = base_vol * volume;
        let entry = self.data.entry(sound).or_default();
        entry.volume += vol;
        entry.pitch += pitch * vol;
        entry.volume = clamp01(entry.volume);
        entry.total += base_vol;
        entry.total_volume += vol;
        entry.sum_x += x * base_vol;
        entry.sum_y += y * base_vol;
    }

    /// Per-sound data (tests/inspector).
    pub fn data(&self, sound: SoundId) -> Option<&LoopData> {
        self.data.get(&sound)
    }

    /// `SoundControl.updateLoops`: menu clear, pause freeze, voice lifecycle.
    pub fn update(
        &mut self,
        avol: f32,
        in_game: bool,
        paused: bool,
        listener: Listener,
        out: &mut impl LoopOutput,
    ) {
        if !in_game {
            self.data.clear();
            return;
        }
        if paused {
            return;
        }

        for (sound, data) in self.data.iter_mut() {
            data.cur_volume = lerp_delta(data.cur_volume, data.volume * avol, LOOP_LERP);

            let play = data.cur_volume > LOOP_START_VOLUME;
            let pan = if is_zero(data.total, 0.0001) {
                0.0
            } else {
                calc_pan(data.sum_x / data.total, listener)
            };
            let pitch = if is_zero(data.total_volume, 0.0001) {
                1.0
            } else {
                data.pitch / data.total_volume
            };

            match data.voice {
                None => {
                    if play {
                        let voice = out.start_loop(*sound, data.cur_volume, pitch, pan);
                        data.voice = Some(voice);
                    }
                }
                Some(voice) if !out.is_playing(voice) => {
                    if play {
                        let started = out.start_loop(*sound, data.cur_volume, pitch, pan);
                        data.voice = Some(started);
                    }
                }
                Some(voice) => {
                    if data.cur_volume <= LOOP_STOP_VOLUME {
                        out.stop_loop(voice);
                        data.voice = None;
                        continue;
                    }
                    out.update_loop(voice, data.cur_volume, pitch, pan);
                }
            }

            data.reset_accum();
        }
    }

    /// Merges 20 Hz ambient results into the accumulators (`updateLoops` tail).
    pub fn merge_ambient(&mut self, results: &[AmbientSoundData]) {
        for ambient in results {
            let target = self.data.entry(ambient.sound).or_default();
            target.pitch = ambient.pitch;
            target.volume = ambient.volume;
            target.total = ambient.total;
            target.total_volume = ambient.total_volume;
            target.sum_x = ambient.sum_x;
            target.sum_y = ambient.sum_y;
        }
    }

    /// Clears all loop voices (leaving-game / `ResetEvent`).
    pub fn stop_all(&mut self, out: &mut impl LoopOutput) {
        for (_, data) in self.data.iter_mut() {
            if let Some(voice) = data.voice.take() {
                out.stop_loop(voice);
            }
        }
        self.data.clear();
    }
}

/// `SoundLoop` per-instance fade state (plan 18 §3.7).
#[derive(Debug, Clone, PartialEq)]
pub struct SoundLoopState {
    /// Looped sound.
    pub sound: SoundId,
    /// Base volume.
    pub base_volume: f32,
    /// Fade volume `0..1`.
    pub volume: f32,
    /// Stable voice key.
    pub key: VoiceKey,
    /// Live voice id.
    pub voice: Option<u64>,
    /// Fade speed per frame.
    pub fade_speed: f32,
}

/// Result of one `SoundLoopState::update`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoundLoopAction {
    /// Nothing to do.
    None,
    /// Start the loop voice.
    Start {
        /// Volume.
        volume: f32,
        /// Pan.
        pan: f32,
        /// Pitch.
        pitch: f32,
    },
    /// Update the existing voice.
    Update {
        /// Voice id.
        voice: u64,
        /// Volume.
        volume: f32,
        /// Pan.
        pan: f32,
    },
    /// Stop the voice.
    Stop {
        /// Voice id.
        voice: u64,
    },
}

impl SoundLoopState {
    /// Creates a fade loop for `sound` with `base_volume`.
    pub fn new(sound: SoundId, base_volume: f32, key: VoiceKey) -> Self {
        SoundLoopState {
            sound,
            base_volume,
            volume: 0.0,
            key,
            voice: None,
            fade_speed: SOUND_LOOP_FADE,
        }
    }

    /// `SoundLoop.update(x, y, play, volumeScl)` (`delta = 1` frame).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        x: f32,
        y: f32,
        play: bool,
        volume_scl: f32,
        listener: Listener,
    ) -> SoundLoopAction {
        if self.base_volume <= 0.0 {
            return SoundLoopAction::None;
        }

        if self.voice.is_none() {
            if play {
                let falloff = calc_falloff(x, y, listener, 0.0);
                return SoundLoopAction::Start {
                    volume: falloff * self.volume * self.base_volume * volume_scl,
                    pan: calc_pan(x, listener),
                    pitch: 1.0,
                };
            }
            return SoundLoopAction::None;
        }

        if play {
            self.volume = clamp01(self.volume + self.fade_speed);
        } else {
            self.volume = clamp01(self.volume - self.fade_speed);
            if self.volume <= SOUND_LOOP_STOP_VOLUME {
                let voice = self.voice.take().unwrap_or(0);
                return SoundLoopAction::Stop { voice };
            }
        }

        let falloff = calc_falloff(x, y, listener, 0.0);
        SoundLoopAction::Update {
            voice: self.voice.unwrap_or(0),
            volume: falloff * self.volume * self.base_volume * volume_scl,
            pan: calc_pan(x, listener),
        }
    }

    /// Applies a returned action to the live voice id (client glue).
    pub fn apply(&mut self, action: SoundLoopAction) -> Option<u64> {
        match action {
            SoundLoopAction::Start { volume, .. } => {
                let voice = self.next_voice_id();
                self.voice = Some(voice);
                let _ = volume;
                Some(voice)
            }
            SoundLoopAction::Update { voice, .. } => Some(voice),
            SoundLoopAction::Stop { voice } => {
                self.voice = None;
                Some(voice)
            }
            SoundLoopAction::None => None,
        }
    }

    fn next_voice_id(&self) -> u64 {
        // Deterministic per-key voice id for the pure event path; the client
        // replaces this with the real pool id on `Start`.
        self.key
    }

    /// `SoundControl.stop`.
    pub fn stop(&mut self) -> Option<u64> {
        if self.voice.is_some() {
            self.volume = 0.0;
            return self.voice.take();
        }
        None
    }

    /// Builds the `LoopInstance` event for this instance.
    pub fn event(&self, x: f32, y: f32, play: bool, volume_scl: f32) -> AudioEvent {
        AudioEvent::loop_instance(self.key, self.sound, x, y, play, volume_scl)
    }
}

/// Upstream `AmbientSource` (position + sound query).
pub trait AmbientProvider {
    /// Whether the source is still valid (`isValid`).
    fn is_valid(&self) -> bool;
    /// Whether it should currently emit ambient sound.
    fn should_ambient_sound(&self) -> bool;
    /// Ambient volume.
    fn ambient_volume(&self) -> f32;
    /// Ambient sound.
    fn ambient_sound(&self) -> SoundId;
    /// World position.
    fn position(&self) -> (f32, f32);
}

/// One pre-resolved ambient source in a snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbientEntry {
    /// Ambient sound.
    pub sound: SoundId,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Volume.
    pub volume: f32,
}

/// A main-thread-built snapshot of live ambient sources.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AmbientSnapshot {
    /// Entries in deterministic (query) order.
    pub entries: Vec<AmbientEntry>,
}

impl AmbientSnapshot {
    /// Rebuilds the snapshot from providers (invalid entries are dropped, which
    /// mirrors `isValid()` swap-removal). Allocation is reused.
    pub fn build_from<'a, I>(&mut self, providers: I)
    where
        I: IntoIterator<Item = &'a dyn AmbientProvider>,
    {
        self.entries.clear();
        for provider in providers {
            if !provider.is_valid() || !provider.should_ambient_sound() {
                continue;
            }
            let volume = provider.ambient_volume();
            if volume <= MIN_LOOP_VOLUME {
                continue;
            }
            let sound = provider.ambient_sound();
            if sound.is_none_or_unset() {
                continue;
            }
            let (x, y) = provider.position();
            self.entries.push(AmbientEntry {
                sound,
                x,
                y,
                volume,
            });
        }
    }
}

/// One per-sound ambient aggregate (`AudioThread.SoundData` output).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AmbientSoundData {
    /// Sound.
    pub sound: SoundId,
    /// Accumulated volume.
    pub volume: f32,
    /// Accumulated weighted pitch.
    pub pitch: f32,
    /// Sum of base falloff.
    pub total: f32,
    /// Sum of weighted volume.
    pub total_volume: f32,
    /// Weighted x centroid.
    pub sum_x: f32,
    /// Weighted y centroid.
    pub sum_y: f32,
}

/// 20 Hz ambient poller (main-thread default; plan 18 §3.8, OD-18-A).
#[derive(Debug, Clone)]
pub struct AmbientPoller {
    /// Poll interval in ms (50 = 20 FPS).
    pub interval_ms: f64,
    /// Accumulated time since the last poll.
    pub accum_ms: f64,
    /// Total polls performed.
    pub polls: u64,
    scratch: IndexMap<SoundId, AmbientSoundData>,
    output: Vec<AmbientSoundData>,
}

impl Default for AmbientPoller {
    fn default() -> Self {
        AmbientPoller::new(50.0)
    }
}

impl AmbientPoller {
    /// Creates a poller at the given interval.
    pub fn new(interval_ms: f64) -> Self {
        AmbientPoller {
            interval_ms,
            accum_ms: 0.0,
            polls: 0,
            scratch: IndexMap::with_capacity(16),
            output: Vec::with_capacity(16),
        }
    }

    /// Advances the clock and, when due, runs `AudioThread.doLoop`.
    ///
    /// `is_playing` is the upstream `state.isPlaying()` gate (menu breaks the
    /// thread). Returns the aggregate list (empty when not due).
    pub fn update(
        &mut self,
        delta_ms: f64,
        is_menu: bool,
        is_playing: bool,
        snapshot: &AmbientSnapshot,
        listener: Listener,
    ) -> &[AmbientSoundData] {
        if is_menu {
            self.output.clear();
            return &self.output;
        }
        if !is_playing {
            self.output.clear();
            return &self.output;
        }
        self.accum_ms += delta_ms;
        if self.accum_ms < self.interval_ms {
            self.output.clear();
            return &self.output;
        }
        self.accum_ms -= self.interval_ms;
        self.polls += 1;
        self.do_loop(snapshot, listener)
    }

    fn do_loop(&mut self, snapshot: &AmbientSnapshot, listener: Listener) -> &[AmbientSoundData] {
        self.scratch.clear();
        self.output.clear();

        for entry in &snapshot.entries {
            let data = self
                .scratch
                .entry(entry.sound)
                .or_insert_with(|| AmbientSoundData {
                    sound: entry.sound,
                    ..AmbientSoundData::default()
                });
            let silent = data.volume == 0.0;

            let base_vol = calc_falloff(entry.x, entry.y, listener, 0.0);
            let vol = base_vol * entry.volume;
            data.volume += vol;
            data.pitch += 1.0 * vol;
            data.volume = clamp01(data.volume);
            data.total += base_vol;
            data.total_volume += vol;
            data.sum_x += entry.x * base_vol;
            data.sum_y += entry.y * base_vol;

            if silent && data.volume > 0.0 && !self.output.iter().any(|o| o.sound == entry.sound) {
                self.output.push(data.clone());
            }
        }
        &self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingLoopOutput {
        voices: Vec<(u64, SoundId, f32, f32)>,
        stopped: Vec<u64>,
        playing: Vec<u64>,
        next: u64,
    }

    impl LoopOutput for RecordingLoopOutput {
        fn start_loop(&mut self, sound: SoundId, volume: f32, pitch: f32, pan: f32) -> u64 {
            self.next += 1;
            let id = self.next;
            self.voices.push((id, sound, volume, pan));
            self.playing.push(id);
            let _ = pitch;
            id
        }
        fn update_loop(&mut self, voice: u64, volume: f32, _pitch: f32, pan: f32) {
            self.voices.push((voice, SoundId::NONE, volume, pan));
        }
        fn stop_loop(&mut self, voice: u64) {
            self.stopped.push(voice);
            self.playing.retain(|id| *id != voice);
        }
        fn is_playing(&self, voice: u64) -> bool {
            self.playing.contains(&voice)
        }
    }

    fn listener() -> Listener {
        Listener::new(0.0, 0.0, 100.0)
    }

    #[test]
    fn aggregation_math() {
        let mut mixer = LoopMixer::new();
        // Two sources of the same sound at the same point.
        mixer.accumulate(SoundId::LOOP_CONVEYOR, 0.0, 0.0, 0.5, 1.0, listener());
        mixer.accumulate(SoundId::LOOP_CONVEYOR, 0.0, 0.0, 0.3, 1.0, listener());
        let data = mixer.data(SoundId::LOOP_CONVEYOR).unwrap();
        // base falloff = 1 at the listener; volume = 0.5 + 0.3 clamped.
        assert!((data.volume - 0.8).abs() < 1.0e-6);
        assert!((data.total - 2.0).abs() < 1.0e-6);
        assert!((data.total_volume - 0.8).abs() < 1.0e-6);
        assert!((data.pitch - 0.8).abs() < 1.0e-6);
        assert_eq!(data.sum_x, 0.0);
    }

    #[test]
    fn weighted_centroid_pan() {
        let mut mixer = LoopMixer::new();
        mixer.accumulate(SoundId::LOOP_CONVEYOR, 25.0, 0.0, 1.0, 1.0, listener());
        mixer.accumulate(SoundId::LOOP_CONVEYOR, -25.0, 0.0, 1.0, 1.0, listener());
        let mut out = RecordingLoopOutput::default();
        // Pan is zero when the weighted centroid is centered.
        mixer.update(1.0, true, false, listener(), &mut out);
        let (_, _, _, pan) = out.voices[0];
        assert!(pan.abs() < 1.0e-6);
    }

    #[test]
    fn lerp_and_reset_and_voice_lifecycle() {
        let mut mixer = LoopMixer::new();
        let mut out = RecordingLoopOutput::default();
        mixer.accumulate(SoundId::LOOP_CONVEYOR, 0.0, 0.0, 1.0, 1.0, listener());
        // First update: cur = 0.11 -> starts a voice.
        mixer.update(1.0, true, false, listener(), &mut out);
        assert_eq!(out.voices.len(), 1);
        let data = mixer.data(SoundId::LOOP_CONVEYOR).unwrap();
        assert!((data.cur_volume - 0.11).abs() < 1.0e-6);
        assert!(data.voice.is_some());

        // Accumulators were reset after the pass.
        mixer.update(1.0, true, false, listener(), &mut out);
        let data = mixer.data(SoundId::LOOP_CONVEYOR).unwrap();
        // target volume 0 -> cur fades toward 0.
        assert!(data.cur_volume < 0.11);

        // Enough silent frames -> stop.
        for _ in 0..80 {
            mixer.update(1.0, true, false, listener(), &mut out);
        }
        assert!(!out.stopped.is_empty());
    }

    #[test]
    fn menu_clear_and_pause_freeze() {
        let mut mixer = LoopMixer::new();
        let mut out = RecordingLoopOutput::default();
        mixer.accumulate(SoundId::LOOP_CONVEYOR, 0.0, 0.0, 1.0, 1.0, listener());
        // Paused: no processing, data retained.
        mixer.update(1.0, true, true, listener(), &mut out);
        assert_eq!(mixer.len(), 1);
        assert!(out.voices.is_empty());
        // Menu: cleared entirely.
        mixer.update(1.0, false, false, listener(), &mut out);
        assert!(mixer.is_empty());
    }

    #[test]
    fn ambient_merge_silent_gate() {
        let mut mixer = LoopMixer::new();
        let provider = TestProvider {
            valid: true,
            sound: true,
            volume: 1.0,
            sound_id: SoundId::LOOP_HUM,
            pos: (0.0, 0.0),
        };
        let mut snapshot = AmbientSnapshot::default();
        snapshot.build_from([&provider as &dyn AmbientProvider]);
        assert_eq!(snapshot.entries.len(), 1);

        let mut poller = AmbientPoller::new(50.0);
        // Not due yet.
        assert!(
            poller
                .update(10.0, false, true, &snapshot, listener())
                .is_empty()
        );
        // Due: one aggregate (silent -> audible).
        let result = poller
            .update(50.0, false, true, &snapshot, listener())
            .to_vec();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].sound, SoundId::LOOP_HUM);
        assert!(result[0].volume > 0.0);
        mixer.merge_ambient(&result);
        assert!(mixer.data(SoundId::LOOP_HUM).is_some());

        // Invalid provider disappears (swap-removal equivalent).
        let invalid = TestProvider {
            valid: false,
            ..provider
        };
        let mut snapshot2 = AmbientSnapshot::default();
        snapshot2.build_from([&invalid as &dyn AmbientProvider]);
        assert!(snapshot2.entries.is_empty());

        // Menu/not-playing clear the output.
        assert!(
            poller
                .update(50.0, true, false, &snapshot, listener())
                .is_empty()
        );
    }

    #[derive(Clone, Copy)]
    struct TestProvider {
        valid: bool,
        sound: bool,
        volume: f32,
        sound_id: SoundId,
        pos: (f32, f32),
    }

    impl AmbientProvider for TestProvider {
        fn is_valid(&self) -> bool {
            self.valid
        }
        fn should_ambient_sound(&self) -> bool {
            self.sound
        }
        fn ambient_volume(&self) -> f32 {
            self.volume
        }
        fn ambient_sound(&self) -> SoundId {
            self.sound_id
        }
        fn position(&self) -> (f32, f32) {
            self.pos
        }
    }

    #[test]
    fn sound_loop_fade() {
        let mut state = SoundLoopState::new(SoundId::LOOP_DRILL, 1.0, 7);
        // Starts at volume 0 (fade in).
        let action = state.update(0.0, 0.0, true, 1.0, listener());
        assert!(matches!(action, SoundLoopAction::Start { volume, .. } if volume == 0.0));
        state.voice = Some(1);
        state.volume = 0.0;

        // Fades in at 0.05/frame.
        state.update(0.0, 0.0, true, 1.0, listener());
        assert!((state.volume - 0.05).abs() < 1.0e-6);
        for _ in 0..19 {
            state.update(0.0, 0.0, true, 1.0, listener());
        }
        assert!((state.volume - 1.0).abs() < 1.0e-6);

        // Fades out and stops at <= 0.001.
        let mut stopped = false;
        for _ in 0..25 {
            if let SoundLoopAction::Stop { .. } = state.update(0.0, 0.0, false, 1.0, listener()) {
                stopped = true;
                break;
            }
        }
        assert!(stopped);
        assert!(state.voice.is_none());

        // baseVolume <= 0 -> no-op.
        let mut state = SoundLoopState::new(SoundId::LOOP_DRILL, 0.0, 8);
        assert_eq!(
            state.update(0.0, 0.0, true, 1.0, listener()),
            SoundLoopAction::None
        );

        // Event shape.
        let state = SoundLoopState::new(SoundId::LOOP_MINE_BEAM, 0.8, 17);
        let event = state.event(520.0, 300.0, true, 0.8);
        assert_eq!(
            event,
            AudioEvent::loop_instance(17, SoundId::LOOP_MINE_BEAM, 520.0, 300.0, true, 0.8)
        );
    }

    #[test]
    fn zero_volume_poll_no_output() {
        let provider = TestProvider {
            valid: true,
            sound: true,
            volume: 0.0,
            sound_id: SoundId::LOOP_HUM,
            pos: (0.0, 0.0),
        };
        let mut snapshot = AmbientSnapshot::default();
        snapshot.build_from([&provider as &dyn AmbientProvider]);
        assert!(snapshot.entries.is_empty());
    }
}

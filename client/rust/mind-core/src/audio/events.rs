// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `AudioSink` event boundary (plan 18 §3.3).
//!
//! Sim code emits [`AudioEvent`]s unconditionally; headless/dedicated installs
//! [`NoopAudioSink`], the client installs `mind-gdext`'s `GodotAudioSink`, and
//! tests/`mind-headless audio *` use [`RecordingAudioSink`] as the deterministic
//! oracle. Event variants are **append-only**.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::ids::{MusicRef, SoundId, VoiceKey};

/// One audio event emitted by sim/UI code.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioEvent {
    /// `Sound.at(x, y, pitch, volume)` with `checkFrame`; world px.
    At {
        /// Sound to play.
        sound: SoundId,
        /// World x (px).
        x: f32,
        /// World y (px).
        y: f32,
        /// Pitch multiplier.
        pitch: f32,
        /// Raw per-call volume (the client applies `calcFalloff * sfxvol`).
        volume: f32,
        /// Whether the 16 ms same-frame intensification applies.
        check_frame: bool,
    },
    /// `Sound.play(volume, pitch, pan, loop, checkFrame)`; volume already final.
    Play {
        /// Sound to play.
        sound: SoundId,
        /// Final volume (call site applied `sfxvol` where upstream did).
        volume: f32,
        /// Pitch multiplier.
        pitch: f32,
        /// Pan `[-1, 1]`.
        pan: f32,
        /// Whether the voice loops.
        #[serde(rename = "loop")]
        loop_: bool,
        /// `checkFrame` flag.
        check_frame: bool,
    },
    /// `SoundControl.loop(sound, pos, volume[, pitch])` aggregation input.
    LoopAdd {
        /// Looped sound.
        sound: SoundId,
        /// World x (px).
        x: f32,
        /// World y (px).
        y: f32,
        /// Call-site volume.
        volume: f32,
        /// Pitch multiplier.
        pitch: f32,
    },
    /// `SoundLoop.update(...)` — per-instance loop with a stable voice key.
    LoopInstance {
        /// Stable per-instance voice key.
        key: VoiceKey,
        /// Looped sound.
        sound: SoundId,
        /// World x (px).
        x: f32,
        /// World y (px).
        y: f32,
        /// Whether the loop should be playing.
        play: bool,
        /// Call-site volume scale.
        volume_scl: f32,
    },
    /// Weather/other camera-centred loops.
    LoopCamera {
        /// Looped sound.
        sound: SoundId,
        /// Volume.
        volume: f32,
    },
    /// Music control from mlog / map scripts (`findMusic` + `playMusic`).
    MusicPlay {
        /// Music to play.
        name: MusicRef,
        /// Whether to interrupt the current track.
        interrupt: bool,
    },
    /// Stop music (`playMusic(null ...)` / `stopMusic`).
    MusicStop,
    /// `SoundControl.keepSilent()`.
    KeepSilent,
    /// Stop all loop voices (`ResetEvent` teardown).
    StopLoops,
}

impl AudioEvent {
    /// `Sound.at(...)` constructor.
    #[allow(clippy::too_many_arguments)]
    pub fn at(sound: SoundId, x: f32, y: f32, pitch: f32, volume: f32, check_frame: bool) -> Self {
        AudioEvent::At {
            sound,
            x,
            y,
            pitch,
            volume,
            check_frame,
        }
    }

    /// `Sound.play(...)` constructor.
    pub fn play(
        sound: SoundId,
        volume: f32,
        pitch: f32,
        pan: f32,
        loop_: bool,
        check_frame: bool,
    ) -> Self {
        AudioEvent::Play {
            sound,
            volume,
            pitch,
            pan,
            loop_,
            check_frame,
        }
    }

    /// `SoundControl.loop(...)` constructor.
    pub fn loop_add(sound: SoundId, x: f32, y: f32, volume: f32, pitch: f32) -> Self {
        AudioEvent::LoopAdd {
            sound,
            x,
            y,
            volume,
            pitch,
        }
    }

    /// `SoundLoop.update(...)` constructor.
    pub fn loop_instance(
        key: VoiceKey,
        sound: SoundId,
        x: f32,
        y: f32,
        play: bool,
        volume_scl: f32,
    ) -> Self {
        AudioEvent::LoopInstance {
            key,
            sound,
            x,
            y,
            play,
            volume_scl,
        }
    }

    /// Weather `LoopCamera` constructor.
    pub fn loop_camera(sound: SoundId, volume: f32) -> Self {
        AudioEvent::LoopCamera { sound, volume }
    }

    /// `findMusic` + `playMusic` constructor.
    pub fn music_play(name: MusicRef, interrupt: bool) -> Self {
        AudioEvent::MusicPlay { name, interrupt }
    }

    /// Sound id carried by this event, if any.
    pub fn sound(&self) -> Option<SoundId> {
        match self {
            AudioEvent::At { sound, .. }
            | AudioEvent::Play { sound, .. }
            | AudioEvent::LoopAdd { sound, .. }
            | AudioEvent::LoopInstance { sound, .. }
            | AudioEvent::LoopCamera { sound, .. } => Some(*sound),
            _ => None,
        }
    }
}

/// Receives audio events. Implementations gate playback (no-op headless).
pub trait AudioSink: Send {
    /// Emits one event. Must not panic.
    fn emit(&mut self, event: AudioEvent);
}

/// Headless/dedicated-server sink (the D1 default).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopAudioSink;

impl AudioSink for NoopAudioSink {
    fn emit(&mut self, _event: AudioEvent) {}
}

/// An event stamped with the sim tick it was emitted on.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TickedAudioEvent {
    /// `SimClock::tick` at emission time.
    pub tick: u64,
    /// Emission order within the tick.
    pub order: u64,
    /// The event.
    #[serde(flatten)]
    pub event: AudioEvent,
}

/// Deterministic event recorder used as the headless oracle (plan 18 §3.3).
#[derive(Debug, Default, Clone)]
pub struct RecordingAudioSink {
    /// Recorded events in emission order.
    pub events: Vec<TickedAudioEvent>,
    tick: u64,
    order: u64,
    /// Per-name missing-stream warnings (missing audio logs once; §3.2).
    pub missing: Vec<String>,
}

impl RecordingAudioSink {
    /// Creates an empty recorder.
    pub fn new() -> Self {
        RecordingAudioSink::default()
    }

    /// Sets the tick stamped on subsequent emissions.
    pub fn set_tick(&mut self, tick: u64) {
        self.tick = tick;
        self.order = 0;
    }

    /// Clears recorded events (keeps the current tick).
    pub fn clear(&mut self) {
        self.events.clear();
        self.order = 0;
    }

    /// Records a missing-audio warning once per distinct name.
    pub fn record_missing(&mut self, name: &str) -> bool {
        if self.missing.iter().any(|existing| existing == name) {
            return false;
        }
        self.missing.push(name.to_owned());
        true
    }

    /// Serializes the events to the `audio events` dump JSON (`format: 1`).
    pub fn dump_json(&self) -> String {
        #[derive(Serialize)]
        struct Dump<'a> {
            format: u32,
            tick: u64,
            events: &'a [TickedAudioEvent],
        }
        serde_json::to_string(&Dump {
            format: 1,
            tick: self.tick,
            events: &self.events,
        })
        .unwrap_or_else(|_| String::from("{}"))
    }
}

impl AudioSink for RecordingAudioSink {
    fn emit(&mut self, event: AudioEvent) {
        let order = self.order;
        self.order += 1;
        self.events.push(TickedAudioEvent {
            tick: self.tick,
            order,
            event,
        });
    }
}

/// Shared, cheaply-cloneable [`RecordingAudioSink`].
///
/// The harness installs one clone as the active [`AudioSink`] and keeps another
/// to drain the recorded events after a scenario runs (plan 18 §3.3/§7b). The
/// interior `Mutex` also lets the ECS resource and the sim call sites share one
/// log without changing the [`AudioSink`] trait.
#[derive(Debug, Clone, Default)]
pub struct SharedAudioLog(Arc<Mutex<RecordingAudioSink>>);

impl SharedAudioLog {
    /// Creates an empty shared log.
    pub fn new() -> Self {
        SharedAudioLog(Arc::new(Mutex::new(RecordingAudioSink::new())))
    }

    fn with<R>(&self, f: impl FnOnce(&RecordingAudioSink) -> R) -> Option<R> {
        self.0.lock().ok().map(|guard| f(&guard))
    }

    /// Snapshot of recorded events in emission order.
    pub fn events(&self) -> Vec<TickedAudioEvent> {
        self.with(|sink| sink.events.clone()).unwrap_or_default()
    }

    /// Number of recorded events.
    pub fn len(&self) -> usize {
        self.with(|sink| sink.events.len()).unwrap_or(0)
    }

    /// Whether nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears recorded events.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.0.lock() {
            guard.clear();
        }
    }

    /// Sets the tick stamped on subsequent emissions.
    pub fn set_tick(&self, tick: u64) {
        if let Ok(mut guard) = self.0.lock() {
            guard.set_tick(tick);
        }
    }

    /// Distinct missing-audio names recorded so far.
    pub fn missing(&self) -> Vec<String> {
        self.with(|sink| sink.missing.clone()).unwrap_or_default()
    }

    /// Serializes the recorded events in the `audio events` dump format.
    pub fn dump_json(&self) -> String {
        self.with(RecordingAudioSink::dump_json)
            .unwrap_or_else(|| String::from("{}"))
    }
}

impl AudioSink for SharedAudioLog {
    fn emit(&mut self, event: AudioEvent) {
        if let Ok(mut guard) = self.0.lock() {
            guard.emit(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_sink_stamps_and_dumps() {
        let mut sink = RecordingAudioSink::new();
        sink.set_tick(417);
        sink.emit(AudioEvent::at(
            SoundId::SHOOT_DUO,
            512.0,
            288.0,
            1.03,
            1.0,
            true,
        ));
        sink.emit(AudioEvent::loop_add(
            SoundId::LOOP_CONVEYOR,
            520.0,
            300.0,
            1.0,
            1.0,
        ));
        assert_eq!(sink.events.len(), 2);
        assert_eq!(sink.events[0].tick, 417);
        assert_eq!(sink.events[0].order, 0);
        assert_eq!(sink.events[1].order, 1);
        let json = sink.dump_json();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["format"], 1);
        assert_eq!(value["events"][0]["kind"], "at");
        assert_eq!(value["events"][0]["sound"], "shootDuo");
        assert_eq!(value["events"][1]["kind"], "loop_add");
    }

    #[test]
    fn sound_id_serde_by_name() {
        for id in [SoundId::NONE, SoundId::UNSET, SoundId::SHOOT_DUO] {
            let text = serde_json::to_string(&id).unwrap();
            let back: SoundId = serde_json::from_str(&text).unwrap();
            assert_eq!(id, back);
        }
        assert_eq!(serde_json::to_string(&SoundId::NONE).unwrap(), "\"none\"");
        assert_eq!(SoundId::by_name("shootDuo"), Some(SoundId::SHOOT_DUO));
    }
}

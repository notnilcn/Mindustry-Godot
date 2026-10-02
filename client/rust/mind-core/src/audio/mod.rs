// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Audio core (plan 18): the Godot-free `SoundControl`/`SoundPriority`/
//! `AmbientSource`/`SoundLoop`/`MusicContainer` semantics plus the `AudioSink`
//! event boundary.
//!
//! Playback/streams/buses live in `mind-gdext`; this module stays Godot-free and
//! tokio-free (HLP §2.2). Sim code emits [`AudioEvent`]s through the installed
//! [`AudioSink`]; the headless/dedicated default is [`NoopAudioSink`].

pub mod events;
pub mod ids;
pub mod loops;
pub mod math;
pub mod music;
pub mod priority;
pub mod sim;

use std::sync::{Arc, Mutex};

use bevy_ecs::world::World;

pub use events::{
    AudioEvent, AudioSink, NoopAudioSink, RecordingAudioSink, SharedAudioLog, TickedAudioEvent,
};
pub use ids::{
    BusKind, MusicCatalog, MusicRef, SoundId, VoiceKey, audition_name, bus_for_sound,
    find_music_chain, settings_keys,
};
pub use loops::{
    AmbientEntry, AmbientPoller, AmbientProvider, AmbientSnapshot, AmbientSoundData, LoopData,
    LoopMixer, LoopOutput, SoundLoopAction, SoundLoopState,
};
pub use math::{
    AudioRng, DEFAULT_SOUND_MAX_CONCURRENT, FALLOFF, FIN_TIME, FOUT_TIME, Listener,
    MIN_INTERVAL_MS, MUSIC_CHANCE, MUSIC_INTERVAL, MUSIC_WAVE_CHANCE, SeededAudioRng, calc_falloff,
    calc_pan, clamp01, lerp_delta, random_index_excluding, sanitize,
};
pub use music::{MusicContext, MusicOutput, MusicPlayer, MusicRules, PlanetMusic};
pub use priority::{
    ActiveVoice, Admission, PlayRequest, SoundPlayState, SoundPrioritySpec, SoundPriorityTable,
    admit,
};

/// Bevy resource holding the active [`AudioSink`] behind a shared `Mutex` (the
/// sink itself only needs `Send`; the client may use non-`Sync` internals).
///
/// `Arc`-backed and `Clone` so harnesses can keep a handle outside the ECS
/// world while also inserting it for `&mut World` sim systems (unit lifecycle).
#[derive(Clone, bevy_ecs::resource::Resource)]
pub struct AudioSinkRes(Arc<Mutex<Box<dyn AudioSink>>>);

impl AudioSinkRes {
    /// Wraps an installed sink.
    pub fn new(sink: impl AudioSink + 'static) -> Self {
        AudioSinkRes(Arc::new(Mutex::new(Box::new(sink))))
    }

    /// The headless/dedicated default.
    pub fn noop() -> Self {
        AudioSinkRes::new(NoopAudioSink)
    }

    /// Emits an event on the installed sink (no-op if the lock is poisoned).
    pub fn emit(&self, event: AudioEvent) {
        if let Ok(mut sink) = self.0.lock() {
            sink.emit(event);
        }
    }
}

/// Installs the active [`AudioSink`] as an ECS resource (plan 18 §3.3).
pub struct AudioPlugin {
    sink: Box<dyn AudioSink>,
}

impl AudioPlugin {
    /// Creates a plugin that installs `sink`.
    pub fn new(sink: impl AudioSink + 'static) -> Self {
        AudioPlugin {
            sink: Box::new(sink),
        }
    }

    /// Installs the sink into the world (idempotent: replaces any previous sink).
    pub fn install(self, world: &mut World) {
        world.remove_resource::<AudioSinkRes>();
        world.insert_resource(AudioSinkRes(Arc::new(Mutex::new(self.sink))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_installs_and_emits() {
        let mut world = World::new();
        AudioPlugin::new(RecordingAudioSink::new()).install(&mut world);
        let sink = world.resource::<AudioSinkRes>();
        sink.emit(AudioEvent::play(
            SoundId::UI_BUTTON,
            1.0,
            1.0,
            0.0,
            false,
            false,
        ));
        // Re-install replaces cleanly.
        AudioPlugin::new(NoopAudioSink).install(&mut world);
        assert!(world.get_resource::<AudioSinkRes>().is_some());
    }

    #[test]
    fn noop_sink_is_a_valid_default() {
        let mut sink = NoopAudioSink;
        sink.emit(AudioEvent::MusicStop);
    }

    #[test]
    fn shared_log_records_and_drains() {
        let log = SharedAudioLog::new();
        let mut sink = log.clone();
        sink.set_tick(9);
        sink.emit(AudioEvent::music_play(MusicRef::new("game1"), true));
        assert_eq!(log.len(), 1);
        assert_eq!(log.events()[0].tick, 9);
        log.clear();
        assert!(log.is_empty());
    }

    /// Plan 18 §7d steady-state allocation audit for loop aggregation + the
    /// 20 Hz ambient poll. Run in isolation:
    /// `cargo test -p mind-core --features alloc-audit audio::tests::steady_state_audio_allocates_nothing -- --test-threads=1`.
    #[cfg(feature = "alloc-audit")]
    #[test]
    fn steady_state_audio_allocates_nothing() {
        use crate::util::alloc::alloc_count;

        struct NullLoopOut;
        impl loops::LoopOutput for NullLoopOut {
            fn start_loop(&mut self, _sound: SoundId, _v: f32, _p: f32, _pan: f32) -> u64 {
                1
            }
            fn update_loop(&mut self, _voice: u64, _v: f32, _p: f32, _pan: f32) {}
            fn stop_loop(&mut self, _voice: u64) {}
            fn is_playing(&self, _voice: u64) -> bool {
                false
            }
        }

        struct Provider {
            sound: SoundId,
        }
        impl AmbientProvider for Provider {
            fn is_valid(&self) -> bool {
                true
            }
            fn should_ambient_sound(&self) -> bool {
                true
            }
            fn ambient_volume(&self) -> f32 {
                1.0
            }
            fn ambient_sound(&self) -> SoundId {
                self.sound
            }
            fn position(&self) -> (f32, f32) {
                (0.0, 0.0)
            }
        }

        const SOUNDS: [SoundId; 3] = [SoundId::LOOP_CONVEYOR, SoundId::LOOP_DRILL, SoundId::RAIN];
        let providers: Vec<Provider> = SOUNDS
            .iter()
            .map(|sound| Provider { sound: *sound })
            .collect();
        let refs: Vec<&dyn AmbientProvider> = providers
            .iter()
            .map(|p| p as &dyn AmbientProvider)
            .collect();
        let mut snapshot = AmbientSnapshot::default();
        snapshot.build_from(refs);
        let listener = math::Listener::new(0.0, 0.0, 100.0);
        let mut mixer = LoopMixer::new();
        let mut out = NullLoopOut;
        let mut poller = AmbientPoller::new(50.0);
        let mut scratch: Vec<AmbientSoundData> = Vec::with_capacity(16);

        let frame = |mixer: &mut LoopMixer,
                     out: &mut NullLoopOut,
                     poller: &mut AmbientPoller,
                     scratch: &mut Vec<AmbientSoundData>| {
            for sound in SOUNDS {
                mixer.accumulate(sound, 8.0, 8.0, 0.8, 1.0, listener);
            }
            scratch.clear();
            scratch.extend_from_slice(poller.update(
                1000.0 / 60.0,
                false,
                true,
                &snapshot,
                listener,
            ));
            mixer.merge_ambient(scratch);
            mixer.update(1.0, true, false, listener, out);
        };

        for _ in 0..1000 {
            frame(&mut mixer, &mut out, &mut poller, &mut scratch);
        }
        let before = alloc_count();
        for _ in 0..1000 {
            frame(&mut mixer, &mut out, &mut poller, &mut scratch);
        }
        let after = alloc_count();
        assert_eq!(
            after,
            before,
            "steady-state audio allocated {} times",
            after - before
        );
    }
}

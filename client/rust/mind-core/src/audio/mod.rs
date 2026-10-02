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

use std::sync::Mutex;

use bevy_ecs::world::World;

pub use events::{AudioEvent, AudioSink, NoopAudioSink, RecordingAudioSink, TickedAudioEvent};
pub use ids::{
    BusKind, MusicCatalog, MusicRef, SoundId, VoiceKey, bus_for_sound, find_music_chain,
    settings_keys,
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

/// Bevy resource holding the active [`AudioSink`] behind a `Mutex` (the sink
/// itself only needs `Send`; the client may use non-`Sync` internals).
#[derive(bevy_ecs::resource::Resource)]
pub struct AudioSinkRes(pub Mutex<Box<dyn AudioSink>>);

impl AudioSinkRes {
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
        world.insert_resource(AudioSinkRes(Mutex::new(self.sink)));
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
}

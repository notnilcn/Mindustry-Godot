// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Bus layout ported from `SoundControl.java` / `SoundPriority.java` / Arc `AudioBus`.

//! Godot bus layout `Master → Sound/Music/UI` with the `Sound` lowpass slot
//! (plan 18 §3.5). Created programmatically at `MindAudio` ready; verified by
//! the §7c MCP checks and documented in `client/audio/bus_layout.json`.

use godot::classes::{AudioEffect, AudioEffectLowPassFilter, AudioServer};
use godot::obj::Singleton;
use godot::prelude::*;

/// Dry lowpass cutoff (wet = 0).
pub const LOWPASS_DRY_HZ: f32 = 20500.0;
/// Wet lowpass cutoff (wet = 1).
pub const LOWPASS_WET_HZ: f32 = 500.0;

/// Resolved bus indices.
#[derive(Debug, Clone, Copy, Default)]
pub struct BusLayout {
    /// `Sound` bus.
    pub sound: i32,
    /// `Music` bus.
    pub music: i32,
    /// `UI` bus.
    pub ui: i32,
}

impl BusLayout {
    /// Whether every bus resolved.
    pub fn is_ready(&self) -> bool {
        self.sound > 0 && self.music > 0 && self.ui > 0
    }
}

fn ensure_bus(server: &mut Gd<AudioServer>, name: &str, send: &str) -> i32 {
    let index = server.get_bus_index(&StringName::from(name));
    if index >= 0 {
        return index;
    }
    server.add_bus();
    let index = server.get_bus_count() - 1;
    server.set_bus_name(index, &GString::from(name));
    server.set_bus_send(index, &StringName::from(send));
    index
}

/// Creates the layout if missing and installs the `Sound` lowpass filter.
pub fn ensure_buses() -> BusLayout {
    let mut server = AudioServer::singleton();
    let layout = BusLayout {
        sound: ensure_bus(&mut server, "Sound", "Master"),
        music: ensure_bus(&mut server, "Music", "Master"),
        ui: ensure_bus(&mut server, "UI", "Master"),
    };

    // Slot 0 on `Sound`: `AudioEffectLowPassFilter` (plan 18 deviation 3).
    if layout.sound >= 0 && server.get_bus_effect_count(layout.sound) == 0 {
        let mut filter = AudioEffectLowPassFilter::new_gd();
        filter.set_cutoff(LOWPASS_DRY_HZ);
        filter.set_resonance(1.0);
        let effect: Gd<AudioEffect> = filter.upcast();
        server.add_bus_effect(layout.sound, &effect);
    }
    layout
}

/// Returns the `Sound`-bus lowpass filter if present.
pub fn sound_lowpass(
    server: &mut Gd<AudioServer>,
    sound_bus: i32,
) -> Option<Gd<AudioEffectLowPassFilter>> {
    if sound_bus < 0 || server.get_bus_effect_count(sound_bus) == 0 {
        return None;
    }
    server
        .get_bus_effect(sound_bus, 0)
        .and_then(|effect| effect.try_cast::<AudioEffectLowPassFilter>().ok())
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Sim-side audio call sites (plan 18 §2.3, §3.3).
//!
//! Ports the `Sound.at(...)` call sites that live inside gameplay code
//! (`Block.init` defaults, `Turret.shoot`, `BulletType.hit/despawned`,
//! `UnitComp` death/wreck) into small, pure emitters over the [`AudioSink`]
//! boundary. Sim code calls these; the sink decides whether anything is audible
//! (`NoopAudioSink` headless, `GodotAudioSink` client). Emission is
//! deterministic and never touches sim state, so checksums are unaffected.
//!
//! Volume semantics follow plan 18 §3.3: [`AudioEvent::At`] carries the raw
//! per-call volume and the client applies `calcFalloff * sfxvol`.

use super::AudioSinkRes;
use super::events::{AudioEvent, AudioSink};
use super::ids::SoundId;

/// `Block.init` default place sound by block size (`Block.java` 1367-1372).
pub fn block_place_sound(size: i32) -> SoundId {
    if size >= 3 {
        SoundId::BLOCK_PLACE3
    } else if size >= 2 {
        SoundId::BLOCK_PLACE2
    } else {
        SoundId::BLOCK_PLACE1
    }
}

/// `Block.init` default break sound by block size (`Block.java` 1374-1379).
pub fn block_break_sound(size: i32) -> SoundId {
    if size >= 3 {
        SoundId::BLOCK_BREAK3
    } else if size >= 2 {
        SoundId::BLOCK_BREAK2
    } else {
        SoundId::BLOCK_BREAK1
    }
}

/// `Block.init` default destroy sound by block size (`Block.java` 1360-1365).
///
/// Upstream wraps the size-2/1 variants in a `RandomSound`; the deterministic
/// port picks the primary variant and leaves the alternate to the sim RNG
/// (deferred: alternate selection is not yet wired — documented in the plan).
pub fn block_destroy_sound(size: i32) -> SoundId {
    if size >= 3 {
        SoundId::BLOCK_EXPLODE3
    } else if size >= 2 {
        SoundId::BLOCK_EXPLODE2
    } else {
        SoundId::BLOCK_EXPLODE1
    }
}

/// Emits `Sound.at` for a block placement.
pub fn emit_block_place(audio: &AudioSinkRes, size: i32, x: f32, y: f32, pitch: f32) {
    emit_at(audio, block_place_sound(size), x, y, 1.0, pitch, true);
}

/// Emits `Sound.at` for a block break/deconstruct.
pub fn emit_block_break(audio: &AudioSinkRes, size: i32, x: f32, y: f32, pitch: f32) {
    emit_at(audio, block_break_sound(size), x, y, 1.0, pitch, true);
}

/// Emits `Sound.at` for a block destruction (`destroySound` + volume).
pub fn emit_block_destroy(
    audio: &AudioSinkRes,
    size: i32,
    x: f32,
    y: f32,
    volume: f32,
    pitch: f32,
) {
    emit_at(audio, block_destroy_sound(size), x, y, volume, pitch, true);
}

/// Emits the turret shoot sound (`Turret.shoot` `shootSound.at`, `Turret.java`
/// 804): position is the bullet spawn, pitch is `random(soundPitchMin, max)`.
pub fn emit_turret_shoot(
    audio: &AudioSinkRes,
    sound: SoundId,
    x: f32,
    y: f32,
    volume: f32,
    pitch: f32,
) {
    emit_at(audio, sound, x, y, volume, pitch, true);
}

/// Emits a bullet hit sound (`BulletType.hit`).
pub fn emit_bullet_hit(
    audio: &AudioSinkRes,
    sound: SoundId,
    x: f32,
    y: f32,
    volume: f32,
    pitch: f32,
) {
    emit_at(audio, sound, x, y, volume, pitch, true);
}

/// Emits a bullet despawn sound (`BulletType.despawned`).
pub fn emit_bullet_despawn(
    audio: &AudioSinkRes,
    sound: SoundId,
    x: f32,
    y: f32,
    volume: f32,
    pitch: f32,
) {
    emit_at(audio, sound, x, y, volume, pitch, true);
}

/// Emits a unit death sound (`UnitType.deathSound`).
pub fn emit_unit_death(audio: &AudioSinkRes, sound: SoundId, x: f32, y: f32, volume: f32) {
    emit_at(audio, sound, x, y, volume, 1.0, false);
}

/// Emits a unit wreck sound (`UnitType.wreckSound`).
pub fn emit_unit_wreck(audio: &AudioSinkRes, sound: SoundId, x: f32, y: f32, volume: f32) {
    emit_at(audio, sound, x, y, volume, 1.0, false);
}

/// Emits a unit step sound (`UnitType.stepSound`).
pub fn emit_unit_step(
    audio: &AudioSinkRes,
    sound: SoundId,
    x: f32,
    y: f32,
    volume: f32,
    pitch: f32,
) {
    emit_at(audio, sound, x, y, volume, pitch, true);
}

/// Emits an aggregated positional loop input (`SoundControl.loop`).
pub fn emit_loop_add(
    audio: &AudioSinkRes,
    sound: SoundId,
    x: f32,
    y: f32,
    volume: f32,
    pitch: f32,
) {
    if sound.is_none_or_unset() {
        return;
    }
    audio.emit(AudioEvent::loop_add(sound, x, y, volume, pitch));
}

/// Emits a `SoundControl.loop` aggregation row for a block/turret loop.
pub fn emit_loop_instance(
    audio: &AudioSinkRes,
    key: super::ids::VoiceKey,
    sound: SoundId,
    x: f32,
    y: f32,
    play: bool,
    volume_scl: f32,
) {
    if sound.is_none_or_unset() {
        return;
    }
    audio.emit(AudioEvent::loop_instance(
        key, sound, x, y, play, volume_scl,
    ));
}

/// Shared `Sound.at` body: skips the silent sentinels and emits an
/// [`AudioEvent::At`].
fn emit_at(
    audio: &AudioSinkRes,
    sound: SoundId,
    x: f32,
    y: f32,
    volume: f32,
    pitch: f32,
    check_frame: bool,
) {
    if sound.is_none_or_unset() {
        return;
    }
    audio.emit(AudioEvent::at(sound, x, y, pitch, volume, check_frame));
}

/// Emits a `Sound.at` directly on a plain sink (tests / non-ECS callers).
pub fn at(sink: &mut dyn AudioSink, sound: SoundId, x: f32, y: f32, pitch: f32, volume: f32) {
    if sound.is_none_or_unset() {
        return;
    }
    sink.emit(AudioEvent::at(sound, x, y, pitch, volume, true));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::SharedAudioLog;

    #[test]
    fn block_defaults_match_block_init() {
        assert_eq!(block_place_sound(1), SoundId::BLOCK_PLACE1);
        assert_eq!(block_place_sound(2), SoundId::BLOCK_PLACE2);
        assert_eq!(block_place_sound(3), SoundId::BLOCK_PLACE3);
        assert_eq!(block_break_sound(1), SoundId::BLOCK_BREAK1);
        assert_eq!(block_break_sound(2), SoundId::BLOCK_BREAK2);
        assert_eq!(block_break_sound(3), SoundId::BLOCK_BREAK3);
        assert_eq!(block_destroy_sound(1), SoundId::BLOCK_EXPLODE1);
        assert_eq!(block_destroy_sound(2), SoundId::BLOCK_EXPLODE2);
        assert_eq!(block_destroy_sound(3), SoundId::BLOCK_EXPLODE3);
    }

    #[test]
    fn sentinels_never_emit() {
        let log = SharedAudioLog::new();
        let audio = AudioSinkRes::new(log.clone());
        emit_at(&audio, SoundId::NONE, 1.0, 2.0, 1.0, 1.0, true);
        emit_loop_add(&audio, SoundId::UNSET, 1.0, 2.0, 1.0, 1.0);
        assert!(log.is_empty());
    }

    #[test]
    fn emits_expected_event_shapes() {
        let log = SharedAudioLog::new();
        let audio = AudioSinkRes::new(log.clone());
        emit_block_place(&audio, 1, 8.0, 8.0, 1.0);
        emit_turret_shoot(&audio, SoundId::SHOOT_DUO, 16.0, 16.0, 1.0, 1.03);
        emit_loop_add(&audio, SoundId::LOOP_CONVEYOR, 4.0, 4.0, 0.5, 1.0);
        let events = log.events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event.sound(), Some(SoundId::BLOCK_PLACE1));
        assert_eq!(events[1].event.sound(), Some(SoundId::SHOOT_DUO));
        assert_eq!(events[2].event.sound(), Some(SoundId::LOOP_CONVEYOR));
    }
}

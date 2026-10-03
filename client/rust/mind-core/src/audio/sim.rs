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
use super::math::AudioRng;
use crate::determinism::{RngStream, SimRng};

/// Deterministic [`AudioRng`] over the isolated sim `Fx` stream (plan 18 §2.4
/// deviation 9). `RandomSound` alternate selection is reproducible per sim seed
/// but never consumes the gameplay `RngStream::Sim`.
pub struct FxAudioRng<'a> {
    /// The sim RNG (only its `Fx` stream is read).
    pub rng: &'a mut SimRng,
}

impl AudioRng for FxAudioRng<'_> {
    fn next_f32(&mut self) -> f32 {
        self.rng.next_float(RngStream::Fx)
    }
}

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

/// Arc `RandomSound` candidates for `Block.destroySound` (`Block.java`
/// 1360-1364). Sizes 2/1 wrap a primary+alternate pair in an Arc `RandomSound`;
/// size 3+ is a single sound.
pub fn block_destroy_alternates(size: i32) -> &'static [SoundId] {
    if size >= 3 {
        &[SoundId::BLOCK_EXPLODE3]
    } else if size >= 2 {
        &[SoundId::BLOCK_EXPLODE2, SoundId::BLOCK_EXPLODE2_ALT]
    } else {
        &[SoundId::BLOCK_EXPLODE1, SoundId::BLOCK_EXPLODE1_ALT]
    }
}

/// Arc `RandomSound.play`: uniform selection over `candidates`. Never consumes
/// the gameplay RNG; callers pass [`FxAudioRng`] (sim) or the gdext wall-clock
/// [`super::math::SeededAudioRng`].
pub fn pick_random_sound(rng: &mut impl AudioRng, candidates: &[SoundId]) -> SoundId {
    if candidates.is_empty() {
        return SoundId::NONE;
    }
    let pick = (rng.next_f32() * candidates.len() as f32) as usize;
    candidates[pick.min(candidates.len() - 1)]
}

/// `Block.destroySound` with `RandomSound` alternate selection.
pub fn block_destroy_sound(rng: &mut impl AudioRng, size: i32) -> SoundId {
    pick_random_sound(rng, block_destroy_alternates(size))
}

/// Emits `Sound.at` for a block placement.
pub fn emit_block_place(audio: &AudioSinkRes, size: i32, x: f32, y: f32, pitch: f32) {
    emit_at(audio, block_place_sound(size), x, y, 1.0, pitch, true);
}

/// Emits `Sound.at` for a block break/deconstruct.
pub fn emit_block_break(audio: &AudioSinkRes, size: i32, x: f32, y: f32, pitch: f32) {
    emit_at(audio, block_break_sound(size), x, y, 1.0, pitch, true);
}

/// Emits `Sound.at` for a block destruction (`destroySound` + volume) with
/// `RandomSound` alternate selection (Arc `Block.destroySound`).
pub fn emit_block_destroy(
    audio: &AudioSinkRes,
    rng: &mut impl AudioRng,
    size: i32,
    x: f32,
    y: f32,
    volume: f32,
    pitch: f32,
) {
    emit_at(
        audio,
        block_destroy_sound(rng, size),
        x,
        y,
        volume,
        pitch,
        true,
    );
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
    use crate::audio::math::SeededAudioRng;

    #[test]
    fn block_defaults_match_block_init() {
        assert_eq!(block_place_sound(1), SoundId::BLOCK_PLACE1);
        assert_eq!(block_place_sound(2), SoundId::BLOCK_PLACE2);
        assert_eq!(block_place_sound(3), SoundId::BLOCK_PLACE3);
        assert_eq!(block_break_sound(1), SoundId::BLOCK_BREAK1);
        assert_eq!(block_break_sound(2), SoundId::BLOCK_BREAK2);
        assert_eq!(block_break_sound(3), SoundId::BLOCK_BREAK3);
        assert_eq!(
            block_destroy_alternates(1),
            &[SoundId::BLOCK_EXPLODE1, SoundId::BLOCK_EXPLODE1_ALT]
        );
        assert_eq!(
            block_destroy_alternates(2),
            &[SoundId::BLOCK_EXPLODE2, SoundId::BLOCK_EXPLODE2_ALT]
        );
        assert_eq!(block_destroy_alternates(3), &[SoundId::BLOCK_EXPLODE3]);
        // Arc `RandomSound.play`: the size-3 single is always chosen.
        let mut rng = SeededAudioRng::new(0);
        assert_eq!(block_destroy_sound(&mut rng, 3), SoundId::BLOCK_EXPLODE3);
    }

    #[test]
    fn random_sound_alternates_are_both_reachable() {
        // Arc `RandomSound`: size 1/2 pick uniformly from primary+alternate.
        let mut rng = SeededAudioRng::new(3);
        let mut primary = 0;
        let mut alternate = 0;
        for _ in 0..256 {
            match block_destroy_sound(&mut rng, 1) {
                SoundId::BLOCK_EXPLODE1 => primary += 1,
                SoundId::BLOCK_EXPLODE1_ALT => alternate += 1,
                other => panic!("unexpected destroy sound {other:?}"),
            }
        }
        assert!(primary > 0 && alternate > 0, "{primary}/{alternate}");
        assert_eq!(
            pick_random_sound(&mut rng, &[SoundId::BLOCK_EXPLODE3]),
            SoundId::BLOCK_EXPLODE3
        );
        assert_eq!(pick_random_sound(&mut rng, &[]), SoundId::NONE);
    }

    #[test]
    fn random_sound_reads_fx_stream_not_gameplay_rng() {
        // The alternate draw is deterministic and isolated from `RngStream::Sim`.
        let mut sim = SimRng::new(7);
        let gameplay = sim.stream_state(RngStream::Sim);
        let fx_before = sim.stream_state(RngStream::Fx);
        {
            let mut rng = FxAudioRng { rng: &mut sim };
            let _ = block_destroy_sound(&mut rng, 2);
        }
        assert_eq!(sim.stream_state(RngStream::Sim), gameplay);
        assert_ne!(sim.stream_state(RngStream::Fx), fx_before);
        assert_eq!(
            sim.stream_state(RngStream::Sim),
            SimRng::new(7).stream_state(RngStream::Sim)
        );
    }

    #[test]
    fn unit_despawn_sound_order_is_death_then_wreck() {
        // `UnitComp.kill`/`destroy`: death sound fires before the wreck sound,
        // and both before `remove` (`BulletComp.remove`'s despawned→removed
        // ordering for bullets is asserted in `combat::bullet`).
        let log = SharedAudioLog::new();
        let audio = AudioSinkRes::new(log.clone());
        emit_unit_death(&audio, SoundId::UNIT_EXPLODE1, 0.0, 0.0, 1.0);
        emit_unit_wreck(&audio, SoundId::WRECK_FALL, 0.0, 0.0, 1.0);
        let events = log.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event.sound(), Some(SoundId::UNIT_EXPLODE1));
        assert_eq!(events[1].event.sound(), Some(SoundId::WRECK_FALL));
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

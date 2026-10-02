// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Audio math + RNG (plan 18 §3.6, §6.7).
//!
//! Behavioral port of Arc `Sound.calcFalloff`/`calcPan`, `Mathf.lerpDelta`,
//! `Mathf.chance` and `Seq.random(exclude)`. All of this is render-only and must
//! never feed back into the deterministic sim (HLP §2.4; plan 18 boundary 3).

/// Distance falloff constant (`Core.audio.falloff`).
pub const FALLOFF: f32 = 16000.0;
/// `Audio.defaultSoundMaxConcurrent`.
pub const DEFAULT_SOUND_MAX_CONCURRENT: i32 = 6;
/// `Sound.minInterval` default, milliseconds.
pub const MIN_INTERVAL_MS: f64 = 16.0;
/// Music fade-in time, delta-frames (`SoundControl.finTime`).
pub const FIN_TIME: f32 = 120.0;
/// Music fade-out time, delta-frames (`SoundControl.foutTime`).
pub const FOUT_TIME: f32 = 120.0;
/// `SoundControl.musicInterval` (`3f * Time.toMinutes`, `toMinutes = 60*60`).
pub const MUSIC_INTERVAL: f64 = 3.0 * 3600.0;
/// `SoundControl.musicChance`.
pub const MUSIC_CHANCE: f32 = 0.8;
/// `SoundControl.musicWaveChance`.
pub const MUSIC_WAVE_CHANCE: f32 = 0.46;
/// Loop aggregation lerp alpha (`SoundControl.updateLoops`).
pub const LOOP_LERP: f32 = 0.11;
/// `SoundLoop.fadeSpeed`.
pub const SOUND_LOOP_FADE: f32 = 0.05;
/// `Sound` minimum volume before an `At` call is discarded.
pub const MIN_PLAY_VOLUME: f32 = 0.005;
/// `SoundControl` loop accumulation floor (`volume <= 0.00001f`).
pub const MIN_LOOP_VOLUME: f32 = 0.00001;
/// Loop voice start threshold (`data.curVolume > 0.01f`).
pub const LOOP_START_VOLUME: f32 = 0.01;
/// Loop voice stop threshold (`data.curVolume <= 0.001f`).
pub const LOOP_STOP_VOLUME: f32 = 0.001;
/// `SoundLoop.stop` threshold.
pub const SOUND_LOOP_STOP_VOLUME: f32 = 0.001;

/// Camera/listener frame used for falloff + pan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    /// Camera centre x (world px).
    pub x: f32,
    /// Camera centre y (world px).
    pub y: f32,
    /// Camera viewport width (world px).
    pub width: f32,
}

impl Default for Listener {
    fn default() -> Self {
        Listener {
            x: 0.0,
            y: 0.0,
            width: 1.0,
        }
    }
}

impl Listener {
    /// Creates a listener at a position with a viewport width.
    pub fn new(x: f32, y: f32, width: f32) -> Self {
        Listener { x, y, width }
    }
}

/// Arc `Sound.calcFalloff`: `clamp(1 / (max(dst2 - off^2, 0) / FALLOFF))`.
pub fn calc_falloff(x: f32, y: f32, listener: Listener, falloff_offset: f32) -> f32 {
    let dx = x - listener.x;
    let dy = y - listener.y;
    let offset2 = falloff_offset * falloff_offset;
    let dst2 = (dx * dx + dy * dy - offset2).max(0.0);
    (1.0 / (dst2 / FALLOFF)).clamp(0.0, 1.0)
}

/// Arc `Sound.calcPan`: camera-relative pan clamped to `[-0.9, 0.9]`.
pub fn calc_pan(x: f32, listener: Listener) -> f32 {
    if listener.width.abs() < f32::EPSILON {
        return 0.0;
    }
    ((x - listener.x) / (listener.width / 2.0)).clamp(-0.9, 0.9)
}

/// Arc `Mathf.lerpDelta(a, b, alpha)` at `delta = 1` (one frame):
/// `a + (b - a) * alpha`.
pub fn lerp_delta(from: f32, to: f32, alpha: f32) -> f32 {
    from + (to - from) * alpha
}

/// `Mathf.zero(value, epsilon)` — true when `|value| < epsilon`.
pub fn is_zero(value: f32, epsilon: f32) -> bool {
    value.abs() < epsilon
}

/// Clamps to `[0, 1]`.
pub fn clamp01(value: f32) -> f32 {
    if value.is_nan() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

/// Sanitizes a float exactly as Arc `Sound.play` does (NaN/Inf → fallback).
pub fn sanitize(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// Pseudo-random source for audio selection. Never the deterministic sim RNG
/// (plan 18 §2.4 deviation 9, §3.11 boundary 3).
pub trait AudioRng {
    /// Uniform `f32` in `[0, 1)`.
    fn next_f32(&mut self) -> f32;

    /// Arc `Mathf.chance(p)`: `random() < p`; `p <= 0` false, `p >= 1` true,
    /// NaN false.
    fn chance(&mut self, p: f32) -> bool {
        if p.is_nan() || p <= 0.0 {
            return false;
        }
        if p >= 1.0 {
            return true;
        }
        self.next_f32() < p
    }
}

/// Deterministic (wall-clock-seeded in `mind-gdext`) audio RNG.
#[derive(Debug, Clone)]
pub struct SeededAudioRng {
    state: u64,
}

impl SeededAudioRng {
    /// Creates a seeded RNG (SplitMix64 stream).
    pub fn new(seed: u64) -> Self {
        SeededAudioRng { state: seed }
    }
}

impl AudioRng for SeededAudioRng {
    fn next_f32(&mut self) -> f32 {
        // SplitMix64: deterministic, cheap, good enough for audio selection.
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // Top 24 bits → [0, 1).
        ((z >> 40) as f32) / (1u32 << 24) as f32
    }
}

/// Arc `Seq.random(exclude)`: uniform over elements except `exclude`; returns
/// the chosen index, or `None` for an empty list / only-excluded list.
pub fn random_index_excluding(
    rng: &mut impl AudioRng,
    len: usize,
    exclude: Option<usize>,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    if let Some(ex) = exclude {
        if len == 1 && ex == 0 {
            return None;
        }
        // Draw in the reduced space, then map around the excluded index.
        let pick = (rng.next_f32() * (len - 1) as f32) as usize;
        let pick = pick.min(len - 2);
        Some(if pick >= ex { pick + 1 } else { pick })
    } else {
        let pick = (rng.next_f32() * len as f32) as usize;
        Some(pick.min(len - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener() -> Listener {
        Listener::new(100.0, 100.0, 200.0)
    }

    #[test]
    fn falloff_and_pan_values() {
        let l = listener();
        assert_eq!(calc_falloff(100.0, 100.0, l, 0.0), 1.0);
        // dst2 == FALLOFF -> 1.
        let x = 100.0 + FALLOFF.sqrt();
        assert!((calc_falloff(x, 100.0, l, 0.0) - 1.0).abs() < 1.0e-5);
        // dst2 == 2*FALLOFF -> 0.5.
        let x = 100.0 + (2.0 * FALLOFF).sqrt();
        assert!((calc_falloff(x, 100.0, l, 0.0) - 0.5).abs() < 1.0e-5);
        // offset removes the near field.
        let x = 100.0 + 50.0;
        assert_eq!(calc_falloff(x, 100.0, l, 100.0), 1.0);

        assert_eq!(calc_pan(100.0, l), 0.0);
        assert_eq!(calc_pan(1000.0, l), 0.9);
        assert_eq!(calc_pan(-1000.0, l), -0.9);
    }

    #[test]
    fn sanitize_and_lerp() {
        assert_eq!(sanitize(f32::NAN, 0.0), 0.0);
        assert_eq!(sanitize(f32::INFINITY, 1.0), 1.0);
        assert_eq!(sanitize(0.5, 0.0), 0.5);
        assert!((lerp_delta(0.0, 1.0, 0.11) - 0.11).abs() < 1.0e-6);
    }

    #[test]
    fn chance_edges() {
        let mut rng = SeededAudioRng::new(1);
        assert!(!rng.chance(0.0));
        assert!(!rng.chance(-1.0));
        assert!(!rng.chance(f32::NAN));
        assert!(rng.chance(1.0));
        assert!(rng.chance(2.0));
    }

    #[test]
    fn rng_is_deterministic_and_bounded() {
        let mut a = SeededAudioRng::new(7);
        let mut b = SeededAudioRng::new(7);
        for _ in 0..1000 {
            let x = a.next_f32();
            assert_eq!(x, b.next_f32());
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn random_excluding_maps_around_exclusion() {
        let mut rng = SeededAudioRng::new(3);
        // Only element is excluded -> none.
        assert_eq!(random_index_excluding(&mut rng, 1, Some(0)), None);
        assert_eq!(random_index_excluding(&mut rng, 0, None), None);
        // Never returns the excluded index.
        for _ in 0..1000 {
            let pick = random_index_excluding(&mut rng, 4, Some(2)).unwrap();
            assert_ne!(pick, 2);
            assert!(pick < 4);
        }
        // No exclusion covers the full range.
        for _ in 0..1000 {
            let pick = random_index_excluding(&mut rng, 3, None).unwrap();
            assert!(pick < 3);
        }
    }
}

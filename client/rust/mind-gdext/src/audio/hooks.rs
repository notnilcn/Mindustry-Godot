// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `BurstDrill.ambientVolume`/`drillSound` and `LaunchPad.launchSound` formulas.

//! Sim-side `BurstDrill`/`LaunchPad` audio hook formulas.
//!
//! The triggers live in `mind-core` (`world/behavior/campaign.rs`
//! `LaunchPad.updateTile`, `audio/ambient.rs` `ambient_volume_scale`,
//! `world/blocks/production` `BurstDrill` bursts); this module owns the audio
//! half so the client API and the future sim call sites share one tested
//! definition:
//!
//! - `BurstDrill.ambientVolume()` = `super.ambientVolume() * pow(progress(), 4)`
//!   (`BurstDrill.java:123-124`);
//! - `drillSound.at(x, y, 1 + range(0.1), 0.6)` with `Sounds.drillImpact`
//!   (`BurstDrill.java:32-33,116`);
//! - `launchSound.at(x, y, 1 + range(0.1))` with `Sounds.padLaunch`
//!   (`LaunchPad.java:33-34,149`).

use mind_core::audio::math::AudioRng;

/// `BurstDrill.drillSoundVolume` (`BurstDrill.java:33`).
pub const BURST_DRILL_SOUND_VOLUME: f32 = 0.6;
/// `BurstDrill.drillSoundPitchRand` (`BurstDrill.java:33`).
pub const BURST_DRILL_PITCH_RAND: f32 = 0.1;
/// `LaunchPad.launchSoundPitchRand` (`LaunchPad.java:33`).
pub const LAUNCH_PAD_PITCH_RAND: f32 = 0.1;

/// `BurstDrill.ambientVolume()` progress factor: `Mathf.pow(progress, 4f)`
/// (`BurstDrill.java:123-124`). `progress` is `DrillBuild.progress()` =
/// `clamp(drillProgress / getDrillTime(item))` (`Drill.java:328-330`), `0` when
/// the drill has no dominant item.
pub fn burst_drill_ambient_factor(progress: f32) -> f32 {
    if !progress.is_finite() {
        return 0.0;
    }
    progress.clamp(0.0, 1.0).powi(4)
}

/// `1f + Mathf.range(rand)` — the pitch both block hooks pass to `Sound.at`
/// (`Mathf.range(x) = random(-x, x)`).
pub fn pitch_with_rand(rng: &mut impl AudioRng, rand: f32) -> f32 {
    let spread = if rand.is_finite() { rand.abs() } else { 0.0 };
    1.0 + (rng.next_f32() * 2.0 - 1.0) * spread
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::audio::SeededAudioRng;

    #[test]
    fn burstdrill_ambient_factor_is_progress_pow_4() {
        assert_eq!(burst_drill_ambient_factor(0.0), 0.0);
        assert!((burst_drill_ambient_factor(0.5) - 0.0625).abs() < 1.0e-6);
        assert!((burst_drill_ambient_factor(0.8) - 0.4096).abs() < 1.0e-6);
        assert_eq!(burst_drill_ambient_factor(1.0), 1.0);
        // `progress()` is clamped upstream; a hostile NaN must stay silent.
        assert_eq!(burst_drill_ambient_factor(1.5), 1.0);
        assert_eq!(burst_drill_ambient_factor(-1.0), 0.0);
        assert_eq!(burst_drill_ambient_factor(f32::NAN), 0.0);
        assert_eq!(burst_drill_ambient_factor(f32::INFINITY), 0.0);
    }

    #[test]
    fn pitch_rand_stays_in_upstream_bounds() {
        let mut rng = SeededAudioRng::new(11);
        for _ in 0..1000 {
            let pitch = pitch_with_rand(&mut rng, BURST_DRILL_PITCH_RAND);
            assert!(
                (1.0 - BURST_DRILL_PITCH_RAND..1.0 + BURST_DRILL_PITCH_RAND).contains(&pitch),
                "pitch {pitch}"
            );
        }
        // `Mathf.range(0)` is exactly 1.
        assert_eq!(pitch_with_rand(&mut rng, 0.0), 1.0);
        // Deterministic for a given seed.
        let mut a = SeededAudioRng::new(7);
        let mut b = SeededAudioRng::new(7);
        assert_eq!(
            pitch_with_rand(&mut a, LAUNCH_PAD_PITCH_RAND),
            pitch_with_rand(&mut b, LAUNCH_PAD_PITCH_RAND)
        );
    }

    #[test]
    fn hook_constants_match_upstream_fields() {
        assert_eq!(BURST_DRILL_SOUND_VOLUME, 0.6);
        assert_eq!(BURST_DRILL_PITCH_RAND, 0.1);
        assert_eq!(LAUNCH_PAD_PITCH_RAND, 0.1);
    }
}

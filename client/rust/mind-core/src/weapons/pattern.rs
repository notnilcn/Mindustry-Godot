// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shoot patterns (`core/src/mindustry/entities/pattern/*.java` behavior half).
//!
//! The data half (kind tag + fields) lives in plan 02's
//! [`crate::content::registries::units::weapon::ShootPatternSpec`]. This module
//! ports the `shoot(totalShots, handler, barrelIncrementer)` algorithms and the
//! [`BulletHandler`] seam the weapon engine implements.
//!
//! `ShootSummon` uses the deterministic random stream supplied by the caller;
//! `Mathf.random(360f)`/`Mathf.range` become `SimRng` draws (plan 10 §3.2), so a
//! summon pattern is only meaningful when driven by the weapon engine that owns
//! the RNG. The pure `emit_shot_buffer` entry point used by tests takes an
//! explicit RNG.

use smallvec::SmallVec;

use crate::content::registries::units::weapon::{ShootPatternKind, ShootPatternSpec};
use crate::determinism::{RngStream, SimRng};

use crate::combat::bullet::ShotMover;

/// Upper bound on shots produced by one pattern trigger (`ShootMulti` nests).
pub const MAX_PATTERN_SHOTS: usize = 64;

/// A single bullet emitted by a pattern, before world-space transformation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatternShot {
    /// Barrel index at emission time (`Weapon.shoot` handler `barrel`).
    pub barrel: i32,
    /// X offset in weapon space.
    pub x: f32,
    /// Y offset in weapon space.
    pub y: f32,
    /// Rotation offset in degrees.
    pub rotation: f32,
    /// Delay in ticks before firing.
    pub delay: f32,
    /// Optional per-tick mover (helix).
    pub mover: Option<ShotMover>,
}

/// Bullet sink used by [`emit`] (`ShootPattern.BulletHandler`).
pub trait BulletHandler {
    /// Emits one bullet (`handler.shoot(x, y, rotation, delay, mover)`).
    fn shoot(
        &mut self,
        barrel: i32,
        x: f32,
        y: f32,
        rotation: f32,
        delay: f32,
        mover: Option<ShotMover>,
    );
}

/// A [`BulletHandler`] that collects shots into a buffer (tests + engine).
#[derive(Debug, Default)]
pub struct ShotBuffer {
    /// Emitted shots in pattern order.
    pub shots: SmallVec<[PatternShot; MAX_PATTERN_SHOTS]>,
}

impl BulletHandler for ShotBuffer {
    fn shoot(
        &mut self,
        barrel: i32,
        x: f32,
        y: f32,
        rotation: f32,
        delay: f32,
        mover: Option<ShotMover>,
    ) {
        self.shots.push(PatternShot {
            barrel,
            x,
            y,
            rotation,
            delay,
            mover,
        });
    }
}

/// `Mathf.sign(boolean)`.
fn sign(b: bool) -> f32 {
    if b { -1.0 } else { 1.0 }
}

/// Ports `ShootPattern.shoot(totalShots, handler, barrelIncrementer)`.
///
/// `barrel_counter` is the mount's live barrel counter; patterns that run the
/// incrementer advance it in place (matching Java's `() -> mount.barrelCounter++`).
pub fn emit(
    spec: &ShootPatternSpec,
    total_shots: i32,
    handler: &mut dyn BulletHandler,
    barrel_counter: &mut i32,
    rng: &mut SimRng,
) {
    match spec.kind {
        ShootPatternKind::ShootPattern => {
            for i in 0..spec.shots {
                handler.shoot(
                    *barrel_counter,
                    0.0,
                    0.0,
                    0.0,
                    spec.first_shot_delay + spec.shot_delay * i as f32,
                    None,
                );
            }
        }
        ShootPatternKind::ShootAlternate => {
            let barrels = spec.barrels.max(1);
            for i in 0..spec.shots {
                let index = ((total_shots + i + spec.barrel_offset) % barrels) as f32
                    - (barrels - 1) as f32 / 2.0;
                handler.shoot(
                    *barrel_counter,
                    index * spec.spread * -sign(spec.mirror),
                    0.0,
                    0.0,
                    spec.first_shot_delay + spec.shot_delay * i as f32,
                    None,
                );
                *barrel_counter += 1;
            }
        }
        ShootPatternKind::ShootSpread => {
            for i in 0..spec.shots {
                let angle = i as f32 * spec.spread - (spec.shots - 1) as f32 * spec.spread / 2.0;
                handler.shoot(
                    *barrel_counter,
                    0.0,
                    0.0,
                    angle,
                    spec.first_shot_delay + spec.shot_delay * i as f32,
                    None,
                );
            }
        }
        ShootPatternKind::ShootHelix => {
            for i in 0..spec.shots {
                for s in [1.0f32, -1.0f32] {
                    handler.shoot(
                        *barrel_counter,
                        0.0,
                        0.0,
                        0.0,
                        spec.first_shot_delay + spec.shot_delay * i as f32,
                        Some(ShotMover::Helix {
                            scl: spec.scl,
                            mag: spec.mag,
                            offset: spec.offset,
                            sign: s,
                        }),
                    );
                }
            }
        }
        ShootPatternKind::ShootBarrel => {
            let count = spec.barrel_list.len().max(1);
            for i in 0..spec.shots {
                let index =
                    (i + total_shots + spec.barrel_offset).rem_euclid(count as i32) as usize;
                let barrel = spec
                    .barrel_list
                    .get(index)
                    .copied()
                    .unwrap_or([0.0, 0.0, 0.0]);
                handler.shoot(
                    *barrel_counter,
                    barrel[0],
                    barrel[1],
                    barrel[2],
                    spec.first_shot_delay + spec.shot_delay * i as f32,
                    None,
                );
                *barrel_counter += 1;
            }
        }
        ShootPatternKind::ShootSine => {
            for i in 0..spec.shots {
                let angle_offset = ((i + total_shots) as f32 * spec.sine_scl).sin() * spec.sine_mag;
                handler.shoot(
                    *barrel_counter,
                    0.0,
                    0.0,
                    angle_offset,
                    spec.first_shot_delay + spec.shot_delay * i as f32,
                    None,
                );
            }
        }
        ShootPatternKind::ShootSummon => {
            for i in 0..spec.shots {
                let angle = rng.range(RngStream::Sim, 0.0, 360.0);
                let len = rng.range(RngStream::Sim, 0.0, spec.summon_radius);
                let rad = angle.to_radians();
                let spread = rng.range(RngStream::Sim, -spec.summon_spread, spec.summon_spread);
                handler.shoot(
                    *barrel_counter,
                    spec.summon_x + rad.cos() * len,
                    spec.summon_y + rad.sin() * len,
                    spread,
                    spec.first_shot_delay + spec.shot_delay * i as f32,
                    None,
                );
            }
        }
        ShootPatternKind::ShootMulti => {
            let Some(source) = spec.multi_source.as_deref() else {
                return;
            };
            // `ShootMulti` runs the source pattern, then fans every source shot
            // out through every dest pattern (offsets/rotations/delays add). The
            // barrel incrementer is only passed to the source (Java passes `null`
            // to the dest patterns).
            let mut src_buffer = ShotBuffer::default();
            let mut src_barrel = *barrel_counter;
            emit(source, total_shots, &mut src_buffer, &mut src_barrel, rng);
            for src_shot in &src_buffer.shots {
                for pattern in &spec.multi_dest {
                    let mut dest_buffer = ShotBuffer::default();
                    let mut dest_barrel = *barrel_counter;
                    emit(
                        pattern,
                        total_shots,
                        &mut dest_buffer,
                        &mut dest_barrel,
                        rng,
                    );
                    for dest in &dest_buffer.shots {
                        handler.shoot(
                            dest.barrel,
                            src_shot.x + dest.x,
                            src_shot.y + dest.y,
                            src_shot.rotation + dest.rotation,
                            src_shot.delay + dest.delay,
                            dest.mover,
                        );
                    }
                }
            }
            *barrel_counter = src_barrel;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(spec: &ShootPatternSpec, total: i32) -> Vec<PatternShot> {
        let mut buffer = ShotBuffer::default();
        let mut barrel = 0;
        let mut rng = SimRng::new(7);
        emit(spec, total, &mut buffer, &mut barrel, &mut rng);
        buffer.shots.into_vec()
    }

    #[test]
    fn single_delays() {
        let spec = ShootPatternSpec::plain(3, 4.0, 2.0);
        let shots = run(&spec, 0);
        assert_eq!(shots.len(), 3);
        assert_eq!(shots[0].delay, 2.0);
        assert_eq!(shots[1].delay, 6.0);
        assert_eq!(shots[2].delay, 10.0);
    }

    #[test]
    fn alternate_barrel_order() {
        // 4 barrels, spread 5 world units; totalShots starts at 0.
        let mut spec = ShootPatternSpec::alternate(4, 0.0, 5.0, 4);
        spec.mirror = false;
        let shots = run(&spec, 0);
        // index = ((0 + i + 0) % 4) - 1.5 => -1.5, -0.5, 0.5, 1.5; x = index*5.
        let xs: Vec<f32> = shots.iter().map(|s| s.x).collect();
        assert_eq!(xs, vec![7.5, 2.5, -2.5, -7.5]);
        // The barrel incrementer ran once per shot.
        let mut buffer = ShotBuffer::default();
        let mut barrel = 3;
        let mut rng = SimRng::new(1);
        emit(&spec, 0, &mut buffer, &mut barrel, &mut rng);
        assert_eq!(barrel, 7);
    }

    #[test]
    fn alternate_mirror_flips_sign() {
        let mut a = ShootPatternSpec::alternate(1, 0.0, 5.0, 2);
        a.mirror = false;
        let mut b = a.clone();
        b.flip();
        assert_ne!(run(&a, 0)[0].x, run(&b, 0)[0].x);
        assert_eq!(run(&a, 0)[0].x, -run(&b, 0)[0].x);
    }

    #[test]
    fn spread_centering() {
        let spec = ShootPatternSpec::spread(5, 10.0);
        let shots = run(&spec, 0);
        let angles: Vec<f32> = shots.iter().map(|s| s.rotation).collect();
        assert_eq!(angles, vec![-20.0, -10.0, 0.0, 10.0, 20.0]);
    }

    #[test]
    fn helix_signs() {
        let spec = ShootPatternSpec::helix(2.0, 1.5);
        let shots = run(&spec, 0);
        assert_eq!(shots.len(), 2);
        let a = shots[0].mover.unwrap();
        let b = shots[1].mover.unwrap();
        match (a, b) {
            (ShotMover::Helix { sign: sa, .. }, ShotMover::Helix { sign: sb, .. }) => {
                assert_eq!((sa, sb), (1.0, -1.0))
            }
        }
    }

    #[test]
    fn barrel_cycle() {
        let spec =
            ShootPatternSpec::barrel(vec![[1.0, 0.0, 0.0], [2.0, 0.0, 9.0], [3.0, 0.0, 18.0]], 0);
        let shots = run(&spec, 5);
        // i+totalShots index: 5%3=2 -> barrel 3.
        assert_eq!(shots[0].x, 3.0);
        assert_eq!(shots[0].rotation, 18.0);
        let mut flipped = spec.clone();
        flipped.flip();
        let f = run(&flipped, 5);
        assert_eq!(f[0].x, -3.0);
        assert_eq!(f[0].rotation, -18.0);
    }

    #[test]
    fn sine_offsets() {
        let spec = ShootPatternSpec::sine(1.0, 10.0);
        let shots = run(&spec, 0);
        let expected: Vec<f32> = (0..4).map(|i| ((i as f32) * 1.0).sin() * 10.0).collect();
        let got: Vec<f32> = shots.iter().map(|s| s.rotation).collect();
        for (a, b) in expected.iter().zip(got.iter()) {
            assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        }
    }

    #[test]
    fn summon_radius_is_seeded() {
        let spec = ShootPatternSpec::summon(0.0, 0.0, 12.0, 5.0);
        let a = run(&spec, 0);
        let b = run(&spec, 0);
        assert_eq!(a, b, "same seed => same summon positions");
        for shot in &a {
            let len = (shot.x * shot.x + shot.y * shot.y).sqrt();
            assert!(len <= 12.0 + 1e-4);
        }
        assert!(a.iter().any(|s| s.x != 0.0 || s.y != 0.0));
    }

    #[test]
    fn multi_composition() {
        let spec = ShootPatternSpec::multi(
            ShootPatternSpec::spread(2, 0.0),
            vec![ShootPatternSpec::plain(2, 0.0, 0.0)],
        );
        let shots = run(&spec, 0);
        // source emits 2, each fanned to dest's 2 => 4 shots.
        assert_eq!(shots.len(), 4);
    }
}

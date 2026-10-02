// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SoundPriority` table + pure voice admission/eviction policy (plan 18 §3.6,
//! §6.3). Ported from `core/src/mindustry/audio/SoundPriority.java` and the
//! SoLoud concurrency behavior it configures.

use super::ids::{BusKind, SoundId};
use super::math::{DEFAULT_SOUND_MAX_CONCURRENT, MIN_INTERVAL_MS};
use crate::content::registries::sound_meta::SOUNDS;

/// Per-sound playback policy resolved from the `SoundPriority.init` table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundPrioritySpec {
    /// Interruption priority; higher wins.
    pub priority: f32,
    /// Max concurrent voices (`0` = unlimited; default 6).
    pub max_concurrent: i32,
    /// Shared concurrency group (`0` = unique/ungrouped).
    pub group: i32,
    /// Absolute minimum interruptible playtime (seconds), if set.
    pub min_interrupt_absolute: Option<f32>,
    /// `(min, fraction)` interruptible-playtime rule, default `(0.25, 0.5)`.
    pub min_interrupt_fraction: Option<(f32, f32)>,
    /// Falloff near-field offset (px).
    pub falloff_offset: f32,
    /// Forced bus override.
    pub bus: Option<BusKind>,
}

impl Default for SoundPrioritySpec {
    fn default() -> Self {
        SoundPrioritySpec {
            priority: 0.0,
            max_concurrent: DEFAULT_SOUND_MAX_CONCURRENT,
            group: 0,
            min_interrupt_absolute: None,
            min_interrupt_fraction: Some((0.25, 0.5)),
            falloff_offset: 0.0,
            bus: None,
        }
    }
}

impl SoundPrioritySpec {
    /// Resolves `minInterrupt` from the stream length (`AudioSource.setMinConcurrentInterrupt*`).
    pub fn min_interrupt(&self, length_s: f32) -> f32 {
        if let Some(absolute) = self.min_interrupt_absolute {
            return absolute;
        }
        match self.min_interrupt_fraction {
            Some((min, fraction)) => min.min(length_s * fraction),
            None => 0.0,
        }
    }
}

/// Exact port of the `SoundPriority.init()` assignment table (plan 18 §6.3).
#[derive(Debug, Clone)]
pub struct SoundPriorityTable {
    specs: Vec<SoundPrioritySpec>,
}

impl Default for SoundPriorityTable {
    fn default() -> Self {
        SoundPriorityTable::build()
    }
}

impl SoundPriorityTable {
    /// Builds the table indexed by [`SoundId`] seed index.
    pub fn build() -> Self {
        let mut specs = vec![SoundPrioritySpec::default(); SOUNDS.len()];

        // `coreLaunch.setBus(uiBus)`.
        set_bus(&mut specs, "coreLaunch", BusKind::Ui);
        max(
            &mut specs,
            7,
            &["beamPlasma", "shootMeltdown", "beamMeltdown"],
        );

        set(
            &mut specs,
            3.0,
            &[
                "acceleratorLaunch",
                "acceleratorCharge",
                "coreLand",
                "coreLaunch",
            ],
        );
        set(
            &mut specs,
            2.0,
            &[
                "beamMeltdown",
                "beamLustre",
                "beamPlasma",
                "explosionReactor",
                "explosionReactor2",
                "explosionReactorNeoplasm",
                "explosionCore",
                "blockExplodeElectricBig",
                "blockExplodeExplosive",
                "blockExplodeExplosiveAlt",
            ],
        );
        set(
            &mut specs,
            1.5,
            &[
                "shootMeltdown",
                "shootSublimate",
                "shootForeshadow",
                "shootConquer",
                "shootCorvus",
                "chargeCorvus",
                "chargeVela",
                "chargeLancer",
                "shootReign",
                "shootEclipse",
                "shootArtillerySapBig",
                "shootToxopidShotgun",
                "beamPlasmaSmall",
                "shootNavanax",
                "explosionNavanax",
            ],
        );
        set(
            &mut specs,
            1.0,
            &[
                "loopConveyor",
                "loopSmelter",
                "loopDrill",
                "loopExtract",
                "loopFlux",
                "loopHum",
                "loopBio",
                "loopTech",
                "loopUnitBuilding",
            ],
        );

        max(&mut specs, 5, &["shootLancer"]);

        same_group(&mut specs, 1, &["shootFlame", "shootFlamePlasma"]);
        same_group(
            &mut specs,
            2,
            &[
                "shootMissile",
                "shootMissileShort",
                "shootMissilePlasmaShort",
            ],
        );
        same_group(&mut specs, 3, &["shootArc", "shootPulsar"]);

        // `setMinConcurrentInterruptFraction(0.25, 0.5)` on every sound is the
        // default already; these absolutes override it.
        min_interrupt(&mut specs, 0.5, &["mechStepSmall", "mechStep"]);
        min_interrupt(&mut specs, 0.6, &["walkerStep", "mechStepHeavy"]);

        max(&mut specs, 4, &["shieldHit"]);
        max(
            &mut specs,
            5,
            &[
                "mechStep",
                "mechStepHeavy",
                "walkerStep",
                "walkerStepSmall",
                "walkerStepTiny",
            ],
        );

        set(&mut specs, -1.0, &["blockHeal", "healWave"]);
        set(
            &mut specs,
            -2.0,
            &[
                "mechStep",
                "mechStepHeavy",
                "walkerStep",
                "walkerStepSmall",
                "walkerStepTiny",
                "mechStepSmall",
            ],
        );

        falloff_offset(&mut specs, "explosionCore", 100.0);
        falloff_offset(&mut specs, "blockExplodeElectricBig", 70.0);

        SoundPriorityTable { specs }
    }

    /// Spec for a sound.
    pub fn spec(&self, sound: SoundId) -> SoundPrioritySpec {
        self.specs
            .get(sound.raw() as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Number of entries (one per seed sound).
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }
}

fn index(name: &str) -> Option<usize> {
    SOUNDS.iter().position(|meta| meta.name == name)
}

fn set(specs: &mut [SoundPrioritySpec], value: f32, names: &[&str]) {
    for name in names {
        if let Some(i) = index(name) {
            specs[i].priority = value;
        }
    }
}

fn max(specs: &mut [SoundPrioritySpec], value: i32, names: &[&str]) {
    for name in names {
        if let Some(i) = index(name) {
            specs[i].max_concurrent = value;
        }
    }
}

fn same_group(specs: &mut [SoundPrioritySpec], group: i32, names: &[&str]) {
    for name in names {
        if let Some(i) = index(name) {
            specs[i].group = group;
        }
    }
}

fn min_interrupt(specs: &mut [SoundPrioritySpec], value: f32, names: &[&str]) {
    for name in names {
        if let Some(i) = index(name) {
            specs[i].min_interrupt_absolute = Some(value);
        }
    }
}

fn falloff_offset(specs: &mut [SoundPrioritySpec], name: &str, value: f32) {
    if let Some(i) = index(name) {
        specs[i].falloff_offset = value;
    }
}

fn set_bus(specs: &mut [SoundPrioritySpec], name: &str, bus: BusKind) {
    if let Some(i) = index(name) {
        specs[i].bus = Some(bus);
    }
}

/// An active voice tracked by the policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveVoice {
    /// Stable voice id.
    pub voice: u64,
    /// Sound id.
    pub sound: SoundId,
    /// Concurrency group (`0` = none).
    pub group: i32,
    /// Spec priority.
    pub priority: f32,
    /// Start time (ms).
    pub started_ms: f64,
    /// `min_interrupt` resolved from the stream length.
    pub min_interrupt: f32,
    /// Current volume.
    pub volume: f32,
}

/// A requested playback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayRequest {
    /// Sound id.
    pub sound: SoundId,
    /// Spec priority.
    pub priority: f32,
    /// Concurrency group (`0` = none).
    pub group: i32,
    /// Max concurrent (`0` = unlimited).
    pub max_concurrent: i32,
    /// `min_interrupt` resolved from the stream length.
    pub min_interrupt: f32,
}

/// Result of an admission attempt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Admission {
    /// Start a new voice.
    Play,
    /// Deny (concurrency/interrupt rules).
    Deny,
    /// Take over an existing voice (the caller reuses the slot).
    Replace {
        /// Voice to replace.
        voice: u64,
    },
}

impl Admission {
    /// Whether the request results in audible playback.
    pub fn is_play(self) -> bool {
        !matches!(self, Admission::Deny)
    }
}

/// Pure admission/eviction policy (plan 18 §3.6).
pub fn admit(active: &[ActiveVoice], request: &PlayRequest, now_ms: f64) -> Admission {
    let sound_count = active
        .iter()
        .filter(|voice| voice.sound == request.sound)
        .count();
    let group_count = if request.group == 0 {
        sound_count
    } else {
        active
            .iter()
            .filter(|voice| voice.group == request.group)
            .count()
    };

    let under_limit =
        |count: usize| request.max_concurrent <= 0 || count < request.max_concurrent as usize;

    if under_limit(sound_count) && under_limit(group_count) {
        return Admission::Play;
    }

    // If any active instance is still inside its non-interruptible window, deny.
    let any_protected = active.iter().any(|voice| {
        (voice.sound == request.sound || (request.group != 0 && voice.group == request.group))
            && ((now_ms - voice.started_ms) / 1000.0) < f64::from(voice.min_interrupt)
    });
    if any_protected {
        return Admission::Deny;
    }

    // Replacement only considers voices of the same sound/group.
    let mut candidates: Vec<&ActiveVoice> = active
        .iter()
        .filter(|voice| {
            voice.sound == request.sound || (request.group != 0 && voice.group == request.group)
        })
        .collect();
    if candidates.is_empty() {
        return Admission::Deny;
    }
    candidates.sort_by(|a, b| {
        a.priority
            .partial_cmp(&b.priority)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.started_ms
                    .partial_cmp(&b.started_ms)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let lowest = candidates[0];
    if request.priority >= lowest.priority {
        Admission::Replace {
            voice: lowest.voice,
        }
    } else {
        Admission::Deny
    }
}

/// Per-sound `Sound.lastTimePlayed`/`lastVoice`/`lastVolume` state for the
/// `checkFrame` 16 ms intensification (plan 18 §3.6).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SoundPlayState {
    /// Last play time (ms).
    pub last_time_played: f64,
    /// Last voice id.
    pub last_voice: u64,
    /// Last played volume.
    pub last_volume: f32,
}

impl SoundPlayState {
    /// Whether a second play of this sound is inside the min-interval window.
    pub fn within_min_interval(&self, now_ms: f64, min_interval_ms: f64) -> bool {
        now_ms - self.last_time_played <= min_interval_ms
    }

    /// Arc `Sound.play` intensification:
    /// `lastVolume = max(lastVolume, min(lastVolume + volume, volume * 1.25))`.
    pub fn intensify(&mut self, volume: f32) -> f32 {
        self.last_volume = self
            .last_volume
            .max((self.last_volume + volume).min(volume * 1.25));
        self.last_volume
    }

    /// Records a new voice after a successful play.
    pub fn record(&mut self, voice: u64, volume: f32, now_ms: f64) {
        self.last_voice = voice;
        self.last_volume = volume;
        self.last_time_played = now_ms;
    }

    /// Default minimum interval.
    pub fn default_min_interval() -> f64 {
        MIN_INTERVAL_MS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_sound_priority_init() {
        let table = SoundPriorityTable::build();
        assert_eq!(table.len(), SOUNDS.len());

        assert_eq!(table.spec(SoundId::ACCELERATOR_LAUNCH).priority, 3.0);
        assert_eq!(table.spec(SoundId::CORE_LAND).priority, 3.0);
        assert_eq!(table.spec(SoundId::BEAM_MELTDOWN).priority, 2.0);
        assert_eq!(table.spec(SoundId::SHOOT_CORVUS).priority, 1.5);
        assert_eq!(table.spec(SoundId::LOOP_CONVEYOR).priority, 1.0);
        assert_eq!(table.spec(SoundId::BLOCK_HEAL).priority, -1.0);
        assert_eq!(table.spec(SoundId::MECH_STEP).priority, -2.0);

        assert_eq!(table.spec(SoundId::BEAM_PLASMA).max_concurrent, 7);
        assert_eq!(table.spec(SoundId::SHOOT_LANCER).max_concurrent, 5);
        assert_eq!(table.spec(SoundId::SHIELD_HIT).max_concurrent, 4);
        assert_eq!(
            table.spec(SoundId::SHOOT_DUO).max_concurrent,
            DEFAULT_SOUND_MAX_CONCURRENT
        );

        assert_eq!(table.spec(SoundId::SHOOT_FLAME).group, 1);
        assert_eq!(table.spec(SoundId::SHOOT_FLAME_PLASMA).group, 1);
        assert_eq!(table.spec(SoundId::SHOOT_MISSILE).group, 2);
        assert_eq!(table.spec(SoundId::SHOOT_ARC).group, 3);
        assert_eq!(table.spec(SoundId::SHOOT_PULSAR).group, 3);
        assert_eq!(table.spec(SoundId::SHOOT_DUO).group, 0);

        assert_eq!(table.spec(SoundId::CORE_LAUNCH).bus, Some(BusKind::Ui));
        assert_eq!(table.spec(SoundId::EXPLOSION_CORE).falloff_offset, 100.0);
        assert_eq!(
            table
                .spec(SoundId::BLOCK_EXPLODE_ELECTRIC_BIG)
                .falloff_offset,
            70.0
        );

        // Interrupt rules: absolute overrides win.
        assert_eq!(table.spec(SoundId::MECH_STEP).min_interrupt(2.0), 0.5);
        assert_eq!(table.spec(SoundId::MECH_STEP_SMALL).min_interrupt(2.0), 0.5);
        assert_eq!(table.spec(SoundId::WALKER_STEP).min_interrupt(2.0), 0.6);
        // Fraction default: min(0.25, length * 0.5).
        assert_eq!(table.spec(SoundId::SHOOT_DUO).min_interrupt(0.2), 0.1);
        assert_eq!(table.spec(SoundId::SHOOT_DUO).min_interrupt(4.0), 0.25);
    }

    fn voice(voice: u64, sound: SoundId, priority: f32, started_ms: f64) -> ActiveVoice {
        ActiveVoice {
            voice,
            sound,
            group: 0,
            priority,
            started_ms,
            min_interrupt: 0.25,
            volume: 1.0,
        }
    }

    fn request(sound: SoundId, priority: f32, max_concurrent: i32) -> PlayRequest {
        PlayRequest {
            sound,
            priority,
            group: 0,
            max_concurrent,
            min_interrupt: 0.25,
        }
    }

    #[test]
    fn admission_policy() {
        let req = request(SoundId::SHOOT_DUO, 0.0, 6);
        assert_eq!(admit(&[], &req, 0.0), Admission::Play);

        // Under the limit -> play.
        let active = vec![voice(1, SoundId::SHOOT_DUO, 0.0, 0.0)];
        assert_eq!(admit(&active, &req, 10_000.0), Admission::Play);

        // Unlimited (`0`) always plays, even at many active voices.
        let unlimited = request(SoundId::SHOOT_DUO, 0.0, 0);
        let six = vec![
            voice(1, SoundId::SHOOT_DUO, 0.0, 0.0),
            voice(2, SoundId::SHOOT_DUO, 0.0, 0.0),
            voice(3, SoundId::SHOOT_DUO, 0.0, 0.0),
            voice(4, SoundId::SHOOT_DUO, 0.0, 0.0),
            voice(5, SoundId::SHOOT_DUO, 0.0, 0.0),
            voice(6, SoundId::SHOOT_DUO, 0.0, 0.0),
        ];
        assert_eq!(admit(&six, &unlimited, 10_000.0), Admission::Play);

        // At the limit, in protected window -> deny.
        let fresh = vec![
            voice(1, SoundId::SHOOT_DUO, 0.0, 0.0),
            voice(2, SoundId::SHOOT_DUO, 0.0, 0.0),
            voice(3, SoundId::SHOOT_DUO, 0.0, 0.0),
            voice(4, SoundId::SHOOT_DUO, 0.0, 0.0),
            voice(5, SoundId::SHOOT_DUO, 0.0, 0.0),
            voice(6, SoundId::SHOOT_DUO, 0.0, 0.0),
        ];
        assert_eq!(admit(&fresh, &req, 100.0), Admission::Deny);

        // At the limit, all past the window, equal priority -> replace oldest.
        let old = (1..=6)
            .map(|v| voice(v, SoundId::SHOOT_DUO, 0.0, 0.0))
            .collect::<Vec<_>>();
        assert_eq!(admit(&old, &req, 10_000.0), Admission::Replace { voice: 1 });
    }

    #[test]
    fn group_limits() {
        let a = ActiveVoice {
            group: 1,
            ..voice(1, SoundId::SHOOT_FLAME, 0.0, 0.0)
        };
        let b = ActiveVoice {
            group: 1,
            ..voice(2, SoundId::SHOOT_FLAME_PLASMA, 0.0, 0.0)
        };
        let req = PlayRequest {
            sound: SoundId::SHOOT_FLAME,
            priority: 0.0,
            group: 1,
            max_concurrent: 2,
            min_interrupt: 0.25,
        };
        // Group count 1 < 2 -> play.
        assert_eq!(admit(&[a], &req, 10_000.0), Admission::Play);
        // Group count 2 == limit, both fresh -> deny.
        assert_eq!(admit(&[a, b], &req, 100.0), Admission::Deny);
    }

    #[test]
    fn interrupt_window_and_priority_replacement() {
        // Same sound, one fresh protected voice -> deny a higher-priority request.
        let fresh_high = ActiveVoice {
            priority: 0.0,
            min_interrupt: 1.0,
            ..voice(1, SoundId::SHOOT_DUO, 0.0, 9_900.0)
        };
        let req = request(SoundId::SHOOT_DUO, 5.0, 1);
        assert_eq!(admit(&[fresh_high], &req, 10_000.0), Admission::Deny);

        // Past the window: higher priority replaces the lowest.
        let old_low = ActiveVoice {
            priority: 0.0,
            min_interrupt: 0.25,
            ..voice(7, SoundId::SHOOT_DUO, 0.0, 0.0)
        };
        assert_eq!(
            admit(&[old_low], &req, 10_000.0),
            Admission::Replace { voice: 7 }
        );

        // Lower priority than the active -> deny.
        let low_req = request(SoundId::SHOOT_DUO, -1.0, 1);
        assert_eq!(admit(&[old_low], &low_req, 10_000.0), Admission::Deny);
    }

    #[test]
    fn check_frame_boost() {
        let mut state = SoundPlayState::default();
        state.record(42, 0.5, 1000.0);
        assert!(state.within_min_interval(1000.0 + 16.0, SoundPlayState::default_min_interval()));
        assert!(!state.within_min_interval(1000.0 + 17.0, SoundPlayState::default_min_interval()));

        // last=0.5, new=0.5 -> min(1.0, 0.625) = 0.625
        assert!((state.intensify(0.5) - 0.625).abs() < 1.0e-6);
        // last=0.625, new=0.5 -> min(1.125, 0.625) = 0.625
        assert!((state.intensify(0.5) - 0.625).abs() < 1.0e-6);
    }
}

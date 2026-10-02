// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/type/Weapon.java (metadata half),
//         core/src/mindustry/type/weapons/{BuildWeapon,MineWeapon,PointDefenseWeapon,
//         PointDefenseBulletWeapon,RepairBeamWeapon}.java (class defaults only),
//         core/src/mindustry/entities/pattern/{ShootPattern,ShootAlternate,
//         ShootSpread,ShootHelix}.java (pattern fields).

//! Weapon mount metadata (plan 02 M5).
//!
//! Behavior (`update`, `shoot`, mount state, draw) is owned by plans 10/11; this
//! module carries the data half of `Weapon` plus the shoot-pattern specs that the
//! vanilla units use (`ShootBarrel` is unused by vanilla units and intentionally
//! not modelled — the kind enum is append-only).

use super::super::bullets::BulletSpec;
use super::EffectRef;
use super::parts::DrawPartSpec;
use crate::content::ContentError;
use crate::content::color::Rgba;
use crate::content::id::StatusId;
use crate::content::load::ContentRegistry;
use crate::content::registries::fx_meta::EffectId;
use crate::content::registries::pal;
use crate::content::registries::sound_meta::SoundId;

/// Java weapon class tag (`WeaponKind` content ABI; append-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WeaponKind {
    /// `mindustry.type.Weapon`.
    #[default]
    Weapon = 0,
    /// `mindustry.type.weapons.BuildWeapon`.
    BuildWeapon = 1,
    /// `mindustry.type.weapons.MineWeapon`.
    MineWeapon = 2,
    /// `mindustry.type.weapons.PointDefenseWeapon`.
    PointDefenseWeapon = 3,
    /// `mindustry.type.weapons.PointDefenseBulletWeapon`.
    PointDefenseBulletWeapon = 4,
    /// `mindustry.type.weapons.RepairBeamWeapon`.
    RepairBeamWeapon = 5,
}

impl WeaponKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            WeaponKind::Weapon => "Weapon",
            WeaponKind::BuildWeapon => "BuildWeapon",
            WeaponKind::MineWeapon => "MineWeapon",
            WeaponKind::PointDefenseWeapon => "PointDefenseWeapon",
            WeaponKind::PointDefenseBulletWeapon => "PointDefenseBulletWeapon",
            WeaponKind::RepairBeamWeapon => "RepairBeamWeapon",
        }
    }
}

/// Shoot-pattern class tag (`entities/pattern/*`; append-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ShootPatternKind {
    /// `ShootPattern` (single stream of shots).
    #[default]
    ShootPattern = 0,
    /// `ShootAlternate` (per-barrel alternation).
    ShootAlternate = 1,
    /// `ShootSpread` (fan of `shots` bullets).
    ShootSpread = 2,
    /// `ShootHelix` (sine-wave shots).
    ShootHelix = 3,
}

impl ShootPatternKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            ShootPatternKind::ShootPattern => "ShootPattern",
            ShootPatternKind::ShootAlternate => "ShootAlternate",
            ShootPatternKind::ShootSpread => "ShootSpread",
            ShootPatternKind::ShootHelix => "ShootHelix",
        }
    }
}

/// `ShootPattern` metadata (`shots`/`firstShotDelay`/`shotDelay` plus subclass
/// fields). Fields not relevant to a kind keep the base defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct ShootPatternSpec {
    /// Pattern class tag.
    pub kind: ShootPatternKind,
    /// `ShootPattern.shots`.
    pub shots: i32,
    /// `ShootPattern.firstShotDelay` (charge-up delay of the first shot).
    pub first_shot_delay: f32,
    /// `ShootPattern.shotDelay` (delay between subsequent shots).
    pub shot_delay: f32,
    /// `ShootAlternate.barrels`.
    pub barrels: i32,
    /// `ShootAlternate.spread` / `ShootSpread.spread`.
    pub spread: f32,
    /// `ShootAlternate.barrelOffset`.
    pub barrel_offset: i32,
    /// `ShootAlternate.mirror` (shoot-order flip; toggled by `flip()`).
    pub mirror: bool,
    /// `ShootHelix.scl`.
    pub scl: f32,
    /// `ShootHelix.mag`.
    pub mag: f32,
    /// `ShootHelix.offset` (defaults to `Mathf.PI * 1.25f`).
    pub offset: f32,
}

impl Default for ShootPatternSpec {
    fn default() -> Self {
        Self {
            kind: ShootPatternKind::ShootPattern,
            shots: 1,
            first_shot_delay: 0.0,
            shot_delay: 0.0,
            barrels: 2,
            spread: 5.0,
            barrel_offset: 0,
            mirror: false,
            scl: 2.0,
            mag: 1.5,
            offset: std::f32::consts::PI * 1.25,
        }
    }
}

impl ShootPatternSpec {
    /// `new ShootPattern()` with overridden fields.
    pub fn plain(shots: i32, shot_delay: f32, first_shot_delay: f32) -> Self {
        Self {
            shots,
            shot_delay,
            first_shot_delay,
            ..Self::default()
        }
    }

    /// `new ShootAlternate()` / `new ShootAlternate(spread)` with overrides.
    pub fn alternate(shots: i32, shot_delay: f32, spread: f32, barrels: i32) -> Self {
        Self {
            kind: ShootPatternKind::ShootAlternate,
            shots,
            shot_delay,
            spread,
            barrels,
            ..Self::default()
        }
    }

    /// `new ShootSpread(shots, spread)`.
    pub fn spread(shots: i32, spread: f32) -> Self {
        Self {
            kind: ShootPatternKind::ShootSpread,
            shots,
            spread,
            ..Self::default()
        }
    }

    /// `new ShootHelix()` with overrides (`scl`/`mag`/`offset` fields).
    pub fn helix(scl: f32, mag: f32) -> Self {
        Self {
            kind: ShootPatternKind::ShootHelix,
            scl,
            mag,
            ..Self::default()
        }
    }

    /// `ShootPattern.flip()` (mirrored weapons): `ShootAlternate` toggles
    /// `mirror`; other vanilla-unit patterns flip nothing (`ShootPattern.java:29`).
    pub fn flip(&mut self) {
        if self.kind == ShootPatternKind::ShootAlternate {
            self.mirror = !self.mirror;
        }
    }
}

/// Which bullet a weapon fires (`Weapon.bullet`).
#[derive(Debug, Clone, Default)]
pub enum BulletRef {
    /// `Bullets.placeholder` (id 0) — the `Weapon.bullet` default.
    #[default]
    Placeholder,
    /// A unit-local bullet variable constructed before the weapons
    /// (`BulletType x = new ...;` in the unit initializer), by index.
    Pre(usize),
    /// An inline bullet constructed at this point in the unit initializer.
    Inline(Box<BulletSpec>),
}

/// Generated weapon input record (`new Weapon(...){{...}}`).
///
/// Fields default to the Java `Weapon` defaults plus the class-tag initializer
/// overrides (see [`WeaponDef::from_spec`]); `None` means "class default".
#[derive(Debug, Clone, Default)]
pub struct WeaponSpec {
    /// Region/weapon name (`Weapon(name)`; empty for the no-arg constructor).
    pub name: &'static str,
    /// Java class tag.
    pub kind: WeaponKind,
    /// Bullet fired.
    pub bullet: BulletRef,
    /// Whether the weapon shows in unit stats.
    pub display: Option<bool>,
    /// Whether a mirrored copy is created on init.
    pub mirror: Option<bool>,
    /// Whether mirrored mounts alternate shots.
    pub alternate: Option<bool>,
    /// Whether the weapon rotates toward the target.
    pub rotate: Option<bool>,
    /// Whether the weapon sprite shows in database stats.
    pub show_stat_sprite: Option<bool>,
    /// Starting rotation.
    pub base_rotation: Option<f32>,
    /// Whether the outline renders on top of the unit.
    pub top: Option<bool>,
    /// Hold the bullet in place while firing.
    pub continuous: Option<bool>,
    /// Continuous fire without reloading.
    pub always_continuous: Option<bool>,
    /// Aim lerp speed for point lasers (`Float.POSITIVE_INFINITY` default).
    pub aim_change_speed: Option<f32>,
    /// Player-controllable.
    pub controllable: Option<bool>,
    /// AI-controllable.
    pub ai_controllable: Option<bool>,
    /// Always shooting regardless of targets/cone.
    pub always_shooting: Option<bool>,
    /// Auto-targeting (requires `controllable = false`).
    pub auto_target: Option<bool>,
    /// Target-trajectory prediction.
    pub predict_target: Option<bool>,
    /// Whether this weapon counts for unit attack-range calculations.
    pub use_attack_range: Option<bool>,
    /// Ticks between target scans.
    pub target_interval: Option<f32>,
    /// Ticks between target switches.
    pub target_switch_interval: Option<f32>,
    /// Rotation speed in degrees/tick.
    pub rotate_speed: Option<f32>,
    /// Reload in ticks.
    pub reload: Option<f32>,
    /// Inaccuracy in degrees.
    pub inaccuracy: Option<f32>,
    /// Screen shake per shot.
    pub shake: Option<f32>,
    /// Visual recoil distance.
    pub recoil: Option<f32>,
    /// Extra recoil counters.
    pub recoils: Option<i32>,
    /// Recoil return time (`<0` = reload).
    pub recoil_time: Option<f32>,
    /// Recoil curve power.
    pub recoil_pow: Option<f32>,
    /// Heat region cooldown ticks.
    pub cooldown_time: Option<f32>,
    /// Muzzle offsets.
    pub shoot_x: Option<f32>,
    /// Muzzle Y offset.
    pub shoot_y: Option<f32>,
    /// Mount offsets.
    pub x: Option<f32>,
    /// Mount Y offset.
    pub y: Option<f32>,
    /// Random mount spread.
    pub x_rand: Option<f32>,
    /// Random mount Y spread.
    pub y_rand: Option<f32>,
    /// Shoot pattern.
    pub shoot: Option<ShootPatternSpec>,
    /// Weapon shadow radius.
    pub shadow: Option<f32>,
    /// Random velocity fraction.
    pub velocity_rnd: Option<f32>,
    /// Extra velocity fraction.
    pub extra_velocity: Option<f32>,
    /// Random lifetime fraction.
    pub life_rnd: Option<f32>,
    /// Extra lifetime fraction.
    pub extra_life: Option<f32>,
    /// Shooting cone half-radius.
    pub shoot_cone: Option<f32>,
    /// Mount rotation cone.
    pub rotation_limit: Option<f32>,
    /// Minimum warmup before firing.
    pub min_warmup: Option<f32>,
    /// Warmup lerp speed.
    pub shoot_warmup_speed: Option<f32>,
    /// Smooth-reload lerp speed.
    pub smooth_reload_speed: Option<f32>,
    /// Linear warmup flag.
    pub linear_warmup: Option<bool>,
    /// Sound pitch range.
    pub sound_pitch_min: Option<f32>,
    /// Sound pitch range max.
    pub sound_pitch_max: Option<f32>,
    /// Ignore shooter rotation when shooting.
    pub ignore_rotation: Option<bool>,
    /// Weapon cannot attack targets.
    pub no_attack: Option<bool>,
    /// Minimum shooter velocity to fire.
    pub min_shoot_velocity: Option<f32>,
    /// Maximum shooter velocity to fire.
    pub max_shoot_velocity: Option<f32>,
    /// Shoot effects follow the unit.
    pub parentize_effects: Option<bool>,
    /// Draw Z offset.
    pub layer_offset: Option<f32>,
    /// Loop sound while shooting.
    pub active_sound: Option<SoundId>,
    /// Active sound volume.
    pub active_sound_volume: Option<f32>,
    /// Shoot sound.
    pub shoot_sound: Option<SoundId>,
    /// Shoot sound volume.
    pub shoot_sound_volume: Option<f32>,
    /// First-fire sound for continuous weapons.
    pub initial_shoot_sound: Option<SoundId>,
    /// Charge-up sound for delayed weapons.
    pub charge_sound: Option<SoundId>,
    /// Shell-ejection effect.
    pub eject_effect: Option<EffectRef>,
    /// Heat region tint.
    pub heat_color: Option<Rgba>,
    /// Status applied to the shooter when shooting.
    pub shoot_status: Option<&'static str>,
    /// Shoot status duration.
    pub shoot_status_duration: Option<f32>,
    /// Fire when the owner dies.
    pub shoot_on_death: Option<bool>,
    /// Shoot-effect override for death shots.
    pub shoot_on_death_effect: Option<EffectRef>,
    /// Whether the weapon uses the alternate `WeaponMount` subclass
    /// (`RepairBeamWeapon` initializer `mountType = HealBeamMount::new`).
    pub heal_beam_mount: Option<bool>,
    /// Draw parts.
    pub parts: Vec<DrawPartSpec>,
    /// `PointDefenseWeapon.color` (beam color).
    pub beam_color: Option<Rgba>,
    /// `PointDefenseWeapon.beamEffect`.
    pub beam_effect: Option<EffectId>,
    /// `PointDefenseBulletWeapon.damageTargetWeight`.
    pub damage_target_weight: Option<f32>,
    /// `RepairBeamWeapon.targetBuildings`.
    pub target_buildings: Option<bool>,
    /// `RepairBeamWeapon.targetUnits`.
    pub target_units: Option<bool>,
    /// `RepairBeamWeapon.repairSpeed`.
    pub repair_speed: Option<f32>,
    /// `RepairBeamWeapon.fractionRepairSpeed`.
    pub fraction_repair_speed: Option<f32>,
    /// `RepairBeamWeapon.beamWidth`.
    pub beam_width: Option<f32>,
    /// `RepairBeamWeapon.pulseRadius`.
    pub pulse_radius: Option<f32>,
    /// `RepairBeamWeapon.pulseStroke`.
    pub pulse_stroke: Option<f32>,
    /// `RepairBeamWeapon.widthSinMag`.
    pub width_sin_mag: Option<f32>,
    /// `RepairBeamWeapon.widthSinScl`.
    pub width_sin_scl: Option<f32>,
    /// `RepairBeamWeapon.laserColor`.
    pub laser_color: Option<Rgba>,
    /// `RepairBeamWeapon.healColor` (block heals only).
    pub heal_color: Option<Rgba>,
}

/// Weapon metadata record (`mindustry.type.Weapon` data half).
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponDef {
    /// Region/weapon name.
    pub name: String,
    /// Java class tag.
    pub kind: WeaponKind,
    /// Bullet fired (resolved in the unit load pass).
    pub bullet: super::ResolvedBullet,
    /// Shows in unit stats.
    pub display: bool,
    /// Mirrored copy created on init (consumed by the unit mirror pass).
    pub mirror: bool,
    /// Sprite flipped on render (set by the mirror pass).
    pub flip_sprite: bool,
    /// Mirrored mounts alternate shots.
    pub alternate: bool,
    /// Rotates toward target.
    pub rotate: bool,
    /// Sprite shows in stats.
    pub show_stat_sprite: bool,
    /// Starting rotation.
    pub base_rotation: f32,
    /// Outline on top.
    pub top: bool,
    /// Continuous hold.
    pub continuous: bool,
    /// Continuous without reload.
    pub always_continuous: bool,
    /// Aim lerp speed.
    pub aim_change_speed: f32,
    /// Player-controllable.
    pub controllable: bool,
    /// AI-controllable.
    pub ai_controllable: bool,
    /// Always shooting.
    pub always_shooting: bool,
    /// Auto-targeting.
    pub auto_target: bool,
    /// Prediction.
    pub predict_target: bool,
    /// Counts for attack range.
    pub use_attack_range: bool,
    /// Target scan interval.
    pub target_interval: f32,
    /// Target switch interval.
    pub target_switch_interval: f32,
    /// Rotation speed.
    pub rotate_speed: f32,
    /// Reload ticks (doubled by the mirror pass).
    pub reload: f32,
    /// Inaccuracy degrees.
    pub inaccuracy: f32,
    /// Screen shake.
    pub shake: f32,
    /// Recoil distance.
    pub recoil: f32,
    /// Extra recoil counters.
    pub recoils: i32,
    /// Recoil return ticks (doubled by the mirror pass).
    pub recoil_time: f32,
    /// Recoil curve power.
    pub recoil_pow: f32,
    /// Heat cooldown ticks.
    pub cooldown_time: f32,
    /// Muzzle offsets.
    pub shoot_x: f32,
    /// Muzzle Y offset.
    pub shoot_y: f32,
    /// Mount offsets.
    pub x: f32,
    /// Mount Y offset.
    pub y: f32,
    /// Random mount spread.
    pub x_rand: f32,
    /// Random mount Y spread.
    pub y_rand: f32,
    /// Shoot pattern.
    pub shoot: ShootPatternSpec,
    /// Shadow radius.
    pub shadow: f32,
    /// Random velocity fraction.
    pub velocity_rnd: f32,
    /// Extra velocity fraction.
    pub extra_velocity: f32,
    /// Random lifetime fraction.
    pub life_rnd: f32,
    /// Extra lifetime fraction.
    pub extra_life: f32,
    /// Shoot cone.
    pub shoot_cone: f32,
    /// Rotation cone.
    pub rotation_limit: f32,
    /// Minimum warmup.
    pub min_warmup: f32,
    /// Warmup lerp speed.
    pub shoot_warmup_speed: f32,
    /// Smooth-reload lerp speed.
    pub smooth_reload_speed: f32,
    /// Linear warmup.
    pub linear_warmup: bool,
    /// Sound pitch min.
    pub sound_pitch_min: f32,
    /// Sound pitch max.
    pub sound_pitch_max: f32,
    /// Ignore shooter rotation.
    pub ignore_rotation: bool,
    /// Cannot attack.
    pub no_attack: bool,
    /// Minimum shooter velocity.
    pub min_shoot_velocity: f32,
    /// Maximum shooter velocity.
    pub max_shoot_velocity: f32,
    /// Effects follow the unit.
    pub parentize_effects: bool,
    /// Mirror-pass link to the other-side weapon index (`-1` = none).
    pub other_side: i32,
    /// Draw Z offset.
    pub layer_offset: f32,
    /// Loop sound while shooting.
    pub active_sound: SoundId,
    /// Active sound volume.
    pub active_sound_volume: f32,
    /// Shoot sound.
    pub shoot_sound: SoundId,
    /// Shoot sound volume.
    pub shoot_sound_volume: f32,
    /// First-fire sound.
    pub initial_shoot_sound: SoundId,
    /// Charge sound.
    pub charge_sound: SoundId,
    /// Ejection effect.
    pub eject_effect: EffectRef,
    /// Heat region tint.
    pub heat_color: Rgba,
    /// Status applied when shooting (resolved at load; `none` default).
    pub shoot_status: StatusId,
    /// Shoot status duration.
    pub shoot_status_duration: f32,
    /// Fire on death.
    pub shoot_on_death: bool,
    /// Death-shot effect override.
    pub shoot_on_death_effect: Option<EffectRef>,
    /// Uses the `HealBeamMount` subclass.
    pub heal_beam_mount: bool,
    /// Draw parts.
    pub parts: Vec<DrawPartSpec>,
    /// `PointDefenseWeapon.color`.
    pub beam_color: Rgba,
    /// `PointDefenseWeapon.beamEffect`.
    pub beam_effect: EffectId,
    /// `PointDefenseBulletWeapon.damageTargetWeight`.
    pub damage_target_weight: f32,
    /// `RepairBeamWeapon.targetBuildings`.
    pub target_buildings: bool,
    /// `RepairBeamWeapon.targetUnits`.
    pub target_units: bool,
    /// `RepairBeamWeapon.repairSpeed`.
    pub repair_speed: f32,
    /// `RepairBeamWeapon.fractionRepairSpeed`.
    pub fraction_repair_speed: f32,
    /// `RepairBeamWeapon.beamWidth`.
    pub beam_width: f32,
    /// `RepairBeamWeapon.pulseRadius`.
    pub pulse_radius: f32,
    /// `RepairBeamWeapon.pulseStroke`.
    pub pulse_stroke: f32,
    /// `RepairBeamWeapon.widthSinMag`.
    pub width_sin_mag: f32,
    /// `RepairBeamWeapon.widthSinScl`.
    pub width_sin_scl: f32,
    /// `RepairBeamWeapon.laserColor`.
    pub laser_color: Rgba,
    /// `RepairBeamWeapon.healColor` (block heals only).
    pub heal_color: Rgba,
}

impl WeaponDef {
    /// Resolves a generated [`WeaponSpec`], applying the Java `Weapon` field
    /// defaults and the class-tag instance-initializer overrides
    /// (`weapons/*.java` `{ ... }` blocks).
    pub fn from_spec(
        spec: WeaponSpec,
        bullet: super::ResolvedBullet,
        registry: &ContentRegistry,
    ) -> Result<Self, ContentError> {
        let mut def = Self {
            name: spec.name.to_owned(),
            kind: spec.kind,
            bullet,
            display: true,
            mirror: true,
            flip_sprite: false,
            alternate: true,
            rotate: false,
            show_stat_sprite: true,
            base_rotation: 0.0,
            top: true,
            continuous: false,
            always_continuous: false,
            aim_change_speed: f32::INFINITY,
            controllable: true,
            ai_controllable: true,
            always_shooting: false,
            auto_target: false,
            predict_target: true,
            use_attack_range: true,
            target_interval: 40.0,
            target_switch_interval: 70.0,
            rotate_speed: 20.0,
            reload: 1.0,
            inaccuracy: 0.0,
            shake: 0.0,
            recoil: 1.5,
            recoils: -1,
            recoil_time: -1.0,
            recoil_pow: 1.8,
            cooldown_time: 20.0,
            shoot_x: 0.0,
            shoot_y: 3.0,
            x: 5.0,
            y: 0.0,
            x_rand: 0.0,
            y_rand: 0.0,
            shoot: ShootPatternSpec::default(),
            shadow: -1.0,
            velocity_rnd: 0.0,
            extra_velocity: 0.0,
            life_rnd: 0.0,
            extra_life: 0.0,
            shoot_cone: 5.0,
            rotation_limit: 361.0,
            min_warmup: 0.0,
            shoot_warmup_speed: 0.1,
            smooth_reload_speed: 0.15,
            linear_warmup: false,
            sound_pitch_min: 0.8,
            sound_pitch_max: 1.0,
            ignore_rotation: false,
            no_attack: false,
            min_shoot_velocity: -1.0,
            max_shoot_velocity: -1.0,
            parentize_effects: false,
            other_side: -1,
            layer_offset: 0.0,
            active_sound: SoundId::NONE,
            active_sound_volume: 1.0,
            shoot_sound: SoundId::SHOOT,
            shoot_sound_volume: 1.0,
            initial_shoot_sound: SoundId::NONE,
            charge_sound: SoundId::NONE,
            eject_effect: EffectRef::Named(EffectId::NONE),
            heat_color: pal::TURRET_HEAT,
            shoot_status: StatusId::NONE,
            shoot_status_duration: 60.0 * 5.0,
            shoot_on_death: false,
            shoot_on_death_effect: None,
            heal_beam_mount: false,
            parts: spec.parts,
            beam_color: Rgba::WHITE,
            beam_effect: EffectId::POINT_BEAM,
            damage_target_weight: 10.0,
            target_buildings: false,
            target_units: true,
            repair_speed: 0.3,
            fraction_repair_speed: 0.0,
            beam_width: 1.0,
            pulse_radius: 6.0,
            pulse_stroke: 2.0,
            width_sin_mag: 0.0,
            width_sin_scl: 4.0,
            laser_color: pal::REPAIR_LASER,
            heal_color: pal::HEAL,
        };
        // Class-tag instance-initializer overrides (`weapons/*.java` `{...}`).
        match spec.kind {
            WeaponKind::Weapon => {}
            WeaponKind::BuildWeapon | WeaponKind::MineWeapon => {
                def.rotate = true;
                def.no_attack = true;
                def.predict_target = false;
                def.display = false;
                def.use_attack_range = false;
                // `bullet = new BulletType()` is emitted inline by the generator.
            }
            WeaponKind::PointDefenseWeapon => {}
            WeaponKind::PointDefenseBulletWeapon => {
                def.auto_target = true;
                def.controllable = false;
                def.rotate = true;
                def.use_attack_range = false;
                def.target_interval = 5.0;
                def.target_switch_interval = 5.0;
            }
            WeaponKind::RepairBeamWeapon => {
                def.reload = 1.0;
                def.predict_target = false;
                def.auto_target = true;
                def.controllable = false;
                def.rotate = true;
                def.heal_beam_mount = true;
                def.recoil = 0.0;
            }
        }
        // Spec overrides.
        macro_rules! apply {
            ($($field:ident),* $(,)?) => {
                $(if let Some(value) = spec.$field {
                    def.$field = value;
                })*
            };
        }
        apply!(
            display,
            mirror,
            alternate,
            rotate,
            show_stat_sprite,
            base_rotation,
            top,
            continuous,
            always_continuous,
            aim_change_speed,
            controllable,
            ai_controllable,
            always_shooting,
            auto_target,
            predict_target,
            use_attack_range,
            target_interval,
            target_switch_interval,
            rotate_speed,
            reload,
            inaccuracy,
            shake,
            recoil,
            recoils,
            recoil_time,
            recoil_pow,
            cooldown_time,
            shoot_x,
            shoot_y,
            x,
            y,
            x_rand,
            y_rand,
            shadow,
            velocity_rnd,
            extra_velocity,
            life_rnd,
            extra_life,
            shoot_cone,
            rotation_limit,
            min_warmup,
            shoot_warmup_speed,
            smooth_reload_speed,
            linear_warmup,
            sound_pitch_min,
            sound_pitch_max,
            ignore_rotation,
            no_attack,
            min_shoot_velocity,
            max_shoot_velocity,
            parentize_effects,
            layer_offset,
            active_sound_volume,
            shoot_sound_volume,
            heat_color,
            shoot_status_duration,
            shoot_on_death,
            heal_beam_mount,
            beam_color,
            damage_target_weight,
            target_buildings,
            target_units,
            repair_speed,
            fraction_repair_speed,
            beam_width,
            pulse_radius,
            pulse_stroke,
            width_sin_mag,
            width_sin_scl,
            laser_color,
            heal_color,
        );
        if let Some(shoot) = spec.shoot {
            def.shoot = shoot;
        }
        if let Some(sound) = spec.active_sound {
            def.active_sound = sound;
        }
        if let Some(sound) = spec.shoot_sound {
            def.shoot_sound = sound;
        }
        if let Some(sound) = spec.initial_shoot_sound {
            def.initial_shoot_sound = sound;
        }
        if let Some(sound) = spec.charge_sound {
            def.charge_sound = sound;
        }
        if let Some(effect) = spec.eject_effect {
            def.eject_effect = effect;
        }
        if let Some(effect) = spec.beam_effect {
            def.beam_effect = effect;
        }
        if let Some(effect) = spec.shoot_on_death_effect {
            def.shoot_on_death_effect = Some(effect);
        }
        if let Some(status) = spec.shoot_status {
            def.shoot_status = registry
                .status_id(status)
                .ok_or_else(|| ContentError::UnknownName(status.to_owned()))?;
        }
        Ok(def)
    }

    /// `Weapon.init()`: `alwaysContinuous` implies `continuous`.
    pub fn init(&mut self) {
        if self.always_continuous {
            self.continuous = true;
        }
    }

    /// `Weapon.flip()` — negates mount geometry and flips the shoot pattern.
    pub fn flip(&mut self) {
        self.x *= -1.0;
        self.shoot_x *= -1.0;
        self.base_rotation *= -1.0;
        self.flip_sprite = !self.flip_sprite;
        self.shoot.flip();
    }

    /// `Weapon.range()` — the bullet's (derived) range.
    pub fn range(&self) -> f32 {
        self.bullet.range()
    }

    /// `Weapon.dps()` (`bullet.estimateDPS() / reload * shoot.shots * 60`).
    pub fn dps(&self) -> f32 {
        (self.bullet.estimate_dps() / self.reload) * self.shoot.shots as f32 * 60.0
    }
}

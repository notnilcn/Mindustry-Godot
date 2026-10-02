// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Bullets.java (6 shared internal bullets),
//         core/src/mindustry/entities/bullet/BulletType.java (base fields,
//         `init`/`calculateRange` metadata derivations),
//         {Basic,Artillery,Missile,LaserBolt,Sap,Lightning,Laser,Flak,Explosion,
//         Rail,Continuous(Flame)Laser,Shrapnel,Liquid,Emp,Bomb,Fire,
//         SpaceLiquid}BulletType.java (class defaults only; behavior in plan 10).

//! Bullet metadata registry.
//!
//! M1 ported the 6 shared internal bullets; M5 generalizes [`BulletDef`] to the
//! full metadata half so weapon bullets from `UnitTypes.java` register in the
//! same content space (upstream registers every `new *BulletType` at
//! construction, in load order). Turret ammo bullets from `Blocks.java` are
//! *not* registered yet — plan 10 ports them with turret behavior; upstream
//! constructs them after `UnitTypes.load()`, so the current bullet id space is
//! an upstream-prefix (ids 0..N match) and stays append-only.
//!
//! [`BulletSpec`] is the generated wave input (name/effect references resolved
//! at load); nested inline content (`fragBullet`, `intervalBullet`,
//! `lightningType`, `spawnBullets`, `spawnUnit`) is registered by the unit load
//! sink, which patches the resolved ids.

use super::fx_meta::{EffectId, EffectRef};
use super::pal;
use super::sound_meta::SoundId;
use super::units::UnitSpec;
use super::units::parts::InterpKind;
use crate::content::color::Rgba;
use crate::content::ctype::{Content, ModContentInfo};
use crate::content::id::{BulletId, LiquidId, StatusId, UnitTypeId};
use crate::content::load::ContentRegistry;
use crate::content::{ContentError, ContentType};

/// `Bullets.placeholder` id (first internal bullet).
pub const PLACEHOLDER: BulletId = BulletId::new(0);
/// `Bullets.damageLightning` id (load order pinned by `bullets::tests`).
pub const DAMAGE_LIGHTNING: BulletId = BulletId::new(1);
/// `Bullets.damageLightningGround` id.
pub const DAMAGE_LIGHTNING_GROUND: BulletId = BulletId::new(2);
/// `Bullets.damageLightningAir` id.
pub const DAMAGE_LIGHTNING_AIR: BulletId = BulletId::new(3);

/// Bullet class tag (`BulletType` hierarchy; content ABI, append-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BulletKind {
    /// Plain `BulletType`.
    #[default]
    Plain = 0,
    /// `BasicBulletType`.
    Basic = 1,
    /// `FireBulletType`.
    Fire = 2,
    /// `SpaceLiquidBulletType`.
    SpaceLiquid = 3,
    /// `ArtilleryBulletType`.
    Artillery = 4,
    /// `MissileBulletType`.
    Missile = 5,
    /// `LaserBoltBulletType`.
    LaserBolt = 6,
    /// `SapBulletType`.
    Sap = 7,
    /// `LightningBulletType`.
    Lightning = 8,
    /// `LaserBulletType`.
    Laser = 9,
    /// `FlakBulletType`.
    Flak = 10,
    /// `ExplosionBulletType`.
    Explosion = 11,
    /// `RailBulletType`.
    Rail = 12,
    /// `ContinuousLaserBulletType` (extends `ContinuousBulletType`).
    ContinuousLaser = 13,
    /// `ShrapnelBulletType`.
    Shrapnel = 14,
    /// `LiquidBulletType`.
    Liquid = 15,
    /// `EmpBulletType`.
    Emp = 16,
    /// `BombBulletType`.
    Bomb = 17,
}

impl BulletKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            BulletKind::Plain => "BulletType",
            BulletKind::Basic => "BasicBulletType",
            BulletKind::Fire => "FireBulletType",
            BulletKind::SpaceLiquid => "SpaceLiquidBulletType",
            BulletKind::Artillery => "ArtilleryBulletType",
            BulletKind::Missile => "MissileBulletType",
            BulletKind::LaserBolt => "LaserBoltBulletType",
            BulletKind::Sap => "SapBulletType",
            BulletKind::Lightning => "LightningBulletType",
            BulletKind::Laser => "LaserBulletType",
            BulletKind::Flak => "FlakBulletType",
            BulletKind::Explosion => "ExplosionBulletType",
            BulletKind::Rail => "RailBulletType",
            BulletKind::ContinuousLaser => "ContinuousLaserBulletType",
            BulletKind::Shrapnel => "ShrapnelBulletType",
            BulletKind::Liquid => "LiquidBulletType",
            BulletKind::Emp => "EmpBulletType",
            BulletKind::Bomb => "BombBulletType",
        }
    }
}

/// Bullet content record (`mindustry.entities.bullet.BulletType` metadata).
///
/// The record is the union of the base `BulletType` fields and the subclass
/// fields used by vanilla content; fields that do not apply to a kind keep the
/// `Basic`/`BulletType` base defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct BulletDef {
    /// Dense id in the bullet content space.
    pub id: BulletId,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Class-kind tag.
    pub kind: BulletKind,
    /// Lifetime in ticks.
    pub lifetime: f32,
    /// Speed in units/tick.
    pub speed: f32,
    /// Direct damage on hit.
    pub damage: f32,
    /// Hitbox size.
    pub hit_size: f32,
    /// Clipping hitbox (derived with trail length).
    pub draw_size: f32,
    /// Drag as a fraction of velocity.
    pub drag: f32,
    /// Acceleration per frame.
    pub accel: f32,
    /// Whether velocity is inherited from the shooter.
    pub keep_velocity: bool,
    /// Whether keep-velocity is scaled by the shooter velocity fraction.
    pub scale_keep_velocity: bool,
    /// Whether lifetime is scaled to disappear at the target (artillery).
    pub scale_life: bool,
    /// Whether to pierce units.
    pub pierce: bool,
    /// Whether to pierce buildings.
    pub pierce_building: bool,
    /// Max number of pierced objects (`-1` unlimited).
    pub pierce_cap: i32,
    /// Multiplier of damage decreased per health pierced.
    pub pierce_damage_factor: f32,
    /// Whether the bullet is removed after `pierceCap` is exceeded.
    pub remove_after_pierce: bool,
    /// Absorbed by plastanium walls (piercing lasers).
    pub laser_absorb: bool,
    /// Whether this counts as a laser bullet for wall absorption.
    pub laser_bullet: bool,
    /// Life fraction of peak effectiveness (continuous weapons).
    pub optimal_life_fract: f32,
    /// Multiplied by turret reload speed to get the final shoot speed.
    pub reload_multiplier: f32,
    /// Ammo created per item/liquid.
    pub ammo_multiplier: f32,
    /// Whether the ammo multiplier shows in stats.
    pub display_ammo_multiplier: bool,
    /// Recoil applied to the shooter.
    pub recoil: f32,
    /// Kills the shooter when fired.
    pub kill_shooter: bool,
    /// Disappears instantly.
    pub instant_disappear: bool,
    /// Splash damage (`0` disables).
    pub splash_damage: f32,
    /// Splash damage radius (`<0` disables).
    pub splash_damage_radius: f32,
    /// Whether splash damage pierces.
    pub splash_damage_pierce: bool,
    /// Whether splash damage is scaled by hitbox size.
    pub scaled_splash_damage: bool,
    /// Knockback in velocity.
    pub knockback: f32,
    /// Knockback follows the bullet direction.
    pub impact: bool,
    /// Damage multiplier against tiles.
    pub building_damage_multiplier: f32,
    /// Damage multiplier against force shields.
    pub shield_damage_multiplier: f32,
    /// Status effect applied on hit.
    pub status: StatusId,
    /// Applied status duration.
    pub status_duration: f32,
    /// Chance to apply the status.
    pub status_chance: f32,
    /// Whether unit armor is ignored.
    pub pierce_armor: bool,
    /// Multiplier of unit/building armor used in damage calculations.
    pub armor_multiplier: f32,
    /// Multiplier of building armor only.
    pub block_armor_multiplier: f32,
    /// Whether the bullet can be hit by point defense.
    pub hittable: bool,
    /// Whether the bullet can be reflected.
    pub reflectable: bool,
    /// Whether the projectile can be absorbed by shields.
    pub absorbable: bool,
    /// Whether the bullet collides with anything at all.
    pub collides: bool,
    /// Whether the bullet collides with air units.
    pub collides_air: bool,
    /// Whether the bullet collides with ground units.
    pub collides_ground: bool,
    /// Whether the bullet collides with tiles.
    pub collides_tiles: bool,
    /// Whether the bullet collides with friendly tiles.
    pub collides_team: bool,
    /// Whether the bullet collides with floors (dyn. walls).
    pub collide_floor: bool,
    /// Whether the bullet collides with terrain walls.
    pub collide_terrain: bool,
    /// Extra inaccuracy when firing.
    pub inaccuracy: f32,
    /// Effect shown on direct hit.
    pub hit_effect: EffectRef,
    /// Effect shown when the bullet despawns.
    pub despawn_effect: EffectRef,
    /// Effect created when shooting.
    pub shoot_effect: EffectRef,
    /// Extra smoke effect created when shooting.
    pub smoke_effect: EffectRef,
    /// Charge effect (single-shot weapons with `firstShotDelay`).
    pub charge_effect: EffectRef,
    /// Trail effect spawned behind the bullet.
    pub trail_effect: EffectRef,
    /// Overrides the shoot sound in turrets (unused by units).
    pub shoot_sound: SoundId,
    /// Sound made when hitting something.
    pub hit_sound: SoundId,
    /// Sound made when despawning.
    pub despawn_sound: SoundId,
    /// Hit sound volume.
    pub hit_sound_volume: f32,
    /// Hit sound pitch.
    pub hit_sound_pitch: f32,
    /// Hit sound pitch range.
    pub hit_sound_pitch_range: f32,
    /// Screen shake on hit.
    pub hit_shake: f32,
    /// Screen shake on despawn.
    pub despawn_shake: f32,
    /// Whether hit effects also play on despawn (derived).
    pub despawn_hit: bool,
    /// Trail length (`<=0` disables the trail).
    pub trail_length: i32,
    /// Trail width.
    pub trail_width: f32,
    /// Trail spawn chance per tick.
    pub trail_chance: f32,
    /// Trail spawn interval.
    pub trail_interval: f32,
    /// Trail effect parameter.
    pub trail_param: f32,
    /// Trail spread.
    pub trail_spread: f32,
    /// Trail color.
    pub trail_color: Rgba,
    /// Whether the trail rotates with the bullet.
    pub trail_rotation: bool,
    /// Trail shrink interpolation.
    pub trail_interp: InterpKind,
    /// Frag bullet created on hit/despawn.
    pub frag_bullet: Option<BulletId>,
    /// Number of frag bullets created.
    pub frag_bullets: i32,
    /// Whether frags are created on hit.
    pub frag_on_hit: bool,
    /// Whether frags are created on despawn.
    pub frag_on_despawn: bool,
    /// Whether frags are created on absorb.
    pub frag_on_absorb: bool,
    /// Frags delayed to despawn.
    pub delay_frags: bool,
    /// Frag random spread.
    pub frag_random_spread: f32,
    /// Frag uniform spread.
    pub frag_spread: f32,
    /// Frag angle offset.
    pub frag_angle: f32,
    /// Frag velocity range.
    pub frag_velocity_min: f32,
    /// Frag velocity max.
    pub frag_velocity_max: f32,
    /// Frag lifetime range.
    pub frag_life_min: f32,
    /// Frag lifetime max.
    pub frag_life_max: f32,
    /// Frag offset range.
    pub frag_offset_min: f32,
    /// Frag offset max.
    pub frag_offset_max: f32,
    /// Max frags per pierce (`-1` unlimited).
    pub pierce_frag_cap: i32,
    /// Bullet created at a fixed interval.
    pub interval_bullet: Option<BulletId>,
    /// Interval in ticks between interval bullets.
    pub bullet_interval: f32,
    /// Bullets created per interval.
    pub interval_bullets: i32,
    /// Interval random spread.
    pub interval_random_spread: f32,
    /// Interval spread.
    pub interval_spread: f32,
    /// Interval angle offset.
    pub interval_angle: f32,
    /// First interval delay.
    pub interval_delay: f32,
    /// Whether the bullet travels underwater.
    pub underwater: bool,
    /// Lightning root count.
    pub lightning: i32,
    /// Lightning strand length.
    pub lightning_length: i32,
    /// Lightning random extra length.
    pub lightning_length_rand: i32,
    /// Lightning damage (`<0` uses bullet damage).
    pub lightning_damage: f32,
    /// Lightning spread cone.
    pub lightning_cone: f32,
    /// Lightning angle offset.
    pub lightning_angle: f32,
    /// Lightning bullet override (derived default in `link`).
    pub lightning_type: Option<BulletId>,
    /// Lightning color.
    pub lightning_color: Rgba,
    /// Bullets spawned around this one (vanquish).
    pub spawn_bullets: Vec<BulletId>,
    /// Spawn bullet random spread.
    pub spawn_bullet_random_spread: f32,
    /// Number of puddles created.
    pub puddles: i32,
    /// Range of puddles around the bullet.
    pub puddle_range: f32,
    /// Liquid count per puddle.
    pub puddle_amount: f32,
    /// Liquid the puddles consist of.
    pub puddle_liquid: LiquidId,
    /// Unit spawned instead of the bullet.
    pub spawn_unit: Option<UnitTypeId>,
    /// Unit spawned on despawn.
    pub despawn_unit: Option<UnitTypeId>,
    /// Suppression field range (`<0` disables).
    pub suppression_range: f32,
    /// Suppression status duration.
    pub suppression_duration: f32,
    /// Suppression effect chance.
    pub suppression_effect_chance: f32,
    /// Suppression color.
    pub suppress_color: Rgba,
    /// Front sprite region name.
    pub sprite: Option<String>,
    /// Back sprite region name.
    pub back_sprite: Option<String>,
    /// Sprite width (`width` on `Basic`/laser/shrapnel classes).
    pub width: f32,
    /// Sprite height.
    pub height: f32,
    /// Back sprite color.
    pub back_color: Rgba,
    /// Front sprite color.
    pub front_color: Rgba,
    /// Color lerp start (`Basic.mixColorFrom`).
    pub mix_color_from: Rgba,
    /// Color lerp end (`Basic.mixColorTo`).
    pub mix_color_to: Rgba,
    /// X shrink over lifetime.
    pub shrink_x: f32,
    /// Y shrink over lifetime.
    pub shrink_y: f32,
    /// Shrink interpolation.
    pub shrink_interp: InterpKind,
    /// Sprite spin.
    pub spin: f32,
    /// Sprite rotation offset.
    pub rotation_offset: f32,
    /// Laser color gradient (`Laser`/`ContinuousLaser`).
    pub colors: Vec<Rgba>,
    /// Beam/ray length (`Laser`/`Rail`/`Sap`/`Shrapnel`/`ContinuousLaser`).
    pub length: f32,
    /// `ContinuousBulletType.damageInterval`.
    pub damage_interval: f32,
    /// `ContinuousBulletType.shake`.
    pub shake: f32,
    /// Large hitbox display (`Laser.largeHit`).
    pub large_hit: bool,
    /// Laser side ray length.
    pub side_length: f32,
    /// Laser side ray width.
    pub side_width: f32,
    /// Laser side ray angle.
    pub side_angle: f32,
    /// Laser lightning spacing (`<0` derives).
    pub lightning_spacing: f32,
    /// Laser lightning delay.
    pub lightning_delay: f32,
    /// Laser lightning random angle.
    pub lightning_angle_rand: f32,
    /// `ShrapnelBulletType` serration count.
    pub serrations: i32,
    /// Serration length scale.
    pub serration_len_scl: f32,
    /// Serration width.
    pub serration_width: f32,
    /// Serration spacing.
    pub serration_spacing: f32,
    /// Serration space offset.
    pub serration_space_offset: f32,
    /// Serration fade offset.
    pub serration_fade_offset: f32,
    /// Shrapnel start color.
    pub from_color: Rgba,
    /// Shrapnel end color.
    pub to_color: Rgba,
    /// `SapBulletType.sapStrength`.
    pub sap_strength: f32,
    /// `SapBulletType.color` (beam color).
    pub beam_color: Rgba,
    /// `SapBulletType.lengthRand`.
    pub length_rand: f32,
    /// `EmpBulletType.radius`.
    pub emp_radius: f32,
    /// `EmpBulletType.timeIncrease`.
    pub time_increase: f32,
    /// `EmpBulletType.timeDuration`.
    pub time_duration: f32,
    /// `EmpBulletType.powerDamageScl`.
    pub power_damage_scl: f32,
    /// `EmpBulletType.powerSclDecrease`.
    pub power_scl_decrease: f32,
    /// `EmpBulletType.unitDamageScl`.
    pub unit_damage_scl: f32,
    /// `EmpBulletType.hitPowerEffect`.
    pub hit_power_effect: EffectRef,
    /// `EmpBulletType.chainEffect`.
    pub chain_effect: EffectRef,
    /// `EmpBulletType.applyEffect`.
    pub apply_effect: EffectRef,
    /// `EmpBulletType.hitUnits`.
    pub hit_units: bool,
    /// `RailBulletType.pierceEffect`.
    pub pierce_effect: EffectRef,
    /// `RailBulletType.pointEffect`.
    pub point_effect: EffectRef,
    /// `RailBulletType.lineEffect`.
    pub line_effect: EffectRef,
    /// `RailBulletType.endEffect`.
    pub end_effect: EffectRef,
    /// `RailBulletType.pointEffectSpace`.
    pub point_effect_space: f32,
    /// Draw layer (`BulletType.layer = Layer.bullet`).
    pub layer: f32,
    /// `ArtilleryBulletType.trailMult`.
    pub trail_mult: f32,
    /// `ArtilleryBulletType.trailSize`.
    pub trail_size: f32,
    /// `FlakBulletType.explodeRange`.
    pub explode_range: f32,
    /// `FlakBulletType.explodeDelay`.
    pub explode_delay: f32,
    /// `FlakBulletType.flakDelay`.
    pub flak_delay: f32,
    /// `FlakBulletType.flakInterval`.
    pub flak_interval: f32,
    /// Range cap (`<0` none).
    pub max_range: f32,
    /// Range override (`<0` none; `ExplosionBulletType` sets it in the ctor).
    pub range_override: f32,
    /// Derived range (`BulletType.init` → `calculateRange`).
    pub range: f32,
    /// Homing power.
    pub homing_power: f32,
    /// Homing range.
    pub homing_range: f32,
    /// Homing delay.
    pub homing_delay: f32,
    /// Follow-aim speed (lasers).
    pub follow_aim_speed: f32,
    /// Weave scale.
    pub weave_scale: f32,
    /// Weave magnitude.
    pub weave_mag: f32,
    /// Weave randomness flag.
    pub weave_random: bool,
    /// Color used for hit/despawn effects.
    pub hit_color: Rgba,
    /// Radius of emitted light (derived when `<= -1`).
    pub light_radius: f32,
    /// Emitted light opacity.
    pub light_opacity: f32,
    /// Color of light emitted by the bullet.
    pub light_color: Rgba,
    /// Whether the bullet creates fires on impact.
    pub make_fire: bool,
    /// Number of fires attempted around the bullet.
    pub incend_amount: i32,
    /// Spread of fires around the bullet.
    pub incend_spread: f32,
    /// Chance of fire creation.
    pub incend_chance: f32,
    /// Lifesteal fraction of dealt damage.
    pub lifesteal: f32,
    /// Flat block healing on hit.
    pub heal_amount: f32,
    /// Percent block healing on hit.
    pub heal_percent: f32,
    /// Show in unit stat blocks.
    pub show_stats: bool,
    /// Whether `init()` applies the default status/despawn-hit rules.
    pub set_defaults: bool,
    /// `(lifetime, speed)` snapshot of `spawnUnit` for `calculateRange`.
    pub(crate) spawn_unit_range: Option<(f32, f32)>,
    /// `(lifetime, speed)` snapshot of `despawnUnit` for `calculateRange`.
    pub(crate) despawn_unit_range: Option<(f32, f32)>,
}

impl BulletDef {
    /// Creates a bullet with `BulletType` defaults; kind-specific subclass
    /// constructor/initializer defaults are applied here
    /// (`entities/bullet/*.java`).
    pub fn new(kind: BulletKind) -> Self {
        let mut bullet = Self {
            id: BulletId::new(0),
            minfo: ModContentInfo::default(),
            removed: false,
            kind,
            lifetime: 40.0,
            speed: 1.0,
            damage: 1.0,
            hit_size: 4.0,
            draw_size: 40.0,
            drag: 0.0,
            accel: 0.0,
            keep_velocity: true,
            scale_keep_velocity: false,
            scale_life: false,
            pierce: false,
            pierce_building: false,
            pierce_cap: -1,
            pierce_damage_factor: 0.0,
            remove_after_pierce: true,
            laser_absorb: true,
            laser_bullet: false,
            optimal_life_fract: 0.0,
            reload_multiplier: 1.0,
            ammo_multiplier: 2.0,
            display_ammo_multiplier: true,
            recoil: 0.0,
            kill_shooter: false,
            instant_disappear: false,
            splash_damage: 0.0,
            splash_damage_radius: -1.0,
            splash_damage_pierce: false,
            scaled_splash_damage: false,
            knockback: 0.0,
            impact: false,
            building_damage_multiplier: 1.0,
            shield_damage_multiplier: 1.0,
            status: StatusId::NONE,
            status_duration: 60.0 * 8.0,
            status_chance: 1.0,
            pierce_armor: false,
            armor_multiplier: 1.0,
            block_armor_multiplier: 1.0,
            hittable: true,
            reflectable: true,
            absorbable: true,
            collides: true,
            collides_air: true,
            collides_ground: true,
            collides_tiles: true,
            collides_team: false,
            collide_floor: false,
            collide_terrain: false,
            inaccuracy: 0.0,
            hit_effect: EffectRef::Named(EffectId::HIT_BULLET_SMALL),
            despawn_effect: EffectRef::Named(EffectId::HIT_BULLET_SMALL),
            shoot_effect: EffectRef::Named(EffectId::SHOOT_SMALL),
            smoke_effect: EffectRef::Named(EffectId::SHOOT_SMALL_SMOKE),
            charge_effect: EffectRef::Named(EffectId::NONE),
            trail_effect: EffectRef::Named(EffectId::MISSILE_TRAIL),
            shoot_sound: SoundId::NONE,
            hit_sound: SoundId::NONE,
            despawn_sound: SoundId::NONE,
            hit_sound_volume: 1.0,
            hit_sound_pitch: 1.0,
            hit_sound_pitch_range: 0.1,
            hit_shake: 0.0,
            despawn_shake: 0.0,
            despawn_hit: false,
            trail_length: -1,
            trail_width: 2.0,
            trail_chance: -0.0001,
            trail_interval: 0.0,
            trail_param: 2.0,
            trail_spread: 0.0,
            trail_color: pal::MISSILE_YELLOW_BACK,
            trail_rotation: false,
            trail_interp: InterpKind::One,
            frag_bullet: None,
            frag_bullets: 9,
            frag_on_hit: true,
            frag_on_despawn: true,
            frag_on_absorb: true,
            delay_frags: false,
            frag_random_spread: 360.0,
            frag_spread: 0.0,
            frag_angle: 0.0,
            frag_velocity_min: 0.2,
            frag_velocity_max: 1.0,
            frag_life_min: 1.0,
            frag_life_max: 1.0,
            frag_offset_min: 1.0,
            frag_offset_max: 7.0,
            pierce_frag_cap: -1,
            interval_bullet: None,
            bullet_interval: 20.0,
            interval_bullets: 1,
            interval_random_spread: 360.0,
            interval_spread: 0.0,
            interval_angle: 0.0,
            interval_delay: -1.0,
            underwater: false,
            lightning: 0,
            lightning_length: 5,
            lightning_length_rand: 0,
            lightning_damage: -1.0,
            lightning_cone: 360.0,
            lightning_angle: 0.0,
            lightning_type: None,
            lightning_color: pal::SURGE,
            spawn_bullets: Vec::new(),
            spawn_bullet_random_spread: 0.0,
            puddles: 0,
            puddle_range: 0.0,
            puddle_amount: 5.0,
            puddle_liquid: LiquidId::WATER,
            spawn_unit: None,
            despawn_unit: None,
            suppression_range: -1.0,
            suppression_duration: 60.0 * 8.0,
            suppression_effect_chance: 50.0,
            suppress_color: pal::SAP_BULLET,
            sprite: None,
            back_sprite: None,
            width: 5.0,
            height: 7.0,
            back_color: pal::BULLET_YELLOW_BACK,
            front_color: pal::BULLET_YELLOW,
            mix_color_from: Rgba::CLEAR,
            mix_color_to: Rgba::CLEAR,
            shrink_x: 0.0,
            shrink_y: 0.5,
            shrink_interp: InterpKind::Linear,
            spin: 0.0,
            rotation_offset: 0.0,
            colors: Vec::new(),
            length: 0.0,
            damage_interval: 5.0,
            shake: 0.0,
            large_hit: false,
            side_length: 29.0,
            side_width: 0.7,
            side_angle: 90.0,
            lightning_spacing: -1.0,
            lightning_delay: 0.1,
            lightning_angle_rand: 0.0,
            serrations: 7,
            serration_len_scl: 10.0,
            serration_width: 4.0,
            serration_spacing: 8.0,
            serration_space_offset: 80.0,
            serration_fade_offset: 0.5,
            from_color: Rgba::WHITE,
            to_color: pal::LANCER_LASER,
            sap_strength: 0.5,
            beam_color: Rgba::WHITE,
            length_rand: 0.0,
            emp_radius: 100.0,
            time_increase: 2.5,
            time_duration: 60.0 * 10.0,
            power_damage_scl: 2.0,
            power_scl_decrease: 0.2,
            unit_damage_scl: 0.7,
            hit_power_effect: EffectRef::Named(EffectId::HIT_EMP_SPARK),
            chain_effect: EffectRef::Named(EffectId::CHAIN_EMP),
            apply_effect: EffectRef::Named(EffectId::HEAL),
            hit_units: true,
            pierce_effect: EffectRef::Named(EffectId::HIT_BULLET_SMALL),
            point_effect: EffectRef::Named(EffectId::NONE),
            line_effect: EffectRef::Named(EffectId::NONE),
            end_effect: EffectRef::Named(EffectId::NONE),
            point_effect_space: 20.0,
            layer: 100.0,
            trail_mult: 1.0,
            trail_size: 4.0,
            explode_range: 30.0,
            explode_delay: 5.0,
            flak_delay: 0.0,
            flak_interval: 6.0,
            max_range: -1.0,
            range_override: -1.0,
            range: 0.0,
            homing_power: 0.0,
            homing_range: 50.0,
            homing_delay: -1.0,
            follow_aim_speed: 0.0,
            weave_scale: 1.0,
            weave_mag: 0.0,
            weave_random: true,
            hit_color: Rgba::WHITE,
            light_radius: -1.0,
            light_opacity: 0.3,
            light_color: pal::POWER_LIGHT,
            make_fire: false,
            incend_amount: 0,
            incend_spread: 8.0,
            incend_chance: 1.0,
            lifesteal: 0.0,
            heal_amount: 0.0,
            heal_percent: 0.0,
            show_stats: false,
            set_defaults: true,
            spawn_unit_range: None,
            despawn_unit_range: None,
        };
        match kind {
            BulletKind::Plain => {}
            BulletKind::Basic => {
                bullet.sprite = Some(String::from("bullet"));
            }
            BulletKind::Fire => {
                // `FireBulletType` instance initializer.
                bullet.pierce = true;
                bullet.collides_tiles = false;
                bullet.collides = false;
                bullet.drag = 0.03;
                bullet.hit_effect = EffectRef::Named(EffectId::NONE);
                bullet.despawn_effect = EffectRef::Named(EffectId::NONE);
                bullet.trail_effect = EffectRef::Named(EffectId::FIREBALLSMOKE);
            }
            BulletKind::SpaceLiquid => {
                // `SpaceLiquidBulletType()` constructor.
                bullet.speed = 3.5;
                bullet.damage = 0.0;
                bullet.collides = false;
                bullet.lifetime = 90.0;
                bullet.despawn_effect = EffectRef::Named(EffectId::NONE);
                bullet.hit_effect = EffectRef::Named(EffectId::NONE);
                bullet.smoke_effect = EffectRef::Named(EffectId::NONE);
                bullet.shoot_effect = EffectRef::Named(EffectId::NONE);
                bullet.drag = 0.002;
                bullet.hittable = false;
            }
            BulletKind::Artillery => {
                // `ArtilleryBulletType(speed, damage, "shell")` constructor.
                bullet.sprite = Some(String::from("shell"));
                bullet.collides_tiles = false;
                bullet.collides = false;
                bullet.collides_air = false;
                bullet.scale_life = true;
                bullet.hit_shake = 1.0;
                bullet.hit_sound = SoundId::EXPLOSION_ARTILLERY;
                bullet.hit_effect = EffectRef::Named(EffectId::FLAK_EXPLOSION);
                bullet.shoot_effect = EffectRef::Named(EffectId::SHOOT_BIG);
                bullet.trail_effect = EffectRef::Named(EffectId::ARTILLERY_TRAIL);
                bullet.shrink_x = 0.15;
                bullet.shrink_y = 0.5;
                bullet.shrink_interp = InterpKind::Slope;
            }
            BulletKind::Missile => {
                // `MissileBulletType(speed, damage, "missile")` constructor.
                bullet.sprite = Some(String::from("missile"));
                bullet.back_color = pal::MISSILE_YELLOW_BACK;
                bullet.front_color = pal::MISSILE_YELLOW;
                bullet.homing_power = 0.08;
                bullet.shrink_y = 0.0;
                bullet.width = 8.0;
                bullet.height = 8.0;
                bullet.hit_sound = SoundId::EXPLOSION;
                bullet.trail_chance = 0.2;
                bullet.lifetime = 52.0;
            }
            BulletKind::LaserBolt => {
                // `LaserBoltBulletType` constructor.
                bullet.sprite = Some(String::from("bullet"));
                bullet.smoke_effect = EffectRef::Named(EffectId::HIT_LASER);
                bullet.hit_effect = EffectRef::Named(EffectId::HIT_LASER);
                bullet.despawn_effect = EffectRef::Named(EffectId::HIT_LASER);
                bullet.hittable = false;
                bullet.reflectable = false;
                bullet.laser_bullet = true;
                bullet.light_color = pal::HEAL;
                bullet.light_opacity = 0.6;
            }
            BulletKind::Sap => {
                // `SapBulletType()` constructor.
                bullet.speed = 0.0;
                bullet.despawn_effect = EffectRef::Named(EffectId::NONE);
                bullet.pierce = true;
                bullet.collides = false;
                bullet.hit_size = 0.0;
                bullet.hittable = false;
                bullet.hit_effect = EffectRef::Named(EffectId::HIT_LIQUID);
                // `status = StatusEffects.sapped` (resolved in `from_spec`).
                bullet.light_color = pal::SAP;
                bullet.light_opacity = 0.6;
                bullet.status_duration = 60.0 * 3.0;
                bullet.impact = true;
                bullet.length = 100.0;
                bullet.width = 0.4;
                bullet.sprite = Some(String::from("laser"));
            }
            BulletKind::Lightning => {
                // `LightningBulletType()` constructor.
                bullet.damage = 1.0;
                bullet.speed = 0.0;
                bullet.lifetime = 1.0;
                bullet.despawn_effect = EffectRef::Named(EffectId::NONE);
                bullet.hit_effect = EffectRef::Named(EffectId::HIT_LANCER);
                bullet.keep_velocity = false;
                bullet.hittable = false;
                // `status = StatusEffects.shocked` (resolved in `from_spec`).
                bullet.lightning_length = 25;
                bullet.lightning_color = pal::LANCER_LASER;
            }
            BulletKind::Laser => {
                // `LaserBulletType(damage)` constructor.
                bullet.speed = 0.0;
                bullet.hit_effect = EffectRef::Named(EffectId::HIT_LASER_BLAST);
                bullet.hit_color = Rgba::WHITE;
                bullet.despawn_effect = EffectRef::Named(EffectId::NONE);
                bullet.shoot_effect = EffectRef::Named(EffectId::HIT_LANCER);
                bullet.smoke_effect = EffectRef::Named(EffectId::NONE);
                bullet.hit_size = 4.0;
                bullet.lifetime = 16.0;
                bullet.impact = true;
                bullet.keep_velocity = false;
                bullet.collides = false;
                bullet.pierce = true;
                bullet.hittable = false;
                bullet.absorbable = false;
                bullet.remove_after_pierce = false;
                bullet.delay_frags = true;
                // `colors = {Pal.lancerLaser.cpy().mul(1,1,1,0.4), Pal.lancerLaser, Color.white}`.
                bullet.colors = vec![
                    pal::LANCER_LASER.with_alpha(0.4),
                    pal::LANCER_LASER,
                    Rgba::WHITE,
                ];
                bullet.length = 160.0;
                bullet.width = 15.0;
            }
            BulletKind::Flak => {
                // `FlakBulletType(speed, damage)` constructor.
                bullet.sprite = Some(String::from("shell"));
                bullet.splash_damage = 15.0;
                bullet.splash_damage_radius = 34.0;
                bullet.hit_effect = EffectRef::Named(EffectId::FLAK_EXPLOSION_BIG);
                bullet.width = 8.0;
                bullet.height = 10.0;
                bullet.collides_ground = false;
            }
            BulletKind::Explosion => {}
            BulletKind::Rail => {
                // `RailBulletType()` constructor.
                bullet.speed = 0.0;
                bullet.pierce_building = true;
                bullet.pierce = true;
                bullet.reflectable = false;
                bullet.hit_effect = EffectRef::Named(EffectId::NONE);
                bullet.despawn_effect = EffectRef::Named(EffectId::NONE);
                bullet.collides = false;
                bullet.keep_velocity = false;
                bullet.lifetime = 1.0;
                bullet.delay_frags = true;
                bullet.length = 100.0;
            }
            BulletKind::ContinuousLaser => {
                // `ContinuousBulletType`/`ContinuousLaserBulletType` initializers.
                bullet.length = 220.0;
                bullet.colors = vec![
                    Rgba::from_rgba8888(0xec745855),
                    Rgba::from_rgba8888(0xec7458aa),
                    Rgba::from_rgba8888(0xff9c5aff),
                    Rgba::WHITE,
                ];
                bullet.width = 9.0;
            }
            BulletKind::Shrapnel => {
                // `ShrapnelBulletType()` constructor.
                bullet.speed = 0.0;
                bullet.hit_effect = EffectRef::Named(EffectId::HIT_LANCER);
                bullet.shoot_effect = EffectRef::Named(EffectId::LIGHTNING_SHOOT);
                bullet.smoke_effect = EffectRef::Named(EffectId::LIGHTNING_SHOOT);
                bullet.lifetime = 10.0;
                bullet.despawn_effect = EffectRef::Named(EffectId::NONE);
                bullet.keep_velocity = false;
                bullet.collides = false;
                bullet.pierce = true;
                bullet.hittable = false;
                bullet.absorbable = false;
                bullet.light_opacity = 0.6;
                bullet.length = 100.0;
                bullet.width = 20.0;
            }
            BulletKind::Liquid => {
                // `LiquidBulletType(liquid)` constructor (`super(3.5f, 0)`).
                bullet.speed = 3.5;
                bullet.damage = 0.0;
                bullet.ammo_multiplier = 1.0;
                bullet.lifetime = 34.0;
                bullet.status_duration = 60.0 * 2.0;
                bullet.despawn_effect = EffectRef::Named(EffectId::NONE);
                bullet.hit_effect = EffectRef::Named(EffectId::HIT_LIQUID);
                bullet.smoke_effect = EffectRef::Named(EffectId::NONE);
                bullet.shoot_effect = EffectRef::Named(EffectId::NONE);
                bullet.drag = 0.001;
                bullet.knockback = 0.55;
                bullet.display_ammo_multiplier = false;
            }
            BulletKind::Emp => {
                // `EmpBulletType` field initializers (no explicit ctor).
                bullet.sprite = Some(String::from("bullet"));
            }
            BulletKind::Bomb => {
                // `BombBulletType(damage, radius, "shell")` (`super(0.7f, 0, sprite)`).
                bullet.sprite = Some(String::from("shell"));
                bullet.speed = 0.7;
                bullet.damage = 0.0;
                bullet.collides_tiles = false;
                bullet.collides = false;
                bullet.shrink_y = 0.7;
                bullet.lifetime = 30.0;
                bullet.drag = 0.05;
                bullet.keep_velocity = false;
                bullet.collides_air = false;
                bullet.hit_sound = SoundId::EXPLOSION;
            }
        }
        bullet
    }

    /// `BulletType.calculateRange()` (`BulletType.java:443-449`).
    ///
    /// Computed at registration from load-time constants (vanilla never
    /// mutates the inputs between load and `init`); `init_self` re-derives the
    /// same value into [`BulletDef::range`].
    pub fn compute_range(&self) -> f32 {
        if self.range_override > 0.0 {
            return self.range_override;
        }
        if let Some((lifetime, speed)) = self.spawn_unit_range {
            return lifetime * speed;
        }
        if let Some((lifetime, speed)) = self.despawn_unit_range {
            return lifetime * speed;
        }
        // `Mathf.zero(drag)`.
        if self.drag.abs() <= 0.001 {
            (self.speed * self.lifetime).max(self.max_range)
        } else {
            (self.speed * (1.0 - (1.0 - self.drag).powf(self.lifetime)) / self.drag)
                .max(self.max_range)
        }
    }

    /// `BulletType.heals()`.
    pub fn heals(&self) -> bool {
        self.heal_percent > 0.0 || self.heal_amount > 0.0
    }

    /// Resolves a generated [`BulletSpec`] (nested inline content is registered
    /// and patched by the unit load sink).
    pub fn from_spec(spec: &BulletSpec, registry: &ContentRegistry) -> Result<Self, ContentError> {
        let mut bullet = Self::new(spec.kind);
        macro_rules! apply {
            ($($field:ident),* $(,)?) => {
                $(if let Some(value) = &spec.$field {
                    bullet.$field = value.clone();
                })*
            };
        }
        apply!(
            lifetime,
            speed,
            damage,
            hit_size,
            draw_size,
            drag,
            accel,
            keep_velocity,
            scale_keep_velocity,
            scale_life,
            pierce,
            pierce_building,
            pierce_cap,
            pierce_damage_factor,
            remove_after_pierce,
            laser_absorb,
            laser_bullet,
            optimal_life_fract,
            reload_multiplier,
            ammo_multiplier,
            display_ammo_multiplier,
            recoil,
            kill_shooter,
            instant_disappear,
            splash_damage,
            splash_damage_radius,
            splash_damage_pierce,
            scaled_splash_damage,
            knockback,
            impact,
            building_damage_multiplier,
            shield_damage_multiplier,
            status_duration,
            status_chance,
            pierce_armor,
            armor_multiplier,
            block_armor_multiplier,
            hittable,
            reflectable,
            absorbable,
            collides,
            collides_air,
            collides_ground,
            collides_tiles,
            collides_team,
            collide_floor,
            collide_terrain,
            inaccuracy,
            hit_sound_volume,
            hit_sound_pitch,
            hit_sound_pitch_range,
            hit_shake,
            despawn_shake,
            despawn_hit,
            trail_length,
            trail_width,
            trail_chance,
            trail_interval,
            trail_param,
            trail_spread,
            trail_color,
            trail_rotation,
            trail_interp,
            frag_bullets,
            frag_on_hit,
            frag_on_despawn,
            frag_on_absorb,
            delay_frags,
            frag_random_spread,
            frag_spread,
            frag_angle,
            frag_velocity_min,
            frag_velocity_max,
            frag_life_min,
            frag_life_max,
            frag_offset_min,
            frag_offset_max,
            pierce_frag_cap,
            bullet_interval,
            interval_bullets,
            interval_random_spread,
            interval_spread,
            interval_angle,
            interval_delay,
            underwater,
            lightning,
            lightning_length,
            lightning_length_rand,
            lightning_damage,
            lightning_cone,
            lightning_angle,
            lightning_color,
            spawn_bullet_random_spread,
            puddles,
            puddle_range,
            puddle_amount,
            suppression_range,
            suppression_duration,
            suppression_effect_chance,
            suppress_color,
            sprite,
            back_sprite,
            width,
            height,
            back_color,
            front_color,
            mix_color_from,
            mix_color_to,
            shrink_x,
            shrink_y,
            shrink_interp,
            spin,
            rotation_offset,
            colors,
            length,
            damage_interval,
            large_hit,
            side_length,
            side_width,
            side_angle,
            lightning_spacing,
            lightning_delay,
            lightning_angle_rand,
            serrations,
            serration_len_scl,
            serration_width,
            serration_spacing,
            serration_space_offset,
            serration_fade_offset,
            from_color,
            to_color,
            sap_strength,
            beam_color,
            length_rand,
            emp_radius,
            time_increase,
            time_duration,
            power_damage_scl,
            power_scl_decrease,
            unit_damage_scl,
            hit_units,
            point_effect_space,
            max_range,
            range_override,
            homing_power,
            homing_range,
            homing_delay,
            follow_aim_speed,
            weave_scale,
            weave_mag,
            weave_random,
            hit_color,
            light_radius,
            light_opacity,
            light_color,
            make_fire,
            incend_amount,
            incend_spread,
            incend_chance,
            lifesteal,
            heal_amount,
            heal_percent,
            show_stats,
            set_defaults,
        );
        if let Some(effect) = &spec.hit_effect {
            bullet.hit_effect = effect.clone();
        }
        if let Some(effect) = &spec.despawn_effect {
            bullet.despawn_effect = effect.clone();
        }
        if let Some(effect) = &spec.shoot_effect {
            bullet.shoot_effect = effect.clone();
        }
        if let Some(effect) = &spec.smoke_effect {
            bullet.smoke_effect = effect.clone();
        }
        if let Some(effect) = &spec.charge_effect {
            bullet.charge_effect = effect.clone();
        }
        if let Some(effect) = &spec.trail_effect {
            bullet.trail_effect = effect.clone();
        }
        if let Some(effect) = &spec.hit_power_effect {
            bullet.hit_power_effect = effect.clone();
        }
        if let Some(effect) = &spec.chain_effect {
            bullet.chain_effect = effect.clone();
        }
        if let Some(effect) = &spec.apply_effect {
            bullet.apply_effect = effect.clone();
        }
        if let Some(effect) = &spec.pierce_effect {
            bullet.pierce_effect = effect.clone();
        }
        if let Some(effect) = &spec.point_effect {
            bullet.point_effect = effect.clone();
        }
        if let Some(effect) = &spec.line_effect {
            bullet.line_effect = effect.clone();
        }
        if let Some(effect) = &spec.end_effect {
            bullet.end_effect = effect.clone();
        }
        if let Some(sound) = spec.shoot_sound {
            bullet.shoot_sound = sound;
        }
        if let Some(sound) = spec.hit_sound {
            bullet.hit_sound = sound;
        }
        if let Some(sound) = spec.despawn_sound {
            bullet.despawn_sound = sound;
        }
        if let Some(name) = spec.status {
            bullet.status = registry
                .status_id(name)
                .ok_or_else(|| ContentError::UnknownName(name.to_owned()))?;
        }
        if let Some(name) = spec.puddle_liquid {
            bullet.puddle_liquid = registry
                .liquid_id(name)
                .ok_or_else(|| ContentError::UnknownName(name.to_owned()))?;
        }
        if let Some(name) = spec.liquid {
            // `LiquidBulletType(liquid)` constructor resolution.
            let liquid = registry
                .liquid_by_name(name)
                .ok_or_else(|| ContentError::UnknownName(name.to_owned()))?;
            bullet.status = liquid.effect;
            bullet.hit_color = liquid.color;
            bullet.light_color = liquid.light_color;
            bullet.light_opacity = liquid.color.a;
        }
        // `SapBulletType` / `LightningBulletType` constructor status defaults.
        if spec.kind == BulletKind::Sap && spec.status.is_none() {
            bullet.status = registry
                .status_id("sapped")
                .ok_or_else(|| ContentError::UnknownName(String::from("sapped")))?;
        }
        if spec.kind == BulletKind::Lightning && spec.status.is_none() {
            bullet.status = registry
                .status_id("shocked")
                .ok_or_else(|| ContentError::UnknownName(String::from("shocked")))?;
        }
        Ok(bullet)
    }
}

impl Content for BulletDef {
    const TYPE: ContentType = ContentType::Bullet;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = BulletId::new(id);
    }

    fn minfo(&self) -> &ModContentInfo {
        &self.minfo
    }

    fn minfo_mut(&mut self) -> &mut ModContentInfo {
        &mut self.minfo
    }

    fn removed(&self) -> bool {
        self.removed
    }

    fn set_removed(&mut self, removed: bool) {
        self.removed = removed;
    }

    fn kind_name(&self) -> &'static str {
        self.kind.name()
    }

    /// `BulletType.init()` self-contained derivations (`BulletType.java:397-411`).
    ///
    /// Cross-content pieces (the `shocked` lightning default, the
    /// `damageLightning` family default, frag `keepVelocity` resets) run in
    /// [`link`].
    fn init_self(&mut self) -> Result<(), ContentError> {
        if self.pierce_cap >= 1 {
            self.pierce = true;
        }
        if self.set_defaults
            && (self.frag_bullet.is_some() || self.splash_damage_radius > 0.0 || self.lightning > 0)
        {
            self.despawn_hit = true;
        }
        if self.light_radius <= -1.0 {
            self.light_radius = 18.0_f32.max(self.hit_size * 5.0);
        }
        self.draw_size = self
            .draw_size
            .max(self.trail_length as f32 * self.speed * 2.0);
        self.range = self.compute_range();
        Ok(())
    }
}

/// Post-`init()` bullet link pass (`BulletType.init()` cross-content pieces):
/// `shocked` default for lightning bullets, the `damageLightning` family
/// default for `lightningType`, and frag `keepVelocity`/`scaleKeepVelocity`
/// resets on the frag bullets.
pub(crate) fn link(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    let shocked = registry.status_id("shocked");
    let mut frag_resets: Vec<BulletId> = Vec::new();
    let mut lightning_defaults: Vec<(BulletId, BulletId)> = Vec::new();
    let mut status_defaults: Vec<BulletId> = Vec::new();
    for bullet in registry.bullets() {
        if bullet.set_defaults && bullet.lightning > 0 && bullet.status == StatusId::NONE {
            status_defaults.push(bullet.id);
        }
        if let Some(frag) = bullet.frag_bullet {
            frag_resets.push(frag);
        }
        if bullet.lightning_type.is_none() {
            let default = if !bullet.collides_air {
                DAMAGE_LIGHTNING_GROUND
            } else if !bullet.collides_ground {
                DAMAGE_LIGHTNING_AIR
            } else {
                DAMAGE_LIGHTNING
            };
            lightning_defaults.push((bullet.id, default));
        }
    }
    for id in status_defaults {
        if let Some(shocked) = shocked
            && let Some(bullet) = registry.bullet_mut(id)
        {
            bullet.status = shocked;
        }
    }
    for frag in frag_resets {
        if let Some(bullet) = registry.bullet_mut(frag) {
            bullet.keep_velocity = false;
            bullet.scale_keep_velocity = false;
        }
    }
    for (id, default) in lightning_defaults {
        if let Some(bullet) = registry.bullet_mut(id) {
            bullet.lightning_type = Some(default);
        }
    }
    Ok(())
}

/// Generated bullet input record (name/effect references resolved at load).
///
/// Fields default to the [`BulletDef::new`] class defaults; `None` means
/// "class default". Nested inline content is registered by the unit load sink
/// in upstream construction order.
#[derive(Debug, Clone, Default)]
pub struct BulletSpec {
    /// Bullet class tag.
    pub kind: BulletKind,
    /// See [`BulletDef`] fields of the same names.
    pub lifetime: Option<f32>,
    /// Speed.
    pub speed: Option<f32>,
    /// Damage.
    pub damage: Option<f32>,
    /// Hit size.
    pub hit_size: Option<f32>,
    /// Draw size.
    pub draw_size: Option<f32>,
    /// Drag.
    pub drag: Option<f32>,
    /// Accel.
    pub accel: Option<f32>,
    /// Keep velocity.
    pub keep_velocity: Option<bool>,
    /// Scale keep velocity.
    pub scale_keep_velocity: Option<bool>,
    /// Scale life.
    pub scale_life: Option<bool>,
    /// Pierce.
    pub pierce: Option<bool>,
    /// Pierce buildings.
    pub pierce_building: Option<bool>,
    /// Pierce cap.
    pub pierce_cap: Option<i32>,
    /// Pierce damage factor.
    pub pierce_damage_factor: Option<f32>,
    /// Remove after pierce.
    pub remove_after_pierce: Option<bool>,
    /// Laser absorb.
    pub laser_absorb: Option<bool>,
    /// Laser bullet.
    pub laser_bullet: Option<bool>,
    /// Optimal life fraction.
    pub optimal_life_fract: Option<f32>,
    /// Reload multiplier.
    pub reload_multiplier: Option<f32>,
    /// Ammo multiplier.
    pub ammo_multiplier: Option<f32>,
    /// Display ammo multiplier.
    pub display_ammo_multiplier: Option<bool>,
    /// Recoil.
    pub recoil: Option<f32>,
    /// Kill shooter.
    pub kill_shooter: Option<bool>,
    /// Instant disappear.
    pub instant_disappear: Option<bool>,
    /// Splash damage.
    pub splash_damage: Option<f32>,
    /// Splash radius.
    pub splash_damage_radius: Option<f32>,
    /// Splash pierce.
    pub splash_damage_pierce: Option<bool>,
    /// Scaled splash damage.
    pub scaled_splash_damage: Option<bool>,
    /// Knockback.
    pub knockback: Option<f32>,
    /// Impact.
    pub impact: Option<bool>,
    /// Building damage multiplier.
    pub building_damage_multiplier: Option<f32>,
    /// Shield damage multiplier.
    pub shield_damage_multiplier: Option<f32>,
    /// Status (name).
    pub status: Option<&'static str>,
    /// Status duration.
    pub status_duration: Option<f32>,
    /// Status chance.
    pub status_chance: Option<f32>,
    /// Pierce armor.
    pub pierce_armor: Option<bool>,
    /// Armor multiplier.
    pub armor_multiplier: Option<f32>,
    /// Block armor multiplier.
    pub block_armor_multiplier: Option<f32>,
    /// Hittable.
    pub hittable: Option<bool>,
    /// Reflectable.
    pub reflectable: Option<bool>,
    /// Absorbable.
    pub absorbable: Option<bool>,
    /// Collides.
    pub collides: Option<bool>,
    /// Collides air.
    pub collides_air: Option<bool>,
    /// Collides ground.
    pub collides_ground: Option<bool>,
    /// Collides tiles.
    pub collides_tiles: Option<bool>,
    /// Collides team.
    pub collides_team: Option<bool>,
    /// Collide floor.
    pub collide_floor: Option<bool>,
    /// Collide terrain.
    pub collide_terrain: Option<bool>,
    /// Inaccuracy.
    pub inaccuracy: Option<f32>,
    /// Hit effect.
    pub hit_effect: Option<EffectRef>,
    /// Despawn effect.
    pub despawn_effect: Option<EffectRef>,
    /// Shoot effect.
    pub shoot_effect: Option<EffectRef>,
    /// Smoke effect.
    pub smoke_effect: Option<EffectRef>,
    /// Charge effect.
    pub charge_effect: Option<EffectRef>,
    /// Trail effect.
    pub trail_effect: Option<EffectRef>,
    /// Shoot sound (`BulletType.shootSound`).
    pub shoot_sound: Option<SoundId>,
    /// Hit sound.
    pub hit_sound: Option<SoundId>,
    /// Despawn sound.
    pub despawn_sound: Option<SoundId>,
    /// Hit sound volume.
    pub hit_sound_volume: Option<f32>,
    /// Hit sound pitch.
    pub hit_sound_pitch: Option<f32>,
    /// Hit sound pitch range.
    pub hit_sound_pitch_range: Option<f32>,
    /// Hit shake.
    pub hit_shake: Option<f32>,
    /// Despawn shake.
    pub despawn_shake: Option<f32>,
    /// Despawn hit.
    pub despawn_hit: Option<bool>,
    /// Trail length.
    pub trail_length: Option<i32>,
    /// Trail width.
    pub trail_width: Option<f32>,
    /// Trail chance.
    pub trail_chance: Option<f32>,
    /// Trail interval.
    pub trail_interval: Option<f32>,
    /// Trail param.
    pub trail_param: Option<f32>,
    /// Trail spread.
    pub trail_spread: Option<f32>,
    /// Trail color.
    pub trail_color: Option<Rgba>,
    /// Trail rotation.
    pub trail_rotation: Option<bool>,
    /// Trail interp.
    pub trail_interp: Option<InterpKind>,
    /// Frag bullet (inline construction).
    pub frag_bullet: Option<Box<BulletSpec>>,
    /// Frag bullets count.
    pub frag_bullets: Option<i32>,
    /// Frag on hit.
    pub frag_on_hit: Option<bool>,
    /// Frag on despawn.
    pub frag_on_despawn: Option<bool>,
    /// Frag on absorb.
    pub frag_on_absorb: Option<bool>,
    /// Delay frags.
    pub delay_frags: Option<bool>,
    /// Frag random spread.
    pub frag_random_spread: Option<f32>,
    /// Frag spread.
    pub frag_spread: Option<f32>,
    /// Frag angle.
    pub frag_angle: Option<f32>,
    /// Frag velocity min.
    pub frag_velocity_min: Option<f32>,
    /// Frag velocity max.
    pub frag_velocity_max: Option<f32>,
    /// Frag life min.
    pub frag_life_min: Option<f32>,
    /// Frag life max.
    pub frag_life_max: Option<f32>,
    /// Frag offset min.
    pub frag_offset_min: Option<f32>,
    /// Frag offset max.
    pub frag_offset_max: Option<f32>,
    /// Pierce frag cap.
    pub pierce_frag_cap: Option<i32>,
    /// Interval bullet (inline construction).
    pub interval_bullet: Option<Box<BulletSpec>>,
    /// Bullet interval.
    pub bullet_interval: Option<f32>,
    /// Interval bullets.
    pub interval_bullets: Option<i32>,
    /// Interval random spread.
    pub interval_random_spread: Option<f32>,
    /// Interval spread.
    pub interval_spread: Option<f32>,
    /// Interval angle.
    pub interval_angle: Option<f32>,
    /// Interval delay.
    pub interval_delay: Option<f32>,
    /// Underwater.
    pub underwater: Option<bool>,
    /// Lightning count.
    pub lightning: Option<i32>,
    /// Lightning length.
    pub lightning_length: Option<i32>,
    /// Lightning length rand.
    pub lightning_length_rand: Option<i32>,
    /// Lightning damage.
    pub lightning_damage: Option<f32>,
    /// Lightning cone.
    pub lightning_cone: Option<f32>,
    /// Lightning angle.
    pub lightning_angle: Option<f32>,
    /// Lightning type (inline construction).
    pub lightning_type: Option<Box<BulletSpec>>,
    /// Lightning color.
    pub lightning_color: Option<Rgba>,
    /// Spawn bullets (inline constructions).
    pub spawn_bullets: Vec<BulletSpec>,
    /// Spawn bullet random spread.
    pub spawn_bullet_random_spread: Option<f32>,
    /// Puddles.
    pub puddles: Option<i32>,
    /// Puddle range.
    pub puddle_range: Option<f32>,
    /// Puddle amount.
    pub puddle_amount: Option<f32>,
    /// Puddle liquid (name).
    pub puddle_liquid: Option<&'static str>,
    /// Spawn unit (inline `MissileUnitType` construction).
    pub spawn_unit: Option<Box<UnitSpec>>,
    /// Suppression range.
    pub suppression_range: Option<f32>,
    /// Suppression duration.
    pub suppression_duration: Option<f32>,
    /// Suppression effect chance.
    pub suppression_effect_chance: Option<f32>,
    /// Suppress color.
    pub suppress_color: Option<Rgba>,
    /// Sprite.
    pub sprite: Option<Option<String>>,
    /// Back sprite.
    pub back_sprite: Option<Option<String>>,
    /// Width.
    pub width: Option<f32>,
    /// Height.
    pub height: Option<f32>,
    /// Back color.
    pub back_color: Option<Rgba>,
    /// Front color.
    pub front_color: Option<Rgba>,
    /// Mix color from.
    pub mix_color_from: Option<Rgba>,
    /// Mix color to.
    pub mix_color_to: Option<Rgba>,
    /// Shrink X.
    pub shrink_x: Option<f32>,
    /// Shrink Y.
    pub shrink_y: Option<f32>,
    /// Shrink interp.
    pub shrink_interp: Option<InterpKind>,
    /// Spin.
    pub spin: Option<f32>,
    /// Rotation offset.
    pub rotation_offset: Option<f32>,
    /// Laser colors.
    pub colors: Option<Vec<Rgba>>,
    /// Beam length.
    pub length: Option<f32>,
    /// Damage interval.
    pub damage_interval: Option<f32>,
    /// Shake (`ContinuousBulletType.shake`).
    pub shake: Option<f32>,
    /// Large hit.
    pub large_hit: Option<bool>,
    /// Side length.
    pub side_length: Option<f32>,
    /// Side width.
    pub side_width: Option<f32>,
    /// Side angle.
    pub side_angle: Option<f32>,
    /// Lightning spacing.
    pub lightning_spacing: Option<f32>,
    /// Lightning delay.
    pub lightning_delay: Option<f32>,
    /// Lightning angle rand.
    pub lightning_angle_rand: Option<f32>,
    /// Serrations.
    pub serrations: Option<i32>,
    /// Serration length scale.
    pub serration_len_scl: Option<f32>,
    /// Serration width.
    pub serration_width: Option<f32>,
    /// Serration spacing.
    pub serration_spacing: Option<f32>,
    /// Serration space offset.
    pub serration_space_offset: Option<f32>,
    /// Serration fade offset.
    pub serration_fade_offset: Option<f32>,
    /// From color.
    pub from_color: Option<Rgba>,
    /// To color.
    pub to_color: Option<Rgba>,
    /// Sap strength.
    pub sap_strength: Option<f32>,
    /// Beam (sap) color.
    pub beam_color: Option<Rgba>,
    /// Length rand.
    pub length_rand: Option<f32>,
    /// EMP radius.
    pub emp_radius: Option<f32>,
    /// Time increase.
    pub time_increase: Option<f32>,
    /// Time duration.
    pub time_duration: Option<f32>,
    /// Power damage scale.
    pub power_damage_scl: Option<f32>,
    /// Power scale decrease.
    pub power_scl_decrease: Option<f32>,
    /// Unit damage scale.
    pub unit_damage_scl: Option<f32>,
    /// Hit power effect.
    pub hit_power_effect: Option<EffectRef>,
    /// Chain effect.
    pub chain_effect: Option<EffectRef>,
    /// Apply effect.
    pub apply_effect: Option<EffectRef>,
    /// Hit units.
    pub hit_units: Option<bool>,
    /// Pierce effect.
    pub pierce_effect: Option<EffectRef>,
    /// Point effect.
    pub point_effect: Option<EffectRef>,
    /// Line effect.
    pub line_effect: Option<EffectRef>,
    /// End effect.
    pub end_effect: Option<EffectRef>,
    /// Point effect space.
    pub point_effect_space: Option<f32>,
    /// Draw layer.
    pub layer: Option<f32>,
    /// Trail multiplier (`ArtilleryBulletType.trailMult`).
    pub trail_mult: Option<f32>,
    /// Trail size (`ArtilleryBulletType.trailSize`).
    pub trail_size: Option<f32>,
    /// Explode range (`FlakBulletType.explodeRange`).
    pub explode_range: Option<f32>,
    /// Explode delay (`FlakBulletType.explodeDelay`).
    pub explode_delay: Option<f32>,
    /// Flak delay (`FlakBulletType.flakDelay`).
    pub flak_delay: Option<f32>,
    /// Flak interval (`FlakBulletType.flakInterval`).
    pub flak_interval: Option<f32>,
    /// Max range.
    pub max_range: Option<f32>,
    /// Range override.
    pub range_override: Option<f32>,
    /// Homing power.
    pub homing_power: Option<f32>,
    /// Homing range.
    pub homing_range: Option<f32>,
    /// Homing delay.
    pub homing_delay: Option<f32>,
    /// Follow aim speed.
    pub follow_aim_speed: Option<f32>,
    /// Weave scale.
    pub weave_scale: Option<f32>,
    /// Weave mag.
    pub weave_mag: Option<f32>,
    /// Weave random.
    pub weave_random: Option<bool>,
    /// Hit color.
    pub hit_color: Option<Rgba>,
    /// Light radius.
    pub light_radius: Option<f32>,
    /// Light opacity.
    pub light_opacity: Option<f32>,
    /// Light color.
    pub light_color: Option<Rgba>,
    /// Make fire.
    pub make_fire: Option<bool>,
    /// Incend amount.
    pub incend_amount: Option<i32>,
    /// Incend spread.
    pub incend_spread: Option<f32>,
    /// Incend chance.
    pub incend_chance: Option<f32>,
    /// Lifesteal.
    pub lifesteal: Option<f32>,
    /// Heal amount.
    pub heal_amount: Option<f32>,
    /// Heal percent.
    pub heal_percent: Option<f32>,
    /// Show stats.
    pub show_stats: Option<bool>,
    /// Set defaults.
    pub set_defaults: Option<bool>,
    /// `LiquidBulletType` liquid (name; constructor resolution).
    pub liquid: Option<&'static str>,
}

/// Loads the 6 shared internal bullets in `Bullets.load()` order.
pub fn load(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    // Status effects load before bullets (upstream order).
    let shocked = registry
        .status_id("shocked")
        .ok_or_else(|| ContentError::UnknownName(String::from("shocked")))?;

    // Not allowed in weapons - used only to prevent NullPointerExceptions.
    registry.add_bullet({
        let mut bullet = BulletDef::new(BulletKind::Basic);
        bullet.speed = 2.5;
        bullet.damage = 9.0;
        bullet.sprite = Some(String::from("ohno"));
        bullet.width = 7.0;
        bullet.height = 9.0;
        bullet.lifetime = 60.0;
        bullet.ammo_multiplier = 2.0;
        bullet
    })?;

    // Lightning bullets need to be initialized first.
    let damage_lightning = registry.add_bullet({
        let mut bullet = BulletDef::new(BulletKind::Plain);
        bullet.speed = 0.0001;
        bullet.damage = 0.0;
        bullet.lifetime = EffectId::LIGHTNING.meta().lifetime;
        bullet.hit_effect = EffectRef::Named(EffectId::HIT_LANCER);
        bullet.despawn_effect = EffectRef::Named(EffectId::NONE);
        bullet.status = shocked;
        bullet.status_duration = 10.0;
        bullet.hittable = false;
        bullet.light_color = Rgba::WHITE;
        bullet
    })?;

    // Copy that does not damage air units.
    let base = registry
        .bullet(damage_lightning)
        .ok_or(ContentError::UnknownId(damage_lightning.raw()))?
        .clone();
    let mut ground = base.clone();
    ground.collides_air = false;
    registry.add_bullet(ground)?;

    // Copy that does not damage ground units or tiles.
    let mut air = base;
    air.collides_ground = false;
    air.collides_tiles = false;
    registry.add_bullet(air)?;

    registry.add_bullet({
        let mut bullet = BulletDef::new(BulletKind::Fire);
        bullet.speed = 1.0;
        bullet.damage = 4.0;
        bullet.hittable = false;
        bullet
    })?;

    registry.add_bullet({
        let mut bullet = BulletDef::new(BulletKind::SpaceLiquid);
        bullet.knockback = 0.7;
        bullet.drag = 0.01;
        bullet
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;
    use super::*;

    /// `bullets::damage_lightning_copy_flags` (plan 02 §5 M1) plus the internal
    /// bullet ids/values.
    #[test]
    fn damage_lightning_copy_flags() {
        let registry = test_registry();
        assert!(registry.bullets().len() >= 6, "internal bullets present");

        let placeholder = registry.bullet(PLACEHOLDER).unwrap();
        assert_eq!(placeholder.speed, 2.5);
        assert_eq!(placeholder.damage, 9.0);
        assert_eq!(placeholder.sprite.as_deref(), Some("ohno"));
        assert_eq!((placeholder.width, placeholder.height), (7.0, 9.0));
        assert_eq!(placeholder.lifetime, 60.0);
        assert_eq!(placeholder.ammo_multiplier, 2.0);

        let lightning = registry.bullet(DAMAGE_LIGHTNING).unwrap();
        assert_eq!(lightning.lifetime, EffectId::LIGHTNING.meta().lifetime);
        assert!(matches!(
            lightning.hit_effect,
            EffectRef::Named(EffectId::HIT_LANCER)
        ));
        assert!(matches!(
            lightning.despawn_effect,
            EffectRef::Named(EffectId::NONE)
        ));
        assert_eq!(lightning.status, registry.status_id("shocked").unwrap());
        assert_eq!(lightning.status_duration, 10.0);
        assert!(!lightning.hittable);

        let ground = registry.bullet(DAMAGE_LIGHTNING_GROUND).unwrap();
        assert!(!ground.collides_air);
        assert!(ground.collides_ground);
        assert!(ground.collides_tiles);
        assert_eq!(ground.lifetime, lightning.lifetime);

        let air = registry.bullet(DAMAGE_LIGHTNING_AIR).unwrap();
        assert!(air.collides_air);
        assert!(!air.collides_ground);
        assert!(!air.collides_tiles);

        let fireball = registry.bullet(BulletId::new(4)).unwrap();
        assert_eq!(fireball.speed, 1.0);
        assert_eq!(fireball.damage, 4.0);
        assert!(!fireball.hittable);
        assert!(fireball.pierce);
        assert!(!fireball.collides);
        assert_eq!(fireball.drag, 0.03);

        let space = registry.bullet(BulletId::new(5)).unwrap();
        assert_eq!(space.knockback, 0.7);
        assert_eq!(space.drag, 0.01);
        assert_eq!(space.speed, 3.5);
        assert_eq!(space.lifetime, 90.0);
        assert!(!space.hittable);
    }

    /// `BulletType.calculateRange()` derivations (drag / override / max range).
    #[test]
    fn calculate_range() {
        let mut bullet = BulletDef::new(BulletKind::Basic);
        bullet.speed = 2.5;
        bullet.lifetime = 60.0;
        assert_eq!(bullet.compute_range(), 150.0);

        // `speed * (1 - (1-drag)^lifetime) / drag`.
        bullet.drag = 0.01;
        let expected = 2.5 * (1.0 - (1.0_f32 - 0.01).powf(60.0)) / 0.01;
        assert!((bullet.compute_range() - expected).abs() < 0.001);

        bullet.range_override = 42.0;
        assert_eq!(bullet.compute_range(), 42.0);

        bullet.range_override = -1.0;
        bullet.max_range = 500.0;
        assert_eq!(bullet.compute_range(), 500.0);

        bullet.spawn_unit_range = Some((102.0, 4.0));
        assert_eq!(bullet.compute_range(), 102.0 * 4.0);
    }
}

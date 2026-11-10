// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret ammo/config/state types (`Turret`, `ItemTurret`, `LiquidTurret`, `PowerTurret`).
//! Upstream: core/src/mindustry/world/blocks/defense/turrets/{BaseTurret,Turret,ItemTurret,LiquidTurret,PowerTurret}.java

use std::sync::Arc;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use smallvec::SmallVec;

use crate::combat::targeting::UnitSort;
use crate::content::registries::sound_meta::SoundId;
use crate::content::registries::units::weapon::ShootPatternSpec;
use crate::content::{BulletId, ContentRegistry, ItemId, LiquidId};
use crate::weapons::pattern::PatternShot;
use crate::world::PayloadRef;
use crate::world::blocks::payloads::PayloadKind;
use crate::world::plan::BuildPlan;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemAmmo {
    /// Accepted item.
    pub item: ItemId,
    /// Bullet fired.
    pub bullet: BulletId,
    /// Ammo units added per item (`BulletType.ammoMultiplier`).
    pub ammo_multiplier: i32,
    /// `BulletType.rangeChange` of the fired bullet (`Turret.range()`,
    /// `Turret.java:348-353`).
    pub range_change: f32,
    /// `BulletType.minRangeChange` of the fired bullet
    /// (`Turret.TurretBuild.minRange()`, `Turret.java:340-344`).
    pub min_range_change: f32,
}

/// One resolved liquid-ammo entry (`LiquidTurret.ammoTypes`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiquidAmmo {
    /// Accepted liquid.
    pub liquid: LiquidId,
    /// Bullet fired.
    pub bullet: BulletId,
    /// `BulletType.ammoMultiplier` (liquid units per shot = `1/multiplier`).
    pub ammo_multiplier: f32,
    /// `BulletType.rangeChange` of the fired bullet.
    pub range_change: f32,
    /// `BulletType.minRangeChange` of the fired bullet.
    pub min_range_change: f32,
}

/// One payload-ammo entry (`PayloadAmmoTurret.ammoTypes`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PayloadAmmo {
    /// Accepted payload content (block or unit).
    pub content: PayloadContent,
    /// Bullet fired when this payload is consumed.
    pub bullet: BulletId,
    /// `BulletType.rangeChange` of the fired bullet.
    pub range_change: f32,
    /// `BulletType.minRangeChange` of the fired bullet.
    pub min_range_change: f32,
}

/// Payload content key (block or unit) for `PayloadAmmoTurret`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadContent {
    /// Whether the content is a building or a unit (`Payload` type tag).
    pub kind: PayloadKind,
    /// Raw block/unit content id.
    pub id: u16,
}

impl PayloadContent {
    /// Extracts the content key from an incoming payload handle.
    pub fn from_payload(payload: PayloadRef) -> Self {
        Self {
            kind: if payload.is_block {
                PayloadKind::Build
            } else {
                PayloadKind::Unit
            },
            id: payload.content,
        }
    }
}

/// One per-instance payload-ammo stack (`PayloadSeq` entry).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PayloadStack {
    /// Stored content.
    pub content: PayloadContent,
    /// Number of stored payloads of this content.
    pub count: i32,
}

/// Ammo family (`ItemTurret`/`LiquidTurret`/`PowerTurret`/`PayloadAmmoTurret`).
#[derive(Debug, Clone, PartialEq)]
pub enum TurretAmmo {
    /// `ItemTurret.ammoTypes`.
    Item(Vec<ItemAmmo>),
    /// `LiquidTurret.ammoTypes`.
    Liquid(Vec<LiquidAmmo>),
    /// `PowerTurret.shootType` with its bullet def's range knobs snapshotted
    /// at config time (J04 T6.2: `peekAmmo().rangeChange`/`minRangeChange`
    /// read the fired bullet, `Turret.java:348-353`,`:340-344`).
    Power(PowerAmmo),
    /// `PayloadAmmoTurret.ammoTypes` (content → bullet).
    Payload(Vec<PayloadAmmo>),
}

/// One resolved power-ammo entry (`PowerTurret.shootType`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerAmmo {
    /// Bullet fired.
    pub bullet: BulletId,
    /// `BulletType.rangeChange` of the fired bullet.
    pub range_change: f32,
    /// `BulletType.minRangeChange` of the fired bullet.
    pub min_range_change: f32,
}

/// Turret behavior class (`BaseTurret` subclass dispatch, M5/M6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurretKind {
    /// `ItemTurret` (`Turret` core + item ammo stacks).
    #[default]
    Item,
    /// `LiquidTurret` (`Turret` core + liquid ammo).
    Liquid,
    /// `PowerTurret` (`Turret` core + power gate).
    Power,
    /// `ContinuousTurret`/`ContinuousLiquidTurret` (always-firing keep-alive beam).
    Continuous,
    /// `LaserTurret` (continuous beam gated by a coolant-fueled reload).
    Laser,
    /// `PointDefenseTurret` (intercepts enemy bullets).
    PointDefense,
    /// `TractorBeamTurret` (pulls/status/damages a single unit).
    TractorBeam,
    /// `PayloadAmmoTurret` (fires accepted payloads; plan 08 hook).
    PayloadAmmo,
    /// `BuildTurret` (proxy build plan; plan 11/15 hook).
    Build,
}

/// Resolved turret knobs (`BaseTurret` + `ReloadTurret` + `Turret` fields).
#[derive(Debug, Clone, PartialEq)]
pub struct TurretConfig {
    /// Ammo family.
    pub ammo: TurretAmmo,
    /// `ContinuousLiquidTurret.liquidConsumed` (per-tick fuel drain; T14).
    pub liquid_consumed: f32,
    /// `BaseTurret.range`.
    pub range: f32,
    /// `ReloadTurret.reload` (ticks between shots).
    pub reload: f32,
    /// `Turret.maxAmmo`.
    pub max_ammo: i32,
    /// `Turret.ammoPerShot`.
    pub ammo_per_shot: i32,
    /// `Turret.consumeAmmoOnce`.
    pub consume_ammo_once: bool,
    /// `BaseTurret.rotateSpeed`.
    pub rotate_speed: f32,
    /// `Turret.shootCone`.
    pub shoot_cone: f32,
    /// `Turret.minWarmup`.
    pub min_warmup: f32,
    /// `Turret.shootWarmupSpeed`.
    pub shoot_warmup_speed: f32,
    /// `Turret.linearWarmup`.
    pub linear_warmup: bool,
    /// `Turret.warmupMaintainTime`.
    pub warmup_maintain_time: f32,
    /// `Turret.targetInterval`.
    pub target_interval: f32,
    /// `Turret.newTargetInterval` (`Turret.java:44`): the retarget interval used
    /// while a target is held; `<= 0` falls back to `target_interval`
    /// (`Turret.init`, `:216`).
    pub new_target_interval: f32,
    /// `Turret.unitSort` (`Turret.java:113`, default `UnitSorts.closest`).
    /// Turret overrides: ripple `grouped`, foreshadow/lustre/scathe/malign
    /// `strongest`.
    pub unit_sort: UnitSort,
    /// `Turret.inaccuracy` (degrees).
    pub inaccuracy: f32,
    /// `Turret.velocityRnd`.
    pub velocity_rnd: f32,
    /// `Turret.extraVelocity`.
    pub extra_velocity: f32,
    /// `Turret.lifeRnd`.
    pub life_rnd: f32,
    /// `Turret.extraLife`.
    pub extra_life: f32,
    /// `Turret.scaleLifetimeOffset`.
    pub scale_lifetime_offset: f32,
    /// `Turret.minRange` (`Turret.java:80`): the minimum bullet range (the
    /// `scaleLife` lifetime lower clamp, `Turret.java:797`).
    pub min_range: f32,
    /// `Turret.trackingRange` (`Turret.java:78`): post-`init` this is
    /// `max(range, trackingRange)` (`Turret.java:223`); ammo `rangeChange`
    /// shifts it at use time (`trackingRange()`, `:355-356`).
    pub tracking_range: f32,
    /// `Turret.recoil`.
    pub recoil: f32,
    /// `Turret.recoils`.
    pub recoils: i32,
    /// `Turret.recoilTime` (`<0` = reload).
    pub recoil_time: f32,
    /// `Turret.recoilPow`.
    pub recoil_pow: f32,
    /// `Turret.cooldownTime`.
    pub cooldown_time: f32,
    /// `Turret.shootX`.
    pub shoot_x: f32,
    /// `Turret.shootY`.
    pub shoot_y: f32,
    /// `Turret.xRand`.
    pub x_rand: f32,
    /// `Turret.shoot` pattern (per-bullet override wins at shoot time).
    pub shoot: ShootPatternSpec,
    /// `Turret.targetAir`.
    pub target_air: bool,
    /// `Turret.targetGround`.
    pub target_ground: bool,
    /// `Turret.targetBlocks`.
    pub target_blocks: bool,
    /// `Turret.targetUnderBlocks` (`Turret.java:107`; `scathe` sets false,
    /// `Blocks.java:5668`).
    pub target_under_blocks: bool,
    /// `Turret.targetHealing` (`Turret.java:101`): damaged allies are healed
    /// when no enemy is found and the ammo both heals and collides with the
    /// turret's team.
    pub target_healing: bool,
    /// `Turret.alwaysShooting`.
    pub always_shooting: bool,
    /// `Turret.predictTarget` (`Turret.java:111`; set false only by `scathe`,
    /// `Blocks.java:5248`).
    pub predict_target: bool,
    /// `Turret.accurateDelay` (`Turret.java:84`; `lancer` sets false,
    /// `Blocks.java:3594`).
    pub accurate_delay: bool,
    /// `Turret.moveWhileCharging` (`Turret.java:86`; `lancer` sets false,
    /// `Blocks.java:3595`).
    pub move_while_charging: bool,
    /// `BaseTurret.activationTime`.
    pub activation_time: f32,
    /// `BaseTurret.coolantMultiplier`.
    pub coolant_multiplier: f32,
    /// Coolant units consumed per tick (`ConsumeCoolant.amount`); `0` = no coolant.
    pub coolant_amount: f32,
    /// Liquids the coolant consumer accepts, in content order
    /// (`ConsumeCoolant` filter: `coolant && !gas && temperature <= 0.5 &&
    /// flammability < 0.1`, or the single `ConsumeLiquid` liquid).
    pub coolant_liquids: Vec<LiquidId>,
    /// `Turret.heatRequirement` (`<0` = none).
    pub heat_requirement: f32,
    /// `Turret.maxHeatEfficiency`.
    pub max_heat_efficiency: f32,
    /// Behavior class (M5/M6 dispatch).
    pub kind: TurretKind,
    /// `PointDefenseTurret.retargetTime` / `TractorBeamTurret.retargetTime`.
    pub retarget_time: f32,
    /// `PointDefenseTurret.shootLength` / `TractorBeamTurret.shootLength`.
    pub shoot_length: f32,
    /// `ContinuousTurret.aimChangeSpeed`.
    pub aim_change_speed: f32,
    /// `ContinuousTurret.scaleDamageEfficiency`.
    pub scale_damage_efficiency: bool,
    /// `PointDefenseTurret.bulletDamage`.
    pub bullet_damage: f32,
    /// `TractorBeamTurret.force`.
    pub force: f32,
    /// `TractorBeamTurret.scaledForce`.
    pub scaled_force: f32,
    /// `TractorBeamTurret.damage` (per tick).
    pub beam_damage: f32,
    /// `TractorBeamTurret.status`.
    pub status: crate::content::StatusId,
    /// `TractorBeamTurret.statusDuration`.
    pub status_duration: f32,
    /// `TractorBeamTurret.statusChance`.
    pub status_chance: f32,
    /// `LaserTurret.shootDuration` (beam lifetime after firing).
    pub shoot_duration: f32,
    /// `LaserTurret.firingMoveFract` (rotation speed scale while firing).
    pub firing_move_fract: f32,
    /// `BuildTurret.buildSpeed` (construction units per tick).
    pub build_speed: f32,
    /// `Turret.shootSound` (default `Sounds.shootDuo`, `Turret.java:127-133`).
    pub shoot_sound: SoundId,
    /// `Turret.shootSoundVolume` (default 1).
    pub shoot_sound_volume: f32,
    /// `Turret.chargeSound` (default `Sounds.none`).
    pub charge_sound: SoundId,
    /// `Turret.loopSound` (default `Sounds.none`).
    pub loop_sound: SoundId,
    /// `Turret.loopSoundVolume` (default 0.5).
    pub loop_sound_volume: f32,
    /// `Turret.soundPitchMin` (default 0.9).
    pub sound_pitch_min: f32,
    /// `Turret.soundPitchMax` (default 1.1).
    pub sound_pitch_max: f32,
    /// `Turret.fogRadiusMultiplier` (default 1; scathe 0.4,
    /// `Blocks.java:5661`).
    pub fog_radius_multiplier: f32,
}

impl TurretConfig {
    /// Whether this turret consumes a coolant liquid.
    pub fn uses_coolant(&self) -> bool {
        self.coolant_amount > 0.0
    }

    /// `Turret.ammoReloadMultiplier` from the current ammo entry.
    pub fn ammo_reload_multiplier(&self, ammo: Option<BulletId>, content: &ContentRegistry) -> f32 {
        ammo.and_then(|bullet| content.bullet(bullet))
            .map(|def| def.reload_multiplier)
            .unwrap_or(1.0)
    }
}

/// Persistent per-turret state (`Turret.TurretBuild`).
#[derive(Debug, Clone, Component)]
pub struct TurretState {
    /// Resolved knobs (shared with the harness).
    pub config: Arc<TurretConfig>,
    /// `BaseTurretBuild.rotation` (degrees).
    pub rotation: f32,
    /// `TurretBuild.reloadCounter`.
    pub reload_counter: f32,
    /// `TurretBuild.curRecoil`.
    pub cur_recoil: f32,
    /// `TurretBuild.curRecoils`.
    pub cur_recoils: SmallVec<[f32; 4]>,
    /// `TurretBuild.heat`.
    pub heat: f32,
    /// `TurretBuild.heatReq`.
    pub heat_req: f32,
    /// `TurretBuild.shootWarmup`.
    pub shoot_warmup: f32,
    /// `TurretBuild.charge`.
    pub charge: f32,
    /// `TurretBuild.warmupHold`.
    pub warmup_hold: f32,
    /// `TurretBuild.totalShots`.
    pub total_shots: i32,
    /// `TurretBuild.barrelCounter`.
    pub barrel_counter: i32,
    /// Shots queued by a pattern `delay` (`Turret.java:763-778`
    /// `queuedBullets`; runtime-only, never saved — upstream `Time.run` tasks
    /// are not written either).
    pub pending: SmallVec<[PendingShot; 4]>,
    /// `TurretBuild.ammo` (item entries; empty for liquid/power).
    pub ammo: SmallVec<[AmmoEntry; 4]>,
    /// `TurretBuild.totalAmmo`.
    pub total_ammo: i32,
    /// `TurretBuild.target`.
    pub target: Option<Entity>,
    /// `TurretBuild.targetPos`.
    pub target_pos: (f32, f32),
    /// `TurretBuild.target` timer.
    pub target_timer: f32,
    /// `TurretBuild.wasShooting`.
    pub was_shooting: bool,
    /// `TurretBuild.isShooting`.
    pub is_shooting: bool,
    /// `BaseTurretBuild.activationTimer`.
    pub activation_timer: f32,
    /// `ContinuousLiquidTurretBuild.activated` (T14): the four-tick fuel latch.
    pub activated: bool,
    /// `TurretBuild.logicControlTime`.
    pub logic_control_time: f32,
    /// `TurretBuild.logicShooting`.
    pub logic_shooting: bool,
    /// `ContinuousTurretBuild.bullets` / `LaserTurretBuild.bullets`: live beam
    /// bullets owned by this turret, with their per-entry offsets/life.
    pub bullets: SmallVec<[BeamEntry; 2]>,
    /// `ContinuousTurretBuild.lastLength`.
    pub last_length: f32,
    /// Retarget timer for `PointDefenseTurret`/`TractorBeamTurret`.
    pub retarget_timer: f32,
    /// `TractorBeamBuild.strength` (beam fade).
    pub strength: f32,
    /// `TractorBeamBuild.target` (unit being pulled).
    pub unit_target: Option<Entity>,
    /// `PointDefenseBuild.target` (enemy bullet being intercepted).
    pub bullet_target: Option<Entity>,
    /// `TractorBeamBuild.any` (beam is active this tick).
    pub any: bool,
    /// `PayloadTurretBuild.payloads` (payload-ammo stacks by content).
    pub payloads: SmallVec<[PayloadStack; 4]>,
    /// `BuildTurretBuild.warmup`.
    pub build_warmup: f32,
    /// `BuildTurretBuild` proxy build-plan queue.
    pub build_plans: SmallVec<[BuildPlan; 4]>,
    /// `BuildTurretBuild.following`-equivalent: the plan currently being built.
    pub build_target: Option<(i32, i32)>,
    /// `BuildTurretBuild.following` (`BuildTurret.java:143`): the nearby
    /// actively-building unit whose plan queue is mirrored.
    pub following: Option<Entity>,
    /// `BuildTurretBuild.lastPlan` (`BuildTurret.java:144`): the team rebuild
    /// queue entry last taken; its `removed` flag invalidates the active plan.
    pub last_plan: Option<crate::game::teams::BlockPlan>,
    /// `BuildTurret.timerTarget` (`BuildTurret.java:32`, interval 15): the
    /// team queue / follower scan gate while idle.
    pub build_target_timer: f32,
    /// `BuildTurret.timerTarget2` (`BuildTurret.java:33`, interval 30): the
    /// another-player-breaking check gate.
    pub build_break_timer: f32,
    /// The detached `block` unit proxy (`TurretBuild.unit`; `Turret.java:297`).
    /// Never added to `UnitRuntime.units`, so it is never saved (I01 T12).
    pub block_unit: Option<Entity>,
}

/// One continuous/laser beam bullet entry (`Turret.BulletEntry`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeamEntry {
    /// Beam bullet entity.
    pub bullet: Entity,
    /// Barrel x offset.
    pub x: f32,
    /// Barrel y offset.
    pub y: f32,
    /// Barrel angle offset.
    pub rotation: f32,
    /// Remaining life (laser turret).
    pub life: f32,
}

/// A pattern shot waiting out its `delay` (`Turret.shoot`'s `Time.run(delay,
/// …)` branch, `Turret.java:763-778`; `queuedBullets` counts these).
#[derive(Debug, Clone, Copy)]
pub struct PendingShot {
    /// Bullet type captured at emission time (`type` in `Turret.shoot`).
    pub bullet: BulletId,
    /// Pattern payload (`xOffset`/`yOffset`/`angleOffset`/`mover`).
    pub shot: PatternShot,
    /// Remaining delay in ticks.
    pub delay: f32,
    /// `barrelCounter` at emission time, restored around the delayed bullet
    /// (`Turret.java:788-795`: "make sure the barrel is the same as what it was
    /// when the bullet was queued to fire").
    pub barrel: i32,
}

/// One item-ammo stack (`ItemTurret.ItemEntry`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmmoEntry {
    /// Item type.
    pub item: ItemId,
    /// Bullet fired by this item.
    pub bullet: BulletId,
    /// Remaining amount (ammo units).
    pub amount: i32,
}

impl TurretState {
    /// Plan-16 draw state (`Layer::turret`); view-only, computed from sim state.
    pub fn draw_state(&self) -> crate::combat::view::TurretDrawState {
        use crate::combat::view::TurretDrawState;
        let ammo_fraction = if self.config.max_ammo > 0 {
            (self.total_ammo as f32 / self.config.max_ammo as f32).clamp(0.0, 1.0)
        } else if self.config.kind == TurretKind::Power {
            1.0
        } else {
            0.0
        };
        TurretDrawState {
            rotation: self.rotation,
            recoil: self.cur_recoil,
            heat: self.heat,
            warmup: self.shoot_warmup,
            charge: self.charge,
            ammo_fraction,
        }
    }

    /// Creates the initial state for a config.
    pub fn new(config: Arc<TurretConfig>) -> Self {
        let activation = config.activation_time;
        let target_interval = config.target_interval;
        Self {
            config,
            rotation: 90.0,
            reload_counter: 0.0,
            cur_recoil: 0.0,
            cur_recoils: SmallVec::new(),
            heat: 0.0,
            heat_req: 0.0,
            shoot_warmup: 0.0,
            charge: 0.0,
            warmup_hold: 0.0,
            total_shots: 0,
            barrel_counter: 0,
            pending: SmallVec::new(),
            ammo: SmallVec::new(),
            total_ammo: 0,
            target: None,
            target_pos: (0.0, 0.0),
            target_timer: target_interval,
            was_shooting: false,
            is_shooting: false,
            activation_timer: activation,
            activated: false,
            logic_control_time: -1.0,
            logic_shooting: false,
            bullets: SmallVec::new(),
            last_length: 0.0,
            retarget_timer: 0.0,
            strength: 0.0,
            unit_target: None,
            bullet_target: None,
            any: false,
            payloads: SmallVec::new(),
            build_warmup: 0.0,
            build_plans: SmallVec::new(),
            build_target: None,
            following: None,
            last_plan: None,
            // `Util timers` start at 0: the first scan happens immediately.
            build_target_timer: 0.0,
            build_break_timer: 0.0,
            block_unit: None,
        }
    }
}

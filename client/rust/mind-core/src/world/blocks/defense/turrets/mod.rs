// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret behavior (`core/src/mindustry/world/blocks/defense/turrets/*.java`).
//!
//! Plan 10 §3.8. This milestone (M5) ports the `BaseTurret` → `ReloadTurret` →
//! `Turret` state machine plus the three core ammo families (`ItemTurret`,
//! `LiquidTurret`, `PowerTurret`): targeting, reload/coolant, ammo stacks,
//! warmup/recoil/heat and the shoot pipeline. The advanced turret classes
//! (`ContinuousTurret`, `LaserTurret`, `PointDefenseTurret`, …) are M6 and live
//! in sibling modules as they land.
//!
//! ## Integration note (plan 10 §3.2 fallback, R-10-1)
//!
//! Plan 07's [`crate::world::behavior::BuildingBehavior::update_tile`] receives
//! only `&mut World`, but turret shooting resolves bullet metadata from the
//! [`crate::content::ContentRegistry`]. Rather than duplicate the content
//! registry into every building behavior (or take a hard dependency on plan 11's
//! `TargetQueries`, which does not exist yet), this M5 slice drives turrets
//! through the plan-10 [`crate::combat::CombatHarness`], which owns the content
//! registry and the deterministic bullet pass. Plan 11/07 reconciliation will
//! hoist this into `BuildingBehavior` once the unit/target queries and a
//! building-side content handle land. The item/liquid hooks here are the same
//! code logistics will call through `accepted_item`/`handle_liquid`.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::ai::types::player::PlayerInput;
use crate::audio::math::AudioRng;
use crate::combat::bullet::unit_is_grounded;
use crate::combat::bullet::{BulletSpawn, CombatCtx};
use crate::combat::targeting::{TargetQueries, UnitSort};
use crate::content::registries::bullets::BulletDef;
use crate::content::registries::sound_meta::SoundId;
use crate::content::registries::units::weapon::ShootPatternSpec;
use crate::content::{BulletId, BulletKind, ContentRegistry, ItemId, LiquidId};
use crate::entities::comp::unit::comp::UnitCore;
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::weapons::pattern::{self, PatternShot, ShotBuffer};
use crate::world::PayloadRef;
use crate::world::blocks::payloads::PayloadKind;
use crate::world::construct::ConstructState;
use crate::world::modules::{LiquidModule, PowerModule};
use crate::world::plan::BuildPlan;

pub mod advanced;
pub mod behavior;
pub mod save;

/// One resolved item-ammo entry (`ItemTurret.ItemEntry` + `ammoTypes`).
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

// ---- content registration (plan 10 owns turret ammo; §4/§6.1) ----

/// Registers the plan-10 turret-ammo fixture bullets (`Blocks.java`
/// `ammoTypes`) used by the M5 oracle. Only the `duo` ammo set is ported so
/// far; the remaining vanilla turret ammo is a tracked M5/M6 content delta.
pub fn register_bullets(content: &mut ContentRegistry, names: &mut BTreeMap<String, BulletId>) {
    let mut add = |name: &str, kind: BulletKind, configure: &dyn Fn(&mut BulletDef)| {
        let mut def = BulletDef::new(kind);
        configure(&mut def);
        if let Ok(id) = content.add_bullet(def) {
            names.insert(name.to_owned(), id);
        }
    };

    // `duo` ammo (Blocks.java `duo`): copper/graphite/silicon.
    let shoot = crate::content::registries::sound_meta::SoundId::SHOOT_DUO;
    add("duo_copper", BulletKind::Basic, &|def| {
        def.speed = 2.5;
        def.lifetime = 60.0;
        def.damage = 9.0;
        def.ammo_multiplier = 2.0;
        def.hit_size = 4.0;
        def.drag = 0.0;
        def.shoot_sound = shoot;
    });
    add("duo_graphite", BulletKind::Basic, &|def| {
        def.speed = 3.5;
        def.lifetime = 60.0;
        def.damage = 18.0;
        // `Blocks.java:3295`: `rangeChange = 16f`.
        def.range_change = 16.0;
        def.ammo_multiplier = 4.0;
        def.reload_multiplier = 0.8;
        def.hit_size = 4.0;
        def.drag = 0.0;
        def.shoot_sound = shoot;
    });
    add("duo_silicon", BulletKind::Basic, &|def| {
        def.speed = 3.0;
        def.lifetime = 60.0;
        def.damage = 12.0;
        def.ammo_multiplier = 5.0;
        def.reload_multiplier = 1.5;
        def.homing_power = 0.2;
        def.hit_size = 4.0;
        def.drag = 0.0;
        def.shoot_sound = shoot;
    });

    // M5/M6 remainder: the rest of the vanilla turret ammo tables (`Blocks.java`).
    advanced::register_bullets(content, names);

    // The ammo bullets are anonymous upstream; name them in the registry so
    // `defense::register` (and the live `BlockTable`) can resolve each turret's
    // ammo table against *this* registry's ids (PAR-0086).
    for (name, id) in names.iter() {
        content.name_bullet(name, *id);
    }

    // `Blocks.java` calls `limitRange(...)` in the turret's double-brace init;
    // upstream mutates the shared `BulletType` instances, so the port mutates
    // the (per-turret) ammo `BulletDef`s after all of them exist.
    limit_range(content, names);
    rebalance_erekir_turret_ammo(content, names);
}

/// Every vanilla turret the port resolves a config for (`config_for`).
const VANILLA_TURRETS: &[&str] = &[
    "duo",
    "scatter",
    "scorch",
    "hail",
    "salvo",
    "swarmer",
    "fuse",
    "ripple",
    "lancer",
    "arc",
    "meltdown",
    "cyclone",
    "foreshadow",
    "spectre",
    "parallax",
    "segment",
    "wave",
    "tsunami",
    "breach",
    "diffuse",
    "sublimate",
    "titan",
    "disperse",
    "afflict",
    "lustre",
    "scathe",
    "smite",
    "malign",
    "build-tower",
];

/// `ErekirTechTree.rebalance()` part (b) (`ErekirTechTree.java:37-47`).
///
/// Every `Turret` whose requirements include an item outside
/// `Items.serpuloItems` scales its **ammo** bullet damage by `* 0.75`, once per
/// bullet id. Java's `instanceof` chain admits `ItemTurret`,
/// `ContinuousLiquidTurret` and `ContinuousTurret` only — `PowerTurret`
/// (`afflict`, `malign`) is deliberately excluded.
///
/// Ordering note: upstream runs this from `ErekirTechTree.load()` while the
/// turret ammo is created inline in `Blocks.load()` (`ItemTurret.ammo`), so it
/// precedes tech-tree construction. The port's turret ammo is installed after
/// content boot (`install_turret_ammo`, GAPS #11), so the equivalent pass runs
/// at that point; `content::tech::ekir::rebalance` keeps owning part (a).
fn rebalance_erekir_turret_ammo(content: &mut ContentRegistry, names: &BTreeMap<String, BulletId>) {
    let serpulo = content.serpulo_items().to_vec();
    let mut balanced: std::collections::BTreeSet<BulletId> = std::collections::BTreeSet::new();
    let mut updates: Vec<(BulletId, f32)> = Vec::new();
    for &name in VANILLA_TURRETS {
        let Some(block) = content.block_by_name(name) else {
            continue;
        };
        // `Structs.contains(block.requirements, i -> !Items.serpuloItems.contains(i.item))`.
        if !block
            .requirements
            .iter()
            .any(|stack| !serpulo.contains(&stack.item))
        {
            continue;
        }
        let Some(config) = config_for(content, name, names) else {
            continue;
        };
        // Java's family gate: ItemTurret | ContinuousLiquidTurret | ContinuousTurret.
        let bullets: Vec<BulletId> = match config.kind {
            TurretKind::Item => match &config.ammo {
                TurretAmmo::Item(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
                _ => Vec::new(),
            },
            TurretKind::Continuous => match &config.ammo {
                TurretAmmo::Liquid(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
                TurretAmmo::Power(ammo) => vec![ammo.bullet],
                TurretAmmo::Item(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
                TurretAmmo::Payload(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
            },
            // `PowerTurret`/`LaserTurret`/`LiquidTurret`/… are not in the Java chain.
            _ => continue,
        };
        for id in bullets {
            if !balanced.insert(id) {
                continue;
            }
            if let Some(def) = content.bullet(id) {
                updates.push((id, def.damage * 0.75));
            }
        }
    }
    for (id, damage) in updates {
        if let Some(def) = content.bullet_mut(id) {
            def.damage = damage;
        }
    }
}

/// `Blocks.java` `limitRange(margin)` calls (`Turret.limitRange`,
/// `Turret.java:246-253`). Absent names do not limit their ammo.
///
/// Serpulo: `duo` 5, `scatter` 2, `hail` 0, `swarmer` 5, `salvo`/`cyclone`/
/// `spectre` default 9. Erekir: `breach` 12, `diffuse` 25, `disperse` 16,
/// `afflict` -55 (`PowerTurret.limitRange`), `scathe`/`smite` default 9.
const LIMIT_RANGE_MARGINS: &[(&str, f32)] = &[
    // `Blocks.java:3348`, `:3434`, `:3544`, `:3748`, `:3853`, `:4225`, `:4338`.
    ("duo", 5.0),
    ("scatter", 2.0),
    ("hail", 0.0),
    ("swarmer", 5.0),
    ("salvo", 9.0),
    ("cyclone", 9.0),
    ("spectre", 9.0),
    // `Blocks.java:4487`, `:4577`, `:5054`, `:5177`, `:5686`, `:5953`.
    ("breach", 12.0),
    ("diffuse", 25.0),
    ("disperse", 16.0),
    ("afflict", -55.0),
    ("scathe", 9.0),
    ("smite", 9.0),
];

/// `BulletType.extraRangeMargin` (`BulletType.java:166-167`): only `disperse`'s
/// thorium/silicon ammo set it (`Blocks.java:4922`, `:4950` = 32).
const EXTRA_RANGE_MARGINS: &[(&str, f32)] =
    &[("disperse_thorium", 32.0), ("disperse_silicon", 32.0)];

/// `Turret.limitRange(BulletType, margin)` (`Turret.java:246-253`):
/// `bullet.lifetime = (rangeChange + range + margin + extraRangeMargin + 10) / speed`.
fn limit_range(content: &mut ContentRegistry, names: &BTreeMap<String, BulletId>) {
    let mut updates: Vec<(BulletId, f32)> = Vec::new();
    for &(turret, margin) in LIMIT_RANGE_MARGINS {
        let Some(config) = config_for(content, turret, names) else {
            continue;
        };
        let bullets: Vec<BulletId> = match &config.ammo {
            TurretAmmo::Item(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
            TurretAmmo::Liquid(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
            TurretAmmo::Power(ammo) => vec![ammo.bullet],
            TurretAmmo::Payload(ammo) => ammo.iter().map(|entry| entry.bullet).collect(),
        };
        for id in bullets {
            let Some(def) = content.bullet(id) else {
                continue;
            };
            // `lifetime = (…) / speed`: a zero-speed bullet (LightningBulletType)
            // has no travel time to limit; Java would divide by zero.
            if def.speed <= 0.0 {
                continue;
            }
            let extra = names
                .iter()
                .find_map(|(name, other)| (*other == id).then_some(name.as_str()))
                .and_then(|name| {
                    EXTRA_RANGE_MARGINS
                        .iter()
                        .find_map(|&(other, extra)| (other == name).then_some(extra))
                })
                .unwrap_or(0.0);
            let real_range = def.range_change + config.range;
            updates.push((id, (real_range + margin + extra + 10.0) / def.speed));
        }
    }
    for (id, lifetime) in updates {
        if let Some(def) = content.bullet_mut(id) {
            def.lifetime = lifetime;
        }
    }
}

/// Registers the vanilla turret ammo bullets into a booted registry once
/// (idempotent). The live sim's content boot calls this so placed turrets get
/// a real [`behavior::TurretBehavior`] instead of the inert default (PAR-0086).
pub fn install_turret_ammo(content: &mut ContentRegistry) {
    if content.bullet_by_name("duo_copper").is_some() {
        return;
    }
    let mut names = BTreeMap::new();
    register_bullets(content, &mut names);
    // The anonymous ammo bullets are appended after the registry's `link`
    // pass, so patch their `BulletType.estimateDPS()` too (`BulletType.java:
    // 424-441`; `Turret.estimateDps` reads it for the RTS `estimateStats`).
    crate::content::registries::units::compute_bullet_dps(content);
}

/// Builds the resolved [`TurretConfig`] for a supported block name.
///
/// `names` is the harness bullet-name map (vanilla bullets are anonymous, so
/// the plan-10 fixtures are addressed by name). `None` for unported turrets.
pub fn config_for(
    content: &ContentRegistry,
    name: &str,
    names: &BTreeMap<String, BulletId>,
) -> Option<TurretConfig> {
    let mut config = config_for_inner(content, name, names)?;
    // `Turret.init` (`Turret.java:223`): `trackingRange = Math.max(range,
    // trackingRange)`.
    config.tracking_range = config.tracking_range.max(config.range);
    apply_block_coolant(content, name, &mut config);
    apply_turret_sounds(name, &mut config);
    // Harness fixtures (`test-item`, ...) carry a coolant amount but no block
    // consumer: default to the `consumeCoolant` liquid filter.
    if config.uses_coolant() && config.coolant_liquids.is_empty() {
        config.coolant_liquids = coolant_filter(content, true, false);
    }
    Some(config)
}

/// `ConsumeCoolant.filter`: `coolant && (allowLiquid && !gas || allowGas &&
/// gas) && temperature <= maxTemp(0.5) && flammability < maxFlammability(0.1)`.
fn coolant_filter(content: &ContentRegistry, allow_liquid: bool, allow_gas: bool) -> Vec<LiquidId> {
    content
        .liquids()
        .iter()
        .filter(|l| {
            l.coolant
                && ((allow_liquid && !l.gas) || (allow_gas && l.gas))
                && l.temperature <= 0.5
                && l.flammability < 0.1
        })
        .map(|l| l.id)
        .collect()
}

/// `Blocks.java` `coolantMultiplier` for the vanilla turrets that declare a
/// coolant consumer (absent = the `BaseTurret` default 5, or the class
/// default: `LaserTurret`/`ContinuousTurret`/`TractorBeamTurret` 1,
/// `PointDefenseTurret` 2).
const COOLANT_MULTIPLIERS: &[(&str, f32)] = &[
    ("duo", 10.0),
    ("scorch", 1.5),
    ("hail", 10.0),
    ("arc", 30.0),
    ("foreshadow", 0.4),
    ("spectre", 0.5),
    ("meltdown", 1.0),
    ("breach", 15.0),
    ("diffuse", 15.0),
    ("titan", 3.75),
    ("disperse", 6.25),
    ("scathe", 15.0),
    ("smite", 15.0),
];

/// `Blocks.java` turret sound fields (name, `shootSound`, `shootSoundVolume`,
/// `chargeSound`, `loopSound`, `loopSoundVolume`). Absent names keep the
/// `Turret` class defaults (`Turret.java:127-138`).
const TURRET_SOUNDS: &[(&str, &str, f32, &str, &str, f32)] = &[
    ("duo", "shootDuo", 1.0, "none", "none", 0.5),
    ("scatter", "shootScatter", 1.0, "none", "none", 0.5),
    ("scorch", "shootFlame", 1.0, "none", "none", 0.5),
    ("hail", "shootArtillerySmall", 1.0, "none", "none", 0.5),
    ("wave", "none", 1.0, "none", "loopSpray", 0.5),
    ("tsunami", "none", 1.0, "none", "loopSpray", 0.5),
    ("lancer", "shootLancer", 1.0, "chargeLancer", "none", 0.5),
    ("arc", "shootArc", 1.0, "none", "none", 0.5),
    ("swarmer", "shootMissile", 1.0, "none", "none", 0.5),
    ("salvo", "shootSalvo", 1.0, "none", "none", 0.5),
    ("fuse", "shootFuse", 0.9, "none", "none", 0.5),
    ("ripple", "shootRipple", 1.0, "none", "none", 0.5),
    ("cyclone", "shootCyclone", 1.0, "none", "none", 0.5),
    ("foreshadow", "shootForeshadow", 1.0, "none", "none", 0.5),
    ("spectre", "shootSpectre", 1.0, "none", "none", 0.5),
    (
        "meltdown",
        "shootMeltdown",
        1.0,
        "none",
        "beamMeltdown",
        2.0,
    ),
    ("breach", "shootBreach", 1.0, "none", "none", 0.5),
    ("diffuse", "shootDiffuse", 1.0, "none", "none", 0.5),
    ("sublimate", "none", 1.0, "none", "shootSublimate", 1.0),
    ("titan", "shootTank", 1.0, "none", "none", 0.5),
    ("disperse", "shootDisperse", 1.0, "none", "none", 0.5),
    ("lustre", "none", 1.0, "none", "beamLustre", 1.0),
    ("scathe", "shootScathe", 1.0, "none", "none", 0.5),
    ("smite", "shootSmite", 1.0, "none", "loopGlow", 0.8),
    ("malign", "shootMalign", 1.0, "none", "loopMalign", 1.3),
    ("segment", "shootSegment", 1.0, "none", "none", 0.5),
];

/// Applies the `Blocks.java` sound overrides for one turret name.
fn apply_turret_sounds(name: &str, config: &mut TurretConfig) {
    let Some(row) = TURRET_SOUNDS.iter().find(|row| row.0 == name) else {
        return;
    };
    let id = |n: &str| SoundId::by_name(n).unwrap_or(SoundId::NONE);
    config.shoot_sound = id(row.1);
    config.shoot_sound_volume = row.2;
    config.charge_sound = id(row.3);
    config.loop_sound = id(row.4);
    config.loop_sound_volume = row.5;
}

/// `BaseTurret.init`: the coolant consumer comes from the block's content
/// spec (`coolant = consumeCoolant(x)` / `consume(new ConsumeLiquid(water,
/// x))`, both made optional), so the config mirrors the content data
/// (PAR-0087).
fn apply_block_coolant(content: &ContentRegistry, name: &str, config: &mut TurretConfig) {
    use crate::content::Consume;
    let Some(def) = content.block_id(name).and_then(|id| content.block(id)) else {
        return;
    };
    let mut found = false;
    for spec in &def.consumes {
        match &spec.consume {
            Consume::Coolant {
                amount,
                allow_liquid,
                allow_gas,
            } => {
                config.coolant_amount = *amount;
                // `ConsumeCoolant` filter (maxTemp 0.5, maxFlammability 0.1).
                config.coolant_liquids = coolant_filter(content, *allow_liquid, *allow_gas);
                found = true;
            }
            Consume::Liquid { liquid, amount } if spec.optional => {
                config.coolant_amount = *amount;
                config.coolant_liquids = vec![*liquid];
                found = true;
            }
            _ => {}
        }
    }
    if !found {
        return;
    }
    config.coolant_multiplier = COOLANT_MULTIPLIERS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, m)| *m)
        .unwrap_or(match config.kind {
            TurretKind::Laser | TurretKind::Continuous | TurretKind::TractorBeam => 1.0,
            TurretKind::PointDefense => 2.0,
            _ => 5.0,
        });
}

fn config_for_inner(
    content: &ContentRegistry,
    name: &str,
    names: &BTreeMap<String, BulletId>,
) -> Option<TurretConfig> {
    let item = |n: &str| content.item_id(n);
    let liquid = |n: &str| content.liquid_id(n);
    let bullet = |n: &str| names.get(n).copied();
    let item_ammo = |item_name: &str, bullet_name: &str| -> Option<ItemAmmo> {
        let id = bullet(bullet_name)?;
        let def = content.bullet(id);
        let mult = def.map(|d| d.ammo_multiplier as i32).unwrap_or(1);
        Some(ItemAmmo {
            item: item(item_name)?,
            bullet: id,
            ammo_multiplier: mult.max(1),
            range_change: def.map(|d| d.range_change).unwrap_or(0.0),
            min_range_change: def.map(|d| d.min_range_change).unwrap_or(0.0),
        })
    };
    let liquid_ammo = |liquid_name: &str, bullet_name: &str| -> Option<LiquidAmmo> {
        let id = bullet(bullet_name)?;
        let def = content.bullet(id);
        let mult = def.map(|d| d.ammo_multiplier).unwrap_or(1.0);
        Some(LiquidAmmo {
            liquid: liquid(liquid_name)?,
            bullet: id,
            ammo_multiplier: mult.max(0.001),
            range_change: def.map(|d| d.range_change).unwrap_or(0.0),
            min_range_change: def.map(|d| d.min_range_change).unwrap_or(0.0),
        })
    };

    match name {
        "duo" => {
            let copper = item_ammo("copper", "duo_copper")?;
            let graphite = item_ammo("graphite", "duo_graphite")?;
            let silicon = item_ammo("silicon", "duo_silicon")?;
            Some(TurretConfig {
                ammo: TurretAmmo::Item(vec![copper, graphite, silicon]),
                liquid_consumed: 1.0 / 60.0,
                range: 160.0,
                reload: 20.0,
                max_ammo: 30,
                ammo_per_shot: 1,
                consume_ammo_once: true,
                rotate_speed: 10.0,
                shoot_cone: 15.0,
                min_warmup: 0.0,
                shoot_warmup_speed: 0.1,
                linear_warmup: false,
                warmup_maintain_time: 0.0,
                target_interval: 20.0,
                new_target_interval: -1.0,
                unit_sort: UnitSort::Closest,
                inaccuracy: 2.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
                min_range: 0.0,
                tracking_range: 0.0,
                recoil: 0.5,
                recoils: 2,
                recoil_time: 20.0,
                recoil_pow: 1.8,
                cooldown_time: 20.0,
                shoot_x: 0.0,
                shoot_y: 3.0,
                x_rand: 0.0,
                shoot: ShootPatternSpec::alternate(1, 0.0, 3.5, 2),
                target_air: true,
                target_ground: true,
                target_blocks: true,
                target_under_blocks: true,
                target_healing: false,
                always_shooting: false,
                predict_target: true,
                accurate_delay: true,
                move_while_charging: true,
                activation_time: 0.0,
                coolant_multiplier: 10.0,
                coolant_amount: 0.1,
                coolant_liquids: Vec::new(),
                heat_requirement: -1.0,
                max_heat_efficiency: 3.0,
                kind: TurretKind::Item,
                retarget_time: 5.0,
                shoot_length: 0.0,
                aim_change_speed: f32::INFINITY,
                scale_damage_efficiency: false,
                bullet_damage: 0.0,
                force: 0.0,
                scaled_force: 0.0,
                beam_damage: 0.0,
                status: crate::content::StatusId::NONE,
                status_duration: 0.0,
                status_chance: 0.0,
                shoot_duration: 0.0,
                firing_move_fract: 1.0,
                build_speed: 1.0,
                shoot_sound: SoundId::SHOOT_DUO,
                shoot_sound_volume: 1.0,
                charge_sound: SoundId::NONE,
                loop_sound: SoundId::NONE,
                loop_sound_volume: 0.5,
                sound_pitch_min: 0.9,
                sound_pitch_max: 1.1,
                fog_radius_multiplier: 1.0,
            })
        }
        "test-item" => {
            let copper = item_ammo("copper", "duo_copper")?;
            Some(TurretConfig {
                ammo: TurretAmmo::Item(vec![copper]),
                liquid_consumed: 1.0 / 60.0,
                range: 120.0,
                reload: 10.0,
                max_ammo: 30,
                ammo_per_shot: 1,
                consume_ammo_once: true,
                rotate_speed: 20.0,
                shoot_cone: 15.0,
                min_warmup: 0.0,
                shoot_warmup_speed: 0.5,
                linear_warmup: false,
                warmup_maintain_time: 0.0,
                target_interval: 5.0,
                new_target_interval: -1.0,
                unit_sort: UnitSort::Closest,
                inaccuracy: 0.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
                min_range: 0.0,
                tracking_range: 0.0,
                recoil: 1.0,
                recoils: -1,
                recoil_time: 10.0,
                recoil_pow: 1.8,
                cooldown_time: 10.0,
                shoot_x: 0.0,
                shoot_y: 0.0,
                x_rand: 0.0,
                shoot: ShootPatternSpec::plain(1, 0.0, 0.0),
                target_air: true,
                target_ground: true,
                target_blocks: true,
                target_under_blocks: true,
                target_healing: false,
                always_shooting: false,
                predict_target: true,
                accurate_delay: true,
                move_while_charging: true,
                activation_time: 0.0,
                coolant_multiplier: 10.0,
                coolant_amount: 0.1,
                coolant_liquids: Vec::new(),
                heat_requirement: -1.0,
                max_heat_efficiency: 3.0,
                kind: TurretKind::Item,
                retarget_time: 5.0,
                shoot_length: 0.0,
                aim_change_speed: f32::INFINITY,
                scale_damage_efficiency: false,
                bullet_damage: 0.0,
                force: 0.0,
                scaled_force: 0.0,
                beam_damage: 0.0,
                status: crate::content::StatusId::NONE,
                status_duration: 0.0,
                status_chance: 0.0,
                shoot_duration: 0.0,
                firing_move_fract: 1.0,
                build_speed: 1.0,
                shoot_sound: SoundId::SHOOT_DUO,
                shoot_sound_volume: 1.0,
                charge_sound: SoundId::NONE,
                loop_sound: SoundId::NONE,
                loop_sound_volume: 0.5,
                sound_pitch_min: 0.9,
                sound_pitch_max: 1.1,
                fog_radius_multiplier: 1.0,
            })
        }
        "test-liquid" => {
            let water = liquid_ammo("water", "fuse")?;
            Some(TurretConfig {
                ammo: TurretAmmo::Liquid(vec![water]),
                liquid_consumed: 1.0 / 60.0,
                range: 120.0,
                reload: 5.0,
                max_ammo: 0,
                ammo_per_shot: 1,
                consume_ammo_once: true,
                rotate_speed: 20.0,
                shoot_cone: 15.0,
                min_warmup: 0.0,
                shoot_warmup_speed: 0.5,
                linear_warmup: false,
                warmup_maintain_time: 0.0,
                target_interval: 5.0,
                new_target_interval: -1.0,
                unit_sort: UnitSort::Closest,
                inaccuracy: 0.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
                min_range: 0.0,
                tracking_range: 0.0,
                recoil: 1.0,
                recoils: -1,
                recoil_time: 5.0,
                recoil_pow: 1.8,
                cooldown_time: 5.0,
                shoot_x: 0.0,
                shoot_y: 0.0,
                x_rand: 0.0,
                shoot: ShootPatternSpec::plain(1, 0.0, 0.0),
                target_air: true,
                target_ground: true,
                target_blocks: true,
                target_under_blocks: true,
                target_healing: false,
                always_shooting: false,
                predict_target: true,
                accurate_delay: true,
                move_while_charging: true,
                activation_time: 0.0,
                coolant_multiplier: 0.0,
                coolant_amount: 0.0,
                coolant_liquids: Vec::new(),
                heat_requirement: -1.0,
                max_heat_efficiency: 3.0,
                kind: TurretKind::Item,
                retarget_time: 5.0,
                shoot_length: 0.0,
                aim_change_speed: f32::INFINITY,
                scale_damage_efficiency: false,
                bullet_damage: 0.0,
                force: 0.0,
                scaled_force: 0.0,
                beam_damage: 0.0,
                status: crate::content::StatusId::NONE,
                status_duration: 0.0,
                status_chance: 0.0,
                shoot_duration: 0.0,
                firing_move_fract: 1.0,
                build_speed: 1.0,
                shoot_sound: SoundId::SHOOT_DUO,
                shoot_sound_volume: 1.0,
                charge_sound: SoundId::NONE,
                loop_sound: SoundId::NONE,
                loop_sound_volume: 0.5,
                sound_pitch_min: 0.9,
                sound_pitch_max: 1.1,
                fog_radius_multiplier: 1.0,
            })
        }
        "test-power" => {
            let bullet = bullet("fuse")?;
            let def = content.bullet(bullet);
            Some(TurretConfig {
                ammo: TurretAmmo::Power(PowerAmmo {
                    bullet,
                    range_change: def.map(|d| d.range_change).unwrap_or(0.0),
                    min_range_change: def.map(|d| d.min_range_change).unwrap_or(0.0),
                }),
                liquid_consumed: 1.0 / 60.0,
                range: 120.0,
                reload: 10.0,
                max_ammo: 0,
                ammo_per_shot: 1,
                consume_ammo_once: true,
                rotate_speed: 20.0,
                shoot_cone: 15.0,
                min_warmup: 0.0,
                shoot_warmup_speed: 0.5,
                linear_warmup: false,
                warmup_maintain_time: 0.0,
                target_interval: 5.0,
                new_target_interval: -1.0,
                unit_sort: UnitSort::Closest,
                inaccuracy: 0.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
                min_range: 0.0,
                tracking_range: 0.0,
                recoil: 1.0,
                recoils: -1,
                recoil_time: 10.0,
                recoil_pow: 1.8,
                cooldown_time: 10.0,
                shoot_x: 0.0,
                shoot_y: 0.0,
                x_rand: 0.0,
                shoot: ShootPatternSpec::plain(1, 0.0, 0.0),
                target_air: true,
                target_ground: true,
                target_blocks: true,
                target_under_blocks: true,
                target_healing: false,
                always_shooting: false,
                predict_target: true,
                accurate_delay: true,
                move_while_charging: true,
                activation_time: 0.0,
                coolant_multiplier: 0.0,
                coolant_amount: 0.0,
                coolant_liquids: Vec::new(),
                heat_requirement: -1.0,
                max_heat_efficiency: 3.0,
                kind: TurretKind::Item,
                retarget_time: 5.0,
                shoot_length: 0.0,
                aim_change_speed: f32::INFINITY,
                scale_damage_efficiency: false,
                bullet_damage: 0.0,
                force: 0.0,
                scaled_force: 0.0,
                beam_damage: 0.0,
                status: crate::content::StatusId::NONE,
                status_duration: 0.0,
                status_chance: 0.0,
                shoot_duration: 0.0,
                firing_move_fract: 1.0,
                build_speed: 1.0,
                shoot_sound: SoundId::SHOOT_DUO,
                shoot_sound_volume: 1.0,
                charge_sound: SoundId::NONE,
                loop_sound: SoundId::NONE,
                loop_sound_volume: 0.5,
                sound_pitch_min: 0.9,
                sound_pitch_max: 1.1,
                fog_radius_multiplier: 1.0,
            })
        }
        _ => advanced::config_for(content, name, names),
    }
}

// ---- math helpers (`Angles`/`Mathf` subset, delta == 1) ----

fn trnsx(angle_deg: f32, x: f32, y: f32) -> f32 {
    let r = angle_deg.to_radians();
    x * r.cos() + y * r.sin()
}

fn trnsy(angle_deg: f32, x: f32, y: f32) -> f32 {
    let r = angle_deg.to_radians();
    x * r.sin() - y * r.cos()
}

fn angle_to(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    (y2 - y1).atan2(x2 - x1).to_degrees()
}

fn angle_dist(a: f32, b: f32) -> f32 {
    let d = (b - a).rem_euclid(360.0);
    if d > 180.0 { 360.0 - d } else { d }
}

fn move_toward(from: f32, to: f32, speed: f32) -> f32 {
    if from < to {
        (from + speed).min(to)
    } else {
        (from - speed).max(to)
    }
}

fn approach_delta(from: f32, to: f32, speed: f32) -> f32 {
    move_toward(from, to, speed)
}

fn lerp_delta(from: f32, to: f32, alpha: f32) -> f32 {
    from + (to - from) * alpha.clamp(0.0, 1.0)
}

fn efficiency(world: &World, e: Entity) -> f32 {
    world
        .get::<crate::entities::comp::Building>(e)
        .map(|b| b.efficiency)
        .unwrap_or(1.0)
}

/// `TeamComp.cheating()` (`TeamComp.java:17-19`): `team.rules().cheat`.
///
/// A cheating team never consumes turret ammo and ignores the heat efficiency
/// floor (`Turret.java:679`, `:685`, `:715`, `ItemTurret.java:109`).
pub fn cheating(world: &World, e: Entity) -> bool {
    let team = world.get::<TeamComp>(e).map(|team| team.team).unwrap_or(0);
    crate::game::runtime::rules_of(world).is_some_and(|rules| rules.team_rule(team).cheat)
}

fn position(world: &World, e: Entity) -> Option<(f32, f32)> {
    world.get::<Pos>(e).map(|p| (p.x, p.y))
}

// ---- ammo ----

/// `TurretBuild.peekAmmo`: the bullet that the next `use_ammo` returns.
pub fn peek_ammo(world: &World, e: Entity) -> Option<BulletId> {
    let state = world.get::<TurretState>(e)?;
    peek_ammo_state(state, world, e)
}

fn peek_ammo_state(state: &TurretState, world: &World, e: Entity) -> Option<BulletId> {
    match &state.config.ammo {
        TurretAmmo::Item(_) => state.ammo.last().map(|entry| entry.bullet),
        // `LiquidTurretBuild.peekAmmo` / `ContinuousLiquidTurretBuild.peekAmmo`
        // (`LiquidTurret.java:136-139`, `ContinuousLiquidTurret.java:118-121`):
        // `ammoTypes.get(liquids.current())`, independent of the stored amount.
        TurretAmmo::Liquid(ammo) => {
            let liquids = world.get::<LiquidModule>(e)?;
            current_liquid_ammo(liquids, ammo).map(|entry| entry.bullet)
        }
        TurretAmmo::Power(ammo) => Some(ammo.bullet),
        TurretAmmo::Payload(ammo) => ammo
            .iter()
            .find(|entry| {
                state
                    .payloads
                    .iter()
                    .any(|stack| stack.content == entry.content && stack.count > 0)
            })
            .map(|entry| entry.bullet),
    }
}

/// `TurretBuild.peekAmmo().rangeChange` (`BulletType.rangeChange`).
///
/// Mirrors [`peek_ammo_state`]'s selection: the last item entry, the entry of
/// `liquids.current()`, the power bullet def or the first stocked payload entry
/// (`Power` resolves the def from the runtime content because the ammo enum
/// keeps only the bullet id).
pub fn peek_ammo_range_change(world: &World, e: Entity) -> f32 {
    let Some(state) = world.get::<TurretState>(e) else {
        return 0.0;
    };
    peek_ammo_range_change_of(state, world, e)
}

/// [`peek_ammo_range_change`] for callers that already hold the dispatched
/// state (the turret pass takes `TurretState` out of the entity for the
/// duration of `update_turret`).
pub fn peek_ammo_range_change_of(state: &TurretState, world: &World, e: Entity) -> f32 {
    match &state.config.ammo {
        // `peekAmmo()` returns the bullet of the last stack (`peek_ammo_state`);
        // its `rangeChange` comes from the matching config entry.
        TurretAmmo::Item(ammo) => state
            .ammo
            .last()
            .and_then(|entry| ammo.iter().find(|config| config.bullet == entry.bullet))
            .map_or(0.0, |config| config.range_change),
        TurretAmmo::Liquid(ammo) => {
            let Some(liquids) = world.get::<LiquidModule>(e) else {
                return 0.0;
            };
            current_liquid_ammo(liquids, ammo).map_or(0.0, |entry| entry.range_change)
        }
        TurretAmmo::Power(ammo) => ammo.range_change,
        TurretAmmo::Payload(ammo) => ammo
            .iter()
            .find(|entry| {
                state
                    .payloads
                    .iter()
                    .any(|stack| stack.content == entry.content && stack.count > 0)
            })
            .map_or(0.0, |entry| entry.range_change),
    }
}

/// `TurretBuild.peekAmmo().minRangeChange` (`BulletType.minRangeChange`):
/// mirrors [`peek_ammo_range_change_of`]'s ammo selection for the minimum
/// range (`Turret.TurretBuild.minRange()`, `Turret.java:340-344`); `Power`
/// resolves the bullet def from the runtime content like
/// [`peek_ammo_range_change_of`].
pub fn peek_ammo_min_range_change_of(state: &TurretState, world: &World, e: Entity) -> f32 {
    match &state.config.ammo {
        TurretAmmo::Item(ammo) => state
            .ammo
            .last()
            .and_then(|entry| ammo.iter().find(|config| config.bullet == entry.bullet))
            .map_or(0.0, |config| config.min_range_change),
        TurretAmmo::Liquid(ammo) => {
            let Some(liquids) = world.get::<LiquidModule>(e) else {
                return 0.0;
            };
            current_liquid_ammo(liquids, ammo).map_or(0.0, |entry| entry.min_range_change)
        }
        TurretAmmo::Power(ammo) => ammo.min_range_change,
        TurretAmmo::Payload(ammo) => ammo
            .iter()
            .find(|entry| {
                state
                    .payloads
                    .iter()
                    .any(|stack| stack.content == entry.content && stack.count > 0)
            })
            .map_or(0.0, |entry| entry.min_range_change),
    }
}

/// `Turret.TurretBuild.range()` (`Turret.java:348-353`): `range +
/// peekAmmo().rangeChange`, the range used by targeting, the range circle, the
/// fog radius and the logic `@range` sensor.
pub fn turret_range(world: &World, e: Entity) -> f32 {
    world
        .get::<TurretState>(e)
        .map_or(0.0, |state| state.config.range)
        + peek_ammo_range_change(world, e)
}

/// [`turret_range`] for callers that already hold the dispatched state.
pub fn turret_range_of(state: &TurretState, world: &World, e: Entity) -> f32 {
    state.config.range + peek_ammo_range_change_of(state, world, e)
}

/// `Turret.logicControlCooldown` (`60 * 2` ticks).
pub const LOGIC_CONTROL_COOLDOWN: f32 = 120.0;

/// `TurretBuild.targetPosition(Posc)` for the logic `shootp` path: the same
/// intercept math as [`target_position`], sourced from the world alone (the
/// logic VM has no `CombatCtx`).
pub fn logic_target_position(
    world: &World,
    e: Entity,
    state: &TurretState,
    target: Entity,
) -> Option<(f32, f32)> {
    let (x, y) = position(world, e)?;
    let (tx, ty) = position(world, target)?;
    let speed = world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .and_then(|runtime| {
            peek_ammo_state(state, world, e)
                .and_then(|id| runtime.content.bullet(id))
                .map(|def| def.speed)
        })
        .unwrap_or(0.0);
    let delta = world
        .get::<crate::entities::comp::Vel>(target)
        .copied()
        .unwrap_or(crate::entities::comp::Vel { x: 0.0, y: 0.0 });
    let config = &state.config;
    let offset = if config.accurate_delay && !config.move_while_charging {
        (
            delta.x * config.shoot.first_shot_delay,
            delta.y * config.shoot.first_shot_delay,
        )
    } else {
        (0.0, 0.0)
    };
    let aim = if config.predict_target && speed >= 0.01 {
        crate::combat::targeting::intercept_positions(
            (x, y),
            (0.0, 0.0),
            (tx + offset.0, ty + offset.1),
            (delta.x, delta.y),
            true,
            speed,
        )
    } else {
        (tx, ty)
    };
    Some(if aim.0 == 0.0 && aim.1 == 0.0 {
        (tx, ty)
    } else {
        aim
    })
}

/// `TurretBuild.hasAmmo`, including the "swap a usable entry to the back" rule.
pub fn has_ammo(world: &World, e: Entity) -> bool {
    let Some(state) = world.get::<TurretState>(e) else {
        return false;
    };
    has_ammo_state(state, world, e)
}

fn has_ammo_state(state: &TurretState, world: &World, e: Entity) -> bool {
    match &state.config.ammo {
        // `Turret.java:715`: `ammo.size > 0 && (ammo.peek().amount >=
        // ammoPerShot || cheating())`.
        TurretAmmo::Item(_) => state
            .ammo
            .last()
            .is_some_and(|entry| entry.amount >= state.config.ammo_per_shot || cheating(world, e)),
        TurretAmmo::Liquid(ammo) => {
            let Some(liquids) = world.get::<LiquidModule>(e) else {
                return false;
            };
            // `ContinuousLiquidTurret.hasAmmo` (`:124-126`; T14): the activated
            // latch plus a correct, non-empty current liquid.
            if state.config.kind == TurretKind::Continuous {
                return state.activated
                    && current_ammo_liquid(liquids, &state.config).is_some()
                    && liquids.current_liquid_amount() > 0.0;
            }
            // `LiquidTurretBuild.hasAmmo` (`LiquidTurret.java:141-144`):
            // `ammoTypes.get(current) != null && currentAmount() >= 1 /
            // ammoTypes.get(current).ammoMultiplier`.
            current_liquid_ammo(liquids, ammo)
                .is_some_and(|entry| liquids.current_liquid_amount() >= 1.0 / entry.ammo_multiplier)
        }
        TurretAmmo::Power(_) => true,
        TurretAmmo::Payload(ammo) => ammo.iter().any(|entry| {
            state
                .payloads
                .iter()
                .any(|stack| stack.content == entry.content && stack.count > 0)
        }),
    }
}

/// `ammoTypes.get(liquids.current())`: the ammo entry of the module's current
/// liquid (`LiquidModule.current()`, the last liquid added/removed/loaded,
/// `LiquidModule.java:89-91`), or `None` when that liquid is not ammo. Java
/// never picks the most-filled slot.
fn current_liquid_ammo<'a>(
    liquids: &LiquidModule,
    ammo: &'a [LiquidAmmo],
) -> Option<&'a LiquidAmmo> {
    let current = liquids.current_liquid();
    ammo.iter().find(|entry| entry.liquid == current)
}

/// `liquids.current()` when `ammoTypes` contains it (`ContinuousLiquidTurret`
/// reads `liquids.current()`/`currentAmount`, T14).
fn current_ammo_liquid(
    liquids: &LiquidModule,
    config: &TurretConfig,
) -> Option<crate::content::LiquidId> {
    let TurretAmmo::Liquid(ammo) = &config.ammo else {
        return None;
    };
    current_liquid_ammo(liquids, ammo).map(|entry| entry.liquid)
}

/// `ConsumeLiquidFilter.getConsumed` (`ConsumeLiquidFilter.java:66-80`): the
/// current liquid when it passes the ammo filter and is non-empty, else the
/// lowest-id ammo liquid with a positive amount.
fn consumed_ammo_liquid(
    liquids: &LiquidModule,
    config: &TurretConfig,
) -> Option<crate::content::LiquidId> {
    if liquids.current_liquid_amount() > 0.0
        && let Some(liquid) = current_ammo_liquid(liquids, config)
    {
        return Some(liquid);
    }
    let TurretAmmo::Liquid(ammo) = &config.ammo else {
        return None;
    };
    ammo.iter()
        .map(|entry| entry.liquid)
        .filter(|liquid| liquids.get(*liquid) > 0.0)
        .min_by_key(|liquid| liquid.index())
}

/// `TurretBuild.canConsume`: power/heat gates (item/liquid gate handled by
/// `has_ammo`).
fn can_consume(state: &TurretState, world: &World, e: Entity, config: &TurretConfig) -> bool {
    if config.heat_requirement > 0.0 && state.heat_req <= 0.0 {
        return false;
    }
    if matches!(config.ammo, TurretAmmo::Power(_)) {
        return world
            .get::<PowerModule>(e)
            .map(|power| power.status > 0.0)
            .unwrap_or(false);
    }
    true
}

/// `ItemTurretBuild.acceptItem`.
pub fn accept_item(world: &World, e: Entity, item: ItemId) -> bool {
    let Some(state) = world.get::<TurretState>(e) else {
        return false;
    };
    let TurretAmmo::Item(ammo) = &state.config.ammo else {
        return false;
    };
    let Some(entry) = ammo.iter().find(|entry| entry.item == item) else {
        return false;
    };
    state.total_ammo + entry.ammo_multiplier <= state.config.max_ammo
}

/// `ItemTurretBuild.handleItem`.
pub fn handle_item(world: &mut World, e: Entity, item: ItemId) {
    let Some(mut state) = world.entity_mut(e).take::<TurretState>() else {
        return;
    };
    let TurretAmmo::Item(ammo) = &state.config.ammo else {
        world.entity_mut(e).insert(state);
        return;
    };
    let Some(entry) = ammo.iter().find(|entry| entry.item == item).copied() else {
        world.entity_mut(e).insert(state);
        return;
    };
    state.total_ammo += entry.ammo_multiplier;
    // Merge into an existing entry and move it to the back (`ammo.swap`).
    if let Some(index) = state.ammo.iter().position(|existing| existing.item == item) {
        state.ammo[index].amount += entry.ammo_multiplier;
        let last = state.ammo.len() - 1;
        state.ammo.swap(index, last);
    } else {
        state.ammo.push(AmmoEntry {
            item,
            bullet: entry.bullet,
            amount: entry.ammo_multiplier,
        });
    }
    world.entity_mut(e).insert(state);
}

/// `LiquidTurretBuild.acceptLiquid`.
pub fn accept_liquid(world: &World, e: Entity, liquid: LiquidId) -> bool {
    let Some(state) = world.get::<TurretState>(e) else {
        return false;
    };
    let TurretAmmo::Liquid(ammo) = &state.config.ammo else {
        // `Building.acceptLiquid`: `hasLiquids && consumesLiquid(liquid)` —
        // the coolant consumer's filter (PAR-0087).
        return state.config.coolant_liquids.contains(&liquid)
            && world.get::<LiquidModule>(e).is_some();
    };
    if !ammo.iter().any(|entry| entry.liquid == liquid) {
        return false;
    }
    let Some(liquids) = world.get::<LiquidModule>(e) else {
        return false;
    };
    // `LiquidTurret.java:151-155` / `ContinuousLiquidTurret.java:138-142`:
    // `current == liquid || !ammoTypes.containsKey(current) ||
    // liquids.get(current) <= 1 / ammoTypes.get(current).ammoMultiplier + 0.001`.
    let current = liquids.current_liquid();
    if current == liquid {
        return true;
    }
    match current_liquid_ammo(liquids, ammo) {
        Some(entry) => liquids.get(current) <= 1.0 / entry.ammo_multiplier + 0.001,
        None => true,
    }
}

/// `LiquidTurretBuild.handleLiquid` (only fills the storage module).
pub fn handle_liquid(world: &mut World, e: Entity, liquid: LiquidId, amount: f32) {
    // `Building.handleLiquid` fills up to the block's `liquidCapacity`
    // (duo 20, wave 10, tsunami 40; PAR-0087). Fixture turrets without a
    // placed block fall back to the old 60-unit liquid-ammo capacity.
    let block_capacity = world
        .get::<crate::entities::comp::Building>(e)
        .map(|b| b.block)
        .and_then(|block| {
            world
                .get_resource::<crate::world::block::BlockTable>()
                .and_then(|table| table.instance(block))
        })
        .map(|inst| inst.def.liquid_capacity);
    let capacity = match block_capacity {
        Some(capacity) if capacity > 0.0 => capacity,
        _ => world
            .get::<TurretState>(e)
            .map(|state| match &state.config.ammo {
                TurretAmmo::Liquid(_) => 60.0f32,
                _ if state.config.uses_coolant() => 20.0,
                _ => 0.0,
            })
            .unwrap_or(0.0),
    };
    if let Some(mut liquids) = world.get_mut::<LiquidModule>(e) {
        liquids.add(liquid, amount, capacity);
    }
}

/// `PayloadTurretBuild.acceptPayload`.
pub fn accept_payload(world: &World, e: Entity, payload: PayloadRef) -> bool {
    let Some(state) = world.get::<TurretState>(e) else {
        return false;
    };
    let TurretAmmo::Payload(ammo) = &state.config.ammo else {
        return false;
    };
    let content = PayloadContent::from_payload(payload);
    let total: i32 = state.payloads.iter().map(|stack| stack.count).sum();
    total < state.config.max_ammo && ammo.iter().any(|entry| entry.content == content)
}

/// `PayloadTurretBuild.handlePayload`.
pub fn handle_payload(world: &mut World, e: Entity, payload: PayloadRef) {
    let Some(mut state) = world.entity_mut(e).take::<TurretState>() else {
        return;
    };
    let content = PayloadContent::from_payload(payload);
    if let TurretAmmo::Payload(ammo) = &state.config.ammo
        && ammo.iter().any(|entry| entry.content == content)
    {
        if let Some(stack) = state
            .payloads
            .iter_mut()
            .find(|stack| stack.content == content)
        {
            stack.count += 1;
        } else {
            state.payloads.push(PayloadStack { content, count: 1 });
        }
    }
    world.entity_mut(e).insert(state);
}

/// Queues a build plan on a `BuildTurret` (`unit.addBuild`; plan 11/15 seam).
pub fn add_build_plan(world: &mut World, e: Entity, plan: BuildPlan) -> bool {
    let Some(mut state) = world.entity_mut(e).take::<TurretState>() else {
        return false;
    };
    if state.config.kind != TurretKind::Build {
        world.entity_mut(e).insert(state);
        return false;
    }
    state.build_plans.push(plan);
    world.entity_mut(e).insert(state);
    true
}

/// Removes one shot's worth of ammo and returns the fired bullet
/// (`TurretBuild.useAmmo`).
fn use_ammo(state: &mut TurretState, world: &mut World, e: Entity) -> Option<BulletId> {
    let bullet = peek_ammo_state(state, world, e)?;
    // `Turret.java:683-686` / `LiquidTurret.java:128-131`: a cheating team
    // fires without consuming ammo (the item stack or the liquid charge).
    if matches!(
        &state.config.ammo,
        TurretAmmo::Item(_) | TurretAmmo::Liquid(_)
    ) && cheating(world, e)
    {
        return Some(bullet);
    }
    let mut liquid_remove: Option<(LiquidId, f32)> = None;
    match &state.config.ammo {
        TurretAmmo::Item(_) => {
            if let Some(entry) = state.ammo.last_mut() {
                entry.amount -= state.config.ammo_per_shot;
                if entry.amount <= 0 {
                    state.ammo.pop();
                }
            }
            state.total_ammo = (state.total_ammo - state.config.ammo_per_shot).max(0);
        }
        // `LiquidTurretBuild.useAmmo` (`LiquidTurret.java:128-133`):
        // `liquids.remove(current, 1 / type.ammoMultiplier)`.
        // `ContinuousLiquidTurretBuild.useAmmo` does not consume (`:112-116`).
        TurretAmmo::Liquid(ammo) => {
            if state.config.kind != TurretKind::Continuous
                && let Some(liquids) = world.get::<LiquidModule>(e)
                && let Some(entry) = current_liquid_ammo(liquids, ammo)
            {
                let amount = 1.0 / entry.ammo_multiplier.max(0.001);
                liquid_remove = Some((entry.liquid, amount));
            }
        }
        TurretAmmo::Power(_) => {}
        TurretAmmo::Payload(ammo) => {
            let content = ammo
                .iter()
                .find(|entry| {
                    state
                        .payloads
                        .iter()
                        .any(|stack| stack.content == entry.content && stack.count > 0)
                })
                .map(|entry| entry.content);
            if let Some(content) = content {
                if let Some(stack) = state
                    .payloads
                    .iter_mut()
                    .find(|stack| stack.content == content)
                {
                    stack.count -= 1;
                }
                state.payloads.retain(|stack| stack.count > 0);
            }
        }
    }
    if let Some((liquid, amount)) = liquid_remove
        && let Some(mut liquids) = world.get_mut::<LiquidModule>(e)
    {
        liquids.remove(liquid, amount);
    }
    Some(bullet)
}

// ---- update engine ----

/// Updates every turret entity one tick (`Groups.build` turret half).
pub fn update_turrets(ctx: &mut CombatCtx<'_>) {
    // Plan-10 §3.2/§3.7: build the plan-11 `BlockIndexer` + unit snapshot once
    // per pass and query it deterministically instead of scanning all entities.
    let targets = TargetQueries::build(ctx.world, ctx.content);
    let turrets: Vec<Entity> = ctx
        .world
        .iter_entities()
        .filter(|entity| entity.contains::<TurretState>())
        .map(|entity| entity.id())
        .collect();
    for turret in turrets {
        if ctx.world.get::<TurretState>(turret).is_none() {
            continue;
        }
        // `Building.update()`: `if(enabled || !block.noUpdateDisabled)
        // updateTile();` — `Turret` keeps the default `noUpdateDisabled =
        // false`, so a disabled turret does nothing (H09 T4). Fixture entities
        // without a `Building` component (harness turrets) still update.
        if ctx
            .world
            .get::<crate::entities::comp::Building>(turret)
            .is_some_and(|building| !building.enabled)
        {
            continue;
        }
        let Some(mut state) = ctx.world.entity_mut(turret).take::<TurretState>() else {
            continue;
        };
        update_turret(ctx, turret, &mut state, &targets);
        ctx.world.entity_mut(turret).insert(state);
    }
}

fn update_turret(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    targets: &TargetQueries,
) {
    let config = state.config.clone();
    let (x, y) = position(ctx.world, e).unwrap_or((0.0, 0.0));
    let team = ctx.team(e);
    // `TurretBuild.updateTile` (`Turret.java:544-546`): `heatReq =
    // calculateHeat(sideHeat)` — the heat network pulls from adjacent
    // producers/conductors (plan 09 `heat::calculate_heat`).
    if config.heat_requirement > 0.0 {
        let update_id = crate::world::update::build_time(ctx.world) as u64;
        let mut side_heat = [0.0f32; 4];
        let mut came_from = crate::util::IdSet::new();
        state.heat_req = crate::world::blocks::heat::calculate_heat(
            ctx.world,
            e,
            &mut side_heat,
            &mut came_from,
            update_id,
            0,
        );
    }
    // `TurretBuild.updateEfficiencyMultiplier` (`Turret.java:677-681`).
    let eff = update_efficiency_multiplier(ctx.world, e, efficiency(ctx.world, e), state, &config);

    // `Time.run(delay, …)` tasks run at the start of the frame before the
    // building update (Arc `Time.update`), so a delayed shot that comes due this
    // tick fires before reload/charge/shoot (`Turret.java:763-778`).
    update_pending_shots(ctx, e, state);

    // `PayloadTurretBuild.updateTile`: `totalAmmo = payloads.total()`.
    if let TurretAmmo::Payload(_) = &config.ammo {
        state.total_ammo = state.payloads.iter().map(|stack| stack.count).sum();
    }

    // `TurretBuild.updateTile`: `unit.tile(this); unit.rotation(rotation);
    // unit.team(team);` (`Turret.java:535-537`) — keep the detached proxy on
    // the turret (I01 T12).
    if let Some(unit) = state.block_unit
        && ctx.world.get_entity(unit).is_ok()
    {
        if let Some(mut pos) = ctx.world.get_mut::<Pos>(unit) {
            pos.x = x;
            pos.y = y;
        }
        if let Some(mut core) = ctx.world.get_mut::<UnitCore>(unit) {
            core.rotation = state.rotation;
        }
        if let Some(mut unit_team) = ctx.world.get_mut::<TeamComp>(unit) {
            unit_team.team = team;
        }
        if let Some(building) = ctx.world.get::<crate::entities::comp::Building>(e) {
            let packed = building.tile.pack();
            ctx.world
                .entity_mut(unit)
                .insert(crate::entities::comp::unit::comp::BlockUnitComp { tile: packed });
        }
    }

    // `TurretBuild.controlled()` = `unit.isPlayer()` (`Turret.java:448-450`):
    // the proxy carries the `Player` marker while possessed; the player's aim
    // and shoot button come from the shared input resource
    // (`PlayerInput.aim_x/aim_y/shooting`).
    let controlled = state.block_unit.is_some_and(|unit| {
        ctx.world
            .get::<crate::entities::comp::Player>(unit)
            .is_some()
    });
    let player_aim = ctx
        .world
        .get_resource::<PlayerInput>()
        .map(|input| (input.aim_x, input.aim_y, input.shooting))
        .unwrap_or((0.0, 0.0, false));

    // Target validation + shooting intent.
    // `Turret.updateTile` (`Turret.java:540-542`): the logic cooldown ticks
    // down at the top of the update while active.
    if state.logic_control_time > 0.0 {
        state.logic_control_time -= 1.0;
    }
    let logic_controlled = state.logic_control_time > 0.0;
    if state.target.is_some() && !logic_controlled && !validate_target(ctx, e, state) {
        state.target = None;
    }
    // `isShooting = alwaysShooting || (isControlled() ? unit.isShooting() :
    //  logicControlled() ? logicShooting : target != null)`
    // (`Turret.java:555`; the port has no player-controlled turret yet).
    state.is_shooting = config.always_shooting
        || if controlled {
            // `unit.controller() instanceof Player ? unit.isShooting()`
            // (`Turret.java:555`).
            player_aim.2
        } else if logic_controlled {
            state.logic_shooting
        } else {
            state.target.is_some()
        };

    // Warmup.
    let mut warmup_target =
        if (state.is_shooting && can_consume(state, ctx.world, e, &config)) || charging(state) {
            1.0
        } else {
            0.0
        };
    if warmup_target > 0.0 {
        state.warmup_hold = 1.0;
    }
    if state.warmup_hold > 0.0 {
        if config.warmup_maintain_time > 0.0 {
            state.warmup_hold -= 1.0 / config.warmup_maintain_time;
        } else {
            state.warmup_hold = 0.0;
        }
        state.warmup_hold = state.warmup_hold.max(0.0);
        warmup_target = 1.0;
    }
    let warmup_speed = config.shoot_warmup_speed * if warmup_target > 0.0 { eff } else { 1.0 };
    state.shoot_warmup = if config.linear_warmup {
        approach_delta(state.shoot_warmup, warmup_target, warmup_speed)
    } else {
        lerp_delta(state.shoot_warmup, warmup_target, warmup_speed)
    };

    state.was_shooting = false;
    let recoil_time = if config.recoil_time <= 0.0 {
        config.reload
    } else {
        config.recoil_time
    }
    .max(0.0001);
    state.cur_recoil = approach_delta(state.cur_recoil, 0.0, 1.0 / recoil_time);
    if config.recoils > 0 {
        let count = config.recoils as usize;
        if state.cur_recoils.len() < count {
            state.cur_recoils.resize(count, 0.0);
        }
        for i in 0..count {
            state.cur_recoils[i] = approach_delta(state.cur_recoils[i], 0.0, 1.0 / recoil_time);
        }
    }
    let cooldown = if config.cooldown_time <= 0.0 {
        config.reload
    } else {
        config.cooldown_time
    }
    .max(0.0001);
    state.heat = approach_delta(state.heat, 0.0, 1.0 / cooldown);
    state.charge = if charging(state) && config.shoot.first_shot_delay > 0.0 {
        approach_delta(state.charge, 1.0, 1.0 / config.shoot.first_shot_delay)
    } else {
        0.0
    };

    // Reload + coolant (always runs; upstream `handleReload` precedes activation).
    handle_reload(ctx, e, state, &config, eff);

    if state.activation_timer > 0.0 {
        state.activation_timer -= 1.0;
        return;
    }

    // M6 advanced turret classes dispatch before the generic item/liquid/power
    // shooting pipeline (`PointDefenseTurret`/`TractorBeamTurret`/
    // `ContinuousTurret`/`LaserTurret`).
    match config.kind {
        TurretKind::PointDefense => {
            advanced::update_point_defense(ctx, e, state, &config, x, y, team, eff);
            return;
        }
        TurretKind::TractorBeam => {
            advanced::update_tractor(ctx, e, state, &config, x, y, team, eff, targets);
            return;
        }
        TurretKind::Continuous => {
            advanced::update_continuous(ctx, e, state, &config, x, y, team, eff, targets);
            return;
        }
        TurretKind::Laser => {
            advanced::update_laser(ctx, e, state, &config, x, y, team, eff, targets);
            return;
        }
        TurretKind::Build => {
            update_build(ctx, e, state, &config, x, y, eff);
            return;
        }
        _ => {}
    }

    if !has_ammo_state(state, ctx.world, e) {
        return;
    }

    // Target acquisition on the interval timer: `timer(timerTarget, target !=
    // null ? newTargetInterval : targetInterval)` (`Turret.java:571`).
    state.target_timer -= 1.0;
    if state.target_timer <= 0.0 {
        find_target(ctx, e, state, team, x, y, targets);
        state.target_timer = if state.target.is_some() {
            config.new_target_interval.max(0.0001)
        } else {
            config.target_interval
        };
    }

    // `Turret.updateTile` (`Turret.java:571-605`): with a valid target the aim
    // point is re-derived every tick (`targetPosition` leads through
    // `Predict.intercept`); logic control drives firing and keeps whatever aim
    // point logic last set (`validateTarget` holds a null target while
    // `logicControlled()`).
    let can_shoot = if controlled {
        // `Turret.java:575-582` player behavior: `targetPos.set(unit.aimX(),
        // unit.aimY()); canShoot = unit.isShooting();`.
        state.target_pos = (player_aim.0, player_aim.1);
        player_aim.2
    } else if state.logic_control_time > 0.0 {
        state.logic_shooting
    } else {
        let Some(target) = state.target else {
            return;
        };
        target_position(ctx, e, state, x, y, target);
        within_target_range(ctx, target, x, y, turret_range_of(state, ctx.world, e))
    };

    // Turn toward the aim point (`if(shouldTurn()) turnToTarget(targetRot)`,
    // `Turret.java:592-594`).
    let target_rot = angle_to(x, y, state.target_pos.0, state.target_pos.1);
    if should_turn(state) {
        state.rotation = move_toward(state.rotation, target_rot, config.rotate_speed * eff);
    }

    let in_cone = angle_dist(state.rotation, target_rot) < config.shoot_cone;
    if config.always_shooting || (in_cone && can_shoot) {
        state.was_shooting = true;
        update_shooting(ctx, e, state);
    }
}

/// `TurretBuild.updateEfficiencyMultiplier` (`Turret.java:677-681`):
/// `efficiency *= min(max(heatReq / heatRequirement, cheating() ? 1 : 0),
/// maxHeatEfficiency)`.
fn update_efficiency_multiplier(
    world: &World,
    e: Entity,
    eff: f32,
    state: &TurretState,
    config: &TurretConfig,
) -> f32 {
    if config.heat_requirement <= 0.0 {
        return eff;
    }
    let floor = if cheating(world, e) { 1.0 } else { 0.0 };
    eff * (state.heat_req / config.heat_requirement)
        .max(floor)
        .min(config.max_heat_efficiency)
}

/// `TurretBuild.handleReload` + `updateReload` + `updateCooling`.
fn handle_reload(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    config: &TurretConfig,
    eff: f32,
) {
    // `TurretBuild.handleReload`: `(reloadWhileCharging || !charging()) &&
    // reloadCounter < reload`. `reloadWhileCharging` is the class default true
    // (`Turret.java:88`) and no vanilla block overrides it, so charging does not
    // pause reload here.
    // `baseReloadSpeed() == efficiency`, which is zero without power/heat
    // (`TurretBuild.updateReload` × `updateEfficiencyMultiplier`).
    if !can_consume(state, ctx.world, e, config) {
        return;
    }
    if state.reload_counter < config.reload {
        let ammo = peek_ammo_state(state, ctx.world, e);
        state.reload_counter += config.ammo_reload_multiplier(ammo, ctx.content) * eff;
        // `ReloadTurretBuild.updateCooling` (`coolant.efficiency > 0 &&
        // efficiency > 0`): `getConsumed` picks the module's CURRENT filter
        // liquid first (`ConsumeLiquidFilter.java:57-71`), `coolant.update`
        // drains it and the reload gains
        // `amount * edelta * heatCapacity * coolantMultiplier * ammoReloadMultiplier`.
        if config.uses_coolant() && eff > 0.0 {
            let consumed = ctx.world.get::<LiquidModule>(e).and_then(|liquids| {
                crate::world::update::consumed_liquid(liquids, &|liquid| {
                    config.coolant_liquids.contains(&liquid)
                })
                .map(|l| (l, liquids.get(l)))
            });
            if let Some((liquid, stored)) = consumed {
                let needed = config.coolant_amount * eff;
                let coolant_eff = (stored / needed.max(1e-8)).min(1.0);
                let capacity = ctx.content.liquid(liquid).map_or(0.4, |l| l.heat_capacity);
                if let Some(mut liquids) = ctx.world.get_mut::<LiquidModule>(e) {
                    liquids.remove(liquid, needed);
                }
                state.reload_counter += config.coolant_amount
                    * coolant_eff
                    * eff
                    * capacity
                    * config.coolant_multiplier
                    * config.ammo_reload_multiplier(ammo, ctx.content);
            }
        }
    }
}

/// `TurretBuild.charging()` (`Turret.java:718-720`): `queuedBullets > 0 &&
/// shoot.firstShotDelay > 0` — a shot is waiting out its pattern delay.
fn charging(state: &TurretState) -> bool {
    !state.pending.is_empty() && state.config.shoot.first_shot_delay > 0.0
}

/// `TurretBuild.shouldTurn()` (`Turret.java:672-674`): `moveWhileCharging ||
/// !charging()`; `lancer` sets `moveWhileCharging = false` (`Blocks.java:3594`).
fn should_turn(state: &TurretState) -> bool {
    state.config.move_while_charging || !charging(state)
}

/// Fires the queued shots whose `delay` elapsed this tick (`Time.run` task
/// pass). Order is insertion order, so the pattern's per-shot draws stay
/// deterministic.
fn update_pending_shots(ctx: &mut CombatCtx<'_>, e: Entity, state: &mut TurretState) {
    if state.pending.is_empty() {
        return;
    }
    let mut due: SmallVec<[PendingShot; 4]> = SmallVec::new();
    let mut remain: SmallVec<[PendingShot; 4]> = SmallVec::new();
    for mut shot in std::mem::take(&mut state.pending) {
        shot.delay -= 1.0;
        if shot.delay <= 0.0 {
            due.push(shot);
        } else {
            remain.push(shot);
        }
    }
    state.pending = remain;
    for shot in due {
        // `bullet(type, xOffset, yOffset, angle, mover)` with the barrel counter
        // restored (`Turret.java:788-795`).
        bullet(ctx, e, state, shot.bullet, &shot.shot, shot.barrel);
    }
}

/// `TurretBuild.updateShooting`.
fn update_shooting(ctx: &mut CombatCtx<'_>, e: Entity, state: &mut TurretState) {
    if state.reload_counter >= state.config.reload
        && !charging(state)
        && state.shoot_warmup >= state.config.min_warmup
    {
        if let Some(bullet) = peek_ammo_state(state, ctx.world, e) {
            shoot(ctx, e, state, bullet);
        }
        let reload = state.config.reload.max(0.0001);
        state.reload_counter %= reload;
    }
}

/// `if(checkSuppression()) efficiency = potentialEfficiency = 0`
/// (BuildTurret.java:132-134): a heal-suppressed build tower builds (and
/// moves) at speed 0 (`buildSpeedMultiplier(potentialEfficiency * timeScale)`).
pub fn build_turret_efficiency(world: &World, e: Entity, eff: f32) -> f32 {
    if crate::world::blocks::defense::projectors::is_heal_suppressed(world, e) {
        0.0
    } else {
        eff
    }
}

/// `BuildTurret.timerTarget` interval (`BuildTurret.java:32`).
const BUILD_TARGET_INTERVAL: f32 = 15.0;
/// `timer.get(timerTarget2, 30f)` (`BuildTurret.java:196`).
const BUILD_BREAK_CHECK_INTERVAL: f32 = 30.0;

/// `within(block.x * tilesize, block.y * tilesize, range)`
/// (`BuildTurret.java:161`): the plan tile centre against the tower's own
/// position.
fn build_plan_within(x: f32, y: f32, plan: &BuildPlan, range: f32) -> bool {
    let tile = crate::world::block::TILE_SIZE;
    let dx = plan.x as f32 * tile - x;
    let dy = plan.y as f32 * tile - y;
    dx * dx + dy * dy <= range * range
}

/// `Build.validPlace(type, team, x, y, rot)` (`Build.java:171-174`): the
/// 5-arg form defaults `checkVisible`/`checkCoreRadius` to true and still runs
/// the unit-overlap check (`validPlaceIgnoreUnits` + `checkNoUnitOverlap`).
fn build_valid_place(
    world: &World,
    content: &ContentRegistry,
    grid: &crate::world::WorldGrid,
    block: crate::content::BlockId,
    team: u8,
    rotation: u8,
    x: i32,
    y: i32,
) -> bool {
    let rules = world
        .get_resource::<crate::world::limits::BuildRules>()
        .cloned()
        .unwrap_or_default();
    let (Some(table), Some(counter)) = (
        world.get_resource::<crate::world::block::BlockTable>(),
        world.get_resource::<crate::world::limits::BlockCounter>(),
    ) else {
        return false;
    };
    crate::world::build::valid_place_query(
        content,
        table,
        &rules,
        counter,
        grid,
        Some(world),
        block,
        team,
        rotation,
        x,
        y,
        crate::world::build::PlaceFlags::PLAYER,
    )
}

/// `Build.validBreak(team, x, y)` (`Build.java:319`).
fn build_valid_break(
    world: &World,
    content: &ContentRegistry,
    grid: &crate::world::WorldGrid,
    team: u8,
    x: i32,
    y: i32,
) -> bool {
    let rules = world
        .get_resource::<crate::world::limits::BuildRules>()
        .cloned()
        .unwrap_or_default();
    crate::world::build::valid_break(content, grid, Some(world), &rules, team, x, y)
}

/// `state.rules.infiniteResources || team.rules().infiniteResources` plus
/// `state.rules.buildCostMultiplier` (`BuildTurret.java:166`).
fn build_economy(world: &World, team: u8) -> (bool, f32) {
    if let Some(rules) = crate::game::runtime::rules_of(world) {
        (
            rules.is_infinite_resources(team),
            rules.build_cost_multiplier,
        )
    } else {
        let rules = world
            .get_resource::<crate::world::limits::BuildRules>()
            .cloned()
            .unwrap_or_default();
        (
            rules.infinite_resources || rules.cheat,
            rules.build_cost_multiplier,
        )
    }
}

/// `team.items().has(btype.requirements, state.rules.buildCostMultiplier)`
/// (`BuildTurret.java:166`): `ItemModule.has` rounds each amount
/// (`Mathf.round(amount * scale)`), so a sub-0.5 requirement is free.
fn build_requirements_met(
    inventory: Option<&crate::world::modules::ItemModule>,
    content: &ContentRegistry,
    block: crate::content::BlockId,
    cost_multiplier: f32,
) -> bool {
    let Some(def) = content.block(block) else {
        return false;
    };
    let Some(inventory) = inventory else {
        // No team inventory (pre-campaign harness): nothing to fund.
        return true;
    };
    def.requirements.iter().all(|stack| {
        let need = (stack.amount as f32 * cost_multiplier).round() as i32;
        inventory.get(stack.item) >= need
    })
}

/// `BuilderComp.canBuild()` (`BuilderComp.java:46-48`): a build-capable unit
/// type with a positive build-speed multiplier.
fn unit_can_build(world: &World, content: &ContentRegistry, entity: Entity) -> bool {
    let build_speed = world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
        .map_or(0.0, |def| def.build_speed);
    let multiplier = world
        .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
        .map_or(1.0, |status| status.build_speed_multiplier);
    build_speed > 0.0 && multiplier > 0.0
}

/// `u.canBuild() && u.activelyBuilding()` (`BuildTurret.java:181-182`) for any
/// unit (`BuilderComp.activelyBuilding`, `BuilderComp.java:301-309`).
fn unit_actively_building(ctx: &mut CombatCtx<'_>, entity: Entity) -> bool {
    if !unit_can_build(ctx.world, ctx.content, entity) {
        return false;
    }
    let Some(pos) = ctx.world.get::<Pos>(entity).copied() else {
        return false;
    };
    crate::ai::types::player::player_actively_building(
        ctx.world,
        ctx.content,
        entity,
        (pos.x, pos.y),
    )
    .is_some()
}

/// `BuildTurret.java:181-192`: the first nearby unit that can build and is
/// actively building a `ConstructBuild` inside the tower's range. Java iterates
/// `Units.nearby` (quadtree order); the port uses `EntitySeq` order for
/// determinism.
fn scan_follow_unit(
    ctx: &mut CombatCtx<'_>,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
) -> Option<Entity> {
    let candidates =
        crate::entities::comp::unit::queries::in_radius(ctx.world, x, y, range, Some(team));
    for other in candidates {
        if !unit_actively_building(ctx, other) {
            continue;
        }
        let Some(plan) = ctx
            .world
            .get::<crate::sim::builder::BuilderPlans>(other)
            .and_then(|plans| plans.plans.front().cloned())
        else {
            continue;
        };
        // `world.build(plan.x, plan.y) instanceof ConstructBuild`.
        let Some(build) = ctx
            .grid
            .entity_at(crate::world::TilePos::new(plan.x, plan.y))
        else {
            continue;
        };
        if ctx.world.get::<ConstructState>(build).is_none() {
            continue;
        }
        let Some(bpos) = ctx.world.get::<Pos>(build).copied() else {
            continue;
        };
        let dx = bpos.x - x;
        let dy = bpos.y - y;
        if dx * dx + dy * dy <= range * range {
            return Some(other);
        }
    }
    None
}

/// `BuildPlan` copy of a unit plan (`unit.plans().addFirst(following.buildPlan())`,
/// `BuildTurret.java:189-190`).
fn unit_plan_to_build_plan(plan: &crate::sim::builder::UnitBuildPlan) -> BuildPlan {
    BuildPlan {
        x: i32::from(plan.x),
        y: i32::from(plan.y),
        rotation: plan.rotation,
        block: plan.block,
        config: plan.config.clone(),
        breaking: plan.breaking,
    }
}

/// `BuildTurret.java:196-208`: another player of the same team is breaking the
/// tile the tower wants to build on; the plan is dropped and the tile removed
/// from the team rebuild queue.
fn player_breaking_plan(ctx: &mut CombatCtx<'_>, team: u8, plan: &BuildPlan) -> bool {
    let mut players: SmallVec<[Entity; 4]> = SmallVec::new();
    if let Some(runtime) = ctx
        .world
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        && let Some(data) = runtime.session.teams.get_or_null(crate::ecs::TeamId(team))
    {
        players.extend(data.players.iter().copied());
    }
    if players.is_empty() {
        players.extend(
            ctx.world
                .iter_entities()
                .filter(|entity| {
                    entity.get::<crate::entities::comp::Player>().is_some()
                        && entity.get::<TeamComp>().map(|t| t.team) == Some(team)
                })
                .map(|entity| entity.id()),
        );
    }
    for player in players {
        if !unit_actively_building(ctx, player) {
            continue;
        }
        let Some(other) = ctx
            .world
            .get::<crate::sim::builder::BuilderPlans>(player)
            .and_then(|plans| plans.plans.front())
        else {
            continue;
        };
        if other.breaking && i32::from(other.x) == plan.x && i32::from(other.y) == plan.y {
            return true;
        }
    }
    false
}

/// `team.data().plans.remove(p -> p.x == req.x && p.y == req.y)`
/// (`BuildTurret.java:205`): arc `Queue.remove(pred)` drops the first match.
fn remove_team_plan(world: &mut World, team: u8, plan: &BuildPlan) {
    if let Some(mut runtime) = world.get_resource_mut::<crate::game::runtime::CampaignRuntime>() {
        let plans = &mut runtime.session.teams.get(crate::ecs::TeamId(team)).plans;
        if let Some(index) = plans
            .iter()
            .position(|entry| i32::from(entry.x) == plan.x && i32::from(entry.y) == plan.y)
        {
            plans.remove(index);
        }
    }
}

/// `BuildTurretBuild.updateTile` (`BuildTurret.java:132-260`): proxy-unit plan
/// selection (team rebuild queue + follower scan), per-tick plan validation,
/// the another-player-breaking drop, and `unit.updateBuildLogic()`'s
/// construct/deconstruct progress through plan 07's [`ConstructState`].
///
/// Placement finalisation stays with plan 07 (`BuildHarness::construct_tick`);
/// `Build.beginPlace`/`beginBreak` need the live `SimRuntime` and are not
/// reachable from the combat pass, so a plan whose tile holds no
/// `ConstructState` is discarded (Java would begin the construct/break there).
fn update_build(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    config: &TurretConfig,
    x: f32,
    y: f32,
    eff: f32,
) {
    let eff = build_turret_efficiency(ctx.world, e, eff);
    let team = ctx.team(e);
    let range = config.range;
    let (infinite, cost_multiplier) = build_economy(ctx.world, team);

    // Drop plans outside the grid (`Build.validPlace` rejects out-of-bounds).
    state
        .build_plans
        .retain(|plan| ctx.grid.tiles.in_bounds(plan.x, plan.y));

    // `TurretBuild.updateTile` (`Turret.java:535-537`) keeps the proxy on the
    // tower; `isControlled()` = the proxy carries the `Player` marker
    // (`Turret.java:448-450`).
    let controlled = state.block_unit.is_some_and(|unit| {
        ctx.world
            .get::<crate::entities::comp::Player>(unit)
            .is_some()
    });

    // `warmup = Mathf.lerpDelta(warmup, unit.activelyBuilding() ? efficiency :
    // 0f, 0.1f)` (`BuildTurret.java:141`) uses the queue from before this
    // tick's mutations. `activelyBuilding` is false while player-controlled.
    let was_building = !controlled
        && state
            .build_plans
            .first()
            .is_some_and(|plan| infinite || build_plan_within(x, y, plan, range));
    state.build_warmup = lerp_delta(
        state.build_warmup,
        if was_building { eff } else { 0.0 },
        0.1,
    );

    if !controlled {
        if let Some(following) = state.following {
            // `BuildTurret.java:146-155`: validate the follower and mirror its
            // head plan, or forget everything.
            let still_following = ctx.world.get_entity(following).is_ok()
                && unit_actively_building(ctx, following)
                && ctx
                    .world
                    .get::<crate::sim::builder::BuilderPlans>(following)
                    .and_then(|plans| plans.plans.front())
                    .is_some();
            if !still_following {
                state.following = None;
                state.build_plans.clear();
            } else {
                let plan = ctx
                    .world
                    .get::<crate::sim::builder::BuilderPlans>(following)
                    .and_then(|plans| plans.plans.front())
                    .map(unit_plan_to_build_plan);
                state.build_plans.clear();
                if let Some(plan) = plan {
                    state.build_plans.push(plan);
                }
                state.last_plan = None;
            }
        } else if state.build_plans.is_empty() {
            // `unit.buildPlan() == null && timer(timerTarget, targetInterval)`
            // (`BuildTurret.java:156-194`).
            state.build_target_timer -= 1.0;
            if state.build_target_timer <= 0.0 {
                state.build_target_timer = BUILD_TARGET_INTERVAL;
                // (1) Team rebuild queue: first `!removed` entry that is in
                // range and legal + affordable; rotate it to the tail.
                let queue = ctx
                    .world
                    .get_resource::<crate::game::runtime::CampaignRuntime>()
                    .and_then(|runtime| {
                        runtime
                            .session
                            .teams
                            .get_or_null(crate::ecs::TeamId(team))
                            .map(|data| data.plans.clone())
                    });
                let inventory = ctx
                    .world
                    .get_resource::<crate::game::runtime::CampaignRuntime>()
                    .and_then(|runtime| {
                        runtime
                            .session
                            .teams
                            .team_items(crate::ecs::TeamId(team))
                            .cloned()
                    });
                let selected = queue.as_ref().and_then(|queue| {
                    crate::world::blocks::defense::build_turret_plans::scan_team_rebuild_queue(
                        queue,
                        |plan| build_plan_within(x, y, plan, range),
                        |plan| {
                            build_valid_place(
                                ctx.world,
                                ctx.content,
                                ctx.grid,
                                plan.block,
                                team,
                                plan.rotation,
                                plan.x,
                                plan.y,
                            ) && (infinite
                                || build_requirements_met(
                                    inventory.as_ref(),
                                    ctx.content,
                                    plan.block,
                                    cost_multiplier,
                                ))
                        },
                    )
                });
                if let Some((index, plan)) = selected {
                    if let Some(mut runtime) = ctx
                        .world
                        .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
                        && let Some(entry) =
                            crate::world::blocks::defense::build_turret_plans::rotate_plan_to_tail(
                                &mut runtime.session.teams.get(crate::ecs::TeamId(team)).plans,
                                index,
                            )
                    {
                        state.last_plan = Some(entry);
                    }
                    state.build_plans.push(plan);
                }
                // (2) `BuildTurret.java:178-194`: still not building, mimic a
                // nearby actively-building unit.
                if state.build_plans.is_empty() {
                    state.following = scan_follow_unit(ctx, team, x, y, range);
                }
            }
        } else if let Some(head) = state.build_plans.first().cloned() {
            // `BuildTurret.java:195-226`: validate the current plan.
            // Clear the break plan if another player is breaking the same tile
            // (`timer.get(timerTarget2, 30f)`).
            state.build_break_timer -= 1.0;
            if !head.breaking && state.build_break_timer <= 0.0 {
                state.build_break_timer = BUILD_BREAK_CHECK_INTERVAL;
                if player_breaking_plan(ctx, team, &head) {
                    state.build_plans.remove(0);
                    remove_team_plan(ctx.world, team, &head);
                    // Java `return`s from `updateTile` here, skipping the
                    // build-logic pass for this tick.
                    state.build_target = None;
                    state.is_shooting = false;
                    state.was_shooting = false;
                    state.any = false;
                    return;
                }
            }

            let last_removed = state.last_plan.as_ref().is_some_and(|plan| plan.removed);
            let construct_ok = ctx
                .grid
                .entity_at(crate::world::TilePos::new(head.x as i16, head.y as i16))
                .and_then(|build| {
                    ctx.world
                        .get::<ConstructState>(build)
                        .map(|cons| cons.current)
                })
                == Some(head.block);
            let valid = !last_removed
                && (construct_ok
                    || if head.breaking {
                        build_valid_break(ctx.world, ctx.content, ctx.grid, team, head.x, head.y)
                    } else {
                        build_valid_place(
                            ctx.world,
                            ctx.content,
                            ctx.grid,
                            head.block,
                            team,
                            head.rotation,
                            head.x,
                            head.y,
                        )
                    });
            if !valid {
                // Discard the invalid request (`BuildTurret.java:220-224`).
                state.build_plans.remove(0);
                state.last_plan = None;
            }
        }
    } else {
        // Player-controlled: forget everything (`BuildTurret.java:228-231`).
        state.following = None;
        state.last_plan = None;
    }

    // `unit.plans().remove(b -> b.build() == this)` (`BuildTurret.java:234`):
    // never build over the tower itself.
    state.build_plans.retain(|plan| {
        ctx.grid
            .entity_at(crate::world::TilePos::new(plan.x as i16, plan.y as i16))
            != Some(e)
    });

    // `unit.updateBuildLogic()`: drive the head plan's `ConstructBuild`.
    let mut actively_building = false;
    if let Some(head) = state.build_plans.first().cloned() {
        let target = ctx.grid.tiles.get(head.x, head.y).build;
        if let Some(entity) = target
            && ctx.world.get::<ConstructState>(entity).is_some()
        {
            let Some(mut construct) = ctx.world.get::<ConstructState>(entity).cloned() else {
                return;
            };
            construct.last_builder = Some(e);
            let refund = ctx
                .world
                .get_resource::<crate::world::limits::BuildRules>()
                .map(|rules| rules.deconstruct_refund_multiplier)
                .unwrap_or(0.5);
            let done = if head.breaking {
                // `ConstructBuild.deconstruct` (`BuilderComp.java:244-246`):
                // plan 07's deconstruct path, never construct.
                if construct.active_deconstruct {
                    construct
                        .advance_deconstruct(config.build_speed * eff, refund)
                        .0
                } else {
                    true
                }
            } else {
                construct.advance_construct(config.build_speed * eff)
            };
            ctx.world.entity_mut(entity).insert(construct);
            actively_building = true;
            if done {
                state.build_plans.remove(0);
                state.last_plan = None;
            }
        } else {
            // No `ConstructBuild` on the tile: `Build.beginPlace`/`beginBreak`
            // (plan 07's `SimRuntime`) owns that swap; discard the stale plan.
            state.build_plans.remove(0);
            state.last_plan = None;
        }
    }

    state.build_target = state.build_plans.first().map(|plan| (plan.x, plan.y));
    state.is_shooting = actively_building;
    state.was_shooting = actively_building;
    state.any = actively_building;
}

/// `TurretBuild.shoot`.
fn shoot(ctx: &mut CombatCtx<'_>, e: Entity, state: &mut TurretState, bullet_type: BulletId) {
    let config = state.config.clone();
    let (x, y) = position(ctx.world, e).unwrap_or((0.0, 0.0));
    let rotation = state.rotation;

    // `Turret.shoot`: `chargeSound.at(bulletX, bulletY, random(pitch))` when
    // `shoot.firstShotDelay > 0` (`Turret.java:756-758`).
    if config.shoot.first_shot_delay > 0.0 {
        let cx = x + trnsx(rotation - 90.0, config.shoot_x, config.shoot_y);
        let cy = y + trnsy(rotation - 90.0, config.shoot_x, config.shoot_y);
        let pitch = {
            let mut rng = crate::audio::sim::FxAudioRng { rng: &mut *ctx.rng };
            config.sound_pitch_min
                + rng.next_f32() * (config.sound_pitch_max - config.sound_pitch_min)
        };
        crate::audio::sim::emit_turret_shoot(ctx.audio, config.charge_sound, cx, cy, 1.0, pitch);
    }

    let mut buffer = ShotBuffer::default();
    let mut barrel = state.barrel_counter;
    pattern::emit(
        &config.shoot,
        state.total_shots,
        &mut buffer,
        &mut barrel,
        ctx.rng,
    );
    state.barrel_counter = barrel;

    // `Turret.shoot`'s handler (`Turret.java:763-778`): `if(delay > 0f)
    // Time.run(delay, …) else bullet(…)`. The pending queue is `queuedBullets`:
    // an immediate shot leaves it at once, a delayed one stays until its tick.
    for shot in &buffer.shots {
        if shot.delay > 0.0 {
            state.pending.push(PendingShot {
                bullet: bullet_type,
                shot: *shot,
                delay: shot.delay,
                barrel: shot.barrel,
            });
        } else {
            bullet(ctx, e, state, bullet_type, shot, shot.barrel);
        }
    }

    if config.consume_ammo_once {
        let _ = use_ammo(state, ctx.world, e);
    }
}

/// `TurretBuild.bullet` (`Turret.java:785-828`): spawns one pattern shot from
/// the turret's *current* pose — a delayed shot recomputes the muzzle position,
/// angle, spread, lifetime and `targetPos` at fire time and restores `barrel`
/// for the recoil index.
fn bullet(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    bullet_type: BulletId,
    shot: &PatternShot,
    barrel: i32,
) {
    let config = state.config.clone();
    // `if(dead || (!consumeAmmoOnce && !hasAmmo())) return;`
    if !config.consume_ammo_once && !has_ammo_state(state, ctx.world, e) {
        return;
    }
    let (x, y) = position(ctx.world, e).unwrap_or((0.0, 0.0));
    let team = ctx.team(e);
    let rotation = state.rotation;

    let def_inaccuracy = ctx
        .content
        .bullet(bullet_type)
        .map(|def| def.inaccuracy)
        .unwrap_or(0.0);
    let scale_life = ctx
        .content
        .bullet(bullet_type)
        .map(|def| def.scale_life)
        .unwrap_or(false);
    let bullet_range = ctx
        .content
        .bullet(bullet_type)
        .map(|def| def.range)
        .unwrap_or(0.0);
    // `Turret.bullet`: `(type.shootSound != none ? type.shootSound : shootSound)`
    // played per spawned bullet (`Turret.java:804`); the pitch is
    // `Mathf.random(soundPitchMin, soundPitchMax)` drawn from the audio-only Fx
    // stream, so the checksum is unaffected.
    let bullet_sound = ctx
        .content
        .bullet(bullet_type)
        .map(|def| def.shoot_sound)
        .unwrap_or(SoundId::NONE);
    let shoot_sound = if !bullet_sound.is_none_or_unset() {
        bullet_sound
    } else {
        config.shoot_sound
    };

    let x_spread = ctx.rng.range(
        crate::determinism::RngStream::Sim,
        -config.x_rand,
        config.x_rand,
    );
    let bx = x + trnsx(
        rotation - 90.0,
        config.shoot_x + shot.x + x_spread,
        config.shoot_y + shot.y,
    );
    let by = y + trnsy(
        rotation - 90.0,
        config.shoot_x + shot.x + x_spread,
        config.shoot_y + shot.y,
    );
    let angle = rotation
        + shot.rotation
        + ctx.rng.range(
            crate::determinism::RngStream::Sim,
            -(config.inaccuracy + def_inaccuracy),
            config.inaccuracy + def_inaccuracy,
        );
    let base_life = (1.0 - config.life_rnd)
        + ctx
            .rng
            .range(crate::determinism::RngStream::Sim, 0.0, config.life_rnd)
        + config.extra_life;
    let life_scl = if scale_life && bullet_range > 0.0 {
        let dst = ((state.target_pos.0 - bx).powi(2) + (state.target_pos.1 - by).powi(2)).sqrt();
        // `Turret.bullet` (`Turret.java:797`): `clamp((baseLife +
        // scaleLifetimeOffset) * dst / type.range, minRange()/type.range,
        // range()/type.range)`.
        let min_range = min_range_of(state, ctx.world, e);
        let max_range = turret_range_of(state, ctx.world, e);
        ((base_life + config.scale_lifetime_offset) * dst / bullet_range)
            .clamp(min_range / bullet_range, max_range / bullet_range)
    } else {
        base_life
    };
    let velocity_scl = (1.0 - config.velocity_rnd)
        + ctx
            .rng
            .range(crate::determinism::RngStream::Sim, 0.0, config.velocity_rnd)
        + config.extra_velocity;
    let spawn = BulletSpawn {
        def: bullet_type,
        owner: Some(e),
        shooter: Some(e),
        team,
        x: bx,
        y: by,
        angle,
        velocity_scl,
        lifetime_scl: life_scl,
        aim_x: state.target_pos.0,
        aim_y: state.target_pos.1,
        mover: shot.mover,
        ..BulletSpawn::default()
    };
    let _ = ctx.spawn(&spawn);
    let pitch = {
        let mut rng = crate::audio::sim::FxAudioRng { rng: &mut *ctx.rng };
        config.sound_pitch_min + rng.next_f32() * (config.sound_pitch_max - config.sound_pitch_min)
    };
    crate::audio::sim::emit_turret_shoot(
        ctx.audio,
        shoot_sound,
        bx,
        by,
        config.shoot_sound_volume,
        pitch,
    );

    state.total_shots += 1;
    if !config.consume_ammo_once {
        let _ = use_ammo(state, ctx.world, e);
    }
    state.cur_recoil = 1.0;
    if config.recoils > 0 {
        let count = config.recoils as usize;
        if state.cur_recoils.len() < count {
            state.cur_recoils.resize(count, 0.0);
        }
        state.cur_recoils[barrel.rem_euclid(count as i32) as usize] = 1.0;
    }
    state.heat = 1.0;
}

/// `LiquidTurretBuild.findTarget` fire scan (`LiquidTurret.java:93-115`).
///
/// Scans the `range / tilesize` square around the turret, keeps the nearest
/// fire **not** on another team's building (`other.build == null ||
/// other.team() == team`), and skips the turret's own tile (`other.build !=
/// this`).
fn nearest_fire(
    ctx: &CombatCtx<'_>,
    e: Entity,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
) -> Option<Entity> {
    let ts = crate::config::TILESIZE as f32;
    let tx = crate::world::WorldGrid::to_tile(x);
    let ty = crate::world::WorldGrid::to_tile(y);
    let tr = (range / ts) as i32;
    let mut best: Option<(f32, Entity)> = None;
    for dy in -tr..=tr {
        for dx in -tr..=tr {
            let (fx, fy) = (tx + dx, ty + dy);
            if !ctx.grid.tiles.in_bounds(fx, fy) {
                continue;
            }
            let tile = ctx.grid.tiles.get(fx, fy);
            // `other.build != this`.
            if tile.build == Some(e) {
                continue;
            }
            // `other.build == null || other.team() == team`.
            if let Some(build) = tile.build
                && ctx
                    .world
                    .get::<TeamComp>(build)
                    .is_some_and(|comp| comp.team != team)
            {
                continue;
            }
            let Some(fire) = crate::combat::fires::find_at(ctx.world, fx as i16, fy as i16) else {
                continue;
            };
            let Some(pos) = ctx.world.get::<Pos>(fire) else {
                continue;
            };
            let d2 = (pos.x - x) * (pos.x - x) + (pos.y - y) * (pos.y - y);
            if d2 > range * range {
                continue;
            }
            if best.is_none_or(|(bd, _)| d2 < bd) {
                best = Some((d2, fire));
            }
        }
    }
    best.map(|(_, fire)| fire)
}

/// `TurretBuild.findTarget` / `findEnemy` (`Turret.java:631-677`) via plan 11's
/// [`TargetQueries`] (plan 10 §3.8).
///
/// Two passes: the firing range first, then `trackingRange()` when it differs
/// (a null first pass only), and finally a damaged ally when the turret heals.
#[allow(clippy::too_many_arguments)]
fn find_target(
    ctx: &CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    team: u8,
    x: f32,
    y: f32,
    targets: &TargetQueries,
) {
    let config = &state.config;
    // The dispatched state is out of the world during this pass, so the ranges
    // come from the state handle (`turret_range_of`).
    let range = turret_range_of(state, ctx.world, e);

    // `LiquidTurretBuild.findTarget` (`LiquidTurret.java:91-119`): a liquid
    // turret whose current ammo liquid can extinguish targets the nearest fire
    // in its range square instead of running the standard targeting pass.
    if config.kind == TurretKind::Liquid
        && let Some(liquids) = ctx.world.get::<LiquidModule>(e)
        && ctx
            .content
            .liquid(liquids.current_liquid())
            .is_some_and(|liquid| liquid.can_extinguish())
        && let Some(fire) = nearest_fire(ctx, e, team, x, y, range)
    {
        state.target = Some(fire);
        return;
    }
    let track_range = tracking_range_of(state, ctx.world, e);
    let sort = ammo_unit_sort(state, ctx.world, e, ctx.content);
    let ammo = peek_ammo_state(state, ctx.world, e).and_then(|id| ctx.content.bullet(id));
    let missiles = ammo.is_none_or(|def| def.target_missiles);
    let buildings =
        config.target_ground && config.target_blocks && ammo.is_none_or(|def| def.target_blocks);

    let find = |query_range: f32| -> Option<Entity> {
        if config.target_air && !config.target_ground {
            // `Units.bestEnemy` with `!e.isGrounded()` (`Turret.java:645`): the
            // predicate is an ACCEPT filter and Java's `isGrounded()` is
            // `elevation < 0.001` (`UnitComp.java:80-82`), so air-only turrets
            // keep every non-grounded unit — not just `isFlying()`
            // (`elevation >= 0.09`).
            targets.best_enemy(
                ctx.world,
                ctx.content,
                team,
                x,
                y,
                query_range,
                sort,
                &mut |world, _content, unit| !unit_is_grounded(world, unit),
            )
        } else {
            // `Units.bestTarget` (`Turret.java:647-651`): the unit gate is
            // `(e.isGrounded() || targetAir) && (!e.isGrounded() || targetGround)`
            // — grounded needs `targetGround`, non-grounded needs `targetAir` —
            // and it skips missiles unless the ammo allows them.
            targets.best_target(
                ctx.world,
                ctx.content,
                team,
                x,
                y,
                query_range,
                sort,
                &mut |world, _content, unit| {
                    let grounded = unit_is_grounded(world, unit);
                    (grounded || config.target_air)
                        && (!grounded || config.target_ground)
                        && (missiles
                            || world
                                .get::<crate::entities::comp::unit::comp::TimedKillComp>(unit)
                                .is_none())
                },
                buildings,
                config.target_under_blocks,
            )
        }
    };

    let mut target = find(range);
    // `findTarget`: a different tracking range gets a second chance only when
    // nothing was found in the firing range (`Turret.java:655-658`;
    // `Mathf.equal`).
    if (track_range - range).abs() > f32::EPSILON && target.is_none() {
        target = find(track_range);
    }
    if target.is_none() && can_heal(state, ctx.world, ctx.content, e) {
        target = targets.find_ally_tile(ctx.world, team, x, y, range, Some(e));
    }
    state.target = target;
    if let Some(target) = target {
        state.target_pos = position(ctx.world, target).unwrap_or((x, y));
    }
}

/// `TurretBuild.canHeal` (`Turret.java:634-636`): `targetHealing && hasAmmo()
/// && peekAmmo().collidesTeam && peekAmmo().heals()`.
fn can_heal(state: &TurretState, world: &World, content: &ContentRegistry, e: Entity) -> bool {
    if !state.config.target_healing || !has_ammo_state(state, world, e) {
        return false;
    }
    peek_ammo_state(state, world, e)
        .and_then(|id| content.bullet(id))
        .is_some_and(|def| def.collides_team && def.heals())
}

/// `TurretBuild.minRange()` (`Turret.java:340-344`): `minRange +
/// peekAmmo().minRangeChange`.
pub fn min_range_of(state: &TurretState, world: &World, e: Entity) -> f32 {
    state.config.min_range + peek_ammo_min_range_change_of(state, world, e)
}

/// `TurretBuild.trackingRange()` (`Turret.java:355-356`): the init-maxed
/// tracking range shifted by the ammo's `rangeChange`.
pub fn tracking_range_of(state: &TurretState, world: &World, e: Entity) -> f32 {
    state.config.tracking_range + peek_ammo_range_change_of(state, world, e)
}

/// `Turret.findEnemy`'s sort (`Turret.java:643`): `ammo.unitSort != closest ?
/// ammo.unitSort : unitSort`.
fn ammo_unit_sort(
    state: &TurretState,
    world: &World,
    e: Entity,
    content: &ContentRegistry,
) -> UnitSort {
    peek_ammo_state(state, world, e)
        .and_then(|id| content.bullet(id))
        .map(|def| def.unit_sort)
        .filter(|sort| *sort != UnitSort::Closest)
        .unwrap_or(state.config.unit_sort)
}

/// `Turret.targetPosition` (`Turret.java:459-479`): the per-tick aim point for
/// the current ammo, leading the target through `Predict.intercept`.
fn target_position(
    ctx: &CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    x: f32,
    y: f32,
    target: Entity,
) {
    if !has_ammo_state(state, ctx.world, e) {
        return;
    }
    let Some((tx, ty)) = position(ctx.world, target) else {
        return;
    };
    let speed = peek_ammo_state(state, ctx.world, e)
        .and_then(|id| ctx.content.bullet(id))
        .map(|def| def.speed)
        .unwrap_or(0.0);
    // Target per-tick displacement (`Hitboxc.deltaX/deltaY`); buildings carry
    // no `Vel` and are static here.
    let delta = ctx
        .world
        .get::<crate::entities::comp::Vel>(target)
        .copied()
        .unwrap_or(crate::entities::comp::Vel { x: 0.0, y: 0.0 });
    // `when delay is accurate, assume unit has moved by chargeTime already`.
    let config = &state.config;
    let offset = if config.accurate_delay && !config.move_while_charging {
        (
            delta.x * config.shoot.first_shot_delay,
            delta.y * config.shoot.first_shot_delay,
        )
    } else {
        (0.0, 0.0)
    };
    let aim = if config.predict_target && speed >= 0.01 {
        // `Predict.intercept(this, pos, offset.x, offset.y, bullet.speed)`:
        // the turret is not a `Hitboxc`, so the source velocity is zero.
        crate::combat::targeting::intercept_positions(
            (x, y),
            (0.0, 0.0),
            (tx + offset.0, ty + offset.1),
            (delta.x, delta.y),
            true,
            speed,
        )
    } else {
        (tx, ty)
    };
    // `if(targetPos.isZero()) targetPos.set(pos)`.
    state.target_pos = if aim.0 == 0.0 && aim.1 == 0.0 {
        (tx, ty)
    } else {
        aim
    };
}

/// `Turret.updateTile`'s `within(target, range() + target.hitSize()/1.9f)`
/// (`Turret.java:587`): the fire gate uses the TARGET'S position, not the
/// intercept point. Buildings use `BuildingComp.hitSize() = size * tilesize`
/// (`BuildingComp.java:2054`).
fn within_target_range(ctx: &CombatCtx<'_>, target: Entity, x: f32, y: f32, range: f32) -> bool {
    let Some((tx, ty)) = position(ctx.world, target) else {
        return false;
    };
    let hit_size = ctx
        .world
        .get::<crate::entities::comp::unit::comp::HitboxComp>(target)
        .map(|hitbox| hitbox.hit_size)
        .or_else(|| {
            ctx.world
                .get::<crate::entities::comp::Building>(target)
                .and_then(|building| ctx.content.block(building.block))
                .map(|def| def.size as f32 * crate::world::block::TILE_SIZE)
        })
        .unwrap_or(0.0);
    let limit = range + hit_size / 1.9;
    let dx = tx - x;
    let dy = ty - y;
    dx * dx + dy * dy <= limit * limit
}

/// `TurretBuild.validateTarget`.
fn validate_target(ctx: &CombatCtx<'_>, _e: Entity, state: &TurretState) -> bool {
    let Some(target) = state.target else {
        return false;
    };
    if ctx.world.get_entity(target).is_err() {
        return false;
    }
    // `Units.invalidateTarget` (`Units.java:151-157`): the dead-target rule is
    // `target instanceof Healthc h && !h.isValid()` — entities without health
    // (a `Fire` the liquid turret is extinguishing) stay valid.
    if ctx.world.get::<Health>(target).is_some_and(|h| h.dead) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    /// Sets a test unit's `elevation` plus the flying-type mirror
    /// (`UnitType.java:566`: `unit.elevation = flying ? 1f : 0`), so targeting
    /// fixtures can occupy Java's elevation bands (`UnitComp.java:80-86`).
    pub(super) fn set_unit_elevation(
        harness: &mut CombatHarness,
        unit: Entity,
        elevation: f32,
        physics_flying: bool,
    ) {
        use crate::entities::comp::unit::comp::{PhysicsComp, UnitCore};
        let world = &mut harness.build.world;
        world.entity_mut(unit).insert(UnitCore {
            elevation,
            ..UnitCore::new(0.0)
        });
        world.entity_mut(unit).insert(PhysicsComp {
            flying: physics_flying,
            ..PhysicsComp::default()
        });
    }

    fn turret_at(harness: &mut CombatHarness, name: &str) -> (Entity, Entity) {
        let wall = harness.content().block_id("copper-wall").expect("wall");
        // Target at tile 10,8; turret at tile 4,8.
        assert!(harness.place(10, 8, wall, 0, true));
        let target = harness.build_at(10, 8).expect("target");
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret(name, tx, ty, 1)
            .unwrap_or_else(|| panic!("turret `{name}`"));
        (turret, target)
    }

    #[test]
    fn duo_graphite_range_is_176() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let (turret, _) = turret_at(&mut harness, "duo");
        assert_eq!(
            turret_range(&harness.build.world, turret),
            160.0,
            "base range"
        );
        let graphite = harness.content().item_id("graphite").expect("graphite");
        handle_item(&mut harness.build.world, turret, graphite);
        assert_eq!(
            turret_range(&harness.build.world, turret),
            176.0,
            "graphite rangeChange 16"
        );
        let silicon = harness.content().item_id("silicon").expect("silicon");
        handle_item(&mut harness.build.world, turret, silicon);
        // `peekAmmo()` is the last stack; silicon has no rangeChange.
        assert_eq!(turret_range(&harness.build.world, turret), 160.0);
    }

    #[test]
    fn item_ammo_acceptance_and_merge() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let (turret, _) = turret_at(&mut harness, "test-item");
        let copper = harness.content().item_id("copper").expect("copper");
        let lead = harness.content().item_id("lead").expect("lead");
        assert!(accept_item(&harness.build.world, turret, copper));
        assert!(!accept_item(&harness.build.world, turret, lead));
        handle_item(&mut harness.build.world, turret, copper);
        handle_item(&mut harness.build.world, turret, copper);
        let state = harness
            .build
            .world
            .get::<TurretState>(turret)
            .expect("state");
        assert_eq!(state.ammo.len(), 1, "same item merges into one entry");
        assert_eq!(state.ammo[0].amount, 4, "duo copper ammoMultiplier 2 x2");
        assert!(has_ammo(&harness.build.world, turret));
    }

    #[test]
    fn ammo_consumption_decrements_total() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let (turret, target) = turret_at(&mut harness, "test-item");
        let copper = harness.content().item_id("copper").expect("copper");
        for _ in 0..5 {
            handle_item(&mut harness.build.world, turret, copper);
        }
        let before = harness
            .build
            .world
            .get::<TurretState>(turret)
            .unwrap()
            .total_ammo;
        let hp_before = harness.building_health_at(10, 8);
        for _ in 0..40 {
            harness.tick();
        }
        let state = harness.build.world.get::<TurretState>(turret).unwrap();
        assert!(state.total_ammo < before, "ammo consumed");
        assert!(state.total_shots > 0, "turret fired");
        assert!(
            harness.building_health_at(10, 8) < hp_before,
            "target damaged"
        );
        let _ = target;
    }

    #[test]
    fn coolant_speeds_reload() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret("test-item", tx, ty, 1)
            .expect("turret");
        let water = harness.content().liquid_id("water").expect("water");
        let copper = harness.content().item_id("copper").expect("copper");
        handle_item(&mut harness.build.world, turret, copper);
        // No target: `handleReload` still accumulates (reload is independent).
        for _ in 0..10 {
            harness.tick();
        }
        let without = harness
            .build
            .world
            .get::<TurretState>(turret)
            .unwrap()
            .reload_counter;
        assert!(without > 0.0, "reload accumulated without a target");
        harness
            .build
            .world
            .get_mut::<TurretState>(turret)
            .unwrap()
            .reload_counter = 0.0;
        harness
            .build
            .world
            .get_mut::<LiquidModule>(turret)
            .unwrap()
            .add(water, 20.0, 60.0);
        for _ in 0..10 {
            harness.tick();
        }
        let with = harness
            .build
            .world
            .get::<TurretState>(turret)
            .unwrap()
            .reload_counter;
        assert!(with > without, "coolant boosted reload: {with} > {without}");
    }

    #[test]
    fn power_gate_blocks_until_powered() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let (turret, _) = turret_at(&mut harness, "test-power");
        harness
            .build
            .world
            .get_mut::<PowerModule>(turret)
            .unwrap()
            .status = 0.0;
        for _ in 0..40 {
            harness.tick();
        }
        assert_eq!(
            harness
                .build
                .world
                .get::<TurretState>(turret)
                .unwrap()
                .total_shots,
            0,
            "no shots without power"
        );
        harness
            .build
            .world
            .get_mut::<PowerModule>(turret)
            .unwrap()
            .status = 1.0;
        for _ in 0..40 {
            harness.tick();
        }
        assert!(
            harness
                .build
                .world
                .get::<TurretState>(turret)
                .unwrap()
                .total_shots
                > 0,
            "shoots once powered"
        );
    }

    #[test]
    fn cone_gate_delays_first_shot_until_aligned() {
        let mut harness = CombatHarness::new(48, 16, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 8, wall, 0, true));
        // Turret starts at rotation 90 (up); target is to the right (angle 0).
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret("test-item", tx, ty, 1)
            .expect("turret");
        let copper = harness.content().item_id("copper").expect("copper");
        for _ in 0..10 {
            handle_item(&mut harness.build.world, turret, copper);
        }
        // One tick: reload not full, so no shot regardless; step until aligned.
        let mut fired_tick = None;
        for tick in 1..=30 {
            harness.tick();
            if harness
                .build
                .world
                .get::<TurretState>(turret)
                .unwrap()
                .total_shots
                > 0
            {
                fired_tick = Some(tick);
                break;
            }
        }
        assert!(fired_tick.is_some(), "turret aligned and fired");
    }

    #[test]
    fn activation_timer_blocks_shooting() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let (turret, _) = turret_at(&mut harness, "test-item");
        harness
            .build
            .world
            .get_mut::<TurretState>(turret)
            .unwrap()
            .activation_timer = 5.0;
        let copper = harness.content().item_id("copper").expect("copper");
        for _ in 0..5 {
            handle_item(&mut harness.build.world, turret, copper);
        }
        for _ in 0..3 {
            harness.tick();
        }
        assert_eq!(
            harness
                .build
                .world
                .get::<TurretState>(turret)
                .unwrap()
                .total_shots,
            0,
            "activation timer blocks fire"
        );
    }

    #[test]
    fn payload_ammo_accept_and_consume() {
        let mut harness = CombatHarness::new(48, 16, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        // Target at tile 10,8; turret at tile 4,8.
        assert!(harness.place(10, 8, wall, 0, true));
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret("test-payload", tx, ty, 1)
            .expect("payload turret");
        let payload = crate::world::PayloadRef {
            entity: None,
            content: wall.raw(),
            is_block: true,
        };
        assert!(accept_payload(&harness.build.world, turret, payload));
        for _ in 0..3 {
            handle_payload(&mut harness.build.world, turret, payload);
        }
        // A fourth payload is rejected at the capacity of 3.
        assert!(!accept_payload(&harness.build.world, turret, payload));
        let hp_before = harness.building_health_at(10, 8);
        for _ in 0..80 {
            harness.tick();
        }
        let state = harness.build.world.get::<TurretState>(turret).unwrap();
        assert!(state.total_shots > 0, "payload turret fired");
        assert!(state.total_ammo < 3, "payload ammo consumed");
        assert!(
            harness.building_health_at(10, 8) < hp_before,
            "payload bullet damaged target"
        );
    }

    #[test]
    fn build_turret_advances_construction() {
        let mut harness = CombatHarness::new(48, 16, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        // Non-instant placement creates a plan-07 construction at the tile.
        assert!(harness.place(10, 8, wall, 0, false));
        let construct = harness.build_at(10, 8).expect("construct entity");
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret("build-tower", tx, ty, 1)
            .expect("build tower");
        assert!(add_build_plan(
            &mut harness.build.world,
            turret,
            BuildPlan::place(10, 8, 0, wall)
        ));
        for _ in 0..20 {
            harness.tick();
        }
        let progress = harness
            .build
            .world
            .get::<ConstructState>(construct)
            .expect("construct state")
            .progress;
        assert!(progress > 0.0, "build turret advanced the construct");
        let state = harness.build.world.get::<TurretState>(turret).unwrap();
        assert!(state.build_warmup > 0.0, "build warmup ramped");
    }

    /// L06 stretch: a heal-suppressed build tower builds at efficiency 0
    /// (`checkSuppression`, BuildTurret.java:132-134; `update_build` routes its
    /// efficiency through `build_turret_efficiency`).
    #[test]
    fn suppressed_build_turret_has_zero_efficiency() {
        let mut harness = CombatHarness::new(48, 16, 7);
        let tower = harness.content().block_id("build-tower").expect("tower");
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret("build-tower", tx, ty, 1)
            .expect("build tower");
        // `checkSuppression` reads the building's block (`suppressable`) and
        // `healSuppressionTime`.
        harness
            .build
            .world
            .entity_mut(turret)
            .insert(crate::entities::comp::Building::new(
                crate::world::TilePos::new(4, 8),
                tower,
                0,
            ));
        assert_eq!(
            build_turret_efficiency(&harness.build.world, turret, 1.0),
            1.0
        );
        crate::world::blocks::defense::projectors::apply_suppression(
            &mut harness.build.world,
            2,
            tx,
            ty,
            40.0,
            600.0,
        );
        assert_eq!(
            build_turret_efficiency(&harness.build.world, turret, 1.0),
            0.0,
            "a suppressed build turret builds nothing"
        );
        // Own-team suppression never applies (`Damage.applySuppression`).
        let mut harness = CombatHarness::new(48, 16, 7);
        let turret = harness
            .spawn_test_turret("build-tower", tx, ty, 1)
            .expect("build tower");
        harness
            .build
            .world
            .entity_mut(turret)
            .insert(crate::entities::comp::Building::new(
                crate::world::TilePos::new(4, 8),
                tower,
                0,
            ));
        crate::world::blocks::defense::projectors::apply_suppression(
            &mut harness.build.world,
            1,
            tx,
            ty,
            40.0,
            600.0,
        );
        assert_eq!(
            build_turret_efficiency(&harness.build.world, turret, 1.0),
            1.0
        );
    }

    #[test]
    fn turret_predict_flags_match_blocks_java() {
        let harness = CombatHarness::new(8, 8, 1);
        let cfg = |name: &str| {
            config_for(harness.content(), name, harness.names_map())
                .unwrap_or_else(|| panic!("config `{name}`"))
        };
        // `Turret.java:84,86,111`: class defaults (accurateDelay, moveWhileCharging,
        // predictTarget all true unless a block overrides them).
        let duo = cfg("duo");
        assert!(duo.predict_target);
        assert!(duo.accurate_delay);
        assert!(duo.move_while_charging);
        // `Blocks.java:3594-3595`: lancer sets moveWhileCharging/accurateDelay false.
        let lancer = cfg("lancer");
        assert!(!lancer.accurate_delay);
        assert!(!lancer.move_while_charging);
        assert!(lancer.predict_target);
        // `Blocks.java:5248`: scathe sets predictTarget false.
        let scathe = cfg("scathe");
        assert!(!scathe.predict_target);
        assert!(scathe.accurate_delay);
        assert!(scathe.move_while_charging);
    }

    #[test]
    fn turret_sound_table_matches_blocks_java() {
        let harness = CombatHarness::new(8, 8, 1);
        let cfg = |name: &str| {
            config_for(harness.content(), name, harness.names_map())
                .unwrap_or_else(|| panic!("config `{name}`"))
        };
        // `Blocks.java:3540`: hail carries shootArtillerySmall at volume 1.
        let hail = cfg("hail");
        assert_eq!(hail.shoot_sound, SoundId::SHOOT_ARTILLERY_SMALL);
        assert_eq!(hail.shoot_sound_volume, 1.0);
        // `Blocks.java:3952-3953`: fuse volume 0.9.
        assert_eq!(cfg("fuse").shoot_sound_volume, 0.9);
        // `Blocks.java:3596-3598`: lancer chargeSound.
        assert_eq!(cfg("lancer").charge_sound, SoundId::CHARGE_LANCER);
        assert_eq!(cfg("lancer").shoot_sound, SoundId::SHOOT_LANCER);
        // `Blocks.java`: wave/tsunami are shoot-soundless LiquidTurrets with a
        // `loopSpray` loop.
        let wave = cfg("wave");
        assert!(wave.shoot_sound.is_none_or_unset());
        assert_eq!(wave.loop_sound, SoundId::LOOP_SPRAY);
        assert_eq!(wave.loop_sound_volume, 0.5);
        // `Blocks.java:4352-4354`: meltdown loop volume 2.0.
        assert_eq!(cfg("meltdown").loop_sound, SoundId::BEAM_MELTDOWN);
        assert_eq!(cfg("meltdown").loop_sound_volume, 2.0);
        // `Blocks.java:5222-5224`: lustre loops beamLustre at volume 1.0.
        assert_eq!(cfg("lustre").loop_sound, SoundId::BEAM_LUSTRE);
        assert_eq!(cfg("lustre").loop_sound_volume, 1.0);
        // `PointDefenseTurret.java:30`: segment has its own intercept sound.
        assert_eq!(cfg("segment").shoot_sound, SoundId::SHOOT_SEGMENT);
        // `Turret.java:127-138` class defaults survive where Blocks.java does
        // not override.
        let duo = cfg("duo");
        assert_eq!(duo.shoot_sound, SoundId::SHOOT_DUO);
        assert_eq!(duo.shoot_sound_volume, 1.0);
        assert!(duo.charge_sound.is_none_or_unset());
        assert_eq!(
            (duo.sound_pitch_min, duo.sound_pitch_max),
            (0.9, 1.1),
            "Turret.java:127-138 pitch window"
        );
    }

    #[test]
    fn hail_shot_emits_its_own_sound() {
        use crate::audio::{AudioEvent, SharedAudioLog};
        let mut harness = CombatHarness::new(48, 16, 7);
        let log = SharedAudioLog::new();
        harness.build.set_audio_sink(log.clone());
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 8, wall, 0, true));
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret("hail", tx, ty, 1)
            .unwrap_or_else(|| panic!("hail"));
        let graphite = harness.content().item_id("graphite").expect("graphite");
        for _ in 0..10 {
            handle_item(&mut harness.build.world, turret, graphite);
        }
        for _ in 0..180 {
            harness.tick();
        }
        assert!(
            harness
                .build
                .world
                .get::<TurretState>(turret)
                .unwrap()
                .total_shots
                > 0,
            "hail fired"
        );
        let mut matched = 0;
        for event in log.events() {
            if let AudioEvent::At { sound, pitch, .. } = &event.event
                && *sound == SoundId::SHOOT_ARTILLERY_SMALL
            {
                assert!((0.9..=1.1).contains(pitch), "pitch {pitch}");
                matched += 1;
            }
        }
        assert!(matched > 0, "hail emitted its own shoot sound");
    }

    /// Runs the harness to the first spawned bullet, then returns the per-tick
    /// spawn counts for the next `window` ticks (index 0 is the first shot's
    /// tick). `bullets_created` is monotonic, so hits/expiry cannot hide a
    /// spawn.
    fn volley_spawn_counts(harness: &mut CombatHarness, window: usize) -> Vec<u64> {
        let mut first = None;
        for _ in 0..240 {
            let before = harness.bullets_created;
            harness.tick();
            if harness.bullets_created > before {
                first = Some(harness.bullets_created - before);
                break;
            }
        }
        let first = first.expect("turret fired");
        let mut counts = vec![first];
        for _ in 0..window {
            let before = harness.bullets_created;
            harness.tick();
            counts.push(harness.bullets_created - before);
        }
        counts
    }

    /// PAR-0451 (`Turret.java:751-830`): `shoot()` defers every pattern shot
    /// with `delay > 0` through `Time.run`, so a multi-shot pattern fires one
    /// bullet per delayed tick instead of collapsing the burst onto the reload
    /// tick.
    #[test]
    fn multi_shot_patterns_fire_on_their_delayed_ticks() {
        // `Blocks.java:3846-3847`: salvo `shots = 4; shotDelay = 3` => T, +3,
        // +6, +9. `Blocks.java:3733-3734`: swarmer `ShootBarrel` `shots = 4;
        // shotDelay = 5` => T, +5, +10, +15.
        for (name, item, offsets) in [
            ("salvo", "copper", [0usize, 3, 6, 9]),
            ("swarmer", "blast-compound", [0, 5, 10, 15]),
        ] {
            let mut harness = CombatHarness::new(48, 16, 7);
            let wall = harness.content().block_id("copper-wall").expect("wall");
            assert!(harness.place(10, 8, wall, 0, true));
            let (tx, ty) = CombatHarness::tile_center(4, 8);
            let turret = harness
                .spawn_test_turret(name, tx, ty, 1)
                .unwrap_or_else(|| panic!("turret `{name}`"));
            let item = harness.content().item_id(item).expect(item);
            for _ in 0..20 {
                handle_item(&mut harness.build.world, turret, item);
            }
            let counts = volley_spawn_counts(&mut harness, offsets[3] + 1);
            let ticks: Vec<usize> = counts
                .iter()
                .enumerate()
                .filter(|(_, count)| **count > 0)
                .map(|(tick, _)| tick)
                .collect();
            assert_eq!(
                ticks, offsets,
                "`{name}` fires one shot per delayed tick, not a burst"
            );
            assert_eq!(counts.iter().sum::<u64>(), 4, "`{name}` 4 shots total");
        }
    }

    /// PAR-0451: scatter's 2-shot pattern (`Blocks.java:3420-3421`
    /// `shotDelay = 5; shots = 2`) spreads over T and T+5. Scatter is anti-air
    /// (`targetGround = false`, `Turret.java:645`), so the harness supplies an
    /// airborne stand-in target (`!isGrounded()`, `UnitComp.java:80-82`); the
    /// assertion only covers spawn ticks.
    #[test]
    fn scatter_fires_one_shot_per_delayed_tick() {
        let mut harness = CombatHarness::new(48, 16, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret("scatter", tx, ty, 1)
            .expect("scatter");
        let scrap = harness.content().item_id("scrap").expect("scrap");
        for _ in 0..10 {
            handle_item(&mut harness.build.world, turret, scrap);
        }
        // A grounded stand-in would be rejected by the air-only branch; this
        // one hovers at elevation 1, like a flying unit type
        // (`UnitType.java:566`).
        let target = harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
        set_unit_elevation(&mut harness, target, 1.0, true);
        let counts = volley_spawn_counts(&mut harness, 6);
        let ticks: Vec<usize> = counts
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(tick, _)| tick)
            .collect();
        assert_eq!(ticks, vec![0, 5], "scatter T and T+5, not 2 at T");
        assert_eq!(counts.iter().sum::<u64>(), 2);
    }

    /// PAR-0451 (`Turret.java:718-720,742,751-830`): `charging()` is
    /// `queuedBullets > 0 && shoot.firstShotDelay > 0`, so lancer's 40-tick
    /// `firstShotDelay` queues its only shot, ramps `charge` at 1/40 per tick,
    /// freezes rotation (`moveWhileCharging = false`, `Blocks.java:3594`) and
    /// gates `updateShooting` until the shot fires.
    #[test]
    fn lancer_charges_before_its_delayed_shot() {
        let mut harness = CombatHarness::new(48, 16, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 8, wall, 0, true));
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret("lancer", tx, ty, 1)
            .expect("lancer");
        {
            let mut state = harness.build.world.get_mut::<TurretState>(turret).unwrap();
            // `reload = 80` (`Blocks.java:3586`): start full so the first tick
            // queues the charged shot; aim already on the wall (angle 0) and a
            // zeroed retarget timer acquire it immediately.
            state.reload_counter = 80.0;
            state.target_timer = 0.0;
            state.rotation = 0.0;
        }
        // Tick 1 queues the shot: nothing spawns, and `charge` is still 0 (it is
        // updated before `updateShooting`).
        harness.tick();
        let state = harness.turret_state(turret).unwrap();
        assert_eq!(harness.bullets_created, 0, "no bullet on the queue tick");
        assert_eq!(state.total_shots, 0);
        assert_eq!(state.charge, 0.0);
        // Freeze probe: aim the turret away; `shouldTurn()` is false while
        // charging, so the rotation must not follow the target.
        harness
            .build
            .world
            .get_mut::<TurretState>(turret)
            .unwrap()
            .rotation = 90.0;
        let mut charge_mid = 0.0f32;
        for k in 2..=40 {
            harness.tick();
            let state = harness.turret_state(turret).unwrap();
            assert_eq!(harness.bullets_created, 0, "no bullet on charge tick {k}");
            assert_eq!(state.rotation, 90.0, "rotation frozen on charge tick {k}");
            if k == 21 {
                charge_mid = state.charge;
            }
        }
        let state = harness.turret_state(turret).unwrap();
        assert!(
            (charge_mid - 0.5).abs() < 0.01,
            "charge ramps 1/40 per tick, got {charge_mid} at tick 21"
        );
        assert!(
            state.reload_counter > 30.0,
            "reload continues while charging (`reloadWhileCharging`): {}",
            state.reload_counter
        );
        // The 40th tick after queueing fires the single charged shot.
        harness.tick();
        let state = harness.turret_state(turret).unwrap();
        assert_eq!(harness.bullets_created, 1, "exactly one charged shot");
        assert_eq!(state.total_shots, 1);
        assert_eq!(state.charge, 0.0, "charge resets after firing");
    }
}

/// J04 T1 (`Turret.findTarget`, `Turret.java:631-677`): unit sorts, min-range
/// lifetime clamp and tracking range.
#[cfg(test)]
mod find_target_tests {
    use super::tests::set_unit_elevation;
    use super::*;
    use crate::combat::harness::CombatHarness;
    use crate::entities::comp::Health;

    /// Spawns a turret with one ammo entry (`amount` units) and a zeroed
    /// retarget timer so the next pass acquires a target.
    fn armed_turret(
        harness: &mut CombatHarness,
        name: &str,
        x: f32,
        y: f32,
        item_name: &str,
        bullet_name: &str,
    ) -> Entity {
        let turret = harness
            .spawn_test_turret(name, x, y, 1)
            .unwrap_or_else(|| panic!("turret `{name}`"));
        let item = harness.content().item_id(item_name).expect(item_name);
        let bullet = harness.content().bullet_names()[bullet_name];
        let mut state = harness
            .build
            .world
            .get_mut::<TurretState>(turret)
            .expect("turret state");
        state.ammo.push(AmmoEntry {
            item,
            bullet,
            amount: 40,
        });
        state.target_timer = 0.0;
        turret
    }

    /// Two enemies at different health and distance: foreshadow (`strongest`,
    /// `Blocks.java:4261`) picks the far high-health unit, duo (`closest`,
    /// `Turret.java:113` default) the near one.
    #[test]
    fn find_target_strongest_and_closest_sorts() {
        for (turret_name, item_name, bullet_name, expect_far) in [
            ("foreshadow", "surge-alloy", "foreshadow_rail", true),
            ("duo", "copper", "duo_copper", false),
        ] {
            let mut harness = CombatHarness::new(64, 64, 7);
            let (tx, ty) = CombatHarness::tile_center(4, 8);
            let turret = armed_turret(&mut harness, turret_name, tx, ty, item_name, bullet_name);
            // Near unit (40 px, 100 health) vs far unit (80 px, 1000 health);
            // `strongest = -maxHealth + dst2/6400` (`UnitSorts.java:13`).
            let near = harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
            harness
                .build
                .world
                .get_mut::<Health>(near)
                .unwrap()
                .max_health = 100.0;
            let far = harness.spawn_test_unit(tx + 80.0, ty, 2, Vec::new());
            harness
                .build
                .world
                .get_mut::<Health>(far)
                .unwrap()
                .max_health = 1000.0;
            harness.update_turrets_only();
            let state = harness.turret_state(turret).expect("state");
            let expected = if expect_far { far } else { near };
            assert_eq!(state.target, Some(expected), "`{turret_name}` target sort");
        }
    }

    /// Ripple (`unitSort = grouped`, `Blocks.java:4107`) picks a member of the
    /// clustered trio over a lone nearer unit (`-getClustered + dst * 0.003`,
    /// `UnitSorts.java:24-27`).
    #[test]
    fn find_target_grouped_prefers_cluster() {
        let mut harness = CombatHarness::new(64, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = armed_turret(
            &mut harness,
            "ripple",
            tx,
            ty,
            "graphite",
            "ripple_graphite",
        );
        // Lone unit 40 px away: cost -1 + 40 * 0.003 = -0.88.
        harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
        // Three units in one 70 px cluster cell 200 px away: cost -3 + 0.6.
        let first_cluster = harness.spawn_test_unit(tx + 200.0, ty, 2, Vec::new());
        harness.spawn_test_unit(tx + 201.0, ty, 2, Vec::new());
        harness.spawn_test_unit(tx + 202.0, ty, 2, Vec::new());
        harness.update_turrets_only();
        let state = harness.turret_state(turret).expect("state");
        assert_eq!(
            state.target,
            Some(first_cluster),
            "grouped picks the cluster (first in stable unit order on a tie)"
        );
    }

    /// Ripple's `minRange = 50` (`Blocks.java:4100`) clamps the `scaleLife`
    /// lifetime to `minRange()/range` (`Turret.java:797`): a shell aimed 40 px
    /// away lives for the minimum fraction, not the shorter raw distance ratio.
    #[test]
    fn ripple_min_range_clamps_bullet_lifetime() {
        let mut harness = CombatHarness::new(64, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = armed_turret(
            &mut harness,
            "ripple",
            tx,
            ty,
            "graphite",
            "ripple_graphite",
        );
        // Target inside the 50-unit minimum range.
        let target = harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
        {
            let mut state = harness.build.world.get_mut::<TurretState>(turret).unwrap();
            state.target = Some(target);
            state.target_pos = (tx + 40.0, ty);
        }
        let def_lifetime = {
            let bullet = harness.content().bullet_names()["ripple_graphite"];
            harness
                .content()
                .bullet(bullet)
                .map(|def| def.lifetime)
                .unwrap_or(0.0)
        };
        let mut fired: Option<f32> = None;
        for _ in 0..200 {
            harness.tick();
            if
            // A newer bullet than the pre-set state: the first spawn.
            let Some(entity) = harness.bullets.last().copied()
                && let Some(bullet) = harness
                    .build
                    .world
                    .get::<crate::combat::bullet::Bullet>(entity)
            {
                fired = Some(bullet.lifetime);
                break;
            }
        }
        let lifetime = fired.expect("a shell spawned");
        // (baseLife(1) + scaleLifetimeOffset(0.1)) * dst(40) / type.range(240)
        // = 0.1833, clamped up to minRange(50)/range(240) = 0.2083.
        let expected = def_lifetime * (50.0 / 240.0);
        assert!(
            (lifetime - expected).abs() < 0.5,
            "clamped lifetime {lifetime} ~= {expected}"
        );
        let unclamped = def_lifetime * 1.1 * 40.0 / 240.0;
        assert!(
            lifetime > unclamped,
            "the min-range clamp raised the lifetime"
        );
    }

    /// Smite's `trackingRange = range * 1.4` (`Blocks.java:5947`): a target
    /// beyond the firing range but inside the tracking range is acquired; one
    /// beyond it is not.
    #[test]
    fn find_target_uses_tracking_range() {
        let mut harness = CombatHarness::new(128, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = armed_turret(&mut harness, "smite", tx, ty, "surge-alloy", "smite_orb");
        let config = harness.turret_state(turret).expect("state").config.clone();
        assert!((config.tracking_range - config.range * 1.4).abs() < 0.01);
        // 350 px: outside `range` 300, inside `trackingRange` 420.
        let tracked = harness.spawn_test_unit(tx + 350.0, ty, 2, Vec::new());
        harness.update_turrets_only();
        assert_eq!(
            harness.turret_state(turret).unwrap().target,
            Some(tracked),
            "second pass at trackingRange()"
        );
        // 460 px: outside the tracking range too.
        harness.build.world.get_mut::<Pos>(tracked).unwrap().x = tx + 460.0;
        harness
            .build
            .world
            .get_mut::<TurretState>(turret)
            .unwrap()
            .target_timer = 0.0;
        harness.update_turrets_only();
        assert_eq!(
            harness.turret_state(turret).unwrap().target,
            None,
            "out of tracking range"
        );
    }

    /// `Turret.newTargetInterval` (`Turret.java:571`): scathe retargets on 40
    /// ticks while holding a target (`Blocks.java:5666`), the plain interval
    /// otherwise.
    #[test]
    fn new_target_interval_applies_with_a_target() {
        let mut harness = CombatHarness::new(64, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = armed_turret(&mut harness, "scathe", tx, ty, "carbide", "scathe_carbide");
        assert_eq!(
            harness
                .turret_state(turret)
                .unwrap()
                .config
                .new_target_interval,
            40.0
        );
        // First pass: no target held -> the plain interval.
        harness.update_turrets_only();
        assert_eq!(harness.turret_state(turret).unwrap().target, None);
        assert_eq!(
            harness.turret_state(turret).unwrap().target_timer,
            harness.turret_state(turret).unwrap().config.target_interval
        );
        // With a target in range the retarget interval becomes 40.
        harness.spawn_test_unit(tx + 100.0, ty, 2, Vec::new());
        harness
            .build
            .world
            .get_mut::<TurretState>(turret)
            .unwrap()
            .target_timer = 0.0;
        harness.update_turrets_only();
        assert!(harness.turret_state(turret).unwrap().target.is_some());
        assert_eq!(harness.turret_state(turret).unwrap().target_timer, 40.0);
    }

    /// `Turret.findEnemy` air-only branch (`Turret.java:644-646`): scatter
    /// (`targetGround = false`) rejects grounded units
    /// (`isGrounded = elevation < 0.001`, `UnitComp.java:80-82`) and targets
    /// aircraft. The accept predicate is `!e.isGrounded()` — not the
    /// `isFlying()` (`elevation >= 0.09`) band.
    #[test]
    fn air_only_turret_targets_aircraft_over_ground() {
        let mut harness = CombatHarness::new(64, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = armed_turret(&mut harness, "scatter", tx, ty, "scrap", "scatter_scrap");
        let config = harness.turret_state(turret).expect("state").config.clone();
        assert!(
            config.target_air && !config.target_ground,
            "scatter is air-only"
        );
        // Grounded enemy 40 px away: nearest, but not a legal target.
        harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
        // Flying enemy 100 px away: the only legal target.
        let flying = harness.spawn_test_unit(tx + 100.0, ty, 2, Vec::new());
        set_unit_elevation(&mut harness, flying, 1.0, true);
        harness.update_turrets_only();
        assert_eq!(
            harness.turret_state(turret).expect("state").target,
            Some(flying),
            "`!isGrounded()` keeps the aircraft and drops the grounded unit"
        );
    }

    /// Air-only turret with only grounded enemies in range: no target
    /// (`Turret.java:645` rejects every `isGrounded()` unit).
    #[test]
    fn air_only_turret_rejects_grounded_only() {
        let mut harness = CombatHarness::new(64, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = armed_turret(&mut harness, "scatter", tx, ty, "scrap", "scatter_scrap");
        harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
        harness.spawn_test_unit(tx + 120.0, ty, 2, Vec::new());
        harness.update_turrets_only();
        assert_eq!(harness.turret_state(turret).expect("state").target, None);
    }

    /// Java's air-only gate is `!isGrounded()` (`elevation < 0.001`), so a unit
    /// in the 0.001..0.09 band is a legal target even though `isFlying()`
    /// (`elevation >= 0.09`, `UnitComp.java:84-86`) is false — the air-only
    /// branch must not be written as `isFlying()`.
    #[test]
    fn air_only_turret_accepts_mid_band_elevation() {
        let mut harness = CombatHarness::new(64, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = armed_turret(&mut harness, "scatter", tx, ty, "scrap", "scatter_scrap");
        let band = harness.spawn_test_unit(tx + 60.0, ty, 2, Vec::new());
        set_unit_elevation(&mut harness, band, 0.05, false);
        harness.update_turrets_only();
        assert_eq!(
            harness.turret_state(turret).expect("state").target,
            Some(band),
            "0.05 elevation is not grounded, so the air-only branch keeps it"
        );
    }

    /// General branch (`Turret.java:647-651`): a `targetGround && targetAir`
    /// turret keeps both bands — `(isGrounded() || targetAir) &&
    /// (!isGrounded() || targetGround)`.
    #[test]
    fn general_turret_targets_ground_and_air() {
        for (elevation, physics_flying) in [(0.0, false), (1.0, true)] {
            let mut harness = CombatHarness::new(64, 64, 7);
            let (tx, ty) = CombatHarness::tile_center(4, 8);
            let turret = armed_turret(&mut harness, "duo", tx, ty, "copper", "duo_copper");
            let config = harness.turret_state(turret).expect("state").config.clone();
            assert!(
                config.target_air && config.target_ground,
                "duo targets both"
            );
            let unit = harness.spawn_test_unit(tx + 40.0, ty, 2, Vec::new());
            set_unit_elevation(&mut harness, unit, elevation, physics_flying);
            harness.update_turrets_only();
            assert_eq!(
                harness.turret_state(turret).expect("state").target,
                Some(unit),
                "targetAir && targetGround keeps elevation {elevation}"
            );
        }
    }

    /// General branch with `targetAir = false` (hail, `Blocks.java`): a unit in
    /// the 0.001..0.09 band is not grounded, so Java rejects it in both terms
    /// (`(isGrounded() || targetAir)` is false and `!isGrounded()` alone is not
    /// enough without the first term).
    #[test]
    fn ground_only_turret_rejects_mid_band_elevation() {
        let mut harness = CombatHarness::new(64, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = armed_turret(&mut harness, "hail", tx, ty, "graphite", "hail_graphite");
        let config = harness.turret_state(turret).expect("state").config.clone();
        assert!(
            config.target_ground && !config.target_air,
            "hail is ground-only"
        );
        let band = harness.spawn_test_unit(tx + 60.0, ty, 2, Vec::new());
        set_unit_elevation(&mut harness, band, 0.05, false);
        harness.update_turrets_only();
        assert_eq!(
            harness.turret_state(turret).expect("state").target,
            None,
            "0.05 elevation is not grounded and hail has targetAir = false"
        );
        // A truly grounded unit at the same distance is still targeted.
        let grounded = harness.spawn_test_unit(tx + 60.0, ty, 2, Vec::new());
        harness
            .build
            .world
            .get_mut::<TurretState>(turret)
            .expect("state")
            .target_timer = 0.0;
        harness.update_turrets_only();
        let _ = band;
        assert_eq!(
            harness.turret_state(turret).expect("state").target,
            Some(grounded)
        );
    }

    // ---- GAPS #2: Turret.limitRange -------------------------------------

    /// `Turret.limitRange` (`Turret.java:246-253`): every ammo bullet's
    /// lifetime is recomputed from the turret range, its `rangeChange`, the
    /// per-turret margin and `extraRangeMargin`.
    #[test]
    fn limit_range_recomputes_ammo_lifetime() {
        let harness = CombatHarness::new(16, 16, 7);
        let lifetime = |name: &str| {
            let id = harness.content().bullet_by_name(name).expect(name);
            harness.content().bullet(id).expect(name).lifetime
        };
        // `Blocks.java:3348` duo `limitRange(5f)`: (160 + 5 + 10) / 2.5.
        assert_eq!(lifetime("duo_copper"), 70.0);
        // `rangeChange = 16f` shifts the real range, not just base range.
        assert!((lifetime("duo_graphite") - (160.0 + 16.0 + 5.0 + 10.0) / 3.5).abs() < 1e-4);
        // `Blocks.java:3853` salvo default margin 9: (190 + 9 + 10) / 2.5.
        assert!((lifetime("salvo_copper") - 83.6).abs() < 1e-4);
        // `Blocks.java:3544` hail `limitRange(0f)` (range 235).
        assert!((lifetime("hail_graphite") - (235.0 + 0.0 + 10.0) / 3.0).abs() < 1e-4);
        // `Blocks.java:4487` breach `limitRange(12f)`: (190 + 12 + 10) / 7.5.
        assert!((lifetime("breach_beryllium") - (190.0 + 12.0 + 10.0) / 7.5).abs() < 1e-4);
        // `Blocks.java:4577` diffuse `limitRange(25f)` (range 125).
        assert!(
            (lifetime("diffuse_graphite") - (125.0 + 25.0 + 10.0) / 8.0).abs() < 1e-4,
            "diffuse graphite lifetime {}",
            lifetime("diffuse_graphite")
        );
        // `Blocks.java:5054` disperse `limitRange(16f)` + `extraRangeMargin 32`
        // (`Blocks.java:4922`): rangeChange -120 → (310 - 120 + 16 + 32 + 10) / 9.5.
        assert!(
            (lifetime("disperse_thorium") - (310.0 - 120.0 + 16.0 + 32.0 + 10.0) / 9.5).abs()
                < 1e-4,
            "disperse thorium lifetime {}",
            lifetime("disperse_thorium")
        );
        // `Blocks.java:5177` afflict `PowerTurret.limitRange(-55f)`: (368 - 55 + 10) / 5.
        assert!((lifetime("afflict_orb") - (368.0 - 55.0 + 10.0) / 5.0).abs() < 1e-4);
    }

    // ---- GAPS #3: ErekirTechTree.rebalance part (b) ----------------------

    /// `ErekirTechTree.rebalance()` (`ErekirTechTree.java:37-47`) scales ammo
    /// damage of turrets with non-Serpulo requirements by `0.75`; `PowerTurret`
    /// is excluded by the Java `instanceof` chain.
    #[test]
    fn erekir_turret_ammo_rebalanced() {
        let harness = CombatHarness::new(16, 16, 7);
        let damage = |name: &str| {
            let id = harness.content().bullet_by_name(name).expect(name);
            harness.content().bullet(id).expect(name).damage
        };
        assert_eq!(damage("breach_beryllium"), 85.0 * 0.75);
        assert_eq!(damage("breach_tungsten"), 95.0 * 0.75);
        assert_eq!(damage("diffuse_graphite"), 41.0 * 0.75);
        assert_eq!(damage("titan_thorium"), 350.0 * 0.75);
        assert_eq!(damage("disperse_surge"), 65.0 * 0.75);
        assert_eq!(damage("sublimate_ozone"), 60.0 * 0.75);
        assert_eq!(damage("lustre_laser"), 210.0 * 0.75);
        assert_eq!(damage("smite_orb"), 250.0 * 0.75);
        // Serpulo turrets are untouched.
        assert_eq!(damage("duo_copper"), 9.0);
        assert_eq!(damage("salvo_copper"), 15.0);
        // `PowerTurret` (afflict/malign) is outside Java's `instanceof` chain.
        assert_eq!(damage("afflict_orb"), 180.0);
        assert_eq!(damage("malign_flak"), 70.0);
    }

    // ---- GAPS #5: cheating() --------------------------------------------

    /// Installs a `CampaignRuntime` whose `team` rule has `cheat` set
    /// (`TeamComp.cheating()` reads `team.rules().cheat`).
    fn install_cheat_rules(h: &mut CombatHarness, team: u8) {
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        // `BuildHarness::place` assigns `rules.default_team` to new buildings.
        h.build.rules.default_team = team;
        let campaign = Campaign::from_registry(h.content(), &EmptyNeighborhood);
        let mut rules = Rules {
            default_team: team,
            ..Rules::default()
        };
        rules.team_rule_mut(team).cheat = true;
        h.build
            .world
            .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
    }

    /// `Turret.java:715`: a cheating team's turret reports ammo even below
    /// `ammoPerShot`, and `useAmmo` (`:685`) does not consume it.
    #[test]
    fn cheating_gives_infinite_ammo() {
        let mut h = CombatHarness::new(32, 32, 7);
        let turret = h
            .spawn_test_turret("breach", 60.0, 60.0, 1)
            .expect("breach");
        let tungsten = h.content().item_id("beryllium").expect("beryllium");
        let bullet = h
            .content()
            .bullet_by_name("breach_beryllium")
            .expect("ammo");
        {
            let mut state = h.build.world.get_mut::<TurretState>(turret).expect("state");
            state.ammo.push(AmmoEntry {
                item: tungsten,
                bullet,
                amount: 1, // < ammoPerShot (2)
            });
            state.total_ammo = 1;
        }
        assert!(!has_ammo(&h.build.world, turret), "1 < ammoPerShot");
        install_cheat_rules(&mut h, 1);
        assert!(has_ammo(&h.build.world, turret), "cheat ignores the stack");
        // `useAmmo` leaves the stack untouched.
        let mut state = h
            .build
            .world
            .entity_mut(turret)
            .take::<TurretState>()
            .expect("state");
        let used = use_ammo(&mut state, &mut h.build.world, turret);
        assert_eq!(used, Some(bullet));
        assert_eq!(state.total_ammo, 1, "cheat never consumes ammo");
        assert_eq!(state.ammo[0].amount, 1);
        h.build.world.entity_mut(turret).insert(state);
    }

    /// `ItemTurret.java:103-110`: a cheating block placed without ammo gets its
    /// first ammo type in `onProximityAdded`.
    #[test]
    fn cheating_item_turret_auto_fills_on_placement() {
        let mut h = CombatHarness::new(32, 32, 7);
        install_cheat_rules(&mut h, 1);
        let duo = h.content().block_id("duo").expect("duo");
        assert!(h.place(4, 4, duo, 1, true), "place duo");
        let turret = h.build_at(4, 4).expect("turret building");
        let state = h.build.world.get::<TurretState>(turret).expect("state");
        assert_eq!(state.total_ammo, 2, "first ammo type auto-filled");
        assert_eq!(state.ammo.len(), 1);
        assert_eq!(
            state.ammo[0].item,
            h.content().item_id("copper").expect("copper")
        );
    }

    /// `Turret.java:677-681`: the heat efficiency multiplier is floored at 1
    /// only for a cheating team, capped by `maxHeatEfficiency`.
    #[test]
    fn cheating_bypasses_heat_efficiency_floor() {
        let mut h = CombatHarness::new(32, 32, 7);
        let turret = h
            .spawn_test_turret("afflict", 60.0, 60.0, 1)
            .expect("afflict");
        let mut state = h
            .build
            .world
            .entity_mut(turret)
            .take::<TurretState>()
            .expect("state");
        state.heat_req = 0.0;
        let config = state.config.clone();
        assert_eq!(
            update_efficiency_multiplier(&h.build.world, turret, 1.0, &state, &config),
            0.0,
            "no heat and no cheat -> efficiency 0"
        );
        h.build.world.entity_mut(turret).insert(state);
        install_cheat_rules(&mut h, 1);
        let state = h.build.world.get::<TurretState>(turret).expect("state");
        assert_eq!(
            update_efficiency_multiplier(&h.build.world, turret, 1.0, state, &config),
            1.0,
            "cheat floors the heat ratio at 1 (capped by maxHeatEfficiency 1)"
        );
    }

    // ---- GAPS #7: LiquidTurret fire extinguishing -------------------------

    /// `LiquidTurretBuild.findTarget` (`LiquidTurret.java:91-119`): a liquid
    /// turret whose current liquid can extinguish targets the nearest fire in
    /// range instead of running the standard targeting pass.
    #[test]
    fn liquid_turret_targets_fires() {
        let mut h = CombatHarness::new(32, 32, 7);
        let water = h.content().liquid_id("water").expect("water");
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = h.spawn_test_turret("wave", tx, ty, 1).expect("wave");
        {
            let mut liquids = h
                .build
                .world
                .get_mut::<LiquidModule>(turret)
                .expect("liquids");
            liquids.liquids[water.index()] = 100.0;
            liquids.current_liquid = water;
        }
        // A fire on the turret's right, inside `range` (110).
        let fire =
            crate::combat::fires::create(&mut h.build.world, 10, 8, &mut h.rng).expect("fire");
        // A fire on an enemy block must be ignored (`other.team() == team` gate).
        let wall = h.content().block_id("copper-wall").expect("wall");
        assert!(h.place(3, 8, wall, 0, true));
        let blocked_fire =
            crate::combat::fires::create(&mut h.build.world, 3, 8, &mut h.rng).expect("fire");
        h.build
            .world
            .get_mut::<TurretState>(turret)
            .expect("state")
            .target_timer = 0.0;
        h.update_turrets_only();
        assert_eq!(
            h.turret_state(turret).expect("state").target,
            Some(fire),
            "nearest own-team fire is targeted"
        );
        let _ = blocked_fire;
    }

    /// A non-extinguishing liquid (oil) never takes the fire path
    /// (`liquids.current().canExtinguish()`).
    #[test]
    fn liquid_turret_ignores_fire_with_flammable_ammo() {
        let mut h = CombatHarness::new(32, 32, 7);
        let oil = h.content().liquid_id("oil").expect("oil");
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = h.spawn_test_turret("wave", tx, ty, 1).expect("wave");
        {
            let mut liquids = h
                .build
                .world
                .get_mut::<LiquidModule>(turret)
                .expect("liquids");
            liquids.liquids[oil.index()] = 100.0;
            liquids.current_liquid = oil;
        }
        let _fire = crate::combat::fires::create(&mut h.build.world, 10, 8, &mut h.rng);
        h.build
            .world
            .get_mut::<TurretState>(turret)
            .expect("state")
            .target_timer = 0.0;
        h.update_turrets_only();
        assert_eq!(
            h.turret_state(turret).expect("state").target,
            None,
            "oil cannot extinguish, so the fire is not a target"
        );
    }

    // ---- GAPS #8: TractorBeamTurret --------------------------------------

    /// `TractorBeamTurret.java:127-130`: the beam uses
    /// `damageContinuousPierce` — shields absorb first, the death path runs —
    /// and `targetGround = false` keeps grounded units safe.
    #[test]
    fn tractor_beam_damages_through_shields_and_ignores_ground() {
        use crate::entities::comp::unit::comp::{PhysicsComp, ShieldComp};

        let mut h = CombatHarness::new(64, 64, 7);
        let (tx, ty) = CombatHarness::tile_center(8, 8);
        let turret = h
            .spawn_test_turret("parallax", tx, ty, 1)
            .expect("parallax");
        // Default turret rotation is 90° (+y): place the air target straight up.
        let air = h.spawn_test_unit(tx, ty + 100.0, 2, Vec::new());
        h.build.world.entity_mut(air).insert(PhysicsComp {
            flying: true,
            ..PhysicsComp::default()
        });
        h.build.world.entity_mut(air).insert(ShieldComp {
            shield: 2.0,
            shield_alpha: 0.0,
        });
        // A grounded unit at the same distance must be ignored.
        let ground = h.spawn_test_unit(tx, ty + 60.0, 2, Vec::new());
        {
            let mut state = h.build.world.get_mut::<TurretState>(turret).expect("state");
            state.retarget_timer = 0.0;
            state.target_timer = 0.0;
            state.rotation = 90.0;
        }
        for _ in 0..10 {
            h.update_turrets_only();
        }
        let shield = h.build.world.get::<ShieldComp>(air).expect("shield").shield;
        let air_health = h.build.world.get::<Health>(air).expect("health").health;
        assert!(shield < 2.0, "the shield absorbed beam damage: {shield}");
        assert!(
            air_health < 1_000.0,
            "the beam also reaches health after the shield: {air_health}"
        );
        assert_eq!(
            h.build.world.get::<Health>(ground).expect("health").health,
            1_000.0,
            "grounded units are outside `targetGround = false`"
        );
    }

    /// PAR-0528: `BuildTurret.java:156-176` — with no queued plan the tower
    /// scans the team rebuild queue, takes the first in-range legal entry,
    /// rotates it to the tail (`lastPlan` is recorded) and drives the
    /// `ConstructBuild` at that tile.
    #[test]
    fn build_turret_scans_the_team_rebuild_queue() {
        use crate::ecs::TeamId;
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::teams::BlockPlan;
        use crate::game::universe::Campaign;

        let mut harness = CombatHarness::new(48, 16, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 8, wall, 0, false));
        let construct = harness.build_at(10, 8).expect("construct entity");
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let _turret = harness
            .spawn_test_turret("build-tower", tx, ty, 1)
            .expect("build tower");
        // The team rebuild queue is a `CampaignRuntime` field
        // (`team.data().plans`).
        let campaign = Campaign::from_registry(harness.build.content(), &EmptyNeighborhood);
        let session = PlaySession::new(Rules {
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        let mut runtime = CampaignRuntime::new(session, campaign);
        runtime
            .session
            .teams
            .get(TeamId(1))
            .plans
            .push_back(BlockPlan {
                x: 10,
                y: 8,
                rotation: 0,
                block: wall,
                config: None,
                removed: false,
            });
        runtime
            .session
            .teams
            .get(TeamId(1))
            .plans
            .push_back(BlockPlan {
                x: 30,
                y: 8,
                rotation: 0,
                block: wall,
                config: None,
                removed: false,
            });
        harness.build.world.insert_resource(runtime);

        for _ in 0..12 {
            harness.tick();
        }
        let progress = harness
            .build
            .world
            .get::<ConstructState>(construct)
            .expect("construct state")
            .progress;
        assert!(progress > 0.0, "team-queue plan drove the construction");
        // `blocks.addLast(blocks.removeIndex(i))`: the taken plan moved to the
        // tail, the out-of-range one is now first — proof the scan selected
        // `(10, 8)` rather than leaving the queue in insertion order.
        let queue = &harness
            .build
            .world
            .get_resource::<CampaignRuntime>()
            .expect("runtime")
            .session
            .teams
            .get_or_null(TeamId(1))
            .expect("team data")
            .plans;
        assert_eq!(queue.len(), 2);
        assert_eq!((queue[0].x, queue[0].y), (30, 8));
        assert_eq!((queue[1].x, queue[1].y), (10, 8));
    }
}

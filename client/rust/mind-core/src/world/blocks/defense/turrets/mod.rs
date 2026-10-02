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

use crate::combat::bullet::{BulletSpawn, CombatCtx};
use crate::content::registries::bullets::BulletDef;
use crate::content::registries::units::weapon::ShootPatternSpec;
use crate::content::{BulletId, BulletKind, ContentRegistry, ItemId, LiquidId};
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::weapons::pattern::{self, ShotBuffer};
use crate::world::modules::{LiquidModule, PowerModule};

pub mod advanced;
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
}

/// Ammo family (`ItemTurret`/`LiquidTurret`/`PowerTurret`).
#[derive(Debug, Clone, PartialEq)]
pub enum TurretAmmo {
    /// `ItemTurret.ammoTypes`.
    Item(Vec<ItemAmmo>),
    /// `LiquidTurret.ammoTypes`.
    Liquid(Vec<LiquidAmmo>),
    /// `PowerTurret.shootType`.
    Power(BulletId),
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
    /// `Turret.alwaysShooting`.
    pub always_shooting: bool,
    /// `BaseTurret.activationTime`.
    pub activation_time: f32,
    /// `BaseTurret.coolantMultiplier`.
    pub coolant_multiplier: f32,
    /// Coolant units consumed per tick (`ConsumeCoolant.amount`); `0` = no coolant.
    pub coolant_amount: f32,
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
            ammo: SmallVec::new(),
            total_ammo: 0,
            target: None,
            target_pos: (0.0, 0.0),
            target_timer: target_interval,
            was_shooting: false,
            is_shooting: false,
            activation_timer: activation,
            logic_control_time: -1.0,
            logic_shooting: false,
            bullets: SmallVec::new(),
            last_length: 0.0,
            retarget_timer: 0.0,
            strength: 0.0,
            unit_target: None,
            bullet_target: None,
            any: false,
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
    add("duo_copper", BulletKind::Basic, &|def| {
        def.speed = 2.5;
        def.lifetime = 60.0;
        def.damage = 9.0;
        def.ammo_multiplier = 2.0;
        def.hit_size = 4.0;
        def.drag = 0.0;
    });
    add("duo_graphite", BulletKind::Basic, &|def| {
        def.speed = 3.5;
        def.lifetime = 60.0;
        def.damage = 18.0;
        def.ammo_multiplier = 4.0;
        def.reload_multiplier = 0.8;
        def.hit_size = 4.0;
        def.drag = 0.0;
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
    });

    // M5/M6 remainder: the rest of the vanilla turret ammo tables (`Blocks.java`).
    advanced::register_bullets(content, names);
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
    let item = |n: &str| content.item_id(n);
    let liquid = |n: &str| content.liquid_id(n);
    let bullet = |n: &str| names.get(n).copied();
    let item_ammo = |item_name: &str, bullet_name: &str| -> Option<ItemAmmo> {
        let id = bullet(bullet_name)?;
        let mult = content
            .bullet(id)
            .map(|d| d.ammo_multiplier as i32)
            .unwrap_or(1);
        Some(ItemAmmo {
            item: item(item_name)?,
            bullet: id,
            ammo_multiplier: mult.max(1),
        })
    };
    let liquid_ammo = |liquid_name: &str, bullet_name: &str| -> Option<LiquidAmmo> {
        let id = bullet(bullet_name)?;
        let mult = content.bullet(id).map(|d| d.ammo_multiplier).unwrap_or(1.0);
        Some(LiquidAmmo {
            liquid: liquid(liquid_name)?,
            bullet: id,
            ammo_multiplier: mult.max(0.001),
        })
    };

    match name {
        "duo" => {
            let copper = item_ammo("copper", "duo_copper")?;
            let graphite = item_ammo("graphite", "duo_graphite")?;
            let silicon = item_ammo("silicon", "duo_silicon")?;
            Some(TurretConfig {
                ammo: TurretAmmo::Item(vec![copper, graphite, silicon]),
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
                inaccuracy: 2.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
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
                always_shooting: false,
                activation_time: 0.0,
                coolant_multiplier: 10.0,
                coolant_amount: 0.1,
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
            })
        }
        "test-item" => {
            let copper = item_ammo("copper", "duo_copper")?;
            Some(TurretConfig {
                ammo: TurretAmmo::Item(vec![copper]),
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
                inaccuracy: 0.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
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
                always_shooting: false,
                activation_time: 0.0,
                coolant_multiplier: 10.0,
                coolant_amount: 0.1,
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
            })
        }
        "test-liquid" => {
            let water = liquid_ammo("water", "fuse")?;
            Some(TurretConfig {
                ammo: TurretAmmo::Liquid(vec![water]),
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
                inaccuracy: 0.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
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
                always_shooting: false,
                activation_time: 0.0,
                coolant_multiplier: 0.0,
                coolant_amount: 0.0,
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
            })
        }
        "test-power" => {
            let bullet = bullet("fuse")?;
            Some(TurretConfig {
                ammo: TurretAmmo::Power(bullet),
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
                inaccuracy: 0.0,
                velocity_rnd: 0.0,
                extra_velocity: 0.0,
                life_rnd: 0.0,
                extra_life: 0.0,
                scale_lifetime_offset: 0.0,
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
                always_shooting: false,
                activation_time: 0.0,
                coolant_multiplier: 0.0,
                coolant_amount: 0.0,
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
        TurretAmmo::Liquid(ammo) => {
            let liquids = world.get::<LiquidModule>(e)?;
            let mut best: Option<(f32, BulletId)> = None;
            for entry in ammo {
                let amount = liquids.get(entry.liquid);
                if amount > 0.0 && best.is_none_or(|(current, _)| amount > current) {
                    best = Some((amount, entry.bullet));
                }
            }
            best.map(|(_, bullet)| bullet)
        }
        TurretAmmo::Power(bullet) => Some(*bullet),
    }
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
        TurretAmmo::Item(_) => state
            .ammo
            .last()
            .is_some_and(|entry| entry.amount >= state.config.ammo_per_shot),
        TurretAmmo::Liquid(ammo) => {
            let Some(liquids) = world.get::<LiquidModule>(e) else {
                return false;
            };
            ammo.iter()
                .any(|entry| liquids.get(entry.liquid) >= 1.0 / entry.ammo_multiplier)
        }
        TurretAmmo::Power(_) => true,
    }
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
        return false;
    };
    if !ammo.iter().any(|entry| entry.liquid == liquid) {
        return false;
    }
    let Some(liquids) = world.get::<LiquidModule>(e) else {
        return false;
    };
    let current = ammo
        .iter()
        .filter(|entry| liquids.get(entry.liquid) > 0.0)
        .max_by(|a, b| {
            liquids
                .get(a.liquid)
                .partial_cmp(&liquids.get(b.liquid))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|entry| entry.liquid);
    match current {
        Some(active) => active == liquid || liquids.get(active) <= 0.001,
        None => true,
    }
}

/// `LiquidTurretBuild.handleLiquid` (only fills the storage module).
pub fn handle_liquid(world: &mut World, e: Entity, liquid: LiquidId, amount: f32) {
    let capacity = world
        .get::<TurretState>(e)
        .and_then(|state| match &state.config.ammo {
            TurretAmmo::Liquid(_) => Some(60.0f32),
            _ => None,
        })
        .unwrap_or(0.0);
    if let Some(mut liquids) = world.get_mut::<LiquidModule>(e) {
        liquids.add(liquid, amount, capacity);
    }
}

/// Removes one shot's worth of ammo and returns the fired bullet
/// (`TurretBuild.useAmmo`).
fn use_ammo(state: &mut TurretState, world: &mut World, e: Entity) -> Option<BulletId> {
    let bullet = peek_ammo_state(state, world, e)?;
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
        TurretAmmo::Liquid(ammo) => {
            if let Some(entry) = ammo.iter().find(|entry| entry.bullet == bullet) {
                let amount = 1.0 / entry.ammo_multiplier.max(0.001);
                liquid_remove = Some((entry.liquid, amount));
            }
        }
        TurretAmmo::Power(_) => {}
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
        let Some(mut state) = ctx.world.entity_mut(turret).take::<TurretState>() else {
            continue;
        };
        update_turret(ctx, turret, &mut state);
        ctx.world.entity_mut(turret).insert(state);
    }
}

fn update_turret(ctx: &mut CombatCtx<'_>, e: Entity, state: &mut TurretState) {
    let config = state.config.clone();
    let (x, y) = position(ctx.world, e).unwrap_or((0.0, 0.0));
    let team = ctx.team(e);
    let eff = efficiency(ctx.world, e);

    // Target validation + shooting intent.
    if state.target.is_some() && !validate_target(ctx, e, state) {
        state.target = None;
    }
    state.is_shooting = config.always_shooting || state.target.is_some() || state.logic_shooting;

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
            advanced::update_tractor(ctx, e, state, &config, x, y, team, eff);
            return;
        }
        TurretKind::Continuous => {
            advanced::update_continuous(ctx, e, state, &config, x, y, team, eff);
            return;
        }
        TurretKind::Laser => {
            advanced::update_laser(ctx, e, state, &config, x, y, team, eff);
            return;
        }
        _ => {}
    }

    if !has_ammo_state(state, ctx.world, e) {
        return;
    }

    // Target acquisition on the interval timer.
    state.target_timer -= 1.0;
    if state.target_timer <= 0.0 {
        find_target(ctx, e, state, team, x, y);
        state.target_timer = config.target_interval;
    }
    if state.target.is_none() {
        return;
    }

    // Turn toward the target.
    let target_rot = angle_to(x, y, state.target_pos.0, state.target_pos.1);
    state.rotation = move_toward(state.rotation, target_rot, config.rotate_speed * eff);

    let in_cone = angle_dist(state.rotation, target_rot) < config.shoot_cone;
    if config.always_shooting || in_cone {
        state.was_shooting = true;
        update_shooting(ctx, e, state);
    }
}

/// `TurretBuild.handleReload` + `updateReload` + `updateCooling`.
fn handle_reload(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    config: &TurretConfig,
    eff: f32,
) {
    if charging(state) {
        return;
    }
    // `baseReloadSpeed() == efficiency`, which is zero without power/heat
    // (`TurretBuild.updateReload` × `updateEfficiencyMultiplier`).
    if !can_consume(state, ctx.world, e, config) {
        return;
    }
    if state.reload_counter < config.reload {
        let ammo = peek_ammo_state(state, ctx.world, e);
        state.reload_counter += config.ammo_reload_multiplier(ammo, ctx.content) * eff;
        if config.uses_coolant()
            && let Some(liquids) = ctx.world.get::<LiquidModule>(e)
        {
            // Coolant capacity from the current liquid (`Liquid.heatCapacity`).
            let mut capacity = 0.0f32;
            for (index, amount) in liquids.liquids.iter().enumerate() {
                if *amount > 0.0 {
                    let id = LiquidId::new(index as u16);
                    if let Some(liquid) = ctx.content.liquid(id)
                        && liquid.coolant
                    {
                        capacity = capacity.max(liquid.heat_capacity);
                    }
                }
            }
            if capacity > 0.0 {
                state.reload_counter +=
                    config.coolant_amount * capacity * config.coolant_multiplier;
            }
        }
    }
}

fn charging(state: &TurretState) -> bool {
    state.charge > 0.0 && state.config.shoot.first_shot_delay > 0.0
}

/// `TurretBuild.updateShooting`.
fn update_shooting(ctx: &mut CombatCtx<'_>, e: Entity, state: &mut TurretState) {
    if state.reload_counter >= state.config.reload && state.shoot_warmup >= state.config.min_warmup
    {
        if let Some(bullet) = peek_ammo_state(state, ctx.world, e) {
            shoot(ctx, e, state, bullet);
        }
        let reload = state.config.reload.max(0.0001);
        state.reload_counter %= reload;
    }
}

/// `TurretBuild.shoot`.
fn shoot(ctx: &mut CombatCtx<'_>, e: Entity, state: &mut TurretState, bullet: BulletId) {
    let config = state.config.clone();
    let (x, y) = position(ctx.world, e).unwrap_or((0.0, 0.0));
    let team = ctx.team(e);
    let rotation = state.rotation;

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

    let def_inaccuracy = ctx
        .content
        .bullet(bullet)
        .map(|def| def.inaccuracy)
        .unwrap_or(0.0);
    let scale_life = ctx
        .content
        .bullet(bullet)
        .map(|def| def.scale_life)
        .unwrap_or(false);
    let bullet_range = ctx
        .content
        .bullet(bullet)
        .map(|def| def.range)
        .unwrap_or(0.0);

    for shot in &buffer.shots {
        if !config.consume_ammo_once && !has_ammo_state(state, ctx.world, e) {
            break;
        }
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
            let dst =
                ((state.target_pos.0 - bx).powi(2) + (state.target_pos.1 - by).powi(2)).sqrt();
            ((base_life + config.scale_lifetime_offset) * dst / bullet_range).clamp(0.0, 1.0)
        } else {
            base_life
        };
        let velocity_scl = (1.0 - config.velocity_rnd)
            + ctx
                .rng
                .range(crate::determinism::RngStream::Sim, 0.0, config.velocity_rnd)
            + config.extra_velocity;
        let spawn = BulletSpawn {
            def: bullet,
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
        state.total_shots += 1;
        if !config.consume_ammo_once {
            let _ = use_ammo(state, ctx.world, e);
        }
    }

    if config.consume_ammo_once {
        let _ = use_ammo(state, ctx.world, e);
    }

    state.cur_recoil = 1.0;
    if config.recoils > 0 {
        let count = config.recoils as usize;
        if state.cur_recoils.len() >= count {
            state.cur_recoils[state.barrel_counter.rem_euclid(count as i32) as usize] = 1.0;
        }
    }
    state.heat = 1.0;
}

/// `TurretBuild.findTarget` (closest enemy building/unit; plan 11 replaces the
/// scan with `TargetQueries`).
fn find_target(ctx: &CombatCtx<'_>, e: Entity, state: &mut TurretState, team: u8, x: f32, y: f32) {
    let range = state.config.range;
    let mut best: Option<(f32, Entity)> = None;
    for entity_ref in ctx.world.iter_entities() {
        let other = entity_ref.id();
        if other == e {
            continue;
        }
        let is_unit = entity_ref.contains::<crate::entities::comp::Unit>();
        let is_building = entity_ref.contains::<crate::entities::comp::Building>();
        if !is_unit && !is_building {
            continue;
        }
        if (is_unit && !state.config.target_air) || (is_building && !state.config.target_ground) {
            continue;
        }
        if !entity_ref.contains::<Health>() {
            continue;
        }
        if entity_ref.get::<TeamComp>().map(|t| t.team) == Some(team) {
            continue;
        }
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let dst2 = (pos.x - x).powi(2) + (pos.y - y).powi(2);
        if dst2 > range * range {
            continue;
        }
        let better = best.is_none_or(|(current, current_entity)| {
            dst2 < current || (dst2 == current && other.index() < current_entity.index())
        });
        if better {
            best = Some((dst2, other));
        }
    }
    state.target = best.map(|(_, entity)| entity);
    if let Some(target) = state.target {
        state.target_pos = position(ctx.world, target).unwrap_or((x, y));
    }
}

/// `TurretBuild.validateTarget`.
fn validate_target(ctx: &CombatCtx<'_>, _e: Entity, state: &TurretState) -> bool {
    let Some(target) = state.target else {
        return false;
    };
    if ctx.world.get_entity(target).is_err() {
        return false;
    }
    if ctx.world.get::<Health>(target).is_none() {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

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
}

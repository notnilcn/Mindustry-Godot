// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! M5/M6 advanced turrets and the remaining vanilla turret ammo.
//!
//! Ported from `world/blocks/defense/turrets/{ContinuousTurret,LaserTurret,
//! PointDefenseTurret,TractorBeamTurret}.java` plus the `Blocks.java` turret
//! `ammo(...)` tables that were not part of the M5 core (`duo`). The engine
//! dispatches on [`super::TurretKind`] and keeps the same deterministic,
//! harness-driven contract as the core pipeline (plan 10 §3.2 fallback).
//!
//! ## Intentional deviations
//! - `PointDefenseTurret`/`TractorBeamTurret` models keep the upstream target
//!   selection order but replace `Groups.bullet.intersect` / `Units.closestEnemy`
//!   with stable `EntitySeq` scans until plan 05's spatial groups and plan 11's
//!   `TargetQueries` land.
//! - `TractorBeamTurret` cannot distinguish air/ground units yet (plan 11 owns
//!   the unit type flags); a unit is treated as ground, so `parallax` sets both
//!   target flags until then (recorded in the plan Changelog).

use std::collections::BTreeMap;

use bevy_ecs::entity::Entity;

use crate::combat::bullet::{Bullet, BulletSpawn, CombatCtx};
use crate::content::registries::bullets::BulletDef;
use crate::content::registries::units::weapon::ShootPatternSpec;
use crate::content::{BulletId, BulletKind, ContentRegistry, LiquidId, StatusId};
use crate::entities::comp::{Health, Pos, TeamComp, Unit, Vel};

use super::{BeamEntry, ItemAmmo, LiquidAmmo, TurretAmmo, TurretConfig, TurretKind, TurretState};

// ---------------------------------------------------------------------------
// Content: remaining vanilla turret ammo (`Blocks.java`)
// ---------------------------------------------------------------------------

/// Registers the M5/M6 turret ammo fixture bullets (`Blocks.java` `ammoTypes`).
pub fn register_bullets(content: &mut ContentRegistry, names: &mut BTreeMap<String, BulletId>) {
    let mut add = |name: &str, kind: BulletKind, configure: &dyn Fn(&mut BulletDef)| {
        let mut def = BulletDef::new(kind);
        configure(&mut def);
        if let Ok(id) = content.add_bullet(def) {
            names.insert(name.to_owned(), id);
        }
    };

    // `scatter` (FlakBulletType).
    add("scatter_scrap", BulletKind::Flak, &|def| {
        def.speed = 4.0;
        def.lifetime = 60.0;
        def.damage = 3.0;
        def.ammo_multiplier = 5.0;
        def.reload_multiplier = 0.5;
        def.splash_damage = 33.0;
        def.splash_damage_radius = 24.0;
        def.hit_size = 4.0;
        def.width = 6.0;
        def.height = 8.0;
    });
    add("scatter_lead", BulletKind::Flak, &|def| {
        def.speed = 4.2;
        def.lifetime = 60.0;
        def.damage = 3.0;
        def.ammo_multiplier = 4.0;
        def.splash_damage = 40.5;
        def.splash_damage_radius = 15.0;
        def.hit_size = 4.0;
        def.width = 6.0;
        def.height = 8.0;
    });
    add("scatter_glass", BulletKind::Flak, &|def| {
        def.speed = 4.0;
        def.lifetime = 60.0;
        def.damage = 3.0;
        def.ammo_multiplier = 5.0;
        def.reload_multiplier = 0.8;
        def.splash_damage = 45.0;
        def.splash_damage_radius = 20.0;
        def.frag_bullets = 6;
        def.hit_size = 4.0;
        def.width = 6.0;
        def.height = 8.0;
    });
    add("scatter_frag", BulletKind::Basic, &|def| {
        def.speed = 3.0;
        def.lifetime = 20.0;
        def.damage = 5.0;
        def.collides_ground = false;
        def.hit_size = 3.0;
        def.width = 5.0;
        def.height = 12.0;
    });

    // `hail` (ArtilleryBulletType, targetAir = false).
    add("hail_graphite", BulletKind::Artillery, &|def| {
        def.speed = 3.0;
        def.lifetime = 80.0;
        def.damage = 20.0;
        def.knockback = 0.8;
        def.collides_tiles = false;
        def.splash_damage_radius = 18.75;
        def.splash_damage = 33.0;
        def.hit_size = 5.5;
    });
    add("hail_silicon", BulletKind::Artillery, &|def| {
        def.speed = 3.0;
        def.lifetime = 80.0;
        def.damage = 20.0;
        def.knockback = 0.8;
        def.collides_tiles = false;
        def.splash_damage_radius = 18.75;
        def.splash_damage = 33.0;
        def.reload_multiplier = 1.2;
        def.ammo_multiplier = 3.0;
        def.homing_power = 0.08;
        def.homing_range = 50.0;
        def.hit_size = 5.5;
    });
    add("hail_pyratite", BulletKind::Artillery, &|def| {
        def.speed = 3.0;
        def.lifetime = 80.0;
        def.damage = 25.0;
        def.knockback = 0.8;
        def.collides_tiles = false;
        def.splash_damage_radius = 18.75;
        def.splash_damage = 45.0;
        def.ammo_multiplier = 4.0;
        def.make_fire = true;
        def.hit_size = 6.5;
    });

    // `salvo` (BasicBulletType).
    add("salvo_copper", BulletKind::Basic, &|def| {
        def.speed = 2.5;
        def.lifetime = 60.0;
        def.damage = 15.0;
        def.ammo_multiplier = 5.0;
        def.armor_multiplier = 1.5;
        def.hit_size = 4.0;
    });
    add("salvo_graphite", BulletKind::Basic, &|def| {
        def.speed = 3.5;
        def.lifetime = 60.0;
        def.damage = 31.0;
        def.ammo_multiplier = 4.0;
        def.reload_multiplier = 0.8;
        def.hit_size = 4.0;
        def.width = 9.0;
        def.height = 12.0;
    });
    add("salvo_pyratite", BulletKind::Basic, &|def| {
        def.speed = 3.2;
        def.lifetime = 60.0;
        def.damage = 25.0;
        def.ammo_multiplier = 5.0;
        def.splash_damage = 15.0;
        def.splash_damage_radius = 22.0;
        def.make_fire = true;
        def.status = StatusId::NONE;
        def.hit_size = 4.0;
    });
    add("salvo_silicon", BulletKind::Basic, &|def| {
        def.speed = 3.0;
        def.lifetime = 60.0;
        def.damage = 23.0;
        def.homing_power = 0.2;
        def.reload_multiplier = 1.5;
        def.ammo_multiplier = 5.0;
        def.hit_size = 4.0;
    });
    add("salvo_thorium", BulletKind::Basic, &|def| {
        def.speed = 4.0;
        def.lifetime = 60.0;
        def.damage = 28.0;
        def.ammo_multiplier = 4.0;
        def.armor_multiplier = 0.8;
        def.hit_size = 4.0;
        def.width = 8.0;
        def.height = 13.0;
    });

    // `swarmer` (MissileBulletType).
    add("swarmer_blast", BulletKind::Missile, &|def| {
        def.speed = 3.7;
        def.lifetime = 60.0;
        def.damage = 10.0;
        def.splash_damage_radius = 30.0;
        def.splash_damage = 45.0;
        def.ammo_multiplier = 5.0;
        def.status = StatusId::NONE;
        def.hit_size = 4.0;
    });
    add("swarmer_pyratite", BulletKind::Missile, &|def| {
        def.speed = 3.7;
        def.lifetime = 60.0;
        def.damage = 12.0;
        def.homing_power = 0.08;
        def.splash_damage_radius = 20.0;
        def.splash_damage = 45.0;
        def.make_fire = true;
        def.ammo_multiplier = 5.0;
        def.status = StatusId::NONE;
        def.hit_size = 4.0;
    });
    add("swarmer_surge", BulletKind::Missile, &|def| {
        def.speed = 3.7;
        def.lifetime = 60.0;
        def.damage = 18.0;
        def.splash_damage_radius = 25.0;
        def.splash_damage = 35.0;
        def.ammo_multiplier = 4.0;
        def.lightning = 2;
        def.lightning_length = 10;
        def.lightning_damage = 10.0;
        def.hit_size = 4.0;
    });

    // `fuse` (ShrapnelBulletType).
    add("fuse_titanium", BulletKind::Shrapnel, &|def| {
        def.speed = 0.0;
        def.lifetime = 10.0;
        def.length = 100.0;
        def.damage = 66.0;
        def.ammo_multiplier = 4.0;
        def.reload_multiplier = 1.3;
        def.width = 17.0;
    });
    add("fuse_thorium", BulletKind::Shrapnel, &|def| {
        def.speed = 0.0;
        def.lifetime = 10.0;
        def.length = 100.0;
        def.damage = 105.0;
        def.ammo_multiplier = 5.0;
        def.width = 17.0;
    });

    // `ripple` (ArtilleryBulletType, targetAir = false).
    add("ripple_graphite", BulletKind::Artillery, &|def| {
        def.speed = 3.0;
        def.lifetime = 80.0;
        def.damage = 40.0;
        def.knockback = 0.8;
        def.collides_tiles = false;
        def.splash_damage_radius = 22.5;
        def.splash_damage = 70.0;
        def.hit_size = 6.0;
    });
    add("ripple_silicon", BulletKind::Artillery, &|def| {
        def.speed = 3.0;
        def.lifetime = 80.0;
        def.damage = 40.0;
        def.knockback = 0.8;
        def.collides_tiles = false;
        def.splash_damage_radius = 22.5;
        def.splash_damage = 70.0;
        def.reload_multiplier = 1.2;
        def.ammo_multiplier = 3.0;
        def.homing_power = 0.08;
        def.homing_range = 50.0;
        def.hit_size = 6.0;
    });
    add("ripple_pyratite", BulletKind::Artillery, &|def| {
        def.speed = 3.0;
        def.lifetime = 80.0;
        def.damage = 48.0;
        def.knockback = 0.8;
        def.collides_tiles = false;
        def.splash_damage_radius = 22.5;
        def.splash_damage = 90.0;
        def.ammo_multiplier = 4.0;
        def.make_fire = true;
        def.hit_size = 6.5;
    });
    add("ripple_blast", BulletKind::Artillery, &|def| {
        def.speed = 2.0;
        def.lifetime = 80.0;
        def.damage = 40.0;
        def.knockback = 0.8;
        def.collides_tiles = false;
        def.ammo_multiplier = 4.0;
        def.splash_damage_radius = 37.5;
        def.splash_damage = 90.0;
        def.status = StatusId::NONE;
        def.hit_size = 7.0;
    });

    // `wave` (LiquidBulletType). The liquid is patched by name below.
    for (name, damage) in [
        ("wave_water", 0.0f32),
        ("wave_slag", 4.0),
        ("wave_cryofluid", 0.0),
        ("wave_oil", 0.0),
    ] {
        add(name, BulletKind::Liquid, &|def| {
            def.speed = 4.0;
            def.lifetime = 40.0;
            def.damage = damage;
            def.knockback = 0.7;
            def.drag = 0.01;
            def.hit_size = 3.0;
            def.puddle_liquid = LiquidId::WATER;
        });
    }

    // `tsunami` (LiquidBulletType). The liquid is patched by name below.
    for (name, damage) in [
        ("tsunami_water", 0.2f32),
        ("tsunami_slag", 4.75),
        ("tsunami_cryofluid", 0.2),
        ("tsunami_oil", 0.2),
    ] {
        add(name, BulletKind::Liquid, &|def| {
            def.speed = 4.0;
            def.lifetime = 49.0;
            def.damage = damage;
            def.knockback = 1.7;
            def.drag = 0.001;
            def.ammo_multiplier = 0.4;
            def.hit_size = 4.0;
            def.puddle_liquid = LiquidId::WATER;
        });
    }

    // `lancer` (LaserBulletType).
    add("lancer_laser", BulletKind::Laser, &|def| {
        def.speed = 0.0;
        def.lifetime = 16.0;
        def.damage = 140.0;
        def.length = 173.0;
        def.building_damage_multiplier = 0.25;
        def.armor_multiplier = 4.0;
        def.pierce_cap = 4;
        def.collides_air = false;
        def.hit_size = 4.0;
        def.large_hit = true;
    });

    // `arc` (LightningBulletType).
    add("arc_lightning", BulletKind::Lightning, &|def| {
        def.speed = 0.0001;
        def.lifetime = 10.0;
        def.damage = 12.0;
        def.lightning_length = 25;
        def.collides_air = false;
        def.building_damage_multiplier = 0.4;
        def.pierce_cap = 5;
        def.shield_damage_multiplier = 0.2;
    });

    // Placeholder ammo for the beam/interceptor turrets (no bullet is fired).
    add("tractor_dummy", BulletKind::Empty, &|def| {
        def.lifetime = 1.0;
    });
    add("pointdefense_dummy", BulletKind::Empty, &|def| {
        def.lifetime = 1.0;
    });

    // Cross-references (child defs must exist first).
    if let (Some(parent), Some(child)) = (names.get("scatter_glass"), names.get("scatter_frag"))
        && let Some(def) = content.bullet_mut(*parent)
    {
        def.frag_bullet = Some(*child);
    }

    // Status/liquid references resolve against the plan-02 registries (the
    // closure above cannot borrow `content` immutably).
    let status_refs: [(&str, &str); 4] = [
        ("salvo_pyratite", "burning"),
        ("swarmer_blast", "blasted"),
        ("swarmer_pyratite", "burning"),
        ("ripple_blast", "blasted"),
    ];
    for (bullet_name, status_name) in status_refs {
        if let (Some(id), Some(status)) = (
            names.get(bullet_name).copied(),
            content.status_id(status_name),
        ) && let Some(def) = content.bullet_mut(id)
        {
            def.status = status;
        }
    }
    let liquid_refs: [(&str, &str); 8] = [
        ("wave_water", "water"),
        ("wave_slag", "slag"),
        ("wave_cryofluid", "cryofluid"),
        ("wave_oil", "oil"),
        ("tsunami_water", "water"),
        ("tsunami_slag", "slag"),
        ("tsunami_cryofluid", "cryofluid"),
        ("tsunami_oil", "oil"),
    ];
    for (bullet_name, liquid_name) in liquid_refs {
        if let (Some(id), Some(liquid)) = (
            names.get(bullet_name).copied(),
            content.liquid_id(liquid_name),
        ) && let Some(def) = content.bullet_mut(id)
        {
            def.puddle_liquid = liquid;
        }
    }
}

// ---------------------------------------------------------------------------
// Configs
// ---------------------------------------------------------------------------

/// Base config with neutral defaults (`Turret` constructor defaults).
#[allow(clippy::too_many_lines)]
pub fn base_config(kind: TurretKind, ammo: TurretAmmo) -> TurretConfig {
    TurretConfig {
        ammo,
        range: 0.0,
        reload: 10.0,
        max_ammo: 0,
        ammo_per_shot: 1,
        consume_ammo_once: true,
        rotate_speed: 10.0,
        shoot_cone: 15.0,
        min_warmup: 0.0,
        shoot_warmup_speed: 0.1,
        linear_warmup: false,
        warmup_maintain_time: 0.0,
        target_interval: 20.0,
        inaccuracy: 0.0,
        velocity_rnd: 0.0,
        extra_velocity: 0.0,
        life_rnd: 0.0,
        extra_life: 0.0,
        scale_lifetime_offset: 0.0,
        recoil: 0.0,
        recoils: -1,
        recoil_time: 0.0,
        recoil_pow: 1.8,
        cooldown_time: 0.0,
        shoot_x: 0.0,
        shoot_y: 0.0,
        x_rand: 0.0,
        shoot: ShootPatternSpec::plain(1, 0.0, 0.0),
        target_air: true,
        target_ground: true,
        target_blocks: true,
        always_shooting: false,
        activation_time: 0.0,
        coolant_multiplier: 1.0,
        coolant_amount: 0.0,
        heat_requirement: -1.0,
        max_heat_efficiency: 3.0,
        kind,
        retarget_time: 5.0,
        shoot_length: 0.0,
        aim_change_speed: f32::INFINITY,
        scale_damage_efficiency: false,
        bullet_damage: 0.0,
        force: 0.0,
        scaled_force: 0.0,
        beam_damage: 0.0,
        status: StatusId::NONE,
        status_duration: 0.0,
        status_chance: 0.0,
        shoot_duration: 0.0,
        firing_move_fract: 1.0,
    }
}

/// Builds the resolved [`TurretConfig`] for an M5/M6 turret name.
pub fn config_for(
    content: &ContentRegistry,
    name: &str,
    names: &BTreeMap<String, BulletId>,
) -> Option<TurretConfig> {
    let bullet = |n: &str| names.get(n).copied();
    let item_ammo = |item_name: &str, bullet_name: &str| -> Option<ItemAmmo> {
        let id = bullet(bullet_name)?;
        let mult = content
            .bullet(id)
            .map(|d| d.ammo_multiplier as i32)
            .unwrap_or(1);
        Some(ItemAmmo {
            item: content.item_id(item_name)?,
            bullet: id,
            ammo_multiplier: mult.max(1),
        })
    };
    let liquid_ammo = |liquid_name: &str, bullet_name: &str| -> Option<LiquidAmmo> {
        let id = bullet(bullet_name)?;
        let mult = content.bullet(id).map(|d| d.ammo_multiplier).unwrap_or(1.0);
        Some(LiquidAmmo {
            liquid: content.liquid_id(liquid_name)?,
            bullet: id,
            ammo_multiplier: mult.max(0.001),
        })
    };
    let power =
        |bullet_name: &str| -> Option<TurretAmmo> { Some(TurretAmmo::Power(bullet(bullet_name)?)) };

    match name {
        "scatter" => {
            let mut c = base_config(
                TurretKind::Item,
                TurretAmmo::Item(vec![
                    item_ammo("scrap", "scatter_scrap")?,
                    item_ammo("lead", "scatter_lead")?,
                    item_ammo("metaglass", "scatter_glass")?,
                ]),
            );
            c.range = 220.0;
            c.reload = 18.0;
            c.rotate_speed = 15.0;
            c.inaccuracy = 17.0;
            c.shoot_cone = 35.0;
            c.target_ground = false;
            c.shoot = ShootPatternSpec::plain(2, 5.0, 0.0);
            c.recoil = 1.0;
            Some(c)
        }
        "hail" => {
            let mut c = base_config(
                TurretKind::Item,
                TurretAmmo::Item(vec![
                    item_ammo("graphite", "hail_graphite")?,
                    item_ammo("silicon", "hail_silicon")?,
                    item_ammo("pyratite", "hail_pyratite")?,
                ]),
            );
            c.range = 235.0;
            c.reload = 60.0;
            c.rotate_speed = 10.0;
            c.inaccuracy = 1.0;
            c.shoot_cone = 10.0;
            c.target_air = false;
            c.recoil = 2.0;
            Some(c)
        }
        "salvo" => {
            let mut c = base_config(
                TurretKind::Item,
                TurretAmmo::Item(vec![
                    item_ammo("copper", "salvo_copper")?,
                    item_ammo("graphite", "salvo_graphite")?,
                    item_ammo("pyratite", "salvo_pyratite")?,
                    item_ammo("silicon", "salvo_silicon")?,
                    item_ammo("thorium", "salvo_thorium")?,
                ]),
            );
            c.range = 190.0;
            c.reload = 29.0;
            c.consume_ammo_once = false;
            c.recoil = 0.0;
            c.shoot = ShootPatternSpec::plain(4, 3.0, 0.0);
            c.rotate_speed = 10.0;
            Some(c)
        }
        "swarmer" => {
            let mut c = base_config(
                TurretKind::Item,
                TurretAmmo::Item(vec![
                    item_ammo("blast-compound", "swarmer_blast")?,
                    item_ammo("pyratite", "swarmer_pyratite")?,
                    item_ammo("surge-alloy", "swarmer_surge")?,
                ]),
            );
            c.range = 240.0;
            c.reload = 60.0 * 4.0 / 7.0;
            c.consume_ammo_once = false;
            c.rotate_speed = 4.0;
            c.inaccuracy = 10.0;
            c.shoot_y = 4.5;
            c.shoot = ShootPatternSpec::barrel(
                vec![[-4.0, -1.25, 0.0], [0.0, 0.0, 0.0], [4.0, -1.25, 0.0]],
                0,
            );
            c.shoot.shots = 4;
            c.shoot.shot_delay = 5.0;
            Some(c)
        }
        "fuse" => {
            let mut c = base_config(
                TurretKind::Item,
                TurretAmmo::Item(vec![
                    item_ammo("titanium", "fuse_titanium")?,
                    item_ammo("thorium", "fuse_thorium")?,
                ]),
            );
            c.range = 90.0;
            c.reload = 35.0;
            c.recoil = 5.0;
            c.shoot_cone = 30.0;
            c.shoot = ShootPatternSpec::spread(3, 20.0);
            c.coolant_amount = 0.3;
            Some(c)
        }
        "ripple" => {
            let mut c = base_config(
                TurretKind::Item,
                TurretAmmo::Item(vec![
                    item_ammo("graphite", "ripple_graphite")?,
                    item_ammo("silicon", "ripple_silicon")?,
                    item_ammo("pyratite", "ripple_pyratite")?,
                    item_ammo("blast-compound", "ripple_blast")?,
                ]),
            );
            c.range = 290.0;
            c.reload = 120.0;
            c.ammo_per_shot = 2;
            c.inaccuracy = 11.0;
            c.velocity_rnd = 0.2;
            c.recoil = 6.0;
            c.target_air = false;
            c.scale_lifetime_offset = 0.1;
            c.shoot = ShootPatternSpec::plain(4, 0.0, 0.0);
            Some(c)
        }
        "wave" => {
            let mut c = base_config(
                TurretKind::Liquid,
                TurretAmmo::Liquid(vec![
                    liquid_ammo("water", "wave_water")?,
                    liquid_ammo("slag", "wave_slag")?,
                    liquid_ammo("cryofluid", "wave_cryofluid")?,
                    liquid_ammo("oil", "wave_oil")?,
                ]),
            );
            c.range = 110.0;
            c.reload = 3.0;
            c.rotate_speed = 10.0;
            c.inaccuracy = 5.0;
            c.shoot_cone = 50.0;
            c.recoil = 0.0;
            Some(c)
        }
        "tsunami" => {
            let mut c = base_config(
                TurretKind::Liquid,
                TurretAmmo::Liquid(vec![
                    liquid_ammo("water", "tsunami_water")?,
                    liquid_ammo("slag", "tsunami_slag")?,
                    liquid_ammo("cryofluid", "tsunami_cryofluid")?,
                    liquid_ammo("oil", "tsunami_oil")?,
                ]),
            );
            c.range = 190.0;
            c.reload = 3.0;
            c.rotate_speed = 10.0;
            c.inaccuracy = 3.0;
            c.shoot_cone = 45.0;
            c.velocity_rnd = 0.1;
            c.recoil = 1.0;
            c.shoot = ShootPatternSpec::alternate(2, 0.0, 4.0, 2);
            Some(c)
        }
        "lancer" => {
            let mut c = base_config(TurretKind::Power, power("lancer_laser")?);
            c.range = 165.0;
            c.reload = 80.0;
            c.recoil = 2.0;
            c.target_air = false;
            c.shoot.first_shot_delay = 40.0;
            c.coolant_amount = 0.2;
            c.coolant_multiplier = 10.0;
            Some(c)
        }
        "arc" => {
            let mut c = base_config(TurretKind::Power, power("arc_lightning")?);
            c.range = 90.0;
            c.reload = 35.0;
            c.shoot_cone = 40.0;
            c.rotate_speed = 8.0;
            c.target_air = false;
            c.recoil = 1.0;
            c.coolant_amount = 0.1;
            c.coolant_multiplier = 30.0;
            Some(c)
        }
        "parallax" => {
            let mut c = base_config(TurretKind::TractorBeam, power("tractor_dummy")?);
            c.range = 300.0;
            c.rotate_speed = 12.0;
            c.shoot_cone = 6.0;
            c.shoot_length = 5.0;
            c.retarget_time = 5.0;
            c.force = 16.0;
            c.scaled_force = 9.0;
            c.beam_damage = 0.5;
            // Deviation: units have no air/ground flag until plan 11.
            c.target_air = true;
            c.target_ground = true;
            Some(c)
        }
        "segment" => {
            let mut c = base_config(TurretKind::PointDefense, power("pointdefense_dummy")?);
            c.range = 180.0;
            c.reload = 8.0;
            c.rotate_speed = 20.0;
            c.shoot_cone = 5.0;
            c.shoot_length = 5.0;
            c.bullet_damage = 30.0;
            c.retarget_time = 5.0;
            c.coolant_multiplier = 2.0;
            Some(c)
        }
        // Harness fixture: a continuous beam using the `continuous` fixture
        // bullet (mirrors `meltdown`'s class, no vanilla ammo port yet).
        "test-continuous" => {
            let mut c = base_config(TurretKind::Continuous, power("continuous")?);
            c.range = 120.0;
            c.reload = 0.0;
            c.rotate_speed = 20.0;
            c.target_interval = 5.0;
            c.shoot_warmup_speed = 0.5;
            Some(c)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Behavior
// ---------------------------------------------------------------------------

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

fn lerp_delta(from: f32, to: f32, alpha: f32) -> f32 {
    from + (to - from) * alpha.clamp(0.0, 1.0)
}

/// `PointDefenseBuild.updateTile` (M6).
#[allow(clippy::too_many_arguments)]
pub fn update_point_defense(
    ctx: &mut CombatCtx<'_>,
    _e: Entity,
    state: &mut TurretState,
    config: &TurretConfig,
    x: f32,
    y: f32,
    team: u8,
    eff: f32,
) {
    state.retarget_timer -= 1.0;
    if state.retarget_timer <= 0.0 {
        state.bullet_target = find_enemy_bullet(ctx, x, y, config.range, team);
        state.retarget_timer = config.retarget_time;
    }
    let Some(target) = state.bullet_target else {
        return;
    };
    if ctx.world.get_entity(target).is_err() || ctx.world.get::<Bullet>(target).is_none() {
        state.bullet_target = None;
        return;
    }
    let Some(tpos) = ctx.world.get::<Pos>(target).map(|p| (p.x, p.y)) else {
        state.bullet_target = None;
        return;
    };
    let hittable = ctx
        .world
        .get::<Bullet>(target)
        .and_then(|b| ctx.content.bullet(b.def))
        .map(|d| d.hittable)
        .unwrap_or(false);
    if !hittable {
        return;
    }
    let dest = angle_to(x, y, tpos.0, tpos.1);
    state.rotation = move_toward(state.rotation, dest, config.rotate_speed * eff);
    state.reload_counter += eff;
    if angle_dist(state.rotation, dest) < config.shoot_cone && state.reload_counter >= config.reload
    {
        let real = config.bullet_damage;
        let lethal = ctx
            .world
            .get::<Bullet>(target)
            .map(|b| b.damage <= real)
            .unwrap_or(true);
        if lethal {
            let _ = ctx.world.despawn(target);
        } else if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(target) {
            bullet.damage -= real;
        }
        state.reload_counter = 0.0;
        state.total_shots += 1;
        state.any = true;
    }
}

/// Closest enemy hittable bullet within `range` (stable tie-break by index).
fn find_enemy_bullet(ctx: &CombatCtx<'_>, x: f32, y: f32, range: f32, team: u8) -> Option<Entity> {
    let range2 = range * range;
    let mut best: Option<(f32, Entity)> = None;
    for entity_ref in ctx.world.iter_entities() {
        let other = entity_ref.id();
        let Some(bullet) = entity_ref.get::<Bullet>() else {
            continue;
        };
        if entity_ref.get::<TeamComp>().map(|t| t.team) == Some(team) {
            continue;
        }
        if !ctx
            .content
            .bullet(bullet.def)
            .map(|d| d.hittable)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let dst2 = (pos.x - x).powi(2) + (pos.y - y).powi(2);
        if dst2 > range2 {
            continue;
        }
        let better = best.is_none_or(|(current, current_entity)| {
            dst2 < current || (dst2 == current && other.index() < current_entity.index())
        });
        if better {
            best = Some((dst2, other));
        }
    }
    best.map(|(_, entity)| entity)
}

/// `TractorBeamBuild.updateTile` (M6).
#[allow(clippy::too_many_arguments)]
pub fn update_tractor(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    config: &TurretConfig,
    x: f32,
    y: f32,
    team: u8,
    eff: f32,
) {
    state.retarget_timer -= 1.0;
    if state.retarget_timer <= 0.0 {
        state.unit_target = find_enemy_unit(ctx, x, y, config.range, team, config);
        state.retarget_timer = config.retarget_time;
    }
    let Some(target) = state.unit_target else {
        state.strength = lerp_delta(state.strength, 0.0, 0.1);
        state.any = false;
        return;
    };
    let Some((tx, ty)) = ctx.world.get::<Pos>(target).map(|p| (p.x, p.y)) else {
        state.unit_target = None;
        state.any = false;
        return;
    };
    let hit_size = ctx
        .world
        .get::<Health>(target)
        .map(|_| 4.0f32)
        .unwrap_or(4.0);
    let dst2 = (tx - x).powi(2) + (ty - y).powi(2);
    let max_dst = config.range + hit_size / 2.0;
    state.any = false;
    if dst2 > max_dst * max_dst || eff <= 0.02 {
        state.strength = lerp_delta(state.strength, 0.0, 0.1);
        return;
    }
    let dest = angle_to(x, y, tx, ty);
    state.rotation = move_toward(state.rotation, dest, config.rotate_speed * eff);
    state.target_pos = (tx, ty);
    state.strength = lerp_delta(state.strength, 1.0, 0.1);
    if angle_dist(state.rotation, dest) < config.shoot_cone {
        if config.beam_damage > 0.0
            && let Some(mut health) = ctx.world.get_mut::<Health>(target)
        {
            health.health -= config.beam_damage * eff;
        }
        if config.status != StatusId::NONE
            && (config.status_chance <= 0.0 || state.strength >= config.status_chance)
        {
            crate::combat::damage::status::apply_status(
                ctx.world,
                target,
                config.status,
                config.status_duration,
            );
        }
        // `impulseNet`: push the unit away from the turret, clamped.
        let dir = ((x - tx), (y - ty));
        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
        if len > 0.0001 {
            let dist = len;
            let mag =
                (config.force + (1.0 - (dist / config.range).min(1.0)) * config.scaled_force) * eff;
            let nx = dir.0 / len;
            let ny = dir.1 / len;
            if let Some(mut vel) = ctx.world.get_mut::<Vel>(target) {
                vel.x += nx * mag;
                vel.y += ny * mag;
            }
        }
        state.any = true;
        state.total_shots += 1;
    }
    let _ = e;
}

/// Closest enemy unit matching the target flags (stable tie-break).
fn find_enemy_unit(
    ctx: &CombatCtx<'_>,
    x: f32,
    y: f32,
    range: f32,
    team: u8,
    config: &TurretConfig,
) -> Option<Entity> {
    if !config.target_air && !config.target_ground {
        return None;
    }
    let range2 = range * range;
    let mut best: Option<(f32, Entity)> = None;
    for entity_ref in ctx.world.iter_entities() {
        let other = entity_ref.id();
        if entity_ref.get::<Unit>().is_none() {
            continue;
        }
        if entity_ref.get::<TeamComp>().map(|t| t.team) == Some(team) {
            continue;
        }
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let dst2 = (pos.x - x).powi(2) + (pos.y - y).powi(2);
        if dst2 > range2 {
            continue;
        }
        let better = best.is_none_or(|(current, current_entity)| {
            dst2 < current || (dst2 == current && other.index() < current_entity.index())
        });
        if better {
            best = Some((dst2, other));
        }
    }
    best.map(|(_, entity)| entity)
}

/// `ContinuousTurretBuild.updateTile` (M6): keep-alive beam that follows the aim.
#[allow(clippy::too_many_arguments)]
pub fn update_continuous(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    config: &TurretConfig,
    x: f32,
    y: f32,
    team: u8,
    eff: f32,
) {
    // Drop dead/foreign beams (`bullets.removeAll(...)`).
    state
        .bullets
        .retain(|entry| ctx.world.get_entity(entry.bullet).is_ok());

    if !state.bullets.is_empty() {
        let aim = state.target_pos;
        let dst = ((aim.0 - x).powi(2) + (aim.1 - y).powi(2)).sqrt();
        let shoot_length = dst.min(config.range);
        let mut last_length = state.last_length;
        for index in 0..state.bullets.len() {
            let entry = state.bullets[index];
            let bullet_x = x + trnsx(
                state.rotation - 90.0,
                config.shoot_x + entry.x,
                config.shoot_y + entry.y,
            );
            let bullet_y = y + trnsy(
                state.rotation - 90.0,
                config.shoot_x + entry.x,
                config.shoot_y + entry.y,
            );
            let angle = state.rotation + entry.rotation;
            let cur = ctx
                .world
                .get::<Bullet>(entry.bullet)
                .map(|b| {
                    let dx = b.aim.0 - x;
                    let dy = b.aim.1 - y;
                    (dx * dx + dy * dy).sqrt()
                })
                .unwrap_or(0.0);
            let result_length = move_toward(cur, shoot_length, config.aim_change_speed.max(0.0));
            last_length = result_length;
            let aim_x = x + trnsx(state.rotation, result_length, 0.0);
            let aim_y = y + trnsy(state.rotation, result_length, 0.0);
            if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(entry.bullet) {
                bullet.rotation = angle;
                bullet.aim = (aim_x, aim_y);
                bullet.time = bullet.lifetime
                    * ctx
                        .content
                        .bullet(bullet.def)
                        .map(|d| d.optimal_life_fract)
                        .unwrap_or(1.0)
                    * state.shoot_warmup.min(eff).max(0.0);
                bullet.set(crate::combat::bullet::KEEP_ALIVE);
            }
            if let Some(mut pos) = ctx.world.get_mut::<Pos>(entry.bullet) {
                pos.x = bullet_x;
                pos.y = bullet_y;
            }
            if let Some(mut vel) = ctx.world.get_mut::<Vel>(entry.bullet) {
                vel.x = 0.0;
                vel.y = 0.0;
            }
        }
        state.last_length = last_length;
        state.was_shooting = true;
        state.heat = 1.0;
        state.cur_recoil = config.recoil;
        return;
    }

    // Target acquisition (`TurretBuild` interval timer).
    state.target_timer -= 1.0;
    if state.target_timer <= 0.0 {
        super::find_target(ctx, e, state, team, x, y);
        state.target_timer = config.target_interval;
    }
    if state.target.is_none() {
        return;
    }
    let dest = angle_to(x, y, state.target_pos.0, state.target_pos.1);
    state.rotation = move_toward(state.rotation, dest, config.rotate_speed * eff);
    if angle_dist(state.rotation, dest) >= config.shoot_cone && !config.always_shooting {
        return;
    }
    if !super::can_consume(state, ctx.world, e, config) || state.shoot_warmup < config.min_warmup {
        return;
    }
    spawn_beam(ctx, e, state, config, x, y, team, 0.0);
}

/// `LaserTurretBuild.updateTile` (M6): coolant-gated continuous beam.
#[allow(clippy::too_many_arguments)]
pub fn update_laser(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    config: &TurretConfig,
    x: f32,
    y: f32,
    team: u8,
    eff: f32,
) {
    state
        .bullets
        .retain(|entry| ctx.world.get_entity(entry.bullet).is_ok() && entry.life > 0.0);

    if !state.bullets.is_empty() {
        for index in 0..state.bullets.len() {
            let entry = state.bullets[index];
            let bullet_x = x + trnsx(
                state.rotation - 90.0,
                config.shoot_x + entry.x,
                config.shoot_y + entry.y,
            );
            let bullet_y = y + trnsy(
                state.rotation - 90.0,
                config.shoot_x + entry.x,
                config.shoot_y + entry.y,
            );
            let angle = state.rotation + entry.rotation;
            let optimal = ctx
                .content
                .bullet(
                    ctx.world
                        .get::<Bullet>(entry.bullet)
                        .map(|b| b.def)
                        .unwrap_or_default(),
                )
                .map(|d| d.optimal_life_fract)
                .unwrap_or(1.0);
            if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(entry.bullet) {
                bullet.rotation = angle;
                bullet.time = bullet.lifetime * optimal;
                bullet.set(crate::combat::bullet::KEEP_ALIVE);
            }
            if let Some(mut pos) = ctx.world.get_mut::<Pos>(entry.bullet) {
                pos.x = bullet_x;
                pos.y = bullet_y;
            }
            state.bullets[index].life -= 1.0 / eff.max(0.00001);
        }
        state.was_shooting = true;
        state.heat = 1.0;
        state.cur_recoil = config.recoil;
        return;
    }

    // Reload recharges from coolant (falling back to efficiency).
    if state.reload_counter > 0.0 {
        if config.uses_coolant()
            && let Some(mut liquids) = ctx.world.get_mut::<crate::world::modules::LiquidModule>(e)
        {
            // `Liquid.heatCapacity` of the current liquid.
            let mut capacity = 0.0f32;
            let mut current = None;
            for (index, amount) in liquids.liquids.iter().enumerate() {
                if *amount > 0.0 {
                    let id = LiquidId::new(index as u16);
                    if let Some(liquid) = ctx.content.liquid(id)
                        && liquid.coolant
                    {
                        capacity = capacity.max(liquid.heat_capacity);
                        current = Some(id);
                    }
                }
            }
            if let Some(id) = current {
                let max_used = config.coolant_amount;
                let used = liquids.get(id).min(max_used);
                liquids.remove(id, used);
                state.reload_counter -= used * capacity * config.coolant_multiplier;
            }
        } else {
            state.reload_counter -= eff;
        }
    }

    state.target_timer -= 1.0;
    if state.target_timer <= 0.0 {
        super::find_target(ctx, e, state, team, x, y);
        state.target_timer = config.target_interval;
    }
    if state.target.is_none() {
        return;
    }
    let dest = angle_to(x, y, state.target_pos.0, state.target_pos.1);
    state.rotation = move_toward(state.rotation, dest, config.rotate_speed * eff);
    if eff > 0.0
        && state.reload_counter <= 0.0
        && state.shoot_warmup >= config.min_warmup
        && (angle_dist(state.rotation, dest) < config.shoot_cone || config.always_shooting)
    {
        spawn_beam(ctx, e, state, config, x, y, team, config.shoot_duration);
        state.reload_counter = config.reload;
    }
}

/// Spawns and records one keep-alive beam bullet (`TurretBuild.shoot` +
/// `handleBullet` for the continuous/laser classes).
#[allow(clippy::too_many_arguments)]
fn spawn_beam(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    config: &TurretConfig,
    x: f32,
    y: f32,
    team: u8,
    life: f32,
) {
    let bullet = if let TurretAmmo::Power(bullet) = &config.ammo {
        *bullet
    } else {
        super::peek_ammo_state(state, ctx.world, e).unwrap_or_default()
    };
    let bx = x + trnsx(state.rotation - 90.0, config.shoot_x, config.shoot_y);
    let by = y + trnsy(state.rotation - 90.0, config.shoot_x, config.shoot_y);
    let dst = ((state.target_pos.0 - x).powi(2) + (state.target_pos.1 - y).powi(2)).sqrt();
    let result_length = dst.min(config.range);
    let aim_x = x + trnsx(state.rotation, result_length, 0.0);
    let aim_y = y + trnsy(state.rotation, result_length, 0.0);
    let spawn = BulletSpawn {
        def: bullet,
        owner: Some(e),
        shooter: Some(e),
        team,
        x: bx,
        y: by,
        angle: state.rotation,
        aim_x,
        aim_y,
        ..BulletSpawn::default()
    };
    if let Some(entity) = ctx.spawn(&spawn) {
        state.bullets.push(BeamEntry {
            bullet: entity,
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            life,
        });
        state.total_shots += 1;
        state.last_length = result_length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    fn turret_at(harness: &mut CombatHarness, name: &str) -> (Entity, Entity) {
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 8, wall, 0, true));
        let target = harness.build_at(10, 8).expect("target");
        let (tx, ty) = CombatHarness::tile_center(4, 8);
        let turret = harness
            .spawn_test_turret(name, tx, ty, 1)
            .unwrap_or_else(|| panic!("turret `{name}`"));
        (turret, target)
    }

    #[test]
    fn vanilla_turret_ammo_registers() {
        let harness = CombatHarness::new(8, 8, 1);
        for name in [
            "scatter_scrap",
            "hail_graphite",
            "salvo_thorium",
            "swarmer_surge",
            "fuse_thorium",
            "ripple_blast",
            "wave_slag",
            "tsunami_water",
            "lancer_laser",
            "arc_lightning",
        ] {
            assert!(harness.bullet_id(name).is_some(), "missing {name}");
        }
    }

    #[test]
    fn all_supported_turret_configs_resolve() {
        let harness = CombatHarness::new(8, 8, 1);
        for name in [
            "duo", "scatter", "hail", "salvo", "swarmer", "fuse", "ripple", "wave", "tsunami",
            "lancer", "arc", "parallax", "segment",
        ] {
            assert!(
                super::super::config_for(harness.content(), name, harness.names_map()).is_some(),
                "config missing for {name}"
            );
        }
    }

    #[test]
    fn point_defense_intercepts_enemy_bullet() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let (turret, _) = turret_at(&mut harness, "segment");
        // Enemy bullet flying toward the turret.
        let (bx, by) = CombatHarness::tile_center(12, 8);
        let bullet = harness
            .spawn_bullet("fuse", bx, by, 180.0, 2)
            .expect("enemy bullet");
        for _ in 0..40 {
            harness.tick();
            if harness.build.world.get_entity(bullet).is_err() {
                break;
            }
        }
        assert!(
            harness.build.world.get_entity(bullet).is_err(),
            "point defense removed the enemy bullet"
        );
        let _ = turret;
    }

    #[test]
    fn tractor_beam_damages_and_pulls_unit() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let (tx, ty) = CombatHarness::tile_center(8, 8);
        let turret = harness
            .spawn_test_turret("parallax", tx, ty, 1)
            .expect("turret");
        let (ux, uy) = CombatHarness::tile_center(12, 8);
        let unit = harness.spawn_test_unit(ux, uy, 2, Vec::new());
        let before = harness.build.world.get::<Health>(unit).unwrap().health;
        for _ in 0..120 {
            harness.tick();
        }
        let after = harness.build.world.get::<Health>(unit).unwrap().health;
        assert!(
            after < before,
            "tractor beam damaged the unit: {after} < {before}"
        );
        let strength = harness
            .build
            .world
            .get::<TurretState>(turret)
            .unwrap()
            .strength;
        assert!(strength > 0.0, "beam strength ramped up");
    }

    #[test]
    fn continuous_turret_spawns_keepalive_beam() {
        let mut harness = CombatHarness::new(48, 16, 7);
        let (turret, _) = turret_at(&mut harness, "test-continuous");
        for _ in 0..120 {
            harness.tick();
        }
        let state = harness.build.world.get::<TurretState>(turret).unwrap();
        assert!(!state.bullets.is_empty(), "continuous beam is alive");
    }
}

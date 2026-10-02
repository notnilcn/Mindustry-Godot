// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/UnitTypes.java (wave `standard`).
//
//! Generated unit metadata (`UnitTypes.java`, wave `standard`).
//! Regenerate with `parity/tools/gen_units.py` after upstream content changes.

#![allow(unused_imports)]

use super::ability::{AbilityKind, AbilitySpec};
use super::parts::{BlendingKind, DrawPartSpec, InterpKind, PartMoveSpec, PartProgressSpec};
use super::weapon::{BulletRef, ShootPatternSpec, WeaponKind, WeaponSpec};
use super::{
    AiControllerKind, ControllerKind, EngineSpec, TreadRect, UnitKind, UnitSink, entity, spec,
};
use crate::content::ContentError;
use crate::content::color::Rgba;
use crate::content::registries::blocks::{BlockFlag, EnvMask};
use crate::content::registries::bullets::{BulletKind, BulletSpec};
use crate::content::registries::fx_meta::{EffectId, EffectRef, EffectSpec};
use crate::content::registries::planets::EnvFlag;
use crate::content::registries::sound_meta::SoundId;

/// Loads the `standard` wave in upstream order.
pub fn load(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    dagger(sink)?;
    mace(sink)?;
    fortress(sink)?;
    scepter(sink)?;
    reign(sink)?;
    nova(sink)?;
    pulsar(sink)?;
    quasar(sink)?;
    vela(sink)?;
    corvus(sink)?;
    crawler(sink)?;
    atrax(sink)?;
    spiroct(sink)?;
    arkyid(sink)?;
    toxopid(sink)?;
    flare(sink)?;
    horizon(sink)?;
    zenith(sink)?;
    antumbra(sink)?;
    eclipse(sink)?;
    mono(sink)?;
    poly(sink)?;
    mega(sink)?;
    quad(sink)?;
    oct(sink)?;
    risso(sink)?;
    minke(sink)?;
    bryde(sink)?;
    sei(sink)?;
    omura(sink)?;
    retusa(sink)?;
    oxynoe(sink)?;
    cyerce(sink)?;
    aegires(sink)?;
    navanax(sink)?;
    alpha(sink)?;
    beta(sink)?;
    gamma(sink)?;
    Ok(())
}

fn dagger(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("dagger", UnitKind::UnitType, entity::MECH);
        unit.research_cost_multiplier = Some(0.5);
        unit.speed = Some(0.5);
        unit.hit_size = Some(8.0);
        unit.health = Some(150.0);
        unit.step_sound_volume = Some(0.4);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "large-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(2.5),
                    damage: Some(9.0),
                    width: Some(7.0),
                    height: Some(9.0),
                    lifetime: Some(60.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(13.0);
            weapon.x = Some(4.0);
            weapon.y = Some(2.0);
            weapon.top = Some(false);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon
        });
        unit
    })
}

fn mace(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("mace", UnitKind::UnitType, entity::MECH);
        unit.speed = Some(0.5);
        unit.hit_size = Some(10.0);
        unit.health = Some(550.0);
        unit.armor = Some(4.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "flamethrower",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    speed: Some(4.2),
                    damage: Some(74.0),
                    ammo_multiplier: Some(3.0),
                    hit_size: Some(7.0),
                    lifetime: Some(13.0),
                    pierce: Some(true),
                    pierce_building: Some(true),
                    pierce_cap: Some(2),
                    status_duration: Some(300.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL_FLAME)),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_FLAME_SMALL)),
                    despawn_effect: Some(EffectRef::Named(EffectId::NONE)),
                    status: Some("burning"),
                    keep_velocity: Some(false),
                    hittable: Some(false),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.shoot_sound = Some(SoundId::SHOOT_FLAME);
            weapon.shoot_y = Some(2.0);
            weapon.reload = Some(22.0);
            weapon.recoil = Some(1.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon
        });
        unit.immunities.push("burning");
        unit
    })
}

fn fortress(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("fortress", UnitKind::UnitType, entity::MECH);
        unit.speed = Some(0.43);
        unit.hit_size = Some(13.0);
        unit.rotate_speed = Some(3.0);
        unit.target_air = Some(false);
        unit.health = Some(900.0);
        unit.armor = Some(9.0);
        unit.mech_front_sway = Some(0.55);
        unit.step_sound = Some(SoundId::MECH_STEP_SMALL);
        unit.step_sound_pitch = Some(0.8);
        unit.step_sound_volume = Some(0.65);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "artillery",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Artillery,
                    speed: Some(2.0),
                    damage: Some(20.0),
                    sprite: Some(Some(String::from("shell"))),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    knockback: Some(0.8),
                    lifetime: Some(106.5),
                    max_range: Some(240.0),
                    width: Some(14.0),
                    height: Some(14.0),
                    collides: Some(true),
                    collides_tiles: Some(true),
                    splash_damage_radius: Some(35.0),
                    splash_damage: Some(80.0),
                    back_color: Some(Rgba::new(0.9764706, 0.7607843, 0.47843137, 1.0)),
                    front_color: Some(Rgba::new(1.0, 0.972549, 0.9098039, 1.0)),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.y = Some(1.0);
            weapon.x = Some(9.0);
            weapon.reload = Some(60.0);
            weapon.recoil = Some(4.0);
            weapon.shake = Some(2.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING2));
            weapon.shoot_sound = Some(SoundId::SHOOT_ARTILLERY);
            weapon
        });
        unit
    })
}

fn scepter(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("scepter", UnitKind::UnitType, entity::MECH);
        unit.speed = Some(0.36);
        unit.hit_size = Some(22.0);
        unit.rotate_speed = Some(2.1);
        unit.health = Some(9000.0);
        unit.armor = Some(20.0);
        unit.mech_front_sway = Some(1.0);
        unit.mech_step_particles = Some(true);
        unit.step_shake = Some(0.15);
        unit.single_target = Some(true);
        unit.drown_time_multiplier = Some(1.5);
        unit.step_sound = Some(SoundId::MECH_STEP);
        unit.step_sound_pitch = Some(0.9);
        unit.step_sound_volume = Some(0.35);
        unit.pre_bullets.push(BulletSpec {
            kind: BulletKind::Basic,
            speed: Some(12.0),
            damage: Some(20.0),
            width: Some(4.5),
            height: Some(35.0),
            lifetime: Some(17.333334),
            shrink_x: Some(0.6),
            shrink_y: Some(0.0),
            shrink_interp: Some(InterpKind::Slope),
            trail_chance: Some(0.16666667),
            trail_color: Some(Rgba::new(0.9764706, 0.7607843, 0.47843137, 1.0)),
            trail_effect: Some(EffectRef::Named(EffectId::BULLET_SPARK_SMOKE_TRAIL_SMALL)),
            trail_spread: Some(12.0),
            shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SCEPTER_SECONDARY)),
            hit_effect: Some(EffectRef::Named(EffectId::HIT_SCEPTER_SECONDARY)),
            ..BulletSpec::default()
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "scepter-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(3, 4.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(8.0),
                    damage: Some(70.0),
                    width: Some(11.0),
                    height: Some(20.0),
                    lifetime: Some(27.0),
                    shrink_x: Some(0.4),
                    shrink_y: Some(0.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG)),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    trail_param: Some(0.5),
                    lightning: Some(2),
                    lightning_length: Some(6),
                    lightning_color: Some(Rgba::new(0.9529412, 0.9137255, 0.4745098, 1.0)),
                    lightning_damage: Some(20.0),
                    despawn_sound: Some(SoundId::SHOCK_BULLET),
                    bullet_interval: Some(4.0),
                    interval_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Lightning,
                        damage: Some(5.0),
                        lightning_length: Some(3),
                        lightning_length_rand: Some(4),
                        lightning_color: Some(Rgba::new(0.9529412, 0.9137255, 0.4745098, 1.0)),
                        hit_effect: Some(EffectRef::Named(EffectId::HIT_LANCER_LOW)),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.y = Some(1.0);
            weapon.x = Some(16.0);
            weapon.shoot_y = Some(8.0);
            weapon.reload = Some(45.0);
            weapon.recoil = Some(5.0);
            weapon.shake = Some(2.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING3));
            weapon.shoot_sound = Some(SoundId::SHOOT_SCEPTER);
            weapon.shoot_sound_volume = Some(0.95);
            weapon.inaccuracy = Some(3.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "scepter-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Pre(0),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(12.0);
            weapon.x = Some(8.5);
            weapon.y = Some(6.0);
            weapon.rotate = Some(true);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.shoot_sound = Some(SoundId::SHOOT_SCEPTER_SECONDARY);
            weapon.rotate_speed = Some(3.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "scepter-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Pre(0),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(15.0);
            weapon.x = Some(8.5);
            weapon.y = Some(-7.0);
            weapon.rotate = Some(true);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.shoot_sound = Some(SoundId::SHOOT_SCEPTER_SECONDARY);
            weapon.rotate_speed = Some(3.0);
            weapon
        });
        unit.abilities
            .push(AbilitySpec::shield_regen_field(25.0, 250.0, 60.0, 60.0));
        unit
    })
}

fn reign(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("reign", UnitKind::UnitType, entity::MECH);
        unit.speed = Some(0.4);
        unit.hit_size = Some(30.0);
        unit.rotate_speed = Some(1.65);
        unit.health = Some(24000.0);
        unit.armor = Some(30.0);
        unit.mech_step_particles = Some(true);
        unit.step_shake = Some(0.75);
        unit.drown_time_multiplier = Some(1.6);
        unit.mech_front_sway = Some(1.9);
        unit.mech_side_sway = Some(0.6);
        unit.step_sound = Some(SoundId::MECH_STEP_HEAVY);
        unit.step_sound_pitch = Some(0.9);
        unit.step_sound_volume = Some(0.45);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "reign-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(13.0),
                    damage: Some(80.0),
                    pierce: Some(true),
                    pierce_cap: Some(10),
                    width: Some(14.0),
                    height: Some(33.0),
                    lifetime: Some(15.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG)),
                    frag_velocity_min: Some(0.4),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    splash_damage: Some(18.0),
                    splash_damage_radius: Some(13.0),
                    frag_bullets: Some(3),
                    frag_life_min: Some(0.0),
                    frag_random_spread: Some(30.0),
                    despawn_sound: Some(SoundId::EXPLOSION),
                    frag_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Basic,
                        speed: Some(9.0),
                        damage: Some(20.0),
                        width: Some(10.0),
                        height: Some(10.0),
                        pierce: Some(true),
                        pierce_building: Some(true),
                        pierce_cap: Some(3),
                        lifetime: Some(20.0),
                        hit_effect: Some(EffectRef::Named(EffectId::FLAK_EXPLOSION)),
                        splash_damage: Some(15.0),
                        splash_damage_radius: Some(10.0),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.y = Some(1.0);
            weapon.x = Some(21.5);
            weapon.shoot_y = Some(11.0);
            weapon.reload = Some(9.0);
            weapon.recoil = Some(5.0);
            weapon.shake = Some(2.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING4));
            weapon.shoot_sound = Some(SoundId::SHOOT_REIGN);
            weapon
        });
        unit
    })
}

fn nova(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("nova", UnitKind::UnitType, entity::MECH_LEGACY);
        unit.can_boost = Some(true);
        unit.boost_multiplier = Some(2.0);
        unit.speed = Some(0.55);
        unit.hit_size = Some(8.0);
        unit.health = Some(200.0);
        unit.build_speed = Some(0.3);
        unit.armor = Some(1.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "heal-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::LaserBolt,
                    speed: Some(5.2),
                    damage: Some(13.0),
                    lifetime: Some(30.0),
                    heal_percent: Some(5.0),
                    pierce: Some(true),
                    pierce_building: Some(true),
                    pierce_cap: Some(2),
                    collides_team: Some(true),
                    back_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.shoot_y = Some(2.0);
            weapon.reload = Some(30.0);
            weapon.x = Some(4.5);
            weapon.alternate = Some(false);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon.recoil = Some(2.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_LASER);
            weapon
        });
        unit.abilities.push({
            let mut ability = AbilitySpec::repair_field(20.0, 120.0, 100.0);
            ability.same_type_heal_mult = 0.15;
            ability.max_targets = 6;
            ability.smart_heal = true;
            ability.smart_downtime = 240.0;
            ability
        });
        unit
    })
}

fn pulsar(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("pulsar", UnitKind::UnitType, entity::MECH_LEGACY);
        unit.can_boost = Some(true);
        unit.boost_multiplier = Some(1.6);
        unit.speed = Some(0.7);
        unit.hit_size = Some(11.0);
        unit.health = Some(320.0);
        unit.build_speed = Some(0.5);
        unit.armor = Some(4.0);
        unit.rise_speed = Some(0.07);
        unit.descent_speed = Some(0.07);
        unit.mine_tier = Some(2);
        unit.mine_speed = Some(3.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "heal-shotgun-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(3, 0.5, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Lightning,
                    lightning_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    hit_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    damage: Some(15.0),
                    lightning_length: Some(8),
                    lightning_length_rand: Some(7),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_HEAL)),
                    heal_percent: Some(2.0),
                    lightning_type: Some(Box::new(BulletSpec {
                        kind: BulletKind::Plain,
                        speed: Some(0.0001),
                        damage: Some(0.0),
                        lifetime: Some(10.0),
                        hit_effect: Some(EffectRef::Named(EffectId::HIT_LANCER)),
                        despawn_effect: Some(EffectRef::Named(EffectId::NONE)),
                        status: Some("shocked"),
                        status_duration: Some(10.0),
                        hittable: Some(false),
                        heal_percent: Some(1.6),
                        collides_team: Some(true),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.x = Some(5.0);
            weapon.shake = Some(2.2);
            weapon.y = Some(0.5);
            weapon.shoot_y = Some(2.5);
            weapon.reload = Some(36.0);
            weapon.inaccuracy = Some(35.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon.recoil = Some(2.5);
            weapon.shoot_sound = Some(SoundId::SHOOT_PULSAR);
            weapon
        });
        unit.abilities
            .push(AbilitySpec::shield_regen_field(20.0, 40.0, 300.0, 60.0));
        unit
    })
}

fn quasar(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("quasar", UnitKind::UnitType, entity::MECH_LEGACY);
        unit.mine_tier = Some(3);
        unit.boost_multiplier = Some(2.0);
        unit.health = Some(640.0);
        unit.build_speed = Some(1.1);
        unit.can_boost = Some(true);
        unit.armor = Some(9.0);
        unit.mech_land_shake = Some(2.0);
        unit.rise_speed = Some(0.05);
        unit.descent_speed = Some(0.05);
        unit.mech_front_sway = Some(0.55);
        unit.step_sound = Some(SoundId::MECH_STEP_SMALL);
        unit.step_sound_pitch = Some(0.9);
        unit.step_sound_volume = Some(0.6);
        unit.speed = Some(0.5);
        unit.hit_size = Some(13.0);
        unit.mine_speed = Some(4.0);
        unit.draw_shields = Some(false);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "beam-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Laser,
                    damage: Some(45.0),
                    recoil: Some(0.0),
                    side_angle: Some(45.0),
                    side_width: Some(1.0),
                    side_length: Some(70.0),
                    heal_percent: Some(10.0),
                    collides_team: Some(true),
                    length: Some(150.0),
                    colors: Some(vec![
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.4),
                        Rgba::new(0.59607846, 1.0, 0.6627451, 1.0),
                        Rgba::WHITE,
                    ]),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.shake = Some(2.0);
            weapon.shoot_y = Some(4.0);
            weapon.x = Some(6.5);
            weapon.reload = Some(55.0);
            weapon.recoil = Some(4.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_LANCER);
            weapon
        });
        unit.abilities.push(AbilitySpec::force_field(
            60.0, 0.4, 500.0, 360.0, None, None,
        ));
        unit
    })
}

fn vela(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("vela", UnitKind::UnitType, entity::MECH);
        unit.hit_size = Some(24.0);
        unit.rotate_speed = Some(1.8);
        unit.mech_front_sway = Some(1.0);
        unit.build_speed = Some(3.0);
        unit.mech_step_particles = Some(true);
        unit.step_shake = Some(0.15);
        unit.drown_time_multiplier = Some(1.3);
        unit.speed = Some(0.44);
        unit.boost_multiplier = Some(2.4);
        unit.engine_offset = Some(12.0);
        unit.engine_size = Some(6.0);
        unit.low_altitude = Some(true);
        unit.rise_speed = Some(0.02);
        unit.descent_speed = Some(0.02);
        unit.health = Some(8200.0);
        unit.armor = Some(16.0);
        unit.can_boost = Some(true);
        unit.mech_land_shake = Some(4.0);
        unit.single_target = Some(true);
        unit.step_sound = Some(SoundId::MECH_STEP);
        unit.step_sound_pitch = Some(0.9);
        unit.step_sound_volume = Some(0.25);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "vela-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(1, 0.0, 39.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::ContinuousLaser,
                    damage: Some(35.0),
                    length: Some(180.0),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_MELT_HEAL)),
                    draw_size: Some(420.0),
                    lifetime: Some(160.0),
                    shake: Some(1.0),
                    despawn_effect: Some(EffectRef::Named(EffectId::SMOKE_CLOUD)),
                    smoke_effect: Some(EffectRef::Named(EffectId::NONE)),
                    charge_effect: Some(EffectRef::Named(EffectId::GREEN_LASER_CHARGE_SMALL)),
                    incend_chance: Some(0.1),
                    incend_spread: Some(5.0),
                    incend_amount: Some(1),
                    heal_percent: Some(1.0),
                    collides_team: Some(true),
                    colors: Some(vec![
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.2),
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.5),
                        Rgba::new(0.7152941, 1.0, 0.79529417, 1.0),
                        Rgba::WHITE,
                    ]),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.mirror = Some(false);
            weapon.top = Some(false);
            weapon.shake = Some(4.0);
            weapon.shoot_y = Some(14.0);
            weapon.x = Some(0.0);
            weapon.y = Some(0.0);
            weapon.parentize_effects = Some(true);
            weapon.reload = Some(155.0);
            weapon.recoil = Some(0.0);
            weapon.charge_sound = Some(SoundId::CHARGE_VELA);
            weapon.shoot_sound = Some(SoundId::BEAM_PLASMA);
            weapon.initial_shoot_sound = Some(SoundId::SHOOT_BEAM_PLASMA);
            weapon.continuous = Some(true);
            weapon.cooldown_time = Some(200.0);
            weapon.shoot_status = Some("slow");
            weapon.shoot_status_duration = Some(199.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "repair-beam-weapon-center-large",
                kind: WeaponKind::RepairBeamWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    max_range: Some(120.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.x = Some(11.0);
            weapon.y = Some(-7.5);
            weapon.shoot_y = Some(6.0);
            weapon.beam_width = Some(0.8);
            weapon.repair_speed = Some(1.4);
            weapon
        });
        unit.immunities.push("burning");
        unit
    })
}

fn corvus(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("corvus", UnitKind::UnitType, entity::LEGS);
        unit.hit_size = Some(29.0);
        unit.health = Some(18000.0);
        unit.armor = Some(14.0);
        unit.step_shake = Some(1.5);
        unit.rotate_speed = Some(1.5);
        unit.drown_time_multiplier = Some(1.6);
        unit.step_sound = Some(SoundId::WALKER_STEP);
        unit.step_sound_volume = Some(1.1);
        unit.step_sound_pitch = Some(0.9);
        unit.leg_count = Some(4);
        unit.leg_length = Some(14.0);
        unit.leg_base_offset = Some(11.0);
        unit.leg_move_space = Some(1.5);
        unit.leg_forward_scl = Some(0.58);
        unit.hovering = Some(true);
        unit.shadow_elevation = Some(0.2);
        unit.ground_layer = Some(75.0);
        unit.speed = Some(0.3);
        unit.draw_shields = Some(false);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "corvus-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(1, 0.0, 80.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Laser,
                    length: Some(460.0),
                    damage: Some(560.0),
                    width: Some(75.0),
                    lifetime: Some(65.0),
                    lightning_spacing: Some(35.0),
                    lightning_length: Some(5),
                    lightning_delay: Some(1.1),
                    lightning_length_rand: Some(15),
                    lightning_damage: Some(50.0),
                    lightning_angle_rand: Some(40.0),
                    large_hit: Some(true),
                    light_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    lightning_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    charge_effect: Some(EffectRef::Named(EffectId::GREEN_LASER_CHARGE)),
                    heal_percent: Some(25.0),
                    collides_team: Some(true),
                    side_angle: Some(15.0),
                    side_width: Some(0.0),
                    side_length: Some(0.0),
                    colors: Some(vec![
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.4),
                        Rgba::new(0.59607846, 1.0, 0.6627451, 1.0),
                        Rgba::WHITE,
                    ]),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_CORVUS);
            weapon.charge_sound = Some(SoundId::CHARGE_CORVUS);
            weapon.sound_pitch_min = Some(1.0);
            weapon.top = Some(false);
            weapon.mirror = Some(false);
            weapon.shake = Some(14.0);
            weapon.shoot_y = Some(5.0);
            weapon.x = Some(0.0);
            weapon.y = Some(0.0);
            weapon.reload = Some(350.0);
            weapon.recoil = Some(0.0);
            weapon.cooldown_time = Some(350.0);
            weapon.shoot_status_duration = Some(120.0);
            weapon.shoot_status = Some("unmoving");
            weapon.parentize_effects = Some(true);
            weapon
        });
        unit
    })
}

fn crawler(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("crawler", UnitKind::UnitType, entity::MECH);
        unit.research_cost_multiplier = Some(0.5);
        unit.ai_controller = Some(AiControllerKind::Suicide);
        unit.speed = Some(1.0);
        unit.hit_size = Some(8.0);
        unit.health = Some(150.0);
        unit.mech_side_sway = Some(0.25);
        unit.range = Some(40.0);
        unit.step_sound = Some(SoundId::WALKER_STEP_TINY);
        unit.step_sound_volume = Some(0.2);
        unit.target_under_blocks = Some(false);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    collides_tiles: Some(false),
                    collides: Some(false),
                    range_override: Some(25.0),
                    hit_effect: Some(EffectRef::Named(EffectId::PULVERIZE)),
                    speed: Some(0.0),
                    splash_damage_radius: Some(44.0),
                    instant_disappear: Some(true),
                    splash_damage: Some(80.0),
                    building_damage_multiplier: Some(0.68),
                    kill_shooter: Some(true),
                    hittable: Some(false),
                    collides_air: Some(true),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_on_death = Some(true);
            weapon.reload = Some(24.0);
            weapon.shoot_cone = Some(180.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon.shoot_sound = Some(SoundId::EXPLOSION_CRAWLER);
            weapon.shoot_sound_volume = Some(0.4);
            weapon.x = Some(0.0);
            weapon.shoot_y = Some(0.0);
            weapon.mirror = Some(false);
            weapon
        });
        unit
    })
}

fn atrax(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("atrax", UnitKind::UnitType, entity::LEGS);
        unit.speed = Some(0.6);
        unit.drag = Some(0.4);
        unit.hit_size = Some(13.0);
        unit.rotate_speed = Some(3.0);
        unit.target_air = Some(false);
        unit.health = Some(600.0);
        unit.step_sound = Some(SoundId::WALKER_STEP_SMALL);
        unit.step_sound_pitch = Some(1.0);
        unit.step_sound_volume = Some(0.25);
        unit.leg_count = Some(4);
        unit.leg_length = Some(9.0);
        unit.leg_forward_scl = Some(0.6);
        unit.leg_move_space = Some(1.4);
        unit.hovering = Some(true);
        unit.armor = Some(3.0);
        unit.shadow_elevation = Some(0.2);
        unit.ground_layer = Some(74.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "atrax-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Liquid,
                    liquid: Some("slag"),
                    damage: Some(13.0),
                    speed: Some(2.5),
                    drag: Some(0.009),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL)),
                    lifetime: Some(57.0),
                    collides_air: Some(false),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.shoot_y = Some(3.0);
            weapon.reload = Some(9.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon.recoil = Some(1.0);
            weapon.x = Some(7.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_ATRAX);
            weapon
        });
        unit.immunities.push("burning");
        unit.immunities.push("melting");
        unit
    })
}

fn spiroct(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("spiroct", UnitKind::UnitType, entity::LEGS_LEGACY);
        unit.speed = Some(0.54);
        unit.drag = Some(0.4);
        unit.hit_size = Some(15.0);
        unit.rotate_speed = Some(3.0);
        unit.health = Some(1000.0);
        unit.leg_count = Some(6);
        unit.leg_length = Some(13.0);
        unit.leg_forward_scl = Some(0.8);
        unit.leg_move_space = Some(1.4);
        unit.leg_base_offset = Some(2.0);
        unit.hovering = Some(true);
        unit.armor = Some(9.0);
        unit.shadow_elevation = Some(0.3);
        unit.ground_layer = Some(75.0);
        unit.step_sound = Some(SoundId::WALKER_STEP_SMALL);
        unit.step_sound_pitch = Some(0.7);
        unit.step_sound_volume = Some(0.35);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "spiroct-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Sap,
                    sap_strength: Some(0.4),
                    length: Some(75.0),
                    damage: Some(23.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL)),
                    hit_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    beam_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    despawn_effect: Some(EffectRef::Named(EffectId::NONE)),
                    width: Some(0.54),
                    lifetime: Some(35.0),
                    knockback: Some(-1.24),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_y = Some(4.0);
            weapon.reload = Some(14.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon.recoil = Some(2.0);
            weapon.rotate = Some(true);
            weapon.shoot_sound = Some(SoundId::SHOOT_SAP);
            weapon.x = Some(8.5);
            weapon.y = Some(-1.5);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "mount-purple-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Sap,
                    sap_strength: Some(0.7),
                    length: Some(40.0),
                    damage: Some(18.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL)),
                    hit_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    beam_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    despawn_effect: Some(EffectRef::Named(EffectId::NONE)),
                    width: Some(0.4),
                    lifetime: Some(25.0),
                    knockback: Some(-0.65),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(18.0);
            weapon.rotate = Some(true);
            weapon.x = Some(4.0);
            weapon.y = Some(3.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_SAP);
            weapon
        });
        unit
    })
}

fn arkyid(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("arkyid", UnitKind::UnitType, entity::LEGS_LEGACY);
        unit.drag = Some(0.1);
        unit.speed = Some(0.62);
        unit.hit_size = Some(23.0);
        unit.health = Some(8000.0);
        unit.armor = Some(14.0);
        unit.rotate_speed = Some(2.7);
        unit.leg_count = Some(6);
        unit.leg_move_space = Some(1.0);
        unit.leg_pair_offset = Some(3.0);
        unit.leg_length = Some(30.0);
        unit.leg_extension = Some(-15.0);
        unit.leg_base_offset = Some(10.0);
        unit.step_shake = Some(1.0);
        unit.leg_length_scl = Some(0.96);
        unit.ripple_scale = Some(2.0);
        unit.leg_speed = Some(0.2);
        unit.step_sound = Some(SoundId::WALKER_STEP);
        unit.step_sound_volume = Some(0.85);
        unit.step_sound_pitch = Some(1.1);
        unit.leg_splash_damage = Some(32.0);
        unit.leg_splash_range = Some(30.0);
        unit.hovering = Some(true);
        unit.shadow_elevation = Some(0.65);
        unit.ground_layer = Some(75.0);
        unit.pre_bullets.push(BulletSpec {
            kind: BulletKind::Sap,
            sap_strength: Some(0.85),
            length: Some(55.0),
            damage: Some(40.0),
            shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL)),
            hit_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
            beam_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
            despawn_effect: Some(EffectRef::Named(EffectId::NONE)),
            width: Some(0.55),
            lifetime: Some(30.0),
            knockback: Some(-1.0),
            ..BulletSpec::default()
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "spiroct-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Pre(0),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(9.0);
            weapon.x = Some(4.0);
            weapon.y = Some(8.0);
            weapon.rotate = Some(true);
            weapon.shoot_sound = Some(SoundId::SHOOT_SAP);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "spiroct-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Pre(0),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(14.0);
            weapon.x = Some(9.0);
            weapon.y = Some(6.0);
            weapon.rotate = Some(true);
            weapon.shoot_sound = Some(SoundId::SHOOT_SAP);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "spiroct-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Pre(0),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(22.0);
            weapon.x = Some(14.0);
            weapon.y = Some(0.0);
            weapon.rotate = Some(true);
            weapon.shoot_sound = Some(SoundId::SHOOT_SAP);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "large-purple-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Artillery,
                    speed: Some(2.0),
                    damage: Some(12.0),
                    hit_effect: Some(EffectRef::Named(EffectId::SAP_EXPLOSION)),
                    despawn_sound: Some(SoundId::EXPLOSION_ARTILLERY_SHOCK),
                    knockback: Some(0.8),
                    lifetime: Some(70.0),
                    width: Some(19.0),
                    height: Some(19.0),
                    collides_tiles: Some(true),
                    ammo_multiplier: Some(4.0),
                    splash_damage_radius: Some(70.0),
                    splash_damage: Some(65.0),
                    back_color: Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0)),
                    front_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    lightning_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    lightning: Some(3),
                    lightning_length: Some(10),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE2)),
                    status: Some("sapped"),
                    status_duration: Some(600.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.y = Some(-7.0);
            weapon.x = Some(9.0);
            weapon.shoot_y = Some(7.0);
            weapon.reload = Some(45.0);
            weapon.shake = Some(5.0);
            weapon.rotate_speed = Some(2.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.shoot_sound = Some(SoundId::SHOOT_ARTILLERY_SAP);
            weapon.rotate = Some(true);
            weapon.shadow = Some(8.0);
            weapon.recoil = Some(3.0);
            weapon
        });
        unit
    })
}

fn toxopid(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("toxopid", UnitKind::UnitType, entity::LEGS_LEGACY);
        unit.drag = Some(0.1);
        unit.speed = Some(0.5);
        unit.hit_size = Some(26.0);
        unit.health = Some(22000.0);
        unit.armor = Some(22.0);
        unit.light_radius = Some(140.0);
        unit.step_sound = Some(SoundId::WALKER_STEP);
        unit.step_sound_volume = Some(1.1);
        unit.rotate_speed = Some(1.9);
        unit.leg_count = Some(8);
        unit.leg_move_space = Some(0.8);
        unit.leg_pair_offset = Some(3.0);
        unit.leg_length = Some(75.0);
        unit.leg_extension = Some(-20.0);
        unit.leg_base_offset = Some(8.0);
        unit.step_shake = Some(1.0);
        unit.leg_length_scl = Some(0.93);
        unit.ripple_scale = Some(3.0);
        unit.leg_speed = Some(0.19);
        unit.leg_splash_damage = Some(80.0);
        unit.leg_splash_range = Some(60.0);
        unit.hovering = Some(true);
        unit.shadow_elevation = Some(0.95);
        unit.ground_layer = Some(75.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "large-purple-mount",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::spread(2, 17.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Shrapnel,
                    length: Some(90.0),
                    damage: Some(110.0),
                    width: Some(25.0),
                    serration_len_scl: Some(7.0),
                    serration_space_offset: Some(60.0),
                    serration_fade_offset: Some(0.0),
                    serrations: Some(10),
                    serration_width: Some(6.0),
                    from_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    to_color: Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0)),
                    shoot_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.y = Some(-5.0);
            weapon.x = Some(11.0);
            weapon.shoot_y = Some(7.0);
            weapon.reload = Some(30.0);
            weapon.shake = Some(4.0);
            weapon.rotate_speed = Some(2.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.shoot_sound = Some(SoundId::SHOOT_TOXOPID_SHOTGUN);
            weapon.shoot_sound_volume = Some(0.8);
            weapon.rotate = Some(true);
            weapon.shadow = Some(12.0);
            weapon.recoil = Some(3.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "toxopid-cannon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Artillery,
                    speed: Some(3.0),
                    damage: Some(50.0),
                    despawn_sound: Some(SoundId::EXPLOSION_ARTILLERY_SHOCK_BIG),
                    hit_effect: Some(EffectRef::Named(EffectId::SAP_EXPLOSION)),
                    knockback: Some(0.8),
                    lifetime: Some(80.0),
                    width: Some(25.0),
                    height: Some(25.0),
                    collides_tiles: Some(true),
                    collides: Some(true),
                    ammo_multiplier: Some(4.0),
                    splash_damage_radius: Some(80.0),
                    splash_damage: Some(75.0),
                    back_color: Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0)),
                    front_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    lightning_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    lightning: Some(5),
                    lightning_length: Some(20),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE2)),
                    hit_shake: Some(10.0),
                    light_radius: Some(40.0),
                    light_color: Some(Rgba::new(0.4, 0.36078432, 0.62352943, 1.0)),
                    light_opacity: Some(0.6),
                    status: Some("sapped"),
                    status_duration: Some(600.0),
                    frag_life_min: Some(0.3),
                    frag_bullets: Some(9),
                    frag_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Artillery,
                        speed: Some(2.3),
                        damage: Some(30.0),
                        despawn_sound: Some(SoundId::EXPLOSION_ARTILLERY_SHOCK),
                        hit_effect: Some(EffectRef::Named(EffectId::SAP_EXPLOSION)),
                        knockback: Some(0.8),
                        lifetime: Some(90.0),
                        width: Some(20.0),
                        height: Some(20.0),
                        collides_tiles: Some(false),
                        splash_damage_radius: Some(70.0),
                        splash_damage: Some(40.0),
                        back_color: Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0)),
                        front_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                        lightning_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                        lightning: Some(2),
                        lightning_length: Some(5),
                        smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE2)),
                        hit_shake: Some(5.0),
                        light_radius: Some(30.0),
                        light_color: Some(Rgba::new(0.4, 0.36078432, 0.62352943, 1.0)),
                        light_opacity: Some(0.5),
                        status: Some("sapped"),
                        status_duration: Some(600.0),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.y = Some(-14.0);
            weapon.x = Some(0.0);
            weapon.shoot_y = Some(22.0);
            weapon.mirror = Some(false);
            weapon.reload = Some(210.0);
            weapon.shake = Some(10.0);
            weapon.recoil = Some(10.0);
            weapon.rotate_speed = Some(1.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING3));
            weapon.shoot_sound = Some(SoundId::SHOOT_ARTILLERY_SAP_BIG);
            weapon.rotate = Some(true);
            weapon.shadow = Some(30.0);
            weapon.rotation_limit = Some(80.0);
            weapon
        });
        unit
    })
}

fn flare(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("flare", UnitKind::UnitType, entity::AIR);
        unit.research_cost_multiplier = Some(0.5);
        unit.speed = Some(2.7);
        unit.accel = Some(0.08);
        unit.drag = Some(0.04);
        unit.flying = Some(true);
        unit.health = Some(70.0);
        unit.engine_offset = Some(5.75);
        unit.hit_size = Some(9.0);
        unit.item_capacity = Some(10);
        unit.circle_target = Some(true);
        unit.omni_movement = Some(false);
        unit.rotate_speed = Some(5.0);
        unit.circle_target_radius = Some(60.0);
        unit.wreck_sound_volume = Some(0.7);
        unit.move_sound = Some(SoundId::LOOP_THRUSTER);
        unit.move_sound_pitch_min = Some(0.3);
        unit.move_sound_pitch_max = Some(1.5);
        unit.move_sound_volume = Some(0.2);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(3, 3.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(2.5),
                    damage: Some(9.0),
                    inaccuracy: Some(4.0),
                    width: Some(7.0),
                    height: Some(9.0),
                    lifetime: Some(32.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL_SMOKE)),
                    ammo_multiplier: Some(2.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.y = Some(1.0);
            weapon.x = Some(0.0);
            weapon.min_shoot_velocity = Some(2.0);
            weapon.shoot_cone = Some(10.0);
            weapon.reload = Some(80.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.mirror = Some(false);
            weapon
        });
        unit.target_flags = vec![Some(BlockFlag::Generator), None];
        unit
    })
}

fn horizon(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("horizon", UnitKind::UnitType, entity::AIR);
        unit.health = Some(340.0);
        unit.speed = Some(1.65);
        unit.accel = Some(0.08);
        unit.drag = Some(0.03);
        unit.flying = Some(true);
        unit.hit_size = Some(11.0);
        unit.target_air = Some(false);
        unit.engine_offset = Some(7.8);
        unit.range = Some(140.0);
        unit.face_target = Some(false);
        unit.auto_drop_bombs = Some(true);
        unit.armor = Some(3.0);
        unit.item_capacity = Some(0);
        unit.circle_target = Some(true);
        unit.omni_movement = Some(false);
        unit.rotate_speed = Some(4.5);
        unit.circle_target_radius = Some(40.0);
        unit.move_sound = Some(SoundId::LOOP_THRUSTER);
        unit.move_sound_pitch_min = Some(0.6);
        unit.move_sound_volume = Some(0.4);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Bomb,
                    splash_damage: Some(27.0),
                    splash_damage_radius: Some(25.0),
                    width: Some(10.0),
                    height: Some(14.0),
                    hit_effect: Some(EffectRef::Named(EffectId::FLAK_EXPLOSION)),
                    shoot_effect: Some(EffectRef::Named(EffectId::NONE)),
                    smoke_effect: Some(EffectRef::Named(EffectId::NONE)),
                    status: Some("blasted"),
                    status_duration: Some(60.0),
                    damage: Some(13.5),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.min_shoot_velocity = Some(1.0);
            weapon.x = Some(3.0);
            weapon.shoot_y = Some(0.0);
            weapon.reload = Some(12.0);
            weapon.shoot_cone = Some(180.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon.inaccuracy = Some(15.0);
            weapon.ignore_rotation = Some(true);
            weapon.shoot_sound = Some(SoundId::SHOOT_HORIZON);
            weapon.sound_pitch_max = Some(1.2);
            weapon
        });
        unit.target_flags = vec![Some(BlockFlag::Factory), None];
        unit
    })
}

fn zenith(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("zenith", UnitKind::UnitType, entity::AIR);
        unit.health = Some(700.0);
        unit.speed = Some(1.7);
        unit.accel = Some(0.04);
        unit.drag = Some(0.016);
        unit.flying = Some(true);
        unit.range = Some(140.0);
        unit.hit_size = Some(20.0);
        unit.low_altitude = Some(true);
        unit.force_multi_target = Some(true);
        unit.armor = Some(5.0);
        unit.engine_offset = Some(12.0);
        unit.engine_size = Some(3.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "zenith-missiles",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(2, 0.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Missile,
                    speed: Some(3.0),
                    damage: Some(14.0),
                    width: Some(8.0),
                    height: Some(8.0),
                    shrink_y: Some(0.0),
                    drag: Some(-0.003),
                    homing_range: Some(60.0),
                    scale_keep_velocity: Some(true),
                    splash_damage_radius: Some(25.0),
                    splash_damage: Some(15.0),
                    lifetime: Some(50.0),
                    trail_color: Some(Rgba::new(0.8156863, 0.41960785, 0.3254902, 1.0)),
                    back_color: Some(Rgba::new(0.8156863, 0.41960785, 0.3254902, 1.0)),
                    front_color: Some(Rgba::new(1.0, 0.6509804, 0.39607844, 1.0)),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    despawn_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    weave_scale: Some(6.0),
                    weave_mag: Some(1.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(40.0);
            weapon.x = Some(7.0);
            weapon.rotate = Some(true);
            weapon.shake = Some(1.0);
            weapon.inaccuracy = Some(5.0);
            weapon.velocity_rnd = Some(0.2);
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE_LONG);
            weapon
        });
        unit.target_flags = vec![
            Some(BlockFlag::LaunchPad),
            Some(BlockFlag::Storage),
            Some(BlockFlag::Battery),
            None,
        ];
        unit
    })
}

fn antumbra(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("antumbra", UnitKind::UnitType, entity::AIR);
        unit.speed = Some(0.8);
        unit.accel = Some(0.04);
        unit.drag = Some(0.04);
        unit.rotate_speed = Some(1.9);
        unit.flying = Some(true);
        unit.low_altitude = Some(true);
        unit.health = Some(7200.0);
        unit.armor = Some(17.0);
        unit.engine_offset = Some(21.0);
        unit.engine_size = Some(5.3);
        unit.hit_size = Some(46.0);
        unit.loop_sound = Some(SoundId::LOOP_HOVER);
        unit.pre_bullets.push(BulletSpec {
            kind: BulletKind::Missile,
            speed: Some(2.7),
            damage: Some(18.0),
            width: Some(8.0),
            height: Some(8.0),
            shrink_y: Some(0.0),
            drag: Some(-0.01),
            splash_damage_radius: Some(20.0),
            splash_damage: Some(37.0),
            ammo_multiplier: Some(4.0),
            lifetime: Some(50.0),
            hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
            despawn_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
            status: Some("blasted"),
            status_duration: Some(60.0),
            ..BulletSpec::default()
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "missiles-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Pre(0),
                ..WeaponSpec::default()
            };
            weapon.y = Some(8.0);
            weapon.x = Some(17.0);
            weapon.reload = Some(20.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.rotate_speed = Some(8.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE);
            weapon.rotate = Some(true);
            weapon.shadow = Some(6.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "missiles-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Pre(0),
                ..WeaponSpec::default()
            };
            weapon.y = Some(-8.0);
            weapon.x = Some(17.0);
            weapon.reload = Some(35.0);
            weapon.rotate_speed = Some(8.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE);
            weapon.rotate = Some(true);
            weapon.shadow = Some(6.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "large-bullet-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(7.0),
                    damage: Some(55.0),
                    width: Some(12.0),
                    height: Some(18.0),
                    lifetime: Some(25.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG)),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.y = Some(2.0);
            weapon.x = Some(10.0);
            weapon.shoot_y = Some(10.0);
            weapon.reload = Some(12.0);
            weapon.shake = Some(1.0);
            weapon.rotate_speed = Some(2.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.shoot_sound = Some(SoundId::SHOOT_SPECTRE);
            weapon.rotate = Some(true);
            weapon.shadow = Some(8.0);
            weapon
        });
        unit.target_flags = vec![Some(BlockFlag::Generator), Some(BlockFlag::Core), None];
        unit
    })
}

fn eclipse(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("eclipse", UnitKind::UnitType, entity::AIR);
        unit.speed = Some(0.54);
        unit.accel = Some(0.04);
        unit.drag = Some(0.04);
        unit.rotate_speed = Some(1.0);
        unit.flying = Some(true);
        unit.low_altitude = Some(true);
        unit.health = Some(22000.0);
        unit.engine_offset = Some(38.0);
        unit.engine_size = Some(7.3);
        unit.hit_size = Some(58.0);
        unit.armor = Some(22.0);
        unit.loop_sound = Some(SoundId::LOOP_HOVER);
        unit.pre_bullets.push(BulletSpec {
            kind: BulletKind::Flak,
            speed: Some(4.0),
            damage: Some(15.0),
            shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG)),
            ammo_multiplier: Some(4.0),
            splash_damage: Some(65.0),
            splash_damage_radius: Some(25.0),
            collides_ground: Some(true),
            lifetime: Some(47.0),
            status: Some("blasted"),
            status_duration: Some(60.0),
            ..BulletSpec::default()
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "large-laser-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Laser,
                    damage: Some(115.0),
                    side_angle: Some(20.0),
                    side_width: Some(1.5),
                    side_length: Some(80.0),
                    width: Some(25.0),
                    length: Some(230.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOCKWAVE)),
                    colors: Some(vec![
                        Rgba::new(0.9254902, 0.45490196, 0.34509805, 0.6666667),
                        Rgba::new(1.0, 0.6117647, 0.3529412, 1.0),
                        Rgba::WHITE,
                    ]),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shake = Some(4.0);
            weapon.shoot_y = Some(9.0);
            weapon.x = Some(18.0);
            weapon.y = Some(5.0);
            weapon.rotate_speed = Some(2.0);
            weapon.reload = Some(45.0);
            weapon.recoil = Some(4.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_ECLIPSE);
            weapon.shadow = Some(20.0);
            weapon.rotate = Some(true);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "large-artillery",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Pre(0),
                ..WeaponSpec::default()
            };
            weapon.x = Some(11.0);
            weapon.y = Some(27.0);
            weapon.rotate_speed = Some(2.0);
            weapon.reload = Some(9.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_CYCLONE);
            weapon.shadow = Some(7.0);
            weapon.rotate = Some(true);
            weapon.recoil = Some(0.5);
            weapon.shoot_y = Some(7.25);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "large-artillery",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Pre(0),
                ..WeaponSpec::default()
            };
            weapon.y = Some(-13.0);
            weapon.x = Some(20.0);
            weapon.reload = Some(12.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.rotate_speed = Some(7.0);
            weapon.shake = Some(1.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_CYCLONE);
            weapon.rotate = Some(true);
            weapon.shadow = Some(12.0);
            weapon.shoot_y = Some(7.25);
            weapon
        });
        unit.target_flags = vec![
            Some(BlockFlag::Reactor),
            Some(BlockFlag::Battery),
            Some(BlockFlag::Core),
            None,
        ];
        unit
    })
}

fn mono(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("mono", UnitKind::UnitType, entity::AIR_LEGACY_MONO);
        unit.default_command = Some("mine");
        unit.flying = Some(true);
        unit.drag = Some(0.06);
        unit.accel = Some(0.12);
        unit.speed = Some(1.5);
        unit.health = Some(100.0);
        unit.engine_size = Some(1.8);
        unit.engine_offset = Some(5.7);
        unit.range = Some(50.0);
        unit.is_enemy = Some(false);
        unit.control_select_global = Some(false);
        unit.wreck_sound_volume = Some(0.7);
        unit.death_sound_volume = Some(0.7);
        unit.mine_tier = Some(1);
        unit.mine_speed = Some(2.5);
        unit
    })
}

fn poly(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("poly", UnitKind::UnitType, entity::AIR_LEGACY_POLY);
        unit.default_command = Some("rebuild");
        unit.flying = Some(true);
        unit.drag = Some(0.05);
        unit.speed = Some(2.6);
        unit.rotate_speed = Some(15.0);
        unit.accel = Some(0.1);
        unit.range = Some(130.0);
        unit.health = Some(400.0);
        unit.build_speed = Some(0.4);
        unit.engine_offset = Some(6.5);
        unit.hit_size = Some(9.0);
        unit.low_altitude = Some(true);
        unit.mine_tier = Some(2);
        unit.mine_speed = Some(3.5);
        unit.wreck_sound_volume = Some(0.9);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "poly-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Missile,
                    speed: Some(4.0),
                    damage: Some(12.0),
                    homing_power: Some(0.08),
                    weave_mag: Some(4.0),
                    weave_scale: Some(4.0),
                    lifetime: Some(50.0),
                    scale_keep_velocity: Some(true),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_HEAL)),
                    smoke_effect: Some(EffectRef::Named(EffectId::HIT_LASER)),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_LASER)),
                    despawn_effect: Some(EffectRef::Named(EffectId::HIT_LASER)),
                    front_color: Some(Rgba::WHITE),
                    hit_sound: Some(SoundId::NONE),
                    heal_percent: Some(5.5),
                    collides_team: Some(true),
                    reflectable: Some(false),
                    back_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    trail_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.y = Some(-2.5);
            weapon.x = Some(3.75);
            weapon.reload = Some(30.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon.recoil = Some(2.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE_PLASMA_SHORT);
            weapon.velocity_rnd = Some(0.5);
            weapon.inaccuracy = Some(15.0);
            weapon.alternate = Some(true);
            weapon
        });
        unit.abilities.push({
            let mut ability = AbilitySpec::for_kind(AbilityKind::RepairField);
            ability.amount = 5.0;
            ability.reload = 480.0;
            ability.range = 50.0;
            ability
        });
        unit
    })
}

fn mega(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("mega", UnitKind::UnitType, entity::AIR_PAYLOAD);
        unit.default_command = Some("repair");
        unit.mine_tier = Some(3);
        unit.mine_speed = Some(4.0);
        unit.health = Some(460.0);
        unit.armor = Some(3.0);
        unit.speed = Some(2.5);
        unit.accel = Some(0.06);
        unit.drag = Some(0.017);
        unit.low_altitude = Some(true);
        unit.flying = Some(true);
        unit.engine_offset = Some(10.5);
        unit.face_target = Some(false);
        unit.hit_size = Some(16.05);
        unit.engine_size = Some(3.0);
        unit.payload_capacity = Some(256.0);
        unit.build_speed = Some(2.6);
        unit.is_enemy = Some(false);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "heal-weapon-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::LaserBolt,
                    speed: Some(5.2),
                    damage: Some(10.0),
                    lifetime: Some(35.0),
                    heal_percent: Some(5.5),
                    collides_team: Some(true),
                    back_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_LASER);
            weapon.reload = Some(24.0);
            weapon.x = Some(8.0);
            weapon.y = Some(-6.0);
            weapon.rotate = Some(true);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "heal-weapon-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::LaserBolt,
                    speed: Some(5.2),
                    damage: Some(8.0),
                    lifetime: Some(35.0),
                    heal_percent: Some(3.0),
                    collides_team: Some(true),
                    back_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_LASER);
            weapon.reload = Some(15.0);
            weapon.x = Some(4.0);
            weapon.y = Some(5.0);
            weapon.rotate = Some(true);
            weapon
        });
        unit
    })
}

fn quad(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("quad", UnitKind::UnitType, entity::AIR_PAYLOAD_LEGACY_QUAD);
        unit.armor = Some(10.0);
        unit.health = Some(6000.0);
        unit.speed = Some(1.2);
        unit.rotate_speed = Some(2.0);
        unit.accel = Some(0.05);
        unit.drag = Some(0.017);
        unit.low_altitude = Some(false);
        unit.flying = Some(true);
        unit.auto_drop_bombs = Some(true);
        unit.circle_target = Some(true);
        unit.engine_offset = Some(13.0);
        unit.engine_size = Some(7.0);
        unit.face_target = Some(false);
        unit.hit_size = Some(36.0);
        unit.payload_capacity = Some(576.0);
        unit.build_speed = Some(2.5);
        unit.build_beam_offset = Some(23.0);
        unit.range = Some(140.0);
        unit.target_air = Some(false);
        unit.loop_sound = Some(SoundId::LOOP_HOVER);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    sprite: Some(Some(String::from("large-bomb"))),
                    width: Some(30.0),
                    height: Some(30.0),
                    max_range: Some(30.0),
                    back_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    mix_color_to: Some(Rgba::WHITE),
                    hit_sound: Some(SoundId::EXPLOSION_QUAD),
                    hit_sound_volume: Some(0.9),
                    hit_shake: Some(4.0),
                    collides_air: Some(false),
                    lifetime: Some(70.0),
                    despawn_effect: Some(EffectRef::Named(EffectId::GREEN_BOMB)),
                    hit_effect: Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION)),
                    keep_velocity: Some(false),
                    spin: Some(2.0),
                    shrink_x: Some(0.7),
                    shrink_y: Some(0.7),
                    speed: Some(0.0),
                    collides: Some(false),
                    heal_percent: Some(15.0),
                    splash_damage: Some(220.0),
                    splash_damage_radius: Some(80.0),
                    damage: Some(154.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.x = Some(0.0);
            weapon.y = Some(0.0);
            weapon.mirror = Some(false);
            weapon.reload = Some(55.0);
            weapon.min_shoot_velocity = Some(0.01);
            weapon.sound_pitch_min = Some(1.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_QUAD);
            weapon.ignore_rotation = Some(true);
            weapon.shoot_cone = Some(180.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon
        });
        unit.target_flags = vec![Some(BlockFlag::Battery), Some(BlockFlag::Factory), None];
        unit
    })
}

fn oct(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("oct", UnitKind::UnitType, entity::AIR_PAYLOAD_LEGACY_OCT);
        unit.ai_controller = Some(AiControllerKind::Defender);
        unit.armor = Some(20.0);
        unit.health = Some(24000.0);
        unit.speed = Some(0.8);
        unit.rotate_speed = Some(1.0);
        unit.accel = Some(0.04);
        unit.drag = Some(0.018);
        unit.flying = Some(true);
        unit.engine_offset = Some(46.0);
        unit.engine_size = Some(7.8);
        unit.face_target = Some(false);
        unit.hit_size = Some(66.0);
        unit.payload_capacity = Some(1936.0);
        unit.build_speed = Some(4.0);
        unit.draw_shields = Some(false);
        unit.low_altitude = Some(true);
        unit.build_beam_offset = Some(43.0);
        unit.loop_sound = Some(SoundId::LOOP_HOVER);
        unit.abilities.push({
            let mut ability =
                AbilitySpec::force_field(140.0, 4.0, 7000.0, 480.0, Some(8), Some(0.0));
            ability.break_sound = SoundId::SHIELD_BREAK;
            ability
        });
        unit.abilities
            .push(AbilitySpec::repair_field(130.0, 120.0, 140.0));
        unit
    })
}

fn risso(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("risso", UnitKind::UnitType, entity::NAVAL);
        unit.speed = Some(1.1);
        unit.drag = Some(0.13);
        unit.hit_size = Some(10.0);
        unit.health = Some(280.0);
        unit.armor = Some(2.0);
        unit.accel = Some(0.4);
        unit.rotate_speed = Some(3.3);
        unit.face_target = Some(false);
        unit.trail_length = Some(20);
        unit.wave_trail_x = Some(4.0);
        unit.trail_scl = Some(1.3);
        unit.move_sound_volume = Some(0.4);
        unit.move_sound = Some(SoundId::SHIP_MOVE);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "mount-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(2.5),
                    damage: Some(9.0),
                    width: Some(7.0),
                    height: Some(9.0),
                    lifetime: Some(60.0),
                    ammo_multiplier: Some(2.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(13.0);
            weapon.x = Some(4.0);
            weapon.shoot_y = Some(4.0);
            weapon.y = Some(1.5);
            weapon.rotate = Some(true);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "missiles-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Missile,
                    speed: Some(2.7),
                    damage: Some(12.0),
                    sprite: Some(Some(String::from("missile"))),
                    keep_velocity: Some(true),
                    width: Some(8.0),
                    height: Some(8.0),
                    shrink_y: Some(0.0),
                    drag: Some(-0.003),
                    homing_range: Some(60.0),
                    splash_damage_radius: Some(25.0),
                    splash_damage: Some(10.0),
                    lifetime: Some(65.0),
                    trail_color: Some(Rgba::new(0.5, 0.5, 0.5, 1.0)),
                    back_color: Some(Rgba::new(0.9764706, 0.7607843, 0.47843137, 1.0)),
                    front_color: Some(Rgba::new(1.0, 0.972549, 0.9098039, 1.0)),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    despawn_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    weave_scale: Some(8.0),
                    weave_mag: Some(2.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.mirror = Some(false);
            weapon.reload = Some(25.0);
            weapon.x = Some(0.0);
            weapon.y = Some(-5.0);
            weapon.rotate = Some(true);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE_SHORT);
            weapon
        });
        unit
    })
}

fn minke(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("minke", UnitKind::UnitType, entity::NAVAL);
        unit.health = Some(600.0);
        unit.speed = Some(0.9);
        unit.drag = Some(0.15);
        unit.hit_size = Some(13.0);
        unit.armor = Some(4.0);
        unit.accel = Some(0.3);
        unit.rotate_speed = Some(2.6);
        unit.face_target = Some(false);
        unit.move_sound_volume = Some(0.55);
        unit.move_sound_pitch_min = Some(0.9);
        unit.move_sound_pitch_max = Some(0.9);
        unit.move_sound = Some(SoundId::SHIP_MOVE);
        unit.trail_length = Some(20);
        unit.wave_trail_x = Some(5.5);
        unit.wave_trail_y = Some(-4.0);
        unit.trail_scl = Some(1.9);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "mount-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Flak,
                    speed: Some(4.2),
                    damage: Some(3.0),
                    lifetime: Some(52.5),
                    ammo_multiplier: Some(4.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL)),
                    width: Some(6.0),
                    height: Some(8.0),
                    hit_effect: Some(EffectRef::Named(EffectId::FLAK_EXPLOSION)),
                    splash_damage: Some(40.5),
                    splash_damage_radius: Some(15.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(10.0);
            weapon.x = Some(5.0);
            weapon.y = Some(3.5);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(5.0);
            weapon.inaccuracy = Some(8.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING1));
            weapon.shoot_sound = Some(SoundId::SHOOT_DUO);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "artillery-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Artillery,
                    speed: Some(3.0),
                    damage: Some(20.0),
                    sprite: Some(Some(String::from("shell"))),
                    hit_effect: Some(EffectRef::Named(EffectId::FLAK_EXPLOSION)),
                    knockback: Some(0.8),
                    lifetime: Some(73.5),
                    width: Some(11.0),
                    height: Some(11.0),
                    collides_tiles: Some(false),
                    splash_damage_radius: Some(22.5),
                    splash_damage: Some(40.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(30.0);
            weapon.x = Some(5.0);
            weapon.y = Some(-5.0);
            weapon.rotate = Some(true);
            weapon.inaccuracy = Some(2.0);
            weapon.rotate_speed = Some(2.0);
            weapon.shake = Some(1.5);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING2));
            weapon.shoot_sound = Some(SoundId::SHOOT_ARTILLERY_SMALL);
            weapon
        });
        unit
    })
}

fn bryde(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("bryde", UnitKind::UnitType, entity::NAVAL);
        unit.health = Some(910.0);
        unit.speed = Some(0.85);
        unit.accel = Some(0.2);
        unit.rotate_speed = Some(1.8);
        unit.drag = Some(0.17);
        unit.hit_size = Some(20.0);
        unit.armor = Some(7.0);
        unit.face_target = Some(false);
        unit.move_sound_volume = Some(0.7);
        unit.move_sound_pitch_min = Some(0.77);
        unit.move_sound_pitch_max = Some(0.77);
        unit.move_sound = Some(SoundId::SHIP_MOVE);
        unit.trail_length = Some(22);
        unit.wave_trail_x = Some(7.0);
        unit.wave_trail_y = Some(-9.0);
        unit.trail_scl = Some(1.5);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "large-artillery",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Artillery,
                    speed: Some(3.2),
                    damage: Some(15.0),
                    trail_mult: Some(0.8),
                    hit_effect: Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION)),
                    knockback: Some(1.5),
                    lifetime: Some(84.0),
                    height: Some(15.5),
                    width: Some(15.0),
                    collides_tiles: Some(false),
                    splash_damage_radius: Some(40.0),
                    splash_damage: Some(70.0),
                    back_color: Some(Rgba::new(0.8980392, 0.5372549, 0.3372549, 1.0)),
                    front_color: Some(Rgba::new(1.0, 0.8235294, 0.68235296, 1.0)),
                    trail_effect: Some(EffectRef::Named(EffectId::ARTILLERY_TRAIL)),
                    trail_size: Some(6.0),
                    hit_shake: Some(4.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG2)),
                    status: Some("blasted"),
                    status_duration: Some(60.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(65.0);
            weapon.mirror = Some(false);
            weapon.x = Some(0.0);
            weapon.y = Some(-3.5);
            weapon.rotate_speed = Some(1.7);
            weapon.rotate = Some(true);
            weapon.shoot_y = Some(7.0);
            weapon.shake = Some(5.0);
            weapon.recoil = Some(4.0);
            weapon.shadow = Some(12.0);
            weapon.inaccuracy = Some(3.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING3));
            weapon.shoot_sound = Some(SoundId::SHOOT_ARTILLERY);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "missiles-mount",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(2, 3.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Missile,
                    speed: Some(2.7),
                    damage: Some(12.0),
                    width: Some(8.0),
                    height: Some(8.0),
                    shrink_y: Some(0.0),
                    drag: Some(-0.003),
                    homing_range: Some(60.0),
                    keep_velocity: Some(false),
                    splash_damage_radius: Some(25.0),
                    splash_damage: Some(10.0),
                    lifetime: Some(70.0),
                    trail_color: Some(Rgba::new(0.5, 0.5, 0.5, 1.0)),
                    back_color: Some(Rgba::new(0.9764706, 0.7607843, 0.47843137, 1.0)),
                    front_color: Some(Rgba::new(1.0, 0.972549, 0.9098039, 1.0)),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    despawn_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    weave_scale: Some(8.0),
                    weave_mag: Some(1.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(20.0);
            weapon.x = Some(8.5);
            weapon.y = Some(-9.0);
            weapon.shadow = Some(6.0);
            weapon.rotate_speed = Some(4.0);
            weapon.rotate = Some(true);
            weapon.inaccuracy = Some(5.0);
            weapon.velocity_rnd = Some(0.1);
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE_SHORT);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon
        });
        unit.abilities
            .push(AbilitySpec::shield_regen_field(20.0, 40.0, 240.0, 60.0));
        unit
    })
}

fn sei(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("sei", UnitKind::UnitType, entity::NAVAL);
        unit.health = Some(11000.0);
        unit.armor = Some(12.0);
        unit.speed = Some(0.73);
        unit.drag = Some(0.17);
        unit.hit_size = Some(39.0);
        unit.accel = Some(0.2);
        unit.rotate_speed = Some(1.3);
        unit.face_target = Some(false);
        unit.move_sound_volume = Some(1.0);
        unit.move_sound = Some(SoundId::SHIP_MOVE_BIG);
        unit.move_sound_pitch_min = Some(0.95);
        unit.move_sound_pitch_max = Some(0.95);
        unit.trail_length = Some(50);
        unit.wave_trail_x = Some(18.0);
        unit.wave_trail_y = Some(-21.0);
        unit.trail_scl = Some(3.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "sei-launcher",
                kind: WeaponKind::Weapon,
                shoot: Some({
                    let mut shoot = ShootPatternSpec::alternate(6, 1.5, 5.0, 3);
                    shoot.spread = 4.0;
                    shoot
                }),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Missile,
                    speed: Some(4.2),
                    damage: Some(42.0),
                    homing_power: Some(0.12),
                    width: Some(8.0),
                    height: Some(8.0),
                    shrink_x: Some(0.0),
                    shrink_y: Some(0.0),
                    drag: Some(-0.003),
                    homing_range: Some(80.0),
                    keep_velocity: Some(false),
                    splash_damage_radius: Some(35.0),
                    splash_damage: Some(45.0),
                    lifetime: Some(62.0),
                    trail_color: Some(Rgba::new(0.9764706, 0.7607843, 0.47843137, 1.0)),
                    back_color: Some(Rgba::new(0.9764706, 0.7607843, 0.47843137, 1.0)),
                    front_color: Some(Rgba::new(1.0, 0.972549, 0.9098039, 1.0)),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    despawn_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    weave_scale: Some(8.0),
                    weave_mag: Some(2.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.x = Some(0.0);
            weapon.y = Some(0.0);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(4.0);
            weapon.mirror = Some(false);
            weapon.shadow = Some(20.0);
            weapon.shoot_y = Some(4.5);
            weapon.recoil = Some(4.0);
            weapon.reload = Some(45.0);
            weapon.velocity_rnd = Some(0.4);
            weapon.inaccuracy = Some(7.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon.shake = Some(1.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE_LONG);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "large-bullet-mount",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(3, 4.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(7.0),
                    damage: Some(57.0),
                    width: Some(13.0),
                    height: Some(19.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG)),
                    lifetime: Some(35.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(60.0);
            weapon.cooldown_time = Some(90.0);
            weapon.x = Some(17.5);
            weapon.y = Some(-16.5);
            weapon.rotate_speed = Some(4.0);
            weapon.rotate = Some(true);
            weapon.shoot_y = Some(7.0);
            weapon.shake = Some(2.0);
            weapon.recoil = Some(3.0);
            weapon.shadow = Some(12.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::CASING3));
            weapon.shoot_sound = Some(SoundId::SHOOT_SPECTRE);
            weapon.inaccuracy = Some(1.0);
            weapon
        });
        unit
    })
}

fn omura(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("omura", UnitKind::UnitType, entity::NAVAL);
        unit.health = Some(22000.0);
        unit.speed = Some(0.62);
        unit.drag = Some(0.18);
        unit.hit_size = Some(58.0);
        unit.armor = Some(16.0);
        unit.accel = Some(0.19);
        unit.rotate_speed = Some(0.9);
        unit.face_target = Some(false);
        unit.move_sound_volume = Some(1.1);
        unit.move_sound = Some(SoundId::SHIP_MOVE_BIG);
        unit.move_sound_pitch_min = Some(0.9);
        unit.move_sound_pitch_max = Some(0.9);
        unit.trail_length = Some(70);
        unit.wave_trail_x = Some(23.0);
        unit.wave_trail_y = Some(-32.0);
        unit.trail_scl = Some(3.5);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "omura-cannon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Rail,
                    shoot_effect: Some(EffectRef::Named(EffectId::RAIL_SHOOT)),
                    length: Some(500.0),
                    point_effect_space: Some(60.0),
                    pierce_effect: Some(EffectRef::Named(EffectId::RAIL_HIT)),
                    point_effect: Some(EffectRef::Named(EffectId::RAIL_TRAIL)),
                    hit_effect: Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG2)),
                    damage: Some(1250.0),
                    pierce_damage_factor: Some(0.5),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(110.0);
            weapon.cooldown_time = Some(90.0);
            weapon.mirror = Some(false);
            weapon.x = Some(0.0);
            weapon.y = Some(-3.5);
            weapon.rotate_speed = Some(1.4);
            weapon.rotate = Some(true);
            weapon.shoot_y = Some(23.0);
            weapon.shake = Some(6.0);
            weapon.recoil = Some(10.5);
            weapon.shadow = Some(50.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_OMURA);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon
        });
        unit
    })
}

fn retusa(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("retusa", UnitKind::UnitType, entity::NAVAL);
        unit.speed = Some(0.9);
        unit.drag = Some(0.14);
        unit.hit_size = Some(11.0);
        unit.health = Some(270.0);
        unit.accel = Some(0.4);
        unit.rotate_speed = Some(5.0);
        unit.trail_length = Some(20);
        unit.wave_trail_x = Some(5.0);
        unit.trail_scl = Some(1.3);
        unit.face_target = Some(false);
        unit.range = Some(100.0);
        unit.armor = Some(3.0);
        unit.move_sound_volume = Some(0.4);
        unit.move_sound = Some(SoundId::SHIP_MOVE);
        unit.build_speed = Some(1.5);
        unit.rotate_to_building = Some(false);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "repair-beam-weapon-center",
                kind: WeaponKind::RepairBeamWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    max_range: Some(120.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.x = Some(0.0);
            weapon.y = Some(-5.5);
            weapon.shoot_y = Some(6.0);
            weapon.beam_width = Some(0.8);
            weapon.mirror = Some(false);
            weapon.repair_speed = Some(0.75);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "retusa-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::LaserBolt,
                    speed: Some(5.2),
                    damage: Some(12.0),
                    lifetime: Some(30.0),
                    heal_percent: Some(5.5),
                    collides_team: Some(true),
                    back_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_LASER);
            weapon.reload = Some(22.0);
            weapon.x = Some(4.5);
            weapon.y = Some(-3.5);
            weapon.rotate_speed = Some(5.0);
            weapon.mirror = Some(true);
            weapon.rotate = Some(true);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(3, 7.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    sprite: Some(Some(String::from("mine-bullet"))),
                    width: Some(8.0),
                    height: Some(8.0),
                    layer: Some(10.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::NONE)),
                    smoke_effect: Some(EffectRef::Named(EffectId::NONE)),
                    max_range: Some(50.0),
                    heal_percent: Some(4.0),
                    back_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    mix_color_to: Some(Rgba::WHITE),
                    hit_sound: Some(SoundId::EXPLOSION_PLASMA_SMALL),
                    underwater: Some(true),
                    hit_size: Some(22.0),
                    collides_air: Some(false),
                    lifetime: Some(87.0),
                    keep_velocity: Some(false),
                    shrink_x: Some(0.0),
                    shrink_y: Some(0.0),
                    inaccuracy: Some(2.0),
                    weave_mag: Some(5.0),
                    weave_scale: Some(4.0),
                    speed: Some(0.7),
                    drag: Some(-0.017),
                    homing_power: Some(0.05),
                    collide_floor: Some(true),
                    trail_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    trail_width: Some(3.0),
                    trail_length: Some(8),
                    splash_damage: Some(40.0),
                    splash_damage_radius: Some(32.0),
                    hit_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        80.0,
                        vec![
                            EffectRef::Named(EffectId::BLAST_EXPLOSION),
                            EffectRef::Named(EffectId::GREEN_CLOUD),
                        ],
                    )))),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.mirror = Some(false);
            weapon.rotate = Some(true);
            weapon.reload = Some(90.0);
            weapon.x = Some(0.0);
            weapon.y = Some(0.0);
            weapon.shoot_x = Some(0.0);
            weapon.shoot_y = Some(0.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_RETUSA);
            weapon.rotate_speed = Some(180.0);
            weapon.shoot_sound_volume = Some(0.9);
            weapon.ignore_rotation = Some(true);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon
        });
        unit
    })
}

fn oxynoe(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("oxynoe", UnitKind::UnitType, entity::NAVAL);
        unit.health = Some(560.0);
        unit.speed = Some(0.83);
        unit.drag = Some(0.14);
        unit.hit_size = Some(14.0);
        unit.armor = Some(4.0);
        unit.accel = Some(0.4);
        unit.rotate_speed = Some(4.0);
        unit.face_target = Some(false);
        unit.move_sound_volume = Some(0.55);
        unit.move_sound_pitch_min = Some(0.9);
        unit.move_sound_pitch_max = Some(0.9);
        unit.move_sound = Some(SoundId::SHIP_MOVE);
        unit.trail_length = Some(22);
        unit.wave_trail_x = Some(5.5);
        unit.wave_trail_y = Some(-4.0);
        unit.trail_scl = Some(1.9);
        unit.build_speed = Some(2.0);
        unit.rotate_to_building = Some(false);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "plasma-mount-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    speed: Some(3.4),
                    damage: Some(23.0),
                    heal_percent: Some(1.5),
                    collides_team: Some(true),
                    ammo_multiplier: Some(3.0),
                    hit_size: Some(7.0),
                    lifetime: Some(18.0),
                    pierce: Some(true),
                    collides_air: Some(false),
                    status_duration: Some(240.0),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_FLAME_PLASMA)),
                    despawn_effect: Some(EffectRef::Named(EffectId::NONE)),
                    status: Some("burning"),
                    keep_velocity: Some(false),
                    hittable: Some(false),
                    shoot_effect: Some(EffectRef::Inline(Box::new(EffectSpec::plain_clip(
                        32.0, 80.0,
                    )))),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(5.0);
            weapon.x = Some(4.5);
            weapon.y = Some(6.5);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(5.0);
            weapon.inaccuracy = Some(10.0);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon.shoot_sound = Some(SoundId::SHOOT_FLAME_PLASMA);
            weapon.shoot_sound_volume = Some(0.9);
            weapon.shoot_cone = Some(30.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "point-defense-mount",
                kind: WeaponKind::PointDefenseWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    shoot_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    hit_effect: Some(EffectRef::Named(EffectId::POINT_HIT)),
                    max_range: Some(100.0),
                    damage: Some(17.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.mirror = Some(false);
            weapon.x = Some(0.0);
            weapon.y = Some(1.0);
            weapon.reload = Some(9.0);
            weapon.target_interval = Some(10.0);
            weapon.target_switch_interval = Some(15.0);
            weapon
        });
        unit.abilities
            .push(AbilitySpec::status_field("overclock", 360.0, 360.0, 60.0));
        unit
    })
}

fn cyerce(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("cyerce", UnitKind::UnitType, entity::NAVAL);
        unit.health = Some(870.0);
        unit.speed = Some(0.86);
        unit.accel = Some(0.22);
        unit.rotate_speed = Some(2.6);
        unit.drag = Some(0.16);
        unit.hit_size = Some(20.0);
        unit.armor = Some(6.0);
        unit.face_target = Some(false);
        unit.move_sound_volume = Some(0.7);
        unit.move_sound_pitch_min = Some(0.77);
        unit.move_sound_pitch_max = Some(0.77);
        unit.move_sound = Some(SoundId::SHIP_MOVE);
        unit.trail_length = Some(23);
        unit.wave_trail_x = Some(9.0);
        unit.wave_trail_y = Some(-9.0);
        unit.trail_scl = Some(2.0);
        unit.build_speed = Some(2.0);
        unit.rotate_to_building = Some(false);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "repair-beam-weapon-center",
                kind: WeaponKind::RepairBeamWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    max_range: Some(130.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.x = Some(11.0);
            weapon.y = Some(-10.0);
            weapon.shoot_y = Some(6.0);
            weapon.beam_width = Some(0.8);
            weapon.repair_speed = Some(0.7);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "plasma-missile-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Flak,
                    speed: Some(2.5),
                    damage: Some(25.0),
                    sprite: Some(Some(String::from("missile-large"))),
                    collides_ground: Some(true),
                    collides_air: Some(true),
                    explode_range: Some(40.0),
                    width: Some(12.0),
                    height: Some(12.0),
                    shrink_y: Some(0.0),
                    drag: Some(-0.003),
                    homing_range: Some(60.0),
                    keep_velocity: Some(false),
                    light_radius: Some(60.0),
                    light_opacity: Some(0.7),
                    light_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    despawn_sound: Some(SoundId::EXPLOSION),
                    splash_damage_radius: Some(30.0),
                    splash_damage: Some(25.0),
                    lifetime: Some(80.0),
                    back_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    weave_scale: Some(8.0),
                    weave_mag: Some(1.0),
                    trail_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    trail_width: Some(4.5),
                    trail_length: Some(29),
                    frag_bullets: Some(7),
                    frag_velocity_min: Some(0.3),
                    hit_effect: Some({
                        let mut effect = EffectSpec::explosion();
                        effect.lifetime = 28.0;
                        effect.wave_stroke = 6.0;
                        effect.wave_life = 10.0;
                        effect.wave_rad_base = 7.0;
                        effect.wave_color = Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0));
                        effect.wave_rad = 30.0;
                        effect.smokes = 6;
                        effect.smoke_color = Some(Rgba::WHITE);
                        effect.spark_color = Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0));
                        effect.sparks = 6;
                        effect.spark_rad = 35.0;
                        effect.spark_stroke = 1.5;
                        effect.spark_len = 4.0;
                        EffectRef::Inline(Box::new(effect))
                    }),
                    frag_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Missile,
                        speed: Some(3.9),
                        damage: Some(11.0),
                        homing_power: Some(0.2),
                        weave_mag: Some(4.0),
                        weave_scale: Some(4.0),
                        lifetime: Some(60.0),
                        keep_velocity: Some(false),
                        shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_HEAL)),
                        smoke_effect: Some(EffectRef::Named(EffectId::HIT_LASER)),
                        splash_damage: Some(13.0),
                        splash_damage_radius: Some(20.0),
                        front_color: Some(Rgba::WHITE),
                        hit_sound: Some(SoundId::NONE),
                        light_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                        light_radius: Some(40.0),
                        light_opacity: Some(0.7),
                        trail_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                        trail_width: Some(2.5),
                        trail_length: Some(20),
                        trail_chance: Some(-1.0),
                        heal_percent: Some(2.8),
                        collides_team: Some(true),
                        back_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                        despawn_effect: Some(EffectRef::Named(EffectId::NONE)),
                        hit_effect: Some({
                            let mut effect = EffectSpec::explosion();
                            effect.lifetime = 20.0;
                            effect.wave_stroke = 2.0;
                            effect.wave_color = Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0));
                            effect.wave_rad = 12.0;
                            effect.smoke_size = 0.0;
                            effect.smoke_size_base = 0.0;
                            effect.spark_color = Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0));
                            effect.sparks = 9;
                            effect.spark_rad = 35.0;
                            effect.spark_len = 4.0;
                            effect.spark_stroke = 1.5;
                            EffectRef::Inline(Box::new(effect))
                        }),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(60.0);
            weapon.x = Some(9.0);
            weapon.y = Some(3.0);
            weapon.shadow = Some(5.0);
            weapon.rotate_speed = Some(4.0);
            weapon.rotate = Some(true);
            weapon.inaccuracy = Some(1.0);
            weapon.velocity_rnd = Some(0.1);
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE_PLASMA);
            weapon.eject_effect = Some(EffectRef::Named(EffectId::NONE));
            weapon
        });
        unit
    })
}

fn aegires(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("aegires", UnitKind::UnitType, entity::NAVAL);
        unit.health = Some(12000.0);
        unit.armor = Some(12.0);
        unit.speed = Some(0.7);
        unit.drag = Some(0.17);
        unit.hit_size = Some(44.0);
        unit.accel = Some(0.2);
        unit.rotate_speed = Some(1.4);
        unit.face_target = Some(false);
        unit.move_sound_volume = Some(1.0);
        unit.move_sound = Some(SoundId::SHIP_MOVE_BIG);
        unit.move_sound_pitch_min = Some(0.95);
        unit.move_sound_pitch_max = Some(0.95);
        unit.clip_size = Some(250.0);
        unit.trail_length = Some(50);
        unit.wave_trail_x = Some(18.0);
        unit.wave_trail_y = Some(-17.0);
        unit.trail_scl = Some(3.2);
        unit.build_speed = Some(3.0);
        unit.rotate_to_building = Some(false);
        unit.range = Some(180.0);
        unit.max_range = Some(180.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "point-defense-mount",
                kind: WeaponKind::PointDefenseWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    shoot_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    hit_effect: Some(EffectRef::Named(EffectId::POINT_HIT)),
                    max_range: Some(180.0),
                    damage: Some(30.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.x = Some(12.5);
            weapon.y = Some(-18.0);
            weapon.reload = Some(4.0);
            weapon.target_interval = Some(8.0);
            weapon.target_switch_interval = Some(8.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "point-defense-mount",
                kind: WeaponKind::PointDefenseWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    shoot_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    hit_effect: Some(EffectRef::Named(EffectId::POINT_HIT)),
                    max_range: Some(180.0),
                    damage: Some(30.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.x = Some(12.5);
            weapon.y = Some(14.0);
            weapon.reload = Some(4.0);
            weapon.target_interval = Some(8.0);
            weapon.target_switch_interval = Some(8.0);
            weapon
        });
        unit.abilities.push({
            let mut ability = AbilitySpec::energy_field(40.0, 65.0, 180.0);
            ability.duration = 360.0;
            ability.max_targets = 25;
            ability.heal_percent = 1.5;
            ability.same_type_heal_mult = 0.5;
            ability
        });
        unit
    })
}

fn navanax(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("navanax", UnitKind::UnitType, entity::NAVAL);
        unit.health = Some(20000.0);
        unit.speed = Some(0.65);
        unit.drag = Some(0.17);
        unit.hit_size = Some(58.0);
        unit.armor = Some(20.0);
        unit.accel = Some(0.2);
        unit.rotate_speed = Some(1.1);
        unit.face_target = Some(false);
        unit.move_sound_volume = Some(1.1);
        unit.move_sound = Some(SoundId::SHIP_MOVE_BIG);
        unit.move_sound_pitch_min = Some(0.9);
        unit.move_sound_pitch_max = Some(0.9);
        unit.trail_length = Some(70);
        unit.wave_trail_x = Some(23.0);
        unit.wave_trail_y = Some(-32.0);
        unit.trail_scl = Some(3.5);
        unit.build_speed = Some(3.5);
        unit.rotate_to_building = Some(false);
        unit.clip_size = Some(250.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "emp-cannon-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Emp,
                    scale_life: Some(true),
                    light_opacity: Some(0.7),
                    unit_damage_scl: Some(0.8),
                    heal_percent: Some(20.0),
                    time_increase: Some(3.0),
                    time_duration: Some(1200.0),
                    power_damage_scl: Some(3.0),
                    damage: Some(110.0),
                    hit_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    light_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    light_radius: Some(70.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::HIT_EMP_SPARK)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE2)),
                    lifetime: Some(60.0),
                    sprite: Some(Some(String::from("circle-bullet"))),
                    back_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    width: Some(12.0),
                    height: Some(12.0),
                    shrink_y: Some(0.0),
                    speed: Some(5.0),
                    trail_length: Some(20),
                    trail_width: Some(6.0),
                    trail_color: Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0)),
                    trail_interval: Some(3.0),
                    splash_damage: Some(110.0),
                    splash_damage_radius: Some(100.0),
                    hit_shake: Some(4.0),
                    trail_rotation: Some(true),
                    status: Some("electrified"),
                    hit_sound: Some(SoundId::EXPLOSION_NAVANAX),
                    trail_effect: Some(EffectRef::Inline(Box::new(EffectSpec::plain(16.0)))),
                    hit_effect: Some(EffectRef::Inline(Box::new(EffectSpec::plain_clip(
                        50.0, 100.0,
                    )))),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.rotate = Some(true);
            weapon.x = Some(17.5);
            weapon.y = Some(-6.5);
            weapon.reload = Some(65.0);
            weapon.shake = Some(3.0);
            weapon.rotate_speed = Some(2.0);
            weapon.shadow = Some(30.0);
            weapon.shoot_y = Some(7.0);
            weapon.recoil = Some(4.0);
            weapon.cooldown_time = Some(55.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_NAVANAX);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "plasma-laser-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::ContinuousLaser,
                    max_range: Some(90.0),
                    damage: Some(27.0),
                    length: Some(95.0),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_MELT_HEAL)),
                    draw_size: Some(200.0),
                    lifetime: Some(155.0),
                    shake: Some(1.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_HEAL)),
                    smoke_effect: Some(EffectRef::Named(EffectId::NONE)),
                    width: Some(4.0),
                    large_hit: Some(false),
                    incend_chance: Some(0.03),
                    incend_spread: Some(5.0),
                    incend_amount: Some(1),
                    heal_percent: Some(0.4),
                    collides_team: Some(true),
                    colors: Some(vec![
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.2),
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.5),
                        Rgba::new(0.7152941, 1.0, 0.79529417, 1.0),
                        Rgba::WHITE,
                    ]),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shadow = Some(20.0);
            weapon.controllable = Some(false);
            weapon.auto_target = Some(true);
            weapon.mirror = Some(false);
            weapon.shake = Some(3.0);
            weapon.shoot_y = Some(7.0);
            weapon.rotate = Some(true);
            weapon.x = Some(-21.0);
            weapon.y = Some(-29.25);
            weapon.target_interval = Some(20.0);
            weapon.target_switch_interval = Some(35.0);
            weapon.rotate_speed = Some(3.5);
            weapon.reload = Some(170.0);
            weapon.recoil = Some(1.0);
            weapon.shoot_sound = Some(SoundId::BEAM_PLASMA_SMALL);
            weapon.initial_shoot_sound = Some(SoundId::SHOOT_BEAM_PLASMA_SMALL);
            weapon.continuous = Some(true);
            weapon.cooldown_time = Some(170.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "plasma-laser-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::ContinuousLaser,
                    max_range: Some(90.0),
                    damage: Some(27.0),
                    length: Some(95.0),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_MELT_HEAL)),
                    draw_size: Some(200.0),
                    lifetime: Some(155.0),
                    shake: Some(1.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_HEAL)),
                    smoke_effect: Some(EffectRef::Named(EffectId::NONE)),
                    width: Some(4.0),
                    large_hit: Some(false),
                    incend_chance: Some(0.03),
                    incend_spread: Some(5.0),
                    incend_amount: Some(1),
                    heal_percent: Some(0.4),
                    collides_team: Some(true),
                    colors: Some(vec![
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.2),
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.5),
                        Rgba::new(0.7152941, 1.0, 0.79529417, 1.0),
                        Rgba::WHITE,
                    ]),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shadow = Some(20.0);
            weapon.controllable = Some(false);
            weapon.auto_target = Some(true);
            weapon.mirror = Some(false);
            weapon.shake = Some(3.0);
            weapon.shoot_y = Some(7.0);
            weapon.rotate = Some(true);
            weapon.x = Some(21.0);
            weapon.y = Some(-29.25);
            weapon.target_interval = Some(20.0);
            weapon.target_switch_interval = Some(35.0);
            weapon.rotate_speed = Some(3.5);
            weapon.reload = Some(170.0);
            weapon.recoil = Some(1.0);
            weapon.shoot_sound = Some(SoundId::BEAM_PLASMA_SMALL);
            weapon.initial_shoot_sound = Some(SoundId::SHOOT_BEAM_PLASMA_SMALL);
            weapon.continuous = Some(true);
            weapon.cooldown_time = Some(170.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "plasma-laser-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::ContinuousLaser,
                    max_range: Some(90.0),
                    damage: Some(27.0),
                    length: Some(95.0),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_MELT_HEAL)),
                    draw_size: Some(200.0),
                    lifetime: Some(155.0),
                    shake: Some(1.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_HEAL)),
                    smoke_effect: Some(EffectRef::Named(EffectId::NONE)),
                    width: Some(4.0),
                    large_hit: Some(false),
                    incend_chance: Some(0.03),
                    incend_spread: Some(5.0),
                    incend_amount: Some(1),
                    heal_percent: Some(0.4),
                    collides_team: Some(true),
                    colors: Some(vec![
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.2),
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.5),
                        Rgba::new(0.7152941, 1.0, 0.79529417, 1.0),
                        Rgba::WHITE,
                    ]),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shadow = Some(20.0);
            weapon.controllable = Some(false);
            weapon.auto_target = Some(true);
            weapon.mirror = Some(false);
            weapon.shake = Some(3.0);
            weapon.shoot_y = Some(7.0);
            weapon.rotate = Some(true);
            weapon.x = Some(-21.0);
            weapon.y = Some(12.5);
            weapon.target_interval = Some(20.0);
            weapon.target_switch_interval = Some(35.0);
            weapon.rotate_speed = Some(3.5);
            weapon.reload = Some(170.0);
            weapon.recoil = Some(1.0);
            weapon.shoot_sound = Some(SoundId::BEAM_PLASMA_SMALL);
            weapon.initial_shoot_sound = Some(SoundId::SHOOT_BEAM_PLASMA_SMALL);
            weapon.continuous = Some(true);
            weapon.cooldown_time = Some(170.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "plasma-laser-mount",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::ContinuousLaser,
                    max_range: Some(90.0),
                    damage: Some(27.0),
                    length: Some(95.0),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_MELT_HEAL)),
                    draw_size: Some(200.0),
                    lifetime: Some(155.0),
                    shake: Some(1.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_HEAL)),
                    smoke_effect: Some(EffectRef::Named(EffectId::NONE)),
                    width: Some(4.0),
                    large_hit: Some(false),
                    incend_chance: Some(0.03),
                    incend_spread: Some(5.0),
                    incend_amount: Some(1),
                    heal_percent: Some(0.4),
                    collides_team: Some(true),
                    colors: Some(vec![
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.2),
                        Rgba::new(0.59607846, 1.0, 0.6627451, 0.5),
                        Rgba::new(0.7152941, 1.0, 0.79529417, 1.0),
                        Rgba::WHITE,
                    ]),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shadow = Some(20.0);
            weapon.controllable = Some(false);
            weapon.auto_target = Some(true);
            weapon.mirror = Some(false);
            weapon.shake = Some(3.0);
            weapon.shoot_y = Some(7.0);
            weapon.rotate = Some(true);
            weapon.x = Some(21.0);
            weapon.y = Some(12.5);
            weapon.target_interval = Some(20.0);
            weapon.target_switch_interval = Some(35.0);
            weapon.rotate_speed = Some(3.5);
            weapon.reload = Some(170.0);
            weapon.recoil = Some(1.0);
            weapon.shoot_sound = Some(SoundId::BEAM_PLASMA_SMALL);
            weapon.initial_shoot_sound = Some(SoundId::SHOOT_BEAM_PLASMA_SMALL);
            weapon.continuous = Some(true);
            weapon.cooldown_time = Some(170.0);
            weapon
        });
        unit.abilities.push({
            let mut ability = AbilitySpec::suppression_field();
            ability.orb_radius = 5.0;
            ability.particle_size = 3.0;
            ability.y = -10.0;
            ability.particles = 10;
            ability.color = Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0));
            ability.particle_color = Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0));
            ability.effect_color = Some(Rgba::new(0.59607846, 1.0, 0.6627451, 1.0));
            ability
        });
        unit.immunities.push("burning");
        unit.immunities.push("burning");
        unit.immunities.push("burning");
        unit.immunities.push("burning");
        unit
    })
}

fn alpha(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("alpha", UnitKind::UnitType, entity::AIR_LEGACY_ALPHA);
        unit.controller = Some(ControllerKind::BuilderOrCommand);
        unit.is_enemy = Some(false);
        unit.target_buildings_mobile = Some(false);
        unit.low_altitude = Some(true);
        unit.flying = Some(true);
        unit.mine_speed = Some(6.5);
        unit.mine_tier = Some(1);
        unit.build_speed = Some(0.5);
        unit.drag = Some(0.05);
        unit.speed = Some(3.0);
        unit.rotate_speed = Some(15.0);
        unit.accel = Some(0.1);
        unit.fog_radius = Some(0.0);
        unit.item_capacity = Some(30);
        unit.health = Some(150.0);
        unit.engine_offset = Some(6.0);
        unit.hit_size = Some(8.0);
        unit.always_unlocked = Some(true);
        unit.wreck_sound_volume = Some(0.8);
        unit.death_sound_volume = Some(0.7);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "small-basic-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::LaserBolt,
                    speed: Some(2.5),
                    damage: Some(11.0),
                    scale_keep_velocity: Some(true),
                    width: Some(1.5),
                    height: Some(4.5),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    despawn_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    trail_width: Some(1.2),
                    trail_length: Some(3),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL_COLOR)),
                    smoke_effect: Some(EffectRef::Named(EffectId::HIT_LASER_COLOR)),
                    back_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    trail_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    hit_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    light_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    lifetime: Some(60.0),
                    building_damage_multiplier: Some(0.0),
                    homing_power: Some(0.02),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(17.0);
            weapon.x = Some(2.75);
            weapon.y = Some(1.0);
            weapon.top = Some(false);
            weapon.shoot_sound = Some(SoundId::SHOOT_ALPHA);
            weapon
        });
        unit
    })
}

fn beta(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("beta", UnitKind::UnitType, entity::AIR_LEGACY_ALPHA);
        unit.controller = Some(ControllerKind::BuilderOrCommand);
        unit.is_enemy = Some(false);
        unit.target_buildings_mobile = Some(false);
        unit.flying = Some(true);
        unit.mine_speed = Some(7.0);
        unit.mine_tier = Some(1);
        unit.build_speed = Some(0.75);
        unit.drag = Some(0.05);
        unit.speed = Some(3.3);
        unit.rotate_speed = Some(17.0);
        unit.accel = Some(0.1);
        unit.fog_radius = Some(0.0);
        unit.item_capacity = Some(50);
        unit.health = Some(170.0);
        unit.engine_offset = Some(6.0);
        unit.hit_size = Some(9.0);
        unit.low_altitude = Some(true);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "small-mount-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(2, 4.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::LaserBolt,
                    speed: Some(3.0),
                    damage: Some(11.0),
                    scale_keep_velocity: Some(true),
                    width: Some(1.5),
                    height: Some(4.5),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    despawn_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    trail_width: Some(1.2),
                    trail_length: Some(3),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL_COLOR)),
                    smoke_effect: Some(EffectRef::Named(EffectId::HIT_LASER_COLOR)),
                    back_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    trail_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    hit_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    light_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    lifetime: Some(60.0),
                    building_damage_multiplier: Some(0.0),
                    homing_power: Some(0.03),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.reload = Some(20.0);
            weapon.x = Some(3.0);
            weapon.y = Some(1.0);
            weapon.recoil = Some(1.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_ALPHA);
            weapon
        });
        unit
    })
}

fn gamma(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("gamma", UnitKind::UnitType, entity::AIR_LEGACY_ALPHA);
        unit.controller = Some(ControllerKind::BuilderOrCommand);
        unit.is_enemy = Some(false);
        unit.target_buildings_mobile = Some(false);
        unit.low_altitude = Some(true);
        unit.flying = Some(true);
        unit.mine_speed = Some(8.0);
        unit.mine_tier = Some(2);
        unit.build_speed = Some(1.0);
        unit.drag = Some(0.05);
        unit.speed = Some(3.55);
        unit.rotate_speed = Some(19.0);
        unit.accel = Some(0.11);
        unit.fog_radius = Some(0.0);
        unit.item_capacity = Some(70);
        unit.health = Some(220.0);
        unit.engine_offset = Some(6.0);
        unit.hit_size = Some(11.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "small-mount-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some({
                    let mut shoot = ShootPatternSpec::spread(2, 5.0);
                    shoot.shot_delay = 3.0;
                    shoot.spread = 2.0;
                    shoot
                }),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::LaserBolt,
                    speed: Some(3.5),
                    damage: Some(11.0),
                    scale_keep_velocity: Some(true),
                    width: Some(1.5),
                    height: Some(5.0),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    despawn_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    trail_width: Some(1.2),
                    trail_length: Some(4),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_SMALL_COLOR)),
                    smoke_effect: Some(EffectRef::Named(EffectId::HIT_LASER_COLOR)),
                    back_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    trail_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    hit_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    light_color: Some(Rgba::new(1.0, 0.8235294, 0.49411765, 1.0)),
                    lifetime: Some(70.0),
                    building_damage_multiplier: Some(0.0),
                    homing_power: Some(0.04),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.top = Some(false);
            weapon.reload = Some(15.0);
            weapon.x = Some(1.0);
            weapon.y = Some(2.0);
            weapon.inaccuracy = Some(3.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_ALPHA);
            weapon
        });
        unit
    })
}

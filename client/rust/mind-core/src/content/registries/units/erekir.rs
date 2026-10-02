// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/UnitTypes.java (wave `erekir`).
//
//! Generated unit metadata (`UnitTypes.java`, wave `erekir`).
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

/// Loads the `erekir` wave in upstream order.
pub fn load(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    stell(sink)?;
    locus(sink)?;
    precept(sink)?;
    vanquish(sink)?;
    conquer(sink)?;
    merui(sink)?;
    cleroi(sink)?;
    anthicus(sink)?;
    tecta(sink)?;
    collaris(sink)?;
    elude(sink)?;
    avert(sink)?;
    obviate(sink)?;
    quell(sink)?;
    disrupt(sink)?;
    renale(sink)?;
    latum(sink)?;
    evoke(sink)?;
    incite(sink)?;
    emanate(sink)?;
    Ok(())
}

fn stell(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("stell", UnitKind::TankUnitType, entity::TANK);
        unit.hit_size = Some(12.0);
        unit.tread_pull_offset = Some(3);
        unit.speed = Some(0.75);
        unit.rotate_speed = Some(3.5);
        unit.health = Some(850.0);
        unit.armor = Some(6.0);
        unit.item_capacity = Some(0);
        unit.floor_multiplier = Some(0.95);
        unit.research_cost_multiplier = Some(0.0);
        unit.tank_move_sound = Some(SoundId::TANK_MOVE_SMALL);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "stell-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(4.0),
                    damage: Some(40.0),
                    sprite: Some(Some(String::from("missile-large"))),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE)),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_COLOR)),
                    width: Some(5.0),
                    height: Some(7.0),
                    lifetime: Some(40.0),
                    hit_size: Some(4.0),
                    hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    trail_width: Some(1.7),
                    trail_length: Some(5),
                    despawn_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_STELL);
            weapon.layer_offset = Some(0.0001);
            weapon.reload = Some(50.0);
            weapon.shoot_y = Some(4.5);
            weapon.recoil = Some(1.0);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(2.2);
            weapon.mirror = Some(false);
            weapon.x = Some(0.0);
            weapon.y = Some(-0.75);
            weapon.heat_color = Some(Rgba::new(0.9764706, 0.20784314, 0.05882353, 1.0));
            weapon.cooldown_time = Some(30.0);
            weapon
        });
        unit.tread_rects = vec![TreadRect {
            x: -20.0,
            y: -25.0,
            width: 14.0,
            height: 51.0,
        }];
        unit
    })
}

fn locus(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("locus", UnitKind::TankUnitType, entity::TANK);
        unit.hit_size = Some(18.0);
        unit.tread_pull_offset = Some(5);
        unit.speed = Some(0.7);
        unit.rotate_speed = Some(2.6);
        unit.health = Some(2100.0);
        unit.armor = Some(8.0);
        unit.item_capacity = Some(0);
        unit.floor_multiplier = Some(0.8);
        unit.crush_fragile = Some(true);
        unit.research_cost_multiplier = Some(0.0);
        unit.tank_move_sound = Some(SoundId::TANK_MOVE);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "locus-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::alternate(1, 0.0, 3.5, 2)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Rail,
                    length: Some(160.0),
                    damage: Some(48.0),
                    hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    pierce_damage_factor: Some(0.8),
                    smoke_effect: Some(EffectRef::Named(EffectId::COLOR_SPARK)),
                    end_effect: Some(EffectRef::Inline(Box::new(EffectSpec::plain(14.0)))),
                    shoot_effect: Some(EffectRef::Inline(Box::new(EffectSpec::plain(10.0)))),
                    line_effect: Some(EffectRef::Inline(Box::new(EffectSpec::plain(20.0)))),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_LOCUS);
            weapon.layer_offset = Some(0.0001);
            weapon.reload = Some(18.0);
            weapon.shoot_y = Some(10.0);
            weapon.recoil = Some(1.0);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(1.4);
            weapon.mirror = Some(false);
            weapon.shoot_cone = Some(2.0);
            weapon.x = Some(0.0);
            weapon.y = Some(0.0);
            weapon.heat_color = Some(Rgba::new(0.9764706, 0.20784314, 0.05882353, 1.0));
            weapon.cooldown_time = Some(30.0);
            weapon
        });
        unit.tread_rects = vec![TreadRect {
            x: -31.0,
            y: -38.0,
            width: 19.0,
            height: 76.0,
        }];
        unit
    })
}

fn precept(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("precept", UnitKind::TankUnitType, entity::TANK);
        unit.hit_size = Some(24.0);
        unit.tread_pull_offset = Some(5);
        unit.speed = Some(0.64);
        unit.rotate_speed = Some(1.5);
        unit.health = Some(5000.0);
        unit.armor = Some(11.0);
        unit.item_capacity = Some(0);
        unit.floor_multiplier = Some(0.65);
        unit.drown_time_multiplier = Some(1.2);
        unit.crush_fragile = Some(true);
        unit.research_cost_multiplier = Some(0.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "precept-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(7.0),
                    damage: Some(120.0),
                    sprite: Some(Some(String::from("missile-large"))),
                    width: Some(7.5),
                    height: Some(13.0),
                    lifetime: Some(28.0),
                    hit_size: Some(6.0),
                    pierce_cap: Some(2),
                    pierce: Some(true),
                    pierce_building: Some(true),
                    hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    trail_width: Some(2.8),
                    trail_length: Some(8),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    despawn_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_TITAN)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMOKE_TITAN)),
                    splash_damage_radius: Some(20.0),
                    splash_damage: Some(50.0),
                    trail_effect: Some(EffectRef::Named(EffectId::HIT_SQUARES_COLOR)),
                    trail_rotation: Some(true),
                    trail_interval: Some(3.0),
                    frag_bullets: Some(4),
                    frag_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Basic,
                        speed: Some(5.0),
                        damage: Some(35.0),
                        sprite: Some(Some(String::from("missile-large"))),
                        width: Some(5.0),
                        height: Some(7.0),
                        lifetime: Some(15.0),
                        hit_size: Some(4.0),
                        pierce_cap: Some(3),
                        pierce: Some(true),
                        pierce_building: Some(true),
                        hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                        back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                        trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                        front_color: Some(Rgba::WHITE),
                        trail_width: Some(1.7),
                        trail_length: Some(3),
                        drag: Some(0.01),
                        despawn_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                        hit_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::EXPLOSION_DULL);
            weapon.layer_offset = Some(0.0001);
            weapon.reload = Some(80.0);
            weapon.shoot_y = Some(16.0);
            weapon.recoil = Some(3.0);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(1.625);
            weapon.mirror = Some(false);
            weapon.shoot_cone = Some(2.0);
            weapon.x = Some(0.0);
            weapon.y = Some(-1.0);
            weapon.heat_color = Some(Rgba::new(0.9764706, 0.20784314, 0.05882353, 1.0));
            weapon.cooldown_time = Some(30.0);
            weapon
        });
        unit.immunities.push("burning");
        unit.immunities.push("melting");
        unit.tread_rects = vec![
            TreadRect {
                x: -44.0,
                y: -22.0,
                width: 30.0,
                height: 75.0,
            },
            TreadRect {
                x: -16.0,
                y: -53.0,
                width: 17.0,
                height: 60.0,
            },
        ];
        unit
    })
}

fn vanquish(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("vanquish", UnitKind::TankUnitType, entity::TANK);
        unit.hit_size = Some(28.0);
        unit.tread_pull_offset = Some(4);
        unit.speed = Some(0.63);
        unit.health = Some(11000.0);
        unit.armor = Some(20.0);
        unit.item_capacity = Some(0);
        unit.crush_damage = Some(2.6);
        unit.floor_multiplier = Some(0.5);
        unit.drown_time_multiplier = Some(1.25);
        unit.crush_fragile = Some(true);
        unit.tank_move_sound = Some(SoundId::TANK_MOVE_HEAVY);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "vanquish-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(8.0),
                    damage: Some(150.0),
                    sprite: Some(Some(String::from("missile-large"))),
                    width: Some(9.5),
                    height: Some(18.0),
                    lifetime: Some(16.0),
                    hit_size: Some(6.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_TITAN)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMOKE_TITAN)),
                    pierce_cap: Some(2),
                    pierce: Some(true),
                    pierce_building: Some(true),
                    hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    trail_width: Some(3.1),
                    trail_length: Some(8),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    despawn_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    splash_damage_radius: Some(20.0),
                    splash_damage: Some(50.0),
                    max_range: Some(190.0),
                    frag_on_hit: Some(false),
                    pierce_frag_cap: Some(1),
                    frag_random_spread: Some(0.0),
                    frag_spread: Some(10.0),
                    frag_bullets: Some(5),
                    frag_velocity_min: Some(1.0),
                    despawn_sound: Some(SoundId::EXPLOSION_DULL),
                    frag_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Basic,
                        speed: Some(8.0),
                        damage: Some(35.0),
                        sprite: Some(Some(String::from("missile-large"))),
                        width: Some(8.0),
                        height: Some(16.0),
                        lifetime: Some(10.0),
                        hit_size: Some(4.0),
                        hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                        back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                        trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                        front_color: Some(Rgba::WHITE),
                        trail_width: Some(2.8),
                        trail_length: Some(6),
                        hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                        despawn_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                        splash_damage_radius: Some(10.0),
                        splash_damage: Some(20.0),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_TANK);
            weapon.layer_offset = Some(0.0001);
            weapon.reload = Some(80.0);
            weapon.shoot_y = Some(17.75);
            weapon.shake = Some(5.0);
            weapon.recoil = Some(4.0);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(1.0);
            weapon.mirror = Some(false);
            weapon.x = Some(0.0);
            weapon.y = Some(0.0);
            weapon.shadow = Some(28.0);
            weapon.heat_color = Some(Rgba::new(0.9764706, 0.20784314, 0.05882353, 1.0));
            weapon.cooldown_time = Some(80.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "vanquish-point-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(12.0),
                    damage: Some(50.0),
                    sprite: Some(Some(String::from("missile-large"))),
                    width: Some(6.5),
                    height: Some(11.0),
                    shrink_y: Some(0.0),
                    shrink_x: Some(0.2),
                    lifetime: Some(15.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE)),
                    hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    trail_width: Some(2.5),
                    trail_length: Some(5),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    despawn_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(22.0);
            weapon.x = Some(12.0);
            weapon.y = Some(8.5);
            weapon.shoot_y = Some(5.5);
            weapon.recoil = Some(2.0);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(2.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_STELL);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "vanquish-point-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(12.0),
                    damage: Some(50.0),
                    sprite: Some(Some(String::from("missile-large"))),
                    width: Some(6.5),
                    height: Some(11.0),
                    shrink_y: Some(0.0),
                    shrink_x: Some(0.2),
                    lifetime: Some(15.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE)),
                    hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    trail_width: Some(2.5),
                    trail_length: Some(5),
                    hit_effect: Some(EffectRef::Named(EffectId::BLAST_EXPLOSION)),
                    despawn_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.reload = Some(22.0);
            weapon.x = Some(12.0);
            weapon.y = Some(-9.0);
            weapon.shoot_y = Some(5.5);
            weapon.recoil = Some(2.0);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(2.0);
            weapon.shoot_sound = Some(SoundId::SHOOT_STELL);
            weapon
        });
        unit.immunities.push("burning");
        unit.immunities.push("melting");
        unit.tread_rects = vec![TreadRect {
            x: -55.0,
            y: -61.0,
            width: 28.0,
            height: 130.0,
        }];
        unit
    })
}

fn conquer(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("conquer", UnitKind::TankUnitType, entity::TANK);
        unit.hit_size = Some(46.0);
        unit.tread_pull_offset = Some(1);
        unit.speed = Some(0.48);
        unit.health = Some(22000.0);
        unit.armor = Some(26.0);
        unit.crush_damage = Some(5.0);
        unit.rotate_speed = Some(0.8);
        unit.floor_multiplier = Some(0.3);
        unit.tank_move_sound = Some(SoundId::TANK_MOVE_HEAVY);
        unit.crush_fragile = Some(true);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "conquer-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(8.0),
                    damage: Some(360.0),
                    sprite: Some(Some(String::from("missile-large"))),
                    width: Some(12.0),
                    height: Some(20.0),
                    lifetime: Some(35.0),
                    hit_size: Some(6.0),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMOKE_TITAN)),
                    pierce_cap: Some(3),
                    pierce: Some(true),
                    pierce_building: Some(true),
                    hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    trail_width: Some(4.0),
                    trail_length: Some(9),
                    hit_effect: Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION)),
                    despawn_effect: Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION)),
                    shoot_effect: Some({
                        let mut effect = EffectSpec::explosion();
                        effect.lifetime = 40.0;
                        effect.wave_stroke = 4.0;
                        effect.wave_color = Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                        effect.spark_color = Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                        effect.wave_rad = 15.0;
                        effect.smoke_size = 5.0;
                        effect.smokes = 8;
                        effect.smoke_size_base = 0.0;
                        effect.smoke_color = Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                        effect.sparks = 8;
                        effect.spark_rad = 40.0;
                        effect.spark_len = 4.0;
                        effect.spark_stroke = 3.0;
                        EffectRef::Inline(Box::new(effect))
                    }),
                    spawn_bullets: vec![
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(1.7333333),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(62.534245),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(2.5),
                            weave_mag: Some(-3.5666666),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(1.7333333),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(62.534245),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(true),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(2.5),
                            weave_mag: Some(3.5666666),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(3.0666666),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(55.60241),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(2.9166665),
                            weave_mag: Some(-3.2333333),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(3.0666666),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(55.60241),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(2.9166665),
                            weave_mag: Some(3.2333333),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(4.4),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(50.161293),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(3.3333333),
                            weave_mag: Some(-2.9),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(4.4),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(50.161293),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(3.3333333),
                            weave_mag: Some(2.9),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(5.733333),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(45.7767),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(3.7499998),
                            weave_mag: Some(-2.5666666),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(5.733333),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(45.7767),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(3.7499998),
                            weave_mag: Some(2.5666666),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(7.0666666),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(42.16814),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(4.1666665),
                            weave_mag: Some(-2.2333333),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(7.0666666),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(42.16814),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(4.1666665),
                            weave_mag: Some(2.2333333),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(8.4),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(39.146343),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(4.583333),
                            weave_mag: Some(-1.9000001),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                        BulletSpec {
                            kind: BulletKind::Basic,
                            speed: Some(8.4),
                            damage: Some(60.0),
                            drag: Some(0.002),
                            width: Some(12.0),
                            height: Some(11.0),
                            lifetime: Some(39.146343),
                            weave_random: Some(false),
                            hit_size: Some(5.0),
                            pierce_cap: Some(2),
                            pierce: Some(true),
                            show_stats: Some(false),
                            pierce_building: Some(true),
                            hit_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            back_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            trail_color: Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0)),
                            front_color: Some(Rgba::WHITE),
                            trail_width: Some(2.5),
                            trail_length: Some(7),
                            weave_scale: Some(4.583333),
                            weave_mag: Some(1.9000001),
                            splash_damage: Some(65.0),
                            splash_damage_radius: Some(30.0),
                            despawn_effect: Some({
                                let mut effect = EffectSpec::explosion();
                                effect.lifetime = 50.0;
                                effect.wave_stroke = 4.0;
                                effect.wave_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.spark_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.wave_rad = 30.0;
                                effect.smoke_size = 7.0;
                                effect.smokes = 6;
                                effect.smoke_size_base = 0.0;
                                effect.smoke_color =
                                    Some(Rgba::new(0.99607843, 0.7019608, 0.5019608, 1.0));
                                effect.sparks = 5;
                                effect.spark_rad = 30.0;
                                effect.spark_len = 3.0;
                                effect.spark_stroke = 1.5;
                                EffectRef::Inline(Box::new(effect))
                            }),
                            ..BulletSpec::default()
                        },
                    ],
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_CONQUER);
            weapon.layer_offset = Some(0.1);
            weapon.reload = Some(100.0);
            weapon.shoot_y = Some(32.5);
            weapon.shake = Some(5.0);
            weapon.recoil = Some(5.0);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(0.6);
            weapon.mirror = Some(false);
            weapon.x = Some(0.0);
            weapon.y = Some(-2.0);
            weapon.shadow = Some(50.0);
            weapon.heat_color = Some(Rgba::new(0.9764706, 0.20784314, 0.05882353, 1.0));
            weapon.shoot_warmup_speed = Some(0.06);
            weapon.cooldown_time = Some(110.0);
            weapon.min_warmup = Some(0.9);
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-glow");
                part.color = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
                part.blending = BlendingKind::Additive;
                part.outline = false;
                part.mirror = false;
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-sides");
                part.progress = PartProgressSpec::Warmup;
                part.mirror = true;
                part.under = true;
                part.move_x = 0.75;
                part.move_y = 0.75;
                part.move_rot = 82.0;
                part.x = 9.25;
                part.y = 2.0;
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-sinks");
                part.progress = PartProgressSpec::Warmup;
                part.mirror = true;
                part.under = true;
                part.heat_color = Some(Rgba::new(1.0, 0.1, 0.1, 1.0));
                part.move_x = 4.25;
                part.move_y = -3.75;
                part.x = 8.0;
                part.y = -8.5;
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-sinks-heat");
                part.blending = BlendingKind::Additive;
                part.progress = PartProgressSpec::Warmup;
                part.mirror = true;
                part.outline = false;
                part.color_to = Some(Rgba::new(1.0, 0.0, 0.0, 0.5));
                part.color = Some(Rgba::new(1.0, 0.0, 0.0, 0.0));
                part.move_x = 4.25;
                part.move_y = -3.75;
                part.x = 8.0;
                part.y = -8.5;
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.progress = PartProgressSpec::Blend(
                    Box::new(PartProgressSpec::Delay(
                        Box::new(PartProgressSpec::Warmup),
                        0.6,
                    )),
                    Box::new(PartProgressSpec::Reload),
                    0.3,
                );
                part.heat_progress = PartProgressSpec::Min(
                    Box::new(PartProgressSpec::Add(Box::new(PartProgressSpec::Heat), 0.3)),
                    Box::new(PartProgressSpec::Warmup),
                );
                part.heat_color = Some(Rgba::new(1.0, 0.1, 0.1, 1.0));
                part.mirror = true;
                part.under = true;
                part.move_rot = -40.0;
                part.move_x = 3.0;
                part.layer_offset = -0.002;
                part.x = 2.75;
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.progress = PartProgressSpec::Blend(
                    Box::new(PartProgressSpec::Delay(
                        Box::new(PartProgressSpec::Warmup),
                        0.3,
                    )),
                    Box::new(PartProgressSpec::Reload),
                    0.3,
                );
                part.heat_progress = PartProgressSpec::Min(
                    Box::new(PartProgressSpec::Add(Box::new(PartProgressSpec::Heat), 0.3)),
                    Box::new(PartProgressSpec::Warmup),
                );
                part.heat_color = Some(Rgba::new(1.0, 0.1, 0.1, 1.0));
                part.mirror = true;
                part.under = true;
                part.move_rot = -80.0;
                part.move_x = 3.0;
                part.layer_offset = -0.002;
                part.x = 2.75;
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.progress = PartProgressSpec::Blend(
                    Box::new(PartProgressSpec::Delay(
                        Box::new(PartProgressSpec::Warmup),
                        0.0,
                    )),
                    Box::new(PartProgressSpec::Reload),
                    0.3,
                );
                part.heat_progress = PartProgressSpec::Min(
                    Box::new(PartProgressSpec::Add(Box::new(PartProgressSpec::Heat), 0.3)),
                    Box::new(PartProgressSpec::Warmup),
                );
                part.heat_color = Some(Rgba::new(1.0, 0.1, 0.1, 1.0));
                part.mirror = true;
                part.under = true;
                part.move_rot = -120.0;
                part.move_x = 3.0;
                part.layer_offset = -0.002;
                part.x = 2.75;
                part
            });
            weapon
        });
        unit.parts.push({
            let mut part = DrawPartSpec::region("-glow");
            part.color = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
            part.blending = BlendingKind::Additive;
            part.layer = -1.0;
            part.outline = false;
            part
        });
        unit.immunities.push("burning");
        unit.immunities.push("melting");
        unit.tread_rects = vec![
            TreadRect {
                x: -88.5,
                y: 36.5,
                width: 56.0,
                height: 73.0,
            },
            TreadRect {
                x: -91.5,
                y: -73.5,
                width: 29.0,
                height: 17.0,
            },
            TreadRect {
                x: -56.5,
                y: -106.5,
                width: 39.0,
                height: 19.0,
            },
        ];
        unit
    })
}

fn merui(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("merui", UnitKind::ErekirUnitType, entity::LEGS);
        unit.speed = Some(0.72);
        unit.drag = Some(0.11);
        unit.hit_size = Some(9.0);
        unit.rotate_speed = Some(3.0);
        unit.health = Some(680.0);
        unit.armor = Some(4.0);
        unit.leg_straightness = Some(0.3);
        unit.step_shake = Some(0.0);
        unit.step_sound = Some(SoundId::WALKER_STEP_TINY);
        unit.step_sound_volume = Some(0.4);
        unit.leg_count = Some(6);
        unit.leg_length = Some(8.0);
        unit.lock_leg_base = Some(true);
        unit.leg_continuous_move = Some(true);
        unit.leg_extension = Some(-2.0);
        unit.leg_base_offset = Some(3.0);
        unit.leg_max_length = Some(1.1);
        unit.leg_min_length = Some(0.2);
        unit.leg_length_scl = Some(0.96);
        unit.leg_forward_scl = Some(1.1);
        unit.leg_group_size = Some(3);
        unit.ripple_scale = Some(0.2);
        unit.leg_move_space = Some(1.0);
        unit.allow_leg_step = Some(true);
        unit.hovering = Some(true);
        unit.leg_physics_layer = Some(false);
        unit.shadow_elevation = Some(0.1);
        unit.ground_layer = Some(74.0);
        unit.target_air = Some(false);
        unit.research_cost_multiplier = Some(0.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "merui-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Artillery,
                    speed: Some(3.0),
                    damage: Some(40.0),
                    collides_tiles: Some(true),
                    back_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    hit_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    knockback: Some(0.8),
                    lifetime: Some(46.0),
                    width: Some(9.0),
                    height: Some(9.0),
                    splash_damage_radius: Some(19.0),
                    splash_damage: Some(30.0),
                    trail_length: Some(27),
                    trail_width: Some(2.5),
                    trail_effect: Some(EffectRef::Named(EffectId::NONE)),
                    trail_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    trail_interp: Some(InterpKind::Slope),
                    shrink_x: Some(0.6),
                    shrink_y: Some(0.2),
                    shoot_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        9.0,
                        vec![
                            EffectRef::Named(EffectId::SHOOT_SMALL_COLOR),
                            EffectRef::Inline(Box::new(EffectSpec::plain(9.0))),
                        ],
                    )))),
                    hit_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        14.0,
                        vec![EffectRef::Named(EffectId::HIT_SQUARES_COLOR), {
                            let mut effect = EffectSpec::wave();
                            effect.wave_color =
                                Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                            effect.color_to =
                                Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                            effect.size_to = 21.0;
                            effect.lifetime = 9.0;
                            effect.stroke_from = 2.0;
                            EffectRef::Inline(Box::new(effect))
                        }],
                    )))),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_MERUI);
            weapon.mirror = Some(false);
            weapon.show_stat_sprite = Some(false);
            weapon.x = Some(0.0);
            weapon.y = Some(1.0);
            weapon.shoot_y = Some(4.0);
            weapon.reload = Some(63.0);
            weapon.cooldown_time = Some(42.0);
            weapon.heat_color = Some(Rgba::new(0.67058825, 0.20392157, 0.0, 1.0));
            weapon
        });
        unit
    })
}

fn cleroi(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("cleroi", UnitKind::ErekirUnitType, entity::LEGS);
        unit.speed = Some(0.6);
        unit.drag = Some(0.1);
        unit.hit_size = Some(14.0);
        unit.rotate_speed = Some(3.0);
        unit.health = Some(1100.0);
        unit.armor = Some(5.0);
        unit.step_shake = Some(0.0);
        unit.step_sound = Some(SoundId::WALKER_STEP_SMALL);
        unit.leg_count = Some(4);
        unit.leg_length = Some(14.0);
        unit.lock_leg_base = Some(true);
        unit.leg_continuous_move = Some(true);
        unit.leg_extension = Some(-3.0);
        unit.leg_base_offset = Some(5.0);
        unit.leg_max_length = Some(1.1);
        unit.leg_min_length = Some(0.2);
        unit.leg_length_scl = Some(0.95);
        unit.leg_forward_scl = Some(0.7);
        unit.leg_move_space = Some(1.0);
        unit.hovering = Some(true);
        unit.shadow_elevation = Some(0.2);
        unit.ground_layer = Some(74.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "cleroi-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(3.5),
                    damage: Some(30.0),
                    back_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    trail_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    hit_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    width: Some(7.5),
                    height: Some(10.0),
                    lifetime: Some(40.0),
                    trail_width: Some(2.0),
                    trail_length: Some(4),
                    trail_effect: Some(EffectRef::Named(EffectId::MISSILE_TRAIL)),
                    trail_param: Some(1.8),
                    trail_interval: Some(6.0),
                    splash_damage_radius: Some(30.0),
                    splash_damage: Some(43.0),
                    despawn_sound: Some(SoundId::EXPLOSION_CLEROI),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMOKE_SQUARE)),
                    ammo_multiplier: Some(2.0),
                    hit_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        14.0,
                        vec![EffectRef::Named(EffectId::HIT_BULLET_COLOR), {
                            let mut effect = EffectSpec::wave();
                            effect.wave_color =
                                Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                            effect.color_to =
                                Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                            effect.size_to = 33.0;
                            effect.lifetime = 9.0;
                            effect.stroke_from = 3.0;
                            EffectRef::Inline(Box::new(effect))
                        }],
                    )))),
                    shoot_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        11.0,
                        vec![
                            EffectRef::Named(EffectId::SHOOT_BIG_COLOR),
                            EffectRef::Inline(Box::new(EffectSpec::plain(9.0))),
                        ],
                    )))),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_CLEROI);
            weapon.x = Some(3.5);
            weapon.y = Some(8.25);
            weapon.reload = Some(33.0);
            weapon.layer_offset = Some(-0.002);
            weapon.alternate = Some(false);
            weapon.heat_color = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
            weapon.cooldown_time = Some(25.0);
            weapon.smooth_reload_speed = Some(0.15);
            weapon.recoil = Some(2.0);
            weapon.shake = Some(1.0);
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "cleroi-point-defense",
                kind: WeaponKind::PointDefenseWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    shoot_sound: Some(SoundId::SHOOT_LASER),
                    shoot_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    hit_effect: Some(EffectRef::Named(EffectId::POINT_HIT)),
                    max_range: Some(100.0),
                    damage: Some(38.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.x = Some(4.0);
            weapon.y = Some(-5.0);
            weapon.reload = Some(9.0);
            weapon.target_interval = Some(9.0);
            weapon.target_switch_interval = Some(12.0);
            weapon.recoil = Some(0.5);
            weapon
        });
        unit.parts.push({
            let mut part = DrawPartSpec::region("-spine");
            part.y = 5.25;
            part.move_x = 5.25;
            part.move_rot = 10.0;
            part.progress = PartProgressSpec::Sustain(
                Box::new(PartProgressSpec::Add(
                    Box::new(PartProgressSpec::Mul(
                        Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
                        1.3,
                    )),
                    0.1,
                )),
                0.0,
                0.14,
                0.14,
            );
            part.layer_offset = -0.001;
            part.mirror = true;
            part
        });
        unit.parts.push({
            let mut part = DrawPartSpec::region("-spine");
            part.y = 2.4375;
            part.move_x = 5.875;
            part.move_rot = -4.0;
            part.progress = PartProgressSpec::Sustain(
                Box::new(PartProgressSpec::Add(
                    Box::new(PartProgressSpec::Mul(
                        Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
                        1.3,
                    )),
                    0.1,
                )),
                0.085,
                0.14,
                0.14,
            );
            part.layer_offset = -0.001;
            part.mirror = true;
            part
        });
        unit.parts.push({
            let mut part = DrawPartSpec::region("-spine");
            part.y = -0.375;
            part.move_x = 6.5;
            part.move_rot = -18.0;
            part.progress = PartProgressSpec::Sustain(
                Box::new(PartProgressSpec::Add(
                    Box::new(PartProgressSpec::Mul(
                        Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
                        1.3,
                    )),
                    0.1,
                )),
                0.17,
                0.14,
                0.14,
            );
            part.layer_offset = -0.001;
            part.mirror = true;
            part
        });
        unit.parts.push({
            let mut part = DrawPartSpec::region("-spine");
            part.y = -3.1875;
            part.move_x = 5.875;
            part.move_rot = -32.0;
            part.progress = PartProgressSpec::Sustain(
                Box::new(PartProgressSpec::Add(
                    Box::new(PartProgressSpec::Mul(
                        Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
                        1.3,
                    )),
                    0.1,
                )),
                0.255,
                0.14,
                0.14,
            );
            part.layer_offset = -0.001;
            part.mirror = true;
            part
        });
        unit.parts.push({
            let mut part = DrawPartSpec::region("-spine");
            part.y = -6.0;
            part.move_x = 5.25;
            part.move_rot = -46.0;
            part.progress = PartProgressSpec::Sustain(
                Box::new(PartProgressSpec::Add(
                    Box::new(PartProgressSpec::Mul(
                        Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
                        1.3,
                    )),
                    0.1,
                )),
                0.34,
                0.14,
                0.14,
            );
            part.layer_offset = -0.001;
            part.mirror = true;
            part
        });
        unit
    })
}

fn anthicus(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("anthicus", UnitKind::ErekirUnitType, entity::LEGS);
        unit.speed = Some(0.65);
        unit.drag = Some(0.1);
        unit.hit_size = Some(21.0);
        unit.rotate_speed = Some(3.0);
        unit.health = Some(2700.0);
        unit.armor = Some(7.0);
        unit.fog_radius = Some(40.0);
        unit.step_shake = Some(0.0);
        unit.step_sound = Some(SoundId::WALKER_STEP_SMALL);
        unit.step_sound_pitch = Some(0.78);
        unit.leg_count = Some(6);
        unit.leg_length = Some(18.0);
        unit.leg_group_size = Some(3);
        unit.lock_leg_base = Some(true);
        unit.leg_continuous_move = Some(true);
        unit.leg_extension = Some(-3.0);
        unit.leg_base_offset = Some(7.0);
        unit.leg_max_length = Some(1.1);
        unit.leg_min_length = Some(0.2);
        unit.leg_length_scl = Some(0.95);
        unit.leg_forward_scl = Some(0.9);
        unit.leg_move_space = Some(1.0);
        unit.hovering = Some(true);
        unit.shadow_elevation = Some(0.2);
        unit.ground_layer = Some(74.0);
        unit.always_shoot_when_moving = Some(true);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "anthicus-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(2, 6.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE2)),
                    speed: Some(0.0),
                    keep_velocity: Some(false),
                    inaccuracy: Some(2.0),
                    shoot_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        12.0,
                        vec![
                            EffectRef::Named(EffectId::SHOOT_BIG_COLOR),
                            EffectRef::Inline(Box::new(EffectSpec::plain(9.0))),
                            {
                                let mut effect = EffectSpec::wave();
                                effect.wave_color =
                                    Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                                effect.color_to =
                                    Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                                effect.size_to = 15.0;
                                effect.lifetime = 12.0;
                                effect.stroke_from = 3.0;
                                EffectRef::Inline(Box::new(effect))
                            },
                        ],
                    )))),
                    spawn_unit: Some(Box::new({
                        let mut unit = spec(
                            "anthicus-missile",
                            UnitKind::MissileUnitType,
                            entity::MISSILE,
                        );
                        unit.trail_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                        unit.engine_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                        unit.engine_size = Some(1.75);
                        unit.engine_layer = Some(110.0);
                        unit.speed = Some(3.35);
                        unit.max_range = Some(6.0);
                        unit.lifetime = Some(99.6);
                        unit.outline_color =
                            Some(Rgba::new(0.1764706, 0.18431373, 0.22352941, 1.0));
                        unit.health = Some(55.0);
                        unit.low_altitude = Some(true);
                        unit.weapons.push({
                            let mut weapon = WeaponSpec {
                                name: "",
                                kind: WeaponKind::Weapon,
                                bullet: BulletRef::Inline(Box::new(BulletSpec {
                                    kind: BulletKind::Explosion,
                                    splash_damage: Some(140.0),
                                    splash_damage_radius: Some(25.0),
                                    range_override: Some(16.666666),
                                    shoot_effect: Some(EffectRef::Inline(Box::new(
                                        EffectSpec::multi(
                                            12.0,
                                            vec![
                                                EffectRef::Inline(Box::new(EffectSpec::wrap(
                                                    EffectRef::Named(EffectId::NONE),
                                                    Rgba::new(
                                                        0.54901963, 0.6627451, 0.9098039, 1.0,
                                                    ),
                                                    0.0,
                                                ))),
                                                {
                                                    let mut effect = EffectSpec::wave();
                                                    effect.wave_color = Some(Rgba::new(
                                                        0.54901963, 0.6627451, 0.9098039, 1.0,
                                                    ));
                                                    effect.color_to = Some(Rgba::new(
                                                        0.54901963, 0.6627451, 0.9098039, 1.0,
                                                    ));
                                                    effect.size_to = 40.0;
                                                    effect.lifetime = 12.0;
                                                    effect.stroke_from = 4.0;
                                                    EffectRef::Inline(Box::new(effect))
                                                },
                                            ],
                                        ),
                                    ))),
                                    ..BulletSpec::default()
                                })),
                                ..WeaponSpec::default()
                            };
                            weapon.shoot_sound = Some(SoundId::NONE);
                            weapon.shoot_cone = Some(360.0);
                            weapon.mirror = Some(false);
                            weapon.reload = Some(1.0);
                            weapon.shoot_on_death = Some(true);
                            weapon.shoot_on_death_effect =
                                Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION));
                            weapon
                        });
                        unit.parts.push({
                            let mut part = DrawPartSpec::flare();
                            part.progress = PartProgressSpec::CurveInterp(
                                Box::new(PartProgressSpec::Slope(Box::new(PartProgressSpec::Life))),
                                InterpKind::Pow2In,
                            );
                            part.radius = 0.0;
                            part.radius_to = 35.0;
                            part.stroke = 3.0;
                            part.rotation = 45.0;
                            part.y = -5.0;
                            part.follow_rotation = true;
                            part
                        });
                        unit
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE_LARGE);
            weapon.shoot_sound_volume = Some(0.5);
            weapon.x = Some(7.25);
            weapon.y = Some(-2.75);
            weapon.shoot_y = Some(1.5);
            weapon.show_stat_sprite = Some(false);
            weapon.reload = Some(130.0);
            weapon.layer_offset = Some(0.01);
            weapon.heat_color = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
            weapon.cooldown_time = Some(60.0);
            weapon.smooth_reload_speed = Some(0.15);
            weapon.shoot_warmup_speed = Some(0.05);
            weapon.min_warmup = Some(0.9);
            weapon.rotation_limit = Some(70.0);
            weapon.rotate_speed = Some(2.0);
            weapon.inaccuracy = Some(20.0);
            weapon.shoot_status = Some("slow");
            weapon.rotate = Some(true);
            weapon.shake = Some(2.0);
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.moves.push(PartMoveSpec {
                    progress: PartProgressSpec::Reload,
                    x: 1.0,
                    y: 0.0,
                    gx: 0.0,
                    gy: 0.0,
                    rot: 0.0,
                });
                part.mirror = true;
                part.move_rot = -25.0;
                part.under = true;
                part.heat_color = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.moves.push(PartMoveSpec {
                    progress: PartProgressSpec::Shorten(Box::new(PartProgressSpec::Reload), 0.5),
                    x: 1.0,
                    y: 0.0,
                    gx: 0.0,
                    gy: 0.0,
                    rot: -15.0,
                });
                part.mirror = true;
                part.move_rot = -50.0;
                part.move_y = -2.0;
                part.under = true;
                part.heat_color = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
                part
            });
            weapon
        });
        unit.parts.push({
            let mut part = DrawPartSpec::region("-blade");
            part.layer_offset = -0.01;
            part.heat_layer_offset = 0.005;
            part.x = 2.0;
            part.move_x = 6.0;
            part.move_y = 8.0;
            part.move_rot = 40.0;
            part.mirror = true;
            part.progress = PartProgressSpec::Delay(Box::new(PartProgressSpec::Warmup), 0.0);
            part.heat_progress = PartProgressSpec::AbsinTime {
                offset: 0.0,
                scl: 7.0,
                mag: 1.0,
            };
            part.heat_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
            part
        });
        unit.parts.push({
            let mut part = DrawPartSpec::region("-blade");
            part.layer_offset = -0.01;
            part.heat_layer_offset = 0.005;
            part.x = 2.0;
            part.move_x = 7.9;
            part.move_y = 4.0;
            part.move_rot = 15.0;
            part.mirror = true;
            part.progress = PartProgressSpec::Delay(Box::new(PartProgressSpec::Warmup), 0.2);
            part.heat_progress = PartProgressSpec::AbsinTime {
                offset: 14.0,
                scl: 7.0,
                mag: 1.0,
            };
            part.heat_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
            part
        });
        unit.parts.push({
            let mut part = DrawPartSpec::region("-blade");
            part.layer_offset = -0.01;
            part.heat_layer_offset = 0.005;
            part.x = 2.0;
            part.move_x = 9.8;
            part.move_y = 0.0;
            part.move_rot = -10.0;
            part.mirror = true;
            part.progress = PartProgressSpec::Delay(Box::new(PartProgressSpec::Warmup), 0.4);
            part.heat_progress = PartProgressSpec::AbsinTime {
                offset: 28.0,
                scl: 7.0,
                mag: 1.0,
            };
            part.heat_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
            part
        });
        unit
    })
}

fn tecta(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("tecta", UnitKind::ErekirUnitType, entity::LEGS);
        unit.drag = Some(0.1);
        unit.speed = Some(0.6);
        unit.hit_size = Some(30.0);
        unit.health = Some(6500.0);
        unit.armor = Some(5.0);
        unit.lock_leg_base = Some(true);
        unit.leg_continuous_move = Some(true);
        unit.leg_group_size = Some(3);
        unit.leg_straightness = Some(0.4);
        unit.base_leg_straightness = Some(0.5);
        unit.leg_max_length = Some(1.3);
        unit.research_cost_multiplier = Some(0.0);
        unit.step_sound = Some(SoundId::WALKER_STEP);
        unit.step_sound_volume = Some(1.0);
        unit.step_sound_pitch = Some(1.0);
        unit.rotate_speed = Some(2.1);
        unit.leg_count = Some(6);
        unit.leg_length = Some(15.0);
        unit.leg_forward_scl = Some(0.45);
        unit.leg_move_space = Some(1.4);
        unit.ripple_scale = Some(2.0);
        unit.step_shake = Some(0.5);
        unit.leg_extension = Some(-5.0);
        unit.leg_base_offset = Some(5.0);
        unit.leg_splash_damage = Some(32.0);
        unit.leg_splash_range = Some(30.0);
        unit.drown_time_multiplier = Some(0.5);
        unit.hovering = Some(true);
        unit.shadow_elevation = Some(0.4);
        unit.ground_layer = Some(75.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "tecta-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(3, 0.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Missile,
                    speed: Some(4.2),
                    damage: Some(51.0),
                    homing_power: Some(0.2),
                    weave_mag: Some(4.0),
                    weave_scale: Some(4.0),
                    lifetime: Some(55.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG2)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMOKE_TITAN)),
                    splash_damage: Some(60.0),
                    splash_damage_radius: Some(30.0),
                    front_color: Some(Rgba::WHITE),
                    hit_sound: Some(SoundId::NONE),
                    width: Some(10.0),
                    height: Some(10.0),
                    light_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    trail_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    back_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    light_radius: Some(40.0),
                    light_opacity: Some(0.7),
                    trail_width: Some(2.8),
                    trail_length: Some(20),
                    trail_chance: Some(-1.0),
                    despawn_sound: Some(SoundId::EXPLOSION_DULL),
                    despawn_effect: Some(EffectRef::Named(EffectId::NONE)),
                    hit_effect: Some({
                        let mut effect = EffectSpec::explosion();
                        effect.lifetime = 20.0;
                        effect.wave_stroke = 2.0;
                        effect.wave_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                        effect.spark_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                        effect.wave_rad = 12.0;
                        effect.smoke_size = 0.0;
                        effect.smoke_size_base = 0.0;
                        effect.sparks = 10;
                        effect.spark_rad = 35.0;
                        effect.spark_len = 4.0;
                        effect.spark_stroke = 1.5;
                        EffectRef::Inline(Box::new(effect))
                    }),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_MALIGN);
            weapon.mirror = Some(true);
            weapon.top = Some(false);
            weapon.x = Some(15.5);
            weapon.y = Some(1.0);
            weapon.shoot_y = Some(11.75);
            weapon.recoil = Some(3.0);
            weapon.reload = Some(40.0);
            weapon.shake = Some(3.0);
            weapon.cooldown_time = Some(40.0);
            weapon.inaccuracy = Some(3.0);
            weapon.velocity_rnd = Some(0.33);
            weapon.heat_color = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
            weapon
        });
        unit.abilities.push({
            let mut ability = AbilitySpec::shield_arc();
            ability.region = String::from("tecta-shield");
            ability.range = 45.0;
            ability.angle = 82.0;
            ability.regen = 0.75;
            ability.cooldown = 480.0;
            ability.max = 2500.0;
            ability.y = -20.0;
            ability.width = 8.0;
            ability.when_shooting = false;
            ability.chance_deflect = 1.0;
            ability
        });
        unit
    })
}

fn collaris(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("collaris", UnitKind::ErekirUnitType, entity::LEGS);
        unit.drag = Some(0.1);
        unit.speed = Some(1.1);
        unit.hit_size = Some(44.0);
        unit.health = Some(18000.0);
        unit.armor = Some(9.0);
        unit.rotate_speed = Some(1.6);
        unit.lock_leg_base = Some(true);
        unit.leg_continuous_move = Some(true);
        unit.leg_straightness = Some(0.6);
        unit.base_leg_straightness = Some(0.5);
        unit.step_sound = Some(SoundId::WALKER_STEP);
        unit.step_sound_volume = Some(1.1);
        unit.step_sound_pitch = Some(0.9);
        unit.leg_count = Some(8);
        unit.leg_length = Some(30.0);
        unit.leg_forward_scl = Some(2.1);
        unit.leg_move_space = Some(1.05);
        unit.ripple_scale = Some(1.2);
        unit.step_shake = Some(0.5);
        unit.leg_group_size = Some(2);
        unit.leg_extension = Some(-6.0);
        unit.leg_base_offset = Some(19.0);
        unit.leg_straight_length = Some(0.9);
        unit.leg_max_length = Some(1.2);
        unit.leg_splash_damage = Some(32.0);
        unit.leg_splash_range = Some(32.0);
        unit.drown_time_multiplier = Some(0.5);
        unit.hovering = Some(true);
        unit.shadow_elevation = Some(0.4);
        unit.ground_layer = Some(75.0);
        unit.target_air = Some(false);
        unit.always_shoot_when_moving = Some(true);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "collaris-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(1, 0.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Artillery,
                    speed: Some(5.5),
                    damage: Some(260.0),
                    collides_tiles: Some(true),
                    collides: Some(true),
                    lifetime: Some(60.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_COLOR)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMOKE_SQUARE_BIG)),
                    front_color: Some(Rgba::WHITE),
                    hit_sound: Some(SoundId::NONE),
                    width: Some(18.0),
                    height: Some(24.0),
                    range_override: Some(385.0),
                    light_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    trail_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    hit_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    back_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                    light_radius: Some(40.0),
                    light_opacity: Some(0.7),
                    trail_width: Some(4.5),
                    trail_length: Some(19),
                    trail_chance: Some(-1.0),
                    despawn_effect: Some(EffectRef::Named(EffectId::NONE)),
                    despawn_sound: Some(SoundId::EXPLOSION_DULL),
                    splash_damage: Some(120.0),
                    splash_damage_radius: Some(36.0),
                    frag_bullets: Some(15),
                    frag_velocity_min: Some(0.5),
                    frag_random_spread: Some(130.0),
                    frag_life_min: Some(0.3),
                    despawn_shake: Some(5.0),
                    trail_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        50.0,
                        vec![
                            EffectRef::Named(EffectId::ARTILLERY_TRAIL),
                            EffectRef::Named(EffectId::ARTILLERY_TRAIL_SMOKE),
                        ],
                    )))),
                    hit_effect: Some({
                        let mut effect = EffectSpec::explosion();
                        effect.lifetime = 50.0;
                        effect.wave_stroke = 5.0;
                        effect.wave_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                        effect.spark_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                        effect.wave_rad = 45.0;
                        effect.smoke_size = 0.0;
                        effect.smoke_size_base = 0.0;
                        effect.sparks = 10;
                        effect.spark_rad = 25.0;
                        effect.spark_len = 8.0;
                        effect.spark_stroke = 3.0;
                        EffectRef::Inline(Box::new(effect))
                    }),
                    frag_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Basic,
                        speed: Some(5.5),
                        damage: Some(37.0),
                        pierce_cap: Some(2),
                        pierce_building: Some(true),
                        homing_power: Some(0.09),
                        homing_range: Some(150.0),
                        lifetime: Some(40.0),
                        shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_COLOR)),
                        smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMOKE_SQUARE_BIG)),
                        front_color: Some(Rgba::WHITE),
                        hit_sound: Some(SoundId::NONE),
                        width: Some(12.0),
                        height: Some(20.0),
                        light_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                        trail_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                        hit_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                        back_color: Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0)),
                        light_radius: Some(40.0),
                        light_opacity: Some(0.7),
                        trail_width: Some(2.2),
                        trail_length: Some(7),
                        trail_chance: Some(-1.0),
                        collides_air: Some(false),
                        despawn_effect: Some(EffectRef::Named(EffectId::NONE)),
                        splash_damage: Some(35.0),
                        splash_damage_radius: Some(30.0),
                        hit_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                            30.0,
                            vec![
                                {
                                    let mut effect = EffectSpec::explosion();
                                    effect.lifetime = 30.0;
                                    effect.wave_stroke = 2.0;
                                    effect.wave_color =
                                        Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                                    effect.spark_color =
                                        Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                                    effect.wave_rad = 5.0;
                                    effect.smoke_size = 0.0;
                                    effect.smoke_size_base = 0.0;
                                    effect.sparks = 5;
                                    effect.spark_rad = 20.0;
                                    effect.spark_len = 6.0;
                                    effect.spark_stroke = 2.0;
                                    EffectRef::Inline(Box::new(effect))
                                },
                                EffectRef::Named(EffectId::BLAST_EXPLOSION),
                            ],
                        )))),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_COLLARIS);
            weapon.mirror = Some(true);
            weapon.rotation_limit = Some(30.0);
            weapon.rotate_speed = Some(0.4);
            weapon.rotate = Some(true);
            weapon.x = Some(12.0);
            weapon.y = Some(-7.0);
            weapon.shoot_y = Some(16.0);
            weapon.recoil = Some(4.0);
            weapon.reload = Some(130.0);
            weapon.cooldown_time = Some(156.0);
            weapon.shake = Some(7.0);
            weapon.layer_offset = Some(0.02);
            weapon.shadow = Some(10.0);
            weapon.shoot_status = Some("slow");
            weapon.shoot_status_duration = Some(300.0);
            weapon.heat_color = Some(Rgba::new(1.0, 0.0, 0.0, 1.0));
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.moves.push(PartMoveSpec {
                    progress: PartProgressSpec::CurveRange(
                        Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Mul(
                            Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
                            1.8,
                        )))),
                        0.0,
                        0.2,
                    ),
                    x: 0.0,
                    y: 0.0,
                    gx: 0.0,
                    gy: 0.0,
                    rot: 36.0,
                });
                part.under = true;
                part.layer_offset = -0.001;
                part.heat_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                part.heat_progress = PartProgressSpec::Min(
                    Box::new(PartProgressSpec::Add(Box::new(PartProgressSpec::Heat), 0.2)),
                    Box::new(PartProgressSpec::Warmup),
                );
                part.progress = PartProgressSpec::Blend(
                    Box::new(PartProgressSpec::Warmup),
                    Box::new(PartProgressSpec::Reload),
                    0.1,
                );
                part.x = 3.375;
                part.y = 2.5;
                part.move_y = 1.0;
                part.move_x = 0.0;
                part.move_rot = -45.0;
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.moves.push(PartMoveSpec {
                    progress: PartProgressSpec::CurveRange(
                        Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Mul(
                            Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
                            1.8,
                        )))),
                        0.2,
                        0.2,
                    ),
                    x: 0.0,
                    y: 0.0,
                    gx: 0.0,
                    gy: 0.0,
                    rot: 36.0,
                });
                part.under = true;
                part.layer_offset = -0.001;
                part.heat_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                part.heat_progress = PartProgressSpec::Min(
                    Box::new(PartProgressSpec::Add(Box::new(PartProgressSpec::Heat), 0.2)),
                    Box::new(PartProgressSpec::Warmup),
                );
                part.progress = PartProgressSpec::Blend(
                    Box::new(PartProgressSpec::Warmup),
                    Box::new(PartProgressSpec::Reload),
                    0.1,
                );
                part.x = 3.375;
                part.y = 0.5;
                part.move_y = 0.0;
                part.move_x = 0.3;
                part.move_rot = -62.0;
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.moves.push(PartMoveSpec {
                    progress: PartProgressSpec::CurveRange(
                        Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Mul(
                            Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
                            1.8,
                        )))),
                        0.4,
                        0.2,
                    ),
                    x: 0.0,
                    y: 0.0,
                    gx: 0.0,
                    gy: 0.0,
                    rot: 36.0,
                });
                part.under = true;
                part.layer_offset = -0.001;
                part.heat_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                part.heat_progress = PartProgressSpec::Min(
                    Box::new(PartProgressSpec::Add(Box::new(PartProgressSpec::Heat), 0.2)),
                    Box::new(PartProgressSpec::Warmup),
                );
                part.progress = PartProgressSpec::Blend(
                    Box::new(PartProgressSpec::Warmup),
                    Box::new(PartProgressSpec::Reload),
                    0.1,
                );
                part.x = 3.375;
                part.y = -1.5;
                part.move_y = -1.0;
                part.move_x = 0.6;
                part.move_rot = -79.0;
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.moves.push(PartMoveSpec {
                    progress: PartProgressSpec::CurveRange(
                        Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Mul(
                            Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
                            1.8,
                        )))),
                        0.6,
                        0.2,
                    ),
                    x: 0.0,
                    y: 0.0,
                    gx: 0.0,
                    gy: 0.0,
                    rot: 36.0,
                });
                part.under = true;
                part.layer_offset = -0.001;
                part.heat_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                part.heat_progress = PartProgressSpec::Min(
                    Box::new(PartProgressSpec::Add(Box::new(PartProgressSpec::Heat), 0.2)),
                    Box::new(PartProgressSpec::Warmup),
                );
                part.progress = PartProgressSpec::Blend(
                    Box::new(PartProgressSpec::Warmup),
                    Box::new(PartProgressSpec::Reload),
                    0.1,
                );
                part.x = 3.375;
                part.y = -3.5;
                part.move_y = -2.0;
                part.move_x = 0.90000004;
                part.move_rot = -96.0;
                part
            });
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.moves.push(PartMoveSpec {
                    progress: PartProgressSpec::CurveRange(
                        Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Mul(
                            Box::new(PartProgressSpec::Inv(Box::new(PartProgressSpec::Reload))),
                            1.8,
                        )))),
                        0.8,
                        0.2,
                    ),
                    x: 0.0,
                    y: 0.0,
                    gx: 0.0,
                    gy: 0.0,
                    rot: 36.0,
                });
                part.under = true;
                part.layer_offset = -0.001;
                part.heat_color = Some(Rgba::new(0.54901963, 0.6627451, 0.9098039, 1.0));
                part.heat_progress = PartProgressSpec::Min(
                    Box::new(PartProgressSpec::Add(Box::new(PartProgressSpec::Heat), 0.2)),
                    Box::new(PartProgressSpec::Warmup),
                );
                part.progress = PartProgressSpec::Blend(
                    Box::new(PartProgressSpec::Warmup),
                    Box::new(PartProgressSpec::Reload),
                    0.1,
                );
                part.x = 3.375;
                part.y = -5.5;
                part.move_y = -3.0;
                part.move_x = 1.2;
                part.move_rot = -113.0;
                part
            });
            weapon
        });
        unit
    })
}

fn elude(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("elude", UnitKind::ErekirUnitType, entity::HOVER);
        unit.hovering = Some(true);
        unit.can_drown = Some(false);
        unit.shadow_elevation = Some(0.1);
        unit.drag = Some(0.07);
        unit.speed = Some(1.8);
        unit.rotate_speed = Some(5.0);
        unit.accel = Some(0.09);
        unit.health = Some(600.0);
        unit.armor = Some(1.0);
        unit.hit_size = Some(11.0);
        unit.engine_offset = Some(7.0);
        unit.engine_size = Some(2.0);
        unit.item_capacity = Some(0);
        unit.use_engine_elevation = Some(false);
        unit.research_cost_multiplier = Some(0.0);
        unit.move_sound = Some(SoundId::LOOP_EXTRACT);
        unit.move_sound_volume = Some(0.25);
        unit.move_sound_pitch_min = Some(0.7);
        unit.move_sound_pitch_max = Some(1.5);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "elude-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::spread(2, 11.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(5.0),
                    damage: Some(16.0),
                    homing_power: Some(0.19),
                    homing_delay: Some(4.0),
                    width: Some(7.0),
                    height: Some(12.0),
                    lifetime: Some(30.0),
                    shoot_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE)),
                    hit_color: Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0)),
                    back_color: Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0)),
                    trail_color: Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    trail_width: Some(1.5),
                    trail_length: Some(5),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    despawn_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_ELUDE);
            weapon.y = Some(-2.0);
            weapon.x = Some(4.0);
            weapon.top = Some(true);
            weapon.mirror = Some(true);
            weapon.reload = Some(40.0);
            weapon.base_rotation = Some(-35.0);
            weapon.shoot_cone = Some(360.0);
            weapon
        });
        unit.abilities.push({
            let mut ability = AbilitySpec::move_effect(
                0.0,
                -7.0,
                Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0),
                EffectId::MISSILE_TRAIL_SHORT,
                4.0,
            );
            ability.team_color = true;
            ability
        });
        unit.parts.push({
            let mut part = DrawPartSpec::hover();
            part.x = 3.9;
            part.y = -3.0;
            part.mirror = true;
            part.radius = 6.0;
            part.phase = 90.0;
            part.stroke = 2.0;
            part.layer_offset = -0.001;
            part.color = Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0));
            part
        });
        unit.parts.push({
            let mut part = DrawPartSpec::hover();
            part.x = 3.9;
            part.y = 3.0;
            part.mirror = true;
            part.radius = 6.0;
            part.phase = 90.0;
            part.stroke = 2.0;
            part.layer_offset = -0.001;
            part.color = Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0));
            part
        });
        unit
    })
}

fn avert(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("avert", UnitKind::ErekirUnitType, entity::AIR);
        unit.low_altitude = Some(false);
        unit.flying = Some(true);
        unit.drag = Some(0.08);
        unit.speed = Some(2.0);
        unit.rotate_speed = Some(8.0);
        unit.accel = Some(0.09);
        unit.health = Some(1100.0);
        unit.armor = Some(3.0);
        unit.hit_size = Some(12.0);
        unit.engine_size = Some(0.0);
        unit.fog_radius = Some(25.0);
        unit.item_capacity = Some(0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "avert-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::helix(2.0, 1.5)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(5.0),
                    damage: Some(29.333334),
                    width: Some(7.0),
                    height: Some(12.0),
                    lifetime: Some(18.0),
                    building_damage_multiplier: Some(0.599999),
                    block_armor_multiplier: Some(0.5),
                    shoot_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE)),
                    hit_color: Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0)),
                    back_color: Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0)),
                    trail_color: Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0)),
                    front_color: Some(Rgba::WHITE),
                    trail_width: Some(1.5),
                    trail_length: Some(5),
                    frag_on_despawn: Some(false),
                    frag_bullets: Some(2),
                    hit_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        14.0,
                        vec![
                            EffectRef::Named(EffectId::HIT_SQUARES_COLOR),
                            EffectRef::Named(EffectId::SQUARE_WAVE_EFFECT),
                        ],
                    )))),
                    frag_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Basic,
                        speed: Some(3.0),
                        damage: Some(10.0),
                        width: Some(5.0),
                        height: Some(8.0),
                        lifetime: Some(14.0),
                        frag_velocity_max: Some(1.0),
                        frag_velocity_min: Some(0.7),
                        hit_color: Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0)),
                        back_color: Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0)),
                        trail_color: Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0)),
                        front_color: Some(Rgba::WHITE),
                        trail_width: Some(1.2),
                        trail_length: Some(4),
                        hit_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                        despawn_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_AVERT);
            weapon.reload = Some(35.0);
            weapon.x = Some(0.0);
            weapon.y = Some(6.5);
            weapon.shoot_y = Some(5.0);
            weapon.recoil = Some(1.0);
            weapon.top = Some(false);
            weapon.layer_offset = Some(-0.01);
            weapon.rotate = Some(false);
            weapon.mirror = Some(false);
            weapon
        });
        unit.engines_mirror
            .push(EngineSpec::new(8.75, -9.5, 3.0, 315.0));
        unit.engines_mirror
            .push(EngineSpec::new(9.75, -4.0, 3.0, 315.0));
        unit
    })
}

fn obviate(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("obviate", UnitKind::ErekirUnitType, entity::AIR);
        unit.flying = Some(true);
        unit.drag = Some(0.08);
        unit.speed = Some(1.8);
        unit.rotate_speed = Some(2.5);
        unit.accel = Some(0.09);
        unit.health = Some(2300.0);
        unit.armor = Some(6.0);
        unit.hit_size = Some(25.0);
        unit.engine_size = Some(4.3);
        unit.engine_offset = Some(13.5);
        unit.fog_radius = Some(25.0);
        unit.item_capacity = Some(0);
        unit.low_altitude = Some(true);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::helix(5.0, 1.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMOKE_TITAN)),
                    hit_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    despawn_sound: Some(SoundId::EXPLOSION_ARTILLERY_SHOCK),
                    sprite: Some(Some(String::from("large-orb"))),
                    trail_effect: Some(EffectRef::Named(EffectId::MISSILE_TRAIL)),
                    trail_interval: Some(3.0),
                    trail_param: Some(4.0),
                    speed: Some(3.0),
                    damage: Some(75.0),
                    lifetime: Some(60.0),
                    width: Some(15.0),
                    height: Some(15.0),
                    back_color: Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0)),
                    front_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    shrink_x: Some(0.0),
                    shrink_y: Some(0.0),
                    trail_color: Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0)),
                    trail_length: Some(12),
                    trail_width: Some(2.2),
                    bullet_interval: Some(4.0),
                    lightning_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                    lightning_damage: Some(17.0),
                    lightning: Some(8),
                    lightning_length: Some(2),
                    lightning_length_rand: Some(8),
                    shoot_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        14.0,
                        vec![EffectRef::Named(EffectId::SHOOT_TITAN), {
                            let mut effect = EffectSpec::wave();
                            effect.color_to =
                                Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0));
                            effect.size_to = 26.0;
                            effect.lifetime = 14.0;
                            effect.stroke_from = 4.0;
                            EffectRef::Inline(Box::new(effect))
                        }],
                    )))),
                    despawn_effect: Some({
                        let mut effect = EffectSpec::explosion();
                        effect.wave_color = Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0));
                        effect.smoke_color = Some(Rgba::new(0.5, 0.5, 0.5, 1.0));
                        effect.spark_color = Some(Rgba::new(0.4, 0.36078432, 0.62352943, 1.0));
                        effect.wave_stroke = 4.0;
                        effect.wave_rad = 40.0;
                        EffectRef::Inline(Box::new(effect))
                    }),
                    interval_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Lightning,
                        damage: Some(16.0),
                        collides_air: Some(false),
                        ammo_multiplier: Some(1.0),
                        lightning_color: Some(Rgba::new(0.7490196, 0.57254905, 0.9764706, 1.0)),
                        lightning_length: Some(3),
                        lightning_length_rand: Some(6),
                        building_damage_multiplier: Some(0.25),
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
                            light_color: Some(Rgba::WHITE),
                            building_damage_multiplier: Some(0.25),
                            ..BulletSpec::default()
                        })),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::EXPLOSION_OBVIATE);
            weapon.x = Some(0.0);
            weapon.y = Some(-2.0);
            weapon.shoot_y = Some(0.0);
            weapon.reload = Some(140.0);
            weapon.mirror = Some(false);
            weapon.min_warmup = Some(0.95);
            weapon.shake = Some(3.0);
            weapon.cooldown_time = Some(130.0);
            weapon
        });
        unit.parts.push({
            let mut part = DrawPartSpec::region("-blade");
            part.moves.push(PartMoveSpec {
                progress: PartProgressSpec::Reload,
                x: 2.0,
                y: 1.0,
                gx: 0.0,
                gy: 0.0,
                rot: -5.0,
            });
            part.move_rot = -10.0;
            part.move_x = -1.0;
            part.progress = PartProgressSpec::Warmup;
            part.mirror = true;
            part
        });
        unit.engines_mirror
            .push(EngineSpec::new(9.5, -11.5, 3.1, 315.0));
        unit
    })
}

fn quell(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("quell", UnitKind::ErekirUnitType, entity::AIR_PAYLOAD);
        unit.ai_controller = Some(AiControllerKind::FlyingFollow);
        unit.env_disabled = Some(EnvMask::none());
        unit.low_altitude = Some(false);
        unit.flying = Some(true);
        unit.drag = Some(0.06);
        unit.speed = Some(1.1);
        unit.rotate_speed = Some(3.2);
        unit.accel = Some(0.1);
        unit.health = Some(6000.0);
        unit.armor = Some(4.0);
        unit.hit_size = Some(36.0);
        unit.payload_capacity = Some(1024.0);
        unit.research_cost_multiplier = Some(0.0);
        unit.target_air = Some(false);
        unit.engine_size = Some(4.8);
        unit.engine_offset = Some(15.25);
        unit.range = Some(361.19998);
        unit.loop_sound_volume = Some(0.85);
        unit.loop_sound = Some(SoundId::LOOP_HOVER);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "quell-weapon",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Basic,
                    speed: Some(4.3),
                    damage: Some(70.0),
                    sprite: Some(Some(String::from("missile-large"))),
                    shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE2)),
                    lifetime: Some(29.76),
                    range_override: Some(361.2),
                    follow_aim_speed: Some(5.0),
                    width: Some(12.0),
                    height: Some(22.0),
                    hit_size: Some(7.0),
                    hit_color: Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0)),
                    back_color: Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0)),
                    trail_color: Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0)),
                    trail_width: Some(3.0),
                    trail_length: Some(12),
                    hit_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    despawn_effect: Some(EffectRef::Named(EffectId::HIT_BULLET_COLOR)),
                    keep_velocity: Some(false),
                    collides_ground: Some(true),
                    collides_air: Some(false),
                    frag_random_spread: Some(0.0),
                    frag_bullets: Some(1),
                    frag_velocity_min: Some(1.0),
                    frag_offset_max: Some(1.0),
                    frag_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Plain,
                        speed: Some(0.0),
                        keep_velocity: Some(false),
                        collides_air: Some(false),
                        spawn_unit: Some(Box::new({
                            let mut unit =
                                spec("quell-missile", UnitKind::MissileUnitType, entity::MISSILE);
                            unit.target_air = Some(false);
                            unit.speed = Some(4.3);
                            unit.max_range = Some(6.0);
                            unit.lifetime = Some(54.239998);
                            unit.outline_color =
                                Some(Rgba::new(0.1764706, 0.18431373, 0.22352941, 1.0));
                            unit.engine_color =
                                Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0));
                            unit.trail_color =
                                Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0));
                            unit.engine_layer = Some(110.0);
                            unit.health = Some(45.0);
                            unit.loop_sound_volume = Some(0.1);
                            unit.weapons.push({
                                let mut weapon = WeaponSpec {
                                    name: "",
                                    kind: WeaponKind::Weapon,
                                    bullet: BulletRef::Inline(Box::new(BulletSpec {
                                        kind: BulletKind::Explosion,
                                        splash_damage: Some(110.0),
                                        splash_damage_radius: Some(25.0),
                                        range_override: Some(16.666666),
                                        collides_air: Some(false),
                                        shoot_effect: Some(EffectRef::Inline(Box::new(
                                            EffectSpec::wrap(
                                                EffectRef::Named(EffectId::NONE),
                                                Rgba::new(0.64, 0.5772549, 0.99764705, 1.0),
                                                0.0,
                                            ),
                                        ))),
                                        ..BulletSpec::default()
                                    })),
                                    ..WeaponSpec::default()
                                };
                                weapon.shoot_sound = Some(SoundId::NONE);
                                weapon.shoot_cone = Some(360.0);
                                weapon.mirror = Some(false);
                                weapon.reload = Some(1.0);
                                weapon.shoot_on_death = Some(true);
                                weapon.shoot_on_death_effect =
                                    Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION));
                                weapon
                            });
                            unit
                        })),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE_SMALL);
            weapon.x = Some(12.75);
            weapon.y = Some(1.25);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(2.0);
            weapon.reload = Some(55.0);
            weapon.layer_offset = Some(-0.001);
            weapon.recoil = Some(1.0);
            weapon.rotation_limit = Some(60.0);
            weapon.shake = Some(1.0);
            weapon
        });
        unit.abilities.push({
            let mut ability = AbilitySpec::suppression_field();
            ability.reload = 480.0;
            ability.orb_radius = 5.3;
            ability.y = 1.0;
            ability
        });
        unit.engines_mirror
            .push(EngineSpec::new(15.5, -15.0, 3.9, 315.0));
        unit.engines_mirror
            .push(EngineSpec::new(18.0, -7.25, 3.0, 315.0));
        unit
    })
}

fn disrupt(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("disrupt", UnitKind::ErekirUnitType, entity::AIR_PAYLOAD);
        unit.ai_controller = Some(AiControllerKind::FlyingFollow);
        unit.env_disabled = Some(EnvMask::none());
        unit.low_altitude = Some(false);
        unit.flying = Some(true);
        unit.drag = Some(0.07);
        unit.speed = Some(1.0);
        unit.rotate_speed = Some(2.0);
        unit.accel = Some(0.1);
        unit.health = Some(12000.0);
        unit.armor = Some(9.0);
        unit.hit_size = Some(46.0);
        unit.payload_capacity = Some(2304.0);
        unit.target_air = Some(false);
        unit.engine_size = Some(6.0);
        unit.engine_offset = Some(25.25);
        unit.loop_sound = Some(SoundId::LOOP_HOVER);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "disrupt-weapon",
                kind: WeaponKind::Weapon,
                shoot: Some(ShootPatternSpec::plain(3, 5.0, 0.0)),
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    shoot_effect: Some(EffectRef::Named(EffectId::SPARK_SHOOT)),
                    smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMOKE_TITAN)),
                    hit_color: Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0)),
                    speed: Some(0.0),
                    keep_velocity: Some(false),
                    collides_air: Some(false),
                    spawn_unit: Some(Box::new({
                        let mut unit = spec(
                            "disrupt-missile",
                            UnitKind::MissileUnitType,
                            entity::MISSILE,
                        );
                        unit.target_air = Some(false);
                        unit.speed = Some(4.6);
                        unit.max_range = Some(5.0);
                        unit.outline_color =
                            Some(Rgba::new(0.1764706, 0.18431373, 0.22352941, 1.0));
                        unit.health = Some(70.0);
                        unit.homing_delay = Some(10.0);
                        unit.low_altitude = Some(true);
                        unit.engine_size = Some(3.0);
                        unit.engine_color = Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0));
                        unit.trail_color = Some(Rgba::new(0.42745098, 0.3372549, 0.7490196, 1.0));
                        unit.engine_layer = Some(110.0);
                        unit.death_explosion_effect = Some(EffectRef::Named(EffectId::NONE));
                        unit.loop_sound_volume = Some(0.1);
                        unit.weapons.push({
                            let mut weapon = WeaponSpec {
                                name: "",
                                kind: WeaponKind::Weapon,
                                bullet: BulletRef::Inline(Box::new(BulletSpec {
                                    kind: BulletKind::Explosion,
                                    splash_damage: Some(140.0),
                                    splash_damage_radius: Some(25.0),
                                    range_override: Some(16.666666),
                                    collides_air: Some(false),
                                    suppression_range: Some(140.0),
                                    shoot_effect: Some({
                                        let mut effect = EffectSpec::explosion();
                                        effect.lifetime = 50.0;
                                        effect.wave_stroke = 5.0;
                                        effect.wave_life = 12.0;
                                        effect.wave_color =
                                            Some(Rgba::new(0.71999997, 0.64941174, 1.0, 1.0));
                                        effect.spark_color =
                                            Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0));
                                        effect.smoke_color =
                                            Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0));
                                        effect.wave_rad = 40.0;
                                        effect.smoke_size = 4.0;
                                        effect.smokes = 7;
                                        effect.smoke_size_base = 0.0;
                                        effect.sparks = 10;
                                        effect.spark_rad = 40.0;
                                        effect.spark_len = 6.0;
                                        effect.spark_stroke = 2.0;
                                        EffectRef::Inline(Box::new(effect))
                                    }),
                                    ..BulletSpec::default()
                                })),
                                ..WeaponSpec::default()
                            };
                            weapon.shoot_cone = Some(360.0);
                            weapon.mirror = Some(false);
                            weapon.reload = Some(1.0);
                            weapon.shoot_on_death = Some(true);
                            weapon.shoot_on_death_effect =
                                Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION));
                            weapon
                        });
                        unit.parts.push({
                            let mut part = DrawPartSpec::shape();
                            part.layer = 110.0;
                            part.circle = true;
                            part.y = -0.25;
                            part.radius = 1.5;
                            part.color = Some(Rgba::new(0.64, 0.5772549, 0.99764705, 1.0));
                            part.color_to = Some(Rgba::WHITE);
                            part.progress = PartProgressSpec::CurveInterp(
                                Box::new(PartProgressSpec::Life),
                                InterpKind::Pow5In,
                            );
                            part
                        });
                        unit.parts.push({
                            let mut part = DrawPartSpec::region("-fin");
                            part.mirror = true;
                            part.progress = PartProgressSpec::CurveInterp(
                                Box::new(PartProgressSpec::Mul(
                                    Box::new(PartProgressSpec::Life),
                                    3.0,
                                )),
                                InterpKind::Pow5In,
                            );
                            part.move_rot = 32.0;
                            part.rotation = -6.0;
                            part.move_y = 1.5;
                            part.x = 0.75;
                            part.y = -1.5;
                            part
                        });
                        unit
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_sound = Some(SoundId::SHOOT_MISSILE_LARGE);
            weapon.shoot_sound_volume = Some(0.6);
            weapon.x = Some(19.5);
            weapon.y = Some(-2.5);
            weapon.mirror = Some(true);
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(0.4);
            weapon.reload = Some(70.0);
            weapon.layer_offset = Some(-20.0);
            weapon.recoil = Some(1.0);
            weapon.rotation_limit = Some(22.0);
            weapon.min_warmup = Some(0.95);
            weapon.shoot_warmup_speed = Some(0.1);
            weapon.shoot_y = Some(2.0);
            weapon.shoot_cone = Some(40.0);
            weapon.inaccuracy = Some(28.0);
            weapon.shake = Some(1.0);
            weapon.parts.push({
                let mut part = DrawPartSpec::region("-blade");
                part.heat_progress = PartProgressSpec::Warmup;
                part.progress = PartProgressSpec::Blend(
                    Box::new(PartProgressSpec::Warmup),
                    Box::new(PartProgressSpec::Reload),
                    0.15,
                );
                part.heat_color = Some(Rgba::new(0.6117647, 0.3137255, 1.0, 1.0));
                part.x = 1.25;
                part.y = 0.0;
                part.move_rot = -33.0;
                part.move_y = -1.0;
                part.move_x = -1.0;
                part.under = true;
                part.mirror = true;
                part
            });
            weapon
        });
        unit.abilities.push({
            let mut ability = AbilitySpec::suppression_field();
            ability.reload = 900.0;
            ability.range = 320.0;
            ability.orb_radius = 5.0;
            ability.particle_size = 3.0;
            ability.y = 10.0;
            ability.particles = 10;
            ability
        });
        unit.abilities.push({
            let mut ability = AbilitySpec::suppression_field();
            ability.orb_radius = 5.0;
            ability.particle_size = 3.0;
            ability.y = -8.0;
            ability.x = -10.75;
            ability.particles = 10;
            ability.active = false;
            ability
        });
        unit.abilities.push({
            let mut ability = AbilitySpec::suppression_field();
            ability.orb_radius = 5.0;
            ability.particle_size = 3.0;
            ability.y = -8.0;
            ability.x = 10.75;
            ability.particles = 10;
            ability.active = false;
            ability
        });
        unit.engines_mirror
            .push(EngineSpec::new(23.75, -14.0, 5.0, 330.0));
        unit.engines_mirror
            .push(EngineSpec::new(22.25, -23.75, 4.0, 315.0));
        unit
    })
}

fn renale(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("renale", UnitKind::NeoplasmUnitType, entity::CRAWL);
        unit.health = Some(500.0);
        unit.armor = Some(2.0);
        unit.hit_size = Some(9.0);
        unit.omni_movement = Some(false);
        unit.rotate_speed = Some(2.5);
        unit.drown_time_multiplier = Some(1.75);
        unit.segments = Some(3);
        unit.draw_body = Some(false);
        unit.hidden = Some(true);
        unit.crush_damage = Some(0.5);
        unit.ai_controller = Some(AiControllerKind::Hug);
        unit.target_air = Some(false);
        unit.segment_scl = Some(3.0);
        unit.segment_phase = Some(5.0);
        unit.segment_mag = Some(0.5);
        unit.speed = Some(1.2);
        unit
    })
}

fn latum(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("latum", UnitKind::NeoplasmUnitType, entity::CRAWL);
        unit.health = Some(20000.0);
        unit.armor = Some(12.0);
        unit.hit_size = Some(48.0);
        unit.omni_movement = Some(false);
        unit.rotate_speed = Some(1.7);
        unit.segments = Some(4);
        unit.draw_body = Some(false);
        unit.hidden = Some(true);
        unit.crush_damage = Some(2.0);
        unit.ai_controller = Some(AiControllerKind::Hug);
        unit.target_air = Some(false);
        unit.segment_scl = Some(4.0);
        unit.segment_phase = Some(5.0);
        unit.speed = Some(1.0);
        unit.abilities
            .push(AbilitySpec::spawn_death("renale", 5, 11.0));
        unit
    })
}

fn evoke(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("evoke", UnitKind::ErekirUnitType, entity::AIR_PAYLOAD);
        unit.core_unit_dock = Some(true);
        unit.controller = Some(ControllerKind::Builder {
            core_flee_range: 500.0,
        });
        unit.is_enemy = Some(false);
        unit.env_disabled = Some(EnvMask::none());
        unit.range = Some(60.0);
        unit.face_target = Some(true);
        unit.target_priority = Some(-2.0);
        unit.low_altitude = Some(false);
        unit.mine_walls = Some(true);
        unit.mine_floor = Some(false);
        unit.mine_hardness_scaling = Some(false);
        unit.flying = Some(true);
        unit.mine_speed = Some(6.0);
        unit.mine_tier = Some(3);
        unit.build_speed = Some(1.2);
        unit.drag = Some(0.08);
        unit.speed = Some(5.6);
        unit.rotate_speed = Some(7.0);
        unit.accel = Some(0.09);
        unit.item_capacity = Some(60);
        unit.health = Some(300.0);
        unit.armor = Some(1.0);
        unit.hit_size = Some(9.0);
        unit.engine_size = Some(0.0);
        unit.payload_capacity = Some(256.0);
        unit.pickup_units = Some(false);
        unit.vulnerable_with_payloads = Some(true);
        unit.fog_radius = Some(0.0);
        unit.targetable = Some(false);
        unit.hittable = Some(false);
        unit.aim_dst = Some(0.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::RepairBeamWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    max_range: Some(60.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.width_sin_mag = Some(0.11);
            weapon.reload = Some(20.0);
            weapon.x = Some(0.0);
            weapon.y = Some(6.5);
            weapon.rotate = Some(false);
            weapon.shoot_y = Some(0.0);
            weapon.beam_width = Some(0.7);
            weapon.repair_speed = Some(3.1);
            weapon.fraction_repair_speed = Some(0.06);
            weapon.shoot_cone = Some(15.0);
            weapon.mirror = Some(false);
            weapon.target_units = Some(false);
            weapon.target_buildings = Some(true);
            weapon.auto_target = Some(false);
            weapon.controllable = Some(true);
            weapon.laser_color = Some(Rgba::new(1.0, 0.827451, 0.49803922, 1.0));
            weapon.heal_color = Some(Rgba::new(1.0, 0.827451, 0.49803922, 1.0));
            weapon
        });
        unit.engines_mirror
            .push(EngineSpec::new(5.25, 4.75, 2.2, 45.0));
        unit.engines_mirror
            .push(EngineSpec::new(5.75, -5.5, 2.2, 315.0));
        unit
    })
}

fn incite(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("incite", UnitKind::ErekirUnitType, entity::AIR_PAYLOAD);
        unit.core_unit_dock = Some(true);
        unit.controller = Some(ControllerKind::Builder {
            core_flee_range: 500.0,
        });
        unit.is_enemy = Some(false);
        unit.env_disabled = Some(EnvMask::none());
        unit.range = Some(60.0);
        unit.target_priority = Some(-2.0);
        unit.low_altitude = Some(false);
        unit.face_target = Some(true);
        unit.mine_walls = Some(true);
        unit.mine_floor = Some(false);
        unit.mine_hardness_scaling = Some(false);
        unit.flying = Some(true);
        unit.mine_speed = Some(8.0);
        unit.mine_tier = Some(3);
        unit.build_speed = Some(1.4);
        unit.drag = Some(0.08);
        unit.speed = Some(7.0);
        unit.rotate_speed = Some(8.0);
        unit.accel = Some(0.09);
        unit.item_capacity = Some(90);
        unit.health = Some(500.0);
        unit.armor = Some(2.0);
        unit.hit_size = Some(11.0);
        unit.payload_capacity = Some(256.0);
        unit.pickup_units = Some(false);
        unit.vulnerable_with_payloads = Some(true);
        unit.fog_radius = Some(0.0);
        unit.targetable = Some(false);
        unit.hittable = Some(false);
        unit.engine_offset = Some(7.2);
        unit.engine_size = Some(3.1);
        unit.aim_dst = Some(0.0);
        unit.draw_build_beam = Some(false);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::RepairBeamWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    max_range: Some(60.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.width_sin_mag = Some(0.11);
            weapon.reload = Some(20.0);
            weapon.x = Some(0.0);
            weapon.y = Some(7.5);
            weapon.rotate = Some(false);
            weapon.shoot_y = Some(0.0);
            weapon.beam_width = Some(0.7);
            weapon.shoot_cone = Some(15.0);
            weapon.mirror = Some(false);
            weapon.repair_speed = Some(3.3);
            weapon.fraction_repair_speed = Some(0.06);
            weapon.target_units = Some(false);
            weapon.target_buildings = Some(true);
            weapon.auto_target = Some(false);
            weapon.controllable = Some(true);
            weapon.laser_color = Some(Rgba::new(1.0, 0.827451, 0.49803922, 1.0));
            weapon.heal_color = Some(Rgba::new(1.0, 0.827451, 0.49803922, 1.0));
            weapon
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "build-weapon",
                kind: WeaponKind::BuildWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.rotate = Some(true);
            weapon.rotate_speed = Some(7.0);
            weapon.x = Some(3.5);
            weapon.y = Some(3.75);
            weapon.layer_offset = Some(-0.001);
            weapon.shoot_y = Some(3.0);
            weapon
        });
        unit.engines_mirror
            .push(EngineSpec::new(6.25, -0.25, 2.4, 300.0));
        unit
    })
}

fn emanate(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("emanate", UnitKind::ErekirUnitType, entity::AIR_PAYLOAD);
        unit.core_unit_dock = Some(true);
        unit.controller = Some(ControllerKind::Builder {
            core_flee_range: 500.0,
        });
        unit.is_enemy = Some(false);
        unit.env_disabled = Some(EnvMask::none());
        unit.range = Some(65.0);
        unit.face_target = Some(true);
        unit.target_priority = Some(-2.0);
        unit.low_altitude = Some(false);
        unit.mine_walls = Some(true);
        unit.mine_floor = Some(false);
        unit.mine_hardness_scaling = Some(false);
        unit.flying = Some(true);
        unit.mine_speed = Some(9.0);
        unit.mine_tier = Some(3);
        unit.build_speed = Some(1.5);
        unit.drag = Some(0.08);
        unit.speed = Some(7.5);
        unit.rotate_speed = Some(8.0);
        unit.accel = Some(0.08);
        unit.item_capacity = Some(110);
        unit.health = Some(700.0);
        unit.armor = Some(3.0);
        unit.hit_size = Some(12.0);
        unit.build_beam_offset = Some(8.0);
        unit.payload_capacity = Some(256.0);
        unit.pickup_units = Some(false);
        unit.vulnerable_with_payloads = Some(true);
        unit.fog_radius = Some(0.0);
        unit.targetable = Some(false);
        unit.hittable = Some(false);
        unit.engine_offset = Some(7.5);
        unit.engine_size = Some(3.4);
        unit.aim_dst = Some(0.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::RepairBeamWeapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Plain,
                    max_range: Some(65.0),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.width_sin_mag = Some(0.11);
            weapon.reload = Some(20.0);
            weapon.x = Some(4.75);
            weapon.y = Some(4.75);
            weapon.rotate = Some(false);
            weapon.shoot_y = Some(0.0);
            weapon.beam_width = Some(0.7);
            weapon.shoot_cone = Some(40.0);
            weapon.mirror = Some(true);
            weapon.repair_speed = Some(1.8);
            weapon.fraction_repair_speed = Some(0.03);
            weapon.target_units = Some(false);
            weapon.target_buildings = Some(true);
            weapon.auto_target = Some(false);
            weapon.controllable = Some(true);
            weapon.laser_color = Some(Rgba::new(1.0, 0.827451, 0.49803922, 1.0));
            weapon.heal_color = Some(Rgba::new(1.0, 0.827451, 0.49803922, 1.0));
            weapon
        });
        unit.engines_mirror
            .push(EngineSpec::new(8.75, -3.25, 2.7, 315.0));
        unit.engines_mirror
            .push(EngineSpec::new(7.0, -8.75, 2.7, 315.0));
        unit
    })
}

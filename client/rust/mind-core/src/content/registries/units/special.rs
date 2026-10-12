// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/UnitTypes.java (wave `special`).
//
//! Generated unit metadata (`UnitTypes.java`, wave `special`).
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
use crate::content::registries::pal;
use crate::content::registries::planets::EnvFlag;
use crate::content::registries::sound_meta::SoundId;

/// Loads the `special` wave in upstream order.
pub fn load(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    block(sink)?;
    manifold(sink)?;
    assembly_drone(sink)?;
    dummy(sink)?;
    Ok(())
}

/// `Blocks.java` `scathe` ammo `spawnUnit` missiles.
///
/// Java constructs these inline while the `scathe` block initializes
/// (`Blocks.java:5251-5530`), so their unit ids chain straight after `dummy`
/// (65..68 in JVM content order: `scathe-missile`, `scathe-missile-phase`,
/// `scathe-missile-surge`, `scathe-missile-surge-split`). The split missile is
/// registered by the surge missile's frag bullet, exactly like upstream
/// (`BulletSpec.spawn_unit` registers it inline through the sink).
pub fn scathe_missiles(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    // `Items.carbide` ammo: `new MissileUnitType("scathe-missile")`.
    sink.push_unit({
        let mut unit = spec("scathe-missile", UnitKind::MissileUnitType, entity::MISSILE);
        unit.speed = Some(4.6);
        unit.max_range = Some(6.0);
        unit.lifetime = Some(60.0 * 5.5);
        unit.hit_size = Some(10.0);
        unit.engine_color = Some(pal::RED_LIGHT);
        unit.trail_color = Some(pal::RED_LIGHT);
        // `engineLayer = Layer.effect` (110).
        unit.engine_layer = Some(110.0);
        unit.engine_size = Some(3.1);
        unit.engine_offset = Some(10.0);
        unit.rotate_speed = Some(0.25);
        unit.trail_length = Some(18);
        unit.missile_accel_time = Some(50.0);
        unit.low_altitude = Some(true);
        unit.loop_sound_volume = Some(0.6);
        unit.death_sound = Some(SoundId::EXPLOSION_MISSILE);
        unit.target_air = Some(false);
        unit.target_under_blocks = Some(false);
        unit.fog_radius = Some(6.0);
        unit.health = Some(240.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Explosion,
                    splash_damage: Some(1000.0),
                    splash_damage_radius: Some(65.0),
                    // `rangeOverride = max(rangeOverride, radius * 2 / 3)`
                    // (ExplosionBulletType.java:11): 65 * 2/3.
                    range_override: Some(43.333336),
                    hit_color: Some(pal::RED_LIGHT),
                    shoot_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        12.0,
                        vec![
                            EffectRef::Named(EffectId::MASSIVE_EXPLOSION),
                            EffectRef::Named(EffectId::SCATHE_EXPLOSION),
                            EffectRef::Named(EffectId::SCATHE_LIGHT),
                            scathe_wave(130.0),
                        ],
                    )))),
                    collides_air: Some(false),
                    building_damage_multiplier: Some(0.1),
                    ammo_multiplier: Some(1.0),
                    frag_life_min: Some(0.1),
                    frag_bullets: Some(7),
                    frag_bullet: Some(Box::new(scathe_artillery_frag(
                        0.1,
                        40.0,
                        100.0,
                        pal::RED_LIGHT,
                    ))),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_cone = Some(360.0);
            weapon.mirror = Some(false);
            weapon.reload = Some(1.0);
            weapon.shake = Some(10.0);
            weapon.shoot_on_death = Some(true);
            weapon.shoot_on_death_effect = Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION));
            weapon
        });
        unit
    })?;

    // `Items.phaseFabric` ammo: `new MissileUnitType("scathe-missile-phase")`.
    // `Color.valueOf("ffd37f")` is `Pal.accent`.
    sink.push_unit({
        let mut unit = spec(
            "scathe-missile-phase",
            UnitKind::MissileUnitType,
            entity::MISSILE,
        );
        unit.speed = Some(2.5);
        unit.max_range = Some(6.0);
        unit.lifetime = Some(60.0 * 9.77);
        unit.hit_size = Some(10.0);
        unit.engine_color = Some(pal::ACCENT);
        unit.trail_color = Some(pal::ACCENT);
        // `engineLayer = Layer.effect` (110).
        unit.engine_layer = Some(110.0);
        unit.engine_size = Some(3.1);
        unit.engine_offset = Some(10.0);
        unit.rotate_speed = Some(0.2);
        unit.trail_length = Some(18);
        unit.missile_accel_time = Some(50.0);
        unit.low_altitude = Some(true);
        unit.loop_sound_volume = Some(0.6);
        unit.death_sound = Some(SoundId::EXPLOSION_MISSILE);
        unit.target_air = Some(false);
        unit.target_under_blocks = Some(false);
        unit.fog_radius = Some(6.0);
        unit.health = Some(500.0);
        // `ShapePart` (engine ring; purely visual but cheap to carry).
        unit.parts.push({
            let mut part = DrawPartSpec::shape();
            part.progress = PartProgressSpec::Constant(1.0);
            part.color = Some(pal::ACCENT);
            part.sides = 6;
            part.radius = 3.0;
            part.rotate_speed = 3.0;
            part.hollow = true;
            // `layer = Layer.effect` (110).
            part.layer = 110.0;
            part.y = 1.8;
            part
        });
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Explosion,
                    splash_damage: Some(320.0),
                    splash_damage_radius: Some(120.0),
                    // `120 * 2/3` (ExplosionBulletType.java:11).
                    range_override: Some(80.0),
                    reload_multiplier: Some(0.8),
                    ammo_multiplier: Some(5.0),
                    hit_color: Some(pal::ACCENT),
                    shoot_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        12.0,
                        vec![
                            EffectRef::Named(EffectId::MASSIVE_EXPLOSION),
                            EffectRef::Named(EffectId::SCATHE_EXPLOSION),
                            EffectRef::Named(EffectId::SCATHE_LIGHT),
                            scathe_wave(130.0),
                        ],
                    )))),
                    collides_air: Some(false),
                    building_damage_multiplier: Some(0.1),
                    frag_life_min: Some(0.1),
                    frag_bullets: Some(7),
                    frag_bullet: Some(Box::new(scathe_artillery_frag(
                        0.2,
                        56.0,
                        120.0,
                        pal::ACCENT,
                    ))),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_cone = Some(360.0);
            weapon.mirror = Some(false);
            weapon.reload = Some(1.0);
            weapon.shake = Some(10.0);
            weapon.shoot_on_death = Some(true);
            weapon.shoot_on_death_effect = Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION));
            weapon
        });
        // `new ForceFieldAbility(120f, 0f, 3000f, 999999999f)`.
        unit.abilities.push(AbilitySpec::force_field(
            120.0,
            0.0,
            3000.0,
            999_999_999.0,
            None,
            None,
        ));
        unit
    })?;

    // `Items.surgeAlloy` ammo: `new MissileUnitType("scathe-missile-surge")`.
    // Its weapon's frag bullet spawns `scathe-missile-surge-split` inline.
    // `Color.valueOf("f7e97e")`.
    const SURGE: Rgba = Rgba::from_rgba8888(0xf7e97eff);
    sink.push_unit({
        let mut unit = spec(
            "scathe-missile-surge",
            UnitKind::MissileUnitType,
            entity::MISSILE,
        );
        unit.speed = Some(4.4);
        unit.max_range = Some(6.0);
        unit.lifetime = Some(60.0 * 1.4);
        unit.hit_size = Some(10.0);
        unit.engine_color = Some(SURGE);
        unit.trail_color = Some(SURGE);
        // `engineLayer = Layer.effect` (110).
        unit.engine_layer = Some(110.0);
        unit.engine_size = Some(3.1);
        unit.engine_offset = Some(10.0);
        unit.rotate_speed = Some(0.25);
        unit.trail_length = Some(18);
        unit.missile_accel_time = Some(30.0);
        unit.low_altitude = Some(true);
        unit.loop_sound_volume = Some(0.6);
        unit.death_sound = Some(SoundId::EXPLOSION_MISSILE);
        unit.target_air = Some(false);
        unit.target_under_blocks = Some(false);
        unit.fog_radius = Some(6.0);
        unit.health = Some(300.0);
        unit.weapons.push({
            let mut weapon = WeaponSpec {
                name: "",
                kind: WeaponKind::Weapon,
                bullet: BulletRef::Inline(Box::new(BulletSpec {
                    kind: BulletKind::Explosion,
                    splash_damage: Some(1800.0),
                    splash_damage_radius: Some(40.0),
                    // `40 * 2/3` (ExplosionBulletType.java:11).
                    range_override: Some(26.666668),
                    ammo_multiplier: Some(1.0),
                    reload_multiplier: Some(0.9),
                    lightning: Some(10),
                    lightning_damage: Some(45.0),
                    lightning_length: Some(12),
                    hit_color: Some(SURGE),
                    shoot_effect: Some(EffectRef::Inline(Box::new(EffectSpec::multi(
                        12.0,
                        vec![
                            EffectRef::Named(EffectId::MASSIVE_EXPLOSION),
                            EffectRef::Named(EffectId::SCATHE_EXPLOSION_SMALL),
                        ],
                    )))),
                    collides_air: Some(false),
                    building_damage_multiplier: Some(0.1),
                    frag_life_min: Some(0.1),
                    frag_bullets: Some(5),
                    frag_random_spread: Some(0.0),
                    frag_spread: Some(20.0),
                    frag_bullet: Some(Box::new(BulletSpec {
                        kind: BulletKind::Plain,
                        shoot_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG)),
                        smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_SMOKE_MISSILE_COLOR)),
                        hit_color: Some(SURGE),
                        ammo_multiplier: Some(1.0),
                        spawn_unit: Some(Box::new({
                            let mut split = spec(
                                "scathe-missile-surge-split",
                                UnitKind::MissileUnitType,
                                entity::MISSILE,
                            );
                            split.speed = Some(4.8);
                            split.max_range = Some(6.0);
                            split.lifetime = Some(60.0 * 3.7);
                            split.engine_color = Some(SURGE);
                            split.trail_color = Some(SURGE);
                            // `engineLayer = Layer.effect` (110).
                            split.engine_layer = Some(110.0);
                            split.engine_size = Some(2.2);
                            split.engine_offset = Some(8.0);
                            split.rotate_speed = Some(1.4);
                            split.trail_length = Some(12);
                            split.low_altitude = Some(true);
                            split.loop_sound_volume = Some(0.6);
                            split.death_sound = Some(SoundId::EXPLOSION_MISSILE);
                            split.target_air = Some(false);
                            split.target_under_blocks = Some(false);
                            split.fog_radius = Some(6.0);
                            split.health = Some(50.0);
                            split.weapons.push({
                                let mut weapon = WeaponSpec {
                                    name: "",
                                    kind: WeaponKind::Weapon,
                                    bullet: BulletRef::Inline(Box::new(BulletSpec {
                                        kind: BulletKind::Explosion,
                                        splash_damage: Some(180.0),
                                        splash_damage_radius: Some(35.0),
                                        // `35 * 2/3` (ExplosionBulletType.java:11).
                                        range_override: Some(23.333334),
                                        lightning: Some(4),
                                        lightning_damage: Some(25.0),
                                        lightning_length: Some(6),
                                        hit_color: Some(SURGE),
                                        shoot_effect: Some(EffectRef::Inline(Box::new(
                                            EffectSpec::multi(
                                                12.0,
                                                vec![
                                                    EffectRef::Named(EffectId::MASSIVE_EXPLOSION),
                                                    EffectRef::Named(
                                                        EffectId::SCATHE_EXPLOSION_SMALL,
                                                    ),
                                                    EffectRef::Named(EffectId::SCATHE_LIGHT_SMALL),
                                                    scathe_wave(100.0),
                                                ],
                                            ),
                                        ))),
                                        collides_air: Some(false),
                                        building_damage_multiplier: Some(0.1),
                                        ..BulletSpec::default()
                                    })),
                                    ..WeaponSpec::default()
                                };
                                weapon.shoot_cone = Some(360.0);
                                weapon.mirror = Some(false);
                                weapon.reload = Some(1.0);
                                weapon.shake = Some(10.0);
                                weapon.shoot_on_death = Some(true);
                                weapon.shoot_on_death_effect =
                                    Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION));
                                weapon
                            });
                            split
                        })),
                        ..BulletSpec::default()
                    })),
                    ..BulletSpec::default()
                })),
                ..WeaponSpec::default()
            };
            weapon.shoot_cone = Some(360.0);
            weapon.rotate = Some(true);
            // `rotationLimit = rotateSpeed = 0f`.
            weapon.rotation_limit = Some(0.0);
            weapon.rotate_speed = Some(0.0);
            weapon.reload = Some(1.0);
            weapon.shake = Some(10.0);
            weapon.shoot_on_death = Some(true);
            weapon.shoot_on_death_effect = Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION));
            weapon
        });
        unit
    })?;
    Ok(())
}

/// `new WaveEffect(){lifetime = 10f; strokeFrom = 4f; sizeTo = <size>}` used by
/// the scathe death explosions (`Blocks.java:5278`, `:5369`, `:5519`).
fn scathe_wave(size_to: f32) -> EffectRef {
    let mut effect = EffectSpec::wave();
    effect.lifetime = 10.0;
    effect.stroke_from = 4.0;
    effect.size_to = size_to;
    EffectRef::Inline(Box::new(effect))
}

/// `new ArtilleryBulletType(3.4f, 32f)` scathe frag bullet (`Blocks.java`
/// `:5295`, `:5391`). Only `buildingDamageMultiplier`, the splash values and the
/// tint color differ per ammo type.
fn scathe_artillery_frag(
    building_damage_multiplier: f32,
    splash_damage_radius: f32,
    splash_damage: f32,
    color: Rgba,
) -> BulletSpec {
    BulletSpec {
        kind: BulletKind::Artillery,
        speed: Some(3.4),
        damage: Some(32.0),
        building_damage_multiplier: Some(building_damage_multiplier),
        drag: Some(0.02),
        hit_effect: Some(EffectRef::Named(EffectId::MASSIVE_EXPLOSION)),
        despawn_effect: Some(EffectRef::Named(EffectId::SCATHE_SLASH)),
        knockback: Some(0.8),
        lifetime: Some(23.0),
        width: Some(18.0),
        height: Some(18.0),
        collides_tiles: Some(false),
        splash_damage_radius: Some(splash_damage_radius),
        splash_damage: Some(splash_damage),
        back_color: Some(color),
        trail_color: Some(color),
        hit_color: Some(color),
        front_color: Some(Rgba::WHITE),
        smoke_effect: Some(EffectRef::Named(EffectId::SHOOT_BIG_SMOKE2)),
        despawn_shake: Some(7.0),
        light_radius: Some(30.0),
        light_color: Some(color),
        light_opacity: Some(0.5),
        trail_length: Some(20),
        trail_width: Some(3.5),
        trail_effect: Some(EffectRef::Named(EffectId::NONE)),
        ..BulletSpec::default()
    }
}

fn block(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("block", UnitKind::UnitType, entity::BLOCK);
        unit.speed = Some(0.0);
        unit.hit_size = Some(0.0);
        unit.health = Some(1.0);
        unit.rotate_speed = Some(360.0);
        unit.item_capacity = Some(0);
        unit.hidden = Some(true);
        unit.internal = Some(true);
        unit
    })
}

fn manifold(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("manifold", UnitKind::ErekirUnitType, entity::TETHER);
        unit.controller = Some(ControllerKind::Cargo);
        unit.is_enemy = Some(false);
        unit.allowed_in_payloads = Some(false);
        unit.logic_controllable = Some(false);
        unit.player_controllable = Some(false);
        unit.env_disabled = Some(EnvMask::none());
        unit.payload_capacity = Some(0.0);
        unit.low_altitude = Some(false);
        unit.flying = Some(true);
        unit.drag = Some(0.06);
        unit.speed = Some(3.5);
        unit.rotate_speed = Some(9.0);
        unit.accel = Some(0.1);
        unit.item_capacity = Some(100);
        unit.health = Some(200.0);
        unit.hit_size = Some(11.0);
        unit.engine_size = Some(2.3);
        unit.engine_offset = Some(6.5);
        unit.hidden = Some(true);
        unit.engines_mirror
            .push(EngineSpec::new(6.0, -6.0, 2.3, 315.0));
        unit
    })
}

fn assembly_drone(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("assembly-drone", UnitKind::ErekirUnitType, entity::TETHER);
        unit.controller = Some(ControllerKind::Assembler);
        unit.flying = Some(true);
        unit.drag = Some(0.06);
        unit.accel = Some(0.11);
        unit.speed = Some(1.3);
        unit.health = Some(90.0);
        unit.engine_size = Some(2.0);
        unit.engine_offset = Some(6.5);
        unit.payload_capacity = Some(0.0);
        unit.targetable = Some(false);
        unit.bounded = Some(false);
        unit.outline_color = Some(Rgba::new(0.1764706, 0.18431373, 0.22352941, 1.0));
        unit.is_enemy = Some(false);
        unit.hidden = Some(true);
        unit.use_unit_cap = Some(false);
        unit.logic_controllable = Some(false);
        unit.player_controllable = Some(false);
        unit.allowed_in_payloads = Some(false);
        unit.create_wreck = Some(false);
        unit.env_enabled = Some(EnvMask::any());
        unit.env_disabled = Some(EnvMask::none());
        unit
    })
}

fn dummy(sink: &mut dyn UnitSink) -> Result<(), ContentError> {
    sink.push_unit({
        let mut unit = spec("dummy", UnitKind::UnitType, entity::DUMMY);
        unit.controller = Some(ControllerKind::No);
        unit.env_enabled = Some(EnvMask::any());
        unit.env_disabled = Some(EnvMask::none());
        unit.is_enemy = Some(false);
        unit.allowed_in_payloads = Some(false);
        unit.logic_controllable = Some(false);
        unit.player_controllable = Some(false);
        unit.hidden = Some(true);
        unit.hoverable = Some(false);
        unit.can_boost = Some(true);
        unit.use_unit_cap = Some(false);
        unit.killable = Some(false);
        unit.physics = Some(false);
        unit.internal = Some(true);
        unit.internal_generate_sprites = Some(true);
        unit.flying_layer = Some(114.0);
        unit.drag = Some(0.33);
        unit.hit_size = Some(12.0);
        unit.hide_details = Some(false);
        unit.engine_offset = Some(7.0);
        unit.engine_size = Some(2.0);
        unit
    })
}

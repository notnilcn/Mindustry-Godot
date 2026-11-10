// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Team-multiplier, player possession plus repair/sound regressions (uses the `campaign_sim` fixture).
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{health, live_sim, place_at, tick_n};

/// G01 T5: `ShieldComp.damage` divides by `rules.unitHealth(team)`, so a
/// wave-team `unitHealthMultiplier` of 2 halves incoming damage.
#[test]
fn wave_unit_health_multiplier_halves_damage() {
    let mut sim = campaign_sim(48, |rules| {
        rules.team_rule_mut(2).unit_health_multiplier = 2.0;
    });
    let dagger = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("dagger");
    let before = health(&sim, dagger).expect("health");
    let content = crate::content::test_support::test_registry();
    let applied = crate::combat::damage::area::damage_entity(
        &mut sim.ecs.0,
        &content,
        dagger,
        20.0,
        true,
        1.0,
    );
    let after = health(&sim, dagger).expect("health");
    assert_eq!(
        before - after,
        10.0,
        "unitHealth 2 halves 20 damage (applied {applied})"
    );
}

/// G01 T5: `BulletType.damageMultiplier` scales a unit-owned bullet by
/// `rules.unitDamage(team)`.
#[test]
fn unit_damage_multiplier_scales_bullets() {
    use crate::combat::bullet::{Bullet, BulletSpawn};
    use crate::determinism::SimRng;

    let mut sim = campaign_sim(48, |rules| {
        rules.team_rule_mut(1).unit_damage_multiplier = 2.0;
    });
    let shooter = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    let def_id = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime
            .content
            .unit_by_name("dagger")
            .expect("dagger def")
            .weapons[0]
            .bullet
            .id
    };
    let content = crate::content::test_support::test_registry();
    let mut rng = SimRng::new(7);
    let spawn = BulletSpawn {
        def: def_id,
        owner: Some(shooter),
        team: 1,
        x: 100.0,
        y: 100.0,
        damage: -1.0,
        damage_multiplier: 1.0,
        ..BulletSpawn::default()
    };
    let bullet = crate::combat::bullet::create(&mut sim.ecs.0, &content, &mut rng, 1, &spawn)
        .expect("bullet");
    let state = sim.ecs.0.get::<Bullet>(bullet).expect("bullet state");
    let base = content.bullet(def_id).expect("def").damage;
    assert_eq!(
        state.damage,
        2.0 * base,
        "unitDamage 2 doubles the {base} base damage"
    );
}

/// G01 T5: `BuildingComp.damage` turns a zero `rules.blockHealth(team)` into
/// `damage = health + 1` (any hit kills).
#[test]
fn block_health_zero_kills() {
    let mut sim = campaign_sim(48, |rules| {
        rules.block_health_multiplier = 0.0;
    });
    let wall = place_at(&mut sim, 20, 20, "copper-wall", 1);
    crate::combat::damage::area::apply_health(&mut sim.ecs.0, wall, 1.0);
    let state = sim.ecs.0.get::<Health>(wall).expect("health");
    assert!(
        state.dead || state.health <= 0.0,
        "blockHealth 0 kills on 1 damage (health {})",
        state.health
    );
}

/// G01 T8: `CoreBlock.playerSpawn` uses the core's `unitType` and the death
/// timer respawns at the best core after 60 ticks.
#[test]
fn player_respawns_after_death() {
    use crate::content::ctype::Mappable;
    use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};

    let mut sim = live_sim(48);
    let _core = place_at(&mut sim, 20, 20, "core-foundation", 1);
    let player = sim.spawn_player_at_best_core(1).expect("player spawn");
    let unit_type = sim.ecs.0.get::<UnitTypeComp>(player).and_then(|comp| {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.unit(comp.type_id).map(|def| def.name())
    });
    assert_eq!(unit_type, Some("beta"), "foundation core spawns beta");
    assert!(
        sim.ecs
            .0
            .get::<UnitCore>(player)
            .is_some_and(|core| core.spawned_by_core),
        "spawnedByCore set"
    );
    // Kill the player unit.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(player) {
        health.health = 0.0;
        health.dead = true;
    }
    tick_n(&mut sim, 59);
    assert!(
        sim.player_unit().is_none(),
        "no respawn before the 60-tick death delay"
    );
    sim.tick().expect("tick");
    let respawned = sim.player_unit().expect("respawned after 60 ticks");
    let (rx, ry) = sim.player_unit_position().expect("position");
    assert!(
        (rx - 164.0).abs() <= 3.0 && (ry - 164.0).abs() <= 3.0,
        "respawn at the foundation core centre ({rx}, {ry})"
    );
    let kind = sim.ecs.0.get::<UnitTypeComp>(respawned).and_then(|comp| {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.unit(comp.type_id).map(|def| def.name())
    });
    assert_eq!(kind, Some("beta"), "respawn uses the core's unit type");
}

/// G01 T8: a shard core spawns `alpha`.
#[test]
fn core_shard_spawns_alpha() {
    use crate::content::ctype::Mappable;
    use crate::entities::comp::unit::comp::UnitTypeComp;

    let mut sim = live_sim(48);
    let _core = place_at(&mut sim, 20, 20, "core-shard", 1);
    let player = sim.spawn_player_at_best_core(1).expect("player spawn");
    let name = sim.ecs.0.get::<UnitTypeComp>(player).and_then(|comp| {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.unit(comp.type_id).map(|def| def.name())
    });
    assert_eq!(name, Some("alpha"));
}

/// G01 T9: possessing an own AI unit moves the Player controller and despawns
/// the previous core-spawned unit without death effects.
#[test]
fn possess_own_dagger_despawns_core_alpha() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::entities::comp::Player;

    let mut sim = live_sim(48);
    let _core = place_at(&mut sim, 20, 20, "core-shard", 1);
    let alpha = sim.spawn_player_at_best_core(1).expect("alpha");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 180.0, 180.0, 0.0)
        .expect("dagger");
    let seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    assert!(sim.possess_unit(seq), "possess the dagger");
    assert_eq!(sim.player_unit(), Some(dagger));
    assert!(
        sim.ecs.0.get_entity(alpha).is_err(),
        "core alpha despawned by possession"
    );
    assert!(
        sim.ecs.0.get::<Player>(dagger).is_some(),
        "dagger carries the Player marker"
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(dagger)
            .map(|slot| slot.kind),
        Some(AiKind::Player)
    );
}

/// G01 T9: `UnitClear` on a possessed unit releases it to AI and respawns at
/// the core.
#[test]
fn clear_unit_returns_to_core() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::content::ctype::Mappable;
    use crate::entities::comp::Player;
    use crate::entities::comp::unit::comp::UnitTypeComp;

    let mut sim = live_sim(48);
    let _core = place_at(&mut sim, 20, 20, "core-shard", 1);
    let _alpha = sim.spawn_player_at_best_core(1).expect("alpha");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 180.0, 180.0, 0.0)
        .expect("dagger");
    let seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    assert!(sim.possess_unit(seq));
    assert!(sim.clear_unit(), "unit clear");
    let respawned = sim.player_unit().expect("respawned at the core");
    assert_ne!(respawned, dagger, "a fresh core unit was spawned");
    assert!(
        sim.ecs.0.get::<Player>(dagger).is_none(),
        "released unit dropped the Player marker"
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(dagger)
            .map(|slot| slot.kind),
        Some(AiKind::Ground),
        "released unit went back to its team AI"
    );
    let name = sim.ecs.0.get::<UnitTypeComp>(respawned).and_then(|comp| {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.unit(comp.type_id).map(|def| def.name())
    });
    assert_eq!(name, Some("alpha"), "respawn uses the core unit type");
}

/// G01 T9: enemy and dead units cannot be possessed.
#[test]
fn cannot_possess_enemy_or_dead_unit() {
    let mut sim = live_sim(48);
    let _core = place_at(&mut sim, 20, 20, "core-shard", 1);
    let _alpha = sim.spawn_player_at_best_core(1).expect("alpha");
    let enemy = sim
        .spawn_live_unit("dagger", 2, 220.0, 220.0, 0.0)
        .expect("enemy");
    let enemy_seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(enemy)
        .expect("seq")
        .0 as i32;
    assert!(
        !sim.possess_unit(enemy_seq),
        "enemy unit is not possessable"
    );
    let own = sim
        .spawn_live_unit("dagger", 1, 180.0, 180.0, 0.0)
        .expect("own");
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(own) {
        health.health = 0.0;
        health.dead = true;
    }
    let own_seq = sim.ecs.0.get::<crate::ecs::EntitySeq>(own).expect("seq").0 as i32;
    assert!(!sim.possess_unit(own_seq), "dead unit is not possessable");
}

/// G01 T19: `Teams.bosses` lists wave-team units carrying the `boss` status.
#[test]
fn boss_list_tracks_wave_boss() {
    let mut sim = campaign_sim(48, |rules| {
        rules.wave_team = 2;
    });
    let boss = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("wave dagger");
    let own = sim
        .spawn_live_unit("dagger", 1, 130.0, 100.0, 0.0)
        .expect("own dagger");
    let boss_status = sim
        .unit_runtime()
        .expect("rt")
        .content
        .status_id("boss")
        .expect("boss status");
    for entity in [boss, own] {
        crate::combat::damage::status::apply_status(&mut sim.ecs.0, entity, boss_status, 999_999.0);
    }
    assert_eq!(sim.bosses(), vec![boss], "only the wave-team boss listed");
    let json: serde_json::Value = serde_json::from_str(&sim.boss_json()).expect("boss json parses");
    assert_eq!(json["count"], 1);
    assert_eq!(json["names"][0], "dagger");
    let first = &json["first"];
    assert_eq!(first["type"], "dagger");
    assert_eq!(first["flying"], false);
    // After the boss dies the list is empty.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(boss) {
        health.health = 0.0;
        health.dead = true;
    }
    assert!(sim.bosses().is_empty(), "dead boss is dropped");
    let json: serde_json::Value = serde_json::from_str(&sim.boss_json()).expect("boss json parses");
    assert_eq!(json["count"], 0);
    assert!(json["first"].is_null());
}

/// T12: a powered repair point heals a damaged friendly unit once its aim is
/// within 30 degrees (`RepairTurret.java:186-223`); the plan's expected
/// +`repairSpeed * strength * edelta` per aligned tick.
#[test]
fn repair_point_heals_damaged_dagger() {
    let mut sim = live_sim(48);
    let _repair = place_at(&mut sim, 20, 20, "repair-point", 1);
    let _power = place_at(&mut sim, 21, 20, "power-source", 1);
    let tx = 20.0 * 8.0 + 4.0;
    let ty = 20.0 * 8.0 + 4.0;
    let dagger = sim
        .spawn_live_unit("dagger", 1, tx + 30.0, ty, 180.0)
        .expect("friendly dagger");
    sim.ecs
        .0
        .get_mut::<Health>(dagger)
        .expect("dagger health")
        .health = 50.0;
    let max = sim
        .ecs
        .0
        .get::<Health>(dagger)
        .map(|health| health.max_health)
        .unwrap_or(0.0);
    tick_n(&mut sim, 300);
    let healed = health(&sim, dagger).expect("dagger alive");
    assert!(healed > 50.0, "repair point healed the dagger: {healed}");
    assert!(
        healed <= max,
        "heal clamped to max health: {healed} <= {max}"
    );
    // `UnitTypes.java:104`: dagger `health = 150`.
    assert_eq!(max, 150.0, "dagger max health");
}

/// PAR-0073/T3: a killed wall plays `destroySound` (an `At` with the size-1
/// alternate pair) and no `Construct`/break sound (`BuildingComp.java:1524-1543`).
#[test]
fn destroyed_wall_emits_destroy_sound() {
    use crate::audio::{AudioEvent, SharedAudioLog, SoundId};

    let mut sim = live_sim(32);
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let wall = place_at(&mut sim, 10, 10, "copper-wall", 1);
    sim.tick().expect("tick");
    // Drop the placement's own place sound so only the death remains.
    log.clear();
    sim.ecs
        .0
        .get_mut::<Health>(wall)
        .expect("wall health")
        .health = 0.0;
    sim.tick().expect("tick");
    let events = log.events();
    let ats: Vec<SoundId> = events
        .iter()
        .filter_map(|ticked| match &ticked.event {
            AudioEvent::At { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect();
    assert!(
        ats.iter().any(|sound| {
            *sound == SoundId::BLOCK_EXPLODE1 || *sound == SoundId::BLOCK_EXPLODE1_ALT
        }),
        "destroy sound in {ats:?}"
    );
    assert!(
        !events
            .iter()
            .any(|ticked| matches!(ticked.event, AudioEvent::Construct { .. })),
        "a destroyed wall must not play place/break sounds: {events:?}"
    );
}

/// G07 T14: payload pickup/drop sounds (`PayloadComp.java:107,117,145,203`).
#[test]
fn payload_commands_emit_sounds() {
    use crate::audio::{AudioEvent, SharedAudioLog, SoundId};

    let mut sim = live_sim(48);
    let log = SharedAudioLog::new();
    sim.ecs
        .0
        .insert_resource(crate::audio::AudioSinkRes::new(log.clone()));
    let _mega = sim
        .spawn_player_unit("mega", 1, 100.0, 100.0, 0.0)
        .expect("mega");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 110.0, 100.0, 0.0)
        .expect("dagger");
    let seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    assert!(
        sim.apply_payload_command(0, 0.0, 0.0, Some(seq)),
        "the dagger is picked up"
    );
    let events = log.events();
    assert!(
        events.iter().any(|ticked| matches!(
            &ticked.event,
            AudioEvent::At { sound, pitch, .. }
                if *sound == SoundId::PAYLOAD_PICKUP && (0.9..=1.1).contains(pitch)
        )),
        "pickup sound: {events:?}"
    );

    log.clear();
    assert!(
        sim.apply_payload_command(2, 100.0, 100.0, None),
        "the payload is dropped"
    );
    let events = log.events();
    // Dagger hitSize 8 <= 12 -> payloadDrop1.
    assert!(
        events.iter().any(|ticked| matches!(
            &ticked.event,
            AudioEvent::At { sound, .. } if *sound == SoundId::PAYLOAD_DROP1
        )),
        "drop sound: {events:?}"
    );
}

/// G06 T7: unit `@x/@y` are tiles (px/8) and `controlled` uses the Java
/// constants (`ctrlProcessor = 1`, `ctrlPlayer = 2`, `ctrlCommand = 3`).
#[test]
fn unit_xy_tiles_and_controlled() {
    let mut sim = live_sim(16);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 80.0, 160.0, 0.0)
        .expect("dagger");
    use crate::logic::LAccess;
    use crate::logic::access::{Sensed, sense};
    use crate::logic::value::LogicObject;
    let target = LogicObject::Unit(dagger);
    assert_eq!(sense(&sim.ecs.0, &target, LAccess::X), Sensed::Num(10.0));
    assert_eq!(sense(&sim.ecs.0, &target, LAccess::Y), Sensed::Num(20.0));
    assert_eq!(
        sense(&sim.ecs.0, &target, LAccess::Controlled),
        Sensed::Num(0.0)
    );
    // Velocity is tiles/sec (`vel * 60 / 8`).
    let vel = sense(&sim.ecs.0, &target, LAccess::VelocityY);
    let Sensed::Num(vy) = vel else {
        panic!("velocity is numeric");
    };
    assert!((vy - 0.0).abs() < 1e-4, "stationary dagger: {vy}");
}

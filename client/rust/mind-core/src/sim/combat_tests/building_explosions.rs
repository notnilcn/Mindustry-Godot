// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Building destruction and explosion-term regressions.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{health, live_sim, place_at, tick_n};

/// H09 T4: `Building.update()` skips `updateTile` for a disabled turret
/// (`Turret` has the default `noUpdateDisabled = false`): a disabled, fed duo
/// never fires at an enemy in range.
#[test]
fn disabled_duo_does_not_fire() {
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place_at(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let enemy = sim
        .spawn_live_unit("dagger", 2, 20.0 * 8.0 + 60.0, 20.0 * 8.0 + 20.0, 180.0)
        .expect("enemy dagger");
    let start = health(&sim, enemy).expect("health");
    if let Some(mut building) = sim.ecs.0.get_mut::<crate::entities::comp::Building>(duo) {
        building.enabled = false;
    }
    tick_n(&mut sim, 120);
    let shots = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .map(|state| state.total_shots)
        .unwrap_or(-1);
    assert_eq!(shots, 0, "a disabled duo never fires");
    assert_eq!(
        health(&sim, enemy),
        Some(start),
        "the enemy takes no duo damage"
    );
}

/// H09 T7: a container full of 100 blast-compound explodes with
/// `explosiveness = 120` and `flammability = 40` (before the
/// `* 3.5 * explosivenessScale` caller scaling).
#[test]
fn blast_container_explosion_terms() {
    let mut sim = live_sim(32);
    let container = place_at(&mut sim, 5, 5, "container", 1);
    let (content_len, blast, capacity) = {
        let content = &sim.unit_runtime().expect("rt").content;
        (
            content.items().len(),
            content.item_id("blast-compound").expect("blast-compound"),
            content
                .block_by_name("container")
                .map(|def| def.item_capacity)
                .unwrap_or(300),
        )
    };
    let mut module = crate::world::modules::ItemModule::with_items(content_len);
    module.add(blast, 100, capacity.max(100));
    sim.ecs.0.entity_mut(container).insert(module);

    let content = &sim.unit_runtime().expect("rt").content;
    let def = content.block_by_name("container").expect("def").clone();
    let (flammability, explosiveness, power) =
        crate::sim::unit_runtime::building_explosion_terms(&sim.ecs.0, content, container, &def);
    assert!(
        (explosiveness - 120.0).abs() < 1e-3,
        "explosiveness {explosiveness}"
    );
    assert!(
        (flammability - 40.0).abs() < 1e-3,
        "flammability {flammability}"
    );
    assert_eq!(power, 0.0, "no charge items");
    // Caller scaling (`BuildingComp.java:1518`).
    assert!((explosiveness * 3.5 * def.explosiveness_scale - 420.0).abs() < 1e-3);
}

/// H09 T7: a vault linked to a core caps its item explosion at
/// `min(itemCapacity / 60, 6)` (`StorageBlock.java:99-103`).
#[test]
fn linked_vault_explodes_weakly() {
    let mut sim = live_sim(32);
    place_at(&mut sim, 5, 5, "core-shard", 1);
    let vault = place_at(&mut sim, 8, 5, "vault", 1);
    assert!(
        sim.ecs
            .0
            .get::<crate::world::blocks::storage::storage_block::StorageBuild>(vault)
            .and_then(|storage| storage.linked_core)
            .is_some(),
        "the vault links to the adjacent core"
    );
    let (content_len, blast, capacity) = {
        let content = &sim.unit_runtime().expect("rt").content;
        (
            content.items().len(),
            content.item_id("blast-compound").expect("blast-compound"),
            content
                .block_by_name("vault")
                .map(|def| def.item_capacity)
                .unwrap_or(1000),
        )
    };
    let mut module = crate::world::modules::ItemModule::with_items(content_len);
    module.add(blast, 1000, capacity);
    sim.ecs.0.entity_mut(vault).insert(module);

    let content = &sim.unit_runtime().expect("rt").content;
    let def = content.block_by_name("vault").expect("def").clone();
    let (_, explosiveness, _) =
        crate::sim::unit_runtime::building_explosion_terms(&sim.ecs.0, content, vault, &def);
    assert!(
        (explosiveness - 1.2 * 6.0).abs() < 1e-3,
        "capped explosiveness (6 items) {explosiveness}"
    );
}

/// H09 T7: a charged battery's `power` term is `status * capacity` (4000) and
/// `dynamic_explosion` schedules `clamp(4000/700, 0, 8) = 6` lightning bolts.
#[test]
fn battery_lightning_count() {
    use crate::combat::damage::explosion::{
        DelayedAction, DelayedExplosionActions, dynamic_explosion,
    };

    let mut sim = live_sim(32);
    let battery = place_at(&mut sim, 5, 5, "battery", 1);
    if let Some(mut power) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::PowerModule>(battery)
    {
        power.status = 1.0;
    }
    let content = &sim.unit_runtime().expect("rt").content;
    let def = content.block_by_name("battery").expect("def").clone();
    assert_eq!(def.base_explosiveness, 1.0, "battery baseExplosiveness");
    let (_, explosiveness, power) =
        crate::sim::unit_runtime::building_explosion_terms(&sim.ecs.0, content, battery, &def);
    assert_eq!(explosiveness, 1.0);
    assert!(
        (power - 4000.0).abs() < 1e-3,
        "battery buffered capacity 4000: {power}"
    );
    sim.with_combat_ctx(|ctx| {
        dynamic_explosion(
            ctx,
            40.0,
            40.0,
            0.0,
            explosiveness,
            power,
            16.0,
            true,
            false,
            Some(1),
        );
    });
    let actions = sim
        .ecs
        .0
        .get_resource::<DelayedExplosionActions>()
        .expect("actions");
    let bolts = actions
        .0
        .iter()
        .filter(|(_, action)| matches!(action, DelayedAction::Lightning { .. }))
        .count();
    assert_eq!(bolts, 6, "4000 / 700 = 5.71 -> 6 bolts");
    assert_eq!(actions.0.len(), 6, "no fireballs or waves");
}

/// H09 T7: explosiveness 33 splits into 3 waves spaced 2 ticks apart
/// (`Time.run(i * 2f)`).
#[test]
fn explosion_waves_are_staggered() {
    use crate::combat::damage::explosion::dynamic_explosion;

    let mut sim = live_sim(48);
    let wall = place_at(&mut sim, 20, 20, "copper-wall", 1);
    let start = health(&sim, wall).expect("wall health");
    let (x, y) = (20.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0);
    sim.with_combat_ctx(|ctx| {
        // Team 2 is protected; the team-1 wall takes every wave.
        dynamic_explosion(ctx, x, y, 0.0, 33.0, 0.0, 24.0, true, false, Some(2));
    });
    let after_inline = health(&sim, wall).expect("wall health");
    assert!(after_inline < start, "wave 0 damages inline");
    let mut drops = 1;
    let mut previous = after_inline;
    for _ in 0..8 {
        sim.tick().expect("tick");
        let now = health(&sim, wall).expect("wall health");
        if now < previous - 1e-3 {
            drops += 1;
        }
        previous = now;
    }
    assert_eq!(drops, 3, "three staggered waves (+0/+2/+4)");
}

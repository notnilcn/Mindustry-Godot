// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime heat and attribute-crafter regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;
use crate::sim::test_support::{item, liquid, place_team1, runtime_sim, stored_item, tick_n};

/// L06 stretch (PAR-0437): the unit repair tower heals damaged friendly
/// units in range by `healAmount * edelta()` (RepairTower.java:57-86;
/// unit-repair-tower range 100 / healAmount 1.5, Blocks.java:6612-6621), and
/// a heal-suppressed tower (`checkSuppression`) heals nothing.
#[test]
fn repair_tower_heals_units_and_respects_suppression() {
    use crate::entities::comp::Health;
    use crate::world::modules::LiquidModule;

    let mut sim = runtime_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let tower = place_team1(&mut sim, "unit-repair-tower", 10, 10);
    // Size 2 covers tiles 10..11 (`offset`), so the source sits at x = 9.
    let _source = place_team1(&mut sim, "power-source", 9, 10);
    let (dagger, ozone) = {
        let content = sim.block_runtime().expect("runtime").content();
        (
            content.unit_id("dagger").expect("dagger").raw(),
            content.liquid_id("ozone").expect("ozone"),
        )
    };
    sim.ecs
        .0
        .get_mut::<LiquidModule>(tower)
        .expect("liquids")
        .add(ozone, 30.0, 30.0);
    sim.command(SimCommand::SpawnUnit {
        unit: dagger,
        x: 10.5 * 8.0,
        y: 14.0 * 8.0,
        team: 1,
    })
    .expect("spawn");
    let unit = *sim
        .unit_runtime()
        .expect("runtime")
        .units
        .last()
        .expect("spawned");
    let max = sim.ecs.0.get::<Health>(unit).expect("health").max_health;
    sim.ecs.0.get_mut::<Health>(unit).expect("health").health = max - 60.0;
    tick_n(&mut sim, 20);
    let healed = sim.ecs.0.get::<Health>(unit).expect("health").health;
    assert!(
        healed > max - 60.0 + 10.0,
        "the tower heals (1.5/tick after the 6-tick refresh): {healed} of {max}"
    );

    // Heal suppression from an enemy bullet (`applyHealSuppression`).
    sim.ecs.0.get_mut::<Health>(unit).expect("health").health = max - 60.0;
    let (x, y) = (10.0 * 8.0 + 4.0, 10.0 * 8.0 + 4.0);
    crate::world::blocks::defense::projectors::apply_suppression(
        &mut sim.ecs.0,
        2,
        x,
        y,
        40.0,
        600.0,
    );
    tick_n(&mut sim, 20);
    assert_eq!(
        sim.ecs.0.get::<Health>(unit).expect("health").health,
        max - 60.0,
        "a suppressed tower heals nothing"
    );
    let _ = tower;
}

/// GAPS #18 (`AttributeCrafterBuild.efficiencyMultiplier`,
/// `AttributeCrafter.java:110`): a cultivator on spore-moss floors gets
/// `baseEfficiency + min(maxBoost, boostScale * attrsum)` = `1 + 1.2` progress,
/// and an attribute-free control stays at 1x.
#[test]
fn attribute_crafter_boosts_progress_from_footprint_attributes() {
    let progress = |sim: &Sim, e: Entity| {
        sim.ecs
            .0
            .get::<crate::entities::comp::CrafterState>(e)
            .map(|state| state.progress)
            .unwrap_or(0.0)
    };
    let progress_per_tick = |sim: &mut Sim, e: Entity| {
        sim.tick().expect("tick");
        let before = progress(sim, e);
        sim.tick().expect("tick");
        let after = progress(sim, e);
        if after >= before {
            after - before
        } else {
            after + 1.0 - before
        }
    };

    let mut sim = runtime_sim();
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("rules")
        .cheat = true;
    // Spore-moss carries `Attribute.spores = 0.3`; a size-2 cultivator covers
    // four tiles -> attrsum 1.2 (`Block.sumAttribute`).
    for x in 4..=7 {
        for y in 4..=7 {
            set_floor(&mut sim, x, y, "spore-moss");
        }
    }
    let boosted = place_team1(&mut sim, "cultivator", 5, 5);
    let attr = sim
        .ecs
        .0
        .get::<crate::world::behavior::production::AttributeCrafterState>(boosted)
        .expect("attribute crafter state")
        .attr_sum;
    assert!(
        (attr - 1.2).abs() < 1e-5,
        "attrsum over 4 spore-moss: {attr}"
    );
    let inc = progress_per_tick(&mut sim, boosted);
    assert!(
        (inc - 0.022).abs() < 1e-4,
        "progress = 1/craftTime(100) * (1 + min(2, 1 * 1.2)), got {inc}"
    );

    let mut plain = runtime_sim();
    plain
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("rules")
        .cheat = true;
    let control = place_team1(&mut plain, "cultivator", 5, 5);
    let attr = plain
        .ecs
        .0
        .get::<crate::world::behavior::production::AttributeCrafterState>(control)
        .expect("attribute crafter state")
        .attr_sum;
    assert_eq!(attr, 0.0, "stone floors have no spores attribute");
    let inc = progress_per_tick(&mut plain, control);
    assert!(
        (inc - 0.01).abs() < 1e-4,
        "unboosted progress = 1/100, got {inc}"
    );
}

/// GAPS #19 (`FrackerBuild.updateTile`, `Fracker.java:30-48`): the oil
/// extractor consumes one sand per `itemUseTime` (60) ticks and resets
/// `lastPump` when the result buffer is full.
#[test]
fn fracker_consumes_items_on_its_use_timer_and_resets_last_pump() {
    let mut sim = runtime_sim();
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("rules")
        .cheat = true;
    // `Attribute.oil` floors satisfy `SolidPump.canPlaceOn` (`baseEfficiency`
    // 0); darksand carries oil 1.5 per tile.
    for x in 4..=8 {
        for y in 4..=8 {
            set_floor(&mut sim, x, y, "darksand");
        }
    }
    let fracker = place_team1(&mut sim, "oil-extractor", 5, 5);
    let sand = item(&sim, "sand");
    sim.ecs
        .0
        .get_mut::<crate::world::modules::ItemModule>(fracker)
        .expect("items")
        .add(sand, 10, 100);

    tick_n(&mut sim, 60);
    assert_eq!(
        stored_item(&sim, fracker, sand),
        10,
        "no consume() before itemUseTime"
    );
    sim.tick().expect("tick");
    assert_eq!(
        stored_item(&sim, fracker, sand),
        9,
        "consume() on the 60-tick timer"
    );
    let state = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::world::behavior::production::SolidPumpState>(fracker)
            .copied()
            .expect("solid pump state")
    };
    let pumping = state(&sim);
    assert!(pumping.last_pump > 0.0, "lastPump while pumping");
    assert!(
        pumping.accumulator >= 0.99,
        "accumulator: {}",
        pumping.accumulator
    );

    // Fill the oil buffer: `shouldConsume()` false -> efficiency 0 -> the idle
    // branch resets `lastPump` (`Fracker.java:42-46`) but keeps the accumulator.
    let oil = liquid(&sim, "oil");
    sim.ecs
        .0
        .get_mut::<crate::world::modules::LiquidModule>(fracker)
        .expect("liquids")
        .add(oil, 40.0, 40.0);
    tick_n(&mut sim, 3);
    let idle = state(&sim);
    assert_eq!(idle.last_pump, 0.0, "lastPump reset while idle");
    assert!(
        idle.accumulator >= pumping.accumulator - 0.01,
        "accumulator is not reset on idle"
    );
}

/// GAPS #18 (`AttributeCrafterBuild.outputMultiplier`/`scaleOutput`,
/// `AttributeCrafter.java:106,119-121`): silicon-crucible's `outputScale =
/// 0.15` doubles its 8-silicon craft on a 3x3 heat floor (attrsum 6.75 ->
/// `1 + min(1, 0.15 * 6.75)`), while its speed boost uses `boostScale` too.
#[test]
fn silicon_crucible_output_scales_with_heat_attribute() {
    let mut sim = runtime_sim();
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("rules")
        .cheat = true;
    // `magmarock` carries `Attribute.heat = 0.75`; a size-3 crucible covers
    // nine tiles -> attrsum 6.75.
    for x in 3..=7 {
        for y in 3..=7 {
            set_floor(&mut sim, x, y, "magmarock");
        }
    }
    let crucible = place_team1(&mut sim, "silicon-crucible", 5, 5);
    let attr = sim
        .ecs
        .0
        .get::<crate::world::behavior::production::AttributeCrafterState>(crucible)
        .expect("attribute crafter state")
        .attr_sum;
    assert!(
        (attr - 6.75).abs() < 1e-4,
        "attrsum over 9 magmarock: {attr}"
    );
    for name in ["coal", "sand", "pyratite"] {
        let id = item(&sim, name);
        sim.ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(crucible)
            .expect("items")
            .add(id, 100, 1000);
    }
    // Speed multiplier 2: the first craft lands at ~45 ticks (craftTime 90);
    // at the unboosted 1x rate 50 ticks would still be 0.55 progress.
    tick_n(&mut sim, 50);
    let silicon = stored_item(&sim, crucible, item(&sim, "silicon"));
    // 8 output * outputMultiplier(2) = 16 for the first craft.
    assert!(
        silicon >= 16,
        "heated silicon-crucible should output 16 per craft, got {silicon}"
    );
}

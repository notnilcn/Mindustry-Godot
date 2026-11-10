// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime wall-crafter and cliff-crusher regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;

/// PAR-0079: a plasma bore mines the Erekir wall ore in front of it
/// (`BeamDrill.updateFacing` + `canPlaceOn` scan the facing edge, not the
/// footprint).
#[test]
fn plasma_bore_mines_wall_ore() {
    let mut sim = runtime_sim();
    // `ore-wall-beryllium` two tiles in front of the facing edge (rotation 0);
    // `BeamDrill.canPlaceOn` requires it before the block can be placed.
    let ore =
        crate::content::BlockId::new(sim.world_block_id("ore-wall-beryllium").expect("ore wall"));
    // Java maps put `ore-wall-*` as the *overlay* of a static wall
    // (`Tile.wallDrop()`), never as the block itself.
    let wall = crate::content::BlockId::new(
        sim.world_block_id("beryllic-stone-wall")
            .expect("static wall"),
    );
    for y in [5, 6] {
        let tile = sim.grid.tiles.get_mut(9, y);
        tile.block = wall;
        tile.overlay = ore;
    }
    let _source = place(&mut sim, "power-source", 4, 5);
    let bore = place(&mut sim, "plasma-bore", 5, 5);
    {
        let crate::sim::Sim {
            grid,
            ecs,
            block_runtime,
            ..
        } = &mut sim;
        let content = block_runtime.as_ref().expect("runtime").content();
        crate::world::behavior::production::refresh_drill_ore(&mut ecs.0, grid, content, bore);
    }
    let state = sim
        .ecs
        .0
        .get::<crate::entities::comp::DrillState>(bore)
        .expect("drill state");
    assert_eq!(
        state.facing_amount, 2.0,
        "two facing edges see the wall ore"
    );

    let beryllium = item(&sim, "beryllium");
    for _ in 0..400 {
        sim.tick().expect("tick");
    }
    assert!(
        stored_item(&sim, bore, beryllium) > 0,
        "plasma bore produced beryllium"
    );
}

/// PAR-0079: a cliff crusher turns the sand attribute of the walls in front into
/// sand (`WallCrafter.getEfficiency`/`updateTile`).
#[test]
fn cliff_crusher_makes_sand_from_sand_walls() {
    let mut sim = runtime_sim();
    // `sand-wall` carries `Attribute.sand = 2` (Java Blocks.java);
    // `WallCrafter.canPlaceOn` requires it before the block can be placed.
    let wall = crate::content::BlockId::new(sim.world_block_id("sand-wall").expect("sand-wall"));
    sim.grid.tiles.get_mut(7, 5).block = wall;
    sim.grid.tiles.get_mut(7, 6).block = wall;
    let _source = place(&mut sim, "power-source", 4, 5);
    let crusher = place(&mut sim, "cliff-crusher", 5, 5);
    {
        let crate::sim::Sim {
            grid,
            ecs,
            block_runtime,
            ..
        } = &mut sim;
        let content = block_runtime.as_ref().expect("runtime").content();
        crate::world::behavior::production::refresh_drill_ore(&mut ecs.0, grid, content, crusher);
    }
    let eff = sim
        .ecs
        .0
        .get::<crate::entities::comp::DrillState>(crusher)
        .map(|state| state.facing_amount)
        .unwrap_or(0.0);
    assert_eq!(eff, 4.0, "two facing edges * sand 2");

    let sand = item(&sim, "sand");
    for _ in 0..40 {
        sim.tick().expect("tick");
    }
    for _ in 0..400 {
        sim.tick().expect("tick");
    }
    assert!(
        stored_item(&sim, crusher, sand) > 0,
        "cliff crusher produced sand"
    );
}

/// PAR-0474 / `WallCrafter.getEfficiency` (`WallCrafter.java:141-181`): the
/// attribute sum runs over the solid tiles of the facing row only, so a
/// sand-wall (2) plus a stone-wall (1) yields 3. `updateTile`
/// (`WallCrafter.java:187-223`) turns that into `lastEfficiency`
/// (`eff * timeScale * efficiency`) and `edelta() * eff` drill progress.
#[test]
fn wall_crafter_sums_facing_attributes_and_tracks_last_efficiency() {
    let mut sim = runtime_sim();
    // Facing row of the rotation-0 size-2 crusher at (5,5): (7,5) and (7,6).
    set_block(&mut sim, 7, 5, "sand-wall");
    set_block(&mut sim, 7, 6, "stone-wall");
    let _source = place(&mut sim, "power-source", 4, 5);
    let crusher = place(&mut sim, "cliff-crusher", 5, 5);
    refresh_wall_crafter(&mut sim, crusher);
    assert_eq!(
        wall_crafter_state(&sim, crusher).facing_amount,
        3.0,
        "sand-wall 2 + stone-wall 1"
    );

    // Wait for the power graph; then measure one full-efficiency tick.
    tick_until_efficient(&mut sim, crusher);
    let before = wall_crafter_state(&sim, crusher);
    let efficiency = building_efficiency(&sim, crusher);
    sim.tick().expect("tick");
    let after = wall_crafter_state(&sim, crusher);
    let efficiency_after = building_efficiency(&sim, crusher);
    // `timeScale` is 1 (no overdrive), so `edelta() == efficiency`.
    assert!(
        (after.last_efficiency - 3.0 * efficiency_after).abs() < 1e-4,
        "lastEfficiency = eff * timeScale * efficiency, got {}",
        after.last_efficiency
    );
    assert!(
        (after.progress - before.progress - efficiency * 3.0).abs() < 1e-4,
        "time += edelta() * eff, got delta {}",
        after.progress - before.progress
    );
    assert!(
        (after.warmup - (before.warmup + 1.0 / 40.0).min(1.0)).abs() < 1e-5,
        "approachDelta(warmup, 1, 1/40), got {}",
        after.warmup
    );
    assert!(
        (after.total_time - before.total_time - efficiency_after * after.warmup).abs() < 1e-4,
        "totalTime += edelta() * warmup, got delta {}",
        after.total_time - before.total_time
    );
}

/// PAR-0474 / `WallCrafter.updateTile` liquid boost
/// (`WallCrafter.java:205-208`): no vanilla wall crafter declares a booster
/// liquid consumer (`large-cliff-crusher`'s hydrogen consumer is required, not
/// `.boost()`ed), so the test patches one onto `cliff-crusher`:
/// `eff * lerp(1, liquidBoostIntensity, optionalEfficiency)` with
/// `liquidBoostIntensity = 1.6`.
#[test]
fn wall_crafter_liquid_booster_scales_efficiency() {
    let mut content = crate::sim::runtime::load_vanilla_content().expect("vanilla content");
    let water = content.liquid_id("water").expect("water");
    let crusher_id = content.block_id("cliff-crusher").expect("cliff-crusher");
    let air = content.block_id("air").expect("air");
    {
        // `consumeLiquid(Liquids.water, 6f / 60f).boost()`: an optional (and
        // therefore booster, in the port) liquid consumer.
        let def = content.block_mut(crusher_id).expect("crusher def");
        def.consumes.push(crate::content::ConsumeSpec {
            consume: crate::content::Consume::Liquid {
                liquid: water,
                amount: 6.0 / 60.0,
            },
            optional: true,
            update: true,
            ignore: false,
        });
        def.has_liquids = true;
        def.liquid_capacity = 60.0;
    }
    {
        // A non-solid block with the same attribute must not contribute.
        let def = content.block_mut(air).expect("air def");
        def.attributes = vec![(String::from("sand"), 5.0)];
    }
    let mut sim = runtime_sim_with_content(content);
    set_block(&mut sim, 7, 5, "sand-wall");
    sim.grid.tiles.get_mut(7, 6).block = BlockId::AIR;
    let _source = place(&mut sim, "power-source", 4, 5);
    let crusher = place(&mut sim, "cliff-crusher", 5, 5);
    let has_booster = sim
        .ecs
        .0
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.get(crusher_block_id(&sim, "cliff-crusher")))
        .is_some_and(|instance| match &instance.kind_data {
            crate::world::BlockKindData::WallCrafter(wall) => wall.has_liquid_booster,
            _ => false,
        });
    assert!(
        has_booster,
        "`WallCrafter.init()` finds the booster liquid consumer"
    );
    // Fill the liquid module (`consumeLiquid` has no item trigger; the tank
    // starts empty).
    {
        let mut liquids = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::LiquidModule>(crusher)
            .expect("liquid module");
        liquids.add(water, 60.0, 60.0);
    }
    refresh_wall_crafter(&mut sim, crusher);
    assert_eq!(
        wall_crafter_state(&sim, crusher).facing_amount,
        2.0,
        "only the solid sand-wall contributes (air is not solid)"
    );
    tick_until_efficient(&mut sim, crusher);
    let efficiency = building_efficiency(&sim, crusher);
    let state = wall_crafter_state(&sim, crusher);
    assert!(
        (state.last_efficiency - 2.0 * 1.6 * efficiency).abs() < 1e-3,
        "liquid boost is 1.6x, got {} (eff {efficiency})",
        state.last_efficiency
    );
}

/// PAR-0474 / `Blocks.java:3049-3051` (`large-cliff-crusher`):
/// `itemConsumer = consumeItem(Items.graphite).boost()` with
/// `boostItemUseTime = 60f / 0.75f`. While graphite is stocked,
/// `itemValid` multiplies the drill efficiency by `itemBoostIntensity` (1.6) and
/// `timer(timerUse, boostItemUseTime / timeScale)` calls `consume()`, removing
/// one graphite.
#[test]
fn large_cliff_crusher_consumes_graphite_for_the_item_boost() {
    let mut sim = runtime_sim();
    // Size-3 crusher at (5,5), rotation 0: facing row (7,4), (7,5), (7,6).
    for (x, y) in [(7, 4), (7, 5), (7, 6)] {
        set_block(&mut sim, x, y, "sand-wall");
    }
    let _source = place(&mut sim, "power-source", 3, 5);
    let crusher = place(&mut sim, "large-cliff-crusher", 5, 5);
    let hydrogen = liquid(&sim, "hydrogen");
    let graphite = item(&sim, "graphite");
    // `consumeLiquid(hydrogen, 1f / 60f)` is a *required* consumer, so the
    // building only runs with hydrogen in its tank.
    {
        let mut liquids = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::LiquidModule>(crusher)
            .expect("liquid module");
        liquids.add(hydrogen, 600.0, 600.0);
    }
    {
        let mut items = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(crusher)
            .expect("item module");
        items.add(graphite, 1, 20);
    }
    refresh_wall_crafter(&mut sim, crusher);
    assert_eq!(
        wall_crafter_state(&sim, crusher).facing_amount,
        6.0,
        "three facing edges * sand 2"
    );
    // The first tick with a live power graph applies the item boost and
    // `consume()`s the graphite.
    tick_until_efficient(&mut sim, crusher);
    assert_eq!(
        stored_item(&sim, crusher, graphite),
        0,
        "`consume()` removed the graphite on the first boost tick"
    );
    let efficiency = building_efficiency(&sim, crusher);
    let boosted = wall_crafter_state(&sim, crusher);
    assert!(
        (boosted.last_efficiency - 6.0 * 1.6 * efficiency).abs() < 1e-3,
        "item boost is 1.6x, got {}",
        boosted.last_efficiency
    );

    // No graphite left => `itemValid` false => base efficiency again, and the
    // timer must not consume anything else.
    sim.tick().expect("tick");
    let efficiency = building_efficiency(&sim, crusher);
    let plain = wall_crafter_state(&sim, crusher);
    assert!(
        (plain.last_efficiency - 6.0 * efficiency).abs() < 1e-3,
        "no item boost without graphite, got {}",
        plain.last_efficiency
    );
    assert_eq!(stored_item(&sim, crusher, graphite), 0);
}

/// PAR-0474: without booster consumers (`cliff-crusher` declares none) the
/// efficiency stays at the raw attribute sum even when items sit in the block
/// — `itemValid` is `itemConsumer != null && ...`, so the item boost cannot
/// fire, and no item is consumed.
#[test]
fn wall_crafter_without_boosters_gets_no_boost() {
    let mut sim = runtime_sim();
    set_block(&mut sim, 7, 5, "sand-wall");
    set_block(&mut sim, 7, 6, "sand-wall");
    let _source = place(&mut sim, "power-source", 4, 5);
    let crusher = place(&mut sim, "cliff-crusher", 5, 5);
    let graphite = item(&sim, "graphite");
    {
        let mut items = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(crusher)
            .expect("item module");
        items.add(graphite, 4, 10);
    }
    refresh_wall_crafter(&mut sim, crusher);
    tick_until_efficient(&mut sim, crusher);
    sim.tick().expect("tick");
    let efficiency = building_efficiency(&sim, crusher);
    let state = wall_crafter_state(&sim, crusher);
    assert!(
        (state.last_efficiency - 4.0 * efficiency).abs() < 1e-4,
        "no item consumer => no 1.6x boost, got {}",
        state.last_efficiency
    );
    assert_eq!(
        stored_item(&sim, crusher, graphite),
        4,
        "nothing consumes the graphite"
    );
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Landing-pad import queue (`world/blocks/campaign/LandingPad.java`).
//!
//! `updateTimers` (`LandingPad.java:165-201`) advances the played sector's
//! per-item import cooldown timers from the import rates and lands the
//! highest-priority waiting pad per item (`Call.landingPadLanded` ->
//! `handleLanding`). The per-pad half (water draw, arrival, dump) lives in
//! `world::behavior::campaign::LandingPadBehavior`.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::PlanetId;
use crate::entities::comp::LandingPadState;
use crate::game::runtime::CampaignRuntime;
use crate::sim::runtime::ContentIndex;
use crate::world::blocks::distribution::transfer;

/// `LandingPad.updateTimers` + the `waiting` queue (`LandingPad.java:165-201`):
/// campaign-only, default team, `!legacyLaunchPads`.
pub fn landing_pads_system(world: &mut World) {
    let Some(mut runtime) = world.remove_resource::<CampaignRuntime>() else {
        return;
    };
    let Some(played) = runtime.session.sector else {
        world.insert_resource(runtime);
        return;
    };
    if !runtime.session.is_campaign() {
        world.insert_resource(runtime);
        return;
    }
    let legacy = runtime
        .campaign
        .planet(played.0)
        .is_some_and(|planet| planet.campaign_rules.legacy_launch_pads);
    if legacy {
        world.insert_resource(runtime);
        return;
    }
    let item_names: Vec<String> = world
        .get_resource::<ContentIndex>()
        .map(|index| index.item_names.clone())
        .unwrap_or_default();

    // `itemCapacity` for the rate math; every landing pad shares it (100).
    let capacity = {
        let mut query = world.query::<(Entity, &LandingPadState)>();
        query
            .iter(world)
            .next()
            .map(|(entity, _)| transfer::item_capacity(world, entity))
    };

    // (a) `float[] imports = info.getImportRates(planet)`; per item
    // `timer += 1 / (itemCapacity / (rate / 60)) * delta`, clamped to 1; a zero
    // rate resets the timer (`LandingPad.java:168-182`).
    if let Some(capacity) = capacity.filter(|capacity| *capacity > 0)
        && let Some(sector) = runtime.campaign.sector_mut(played.0, played.1)
    {
        let rates: Vec<f32> = sector.info.get_import_rates().to_vec();
        for (index, rate) in rates.iter().enumerate() {
            let Some(name) = item_names.get(index) else {
                continue;
            };
            let per_frame = rate / 60.0;
            if per_frame > 0.0 {
                let frames_between = capacity as f32 / per_frame;
                let slot = sector
                    .info
                    .info
                    .import_cooldown_timers
                    .entry(name.clone())
                    .or_insert(0.0);
                *slot = (*slot + (1.0 / frames_between.max(0.0001))).min(1.0);
            } else {
                sector
                    .info
                    .info
                    .import_cooldown_timers
                    .insert(name.clone(), 0.0);
            }
        }
    }

    // (b) group the waiting pads by config item, land the lowest priority
    // (`pads.first()` after `pads.sort(p -> p.priority)`), swap its priority
    // with the last, clear the lists (`LandingPad.java:184-201`).
    let mut waiting: std::collections::BTreeMap<u16, Vec<(i32, Entity)>> =
        std::collections::BTreeMap::new();
    {
        let mut query = world.query::<(Entity, &LandingPadState)>();
        for (entity, state) in query.iter(world) {
            if state.wants_landing
                && let Some(config) = state.config
            {
                waiting
                    .entry(config.raw())
                    .or_default()
                    .push((state.priority, entity));
            }
        }
    }
    let mut to_land: Vec<Entity> = Vec::new();
    for pads in waiting.values_mut() {
        if pads.is_empty() {
            continue;
        }
        pads.sort_by_key(|(priority, _)| *priority);
        let first = pads[0];
        let last = *pads.last().unwrap_or(&first);
        to_land.push(first.1);
        if first.1 != last.1 {
            if let Some(mut state) = world.get_mut::<LandingPadState>(first.1) {
                state.priority = last.0;
            }
            if let Some(mut state) = world.get_mut::<LandingPadState>(last.1) {
                state.priority = first.0;
            }
        }
    }
    for entity in to_land {
        handle_landing(world, &mut runtime, entity, played, &item_names);
    }
    // Every pad recomputes `wants_landing` on its next `updateTile`.
    {
        let mut query = world.query::<&mut LandingPadState>();
        for mut state in query.iter_mut(world) {
            state.wants_landing = false;
        }
    }
    world.insert_resource(runtime);
}

/// `LandingPadBuild.handleLanding` (`LandingPad.java:146-158`).
fn handle_landing(
    world: &mut World,
    runtime: &mut CampaignRuntime,
    entity: Entity,
    played: (PlanetId, u16),
    item_names: &[String],
) {
    let Some(config) = world
        .get::<LandingPadState>(entity)
        .and_then(|state| state.config)
    else {
        return;
    };
    if let Some(mut state) = world.get_mut::<LandingPadState>(entity) {
        state.cooldown = 1.0;
        state.arriving = Some(config);
        state.arriving_timer = 0.0;
        state.liquid_removed = 0.0;
    }
    if let Some(name) = item_names.get(config.index())
        && let Some(sector) = runtime.campaign.sector_mut(played.0, played.1)
    {
        sector
            .info
            .info
            .import_cooldown_timers
            .insert(name.clone(), 0.0);
    }
}

/// One landing-pad row `(id, x, y, block, config, arriving, arriving_timer,
/// cooldown)`; H03 draws pods and the cooldown bar from it.
pub type LandingPadRow = (
    u32,
    f32,
    f32,
    crate::content::BlockId,
    Option<u16>,
    Option<u16>,
    f32,
    f32,
);

/// Snapshot of the landing pads; sorted in entity order.
pub fn landing_pad_rows(world: &mut World) -> Vec<LandingPadRow> {
    let mut rows: Vec<LandingPadRow> = Vec::new();
    let mut query = world.query::<(
        Entity,
        &LandingPadState,
        &crate::entities::comp::Pos,
        &crate::entities::comp::Building,
    )>();
    for (entity, state, pos, building) in query.iter(world) {
        rows.push((
            entity.index().index(),
            pos.x,
            pos.y,
            building.block,
            state.config.map(|item| item.raw()),
            state.arriving.map(|item| item.raw()),
            state.arriving_timer,
            state.cooldown,
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::universe::Campaign;
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use crate::world::modules::{ItemModule, LiquidModule};
    use bevy_ecs::world::World as EcsWorld;

    /// A placed `landing-pad` with its behavior state.
    fn pad(
        content: &crate::content::ContentRegistry,
        table: &BlockTable,
        team: u8,
    ) -> (
        EcsWorld,
        std::sync::Arc<crate::world::block::BlockInstance>,
        Entity,
    ) {
        let inst = table.get_named("landing-pad").expect("landing-pad").clone();
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        let e = inst.spawn(
            &mut world,
            0,
            TilePos::new(4, 4),
            team,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        inst.behavior.create_state(&mut world, e);
        world.insert_resource(table.clone());
        (world, inst, e)
    }

    fn item_names(content: &crate::content::ContentRegistry) -> Vec<String> {
        content
            .items()
            .iter()
            .map(|item| item.name.clone())
            .collect()
    }

    #[test]
    fn landing_pad_lands_when_timer_full() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let (mut world, inst, e) = pad(&content, &table, 1);
        let copper = content.item_id("copper").expect("copper");
        let water = content.liquid_id("water").expect("water");
        inst.behavior.configured(
            &mut world,
            e,
            None,
            crate::world::config::ConfigValue::Item(copper),
        );
        if let Some(mut liquids) = world.get_mut::<LiquidModule>(e) {
            liquids.add(water, 3000.0, 3000.0);
        }
        // Campaign runtime: played sector with a 1.0/s copper import rate.
        let planet = content.planet_id("serpulo").expect("serpulo");
        let mut campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
        let played = content
            .sector_by_name("groundZero")
            .expect("groundZero")
            .sector;
        {
            let sector = campaign.sector_mut(planet, played).expect("sector");
            sector.save = Some("sector-serpulo-170".to_owned());
            sector.info.info.has_core = true;
            let copper_index = copper.index();
            sector.info.windows.import_rate_cache =
                vec![0.0; content.items().len().max(copper_index + 1)];
            sector.info.windows.import_rate_cache[copper_index] = 1.0;
            // One tick short of the gate (`1/6000` per tick at 1/s, cap 100).
            sector
                .info
                .info
                .import_cooldown_timers
                .insert("copper".to_owned(), 1.0 - 1.0 / 6000.0);
        }
        let mut session = PlaySession::new(Rules::default());
        session.sector = Some((planet, played));
        session.rules.default_team = 1;
        world.insert_resource(CampaignRuntime::new(session, campaign));
        let queues = crate::game::runtime::install_campaign_core_hooks(&mut world);
        {
            let mut state = queues.lock().expect("lock");
            state.is_campaign = true;
            state.default_team = 1;
        }
        world.insert_resource(ContentIndex {
            item_names: item_names(&content),
            ..ContentIndex::default()
        });
        // Pad tick, then the system advances the timer and lands the pad once
        // `importCooldownTimers` reaches 1.
        let mut landed = false;
        for _ in 0..5 {
            inst.behavior.update_tile(&mut world, e);
            landing_pads_system(&mut world);
            if world
                .get::<LandingPadState>(e)
                .is_some_and(|state| state.arriving.is_some())
            {
                landed = true;
                break;
            }
        }
        assert!(landed, "the pad starts an arrival");
        assert_eq!(
            world.get::<LandingPadState>(e).expect("state").arriving,
            Some(copper)
        );
        // Arrival: Java accumulates `Time.delta / arrivalDuration` in f32, so
        // 150 additions land a hair under 1.0 and the pod completes on tick
        // 151 (identical arithmetic).
        for _ in 0..160 {
            inst.behavior.update_tile(&mut world, e);
            if world
                .get::<LandingPadState>(e)
                .is_some_and(|state| state.arriving.is_none())
            {
                break;
            }
        }
        assert!(
            world
                .get::<LandingPadState>(e)
                .is_some_and(|state| state.arriving.is_none()),
            "arrival completes"
        );
        assert_eq!(
            world.get::<ItemModule>(e).expect("items").get(copper),
            100,
            "the pad stores a full pod"
        );
        assert_eq!(
            world.get::<LiquidModule>(e).expect("liquids").get(water),
            1500.0,
            "exactly consumeLiquidAmount is removed"
        );
        let state = queues.lock().expect("lock");
        assert!(
            state.imports.contains(&(copper, 100)),
            "handleItemImport queued: {:?}",
            state.imports
        );
    }

    #[test]
    fn fake_landing_pad_is_an_item_source() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        // Non-campaign, enemy-team pad: `isFake()` -> lands without imports.
        let (mut world, inst, e) = pad(&content, &table, 2);
        let copper = content.item_id("copper").expect("copper");
        let water = content.liquid_id("water").expect("water");
        inst.behavior.configured(
            &mut world,
            e,
            None,
            crate::world::config::ConfigValue::Item(copper),
        );
        if let Some(mut liquids) = world.get_mut::<LiquidModule>(e) {
            liquids.add(water, 3000.0, 3000.0);
        }
        inst.behavior.update_tile(&mut world, e);
        assert_eq!(
            world.get::<LandingPadState>(e).expect("state").arriving,
            Some(copper),
            "a fake pad lands immediately"
        );
        for _ in 0..160 {
            inst.behavior.update_tile(&mut world, e);
            if world
                .get::<LandingPadState>(e)
                .is_some_and(|state| state.arriving.is_none())
            {
                break;
            }
        }
        assert_eq!(world.get::<ItemModule>(e).expect("items").get(copper), 100);
        assert_eq!(
            world.get::<LiquidModule>(e).expect("liquids").get(water),
            1500.0
        );
    }
}

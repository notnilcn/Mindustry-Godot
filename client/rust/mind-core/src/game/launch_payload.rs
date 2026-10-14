// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Launch pods (`world/blocks/campaign/LaunchPad.java` `LaunchPayloadComp` and
//! `type/LaunchPayload`).
//!
//! A launched payload is a real ECS entity with a 120-tick lifetime
//! (`LaunchPad.java:143-161`); on removal it records the export on the played
//! sector and, with `legacyLaunchPads`, ships the items to the destination
//! record (`LaunchPayloadComp.remove`, `LaunchPad.java:290-311`). The draw side
//! is H03 (`launch_payloads_json`).

use bevy_ecs::prelude::{Component, Resource};
use bevy_ecs::world::World;

use crate::content::{BlockId, ItemId};
use crate::entities::comp::TeamComp;
use crate::game::runtime::CampaignRuntime;
use crate::sim::runtime::ContentIndex;

/// One in-flight launch pod (`LaunchPayloadComp` state + `LaunchPayload`).
#[derive(Debug, Clone, PartialEq, Component)]
pub struct LaunchPayloadState {
    /// `LaunchPayload.id()` (view draw seed).
    pub id: u32,
    /// Launched stacks in item-id order.
    pub stacks: Vec<(ItemId, i32)>,
    /// Ticks in flight (`time`).
    pub time: f32,
    /// Total flight time (`lifetime`, 120 ticks).
    pub lifetime: f32,
    /// Source pad block (draw region `<block>-pod`).
    pub block: BlockId,
}

/// Monotonic payload id (`LaunchPayload` entity id half; inserted lazily).
#[derive(Debug, Default, Resource)]
pub struct LaunchPayloadSeq(pub u32);

/// Next payload id (`wrapping_add`).
pub fn next_payload_id(world: &mut World) -> u32 {
    if let Some(mut seq) = world.get_resource_mut::<LaunchPayloadSeq>() {
        let id = seq.0;
        seq.0 = seq.0.wrapping_add(1);
        id
    } else {
        world.insert_resource(LaunchPayloadSeq(1));
        0
    }
}

/// Advances every payload by one tick; expired pods record their export and
/// despawn (`LaunchPayloadComp.update`/`remove`).
pub fn launch_payload_system(world: &mut World) {
    let Some(mut runtime) = world.remove_resource::<CampaignRuntime>() else {
        return;
    };
    let item_names: Vec<String> = world
        .get_resource::<ContentIndex>()
        .map(|index| index.item_names.clone())
        .unwrap_or_default();
    let mut entities: Vec<(u32, bevy_ecs::entity::Entity)> = Vec::new();
    {
        let mut query = world.query::<(bevy_ecs::entity::Entity, &LaunchPayloadState)>();
        for (entity, state) in query.iter(world) {
            entities.push((state.id, entity));
        }
    }
    entities.sort_by_key(|(id, _)| *id);
    for (_id, entity) in entities {
        let (stacks, time, lifetime) = match world.get_mut::<LaunchPayloadState>(entity) {
            Some(mut state) => {
                state.time += 1.0;
                (state.stacks.clone(), state.time, state.lifetime)
            }
            None => continue,
        };
        if time < lifetime {
            continue;
        }
        let team = world
            .get::<TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);
        // `LaunchPayloadComp.remove` (`LaunchPad.java:290-311`): campaign-only,
        // default-team, with a different destination sector.
        if let Some(played) = runtime.session.sector
            && runtime.session.is_campaign()
            && team == runtime.session.rules.default_team
            && let Some(destination) = runtime
                .campaign
                .sector(played.0, played.1)
                .and_then(|sector| sector.info.info.destination.clone())
            && let Some(planet_name) = destination.planet()
            && let Some(destination_sector) = destination.sector()
            && let Some(destination_planet) = runtime.campaign.planet_id_by_name(planet_name)
            && (destination_planet, destination_sector) != played
        {
            for (item, amount) in &stacks {
                let Some(name) = item_names.get(item.index()) else {
                    continue;
                };
                if let Some(sector) = runtime.campaign.sector_mut(played.0, played.1) {
                    sector.info.handle_item_export(name, *amount);
                }
            }
            if runtime
                .campaign
                .planet(played.0)
                .is_some_and(|planet| planet.campaign_rules.legacy_launch_pads)
            {
                let items: Vec<(String, i32)> = stacks
                    .iter()
                    .filter_map(|(item, amount)| {
                        item_names
                            .get(item.index())
                            .map(|name| (name.clone(), *amount))
                    })
                    .collect();
                if let Some(dest) = runtime
                    .campaign
                    .sector_mut(destination_planet, destination_sector)
                {
                    dest.add_items(items);
                }
                queue_info_save(&mut runtime, destination_planet, destination_sector);
            }
        }
        world.despawn(entity);
    }
    world.insert_resource(runtime);
}

/// Snapshot of in-flight payloads `(id, x, y, time, lifetime, block, team)`,
/// sorted by id (host seam for H03's draw).
pub fn payload_rows(world: &mut World) -> Vec<(u32, f32, f32, f32, f32, BlockId, u8)> {
    let mut rows: Vec<(u32, f32, f32, f32, f32, BlockId, u8)> = Vec::new();
    let mut query = world.query::<(
        bevy_ecs::entity::Entity,
        &LaunchPayloadState,
        &crate::entities::comp::Pos,
    )>();
    for (entity, state, pos) in query.iter(world) {
        let team = world
            .get::<TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);
        rows.push((
            state.id,
            pos.x,
            pos.y,
            state.time,
            state.lifetime,
            state.block,
            team,
        ));
    }
    rows.sort_by_key(|(id, ..)| *id);
    rows
}

/// Queues the destination record for a host-side `save_info` flush.
pub fn queue_info_save(
    runtime: &mut CampaignRuntime,
    planet: crate::content::PlanetId,
    sector: u16,
) {
    if !runtime.pending_info_saves.contains(&(planet, sector)) {
        runtime.pending_info_saves.push((planet, sector));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::entities::comp::Pos;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::universe::Campaign;
    use crate::io::json::content_serde::SectorKey;

    fn registry() -> ContentRegistry {
        create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).expect("content")
    }

    /// Campaign runtime whose played sector (170) exports to sector 98.
    fn campaign_world(registry: &ContentRegistry) -> (World, crate::content::PlanetId, u16) {
        let planet = registry.planet_id("serpulo").expect("serpulo");
        let mut campaign =
            Campaign::from_registry(registry, &crate::game::planet::EmptyNeighborhood);
        let played = 170u16;
        {
            let sector = campaign.sector_mut(planet, played).expect("sector");
            sector.save = Some("sector-serpulo-170".to_owned());
            sector.info.info.has_core = true;
            sector.info.info.destination = Some(SectorKey::format("serpulo", 98));
        }
        let mut session = PlaySession::new(Rules::default());
        session.sector = Some((planet, played));
        session.rules.default_team = 1;
        let mut world = World::new();
        world.insert_resource(CampaignRuntime::new(session, campaign));
        let index = ContentIndex {
            item_names: registry
                .items()
                .iter()
                .map(|item| item.name.clone())
                .collect(),
            ..ContentIndex::default()
        };
        world.insert_resource(index);
        (world, planet, played)
    }

    #[test]
    fn payload_exports_on_removal() {
        let registry = registry();
        let (mut world, planet, played) = campaign_world(&registry);
        let copper = registry.item_id("copper").expect("copper");
        let entity = world
            .spawn((
                Pos { x: 100.0, y: 100.0 },
                TeamComp { team: 1 },
                LaunchPayloadState {
                    id: 7,
                    stacks: vec![(copper, 100)],
                    time: 119.0,
                    lifetime: 120.0,
                    block: BlockId::AIR,
                },
            ))
            .id();
        launch_payload_system(&mut world);
        assert!(
            world.get::<LaunchPayloadState>(entity).is_none(),
            "payload despawned"
        );
        let runtime = world
            .remove_resource::<CampaignRuntime>()
            .expect("runtime installed");
        let record = runtime.campaign.sector(planet, played).expect("sector");
        // `handleItemExport` fills the transient counter; the mean moves on the
        // next 1-s refresh (`SectorInfo.java:123-130`).
        assert_eq!(record.info.info.export["copper"].counter, 100.0);
        assert_eq!(record.info.get_export("copper"), 0.0);
    }

    #[test]
    fn payload_without_destination_expires_silently() {
        let registry = registry();
        let (mut world, planet, played) = campaign_world(&registry);
        {
            let mut runtime = world
                .remove_resource::<CampaignRuntime>()
                .expect("runtime installed");
            runtime
                .campaign
                .sector_mut(planet, played)
                .expect("sector")
                .info
                .info
                .destination = None;
            world.insert_resource(runtime);
        }
        let copper = registry.item_id("copper").expect("copper");
        let entity = world
            .spawn((
                Pos { x: 0.0, y: 0.0 },
                TeamComp { team: 1 },
                LaunchPayloadState {
                    id: 1,
                    stacks: vec![(copper, 10)],
                    time: 119.0,
                    lifetime: 120.0,
                    block: BlockId::AIR,
                },
            ))
            .id();
        launch_payload_system(&mut world);
        assert!(world.get::<LaunchPayloadState>(entity).is_none());
        let runtime = world
            .remove_resource::<CampaignRuntime>()
            .expect("runtime installed");
        let record = runtime.campaign.sector(planet, played).expect("sector");
        assert!(record.info.info.export.get("copper").is_none());
    }
}

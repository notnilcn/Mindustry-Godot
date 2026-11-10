// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live campaign-sector, locked-upgrade and interplanetary-accelerator regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;

/// I01 T7: `core-bastion`/`core-citadel`/`core-acropolis` set
/// `requiresCoreZone = true` (`Blocks.java:3198,3218,3236`), so the upgrade
/// branch of `CoreBlock.canPlaceOn` (`CoreBlock.java:202`) needs every footprint
/// floor to allow core placement.
#[test]
fn bastion_requires_core_zone() {
    let mut sim = runtime_sim();
    // API staging skips `canPlaceOn`; the player-form query below runs it.
    place_team(&mut sim, 10, 10, "core-shard", 0, 1);
    // Skip the item requirement so only the core-zone rule decides.
    sim.ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
        .expect("build rules")
        .infinite_resources = true;
    // Player-form query (visible + core radius; no unit overlap).
    let flags = crate::world::build::PlaceFlags {
        check_visible: true,
        check_core_radius: true,
        check_units: false,
    };
    // Stone floor everywhere (no core zone): the upgrade is rejected.
    assert!(
        !place_query(&sim, 10, 10, "core-bastion", 0, 1, flags),
        "core-bastion needs the core zone under its footprint"
    );
    // A core-zone floor under the whole 4x4 footprint accepts it.
    let zone = BlockId::new(sim.world_block_id("core-zone").expect("core-zone"));
    for x in 9..13 {
        for y in 9..13 {
            sim.grid.tiles.get_mut(x, y).floor = zone;
        }
    }

    assert!(
        place_query(&sim, 10, 10, "core-bastion", 0, 1, flags),
        "core-zone floor satisfies requiresCoreZone"
    );
}

/// I04 T3: the Reconstructor refuses a locked upgrade target for a non-AI team
/// and arms `Trigger.cannotUpgrade` (`Reconstructor.java:252-257` ->
/// `HintsFragment.java:84`); once the target is in the live unlocked set the
/// payload is accepted again (`team.isAI() || upgrade.unlockedNowHost()`).
#[test]
fn locked_upgrade_blocks_and_fires_hint() {
    use crate::content::PlanetId;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::blocks::payloads::{PayloadRef, dispatch_accept_payload};

    let mut sim = runtime_sim();
    let campaign = {
        let content = sim.block_runtime().expect("runtime").content();
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    session.phase = crate::game::State::Playing;
    session.sector = Some((PlanetId::new(0), 170));
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let recon = place(&mut sim, "additive-reconstructor", 4, 4);
    let (dagger, mace_name) = {
        let content = sim.block_runtime().expect("runtime").content();
        let dagger = content.unit_id("dagger").expect("dagger");
        let mace = content.unit_id("mace").expect("mace");
        let name = content.unit(mace).expect("mace def").name.clone();
        (dagger, name)
    };
    let payload = PayloadRef {
        entity: None,
        content: dagger.raw(),
        is_block: false,
    };
    // The live unlocked set is empty: the dagger -> mace upgrade is locked.
    assert!(
        !dispatch_accept_payload(&sim.ecs.0, recon, recon, payload),
        "a locked upgrade target is refused for a non-AI team"
    );
    sim.tick().expect("tick");
    assert!(
        sim.hint_events.events.contains("cannotupgrade"),
        "the refusal fired Trigger.cannotUpgrade"
    );

    // Unlocking the target accepts it again.
    sim.ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("campaign")
        .set_unlocked_content([mace_name]);
    assert!(
        dispatch_accept_payload(&sim.ecs.0, recon, recon, payload),
        "an unlocked upgrade target is accepted"
    );
}

/// L06 T6.3: abandoning the played sector (`PlanetDialog.abandonSectorConfirm`,
/// `PlanetDialog.java:1153-1161`) kills every player-team core 7 ticks later
/// (`Time.runTask(7f, ... core.kill())`), so the campaign `checkGameState`
/// (`Logic.java:329-334`) plays the game-over / sector-loss path instead of the
/// record being deleted.
#[test]
fn abandon_played_sector_kills_cores_and_plays_game_over() {
    use crate::content::PlanetId;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::{PlayEvent, PlaySession};
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = runtime_sim();
    let core = place(&mut sim, "core-shard", 10, 10);
    let campaign = {
        let content = sim.block_runtime().expect("runtime").content();
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = crate::game::State::Playing;
    session.sector = Some((PlanetId::new(0), 15));
    let rules = session.rules.clone();
    session
        .teams
        .register_core(core, crate::ecs::TeamId(1), &rules);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    sim.ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("campaign")
        .abandon_played_sector();
    for _ in 0..6 {
        sim.tick().expect("tick");
    }
    assert!(
        sim.ecs.0.get_entity(core).is_ok(),
        "the core survives the 7-tick delay"
    );
    let early = sim
        .ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("campaign")
        .take_events();
    assert!(
        !early
            .iter()
            .any(|event| matches!(event, PlayEvent::SectorLose { .. })),
        "no loss before the cores die: {early:?}"
    );
    for _ in 0..6 {
        sim.tick().expect("tick");
    }
    assert!(
        sim.grid.entity_at(TilePos::new(10, 10)).is_none(),
        "the abandoned sector's core was killed"
    );
    let events = sim
        .ecs
        .0
        .get_resource_mut::<CampaignRuntime>()
        .expect("campaign")
        .take_events();
    assert!(
        events.iter().any(|event| matches!(
            event,
            PlayEvent::SectorLose {
                sector: 15,
                winner: 2,
                ..
            }
        )),
        "the game-over path ran: {events:?}"
    );
}

/// L06 T7 (PAR-0296): the Interplanetary Accelerator builds its launch core
/// once full and powered (`progress += Time.delta * efficiency /
/// buildDuration`, Accelerator.java:121-138), takes the core's capacity on top
/// once built (`getMaximumAccepted`, :293-295), launches only through
/// `canLaunch()` (:239-241; campaign + 1e6 battery buffer + progress >= 1) and
/// the launch consumes the core, carries the rest, drains the buffer and
/// resets `progress` (:262-272).
#[test]
fn accelerator_builds_core_gates_and_launches() {
    use crate::world::behavior::BuildingBehavior;
    use crate::world::behavior::campaign::AcceleratorBehavior;
    use crate::world::modules::ItemModule;

    let mut sim = Sim::new(3, 72, 24, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    install_campaign_session(
        &mut sim,
        crate::game::rules::Rules {
            default_team: 1,
            ..Default::default()
        },
    );
    if let Some(mut runtime) = sim
        .ecs
        .0
        .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
    {
        runtime.session.sector = Some((crate::content::PlanetId::new(0), 15));
    }
    let accelerator = place(&mut sim, "interplanetary-accelerator", 10, 10);
    let _source = place(&mut sim, "power-source", 10, 6);
    // 21 large batteries (50000 each, Blocks.java:2519) in a row on the top
    // edge: > the 1_000_000 `powerBufferRequirement` (Blocks.java:6876).
    for index in 0..21 {
        place(&mut sim, "battery-large", 8 + index * 3, 15);
    }
    let copper = item(&sim, "copper");
    let titanium = item(&sim, "titanium");
    let behavior = AcceleratorBehavior;
    assert_eq!(
        behavior.get_maximum_accepted(&sim.ecs.0, accelerator, copper),
        8000,
        "capacities = coreNucleus.requirements before the core is built"
    );
    assert!(!behavior.accept_item(&sim.ecs.0, accelerator, accelerator, titanium));

    for _ in 0..20 {
        sim.tick().expect("tick");
    }
    let progress = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::entities::comp::CampaignState>(accelerator)
            .map(|state| state.progress)
            .unwrap_or(-1.0)
    };
    assert_eq!(
        progress(&sim),
        0.0,
        "no items -> no efficiency -> no progress"
    );

    {
        let content = sim.block_runtime().expect("runtime").content();
        let stacks: Vec<(crate::content::ItemId, i32)> = ["copper", "lead", "silicon", "thorium"]
            .iter()
            .zip([8000, 8000, 5000, 4000])
            .map(|(name, amount)| (content.item_id(name).expect("item"), amount))
            .collect();
        let mut items = sim.ecs.0.get_mut::<ItemModule>(accelerator).expect("items");
        for (item, amount) in stacks {
            items.add(item, amount, 25000);
        }
    }
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let mid = progress(&sim);
    assert!(mid > 0.3 && mid < 0.7, "120-tick build: {mid}");
    assert!(
        !sim.accelerator_can_launch(accelerator),
        "core not built yet"
    );
    for _ in 0..80 {
        sim.tick().expect("tick");
    }
    assert_eq!(progress(&sim), 1.0);
    assert_eq!(
        behavior.get_maximum_accepted(&sim.ecs.0, accelerator, titanium),
        13000,
        "a built core adds launchBlock.itemCapacity"
    );
    sim.ecs
        .0
        .get_mut::<ItemModule>(accelerator)
        .expect("items")
        .add(titanium, 100, 40000);
    assert!(
        sim.accelerator_can_launch(accelerator),
        "built + powered + buffered"
    );
    assert!(sim.accelerator_config_tapped(accelerator));
    assert_eq!(
        sim.triggers
            .fire_count(crate::sim::events::Trigger::AcceleratorUse),
        1
    );

    let resources = sim.accelerator_launch(accelerator).expect("launch");
    assert_eq!(
        resources,
        vec![(titanium, 100)],
        "consume() takes the core; the rest is carried"
    );
    let state = sim
        .ecs
        .0
        .get::<crate::entities::comp::CampaignState>(accelerator)
        .cloned()
        .expect("state");
    assert_eq!(state.progress, 0.0);
    assert!(state.launching);
    assert_eq!(
        sim.ecs
            .0
            .get::<ItemModule>(accelerator)
            .expect("items")
            .total(),
        0
    );
    assert!(!sim.accelerator_can_launch(accelerator), "launching");
    assert!(sim.accelerator_launch(accelerator).is_none());
}

/// L06 T7: outside a campaign the accelerator never launches (`canLaunch`'s
/// `state.isCampaign()`), and `version() = 1` writes `progress`.
#[test]
fn accelerator_requires_campaign_and_writes_progress() {
    use crate::world::behavior::BuildingBehavior;
    use crate::world::behavior::campaign::AcceleratorBehavior;

    let mut sim = runtime_sim();
    let accelerator = place(&mut sim, "interplanetary-accelerator", 10, 10);
    if let Some(mut state) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::CampaignState>(accelerator)
    {
        state.progress = 1.0;
    }
    assert!(!sim.accelerator_can_launch(accelerator));
    assert_eq!(AcceleratorBehavior.version(&sim.ecs.0, accelerator), 1);
}

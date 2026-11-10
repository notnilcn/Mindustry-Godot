// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TickSet::Objectives` and the objective environment tests.
//! Upstream: none (port-only tests).

use crate::entities::comp::TeamComp;
use crate::game::State;
use crate::game::map_objectives::ObjectiveEnv;
use crate::game::runtime::objectives::RuntimeObjectiveEnv;
use crate::game::runtime::test_support::*;
use crate::game::runtime::*;
use crate::game::stats::GameStats;
use crate::game::teams::Teams;
use crate::sim::runtime::ContentIndex;
use crate::world::modules::ItemModule;
use bevy_ecs::entity::Entity;

/// T4: `RuntimeObjectiveEnv.block_at` resolves a live building through its
/// footprint; a `DestroyBlock` objective for a duo at (10,10) team 2 is not
/// complete while the duo stands and completes after the break.
#[test]
fn runtime_block_at_reports_building() {
    use crate::command::Command;
    use crate::content::BlockId;

    let registry = content();
    let campaign = campaign(&registry);
    let objectives: crate::io::json::objectives::MapObjectives = serde_json::from_str(
        r#"[{"class":"DestroyBlock","pos":{"x":10,"y":10},"team":2,"block":"duo"}]"#,
    )
    .expect("objectives json");
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        objectives,
        ..Rules::default()
    });
    session.phase = State::Playing;
    session.objectives =
        crate::game::map_objectives::MapObjectivesRuntime::from_rules(&session.rules);

    let mut sim = crate::sim::Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    let duo = sim.content().id("duo").expect("duo");
    assert!(
        sim.place_for_team(10, 10, duo, 0, 2),
        "duo placed on team 2"
    );
    let _ = sim.set_phase(State::Playing);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    sim.tick().expect("tick");
    {
        let runtime = sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .expect("runtime");
        assert!(
            !runtime.session.objectives.is_completed(0),
            "duo still stands"
        );
    }
    sim.apply(Command::Break { x: 10, y: 10 }).expect("break");
    sim.tick().expect("tick");
    let runtime = sim
        .ecs
        .0
        .get_resource::<CampaignRuntime>()
        .expect("runtime");
    assert!(
        runtime.session.objectives.is_completed(0),
        "duo destroyed completes the objective"
    );
}

/// GAPS #21: `ItemObjective` asks `Team.items()` (`MapObjectives.java:470`
/// region), which Java answers from the team's one aliased core module
/// (`CoreBlock.java:764-770`). Two cores of a team must agree on the pool
/// and the count must not double the inert per-core mirror.
#[test]
fn team_has_item_reads_the_shared_team_pool() {
    use crate::world::blocks::storage::core_block::CoreBuild;

    let registry = content();
    let copper = registry.item_id("copper").expect("copper");
    let lead = registry.item_id("lead").expect("lead");
    let items = registry.items().len();
    let mut world = World::new();
    let core = |world: &mut World, holder: Option<Entity>| {
        world
            .spawn((
                TeamComp { team: 1 },
                CoreBuild {
                    items_holder: holder,
                    ..CoreBuild::default()
                },
                ItemModule::with_items(items),
            ))
            .id()
    };
    let first = core(&mut world, None);
    let second = core(&mut world, Some(first));
    world
        .get_mut::<ItemModule>(first)
        .expect("pool")
        .add(copper, 100, 4000);
    // A save written by upstream stores the pool on every core; the mirror
    // must not be summed.
    world
        .get_mut::<ItemModule>(second)
        .expect("mirror")
        .add(copper, 100, 4000);

    let rules = Rules::default();
    let teams = Teams::new();
    let unlocked = std::collections::HashSet::new();
    let stats = GameStats::default();
    let index = ContentIndex {
        item_ids: [
            ("copper".to_owned(), copper.raw()),
            ("lead".to_owned(), lead.raw()),
        ]
        .into_iter()
        .collect(),
        ..ContentIndex::default()
    };
    let env = RuntimeObjectiveEnv {
        rules: &rules,
        teams: &teams,
        unlocked: &unlocked,
        stats: &stats,
        world: &world,
        index: Some(&index),
        command_mode_satisfied: false,
    };
    assert!(env.team_has_item(1, "copper", 100), "exactly the pool");
    assert!(
        !env.team_has_item(1, "copper", 101),
        "mirror not double counted"
    );
    assert!(!env.team_has_item(1, "lead", 1));
    assert!(!env.team_has_item(2, "copper", 1), "other teams untouched");
    // A deposit into the second core reaches the same pool.
    crate::world::blocks::storage::team_items::add_to_core(&mut world, second, copper, 5, 4000);
    let env = RuntimeObjectiveEnv {
        rules: &rules,
        teams: &teams,
        unlocked: &unlocked,
        stats: &stats,
        world: &world,
        index: Some(&index),
        command_mode_satisfied: false,
    };
    assert!(env.team_has_item(1, "copper", 105));
    assert!(!env.team_has_item(1, "copper", 106));
}

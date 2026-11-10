// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime core incineration and placement-rule regressions.
//! Upstream: none (port-only; live-runtime production/logistics regressions).

use super::*;

/// H02 T1: a full campaign core keeps accepting items (Java
/// `Rules.coreIncinerates`, `CoreBlock.java:744-751,886-913`) and incinerates
/// the overflow instead of rejecting it.
#[test]
fn full_core_incinerates_and_counts() {
    let mut sim = runtime_sim();
    let core = place(&mut sim, "core-shard", 10, 10);
    let copper = item(&sim, "copper");
    let cap = sim
        .ecs
        .0
        .get::<crate::world::blocks::storage::CoreBuild>(core)
        .map(|c| c.storage_capacity)
        .expect("core build");
    assert_eq!(cap, 4000, "core-shard itemCapacity (Blocks.java:3152)");
    {
        let mut items = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(core)
            .expect("items");
        items.add(copper, cap, cap);
    }
    // Default hooks (no resource) incinerate, so the full core accepts and the
    // overflow handle is dropped instead of stored.
    assert!(transfer::dispatch_accept_item(
        &sim.ecs.0, core, core, copper
    ));
    transfer::dispatch_handle_item(&mut sim.ecs.0, core, core, copper);
    assert_eq!(stored_item(&sim, core, copper), cap, "overflow incinerated");
}

/// H02 T1: `Rules.coreIncinerates = false` restores the rejecting behavior.
#[test]
fn core_rejects_when_not_incinerating() {
    #[derive(Debug)]
    struct NotIncinerating;
    impl crate::world::blocks::storage::CoreCampaignHooks for NotIncinerating {
        fn core_incinerates(&self) -> bool {
            false
        }
    }
    let mut sim = runtime_sim();
    let core = place(&mut sim, "core-shard", 10, 10);
    let copper = item(&sim, "copper");
    let cap = sim
        .ecs
        .0
        .get::<crate::world::blocks::storage::CoreBuild>(core)
        .map(|c| c.storage_capacity)
        .expect("core build");
    {
        let mut items = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(core)
            .expect("items");
        items.add(copper, cap, cap);
    }
    sim.ecs
        .0
        .insert_resource(crate::world::blocks::storage::CoreHooks(
            std::sync::Arc::new(NotIncinerating),
        ));
    assert!(!transfer::dispatch_accept_item(
        &sim.ecs.0, core, core, copper
    ));
}

/// H02 T2: `sync_build_rules` copies the placement/break rule subset of a
/// `Rules` value into the `BuildRules` resource (`World.setRules` half).
#[test]
fn sync_build_rules_copies_rules() {
    use crate::game::rules::Rules;
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    let copper_wall = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("copper-wall")
        .expect("copper-wall");
    let titanium_wall = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("titanium-wall")
        .expect("titanium-wall");
    let mut rules = Rules::default();
    rules.banned_blocks.insert("copper-wall".to_owned());
    rules.block_whitelist = false;
    rules.derelict_repair = false;
    rules.place_range_check = true;
    rules.enemy_core_build_radius = 320.0;
    rules.team_rule_mut(2).extra_core_build_radius = 40.0;
    crate::world::limits::sync_build_rules(&mut sim.ecs.0, &rules, true);
    {
        let build = sim.ecs.0.get_resource::<BuildRules>().expect("build rules");
        assert!(build.is_banned(copper_wall), "banned list copied");
        assert!(!build.allow_derelict_repair);
        assert!(build.place_range_check);
        assert_eq!(build.build_radius(2), 360.0, "320 + 40 extra");
        assert_eq!(build.build_radius(1), 320.0, "default team extra 0");
        assert!(build.is_campaign);
        assert_eq!(build.banned_blocks, vec![copper_wall]);
    }
    // `Rules.isBanned`: `blockWhitelist != bannedBlocks.contains(block)`.
    rules.block_whitelist = true;
    crate::world::limits::sync_build_rules(&mut sim.ecs.0, &rules, true);
    let build = sim.ecs.0.get_resource::<BuildRules>().expect("build rules");
    assert!(!build.is_banned(copper_wall), "whitelisted");
    assert!(build.is_banned(titanium_wall), "not in the whitelist");
}

/// H02 T2: `Build.validBreak` rejects other teams' buildings
/// (`Build.java:317-320`, `Teams.java:125-127`); derelict and own team stay
/// breakable.
#[test]
fn cannot_break_enemy_building() {
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    let wall = sim.world_block_id("copper-wall").expect("copper-wall");
    for (x, y, team) in [(5i16, 5i16, 2u8), (7, 5, 0), (9, 5, 1)] {
        sim.command(SimCommand::Place {
            x,
            y,
            block: wall,
            rotation: 0,
            team,
            player: None,
        })
        .expect("place wall");
    }
    let rules = sim
        .ecs
        .0
        .get_resource::<BuildRules>()
        .cloned()
        .expect("rules");
    let runtime = sim.block_runtime().expect("runtime");
    assert!(
        !crate::world::build::valid_break(
            runtime.content(),
            &sim.grid,
            Some(&sim.ecs.0),
            &rules,
            1,
            5,
            5
        ),
        "enemy building rejected"
    );
    assert!(
        crate::world::build::valid_break(
            runtime.content(),
            &sim.grid,
            Some(&sim.ecs.0),
            &rules,
            1,
            7,
            5
        ),
        "derelict building allowed"
    );
    assert!(
        crate::world::build::valid_break(
            runtime.content(),
            &sim.grid,
            Some(&sim.ecs.0),
            &rules,
            1,
            9,
            5
        ),
        "own building allowed"
    );
}

/// H02 T2: `CoreBlock.canBreak` (`CoreBlock.java:173-177`) keeps the last core
/// unbreakable; `coreBuildAndConfig` with a second core allows it.
#[test]
fn last_core_unbreakable() {
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    let core = sim.world_block_id("core-shard").expect("core-shard");
    sim.command(SimCommand::Place {
        x: 10,
        y: 10,
        block: core,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("place core");
    let rules = sim
        .ecs
        .0
        .get_resource::<BuildRules>()
        .cloned()
        .expect("rules");
    let runtime = sim.block_runtime().expect("runtime");
    assert!(
        !crate::world::build::valid_break(
            runtime.content(),
            &sim.grid,
            Some(&sim.ecs.0),
            &rules,
            1,
            10,
            10
        ),
        "last core unbreakable"
    );
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("rules")
        .core_build_and_config = true;
    sim.command(SimCommand::Place {
        x: 20,
        y: 20,
        block: core,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("place second core");
    let rules = sim
        .ecs
        .0
        .get_resource::<BuildRules>()
        .cloned()
        .expect("rules");
    let runtime = sim.block_runtime().expect("runtime");
    assert!(
        crate::world::build::valid_break(
            runtime.content(),
            &sim.grid,
            Some(&sim.ecs.0),
            &rules,
            1,
            10,
            10
        ),
        "two cores with coreBuildAndConfig are breakable"
    );
}

/// H02 T3: enemy buildings cannot be replaced (`Build.java:250-262`); own
/// conveyor upgrades pass.
#[test]
fn cannot_replace_enemy_conveyor() {
    use crate::world::build::PlaceFlags;

    let mut sim = runtime_sim();
    place_team(&mut sim, 5, 5, "conveyor", 0, 2);
    assert!(
        !place_query(&sim, 5, 5, "titanium-conveyor", 0, 1, PlaceFlags::PLAYER),
        "enemy conveyor cannot be replaced"
    );
    place_team(&mut sim, 7, 5, "conveyor", 0, 1);
    assert!(
        place_query(&sim, 7, 5, "titanium-conveyor", 0, 1, PlaceFlags::PLAYER),
        "own conveyor upgrades"
    );
}

/// H02 T3: deep water rejects non-floating blocks and pumps-without-contact
/// (`Build.java:249,258`); a conduit at the deep-water edge passes via
/// `contactsShallows` (`Build.java:294-314`).
#[test]
fn cannot_build_on_deep_water() {
    use crate::world::build::PlaceFlags;

    let mut sim = runtime_sim();
    for x in 7..=9 {
        for y in 7..=9 {
            set_floor(&mut sim, x, y, "deep-water");
        }
    }
    assert!(
        !place_query(&sim, 8, 8, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "wall on deep water"
    );
    assert!(
        !place_query(&sim, 8, 8, "conduit", 0, 1, PlaceFlags::PLAYER),
        "conduit in the middle of deep water has no shallows contact"
    );
    assert!(
        place_query(&sim, 7, 8, "conduit", 0, 1, PlaceFlags::PLAYER),
        "conduit at the deep-water edge touches the stone neighbour"
    );
}

/// H02 T3: same block + same rotation is rejected (`Build.java:244`); a
/// different rotation passes; non-rotating blocks reject same over same.
#[test]
fn same_block_same_rotation_rejected() {
    use crate::world::build::PlaceFlags;

    let mut sim = runtime_sim();
    place_team(&mut sim, 5, 5, "conveyor", 0, 1);
    assert!(
        !place_query(&sim, 5, 5, "conveyor", 0, 1, PlaceFlags::PLAYER),
        "same rotation rejected"
    );
    assert!(
        place_query(&sim, 5, 5, "conveyor", 1, 1, PlaceFlags::PLAYER),
        "different rotation replaces"
    );
    place_team(&mut sim, 9, 5, "router", 0, 1);
    assert!(
        !place_query(&sim, 9, 5, "router", 0, 1, PlaceFlags::PLAYER),
        "a router does not rotate, so router-over-router is rejected"
    );
}

/// H02 T3: derelict same-block repair (`Build.java:251`);
/// `allowDerelictRepair = false` rejects it.
#[test]
fn derelict_same_block_repair() {
    use crate::world::build::PlaceFlags;
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    place_team(&mut sim, 5, 5, "copper-wall", 0, 0);
    assert!(
        place_query(&sim, 5, 5, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "derelict same-block repair passes"
    );
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("rules")
        .allow_derelict_repair = false;
    assert!(
        !place_query(&sim, 5, 5, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "derelict repair disabled"
    );
}

/// H02 T3: the enemy-core no-build radius (`Build.java:195-208`,
/// `Rules.buildRadius` `Rules.java:294-296`) gates player placement but not
/// the API form.
#[test]
fn enemy_core_radius_blocks_placement() {
    use crate::content::BlockId;
    use crate::world::build::PlaceFlags;
    use crate::world::limits::BuildRules;

    let mut sim = Sim::new(3, 128, 128, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("rules")
        .enemy_core_build_radius = 400.0;
    place_team(&mut sim, 40, 40, "core-shard", 0, 2);
    place_team(&mut sim, 40, 90, "copper-wall", 0, 1);
    // Core center (324, 324); wall center (324, 724): distance 400 < 400+8.
    assert!(
        !place_query(&sim, 40, 90, "titanium-wall", 0, 1, PlaceFlags::PLAYER),
        "inside the enemy core radius"
    );
    // (40, 92) center (324, 740): distance 416 > 408.
    assert!(
        place_query(&sim, 40, 92, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "outside the enemy core radius"
    );
    assert!(
        place_query(&sim, 40, 90, "titanium-wall", 0, 1, PlaceFlags::API),
        "API skips the core radius"
    );
    assert!(
        place_query(&sim, 40, 92, "copper-wall", 0, 1, PlaceFlags::API),
        "API outside the radius"
    );
}

/// H02 T3: bans/whitelist only gate the visible (player) form
/// (`Block.isPlaceable` `Block.java:1031-1037`).
#[test]
fn banned_block_rejected_for_player_only() {
    use crate::world::build::PlaceFlags;
    use crate::world::limits::BuildRules;

    let mut sim = runtime_sim();
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("rules")
        .banned_blocks = vec![
        sim.block_runtime()
            .expect("runtime")
            .content()
            .block_id("copper-wall")
            .expect("copper-wall"),
    ];
    assert!(
        !place_query(&sim, 5, 5, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "banned for player placement"
    );
    assert!(
        place_query(&sim, 5, 5, "copper-wall", 0, 1, PlaceFlags::API),
        "API skips bans"
    );
    sim.ecs
        .0
        .get_resource_mut::<BuildRules>()
        .expect("rules")
        .block_whitelist = true;
    assert!(
        place_query(&sim, 5, 5, "copper-wall", 0, 1, PlaceFlags::PLAYER),
        "whitelist inverts the ban"
    );
}

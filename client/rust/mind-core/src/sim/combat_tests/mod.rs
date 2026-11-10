// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-sim combat regressions (turrets, bullet-vs-unit hits, unit/building
//! death) driven through the real `Sim` schedule with the block + unit
//! runtimes installed. Ported behaviors: `Turret.TurretBuild.updateTile`,
//! `BulletComp.collision`/`BulletType.hitEntity`, `UnitComp.update` death.

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::determinism::SimCommand;
use crate::entities::comp::{Health, Pos, Vel};
use crate::sim::test_support::{live_sim, place_at, tick_n};

use super::Sim;

mod ai_kinds;
mod assist_cargo_control;
mod building_explosions;
mod command_explosion;
mod commands_ai;
mod deaths;
mod defense_blocks;
mod environment;
mod fire_weather;
mod fog;
mod motion_payload;
mod multipliers_player;
mod pathing_mining;
mod payload_weapons;
mod physics;
mod player_campaign;
mod targeting;
mod turrets_bullets;
mod walls;

fn feed(sim: &mut Sim, turret: Entity, item: &str, count: usize) {
    let id = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .item_id(item)
        .expect("item");
    for _ in 0..count {
        crate::world::blocks::defense::turrets::handle_item(&mut sim.ecs.0, turret, id);
    }
}

fn shots_in(sim_setup: impl Fn(&mut Sim, Entity), ticks: usize) -> (i32, f32) {
    let mut sim = live_sim(48);
    let duo = place_at(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    sim_setup(&mut sim, duo);
    // Hostile wall dummy inside the duo's 150 px range: a unit target would
    // return fire (G01 T6a gives it the Java targeting cadence), which kills
    // the duo before the coolant window ends.
    let _wall = place_at(&mut sim, 31, 20, "copper-wall", 2);
    let start = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map_or(0, |s| s.total_ammo);
    tick_n(&mut sim, ticks);
    let end = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map_or(0, |s| s.total_ammo);
    let water = sim
        .unit_runtime()
        .expect("rt")
        .content
        .liquid_id("water")
        .expect("water");
    let left = sim
        .ecs
        .0
        .get::<crate::world::modules::LiquidModule>(duo)
        .map_or(0.0, |l| l.get(water));
    (start - end, left)
}

/// Finds the first weapon bullet on `unit` whose def sets `spawnUnit`
/// (`BulletType.spawnUnit`), returning `(bullet, unit)` ids.
fn spawn_unit_weapon(sim: &Sim, unit: &str) -> Option<(u16, crate::content::UnitTypeId)> {
    let content = &sim.unit_runtime().expect("rt").content;
    let id = content.unit_id(unit)?;
    let def = content.unit(id)?;
    def.weapons
        .iter()
        .map(|weapon| weapon.bullet.id)
        .find_map(|bullet| {
            let def = content.bullet(bullet)?;
            def.spawn_unit.map(|unit| (bullet.get(), unit))
        })
}

/// Builds a `live_sim` with a `CampaignRuntime` whose rules are mutated by
/// `mutate` (team multipliers for the G01 T5 tests).
fn campaign_sim(size: i32, mutate: impl FnOnce(&mut crate::game::rules::Rules)) -> Sim {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(size);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    mutate(&mut rules);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
    sim
}

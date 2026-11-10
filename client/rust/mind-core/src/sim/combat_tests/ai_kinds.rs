// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! AI-kind regressions: Oct, Quell, Renale and stuck avoidance.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;

/// H01 T9: `DefenderAI.findTarget` (`DefenderAI.java:26-38`) — an oct follows
/// the healthiest player-controllable ally in `max(range, 400)` (key
/// `-maxHealth + dst2 / 6400`), keeping `targetHit/2*1.1 + ownHit/2 + 15` px.
#[test]
fn oct_follows_healthiest_ally() {
    use crate::entities::comp::unit::comp::HitboxComp;
    let mut sim = live_sim(64);
    let oct = sim
        .spawn_live_unit("oct", 2, 100.0, 100.0, 0.0)
        .expect("oct");
    let _dagger = sim
        .spawn_live_unit("dagger", 2, 150.0, 100.0, 0.0)
        .expect("dagger");
    let mace = sim
        .spawn_live_unit("mace", 2, 300.0, 100.0, 0.0)
        .expect("mace");
    for _ in 0..1500 {
        sim.tick().expect("tick");
    }
    let hit = |sim: &Sim, e| {
        sim.ecs
            .0
            .get::<HitboxComp>(e)
            .map_or(0.0, |hitbox| hitbox.hit_size)
    };
    let opos = *sim.ecs.0.get::<Pos>(oct).expect("oct alive");
    let mpos = *sim.ecs.0.get::<Pos>(mace).expect("mace alive");
    let keep = hit(&sim, mace) / 2.0 * 1.1 + hit(&sim, oct) / 2.0 + 15.0;
    let dst = ((mpos.x - opos.x).powi(2) + (mpos.y - opos.y).powi(2)).sqrt();
    assert!(
        dst <= keep + 10.0,
        "oct within keep {keep} + 10 of the mace (dist {dst}, oct {opos:?}, mace {mpos:?})"
    );
}

/// H01 T9: `FlyingFollowAI` (`FlyingFollowAI.java:26-29`) — a wave-team quell
/// follows the healthiest allied non-follower.
#[test]
fn quell_follows_ally_on_wave_team() {
    use crate::entities::comp::unit::comp::HitboxComp;
    let mut sim = live_sim(64);
    let quell = sim
        .spawn_live_unit("quell", 2, 60.0, 100.0, 0.0)
        .expect("quell");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 200.0, 100.0, 0.0)
        .expect("dagger");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let hit = |sim: &Sim, e| {
        sim.ecs
            .0
            .get::<HitboxComp>(e)
            .map_or(0.0, |hitbox| hitbox.hit_size)
    };
    let qpos = *sim.ecs.0.get::<Pos>(quell).expect("quell alive");
    let dpos = *sim.ecs.0.get::<Pos>(dagger).expect("dagger alive");
    let keep = hit(&sim, dagger) / 2.0 * 1.1 + hit(&sim, quell) / 2.0 + 15.0;
    let dst = ((dpos.x - qpos.x).powi(2) + (dpos.y - qpos.y).powi(2)).sqrt();
    assert!(
        dst <= keep + 10.0,
        "quell within keep {keep} + 10 of the dagger (dist {dst})"
    );
}

/// H01 T9: `FlyingFollowAI.useFallback()` — a player-team quell runs `FlyingAI`
/// (toward the enemy core) instead of following the ally.
#[test]
fn quell_uses_flying_ai_on_player_team() {
    let mut sim = live_sim(64);
    let quell = sim
        .spawn_live_unit("quell", 1, 60.0, 100.0, 0.0)
        .expect("quell");
    let _ally = sim
        .spawn_live_unit("dagger", 1, 200.0, 100.0, 0.0)
        .expect("dagger");
    let core = place(&mut sim, 30, 30, "core-shard", 2);
    let core_pos = *sim.ecs.0.get::<Pos>(core).expect("core pos");
    let start = *sim.ecs.0.get::<Pos>(quell).expect("quell pos");
    let start_dst = ((core_pos.x - start.x).powi(2) + (core_pos.y - start.y).powi(2)).sqrt();
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(quell).expect("quell alive");
    let dst = ((core_pos.x - pos.x).powi(2) + (core_pos.y - pos.y).powi(2)).sqrt();
    assert!(
        dst < start_dst - 30.0,
        "the player-team quell flew toward the enemy core ({start_dst} -> {dst})"
    );
}

/// H01 T9: `HugAI.updateMovement` (`HugAI.java:16-61`) — a renale closes on an
/// in-range enemy until the contact circle
/// `(unit.hitSize + target.hitSize) * 0.5`.
#[test]
fn renale_hugs_target() {
    use crate::entities::comp::unit::comp::HitboxComp;
    let mut sim = live_sim(64);
    let range = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_by_name("renale")
        .expect("renale")
        .range;
    let renale = sim
        .spawn_live_unit("renale", 2, 150.0, 100.0, 0.0)
        .expect("renale");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 150.0 + range * 0.8, 100.0, 180.0)
        .expect("dagger");
    for _ in 0..900 {
        sim.tick().expect("tick");
    }
    let hit = |sim: &Sim, e| {
        sim.ecs
            .0
            .get::<HitboxComp>(e)
            .map_or(0.0, |hitbox| hitbox.hit_size)
    };
    let rpos = *sim.ecs.0.get::<Pos>(renale).expect("renale alive");
    let dpos = *sim.ecs.0.get::<Pos>(dagger).expect("dagger alive");
    let contact = (hit(&sim, renale) + hit(&sim, dagger)) * 0.5;
    let dst = ((dpos.x - rpos.x).powi(2) + (dpos.y - rpos.y).powi(2)).sqrt();
    assert!(
        dst <= contact + 2.0,
        "renale reached the contact circle {contact} (dist {dst})"
    );
}

/// H01 T9: `NoAI.updateUnit(){}` (`NoAI.java`) — the dummy never moves.
#[test]
fn dummy_never_moves() {
    let mut sim = live_sim(64);
    let dummy = sim
        .spawn_live_unit("dummy", 2, 100.0, 100.0, 0.0)
        .expect("dummy");
    let start = *sim.ecs.0.get::<Pos>(dummy).expect("dummy pos");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(dummy).expect("dummy alive");
    assert_eq!((pos.x, pos.y), (start.x, start.y), "the dummy never moves");
}

/// H01 T11: `GroundAI` stuck bookkeeping (`GroundAI.java:24,63-85`): a unit
/// that has `moved` true but cannot leave a 12 px window accumulates
/// `stuckTime` past `stuckThreshold = max(1, tilesize * 1.5 * 2 / speed)`
/// (48 ticks for a dagger), then requests flowfield avoidance: the process
/// activates (`AvoidanceProcess.java:40-48`) and its grid marks every ground
/// AI unit's tile (`:52-106`). The "different next tile" selection itself is
/// covered by `get_target_tile_skips_avoided_neighbour`.
#[test]
fn stuck_dagger_activates_avoidance_in_a_pocket() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ai::types::ground::GroundAiState;
    use crate::ecs::EntitySeq;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::WorldGrid;

    let mut sim = live_sim(64);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    // The enemy core and a stationary same-team unit elsewhere on the map.
    place(&mut sim, 48, 31, "core-shard", 1);
    let blocker = sim
        .spawn_live_unit("dagger", 2, 24.0 * 8.0, 24.0 * 8.0, 0.0)
        .expect("blocker");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(blocker) {
        slot.kind = AiKind::NoAi;
    }
    // A 1-tile pocket: the follower is walled in on all four sides, so
    // `should_move` stays true while its position cannot change.
    for (x, y) in [(20, 30), (20, 32), (19, 31), (21, 31)] {
        place(&mut sim, x, y, "copper-wall", 2);
    }
    let follower = sim
        .spawn_live_unit("dagger", 2, 20.0 * 8.0, 31.0 * 8.0, 0.0)
        .expect("follower");
    let blocker_id = sim
        .ecs
        .0
        .get::<EntitySeq>(blocker)
        .map_or(0, |seq| (seq.0 & 0x7fff_ffff) as i32);
    let width = sim.unit_runtime().expect("rt").grid.width();
    let start = *sim.ecs.0.get::<Pos>(follower).expect("follower pos");

    let mut max_stuck = 0.0f32;
    let mut activated_at = None;
    let mut marked = false;
    for tick in 0..300 {
        sim.tick().expect("tick");
        if let Some(state) = sim.ecs.0.get::<GroundAiState>(follower) {
            max_stuck = max_stuck.max(state.stuck_time);
        }
        if activated_at.is_none() && sim.unit_runtime().is_some_and(|rt| rt.avoidance_active) {
            activated_at = Some(tick);
        }
        let bpos = *sim.ecs.0.get::<Pos>(blocker).expect("blocker alive");
        if let Some(rt) = sim.unit_runtime() {
            let bx = WorldGrid::to_tile(bpos.x);
            let by = WorldGrid::to_tile(bpos.y);
            if rt.avoidance.get((bx + by * width) as usize).copied() == Some(i32::MAX - blocker_id)
            {
                marked = true;
            }
        }
    }
    // `stuckThreshold = max(1, tilesize * 1.5 * 2 / speed)` = 48 for a dagger.
    assert!(
        max_stuck >= 48.0,
        "the stuck time crossed the threshold (max {max_stuck})"
    );
    assert!(
        activated_at.is_some(),
        "the flowfield avoidance grid activated after the jam"
    );
    assert!(marked, "the blocker's tile is marked on the avoidance grid");
    // Walled in: the follower never left its tile.
    let end = *sim.ecs.0.get::<Pos>(follower).expect("follower alive");
    assert_eq!(
        (end.x, end.y),
        (start.x, start.y),
        "the pocketed follower stayed put"
    );
    // The blocker never moved.
    let bpos = *sim.ecs.0.get::<Pos>(blocker).expect("blocker alive");
    assert_eq!(
        (bpos.x, bpos.y),
        (24.0 * 8.0, 24.0 * 8.0),
        "the NoAI blocker stayed put"
    );
}

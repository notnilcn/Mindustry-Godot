// SPDX-License-Identifier: GPL-3.0-only

//! PAR-0600: the relay-assigned team setter (`PlayerComp.team` outside the editor gate).
//!
//! Upstream: none (port-only tests).

use crate::entities::comp::TeamComp;
use crate::sim::test_support::live_sim_seeded;

/// A relay-assigned team moves both `PlayerComp.team` and the possessed
/// unit's `TeamComp` (`PlayerComp.java:266-271`), and the possessed unit is
/// kept (only its team changes).
#[test]
fn relay_team_moves_the_player_unit_and_respawn_team() {
    let mut sim = live_sim_seeded(7, 24);
    let unit = sim
        .spawn_live_unit("alpha", 1, 32.0, 32.0, 0.0)
        .expect("alpha unit spawned");
    sim.player_unit = Some(unit);

    sim.set_relay_player_team(3);

    assert_eq!(sim.player_team, Some(3), "PlayerComp.team moved");
    assert_eq!(
        sim.ecs.0.get::<TeamComp>(unit).map(|comp| comp.team),
        Some(3),
        "possessed unit followed PlayerComp.team"
    );
    assert_eq!(sim.player_unit(), Some(unit), "unit stays possessed");
}

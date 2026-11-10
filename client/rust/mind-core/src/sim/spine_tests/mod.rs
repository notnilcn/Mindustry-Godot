// SPDX-License-Identifier: GPL-3.0-only

//! Tests that moved out of `sim/mod.rs`: the privileged logic event sinks, the team-AI
//! host actions, the ability pending-queue drains and the relay player-team setter.
//!
//! Upstream: none (port-only tests).

mod ability_queue_tests;
mod logic_sink_tests;
mod relay_player_team_tests;
mod team_ai_host_tests;

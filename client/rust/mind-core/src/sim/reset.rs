// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Logic.play` / `Logic.reset` / pause flows (plan 05 M3).
//!
//! Ported from `core/src/mindustry/core/Logic.java` (`play()`, `reset()`), and
//! `core/GameState.java` transitions. A full world reload is plan 06; here the
//! flows clear the P0 grid/entities and fire the canonical events.

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::event::{PlayEvent, ResetEvent, SimEvent, StateChangeEvent};
use crate::game::{GameState, State};

use super::events::Trigger;

impl super::Sim {
    /// Begins/resumes play: sets `Playing`, fires `StateChangeEvent`,
    /// `PlayEvent` and `Trigger.newGame` (port of `Logic.play`).
    pub fn play(&mut self) -> bool {
        let from = self.state.phase;
        if !self.state.set_phase(State::Playing) {
            return false;
        }
        self.push_event(SimEvent::StateChangeEvent(StateChangeEvent {
            from,
            to: State::Playing,
        }));
        self.push_event(SimEvent::PlayEvent(PlayEvent));
        self.triggers.fire(Trigger::NewGame, &mut self.ecs.0);
        true
    }

    /// Resets to the menu: clears entities/grid/delayed runs, resets the clock,
    /// fires `ResetEvent` (and `StateChangeEvent` when leaving a game); port of
    /// `Logic.reset` (`Groups.clear`, `Time.clear`, reset `GameState`).
    pub fn reset(&mut self) {
        let from = self.state.phase;
        if self.block_runtime.is_some() {
            // Live runtime: despawn only sim entities. `clear_entities` would
            // also drop the entity-backed resources (`BlockTable`, `BuildRules`,
            // `PowerGrids`) the runtime needs; a fresh arena leaves no stale
            // handles and the counter restarts empty (`Groups.clear`).
            let entities: Vec<Entity> = self
                .ecs
                .0
                .iter_entities()
                .filter(|entity| entity.contains::<crate::ecs::EntitySeq>())
                .map(|entity| entity.id())
                .collect();
            for entity in entities {
                self.ecs.0.despawn(entity);
            }
            self.ecs
                .0
                .insert_resource(crate::world::blocks::power::PowerGrids::new());
            if let Some(runtime) = self.block_runtime.as_mut() {
                runtime.reset();
            }
        } else {
            self.ecs.0.clear_entities();
        }
        self.grid.fill(BlockId::AIR, BlockId::AIR);
        // Live units/bullets are despawned above; clear the runtime's lists and
        // respawn window so the next world starts clean (`Groups.clear`).
        let grid = self.grid.clone();
        if let Some(mut runtime) = self
            .ecs
            .0
            .get_non_send_mut::<super::unit_runtime::UnitRuntime>()
        {
            runtime.reset(&grid);
        }
        self.time.clear();
        self.clock.reset();
        self.state = GameState::new(State::Menu);
        if from != State::Menu {
            self.push_event(SimEvent::StateChangeEvent(StateChangeEvent {
                from,
                to: State::Menu,
            }));
        }
        self.push_event(SimEvent::ResetEvent(ResetEvent));
    }

    /// Pauses a running game; returns whether the phase changed.
    pub fn pause(&mut self) -> bool {
        self.set_phase(State::Paused)
    }

    /// Resumes to `Playing`; returns whether the phase changed.
    pub fn resume(&mut self) -> bool {
        self.set_phase(State::Playing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::SimEvent;
    use crate::sim::Sim;

    #[test]
    fn reset_play_cycle() {
        let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
        sim.set_phase(State::Menu);
        // play → playing, tick advances.
        assert!(sim.play());
        assert_eq!(sim.phase(), State::Playing);
        for _ in 0..5 {
            sim.tick().expect("tick");
        }
        assert_eq!(sim.tick_count(), 5);
        // pause/resume.
        assert!(sim.pause());
        assert!(sim.is_paused());
        assert!(sim.resume());
        assert_eq!(sim.phase(), State::Playing);
        // reset → menu, clock zeroed, reset event queued.
        sim.reset();
        assert_eq!(sim.phase(), State::Menu);
        assert_eq!(sim.clock().time, 0.0);
        assert_eq!(sim.clock().update_id, 0);
        let events = sim.pending_events.clone();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, SimEvent::ResetEvent(_)))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, SimEvent::StateChangeEvent(_)))
        );
    }

    #[test]
    fn reset_clears_live_runtime_state() {
        use crate::world::blocks::power::PowerGrids;

        let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("install");
        let node = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .block_id("power-node")
            .expect("power-node");
        sim.apply(crate::command::Command::Place {
            x: 2,
            y: 2,
            block: node,
        })
        .expect("place");
        assert_eq!(
            sim.block_runtime()
                .expect("runtime")
                .counter()
                .count(0, node),
            1
        );
        sim.reset();
        assert_eq!(
            sim.block_runtime()
                .expect("runtime")
                .counter()
                .count(0, node),
            0
        );
        assert_eq!(
            sim.ecs
                .0
                .get_resource::<PowerGrids>()
                .expect("arena")
                .graph_count(),
            0
        );
    }
}

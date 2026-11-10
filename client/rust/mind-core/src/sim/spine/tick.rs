// SPDX-License-Identifier: GPL-3.0-only

//! `Sim::tick`: the fixed-step simulation pipeline.
//!
//! Upstream: core/src/mindustry/core/Logic.java (`update`).

use crate::sim::*;

impl Sim {
    /// Advances the simulation by one fixed step and flushes queued events.
    ///
    /// Schedule order follows `Logic.update()` (plan 05 §3.4): `Trigger.update`
    /// → set chain → delayed runs (`Time.update`) → `Trigger.afterGameUpdate`.
    pub fn tick(&mut self) -> Result<(), SimError> {
        // `IoSet::Apply` is the pre-tick boundary (plan 04 §3.10): no load may
        // mutate the world mid-tick.
        self.pump_io(IoSet::Apply);
        self.triggers.fire(Trigger::Update, &mut self.ecs.0);
        // `Logic.update` -> `logicVars.update()` (Logic.java:546) runs before
        // the entity update pass so processors observe the incremented tick.
        self.update_logic_globals();
        // `Logic.java:533-535`: `fogControl.update()` when `rules.fog`, after
        // `updateTeamStats()` and before the entity pass.
        self.update_fog();
        // `UnitAssembler.checkSolid` needs the grid (L06 T1): refresh
        // `wasOccupied` just before the building pass reads it.
        self.refresh_assembler_occupancy();
        self.schedule.run(&mut self.ecs.0);
        // `BeamDrill.updateFacing`/`WallCrafter.getEfficiency` (T5): the facing
        // rescan needs the grid, so it runs outside the scheduled behaviors.
        self.refresh_facing_drills();
        // `ThermalGeneratorBuild.onProximityAdded` (T14): the floor-attribute
        // sum also needs the grid.
        self.refresh_thermal_generators();
        // `BeamNode.updateTile` -> `updateDirections` (T15): the directional
        // scan needs the grid and the power graph arena.
        self.refresh_beam_nodes();
        // `ConsumeItemExplode.update` (T2): explosive generator fuel damages
        // the building and fires `Trigger.blastGenerator`.
        self.update_item_explode_consumers();
        // GAPS #58: the generic `ConsumeItemExplode` half of the same Java body —
        // the scheduled consumer pass queued `Building.damage` requests; apply
        // them (and the spark effect / `Trigger.blastGenerator`) before the
        // death pass below.
        self.drain_consumer_damage();
        // `PowerGenerator.onExplosion`'s sim half (T4): damage, fires, props,
        // fireballs and puddles (the fireballs drain in the fires/puddles pass
        // below, matching `onExplosion`'s immediate `createNet`).
        self.apply_generator_explosions();
        // `Groups.fires`/`Groups.puddles.update()` (`Logic.updateEntities`):
        // fires burn/spread and puddles evaporate/apply statuses every tick
        // (PAR-0091). Damage they deal is picked up by `resolve_deaths` below.
        self.update_fires_and_puddles();
        // `BuilderComp.update` (PAR-0098): player build plans drive the
        // construct blocks after the entity update pass.
        self.update_builders();
        // `Trigger.fireCreate` (G05 T4b): any fire born this tick (bullets,
        // puddles, explosions) arms the `waveFire` hint.
        if self
            .ecs
            .0
            .remove_resource::<crate::combat::fires::FireCreated>()
            .is_some()
        {
            self.hint_events.events.insert("fire");
        }
        // `Trigger.cannotUpgrade` (I04 T3 `HintsFragment.java:84`): a
        // Reconstructor refused a locked upgrade target for a non-AI team
        // (`Reconstructor.java:252-257`). The marker uses interior mutability
        // (the `accept_payload` hook only has `&World`), so clear it here.
        if self
            .ecs
            .0
            .get_resource::<crate::world::blocks::units::behavior::CannotUpgradeFired>()
            .is_some_and(|fired| fired.0.swap(false, std::sync::atomic::Ordering::Relaxed))
        {
            self.hint_events.events.insert("cannotupgrade");
        }
        // World-processor UI events (announce/toast/cutscene) are drained here so
        // the log never grows unbounded (T4; T5/T6 extend the match).
        self.drain_logic_events();
        // Zero-health units/buildings die (`HealthComp.damage` -> `kill`).
        let _ = self.resolve_deaths();
        // `PlayerComp.update`: respawn at the best core after `deathDelay`.
        self.update_player_respawn();
        // Capture-derived world requests (T7) apply after the death pass and
        // before the events flush, mirroring Java's next-frame `Core.app.post`.
        self.apply_campaign_world_requests();
        // A mid-match rules change (G06 `setrule`, relay `setRules`, invasion)
        // re-derives the world from the session rules (T5).
        self.apply_dirty_rules_to_world();
        self.flush_events();
        self.time.update();
        self.clock.update(&mut self.ecs.0);
        self.state.advance();
        self.triggers
            .fire(Trigger::AfterGameUpdate, &mut self.ecs.0);
        // `IoSet::Capture` is the post-update boundary.
        self.pump_io(IoSet::Capture);
        Ok(())
    }
}

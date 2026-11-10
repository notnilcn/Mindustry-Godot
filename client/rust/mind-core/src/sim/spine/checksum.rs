// SPDX-License-Identifier: GPL-3.0-only

//! `Sim` state-name/checksum accessors (the frozen deterministic stream).
//!
//! Upstream: none (port-only; the checksum is the port's determinism gate).

use crate::sim::*;

impl Sim {
    /// Current phase name (inspector/parity).
    pub fn state_name(&self) -> &'static str {
        self.state.phase.name()
    }

    /// Per-group live entity counts (plan 05 M9 inspector surface).
    ///
    /// At the P0/M9 stage only the `build` group carries entities; the remaining
    /// groups report `0` until their plans (08–11) register membership. The map
    /// is a `BTreeMap` so the inspector dictionary is deterministic.
    pub fn group_counts(&self) -> std::collections::BTreeMap<&'static str, usize> {
        // `all` counts real sim entities (`EntitySeq`); Bevy's raw entity slots
        // can include internal/reserved ids that are not sim entities.
        let all = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| entity.get::<crate::ecs::EntitySeq>().is_some())
            .count();
        let build = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| entity.get::<crate::ecs::BuildingComp>().is_some())
            .count();
        // Live unit/bullet halves of `Groups` (present only once the opt-in
        // unit runtime is installed).
        let (unit, bullet) = match self.ecs.0.get_non_send::<unit_runtime::UnitRuntime>() {
            Some(runtime) => (
                runtime.unit_count(&self.ecs.0),
                runtime.bullet_count(&self.ecs.0),
            ),
            None => (0, 0),
        };
        let weather = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| {
                entity
                    .get::<crate::entities::comp::WeatherState>()
                    .is_some()
            })
            .count();
        // Tile entities (`Groups.fires` / `Groups.puddles`).
        let fire = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| entity.get::<crate::combat::fires::FireState>().is_some())
            .count();
        let puddle = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| {
                entity
                    .get::<crate::combat::puddles::PuddleState>()
                    .is_some()
            })
            .count();
        let mut counts = std::collections::BTreeMap::new();
        counts.insert("all", all);
        counts.insert("unit", unit);
        counts.insert("build", build);
        counts.insert("bullet", bullet);
        counts.insert("player", 0);
        counts.insert("effect", 0);
        counts.insert("weather", weather);
        counts.insert("power_graph", 0);
        counts.insert("sync", 0);
        counts.insert("draw", 0);
        counts.insert("fire", fire);
        counts.insert("puddle", puddle);
        counts
    }

    /// The deterministic fixed-step clock.
    pub fn clock(&self) -> &SimClock {
        &self.clock
    }

    /// The simulation configuration.
    pub fn config(&self) -> &SimConfig {
        &self.config
    }

    /// The host platform seam.
    pub fn platform(&self) -> &dyn Platform {
        self.platform.as_ref()
    }

    /// Canonical deterministic checksum (§6.5; HLP C2): FNV-1a-64 over the
    /// versioned stream — `GameState` scalars → clock time/runs → `Sim` RNG
    /// state → grid row-major → entities in stable seq order.
    pub fn checksum(&self) -> u64 {
        self.checksum_value().value()
    }

    /// The strongly-typed [`Checksum`].
    pub fn checksum_value(&self) -> Checksum {
        let mut c = Checksummer::new();
        c.part(&self.state.phase.as_u8());
        c.part(&self.state.tick);
        c.part(&self.state.update_id);
        c.part(&self.clock.time_units());
        c.part(&(self.clock.runs_len() as u64));
        c.part(&self.rng_streams.sim_state());
        c.part(&self.seed);

        for (_pos, index) in self.grid.iter_row_major() {
            let tile = self.grid.tile_ref(index);
            let block = tile.block;
            let build = tile.build;
            c.part(&block.raw());
            // Team/rotation live on the building entity; the P0 path always uses
            // team `sharded` (0) / rotation 0, preserved here to keep the
            // canonical stream byte-identical.
            c.part(&0u8);
            c.part(&0u8);
            match build.and_then(|entity| self.ecs.seq_of(entity)) {
                Some(build_id) => {
                    c.part(&1u8);
                    c.part(&build_id);
                }
                None => {
                    c.part(&0u8);
                    c.part(&0u64);
                }
            }
        }

        for (seq, _entity, comp) in self.ecs.entities_by_seq() {
            c.part(&seq);
            c.part(&comp.block.raw());
            c.part(&comp.team.0);
            c.part(&comp.rot);
            c.part(&comp.pos.x());
            c.part(&comp.pos.y());
        }

        c.finish()
    }

    /// Checksum as 16 lowercase hex digits (golden format).
    pub fn checksum_hex(&self) -> String {
        self.checksum_value().to_hex()
    }

    /// Scoped checksum (plan 21 §6.5): walks the identical plan-05 hasher order
    /// but replaces fields excluded by `scope` with their declared defaults.
    ///
    /// At the P0/M9 stage there are no `SyncLocal`/player-possessed components,
    /// so no field is masked and the value equals [`Sim::checksum`]. The seam is
    /// additive: plans 05/11/16 hook masked fields here without changing the
    /// relay API. `scope.bits()` is the value peers publish in
    /// `match_checksum.scope`.
    pub fn checksum_scoped(&self, scope: &ChecksumScope) -> u64 {
        let _ = scope;
        self.checksum()
    }
}

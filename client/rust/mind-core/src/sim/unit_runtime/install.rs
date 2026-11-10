// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Installation and refresh of the opt-in [`UnitRuntime`] plus the wave-spawn
//! request half of `Logic`.
//! Upstream: core/src/mindustry/core/Logic.java (runWave/spawnWave)

use super::waves::spawn_group_unit;
use super::*;

impl Sim {
    /// Whether the opt-in live unit runtime is installed.
    pub fn has_unit_runtime(&self) -> bool {
        self.ecs.0.contains_non_send::<UnitRuntime>()
    }

    /// Read-only live unit runtime, if installed.
    pub fn unit_runtime(&self) -> Option<&UnitRuntime> {
        self.ecs.0.get_non_send::<UnitRuntime>()
    }

    /// Mutable live unit runtime, if installed.
    pub fn unit_runtime_mut(&mut self) -> Option<bevy_ecs::change_detection::Mut<'_, UnitRuntime>> {
        self.ecs.0.get_non_send_mut::<UnitRuntime>()
    }

    /// Installs the live unit runtime with vanilla content (idempotent).
    pub fn install_unit_runtime(&mut self) -> Result<(), RuntimeError> {
        if self.has_unit_runtime() {
            return Ok(());
        }
        let content = load_vanilla_content()?;
        self.install_unit_runtime_from(content);
        Ok(())
    }

    /// Installs the live unit runtime over a prebuilt content registry.
    pub fn install_unit_runtime_from(&mut self, content: ContentRegistry) {
        // A loaded world carries door `open` state; mirror `checkSolid` into the
        // grid before the pathfinder packs it.
        crate::world::sync_dynamic_solid(&mut self.grid, &self.ecs.0);
        // The content-backed status applier must exist before anything can
        // apply a status (`StatusApplier` resource; NEW-1): install it while
        // the registry is still owned here, then move it into the runtime.
        crate::combat::damage::status::install_status_applier(&mut self.ecs.0, &content);
        // `BuildDamageEvent` log (G01 T15): drained by the minimap seam.
        self.ecs
            .0
            .insert_resource(crate::world::events::BuildDamageLog::default());
        // `flashHit` wall-collision log (I02 T4): drained by the block pass.
        self.ecs
            .0
            .insert_resource(crate::world::events::WallFlashLog::default());
        let wave_team = crate::game::rules::Rules::default().wave_team;
        let core_capture =
            crate::game::runtime::rules_of(&self.ecs.0).is_some_and(|rules| rules.core_capture);
        let runtime = UnitRuntime::new(content, &self.grid, &self.ecs.0, wave_team, core_capture);
        self.ecs.0.insert_non_send(runtime);
    }

    /// Sets the live wave/`WaveSpawner` team (`session.waveTeam`) and rebuilds
    /// the packed path tiles when it changes.
    pub fn set_unit_wave_team(&mut self, team: u8) {
        let grid = self.grid.clone();
        let core_capture =
            crate::game::runtime::rules_of(&self.ecs.0).is_some_and(|rules| rules.core_capture);
        let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() else {
            return;
        };
        runtime.wave_team = team;
        if runtime.path_team != team {
            runtime.path_team = team;
            runtime.refresh_grid(&grid, &self.ecs.0, core_capture);
        }
        self.ecs.0.insert_non_send(runtime);
    }

    /// Requests a wave spawn for `wave` (0-based; `Logic.runWave` calls
    /// `spawnEnemies` before incrementing `state.wave`). Re-snapshots the live
    /// grid first so spawn-overlay/terrain changes are visible to the spawner.
    pub fn request_wave_spawn(&mut self, wave: i32) {
        self.refresh_unit_runtime();
        let Some(mut runtime) = self.ecs.0.get_non_send_mut::<UnitRuntime>() else {
            return;
        };
        runtime.pending_wave = Some(wave);
    }

    /// `Logic.skipWave()` -> `runWave()` (`Logic.java:315-325`; `SpawnWaveI`
    /// with `natural = true`, `J/logic/LExecutor.java:2173-2175`) on the live
    /// session: advances the wave and queues `spawnEnemies()` for the
    /// pre-increment wave. Without a `CampaignRuntime` (sandbox/scenario)
    /// Java still runs the wave: `LogicWorldState.wave` increments and the
    /// unit runtime spawns `getSpawned(wave - 1)`.
    pub fn logic_skip_wave(&mut self) {
        self.refresh_unit_runtime();
        if let Some(mut runtime) = self
            .ecs
            .0
            .remove_resource::<crate::game::runtime::CampaignRuntime>()
        {
            {
                let mut unit_runtime = self.ecs.0.get_non_send_mut::<UnitRuntime>();
                crate::game::runtime::run_wave_now(&mut runtime, unit_runtime.as_deref_mut());
            }
            self.ecs.0.insert_resource(runtime);
            return;
        }
        let spawn_index = {
            self.ecs
                .0
                .get_resource_mut::<crate::logic::world::LogicWorldState>()
                .map(|mut state| {
                    let index = state.wave - 1;
                    state.wave += 1;
                    index
                })
        };
        if let Some(spawn_index) = spawn_index {
            self.request_wave_spawn(spawn_index);
        }
    }

    /// `SpawnWaveI` positional path (`J/logic/LExecutor.java:2177-2198`):
    /// every spawn group whose `spawn` is `-1` or the packed `(x, y)` emits
    /// `group.getSpawned(state.wave - 1)` units within `tilesize * 2` px of
    /// `World.unconv(x, y)`, facing the map center; the wave does not advance.
    pub fn logic_spawn_wave_at(&mut self, x: i32, y: i32) {
        let Some((groups, wave, wave_team)) = self
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .map(|campaign| {
                (
                    groups_from_rules(&campaign.session.rules),
                    campaign.session.wave,
                    campaign.session.wave_team(),
                )
            })
        else {
            return;
        };
        let packed = TilePos::new(x as i16, y as i16).pack();
        let width_px = self.grid.width() as f32 * TILE_SIZE;
        let height_px = self.grid.height() as f32 * TILE_SIZE;
        // Java `Tmp.v1.rnd(spread)` with `spread = tilesize * 2`: a random
        // direction of length 16 (same draw as `spawn_enemies`' ground branch).
        let (wx, wy) = (x as f32 * 8.0, y as f32 * 8.0);
        let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() else {
            return;
        };
        let UnitRuntime {
            content,
            seq,
            units,
            units_created,
            rng,
            ..
        } = &mut runtime;
        let unmoving = content.status_id("unmoving");
        let invincible = content.status_id("invincible");
        let mut emitted = 0u64;
        for group in &groups {
            // `if(group.type == null) continue` (unresolvable unit type).
            if content
                .unit_id(&group.unit)
                .and_then(|id| content.unit(id))
                .is_none()
            {
                continue;
            }
            if group.spawn != -1 && group.spawn != packed {
                continue;
            }
            let count = group.get_spawned(wave - 1);
            for _ in 0..count {
                let theta = rng
                    .range(crate::determinism::RngStream::Sim, 0.0, 360.0)
                    .to_radians();
                let sx = wx + theta.cos() * 16.0;
                let sy = wy + theta.sin() * 16.0;
                let rotation = (height_px / 2.0 - sy)
                    .atan2(width_px / 2.0 - sx)
                    .to_degrees();
                let team = group.team.unwrap_or(wave_team);
                if let Some(entity) = spawn_group_unit(
                    &mut self.ecs.0,
                    content,
                    seq,
                    group,
                    team,
                    sx,
                    sy,
                    rotation,
                    wave - 1,
                    unmoving,
                    invincible,
                ) {
                    units.push(entity);
                    emitted += 1;
                }
            }
        }
        *units_created += emitted;
        self.ecs.0.insert_non_send(runtime);
    }

    /// Re-snapshots the live grid into the unit runtime (map load/tile change).
    pub fn refresh_unit_runtime(&mut self) {
        // `Tile.solid()`'s `checkSolid` term is mirrored into the grid before
        // anything packs/collides against it (`world::sync_dynamic_solid`).
        crate::world::sync_dynamic_solid(&mut self.grid, &self.ecs.0);
        let grid = self.grid.clone();
        let core_capture =
            crate::game::runtime::rules_of(&self.ecs.0).is_some_and(|rules| rules.core_capture);
        let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() else {
            return;
        };
        runtime.refresh_grid(&grid, &self.ecs.0, core_capture);
        self.ecs.0.insert_non_send(runtime);
    }
}

/// Converts the persisted `Rules.spawns` shape into runtime spawn groups.
pub(super) fn groups_from_rules(rules: &crate::game::rules::Rules) -> Vec<SpawnGroup> {
    rules
        .spawns
        .iter()
        .filter_map(|group| {
            serde_json::to_value(group)
                .ok()
                .and_then(|value| SpawnGroup::from_json(&value).ok())
        })
        .collect()
}

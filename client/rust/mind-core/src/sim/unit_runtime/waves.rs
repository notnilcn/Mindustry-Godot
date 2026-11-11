// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Wave spawning: the `TickSet::RunWave` system, its per-group spawn body and
//! the core spawn positioning.
//! Upstream: core/src/mindustry/core/Logic.java (runWave)
//! core/src/mindustry/ai/WaveSpawner.java

use super::install::groups_from_rules;
use super::*;

/// `TickSet::RunWave` system: advances the spawn window and emits the pending
/// wave's groups through `WaveSpawner` (`Logic.runWave` + `WaveSpawner.update`).
/// One `SpawnGroup.createUnit` + `WaveSpawner.spawnEffect` emission
/// (`WaveSpawner.java:224-247`): the extracted per-unit body of
/// `run_wave_system`'s closure, shared with the positional `spawnwave` path
/// (`LExecutor.java:2190-2195`, H06 T2).
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_group_unit(
    world: &mut World,
    content: &ContentRegistry,
    seq: &mut u64,
    group: &SpawnGroup,
    team: u8,
    x: f32,
    y: f32,
    rotation: f32,
    wave: i32,
    unmoving: Option<crate::content::StatusId>,
    invincible: Option<crate::content::StatusId>,
) -> Option<Entity> {
    let entity = group.create_unit(world, content, *seq, team, x, y, rotation, wave)?;
    if let Some(status) = unmoving {
        crate::combat::damage::status::apply_status(
            world,
            entity,
            status,
            SPAWN_EFFECT_UNMOVING_TICKS,
        );
    }
    if let Some(status) = invincible {
        crate::combat::damage::status::apply_status(
            world,
            entity,
            status,
            SPAWN_EFFECT_INVINCIBLE_TICKS,
        );
    }
    let payloads = group
        .payloads
        .as_ref()
        .map_or(0, |payloads| payloads.len() as u64);
    *seq = seq.wrapping_add(1 + payloads);
    Some(entity)
}

pub fn run_wave_system(world: &mut World) {
    let _ = with_unit_runtime_removed(world, |world, runtime| {
        runtime.wave_spawner.update();
        let Some(wave) = runtime.pending_wave.take() else {
            return;
        };

        // `SpawnGroup.getSpawned(state.wave - 1)`: the pending value is the
        // pre-increment wave minus one (`WaveSpawner.java:74`;
        // `MindCampaign.run_wave`/the campaign timer return it from
        // `run_wave_campaign`).
        let (groups, wave_team) = match world.get_resource::<CampaignRuntime>() {
            Some(campaign) if !campaign.session.rules.spawns.is_empty() => (
                groups_from_rules(&campaign.session.rules),
                campaign.session.wave_team(),
            ),
            Some(campaign) => (runtime.waves.get().to_vec(), campaign.session.wave_team()),
            None => (runtime.waves.get().to_vec(), runtime.wave_team),
        };
        runtime.wave_team = wave_team;
        runtime.path_team = wave_team;
        let core_capture = world
            .get_resource::<CampaignRuntime>()
            .is_some_and(|campaign| campaign.session.rules.core_capture);
        {
            let UnitRuntime {
                grid,
                content,
                pathfinder,
                ..
            } = runtime;
            pathfinder.rebuild(grid, content, world, core_capture);
        }

        // `WaveSpawner` spawn tiles are the `spawn` overlay tiles of the live grid
        // (`TileOverlayChangeEvent` rebuild covers map loads here).
        let mut spawns: Vec<TilePos> = runtime
            .grid
            .tiles
            .array()
            .iter()
            .filter(|tile| {
                runtime
                    .content
                    .block(tile.overlay)
                    .is_some_and(|def| def.kind == BlockKind::SpawnBlock)
            })
            .map(|tile| TilePos::new(tile.x, tile.y))
            .collect();

        // `WaveSpawner.eachGroundSpawn`/`eachFlyerSpawn` context
        // (WaveSpawner.java:122-200). Overlay ground spawns pass `worldx() = x*8`
        // (corner) and take the drop-zone shockwave; attack-mode core spawns keep
        // `shockwave = false`. Flyers spawn at the clamped map edge unless
        // `rules.airUseSpawns`, plus the wave-team cores in attack mode.
        //
        // The core branch (`WaveSpawner.java:134-173`): with `rules.wavesSpawnAtCores`
        // in attack mode every wave-team core is also a spawn position, stepped
        // toward the first player core until the 3x3 tile area is clear (PAR-0139
        // prerequisite: attack sectors author no spawn-overlay tiles).
        let width = runtime.grid.width();
        let height = runtime.grid.height();
        let air_use_spawns = world
            .get_resource::<CampaignRuntime>()
            .is_some_and(|campaign| campaign.session.rules.air_use_spawns);
        let ts = TILE_SIZE;
        let mut ctx = SpawnContext {
            width_px: width as f32 * ts,
            height_px: height as f32 * ts,
            ground: Vec::new(),
            flyer: Vec::new(),
            campaign_mult: world
                .get_resource::<CampaignRuntime>()
                .and_then(|campaign| {
                    campaign
                        .session
                        .is_campaign()
                        .then_some(campaign.session.difficulty_enemy_spawn_multiplier)
                }),
        };
        for tile in &spawns {
            ctx.ground.push((
                tile.x() as f32 * ts,
                tile.y() as f32 * ts,
                tile.pack(),
                true,
            ));
            if air_use_spawns {
                ctx.flyer
                    .push((tile.x() as f32 * ts, tile.y() as f32 * ts, tile.pack()));
            } else {
                let (fx, fy) = flyer_edge_position(width, height, *tile);
                ctx.flyer.push((fx, fy, tile.pack()));
            }
        }
        if let Some(campaign) = world.get_resource::<CampaignRuntime>() {
            let rules = campaign.session.rules.clone();
            if rules.waves_spawn_at_cores && rules.attack_mode {
                let player_core = campaign
                    .session
                    .teams
                    .get_or_null(crate::ecs::TeamId(rules.default_team))
                    .and_then(|data| data.cores.first().copied())
                    .and_then(|core| world.get::<Pos>(core).map(|pos| (pos.x, pos.y)));
                if let Some((px, py)) = player_core {
                    let wave_cores: Vec<Entity> = campaign
                        .session
                        .teams
                        .get_or_null(crate::ecs::TeamId(wave_team))
                        .map(|data| data.cores.clone())
                        .unwrap_or_default();
                    for core in wave_cores {
                        let Some(pos) = world.get::<Pos>(core).copied() else {
                            continue;
                        };
                        let packed = world
                            .get::<BuildingComp>(core)
                            .map(|comp| comp.pos.pack())
                            .unwrap_or(-1);
                        // `cons.accept(core.x, core.y, false)` (flyer branch) uses
                        // the raw core position; the ground branch steps toward the
                        // player core (`core_spawn_position`).
                        ctx.flyer.push((pos.x, pos.y, packed));
                        if let Some(tile) = core_spawn_position(
                            &runtime.grid,
                            &runtime.content,
                            pos.x,
                            pos.y,
                            px,
                            py,
                        ) {
                            ctx.ground.push((
                                tile.x() as f32 * ts,
                                tile.y() as f32 * ts,
                                packed,
                                false,
                            ));
                            // Keep the public spawn-tile list complete (T6a's
                            // `closest_spawn_tile`, `playerNear`).
                            spawns.push(tile);
                        }
                    }
                }
            }
        }

        // `doShockwave` (`WaveSpawner.java:104-106`): 99,999,999 damage over the
        // drop-zone radius on every overlay spawn, before any unit is emitted.
        let drop_zone_radius = world
            .get_resource::<CampaignRuntime>()
            .map(|campaign| campaign.session.rules.drop_zone_radius)
            .unwrap_or(300.0);
        let shock_targets: Vec<(f32, f32)> = ctx
            .ground
            .iter()
            .filter_map(|&(x, y, _, shockwave)| shockwave.then_some((x, y)))
            .collect();
        for (x, y) in shock_targets {
            crate::combat::damage::area::damage_area(
                world,
                &runtime.content,
                Some(wave_team),
                x,
                y,
                drop_zone_radius,
                99_999_999.0,
                crate::combat::damage::area::DamageOptions::default(),
            );
        }
        runtime.wave_spawner.set_spawns(spawns);
        log::info!(
            "wave spawn: wave {wave} with {} group(s), {} ground / {} flyer position(s)",
            groups.len(),
            ctx.ground.len(),
            ctx.flyer.len(),
        );

        let UnitRuntime {
            wave_spawner,
            content,
            seq,
            units,
            units_created,
            rng,
            ..
        } = runtime;
        // `WaveSpawner.spawnEffect` (WaveSpawner.java:228-235): unmoving 30,
        // invincible 60 on every spawned unit.
        let unmoving = content.status_id("unmoving");
        let invincible = content.status_id("invincible");
        let mut spawned: Vec<Entity> = Vec::new();
        let emitted = wave_spawner.spawn_enemies(
            &groups,
            wave,
            &ctx,
            content,
            rng,
            |group, x, y, rotation| {
                let team = group.team.unwrap_or(wave_team);
                let entity = spawn_group_unit(
                    world, content, seq, group, team, x, y, rotation, wave, unmoving, invincible,
                )?;
                spawned.push(entity);
                Some(entity)
            },
        );
        units.extend(spawned);
        *units_created += u64::from(emitted);
        log::info!("wave spawn: emitted {emitted} unit(s) for wave {wave}");
    });
}

/// `WaveSpawner.eachGroundSpawn` core branch: a spawn tile near `(cx, cy)`
/// stepped toward `(px, py)` until no solid tile lies within radius 3
/// (`coreMargin = tilesize * 2`, `maxSteps = 30`, core size 3).
pub(super) fn core_spawn_position(
    grid: &WorldGrid,
    content: &ContentRegistry,
    cx: f32,
    cy: f32,
    px: f32,
    py: f32,
) -> Option<TilePos> {
    let tile = crate::config::TILESIZE as f32;
    let (dx, dy) = (px - cx, py - cy);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return None;
    }
    let limit = (tile * 2.0 + tile * 1.5 * std::f32::consts::SQRT_2).min(len);
    let mut offset = limit;
    for _ in 0..30 {
        let tx = ((cx + dx / len * offset) / tile).round() as i32;
        let ty = ((cy + dy / len * offset) / tile).round() as i32;
        let mut blocked = false;
        for ox in -3..=3i32 {
            for oy in -3..=3i32 {
                if ox * ox + oy * oy > 9 {
                    continue;
                }
                if grid.tiles.getc(tx + ox, ty + oy).solid_full(content) {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                break;
            }
        }
        if !blocked {
            return Some(TilePos::new(tx as i16, ty as i16));
        }
        offset += tile * 1.1;
    }
    None
}

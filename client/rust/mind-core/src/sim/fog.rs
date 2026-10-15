// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live `FogControl` glue (`FogControl.java` + `Logic.java:533-535`): the
//! `Sim` owns the resource, (re)initialises it on world load / rules apply,
//! updates it once per tick before the entity pass and exposes the
//! `isVisibleTile`/`isDiscovered` probes. The `inFogTo`/`isDiscovered` gates
//! live in `crate::game::fog_gate`.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::registries::blocks::BlockFlag;
use crate::content::{BlockDef, ContentRegistry};
use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
use crate::entities::comp::{Building, Pos, TeamComp};
use crate::game::fog::{FogControl, FogSource, FogUnit};
use crate::game::runtime::{CampaignRuntime, rules_of};
use crate::sim::Sim;

impl Sim {
    /// `WorldLoadEvent` half: resets and sizes the fog for the current grid
    /// (`FogControl.java:46-62`). No rules (or `rules.fog` off) removes the
    /// resource so Serpulo pays nothing.
    pub fn reset_fog(&mut self) {
        let Some(rules) = rules_of(&self.ecs.0) else {
            self.ecs.0.remove_resource::<FogControl>();
            self.refresh_static_fog_mask();
            return;
        };
        let (fog_on, static_fog) = (rules.fog, rules.static_fog);
        if !fog_on {
            self.ecs.0.remove_resource::<FogControl>();
            self.refresh_static_fog_mask();
            return;
        }
        let is_campaign = self.is_campaign_session();

        // `pushStaticBlocks`: every `hasFogRadius` building that is not on an
        // only-AI team. With no live content yet the resource is still created
        // (sized); the next `update` fills it in.
        let blocks: Vec<FogSource> = {
            let mut blocks = Vec::new();
            if let Some(content) = sim_content(self) {
                let (collected, _) = collect_fog_sources(&self.ecs.0, content);
                if let Some(rules) = rules_of(&self.ecs.0) {
                    blocks = collected
                        .into_iter()
                        .filter(|src| !rules.team_is_only_ai(src.team, is_campaign))
                        .collect();
                }
            }
            blocks
        };

        if self.ecs.0.get_resource::<FogControl>().is_none() {
            self.ecs.0.insert_resource(FogControl::new());
        }
        let (width, height) = (self.grid.width(), self.grid.height());
        if let Some(mut fog) = self.ecs.0.remove_resource::<FogControl>() {
            fog.on_world_load(width as u16, height as u16, fog_on, static_fog, &blocks);
            self.ecs.0.insert_resource(fog);
        }
        self.refresh_static_fog_mask();
    }

    /// `Logic.update` -> `fogControl.update()` when `rules.fog`
    /// (`Logic.java:533-535`), plus the chunked `updateFogVisibility` pass
    /// (`FogControl.java:219-241`, `BuildingComp.java:2285-2293`).
    pub(crate) fn update_fog(&mut self) {
        let (fog_on, static_fog) = {
            let Some(rules) = rules_of(&self.ecs.0) else {
                return;
            };
            (rules.fog, rules.static_fog)
        };
        if !fog_on {
            return;
        }
        // `FogControl.update`: `if(fog == null) fog = new FogData[256];` — the
        // sector start path applies the world rules before the campaign
        // runtime exists, so the resource can be missing on the first tick.
        if self.ecs.0.get_resource::<FogControl>().is_none() {
            self.reset_fog();
            if self.ecs.0.get_resource::<FogControl>().is_none() {
                return;
            }
        }
        let is_campaign = self.is_campaign_session();

        // Read phase: sources + present teams (`state.teams.present`).
        let (blocks, units, teams, default_team) = {
            let Some(content) = sim_content(self) else {
                return;
            };
            let Some(rules) = rules_of(&self.ecs.0) else {
                return;
            };
            let (blocks, units) = collect_fog_sources(&self.ecs.0, content);
            let blocks = blocks
                .into_iter()
                .filter(|src| !rules.team_is_only_ai(src.team, is_campaign))
                .collect::<Vec<_>>();
            let mut teams = present_teams(&self.ecs.0);
            teams.retain(|&team| !rules.team_is_only_ai(team, is_campaign));
            (blocks, units, teams, rules.default_team)
        };

        let tick = self.state.tick;
        let mut static_dirty = false;
        if let Some(mut fog) = self.ecs.0.remove_resource::<FogControl>() {
            fog.update(tick, &teams, &blocks, &units, static_fog);
            static_dirty = fog.static_dirty;
            fog.static_dirty = false;
            self.ecs.0.insert_resource(fog);
        }
        // `Build.validPlace` (Build.java:252) reads the live static bits; the
        // port snapshots them into `BuildRules` whenever they changed (or no
        // snapshot exists yet).
        if static_fog && (static_dirty || self.static_fog_mask_missing()) {
            self.refresh_static_fog_mask();
        }

        // `updateFogVisibility` for the local player's team (single player =
        // default team): reveal newly visible buildings on the minimap.
        self.update_fog_visibility(default_team);
    }

    /// Java TileChange/TilePreChange listeners (`FogControl.java:64-88`): a
    /// placed fog-radius building schedules the static + dynamic redraw, a
    /// removed one only the dynamic redraw.
    ///
    /// `BlockBrokenEvent` carries no team before T10, so the broken half marks
    /// every non-AI team (a redraw is harmless).
    pub(crate) fn handle_fog_block_event(&mut self, event: &crate::event::SimEvent) {
        use crate::event::SimEvent;

        let block = match event {
            SimEvent::BlockPlacedEvent(placed) => placed.block,
            SimEvent::BlockBrokenEvent(broken) => broken.block,
            _ => return,
        };
        let (fog_on, static_fog) = {
            let Some(rules) = rules_of(&self.ecs.0) else {
                return;
            };
            (rules.fog, rules.static_fog)
        };
        if !fog_on || self.ecs.0.get_resource::<FogControl>().is_none() {
            return;
        }
        let has_fog_radius = {
            let Some(content) = sim_content(self) else {
                return;
            };
            content
                .block(block)
                .is_some_and(|def| def.flags.contains(&BlockFlag::HasFogRadius))
        };
        if !has_fog_radius {
            return;
        }
        let is_campaign = self.is_campaign_session();

        // Placed: the new building's fog event (`TileChangeEvent` half).
        let source = if let SimEvent::BlockPlacedEvent(placed) = event {
            let only_ai = rules_of(&self.ecs.0)
                .is_some_and(|rules| rules.team_is_only_ai(placed.team, is_campaign));
            if only_ai {
                return;
            }
            let radius = {
                let Some(content) = sim_content(self) else {
                    return;
                };
                let entity = self
                    .grid
                    .entity_at(crate::world::TilePos::new(placed.x, placed.y));
                let Some(entity) = entity else {
                    return;
                };
                let Some(def) = content.block(block) else {
                    return;
                };
                (building_fog_radius(&self.ecs.0, def, entity) + 0.5).floor() as i32
            };
            Some(FogSource {
                x: placed.x as i32,
                y: placed.y as i32,
                radius,
                team: placed.team,
            })
        } else {
            None
        };

        // Broken: `mark_dynamic` for every non-AI team (T10 adds the team).
        let mark_teams: Vec<u8> = if source.is_none() {
            let Some(rules) = rules_of(&self.ecs.0) else {
                return;
            };
            (0..=255u8)
                .filter(|team| !rules.team_is_only_ai(*team, is_campaign))
                .collect()
        } else {
            Vec::new()
        };

        let Some(mut fog) = self.ecs.0.remove_resource::<FogControl>() else {
            return;
        };
        if let Some(source) = source {
            fog.on_source_changed(source, static_fog);
        }
        for team in mark_teams {
            fog.mark_dynamic(team);
        }
        self.ecs.0.insert_resource(fog);
    }

    /// `FogControl.isVisibleTile(team, x, y)` (`FogControl.java:117-123`).
    pub fn fog_visible_tile(&self, team: u8, x: i32, y: i32) -> bool {
        let Some(rules) = rules_of(&self.ecs.0) else {
            return true;
        };
        let fog_on = rules.fog;
        let is_ai = rules.team_is_ai(team, self.is_campaign_session());
        match self.ecs.0.get_resource::<FogControl>() {
            Some(fog) => fog.is_visible_tile(team, x, y, fog_on, is_ai),
            None => !fog_on,
        }
    }

    /// `FogControl.isDiscovered(team, x, y)` (`FogControl.java:104-111`).
    pub fn fog_discovered(&self, team: u8, x: i32, y: i32) -> bool {
        let Some(rules) = rules_of(&self.ecs.0) else {
            return true;
        };
        let (fog_on, static_fog) = (rules.fog, rules.static_fog);
        let is_ai = rules.team_is_ai(team, self.is_campaign_session());
        match self.ecs.0.get_resource::<FogControl>() {
            Some(fog) => fog.is_discovered(team, x, y, static_fog, fog_on, is_ai),
            None => !fog_on,
        }
    }

    /// Static discovered bits for `team` as `(width, height, bytes)` with one
    /// byte per tile (`0`/`255`, row-major, y up) — the H03 fog-render seam.
    pub fn fog_static_bytes(&self, team: u8) -> Option<(i32, i32, Vec<u8>)> {
        let fog = self.ecs.0.get_resource::<FogControl>()?;
        let data = fog.get_discovered(team)?;
        let (ww, wh) = (fog.ww as i32, fog.wh as i32);
        let mut bytes = vec![0u8; data.len()];
        for (index, byte) in bytes.iter_mut().enumerate() {
            if data.get(index) {
                *byte = 255;
            }
        }
        Some((ww, wh, bytes))
    }

    /// Dynamic vision sources for `team` as `(world x, world y, radius px)` —
    /// the H03 fog-render seam (radius = fog radius in tiles * 8).
    pub fn fog_sources(&self, team: u8) -> Vec<(f32, f32, f32)> {
        let Some(content) = sim_content(self) else {
            return Vec::new();
        };
        let (blocks, units) = collect_fog_sources(&self.ecs.0, content);
        let mut out = Vec::new();
        // `radius <= 0` sources never enter the dynamic queue
        // (`FogControl.java:259`), so the render seam skips them too.
        for src in blocks
            .into_iter()
            .filter(|src| src.team == team && src.radius > 0)
        {
            out.push((
                src.x as f32 * 8.0 + 4.0,
                src.y as f32 * 8.0 + 4.0,
                src.radius as f32 * 8.0,
            ));
        }
        for src in units
            .into_iter()
            .filter(|src| src.team == team && src.radius > 0)
        {
            out.push((
                src.tile_x as f32 * 8.0 + 4.0,
                src.tile_y as f32 * 8.0 + 4.0,
                src.radius as f32 * 8.0,
            ));
        }
        out
    }

    /// `TeamData.present` membership is campaign/session state, not stored on
    /// `Rules`: `CampaignRuntime.session.is_campaign()`.
    fn is_campaign_session(&self) -> bool {
        self.ecs
            .0
            .get_resource::<CampaignRuntime>()
            .is_some_and(|runtime| runtime.session.is_campaign())
    }

    /// Whether `BuildRules` has no static fog snapshot yet.
    fn static_fog_mask_missing(&self) -> bool {
        self.ecs
            .0
            .get_resource::<crate::world::limits::BuildRules>()
            .is_some_and(|rules| rules.static_fog_mask.is_none())
    }

    /// Rebuilds `BuildRules.static_fog_mask` from the live static bits for the
    /// default team (`Build.java:252`): `None` when fog is off, all-zero bits
    /// while the player team has no fog data yet (Java `isDiscovered` returns
    /// false without `FogData`).
    fn refresh_static_fog_mask(&mut self) {
        let (fog_on, static_fog, default_team) = match rules_of(&self.ecs.0) {
            Some(rules) => (rules.fog, rules.static_fog, rules.default_team),
            None => (false, false, 0),
        };
        let mask = if fog_on && static_fog {
            let fog = self.ecs.0.get_resource::<FogControl>();
            let (ww, wh) = fog.map_or((0, 0), |fog| (fog.ww as i32, fog.wh as i32));
            let bits = fog
                .and_then(|fog| fog.get_discovered(default_team))
                .cloned()
                .unwrap_or_else(|| crate::game::fog::FogBits::new((ww * wh).max(0) as usize));
            Some(crate::world::limits::StaticFogMask {
                team: default_team,
                width: ww,
                height: wh,
                bits,
            })
        } else {
            None
        };
        if let Some(mut rules) = self
            .ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
        {
            rules.static_fog_mask = mask.map(std::sync::Arc::new);
        }
    }

    /// Chunked `BuildingComp.updateFogVisibility` pass
    /// (`FogControl.java:219-241`), default team only.
    fn update_fog_visibility(&mut self, default_team: u8) {
        let mut buildings: Vec<Entity> = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| entity.contains::<Building>())
            .map(|entity| entity.id())
            .collect();
        buildings.sort_by_key(|entity| entity.index());
        let size = buildings.len();
        if size == 0 {
            return;
        }

        let (start, iterated) = {
            let Some(fog) = self.ecs.0.get_resource::<FogControl>() else {
                return;
            };
            let chunks = 5usize.min(size);
            (fog.last_entity_update_index % size, (size / chunks).max(1))
        };

        // Read phase: which of this tick's visited chunk is newly visible.
        let mut reveal: Vec<Entity> = Vec::new();
        {
            let Some(content) = sim_content(self) else {
                return;
            };
            for step in 0..iterated {
                let entity = buildings[(start + step) % size];
                let Some(building) = self.ecs.0.get::<Building>(entity) else {
                    continue;
                };
                if building.was_visible {
                    continue;
                }
                if !crate::game::fog_gate::building_in_fog_to(
                    &self.ecs.0,
                    content,
                    Some(&self.grid),
                    entity,
                    default_team,
                ) {
                    reveal.push(entity);
                }
            }
        }
        for entity in reveal {
            if let Some(mut building) = self.ecs.0.get_mut::<Building>(entity) {
                building.visible_flags |= 1u64 << default_team;
                building.was_visible = true;
            }
        }
        if let Some(mut fog) = self.ecs.0.get_resource_mut::<FogControl>() {
            fog.last_entity_update_index = (start + iterated) % size;
        }
    }
}

/// Content registry backing the live runtimes (block runtime preferred).
fn sim_content(sim: &Sim) -> Option<&ContentRegistry> {
    if let Some(runtime) = sim.block_runtime.as_ref() {
        return Some(runtime.content());
    }
    sim.unit_runtime().map(|runtime| &runtime.content)
}

/// Team ids that own at least one building or unit (`state.teams.present`,
/// `Teams.java:188,219`), ascending.
fn present_teams(world: &World) -> Vec<u8> {
    let mut present = [false; 256];
    for entity in world.iter_entities() {
        if !entity.contains::<TeamComp>() {
            continue;
        }
        if (entity.contains::<Building>() || entity.contains::<UnitTypeComp>())
            && let Some(team) = entity.get::<TeamComp>()
        {
            present[team.team as usize] = true;
        }
    }
    present
        .iter()
        .enumerate()
        .filter_map(|(team, &is_present)| is_present.then_some(team as u8))
        .collect()
}

/// Collects `(blocks, units)` fog sources from the ECS in stable order
/// (`entity.index()`), mirroring `FogControl.pushStaticBlocks`/`update`.
pub(crate) fn collect_fog_sources(
    world: &World,
    content: &ContentRegistry,
) -> (Vec<FogSource>, Vec<FogUnit>) {
    let mut block_entities: Vec<Entity> = world
        .iter_entities()
        .filter(|entity| entity.contains::<Building>() && entity.contains::<TeamComp>())
        .map(|entity| entity.id())
        .collect();
    block_entities.sort_by_key(|entity| entity.index());
    let mut blocks = Vec::new();
    for entity in block_entities {
        let (Some(building), Some(team)) =
            (world.get::<Building>(entity), world.get::<TeamComp>(entity))
        else {
            continue;
        };
        let Some(def) = content.block(building.block) else {
            continue;
        };
        // `Block.java:1390`: `fogRadius > 0` sets the flag; radar/turret/core
        // kinds are runtime-radius sources (`building_fog_radius`).
        let is_source = def.flags.contains(&BlockFlag::HasFogRadius)
            || world
                .get::<crate::entities::comp::building::RadarState>(entity)
                .is_some()
            || world
                .get::<crate::world::blocks::defense::turrets::TurretState>(entity)
                .is_some()
            || world
                .get::<crate::world::blocks::storage::core_block::CoreBuild>(entity)
                .is_some();
        if !is_source {
            continue;
        }
        blocks.push(FogSource {
            x: building.tile.x() as i32,
            y: building.tile.y() as i32,
            radius: (building_fog_radius(world, def, entity) + 0.5).floor() as i32,
            team: team.team,
        });
    }

    let mut unit_entities: Vec<Entity> = world
        .iter_entities()
        .filter(|entity| {
            entity.contains::<UnitTypeComp>()
                && entity.contains::<Pos>()
                && entity.contains::<TeamComp>()
        })
        .map(|entity| entity.id())
        .collect();
    unit_entities.sort_by_key(|entity| entity.index());
    let mut units = Vec::new();
    for entity in unit_entities {
        let (Some(core), Some(pos), Some(team), Some(type_comp)) = (
            world.get::<UnitCore>(entity),
            world.get::<Pos>(entity),
            world.get::<TeamComp>(entity),
            world.get::<UnitTypeComp>(entity),
        ) else {
            continue;
        };
        if core.dead {
            continue;
        }
        let Some(def) = content.unit(type_comp.type_id) else {
            continue;
        };
        // `int tx = unit.tileX()`: port tiles span `[x*8, x*8+8)`.
        units.push(FogUnit {
            entity,
            tile_x: (pos.x / 8.0).floor() as i32,
            tile_y: (pos.y / 8.0).floor() as i32,
            radius: def.fog_radius as i32,
            team: team.team,
        });
    }
    (blocks, units)
}

/// `Building.fogRadius()` for every source kind (`BuildingComp.java:579-581`
/// plus the per-class overrides).
pub(crate) fn building_fog_radius(world: &World, def: &BlockDef, e: Entity) -> f32 {
    // `Radar.fogRadius()` (`Radar.java:55-57`).
    if let Some(state) = world.get::<crate::entities::comp::building::RadarState>(e) {
        return def.fog_radius as f32 * state.progress * state.smooth_efficiency;
    }
    // `Turret.fogRadius()` (`Turret.java:426-428`): `(range + (hasAmmo ?
    // peekAmmo().rangeChange : 0)) / tilesize * fogRadiusMultiplier`.
    if let Some(state) = world.get::<crate::world::blocks::defense::turrets::TurretState>(e) {
        let range = if crate::world::blocks::defense::turrets::has_ammo(world, e) {
            crate::world::blocks::defense::turrets::turret_range(world, e)
        } else {
            state.config.range
        };
        return range / 8.0 * state.config.fog_radius_multiplier;
    }
    // Cores: `init_self` folds `CoreBlock.java:158-159` into `def.fog_radius`.
    def.fog_radius as f32
}

/// `FogControl.forceUpdate(team, build)` (`FogControl.java:177-187`): marks the
/// dynamic flush pending and pushes the changed source's static event.
pub(crate) fn fog_force_update(world: &mut World, e: Entity) {
    let Some(rules) = rules_of(world) else {
        return;
    };
    if !rules.fog {
        return;
    }
    let static_fog = rules.static_fog;
    let source = {
        let Some(building) = world.get::<Building>(e) else {
            return;
        };
        let Some(team) = world.get::<TeamComp>(e).map(|team| team.team) else {
            return;
        };
        let Some(table) = world.get_resource::<crate::world::BlockTable>() else {
            return;
        };
        let Some(instance) = table.instance(building.block) else {
            return;
        };
        let radius = (building_fog_radius(world, &instance.def, e) + 0.5).floor() as i32;
        FogSource {
            x: building.tile.x() as i32,
            y: building.tile.y() as i32,
            radius,
            team,
        }
    };
    if let Some(mut fog) = world.get_resource_mut::<FogControl>() {
        fog.on_source_changed(source, static_fog);
    }
}

#[cfg(test)]
mod tests {
    use super::super::combat_tests;

    /// T1/H03 seam: `fog_static_bytes` and `fog_sources` expose the live
    /// static bits and dynamic vision circles.
    #[test]
    fn fog_view_seams_expose_static_bits_and_sources() {
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let mut sim = combat_tests::live_sim(32);
        let campaign = {
            let content = &sim.unit_runtime().expect("rt").content;
            Campaign::from_registry(content, &EmptyNeighborhood)
        };
        let mut rules = Rules {
            default_team: 1,
            ..Rules::default()
        };
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        rules.disable_unit_cap = true;
        sim.ecs
            .0
            .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
        sim.reset_fog();
        sim.spawn_live_unit("dagger", 1, 10.0 * 8.0 + 4.0, 10.0 * 8.0 + 4.0, 0.0)
            .expect("dagger");
        sim.tick().expect("tick");

        let sources = sim.fog_sources(1);
        assert_eq!(sources.len(), 1, "one unit source");
        let (sx, sy, radius) = sources[0];
        assert_eq!((sx, sy), (10.0 * 8.0 + 4.0, 10.0 * 8.0 + 4.0));
        assert!((radius - 21.0 * 8.0).abs() < 1e-3, "radius {radius}");

        let (ww, wh, bytes) = sim.fog_static_bytes(1).expect("static bits");
        assert_eq!((ww, wh), (32, 32));
        assert_eq!(bytes.len(), 32 * 32);
        assert_eq!(bytes[10 + 10 * 32], 255, "the unit tile is discovered");
        assert_eq!(bytes[31 + 31 * 32], 0, "the far corner is not");
    }
}

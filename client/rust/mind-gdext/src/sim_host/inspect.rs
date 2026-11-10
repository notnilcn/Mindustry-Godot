// SPDX-License-Identifier: GPL-3.0-only

//! MCP/debug inspection probes and world-generation counters.
//! Upstream: none (port-only; MCP/debug inspection API)

use super::*;

/// `(x, y)` tile-coordinate list for the `world_gen_debug` probe.
pub(super) type GenCoords = Vec<(i32, i32)>;

/// Counts generated ore overlays (`OreBlock`) and boulder props (the
/// `StaticProp` blocks named `*-boulder`) on a loaded grid, returning the
/// counts plus a few ore and boulder tile coordinates (J01 T1 debug probe; the
/// generation filters' observable output).
pub(super) fn count_world_gen(
    grid: &WorldGrid,
    content: &ContentRegistry,
) -> (i64, i64, GenCoords, GenCoords, Vec<(String, i64)>) {
    let mut ore = 0i64;
    let mut boulders = 0i64;
    let mut ore_sample: GenCoords = Vec::new();
    let mut boulder_sample: GenCoords = Vec::new();
    let mut boulder_blocks: std::collections::BTreeMap<String, i64> =
        std::collections::BTreeMap::new();
    for (pos, index) in grid.iter_row_major() {
        let tile = grid.tile_ref(index);
        if content
            .block(tile.overlay)
            .is_some_and(|def| def.kind == BlockKind::OreBlock)
        {
            ore += 1;
            if ore_sample.len() < 8 {
                ore_sample.push((i32::from(pos.x()), i32::from(pos.y())));
            }
        }
        if content
            .block(tile.block)
            .is_some_and(|def| def.kind == BlockKind::StaticProp && def.name.contains("boulder"))
        {
            boulders += 1;
            if let Some(def) = content.block(tile.block) {
                *boulder_blocks.entry(def.name.clone()).or_insert(0) += 1;
            }
            if boulder_sample.len() < 8 {
                boulder_sample.push((i32::from(pos.x()), i32::from(pos.y())));
            }
        }
    }
    (
        ore,
        boulders,
        ore_sample,
        boulder_sample,
        boulder_blocks.into_iter().collect(),
    )
}

/// Packs `(x, y)` pairs into a flat `PackedInt32Array` (debug probes).
pub(super) fn pack_coords(coords: GenCoords) -> PackedInt32Array {
    let mut packed = PackedInt32Array::new();
    for (x, y) in coords {
        packed.push(x);
        packed.push(y);
    }
    packed
}

impl MindSimHost {
    /// `InputHandler.selectedUnit()`'s ControlBlock branch
    /// (`InputHandler.java:2159-2161`, I01 T12): the detached `block` unit of an
    /// own controllable turret at `(x, y)` as its sim `EntitySeq`. `None` when
    /// the tile holds no turret, the team differs, or the turret is the
    /// player's own unit already. Rust-only seam (`#[func]`-free, like
    /// `building_team`); the desktop input calls it through `bind()`.
    pub fn control_block_unit_id(&mut self, x: i32, y: i32) -> Option<i32> {
        if !self.sim.grid.tiles.in_bounds(x, y) {
            return None;
        }
        self.sim
            .control_block_unit_id(TilePos::new(x as i16, y as i16))
    }

    /// Discovered (static) fog bits for `team`, one byte per tile (`0`/`255`,
    /// row-major, y up), or `None` when fog is off (H03 T12 render seam).
    pub fn fog_static_bytes(&self, team: u8) -> Option<(i32, i32, Vec<u8>)> {
        self.sim.fog_static_bytes(team)
    }

    /// Dynamic vision sources for `team`: `(world x, world y, radius px)`
    /// (H03 T12 render seam; radius = fog radius in tiles * 8).
    pub fn fog_sources(&self, team: u8) -> Vec<(f32, f32, f32)> {
        self.sim.fog_sources(team)
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Debug seam (G01 T19): apply a status by name to a unit by `EntitySeq`
    /// (`apply_sim_command_json` has no status op).
    #[func]
    pub fn debug_apply_status(&mut self, seq: i64, status: GString, ticks: f64) -> bool {
        self.sim
            .debug_apply_status(seq, &status.to_string(), ticks as f32)
    }

    /// Building state at `(x, y)` (debug seam: `{block, enabled, efficiency,
    /// potential_efficiency, optional_efficiency, time_scale}` plus
    /// `{item, liquid, accumulator, emitted}` for sandbox sources/voids).
    #[func]
    pub fn building_debug(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::<GString, Variant>::new();
        let entity = self.build_at(x, y);
        let Some(entity) = entity else {
            return out;
        };
        let world = self.world();
        let Some(building) = world.get::<mind_core::entities::comp::Building>(entity) else {
            return out;
        };
        {
            let mut put = |key: &str, value: Variant| {
                out.set(key, &value);
            };
            put("block", Variant::from(i64::from(building.block.raw())));
            put("rotation", Variant::from(i64::from(building.rotation)));
            put("enabled", Variant::from(building.enabled));
            if let Some(team) = world.get::<mind_core::entities::comp::TeamComp>(entity) {
                put("team", Variant::from(i64::from(team.team)));
            }
            put("efficiency", Variant::from(f64::from(building.efficiency)));
            put(
                "potential_efficiency",
                Variant::from(f64::from(building.potential_efficiency)),
            );
            put(
                "optional_efficiency",
                Variant::from(f64::from(building.optional_efficiency)),
            );
            put("time_scale", Variant::from(f64::from(building.time_scale)));
            // Per-kind body readback after a load (PAR-0161): drill progress,
            // crafter progress and turret ammo.
            if let Some(state) = world.get::<mind_core::entities::comp::DrillState>(entity) {
                put("drill_progress", Variant::from(f64::from(state.progress)));
                put("drill_warmup", Variant::from(f64::from(state.warmup)));
            }
            if let Some(state) = world.get::<mind_core::entities::comp::CrafterState>(entity) {
                put("crafter_progress", Variant::from(f64::from(state.progress)));
                put("crafter_warmup", Variant::from(f64::from(state.warmup)));
            }
            if let Some(state) =
                world.get::<mind_core::world::blocks::defense::turrets::TurretState>(entity)
            {
                put("turret_total_ammo", Variant::from(state.total_ammo));
            }
            // Fog discovery progress (`Radar.progress`; H09 T3 readback).
            if let Some(state) = world.get::<mind_core::entities::comp::RadarState>(entity) {
                put("radarProgress", Variant::from(f64::from(state.progress)));
            }
            // In-progress construction (`ConstructBuild`; PAR-0098 §2d).
            if let Some(state) = world.get::<mind_core::world::construct::ConstructState>(entity) {
                put(
                    "construct_current",
                    Variant::from(i64::from(state.current.raw())),
                );
                put(
                    "construct_previous",
                    Variant::from(i64::from(state.previous.raw())),
                );
                put(
                    "construct_progress",
                    Variant::from(f64::from(state.progress)),
                );
                put(
                    "construct_deconstruct",
                    Variant::from(state.active_deconstruct),
                );
            }
            // Item module contents for `has_items` blocks (`{item: amount}`).
            if let Some(content) = self.content_snapshot.as_ref()
                && let Some(json) = super::config::block_items_json(&self.sim, content, x, y)
            {
                put("items_json", Variant::from(json.as_str()));
            }
            if let Some(state) = world.get::<mind_core::entities::comp::SandboxState>(entity) {
                put(
                    "sandbox_item",
                    state
                        .item
                        .map(|item| Variant::from(i64::from(item.raw())))
                        .unwrap_or(Variant::nil()),
                );
                put(
                    "sandbox_liquid",
                    state
                        .liquid
                        .map(|liquid| Variant::from(i64::from(liquid.raw())))
                        .unwrap_or(Variant::nil()),
                );
                put("accumulator", Variant::from(f64::from(state.accumulator)));
                put("emitted", Variant::from(i64::from(state.emitted)));
            }
            // Force projector state (`ForceProjector.java:166-167`; H02 T7).
            if let Some(state) =
                world.get::<mind_core::world::blocks::defense::shields::ForceProjectorState>(entity)
            {
                put("force_radscl", Variant::from(f64::from(state.radscl)));
                put("force_buildup", Variant::from(f64::from(state.buildup)));
                put("force_broken", Variant::from(state.broken));
                put(
                    "force_phase_heat",
                    Variant::from(f64::from(state.phase_heat)),
                );
            }
        }
        out
    }

    /// Power network values at `(x, y)` (`{graph_id, status, stored, init}`).
    #[func]
    pub fn power_debug(&self, x: i32, y: i32) -> Dictionary<GString, f64> {
        let mut out = Dictionary::<GString, f64>::new();
        let entity = self.build_at(x, y);
        let Some(entity) = entity else {
            return out;
        };
        let world = self.world();
        if let Some(module) = world.get::<PowerModule>(entity) {
            out.set("status", module.status as f64);
            out.set("stored", module.stored as f64);
            out.set("init", if module.init { 1.0 } else { 0.0 });
            if let Some(graph) = world
                .get_resource::<PowerGrids>()
                .and_then(|grids| grids.graph(module.graph))
            {
                out.set("graph_id", graph.debug_id as f64);
                out.set("satisfaction", graph.get_satisfaction() as f64);
            }
        }
        out
    }

    /// Liquid values at `(x, y)` (`{current, has_any}`).
    #[func]
    pub fn liquid_debug(&self, x: i32, y: i32) -> Dictionary<GString, f64> {
        let mut out = Dictionary::<GString, f64>::new();
        let entity = self.build_at(x, y);
        let Some(entity) = entity else {
            return out;
        };
        if let Some(module) = self.world().get::<LiquidModule>(entity) {
            out.set("current", module.current_amount as f64);
            out.set("has_any", if module.has_any() { 1.0 } else { 0.0 });
        }
        out
    }

    /// Heat values at `(x, y)` (`{heat, heat_output, frac}`).
    #[func]
    pub fn heat_debug(&self, x: i32, y: i32) -> Dictionary<GString, f64> {
        let mut out = Dictionary::<GString, f64>::new();
        let entity = self.build_at(x, y);
        let Some(entity) = entity else {
            return out;
        };
        if let Some(heat) = self.world().get::<HeatState>(entity) {
            out.set("heat", heat.heat as f64);
            out.set("heat_output", heat.heat_output as f64);
            out.set(
                "frac",
                mind_core::world::blocks::heat::heat_frac(heat.heat, heat.heat_output) as f64,
            );
        }
        out
    }

    /// Playtest seam (H01 T3): kills the unit with `EntitySeq == seq`; the
    /// next tick's `resolve_deaths` runs the normal death path (respawn timer,
    /// wreck for flyers, kill stats).
    #[func]
    pub fn debug_kill_unit(&mut self, seq: i64) -> bool {
        self.sim.debug_kill_unit(seq)
    }

    /// Playtest seam: kills a building (`Health.health = 0`); the next tick's
    /// `resolve_deaths` destroys it and pushes the ghost plan.
    #[func]
    pub fn debug_kill_building(&mut self, x: i32, y: i32) -> bool {
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
            return false;
        };
        let Some(entity) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return false;
        };
        let Some(mut health) = self
            .world_mut()
            .get_mut::<mind_core::entities::comp::Health>(entity)
        else {
            return false;
        };
        health.health = 0.0;
        true
    }

    /// Playtest seam: starts a fire at `(x, y)` (`Fires.create`) through the
    /// live unit runtime rng. `false` when fire is disabled or there is no
    /// runtime.
    #[func]
    pub fn debug_create_fire(&mut self, x: i32, y: i32) -> bool {
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
            return false;
        };
        let Some(mut runtime) = self
            .world_mut()
            .remove_non_send::<mind_core::sim::unit_runtime::UnitRuntime>()
        else {
            return false;
        };
        let created =
            mind_core::combat::fires::create(self.world_mut(), tx, ty, &mut runtime.rng).is_some();
        self.world_mut().insert_non_send(runtime);
        created
    }

    /// Playtest seam: deposits `amount` of `liquid` at `(x, y)`
    /// (`Puddles.deposit`) through the live unit runtime rng. `false` when the
    /// liquid is unknown, the deposit reacts/vaporizes, or there is no runtime.
    #[func]
    pub fn debug_deposit_puddle(&mut self, x: i32, y: i32, liquid: GString, amount: f64) -> bool {
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
            return false;
        };
        let Some(id) = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.liquid_id(liquid.to_string().as_str()))
        else {
            return false;
        };
        let Some(mut runtime) = self
            .world_mut()
            .remove_non_send::<mind_core::sim::unit_runtime::UnitRuntime>()
        else {
            return false;
        };
        // Field path: the world is borrowed mutably below while `content` is
        // still live, which the `content_registry()` accessor (borrowing all
        // of `self`) would reject.
        let Some(content) = self.content_snapshot.as_ref() else {
            self.world_mut().insert_non_send(runtime);
            return false;
        };
        let created = mind_core::combat::puddles::deposit(
            // Field path: `self.sim.grid.tiles` and `content` are borrowed
            // alongside the world.
            &mut self.sim.ecs.0,
            &self.sim.grid.tiles,
            content,
            tx,
            ty,
            id,
            amount as f32,
            &mut runtime.rng,
        )
        .is_some();
        self.world_mut().insert_non_send(runtime);
        created
    }

    /// Playtest seam: force-spawns the player unit without the sector env gate
    /// (`CoreBlock.requestSpawn` refuses alpha when `rules.env` wrongly carries
    /// scorching — G01's recorded divergence). Lets input-path probes run on
    /// Ground Zero; the real spawn stays `spawn_player_unit()`.
    #[func]
    pub fn debug_spawn_player(&mut self, name: GString, team: i64, x: f32, y: f32) -> bool {
        self.sim
            .spawn_player_unit(&name.to_string(), team.max(0) as u8, x, y, 90.0)
            .is_some()
    }

    /// Debug/playtest seam: adds `amount` of `item` to the player team's first
    /// core (the Java console/sandbox `core.items.add`); returns the amount
    /// accepted (negative removes).
    #[func]
    pub fn debug_add_core_items(&mut self, item: GString, amount: i64) -> i64 {
        let team = self.player_team();
        let amount = i32::try_from(amount).unwrap_or(0);
        if amount < 0 {
            return -i64::from(self.remove_player_core_items(team, &item.to_string(), -amount));
        }
        let Some(item) = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.item_id(&item.to_string()))
        else {
            return 0;
        };
        let target = self
            .sim
            .ecs
            .entities_by_seq()
            .into_iter()
            .find_map(|(_, entity, comp)| {
                (comp.team.0 == team
                    && self.world().get::<ItemModule>(entity).is_some()
                    && self
                        .world()
                        .get::<mind_core::world::blocks::storage::core_block::CoreBuild>(entity)
                        .is_some())
                .then_some(entity)
            });
        let Some(entity) = target else {
            return 0;
        };
        let capacity = self
            .world()
            .get::<mind_core::world::blocks::storage::core_block::CoreBuild>(entity)
            .map(|core| core.storage_capacity)
            .unwrap_or(i32::MAX);
        match self.world_mut().get_mut::<ItemModule>(entity) {
            Some(mut module) => i64::from(module.add(item, amount, capacity)),
            None => 0,
        }
    }

    /// Debug/playtest seam: adds `amount` of `liquid` to the building at (x, y)
    /// (clamped to `Block.liquidCapacity`); returns the amount accepted.
    #[func]
    pub fn debug_add_building_liquid(
        &mut self,
        x: i32,
        y: i32,
        liquid: GString,
        amount: f64,
    ) -> f64 {
        let Some(entity) = self.sim.grid.tiles.getn(x, y).and_then(|tile| tile.build) else {
            return 0.0;
        };
        let Some(liquid) = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.liquid_id(&liquid.to_string()))
        else {
            return 0.0;
        };
        let capacity = self
            .world()
            .get_resource::<mind_core::world::block::BlockTable>()
            .and_then(|table| {
                self.world()
                    .get::<mind_core::entities::comp::Building>(entity)
                    .map(|building| building.block)
                    .and_then(|block| table.get(block).map(|inst| inst.def.liquid_capacity))
            })
            .unwrap_or(0.0);
        match self
            .world_mut()
            .get_mut::<mind_core::world::modules::LiquidModule>(entity)
        {
            Some(mut module) => f64::from(module.add(liquid, amount as f32, capacity)),
            None => 0.0,
        }
    }

    /// Debug/playtest seam: adds `amount` of `item` to the building at (x, y)
    /// (clamped to `Block.itemCapacity`); returns the amount accepted.
    #[func]
    pub fn debug_add_building_items(&mut self, x: i32, y: i32, item: GString, amount: i64) -> i64 {
        let Ok(amount) = i32::try_from(amount) else {
            return 0;
        };
        let Some(entity) = self.sim.grid.tiles.getn(x, y).and_then(|tile| tile.build) else {
            return 0;
        };
        let Some(item) = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.item_id(&item.to_string()))
        else {
            return 0;
        };
        let block = self
            .world()
            .get::<mind_core::entities::comp::Building>(entity)
            .map(|building| building.block);
        let capacity = block
            .and_then(|block| {
                self.world()
                    .get_resource::<mind_core::world::block::BlockTable>()
                    .and_then(|table| table.get(block).map(|inst| inst.def.item_capacity))
            })
            .unwrap_or(0);
        match self.world_mut().get_mut::<ItemModule>(entity) {
            Some(mut module) => i64::from(module.add(item, amount, capacity)),
            None => 0,
        }
    }

    /// T9: weather rules entries + active runtime state (playtest readback).
    #[func]
    pub fn weather_json(&self) -> GString {
        let content = self.sim.block_runtime().map(|runtime| runtime.content());
        let entries: Vec<serde_json::Value> = self
            .campaign_runtime()
            .map(|campaign| {
                campaign
                    .session
                    .rules
                    .weather
                    .iter()
                    .map(|entry| {
                        serde_json::json!({
                            "weather": entry.weather,
                            "cooldown": entry.cooldown,
                            "minFrequency": entry.min_frequency,
                            "maxFrequency": entry.max_frequency,
                            "minDuration": entry.min_duration,
                            "maxDuration": entry.max_duration,
                            "always": entry.always,
                            "intensity": entry.intensity,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut active: Vec<(u64, serde_json::Value)> = self
            .world()
            .iter_entities()
            .filter_map(|entity| {
                let state = entity.get::<mind_core::entities::comp::weather::WeatherRuntime>()?;
                let name = content
                    .and_then(|content| content.weather(state.weather))
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                let seq = entity
                    .get::<mind_core::ecs::EntitySeq>()
                    .map_or(u64::MAX, |seq| seq.0);
                Some((
                    seq,
                    serde_json::json!({
                        "weather": name,
                        "intensity": state.intensity,
                        "opacity": state.opacity,
                        "life": state.life,
                        "windX": state.wind_x,
                        "windY": state.wind_y,
                    }),
                ))
            })
            .collect();
        active.sort_by_key(|(seq, _)| *seq);
        let active: Vec<serde_json::Value> = active.into_iter().map(|(_, value)| value).collect();
        let value = serde_json::json!({
            "entries": entries,
            "active": active,
        });
        crate::gdutil::json::value_gstring(&value)
    }

    /// T9 test seam: force a weather (`Call.createWeather`), same shape as
    /// `weather_system`'s spawn. `false` when the name is unknown or a weather
    /// of that type is already active.
    #[func]
    pub fn debug_create_weather(&mut self, name: GString, intensity: f32, duration: f32) -> bool {
        let Some(weather) = self
            .sim
            .block_runtime()
            .map(|runtime| runtime.content())
            .and_then(|content| content.weather_by_name(&name.to_string()))
            .map(|def| def.id)
        else {
            log::warn!("debug_create_weather: unknown weather `{name}`");
            return false;
        };
        if self.world().iter_entities().any(|entity| {
            entity
                .get::<mind_core::entities::comp::weather::WeatherRuntime>()
                .is_some_and(|state| state.weather == weather)
        }) {
            return false;
        }
        let Some(mut runtime) = self
            .world_mut()
            .remove_non_send::<mind_core::sim::unit_runtime::UnitRuntime>()
        else {
            return false;
        };
        let angle = runtime
            .rng
            .range(mind_core::determinism::RngStream::Fx, 0.0, 360.0)
            .to_radians();
        let seq = runtime.seq;
        runtime.seq = seq.wrapping_add(1);
        mind_core::entities::comp::weather::create_weather(
            self.world_mut(),
            seq,
            weather,
            intensity,
            duration,
            (angle.cos(), angle.sin()),
        );
        self.world_mut().insert_non_send(runtime);
        true
    }

    /// Debug probe (PAR-0152): block name/kind at a tile and whether the tile
    /// holds a `ConstructState`. `sim_name` reads the sim's own content table
    /// and `construct` the render seam, so a mismatch between them is visible.
    #[func]
    pub fn tile_debug(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut d = Dictionary::new();
        // Off-map coordinates from MCP/input must not panic (`Tiles::get`
        // indexes unchecked).
        let Some(tile) = self.sim.grid.tiles.getn(x, y) else {
            return d;
        };
        let def = self
            .content_snapshot
            .as_ref()
            .and_then(|c| c.block(tile.block));
        let name = def
            .map(|def| def.name.clone())
            .unwrap_or_else(|| "?".into());
        let kind = def.map(|def| format!("{:?}", def.kind)).unwrap_or_default();
        let sim_name = self.sim.block_name_of(tile.block);
        let floor = self
            .content_snapshot
            .as_ref()
            .and_then(|c| c.block(tile.floor))
            .map(|def| def.name.clone())
            .unwrap_or_default();
        let overlay = self
            .content_snapshot
            .as_ref()
            .and_then(|c| c.block(tile.overlay))
            .map(|def| def.name.clone())
            .unwrap_or_default();
        d.set("raw", &(tile.block.raw() as i64).to_variant());
        d.set("name", &GString::from(name.as_str()).to_variant());
        d.set("kind", &GString::from(kind.as_str()).to_variant());
        d.set("sim_name", &GString::from(sim_name.as_str()).to_variant());
        d.set("floor", &GString::from(floor.as_str()).to_variant());
        d.set("overlay", &GString::from(overlay.as_str()).to_variant());
        d.set("has_build", &tile.build.is_some().to_variant());
        d.set(
            "construct",
            &self.construct_draw(x, y).is_some().to_variant(),
        );
        // Live building health (J04 T2 verification; debug probe only).
        match tile.build.and_then(|entity| {
            self.world()
                .get::<mind_core::entities::comp::health::Health>(entity)
        }) {
            Some(health) => {
                d.set("health", &health.health.to_variant());
                d.set("max_health", &health.max_health.to_variant());
            }
            None => {
                d.set("health", &Variant::nil());
                d.set("max_health", &Variant::nil());
            }
        }
        // Stored liquids `[{"liquid": name, "amount": f}]` (J04 T3 verification;
        // debug probe only).
        let mut liquid_rows: Vec<serde_json::Value> = Vec::new();
        if let Some(entity) = tile.build
            && let Some(module) = self
                .world()
                .get::<mind_core::world::modules::LiquidModule>(entity)
        {
            for (index, amount) in module.liquids.iter().enumerate() {
                if *amount <= 0.0 {
                    continue;
                }
                let name = self
                    .content_snapshot
                    .as_ref()
                    .and_then(|content| {
                        content.liquid(mind_core::content::LiquidId::new(index as u16))
                    })
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                liquid_rows.push(serde_json::json!({"liquid": name, "amount": amount}));
            }
        }
        d.set(
            "liquids",
            &crate::gdutil::json::value_gstring(&serde_json::Value::Array(liquid_rows))
                .to_variant(),
        );
        // `Rules.limitMapArea` play area (`World.getDarkness` border half);
        // `null` when unrestricted.
        match self.sim.grid.limit_area {
            Some((lx, ly, lw, lh)) => {
                let mut area = PackedInt32Array::new();
                area.push(lx);
                area.push(ly);
                area.push(lw);
                area.push(lh);
                d.set("limit_area", &area.to_variant());
            }
            None => d.set("limit_area", &Variant::nil()),
        }
        d
    }

    /// Debug probe (J01 T1): the loaded grid's generation facts — the recorded
    /// `World.FilterContext` seed (`-1` when the load ran no filters) plus ore
    /// overlay and boulder block counts and a few ore tile coordinates for
    /// `tile_debug` spot checks. Companion to `tile_debug`.
    #[func]
    pub fn world_gen_debug(&self) -> Dictionary<GString, Variant> {
        let mut d = Dictionary::new();
        let seed = self
            .sim
            .map_gen_seed()
            .map(|seed| seed as i64)
            .unwrap_or(-1);
        d.set("seed", &seed.to_variant());
        // Exact decimal form: the seed is a u64 and Godot `int` is signed, so a
        // draw above `i64::MAX` (common: `Time.get_ticks_usec()`-derived) wraps
        // in `seed`. `seed_u64` is the reproducible value for
        // `MindSimHost::set_next_map_gen_seed`.
        let seed_u64 = self
            .sim
            .map_gen_seed()
            .map(|seed| seed.to_string())
            .unwrap_or_default();
        d.set("seed_u64", &GString::from(seed_u64.as_str()).to_variant());
        let (ore, boulders, ore_sample, boulder_sample, boulder_blocks) =
            match self.content_snapshot.as_ref() {
                Some(content) => count_world_gen(&self.sim.grid, content),
                None => (0, 0, Vec::new(), Vec::new(), Vec::new()),
            };
        d.set("ore", &ore.to_variant());
        d.set("boulders", &boulders.to_variant());
        d.set("ore_tiles", &pack_coords(ore_sample).to_variant());
        d.set("boulder_tiles", &pack_coords(boulder_sample).to_variant());
        // Per-prop breakdown (the genfilter's own output mix, e.g.
        // `snow-boulder`/`basalt-boulder` for `groundZero`'s scatter stack).
        let mut blocks: Dictionary<GString, Variant> = Dictionary::new();
        for (name, count) in boulder_blocks {
            blocks.set(name.as_str(), &count.to_variant());
        }
        d.set("boulder_blocks", &blocks.to_variant());
        d
    }

    /// Fog controller state + counts, **no per-tile data** (I01 T15): the
    /// callable-with-no-arguments readback. `{fog, staticFog, teams:[id],
    /// sources:<live source count>, discovered:<set static bits>}`. Reports
    /// `{"fog":false}` without a `CampaignRuntime`.
    ///
    /// The T15 stall was a script error from calling the old 3-argument probe
    /// with no arguments (break-on-error wedges the frame loop); keep this
    /// entry cheap so a playtest can always probe the fog state.
    #[func]
    pub fn fog_debug_json(&self) -> GString {
        let Some(rules) = mind_core::game::runtime::rules_of(self.world()) else {
            return GString::from("{\"fog\":false}");
        };
        let mut teams: Vec<u8> = Vec::new();
        let mut discovered = 0usize;
        if let Some(fog) = self
            .world()
            .get_resource::<mind_core::game::fog::FogControl>()
        {
            for (id, data) in fog.fog.iter().enumerate() {
                if let Some(data) = data {
                    teams.push(id as u8);
                    discovered += data.static_data.count_set_bits();
                }
            }
        }
        let sources = self.sim.fog_sources(0).len().max(
            self.world()
                .get_resource::<mind_core::game::fog::FogControl>()
                .map_or(0, |fog| fog.fog.iter().flatten().count()),
        );
        let value = serde_json::json!({
            "fog": rules.fog,
            "staticFog": rules.static_fog,
            "teams": teams,
            "discoveredTiles": discovered,
            "sources": sources,
        });
        crate::gdutil::json::value_gstring(&value)
    }

    /// Single-tile fog probe (`FogControl.isVisibleTile`/`isDiscovered`):
    /// `{fog, staticFog, visible, discovered, teams:[id]}` (the pre-T15 3-arg
    /// readback, renamed).
    #[func]
    pub fn fog_debug_tile_json(&self, team: i32, x: i32, y: i32) -> GString {
        let Some(rules) = mind_core::game::runtime::rules_of(self.world()) else {
            return GString::from("{\"fog\":false}");
        };
        let team = (team & 0xFF) as u8;
        let mut teams: Vec<u8> = Vec::new();
        if let Some(fog) = self
            .world()
            .get_resource::<mind_core::game::fog::FogControl>()
        {
            for (id, data) in fog.fog.iter().enumerate() {
                if data.is_some() {
                    teams.push(id as u8);
                }
            }
        }
        let value = serde_json::json!({
            "fog": rules.fog,
            "staticFog": rules.static_fog,
            "visible": self.sim.fog_visible_tile(team, x, y),
            "discovered": self.sim.fog_discovered(team, x, y),
            "teams": teams,
        });
        crate::gdutil::json::value_gstring(&value)
    }

    /// Bounded per-tile fog rect probe (I01 T15): visibility + discovery for
    /// `w x h` tiles from `(x, y)`, clamped to 64x64 so a probe can never
    /// walk a whole large map.
    #[func]
    pub fn fog_debug_json_rect(&self, team: i32, x: i32, y: i32, w: i32, h: i32) -> GString {
        let team = (team & 0xFF) as u8;
        let w = w.clamp(0, 64);
        let h = h.clamp(0, 64);
        let mut rows: Vec<serde_json::Value> = Vec::with_capacity(h as usize);
        for ty in y..y.saturating_add(h) {
            let mut row: Vec<u8> = Vec::with_capacity(w as usize);
            for tx in x..x.saturating_add(w) {
                let visible = self.sim.fog_visible_tile(team, tx, ty);
                let discovered = self.sim.fog_discovered(team, tx, ty);
                row.push(match (visible, discovered) {
                    (true, _) => 1,
                    (false, true) => 2,
                    (false, false) => 0,
                });
            }
            rows.push(serde_json::Value::Array(
                row.into_iter().map(serde_json::Value::from).collect(),
            ));
        }
        let value = serde_json::json!({ "x": x, "y": y, "w": w, "h": h, "rows": rows });
        crate::gdutil::json::value_gstring(&value)
    }

    /// Debug: forces `rules.fog`/`rules.staticFog` on the live session and
    /// re-initialises fog. `false` without a `CampaignRuntime`.
    #[func]
    pub fn debug_set_fog(&mut self, fog: bool, static_fog: bool) -> bool {
        {
            let Some(mut runtime) = self.campaign_runtime_mut() else {
                return false;
            };
            runtime.session.rules.fog = fog;
            runtime.session.rules.static_fog = static_fog;
        }
        self.sim.reset_fog();
        true
    }

    /// Debug: `Tile.setTeam` for the building at `(x, y)` (`BuildingComp.changeTeam`
    /// -> `checkAllowUpdate`); `false` when the tile has no building.
    #[func]
    pub fn debug_set_building_team(&mut self, x: i32, y: i32, team: i32) -> bool {
        self.sim.debug_set_building_team(x, y, (team & 0xFF) as u8)
    }
}

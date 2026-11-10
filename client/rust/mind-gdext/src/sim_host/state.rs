// SPDX-License-Identifier: GPL-3.0-only

//! State, checksum and hint JSON views of the sim.
//! Upstream: core/src/mindustry/ui/fragments/HintsFragment.java (hint state)

use super::*;

/// `HintsFragment` poll data (`HintsFragment.java:74-102`; T4a/T4b inputs).
/// Read-only and cheap: one building-entity pass plus one grid pass.
pub(super) fn hint_state_value(
    sim: &Sim,
    content: Option<&ContentRegistry>,
    team: u8,
) -> serde_json::Value {
    let block_name = |id: BlockId| {
        content
            .and_then(|content| content.block(id))
            .map(|def| def.name.clone())
            .unwrap_or_default()
    };

    // `Trigger.placedBlocks` / `events` sets.
    let placed: Vec<String> = sim
        .hint_events
        .placed_by_player
        .iter()
        .map(|id| block_name(*id))
        .collect();
    let events: Vec<&str> = sim.hint_events.events.iter().copied().collect();

    // Player unit inputs (`UnitCore`/`ItemsComp`/`UnitPayloads`).
    let (player_dead, spawned_by_core, player_has_item, player_payload, player_can_boost) =
        hint_player_inputs(sim, content);
    // Live session counters (`HintsFragment` reads `state.wave`/wave team AI).
    let runtime = sim
        .ecs
        .0
        .get_resource::<mind_core::game::runtime::CampaignRuntime>();
    let (enemies, wave) = runtime
        .map(|runtime| (runtime.session.enemies as i64, runtime.session.wave as i64))
        .unwrap_or((0, 0));

    // Team unit count (`TeamData.units`).
    let team_units = hint_team_units(sim, team);
    // Building counters + the first core's identity (`state.teams.get(team)`).
    let HintBuildings {
        conveyors,
        cores,
        fabricators,
        extinguishers,
        core_block,
        core_capacity,
        core_copper,
        core_has_foundation_cost,
        core_zone_present,
    } = hint_building_pass(sim, content, team);
    // `Trigger.blockInfo`-style live plan count (the player's build queue).
    let plans = sim.player_build_plans().len() as i64;

    // Campaign facts for the T4b predicates (live runtime campaign + rules).
    let HintCampaign {
        attack_mode,
        serpulo,
        team_rebuild_plans,
        fungal_pass_saved,
        tar_fields_captured,
        sector_threat,
        cores_destroyed,
        boss,
    } = hint_campaign_facts(sim, content, team, runtime);
    serde_json::json!({
        "placed": placed,
        "events": events,
        "player_dead": player_dead,
        "player_has_item": player_has_item,
        "player_can_boost": player_can_boost,
        "player_spawned_by_core": spawned_by_core,
        "player_payload": player_payload,
        "team_units": team_units,
        "enemies": enemies,
        "wave": wave,
        "conveyors": conveyors,
        "plans": plans,
        "core_block": core_block,
        "core_copper": core_copper,
        "core_capacity": core_capacity,
        "core_has_foundation_cost": core_has_foundation_cost,
        "cores": cores,
        "fabricators": fabricators,
        "extinguishers": extinguishers,
        "core_zone_present": core_zone_present,
        "cores_destroyed": cores_destroyed,
        "boss": boss,
        "team_rebuild_plans": team_rebuild_plans,
        "serpulo": serpulo,
        "fungal_pass_saved": fungal_pass_saved,
        "tar_fields_captured": tar_fields_captured,
        "sector_threat": sector_threat,
        "attack_mode": attack_mode,
    })
}

/// Building-pass counters for the hint predicates (`state.teams.get(team)`).
struct HintBuildings {
    conveyors: i64,
    cores: i64,
    fabricators: i64,
    extinguishers: i64,
    core_block: String,
    core_capacity: i64,
    core_copper: i64,
    core_has_foundation_cost: bool,
    core_zone_present: bool,
}

/// Campaign facts for the T4b hint predicates (live runtime campaign + rules).
struct HintCampaign {
    attack_mode: bool,
    serpulo: bool,
    team_rebuild_plans: i64,
    fungal_pass_saved: bool,
    tar_fields_captured: bool,
    sector_threat: f64,
    cores_destroyed: i64,
    boss: serde_json::Value,
}

/// Player-unit inputs for the hint predicates (`UnitCore`/`ItemsComp`/
/// `UnitPayloads`): `(dead, spawned_by_core, has_item, payload_count,
/// can_boost)`.
fn hint_player_inputs(
    sim: &Sim,
    content: Option<&ContentRegistry>,
) -> (bool, bool, bool, i64, bool) {
    use mind_core::entities::comp::unit::comp::{ItemsComp, UnitCore, UnitPayloads, UnitTypeComp};
    match sim.player_unit() {
        Some(entity) => {
            let core = sim.ecs.0.get::<UnitCore>(entity);
            let items = sim.ecs.0.get::<ItemsComp>(entity);
            let payloads = sim.ecs.0.get::<UnitPayloads>(entity);
            let can_boost = sim
                .ecs
                .0
                .get::<UnitTypeComp>(entity)
                .and_then(|comp| content.and_then(|content| content.unit(comp.type_id)))
                .is_some_and(|def| def.can_boost);
            (
                core.map(|core| core.dead).unwrap_or(false),
                core.map(|core| core.spawned_by_core).unwrap_or(false),
                items.and_then(|items| items.item).is_some(),
                payloads
                    .map(|payloads| payloads.stack.len() as i64)
                    .unwrap_or(-1),
                can_boost,
            )
        }
        None => (true, false, false, -1, false),
    }
}

/// Team unit count (`TeamData.units`).
fn hint_team_units(sim: &Sim, team: u8) -> i64 {
    use mind_core::entities::comp::TeamComp;
    sim.ecs
        .0
        .get_non_send::<mind_core::sim::unit_runtime::UnitRuntime>()
        .map(|runtime| {
            runtime
                .units
                .iter()
                .filter(|entity| {
                    sim.ecs
                        .0
                        .get::<TeamComp>(**entity)
                        .is_some_and(|comp| comp.team == team)
                })
                .count() as i64
        })
        .unwrap_or(0)
}

/// Building counters, the first core's identity and `core-zone` presence.
fn hint_building_pass(sim: &Sim, content: Option<&ContentRegistry>, team: u8) -> HintBuildings {
    use mind_core::content::BlockFlag;
    let mut conveyors = 0i64;
    let mut cores = 0i64;
    let mut fabricators = 0i64;
    let mut extinguishers = 0i64;
    let mut core_block = String::new();
    let mut core_capacity = 0i64;
    let mut core_copper = 0i64;
    let mut core_has_foundation_cost = false;
    let copper_id = content.and_then(|content| content.item_id("copper"));
    let foundation = content.and_then(|content| content.block_by_name("core-foundation"));
    for (_seq, entity, comp) in sim.ecs.entities_by_seq() {
        let Some(def) = content.and_then(|content| content.block(comp.block)) else {
            continue;
        };
        if comp.team.0 == team
            && matches!(
                def.kind,
                BlockKind::Conveyor | BlockKind::ArmoredConveyor | BlockKind::StackConveyor
            )
        {
            conveyors += 1;
        }
        if def.name == "tank-fabricator" || def.name == "ground-factory" {
            fabricators += 1;
        }
        if def.flags.contains(&BlockFlag::Extinguisher) {
            extinguishers += 1;
        }
        if def.kind != BlockKind::CoreBlock || comp.team.0 != team {
            continue;
        }
        cores += 1;
        if !core_block.is_empty() {
            continue;
        }
        core_block = def.name.clone();
        core_capacity = def.item_capacity as i64;
        if let Some(module) = sim.ecs.0.get::<ItemModule>(entity) {
            if let Some(copper) = copper_id {
                core_copper = module.get(copper) as i64;
            }
            core_has_foundation_cost = foundation.is_some_and(|def| {
                def.requirements
                    .iter()
                    .all(|stack| module.get(stack.item) >= stack.amount)
            });
        }
    }

    // `Indexer.isBlockPresent(Blocks.coreZone)`; checked across the tile's
    // block/floor/overlay layers.
    let core_zone_present = content
        .and_then(|content| content.block_id("core-zone"))
        .is_some_and(|zone| {
            sim.grid.iter_row_major().any(|(pos, _)| {
                sim.grid
                    .tiles
                    .getn(pos.x() as i32, pos.y() as i32)
                    .is_some_and(|tile| {
                        tile.block == zone || tile.floor == zone || tile.overlay == zone
                    })
            })
        });

    HintBuildings {
        conveyors,
        cores,
        fabricators,
        extinguishers,
        core_block,
        core_capacity,
        core_copper,
        core_has_foundation_cost,
        core_zone_present,
    }
}

/// Campaign facts for the T4b predicates (live runtime campaign + rules).
fn hint_campaign_facts(
    sim: &Sim,
    content: Option<&ContentRegistry>,
    team: u8,
    runtime: Option<&CampaignRuntime>,
) -> HintCampaign {
    let rules = mind_core::game::runtime::rules_of(&sim.ecs.0);
    let attack_mode = rules.is_some_and(|rules| rules.attack_mode);
    // `!rules.hasEnv(Env.scorching)` (`world/meta/Env.java`; the one bit table
    // is `game::planet::env_bit`).
    let serpulo = rules.is_none_or(|rules| {
        rules.env & mind_core::game::planet::env_bit(mind_core::content::EnvFlag::Scorching) == 0
    });
    let team_rebuild_plans = runtime
        .and_then(|runtime| {
            runtime
                .session
                .teams
                .get_or_null(mind_core::ecs::TeamId(team))
        })
        .map(|data| data.plans.len() as i64)
        .unwrap_or(0);
    let sector_fact = |name: &str| -> (bool, bool) {
        let (Some(registry), Some(campaign)) = (content, runtime.map(|r| &r.campaign)) else {
            return (false, false);
        };
        let Some(preset) = registry.sector_by_name(name) else {
            return (false, false);
        };
        let Some(record) = campaign.sector(preset.planet, preset.sector) else {
            return (false, false);
        };
        (record.has_save(), record.is_captured(None))
    };
    let (fungal_pass_saved, _) = sector_fact("fungalPass");
    let (_, tar_fields_captured) = sector_fact("tarFields");
    let sector_threat = runtime
        .and_then(|runtime| runtime.session.sector)
        .and_then(|(planet, sector)| {
            runtime.and_then(|runtime| runtime.campaign.sector(planet, sector))
        })
        .map(|record| record.threat as f64)
        .unwrap_or(0.0);

    // `state.stats.getDestroyed(...)` sum (`HintsFragment.java:285`,
    // `serpuloCoreZone`): the session's counters are name-keyed since H09 T10.
    let cores_destroyed = runtime
        .map(|runtime| {
            let stats = &runtime.session.stats;
            i64::from(stats.get_destroyed("core-shard"))
                + i64::from(stats.get_destroyed("core-foundation"))
                + i64::from(stats.get_destroyed("core-nucleus"))
        })
        .unwrap_or(0);
    let boss: serde_json::Value =
        serde_json::from_str(&sim.boss_json()).unwrap_or(serde_json::Value::Null);

    HintCampaign {
        attack_mode,
        serpulo,
        team_rebuild_plans,
        fungal_pass_saved,
        tar_fields_captured,
        sector_threat,
        cores_destroyed,
        boss,
    }
}

impl MindSimHost {
    /// Plan-21 scoped checksum value (Rust-only seam for relay publication).
    pub fn checksum_scoped_value(&self) -> u64 {
        self.sim
            .checksum_scoped(&mind_core::sim::ChecksumScope::relay())
    }

    /// I02 T4: drains the per-tick `flashHit` wall-collision log as
    /// `[(tile_x, tile_y)]`. Rust-only seam for `render/blocks.rs`, which owns
    /// the `hit` decay (`Wall.WallBuild.draw`, `Wall.java:146-161`).
    pub fn drain_wall_flashes(&mut self) -> Vec<(i16, i16)> {
        self.sim.drain_wall_flashes()
    }

    /// Read-only content registry snapshot for block draw metadata.
    pub fn content_registry(&self) -> Option<&ContentRegistry> {
        self.content_snapshot.as_ref()
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Canonical sparse state dump as pretty JSON (same schema as `mind-headless`).
    #[func]
    pub fn get_state_json(&self) -> GString {
        match self.sim.dump_json(false) {
            Ok(json) => GString::from(json.as_str()),
            Err(err) => {
                log::error!("get_state_json: dump failed: {err}");
                crate::gdutil::json::empty_object()
            }
        }
    }

    /// Current checksum as 16 lowercase hex digits.
    #[func]
    pub fn get_checksum(&self) -> GString {
        GString::from(self.sim.checksum_hex().as_str())
    }

    /// Plan-21 checksum-scope hook (relay scope; the mask is a no-op until plan
    /// 05 adds possessed-unit fields).
    #[func]
    pub fn get_checksum_scoped(&self) -> GString {
        GString::from(format!("{:016x}", self.checksum_scoped_value()).as_str())
    }

    /// `state.rules.defaultTeam` of the live session (campaign rules, then the
    /// build guard resource). The RTS input layer filters selections by it
    /// (`player.team().id`; PAR-0140).
    #[func]
    pub fn default_team(&self) -> i64 {
        self.sim.live_default_team() as i64
    }

    /// `Teams.bosses`/`GameState.boss()` seam (G01 T19): G03's boss bar reads
    /// count/names/value, G05's guardian hint reads `first`.
    #[func]
    pub fn boss_json(&self) -> GString {
        GString::from(self.sim.boss_json().as_str())
    }

    /// Per-type content counts for the inspector `Content` tab (plan 02 §3.7).
    /// Read-only; plan 05 moves this to the sim-owned `Content` resource.
    #[func]
    pub fn content_counts(&self) -> Dictionary<GString, i64> {
        let mut out = Dictionary::<GString, i64>::new();
        let Some(registry) = self.content_snapshot.as_ref() else {
            return out;
        };
        for (type_, count) in content_counts(registry) {
            out.set(type_.name(), count as i64);
        }
        out
    }

    /// Ordered content names of `type_name` (`content_list("item")`, plan 02
    /// §3.7). Unknown types return an empty array; read-only.
    #[func]
    pub fn content_list(&self, type_name: GString) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        let Some(registry) = self.content_snapshot.as_ref() else {
            return out;
        };
        let requested = type_name.to_string();
        let Some(type_) = ContentType::ALL
            .iter()
            .copied()
            .find(|type_| type_.name() == requested)
        else {
            return out;
        };
        for entry in registry.entries(type_) {
            if let Some(name) = entry.name {
                out.push(&GString::from(name));
            }
        }
        out
    }

    /// FNV-1a-64 over the registered content names
    /// (`ContentRegistry::content_hash`); the relay build gate compares it
    /// between host and joiner (I08 T4). 0 when no registry is loaded.
    #[func]
    pub fn content_hash(&self) -> i64 {
        self.content_snapshot
            .as_ref()
            .map(|registry| registry.content_hash() as i64)
            .unwrap_or(0)
    }

    /// Sets the sim tick counter (snapshot restore / late join, I08 T5; the
    /// save-load path sets the same field at `sim_io.rs:1325`).
    #[func]
    pub fn set_sim_tick(&mut self, tick: i64) {
        self.sim.state.tick = tick.max(0) as u64;
    }

    /// Saves the live world through the plan-05 IoSet handler and returns the
    /// `.msav` bytes (I08 T4/T5 host snapshot encoder). `flush_io` runs the
    /// save boundary without advancing the tick (`io_set.rs:227`), so the
    /// bytes are ready when this returns; empty on failure.
    #[func]
    pub fn encode_snapshot(&mut self) -> PackedByteArray {
        let relative = GString::from("user://mp/host-snapshot.msav");
        let path = globalize(&relative);
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            log::warn!("encode_snapshot: cannot create {}", parent.display());
            return PackedByteArray::new();
        }
        self.sim.request_save(path.clone(), false);
        self.sim.flush_io();
        match std::fs::read(&path) {
            Ok(bytes) => PackedByteArray::from(bytes.as_slice()),
            Err(error) => {
                log::warn!("encode_snapshot: read {} failed: {error}", path.display());
                PackedByteArray::new()
            }
        }
    }

    /// Completed sim ticks.
    #[func]
    pub fn get_tick(&self) -> i64 {
        self.sim.tick_count() as i64
    }

    /// Monotonic world-mutation counter (renderer chunk invalidation).
    #[func]
    pub fn world_revision(&self) -> i64 {
        self.revision as i64
    }

    /// Monotonic update counter (`GameState.updateId`).
    #[func]
    pub fn get_update_id(&self) -> i64 {
        self.sim.update_id() as i64
    }

    /// Phase name (`menu` / `playing` / `paused`).
    #[func]
    pub fn get_state(&self) -> GString {
        GString::from(self.sim.state_name())
    }

    /// Live per-group entity counts for the inspector (plan 05 M9).
    #[func]
    pub fn get_group_counts(&self) -> Dictionary<GString, i64> {
        let mut out = Dictionary::<GString, i64>::new();
        for (name, count) in self.sim.group_counts() {
            out.set(name, count as i64);
        }
        out
    }

    /// One group count by name (`0` when the group is unknown).
    #[func]
    pub fn get_group_count(&self, name: GString) -> i64 {
        let requested = name.to_string();
        self.sim
            .group_counts()
            .get(requested.as_str())
            .copied()
            .unwrap_or(0) as i64
    }

    /// Whether the fixed-step pump is halted.
    #[func]
    pub fn is_paused(&self) -> bool {
        self.sim.is_paused()
    }

    /// `state.rules.pauseDisabled` of the live session (`false` without
    /// rules); the UI's pause paths gate on it (`Control.java:620,629`).
    #[func]
    pub fn rules_pause_disabled(&self) -> bool {
        self.live_rules().is_some_and(|rules| rules.pause_disabled)
    }

    // ---- Plan 09 networks debug API (data side; §7c MCP run deferred) ----

    /// Whole-network projection as pretty JSON (plan 09 §3.11).
    #[func]
    pub fn network_state(&self) -> GString {
        let tick = self.sim.tick_count();
        let state = mind_core::world::NetworkState::capture(self.world(), tick);
        GString::from(state.to_json().as_str())
    }

    /// Live network counts `{graphs, liquids, heat_buildings}` (plan 09 §3.12).
    #[func]
    pub fn network_counts(&self) -> Dictionary<GString, i64> {
        let world = self.world();
        let graphs = world
            .get_resource::<PowerGrids>()
            .map(|grids| grids.graph_count())
            .unwrap_or(0);
        let mut liquids = 0i64;
        let mut heat = 0i64;
        for entity in world.iter_entities() {
            if entity.get::<LiquidModule>().is_some() {
                liquids += 1;
            }
            if entity.get::<HeatState>().is_some() {
                heat += 1;
            }
        }
        let mut out = Dictionary::<GString, i64>::new();
        out.set("graphs", graphs as i64);
        out.set("liquids", liquids);
        out.set("heat_buildings", heat);
        out
    }

    /// `HintsFragment` poll (`HintsFragment.java:74-102`): event sets + the
    /// per-frame predicate inputs for the hint driver (T4a/T4b).
    #[func]
    pub fn hint_state_json(&self) -> GString {
        GString::from(
            hint_state_value(
                &self.sim,
                self.content_snapshot.as_ref(),
                self.player_team(),
            )
            .to_string()
            .as_str(),
        )
    }

    /// Inserts a hint event by name (G05 T4b). `derelictmouse` is host-detected
    /// (`HintsFragment.java:100-102`, `Trigger.derelictMouse`); unknown names
    /// are ignored.
    #[func]
    pub fn hint_event(&mut self, name: GString) {
        if name == "derelictmouse" {
            self.sim.hint_events.events.insert("derelictmouse");
        }
    }
}

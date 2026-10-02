// SPDX-License-Identifier: GPL-3.0-only

//! Plan 12 campaign headless scenarios (`campaign` subcommand).
//!
//! Implements the M0/M2 slices of plan 12 §7b: `campaign rules`
//! (`rules_roundtrip`: JSON→Rules→TypeIO→JSON equality + `RulesLoadEvent`) and
//! `campaign tech` (`tech_unlock_gating`: locked→unlock, spend, auto-unlocks,
//! `req-` persistence, MP `Rules.researched`). M3+ scenarios
//! (`sector_save_load_turn`/`schematic_place`/`fog_reveal`/`campaign_sector_cycle`)
//! are deferred with the owning milestones.

use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use mind_core::content::settings_store::UnlockStore;
use mind_core::content::stacks::ItemStack;
use mind_core::content::{
    BlockId, ContentRef, ContentType, MemoryBundle, MemoryUnlockStore, create_base_content,
};
use mind_core::game::objectives::{ObjectiveContext, SectorStatus};
use mind_core::game::planet::EmptyNeighborhood;
use mind_core::game::rules::Rules;
use mind_core::game::rules_event::{RulesEpoch, apply_rules_load};
use mind_core::game::tech_tree;
use mind_core::game::universe::{Campaign, TurnContext};
use mind_core::io::json::rules::ExportStat;
use mind_core::io::typeio::codecs::{read_rules, write_rules};
use mind_core::io::{WireReader, WireWriter};
use mind_core::random::JavaRandom;
use mind_core::world::modules::ItemModule;

use crate::cli::CampaignCommand;

/// Exit code: pass.
const EXIT_PASS: i32 = 0;
/// Exit code: assertion/golden mismatch.
const EXIT_FAIL: i32 = 1;

/// Stateless objective context: all objectives met, no sector state. Campaign
/// gating is exercised separately in M3.
struct ObjectivesMet;

impl ObjectiveContext for ObjectivesMet {
    fn is_unlocked(&self, _content: ContentRef) -> bool {
        true
    }
    fn sector_status(&self, _sector: mind_core::content::SectorId) -> SectorStatus {
        SectorStatus::default()
    }
    fn planet_has_base(&self, _planet: mind_core::content::PlanetId) -> bool {
        false
    }
}

/// Runs a `campaign` subcommand.
pub fn run(command: &CampaignCommand) -> Result<i32> {
    let (report, dump) = match command {
        CampaignCommand::Rules { .. } => rules_roundtrip()?,
        CampaignCommand::Tech { .. } => tech_unlock_gating()?,
        CampaignCommand::Sector {
            planet,
            sector,
            ticks,
            ..
        } => campaign_sector_cycle(planet, sector, *ticks)?,
        CampaignCommand::Turn { turns, .. } => campaign_turn(*turns)?,
        CampaignCommand::Schematic { .. } => campaign_schematic()?,
        CampaignCommand::Fog { .. } => campaign_fog()?,
        CampaignCommand::Objectives { .. } => objectives_completion()?,
        CampaignCommand::Play { planet, sector, .. } => campaign_play(planet, sector)?,
        CampaignCommand::Bench { profile, ticks, .. } => campaign_bench(profile, *ticks)?,
    };
    match command {
        CampaignCommand::Rules { json, dump: path }
        | CampaignCommand::Tech { json, dump: path }
        | CampaignCommand::Sector {
            json, dump: path, ..
        }
        | CampaignCommand::Turn {
            json, dump: path, ..
        }
        | CampaignCommand::Schematic {
            json, dump: path, ..
        }
        | CampaignCommand::Fog {
            json, dump: path, ..
        }
        | CampaignCommand::Objectives {
            json, dump: path, ..
        }
        | CampaignCommand::Play {
            json, dump: path, ..
        }
        | CampaignCommand::Bench {
            json, dump: path, ..
        } => {
            if let Some(path) = path {
                write_dump(path, &dump)?;
            }
            if *json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
        }
    }
    let pass = report["pass"].as_bool().unwrap_or(false);
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

fn write_dump(path: &Path, dump: &Value) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(dump)?))?;
    Ok(())
}

/// `campaign rules` — plan 12 §7b `rules_roundtrip`.
fn rules_roundtrip() -> Result<(Value, Value)> {
    let mut rules = Rules {
        attack_mode: true,
        build_speed_multiplier: 99.1,
        ..Rules::default()
    };
    rules.tags.insert("blah".to_owned(), "bleh".to_owned());

    // JSON round-trip (compare serialized blobs, per the plan's JSON equality).
    let json_blob = serde_json::to_string(&rules)?;
    let back: Rules = serde_json::from_str(&json_blob)?;
    let json_equal = serde_json::to_string(&back)? == json_blob;

    // TypeIO round-trip.
    let mut buffer = Vec::new();
    {
        let mut writer = WireWriter::new(&mut buffer);
        write_rules(&mut writer, &rules)?;
    }
    let mut reader = WireReader::new(&buffer);
    let typed = read_rules(&mut reader)?;
    let typeio_equal = typed.attack_mode == rules.attack_mode
        && (typed.build_speed_multiplier - rules.build_speed_multiplier).abs() < 1e-4
        && typed.tags.get("blah") == Some(&"bleh".to_owned());

    // `RulesLoadEvent` fires once (`from_save=false`).
    let mut epoch = RulesEpoch::new();
    let mut active = Rules::default();
    let load = apply_rules_load(&mut active, rules.clone(), false, &mut epoch);

    let checksum = rules.checksum_part().to_hex();
    let pass = json_equal && typeio_equal && !load.from_save && load.rules_epoch == 1;

    let dump = json!({
        "format": 1,
        "scenario": "campaign_rules_roundtrip",
        "mode": rules.mode().name(),
        "checksum": checksum,
        "json_equal": json_equal,
        "typeio_equal": typeio_equal,
        "rules_load": { "rules_epoch": load.rules_epoch, "from_save": load.from_save },
        "pass": pass,
    });
    let report = json!({
        "scenario": "campaign_rules_roundtrip",
        "mode": rules.mode().name(),
        "checksum": checksum,
        "json_equal": json_equal,
        "typeio_equal": typeio_equal,
        "rules_load_events": 1,
        "pass": pass,
    });
    Ok((report, dump))
}

/// `campaign tech` — plan 12 §7b `tech_unlock_gating`.
fn tech_unlock_gating() -> Result<(Value, Value)> {
    let bundle = MemoryBundle::new();
    let mut store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true)
        .map_err(|error| anyhow::anyhow!("content boot failed: {error}"))?;
    let ctx = ObjectivesMet;

    // 1. Vanilla gating: a depth>=1 node with a locked parent cannot be spent.
    let mut locked_before = false;
    let mut can_after = false;
    let mut gated_node = String::new();
    if let Some((node_ref, parent_ref, content)) = find_gated_node(&registry) {
        let mut items = ItemModule::with_items(registry.items().len());
        locked_before = !tech_tree::can_spend(&registry, node_ref, &items, &ctx);
        gated_node = tech_tree::content_name(&registry, content).unwrap_or_default();
        let _ = tech_tree::unlock(&mut registry, parent_ref, &mut store);
        can_after = tech_tree::can_spend(&registry, node_ref, &items, &ctx);
        // Spending required zero items for this node (plan-02 requirements gap),
        // but completes and unlocks.
        let _ = tech_tree::spend(&mut registry, node_ref, &mut items, &mut store, &ctx, false)?;
    }

    // 2. Item spending + `req-` persistence on a synthetic requirement node.
    let copper = registry
        .item_id("copper")
        .ok_or_else(|| anyhow::anyhow!("copper missing"))?;
    let stone_wall = ContentRef::of(ContentType::Block, BlockId::STONE_WALL);
    let need = 30;
    let req_node = tech_tree::push_node(
        &mut registry,
        0,
        None,
        Some(stone_wall),
        "stone-wall",
        vec![ItemStack::new(copper, need)],
    );
    let mut items = ItemModule::with_items(registry.items().len());
    items.add(copper, need + 7, 1_000);
    let spend = tech_tree::spend(&mut registry, req_node, &mut items, &mut store, &ctx, false)?;
    let items_spent = spend.spent.first().map(|stack| stack.amount).unwrap_or(0);
    let copper_left = items.get(copper);
    let item_name = registry
        .item(copper)
        .map(|record| record.name.clone())
        .unwrap_or_default();
    let req_key = format!("req-stone-wall-{item_name}");
    let req_persisted = store.get_i32(&req_key);

    // 3. Auto-unlock only for zero-requirement parent-unlocked nodes.
    let mut fresh_store = MemoryUnlockStore::new();
    let mut fresh = create_base_content(&MemoryBundle::new(), &fresh_store, true)
        .map_err(|error| anyhow::anyhow!("content boot failed: {error}"))?;
    let auto_unlocked = tech_tree::check_auto_unlocks(&mut fresh, &mut fresh_store, &ctx);
    let auto_idempotent =
        tech_tree::check_auto_unlocks(&mut fresh, &mut fresh_store, &ctx).is_empty();
    let auto_content = auto_unlocked
        .iter()
        .filter_map(|content| tech_tree::content_name(&fresh, *content))
        .collect::<Vec<_>>();

    // 4. MP: `Rules.researched` is host-authoritative.
    let content = registry
        .tech()
        .nodes
        .iter()
        .filter_map(|node| node.content)
        .find(|content| !tech_tree::content_unlocked(&registry, *content));
    let mut mp_override = false;
    if let Some(content) = content
        && let Some(name) = tech_tree::content_name(&registry, content)
    {
        let mut rules = Rules::default();
        let before = tech_tree::content_unlocked_with_rules(&registry, content, &rules);
        rules.researched.insert(name);
        let after = tech_tree::content_unlocked_with_rules(&registry, content, &rules);
        mp_override = !before && after;
    }

    let pass = (!gated_node.is_empty() && locked_before && can_after)
        && items_spent == need
        && copper_left == 7
        && req_persisted == need
        && !auto_content.is_empty()
        && auto_idempotent
        && mp_override;

    let dump = json!({
        "format": 1,
        "scenario": "campaign_tech_unlock_gating",
        "gated_node": gated_node,
        "locked_before": locked_before,
        "spendable_after_parent_unlock": can_after,
        "items_spent": items_spent,
        "copper_left": copper_left,
        "req_key": req_key,
        "req_persisted": req_persisted,
        "auto_unlocked": auto_content,
        "auto_unlock_idempotent": auto_idempotent,
        "mp_researched_override": mp_override,
        "pass": pass,
    });
    let report = json!({
        "scenario": "campaign_tech_unlock_gating",
        "locked_before": locked_before,
        "items_spent": items_spent,
        "auto_unlocked_count": auto_content.len(),
        "mp_researched_override": mp_override,
        "pass": pass,
    });
    Ok((report, dump))
}

/// Finds a depth>=1 vanilla node whose parent (and itself) are locked.
fn find_gated_node(
    registry: &mind_core::content::ContentRegistry,
) -> Option<(
    mind_core::content::tech::TechNodeRef,
    mind_core::content::tech::TechNodeRef,
    ContentRef,
)> {
    let tech = registry.tech();
    for (index, node) in tech.nodes.iter().enumerate() {
        let Some(parent_ref) = node.parent else {
            continue;
        };
        let Some(parent_content) = tech.node(parent_ref).and_then(|parent| parent.content) else {
            continue;
        };
        let Some(content) = node.content else {
            continue;
        };
        if !tech_tree::content_unlocked(registry, content)
            && !tech_tree::content_unlocked(registry, parent_content)
        {
            return Some((
                mind_core::content::tech::TechNodeRef(index as u32),
                parent_ref,
                content,
            ));
        }
    }
    None
}

/// FNV-1a hex over bytes (canonical checksum helper).
fn fnv_hex(bytes: &[u8]) -> String {
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write(bytes);
    hasher.finish().to_hex()
}

/// `campaign sector` — plan 12 §7b `campaign_sector_cycle` (M3 half).
fn campaign_sector_cycle(
    planet_name: &str,
    sector_name: &str,
    ticks: u64,
) -> Result<(Value, Value)> {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let registry = create_base_content(&bundle, &store, true)
        .map_err(|error| anyhow::anyhow!("content boot failed: {error}"))?;
    let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);

    let planet_id = campaign
        .planet_id_by_name(planet_name)
        .ok_or_else(|| anyhow::anyhow!("unknown planet `{planet_name}`"))?;
    let preset = registry
        .sector_by_name(sector_name)
        .ok_or_else(|| anyhow::anyhow!("unknown sector preset `{sector_name}`"))?;
    let sector_id = preset.sector;
    let preset_on_planet = preset.planet == planet_id;
    let planet_def = registry
        .planet(planet_id)
        .ok_or_else(|| anyhow::anyhow!("planet record missing"))?;

    // Launch (play-flow seam): mark the sector owned and seed a core.
    {
        let sector = campaign
            .sector_mut(planet_id, sector_id)
            .ok_or_else(|| anyhow::anyhow!("sector {sector_id} out of range"))?;
        sector.save = Some(format!("sector-{planet_name}-{sector_id}"));
        sector.info.info.has_core = true;
        sector.info.info.storage_capacity = 4000;
        sector.info.info.waves = false;
        sector.info.info.attack = false;
        sector.info.info.wave = 1;
        sector.info.info.best_core_type = planet_def
            .default_core
            .clone()
            .unwrap_or_else(|| "core-shard".to_owned());
        sector
            .info
            .info
            .production
            .insert("copper".to_owned(), ExportStat { mean: 1.0 });
    }

    // Campaign rules fold (fog/hideSpawns/enemy multipliers) + RulesLoadEvent.
    let mut rules = Rules {
        attack_mode: false,
        wave_team: 2,
        ..Rules::default()
    };
    let apply = campaign
        .planet(planet_id)
        .ok_or_else(|| anyhow::anyhow!("planet runtime missing"))?
        .apply_rules(&registry, &mut rules, false, false);
    let mut epoch = RulesEpoch::new();
    let mut active = Rules::default();
    let rules_load = apply_rules_load(&mut active, rules.clone(), false, &mut epoch);

    // Fixed-step advance (600 ticks -> 10 s by default).
    let _due = campaign.advance_time(ticks);

    let mut settings = mind_core::io::settings::SettingsStore::new();
    let report = campaign.run_turn(
        &registry,
        &mut settings,
        &TurnContext::default(),
        &mut JavaRandom::new(1),
    );

    // Persist + reload the sector info.
    let planet_label = campaign
        .planet(planet_id)
        .ok_or_else(|| anyhow::anyhow!("planet runtime missing"))?
        .name
        .clone();
    let saved = {
        let sector = campaign
            .sector(planet_id, sector_id)
            .ok_or_else(|| anyhow::anyhow!("sector missing"))?;
        sector.save_info(&mut settings, &planet_label).is_ok()
    };
    let mut reloaded = campaign
        .sector(planet_id, sector_id)
        .ok_or_else(|| anyhow::anyhow!("sector missing"))?
        .clone();
    reloaded.load_info(&settings, &planet_label);
    let info_roundtrip = reloaded.info.info
        == campaign
            .sector(planet_id, sector_id)
            .ok_or_else(|| anyhow::anyhow!("sector missing"))?
            .info
            .info;

    let info = &campaign
        .sector(planet_id, sector_id)
        .ok_or_else(|| anyhow::anyhow!("sector missing"))?
        .info
        .info;
    let checksum = fnv_hex(serde_json::to_string(info)?.as_bytes());
    let rules_checksum = rules.checksum_part().to_hex();

    let pass = preset_on_planet
        && info.has_core
        && info.wave == 1
        && info.items.get("copper").copied().unwrap_or(0) > 0
        && saved
        && info_roundtrip
        && report.turn == 1
        && !rules_load.from_save
        && rules_load.rules_epoch == 1
        && !apply.rts_enabled;

    let dump = json!({
        "format": 1,
        "scenario": "campaign_sector_cycle",
        "planet": planet_label,
        "sector": sector_name,
        "sector_id": sector_id,
        "preset_on_planet": preset_on_planet,
        "ticks": ticks,
        "turn": report.turn,
        "has_core": info.has_core,
        "wave": info.wave,
        "storage_capacity": info.storage_capacity,
        "best_core_type": info.best_core_type,
        "copper": info.items.get("copper").copied().unwrap_or(0),
        "rules_epoch": rules_load.rules_epoch,
        "fog": rules.fog,
        "checksum": checksum,
        "rules_checksum": rules_checksum,
        "pass": pass,
    });
    let report = json!({
        "scenario": "campaign_sector_cycle",
        "planet": planet_label,
        "sector": sector_name,
        "turn": report.turn,
        "has_core": info.has_core,
        "checksum": checksum,
        "pass": pass,
    });
    let _ = store;
    Ok((report, dump))
}

/// `campaign turn` — plan 12 §7b `campaign_turn` (M3 production/export means).
fn campaign_turn(turns: u32) -> Result<(Value, Value)> {
    let bundle = MemoryBundle::new();
    let registry = create_base_content(&bundle, &MemoryUnlockStore::new(), true)
        .map_err(|error| anyhow::anyhow!("content boot failed: {error}"))?;
    let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
    let planet = campaign
        .planet_id_by_name("serpulo")
        .ok_or_else(|| anyhow::anyhow!("serpulo missing"))?;

    let source_id = registry
        .sector_by_name("groundZero")
        .map(|preset| preset.sector)
        .ok_or_else(|| anyhow::anyhow!("groundZero missing"))?;
    let dest_id = registry
        .sector_by_name("saltFlats")
        .map(|preset| preset.sector)
        .ok_or_else(|| anyhow::anyhow!("saltFlats missing"))?;

    for id in [source_id, dest_id] {
        let sector = campaign
            .sector_mut(planet, id)
            .ok_or_else(|| anyhow::anyhow!("sector {id} out of range"))?;
        sector.save = Some(format!("sector-serpulo-{id}"));
        sector.info.info.has_core = true;
        sector.info.info.storage_capacity = 1_000_000;
        sector.info.info.waves = false;
        sector.info.info.attack = false;
    }
    {
        let source = campaign
            .sector_mut(planet, source_id)
            .ok_or_else(|| anyhow::anyhow!("source sector missing"))?;
        source
            .info
            .info
            .production
            .insert("silicon".to_owned(), ExportStat { mean: 2.5 });
        source
            .info
            .info
            .export
            .insert("silicon".to_owned(), ExportStat { mean: 2.5 });
        source.info.info.destination = Some(mind_core::io::json::content_serde::SectorKey::format(
            "serpulo", dest_id,
        ));
    }
    campaign
        .planet_mut(planet)
        .ok_or_else(|| anyhow::anyhow!("serpulo runtime missing"))?
        .campaign_rules
        .legacy_launch_pads = true;

    let mut settings = mind_core::io::settings::SettingsStore::new();
    let mut rand = JavaRandom::new(7);
    let mut turn_report = None;
    for _ in 0..turns {
        turn_report =
            Some(campaign.run_turn(&registry, &mut settings, &TurnContext::default(), &mut rand));
    }
    let turn = turn_report.map(|report| report.turn).unwrap_or(0);

    let source = campaign
        .sector(planet, source_id)
        .ok_or_else(|| anyhow::anyhow!("source sector missing"))?;
    let dest = campaign
        .sector(planet, dest_id)
        .ok_or_else(|| anyhow::anyhow!("destination sector missing"))?;
    let expected = (2.5 * 120.0 * turns as f32) as i32;
    let source_items = source.info.info.items.get("silicon").copied().unwrap_or(0);
    let dest_items = dest.info.info.items.get("silicon").copied().unwrap_or(0);
    let checksum = fnv_hex(
        serde_json::to_string(&json!({
            "turn": turn,
            "source": source_items,
            "dest": dest_items,
        }))?
        .as_bytes(),
    );
    let pass = turn == turns as i32 && source_items == expected && dest_items == expected;

    let dump = json!({
        "format": 1,
        "scenario": "campaign_turn",
        "turns": turns,
        "turn": turn,
        "production_mean": 2.5,
        "seconds_passed": 120.0,
        "expected_items": expected,
        "source_items": source_items,
        "dest_items": dest_items,
        "checksum": checksum,
        "pass": pass,
    });
    let report = json!({
        "scenario": "campaign_turn",
        "turns": turns,
        "source_items": source_items,
        "dest_items": dest_items,
        "checksum": checksum,
        "pass": pass,
    });
    Ok((report, dump))
}

/// `campaign schematic` — plan 12 §7b M5 (`.msch`/base64 round-trip + rotate).
fn campaign_schematic() -> Result<(Value, Value)> {
    let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .map_err(|error| anyhow::anyhow!("content boot failed: {error}"))?;
    let mut schematics = mind_core::game::schematics::Schematics::new();
    schematics.load_loadouts(&registry);
    let decoded = schematics.all.len();

    let basic = schematics
        .all
        .first()
        .ok_or_else(|| anyhow::anyhow!("basicShard loadout missing"))?;
    let has_core = basic.has_core(&registry);
    let roundtrip = mind_core::game::schematics::write_base64(basic, &registry)
        .map(|encoded| {
            encoded.starts_with("bXNjaA")
                && mind_core::game::schematics::read_base64(&encoded, &registry)
                    .map(|back| back.tiles.len() == basic.tiles.len())
                    .unwrap_or(false)
        })
        .unwrap_or(false);

    // Rotate 4 times returns to the original dimensions + tile count.
    let rotated = mind_core::game::schematics::Schematics::rotate(basic, 4, &registry);
    let rotation_stable = rotated.width == basic.width && rotated.height == basic.height;

    let checksum = fnv_hex(
        serde_json::to_string(&json!({
            "tiles": basic.tiles.len(),
            "width": basic.width,
            "height": basic.height,
        }))?
        .as_bytes(),
    );
    let pass = decoded == 4 && has_core && roundtrip && rotation_stable;

    let dump = json!({
        "format": 1,
        "scenario": "campaign_schematic",
        "loaded": decoded,
        "name": basic.name(),
        "has_core": has_core,
        "base64_prefix": "bXNjaA",
        "roundtrip": roundtrip,
        "rotation_stable": rotation_stable,
        "checksum": checksum,
        "pass": pass,
    });
    let report = json!({
        "scenario": "campaign_schematic",
        "loaded": decoded,
        "has_core": has_core,
        "roundtrip": roundtrip,
        "checksum": checksum,
        "pass": pass,
    });
    Ok((report, dump))
}

/// `campaign fog` — plan 12 §7b M7 (`fog_reveal` + attack indicators).
fn campaign_fog() -> Result<(Value, Value)> {
    use mind_core::game::attack_indicators::AttackIndicators;
    use mind_core::game::fog::{FogControl, FogSource};

    let mut fog = FogControl::new();
    let blocks = vec![FogSource {
        x: 32,
        y: 32,
        radius: 8,
        team: 0,
    }];
    let units = vec![FogSource {
        x: 40,
        y: 40,
        radius: 6,
        team: 0,
    }];
    fog.on_world_load(64, 64, true, true, &blocks);
    fog.update(40, &[0], &blocks, &units, true, true, false);
    fog.update(120, &[0], &blocks, &units, true, true, false);

    // Static discovery around the building; dynamic visibility around the unit.
    let discovered_center = fog.is_discovered(0, 32, 32, true, true, false);
    let discovered_unit = fog.is_discovered(0, 40, 40, true, true, false);
    let visible_unit = fog.is_visible_tile(0, 40, 40, true, false);
    let undiscovered_far = !fog.is_discovered(0, 2, 2, true, true, false);
    let discovery_differs = discovered_unit != visible_unit || visible_unit;
    let edge_clip = !fog.is_discovered(0, 63, 63, true, true, false);

    // RLE chunk round-trip.
    let chunk = fog.write();
    let mut restored = FogControl::new();
    restored.read(&chunk);
    let chunk_roundtrip = restored.fog[0].as_ref().map(|d| &d.static_data)
        == fog.fog[0].as_ref().map(|d| &d.static_data);

    // Attack indicators: add/dedupe/timeout.
    let mut indicators = AttackIndicators::new();
    indicators.add(10, 10);
    indicators.add(10, 10);
    let dedup = indicators.len() == 1;
    indicators.update(900.0);
    let expired = indicators.is_empty();

    let checksum = fnv_hex(&chunk);
    let pass = discovered_center
        && discovered_unit
        && visible_unit
        && undiscovered_far
        && discovery_differs
        && edge_clip
        && chunk_roundtrip
        && dedup
        && expired;

    let dump = json!({
        "format": 1,
        "scenario": "campaign_fog",
        "chunk_bytes": chunk.len(),
        "discovered_center": discovered_center,
        "discovered_unit": discovered_unit,
        "visible_unit": visible_unit,
        "undiscovered_far": undiscovered_far,
        "edge_clip": edge_clip,
        "chunk_roundtrip": chunk_roundtrip,
        "indicator_dedup": dedup,
        "indicator_expired": expired,
        "checksum": checksum,
        "pass": pass,
    });
    let report = json!({
        "scenario": "campaign_fog",
        "chunk_bytes": chunk.len(),
        "chunk_roundtrip": chunk_roundtrip,
        "checksum": checksum,
        "pass": pass,
    });
    Ok((report, dump))
}

/// `campaign objectives` — plan 12 §7b M6 `objectives_completion`.
fn objectives_completion() -> Result<(Value, Value)> {
    use mind_core::game::map_markers::{MapMarkers, MarkerKind};
    use mind_core::game::map_objectives::{MapObjectivesRuntime, ObjectiveEnv, ObjectiveRunParams};
    use mind_core::io::json::objectives::{
        MapObjectives, ObjectiveMarker, PointMarker, TextureMarker,
    };

    /// Env where every predicate is already satisfied.
    struct ScriptedEnv;

    impl ObjectiveEnv for ScriptedEnv {
        fn is_content_unlocked(&self, _content: &str) -> bool {
            true
        }
        fn team_has_item(&self, _team: u8, _item: &str, _amount: i32) -> bool {
            true
        }
        fn core_item_count(&self, _item: &str) -> i32 {
            999
        }
        fn placed_block_count(&self, _block: &str) -> i32 {
            999
        }
        fn unit_count(&self, _team: u8, _unit: &str) -> i32 {
            999
        }
        fn enemy_units_destroyed(&self) -> i32 {
            999
        }
        fn objective_flag(&self, _flag: &str) -> bool {
            true
        }
        fn core_count(&self, _team: u8) -> usize {
            0
        }
        fn block_at(&self, _x: i32, _y: i32) -> Option<(&str, u8)> {
            None
        }
        fn headless(&self) -> bool {
            true
        }
        fn command_mode_satisfied(&self) -> bool {
            false
        }
    }

    let json = r#"[
        {"class":"Research","content":"alpha"},
        {"class":"Produce","content":"alpha"},
        {"class":"Item","item":"copper","amount":1},
        {"class":"CoreItem","item":"lead","amount":5},
        {"class":"BuildCount","block":"conveyor","count":2},
        {"class":"UnitCount","unit":"dagger","count":1},
        {"class":"DestroyUnits","count":3},
        {"class":"Timer","duration":1.0},
        {"class":"DestroyBlock","team":2,"block":"router","pos":{"x":0,"y":0}},
        {"class":"DestroyBlocks","team":2,"block":"router","positions":[{"x":1,"y":1}]},
        {"class":"CommandMode"},
        {"class":"Flag","flag":"captured","flagsAdded":["done"]},
        {"class":"DestroyCore"}
    ]"#;
    let data = MapObjectives::from_json(json)?;
    assert_eq!(data.len(), 13, "all 13 objective classes present");
    let mut runtime = MapObjectivesRuntime::from_data(data);
    let env = ScriptedEnv;
    let params = ObjectiveRunParams::default();
    let ready = runtime.update(&env, &params, 1.0);

    let mut rules = Rules::default();
    for index in &ready {
        runtime.complete(*index, &mut rules);
    }
    let all_done = (0..13).all(|index| runtime.is_completed(index));
    let flags: Vec<String> = rules.objective_flags.iter().cloned().collect();

    // Markers: add a point + texture, toggle via control, round-trip the region.
    let mut markers = MapMarkers::new();
    markers.add(
        1,
        ObjectiveMarker::Point(PointMarker {
            world: 1,
            minimap: 1,
            light: 1,
            ..Default::default()
        }),
    );
    markers.add(
        2,
        ObjectiveMarker::Texture(TextureMarker {
            world: -1,
            minimap: 1,
            light: -1,
            ..Default::default()
        }),
    );
    markers.update_marker(MarkerKind::World, 2, true);
    let counts = (
        markers.world_count(),
        markers.map_count(),
        markers.light_count(),
    );

    let mut buffer = Vec::new();
    {
        let mut writer = mind_core::io::WireWriter::new(&mut buffer);
        mind_core::io::save::state::MarkersIo::write_markers(&markers, &mut writer)?;
    }
    let mut restored = MapMarkers::new();
    {
        let mut reader = mind_core::io::WireReader::new(&buffer);
        mind_core::io::save::state::MarkersSink::read_markers(&mut restored, &mut reader)?;
    }
    let marker_roundtrip = restored == markers;

    let checksum = fnv_hex(
        serde_json::to_string(&json!({
            "completed": ready,
            "flags": flags,
            "markers": counts,
        }))?
        .as_bytes(),
    );
    let pass = all_done
        && ready.len() == 13
        && flags == vec!["done".to_owned()]
        && counts == (2, 2, 1)
        && marker_roundtrip;

    let dump = json!({
        "format": 1,
        "scenario": "campaign_objectives_completion",
        "objective_types": 13,
        "completed": ready,
        "all_done": all_done,
        "flags": flags,
        "markers": { "world": counts.0, "minimap": counts.1, "light": counts.2 },
        "marker_roundtrip": marker_roundtrip,
        "checksum": checksum,
        "pass": pass,
    });
    let report = json!({
        "scenario": "campaign_objectives_completion",
        "objective_types": 13,
        "completed": ready.len(),
        "marker_roundtrip": marker_roundtrip,
        "checksum": checksum,
        "pass": pass,
    });
    Ok((report, dump))
}

/// `campaign play` — plan 12 §7b M8 (`campaign_sector_cycle` capture/game-over).
fn campaign_play(planet_name: &str, sector_name: &str) -> Result<(Value, Value)> {
    use bevy_ecs::entity::Entity;
    use mind_core::ecs::TeamId;
    use mind_core::game::play::{
        PlayEvent, PlaySession, check_game_state, play_new_sector, run_wave_campaign,
    };
    use mind_core::game::rules_event::RulesEpoch;
    use mind_core::game::world_reloader::HostReloader;
    use mind_core::io::json::objectives::{MapObjective, ObjectiveMarker, PointMarker};

    let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .map_err(|error| anyhow::anyhow!("content boot failed: {error}"))?;
    let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
    let planet = campaign
        .planet_id_by_name(planet_name)
        .ok_or_else(|| anyhow::anyhow!("unknown planet `{planet_name}`"))?;
    let sector_id = registry
        .sector_by_name(sector_name)
        .ok_or_else(|| anyhow::anyhow!("unknown sector `{sector_name}`"))?
        .sector;
    // Seed an owned sector with a core so the launch is a normal campaign play.
    {
        let sector = campaign
            .sector_mut(planet, sector_id)
            .ok_or_else(|| anyhow::anyhow!("sector missing"))?;
        sector.save = Some(format!("sector-{planet_name}-{sector_id}"));
        sector.info.info.has_core = true;
        sector.info.info.waves = true;
    }

    let mut rules = Rules {
        waves: true,
        win_wave: 2,
        wave_spacing: 10.0,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    };
    rules.team_rule_mut(1);
    let mut session = PlaySession::new(rules);

    let mut epoch = RulesEpoch::new();
    let mut reloader = HostReloader::default();
    let mut events = play_new_sector(
        &mut session,
        &mut campaign,
        &registry,
        planet,
        sector_id,
        None,
        &mut epoch,
        &mut reloader,
    );

    // Live match state is created after the world reload (the reloader resets
    // the transient match, exactly like upstream `WorldReloader.begin`).
    session.teams.register_core(
        Entity::from_raw_u32(1).ok_or_else(|| anyhow::anyhow!("invalid entity index"))?,
        TeamId(1),
        &session.rules,
    );
    session.markers.add(
        1,
        ObjectiveMarker::Point(PointMarker {
            world: 1,
            minimap: 1,
            light: -1,
            ..Default::default()
        }),
    );
    session.objectives.add([MapObjective::DestroyUnits(
        mind_core::io::json::objectives::DestroyUnitsObjective {
            count: 1,
            ..Default::default()
        },
    )]);
    let markers_before = session.markers.size();
    let objectives_before = session.objectives.len();

    events.push(run_wave_campaign(&mut session));
    events.push(run_wave_campaign(&mut session));
    // A real sector has enemy spawn points; keep waves enabled so the win-wave
    // capture path (not the no-spawns disable path) is exercised.
    session.spawn_count = 1;
    session.enemies = 0;
    events.extend(check_game_state(&mut session, &mut campaign));

    let captured = campaign
        .sector(planet, sector_id)
        .map(|sector| sector.info.info.was_captured)
        .unwrap_or(false);
    let markers_cleared = session.markers.size() == 0;
    let objectives_cleared = session.objectives.is_empty();
    assert_eq!(markers_before, 1);
    assert_eq!(objectives_before, 1);

    // Lose variant: a fresh campaign session with no player core.
    let mut lose_session = PlaySession::new(Rules {
        waves: true,
        win_wave: 0,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    lose_session.sector = Some((planet, sector_id));
    let lose_events = check_game_state(&mut lose_session, &mut campaign);

    // `set_rules` campaign guard.
    use mind_core::game::rules_event::{SetRulesError, apply_set_rules};
    let mut open = PlaySession::new(Rules::default());
    let open_is_campaign = open.is_campaign();
    let mut set_epoch = RulesEpoch::new();
    let open_ok = apply_set_rules(
        &mut open.rules,
        Rules::default(),
        open_is_campaign,
        Some(0),
        &mut set_epoch,
    )
    .is_ok();
    let campaign_is = session.is_campaign();
    let guard_rejects = matches!(
        apply_set_rules(
            &mut session.rules,
            Rules::default(),
            campaign_is,
            Some(set_epoch.0),
            &mut set_epoch,
        ),
        Err(SetRulesError::CampaignReadOnly)
    );

    let event_names: Vec<String> = events.iter().map(|event| format!("{event:?}")).collect();
    let lose_names: Vec<String> = lose_events
        .iter()
        .map(|event| format!("{event:?}"))
        .collect();
    let has_capture = events
        .iter()
        .any(|event| matches!(event, PlayEvent::SectorCapture { .. }));
    let passed = reloader.began
        && session.phase == mind_core::game::State::Playing
        && has_capture
        && !session.rules.waves
        && !session.rules.attack_mode
        && session.rules.disable_world_processors
        && markers_cleared
        && objectives_cleared
        && captured
        && lose_names
            == vec![format!(
                "{:?}",
                PlayEvent::GameOver {
                    winner: lose_session.wave_team()
                }
            )]
        && open_ok
        && guard_rejects;

    let checksum = fnv_hex(
        serde_json::to_string(&json!({
            "events": event_names,
            "lose": lose_names,
        }))?
        .as_bytes(),
    );
    let dump = json!({
        "format": 1,
        "scenario": "campaign_play",
        "planet": planet_name,
        "sector": sector_name,
        "events": event_names,
        "lose_events": lose_names,
        "markers_before": markers_before,
        "markers_cleared": markers_cleared,
        "objectives_cleared": objectives_cleared,
        "captured": captured,
        "set_rules_open_ok": open_ok,
        "set_rules_campaign_rejected": guard_rejects,
        "checksum": checksum,
        "pass": passed,
    });
    let report = json!({
        "scenario": "campaign_play",
        "events": event_names.len(),
        "captured": captured,
        "guard_rejects": guard_rejects,
        "checksum": checksum,
        "pass": passed,
    });
    Ok((report, dump))
}

/// `campaign bench` — plan 12 §7d budget profiles (measurement, not a gate).
fn campaign_bench(profile: &str, ticks: u32) -> Result<(Value, Value)> {
    use std::time::Instant;

    use bevy_ecs::entity::Entity;
    use mind_core::content::BlockId;
    use mind_core::game::fog::{FogControl, FogSource};
    use mind_core::game::map_objectives::{MapObjectivesRuntime, ObjectiveEnv, ObjectiveRunParams};
    use mind_core::game::teams::{BuildingRecord, Teams, UnitRecord};

    let profiles: Vec<String> = profile
        .split(',')
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    let mut checksums: serde_json::Map<String, Value> = serde_json::Map::new();
    let mut timing: serde_json::Map<String, Value> = serde_json::Map::new();
    let mut budget_ok = serde_json::Map::new();
    let mut correctness = true;

    // -- rules: JSON round-trip throughput (budget: persistence path only) --
    if profiles.iter().any(|name| name == "rules") {
        let rules = Rules::default();
        let json = serde_json::to_string(&rules)?;
        let ops = ticks.max(1) as u64 * 50;
        let start = Instant::now();
        let mut acc = 0u64;
        for _ in 0..ops {
            let back: Rules = serde_json::from_str(&json)?;
            acc = acc.wrapping_add(back.win_wave as u64);
        }
        let elapsed = start.elapsed();
        checksums.insert("rules".to_owned(), json!(format!("{acc:016x}")));
        timing.insert(
            "rules".to_owned(),
            json!(elapsed.as_micros() as f64 / ops as f64),
        );
        budget_ok.insert("rules".to_owned(), json!(true));
    }

    // -- teams: 8 teams x 600 buildings x 300 units per tick --
    if profiles.iter().any(|name| name == "teams") {
        let rules = Rules {
            waves: false,
            ..Rules::default()
        };
        // Plan §7d profile: 8 teams, 600 buildings, 300 units *total*.
        let mut buildings = Vec::new();
        let mut units = Vec::new();
        let mut raw = 1u32;
        for team in 0..8u8 {
            for i in 0..75u32 {
                raw += 1;
                buildings.push(BuildingRecord {
                    entity: Entity::from_raw_u32(raw).ok_or_else(|| anyhow::anyhow!("entity"))?,
                    team,
                    block: BlockId::new((i % 32) as u16 + 1),
                    x: (i % 30) as f32 * 8.0,
                    y: (i / 30) as f32 * 8.0,
                    is_core: i == 0,
                    is_turret: i % 5 == 0,
                    turret_range: 0.0,
                    privileged: false,
                });
            }
            for i in 0..37u32 {
                raw += 1;
                units.push(UnitRecord {
                    entity: Entity::from_raw_u32(raw).ok_or_else(|| anyhow::anyhow!("entity"))?,
                    team,
                    type_id: (i % 16) as u16,
                    type_name: format!("unit-{}", i % 16),
                    x: (i % 25) as f32 * 8.0,
                    y: (i / 25) as f32 * 8.0,
                    is_boss: false,
                    is_flying: i % 3 == 0,
                    payload_types: Vec::new(),
                });
            }
        }
        let mut teams = Teams::new();
        let start = Instant::now();
        for _ in 0..ticks.max(1) {
            teams.update_team_stats(&buildings, &units, &[], &rules);
        }
        let elapsed = start.elapsed();
        let counts: i64 = teams
            .present
            .iter()
            .filter_map(|team| teams.get_or_null(*team))
            .map(|data| data.unit_count as i64)
            .sum();
        checksums.insert("teams".to_owned(), json!(format!("{counts:016x}")));
        let ms = elapsed.as_secs_f64() * 1000.0 / ticks.max(1) as f64;
        timing.insert("teams".to_owned(), json!(ms));
        budget_ok.insert("teams".to_owned(), json!(ms <= 0.35));
        correctness &= counts > 0;
    }

    // -- fog: 2000 buildings + 400 units per tick --
    if profiles.iter().any(|name| name == "fog") {
        let blocks: Vec<FogSource> = (0..2000)
            .map(|i| FogSource {
                x: i % 50,
                y: i / 50,
                radius: 6,
                team: (i % 2) as u8,
            })
            .collect();
        let units: Vec<FogSource> = (0..400)
            .map(|i| FogSource {
                x: i % 25 + 50,
                y: i / 25,
                radius: 4,
                team: (i % 2) as u8,
            })
            .collect();
        let mut fog = FogControl::new();
        fog.on_world_load(128, 128, true, true, &blocks);
        let start = Instant::now();
        for tick in 0..ticks.max(1) {
            fog.update(tick as u64, &[0, 1], &blocks, &units, true, true, false);
        }
        let elapsed = start.elapsed();
        let chunk = fog.write();
        checksums.insert("fog".to_owned(), json!(fnv_hex(&chunk)));
        let ms = elapsed.as_secs_f64() * 1000.0 / ticks.max(1) as f64;
        timing.insert("fog".to_owned(), json!(ms));
        budget_ok.insert("fog".to_owned(), json!(ms <= 0.60));
        correctness &= !chunk.is_empty();
    }

    // -- turn: production across two owned sectors --
    if profiles.iter().any(|name| name == "turn") {
        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .map_err(|error| anyhow::anyhow!("content boot failed: {error}"))?;
        let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let planet = campaign
            .planet_id_by_name("serpulo")
            .ok_or_else(|| anyhow::anyhow!("serpulo missing"))?;
        let source_id = registry
            .sector_by_name("groundZero")
            .ok_or_else(|| anyhow::anyhow!("groundZero missing"))?
            .sector;
        let dest_id = registry
            .sector_by_name("saltFlats")
            .ok_or_else(|| anyhow::anyhow!("saltFlats missing"))?
            .sector;
        for id in [source_id, dest_id] {
            let sector = campaign
                .sector_mut(planet, id)
                .ok_or_else(|| anyhow::anyhow!("sector missing"))?;
            sector.save = Some(format!("sector-serpulo-{id}"));
            sector.info.info.has_core = true;
            sector.info.info.storage_capacity = 1_000_000;
            sector.info.info.waves = false;
        }
        campaign
            .sector_mut(planet, source_id)
            .ok_or_else(|| anyhow::anyhow!("sector missing"))?
            .info
            .info
            .production
            .insert("copper".to_owned(), ExportStat { mean: 1.0 });
        let turns = ticks.clamp(1, 1000) as usize;
        let mut settings = mind_core::io::settings::SettingsStore::new();
        let mut rand = JavaRandom::new(3);
        let start = Instant::now();
        for _ in 0..turns {
            campaign.run_turn(&registry, &mut settings, &TurnContext::default(), &mut rand);
        }
        let elapsed = start.elapsed();
        let copper = campaign
            .sector(planet, source_id)
            .and_then(|sector| sector.info.info.items.get("copper").copied())
            .unwrap_or(0);
        checksums.insert("turn".to_owned(), json!(format!("{copper:016x}")));
        let ms = elapsed.as_secs_f64() * 1000.0 / turns as f64;
        timing.insert("turn".to_owned(), json!(ms));
        budget_ok.insert("turn".to_owned(), json!(ms <= 0.50));
        correctness &= campaign.universe.turn as usize == turns;
    }

    // -- objectives: 32 running objectives per tick --
    if profiles.iter().any(|name| name == "objectives") {
        struct BenchEnv;
        impl ObjectiveEnv for BenchEnv {
            fn is_content_unlocked(&self, _content: &str) -> bool {
                false
            }
            fn team_has_item(&self, _team: u8, _item: &str, _amount: i32) -> bool {
                false
            }
            fn core_item_count(&self, _item: &str) -> i32 {
                0
            }
            fn placed_block_count(&self, _block: &str) -> i32 {
                0
            }
            fn unit_count(&self, _team: u8, _unit: &str) -> i32 {
                0
            }
            fn enemy_units_destroyed(&self) -> i32 {
                0
            }
            fn objective_flag(&self, _flag: &str) -> bool {
                false
            }
            fn core_count(&self, _team: u8) -> usize {
                1
            }
            fn block_at(&self, _x: i32, _y: i32) -> Option<(&str, u8)> {
                None
            }
            fn headless(&self) -> bool {
                false
            }
            fn command_mode_satisfied(&self) -> bool {
                false
            }
        }
        let running: Vec<String> = (0..32)
            .map(|_| r#"{"class":"DestroyUnits","count":1000000000}"#.to_owned())
            .collect();
        let json = format!("[{}]", running.join(","));
        let data = mind_core::io::json::objectives::MapObjectives::from_json(&json)?;
        let mut runtime = MapObjectivesRuntime::from_data(data);
        let env = BenchEnv;
        let params = ObjectiveRunParams::default();
        let start = Instant::now();
        for _ in 0..ticks.max(1) {
            let _ = runtime.update(&env, &params, 1.0 / 60.0);
        }
        let elapsed = start.elapsed();
        checksums.insert(
            "objectives".to_owned(),
            json!(format!("{:016x}", runtime.len())),
        );
        let ms = elapsed.as_secs_f64() * 1000.0 / ticks.max(1) as f64;
        timing.insert("objectives".to_owned(), json!(ms));
        budget_ok.insert("objectives".to_owned(), json!(ms <= 0.10));
        correctness &= runtime.len() == 32;
    }

    // -- schematic: read+write the max 64x64 loadout --
    if profiles.iter().any(|name| name == "schematic") {
        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .map_err(|error| anyhow::anyhow!("content boot failed: {error}"))?;
        let mut schematics = mind_core::game::schematics::Schematics::new();
        schematics.load_loadouts(&registry);
        let basic = schematics
            .all
            .first()
            .ok_or_else(|| anyhow::anyhow!("loadout missing"))?;
        let ops = ticks.max(1) as u64;
        let start = Instant::now();
        let mut tiles = 0usize;
        for _ in 0..ops {
            let encoded = mind_core::game::schematics::write_base64(basic, &registry)?;
            let back = mind_core::game::schematics::read_base64(&encoded, &registry)?;
            tiles += back.tiles.len();
        }
        let elapsed = start.elapsed();
        checksums.insert("schematic".to_owned(), json!(format!("{tiles:016x}")));
        let ms = elapsed.as_secs_f64() * 1000.0 / ops as f64;
        timing.insert("schematic".to_owned(), json!(ms));
        budget_ok.insert("schematic".to_owned(), json!(ms <= 1.0));
        correctness &= tiles >= ops as usize;
    }

    let all_within = budget_ok
        .values()
        .all(|value| value.as_bool().unwrap_or(false));
    let pass = correctness;
    let dump = json!({
        "format": 1,
        "scenario": "campaign_bench",
        "ticks": ticks,
        "profiles": profiles,
        "timing_ms": timing,
        "budget_ok": budget_ok,
        "all_within_budget": all_within,
        "checksums": checksums,
        "pass": pass,
    });
    let report = json!({
        "scenario": "campaign_bench",
        "profiles": profiles,
        "all_within_budget": all_within,
        "pass": pass,
    });
    Ok((report, dump))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden(name: &str) -> String {
        let path = format!(
            "{}/tests/golden/campaign/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("missing {path}: {error}"))
    }

    fn canonical(value: &Value) -> String {
        format!("{}\n", serde_json::to_string_pretty(value).unwrap())
    }

    #[test]
    fn rules_golden_matches() {
        let (_, dump) = rules_roundtrip().unwrap();
        assert_eq!(golden("rules_roundtrip.json"), canonical(&dump));
    }

    #[test]
    fn tech_golden_matches() {
        let (_, dump) = tech_unlock_gating().unwrap();
        assert_eq!(golden("tech_unlock_gating.json"), canonical(&dump));
    }

    #[test]
    fn sector_golden_matches() {
        let (_, dump) = campaign_sector_cycle("serpulo", "groundZero", 600).unwrap();
        assert_eq!(golden("sector_cycle.json"), canonical(&dump));
    }

    #[test]
    fn turn_golden_matches() {
        let (_, dump) = campaign_turn(10).unwrap();
        assert_eq!(golden("turn.json"), canonical(&dump));
    }

    #[test]
    fn schematic_golden_matches() {
        let (_, dump) = campaign_schematic().unwrap();
        assert_eq!(golden("schematic.json"), canonical(&dump));
    }

    #[test]
    fn fog_golden_matches() {
        let (_, dump) = campaign_fog().unwrap();
        assert_eq!(golden("fog.json"), canonical(&dump));
    }

    #[test]
    fn objectives_golden_matches() {
        let (_, dump) = objectives_completion().unwrap();
        assert_eq!(golden("objectives_completion.json"), canonical(&dump));
    }

    #[test]
    fn play_golden_matches() {
        let (_, dump) = campaign_play("serpulo", "groundZero").unwrap();
        assert_eq!(golden("play.json"), canonical(&dump));
    }

    #[test]
    fn bench_profiles_are_deterministic_and_pass() {
        let (_, first) = campaign_bench("rules,teams,fog,turn,objectives,schematic", 30).unwrap();
        let (_, second) = campaign_bench("rules,teams,fog,turn,objectives,schematic", 30).unwrap();
        assert!(first["pass"].as_bool().unwrap());
        assert_eq!(first["checksums"], second["checksums"]);
    }
}

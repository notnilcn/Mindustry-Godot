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
use mind_core::game::rules::Rules;
use mind_core::game::rules_event::{RulesEpoch, apply_rules_load};
use mind_core::game::tech_tree;
use mind_core::io::typeio::codecs::{read_rules, write_rules};
use mind_core::io::{WireReader, WireWriter};
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
    };
    match command {
        CampaignCommand::Rules { json, dump: path }
        | CampaignCommand::Tech { json, dump: path } => {
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
}

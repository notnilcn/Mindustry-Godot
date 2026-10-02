// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: tests/src/test/java/{ApplicationTests,DataAssetTests}.java (mechanical
// audit surface), `core/ContentLoader.java` (`logContent`), and plan 02 §6.2/§7.

//! Source-derived parity golden and mechanical content audit (plan 02 M7).
//!
//! `mind-headless content dump` writes [`GoldenContent`] (see plan 02 §6.2);
//! `mind-headless content audit` diffs the live registry against the committed
//! golden plus optional bundle/`.properties` keys and the plan-03 asset
//! manifest. `cargo test` runs the same audit via [`tests`] over the committed
//! `parity/*.json` inputs. The JVM golden (`parity/java/DumpContent.java`) is
//! the NUD-10 upgrade path; this module's golden is the source-derived fallback
//! until a JDK is available (plan 02 R1).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::ctype::{Content, UnlockFields};
use super::load::ContentRegistry;
use super::names::mod_content_name_map_entries;
use super::{ContentRef, ContentType};

/// Golden schema version.
pub const GOLDEN_FORMAT: u32 = 1;
/// Upstream version this golden tracks.
pub const MINDY_VERSION: &str = "v146";

/// Committed golden content snapshot (plan 02 §6.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoldenContent {
    /// Schema version (`GOLDEN_FORMAT`).
    pub format: u32,
    /// Upstream version (`MINDY_VERSION`).
    pub mindustry_version: String,
    /// How the golden was produced.
    pub generator: String,
    /// Per-type record counts.
    pub counts: BTreeMap<String, usize>,
    /// Ordered per-type entries.
    pub types: Vec<GoldenType>,
    /// Tech-tree structure (Serpulo, Erekir).
    #[serde(default)]
    pub tech_trees: Vec<GoldenTree>,
    /// `SaveFileReader.modContentNameMap` legacy fallbacks.
    #[serde(default)]
    pub mod_content_name_map: BTreeMap<String, String>,
}

/// One content type section of the golden.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoldenType {
    /// `ContentType.name()`.
    #[serde(rename = "type")]
    pub type_: String,
    /// Entries in id order.
    pub entries: Vec<GoldenEntry>,
}

/// One golden content record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoldenEntry {
    /// Dense id.
    pub id: u16,
    /// Mappable name (`None` for bullets).
    pub name: Option<String>,
    /// Java class-ish kind tag.
    pub kind: String,
    /// Localized name captured at construction.
    pub localized: String,
    /// Expected bundle key (`<type>.<name>.name`, `command.<name>`, ...).
    pub bundle: Option<GoldenBundle>,
    /// Expected authored sprite regions (items/liquids; plan 03 extends).
    #[serde(default)]
    pub regions: Vec<String>,
    /// Selected metadata fields (field-diff audit input).
    #[serde(default)]
    pub fields: Map<String, Value>,
}

/// Bundle key expectation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoldenBundle {
    /// Bundle key for the localized name.
    pub name: String,
}

/// One tech tree in the golden.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoldenTree {
    /// Root name (`techtree.<name>`).
    pub root: String,
    /// Nodes in DFS creation order.
    pub nodes: Vec<GoldenNode>,
}

/// One tech-tree node in the golden.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoldenNode {
    /// Content name (`<unknown>`-safe path).
    pub path: String,
    /// Resolved content name, when the registry knew it.
    pub content: Option<String>,
    /// Parent content name.
    pub parent: Option<String>,
    /// Depth in the tree.
    pub depth: u32,
    /// Item-name/amount requirements.
    pub requirements: Vec<(String, i32)>,
    /// Objective descriptions.
    #[serde(default)]
    pub objectives: Vec<String>,
}

/// Bundle keys file (`parity/bundle_keys.json`; source-derived).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BundleKeysFile {
    /// Provenance note.
    pub source: String,
    /// Raw key/value map from `bundle.properties`.
    pub keys: BTreeMap<String, String>,
    /// Vanilla content whose `<type>.<name>.name` key is absent upstream
    /// (internal/hidden records: `air`, `build*`, ores, ...). Missing keys
    /// outside this list are audit failures.
    #[serde(default)]
    pub exempt: Vec<String>,
}

/// Asset manifest (`parity/asset_manifest.json`; plan 03 owns the real one).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetManifest {
    /// Provenance note.
    #[serde(default)]
    pub source: String,
    /// All atlas region names.
    #[serde(default)]
    pub regions: Vec<String>,
    /// Per-content region expectations (`"type.name"` → regions).
    #[serde(default)]
    pub content: BTreeMap<String, Vec<String>>,
}

/// One audit check row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditCheck {
    /// Check name.
    pub name: String,
    /// `pass`, `fail`, `skipped` or `warn`.
    pub status: String,
    /// Human-readable detail (counts, first failure).
    pub detail: String,
}

/// Mechanical audit result (plan 02 §7b).
#[derive(Debug, Clone, Serialize, Default)]
pub struct AuditReport {
    /// Golden path/description being audited.
    pub golden: String,
    /// Checks performed.
    pub checks: Vec<AuditCheck>,
    /// Hard failures (exit nonzero).
    pub errors: Vec<String>,
    /// Soft findings (ledger triage).
    pub warnings: Vec<String>,
}

impl AuditReport {
    /// Whether no hard failure occurred.
    pub fn pass(&self) -> bool {
        self.errors.is_empty()
    }

    fn check(&mut self, name: &str, status: &str, detail: String) {
        self.checks.push(AuditCheck {
            name: name.to_owned(),
            status: status.to_owned(),
            detail,
        });
    }
}

/// Live content types in ordinal order (goldens cover 12 types).
pub fn live_types() -> Vec<ContentType> {
    ContentType::ALL
        .iter()
        .copied()
        .filter(|type_| type_.is_live())
        .collect()
}

/// Bundle key for a mappable content (`<type>.<name>.name` unless overridden).
pub fn bundle_key(type_: ContentType, name: &str) -> Option<String> {
    match type_ {
        ContentType::UnitCommand => Some(format!("command.{name}")),
        ContentType::UnitStance => Some(format!("stance.{name}")),
        ContentType::Bullet | ContentType::Error => None,
        _ => Some(format!("{}.{name}.name", type_.name())),
    }
}

/// Bundle key recorded on the record itself when it overrides the convention
/// (item stances use `stance.mine`; commands use `command.<name>`).
pub fn recorded_bundle_key(
    registry: &ContentRegistry,
    type_: ContentType,
    id: u16,
    name: &str,
) -> Option<String> {
    match type_ {
        ContentType::UnitCommand => registry
            .unit_command(super::id::UnitCommandId::new(id))
            .map(|record| record.localized_key.clone()),
        ContentType::UnitStance => registry
            .unit_stance(super::id::UnitStanceId::new(id))
            .map(|record| record.localized_key.clone()),
        _ => bundle_key(type_, name),
    }
}

/// Type-erased unlock fields lookup (unlockable content only).
pub fn unlock_fields_of(registry: &ContentRegistry, content: ContentRef) -> Option<&UnlockFields> {
    use super::id::{ItemId, LiquidId, PlanetId, SectorId, StatusId, UnitTypeId, WeatherId};

    match content.type_ {
        ContentType::Item => registry
            .item(ItemId::new(content.id))
            .and_then(|record| record.unlock_fields()),
        ContentType::Block => registry
            .block(super::id::BlockId::new(content.id))
            .and_then(|record| record.unlock_fields()),
        ContentType::Liquid => registry
            .liquid(LiquidId::new(content.id))
            .and_then(|record| record.unlock_fields()),
        ContentType::Status => registry
            .status(StatusId::new(content.id))
            .and_then(|record| record.unlock_fields()),
        ContentType::Unit => registry
            .unit(UnitTypeId::new(content.id))
            .and_then(|record| record.unlock_fields()),
        ContentType::Weather => registry
            .weather(WeatherId::new(content.id))
            .and_then(|record| record.unlock_fields()),
        ContentType::Sector => registry
            .sector(SectorId::new(content.id))
            .and_then(|record| record.unlock_fields()),
        ContentType::Planet => registry
            .planet(PlanetId::new(content.id))
            .and_then(|record| record.unlock_fields()),
        ContentType::Team => registry
            .team(super::id::TeamEntryId::new(content.id))
            .and_then(|record| record.unlock_fields()),
        _ => None,
    }
}

/// JSON-safe f32: shortest decimal that round-trips to the same `f32`, parsed
/// back to `f64`. Storing the widened `f32` directly would not survive a
/// JSON round-trip (shortest-`f64` output can differ by 1 ULP from the widened
/// value, breaking golden diffs).
fn finite(value: f32) -> Value {
    if !value.is_finite() {
        return Value::Null;
    }
    match format!("{value}").parse::<f64>() {
        Ok(parsed) => Value::from(parsed),
        Err(_) => Value::Null,
    }
}

fn entry_fields(registry: &ContentRegistry, type_: ContentType, id: u16) -> Map<String, Value> {
    use super::id::{BlockId, BulletId, ItemId, LiquidId, UnitTypeId};

    let mut fields = Map::new();
    match type_ {
        ContentType::Item => {
            if let Some(record) = registry.item(ItemId::new(id)) {
                fields.insert("hardness".into(), Value::from(record.hardness));
                fields.insert("cost".into(), finite(record.cost));
                fields.insert("health_scaling".into(), finite(record.health_scaling));
                fields.insert("explosiveness".into(), finite(record.explosiveness));
                fields.insert("flammability".into(), finite(record.flammability));
                fields.insert("radioactivity".into(), finite(record.radioactivity));
                fields.insert("charge".into(), finite(record.charge));
                fields.insert("buildable".into(), Value::from(record.buildable));
                fields.insert("hidden".into(), Value::from(record.hidden));
                fields.insert("frames".into(), Value::from(record.frames));
                fields.insert(
                    "transition_frames".into(),
                    Value::from(record.transition_frames),
                );
                fields.insert("frame_time".into(), finite(record.frame_time));
            }
        }
        ContentType::Block => {
            if let Some(record) = registry.block(BlockId::new(id)) {
                fields.insert("size".into(), Value::from(record.size));
                fields.insert("health".into(), Value::from(record.health));
                fields.insert("scaled_health".into(), finite(record.scaled_health));
                fields.insert("armor".into(), finite(record.armor));
                fields.insert("category".into(), Value::from(record.category.name()));
                fields.insert(
                    "build_cost_multiplier".into(),
                    finite(record.build_cost_multiplier),
                );
                fields.insert("build_time".into(), finite(record.build_time));
                fields.insert("item_capacity".into(), Value::from(record.item_capacity));
                fields.insert("liquid_capacity".into(), finite(record.liquid_capacity));
                fields.insert("solid".into(), Value::from(record.solid));
                fields.insert("update".into(), Value::from(record.update));
                fields.insert("configurable".into(), Value::from(record.configurable));
                fields.insert("has_items".into(), Value::from(record.has_items));
                fields.insert("has_liquids".into(), Value::from(record.has_liquids));
                fields.insert("has_power".into(), Value::from(record.has_power));
            }
        }
        ContentType::Unit => {
            if let Some(record) = registry.unit(UnitTypeId::new(id)) {
                fields.insert("health".into(), finite(record.health));
                fields.insert("armor".into(), finite(record.armor));
                fields.insert("speed".into(), finite(record.speed));
                fields.insert("hit_size".into(), finite(record.hit_size));
                fields.insert("rotate_speed".into(), finite(record.rotate_speed));
                fields.insert("item_capacity".into(), Value::from(record.item_capacity));
                fields.insert("range".into(), finite(record.range));
                fields.insert("max_range".into(), finite(record.max_range));
                fields.insert("fog_radius".into(), finite(record.fog_radius));
                fields.insert("payload_capacity".into(), finite(record.payload_capacity));
                fields.insert("hidden".into(), Value::from(record.hidden));
                fields.insert("can_attack".into(), Value::from(record.can_attack));
            }
        }
        ContentType::Bullet => {
            if let Some(record) = registry.bullet(BulletId::new(id)) {
                fields.insert("speed".into(), finite(record.speed));
                fields.insert("lifetime".into(), finite(record.lifetime));
                fields.insert("damage".into(), finite(record.damage));
                fields.insert("splash_damage".into(), finite(record.splash_damage));
                fields.insert("pierce".into(), Value::from(record.pierce));
                fields.insert("collides".into(), Value::from(record.collides));
            }
        }
        ContentType::Liquid => {
            if let Some(record) = registry.liquid(LiquidId::new(id)) {
                fields.insert("gas".into(), Value::from(record.gas));
                fields.insert("temperature".into(), finite(record.temperature));
                fields.insert("viscosity".into(), finite(record.viscosity));
                fields.insert("flammability".into(), finite(record.flammability));
                fields.insert("heat_capacity".into(), finite(record.heat_capacity));
                fields.insert("explosiveness".into(), finite(record.explosiveness));
                fields.insert("coolant".into(), Value::from(record.coolant));
                fields.insert("hidden".into(), Value::from(record.hidden));
            }
        }
        _ => {}
    }
    fields
}

/// Authored region expectations recorded in the golden (plan 03 extends).
pub fn expected_regions(type_: ContentType, name: &str) -> Vec<String> {
    match type_ {
        ContentType::Item => vec![format!("item-{name}")],
        ContentType::Liquid => vec![format!("liquid-{name}")],
        _ => Vec::new(),
    }
}

/// Builds the golden snapshot from the live registry.
pub fn dump_golden(registry: &ContentRegistry) -> GoldenContent {
    let counts = live_types()
        .iter()
        .map(|type_| (type_.name().to_owned(), registry.type_len(*type_)))
        .collect();
    let types = live_types()
        .iter()
        .map(|type_| GoldenType {
            type_: type_.name().to_owned(),
            entries: registry
                .entries(*type_)
                .into_iter()
                .map(|entry| {
                    let name = entry.name.map(str::to_owned);
                    // Commands/stances do not store `UnlockFields`; their
                    // localized name is the bundle value at UI time (plan 14),
                    // so the golden falls back to the content name for them.
                    let localized = name
                        .as_deref()
                        .map(|name| {
                            unlock_fields_of(registry, ContentRef::new(*type_, entry.id))
                                .map(|fields| fields.localized_name.clone())
                                .unwrap_or_else(|| name.to_owned())
                        })
                        .unwrap_or_default();
                    let bundle = name
                        .as_deref()
                        .and_then(|name| recorded_bundle_key(registry, *type_, entry.id, name))
                        .map(|name| GoldenBundle { name });
                    let regions = name
                        .as_deref()
                        .map(|name| expected_regions(*type_, name))
                        .unwrap_or_default();
                    GoldenEntry {
                        id: entry.id,
                        name,
                        kind: entry.kind.to_owned(),
                        localized,
                        bundle,
                        regions,
                        fields: entry_fields(registry, *type_, entry.id),
                    }
                })
                .collect(),
        })
        .collect();

    let tech_trees = registry
        .tech()
        .trees
        .iter()
        .map(|tree| dump_tree(registry, tree))
        .collect();
    let mod_content_name_map = mod_content_name_map_entries()
        .iter()
        .map(|&(from, to)| (from.to_owned(), to.to_owned()))
        .collect();

    GoldenContent {
        format: GOLDEN_FORMAT,
        mindustry_version: MINDY_VERSION.to_owned(),
        generator: String::from(
            "source-derived fallback (NUD-10): mind-headless content dump over parity/tools generators",
        ),
        counts,
        types,
        tech_trees,
        mod_content_name_map,
    }
}

fn dump_tree(registry: &ContentRegistry, tree: &super::tech::TechTree) -> GoldenTree {
    let mut nodes = Vec::new();
    registry.tech().each(tree.root, &mut |node_ref| {
        if let Some(node) = registry.tech().node(node_ref) {
            let parent = node.parent.and_then(|parent| {
                registry
                    .tech()
                    .node(parent)
                    .map(|record| record.content_name.clone())
            });
            let requirements = node
                .requirements
                .iter()
                .map(|stack| {
                    let item = registry
                        .item(stack.item)
                        .map(|record| record.name.clone())
                        .unwrap_or_else(|| format!("item#{}", stack.item.raw()));
                    (item, stack.amount)
                })
                .collect();
            nodes.push(GoldenNode {
                path: node.content_name.clone(),
                content: node
                    .content
                    .and_then(|content| content_name(registry, content)),
                parent,
                depth: node.depth,
                requirements,
                objectives: node
                    .objectives
                    .iter()
                    .map(|objective| objective_name(registry, objective))
                    .collect(),
            });
        }
    });
    GoldenTree {
        root: tree.name.clone(),
        nodes,
    }
}

fn content_name(registry: &ContentRegistry, content: ContentRef) -> Option<String> {
    use super::id::{BlockId, ItemId, LiquidId, PlanetId, SectorId, StatusId, UnitTypeId};

    match content.type_ {
        ContentType::Item => registry
            .item(ItemId::new(content.id))
            .map(|r| r.name.clone()),
        ContentType::Block => registry
            .block(BlockId::new(content.id))
            .map(|r| r.name.clone()),
        ContentType::Liquid => registry
            .liquid(LiquidId::new(content.id))
            .map(|r| r.name.clone()),
        ContentType::Status => registry
            .status(StatusId::new(content.id))
            .map(|r| r.name.clone()),
        ContentType::Unit => registry
            .unit(UnitTypeId::new(content.id))
            .map(|r| r.name.clone()),
        ContentType::Sector => registry
            .sector(SectorId::new(content.id))
            .map(|r| r.name.clone()),
        ContentType::Planet => registry
            .planet(PlanetId::new(content.id))
            .map(|r| r.name.clone()),
        _ => None,
    }
}

fn objective_name(registry: &ContentRegistry, objective: &super::tech::ObjectiveSpec) -> String {
    use super::tech::ObjectiveSpec;
    let named = |content: &ContentRef| {
        content_name(registry, *content)
            .unwrap_or_else(|| format!("{}#{}", content.type_, content.id))
    };
    let sector = |id: &super::id::SectorId| {
        registry
            .sector(*id)
            .map(|record| record.name.clone())
            .unwrap_or_else(|| format!("sector#{}", id.raw()))
    };
    match objective {
        ObjectiveSpec::SectorComplete(id) => format!("sector-complete:{}", sector(id)),
        ObjectiveSpec::Research(content) => format!("research:{}", named(content)),
        ObjectiveSpec::Produce(content) => format!("produce:{}", named(content)),
        ObjectiveSpec::OnSector(id) => format!("on-sector:{}", sector(id)),
        ObjectiveSpec::OnPlanet(id) => format!("on-planet:{}", id.raw()),
    }
}

/// Runs the mechanical audit against the golden plus optional inputs.
pub fn audit(
    registry: &ContentRegistry,
    golden: &GoldenContent,
    bundle: Option<&BundleKeysFile>,
    manifest: Option<&AssetManifest>,
) -> AuditReport {
    let mut report = AuditReport {
        golden: format!("{} ({})", golden.mindustry_version, golden.generator),
        ..AuditReport::default()
    };

    audit_counts(registry, golden, &mut report);
    audit_entries(registry, golden, &mut report);
    audit_tech(registry, golden, &mut report);
    audit_name_map(golden, &mut report);
    audit_dangling(registry, &mut report);
    audit_bundle(golden, bundle, &mut report);
    audit_regions(registry, golden, manifest, &mut report);

    report
}

fn audit_counts(registry: &ContentRegistry, golden: &GoldenContent, report: &mut AuditReport) {
    let mut mismatches = Vec::new();
    for type_ in live_types() {
        let name = type_.name();
        let expected = golden.counts.get(name).copied().unwrap_or(0);
        let actual = registry.type_len(type_);
        if expected != actual {
            mismatches.push(format!("{name}: golden {expected} != actual {actual}"));
        }
    }
    if golden.counts.len() != live_types().len() {
        mismatches.push(format!(
            "golden has {} type sections, live registry has {}",
            golden.counts.len(),
            live_types().len()
        ));
    }
    status(
        report,
        "counts",
        &mismatches,
        format!("{} types compared", live_types().len()),
    );
}

fn audit_entries(registry: &ContentRegistry, golden: &GoldenContent, report: &mut AuditReport) {
    let mut mismatches = Vec::new();
    for type_ in live_types() {
        let name = type_.name();
        let Some(golden_type) = golden.types.iter().find(|entry| entry.type_ == name) else {
            mismatches.push(format!("{name}: missing golden section"));
            continue;
        };
        let actual = registry.entries(type_);
        if actual.len() != golden_type.entries.len() {
            mismatches.push(format!(
                "{name}: golden {} entries != actual {}",
                golden_type.entries.len(),
                actual.len()
            ));
        }
        for (index, (expected, found)) in golden_type.entries.iter().zip(actual.iter()).enumerate()
        {
            if expected.id != found.id
                || expected.name.as_deref() != found.name
                || expected.kind != found.kind
            {
                mismatches.push(format!(
                    "{name}[{index}]: golden ({}:{:?}:{}) != actual ({}:{:?}:{})",
                    expected.id, expected.name, expected.kind, found.id, found.name, found.kind
                ));
                break;
            }
            if expected.localized
                != unlock_fields_of(registry, ContentRef::new(type_, found.id))
                    .map(|fields| fields.localized_name.as_str())
                    .unwrap_or_else(|| found.name.unwrap_or(""))
            {
                mismatches.push(format!(
                    "{name}[{index}] `{}`: localized golden `{}` != actual",
                    found.name.unwrap_or("?"),
                    expected.localized
                ));
                break;
            }
            let actual_fields = entry_fields(registry, type_, found.id);
            if expected.fields != actual_fields {
                let mut differing: Vec<String> = Vec::new();
                for key in expected.fields.keys().chain(actual_fields.keys()) {
                    let expected_value = expected.fields.get(key);
                    let actual_value = actual_fields.get(key);
                    if expected_value != actual_value {
                        differing.push(format!("{key}: {expected_value:?} != {actual_value:?}"));
                    }
                }
                differing.sort();
                differing.dedup();
                mismatches.push(format!(
                    "{name}[{index}] `{}`: metadata fields differ ({})",
                    found.name.unwrap_or("?"),
                    differing.join(", ")
                ));
            }
        }
    }
    status(
        report,
        "names_ids_fields",
        &mismatches,
        String::from("id/name/kind/localized/fields compared"),
    );
}

fn audit_tech(registry: &ContentRegistry, golden: &GoldenContent, report: &mut AuditReport) {
    let mut mismatches = Vec::new();
    let actual: Vec<GoldenTree> = registry
        .tech()
        .trees
        .iter()
        .map(|tree| dump_tree(registry, tree))
        .collect();
    if actual.len() != golden.tech_trees.len() {
        mismatches.push(format!(
            "golden has {} trees, actual {}",
            golden.tech_trees.len(),
            actual.len()
        ));
    }
    for (expected, found) in golden.tech_trees.iter().zip(actual.iter()) {
        if expected.root != found.root {
            mismatches.push(format!(
                "tree root golden `{}` != actual `{}`",
                expected.root, found.root
            ));
            continue;
        }
        if expected.nodes.len() != found.nodes.len() {
            mismatches.push(format!(
                "{}: golden {} nodes != actual {}",
                expected.root,
                expected.nodes.len(),
                found.nodes.len()
            ));
        }
        for (index, (expected_node, found_node)) in
            expected.nodes.iter().zip(found.nodes.iter()).enumerate()
        {
            if expected_node != found_node {
                mismatches.push(format!(
                    "{}[{index}] `{}`: golden {expected_node:?} != actual {found_node:?}",
                    expected.root, found_node.path
                ));
                break;
            }
        }
    }
    status(
        report,
        "tech_trees",
        &mismatches,
        format!("{} trees compared", golden.tech_trees.len()),
    );
}

fn audit_name_map(golden: &GoldenContent, report: &mut AuditReport) {
    let mut mismatches = Vec::new();
    for &(from, to) in mod_content_name_map_entries() {
        match golden.mod_content_name_map.get(from) {
            Some(expected) if expected == to => {}
            other => mismatches.push(format!("{from} -> golden {other:?} != {to}")),
        }
    }
    status(
        report,
        "mod_content_name_map",
        &mismatches,
        format!("{} fallbacks compared", golden.mod_content_name_map.len()),
    );
}

fn audit_dangling(registry: &ContentRegistry, report: &mut AuditReport) {
    let mut mismatches = Vec::new();
    for (tree, build) in registry.tech_build_reports() {
        if !build.missing.is_empty() || build.unresolved_nodes != 0 {
            mismatches.push(format!(
                "{tree}: {} missing names, {} unresolved nodes",
                build.missing.len(),
                build.unresolved_nodes
            ));
        }
    }
    if let Err(error) = registry.log_content() {
        mismatches.push(error.to_string());
    }
    status(
        report,
        "dangling_refs",
        &mismatches,
        String::from("dense ids + tech-tree resolution"),
    );
}

fn audit_bundle(golden: &GoldenContent, bundle: Option<&BundleKeysFile>, report: &mut AuditReport) {
    let Some(bundle) = bundle else {
        report.check("bundle_keys", "skipped", String::from("no bundle provided"));
        return;
    };
    let exempt: std::collections::BTreeSet<&str> =
        bundle.exempt.iter().map(String::as_str).collect();
    let mut missing = Vec::new();
    let mut exempt_count = 0usize;
    let mut checked = 0usize;
    for type_ in &golden.types {
        for entry in &type_.entries {
            let Some(bundle_key) = &entry.bundle else {
                continue;
            };
            checked += 1;
            if bundle.keys.contains_key(&bundle_key.name) {
                continue;
            }
            if exempt.contains(bundle_key.name.as_str()) {
                exempt_count += 1;
            } else {
                missing.push(format!("bundle key missing: {}", bundle_key.name));
            }
        }
    }
    for missing_key in &missing {
        report.errors.push(missing_key.clone());
    }
    report.check(
        "bundle_keys",
        if missing.is_empty() { "pass" } else { "fail" },
        format!(
            "{checked} keys checked, {exempt_count} upstream exemptions, {} missing",
            missing.len()
        ),
    );
}

fn audit_regions(
    registry: &ContentRegistry,
    golden: &GoldenContent,
    manifest: Option<&AssetManifest>,
    report: &mut AuditReport,
) {
    let Some(manifest) = manifest else {
        report.check("regions", "skipped", String::from("no manifest provided"));
        return;
    };
    let available: std::collections::BTreeSet<&str> =
        manifest.regions.iter().map(String::as_str).collect();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Manifest-declared expectations are hard requirements.
    for (key, regions) in &manifest.content {
        let mut parts = key.splitn(2, '.');
        let type_name = parts.next().unwrap_or_default();
        let content_name = parts.next().unwrap_or_default();
        let Some(type_) = ContentType::ALL
            .iter()
            .copied()
            .find(|type_| type_.name() == type_name)
        else {
            errors.push(format!("manifest content `{key}`: unknown type"));
            continue;
        };
        let resolved = registry.get_by_name(type_, content_name);
        if resolved.is_none() {
            errors.push(format!("manifest content `{key}`: not in registry"));
            continue;
        }
        for region in regions {
            if !available.contains(region.as_str()) {
                errors.push(format!(
                    "manifest content `{key}`: missing region `{region}`"
                ));
            }
        }
    }

    // Golden authored expectations are soft (plan 03 owns the final manifest).
    let mut checked = 0usize;
    for type_ in &golden.types {
        for entry in &type_.entries {
            for region in &entry.regions {
                checked += 1;
                if !available.contains(region.as_str()) {
                    warnings.push(format!(
                        "{}.{}: expected authored region `{region}` absent from manifest",
                        type_.type_,
                        entry.name.as_deref().unwrap_or("?")
                    ));
                }
            }
        }
    }
    for warning in &warnings {
        report.warnings.push(warning.clone());
    }
    if errors.is_empty() {
        report.check(
            "regions",
            if warnings.is_empty() { "pass" } else { "warn" },
            format!(
                "{} manifest entries, {} regions; {checked} authored expectations, {} soft misses",
                manifest.content.len(),
                manifest.regions.len(),
                warnings.len()
            ),
        );
    } else {
        for error in &errors {
            report.errors.push(error.clone());
        }
        report.check(
            "regions",
            "fail",
            format!("{} manifest region failures", errors.len()),
        );
    }
}

fn status(report: &mut AuditReport, name: &str, errors: &[String], detail: String) {
    if errors.is_empty() {
        report.check(name, "pass", detail);
    } else {
        for error in errors {
            report.errors.push(error.clone());
        }
        report.check(
            name,
            "fail",
            format!(
                "{}: {}",
                errors.len(),
                errors.first().cloned().unwrap_or_default()
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::bundle::MemoryBundle;
    use crate::content::registries::create_base_content;
    use crate::content::settings_store::MemoryUnlockStore;

    const GOLDEN: &str = include_str!("../../../../../parity/golden_content.json");
    const BUNDLE: &str = include_str!("../../../../../parity/bundle_keys.json");
    const MANIFEST: &str = include_str!("../../../../../parity/asset_manifest.json");

    fn golden() -> GoldenContent {
        serde_json::from_str(GOLDEN).expect("golden parses")
    }

    fn bundle_file() -> BundleKeysFile {
        serde_json::from_str(BUNDLE).expect("bundle keys parse")
    }

    fn manifest() -> AssetManifest {
        serde_json::from_str(MANIFEST).expect("manifest parses")
    }

    fn boot(keys: &BTreeMap<String, String>) -> ContentRegistry {
        let bundle =
            MemoryBundle::with_pairs(keys.iter().map(|(key, value)| (key.clone(), value.clone())));
        let store = MemoryUnlockStore::new();
        let mut registry = create_base_content(&bundle, &store, true).expect("create base content");
        registry.init().expect("content init");
        registry.post_init().expect("content post init");
        registry
    }

    /// Plan 02 §7a: `parity::tests::names_ids_match_golden` — ordered IDs,
    /// names, kinds and counts match the committed golden.
    #[test]
    fn names_ids_match_golden() {
        let bundle = bundle_file();
        let registry = boot(&bundle.keys);
        let report = audit(&registry, &golden(), Some(&bundle), Some(&manifest()));
        let names = report
            .checks
            .iter()
            .find(|check| check.name == "names_ids_fields")
            .expect("names check ran");
        assert_eq!(names.status, "pass", "{}", names.detail);
        let counts = report
            .checks
            .iter()
            .find(|check| check.name == "counts")
            .expect("counts check ran");
        assert_eq!(counts.status, "pass", "{}", counts.detail);
    }

    /// Plan 02 §7a: `parity::tests::bundle_keys` — every golden bundle key is
    /// present in the source-derived `bundle.properties` key set.
    #[test]
    fn bundle_keys() {
        let bundle = bundle_file();
        let registry = boot(&bundle.keys);
        let report = audit(&registry, &golden(), Some(&bundle), Some(&manifest()));
        let check = report
            .checks
            .iter()
            .find(|check| check.name == "bundle_keys")
            .expect("bundle check ran");
        assert_eq!(check.status, "pass", "{}", check.detail);
    }

    /// Plan 02 §7a: `parity::tests::regions` — manifest content expectations
    /// resolve and all declared regions exist in the manifest region set.
    #[test]
    fn regions() {
        let bundle = bundle_file();
        let registry = boot(&bundle.keys);
        let report = audit(&registry, &golden(), Some(&bundle), Some(&manifest()));
        let check = report
            .checks
            .iter()
            .find(|check| check.name == "regions")
            .expect("region check ran");
        assert_eq!(check.status, "pass", "{}", check.detail);
    }

    /// Plan 02 §7a: `parity::tests::field_diff` — selected metadata fields and
    /// tech-tree structure match the golden byte-for-byte.
    #[test]
    fn field_diff() {
        let bundle = bundle_file();
        let registry = boot(&bundle.keys);
        let report = audit(&registry, &golden(), Some(&bundle), Some(&manifest()));
        let tech = report
            .checks
            .iter()
            .find(|check| check.name == "tech_trees")
            .expect("tech check ran");
        assert_eq!(tech.status, "pass", "{}", tech.detail);
        assert!(
            report.errors.is_empty(),
            "golden audit errors: {:?}",
            report.errors
        );
    }
}

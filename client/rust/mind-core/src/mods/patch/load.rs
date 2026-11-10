// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Patch loading: the post-apply content-array fixups, per-mod patch
//! collection and `unit.targetFlags` resolution.
//!
//! Upstream: core/src/mindustry/mod/DataPatcher.java

use super::*;

/// `DataPatcher.fixContentArrays`: grows/shrinks the registry-backed dense
/// item tables after patch/embedded content added or removed items
/// (`Block.checkContentArrayCapacity` parity).
///
/// The port stores the block-level item tables as the per-item `BlockDef`
/// arrays `item_costs`/`item_health_scaling` (used by `Block.init_self` for
/// `build_time`/`health`). This hook resizes them to the live item count,
/// matching upstream's `Arrays.copyOf(filter, items)`. The world half — live
/// building `ItemModule`/`LiquidModule` tables and the editor tile remap — is
/// [`fix_world_content_arrays`] (plan 09/19). Upstream's `ItemSeq` usage is
/// campaign-local and sized at construction; the port's transient `ItemSeq`s
/// (`UnitType.getRequirements`) are already built with `registry.items().len()`.
pub fn fix_content_arrays(registry: &mut ContentRegistry) {
    // Upstream guards this with `DataPatcher.needsArrayFix` (set when content is
    // created); the port runs it unconditionally, which is idempotent because
    // the resize is a no-op unless the item count changed.
    let item_count = registry.items().len();
    if item_count == 0 {
        return;
    }
    let costs: Vec<f32> = registry.items().iter().map(|item| item.cost).collect();
    let scaling: Vec<f32> = registry
        .items()
        .iter()
        .map(|item| item.health_scaling)
        .collect();
    for block in registry.blocks_mut() {
        // `!Vars.content.blocks().synthetic()` blocks are still resized upstream;
        // the port has no `synthetic()` flag, so every block is handled.
        if block.item_costs.len() != item_count {
            block.item_costs = costs.clone();
        }
        if block.item_health_scaling.len() != item_count {
            block.item_health_scaling = scaling.clone();
        }
    }
}

/// `DataPatcher.fixContentArrays` world half: grows every live building's
/// [`ItemModule`](crate::world::ItemModule)/[`LiquidModule`](crate::world::LiquidModule)
/// to the dense content counts. Upstream only does this for the editor world
/// (`!Vars.headless && ui.editor.isShown()`); the port exposes it as a hook for
/// plan 09/19 instead of reaching into `Vars.world`.
pub fn fix_world_content_arrays(world: &mut bevy_ecs::world::World, registry: &ContentRegistry) {
    let items = registry.items().len();
    let liquids = registry.liquids().len();
    let mut item_query = world.query::<&mut crate::world::ItemModule>();
    for mut module in item_query.iter_mut(world) {
        module.check_array_capacity(items);
    }
    let mut liquid_query = world.query::<&mut crate::world::LiquidModule>();
    for mut module in liquid_query.iter_mut(world) {
        module.check_array_capacity(liquids);
    }
}

/// `Mods.loadModPatches`: applies each enabled mod's `patches/**.json` with a
/// fresh [`DataPatcher`] (so mods never reset each other), files sorted.
///
/// Returns one warning string per mod whose patches failed to apply.
pub fn load_mod_patches(
    mods: &mut Mods,
    registry: &mut ContentRegistry,
    fs: &dyn FileSystem,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for set in collect_mod_patches(mods, fs) {
        let mut patcher = DataPatcher::new();
        if let Err(error) = patcher.apply(registry, &set.assets) {
            warnings.push(format!(
                "Failed to apply patches from mod {}: {error}",
                set.mod_name
            ));
        }
        warnings.extend(set.warnings);
    }
    warnings
}

/// One mod's `patches/**.json` files (sorted) for the I07 T4 boot path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModPatchSet {
    /// Internal mod name.
    pub mod_name: String,
    /// Patch assets in sorted path order.
    pub assets: Vec<PatchAsset>,
    /// Unreadable files reported by the walk.
    pub warnings: Vec<String>,
}

/// Collects each enabled, non-hidden mod's patch files in dependency order
/// (`Mods.loadModPatches` file half).
pub fn collect_mod_patches(mods: &mut Mods, fs: &dyn FileSystem) -> Vec<ModPatchSet> {
    let mut sets = Vec::new();
    for index in mods.ordered_mods() {
        let Some(mod_) = mods.mod_at(index) else {
            continue;
        };
        if mod_.meta.hidden {
            continue;
        }
        let Ok(entries) = mod_.root.walk(fs, "patches") else {
            continue;
        };
        let mut paths: Vec<String> = entries
            .into_iter()
            .filter(|path| {
                matches!(
                    path.rsplit('.').next().unwrap_or(""),
                    "json" | "hjson" | "json5"
                )
            })
            .collect();
        if paths.is_empty() {
            continue;
        }
        paths.sort();
        let mut set = ModPatchSet {
            mod_name: mod_.name.clone(),
            ..ModPatchSet::default()
        };
        for path in paths {
            let Ok(json) = mod_.root.read_to_string(fs, &path) else {
                set.warnings
                    .push(format!("mod `{}`: could not read `{path}`", mod_.name));
                continue;
            };
            set.assets.push(PatchAsset { name: path, json });
        }
        sets.push(set);
    }
    sets
}

/// Parses a top-level patch type name (`block`/`item`/… all nine kinds).
pub(super) fn content_type_from_name(name: &str) -> Option<ContentType> {
    match name {
        "item" | "items" => Some(ContentType::Item),
        "block" | "blocks" => Some(ContentType::Block),
        "liquid" | "liquids" => Some(ContentType::Liquid),
        "status" | "statuses" => Some(ContentType::Status),
        "unit" | "units" => Some(ContentType::Unit),
        "weather" | "weathers" => Some(ContentType::Weather),
        "sector" | "sectors" => Some(ContentType::Sector),
        "planet" | "planets" => Some(ContentType::Planet),
        "team" | "teams" => Some(ContentType::Team),
        _ => None,
    }
}

/// Parses typed `liquid`/`liquids` consume entries: `["water/10"]`,
/// `{"water": 10}`, or a single `"water/10"` string.
pub(super) fn parse_liquid_stacks(
    registry: &ContentRegistry,
    value: &Value,
) -> Result<Vec<LiquidStack>, String> {
    let resolve = |name: &str| {
        registry
            .liquid_id(name)
            .or_else(|| registry.liquid_id(&registry.transform_name(name)))
            .ok_or_else(|| format!("unknown liquid `{name}`"))
    };
    let entries: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        Value::String(_) => vec![value],
        Value::Object(map) => {
            let mut out = Vec::with_capacity(map.len());
            for (name, amount) in map {
                out.push(LiquidStack::new(
                    resolve(name)?,
                    amount.as_f64().unwrap_or(0.0) as f32,
                ));
            }
            return Ok(out);
        }
        _ => return Err("`liquids` must be an array/object/string".to_owned()),
    };
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry {
            Value::String(text) => {
                let (name, amount) = text
                    .split_once('/')
                    .map(|(name, amount)| (name, amount.parse::<f32>().unwrap_or(0.0)))
                    .unwrap_or((text.as_str(), 0.0));
                out.push(LiquidStack::new(resolve(name)?, amount));
            }
            Value::Object(map) => {
                let name = map
                    .get("liquid")
                    .and_then(Value::as_str)
                    .ok_or("liquid stack missing `liquid`")?;
                let amount = map.get("amount").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                out.push(LiquidStack::new(resolve(name)?, amount));
            }
            _ => return Err("invalid liquid entry".to_owned()),
        }
    }
    Ok(out)
}

/// Whether a JSON object is an index-keyed container (`{"0": …}`), used to
/// address `Seq` elements in the object form (`PatcherTests.arrayMulti`).
pub(super) fn is_index_map(map: &Map<String, Value>) -> bool {
    !map.is_empty() && map.keys().all(|key| key.parse::<usize>().is_ok())
}

/// Resolves a `unit.targetFlags` name to a [`BlockFlag`] keyword.
pub fn resolve_block_flag(name: &str) -> Option<crate::content::registries::blocks::BlockFlag> {
    use crate::content::registries::blocks::BlockFlag;
    Some(match name {
        "core" => BlockFlag::Core,
        "storage" => BlockFlag::Storage,
        "generator" => BlockFlag::Generator,
        "turret" => BlockFlag::Turret,
        "factory" => BlockFlag::Factory,
        "repair" => BlockFlag::Repair,
        "battery" => BlockFlag::Battery,
        "reactor" => BlockFlag::Reactor,
        "extinguisher" => BlockFlag::Extinguisher,
        "drill" => BlockFlag::Drill,
        "shield" => BlockFlag::Shield,
        "launchPad" => BlockFlag::LaunchPad,
        "unitCargoUnloadPoint" => BlockFlag::UnitCargoUnloadPoint,
        "unitAssembler" => BlockFlag::UnitAssembler,
        "hasFogRadius" => BlockFlag::HasFogRadius,
        "steamVent" => BlockFlag::SteamVent,
        "blockRepair" => BlockFlag::BlockRepair,
        "synced" => BlockFlag::Synced,
        _ => return None,
    })
}

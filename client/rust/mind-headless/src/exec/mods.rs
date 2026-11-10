// SPDX-License-Identifier: GPL-3.0-only

//! `mods` command bodies: discovery, content loading, patches, assets, error isolation, bench and overlay probes (plan 20).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use super::*;

/// Plan 20 M0 (`mods list`): discover fixture/server mods and resolve states.
pub(super) fn cmd_mods_list(dir: &Path, json: bool, check: bool) -> anyhow::Result<i32> {
    use mind_core::io::SettingsStore;
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::Mods;

    let dir = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        paths::find_repo_root(None)?.join(dir)
    };
    let fs = NativeFs;
    let mut settings = SettingsStore::new();
    let mut mods = Mods::new(true, &dir);
    mods.load(&fs, &dir, &mut settings)
        .with_context(|| format!("loading mods from `{}`", dir.display()))?;
    // `Mods.loadScripts` (`mods/mod.rs:464`): the client marks script-only mods
    // `ScriptsUnsupported`; the list report mirrors that state (J09 T5).
    mods.load_scripts(&fs);
    let report = mods.report();

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for entry in &report.mods {
            println!(
                "{}: {:?} enabled={} v{} source={}",
                entry.name, entry.state, entry.enabled, entry.version, entry.source
            );
        }
        println!("{} mod(s) discovered", report.mods.len());
    }

    if !check {
        return Ok(EXIT_PASS);
    }

    let expected_path = dir.join("expected_list.json");
    let text = std::fs::read_to_string(&expected_path)
        .with_context(|| format!("reading `{}`", expected_path.display()))?;
    let expected: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing `{}`", expected_path.display()))?;
    let actual = normalized_mod_list(&serde_json::to_value(&report)?);
    let expected_mods = expected
        .get("mods")
        .cloned()
        .unwrap_or(serde_json::Value::Array(Vec::new()));
    if expected_mods != actual {
        log::error!("mods list mismatch");
        log::error!("expected: {expected_mods}");
        log::error!("actual:   {actual}");
        return Ok(EXIT_FAIL);
    }
    Ok(EXIT_PASS)
}

/// Plan 20 M1 (`mods content`): boot base content + one fixture mod's JSON
/// content and dump the resulting mod content records.
pub(super) fn cmd_mods_content(
    fixture: &str,
    repo: Option<&Path>,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use mind_core::io::SettingsStore;
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::{Mods, provider::ModsContentProvider};

    let root = paths::find_repo_root(repo)?;
    let dir = root.join("parity/mod_fixtures").join(fixture);
    let fs = NativeFs;
    let settings = SettingsStore::new();
    let mut mods = Mods::new(true, &dir);
    mods.load_single(&fs, &dir, &settings)
        .with_context(|| format!("loading fixture `{fixture}`"))?;
    let files = mods.collect_content_files(&fs);
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut provider = ModsContentProvider::new(files, &store);

    let mut registry = create_base_content(&bundle, &store, true)
        .map_err(|error| anyhow!("base content: {error}"))?;
    registry
        .init()
        .map_err(|error| anyhow!("content init: {error}"))?;
    registry
        .post_init()
        .map_err(|error| anyhow!("content post-init: {error}"))?;
    let content_result = registry.create_mod_content(&mut provider);
    let errors: Vec<String> = match &content_result {
        Ok(()) => Vec::new(),
        Err(errors) => errors.iter().map(|error| error.to_string()).collect(),
    };

    let items: Vec<serde_json::Value> = registry
        .items()
        .iter()
        .filter(|item| item.minfo.is_modded())
        .map(|item| serde_json::json!({"name": item.name, "id": item.id.raw()}))
        .collect();
    let blocks: Vec<serde_json::Value> = registry
        .blocks()
        .iter()
        .filter(|block| block.minfo.is_modded())
        .map(|block| {
            serde_json::json!({
                "name": block.name,
                "id": block.id.raw(),
                "kind": block.kind.name(),
            })
        })
        .collect();
    let liquids: Vec<serde_json::Value> = registry
        .liquids()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| serde_json::json!({"name": record.name, "id": record.id.raw(), "gas": record.gas}))
        .collect();
    let statuses: Vec<serde_json::Value> = registry
        .statuses()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| serde_json::json!({"name": record.name, "id": record.id.raw()}))
        .collect();
    let units: Vec<serde_json::Value> = registry
        .units()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| {
            serde_json::json!({
                "name": record.name,
                "id": record.id.raw(),
                "kind": record.kind.name(),
                "weapons": record.weapons.len(),
            })
        })
        .collect();
    let weathers: Vec<serde_json::Value> = registry
        .weathers()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| serde_json::json!({"name": record.name, "id": record.id.raw(), "kind": record.kind.name()}))
        .collect();
    let planets: Vec<serde_json::Value> = registry
        .planets()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| {
            serde_json::json!({
                "name": record.name,
                "id": record.id.raw(),
                "radius": record.radius,
                "sectors": record.sector_count,
            })
        })
        .collect();
    let sectors: Vec<serde_json::Value> = registry
        .sectors()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| serde_json::json!({"name": record.name, "id": record.id.raw(), "planet": record.planet.raw()}))
        .collect();
    let teams: Vec<serde_json::Value> = registry
        .teams()
        .iter()
        .filter(|record| record.minfo.is_modded())
        .map(|record| serde_json::json!({"name": record.name, "id": record.id.raw(), "team": record.team}))
        .collect();
    let warnings: Vec<String> = provider
        .parser()
        .warnings
        .iter()
        .map(|warning| format!("{}: {}", warning.file, warning.message))
        .collect();

    let report = serde_json::json!({
        "fixture": fixture,
        "items": items,
        "blocks": blocks,
        "liquids": liquids,
        "statuses": statuses,
        "units": units,
        "weathers": weathers,
        "planets": planets,
        "sectors": sectors,
        "teams": teams,
        "warnings": warnings,
        "errors": errors,
    });
    let text = format!("{}\n", serde_json::to_string_pretty(&report)?);
    if let Some(path) = dump {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || dump.is_none() {
        print!("{text}");
    }
    if errors.is_empty() {
        Ok(EXIT_PASS)
    } else {
        log::error!("mods content: {} content error(s)", errors.len());
        Ok(EXIT_FAIL)
    }
}

/// Plan 20 M3 (`mods patch`): apply a fixture mod's patches and assert that
/// `unapply` restores the baseline field values.
pub(super) fn cmd_mods_patch(
    fixture: &str,
    repo: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::content::parser_hooks::PatchAsset;
    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use mind_core::io::FileSystem;
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::Mods;
    use mind_core::mods::patch::DataPatcher;

    let root = paths::find_repo_root(repo)?;
    let dir = root.join("parity/mod_fixtures").join(fixture);
    let fs = NativeFs;
    let mut mods = Mods::new(true, &dir);
    mods.load_single(&fs, &dir, &mind_core::io::SettingsStore::new())
        .with_context(|| format!("loading fixture `{fixture}`"))?;
    let patch_dir = dir.join("patches");
    let mut patches = Vec::new();
    if let Ok(files) = fs.walk(&patch_dir) {
        let mut files: Vec<_> = files
            .into_iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        for file in files {
            let text = fs
                .read(&file)
                .with_context(|| format!("reading `{}`", file.display()))?;
            let text = String::from_utf8(text)
                .map_err(|_| anyhow!("`{}` is not UTF-8", file.display()))?;
            patches.push(PatchAsset {
                name: file.display().to_string(),
                json: text,
            });
        }
    }

    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true)
        .map_err(|error| anyhow!("base content: {error}"))?;
    registry
        .init()
        .map_err(|error| anyhow!("content init: {error}"))?;
    let baseline_health = registry
        .block_id("router")
        .and_then(|id| registry.block(id))
        .map(|block| block.health)
        .unwrap_or(0);

    let mut patcher = DataPatcher::new();
    patcher
        .apply(&mut registry, &patches)
        .map_err(|error| anyhow!("apply: {error}"))?;
    let patched_health = registry
        .block_id("router")
        .and_then(|id| registry.block(id))
        .map(|block| block.health)
        .unwrap_or(0);
    let applied = patcher.is_applied();
    patcher.unapply(&mut registry);
    let restored_health = registry
        .block_id("router")
        .and_then(|id| registry.block(id))
        .map(|block| block.health)
        .unwrap_or(0);

    let pass = applied && restored_health == baseline_health && patched_health != baseline_health;
    let report = serde_json::json!({
        "fixture": fixture,
        "patches": patches.len(),
        "baselineHealth": baseline_health,
        "patchedHealth": patched_health,
        "restoredHealth": restored_health,
        "warnings": patcher.warnings(),
        "pass": pass,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "mods patch: {} patch(es), router health {} -> {} -> {}: {}",
            report["patches"],
            baseline_health,
            patched_health,
            restored_health,
            if pass { "PASS" } else { "FAIL" }
        );
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Plan 20 M4 (`mods assets`): load a fixture mod's data assets headlessly and
/// report `dp-` image names, sound ids, bundle merges and external assets.
pub(super) fn cmd_mods_assets(
    fixture: &str,
    repo: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::Mods;
    use mind_core::mods::assets::{DataAsset, DataAssetType, DataAssets, ModDataManager};

    let root = paths::find_repo_root(repo)?;
    let dir = root.join("parity/mod_fixtures").join(fixture);
    let fs = NativeFs;
    let mut mods = Mods::new(true, &dir);
    mods.load_single(&fs, &dir, &mind_core::io::SettingsStore::new())
        .with_context(|| format!("loading fixture `{fixture}`"))?;

    let mut assets = Vec::new();
    for file in mods.collect_content_files(&fs) {
        assets.push(DataAsset::content(file.path, file.type_, file.json));
    }
    let listed: Vec<(DataAssetType, String)> = {
        let mod_ = mods
            .mod_at(0)
            .ok_or_else(|| anyhow!("fixture `{fixture}` has no mod record"))?;
        let mut out = Vec::new();
        for (folder, ty) in [
            ("patches", DataAssetType::Patch),
            ("bundles", DataAssetType::Bundle),
            ("sprites", DataAssetType::Image),
            ("sounds", DataAssetType::Sound),
            ("music", DataAssetType::Music),
        ] {
            if let Ok(files) = mod_.root.walk(&fs, folder) {
                for path in files {
                    out.push((ty, path));
                }
            }
        }
        out
    };
    {
        let mod_ = mods
            .mod_at(0)
            .ok_or_else(|| anyhow!("fixture `{fixture}` has no mod record"))?;
        for (ty, path) in listed {
            match ty {
                DataAssetType::Patch => {
                    if let Ok(text) = mod_.root.read_to_string(&fs, &path) {
                        assets.push(DataAsset::patch(path, text));
                    }
                }
                DataAssetType::Bundle => {
                    if let Ok(text) = mod_.root.read_to_string(&fs, &path) {
                        assets.push(DataAsset::bundle(path, &text));
                    }
                }
                _ => {
                    if let Ok(bytes) = mod_.root.read(&fs, &path) {
                        assets.push(DataAsset::blob(path, ty, bytes, false));
                    }
                }
            }
        }
    }

    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true)
        .map_err(|error| anyhow!("base content: {error}"))?;
    registry
        .init()
        .map_err(|error| anyhow!("content init: {error}"))?;

    let mut manager = ModDataManager::new();
    manager
        .load(assets, &mut registry)
        .map_err(|error| anyhow!("asset load: {error}"))?;
    manager.regenerate_content_sprites(false);

    let images: Vec<serde_json::Value> = manager
        .image_applier()
        .entries()
        .iter()
        .map(|entry| {
            serde_json::json!({
                "name": entry.name,
                "page": entry.page.name(),
                "generated": entry.generated,
            })
        })
        .collect();
    let audio: Vec<serde_json::Value> = manager
        .audio_applier()
        .entries()
        .iter()
        .map(|entry| {
            serde_json::json!({
                "name": entry.name,
                "id": entry.id,
                "streaming": entry.streaming,
                "music": entry.music,
            })
        })
        .collect();
    let external: Vec<&str> = manager
        .ordered_external_assets()
        .iter()
        .map(|asset| asset.path.as_str())
        .collect();
    let missing: Vec<&str> = manager
        .get_missing_assets()
        .iter()
        .map(|asset| asset.path.as_str())
        .collect();
    let bundles: Vec<&str> = manager.bundle_applier().files().collect();

    let report = serde_json::json!({
        "fixture": fixture,
        "assets": manager.all_assets().len(),
        "external": external,
        "missing": missing,
        "bundles": bundles,
        "images": images,
        "audio": audio,
        "logicVars": manager.audio_applier().logic_vars(),
        "contentErrors": manager.content_errors(),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "mods assets: {} assets ({} external), {} images, {} audio, {} bundle(s): PASS",
            report["assets"],
            external.len(),
            images.len(),
            audio.len(),
            bundles.len()
        );
    }
    Ok(EXIT_PASS)
}

/// Total content records across every mappable kind (registry size).
pub(super) fn script_content_count(registry: &mind_core::content::ContentRegistry) -> usize {
    registry.items().len()
        + registry.blocks().len()
        + registry.liquids().len()
        + registry.statuses().len()
        + registry.units().len()
        + registry.weathers().len()
        + registry.planets().len()
        + registry.sectors().len()
        + registry.teams().len()
}

/// Plan 20 M1/M3 §7b (`mods error-isolation`): one bad content file among good
/// ones must not remove the good records; the failure is reported instead.
pub(super) fn cmd_mods_error_isolation(
    fixture: &str,
    repo: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use mind_core::io::SettingsStore;
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::{Mods, provider::ModsContentProvider};

    let root = paths::find_repo_root(repo)?;
    let dir = root.join("parity/mod_fixtures").join(fixture);
    let fs = NativeFs;
    let mut mods = Mods::new(true, &dir);
    mods.load_single(&fs, &dir, &SettingsStore::new())
        .with_context(|| format!("loading fixture `{fixture}`"))?;
    let files = mods.collect_content_files(&fs);
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut provider = ModsContentProvider::new(files, &store);

    let mut registry = create_base_content(&bundle, &store, true)
        .map_err(|error| anyhow!("base content: {error}"))?;
    registry
        .init()
        .map_err(|error| anyhow!("content init: {error}"))?;
    registry
        .post_init()
        .map_err(|error| anyhow!("content post-init: {error}"))?;

    let baseline = script_content_count(&registry);
    let result = registry.create_mod_content(&mut provider);
    let errors: Vec<String> = match &result {
        Ok(()) => Vec::new(),
        Err(errors) => errors.iter().map(|error| error.to_string()).collect(),
    };
    let delta = script_content_count(&registry) - baseline;

    let modded_items: Vec<&str> = registry
        .items()
        .iter()
        .filter(|item| item.minfo.is_modded())
        .map(|item| item.name.as_str())
        .collect();
    let modded_blocks: Vec<&str> = registry
        .blocks()
        .iter()
        .filter(|block| block.minfo.is_modded())
        .map(|block| block.name.as_str())
        .collect();
    let good_item = registry.item_id("error-mod-good-item").is_some();
    let bad_block_present = registry.block_id("error-mod-bad-block").is_some();
    // The `error` fixture has two good item files (one with an unknown field
    // that only warns) and one block with an unknown `type` (rejected).
    let pass = good_item && !bad_block_present && !errors.is_empty() && delta == 2;
    if !pass {
        log::error!(
            "mods error-isolation failed: good_item={good_item} bad_block={bad_block_present} delta={delta} errors={errors:?}"
        );
    }
    let report = serde_json::json!({
        "fixture": fixture,
        "goodItem": good_item,
        "badBlockPresent": bad_block_present,
        "errors": errors,
        "moddedItems": modded_items,
        "moddedBlocks": modded_blocks,
        "registryDelta": delta,
        "modHasContentErrors": mods.has_content_errors(),
        "pass": pass,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "mods error-isolation: {} error(s), registry delta {delta}, goodItem={good_item}, badBlock={bad_block_present}: {}",
            report["errors"].as_array().map_or(0, Vec::len),
            if pass { "PASS" } else { "FAIL" }
        );
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Plan 20 M9 (`mods bench`): times one pipeline scene (§7d).
pub(super) fn cmd_mods_bench(
    scene: &str,
    dir: &Path,
    fixture: Option<&str>,
    repo: Option<&Path>,
    runs: usize,
    json: bool,
) -> anyhow::Result<i32> {
    if runs == 0 {
        return Err(anyhow!("mods bench --runs must be greater than 0"));
    }
    let mut samples = Vec::with_capacity(runs);
    let mut detail = serde_json::Map::new();

    match scene {
        "discover" => {
            use mind_core::io::SettingsStore;
            use mind_core::io::fs::NativeFs;
            use mind_core::mods::Mods;
            let dir = if dir.is_absolute() {
                dir.to_path_buf()
            } else {
                paths::find_repo_root(None)?.join(dir)
            };
            let fs = NativeFs;
            let mut settings = SettingsStore::new();
            let mut mod_count = 0;
            for _ in 0..runs {
                let mut mods = Mods::new(true, &dir);
                let start = Instant::now();
                let report = mods
                    .load(&fs, &dir, &mut settings)
                    .with_context(|| format!("loading mods from `{}`", dir.display()))?;
                samples.push(start.elapsed().as_nanos() as u64);
                mod_count = report.mods.len();
            }
            detail.insert("mods".into(), serde_json::json!(mod_count));
        }
        "parse" => {
            use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
            use mind_core::io::SettingsStore;
            use mind_core::io::fs::NativeFs;
            use mind_core::mods::{Mods, provider::ModsContentProvider};
            let root = paths::find_repo_root(repo)?;
            let fixture = fixture.ok_or_else(|| anyhow!("bench parse needs --fixture"))?;
            let dir = root.join("parity/mod_fixtures").join(fixture);
            let fs = NativeFs;
            let mut mods = Mods::new(true, &dir);
            mods.load_single(&fs, &dir, &SettingsStore::new())
                .with_context(|| format!("loading fixture `{fixture}`"))?;
            let files = mods.collect_content_files(&fs);
            let file_count = files.len();
            for _ in 0..runs {
                let bundle = MemoryBundle::new();
                let store = MemoryUnlockStore::new();
                let mut provider = ModsContentProvider::new(files.clone(), &store);
                let mut registry = create_base_content(&bundle, &store, true)
                    .map_err(|error| anyhow!("base content: {error}"))?;
                registry.init().map_err(|error| anyhow!("init: {error}"))?;
                let start = Instant::now();
                let parsed = registry.create_mod_content(&mut provider);
                samples.push(start.elapsed().as_nanos() as u64);
                if let Err(errors) = parsed {
                    return Err(anyhow!("mod content errors: {errors:?}"));
                }
            }
            detail.insert("files".into(), serde_json::json!(file_count));
        }
        "patch" => {
            use mind_core::content::parser_hooks::PatchAsset;
            use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
            use mind_core::io::FileSystem;
            use mind_core::io::fs::NativeFs;
            use mind_core::mods::Mods;
            use mind_core::mods::patch::DataPatcher;
            let root = paths::find_repo_root(repo)?;
            let fixture = fixture.ok_or_else(|| anyhow!("bench patch needs --fixture"))?;
            let dir = root.join("parity/mod_fixtures").join(fixture);
            let fs = NativeFs;
            let mut mods = Mods::new(true, &dir);
            mods.load_single(&fs, &dir, &mind_core::io::SettingsStore::new())
                .with_context(|| format!("loading fixture `{fixture}`"))?;
            let patch_dir = dir.join("patches");
            let mut patches = Vec::new();
            if let Ok(files) = fs.walk(&patch_dir) {
                let mut files: Vec<_> = files
                    .into_iter()
                    .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                    .collect();
                files.sort();
                for file in files {
                    let text = String::from_utf8(fs.read(&file)?)
                        .map_err(|_| anyhow!("`{}` is not UTF-8", file.display()))?;
                    patches.push(PatchAsset {
                        name: file.display().to_string(),
                        json: text,
                    });
                }
            }
            let patch_count = patches.len();
            for _ in 0..runs {
                let bundle = MemoryBundle::new();
                let store = MemoryUnlockStore::new();
                let mut registry = create_base_content(&bundle, &store, true)
                    .map_err(|error| anyhow!("base content: {error}"))?;
                registry.init().map_err(|error| anyhow!("init: {error}"))?;
                let start = Instant::now();
                let mut patcher = DataPatcher::new();
                patcher
                    .apply(&mut registry, &patches)
                    .map_err(|error| anyhow!("apply: {error}"))?;
                patcher.unapply(&mut registry);
                samples.push(start.elapsed().as_nanos() as u64);
            }
            detail.insert("patches".into(), serde_json::json!(patch_count));
        }
        "cache" => {
            use mind_core::io::MockFs;
            use mind_core::mods::assets::DataAssetCache;
            let fs = MockFs::new();
            let mut cache = DataAssetCache::load("/cache");
            let payload = vec![0xABu8; 1024 * 1024];
            for _ in 0..runs {
                let start = Instant::now();
                cache
                    .add(&fs, &payload)
                    .map_err(|e| anyhow!("cache add: {e}"))?;
                samples.push(start.elapsed().as_nanos() as u64);
            }
            detail.insert("bytes".into(), serde_json::json!(payload.len()));
        }
        "overlay" => {
            use mind_core::assets::atlas::AtlasIndex;
            use mind_core::assets::overlay::AssetOverlayProvider;
            use mind_core::io::SettingsStore;
            use mind_core::io::fs::NativeFs;
            use mind_core::mods::Mods;
            let root = paths::find_repo_root(repo)?;
            let fixture = fixture.ok_or_else(|| anyhow!("bench overlay needs --fixture"))?;
            let dir = root.join("parity/mod_fixtures").join(fixture);
            let fs = NativeFs;
            let mut mods = Mods::new(true, &dir);
            mods.load_single(&fs, &dir, &SettingsStore::new())
                .with_context(|| format!("loading fixture `{fixture}`"))?;
            let atlas = std::fs::read_to_string(root.join("assets/sprites/sprites.atlas.json"))
                .ok()
                .and_then(|text| AtlasIndex::from_manifest_json(&text).ok());
            let atlas_has = |name: &str| atlas.as_ref().is_some_and(|i| i.find(name).is_some());
            for _ in 0..runs {
                let start = Instant::now();
                let overlay = mods.build_overlay(&fs, &atlas_has);
                samples.push(start.elapsed().as_nanos() as u64);
                detail.insert("sprites".into(), serde_json::json!(overlay.sprites().len()));
            }
        }
        "save-patches" => {
            use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
            use mind_core::io::save::versions::v1::base_meta_tags;
            use mind_core::io::save::{SaveIo, SaveOptions, SaveReadState, WriteContext};
            use mind_core::mods::assets::{DataAsset, DataAssetType, DataAssets, ModDataManager};
            for _ in 0..runs {
                let bundle = MemoryBundle::new();
                let store = MemoryUnlockStore::new();
                let mut registry = create_base_content(&bundle, &store, true)
                    .map_err(|error| anyhow!("base content: {error}"))?;
                registry
                    .init()
                    .map_err(|error| anyhow!("content init: {error}"))?;
                let mut manager = ModDataManager::new();
                manager.push(DataAsset::patch(
                    "patches/a.json",
                    "{\"block.router.health\": 7}".to_owned(),
                ));
                manager.push(DataAsset::bundle("bundles/bundle.properties", "key=value"));
                manager.push(DataAsset::blob(
                    "sprites/external.png",
                    DataAssetType::Image,
                    vec![0xAB; 4096],
                    false,
                ));

                let start = Instant::now();
                let mut ctx = WriteContext::meta_only(base_meta_tags(0, 0, 0, "bench"));
                ctx.patches = Some(&manager);
                let bytes = SaveIo::write_to_vec(&ctx, &SaveOptions::new())
                    .map_err(|error| anyhow!("save write: {error}"))?;
                let mut state = SaveReadState::default();
                SaveIo::load_bytes(&bytes, &mut state)
                    .map_err(|error| anyhow!("save read: {error}"))?;
                let decoded = state.patches.clone().unwrap_or_default();
                let mut loaded = ModDataManager::new();
                loaded
                    .load(decoded, &mut registry)
                    .map_err(|error| anyhow!("asset load: {error}"))?;
                samples.push(start.elapsed().as_nanos() as u64);
            }
            detail.insert("bytes".into(), serde_json::json!(4096 + 64));
        }
        other => {
            return Err(anyhow!(
                "unknown mods bench scene `{other}` (discover/parse/patch/cache/overlay/save-patches)"
            ));
        }
    }

    samples.sort_unstable();
    let p50 = samples[samples.len() / 2];
    let p99 = samples[samples.len() - 1];
    let mut report = serde_json::json!({
        "scene": scene,
        "runs": runs,
        "p50_us": p50.div_ceil(1_000),
        "p99_us": p99.div_ceil(1_000),
    });
    if let Some(object) = report.as_object_mut() {
        for (key, value) in detail {
            object.insert(key, value);
        }
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "mods bench {scene}: p50 {}us, p99 {}us over {runs} runs",
            p50.div_ceil(1_000),
            p99.div_ceil(1_000)
        );
    }
    Ok(EXIT_PASS)
}

/// Plan 20 M5 (`mods overlay`): build a fixture mod's overlay and probe region
/// names against the resolved prefix/override/page rules.
pub(super) fn cmd_mods_overlay(
    fixture: &str,
    repo: Option<&Path>,
    probes: &[String],
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::assets::atlas::AtlasIndex;
    use mind_core::assets::overlay::AssetOverlayProvider;
    use mind_core::io::SettingsStore;
    use mind_core::io::fs::NativeFs;
    use mind_core::mods::Mods;

    let root = paths::find_repo_root(repo)?;
    let dir = root.join("parity/mod_fixtures").join(fixture);
    let fs = NativeFs;
    let mut mods = Mods::new(true, &dir);
    mods.load_single(&fs, &dir, &SettingsStore::new())
        .with_context(|| format!("loading fixture `{fixture}`"))?;

    let atlas_path = root.join("assets/sprites/sprites.atlas.json");
    let atlas = std::fs::read_to_string(&atlas_path)
        .ok()
        .and_then(|text| AtlasIndex::from_manifest_json(&text).ok());
    let atlas_has = |name: &str| {
        atlas
            .as_ref()
            .is_some_and(|index| index.find(name).is_some())
    };

    let overlay = mods.build_overlay(&fs, &atlas_has);
    let mut pass = true;
    let probe_reports: Vec<serde_json::Value> = probes
        .iter()
        .map(|name| match overlay.probe(name) {
            Some((path, page)) => serde_json::json!({
                "name": name,
                "found": true,
                "path": path,
                "page": page.name(),
            }),
            None => {
                pass = false;
                serde_json::json!({"name": name, "found": false})
            }
        })
        .collect();

    let report = serde_json::json!({
        "fixture": fixture,
        "sprites": overlay.sprites().len(),
        "bundles": overlay.bundles().len(),
        "pregenerated": overlay.pregenerated,
        "warnings": overlay.warnings,
        "probes": probe_reports,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "mods overlay: {} sprites, {} bundles, {} warning(s)",
            overlay.sprites().len(),
            overlay.bundles().len(),
            overlay.warnings.len()
        );
        for probe in &probe_reports {
            println!("{probe}");
        }
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// Normalizes a mod report to the stable `expected_list.json` fields.
pub(super) fn normalized_mod_list(report: &serde_json::Value) -> serde_json::Value {
    let mods = report
        .get("mods")
        .and_then(|value| value.as_array())
        .map(|mods| {
            mods.iter()
                .map(|entry| {
                    serde_json::json!({
                        "name": entry.get("name"),
                        "state": entry.get("state"),
                        "enabled": entry.get("enabled"),
                        "version": entry.get("version"),
                        "texturescale": entry.get("texturescale"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    serde_json::Value::Array(mods)
}

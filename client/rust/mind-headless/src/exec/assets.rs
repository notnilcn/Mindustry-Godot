// SPDX-License-Identifier: GPL-3.0-only

//! `assets` command bodies: atlas index/regions, bundle diff, sound registry, fallback boot and migration checks (plan 03 M1-M10).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use super::content::boot_content;
use super::*;

/// Plan 03 M1 `assets index`: manifest summary + region probes as JSON.
pub(super) fn cmd_assets_index(atlas: &Path, probes: &[String]) -> anyhow::Result<i32> {
    let manifest_path = atlas.join("sprites.atlas.json");
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let index = mind_core::assets::atlas::AtlasIndex::from_manifest_json(&text)
        .map_err(|error| anyhow!("{error}"))?;

    let mut pass = true;
    let probe_reports: Vec<serde_json::Value> = probes
        .iter()
        .map(|name| {
            let found = index.find(name);
            if found.is_none() {
                log::error!("assets index: region `{name}` not found");
                pass = false;
            }
            match found {
                Some(region) => serde_json::json!({
                    "name": name,
                    "found": true,
                    "page": region.page,
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                    "splits": region.splits,
                    "pads": region.pads,
                    "offsets": region.offsets,
                    "pageType": region.page_type.name(),
                }),
                None => serde_json::json!({"name": name, "found": false}),
            }
        })
        .collect();

    let report = serde_json::json!({
        "atlas": manifest_path.display().to_string(),
        "fallback": index.fallback,
        "inputsHash": index.inputs_hash,
        "pages": index.pages().iter().map(|page| serde_json::json!({
            "index": page.index,
            "type": page.type_.name(),
            "file": page.file,
            "width": page.width,
            "height": page.height,
        })).collect::<Vec<_>>(),
        "regions": index.len(),
        "probes": probe_reports,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(exit_code(pass))
}

/// Minimal shape of `build/assets/region_inventory.json` (plan 03 M3).
#[derive(Debug, serde::Deserialize)]
pub(super) struct RegionInventoryFile {
    #[serde(default)]
    regions: Vec<String>,
    #[serde(default, rename = "missingInSources")]
    missing_in_sources: Vec<String>,
}

/// Plan 03 §7.1b `assets regions`: every content-driven expected region must
/// resolve in the packed atlas. `--assert-complete` turns misses into a
/// non-zero exit; misses are always listed.
pub(super) fn cmd_assets_regions(
    atlas: &Path,
    inventory: Option<&Path>,
    assert_complete: bool,
    json: bool,
) -> anyhow::Result<i32> {
    let manifest_path = atlas.join("sprites.atlas.json");
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let index = mind_core::assets::atlas::AtlasIndex::from_manifest_json(&text)
        .map_err(|error| anyhow!("{error}"))?;

    let inventory_path = match inventory {
        Some(path) => path.to_path_buf(),
        None => paths::find_repo_root(None)?.join("build/assets/region_inventory.json"),
    };
    let inventory_text = std::fs::read_to_string(&inventory_path)
        .with_context(|| format!("reading {}", inventory_path.display()))?;
    let inventory: RegionInventoryFile = serde_json::from_str(&inventory_text)
        .with_context(|| format!("parsing {}", inventory_path.display()))?;

    // `block_colors` is a loose texture next to the pages, not an atlas region.
    let missing: Vec<&str> = inventory
        .regions
        .iter()
        .filter(|name| {
            if name.as_str() == "block_colors" {
                !atlas.join("block_colors.png").is_file()
            } else {
                index.find(name).is_none()
            }
        })
        .map(String::as_str)
        .collect();

    // M6 runtime half: resolve every `@Load` block field through the derive
    // macro and fold misses into a `RegionAudit`.
    //
    // Arc `find(name)` with no explicit `fallback` returns the `error` region
    // (`found() == false`) rather than failing, so several vanilla `@Load`
    // fields (e.g. `Conveyor.regions[5..7]`, `StaticWall.large` for walls with
    // no large sprite) are legitimately absent. The port keeps the strict
    // `fallback=error` audit *for regions the pack promises*: a default miss is
    // fatal only when the resolved name is in the generated inventory.
    let mut registry = boot_content()?;
    let audit = registry.load_regions(&index);
    let expected: std::collections::BTreeSet<&str> =
        inventory.regions.iter().map(String::as_str).collect();
    let runtime_errors: Vec<&str> = audit
        .errors
        .iter()
        .map(String::as_str)
        .filter(|name| expected.contains(name))
        .collect();
    let upstream_not_found = audit.errors.len() - runtime_errors.len();

    if json {
        let report = serde_json::json!({
            "atlas": manifest_path.display().to_string(),
            "inventory": inventory_path.display().to_string(),
            "expected": inventory.regions.len(),
            "resolved": inventory.regions.len() - missing.len(),
            "missing": missing,
            "missingInSources": inventory.missing_in_sources,
            "loadRegions": {
                "fatal": runtime_errors,
                "explicitFallbacks": audit.explicit_fallbacks,
                "upstreamNotFound": upstream_not_found,
            },
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "assets regions: {}/{} expected regions resolve ({} missing)",
            inventory.regions.len() - missing.len(),
            inventory.regions.len(),
            missing.len()
        );
        println!(
            "assets regions: @Load audit {} fallback=error miss(es), {} explicit fallback(s), {} upstream not-found",
            runtime_errors.len(),
            audit.explicit_fallbacks.len(),
            upstream_not_found
        );
        for name in &missing {
            log::error!("assets regions: missing region `{name}`");
        }
        for name in &runtime_errors {
            log::error!("assets regions: @Load fallback=error miss `{name}`");
        }
    }

    let inventory_ok =
        inventory.missing_in_sources.is_empty() && (missing.is_empty() || !assert_complete);
    if inventory_ok && runtime_errors.is_empty() {
        Ok(EXIT_PASS)
    } else {
        if !inventory.missing_in_sources.is_empty() {
            log::error!(
                "assets regions: {} expected regions were missing in the source atlas",
                inventory.missing_in_sources.len()
            );
        }
        Ok(EXIT_FAIL)
    }
}

/// Plan 03 M7 `assets bundle-diff`: no locale may contain keys absent from
/// English; reports per-locale missing keys and confirms the `global.properties`
/// overlay. Locales are allowed to be partial (report only).
pub(super) fn cmd_assets_bundle_diff(dir: &Path, json: bool) -> anyhow::Result<i32> {
    use mind_core::assets::bundle::parse_properties;

    let base_path = dir.join("bundle.properties");
    let base_text = std::fs::read_to_string(&base_path)
        .with_context(|| format!("reading {}", base_path.display()))?;
    let base = parse_properties(&base_text);
    let base_keys: std::collections::BTreeSet<String> = base.keys().cloned().collect();

    let mut locales: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .flatten()
    {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(locale) = stem.strip_prefix("bundle_") else {
            continue;
        };
        if path.extension().is_some_and(|ext| ext == "properties") {
            locales.push((locale.to_owned(), path));
        }
    }
    locales.sort_by(|a, b| a.0.cmp(&b.0));

    let mut errors = 0usize;
    let mut reports = Vec::new();
    for (locale, path) in &locales {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let properties = parse_properties(&text);
        let extra: Vec<&str> = properties
            .keys()
            .filter(|key| !base_keys.contains(key.as_str()))
            .map(String::as_str)
            .collect();
        let missing: Vec<&str> = base_keys
            .iter()
            .filter(|key| !properties.contains_key(key.as_str()))
            .map(String::as_str)
            .collect();
        if !extra.is_empty() {
            errors += 1;
            log::error!(
                "assets bundle-diff: locale `{locale}` has {} key(s) absent from English",
                extra.len()
            );
        }
        reports.push(serde_json::json!({
            "locale": locale,
            "keys": properties.len(),
            "extra": extra,
            "missing": missing,
        }));
    }

    let global_path = dir.join("global.properties");
    let global_keys = if global_path.is_file() {
        let text = std::fs::read_to_string(&global_path)
            .with_context(|| format!("reading {}", global_path.display()))?;
        parse_properties(&text).len()
    } else {
        0
    };

    if json {
        let report = serde_json::json!({
            "dir": dir.display().to_string(),
            "englishKeys": base_keys.len(),
            "globalKeys": global_keys,
            "locales": reports,
            "errors": errors,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "assets bundle-diff: {} english keys, {} locales, {} global keys, {} error locale(s)",
            base_keys.len(),
            locales.len(),
            global_keys,
            errors
        );
    }
    Ok(exit_code(errors == 0))
}

/// Plan 03 M8/M10 `assets sounds-check`: the registry equals the recursive
/// sound file listing, ids are dense/append-only and `none`/`unset` are present.
pub(super) fn cmd_assets_sounds_check(root: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    use mind_core::assets::sounds::Sounds;

    let root = paths::find_repo_root(root)?;
    let index_path = root.join("assets/sounds.index.json");
    let text = std::fs::read_to_string(&index_path)
        .with_context(|| format!("reading {}", index_path.display()))?;
    let sounds = Sounds::from_index_json(&text).map_err(|error| anyhow!("{error}"))?;

    let mut files = Vec::new();
    collect_audio(&root.join("assets/sounds"), &mut files)?;
    let file_count = files.len();

    let mut missing_files = Vec::new();
    let mut ids = Vec::new();
    for entry in sounds.entries().iter().filter(|entry| !entry.is_dummy()) {
        ids.push(entry.id);
        let Some(file) = &entry.file else {
            missing_files.push(entry.name.clone());
            continue;
        };
        if !root.join("assets").join(file).is_file() {
            missing_files.push(entry.name.clone());
        }
    }
    ids.sort_unstable();
    let dense = ids.iter().enumerate().all(|(i, id)| *id == i as i32);
    let count_matches = ids.len() == file_count;
    let has_none = sounds.get("none").is_some_and(|entry| entry.is_dummy());
    let has_unset = sounds.get("unset").is_some_and(|entry| entry.is_dummy());

    let pass = dense && count_matches && missing_files.is_empty() && has_none && has_unset;
    if json {
        let report = serde_json::json!({
            "index": index_path.display().to_string(),
            "entries": sounds.len(),
            "realSounds": ids.len(),
            "files": file_count,
            "denseIds": dense,
            "countMatches": count_matches,
            "noneDummy": has_none,
            "unsetDummy": has_unset,
            "missingFiles": missing_files,
            "pass": pass,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "assets sounds-check: {} entries, {} files, dense={dense}, countMatch={count_matches}, missing={} -> {}",
            sounds.len(),
            file_count,
            missing_files.len(),
            if pass { "PASS" } else { "FAIL" }
        );
        for name in &missing_files {
            log::error!("assets sounds-check: missing file for sound `{name}`");
        }
    }
    Ok(exit_code(pass))
}

/// Recursively collects `*.ogg`/`*.mp3` paths under `dir` (sorted).
pub(super) fn collect_audio(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> anyhow::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_audio(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext == "ogg" || ext == "mp3")
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Plan 03 M4/M10 `assets fallback-boot`: the 2048 fallback atlas parses and
/// every page & region fits within 2048.
pub(super) fn cmd_assets_fallback_boot(atlas: &Path, json: bool) -> anyhow::Result<i32> {
    let manifest_path = atlas.join("sprites.atlas.json");
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let index = mind_core::assets::atlas::AtlasIndex::from_manifest_json(&text)
        .map_err(|error| anyhow!("{error}"))?;

    let oversized_pages: Vec<&str> = index
        .pages()
        .iter()
        .filter(|page| page.width > 2048 || page.height > 2048)
        .map(|page| page.file.as_str())
        .collect();
    let oversized_regions: Vec<&str> = index
        .regions()
        .iter()
        .filter(|region| region.w > 2048 || region.h > 2048)
        .map(|region| region.name.as_str())
        .collect();
    let pass = index.fallback
        && !index.pages().is_empty()
        && oversized_pages.is_empty()
        && oversized_regions.is_empty();

    if json {
        let report = serde_json::json!({
            "atlas": manifest_path.display().to_string(),
            "fallback": index.fallback,
            "pages": index.pages().len(),
            "regions": index.len(),
            "oversizedPages": oversized_pages,
            "oversizedRegions": oversized_regions,
            "pass": pass,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "assets fallback-boot: fallback={} pages={} regions={} -> {}",
            index.fallback,
            index.pages().len(),
            index.len(),
            if pass { "PASS" } else { "FAIL" }
        );
        for name in &oversized_pages {
            log::error!("assets fallback-boot: oversized page `{name}`");
        }
        for name in &oversized_regions {
            log::error!("assets fallback-boot: oversized region `{name}`");
        }
    }
    Ok(exit_code(pass))
}

/// Plan 03 §7.1b `assets migrate-check`: the vendored trees match
/// `build/assets/migration_manifest.json` and forbidden generated files are absent.
pub(super) fn cmd_assets_migrate_check(
    repo: Option<&Path>,
    manifest: Option<&Path>,
) -> anyhow::Result<i32> {
    let root = paths::find_repo_root(repo)?;
    let manifest_path = manifest
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("build/assets/migration_manifest.json"));
    let problems = mind_atlas::migrate::MigrationManifest::read(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?
        .verify(&root)?;
    if problems.is_empty() {
        println!("assets migrate-check: OK (repo {})", root.display());
        Ok(EXIT_PASS)
    } else {
        for problem in &problems {
            log::error!("assets migrate-check: {problem}");
        }
        Ok(EXIT_FAIL)
    }
}

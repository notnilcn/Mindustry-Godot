// SPDX-License-Identifier: GPL-3.0-only

//! `content` command bodies: registry load/audit/dump/ids/seed and the content benchmark (plan 02).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use super::*;

/// Boots base content (create + init + postInit + load), the `content` harness
/// path. Assets/bundle are in-memory; headless skips icon/region loading.
pub(super) fn boot_content() -> anyhow::Result<ContentRegistry> {
    boot_content_with(None)
}

/// Boots base content with an optional bundle key map (localized names).
pub(super) fn boot_content_with(
    keys: Option<&BTreeMap<String, String>>,
) -> anyhow::Result<ContentRegistry> {
    let bundle = match keys {
        Some(keys) => {
            MemoryBundle::with_pairs(keys.iter().map(|(key, value)| (key.clone(), value.clone())))
        }
        None => MemoryBundle::new(),
    };
    let store = MemoryUnlockStore::new();
    let mut registry = create_base_content(&bundle, &store, true)?;
    registry.init()?;
    registry.post_init()?;
    registry.load()?;
    registry.log_content()?;
    Ok(registry)
}

/// Loads a `parity/bundle_keys.json` file, if the path exists.
pub(super) fn load_bundle_keys(path: Option<&Path>) -> anyhow::Result<Option<BundleKeysFile>> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading `{}`", path.display()))?;
    let file: BundleKeysFile = serde_json::from_str(&text)
        .with_context(|| format!("parsing bundle keys `{}`", path.display()))?;
    Ok(Some(file))
}

/// Loads an `asset_manifest.json` file, if the path exists.
pub(super) fn load_manifest(path: Option<&Path>) -> anyhow::Result<Option<AssetManifest>> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading `{}`", path.display()))?;
    let manifest: AssetManifest = serde_json::from_str(&text)
        .with_context(|| format!("parsing manifest `{}`", path.display()))?;
    Ok(Some(manifest))
}

pub(super) fn type_counts(registry: &ContentRegistry) -> Vec<ContentTypeCount> {
    let counts = content_counts(registry);
    registry::LIVE_CONTENT_TYPES
        .iter()
        .map(|type_| ContentTypeCount {
            type_: *type_,
            count: counts.get(type_).copied().unwrap_or(0),
        })
        .collect()
}

/// Plan 02 §7b negative scenario: `Liquids` before `StatusEffects` must fail
/// with the missing-status error naming the load-order violation.
pub(super) fn cmd_content_load_order_bad() -> anyhow::Result<i32> {
    match mind_core::content::registries::create_base_content_bad_order(
        &MemoryBundle::new(),
        &MemoryUnlockStore::new(),
        true,
    ) {
        Err(error) => {
            println!("content load-order-bad: failed as expected: {error}");
            Ok(EXIT_PASS)
        }
        Ok(_) => {
            log::error!("content load-order-bad: content loaded, expected a load-order error");
            Ok(EXIT_FAIL)
        }
    }
}

/// `content dump`: writes the parity golden snapshot (plan 02 §6.2/§7b).
pub(super) fn cmd_content_dump(
    out: Option<&Path>,
    bundle: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    let bundle_file = load_bundle_keys(bundle)?;
    let registry = boot_content_with(bundle_file.as_ref().map(|file| &file.keys))?;
    let golden = dump_golden(&registry);
    let text = format!("{}\n", serde_json::to_string_pretty(&golden)?);
    if let Some(path) = out {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || out.is_none() {
        print!("{text}");
    } else if let Some(path) = out {
        log::info!("content dump written to {}", path.display());
    }
    Ok(EXIT_PASS)
}

/// `content audit`: mechanical parity gate (plan 02 §7b/§7e).
pub(super) fn cmd_content_audit(
    golden: Option<&Path>,
    bundle: Option<&Path>,
    manifest: Option<&Path>,
    out: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    let golden_path = golden.unwrap_or_else(|| Path::new("parity/golden_content.json"));
    let text = std::fs::read_to_string(golden_path).with_context(|| {
        format!(
            "reading golden `{}` (run `content dump --out` to create it)",
            golden_path.display()
        )
    })?;
    let golden: GoldenContent = serde_json::from_str(&text)
        .with_context(|| format!("parsing golden `{}`", golden_path.display()))?;

    let bundle_path = bundle.unwrap_or_else(|| Path::new("parity/bundle_keys.json"));
    let manifest_path = manifest.unwrap_or_else(|| Path::new("parity/asset_manifest.json"));
    let bundle_file = load_bundle_keys(Some(bundle_path))?;
    let manifest = load_manifest(Some(manifest_path))?;

    let registry = boot_content_with(bundle_file.as_ref().map(|file| &file.keys))?;
    let report = audit(&registry, &golden, bundle_file.as_ref(), manifest.as_ref());

    if let Some(path) = out {
        let mut markdown = String::new();
        markdown.push_str("# Content audit report\n\n");
        markdown.push_str(&format!("- Golden: `{}`\n", golden_path.display()));
        markdown.push_str(&format!(
            "- Status: **{}**\n\n",
            if report.pass() { "PASS" } else { "FAIL" }
        ));
        markdown.push_str("| check | status | detail |\n|---|---|---|\n");
        for check in &report.checks {
            markdown.push_str(&format!(
                "| {} | {} | {} |\n",
                check.name, check.status, check.detail
            ));
        }
        if !report.errors.is_empty() {
            markdown.push_str("\n## Errors\n\n");
            for error in &report.errors {
                markdown.push_str(&format!("- {error}\n"));
            }
        }
        if !report.warnings.is_empty() {
            markdown.push_str("\n## Warnings\n\n");
            for warning in &report.warnings {
                markdown.push_str(&format!("- {warning}\n"));
            }
        }
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, markdown).with_context(|| format!("writing `{}`", path.display()))?;
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for check in &report.checks {
            println!("{:>16}: {} ({})", check.name, check.status, check.detail);
        }
        println!(
            "content audit: {} errors, {} warnings",
            report.errors.len(),
            report.warnings.len()
        );
        for error in report.errors.iter().take(10) {
            println!("  error: {error}");
        }
    }
    Ok(exit_code(report.pass()))
}

pub(super) fn cmd_content_load(json: bool) -> anyhow::Result<i32> {
    let registry = boot_content()?;
    let types = type_counts(&registry);
    let total: usize = types.iter().map(|entry| entry.count).sum();
    if json {
        let report = ContentLoadReport { types, total };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for entry in &types {
            println!("{:>12}: {}", entry.type_.name(), entry.count);
        }
        println!("       total: {total}");
    }
    Ok(EXIT_PASS)
}

pub(super) fn cmd_content_ids(json: bool, out: Option<&Path>) -> anyhow::Result<i32> {
    let registry = boot_content()?;
    let types: Vec<ContentTypeEntries> = registry::LIVE_CONTENT_TYPES
        .iter()
        .map(|type_| ContentTypeEntries {
            type_: *type_,
            entries: registry
                .entries(*type_)
                .into_iter()
                .map(|entry| ContentIdEntry {
                    id: entry.id,
                    name: entry.name.map(str::to_owned),
                    kind: entry.kind.to_owned(),
                })
                .collect(),
        })
        .collect();
    let report = ContentIdsReport { format: 1, types };
    let text = format!("{}\n", serde_json::to_string_pretty(&report)?);
    if let Some(path) = out {
        // Schema round-trip before writing (same rule as state dumps).
        let _decoded: serde_json::Value = serde_json::from_str(&text).with_context(|| {
            format!(
                "content ids failed schema round-trip for `{}`",
                path.display()
            )
        })?;
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json {
        print!("{text}");
    } else {
        for entry in &report.types {
            println!("{:>12}: {}", entry.type_.name(), entry.entries.len());
        }
        if out.is_none() {
            log::info!("pass --out <path> to write the deterministic content_ids.json dump");
        }
    }
    Ok(EXIT_PASS)
}

/// Plan-21 §3.12.4: generate the server module's `content_seed.rs`.
pub(super) fn cmd_content_seed(out: &Path) -> anyhow::Result<i32> {
    let registry = boot_content()?;
    let mut names: BTreeMap<String, u8> = BTreeMap::new();
    for (index, type_) in registry::LIVE_CONTENT_TYPES.iter().enumerate() {
        for entry in registry.entries(*type_) {
            if let Some(name) = entry.name {
                names.entry(name.to_owned()).or_insert(index as u8);
            }
        }
    }
    let mut text = String::new();
    text.push_str("// SPDX-License-Identifier: GPL-3.0-only\n\n");
    text.push_str("//! GENERATED by `mind-headless content seed` (plan 21 §3.12.4).\n");
    text.push_str("//! Regenerate via `server/gen_content_seed.sh`; never hand-edit.\n\n");
    text.push_str("/// Vanilla content as `(content_type, name)` pairs (append-only ABI).\n");
    text.push_str("pub const VANILLA_CONTENT: &[(u8, &str)] = &[\n");
    for (name, content_type) in &names {
        text.push_str(&format!("    ({content_type}, {name:?}),\n"));
    }
    text.push_str("];\n");
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out, &text).with_context(|| format!("writing `{}`", out.display()))?;
    log::info!("wrote {} content entries to {}", names.len(), out.display());
    Ok(EXIT_PASS)
}

pub(super) fn cmd_content_bench(runs: usize, json: bool) -> anyhow::Result<i32> {
    if runs == 0 {
        return Err(anyhow!("--runs must be greater than zero"));
    }
    let mut samples_ns: Vec<u64> = Vec::with_capacity(runs);
    let mut last: Option<ContentRegistry> = None;
    for _ in 0..runs {
        let start = Instant::now();
        let registry = boot_content()?;
        samples_ns.push(start.elapsed().as_nanos() as u64);
        last = Some(registry);
    }
    let types = last.as_ref().map(type_counts).unwrap_or_default();
    samples_ns.sort_unstable();
    let pick = |index: usize| samples_ns.get(index).copied().unwrap_or(0);
    let median_ns = {
        let index = samples_ns.len() / 2;
        pick(index)
    };
    let min_ns = pick(0);
    let max_ns = pick(samples_ns.len().saturating_sub(1));
    let to_ms = |ns: u64| ns as f64 / 1_000_000.0;
    let median_ms = to_ms(median_ns);
    let within_budget = median_ms <= 200.0;
    if !within_budget {
        log::warn!(
            "content load median {median_ms:.1}ms exceeds the 200ms budget (release budget; debug builds are slower)"
        );
    }
    let report = ContentBenchReport {
        runs,
        median_ms,
        min_ms: to_ms(min_ns),
        max_ms: to_ms(max_ns),
        within_budget,
        types,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "content bench: runs={} median={:.2}ms min={:.2}ms max={:.2}ms within_budget={}",
            report.runs, report.median_ms, report.min_ms, report.max_ms, report.within_budget
        );
    }
    Ok(EXIT_PASS)
}

// SPDX-License-Identifier: GPL-3.0-only

//! `meta`/`trace` command bodies: entity/component metadata and the deterministic schedule trace (plan 05 M5/M6).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use super::*;

/// Plan 05 M5: stable entity/component metadata JSON (`entitymeta.json`).
pub(super) fn cmd_meta_entities(out: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let registry = mind_core::entities::vanilla_registry()
        .map_err(|error| anyhow!("building entity registry: {error}"))?;
    let value = registry.metadata_json();
    let text = format!("{}\n", serde_json::to_string_pretty(&value)?);
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
        log::info!(
            "meta entities: {} defs / {} components written to {}",
            registry.defs_len(),
            registry.components_len(),
            path.display()
        );
    }
    Ok(EXIT_PASS)
}

/// Plan 05 M6: deterministic schedule-order trace (golden `trace order`).
pub(super) fn cmd_trace_order(ticks: u64, out: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    use mind_core::sim::schedule::{RunContext, render_trace, schedule_trace};

    let contexts = [
        ("playing", RunContext::playing_headless()),
        ("menu", RunContext::menu()),
        ("paused", RunContext::paused()),
        ("editor", RunContext::editor()),
        ("client", RunContext::client()),
    ];
    let text = render_trace(&contexts);
    if let Some(path) = out {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json {
        let rows: Vec<serde_json::Value> = contexts
            .iter()
            .map(|(label, ctx)| {
                let entries: Vec<serde_json::Value> = schedule_trace(*ctx)
                    .into_iter()
                    .map(|entry| serde_json::json!({"set": entry.set, "ran": entry.ran}))
                    .collect();
                serde_json::json!({"context": label, "sets": entries})
            })
            .collect();
        let report = serde_json::json!({
            "ticks": ticks,
            "tickSets": mind_core::sim::schedule::TICK_SETS.len(),
            "entitySets": mind_core::sim::schedule::ENTITY_SETS.len(),
            "contexts": rows,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if out.is_none() {
        print!("{text}");
    }
    Ok(EXIT_PASS)
}

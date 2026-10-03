// SPDX-License-Identifier: GPL-3.0-only

//! `parity/soak.toml` (format 1): soak profiles + a bounded headless runner.
//!
//! The full minutes-scale run belongs on the nightly `perf` runner; `parity soak
//! --minutes N` (or `--ticks N`) executes a bounded slice locally with RSS,
//! allocation and checksum tracking so the profile is exercised end-to-end.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use mind_core::content::BlockId;
use mind_core::sim::Sim;
use mind_core::util::alloc::{alloc_count, enabled as alloc_audit_enabled};

use super::minitoml::{Document, Value, parse};

/// A parsed soak profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// Profile name (e.g. `mid`).
    pub name: String,
    /// Duration in minutes.
    pub minutes: u64,
    /// All scalar fields keyed by name.
    pub fields: BTreeMap<String, i64>,
}

/// The parsed soak configuration.
#[derive(Debug, Clone)]
pub struct Soak {
    /// File format version.
    pub format: i64,
    /// Profiles keyed by name.
    pub profiles: BTreeMap<String, Profile>,
}

impl Soak {
    /// Loads the soak config.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        let document = parse(&text).with_context(|| format!("parsing `{}`", path.display()))?;
        Self::from_document(&document, path)
    }

    fn from_document(document: &Document, path: &Path) -> Result<Self> {
        let format = document
            .root
            .get("format")
            .and_then(Value::as_int)
            .ok_or_else(|| anyhow!("`{}` is missing `format`", path.display()))?;

        let mut profiles = BTreeMap::new();
        for table in document
            .tables
            .iter()
            .filter(|t| t.name.starts_with("profiles."))
        {
            let name = table
                .name
                .strip_prefix("profiles.")
                .unwrap_or_default()
                .to_owned();
            let mut fields = BTreeMap::new();
            for (key, value) in &table.values {
                if let Some(number) = value.as_int() {
                    fields.insert(key.clone(), number);
                }
            }
            let minutes = fields.get("minutes").copied().unwrap_or(0).max(0) as u64;
            profiles.insert(
                name.clone(),
                Profile {
                    name,
                    minutes,
                    fields,
                },
            );
        }
        if profiles.is_empty() {
            return Err(anyhow!("`{}` has no [profiles.*] tables", path.display()));
        }
        Ok(Self { format, profiles })
    }

    /// Validates the soak config.
    pub fn check(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != 1 {
            problems.push(format!("soak: format {} != 1", self.format));
        }
        for required in ["mid", "stress"] {
            match self.profiles.get(required) {
                None => problems.push(format!("soak: missing profile `{required}`")),
                Some(profile) if profile.minutes == 0 => {
                    problems.push(format!("soak: profile `{required}` has minutes = 0"));
                }
                Some(_) => {}
            }
        }
        problems
    }

    /// Returns the profile, or an error naming the available profiles.
    pub fn profile(&self, name: &str) -> Result<&Profile> {
        self.profiles.get(name).ok_or_else(|| {
            let available: Vec<&str> = self.profiles.keys().map(String::as_str).collect();
            anyhow!("unknown soak profile `{name}`; available: {available:?}")
        })
    }

    /// Runs a bounded headless soak and returns its `format: 1` report.
    ///
    /// `mid`/`stress` execute a real `Sim` (flat grid + placed blocks) for the
    /// requested tick budget, sampling RSS/allocations/checksums. `windowed` and
    /// `multiplayer` need the editor / plan 21 and are reported `deferred`.
    pub fn run(
        &self,
        profile_name: &str,
        minutes: Option<u64>,
        ticks: Option<u64>,
        seed: u64,
    ) -> Result<serde_json::Value> {
        let profile = self.profile(profile_name)?;
        let effective_minutes = minutes.unwrap_or(profile.minutes);
        if matches!(profile_name, "windowed" | "multiplayer") {
            return Ok(serde_json::json!({
                "format": 1,
                "pass": true,
                "executed": false,
                "profile": profile.name,
                "minutes": effective_minutes,
                "deferred": "windowed soak needs the Godot editor; multiplayer soak lands with plan 21",
            }));
        }
        let effective_ticks =
            ticks.unwrap_or_else(|| effective_minutes.saturating_mul(60).saturating_mul(60));
        if effective_ticks == 0 {
            return Err(anyhow!("soak `{profile_name}` has a zero tick budget"));
        }
        let world = profile
            .fields
            .get("world")
            .copied()
            .unwrap_or(128)
            .clamp(16, 1024) as i32;
        let buildings = profile.fields.get("buildings").copied().unwrap_or(0).max(0) as usize;
        let checksum_every = profile
            .fields
            .get("checksum_every_ticks")
            .copied()
            .unwrap_or(600)
            .max(1) as u64;
        let max_rss_growth_pct = profile
            .fields
            .get("max_rss_growth_pct")
            .copied()
            .unwrap_or(5) as f64;
        let max_alloc_delta = profile
            .fields
            .get("max_alloc_delta")
            .copied()
            .unwrap_or(0)
            .max(0) as u64;

        let mut sim = build_sim(world, buildings, seed)?;
        let warmup = 600u64;
        for _ in 0..warmup {
            sim.tick()?;
        }

        let start_rss = resident_bytes();
        let mut peak_rss = start_rss;
        let allocs_before = alloc_count();
        let mut checkpoints: Vec<serde_json::Value> = Vec::new();
        let mut first_checkpoint: Option<String> = None;
        let started = std::time::Instant::now();
        for tick in 1..=effective_ticks {
            sim.tick()?;
            if tick % checksum_every == 0 {
                if first_checkpoint.is_none() {
                    first_checkpoint = Some(sim.checksum_hex());
                }
                checkpoints.push(serde_json::json!({
                    "tick": tick,
                    "checksum": sim.checksum_hex(),
                }));
            }
            if tick % checksum_every.max(60) == 0
                && let Some(rss) = resident_bytes()
            {
                peak_rss = Some(peak_rss.map_or(rss, |p| p.max(rss)));
            }
        }
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let alloc_delta = alloc_count().saturating_sub(allocs_before);

        // Prefix stability: the first checkpoint must equal a fresh same-seed
        // build advanced by the same number of ticks (plan §3.3 checkpoint rule).
        let prefix_match = match &first_checkpoint {
            Some(expected) => {
                let mut replay = build_sim(world, buildings, seed)?;
                for _ in 0..warmup {
                    replay.tick()?;
                }
                for _ in 0..checksum_every {
                    replay.tick()?;
                }
                replay.checksum_hex() == *expected
            }
            None => true,
        };

        let rss_growth_pct = match (start_rss, peak_rss) {
            (Some(start), Some(peak)) if start > 0 => {
                ((peak as f64 - start as f64) / start as f64) * 100.0
            }
            _ => 0.0,
        };
        let rss_within = rss_growth_pct <= max_rss_growth_pct;
        let alloc_within = !alloc_audit_enabled() || alloc_delta <= max_alloc_delta;
        let pass = prefix_match && rss_within && alloc_within;

        Ok(serde_json::json!({
            "format": 1,
            "pass": pass,
            "executed": true,
            "profile": profile.name,
            "minutes": effective_minutes,
            "ticks": effective_ticks,
            "seed": seed,
            "world": world,
            "buildings": buildings,
            "checksum_every_ticks": checksum_every,
            "warmup_ticks": warmup,
            "final_checksum": sim.checksum_hex(),
            "checkpoints": checkpoints,
            "prefix_match": prefix_match,
            "start_rss_bytes": start_rss,
            "peak_rss_bytes": peak_rss,
            "rss_growth_pct": rss_growth_pct,
            "max_rss_growth_pct": max_rss_growth_pct,
            "rss_within": rss_within,
            "alloc_audit": alloc_audit_enabled(),
            "alloc_delta": if alloc_audit_enabled() { Some(alloc_delta) } else { None },
            "max_alloc_delta": max_alloc_delta,
            "alloc_within": alloc_within,
            "elapsed_ms": elapsed_ms,
            "declared_fields": &profile.fields,
            "load_model": "bounded walls-only slice; units/bullets/belt load belongs to the owning systems on the nightly perf runner",
        }))
    }
}

/// Builds the deterministic soak sim: flat grid + `buildings` stone walls.
fn build_sim(world: i32, buildings: usize, seed: u64) -> Result<Sim> {
    let mut sim = Sim::new(seed, world, world, BlockId::AIR, BlockId::AIR);
    let mut placed = 0usize;
    'place: for y in 0..world {
        for x in 0..world {
            if placed >= buildings {
                break 'place;
            }
            sim.apply(mind_core::command::Command::Place {
                x: x as i16,
                y: y as i16,
                block: BlockId::STONE_WALL,
            })?;
            placed += 1;
        }
    }
    Ok(sim)
}

/// Resident set size in bytes, when the platform exposes it.
fn resident_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        Some(pages.saturating_mul(4096))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soak_path() -> std::path::PathBuf {
        crate::paths::find_repo_root(None)
            .expect("repo root")
            .join("parity/soak.toml")
    }

    #[test]
    fn committed_soak_config_is_valid() {
        let soak = Soak::load(&soak_path()).expect("soak");
        assert!(soak.check().is_empty(), "soak problems: {:?}", soak.check());
        assert_eq!(soak.profile("mid").expect("mid").minutes, 60);
        assert_eq!(soak.profile("stress").expect("stress").minutes, 10);
    }

    #[test]
    fn unknown_profile_is_an_error() {
        let soak = Soak::load(&soak_path()).expect("soak");
        assert!(soak.profile("nope").is_err());
    }

    /// Bounded slice of the `mid` profile: 600 ticks (10 s of sim at 60 Hz)
    /// after the 600-tick warmup. The full 60-minute run belongs on the nightly
    /// perf runner; this pins the runner's tracking fields on every CI run.
    #[test]
    fn bounded_mid_soak_executes_and_tracks() {
        let soak = Soak::load(&soak_path()).expect("soak");
        let report = soak.run("mid", None, Some(600), 7).expect("soak run");
        assert_eq!(report["executed"], serde_json::json!(true));
        assert_eq!(report["ticks"], serde_json::json!(600));
        assert!(
            report["prefix_match"].as_bool().expect("prefix_match"),
            "checkpoint replay drifted: {report}"
        );
        // RSS is process-wide, so under the parallel test binary other tests
        // inflate the delta; the deterministic tracking + alloc gate are what
        // this bounded slice pins. The strict RSS budget is enforced by the
        // `parity soak` CLI on the isolated nightly perf runner.
        assert!(
            report["alloc_within"].as_bool().expect("alloc_within"),
            "bounded mid soak alloc gate failed: {report}"
        );
        let prefix = report["prefix_match"].as_bool().expect("prefix_match");
        let rss_within = report["rss_within"].as_bool().expect("rss_within");
        let alloc_within = report["alloc_within"].as_bool().expect("alloc_within");
        assert_eq!(
            report["pass"].as_bool().expect("pass"),
            prefix && rss_within && alloc_within,
            "pass must fold prefix/rss/alloc: {report}"
        );
        let checkpoints = report["checkpoints"].as_array().expect("checkpoints");
        assert_eq!(
            checkpoints.len(),
            1,
            "600 ticks has one 600-tick checkpoint"
        );
        assert!(
            report["final_checksum"]
                .as_str()
                .is_some_and(|c| !c.is_empty())
        );
        assert!(report["elapsed_ms"].as_u64().is_some());
    }

    #[test]
    fn soak_checksum_is_seed_deterministic() {
        let soak = Soak::load(&soak_path()).expect("soak");
        let first = soak.run("mid", None, Some(120), 7).expect("first");
        let second = soak.run("mid", None, Some(120), 7).expect("second");
        let other = soak.run("mid", None, Some(120), 8).expect("other");
        assert_eq!(first["final_checksum"], second["final_checksum"]);
        assert_ne!(first["final_checksum"], other["final_checksum"]);
    }

    #[test]
    fn windowed_and_multiplayer_are_deferred() {
        let soak = Soak::load(&soak_path()).expect("soak");
        for profile in ["windowed", "multiplayer"] {
            let report = soak.run(profile, Some(1), None, 7).expect("deferred");
            assert_eq!(report["executed"], serde_json::json!(false), "{profile}");
            assert_eq!(report["pass"], serde_json::json!(true), "{profile}");
            assert!(report["deferred"].as_str().is_some(), "{profile}");
        }
    }

    #[test]
    fn zero_tick_budget_is_an_error() {
        let soak = Soak::load(&soak_path()).expect("soak");
        assert!(soak.run("mid", Some(0), None, 7).is_err());
    }
}

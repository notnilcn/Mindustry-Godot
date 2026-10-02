// SPDX-License-Identifier: GPL-3.0-only

//! Scenario-directory discovery.

use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};

/// Marker file that identifies the canonical scenarios directory.
const MARKER: &str = "spine_place_break.json";

/// Finds the scenarios directory: explicit flag first, then `MIND_SCENARIOS_DIR`,
/// then walking up from the current directory until `scenarios/<MARKER>` exists.
pub fn find_scenarios_dir(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(dir) = explicit {
        if dir.join(MARKER).is_file() {
            return Ok(dir.to_path_buf());
        }
        return Err(anyhow!(
            "scenarios dir `{}` does not contain {MARKER}",
            dir.display()
        ));
    }

    if let Some(dir) = std::env::var_os("MIND_SCENARIOS_DIR") {
        let dir = PathBuf::from(dir);
        if dir.join(MARKER).is_file() {
            return Ok(dir);
        }
        return Err(anyhow!(
            "MIND_SCENARIOS_DIR=`{}` does not contain {MARKER}",
            dir.display()
        ));
    }

    let start = std::env::current_dir().context("resolving current directory")?;
    let mut current = start.as_path();
    loop {
        let candidate = current.join("scenarios");
        if candidate.join(MARKER).is_file() {
            return Ok(candidate);
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                return Err(anyhow!(
                    "could not find `scenarios/{MARKER}` above `{}`; pass --scenarios-dir",
                    start.display()
                ));
            }
        }
    }
}

/// Finds the repo root (the directory containing `assets/` + `assets-raw/`):
/// explicit flag first, then `$MIND_REPO`, then walking up from the current
/// directory until `assets-raw/sprites/pack.json` exists (plan 03 M0 marker).
pub fn find_repo_root(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    const ROOT_MARKER: &str = "assets-raw/sprites/pack.json";
    if let Some(dir) = explicit {
        if dir.join(ROOT_MARKER).is_file() {
            return Ok(dir.to_path_buf());
        }
        return Err(anyhow!(
            "repo root `{}` does not contain {ROOT_MARKER} (run `mind-tools migrate`)",
            dir.display()
        ));
    }

    if let Some(dir) = std::env::var_os("MIND_REPO") {
        let dir = PathBuf::from(dir);
        if dir.join(ROOT_MARKER).is_file() {
            return Ok(dir);
        }
        return Err(anyhow!(
            "MIND_REPO=`{}` does not contain {ROOT_MARKER} (run `mind-tools migrate`)",
            dir.display()
        ));
    }

    let start = std::env::current_dir().context("resolving current directory")?;
    let mut current = start.as_path();
    loop {
        if current.join(ROOT_MARKER).is_file() {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                return Err(anyhow!(
                    "could not find `{ROOT_MARKER}` above `{}`; pass --repo",
                    start.display()
                ));
            }
        }
    }
}

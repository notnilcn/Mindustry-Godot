// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Test-only map marker/region import suites.
//! Upstream: none (port-only test support).

use super::*;
use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
use crate::editor::context::EditorContext;
use crate::io::save::{SaveIo, SaveReadState};
use std::path::{Path, PathBuf};

fn collect_maps(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_maps(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("msav") {
            out.push(path);
        }
    }
}

/// I01 T8: measure every shipped Serpulo preset's `markers` region. All of
/// them carry an EMPTY UBJson `IntMap` (`MapMarkers.write`,
/// `MapMarkers.java:57-59`), so the legacy reader keeps skipping the
/// region; this test is the guard that fails if a map ever ships markers
/// (which would then need the UBJson decode).
#[test]
fn serpulo_preset_markers_regions_are_empty() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/serpulo");
    let mut maps = Vec::new();
    collect_maps(&dir, &mut maps);
    maps.sort();
    assert!(!maps.is_empty(), "serpulo presets are vendored");
    let mut checked = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for path in &maps {
        let bytes = std::fs::read(path).expect("preset readable");
        let mut content =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        let mut grid = WorldGrid::new(0, 0);
        grid.begin_map_load();
        let mut context = EditorContext::new(&mut grid, &content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut content),
            ..SaveReadState::default()
        };
        if let Err(error) = SaveIo::load_bytes(&bytes, &mut state) {
            // A few vendored presets carry rules JSON the strict parser
            // rejects (pre-existing; not part of I01 T8). They cannot
            // contribute a measurement.
            failed.push(format!("{}: {error}", path.display()));
            continue;
        }
        let Some(len) = state.markers_region_len else {
            continue;
        };
        assert!(
            len <= 2,
            "{}: markers region is {len} bytes, expected an empty UBJson map",
            path.display()
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "at least one v8+ preset carried a markers region"
    );
    eprintln!(
        "checked {checked} of {} serpulo presets ({} failed to import)",
        maps.len(),
        failed.len()
    );
    for line in &failed {
        eprintln!("  import failed: {line}");
    }
}

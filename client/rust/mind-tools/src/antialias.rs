// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/build.gradle` `pack` AA step (`antialias(...)` + the skip predicate
// at line 85) and the `ui/icons` copy/delete pair.

//! Stage 4/5 (`move-ui-icons`, `antialias`, plan 03 §3.5).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use mind_atlas::pixmaps;
use mind_atlas::png_io;

/// `tools/build.gradle` AA skip predicate (line 85), ported **including** the
/// upstream dead branch: `file.toString().startsWith("icon-")` never matches
/// an absolute path, so the effective skips are `*.9.png` and names
/// containing `aaaa` only.
pub fn aa_skipped(rel_normalized: &str) -> bool {
    rel_normalized.contains(".9.png") || rel_normalized.contains("aaaa")
}

/// Runs the AA pass over every staged PNG (skip predicate above), fanning
/// reads out across threads; results are written back in place.
pub fn antialias(staging: &Path) -> Result<u64> {
    let mut files = Vec::new();
    collect_pngs(staging, staging, &mut files)?;
    files.sort();
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);
    let mut processed: Vec<Option<std::result::Result<Vec<u8>, String>>> = Vec::new();
    processed.resize_with(files.len(), || None);
    std::thread::scope(|scope| {
        let chunk = files.len().div_ceil(threads).max(1);
        let mut slots = processed.as_mut_slice();
        for shard in files.chunks(chunk) {
            let (head, tail) = slots.split_at_mut(shard.len().min(slots.len()));
            slots = tail;
            scope.spawn(move || {
                for (path, slot) in shard.iter().zip(head.iter_mut()) {
                    let result = std::fs::read(path)
                        .map_err(|error| format!("reading {}: {error}", path.display()))
                        .and_then(|bytes| {
                            png_io::read_png(&bytes)
                                .map_err(|error| format!("decoding {}: {error}", path.display()))
                        })
                        .and_then(|mut pixmap| {
                            pixmaps::antialias(&mut pixmap);
                            png_io::write_png(&pixmap)
                                .map_err(|error| format!("encoding {}: {error}", path.display()))
                        });
                    *slot = Some(result);
                }
            });
        }
    });
    let mut count = 0u64;
    for (path, slot) in files.iter().zip(processed) {
        let bytes = slot
            .with_context(|| format!("missing AA result for {}", path.display()))?
            .map_err(anyhow::Error::msg)?;
        fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))?;
        count += 1;
    }
    Ok(count)
}

/// `tools/build.gradle` `copy sprites_out/ui/icons -> sprites_out/ui/` +
/// delete (stage 4 `move-ui-icons`).
pub fn move_ui_icons(staging: &Path) -> Result<u64> {
    let icons = staging.join("ui/icons");
    if !icons.is_dir() {
        return Ok(0);
    }
    let target = staging.join("ui");
    let mut files = Vec::new();
    collect_pngs(&icons, &icons, &mut files)?;
    files.sort();
    let mut moved = 0u64;
    for file in files {
        let rel = file.strip_prefix(&icons)?;
        let dest = target.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&file, &dest)
            .with_context(|| format!("moving {} -> {}", file.display(), dest.display()))?;
        moved += 1;
    }
    fs::remove_dir_all(&icons).with_context(|| format!("deleting {}", icons.display()))?;
    Ok(moved)
}

fn collect_pngs(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_pngs(root, &path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "png") {
            let rel = path
                .strip_prefix(root)
                .with_context(|| format!("stripping {}", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            if !aa_skipped(&rel) {
                out.push(path);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_predicate_matches_gradle_line_85() {
        assert!(aa_skipped("ui/bar.9.png"));
        assert!(aa_skipped("effects/alphaaaa.png"));
        assert!(!aa_skipped("ui/icon-add.png"));
        assert!(!aa_skipped("blocks/walls/copper-wall.png"));
    }
}

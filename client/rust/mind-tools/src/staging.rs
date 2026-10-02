// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/build.gradle` `pack` staging copy + `ImagePacker.fixSubdirectory`.

//! Stage 1 (`staging`): mirror `assets-raw/sprites/**` into
//! `build/assets/staging/` and apply the upstream subdirectory flattens
//! (plan 03 §3.5).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Subdirectories flattened into their parent before generation
/// (`ImagePacker.fixSubdirectory`).
pub const FIX_SUBDIRECTORIES: &[&str] = &[
    "blocks/environment/character-overlay",
    "blocks/environment/rune-overlay",
];

/// Deletes the staging dir, copies the sprite sources in, applies the
/// `fixSubdirectory` moves. Returns the number of copied files.
pub fn stage(source: &Path, staging: &Path) -> Result<u64> {
    if !source.is_dir() {
        bail!("sprite source {} is not a directory", source.display());
    }
    if staging.exists() {
        fs::remove_dir_all(staging).with_context(|| format!("wiping {}", staging.display()))?;
    }
    fs::create_dir_all(staging).with_context(|| format!("creating {}", staging.display()))?;
    let mut files = Vec::new();
    collect_files(source, &mut files)?;
    files.sort();
    // Fan the copy out across threads (deterministic: the file list is sorted
    // and every target path is unique).
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);
    let copied = std::sync::atomic::AtomicU64::new(0);
    let first_error = std::sync::Mutex::new(None::<String>);
    let copied_ref = &copied;
    let error_ref = &first_error;
    std::thread::scope(|scope| {
        let chunk = files.len().div_ceil(threads).max(1);
        for shard in files.chunks(chunk) {
            scope.spawn(move || {
                for path in shard {
                    let Ok(rel) = path.strip_prefix(source) else {
                        continue;
                    };
                    let target = staging.join(rel);
                    let result = (|| -> std::io::Result<()> {
                        if let Some(parent) = target.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::copy(path, &target)?;
                        copied_ref.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        Ok(())
                    })();
                    if let Err(error) = result {
                        let mut guard = match error_ref.lock() {
                            Ok(guard) => guard,
                            Err(poison) => poison.into_inner(),
                        };
                        if guard.is_none() {
                            *guard = Some(format!("copying {}: {error}", path.display()));
                        }
                    }
                }
            });
        }
    });
    if let Some(error) = first_error
        .lock()
        .map_err(|e| anyhow::anyhow!("lock: {e}"))?
        .take()
    {
        bail!(error);
    }
    for sub in FIX_SUBDIRECTORIES {
        fix_subdirectory(staging, sub)?;
    }
    Ok(copied.load(std::sync::atomic::Ordering::Relaxed))
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// Moves every file in `<staging>/<sub>` up to its parent and deletes the
/// directory (`ImagePacker.fixSubdirectory`).
fn fix_subdirectory(staging: &Path, sub: &str) -> Result<()> {
    let folder = staging.join(sub);
    if !folder.is_dir() {
        return Ok(());
    }
    let parent = folder
        .parent()
        .with_context(|| format!("{} has no parent", folder.display()))?
        .to_path_buf();
    let mut stack = vec![folder.clone()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<PathBuf> = fs::read_dir(&dir)?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<std::io::Result<_>>()?;
        entries.sort();
        for path in entries {
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                let name = path
                    .file_name()
                    .with_context(|| format!("{} has no file name", path.display()))?;
                fs::rename(&path, parent.join(name)).with_context(|| {
                    format!("moving {} -> {}", path.display(), parent.display())
                })?;
            }
        }
    }
    fs::remove_dir_all(&folder).with_context(|| format!("deleting {}", folder.display()))?;
    Ok(())
}

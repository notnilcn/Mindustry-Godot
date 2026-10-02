// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/ImagePacker.java` (`GenRegion`/`PackIndex` fake
// atlas + `save`/`replace`/`delete` staging mutation semantics).

//! The pack-time fake atlas (plan 03 §3.5 stage 2 `enumerate`).
//!
//! Mirrors `ImagePacker`'s in-memory atlas over the staging tree: names are
//! staging-relative paths without extension (`blocks/walls/copper-wall`),
//! pixmaps decode lazily and are shared, and generator writes go through
//! [`PackAtlas::save`]/[`PackAtlas::replace`]/[`PackAtlas::delete`] so later
//! passes see a consistent view.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, bail};
use mind_atlas::pixmaps::Pixmap;
use mind_atlas::png_io;

/// Fake atlas over the staging tree (`ImagePacker.cache`).
pub struct PackAtlas {
    root: PathBuf,
    /// name -> (staging-relative file path, decoded-once pixels).
    entries: BTreeMap<String, PackIndex>,
    /// Lazily decoded pixels, shared with generators.
    pixels: HashMap<String, Rc<Pixmap>>,
}

struct PackIndex {
    file: PathBuf,
}

impl PackAtlas {
    /// Walks `staging` for `*.png` and builds the index (`ImagePacker.main`'s
    /// `sprites_out` walk). Keys are **flattened** file names without
    /// extension (`copper-wall`, not `blocks/walls/copper-wall`); duplicate
    /// base names across folders are a pack error (§3.4).
    pub fn enumerate(staging: &Path) -> Result<PackAtlas> {
        let mut files = Vec::new();
        collect_pngs(staging, &mut files)?;
        files.sort();
        let mut entries = BTreeMap::new();
        for file in files {
            let name = file
                .file_stem()
                .and_then(|stem| stem.to_str())
                .with_context(|| format!("bad png name {}", file.display()))?
                .to_owned();
            if entries.contains_key(&name) {
                bail!("duplicate region base name `{name}` (flattenPaths collision)");
            }
            entries.insert(name, PackIndex { file });
        }
        Ok(PackAtlas {
            root: staging.to_path_buf(),
            entries,
            pixels: HashMap::new(),
        })
    }

    /// `Core.atlas.has(name)` at enumerate time.
    pub fn has(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// All region names (enumerate-time + saved this run), sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// `ImagePacker.get(name)` — decoded pixels of a region. Errors match the
    /// upstream `Region does not exist` failure.
    pub fn get(&mut self, name: &str) -> Result<Rc<Pixmap>> {
        if let Some(pixmap) = self.pixels.get(name) {
            return Ok(pixmap.clone());
        }
        let index = self
            .entries
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Region does not exist: {name}"))?;
        let bytes =
            fs::read(&index.file).with_context(|| format!("reading {}", index.file.display()))?;
        let pixmap = png_io::read_png(&bytes)
            .with_context(|| format!("decoding {}", index.file.display()))?;
        let shared = Rc::new(pixmap);
        self.pixels.insert(name.to_owned(), shared.clone());
        Ok(shared)
    }

    /// `ImagePacker.save(pix, path)` — write `staging/<path>.png` and update
    /// the index (keyed by the flattened base name) so later `get` sees the
    /// new content.
    pub fn save(&mut self, pixmap: &Pixmap, path: &str) -> Result<()> {
        let file = self.root.join(format!("{path}.png"));
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file, png_io::write_png(pixmap)?)
            .with_context(|| format!("writing {}", file.display()))?;
        let name = file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .with_context(|| format!("bad png name {}", file.display()))?
            .to_owned();
        self.entries.insert(name.clone(), PackIndex { file });
        self.pixels.insert(name, Rc::new(pixmap.copy()));
        Ok(())
    }

    /// `ImagePacker.delete(name)` — delete a region's file and index entry.
    pub fn delete(&mut self, name: &str) -> Result<()> {
        if let Some(index) = self.entries.remove(name)
            && index.file.exists()
        {
            fs::remove_file(&index.file)
                .with_context(|| format!("deleting {}", index.file.display()))?;
        }
        self.pixels.remove(name);
        Ok(())
    }

    /// `ImagePacker.replace(path, name, image)` — write the new image at
    /// `path`, then delete the region's old file when it differs.
    pub fn replace(&mut self, path: &str, name: &str, image: &Pixmap) -> Result<()> {
        let old_file = self.entries.get(name).map(|index| index.file.clone());
        self.save(image, path)?;
        // The name now resolves to the new file (same path when path == name's
        // location); delete the old file when the region moved.
        if let Some(old) = old_file {
            let new_file = self
                .root
                .join(format!("{}.png", path.trim_end_matches(".png")));
            if old != new_file && old.exists() {
                fs::remove_file(&old).with_context(|| format!("deleting {}", old.display()))?;
            }
        }
        Ok(())
    }

    /// `ImagePacker.replace(region, image)` — rewrite the region at its own
    /// path.
    pub fn replace_in_place(&mut self, name: &str, image: &Pixmap) -> Result<()> {
        if !self.has(name) {
            bail!("Region does not exist: {name}");
        }
        self.replace(name, name, image)
    }
}

fn collect_pngs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_pngs(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "png") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_atlas::pixmaps::rgba8888;

    fn fixture() -> (PathBuf, PackAtlas) {
        let root = std::env::temp_dir().join(format!("pack-atlas-test-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let sprites = root.join("staging");
        fs::create_dir_all(sprites.join("blocks/walls")).unwrap();
        let mut wall = Pixmap::new(4, 4);
        wall.fill(rgba8888(1, 2, 3, 255));
        fs::write(
            sprites.join("blocks/walls/copper-wall.png"),
            png_io::write_png(&wall).unwrap(),
        )
        .unwrap();
        let atlas = PackAtlas::enumerate(&sprites).unwrap();
        (root, atlas)
    }

    #[test]
    fn enumerate_get_save_replace_delete() {
        let (root, mut atlas) = fixture();
        assert!(atlas.has("copper-wall"));
        assert!(!atlas.has("missing"));
        assert_eq!(atlas.get("copper-wall").unwrap().width, 4);
        assert!(atlas.get("missing").is_err());

        // save makes new content visible (keyed by flattened base name).
        let mut icon = Pixmap::new(2, 2);
        icon.fill(rgba8888(9, 9, 9, 255));
        atlas.save(&icon, "generated/block-copper-wall-ui").unwrap();
        assert!(atlas.has("block-copper-wall-ui"));
        assert_eq!(atlas.get("block-copper-wall-ui").unwrap().width, 2);

        // replace moves the region and deletes the old file.
        let staging = root.join("staging");
        atlas
            .replace("ui/block-copper-wall-ui", "block-copper-wall-ui", &icon)
            .unwrap();
        assert!(!staging.join("generated/block-copper-wall-ui.png").exists());
        assert!(staging.join("ui/block-copper-wall-ui.png").exists());

        // delete removes file + entry.
        atlas.delete("block-copper-wall-ui").unwrap();
        assert!(!atlas.has("block-copper-wall-ui"));

        fs::remove_dir_all(&root).unwrap();
    }
}

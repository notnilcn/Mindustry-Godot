// SPDX-License-Identifier: GPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! M1 vertical slice: one authored sprite flowing source → staging → pack →
//! manifest → runtime lookup, with byte-determinism across two runs
//! (plan 03 §5 M1 verify).

use std::fs;
use std::path::{Path, PathBuf};

use mind_atlas::manifest::sha256_hex;
use mind_atlas::pixmaps::{Pixmap, rgba8888};
use mind_atlas::png_io;
use mind_core::assets::atlas::AtlasIndex;

const ROOT_PACK_JSON: &str = r#"{
  duplicatePadding: true,
  combineSubdirectories: true,
  flattenPaths: true,
  maxWidth: 4096,
  maxHeight: 4096,
  fast: true,
  stripWhitespaceCenter: true,
  ignoredWhitespaceStrings: ["effects/"]
}"#;

fn write_png(path: &Path, pixmap: &Pixmap) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, png_io::write_png(pixmap).unwrap()).unwrap();
}

/// Builds the fixture source tree: `copper-wall` (32x32), `error` (3x3) and a
/// `bar.9.png` ninepatch.
fn make_fixture(root: &Path) -> PathBuf {
    let sprites = root.join("sprites");
    fs::create_dir_all(&sprites).unwrap();
    fs::write(sprites.join("pack.json"), ROOT_PACK_JSON).unwrap();

    let mut wall = Pixmap::new(32, 32);
    wall.fill(rgba8888(197, 121, 55, 255));
    for i in 0..32 {
        wall.set_raw(i, 0, rgba8888(255, 200, 120, 255));
    }
    write_png(&sprites.join("blocks/walls/copper-wall.png"), &wall);

    let mut error = Pixmap::new(3, 3);
    error.fill(rgba8888(255, 0, 255, 255));
    write_png(&sprites.join("effects/error.png"), &error);

    let mut bar = Pixmap::new(10, 10);
    for y in 1..9 {
        for x in 1..9 {
            bar.set_raw(x, y, rgba8888(90, 90, 90, 255));
        }
    }
    for x in 2..6 {
        bar.set_raw(x, 0, rgba8888(0, 0, 0, 255));
        bar.set_raw(0, x, rgba8888(0, 0, 0, 255));
    }
    write_png(&sprites.join("ui/bar.9.png"), &bar);

    // A nested pack.json dir (its own pass, environment page type).
    fs::create_dir_all(sprites.join("blocks/environment")).unwrap();
    fs::write(
        sprites.join("blocks/environment/pack.json"),
        "{ maxWidth: 2048, maxHeight: 2048, stripWhitespaceCenter: false }",
    )
    .unwrap();
    let mut dirt = Pixmap::new(32, 32);
    dirt.fill(rgba8888(80, 60, 40, 255));
    write_png(&sprites.join("blocks/environment/dirt.png"), &dirt);

    sprites
}

fn pack_fixture(name: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("mind-tools-slice-{name}-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    let sprites = make_fixture(&root);
    let staging = root.join("staging");
    mind_tools::staging::stage(&sprites, &staging).unwrap();
    let out = root.join("out");
    mind_tools::pack_pipeline::pack(&staging, &out, false, &std::collections::BTreeMap::new())
        .unwrap();
    (root, out)
}

fn hash_outputs(out: &Path) -> Vec<(String, String)> {
    let mut hashes: Vec<(String, String)> = Vec::new();
    let mut stack = vec![out.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(out)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                hashes.push((rel, sha256_hex(&fs::read(&path).unwrap())));
            }
        }
    }
    hashes.sort();
    hashes
}

#[test]
fn vertical_slice_pack_and_lookup() {
    let (root, out) = pack_fixture("slice");
    let manifest_text = fs::read_to_string(out.join("sprites.atlas.json")).unwrap();
    let index = AtlasIndex::from_manifest_json(&manifest_text).unwrap();

    // The two authored regions resolve with authored dimensions.
    let wall = index.find("copper-wall").expect("copper-wall region");
    assert_eq!((wall.w, wall.h), (32, 32));
    let error = index.find("error").expect("error region");
    assert!(error.w >= 1 && error.h >= 1);
    // Missing names do not silently fall back.
    assert!(index.find("this-region-does-not-exist").is_none());
    assert_eq!(index.find_or("missing", "error").unwrap().name, "error");
    // Ninepatch splits flow through.
    assert!(index.find("bar").unwrap().splits.is_some());
    // Nested-config dirs get their own pass and page type.
    let dirt = index.find("dirt").expect("dirt region");
    assert_eq!(
        dirt.page_type,
        mind_core::assets::atlas::PageType::Environment
    );
    assert_eq!(wall.page_type, mind_core::assets::atlas::PageType::Main);
    assert!(index.pages().len() >= 2);

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn pack_is_byte_deterministic() {
    let (root_a, out_a) = pack_fixture("det-a");
    let (root_b, out_b) = pack_fixture("det-b");
    let hashes_a = hash_outputs(&out_a);
    let hashes_b = hash_outputs(&out_b);
    assert_eq!(hashes_a, hashes_b);
    fs::remove_dir_all(&root_a).unwrap();
    fs::remove_dir_all(&root_b).unwrap();
}

/// A re-pack into a tree that already has the vendored loose art must keep it
/// (`sprites/space.png`, `sprites/planets/*`) while replacing the generated
/// atlas outputs (EV-0015).
#[test]
fn pack_preserves_loose_sprites() {
    let (root, out) = pack_fixture("loose");
    let staging = root.join("staging");
    let loose = [
        ("space.png", b"space".as_slice()),
        ("planets/serpulo.png", b"serpulo".as_slice()),
        ("error.png", b"error".as_slice()),
    ];
    for (name, bytes) in &loose {
        let path = out.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
    }

    let output =
        mind_tools::pack_pipeline::pack(&staging, &out, false, &std::collections::BTreeMap::new())
            .unwrap();

    for (name, bytes) in &loose {
        let path = out.join(name);
        assert!(path.is_file(), "{name} was deleted by pack");
        assert_eq!(fs::read(&path).unwrap(), *bytes, "{name} content changed");
    }
    // The regenerated manifest is still valid and complete.
    let manifest_text = fs::read_to_string(out.join("sprites.atlas.json")).unwrap();
    let index = AtlasIndex::from_manifest_json(&manifest_text).unwrap();
    assert!(index.find("copper-wall").is_some());
    assert!(index.find("error").is_some());
    assert!(output.regions >= 3);
    fs::remove_dir_all(&root).unwrap();
}

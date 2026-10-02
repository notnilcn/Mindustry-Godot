// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/build.gradle` `pack` stages 6/7 (TexturePacker.process + fallback
// rewrite) and Arc `TexturePackerFileProcessor` (pass discovery/inheritance).

//! Stage 6/7 (`pack`, `pack-fallback`): pass discovery over the staging tree,
//! page rendering and manifest writing (plan 03 §3.5, §6.3).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use mind_atlas::atlas_json::{AtlasManifest, page_file_name};
use mind_atlas::manifest::{FileHash, TreeManifest, combined_hash, hash_tree};
use mind_atlas::pack::{InputImage, PackResult, pack_images};
use mind_atlas::pack_json::PackSettings;
use mind_atlas::page::PageType;
use mind_atlas::png_io;

/// Output of one pack run.
#[derive(Debug)]
pub struct PackOutput {
    /// Written files relative to the output dir (sorted).
    pub files: Vec<FileHash>,
    /// Region count.
    pub regions: usize,
    /// Page count.
    pub pages: usize,
    /// `inputsHash` recorded in the manifest.
    pub inputs_hash: String,
}

/// One pack pass: a directory with resolved settings and image names
/// (staging-relative, `/`-separated, without `.png`).
struct Pass {
    /// Staging-relative dir (`""` = root).
    dir: String,
    settings: PackSettings,
    images: Vec<String>,
}

/// Runs the full pack over `staging` into `out_dir`. When `fallback`, page
/// caps above 2048 are rewritten to 2048 (the upstream in-place `pack.json`
/// rewrite) and the manifest is marked `fallback: true`. `extras` are
/// generator outputs that live outside the staging tree (e.g.
/// `block_colors`) and are written straight into `out_dir`.
pub fn pack(
    staging: &Path,
    out_dir: &Path,
    fallback: bool,
    extras: &BTreeMap<String, mind_atlas::pixmaps::Pixmap>,
) -> Result<PackOutput> {
    let passes = discover_passes(staging, fallback)?;
    let inputs_hash = staged_inputs_hash(staging)?;

    if out_dir.exists() {
        fs::remove_dir_all(out_dir).with_context(|| format!("wiping {}", out_dir.display()))?;
    }
    fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    for (name, pixmap) in extras {
        let file = out_dir.join(format!("{name}.png"));
        fs::write(&file, png_io::write_png(pixmap)?)
            .with_context(|| format!("writing {}", file.display()))?;
    }

    let mut page_types: Vec<PageType> = Vec::new();
    let mut pngs: Vec<Vec<u8>> = Vec::new();
    let mut page_dims: Vec<(i32, i32)> = Vec::new();
    let mut all_regions: Vec<mind_atlas::pack::PackedRegion> = Vec::new();

    for pass in &passes {
        let inputs = decode_inputs(staging, &pass.images)?;
        let page_type = PageType::for_path(&format!("{}/x", pass.dir));
        let result: PackResult = pack_images(&inputs, &pass.settings).with_context(|| {
            format!("packing pass `{}` ({} images)", pass.dir, pass.images.len())
        })?;
        let base_page = page_types.len();
        for page in &result.pages {
            let index = page_types.len();
            let png = png_io::write_png(&page.pixmap).context("encoding page")?;
            let file = out_dir.join(page_file_name(index));
            fs::write(&file, &png).with_context(|| format!("writing {}", file.display()))?;
            page_types.push(page_type);
            pngs.push(png);
            page_dims.push((page.width, page.height));
        }
        for mut region in result.regions {
            region.page += base_page;
            all_regions.push(region);
        }
    }

    all_regions.sort_by(|a, b| a.name.cmp(&b.name));

    let out_pages: Vec<mind_atlas::pack::PackedPage> = page_dims
        .iter()
        .enumerate()
        .map(|(index, &(width, height))| mind_atlas::pack::PackedPage {
            index,
            pixmap: mind_atlas::pixmaps::Pixmap::new(0, 0),
            width,
            height,
        })
        .collect();
    let page_cap = if fallback { 2048 } else { 4096 };
    let manifest = AtlasManifest::from_pack(
        &out_pages,
        &all_regions,
        &page_types,
        page_cap,
        fallback,
        inputs_hash.clone(),
        &pngs,
    );
    assert_no_duplicate_region_names(&manifest)?;
    manifest.write(&out_dir.join("sprites.atlas.json"))?;

    let files = hash_tree(out_dir, &|_, _| false)?;
    Ok(PackOutput {
        regions: manifest.regions.len(),
        pages: manifest.pages.len(),
        inputs_hash,
        files,
    })
}

/// Decodes a pass's images in index order, fanning reads out across threads
/// (deterministic: results are collected back in input order; §3.5 allows
/// parallel generation with ordered writes).
fn decode_inputs(staging: &Path, names: &[String]) -> Result<Vec<InputImage>> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);
    let mut decoded: Vec<Option<std::result::Result<mind_atlas::pixmaps::Pixmap, String>>> =
        Vec::new();
    decoded.resize_with(names.len(), || None);
    std::thread::scope(|scope| {
        let chunk = names.len().div_ceil(threads);
        let mut slices = decoded.as_mut_slice();
        for (thread, names_chunk) in names.chunks(chunk.max(1)).enumerate() {
            let (head, tail) = slices.split_at_mut(names_chunk.len().min(slices.len()));
            slices = tail;
            let _ = thread;
            scope.spawn(move || {
                for (name, slot) in names_chunk.iter().zip(head.iter_mut()) {
                    let path = staging.join(format!("{name}.png"));
                    *slot = Some(
                        std::fs::read(&path)
                            .map_err(|error| format!("reading {}: {error}", path.display()))
                            .and_then(|bytes| {
                                png_io::read_png(&bytes).map_err(|error| {
                                    format!("decoding {}: {error}", path.display())
                                })
                            }),
                    );
                }
            });
        }
    });
    let mut inputs = Vec::with_capacity(names.len());
    for (name, slot) in names.iter().zip(decoded) {
        let pixmap = slot
            .with_context(|| format!("missing decode result for {name}"))?
            .map_err(anyhow::Error::msg)?;
        inputs.push(InputImage {
            name: name.clone(),
            pixmap: std::rc::Rc::new(pixmap),
        });
    }
    Ok(inputs)
}

/// Writes `build/assets/asset_manifest.json` over a set of pack outputs
/// (stage 8 `manifest`; plan 03 §2.2/§7.1b `pack-check`).
pub fn write_asset_manifest(outputs: &BTreeMap<String, PackOutput>, path: &Path) -> Result<()> {
    let mut files = Vec::new();
    for (label, output) in outputs {
        for file in &output.files {
            files.push(FileHash {
                path: format!("{label}/{}", file.path),
                sha256: file.sha256.clone(),
                len: file.len,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = TreeManifest {
        format: 1,
        root: String::from("assets-pack"),
        upstream_commit: None,
        tree_hash: combined_hash(&files),
        files,
    };
    manifest
        .write(path)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// sha256 over the staged sources (`inputsHash`, plan 03 §6.3).
fn staged_inputs_hash(staging: &Path) -> Result<String> {
    let files = hash_tree(staging, &|_, _| false)?;
    Ok(combined_hash(&files))
}

/// Discovers pack passes (root first, then remaining dirs in sorted order)
/// with Arc `TexturePackerFileProcessor` semantics: settings inherit from the
/// closest parent config; a dir with `combineSubdirectories` consumes every
/// file below it except under nested-config dirs.
fn discover_passes(staging: &Path, fallback: bool) -> Result<Vec<Pass>> {
    if !staging.join("pack.json").is_file() {
        bail!("missing root pack.json in {}", staging.display());
    }
    let mut config_dirs: Vec<PathBuf> = Vec::new();
    collect_config_dirs(staging, &mut config_dirs)?;

    // Resolve settings per config dir (parent-first so children can inherit).
    // Upstream inherits from the closest ANCESTOR config, root included.
    let mut by_len = config_dirs.clone();
    by_len.sort_by_key(|dir| dir.to_string_lossy().len());
    let mut dir_settings: BTreeMap<PathBuf, PackSettings> = BTreeMap::new();
    for dir in &by_len {
        let mut inherited: Option<PackSettings> = None;
        let mut parent = dir.parent().map(Path::to_path_buf);
        while let Some(candidate) = parent {
            if let Some(found) = dir_settings.get(&candidate) {
                inherited = Some(found.clone());
                break;
            }
            if candidate == staging {
                break;
            }
            parent = candidate.parent().map(Path::to_path_buf);
        }
        let base = inherited.unwrap_or_default();
        let config_path = dir.join("pack.json");
        let text = fs::read_to_string(&config_path)
            .with_context(|| format!("reading {}", config_path.display()))?;
        let (merged, warnings) = base
            .merged_with(&text)
            .with_context(|| format!("parsing {}", config_path.display()))?;
        for warning in warnings {
            eprintln!("mind-tools: warning: {}: {warning}", config_path.display());
        }
        dir_settings.insert(dir.clone(), merged);
    }

    let mut passes = Vec::new();
    visit_dir(staging, staging, &dir_settings, &mut passes)?;
    if std::env::var_os("MIND_DEBUG_PASSES").is_some() {
        for pass in &passes {
            eprintln!(
                "pass `{}`: {} images, combine={}",
                pass.dir,
                pass.images.len(),
                pass.settings.combine_subdirectories
            );
        }
    }

    if fallback {
        for pass in &mut passes {
            // Upstream rewrites the staged pack.json text 4096 -> 2048 in
            // place; clamping both caps is the faithful equivalent.
            pass.settings.max_width = pass.settings.max_width.min(2048);
            pass.settings.max_height = pass.settings.max_height.min(2048);
        }
    }
    Ok(passes)
}

/// Visits one dir: makes a pass for it, then recurses into the subdirs that
/// keep their own pass (nested-config dirs when this dir combines; immediate
/// subdirs otherwise). Pass order is depth-first, root first, matching Arc's
/// traversal.
fn visit_dir(
    staging: &Path,
    dir: &Path,
    dir_settings: &BTreeMap<PathBuf, PackSettings>,
    passes: &mut Vec<Pass>,
) -> Result<()> {
    let settings = resolve_settings(staging, dir, dir_settings)?;
    if settings.ignore {
        return Ok(());
    }

    let rel = dir
        .strip_prefix(staging)?
        .to_string_lossy()
        .replace('\\', "/");
    let mut pass = Pass {
        dir: rel,
        settings: settings.clone(),
        images: Vec::new(),
    };
    let mut unvisited: Vec<PathBuf> = Vec::new();

    // Own files.
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in &entries {
        if path.is_file() && path.extension().is_some_and(|ext| ext == "png") {
            pass.images.push(image_name(staging, path)?);
        }
    }

    if settings.combine_subdirectories {
        // Everything below that lacks its own pack.json belongs to this pass;
        // subdirs with their own config keep their own pass.
        for path in &entries {
            if path.is_dir() {
                if path.join("pack.json").is_file() {
                    unvisited.push(path.clone());
                } else {
                    collect_combined(staging, path, &mut pass, &mut unvisited)?;
                }
            }
        }
    } else {
        unvisited.extend(entries.iter().filter(|path| path.is_dir()).cloned());
    }

    if !pass.images.is_empty() {
        passes.push(pass);
    }
    for sub in unvisited {
        visit_dir(staging, &sub, dir_settings, passes)?;
    }
    Ok(())
}

/// Recursively collects images for a combine pass; nested-config dirs are
/// pushed to `unvisited` (they keep their own pass).
fn collect_combined(
    staging: &Path,
    dir: &Path,
    pass: &mut Pass,
    unvisited: &mut Vec<PathBuf>,
) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if path.join("pack.json").is_file() {
                unvisited.push(path);
            } else {
                collect_combined(staging, &path, pass, unvisited)?;
            }
        } else if path.is_file() && path.extension().is_some_and(|ext| ext == "png") {
            pass.images.push(image_name(staging, &path)?);
        }
    }
    Ok(())
}

/// Settings for a dir: its own config when present, else the nearest config
/// ancestor's (Arc `processDir` parent walk).
fn resolve_settings(
    staging: &Path,
    dir: &Path,
    dir_settings: &BTreeMap<PathBuf, PackSettings>,
) -> Result<PackSettings> {
    let mut current = Some(dir.to_path_buf());
    while let Some(candidate) = current {
        if let Some(settings) = dir_settings.get(&candidate) {
            return Ok(settings.clone());
        }
        if candidate == staging {
            break;
        }
        current = candidate.parent().map(Path::to_path_buf);
    }
    Ok(PackSettings::default())
}

/// Staging-relative image name without `.png` (`blocks/walls/copper-wall`).
fn image_name(staging: &Path, path: &Path) -> Result<String> {
    let rel = path
        .strip_prefix(staging)
        .with_context(|| format!("stripping {}", path.display()))?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(rel.trim_end_matches(".png").to_owned())
}

fn collect_config_dirs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if dir.join("pack.json").is_file() {
        out.push(dir.to_path_buf());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_config_dirs(&path, out)?;
        }
    }
    Ok(())
}

/// Guards against a `flattenPaths` collision (plan 03 §3.4).
pub fn assert_no_duplicate_region_names(manifest: &AtlasManifest) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for region in &manifest.regions {
        if !seen.insert(&region.name) {
            bail!(
                "duplicate region base name `{}` (flattenPaths collision)",
                region.name
            );
        }
    }
    Ok(())
}

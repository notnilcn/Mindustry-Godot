// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `core/src/mindustry/graphics/Shaders.java` (asset-facing parts) and
// `tools/build.gradle` (shader asset copy). Plan 03 §3.7 / §6.7.

//! Shader asset preparation and drift check (plan 03 M4 + plan 16 M8).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use mind_core::render::shaders::{self, Uniforms};
use serde::{Deserialize, Serialize};

/// One shader manifest entry (§6.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderEntry {
    /// Logical shader name (plan 16 registry key).
    pub name: String,
    /// Upstream GLSL fragment source, when present.
    #[serde(rename = "glslFrag", skip_serializing_if = "Option::is_none", default)]
    pub glsl_frag: Option<String>,
    /// Upstream GLSL vertex source, when present.
    #[serde(rename = "glslVert", skip_serializing_if = "Option::is_none", default)]
    pub glsl_vert: Option<String>,
    /// Generated ported Godot shader path.
    pub gdshader: String,
    /// Uniform names declared by the upstream GLSL (non-sampler).
    #[serde(default)]
    pub uniforms: Vec<String>,
    /// Sampler/texture uniform names declared by the upstream GLSL.
    #[serde(default)]
    pub textures: Vec<String>,
    /// `fragment`/`vertex`.
    pub stage: String,
}

/// Serialized `shader.index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderIndex {
    /// Format version.
    pub format: u32,
    /// Entries sorted by name.
    pub shaders: Vec<ShaderEntry>,
}

/// Result of `shaders check` (plan 03 forward + plan 16 M8 reverse).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShaderCheckReport {
    /// Shaders with a ported `.gdshader`.
    pub ported: usize,
    /// Required shaders without a ported `.gdshader`.
    pub unported: Vec<String>,
    /// Upstream-disabled shaders intentionally not ported (`shockwave`).
    pub unported_ok: Vec<String>,
    /// `(shader, upstream-only uniform/texture)` pairs (forward drift).
    pub drift: Vec<String>,
    /// `(shader, ported-only uniform/texture)` pairs (reverse drift, M8).
    pub reverse_drift: Vec<String>,
}

impl ShaderCheckReport {
    /// Whether the report is clean (no required-unported, no drift).
    pub fn is_clean(&self, reverse: bool) -> bool {
        self.unported.is_empty()
            && self.drift.is_empty()
            && (!reverse || self.reverse_drift.is_empty())
    }
}

/// Builds the shader index over `assets/shaders` and copies any ported
/// `client/shaders/**/*.gdshader` into `assets/shaders/godot/`.
pub fn build(root: &Path) -> Result<ShaderIndex> {
    let source_dir = root.join("assets/shaders");
    let godot_dir = source_dir.join("godot");

    // Copy ported Godot shaders (tracked in `client/shaders/`), preserving
    // relative paths.
    let client_dir = root.join("client/shaders");
    if client_dir.is_dir() {
        let mut files = Vec::new();
        collect_by_ext(&client_dir, "gdshader", &mut files)?;
        files.sort();
        for file in files {
            let rel = file.strip_prefix(&client_dir)?;
            let target = godot_dir.join(rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&file, &target)
                .with_context(|| format!("copying {} -> {}", file.display(), target.display()))?;
        }
    }

    // Collect upstream GLSL by logical name.
    let mut entries: BTreeMap<String, ShaderEntry> = BTreeMap::new();
    let mut glsl = Vec::new();
    collect_glsl(&source_dir, &mut glsl)?;
    glsl.sort();
    for file in glsl {
        let stem = file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .with_context(|| format!("bad shader name {}", file.display()))?
            .to_owned();
        let relative = file
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        let source =
            fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
        let uniforms = shaders::parse_uniforms(&source);
        let is_vert = file.extension().is_some_and(|ext| ext == "vert");
        let entry = entries.entry(stem.clone()).or_insert_with(|| ShaderEntry {
            name: stem.clone(),
            glsl_frag: None,
            glsl_vert: None,
            gdshader: format!("shaders/godot/{stem}.gdshader"),
            uniforms: Vec::new(),
            textures: Vec::new(),
            stage: if is_vert { "vertex" } else { "fragment" }.to_owned(),
        });
        if is_vert {
            entry.glsl_vert = Some(relative);
            entry.stage = String::from("vertex");
        } else {
            entry.glsl_frag = Some(relative);
        }
        entry.uniforms.extend(uniforms.uniforms);
        entry.uniforms.sort();
        entry.uniforms.dedup();
        entry.textures.extend(uniforms.textures);
        entry.textures.sort();
        entry.textures.dedup();
    }

    let index = ShaderIndex {
        format: 1,
        shaders: entries.into_values().collect(),
    };
    fs::create_dir_all(&source_dir)
        .with_context(|| format!("creating {}", source_dir.display()))?;
    let path = source_dir.join("shader.index.json");
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&index)?),
    )
    .with_context(|| format!("writing {}", path.display()))?;
    Ok(index)
}

/// Reads `shader.index.json` and checks every ported shader for uniform drift.
///
/// The forward direction (plan 03) asserts every upstream uniform/texture is
/// declared by the ported `.gdshader`. When `reverse` is set (plan 16 M8), the
/// reverse direction additionally flags `.gdshader` uniforms with no upstream
/// counterpart (typos/leftovers the `apply()` equivalent never feeds).
pub fn check(root: &Path, reverse: bool) -> Result<ShaderCheckReport> {
    let path = root.join("assets/shaders/shader.index.json");
    let text = fs::read_to_string(&path).with_context(|| {
        format!(
            "reading {} (run `mind-tools shaders build` first)",
            path.display()
        )
    })?;
    let index: ShaderIndex =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;

    let mut report = ShaderCheckReport::default();
    let mut seen: Vec<&str> = Vec::new();
    for shader in &index.shaders {
        seen.push(shader.name.as_str());
        if !shaders::requires_port(&shader.name) {
            // Upstream-disabled (e.g. `shockwave`): must not be ported.
            if !root.join("assets").join(&shader.gdshader).is_file() {
                report.unported_ok.push(shader.name.clone());
            }
            continue;
        }
        let godot_path = root.join("assets").join(&shader.gdshader);
        if !godot_path.is_file() {
            report.unported.push(shader.name.clone());
            continue;
        }
        report.ported += 1;
        let source = fs::read_to_string(&godot_path)
            .with_context(|| format!("reading {}", godot_path.display()))?;
        let upstream = Uniforms {
            uniforms: shader.uniforms.clone(),
            textures: shader.textures.clone(),
        };
        let ported = shaders::parse_uniforms(&source);
        for name in shaders::forward_drift(&upstream, &ported) {
            report
                .drift
                .push(format!("{}: {name} (upstream-only)", shader.name));
        }
        if reverse {
            for name in shaders::reverse_drift(&upstream, &ported) {
                report
                    .reverse_drift
                    .push(format!("{}: {name} (ported-only)", shader.name));
            }
        }
    }
    // Every plan-16 registry key must exist in the index.
    for entry in shaders::EXPECTED {
        if !seen.contains(&entry.name) {
            report.unported.push(entry.name.to_owned());
        }
    }
    report.unported.sort();
    report.unported.dedup();
    Ok(report)
}

fn collect_glsl(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "godot") {
                continue;
            }
            collect_glsl(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext == "frag" || ext == "vert")
        {
            out.push(path);
        }
    }
    Ok(())
}

fn collect_by_ext(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_by_ext(&path, ext, out)?;
        } else if path.extension().is_some_and(|found| found == ext) {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_parsing_splits_samplers() {
        let source = r#"
            uniform mat4 u_proj;
            uniform float u_time; // comment
            uniform sampler2D u_texture;
            uniform samplerCube noise;
        "#;
        let parsed = shaders::parse_uniforms(source);
        assert_eq!(parsed.uniforms, vec!["u_proj", "u_time"]);
        assert_eq!(parsed.textures, vec!["noise", "u_texture"]);
    }
}

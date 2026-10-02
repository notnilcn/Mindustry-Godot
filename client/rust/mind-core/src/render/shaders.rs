// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shader registry contract (plan 16 §3.9 / M8).
//!
//! This module owns the **expected logical shader-name list** and the pure
//! uniform-drift helpers shared by `mind-tools shaders check` and the
//! `mind-gdext` `ShaderRegistry`. It is Godot-free: the actual `.gdshader`
//! sources live in `client/shaders/` and the generated index in
//! `assets/shaders/shader.index.json` (plan 03 §6.7).
//!
//! The logical keys mirror `graphics/Shaders.java`. `shockwave` is upstream
//! `disabled` (§3.9) and is deliberately not ported; the `default` and
//! `cubemap` helper shaders are ported but are not registry keys.

/// Godot shader stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderStage {
    /// `shader_type canvas_item;`.
    CanvasItem,
    /// `shader_type spatial;`.
    Spatial,
}

/// One expected logical shader (plan 16 §3.9).
#[derive(Clone, Copy, Debug)]
pub struct ExpectedShader {
    /// Logical registry key (also the `.gdshader` stem).
    pub name: &'static str,
    /// Godot stage.
    pub stage: ShaderStage,
    /// `Shaders.shield` is nullable: a missing file skips the shield bracket.
    pub nullable: bool,
}

const fn shader(name: &'static str, stage: ShaderStage) -> ExpectedShader {
    ExpectedShader {
        name,
        stage,
        nullable: false,
    }
}

/// The 24 logical registry keys from plan 16 §3.9 (order = upstream
/// `Shaders.init()` grouping, append-only).
pub const EXPECTED: &[ExpectedShader] = &[
    shader("blockbuild", ShaderStage::CanvasItem),
    shader("buildbeam", ShaderStage::CanvasItem),
    shader("unitbuild", ShaderStage::CanvasItem),
    shader("unitarmor", ShaderStage::CanvasItem),
    shader("darkness", ShaderStage::CanvasItem),
    shader("fog", ShaderStage::CanvasItem),
    shader("light", ShaderStage::CanvasItem),
    shader("water", ShaderStage::CanvasItem),
    shader("mud", ShaderStage::CanvasItem),
    shader("tar", ShaderStage::CanvasItem),
    shader("slag", ShaderStage::CanvasItem),
    shader("cryofluid", ShaderStage::CanvasItem),
    shader("space", ShaderStage::CanvasItem),
    shader("caustics", ShaderStage::CanvasItem),
    shader("arkycite", ShaderStage::CanvasItem),
    shader("planet", ShaderStage::Spatial),
    shader("clouds", ShaderStage::Spatial),
    shader("planetgrid", ShaderStage::Spatial),
    shader("atmosphere", ShaderStage::Spatial),
    shader("mesh", ShaderStage::Spatial),
    shader("unlit", ShaderStage::Spatial),
    shader("unlitwhite", ShaderStage::Spatial),
    shader("screenspace", ShaderStage::CanvasItem),
    ExpectedShader {
        name: "shield",
        stage: ShaderStage::CanvasItem,
        nullable: true,
    },
];

/// Ported helper shaders that are not registry keys but are still checked.
pub const AUXILIARY: &[&str] = &["default", "cubemap"];

/// Upstream shaders explicitly not ported (`Shaders.java` disabled / dead).
pub const UNPORTED: &[&str] = &["shockwave"];

/// Whether a logical name is a registry key.
pub fn is_registry_key(name: &str) -> bool {
    EXPECTED.iter().any(|entry| entry.name == name)
}

/// Looks up an expected registry entry.
pub fn expected(name: &str) -> Option<&'static ExpectedShader> {
    EXPECTED.iter().find(|entry| entry.name == name)
}

/// Whether a shader stem must have a ported `.gdshader`.
pub fn requires_port(name: &str) -> bool {
    is_registry_key(name) || AUXILIARY.contains(&name)
}

/// `Shaders.shield` parity: `None` skips the shield composite.
pub fn is_nullable(name: &str) -> bool {
    expected(name).is_some_and(|entry| entry.nullable)
}

/// Parsed `uniform` declarations (samplers split out).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Uniforms {
    /// Non-sampler uniform names.
    pub uniforms: Vec<String>,
    /// Sampler uniform names.
    pub textures: Vec<String>,
}

impl Uniforms {
    /// Every declared name (uniforms + textures), sorted.
    pub fn names(&self) -> Vec<String> {
        let mut all: Vec<String> = self
            .uniforms
            .iter()
            .chain(self.textures.iter())
            .cloned()
            .collect();
        all.sort();
        all.dedup();
        all
    }
}

/// Parses Godot `uniform <type> <name>[: hint][ = default];` declarations.
///
/// Handles both the Godot and Arc GLSL spellings (`sampler2D`, `samplerCube`)
/// and ignores comments. Samplers are collected into [`Uniforms::textures`].
pub fn parse_uniforms(source: &str) -> Uniforms {
    let mut result = Uniforms::default();
    for raw in source.lines() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if !line.starts_with("uniform") {
            continue;
        }
        let rest = line.trim_start_matches("uniform").trim();
        let mut parts = rest.split_whitespace();
        let Some(kind) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        let name = name
            .trim_end_matches(';')
            .split(['[', '=', ':', ','])
            .next()
            .unwrap_or(name)
            .trim();
        if name.is_empty() {
            continue;
        }
        if kind.starts_with("sampler") {
            result.textures.push(name.to_owned());
        } else {
            result.uniforms.push(name.to_owned());
        }
    }
    result.uniforms.sort();
    result.uniforms.dedup();
    result.textures.sort();
    result.textures.dedup();
    result
}

/// Upstream uniforms/textures that a ported `.gdshader` is missing.
pub fn forward_drift(upstream: &Uniforms, ported: &Uniforms) -> Vec<String> {
    let ported_names = ported.names();
    upstream
        .names()
        .into_iter()
        .filter(|name| !ported_names.contains(name))
        .collect()
}

/// Godot-declared uniforms/textures with no upstream counterpart (reverse
/// drift: catches typos and leftover declarations the `apply()` never feeds).
pub fn reverse_drift(upstream: &Uniforms, ported: &Uniforms) -> Vec<String> {
    let upstream_names = upstream.names();
    ported
        .names()
        .into_iter()
        .filter(|name| !upstream_names.contains(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_registry_keys_unique_and_ported() {
        let mut names: Vec<&str> = EXPECTED.iter().map(|entry| entry.name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate expected shader key");
        assert_eq!(EXPECTED.len(), 24, "plan 16 §3.9 logical key count");
        assert!(is_registry_key("shield"));
        assert!(is_nullable("shield"));
        assert!(!is_nullable("light"));
        for name in ["default", "cubemap"] {
            assert!(requires_port(name), "auxiliary {name} must be ported");
        }
        assert!(!requires_port("shockwave"));
        assert!(UNPORTED.contains(&"shockwave"));
    }

    #[test]
    fn parse_godot_uniform_syntax() {
        let source = r#"
            shader_type canvas_item;
            uniform sampler2D u_texture;
            uniform sampler2D u_noise : hint_default_white;
            uniform vec2 u_campos = vec2(0.0);
            uniform float u_time;
            // uniform float u_ignored;
        "#;
        let parsed = parse_uniforms(source);
        assert_eq!(parsed.uniforms, vec!["u_campos", "u_time"]);
        assert_eq!(parsed.textures, vec!["u_noise", "u_texture"]);
    }

    #[test]
    fn drift_directions_are_exact() {
        let upstream = parse_uniforms("uniform sampler2D u_texture;\nuniform float u_time;");
        let ported = parse_uniforms("uniform sampler2D u_texture;\nuniform float u_tame;");
        assert_eq!(forward_drift(&upstream, &ported), vec!["u_time"]);
        assert_eq!(reverse_drift(&upstream, &ported), vec!["u_tame"]);
        // Identical sets produce no drift in either direction.
        let same = parse_uniforms("uniform sampler2D u_texture;\nuniform float u_time;");
        assert!(forward_drift(&upstream, &same).is_empty());
        assert!(reverse_drift(&upstream, &same).is_empty());
    }
}

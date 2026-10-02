// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: annotations/src/main/java/mindustry/annotations/misc/
//         LoadRegionProcessor.java (templating + `fallback` audit),
//         core/src/mindustry/ctype/UnlockableContent.java (`loadIcon` chain).

//! `@Load` region population (plan 03 §3.3/M6).
//!
//! The Godot-free half of `@Load`: the [`LoadCtx`] passed to content load, the
//! [`LoadRegions`] trait the derived impls implement, the exact `@`/`@size`/
//! `#`/`#1`/`#2` substitution order from `LoadRegionProcessor.parse`, and
//! [`RegionAudit`] which separates explicit-`fallback` misses (reported) from
//! `fallback=error` misses (fatal).
//!
//! The `#[derive(LoadRegions)]` proc-macro itself lives in plan 05's
//! `mind-macros` crate (not yet on this branch); see the M6 hand-off in
//! `03_ASSETS_IMPLEMENTATION_PLAN.md`.

use super::atlas::{AtlasIndex, Region};

/// `LoadRegionProcessor` load context for one content record.
#[derive(Debug, Clone, Copy)]
pub struct LoadCtx<'a> {
    /// Region index (`Core.atlas`).
    pub atlas: &'a AtlasIndex,
    /// `content.name` substituted for `@`.
    pub content_name: &'a str,
    /// `Block.size` substituted for `@size` (`0` for non-block content).
    pub size: u32,
    /// Loop indices substituted for `#1`/`#2`/`#` (`0` when not looping).
    pub indices: [usize; 2],
}

impl<'a> LoadCtx<'a> {
    /// Builds a context for a non-block, non-array field.
    pub fn new(atlas: &'a AtlasIndex, content_name: &'a str) -> Self {
        Self {
            atlas,
            content_name,
            size: 0,
            indices: [0, 0],
        }
    }

    /// Expands a `@Load` template exactly like `LoadRegionProcessor.parse`.
    pub fn resolve(&self, pattern: &str) -> String {
        template(pattern, self)
    }

    /// Resolves a template and looks it up with [`AtlasIndex::find`] semantics.
    pub fn find(&self, pattern: &str) -> Option<&'a Region> {
        self.atlas.find(&self.resolve(pattern))
    }
}

/// Implemented by region-holder structs whose fields carry `@Load` annotations.
///
/// The `#[derive(LoadRegions)]` macro (`mind-macros`) generates this impl: each
/// annotated scalar field becomes `audit.load(ctx, pattern, fallback).cloned()`
/// and each array field emits the same nested `for` loops as
/// `LoadRegionProcessor` (setting `ctx.indices`), so the templating order stays
/// ABI. The resolved [`Region`]s are owned (`Option<Region>`) so the holder can
/// be a transient audit view and never needs a lifetime.
pub trait LoadRegions {
    /// Populates every `@Load` field from `ctx`, folding misses into `audit`.
    fn load_regions(&mut self, ctx: &mut LoadCtx, audit: &mut RegionAudit);
}

/// `LoadRegionProcessor.parse` replacement order:
/// `@size` → `@` → `#1` → `#2` → `#` (order is ABI; `#1`/`#2` before `#`).
pub fn template(pattern: &str, ctx: &LoadCtx) -> String {
    let mut out = pattern.replace("@size", &ctx.size.to_string());
    out = out.replace('@', ctx.content_name);
    out = out.replace("#1", &ctx.indices[0].to_string());
    out = out.replace("#2", &ctx.indices[1].to_string());
    out = out.replace('#', &ctx.indices[0].to_string());
    out
}

/// `@Load` miss accumulator (`fallback` audit, plan 03 §2.2/§7.1b).
///
/// [`RegionAudit::load`] mirrors `Core.atlas.find(value[, fallback])`: a hit
/// returns the region; an explicit non-`error` fallback records a recoverable
/// miss and returns the fallback region; a `fallback=error` miss records a fatal
/// error and bridges to the always-present `error` region for rendering.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegionAudit {
    /// Names that missed but had an explicit non-`error` fallback.
    pub explicit_fallbacks: Vec<String>,
    /// Names that missed with `fallback=error` (fatal; must be empty at exit).
    pub errors: Vec<String>,
}

impl RegionAudit {
    /// Empty audit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves one `@Load` pattern. `fallback == None` means the annotation
    /// default `"error"`. Returns the resolved region (the fallback/`error`
    /// region when the primary name is absent).
    pub fn load<'a>(
        &mut self,
        ctx: &LoadCtx<'a>,
        pattern: &str,
        fallback: Option<&str>,
    ) -> Option<&'a Region> {
        let name = ctx.resolve(pattern);
        if let Some(region) = ctx.atlas.find(&name) {
            return Some(region);
        }
        match fallback {
            Some(fb) if fb != "error" => {
                self.explicit_fallbacks.push(name);
                ctx.atlas.find(fb)
            }
            _ => {
                self.errors.push(name);
                ctx.atlas.find("error")
            }
        }
    }

    /// Whether any `fallback=error` miss was recorded.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::atlas::AtlasIndex;

    const FIXTURE: &str = r#"{
  "format": 1,
  "pages": [{"index":0,"type":"main","file":"sprites.png","width":64,"height":64,"sha256":""}],
  "regions": [
    {"name":"foo","page":0,"x":0,"y":0,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"foo-team-3","page":0,"x":8,"y":0,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"weapons/foo-0","page":0,"x":16,"y":0,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"foo-barrel-2","page":0,"x":24,"y":0,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"foo-x-1","page":0,"x":32,"y":0,"w":8,"h":8,"offsets":[0,0],"pageType":"main"},
    {"name":"error","page":0,"x":40,"y":0,"w":3,"h":3,"offsets":[0,0],"pageType":"main"},
    {"name":"fallback-region","page":0,"x":44,"y":0,"w":3,"h":3,"offsets":[0,0],"pageType":"main"}
  ]
}"#;

    #[test]
    fn templating_matches_processor() {
        let index = AtlasIndex::from_manifest_json(FIXTURE).unwrap();
        let ctx = LoadCtx {
            atlas: &index,
            content_name: "foo",
            size: 3,
            indices: [0, 2],
        };
        // `@size` before `@`; `#1`/`#2` before `#`.
        assert_eq!(ctx.resolve("@-team-@size"), "foo-team-3");
        assert_eq!(ctx.resolve("weapons/@-#1"), "weapons/foo-0");
        assert_eq!(ctx.resolve("@-barrel-#"), "foo-barrel-0");
        assert_eq!(ctx.resolve("@-x-#2"), "foo-x-2");
        assert_eq!(ctx.resolve("@"), "foo");
        // A pattern with no tokens is untouched.
        assert_eq!(ctx.resolve("plain-region"), "plain-region");
    }

    #[test]
    fn find_uses_atlas_semantics() {
        let index = AtlasIndex::from_manifest_json(FIXTURE).unwrap();
        let ctx = LoadCtx::new(&index, "foo");
        assert!(ctx.find("@").is_some());
        assert!(ctx.find("@-missing").is_none());
    }

    #[test]
    fn audit_separates_explicit_fallback_from_errors() {
        let index = AtlasIndex::from_manifest_json(FIXTURE).unwrap();
        let ctx = LoadCtx::new(&index, "foo");
        let mut audit = RegionAudit::new();

        // Hit.
        assert_eq!(audit.load(&ctx, "@", None).unwrap().name, "foo");
        // Explicit fallback: reported, resolved to the fallback region.
        assert_eq!(
            audit
                .load(&ctx, "@-missing", Some("fallback-region"))
                .unwrap()
                .name,
            "fallback-region"
        );
        assert_eq!(audit.explicit_fallbacks, vec!["foo-missing".to_owned()]);
        assert!(!audit.has_errors());
        // `fallback=error` (None) miss: fatal, bridges to `error`.
        assert_eq!(audit.load(&ctx, "@-gone", None).unwrap().name, "error");
        assert_eq!(audit.errors, vec!["foo-gone".to_owned()]);
        assert!(audit.has_errors());
    }
}

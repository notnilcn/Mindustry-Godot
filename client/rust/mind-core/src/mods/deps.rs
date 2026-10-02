// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Dependency resolution (`Mods.resolveDependencies`/`resolve`).
//!
//! Ported from `core/src/mindustry/mod/Mods.java:1044-1097` with the same
//! depth-first traversal, visited set and `OrderedMap` output shape. Rust's
//! `IndexMap`/`IndexSet` replace Arc `ObjectMap`/`OrderedSet`.

use indexmap::{IndexMap, IndexSet};

use super::ModState;
use super::meta::ModMeta;

/// `Mods.ModDependency`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModDependency {
    /// Internal dependency name.
    pub name: String,
    /// Required (vs soft) dependency.
    pub required: bool,
}

/// `Mods.ModResolutionContext`.
#[derive(Debug, Default)]
pub struct ModResolutionContext {
    /// internal name → declared dependencies.
    pub dependencies: IndexMap<String, Vec<ModDependency>>,
    /// DFS recursion stack (cleared between roots).
    pub visited: IndexSet<String>,
    /// Successfully ordered names.
    pub ordered: IndexSet<String>,
    /// Failed names and their states.
    pub invalid: IndexMap<String, ModState>,
}

/// `Mods.resolveDependencies`: returns an insertion-ordered
/// `internal-name → state` map. `is_enabled` reads
/// `mod-<name>-enabled` (default true).
pub fn resolve_dependencies(
    metas: &[ModMeta],
    is_enabled: &dyn Fn(&str) -> bool,
) -> IndexMap<String, ModState> {
    let mut context = ModResolutionContext::default();
    for meta in metas {
        let mut dependencies = Vec::new();
        for name in &meta.dependencies {
            dependencies.push(ModDependency {
                name: name.clone(),
                required: true,
            });
        }
        for name in &meta.soft_dependencies {
            dependencies.push(ModDependency {
                name: name.clone(),
                required: false,
            });
        }
        context
            .dependencies
            .insert(meta.internal_name.clone(), dependencies);
    }

    let keys: Vec<String> = context.dependencies.keys().cloned().collect();
    for key in keys {
        if context.ordered.contains(&key) {
            continue;
        }
        resolve(&key, is_enabled, &mut context);
        context.visited.clear();
    }

    let mut result = IndexMap::new();
    for name in &context.ordered {
        result.insert(name.clone(), ModState::Enabled);
    }
    for (name, state) in context.invalid {
        result.insert(name, state);
    }
    result
}

/// `Mods.resolve`: single DFS step. Returns whether `element` is resolvable.
fn resolve(
    element: &str,
    is_enabled: &dyn Fn(&str) -> bool,
    context: &mut ModResolutionContext,
) -> bool {
    context.visited.insert(element.to_owned());
    let dependencies = context
        .dependencies
        .get(element)
        .cloned()
        .unwrap_or_default();
    for dependency in dependencies {
        if context.visited.contains(&dependency.name) && !context.ordered.contains(&dependency.name)
        {
            context
                .invalid
                .insert(dependency.name, ModState::CircularDependencies);
            return false;
        } else if context.dependencies.contains_key(&dependency.name) {
            let resolved = context.ordered.contains(&dependency.name)
                || resolve(&dependency.name, is_enabled, context);
            if (!resolved || !is_enabled(&dependency.name)) && dependency.required {
                context
                    .invalid
                    .insert(element.to_owned(), ModState::IncompleteDependencies);
                return false;
            }
        } else if dependency.required {
            context
                .invalid
                .insert(element.to_owned(), ModState::MissingDependencies);
            return false;
        }
    }
    if !context.ordered.contains(element) {
        context.ordered.insert(element.to_owned());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(name: &str, deps: &[&str], soft: &[&str]) -> ModMeta {
        ModMeta {
            name: name.to_owned(),
            internal_name: name.to_owned(),
            dependencies: deps.iter().map(|d| (*d).to_owned()).collect(),
            soft_dependencies: soft.iter().map(|d| (*d).to_owned()).collect(),
            ..ModMeta::default()
        }
    }

    fn always_enabled(_: &str) -> bool {
        true
    }

    /// Plan 20 M0: `deps::missing_incomplete_circular`.
    #[test]
    fn missing_incomplete_circular() {
        let metas = vec![
            meta("a", &["missing"], &[]),
            meta("b", &[], &[]),
            meta("c", &["b"], &[]),
            meta("d", &["d"], &[]),
        ];
        let is_enabled = |name: &str| name != "b";
        let resolved = resolve_dependencies(&metas, &is_enabled);
        assert_eq!(resolved.get("a"), Some(&ModState::MissingDependencies));
        assert_eq!(resolved.get("c"), Some(&ModState::IncompleteDependencies));
        assert_eq!(resolved.get("d"), Some(&ModState::CircularDependencies));
    }

    /// Plan 20 M0: `deps::soft_ignored`.
    #[test]
    fn soft_ignored() {
        let metas = vec![meta("a", &[], &["missing-soft"])];
        let resolved = resolve_dependencies(&metas, &always_enabled);
        assert_eq!(resolved.get("a"), Some(&ModState::Enabled));
    }

    /// Plan 20 M0: `deps::order_matches_expected` — dependencies precede
    /// dependents.
    #[test]
    fn order_matches_expected() {
        let metas = vec![
            meta("app", &["lib"], &[]),
            meta("lib", &["core"], &[]),
            meta("core", &[], &[]),
        ];
        let resolved = resolve_dependencies(&metas, &always_enabled);
        let ordered: Vec<&String> = resolved
            .iter()
            .filter(|(_, state)| **state == ModState::Enabled)
            .map(|(name, _)| name)
            .collect();
        let core = ordered.iter().position(|n| *n == "core").expect("core");
        let lib = ordered.iter().position(|n| *n == "lib").expect("lib");
        let app = ordered.iter().position(|n| *n == "app").expect("app");
        assert!(core < lib);
        assert!(lib < app);
    }
}

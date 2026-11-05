// SPDX-License-Identifier: GPL-3.0-only

//! Scene-injected dependency helpers (skill §3: no reaching out).
//!
//! Migrated view classes take their spine dependencies as
//! `#[export] Option<Gd<T>>` fields wired from `client/scenes/game.tscn`
//! (`node_paths` + `NodePath`). These helpers clone the injected reference once,
//! drop freed objects, and log exactly one warning per missing required
//! dependency so the message format cannot drift between classes. Optional
//! dependencies stay silent: the class already guards the `None` branch.
//!
//! Missing required dependency (one line, then the class degrades):
//! ``MindRender: missing required `renderer` (wire it in the owning scene)``.

use godot::classes::Node;
use godot::obj::{Gd, GodotClass, Inherits};

/// Clones `dep` when it is set and still valid; otherwise logs one warning
/// naming the owner class and property, then returns `None`.
pub(crate) fn required<T>(owner: &str, property: &str, dep: &Option<Gd<T>>) -> Option<Gd<T>>
where
    T: GodotClass + Inherits<Node>,
{
    let dep = dep.as_ref().filter(|dep| dep.is_instance_valid()).cloned();
    if dep.is_none() {
        log::warn!("{owner}: missing required `{property}` (wire it in the owning scene)");
    }
    dep
}

/// Clones `dep` when it is set and still valid; optional deps log nothing.
pub(crate) fn optional<T: GodotClass>(dep: &Option<Gd<T>>) -> Option<Gd<T>> {
    dep.as_ref().filter(|dep| dep.is_instance_valid()).cloned()
}

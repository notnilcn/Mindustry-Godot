// SPDX-License-Identifier: GPL-3.0-only

//! `MindHello` — M1 smoke class proving the Rust class registry and log bridge.
//!
//! Milestone M4 replaces it with `MindSimHost`/`MindCamera2D`/`MindTileGrid`
//! (plan `00_FOUNDATION_IMPLEMENTATION_PLAN.md` §3.5); it stays registered until
//! then so `scenes/hello.tscn` remains a valid GDExtension round-trip check.

use godot::prelude::*;

/// Trivial node that logs on `_ready`; its `crate_version` `#[func]` establishes
/// the M4 test-API style for `godot_exec call` checks.
#[derive(GodotClass)]
#[class(init, base=Node)]
pub struct MindHello {
    base: Base<Node>,
}

#[godot_api]
impl INode for MindHello {
    fn ready(&mut self) {
        log::info!("mind-gdext loaded (mind-gdext {})", crate::MIND_VERSION);
    }
}

#[godot_api]
impl MindHello {
    /// Returns the `mind-gdext` crate version as a Godot string.
    #[func]
    pub fn crate_version(&self) -> GString {
        GString::from(crate::MIND_VERSION)
    }
}

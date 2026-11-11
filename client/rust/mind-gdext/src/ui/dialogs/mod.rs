// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Per-dialog controllers for the shared [`super::dialog::MindDialog`] shell.
//!
//! Each module ports one upstream Mindustry dialog (originally one
//! `client/ui/dialogs/*.gd` script) into a
//! [`super::dialog::DialogController`] implementation. The dialog node resolves
//! its registry key from the manifest and dispatches here, so the 46 scenes that
//! inherit `mind_dialog_base.tscn` keep one native root class.
//!
//! The controllers are grouped by upstream dialog family: [`menu`], [`settings`],
//! [`database`], [`game`], [`net`], [`pickers`], [`campaign`], [`maps`],
//! [`schematics`], [`editor`], [`dev`], [`rules`], [`logic`] and [`mods`].
//! Helpers the groups share live in [`common`]. Each id is owned by exactly one
//! module; [`every_manifest_id_resolves_exactly_once`] proves it.

pub mod campaign;
pub mod common;
pub mod database;
pub mod dev;
pub mod editor;
pub mod game;
pub mod logic;
pub mod maps;
pub mod menu;
pub mod mods;
pub mod net;
pub mod pickers;
pub mod rules;
pub mod schematics;
pub mod settings;

use super::dialog::DialogController;

/// The controller for a dialog registry key (`about`, `settings`, …).
pub fn controller_for(id: &str) -> Option<Box<dyn DialogController>> {
    if let Some(controller) = campaign::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = database::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = dev::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = editor::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = game::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = logic::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = maps::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = menu::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = mods::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = net::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = pickers::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = rules::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = schematics::dispatch(id) {
        return Some(controller);
    }
    if let Some(controller) = settings::dispatch(id) {
        return Some(controller);
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::path::PathBuf;

    use super::*;

    /// One module's `dispatch` entry point.
    type Dispatch = fn(&str) -> Option<Box<dyn DialogController>>;

    /// Every dialog id in `client/ui/dialogs_manifest.json` resolves to exactly
    /// one controller.
    ///
    /// Engine-free: the id registry is data, so a plain `cargo test` run needs
    /// no live `GodotInstance`. Some controllers do build engine handles, and
    /// godot-ffi panics without the engine, so a probe that panics still counts
    /// as "this module claims the id": a `match` arm only runs its body for the
    /// id it matched. Two modules answering one id would make the order in
    /// [`controller_for`] load-bearing, which this test forbids.
    #[test]
    fn every_manifest_id_resolves_exactly_once() {
        // `client/ui/dialogs_manifest.json`, next to the Godot project.
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/dialogs_manifest.json");
        let text = std::fs::read_to_string(&path).expect("dialogs_manifest.json in the checkout");
        let manifest: serde_json::Value =
            serde_json::from_str(&text).expect("dialogs_manifest.json is valid JSON");
        let dialogs = manifest["dialogs"]
            .as_array()
            .expect("dialogs_manifest.json has a `dialogs` array");
        // Every dispatch `controller_for` asks, in its order.
        let modules: [Dispatch; 14] = [
            campaign::dispatch,
            database::dispatch,
            dev::dispatch,
            editor::dispatch,
            game::dispatch,
            logic::dispatch,
            maps::dispatch,
            menu::dispatch,
            mods::dispatch,
            net::dispatch,
            pickers::dispatch,
            rules::dispatch,
            schematics::dispatch,
            settings::dispatch,
        ];
        for entry in dialogs {
            let id = entry["name"].as_str().expect("every dialog has a name");
            let mut hits = 0usize;
            for dispatch in modules {
                // A missing engine is an expected, handled probe here, not a
                // test failure.
                let probed = std::panic::catch_unwind(|| dispatch(id).is_some());
                match probed {
                    Ok(true) | Err(_) => hits += 1,
                    Ok(false) => {}
                }
            }
            assert_eq!(
                hits, 1,
                "dialog `{id}` resolves to {hits} controllers, not 1"
            );
        }
    }
}

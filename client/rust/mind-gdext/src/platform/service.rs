// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Client service seam (plan 22 §3.9, OD4).
//!
//! The `GameService` trait and its no-op [`NullService`] live in `mind-core`
//! (Godot-free and unit-tested); this module re-exports them for the
//! `MindPlatform` node so plans 12/19 can call through one path. Steam and
//! Discord are deferred (NUD-04/51=A), so the node always returns the null
//! service until that lands.

pub use mind_core::service::{DISCORD_APP_ID, GameService, NullService, STEAM_APP_ID};

/// The service registry exposed by `MindPlatform`.
///
/// Holds the active [`GameService`]; always [`NullService`] until OD4.
pub struct ServiceRegistry {
    service: NullService,
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self {
            service: NullService,
        }
    }
}

impl ServiceRegistry {
    /// The active service (`&dyn GameService`).
    pub fn service(&self) -> &dyn GameService {
        &self.service
    }

    /// The active service mutably (`&mut dyn GameService`).
    pub fn service_mut(&mut self) -> &mut dyn GameService {
        &mut self.service
    }
}

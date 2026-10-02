// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Client capability hooks — the render/UI/audio seam.
//!
//! `mind-gdext` implements [`ClientHooks`]; `mind-core` only *notifies* it from
//! non-sim contexts (plan 05 §3.3 invariant 1). Hooks are never consulted to
//! make a simulation decision.

use crate::game::State;

/// Render/UI/audio/input notifications. All methods default to no-ops so the
/// headless host needs no implementation.
pub trait ClientHooks: Send {
    /// The game phase changed.
    fn on_state_change(&mut self, _from: State, _to: State) {}
    /// A world finished loading.
    fn on_world_loaded(&mut self) {}
    /// The sim was reset.
    fn on_reset(&mut self) {}
    /// View interpolation alpha (render-only).
    fn view_interpolate(&mut self, _alpha: f32) {}
}

/// No-op hooks (headless default).
#[derive(Debug, Default)]
pub struct NoopClientHooks;

impl ClientHooks for NoopClientHooks {}

/// Optional owner of a [`ClientHooks`] implementation.
///
/// The handle exposes only `&mut` notification methods; sim systems cannot read
/// state back out of it, which enforces the "hooks are notified, never
/// consulted" invariant at the type level.
#[derive(Default)]
pub struct ClientHooksHandle(Option<Box<dyn ClientHooks>>);

impl std::fmt::Debug for ClientHooksHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientHooksHandle")
            .field("present", &self.0.is_some())
            .finish()
    }
}

impl ClientHooksHandle {
    /// Creates a no-op handle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Wraps a concrete hooks implementation.
    pub fn with(hooks: impl ClientHooks + 'static) -> Self {
        Self(Some(Box::new(hooks)))
    }

    /// Whether a hooks implementation is attached.
    pub fn is_present(&self) -> bool {
        self.0.is_some()
    }

    /// Notifies the hooks (no-op when empty).
    pub fn on_state_change(&mut self, from: State, to: State) {
        if let Some(hooks) = &mut self.0 {
            hooks.on_state_change(from, to);
        }
    }

    /// Notifies the hooks (no-op when empty).
    pub fn on_world_loaded(&mut self) {
        if let Some(hooks) = &mut self.0 {
            hooks.on_world_loaded();
        }
    }

    /// Notifies the hooks (no-op when empty).
    pub fn on_reset(&mut self) {
        if let Some(hooks) = &mut self.0 {
            hooks.on_reset();
        }
    }

    /// Notifies the hooks (no-op when empty).
    pub fn view_interpolate(&mut self, alpha: f32) {
        if let Some(hooks) = &mut self.0 {
            hooks.view_interpolate(alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct Recorder(Arc<Mutex<Vec<String>>>);

    impl ClientHooks for Recorder {
        fn on_state_change(&mut self, from: State, to: State) {
            self.0
                .lock()
                .expect("lock")
                .push(format!("{from:?}->{to:?}"));
        }
    }

    #[test]
    fn handle_notifies_and_defaults_to_noop() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut handle = ClientHooksHandle::with(Recorder(log.clone()));
        assert!(handle.is_present());
        handle.on_state_change(State::Menu, State::Playing);
        assert_eq!(log.lock().expect("lock").as_slice(), ["Menu->Playing"]);

        let mut empty = ClientHooksHandle::new();
        assert!(!empty.is_present());
        empty.on_state_change(State::Playing, State::Paused);
        empty.view_interpolate(0.5);
    }
}

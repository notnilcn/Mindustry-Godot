// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `WorldReloader` host/client semantics (plan 12 M8).
//!
//! Ported from `core/src/mindustry/net/WorldReloader.java`. The trait is the
//! deterministic seam the play flows call around a world (re)load; plan 21
//! supplies the networked relay bodies (`Call.worldDataBegin`, per-player
//! `sendWorldAndAssets`) while headless/unit tests use [`NoReloader`] (defined
//! in [`super::play`]).

use bevy_ecs::entity::Entity;

use super::play::PlaySession;

/// Player-state handshake around a world reload (`WorldReloader`).
pub trait WorldReloader {
    /// `WorldReloader.begin()`: snapshot players, reset the world.
    fn begin(&mut self, session: &mut PlaySession);
    /// `WorldReloader.end()`: re-send world/assets to players (plan 21).
    fn end(&mut self, session: &mut PlaySession);
}

/// Host/server reloader: captures remote players and resets the logic world.
#[derive(Debug, Clone, Default)]
pub struct HostReloader {
    /// Snapshot of players disconnected during the reload.
    pub players: Vec<Entity>,
    /// Whether `begin` ran.
    pub began: bool,
    /// Whether this process was the server (`net.server()`).
    pub was_server: bool,
}

impl WorldReloader for HostReloader {
    fn begin(&mut self, session: &mut PlaySession) {
        if self.began {
            return;
        }
        self.was_server = true;
        self.players.clear();
        let present = session.teams.present.clone();
        for team in present {
            if let Some(data) = session.teams.get_or_null(team) {
                for player in &data.players {
                    self.players.push(*player);
                }
            }
        }
        // `logic.reset()` world half.
        session.reset_world();
        self.began = true;
    }

    fn end(&mut self, _session: &mut PlaySession) {
        // Plan 21 re-sends world data/assets and assigns PvP teams here.
    }
}

/// Client reloader: resets and disconnects (`net.reset()`).
#[derive(Debug, Clone, Default)]
pub struct ClientReloader {
    /// Whether `begin` ran.
    pub began: bool,
    /// Whether the client disconnected (`net.reset()`).
    pub disconnected: bool,
}

impl WorldReloader for ClientReloader {
    fn begin(&mut self, session: &mut PlaySession) {
        if self.began {
            return;
        }
        self.disconnected = true;
        session.reset_world();
        self.began = true;
    }

    fn end(&mut self, _session: &mut PlaySession) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::rules::Rules;

    #[test]
    fn host_reloader_is_idempotent_and_resets() {
        let mut session = PlaySession::new(Rules::default());
        session.wave = 5;
        let mut reloader = HostReloader::default();
        reloader.begin(&mut session);
        assert!(reloader.began && reloader.was_server);
        assert_eq!(session.wave, 0, "logic.reset clears the match");
        // Second begin is a no-op.
        session.wave = 9;
        reloader.begin(&mut session);
        assert_eq!(session.wave, 9);
    }

    #[test]
    fn client_reloader_disconnects() {
        let mut session = PlaySession::new(Rules::default());
        let mut reloader = ClientReloader::default();
        reloader.begin(&mut session);
        assert!(reloader.began && reloader.disconnected);
        assert_eq!(session.phase, crate::game::State::Menu);
    }
}

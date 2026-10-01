// SPDX-License-Identifier: GPL-3.0-only

//! Subscription waves, ported from `TableSubscriber.BaseTables/LobbyTables/
//! GameTables`. The lists hold **generated accessor names** (never SQL); each
//! table belongs to exactly one wave (plan 01 §3.6/§3.12 invariant 5).
//!
//! M2 ships the `WaveName` + static lists; M3 adds the live `SubscriptionWaves`
//! state machine (`is_applied`, applied/error events, manual toggles).

/// Base wave: subscribed automatically on connect; every screen needs these.
/// No applied callback by design — late binders replay the cache.
pub const BASE_TABLES: &[&str] = &["protocol_info", "relay_config", "local_client_settings"];

/// Lobby wave: player roster/profile selection before joining a match.
pub const LOBBY_TABLES: &[&str] = &["local_player", "local_player_profile", "all_players"];

/// Game wave: the live match (M4 adds `my_match`/`my_match_commands`); raised
/// explicitly on join and dropped on leave.
pub const GAME_TABLES: &[&str] = &[];

/// One of the three subscription waves (plan 01 §3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WaveName {
    /// Auto-subscribed on connect; no applied event.
    Base,
    /// Auto-subscribed on connect; raises `WaveApplied(Lobby)`.
    Lobby,
    /// Explicit via `subscribe_game()` / dropped via `unsubscribe_game()`.
    Game,
}

impl WaveName {
    /// Every wave, in subscribe order.
    pub const ALL: [Self; 3] = [Self::Base, Self::Lobby, Self::Game];

    /// Stable debug/log name (`"base"`, `"lobby"`, `"game"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Lobby => "lobby",
            Self::Game => "game",
        }
    }

    /// Generated table accessor names in this wave.
    pub fn tables(self) -> &'static [&'static str] {
        match self {
            Self::Base => BASE_TABLES,
            Self::Lobby => LOBBY_TABLES,
            Self::Game => GAME_TABLES,
        }
    }

    /// The wave that contains `table`, if any (uses the generated accessor name).
    pub fn for_table(table: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|wave| wave.tables().contains(&table))
    }
}

/// Every table name across all waves, in wave order, without duplicates.
pub fn all_tables() -> Vec<&'static str> {
    let mut tables = Vec::new();
    for wave in WaveName::ALL {
        for table in wave.tables() {
            if !tables.contains(table) {
                tables.push(*table);
            }
        }
    }
    tables
}

/// Per-connection wave bookkeeping (plan §3.6).
///
/// `desired` survives disconnects (the connector re-issues desired waves on
/// every connect); `applied` is cleared on disconnect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionWaves {
    applied: [bool; 3],
    desired: [bool; 3],
}

impl Default for SubscriptionWaves {
    fn default() -> Self {
        let mut desired = [false; 3];
        // Base + Lobby are auto-subscribed on connect; Game is explicit.
        desired[wave_index(WaveName::Base)] = true;
        desired[wave_index(WaveName::Lobby)] = true;
        Self {
            applied: [false; 3],
            desired,
        }
    }
}

impl SubscriptionWaves {
    /// New state with Base+Lobby desired and nothing applied.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `wave` has applied on the current connection.
    pub fn is_applied(&self, wave: WaveName) -> bool {
        self.applied[wave_index(wave)]
    }

    /// Whether `wave` should be subscribed whenever connected.
    pub fn is_desired(&self, wave: WaveName) -> bool {
        self.desired[wave_index(wave)]
    }

    /// Sets the desired flag (does not touch `applied`).
    pub fn set_desired(&mut self, wave: WaveName, desired: bool) {
        self.desired[wave_index(wave)] = desired;
    }

    /// Records that `wave` applied.
    pub fn mark_applied(&mut self, wave: WaveName) {
        self.applied[wave_index(wave)] = true;
    }

    /// Records that `wave` is no longer active.
    pub fn mark_dropped(&mut self, wave: WaveName) {
        self.applied[wave_index(wave)] = false;
    }

    /// Clears every applied flag (connection dropped); desired flags survive.
    pub fn clear(&mut self) {
        self.applied = [false; 3];
    }
}

/// Wave lifecycle event (public mirror of the connector's internal callback).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaveEvent {
    /// The wave's rows are in the client cache.
    Applied(WaveName),
    /// The wave failed to subscribe.
    Error {
        /// Failed wave.
        wave: WaveName,
        /// Human-readable reason.
        message: String,
    },
}

fn wave_index(wave: WaveName) -> usize {
    match wave {
        WaveName::Base => 0,
        WaveName::Lobby => 1,
        WaveName::Game => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_tables_are_unique_and_all_known() {
        let mut seen: Vec<&str> = Vec::new();
        for wave in WaveName::ALL {
            assert!(!wave.tables().is_empty() || wave == WaveName::Game);
            for table in wave.tables() {
                assert!(
                    !seen.contains(table),
                    "table `{table}` is listed in more than one wave"
                );
                seen.push(table);
                assert_eq!(WaveName::for_table(table), Some(wave));
            }
        }
        assert_eq!(all_tables().len(), seen.len());
    }

    #[test]
    fn subscription_waves_track_desired_and_applied() {
        let mut waves = SubscriptionWaves::new();
        assert!(waves.is_desired(WaveName::Base));
        assert!(waves.is_desired(WaveName::Lobby));
        assert!(!waves.is_desired(WaveName::Game));
        assert!(!waves.is_applied(WaveName::Base));

        waves.set_desired(WaveName::Game, true);
        waves.mark_applied(WaveName::Game);
        assert!(waves.is_applied(WaveName::Game));

        // Disconnect clears applied but keeps desired.
        waves.clear();
        assert!(!waves.is_applied(WaveName::Game));
        assert!(waves.is_desired(WaveName::Game));

        waves.mark_dropped(WaveName::Base);
        assert!(!waves.is_applied(WaveName::Base));
    }
}

// SPDX-License-Identifier: GPL-3.0-only

//! Subscription waves, ported from `TableSubscriber.BaseTables/LobbyTables/
//! GameTables`. The lists hold **generated accessor names** (never SQL); each
//! table belongs to exactly one wave (plan 01 §3.6/§3.12 invariant 5).
//!
//! M2 ships the `WaveName` + static lists; M3 adds the live `SubscriptionWaves`
//! state machine (`is_applied`, applied/error events, manual toggles).

/// Base wave: subscribed automatically on connect; every screen needs these.
/// No applied callback by design — late binders replay the cache.
pub const BASE_TABLES: &[&str] = &[
    "protocol_info",
    "relay_config",
    "local_client_settings",
    "server_config",
];

/// Lobby wave: player roster/profile selection before joining a match.
pub const LOBBY_TABLES: &[&str] = &[
    "local_player",
    "local_player_profile",
    "all_players",
    "my_matches",
    "all_matches",
];

/// Game wave: the live match; raised explicitly on join and dropped on leave.
///
/// Plan 21 M3/M4 add the player-state, plan, UI-event, chat, checksum and
/// snapshot-metadata views.
pub const GAME_TABLES: &[&str] = &[
    "my_match",
    "my_match_commands",
    "my_match_members",
    "my_match_state",
    "my_kick",
    "my_sender_command_state",
    "my_match_player_states",
    "my_match_plans",
    "my_match_plan_chunks",
    "my_match_ui_events",
    "my_match_chat",
    "my_match_checksums",
    "my_match_snapshots",
    "my_match_snapshot_requests",
];

/// Snapshot wave: on-demand chunk download (plan §6.2); only while resyncing.
pub const SNAPSHOT_TABLES: &[&str] = &["my_match_snapshot_chunks"];

/// One of the four subscription waves (plan 01 §3.6; plan 21 M4 adds Snapshot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WaveName {
    /// Auto-subscribed on connect; no applied event.
    Base,
    /// Auto-subscribed on connect; raises `WaveApplied(Lobby)`.
    Lobby,
    /// Explicit via `subscribe_game()` / dropped via `unsubscribe_game()`.
    Game,
    /// Explicit via `subscribe_snapshot()` while downloading chunks.
    Snapshot,
}

impl WaveName {
    /// Every wave, in subscribe order.
    pub const ALL: [Self; 4] = [Self::Base, Self::Lobby, Self::Game, Self::Snapshot];

    /// Stable debug/log name (`"base"`, `"lobby"`, `"game"`, `"snapshot"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Lobby => "lobby",
            Self::Game => "game",
            Self::Snapshot => "snapshot",
        }
    }

    /// Generated table accessor names in this wave.
    pub fn tables(self) -> &'static [&'static str] {
        match self {
            Self::Base => BASE_TABLES,
            Self::Lobby => LOBBY_TABLES,
            Self::Game => GAME_TABLES,
            Self::Snapshot => SNAPSHOT_TABLES,
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
    applied: [bool; 4],
    desired: [bool; 4],
}

impl Default for SubscriptionWaves {
    fn default() -> Self {
        let mut desired = [false; 4];
        // Base + Lobby are auto-subscribed on connect; Game/Snapshot are explicit.
        desired[wave_index(WaveName::Base)] = true;
        desired[wave_index(WaveName::Lobby)] = true;
        Self {
            applied: [false; 4],
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
        self.applied = [false; 4];
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
        WaveName::Snapshot => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_tables_are_unique_and_all_known() {
        let mut seen: Vec<&str> = Vec::new();
        for wave in WaveName::ALL {
            assert!(!wave.tables().is_empty());
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

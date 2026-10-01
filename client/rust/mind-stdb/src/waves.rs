// SPDX-License-Identifier: GPL-3.0-only

//! Subscription waves, ported from `TableSubscriber.BaseTables/LobbyTables/GameTables`.
//!
//! The lists below hold **generated accessor names** (never SQL): the P0 module only has
//! the raw public `player` table, so it is the sole Base entry. Plan `01` replaces these
//! with the view lists (`protocol_info`/`relay_config`/... Base,
//! `local_player`/`all_players`/... Lobby, `my_match`/`my_match_commands` Game);
//! [`SubscriptionWave::tables`] and [`all_tables`] are the stable consumers of the lists.

/// Base wave: subscribed automatically on connect; every screen needs these.
///
/// P0: the raw public `player` row (identity/username). Plan `01` targets the
/// `protocol_info`/`relay_config`/`relay_match`/`local_client_settings` views.
pub const BASE_TABLES: &[&str] = &["player"];

/// Lobby wave: profile selection before joining a match (plan `01`:
/// `local_player`, `local_player_profile`, `all_players`, `my_matches`).
pub const LOBBY_TABLES: &[&str] = &[];

/// Game wave: the live match (`my_match`, `my_match_commands`; raised manually on join).
pub const GAME_TABLES: &[&str] = &[];

/// One of the three subscription waves (see the plan-00/01 wave table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SubscriptionWave {
    Base,
    Lobby,
    Game,
}

impl SubscriptionWave {
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
    for wave in SubscriptionWave::ALL {
        for table in wave.tables() {
            if !tables.contains(table) {
                tables.push(*table);
            }
        }
    }
    tables
}

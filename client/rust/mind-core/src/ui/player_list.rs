// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/PlayerListFragment.java (plan 14 M7 §3.5).

//! Player-list view model + relay seam (plan 14 M7).
//!
//! The row projection, sort (`team` then admin-first), fog/team filtering and
//! action surface of `PlayerListFragment`. The actions themselves are plan 21's
//! relay (`Call.kick`/`ban`/`trace`/`ping`/`vote`); [`PlayerAction::dispatch`]
//! is the documented seam and returns [`PlayerListError::RelayUnavailable`]
//! until the relay lands. No server behaviour is faked (task rule).

/// Action available from a player row (`PlayerListFragment`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerAction {
    /// Spectate the player (`control.input.spectate`).
    Spectate,
    /// Kick (`Call.kick`).
    Kick,
    /// Ban (`Call.adminRequest`/ban dialog).
    Ban,
    /// Trace (`TraceDialog`).
    Trace,
    /// Ping a location (`Call.pingLocation`).
    Ping,
    /// Vote-kick.
    Vote,
}

/// Relay failures (plan 21 owns the transport).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerListError {
    /// The multiplayer relay is not available (plan 21 not landed).
    RelayUnavailable,
    /// The player/team is not actionable (dead, other team, no connection).
    NotActionable,
}

impl std::fmt::Display for PlayerListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayerListError::RelayUnavailable => {
                write!(f, "multiplayer relay unavailable (plan 21)")
            }
            PlayerListError::NotActionable => write!(f, "player is not actionable"),
        }
    }
}

impl std::error::Error for PlayerListError {}

/// One player row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSummary {
    /// Player name.
    pub name: String,
    /// Team id.
    pub team: u8,
    /// Team color `rrggbb` (for the icon border).
    pub color: String,
    /// Admin flag.
    pub admin: bool,
    /// Dead (spectate disabled).
    pub dead: bool,
    /// Local player.
    pub local: bool,
    /// Has a live connection id (server clients only; `local` players may not).
    pub connection: Option<u32>,
    /// Ping in ms, when known.
    pub ping: Option<i32>,
}

/// `PlayerListFragment` view model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerListModel {
    /// All players.
    pub players: Vec<PlayerSummary>,
    /// Search text.
    pub search: String,
    /// `!state.isCampaign() && (rules.pvp || rules.infiniteResources)`.
    pub allow_team_switch: bool,
    /// `rules.fog && rules.pvp`.
    pub fog_pvp: bool,
    /// Local player team (for the fog/team clickable rule).
    pub local_team: u8,
    /// `net.active() && state.isGame()`.
    pub active: bool,
}

impl Default for PlayerListModel {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerListModel {
    /// Empty model.
    pub fn new() -> Self {
        Self {
            players: Vec::new(),
            search: String::new(),
            allow_team_switch: false,
            fog_pvp: false,
            local_team: 0,
            active: true,
        }
    }

    /// `rebuild`: sorts by `(team, !admin)` then applies the search filter.
    pub fn rebuild(&mut self) -> Vec<&PlayerSummary> {
        self.players.sort_by(|a, b| {
            a.team
                .cmp(&b.team)
                .then_with(|| b.admin.cmp(&a.admin))
                .then_with(|| a.name.cmp(&b.name))
        });
        self.filtered()
    }

    /// Post-sort search filter (`Strings.stripColors(name).toLowerCase().contains`).
    pub fn filtered(&self) -> Vec<&PlayerSummary> {
        if self.search.is_empty() {
            return self.players.iter().collect();
        }
        let needle = self.search.to_lowercase();
        self.players
            .iter()
            .filter(|player| strip_colors(&player.name).to_lowercase().contains(&needle))
            .collect()
    }

    /// `clickable` — fog/pvp hides other-team players.
    pub fn is_clickable(&self, player: &PlayerSummary) -> bool {
        !(self.fog_pvp && player.team != self.local_team)
    }

    /// The options a row exposes for a player.
    pub fn actions_for(&self, player: &PlayerSummary) -> Vec<PlayerAction> {
        let mut actions = Vec::new();
        if !player.dead && self.is_clickable(player) {
            actions.push(PlayerAction::Spectate);
        }
        actions.push(PlayerAction::Kick);
        actions.push(PlayerAction::Ban);
        actions.push(PlayerAction::Trace);
        actions.push(PlayerAction::Ping);
        actions
    }

    /// Documented seam for row actions. Plan 21's relay performs the send; until
    /// then every action fails with [`PlayerListError::RelayUnavailable`].
    pub fn dispatch(
        &self,
        player: &PlayerSummary,
        action: PlayerAction,
    ) -> Result<(), PlayerListError> {
        if !self.active {
            return Err(PlayerListError::NotActionable);
        }
        if action == PlayerAction::Spectate && (player.dead || !self.is_clickable(player)) {
            return Err(PlayerListError::NotActionable);
        }
        Err(PlayerListError::RelayUnavailable)
    }
}

/// `Strings.stripColors` — removes `[xxx]` color tags.
pub fn strip_colors(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '[' {
            let mut consumed = false;
            for next in chars.by_ref() {
                if next == ']' {
                    consumed = true;
                    break;
                }
            }
            let _ = consumed;
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(name: &str, team: u8, admin: bool) -> PlayerSummary {
        PlayerSummary {
            name: name.to_owned(),
            team,
            color: String::from("ffffff"),
            admin,
            dead: false,
            local: false,
            connection: Some(1),
            ping: Some(20),
        }
    }

    #[test]
    fn sorts_by_team_then_admin() {
        let mut model = PlayerListModel {
            players: vec![
                player("bob", 2, false),
                player("alice", 1, false),
                player("admin", 1, true),
                player("other", 1, false),
            ],
            ..PlayerListModel::new()
        };
        let rows = model.rebuild();
        let names: Vec<&str> = rows.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["admin", "alice", "other", "bob"]);
    }

    #[test]
    fn search_strips_colors() {
        let mut model = PlayerListModel {
            players: vec![player("[red]Bob[]", 1, false), player("alice", 1, false)],
            search: "bob".to_owned(),
            ..PlayerListModel::new()
        };
        model.rebuild();
        let rows = model.filtered();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "[red]Bob[]");
        assert_eq!(strip_colors("[red]Bob[]"), "Bob");
    }

    #[test]
    fn fog_pvp_hides_other_team() {
        let model = PlayerListModel {
            players: vec![player("enemy", 2, false)],
            fog_pvp: true,
            local_team: 1,
            ..PlayerListModel::new()
        };
        assert!(!model.is_clickable(&model.players[0]));
        assert!(
            !model
                .actions_for(&model.players[0])
                .contains(&PlayerAction::Spectate)
        );
    }

    #[test]
    fn actions_dispatch_is_gated_on_relay() {
        let model = PlayerListModel {
            players: vec![player("bob", 1, false)],
            ..PlayerListModel::new()
        };
        assert_eq!(
            model.dispatch(&model.players[0], PlayerAction::Kick),
            Err(PlayerListError::RelayUnavailable)
        );
    }

    #[test]
    #[ignore = "plan 21: player-list relay actions require the multiplayer relay"]
    fn relay_actions_round_trip() {
        // Enabled when plan 21 supplies MindUi.chat_send/player_action.
    }
}

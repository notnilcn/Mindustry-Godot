// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Relay player roster and player-state data path (plan 21 §3.9, PAR-0122).
//!
//! Ported from `core/src/mindustry/core/NetServer.java` (`assignTeam`
//! `:530-548`, `kickAll` `:1096-1100`), `core/src/mindustry/net/Administration.java`
//! (`PlayerInfo` history) and `core/src/mindustry/ui/fragments/PlayerListFragment.java`
//! (`PlayerListFragment` rows: name, team, admin flag, connection state).
//!
//! The roster is a pure aggregation over the plan-21 row binders
//! (`relay_member`, `player`, `player_session`, `admin_identity`, `player_ban`,
//! `whitelist_entry`, `match_kick`, `admin_action_log`) plus the action mapping
//! the admin UI/console dispatch through: every [`PlayerAction`] names the
//! reducer it calls, so the same description drives the in-game player list,
//! the dedicated-server console and the JSON the UI consumes.
//!
//! Deviation (documented, D2): there are no peer IPs and no locale/mobile
//! columns in the relay schema, so `PlayerEntry::locale` stays empty and the
//! trace shows session/kick/action history instead of IP history.

use std::collections::{BTreeMap, BTreeSet};

use spacetimedb_sdk::Identity;

use crate::binder::RowChange;
use crate::connector::{Connector, ConnectorError};
use crate::module_bindings::{
    AdminActionKind, AdminActionLog, AdminIdentity, MatchKick, Player, PlayerBan, PlayerSession,
    PlayerTrace, RelayMember, WhitelistEntry,
};

/// One player-list row (`PlayerListFragment` peer entry / console `players`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerEntry {
    /// Identity hex (64 lowercase hex chars).
    pub identity: String,
    /// Last-known username (`""` when unresolved).
    pub name: String,
    /// Assigned team index (`Player.team`; `None` when unassigned).
    pub team: Option<u8>,
    /// Whether a global admin record exists (`Player.admin`).
    pub admin: bool,
    /// Whether the member is currently connected.
    pub connected: bool,
    /// Membership role (`player`/`spectator`).
    pub role: String,
    /// Locale (always `""` until the relay schema carries one).
    pub locale: String,
    /// Mobile flag (always `false` until the relay schema carries one).
    pub mobile: bool,
    /// Last-seen timestamp (micros since the Unix epoch; `None` when unknown).
    pub last_seen_micros: Option<i64>,
    /// Kick reason recorded on the `relay_member` row, if any.
    pub kicked_reason: Option<String>,
}

impl PlayerEntry {
    /// Stable JSON row for `MindNet.players_json()` / the UI player list.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "identity": self.identity,
            "name": self.name,
            "team": self.team,
            "admin": self.admin,
            "connected": self.connected,
            "role": self.role,
            "locale": self.locale,
            "mobile": self.mobile,
            "kicked": self.kicked_reason.is_some(),
            "lastSeen": self.last_seen_micros,
        })
    }

    /// Whether `target` (name or identity prefix) matches this row.
    pub fn matches(&self, target: &str) -> bool {
        let needle = target.to_ascii_lowercase();
        !needle.is_empty()
            && (self.identity.to_ascii_lowercase().starts_with(&needle)
                || self.name.to_ascii_lowercase() == needle)
    }
}

/// Admin action history for one identity (`Administration.PlayerInfo`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceInfo {
    /// Identity hex.
    pub identity: String,
    /// Known usernames (most recent first).
    pub names: Vec<String>,
    /// Total recorded sessions (`player_session` rows).
    pub sessions: usize,
    /// Whether a session is still open (`ended_at == null`).
    pub connected: bool,
    /// Join time of the latest session (micros).
    pub last_join_micros: Option<i64>,
    /// Kicks `(reason, by hex, at micros)`.
    pub kicks: Vec<(String, String, i64)>,
    /// Admin actions `(kind, detail, at micros)`.
    pub actions: Vec<(String, String, i64)>,
    /// Whether a ban record exists.
    pub banned: bool,
    /// Whether a whitelist record exists.
    pub whitelisted: bool,
}

impl TraceInfo {
    /// Stable JSON for the trace dialog / console `info`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "identity": self.identity,
            "names": self.names,
            "sessions": self.sessions,
            "connected": self.connected,
            "lastJoin": self.last_join_micros,
            "banned": self.banned,
            "whitelisted": self.whitelisted,
            "kicks": self.kicks.iter().map(|(reason, by, at)| serde_json::json!({
                "reason": reason, "by": by, "at": at,
            })).collect::<Vec<_>>(),
            "actions": self.actions.iter().map(|(kind, detail, at)| serde_json::json!({
                "kind": kind, "detail": detail, "at": at,
            })).collect::<Vec<_>>(),
        })
    }
}

/// A team candidate for `assignTeam` (`TeamData` liveness inputs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssignTeamView {
    /// Team id.
    pub team: u8,
    /// Whether the team has a core (`TeamData.hasCore`).
    pub has_core: bool,
    /// Whether the team protects cores (`Team.rules().protectCores`).
    pub protect_cores: bool,
}

/// An admin action the player list / console can dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerAction {
    /// Kick from the match (`admin_kick`).
    Kick {
        /// Kick reason shown to the client.
        reason: String,
    },
    /// Ban an identity (`admin_ban`; duration in seconds, `None` = permanent).
    Ban {
        /// Ban reason.
        reason: String,
        /// Optional ban duration in seconds.
        duration_secs: Option<u64>,
    },
    /// Lift an identity ban (`admin_unban`).
    Unban,
    /// Grant/revoke global admin (`admin_grant`).
    GrantAdmin(bool),
    /// Add/remove a whitelist entry (`admin_whitelist`).
    Whitelist(bool),
    /// Move a player to `team` (`admin_switch_team`).
    SwitchTeam(u8),
    /// Ban a username pattern (`admin_ban_name`, GAPS #34).
    BanName {
        /// Case-insensitive regex enforced on create/join.
        pattern: String,
    },
    /// Lift a username-pattern ban (`admin_unban_name`).
    UnbanName {
        /// Pattern to remove.
        pattern: String,
    },
    /// Request a server-computed trace answer (`admin_request_trace`).
    RequestTrace,
}

impl PlayerAction {
    /// The reducer name this action maps to (UI/console/JSON).
    pub fn reducer(&self) -> &'static str {
        match self {
            PlayerAction::Kick { .. } => "admin_kick",
            PlayerAction::Ban { .. } => "admin_ban",
            PlayerAction::Unban => "admin_unban",
            PlayerAction::GrantAdmin(_) => "admin_grant",
            PlayerAction::Whitelist(_) => "admin_whitelist",
            PlayerAction::SwitchTeam(_) => "admin_switch_team",
            PlayerAction::BanName { .. } => "admin_ban_name",
            PlayerAction::UnbanName { .. } => "admin_unban_name",
            PlayerAction::RequestTrace => "admin_request_trace",
        }
    }

    /// Short UI name (`kick`/`ban`/`unban`/`admin`/`whitelist`/`team`/
    /// `ban_name`/`unban_name`/`trace`).
    pub fn name(&self) -> &'static str {
        match self {
            PlayerAction::Kick { .. } => "kick",
            PlayerAction::Ban { .. } => "ban",
            PlayerAction::Unban => "unban",
            PlayerAction::GrantAdmin(_) => "admin",
            PlayerAction::Whitelist(_) => "whitelist",
            PlayerAction::SwitchTeam(_) => "team",
            PlayerAction::BanName { .. } => "ban_name",
            PlayerAction::UnbanName { .. } => "unban_name",
            PlayerAction::RequestTrace => "trace",
        }
    }

    /// Dispatches the reducer against `connector` for the hosted `match_id`.
    pub fn apply(
        &self,
        connector: &mut Connector,
        match_id: u64,
        target: Identity,
    ) -> Result<(), ConnectorError> {
        match self {
            PlayerAction::Kick { reason } => connector.admin_kick(match_id, target, reason),
            PlayerAction::Ban {
                reason,
                duration_secs,
            } => connector.admin_ban(target, reason, *duration_secs),
            PlayerAction::Unban => connector.admin_unban(target),
            PlayerAction::GrantAdmin(on) => connector.admin_grant(target, *on),
            PlayerAction::Whitelist(on) => connector.admin_whitelist(target, *on),
            PlayerAction::SwitchTeam(team) => connector.admin_switch_team(match_id, target, *team),
            // Name bans are identity-independent: the UI passes the offending
            // name as the pattern, so `target` is unused by this arm.
            PlayerAction::BanName { pattern } => connector.admin_ban_name(pattern),
            PlayerAction::UnbanName { pattern } => connector.admin_unban_name(pattern),
            PlayerAction::RequestTrace => connector.admin_request_trace(Some(match_id), target),
        }
    }
}

/// Live relay player roster (`NetServer.admins` + `Groups.player` halves).
#[derive(Debug, Default)]
pub struct Roster {
    members: BTreeMap<String, RelayMember>,
    players: BTreeMap<String, Player>,
    sessions: BTreeMap<String, Vec<PlayerSession>>,
    admins: BTreeSet<String>,
    bans: BTreeSet<String>,
    whitelist: BTreeSet<String>,
    kicks: Vec<MatchKick>,
    actions: Vec<AdminActionLog>,
    traces: Vec<PlayerTrace>,
}

impl Roster {
    /// Empty roster.
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops every row (disconnect/resync).
    pub fn clear(&mut self) {
        self.members.clear();
        self.players.clear();
        self.sessions.clear();
        self.admins.clear();
        self.bans.clear();
        self.whitelist.clear();
        self.kicks.clear();
        self.actions.clear();
        self.traces.clear();
    }

    /// Applies a `relay_member` change.
    pub fn apply_member(&mut self, change: RowChange<RelayMember>) {
        match change {
            RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                self.members.insert(row.identity.to_hex().to_string(), row);
            }
            RowChange::Delete(row) => {
                self.members.remove(&row.identity.to_hex().to_string());
            }
        }
    }

    /// Applies a `player` change (username/colour).
    pub fn apply_player(&mut self, change: RowChange<Player>) {
        match change {
            RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                self.players.insert(row.identity.to_hex().to_string(), row);
            }
            RowChange::Delete(row) => {
                self.players.remove(&row.identity.to_hex().to_string());
            }
        }
    }

    /// Applies a `player_session` change (trace join history).
    pub fn apply_session(&mut self, change: RowChange<PlayerSession>) {
        let identity = match &change {
            RowChange::Insert(row) | RowChange::Delete(row) => row.identity.to_hex().to_string(),
            RowChange::Update { old, .. } => old.identity.to_hex().to_string(),
        };
        let entry = self.sessions.entry(identity).or_default();
        match change {
            RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                if let Some(existing) = entry
                    .iter_mut()
                    .find(|session| session.session_id == row.session_id)
                {
                    *existing = row;
                } else {
                    entry.push(row);
                }
            }
            RowChange::Delete(row) => {
                entry.retain(|session| session.session_id != row.session_id);
            }
        }
    }

    /// Applies an `admin_identity` change.
    pub fn apply_admin(&mut self, change: RowChange<AdminIdentity>) {
        let identity = match &change {
            RowChange::Insert(row) | RowChange::Delete(row) => row.identity.to_hex().to_string(),
            RowChange::Update { old, .. } => old.identity.to_hex().to_string(),
        };
        match change {
            RowChange::Delete(_) => {
                self.admins.remove(&identity);
            }
            _ => {
                self.admins.insert(identity);
            }
        }
    }

    /// Applies a `player_ban` change.
    pub fn apply_ban(&mut self, change: RowChange<PlayerBan>) {
        let identity = match &change {
            RowChange::Insert(row) | RowChange::Delete(row) => row.identity.to_hex().to_string(),
            RowChange::Update { old, .. } => old.identity.to_hex().to_string(),
        };
        match change {
            RowChange::Delete(_) => {
                self.bans.remove(&identity);
            }
            _ => {
                self.bans.insert(identity);
            }
        }
    }

    /// Applies a `whitelist_entry` change.
    pub fn apply_whitelist(&mut self, change: RowChange<WhitelistEntry>) {
        let identity = match &change {
            RowChange::Insert(row) | RowChange::Delete(row) => row.identity.to_hex().to_string(),
            RowChange::Update { old, .. } => old.identity.to_hex().to_string(),
        };
        match change {
            RowChange::Delete(_) => {
                self.whitelist.remove(&identity);
            }
            _ => {
                self.whitelist.insert(identity);
            }
        }
    }

    /// Applies a `match_kick`/`my_kick` change (trace history).
    pub fn apply_kick(&mut self, change: RowChange<MatchKick>) {
        match change {
            RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                if !self.kicks.iter().any(|kick| kick.kick_id == row.kick_id) {
                    self.kicks.push(row);
                }
            }
            RowChange::Delete(row) => {
                self.kicks.retain(|kick| kick.kick_id != row.kick_id);
            }
        }
    }

    /// Applies an `admin_action_log`/`my_admin_actions` change (trace history).
    pub fn apply_admin_action(&mut self, change: RowChange<AdminActionLog>) {
        match change {
            RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                if !self
                    .actions
                    .iter()
                    .any(|action| action.action_id == row.action_id)
                {
                    self.actions.push(row);
                }
            }
            RowChange::Delete(row) => {
                self.actions
                    .retain(|action| action.action_id != row.action_id);
            }
        }
    }

    /// Applies a `my_traces` change (server answer to `admin_request_trace`).
    pub fn apply_trace(&mut self, change: RowChange<PlayerTrace>) {
        match change {
            RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                if let Some(existing) = self
                    .traces
                    .iter_mut()
                    .find(|trace| trace.trace_id == row.trace_id)
                {
                    *existing = row;
                } else {
                    self.traces.push(row);
                }
            }
            RowChange::Delete(row) => {
                self.traces.retain(|trace| trace.trace_id != row.trace_id);
            }
        }
    }

    /// Every known player, sorted by team then name (`PlayerListFragment` order).
    pub fn players(&self) -> Vec<PlayerEntry> {
        let mut out: Vec<PlayerEntry> = self
            .members
            .values()
            .map(|member| self.entry(member))
            .collect();
        out.sort_by(|left, right| {
            left.team
                .unwrap_or(u8::MAX)
                .cmp(&right.team.unwrap_or(u8::MAX))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.identity.cmp(&right.identity))
        });
        out
    }

    /// Connected players only (`Groups.player`).
    pub fn connected(&self) -> Vec<PlayerEntry> {
        self.players()
            .into_iter()
            .filter(|player| player.connected)
            .collect()
    }

    /// Resolves a console/UI target (identity prefix or exact name).
    pub fn find(&self, target: &str) -> Option<PlayerEntry> {
        self.players()
            .into_iter()
            .find(|player| player.matches(target))
    }

    /// Connected players per team (`TeamData` player counts).
    pub fn team_count(&self, team: u8) -> usize {
        self.connected()
            .into_iter()
            .filter(|player| player.team == Some(team))
            .count()
    }

    /// Whether an identity holds a global admin record.
    pub fn is_admin(&self, identity: &str) -> bool {
        self.admins.contains(identity)
    }

    /// Whether an identity holds a ban record.
    pub fn is_banned(&self, identity: &str) -> bool {
        self.bans.contains(identity)
    }

    /// Whether an identity holds a whitelist record.
    pub fn is_whitelisted(&self, identity: &str) -> bool {
        self.whitelist.contains(identity)
    }

    /// `NetServer.assigner` (`NetServer.java:57-75`): PvP balancing across the
    /// live teams with a core, else the default team.
    ///
    /// Upstream breaks equal player counts with a random jitter; this port
    /// breaks ties by the lower team id so a scenario/trace replays
    /// byte-for-byte (documented deviation, R13).
    pub fn assign_team(
        &self,
        pvp: bool,
        default_team: u8,
        wave_team: u8,
        waves: bool,
        teams: &[AssignTeamView],
    ) -> Option<u8> {
        if !pvp {
            return Some(default_team);
        }
        let mut best: Option<(usize, u8)> = None;
        for view in teams {
            if (waves && view.team == wave_team) || !view.has_core || !view.protect_cores {
                continue;
            }
            if view.team == 0 {
                // `Team.derelict` is never a player team.
                continue;
            }
            let count = self.team_count(view.team);
            let better = match best {
                Some((best_count, best_team)) => {
                    count < best_count || (count == best_count && view.team < best_team)
                }
                None => true,
            };
            if better {
                best = Some((count, view.team));
            }
        }
        best.map(|(_, team)| team)
    }

    /// Admin action history for `identity` (`Administration.PlayerInfo`).
    pub fn trace(&self, identity: &str) -> TraceInfo {
        let identity = identity.to_ascii_lowercase();
        let sessions = self.sessions.get(&identity).cloned().unwrap_or_default();
        let connected = sessions.iter().any(|session| session.ended_at.is_none());
        let last_join = sessions
            .iter()
            .map(|session| session.started_at.to_micros_since_unix_epoch())
            .max();
        let names = self
            .players
            .get(&identity)
            .map(|player| vec![player.username.clone()])
            .unwrap_or_default();
        let kicks = self
            .kicks
            .iter()
            .filter(|kick| kick.target.to_hex().to_string() == identity)
            .map(|kick| {
                (
                    kick.reason.clone(),
                    kick.by.to_hex().to_string(),
                    kick.at.to_micros_since_unix_epoch(),
                )
            })
            .collect();
        let actions = self
            .actions
            .iter()
            .filter(|action| {
                action
                    .target
                    .map(|target| target.to_hex().to_string() == identity)
                    .unwrap_or(false)
            })
            .map(|action| {
                (
                    admin_kind_name(&action.kind).to_owned(),
                    action.detail.clone(),
                    action.at.to_micros_since_unix_epoch(),
                )
            })
            .collect();
        TraceInfo {
            identity: identity.clone(),
            names,
            sessions: sessions.len(),
            connected,
            last_join_micros: last_join,
            kicks,
            actions,
            banned: self.bans.contains(&identity),
            whitelisted: self.whitelist.contains(&identity),
        }
    }

    /// The player list as a JSON array (UI `player_list_json`).
    pub fn players_json(&self) -> serde_json::Value {
        serde_json::Value::Array(self.players().iter().map(PlayerEntry::to_json).collect())
    }

    /// The trace as JSON (trace dialog / console).
    ///
    /// The locally aggregated history is authoritative for the live relay; when
    /// the module answered an `admin_request_trace` with a `my_traces` row, the
    /// server-computed names/joins/kicks are added under `"server"` (Java
    /// `Administration.TraceInfo`).
    pub fn trace_json(&self, identity: &str) -> serde_json::Value {
        let mut json = self.trace(identity).to_json();
        if let Some(trace) = self.latest_trace(identity) {
            json["server"] = server_trace_json(trace);
        }
        json
    }

    /// Newest `my_traces` answer for `identity` (name or identity hex).
    pub fn latest_trace(&self, identity: &str) -> Option<&PlayerTrace> {
        let query = identity.to_ascii_lowercase();
        self.traces
            .iter()
            .filter(|trace| {
                trace.target.to_hex().to_string() == query
                    || trace
                        .names
                        .iter()
                        .any(|name| name.to_ascii_lowercase() == query)
            })
            .max_by_key(|trace| {
                (
                    trace.requested_at.to_micros_since_unix_epoch(),
                    trace.trace_id,
                )
            })
    }

    /// Builds one entry from the member row plus the aux tables.
    fn entry(&self, member: &RelayMember) -> PlayerEntry {
        let identity = member.identity.to_hex().to_string();
        let player = self.players.get(&identity);
        PlayerEntry {
            name: player
                .map(|player| player.username.clone())
                .unwrap_or_default(),
            team: member.team,
            admin: self.admins.contains(&identity),
            connected: member.connected,
            role: format!("{:?}", member.role).to_ascii_lowercase(),
            locale: String::new(),
            mobile: false,
            last_seen_micros: player.map(|player| player.last_seen_at.to_micros_since_unix_epoch()),
            kicked_reason: member.kicked_reason.clone(),
            identity,
        }
    }
}

/// Stable name for an [`AdminActionKind`] (trace JSON).
fn admin_kind_name(kind: &AdminActionKind) -> &'static str {
    match kind {
        AdminActionKind::Kick => "kick",
        AdminActionKind::Ban => "ban",
        AdminActionKind::Unban => "unban",
        AdminActionKind::GrantAdmin => "grantAdmin",
        AdminActionKind::RevokeAdmin => "revokeAdmin",
        AdminActionKind::WhitelistAdd => "whitelistAdd",
        AdminActionKind::WhitelistRemove => "whitelistRemove",
        AdminActionKind::SetWhitelist => "setWhitelist",
        AdminActionKind::SetConfig => "setConfig",
        AdminActionKind::RunWave => "runWave",
        AdminActionKind::SkipWave => "skipWave",
        AdminActionKind::SwitchTeam => "switchTeam",
        AdminActionKind::TileOp => "tileOp",
        AdminActionKind::AddChatFilter => "addChatFilter",
        AdminActionKind::RemoveChatFilter => "removeChatFilter",
        AdminActionKind::AddMatchChatFilter => "addMatchChatFilter",
        AdminActionKind::RemoveMatchChatFilter => "removeMatchChatFilter",
        AdminActionKind::BanName => "banName",
        AdminActionKind::UnbanName => "unbanName",
        AdminActionKind::RequestTrace => "requestTrace",
        AdminActionKind::AddActionFilter => "addActionFilter",
        AdminActionKind::RemoveActionFilter => "removeActionFilter",
    }
}

/// Stable JSON for one `my_traces` answer (Java `Administration.TraceInfo`).
///
/// The address columns stay empty under deviation D2 (the relay schema has no
/// client addresses), like [`TraceInfo`].
pub fn server_trace_json(trace: &PlayerTrace) -> serde_json::Value {
    serde_json::json!({
        "traceId": trace.trace_id,
        "target": trace.target.to_hex().to_string(),
        "matchId": trace.match_id,
        "ip": trace.ip,
        "locale": trace.locale,
        "timesJoined": trace.times_joined,
        "timesKicked": trace.times_kicked,
        "ips": trace.ips,
        "names": trace.names,
        "requestedAt": trace.requested_at.to_micros_since_unix_epoch(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::config::{ConnectionConfig, StdbMode};
    use crate::module_bindings::MemberRole;
    use spacetimedb_sdk::Timestamp;

    fn identity(byte: u8) -> Identity {
        Identity::from_byte_array([byte; 32])
    }

    fn member(byte: u8, name_team: Option<u8>, connected: bool, role: MemberRole) -> RelayMember {
        let now = Timestamp::from_micros_since_unix_epoch(1_000_000);
        RelayMember {
            member_id: byte as u64,
            match_id: 7,
            identity: identity(byte),
            joined_at: now,
            role,
            team: name_team,
            connected,
            ready: connected,
            last_seen_at: now,
            kicked_reason: None,
        }
    }

    fn player(byte: u8, username: &str) -> Player {
        let now = Timestamp::from_micros_since_unix_epoch(2_000_000);
        Player {
            identity: identity(byte),
            username: username.to_owned(),
            created_at: now,
            last_seen_at: now,
            protocol_version: 1,
            color: 0xffff_ffff,
        }
    }

    fn admin(byte: u8) -> AdminIdentity {
        AdminIdentity {
            identity: identity(byte),
            granted_by: identity(1),
            granted_at: Timestamp::UNIX_EPOCH,
        }
    }

    /// Member + player + admin rows produce a populated player list.
    #[test]
    fn member_rows_produce_player_entries() {
        let mut roster = Roster::new();
        roster.apply_member(RowChange::Insert(member(
            2,
            Some(1),
            true,
            MemberRole::Player,
        )));
        roster.apply_member(RowChange::Insert(member(
            3,
            Some(2),
            true,
            MemberRole::Player,
        )));
        roster.apply_member(RowChange::Insert(member(
            4,
            None,
            false,
            MemberRole::Spectator,
        )));
        roster.apply_player(RowChange::Insert(player(2, "Alice")));
        roster.apply_player(RowChange::Insert(player(3, "Bob")));
        roster.apply_admin(RowChange::Insert(admin(3)));

        let players = roster.players();
        assert_eq!(players.len(), 3);
        // Team 1 before team 2 before unassigned.
        assert_eq!(players[0].name, "Alice");
        assert_eq!(players[0].team, Some(1));
        assert!(!players[0].admin);
        assert!(players[0].connected);
        assert_eq!(players[1].name, "Bob");
        assert!(players[1].admin, "admin flag from admin_identity");
        assert_eq!(players[2].role, "spectator");
        assert!(!players[2].connected);
        assert_eq!(roster.connected().len(), 2);
        assert_eq!(roster.team_count(1), 1);

        let json = roster.players_json();
        assert_eq!(json.as_array().map(Vec::len), Some(3));
        assert_eq!(json[0]["identity"], identity(2).to_hex().to_string());
        assert!(roster.find("alice").is_some());
        assert!(roster.find("bo").is_none(), "identity prefix needs hex");
        assert!(roster.find(identity(3).to_hex().as_ref()).is_some());

        // Deletes drop the row.
        roster.apply_member(RowChange::Delete(member(
            2,
            Some(1),
            true,
            MemberRole::Player,
        )));
        assert_eq!(roster.players().len(), 2);
        roster.clear();
        assert!(roster.players().is_empty());
    }

    /// `assignTeam` balances PvP teams and honours the filter rules.
    #[test]
    fn assign_team_balances_pvp() {
        let mut roster = Roster::new();
        roster.apply_member(RowChange::Insert(member(
            2,
            Some(1),
            true,
            MemberRole::Player,
        )));
        roster.apply_member(RowChange::Insert(member(
            3,
            Some(1),
            true,
            MemberRole::Player,
        )));
        roster.apply_member(RowChange::Insert(member(
            4,
            Some(2),
            true,
            MemberRole::Player,
        )));
        let teams = [
            AssignTeamView {
                team: 1,
                has_core: true,
                protect_cores: true,
            },
            AssignTeamView {
                team: 2,
                has_core: true,
                protect_cores: true,
            },
            AssignTeamView {
                team: 3,
                has_core: false,
                protect_cores: true,
            },
            AssignTeamView {
                team: 4,
                has_core: true,
                protect_cores: false,
            },
        ];
        // Non-PvP: the default team.
        assert_eq!(roster.assign_team(false, 1, 9, true, &teams), Some(1));
        // PvP: team 2 has fewer players; teams 3/4 are filtered out.
        assert_eq!(roster.assign_team(true, 1, 9, true, &teams), Some(2));
        // The wave team is excluded while waves are enabled.
        let only_wave = [AssignTeamView {
            team: 2,
            has_core: true,
            protect_cores: true,
        }];
        assert_eq!(roster.assign_team(true, 1, 2, true, &only_wave), None);
        assert_eq!(roster.assign_team(true, 1, 2, false, &only_wave), Some(2));
    }

    /// Trace aggregates sessions, kicks, bans and admin actions.
    #[test]
    fn trace_aggregates_history() {
        let mut roster = Roster::new();
        let identity_hex = identity(2).to_hex().to_string();
        let now = Timestamp::from_micros_since_unix_epoch(3_000_000);
        roster.apply_session(RowChange::Insert(PlayerSession {
            session_id: 1,
            identity: identity(2),
            connection_id: None,
            started_at: now,
            ended_at: None,
        }));
        roster.apply_player(RowChange::Insert(player(2, "Alice")));
        roster.apply_kick(RowChange::Insert(MatchKick {
            kick_id: 1,
            match_id: 7,
            target: identity(2),
            reason: "spam".to_owned(),
            by: identity(1),
            at: now,
        }));
        roster.apply_admin_action(RowChange::Insert(AdminActionLog {
            action_id: 1,
            actor: identity(1),
            kind: AdminActionKind::SwitchTeam,
            target: Some(identity(2)),
            match_id: Some(7),
            detail: "team 2".to_owned(),
            at: now,
        }));
        roster.apply_ban(RowChange::Insert(PlayerBan {
            ban_id: 1,
            identity: identity(2),
            reason: "spam".to_owned(),
            banned_by: identity(1),
            created_at: now,
            expires_at: None,
        }));

        let trace = roster.trace(&identity_hex);
        assert_eq!(trace.names, vec!["Alice".to_owned()]);
        assert_eq!(trace.sessions, 1);
        assert!(trace.connected);
        assert!(trace.banned);
        assert!(!trace.whitelisted);
        assert_eq!(trace.kicks.len(), 1);
        assert_eq!(trace.kicks[0].0, "spam");
        assert_eq!(trace.actions.len(), 1);
        assert_eq!(trace.actions[0].0, "switchTeam");
        let json = roster.trace_json(&identity_hex);
        assert_eq!(json["sessions"], 1);
        assert_eq!(json["banned"], true);

        // Whitelist + unban clear the flags.
        roster.apply_whitelist(RowChange::Insert(WhitelistEntry {
            identity: identity(2),
            added_by: identity(1),
            added_at: now,
        }));
        roster.apply_ban(RowChange::Delete(PlayerBan {
            ban_id: 1,
            identity: identity(2),
            reason: String::new(),
            banned_by: identity(1),
            created_at: now,
            expires_at: None,
        }));
        let trace = roster.trace(&identity_hex);
        assert!(!trace.banned);
        assert!(trace.whitelisted);
    }

    /// Every action names its reducer and dispatches through the connector
    /// (offline here, so the reducer path reports `Offline`).
    #[test]
    fn player_actions_map_to_reducers() {
        let cases = [
            (
                PlayerAction::Kick {
                    reason: "spam".to_owned(),
                },
                "kick",
                "admin_kick",
            ),
            (
                PlayerAction::Ban {
                    reason: "spam".to_owned(),
                    duration_secs: None,
                },
                "ban",
                "admin_ban",
            ),
            (PlayerAction::Unban, "unban", "admin_unban"),
            (PlayerAction::GrantAdmin(true), "admin", "admin_grant"),
            (
                PlayerAction::Whitelist(false),
                "whitelist",
                "admin_whitelist",
            ),
            (PlayerAction::SwitchTeam(2), "team", "admin_switch_team"),
            (
                PlayerAction::BanName {
                    pattern: "bad.*name".to_owned(),
                },
                "ban_name",
                "admin_ban_name",
            ),
            (
                PlayerAction::UnbanName {
                    pattern: "bad.*name".to_owned(),
                },
                "unban_name",
                "admin_unban_name",
            ),
            (PlayerAction::RequestTrace, "trace", "admin_request_trace"),
        ];
        let config = ConnectionConfig {
            mode: StdbMode::Offline,
            ..ConnectionConfig::local()
        };
        let mut connector = Connector::new(config);
        for (action, name, reducer) in cases {
            assert_eq!(action.name(), name);
            assert_eq!(action.reducer(), reducer);
            let error = action
                .apply(&mut connector, 7, identity(2))
                .expect_err("offline connector rejects reducers");
            assert!(
                matches!(error, ConnectorError::Offline),
                "{} must call the reducer path: {error:?}",
                action.reducer()
            );
        }
    }

    /// `kickAll`-style removal of members on disconnect clears connections.
    #[test]
    fn disconnect_marks_members_offline() {
        let mut roster = Roster::new();
        let mut row = member(2, Some(1), true, MemberRole::Player);
        roster.apply_member(RowChange::Insert(row.clone()));
        assert_eq!(roster.connected().len(), 1);
        row.connected = false;
        row.kicked_reason = Some("gameover".to_owned());
        roster.apply_member(RowChange::Update {
            old: member(2, Some(1), true, MemberRole::Player),
            new: row,
        });
        assert!(roster.connected().is_empty());
        assert_eq!(
            roster.players()[0].kicked_reason.as_deref(),
            Some("gameover")
        );
    }

    /// Every admin action kind has a stable trace name (ABI append-only:
    /// a new variant fails this test until it is mapped).
    #[test]
    fn admin_action_kinds_have_stable_names() {
        let cases = [
            (AdminActionKind::Kick, "kick"),
            (AdminActionKind::Ban, "ban"),
            (AdminActionKind::Unban, "unban"),
            (AdminActionKind::GrantAdmin, "grantAdmin"),
            (AdminActionKind::RevokeAdmin, "revokeAdmin"),
            (AdminActionKind::WhitelistAdd, "whitelistAdd"),
            (AdminActionKind::WhitelistRemove, "whitelistRemove"),
            (AdminActionKind::SetWhitelist, "setWhitelist"),
            (AdminActionKind::SetConfig, "setConfig"),
            (AdminActionKind::RunWave, "runWave"),
            (AdminActionKind::SkipWave, "skipWave"),
            (AdminActionKind::SwitchTeam, "switchTeam"),
            (AdminActionKind::TileOp, "tileOp"),
            (AdminActionKind::AddChatFilter, "addChatFilter"),
            (AdminActionKind::RemoveChatFilter, "removeChatFilter"),
            (AdminActionKind::AddMatchChatFilter, "addMatchChatFilter"),
            (
                AdminActionKind::RemoveMatchChatFilter,
                "removeMatchChatFilter",
            ),
            (AdminActionKind::BanName, "banName"),
            (AdminActionKind::UnbanName, "unbanName"),
            (AdminActionKind::RequestTrace, "requestTrace"),
            (AdminActionKind::AddActionFilter, "addActionFilter"),
            (AdminActionKind::RemoveActionFilter, "removeActionFilter"),
        ];
        let target = identity(2).to_hex().to_string();
        for (kind, name) in cases {
            assert_eq!(admin_kind_name(&kind), name);
            let mut roster = Roster::new();
            roster.apply_admin_action(RowChange::Insert(AdminActionLog {
                action_id: 1,
                actor: identity(1),
                kind,
                target: Some(identity(2)),
                match_id: Some(7),
                detail: "detail".to_owned(),
                at: Timestamp::UNIX_EPOCH,
            }));
            let trace = roster.trace(&target);
            assert_eq!(trace.actions.len(), 1);
            assert_eq!(trace.actions[0].0, name, "{kind:?} JSON name");
        }
    }

    /// `my_traces` answers (`admin_request_trace`) merge into the trace JSON.
    #[test]
    fn server_trace_answers_merge_into_trace_json() {
        let mut roster = Roster::new();
        let target = identity(2).to_hex().to_string();
        let first = PlayerTrace {
            trace_id: 1,
            requested_by: identity(1),
            target: identity(2),
            match_id: Some(7),
            ip: None,
            locale: Some("en".to_owned()),
            times_joined: 4,
            times_kicked: 1,
            ips: Vec::new(),
            names: vec!["Alice".to_owned(), "Alicia".to_owned()],
            requested_at: Timestamp::from_micros_since_unix_epoch(4_000_000),
        };
        roster.apply_trace(RowChange::Insert(first.clone()));
        let json = roster.trace_json(&target);
        assert_eq!(json["server"]["traceId"], 1);
        assert_eq!(json["server"]["timesJoined"], 4);
        assert_eq!(json["server"]["timesKicked"], 1);
        assert_eq!(json["server"]["locale"], "en");
        assert_eq!(json["server"]["names"][0], "Alice");
        assert_eq!(json["server"]["matchId"], 7);
        assert!(roster.latest_trace(&target).is_some());
        assert!(roster.latest_trace("alice").is_some(), "name lookup");

        // A newer answer wins, updates replace in place.
        roster.apply_trace(RowChange::Insert(PlayerTrace {
            trace_id: 2,
            times_joined: 5,
            requested_at: Timestamp::from_micros_since_unix_epoch(5_000_000),
            ..first.clone()
        }));
        assert_eq!(
            roster.latest_trace(&target).map(|trace| trace.trace_id),
            Some(2)
        );
        roster.apply_trace(RowChange::Update {
            old: PlayerTrace {
                trace_id: 2,
                times_joined: 5,
                requested_at: Timestamp::from_micros_since_unix_epoch(5_000_000),
                ..first.clone()
            },
            new: PlayerTrace {
                trace_id: 2,
                times_joined: 6,
                requested_at: Timestamp::from_micros_since_unix_epoch(5_000_000),
                ..first.clone()
            },
        });
        assert_eq!(
            roster.latest_trace(&target).map(|trace| trace.times_joined),
            Some(6)
        );

        // Deletes fall back to the previous answer, then to no answer.
        roster.apply_trace(RowChange::Delete(PlayerTrace {
            trace_id: 2,
            ..first.clone()
        }));
        assert_eq!(
            roster.latest_trace(&target).map(|trace| trace.trace_id),
            Some(1)
        );
        roster.apply_trace(RowChange::Delete(first));
        assert!(roster.latest_trace(&target).is_none());
        let json = roster.trace_json(&target);
        assert!(json.get("server").is_none(), "no server answer -> no key");
        roster.apply_trace(RowChange::Insert(PlayerTrace {
            trace_id: 3,
            requested_by: identity(1),
            target: identity(2),
            match_id: None,
            ip: None,
            locale: None,
            times_joined: 0,
            times_kicked: 0,
            ips: Vec::new(),
            names: Vec::new(),
            requested_at: Timestamp::UNIX_EPOCH,
        }));
        roster.clear();
        assert!(roster.latest_trace(&target).is_none(), "clear drops traces");
    }
}

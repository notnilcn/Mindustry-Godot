// SPDX-License-Identifier: GPL-3.0-only

//! Chat rows: the `my_match_chat` view and the client chat log (GAPS #33,
//! PAR-0125).
//!
//! Ported from Mindustry `core/src/mindustry/ui/fragments/ChatFragment.java`
//! (`addMessage`, `messagesShown`, `clearMessages`) and
//! `core/src/mindustry/core/NetServer.java` (`sendChatMessage`: `/t` team chat,
//! `/a` admin channel). The visibility predicate mirrors the module's
//! `my_match_chat` view (`server/spacetimedb/src/chat/mod.rs::chat_visible`):
//! `All` + `System` for every member, `Team` for the sender's team, `Admin`
//! only for admins, and a private (`target`) row only for its target.
//!
//! The view already filters server-side. This layer repeats the predicate so a
//! row cached across a resync, an admin-flag revoke or a team switch can never
//! render for the wrong viewer.

use std::collections::BTreeMap;

use spacetimedb_sdk::Identity;

use crate::binder::RowChange;
use crate::module_bindings::{ChatKind, MatchChat};

/// Maximum number of chat lines retained client-side.
///
/// Upstream `ChatFragment` keeps every message of the session; the relay view
/// is retention-bounded and a long session should not grow the mirror without
/// bound, so the log keeps the newest [`Self::limit`] lines (documented
/// deviation, R13).
pub const CHAT_HISTORY_LIMIT: usize = 512;

/// Stable channel name (`all`/`team`/`system`/`admin`) for JSON and UI labels.
pub fn kind_name(kind: ChatKind) -> &'static str {
    match kind {
        ChatKind::All => "all",
        ChatKind::Team => "team",
        ChatKind::System => "system",
        ChatKind::Admin => "admin",
    }
}

/// Whether `kind` is the admin-only channel (`/a`, GAPS #33).
pub fn is_admin_channel(kind: ChatKind) -> bool {
    matches!(kind, ChatKind::Admin)
}

/// One chat row projected for the client mirror.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    /// Auto-inc message id (`MatchChat.chat_id`).
    pub chat_id: u64,
    /// Owning match.
    pub match_id: u64,
    /// Author identity hex (64 lowercase hex chars).
    pub sender: String,
    /// Channel.
    pub kind: ChatKind,
    /// Team id for `Team` chat.
    pub team: Option<u8>,
    /// Sanitized text.
    pub text: String,
    /// Server receive time (micros since the Unix epoch).
    pub sent_at_micros: i64,
    /// Private-delivery target hex (`None` = the whole channel).
    pub target: Option<String>,
}

impl ChatLine {
    /// Projects one generated row (never filters; use [`ChatLog::apply`]).
    pub fn new(row: &MatchChat) -> Self {
        Self {
            chat_id: row.chat_id,
            match_id: row.match_id,
            sender: row.sender.to_hex().to_string(),
            kind: row.kind,
            team: row.team,
            text: row.text.clone(),
            sent_at_micros: row.sent_at.to_micros_since_unix_epoch(),
            target: row.target.map(|target| target.to_hex().to_string()),
        }
    }

    /// Whether this row is on the admin-only channel (`ChatKind::Admin`).
    pub fn admin_only(&self) -> bool {
        is_admin_channel(self.kind)
    }

    /// Whether this row is addressed to one identity (`MatchChat.target`).
    pub fn private(&self) -> bool {
        self.target.is_some()
    }

    /// Whether `viewer` may see this row.
    ///
    /// `viewer` is the local identity hex, `viewer_team` the viewer's assigned
    /// team and `viewer_admin` whether the viewer holds a global admin record.
    /// Private rows win over the channel: only the target sees them.
    pub fn visible_to(
        &self,
        viewer: Option<&str>,
        viewer_team: Option<u8>,
        viewer_admin: bool,
    ) -> bool {
        if let Some(target) = self.target.as_deref() {
            return viewer.is_some_and(|viewer| viewer.eq_ignore_ascii_case(target));
        }
        match self.kind {
            ChatKind::All | ChatKind::System => true,
            ChatKind::Team => self.team.is_some() && self.team == viewer_team,
            ChatKind::Admin => viewer_admin,
        }
    }

    /// Stable JSON line for the chat fragment / console.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.chat_id,
            "matchId": self.match_id,
            "sender": self.sender,
            "kind": kind_name(self.kind),
            "team": self.team,
            "text": self.text,
            "sentAt": self.sent_at_micros,
            "target": self.target,
            "admin": self.admin_only(),
            "private": self.private(),
        })
    }
}

/// Ordered client mirror of the `my_match_chat` view.
///
/// [`Self::apply`] drops rows the viewer may not see, so the same log serves
/// admins (`Admin` + private notices) and regular members (`All`/`System`/own
/// `Team` only). [`Self::set_viewer`] re-filters the retained lines when the
/// viewer's team or admin flag changes.
#[derive(Debug, Clone)]
pub struct ChatLog {
    viewer: Option<String>,
    viewer_team: Option<u8>,
    viewer_admin: bool,
    limit: usize,
    lines: BTreeMap<u64, ChatLine>,
}

impl Default for ChatLog {
    fn default() -> Self {
        Self {
            viewer: None,
            viewer_team: None,
            viewer_admin: false,
            limit: CHAT_HISTORY_LIMIT,
            lines: BTreeMap::new(),
        }
    }
}

impl ChatLog {
    /// Empty log with the default history limit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty log retaining at most `limit` lines (clamped to at least one).
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            ..Self::default()
        }
    }

    /// Updates the viewer used to filter incoming rows.
    ///
    /// Retained lines are re-filtered immediately, so an admin revoke or a team
    /// switch drops lines the viewer may no longer see. Rows that become
    /// visible are re-sent by the module's view and arrive through
    /// [`Self::apply`] (the log never resurrects a dropped row on its own).
    pub fn set_viewer(&mut self, viewer: Option<Identity>, team: Option<u8>, admin: bool) {
        self.viewer = viewer.map(|identity| identity.to_hex().to_string());
        self.viewer_team = team;
        self.viewer_admin = admin;
        self.retain_visible();
    }

    /// Local identity hex, if a viewer was set.
    pub fn viewer(&self) -> Option<&str> {
        self.viewer.as_deref()
    }

    /// Viewer team, if set.
    pub fn viewer_team(&self) -> Option<u8> {
        self.viewer_team
    }

    /// Whether the viewer is treated as an admin.
    pub fn viewer_admin(&self) -> bool {
        self.viewer_admin
    }

    /// History limit.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Applies one `my_match_chat` change, keeping only rows the viewer sees.
    ///
    /// An update or insert of a now-invisible row removes any stale line (the
    /// server view emits a delete in that case; this covers a racing cache).
    pub fn apply(&mut self, change: RowChange<MatchChat>) {
        match change {
            RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                let line = ChatLine::new(&row);
                if self.visible(&line) {
                    self.lines.insert(line.chat_id, line);
                    self.trim();
                } else {
                    self.lines.remove(&line.chat_id);
                }
            }
            RowChange::Delete(row) => {
                self.lines.remove(&row.chat_id);
            }
        }
    }

    /// Whether `line` is visible to the current viewer.
    pub fn visible(&self, line: &ChatLine) -> bool {
        line.visible_to(self.viewer.as_deref(), self.viewer_team, self.viewer_admin)
    }

    /// One retained line by id.
    pub fn line(&self, chat_id: u64) -> Option<&ChatLine> {
        self.lines.get(&chat_id)
    }

    /// Retained lines in chronological (`chat_id`) order.
    pub fn lines(&self) -> Vec<&ChatLine> {
        self.lines.values().collect()
    }

    /// The newest `count` lines in chronological order (`ChatFragment` shows
    /// the last `messagesShown` messages).
    pub fn recent(&self, count: usize) -> Vec<&ChatLine> {
        let skip = self.lines.len().saturating_sub(count);
        self.lines.values().skip(skip).collect()
    }

    /// Greatest retained `chat_id`, if any.
    pub fn last_id(&self) -> Option<u64> {
        self.lines.keys().next_back().copied()
    }

    /// Retained line count.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether no line is retained.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Drops every retained line (disconnect/resync).
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// The log as a JSON array (`ChatFragment` render input).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::Value::Array(self.lines.values().map(ChatLine::to_json).collect())
    }

    fn retain_visible(&mut self) {
        let viewer = self.viewer.clone();
        let team = self.viewer_team;
        let admin = self.viewer_admin;
        self.lines
            .retain(|_, line| line.visible_to(viewer.as_deref(), team, admin));
    }

    fn trim(&mut self) {
        while self.lines.len() > self.limit {
            let Some(oldest) = self.lines.keys().next().copied() else {
                break;
            };
            self.lines.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use spacetimedb_sdk::Timestamp;

    fn identity(byte: u8) -> Identity {
        Identity::from_byte_array([byte; 32])
    }

    fn row(
        chat_id: u64,
        sender: u8,
        kind: ChatKind,
        team: Option<u8>,
        target: Option<u8>,
    ) -> MatchChat {
        MatchChat {
            chat_id,
            match_id: 7,
            sender: identity(sender),
            kind,
            team,
            text: format!("line {chat_id}"),
            sent_at: Timestamp::from_micros_since_unix_epoch(chat_id as i64 * 1_000_000),
            target: target.map(identity),
        }
    }

    #[test]
    fn kind_names_cover_every_channel() {
        assert_eq!(kind_name(ChatKind::All), "all");
        assert_eq!(kind_name(ChatKind::Team), "team");
        assert_eq!(kind_name(ChatKind::System), "system");
        assert_eq!(kind_name(ChatKind::Admin), "admin");
        assert!(is_admin_channel(ChatKind::Admin));
        assert!(!is_admin_channel(ChatKind::All));
        assert!(!is_admin_channel(ChatKind::System));
    }

    #[test]
    fn admin_channel_is_admin_only() {
        let mut log = ChatLog::new();
        log.apply(RowChange::Insert(row(1, 2, ChatKind::Admin, None, None)));
        log.apply(RowChange::Insert(row(2, 2, ChatKind::All, None, None)));
        assert_eq!(log.len(), 1, "no viewer: Admin hidden, All passes");
        assert_eq!(log.lines()[0].chat_id, 2);

        log.set_viewer(Some(identity(9)), Some(1), false);
        assert_eq!(log.len(), 1);

        // Gaining admin makes the view re-send the admin rows.
        log.set_viewer(Some(identity(9)), Some(1), true);
        log.apply(RowChange::Insert(row(1, 2, ChatKind::Admin, None, None)));
        assert_eq!(
            log.lines()
                .iter()
                .map(|line| line.chat_id)
                .collect::<Vec<_>>(),
            vec![1, 2],
            "admin lines appear once the viewer is an admin"
        );
        assert!(log.lines()[0].admin_only());

        // Losing admin drops the retained admin lines (view re-filter).
        log.set_viewer(Some(identity(9)), Some(1), false);
        assert_eq!(log.len(), 1);
        assert_eq!(log.last_id(), Some(2));
    }

    #[test]
    fn private_rows_only_reach_their_target() {
        let mut log = ChatLog::new();
        log.set_viewer(Some(identity(2)), None, false);
        // A private system notice for identity 2.
        log.apply(RowChange::Insert(row(
            1,
            2,
            ChatKind::System,
            None,
            Some(2),
        )));
        // A private notice for someone else.
        log.apply(RowChange::Insert(row(
            2,
            3,
            ChatKind::System,
            None,
            Some(3),
        )));
        assert_eq!(log.len(), 1);
        assert_eq!(log.last_id(), Some(1));
        let line = log.line(1).expect("targeted row retained");
        assert!(line.private());
        assert_eq!(line.target.as_deref(), Some(identity(2).to_hex().as_str()));

        // Another viewer never sees the private row, not even an admin.
        log.set_viewer(Some(identity(3)), None, true);
        assert_eq!(log.len(), 0);
        // A viewer without an identity cannot receive private rows either.
        log.set_viewer(None, None, true);
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn team_channel_requires_a_matching_team() {
        let mut log = ChatLog::new();
        log.set_viewer(Some(identity(2)), Some(1), false);
        log.apply(RowChange::Insert(row(1, 2, ChatKind::Team, Some(1), None)));
        log.apply(RowChange::Insert(row(2, 3, ChatKind::Team, Some(2), None)));
        log.apply(RowChange::Insert(row(3, 3, ChatKind::Team, None, None)));
        assert_eq!(log.len(), 1);
        assert_eq!(log.lines()[0].chat_id, 1);

        // Moving teams re-filters the retained history; the view re-sends the
        // rows that are now visible.
        log.set_viewer(Some(identity(2)), Some(2), false);
        assert_eq!(log.len(), 0, "team-1 line is re-filtered out");
        log.apply(RowChange::Insert(row(2, 3, ChatKind::Team, Some(2), None)));
        assert_eq!(log.len(), 1);
        assert_eq!(log.lines()[0].chat_id, 2);
    }

    #[test]
    fn log_orders_dedupes_and_bounds() {
        let mut log = ChatLog::with_limit(3);
        log.set_viewer(Some(identity(2)), None, true);
        log.apply(RowChange::Insert(row(3, 2, ChatKind::All, None, None)));
        log.apply(RowChange::Insert(row(1, 2, ChatKind::All, None, None)));
        log.apply(RowChange::Insert(row(2, 2, ChatKind::System, None, None)));
        assert_eq!(
            log.lines()
                .iter()
                .map(|line| line.chat_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            log.recent(2)
                .iter()
                .map(|line| line.chat_id)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );

        // Re-inserting the same id updates in place (no duplicate).
        log.apply(RowChange::Insert(row(2, 9, ChatKind::All, None, None)));
        assert_eq!(log.len(), 3);
        assert_eq!(
            log.line(2).map(|line| line.sender.as_str()),
            Some(identity(9).to_hex().as_str())
        );

        // Newest beyond the limit evicts the oldest.
        log.apply(RowChange::Insert(row(4, 2, ChatKind::All, None, None)));
        assert_eq!(log.len(), 3);
        assert_eq!(
            log.lines()
                .iter()
                .map(|line| line.chat_id)
                .collect::<Vec<_>>(),
            vec![2, 3, 4]
        );

        // Deletes remove the row; the JSON mirrors the retained set.
        log.apply(RowChange::Delete(row(3, 2, ChatKind::All, None, None)));
        let json = log.to_json();
        assert_eq!(json.as_array().map(Vec::len), Some(2));
        assert_eq!(json[0]["id"], 2);
        assert_eq!(json[0]["kind"], "all");
        assert_eq!(json[0]["private"], false);
        assert_eq!(json[1]["id"], 4);
        assert_eq!(json[0]["sender"], identity(9).to_hex().to_string());
        log.clear();
        assert!(log.is_empty());
    }
}

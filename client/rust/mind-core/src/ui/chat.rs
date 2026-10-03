// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/ChatFragment.java (plan 14 M7 §3.5).

//! Chat state machine + validation (plan 14 M7).
//!
//! Godot-free port of `ChatFragment`'s non-rendering behaviour: the
//! normal/team/admin modes and their normalized prefixes, the ten-message
//! rolling buffer, history navigation and the `x,y [text]` ping parser. The
//! actual transport is plan 21's relay; the fragment calls
//! `MindUi.chat_send(...)` guarded by `has_method` and this module validates /
//! formats first (plan 14 §3.9).

/// `ChatFragment.messagesShown`.
pub const MESSAGES_SHOWN: usize = 10;

/// `ChatMode` (declaration order is the mode-cycle ABI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMode {
    /// `normal("")`.
    Normal,
    /// `team("/t")`.
    Team,
    /// `admin("/a", player::admin)`.
    Admin,
}

impl ChatMode {
    /// Every mode in declaration order.
    pub const ALL: [ChatMode; 3] = [ChatMode::Normal, ChatMode::Team, ChatMode::Admin];

    /// Raw command prefix.
    pub const fn prefix(self) -> &'static str {
        match self {
            ChatMode::Normal => "",
            ChatMode::Team => "/t",
            ChatMode::Admin => "/a",
        }
    }

    /// `normalizedPrefix` (prefix plus a space when non-empty).
    pub fn normalized_prefix(self) -> &'static str {
        match self {
            ChatMode::Normal => "",
            ChatMode::Team => "/t ",
            ChatMode::Admin => "/a ",
        }
    }

    /// `next()` (wraps).
    pub fn next(self) -> ChatMode {
        let index = ChatMode::ALL
            .iter()
            .position(|mode| *mode == self)
            .unwrap_or(0);
        ChatMode::ALL[(index + 1) % ChatMode::ALL.len()]
    }

    /// `isValid()` — admin requires the flag.
    pub fn is_valid(self, admin: bool) -> bool {
        self != ChatMode::Admin || admin
    }
}

/// A parsed ping location (`x,y [text]`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Ping {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Trailing ping text.
    pub text: String,
}

/// A validated outgoing chat message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSend {
    /// Upstream `UI.formatIcons` output is applied by the caller; this is the
    /// trimmed raw message.
    pub message: String,
    /// Parsed ping, when the message carried an in-bounds `x,y [text]`.
    pub ping: Option<Ping>,
}

/// Chat fragment state (rolling messages + input history).
#[derive(Debug, Clone, PartialEq)]
pub struct ChatState {
    /// Newest-first message buffer (`messages`).
    pub messages: Vec<String>,
    /// Newest-first input history (`history`, index 0 is the live input).
    pub history: Vec<String>,
    /// Current history cursor.
    pub history_pos: usize,
    /// Scroll position from the newest message.
    pub scroll_pos: usize,
    /// Active mode.
    pub mode: ChatMode,
    /// Fade timer in message-count units (`fadetime`).
    pub fadetime: f32,
    /// Input visible.
    pub shown: bool,
}

impl Default for ChatState {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatState {
    /// Empty state with the `[""]` history seeding (`history.insert(0, "")`).
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            history: vec![String::new()],
            history_pos: 0,
            scroll_pos: 0,
            mode: ChatMode::Normal,
            fadetime: 0.0,
            shown: false,
        }
    }

    /// `clearMessages` (messages + history reset).
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.history.clear();
        self.history = vec![String::new()];
    }

    /// `addMessage` (insert at front + fade bookkeeping).
    pub fn add_message(&mut self, message: impl Into<String>) {
        self.messages.insert(0, message.into());
        self.fadetime = (self.fadetime + 1.0).min(MESSAGES_SHOWN as f32) + 1.0;
        if self.scroll_pos > 0 {
            self.scroll_pos += 1;
        }
    }

    /// `sendMessage` — trims, applies the empty/prefix guard, records history
    /// and parses a ping. `map` bounds gate the ping like `world.tiles.in`.
    pub fn send(&mut self, raw: &str, admin: bool, map: (i32, i32)) -> Option<ChatSend> {
        let message = raw.trim().to_owned();
        self.clear_input();
        if message.is_empty()
            || (message.starts_with(self.mode.prefix())
                && message[self.mode.prefix().len()..].is_empty())
        {
            return None;
        }
        if self.history.len() < 2
            || self.history.get(1).map(String::as_str) != Some(message.as_str())
        {
            self.history.insert(1, message.clone());
        }
        let ping = check_ping(&message, map.0, map.1);
        let _ = admin;
        Some(ChatSend { message, ping })
    }

    /// `nextMode` — cycles to the next valid mode and rewrites the prefix.
    pub fn next_mode(&mut self, admin: bool, input: &str) -> String {
        let previous = self.mode;
        let mut mode = self.mode;
        loop {
            mode = mode.next();
            if mode.is_valid(admin) {
                break;
            }
        }
        self.mode = mode;
        let previous_prefix = previous.normalized_prefix();
        if let Some(rest) = input.strip_prefix(previous_prefix) {
            format!("{}{}", mode.normalized_prefix(), rest)
        } else {
            mode.normalized_prefix().to_owned()
        }
    }

    /// `clearChatInput`.
    pub fn clear_input(&mut self) {
        self.history_pos = 0;
        if self.history.is_empty() {
            self.history.push(String::new());
        }
        self.history[0] = String::new();
    }

    /// `updateChat` — the history text for the current cursor.
    pub fn history_text(&self) -> String {
        format!(
            "{}{}",
            self.mode.normalized_prefix(),
            self.history
                .get(self.history_pos)
                .map(String::as_str)
                .unwrap_or("")
        )
    }

    /// `chatHistoryPrev` key handling.
    pub fn history_prev(&mut self, current: &str) -> bool {
        if self.history_pos < self.history.len().saturating_sub(1) {
            if self.history_pos == 0 {
                self.history[0] = current.to_owned();
            }
            self.history_pos += 1;
            true
        } else {
            false
        }
    }

    /// `chatHistoryNext` key handling.
    pub fn history_next(&mut self) -> bool {
        if self.history_pos > 0 {
            self.history_pos -= 1;
            true
        } else {
            false
        }
    }

    /// Scroll clamp (max = `messages - MESSAGES_SHOWN`).
    pub fn scroll(&mut self, delta: i32) {
        let max = self.messages.len().saturating_sub(MESSAGES_SHOWN) as i32;
        self.scroll_pos = (self.scroll_pos as i32 + delta).clamp(0, max) as usize;
    }
}

/// `checkPing` — parses `x,y [text]` (with the space-after-comma variant) and
/// returns it only when the tile is inside the map.
pub fn check_ping(message: &str, map_w: i32, map_h: i32) -> Option<Ping> {
    let comma = message.find(',')?;
    let mut space = message[comma + 1..]
        .find(' ')
        .map(|index| index + comma + 1);
    let mut extra = false;
    if space == Some(comma + 1) {
        extra = true;
        space = message[comma + 2..]
            .find(' ')
            .map(|index| index + comma + 2);
    }
    let space = space?;
    let x = message[..comma].trim().parse::<i32>().ok()?;
    let y_start = comma + 1 + if extra { 1 } else { 0 };
    let y = message[y_start..space].trim().parse::<i32>().ok()?;
    if x < 0 || y < 0 || x >= map_w || y >= map_h {
        return None;
    }
    Some(Ping {
        x,
        y,
        text: message[space..].trim().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_cycle_skips_admin_without_permission() {
        let mut state = ChatState::new();
        let mut input = String::new();
        let next = state.next_mode(false, &input);
        assert_eq!(state.mode, ChatMode::Team);
        assert_eq!(next, "/t ");
        input = next;
        let next = state.next_mode(false, &input);
        assert_eq!(state.mode, ChatMode::Normal, "admin skipped");
        assert_eq!(next, "");

        let mut admin = ChatState::new();
        assert_eq!(admin.next_mode(true, ""), "/t ");
        assert_eq!(admin.next_mode(true, "/t "), "/a ");
        assert_eq!(admin.next_mode(true, "/a "), "");
    }

    #[test]
    fn send_rejects_prefix_only_and_empty() {
        let mut state = ChatState::new();
        assert!(state.send("", false, (100, 100)).is_none());
        assert!(state.send("   ", false, (100, 100)).is_none());
        state.mode = ChatMode::Team;
        assert!(state.send("/t", false, (100, 100)).is_none(), "prefix-only");
        assert!(state.send("/t hi", false, (100, 100)).is_some());
    }

    #[test]
    fn send_records_history_and_message() {
        let mut state = ChatState::new();
        let sent = state.send("hello", false, (100, 100)).unwrap();
        assert_eq!(sent.message, "hello");
        assert_eq!(state.history[1], "hello");
        // Duplicate not re-inserted.
        state.send("hello", false, (100, 100));
        assert_eq!(state.history.len(), 2);
    }

    #[test]
    fn ping_parses_and_bounds_checks() {
        let ping = check_ping("12,34 [help]", 100, 100).unwrap();
        assert_eq!((ping.x, ping.y), (12, 34));
        assert_eq!(ping.text, "[help]");
        // Space after comma variant.
        let ping = check_ping("12, 34 [help]", 100, 100).unwrap();
        assert_eq!((ping.x, ping.y), (12, 34));
        // Out of bounds.
        assert!(check_ping("200,10 [x]", 100, 100).is_none());
        // No comma.
        assert!(check_ping("hello", 100, 100).is_none());
    }

    #[test]
    fn history_navigation_clamps() {
        let mut state = ChatState::new();
        state.send("a", false, (1, 1));
        state.send("b", false, (1, 1));
        assert!(state.history_prev("live"));
        assert_eq!(state.history[0], "live");
        assert!(state.history_prev("x"));
        assert!(!state.history_prev("x"), "at oldest");
        assert!(state.history_next());
        assert!(state.history_next());
        assert!(!state.history_next());
    }

    #[test]
    fn messages_are_newest_first() {
        let mut state = ChatState::new();
        state.add_message("first");
        state.add_message("second");
        assert_eq!(state.messages, vec!["second", "first"]);
    }
}

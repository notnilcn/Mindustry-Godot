// SPDX-License-Identifier: GPL-3.0-only

//! Player-action dispatch, server calls and the chat/event drains.
//! Upstream: core/src/mindustry/ui/fragments/PlayerListFragment.java

use super::adapters::{relay_trace_info, resolve_action_target};
use super::*;

impl MindNet {
    /// Maps a player-list action name to the plan-21 reducer and sends it
    /// (`PlayerListFragment` Ban/Kick/Trace/Team/Admin buttons, `PlayerListFragment.java:149-247`).
    pub(super) fn dispatch_player_action(
        &mut self,
        action: GString,
        target: GString,
        value: i64,
    ) -> bool {
        let target = target.to_string();
        // GAPS #34/#39: the roster dialogs act on a `PlayerInfo.id` identity
        // (`BansDialog`'s `unbanPlayerID`, `AdminsDialog`'s `unAdminPlayer`),
        // and a banned/unadmin'd identity is usually offline (no
        // `relay_member` row), so a full identity hex must resolve directly
        // instead of through `Roster::find`.
        let Some(identity_hex) = resolve_action_target(&self.roster, target.as_str()) else {
            log::warn!("player_action: no player matching `{target}`");
            return false;
        };
        let Ok(identity) = mind_stdb::Identity::from_hex(&identity_hex) else {
            log::warn!("player_action: bad identity `{identity_hex}`");
            return false;
        };
        let Some(match_id) = self.session.match_id() else {
            log::warn!("player_action: no match hosted");
            return false;
        };
        let action = match action.to_string().as_str() {
            "kick" => PlayerAction::Kick {
                reason: "Kicked by an admin".to_owned(),
            },
            "ban" => PlayerAction::Ban {
                reason: "Banned by an admin".to_owned(),
                duration_secs: None,
            },
            "unban" => PlayerAction::Unban,
            "admin" => PlayerAction::GrantAdmin(true),
            "unadmin" => PlayerAction::GrantAdmin(false),
            "whitelist" => PlayerAction::Whitelist(true),
            "unwhitelist" => PlayerAction::Whitelist(false),
            "team" => PlayerAction::SwitchTeam(value.clamp(0, u8::MAX as i64) as u8),
            "trace" => PlayerAction::RequestTrace,
            other => {
                log::warn!("player_action: unknown action `{other}`");
                return false;
            }
        };
        let Some(connector) = self.connector.as_mut() else {
            log::warn!("player_action: no connector (single-player)");
            return false;
        };
        let kick_reason = match &action {
            PlayerAction::Kick { reason } => Some(reason.clone()),
            _ => None,
        };
        match action.apply(connector, match_id, identity) {
            Ok(()) => {
                // GAPS #35: Java's `adminRequest(kick)` sends
                // `Call.kick(other.con, reason)`; publish the same call so the
                // target's client shows the localized kick, not just `my_kick`.
                if let Some(reason) = kick_reason {
                    self.publish_call_packet(
                        match_id,
                        CallPacketKind::Kick,
                        Some(identity),
                        &CallPacket::Kick {
                            reason: Some(reason),
                        },
                    );
                }
                // PAR-0601: `adminRequest(trace)` answers the requesting admin
                // with `Call.traceInfo(player.con, other, info)`
                // (`NetServer.java:941`); the trace payload is the roster
                // aggregation plus the module's `my_traces` answer.
                if matches!(action, PlayerAction::RequestTrace) {
                    self.publish_trace_call(&identity_hex);
                }
                true
            }
            Err(error) => {
                log::warn!("player_action {} failed: {error}", action.reducer());
                false
            }
        }
    }

    /// Publishes `Call.traceInfo` to the requesting admin (PAR-0601).
    ///
    /// The relay has no arcnet return path, so the admin's own client receives
    /// the call through `my_match_calls` (the target is the local identity, as
    /// in Java, where `player.con` is the requester). The payload is the
    /// roster's aggregated trace; the module's `my_traces` row still carries
    /// the server-computed history (`Roster::trace_json`).
    pub(super) fn publish_trace_call(&mut self, identity_hex: &str) {
        let Some(match_id) = self.session.match_id() else {
            return;
        };
        let Some(me) = self.session.local_identity() else {
            return;
        };
        let info = relay_trace_info(&self.roster.trace(identity_hex));
        self.publish_call_packet(
            match_id,
            CallPacketKind::TraceInfo,
            Some(me),
            &CallPacket::TraceInfo { player: None, info },
        );
    }

    /// Drains the call channel every frame (GAPS #35): effects the net layer
    /// owns are applied immediately (a kick must not wait for a UI poll) and
    /// every applied call is retained for [`Self::server_calls_json`].
    pub(super) fn pump_server_calls(&mut self) {
        let calls = self.calls.drain();
        if calls.is_empty() {
            return;
        }
        for call in calls {
            let effect = CallBridge::classify(&call);
            if let Some(effect) = &effect {
                self.apply_call_effect(effect);
            }
            self.pending_calls.push(serde_json::json!({
                "call": call.to_json(),
                "effect": effect_json(effect.as_ref()),
            }));
        }
        let limit = mind_stdb::CALL_QUEUE_LIMIT;
        if self.pending_calls.len() > limit {
            let excess = self.pending_calls.len() - limit;
            self.pending_calls.drain(..excess);
        }
    }

    /// Applies one classified call effect (GAPS #35). Kick shows the reason and
    /// leaves the match; the other effects are data for their owners.
    pub(super) fn apply_call_effect(&mut self, effect: &CallEffect) {
        match effect {
            CallEffect::Kick { key, detail, quiet } => {
                self.kicked = Some(if detail.is_empty() {
                    key.clone()
                } else {
                    format!("{key}: {detail}")
                });
                log::warn!("MindNet: kicked from the match: {key} {detail}");
                if !*quiet {
                    self.show_error(&format!("@{key}"), detail);
                }
                if self.session.match_id().is_some()
                    && let Some(connector) = self.connector.as_mut()
                    && let Err(error) = self.session.leave(connector)
                {
                    log::warn!("MindNet: leave after kick failed: {error}");
                }
            }
            CallEffect::Camera { x, y } => {
                log::debug!("MindNet: server camera pan to ({x}, {y})");
            }
            CallEffect::InfoMessage { message } => log::info!("MindNet: {message}"),
            CallEffect::PlayMusic { music, interrupt } => {
                log::debug!("MindNet: server music `{music}` (interrupt={interrupt})");
            }
        }
    }

    /// Applies every pending `my_match_events` row to the client `EventType`
    /// bus. Rows are deduplicated by `event_id` inside [`EventBus`], so a
    /// reconnect snapshot replay is harmless.
    pub(super) fn drain_server_events(&mut self) {
        let Some(changes) = self.events_binder.as_ref().map(|binder| binder.drain()) else {
            return;
        };
        for change in changes {
            self.events.apply(change);
        }
    }

    /// Applies every pending `my_match_chat` row to the chat mirror
    /// (PAR-0121/0125). [`ChatLog::apply`] drops rows the current viewer may
    /// not see, so the same log serves members and admins.
    pub(super) fn drain_server_chat(&mut self) {
        let Some(changes) = self.chat_binder.as_ref().map(|binder| binder.drain()) else {
            return;
        };
        for change in changes {
            self.chat.apply(change);
        }
    }

    /// Keeps the chat viewer in sync with the local identity, team and admin
    /// flag (`ChatLog::set_viewer` re-filters already-retained lines on a team
    /// switch or an admin grant/revoke).
    pub(super) fn refresh_chat_viewer(&mut self) {
        let identity = self.session.local_identity();
        let admin = self.session.is_host()
            || identity.is_some_and(|identity| self.roster.is_admin(&identity.to_hex()));
        let team = u8::try_from(self.my_team()).ok();
        let identity_hex = identity.map(|identity| identity.to_hex().to_string());
        if self.chat.viewer() == identity_hex.as_deref()
            && self.chat.viewer_team() == team
            && self.chat.viewer_admin() == admin
        {
            return;
        }
        self.chat.set_viewer(identity, team, admin);
    }

    /// Applies every pending `my_match_calls` row (GAPS #35 / PAR-0576).
    ///
    /// Each row becomes a `[kind][body]` frame pushed into the same
    /// [`CallBridge`] `MindNet.push_call_frame` feeds, so decoding, rejection
    /// accounting and the net-layer effects (kick/leave, camera, music,
    /// infoMessage) are shared by both entry points. [`CallStream`] dedupes by
    /// `call_id`, so a reconnect replay never re-applies a call.
    pub(super) fn drain_server_calls(&mut self) {
        let Some(changes) = self.call_rows_binder.as_ref().map(|binder| binder.drain()) else {
            return;
        };
        for change in changes {
            self.call_rows.apply(change);
        }
        for (call_id, frame) in self.call_rows.drain_new() {
            if !self.calls.push_frame(&frame) {
                log::warn!(
                    "MindNet: rejected published call {call_id}: {}",
                    self.calls.rejects_json()
                );
            }
        }
    }

    /// Publishes one server→client call through the module (host/admin,
    /// GAPS #35 / PAR-0576): Java `adminRequest(kick)` ends with
    /// `Call.kick(other.con, ...)`, so the relay's kick path publishes the
    /// matching `Kick` frame for the target.
    pub(super) fn publish_call_packet(
        &mut self,
        match_id: u64,
        kind: CallPacketKind,
        target: Option<mind_stdb::Identity>,
        packet: &CallPacket,
    ) -> bool {
        let Ok(payload) = packet.encode_body() else {
            log::warn!("publish_call_packet: cannot encode {packet:?}");
            return false;
        };
        self.with_connector(|connector| connector.publish_call(match_id, kind, target, payload))
            .map(|result| match result {
                Ok(()) => true,
                Err(error) => {
                    log::warn!("publish_call failed: {error}");
                    false
                }
            })
            .unwrap_or(false)
    }
}

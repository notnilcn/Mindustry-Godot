// SPDX-License-Identifier: GPL-3.0-only

//! Server→client `@Remote` call rows → `CallPacket` frames (GAPS #35 /
//! PAR-0576).
//!
//! The module publishes one private `match_call` row per server→client call
//! (`server/spacetimedb/src/relay/calls.rs`); `my_match_calls` exposes the
//! caller's broadcast + targeted rows. This module is the client transport half
//! the module cannot own: it maps the module's append-only
//! [`CallPacketKind`] ABI onto the client wire ordinals
//! ([`crate::call_packet::CallKind`]), prepends the kind byte and hands the
//! resulting `[kind][body]` frame to [`crate::call_packet::CallChannel`] (the
//! `MindNet.push_call_frame` seam).
//!
//! Java parity: the `Call.*` methods are generated from the `@Remote`
//! annotation over arcnet (`core/src/mindustry/core/NetServer.java`, packet
//! layouts in `core/build/generated/.../mindustry/gen`); SpacetimeDB replaces
//! the transport, not the packet bodies. The bodies are byte-for-byte the
//! `*CallPacket.write(...)` payloads, so the module never has to know TypeIO.

use std::collections::BTreeMap;

use crate::binder::RowChange;
use crate::call_packet::{self, CallError, CallKind, CallPacket};
use crate::module_bindings::{CallPacketKind, MatchCall};

/// Maximum call rows retained client-side. The server prunes the table by
/// `CALL_RETENTION_SECS`, so this only bounds a reconnect replay spike.
pub const CALL_ROW_LIMIT: usize = 512;

/// The client wire ordinal for one module call kind.
///
/// Exhaustive on purpose: a new module variant fails to compile here until it is
/// mapped (the module enum and [`CallKind`] are append-only ABIs maintained in
/// two crates; this match is the single synchronization point).
pub fn call_kind(kind: CallPacketKind) -> CallKind {
    match kind {
        CallPacketKind::InfoMessage => CallKind::InfoMessage,
        CallPacketKind::Menu => CallKind::Menu,
        CallPacketKind::FollowUpMenu => CallKind::FollowUpMenu,
        CallPacketKind::HideFollowUpMenu => CallKind::HideFollowUpMenu,
        CallPacketKind::MenuBuilder => CallKind::MenuBuilder,
        CallPacketKind::MenuBuilderUpdate => CallKind::MenuBuilderUpdate,
        CallPacketKind::HideMenuBuilder => CallKind::HideMenuBuilder,
        CallPacketKind::SetCameraPosition => CallKind::SetCameraPosition,
        CallPacketKind::Sound => CallKind::Sound,
        CallPacketKind::SoundAt => CallKind::SoundAt,
        CallPacketKind::Effect => CallKind::Effect,
        CallPacketKind::EffectReliable => CallKind::EffectReliable,
        CallPacketKind::PlayMusic => CallKind::PlayMusic,
        CallPacketKind::TraceInfo => CallKind::TraceInfo,
        CallPacketKind::Label => CallKind::Label,
        CallPacketKind::LabelWithId => CallKind::LabelWithId,
        CallPacketKind::LabelWithFlags => CallKind::LabelWithFlags,
        CallPacketKind::LabelReliable => CallKind::LabelReliable,
        CallPacketKind::LabelReliableWithId => CallKind::LabelReliableWithId,
        CallPacketKind::LabelReliableWithFlags => CallKind::LabelReliableWithFlags,
        CallPacketKind::RemoveWorldLabel => CallKind::RemoveWorldLabel,
        CallPacketKind::Kick => CallKind::Kick,
        CallPacketKind::KickReason => CallKind::KickReason,
    }
}

/// The `[kind][body]` frame for one published call row.
pub fn frame_for_row(row: &MatchCall) -> Vec<u8> {
    let mut frame = Vec::with_capacity(1 + row.payload.len());
    frame.push(call_kind(row.kind).ordinal());
    frame.extend_from_slice(&row.payload);
    frame
}

/// Decodes one row's frame (a malformed/unknown body is rejected, never
/// panics; the same rejection tail `CallChannel` keeps).
pub fn decode_row(row: &MatchCall) -> Result<CallPacket, CallError> {
    call_packet::decode(&frame_for_row(row))
}

/// Ordered mirror of `my_match_calls`.
///
/// Rows are deduplicated by `call_id` (a replayed cache re-inserts the same
/// rows) and retained newest-`limit`. [`Self::drain_new`] replays only the
/// frames a consumer has not applied yet, the same watermark idiom as
/// [`crate::CommandStream`] and [`crate::EventBus`].
#[derive(Debug, Clone)]
pub struct CallStream {
    limit: usize,
    rows: BTreeMap<u64, Vec<u8>>,
    watermark: Option<u64>,
}

impl Default for CallStream {
    fn default() -> Self {
        Self {
            limit: CALL_ROW_LIMIT,
            rows: BTreeMap::new(),
            watermark: None,
        }
    }
}

impl CallStream {
    /// Empty stream with the default retention limit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty stream retaining at most `limit` rows (clamped to at least one).
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            ..Self::default()
        }
    }

    /// Applies one `my_match_calls` change.
    pub fn apply(&mut self, change: RowChange<MatchCall>) {
        match change {
            RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                self.rows.insert(row.call_id, frame_for_row(&row));
                self.trim();
            }
            RowChange::Delete(row) => {
                self.rows.remove(&row.call_id);
            }
        }
    }

    /// Frames with `call_id > watermark`, in commit order; advances the
    /// watermark to the newest drained id.
    pub fn drain_new(&mut self) -> Vec<(u64, Vec<u8>)> {
        let after = self.watermark.unwrap_or(0);
        let drained: Vec<(u64, Vec<u8>)> = self
            .rows
            .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
            .map(|(call_id, frame)| (*call_id, frame.clone()))
            .collect();
        if let Some((last, _)) = drained.last() {
            self.watermark = Some(*last);
        }
        drained
    }

    /// Frames applied by a consumer, in commit order (`call_id > after`).
    pub fn after(&self, after: u64) -> Vec<(u64, &[u8])> {
        self.rows
            .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
            .map(|(call_id, frame)| (*call_id, frame.as_slice()))
            .collect()
    }

    /// One retained frame by id.
    pub fn frame(&self, call_id: u64) -> Option<&[u8]> {
        self.rows.get(&call_id).map(Vec::as_slice)
    }

    /// Greatest retained `call_id`, if any.
    pub fn last_call_id(&self) -> Option<u64> {
        self.rows.keys().next_back().copied()
    }

    /// Consumer watermark (the last drained `call_id`).
    pub fn watermark(&self) -> Option<u64> {
        self.watermark
    }

    /// Rows retained but not yet drained.
    pub fn pending(&self) -> usize {
        let after = self.watermark.unwrap_or(0);
        self.rows
            .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
            .count()
    }

    /// Retained row count.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether no row is retained.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Retention limit.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Drops every retained row and the watermark (disconnect/resync).
    pub fn clear(&mut self) {
        self.rows.clear();
        self.watermark = None;
    }

    /// Stable debug JSON (`[{id, kind}]`, commit order).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.rows
                .iter()
                .map(|(call_id, frame)| {
                    serde_json::json!({
                        "id": call_id,
                        "kind": frame.first().copied().unwrap_or(0),
                        "bytes": frame.len(),
                    })
                })
                .collect(),
        )
    }

    fn trim(&mut self) {
        while self.rows.len() > self.limit {
            let Some(oldest) = self.rows.keys().next().copied() else {
                break;
            };
            self.rows.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::typeio::{KickReason, SoundId};
    use spacetimedb_sdk::{Identity, Timestamp};

    fn row(call_id: u64, kind: CallPacketKind, payload: Vec<u8>) -> MatchCall {
        MatchCall {
            call_id,
            match_id: 3,
            sender: Identity::from_byte_array([2u8; 32]),
            kind,
            target: None,
            payload,
            sent_at: Timestamp::from_micros_since_unix_epoch(call_id as i64 * 1_000),
        }
    }

    /// Every module kind maps to a distinct client ordinal (the append-only
    /// ABIs stay in lockstep).
    #[test]
    fn module_kinds_map_onto_the_client_wire_ordinals() {
        let kinds = [
            CallPacketKind::InfoMessage,
            CallPacketKind::Menu,
            CallPacketKind::FollowUpMenu,
            CallPacketKind::HideFollowUpMenu,
            CallPacketKind::MenuBuilder,
            CallPacketKind::MenuBuilderUpdate,
            CallPacketKind::HideMenuBuilder,
            CallPacketKind::SetCameraPosition,
            CallPacketKind::Sound,
            CallPacketKind::SoundAt,
            CallPacketKind::Effect,
            CallPacketKind::EffectReliable,
            CallPacketKind::PlayMusic,
            CallPacketKind::TraceInfo,
            CallPacketKind::Label,
            CallPacketKind::LabelWithId,
            CallPacketKind::LabelWithFlags,
            CallPacketKind::LabelReliable,
            CallPacketKind::LabelReliableWithId,
            CallPacketKind::LabelReliableWithFlags,
            CallPacketKind::RemoveWorldLabel,
            CallPacketKind::Kick,
            CallPacketKind::KickReason,
        ];
        assert_eq!(kinds.len(), CallKind::ALL.len());
        let mut ordinals = Vec::new();
        for kind in kinds {
            let ordinal = call_kind(kind).ordinal();
            assert!(!ordinals.contains(&ordinal), "{kind:?} maps twice");
            ordinals.push(ordinal);
        }
        assert_eq!(ordinals.len(), 23);
    }

    /// A published row decodes to the exact packet its body was written from.
    #[test]
    fn rows_decode_to_call_packets() {
        let info = CallPacket::InfoMessage {
            message: Some("hello".to_string()),
        };
        let decoded = decode_row(&row(
            1,
            CallPacketKind::InfoMessage,
            info.encode_body().unwrap(),
        ))
        .unwrap();
        assert_eq!(decoded, info);

        let sound = CallPacket::Sound {
            sound: Some(SoundId(4)),
            volume: 0.5,
            pitch: 1.25,
            pan: -0.25,
        };
        let decoded =
            decode_row(&row(2, CallPacketKind::Sound, sound.encode_body().unwrap())).unwrap();
        assert_eq!(decoded, sound);

        // The frame is exactly the packet's own `[kind][body]` encoding.
        let row = row(3, CallPacketKind::SetCameraPosition, {
            CallPacket::SetCameraPosition { x: 8.0, y: 16.0 }
                .encode_body()
                .unwrap()
        });
        assert_eq!(
            frame_for_row(&row),
            CallPacket::SetCameraPosition { x: 8.0, y: 16.0 }
                .encode()
                .unwrap()
        );
    }

    /// Kick rows carry the reason body; the client keeps the reason form.
    #[test]
    fn kick_rows_decode_both_forms() {
        let string_kick = CallPacket::Kick {
            reason: Some("bye".to_string()),
        };
        assert_eq!(
            decode_row(&row(
                4,
                CallPacketKind::Kick,
                string_kick.encode_body().unwrap()
            ))
            .unwrap(),
            string_kick
        );
        let reason_kick = CallPacket::KickReason {
            reason: KickReason::NameInUse,
        };
        assert_eq!(
            decode_row(&row(
                5,
                CallPacketKind::KickReason,
                reason_kick.encode_body().unwrap()
            ))
            .unwrap(),
            reason_kick
        );
    }

    /// A malformed body is a rejection, not a panic (same contract as
    /// `CallChannel`).
    #[test]
    fn malformed_rows_are_rejected() {
        let row = row(6, CallPacketKind::InfoMessage, vec![0xff, 0xff]);
        assert!(decode_row(&row).is_err());
    }

    #[test]
    fn stream_orders_dedupes_and_advances_the_watermark() {
        let mut stream = CallStream::new();
        let packet = CallPacket::InfoMessage {
            message: Some("a".to_string()),
        };
        let body = packet.encode_body().unwrap();
        stream.apply(RowChange::Insert(row(
            3,
            CallPacketKind::InfoMessage,
            body.clone(),
        )));
        stream.apply(RowChange::Insert(row(
            1,
            CallPacketKind::InfoMessage,
            body.clone(),
        )));
        stream.apply(RowChange::Insert(row(
            2,
            CallPacketKind::InfoMessage,
            body.clone(),
        )));
        assert_eq!(stream.len(), 3);
        assert_eq!(stream.pending(), 3);

        // Replayed rows are deduplicated by id.
        stream.apply(RowChange::Insert(row(
            2,
            CallPacketKind::InfoMessage,
            body.clone(),
        )));
        assert_eq!(stream.len(), 3);

        let drained = stream.drain_new();
        assert_eq!(
            drained.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(stream.watermark(), Some(3));
        assert!(stream.drain_new().is_empty());
        assert_eq!(stream.pending(), 0);

        // A late row still replays; delete removes it.
        stream.apply(RowChange::Insert(row(
            4,
            CallPacketKind::InfoMessage,
            body.clone(),
        )));
        assert_eq!(stream.drain_new().len(), 1);
        stream.apply(RowChange::Delete(row(4, CallPacketKind::InfoMessage, body)));
        assert_eq!(stream.len(), 3);
        assert!(stream.frame(4).is_none());
    }

    #[test]
    fn stream_retention_is_bounded() {
        let mut stream = CallStream::with_limit(2);
        let body = CallPacket::InfoMessage {
            message: Some("x".to_string()),
        }
        .encode_body()
        .unwrap();
        for id in 1..=4 {
            stream.apply(RowChange::Insert(row(
                id,
                CallPacketKind::InfoMessage,
                body.clone(),
            )));
        }
        assert_eq!(stream.len(), 2);
        assert!(stream.frame(3).is_some());
        assert!(stream.frame(4).is_some());
        stream.clear();
        assert!(stream.is_empty());
        assert_eq!(stream.watermark(), None);
    }
}

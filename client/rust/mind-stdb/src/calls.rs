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
//!
//! Producer status (PAR-0601): [`ProducedCall`] pairs a packet with the
//! `publish_call` envelope, and the host wires it at the call sites it already
//! triggers:
//! - `infoMessage` — headless `NetHost::info_message` (game over / next map,
//!   `ServerControl.java:116`). The whitelist rejection
//!   (`NetServer.java:233`) happens in the module's join gate, which cannot
//!   publish calls today, so it stays outstanding.
//! - `kick`/`kickReason` — headless `Admins::kick`/`NetHost::kick_all`
//!   (`NetServer.java:453-520`) and `MindNet.player_action("kick")`.
//! - `traceInfo` — `MindNet.player_action("trace")` (`NetServer.java:941`).
//!
//! Remaining Java call sites (no producer yet; the constructors above are
//! ready):
//! - `menu`/`followUpMenu`/`hideFollowUpMenu` — `Menus.java:145-168` are the
//!   client render handlers; core has no producer (mods call them). The port's
//!   menu surface is `gx/ui` (`Menus`/dialog registry) and needs a host publish
//!   seam when it grows one.
//! - `menuBuilder`/`menuBuilderUpdate`/`hideMenuBuilder` —
//!   `MenuBuilder.java:40-50` (`show`/`showAll`/`update`); the builder port is
//!   `gx/ui`.
//! - `setCameraPosition` — only the `NetClient.java:509` handler exists in
//!   core; producers are mods/logic (`LExecutor`).
//! - `sound`/`soundAt`/`effect`/`effectReliable`/`effectData`/`playMusic` —
//!   only `NetClient.java` handlers plus the logic executor
//!   (`LExecutor.java:2278` music, `LExecutor.java` effect data); the logic port
//!   (`mc/logic`) owns those producers. [`ProducedCall::effect_data`] builds
//!   the `EffectCallPacket2` call; the module ABI kind
//!   (`CallPacketKind::EffectData`, appended by PAR-0578) and the mapping exist,
//!   so logic only has to call it.
//! - `label`/`labelReliable`/`removeWorldLabel` — `Menus.label*` are render
//!   handlers; the only core producer is `WorldLabelComp.hide`
//!   (`WorldLabelComp.java:128/135`, removal only), ported under `mc/world`.
//!
//! The join-gate `infoMessage` (`NetServer.java:233`, whitelist rejection)
//! stays outstanding for a structural reason, not a missing constructor: the
//! rejected identity is not a `relay_member` yet, so `publish_call`'s
//! target-membership check rejects it and `my_match_calls` (member semijoin)
//! could not deliver the row anyway. A join-rejection notice needs its own
//! identity-keyed row + view, not a `match_call` row.

use std::collections::BTreeMap;

use crate::Identity;
use crate::binder::RowChange;
use crate::call_packet::{self, CallError, CallKind, CallPacket, EffectDataBody};
use crate::module_bindings::{CallPacketKind, MatchCall};
use crate::typeio::{self, EffectId, KickReason, Rgba, SoundId, TraceInfo};

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
        CallPacketKind::EffectData => CallKind::EffectData,
    }
}

/// The module ABI kind for one client call kind (inverse of [`call_kind`]).
///
/// Used by the host-side producers ([`ProducedCall`]): a producer builds the
/// packet first, this maps it onto the `match_call.kind` column the module
/// stores. Exhaustive on purpose, like [`call_kind`].
pub fn packet_kind(kind: CallKind) -> CallPacketKind {
    match kind {
        CallKind::InfoMessage => CallPacketKind::InfoMessage,
        CallKind::Menu => CallPacketKind::Menu,
        CallKind::FollowUpMenu => CallPacketKind::FollowUpMenu,
        CallKind::HideFollowUpMenu => CallPacketKind::HideFollowUpMenu,
        CallKind::MenuBuilder => CallPacketKind::MenuBuilder,
        CallKind::MenuBuilderUpdate => CallPacketKind::MenuBuilderUpdate,
        CallKind::HideMenuBuilder => CallPacketKind::HideMenuBuilder,
        CallKind::SetCameraPosition => CallPacketKind::SetCameraPosition,
        CallKind::Sound => CallPacketKind::Sound,
        CallKind::SoundAt => CallPacketKind::SoundAt,
        CallKind::Effect => CallPacketKind::Effect,
        CallKind::EffectReliable => CallPacketKind::EffectReliable,
        CallKind::PlayMusic => CallPacketKind::PlayMusic,
        CallKind::TraceInfo => CallPacketKind::TraceInfo,
        CallKind::Label => CallPacketKind::Label,
        CallKind::LabelWithId => CallPacketKind::LabelWithId,
        CallKind::LabelWithFlags => CallPacketKind::LabelWithFlags,
        CallKind::LabelReliable => CallPacketKind::LabelReliable,
        CallKind::LabelReliableWithId => CallPacketKind::LabelReliableWithId,
        CallKind::LabelReliableWithFlags => CallPacketKind::LabelReliableWithFlags,
        CallKind::RemoveWorldLabel => CallPacketKind::RemoveWorldLabel,
        CallKind::Kick => CallPacketKind::Kick,
        CallKind::KickReason => CallPacketKind::KickReason,
        CallKind::EffectData => CallPacketKind::EffectData,
    }
}

/// One server→client call a host action produces (PAR-0601).
///
/// `Connector::publish_call` takes `(match_id, kind, target, payload)`; this
/// pairs a decoded [`CallPacket`] with that envelope so a host call site reads
/// as one expression (`host.publish(ProducedCall::info_message(..))`). The
/// module validates the kind/target; nothing here can mutate the sim.
#[derive(Debug, Clone, PartialEq)]
pub struct ProducedCall {
    /// `match_call.kind` ABI value (derived from the packet's kind).
    pub kind: CallPacketKind,
    /// Single recipient (`None` = every match member, like a broadcast call).
    pub target: Option<Identity>,
    /// The packet; [`Self::encode_body`] is the `match_call.payload` value.
    pub packet: CallPacket,
}

impl ProducedCall {
    /// Broadcast `packet`.
    pub fn broadcast(packet: CallPacket) -> Self {
        Self {
            kind: packet_kind(packet.kind()),
            target: None,
            packet,
        }
    }

    /// One-recipient `packet`.
    pub fn to(target: Identity, packet: CallPacket) -> Self {
        Self {
            kind: packet_kind(packet.kind()),
            target: Some(target),
            packet,
        }
    }

    /// `match_call.payload` for this call.
    pub fn encode_body(&self) -> Result<Vec<u8>, CallError> {
        self.packet.encode_body()
    }

    /// `[kind][body]` frame (what the client channel pushes).
    pub fn frame(&self) -> Result<Vec<u8>, CallError> {
        self.packet.encode()
    }

    /// Debug projection (`{kind, target, packet}`; `packet` is
    /// [`CallPacket::to_json`]).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": format!("{:?}", self.kind),
            "target": self.target.map(|target| target.to_hex().to_string()),
            "packet": self.packet.to_json(),
        })
    }

    /// `Menus.infoMessage(String)` broadcast (`ServerControl.java:116`).
    pub fn info_message(message: impl Into<String>) -> Self {
        Self::broadcast(CallPacket::InfoMessage {
            message: Some(message.into()),
        })
    }

    /// `Menus.infoMessage(NetConnection, String)` to one player
    /// (`NetServer.java:233`, the whitelist rejection).
    pub fn info_message_to(target: Identity, message: impl Into<String>) -> Self {
        Self::to(
            target,
            CallPacket::InfoMessage {
                message: Some(message.into()),
            },
        )
    }

    /// `Call.kick(NetConnection, String)` (`NetServer.adminRequest(kick)`).
    pub fn kick(target: Identity, reason: impl Into<String>) -> Self {
        Self::to(
            target,
            CallPacket::Kick {
                reason: Some(reason.into()),
            },
        )
    }

    /// `Call.kick(NetConnection, KickReason)` (`Player.kick(KickReason)`).
    pub fn kick_reason(target: Identity, reason: KickReason) -> Self {
        Self::to(target, CallPacket::KickReason { reason })
    }

    /// `Call.traceInfo(Player, TraceInfo)` (`NetServer.java:941`).
    pub fn trace_info(target: Identity, player: Option<i32>, info: TraceInfo) -> Self {
        Self::to(target, CallPacket::TraceInfo { player, info })
    }

    /// `Call.setCameraPosition(float, float)` (`NetClient.java:509`).
    pub fn set_camera_position(x: f32, y: f32) -> Self {
        Self::broadcast(CallPacket::SetCameraPosition { x, y })
    }

    /// `Call.playMusic(String, boolean)` (`NetClient.java:265`).
    pub fn play_music(track: impl Into<String>, interrupt: bool) -> Self {
        Self::broadcast(CallPacket::PlayMusic {
            music: Some(track.into()),
            interrupt,
        })
    }

    /// `Call.sound(Sound, float, float, float)`.
    pub fn sound(sound: Option<SoundId>, volume: f32, pitch: f32, pan: f32) -> Self {
        Self::broadcast(CallPacket::Sound {
            sound,
            volume,
            pitch,
            pan,
        })
    }

    /// `Call.soundAt(Sound, float, float, float, float)`.
    pub fn sound_at(sound: Option<SoundId>, x: f32, y: f32, volume: f32, pitch: f32) -> Self {
        Self::broadcast(CallPacket::SoundAt {
            sound,
            x,
            y,
            volume,
            pitch,
        })
    }

    /// `Call.effect(Effect, float, float, float, Color)`.
    pub fn effect(effect: Option<EffectId>, x: f32, y: f32, rotation: f32, color: Rgba) -> Self {
        Self::broadcast(CallPacket::Effect {
            effect,
            x,
            y,
            rotation,
            color,
        })
    }

    /// `Call.effectReliable(Effect, float, float, float, Color)`.
    pub fn effect_reliable(
        effect: Option<EffectId>,
        x: f32,
        y: f32,
        rotation: f32,
        color: Rgba,
    ) -> Self {
        Self::broadcast(CallPacket::EffectReliable {
            effect,
            x,
            y,
            rotation,
            color,
        })
    }

    /// `Call.effect(Effect, float, float, float, Color, Object data)` — the
    /// `EffectCallPacket2` overload (PAR-0578; module kind
    /// `CallPacketKind::EffectData`).
    pub fn effect_data(
        effect: Option<EffectId>,
        x: f32,
        y: f32,
        rotation: f32,
        color: Rgba,
        data: typeio::TaggedValue,
    ) -> Self {
        Self::broadcast(CallPacket::EffectData(EffectDataBody {
            effect,
            x,
            y,
            rotation,
            color,
            data,
        }))
    }

    /// `Call.label(String, float, float, float)`.
    pub fn label(message: impl Into<String>, duration: f32, world_x: f32, world_y: f32) -> Self {
        Self::broadcast(CallPacket::Label {
            message: Some(message.into()),
            duration,
            world_x,
            world_y,
        })
    }

    /// `Call.labelReliable(String, float, float, float)`.
    pub fn label_reliable(
        message: impl Into<String>,
        duration: f32,
        world_x: f32,
        world_y: f32,
    ) -> Self {
        Self::broadcast(CallPacket::LabelReliable {
            message: Some(message.into()),
            duration,
            world_x,
            world_y,
        })
    }

    /// `Call.removeWorldLabel(int)` (`WorldLabelComp.hide`).
    pub fn remove_world_label(id: i32) -> Self {
        Self::broadcast(CallPacket::RemoveWorldLabel { id })
    }

    /// `Call.menu(int, String, String, String[][])` (`Menus.java:147`).
    pub fn menu(
        menu_id: i32,
        title: impl Into<String>,
        message: impl Into<String>,
        options: Vec<Vec<Option<String>>>,
    ) -> Self {
        Self::broadcast(CallPacket::Menu {
            menu_id,
            title: Some(title.into()),
            message: Some(message.into()),
            options,
        })
    }

    /// `Call.followUpMenu(int, String, String, String[][])`
    /// (`Menus.java:156`).
    pub fn follow_up_menu(
        menu_id: i32,
        title: impl Into<String>,
        message: impl Into<String>,
        options: Vec<Vec<Option<String>>>,
    ) -> Self {
        Self::broadcast(CallPacket::FollowUpMenu {
            menu_id,
            title: Some(title.into()),
            message: Some(message.into()),
            options,
        })
    }

    /// `Call.hideFollowUpMenu(int)` (`Menus.hideFollowUpMenu`).
    pub fn hide_follow_up_menu(menu_id: i32) -> Self {
        Self::broadcast(CallPacket::HideFollowUpMenu { menu_id })
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
            CallPacketKind::EffectData,
        ];
        assert_eq!(kinds.len(), CallKind::ALL.len());
        let mut ordinals = Vec::new();
        for kind in kinds {
            let ordinal = call_kind(kind).ordinal();
            assert!(!ordinals.contains(&ordinal), "{kind:?} maps twice");
            ordinals.push(ordinal);
        }
        assert_eq!(ordinals.len(), 24);
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

/// PAR-0601: the host-side call producers.
#[cfg(test)]
mod producer_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::call_packet::decode;
    use crate::typeio::{EffectId, KickReason, Rgba, SoundId, TraceInfo};

    fn identity(byte: u8) -> Identity {
        Identity::from_byte_array([byte; 32])
    }

    /// Every `ProducedCall` constructor round-trips through the same body
    /// encoding `Connector::publish_call` sends and the same frame decoder the
    /// client channel uses; the module kind is the inverse mapping.
    #[test]
    fn producers_round_trip_through_the_call_channel() {
        let target = identity(2);
        let options = vec![vec![Some("ok".to_string()), None]];
        let produced = vec![
            ProducedCall::info_message("hello"),
            ProducedCall::info_message_to(target, "You are not whitelisted here."),
            ProducedCall::kick(target, "bye"),
            ProducedCall::kick_reason(target, KickReason::Gameover),
            ProducedCall::trace_info(
                target,
                Some(7),
                TraceInfo {
                    uuid: Some("abc".to_string()),
                    names: vec![Some("alice".to_string())],
                    ..TraceInfo::default()
                },
            ),
            ProducedCall::set_camera_position(8.0, 16.0),
            ProducedCall::play_music("calm", true),
            ProducedCall::sound(Some(SoundId(3)), 0.5, 1.0, 0.0),
            ProducedCall::sound_at(Some(SoundId(3)), 1.0, 2.0, 0.5, 1.0),
            ProducedCall::effect(
                Some(EffectId(5)),
                1.0,
                2.0,
                90.0,
                Rgba {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            ),
            ProducedCall::effect_reliable(Some(EffectId(5)), 1.0, 2.0, 0.0, Rgba::default()),
            ProducedCall::effect_data(
                Some(EffectId(6)),
                3.0,
                4.0,
                45.0,
                Rgba::default(),
                typeio::TaggedValue::ObjectArray(vec![
                    typeio::TaggedValue::Int(1),
                    typeio::TaggedValue::String(Some("hit".to_string())),
                ]),
            ),
            ProducedCall::label("hi", 5.0, 3.0, 4.0),
            ProducedCall::label_reliable("hi", 5.0, 3.0, 4.0),
            ProducedCall::remove_world_label(9),
            ProducedCall::menu(1, "title", "message", options.clone()),
            ProducedCall::follow_up_menu(1, "title", "message", options),
            ProducedCall::hide_follow_up_menu(1),
        ];
        for call in produced {
            let body = call.encode_body().unwrap();
            let frame = call.frame().unwrap();
            assert_eq!(frame[0], call.packet.kind().ordinal(), "{:?}", call.kind);
            assert_eq!(&frame[1..], body.as_slice());
            assert_eq!(decode(&frame).unwrap(), call.packet, "{:?}", call.kind);
            // `publish_call` receives this kind/target/payload triple.
            assert_eq!(call.kind, packet_kind(call.packet.kind()));
            assert_eq!(
                call.kind,
                packet_kind(call_kind(call.kind)),
                "module ABI mapping is stable"
            );
        }

        // Broadcast vs targeted kinds (module `is_targeted` requires a target
        // for kick/traceInfo; infoMessage picks the broadcast overload).
        assert!(ProducedCall::kick(target, "x").target.is_some());
        assert!(
            ProducedCall::kick_reason(target, KickReason::Vote)
                .target
                .is_some()
        );
        assert!(
            ProducedCall::trace_info(target, None, TraceInfo::default())
                .target
                .is_some()
        );
        assert!(ProducedCall::info_message("x").target.is_none());
        assert!(ProducedCall::set_camera_position(0.0, 0.0).target.is_none());
    }

    /// `packet_kind` is the exact inverse of `call_kind` over the whole
    /// append-only ABI (a new kind fails to compile in both matches). The
    /// generated client enum has no `ALL` const, so the module's 24-kind list
    /// is spelled here like the transport test above.
    #[test]
    fn module_and_client_kind_mappings_are_inverse() {
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
            CallPacketKind::EffectData,
        ];
        for kind in kinds {
            assert_eq!(packet_kind(call_kind(kind)), kind, "{kind:?}");
        }
        // Every client kind has a module kind and vice versa.
        assert_eq!(CallKind::ALL.len(), kinds.len());
    }

    /// PAR-0578 residual: a row published with the appended module kind
    /// `CallPacketKind::EffectData` decodes to the client's `EffectCallPacket2`
    /// packet, byte-for-byte through the same binder path
    /// `MindNet.push_call_frame` uses (module kind → client ordinal → body).
    #[test]
    fn effect_data_rows_round_trip_with_the_module_kind() {
        use crate::call_packet::EffectDataBody;
        use crate::typeio::TaggedValue;

        let body = EffectDataBody {
            effect: Some(EffectId(5)),
            x: 1.5,
            y: -2.5,
            rotation: 90.0,
            color: Rgba {
                r: 255,
                g: 128,
                b: 0,
                a: 255,
            },
            data: TaggedValue::ObjectArray(vec![
                TaggedValue::Int(3),
                TaggedValue::String(Some("hit".to_string())),
            ]),
        };
        let payload = call_packet::encode_effect_data_body(
            body.effect,
            body.x,
            body.y,
            body.rotation,
            body.color,
            &body.data,
        )
        .unwrap();
        // The exact row `publish_call(EffectData, ...)` stores: the module
        // kind + the `EffectCallPacket2.write` body.
        let row = MatchCall {
            call_id: 1,
            match_id: 3,
            sender: Identity::from_byte_array([2u8; 32]),
            kind: CallPacketKind::EffectData,
            target: None,
            payload,
            sent_at: spacetimedb_sdk::Timestamp::from_micros_since_unix_epoch(1_000),
        };

        // The module kind maps to the appended client ordinal.
        assert_eq!(call_kind(row.kind), CallKind::EffectData);
        assert_eq!(call_kind(row.kind).ordinal(), 23);
        assert_eq!(frame_for_row(&row)[0], 23);
        // ...and the row decodes back to the exact EffectCallPacket2 call.
        assert_eq!(
            decode_row(&row).unwrap(),
            CallPacket::EffectData(body.clone())
        );
        // The producer-side constructor sends the same kind/body triple.
        let produced = ProducedCall::effect_data(
            body.effect,
            body.x,
            body.y,
            body.rotation,
            body.color,
            body.data.clone(),
        );
        assert_eq!(produced.kind, CallPacketKind::EffectData);
        assert_eq!(
            produced.encode_body().unwrap().as_slice(),
            &frame_for_row(&row)[1..]
        );
        assert_eq!(
            call_packet::decode(&produced.frame().unwrap()).unwrap(),
            CallPacket::EffectData(body)
        );
    }

    /// The debug JSON names the module kind and the recipient.
    #[test]
    fn producer_json_names_kind_target_and_packet() {
        let call = ProducedCall::kick(identity(4), "bye");
        let json = call.to_json();
        assert_eq!(json["kind"], "Kick");
        assert_eq!(json["target"], identity(4).to_hex().to_string());
        assert_eq!(json["packet"]["body"]["reason"], "bye");
        assert_eq!(json["packet"]["call"], "kick");
    }
}

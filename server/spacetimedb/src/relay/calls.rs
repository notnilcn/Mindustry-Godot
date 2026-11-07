// SPDX-License-Identifier: GPL-3.0-only

//! Server→client `@Remote` call publication (GAPS #35 / PAR-0576).
//!
//! Java's server sends the `@Remote`-generated `Call.*` methods over arcnet:
//! `Call.infoMessage`, `Call.menu`/`followUpMenu`/`hideFollowUpMenu`,
//! `Call.menuBuilder`/`menuBuilderUpdate`/`hideMenuBuilder`,
//! `Call.setCameraPosition`, `Call.sound`/`soundAt`, `Call.effect`/
//! `effectReliable`, `Call.playMusic`, `Call.traceInfo`, the three `label` and
//! three `labelReliable` overloads, `Call.removeWorldLabel` and
//! `Call.kick`/`Call.kick(KickReason)` (`core/src/mindustry/core/NetServer.java`
//! call sites; packet layouts in `core/build/generated/.../mindustry/gen`).
//!
//! A SpacetimeDB module has no arcnet transport, so each call becomes an
//! append-only [`MatchCall`] row: [`publish_call`] stores the kind, the optional
//! single recipient and the byte-for-byte `*CallPacket.write(...)` body (the
//! kind byte is *not* part of the body — the client binder prepends it, see
//! `client/rust/mind-stdb/src/calls.rs`). `my_match_calls` exposes the caller's
//! broadcast + targeted rows. This mirrors the `match_event`/`match_ui_event`
//! publication shape (GAPS #37 / plan 21 §3.6).
//!
//! Rows are pruned by [`crate::main::global::CALL_RETENTION_SECS`].

use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, Timestamp, reducer, table};

use super::methods::{is_host_or_admin, require_match, require_member};
use super::tables::{MatchStatus, relay_member};
use crate::main::global::MAX_CALL_PAYLOAD_BYTES;

/// One server→client `@Remote` call kind (GAPS #35 / PAR-0546).
///
/// Variant names are the module ABI: append-only. The order is the client
/// binder's mapping order in `mind-stdb/src/calls.rs`, but the ordinals are
/// *not* wire ABI: the binder maps every variant explicitly.
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CallPacketKind {
    /// `Call.infoMessage(String)`.
    InfoMessage,
    /// `Call.menu(int, String, String, String[][])`.
    Menu,
    /// `Call.followUpMenu(int, String, String, String[][])`.
    FollowUpMenu,
    /// `Call.hideFollowUpMenu(int)`.
    HideFollowUpMenu,
    /// `Call.menuBuilder(int, long, String, boolean, boolean, boolean, NodeBuilder)`.
    MenuBuilder,
    /// `Call.menuBuilderUpdate(int, String, NodeBuilder)`.
    MenuBuilderUpdate,
    /// `Call.hideMenuBuilder(int)`.
    HideMenuBuilder,
    /// `Call.setCameraPosition(float, float)`.
    SetCameraPosition,
    /// `Call.sound(Sound, float, float, float)`.
    Sound,
    /// `Call.soundAt(Sound, float, float, float, float)`.
    SoundAt,
    /// `Call.effect(Effect, float, float, float, Color)`.
    Effect,
    /// `Call.effectReliable(Effect, float, float, float, Color)`.
    EffectReliable,
    /// `Call.playMusic(String, boolean)`.
    PlayMusic,
    /// `Call.traceInfo(Player, TraceInfo)`.
    TraceInfo,
    /// `Call.label(String, float, float, float)`.
    Label,
    /// `Call.label(String, int, float, float, float)`.
    LabelWithId,
    /// `Call.label(String, int, float, float, float, int)`.
    LabelWithFlags,
    /// `Call.labelReliable(String, float, float, float)`.
    LabelReliable,
    /// `Call.labelReliable(String, int, float, float, float)`.
    LabelReliableWithId,
    /// `Call.labelReliable(String, int, float, float, float, int)`.
    LabelReliableWithFlags,
    /// `Call.removeWorldLabel(int)`.
    RemoveWorldLabel,
    /// `Call.kick(NetConnection, String)`.
    Kick,
    /// `Call.kick(NetConnection, KickReason)`.
    KickReason,
}

impl CallPacketKind {
    /// Every kind, in ABI order (append new variants at the end).
    pub const ALL: [Self; 23] = [
        Self::InfoMessage,
        Self::Menu,
        Self::FollowUpMenu,
        Self::HideFollowUpMenu,
        Self::MenuBuilder,
        Self::MenuBuilderUpdate,
        Self::HideMenuBuilder,
        Self::SetCameraPosition,
        Self::Sound,
        Self::SoundAt,
        Self::Effect,
        Self::EffectReliable,
        Self::PlayMusic,
        Self::TraceInfo,
        Self::Label,
        Self::LabelWithId,
        Self::LabelWithFlags,
        Self::LabelReliable,
        Self::LabelReliableWithId,
        Self::LabelReliableWithFlags,
        Self::RemoveWorldLabel,
        Self::Kick,
        Self::KickReason,
    ];

    /// Whether this call addresses exactly one connection upstream
    /// (`Call.kick` and `Call.traceInfo` take a `NetConnection`/`Player`).
    pub fn is_targeted(self) -> bool {
        matches!(self, Self::Kick | Self::KickReason | Self::TraceInfo)
    }
}

/// One published call row (server-only; exposed through `my_match_calls`).
#[table(accessor = match_call, index(accessor = by_call_match, btree(columns = [match_id, call_id])))]
pub struct MatchCall {
    /// Auto-inc call id; also the client's ordering key.
    #[primary_key]
    #[auto_inc]
    pub call_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Publisher (host/admin).
    pub sender: Identity,
    /// Upstream `@Remote` call kind.
    pub kind: CallPacketKind,
    /// Optional single recipient; `None` broadcasts to every member.
    pub target: Option<Identity>,
    /// `*CallPacket.write(...)` body, kind byte excluded (≤ 64 KiB).
    pub payload: Vec<u8>,
    /// Server receive time.
    pub sent_at: Timestamp,
}

/// Publishes one server→client call (host/admin only; GAPS #35).
#[reducer]
pub fn publish_call(
    ctx: &ReducerContext,
    match_id: u64,
    kind: CallPacketKind,
    target: Option<Identity>,
    payload: Vec<u8>,
) -> Result<(), String> {
    let result = publish_call_impl(ctx, match_id, kind, target, payload);
    if let Err(error) = &result {
        log::warn!("publish_call rejected for match {match_id}: {error}");
    }
    result
}

fn publish_call_impl(
    ctx: &ReducerContext,
    match_id: u64,
    kind: CallPacketKind,
    target: Option<Identity>,
    payload: Vec<u8>,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    require_member(ctx, match_id)?;
    if !is_host_or_admin(ctx, &row) {
        return Err("only the host or an admin may publish server calls".to_string());
    }
    let target_is_member = target
        .map(|target| {
            ctx.db
                .relay_member()
                .by_match_identity()
                .filter((match_id, target))
                .next()
                .is_some()
        })
        .unwrap_or(false);
    validate_call(kind, target, payload.len(), target_is_member)?;
    ctx.db.match_call().insert(MatchCall {
        call_id: 0,
        match_id,
        sender: ctx.sender(),
        kind,
        target,
        payload,
        sent_at: ctx.timestamp,
    });
    Ok(())
}

/// Cheap validation shared by the reducer and unit tests (GAPS #35).
///
/// A call body is opaque TypeIO bytes here (the module has no `TypeIO`
/// decoder), so the checks are the cheap ones the module can make: the payload
/// cap, the single-recipient requirement of `kick`/`traceInfo` and target
/// membership.
pub fn validate_call(
    kind: CallPacketKind,
    target: Option<Identity>,
    payload_len: usize,
    target_is_member: bool,
) -> Result<(), String> {
    if payload_len > MAX_CALL_PAYLOAD_BYTES {
        return Err(format!(
            "call payload exceeds {MAX_CALL_PAYLOAD_BYTES} bytes"
        ));
    }
    if kind.is_targeted() && target.is_none() {
        return Err(format!("{kind:?} requires a target member"));
    }
    if let Some(target) = target {
        if !target_is_member {
            return Err(format!(
                "call target {} is not a member of this match",
                target.to_hex().to_string()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn identity(byte: u8) -> Identity {
        Identity::from_byte_array([byte; 32])
    }

    #[test]
    fn call_kinds_are_distinct_and_append_only() {
        // The enum is schema ABI: a duplicate variant would be caught here.
        assert_eq!(CallPacketKind::ALL.len(), 23);
        for (index, kind) in CallPacketKind::ALL.iter().enumerate() {
            assert!(
                !CallPacketKind::ALL[..index].contains(kind),
                "{kind:?} appears twice"
            );
        }
        let targeted: Vec<CallPacketKind> = CallPacketKind::ALL
            .into_iter()
            .filter(|kind| kind.is_targeted())
            .collect();
        assert_eq!(
            targeted,
            vec![
                CallPacketKind::TraceInfo,
                CallPacketKind::Kick,
                CallPacketKind::KickReason
            ]
        );
    }

    #[test]
    fn payload_cap_is_enforced() {
        assert!(validate_call(CallPacketKind::InfoMessage, None, 0, false).is_ok());
        assert!(
            validate_call(
                CallPacketKind::InfoMessage,
                None,
                MAX_CALL_PAYLOAD_BYTES,
                false
            )
            .is_ok()
        );
        assert!(
            validate_call(
                CallPacketKind::InfoMessage,
                None,
                MAX_CALL_PAYLOAD_BYTES + 1,
                false
            )
            .is_err()
        );
    }

    #[test]
    fn kick_requires_a_member_target() {
        let target = identity(7);
        assert!(validate_call(CallPacketKind::Kick, None, 4, false).is_err());
        assert!(validate_call(CallPacketKind::KickReason, None, 1, false).is_err());
        assert!(validate_call(CallPacketKind::Kick, Some(target), 4, false).is_err());
        assert!(validate_call(CallPacketKind::Kick, Some(target), 4, true).is_ok());
        // Broadcastable calls do not require a target.
        assert!(validate_call(CallPacketKind::InfoMessage, None, 4, false).is_ok());
        // A targeted broadcastable call still needs a member.
        assert!(validate_call(CallPacketKind::Menu, Some(target), 0, false).is_err());
        assert!(validate_call(CallPacketKind::Menu, Some(target), 0, true).is_ok());
    }
}

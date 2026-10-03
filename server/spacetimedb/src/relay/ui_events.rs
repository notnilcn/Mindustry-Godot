// SPDX-License-Identifier: GPL-3.0-only

//! Host-published UI event channel (plan 21 §3.6/§6.1/§6.6; plan 14 payload).
//!
//! Host/admin publishers write `match_ui_event` rows; members render them
//! (optionally targeted). Client results (`MenuChoose`, `MenuBuilderChoose`,
//! `TextInputResult`) travel the ordered command log instead, not this table.

use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, Timestamp, reducer, table};

use super::methods::{is_host_or_admin, require_match, require_member};
use super::tables::{MatchStatus, relay_member};
use crate::main::global::MAX_UI_PAYLOAD_BYTES;

/// UI payload kind (plan §6.6). Variant names are ABI: append-only.
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum UiEventKind {
    /// Show a menu-builder tree.
    MenuBuilderShow,
    /// Update a live menu-builder tree.
    MenuBuilderUpdate,
    /// Hide a menu-builder tree.
    MenuBuilderHide,
    /// Set the HUD text overlay.
    HudText,
    /// Clear the HUD text overlay.
    HideHudText,
    /// Announcement banner.
    Announce,
    /// Info popup dialog.
    InfoPopup,
    /// World label.
    Label,
    /// Info toast.
    InfoToast,
    /// Warning toast.
    WarningToast,
    /// Open a URI.
    OpenUri,
    /// Copy text to the clipboard.
    CopyToClipboard,
    /// Ping marker on the map.
    PingMarker,
}

/// One host-published UI event (plan §6.1).
#[table(accessor = match_ui_event, public, index(accessor = by_ui_event_match, btree(columns = [match_id, event_id])))]
pub struct MatchUiEvent {
    /// Auto-inc event id (also the client's ordering key).
    #[primary_key]
    #[auto_inc]
    pub event_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Publisher (host/admin).
    pub sender: Identity,
    /// Event kind.
    pub kind: UiEventKind,
    /// Optional single recipient; `None` broadcasts to all members.
    pub target: Option<Identity>,
    /// Plan-14 encoded payload (≤ 64 KiB).
    pub payload: Vec<u8>,
    /// Server receive time.
    pub sent_at: Timestamp,
}

/// Publishes one UI event (host/admin only; plan §3.6).
#[reducer]
pub fn publish_ui_event(
    ctx: &ReducerContext,
    match_id: u64,
    kind: UiEventKind,
    target: Option<Identity>,
    payload: Vec<u8>,
) -> Result<(), String> {
    let result = publish_ui_event_impl(ctx, match_id, kind, target, payload);
    if let Err(error) = &result {
        log::warn!("publish_ui_event rejected for match {match_id}: {error}");
    }
    result
}

fn publish_ui_event_impl(
    ctx: &ReducerContext,
    match_id: u64,
    kind: UiEventKind,
    target: Option<Identity>,
    payload: Vec<u8>,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    require_member(ctx, match_id)?;
    if !is_host_or_admin(ctx, &row) {
        return Err("only the host or an admin may publish UI events".to_string());
    }
    if payload.len() > MAX_UI_PAYLOAD_BYTES {
        return Err(format!("UI payload exceeds {MAX_UI_PAYLOAD_BYTES} bytes"));
    }
    if let Some(target) = target
        && ctx
            .db
            .relay_member()
            .by_match_identity()
            .filter((match_id, target))
            .next()
            .is_none()
    {
        return Err("UI event target is not a member of this match".to_string());
    }
    ctx.db.match_ui_event().insert(MatchUiEvent {
        event_id: 0,
        match_id,
        sender: ctx.sender(),
        kind,
        target,
        payload,
        sent_at: ctx.timestamp,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_cap_constant() {
        assert_eq!(MAX_UI_PAYLOAD_BYTES, 64 * 1024);
    }

    #[test]
    fn ui_event_kind_is_copy_and_distinct() {
        assert_ne!(UiEventKind::Announce, UiEventKind::WarningToast);
        let kinds = [
            UiEventKind::MenuBuilderShow,
            UiEventKind::MenuBuilderUpdate,
            UiEventKind::MenuBuilderHide,
            UiEventKind::HudText,
            UiEventKind::HideHudText,
            UiEventKind::Announce,
            UiEventKind::InfoPopup,
            UiEventKind::Label,
            UiEventKind::InfoToast,
            UiEventKind::WarningToast,
            UiEventKind::OpenUri,
            UiEventKind::CopyToClipboard,
            UiEventKind::PingMarker,
        ];
        assert_eq!(kinds.len(), 13);
    }
}

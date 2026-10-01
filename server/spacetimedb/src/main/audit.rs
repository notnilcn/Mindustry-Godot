// SPDX-License-Identifier: GPL-3.0-only

//! Server-only audit trail (plan 01 §3.7/§3.9, R7). Only **committed** actions
//! are audited: reducers that return `Err` roll back their writes, so failure
//! paths must use `log::warn!` instead of [`audit`].

use spacetimedb::{Identity, ReducerContext, Table, Timestamp, table};

use super::tables::AuditKind;

/// Append-only audit log; never exposed to clients (server-only table).
#[table(accessor = audit_log)]
pub struct AuditLog {
    #[primary_key]
    #[auto_inc]
    pub log_id: u64,
    pub at: Timestamp,
    pub actor: Option<Identity>,
    pub kind: AuditKind,
    pub message: String,
}

/// Writes one committed-action audit row. `actor` is `None` for server-side
/// events (for example `init`).
pub fn audit(
    ctx: &ReducerContext,
    actor: Option<Identity>,
    kind: AuditKind,
    message: impl Into<String>,
) {
    ctx.db.audit_log().insert(AuditLog {
        log_id: 0,
        at: ctx.timestamp,
        actor,
        kind,
        message: message.into(),
    });
}

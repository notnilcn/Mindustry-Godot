// SPDX-License-Identifier: GPL-3.0-only

//! Typed per-table binders, the Rust analog of the legacy `TableBinderComponent`
//! (plan 01 §3.5).
//!
//! A binder owns one table and receives owned row changes through a queue that
//! its SDK callbacks push into. Callbacks capture a `Weak<BinderCore>`: when the
//! consumer drops the binder the queue is freed and every callback becomes an
//! inert no-op, so a dropped binder can never grow memory. The callback
//! registrations themselves live on the (per-connection) SDK connection object
//! and die with it on disconnect — the Rust replacement for C# `_ExitTree`
//! unhooking, without needing a connection handle inside a dropped binder.
//!
//! `replay_existing` is a type-gated method: it only exists for accessors whose
//! generated handle implements [`spacetimedb_sdk::TableWithPrimaryKey`], so
//! "never replay event tables" (C# rule 3) is a compile-time property.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Weak};

use spacetimedb_sdk::{Table as SdkTable, TableAccessor, TableWithPrimaryKey};

use crate::module_bindings::RemoteTables;

/// Binder behavior flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BinderOptions {
    /// Enqueue already-cached rows as [`RowChange::Insert`] once bound (only
    /// effective through `Connector::bind_with_replay`, on primary-key tables).
    pub replay_existing: bool,
    /// Log every delivered change at debug level.
    pub verbose: bool,
}

/// A change delivered to a binder. Rows are owned, so nothing crosses a signal
/// boundary (C# `LastRow`/`LastOldRow` disappear).
#[derive(Debug, Clone, PartialEq)]
pub enum RowChange<T> {
    /// A row entered the subscribed set.
    Insert(T),
    /// A primary-key row changed (update-capable binders only).
    Update {
        /// Previous row value.
        old: T,
        /// New row value.
        new: T,
    },
    /// A row left the subscribed set.
    Delete(T),
}

/// Shared queue + options behind one [`TableBinder`].
pub(crate) struct BinderCore<Row> {
    table: &'static str,
    options: Mutex<BinderOptions>,
    queue: Mutex<VecDeque<RowChange<Row>>>,
}

impl<Row> BinderCore<Row> {
    pub(crate) fn new(table: &'static str, options: BinderOptions) -> Self {
        Self {
            table,
            options: Mutex::new(options),
            queue: Mutex::new(VecDeque::new()),
        }
    }

    pub(crate) fn push(&self, change: RowChange<Row>) {
        let mut guard = match self.queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.push_back(change);
    }

    pub(crate) fn drain(&self) -> Vec<RowChange<Row>> {
        let mut guard = match self.queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.drain(..).collect()
    }

    pub(crate) fn replay<I: IntoIterator<Item = Row>>(&self, rows: I) {
        let mut guard = match self.queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.extend(rows.into_iter().map(RowChange::Insert));
    }

    pub(crate) fn options(&self) -> BinderOptions {
        let guard = match self.options.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard
    }

    pub(crate) fn set_replay_existing(&self, enabled: bool) {
        let mut guard = match self.options.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.replay_existing = enabled;
    }
}

/// One consumer's binding to a subscribed table.
pub struct TableBinder<A: TableAccessor<RemoteTables>> {
    core: Arc<BinderCore<A::Row>>,
}

impl<A: TableAccessor<RemoteTables>> TableBinder<A> {
    pub(crate) fn new(core: Arc<BinderCore<A::Row>>) -> Self {
        Self { core }
    }

    /// Generated accessor name of the bound table.
    pub fn table_name(&self) -> &'static str {
        self.core.table
    }

    /// Binder flags as currently configured.
    pub fn options(&self) -> BinderOptions {
        self.core.options()
    }

    /// Number of queued changes not yet drained.
    pub fn pending(&self) -> usize {
        let guard = match self.core.queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.len()
    }

    /// Takes every queued change in delivery order (call after `pump()`).
    pub fn drain(&self) -> Vec<RowChange<A::Row>> {
        self.core.drain()
    }

    /// Enqueues `rows` as [`RowChange::Insert`] in iteration order.
    ///
    /// Used for cache replay (`Connector::bind_with_replay`) and by the
    /// `stdb_binder_replay` scenario/tests with a synthetic row source.
    pub fn replay<I: IntoIterator<Item = A::Row>>(&self, rows: I) {
        self.core.replay(rows);
    }

    /// Synthetic delivery hook for the `stdb_*` scenarios and unit tests; live
    /// delivery goes through SDK callbacks.
    #[doc(hidden)]
    pub fn inject(&self, change: RowChange<A::Row>) {
        self.core.push(change);
    }
}

impl<A> TableBinder<A>
where
    A: TableAccessor<RemoteTables>,
    for<'db> A::Handle<'db>: TableWithPrimaryKey,
{
    /// Enables/disables cache replay for this binder's next re-registration.
    ///
    /// This method does not exist for accessors without a primary key — replay
    /// is a compile-time property (plan §3.5):
    ///
    /// ```compile_fail
    /// use mind_stdb::binder::TableBinder;
    /// use mind_stdb::module_bindings::LocalPlayerTableAccessor;
    ///
    /// // `local_player` is a view without a primary key: no `replay_existing`.
    /// fn not_replayable(binder: &TableBinder<LocalPlayerTableAccessor>) {
    ///     binder.replay_existing(true);
    /// }
    /// ```
    pub fn replay_existing(&self, enabled: bool) {
        self.core.set_replay_existing(enabled);
    }
}

/// Registers insert/delete callbacks for `A` on `db`; safe for every table and
/// view handle.
pub(crate) fn bind_live<A>(core: &Arc<BinderCore<A::Row>>, db: &RemoteTables)
where
    A: TableAccessor<RemoteTables>,
    for<'db> A::Handle<'db>: SdkTable<Row = A::Row>,
    A::Row: Clone + Send,
{
    let handle = A::get(db);
    let insert_core = Arc::downgrade(core);
    let delete_core = Arc::downgrade(core);
    handle.on_insert(move |_ctx, row| push_weak(&insert_core, RowChange::Insert(row.clone())));
    handle.on_delete(move |_ctx, row| push_weak(&delete_core, RowChange::Delete(row.clone())));
}

/// Registers insert/delete/update callbacks for a primary-key table and, when
/// requested, replays the current client cache after the live callbacks are in
/// place (C# `ReplayExistingRows` order).
pub(crate) fn bind_live_with_updates<A>(
    core: &Arc<BinderCore<A::Row>>,
    db: &RemoteTables,
    replay_existing: bool,
) where
    A: TableAccessor<RemoteTables>,
    for<'db> A::Handle<'db>: SdkTable<Row = A::Row> + TableWithPrimaryKey,
    A::Row: Clone + Send,
{
    bind_live::<A>(core, db);
    let handle = A::get(db);
    let update_core = Arc::downgrade(core);
    handle.on_update(move |_ctx, old, new| {
        push_weak(
            &update_core,
            RowChange::Update {
                old: old.clone(),
                new: new.clone(),
            },
        );
    });
    if replay_existing {
        let rows: Vec<A::Row> = A::get(db).iter().collect();
        core.replay(rows);
    }
}

fn push_weak<Row>(weak: &Weak<BinderCore<Row>>, change: RowChange<Row>) {
    if let Some(core) = weak.upgrade() {
        core.push(change);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module_bindings::ProtocolInfo;

    fn row(version: u32) -> ProtocolInfo {
        ProtocolInfo {
            id: version as u8,
            protocol_version: version,
            min_client_build: 1,
            save_format_version: 1,
        }
    }

    #[test]
    fn replay_emits_rows_then_live_inserts() {
        let core = Arc::new(BinderCore::new("protocol_info", BinderOptions::default()));
        let binder: TableBinder<crate::module_bindings::ProtocolInfoTableAccessor> =
            TableBinder::new(core.clone());

        let cached = vec![row(1), row(2)];
        binder.replay(cached.clone());
        let drained = binder.drain();
        assert_eq!(
            drained,
            vec![
                RowChange::Insert(cached[0].clone()),
                RowChange::Insert(cached[1].clone())
            ]
        );
        assert_eq!(binder.pending(), 0);

        // Live inserts arrive after replay, preserving order.
        core.push(RowChange::Insert(row(3)));
        core.push(RowChange::Delete(row(3)));
        core.push(RowChange::Update {
            old: row(1),
            new: row(2),
        });
        let drained = binder.drain();
        assert_eq!(drained.len(), 3);
        assert!(matches!(drained[0], RowChange::Insert(_)));
        assert!(matches!(drained[1], RowChange::Delete(_)));
        assert!(matches!(drained[2], RowChange::Update { .. }));
    }

    #[test]
    fn dropped_binder_callbacks_go_inert() {
        let weak = {
            let core = Arc::new(BinderCore::new("protocol_info", BinderOptions::default()));
            let weak = Arc::downgrade(&core);
            {
                let binder: TableBinder<crate::module_bindings::ProtocolInfoTableAccessor> =
                    TableBinder::new(core.clone());
                // Simulate a live SDK callback holding only a Weak.
                push_weak(&weak, RowChange::Insert(row(7)));
                assert_eq!(binder.pending(), 1);
            }
            weak
        };
        // Binder (the last strong Arc) dropped: every callback upgrade is a
        // no-op and the queue is freed — no unbounded growth.
        assert!(weak.upgrade().is_none());
        push_weak(&weak, RowChange::Insert(row(8)));
    }

    #[test]
    fn verbose_and_replay_options_roundtrip() {
        let core = Arc::new(BinderCore::new(
            "player",
            BinderOptions {
                replay_existing: false,
                verbose: true,
            },
        ));
        let binder: TableBinder<crate::module_bindings::PlayerTableAccessor> =
            TableBinder::new(core);
        assert!(binder.options().verbose);
        assert_eq!(binder.table_name(), "player");
    }
}

// SPDX-License-Identifier: GPL-3.0-only

//! Typed per-table binders, the Rust analog of the legacy `TableBinderComponent`.
//!
//! A binder owns one table and receives owned/borrowed row changes. At P0 rows arrive as
//! `&dyn Any` so a single erased `Box<dyn TableBinder>` list can live inside
//! [`crate::conn::MindDb`]; consumers downcast to the generated row type (e.g.
//! [`crate::module_bindings::Player`]). Plan `01` replaces this with the fully typed
//! `TableBinder<A: TableAccessor<RemoteTables>>` plus an `mpsc` queue per binder
//! (`01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` §3.5); the method names are kept stable.

use std::any::Any;

/// A change delivered to a binder. Plan `01` consumes this shape from live callbacks.
#[derive(Debug, Clone, PartialEq)]
pub enum RowChange<T> {
    Insert(T),
    Update { old: T, new: T },
    Delete(T),
}

/// One consumer's binding to a subscribed table.
///
/// `replay_existing` is the compile-checkable half of the C# `ReplayExistingRows` flag:
/// it must stay `false` for event tables (replaying their history would re-fire every
/// event ever appended). P0 has no event tables and no live callbacks, so the facade only
/// records the flag; plan `01` makes "replay only on primary-key tables" a type property.
pub trait TableBinder: Send {
    /// Generated accessor name of the bound table (`"player"` at P0).
    fn table_name(&self) -> &'static str;

    /// Whether already-cached rows are replayed as [`RowChange::Insert`] once bound.
    fn replay_existing(&self) -> bool {
        false
    }

    /// A subscribed row was inserted; downcast `row` to the bound table's generated type.
    fn on_insert(&mut self, row: &dyn Any);

    /// A subscribed row was updated; both rows are the same generated type.
    fn on_update(&mut self, old: &dyn Any, new: &dyn Any);

    /// A subscribed row was deleted; downcast `row` to the bound table's generated type.
    fn on_delete(&mut self, row: &dyn Any);
}

/// Opaque handle to a binder registered with [`crate::conn::MindDb`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BinderHandle {
    index: usize,
    table: &'static str,
}

impl BinderHandle {
    pub(crate) fn new(index: usize, table: &'static str) -> Self {
        Self { index, table }
    }

    /// Registration index (stable for the life of the `MindDb`).
    pub fn index(self) -> usize {
        self.index
    }

    /// Generated accessor name of the bound table.
    pub fn table(self) -> &'static str {
        self.table
    }
}

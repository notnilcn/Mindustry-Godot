// SPDX-License-Identifier: GPL-3.0-only

//! Transport seam for the relay (plan 21 §3.13, OD3).
//!
//! The D2 default carries **everything** over SpacetimeDB event tables;
//! [`StdbTransport`] is the only implementation. The trait exists so a future
//! direct ENet/UDP **data plane** (bulk snapshot chunks only; authority stays in
//! STDB) can be swapped in mechanically.

use crate::connector::{Connector, ConnectorError};
use crate::module_bindings::CommandKind;

/// Transport-level failures.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The underlying connector failed to send.
    #[error(transparent)]
    Connector(#[from] ConnectorError),
    /// The direct transport is not implemented (seam only).
    #[error("transport `{0}` is not implemented")]
    Unsupported(&'static str),
}

/// The relay data-plane contract (plan §3.13).
pub trait RelayTransport {
    /// Emits one command row (or the equivalent direct packet).
    fn send_command(
        &mut self,
        match_id: u64,
        client_tick: u64,
        kind: CommandKind,
    ) -> Result<(), TransportError>;

    /// Requests a fresh host snapshot (plan §3.7).
    fn request_snapshot(&mut self, _match_id: u64) -> Result<(), TransportError> {
        Ok(())
    }

    /// Stable transport name for diagnostics/tests.
    fn transport_name(&self) -> &'static str;
}

/// SpacetimeDB-backed transport (the D2 default).
pub struct StdbTransport<'c> {
    connector: &'c mut Connector,
}

impl<'c> StdbTransport<'c> {
    /// Wraps a connector as a relay transport.
    pub fn new(connector: &'c mut Connector) -> Self {
        Self { connector }
    }

    /// Borrow the underlying connector.
    pub fn connector(&mut self) -> &mut Connector {
        self.connector
    }
}

impl RelayTransport for StdbTransport<'_> {
    fn send_command(
        &mut self,
        match_id: u64,
        client_tick: u64,
        kind: CommandKind,
    ) -> Result<(), TransportError> {
        self.connector
            .send_match_command(match_id, client_tick, kind)?;
        Ok(())
    }

    fn transport_name(&self) -> &'static str {
        "stdb"
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::config::{ConnectionConfig, StdbMode};

    fn offline_connector() -> Connector {
        let mut config = ConnectionConfig::local();
        config.mode = StdbMode::Offline;
        Connector::new(config)
    }

    #[test]
    fn stdb_transport_contract() {
        let mut connector = offline_connector();
        let mut transport = StdbTransport::new(&mut connector);
        assert_eq!(transport.transport_name(), "stdb");
        // Offline: the send fails with a connector error, never panics.
        let error = transport.send_command(1, 0, CommandKind::Noop).unwrap_err();
        assert!(matches!(error, TransportError::Connector(_)));
        // request_snapshot is a no-op on the STDB seam (rows carry it).
        assert!(transport.request_snapshot(1).is_ok());
    }
}

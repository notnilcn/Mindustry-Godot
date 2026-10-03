// SPDX-License-Identifier: GPL-3.0-only

//! Transport seam for the relay (plan 21 §3.13, OD3).
//!
//! The D2 default carries **everything** over SpacetimeDB event tables;
//! [`StdbTransport`] is the only implementation. The trait exists so a future
//! direct ENet/UDP **data plane** (bulk snapshot chunks only; authority stays in
//! STDB) can be swapped in mechanically. Methods a direct transport would not
//! carry (ordered command delivery, player state, plans) default to
//! [`TransportError::Unsupported`] so a swap cannot silently drop durability.

use spacetimedb_sdk::Identity;

use crate::connector::{Connector, ConnectorError};
use crate::module_bindings::{CommandKind, PlayerStateReport, SnapshotKind};

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

/// One ordered command delivery (sender-scoped sequence + payload).
#[derive(Debug, Clone, PartialEq)]
pub struct CommandDelivery {
    /// Command author.
    pub sender: Identity,
    /// Per-sender continuity sequence (plan 01 §3.8).
    pub sender_seq: u64,
    /// Command payload.
    pub kind: CommandKind,
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

    /// Subscribes the ordered command stream. STDB rows arrive through the
    /// Game wave, so the default is a no-op; a direct transport overrides this.
    fn subscribe_commands(&mut self) -> Result<(), TransportError> {
        Ok(())
    }

    /// Reports the caller's LWW player state. Direct transports do not carry it
    /// (it stays on STDB under D2).
    fn send_player_state(
        &mut self,
        _match_id: u64,
        _report: PlayerStateReport,
    ) -> Result<(), TransportError> {
        Err(TransportError::Unsupported("direct::player_state"))
    }

    /// Uploads one plan-snapshot chunk. Direct transports do not carry it.
    fn report_plan(
        &mut self,
        _match_id: u64,
        _group_id: u32,
        _chunk_index: u16,
        _chunk_count: u16,
        _plans_blob: Vec<u8>,
    ) -> Result<(), TransportError> {
        Err(TransportError::Unsupported("direct::plan"))
    }

    /// Publishes a host snapshot chunk. STDB chunks it into rows; a direct
    /// transport may override this to carry the bulk payload.
    fn publish_snapshot_chunk(
        &mut self,
        _match_id: u64,
        _blob: Vec<u8>,
    ) -> Result<(), TransportError> {
        Err(TransportError::Unsupported("direct::snapshot"))
    }

    /// Requests a fresh host snapshot (plan §3.7).
    fn request_snapshot(&mut self, _match_id: u64) -> Result<(), TransportError> {
        Ok(())
    }

    /// Drains any transport-local command deliveries. STDB ordering lives in
    /// [`crate::relay::CommandStream`], so the default is empty.
    fn drain(&mut self) -> Result<Vec<CommandDelivery>, TransportError> {
        Ok(Vec::new())
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

    fn subscribe_commands(&mut self) -> Result<(), TransportError> {
        self.connector.subscribe_game();
        Ok(())
    }

    fn send_player_state(
        &mut self,
        match_id: u64,
        report: PlayerStateReport,
    ) -> Result<(), TransportError> {
        self.connector.report_player_state(match_id, report)?;
        Ok(())
    }

    fn report_plan(
        &mut self,
        match_id: u64,
        group_id: u32,
        chunk_index: u16,
        chunk_count: u16,
        plans_blob: Vec<u8>,
    ) -> Result<(), TransportError> {
        self.connector.report_plan_snapshot(
            match_id,
            group_id,
            chunk_index,
            chunk_count,
            plans_blob,
        )?;
        Ok(())
    }

    fn publish_snapshot_chunk(
        &mut self,
        match_id: u64,
        blob: Vec<u8>,
    ) -> Result<(), TransportError> {
        // STDB chunks the blob into `match_snapshot_chunk` rows server-side;
        // the header fields are filled by the caller's snapshot codec.
        self.connector.publish_snapshot(
            match_id,
            SnapshotKind::Dynamic,
            1,
            1,
            0,
            0,
            0,
            "",
            0,
            0,
            "",
            0,
            blob,
        )?;
        Ok(())
    }

    fn request_snapshot(&mut self, match_id: u64) -> Result<(), TransportError> {
        self.connector.request_snapshot(match_id)?;
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

    fn sample_report() -> PlayerStateReport {
        PlayerStateReport {
            seq: 1,
            unit_id: -1,
            dead: false,
            x: 1.0,
            y: 2.0,
            vx: 0.0,
            vy: 0.0,
            pointer_x: 0.0,
            pointer_y: 0.0,
            rotation: 0.0,
            base_rotation: 0.0,
            mining_x: -1,
            mining_y: -1,
            boosting: false,
            shooting: false,
            chatting: false,
            building: false,
            selected_block: None,
            selected_rotation: 0,
            view_x: 0.0,
            view_y: 0.0,
            view_width: 0.0,
            view_height: 0.0,
            health: 0.0,
            shield: 0.0,
            team: 0,
        }
    }

    #[test]
    fn stdb_transport_contract() {
        let mut connector = offline_connector();
        let mut transport = StdbTransport::new(&mut connector);
        assert_eq!(transport.transport_name(), "stdb");
        // Offline: sends fail with a connector error, never panic.
        let error = transport.send_command(1, 0, CommandKind::Noop).unwrap_err();
        assert!(matches!(error, TransportError::Connector(_)));
        // STDB owns ordering via `CommandStream`: no local deliveries to drain.
        assert!(transport.drain().unwrap().is_empty());
        // Subscribing is a no-op offline (it flips wave intent only).
        assert!(transport.subscribe_commands().is_ok());
        // request_snapshot is a reducer call, so offline it reports the
        // connector error (STDB carries the request row itself).
        assert!(matches!(
            transport.request_snapshot(1),
            Err(TransportError::Connector(_))
        ));
        // Player state / plans / snapshots route through the connector.
        assert!(matches!(
            transport.send_player_state(1, sample_report()),
            Err(TransportError::Connector(_))
        ));
        assert!(matches!(
            transport.report_plan(1, 0, 0, 1, vec![1, 2, 3]),
            Err(TransportError::Connector(_))
        ));
        assert!(matches!(
            transport.publish_snapshot_chunk(1, vec![0u8; 4]),
            Err(TransportError::Connector(_))
        ));
    }

    #[test]
    fn direct_defaults_are_unsupported() {
        // A minimal direct-transport double only implements the data plane; the
        // durability-carrying methods must default to `Unsupported`.
        struct Direct;
        impl RelayTransport for Direct {
            fn send_command(
                &mut self,
                _match_id: u64,
                _client_tick: u64,
                _kind: CommandKind,
            ) -> Result<(), TransportError> {
                Ok(())
            }
            fn transport_name(&self) -> &'static str {
                "direct"
            }
        }
        let mut direct = Direct;
        assert_eq!(direct.transport_name(), "direct");
        assert!(direct.subscribe_commands().is_ok());
        assert!(direct.drain().unwrap().is_empty());
        assert!(matches!(
            direct.send_player_state(1, sample_report()),
            Err(TransportError::Unsupported("direct::player_state"))
        ));
        assert!(matches!(
            direct.publish_snapshot_chunk(1, Vec::new()),
            Err(TransportError::Unsupported("direct::snapshot"))
        ));
    }
}

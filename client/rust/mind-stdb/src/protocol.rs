// SPDX-License-Identifier: GPL-3.0-only

//! Our protocol version and handshake check (plan 01 §3.3/§6.4). Bump
//! [`PROTOCOL_VERSION`] together with the server's `main/global.rs` constant —
//! it is separate from the crate versions.

use crate::module_bindings::ProtocolInfo;

/// Client-side protocol version; must equal the server's `PROTOCOL_VERSION`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Client build number compared against `ProtocolInfo::min_client_build`.
pub const CLIENT_BUILD: u32 = 1;

/// Protocol handshake failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    /// The server speaks a different envelope version than this client.
    #[error("protocol mismatch: client {client}, server {server}")]
    Mismatch {
        /// Client [`PROTOCOL_VERSION`].
        client: u32,
        /// Server `ProtocolInfo::protocol_version`.
        server: u32,
    },
    /// This client build is older than the server's minimum.
    #[error("client build {client_build} is older than server minimum {min_client_build}")]
    ClientTooOld {
        /// [`CLIENT_BUILD`].
        client_build: u32,
        /// Server `ProtocolInfo::min_client_build`.
        min_client_build: u32,
    },
}

/// Checks the server's `protocol_info` singleton against this client.
///
/// The server row is the Base-wave beacon; consumers call this after the Base
/// wave applies (plan 23 verifies the live row).
pub fn check_protocol(server: &ProtocolInfo) -> Result<(), ProtocolError> {
    if server.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::Mismatch {
            client: PROTOCOL_VERSION,
            server: server.protocol_version,
        });
    }
    if CLIENT_BUILD < server.min_client_build {
        return Err(ProtocolError::ClientTooOld {
            client_build: CLIENT_BUILD,
            min_client_build: server.min_client_build,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(protocol_version: u32, min_client_build: u32) -> ProtocolInfo {
        ProtocolInfo {
            id: 0,
            protocol_version,
            min_client_build,
            save_format_version: 1,
        }
    }

    #[test]
    fn version_mismatch_is_rejected() {
        assert_eq!(check_protocol(&row(PROTOCOL_VERSION, CLIENT_BUILD)), Ok(()));
        assert_eq!(
            check_protocol(&row(PROTOCOL_VERSION + 1, CLIENT_BUILD)),
            Err(ProtocolError::Mismatch {
                client: PROTOCOL_VERSION,
                server: PROTOCOL_VERSION + 1,
            })
        );
        assert_eq!(
            check_protocol(&row(PROTOCOL_VERSION, CLIENT_BUILD + 1)),
            Err(ProtocolError::ClientTooOld {
                client_build: CLIENT_BUILD,
                min_client_build: CLIENT_BUILD + 1,
            })
        );
    }
}

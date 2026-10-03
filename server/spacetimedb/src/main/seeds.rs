// SPDX-License-Identifier: GPL-3.0-only

//! Code seeds (plan 01 §3.7; plan 21 §6.8). Seeds live in code and re-run on
//! every publish (publishing wipes local dev data; main/server rule).

use spacetimedb::{Identity, ReducerContext, Table};

use super::global::{
    ADMIN_IDENTITIES, DEFAULT_COMMANDS_PER_SECOND, DEFAULT_COMMAND_RATE_WINDOW_MS,
    DEFAULT_MAP_HEIGHT_TILES, DEFAULT_MAP_WIDTH_TILES, DEFAULT_MAX_COMMIT_COMMANDS_PER_TRANSACTION,
    MIN_CLIENT_BUILD, PROTOCOL_VERSION, SAVE_FORMAT_VERSION,
};
use super::tables::{ProtocolInfo, RelayConfig, protocol_info, relay_config};
use crate::admin::{AdminIdentity, ServerConfig, admin_identity, default_server_config, server_config};

/// Upsert-style seed: re-seeding a row replaces its contents (publish wipes
/// anyway; this keeps `init` idempotent for tests that call it directly).
pub trait Seed: Sized {
    fn seed(ctx: &ReducerContext, data: Self);
}

impl Seed for ProtocolInfo {
    fn seed(ctx: &ReducerContext, data: Self) {
        if ctx.db.protocol_info().id().find(data.id).is_some() {
            ctx.db.protocol_info().id().update(data);
        } else {
            ctx.db.protocol_info().insert(data);
        }
    }
}

impl Seed for RelayConfig {
    fn seed(ctx: &ReducerContext, data: Self) {
        if ctx.db.relay_config().id().find(data.id).is_some() {
            ctx.db.relay_config().id().update(data);
        } else {
            ctx.db.relay_config().insert(data);
        }
    }
}

impl Seed for ServerConfig {
    fn seed(ctx: &ReducerContext, data: Self) {
        if ctx.db.server_config().id().find(data.id).is_some() {
            ctx.db.server_config().id().update(data);
        } else {
            ctx.db.server_config().insert(data);
        }
    }
}

/// Seeds the singleton [`ProtocolInfo`] row (`id = 0`).
pub fn seed_protocol_info(ctx: &ReducerContext) {
    ProtocolInfo::seed(
        ctx,
        ProtocolInfo {
            id: 0,
            protocol_version: PROTOCOL_VERSION,
            min_client_build: MIN_CLIENT_BUILD,
            save_format_version: SAVE_FORMAT_VERSION,
        },
    );
}

/// Seeds the singleton [`RelayConfig`] row (`id = 0`).
pub fn seed_relay_config(ctx: &ReducerContext) {
    RelayConfig::seed(
        ctx,
        RelayConfig {
            id: 0,
            commands_per_second: DEFAULT_COMMANDS_PER_SECOND,
            command_rate_window_ms: DEFAULT_COMMAND_RATE_WINDOW_MS,
            max_commit_commands_per_transaction: DEFAULT_MAX_COMMIT_COMMANDS_PER_TRANSACTION,
            default_map_width_tiles: DEFAULT_MAP_WIDTH_TILES,
            default_map_height_tiles: DEFAULT_MAP_HEIGHT_TILES,
        },
    );
}

/// Seeds the singleton [`ServerConfig`] row (`id = 0`, plan §6.8).
pub fn seed_server_config(ctx: &ReducerContext) {
    ServerConfig::seed(ctx, default_server_config());
}

/// Seeds the bootstrap admin identities from [`ADMIN_IDENTITIES`] (OD-21-D).
///
/// Empty by default; malformed entries are logged and skipped rather than
/// failing the publish.
pub fn seed_admin_identities(ctx: &ReducerContext) {
    for hex in ADMIN_IDENTITIES {
        match Identity::from_hex(hex) {
            Ok(identity) => {
                if ctx.db.admin_identity().identity().find(identity).is_none() {
                    ctx.db.admin_identity().insert(AdminIdentity {
                        identity,
                        granted_by: identity,
                        granted_at: ctx.timestamp,
                    });
                }
            }
            Err(error) => log::warn!("seed_admin_identities: bad hex `{hex}`: {error}"),
        }
    }
}

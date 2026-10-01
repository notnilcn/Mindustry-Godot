// SPDX-License-Identifier: GPL-3.0-only

//! Pure-ish relay helpers (plan 01 §3.9): membership check, rate gate and the
//! cheap command validator. `validate_kind` is deliberately a pure function so
//! a future authoritative sim can re-use it (D2, §3.10).

use spacetimedb::{ReducerContext, Table};

use super::tables::{
    BreakBlock, CommandKind, CommandRate, ConfigBlock, MatchStatus, PlaceBlock, RelayMatch,
    RelayMember, command_rate, relay_match, relay_member,
};
use crate::main::global::{
    DEFAULT_COMMANDS_PER_SECOND, DEFAULT_COMMAND_RATE_WINDOW_MS,
    DEFAULT_MAP_HEIGHT_TILES, DEFAULT_MAP_WIDTH_TILES,
};
use crate::main::tables::{RelayConfig, relay_config};

/// Map-id cap (plan §6.3 keeps IDs short; plan 21 may relax).
pub const MAX_MAP_ID_LEN: usize = 64;

/// Whether a rate window starting at `window_start_micros` has expired.
pub fn window_expired(now_micros: i64, window_start_micros: i64, window_ms: u32) -> bool {
    now_micros.saturating_sub(window_start_micros) >= i64::from(window_ms) * 1_000
}

/// The next per-sender sequence; `None` on `u64` overflow.
pub fn next_sender_seq(last: u64) -> Option<u64> {
    last.checked_add(1)
}

/// Requires the caller to be a member of `match_id`.
pub fn require_member(ctx: &ReducerContext, match_id: u64) -> Result<RelayMember, String> {
    ctx.db
        .relay_member()
        .by_match_identity()
        .filter((match_id, ctx.sender()))
        .next()
        .ok_or_else(|| "caller is not a member of this match".to_string())
}

/// Loads the singleton relay config (defaults when `init` has not seeded).
pub fn relay_config_or_default(ctx: &ReducerContext) -> RelayConfig {
    ctx.db.relay_config().id().find(0u8).unwrap_or(RelayConfig {
        id: 0,
        commands_per_second: DEFAULT_COMMANDS_PER_SECOND,
        command_rate_window_ms: DEFAULT_COMMAND_RATE_WINDOW_MS,
        max_commit_commands_per_transaction: 256,
        default_map_width_tiles: DEFAULT_MAP_WIDTH_TILES,
        default_map_height_tiles: DEFAULT_MAP_HEIGHT_TILES,
    })
}

/// Cheap coarse validation for the plan-01 command subset (plan §3.9 item 5).
///
/// No tile occupancy, resources or rule legality — those are sim-side.
pub fn validate_kind(kind: &CommandKind, map_width_tiles: i32, map_height_tiles: i32) -> Result<(), String> {
    let in_bounds = |x: i32, y: i32| x >= 0 && y >= 0 && x < map_width_tiles && y < map_height_tiles;
    match kind {
        CommandKind::Noop | CommandKind::Ping(_) => Ok(()),
        CommandKind::PlaceBlock(PlaceBlock {
            x,
            y,
            block_id,
            rotation,
            config: _,
        }) => {
            if !in_bounds(*x, *y) {
                return Err(format!("({x}, {y}) is outside the match bounds"));
            }
            if *block_id == 0 {
                return Err("block_id must not be 0".to_string());
            }
            if *rotation > 3 {
                return Err(format!("rotation {rotation} must be 0..=3"));
            }
            Ok(())
        }
        CommandKind::BreakBlock(BreakBlock { x, y }) => {
            if !in_bounds(*x, *y) {
                return Err(format!("({x}, {y}) is outside the match bounds"));
            }
            Ok(())
        }
        CommandKind::ConfigBlock(ConfigBlock { x, y, config: _ }) => {
            if !in_bounds(*x, *y) {
                return Err(format!("({x}, {y}) is outside the match bounds"));
            }
            Ok(())
        }
    }
}

/// Rate gate + per-sender sequence assignment (plan §3.9 items 2/6).
///
/// Returns the assigned `sender_seq`. Called exactly once per accepted command
/// and rolled back with the reducer on later validation failure.
pub fn rate_allow(ctx: &ReducerContext, config: &RelayConfig) -> Result<u64, String> {
    let identity = ctx.sender();
    let now = ctx.timestamp;
    let now_micros = now.to_micros_since_unix_epoch();

    let existing = ctx.db.command_rate().identity().find(identity);
    let had_row = existing.is_some();
    let mut rate = match existing {
        Some(rate) => rate,
        None => CommandRate {
            identity,
            window_start: now,
            count: 0,
            last_sender_seq: 0,
        },
    };
    if window_expired(now_micros, rate.window_start.to_micros_since_unix_epoch(), config.command_rate_window_ms) {
        rate.window_start = now;
        rate.count = 0;
    }
    if rate.count >= config.commands_per_second {
        return Err(format!(
            "rate limit exceeded ({} commands per {} ms)",
            config.commands_per_second, config.command_rate_window_ms
        ));
    }
    let next_seq =
        next_sender_seq(rate.last_sender_seq).ok_or_else(|| "sender sequence overflow".to_string())?;
    rate.count = rate.count.saturating_add(1);
    rate.last_sender_seq = next_seq;
    if had_row {
        ctx.db.command_rate().identity().update(rate);
    } else {
        ctx.db.command_rate().insert(rate);
    }
    Ok(next_seq)
}

/// Match existence + status gate shared by command/lifecycle reducers.
pub fn require_match(ctx: &ReducerContext, match_id: u64) -> Result<RelayMatch, String> {
    ctx.db
        .relay_match()
        .match_id()
        .find(match_id)
        .ok_or_else(|| format!("unknown match {match_id}"))
}

/// Requires a match in `status`.
pub fn require_status(match_row: &RelayMatch, status: MatchStatus) -> Result<(), String> {
    if match_row.status != status {
        return Err(format!(
            "match {} is {:?}, expected {status:?}",
            match_row.match_id, match_row.status
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(x: i32, y: i32, block_id: u16, rotation: u8) -> CommandKind {
        CommandKind::PlaceBlock(PlaceBlock {
            x,
            y,
            block_id,
            rotation,
            config: 0,
        })
    }

    #[test]
    fn validate_kind_rejects_out_of_bounds_and_zero_block() {
        assert!(validate_kind(&CommandKind::Ping(1), 100, 100).is_ok());
        assert!(validate_kind(&CommandKind::Noop, 100, 100).is_ok());
        assert!(validate_kind(&place(5, 6, 7, 3), 100, 100).is_ok());
        assert!(validate_kind(&place(100, 0, 7, 0), 100, 100).is_err());
        assert!(validate_kind(&place(0, 100, 7, 0), 100, 100).is_err());
        assert!(validate_kind(&place(1, 1, 0, 0), 100, 100).is_err());
        assert!(validate_kind(&place(1, 1, 1, 4), 100, 100).is_err());
        assert!(
            validate_kind(&CommandKind::BreakBlock(BreakBlock { x: -1, y: 0 }), 100, 100)
                .is_err()
        );
        assert!(
            validate_kind(
                &CommandKind::ConfigBlock(ConfigBlock {
                    x: 0,
                    y: 99,
                    config: 3
                }),
                100,
                100
            )
            .is_ok()
        );
    }

    #[test]
    fn rate_window_rolls_after_window() {
        assert!(!window_expired(1_000_000, 0, 1_000));
        assert!(!window_expired(999_999, 0, 1_000));
        assert!(window_expired(1_000_000, 0, 1_000));
        assert!(window_expired(2_000_000, 1_000_000, 1_000));
        // Clock regression must not roll the window.
        assert!(!window_expired(0, 1_000_000, 1_000));
    }

    #[test]
    fn sender_sequence_must_increase() {
        assert_eq!(next_sender_seq(0), Some(1));
        assert_eq!(next_sender_seq(41), Some(42));
        assert_eq!(next_sender_seq(u64::MAX), None);
    }
}

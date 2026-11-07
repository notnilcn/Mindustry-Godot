// SPDX-License-Identifier: GPL-3.0-only

//! Player-state LWW stream (plan 21 §3.6/§6.1).
//!
//! One row per `(match_id, identity)`; `report_player_state` upserts the
//! caller's row after cheap checks (membership, monotonic `seq`, finite
//! floats, clock-skew window, 25 Hz rate). Under D2 the owner is authoritative
//! for these fields (client-owned possessed unit, `SyncLocal`-equivalent), so
//! they are masked out of cross-peer checksum comparison (§3.2/§6.5).

use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, Timestamp, reducer, table};

use super::methods::{is_finite, require_match, require_member, window_expired};
use super::tables::{MatchStatus, match_state};
use crate::main::global::{
    DEFAULT_PLAYER_STATE_PER_SECOND, DEFAULT_RATE_WINDOW_MS, MAX_PLAYER_SPEED,
    MAX_SELECTED_BLOCK_LEN, PLAYER_POSITION_MARGIN_PX,
};

/// The `NetClient.sync()` clientSnapshot payload (plan §6.1). Product struct so
/// the reducer signature stays readable; the field order is not ABI (client
/// bindings are regenerated), but the field *names* mirror the plan table.
#[derive(SpacetimeType, Clone, PartialEq, Debug)]
pub struct PlayerStateReport {
    /// Strictly increasing sender sequence (`u32` wraps after ~2.7 years @15 Hz).
    pub seq: u32,
    /// Possessed unit entity id (`-1` = none/dead).
    pub unit_id: i32,
    /// Whether the player unit is dead (awaiting respawn).
    pub dead: bool,
    /// Unit x (world units).
    pub x: f32,
    /// Unit y.
    pub y: f32,
    /// Unit velocity x.
    pub vx: f32,
    /// Unit velocity y.
    pub vy: f32,
    /// Aim pointer x.
    pub pointer_x: f32,
    /// Aim pointer y.
    pub pointer_y: f32,
    /// Unit rotation (degrees).
    pub rotation: f32,
    /// Base rotation (build orientation).
    pub base_rotation: f32,
    /// Mining tile x (`-1` = none).
    pub mining_x: i16,
    /// Mining tile y (`-1` = none).
    pub mining_y: i16,
    /// Boosting flag (input).
    pub boosting: bool,
    /// Shooting flag (input).
    pub shooting: bool,
    /// Chatting flag (input).
    pub chatting: bool,
    /// Building flag (input).
    pub building: bool,
    /// Selected block content name, if any.
    pub selected_block: Option<String>,
    /// Selected rotation `0..=3`.
    pub selected_rotation: u8,
    /// Camera/view x.
    pub view_x: f32,
    /// Camera/view y.
    pub view_y: f32,
    /// Camera/view width.
    pub view_width: f32,
    /// Camera/view height.
    pub view_height: f32,
    /// Unit health.
    pub health: f32,
    /// Unit shield.
    pub shield: f32,
    /// Team id.
    pub team: u8,
}

/// LWW row keyed by `(match_id, identity)` (plan §6.1).
#[table(accessor = match_player_state, public, index(accessor = by_match_player, btree(columns = [match_id, identity])))]
pub struct MatchPlayerState {
    /// Auto-inc row id.
    #[primary_key]
    #[auto_inc]
    pub state_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Owning player.
    pub identity: Identity,
    /// Sender sequence (monotonic).
    pub seq: u32,
    /// Possessed unit entity id.
    pub unit_id: i32,
    /// Dead flag.
    pub dead: bool,
    /// Unit x.
    pub x: f32,
    /// Unit y.
    pub y: f32,
    /// Unit velocity x.
    pub vx: f32,
    /// Unit velocity y.
    pub vy: f32,
    /// Aim x.
    pub pointer_x: f32,
    /// Aim y.
    pub pointer_y: f32,
    /// Rotation.
    pub rotation: f32,
    /// Base rotation.
    pub base_rotation: f32,
    /// Mining x (`-1` = none).
    pub mining_x: i16,
    /// Mining y (`-1` = none).
    pub mining_y: i16,
    /// Boosting.
    pub boosting: bool,
    /// Shooting.
    pub shooting: bool,
    /// Chatting.
    pub chatting: bool,
    /// Building.
    pub building: bool,
    /// Selected block name.
    pub selected_block: Option<String>,
    /// Selected rotation.
    pub selected_rotation: u8,
    /// View x.
    pub view_x: f32,
    /// View y.
    pub view_y: f32,
    /// View width.
    pub view_width: f32,
    /// View height.
    pub view_height: f32,
    /// Health.
    pub health: f32,
    /// Shield.
    pub shield: f32,
    /// Team.
    pub team: u8,
    /// Server receive time.
    pub updated_at: Timestamp,
}

/// Server-only input-rate gate: one row per identity, 25 reports/s (plan §6.1).
#[table(accessor = match_player_state_rate)]
pub struct PlayerStateRate {
    /// Caller.
    #[primary_key]
    pub identity: Identity,
    /// Current window start.
    pub window_start: Timestamp,
    /// Reports in the window.
    pub count: u32,
}

/// Upserts the caller's LWW player state (plan §3.6/§6.7 step 9).
///
/// Allowed in `Lobby` (pre-spawn) and `Running`; rejected for spectators.
#[reducer]
pub fn report_player_state(
    ctx: &ReducerContext,
    match_id: u64,
    report: PlayerStateReport,
) -> Result<(), String> {
    let result = report_player_state_impl(ctx, match_id, report);
    if let Err(error) = &result {
        log::warn!("report_player_state rejected for match {match_id}: {error}");
    }
    result
}

fn report_player_state_impl(
    ctx: &ReducerContext,
    match_id: u64,
    report: PlayerStateReport,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    let member = require_member(ctx, match_id)?;
    if member.role != super::tables::MemberRole::Player {
        return Err("spectators cannot report player state".to_string());
    }
    let sim_tick = ctx
        .db
        .match_state()
        .by_match_state()
        .filter(match_id)
        .next()
        .map(|state| state.sim_tick);
    validate_player_report(&report, sim_tick)?;
    rate_limit_player_state(ctx)?;

    let existing = ctx
        .db
        .match_player_state()
        .by_match_player()
        .filter((match_id, ctx.sender()))
        .next();
    if let Some(previous) = &existing
        && !seq_supersedes(Some(previous.seq), report.seq)
    {
        return Err(format!(
            "stale player state seq {} <= {}",
            report.seq, previous.seq
        ));
    }
    // GAPS #33: `NetServer.clientSnapshot` movement guard
    // (`NetServer.java:591-746`): clamp the reported velocity to the unit speed
    // and the position to what the unit could have moved since the last report;
    // a position outside the map is ignored (Java corrects it server-side, the
    // relay has no authoritative position to correct to).
    let previous = existing.as_ref();
    let elapsed_ms = previous
        .map(|state| {
            let previous_micros = state.updated_at.to_micros_since_unix_epoch();
            u64::try_from(
                ctx.timestamp
                    .to_micros_since_unix_epoch()
                    .saturating_sub(previous_micros),
            )
            .unwrap_or(u64::MAX)
                / 1_000
        })
        .unwrap_or(0);
    let (x, y, vx, vy) = clamp_player_movement(
        &report,
        previous,
        &MovementLimits {
            width_px: row.map_width_tiles as f32 * TILE_SIZE_PX,
            height_px: row.map_height_tiles as f32 * TILE_SIZE_PX,
            elapsed_ms,
        },
    )?;
    let next = MatchPlayerState {
        state_id: existing.as_ref().map(|state| state.state_id).unwrap_or(0),
        match_id,
        identity: ctx.sender(),
        seq: report.seq,
        unit_id: report.unit_id,
        dead: report.dead,
        x,
        y,
        vx,
        vy,
        pointer_x: report.pointer_x,
        pointer_y: report.pointer_y,
        rotation: report.rotation,
        base_rotation: report.base_rotation,
        mining_x: report.mining_x,
        mining_y: report.mining_y,
        boosting: report.boosting,
        shooting: report.shooting,
        chatting: report.chatting,
        building: report.building,
        selected_block: report.selected_block,
        selected_rotation: report.selected_rotation,
        view_x: report.view_x,
        view_y: report.view_y,
        view_width: report.view_width,
        view_height: report.view_height,
        health: report.health,
        shield: report.shield,
        team: report.team,
        updated_at: ctx.timestamp,
    };
    match existing {
        Some(_) => {
            ctx.db.match_player_state().state_id().update(next);
        }
        None => {
            ctx.db.match_player_state().insert(next);
        }
    }
    Ok(())
}

/// Cheap validation shared by the reducer and unit tests (plan §6.3).
pub fn validate_player_report(
    report: &PlayerStateReport,
    match_state_sim_tick: Option<u64>,
) -> Result<(), String> {
    for (name, value) in [
        ("x", report.x),
        ("y", report.y),
        ("vx", report.vx),
        ("vy", report.vy),
        ("pointer_x", report.pointer_x),
        ("pointer_y", report.pointer_y),
        ("rotation", report.rotation),
        ("base_rotation", report.base_rotation),
        ("view_x", report.view_x),
        ("view_y", report.view_y),
        ("view_width", report.view_width),
        ("view_height", report.view_height),
        ("health", report.health),
        ("shield", report.shield),
    ] {
        if !is_finite(value) {
            return Err(format!("player state field `{name}` is not finite"));
        }
    }
    if report.selected_rotation > 3 {
        return Err(format!(
            "selected rotation {} must be 0..=3",
            report.selected_rotation
        ));
    }
    if let Some(block) = &report.selected_block
        && (block.is_empty() || block.chars().count() > MAX_SELECTED_BLOCK_LEN)
    {
        return Err(format!(
            "selected_block must be 1-{MAX_SELECTED_BLOCK_LEN} characters"
        ));
    }
    // `match_state_sim_tick` is accepted for the clock-skew seam; player state
    // carries no client tick of its own (plan §6.3 lists the skew window for
    // command variants only), so there is nothing further to check cheaply.
    let _ = match_state_sim_tick;
    Ok(())
}

/// World pixels per tile (`Vars.tilesize`).
pub const TILE_SIZE_PX: f32 = 8.0;

/// Java clamps the elapsed window to 1.5 s (`NetServer.java:684`).
pub const MAX_MOVEMENT_ELAPSED_MS: u64 = 1_500;

/// Upstream movement slack (`vector.limit(maxMove)` with `* 1.2`).
pub const MOVEMENT_SLACK: f32 = 1.2;

/// Inputs the relay movement guard needs (Java reads them from the live sim).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementLimits {
    /// World width in pixels (`map_width_tiles * 8`).
    pub width_px: f32,
    /// World height in pixels (`map_height_tiles * 8`).
    pub height_px: f32,
    /// Milliseconds since the previous report (`0` when there is none).
    pub elapsed_ms: u64,
}

/// `NetServer.clientSnapshot` movement guard (`NetServer.java:591-746`),
/// relay edition: clamps velocity and per-report displacement and rejects a
/// position outside the map rectangle.
///
/// Java reads `unit.speed()` from the live unit and limits the move to
/// `elapsed / 1000 * 60 * speed * 1.2`; the module has no unit types, so
/// [`MAX_PLAYER_SPEED`] is the ceiling. Unlike Java it cannot rewind the unit to
/// an authoritative position, so an out-of-bounds report is rejected (ignored)
/// instead of corrected.
pub fn clamp_player_movement(
    report: &PlayerStateReport,
    previous: Option<&MatchPlayerState>,
    limits: &MovementLimits,
) -> Result<(f32, f32, f32, f32), String> {
    let min_x = -PLAYER_POSITION_MARGIN_PX;
    let max_x = limits.width_px + PLAYER_POSITION_MARGIN_PX;
    let min_y = -PLAYER_POSITION_MARGIN_PX;
    let max_y = limits.height_px + PLAYER_POSITION_MARGIN_PX;
    if !(min_x..=max_x).contains(&report.x) || !(min_y..=max_y).contains(&report.y) {
        return Err(format!(
            "player position ({}, {}) is outside the map bounds [{min_x}, {max_x}]x[{min_y}, {max_y}]",
            report.x, report.y
        ));
    }
    // `unit.vel.set(xVelocity, yVelocity).limit(maxSpeed)`.
    let (vx, vy) = clamp_length(report.vx, report.vy, MAX_PLAYER_SPEED);
    let Some(previous) = previous else {
        return Ok((report.x, report.y, vx, vy));
    };
    // `long elapsed = Math.min(Time.timeSinceMillis(...), 1500)`.
    let elapsed = limits.elapsed_ms.min(MAX_MOVEMENT_ELAPSED_MS) as f32 / 1_000.0;
    let max_move = elapsed * 60.0 * MAX_PLAYER_SPEED * MOVEMENT_SLACK;
    // `vector.set(x, y).sub(unit); vector.limit(maxMove)`.
    let (dx, dy) = clamp_length(report.x - previous.x, report.y - previous.y, max_move);
    Ok((previous.x + dx, previous.y + dy, vx, vy))
}

/// Limits a 2-D vector to `max` length (Arc `Vector2.limit`).
fn clamp_length(x: f32, y: f32, max: f32) -> (f32, f32) {
    let length = x.hypot(y);
    if length > max && length > 0.0 {
        (x * max / length, y * max / length)
    } else {
        (x, y)
    }
}

/// Whether a report `seq` may replace a stored one (strictly increasing).
pub fn seq_supersedes(stored: Option<u32>, incoming: u32) -> bool {
    stored.map(|seq| incoming > seq).unwrap_or(true)
}

/// 25 Hz windowed gate; rolls on demand (mirrors `command_rate` semantics).
pub fn rate_limit_player_state(ctx: &ReducerContext) -> Result<(), String> {
    let identity = ctx.sender();
    let now = ctx.timestamp;
    let now_micros = now.to_micros_since_unix_epoch();
    let existing = ctx.db.match_player_state_rate().identity().find(identity);
    let mut rate = existing.unwrap_or(PlayerStateRate {
        identity,
        window_start: now,
        count: 0,
    });
    if window_expired(
        now_micros,
        rate.window_start.to_micros_since_unix_epoch(),
        DEFAULT_RATE_WINDOW_MS,
    ) {
        rate.window_start = now;
        rate.count = 0;
    }
    if rate.count >= DEFAULT_PLAYER_STATE_PER_SECOND {
        return Err(format!(
            "player state rate exceeded ({DEFAULT_PLAYER_STATE_PER_SECOND}/s)"
        ));
    }
    rate.count = rate.count.saturating_add(1);
    if ctx
        .db
        .match_player_state_rate()
        .identity()
        .find(identity)
        .is_some()
    {
        ctx.db.match_player_state_rate().identity().update(rate);
    } else {
        ctx.db.match_player_state_rate().insert(rate);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> PlayerStateReport {
        PlayerStateReport {
            seq: 1,
            unit_id: 7,
            dead: false,
            x: 1.0,
            y: 2.0,
            vx: 0.0,
            vy: 0.0,
            pointer_x: 3.0,
            pointer_y: 4.0,
            rotation: 90.0,
            base_rotation: 0.0,
            mining_x: -1,
            mining_y: -1,
            boosting: false,
            shooting: false,
            chatting: false,
            building: false,
            selected_block: Some("router".to_string()),
            selected_rotation: 2,
            view_x: 0.0,
            view_y: 0.0,
            view_width: 1920.0,
            view_height: 1080.0,
            health: 100.0,
            shield: 0.0,
            team: 0,
        }
    }

    #[test]
    fn finite_float_guard() {
        assert!(validate_player_report(&sample_report(), None).is_ok());
        let mut bad = sample_report();
        bad.x = f32::NAN;
        assert!(validate_player_report(&bad, None).is_err());
        let mut bad = sample_report();
        bad.shield = f32::INFINITY;
        assert!(validate_player_report(&bad, None).is_err());
    }

    #[test]
    fn seq_must_increase() {
        assert!(seq_supersedes(None, 1));
        assert!(seq_supersedes(Some(1), 2));
        assert!(!seq_supersedes(Some(2), 2));
        assert!(!seq_supersedes(Some(3), 2));
    }

    fn stored(seq: u32, x: f32, y: f32) -> MatchPlayerState {
        MatchPlayerState {
            state_id: 1,
            match_id: 1,
            identity: Identity::from_byte_array([9u8; 32]),
            seq,
            unit_id: 7,
            dead: false,
            x,
            y,
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
            view_width: 1.0,
            view_height: 1.0,
            health: 1.0,
            shield: 0.0,
            team: 1,
            updated_at: spacetimedb::Timestamp::from_micros_since_unix_epoch(0),
        }
    }

    fn limits(elapsed_ms: u64) -> MovementLimits {
        MovementLimits {
            width_px: 800.0,
            height_px: 800.0,
            elapsed_ms,
        }
    }

    #[test]
    fn velocity_is_clamped_to_the_speed_ceiling() {
        let mut report = sample_report();
        report.vx = 1_000.0;
        report.vy = 0.0;
        let (_, _, vx, vy) = clamp_player_movement(&report, None, &limits(0)).unwrap();
        assert!((vx - MAX_PLAYER_SPEED).abs() < 1e-4);
        assert!(vy.abs() < 1e-4);
        // Direction is preserved when the velocity is limited.
        let mut report = sample_report();
        report.vx = 300.0;
        report.vy = 400.0;
        let (_, _, vx, vy) = clamp_player_movement(&report, None, &limits(0)).unwrap();
        assert!((vx.hypot(vy) - MAX_PLAYER_SPEED).abs() < 1e-3);
        assert!((vx / vy - 0.75).abs() < 1e-4);
    }

    #[test]
    fn movement_is_limited_to_the_elapsed_window() {
        let previous = stored(1, 100.0, 100.0);
        let mut report = sample_report();
        report.seq = 2;
        report.x = 500.0;
        report.y = 100.0;
        let (x, y, _, _) = clamp_player_movement(&report, Some(&previous), &limits(40)).unwrap();
        // 0.04 s * 60 tps * 20 units/tick * 1.2 = 57.6 px.
        assert!((x - (100.0 + 0.04 * 60.0 * MAX_PLAYER_SPEED * MOVEMENT_SLACK)).abs() < 1e-3);
        assert!((y - 100.0).abs() < 1e-4);
        // A small legitimate move passes through unchanged.
        let mut report = sample_report();
        report.x = 104.0;
        report.y = 103.0;
        let (x, y, _, _) = clamp_player_movement(&report, Some(&previous), &limits(40)).unwrap();
        assert!((x - 104.0).abs() < 1e-4);
        assert!((y - 103.0).abs() < 1e-4);
    }

    #[test]
    fn elapsed_is_capped_at_one_and_a_half_seconds() {
        let previous = stored(1, 0.0, 0.0);
        let mut report = sample_report();
        report.seq = 2;
        report.x = 99_000.0;
        report.y = 0.0;
        let (x, _, _, _) = clamp_player_movement(
            &report,
            Some(&previous),
            &MovementLimits {
                width_px: 100_000.0,
                height_px: 100_000.0,
                elapsed_ms: 60_000,
            },
        )
        .unwrap();
        let expected = 1.5 * 60.0 * MAX_PLAYER_SPEED * MOVEMENT_SLACK;
        assert!((x - expected).abs() < 1e-2, "x = {x}, expected {expected}");
    }

    #[test]
    fn out_of_bounds_positions_are_ignored() {
        let mut report = sample_report();
        report.x = 900.0; // > 800 px map + 64 px margin
        assert!(clamp_player_movement(&report, None, &limits(0)).is_err());
        let mut report = sample_report();
        report.y = -100.0; // < -64 px margin
        assert!(clamp_player_movement(&report, None, &limits(0)).is_err());
        // The margin itself is allowed.
        let mut report = sample_report();
        report.x = -PLAYER_POSITION_MARGIN_PX;
        assert!(clamp_player_movement(&report, None, &limits(0)).is_ok());
    }

    #[test]
    fn first_report_is_not_displacement_clamped() {
        let mut report = sample_report();
        report.x = 700.0;
        report.y = 700.0;
        let (x, y, _, _) = clamp_player_movement(&report, None, &limits(0)).unwrap();
        assert!((x - 700.0).abs() < 1e-4);
        assert!((y - 700.0).abs() < 1e-4);
    }

    #[test]
    fn caps_selected_block_and_rotation() {
        let mut bad = sample_report();
        bad.selected_rotation = 4;
        assert!(validate_player_report(&bad, None).is_err());
        let mut bad = sample_report();
        bad.selected_block = Some(String::new());
        assert!(validate_player_report(&bad, None).is_err());
    }
}

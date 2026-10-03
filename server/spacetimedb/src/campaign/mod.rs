// SPDX-License-Identifier: GPL-3.0-only

//! Multiplayer campaign, sector and schematic persistence (plan 21 §3.12/§6.1,
//! consuming plan 12's `*Row` shapes).
//!
//! Plan 12 owns single-player local saves; under D2 the MP campaign lives in
//! STDB so a late joiner reads campaign state from tables. Only the campaign
//! host (or a global admin) may write. Reducers return no data; clients read
//! the `my_campaign_*` views.

use spacetimedb::{Identity, ReducerContext, Table, Timestamp, ViewContext, reducer, table, view};

use std::ops::Bound;

use crate::admin::server_config_or_default;
use crate::main::global::{
    MAX_PLANET_NAME, MAX_SCHEMATIC_BASE64, MAX_SCHEMATIC_NAME, MAX_SCHEMATIC_TAGS_JSON,
    MAX_SECTOR_INFO_JSON, MAX_STAT_KEY, MAX_UNLOCK_REQUIREMENTS_JSON,
};
use crate::main::tables::AuditKind;
use crate::relay::methods::is_admin;
use crate::relay::tables::{CommandKind, RelayMatch};

/// One multiplayer campaign (plan §6.1; 12's campaign row shape).
#[table(accessor = campaign, public, index(accessor = by_campaign_host, btree(columns = [host])))]
pub struct Campaign {
    #[primary_key]
    #[auto_inc]
    pub campaign_id: u64,
    pub host: Identity,
    pub planet: String,
    pub turn: u32,
    pub seconds: u64,
    pub turn_counter: u32,
    pub rules_json: String,
    pub rules_epoch: u32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Per-sector campaign state (plan §6.1; 12's `SectorInfoRow`).
#[table(accessor = sector_info, public, index(accessor = by_sector_campaign, btree(columns = [campaign_id, sector_id])))]
pub struct SectorInfo {
    #[primary_key]
    #[auto_inc]
    pub sector_row_id: u64,
    pub campaign_id: u64,
    pub planet: String,
    pub sector_id: u32,
    pub info_json: String,
    pub wave: i32,
    pub win_wave: i32,
    pub waves: bool,
    pub attack: bool,
    pub minutes_captured: f32,
    pub playtime: u64,
    pub spawn_position: i32,
    pub last_preset_name: String,
    pub was_captured: bool,
    pub updated_at: Timestamp,
}

/// Research/content unlock state (plan §6.1; 12's `UnlockRow`).
#[table(accessor = unlock, public, index(accessor = by_unlock_campaign, btree(columns = [campaign_id, content_name])))]
pub struct Unlock {
    #[primary_key]
    #[auto_inc]
    pub unlock_id: u64,
    pub campaign_id: u64,
    pub content_name: String,
    pub unlocked: bool,
    pub requirements_json: String,
    pub updated_at: Timestamp,
}

/// Aggregated campaign statistic (plan §6.1; 12's `CampaignStatsRow`).
#[table(accessor = campaign_stats, public, index(accessor = by_stats_campaign, btree(columns = [campaign_id, kind, key])))]
pub struct CampaignStat {
    #[primary_key]
    #[auto_inc]
    pub stat_id: u64,
    pub campaign_id: u64,
    pub kind: u8,
    pub key: String,
    pub value: i64,
    pub updated_at: Timestamp,
}

/// Personal schematic (plan §3.12.2; base64 `.msch`, tags JSON).
#[table(accessor = schematic, public, index(accessor = by_schematic_owner, btree(columns = [owner, updated_at])))]
pub struct Schematic {
    #[primary_key]
    #[auto_inc]
    pub schematic_id: u64,
    pub owner: Identity,
    pub name: String,
    pub base64: String,
    pub tags_json: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

// ---- pure helpers -----------------------------------------------------------

/// Validates a schematic payload (name non-empty/capped, base64 capped).
pub fn validate_schematic(name: &str, base64: &str, tags_json: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_SCHEMATIC_NAME {
        return Err(format!(
            "schematic name must be 1-{MAX_SCHEMATIC_NAME} characters"
        ));
    }
    if base64.len() > MAX_SCHEMATIC_BASE64 {
        return Err(format!("schematic exceeds {MAX_SCHEMATIC_BASE64} bytes"));
    }
    if tags_json.len() > MAX_SCHEMATIC_TAGS_JSON {
        return Err(format!(
            "schematic tags exceed {MAX_SCHEMATIC_TAGS_JSON} bytes"
        ));
    }
    Ok(())
}

/// Validates a sector `info_json` payload.
pub fn validate_sector_info(info_json: &str) -> Result<(), String> {
    if info_json.len() > MAX_SECTOR_INFO_JSON {
        return Err(format!("sector info exceeds {MAX_SECTOR_INFO_JSON} bytes"));
    }
    Ok(())
}

/// Validates unlock requirements JSON.
pub fn validate_requirements(requirements_json: &str) -> Result<(), String> {
    if requirements_json.len() > MAX_UNLOCK_REQUIREMENTS_JSON {
        return Err(format!(
            "unlock requirements exceed {MAX_UNLOCK_REQUIREMENTS_JSON} bytes"
        ));
    }
    Ok(())
}

/// Adds `delta` to an aggregated stat value (`add_campaign_stat`).
pub fn merge_stat(existing: Option<i64>, delta: i64) -> i64 {
    existing.unwrap_or(0).saturating_add(delta)
}

/// Advances a campaign turn counter (`advance_turn`).
pub fn next_turn(turn: u32, turn_counter: u32) -> (u32, u32) {
    (turn.saturating_add(1), turn_counter.saturating_add(1))
}

// ---- reducer plumbing -------------------------------------------------------

fn require_campaign(ctx: &ReducerContext, campaign_id: u64) -> Result<Campaign, String> {
    ctx.db
        .campaign()
        .campaign_id()
        .find(campaign_id)
        .ok_or_else(|| format!("unknown campaign {campaign_id}"))
}

fn require_campaign_writer(ctx: &ReducerContext, campaign: &Campaign) -> Result<(), String> {
    if campaign.host == ctx.sender() || is_admin(ctx) {
        Ok(())
    } else {
        Err("only the campaign host may write".to_string())
    }
}

fn audit_campaign(ctx: &ReducerContext, message: impl Into<String>) {
    crate::main::audit::audit(ctx, Some(ctx.sender()), AuditKind::ConfigChange, message);
}

// ---- reducers ---------------------------------------------------------------

/// Creates a campaign owned by the caller.
#[reducer]
pub fn create_campaign(
    ctx: &ReducerContext,
    planet: String,
    rules_json: String,
) -> Result<(), String> {
    if planet.is_empty() || planet.chars().count() > MAX_PLANET_NAME {
        return Err(format!("planet must be 1-{MAX_PLANET_NAME} characters"));
    }
    if rules_json.chars().count() > crate::main::global::MAX_RULES_JSON {
        return Err("rules_json too large".to_string());
    }
    let now = ctx.timestamp;
    let row = ctx.db.campaign().insert(Campaign {
        campaign_id: 0,
        host: ctx.sender(),
        planet: planet.clone(),
        turn: 0,
        seconds: 0,
        turn_counter: 0,
        rules_json,
        rules_epoch: 1,
        created_at: now,
        updated_at: now,
    });
    audit_campaign(ctx, format!("campaign {} planet={planet}", row.campaign_id));
    Ok(())
}

/// Upserts one sector's persisted state (host/admin only).
#[reducer]
#[allow(clippy::too_many_arguments)]
pub fn save_sector_info(
    ctx: &ReducerContext,
    campaign_id: u64,
    planet: String,
    sector_id: u32,
    info_json: String,
    wave: i32,
    win_wave: i32,
    waves: bool,
    attack: bool,
    minutes_captured: f32,
    playtime: u64,
    spawn_position: i32,
    last_preset_name: String,
    was_captured: bool,
) -> Result<(), String> {
    let campaign = require_campaign(ctx, campaign_id)?;
    require_campaign_writer(ctx, &campaign)?;
    validate_sector_info(&info_json)?;
    let now = ctx.timestamp;
    let existing = ctx
        .db
        .sector_info()
        .by_sector_campaign()
        .filter((campaign_id, sector_id))
        .next();
    let row = SectorInfo {
        sector_row_id: existing.as_ref().map(|row| row.sector_row_id).unwrap_or(0),
        campaign_id,
        planet,
        sector_id,
        info_json,
        wave,
        win_wave,
        waves,
        attack,
        minutes_captured,
        playtime,
        spawn_position,
        last_preset_name,
        was_captured,
        updated_at: now,
    };
    match existing {
        Some(_) => {
            ctx.db.sector_info().sector_row_id().update(row);
        }
        None => {
            ctx.db.sector_info().insert(row);
        }
    }
    ctx.db.campaign().campaign_id().update(Campaign {
        updated_at: now,
        ..campaign
    });
    audit_campaign(
        ctx,
        format!("save sector {sector_id} of campaign {campaign_id}"),
    );
    Ok(())
}

/// Upserts one content unlock (host/admin only).
#[reducer]
pub fn set_unlock(
    ctx: &ReducerContext,
    campaign_id: u64,
    content_name: String,
    unlocked: bool,
    requirements_json: String,
) -> Result<(), String> {
    let campaign = require_campaign(ctx, campaign_id)?;
    require_campaign_writer(ctx, &campaign)?;
    if content_name.is_empty() || content_name.chars().count() > 100 {
        return Err("content name must be 1-100 characters".to_string());
    }
    validate_requirements(&requirements_json)?;
    upsert_unlock(
        ctx,
        campaign_id,
        &content_name,
        unlocked,
        &requirements_json,
    );
    audit_campaign(
        ctx,
        format!("unlock {content_name}={unlocked} in campaign {campaign_id}"),
    );
    Ok(())
}

/// Adds `value` to a campaign statistic (host/admin only).
#[reducer]
pub fn add_campaign_stat(
    ctx: &ReducerContext,
    campaign_id: u64,
    kind: u8,
    key: String,
    delta: i64,
) -> Result<(), String> {
    let campaign = require_campaign(ctx, campaign_id)?;
    require_campaign_writer(ctx, &campaign)?;
    if key.is_empty() || key.chars().count() > MAX_STAT_KEY {
        return Err(format!("stat key must be 1-{MAX_STAT_KEY} characters"));
    }
    let existing = ctx
        .db
        .campaign_stats()
        .by_stats_campaign()
        .filter((campaign_id, kind, key.as_str()))
        .next();
    let value = merge_stat(existing.as_ref().map(|row| row.value), delta);
    match existing {
        Some(row) => {
            ctx.db.campaign_stats().stat_id().update(CampaignStat {
                value,
                updated_at: ctx.timestamp,
                ..row
            });
        }
        None => {
            ctx.db.campaign_stats().insert(CampaignStat {
                stat_id: 0,
                campaign_id,
                kind,
                key,
                value,
                updated_at: ctx.timestamp,
            });
        }
    }
    Ok(())
}

/// Advances the campaign turn counter (host/admin only).
#[reducer]
pub fn advance_turn(ctx: &ReducerContext, campaign_id: u64) -> Result<(), String> {
    let campaign = require_campaign(ctx, campaign_id)?;
    require_campaign_writer(ctx, &campaign)?;
    let (turn, turn_counter) = next_turn(campaign.turn, campaign.turn_counter);
    ctx.db.campaign().campaign_id().update(Campaign {
        turn,
        turn_counter,
        updated_at: ctx.timestamp,
        ..campaign
    });
    audit_campaign(
        ctx,
        format!("advance campaign {campaign_id} to turn {turn}"),
    );
    Ok(())
}

/// Creates or replaces the caller's schematic by name.
#[reducer]
pub fn save_schematic(
    ctx: &ReducerContext,
    name: String,
    base64: String,
    tags_json: String,
) -> Result<(), String> {
    save_schematic_impl(ctx, name, base64, tags_json)
}

/// Imports a schematic (same semantics as [`save_schematic`]).
#[reducer]
pub fn import_schematic(
    ctx: &ReducerContext,
    name: String,
    base64: String,
    tags_json: String,
) -> Result<(), String> {
    save_schematic_impl(ctx, name, base64, tags_json)
}

fn save_schematic_impl(
    ctx: &ReducerContext,
    name: String,
    base64: String,
    tags_json: String,
) -> Result<(), String> {
    let name = name.trim().to_string();
    validate_schematic(&name, &base64, &tags_json)?;
    let now = ctx.timestamp;
    let existing = ctx
        .db
        .schematic()
        .by_schematic_owner()
        .filter(ctx.sender())
        .filter(|row| row.name == name)
        .next();
    match existing {
        Some(row) => {
            ctx.db.schematic().schematic_id().update(Schematic {
                base64,
                tags_json,
                updated_at: now,
                ..row
            });
        }
        None => {
            ctx.db.schematic().insert(Schematic {
                schematic_id: 0,
                owner: ctx.sender(),
                name,
                base64,
                tags_json,
                created_at: now,
                updated_at: now,
            });
        }
    }
    Ok(())
}

/// Deletes one of the caller's schematics.
#[reducer]
pub fn delete_schematic(ctx: &ReducerContext, schematic_id: u64) -> Result<(), String> {
    let Some(row) = ctx.db.schematic().schematic_id().find(schematic_id) else {
        return Err(format!("unknown schematic {schematic_id}"));
    };
    if row.owner != ctx.sender() && !is_admin(ctx) {
        return Err("Cannot delete another player's schematic.".to_string());
    }
    ctx.db.schematic().schematic_id().delete(schematic_id);
    Ok(())
}

/// Persists the observable campaign effects of a sim-visible command (plan
/// §3.12.1: campaign rows are written on the same ordered command, host only).
///
/// Called from [`crate::relay::reducers::send_match_command_impl`] after the
/// host/admin gate. A match with no `campaign_id` is a no-op.
pub(crate) fn persist_from_command(
    ctx: &ReducerContext,
    row: &RelayMatch,
    kind: &CommandKind,
) -> Result<(), String> {
    let Some(campaign_id) = row.campaign_id else {
        return Ok(());
    };
    let Some(campaign) = ctx.db.campaign().campaign_id().find(campaign_id) else {
        return Ok(());
    };
    if campaign.host != ctx.sender() && !is_admin(ctx) {
        return Ok(());
    }
    match kind {
        CommandKind::ResearchUnlock(unlock) => {
            upsert_unlock(ctx, campaign_id, &unlock.content, true, "[]");
        }
        CommandKind::SectorCapture => {
            if let Some(sector_id) = row.sector_id {
                if let Some(mut sector) = ctx
                    .db
                    .sector_info()
                    .by_sector_campaign()
                    .filter((campaign_id, sector_id))
                    .next()
                {
                    sector.was_captured = true;
                    sector.updated_at = ctx.timestamp;
                    ctx.db.sector_info().sector_row_id().update(sector);
                }
            }
        }
        CommandKind::SaveSector => {
            let (turn, turn_counter) = next_turn(campaign.turn, campaign.turn_counter);
            ctx.db.campaign().campaign_id().update(Campaign {
                turn,
                turn_counter,
                updated_at: ctx.timestamp,
                ..campaign
            });
        }
        _ => {}
    }
    Ok(())
}

fn upsert_unlock(
    ctx: &ReducerContext,
    campaign_id: u64,
    content_name: &str,
    unlocked: bool,
    requirements_json: &str,
) {
    let existing = ctx
        .db
        .unlock()
        .by_unlock_campaign()
        .filter((campaign_id, content_name))
        .next();
    let now = ctx.timestamp;
    match existing {
        Some(row) => {
            ctx.db.unlock().unlock_id().update(Unlock {
                unlocked,
                requirements_json: requirements_json.to_string(),
                updated_at: now,
                ..row
            });
        }
        None => {
            ctx.db.unlock().insert(Unlock {
                unlock_id: 0,
                campaign_id,
                content_name: content_name.to_string(),
                unlocked,
                requirements_json: requirements_json.to_string(),
                updated_at: now,
            });
        }
    }
}

/// Whether the auto-pause policy should pause a game (plan §3.11).
///
/// Dedicated servers pause when `player_count == 0`; PvP waiting rooms pause
/// while fewer than two players are present.
pub fn should_auto_pause(auto_pause: bool, player_count: u16, pvp: bool) -> bool {
    if !auto_pause {
        return false;
    }
    if pvp {
        player_count < 2
    } else {
        player_count == 0
    }
}

// ---- views ------------------------------------------------------------------

fn my_campaign_ids(ctx: &ViewContext) -> Vec<u64> {
    ctx.db
        .campaign()
        .by_campaign_host()
        .filter(ctx.sender())
        .map(|row| row.campaign_id)
        .collect()
}

/// Campaigns the caller hosts (plan §6.2).
#[view(accessor = my_campaigns, public)]
fn my_campaigns(ctx: &ViewContext) -> Vec<Campaign> {
    ctx.db
        .campaign()
        .by_campaign_host()
        .filter(ctx.sender())
        .collect()
}

/// Sector rows for the caller's campaigns (plan §6.2).
#[view(accessor = my_campaign_sectors, public)]
fn my_campaign_sectors(ctx: &ViewContext) -> Vec<SectorInfo> {
    let ids = my_campaign_ids(ctx);
    ctx.db
        .sector_info()
        .by_sector_campaign()
        .filter((Bound::<u64>::Unbounded, Bound::<u64>::Unbounded))
        .filter(|row| ids.contains(&row.campaign_id))
        .collect()
}

/// Unlock rows for the caller's campaigns (plan §6.2).
#[view(accessor = my_campaign_unlocks, public)]
fn my_campaign_unlocks(ctx: &ViewContext) -> Vec<Unlock> {
    let ids = my_campaign_ids(ctx);
    ctx.db
        .unlock()
        .by_unlock_campaign()
        .filter((Bound::<u64>::Unbounded, Bound::<u64>::Unbounded))
        .filter(|row| ids.contains(&row.campaign_id))
        .collect()
}

/// Stat rows for the caller's campaigns (plan §6.2).
#[view(accessor = my_campaign_stats, public)]
fn my_campaign_stats(ctx: &ViewContext) -> Vec<CampaignStat> {
    let ids = my_campaign_ids(ctx);
    ctx.db
        .campaign_stats()
        .by_stats_campaign()
        .filter((Bound::<u64>::Unbounded, Bound::<u64>::Unbounded))
        .filter(|row| ids.contains(&row.campaign_id))
        .collect()
}

/// The caller's own schematics (plan §6.2).
#[view(accessor = my_schematics, public)]
fn my_schematics(ctx: &ViewContext) -> Vec<Schematic> {
    ctx.db
        .schematic()
        .by_schematic_owner()
        .filter(ctx.sender())
        .collect()
}

/// Kept so `server_config_or_default` is available to the campaign surface and
/// future per-campaign caps (currently the module-level defaults apply).
#[allow(dead_code)]
pub(crate) fn config(ctx: &ReducerContext) -> crate::admin::ServerConfig {
    server_config_or_default(ctx)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn schematic_validation_caps() {
        assert!(validate_schematic("base", "AAAA", "{}").is_ok());
        assert!(validate_schematic("", "AAAA", "{}").is_err());
        assert!(validate_schematic("base", &"x".repeat(MAX_SCHEMATIC_BASE64 + 1), "{}").is_err());
        assert!(
            validate_schematic("base", "AAAA", &"x".repeat(MAX_SCHEMATIC_TAGS_JSON + 1)).is_err()
        );
    }

    #[test]
    fn sector_and_requirement_caps() {
        assert!(validate_sector_info("{}").is_ok());
        assert!(validate_sector_info(&"x".repeat(MAX_SECTOR_INFO_JSON + 1)).is_err());
        assert!(validate_requirements("[]").is_ok());
        assert!(validate_requirements(&"x".repeat(MAX_UNLOCK_REQUIREMENTS_JSON + 1)).is_err());
    }

    #[test]
    fn stat_merge_and_turn_advance() {
        assert_eq!(merge_stat(None, 5), 5);
        assert_eq!(merge_stat(Some(5), -2), 3);
        assert_eq!(merge_stat(Some(i64::MAX), 1), i64::MAX);
        assert_eq!(next_turn(3, 7), (4, 8));
    }

    #[test]
    fn auto_pause_policy() {
        assert!(!should_auto_pause(false, 0, false));
        assert!(should_auto_pause(true, 0, false));
        assert!(!should_auto_pause(true, 1, false));
        assert!(should_auto_pause(true, 1, true));
        assert!(!should_auto_pause(true, 2, true));
    }
}

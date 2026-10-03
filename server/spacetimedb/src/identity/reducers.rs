// SPDX-License-Identifier: GPL-3.0-only

//! Identity/lobby reducers (plan 01 §2.1/§3.9). `ctx.sender()` is the only
//! principal; each returns `Result<(), String>` and performs cheap validation
//! only. Failed reducers roll back, so rejections log instead of auditing (R7).

use spacetimedb::{ReducerContext, Table, reducer};

use super::methods::{
    validate_keybinds_json, validate_language, validate_profile_name, validate_ui_scale,
    validate_username, validate_volume,
};
use super::tables::{
    ClientSettings, Player, PlayerProfile, client_settings, player, player_profile,
};
use crate::main::audit::audit;
use crate::main::tables::AuditKind;

/// Sets the caller's username.
///
/// The `player` row is created by `client_connected` before any client reducer
/// can run; a missing row is a client bug and is rejected.
#[reducer]
pub fn set_username(ctx: &ReducerContext, username: String) -> Result<(), String> {
    validate_username(&username)?;
    match ctx.db.player().identity().find(ctx.sender()) {
        Some(player) => {
            ctx.db
                .player()
                .identity()
                .update(Player { username, ..player });
            Ok(())
        }
        None => {
            log::warn!("set_username rejected: no player row for caller");
            Err("no player row for caller".to_string())
        }
    }
}

/// Creates a named profile owned by the caller. Profile names are not unique
/// (plan 12 owns selection semantics); the auto-increment ID is the identity.
#[reducer]
pub fn create_profile(ctx: &ReducerContext, name: String) -> Result<(), String> {
    validate_profile_name(&name)?;
    if ctx.db.player().identity().find(ctx.sender()).is_none() {
        log::warn!("create_profile rejected: no player row for caller");
        return Err("no player row for caller".to_string());
    }
    let profile = ctx.db.player_profile().insert(PlayerProfile {
        profile_id: 0,
        identity: ctx.sender(),
        name: name.clone(),
        created_at: ctx.timestamp,
    });
    audit(
        ctx,
        Some(ctx.sender()),
        AuditKind::ProfileCreate,
        format!("profile {} `{name}`", profile.profile_id),
    );
    Ok(())
}

/// Upserts the caller's opaque client settings row (plan 14 owns the surface);
/// every call bumps `revision`.
#[reducer]
pub fn update_client_settings(
    ctx: &ReducerContext,
    ui_scale: f32,
    language: String,
    music_volume: f32,
    sfx_volume: f32,
    keybinds_json: String,
) -> Result<(), String> {
    validate_ui_scale(ui_scale)?;
    validate_language(&language)?;
    validate_volume(music_volume)?;
    validate_volume(sfx_volume)?;
    validate_keybinds_json(&keybinds_json)?;

    let revision = match ctx.db.client_settings().identity().find(ctx.sender()) {
        Some(existing) => existing.revision.saturating_add(1),
        None => 1,
    };
    let row = ClientSettings {
        identity: ctx.sender(),
        ui_scale,
        language,
        music_volume,
        sfx_volume,
        keybinds_json,
        revision,
        updated_at: ctx.timestamp,
    };
    match ctx.db.client_settings().identity().find(ctx.sender()) {
        Some(_) => {
            ctx.db.client_settings().identity().update(row);
        }
        None => {
            ctx.db.client_settings().insert(row);
        }
    }
    audit(
        ctx,
        Some(ctx.sender()),
        AuditKind::ConfigChange,
        format!("client settings revision {revision}"),
    );
    Ok(())
}

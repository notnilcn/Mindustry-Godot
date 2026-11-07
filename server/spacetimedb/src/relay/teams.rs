// SPDX-License-Identifier: GPL-3.0-only

//! PvP team assignment (GAPS #33 / PAR-0545): the relay half of
//! `NetServer.assigner` / `NetServer.assignTeam`.
//!
//! Java's assigner (`core/src/mindustry/core/NetServer.java:50-71`) reads the
//! live server world: for `rules.pvp` it picks the active team with the fewest
//! players, skipping the wave team (while waves run), inactive teams
//! (`TeamData.active()`, i.e. a core exists) and `Team.derelict`; otherwise it
//! returns `rules.defaultTeam`. `NetServer.assignTeam(player)` runs on
//! connection (`NetServer.java:289-291`).
//!
//! A relay match has no server-side world, so the host publishes the team
//! liveness it observes (`match_team`, [`publish_match_teams`]) and the module
//! balances over `relay_member` team counts. Assignment happens on
//! [`crate::relay::reducers::join_match`] (a joiner) and
//! [`crate::relay::reducers::start_match`] (members still unassigned, e.g. the
//! host that created the lobby before teams were published). Non-PvP matches
//! stay unassigned: `relay_member.team = None` means "use the world's default
//! team", which is the client's `rules.defaultTeam` (`mind-gdext` resolves it
//! through `MemberOrigin`/`player_team()`).
//!
//! Rows are replaced per match by the host, so the table stays bounded.

use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, Timestamp, reducer, table};

use super::methods::{is_host_or_admin, require_match};
use super::tables::{Gamemode, MatchStatus, RelayMember, relay_member};
use crate::main::global::{MAX_TEAMS, TEAM_DERELICT};

/// Host-published liveness of one team (the `TeamData` inputs the assigner
/// reads: `active()` and the wave-team rule).
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub struct TeamAssignment {
    /// Team id (`0..MAX_TEAMS`).
    pub team: u8,
    /// `TeamData.active()`: the team has a core in the host world.
    pub active: bool,
    /// `Team.rules().protectCores` (part of the PvP balancer filter in the
    /// client roster port; kept explicit so host and module agree).
    pub protect_cores: bool,
    /// `rules.waves && team == rules.waveTeam`: the AI wave team, never a
    /// player team while waves run.
    pub wave_team: bool,
}

/// One host-published team liveness row (public match data, like `match_state`).
#[table(accessor = match_team, public, index(accessor = by_match_team, btree(columns = [match_id, team])))]
pub struct MatchTeam {
    /// Auto-inc row id.
    #[primary_key]
    #[auto_inc]
    pub team_row_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Team id.
    pub team: u8,
    /// `TeamData.active()`.
    pub active: bool,
    /// `Team.rules().protectCores`.
    pub protect_cores: bool,
    /// `rules.waves && team == rules.waveTeam`.
    pub wave_team: bool,
    /// Publisher.
    pub updated_by: Identity,
    /// Server receive time.
    pub updated_at: Timestamp,
}

/// Replaces the match's team liveness rows (host/admin; GAPS #33).
///
/// The host publishes what its world shows; the module never simulates cores.
#[reducer]
pub fn publish_match_teams(
    ctx: &ReducerContext,
    match_id: u64,
    teams: Vec<TeamAssignment>,
) -> Result<(), String> {
    let result = publish_match_teams_impl(ctx, match_id, teams);
    if let Err(error) = &result {
        log::warn!("publish_match_teams rejected for match {match_id}: {error}");
    }
    result
}

fn publish_match_teams_impl(
    ctx: &ReducerContext,
    match_id: u64,
    teams: Vec<TeamAssignment>,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    if !is_host_or_admin(ctx, &row) {
        return Err("only the host or an admin may publish team data".to_string());
    }
    validate_teams(&teams)?;
    let existing: Vec<MatchTeam> = ctx
        .db
        .match_team()
        .by_match_team()
        .filter(match_id)
        .collect();
    for old in existing {
        ctx.db.match_team().team_row_id().delete(old.team_row_id);
    }
    let now = ctx.timestamp;
    for team in teams {
        ctx.db.match_team().insert(MatchTeam {
            team_row_id: 0,
            match_id,
            team: team.team,
            active: team.active,
            protect_cores: team.protect_cores,
            wave_team: team.wave_team,
            updated_by: ctx.sender(),
            updated_at: now,
        });
    }
    Ok(())
}

/// Cheap validation shared by the reducer and unit tests.
pub fn validate_teams(teams: &[TeamAssignment]) -> Result<(), String> {
    if teams.len() > MAX_TEAMS {
        return Err(format!("at most {MAX_TEAMS} teams may be published"));
    }
    for (index, team) in teams.iter().enumerate() {
        if usize::from(team.team) >= MAX_TEAMS {
            return Err(format!("team id {} must be below {MAX_TEAMS}", team.team));
        }
        if teams[..index].iter().any(|other| other.team == team.team) {
            return Err(format!("team {} is published twice", team.team));
        }
    }
    Ok(())
}

/// `NetServer.assigner` (`NetServer.java:50-71`), relay edition.
///
/// `member_counts` holds the connected-member count per team; a missing team
/// counts as zero. Returns `None` when no candidate survives the filter (Java
/// returns `null`, and the client keeps its default team).
pub fn assign_team(
    pvp: bool,
    candidates: &[TeamAssignment],
    member_counts: &[(u8, usize)],
) -> Option<u8> {
    if !pvp {
        // Java returns `rules.defaultTeam`; the relay has no world, so leave the
        // member unassigned and let the client's `player_team()` decide.
        return None;
    }
    let mut best: Option<(usize, u8)> = None;
    for candidate in candidates {
        if candidate.wave_team
            || !candidate.active
            || !candidate.protect_cores
            || candidate.team == TEAM_DERELICT
        {
            continue;
        }
        let count = member_counts
            .iter()
            .find(|(team, _)| *team == candidate.team)
            .map(|(_, count)| *count)
            .unwrap_or(0);
        let better = match best {
            Some((best_count, best_team)) => {
                // `getActive().min(...)`: first minimum in team-id order wins.
                count < best_count || (count == best_count && candidate.team < best_team)
            }
            None => true,
        };
        if better {
            best = Some((count, candidate.team));
        }
    }
    best.map(|(_, team)| team)
}

/// Connected-member counts per team for `match_id` (Java `Groups.player`).
pub fn member_counts(ctx: &ReducerContext, match_id: u64) -> Vec<(u8, usize)> {
    let mut counts: Vec<(u8, usize)> = Vec::new();
    for member in ctx.db.relay_member().by_match_identity().filter(match_id) {
        if !member.connected {
            continue; // `Groups.player` only holds connected players
        }
        let Some(team) = member.team else {
            continue;
        };
        match counts.iter_mut().find(|(id, _)| *id == team) {
            Some((_, count)) => *count += 1,
            None => counts.push((team, 1)),
        }
    }
    counts
}

/// The team a joiner should receive, or `None` when the mode does not balance.
pub fn assigned_team_for_join(ctx: &ReducerContext, match_id: u64, pvp: bool) -> Option<u8> {
    let candidates: Vec<TeamAssignment> = ctx
        .db
        .match_team()
        .by_match_team()
        .filter(match_id)
        .map(|row| TeamAssignment {
            team: row.team,
            active: row.active,
            protect_cores: row.protect_cores,
            wave_team: row.wave_team,
        })
        .collect();
    assign_team(pvp, &candidates, &member_counts(ctx, match_id))
}

/// Fills `team` for members that joined before team data existed
/// (`start_match`; Java assigns on connect, the host publishes in the lobby).
pub fn assign_missing_teams(ctx: &ReducerContext, match_id: u64, mode: Gamemode) {
    if !is_balanced_mode(mode) {
        return;
    }
    let unassigned: Vec<RelayMember> = ctx
        .db
        .relay_member()
        .by_match_identity()
        .filter(match_id)
        .filter(|member| member.team.is_none())
        .collect();
    for member in unassigned {
        // Recompute per member: the previous update changed the counts.
        let Some(team) = assigned_team_for_join(ctx, match_id, true) else {
            return;
        };
        ctx.db.relay_member().member_id().update(RelayMember {
            team: Some(team),
            ..member
        });
    }
}

/// Whether a match's mode balances players across teams (PVP only).
pub fn is_balanced_mode(mode: Gamemode) -> bool {
    mode == Gamemode::Pvp
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn team(team: u8, active: bool, protect_cores: bool, wave_team: bool) -> TeamAssignment {
        TeamAssignment {
            team,
            active,
            protect_cores,
            wave_team,
        }
    }

    #[test]
    fn non_pvp_stays_unassigned() {
        let teams = [team(1, true, true, false), team(2, true, true, false)];
        assert_eq!(assign_team(false, &teams, &[]), None);
        assert!(!is_balanced_mode(Gamemode::Survival));
        assert!(!is_balanced_mode(Gamemode::Attack));
        assert!(!is_balanced_mode(Gamemode::Sandbox));
        assert!(!is_balanced_mode(Gamemode::Editor));
        assert!(is_balanced_mode(Gamemode::Pvp));
    }

    #[test]
    fn pvp_balances_to_the_least_populated_active_team() {
        let teams = [
            team(1, true, true, false),
            team(2, true, true, false),
            team(3, false, true, false), // inactive: no core
            team(4, true, false, false), // not a protected-core team
            team(TEAM_DERELICT, true, true, false),
        ];
        // No members yet: the first team in id order wins the tie.
        assert_eq!(assign_team(true, &teams, &[]), Some(1));
        assert_eq!(assign_team(true, &teams, &[(1, 1)]), Some(2));
        assert_eq!(assign_team(true, &teams, &[(1, 1), (2, 1)]), Some(1));
        assert_eq!(assign_team(true, &teams, &[(1, 0), (2, 2)]), Some(1));
        // Unknown teams count as zero.
        assert_eq!(assign_team(true, &teams, &[(2, 3)]), Some(1));
    }

    #[test]
    fn wave_team_is_skipped_while_waves_run() {
        let teams = [team(1, true, true, true), team(2, true, true, false)];
        assert_eq!(assign_team(true, &teams, &[]), Some(2));
        // With waves off the host clears the flag, so the team is eligible.
        let teams = [team(1, true, true, false), team(2, true, true, true)];
        assert_eq!(assign_team(true, &teams, &[]), Some(1));
    }

    #[test]
    fn no_candidate_returns_none() {
        assert_eq!(assign_team(true, &[], &[]), None);
        assert_eq!(assign_team(true, &[team(1, false, true, false)], &[]), None);
        assert_eq!(
            assign_team(true, &[team(TEAM_DERELICT, true, true, false)], &[]),
            None
        );
    }

    #[test]
    fn team_publication_is_validated() {
        assert!(validate_teams(&[team(1, true, true, false)]).is_ok());
        assert!(validate_teams(&[]).is_ok());
        assert!(validate_teams(&[team(8, true, true, false)]).is_err());
        assert!(
            validate_teams(&[team(1, true, true, false), team(1, false, false, false)]).is_err()
        );
        let too_many: Vec<TeamAssignment> = (0..=MAX_TEAMS as u8)
            .map(|id| team(id, true, true, false))
            .collect();
        assert!(validate_teams(&too_many).is_err());
    }

    #[test]
    fn assignment_uses_counts_consistently() {
        // The same candidate set and counts must produce the same team twice.
        let teams = [team(1, true, true, false), team(2, true, true, false)];
        let counts = [(1usize, 2usize), (2, 2)];
        let counts: Vec<(u8, usize)> = counts.iter().map(|(t, c)| (*t as u8, *c)).collect();
        assert_eq!(assign_team(true, &teams, &counts), Some(1));
        assert_eq!(assign_team(true, &teams, &counts), Some(1));
    }
}

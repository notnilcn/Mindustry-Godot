// SPDX-License-Identifier: GPL-3.0-only

//! Votekick sessions (PAR-0121; Java `NetServer.VoteSession`,
//! `NetServer.java:1435-1477` and the `/votekick` `/vote` commands).
//!
//! Java keeps `currentlyKicking` in process memory with a `Timer` task. The
//! module persists the session (`match_vote`) plus one ballot row per voter
//! (`vote_ballot`), so every peer sees the same tally and the 30 s expiry is
//! enforced by `tick_maintenance` (`expire_votes`) instead of a timer.
//!
//! A pass kicks the target for [`VOTE_KICK_SECS`] (Java `kickDuration`) through
//! [`crate::admin::history::apply_kick`], which also refuses the rejoin until the
//! cooldown lapses (Java `NetServer` `getKickTime` join check).

use spacetimedb::{Identity, ReducerContext, Table, TimeDuration, Timestamp, table};

use crate::admin::history::{apply_kick, player_info};
use crate::identity::tables::player;
use crate::main::global::{
    VOTE_BASE_REQUIRED, VOTE_COOLDOWN_SECS, VOTE_DURATION_SECS, VOTE_KICK_SECS, VOTE_RETENTION_SECS,
};
use crate::relay::events::{EventKind, emit};
use crate::relay::tables::{MatchStatus, RelayMatch, RelayMember, relay_member};

use super::commands::VoteChoice;

/// One votekick session (Java `NetServer.VoteSession`).
#[table(accessor = match_vote, index(accessor = by_vote_match, btree(columns = [match_id, vote_id])))]
pub struct MatchVote {
    /// Auto-inc session id.
    #[primary_key]
    #[auto_inc]
    pub vote_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Player being voted on.
    pub target: Identity,
    /// Player that started the vote.
    pub started_by: Identity,
    /// Reason line broadcast to the match.
    pub reason: String,
    /// Session start.
    pub started_at: Timestamp,
    /// Expiry (`voteDuration`); the sweep fails the vote past this point.
    pub expires_at: Timestamp,
    /// Whether the session is finished (pass, fail or cancel).
    pub closed: bool,
    /// Cached yes votes.
    pub votes_for: u32,
    /// Cached no votes.
    pub votes_against: u32,
}

/// One ballot in a session; re-voting replaces the row (Java `voted` map).
#[table(accessor = vote_ballot, index(accessor = by_ballot_vote, btree(columns = [vote_id, voter])))]
pub struct VoteBallot {
    /// Auto-inc ballot id.
    #[primary_key]
    #[auto_inc]
    pub ballot_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Session id.
    pub vote_id: u64,
    /// Voter.
    pub voter: Identity,
    /// `1` yes, `-1` no (Java `VoteSession.voted` sign).
    pub sign: i32,
    /// Cast time.
    pub at: Timestamp,
}

/// Per-player votekick cooldown (Java `cooldowns` map in `registerCommands`).
#[table(accessor = vote_cooldown, index(accessor = by_vote_cooldown, btree(columns = [match_id, identity])))]
pub struct VoteCooldown {
    /// Auto-inc row id.
    #[primary_key]
    #[auto_inc]
    pub cooldown_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Player that started the last vote.
    pub identity: Identity,
    /// When that vote started.
    pub last_started_at: Timestamp,
}

/// Votes required for the current player count (Java `votesRequired`).
pub fn votes_required(players: usize) -> u32 {
    VOTE_BASE_REQUIRED + u32::from(players > 4)
}

/// The active (open) session for a match, if any.
pub fn active_vote(ctx: &ReducerContext, match_id: u64) -> Option<MatchVote> {
    ctx.db
        .match_vote()
        .by_vote_match()
        .filter(match_id)
        .find(|vote| !vote.closed)
}

/// Display name for an identity (current username, then history, then a stub).
pub fn player_name(ctx: &ReducerContext, identity: Identity) -> String {
    if let Some(player) = ctx.db.player().identity().find(identity)
        && !player.username.is_empty()
    {
        return player.username;
    }
    if let Some(info) = ctx.db.player_info().identity().find(identity)
        && !info.last_name.is_empty()
    {
        return info.last_name;
    }
    "<unknown>".to_string()
}

/// Member row of `match_id` for `identity`.
pub fn member_of(ctx: &ReducerContext, match_id: u64, identity: Identity) -> Option<RelayMember> {
    ctx.db
        .relay_member()
        .by_match_identity()
        .filter((match_id, identity))
        .next()
}

/// Number of members in a match (the Java `Groups.player.size()` gate).
pub fn member_count(ctx: &ReducerContext, match_id: u64) -> usize {
    ctx.db
        .relay_member()
        .by_match_identity()
        .filter(match_id)
        .count()
}

fn in_window(now_micros: i64, started_micros: i64, window_secs: u64) -> bool {
    now_micros.saturating_sub(started_micros) < (window_secs as i64) * 1_000_000
}

/// Starts a votekick. All validation happens before any write, so an `Err`
/// leaves no partial session behind.
pub fn start_vote(
    ctx: &ReducerContext,
    row: &RelayMatch,
    target: RelayMember,
    reason: &str,
) -> Result<(), String> {
    if row.status == MatchStatus::Ended {
        return Err(format!("match {} has ended", row.match_id));
    }
    let voter = ctx.sender();
    if target.identity == voter {
        return Err("You can't vote to kick yourself.".to_string());
    }
    if crate::relay::methods::is_admin_for(ctx, target.identity) {
        return Err("Did you really expect to be able to kick an admin?".to_string());
    }
    let voter_member = member_of(ctx, row.match_id, voter)
        .ok_or_else(|| "You are not a member of this match.".to_string())?;
    if target.team != voter_member.team {
        return Err("Only players on your team can be kicked.".to_string());
    }
    if active_vote(ctx, row.match_id).is_some() {
        return Err("A vote is already in progress.".to_string());
    }
    let now_micros = ctx.timestamp.to_micros_since_unix_epoch();
    if let Some(cooldown) = ctx
        .db
        .vote_cooldown()
        .by_vote_cooldown()
        .filter((row.match_id, voter))
        .next()
        && in_window(
            now_micros,
            cooldown.last_started_at.to_micros_since_unix_epoch(),
            VOTE_COOLDOWN_SECS,
        )
    {
        return Err(format!(
            "You must wait {} minutes between votekicks.",
            VOTE_COOLDOWN_SECS / 60
        ));
    }

    let required = votes_required(member_count(ctx, row.match_id));
    let expires_at = ctx.timestamp
        + TimeDuration::from_micros((VOTE_DURATION_SECS.saturating_mul(1_000_000)) as i64);
    let vote = ctx.db.match_vote().insert(MatchVote {
        vote_id: 0,
        match_id: row.match_id,
        target: target.identity,
        started_by: voter,
        reason: reason.to_string(),
        started_at: ctx.timestamp,
        expires_at,
        closed: false,
        votes_for: 1,
        votes_against: 0,
    });
    ctx.db.vote_ballot().insert(VoteBallot {
        ballot_id: 0,
        match_id: row.match_id,
        vote_id: vote.vote_id,
        voter,
        sign: 1,
        at: ctx.timestamp,
    });
    match ctx
        .db
        .vote_cooldown()
        .by_vote_cooldown()
        .filter((row.match_id, voter))
        .next()
    {
        Some(existing) => {
            ctx.db.vote_cooldown().cooldown_id().update(VoteCooldown {
                last_started_at: ctx.timestamp,
                ..existing
            });
        }
        None => {
            ctx.db.vote_cooldown().insert(VoteCooldown {
                cooldown_id: 0,
                match_id: row.match_id,
                identity: voter,
                last_started_at: ctx.timestamp,
            });
        }
    }
    let target_name = player_name(ctx, target.identity);
    let voter_name = player_name(ctx, voter);
    if !reason.trim().is_empty() {
        super::broadcast_system(
            ctx,
            row.match_id,
            voter,
            format!("Reason: {}.", reason.trim()),
        );
    }
    super::broadcast_system(
        ctx,
        row.match_id,
        voter,
        format!(
            "{voter_name} has voted on kicking {target_name}. (1/{required})\nType /vote <y/n> to agree."
        ),
    );
    emit(
        ctx,
        row.match_id,
        EventKind::VoteStarted,
        Some(voter),
        Some(target.identity),
        reason.trim(),
    );
    Ok(())
}

/// Closes a session and records the outcome.
fn close_vote(ctx: &ReducerContext, vote: &MatchVote, passed: bool) {
    ctx.db.match_vote().vote_id().update(MatchVote {
        vote_id: vote.vote_id,
        match_id: vote.match_id,
        target: vote.target,
        started_by: vote.started_by,
        reason: vote.reason.clone(),
        started_at: vote.started_at,
        expires_at: vote.expires_at,
        closed: true,
        votes_for: vote.votes_for,
        votes_against: vote.votes_against,
    });
    let kind = if passed {
        EventKind::VotePassed
    } else {
        EventKind::VoteFailed
    };
    emit(
        ctx,
        vote.match_id,
        kind,
        Some(vote.started_by),
        Some(vote.target),
        format!("for={} against={}", vote.votes_for, vote.votes_against),
    );
}

fn fail_vote(ctx: &ReducerContext, vote: &MatchVote) {
    let target_name = player_name(ctx, vote.target);
    super::broadcast_system(
        ctx,
        vote.match_id,
        vote.started_by,
        format!("Vote failed. Not enough votes to kick {target_name}."),
    );
    close_vote(ctx, vote, false);
}

/// Casts (or changes) the caller's vote on the active session.
pub fn cast_vote(ctx: &ReducerContext, match_id: u64, choice: VoteChoice) -> Result<(), String> {
    let Some(vote) = active_vote(ctx, match_id) else {
        return Err("Nobody is being voted on.".to_string());
    };
    let now_micros = ctx.timestamp.to_micros_since_unix_epoch();
    if vote.expires_at.to_micros_since_unix_epoch() <= now_micros {
        fail_vote(ctx, &vote);
        return Err("That vote has already failed.".to_string());
    }
    let voter = ctx.sender();
    if choice == VoteChoice::Cancel {
        if !crate::relay::methods::is_admin(ctx) {
            return Err("Only admins can cancel a vote.".to_string());
        }
        let name = player_name(ctx, voter);
        super::broadcast_system(
            ctx,
            match_id,
            voter,
            format!("Vote canceled by admin {name}."),
        );
        close_vote(ctx, &vote, false);
        return Ok(());
    }
    if vote.target == voter {
        return Err("You can't vote on your own trial.".to_string());
    }
    let Some(voter_member) = member_of(ctx, match_id, voter) else {
        return Err("You are not a member of this match.".to_string());
    };
    let Some(target_member) = member_of(ctx, match_id, vote.target) else {
        fail_vote(ctx, &vote);
        return Err("The voted player has left the match.".to_string());
    };
    if target_member.team != voter_member.team {
        return Err("You can't vote for other teams.".to_string());
    }
    let sign = match choice {
        VoteChoice::Yes => 1,
        VoteChoice::No => -1,
        VoteChoice::Cancel => unreachable!("cancel handled above"),
    };
    let existing = ctx
        .db
        .vote_ballot()
        .by_ballot_vote()
        .filter((vote.vote_id, voter))
        .next();
    if let Some(ballot) = &existing
        && ballot.sign == sign
    {
        return Err(format!(
            "You've already voted {}. Sit down.",
            if sign > 0 { "yes" } else { "no" }
        ));
    }
    match existing {
        Some(ballot) => {
            ctx.db.vote_ballot().ballot_id().update(VoteBallot {
                sign,
                at: ctx.timestamp,
                ..ballot
            });
        }
        None => {
            ctx.db.vote_ballot().insert(VoteBallot {
                ballot_id: 0,
                match_id,
                vote_id: vote.vote_id,
                voter,
                sign,
                at: ctx.timestamp,
            });
        }
    }
    // Recount from the ballots so a changed vote cannot drift the cached tally.
    let ballots: Vec<VoteBallot> = ctx
        .db
        .vote_ballot()
        .by_ballot_vote()
        .filter(vote.vote_id)
        .collect();
    let votes_for = ballots.iter().filter(|ballot| ballot.sign > 0).count() as u32;
    let votes_against = ballots.iter().filter(|ballot| ballot.sign < 0).count() as u32;
    let required = votes_required(member_count(ctx, match_id));
    let updated = MatchVote {
        vote_id: vote.vote_id,
        match_id: vote.match_id,
        target: vote.target,
        started_by: vote.started_by,
        reason: vote.reason.clone(),
        started_at: vote.started_at,
        expires_at: vote.expires_at,
        closed: false,
        votes_for,
        votes_against,
    };
    let target_name = player_name(ctx, vote.target);
    let voter_name = player_name(ctx, voter);
    super::broadcast_system(
        ctx,
        match_id,
        voter,
        format!(
            "{voter_name} has voted on kicking {target_name}. ({votes_for}/{required})\nType /vote <y/n> to agree."
        ),
    );
    if votes_for >= required {
        super::broadcast_system(
            ctx,
            match_id,
            vote.started_by,
            format!(
                "Vote passed. {target_name} will be banned from the server for {} minutes.",
                VOTE_KICK_SECS / 60
            ),
        );
        let _ = apply_kick(
            ctx,
            match_id,
            vote.target,
            vote.started_by,
            "Vote kicked.",
            VOTE_KICK_SECS,
        );
        close_vote(ctx, &updated, true);
    } else {
        ctx.db.match_vote().vote_id().update(updated);
    }
    Ok(())
}

/// Expires every open session past its deadline (called by the 60 s sweep).
pub fn expire_votes(ctx: &ReducerContext) {
    let now_micros = ctx.timestamp.to_micros_since_unix_epoch();
    let expired: Vec<MatchVote> = ctx
        .db
        .match_vote()
        .iter()
        .filter(|vote| !vote.closed && vote.expires_at.to_micros_since_unix_epoch() <= now_micros)
        .collect();
    for vote in expired {
        fail_vote(ctx, &vote);
    }
}

/// Drops finished sessions, their ballots and stale cooldowns past retention.
pub fn sweep_votes(ctx: &ReducerContext, now_micros: i64) {
    let retention = (VOTE_RETENTION_SECS as i64) * 1_000_000;
    let stale: Vec<MatchVote> = ctx
        .db
        .match_vote()
        .iter()
        .filter(|vote| {
            vote.closed
                && now_micros.saturating_sub(vote.started_at.to_micros_since_unix_epoch())
                    > retention
        })
        .collect();
    for vote in stale {
        for ballot in ctx
            .db
            .vote_ballot()
            .by_ballot_vote()
            .filter(vote.vote_id)
            .collect::<Vec<_>>()
        {
            ctx.db.vote_ballot().ballot_id().delete(ballot.ballot_id);
        }
        ctx.db.match_vote().vote_id().delete(vote.vote_id);
    }
    let cooldown_micros = (VOTE_COOLDOWN_SECS as i64) * 1_000_000;
    for cooldown in ctx.db.vote_cooldown().iter().collect::<Vec<_>>() {
        if now_micros.saturating_sub(cooldown.last_started_at.to_micros_since_unix_epoch())
            > cooldown_micros
        {
            ctx.db
                .vote_cooldown()
                .cooldown_id()
                .delete(cooldown.cooldown_id);
        }
    }
}

/// Deletes every session/ballot/cooldown of an ended match (sweep teardown).
pub fn delete_match_votes(ctx: &ReducerContext, match_id: u64) {
    for vote in ctx
        .db
        .match_vote()
        .by_vote_match()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        for ballot in ctx
            .db
            .vote_ballot()
            .by_ballot_vote()
            .filter(vote.vote_id)
            .collect::<Vec<_>>()
        {
            ctx.db.vote_ballot().ballot_id().delete(ballot.ballot_id);
        }
        ctx.db.match_vote().vote_id().delete(vote.vote_id);
    }
    for cooldown in ctx
        .db
        .vote_cooldown()
        .by_vote_cooldown()
        .filter(match_id)
        .collect::<Vec<_>>()
    {
        ctx.db
            .vote_cooldown()
            .cooldown_id()
            .delete(cooldown.cooldown_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_votes_match_java() {
        assert_eq!(votes_required(0), 2);
        assert_eq!(votes_required(3), 2);
        assert_eq!(votes_required(4), 2);
        assert_eq!(votes_required(5), 3);
        assert_eq!(votes_required(8), 3);
    }

    #[test]
    fn cooldown_window() {
        assert!(in_window(1_000_000, 0, VOTE_COOLDOWN_SECS));
        assert!(!in_window(
            (VOTE_COOLDOWN_SECS as i64) * 1_000_000,
            0,
            VOTE_COOLDOWN_SECS
        ));
        assert!(!in_window(
            (VOTE_COOLDOWN_SECS as i64) * 2_000_000,
            0,
            VOTE_COOLDOWN_SECS
        ));
    }
}

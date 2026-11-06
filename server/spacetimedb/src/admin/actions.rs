// SPDX-License-Identifier: GPL-3.0-only

//! Action filters and the interaction-rate gate (GAPS #34; Java
//! `Administration.java:96-119` `addActionFilter`/`allowAction`/`ActionType`).
//!
//! Java's filters are code closures installed in `Administration`'s
//! constructor. The module keeps the same *shape* as persisted rows: an
//! `action_filter` row may deny (or explicitly allow) one [`ActionType`], either
//! database-global or scoped to one match. [`super::send_match_command`] asks
//! [`enforce_action_filters`] before a command enters the relay log.
//!
//! The built-in interaction rate gate mirrors the second upstream action
//! filter: `interactRateWindow`/`interactRateLimit`/`interactRateKick` from
//! `server_config`, applied to every action except break/place/commandUnits
//! (`Administration.java:101-103`) and admins (`!action.player.isLocal()` plus
//! the admin short-circuit for chat spam).

use spacetimedb::{
    Identity, ReducerContext, SpacetimeType, Table, Timestamp, ViewContext, reducer, table, view,
};

use std::ops::Bound;

use super::{AdminActionKind, admin_gate, admin_rate_check, record};
use crate::admin::ServerConfig;
use crate::main::global::INTERACT_KICK_SECS;
use crate::relay::methods::{is_admin, require_match};
use crate::relay::tables::{CommandKind, RelayMatch};

/// Defines a (potentially dangerous) player action (Java `ActionType`,
/// `Administration.java:747-751`; variant names are ABI, append-only).
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionType {
    /// Break a block.
    BreakBlock,
    /// Place a block.
    PlaceBlock,
    /// Rotate a building.
    Rotate,
    /// Configure a building.
    Configure,
    /// Withdraw an item.
    WithdrawItem,
    /// Deposit an item.
    DepositItem,
    /// Possess a unit or a building.
    Control,
    /// Select a build to control.
    BuildSelect,
    /// Issue a unit movement/attack command.
    Command,
    /// Remove queued plans.
    RemovePlanned,
    /// Issue a unit command.
    CommandUnits,
    /// Command a building.
    CommandBuilding,
    /// Respawn the player unit.
    Respawn,
    /// Pick up a payload.
    PickupBlock,
    /// Drop a payload.
    DropPayload,
    /// Ping a location.
    PingLocation,
}

impl ActionType {
    /// Whether the built-in interaction rate gate covers this action.
    ///
    /// Java excludes break/place/`commandUnits` (`Administration.java:101-103`):
    /// those are legitimately high-frequency from the build UI.
    pub fn rate_limited(self) -> bool {
        !matches!(
            self,
            Self::BreakBlock | Self::PlaceBlock | Self::CommandUnits
        )
    }
}

/// One persisted action-filter rule (table columns flattened for pure tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionRule {
    /// `None` = database-global; `Some` = one match only.
    pub match_id: Option<u64>,
    /// `None` = every action.
    pub action: Option<ActionType>,
    /// `true` allows, `false` denies.
    pub allow: bool,
}

impl ActionRule {
    fn applies_to(self, match_id: u64, action: ActionType) -> bool {
        let action_matches = self.action.map(|rule| rule == action).unwrap_or(true);
        let scope_matches = self.match_id.map(|rule| rule == match_id).unwrap_or(true);
        action_matches && scope_matches
    }
}

/// Verdict for one action, or `None` when no rule applies (allowed).
///
/// Match-scoped rules win over database-global rules (a per-room policy must be
/// able to override the server-wide policy); within the winning scope a deny
/// wins over an allow. Mirrors Java's "every filter must allow" for the
/// persisted deny cases.
pub fn action_verdict(rules: &[ActionRule], match_id: u64, action: ActionType) -> Option<bool> {
    let scoped: Vec<&ActionRule> = rules
        .iter()
        .filter(|rule| rule.match_id.is_some() && rule.applies_to(match_id, action))
        .collect();
    let matching: Vec<&ActionRule> = if scoped.is_empty() {
        rules
            .iter()
            .filter(|rule| rule.match_id.is_none() && rule.applies_to(match_id, action))
            .collect()
    } else {
        scoped
    };
    if matching.is_empty() {
        return None;
    }
    Some(matching.iter().all(|rule| rule.allow))
}

/// Result of the built-in interaction-rate gate (Java `Ratekeeper.allow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractDecision {
    /// The action is allowed; `rolled` reports a fresh window.
    Allow {
        /// Whether the window rolled before this action.
        rolled: bool,
        /// Occurrences after this action.
        occurrences: u32,
    },
    /// Over the limit but below the kick threshold: deny + warn.
    Warn {
        /// Occurrences after this action.
        occurrences: u32,
    },
    /// Over the kick threshold: deny + kick (Java `interactRateKick`).
    Kick {
        /// Occurrences after this action.
        occurrences: u32,
    },
}

/// Pure interaction-rate decision mirroring `Ratekeeper.allow`.
///
/// Java increments `occurrences` on *every* call, then allows while
/// `occurrences <= limit`; the kick fires when `occurrences > kick_at`.
pub fn interact_decision(
    window_start_micros: i64,
    now_micros: i64,
    occurrences: u32,
    window_secs: u32,
    limit: u32,
    kick_at: u32,
) -> InteractDecision {
    let window_micros = i64::from(window_secs) * 1_000_000;
    let rolled = now_micros.saturating_sub(window_start_micros) > window_micros;
    let occurrences = if rolled {
        1
    } else {
        occurrences.saturating_add(1)
    };
    if occurrences > kick_at {
        InteractDecision::Kick { occurrences }
    } else if occurrences > limit {
        InteractDecision::Warn { occurrences }
    } else {
        InteractDecision::Allow {
            rolled,
            occurrences,
        }
    }
}

/// Maps a relay command variant onto the upstream [`ActionType`] (or `None`
/// for variants with no java action: rule edits, admin emitters, UI results).
pub fn action_type_for(kind: &CommandKind) -> Option<ActionType> {
    match kind {
        CommandKind::BreakBlock(_) => Some(ActionType::BreakBlock),
        CommandKind::PlaceBlock(_) => Some(ActionType::PlaceBlock),
        CommandKind::Rotate(_) => Some(ActionType::Rotate),
        CommandKind::ConfigBlock(_) => Some(ActionType::Configure),
        CommandKind::DeletePlans(_) => Some(ActionType::RemovePlanned),
        CommandKind::CommandBuilding(_) => Some(ActionType::CommandBuilding),
        CommandKind::Inventory(payload) => match payload.kind {
            0 => Some(ActionType::WithdrawItem),
            1 => Some(ActionType::DepositItem),
            // Java has no "drop item" ActionType; item drops ride with deposits.
            _ => Some(ActionType::DepositItem),
        },
        CommandKind::Payload(payload) => match payload.kind {
            2 => Some(ActionType::DropPayload),
            _ => Some(ActionType::PickupBlock),
        },
        CommandKind::UnitControl(_) | CommandKind::UnitClear => Some(ActionType::Control),
        CommandKind::BuildingControlSelect(_) => Some(ActionType::BuildSelect),
        CommandKind::UnitCommand(_) | CommandKind::UnitCommandQueue(_) => {
            Some(ActionType::CommandUnits)
        }
        CommandKind::UnitStance(_) => Some(ActionType::CommandUnits),
        CommandKind::PlayerSpawn(_) => Some(ActionType::Respawn),
        CommandKind::MenuChoose(_)
        | CommandKind::MenuBuilderChoose(_)
        | CommandKind::TextInputResult(_)
        | CommandKind::Custom(_)
        | CommandKind::Noop
        | CommandKind::Ping(_)
        | CommandKind::Bullet(_)
        | CommandKind::SetRules(_)
        | CommandKind::SetRule(_)
        | CommandKind::ResearchUnlock(_)
        | CommandKind::CompleteObjective(_)
        | CommandKind::ClearObjectives
        | CommandKind::SectorCapture
        | CommandKind::SaveSector
        | CommandKind::SkipWave
        | CommandKind::RunWave(_)
        | CommandKind::AdminSwitchTeam(_)
        | CommandKind::AdminTileOp(_)
        | CommandKind::LogicSync(_)
        | CommandKind::LogicClientData(_) => None,
    }
}

/// Persisted action policy row (plan §3.10 extension, GAPS #34).
#[table(accessor = action_filter, index(accessor = by_action_filter_added, btree(columns = [added_at])))]
pub struct ActionFilter {
    /// Auto-inc rule id.
    #[primary_key]
    #[auto_inc]
    pub filter_id: u64,
    /// `None` = database-global; `Some` = one match only.
    pub match_id: Option<u64>,
    /// `None` = every action.
    pub action: Option<ActionType>,
    /// `true` allows, `false` denies.
    pub allow: bool,
    /// Admin who added it.
    pub added_by: Identity,
    /// When it was added.
    pub added_at: Timestamp,
}

/// Server-only interaction-rate window, one row per (match, identity).
#[table(accessor = action_rate, index(accessor = by_action_rate, btree(columns = [match_id, identity])))]
pub struct ActionRate {
    /// Auto-inc row id (the table needs a single-column primary key).
    #[primary_key]
    #[auto_inc]
    pub rate_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Caller.
    pub identity: Identity,
    /// Current window start.
    pub window_start: Timestamp,
    /// Actions in the window (`Ratekeeper.occurrences`).
    pub occurrences: u32,
    /// Last touch, for idle pruning.
    pub updated_at: Timestamp,
}

/// Outcome of [`enforce_action_filters`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionGate {
    /// The command may enter the relay log.
    Allow,
    /// The command is dropped (Java action-filter denial). The interaction-rate
    /// window, the private warning and any kick were already written, so the
    /// caller returns `Ok` from the reducer without appending: a transaction
    /// error would roll those writes back and the kick threshold would be
    /// unreachable (Java keeps `Ratekeeper.occurrences` across denials).
    Drop,
}

/// Loads every rule as a pure [`ActionRule`] list (caller filters by match).
pub fn action_rules(ctx: &ReducerContext) -> Vec<ActionRule> {
    ctx.db
        .action_filter()
        .by_action_filter_added()
        .filter((Bound::<Timestamp>::Unbounded, Bound::<Timestamp>::Unbounded))
        .map(|row| ActionRule {
            match_id: row.match_id,
            action: row.action,
            allow: row.allow,
        })
        .collect()
}

/// Evaluates persisted filters + the interaction-rate gate for one command.
///
/// Runs before the ordered append so a denied action never reaches peers. A
/// persisted-filter denial returns `Err` (nothing to persist); a rate-gate
/// denial returns [`ActionGate::Drop`] because its warning/kick bookkeeping
/// must survive the reducer.
pub fn enforce_action_filters(
    ctx: &ReducerContext,
    row: &RelayMatch,
    kind: &CommandKind,
    config: &ServerConfig,
) -> Result<ActionGate, String> {
    let Some(action) = action_type_for(kind) else {
        return Ok(ActionGate::Allow);
    };
    if action_verdict(&action_rules(ctx), row.match_id, action) == Some(false) {
        return Err(format!(
            "action `{action:?}` is disabled by a server action filter"
        ));
    }
    // Java's action filter excludes only the local/server player, not admins
    // (`Administration.java:101-103`); admins are still rate-limited here.
    if !config.anti_spam || !action.rate_limited() {
        return Ok(ActionGate::Allow);
    }
    let identity = ctx.sender();
    let now = ctx.timestamp;
    let now_micros = now.to_micros_since_unix_epoch();
    let existing = ctx
        .db
        .action_rate()
        .by_action_rate()
        .filter((row.match_id, identity))
        .next();
    let (window_start, occurrences) = existing
        .as_ref()
        .map(|rate| (rate.window_start, rate.occurrences))
        .unwrap_or((now, 0));
    let decision = interact_decision(
        window_start.to_micros_since_unix_epoch(),
        now_micros,
        occurrences,
        config.interact_rate_window_secs,
        config.interact_rate_limit,
        config.interact_rate_kick,
    );
    let (window_start, occurrences) = match decision {
        InteractDecision::Allow {
            rolled,
            occurrences,
        } => (if rolled { now } else { window_start }, occurrences),
        InteractDecision::Warn { occurrences } | InteractDecision::Kick { occurrences } => {
            (window_start, occurrences)
        }
    };
    let next = ActionRate {
        rate_id: existing.as_ref().map(|rate| rate.rate_id).unwrap_or(0),
        match_id: row.match_id,
        identity,
        window_start,
        occurrences,
        updated_at: now,
    };
    match existing {
        Some(_) => {
            ctx.db.action_rate().rate_id().update(next);
        }
        None => {
            ctx.db.action_rate().insert(next);
        }
    }
    match decision {
        InteractDecision::Allow { .. } => Ok(ActionGate::Allow),
        InteractDecision::Warn { .. } => {
            crate::chat::private_notice(
                ctx,
                row.match_id,
                identity,
                "You are interacting with blocks too quickly.",
            );
            log::warn!(
                "interaction rate limit: dropped {action:?} for match {} (occurrences {occurrences})",
                row.match_id
            );
            Ok(ActionGate::Drop)
        }
        InteractDecision::Kick { .. } => {
            crate::chat::private_notice(
                ctx,
                row.match_id,
                identity,
                "You have been kicked for interacting too quickly.",
            );
            let _ = crate::admin::history::apply_kick(
                ctx,
                row.match_id,
                identity,
                row.host,
                "You have been kicked for interacting too quickly.",
                INTERACT_KICK_SECS,
            );
            log::warn!(
                "interaction rate kick: dropped {action:?} for match {} (occurrences {occurrences})",
                row.match_id
            );
            Ok(ActionGate::Drop)
        }
    }
}

/// Adds one persisted action filter (admin only).
#[reducer]
pub fn admin_add_action_filter(
    ctx: &ReducerContext,
    match_id: Option<u64>,
    action: Option<ActionType>,
    allow: bool,
) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::AddActionFilter)?;
    if let Some(match_id) = match_id {
        require_match(ctx, match_id)?;
    }
    admin_rate_check(ctx)?;
    ctx.db.action_filter().insert(ActionFilter {
        filter_id: 0,
        match_id,
        action,
        allow,
        added_by: ctx.sender(),
        added_at: ctx.timestamp,
    });
    record(
        ctx,
        AdminActionKind::AddActionFilter,
        None,
        match_id,
        format!("action={action:?} allow={allow}"),
    );
    Ok(())
}

/// Removes one persisted action filter by id (admin only).
#[reducer]
pub fn admin_remove_action_filter(ctx: &ReducerContext, filter_id: u64) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::RemoveActionFilter)?;
    let row = ctx
        .db
        .action_filter()
        .filter_id()
        .find(filter_id)
        .ok_or_else(|| format!("unknown action filter {filter_id}"))?;
    admin_rate_check(ctx)?;
    ctx.db.action_filter().filter_id().delete(filter_id);
    record(
        ctx,
        AdminActionKind::RemoveActionFilter,
        None,
        row.match_id,
        format!("filter {filter_id}"),
    );
    Ok(())
}

/// Every action filter, visible to admins only (plan §6.2).
#[view(accessor = all_action_filters, public)]
fn all_action_filters(ctx: &ViewContext) -> Vec<ActionFilter> {
    if !crate::admin::view_is_admin(ctx) {
        return Vec::new();
    }
    ctx.db
        .action_filter()
        .by_action_filter_added()
        .filter((Bound::<Timestamp>::Unbounded, Bound::<Timestamp>::Unbounded))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(match_id: Option<u64>, action: Option<ActionType>, allow: bool) -> ActionRule {
        ActionRule {
            match_id,
            action,
            allow,
        }
    }

    #[test]
    fn global_filter_denies() {
        let rules = [rule(None, Some(ActionType::PlaceBlock), false)];
        assert_eq!(
            action_verdict(&rules, 7, ActionType::PlaceBlock),
            Some(false)
        );
        // Other actions are unaffected.
        assert_eq!(action_verdict(&rules, 7, ActionType::Configure), None);
    }

    #[test]
    fn match_filter_overrides_global() {
        let rules = [
            rule(None, Some(ActionType::PlaceBlock), false),
            rule(Some(7), Some(ActionType::PlaceBlock), true),
        ];
        assert_eq!(
            action_verdict(&rules, 7, ActionType::PlaceBlock),
            Some(true)
        );
        assert_eq!(
            action_verdict(&rules, 8, ActionType::PlaceBlock),
            Some(false)
        );
    }

    #[test]
    fn deny_wins_within_scope() {
        let rules = [
            rule(Some(7), None, true),
            rule(Some(7), Some(ActionType::BreakBlock), false),
        ];
        assert_eq!(
            action_verdict(&rules, 7, ActionType::BreakBlock),
            Some(false)
        );
        assert_eq!(action_verdict(&rules, 7, ActionType::Rotate), Some(true));
    }

    #[test]
    fn interact_rate_warn_and_kick() {
        // Fresh window, under the cap.
        assert_eq!(
            interact_decision(0, 0, 0, 6, 25, 60),
            InteractDecision::Allow {
                rolled: false,
                occurrences: 1
            }
        );
        // 25th action is still allowed.
        assert_eq!(
            interact_decision(0, 1, 24, 6, 25, 60),
            InteractDecision::Allow {
                rolled: false,
                occurrences: 25
            }
        );
        // 26th warns (over limit, at the kick threshold).
        assert_eq!(
            interact_decision(0, 2, 25, 6, 25, 60),
            InteractDecision::Warn { occurrences: 26 }
        );
        // Beyond the kick threshold kicks.
        assert_eq!(
            interact_decision(0, 3, 60, 6, 25, 60),
            InteractDecision::Kick { occurrences: 61 }
        );
        // Window roll resets the counter.
        assert_eq!(
            interact_decision(0, 7_000_000, 60, 6, 25, 60),
            InteractDecision::Allow {
                rolled: true,
                occurrences: 1
            }
        );
    }

    #[test]
    fn action_mapping_covers_java_types() {
        use crate::relay::tables::{
            BreakBlock, ConfigBlock, Inventory, Payload, PlaceBlock, Rotate,
        };
        assert_eq!(
            action_type_for(&CommandKind::PlaceBlock(PlaceBlock {
                x: 0,
                y: 0,
                block: "conveyor".to_string(),
                rotation: 0,
                config: Vec::new(),
            })),
            Some(ActionType::PlaceBlock)
        );
        assert_eq!(
            action_type_for(&CommandKind::BreakBlock(BreakBlock { x: 0, y: 0 })),
            Some(ActionType::BreakBlock)
        );
        assert_eq!(
            action_type_for(&CommandKind::Rotate(Rotate {
                x: 0,
                y: 0,
                direction: false
            })),
            Some(ActionType::Rotate)
        );
        assert_eq!(
            action_type_for(&CommandKind::ConfigBlock(ConfigBlock {
                x: 0,
                y: 0,
                value: Vec::new()
            })),
            Some(ActionType::Configure)
        );
        assert_eq!(
            action_type_for(&CommandKind::Inventory(Inventory {
                kind: 0,
                x: 0,
                y: 0,
                item: None,
                amount: 1,
                angle: 0.0
            })),
            Some(ActionType::WithdrawItem)
        );
        assert_eq!(
            action_type_for(&CommandKind::Payload(Payload {
                kind: 2,
                x: 0.0,
                y: 0.0,
                target: None
            })),
            Some(ActionType::DropPayload)
        );
        assert_eq!(action_type_for(&CommandKind::Noop), None);
        assert_eq!(action_type_for(&CommandKind::ClearObjectives), None);
    }

    #[test]
    fn rate_limited_matches_java_exclusions() {
        assert!(!ActionType::BreakBlock.rate_limited());
        assert!(!ActionType::PlaceBlock.rate_limited());
        assert!(!ActionType::CommandUnits.rate_limited());
        assert!(ActionType::Configure.rate_limited());
        assert!(ActionType::Rotate.rate_limited());
    }
}

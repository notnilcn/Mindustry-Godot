// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RulesLoadEvent` application, `rules_epoch` tracking and the `SetRules`
//! command guard (plan 12 M0/M8, plan 05 `SetRules` seam).
//!
//! Upstream fires `EventType.RulesLoadEvent` from `Logic.play` / the save
//! reader; plan 12 closes plan 05's deferred "JSON-blob until typed `Rules`"
//! seam by decoding the `SimCommand::SetRules { json }` blob into the typed
//! [`Rules`]. The relay shape (`rules_blob` + `rules_epoch`) is plan 21's; this
//! module owns the host-side guard and the epoch bump.

use serde_json::Error as JsonError;

use super::rules::Rules;

/// Maximum accepted rules JSON length (`io.typeio.MAX_RULES_BYTES`, plan 04).
pub const MAX_RULES_BYTES: usize = 100_000;

/// Initial rules epoch; bumped on every successful rules replacement.
pub const RULES_EPOCH_INIT: u32 = 0;

/// Reason a `SetRules` command was refused. Cheap validation only (D2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetRulesError {
    /// The blob exceeded [`MAX_RULES_BYTES`].
    #[error("rules blob too large: {len} > {max} bytes")]
    TooLarge {
        /// Actual length.
        len: usize,
        /// Allowed maximum.
        max: usize,
    },
    /// The JSON did not parse into [`Rules`].
    #[error("invalid rules JSON: {0}")]
    Json(String),
    /// Campaign matches forbid in-game rule edits unless `allowEditRules`.
    #[error("campaign rules are read-only (allowEditRules = false)")]
    CampaignReadOnly,
    /// The client's epoch does not match the host's (stale relay command).
    #[error("rules epoch mismatch: expected {expected}, got {got}")]
    EpochMismatch {
        /// Host epoch.
        expected: u32,
        /// Command epoch.
        got: u32,
    },
}

impl From<JsonError> for SetRulesError {
    fn from(error: JsonError) -> Self {
        SetRulesError::Json(error.to_string())
    }
}

/// Monotonic rules revision (`rules_epoch`), used by plan 21/23 state compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RulesEpoch(pub u32);

impl RulesEpoch {
    /// Fresh epoch.
    pub const fn new() -> Self {
        Self(RULES_EPOCH_INIT)
    }

    /// Advances the epoch and returns the new value.
    pub fn bump(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }
}

/// The decoded intent of a `RulesLoadEvent`, mirroring the upstream event plus
/// the plan-12 epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RulesLoad {
    /// Epoch after the load.
    pub rules_epoch: u32,
    /// Whether the rules came from a save (`fromSave`).
    pub from_save: bool,
}

/// Decodes a rules JSON blob with the 100 000-byte cap.
pub fn decode_rules(json: &str) -> Result<Rules, SetRulesError> {
    if json.len() > MAX_RULES_BYTES {
        return Err(SetRulesError::TooLarge {
            len: json.len(),
            max: MAX_RULES_BYTES,
        });
    }
    Ok(serde_json::from_str(json)?)
}

/// Host guard for a `SetRules` command: campaign matches reject edits unless
/// `allowEditRules` is set. Non-campaign matches always allow.
pub fn guard_set_rules(rules: &Rules, is_campaign: bool) -> Result<(), SetRulesError> {
    if is_campaign && !rules.allow_edit_rules {
        Err(SetRulesError::CampaignReadOnly)
    } else {
        Ok(())
    }
}

/// Applies a decoded `SetRules` blob to `current`, enforcing the campaign guard
/// and the optional epoch match. Returns the new epoch.
pub fn apply_set_rules(
    current: &mut Rules,
    incoming: Rules,
    is_campaign: bool,
    command_epoch: Option<u32>,
    epoch: &mut RulesEpoch,
) -> Result<u32, SetRulesError> {
    guard_set_rules(current, is_campaign)?;
    if let Some(got) = command_epoch
        && got != epoch.0
    {
        return Err(SetRulesError::EpochMismatch {
            expected: epoch.0,
            got,
        });
    }
    *current = incoming;
    Ok(epoch.bump())
}

/// Replaces the active rules and reports the load (`RulesLoadEvent` payload).
///
/// `retain_content_fields` must already have run in the play flow (plan 12
/// invariant 6); this function only performs the wholesale replacement + epoch.
pub fn apply_rules_load(
    current: &mut Rules,
    incoming: Rules,
    from_save: bool,
    epoch: &mut RulesEpoch,
) -> RulesLoad {
    *current = incoming;
    RulesLoad {
        rules_epoch: epoch.bump(),
        from_save,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;

    #[test]
    fn decode_enforces_size_and_shape() {
        let json = serde_json::to_string(&Rules::default()).unwrap();
        let rules = decode_rules(&json).unwrap();
        assert_eq!(rules.default_team, 1);

        let huge = "x".repeat(MAX_RULES_BYTES + 1);
        assert!(matches!(
            decode_rules(&huge),
            Err(SetRulesError::TooLarge { .. })
        ));

        assert!(matches!(
            decode_rules("{ not json"),
            Err(SetRulesError::Json(_))
        ));
    }

    #[test]
    fn campaign_guard_rejects_without_allow_edit() {
        let mut rules = Rules::default();
        assert!(guard_set_rules(&rules, true).is_err());
        rules.allow_edit_rules = true;
        assert!(guard_set_rules(&rules, true).is_ok());
        // Non-campaign is always editable.
        assert!(guard_set_rules(&Rules::default(), false).is_ok());
    }

    #[test]
    fn apply_set_rules_bumps_epoch_and_checks_epoch() {
        let mut current = Rules::default();
        let mut epoch = RulesEpoch::new();
        let mut incoming = Rules::default();
        incoming.win_wave = 10;

        let new_epoch =
            apply_set_rules(&mut current, incoming, false, Some(0), &mut epoch).unwrap();
        assert_eq!(new_epoch, 1);
        assert_eq!(current.win_wave, 10);

        // Stale epoch is rejected and does not mutate.
        let mut stale = Rules::default();
        let result = apply_set_rules(&mut current, stale.clone(), false, Some(0), &mut epoch);
        assert!(matches!(result, Err(SetRulesError::EpochMismatch { .. })));
        assert_eq!(current.win_wave, 10);

        // Campaign read-only blocks before epoch check.
        stale.allow_edit_rules = false;
        assert!(matches!(
            apply_set_rules(&mut stale, Rules::default(), true, None, &mut epoch),
            Err(SetRulesError::CampaignReadOnly)
        ));
    }

    #[test]
    fn apply_rules_load_reports_epoch() {
        let mut current = Rules::default();
        let mut epoch = RulesEpoch::new();
        let load = apply_rules_load(&mut current, Rules::default(), true, &mut epoch);
        assert_eq!(load.rules_epoch, 1);
        assert!(load.from_save);
    }
}

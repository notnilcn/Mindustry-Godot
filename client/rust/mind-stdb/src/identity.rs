// SPDX-License-Identifier: GPL-3.0-only

//! Local player identity and the `--pN` command-line suffix parser, ported from
//! the deleted C# `DatabaseConnector` (`IsPlayerArg`, `IsLocal`).

use spacetimedb_sdk::Identity;

/// The identity this process connected as, plus its launch-arg suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalIdentity {
    /// Server-assigned identity.
    pub identity: Identity,
    /// On-disk suffix form (`_p1`) parsed from `--p1`, if any.
    pub suffix: Option<String>,
}

impl LocalIdentity {
    /// Creates a local identity.
    pub fn new(identity: Identity, suffix: Option<String>) -> Self {
        Self { identity, suffix }
    }

    /// C# `IsLocal`: whether `other` is this process's identity.
    pub fn is_local(&self, other: &Identity) -> bool {
        self.identity == *other
    }

    /// Lowercase 64-hex identity string (gdext/MCP diagnostics).
    pub fn hex(&self) -> String {
        self.identity.to_hex().to_string()
    }
}

/// Exact `^--p\d+$` test: rejects Godot's `--path` and bare `--p`.
pub fn is_player_arg(arg: &str) -> bool {
    let Some(digits) = arg.strip_prefix("--p") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// First `--pN` in `args`, converted to the token-suffix form (`--p1` → `_p1`).
pub fn parse_player_suffix(args: impl Iterator<Item = String>) -> Option<String> {
    args.into_iter()
        .find(|arg| is_player_arg(arg))
        .map(|arg| arg.replacen("--", "_", 1))
}

/// C# argument-source order: engine args are scanned before user args, so an
/// engine `--p1` wins over a user `--p2`.
pub fn parse_player_suffix_from(engine_args: &[String], user_args: &[String]) -> Option<String> {
    parse_player_suffix(engine_args.iter().chain(user_args.iter()).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn parses_p_only_args() {
        assert_eq!(
            parse_player_suffix(args(&["--p1"]).into_iter()),
            Some("_p1".to_string())
        );
        assert_eq!(
            parse_player_suffix(args(&["--verbose", "--p2"]).into_iter()),
            Some("_p2".to_string())
        );
        assert_eq!(
            parse_player_suffix(args(&["--p12"]).into_iter()),
            Some("_p12".to_string())
        );
        // C# regression guard: Godot's `--path` must never match `^--p\d+$`.
        assert_eq!(parse_player_suffix(args(&["--path"]).into_iter()), None);
        assert_eq!(parse_player_suffix(args(&["--p"]).into_iter()), None);
        assert_eq!(parse_player_suffix(args(&["--p1x"]).into_iter()), None);
        assert_eq!(parse_player_suffix(args(&[]).into_iter()), None);
    }

    #[test]
    fn engine_args_win_over_user_args() {
        let engine = args(&["--p1"]);
        let user = args(&["--p2"]);
        assert_eq!(parse_player_suffix_from(&engine, &user), Some("_p1".into()));
        assert_eq!(
            parse_player_suffix_from(&args(&[]), &user),
            Some("_p2".into())
        );
    }

    #[test]
    fn is_local_compares_identity() {
        let identity = Identity::from_byte_array([7u8; 32]);
        let other = Identity::from_byte_array([9u8; 32]);
        let local = LocalIdentity::new(identity, Some("_p1".into()));
        assert!(local.is_local(&identity));
        assert!(!local.is_local(&other));
        assert!(local.hex().starts_with("0707"));
        assert_eq!(local.hex().len(), 64);
    }
}

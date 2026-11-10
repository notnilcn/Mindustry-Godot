// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/JoinDialog.java (version check).

//! The join version gate: the client build identity, the per-component build
//! compare, `getVersionString` rendering and the `safeConnect` decision.
//! Upstream: ui/dialogs/JoinDialog.java (getVersionString/safeConnect)

use godot::prelude::*;

use crate::ui::kit;

// ---------------------------------------------------------------------------
// Server version gate (`JoinDialog.getVersionString`/`safeConnect`, GAPS #45)
// ---------------------------------------------------------------------------

/// `JoinDialog.getVersionString` classification (`JoinDialog.java:778-791`).
///
/// The relay exposes the numeric gate as `RelayMatch.protocol_version` and the
/// human version as `RelayMatch.build_id`; this client's `Version.build`
/// analogue is [`mind_stdb::PROTOCOL_VERSION`] plus [`join_client_build`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JoinVersion {
    /// Same build and protocol (`host.version == Version.build`, official).
    Same,
    /// Older host (`host.version < Version.build`).
    Older,
    /// Newer host (`host.version > Version.build`).
    Newer,
    /// No build advertised (`host.version == 0`).
    Unknown,
}

/// The rendered pieces of Java's `getVersionString` return value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct JoinVersionLine {
    /// `@server.version` arguments `(build, type)`; `None` when Java returns
    /// `""` (same build) or lists only the outdated badge.
    version: Option<(String, String)>,
    /// `@server.outdated` / `@server.outdated.client` bundle key.
    badge: Option<&'static str>,
}

/// The client build identity sent with every join (`MindNet.join_match` sends
/// `mind_stdb::MIND_VERSION` as the relay `build_id`).
fn join_client_build() -> &'static str {
    mind_stdb::MIND_VERSION
}

/// Numeric per-component version compare (`0.1.10 > 0.1.9`), like
/// `Version.build` ordering; non-numeric suffixes are ignored.
fn join_build_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let parts = |value: &str| -> Vec<i64> {
        value
            .trim()
            .split('.')
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| {
                let digits: String = chunk.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse().unwrap_or(0)
            })
            .collect()
    };
    let left_parts = parts(left);
    let right_parts = parts(right);
    for index in 0..left_parts.len().max(right_parts.len()) {
        let a = left_parts.get(index).copied().unwrap_or(0);
        let b = right_parts.get(index).copied().unwrap_or(0);
        match a.cmp(&b) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

/// Classifies one probe match against this client (`getVersionString`'s
/// branch order: `0`/older/newer/equal).
fn join_version(server_build: &str, server_protocol: i64) -> JoinVersion {
    let build = server_build.trim();
    if build.is_empty() || server_protocol == 0 {
        // `host.version == 0` (or an unversioned relay row): `@server.outdated`.
        return JoinVersion::Unknown;
    }
    let client_protocol = i64::from(mind_stdb::PROTOCOL_VERSION);
    if server_protocol != client_protocol {
        return if server_protocol < client_protocol {
            JoinVersion::Older
        } else {
            JoinVersion::Newer
        };
    }
    match join_build_cmp(build, join_client_build()) {
        std::cmp::Ordering::Equal => JoinVersion::Same,
        std::cmp::Ordering::Less => JoinVersion::Older,
        std::cmp::Ordering::Greater => JoinVersion::Newer,
    }
}

/// `JoinDialog.getVersionString` (`:778-791`) split into its display pieces.
fn join_version_line(server_build: &str, server_protocol: i64) -> JoinVersionLine {
    let build = server_build.trim().to_string();
    match join_version(server_build, server_protocol) {
        // Same build and official type: Java returns "" (nothing shown).
        JoinVersion::Same => JoinVersionLine::default(),
        JoinVersion::Unknown => JoinVersionLine {
            version: None,
            badge: Some("@server.outdated"),
        },
        // `@server.version` + `@server.outdated` (Java prepends a newline; the
        // caller lays the line out).
        JoinVersion::Older => JoinVersionLine {
            version: Some((build, String::new())),
            badge: Some("@server.outdated"),
        },
        JoinVersion::Newer => JoinVersionLine {
            version: Some((build, String::new())),
            badge: Some("@server.outdated.client"),
        },
    }
}

/// The `safeConnect` gate result (`JoinDialog.java:675-681`), `None` when the
/// versions match and the connect may proceed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct JoinSafeConnect {
    /// `server.kicked.clientOutdated`/`server.kicked.serverOutdated`.
    reason_key: &'static str,
    /// This client's build (`Version.build`).
    client_version: &'static str,
    /// The host's advertised build id (`host.version`).
    server_version: String,
}

/// `JoinDialog.safeConnect` version comparison: differing builds show the
/// scarlet kick reason plus `@server.versions` and never connect.
fn join_safe_connect(server_build: &str, server_protocol: i64) -> Option<JoinSafeConnect> {
    let state = join_version(server_build, server_protocol);
    if state == JoinVersion::Same {
        return None;
    }
    let reason_key = if state == JoinVersion::Newer {
        "@server.kicked.clientOutdated"
    } else {
        "@server.kicked.serverOutdated"
    };
    let server = server_build.trim();
    Some(JoinSafeConnect {
        reason_key,
        client_version: join_client_build(),
        server_version: if server.is_empty() {
            String::from("?")
        } else {
            server.to_string()
        },
    })
}

/// `getVersionString` rendered with bundle markup for a server row; empty when
/// the versions match (Java returns `""`).
pub(super) fn join_version_string(server_build: &str, server_protocol: i64) -> String {
    let line = join_version_line(server_build, server_protocol);
    let mut out = String::new();
    if let Some((build, version_type)) = &line.version {
        out.push_str(
            &kit::tm_format("@server.version", &[build.as_str(), version_type.as_str()])
                .to_string(),
        );
    }
    if let Some(key) = line.badge {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&kit::tm(key).to_string());
    }
    out
}

/// Shows the `safeConnect` version modal (`ui.showInfo`) and returns whether
/// the connect must be aborted.
pub(super) fn join_abort_on_version(server_build: &str, server_protocol: i64) -> bool {
    let Some(gate) = join_safe_connect(server_build, server_protocol) else {
        return false;
    };
    let text = format!(
        "[scarlet]{}[]\n{}",
        kit::t(gate.reason_key),
        kit::tm_format(
            "@server.versions",
            &[gate.client_version, gate.server_version.as_str()]
        )
    );
    if let Some(mut ui) = kit::ui()
        && ui.has_method("show_text")
    {
        let _ = ui.call(
            "show_text",
            &[
                GString::new().to_variant(),
                GString::from(text.as_str()).to_variant(),
            ],
        );
    }
    true
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn build_compare_is_numeric_per_component() {
        assert_eq!(
            join_build_cmp("0.1.10", "0.1.9"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(join_build_cmp("0.1", "0.1.0"), std::cmp::Ordering::Equal);
        assert_eq!(join_build_cmp("1.2", "1.10"), std::cmp::Ordering::Less);
        // Non-numeric suffixes are ignored (`0.1.0-beta` == `0.1.0`).
        assert_eq!(
            join_build_cmp("0.1.0-beta", "0.1.0"),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            join_build_cmp("", join_client_build()),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn version_gate_matches_get_version_string_branches() {
        // Same build + protocol: nothing shown and safeConnect proceeds.
        assert_eq!(
            join_version(join_client_build(), i64::from(mind_stdb::PROTOCOL_VERSION)),
            JoinVersion::Same
        );
        assert_eq!(
            join_safe_connect(join_client_build(), i64::from(mind_stdb::PROTOCOL_VERSION)),
            None
        );
        // No build / version 0: `@server.outdated` without a version line.
        assert_eq!(join_version("", 2), JoinVersion::Unknown);
        assert_eq!(join_version_line("", 2).badge, Some("@server.outdated"));
        assert!(join_version_line("", 2).version.is_none());
        assert_eq!(join_version("0.1.0", 0), JoinVersion::Unknown);
        // Older/newer host builds map to the two outdated badges.
        let older = join_version_line("0.0.9", i64::from(mind_stdb::PROTOCOL_VERSION));
        assert_eq!(
            older.version.as_ref().map(|(build, _)| build.as_str()),
            Some("0.0.9")
        );
        assert_eq!(older.badge, Some("@server.outdated"));
        let newer = join_version_line("9.9.9", i64::from(mind_stdb::PROTOCOL_VERSION));
        assert_eq!(newer.badge, Some("@server.outdated.client"));
        // The protocol is the numeric gate even when the build string matches.
        assert_eq!(
            join_version(
                join_client_build(),
                i64::from(mind_stdb::PROTOCOL_VERSION) - 1
            ),
            JoinVersion::Older
        );
        assert_eq!(
            join_version(
                join_client_build(),
                i64::from(mind_stdb::PROTOCOL_VERSION) + 1
            ),
            JoinVersion::Newer
        );
    }

    #[test]
    fn safe_connect_reports_the_kick_reason_and_both_versions() {
        let protocol = i64::from(mind_stdb::PROTOCOL_VERSION);
        let older = join_safe_connect("0.0.9", protocol).expect("older host must gate");
        assert_eq!(older.reason_key, "@server.kicked.serverOutdated");
        assert_eq!(older.client_version, join_client_build());
        assert_eq!(older.server_version, "0.0.9");
        let newer = join_safe_connect("9.9.9", protocol).expect("newer host must gate");
        assert_eq!(newer.reason_key, "@server.kicked.clientOutdated");
        // An unversioned host still fills `{1}` rather than leaving it blank.
        let unknown = join_safe_connect("", protocol).expect("unversioned host must gate");
        assert_eq!(unknown.reason_key, "@server.kicked.serverOutdated");
        assert_eq!(unknown.server_version, "?");
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Test-only `expect` helpers shared by the `campaign` unit tests.
//! Upstream: none (port-only; test helpers)

/// Test-only `expect`: the workspace denies `clippy::expect_used`.
pub(super) fn require<T>(value: Option<T>, what: &str) -> T {
    match value {
        Some(value) => value,
        None => panic!("missing {what}"),
    }
}

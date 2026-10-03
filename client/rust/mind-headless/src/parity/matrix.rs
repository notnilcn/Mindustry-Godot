// SPDX-License-Identifier: GPL-3.0-only

//! `parity/matrix.toml` (format 1): the master upstream-test -> Rust-test registry.
//!
//! Structural validation is always available (`parity matrix check`). When a
//! `cargo test -- --list` dump is supplied (`--tests <file>`), every row marked
//! `status = "landed"` must resolve in that dump.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow};

use super::minitoml::{Document, Value, parse};

/// A single upstream-test mapping row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Upstream test id, e.g. `ApplicationTests.sorterOutputCorrect`.
    pub upstream: String,
    /// Owning plan file, e.g. `08_LOGISTICS_IMPLEMENTATION_PLAN.md`.
    pub owner: String,
    /// Phase key (`P0`..`P8`).
    pub phase: String,
    /// Resolving Rust test path (empty for `no-equivalent`).
    pub rust: String,
    /// Whether this row is the primary owner for `upstream`.
    pub primary: bool,
    /// `landed | planned | planned-ignored | no-equivalent | tbd`.
    pub status: String,
    /// Oracle/substitute description.
    pub oracle: String,
    /// Required when `status = "tbd"`.
    pub tbd_by: Option<String>,
}

/// Reads and parses the matrix registry.
pub fn load(path: &Path) -> Result<Vec<Row>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading `{}`", path.display()))?;
    let document = parse(&text).with_context(|| format!("parsing `{}`", path.display()))?;
    rows_from_document(&document)
}

fn rows_from_document(document: &Document) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    for table in document.tables_named("row") {
        rows.push(Row {
            upstream: required(table, "upstream")?,
            owner: required(table, "owner")?,
            phase: required(table, "phase")?,
            rust: optional(table, "rust"),
            primary: table
                .values
                .get("primary")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            status: required(table, "status")?,
            oracle: optional(table, "oracle"),
            tbd_by: table
                .values
                .get("tbd_by")
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
    }
    Ok(rows)
}

fn required(table: &super::minitoml::Table, key: &str) -> Result<String> {
    table
        .values
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("matrix row is missing required string `{key}`"))
}

fn optional(table: &super::minitoml::Table, key: &str) -> String {
    table
        .values
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

const VALID_STATUSES: &[&str] = &[
    "landed",
    "planned",
    "planned-ignored",
    "no-equivalent",
    "tbd",
];

/// Structural validation against repo/plan files.
pub fn check(rows: &[Row], repo: &Path, test_names: Option<&BTreeSet<String>>) -> Vec<String> {
    let mut problems = Vec::new();

    let mut primaries: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        let label = if row.upstream.is_empty() {
            format!("row #{index}")
        } else {
            row.upstream.clone()
        };

        if row.upstream.is_empty() {
            problems.push(format!("{label}: empty `upstream`"));
        }
        if row.owner.is_empty() {
            problems.push(format!("{label}: empty `owner`"));
        } else if crate::paths::resolve_plan_file(repo, &row.owner).is_none() {
            problems.push(format!(
                "{label}: owner plan `{}` does not exist",
                row.owner
            ));
        }
        if row.phase.is_empty() {
            problems.push(format!("{label}: empty `phase`"));
        }
        if row.oracle.is_empty() {
            problems.push(format!("{label}: empty `oracle`"));
        }
        if !VALID_STATUSES.contains(&row.status.as_str()) {
            problems.push(format!(
                "{label}: unknown status `{}` (expected one of {VALID_STATUSES:?})",
                row.status
            ));
        }

        match row.status.as_str() {
            "tbd" => {
                if row.tbd_by.as_deref().unwrap_or_default().is_empty() {
                    problems.push(format!("{label}: status=tbd requires `tbd_by`"));
                }
            }
            "landed" => {
                if row.rust.is_empty() {
                    problems.push(format!("{label}: status=landed requires a `rust` name"));
                } else if let Some(names) = test_names
                    && !names.contains(row.rust.as_str())
                {
                    problems.push(format!(
                        "{label}: rust test `{}` not found in the supplied test list",
                        row.rust
                    ));
                }
            }
            _ => {}
        }

        if row.primary {
            *primaries.entry(row.upstream.as_str()).or_default() += 1;
        }
    }

    for row in rows {
        if row.upstream.is_empty() {
            continue;
        }
        let count = primaries.get(row.upstream.as_str()).copied().unwrap_or(0);
        if count == 0 {
            problems.push(format!("{}: no `primary = true` row", row.upstream));
        } else if count > 1 {
            problems.push(format!(
                "{}: {count} `primary = true` rows (exactly one required)",
                row.upstream
            ));
        }
    }

    problems.sort();
    problems.dedup();
    problems
}

/// Parses a `cargo test -- --list` output file into a set of test paths.
pub fn parse_test_list(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // `path::to::test: test` (bench lines end in `: benchmark`).
        let Some((name, kind)) = line.rsplit_once(':') else {
            continue;
        };
        let kind = kind.trim();
        if kind != "test" {
            continue;
        }
        names.insert(name.trim().to_owned());
    }
    names
}

/// Rows filtered to those whose phase is at or before `phase` (e.g. `P3`).
pub fn rows_through_phase<'a>(rows: &'a [Row], phase: &str) -> Vec<&'a Row> {
    let target = phase_number(phase);
    rows.iter()
        .filter(|row| phase_number(&row.phase) <= target)
        .collect()
}

/// Parses `P<n>` into `n`; unknown phases sort last.
pub fn phase_number(phase: &str) -> u8 {
    phase
        .strip_prefix('P')
        .and_then(|n| n.parse::<u8>().ok())
        .unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Row> {
        vec![
            Row {
                upstream: "A.test".into(),
                owner: "00_FOUNDATION_IMPLEMENTATION_PLAN.md".into(),
                phase: "P0".into(),
                rust: "foo::tests::bar".into(),
                primary: true,
                status: "landed".into(),
                oracle: "oracle".into(),
                tbd_by: None,
            },
            Row {
                upstream: "B.test".into(),
                owner: "13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md".into(),
                phase: "P5".into(),
                rust: "logic::tests::x".into(),
                primary: true,
                status: "planned".into(),
                oracle: "oracle".into(),
                tbd_by: None,
            },
        ]
    }

    #[test]
    fn parse_test_list_extracts_test_paths_only() {
        let text = "foo::tests::a: test\nfoo::tests::b: benchmark\nsome::doc: test\n";
        let names = parse_test_list(text);
        assert!(names.contains("foo::tests::a"));
        assert!(names.contains("some::doc"));
        assert!(!names.contains("foo::tests::b"));
    }

    #[test]
    fn duplicate_primary_is_flagged() {
        let mut rows = sample();
        rows.push(Row {
            primary: true,
            upstream: "A.test".into(),
            ..rows[0].clone()
        });
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let problems = check(&rows, &repo, None);
        assert!(problems.iter().any(|p| p.contains("2 `primary = true`")));
    }

    #[test]
    fn landed_rows_must_resolve_when_list_supplied() {
        let rows = sample();
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let mut names = BTreeSet::new();
        names.insert("foo::tests::bar".to_owned());
        assert!(check(&rows, &repo, Some(&names)).is_empty());

        names.clear();
        let problems = check(&rows, &repo, Some(&names));
        assert!(problems.iter().any(|p| p.contains("not found")));
    }

    #[test]
    fn phase_filter_is_inclusive() {
        let rows = sample();
        assert_eq!(rows_through_phase(&rows, "P0").len(), 1);
        assert_eq!(rows_through_phase(&rows, "P5").len(), 2);
    }
}

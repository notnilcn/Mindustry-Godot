// SPDX-License-Identifier: GPL-3.0-only

//! `parity/soak.toml` (format 1): soak profiles + the runner skeleton.
//! The long-running execution is intentionally a skeleton here (minutes-scale
//! runs belong on the nightly `perf` runner); `parity soak` validates the
//! profile and emits the execution plan.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};

use super::minitoml::{Document, Value, parse};

/// A parsed soak profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// Profile name (e.g. `mid`).
    pub name: String,
    /// Duration in minutes.
    pub minutes: u64,
    /// All scalar fields keyed by name.
    pub fields: BTreeMap<String, i64>,
}

/// The parsed soak configuration.
#[derive(Debug, Clone)]
pub struct Soak {
    /// File format version.
    pub format: i64,
    /// Profiles keyed by name.
    pub profiles: BTreeMap<String, Profile>,
}

impl Soak {
    /// Loads the soak config.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        let document = parse(&text).with_context(|| format!("parsing `{}`", path.display()))?;
        Self::from_document(&document, path)
    }

    fn from_document(document: &Document, path: &Path) -> Result<Self> {
        let format = document
            .root
            .get("format")
            .and_then(Value::as_int)
            .ok_or_else(|| anyhow!("`{}` is missing `format`", path.display()))?;

        let mut profiles = BTreeMap::new();
        for table in document
            .tables
            .iter()
            .filter(|t| t.name.starts_with("profiles."))
        {
            let name = table
                .name
                .strip_prefix("profiles.")
                .unwrap_or_default()
                .to_owned();
            let mut fields = BTreeMap::new();
            for (key, value) in &table.values {
                if let Some(number) = value.as_int() {
                    fields.insert(key.clone(), number);
                }
            }
            let minutes = fields.get("minutes").copied().unwrap_or(0).max(0) as u64;
            profiles.insert(
                name.clone(),
                Profile {
                    name,
                    minutes,
                    fields,
                },
            );
        }
        if profiles.is_empty() {
            return Err(anyhow!("`{}` has no [profiles.*] tables", path.display()));
        }
        Ok(Self { format, profiles })
    }

    /// Validates the soak config.
    pub fn check(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != 1 {
            problems.push(format!("soak: format {} != 1", self.format));
        }
        for required in ["mid", "stress"] {
            match self.profiles.get(required) {
                None => problems.push(format!("soak: missing profile `{required}`")),
                Some(profile) if profile.minutes == 0 => {
                    problems.push(format!("soak: profile `{required}` has minutes = 0"));
                }
                Some(_) => {}
            }
        }
        problems
    }

    /// Returns the profile, or an error naming the available profiles.
    pub fn profile(&self, name: &str) -> Result<&Profile> {
        self.profiles.get(name).ok_or_else(|| {
            let available: Vec<&str> = self.profiles.keys().map(String::as_str).collect();
            anyhow!("unknown soak profile `{name}`; available: {available:?}")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soak_path() -> std::path::PathBuf {
        crate::paths::find_repo_root(None)
            .expect("repo root")
            .join("parity/soak.toml")
    }

    #[test]
    fn committed_soak_config_is_valid() {
        let soak = Soak::load(&soak_path()).expect("soak");
        assert!(soak.check().is_empty(), "soak problems: {:?}", soak.check());
        assert_eq!(soak.profile("mid").expect("mid").minutes, 60);
        assert_eq!(soak.profile("stress").expect("stress").minutes, 10);
    }

    #[test]
    fn unknown_profile_is_an_error() {
        let soak = Soak::load(&soak_path()).expect("soak");
        assert!(soak.profile("nope").is_err());
    }
}

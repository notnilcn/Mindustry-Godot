// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Script-host seam (HIGH_LEVEL_PLAN §10 OD1).
//!
//! Upstream ships Rhino/JS (`mod/Scripts.java`) and Java JAR mods. Neither can
//! run without a JVM, so this module defines the replacement seam: JSON data
//! mods are full parity, script/Java code hooks are surfaced as unsupported by
//! the shipping [`NoScriptHost`]. The engine decision (WASM/Lua/GDScript) is
//! gated at plan 20 M8; this file only defines the interface.

/// Script engine failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptError {
    /// The host does not implement scripting.
    #[error("script mods are not supported in this build")]
    Unsupported,
    /// A script failed to run.
    #[error("script error: {0}")]
    Runtime(String),
}

/// Replacement for `mod/Scripts.java` + `Mod.init`/`registerServerCommands`.
///
/// Implementations must be `Send + Sync` because `Mods` is shared with the
/// asset overlay/provider passes.
pub trait ScriptHost: Send + Sync {
    /// Human-readable engine name for the dialog/migration UI.
    fn engine_name(&self) -> &'static str;

    /// Runs a mod's entry script.
    fn run_mod_script(
        &mut self,
        mod_name: &str,
        script_name: &str,
        source: &str,
    ) -> Result<(), ScriptError>;

    /// Runs a console snippet (dev tooling).
    fn run_console(&mut self, source: &str) -> Result<String, ScriptError>;

    /// Disposes per-mod script state.
    fn dispose_mod(&mut self, mod_name: &str);
}

/// Shipping default: script mods load their JSON/assets but their code hooks are
/// unsupported (plan 20 §3.10).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoScriptHost;

impl ScriptHost for NoScriptHost {
    fn engine_name(&self) -> &'static str {
        "none"
    }

    fn run_mod_script(
        &mut self,
        mod_name: &str,
        script_name: &str,
        _source: &str,
    ) -> Result<(), ScriptError> {
        log::warn!("[Mods] script mods are not supported in this build: {mod_name}/{script_name}");
        Err(ScriptError::Unsupported)
    }

    fn run_console(&mut self, _source: &str) -> Result<String, ScriptError> {
        Err(ScriptError::Unsupported)
    }

    fn dispose_mod(&mut self, _mod_name: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_host_marks_unsupported() {
        let mut host = NoScriptHost;
        assert_eq!(host.engine_name(), "none");
        assert_eq!(
            host.run_mod_script("m", "main", ""),
            Err(ScriptError::Unsupported)
        );
        assert_eq!(host.run_console("1"), Err(ScriptError::Unsupported));
        host.dispose_mod("m");
    }
}

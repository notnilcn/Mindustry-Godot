// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Launch-argument parser (plan 22 M0 §3.4).
//!
//! Ported from `desktop/src/mindustry/desktop/DesktopLauncher.java` argument
//! handling, adapted to Godot: Godot-reserved `--flags` are skipped (R13),
//! Mindustry's single-dash flags are parsed, and unknown tokens are preserved in
//! order. Pure function over `&[String]` so it is testable without Godot; the
//! window/data-dir effects are applied by `mind-gdext::platform`.

use std::path::{Path, PathBuf};

/// Godot-reserved `--flags` that must not be interpreted as Mindustry args
/// (R13). Our own double-dash aliases are handled before this list.
const GODOT_RESERVED: &[&str] = &[
    "--path",
    "--headless",
    "--editor",
    "--project-manager",
    "--quit",
    "--quit-after",
    "--audio-driver",
    "--display-driver",
    "--rendering-driver",
    "--rendering-method",
    "--main-pack",
    "--resolution",
    "--position",
    "--fullscreen",
    "--maximized",
    "--windowed",
    "--single-window",
    "--verbose",
    "--debug",
    "--profile",
    "--script",
    "--check-only",
    "--import",
    "--export-release",
    "--export-debug",
    "--export-pack",
    "--doctool",
    "--gdscript-docs",
    "--xr-mode",
    "--gpu-index",
    "--gpu-validation",
    "--benchmark",
    "--remote-debug",
    "--debug-collisions",
    "--debug-navigation",
];

/// OpenGL/backend flags accepted and ignored with a warning (deviation P22-2).
const IGNORED_GL_FLAGS: &[&str] = &[
    "-gl",
    "-coreGl",
    "-compatibilityGl",
    "-antialias",
    "-gltrace",
];

/// Parsed client launch arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchArgs {
    /// `-width N` viewport width (invalid/0 → `None`).
    pub width: Option<u32>,
    /// `-height N` viewport height (invalid/0 → `None`).
    pub height: Option<u32>,
    /// `-maximized true|false` window mode override.
    pub maximized: Option<bool>,
    /// `-debug` sets the debug log level.
    pub debug: bool,
    /// `-testMobile` / `--mobile-preview` (`Vars.testMobile`).
    pub test_mobile: bool,
    /// `-data-dir <path>` data-root override.
    pub data_dir: Option<PathBuf>,
    /// `-connect <host[:port]>` join shim (plan 21 boot flow).
    pub connect: Option<String>,
    /// `+connect_lobby <id>` Steam lobby id (OD4 seam only).
    pub connect_lobby: Option<String>,
    /// GL flags accepted but ignored (`[W]` logged once by the caller).
    pub ignored_gl: Vec<String>,
    /// Other unknown single-dash tokens, order preserved.
    pub remainder: Vec<String>,
}

/// Parses an argument list (excluding the program name).
pub fn parse<I, S>(args: I) -> LaunchArgs
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let tokens: Vec<String> = args.into_iter().map(|s| s.as_ref().to_owned()).collect();
    let mut result = LaunchArgs::default();
    let mut i = 0;
    let mut after_separator = false;

    while i < tokens.len() {
        let token = tokens[i].as_str();
        i += 1;

        if after_separator {
            result.remainder.push(token.to_owned());
            continue;
        }
        if token == "--" {
            after_separator = true;
            continue;
        }
        if GODOT_RESERVED.contains(&token) {
            // Godot-reserved flags either take no value or own their own
            // parsing; skip the flag and, for the known value-taking forms,
            // consume the following token only when it does not look like a
            // flag (`--path foo`).
            if matches!(
                token,
                "--path" | "--main-pack" | "--resolution" | "--position"
            ) && i < tokens.len()
            {
                i += 1;
            }
            continue;
        }

        match token {
            "-width" => {
                result.width = take(&tokens, &mut i).and_then(|v| parse_dimension(&v));
            }
            "-height" => {
                result.height = take(&tokens, &mut i).and_then(|v| parse_dimension(&v));
            }
            "-maximized" => {
                result.maximized = take(&tokens, &mut i).map(|v| v == "true");
            }
            "-debug" => result.debug = true,
            "-testMobile" | "--mobile-preview" => result.test_mobile = true,
            "-data-dir" | "--data-dir" => {
                result.data_dir = take(&tokens, &mut i).map(PathBuf::from);
            }
            "-connect" | "--connect" => {
                result.connect = take(&tokens, &mut i);
            }
            "+connect_lobby" => {
                result.connect_lobby = take(&tokens, &mut i);
            }
            other if IGNORED_GL_FLAGS.contains(&other) => {
                result.ignored_gl.push(other.to_owned());
            }
            other if other.starts_with("--") => {
                // Remaining Godot flags (including `--export-*` variants).
                continue;
            }
            other => result.remainder.push(other.to_owned()),
        }
    }

    result
}

fn take(tokens: &[String], index: &mut usize) -> Option<String> {
    if *index < tokens.len() {
        let value = tokens[*index].clone();
        *index += 1;
        Some(value)
    } else {
        None
    }
}

fn parse_dimension(value: &str) -> Option<u32> {
    match value.trim().parse::<u32>() {
        Ok(0) | Err(_) => None,
        Ok(n) => Some(n),
    }
}

/// Resolves the data root per plan 22 §6.1 (R4/C5).
///
/// Precedence: `--data-dir` (arg wins) → `MIND_DATA_DIR` →
/// `MINDUSTRY_GODOT_DATA_DIR` → `MINDUSTRY_DATA_DIR` → `None` (caller uses the
/// platform default). The environment is injected for testability.
pub fn data_root<F>(arg: Option<&Path>, env: F) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(path) = arg {
        return Some(path.to_path_buf());
    }
    for key in [
        "MIND_DATA_DIR",
        "MINDUSTRY_GODOT_DATA_DIR",
        "MINDUSTRY_DATA_DIR",
    ] {
        if let Some(value) = env(key)
            && !value.is_empty()
        {
            return Some(PathBuf::from(value));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_height_maximized() {
        let args = parse(["-width", "1200", "-height", "800", "-maximized", "false"]);
        assert_eq!(args.width, Some(1200));
        assert_eq!(args.height, Some(800));
        assert_eq!(args.maximized, Some(false));

        let invalid = parse(["-width", "0", "-height", "abc"]);
        assert_eq!(invalid.width, None);
        assert_eq!(invalid.height, None);
    }

    #[test]
    fn data_dir_precedence() {
        let env = |key: &str| match key {
            "MIND_DATA_DIR" => Some("/env/mind".to_owned()),
            "MINDUSTRY_GODOT_DATA_DIR" => Some("/env/godot".to_owned()),
            "MINDUSTRY_DATA_DIR" => Some("/env/upstream".to_owned()),
            _ => None,
        };
        // Arg wins over every env var.
        assert_eq!(
            data_root(Some(Path::new("/arg/data")), env),
            Some(PathBuf::from("/arg/data"))
        );
        // First env in the list wins when there is no arg.
        assert_eq!(data_root(None, env), Some(PathBuf::from("/env/mind")));
        // Fallback chain.
        let only_upstream =
            |key: &str| (key == "MINDUSTRY_DATA_DIR").then(|| "/env/upstream".to_owned());
        assert_eq!(
            data_root(None, only_upstream),
            Some(PathBuf::from("/env/upstream"))
        );
        assert_eq!(data_root(None, |_| None), None);
    }

    #[test]
    fn gl_flags_ignored() {
        let args = parse([
            "-gl",
            "-coreGl",
            "-compatibilityGl",
            "-antialias",
            "-gltrace",
        ]);
        assert_eq!(args.ignored_gl.len(), 5);
        assert!(args.remainder.is_empty());
    }

    #[test]
    fn connect_lobby_pair() {
        let args = parse(["-debug", "+connect_lobby", "12345"]);
        assert!(args.debug);
        assert_eq!(args.connect_lobby.as_deref(), Some("12345"));

        let connect = parse(["-connect", "127.0.0.1:6567"]);
        assert_eq!(connect.connect.as_deref(), Some("127.0.0.1:6567"));
    }

    #[test]
    fn debug_and_test_mobile_aliases() {
        assert!(parse(["-debug"]).debug);
        assert!(parse(["-testMobile"]).test_mobile);
        assert!(parse(["--mobile-preview"]).test_mobile);
    }

    #[test]
    fn godot_flags_skipped_and_unknown_preserved_in_order() {
        let args = parse(["--headless", "-foo", "--resolution", "1280x720", "bar"]);
        assert_eq!(args.remainder, vec!["-foo", "bar"]);
    }

    #[test]
    fn separator_stops_flag_parsing() {
        let args = parse(["-debug", "--", "-width", "10"]);
        assert!(args.debug);
        assert_eq!(args.width, None);
        assert_eq!(args.remainder, vec!["-width", "10"]);
    }
}

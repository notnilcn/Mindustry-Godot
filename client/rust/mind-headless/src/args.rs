// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Launch-argument parsing, re-exported from `mind-core` (plan 22 M0 §3.4/§7a).
//!
//! The parser lives in `mind_core::platform::args` so it stays Godot-free and
//! usable by both `mind-gdext` and the headless harness; this module is the
//! plan-22 §7a test surface (`mind_headless::args::tests`).

pub use mind_core::platform::args::{LaunchArgs, data_root, parse};

#[cfg(test)]
mod tests {
    //! Mirrors of the plan-22 §7a `mind_headless::args::tests` names; the
    //! implementation tests live in `mind_core::platform::args::tests`.
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn width_height_maximized() {
        let args = parse(["-width", "800", "-height", "600", "-maximized", "true"]);
        assert_eq!(args.width, Some(800));
        assert_eq!(args.height, Some(600));
        assert_eq!(args.maximized, Some(true));
    }

    #[test]
    fn data_dir_precedence() {
        assert_eq!(
            data_root(Some(Path::new("/x")), |_| Some("/y".to_owned())),
            Some(PathBuf::from("/x"))
        );
    }

    #[test]
    fn gl_flags_ignored() {
        assert_eq!(parse(["-gl"]).ignored_gl, vec!["-gl"]);
    }

    #[test]
    fn connect_lobby_pair() {
        assert_eq!(
            parse(["+connect_lobby", "42"]).connect_lobby.as_deref(),
            Some("42")
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-only

//! Generated asset id tables (`Sounds`/`Musics`/`Tex`/`Icon`/`Iconc`).
//!
//! `build.rs` scans the vendored inputs listed in plan 03 §6.8 and writes
//! `$OUT_DIR/asset_ids.rs`; this module includes it so plain `cargo build`
//! works without running the pack pipeline first.

include!(concat!(env!("OUT_DIR"), "/asset_ids.rs"));

#[cfg(test)]
mod tests {
    /// Plan 03 §7.1a smoke check: the code tables are populated from the
    /// vendored inputs with the upstream name mangling.
    #[test]
    fn generated_ids_are_populated() {
        assert_eq!(super::sounds::NONE, "none");
        assert_eq!(super::sounds::UI_BACK, "uiBack");
        assert_eq!(super::musics::GAME1, "game1");
        assert_eq!(super::tex::BAR, "bar");
        assert!(!super::iconc::BLOCK_COPPER_WALL.is_ascii());
        assert!(!super::icon::MAP.is_ascii());
    }
}

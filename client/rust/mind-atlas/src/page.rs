// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `core/src/mindustry/graphics/MultiPacker.java` (`PageType`) +
// `core/src/mindustry/mod/Mods.java` (`getPage` routing).

//! Atlas page types and routing (plan 03 §3.4/§6.2).

/// The four page sets (plan 03 §6.2; runtime overlay routing in §3.8).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum PageType {
    /// Units, weapons, placeable blocks, effects, bullets.
    Main,
    /// Environmental cache-layer sprites.
    Environment,
    /// Content icons, white icons, UI elements.
    Ui,
    /// Scorch textures and wrecks.
    Rubble,
}

impl PageType {
    /// All page types in `MultiPacker.PageType.all` order.
    pub const ALL: [PageType; 4] = [
        PageType::Main,
        PageType::Environment,
        PageType::Ui,
        PageType::Rubble,
    ];

    /// Logical name (`PageType.name()`; manifest `type`/`pageType` values).
    pub const fn name(self) -> &'static str {
        match self {
            PageType::Main => "main",
            PageType::Environment => "environment",
            PageType::Ui => "ui",
            PageType::Rubble => "rubble",
        }
    }

    /// Parses a manifest page-type name.
    pub fn parse(name: &str) -> Option<PageType> {
        match name {
            "main" => Some(PageType::Main),
            "environment" => Some(PageType::Environment),
            "ui" => Some(PageType::Ui),
            "rubble" => Some(PageType::Rubble),
            _ => None,
        }
    }

    /// Page routing by staging path substring (`Mods.getPage`).
    pub fn for_path(path: &str) -> PageType {
        if path.contains("blocks/environment") {
            PageType::Environment
        } else if path.contains("rubble") {
            PageType::Rubble
        } else if path.contains("ui") {
            PageType::Ui
        } else {
            PageType::Main
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_matches_mods_get_page() {
        assert_eq!(
            PageType::for_path("blocks/environment/dirt.png"),
            PageType::Environment
        );
        assert_eq!(
            PageType::for_path("rubble/scorch-0-0.png"),
            PageType::Rubble
        );
        assert_eq!(PageType::for_path("ui/bar.9.png"), PageType::Ui);
        assert_eq!(
            PageType::for_path("blocks/walls/copper-wall.png"),
            PageType::Main
        );
    }
}

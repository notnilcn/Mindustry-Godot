// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Placement-palette block catalogue: the `PlacementFragment` category/block
//! read model and the `BuildVisibility` rules context the palette filters on.
//! Upstream: core/src/mindustry/ui/fragments/PlacementFragment.java

use serde::Serialize;

use crate::content::{ContentRegistry, UnlockStore};
use crate::game::tech_tree;

/// One block entry in the placement palette (`PlacementFragment`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockEntryView {
    /// Content name.
    pub name: String,
    /// Bundle key for the display label (`block.<name>.name`) or the localized
    /// name when base content carries one.
    pub localized: String,
    /// Multiblock size in tiles.
    pub size: i32,
}

/// One placement-palette category (`PlacementFragment` category rail).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockCategoryView {
    /// `Category.name()`.
    pub name: &'static str,
    /// `Category.ordinal()` — the category hotkey digit derives from it
    /// (`blockSelect[ordinal]` in `PlacementFragment`). Appended field; the
    /// JSON contract stays append-only.
    pub ordinal: usize,
    /// Localized label bundle key (`database-tag.<name>`).
    pub label: String,
    /// Buildable blocks in content order.
    pub blocks: Vec<BlockEntryView>,
}

/// The placement-palette catalog consumed by `placement_fragment.gd`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockCatalogView {
    /// Initially selected block name.
    pub selected: String,
    /// Non-empty categories in `Category.all` order.
    pub categories: Vec<BlockCategoryView>,
}

impl BlockCatalogView {
    /// Empty catalog (content boot failure; never panics in a `#[func]`).
    pub fn empty() -> Self {
        Self {
            selected: String::new(),
            categories: Vec::new(),
        }
    }
}

/// Builds the placement-palette catalog from base content.
///
/// The custom/sandbox inventory: `unlockedNowHost` is true outside a campaign,
/// but `PlacementFragment.getUnlockedByCategory` still hides hidden/debug/
/// editor content, other-planet content and non-placeable blocks. The default
/// `State.getPlanet()` (Serpulo) supplies the environment context.
pub fn block_catalog() -> BlockCatalogView {
    block_catalog_with(None, None)
}

/// Custom/sandbox catalog for one planet (`Block.environmentBuildable`).
pub fn block_catalog_for(planet: &str) -> BlockCatalogView {
    block_catalog_with(None, Some(planet))
}

/// Builds the placement-palette catalog filtered by live unlock state.
///
/// `store` is the campaign unlock store: tech-gated blocks are dropped until
/// their `<name>-unlocked` bit is set (`Block.unlockedNowHost`) and the
/// campaign's non-build `BuildVisibility` values are removed. The generated
/// specs leave `build_visibility` to the resolver, which marks
/// requirement-bearing build-menu blocks `Shown` and requirement-less world
/// content `Hidden` (`Block.requirements(Category, ItemStack...)` defaults to
/// `BuildVisibility.shown`), so only real buildables reach this filter.
/// [`block_catalog`] keeps the no-unlock inventory when no store is bound.
pub fn block_catalog_unlocked(store: &dyn UnlockStore) -> BlockCatalogView {
    block_catalog_with(Some(store), None)
}

/// Unlock-filtered catalog for one campaign planet
/// (`PlacementFragment.getUnlockedByCategory` reads `state.getPlanet()` for
/// `Block.environmentBuildable`).
pub fn block_catalog_unlocked_for(store: &dyn UnlockStore, planet: &str) -> BlockCatalogView {
    block_catalog_with(Some(store), Some(planet))
}

/// Whether a tech node is granted at boot: no effective item requirements and
/// no objectives (`Control.checkAutoUnlocks` roots).
///
/// The generated vanilla trees leave `TechNode.requirements` empty and derive
/// the research cost from the content
/// (`UnlockableContent.researchRequirements()`), so the check must go through
/// [`tech_tree::effective_requirements`] instead of the stored list.
fn tech_node_auto_unlocks(
    registry: &ContentRegistry,
    node_ref: crate::content::tech::TechNodeRef,
) -> bool {
    let Some(node) = registry.tech().node(node_ref) else {
        return false;
    };
    tech_tree::effective_requirements(registry, node_ref).is_empty() && node.objectives.is_empty()
}

/// Shared catalog builder; `store` enables the unlock/visibility filter and
/// `planet` overrides the default Serpulo `environmentBuildable` context.
fn block_catalog_with(store: Option<&dyn UnlockStore>, planet: Option<&str>) -> BlockCatalogView {
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

    let empty = MemoryUnlockStore::new();
    let store_ref: &dyn UnlockStore = match store {
        Some(store) => store,
        None => &empty,
    };
    let Ok(mut registry) = create_base_content(&MemoryBundle::new(), store_ref, true) else {
        return BlockCatalogView::empty();
    };
    // `init`/`post_init` derive per-block fields the build-menu filter reads
    // (`build_visibility`, `build_time`) and auto-assign `shownPlanets`.
    if registry.init().is_err() || registry.post_init().is_err() {
        return BlockCatalogView::empty();
    }
    block_catalog_from(&registry, store, planet)
}

/// Rules context for `BuildVisibility.visible()` and `Block.isVisible()`
/// (`world/meta/BuildVisibility.java`, `Block.java:1031-1033,1553-1555`).
///
/// The palette filter evaluates each block's `build_visibility` tag against
/// this context rather than a fixed variant list: the editor-only world-logic
/// blocks (`worldProcessorOnly`) stay out of a sandbox palette while the editor
/// and `allowEditWorldProcessors` maps list them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BlockVisibilityContext {
    /// `state.rules.editor`.
    pub editor: bool,
    /// `state.rules.allowEditWorldProcessors`.
    pub allow_edit_world_processors: bool,
    /// `state.rules.infiniteResources` (the global flag, not the team rule).
    pub infinite_resources: bool,
    /// `state.isCampaign()`.
    pub campaign: bool,
    /// `state.isGame()` (false in the menu).
    pub is_game: bool,
    /// `state.rules.lighting`.
    pub lighting: bool,
    /// `state.rules.fog`.
    pub fog: bool,
    /// `indexer.isBlockPresent(Blocks.coreZone)`.
    pub core_zone_present: bool,
    /// `state.getPlanet().campaignRules.legacyLaunchPads`.
    pub legacy_launch_pads: bool,
    /// `Blocks.advancedLaunchPad.unlocked()`.
    pub advanced_launch_pad_unlocked: bool,
    /// `state.rules.hideBannedBlocks`.
    pub hide_banned_blocks: bool,
    /// `state.rules.blockWhitelist`.
    pub block_whitelist: bool,
    /// `state.rules.bannedBlocks` (content names).
    pub banned_blocks: Vec<String>,
    /// `state.rules.revealedBlocks` (content names).
    pub revealed_blocks: Vec<String>,
}

impl BlockVisibilityContext {
    /// The rules-less custom/sandbox inventory: an in-game, non-campaign
    /// session with `infiniteResources`, so the sandbox blocks stay listed.
    pub fn sandbox() -> Self {
        Self {
            is_game: true,
            infinite_resources: true,
            ..Self::default()
        }
    }

    /// The rules-less campaign context (`state.isCampaign()`, default rules).
    pub fn campaign() -> Self {
        Self {
            is_game: true,
            campaign: true,
            ..Self::default()
        }
    }

    /// Context from the live match rules; `campaign` is `state.isCampaign()`.
    /// The world-derived inputs (`core_zone_present`, `legacy_launch_pads`,
    /// `advanced_launch_pad_unlocked`) stay `false` for the caller to fill.
    pub fn from_rules(rules: &crate::io::json::rules::Rules, campaign: bool) -> Self {
        Self {
            editor: rules.editor,
            allow_edit_world_processors: rules.allow_edit_world_processors,
            infinite_resources: rules.infinite_resources,
            campaign,
            is_game: true,
            lighting: rules.lighting,
            fog: rules.fog,
            hide_banned_blocks: rules.hide_banned_blocks,
            block_whitelist: rules.block_whitelist,
            banned_blocks: rules.banned_blocks.iter().cloned().collect(),
            revealed_blocks: rules.revealed_blocks.iter().cloned().collect(),
            ..Self::default()
        }
    }

    /// `BuildVisibility.visible()` (`BuildVisibility.java:10-22`).
    pub fn build_visible(&self, visibility: crate::content::BuildVisibility) -> bool {
        use crate::content::BuildVisibility as V;
        match visibility {
            V::Hidden | V::DebugOnly => false,
            V::Shown => true,
            V::EditorOnly => self.editor,
            V::CoreZoneOnly => self.core_zone_present || !self.is_game,
            V::WorldProcessorOnly => self.editor || self.allow_edit_world_processors,
            V::SandboxOnly => self.infinite_resources,
            V::CampaignOnly => self.campaign || !self.is_game,
            V::LegacyLaunchPadOnly => {
                self.campaign && self.legacy_launch_pads && self.advanced_launch_pad_unlocked
            }
            V::NotLegacyLaunchPadOnly => {
                !self.is_game
                    || self.infinite_resources
                    || (self.campaign && !self.legacy_launch_pads)
            }
            V::LightingOnly => {
                self.lighting || self.campaign || !self.is_game || self.infinite_resources
            }
            V::FogOnly => self.fog || self.editor || !self.is_game,
        }
    }

    /// `Rules.isBanned(Block)` (`blockWhitelist != bannedBlocks.contains`).
    pub fn is_banned(&self, name: &str) -> bool {
        self.block_whitelist != self.banned_blocks.iter().any(|banned| banned == name)
    }

    /// `Block.isVisible()`: `!isHidden() && (editor || !hideBannedBlocks ||
    /// !isBanned())`, where `isHidden()` is `!buildVisibility.visible() &&
    /// !revealedBlocks.contains(this)`.
    pub fn block_visible(&self, name: &str, visibility: crate::content::BuildVisibility) -> bool {
        !self.block_hidden(name, visibility)
            && (self.editor || !self.hide_banned_blocks || !self.is_banned(name))
    }

    /// `Block.isHidden()` (`Block.java:1553-1555`): `!buildVisibility.visible()
    /// && !state.rules.revealedBlocks.contains(this)` — the `ItemSelection`
    /// filter term for block lists (`ItemSelection.java:68`; L02 T3).
    pub fn block_hidden(&self, name: &str, visibility: crate::content::BuildVisibility) -> bool {
        !self.build_visible(visibility)
            && !self.revealed_blocks.iter().any(|revealed| revealed == name)
    }
}

/// Catalog over a caller-supplied registry: the live session registry in the
/// client, so mod content is in the palette exactly like Java's
/// `PlacementFragment.getUnlockedByCategory` reads the global
/// `Vars.content.blocks()` (`PlacementFragment.java:771-781`). `store` enables
/// the unlock/visibility filter and `planet` overrides the default Serpulo
/// `environmentBuildable` context. Without live rules the visibility context
/// is [`BlockVisibilityContext::campaign`] when a store is bound, else
/// [`BlockVisibilityContext::sandbox`].
pub fn block_catalog_from(
    registry: &ContentRegistry,
    store: Option<&dyn UnlockStore>,
    planet: Option<&str>,
) -> BlockCatalogView {
    let visibility = if store.is_some() {
        BlockVisibilityContext::campaign()
    } else {
        BlockVisibilityContext::sandbox()
    };
    block_catalog_visible(registry, store, planet, &visibility)
}

/// [`block_catalog_from`] under the live rules' visibility context
/// (`getUnlockedByCategory` filters on `Block.isVisible()`, which reads
/// `state.rules`; `PlacementFragment.java:771-773`).
pub fn block_catalog_visible(
    registry: &ContentRegistry,
    store: Option<&dyn UnlockStore>,
    planet: Option<&str>,
    visibility: &BlockVisibilityContext,
) -> BlockCatalogView {
    use crate::content::{Category, PlanetId};

    // `State.getPlanet()` falls back to the rules planet (`serpulo` by default).
    let planet: Option<PlanetId> = planet
        .and_then(|name| registry.planet_id(name))
        .or_else(|| registry.planet_id("serpulo"));
    let mut categories: Vec<BlockCategoryView> = Category::ALL
        .iter()
        .enumerate()
        .map(|(ordinal, category)| BlockCategoryView {
            name: category.name(),
            ordinal,
            label: format!("database-tag.{}", category.name()),
            blocks: Vec::new(),
        })
        .collect();
    for block in registry.blocks() {
        // `Block.isVisible()` (`Block.java:1031-1033`): the block's
        // `BuildVisibility` predicate against the rules, so hidden/debug/editor
        // content and the `worldProcessorOnly` world-logic blocks stay out of a
        // play palette whichever registry (base or live) is passed in.
        if block.removed || !visibility.block_visible(&block.name, block.build_visibility) {
            continue;
        }
        // `PlacementFragment.unlocked(block)`: player-placeable content only.
        if !block.placeable_player {
            continue;
        }
        // `Block.environmentBuildable()` (`isOnPlanet`): the auto-assigned
        // `shownPlanets` drop other planets' content (e.g. Erekir cores).
        if let Some(planet) = planet
            && !block.unlock.shown_planets.is_empty()
            && !block.unlock.shown_planets.contains(&planet)
        {
            continue;
        }
        if store.is_some() {
            // Tech-gated blocks need their unlock bit (or be a zero-requirement,
            // objective-free root that `check_auto_unlocks` grants at boot);
            // blocks outside the tech tree stay available (the port's specs do
            // not model `alwaysUnlocked` for them).
            if let Some(node_ref) = block.unlock.tech_node
                && !block.unlock.unlocked()
                && !tech_node_auto_unlocks(registry, node_ref)
            {
                continue;
            }
        }
        let localized = if block.unlock.localized_name.is_empty()
            || block.unlock.localized_name == block.name
        {
            format!("block.{}.name", block.name)
        } else {
            block.unlock.localized_name.clone()
        };
        categories[block.category.ordinal()]
            .blocks
            .push(BlockEntryView {
                name: block.name.clone(),
                localized,
                size: block.size,
            });
    }
    categories.retain(|category| !category.blocks.is_empty());
    BlockCatalogView {
        selected: String::from("conveyor"),
        categories,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_catalog_covers_build_menu() {
        let catalog = block_catalog();
        assert!(
            catalog.categories.len() >= 4,
            "several non-empty categories: {}",
            catalog.categories.len()
        );
        assert!(
            catalog
                .categories
                .iter()
                .all(|category| !category.blocks.is_empty()),
            "no empty category survives the filter"
        );
        let has = |name: &str| {
            catalog
                .categories
                .iter()
                .any(|category| category.blocks.iter().any(|block| block.name == name))
        };
        assert!(has("conveyor"), "conveyor is in the distribution category");
        // World/editor content carries no build requirements, so the resolver
        // leaves it `BuildVisibility.hidden` (`PlacementFragment.getByCategory`
        // only sees visible, placeable, environment-buildable blocks).
        for world in [
            "air",
            "spawn",
            "remove-wall",
            "deep-water",
            "stone-wall",
            "ore-copper",
            "build2",
            "legacy-mech-pad",
            "command-center",
        ] {
            assert!(!has(world), "world content `{world}` is not build-menu");
        }
        // `Block.environmentBuildable()`: other planets' content stays out of
        // the Serpulo palette (the default `State.getPlanet()`).
        assert!(!has("core-bastion"), "the Erekir core is hidden on Serpulo");
        assert!(
            catalog
                .categories
                .iter()
                .all(|category| { category.label == format!("database-tag.{}", category.name) }),
            "category labels are database-tag bundle keys"
        );
    }

    #[test]
    fn unlocked_catalog_filters_visibility_and_tech_gates() {
        use crate::content::MemoryUnlockStore;

        let full = block_catalog();
        let has = |catalog: &BlockCatalogView, name: &str| {
            catalog
                .categories
                .iter()
                .any(|category| category.blocks.iter().any(|block| block.name == name))
        };
        assert!(
            has(&full, "power-source"),
            "sandbox block in the raw inventory"
        );
        let filtered = block_catalog_unlocked(&MemoryUnlockStore::new());
        assert!(
            filtered.categories.is_empty(),
            "a fresh campaign with no unlock bits has no build-menu content: {:?}",
            filtered
                .categories
                .iter()
                .map(|category| category.name)
                .collect::<Vec<_>>()
        );
        assert!(
            !has(&filtered, "power-source"),
            "sandbox-only block is not buildable in campaign"
        );
        assert!(
            !has(&filtered, "conveyor"),
            "a research-gated block is hidden with an empty unlock store"
        );
        for locked in ["foreshadow", "spectre", "meltdown", "malign"] {
            assert!(
                !has(&filtered, locked),
                "locked turret `{locked}` is not buildable"
            );
        }
        assert!(
            filtered
                .categories
                .iter()
                .all(|category| !category.blocks.is_empty()),
            "empty categories are hidden"
        );

        // The `<name>-unlocked` bit (what research and sector auto-unlocks
        // write) puts the block back into the palette.
        let mut store = MemoryUnlockStore::new();
        store.set_bool("duo-unlocked", true);
        let unlocked = block_catalog_unlocked(&store);
        let turret = unlocked
            .categories
            .iter()
            .find(|category| category.name == "turret")
            .expect("unlocking a turret yields its category");
        assert_eq!(
            turret
                .blocks
                .iter()
                .map(|block| block.name.as_str())
                .collect::<Vec<_>>(),
            vec!["duo"],
            "exactly the researched block appears"
        );
        assert_eq!(
            unlocked.categories.len(),
            1,
            "other categories stay hidden: {:?}",
            unlocked
                .categories
                .iter()
                .map(|category| category.name)
                .collect::<Vec<_>>()
        );
        assert!(
            !has(&unlocked, "foreshadow"),
            "still-locked turret stays hidden"
        );

        // `Block.environmentBuildable()`: the same unlock state yields another
        // planet's palette (Erekir exposes its always-unlocked core).
        let erekir = block_catalog_unlocked_for(&MemoryUnlockStore::new(), "erekir");
        assert!(
            has(&erekir, "core-bastion"),
            "Erekir's always-unlocked core is in the Erekir palette"
        );
        assert!(!has(&erekir, "duo"), "Serpulo content stays off Erekir");

        // The custom/sandbox catalog takes the same planet context.
        let erekir_full = block_catalog_for("erekir");
        assert!(has(&erekir_full, "core-bastion"));
        assert!(!has(&erekir_full, "duo"));
    }

    /// I07 T6 (PAR-0115): the client builds the palette over the live session
    /// registry (`Vars.content.blocks()`, `PlacementFragment.java:771-781`), so
    /// an enabled mod's block reaches the sandbox placement palette.
    #[test]
    fn block_catalog_from_lists_live_mod_content() {
        use crate::content::boot::{build_registry_with_mods, collect_boot_content};
        use crate::content::{MemoryBundle, MemoryUnlockStore};
        use crate::io::SettingsStore;
        use crate::io::fs::NativeFs;
        use crate::mods::Mods;
        use std::path::Path;

        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../parity/mod_fixtures/allkinds");
        let fs = NativeFs;
        let mut mods = Mods::new(true, &dir);
        mods.load_single(&fs, &dir, &SettingsStore::new())
            .expect("load allkinds");
        let boot = collect_boot_content(&mut mods, &fs);
        let (registry, report) = build_registry_with_mods(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
            &boot,
            &[],
        )
        .expect("boot with mods");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let catalog = block_catalog_from(&registry, None, Some("serpulo"));
        let mod_blocks = catalog
            .categories
            .iter()
            .flat_map(|category| category.blocks.iter())
            .filter(|block| block.name.starts_with("all-kinds-"))
            .map(|block| block.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            mod_blocks,
            vec!["all-kinds-all-wall"],
            "the modded block is in the live-registry palette"
        );

        // The base-content builder stays mod-free (the no-session fallback).
        let base = block_catalog_for("serpulo");
        assert!(
            !base.categories.iter().any(|category| category
                .blocks
                .iter()
                .any(|block| block.name == "all-kinds-all-wall")),
            "base content has no mod blocks"
        );
    }

    /// Twin-verification bug 5: `Block.isVisible()` reads the rules, so the
    /// four `worldProcessorOnly` world-logic blocks stay out of the sandbox
    /// palette (`BuildVisibility.java:16`) and appear only in the editor or
    /// with `allowEditWorldProcessors`.
    #[test]
    fn block_catalog_applies_build_visibility_rules() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::io::json::rules::Rules;

        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content boot");
        registry.init().expect("init");
        registry.post_init().expect("post_init");
        let has = |catalog: &BlockCatalogView, name: &str| {
            catalog
                .categories
                .iter()
                .any(|category| category.blocks.iter().any(|block| block.name == name))
        };
        const WORLD_LOGIC: [&str; 4] = [
            "world-processor",
            "world-cell",
            "world-message",
            "world-switch",
        ];

        // Sandbox: infinite resources, not the editor.
        let sandbox_rules = Rules {
            infinite_resources: true,
            ..Rules::default()
        };
        let sandbox = block_catalog_visible(
            &registry,
            None,
            Some("serpulo"),
            &BlockVisibilityContext::from_rules(&sandbox_rules, false),
        );
        for name in WORLD_LOGIC {
            assert!(!has(&sandbox, name), "`{name}` is hidden in a sandbox");
        }
        assert!(has(&sandbox, "micro-processor"));
        assert!(has(&sandbox, "power-source"), "sandboxOnly is listed");
        // The rules-less fallback (`MindInput`'s catalog) agrees.
        let fallback = block_catalog_from(&registry, None, Some("serpulo"));
        for name in WORLD_LOGIC {
            assert!(!has(&fallback, name), "`{name}` is hidden without rules");
        }

        // Survival custom game: no infinite resources hides `sandboxOnly`.
        let survival = block_catalog_visible(
            &registry,
            None,
            Some("serpulo"),
            &BlockVisibilityContext::from_rules(&Rules::default(), false),
        );
        assert!(!has(&survival, "power-source"));
        assert!(has(&survival, "conveyor"));

        // `allowEditWorldProcessors` (and the editor) list them.
        let privileged_rules = Rules {
            allow_edit_world_processors: true,
            ..Rules::default()
        };
        let privileged = block_catalog_visible(
            &registry,
            None,
            Some("serpulo"),
            &BlockVisibilityContext::from_rules(&privileged_rules, false),
        );
        for name in WORLD_LOGIC {
            assert!(
                has(&privileged, name),
                "`{name}` with allowEditWorldProcessors"
            );
        }

        // `hideBannedBlocks` drops a banned block outside the editor.
        let mut banned_rules = sandbox_rules.clone();
        banned_rules.hide_banned_blocks = true;
        banned_rules.banned_blocks.insert("conveyor".to_owned());
        let banned = block_catalog_visible(
            &registry,
            None,
            Some("serpulo"),
            &BlockVisibilityContext::from_rules(&banned_rules, false),
        );
        assert!(!has(&banned, "conveyor"));
        assert!(has(&banned, "junction"));
    }

    #[test]
    fn build_visibility_predicates_match_upstream() {
        use crate::content::BuildVisibility as V;

        let sandbox = BlockVisibilityContext::sandbox();
        assert!(!sandbox.build_visible(V::WorldProcessorOnly));
        assert!(!sandbox.build_visible(V::EditorOnly));
        assert!(sandbox.build_visible(V::SandboxOnly));
        assert!(!sandbox.build_visible(V::CampaignOnly));
        assert!(!sandbox.build_visible(V::CoreZoneOnly));
        assert!(sandbox.build_visible(V::LightingOnly));
        assert!(!sandbox.build_visible(V::FogOnly));

        let campaign = BlockVisibilityContext::campaign();
        assert!(!campaign.build_visible(V::SandboxOnly));
        assert!(campaign.build_visible(V::CampaignOnly));
        assert!(campaign.build_visible(V::LightingOnly));
        assert!(campaign.build_visible(V::NotLegacyLaunchPadOnly));
        assert!(!campaign.build_visible(V::LegacyLaunchPadOnly));

        let editor = BlockVisibilityContext {
            editor: true,
            is_game: true,
            ..BlockVisibilityContext::default()
        };
        assert!(editor.build_visible(V::WorldProcessorOnly));
        assert!(editor.build_visible(V::EditorOnly));
        assert!(editor.build_visible(V::FogOnly));
        assert!(!editor.build_visible(V::DebugOnly));
        assert!(!editor.build_visible(V::Hidden));

        // `revealedBlocks` un-hides a block whose visibility is false.
        let revealed = BlockVisibilityContext {
            revealed_blocks: vec!["world-processor".to_owned()],
            ..BlockVisibilityContext::sandbox()
        };
        assert!(revealed.block_visible("world-processor", V::WorldProcessorOnly));
        assert!(!revealed.block_visible("world-cell", V::WorldProcessorOnly));
    }
}

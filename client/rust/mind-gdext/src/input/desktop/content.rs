// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Free content/config helpers: icon options, shapes and camera world corners.
//! Upstream: core/src/mindustry/input/DesktopInput.java

use super::*;

/// `UnlockableContent.isOnPlanet` (`UnlockableContent.java:132-134`):
/// `planet == null || planet == Planets.sun || shownPlanets.isEmpty() ||
/// shownPlanets.contains(planet)`.
fn content_on_planet(
    content: &ContentRegistry,
    unlock: Option<&mind_core::content::UnlockFields>,
    planet: Option<mind_core::content::PlanetId>,
) -> bool {
    let Some(planet) = planet else {
        return true;
    };
    if content.planet_id("sun") == Some(planet) {
        return true;
    }
    unlock.is_none_or(|unlock| {
        unlock.shown_planets.is_empty() || unlock.shown_planets.contains(&planet)
    })
}

/// One `config_spec` option entry carrying the `ItemSelection` fields
/// (`ItemSelection.java:66-75`): the `uiIcon` fallback chain
/// (`UnlockableContent.java:108-117`), the icon size
/// (`UnlockableContent.java:44`, `Block.java:450`, `UnitType.java:534`) and
/// the unlocked/planet filter result (`ItemSelection.java:68`).
pub(super) fn content_option_json(
    content: &ContentRegistry,
    content_type: ContentType,
    name: &str,
    label: String,
    planet: Option<mind_core::content::PlanetId>,
    unlocked_now: &dyn Fn(&str) -> bool,
    visibility: &mind_core::ui::campaign::BlockVisibilityContext,
) -> serde_json::Value {
    let reference = content.get_by_name(content_type, name);
    let hidden = content_hidden(content, content_type, name, visibility);
    let unlock = reference
        .and_then(|reference| mind_core::game::tech_tree::unlock_fields(content, reference));
    let prefix = content_type.name();
    // `UnlockableContent.loadIcon` (`UnlockableContent.java:108-117`):
    // `{type}-{name}-ui`, else the `fullOverride`/`{type}-{name}-full` chain.
    let mut icons = vec![format!("{prefix}-{name}-ui")];
    if let Some(override_name) = unlock.map(|unlock| unlock.full_override.as_str())
        && !override_name.is_empty()
    {
        icons.push(override_name.to_owned());
    }
    for candidate in [
        format!("{prefix}-{name}-full"),
        format!("{name}-full"),
        name.to_owned(),
        format!("{prefix}-{name}"),
        format!("{name}1"),
    ] {
        icons.push(candidate);
    }
    // `ItemSelection.java:70`: `Mathf.clamp(item.selectionSize, 0f, 40f)`.
    let size = match content_type {
        ContentType::Block => 28.0,
        ContentType::Unit => 30.0,
        _ => unlock.map(|unlock| unlock.selection_size).unwrap_or(24.0),
    };
    serde_json::json!({
        "name": name,
        "label": label,
        "size": size,
        "icons": icons,
        "unlocked": unlocked_now(name),
        // `Item.isOnPlanet` adds `&& !hidden` (`Item.java:61-64`).
        "on_planet": content_on_planet(content, unlock, planet)
            && !(content_type == ContentType::Item && hidden),
        // `item.isHidden()` (`ItemSelection.java:68`; L02 T3).
        "hidden": hidden,
    })
}

/// `UnlockableContent.isHidden()` per content type: `Item.hidden`
/// (`Item.java:67-69`), `Liquid.hidden` (`Liquid.java:102-104`),
/// `UnitType.hidden` (`UnitType.java:785-787`) and `Block.isHidden`
/// (`Block.java:1553-1555`, rules-dependent build visibility).
fn content_hidden(
    content: &ContentRegistry,
    content_type: ContentType,
    name: &str,
    visibility: &mind_core::ui::campaign::BlockVisibilityContext,
) -> bool {
    match content_type {
        ContentType::Item => content
            .item_by_name(name)
            .is_some_and(|item| item.is_hidden()),
        ContentType::Liquid => content
            .liquid_by_name(name)
            .is_some_and(|liquid| liquid.is_hidden()),
        ContentType::Unit => content
            .unit_by_name(name)
            .is_some_and(|unit| unit.is_hidden()),
        ContentType::Block => content
            .block_by_name(name)
            .is_some_and(|block| visibility.block_hidden(name, block.build_visibility)),
        _ => false,
    }
}

/// One command-row option (`UnitFactory.java:311-315`,
/// `Reconstructor.java:222-232`): the `Icon` glyph name, the `command.<name>`
/// bundle key and the checked state — `command == item` or, when no command is
/// configured, the unit's `defaultCommand`.
pub(super) fn command_options_json(
    content: &ContentRegistry,
    commands: &[mind_core::content::UnitCommandId],
    current: Option<&str>,
    default: Option<u16>,
) -> Vec<serde_json::Value> {
    commands
        .iter()
        .filter_map(|id| {
            let def = content.unit_command(*id)?;
            let name = def.name.clone();
            let checked =
                current == Some(name.as_str()) || (current.is_none() && default == Some(id.raw()));
            Some(serde_json::json!({
                "name": name,
                "icon": def.icon,
                "label": format!("command.{name}"),
                "checked": checked,
            }))
        })
        .collect()
}

/// `ItemSelection.buildTable` call sites in `world/**` — `(pick kind, rows,
/// columns)`. Sorter/InvertedSorter (`Sorter.java:147`), ItemSource
/// (`ItemSource.java:116`), LiquidSource (`LiquidSource.java:94`), Unloader
/// (`Unloader.java:286`), DirectionalUnloader (`DirectionalUnloader.java:146`),
/// DuctRouter (`DuctRouter.java:125`), LandingPad (`LandingPad.java:380`),
/// PayloadRouter (`PayloadRouter.java:184`), PayloadSource
/// (`PayloadSource.java:114`), Constructor/LargeConstructor
/// (`Constructor.java:68`), UnitCargoUnloadPoint
/// (`UnitCargoUnloadPoint.java:83`), UnitFactory (`UnitFactory.java:289`).
/// Rows/columns are `Block.selectionRows`/`selectionColumns` (`Block.java:273`,
/// `5`/`4`) except PayloadSource (`PayloadSource.java:35`, both `8`).
pub(super) fn item_selection_shape(
    def: &mind_core::content::BlockDef,
) -> Option<(&'static str, i64, i64)> {
    let (rows, columns) = if def.name == "payload-source" {
        (8, 8)
    } else {
        (5, 4)
    };
    // Keyed on the block kind: the routed families share kinds
    // (sorter/inverted-sorter, constructor/large-constructor, the payload
    // router pair) and every `UnitFactory` (ground/air/naval factory plus the
    // tank/ship/mech fabricators) lists plans.
    let pick = match def.kind {
        BlockKind::Sorter
        | BlockKind::ItemSource
        | BlockKind::Unloader
        | BlockKind::DirectionalUnloader
        | BlockKind::DuctRouter
        | BlockKind::LandingPad
        | BlockKind::UnitCargoUnloadPoint => "item",
        BlockKind::LiquidSource => "liquid",
        BlockKind::Constructor | BlockKind::PayloadRouter | BlockKind::PayloadSource => "block",
        BlockKind::UnitFactory => "unit",
        _ => return None,
    };
    Some((pick, rows, columns))
}

/// The checked option's content name from a `building_config_json` value
/// (`ItemSelection.java:75`: `holder.get() == item`). `item`/`liquid`/`block`/
/// `unit` carry a dense id; `content` carries its own content-type name
/// (unit commands).
pub(super) fn config_content_name(
    content: &ContentRegistry,
    value: &serde_json::Value,
) -> Option<String> {
    let id = u16::try_from(value.get("id")?.as_u64()?).ok()?;
    let content_type = match value.get("type")?.as_str()? {
        "item" => ContentType::Item,
        "liquid" => ContentType::Liquid,
        "block" => ContentType::Block,
        "unit" => ContentType::Unit,
        "content" => match value.get("content")?.as_str()? {
            "unitCommand" => ContentType::UnitCommand,
            "unitStance" => ContentType::UnitStance,
            _ => return None,
        },
        _ => return None,
    };
    content
        .entries(content_type)
        .into_iter()
        .find(|entry| entry.id == id)
        .and_then(|entry| entry.name)
        .map(str::to_owned)
}

/// Camera world coordinate for a viewport point (screen→tile center;
/// Java `Tile.worldx()` convention: the centre is `x * tilesize`).
pub(super) fn camera_world_corner(camera: &Gd<MindCamera2D>, x: f32, y: f32) -> (f32, f32) {
    let tile = camera.bind().screen_to_tile(x as f64, y as f64);
    let unit = mind_core::config::TILESIZE as f32;
    (tile.x as f32 * unit, tile.y as f32 * unit)
}

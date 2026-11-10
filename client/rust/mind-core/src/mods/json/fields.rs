// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Scalar field readers and the accepted-field const tables.
//!
//! Upstream: core/src/mindustry/mod/ContentParser.java

use super::*;

/// Reads a number field as `f32` (accepts both int and float JSON values).
pub(super) fn field_f32(object: &Map<String, Value>, key: &str) -> Option<f32> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .map(|value| value as f32)
}

/// Reads a number field as `i32`.
pub(super) fn field_i32(object: &Map<String, Value>, key: &str) -> Option<i32> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .map(|value| value as i32)
}

/// Reads a boolean field.
pub(super) fn field_bool(object: &Map<String, Value>, key: &str) -> Option<bool> {
    object.get(key).and_then(Value::as_bool)
}

/// `UnlockableContent` fields shared by every unlockable kind
/// (`UnlockableContent.java:34-79`). Read by [`ContentJsonParser::apply_unlock_fields`]
/// and accepted by every top-level kind's field list.
pub(super) const UNLOCK_FIELDS: &[&str] = &[
    "research",
    "details",
    "credit",
    "alwaysUnlocked",
    "inlineDescription",
    "hideDetails",
    "hideDatabase",
    "generateIcons",
    "selectionSize",
    "fullOverride",
    "allDatabaseTabs",
    "shownPlanets",
    "databaseTabs",
    "databaseCategory",
    "databaseTag",
];

/// Known JSON fields per kind (ledger-backed; unknown keys warn).
pub(super) const ITEM_FIELDS: &[&str] = &[
    "name",
    "description",
    "color",
    "hardness",
    "cost",
    "explosiveness",
    "flammability",
    "radioactivity",
    "charge",
    "healthScaling",
    "lowPriority",
    "frames",
    "transitionFrames",
    "frameTime",
    "buildable",
    "hidden",
];

pub(super) const LIQUID_FIELDS: &[&str] = &[
    "name",
    "description",
    "type",
    "color",
    "gas",
    "gasColor",
    "barColor",
    "lightColor",
    "flammability",
    "temperature",
    "heatCapacity",
    "viscosity",
    "explosiveness",
    "blockReactive",
    "coolant",
    "moveThroughBlocks",
    "incinerable",
    "effect",
    "particleEffect",
    "particleSpacing",
    "boilPoint",
    "capPuddles",
    "vaporEffect",
    "hidden",
    "canStayOn",
];

pub(super) const STATUS_FIELDS: &[&str] = &[
    "name",
    "description",
    "damageMultiplier",
    "healthMultiplier",
    "speedMultiplier",
    "reloadMultiplier",
    "buildSpeedMultiplier",
    "dragMultiplier",
    "transitionDamage",
    "disarm",
    "damage",
    "intervalDamageTime",
    "intervalDamage",
    "intervalDamagePierce",
    "effectChance",
    "parentizeEffect",
    "permanent",
    "reactive",
    "dynamic",
    "show",
    "color",
    "effect",
    "applyEffect",
    "applyExtend",
    "applyColor",
    "parentizeApplyEffect",
    "affinities",
    "opposites",
    "outline",
];

pub(super) const WEATHER_FIELDS: &[&str] = &[
    "name",
    "description",
    "type",
    "duration",
    "opacityMultiplier",
    "attrs",
    "sound",
    "soundVol",
    "soundVolMin",
    "soundVolOscMag",
    "soundVolOscScl",
    "hidden",
    "status",
    "statusDuration",
    "statusAir",
    "statusGround",
    "color",
    "yspeed",
    "xspeed",
    "padding",
    "sizeMin",
    "sizeMax",
    "density",
    "minAlpha",
    "maxAlpha",
    "force",
    "noiseScale",
    "stroke",
    "liquid",
];

pub(super) const SECTOR_FIELDS: &[&str] = &[
    "name",
    "description",
    "planet",
    "sector",
    "captureWave",
    "difficulty",
    "startWaveTimeMultiplier",
    "addStartingItems",
    "noLighting",
    "isLastSector",
    "requireUnlock",
    "showHidden",
    "attackAfterWaves",
    "showSectorLandInfo",
    "outline",
    "outlineRadius",
];

pub(super) const PLANET_FIELDS: &[&str] = &[
    "name",
    "description",
    "parent",
    "radius",
    "sectorSize",
    "mesh",
    "cloudMesh",
    "bloom",
    "accessible",
    "visible",
    "hasAtmosphere",
    "updateLighting",
    "lightColor",
    "atmosphereColor",
    "iconColor",
    "defaultEnv",
    "generator",
    "startSector",
    "allowLaunchSchematics",
    "allowLaunchLoadout",
    "allowSectorInvasion",
    "allowWaves",
    "drawOrbit",
];

pub(super) const TEAM_FIELDS: &[&str] = &["name", "description", "team"];

pub(super) const UNIT_FIELDS: &[&str] = &[
    "name",
    "description",
    "type",
    "template",
    "controller",
    "aiController",
    "health",
    "speed",
    "armor",
    "hitSize",
    "drag",
    "accel",
    "rotateSpeed",
    "range",
    "mineRange",
    "buildRange",
    "buildSpeed",
    "mineSpeed",
    "itemCapacity",
    "mineTier",
    "flying",
    "targetAir",
    "targetGround",
    "useUnitCap",
    "playerControllable",
    "logicControllable",
    "hidden",
    "isEnemy",
    "immunities",
    "weapons",
    "requirements",
    "abilities",
    "commands",
    "stances",
    "defaultCommand",
];

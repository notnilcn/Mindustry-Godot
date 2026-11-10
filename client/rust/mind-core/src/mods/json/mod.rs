// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! JSON content parser (plan 20 §3.4): the `ContentParser` equivalent.
//!
//! M1 covered the `item` and `block` top-level kinds with their core fields.
//! M2 extends the same module with `liquid`, `status`, `unit`, `weather`,
//! `sector`, `planet` and `team`, the committed [`ClassMap`](classmap)
//! replacement, nested bullet/weapon creation for units, and null-field
//! validation. `serde_json` is built with `preserve_order`, so object key order
//! is the parse/patch traversal order (invariant 9).
//!
//! Reflection-free field parsing: unknown fields warn and are ignored exactly
//! like upstream `ignoreUnknownFields = true`; the accepted vocabulary per kind
//! is tracked in `parity/ledgers/mod_fields.md`.

pub mod classmap;
pub mod classmap_gen;

mod block;
mod bullet;
mod consume;
mod content;
mod draw;
mod effect;
mod fields;
mod resolve;
#[cfg(test)]
mod tests;
mod unit;

use serde_json::{Map, Value};

use crate::content::UnlockStore;
use crate::content::bundle::MemoryBundle;
use crate::content::parser_hooks::ContentParseError;
use crate::content::registries::blocks::drawer::{DrawerKind, DrawerSpec};
use crate::content::registries::blocks::{
    AssemblerUnitPlanSpec, ConsumeDef, LiquidStackSpec, PayloadStackSpec, StackSpec, UnitPlanSpec,
    liquid_stack, stack,
};
use crate::content::registries::bullets::{BulletDef, BulletKind, BulletSpec};
use crate::content::registries::fx_meta::{
    EffectRef, EffectSpec, InlineEffectKind, effect_by_name,
};
use crate::content::registries::planets::{CloudMeshKind, GeneratorKind, MeshKind, PlanetDef};
use crate::content::registries::sectors::SectorPresetDef;
use crate::content::registries::sound_meta::SoundId;
use crate::content::registries::units::ability::{AbilityKind, AbilitySpec};
use crate::content::registries::units::parts::{
    BlendingKind, DrawPartKind, DrawPartSpec, InterpKind, PartMoveSpec, PartProgressSpec,
};
use crate::content::registries::units::weapon::{
    BulletRef, ShootPatternKind, ShootPatternSpec, WeaponDef, WeaponSpec,
};
use crate::content::registries::units::{
    AiControllerKind, ControllerKind, EntityDefSpec, ResolvedBullet, UnitKind, UnitSpec,
    UnitTypeDef,
};
use crate::content::registries::weathers::{Attribute, WeatherDef, WeatherKind};
use crate::content::settings_store::MemoryUnlockStore;
use crate::content::{
    BlockDef, BlockKind, BlockSpec, BuildVisibility, Category, ContentRef, ContentRegistry,
    ContentType, Item, Liquid, PlanetId, Rgba, StatusEffect, StatusId, TeamEntry, UnlockFields,
};

use classmap::{ClassScope, ClassTagMap};
use fields::*;

/// One parser warning (unknown field, ignored value, deferred feature).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModWarning {
    /// Source file or content name.
    pub file: String,
    /// Warning text.
    pub message: String,
}

/// `ContentParser` equivalent. One instance per parser scope (mod-content load
/// or restricted patch-content load).
pub struct ContentJsonParser<'a> {
    /// Bundle entries registered from JSON `name`/`description` keys.
    pub bundle: MemoryBundle,
    /// Unlock store the registry consults for `<name>-unlocked`
    /// (`Core.settings` in the client; the harness/tests pass a memory store).
    pub store: &'a dyn UnlockStore,
    /// Accumulated warnings (unknown fields, ignored values).
    pub warnings: Vec<ModWarning>,
    /// Whether type resolution through the ClassMap is allowed.
    pub allow_class_resolution: bool,
    /// Whether asset loading is allowed (restricted patch parser: false).
    pub allow_asset_loading: bool,
    /// Whether in-place patching of existing content is allowed.
    pub allow_patching: bool,
    /// Committed ClassMap replacement.
    classmap: ClassTagMap,
    /// Deferred `Floor.decoration` block references, resolved by
    /// [`Self::finish_parsing`] once every mod file is registered (Java defers
    /// all field reads to `ContentParser.finishParsing`,
    /// `ContentParser.java:1005-1007`, so a floor may name a later prop).
    pending_decorations: Vec<PendingDecoration>,
    /// Deferred `research` tech-node insertions (`ContentParser.readFields`
    /// removes the key and defers the tree reparenting to `postreads`;
    /// `ContentParser.java:1270,1337-1400`).
    pending_research: Vec<PendingResearch>,
}

/// One deferred `research` tech-node insertion.
#[derive(Debug, Clone)]
struct PendingResearch {
    file: String,
    mod_id: Option<crate::content::ModId>,
    content: ContentRef,
    value: Value,
}

/// One deferred `decoration` read (`ContentParser.read`, `ContentParser.java:963`).
#[derive(Debug, Clone)]
struct PendingDecoration {
    file: String,
    mod_id: Option<crate::content::ModId>,
    floor: crate::content::BlockId,
    name: String,
}

impl Default for ContentJsonParser<'static> {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-wide empty store for parsers that only need class defaults.
pub(crate) fn empty_store() -> &'static MemoryUnlockStore {
    static EMPTY: std::sync::OnceLock<MemoryUnlockStore> = std::sync::OnceLock::new();
    EMPTY.get_or_init(MemoryUnlockStore::new)
}

impl<'a> ContentJsonParser<'a> {
    /// Parser with default permissions (mod-content load) and no host unlocks.
    pub fn new() -> Self {
        Self::with_store(empty_store())
    }

    /// Parser bound to the host's unlock store (`Core.settings`).
    pub fn with_store(store: &'a dyn UnlockStore) -> Self {
        Self {
            bundle: MemoryBundle::new(),
            store,
            warnings: Vec::new(),
            allow_class_resolution: true,
            allow_asset_loading: true,
            allow_patching: true,
            classmap: ClassTagMap::committed(),
            pending_decorations: Vec::new(),
            pending_research: Vec::new(),
        }
    }

    /// `ContentParser.finishParsing` (`ContentParser.java:1005-1007`) for the
    /// deferred reads the port keeps: each `decoration` resolves as a block
    /// name, mod-prefixed first, then bare (`ContentParser.java:510-515`).
    /// Returns one error per unresolved name (Java marks the content errored).
    pub fn finish_parsing(&mut self, registry: &mut ContentRegistry) -> Vec<ContentParseError> {
        let previous = registry.current_mod().cloned();
        let mut errors = Vec::new();
        for pending in std::mem::take(&mut self.pending_decorations) {
            registry.set_current_mod(pending.mod_id.clone());
            let prefixed = registry.transform_name(&pending.name);
            let found = registry
                .get_by_name(ContentType::Block, &prefixed)
                .or_else(|| registry.get_by_name(ContentType::Block, &pending.name));
            match found {
                Some(reference) => {
                    if let Some(def) = registry.block_mut(pending.floor) {
                        def.decoration = crate::content::BlockId::new(reference.id);
                    }
                }
                None => errors.push(ContentParseError::new(format!(
                    "{}: \"decoration\": No block found with name '{}'.",
                    pending.file, pending.name
                ))),
            }
        }
        for pending in std::mem::take(&mut self.pending_research) {
            registry.set_current_mod(pending.mod_id.clone());
            if let Err(error) = self.apply_research(registry, pending) {
                errors.push(error);
            }
        }
        registry.set_current_mod(previous);
        errors
    }

    /// Applies one deferred `research` node (`ContentParser` `postreads`:
    /// remove the old node, build the replacement, attach to the parent or the
    /// root list, then rebind `content.techNode`).
    fn apply_research(
        &mut self,
        registry: &mut ContentRegistry,
        pending: PendingResearch,
    ) -> Result<(), ContentParseError> {
        use crate::content::tech::{ObjectiveSpec, TechNode, TechNodeRef};

        let file = pending.file.clone();
        let content = pending.content;
        let (
            parent_name,
            custom_requirements,
            root,
            root_name,
            requires_unlock,
            planet_name,
            objective_values,
        ) = match &pending.value {
            Value::String(name) => (Some(name.clone()), None, false, None, false, None, None),
            Value::Object(object) => (
                object
                    .get("parent")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                object.get("requirements").cloned(),
                field_bool(object, "root").unwrap_or(false),
                object
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                field_bool(object, "requiresUnlock").unwrap_or(false),
                object
                    .get("planet")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                object.get("objectives").cloned(),
            ),
            _ => {
                return Err(ContentParseError::new(format!(
                    "{file}: `research` must be a parent name string or an object"
                )));
            }
        };

        let requirements = match &custom_requirements {
            Some(value) => {
                let specs = self.parse_stacks(registry, &file, value)?;
                let mut out = Vec::with_capacity(specs.len());
                for spec in specs {
                    let item = registry.item_id(spec.item).ok_or_else(|| {
                        ContentParseError::new(format!(
                            "{file}: unknown research requirement item `{}`",
                            spec.item
                        ))
                    })?;
                    out.push(crate::content::ItemStack {
                        item,
                        amount: spec.amount,
                    });
                }
                out
            }
            None => crate::content::tech::content_research_requirements(registry, content),
        };

        // `TechNode` constructor: dependencies become `Research` objectives.
        let mut objectives: Vec<ObjectiveSpec> =
            crate::content::tech::content_dependencies(registry, content)
                .into_iter()
                .map(ObjectiveSpec::Research)
                .collect();
        if let Some(Value::Array(entries)) = objective_values {
            for entry in &entries {
                if let Some(objective) = self.parse_research_objective(registry, &file, entry)
                    && !objectives.contains(&objective)
                {
                    objectives.push(objective);
                }
            }
        }
        // All items/liquids get a `Produce` objective unless one was given.
        if matches!(content.type_, ContentType::Item | ContentType::Liquid)
            && !objectives
                .iter()
                .any(|objective| matches!(objective, ObjectiveSpec::Produce(c) if *c == content))
        {
            objectives.push(ObjectiveSpec::Produce(content));
        }

        // `TechNode.remove()`: detach the current node from its parent/roots
        // without shifting the positional `TechNodeRef` indices.
        let old = registry
            .tech
            .nodes
            .iter()
            .position(|node| node.content == Some(content))
            .map(|index| TechNodeRef(index as u32));
        if let Some(old) = old {
            let parent = registry.tech.nodes[old.index()].parent;
            if let Some(parent) = parent
                && let Some(record) = registry.tech.node_mut(parent)
            {
                record.children.retain(|child| *child != old);
            }
            registry.tech.roots.retain(|node| *node != old);
        }

        // Parent lookup: content name first (mod-prefixed then bare), then the
        // stored node name (`ContentParser.java:1392-1401`).
        let parent_ref = parent_name.as_ref().and_then(|name| {
            let prefixed = registry.transform_name(name);
            for candidate in [name.as_str(), prefixed.as_str()] {
                if let Some(reference) = registry.by_name(candidate)
                    && let Some(index) = registry
                        .tech
                        .nodes
                        .iter()
                        .position(|node| node.content == Some(reference))
                {
                    return Some(TechNodeRef(index as u32));
                }
            }
            registry
                .tech
                .nodes
                .iter()
                .position(|node| node.content_name == *name || node.content_name == prefixed)
                .map(|index| TechNodeRef(index as u32))
        });
        if !root && parent_name.is_some() && parent_ref.is_none() {
            self.warn(
                &file,
                format!(
                    "research parent `{}` is not in the tech tree",
                    parent_name.as_deref().unwrap_or("")
                ),
            );
        }

        let planet = planet_name
            .as_ref()
            .and_then(|name| {
                let prefixed = registry.transform_name(name);
                [name.as_str(), prefixed.as_str()]
                    .iter()
                    .find_map(|candidate| {
                        registry.by_name(candidate).and_then(|reference| {
                            (reference.type_ == ContentType::Planet)
                                .then_some(crate::content::PlanetId::new(reference.id))
                        })
                    })
            })
            .or_else(|| {
                parent_ref
                    .and_then(|parent| registry.tech.node(parent))
                    .and_then(|node| node.planet)
            });

        let index = match old {
            Some(old) => old.index(),
            None => {
                registry.tech.nodes.push(TechNode {
                    depth: 0,
                    name: None,
                    requires_unlock: false,
                    parent: None,
                    root_node: None,
                    research_cost_multipliers: Vec::new(),
                    content: None,
                    content_name: String::new(),
                    requirements: Vec::new(),
                    finished_requirements: Vec::new(),
                    objectives: Vec::new(),
                    children: Vec::new(),
                    planet: None,
                    tree: None,
                });
                registry.tech.nodes.len() - 1
            }
        };
        let reference = TechNodeRef(index as u32);
        let content_name =
            crate::content::tech::content_name_of(registry, content).unwrap_or_default();
        let (depth, root_node, multipliers, tree) = match parent_ref {
            Some(parent) => {
                let record = &registry.tech.nodes[parent.index()];
                (
                    record.depth + 1,
                    record.root_node.or(Some(parent)),
                    record.research_cost_multipliers.clone(),
                    record.tree,
                )
            }
            None => (0, None, Vec::new(), None),
        };
        {
            let node = &mut registry.tech.nodes[index];
            node.depth = depth;
            node.parent = parent_ref;
            node.root_node = root_node;
            node.research_cost_multipliers = multipliers;
            node.content = Some(content);
            node.content_name = content_name.clone();
            node.finished_requirements = requirements
                .iter()
                .map(|stack| crate::content::ItemStack {
                    item: stack.item,
                    amount: 0,
                })
                .collect();
            node.requirements = requirements;
            node.objectives = objectives;
            node.planet = planet;
            node.tree = tree;
            if root {
                node.name = Some(root_name.clone().unwrap_or_else(|| content_name.clone()));
                node.requires_unlock = requires_unlock;
            }
        }
        if let Some(parent) = parent_ref
            && let Some(record) = registry.tech.node_mut(parent)
            && !record.children.contains(&reference)
        {
            record.children.push(reference);
        }
        if root {
            registry.tech.roots.push(reference);
        }
        registry.set_tech_node(content, reference);
        Ok(())
    }

    /// `Objectives.Objective` JSON (`ContentParser.java:348-363`): a string is
    /// a content name (`new Research(content)`); an object dispatches on
    /// `type` (`SectorComplete`/`OnSector`/`OnPlanet`/`Research`/`Produce`).
    fn parse_research_objective(
        &mut self,
        registry: &ContentRegistry,
        file: &str,
        value: &Value,
    ) -> Option<crate::content::tech::ObjectiveSpec> {
        use crate::content::tech::ObjectiveSpec;
        let resolve_any = |raw: &str| {
            let prefixed = registry.transform_name(raw);
            registry
                .by_name(raw)
                .or_else(|| registry.by_name(&prefixed))
        };
        match value {
            Value::String(raw) => match resolve_any(raw) {
                Some(reference) => Some(ObjectiveSpec::Research(reference)),
                None => {
                    self.warn(file, format!("unknown research objective content `{raw}`"));
                    None
                }
            },
            Value::Object(object) => {
                let class = object.get("type").and_then(Value::as_str).unwrap_or("");
                let field = |key: &str| object.get(key).and_then(Value::as_str);
                match class {
                    "SectorComplete" | "OnSector" => {
                        let raw = field("preset").or_else(|| field("sector"))?;
                        let reference = resolve_any(raw)?;
                        if reference.type_ != ContentType::Sector {
                            self.warn(file, format!("`{raw}` is not a sector"));
                            return None;
                        }
                        let sector = crate::content::SectorId::new(reference.id);
                        Some(if class == "OnSector" {
                            ObjectiveSpec::OnSector(sector)
                        } else {
                            ObjectiveSpec::SectorComplete(sector)
                        })
                    }
                    "OnPlanet" => {
                        let raw = field("planet")?;
                        let reference = resolve_any(raw)?;
                        if reference.type_ != ContentType::Planet {
                            self.warn(file, format!("`{raw}` is not a planet"));
                            return None;
                        }
                        Some(ObjectiveSpec::OnPlanet(crate::content::PlanetId::new(
                            reference.id,
                        )))
                    }
                    "Research" | "Produce" => {
                        let raw = field("content")
                            .or_else(|| field("block"))
                            .or_else(|| field("unit"))
                            .or_else(|| field("item"))?;
                        let reference = resolve_any(raw)?;
                        Some(if class == "Research" {
                            ObjectiveSpec::Research(reference)
                        } else {
                            ObjectiveSpec::Produce(reference)
                        })
                    }
                    other => {
                        self.warn(
                            file,
                            format!("unknown research objective `{other}`; ignored"),
                        );
                        None
                    }
                }
            }
            _ => None,
        }
    }

    /// Parser restricted for data-patch content (`dp` identity).
    pub fn restricted() -> Self {
        Self {
            allow_class_resolution: false,
            allow_asset_loading: false,
            allow_patching: false,
            ..Self::new()
        }
    }

    /// The committed ClassMap replacement.
    pub fn classmap(&self) -> &ClassTagMap {
        &self.classmap
    }

    /// Parses one content file. `stem` is the file stem (content name before
    /// prefixing).
    pub fn parse(
        &mut self,
        registry: &mut ContentRegistry,
        file: &str,
        stem: &str,
        json: &str,
        content_type: ContentType,
    ) -> Result<ContentRef, ContentParseError> {
        let value: Value = serde_json::from_str(json)
            .map_err(|error| ContentParseError::new(format!("{file}: invalid JSON: {error}")))?;
        let object = value.as_object().ok_or_else(|| {
            ContentParseError::new(format!("{file}: content must be a JSON object"))
        })?;
        self.check_null_fields(file, content_type, object)?;
        let reference = match content_type {
            ContentType::Item => self.parse_item(registry, file, stem, object)?,
            ContentType::Block => self.parse_block(registry, file, stem, object)?,
            ContentType::Liquid => self.parse_liquid(registry, file, stem, object)?,
            ContentType::Status => self.parse_status(registry, file, stem, object)?,
            ContentType::Unit => self.parse_unit(registry, file, stem, object)?,
            ContentType::Weather => self.parse_weather(registry, file, stem, object)?,
            ContentType::Sector => self.parse_sector(registry, file, stem, object)?,
            ContentType::Planet => self.parse_planet(registry, file, stem, object)?,
            ContentType::Team => self.parse_team(registry, file, stem, object)?,
            other => {
                return Err(ContentParseError::new(format!(
                    "{file}: content type `{other}` is not supported yet (plan 20 M2)"
                )));
            }
        };
        // `UnlockableContent` `research` key (`ContentParser.readFields`
        // removes it before the field loop and defers the node insertion to
        // `postreads`, `ContentParser.java:1337-1400`).
        if let Some(research) = object.get("research") {
            self.pending_research.push(PendingResearch {
                file: file.to_owned(),
                mod_id: registry.current_mod().cloned(),
                content: reference,
                value: research.clone(),
            });
        }
        Ok(reference)
    }

    /// `checkNullFields`: refuses explicit `null` on non-nullable fields.
    ///
    /// Upstream reflection inspects declared field types; the port keeps a
    /// per-kind deny list of fields that are never nullable. Region fields
    /// (`TextureRegion`-equivalent) are implicitly nullable and skipped.
    fn check_null_fields(
        &self,
        file: &str,
        content_type: ContentType,
        object: &Map<String, Value>,
    ) -> Result<(), ContentParseError> {
        let non_nullable: &[&str] = match content_type {
            ContentType::Item => &["name"],
            ContentType::Block => &["name"],
            ContentType::Liquid => &["name"],
            ContentType::Status => &["name"],
            ContentType::Unit => &["name"],
            ContentType::Weather => &["name"],
            ContentType::Sector => &["name"],
            ContentType::Planet => &["name"],
            ContentType::Team => &["name"],
            _ => &[],
        };
        for field in non_nullable {
            if matches!(object.get(*field), Some(Value::Null)) {
                return Err(ContentParseError::new(format!(
                    "{file}: Field '{field}' cannot be null."
                )));
            }
        }
        Ok(())
    }

    /// `readBundle`: registers `<type>.<name>.name|description`.
    fn read_bundle(&mut self, object: &Map<String, Value>, content_type: ContentType, name: &str) {
        let key = |suffix: &str| format!("{}.{}.{}", content_type.name(), name, suffix);
        if let Some(text) = object.get("name").and_then(Value::as_str) {
            self.bundle.insert(key("name"), text.to_owned());
        }
        if let Some(text) = object.get("description").and_then(Value::as_str) {
            self.bundle.insert(key("description"), text.to_owned());
        }
    }

    /// Applies the `UnlockableContent` JSON fields (`UnlockableContent.java:34-79`)
    /// to a freshly constructed record.
    ///
    /// Upstream reads these through reflection like any other public field:
    /// `alwaysUnlocked`, `inlineDescription`, `hideDetails`, `hideDatabase`,
    /// `generateIcons`, `selectionSize`, `fullOverride`, `allDatabaseTabs`,
    /// `shownPlanets`, `databaseTabs`, `databaseCategory`, `databaseTag`, plus
    /// the standalone `details`/`credit` strings. `shownPlanets` entries resolve
    /// as planet names; `databaseTabs` entries may name any unlockable kind
    /// (`ObjectSet<UnlockableContent>` upstream).
    fn apply_unlock_fields(
        &mut self,
        registry: &ContentRegistry,
        file: &str,
        object: &Map<String, Value>,
        unlock: &mut UnlockFields,
    ) {
        for (field, target) in [
            ("alwaysUnlocked", &mut unlock.always_unlocked),
            ("inlineDescription", &mut unlock.inline_description),
            ("hideDetails", &mut unlock.hide_details),
            ("hideDatabase", &mut unlock.hide_database),
            ("generateIcons", &mut unlock.generate_icons),
            ("allDatabaseTabs", &mut unlock.all_database_tabs),
        ] {
            if let Some(value) = field_bool(object, field) {
                *target = value;
            }
        }
        if let Some(value) = field_f32(object, "selectionSize") {
            unlock.selection_size = value;
        }
        if let Some(value) = object.get("fullOverride").and_then(Value::as_str) {
            unlock.full_override = value.to_owned();
        }
        if let Some(value) = object.get("details").and_then(Value::as_str) {
            unlock.details = Some(value.to_owned());
        }
        if let Some(value) = object.get("credit").and_then(Value::as_str) {
            unlock.credit = Some(value.to_owned());
        }
        if let Some(value) = object.get("databaseCategory").and_then(Value::as_str) {
            unlock.database_category = Some(value.to_owned());
        }
        if let Some(value) = object.get("databaseTag").and_then(Value::as_str) {
            unlock.database_tag = Some(value.to_owned());
        }
        if let Some(Value::Array(entries)) = object.get("shownPlanets") {
            for entry in entries {
                let Some(raw) = entry.as_str() else {
                    continue;
                };
                match self.resolve_planet(registry, raw) {
                    Some(planet) => unlock.shown_planets.push(planet),
                    None => self.warn(
                        file,
                        format!("`shownPlanets`: unknown planet `{raw}`; ignored"),
                    ),
                }
            }
        }
        if let Some(Value::Array(entries)) = object.get("databaseTabs") {
            for entry in entries {
                let Some(raw) = entry.as_str() else {
                    continue;
                };
                match self.resolve_any_content(registry, raw) {
                    Some(reference) => unlock.database_tabs.push(reference),
                    None => self.warn(
                        file,
                        format!("`databaseTabs`: unknown content `{raw}`; ignored"),
                    ),
                }
            }
        }
    }

    /// Resolves a name against every unlockable content kind
    /// (`ObjectSet<UnlockableContent>` field reads upstream).
    fn resolve_any_content(&self, registry: &ContentRegistry, raw: &str) -> Option<ContentRef> {
        for content_type in [
            ContentType::Item,
            ContentType::Block,
            ContentType::Liquid,
            ContentType::Status,
            ContentType::Unit,
            ContentType::Weather,
            ContentType::Sector,
            ContentType::Planet,
            ContentType::Team,
        ] {
            if let Some(reference) = self.resolve_prefixed(registry, content_type, raw) {
                return Some(reference);
            }
        }
        None
    }

    /// `locate`: returns the (prefixed) content name. Typeless files matching
    /// vanilla content are patched in place by M3; M2 surfaces a warning and
    /// creates a prefixed copy so names never collide.
    fn locate_name(
        &mut self,
        registry: &ContentRegistry,
        content_type: ContentType,
        stem: &str,
        object: &Map<String, Value>,
    ) -> String {
        let has_type = object.get("type").is_some();
        if registry.get_by_name(content_type, stem).is_some() {
            self.warn(
                stem,
                if has_type {
                    format!("content `{stem}` already exists; creating prefixed content")
                } else {
                    format!("content `{stem}` already exists; in-place patching lands in M3")
                },
            );
        }
        registry.transform_name(stem)
    }

    /// Resolves a `<mod>-`-prefixed (or raw) mappable reference by type.
    fn resolve_prefixed(
        &self,
        registry: &ContentRegistry,
        content_type: ContentType,
        raw: &str,
    ) -> Option<ContentRef> {
        if let Some(reference) = registry.get_by_name(content_type, raw) {
            return Some(reference);
        }
        registry.get_by_name(content_type, &registry.transform_name(raw))
    }
}

pub use resolve::*;

// SPDX-License-Identifier: GPL-3.0-only

//! Content registry and lifecycle sweeps.
//!
//! Ported from `core/src/mindustry/core/ContentLoader.java` (ID assignment,
//! name registration, lifecycle phases, `copy`/`remove`/`removeLast`,
//! `logContent`) with the Rust borrow strategy of plan 02 §3.4: per-content
//! `init_self()` sweeps followed by an explicit cross-content `link()` pass.
//!
//! Hot-path rule: content access never mutates the registry and never allocates
//! (HIGH_LEVEL_PLAN §2.4); the allocating helpers here (`entries`, snapshots)
//! are harness/audit-only.

use std::collections::BTreeMap;

use super::ctype::{Content, ErrorContent, Mappable, ModId};
use super::id::{BlockId, BulletId, ItemId, LiquidId, PlanetId, StatusId, TeamEntryId, UnitTypeId};
use super::names::{self, NameMaps};
use super::parser_hooks::{ContentErrors, ModContentProvider, ModErrorSink};
use super::registries::{
    blocks::BlockDef, bullets::BulletDef, commands::UnitCommandDef, items::Item, liquids::Liquid,
    loadouts::LoadoutDef, planets::PlanetDef, sectors::SectorPresetDef, stances::UnitStanceDef,
    statuses::StatusEffect, teams::TeamEntry, units::UnitTypeDef, weathers::WeatherDef,
};
use super::snapshot::RegistryIndexSnapshot;
use super::tech::{TechNodeRef, TechStore, TreeId};
use super::{ContentError, ContentRef, ContentType};
use crate::assets::atlas::AtlasIndex;
use crate::assets::content_regions::audit_block_regions;
use crate::assets::regions::RegionAudit;

/// Lifecycle phases mirrored from `ContentLoader` (`init`, `postInit`,
/// `loadIcon`, `load`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecyclePhase {
    /// `Content.init()` — self-derived state, then the registry `link()` pass.
    Init,
    /// `Content.postInit()`.
    PostInit,
    /// `Content.loadIcon()` — client only (`headless` skips it).
    LoadIcon,
    /// `Content.load()` — client only (`headless` skips it).
    Load,
}

/// Bitset preventing double-run lifecycle phases (`ContentLoader.initialization`).
#[derive(Debug, Clone, Copy, Default)]
pub struct LifecyclePhases {
    /// `init()` already ran.
    pub init: bool,
    /// `post_init()` already ran.
    pub post_init: bool,
    /// `load_icon()` already ran.
    pub load_icon: bool,
    /// `load()` already ran.
    pub load: bool,
}

/// One audit entry for a content record (ordered name list / dump output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentEntry<'a> {
    /// Dense id.
    pub id: u16,
    /// Mappable name (`None` for non-mappable kinds like bullets).
    pub name: Option<&'a str>,
    /// Java class-ish kind tag.
    pub kind: &'static str,
}

/// Result of applying the [`TemporaryMapper`] to an id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappedId {
    /// No mapping defined; use the raw id into the current registry.
    Unmapped,
    /// Unknown/removed content; upstream returns ID 0 (the default value).
    Default0,
    /// Invalid content (`id < 0`); upstream returns `null`.
    Invalid,
    /// Mapped to a specific raw id.
    Id(u16),
}

/// Save-loading ID remap table (`ContentLoader.temporaryMapper`).
///
/// Plan 04 builds this from the save's content header; the registry only applies
/// it in [`ContentRegistry::get_by_id`] (unknown/removed → id 0, invalid → `None`).
#[derive(Debug, Clone, Default)]
pub struct TemporaryMapper {
    mappings: [Vec<Option<u16>>; ContentType::ALL.len()],
}

impl TemporaryMapper {
    /// Empty mapper.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether any type has a mapping installed (upstream checks `length != 0`).
    pub fn is_installed_for(&self, type_: ContentType) -> bool {
        !self.mappings[type_.ordinal()].is_empty()
    }

    /// Sets the mapping for `id` of `type_` (`None` = unknown/removed → id 0).
    pub fn set(&mut self, type_: ContentType, id: u16, mapped: Option<u16>) {
        let slot = &mut self.mappings[type_.ordinal()];
        let index = id as usize;
        if slot.len() <= index {
            slot.resize(index + 1, None);
        }
        slot[index] = mapped;
    }

    /// Applies the mapper to `id` (`ContentLoader.getByID` semantics).
    pub fn map(&self, type_: ContentType, id: i32) -> MappedId {
        if !self.is_installed_for(type_) {
            return MappedId::Unmapped;
        }
        if id < 0 {
            return MappedId::Invalid;
        }
        match self.mappings[type_.ordinal()].get(id as usize) {
            None | Some(None) => MappedId::Default0,
            Some(Some(raw)) => MappedId::Id(*raw),
        }
    }
}

/// The content registry (`Vars.content`).
///
/// Owns every content record. Cross-content references are typed IDs; names are
/// never serialized for mod content (HIGH_LEVEL_PLAN §6.2).
pub struct ContentRegistry {
    pub(crate) items: Vec<Item>,
    pub(crate) blocks: Vec<BlockDef>,
    pub(crate) bullets: Vec<BulletDef>,
    pub(crate) liquids: Vec<Liquid>,
    pub(crate) statuses: Vec<StatusEffect>,
    pub(crate) units: Vec<UnitTypeDef>,
    pub(crate) unit_commands: Vec<UnitCommandDef>,
    pub(crate) unit_stances: Vec<UnitStanceDef>,
    pub(crate) weathers: Vec<WeatherDef>,
    pub(crate) sectors: Vec<SectorPresetDef>,
    pub(crate) planets: Vec<PlanetDef>,
    pub(crate) teams: Vec<TeamEntry>,
    pub(crate) errors: Vec<ErrorContent>,
    pub(crate) names: NameMaps,
    pub(crate) last_added: Option<ContentRef>,
    pub(crate) current_mod: Option<ModId>,
    pub(crate) phases: LifecyclePhases,
    pub(crate) arr_epoch: u32,
    pub(crate) headless: bool,
    pub(crate) mod_error_sink: Option<Box<dyn ModErrorSink>>,
    pub(crate) tech: TechStore,
    tech_reports: Vec<(String, super::tech::TechTreeBuildReport)>,
    loadouts: Vec<LoadoutDef>,
    items_serpulo: Vec<ItemId>,
    items_erekir: Vec<ItemId>,
    items_erekir_only: Vec<ItemId>,
    temporary_mapper: Option<TemporaryMapper>,
    region_audit: Option<RegionAudit>,
}

impl ContentRegistry {
    /// Empty registry. `headless` skips `loadIcon()`/`load()` (plan 00 flag).
    pub fn new(headless: bool) -> Self {
        Self {
            items: Vec::new(),
            blocks: Vec::new(),
            bullets: Vec::new(),
            liquids: Vec::new(),
            statuses: Vec::new(),
            units: Vec::new(),
            unit_commands: Vec::new(),
            unit_stances: Vec::new(),
            weathers: Vec::new(),
            sectors: Vec::new(),
            planets: Vec::new(),
            teams: Vec::new(),
            errors: Vec::new(),
            names: NameMaps::new(),
            last_added: None,
            current_mod: None,
            phases: LifecyclePhases::default(),
            arr_epoch: 0,
            headless,
            mod_error_sink: None,
            tech: TechStore::default(),
            tech_reports: Vec::new(),
            loadouts: Vec::new(),
            items_serpulo: Vec::new(),
            items_erekir: Vec::new(),
            items_erekir_only: Vec::new(),
            temporary_mapper: None,
            region_audit: None,
        }
    }

    /// Installs the per-content error sink (plan 20 `Mods.handleContentError`).
    pub fn set_mod_error_sink(&mut self, sink: Option<Box<dyn ModErrorSink>>) {
        self.mod_error_sink = sink;
    }

    /// `ContentLoader.setCurrentMod`.
    pub fn set_current_mod(&mut self, mod_id: Option<ModId>) {
        self.current_mod = mod_id;
    }

    /// Current mod (`ContentLoader.currentMod`).
    pub fn current_mod(&self) -> Option<&ModId> {
        self.current_mod.as_ref()
    }

    /// `ContentLoader.transformName` for this registry's current mod.
    pub fn transform_name(&self, name: &str) -> String {
        names::transform_name(self.current_mod.as_ref(), name)
    }

    /// `ContentLoader.byName` — global (last registration wins).
    pub fn by_name(&self, name: &str) -> Option<ContentRef> {
        self.names.get_global(name)
    }

    /// `ContentLoader.getByName` with the legacy block-name fallback table.
    pub fn get_by_name(&self, type_: ContentType, name: &str) -> Option<ContentRef> {
        let resolved = if type_ == ContentType::Block {
            names::mod_content_name_map(name).unwrap_or(name)
        } else {
            name
        };
        self.names
            .get(type_, resolved)
            .map(|id| ContentRef::new(type_, id))
    }

    /// `ContentLoader.getByID` honoring the temporary mapper.
    pub fn get_by_id(&self, type_: ContentType, id: i32) -> Option<ContentRef> {
        if let Some(mapper) = &self.temporary_mapper {
            match mapper.map(type_, id) {
                MappedId::Invalid => return None,
                MappedId::Default0 => {
                    return (self.type_len(type_) > 0).then(|| ContentRef::new(type_, 0));
                }
                MappedId::Id(raw) => return Some(ContentRef::new(type_, raw)),
                MappedId::Unmapped => {}
            }
        }
        if id < 0 {
            return None;
        }
        let raw = u16::try_from(id).ok()?;
        ((raw as usize) < self.type_len(type_)).then(|| ContentRef::new(type_, raw))
    }

    /// Installs a save temporary mapper (plan 04).
    pub fn set_temporary_mapper(&mut self, mapper: Option<TemporaryMapper>) {
        self.temporary_mapper = mapper;
    }

    /// Number of records of `type_` (0 for types without a vector in this build).
    pub fn type_len(&self, type_: ContentType) -> usize {
        match type_ {
            ContentType::Item => self.items.len(),
            ContentType::Block => self.blocks.len(),
            ContentType::Bullet => self.bullets.len(),
            ContentType::Liquid => self.liquids.len(),
            ContentType::Status => self.statuses.len(),
            ContentType::Unit => self.units.len(),
            ContentType::UnitCommand => self.unit_commands.len(),
            ContentType::UnitStance => self.unit_stances.len(),
            ContentType::Weather => self.weathers.len(),
            ContentType::Sector => self.sectors.len(),
            ContentType::Planet => self.planets.len(),
            ContentType::Team => self.teams.len(),
            ContentType::Error => self.errors.len(),
            _ => 0,
        }
    }

    /// Ordered audit entries for one content type.
    pub fn entries(&self, type_: ContentType) -> Vec<ContentEntry<'_>> {
        let mut out = Vec::new();
        macro_rules! push_mappable {
            ($field:ident) => {
                for record in &self.$field {
                    out.push(ContentEntry {
                        id: record.id.raw(),
                        name: Some(&record.name),
                        kind: record.kind_name(),
                    });
                }
            };
        }
        match type_ {
            ContentType::Item => push_mappable!(items),
            ContentType::Block => push_mappable!(blocks),
            ContentType::Liquid => push_mappable!(liquids),
            ContentType::Status => push_mappable!(statuses),
            ContentType::Unit => push_mappable!(units),
            ContentType::UnitCommand => push_mappable!(unit_commands),
            ContentType::UnitStance => push_mappable!(unit_stances),
            ContentType::Weather => push_mappable!(weathers),
            ContentType::Sector => push_mappable!(sectors),
            ContentType::Planet => push_mappable!(planets),
            ContentType::Team => push_mappable!(teams),
            ContentType::Bullet => {
                for record in &self.bullets {
                    out.push(ContentEntry {
                        id: record.id.raw(),
                        name: None,
                        kind: record.kind_name(),
                    });
                }
            }
            ContentType::Error => {
                for record in &self.errors {
                    out.push(ContentEntry {
                        id: record.id,
                        name: None,
                        kind: record.kind_name(),
                    });
                }
            }
            _ => {}
        }
        out
    }

    /// `ContentLoader.init()`: `init` sweep then the `link()` pass, once.
    pub fn init(&mut self) -> Result<(), ContentError> {
        if self.phases.init {
            return Ok(());
        }
        self.sweep(LifecyclePhase::Init)?;
        super::registries::bullets::link(self)?;
        super::registries::statuses::link(self)?;
        super::registries::stances::link(self)?;
        super::registries::sectors::link(self)?;
        super::registries::units::link(self)?;
        self.phases.init = true;
        Ok(())
    }

    /// `ContentLoader` `postInit` sweep, once.
    pub fn post_init(&mut self) -> Result<(), ContentError> {
        if self.phases.post_init {
            return Ok(());
        }
        self.sweep(LifecyclePhase::PostInit)?;
        super::registries::blocks::post_init_link(self)?;
        self.phases.post_init = true;
        Ok(())
    }

    /// `ContentLoader.load()`: `loadIcon` + `load` sweeps; skipped headless.
    pub fn load(&mut self) -> Result<(), ContentError> {
        if self.headless {
            return Ok(());
        }
        if !self.phases.load_icon {
            self.sweep(LifecyclePhase::LoadIcon)?;
            self.phases.load_icon = true;
        }
        if !self.phases.load {
            self.sweep(LifecyclePhase::Load)?;
            self.phases.load = true;
        }
        Ok(())
    }

    /// Plan 03 §3.2 stage 8a / M6: resolves every block's `@Load` region fields
    /// against `atlas` and records misses in a [`RegionAudit`].
    ///
    /// This is the `LoadRegions` half of `ContentLoader.load()`: the client boot
    /// calls it after [`init`](Self::init)/[`post_init`](Self::post_init) and
    /// before [`load`](Self::load) to populate render slots (plan 07); headless
    /// tests call it directly because [`load`](Self::load) is skipped there. The
    /// audit is cached and returned; a `fallback=error` miss is fatal for the
    /// `mind-headless assets regions --assert-complete` oracle.
    pub fn load_regions(&mut self, atlas: &AtlasIndex) -> RegionAudit {
        let audit = audit_block_regions(&self.blocks, atlas);
        self.region_audit = Some(audit.clone());
        audit
    }

    /// The most recent [`load_regions`](Self::load_regions) result, if any.
    pub fn region_audit(&self) -> Option<&RegionAudit> {
        self.region_audit.as_ref()
    }

    /// Client boot stage 8 fusion: resolve `@Load` regions (plan 03 M6) and then
    /// run the `loadIcon`/`load` sweeps. [`load`](Self::load) is a headless
    /// no-op, so headless callers use [`load_regions`](Self::load_regions)
    /// directly.
    pub fn load_with_regions(&mut self, atlas: &AtlasIndex) -> Result<(), ContentError> {
        self.load_regions(atlas);
        self.load()
    }

    /// `ContentLoader.afterPatch()` sweep (plan 20 patches).
    pub fn after_patch(&mut self) -> Result<(), ContentError> {
        for record in self.items.iter_mut() {
            record.after_patch()?;
        }
        for record in self.blocks.iter_mut() {
            record.after_patch()?;
        }
        for record in self.bullets.iter_mut() {
            record.after_patch()?;
        }
        for record in self.liquids.iter_mut() {
            record.after_patch()?;
        }
        for record in self.statuses.iter_mut() {
            record.after_patch()?;
        }
        for record in self.units.iter_mut() {
            record.after_patch()?;
        }
        Ok(())
    }

    /// `ContentLoader.logContent()`: dense-ID validation + per-type counts.
    pub fn log_content(&self) -> Result<(), ContentError> {
        for type_ in ContentType::ALL {
            for (index, entry) in self.entries(type_).iter().enumerate() {
                if entry.id as usize != index {
                    let name = match entry.name {
                        Some(name) => name.to_owned(),
                        None => format!("{}#{}", type_.name(), entry.id),
                    };
                    return Err(ContentError::OutOfOrderIds {
                        name,
                        expected: index,
                        got: entry.id,
                    });
                }
            }
            log::debug!("[{}]: loaded {}", type_.name(), self.type_len(type_));
        }
        let total: usize = ContentType::ALL
            .iter()
            .map(|type_| self.type_len(*type_))
            .sum();
        log::debug!("Total content loaded: {total}");
        Ok(())
    }

    /// `ContentLoader.copy()` index equivalent.
    pub fn snapshot_index(&self) -> RegistryIndexSnapshot {
        let mut lengths = [0usize; ContentType::ALL.len()];
        for type_ in ContentType::ALL {
            lengths[type_.ordinal()] = self.type_len(type_);
        }
        RegistryIndexSnapshot::new(
            lengths,
            self.names.clone(),
            self.current_mod.clone(),
            self.temporary_mapper.clone(),
        )
    }

    /// Restores registry membership from a snapshot; payload rollback is plan
    /// 20's `ResetAction` closures (plan 02 §3.4).
    pub fn restore_index(&mut self, snapshot: RegistryIndexSnapshot) {
        for type_ in ContentType::ALL {
            let len = snapshot.type_len(type_);
            match type_ {
                ContentType::Item => self.items.truncate(len),
                ContentType::Block => self.blocks.truncate(len),
                ContentType::Bullet => self.bullets.truncate(len),
                ContentType::Liquid => self.liquids.truncate(len),
                ContentType::Status => self.statuses.truncate(len),
                ContentType::Unit => self.units.truncate(len),
                ContentType::UnitCommand => self.unit_commands.truncate(len),
                ContentType::UnitStance => self.unit_stances.truncate(len),
                ContentType::Weather => self.weathers.truncate(len),
                ContentType::Sector => self.sectors.truncate(len),
                ContentType::Planet => self.planets.truncate(len),
                ContentType::Team => self.teams.truncate(len),
                ContentType::Error => self.errors.truncate(len),
                _ => {}
            }
        }
        self.names = snapshot.names().clone();
        self.current_mod = snapshot.current_mod().cloned();
        self.temporary_mapper = snapshot.temporary_mapper().cloned();
        self.last_added = None;
        self.region_audit = None;
        self.arr_epoch = self.arr_epoch.wrapping_add(1);
    }

    /// `ContentLoader.remove`: removes a record and its names, then runs
    /// `removeContent()`.
    ///
    /// Rust note (plan 02 M6 deviation): like upstream, this shifts later
    /// records down without rewriting their stored ids, so the dense-ID
    /// invariant is only guaranteed again after a full re-creation
    /// (`create_base_content`) or `restore_index`. Remove is therefore a
    /// mod-unload/error path, not a live-patch operation.
    pub fn remove(&mut self, content: ContentRef) {
        match content.type_ {
            ContentType::Item => remove_mappable(&mut self.items, &mut self.names, content.id),
            ContentType::Block => remove_mappable(&mut self.blocks, &mut self.names, content.id),
            ContentType::Liquid => remove_mappable(&mut self.liquids, &mut self.names, content.id),
            ContentType::Status => remove_mappable(&mut self.statuses, &mut self.names, content.id),
            ContentType::Unit => remove_mappable(&mut self.units, &mut self.names, content.id),
            ContentType::UnitCommand => {
                remove_mappable(&mut self.unit_commands, &mut self.names, content.id)
            }
            ContentType::UnitStance => {
                remove_mappable(&mut self.unit_stances, &mut self.names, content.id)
            }
            ContentType::Weather => {
                remove_mappable(&mut self.weathers, &mut self.names, content.id)
            }
            ContentType::Sector => remove_mappable(&mut self.sectors, &mut self.names, content.id),
            ContentType::Planet => remove_mappable(&mut self.planets, &mut self.names, content.id),
            ContentType::Team => remove_mappable(&mut self.teams, &mut self.names, content.id),
            ContentType::Bullet => {
                if let Some(mut record) = take_record(&mut self.bullets, content.id) {
                    record.remove_content();
                }
            }
            ContentType::Error => {
                if let Some(mut record) = take_record(&mut self.errors, content.id) {
                    record.remove_content();
                }
            }
            _ => {}
        }
        self.last_added = None;
        self.arr_epoch = self.arr_epoch.wrapping_add(1);
    }

    /// `ContentLoader.removeLast`: removes the last added record only when it is
    /// still the last element of its type list (`peek() == lastAdded`).
    pub fn remove_last(&mut self) {
        let Some(last) = self.last_added else {
            return;
        };
        let len = self.type_len(last.type_);
        if last.id as usize + 1 == len {
            self.remove(last);
        }
    }

    /// `mods.loadContent()` equivalent (plan 20 drives the provider): loads mod
    /// content into the registry, then materializes item stances for new items
    /// (`UnitStances.loadAfterMods`). Per-content failures inside the provider are
    /// its own concern; a bulk failure is returned to the caller.
    pub fn create_mod_content(
        &mut self,
        provider: &mut dyn ModContentProvider,
    ) -> Result<(), ContentErrors> {
        let mut errors = Vec::new();
        if let Err(mut provider_errors) = provider.load_content(self) {
            errors.append(&mut provider_errors);
        }
        // Upstream runs `UnitStances.loadAfterMods` after `loadContent` even
        // when individual assets produced warnings.
        if let Err(error) = super::registries::stances::load_after_mods(self) {
            errors.push(error);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// The most recently added record (`ContentLoader.getLastAdded`).
    pub fn last_added(&self) -> Option<ContentRef> {
        self.last_added
    }

    /// Registry epoch, bumped when content is added/removed (`arr_epoch`).
    pub fn arr_epoch(&self) -> u32 {
        self.arr_epoch
    }

    /// Whether the registry is in headless mode.
    pub fn is_headless(&self) -> bool {
        self.headless
    }

    /// Item lists used by campaign filters and the Erekir rebalance.
    pub fn set_item_lists(
        &mut self,
        serpulo: Vec<ItemId>,
        erekir: Vec<ItemId>,
        erekir_only: Vec<ItemId>,
    ) {
        self.items_serpulo = serpulo;
        self.items_erekir = erekir;
        self.items_erekir_only = erekir_only;
    }

    /// `Items.serpuloItems`.
    pub fn serpulo_items(&self) -> &[ItemId] {
        &self.items_serpulo
    }

    /// `Items.erekirItems`.
    pub fn erekir_items(&self) -> &[ItemId] {
        &self.items_erekir
    }

    /// `Items.erekirOnlyItems`.
    pub fn erekir_only_items(&self) -> &[ItemId] {
        &self.items_erekir_only
    }

    /// Convenience lookup: status id by name.
    pub fn status_id(&self, name: &str) -> Option<StatusId> {
        self.status_by_name(name).map(|record| record.id)
    }

    /// Convenience lookup: liquid id by name.
    pub fn liquid_id(&self, name: &str) -> Option<LiquidId> {
        self.liquid_by_name(name).map(|record| record.id)
    }

    /// Convenience lookup: item id by name.
    pub fn item_id(&self, name: &str) -> Option<ItemId> {
        self.item_by_name(name).map(|record| record.id)
    }

    /// Convenience lookup: block id by name.
    pub fn block_id(&self, name: &str) -> Option<BlockId> {
        self.block_by_name(name).map(|record| record.id)
    }

    /// Convenience lookup: unit type id by name.
    pub fn unit_id(&self, name: &str) -> Option<UnitTypeId> {
        self.unit_by_name(name).map(|record| record.id)
    }

    /// Convenience lookup: planet id by name.
    pub fn planet_id(&self, name: &str) -> Option<PlanetId> {
        self.planet_by_name(name).map(|record| record.id)
    }

    /// Starting loadouts (`Loadouts.java`; not a content ID space).
    pub fn loadouts(&self) -> &[LoadoutDef] {
        &self.loadouts
    }

    /// Installs the loadout table (`Loadouts.load`).
    pub fn set_loadouts(&mut self, loadouts: Vec<LoadoutDef>) {
        self.loadouts = loadouts;
    }

    /// Tech store (`Vars.content.techTree` equivalent).
    pub fn tech(&self) -> &TechStore {
        &self.tech
    }

    /// Per-tree build reports (missing names pending later registries).
    pub fn tech_build_reports(&self) -> &[(String, super::tech::TechTreeBuildReport)] {
        &self.tech_reports
    }

    /// Records a tree build report (called by `create_base_content`).
    pub(crate) fn push_tech_report(
        &mut self,
        tree: &str,
        report: super::tech::TechTreeBuildReport,
    ) {
        self.tech_reports.push((tree.to_owned(), report));
    }

    /// Registers `content.techNode`/`techNodes` (`TechNode` constructor).
    pub(crate) fn set_tech_node(&mut self, content: ContentRef, node: TechNodeRef) {
        super::tech::set_unlock_tech_node(self, content, node);
    }

    /// `Planets.<x>.techTree = tree`: assigns the tree and its planet to every node.
    pub fn set_planet_tech_tree(&mut self, planet: PlanetId, tree: TreeId) {
        if let Some(record) = self.planet_mut(planet) {
            record.tech_tree = Some(tree);
        }
        let Some(root) = self.tech.tree(tree).map(|record| record.root) else {
            return;
        };
        let mut nodes = Vec::new();
        self.tech.each(root, &mut |node| nodes.push(node));
        for node_ref in nodes {
            if let Some(node) = self.tech.node_mut(node_ref) {
                node.planet = Some(planet);
            }
        }
    }

    /// `TechNode.addPlanet`: adds `planet` to `shownPlanets` of the whole tree.
    pub fn add_planet_to_tree(&mut self, tree: TreeId, planet: PlanetId) {
        let Some(root) = self.tech.tree(tree).map(|record| record.root) else {
            return;
        };
        let mut nodes = Vec::new();
        self.tech.each(root, &mut |node| nodes.push(node));
        for node_ref in nodes {
            let content = self.tech.node(node_ref).and_then(|node| node.content);
            if let Some(content) = content {
                super::tech::with_unlock_fields(self, content, |fields| {
                    if !fields.shown_planets.contains(&planet) {
                        fields.shown_planets.push(planet);
                    }
                });
            }
        }
    }

    /// `TechNode.addDatabaseTab`: adds `tab` to `databaseTabs` of the whole tree.
    pub fn add_database_tab_to_tree(&mut self, tree: TreeId, tab: ContentRef) {
        let Some(root) = self.tech.tree(tree).map(|record| record.root) else {
            return;
        };
        let mut nodes = Vec::new();
        self.tech.each(root, &mut |node| nodes.push(node));
        for node_ref in nodes {
            let content = self.tech.node(node_ref).and_then(|node| node.content);
            if let Some(content) = content {
                super::tech::with_unlock_fields(self, content, |fields| {
                    if !fields.database_tabs.contains(&tab) {
                        fields.database_tabs.push(tab);
                    }
                });
            }
        }
    }

    fn sweep(&mut self, phase: LifecyclePhase) -> Result<(), ContentError> {
        let mut mod_errors: Vec<(ContentType, u16, ContentError)> = Vec::new();
        macro_rules! sweep_type {
            ($field:ident, $ty:ty) => {
                for record in self.$field.iter_mut() {
                    let result = match phase {
                        LifecyclePhase::Init => record.init_self(),
                        LifecyclePhase::PostInit => record.post_init(),
                        LifecyclePhase::LoadIcon => record.load_icon(),
                        LifecyclePhase::Load => record.load(),
                    };
                    if let Err(error) = result {
                        if record.minfo().is_modded() {
                            mod_errors.push((<$ty as Content>::TYPE, record.content_id(), error));
                        } else {
                            return Err(error);
                        }
                    }
                }
            };
        }
        sweep_type!(items, Item);
        sweep_type!(blocks, BlockDef);
        sweep_type!(bullets, BulletDef);
        sweep_type!(liquids, Liquid);
        sweep_type!(statuses, StatusEffect);
        sweep_type!(units, UnitTypeDef);
        sweep_type!(unit_commands, UnitCommandDef);
        sweep_type!(unit_stances, UnitStanceDef);
        sweep_type!(weathers, WeatherDef);
        sweep_type!(sectors, SectorPresetDef);
        sweep_type!(planets, PlanetDef);
        sweep_type!(teams, TeamEntry);
        for (type_, id, error) in &mod_errors {
            if let Some(sink) = self.mod_error_sink.as_mut() {
                sink.handle_content_error(ContentRef::new(*type_, *id), error);
            }
        }
        Ok(())
    }
}

/// Removes and returns the record at raw `id` (`None` when out of range).
fn take_record<T>(records: &mut Vec<T>, id: u16) -> Option<T> {
    let index = id as usize;
    if index < records.len() {
        Some(records.remove(index))
    } else {
        None
    }
}

/// Removes a mappable record and drops its name (`ContentLoader.remove`).
fn remove_mappable<T: Mappable>(records: &mut Vec<T>, names: &mut NameMaps, id: u16) {
    if let Some(mut record) = take_record(records, id) {
        let name = record.name().to_owned();
        let raw = record.content_id();
        names.remove(T::TYPE, &name, raw);
        record.remove_content();
    }
}

/// Registration helper: `handleContent` + `handleMappableContent` semantics.
fn register_mappable<T: Mappable>(
    records: &mut Vec<T>,
    names: &mut NameMaps,
    last_added: &mut Option<ContentRef>,
    current_mod: Option<&ModId>,
    mut record: T,
) -> Result<u16, ContentError> {
    let type_ = T::TYPE;
    let name = record.name().to_owned();
    if names.get(type_, &name).is_some() {
        // Upstream pops the half-registered record before throwing
        // (`ContentLoader.handleMappableContent`); here nothing was pushed yet.
        return Err(ContentError::DuplicateName(name));
    }
    let raw = u16::try_from(records.len()).map_err(|_| ContentError::IdSpaceExhausted)?;
    record.set_content_id(raw);
    if let Some(mod_id) = current_mod {
        let info = record.minfo_mut();
        info.mod_id = Some(mod_id.clone());
        if info.source_file.is_none() {
            info.source_file = Some(name.clone());
        }
    }
    records.push(record);
    names.insert(type_, &name, raw);
    *last_added = Some(ContentRef::new(type_, raw));
    Ok(raw)
}

/// Registration helper for non-mappable content (bullets, errors).
fn register_content<T: Content>(
    records: &mut Vec<T>,
    last_added: &mut Option<ContentRef>,
    mut record: T,
) -> Result<u16, ContentError> {
    let type_ = T::TYPE;
    let raw = u16::try_from(records.len()).map_err(|_| ContentError::IdSpaceExhausted)?;
    record.set_content_id(raw);
    records.push(record);
    *last_added = Some(ContentRef::new(type_, raw));
    Ok(raw)
}

/// Per-type accessor generation. `add_*` performs dense-ID assignment plus name
/// registration; `*_by_name` uses the per-type name map.
macro_rules! mappable_accessors {
    ($add:ident, $all:ident, $all_mut:ident, $get:ident, $get_mut:ident, $by_name:ident, $field:ident, $ty:ty, $id:ty) => {
        /// Adds a record, assigning its dense id and registering its name.
        pub fn $add(&mut self, record: $ty) -> Result<$id, ContentError> {
            let raw = register_mappable(
                &mut self.$field,
                &mut self.names,
                &mut self.last_added,
                self.current_mod.as_ref(),
                record,
            )?;
            self.arr_epoch = self.arr_epoch.wrapping_add(1);
            Ok(<$id>::new(raw))
        }

        /// All records in id order.
        pub fn $all(&self) -> &[$ty] {
            &self.$field
        }

        /// Mutable records in id order (lifecycle/link passes only).
        #[allow(dead_code)] // used by later-milestone link passes (M2/M3/M5)
        pub(crate) fn $all_mut(&mut self) -> &mut [$ty] {
            &mut self.$field
        }

        /// Record by typed id.
        pub fn $get(&self, id: $id) -> Option<&$ty> {
            self.$field.get(id.index())
        }

        /// Mutable record by typed id (mod/patch APIs only).
        pub fn $get_mut(&mut self, id: $id) -> Option<&mut $ty> {
            self.$field.get_mut(id.index())
        }

        /// Record by name.
        pub fn $by_name(&self, name: &str) -> Option<&$ty> {
            let raw = self.names.get(<$ty as Content>::TYPE, name)?;
            self.$field.get(raw as usize)
        }
    };
}

impl ContentRegistry {
    mappable_accessors!(
        add_item,
        items,
        items_mut,
        item,
        item_mut,
        item_by_name,
        items,
        Item,
        ItemId
    );
    mappable_accessors!(
        add_block,
        blocks,
        blocks_mut,
        block,
        block_mut,
        block_by_name,
        blocks,
        BlockDef,
        BlockId
    );
    mappable_accessors!(
        add_liquid,
        liquids,
        liquids_mut,
        liquid,
        liquid_mut,
        liquid_by_name,
        liquids,
        Liquid,
        LiquidId
    );
    mappable_accessors!(
        add_status,
        statuses,
        statuses_mut,
        status,
        status_mut,
        status_by_name,
        statuses,
        StatusEffect,
        StatusId
    );
    mappable_accessors!(
        add_unit,
        units,
        units_mut,
        unit,
        unit_mut,
        unit_by_name,
        units,
        UnitTypeDef,
        UnitTypeId
    );
    mappable_accessors!(
        add_unit_command,
        unit_commands,
        unit_commands_mut,
        unit_command,
        unit_command_mut,
        unit_command_by_name,
        unit_commands,
        UnitCommandDef,
        super::id::UnitCommandId
    );
    mappable_accessors!(
        add_unit_stance,
        unit_stances,
        unit_stances_mut,
        unit_stance,
        unit_stance_mut,
        unit_stance_by_name,
        unit_stances,
        UnitStanceDef,
        super::id::UnitStanceId
    );
    mappable_accessors!(
        add_weather,
        weathers,
        weathers_mut,
        weather,
        weather_mut,
        weather_by_name,
        weathers,
        WeatherDef,
        super::id::WeatherId
    );
    mappable_accessors!(
        add_sector,
        sectors,
        sectors_mut,
        sector,
        sector_mut,
        sector_by_name,
        sectors,
        SectorPresetDef,
        super::id::SectorId
    );
    mappable_accessors!(
        add_planet,
        planets,
        planets_mut,
        planet,
        planet_mut,
        planet_by_name,
        planets,
        PlanetDef,
        PlanetId
    );
    mappable_accessors!(
        add_team,
        teams,
        teams_mut,
        team,
        team_mut,
        team_by_name,
        teams,
        TeamEntry,
        TeamEntryId
    );

    /// Adds a bullet record (non-mappable).
    pub fn add_bullet(&mut self, record: BulletDef) -> Result<BulletId, ContentError> {
        let raw = register_content(&mut self.bullets, &mut self.last_added, record)?;
        self.arr_epoch = self.arr_epoch.wrapping_add(1);
        Ok(BulletId::new(raw))
    }

    /// All bullet records in id order.
    pub fn bullets(&self) -> &[BulletDef] {
        &self.bullets
    }

    /// Bullet record by id.
    pub fn bullet(&self, id: BulletId) -> Option<&BulletDef> {
        self.bullets.get(id.index())
    }

    /// Mutable bullet record by id.
    pub fn bullet_mut(&mut self, id: BulletId) -> Option<&mut BulletDef> {
        self.bullets.get_mut(id.index())
    }

    /// Adds an `ErrorContent` fallback record.
    pub fn add_error(&mut self, record: ErrorContent) -> Result<u16, ContentError> {
        register_content(&mut self.errors, &mut self.last_added, record)
    }

    /// Error record by id.
    pub fn error(&self, id: u16) -> Option<&ErrorContent> {
        self.errors.get(id as usize)
    }
}

/// Counts per content type (harness report helper; deterministic order).
pub fn content_counts(registry: &ContentRegistry) -> BTreeMap<ContentType, usize> {
    let mut counts = BTreeMap::new();
    for type_ in ContentType::ALL {
        counts.insert(type_, registry.type_len(type_));
    }
    counts
}

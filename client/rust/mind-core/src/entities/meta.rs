// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Entity/component metadata replacing the Java annotation processor.
//!
//! Ported from `annotations/src/main/java/mindustry/annotations/Annotations.java`
//! and `entity/EntityProcess.java`. The `#[derive(SimComponent)]` macro emits
//! [`ComponentMeta`]; [`EntityDefSpec`]s (from `entity_def!`) are validated and
//! lifted into [`EntityDef`]s by [`EntityRegistry`].
//!
//! Reconciliation (HLP §13): plan 04 landed `io::entity::{EntityDefMeta,
//! FieldDesc}` first. Plan 05's [`FieldMeta`] is the canonical *sim/sync*
//! surface; [`FieldMeta::to_field_desc`] bridges into plan 04's IO codec so the
//! two never diverge silently.

use std::collections::BTreeMap;

use super::groups::{GroupMask, compute_group_mask};
use crate::io::entity::{FieldDesc, FieldFlag};

/// Wire/logical type of a field (metadata vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// `f32`.
    F32,
    /// `f64`.
    F64,
    /// `i8`.
    I8,
    /// `i16`.
    I16,
    /// `i32`.
    I32,
    /// `i64`.
    I64,
    /// `u8`.
    U8,
    /// `u16`.
    U16,
    /// `u32`.
    U32,
    /// `u64`.
    U64,
    /// `bool`.
    Bool,
    /// A Bevy entity handle.
    Entity,
    /// A content id (`BlockId`, `ItemId`, ...).
    Content,
    /// An opaque struct (name recorded for diagnostics).
    Struct(&'static str),
    /// `Option<T>`.
    Option(&'static FieldType),
    /// `Vec<T>`.
    Vec(&'static FieldType),
}

impl FieldType {
    /// Plan 04 wire type name (`IoField::TYPE_NAME` vocabulary).
    pub const fn wire_name(self) -> &'static str {
        match self {
            FieldType::F32 => "float",
            FieldType::F64 => "double",
            FieldType::I8 => "byte",
            FieldType::I16 => "short",
            FieldType::I32 => "int",
            FieldType::I64 => "long",
            FieldType::U8 => "ubyte",
            FieldType::U16 => "ushort",
            FieldType::U32 => "uint",
            FieldType::U64 => "ulong",
            FieldType::Bool => "bool",
            FieldType::Entity => "entity",
            FieldType::Content => "content",
            FieldType::Struct(_) => "object",
            FieldType::Option(inner) | FieldType::Vec(inner) => inner.wire_name(),
        }
    }

    /// Fixed byte size for plan 04 revision manifests, `-1` when variable.
    pub const fn wire_size(self) -> i32 {
        match self {
            FieldType::F32 | FieldType::I32 | FieldType::U32 => 4,
            FieldType::F64 | FieldType::I64 | FieldType::U64 => 8,
            FieldType::I8 | FieldType::U8 | FieldType::Bool => 1,
            FieldType::I16 | FieldType::U16 => 2,
            FieldType::Entity
            | FieldType::Content
            | FieldType::Struct(_)
            | FieldType::Option(_)
            | FieldType::Vec(_) => -1,
        }
    }
}

/// How a field participates in save/sync (`@SyncField`, `@SyncLocal`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Serialized + synced (`Plain`).
    Plain,
    /// `@SyncField` with interpolation.
    SyncFloat {
        /// Whether the interpolated value is clamped.
        clamped: bool,
        /// Whether plan 21 generates interpolation companions.
        interp: bool,
    },
    /// `@SyncLocal` — client-authoritative, never read from the server.
    SyncLocal,
    /// `@NoSync` — saved, not synced.
    NoSync,
    /// `@NoSerialize` — synced, never saved.
    NoSerialize,
    /// `transient`.
    Transient,
    /// `@ReadOnly` — synced one-way.
    ReadOnly,
}

impl FieldKind {
    /// Whether the field is written to saves (plan 04).
    pub const fn is_saved(self) -> bool {
        !matches!(self, FieldKind::NoSerialize | FieldKind::Transient)
    }

    /// Whether the field is written to network sync (plan 21).
    pub const fn is_synced(self) -> bool {
        !matches!(self, FieldKind::NoSync)
    }

    /// Plan 04 revision-manifest flags for this field kind (static table).
    pub const fn static_flags(self) -> &'static [FieldFlag] {
        const PLAIN: &[FieldFlag] = &[FieldFlag::Save, FieldFlag::Sync];
        const SYNC_FLOAT: &[FieldFlag] =
            &[FieldFlag::Save, FieldFlag::Sync, FieldFlag::InterpLinear];
        const SYNC_FLOAT_CLAMPED: &[FieldFlag] = &[
            FieldFlag::Save,
            FieldFlag::Sync,
            FieldFlag::InterpLinear,
            FieldFlag::Clamped,
        ];
        const SYNC_LOCAL: &[FieldFlag] = &[FieldFlag::Save, FieldFlag::Sync, FieldFlag::SyncLocal];
        const NO_SYNC: &[FieldFlag] = &[FieldFlag::Save];
        const NO_SERIALIZE: &[FieldFlag] = &[FieldFlag::Sync];
        const TRANSIENT: &[FieldFlag] = &[FieldFlag::Transient];
        match self {
            FieldKind::Plain | FieldKind::ReadOnly => PLAIN,
            FieldKind::SyncFloat { clamped: true, .. } => SYNC_FLOAT_CLAMPED,
            FieldKind::SyncFloat { clamped: false, .. } => SYNC_FLOAT,
            FieldKind::SyncLocal => SYNC_LOCAL,
            FieldKind::NoSync => NO_SYNC,
            FieldKind::NoSerialize => NO_SERIALIZE,
            FieldKind::Transient => TRANSIENT,
        }
    }
}

/// One field's metadata (the single sim/sync surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldMeta {
    /// Field name (Rust declaration name).
    pub name: &'static str,
    /// Save/sync participation.
    pub kind: FieldKind,
    /// Logical type.
    pub ty: FieldType,
    /// Revision this field was added in (plan 04 bookkeeping).
    pub revision_added: u16,
}

impl FieldMeta {
    /// Bridges into plan 04's IO codec descriptor.
    pub fn to_field_desc(&self) -> FieldDesc {
        FieldDesc {
            name: self.name,
            type_: self.ty.wire_name(),
            size: self.ty.wire_size(),
            flags: self.kind.static_flags(),
        }
    }
}

/// Ordered system slot for one component method (`@MethodPriority`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemOrder {
    /// Method name (`update`, `updateTile`, ...).
    pub method: &'static str,
    /// Sort priority (ascending runs first; mirrors codegen stable ordering).
    pub priority: i32,
}

/// Metadata emitted by `#[derive(SimComponent)]`.
#[derive(Debug, Clone, Copy)]
pub struct ComponentMeta {
    /// Component name (Rust struct name by default).
    pub name: &'static str,
    /// Whether the component may be the base of a hierarchy.
    pub base: bool,
    /// Field metadata in declaration order.
    pub fields: &'static [FieldMeta],
    /// Ordered method table.
    pub methods: &'static [SystemOrder],
}

/// Implemented by `#[derive(SimComponent)]`.
pub trait SimComponentMeta {
    /// This component's metadata.
    fn component_meta() -> &'static ComponentMeta;

    /// Component name (defaults to the metadata name).
    fn component_name() -> &'static str {
        Self::component_meta().name
    }
}

/// One entry emitted by `entity_def!`.
#[derive(Debug, Clone, Copy)]
pub struct EntityDefSpec {
    /// Def name (parity anchor).
    pub name: &'static str,
    /// Component names in declaration order.
    pub components: &'static [&'static str],
}

/// Internal registry index (not the save/network ABI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityDefId(pub u16);

/// A fully resolved entity archetype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityDef {
    /// Internal registry index.
    pub id: EntityDefId,
    /// Def name.
    pub name: &'static str,
    /// Class id (save/network ABI; from `classids.properties` via plan 04).
    pub class_id: u16,
    /// Computed group membership.
    pub groups: GroupMask,
    /// Whether instances are pooled on removal.
    pub pooled: bool,
    /// Serialized in saves.
    pub serialize: bool,
    /// Serialized in sync.
    pub genio: bool,
    /// Legacy def (not created at runtime).
    pub legacy: bool,
    /// Component names.
    pub components: Vec<&'static str>,
    /// Aggregated field metadata.
    pub fields: Vec<FieldMeta>,
    /// Aggregated ordered method table.
    pub methods: Vec<SystemOrder>,
}

impl EntityDef {
    /// Field metadata for one name, if present.
    pub fn field(&self, name: &str) -> Option<&FieldMeta> {
        self.fields.iter().find(|field| field.name == name)
    }
}

/// Registry errors (structured; never panics on runtime data).
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// A def referenced a component that was never registered.
    #[error("entity def `{def}` references unregistered component `{component}`")]
    UnknownComponent {
        /// Def name.
        def: &'static str,
        /// Missing component.
        component: &'static str,
    },
    /// Two defs share a name.
    #[error("duplicate entity def `{0}`")]
    DuplicateDef(String),
}

/// Validated registry of components and entity defs.
#[derive(Debug, Default)]
pub struct EntityRegistry {
    defs: Vec<EntityDef>,
    by_name: BTreeMap<String, EntityDefId>,
    components: BTreeMap<String, &'static ComponentMeta>,
    /// Whether [`EntityRegistry::build_from_specs`] has run.
    built: bool,
}

impl EntityRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a component's metadata (from `#[derive(SimComponent)]`).
    pub fn register_component(&mut self, meta: &'static ComponentMeta) {
        self.components.insert(meta.name.to_owned(), meta);
    }

    /// Registers a component through its trait impl.
    pub fn register<T: SimComponentMeta>(&mut self) {
        self.register_component(T::component_meta());
    }

    /// Number of registered defs.
    pub fn defs_len(&self) -> usize {
        self.defs.len()
    }

    /// Number of registered components.
    pub fn components_len(&self) -> usize {
        self.components.len()
    }

    /// Whether the registry has been built.
    pub const fn is_built(&self) -> bool {
        self.built
    }

    /// Looks a def up by name.
    pub fn def_by_name(&self, name: &str) -> Option<&EntityDef> {
        self.by_name
            .get(name)
            .and_then(|id| self.defs.get(id.0 as usize))
    }

    /// Looks a def up by internal id.
    pub fn def(&self, id: EntityDefId) -> Option<&EntityDef> {
        self.defs.get(id.0 as usize)
    }

    /// All defs in registration order.
    pub fn defs(&self) -> &[EntityDef] {
        &self.defs
    }

    /// Registers a component if not already present, then validates and
    /// resolves every [`EntityDefSpec`] into an [`EntityDef`].
    ///
    /// `class_ids` maps def names to the plan-04 class-id ABI; missing names
    /// fall back to `1000 + index` (mod defs) so this never collides with the
    /// committed plan-04 range.
    pub fn build_from_specs(
        &mut self,
        specs: &[EntityDefSpec],
        class_ids: &BTreeMap<String, u16>,
    ) -> Result<(), RegistryError> {
        for spec in specs {
            if self.by_name.contains_key(spec.name) {
                return Err(RegistryError::DuplicateDef(spec.name.to_owned()));
            }
            let mut fields = Vec::new();
            let mut methods = Vec::new();
            for component in spec.components {
                let meta = self.components.get(*component).copied().ok_or(
                    RegistryError::UnknownComponent {
                        def: spec.name,
                        component,
                    },
                )?;
                fields.extend_from_slice(meta.fields);
                methods.extend_from_slice(meta.methods);
            }
            // Stable, deterministic method ordering: priority asc, then method
            // name, then component order (mirrors codegen's stable sort).
            methods.sort_by(|a, b| {
                a.priority
                    .cmp(&b.priority)
                    .then_with(|| a.method.cmp(b.method))
            });
            let groups = compute_group_mask(spec.components, &[]);
            let id = EntityDefId(u16::try_from(self.defs.len()).unwrap_or(u16::MAX));
            let class_id = class_ids
                .get(spec.name)
                .copied()
                .unwrap_or_else(|| 1000 + id.0);
            self.defs.push(EntityDef {
                id,
                name: spec.name,
                class_id,
                groups,
                pooled: groups.contains(super::groups::GroupKind::Bullet)
                    || groups.contains(super::groups::GroupKind::Effect),
                serialize: true,
                genio: true,
                legacy: false,
                components: spec.components.to_vec(),
                fields,
                methods,
            });
            self.by_name.insert(spec.name.to_owned(), id);
        }
        self.built = true;
        Ok(())
    }

    /// Group mask for a def by name (`None` when unknown).
    pub fn group_mask(&self, name: &str) -> Option<GroupMask> {
        self.def_by_name(name).map(|def| def.groups)
    }

    /// Stable JSON metadata for `mind-headless meta entities` (§6.2).
    pub fn metadata_json(&self) -> serde_json::Value {
        let defs: Vec<serde_json::Value> = self
            .defs
            .iter()
            .map(|def| {
                let fields: Vec<serde_json::Value> = def
                    .fields
                    .iter()
                    .map(|field| {
                        serde_json::json!({
                            "name": field.name,
                            "kind": format!("{:?}", field.kind),
                            "type": field.ty.wire_name(),
                            "since": field.revision_added,
                        })
                    })
                    .collect();
                let methods: Vec<serde_json::Value> = def
                    .methods
                    .iter()
                    .map(|method| {
                        serde_json::json!({"method": method.method, "priority": method.priority})
                    })
                    .collect();
                serde_json::json!({
                    "name": def.name,
                    "classId": def.class_id,
                    "groups": def.groups.names(),
                    "pooled": def.pooled,
                    "serialize": def.serialize,
                    "components": def.components,
                    "fields": fields,
                    "methods": methods,
                })
            })
            .collect();
        serde_json::json!({
            "format": 1,
            "components": self.components.len(),
            "defs": defs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::groups::GroupKind;

    #[derive(bevy_ecs::component::Component, mind_macros::SimComponent)]
    #[sim(component, base, methods(update_priority = 0))]
    #[allow(dead_code)]
    struct TestHealth {
        health: f32,
        max_health: f32,
        #[sim(sync_local)]
        elevation: f32,
        #[sim(no_serialize, since = 2)]
        cached: i32,
    }

    #[derive(bevy_ecs::component::Component, mind_macros::SimComponent)]
    #[sim(component)]
    #[allow(dead_code)]
    struct TestUnitCore {
        #[sim(sync_float(clamped, interp))]
        rotation: f32,
        dead: bool,
    }

    mind_macros::entity_def! {
        TestUnit = [TestUnitCore, TestHealth];
    }

    fn registry() -> EntityRegistry {
        let mut registry = EntityRegistry::new();
        registry.register_component(TestHealth::component_meta());
        registry.register_component(TestUnitCore::component_meta());
        registry
            .build_from_specs(ENTITY_DEF_SPECS, &BTreeMap::new())
            .expect("registry builds");
        registry
    }

    #[test]
    fn field_meta_golden() {
        let registry = registry();
        let def = registry.def_by_name("TestUnit").expect("def present");
        // `TestUnitCore` declares no group component, so it lands in `all`.
        let health = def.field("health").expect("health field");
        assert_eq!(health.kind, FieldKind::Plain);
        assert_eq!(health.ty, FieldType::F32);
        assert_eq!(
            def.field("elevation").map(|f| f.kind),
            Some(FieldKind::SyncLocal)
        );
        assert_eq!(
            def.field("cached").map(|f| (f.kind, f.revision_added)),
            Some((FieldKind::NoSerialize, 2))
        );
        assert_eq!(
            def.field("rotation").map(|f| f.kind),
            Some(FieldKind::SyncFloat {
                clamped: true,
                interp: true
            })
        );

        // Plan 04 bridge: field flags are the revision-manifest vocabulary.
        let desc = def.field("rotation").map(FieldMeta::to_field_desc);
        assert!(desc.is_some_and(|desc| desc.flags.contains(&FieldFlag::InterpLinear)));
    }

    #[test]
    fn group_membership_is_computed_from_components() {
        let registry = registry();
        // No `Unit` component in the test def, so it is in `all`, not `unit`.
        let mask = registry.group_mask("TestUnit").expect("mask");
        assert!(mask.contains(GroupKind::All));
        assert!(!mask.contains(GroupKind::Unit));
    }

    #[test]
    fn unknown_component_is_an_error() {
        static BROKEN: &[EntityDefSpec] = &[EntityDefSpec {
            name: "Broken",
            components: &["NeverRegistered"],
        }];
        let mut registry = EntityRegistry::new();
        let result = registry.build_from_specs(BROKEN, &BTreeMap::new());
        assert!(matches!(
            result,
            Err(RegistryError::UnknownComponent {
                component: "NeverRegistered",
                ..
            })
        ));
    }
}

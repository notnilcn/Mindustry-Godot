// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Entity framework replacing the Java annotation pipeline (plan 05 §3.5–3.6).
//!
//! Composition is data + systems: `#[derive(SimComponent)]` metadata + explicit
//! ordered system tables. `Groups`/`EntityGroup` provide typed, stable-ordered
//! sets; `lifecycle` provides the single spawn/remove path.

pub mod comp;
pub mod defs;
pub mod group;
pub mod groups;
pub mod lifecycle;
pub mod mapping;
pub mod meta;

pub use defs::{ENTITY_DEF_SPECS, vanilla_registry};
pub use group::{EntityGroup, EntityIndexer, NoopIndexer};
pub use groups::{ALL_GROUPS, GROUP_DEFS, GroupDef, GroupKind, GroupMask, Groups};
pub use lifecycle::{EntityIds, EntityPool, EntityPools, SimEntity, sim_remove, sim_spawn};
pub use mapping::EntityMapping;
pub use meta::{
    ComponentMeta, EntityDef, EntityDefId, EntityDefSpec, EntityRegistry, FieldKind, FieldMeta,
    FieldType, RegistryError, SimComponentMeta, SystemOrder,
};

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The entity-def registry (plan 04 §3.5).
//!
//! Ported from `annotations/.../entity/EntityProcess.java` (`EntityMapping`
//! id map). [`define_entity_defs!`] lists the entity defs with their codec
//! types; plans 05/11 register their components here and keep the field order
//! stable (R2 — this list is the reconciliation point with 05/11).

/// Declares the entity-def registry (the `EntityDefs!{}` list of plan 04 §3.5).
///
/// Each entry is a type implementing [`EntityCodec`] (via
/// `#[derive(mind_derive::EntityIo)]`); the macro emits `entity_defs()`,
/// `def_by_name()` and `def_by_class_id()`.
#[macro_export]
macro_rules! define_entity_defs {
    ($($ty:ty),* $(,)?) => {
        /// All registered entity defs, in declaration order (append-only).
        pub fn entity_defs() -> &'static [$crate::io::entity::EntityDefMeta] {
            static DEFS: ::std::sync::LazyLock<
                ::std::vec::Vec<$crate::io::entity::EntityDefMeta>,
            > = ::std::sync::LazyLock::new(|| {
                ::std::vec![
                    $(
                        $crate::io::entity::EntityDefMeta {
                            name: <$ty as $crate::io::entity::EntityCodec>::NAME,
                            class_id: <$ty as $crate::io::entity::EntityCodec>::CLASS_ID,
                            serialize: <$ty as $crate::io::entity::EntityCodec>::SERIALIZE,
                            sync: <$ty as $crate::io::entity::EntityCodec>::SYNC,
                            fields: <$ty as $crate::io::entity::EntityCodec>::fields(),
                            sync_fields: <$ty as $crate::io::entity::EntityCodec>::sync_fields(),
                        },
                    )*
                ]
            });
            &DEFS
        }

        /// Def metadata by name (`EntityMapping.map(name)`).
        pub fn def_by_name(name: &str) -> Option<&'static $crate::io::entity::EntityDefMeta> {
            entity_defs().iter().find(|def| def.name == name)
        }

        /// Def metadata by class ID (`EntityMapping.idMap[id]`).
        pub fn def_by_class_id(
            class_id: u8,
        ) -> Option<&'static $crate::io::entity::EntityDefMeta> {
            entity_defs().iter().find(|def| def.class_id == class_id)
        }
    };
}

// The canonical def list (R2: plans 05/11 append their components here).
define_entity_defs! {
    crate::ecs::BuildingComp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lookups() {
        assert_eq!(entity_defs().len(), 1);
        let building = def_by_name("BuildingComp").unwrap();
        assert_eq!(building.class_id, 6);
        assert!(building.serialize);
        assert!(def_by_class_id(6).is_some());
        // Unknown class IDs are skipped by save readers, never fatal.
        assert!(def_by_class_id(200).is_none());
        assert!(def_by_name("NopeComp").is_none());
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Content mapping for save-load reads (plan 04 §3.4).
//!
//! Ported from `TypeIO.ContentMapper` and the `content.setTemporaryMapper`
//! discipline (`SaveIO.load` `finally` block, `SaveVersion.readContentHeader`).
//! The [`TemporaryMapperGuard`] is the RAII equivalent of upstream's
//! `finally { content.setTemporaryMapper(null) }` (plan 04 §3.10).

use crate::content::load::TemporaryMapper;
use crate::content::{ContentRegistry, ContentType};

/// Maps save-read content ids to the current registry (`TypeIO.ContentMapper`).
pub trait ContentMapper {
    /// Maps one raw id, or `None` for invalid/removed content.
    fn map_id(&self, type_: ContentType, id: u16) -> Option<u16>;
}

/// Resolves through a [`ContentRegistry`], honoring any installed temporary
/// mapper (`ContentLoader.getByID` semantics: default-0 for unknown, `None`
/// for invalid ids).
pub struct RegistryContentMapper<'a> {
    registry: &'a ContentRegistry,
}

impl<'a> RegistryContentMapper<'a> {
    /// Wraps a registry.
    pub fn new(registry: &'a ContentRegistry) -> Self {
        Self { registry }
    }
}

impl ContentMapper for RegistryContentMapper<'_> {
    fn map_id(&self, type_: ContentType, id: u16) -> Option<u16> {
        self.registry
            .get_by_id(type_, i32::from(id))
            .map(|content| content.id)
    }
}

/// RAII guard installing a [`TemporaryMapper`] and clearing it on drop
/// (`try/finally` parity, plan 04 §3.10).
pub struct TemporaryMapperGuard<'a> {
    registry: &'a mut ContentRegistry,
}

impl<'a> TemporaryMapperGuard<'a> {
    /// Installs the mapper; `None` just defers the clear to drop.
    pub fn install(registry: &'a mut ContentRegistry, mapper: Option<TemporaryMapper>) -> Self {
        registry.set_temporary_mapper(mapper);
        Self { registry }
    }

    /// The wrapped registry.
    pub fn registry(&self) -> &ContentRegistry {
        self.registry
    }

    /// The wrapped registry, mutably.
    pub fn registry_mut(&mut self) -> &mut ContentRegistry {
        self.registry
    }
}

impl Drop for TemporaryMapperGuard<'_> {
    fn drop(&mut self) {
        self.registry.set_temporary_mapper(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    #[test]
    fn registry_mapper_honors_temporary_mapper() {
        let mut registry = test_registry();
        // No mapper: out-of-range ids are invalid.
        assert!(
            RegistryContentMapper::new(&registry)
                .map_id(ContentType::Block, 65_000)
                .is_none()
        );
        assert_eq!(
            RegistryContentMapper::new(&registry).map_id(ContentType::Block, 5),
            Some(5)
        );

        // With a temp mapper installed, id 5 -> id 9.
        let mut mapper = TemporaryMapper::new();
        mapper.set(ContentType::Block, 5, Some(9));
        {
            let _guard = TemporaryMapperGuard::install(&mut registry, Some(mapper));
            assert_eq!(
                RegistryContentMapper::new(_guard.registry()).map_id(ContentType::Block, 5),
                Some(9)
            );
        }
        // Drop clears the mapper.
        assert_eq!(
            RegistryContentMapper::new(&registry).map_id(ContentType::Block, 5),
            Some(5)
        );
    }
}

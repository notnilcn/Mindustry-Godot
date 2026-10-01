// SPDX-License-Identifier: GPL-3.0-only

//! Block registry — P0 placeholder view (plan 02 M3 replaces it).
//!
//! Ported from `core/src/mindustry/content/Blocks.java` (metadata half; behavior
//! in plan 07). Until M3 ships the ~418 vanilla block records, this module keeps
//! the P0 registry that `Sim` and the scenarios use: `air = 0`, `stone-wall = 1`.
//! Names/IDs stay valid across the M3 replacement (append-only parity ABI).

use indexmap::IndexMap;

use super::super::bundle::BundleView;
use super::super::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use super::super::id::BlockId;
use super::super::settings_store::UnlockStore;
use super::super::{ContentError, ContentType};

/// Block content record (identity subset until M3 expands the metadata half).
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDef {
    /// Dense id in the block content space.
    pub id: BlockId,
    /// Content name (parity ABI).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Unlock/database fields.
    pub unlock: UnlockFields,
}

impl BlockDef {
    /// Creates a block record (M3 fills the full field set).
    pub fn new(name: &str, bundle: &dyn BundleView, store: &dyn UnlockStore) -> Self {
        Self {
            id: BlockId::new(0),
            name: name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            unlock: UnlockFields::new(ContentType::Block, name, bundle, store),
        }
    }
}

impl Content for BlockDef {
    const TYPE: ContentType = ContentType::Block;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = BlockId::new(id);
    }

    fn minfo(&self) -> &ModContentInfo {
        &self.minfo
    }

    fn minfo_mut(&mut self) -> &mut ModContentInfo {
        &mut self.minfo
    }

    fn removed(&self) -> bool {
        self.removed
    }

    fn set_removed(&mut self, removed: bool) {
        self.removed = removed;
    }

    fn kind_name(&self) -> &'static str {
        "Block"
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn unlock_fields(&self) -> Option<&UnlockFields> {
        Some(&self.unlock)
    }

    fn post_init(&mut self) -> Result<(), ContentError> {
        self.unlock.post_init();
        Ok(())
    }
}

impl Mappable for BlockDef {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for BlockDef {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

/// P0 block registry view (`air = 0`, `stone-wall = 1`).
///
/// Registration is append-only: ids are assigned in registration order and
/// existing names can never be re-registered.
#[derive(Debug, Clone, Default)]
pub struct Blocks {
    names: Vec<String>,
    by_name: IndexMap<String, u16>,
}

impl Blocks {
    /// Creates the P0 registry (`air = 0`, `stone-wall = 1`).
    ///
    /// Placeholder for `core/src/mindustry/content/Blocks.java`; plan 02 M3
    /// replaces it with the real vanilla content load.
    pub fn new() -> Self {
        let mut blocks = Self::default();
        let air = blocks.push("air");
        let wall = blocks.push("stone-wall");
        debug_assert_eq!(air, BlockId::AIR.raw());
        debug_assert_eq!(wall, BlockId::STONE_WALL.raw());
        debug_assert!(blocks.assert_invariants().is_ok());
        blocks
    }

    /// Number of registered blocks.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the registry has no blocks (never true for `Blocks::new`).
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Name for an id (parity ABI).
    pub fn name(&self, id: BlockId) -> Result<&str, ContentError> {
        self.names
            .get(id.index())
            .map(String::as_str)
            .ok_or(ContentError::UnknownId(id.raw()))
    }

    /// Id for a name (parity ABI).
    pub fn id(&self, name: &str) -> Result<BlockId, ContentError> {
        self.by_name
            .get(name)
            .map(|id| BlockId::new(*id))
            .ok_or_else(|| ContentError::UnknownName(name.to_owned()))
    }

    /// Whether a name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Appends a block, returning its id. The name is the parity ABI and is
    /// rejected on a second registration.
    pub fn register(&mut self, name: &str) -> Result<BlockId, ContentError> {
        if self.by_name.contains_key(name) {
            return Err(ContentError::DuplicateName(name.to_owned()));
        }
        let raw = u16::try_from(self.names.len()).map_err(|_| ContentError::IdSpaceExhausted)?;
        self.names.push(name.to_owned());
        self.by_name.insert(name.to_owned(), raw);
        Ok(BlockId::new(raw))
    }

    /// Asserts the append-only invariants of the P0 registry.
    pub fn assert_invariants(&self) -> Result<(), ContentError> {
        if self.name(BlockId::AIR)? != "air" {
            return Err(ContentError::UnknownName(String::from(
                "air must stay block id 0",
            )));
        }
        if self.name(BlockId::STONE_WALL)? != "stone-wall" {
            return Err(ContentError::UnknownName(String::from(
                "stone-wall must stay block id 1",
            )));
        }
        if self.len() != self.by_name.len() {
            return Err(ContentError::DuplicateName(String::from(
                "registry has duplicate names",
            )));
        }
        for (idx, name) in self.names.iter().enumerate() {
            if self.by_name.get(name).copied() != Some(idx as u16) {
                return Err(ContentError::UnknownName(format!(
                    "name `{name}` maps to an inconsistent id"
                )));
            }
        }
        Ok(())
    }

    fn push(&mut self, name: &str) -> u16 {
        let raw = self.names.len() as u16;
        self.names.push(name.to_owned());
        self.by_name.insert(name.to_owned(), raw);
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `tests/src/test/java/ApplicationTests.java` `initialization()`:
    /// the content map must be non-empty and name/ID lookups must be stable.
    #[test]
    fn spine_registry_non_empty() {
        let mut blocks = Blocks::new();
        assert!(!blocks.is_empty(), "content registry must be non-empty");
        assert_eq!(blocks.name(BlockId::AIR).unwrap(), "air");
        assert_eq!(blocks.name(BlockId::STONE_WALL).unwrap(), "stone-wall");
        assert_eq!(blocks.id("air").unwrap(), BlockId::AIR);
        assert_eq!(blocks.id("stone-wall").unwrap(), BlockId::STONE_WALL);
        assert!(blocks.contains("stone-wall"));
        assert_eq!(
            blocks.id("does-not-exist"),
            Err(ContentError::UnknownName("does-not-exist".to_owned()))
        );
        assert_eq!(
            blocks.name(BlockId::new(99)),
            Err(ContentError::UnknownId(99))
        );

        // Append-only: new registrations take the next id and never move existing names.
        let appended = blocks.register("spine-test").unwrap();
        assert_eq!(appended, BlockId::new(2));
        assert_eq!(blocks.name(BlockId::AIR).unwrap(), "air");
        assert_eq!(blocks.name(BlockId::STONE_WALL).unwrap(), "stone-wall");
        assert_eq!(
            blocks.register("spine-test"),
            Err(ContentError::DuplicateName("spine-test".to_owned()))
        );
        blocks.assert_invariants().unwrap();
    }
}

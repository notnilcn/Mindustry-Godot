// SPDX-License-Identifier: GPL-3.0-only

//! Placeholder content registry.
//!
//! Ported from `core/src/mindustry/ctype/ContentType.java` and `Content.java`.
//! Plan 02 replaces the internals; names and IDs are parity/mod ABI and stay
//! valid (append-only, never reordered).

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Placeholder content type. Variants are append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Block,
}

/// Per-type content id. Append-only per `ContentType` (parity ABI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ContentId(pub u16);

/// Id of a block in the block content space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockId(pub u16);

impl BlockId {
    /// The always-present empty block (`0`).
    pub const AIR: BlockId = BlockId(0);
    /// Placeholder wall block (`1`).
    pub const STONE_WALL: BlockId = BlockId(1);

    /// Raw numeric id.
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl Default for BlockId {
    fn default() -> Self {
        BlockId::AIR
    }
}

/// Errors raised by the content registry.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ContentError {
    /// A content name may only be registered once.
    #[error("content name `{0}` is already registered")]
    DuplicateName(String),
    /// The requested name does not exist.
    #[error("content name `{0}` is unknown")]
    UnknownName(String),
    /// The requested id does not exist.
    #[error("content id {0} is out of range")]
    UnknownId(u16),
    /// The registry exceeded its `u16` id space.
    #[error("content registry exhausted its 16-bit id space")]
    IdSpaceExhausted,
}

/// Block registry.
///
/// P0 contents: `air = 0`, `stone-wall = 1`. Registration is append-only: ids are
/// assigned in registration order and existing names can never be re-registered.
#[derive(Debug, Clone, Default)]
pub struct Blocks {
    names: Vec<String>,
    by_name: IndexMap<String, u16>,
}

impl Blocks {
    /// Creates the P0 registry (`air = 0`, `stone-wall = 1`).
    ///
    /// Placeholder for `core/src/mindustry/content/Blocks.java`; plan 02 replaces
    /// it with the real vanilla content load.
    pub fn new() -> Self {
        let mut blocks = Self::default();
        let air = blocks.push("air");
        let wall = blocks.push("stone-wall");
        debug_assert_eq!(air, BlockId::AIR.get());
        debug_assert_eq!(wall, BlockId::STONE_WALL.get());
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
            .get(id.0 as usize)
            .map(String::as_str)
            .ok_or(ContentError::UnknownId(id.0))
    }

    /// Id for a name (parity ABI).
    pub fn id(&self, name: &str) -> Result<BlockId, ContentError> {
        self.by_name
            .get(name)
            .map(|id| BlockId(*id))
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
        Ok(BlockId(raw))
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
        assert_eq!(blocks.name(BlockId(99)), Err(ContentError::UnknownId(99)));

        // Append-only: new registrations take the next id and never move existing names.
        let appended = blocks.register("spine-test").unwrap();
        assert_eq!(appended, BlockId(2));
        assert_eq!(blocks.name(BlockId::AIR).unwrap(), "air");
        assert_eq!(blocks.name(BlockId::STONE_WALL).unwrap(), "stone-wall");
        assert_eq!(
            blocks.register("spine-test"),
            Err(ContentError::DuplicateName("spine-test".to_owned()))
        );
        blocks.assert_invariants().unwrap();
    }
}

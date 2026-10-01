// SPDX-License-Identifier: GPL-3.0-only

//! Team entries — empty by design.
//!
//! Ported from `core/src/mindustry/content/TeamEntries.java`. Upstream's `load()`
//! has only commented-out `new TeamEntry(Team.x)` lines ("more will be added
//! later"), so the registry stays empty; the commented entries are deliberately
//! not ported (plan 02 §4).

use super::super::bundle::BundleView;
use super::super::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use super::super::id::TeamEntryId;
use super::super::load::ContentRegistry;
use super::super::settings_store::UnlockStore;
use super::super::{ContentError, ContentType};

/// Team lore entry (`mindustry.type.TeamEntry`), database-only.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamEntry {
    /// Dense id in the team content space.
    pub id: TeamEntryId,
    /// Content name (parity ABI; team name).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Unlock/database fields.
    pub unlock: UnlockFields,
    /// Team tag (`Team.name`).
    pub team: String,
}

impl TeamEntry {
    /// Creates a team entry.
    pub fn new(name: &str, team: &str, bundle: &dyn BundleView, store: &dyn UnlockStore) -> Self {
        Self {
            id: TeamEntryId::new(0),
            name: name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            unlock: UnlockFields::new(ContentType::Team, name, bundle, store),
            team: team.to_owned(),
        }
    }

    /// `displayExtra` bundle key (`team.<name>.log`).
    pub fn display_key(&self) -> String {
        format!("team.{}.log", self.name)
    }
}

impl Content for TeamEntry {
    const TYPE: ContentType = ContentType::Team;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = TeamEntryId::new(id);
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
        "TeamEntry"
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

impl Mappable for TeamEntry {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for TeamEntry {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

/// `TeamEntries.load()` — empty by design.
pub fn load(
    _registry: &mut ContentRegistry,
    _bundle: &dyn BundleView,
    _store: &dyn UnlockStore,
) -> Result<(), ContentError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;

    #[test]
    fn empty_by_design() {
        let registry = test_registry();
        assert_eq!(registry.teams().len(), 0);
    }
}

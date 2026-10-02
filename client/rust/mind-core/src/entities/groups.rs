// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Groups` — the ten canonical entity sets and their membership rules.
//!
//! Ported from `core/src/mindustry/entities/GroupDefs.java` and the generated
//! `mindustry.gen.Groups`. Membership for a def is computed once from the exact
//! `EntityProcess.java:273` rule (plan 05 §3.6):
//!
//! ```text
//! group applies ⇔ group.required ⊆ def.components
//!                 ∧ def.components ∩ group.exclude = ∅
//!                 ∧ group.name ∉ def.exclude_groups
//! ```

use bevy_ecs::entity::Entity;

use super::group::EntityGroup;

/// The ten Mindustry entity groups (`GroupDefs.java:7-16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupKind {
    /// `Groups.all` — everything except units/buildings/bullets/players/effects/power graph.
    All = 0,
    /// `Groups.unit`.
    Unit,
    /// `Groups.build`.
    Build,
    /// `Groups.bullet`.
    Bullet,
    /// `Groups.player`.
    Player,
    /// `Groups.effect`.
    Effect,
    /// `Groups.weather`.
    Weather,
    /// `Groups.powerGraph`.
    PowerGraph,
    /// `Groups.sync`.
    Sync,
    /// `Groups.draw`.
    Draw,
}

/// Every group in `GroupDefs.java` order.
pub const ALL_GROUPS: [GroupKind; 10] = [
    GroupKind::All,
    GroupKind::Unit,
    GroupKind::Build,
    GroupKind::Bullet,
    GroupKind::Player,
    GroupKind::Effect,
    GroupKind::Weather,
    GroupKind::PowerGraph,
    GroupKind::Sync,
    GroupKind::Draw,
];

impl GroupKind {
    /// Bit index used by [`GroupMask`].
    pub const fn index(self) -> u16 {
        self as u16
    }

    /// Parity name (matches the Java field name).
    pub const fn name(self) -> &'static str {
        match self {
            GroupKind::All => "all",
            GroupKind::Unit => "unit",
            GroupKind::Build => "build",
            GroupKind::Bullet => "bullet",
            GroupKind::Player => "player",
            GroupKind::Effect => "effect",
            GroupKind::Weather => "weather",
            GroupKind::PowerGraph => "powerGraph",
            GroupKind::Sync => "sync",
            GroupKind::Draw => "draw",
        }
    }

    /// Looks up a group by parity name.
    pub fn by_name(name: &str) -> Option<GroupKind> {
        ALL_GROUPS.into_iter().find(|kind| kind.name() == name)
    }
}

/// Compact bit set of group memberships stored on every [`super::meta::EntityDef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GroupMask(pub u16);

impl GroupMask {
    /// Empty mask.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Single-group mask.
    pub const fn of(kind: GroupKind) -> Self {
        Self(1 << kind.index())
    }

    /// Bitwise union.
    pub const fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether `kind` is a member.
    pub const fn contains(self, kind: GroupKind) -> bool {
        self.0 & (1 << kind.index()) != 0
    }

    /// Sorted member names (deterministic iteration for dumps/goldens).
    pub fn names(self) -> Vec<&'static str> {
        ALL_GROUPS
            .into_iter()
            .filter(|kind| self.contains(*kind))
            .map(GroupKind::name)
            .collect()
    }
}

/// One group definition (`GroupDefs.java`).
#[derive(Debug, Clone, Copy)]
pub struct GroupDef {
    /// Group kind.
    pub kind: GroupKind,
    /// Components that must all be present (`@Component` set / `import`).
    pub required: &'static [&'static str],
    /// Components that disqualify a def.
    pub exclude: &'static [&'static str],
    /// Def-level `excludeGroups` that disqualify a def.
    pub exclude_groups: &'static [&'static str],
    /// Whether the group carries an integer id map.
    pub mapping: bool,
    /// Whether the group carries a (plan 10/11) spatial index.
    pub spatial: bool,
}

/// Canonical group table (`GroupDefs.java:7-16` + `BuildingComp` `excludeGroups`).
pub static GROUP_DEFS: [GroupDef; 10] = [
    GroupDef {
        kind: GroupKind::All,
        required: &[],
        // Buildings carry `excludeGroups = {"all"}` and units/players/bullets/
        // effects/power updaters are updated in their own slots, so `all` holds
        // fires/puddles/decals/world labels/weather states only (plan 05 §3.4 #6).
        exclude: &[
            "Unit",
            "Building",
            "Bullet",
            "Player",
            "EffectState",
            "PowerGraphUpdater",
        ],
        exclude_groups: &[],
        mapping: false,
        spatial: false,
    },
    GroupDef {
        kind: GroupKind::Unit,
        required: &["Unit"],
        exclude: &[],
        exclude_groups: &[],
        mapping: true,
        spatial: true,
    },
    GroupDef {
        kind: GroupKind::Build,
        required: &["Building"],
        exclude: &[],
        exclude_groups: &[],
        mapping: true,
        spatial: false,
    },
    GroupDef {
        kind: GroupKind::Bullet,
        required: &["Bullet"],
        exclude: &[],
        exclude_groups: &[],
        mapping: false,
        spatial: true,
    },
    GroupDef {
        kind: GroupKind::Player,
        required: &["Player"],
        exclude: &[],
        exclude_groups: &[],
        mapping: true,
        spatial: false,
    },
    GroupDef {
        kind: GroupKind::Effect,
        required: &["EffectState"],
        exclude: &[],
        exclude_groups: &[],
        mapping: false,
        spatial: false,
    },
    GroupDef {
        kind: GroupKind::Weather,
        required: &["WeatherState"],
        exclude: &[],
        exclude_groups: &[],
        mapping: false,
        spatial: false,
    },
    GroupDef {
        kind: GroupKind::PowerGraph,
        required: &["PowerGraphUpdater"],
        exclude: &[],
        exclude_groups: &[],
        mapping: false,
        spatial: false,
    },
    GroupDef {
        kind: GroupKind::Sync,
        required: &["BaseEntity"],
        exclude: &[],
        exclude_groups: &[],
        mapping: true,
        spatial: false,
    },
    GroupDef {
        kind: GroupKind::Draw,
        required: &["Draw"],
        exclude: &[],
        exclude_groups: &[],
        mapping: false,
        spatial: false,
    },
];

/// Computes the membership mask for a def.
///
/// `components` are the def's component names, `exclude_groups` its def-level
/// `excludeGroups` list (e.g. `["all"]` on buildings).
pub fn compute_group_mask(components: &[&str], exclude_groups: &[&str]) -> GroupMask {
    let mut mask = GroupMask::empty();
    for def in &GROUP_DEFS {
        let has = |name: &str| components.contains(&name);
        let required_ok = def.required.iter().all(|name| has(name));
        let excluded = def.exclude.iter().any(|name| has(name));
        let def_excluded = exclude_groups.contains(&def.kind.name());
        if required_ok && !excluded && !def_excluded {
            mask = mask.or(GroupMask::of(def.kind));
        }
    }
    mask
}

/// The live `Groups` resource (port of generated `mindustry.gen.Groups`).
#[derive(Debug)]
pub struct Groups {
    /// `Groups.all`.
    pub all: EntityGroup,
    /// `Groups.unit`.
    pub unit: EntityGroup,
    /// `Groups.build`.
    pub build: EntityGroup,
    /// `Groups.bullet`.
    pub bullet: EntityGroup,
    /// `Groups.player`.
    pub player: EntityGroup,
    /// `Groups.effect`.
    pub effect: EntityGroup,
    /// `Groups.weather`.
    pub weather: EntityGroup,
    /// `Groups.powerGraph`.
    pub power_graph: EntityGroup,
    /// `Groups.sync`.
    pub sync: EntityGroup,
    /// `Groups.draw`.
    pub draw: EntityGroup,
}

impl Default for Groups {
    fn default() -> Self {
        Self::new()
    }
}

impl Groups {
    /// Creates all ten groups with the canonical mapping/spatial flags.
    pub fn new() -> Self {
        let make = |kind: GroupKind| {
            let def = GROUP_DEFS
                .iter()
                .find(|def| def.kind == kind)
                .copied()
                .unwrap_or(GroupDef {
                    kind,
                    required: &[],
                    exclude: &[],
                    exclude_groups: &[],
                    mapping: false,
                    spatial: false,
                });
            EntityGroup::new(def.mapping, def.spatial)
        };
        Self {
            all: make(GroupKind::All),
            unit: make(GroupKind::Unit),
            build: make(GroupKind::Build),
            bullet: make(GroupKind::Bullet),
            player: make(GroupKind::Player),
            effect: make(GroupKind::Effect),
            weather: make(GroupKind::Weather),
            power_graph: make(GroupKind::PowerGraph),
            sync: make(GroupKind::Sync),
            draw: make(GroupKind::Draw),
        }
    }

    /// Immutable reference to one group by kind.
    pub fn get(&self, kind: GroupKind) -> &EntityGroup {
        match kind {
            GroupKind::All => &self.all,
            GroupKind::Unit => &self.unit,
            GroupKind::Build => &self.build,
            GroupKind::Bullet => &self.bullet,
            GroupKind::Player => &self.player,
            GroupKind::Effect => &self.effect,
            GroupKind::Weather => &self.weather,
            GroupKind::PowerGraph => &self.power_graph,
            GroupKind::Sync => &self.sync,
            GroupKind::Draw => &self.draw,
        }
    }

    /// Mutable reference to one group by kind.
    pub fn get_mut(&mut self, kind: GroupKind) -> &mut EntityGroup {
        match kind {
            GroupKind::All => &mut self.all,
            GroupKind::Unit => &mut self.unit,
            GroupKind::Build => &mut self.build,
            GroupKind::Bullet => &mut self.bullet,
            GroupKind::Player => &mut self.player,
            GroupKind::Effect => &mut self.effect,
            GroupKind::Weather => &mut self.weather,
            GroupKind::PowerGraph => &mut self.power_graph,
            GroupKind::Sync => &mut self.sync,
            GroupKind::Draw => &mut self.draw,
        }
    }

    /// Adds `entity` (with its monotonic id) to every group set in `mask`.
    pub fn add(&mut self, entity: Entity, id: i32, mask: GroupMask) {
        for kind in ALL_GROUPS {
            if mask.contains(kind) {
                self.get_mut(kind).add_index(entity, id);
            }
        }
    }

    /// Removes `entity` from every group set it belongs to.
    pub fn remove(&mut self, entity: Entity, mask: GroupMask) {
        for kind in ALL_GROUPS {
            if mask.contains(kind) {
                self.get_mut(kind).remove(entity);
            }
        }
    }

    /// Clears every group.
    pub fn clear(&mut self) {
        for kind in ALL_GROUPS {
            self.get_mut(kind).clear();
        }
    }

    /// Counts per group in `ALL_GROUPS` order (state inspector / dumps).
    pub fn counts(&self) -> Vec<(&'static str, usize)> {
        ALL_GROUPS
            .into_iter()
            .map(|kind| (kind.name(), self.get(kind).len()))
            .collect()
    }

    /// Total live entities across `unit`/`build`/`bullet`/`player`/`effect`
    /// (excludes `all` double counting).
    pub fn total(&self) -> usize {
        ALL_GROUPS
            .into_iter()
            .filter(|kind| *kind != GroupKind::All && *kind != GroupKind::Sync)
            .map(|kind| self.get(kind).len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::world::World;

    use super::*;

    #[test]
    fn group_membership_rules() {
        // Unit: has Unit, not Building, not in all.
        let unit = compute_group_mask(
            &[
                "BaseEntity",
                "SimId",
                "DefId",
                "Pos",
                "Vel",
                "TeamComp",
                "Unit",
            ],
            &[],
        );
        assert!(unit.contains(GroupKind::Unit));
        assert!(unit.contains(GroupKind::Sync));
        assert!(!unit.contains(GroupKind::All));
        assert!(!unit.contains(GroupKind::Build));

        // Building: def-level excludeGroups={"all"} removes it from `all`.
        let build = compute_group_mask(
            &[
                "BaseEntity",
                "SimId",
                "DefId",
                "Pos",
                "TeamComp",
                "Building",
            ],
            &["all"],
        );
        assert!(build.contains(GroupKind::Build));
        assert!(!build.contains(GroupKind::All));
        assert!(build.contains(GroupKind::Sync));

        // A fire/effect entity with no core component is in `all`.
        let fire = compute_group_mask(&["BaseEntity", "SimId", "Pos"], &[]);
        assert!(fire.contains(GroupKind::All));
        assert!(!fire.contains(GroupKind::Unit));
    }

    #[test]
    fn groups_add_remove_are_consistent() {
        let mut world = World::new();
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let mut groups = Groups::new();
        let unit_mask = compute_group_mask(
            &[
                "BaseEntity",
                "SimId",
                "DefId",
                "Pos",
                "Vel",
                "TeamComp",
                "Unit",
            ],
            &[],
        );
        groups.add(a, 0, unit_mask);
        groups.add(b, 1, unit_mask);
        assert_eq!(groups.unit.len(), 2);
        assert_eq!(groups.unit.get_by_id(1), Some(b));
        groups.remove(a, unit_mask);
        assert_eq!(groups.unit.len(), 1);
        assert_eq!(groups.sync.len(), 1);
        groups.clear();
        assert_eq!(groups.total(), 0);
    }
}

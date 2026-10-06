// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `mindustry.game.Objectives` — tech-tree requirement classes (plan 12 M2).
//!
//! Ported from `core/src/mindustry/game/Objectives.java`. The classes are pure
//! predicates over a [`ObjectiveContext`] so they can be evaluated without the
//! campaign runtime (`Sector`/`Planet` state is plan 12 M3); `SectorComplete`
//! auto-insertion is performed by plan 02's `TechNode` builder.

use crate::content::tech::ObjectiveSpec;
use crate::content::{ContentRef, ContentRegistry, PlanetId, SectorId};

use super::tech_tree;
use super::universe::Campaign;

/// Sector runtime facts the objectives read (`Sector.save`/`isCaptured`/`hasBase`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SectorStatus {
    /// `Sector.save != null` (a save exists).
    pub has_save: bool,
    /// `Sector.isCaptured()`.
    pub captured: bool,
    /// `Sector.hasBase()`.
    pub has_base: bool,
}

/// State accessor for objective evaluation.
pub trait ObjectiveContext {
    /// `UnlockableContent.unlockedHost()`.
    fn is_unlocked(&self, content: ContentRef) -> bool;

    /// Campaign facts for a sector preset.
    fn sector_status(&self, sector: SectorId) -> SectorStatus;

    /// `planet.sectors.contains(Sector::hasBase)`.
    fn planet_has_base(&self, planet: PlanetId) -> bool;
}

/// One tech-tree objective (`Objectives.*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Objective {
    /// `Objectives.Research`.
    Research(ContentRef),
    /// `Objectives.Produce`.
    Produce(ContentRef),
    /// `Objectives.SectorComplete`.
    SectorComplete(SectorId),
    /// `Objectives.OnSector`.
    OnSector(SectorId),
    /// `Objectives.OnPlanet`.
    OnPlanet(PlanetId),
}

impl Objective {
    /// Converts a plan-02 materialized [`ObjectiveSpec`].
    pub fn from_spec(spec: ObjectiveSpec) -> Self {
        match spec {
            ObjectiveSpec::Research(content) => Objective::Research(content),
            ObjectiveSpec::Produce(content) => Objective::Produce(content),
            ObjectiveSpec::SectorComplete(sector) => Objective::SectorComplete(sector),
            ObjectiveSpec::OnSector(sector) => Objective::OnSector(sector),
            ObjectiveSpec::OnPlanet(planet) => Objective::OnPlanet(planet),
        }
    }

    /// `Objective.complete()`.
    pub fn complete(&self, ctx: &dyn ObjectiveContext) -> bool {
        match self {
            // Both Research and Produce test `unlockedHost()` upstream.
            Objective::Research(content) | Objective::Produce(content) => ctx.is_unlocked(*content),
            Objective::SectorComplete(sector) => {
                let status = ctx.sector_status(*sector);
                status.has_save && status.captured && status.has_base
            }
            Objective::OnSector(sector) => ctx.sector_status(*sector).has_base,
            Objective::OnPlanet(planet) => ctx.planet_has_base(*planet),
        }
    }

    /// Bundle key for the localized requirement text (`Objective.display()`).
    pub const fn display_key(&self) -> &'static str {
        match self {
            Objective::Research(_) => "requirement.research",
            Objective::Produce(_) => "requirement.produce",
            Objective::SectorComplete(_) => "requirement.capture",
            Objective::OnSector(_) => "requirement.onsector",
            Objective::OnPlanet(_) => "requirement.onplanet",
        }
    }

    /// Whether the objective only evaluates in a campaign (has sector/planet
    /// state); research/produce objectives are always evaluable.
    pub const fn requires_campaign(self) -> bool {
        matches!(
            self,
            Objective::SectorComplete(_) | Objective::OnSector(_) | Objective::OnPlanet(_)
        )
    }
}

/// Whether all `objectives` are met.
pub fn all_complete(objectives: &[Objective], ctx: &dyn ObjectiveContext) -> bool {
    objectives.iter().all(|objective| objective.complete(ctx))
}

/// Live campaign [`ObjectiveContext`]: unlock state and sector/planet status
/// snapshotted from the registry + runtime campaign.
///
/// The context owns its data so callers can hold it while mutating the registry
/// (`tech_tree::spend`).
pub struct CampaignObjectiveContext {
    unlocked: std::collections::HashSet<u32>,
    sectors: std::collections::HashMap<u16, SectorStatus>,
    planets_with_base: std::collections::HashSet<u16>,
}

impl CampaignObjectiveContext {
    /// Snapshots the context from a registry + runtime campaign.
    pub fn new(registry: &ContentRegistry, campaign: &Campaign) -> Self {
        let unlocked = registry
            .tech()
            .nodes
            .iter()
            .filter_map(|node| node.content)
            .filter(|content| tech_tree::content_unlocked(registry, *content))
            .map(pack_content)
            .collect();
        let sectors = registry
            .sectors()
            .iter()
            .filter_map(|preset| {
                let record = campaign.sector(preset.planet, preset.sector)?;
                Some((
                    preset.id.raw(),
                    SectorStatus {
                        has_save: record.has_save(),
                        captured: record.is_captured(None),
                        has_base: record.has_base(),
                    },
                ))
            })
            .collect();
        let planets_with_base = campaign
            .planets
            .values()
            .filter(|planet| planet.sectors.iter().any(|sector| sector.has_base()))
            .map(|planet| planet.id.raw())
            .collect();
        Self {
            unlocked,
            sectors,
            planets_with_base,
        }
    }
}

/// Packs a content reference into a `u32` key (`type ordinal << 16 | id`).
fn pack_content(content: ContentRef) -> u32 {
    ((content.type_.ordinal() as u32) << 16) | content.id as u32
}

impl ObjectiveContext for CampaignObjectiveContext {
    fn is_unlocked(&self, content: ContentRef) -> bool {
        self.unlocked.contains(&pack_content(content))
    }

    fn sector_status(&self, sector: SectorId) -> SectorStatus {
        self.sectors.get(&sector.raw()).copied().unwrap_or_default()
    }

    fn planet_has_base(&self, planet: PlanetId) -> bool {
        self.planets_with_base.contains(&planet.raw())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeCtx {
        unlocked: Vec<ContentRef>,
        sectors: std::collections::BTreeMap<u16, SectorStatus>,
        planets_with_base: Vec<u16>,
    }

    impl ObjectiveContext for FakeCtx {
        fn is_unlocked(&self, content: ContentRef) -> bool {
            self.unlocked.contains(&content)
        }
        fn sector_status(&self, sector: SectorId) -> SectorStatus {
            self.sectors.get(&sector.raw()).copied().unwrap_or_default()
        }
        fn planet_has_base(&self, planet: PlanetId) -> bool {
            self.planets_with_base.contains(&planet.raw())
        }
    }

    #[test]
    fn research_and_produce_share_the_unlock_predicate() {
        let content = ContentRef::new(crate::content::ContentType::Block, 5);
        let mut ctx = FakeCtx::default();
        assert!(!Objective::Research(content).complete(&ctx));
        ctx.unlocked.push(content);
        assert!(Objective::Research(content).complete(&ctx));
        assert!(Objective::Produce(content).complete(&ctx));
    }

    #[test]
    fn sector_complete_requires_save_capture_and_base() {
        let sector = SectorId::new(3);
        let mut ctx = FakeCtx::default();
        let objective = Objective::SectorComplete(sector);
        assert!(!objective.complete(&ctx));
        ctx.sectors.insert(
            sector.raw(),
            SectorStatus {
                has_save: true,
                captured: true,
                has_base: false,
            },
        );
        assert!(!objective.complete(&ctx), "base missing");
        ctx.sectors.insert(
            sector.raw(),
            SectorStatus {
                has_save: true,
                captured: true,
                has_base: true,
            },
        );
        assert!(objective.complete(&ctx));
        // OnSector ignores save/captured.
        assert!(Objective::OnSector(sector).complete(&ctx));
    }

    #[test]
    fn on_planet_checks_any_base() {
        let planet = PlanetId::new(2);
        let mut ctx = FakeCtx::default();
        assert!(!Objective::OnPlanet(planet).complete(&ctx));
        ctx.planets_with_base.push(planet.raw());
        assert!(Objective::OnPlanet(planet).complete(&ctx));
    }

    #[test]
    fn spec_round_trip_and_keys() {
        let content = ContentRef::new(crate::content::ContentType::Item, 1);
        let sector = SectorId::new(4);
        let planet = PlanetId::new(1);
        let cases = [
            (ObjectiveSpec::Research(content), "requirement.research"),
            (ObjectiveSpec::Produce(content), "requirement.produce"),
            (ObjectiveSpec::SectorComplete(sector), "requirement.capture"),
            (ObjectiveSpec::OnSector(sector), "requirement.onsector"),
            (ObjectiveSpec::OnPlanet(planet), "requirement.onplanet"),
        ];
        for (spec, key) in cases {
            let objective = Objective::from_spec(spec);
            assert_eq!(objective.display_key(), key);
            assert_eq!(
                objective.requires_campaign(),
                !matches!(objective, Objective::Research(_) | Objective::Produce(_))
            );
        }
    }

    #[test]
    fn all_complete_folds() {
        let content = ContentRef::new(crate::content::ContentType::Block, 2);
        let mut ctx = FakeCtx::default();
        let objectives = [Objective::Research(content), Objective::Research(content)];
        assert!(!all_complete(&objectives, &ctx));
        ctx.unlocked.push(content);
        assert!(all_complete(&objectives, &ctx));
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Controller selection glue for the live unit AI dispatch.
//! Upstream: core/src/mindustry/ai/types/BuilderAI.java (fallback)
//! core/src/mindustry/ai/types/CommandAI.java (init)

use super::*;

pub(super) mod defender;
pub(super) mod flying;
pub(super) mod follow;
pub(super) mod ground;
pub(super) mod hug;

/// `BuilderAI.fallback`/`useFallback` (BuilderAI.java:264-278): wave-team
/// builders without the `rtsAi` rule fight (GroundAI/FlyingAI) instead of
/// building.
pub(super) fn builder_falls_back(world: &World, team: u8) -> bool {
    let Some(campaign) = world.get_resource::<CampaignRuntime>() else {
        return false;
    };
    let rules = &campaign.session.rules;
    rules.waves && team == campaign.session.wave_team() && !rules.team_rule(team).rts_ai
}

/// The `CommandAI` command controller for `entity` (`CommandAI.init`:
/// `command` or the type's `defaultCommand`, CommandAI.java:96-101).
pub(super) fn resolve_command_kind(
    world: &World,
    content: &ContentRegistry,
    entity: Entity,
    slot: &ControllerSlot,
) -> Option<crate::content::registries::commands::ControllerKind> {
    let command = slot
        .command
        .or_else(|| unit_def(world, content, entity).and_then(|def| def.default_command))?;
    content.unit_command(command).map(|def| def.controller)
}

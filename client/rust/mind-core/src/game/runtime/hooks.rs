// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign hook queues: content unlocks and the deferred core/production
//! deltas the campaign tick folds into the played sector's `SectorInfo`.
//! Upstream: core/src/mindustry/world/blocks/storage/CoreBlock.java (campaign
//! hooks), core/src/mindustry/game/EventType.java

use super::*;
use crate::content::{ItemId, LiquidId, UnitTypeId};
use crate::event::EventContent;
use std::sync::{Arc, Mutex};

/// Content unlock queued by the production/liquid paths (H04 T2), resolved
/// host-side into the tech tree (`UnlockableContent.unlock()` ->
/// `UnlockEvent` -> `Control.java:160-174`). `Unit` is pushed by H01 T3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnlockRef {
    /// An item was produced/dumped.
    Item(ItemId),
    /// A liquid was dumped.
    Liquid(LiquidId),
    /// A unit was produced (`UnitTypeId`; H01 T3 pushes this).
    Unit(UnitTypeId),
}

/// Shared campaign hook state: the live session snapshot plus the deferred
/// `SectorInfo.handleCoreItem`/`handleProduction` deltas queued by the core and
/// miner paths (consumed by [`campaign_tick_system`]).
#[derive(Debug)]
pub struct CampaignHookQueues {
    /// `state.isCampaign()` snapshot (refreshed by the campaign tick).
    pub is_campaign: bool,
    /// `state.rules.defaultTeam` snapshot.
    pub default_team: u8,
    /// `Rules.coreIncinerates` snapshot.
    pub core_incinerates: bool,
    /// `Rules.allowCoreUnloaders` snapshot.
    pub allow_core_unloaders: bool,
    /// Pending `handleCoreItem` deltas `(item, amount)`.
    pub core_deltas: Vec<(ItemId, i32)>,
    /// Pending `handleProduction` deltas `(item, amount)`.
    pub production_deltas: Vec<(ItemId, i32)>,
    /// Pending content unlocks (`produced()`/`dumpLiquid()`).
    pub unlocks: Vec<UnlockRef>,
    /// Pending `SectorInfo.handleItemImport` deltas `(item, amount)` queued by
    /// landing pads (H04 T7).
    pub imports: Vec<(ItemId, i32)>,
}

impl Default for CampaignHookQueues {
    fn default() -> Self {
        // Java `Rules.java:225` (`coreIncinerates = true`) and `:135`
        // (`allowCoreUnloaders = true`) are the field defaults.
        Self {
            is_campaign: false,
            default_team: 0,
            core_incinerates: true,
            allow_core_unloaders: true,
            core_deltas: Vec::new(),
            production_deltas: Vec::new(),
            unlocks: Vec::new(),
            imports: Vec::new(),
        }
    }
}

/// `CoreCampaignHooks` impl for a live campaign session: queues the deltas the
/// campaign tick folds into the played sector's `SectorInfo` (plan 12 half of
/// `CoreBlock.handleItem` -> `SectorInfo.handleCoreItem`).
#[derive(Debug, Clone)]
pub struct CampaignCoreHooks {
    queues: Arc<Mutex<CampaignHookQueues>>,
}

impl crate::world::blocks::storage::CoreCampaignHooks for CampaignCoreHooks {
    fn is_campaign(&self) -> bool {
        self.queues
            .lock()
            .map(|queues| queues.is_campaign)
            .unwrap_or(false)
    }

    fn default_team(&self) -> u8 {
        self.queues
            .lock()
            .map(|queues| queues.default_team)
            .unwrap_or(0)
    }

    fn core_incinerates(&self) -> bool {
        self.queues
            .lock()
            .map(|queues| queues.core_incinerates)
            .unwrap_or(true)
    }

    fn allow_core_unloaders(&self) -> bool {
        self.queues
            .lock()
            .map(|queues| queues.allow_core_unloaders)
            .unwrap_or(true)
    }

    fn handle_core_item(&self, item: ItemId, amount: i32) {
        if let Ok(mut queues) = self.queues.lock() {
            queues.core_deltas.push((item, amount));
        }
    }
}

/// The shared queue handle installed beside the campaign core hooks. The
/// campaign tick, the core acceptance path and the miner all reach the same
/// `Arc` through this resource / the hooks impl.
#[derive(Resource, Clone)]
pub struct CampaignCoreQueues(pub Arc<Mutex<CampaignHookQueues>>);

/// Installs the campaign core hooks + queue handle (idempotent: the first
/// install owns the shared handle; later calls keep it).
pub fn install_campaign_core_hooks(world: &mut World) -> Arc<Mutex<CampaignHookQueues>> {
    if let Some(existing) = world.get_resource::<CampaignCoreQueues>() {
        return existing.0.clone();
    }
    let queues = Arc::new(Mutex::new(CampaignHookQueues::default()));
    world.insert_resource(crate::world::blocks::storage::CoreHooks(Arc::new(
        CampaignCoreHooks {
            queues: queues.clone(),
        },
    )));
    world.insert_resource(CampaignCoreQueues(queues.clone()));
    queues
}

/// Clears the shared session snapshot and any pending deltas (menu / runtime
/// removal): the hooks then answer `is_campaign: false` and drop new deltas.
pub fn reset_campaign_core_hooks(world: &mut World) {
    if let Some(queues) = world.get_resource::<CampaignCoreQueues>()
        && let Ok(mut state) = queues.0.lock()
    {
        state.is_campaign = false;
        state.core_incinerates = true;
        state.allow_core_unloaders = true;
        state.core_deltas.clear();
        state.production_deltas.clear();
        state.unlocks.clear();
        state.imports.clear();
    }
}

/// Dedupe key for [`UnlockRef`] (content kind, dense id).
pub(super) fn unlock_sort_key(unlock: &UnlockRef) -> (u8, u16) {
    match unlock {
        UnlockRef::Item(id) => (0, id.raw()),
        UnlockRef::Liquid(id) => (1, id.raw()),
        UnlockRef::Unit(id) => (2, id.raw()),
    }
}

/// [`UnlockRef`] -> the typed event row's content identity (`EventType` wire
/// payloads carry the content name; the host resolves it through
/// `ContentRegistry` when it encodes the row).
pub(super) fn event_content(unlock: UnlockRef) -> EventContent {
    match unlock {
        UnlockRef::Item(id) => EventContent::Item(id.raw()),
        UnlockRef::Liquid(id) => EventContent::Liquid(id.raw()),
        UnlockRef::Unit(id) => EventContent::Unit(id.raw()),
    }
}

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Host/runtime seam: the sim-host lookup, the `CampaignRuntime` mirror and the
//! settings/materialisation helpers shared by the play flows.
//! Upstream: none (port-only; facade seam between the host and CampaignRuntime)

use super::*;

impl MindCampaign {
    /// The wired `MindSimHost` that owns the live sim/runtime.
    pub(super) fn host(&self) -> Option<Gd<MindSimHost>> {
        wiring::optional(&self.sim_host)
    }

    /// `Core.scene.hasDialog()` for the `MindUi` dialog stack.
    pub(super) fn ui_has_dialog(&self) -> bool {
        let Some(mut ui) = self.autoload("MindUi") else {
            return false;
        };
        if !ui.has_method("has_dialog") {
            return false;
        }
        ui.call("has_dialog", &[]).try_to::<bool>().unwrap_or(false)
    }

    /// Persists campaign/universe/unlock state to `user://config/settings.bin`.
    ///
    /// Mirrors the live runtime campaign first: while a sector runs the runtime
    /// copy is authoritative, so writing the facade's last-mirrored copy would
    /// reset the turn counter and clock (Java has one object graph:
    /// `Vars.universe`/`planet.sectors`; `Universe.save()` `Universe.java:293`).
    pub(super) fn persist(&mut self) {
        self.mirror_runtime_campaign();
        if let Some(campaign) = self.campaign.as_mut()
            && let Err(error) = settings_hub::with_settings(|settings| campaign.save_all(settings))
        {
            log::warn!("campaign save_all failed: {error}");
        }
        // `GameService.storeStats` half: the shared tracker's stats and
        // achievement flags land in the same settings store, then flush.
        self.persist_service();
        self.flush_settings("campaign settings flush failed");
    }

    /// Runs `f` on THE live campaign: the runtime's copy while a sector runs
    /// (then mirrors it into the facade), else the facade. Java has one
    /// object graph (`Vars.universe` / `planet.sectors`); the facade is a view
    /// that catches up, never a second copy (H04 T1).
    ///
    /// `f` runs with the runtime taken out of the host, so it must not call
    /// back into Godot (H-environment §3: no held `bind_mut` across calls).
    /// It receives the one shared settings store (`Core.settings`, PAR-0164);
    /// the closure must not call back into the hub (the lock is not re-entrant).
    pub(super) fn with_campaign_mut<R>(
        &mut self,
        f: impl FnOnce(&mut Campaign, &mut SettingsStore) -> R,
    ) -> Option<R> {
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                let out =
                    settings_hub::with_settings(|settings| f(&mut runtime.campaign, settings));
                self.campaign = Some(runtime.campaign.clone());
                host.bind_mut().put_campaign_runtime(runtime);
                return Some(out);
            }
        }
        let mut campaign = self.campaign.take()?;
        let out = settings_hub::with_settings(|settings| f(&mut campaign, settings));
        self.campaign = Some(campaign);
        Some(out)
    }

    /// Copies the runtime campaign + service tracker into the facade (no-op
    /// without a runtime). Facade -> runtime copies never happen (T1: the
    /// runtime stays authoritative until it is dropped).
    pub(super) fn mirror_runtime_campaign(&mut self) {
        let _ = with_host_runtime(self.host(), |runtime| {
            self.campaign = Some(runtime.campaign.clone());
            // The runtime owns the live `Vars.service`-equivalent tracker
            // (GAPS #68); keep the facade's copy current for the persist path
            // and the UI queries.
            self.service = runtime.service.clone();
        });
    }

    /// Refreshes the live runtime's unlocked-content cache from the registry
    /// after research/clear flows. It does **not** copy the facade campaign
    /// into the runtime: while a sector runs the runtime copy is authoritative
    /// (T1) and campaign mutations go through [`Self::with_campaign_mut`], so
    /// only the derived unlocked-content set is pushed.
    pub(super) fn push_campaign_to_runtime(&mut self) {
        let unlocked = self
            .registry
            .as_ref()
            .map(tech_tree::unlocked_content_names)
            .unwrap_or_default();
        let _ = with_host_runtime(self.host(), |runtime| {
            runtime.set_unlocked_content(unlocked);
        });
    }

    /// Flushes the shared `Core.settings` store to disk, warning with the
    /// caller's `ctx` text on failure (`settings_hub::save` + `log::warn!`).
    pub(super) fn flush_settings(&self, ctx: &str) {
        if let Err(error) = settings_hub::save(&self.paths) {
            log::warn!("{ctx}: {error}");
        }
    }

    /// Reads `f` on the live campaign: the runtime copy while a sector runs,
    /// else the facade copy (`None` when neither exists).
    pub(super) fn read_live_campaign<R>(&self, f: impl Fn(&Campaign) -> R) -> Option<R> {
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(runtime) = taken {
                let out = f(&runtime.campaign);
                host.bind_mut().put_campaign_runtime(runtime);
                return Some(out);
            }
        }
        self.campaign.as_ref().map(f)
    }

    /// Pushes the facade session rules into the live runtime session
    /// (`Call.setRules`); `runtime.campaign` stays as-is.
    pub(super) fn push_session_rules_to_runtime(&self) {
        let _ = with_host_runtime(self.host(), |runtime| {
            runtime.session.rules = self.session.rules.clone();
        });
    }

    /// `Control.java:160-165`: one `showUnlock()` toast per unlocked content
    /// (the per-content rule lives in [`Self::push_unlock_toast`]).
    pub(super) fn push_unlock_toasts(&self, registry: &ContentRegistry, list: &[ContentRef]) {
        for content in list {
            self.push_unlock_toast(registry, *content);
        }
    }

    /// Emits `block_unlocked` when `list` contains a block (`UnlockEvent` ->
    /// `PlacementFragment.java:92-96`). The `self.registry` borrow some callers
    /// hold ends before this call, so no host bind is held here.
    pub(super) fn emit_block_unlocked_if_any(&mut self, list: &[ContentRef]) {
        if list
            .iter()
            .any(|content| content.type_ == ContentType::Block)
        {
            let _ = self.base_mut().emit_signal("block_unlocked", &[]);
        }
    }
}

/// Runs `f` on the host's installed `CampaignRuntime`, putting it back before
/// returning. `None` without a host or without an installed runtime — the two
/// cases the replaced `take_campaign_runtime`/`put_campaign_runtime` pairs
/// collapsed to.
pub(super) fn with_host_runtime<R>(
    host: Option<Gd<MindSimHost>>,
    f: impl FnOnce(&mut CampaignRuntime) -> R,
) -> Option<R> {
    let mut host = host?;
    let mut runtime = host.bind_mut().take_campaign_runtime()?;
    let out = f(&mut runtime);
    host.bind_mut().put_campaign_runtime(runtime);
    Some(out)
}

/// `registry.planet_by_name(name)`: the `PlanetDef` for a content name.
pub(super) fn planet_def<'a>(registry: &'a ContentRegistry, name: &str) -> Option<&'a PlanetDef> {
    registry.planet_by_name(name)
}

/// `registry.planet_by_name(name).map(|planet| planet.id)`.
pub(super) fn planet_id_by_name(registry: &ContentRegistry, name: &str) -> Option<PlanetId> {
    registry.planet_by_name(name).map(|planet| planet.id)
}

/// The GDScript `i64` sector id as a `u16` grid id; `None` for a negative id.
/// Values above `u16::MAX` wrap exactly like the `as u16` cast it replaces.
pub(super) fn sector_arg(sector: i64) -> Option<u16> {
    if sector < 0 {
        None
    } else {
        Some(sector as u16)
    }
}

/// `Slot.delete()`'s filesystem half: the save file and its `-backup` sibling,
/// both errors ignored (a missing file counts as deleted).
pub(super) fn delete_save_and_backup(file: &std::path::Path) {
    let _ = NativeFs.delete(file);
    let _ = NativeFs.delete(&SaveIo::backup_file_for(file));
}

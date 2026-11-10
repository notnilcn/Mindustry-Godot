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
        let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") else {
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
        if let Err(error) = settings_hub::save(&self.paths) {
            log::warn!("campaign settings flush failed: {error}");
        }
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
        let Some(mut host) = self.host() else {
            return;
        };
        let taken = host.bind_mut().take_campaign_runtime();
        if let Some(runtime) = taken {
            self.campaign = Some(runtime.campaign.clone());
            // The runtime owns the live `Vars.service`-equivalent tracker
            // (GAPS #68); keep the facade's copy current for the persist path
            // and the UI queries.
            self.service = runtime.service.clone();
            host.bind_mut().put_campaign_runtime(runtime);
        }
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
            .map(collect_unlocked)
            .unwrap_or_default();
        if let Some(mut host) = self.host() {
            let taken = host.bind_mut().take_campaign_runtime();
            if let Some(mut runtime) = taken {
                runtime.set_unlocked_content(unlocked);
                host.bind_mut().put_campaign_runtime(runtime);
            }
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Logic messages and the join overlay refresh.
//! Upstream: core/src/mindustry/ui/fragments/HudFragment.java

use super::*;

impl MindUiRoot {
    // ---- logic messages / join overlay ------------------------------------

    /// World-processor messages (`message`/`localeprint`) drained from the sim
    /// host and routed through the same `MindUi` surfaces as Java's
    /// `FlushMessageI` (`announce`/`hudfrag.showToast`/`showInfoToast`); H06 T4.
    /// Bundle and map-locale resolution happen in core before the drain.
    pub(super) fn drain_logic_messages(&mut self) {
        if self
            .menu_group
            .as_ref()
            .is_some_and(|group| group.is_visible())
        {
            return;
        }
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        if !host.has_method("drain_logic_messages") {
            return;
        }
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let Ok(rows) = host.call("drain_logic_messages", &[]).try_to::<VarArray>() else {
            return;
        };
        for row in rows.iter_shared() {
            let Ok(row) = row.try_to::<VarDictionary>() else {
                continue;
            };
            let text = row
                .get("text")
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_default();
            let duration = row
                .get("duration")
                .and_then(|value| value.try_to_relaxed::<f32>().ok())
                .unwrap_or(0.0);
            let kind = row
                .get("type")
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_default();
            match kind.to_string().as_str() {
                // `LExecutor.java:1939-1942`: an announce is dropped while an
                // announcement is already up (`ui.hasAnnouncement()`).
                "announce" => {
                    if self.has_announcement() {
                        continue;
                    }
                    let _ = ui.call("announce", &[text.to_variant(), duration.to_variant()]);
                }
                // `LExecutor.java:1941`: a notify is dropped while a toast is on
                // screen (`ui.hudfrag.hasToast()`).
                "notify" => {
                    if self.has_toast() {
                        continue;
                    }
                    let _ = ui.call(
                        "toast",
                        &[text.to_variant(), GString::from("info").to_variant()],
                    );
                }
                // `LExecutor.java:1940`: `showInfoToast` is dropped while an
                // announcement is up (`ui.hasAnnouncement()`).
                "toast" => {
                    if self.has_announcement() {
                        continue;
                    }
                    if ui.has_method("show_info_toast") {
                        let _ = ui.call(
                            "show_info_toast",
                            &[text.to_variant(), duration.to_variant()],
                        );
                    } else {
                        let _ = ui.call("announce", &[text.to_variant(), duration.to_variant()]);
                    }
                }
                _ => {}
            }
        }
    }

    /// `MindUi.take_event_toasts`: the `EventType` toasts distilled from the
    /// relay's `my_match_events` rows (join/leave/kick/ban, world/game
    /// lifecycle, config). Each one enters the HUD slot queue through
    /// [`Self::on_toast`] (`HudFragment.scheduleToast`), so a burst of events
    /// spaces out over the 3.5 s slots instead of stacking.
    pub(super) fn drain_event_toasts(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("take_event_toasts") {
            return;
        }
        let Ok(rows) = ui.call("take_event_toasts", &[]).try_to::<VarArray>() else {
            return;
        };
        for row in rows.iter_shared() {
            let Ok(row) = row.try_to::<VarDictionary>() else {
                continue;
            };
            let text = row
                .get("text")
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            let icon = row
                .get("icon")
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_else(|| GString::from("ok"));
            self.on_toast(text, icon);
        }
    }

    /// `NetClient`: the loading fragment shows `@connecting` while a join loads
    /// the host snapshot; progress follows `MindNet.snapshot_progress()` (I08 T5).
    pub(super) fn refresh_join_loading(&mut self) {
        let Some(mut net) = kit::net() else {
            return;
        };
        let Some(mut loading) = self.loading_node() else {
            return;
        };
        let state = net
            .call("session_state", &[])
            .try_to::<GString>()
            .unwrap_or_default();
        let syncing = state == "loading" || state == "snapshot_sync";
        if syncing {
            let progress = if net.has_method("snapshot_progress") {
                net.call("snapshot_progress", &[])
                    .try_to_relaxed::<f32>()
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            let _ = loading.call(
                "set_progress",
                &[progress.to_variant(), kit::t("@connecting").to_variant()],
            );
            self.join_loading_shown = true;
        } else if self.join_loading_shown {
            self.join_loading_shown = false;
            let _ = loading.call("hide_loading", &[]);
        }
    }
}

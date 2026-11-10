// SPDX-License-Identifier: GPL-3.0-only

//! World snapshot publish/join, campaign/module identity values and session-state notification.
//! Upstream: core/src/mindustry/net/NetClient.java

use super::*;

impl MindNet {
    /// Feeds the lobby-wave `my_matches` view into the session: the host's
    /// match id is known from here (the game wave that carries `my_match` does
    /// not exist yet), and a pending [`Self::host_current_world`] start runs
    /// once the row reaches `Lobby` (I08 T4).
    pub(super) fn drain_my_matches(&mut self) {
        let Some(binder) = self.my_matches_binder.as_ref() else {
            return;
        };
        let mut newest: Option<RelayMatch> = None;
        for change in binder.drain() {
            match change {
                RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                    if row.status != MatchStatus::Ended
                        && newest
                            .as_ref()
                            .map(|current| row.match_id > current.match_id)
                            .unwrap_or(true)
                    {
                        newest = Some(row);
                    }
                }
                RowChange::Delete(_) => {}
            }
        }
        let Some(row) = newest else {
            return;
        };
        let local = self
            .connector
            .as_ref()
            .and_then(Connector::local_identity)
            .map(|identity| identity.identity);
        let Some(local) = local else {
            return;
        };
        let is_new = self.session.match_id() != Some(row.match_id);
        self.session.observe_match(&row, &local);
        if is_new {
            self.ensure_relay();
        }
        if self.pending_host_start
            && self.session.is_host()
            && self.session.match_id() == Some(row.match_id)
            && row.status == MatchStatus::Lobby
        {
            self.pending_host_start = false;
            if self.start_match(true) {
                log::info!(
                    "MindNet: hosting match {} started immediately",
                    row.match_id
                );
            }
            self.publish_world_snapshot();
        }
    }

    /// Publishes the live world as a dynamic snapshot so late joiners can load
    /// it (`NetClient`'s `WorldStream`; I08 T4/T5). Wraps
    /// `MindSimHost.encode_snapshot`'s save bytes in the `MGSN` header
    /// (`snapshot.rs`) and sends them through `publish_snapshot`, which the
    /// server splits into chunks.
    pub(super) fn publish_world_snapshot(&mut self) -> bool {
        let Some(hosted) = self.hosted.clone() else {
            return false;
        };
        let Some(match_id) = self.session.match_id() else {
            return false;
        };
        let (save_bytes, sim_tick, checksum) = {
            let Some(host) = self.sim_host.as_mut() else {
                return false;
            };
            let bytes = host.bind_mut().encode_snapshot().to_vec();
            let tick = host.bind().get_tick().max(0) as u64;
            let checksum = host.bind().checksum_scoped_value();
            (bytes, tick, checksum)
        };
        if save_bytes.is_empty() {
            log::warn!("MindNet.publish_world_snapshot: save bytes empty; no snapshot");
            return false;
        }
        let command_id = self
            .runtime
            .as_ref()
            .map(|runtime| runtime.last_applied_command_id())
            .unwrap_or(0);
        let mut payload = SnapshotHeader {
            format: SNAPSHOT_FORMAT,
            checksum_version: mind_core::constants::CHECKSUM_VERSION,
            command_id,
            sim_tick,
            map_id: hosted.map_id.clone(),
            map_seed: hosted.map_seed,
            map_hash: 0,
            build_id: hosted.build_id.clone(),
            content_hash: hosted.content_hash,
            next_entity_id: 0,
            rules_json: hosted.rules_json.clone(),
            wave: 0,
            wavetime: 0.0,
            rng_sim: Vec::new(),
        }
        .to_bytes();
        payload.extend_from_slice(&save_bytes);
        let payload_len = payload.len();
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        match connector.publish_snapshot(
            match_id,
            SnapshotKind::Dynamic,
            SNAPSHOT_FORMAT,
            mind_core::constants::CHECKSUM_VERSION,
            command_id,
            sim_tick,
            checksum,
            &hosted.map_id,
            hosted.map_seed,
            0,
            &hosted.build_id,
            hosted.content_hash,
            payload,
        ) {
            Ok(()) => {
                log::info!(
                    "MindNet: snapshot published for match {match_id} ({payload_len} bytes)"
                );
                true
            }
            Err(error) => {
                log::warn!("MindNet.publish_snapshot failed: {error}");
                false
            }
        }
    }

    /// Reads a `GString`-returning method on the scene-wired `MindCampaign`.
    pub(super) fn campaign_call(&self, method: &str) -> Option<String> {
        let mut campaign = self.campaign.as_ref()?.clone();
        if !campaign.has_method(method) {
            return None;
        }
        campaign
            .call(method, &[])
            .try_to::<GString>()
            .ok()
            .map(|value| value.to_string())
    }

    /// `ContentRegistry::content_hash` from the live sim host (0 when absent).
    pub(super) fn content_hash(&self) -> u64 {
        self.sim_host
            .as_ref()
            .map(|host| host.bind().content_hash() as u64)
            .unwrap_or(0)
    }

    /// Enabled mod strings from `/root/MindMods` (`MindMods.mod_strings`).
    pub(super) fn mod_strings(&self) -> Vec<String> {
        let Some(mut mods) = self
            .base()
            .try_get_node_as::<crate::MindMods>("/root/MindMods")
        else {
            return Vec::new();
        };
        mods.bind_mut()
            .mod_strings()
            .as_slice()
            .iter()
            .map(|value| value.to_string())
            .collect()
    }

    /// Emits `session_changed` when the session state differs from the last
    /// emitted one. Every state mutation funnels through here, and `process()`
    /// calls it once per frame so row-driven transitions (match status,
    /// disconnect) are covered too.
    pub(super) fn notify_session_state(&mut self) {
        let state = self.session.state();
        if state != self.last_session_state {
            self.last_session_state = state;
            self.base_mut().emit_signal(
                "session_changed",
                &[GString::from(state.name()).to_variant()],
            );
        }
    }

    /// Shows a bundle-keyed error through `MindUi.show_error_key`
    /// (`Net.java:133` connection failures; `HostDialog.java:80-99` host
    /// failures). Falls back to the log when `/root/MindUi` is absent.
    pub(super) fn show_error(&self, key: &str, detail: &str) {
        log::warn!("MindNet: {key} {detail}");
        let Some(mut ui) = self.base().try_get_node_as::<crate::MindUi>("/root/MindUi") else {
            return;
        };
        ui.bind_mut()
            .show_error_key(GString::from(key), GString::from(detail));
    }

    /// Tracks the newest snapshot metadata and progress (plan M4/M5). A
    /// pending join requests a fresh host snapshot and targets the newest
    /// available one (I08 T5).
    pub(super) fn drain_snapshots(&mut self) {
        let Some(binder) = self.snapshots_binder.as_ref() else {
            return;
        };
        let mut newest: Option<(u64, u16)> = None;
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change
                && newest.map(|(id, _)| row.snapshot_id > id).unwrap_or(true)
            {
                newest = Some((row.snapshot_id, row.chunk_count));
            }
        }
        let Some((id, chunks)) = newest else {
            return;
        };
        log::debug!("MindNet: snapshot {id} available ({chunks} chunks)");
        if !self.pending_join || self.session.is_host() {
            return;
        }
        // The join requested a fresh snapshot; this row is its metadata (or a
        // fresher republication). Track the newest one and wait for chunks.
        let newer_target = self
            .join_load
            .as_ref()
            .map(|load| load.target_snapshot_id < id)
            .unwrap_or(true);
        if !newer_target {
            return;
        }
        self.join_load = Some(JoinLoad {
            target_snapshot_id: id,
            expected_chunks: chunks,
            chunks: std::collections::BTreeMap::new(),
        });
        self.snapshot_progress = 0.0;
        log::info!("MindNet: loading snapshot {id} ({chunks} chunks)");
    }

    /// Collects the joiner's snapshot chunks and applies the reassembled blob
    /// once every chunk arrived (`snapshot_progress` tracks the fraction;
    /// I08 T5).
    pub(super) fn drain_snapshot_chunks(&mut self) {
        let Some(binder) = self.snapshot_chunks_binder.as_ref() else {
            return;
        };
        let mut rows = Vec::new();
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                rows.push((row.snapshot_id, row.chunk_index, row.data));
            }
        }
        if rows.is_empty() {
            return;
        }
        let blob = {
            let Some(load) = self.join_load.as_mut() else {
                return;
            };
            for (snapshot_id, index, data) in rows {
                if snapshot_id == load.target_snapshot_id {
                    load.chunks.entry(index).or_insert(data);
                }
            }
            let received = load.chunks.len() as u16;
            self.snapshot_progress = if load.expected_chunks == 0 {
                1.0
            } else {
                f32::from(received) / f32::from(load.expected_chunks)
            };
            if load.expected_chunks == 0 || received < load.expected_chunks {
                return;
            }
            let indexed: Vec<(u16, Vec<u8>)> = load
                .chunks
                .iter()
                .map(|(index, data)| (*index, data.clone()))
                .collect();
            mind_stdb::snapshot::reassemble(&indexed, load.expected_chunks as usize)
        };
        let Some(blob) = blob else {
            log::warn!("MindNet: snapshot chunks did not reassemble");
            return;
        };
        self.apply_join_snapshot(&blob);
    }

    /// The host answers every pending snapshot request with a fresh snapshot
    /// (`snapshot.rs` `my_match_snapshot_requests`; I08 T5).
    pub(super) fn drain_snapshot_requests(&mut self) {
        let Some(binder) = self.snapshot_requests_binder.as_ref() else {
            return;
        };
        let mut requested = false;
        for change in binder.drain() {
            if matches!(change, RowChange::Insert(_) | RowChange::Update { .. }) {
                requested = true;
            }
        }
        if requested && self.session.is_host() && self.session.match_id().is_some() {
            log::info!("MindNet: snapshot request received; publishing a fresh snapshot");
            self.publish_world_snapshot();
        }
    }

    /// Applies a reassembled host snapshot (I08 T5): writes the save under
    /// `user://mp/`, loads it (I06 T1 `load_map_world`), starts the session
    /// with the header rules, sets the snapshot tick, skips relayed commands
    /// the blob already includes and shows the world.
    pub(super) fn apply_join_snapshot(&mut self, blob: &[u8]) -> bool {
        let Ok(snapshot) = DynamicSnapshot::decode(blob) else {
            log::warn!("MindNet: snapshot decode failed");
            return false;
        };
        let header = snapshot.header;
        let Some(match_id) = self.session.match_id() else {
            return false;
        };
        let relative = format!("user://mp/join-{match_id}.msav");
        let path = std::path::PathBuf::from(
            ProjectSettings::singleton()
                .globalize_path(&relative)
                .to_string(),
        );
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            log::warn!("MindNet: cannot create {}", parent.display());
            return false;
        }
        if let Err(error) = std::fs::write(&path, &snapshot.body) {
            log::warn!("MindNet: snapshot save write failed: {error}");
            return false;
        }
        let loaded = self
            .sim_host
            .as_mut()
            .map(|host| {
                host.bind_mut()
                    .load_map_world(GString::from(relative.as_str()))
            })
            .unwrap_or(false);
        if !loaded {
            log::warn!("MindNet: snapshot world load failed");
            return false;
        }
        let started = self
            .campaign
            .as_mut()
            .map(|campaign| {
                campaign
                    .bind_mut()
                    .start_joined_world(GString::from(header.rules_json.as_str()))
            })
            .unwrap_or(false);
        if !started {
            log::warn!("MindNet: joined session start failed");
            return false;
        }
        if let Some(host) = self.sim_host.as_mut() {
            host.bind_mut().set_sim_tick(header.sim_tick as i64);
        }
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.skip_through(header.command_id);
        }
        // Java: the state leaves the menu when the world is live.
        if let Some(ui_root) = self.ui_root.as_mut() {
            ui_root.bind_mut().set_menu_visible(false);
        }
        self.pending_join = false;
        self.join_load = None;
        self.snapshot_progress = 1.0;
        self.session.exit_snapshot_sync();
        self.notify_session_state();
        log::info!(
            "MindNet: snapshot applied (tick {}, command {})",
            header.sim_tick,
            header.command_id
        );
        true
    }
}

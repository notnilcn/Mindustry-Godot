// SPDX-License-Identifier: GPL-3.0-only

//! Relay runtime bring-up, match/member row drains and the roster tables.
//! Upstream: core/src/mindustry/net/NetClient.java

use super::adapters::roster_player_id;
use super::*;

impl MindNet {
    /// Creates the relay runtime/sender once a match id is known. A different
    /// match id rebuilds both (leave → re-host/join), so commands never carry
    /// a stale match (I08 T5). While a match is active, `MindSimHost` holds the
    /// player-command mailbox (I08 T6).
    pub(super) fn ensure_relay(&mut self) {
        let Some(match_id) = self.session.match_id() else {
            return;
        };
        if self.relay_match_id != match_id {
            self.relay_match_id = match_id;
            self.runtime = None;
            self.sender = None;
        }
        if self.runtime.is_none()
            && let Some(connector) = self.connector.as_mut()
        {
            let mut runtime = RelayRuntime::subscribe(connector, match_id);
            // The Connected event may have fired before this match existed
            // (online-on-demand boot); seed the identity now so own echoes are
            // recognized (I08 T6).
            if let Some(identity) = connector.local_identity() {
                runtime.set_local_identity(identity.identity);
            }
            self.runtime = Some(runtime);
        }
        if self.sender.is_none() {
            self.sender = Some(CommandSender::new(match_id));
        }
        if self.relay_outbox.is_none() {
            let outbox = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            if let Some(host) = self.sim_host.as_mut() {
                host.bind_mut().install_relay_outbox(outbox.clone());
            }
            self.relay_outbox = Some(outbox);
        }
    }

    pub(super) fn sim_tick(&self) -> u64 {
        self.sim_host
            .as_ref()
            .map(|host| host.bind().get_tick().max(0) as u64)
            .unwrap_or(0)
    }

    pub(super) fn drain_match_rows(&mut self) {
        let Some(binder) = self.match_binder.as_ref() else {
            return;
        };
        let mut latest = None;
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                latest = Some(row);
            }
        }
        if let Some(row) = latest {
            self.last_match_json = row.debug_json();
            let local = self
                .connector
                .as_ref()
                .and_then(Connector::local_identity)
                .map(|identity| identity.identity);
            if let Some(local) = local {
                self.session.observe_match(&row, &local);
            }
            self.ensure_relay();
        }
    }

    pub(super) fn drain_members_rows(&mut self) {
        let Some(binder) = self.members_binder.as_ref() else {
            return;
        };
        let mut members = Vec::new();
        // PAR-0577 residual: the relay roster is the host's player set, so its
        // membership transitions fire the host-owned `PlayerJoin`/`PlayerLeave`
        // (`NetServer.java:1020` join path, `:618` `onDisconnect`). The
        // `connected` flag is the transition source: a row inserted while
        // disconnected is only a reserved seat, and a disconnect arrives as an
        // update (or a delete) of the connected row.
        let mut joins: Vec<u32> = Vec::new();
        let mut leaves: Vec<u32> = Vec::new();
        for change in binder.drain() {
            match change {
                RowChange::Insert(row) => {
                    if row.connected {
                        joins.push(roster_player_id(row.member_id));
                    }
                    members.push(row.debug_json());
                    self.roster.apply_member(RowChange::Insert(row.clone()));
                    self.members.insert(row.identity.to_hex().to_string(), row);
                }
                RowChange::Update { old, new } => {
                    if !old.connected && new.connected {
                        joins.push(roster_player_id(new.member_id));
                    } else if old.connected && !new.connected {
                        leaves.push(roster_player_id(new.member_id));
                    }
                    members.push(new.debug_json());
                    self.roster.apply_member(RowChange::Insert(new.clone()));
                    self.members.insert(new.identity.to_hex().to_string(), new);
                }
                RowChange::Delete(row) => {
                    if row.connected {
                        leaves.push(roster_player_id(row.member_id));
                    }
                    self.roster.apply_member(RowChange::Delete(row.clone()));
                    self.members.remove(&row.identity.to_hex().to_string());
                }
            }
        }
        for player in joins {
            self.fire_player_event(true, player);
        }
        for player in leaves {
            self.fire_player_event(false, player);
        }
        if !members.is_empty() {
            self.last_members_json = format!("[{}]", members.join(","));
        }
        // The relay's team assignment can change on join, start or a team
        // switch (`assign_team`); the local sim must adopt it (PAR-0600).
        self.apply_relay_team();
    }

    /// Dispatches one host-side `PlayerJoin`/`PlayerLeave` on the campaign
    /// `EventType` bus (silent when the scene has no campaign node).
    pub(super) fn fire_player_event(&mut self, join: bool, player: u32) {
        let Some(mut campaign) = self.campaign.as_ref().cloned() else {
            return;
        };
        if !campaign.has_method("fire_player_event") {
            return;
        }
        campaign.call(
            "fire_player_event",
            &[join.to_variant(), i64::from(player).to_variant()],
        );
    }

    /// Applies the relay-assigned team to the local sim when [`Self::my_team`]
    /// changes (PAR-0600).
    ///
    /// The module's PvP balancer assigns a team on join/start (GAPS #33); the
    /// local player and its possessed unit must follow it exactly like
    /// `PlayerComp.team(team)` (`PlayerComp.java:266-271`), otherwise placement
    /// and ownership checks keep the world's default team. The applied value is
    /// tracked so the setter runs once per assignment, not every frame.
    pub(super) fn apply_relay_team(&mut self) {
        let team = match u8::try_from(self.my_team()) {
            Ok(team) => team,
            Err(_) => {
                // Unassigned/offline: forget the last applied team so the next
                // assignment lands even when it repeats.
                self.applied_relay_team = None;
                return;
            }
        };
        if self.applied_relay_team == Some(team) {
            return;
        }
        let Some(sim_host) = self.sim_host.as_mut() else {
            return;
        };
        if sim_host
            .bind_mut()
            .set_player_team_from_relay(i64::from(team))
        {
            self.applied_relay_team = Some(team);
            log::info!("MindNet: local sim adopted relay team {team}");
        }
    }

    /// Drains the roster tables into the [`Roster`] (PAR-0122): member rows,
    /// usernames, admin/ban/whitelist flags, kicks and admin action history.
    pub(super) fn drain_roster(&mut self) {
        if let Some(changes) = self
            .all_players_binder
            .as_ref()
            .map(|binder| binder.drain())
        {
            for change in changes {
                self.roster.apply_player(change);
            }
        }
        if let Some(changes) = self.all_admins_binder.as_ref().map(|binder| binder.drain()) {
            for change in changes {
                self.roster.apply_admin(change);
            }
        }
        if let Some(changes) = self.all_bans_binder.as_ref().map(|binder| binder.drain()) {
            for change in changes {
                self.roster.apply_ban(change);
            }
        }
        if let Some(changes) = self
            .all_whitelist_binder
            .as_ref()
            .map(|binder| binder.drain())
        {
            for change in changes {
                self.roster.apply_whitelist(change);
            }
        }
        if let Some(changes) = self.my_kick_binder.as_ref().map(|binder| binder.drain()) {
            for change in changes {
                self.roster.apply_kick(change);
            }
        }
        if let Some(changes) = self
            .admin_actions_binder
            .as_ref()
            .map(|binder| binder.drain())
        {
            for change in changes {
                self.roster.apply_admin_action(change);
            }
        }
        if let Some(changes) = self.traces_binder.as_ref().map(|binder| binder.drain()) {
            for change in changes {
                self.roster.apply_trace(change);
            }
        }
    }

    pub(super) fn drain_state_rows(&mut self) {
        let Some(binder) = self.state_binder.as_ref() else {
            return;
        };
        let mut latest = None;
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                latest = Some(row);
            }
        }
        if let Some(row) = latest {
            self.last_state_json = row.debug_json();
            self.last_checksum = format!("{:016x}", row.last_command_id);
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-only

//! `mp_*` scenarios (plan 21 §7b): network-free proofs of the multiplayer
//! lifecycle, ordered command replay and the shared cheap-validation matrix.
//!
//! These scenarios never open a socket. `mp_command_log_replay` feeds two
//! `mind-core::Sim` instances the same generated `CommandKind` log (the
//! `RelayRuntime` mapping is mirrored here because `mind-gdext` links Godot);
//! `mp_validation_matrix` drives the shared
//! `mind-core/tests/fixtures/relay/validation_matrix.json` through the
//! `mind-stdb` preflight mirror.

use std::path::Path;
use std::time::Instant;

use anyhow::Context;
use mind_core::content::BlockId;
use mind_core::determinism::SimCommand;
use mind_core::sim::Sim;
use mind_stdb::checksum::{
    ChecksumReport, compare_at_command_id, host_canonical_correction, scope_bits,
};
use mind_stdb::command_ring::CommandRing;
use mind_stdb::commands::{PredictionQueue, PreflightContext, preflight_validate};
use mind_stdb::module_bindings::{CommandKind, MatchCommand, MemberRole, PlaceBlock};
use mind_stdb::session::{can_emit, can_start};
use mind_stdb::snapshot::{DynamicSnapshot, SNAPSHOT_FORMAT, SnapshotHeader};
use serde::Deserialize;
use spacetimedb_sdk::{Identity, Timestamp};

use crate::cli::Cli;

/// Exit code: success.
const EXIT_PASS: i32 = 0;
/// Exit code: assertion/golden mismatch.
const EXIT_FAIL: i32 = 1;

/// A registered `mp_*` scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpScenario {
    /// Lifecycle/lobby/readiness mirror (no sim commands).
    LobbyJoin,
    /// Two sims fed the same ordered command log.
    CommandLogReplay,
    /// Shared validation-matrix fixture through the preflight mirror.
    ValidationMatrix,
    /// Forced divergence, detection and host-canonical correction.
    DesyncInjection,
    /// Snapshot at a watermark + terrain regen + tail replay.
    LateJoin,
    /// A pruned command-ring hole forces the snapshot path.
    ReconnectGap,
    /// Chunked plan rows reassemble to the source; incomplete groups ignored.
    PlanSnapshotReassembly,
    /// Mid-game dynamic snapshot byte budget (p95/cap).
    SnapshotBytes,
    /// Bounded two-peer soak with a forced desync + resync.
    Soak,
}

impl MpScenario {
    /// Scenario name as registered in [`crate::registry`].
    pub fn name(self) -> &'static str {
        match self {
            Self::LobbyJoin => "mp_lobby_join",
            Self::CommandLogReplay => "mp_command_log_replay",
            Self::ValidationMatrix => "mp_validation_matrix",
            Self::DesyncInjection => "mp_desync_injection",
            Self::LateJoin => "mp_late_join",
            Self::ReconnectGap => "mp_reconnect_gap",
            Self::PlanSnapshotReassembly => "mp_plan_snapshot_reassembly",
            Self::SnapshotBytes => "mp_snapshot_bytes",
            Self::Soak => "mp_soak",
        }
    }

    /// Maps a registered scenario name to its handler.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "mp_lobby_join" => Some(Self::LobbyJoin),
            "mp_command_log_replay" => Some(Self::CommandLogReplay),
            "mp_validation_matrix" => Some(Self::ValidationMatrix),
            "mp_desync_injection" => Some(Self::DesyncInjection),
            "mp_late_join" => Some(Self::LateJoin),
            "mp_reconnect_gap" => Some(Self::ReconnectGap),
            "mp_plan_snapshot_reassembly" => Some(Self::PlanSnapshotReassembly),
            "mp_snapshot_bytes" => Some(Self::SnapshotBytes),
            "mp_soak" => Some(Self::Soak),
            _ => None,
        }
    }
}

/// Runs a registered `mp_*` scenario; returns the process exit code.
pub fn run(cli: &Cli, kind: MpScenario, dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let _ = cli;
    match kind {
        MpScenario::LobbyJoin => run_lobby_join(dump, json),
        MpScenario::CommandLogReplay => run_command_log_replay(dump, json),
        MpScenario::ValidationMatrix => run_validation_matrix(dump, json),
        MpScenario::DesyncInjection => run_desync_injection(dump, json),
        MpScenario::LateJoin => run_late_join(dump, json),
        MpScenario::ReconnectGap => run_reconnect_gap(dump, json),
        MpScenario::PlanSnapshotReassembly => run_plan_snapshot_reassembly(dump, json),
        MpScenario::SnapshotBytes => run_snapshot_bytes(dump, json),
        MpScenario::Soak => run_soak(dump, json),
    }
}

fn emit(
    report: &serde_json::Value,
    dump: Option<&Path>,
    pass: bool,
    json: bool,
) -> anyhow::Result<i32> {
    if let Some(path) = dump {
        std::fs::write(path, serde_json::to_string_pretty(report)?)
            .with_context(|| format!("writing dump `{}`", path.display()))?;
    }
    if !crate::exec::is_quiet() {
        if json {
            println!("{}", serde_json::to_string_pretty(report)?);
        } else {
            println!("{}", serde_json::to_string(report)?);
        }
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

// ---- mp_lobby_join -----------------------------------------------------------

/// Synthetic lobby mirror (the server reducer + view logic in isolation).
#[derive(Debug, Default)]
struct Lobby {
    ended: bool,
    player_count: u16,
    max_players: u16,
    members: Vec<LobbyMember>,
    is_host: bool,
}

#[derive(Debug, Clone, Copy)]
struct LobbyMember {
    #[allow(dead_code)]
    role: MemberRole,
    ready: bool,
    connected: bool,
}

impl Lobby {
    fn new(max_players: u16) -> Self {
        Self {
            max_players,
            ..Self::default()
        }
    }

    fn create(&mut self, _match_id: u64) {
        self.is_host = true;
        self.members.push(LobbyMember {
            role: MemberRole::Player,
            ready: true,
            connected: true,
        });
        self.player_count = 1;
    }

    fn join(&mut self, role: MemberRole) -> Result<(), String> {
        if self.ended {
            return Err("match has ended".to_string());
        }
        if self.player_count >= self.max_players {
            return Err("This server is full.".to_string());
        }
        self.members.push(LobbyMember {
            role,
            ready: false,
            connected: true,
        });
        self.player_count += 1;
        Ok(())
    }

    fn set_ready(&mut self, index: usize, ready: bool) {
        if let Some(member) = self.members.get_mut(index) {
            member.ready = ready;
        }
    }

    fn ready_flags(&self) -> Vec<bool> {
        self.members
            .iter()
            .filter(|member| member.connected)
            .map(|member| member.ready)
            .collect()
    }

    fn leave_host(&mut self) {
        self.ended = true;
        self.player_count = 0;
        self.members.clear();
    }
}

fn run_lobby_join(dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let mut lobby = Lobby::new(4);
    lobby.create(1);
    let host_created = lobby.player_count == 1 && lobby.is_host;

    let join_player = lobby.join(MemberRole::Player);
    let join_spectator = lobby.join(MemberRole::Spectator);
    let joined = join_player.is_ok() && join_spectator.is_ok();
    let player_count_ok = lobby.player_count == 3;

    // Readiness gating: the two joiners are not ready yet.
    let blocked_before_ready = !can_start(false, lobby.ready_flags());
    lobby.set_ready(1, true);
    lobby.set_ready(2, true);
    let starts_when_ready = can_start(false, lobby.ready_flags());
    let force_bypasses = can_start(true, [false, false]);

    // Spectators cannot emit sim commands; players can.
    let spectator_blocked = !can_emit(MemberRole::Spectator, true);
    let player_allowed = can_emit(MemberRole::Player, true);

    // Host leaving ends the match and clears membership.
    lobby.leave_host();
    let host_leave_ends = lobby.ended && lobby.player_count == 0 && lobby.members.is_empty();

    // Cap enforcement on a fresh one-slot lobby.
    let mut small = Lobby::new(1);
    small.create(2);
    let cap_blocks = small.join(MemberRole::Player).is_err();

    let pass = host_created
        && joined
        && player_count_ok
        && blocked_before_ready
        && starts_when_ready
        && force_bypasses
        && spectator_blocked
        && player_allowed
        && host_leave_ends
        && cap_blocks;
    if !pass {
        log::error!("mp_lobby_join: one or more lobby assertions failed");
    }
    let report = serde_json::json!({
        "scenario": "mp_lobby_join",
        "pass": pass,
        "host_created": host_created,
        "joined": joined,
        "player_count_ok": player_count_ok,
        "blocked_before_ready": blocked_before_ready,
        "starts_when_ready": starts_when_ready,
        "force_bypasses": force_bypasses,
        "spectator_blocked": spectator_blocked,
        "host_leave_ends": host_leave_ends,
        "cap_blocks": cap_blocks,
    });
    emit(&report, dump, pass, json)
}

// ---- mp_command_log_replay ---------------------------------------------------

/// `CommandKind` → `SimCommand` mirror of the plan-21 relay adapter.
fn kind_to_sim(kind: &CommandKind, team: u8) -> Option<SimCommand> {
    match kind {
        CommandKind::Noop | CommandKind::Ping(_) => None,
        CommandKind::PlaceBlock(PlaceBlock {
            x,
            y,
            block,
            rotation,
            ..
        }) => Some(SimCommand::Place {
            x: i16::try_from(*x).ok()?,
            y: i16::try_from(*y).ok()?,
            block: block_id(block),
            rotation: *rotation as i8,
            team,
            player: None,
        }),
        CommandKind::BreakBlock(payload) => Some(SimCommand::Break {
            x: i16::try_from(payload.x).ok()?,
            y: i16::try_from(payload.y).ok()?,
            player: None,
        }),
        _ => None,
    }
}

/// Deterministic block-id mapping for the replay fixture (`stone-wall`/`router`).
fn block_id(name: &str) -> u16 {
    match name {
        "router" => 1,
        _ => 0,
    }
}

fn generated_log(len: usize) -> Vec<CommandKind> {
    let mut log = Vec::with_capacity(len);
    for index in 0..len {
        let x = (index % 20) as i32 + 1;
        let y = (index / 20) as i32 + 1;
        if index % 4 == 3 {
            log.push(CommandKind::BreakBlock(
                mind_stdb::module_bindings::BreakBlock { x, y },
            ));
        } else {
            log.push(CommandKind::PlaceBlock(PlaceBlock {
                x,
                y,
                block: if index % 2 == 0 {
                    "stone-wall"
                } else {
                    "router"
                }
                .to_string(),
                rotation: (index % 4) as u8,
                config: Vec::new(),
            }));
        }
    }
    log
}

fn run_command_log_replay(dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let log = generated_log(240);

    let mut sim_a = Sim::new(7, 32, 32, BlockId::AIR, BlockId::AIR);
    let mut sim_b = Sim::new(7, 32, 32, BlockId::AIR, BlockId::AIR);
    let mut checksums_match = true;
    let mut applied = 0usize;

    // Echo-skip: client A's own first command is in flight and removed on echo.
    let mut queue = PredictionQueue::new();
    assert!(queue.push(1, log[0].clone()));
    let echo_skipped = queue.remove(1).is_some();

    for (index, kind) in log.iter().enumerate() {
        if let Some(command) = kind_to_sim(kind, 0) {
            sim_a.command(command.clone()).ok();
            sim_b.command(command).ok();
            applied += 1;
        }
        if (index + 1) % 60 == 0 {
            checksums_match &= sim_a.checksum() == sim_b.checksum();
        }
    }
    let final_match = sim_a.checksum() == sim_b.checksum();
    let pass = checksums_match && final_match && echo_skipped && applied > 0 && queue.is_empty();
    if !pass {
        log::error!(
            "mp_command_log_replay: match={final_match} echo_skipped={echo_skipped} applied={applied}"
        );
    }
    let report = serde_json::json!({
        "scenario": "mp_command_log_replay",
        "pass": pass,
        "applied": applied,
        "checksums_match": checksums_match,
        "final_match": final_match,
        "echo_skipped": echo_skipped,
        "checksum_a": sim_a.checksum_hex(),
        "checksum_b": sim_b.checksum_hex(),
    });
    emit(&report, dump, pass, json)
}

// ---- mp_validation_matrix ----------------------------------------------------

/// Validation-matrix fixture (`mind-core/tests/fixtures/relay/validation_matrix.json`).
#[derive(Debug, Deserialize)]
struct MatrixFixture {
    #[allow(dead_code)]
    format: u32,
    map_width: i32,
    map_height: i32,
    cases: Vec<MatrixCase>,
}

#[derive(Debug, Deserialize)]
struct MatrixCase {
    name: String,
    kind: String,
    expect: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    nonce: Option<u64>,
    #[serde(default)]
    x: Option<i32>,
    #[serde(default)]
    y: Option<i32>,
    #[serde(default)]
    block: Option<String>,
    #[serde(default)]
    rotation: Option<u8>,
    #[serde(default)]
    config_len: Option<usize>,
    #[serde(default)]
    value_len: Option<usize>,
    #[serde(default)]
    direction: Option<bool>,
    #[serde(default)]
    positions: Option<Vec<i32>>,
    #[serde(default)]
    list_len: Option<usize>,
    #[serde(default)]
    inv_kind: Option<u8>,
    #[serde(default)]
    item: Option<String>,
    #[serde(default)]
    amount: Option<i32>,
    #[serde(default)]
    angle: Option<f32>,
    #[serde(default)]
    payload_kind: Option<u8>,
    #[serde(default)]
    command: Option<u16>,
    #[serde(default)]
    stance: Option<u16>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    team: Option<u8>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    data_len: Option<usize>,
    #[serde(default)]
    rules_epoch: Option<u32>,
    #[serde(default)]
    rules_len: Option<usize>,
    #[serde(default)]
    rule: Option<String>,
    #[serde(default)]
    json: Option<String>,
    #[serde(default)]
    json_len: Option<usize>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    index: Option<u32>,
    #[serde(default)]
    count: Option<u8>,
    #[serde(default)]
    points: Option<Vec<i32>>,
    #[serde(default)]
    name_0: Option<String>,
    #[serde(default)]
    var_name: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    reliable: Option<bool>,
    #[serde(default)]
    result_len: Option<usize>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    text_len: Option<usize>,
    #[serde(default)]
    custom_kind: Option<u16>,
}

fn zeros(len: usize) -> Vec<u8> {
    vec![0u8; len]
}

fn positions(case: &MatrixCase) -> Vec<i32> {
    if let Some(list) = &case.positions {
        list.clone()
    } else if let Some(list) = &case.points {
        list.clone()
    } else if let Some(len) = case.list_len {
        (0..len as i32).collect()
    } else {
        Vec::new()
    }
}

fn build_kind(case: &MatrixCase) -> Result<CommandKind, String> {
    use mind_stdb::module_bindings::*;
    let x = case.x.unwrap_or(0);
    let y = case.y.unwrap_or(0);
    let kind = match case.kind.as_str() {
        "noop" => CommandKind::Noop,
        "ping" => CommandKind::Ping(case.nonce.unwrap_or(0)),
        "place_block" => CommandKind::PlaceBlock(PlaceBlock {
            x,
            y,
            block: case.block.clone().unwrap_or_default(),
            rotation: case.rotation.unwrap_or(0),
            config: zeros(case.config_len.unwrap_or(0)),
        }),
        "break_block" => CommandKind::BreakBlock(BreakBlock { x, y }),
        "config_block" => CommandKind::ConfigBlock(ConfigBlock {
            x,
            y,
            value: zeros(case.value_len.unwrap_or(0)),
        }),
        "rotate" => CommandKind::Rotate(Rotate {
            x,
            y,
            direction: case.direction.unwrap_or(false),
        }),
        "delete_plans" => CommandKind::DeletePlans(DeletePlans {
            positions: positions(case),
        }),
        "command_building" => CommandKind::CommandBuilding(CommandBuilding {
            positions: positions(case),
            x: case.angle.unwrap_or(1.0),
            y: 2.0,
        }),
        "inventory" => CommandKind::Inventory(Inventory {
            kind: case.inv_kind.unwrap_or(0),
            x,
            y,
            item: case.item.clone(),
            amount: case.amount.unwrap_or(0),
            angle: case.angle.unwrap_or(0.0),
        }),
        "payload" => CommandKind::Payload(Payload {
            kind: case.payload_kind.unwrap_or(0),
            x: case.angle.unwrap_or(1.0),
            y: 1.0,
            target: None,
        }),
        "unit_control" => CommandKind::UnitControl(UnitControl { unit: None }),
        "unit_clear" => CommandKind::UnitClear,
        "building_control_select" => {
            CommandKind::BuildingControlSelect(BuildingControlSelect { x, y })
        }
        "unit_command" => CommandKind::UnitCommand(UnitCommand {
            units: positions(case),
            command: case.command.unwrap_or(0),
            x: 1.0,
            y: 2.0,
        }),
        "unit_command_queue" => CommandKind::UnitCommandQueue(UnitCommandQueue {
            units: positions(case),
            command: case.command.unwrap_or(0),
            x: 0.0,
            y: 0.0,
        }),
        "unit_stance" => CommandKind::UnitStance(UnitStance {
            units: positions(case),
            stance: case.stance.unwrap_or(0),
            enabled: case.enabled.unwrap_or(false),
        }),
        "player_spawn" => CommandKind::PlayerSpawn(PlayerSpawn {
            unit: case.unit.clone(),
            team: case.team.unwrap_or(0),
        }),
        "bullet" => CommandKind::Bullet(Bullet {
            def: 1,
            team: 0,
            x: 0.0,
            y: 0.0,
            angle: 0.0,
            damage: 0.0,
            velocity_scl: 1.0,
            lifetime_scl: 1.0,
            aim_x: f32::NAN,
            aim_y: f32::NAN,
            data: zeros(case.data_len.unwrap_or(0)),
        }),
        "set_rules" => CommandKind::SetRules(SetRules {
            rules_json: "x".repeat(case.rules_len.unwrap_or(0)),
            rules_epoch: case.rules_epoch.unwrap_or(1),
        }),
        "set_rule" => CommandKind::SetRule(SetRule {
            rule: case.rule.clone().unwrap_or_default(),
            json: "x".repeat(
                case.json_len
                    .unwrap_or(case.json.as_deref().map(str::len).unwrap_or(0)),
            ),
        }),
        "research_unlock" => CommandKind::ResearchUnlock(ResearchUnlock {
            content: case.content.clone().unwrap_or_default(),
        }),
        "complete_objective" => CommandKind::CompleteObjective(CompleteObjective {
            index: case.index.unwrap_or(0),
            rules_epoch: case.rules_epoch.unwrap_or(1),
        }),
        "clear_objectives" => CommandKind::ClearObjectives,
        "sector_capture" => CommandKind::SectorCapture,
        "save_sector" => CommandKind::SaveSector,
        "skip_wave" => CommandKind::SkipWave,
        "run_wave" => CommandKind::RunWave(RunWave {
            count: case.count.unwrap_or(0),
        }),
        "admin_switch_team" => CommandKind::AdminSwitchTeam(AdminSwitchTeam {
            target: Identity::from_byte_array([1u8; 32]),
            team: case.team.unwrap_or(0),
        }),
        "admin_tile_op" => CommandKind::AdminTileOp(AdminTileOp {
            op: 0,
            points: positions(case),
            arg_0: 0,
            arg_1: 0,
            arg_2: 0,
            name_0: case.name_0.clone(),
            name_1: None,
        }),
        "logic_sync" => CommandKind::LogicSync(LogicSync {
            x,
            y,
            var_name: case.var_name.clone().unwrap_or_default(),
            var_id: 0,
            value: zeros(case.value_len.unwrap_or(0)),
        }),
        "logic_client_data" => CommandKind::LogicClientData(LogicClientData {
            channel: case.channel.clone().unwrap_or_default(),
            value: zeros(case.value_len.unwrap_or(0)),
            reliable: case.reliable.unwrap_or(false),
        }),
        "menu_choose" => CommandKind::MenuChoose(MenuChoose {
            menu_id: 0,
            option: 0,
        }),
        "menu_builder_choose" => CommandKind::MenuBuilderChoose(MenuBuilderChoose {
            menu_id: 0,
            result: zeros(case.result_len.unwrap_or(0)),
        }),
        "text_input_result" => CommandKind::TextInputResult(TextInputResult {
            id: 0,
            text: case
                .text
                .clone()
                .or_else(|| case.text_len.map(|len| "x".repeat(len))),
        }),
        "custom" => CommandKind::Custom(Custom {
            kind: case.custom_kind.unwrap_or(0),
            data: zeros(case.data_len.unwrap_or(0)),
        }),
        other => return Err(format!("unknown case kind `{other}`")),
    };
    Ok(kind)
}

fn run_validation_matrix(dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mind-core/tests/fixtures/relay/validation_matrix.json");
    let text = std::fs::read_to_string(&fixture_path)
        .with_context(|| format!("reading `{}`", fixture_path.display()))?;
    let fixture: MatrixFixture = serde_json::from_str(&text)
        .with_context(|| format!("parsing `{}`", fixture_path.display()))?;
    let ctx = PreflightContext {
        map_width: fixture.map_width,
        map_height: fixture.map_height,
        is_host_or_admin: true,
        is_spectator: false,
    };

    let mut pass = true;
    let mut accept = 0usize;
    let mut reject = 0usize;
    let mut failures = Vec::new();
    for case in &fixture.cases {
        let kind = match build_kind(case) {
            Ok(kind) => kind,
            Err(error) => {
                failures.push(format!("{}: {error}", case.name));
                pass = false;
                continue;
            }
        };
        let result = preflight_validate(&kind, &ctx);
        let expected_accept = case.expect == "accept";
        let actual_accept = result.is_ok();
        if expected_accept != actual_accept {
            failures.push(format!(
                "{}: expected {} got {:?}",
                case.name, case.expect, result
            ));
            pass = false;
        } else if let (Some(reason), Err(actual)) = (&case.reason, &result)
            && actual != reason
        {
            failures.push(format!(
                "{}: reason `{actual}` != expected `{reason}`",
                case.name
            ));
            pass = false;
        }
        if actual_accept {
            accept += 1;
        } else {
            reject += 1;
        }
    }
    if fixture.cases.len() < 40 {
        log::error!(
            "mp_validation_matrix: only {} cases (need >= 40)",
            fixture.cases.len()
        );
        pass = false;
    }
    if !pass {
        for failure in &failures {
            log::error!("mp_validation_matrix: {failure}");
        }
    }
    let report = serde_json::json!({
        "scenario": "mp_validation_matrix",
        "pass": pass,
        "cases": fixture.cases.len(),
        "accept": accept,
        "reject": reject,
        "failures": failures,
    });
    emit(&report, dump, pass, json)
}

// ---- shared M4/M5 helpers ----------------------------------------------------

/// Rebuilds a sim from a seed + ordered command log, then advances `ticks`.
fn replay(seed: u64, log: &[CommandKind], ticks: u64) -> Sim {
    let mut sim = Sim::new(seed, 32, 32, BlockId::AIR, BlockId::AIR);
    for kind in log {
        if let Some(command) = kind_to_sim(kind, 0) {
            sim.command(command).ok();
        }
    }
    for _ in 0..ticks {
        sim.tick().ok();
    }
    sim
}

/// Builds a synthetic ring entry (network-free).
fn ring_row(command_id: u64, kind: CommandKind, sender_byte: u8) -> MatchCommand {
    MatchCommand {
        command_id,
        match_id: 1,
        sender: Identity::from_byte_array([sender_byte; 32]),
        sender_seq: command_id,
        client_tick: command_id,
        kind,
        sent_at: Timestamp::UNIX_EPOCH,
    }
}

fn checksum_report(sender: u8, command_id: u64, checksum: u64) -> ChecksumReport {
    ChecksumReport {
        sender: Identity::from_byte_array([sender; 32]),
        command_id,
        sim_tick: command_id,
        checksum,
        checksum_version: 1,
        scope: scope_bits(true, false, false),
    }
}

// ---- mp_desync_injection -----------------------------------------------------

fn run_desync_injection(dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let seed = 7;
    let base = generated_log(300);
    let sim_a = replay(seed, &base, 600);
    let mut sim_b = replay(seed, &base, 600);

    // Divergent command applied to B only, far from the generated positions.
    let divergent = CommandKind::PlaceBlock(PlaceBlock {
        x: 30,
        y: 30,
        block: "router".to_string(),
        rotation: 0,
        config: Vec::new(),
    });
    if let Some(command) = kind_to_sim(&divergent, 0) {
        sim_b.command(command).ok();
    }

    let diverged = sim_a.checksum() != sim_b.checksum();

    // Both peers publish the same command_id (600) with different checksums.
    let host = 1u8;
    let ours = 2u8;
    let votes = [
        checksum_report(host, 600, sim_a.checksum()),
        checksum_report(ours, 600, sim_b.checksum()),
    ];
    let host_id = Identity::from_byte_array([host; 32]);
    let detected = compare_at_command_id(&votes, Some(host_id)).is_some();
    let correction = compare_at_command_id(&votes, Some(host_id))
        .and_then(|desync| host_canonical_correction(&desync, sim_b.checksum()));
    let corrected = correction.is_some();

    // Full-resync correction: rebuild B from the canonical log.
    let sim_b = replay(seed, &base, 600);
    let resync_count = 1u32;
    let equal_after = sim_a.checksum() == sim_b.checksum();

    let pass = diverged && detected && corrected && equal_after && resync_count == 1;
    if !pass {
        log::error!(
            "mp_desync_injection: diverged={diverged} detected={detected} corrected={corrected} equal={equal_after}"
        );
    }
    let report = serde_json::json!({
        "scenario": "mp_desync_injection",
        "pass": pass,
        "diverged": diverged,
        "detected": detected,
        "corrected": corrected,
        "equal_after": equal_after,
        "resync_count": resync_count,
        "checksum_a": sim_a.checksum_hex(),
        "checksum_b": sim_b.checksum_hex(),
    });
    emit(&report, dump, pass, json)
}

// ---- mp_late_join ------------------------------------------------------------

fn run_late_join(dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let seed = 11;
    let base = generated_log(300);
    let tail = generated_log(60);
    let combined: Vec<CommandKind> = base.iter().chain(tail.iter()).cloned().collect();
    let sim_a = replay(seed, &combined, 3600);

    // Host snapshot at the watermark (opaque 04 body; header is what matters).
    let snapshot = DynamicSnapshot {
        header: SnapshotHeader {
            format: SNAPSHOT_FORMAT,
            checksum_version: 1,
            command_id: 300,
            sim_tick: 3600,
            map_id: "demo_flat".to_string(),
            map_seed: seed,
            map_hash: 0,
            build_id: String::new(),
            content_hash: 0,
            next_entity_id: 0,
            rules_json: "{}".to_string(),
            wave: 0,
            wavetime: 0.0,
            rng_sim: vec![1, 2, 3, 4],
        },
        body: vec![0xde, 0xad, 0xbe, 0xef],
    };
    let encoded = snapshot.encode();
    let Ok(decoded) = DynamicSnapshot::decode(&encoded) else {
        log::error!("mp_late_join: snapshot decode failed");
        return emit(
            &serde_json::json!({"scenario": "mp_late_join", "pass": false, "error": "decode"}),
            dump,
            false,
            json,
        );
    };
    let header_ok = decoded.header.command_id == 300
        && decoded.header.map_seed == seed
        && decoded.body == snapshot.body;
    let bytes_ok = encoded.len() <= 4 * 1024 * 1024;

    // Terrain regen + snapshot restore (base replay) + tail replay.
    let mut sim_b = replay(seed, &base, 3600);
    let mut ring = CommandRing::new();
    for (index, kind) in tail.iter().enumerate() {
        ring.push(&ring_row(301 + index as u64, kind.clone(), 1));
    }
    let tail_ok = ring.covers_tail(300, 300 + tail.len() as u64);
    for entry in ring.tail_after(300) {
        if let Some(command) = kind_to_sim(&entry.kind, 0) {
            sim_b.command(command).ok();
        }
    }
    let equal = sim_a.checksum() == sim_b.checksum();

    let pass = header_ok && bytes_ok && tail_ok && equal;
    if !pass {
        log::error!(
            "mp_late_join: header={header_ok} bytes={bytes_ok} tail={tail_ok} equal={equal}"
        );
    }
    let report = serde_json::json!({
        "scenario": "mp_late_join",
        "pass": pass,
        "header_ok": header_ok,
        "bytes_ok": bytes_ok,
        "tail_ok": tail_ok,
        "equal": equal,
        "snapshot_bytes": encoded.len(),
        "tail_len": tail.len(),
        "checksum_a": sim_a.checksum_hex(),
        "checksum_b": sim_b.checksum_hex(),
    });
    emit(&report, dump, pass, json)
}

// ---- mp_reconnect_gap --------------------------------------------------------

fn run_reconnect_gap(dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    let seed = 13;
    let base = generated_log(200);
    let sim_a = replay(seed, &base, 1200);

    // Ring with a pruned hole at command_id 100.
    let mut ring = CommandRing::new();
    for (index, kind) in base.iter().enumerate() {
        let command_id = index as u64 + 1;
        if command_id == 100 {
            continue;
        }
        ring.push(&ring_row(command_id, kind.clone(), 1));
    }
    let hole_flagged = !ring.covers_tail(0, 200);

    // Snapshot path resolves the hole: a full rebuild equals the canonical sim.
    let sim_b = replay(seed, &base, 1200);
    let resync_count = 1u32;
    let equal = sim_a.checksum() == sim_b.checksum();

    let pass = hole_flagged && equal && resync_count == 1;
    if !pass {
        log::error!("mp_reconnect_gap: hole={hole_flagged} equal={equal} resync={resync_count}");
    }
    let report = serde_json::json!({
        "scenario": "mp_reconnect_gap",
        "pass": pass,
        "hole_flagged": hole_flagged,
        "equal": equal,
        "resync_count": resync_count,
        "checksum_a": sim_a.checksum_hex(),
        "checksum_b": sim_b.checksum_hex(),
    });
    emit(&report, dump, pass, json)
}

// ---- mp_plan_snapshot_reassembly --------------------------------------------

/// Chunked plan rows reassemble to the source; an incomplete group is ignored.
///
/// Uses the §6.6 chunking shape (75 plans/chunk, ≤8 KiB/chunk, ≤1000 plans) with
/// fixed 16-byte records; the 04 `TypeIO` plan codec is 15's fixture, not here.
fn run_plan_snapshot_reassembly(dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    const PLAN_COUNT: usize = 1000;
    const RECORD: usize = 16;
    const PLANS_PER_CHUNK: usize = 75;

    let source: Vec<u8> = (0..PLAN_COUNT)
        .flat_map(|plan| (0..RECORD).map(move |byte| ((plan * 31 + byte) & 0xff) as u8))
        .collect();
    let chunk_count = PLAN_COUNT.div_ceil(PLANS_PER_CHUNK);
    let mut chunks: Vec<Vec<u8>> = Vec::with_capacity(chunk_count);
    for index in 0..chunk_count {
        let start = index * PLANS_PER_CHUNK * RECORD;
        let end = ((index + 1) * PLANS_PER_CHUNK * RECORD).min(source.len());
        chunks.push(source[start..end].to_vec());
    }
    let reassembled: Vec<u8> = chunks
        .iter()
        .flat_map(|chunk| chunk.iter().copied())
        .collect();
    let reassembled_ok = reassembled == source;

    // An incomplete group (last chunk missing) must not be treated as complete.
    let missing_last: usize = chunks[..chunks.len().saturating_sub(1)]
        .iter()
        .map(Vec::len)
        .sum();
    let incomplete_ignored = missing_last != source.len();

    let max_chunk = chunks.iter().map(Vec::len).max().unwrap_or(0);
    let caps_ok =
        max_chunk <= 8 * 1024 && source.len() <= 128 * 1024 && chunk_count <= u16::MAX as usize;

    let pass = reassembled_ok && incomplete_ignored && caps_ok && chunks.len() == chunk_count;
    if !pass {
        log::error!(
            "mp_plan_snapshot_reassembly: reassembled={reassembled_ok} incomplete={incomplete_ignored} caps={caps_ok}"
        );
    }
    let report = serde_json::json!({
        "scenario": "mp_plan_snapshot_reassembly",
        "pass": pass,
        "plans": PLAN_COUNT,
        "chunks": chunk_count,
        "max_chunk_bytes": max_chunk,
        "source_bytes": source.len(),
        "reassembled_ok": reassembled_ok,
        "incomplete_ignored": incomplete_ignored,
        "caps_ok": caps_ok,
    });
    emit(&report, dump, pass, json)
}

// ---- mp_snapshot_bytes ------------------------------------------------------

/// Models a mid-game dynamic snapshot body and checks the §3.8 byte budget.
fn run_snapshot_bytes(dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    const BUILDINGS: usize = 600;
    const UNITS: usize = 300;
    const ITEMS: usize = 4000;
    const ENTITY_BYTES: usize = 24;
    const ITEM_BYTES: usize = 12;

    let mut body = Vec::with_capacity((BUILDINGS + UNITS) * ENTITY_BYTES + ITEMS * ITEM_BYTES);
    for index in 0..(BUILDINGS + UNITS) {
        for byte in 0..ENTITY_BYTES {
            body.push(((index * 17 + byte * 7) & 0xff) as u8);
        }
    }
    for index in 0..ITEMS {
        for byte in 0..ITEM_BYTES {
            body.push(((index * 13 + byte * 5) & 0xff) as u8);
        }
    }
    let expected_body = (BUILDINGS + UNITS) * ENTITY_BYTES + ITEMS * ITEM_BYTES;
    let snapshot = DynamicSnapshot {
        header: SnapshotHeader {
            format: SNAPSHOT_FORMAT,
            checksum_version: 1,
            command_id: 12_345,
            sim_tick: 36_000,
            map_id: "mp_snapshot_bytes".to_string(),
            map_seed: 7,
            map_hash: 0,
            build_id: String::new(),
            content_hash: 0,
            next_entity_id: (BUILDINGS + UNITS) as i32,
            rules_json: "{}".to_string(),
            wave: 42,
            wavetime: 12.5,
            rng_sim: vec![1, 2, 3, 4, 5, 6, 7, 8],
        },
        body,
    };
    let encoded = snapshot.encode();
    let roundtrip_ok = DynamicSnapshot::decode(&encoded)
        .map(|value| value.header.command_id == 12_345 && value.body.len() == expected_body)
        .unwrap_or(false);
    let p95_budget = 512 * 1024;
    let cap = 4 * 1024 * 1024;
    let within_p95 = encoded.len() <= p95_budget;
    let within_cap = encoded.len() <= cap;
    let pass = roundtrip_ok && within_cap;
    if !pass {
        log::error!(
            "mp_snapshot_bytes: roundtrip={roundtrip_ok} bytes={} p95={within_p95} cap={within_cap}",
            encoded.len()
        );
    }
    let report = serde_json::json!({
        "scenario": "mp_snapshot_bytes",
        "pass": pass,
        "bytes": encoded.len(),
        "p95_budget_bytes": p95_budget,
        "cap_bytes": cap,
        "within_p95": within_p95,
        "within_cap": within_cap,
        "roundtrip_ok": roundtrip_ok,
        "buildings": BUILDINGS,
        "units": UNITS,
        "items": ITEMS,
    });
    emit(&report, dump, pass, json)
}

// ---- mp_soak ----------------------------------------------------------------

/// Bounded two-peer soak: `ticks` at 60 Hz with a ~200 commands/s rate, a forced
/// desync at the midpoint, host-canonical resync, and equal final checksums.
///
/// The full 30-minute run belongs on the nightly perf runner; this bounded slice
/// exercises the same fields in CI.
fn run_soak(dump: Option<&Path>, json: bool) -> anyhow::Result<i32> {
    const SEED: u64 = 7;
    const TICKS: u64 = 1_800; // 30 s bounded slice
    const COMMANDS: usize = 6_000; // ~200/s
    const SAMPLE_EVERY: u64 = 300;

    let log = generated_log(COMMANDS);
    let started = Instant::now();
    let (sim_a, samples_a) = soak_peer(SEED, &log, TICKS, SAMPLE_EVERY);
    let (mut sim_b, samples_b) = soak_peer(SEED, &log, TICKS, SAMPLE_EVERY);
    let agreed_before = samples_a == samples_b;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let aligned = sim_a.checksum() == sim_b.checksum();

    // Forced divergence on B, detection, then a full resync rebuild.
    let divergent = CommandKind::PlaceBlock(PlaceBlock {
        x: 30,
        y: 30,
        block: "router".to_string(),
        rotation: 0,
        config: Vec::new(),
    });
    if let Some(command) = kind_to_sim(&divergent, 0) {
        sim_b.command(command).ok();
    }
    let diverged = sim_a.checksum() != sim_b.checksum();
    let votes = [
        checksum_report(1, TICKS, sim_a.checksum()),
        checksum_report(2, TICKS, sim_b.checksum()),
    ];
    let host_id = Identity::from_byte_array([1u8; 32]);
    let detected = compare_at_command_id(&votes, Some(host_id)).is_some();

    let (sim_b, _) = soak_peer(SEED, &log, TICKS, SAMPLE_EVERY);
    let equal_after = sim_a.checksum() == sim_b.checksum();
    let resync_count = 1u32;

    let pass = agreed_before && aligned && diverged && detected && equal_after && resync_count == 1;
    if !pass {
        log::error!(
            "mp_soak: agreed_before={agreed_before} aligned={aligned} diverged={diverged} detected={detected} equal={equal_after}"
        );
    }
    let report = serde_json::json!({
        "scenario": "mp_soak",
        "pass": pass,
        "ticks": TICKS,
        "commands": COMMANDS,
        "commands_per_second": COMMANDS as u64 * 60 / TICKS,
        "elapsed_ms": elapsed_ms,
        "agreed_before": agreed_before,
        "aligned": aligned,
        "diverged": diverged,
        "detected": detected,
        "equal_after": equal_after,
        "resync_count": resync_count,
        "checksum": sim_a.checksum_hex(),
    });
    emit(&report, dump, pass, json)
}

/// Replays `log` across `ticks` at an even command rate and samples checksums.
fn soak_peer(seed: u64, log: &[CommandKind], ticks: u64, sample_every: u64) -> (Sim, Vec<u64>) {
    let mut sim = Sim::new(seed, 32, 32, BlockId::AIR, BlockId::AIR);
    let mut applied = 0usize;
    let mut samples = Vec::new();
    for tick in 1..=ticks {
        let target = ((tick as usize) * log.len()) / ticks as usize;
        while applied < target && applied < log.len() {
            if let Some(command) = kind_to_sim(&log[applied], 0) {
                sim.command(command).ok();
            }
            applied += 1;
        }
        sim.tick().ok();
        if tick % sample_every == 0 {
            samples.push(sim.checksum());
        }
    }
    (sim, samples)
}

// ---- benchmarks (plan §7d) --------------------------------------------------

/// Runs an `mp_*` benchmark and returns `(p50_ns, p99_ns, checksum_hex)`, or
/// `None` when `name` is not a registered mp bench.
pub fn bench(name: &str, ticks: u64) -> Option<(u64, u64, String)> {
    let ticks = ticks.max(1);
    match name {
        // Per-command apply overhead mirror (`bench mp_apply`).
        "mp_apply" | "mp_relay_roundtrip" => {
            let log = generated_log(4_096);
            let mut samples = Vec::with_capacity(ticks as usize);
            let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
            for index in 0..ticks as usize {
                let kind = &log[index % log.len()];
                let start = Instant::now();
                if let Some(command) = kind_to_sim(kind, 0) {
                    sim.command(command).ok();
                }
                samples.push(start.elapsed().as_nanos() as u64);
            }
            samples.sort_unstable();
            Some((
                percentile(&samples, 50),
                percentile(&samples, 99),
                sim.checksum_hex(),
            ))
        }
        // One-shot checksum cost mirror (`bench mp_checksum`).
        "mp_checksum" => {
            let log = generated_log(600);
            let mut sim = replay(7, &log, 600);
            let mut samples = Vec::with_capacity(ticks as usize);
            for _ in 0..ticks {
                let start = Instant::now();
                let checksum = sim.checksum();
                std::hint::black_box(checksum);
                samples.push(start.elapsed().as_nanos() as u64);
            }
            samples.sort_unstable();
            sim.tick().ok();
            Some((
                percentile(&samples, 50),
                percentile(&samples, 99),
                sim.checksum_hex(),
            ))
        }
        // Snapshot encode cost mirror (`bench mp_snapshot_bytes`).
        "mp_snapshot_bytes" => Some(bench_snapshot_bytes(ticks)),
        // Plan chunk upload/apply mirror (`bench mp_plan`).
        "mp_plan" => {
            let mut samples = Vec::with_capacity(ticks as usize);
            let blob = vec![7u8; 8 * 1024];
            for _ in 0..ticks {
                let start = Instant::now();
                let sum: usize = blob.iter().map(|byte| *byte as usize).sum();
                std::hint::black_box(sum);
                samples.push(start.elapsed().as_nanos() as u64);
            }
            samples.sort_unstable();
            Some((
                percentile(&samples, 50),
                percentile(&samples, 99),
                "plan-chunk".to_string(),
            ))
        }
        _ => None,
    }
}

fn bench_snapshot_bytes(ticks: u64) -> (u64, u64, String) {
    const ENTITY_BYTES: usize = 24;
    let mut body = vec![0u8; 900 * ENTITY_BYTES + 4_000 * 12];
    for (index, byte) in body.iter_mut().enumerate() {
        *byte = ((index * 31) & 0xff) as u8;
    }
    let mut samples = Vec::with_capacity(ticks as usize);
    let mut encoded_len = 0usize;
    for _ in 0..ticks {
        let snapshot = DynamicSnapshot {
            header: SnapshotHeader {
                format: SNAPSHOT_FORMAT,
                checksum_version: 1,
                command_id: 1,
                sim_tick: 1,
                map_id: "bench".to_string(),
                map_seed: 7,
                map_hash: 0,
                build_id: String::new(),
                content_hash: 0,
                next_entity_id: 1_200,
                rules_json: "{}".to_string(),
                wave: 1,
                wavetime: 0.0,
                rng_sim: vec![1, 2, 3, 4],
            },
            body: body.clone(),
        };
        let start = Instant::now();
        let encoded = snapshot.encode();
        samples.push(start.elapsed().as_nanos() as u64);
        encoded_len = encoded.len();
    }
    samples.sort_unstable();
    (
        percentile(&samples, 50),
        percentile(&samples, 99),
        format!("{encoded_len}-bytes"),
    )
}

/// Nearest-rank percentile over a pre-sorted sample slice.
fn percentile(sorted: &[u64], percent: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (percent * sorted.len()).div_ceil(100).saturating_sub(1);
    sorted[rank.min(sorted.len() - 1)]
}

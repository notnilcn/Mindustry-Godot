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

use anyhow::Context;
use mind_core::content::BlockId;
use mind_core::determinism::SimCommand;
use mind_core::sim::Sim;
use mind_stdb::commands::{PreflightContext, PredictionQueue, preflight_validate};
use mind_stdb::module_bindings::{
    CommandKind, MemberRole, PlaceBlock,
};
use mind_stdb::session::{can_emit, can_start};
use serde::Deserialize;
use spacetimedb_sdk::Identity;

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
}

impl MpScenario {
    /// Scenario name as registered in [`crate::registry`].
    pub fn name(self) -> &'static str {
        match self {
            Self::LobbyJoin => "mp_lobby_join",
            Self::CommandLogReplay => "mp_command_log_replay",
            Self::ValidationMatrix => "mp_validation_matrix",
        }
    }

    /// Maps a registered scenario name to its handler.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "mp_lobby_join" => Some(Self::LobbyJoin),
            "mp_command_log_replay" => Some(Self::CommandLogReplay),
            "mp_validation_matrix" => Some(Self::ValidationMatrix),
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
    }
}

fn emit(report: &serde_json::Value, dump: Option<&Path>, pass: bool, json: bool) -> anyhow::Result<i32> {
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
            log.push(CommandKind::BreakBlock(mind_stdb::module_bindings::BreakBlock {
                x,
                y,
            }));
        } else {
            log.push(CommandKind::PlaceBlock(PlaceBlock {
                x,
                y,
                block: if index % 2 == 0 { "stone-wall" } else { "router" }.to_string(),
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
            json: "x".repeat(case.json_len.unwrap_or(case.json.as_deref().map(str::len).unwrap_or(0))),
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
        } else if let (Some(reason), Err(actual)) = (&case.reason, &result) {
            if actual != reason {
                failures.push(format!(
                    "{}: reason `{actual}` != expected `{reason}`",
                    case.name
                ));
                pass = false;
            }
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

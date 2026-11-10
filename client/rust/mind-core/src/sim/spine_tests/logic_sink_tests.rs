// SPDX-License-Identifier: GPL-3.0-only

//! Local sinks for the privileged logic event log (#27): `sync` (server-target
//! executor write) and `clientdata` (per-channel handler bus), plus the
//! `ucontrol payEnter` → core control-select consumer.
//!
//! Upstream: none (port-only tests).

use crate::sim::test_support::stone_sim;
use crate::sim::*;

fn sink_sim() -> Sim {
    let mut sim = stone_sim(32);
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicWorldState {
            headless: false,
            ..crate::logic::world::LogicWorldState::new()
        });
    sim
}

/// #27: `SyncI` drains into the local executor write *and* the host queue
/// (`Call.syncVariable`, `LExecutor.java:2047-2060`).
#[test]
fn sync_event_writes_executor_variable_and_reaches_the_host_queue() {
    use crate::logic::blocks::logic_block::LogicBlockState;
    use crate::logic::world::{LogicSyncEvent, LogicWorldEvent};

    let mut sim = sink_sim();
    let id = sim
        .world_block_id("micro-processor")
        .expect("micro-processor");
    sim.command(crate::determinism::SimCommand::Place {
        x: 5,
        y: 5,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    let entity = sim.grid.entity_at(TilePos::new(5, 5)).expect("entity");
    let var = {
        let mut state = sim
            .ecs
            .0
            .get_mut::<LogicBlockState>(entity)
            .expect("logic state");
        let var = state.executor.arena.put_var("x");
        state.executor.arena.get_mut(var).num = 0.0;
        var
    };
    sim.ecs
        .0
        .get_resource_mut::<crate::logic::world::LogicWorldState>()
        .expect("state")
        .emit(LogicWorldEvent::Sync(Box::new(LogicSyncEvent {
            building_pos: TilePos::new(5, 5).pack(),
            var_name: "x".to_owned(),
            var_id: var as i32,
            is_obj: false,
            num: 42.0,
            obj: None,
        })));
    sim.tick().expect("tick");
    assert_eq!(
        sim.ecs
            .0
            .get::<LogicBlockState>(entity)
            .expect("state")
            .executor
            .arena
            .get(var)
            .num,
        42.0
    );
    let events = sim.take_logic_sync_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].var_name, "x");
    assert_eq!(events[0].num, 42.0);
}

/// #27: `ClientDataI` drains into the registered per-channel handlers
/// (`NetServer.logicClientDataHandlers`, `NetServer.java:131,605-607,698-705`).
#[test]
fn client_data_event_reaches_registered_handlers() {
    use crate::logic::world::{ClientLogicDataEvent, LogicClientDataBus, LogicWorldEvent};
    use std::sync::{Arc, Mutex};

    let mut sim = sink_sim();
    let received = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut bus = LogicClientDataBus::default();
    let sink = Arc::clone(&received);
    bus.add_handler("chan", move |event| {
        sink.lock()
            .expect("lock")
            .push(format!("{}:{}", event.channel, event.num));
    });
    sim.ecs.0.insert_resource(bus);
    sim.ecs
        .0
        .get_resource_mut::<crate::logic::world::LogicWorldState>()
        .expect("state")
        .emit(LogicWorldEvent::ClientData(Box::new(
            ClientLogicDataEvent {
                channel: "chan".to_owned(),
                is_obj: false,
                num: 7.0,
                obj: None,
                reliable: true,
            },
        )));
    sim.tick().expect("tick");
    assert_eq!(*received.lock().expect("lock"), vec!["chan:7".to_owned()]);
    assert!(
        sim.logic_event_log.iter().any(|kind| kind == "clientdata"),
        "{:?}",
        sim.logic_event_log
    );
}

/// #27: `ucontrol payEnter` emits `ControlSelect`; the sim consumer runs
/// `CoreBlock.onControlSelect` (respawn at the core) for the player unit.
#[test]
fn pay_enter_control_select_respawns_the_player_at_the_core() {
    use crate::logic::world::LogicWorldEvent;
    use crate::world::TilePos;

    let mut sim = sink_sim();
    sim.install_unit_runtime().expect("unit runtime");
    let core_id = sim.world_block_id("core-shard").expect("core-shard");
    sim.command(crate::determinism::SimCommand::Place {
        x: 10,
        y: 10,
        block: core_id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    let core = sim.grid.entity_at(TilePos::new(10, 10)).expect("core");
    let old = sim
        .spawn_player_unit("alpha", 1, 200.0, 200.0, 0.0)
        .expect("player unit");
    sim.ecs
        .0
        .get_resource_mut::<crate::logic::world::LogicWorldState>()
        .expect("state")
        .emit(LogicWorldEvent::ControlSelect {
            unit: old,
            build: core,
        });
    sim.tick().expect("tick");
    let player = sim.player_unit.expect("respawned player");
    assert_ne!(player, old, "the old unit was released");
    let (cx, cy) = sim
        .ecs
        .0
        .get::<crate::entities::comp::Pos>(core)
        .map(|pos| (pos.x, pos.y))
        .expect("core pos");
    let (px, py) = sim
        .ecs
        .0
        .get::<crate::entities::comp::Pos>(player)
        .map(|pos| (pos.x, pos.y))
        .expect("player pos");
    assert!(
        (px - cx).abs() < 1.0 && (py - cy).abs() < 1.0,
        "respawned at the core ({px}, {py}) vs ({cx}, {cy})"
    );
}

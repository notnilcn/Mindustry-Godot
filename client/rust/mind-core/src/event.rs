// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic, registration-ordered event bus.
//!
//! Ported from `core/src/mindustry/game/EventType.java` and Arc `Events.fire/on`.
//! The `define_events!` macro generates one *concrete* listener `Vec` per event
//! type (no type erasure), dispatched in registration order. Registering a
//! listener from inside a fired handler is a debug assert; P0 flushes queued
//! events synchronously at tick end.

use crate::content::{BlockId, PlanetId};
use crate::game::State;

/// Event fired when a client/headless host is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientCreateEvent;

/// Event fired when the client game is first loaded (UI-only in Mindustry;
/// core-defined so headless fires it too).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientLoadEvent;

/// Event fired after all content has been initialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentInitEvent;

/// Event fired when the game phase changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateChangeEvent {
    /// Previous phase.
    pub from: State,
    /// New phase.
    pub to: State,
}

/// Event fired after a block is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockPlacedEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Placed block.
    pub block: BlockId,
    /// Owning team (`BlockBuildEndEvent.team`).
    pub team: u8,
}

/// Event fired after a block is broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockBrokenEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Broken block.
    pub block: BlockId,
    /// `true` when the building died (destroy path); `false` for deconstruct /
    /// API break. The destroy path plays `destroySound`, the other plays
    /// `breakSound` (G07 T3 / PAR-0073).
    pub destroyed: bool,
    /// Owning team at removal (`BlockDestroyEvent.tile.team()`; H09 T10).
    pub team: u8,
}

/// Event fired when the simulation is reset to the menu (`EventType.ResetEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetEvent;

/// Event fired when a game begins (`EventType.PlayEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayEvent;

/// Event fired when a wave is triggered (`EventType.WaveEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaveEvent {
    /// Wave number.
    pub wave: i32,
}

/// Event fired after music registration (`EventType.MusicRegisterEvent`, HLP C7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MusicRegisterEvent;

/// Event fired before a save is written (`EventType.SaveWriteEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveWriteEvent;

/// Event fired after a save is loaded (`EventType.SaveLoadEvent`); `is_map`
/// marks a new map load (`WorldContext.isMap`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveLoadEvent {
    /// Whether the load counts as a new map load.
    pub is_map: bool,
}

/// Event fired when rules are (re)assigned from a save
/// (`EventType.RulesLoadEvent`). The rules payload is plan 12's type; for now
/// only the origin flag is modeled (upstream fires it only for non-map,
/// non-campaign loads).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RulesLoadEvent {
    /// Whether the rules came from a save (`fromSave`).
    pub from_save: bool,
}

/// Event fired when a world load completes — `World.loadMap`/`World.loadState`
/// (`EventType.WorldLoadEvent`; `World.java:220-230` + `Logic.java:105-125`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorldLoadEvent;

/// Event fired when the match ends (`EventType.GameOverEvent`;
/// `Logic.gameOver`/`updateGameOver`/`checkGameState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameOverEvent {
    /// Winning team id (`GameOverEvent.winner`).
    pub winner: u8,
}

/// Event fired when a campaign sector is captured (`EventType.SectorCaptureEvent`;
/// `Logic.sectorCapture`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectorCaptureEvent {
    /// Captured planet.
    pub planet: PlanetId,
    /// Captured sector id.
    pub sector: u16,
    /// First-ever capture (`!wasCaptured` before; `SectorCaptureEvent.initialCapture`).
    pub initial: bool,
}

/// Event fired when the player launches to a sector (`EventType.SectorLaunchEvent`;
/// `Control.playNewSector`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectorLaunchEvent {
    /// Launched planet.
    pub planet: PlanetId,
    /// Launched sector id.
    pub sector: u16,
}

/// A content identity on an event row (dense kind + id; the host resolves the
/// name for the `EventType` wire payload, which carries content by name).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventContent {
    /// `ContentType::Item` dense id.
    Item(u16),
    /// `ContentType::Liquid` dense id.
    Liquid(u16),
    /// `ContentType::Unit` dense id.
    Unit(u16),
}

/// Event fired when a content is unlocked (`EventType.UnlockEvent`;
/// `UnlockableContent.unlock` -> `Control.java:160-174`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnlockEvent {
    /// Unlocked content.
    pub content: EventContent,
}

/// Event fired when a research completes (`EventType.ResearchEvent`;
/// `TechTree.completeResearch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchEvent {
    /// Researched content.
    pub content: EventContent,
}

/// Event fired when a player deposits items into a core
/// (`EventType.DepositEvent`; `Logic` deposit listener).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepositEvent {
    /// Tile x of the receiving building.
    pub x: i16,
    /// Tile y of the receiving building.
    pub y: i16,
    /// Dense item id.
    pub item: u16,
    /// Deposited amount.
    pub amount: i32,
}

/// Event fired when the local player selects a build plan
/// (`EventType.BuildSelectEvent`; `InputHandler` build select).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildSelectEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Builder team.
    pub team: u8,
    /// Whether the selected plan is a deconstruction.
    pub breaking: bool,
}

/// Event fired when a building's configuration changes (`EventType.ConfigEvent`;
/// `Building.configure`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigEvent {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Configuring player id, when a player did it.
    pub player: Option<u32>,
}

/// Event fired when a player joins the hosted match (`EventType.PlayerJoin`;
/// `NetServer` join path — host side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerJoin {
    /// Port player id.
    pub player: u32,
}

/// Event fired when a player leaves the hosted match (`EventType.PlayerLeave`;
/// `NetServer.onDisconnect` — host side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerLeave {
    /// Port player id.
    pub player: u32,
}

/// Maximum typed rows a game-side outbox retains before dropping the oldest.
///
/// The host drains the outbox every frame it pumps the session; the cap is a
/// safety valve for a host that never drains, not a queue policy.
pub const EVENT_QUEUE_CAPACITY: usize = 256;

/// Generates an event bus with one listener vector per event type.
///
/// Ported from Arc `Events` (per-type listener lists + registration-order fire).
///
/// `queued` events are the sim's tick-end rows ([`SimEvent`], dispatched by
/// [`Events::dispatch`]). `flow` events are raised by the game flows, which have
/// no `&mut Sim` to fire from; they queue a [`GameEvent`] row ([`EventRow`]) that
/// the host dispatches with [`Events::dispatch_game`]. Both groups share the one
/// listener set, so `Events` stays the single bus.
#[macro_export]
macro_rules! define_events {
    (
        $(#[$outer:meta])*
        pub struct $name:ident {
            queued {
                $(
                    $(#[$qmeta:meta])*
                    $qevent:ident => $qfield:ident : $qon:ident, $qfire:ident
                ),* $(,)?
            }
            flow {
                $(
                    $(#[$fmeta:meta])*
                    $fevent:ident => $ffield:ident : $fon:ident, $ffire:ident
                ),* $(,)?
            }
        }
    ) => {
        $(#[$outer])*
        pub struct $name {
            firing: bool,
            $(
                $(#[$qmeta])*
                $qfield: ::std::vec::Vec<::std::boxed::Box<dyn FnMut(&$qevent) + 'static>>,
            )*
            $(
                $(#[$fmeta])*
                $ffield: ::std::vec::Vec<::std::boxed::Box<dyn FnMut(&$fevent) + 'static>>,
            )*
        }

        impl $name {
            /// Creates an empty bus.
            pub fn new() -> Self {
                Self {
                    firing: false,
                    $($qfield: ::std::vec::Vec::new(),)*
                    $($ffield: ::std::vec::Vec::new(),)*
                }
            }

            $(
                /// Registers a listener. Listeners fire in registration order.
                pub fn $qon<F>(&mut self, listener: F)
                where
                    F: FnMut(&$qevent) + 'static,
                {
                    debug_assert!(
                        !self.firing,
                        "cannot register an event listener while events are being dispatched"
                    );
                    self.$qfield.push(::std::boxed::Box::new(listener));
                }

                /// Fires an event to every registered listener, in registration order.
                pub fn $qfire(&mut self, event: &$qevent) {
                    let was_firing = self.firing;
                    self.firing = true;
                    for listener in &mut self.$qfield {
                        listener(event);
                    }
                    self.firing = was_firing;
                }
            )*

            $(
                /// Registers a listener. Listeners fire in registration order.
                pub fn $fon<F>(&mut self, listener: F)
                where
                    F: FnMut(&$fevent) + 'static,
                {
                    debug_assert!(
                        !self.firing,
                        "cannot register an event listener while events are being dispatched"
                    );
                    self.$ffield.push(::std::boxed::Box::new(listener));
                }

                /// Fires an event to every registered listener, in registration order.
                pub fn $ffire(&mut self, event: &$fevent) {
                    let was_firing = self.firing;
                    self.firing = true;
                    for listener in &mut self.$ffield {
                        listener(event);
                    }
                    self.firing = was_firing;
                }
            )*

            /// Dispatches a queued sim event to its listener list.
            pub fn dispatch(&mut self, event: SimEvent) {
                match event {
                    $(SimEvent::$qevent(event) => self.$qfire(&event),)*
                }
            }

            /// Dispatches a game-flow row to its listener list.
            pub fn dispatch_game(&mut self, event: GameEvent) {
                match event {
                    $(GameEvent::$fevent(event) => self.$ffire(&event),)*
                }
            }
        }

        impl ::std::default::Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        /// A queued sim event (P0: flushed synchronously at tick end).
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum SimEvent {
            $(
                $(#[$qmeta])*
                $qevent($qevent),
            )*
        }

        /// A game-flow event row (queued by the play flows; the host dispatches it).
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum GameEvent {
            $(
                $(#[$fmeta])*
                $fevent($fevent),
            )*
        }
    };
}

define_events! {
    /// The P0 event bus.
    pub struct Events {
        queued {
            ClientCreateEvent => client_create : on_client_create, fire_client_create,
            ClientLoadEvent => client_load : on_client_load, fire_client_load,
            ContentInitEvent => content_init : on_content_init, fire_content_init,
            StateChangeEvent => state_change : on_state_change, fire_state_change,
            BlockPlacedEvent => block_placed : on_block_placed, fire_block_placed,
            BlockBrokenEvent => block_broken : on_block_broken, fire_block_broken,
            ResetEvent => reset : on_reset, fire_reset,
            PlayEvent => play : on_play, fire_play,
            WaveEvent => wave : on_wave, fire_wave,
            MusicRegisterEvent => music_register : on_music_register, fire_music_register,
            SaveWriteEvent => save_write : on_save_write, fire_save_write,
            SaveLoadEvent => save_load : on_save_load, fire_save_load,
            RulesLoadEvent => rules_load : on_rules_load, fire_rules_load,
        }
        flow {
            WorldLoadEvent => world_load : on_world_load, fire_world_load,
            GameOverEvent => game_over : on_game_over, fire_game_over,
            SectorCaptureEvent => sector_capture : on_sector_capture, fire_sector_capture,
            SectorLaunchEvent => sector_launch : on_sector_launch, fire_sector_launch,
            UnlockEvent => unlock : on_unlock, fire_unlock,
            ResearchEvent => research : on_research, fire_research,
            DepositEvent => deposit : on_deposit, fire_deposit,
            BuildSelectEvent => build_select : on_build_select, fire_build_select,
            ConfigEvent => config : on_config, fire_config,
            PlayerJoin => player_join : on_player_join, fire_player_join,
            PlayerLeave => player_leave : on_player_leave, fire_player_leave,
        }
    }
}

/// One typed row queued by the game flows for the host's [`Events`] bus.
///
/// The play flows hold a [`PlaySession`](crate::game::play::PlaySession), not
/// the sim's `Events` instance, so they queue rows (`EventRow`) exactly like the
/// sim queues its tick-end [`SimEvent`]s; the host drains them and dispatches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventRow {
    /// A sim tick row (`SimEvent`; the play flow re-raises `PlayEvent`).
    Sim(SimEvent),
    /// A game-flow row (`GameEvent`).
    Game(GameEvent),
}

impl EventRow {
    /// Upstream `EventType` class name for this row.
    pub fn class_name(&self) -> &'static str {
        match self {
            EventRow::Sim(event) => event.class_name(),
            EventRow::Game(event) => event.class_name(),
        }
    }
}

impl Events {
    /// Dispatches one queued [`EventRow`] to its listener list.
    pub fn dispatch_row(&mut self, row: EventRow) {
        match row {
            EventRow::Sim(event) => self.dispatch(event),
            EventRow::Game(event) => self.dispatch_game(event),
        }
    }
}

impl SimEvent {
    /// Upstream `EventType` class name for this row — the `type` tag the relay
    /// puts in a `MatchEvent.detail` envelope (`mind-stdb/src/event_type.rs`,
    /// the 89-class taxonomy this bus feeds).
    ///
    /// The port's `BlockPlacedEvent`/`BlockBrokenEvent` are its own tile rows;
    /// they collapse onto the upstream build classes (`BlockBuildEndEvent` for
    /// a placement/deconstruction, `BlockDestroyEvent` for the destroy path) so
    /// every queued row names a real `EventType` class.
    pub fn class_name(&self) -> &'static str {
        match self {
            SimEvent::ClientCreateEvent(_) => "ClientCreateEvent",
            SimEvent::ClientLoadEvent(_) => "ClientLoadEvent",
            SimEvent::ContentInitEvent(_) => "ContentInitEvent",
            SimEvent::StateChangeEvent(_) => "StateChangeEvent",
            SimEvent::BlockPlacedEvent(_) => "BlockBuildEndEvent",
            SimEvent::BlockBrokenEvent(broken) => {
                if broken.destroyed {
                    "BlockDestroyEvent"
                } else {
                    "BlockBuildEndEvent"
                }
            }
            SimEvent::ResetEvent(_) => "ResetEvent",
            SimEvent::PlayEvent(_) => "PlayEvent",
            SimEvent::WaveEvent(_) => "WaveEvent",
            SimEvent::MusicRegisterEvent(_) => "MusicRegisterEvent",
            SimEvent::SaveWriteEvent(_) => "SaveWriteEvent",
            SimEvent::SaveLoadEvent(_) => "SaveLoadEvent",
            SimEvent::RulesLoadEvent(_) => "RulesLoadEvent",
        }
    }
}

impl GameEvent {
    /// Upstream `EventType` class name for this row (see [`SimEvent::class_name`]).
    pub fn class_name(&self) -> &'static str {
        match self {
            GameEvent::WorldLoadEvent(_) => "WorldLoadEvent",
            GameEvent::GameOverEvent(_) => "GameOverEvent",
            GameEvent::SectorCaptureEvent(_) => "SectorCaptureEvent",
            GameEvent::SectorLaunchEvent(_) => "SectorLaunchEvent",
            GameEvent::UnlockEvent(_) => "UnlockEvent",
            GameEvent::ResearchEvent(_) => "ResearchEvent",
            GameEvent::DepositEvent(_) => "DepositEvent",
            GameEvent::BuildSelectEvent(_) => "BuildSelectEvent",
            GameEvent::ConfigEvent(_) => "ConfigEvent",
            GameEvent::PlayerJoin(_) => "PlayerJoin",
            GameEvent::PlayerLeave(_) => "PlayerLeave",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn listeners_fire_in_registration_order() {
        let mut events = Events::new();
        let order = Rc::new(RefCell::new(Vec::new()));

        let first = order.clone();
        events.on_block_placed(move |event| {
            assert_eq!(event.block, BlockId::STONE_WALL);
            first.borrow_mut().push(1u32);
        });
        let second = order.clone();
        events.on_block_placed(move |_| second.borrow_mut().push(2));
        let third = order.clone();
        events.on_block_placed(move |_| third.borrow_mut().push(3));

        events.dispatch(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
            x: 4,
            y: 5,
            block: BlockId::STONE_WALL,
            team: 1,
        }));

        assert_eq!(*order.borrow(), vec![1, 2, 3]);
    }

    #[test]
    fn dispatch_routes_to_the_right_event() {
        let mut events = Events::new();
        let seen = Rc::new(RefCell::new(Vec::new()));

        let state_seen = seen.clone();
        events.on_state_change(move |event| {
            state_seen
                .borrow_mut()
                .push(format!("{:?}->{:?}", event.from, event.to));
        });
        let load_seen = seen.clone();
        events.on_client_load(move |_| load_seen.borrow_mut().push("load".to_owned()));

        events.dispatch(SimEvent::StateChangeEvent(StateChangeEvent {
            from: State::Menu,
            to: State::Playing,
        }));
        events.dispatch(SimEvent::ClientLoadEvent(ClientLoadEvent));

        assert_eq!(*seen.borrow(), vec!["Menu->Playing", "load"]);
    }

    /// Every game-flow row names an upstream `EventType` class; the classes
    /// added for GAPS #37 land on the taxonomy names the relay/`EventBus` uses.
    #[test]
    fn event_rows_name_upstream_event_type_classes() {
        let planet = PlanetId::new(0);
        let rows = [
            (GameEvent::WorldLoadEvent(WorldLoadEvent), "WorldLoadEvent"),
            (
                GameEvent::GameOverEvent(GameOverEvent { winner: 2 }),
                "GameOverEvent",
            ),
            (
                GameEvent::SectorCaptureEvent(SectorCaptureEvent {
                    planet,
                    sector: 12,
                    initial: true,
                }),
                "SectorCaptureEvent",
            ),
            (
                GameEvent::SectorLaunchEvent(SectorLaunchEvent { planet, sector: 12 }),
                "SectorLaunchEvent",
            ),
            (
                GameEvent::UnlockEvent(UnlockEvent {
                    content: EventContent::Item(3),
                }),
                "UnlockEvent",
            ),
            (
                GameEvent::ResearchEvent(ResearchEvent {
                    content: EventContent::Unit(1),
                }),
                "ResearchEvent",
            ),
            (
                GameEvent::DepositEvent(DepositEvent {
                    x: 4,
                    y: 5,
                    item: 3,
                    amount: 10,
                }),
                "DepositEvent",
            ),
            (
                GameEvent::BuildSelectEvent(BuildSelectEvent {
                    x: 4,
                    y: 5,
                    team: 1,
                    breaking: false,
                }),
                "BuildSelectEvent",
            ),
            (
                GameEvent::ConfigEvent(ConfigEvent {
                    x: 4,
                    y: 5,
                    player: Some(1),
                }),
                "ConfigEvent",
            ),
            (
                GameEvent::PlayerJoin(PlayerJoin { player: 7 }),
                "PlayerJoin",
            ),
            (
                GameEvent::PlayerLeave(PlayerLeave { player: 7 }),
                "PlayerLeave",
            ),
        ];
        for (row, class) in &rows {
            assert_eq!(row.class_name(), *class, "{row:?} names its upstream class");
            assert!(class.ends_with("Event") || matches!(*class, "PlayerJoin" | "PlayerLeave"));
        }
        // The port's own tile rows collapse onto the upstream build classes.
        assert_eq!(
            SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                x: 0,
                y: 0,
                block: BlockId::STONE_WALL,
                team: 1,
            })
            .class_name(),
            "BlockBuildEndEvent"
        );
        assert_eq!(
            SimEvent::BlockBrokenEvent(BlockBrokenEvent {
                x: 0,
                y: 0,
                block: BlockId::STONE_WALL,
                destroyed: true,
                team: 1,
            })
            .class_name(),
            "BlockDestroyEvent"
        );
        // A row wrapper preserves the class of either group.
        assert_eq!(
            EventRow::Game(GameEvent::WorldLoadEvent(WorldLoadEvent)).class_name(),
            "WorldLoadEvent"
        );
        assert_eq!(
            EventRow::Sim(SimEvent::PlayEvent(PlayEvent)).class_name(),
            "PlayEvent"
        );
    }

    /// A full game-flow sequence round-trips through the bus: each queued row
    /// reaches the listener registered for its own class, in queue order.
    #[test]
    fn play_flow_rows_round_trip_through_the_bus() {
        let planet = PlanetId::new(1);
        let rows = vec![
            EventRow::Game(GameEvent::WorldLoadEvent(WorldLoadEvent)),
            EventRow::Sim(SimEvent::PlayEvent(PlayEvent)),
            EventRow::Game(GameEvent::SectorLaunchEvent(SectorLaunchEvent {
                planet,
                sector: 15,
            })),
            EventRow::Game(GameEvent::SectorCaptureEvent(SectorCaptureEvent {
                planet,
                sector: 15,
                initial: true,
            })),
            EventRow::Game(GameEvent::GameOverEvent(GameOverEvent { winner: 2 })),
        ];

        let mut events = Events::new();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let world_seen = seen.clone();
        events.on_world_load(move |_| world_seen.borrow_mut().push("WorldLoadEvent".to_owned()));
        let play_seen = seen.clone();
        events.on_play(move |_| play_seen.borrow_mut().push("PlayEvent".to_owned()));
        let launch_seen = seen.clone();
        events.on_sector_launch(move |event| {
            assert_eq!(event.sector, 15);
            launch_seen
                .borrow_mut()
                .push("SectorLaunchEvent".to_owned());
        });
        let capture_seen = seen.clone();
        events.on_sector_capture(move |event| {
            assert!(event.initial);
            capture_seen
                .borrow_mut()
                .push("SectorCaptureEvent".to_owned());
        });
        let over_seen = seen.clone();
        events.on_game_over(move |event| {
            assert_eq!(event.winner, 2);
            over_seen.borrow_mut().push("GameOverEvent".to_owned());
        });

        for row in rows {
            let class = row.class_name();
            events.dispatch_row(row);
            assert_eq!(
                seen.borrow().last().map(String::as_str),
                Some(class),
                "row reached its own listener"
            );
        }
        assert_eq!(
            *seen.borrow(),
            vec![
                "WorldLoadEvent",
                "PlayEvent",
                "SectorLaunchEvent",
                "SectorCaptureEvent",
                "GameOverEvent",
            ]
        );
    }
}

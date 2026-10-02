// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic, registration-ordered event bus.
//!
//! Ported from `core/src/mindustry/game/EventType.java` and Arc `Events.fire/on`.
//! The `define_events!` macro generates one *concrete* listener `Vec` per event
//! type (no type erasure), dispatched in registration order. Registering a
//! listener from inside a fired handler is a debug assert; P0 flushes queued
//! events synchronously at tick end.

use crate::content::BlockId;
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
}

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

/// Generates an event bus with one listener vector per event type.
///
/// Ported from Arc `Events` (per-type listener lists + registration-order fire).
#[macro_export]
macro_rules! define_events {
    (
        $(#[$outer:meta])*
        pub struct $name:ident {
            $(
                $(#[$meta:meta])*
                $event:ident => $field:ident : $on:ident, $fire:ident
            ),* $(,)?
        }
    ) => {
        $(#[$outer])*
        pub struct $name {
            firing: bool,
            $(
                $(#[$meta])*
                $field: ::std::vec::Vec<::std::boxed::Box<dyn FnMut(&$event) + 'static>>,
            )*
        }

        impl $name {
            /// Creates an empty bus.
            pub fn new() -> Self {
                Self {
                    firing: false,
                    $($field: ::std::vec::Vec::new(),)*
                }
            }

            $(
                /// Registers a listener. Listeners fire in registration order.
                pub fn $on<F>(&mut self, listener: F)
                where
                    F: FnMut(&$event) + 'static,
                {
                    debug_assert!(
                        !self.firing,
                        "cannot register an event listener while events are being dispatched"
                    );
                    self.$field.push(::std::boxed::Box::new(listener));
                }

                /// Fires an event to every registered listener, in registration order.
                pub fn $fire(&mut self, event: &$event) {
                    let was_firing = self.firing;
                    self.firing = true;
                    for listener in &mut self.$field {
                        listener(event);
                    }
                    self.firing = was_firing;
                }
            )*

            /// Dispatches a queued event to its listener list.
            pub fn dispatch(&mut self, event: SimEvent) {
                match event {
                    $(SimEvent::$event(event) => self.$fire(&event),)*
                }
            }
        }

        impl ::std::default::Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        /// A queued event (P0: flushed synchronously at tick end).
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum SimEvent {
            $(
                $(#[$meta])*
                $event($event),
            )*
        }
    };
}

define_events! {
    /// The P0 event bus.
    pub struct Events {
        ClientCreateEvent => client_create : on_client_create, fire_client_create,
        ClientLoadEvent => client_load : on_client_load, fire_client_load,
        ContentInitEvent => content_init : on_content_init, fire_content_init,
        StateChangeEvent => state_change : on_state_change, fire_state_change,
        BlockPlacedEvent => block_placed : on_block_placed, fire_block_placed,
        BlockBrokenEvent => block_broken : on_block_broken, fire_block_broken,
        SaveWriteEvent => save_write : on_save_write, fire_save_write,
        SaveLoadEvent => save_load : on_save_load, fire_save_load,
        RulesLoadEvent => rules_load : on_rules_load, fire_rules_load,
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
}

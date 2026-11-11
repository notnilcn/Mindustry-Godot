// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign event bus: the fired-event ring, its recorder listeners and the
//! `#[func]`s that expose the log to the MCP oracle.
//! Upstream: core/src/mindustry/game/EventType.java; core/src/mindustry/core/Control.java (Events.on)

use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Fires a host-side `PlayerJoin` (`NetServer` join path,
    /// `NetServer.java:1020`) or `PlayerLeave` (`onDisconnect`,
    /// `NetServer.java:618`).
    ///
    /// The relay roster is identity-based and the port has no `Player.id`
    /// counter, so `player` is the relay member row id (the closest stable
    /// analogue), truncated to the wire `u32`. Called from
    /// `MindNet::drain_members_rows` on `connected` transitions; `#[func]` so
    /// the MCP oracle can drive the bus headlessly.
    #[func]
    pub fn fire_player_event(&mut self, join: bool, player: i64) -> bool {
        let player = u32::try_from(player).unwrap_or(u32::MAX);
        if join {
            self.events
                .dispatch_game(event::GameEvent::PlayerJoin(event::PlayerJoin { player }));
        } else {
            self.events
                .dispatch_game(event::GameEvent::PlayerLeave(event::PlayerLeave { player }));
        }
        true
    }

    /// Drains the rows the `EventType` bus dispatched (`[{class, detail}]`),
    /// oldest first (PAR-0577 residual; MCP/UI observability).
    #[func]
    pub fn take_fired_events(&mut self) -> Array<VarDictionary> {
        let rows = self.fired_events.borrow_mut().take();
        let mut out = Array::<VarDictionary>::new();
        for row in rows {
            let mut dict = VarDictionary::new();
            dict.set("class", &GString::from(row.class));
            dict.set("detail", &GString::from(row.detail.as_str()));
            out.push(&dict);
        }
        out
    }

    /// Rows dispatched per `EventType` class since the last clear
    /// (`[{"class", "count"}]`, declaration order of first dispatch).
    #[func]
    pub fn fired_event_counts_json(&self) -> GString {
        let counts = &self.fired_events.borrow().counts;
        let rows: Vec<serde_json::Value> = counts
            .iter()
            .map(|(class, count)| serde_json::json!({"class": class, "count": count}))
            .collect();
        value_gstring(&serde_json::Value::Array(rows))
    }

    /// Total rows dispatched since the last clear.
    #[func]
    pub fn fired_event_total(&self) -> i64 {
        self.fired_events.borrow().total() as i64
    }

    /// Rows dispatched for one `EventType` class since the last clear.
    #[func]
    pub fn fired_event_count(&self, class: GString) -> i64 {
        self.fired_events.borrow().count(&class.to_string()) as i64
    }

    /// Retained row count (bounded record, not the drained rows).
    #[func]
    pub fn fired_event_pending(&self) -> i64 {
        self.fired_events.borrow().len() as i64
    }

    /// Drops the retained rows and the per-class counters (match reset).
    #[func]
    pub fn clear_fired_events(&mut self) {
        if !self.fired_events.borrow().is_empty() {
            log::info!("campaign event bus: cleared");
        }
        self.fired_events.borrow_mut().clear();
    }
}

/// One `EventType` row dispatched through the campaign bus
/// (`core/src/mindustry/game/EventType.java`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiredEvent {
    /// Upstream `EventType` class name (`EventRow::class_name`).
    pub class: &'static str,
    /// Payload summary the recorder listener built (`"player 7"`,
    /// `"serpulo:15"`, `"item 3"`, ...).
    pub detail: String,
}

/// Bounded record of the rows dispatched through the campaign `Events` bus
/// (PAR-0577 residual).
///
/// `Events` listeners are `FnMut(&T)` closures with no handle on the node, so
/// the facade installs one recorder listener per class (the port's equivalent
/// of `Control.java`'s `Events.on(...)` registrations); this log is the
/// observable sink the MCP oracle reads (`take_fired_events`) and the unit
/// tests assert on.
#[derive(Debug, Default)]
pub struct FiredEvents {
    rows: VecDeque<FiredEvent>,
    counts: BTreeMap<&'static str, u64>,
}

impl FiredEvents {
    /// Empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one dispatched row, dropping the oldest past
    /// `mind_core::event::EVENT_QUEUE_CAPACITY` (the host drains every frame;
    /// the cap only guards a never-draining host).
    pub fn record(&mut self, class: &'static str, detail: impl Into<String>) {
        *self.counts.entry(class).or_insert(0) += 1;
        while self.rows.len() >= event::EVENT_QUEUE_CAPACITY {
            self.rows.pop_front();
        }
        self.rows.push_back(FiredEvent {
            class,
            detail: detail.into(),
        });
    }

    /// Drains the retained rows (oldest first).
    pub fn take(&mut self) -> Vec<FiredEvent> {
        self.rows.drain(..).collect()
    }

    /// Retained row count.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether no row is retained.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Rows dispatched for `class` since the last [`Self::clear`].
    pub fn count(&self, class: &str) -> u64 {
        self.counts.get(class).copied().unwrap_or(0)
    }

    /// Rows dispatched since the last [`Self::clear`].
    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Drops the retained rows and the counters (match reset).
    pub fn clear(&mut self) {
        self.rows.clear();
        self.counts.clear();
    }
}

/// Payload summary of one `EventContent` (`"item 3"`, `"liquid 1"`,
/// `"unit 5"`).
fn event_content_detail(content: event::EventContent) -> String {
    match content {
        event::EventContent::Item(id) => format!("item {id}"),
        event::EventContent::Liquid(id) => format!("liquid {id}"),
        event::EventContent::Unit(id) => format!("unit {id}"),
    }
}

/// `EventContent` projection of a content reference, for the event kinds whose
/// payload is content (`UnlockEvent`/`ResearchEvent`).
///
/// `EventContent` models item/liquid/unit only; block/status/sector content has
/// no payload variant, so those rows cannot be published (`mind-core`
/// residual: `event.rs` needs `Block(u16)`).
pub(super) fn event_content_of(content: ContentRef) -> Option<event::EventContent> {
    match content.type_ {
        ContentType::Item => Some(event::EventContent::Item(content.id)),
        ContentType::Liquid => Some(event::EventContent::Liquid(content.id)),
        ContentType::Unit => Some(event::EventContent::Unit(content.id)),
        _ => None,
    }
}

/// Registers the recorder listeners for every `EventType` class the host owns.
///
/// The outbox the play flows fill ([`mind_core::game::play::PlaySession`])
/// carries the `GameEvent` flow rows plus `SimEvent::PlayEvent` (`Logic.play`);
/// the remaining queued `SimEvent`s are dispatched by the sim's own bus
/// (`Sim::flush_events`). Every upstream listener the port kept inline
/// (capture bookkeeping, autosaves, unlock toasts) already runs in
/// `pump_runtime`; the recorder makes the fired rows observable.
pub(super) fn install_event_listeners(events: &mut event::Events, log: &Rc<RefCell<FiredEvents>>) {
    fn record(log: &Rc<RefCell<FiredEvents>>, class: &'static str, detail: String) {
        if let Ok(mut log) = log.try_borrow_mut() {
            log.record(class, detail);
        }
    }

    macro_rules! recorder {
        ($on:ident, $class:literal, |$e:ident| $detail:expr) => {{
            let log = Rc::clone(log);
            events.$on(move |$e| record(&log, $class, $detail));
        }};
    }

    recorder!(on_play, "PlayEvent", |_e| String::new());
    recorder!(on_world_load, "WorldLoadEvent", |_e| String::new());
    recorder!(on_game_over, "GameOverEvent", |e| format!(
        "winner {}",
        e.winner
    ));
    recorder!(on_sector_capture, "SectorCaptureEvent", |e| format!(
        "{}:{} initial={}",
        e.planet.raw(),
        e.sector,
        e.initial
    ));
    recorder!(on_sector_launch, "SectorLaunchEvent", |e| format!(
        "{}:{}",
        e.planet.raw(),
        e.sector
    ));
    recorder!(on_unlock, "UnlockEvent", |e| event_content_detail(
        e.content
    ));
    recorder!(on_research, "ResearchEvent", |e| event_content_detail(
        e.content
    ));
    recorder!(on_deposit, "DepositEvent", |e| format!(
        "{},{} {} x{}",
        e.x,
        e.y,
        event_content_detail(event::EventContent::Item(e.item)),
        e.amount
    ));
    recorder!(on_build_select, "BuildSelectEvent", |e| format!(
        "{},{} team {} breaking={}",
        e.x, e.y, e.team, e.breaking
    ));
    recorder!(on_config, "ConfigEvent", |e| format!(
        "{},{} player {:?}",
        e.x, e.y, e.player
    ));
    recorder!(on_player_join, "PlayerJoin", |e| format!(
        "player {}",
        e.player
    ));
    recorder!(on_player_leave, "PlayerLeave", |e| format!(
        "player {}",
        e.player
    ));
}

/// Dispatches drained [`event::EventRow`]s to the bus; returns the count.
pub(super) fn dispatch_event_rows(
    events: &mut event::Events,
    rows: impl IntoIterator<Item = event::EventRow>,
) -> usize {
    let mut count = 0;
    for row in rows {
        events.dispatch_row(row);
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PAR-0577 residual: the campaign bus dispatches the play-flow outbox rows
    /// (`WorldLoadEvent` then `PlayEvent`) and the recorder listeners see both.
    #[test]
    fn campaign_bus_dispatches_the_play_flow_outbox() {
        let mut bus = event::Events::new();
        let log = Rc::new(RefCell::new(FiredEvents::new()));
        install_event_listeners(&mut bus, &log);

        let mut session = PlaySession::new(Rules::default());
        let mut epoch = RulesEpoch::new();
        let _ = play_map(&mut session, Rules::default(), None, |_| false, &mut epoch);
        let rows = session.take_event_rows();
        assert_eq!(
            rows.iter()
                .map(event::EventRow::class_name)
                .collect::<Vec<_>>(),
            vec!["WorldLoadEvent", "PlayEvent"]
        );
        assert_eq!(dispatch_event_rows(&mut bus, rows), 2);

        let fired = log.borrow_mut().take();
        assert_eq!(
            fired.iter().map(|row| row.class).collect::<Vec<_>>(),
            vec!["WorldLoadEvent", "PlayEvent"]
        );
        assert_eq!(log.borrow().count("WorldLoadEvent"), 1);
        assert_eq!(log.borrow().count("PlayEvent"), 1);
        assert_eq!(log.borrow().total(), 2);
        assert!(session.take_event_rows().is_empty(), "outbox drained");
    }

    /// A `GameEvent` row routes to its own listener and records its payload; a
    /// `SimEvent` row (`Logic.play`'s `PlayEvent`) routes through the queued
    /// arm of the same bus.
    #[test]
    fn campaign_bus_routes_game_and_sim_rows_with_payloads() {
        let mut bus = event::Events::new();
        let log = Rc::new(RefCell::new(FiredEvents::new()));
        install_event_listeners(&mut bus, &log);

        let planet = PlanetId::new(0);
        dispatch_event_rows(
            &mut bus,
            [
                event::EventRow::Game(event::GameEvent::SectorLaunchEvent(
                    event::SectorLaunchEvent { planet, sector: 15 },
                )),
                event::EventRow::Sim(event::SimEvent::PlayEvent(event::PlayEvent)),
                event::EventRow::Game(event::GameEvent::PlayerJoin(event::PlayerJoin {
                    player: 7,
                })),
                event::EventRow::Game(event::GameEvent::ResearchEvent(event::ResearchEvent {
                    content: event::EventContent::Item(3),
                })),
            ],
        );

        let fired = log.borrow_mut().take();
        let classes: Vec<&str> = fired.iter().map(|row| row.class).collect();
        assert_eq!(
            classes,
            vec![
                "SectorLaunchEvent",
                "PlayEvent",
                "PlayerJoin",
                "ResearchEvent"
            ]
        );
        assert_eq!(fired[0].detail, "0:15");
        assert_eq!(fired[2].detail, "player 7");
        assert_eq!(fired[3].detail, "item 3");
    }

    /// `EventContent` models item/liquid/unit only; a block research node has no
    /// payload (reported as a mind-core residual, not silently mis-typed).
    #[test]
    fn research_payload_covers_item_liquid_and_unit_only() {
        assert_eq!(
            event_content_of(ContentRef::new(ContentType::Item, 3)),
            Some(event::EventContent::Item(3))
        );
        assert_eq!(
            event_content_of(ContentRef::new(ContentType::Liquid, 1)),
            Some(event::EventContent::Liquid(1))
        );
        assert_eq!(
            event_content_of(ContentRef::new(ContentType::Unit, 2)),
            Some(event::EventContent::Unit(2))
        );
        assert_eq!(
            event_content_of(ContentRef::new(ContentType::Block, 2)),
            None
        );
        assert_eq!(event_content_detail(event::EventContent::Unit(2)), "unit 2");
    }

    /// The fired-event record is bounded by `EVENT_QUEUE_CAPACITY` (a
    /// never-draining host cannot grow it without bound) but keeps counting.
    #[test]
    fn fired_events_record_is_bounded_and_counted() {
        let mut log = FiredEvents::new();
        for _ in 0..(event::EVENT_QUEUE_CAPACITY + 5) {
            log.record("PlayerJoin", "player 1");
        }
        assert_eq!(log.len(), event::EVENT_QUEUE_CAPACITY);
        assert_eq!(log.total(), (event::EVENT_QUEUE_CAPACITY + 5) as u64);
        assert_eq!(
            log.count("PlayerJoin"),
            (event::EVENT_QUEUE_CAPACITY + 5) as u64
        );
        let retained = log.take();
        assert_eq!(
            retained.last().map(|row| row.class),
            Some("PlayerJoin"),
            "newest row is retained"
        );
        log.clear();
        assert!(log.is_empty());
        assert_eq!(log.total(), 0);
    }
}

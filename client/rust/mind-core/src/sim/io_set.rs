// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `IoSet` — the save/load tick boundary (plan 05 M9; plan 04 §3.10).
//!
//! Save/load must never run in the middle of a tick: plan 04's `MindIo` cannot
//! capture or apply a live world while systems are mutating it. This module is
//! the seam the host (and later `mind-gdext::MindIo`) registers against:
//!
//! - [`IoSet::Apply`] runs *before* the tick schedule: queued load requests are
//!   handed to the registered [`IoHandler`].
//! - [`IoSet::Capture`] runs *after* `Logic.update` (after
//!   [`crate::sim::schedule::TickSet::AfterGameUpdate`]): queued save requests
//!   are handed to the handler.
//!
//! `Sim::tick` invokes both boundaries at the exact Java positions, so the order
//! is observable and deterministic. With no handler registered, requests stay
//! [`IoQueue::pending`] for the host to fulfill directly (e.g. a synchronous
//! `MindIo.save_game` call at the boundary).

use std::path::PathBuf;

use crate::sim::Sim;

/// The two IO boundary slots, in execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IoSet {
    /// Pre-tick: drain queued load requests.
    Apply,
    /// Post-tick (after `AfterGameUpdate`): drain queued save requests.
    Capture,
}

impl IoSet {
    /// Parity name (trace/report).
    pub const fn name(self) -> &'static str {
        match self {
            IoSet::Apply => "IoSet::Apply",
            IoSet::Capture => "IoSet::Capture",
        }
    }
}

/// A queued save/load request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoRequest {
    /// Capture the live world to `path` at the next `Capture` boundary.
    Save {
        /// Destination file (`.msav`).
        path: PathBuf,
        /// Whether to write map metadata (`true`) or save metadata (`false`).
        as_map: bool,
    },
    /// Apply `path` to the live world at the next `Apply` boundary.
    Load {
        /// Source file (`.msav`).
        path: PathBuf,
    },
}

impl IoRequest {
    /// The boundary at which this request is processed.
    pub const fn set(&self) -> IoSet {
        match self {
            IoRequest::Save { .. } => IoSet::Capture,
            IoRequest::Load { .. } => IoSet::Apply,
        }
    }
}

/// The outcome attached to a processed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoStatus {
    /// The handler completed the request.
    Ok,
    /// The handler failed; the message is logged, never panics.
    Failed(String),
    /// No handler is registered (or the handler declined); the request remains
    /// for the host to fulfill.
    Deferred,
}

/// A processed request plus its outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoResponse {
    /// The originating request.
    pub request: IoRequest,
    /// What happened to it.
    pub status: IoStatus,
}

/// Host-registered IO executor. `mind-gdext::MindIo` will register one backed by
/// plan 04's `SaveIo`/`MapIo` and plan 06's `WorldContext`.
pub trait IoHandler: Send {
    /// Applies queued load requests (the `IoSet::Apply` boundary).
    fn apply(&mut self, sim: &mut Sim, requests: &[IoRequest]) -> Vec<IoResponse>;
    /// Captures queued save requests (the `IoSet::Capture` boundary).
    fn capture(&mut self, sim: &mut Sim, requests: &[IoRequest]) -> Vec<IoResponse>;
}

/// The request/response queue owned by [`Sim`].
#[derive(Default)]
pub struct IoQueue {
    pending: Vec<IoRequest>,
    responses: Vec<IoResponse>,
    handler: Option<Box<dyn IoHandler>>,
}

impl std::fmt::Debug for IoQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IoQueue")
            .field("pending", &self.pending.len())
            .field("responses", &self.responses.len())
            .field("has_handler", &self.handler.is_some())
            .finish()
    }
}

impl IoQueue {
    /// Creates an empty queue with no handler.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers (or clears) the executor.
    pub fn set_handler(&mut self, handler: Option<Box<dyn IoHandler>>) {
        self.handler = handler;
    }

    /// Whether a handler is registered.
    pub fn has_handler(&self) -> bool {
        self.handler.is_some()
    }

    /// Queues a request.
    pub fn push(&mut self, request: IoRequest) {
        self.pending.push(request);
    }

    /// Requests waiting for a boundary.
    pub fn pending(&self) -> &[IoRequest] {
        &self.pending
    }

    /// Number of pending requests.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Drains and returns all pending requests (host polling path when no
    /// handler is registered).
    pub fn take_pending(&mut self) -> Vec<IoRequest> {
        std::mem::take(&mut self.pending)
    }

    /// Responses produced by the handler, oldest first.
    pub fn responses(&self) -> &[IoResponse] {
        &self.responses
    }

    /// Drains and returns all responses.
    pub fn take_responses(&mut self) -> Vec<IoResponse> {
        std::mem::take(&mut self.responses)
    }

    /// Records a host-fulfilled response (host-poll path).
    pub fn push_response(&mut self, response: IoResponse) {
        self.responses.push(response);
    }
}

impl Sim {
    /// Registers the host IO executor (plan 04 `MindIo` / tests).
    pub fn set_io_handler(&mut self, handler: Box<dyn IoHandler>) {
        self.io.set_handler(Some(handler));
    }

    /// Queues a save at the next `IoSet::Capture`.
    pub fn request_save(&mut self, path: impl Into<PathBuf>, as_map: bool) {
        self.io.push(IoRequest::Save {
            path: path.into(),
            as_map,
        });
    }

    /// Queues a load at the next `IoSet::Apply`.
    pub fn request_load(&mut self, path: impl Into<PathBuf>) {
        self.io.push(IoRequest::Load { path: path.into() });
    }

    /// Pending IO request count (host inspector).
    pub fn io_pending(&self) -> usize {
        self.io.pending_len()
    }

    /// Responses produced since the last drain.
    pub fn io_responses(&self) -> &[IoResponse] {
        self.io.responses()
    }

    /// Drains IO responses (host/MindIo).
    pub fn take_io_responses(&mut self) -> Vec<IoResponse> {
        self.io.take_responses()
    }

    /// Drains queued IO requests for the host-poll path (no handler registered).
    pub fn take_io_requests(&mut self) -> Vec<IoRequest> {
        self.io.take_pending()
    }

    /// Records a host-fulfilled response so `take_io_responses` can report it.
    pub fn deliver_io_response(&mut self, response: IoResponse) {
        self.io.push_response(response);
    }

    /// Runs the named IO boundary. Called by [`Sim::tick`] at the exact Java
    /// positions; there is no mid-tick IO (plan 04 §3.10).
    pub(crate) fn pump_io(&mut self, set: IoSet) {
        if self.io.handler.is_none() {
            // Nothing queued for this boundary; leave requests for the host.
            return;
        }
        let requests: Vec<IoRequest> = self
            .io
            .pending
            .iter()
            .filter(|request| request.set() == set)
            .cloned()
            .collect();
        if requests.is_empty() {
            return;
        }
        self.io.pending.retain(|request| request.set() != set);
        let mut handler = self.io.handler.take();
        let responses = match handler.as_mut() {
            Some(handler) => match set {
                IoSet::Apply => handler.apply(self, &requests),
                IoSet::Capture => handler.capture(self, &requests),
            },
            None => Vec::new(),
        };
        self.io.handler = handler;
        self.io.responses.extend(responses);
    }
}

/// Default executor used by tests/hosts that only care about boundary order.
/// Records each request as [`IoStatus::Deferred`] so the host can complete it.
#[derive(Debug, Default)]
pub struct DeferringIoHandler {
    /// Boundary calls in order (`(set, request_count)`).
    pub calls: Vec<(IoSet, usize)>,
}

impl IoHandler for DeferringIoHandler {
    fn apply(&mut self, _sim: &mut Sim, requests: &[IoRequest]) -> Vec<IoResponse> {
        self.calls.push((IoSet::Apply, requests.len()));
        requests
            .iter()
            .cloned()
            .map(|request| IoResponse {
                request,
                status: IoStatus::Deferred,
            })
            .collect()
    }

    fn capture(&mut self, _sim: &mut Sim, requests: &[IoRequest]) -> Vec<IoResponse> {
        self.calls.push((IoSet::Capture, requests.len()));
        requests
            .iter()
            .cloned()
            .map(|request| IoResponse {
                request,
                status: IoStatus::Deferred,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;

    #[test]
    fn io_boundaries_run_in_order_without_mid_tick_io() {
        let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
        sim.set_io_handler(Box::<DeferringIoHandler>::default());
        let run_dir = std::env::temp_dir().join("mind-io-seam");
        sim.request_load(run_dir.join("in.msav"));
        sim.request_save(run_dir.join("out.msav"), false);
        assert_eq!(sim.io_pending(), 2);

        sim.tick().expect("tick");

        // Both requests were processed by their boundary exactly once.
        assert_eq!(sim.io_pending(), 0);
        let responses = sim.take_io_responses();
        assert_eq!(responses.len(), 2);
        assert!(
            responses
                .iter()
                .all(|response| response.status == IoStatus::Deferred)
        );
        // Drain order: the load was applied before the save was captured.
        let kinds: Vec<IoSet> = responses.iter().map(|r| r.request.set()).collect();
        assert_eq!(kinds, vec![IoSet::Apply, IoSet::Capture]);
        assert!(sim.take_io_responses().is_empty());
    }

    #[test]
    fn requests_are_left_pending_without_a_handler() {
        let mut sim = Sim::new(1, 4, 4, BlockId::AIR, BlockId::AIR);
        sim.request_save(std::env::temp_dir().join("x.msav"), false);
        sim.tick().expect("tick");
        assert_eq!(sim.io_pending(), 1);
        assert!(!sim.io.has_handler());
    }
}

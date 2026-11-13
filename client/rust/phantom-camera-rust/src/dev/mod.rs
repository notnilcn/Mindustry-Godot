// SPDX-License-Identifier: GPL-3.0-only
//
//! Development nodes for the Phantom Camera port.
//!
//! Nothing in this module ports upstream code: the upstream GDScript addon has no
//! counterpart for a telemetry recorder or a probe. These nodes let dev tooling
//! observe a live camera rig from *inside* the engine without touching the shipped
//! classes.
//!
//! - [`recorder::PcamRecorder`] — per-frame camera + Phantom Camera JSONL
//!   recorder for offline debugging.
//!
//! `PcamRustProbe` lives in `crate::probe` because it is part of the shipped
//! class surface (`tools/ci.sh` asserts against it in editor-less runs too).

pub mod recorder;

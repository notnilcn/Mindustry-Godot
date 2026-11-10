// SPDX-License-Identifier: GPL-3.0-only

//! `audio` subcommands (plan 13).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `audio` subcommands (plan 18 §7b).
#[derive(Debug, Subcommand)]
pub enum AudioCommand {
    /// Emit the scripted block-place/break/shoot/loop/music event sequence and
    /// dump the `RecordingAudioSink` output (`events_blocks.json` shape).
    Events {
        /// Scenario name (only `audio_events_blocks` is defined).
        #[arg(long, default_value = "audio_events_blocks")]
        scenario: String,
        /// Write the event dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the dump against this golden (defaults to the audio golden dir).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run the scripted `SoundControl` context timeline and dump the selection
    /// trace (`music_select.json` shape).
    Music {
        /// Scenario name (only `audio_music_select` is defined).
        #[arg(long, default_value = "audio_music_select")]
        scenario: String,
        /// Frames to run.
        #[arg(long, default_value_t = 600)]
        frames: u32,
        /// Deterministic audio-RNG seed.
        #[arg(long, default_value_t = 7)]
        seed: u64,
        /// Write the trace dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare against this golden (defaults to the audio golden dir).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run the scripted loop aggregation and dump per-tick rows
    /// (`loops_aggregate.json` shape).
    Loops {
        /// Scenario name (only `audio_loop_aggregate` is defined).
        #[arg(long, default_value = "audio_loop_aggregate")]
        scenario: String,
        /// Ticks to run.
        #[arg(long, default_value_t = 120)]
        ticks: u32,
        /// Write the loop dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare against this golden (defaults to the audio golden dir).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Run the `SoundPriority` admission/eviction policy over a synthetic or
    /// fixture request stream (`--requests`).
    Policy {
        /// Request fixture JSON (defaults to the built-in synthetic stream).
        #[arg(long)]
        requests: Option<PathBuf>,
        /// Write the decision dump here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare against this golden (defaults to the audio golden dir).
        #[arg(long)]
        golden: Option<PathBuf>,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },

    /// Measure the Rust mix update (`MusicPlayer` + `LoopMixer` + drain).
    Bench {
        /// Timed ticks.
        #[arg(long, default_value_t = 3600)]
        ticks: u64,
        /// Live one-shot voices.
        #[arg(long, default_value_t = 128)]
        voices: usize,
        /// Loop sounds to aggregate per tick.
        #[arg(long, default_value_t = 256)]
        loops: usize,
        /// Ambient candidate count (0 disables).
        #[arg(long, default_value_t = 0)]
        ambient: usize,
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}

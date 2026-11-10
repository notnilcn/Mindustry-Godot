// SPDX-License-Identifier: GPL-3.0-only

//! `ui` subcommands (plan 14 §7b).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use std::path::PathBuf;

use clap::Subcommand;

/// `ui` subcommands (plan 14 §7b).
#[derive(Debug, Subcommand)]
pub enum UiCommand {
    /// Render the text/markup corpus and compare against a golden.
    Text {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the rendered corpus JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the rendered corpus against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Parse → write → parse every DSL fixture and compare the normalized output.
    Dsl {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the normalized fixture JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the normalized output against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Encode/decode a menu tree and dump it.
    MenuTree {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the menu-tree JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the menu-tree JSON against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Validate `dialogs_manifest.json` + `styles_manifest.json`.
    Manifest {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Repo root override (defaults to discovery from cwd).
        #[arg(long)]
        repo: Option<PathBuf>,
    },

    /// Compose HUD status/objective text across wave/mission/attack fixtures.
    HudText {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the composed-text JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the composed text against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Render `Displayable`/`StatValues` rows across display-kind fixtures.
    Display {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the rendered-rows JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the rendered rows against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Project the plan-12 campaign state into the M5 dialog read models.
    Campaign {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the campaign-views JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the campaign views against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Validate a `FileChooserParams` request set (plan 14 M7).
    FileChooser {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the file-chooser JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the file-chooser output against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Exercise the Rust console command registry + chat validation (plan 14 M7).
    ChatConsole {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the chat/console JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the chat/console output against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Materialize an MSUI tree through the Godot-free `dsl_factory` (plan 14 M6).
    Builder {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the materialized-tree JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the materialized tree against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Drive the server-menu host lifecycle + plan-21 relay handshake (plan 14 M6).
    MenuHost {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the menu-host JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the menu-host output against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Check the M6 relay codec bytes against the committed `relay_wire.hex`.
    Relay {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the decoded-fixture JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// `relay_wire.hex` to compare against.
        #[arg(long)]
        golden: Option<PathBuf>,
    },

    /// Exercise the Godot-free prompt-helper model (plan 14 §2.1 item 4).
    Prompts {
        /// Emit a machine-readable JSON report on stdout.
        #[arg(long)]
        json: bool,
        /// Write the prompt-model JSON here.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Compare the prompt model against this golden.
        #[arg(long)]
        golden: Option<PathBuf>,
    },
}

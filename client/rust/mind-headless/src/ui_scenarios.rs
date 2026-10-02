// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/*, core/src/mindustry/core/UI.java.

//! `mind-headless ui` scenarios (plan 14 §7b).
//!
//! These exercise the Godot-free `mind-core::ui` surface with committed
//! goldens: text/markup rendering, DSL parse→write→parse, menu-tree wire
//! round-trip, and manifest validation. No Godot, network or assets required.

use std::path::Path;

use anyhow::{Context, Result, bail};
use mind_core::assets::icons::Iconc;
use mind_core::ui::builder::dsl;
use mind_core::ui::builder::dsl_writer;
use mind_core::ui::builder::tree_builder::{BuildContext, dump_json};
use mind_core::ui::builder::ui_node::UiNode;
use mind_core::ui::manifest::{DialogsManifest, StylesManifest};
use mind_core::ui::text::{
    AmountWords, format_amount, format_icons, format_time, render_markup, round_amount,
};
use serde_json::{Value, json};

use crate::cli::UiCommand;

/// Fixture `Iconc` table used by the headless corpora (matches the unit tests).
const FIXTURE_ICONS: &str = "63743=spawn|block-spawn-ui\n63742=copper|item-copper-ui\n";

/// Runs a `ui` subcommand.
pub fn run(command: &UiCommand) -> Result<()> {
    match command {
        UiCommand::Text { json, dump, golden } => text(*json, dump.as_deref(), golden.as_deref()),
        UiCommand::Dsl { json, dump, golden } => {
            dsl_scenario(*json, dump.as_deref(), golden.as_deref())
        }
        UiCommand::MenuTree { json, dump, golden } => {
            menu_tree(*json, dump.as_deref(), golden.as_deref())
        }
        UiCommand::Manifest { json, repo } => manifest(*json, repo.as_deref()),
        UiCommand::HudText { json, dump, golden } => {
            hud_text(*json, dump.as_deref(), golden.as_deref())
        }
        UiCommand::Display { json, dump, golden } => {
            display(*json, dump.as_deref(), golden.as_deref())
        }
    }
}

/// `ui display`: renders the `StatValues`/`Displayable` display kinds to
/// `(left, right)` rows using an inline bundle + iconc fixture (plan §7a).
fn display(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    use mind_core::assets::bundle::{Bundle, parse_properties};
    use mind_core::ui::display::{DisplayRow, Displayable, HoverInfo};
    use mind_core::ui::stat_display::{StatDisplay, StatUnit, display_rows};

    let bundle = Bundle::from_layers(vec![parse_properties(DISPLAY_BUNDLE)]);
    let iconc = Iconc::from_properties(FIXTURE_ICONS);

    let cases: Vec<(&str, StatDisplay)> = vec![
        ("text", StatDisplay::Text(String::from(":copper: holds"))),
        (
            "number",
            StatDisplay::Number {
                value: 2.5,
                unit: StatUnit::PowerSecond,
            },
        ),
        (
            "multiplier",
            StatDisplay::Multiplier {
                value: 0.5,
                unit: StatUnit::Multiplier,
            },
        ),
        (
            "percent",
            StatDisplay::Percent {
                value: 1.1,
                unit: StatUnit::Percent,
            },
        ),
        (
            "squared",
            StatDisplay::Squared {
                value: 2.0,
                unit: StatUnit::Tiles,
            },
        ),
        (
            "items",
            StatDisplay::Items {
                items: vec![(String::from("copper"), 1500), (String::from("lead"), 12)],
            },
        ),
        (
            "status",
            StatDisplay::Status {
                statuses: vec![(String::from("spawn"), 120.0)],
            },
        ),
    ];
    let results: Vec<Value> = cases
        .iter()
        .map(|(name, stat)| {
            let info = display_rows(stat, &bundle, &iconc);
            let rows: Vec<Value> = info
                .to_text_rows(&bundle, &iconc)
                .into_iter()
                .map(|(left, right)| json!({"left": left, "right": right}))
                .collect();
            json!({"name": name, "rows": rows})
        })
        .collect();

    // Also exercise the `Displayable` trait with a mixed-rows fixture.
    struct Fixture;
    impl Displayable for Fixture {
        fn hover_info(&self) -> HoverInfo {
            HoverInfo {
                rows: vec![
                    DisplayRow::Text(String::from("Copper Wall")),
                    DisplayRow::Bar {
                        label: String::from("Health"),
                        color: [1.0, 0.0, 0.0, 1.0],
                        fraction: 0.75,
                    },
                ],
            }
        }
    }
    let mixed: Vec<Value> = Fixture
        .hover_info()
        .to_text_rows(&bundle, &iconc)
        .into_iter()
        .map(|(left, right)| json!({"left": left, "right": right}))
        .collect();

    finish(
        json!({"format": 1, "cases": results, "mixed": mixed}),
        json_out,
        dump,
        golden,
    )
}

/// Inline bundle fixture for `ui display` (`StatUnit` labels).
const DISPLAY_BUNDLE: &str = "unit.power=Power\nunit.powersec=Power/sec\nunit.tiles=Tiles\nunit.percent=Percent\nunit.multiplier=Multiplier\nunit.seconds=Seconds\nunit.billions=B\nunit.millions=M\nunit.thousands=k\n";

fn hud_text(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    use mind_core::assets::bundle::{Bundle, parse_properties};
    use mind_core::ui::hud_text::{HudStatus, status_text};

    let bundle = Bundle::from_layers(vec![parse_properties(HUD_TEXT_BUNDLE)]);
    let iconc = Iconc::from_properties(FIXTURE_ICONS);
    let cases: Vec<(&str, HudStatus)> = vec![
        (
            "wave_timer",
            HudStatus {
                waves: true,
                wave: 5,
                win_wave: 0,
                enemies: 0,
                wave_timer: true,
                wavetime_ticks: 300.0,
                ..Default::default()
            },
        ),
        (
            "wave_cap_enemies",
            HudStatus {
                waves: true,
                wave: 3,
                win_wave: 10,
                enemies: 4,
                wave_timer: false,
                ..Default::default()
            },
        ),
        (
            "attack_mode",
            HudStatus {
                waves: false,
                attack_mode: true,
                enemy_cores: 3,
                ..Default::default()
            },
        ),
        (
            "objectives",
            HudStatus {
                objectives: vec!["build a core".to_owned(), "defend :spawn:".to_owned()],
                ..Default::default()
            },
        ),
        (
            "mission",
            HudStatus {
                mission: "Hold the line".to_owned(),
                ..Default::default()
            },
        ),
        (
            "unit_activation",
            HudStatus {
                unit_activation_remaining: Some(3600.0),
                ..Default::default()
            },
        ),
    ];
    let results: Vec<Value> = cases
        .iter()
        .map(|(name, status)| json!({"name": name, "text": status_text(status, &bundle, &iconc)}))
        .collect();
    finish(
        json!({"format": 1, "cases": results}),
        json_out,
        dump,
        golden,
    )
}

/// Inline bundle fixture for `ui hud-text` (upstream `bundle.properties` keys).
const HUD_TEXT_BUNDLE: &str = "wave=Wave {0}\nwave.cap=Wave {0} / {1}\nwave.enemy=Enemy {0}\nwave.enemies=Enemies {0}\nwave.enemycore=Enemy Core {0}\nwave.enemycores=Enemy Cores {0}\nwave.waiting=Next wave in {0}\nwave.waveInProgress=Wave in progress\nwaiting=Waiting\nsector.curcapture=Capturing sector\nrules.unitfactoryactivation.objective=Activate a unit factory in {0}\n";

fn text(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let iconc = Iconc::from_properties(FIXTURE_ICONS);
    let icons: Vec<(String, String)> = [
        "no icons here",
        ":spawn:",
        "use :spawn: now",
        "a :nope: b",
        "http://x",
    ]
    .into_iter()
    .map(|input| (input.to_owned(), format_icons(input, &iconc)))
    .collect();
    let markup: Vec<(String, String)> = [
        "[accent]hello[]",
        "[red]a[gray]b[]c",
        "[bogus]x",
        ":copper: x",
    ]
    .into_iter()
    .map(|input| (input.to_owned(), render_markup(input, &iconc)))
    .collect();
    let time: Vec<(f32, String)> = [0.0, 60.0, 3660.0, 221_160.0]
        .into_iter()
        .map(|ticks| (ticks, format_time(ticks)))
        .collect();
    let words = AmountWords::default();
    let amount: Vec<(i64, String)> = [0, 999, 1500, 12_345, 1_500_000, 2_000_000_000, i64::MAX]
        .into_iter()
        .map(|number| (number, format_amount(number, words)))
        .collect();
    let round: Vec<(i32, i32)> = [0, 9, 12, 150, 1234, 12_345, 1_234_567]
        .into_iter()
        .map(|number| (number, round_amount(number)))
        .collect();

    let value = json!({
        "format": 1,
        "icons": icons.iter().map(|(i, o)| json!({"input": i, "output": o})).collect::<Vec<_>>(),
        "markup": markup.iter().map(|(i, o)| json!({"input": i, "output": o})).collect::<Vec<_>>(),
        "time": time.iter().map(|(t, o)| json!({"ticks": t, "output": o})).collect::<Vec<_>>(),
        "amount": amount.iter().map(|(n, o)| json!({"number": n, "output": o})).collect::<Vec<_>>(),
        "round": round.iter().map(|(n, o)| json!({"number": n, "output": o})).collect::<Vec<_>>(),
    });
    finish(value, json_out, dump, golden)
}

fn dsl_scenario(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    // Inline corpus: parse -> write -> parse must be structurally stable, and
    // the normalized writer output is the golden.
    let corpus = [
        ("shorthand", "label: \"Hello\"\nspace\nbutton: @ok\n"),
        (
            "table_block",
            "table {\n  background: grayPanel\n  margin: 4\n  row\n  label: \"a b\"\n}\n",
        ),
        (
            "button_block",
            "button: @ok {\n  clicked: \"yes\"\n  width: 100\n}\n",
        ),
        (
            "slider",
            "slider: \"vol\" {\n  min: 0\n  max: 1\n  step: 0.1\n}\n",
        ),
    ];
    let mut entries = Vec::new();
    for (name, source) in corpus {
        let tree = dsl::parse(source).with_context(|| format!("parse fixture '{name}'"))?;
        let written = dsl_writer::write(&tree);
        let reparsed = dsl::parse(&written).with_context(|| format!("reparse fixture '{name}'"))?;
        if tree != reparsed {
            bail!("dsl round-trip mismatch for fixture '{name}'");
        }
        entries.push(json!({"name": name, "source": source, "normalized": written}));
    }
    let value = json!({"format": 1, "fixtures": entries});
    finish(value, json_out, dump, golden)
}

fn menu_tree(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let tree = dsl::parse(
        "table {\n  background: grayPanel\n  row\n  label: \"Hi\" { id: \"title\" }\n  slider: \"vol\" { id: \"vol\" min: 0 max: 1 }\n  image: net-icon\n}\n",
    )?;
    let ctx = BuildContext::default();
    let dump_value = dump_json(&tree, &ctx);
    let bytes = tree.encode_message();
    let decoded = UiNode::decode_message(&bytes)
        .map_err(|error| anyhow::anyhow!("wire decode failed: {error}"))?;
    if decoded != tree {
        bail!("menu-tree wire round-trip mismatch");
    }
    let value = json!({
        "format": 1,
        "wire_len": bytes.len(),
        "wire_hex": hex(&bytes),
        "tree": dump_value,
    });
    finish(value, json_out, dump, golden)
}

fn manifest(json_out: bool, repo: Option<&Path>) -> Result<()> {
    let repo = match repo {
        Some(path) => path.to_path_buf(),
        None => crate::paths::find_repo_root(None).context("discover repo root")?,
    };
    let dialogs_path = repo.join("client/ui/dialogs_manifest.json");
    let styles_path = repo.join("client/ui/styles_manifest.json");
    let dialogs_text = std::fs::read_to_string(&dialogs_path)
        .with_context(|| format!("read {}", dialogs_path.display()))?;
    let styles_text = std::fs::read_to_string(&styles_path)
        .with_context(|| format!("read {}", styles_path.display()))?;
    let dialogs = DialogsManifest::from_json(&dialogs_text)
        .with_context(|| format!("parse {}", dialogs_path.display()))?;
    let styles = StylesManifest::from_json(&styles_text)
        .with_context(|| format!("parse {}", styles_path.display()))?;
    dialogs
        .validate(Some(&repo.join("client")))
        .context("validate dialogs_manifest")?;
    styles.validate().context("validate styles_manifest")?;
    let value = json!({
        "format": 1,
        "dialogs": dialogs.dialogs.len(),
        "fragments": dialogs.fragments.len(),
        "prompts": dialogs.prompts.len(),
        "styles": mind_core::ui::manifest::EXPECTED_STYLES.len(),
    });
    finish(value, json_out, None, None)
}

/// Emits `value`, optionally dumping and/or comparing against a golden.
fn finish(value: Value, json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    let rendered = serde_json::to_string_pretty(&value)?;
    if let Some(path) = dump {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, format!("{rendered}\n"))
            .with_context(|| format!("write {}", path.display()))?;
    }
    if let Some(path) = golden {
        let expected = std::fs::read_to_string(path)
            .with_context(|| format!("read golden {}", path.display()))?;
        let expected: Value = serde_json::from_str(&expected)
            .with_context(|| format!("parse golden {}", path.display()))?;
        if expected != value {
            bail!("golden mismatch: {}", path.display());
        }
    }
    if json_out {
        println!("{rendered}");
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

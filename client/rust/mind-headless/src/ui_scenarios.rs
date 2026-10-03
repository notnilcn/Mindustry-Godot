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
        UiCommand::Campaign { json, dump, golden } => {
            campaign(*json, dump.as_deref(), golden.as_deref())
        }
        UiCommand::FileChooser { json, dump, golden } => {
            file_chooser(*json, dump.as_deref(), golden.as_deref())
        }
        UiCommand::ChatConsole { json, dump, golden } => {
            chat_console(*json, dump.as_deref(), golden.as_deref())
        }
        UiCommand::Builder { json, dump, golden } => {
            builder(*json, dump.as_deref(), golden.as_deref())
        }
        UiCommand::MenuHost { json, dump, golden } => {
            menu_host(*json, dump.as_deref(), golden.as_deref())
        }
        UiCommand::Relay { json, dump, golden } => relay(*json, dump.as_deref(), golden.as_deref()),
        UiCommand::Prompts { json, dump, golden } => {
            prompts(*json, dump.as_deref(), golden.as_deref())
        }
    }
}

/// `ui relay`: every M6 relay payload encoded from the fixed fixture tree must
/// match the committed `relay_wire.hex` bytes (plan 14 §6.6 byte stability).
fn relay(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    use mind_core::ui::builder::ui_relay::{parse_wire_golden, to_hex, wire_fixtures};

    let fixtures = wire_fixtures();
    if let Some(path) = golden {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read relay golden {}", path.display()))?;
        let expected = parse_wire_golden(&text);
        if expected.len() != fixtures.len() {
            bail!(
                "relay golden {} has {} fixtures, expected {}",
                path.display(),
                expected.len(),
                fixtures.len()
            );
        }
        for (actual, (expected_name, expected_bytes)) in fixtures.iter().zip(expected.iter()) {
            if actual.0 != expected_name {
                bail!(
                    "relay golden order mismatch: expected '{expected_name}', got '{}'",
                    actual.0
                );
            }
            if actual.1 != *expected_bytes {
                bail!("relay byte mismatch for '{}'", actual.0);
            }
        }
    }
    let value = json!({
        "format": 1,
        "fixtures": fixtures
            .iter()
            .map(|(name, bytes)| json!({"name": name, "len": bytes.len(), "hex": to_hex(bytes)}))
            .collect::<Vec<_>>(),
    });
    finish(value, json_out, dump, None)
}

/// `ui prompts`: the Godot-free prompt-helper model (catalogue, text-input
/// gating/filtering, confirm specs, popup id replacement, announcement
/// tracking). Plan 14 §2.1 item 4.
fn prompts(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    use mind_core::ui::prompts::{
        AnnouncementTracker, ConfirmOutcome, ConfirmSpec, PROMPT_HELPERS, PopupEntry,
        PopupRegistry, TextInputOutcome, TextInputSpec,
    };

    let catalogue: Vec<Value> = PROMPT_HELPERS
        .iter()
        .filter_map(|name| {
            let kind = mind_core::ui::prompts::PromptKind::from_name(name)?;
            Some(json!({
                "name": name,
                "modal": kind.is_modal(),
                "pause": kind.should_pause(),
                "id_addressable": kind.is_id_addressable(),
            }))
        })
        .collect();

    let numeric = TextInputSpec::new("@name", "Enter a number", 5, "", true, false);
    let lenient = TextInputSpec::new("@name", "Enter text", 4, "abcd", false, true);
    let outcome_name = |outcome: TextInputOutcome| match outcome {
        TextInputOutcome::Cancelled => "cancelled",
        TextInputOutcome::Submitted(_) => "submitted",
        TextInputOutcome::Rejected => "rejected",
    };
    let text_input = json!([
        {
            "name": "numeric_strict",
            "filtered": numeric.filter("a1b2c3d4e5f6"),
            "allows_char": numeric.permits('7') && !numeric.permits('x'),
            "cancel": outcome_name(numeric.outcome(None)),
            "empty": outcome_name(numeric.outcome(Some(""))),
            "submitted": outcome_name(numeric.outcome(Some("12"))),
            "relay_cancel": numeric.relay_result(None),
            "relay_ok": numeric.relay_result(Some("12")),
        },
        {
            "name": "text_allow_empty",
            "filtered": lenient.filter("abcdef"),
            "cancel": outcome_name(lenient.outcome(None)),
            "empty": outcome_name(lenient.outcome(Some(""))),
            "relay_empty": lenient.relay_result(Some("")),
        },
    ]);

    let confirm = ConfirmSpec::confirm("@confirm", "sure?");
    let custom = ConfirmSpec::custom("@t", "body", "Yes!", "Nope");
    let confirm_json = json!({
        "yes": confirm.yes,
        "no": confirm.no,
        "confirmed": match ConfirmSpec::outcome(true) { ConfirmOutcome::Confirmed => "confirmed", ConfirmOutcome::Cancelled => "cancelled" },
        "cancelled": match ConfirmSpec::outcome(false) { ConfirmOutcome::Confirmed => "confirmed", ConfirmOutcome::Cancelled => "cancelled" },
        "custom_yes": custom.yes,
        "custom_no": custom.no,
        "ok_no": ConfirmSpec::ok("@t", "body").no,
    });

    let entry = PopupEntry {
        message: String::new(),
        duration: 2.0,
        align: 1,
        top: 0,
        left: 0,
        bottom: 0,
        right: 0,
    };
    let mut popups = PopupRegistry::new();
    let first = popups.show("first", Some("p"), entry.clone()).is_none();
    let replaced = popups
        .show("second", Some("p"), entry.clone())
        .map(|old| old.message);
    let transient_ignored = popups
        .show("transient", Option::<String>::None, entry)
        .is_none();
    let removed = popups.remove("p").map(|old| old.message);
    let popup_json = json!({
        "first_is_none": first,
        "replaced": replaced,
        "transient_ignored": transient_ignored,
        "removed": removed,
        "len_after_remove": popups.len(),
    });

    let mut announcements = AnnouncementTracker::new();
    let initial = announcements.has_announcement();
    announcements.announce();
    let after = announcements.has_announcement();
    announcements.clear();
    let cleared = announcements.has_announcement();
    let announce_json = json!({
        "initial": initial,
        "after_announce": after,
        "after_clear": cleared,
    });

    let value = json!({
        "format": 1,
        "catalogue": catalogue,
        "text_input": text_input,
        "confirm": confirm_json,
        "popup": popup_json,
        "announcement": announce_json,
    });
    finish(value, json_out, dump, golden)
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

/// `ui campaign`: projects the plan-12 campaign fixture into the M5 dialog read
/// models and locks counts + a content checksum against the committed golden.
fn campaign(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    use mind_core::ui::campaign::CampaignViews;

    let views = CampaignViews::vanilla_fixture();
    let full = serde_json::to_value(&views)?;
    let checksum = fnv_hex(&serde_json::to_vec(&full)?);
    let sample_sector = views
        .sectors
        .iter()
        .find(|sector| sector.preset.as_deref() == Some("groundZero"))
        .or_else(|| views.sectors.first());
    let sample_research = views
        .research
        .iter()
        .find(|node| !node.requirements.is_empty())
        .or_else(|| views.research.first());
    let value = json!({
        "format": 1,
        "planet": views.planet,
        "counts": {
            "planets": views.planets.len(),
            "sectors": views.sectors.len(),
            "research": views.research.len(),
            "schematics": views.schematics.len(),
            "loadouts": views.loadouts.len(),
            "maps": views.maps.len(),
        },
        "planets": views.planets.iter().map(|planet| planet.name.clone()).collect::<Vec<_>>(),
        "rules": views.rules,
        "schematics": views.schematics,
        "sample_sector": sample_sector,
        "sample_research": sample_research,
        "complete": views.complete,
        "checksum": checksum,
    });
    finish(value, json_out, dump, golden)
}

/// `ui file-chooser`: validates `FileChooserParams` request resolution.
fn file_chooser(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    use mind_core::content::MemoryBundle;
    use mind_core::ui::file_chooser::{FileChooserParams, ext_equals, sanitize_filename};

    let bundle = MemoryBundle::with_pairs([("open", "Open File"), ("save", "Save File")]);
    let mut open = FileChooserParams::open(&["msch"]).with_name("my base:1");
    open.submit(&bundle, "import_schematic").ok();
    let mut save = FileChooserParams::save(&["png", "jpg"]);
    save.check_params(&bundle).ok();
    let missing = FileChooserParams::new().check_params(&bundle).is_err();
    let value = json!({
        "format": 1,
        "open": {
            "title": open.title,
            "file_name": open.file_name,
            "accepts_msch": open.accepts("x.msch"),
            "accepts_png": open.accepts("x.png"),
            "allow_multiple": open.allow_multiple,
        },
        "save": {
            "title": save.title,
            "file_name": save.file_name,
            "target": save.save_target_name("shot.jpeg"),
        },
        "sanitized": sanitize_filename("a b/c:d"),
        "ext_equals": ext_equals("MAP.msav", "msav"),
        "missing_extensions_rejected": missing,
    });
    finish(value, json_out, dump, golden)
}

/// `ui chat-console`: exercises the chat state machine and console registry.
fn chat_console(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    use mind_core::ui::chat::{ChatMode, ChatState, check_ping};
    use mind_core::ui::console::ConsoleRegistry;

    let mut chat = ChatState::new();
    chat.mode = ChatMode::Team;
    let prefix_only = chat.send("/t", false, (100, 100)).is_none();
    let sent = chat.send("5,6 [help]", false, (100, 100));
    let ping = sent.as_ref().and_then(|send| send.ping.clone());
    let mode_cycle: Vec<&'static str> = {
        let mut state = ChatState::new();
        let mut input = String::new();
        let mut modes = Vec::new();
        for _ in 0..3 {
            input = state.next_mode(false, &input);
            modes.push(state.mode.prefix());
        }
        modes
    };
    let out_of_bounds = check_ping("200,2 [x]", 100, 100).is_none();

    let mut registry = ConsoleRegistry::with_defaults();
    registry.register("sum", "sum <a> <b>", "add two integers", |args| {
        let a: i32 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let b: i32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        format!("{}", a + b)
    });
    let sum = registry.execute("sum 3 4").output;
    let unknown = registry.execute("nope").output;
    let help_nonempty = !registry.help_text().is_empty();

    let value = json!({
        "format": 1,
        "chat": {
            "prefix_only_rejected": prefix_only,
            "sent_message": sent.map(|send| send.message),
            "ping": ping,
            "mode_cycle": mode_cycle,
            "out_of_bounds_ping_rejected": out_of_bounds,
        },
        "console": {
            "commands": registry.len(),
            "sum": sum,
            "unknown": unknown,
            "help_nonempty": help_nonempty,
        },
    });
    finish(value, json_out, dump, golden)
}

/// Inline styles fixture for `ui builder` (hermetic: the committed
/// `styles_manifest.json` is validated separately by `ui manifest`).
const BUILDER_STYLES: &str = r#"{"format":1,"drawables":["black","grayPanel","none"],"text_buttons":["defaultt","grayt"],"labels":["defaultLabel","outlineLabel"],"sliders":["defaultSlider"],"panes":["defaultPane"],"checks":["defaultCheck"]}"#;

/// `ui builder`: materialize an MSUI tree through the Godot-free `dsl_factory`
/// (style resolution, conditions, id/image collection) and report style lookup
/// plus hot-reload parse diagnostics (plan 14 M6).
fn builder(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    use mind_core::ui::builder::dsl_factory::dump_json as factory_json;
    use mind_core::ui::builder::hot_reload::{error_line, error_source_line, parse_source};
    use mind_core::ui::builder::style_lookup::{StyleKind, StyleLookup};
    use mind_core::ui::manifest::StylesManifest;

    let manifest = StylesManifest::from_json(BUILDER_STYLES).context("parse builder styles")?;
    let lookup = StyleLookup::new(&manifest);
    let source = "background: grayPanel\nrow\nlabel: \"Title\" { id: \"title\" style: \"defaultLabel\" }\nslider: \"vol\" { id: \"vol\" min: 0 max: 1 defaultValue: 0.5 style: \"defaultSlider\" }\nbutton: \"Buy\" { style: \"grayt\" clicked: \"buy\" icon: net-badge }\nimage: net-icon\ncheck: \"enabled\" { id: \"enabled\" checked: true }\nbutton: \"hidden\" { condition: \"landscape\" }\nbutton: \"bad\" { style: \"missingStyle\" }\n";
    let tree = dsl::parse(source).context("parse builder fixture")?;
    let ctx = BuildContext {
        portrait: true,
        width: 720.0,
        height: 1280.0,
    };
    let parsed = match parse_source("label: \"a\"\nbogus: 2\n") {
        Ok(_) => bail!("expected the bogus fixture to fail parsing"),
        Err(error) => error,
    };
    let value = json!({
        "format": 1,
        "factory": factory_json(&tree, &manifest, &ctx),
        "style_lookup": {
            "grayt": lookup.resolve("grayt").map(StyleKind::name),
            "defaultLabel": lookup.resolve("defaultLabel").map(StyleKind::name),
            "missingStyle": lookup.resolve("missingStyle").map(StyleKind::name),
        },
        "hot_reload": {
            "valid": parse_source(source).is_ok(),
            "error_line": parsed.line,
            "error_source": parsed.source_line,
            "extracted_line": error_line("Unknown property at line 2"),
            "extracted_source": error_source_line("label: \"a\"\nbogus: 2\n", "Unknown property at line 2"),
        },
    });
    finish(value, json_out, dump, golden)
}

/// `ui menu-host`: drive the server-menu lifecycle and the plan-21 relay
/// handshake — `match_ui_event` (`show`/`update`/`hide`) in, `MenuBuilderChoose`
/// bytes out (plan 14 M6, never faked transport).
fn menu_host(json_out: bool, dump: Option<&Path>, golden: Option<&Path>) -> Result<()> {
    use mind_core::ui::builder::menu_host::{MenuHost, MenuHostEvent, MenuSelection};
    use mind_core::ui::builder::menu_result::MenuValue;
    use mind_core::ui::builder::ui_key::UiKey;
    use mind_core::ui::builder::ui_relay::{
        MenuBuilderShow, MenuBuilderUpdate, decode_menu_builder_choose, encode_menu_builder_hide,
        encode_menu_builder_show, encode_menu_builder_update,
    };

    let ctx = BuildContext::default();
    let show = MenuBuilderShow {
        id: 1,
        token: 42,
        title: Some("Shop".to_owned()),
        hide_on_click: false,
        hide_existing: true,
        fill_screen: false,
        ui: UiNode::new(UiKey::Table)
            .child(
                UiNode::new(UiKey::Label)
                    .str(UiKey::Text, "Shop")
                    .str(UiKey::Id, "title"),
            )
            .child(
                UiNode::new(UiKey::Slider)
                    .str(UiKey::Id, "amount")
                    .f32(UiKey::Min, 0.0)
                    .f32(UiKey::Max, 5.0)
                    .f32(UiKey::DefaultValue, 2.5),
            )
            .child(
                UiNode::new(UiKey::Field)
                    .str(UiKey::Id, "name")
                    .str(UiKey::Text, "base"),
            )
            .child(
                UiNode::new(UiKey::Button)
                    .str(UiKey::Text, "Buy")
                    .str(UiKey::Clicked, "buy"),
            ),
    };

    let mut host = MenuHost::new();
    let show_events = host
        .apply_event("menu_builder_show", &encode_menu_builder_show(&show), &ctx)
        .context("apply show")?;
    let ids = host
        .get(1)
        .map(|entry| entry.ids.clone())
        .unwrap_or_default();

    let choose_events = host.choose(
        1,
        MenuSelection::new("buy")
            .with("amount", MenuValue::F32(2.5))
            .with("name", MenuValue::Str("base".to_owned())),
    );
    let command = choose_events.iter().find_map(MenuHostEvent::relay_command);
    let choose = match &command {
        Some(command) => {
            let bytes = command.encode();
            let decoded =
                decode_menu_builder_choose(&bytes).context("decode menu_builder_choose")?;
            json!({
                "kind": command.kind_name(),
                "bytes": bytes.len(),
                "roundtrip": decoded.result.token == 42
                    && decoded.result.is("buy")
                    && decoded.result.get_f32("amount") == 2.5
                    && decoded.result.get_str("name") == Some("base"),
            })
        }
        None => Value::Null,
    };

    let update = MenuBuilderUpdate {
        id: 1,
        table_id: "body".to_owned(),
        ui: UiNode::new(UiKey::Table),
    };
    let update_events = host
        .apply_event(
            "menu_builder_update",
            &encode_menu_builder_update(&update),
            &ctx,
        )
        .context("apply update")?;
    let hide_events = host
        .apply_event("menu_builder_hide", &encode_menu_builder_hide(1), &ctx)
        .context("apply hide")?;

    // A menu closed with no choice produces a cancelled result (token only).
    let mut cancelled_host = MenuHost::new();
    cancelled_host
        .apply_event(
            "menu_builder_show",
            &encode_menu_builder_show(&MenuBuilderShow {
                id: 2,
                token: 9,
                title: None,
                hide_on_click: true,
                hide_existing: true,
                fill_screen: false,
                ui: UiNode::new(UiKey::Table),
            }),
            &ctx,
        )
        .context("apply cancelled show")?;
    let cancelled_events = cancelled_host
        .apply_event("menu_builder_hide", &encode_menu_builder_hide(2), &ctx)
        .context("apply cancelled hide")?;
    let cancelled_token = cancelled_events
        .iter()
        .find_map(MenuHostEvent::relay_command)
        .map(|command| match command {
            mind_core::ui::builder::ui_relay::RelayCommand::MenuBuilderChoose(value) => {
                value.result.token
            }
            _ => 0,
        })
        .unwrap_or(0);
    let unknown_event_error = host.apply_event("hud_text", &[], &ctx).is_err();

    let value = json!({
        "format": 1,
        "show": show_events.iter().map(event_name).collect::<Vec<_>>(),
        "ids": ids,
        "choose": choose,
        "update": update_events.iter().map(event_name).collect::<Vec<_>>(),
        "hide": hide_events.iter().map(event_name).collect::<Vec<_>>(),
        "cancelled": cancelled_events.iter().map(event_name).collect::<Vec<_>>(),
        "cancelled_token": cancelled_token,
        "unknown_event_error": unknown_event_error,
    });
    finish(value, json_out, dump, golden)
}

/// Compact event description for the `ui menu-host` golden.
fn event_name(event: &mind_core::ui::builder::menu_host::MenuHostEvent) -> String {
    use mind_core::ui::builder::menu_host::MenuHostEvent;
    match event {
        MenuHostEvent::Show { id, had_previous } => format!("show:{id}:{had_previous}"),
        MenuHostEvent::Update { id, table_id } => format!("update:{id}:{table_id}"),
        MenuHostEvent::Hide { id } => format!("hide:{id}"),
        MenuHostEvent::Choose { menu_id, result } => format!(
            "choose:{menu_id}:{}",
            result.result.as_deref().unwrap_or("<cancelled>")
        ),
    }
}

/// FNV-1a hex over bytes (canonical checksum helper).
fn fnv_hex(bytes: &[u8]) -> String {
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write(bytes);
    hasher.finish().to_hex()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Committed UI golden path (`mind-core/tests/goldens/ui`).
    fn golden(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../mind-core/tests/goldens/ui")
            .join(name)
    }

    /// Runs a golden-bearing scenario against its committed golden; the helper
    /// bails with `golden mismatch` when the output drifts.
    #[test]
    fn ui_goldens_match() {
        text(false, None, Some(&golden("text.json"))).unwrap();
        dsl_scenario(false, None, Some(&golden("dsl.json"))).unwrap();
        menu_tree(false, None, Some(&golden("menu_tree.json"))).unwrap();
        hud_text(false, None, Some(&golden("hud_text.json"))).unwrap();
        display(false, None, Some(&golden("display.json"))).unwrap();
        campaign(false, None, Some(&golden("campaign.json"))).unwrap();
        file_chooser(false, None, Some(&golden("file_chooser.json"))).unwrap();
        chat_console(false, None, Some(&golden("chat_console.json"))).unwrap();
        builder(false, None, Some(&golden("builder.json"))).unwrap();
        menu_host(false, None, Some(&golden("menu_host.json"))).unwrap();
        prompts(false, None, Some(&golden("prompts.json"))).unwrap();
        relay(false, None, Some(&golden("relay_wire.hex"))).unwrap();
    }
}

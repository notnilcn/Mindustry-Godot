# Twin verification — phase 1 orchestrator report (ORCH-1)

- Date: 2026-11-03 (Australia/Brisbane), branch `main`, base commit `98aed5d8`.
- Scope: `TWIN-VERIFICATION.json` (450 items), evidence contract in
  [`docs/TWIN-RUNS.md`](TWIN-RUNS.md).
- This session is the phase-1 batch/close-out pass; the three phase-2
  gap-hunting orchestrators branch from the final `main` left here.

## Status counts

| Status | Before | After |
|---|---:|---:|
| `verified` (pre-cutover, non-terminal) | 391 | 158 |
| `reference-verified` | 18 | **252** |
| `twin-verified` | 5 | 5 |
| `fixing` | 16 | 18 |
| `seeded` | 8 | 7 |
| `mismatch` | 6 | 5 |
| `wontfix` | 5 | 4 |
| `reference` | 1 | 1 |
| **terminal total** | 23 | **257** |

The two new `fixing` items are real code fixes (`PAR-0439`, `PAR-0442`) whose
Godot legs are green but whose fresh Java legs are still open — per
[`docs/TWIN-RUNS.md`](TWIN-RUNS.md) they are **not** `twin-verified`.

## Items fixed (code)

| Item | One-line change | Files |
|---|---|---|
| PAR-0439 | Cultivator now declares `Attribute.spores` with the `AttributeCrafter` default `baseEfficiency = 1` (`Blocks.java:2952-2972`) so `setStats` emits the **Affinities** row; vent-condenser declares `Attribute.steam` (`Blocks.java:3000-3006`). Serpulo floors gained their Java spores attributes (tainted-water / deep-tainted-water / darksand-tainted-water / moss / spore-moss — `Blocks.java:245,260,268,594,596`). | `client/rust/mind-core/src/world/block_kind_data.rs`, `client/rust/mind-core/src/content/registries/blocks/environment.rs` |
| PAR-0439 (test) | New `attribute_crafter_affinities_rows` test (cultivator = Affinities, vent-condenser = Tiles) and the stale "spores is not in the port's Attribute" comment fixed. | `client/rust/mind-core/src/ui/content_stats/production.rs` |
| PAR-0442 | `MindHud.sector_name` now stores `SectorPreset.localizedName` (`campaign.sector_name_localized(planet, sector)`) instead of the raw preset id, and the sector-lost toast reuses it; `GameOverDialog`'s `@sector.lost` headline therefore renders the localized name. | `client/rust/mind-gdext/src/ui/hud.rs` |

Live Godot evidence (display `:10`, pids stamped in the run reports):

- PAR-0439: `runs/PAR-0439/godot/20261103-001611-01-content-info-affinities.png`
  — `content_info_json("cultivator")` carries
  `optional -> affinities` (menu shows `<load map to show>` because the stat
  context is not in-game; in-game it lists the spores floors).
- PAR-0442: `runs/PAR-0442/godot/20261103-001721-02-live-gameover-headline.png`
  — after `start_sector("serpulo",170)` the HUD watcher opened the real
  game-over dialog and its headline read
  `Sector [accent]Ground Zero[] lost!` (the raw id `groundZero` is gone).
- `sector_name_localized("serpulo",170)` = `Ground Zero`, `(…,173)` =
  `Sunken Pier` (the remapped cell indices).

## Batch-verified (Godot-only / oracle legs, not twin legs)

The 391 `verified` items were verified before the native-Rust UI cutover
(`c14c807e`). That commit changed only `mind-gdext/src/ui/**`, `client/scenes/ui/**`,
the deleted GDScript UI, `stdb.rs` and docs — no simulation, render, audio,
input, campaign or editor-logic path. Groups were re-run once per shared code
path and marked `reference-verified` with an `evidence` field naming the
representative run. Anything whose observable behaviour is a live UI flow that
was not run in this session was left non-terminal (see deferrals).

| Group | Items | Representative evidence | Shared code path |
|---|---:|---|---|
| Sim / headless / campaign core | 215 | `runs/PAR-0061/20261103-002044-run.md` — `cargo test -p mind-core` (2833 passed, 0 failed) + `bash tools/parity.sh` (goldens PASS, smoke 4/4, exit 0) | `client/rust/mind-core/**`, `client/rust/mind-headless/**` |
| Database + content info | 4 | `runs/PAR-0068/…-run.md`, `runs/PAR-0068/godot/…-01-database-grid.png`, `runs/PAR-0439/godot/…-01-content-info-affinities.png` | `MindDatabaseDialog` / `ContentInfoDialog` + `mind-core::ui::content_stats` |
| Research / tech tree | 7 | `runs/PAR-0047/…-run.md`, `runs/PAR-0047/godot/20261103-001551-01-research-tree.png` | research read model + `MindTreeLayout` |
| Settings (+ graphics list) | 5 | `runs/PAR-0069/…-run.md`, `02-settings-categories.png`, `runs/PAR-0435/godot/20261103-001900-02-settings-graphics-list.png` | `SettingsMenuDialog` + `settings_hub` + `MindStyles` |
| Custom Game grid | 1 | `runs/PAR-0065/…-run.md`, `01-custom-game-grid.png` | `CustomGameDialog` map grid |
| Game-over dialog | 1 | `runs/PAR-0245/…-run.md`, `runs/PAR-0442/godot/…-02-live-gameover-headline.png` | `GameOverDialog` + `game_stats_json` |
| Wontfix re-check | 1 | `runs/PAR-0409/…-run.md` — the accepted F-K1 difference no longer reproduces: the Custom Game grid is a 5-column scrollable grid, wheel-scrolling reaches Glacier…Triad (frame `02-custom-game-grid-wheelscroll.png`) | `CustomGameDialog` / `MindScroll` |
| Verified re-check | 1 | `runs/PAR-0114/…-run.md` — real per-map previews render in every card (`01-custom-game-map-previews.png`) | `MindPreview` / `MindGridImage` |

`PAR-0120` / `PAR-0123` (Multiplayer) were accidentally swept into the sim batch
and have been reverted to `verified`: multiplayer is deferred (see below). The
batch sweep covered 234 unique items (PAR-0427 appears in both the sim sweep and
the database group because they share the content-stats read model); 232 remain
`reference-verified`.

## Deferrals (concrete reasons)

- **Multiplayer (user-deferred):** `PAR-0121`, `PAR-0122`, `PAR-0124`, `PAR-0125`
  (`seeded`), plus `PAR-0120`, `PAR-0123` (`verified`) — no server/client pair is
  in scope for this phase; the user explicitly deferred multiplayer.
- **Campaign loadout (blocked on campaign progression):** `PAR-0024`, `PAR-0025`,
  `PAR-0026`, `PAR-0027` — a fresh campaign has no owned launcher core, so the
  Configure Loadout flow cannot be reached without playing up to a captured
  sector. `PAR-0025`/`PAR-0027` remain `verified` (unbatched), `PAR-0024`/`PAR-0026`
  likewise.
- **`fixing` (16 pre-existing deep draw/config residuals):** `PAR-0126`, `0296`,
  `0363`, `0365`, `0366`, `0367`, `0368`, `0369`, `0379`, `0380`, `0394`, `0399`,
  `0407`, `0408`, `0420`, `0445` — all need part-tree/geometry or dialog-layout
  work that cannot be validated inside this timebox; none were close enough to
  land safely. They keep their recorded progress notes.
- **`mismatch` (5):** `PAR-0426` (Database rows 10–16 px low), `PAR-0433` (logic
  editor title wrap + pause), `PAR-0435` (Settings Graphics list 9 px high —
  re-confirmed live, frame `runs/PAR-0435/godot/20261103-001900-02-settings-graphics-list.png`),
  `PAR-0436` (planet rim on the dark side), `PAR-0443` (planet `uiAlpha` fade).
  Visual deltas that need a Java leg and pixel measurement; no safe code change
  inside the window.
- **`seeded` (3 non-multiplayer):** `PAR-0151` (host-dialog layout), `PAR-0448`
  (MindBar icon glyphs), `PAR-0449` (hover panel clipping) — each is a real UI
  gap needing a larger widget/layout change.
- **`wontfix` (4):** `PAR-0346`, `PAR-0378`, `PAR-0412`, `PAR-0415` — accepted
  differences not re-run this session (`PAR-0415` needs the input-flush retest).
- **`reference` (1):** `PAR-0057` — minimap probe needs a loaded sector; the
  direct `start_sector` launch used here has no core (the loadout flow is gated
  by campaign progression), so the minimap probe read an empty 32×32 texture and
  cannot substantiate the item.
- **`verified` (158):** mostly live UI/render/campaign-world items (HUD panes,
  world render passes, editor shells, map/campaign flows) that the cutover could
  have touched and that were not re-run this session. Left non-terminal
  deliberately rather than batch-claimed.

## Leftovers for phase 2 (highest value first)

1. The two fixed items need fresh Java legs: `PAR-0439`, `PAR-0442` →
   `twin-verified`.
2. The 5 `mismatch` items are small, measured visual deltas — each is a
   candidate one-leg fix (`PAR-0426`, `PAR-0435`, `PAR-0443` are layout numbers;
   `PAR-0436` is a shader sign; `PAR-0433` is a label width + pause flag).
3. `seeded` `PAR-0448`/`PAR-0449` are the last non-multiplayer UI gaps (MindBar
   icon drawing, hover-panel placement).
4. The 16 `fixing` draw residuals (`PAR-0363/0365-0369/0379/…`) are the render
   half of the port; they need twin legs per part family.
5. The 158 `verified` UI/render/campaign items need live representative legs
   per screen (use the group pattern above; `runs/` is gitignored).

## Exact commands for a follow-up agent

```bash
cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
bash tools/build.sh                          # after any Rust edit
bash tools/parity-session.sh --stop && bash tools/parity-session.sh   # extension is mmap'd
python3 tools/twin_tx.py list --nonterminal --tsv
python3 tools/twin_tx.py show PAR-0442
python3 tools/twin_tx.py claim PAR-0442 --agent orch2
python3 tools/twin_tx.py set  PAR-0442 --status twin-verified --similarity N --field evidence="runs/PAR-0442/..."
cargo test --manifest-path client/rust/Cargo.toml -p mind-core
bash tools/parity.sh                          # headless oracle (goldens + smoke)
python3 tools/twin_verification.py check      # after queue edits
# live legs: MCP bridge 6970, godot_game play {"scene":"res://scenes/game.tscn"},
# evidence via tools/twin_item.sh dir|godot-shot|java-shot|note|log
```

Reference frames for this session live under `runs/PAR-####/` (gitignored,
never under `runs/archive/**`).

## Build / test state at hand-off

- `bash tools/build.sh` — ok (mind-gdext + mind-headless, scenarios synced).
- `cargo test -p mind-core` — ok, 2833 lib tests + all integration binaries pass.
- `bash tools/parity.sh` — ok (goldens PASS, smoke 4/4, exit 0).
- `python3 tools/twin_verification.py check` — run before the hand-off commit.

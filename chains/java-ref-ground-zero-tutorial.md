---
id: java-ref-ground-zero-tutorial
title: Java reference soak — Ground Zero basics (launch flows + gz.* tutorial chain)
status: recording
applies_when: >-
  A reference soak (§2b) records the Java side of the Ground Zero basic
  actions, or a reference-backed item must reproduce a stored Java frame from
  its checkpoint.
preconditions:
  - JDK 17 and the built reference jar at `../Mindustry/desktop/build/libs/Mindustry.jar`.
  - Display `:10`; computer-mcp registered against it. The Godot editor may
    share the display, so raise Mindustry before every capture.
  - `parity-java-home/` normalized (1152x648, UI scale 1.0, language en,
    volumes 0); back up `parity-java-home/.local/share/Mindustry/saves/`
    before forcing a fresh profile.
  - Java only: the Godot client stays down for the whole soak.
tools: [computer-mcp_mouse_move, computer-mcp_click, computer-mcp_drag, computer-mcp_key_press, computer-mcp_key_down, computer-mcp_key_up, window_ctl.py, capture_screen.py]
last_verified: R00 recorded 2026-10-09 in runs/20261009-065416-ref-ground-zero (fresh launch -> tutorial start); R01-R16 pending
---

# java-ref-ground-zero-tutorial

The action table (R00-R16) and the recording rules live in the parity-eval
skill §2b.1/§2b.2. This chain carries the exact calls; rows below start as
plans and are replaced by the recorded sequence on the first soak run.

## Steps

0. **Boot (once per soak).** Detached launch (see `java-reference-leg.md` §1-3
   for the full recipe):

   ```bash
   RUN="runs/$(date -u +%Y%m%d-%H%M%S)-ref-ground-zero"
   mkdir -p "$RUN/java"
   setsid env DISPLAY=:10 HOME="$PWD/parity-java-home" \
     XDG_DATA_HOME="$PWD/parity-java-home/.local/share" \
     XDG_CONFIG_HOME="$PWD/parity-java-home/.config" \
     XDG_CACHE_HOME="$PWD/parity-java-home/.cache" \
     /usr/lib/jvm/java-17-openjdk-amd64/bin/java \
     -jar ../Mindustry/desktop/build/libs/Mindustry.jar \
     >"$RUN/java/game.log" 2>&1 </dev/null &
   pgrep -f 'Mindustry[.]jar' > "$RUN/java/java.pid"
   until grep -q "Total time to load" "$RUN/java/game.log"; do sleep 2; done
   DISPLAY=:10 "$MCP_PY" .opencode/skills/parity-eval/scripts/window_ctl.py unmaximize Mindustry
   DISPLAY=:10 "$MCP_PY" .opencode/skills/parity-eval/scripts/window_ctl.py resize Mindustry 1152 648
   DISPLAY=:10 "$MCP_PY" .opencode/skills/parity-eval/scripts/window_ctl.py move Mindustry 45 69
   DISPLAY=:10 "$MCP_PY" .opencode/skills/parity-eval/scripts/window_ctl.py raise Mindustry
   ```

   Capture `java/step-00-boot.png`; the report's `region` gives the window
   origin, so screen click = window capture offset + origin. Re-measure from
   the current capture; known placements: `+45+69` (campaign seed sweep),
   `+70+63` (EV-0051/EV-0056 runs). Measured 2026-10-09 (xrdp Xorg :10):
   `move Mindustry 45 69` puts the WM frame at +45+69 and the client area at
   **+50+98** (xfwm4 decoration +5/+29) — use the capture `region` for clicks.

1. **Rows R00-R16** — each row records: checkpoint, prefix calls (one
   `execute` batch), action calls, settle wait, reference frame, expected
   bundle keys. Until a row is recorded, this table is a plan only:

   | # | prefix | action | settle | frame | keys |
   |---|---|---|---|---|---|
   | R00 | boot -> menu | Play > Campaign > Serpulo card > OK > Launch | retake until objective panel | `java/r00-launch.png` | `gz.mine`, `hint.desktopMove` |
   | R01 | Save & Quit -> menu | Play > Campaign > Ground Zero hex > Go | prompt resumes | `java/r01-resume.png` | objective HUD |
   | R02 | checkpoint | WASD move near the copper ore | hint visible | `java/r02-move.png` | `hint.desktopMove` |
   | R03 | checkpoint | click the copper ore | mining starts | `java/r03-mine.png` | objective 0, `gz.mine` |
   | R04 | checkpoint | tech tree -> research Mechanical Drill -> production menu -> place on copper | drill placed | `java/r04-drill.png` | objective 1, `gz.research` |
   | R05 | checkpoint | research/place conveyors to the core | core +5 | `java/r05-conveyors.png` | objective 2, `gz.conveyors` |
   | R06 | checkpoint | more drills; mine 100 copper | core 100 | `java/r06-drills.png` | objective 3, `gz.drills` |
   | R07 | checkpoint | lead drills; mine 25 lead | core lead 25 | `java/r07-lead.png` | objective 9, `gz.lead` |
   | R08 | checkpoint | pan camera up | defense area visible | `java/r08-moveup.png` | `gz.moveup` |
   | R09 | checkpoint | research/place 2 Duo turrets | 2 duos | `java/r09-duo.png` | objective 6, `gz.turrets` |
   | R10 | checkpoint | conveyors feed the Duos copper | `duoAmmo` set | `java/r10-duoammo.png` | objective 15, `gz.duoammo` |
   | R11 | checkpoint | place 6 copper walls | 6 walls | `java/r11-walls.png` | objective 7, `gz.walls` |
   | R12 | checkpoint | wait zone1 300 s, zone2 180 s, zone3 180 s | wave starts | `java/r12-zone1.png`... | objectives 4/5/8, `gz.zone1..3` |
   | R13 | checkpoint | research/place a Scatter turret | scatter built | `java/r13-scatter.png` | objective 11, `gz.aa` |
   | R14 | checkpoint | conveyors feed the Scatter lead | `scatterAmmo` set | `java/r14-scatterammo.png` | objective 16, `gz.scatterammo` |
   | R15 | checkpoint | defend the assault | enemies destroyed | `java/r15-defend.png` | objectives 12/13, `gz.defend` |
   | R16 | checkpoint | defend into the capture timer | `allowWaves` runs | `java/r16-finish.png` | objective 14, `gz.finish` |

2. **Capture.** Raise first, then
   `capture_screen.py --window-title Mindustry --output <run>/java/<row>.png`;
   the report must show `blank: false` and `width/height` 1152x648.

## R00 — fresh launch to the tutorial start (recorded 2026-10-09)

Run `runs/20261009-065416-ref-ground-zero/` (JVM pid 18498, client window
1152x648 at client **+50+98**; frame +45+69, xfwm4 decorations +5/+29).
Reference frames: `java/r00-00-boot.png` (menu), `java/r00-01-planet.png`
(Ground Zero / Threat: Low / Launch), `java/r00-03-tutorial.png` (recorded
tutorial start). Candidates retaken, not recorded: `r00-01-launch.png` (failed
batch, menu unchanged), `r00-02-tutorial.png` (+8 s retake; r00-03 at +14 s,
identical metrics).

### Profile prep (client down)

Fresh profile needs **both** empty saves and a false `campaignselect` (the flag
alone suppresses the first-run chooser):

```bash
cp -p parity-java-home/.local/share/Mindustry/settings.bin "$RUN/checkpoints/settings-prer00-backup.bin"
python3 - <<'EOF'
p='parity-java-home/.local/share/Mindustry/settings.bin'
b=bytearray(open(p,'rb').read()); k=b.find(b'campaignselect')
assert b[k+14]==0x00 and b[k+15]==0x01   # type bool, value true
b[k+15]=0x00
open(p,'wb').write(b)
EOF
# saves: back up parity-java-home/.local/share/Mindustry/saves/ to
# "$RUN/checkpoints/saves-prer00-backup/", then leave saves/ empty
```

### Boot (one shell call)

```bash
setsid env DISPLAY=:10 HOME="$PWD/parity-java-home" \
  XDG_DATA_HOME="$PWD/parity-java-home/.local/share" \
  XDG_CONFIG_HOME="$PWD/parity-java-home/.config" \
  XDG_CACHE_HOME="$PWD/parity-java-home/.cache" \
  /usr/lib/jvm/java-17-openjdk-amd64/bin/java \
  -jar ../Mindustry/desktop/build/libs/Mindustry.jar \
  >>"$RUN/java/game.log" 2>&1 </dev/null &
# gate (fresh log file, or the old "Total time to load" matches instantly):
until grep -q "Total time to load" "$RUN/java/game.log"; do sleep 2; done
window_ctl.py unmaximize Mindustry; window_ctl.py resize Mindustry 1152 648
window_ctl.py move Mindustry 45 69; window_ctl.py raise Mindustry
# measured: xwininfo -name Mindustry -> client +50+98 1152x648
```

### Action (one `execute` call; screen = image + (50,98); busy-wait settles)

| image | screen | call | settle |
|---|---|---|---|
| (230,149) | (280,247) | `mouse_move` + `click` Play | 900 ms; **first send is swallowed by activation** — verify with a capture and re-send |
| (477,151) | (527,249) | Campaign | 2000 ms (chooser fade) |
| (414,240) | (464,338) | Serpulo card | 700 ms |
| (577,607) | (627,705) | OK | 3500 ms (planet page + zoom) |
| (577,612) | (627,710) | Launch | 6000 ms (loading); retake at +8 s and +14 s |

Observed state at R00 (non-blank, 1152x648): objective panel `Obtain: 0/15
Copper`; hint `Use [WASD] to move.`; prompt `Move near the copper ore on the
ground and click to begin mining.`; `Skip`; minimap top-right; block-info panel
right (hover Copper); bottom-right icon bar (Schematics/Database/Tech
Tree/Planet map — ledger items PAR-0032..PAR-0036).

Checkpoint (autosave, mtime moved after the launch): the Java tutorial sector
save is `sector-serpulo-170.msav` (Ground Zero = sector 170 here); copied to
`checkpoints/r00-sector-serpulo-170.msav`.

Hiccup (see `computer-mcp-learnings.md`, 2026-10-09): the first click after
this relaunch was consumed by window activation — a 5-click batch left the menu
unchanged (`java/r00-01-launch.png`); a capture + re-send opened the submenu
(`java/probe-click-play.png`).

## R01 — resume from Save & Quit (recorded 2026-10-09)

Same client (no reboot). Action: Escape → pause → Save & Quit → confirm → menu
→ Play → Campaign → Ground Zero hex → Go. Reference frames:
`java/r01-00-pause.png` (pause dialog), `java/r01-01-quitconfirm.png` (Confirm
"Are you sure you want to quit?"), `java/r01-02-menu.png` (menu after
`@saving`), `java/r01-03-planet.png` (planet page: hex alert, "Sectors — 1
under attack"), `java/r01-04-sectorpanel.png` (Ground Zero / Under attack! /
Resources / Stats / Go), `java/r01-05-resume.png` (recorded; +5 s retake
`r01-06` identical).

### Action (screen = image + (50,98); busy-wait settles)

| image | screen | call | settle |
|---|---|---|---|
| — | — | `key_down`+`key_up` Escape | 1000 ms |
| (576,410) | (626,508) | Save & Quit | 900 ms (quit confirm) |
| (676,358) | (726,456) | confirm OK | 4000 ms (`@saving` → menu) |
| (230,149) | (280,247) | Play | 900 ms |
| (477,151) | (527,249) | Campaign | 2800 ms (no chooser) |
| (577,322) | (627,420) | Ground Zero hex | 1200 ms (sector panel) |
| (576,616) | (626,714) | Go | 13000 ms (loading + world); +5 s retake |

The tutorial's zone timer fired around the 300 s mark while the client idled in
R00, so the planet page flagged the sector "1 under attack" (hex warning
triangle) and the action button read `Go`; Go resumed the stored save anyway
and the resumed frame is pixel-identical to the R00 tutorial start
(`frame_diff` r00-03 vs r01-05: changed_fraction 0.00011, mean_abs 0.01). The
autosave was rewritten by Save & Quit (`sector-serpulo-170.msav` mtime 17:07:02
vs R00's 17:02:30) → copied to `checkpoints/r01-sector-serpulo-170.msav`.

## R02 — WASD movement (recorded 2026-10-09)

Action: hold `S` 900 ms, then hold `W` 900 ms (both as `key_down` + busy-wait +
`key_up` in one `execute` call). Reference frames: `java/r02-00-before.png`
(fork: hint `Use [WASD] to move.`, objective 0), `java/r02-04-after-w.png`
(recorded: back at the copper field, hint `Scroll to zoom in or out.`),
supporting `r02-01-move.png` (south view after S), `r02-02` (retake), `r02-03`
(camera settled, +60 s).

| after | wait | frame |
|---|---|---|
| hold S (key_down → busy-wait 900 ms → key_up) | 1200 ms | r02-01 |
| hold W (same) | 1200 ms | r02-04 (recorded) |

Observed: the mech moves and the camera follows; the first movement advances
the rotating hint from `hint.desktopMove` ("Use [WASD] to move.",
bundle.properties:2340) to `hint.zoom` ("Scroll to zoom in or out.", :2341)
while the prompt `Move near the copper ore on the ground and click to begin
mining.` and objective 0 stay. 900 ms of S showed the snow plateau edge south
of the field; 900 ms of W returned to the field (r02-04 vs r02-00: changed
0.48). Camera stable at the new pose (r02-01 vs r02-03 at +60 s: changed
0.043). Autosave `sector-serpulo-170.msav` mtime 17:11:01 (changed from
17:07:02) → `checkpoints/r02-sector-serpulo-170.msav`.

## R03 — click copper ore (recorded 2026-10-09)

Action: click the tutorial-highlighted copper tile. Fork
`java/r02-04-after-w.png` (objective `Obtain: 0/15 Copper`); action
`java/r03-02-mine.png` (+9.5 s); support `java/r03-01-mine.png` (+3.5 s).

| after | wait | frame |
|---|---|---|
| click ore (image 832,412 → screen 882,510) | 3500 ms | r03-01 (counter 39; panel already `Build: 1x Mechanical Drill`) |
| mining | 6000 ms | r03-02 (counter 74, recorded) |

Observed: the mech mines the highlighted tile; the core-copper HUD appears
top-center and climbs fast (39 → 74 in 6 s); objective 0 (`Item 15`, `gz.mine`)
completes as soon as the mined copper crosses 15, so the panel switches to
objective 1 (`Build: 1x Mechanical Drill`) before the first capture — the
pre-click `Obtain: 0/15` state is the fork frame. Autosave mtime 17:13:02
(changed from 17:11:01) → `checkpoints/r03-sector-serpulo-170.msav`.

## R04 — research + place the Mechanical Drill (recorded 2026-10-09)

Action: tech tree → research Mechanical Drill → block picker → place on the
gold 2×2 target. Fork `java/r04-03-world.png` (objective 1); action
`java/r04-05-placed.png`; support `r04-00` (tree open), `r04-01` (Core: Shard
researched), `r04-02` (after the drill research, copper 660), `r04-04`
(placement preview).

| image | screen | call | settle |
|---|---|---|---|
| (974,627) | (1024,725) | tech-tree icon in the bottom bar | 1800 ms |
| (578,395) | (628,493) | core node | 1200 ms (info: Core: Shard / Researched) |
| (440,312) | (490,410) | Mechanical Drill node | 1200 ms — spends 10 copper immediately (desktop click on a selectable+affordable locked node spends, ResearchDialog.java:483) |
| — | — | Escape closes the tree | 1200 ms |
| (883,427) | (933,525) | drill block button in the picker | 900 ms (block info: Key [2,1], Copper 12) |
| (782,447) | (832,545) | hover the gold 2×2 target | 700 ms (placement preview, r04-04) |
| (782,447) | (832,545) | click to place | 1500 ms (r04-05) |

Observed: research costs `researchCost` copper 10 and building 12
(`Blocks.java`); the block picker's grid + category column become visible in
the bottom-right once the drill is unlocked; the drill is 2×2
(`Category.production`) and the gold highlight marks the placement area. On
placement the objective advances from `Build: 1x Mechanical Drill` to
`Move into Core: 0/5 Copper` with the conveyor prompt. Autosave copied to
`checkpoints/r04-sector-serpulo-170.msav`.

## R05 — research + place conveyors; core receives copper (recorded 2026-10-09)

Action: research the Conveyor in the tree → select it in the block picker →
place a 7-tile line from the drill to the core → deliveries. Fork
`java/r05-18-catnext.png` (conveyor selected, objective 2); action
`java/r05-22-delivered2.png` (objective 3 at 24/100); support
`r05-14-researched.png` (tree), `r05-20-line.png` (line + item on belt),
`r05-21-delivered.png`.

| image | screen | call | settle |
|---|---|---|---|
| (974,627) | (1024,725) | tech-tree icon | 2000 ms |
| pan | drag (800,300)→(420,300), then (150,300)→(1100,300) | tree background | 900 ms each — drag RIGHT reveals left content |
| (61,311) | (111,409) | Conveyor node (hover: `Conveyor / Copper 5/5`; click spends 5 copper) | 1200 ms |
| — | — | Escape closes the tree | 1500 ms |
| `.` | — | `Binding.categoryNext`: distribution category + auto-select conveyor (the clipped category button does nothing) | 1200 ms |
| — | scroll(0,-2) | rotate 90° (one-shot pynput); verify the placement arrow in a capture | 1000 ms |
| row | clicks (785,433)(753,433)(721,433)(689,433) + corner (817,433) | place west-facing conveyors at image y=335, x 735/703/671/639 + 767 | 450 ms each |
| column | scroll(0,-2) → N; clicks (817,497)(817,465) | north-facing tiles at image (767,399)/(767,367) | 500 ms each |

Observed: the drill dumps to ANY adjacent transporter (`BuildingComp.dump`
proximity loop), so the line can leave from the drill's north side; copper
nuggets ride the belt west into the core (core 2.9k → 4.0k); objective 2
completes within ~20 s of the first delivery and the panel advances to
`Move into Core: 24/100 Copper` (objective 3) with the "Expand the mining
operation…" prompt and `Right-click and drag to break blocks.` hint. Grid used:
core 3×3 at x 527-623 → 32 px tiles; row centered on the core's middle row
(y≈335). Autosave mtime 17:34:03 → `checkpoints/r05-sector-serpulo-170.msav`.
Hiccups (see `computer-mcp-learnings.md`, 2026-10-09): category-button click
inert (use `.`); `computer-mcp.drag` places only one tile (click per tile).

## R06 — more drills; 100 copper (recorded 2026-10-09)

Action: place a second Mechanical Drill on the copper field with a short column
into the existing row; wait out the counter. Fork `java/r06-00-now.png`
(24/100); action `java/r06-04-100.png` (objective advanced to `0/25 Lead`);
support `r06-01-production.png`, `r06-02-preview.png`, `r06-03-seconddrill.png`.

| image | screen | call | settle |
|---|---|---|---|
| — | — | `,` (categoryPrev → production; drill auto-selected) | 1000 ms |
| (719,511) | (769,609) | hover the 2nd drill spot — preview valid (`Drill Speed: 0.36/s`) | 900 ms |
| (719,511) | (769,609) | click to place (footprint x 687-751, y 479-543) | 1000 ms |
| — | — | `.` (distribution → conveyor) | 900 ms |
| (703,463)(703,431)(703,399)(703,367) | (753,561)(753,529)(753,497)(753,465) | north-facing column into the row tile (703,335) | 450 ms each |
| — | — | wait for the counter | 40 s |

Notes: an earlier click at image (687,479) placed nothing — the boulder at image
650-694,474-520 blocks that 2×2; hover + capture the placement preview before
every click (valid = solid ghost + `Drill Speed` tooltip). The counter (75/100
at 17:36) crossed 100 ~40 s after the second drill started delivering, and the
panel advanced to `Move into Core: 0/25 Lead` (objective 9). Autosave copied to
`checkpoints/r06-sector-serpulo-170.msav`.

## R07 — lead reconnaissance notes (in progress; soak stopped 2026-10-09)

The lead objective (`Move into Core: 0/25 Lead`) is active after R06. Lead
deposit located from the map file (`tools/parse_msav.py` in the run dir parses
`groundZero.msav`): `ore-lead` tiles **x 108-113, y 40-50**; copper patch
x 124-140, y 44-53; core ~tile (129,51). Calibration: image (832,412) = tile
(137,53) at 32 px/tile, screen = image + (50,98), so the lead patch sits ~16
tiles **west** of the core.

Intended route: walk **west only** ~10-12 tiles (watch the minimap), place a
drill on the lead (~tiles 110-113,44-47), run a conveyor row east into the
core's west side, wait for 25 lead.

What went wrong: the first exploration hold was W (2.5 s), which took the mech
to the map's north edge; the correction walk then pushed it south off the
playable island into the black void (minimap marker below the landmass blob)
and it stopped taking movement input. It then ignored keys entirely because the
client had auto-paused (`< Paused >` banner) when a terminal window took focus.

Session end (operator stop): client killed (pid 18498); the live sector save was
rolled back to `checkpoints/r06-sector-serpulo-170.msav` so the next soak
resumes at the lead objective with the mech at the core (see `run.json`
`session_end`). Resume: chain step 0 → normalize → Play > Campaign > Ground
Zero hex > Go (R01 flow) → this row.

## Wait marks

| after | wait | why |
|---|---|---|
| boot launch | `until grep -q "Total time to load"` | first launch is slow; no fixed sleep |
| first click after (re)launch/raise | re-send once after a capture shows no reaction | window activation swallows the first click (2026-10-09) |
| any menu click | 300-500 ms | dialog/menu fade |
| Play > Campaign (fresh) | capture-check, then input | first-run chooser appears after a fade |
| Launch | retake until the objective panel is visible (budget ~30 s) | loading bar |
| objective completes | ~500-1000 ms | prompt/objective transition |
| R12 zones | 300 / 180 / 180 s, chunked <=60 s | map timers; bank write-up time between chunks |

## Success signals

- Every row has a non-blank 1152x648 reference frame plus the exact call batch
  and settle wait that produced it.
- Each row's expected bundle keys/objective text match what the reference frame
  shows (where the session can read frames; otherwise record the metric basis).
- One continuous client session per segment; the checkpoint after each row is
  usable for the next prefix (or the fallback is recorded).

## Failure modes

- First click after launch only focuses the window (observed for Editor/Play):
  verify with a capture and send the click again.
- A move+click can land at the previous pointer position unless the pointer
  read-back settles; prefer one click per named step and verify via capture.
- Held left button persists across MCP calls (mining continues): send an
  explicit `computer-mcp_button_up` before a fresh click.
- Text fields drop injected input (save slot names) — prefer the save slot that
  already exists, or copy `parity-java-home/.local/share/Mindustry/saves/*.msav`
  instead.
- `pkill -f` is not allowed; kill the recorded `java.pid` only.
- Do not press the tutorial `Skip` control in a soak: it leaves the reference
  chain, so the row's expected state no longer matches.

## Variants

- Resumed launch (R01) can also be exercised by restarting the client while a
  sector save exists; record `profile: resumed` on the item.
- Long-timer rows may be paused after the preceding row, with the wait resumed
  in the next iteration; the chunked sleeps and captures are identical.

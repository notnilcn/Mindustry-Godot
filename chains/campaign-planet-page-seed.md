---
id: campaign-planet-page-seed
title: Boot → Play → Campaign → first-run chooser → planet page (both legs, 1152x648, single display :10)
status: verified
applies_when: >-
  A campaign-surface sweep or item needs both clients at the campaign planet
  page (chooser included when the data dir has no selected planet).
preconditions:
  - Xvfb :10 at 1280x720; Godot editor open on :10 with the bridge on 6970; Java jar built.
  - Java data dir parity-java-home is persistent; on a fresh/never-campaigned dir
    the chooser appears after Play > Campaign (both clients show it).
  - Java window normalized to 1152x648 at +45+69 (post-resize MOVE is mandatory:
    the old 900x700-centered origin pushes the right edge off-screen).
  - Raise the Java window immediately BEFORE every Java capture and click: the
    `godot_game play` action raises the editor/game over Java and an obscured X11
    window reads back black (calibration below).
tools: [computer-mcp_mouse_move, computer-mcp_click, window_ctl.py, capture_screen.py, frame_diff.py, godot_health, godot_editor_read, godot_editor_edit, godot_game, godot_exec, godot_input, godot_screenshot]
last_verified: 2026-10-09 runs/20261009-052658-seed-campaign (iter 1: both legs reached DialogLayer/planet, PAR-0015 mismatch; iter 2: Godot saves cleared + chooser re-replayed, aligned pair diff-10-planet-aligned.json, Launch classification PAR-0019/0024)
---

# campaign-planet-page-seed

Exact calls from `runs/20261009-052658-seed-campaign/` (single-display :10,
Godot pid 17804, JVM pid 17325). Screen coords = Java window image + (45,69).

## Java leg

```bash
RUN="runs/$(date -u +%Y%m%d-%H%M%S)-seed-campaign"; mkdir -p "$RUN/java" "$RUN/godot" "$RUN/seed" "$RUN/diff"
JHOME="$PWD/parity-java-home"
setsid env DISPLAY=:10 HOME="$JHOME" XDG_DATA_HOME="$JHOME/.local/share" \
  XDG_CONFIG_HOME="$JHOME/.config" XDG_CACHE_HOME="$JHOME/.cache" \
  /usr/lib/jvm/java-17-openjdk-amd64/bin/java -jar ../Mindustry/desktop/build/libs/Mindustry.jar \
  >"$RUN/java/game.log" 2>&1 </dev/null &
until grep -q "Total time to load" "$RUN/java/game.log"; do sleep 2; done
MCP_PY="$HOME/.local/share/uv/tools/computer-mcp/bin/python"
DISPLAY=:10 $MCP_PY .opencode/skills/parity-eval/scripts/window_ctl.py unmaximize Mindustry
DISPLAY=:10 $MCP_PY .opencode/skills/parity-eval/scripts/window_ctl.py resize Mindustry 1152 648
DISPLAY=:10 $MCP_PY .opencode/skills/parity-eval/scripts/window_ctl.py move Mindustry 45 69   # required; resize keeps the old center
DISPLAY=:10 $MCP_PY .opencode/skills/parity-eval/scripts/window_ctl.py raise Mindustry
```

Clicks (persistent computer-mcp; `click` acts at the current pointer):

| step | screen | image | result |
|---|---|---|---|
| Play | 275,218 | 230,149 | Play submenu (Campaign/Join Game/Custom Game/Load Game) |
| Campaign | 522,220 | 477,151 | first-run "Select Starting Campaign" chooser (Serpulo pre-selected) |
| Serpulo card | 459,309 | 414,240 | Serpulo description shows |
| OK | 622,676 | 577,607 | planet page loads (starfield) |

Capture (raise first, every time):

```bash
DISPLAY=:10 $MCP_PY .opencode/skills/parity-eval/scripts/window_ctl.py raise Mindustry
DISPLAY=:10 $MCP_PY .opencode/skills/parity-eval/scripts/capture_screen.py \
  --window-title Mindustry --output "$RUN/seed/step-04-planet-java.png"
# -> region [45,69,1152,648], mean 13.94, non-blank (starfield background is dark)
```

## Godot leg

```
godot_game  {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
godot_game  {"action":"status"}                       # runtime_connected:true, pid 17804
godot_exec  {"action":"eval","params":{"code":"return {\"pid\": OS.get_process_id(), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick()}"}}
```

Clicks (always press + release + `Input.flush_buffered_events()` in a follow-up eval):

| step | viewport | resolved from |
|---|---|---|
| Play | 230,149 | `/root/Spine/Ui/UiRoot/MenuGroup/menu/Sidebar/Buttons` child 0 center |
| Campaign | 460,149 | `MenuGroup/menu/Submenu` child 0 center (Submenu becomes visible after Play) |
| Serpulo card | 412,194 | `DialogLayer/planet` card button |
| OK | 499,634 | `DialogLayer/planet` footer button |

```
godot_screenshot {"action":"game"}   # copy the returned mcp_screenshots path into $RUN/seed/step-05-planet-godot.png
```

## Compare + calibration

```bash
$MCP_PY .opencode/skills/parity-eval/scripts/frame_diff.py \
  --java "$RUN/seed/step-04-planet-java.png" --godot "$RUN/seed/step-05-planet-godot.png" \
  --out-json "$RUN/diff/diff-05-planet.json"
# changed 0.545, mean_abs [18.6,14.9,27.6], coarse 19.8, palette 0.31 -> MISMATCH (PAR-0015)
```

Menu/planet backgrounds are independently randomized per client launch on BOTH
sides (Java cross-launch 0.83/32.7/39.4/0.74 vs Java same-session temporal
0.0175/0.65/0.68/0.016; see PAR-0001.calibration and
`$RUN/diff/calib-*.json`). Do not read full-frame menu metrics as parity —
compare element positions/labels or masked regions.

## Divergences this sequence exposed

- PAR-0010 chooser render: Godot opens with no selection, different card
  geometry, no Tech Tree footer entry (diff 0.407/23.6/17.4/0.20).
- PAR-0014 chooser footer: Java Back/OK/Tech Tree vs Godot OK/Back.
- PAR-0015 planet page pair mismatch (state-confounded; Java fresh vs Godot
  under-attack — normalize state before pixel work).
- PAR-0016 sector panel shows raw `[scarlet]`/`[red]`/`[accent]`/`[lightgray]`
  markup in Godot (`sector.lockdown` bundle string; upstream colorizes via
  `UI.formatIcons`, PlanetDialog.java:1092).
- PAR-0017 Godot rail has an extra "Select Launch Destination" entry; upstream
  uses `sectors.launchselect` as the sector action label in planetLaunch mode
  (PlanetDialog.java:726), not a rail button.

## State alignment (campaign saves)

When one client has campaign progress (sector saves) and the other does not,
align before comparing pixels:

```bash
cd "$HOME/.local/share/godot/app_userdata/Mindustry-Godot"
cp -a saves saves.bak-<stamp> && cp -a config config.bak-<stamp>
```

```
godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/MindCampaign\").clear_campaign_saves()"}}   # true
godot_game {"action":"stop"}
godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
```

After the reset the chooser re-appears (Serpulo 412,194 → OK 499,634) and the
fresh sector panel matches Java's fields (Ground Zero / Threat: Low / Launch).
The backup preserves the previous state for launch-from-core flows.

## Sector panel + action classification (fresh state)

Aligned pair `diff/diff-10-planet-aligned.json`: changed 0.541, mean_abs
[18.4,14.5,27.4], coarse 20.6, palette 0.29. Element crops (`calib-*-aligned.json`):
panel 0.97/69/46/0.89, rail 0.78/45.6/41.6/0.69 — the panel crop is NOT
background noise: Java black panel + bitmap font + gold pill Launch vs Godot
gray panel + smooth font + flat rect; rail width 327 vs 200 plus the extra
'Select Launch Destination'.

Action classification (Launch):
- Java screen (621,676): free launch — loading bar, then the tutorial world
  (Obtain 0/15 Copper). Upstream `PlanetDialog.java:1397-1403` free-launches
  when `findLauncher(sector) == null`.
- Godot viewport (576,613): `DialogLayer/launch_loadout` "Configure Loadout"
  over the menu background; row 'basicShard' overflows (rect 1188px > 1152) and
  the '1800' RichTextLabel sits at x=1463 off-screen.

Normalize pointer hover before diffing panel crops: Java's Launch showed its
gold hover border only because the pointer was left on it.

## Failure modes

- Black Java captures: the Godot play window raised over Java; re-raise before
  every capture/click (`computer-mcp-learnings.md`, 2026-10-09 entry).
- `capture_screen.py` under a chained `VAR=... && DISPLAY=:10 ...` call can lose
  DISPLAY (Bad display name ""); use `env DISPLAY=:10 <python> ...` instead.
- `godot_exec` on `MindCampaign.campaign_views_json("serpulo")` times out (30 s);
  read `get_sector_state()` and dialog labels instead
  (`open-godot-mcp-learnings.md`, 2026-10-09 entry).
- The Godot chooser only shows when no planet is selected in its data dir; a
  returning state goes straight to the planet page.

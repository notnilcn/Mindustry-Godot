<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Plan verification, session 2, round 2 (2026-10-14): H03, H07

Worktree `Mindustry-Godot-session2` @ `ffbe84dd` (main; H00 not landed), bridge 6980, display `:11`.
Evidence: `runs/20261014-s2-verify2/` (PNG, JSON). Godot profile: `--import` boot
(`{"nodes": 375, "unlocked": 282}`), Ground Zero via the planet "Go" recipe.

Temporary, uncommitted worktree edits (all reverted, rebuilt; `git status` = only the ledger symlink):
H00 T1 Change 1 (`env_bit` Java bits) for the player alpha; `gdext/fx.rs` `PrimKind::Light` arm
disabled (to attribute the FX discs); the gitignored `WT/assets/shaders/godot/light.gdshader` replaced
by `client/shaders/light.gdshader` (restored from backup). The Java client's isolated
`parity-java-home` got `backgroundpause=false` for one run; `settings.bin` restored from backup.

Status legend: CONFIRMED (symptom reproduced), PARTLY, NOT REPRODUCED, NOT CHECKABLE.

## H03 — rendering + HUD

| Task | Status | Evidence | Plan edits |
|---|---|---|---|
| T1 | CONFIRMED | `H03-T1-diff.txt` (only `.prims[14].kind.y2` differs) | — |
| T2 | PARTLY (a,d,h pass; c,g,j fail; e,f partly; b,i not decisive) | `H03-T2*-*.png` | row (d) numeric block id + `rotation`; row results; (c) cause; (e)/(f)/(g) render-path causes |
| T3 | CONFIRMED | `H03-T3-command-mode.png` | Java not observed (keyboard) |
| T4 | CONFIRMED + root cause | `H03-T4-gz-baseline-black-minimap.png`, `java-05-gz-in.png` | black crop = `rebuild_all` row order; step-4 recipe marked superseded |
| T5 | CONFIRMED | `H03-T5-hover-duo.png` | — |
| T6 | CONFIRMED, diagnosis settled | `H03-T6-ambient-*-before.png` vs `-fixed.png` | stale packed shader is the whole cause; `settings_set` (no `settings_set_bool`) |
| T7 | CONFIRMED (code) | `fires.rs:172` | — |
| T8 | CONFIRMED | `H03-T8-T9-a/b.png` (pixel-identical) | coords -> free ground (134,69) |
| T9 | CONFIRMED | `H03-T8-T9-a.png` | coords -> (142,70); `configure int "0"` works |
| T10 | CONFIRMED | `H03-T2e-core-cracks-T10-burning.png` | — |
| T11 | CONFIRMED (by design) | `H03-T11-force-projector.png` | projector reaches efficiency 1.0 |
| T12 | CONFIRMED, worse | `H03-T12-onset-fog.png` (opaque black), `-nofog.png` | symptom sentence; stopgap question |
| T13 | CONFIRMED absent seams | grep | — |
| T14 | CONFIRMED | 6 errors in session-2 `editor-console.log` | — |
| T15 (new) | unit shield bubble on every unit | `H03-T15-T16-circles-hide-strip.png` | new task |
| T16 (new) | FX light prims drawn as discs | `H03-T16-no-fx-light-discs.png` | new task |
| T17 (new) | raw markup in toasts/announces; internal sector name | `H03-T2c-menus-hidden.png`, `H03-T4-core-under-fire.png` | new task |

Key findings:
* **T6:** the light composite renders, but additively: the packed `blend_add` copy wins. Replacing it
  (no rebuild) gives the dark screen + core glow and the red-ambient screen. Step 1 alone fixes PAR-0246.
* **T4:** the minimap is black on every sector entry. `rebuild_all` writes rows bottom-first while
  `update`/`region_for_camera` are top-first (texture pixel (129,55) = core colour; region y = 128).
* **T2(g):** door/sorter configure works; the open/item sprite is never drawn because
  `drawDynamic=false` blocks skip `blocks.rs` (dead branches) and the static cache never recaches.
  Same root as missing wall cracks (e). Not H02.
* **T2(c):** HUD toggle never hides anything (setting without a row + wrong node paths).
* **T12:** Onset is visually unplayable today (the fog placeholder is opaque black).

## H07 — audio + weather

| Task | Status | Evidence | Plan edits |
|---|---|---|---|
| T1 | PARTLY (code confirmed; concurrency not reached in-engine) | `H07-T1-ten-duos.png` | recipe needs targets in range; disarm note |
| T2 | CONFIRMED (code + factory run) | grep; factory dagger | recipe coords (142,70), `int "0"` |
| T3 | CONFIRMED in-engine | events 149 -> 149 while a dagger walked 45 px | recipe: team-1 dagger + `unit_command` |
| T4 | CONFIRMED (code) | `bullet/mod.rs:659,718` | — |
| T5 | CONFIRMED | Frozen Forest `weather_json` empty; dominant floor snow | — |
| T6 | CONFIRMED | `H07-T6-rain.png`, `H07-T6-T7-snow-fog-sandstorm.png` | — |
| T7 | CONFIRMED | same | — |
| T8 | CONFIRMED (code) | `frame.rs:218-219` | — |
| T9 | CONFIRMED (code) | `production.rs:1477` | — |
| T10 | CONFIRMED, cause narrowed | menu "menu"; planet -> ""; editor -> "menu" | step 3 needed (planet case); `get_state()` is "playing" at the menu |
| T11 | CONFIRMED (code) | grep | — |
| T12 | PASS (no commit) | loopBuild 1; payload events +1/+1; rain/windHowl loops 1 | numeric block id; tile (134,58) |
| T13 | CONFIRMED | 0.0 px drift over 1260 ticks in a sandstorm | — |

## Java leg

Java ran on `:11` (pid 349866, then 355291 after a settings fix; both killed by pid after checking
`/proc/<pid>/environ`). Route Play -> Campaign -> Sectors -> Ground Zero -> Go worked by mouse. The first
run stayed `< Paused >` (background pause on Xvfb, `Control.java:619-632,725-727`). `backgroundpause=false`
fixed that. But keyboard input (Space, WASD, Shift) never reached the client, so these were NOT observed:
the command-mode panel, duo range on hover, and wheel/keys.
Frames that were captured: the Java Ground Zero minimap shows terrain (T4), and the seeded profile has no weather (T5/T6 note).
`java-0*.png`, `java-1*.png`.

## Hiccups (logged in `open-godot-mcp-learnings.md` / `java-leg-learnings.md`)

* An eval with a GDScript lambda wedged the frame loop (`--play` recovered).
* A transient `No Godot instance connected` let a following `step(300)` run unguarded.
* A team-2 test dagger destroyed the Ground Zero core: Sector Lost + autosave, so a clean re-import was needed.
* `$C.set_process(false)` survives a sector switch: the next sector looks black.
* Java: background pause, and keyboard input not delivered.
* The Frozen Forest and Onset loads mutated session 2's profile; it was not re-imported at the end
  (the next session-2 run should do `rm -rf $U` + `--import`).

## Coordinator decisions needed

1. **New H03 tasks T15/T16/T17** (shield bubble on every unit, FX light discs, raw markup + internal sector
   name in toasts). T17 overlaps H05 (sector names). Accept the owners?
2. **H03 T2 rows (e)/(g) need a real task.** Cracks on walls and the door/sorter/switch/unloader state
   sprites for `drawDynamic=false` blocks both need recache/state support in `building_cache.rs`.
   That is more than T2's 30-line limit. Make it a new H03 task, or extend T8 (block pass)?
3. **H03 T12 stopgap:** until H09 lands, hide the opaque fog placeholder so Erekir is visible?
4. **H03 T2(j)/T5:** there is no range circle while placing a duo. Cause not isolated (`plans.rs:270`).
   Fold it into T5?
5. **Out of area:** the Erekir (Onset) palette lists Serpulo blocks (H05/H08?). A factory-spawned team-1
   dagger vanished within 60 ticks of a move order (not investigated; H01?).
6. **H00 has not landed on `main`** (`ffbe84dd`). Both plans' start-state check (`grep -m1 'H00'`) fails
   today. Every player-unit check here used the local env-bit patch.

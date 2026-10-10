# Mindustry-Godot — Port Progress

> Snapshot: **2026-10-10** · Reference: Mindustry `2cd7aeec` (see `parity/upstream.lock`) · Source of truth: [`parity-ledger.json`](./parity-ledger.json) (schema `mindustry-godot/parity-ledger@3`).

This document summarizes where the 1:1 port stands. It combines the existing
live-verified ledger evidence with the **2026-10-10 code-first gap scan**
(72 findings, PAR-0070..PAR-0141) that deep-read both codebases across 13
system areas. New items carry `file:line` evidence on both sides and a fix
plan (`fix.plan`); they are `seeded` and still owe an in-engine walk through
the parity-eval pipeline.

## Headline numbers

| Metric | Value |
|---|---|
| Ledger items | **141** (PAR-0001..PAR-0141) |
| Seeded (open for verification or fix) | 108 |
| Verified (seed / chain / reference / twin) | 16 |
| Mismatch (fresh both-leg evidence, fix open) | 10 |
| Blocked (on a prerequisite item) | 6 |
| Reference (Java soak recorded, Godot walk open) | 1 |
| Code-scan findings added 2026-10-10 | 72 |

Verified anchors: the engine/sim spine (golden checksum `a1a7b96167c9718d`,
deterministic pause/step, API + mouse place/break), boot/menu → campaign entry,
the planet page and sector rules, the Ground Zero tutorial through the
conveyor-line objective, HUD wave pane, input control paths, and the
Database/Settings/Custom Game/Editor map-list dialogs. The heaviest verified
chain is **PAR-0037** (saved Ground Zero resume, twin-verified; 21 documented
fix rounds tracked in its calibration block).

## How to read the queue

- `parity-ledger.json` is the coverage + proof record; every item embeds its
  `path` (fast-forward to the fork), `reproduce` (the action under test), and
  `steps` (mirrored evidence); `fix.plan` is the implementation sketch.
- Mismatch items (PAR-0010, 0014, 0015, 0016, 0017, 0019, 0024, 0028, 0031,
  0066) have fresh both-leg frames and are ready for fix work.
- Code-scan items have empty `path`/`reproduce` by design; the first writer run
  refines them into re-runnable sequences.

## System snapshot (as of this scan)

| System | State | Key evidence |
|---|---|---|
| Foundation / sim spine | **Verified** | PAR-0061/0062/0063; deterministic FNV checksum registry; scenarios + goldens |
| Content registries | Counts match upstream (items, liquids, planets, statuses, weathers; 447 blocks, 65 units); sector presets 46 vs JVM 95 | `parity/reports/jvm_golden_diff.md`; PAR-0083 |
| Blocks & production | All blocks registered, but live behavior gaps: fuel generators reject fuel (PAR-0075), reconstructors cannot be supplied (0076), sandbox sources/voids dead (0077), pumps ignore floor liquid (0078), Erekir beam drills inert (0079) | 56 open block field diffs in the JVM golden diff |
| Logistics | Headless parity-complete; belt delivery verified in the tutorial (PAR-0049 twin) | Belt items invisible on screen: PAR-0130 |
| Power / liquids / heat | Headless goldens landed (plan 09); generator fuel + pump floor-liquid gaps above | PAR-0075/0078 |
| Combat / turrets | Turrets are **inert in live play** (no turret behavior registered / no update pass; PAR-0086); coolant rejected (0087); spawnUnit missiles missing (0088); sublimate window (0089); fires/puddles have no save path (0090) and no live update (0091); bullets pass through units (0141) | JVM bullets 202 vs port 112 |
| Units / AI | Controllers/pathfinder/waves landed; floor speed modifiers ignored (0137); payload carry/drop no-op (0138); enemy RTS + BaseBuilder AI never runs (0139); RTS orders do nothing (0140); flat unit rendering (0126) | PAR-0044 signals (spawn/camera/hint) verified |
| Campaign | Entry, planet page, sector rules, tutorial objectives verified (PAR-0009/0012/0037/0046/0049/0059); gaps: free early-tech unlocks (0080), progression not persisted across restart (0081), no offline sector production/exports (0082), attack/submission sectors absent (0083), loss damage skipped (0084), loadout selection ignored (0085), generated sectors without cores/bases (0112) | PAR-0010/0015/0017/0024 are open mismatches |
| Logic / mlog | Interpreter landed (53 statements / 45 ops / 80 accessors) but **no in-game editor** (0103); world-processor side effects inert (0104); displays blank (0105); frozen globals (0106); `lookup` null (0107); switch config dead (0108) | |
| UI dialogs | 37 registered, boot/menu/database/settings/custom-game/editor flows verified (PAR-0064..0069); gaps: custom rules table empty (0132), Map Play dead (0133), Import Save dead (0134), language not persisted (0135), join server list (0136), bans/admins dead (0122) | PAR-0023 verified the campaign-rules dialog opens |
| HUD / fragments | Wave pane verified (PAR-0060); gaps: block config families (0092/0093), block info panel (0094), command panel (0095), minimap attack indicators (0096), F6 status bars (0097), minimap colors (PAR-0057) | |
| Input / RTS | API + control paths verified (PAR-0062/0063); gaps: instant building without builders (0098), schematic copy mode dead (0099), focus/lock guards dead (0100), menu hotkeys leak (0101), axis rebind loses a direction (0102) | |
| Rendering | World layers (floor/blocks/light/fog/overlays) render; presentation delta calibrated in PAR-0037; gaps: flat unit composition (0126), no weather (0127), FX regions as flat quads (0128), shields invisible (0129), belt items invisible (0130), no launch cutscene (0131) | Minimap colors open (PAR-0057) |
| FX / audio | 267/267 effect catalog + audio math landed headless; live delivery missing: no effects/shake (0070), no loops (0071), silent menu music (0072), no block sounds (0073), dead volume sliders (0074) | |
| Maps / editor | Map list + editor open verified (PAR-0065/0067); gaps: editor canvas blank (0109), no New Map/menu (0110), exit discards silently (0111), no pan/pick/zoom (0113), map previews missing (0114) | |
| Mods | Loader, JSON patch surface, classmap landed (plan 20); gaps: mod content never injected (0115), scripts never run (0116), enable state not persisted (0117), no crash recovery/Restore (0118) | |
| Multiplayer | SpacetimeDB relay wired (host/connect/join); gaps: world actions do not sync (0120), chat inert (0121), admin surfaces dead (0122), no late-join catch-up (0123), dedicated `host` command never creates a match (0124), chat filters are database-global (0125) | Join-by-IP/public list are documented STDB deviations |
| Platform / export | Windows export exists; exported builds ship without `assets/` → placeholder quads (0119) | Android/iOS/Steam/Discord deferred (user-locked/ENV) |

## Biggest remaining gaps, ranked

1. **Combat runtime is not live** — turrets never fire, bullets pass through
   units, spawnUnit missiles don't exist (PAR-0086..0091, 0141). This cascades
   into the tutorial's turret/wave objectives and Erekir defense.
2. **FX/audio delivery to the view is unwired** — no impacts, explosions,
   loops, block sounds, menu music; volume sliders dead
   (PAR-0070..0074, 0128).
3. **Campaign progression isn't durable** — sector info resets on restart, no
   offline production/exports, submission sectors absent, loadout ignored
   (PAR-0081..0085, 0083).
4. **Logic can compute but not connect** — no editor, inert world sinks, blank
   displays (PAR-0103..0106).
5. **Editor is a shell over a blank canvas** — no draw pass, no New Map/menu
   (PAR-0109, 0110, 0113, 0114).
6. **Multiplayer sync loop is incomplete** — actions/chat/admin/late-join
   (PAR-0120..0124).
7. **Mods load but never change the game** (PAR-0115..0118).
8. **Building is instantaneous and builder-free**, and block behavior families
   (generators, pumps, drills, reconstructors, configs) are partial
   (PAR-0098, 0075..0079, 0092/0093).
9. **Unit/AI runtime depth** — enemy RTS/base AI, RTS command application,
   payload carry, terrain speed (PAR-0139, 0140, 0138, 0137).
10. **Export/asset shipping** (PAR-0119).

## Known deviations & calibration (already recorded)

- **Rendering gate calibration** (PAR-0037): Java bitmap fonts vs Godot TTF,
  atlas raster, and shader lighting make full-frame gates uncloseable for
  world frames; per-region residuals and the original gate are kept in the
  item. Same-class residuals: PAR-0015/0016/0017 (planet page), PAR-0057
  (minimap shroud/colors).
- **Multiplayer substrate**: Mindustry's Kryo net is deliberately replaced by a
  SpacetimeDB relay (plan 21): join-by-IP and the public server list are
  subsumed by `all_matches`/`connect_to`; in-match chat filters are
  database-global until the authoritative sim (PAR-0125, flagged for decision).
- **`.msav` import**: the Rust IO layer carries legacy import support, but the
  Load Game import path is dead end-to-end (PAR-0134).
- **Mobile / Steam / Discord / iOS**: deferred or user-locked (not gameplay
  parity items).

## Unrecorded leads (reported by the scan, below the recording bar)

Trace history lists / IconSelect "none" cell / Host color palette (UI);
radar filter+sort stubs (logic); build beams, block damage cracks, bloom and
pixelate toggles (render); campaign resume drops unit entity chunks; mobile
input variants; per-match ping binding and chat transport details.

## How to work the queue

1. Pick by severity: run `/fix-gaps` (or the parity-eval item loop) against the
   ledger; the S2 runtime clusters above are the highest leverage.
2. Every code-scan item needs its first in-engine walk: replay the fork, mirror
   the action, capture both legs, and settle `verified` or `mismatch` with
   fresh artifacts (skill `.opencode/skills/parity-eval/SKILL.md`).
3. After a code fix, the item settles through the writer → twin-evaluator
   pipeline (`/eval-gaps`) before it is closed.
4. Keep the ledger as the only queue: update `path`/`reproduce`/`steps` as runs
   refine them; never duplicate an existing symptom — link its id.

Scan provenance: `.opencode/evals/code-scan-20261010/` (curated findings +
raw per-domain staging files under `staging-curated/raw/`).

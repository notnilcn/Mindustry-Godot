# Parity plans — index and shared rules (2026-10-12)

Four implementation plans for bringing Mindustry-Godot's in-campaign gameplay to 1:1 with the Java game (`../Mindustry`). Each plan was written by the agent that did the first slice of the work. All of that work and all four plans are merged into `main` (merge commits after `2706550`).

| Plan | Area | Start with |
|---|---|---|
| [`campaign-import.md`](campaign-import.md) | Java profile import, planet page, autosave, launch loadout, chooser, landing cutscene, generated sectors | §3.A import residuals, then §3.B autosave |
| [`rendering-hud.md`](rendering-hud.md) | text/fonts, HUD, block/unit/conveyor-item sprites, FX, shields, info/command panels, overlays | §3.1 PAR-0152 placeholder building sprites |
| [`building-production.md`](building-production.md) | construction via build plans, sources/voids, reconstructors, Erekir drills, block config, schematic copy, per-family sweep | §11 execution order |
| [`combat-units.md`](combat-units.md) | spawnUnit bullets, statuses/abilities, floor speed, RTS orders, wave attacks, fires/puddles, weather, payloads | §2 item order |

Each plan opens with an environment section, a "done — do not redo" table, then one section per item: symptom, acceptance criterion, Java references with line numbers, current port state, ordered file-level changes, tests, verify commands, pitfalls.

## Corrections that apply to every plan (read before the plans)

The plans were written in parallel, before the merge. Where a plan disagrees with this list, follow this list.

1. **Checkout and session.** The plans name their own worktree (`../Mindustry-Godot-sessionN`), bridge port and display. On `main` use session 1: the repo root, bridge `6970`, display `:10`. For parallel executors run `tools/parity-session.sh --session N` (worktree `../Mindustry-Godot-sessionN`, bridge `6970+10(N-1)`, display `:(9+N)`), then `git merge --ff-only main` inside the worktree. Never run two executors in one checkout.
2. **Generated assets in a fresh worktree.** The sprite atlas, fonts, locales and shader index are gitignored build outputs. In any new worktree run `bash tools/pack.sh pack` (≈1 min) before the first launch, or the client shows placeholder sprites and fallback fonts.
3. **Ground Zero is sector 170, not 15.** The import remaps Java's legacy sector saves to the v8 sector ids: `sector-serpulo-15.msav` becomes `sector-serpulo-170.msav`, and Frozen Forest becomes 64. The launch recipes in `combat-units.md`, `rendering-hud.md` and `building-production.md` say `15`; use `170` once the profile has been imported with the merged code. Recipe:
   ```bash
   export OPEN_GODOT_MCP_PORT=6970   # your session's port
   python3 tools/gmcp.py --timeout 140 \
     --eval 'get_node("/root/Spine/SimHost").load_sector("serpulo", 170); return true' \
     --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 170)' \
     --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
   ```
   Check `get_node("/root/Spine/MindCampaign").get_hud_state()`: you want `hasCore: true` and `sectorName: "groundZero"`. A bare `start_sector` without `load_sector` gives an instant "Game Over / no core".
4. **Profile state.** `tools/session-play.sh N --import` imports `../mindustry.save.full.tech.tree.zip` through the real Game Data import. Expect `get_tech_state()` → `{"nodes":375,"unlocked":282}`. If it ever reads `unlocked: 9`, settings were clobbered (seen once on 2026-10-12, probably by a stale pre-merge game process). Re-run `--import` and kill stray games (`pgrep -af "godot --path"`).
5. **Restarts and memory.** After every `tools/build.sh`, run `tools/session-play.sh N`: the editor mmaps `libmind_gdext.so`, so it must restart to load the new library. This script kills the checkout's old game processes. Older restarts leaked them, and dozens piled up. The host has 14 GB RAM and each software-GL editor uses ≈1.4 GB, so run at most ~4 sessions in parallel.
6. **Already fixed elsewhere — skip:** `rendering-hud.md` §3.11 item 2 (planet page: preset name, sector grid, lock icons, "Sectors / N under attack" panel, Resources/Stats rows) was fixed by the campaign work (`7e14490`, `588a8c8`). Re-check against `runs/20261012-overview/java-planet-groundzero.png` and do only what still differs.
7. **Shared files.** These are edited by more than one plan: `mind-core/src/sim/mod.rs` (`Sim::tick` runs `update_builders()` then `resolve_deaths()`), `sim/runtime.rs` (`remove_building` is the shared tile-removal helper behind `break_block`, `destroy_building` and `remove_block_raw`), `sim/schedule.rs`, `mind-gdext/src/sim_host.rs`, `mind-gdext/src/render/blocks.rs`, `mind-gdext/src/campaign.rs`, and `io/settings.rs`. Settings stores now merge only their changed keys on save (`e0b6089`); never write the whole map back. Pull `main` before you start an item that touches them, and keep the commit small.
8. **Ledger.** `chains.json` is the compact parity ledger (query with jq; its `guide` key lists recipes). Don't dig through git history for an older `parity-ledger.json`. After an item passes its acceptance check, record the outcome in the item's notes and in its plan section.

## Workflow per item (summary; the plans have the details)

1. Read the plan section and the Java code it cites.
2. Reproduce the bug in-engine: launch recipe, `--eval` state JSON, `--screenshot runs/<stamp>-<item>/before.png`.
3. Make the ordered changes. Add the listed tests. Run `cd client/rust && cargo test -p mind-core <filter>` and `cargo clippy -p mind-core -p mind-gdext`.
4. `tools/build.sh`, then `tools/session-play.sh N`. Re-launch and capture `after.png`, then compare with the Java reference frame named in the plan.
5. Commit `Fixes PAR-XXXX: <summary>` (one item per commit).

## Baseline at the time of writing

- `cargo test -p mind-core --lib`: 1866 passed, 0 failed, 4 ignored.
- `mind-headless --lib`: 6 failures that already existed before this work (listed in `combat-units.md`).
- Ground Zero (sector 170) on `main` matches Java on: pixel UI font, portrait hexagon, inline item icons, objective marker colours, minimap colours and coordinates, palette defaulting to distribution, hint suppression. Compare `runs/20261012-overview/godot-merged-groundzero.png` with `java-groundzero-ingame.png`.

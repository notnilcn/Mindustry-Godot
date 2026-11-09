<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W5-C01 — Cleanup: shims, dead code, promoted helpers, docs, ledgers, CI hand-off

**Wave:** W5, alone, after every other package has merged.
**Owns:** everything the earlier packages left as shims, every doc/ledger file in RULES §9, and `tools/refactor/**`.
**Effort:** medium.

## Inputs

Every `docs/PLANS/reports/*.md` (sections "Follow-ups", "Promote in C01" and "Docs to update in C01")
and every `docs/PLANS/moves/*.tsv`.

## Steps

1. **Repoint imports, then delete the shims.**
   - Grep for every path through a shim:
     `grep -rn "dialogs::\(batch_[a-j]\|batch_campaign2\|misc\|simple\|sector\)::" client/rust --include=*.rs`.
     Rewrite each one to the item's new home. At `291e30a8` the known ones are:
     - `ui/fragments/player_list.rs:34` (`misc::trace_context_json` → `net::trace::trace_context_json`);
     - the `batch_a::open_content_info` users;
     - the `batch_campaign2::{Deferred, install_frame_tick, MapListState, maplist_*}` users from the old `batch_i`.
   - Delete the shim files `batch_a.rs … batch_j.rs`, `batch_campaign2.rs`, `misc.rs`, `simple.rs` and `sector.rs`.
     Remove their `pub mod` lines and their entries in `controller_for`.
   - Do the same for any other shim a report lists.
2. **Promote helpers** that reports list under "Promote in C01" into `gdutil`, `ui/kit` or
   `dialogs/common`, when two or more groups have identical copies. Examples: `connect_prompt_once`
   (`editor/` vs `rules/`), and the `building_center` copies outside `sim/` (H08's list). Apply the RULES §4
   diff rule.
3. **Dead code.** Remove items that became unused: old `launch_plan` (if `launch_plan_full`
   replaced every caller), facade remnants, `#[allow(dead_code)] // adopted in W2` markers whose
   item is now used. Remove items still unused after W2 (grep; MCP `#[func]`s are never "unused").
4. **Docs.**
   - Update the layout tables and rows that the reports name:
     - `client/rust/mind-core/src/game/AGENTS.md` (runtime row);
     - `client/rust/mind-core/src/sim/AGENTS.md` (layout table; add the nine-core-lookups table from H08);
     - `client/rust/mind-gdext/AGENTS.md` (sim_host/campaign/dialogs rows);
     - `client/AGENTS.md` (dialog conventions: `dialogs/<group>/<java_snake>.rs`, the util/common rules);
     - `client/rust/mind-gdext/src/ui/PORTING.md` and `PORT-STATUS.md` (fix the non-existent
       `factory::check`/`bar`/`warning_bar`/`items_display`, 02-FINDINGS #16);
     - `client/rust/mind-headless/AGENTS.md`;
     - `client/rust/mind-core/src/mods/AGENTS.md`.
   - Add a short "Where things live" section to `client/rust/mind-gdext/AGENTS.md`. It says that Godot
     classes split across files with `#[godot_api(secondary)]`, signals stay in the primary block,
     and dialogs live in `ui/dialogs/<group>/`, named after the Java class.
   - Fix the stale headers from 02-FINDINGS #13.
5. **Ledgers.** Using `moves/*.tsv`, rewrite stale `file:line` references in `TWIN-VERIFICATION.json`
   (about 30 `batch_*` and 18 `sim_host.rs` refs, plus test paths), `CAMPAIGN_GAPS.md`,
   `docs/GODOT-SCENES-REFACTOR.md`, `docs/parity-plans/{M01,L07}*`, `tools/export.sh:86` (comment)
   and `.port_audit.py:68`.
   - **`TWIN-VERIFICATION.json` is edited by parity agents all the time.** Edit it with a small script
     that only rewrites path strings (`old_path:line` → `new_path:line`, computing the new line from the
     moves table). Run it on fresh `main` in one quick commit, and check that the JSON is still valid
     (`python3 -m json.tool`).
   - Unknown references stay as they are; list them.
6. **Citation check.** Check that every `.rs` file created by the programme carries SPDX plus an
   `Upstream:` line. Example check:
   ```bash
   git diff --name-only --diff-filter=A <W0 base>..HEAD -- '*.rs' | while read f; do
     head -6 "$f" | grep -q 'SPDX-License-Identifier: GPL-3.0-only' || echo "NO SPDX $f"
     head -8 "$f" | grep -q 'Upstream:\|Ported from' || echo "NO CITE $f"
   done
   ```
7. **Size report.** List the files over 1500 lines that remain, with a reason each, in
   `docs/PLANS/reports/C01.md`:
   `find client/rust -name '*.rs' -not -path '*/target/*' -not -path '*/module_bindings/*' | xargs wc -l | sort -rn | head -40`.
   Compare with the baseline in 00-README (sim_host 13.5k, unit_runtime 9.5k, campaign 8.3k, …).
8. **CI hand-off.** Add one `CI_PENDING.json` entry covering the whole programme, with gates
   `fmt,clippy,cargo-check,cargo-test,parity,goldens,spine,godot-build,godot-import,shaders-check,ui-layout-check,ui-widgets-check,dialog-sweep,boundary`.
   Then the CI fixer runs `tools/run-cpu-intensive-task.sh --ci` ([`docs/CI-FIXER-PROMPT.md`](../CI-FIXER-PROMPT.md)).
9. Decide with the user whether `tools/refactor/` stays (it is useful for future splits) or goes.

## Done when

- No shim remains.
- The citation check is clean.
- Docs and ledgers point at the new paths.
- The full gate is green, as reported by the CI fixer.

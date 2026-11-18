<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Rules for every refactor package

Read this whole file before your package file. If your package file contradicts this file, the
package file wins only where it says "exception to RULES §n".

Read these guides once: the repo root [`AGENTS.md`](../../AGENTS.md),
[`client/rust/AGENTS.md`](../../client/rust/AGENTS.md), and the `AGENTS.md` of every crate and
module you touch. Example: S11 reads `client/rust/mind-core/AGENTS.md` and
`client/rust/mind-core/src/sim/AGENTS.md`.

Shorthand used in every package:
- `$MAIN` is `/home/clintonnguyen/g/code_examples/Mindustry-Godot` (the main checkout).
- `$WT` is your worktree.
- `BASE` is the plan base commit `291e30a8`. Line numbers in the plans are at `BASE`.
- `run` means `tools/ai-tools/resource-gate/run-resource-intensive-task.sh`. Always run it from `$WT`.

## §1 Worktree, branch, commits

```bash
cd $MAIN
git worktree add ../mg-<ID> -b refactor/<ID> main     # e.g. ../mg-S01, refactor/S01
cd ../mg-<ID>
bash tools/sync_scenarios.sh      # client/scenarios/ is a git-ignored mirror; some tests and run-all read it
```

- **Pin a branch; never `--detach` a package worktree.** A detached worktree has no branch for
  the orchestrator to merge or fast-forward, and its commits survive only in the reflog. If a
  worktree already exists detached (the earlier `mg-w*` ones do), pin it before use:
  `git -C ../mg-<ID> branch refactor/<ID> HEAD`, and name that branch in your report.
- Do all work in `$WT`. Never edit `$MAIN`, except that the orchestrator merges.
- Commit at least once per destination file group (small commits make review and bisect easy).
  Message format: `refactor(<area>): <what> [<ID>]`, for example
  `refactor(campaign): move research fns into campaign/research.rs [S01]`. End every commit
  message with the attribution line the session requires (currently
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`).
- Do not push. Do not rebase `main`. When you are done, the orchestrator merges your branch.
- If `main` moved and your package's files changed upstream, rebase onto `main` before you
  report: `git fetch` is not needed (same repo), use `git rebase main`. Re-run your checks after.

**Shared artifact cache (orchestrator, once per wave).** A fresh worktree has no
`client/rust/target/`, so its first `cargo` call compiles the whole dependency graph — 123 crates
for `mind-core` alone, ~24G of artifacts once warm. Point every worktree of the wave at one target
directory before dispatching the packages:

```bash
export CARGO_TARGET_DIR="$(git -C "$MAIN" rev-parse --path-format=absolute --git-common-dir)/mindustry-wave-target"
```

Each agent prefixes its queued commands with the same value (`CARGO_TARGET_DIR=… run --cargo-test …`),
because a fresh shell drops the orchestrator's export; `run` forwards the environment. The first
check pays the cold build once for the whole campaign, and every later check reuses the dependency
graph and recompiles only the workspace crates, whose build cargo keys to the checkout path
(measured: 3 crates in 26 s in a second worktree, with sources identical to `main`). Never copy or
hardlink-clone another checkout's `target/`: ext4 has no reflink on this volume, so a copy is a real
24G write per worktree, and cargo rewrites artifact and fingerprint files in place, so a hardlinked
inode can corrupt the tree it was cloned from. Give parity stages their own `PARITY_OUT_DIR`,
because the default lives inside the target dir. Delete the shared directory when the campaign
ends; it sits outside every checkout, so removing a worktree does not reclaim it.

## §2 One heavy job at a time, and close what you open

The queue serializes heavy work; it does not make it free, and it does not by itself say which
agent broke a build. Two rules give both.

**Run only the checks your package needs, from your worktree.** The orchestrator runs the packaged
resource monitor before a wave (`tools/ai-tools/resource-gate/resource-monitor.sh`; see
[`docs/CI-WORKFLOW.md`](../CI-WORKFLOW.md) "Wave guard"). Your worktree is the attribution
boundary: the orchestrator routes a red result back to the worktree it came from, so a red result
you report is one you own.

| You may run, from `$WT` | Form |
|---|---|
| format | `run --cargo-fmt`, then `run --cargo-fmt -- --check` |
| compile | `run --cargo-check -p <crate> --all-targets` |
| cross-crate compile (mechanical splits) | `run --cargo-check --workspace --all-targets` |
| lint | `run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p <crate> --all-targets -- -D warnings` |
| tests | `run --cargo-test -p <crate> [filter]` (mind-gdext: `-p mind-gdext --lib`) |
| determinism (§5 packages, once at the end) | `run --cargo-run -q -p mind-headless -- parity goldens`, `… -- run-all --tier T0`, `run --goldens --check` |

- Run every `cargo`, Godot, pack or golden command through `run` (see the root `AGENTS.md`),
  from `$WT`. Never call `cargo` directly. A second agent's build will wait for yours; that is
  expected.
- **Workspace check is allowed; workspace lint and workspace test are not.** A moved or narrowed
  item breaks importers in other crates, so `--cargo-check --workspace --all-targets` is part of a
  mechanical split (the packages that need it say so). Clippy and test stay per-crate: workspace
  clippy and workspace tests are the sweep's job, and running them here moves a failure after the
  merge, where no worktree owns it.
- **Scoped clippy.** `run --cargo-clippy` lints the whole workspace. For a one-crate lint use the
  `--cmd` form in the table; that line is the only `--cmd` a package runs.
- **Do not run** `--ci`, `--build`, `--pack`, `--export`, `--stdb`, `--bench`, `--parity`,
  `--pcam-parity`, `--godot`, `--goldens` without `--check`, or any other `--cmd`. Those belong to
  the **wave sweep**: the orchestrator runs them once per wave, after every package has reported
  and before merging, so a failure arrives with a worktree attached to it. The queue decides *when*
  a job runs, not *who owns* its failure.
- **Do not run Godot gates in your worktree at all.** Generated assets are git-ignored, so a fresh
  worktree cannot run them; the orchestrator runs them in `$MAIN` after the merge.
- Close everything you start (root `AGENTS.md` "Close what you open"). A turn that leaves a
  process running is incomplete.

## §3 What a *mechanical split* may change (all W1 packages)

A W1 package moves code. It does not rewrite it. Allowed changes, and only these:

1. **Move items verbatim**, with their doc comments and attributes. Line ranges in the plans
   usually start at the `fn`/`struct` line; the item's `#[test]`, `#[func]`, `#[derive]`, `#[cfg]`
   and `///` lines sit 1–5 lines **above** and always move with it. Keep each item's internal
   order and the relative order of items within a destination file. Order across files does
   not matter to Rust.
2. **Visibility** (the *original-scope rule*). An item that was private in module `M` and now
   lives in a child file `M::x` (or a deeper descendant) becomes `pub(in crate::<path of M>)`.
   For a direct child this is the same as `pub(super)`; prefer `pub(super)` there. This keeps
   its effective visibility unchanged. Items that were already `pub`/`pub(crate)` keep that.
   Struct fields that a moved item reads get the same treatment, but only if the struct
   itself moved; fields of a struct that stays in `mod.rs` are already visible to children.
   **Scattered users.** Sometimes the old file's private helpers end up used from several new
   directories, for example a `batch_*.rs` helper used by `dialogs/campaign/` and `dialogs/maps/`.
   Then use the nearest common ancestor module of all users, e.g. `pub(in crate::ui::dialogs)`.
   Never use a wider scope than needed. Never go to `pub` when `pub(crate)` suffices.
   Items that **stay** in the module root need no change: children already see them. Items that are
   already `pub`/`pub(crate)` keep their visibility exactly (do not narrow them). Methods cannot be
   re-exported with `use`; an `impl` method moved to another file stays callable under its old
   `Type::method` path, so it needs only the visibility rule, not a re-export.
3. **Imports and paths.** Add whatever `use` lines each new file needs. The easiest form is `use super::*;`
   (it imports the parent's private `use` items too) plus explicit extras. Remove imports that
   became unused; `clippy -D warnings` fails on them.
   - **Relative paths in moved code.** A `super::`/`self::` path inside moved code changes meaning
     when the code moves one module deeper. Rewrite it to the **equivalent** absolute path, e.g.
     `super::play` (in `game/runtime.rs`) → `crate::game::play`, and `use super::super::{dialog, kit}` (in
     a batch file) → `use crate::ui::{dialog, kit}`. Only the path text may change, never the item it
     names.
   - **Children you don't own.** Before deleting an import from a module root, check whether a
     child file you do **not** own uses `use super::*` and relies on it. Example: `sim/tests.rs:5`
     relies on `sim/mod.rs`'s imports. If so, keep the import. If it is only needed by tests, mark it
     `#[cfg(test)]`, so that clippy's unused-import check stays clean.
   - **Moved tests see less.** A test module moved from inline into `X/tests.rs` with `use super::*`
     sees only `X/mod.rs`'s items and imports, not the siblings'. Import the sibling items it needs
     explicitly (`use super::deaths::*;` etc.), and make them `pub(super)`.
4. **Re-exports.** The old module path of every item must keep resolving. In the new `mod.rs`,
   add `pub use child::Item;` (or `pub(crate) use`, matching the item's visibility). Do this
   for every item that the crate, another crate, a test or a doc names by path. **Grep before
   you decide an item is unused**: `grep -rn "<module>::<Item>\|<Item>" client/rust --include=*.rs`.
5. **`impl` blocks.** You may reopen `impl Type { … }` (or `impl Trait for Type`, which moves
   whole) in any file of the same crate. For a Godot class:
   - Exactly **one** `#[godot_api] impl Class` (the *primary* block) keeps every `#[signal]`,
     every `#[rpc]` and the `#[func]`s you choose to leave in `mod.rs`.
   - Every other block that contains a `#[func]` is `#[godot_api(secondary)] impl Class { … }`.
   - Private helpers with no `#[func]`/`#[signal]` go in a plain `impl Class { … }`.
   - `INode`/`IControl` trait impls (`init`, `ready`, `process`, …) stay in `mod.rs`.
   - Any fn named by a `#[var(get = …, set = …)]` field attribute **must stay in the primary block**.
     Secondary blocks do not feed the class's funcs collection (godot-macros
     `class/data_models/inherent_impl.rs` ~101–129). The same applies to `func_name!(…)` and to
     typed-signal `connect_self`/`connect_other` targets. At `291e30a8` the only such uses are
     `ui/planet.rs:151` (`get_planet`, `set_planet_value`) and `ui/kit/widgets/collapser.rs`.
     Before splitting a class, run
     `grep -rn "#\[var(.*\(get\|set\) *=\|func_name!\|connect_self\|connect_other" client/rust/mind-gdext/src`.
   - Never duplicate a `#[func]` name across blocks. The godot-rust version is 0.5.5;
     secondary blocks are documented in
     `~/.cargo/registry/src/index.crates.io-*/godot-macros-0.5.5/src/lib.rs` around line 1233.
6. **`X.rs` → `X/mod.rs`** with `git mv`, so history follows. Two styles exist in this repo: `fx.rs` +
   `fx/` (a file plus a directory) and `foo/mod.rs`. Use the one your package names. Never keep both
   `X.rs` and `X/mod.rs` (that is error E0761).
7. **Headers.** Every new `.rs` file starts with the same two header lines as its source file (copy
   them; the two existing orders are both fine), then a `//!` doc. The doc has one sentence on
   what lives here and a line `//! Upstream: core/src/mindustry/<path>.java` (with line ranges
   when only part of a Java file is ported). If the source cites no upstream file because the
   code is port-only, write `//! Upstream: none (port-only; <reason>)`.
8. **Fix nothing else.** A misplaced doc comment or a stale header may be fixed only when the
   package file lists it. A rename, a signature change, a new helper, or a reordered statement
   is **not** allowed in W1. Note every temptation in your report under "Follow-ups".

A file that has been emptied of all its items but is still referenced becomes a **shim**: the
header, `//! Moved to <new paths>. Deleted in W5 (C01).`, and the re-exports external code
still needs. For a dialog batch file the shim also keeps
`pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> { None }`.
Do not delete the file or its `mod` line; the ownership matrix gives `mod.rs` to another package,
so C01 deletes it.

**Size target.** Aim for files of 300–1000 lines. 1500 lines is the ceiling for non-test code,
unless the package says a data table or a single `match` can't be cut in W1. Test files may be
larger, but split them by topic when the package says so.

## §4 What a *helper / dedupe* package may change (W2, W3)

- **Behaviour must stay identical.** Extract a helper only when every replaced copy did the same
  thing. Before you replace a copy, `diff` its body against the helper. Bodies that differ in
  fallbacks (for example an `f64` branch, a string-to-int parse, or a default value) either keep
  their local copy or call a *differently named* helper whose name states the difference.
- **Do not touch** the order of statements that have side effects. This covers: Godot calls,
  `bind()`/`bind_mut()` scopes, `take_*`/`put_*` windows, RNG draws, ECS iteration order,
  `emit_*`, `persist()`, and `settings_hub::with_settings` closures. A helper that wraps such
  a sequence must perform it in exactly the same order.
- **Godot borrow rules.** Never hold a `bind()`/`bind_mut()` guard across a call that can re-enter the
  same object (a `call("…")`, a signal emit, `MindUi` calls). If the code being replaced used
  a separate statement to drop a guard early, keep that shape.
- Keep public, `#[func]` and `pub(crate)` signatures unchanged. A W2 package adds APIs; it does not
  remove them. **This also covers `pub(in …)` items used by files outside your package.** For example,
  `rules/util.rs` (H05d) has helpers used by `editor/editor_maps.rs` (H05c), and
  `campaign/util.rs::{Deferred, install_frame_tick}` (H04) is used through the `batch_campaign2`
  shim by `editor/editor_maps.rs`. Before you delete or rename a helper, run
  `grep -rn "<name>" client/rust --include=*.rs`. If any user is outside your owned files, keep the name.
  It can become a re-export or a thin wrapper of the shared version (e.g.
  `pub(crate) use crate::ui::dialogs::common::defer::Deferred;`). Only C01 removes items that became dead, and only after a grep proves it.
- Only shared helpers that already exist (the F1/F2/F3 modules, or a module your package
  owns) may be adopted. If you need a new helper outside the files you own, put it in a file
  you do own (for example your group's `util.rs`) and list it under "Promote in C01" in your
  report.

## §5 Determinism (anything under `mind-core/src/sim`, `game`, `world`, `combat`, `ai`)

Scenario checksums (`scenarios/*.json`) and sha256 goldens (`parity/golden_manifest.json`) pin
the simulation bit-for-bit. A mismatch means **you changed behaviour; revert and find out why**.
Never bless or regenerate a golden in a refactor package.

These must not change:
- ECS iteration order;
- `sort_by_key` keys;
- the float expression shape (no `mul_add`, `hypot`, `f64` widening, or reassociation);
- RNG call order;
- `add_systems` order in `sim/schedule.rs`;
- `remove_non_send`/`insert_non_send` vs `get_non_send` (removing the resource hides it from
  nested code).

## §6 Baseline and verification

Before your first edit, record the baseline in `$WT`. Keep it in your scratch notes, not in the
repo.

```bash
python3 tools/refactor/api_surface.py > /tmp/<ID>-surface-before.txt           # gdext packages
run --cargo-test -p <crate> -- --list 2>/dev/null | grep -E ': (test|bench)$' | sort > /tmp/<ID>-tests-before.txt
```

(`tools/refactor/` is created by F0. If it is missing, your branch predates W0; rebase onto `main`.)

After the work, every package runs **all** of the following that apply, in this order, and pastes
the tail of each output into its report:

| Check | Command | Pass |
|---|---|---|
| format | `run --cargo-fmt` then `run --cargo-fmt -- --check` | no diff |
| compile | `run --cargo-check -p <crate> --all-targets` | ok |
| lint | scoped clippy (§2) for each touched crate | 0 warnings |
| verbatim (W1 only) | `python3 tools/refactor/move_check.py --base BASE-or-branch-point --old <old files> --new <new files/dirs>` | `only-in-old` is 0, or each listed line is explained in the report |
| Godot API (gdext) | `python3 tools/refactor/api_surface.py \| diff /tmp/<ID>-surface-before.txt -` | empty |
| tests | `run --cargo-test -p <crate>` (mind-gdext: `-p mind-gdext --lib`) | all pass |
| test inventory | the `--list` command again, then compare to before | same count; only module paths changed (W1) or as listed (W2) |
| sim goldens (mind-core sim/game/world/io/mods) | `run --cargo-run -q -p mind-headless -- parity goldens` and `run --goldens --check` and `run --cargo-run -q -p mind-headless -- run-all --tier T0` | all pass, no checksum drift |
| feature-gated tests (S11/H07/H08) | `run --cargo-test -p mind-core --features msav-import sim::unit_runtime` | pass, or the same failures as at the baseline (record them) |

For `move_check.py`, `--base` is the commit your branch started from: `git merge-base main HEAD`
at the time you created the branch. Run it once per source file, listing all the destination
files and directories that file fed. Lines it reports under `only-in-new` that are structural
(`#[godot_api(secondary)]`, new `impl` headers, re-exports) are fine. Every `only-in-old` line
must be explained.

## §7 Reports and move tables

When you finish, commit two files in your branch. Both names are unique per package, so they never
conflict:

1. **`docs/PLANS/moves/<ID>.tsv`** (W1 packages only). One line per moved item, tab-separated:
   `old_path  old_first_line(BASE)  old_last_line(BASE)  item_name  new_path`. C01 uses it to
   rewrite stale `file:line` references in the ledgers.
2. **`docs/PLANS/reports/<ID>.md`**, using this template:

```markdown
# <ID> report
- Branch: refactor/<ID> @ <sha>   Base: <merge-base sha>
- Status: done | partial (what is left) | blocked (why)
## What moved / changed
<table or bullets: source → destination>
## Verification
<each §6 check: command + last lines of output>
## Deviations from the plan
<anything you did differently and why>
## Follow-ups
- Promote in C01: <local helpers that should become shared>
- Docs to update in C01: <AGENTS.md rows, PORT-STATUS, etc.>
- Behaviour bugs noticed (NOT fixed): <file:line + description>
## CI_PENDING entry
python3 tools/ci_pending.py add "<summary>" --agent refactor-<ID> --area <area> \
  --files <comma list> --gates <fmt,clippy,cargo-check,cargo-test,...> \
  --repro "<command>" --expected "<pass signal>" --risk "<what is most likely to break>"
```

## §8 When the plan is wrong

Line ranges drift, and analysis can miss something. If an item named in the plan does not exist
at HEAD, or an extra item exists, decide by topic which destination file it belongs to and
record it under Deviations. If a whole step is impossible (for example a borrow error that you
can only fix by restructuring), leave that part in place, finish the rest, and record it. Do
not improvise a redesign.

## §9 Files nobody but C01 / the orchestrator edits

- `CI_PENDING.json`. Your report carries the entry; the orchestrator adds it after the merge. In a
  worktree the file is a per-checkout copy and would conflict.
- Every `AGENTS.md`, `PORT-STATUS.md`, `PORTING.md`, `TWIN-VERIFICATION.json`, `CAMPAIGN_GAPS.md`,
  `.port_audit.py`, and everything under `docs/` except your own `moves/` and `reports/` files.
- `.tscn`, `client/ui/*manifest*.json`, `mind-stdb/src/module_bindings/**`, and any golden or
  scenario JSON.
- The user's in-flight areas (00-README "Never touched").

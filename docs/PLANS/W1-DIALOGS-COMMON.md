<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Common procedure for the W1 dialog-split packages (S03, S04, S06–S10)

Read [W0-F2](W0-F2-dialogs-scaffold.md) first: it describes registration, the target tree and
the ABI strings. Every dialog package follows these steps, plus its own destination table.

## Per dialog (one commit per dialog, or per small group)

1. **Locate** the controller struct, its `impl DialogController`, its state types, and every free
   fn and constant used only by it. Use the package's line ranges as hints, and the section comments
   (`grep -n "^// \(=\|-\)\{3,\}\|^// [A-Z]" <file>`) to confirm the boundaries.
2. **Cut** them verbatim into the destination leaf (below the header, replacing the stub's `use` and
   `dispatch` lines).
3. **Dispatch.** Replace the leaf's stub `dispatch` with a `match` holding only this dialog's id
   arm(s), copied verbatim from the batch file's `dispatch`, then `_ => None`. **In the same commit**,
   delete those arms from the batch file's `dispatch`.
4. **Imports.** Add `use` lines for the shared helpers it needs, from the util leaf of the same
   origin. Example: code from `batch_e.rs` imports `super::util_e::*`, or
   `crate::ui::dialogs::editor::util_e::*` from another group.
5. `run --cargo-check -p mind-gdext`.

## Shared helpers of a batch file

Each batch file starts with a private helper section. Move that whole section into the util leaf
named in your table (`util_<letter>.rs` or `util.rs`) **verbatim, without deduplicating**. Several
util leaves will hold the same `defer_action`/`ui_call`; H05 removes those copies in W2.
Visibility: `pub(in crate::ui::dialogs)` (RULES §3.2, scattered users), unless an item was already
`pub`/`pub(crate)`.

## Group `mod.rs` edits (exception to the F2 ownership)

F2 owns the group `mod.rs` files. A package **may** edit a group `mod.rs`, but only to add
`pub mod <new_file>;` (or `mod`), and only if your package is the **only W1 package that fills
that group**. Never edit the group's `dispatch` chain except to add a `.or_else(|| new::dispatch(id))`
for a new D file.

Groups and their single W1 owners:
- `menu/`, `settings/`, `database/`: S06;
- `game/`, `net/`: S07;
- `dev/`: S08;
- `rules/`: S09;
- `mods/`: S10;
- `maps/`: S03.

The shared groups are `campaign/` (S03, S04, S07), `pickers/` (S03, S07, S08, S10), `editor/`
(S08, S09), `logic/` (S09, S10) and `schematics/` (S03, S09). In a shared group, use only the
leaves F2 created. You may turn a leaf **you own** into a directory: `git mv x.rs x/mod.rs`, then add
children inside `x/`. The group `mod.rs` line `pub mod x;` resolves either way. Do this when a leaf
would exceed about 1500 lines.

## The emptied batch file

It becomes a shim (RULES §3): the header,
`//! Moved to <dirs>. Deleted in W5 (C01).`, the stub `dispatch` returning `None`, and
`pub(crate) use` re-exports for items used **outside** the file. Find them with:
```bash
grep -rn "<batch_name>::" client/rust/mind-gdext/src --include=*.rs | grep -v "^client/rust/mind-gdext/src/ui/dialogs/<batch_name>.rs"
```

## Cross-file imports of *other* batch files

If code you move uses an item from another batch file (`batch_a::open_content_info`,
`batch_campaign2::{Deferred, install_frame_tick, MapListState, maplist_*}`,
`misc::trace_context_json`), import it by its **old full path**, e.g.
`use crate::ui::dialogs::batch_a::open_content_info;`. That path stays valid whether or not the
other package has merged yet, because a shim re-exports it. C01 repoints the import later.

## ABI that must survive byte-for-byte

- dialog ids;
- `.connect("<signal>", …)` names and `Callable::from_fn("<name>", …)` names;
- node meta keys: `"_batch_d_maps_rebuild"` (set in `batch_d.rs` ~1741, read in `batch_i.rs` ~3216), `"_batch_c_frame_"`,
  `"_campaign2_frame_"`, `"mind_dialog_request"`, and any other `set_meta`/`get_meta` string;
- the GodotClass struct names `MindCanvasSurface` (`batch_i.rs`) and `MindLogicOverlay` (`batch_j.rs`).
  Move them whole, with their `#[derive(GodotClass)]` and every impl block.

The meta keys keep their old `_batch_*` spelling even though the files are gone. Renaming them is
a separate change, outside this programme.

## Tests

Each `#[cfg(test)] mod tests` item goes to the file of the code it tests (the package lists them).
A test that exercised the batch file's `dispatch` changes its call target to
`crate::ui::dialogs::controller_for(id)`. That is the one allowed test edit.

## Verification (RULES §6)

```bash
B=$(git merge-base main HEAD); D=client/rust/mind-gdext/src/ui/dialogs
python3 tools/refactor/api_surface.py | diff /tmp/<ID>-surface-before.txt -     # empty
python3 tools/refactor/move_check.py --base $B --old $D/<batch>.rs --new $D/<batch>.rs <every dir/leaf you filled>
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib dialogs
```

The test inventory must be unchanged except for module paths. The orchestrator runs `dialog_sweep_check`
(`DIALOGCHECK: failed=0`) after merging.

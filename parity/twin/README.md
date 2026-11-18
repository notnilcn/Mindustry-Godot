<!-- SPDX-License-Identifier: GPL-3.0-only -->
# parity/twin — canonical twin-verification frames

Committed AVIF copies of the canonical Java/Godot frame pair for an item in
[`TWIN-VERIFICATION.json`](../../TWIN-VERIFICATION.json).

Only items that are `verified` and either fixed (`fix.status` in
`closed`/`done`/`none-needed`) or twin-verified get a committed set; `seed` and
unverified items keep their PNGs in the gitignored `runs/` tree. Each set is
one or two index-aligned pairs:

```
parity/twin/<PAR-ID>/java/01-fork.avif       # optional
parity/twin/<PAR-ID>/java/02-action.avif     # canonical action pair
parity/twin/<PAR-ID>/godot/01-fork.avif
parity/twin/<PAR-ID>/godot/02-action.avif
```

`TWIN-VERIFICATION.json.items[<PAR-ID>].images` lists both legs in the same
order. `similarity` is `100 * (1 - changed_fraction)` for the action pair,
rounded to one decimal; a low percentage can still be a structured-state pass
(the retired ledger classified many twin pairs as presentation only).

Canonical sets are refreshed from the fresh pair captured for a
`twin-verified` item under `runs/<PAR-ID>/{java,godot}/`. Convert each frame to
AVIF with Pillow and `pillow-avif-plugin` (quality 70), write the files into
`parity/twin/<PAR-ID>/{java,godot}/`, and list both legs in the registry
`images` object in the same order. The packaged validator
(`python3 tools/ai-tools/twin/twin_verify.py check --root .`) checks that every
listed file exists. Delete superseded PNG/diff artifacts from `runs/` by hand
after the registry points at the new AVIF pair.

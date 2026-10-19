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

Generate/refresh with `tools/twin_verification.py images` (Pillow +
`pillow-avif-plugin`; AVIF quality 70). Prune the replaced source PNG/diff
artifacts with `tools/twin_verification.py prune --apply`.

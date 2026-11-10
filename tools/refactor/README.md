<!-- SPDX-License-Identifier: GPL-3.0-only -->
# tools/refactor — checks for the docs/PLANS refactor programme

- `api_surface.py [roots…]` prints sorted `class X` / `func X::f` / `signal X::s` lines for
  the gdext crates. Diff it before and after a split; the diff must be empty.
- `move_check.py --base <rev> --old <files> --new <files/dirs>` is a multiset diff of
  normalized code lines. It ignores whitespace, visibility, `use`/`mod` lines, comments, lone
  braces and `impl` headers. `only-in-old` must be 0 for a pure move.

Both scripts are read-only and need no build. See docs/PLANS/01-RULES.md §6.

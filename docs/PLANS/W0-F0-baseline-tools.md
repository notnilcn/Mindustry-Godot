<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W0-F0 — Refactor tools and baseline

**Wave:** W0, first package, runs alone. **Owns:** `tools/refactor/**`, `docs/PLANS/baseline/**`,
`docs/PLANS/moves/.gitkeep`, `docs/PLANS/reports/.gitkeep` (exception to RULES §9 for these
paths only). **Effort:** small (under an hour).

## Goal

Give every later package two mechanical checks, plus a committed baseline:
- `api_surface.py` prints the Godot-visible API (classes, `#[func]`, `#[signal]`). Splitting a
  class across files must leave it unchanged.
- `move_check.py` proves that a split moved lines verbatim.

Both scripts were prototyped against `291e30a8`. `api_surface.py` reports 1890 lines: 119
classes, `MindCampaign` 104 funcs, `MindSimHost` 158 funcs. `move_check.py` was tested: a
self-comparison of `campaign.rs` is clean, and a one-token change is caught.

## Steps

1. Create the worktree (RULES §1) as `../mg-F0`, branch `refactor/F0`.
2. Create `tools/refactor/api_surface.py` with exactly this content, then `chmod +x` it:

```python
#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Print the Godot-visible API surface of the gdext crates, one sorted line per item.

Lines: `func <Class>::<name>`, `signal <Class>::<name>`, `class <Class>`.
Run before and after a refactor; the diff must be empty unless the plan says otherwise.
"""
import re
import sys
from pathlib import Path

ROOTS = sys.argv[1:] or ["client/rust/mind-gdext/src", "client/addons/phantom_camera_rust/rust"]
IMPL_RE = re.compile(r"^\s*impl\s+(?:[\w:]+\s+for\s+)?(\w+)\b")
CLASS_RE = re.compile(r"#\[derive\([^)]*GodotClass")
STRUCT_RE = re.compile(r"^\s*pub(?:\([^)]*\))?\s+struct\s+(\w+)|^\s*struct\s+(\w+)")
FN_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?fn\s+(\w+)")
RENAME_RE = re.compile(r"rename\s*=\s*(\w+)")

out = set()
for root in ROOTS:
    for path in sorted(Path(root).rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        current = None
        pending = None  # ("func"|"signal", rename or None)
        want_class = False
        for line in lines:
            if CLASS_RE.search(line):
                want_class = True
            if want_class:
                m = STRUCT_RE.match(line)
                if m:
                    out.add(f"class {m.group(1) or m.group(2)}")
                    want_class = False
            m = IMPL_RE.match(line)
            if m:
                current = m.group(1)
            s = line.strip()
            if s.startswith("#[func") :
                r = RENAME_RE.search(s)
                pending = ("func", r.group(1) if r else None)
                continue
            if s.startswith("#[signal"):
                pending = ("signal", None)
                continue
            if pending:
                m = FN_RE.match(line)
                if m:
                    kind, rename = pending
                    out.add(f"{kind} {current}::{rename or m.group(1)}")
                    pending = None
for item in sorted(out):
    print(item)
```

3. Create `tools/refactor/move_check.py` with exactly this content, then `chmod +x` it:

```python
#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Verbatim-move checker for refactor splits.

Compares the code lines of OLD files at a git revision against NEW files in the
working tree, as multisets, after normalizing away what a pure move is allowed
to change (indentation, visibility modifiers, `use`/`mod` lines, headers, doc
comments, lone braces, `impl` headers). Anything printed is a line whose count
differs: review each one. A clean pure move prints only structural lines.

usage: move_check.py --base <rev> --old <path>... --new <path-or-dir>...
"""
import argparse
import collections
import re
import subprocess
import sys
from pathlib import Path

VIS = re.compile(r"\bpub(\s*\((crate|super|self|in\s+[\w:]+)\))?\s+")
SKIP = [
    re.compile(r"^(pub\s+)?use\s"),
    re.compile(r"^(pub(\(.*?\))?\s+)?mod\s+\w+\s*;"),
    re.compile(r"^(pub(\(.*?\))?\s+)?mod\s+\w+\s*\{$"),  # inline `mod tests {` header
    re.compile(r"^//"),  # comments, docs, SPDX/citation headers
    re.compile(r"^#\[godot_api"),
    re.compile(r"^#\[cfg\(test\)\]$"),
    re.compile(r"^#!\[allow"),
    re.compile(r"^(unsafe\s+)?impl\b.*\{$"),
    re.compile(r"^[\}\)\];,]*$"),
]


def norm(line: str):
    s = line.strip()
    if not s:
        return None
    s = VIS.sub("", s)
    for rx in SKIP:
        if rx.search(s):
            return None
    return s


USE_START = re.compile(r"^(pub(\s*\([^)]*\))?\s+)?use\s")


def count(text: str, bag: collections.Counter):
    in_use = False  # inside a multi-line `use … { … };`
    for line in text.splitlines():
        s = line.strip()
        if in_use:
            in_use = not s.endswith(";")
            continue
        if USE_START.match(s) and not s.endswith(";"):
            in_use = True
            continue
        n = norm(line)
        if n is not None:
            bag[n] += 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--old", nargs="+", required=True)
    ap.add_argument("--new", nargs="+", required=True)
    a = ap.parse_args()
    old, new = collections.Counter(), collections.Counter()
    for p in a.old:
        count(subprocess.run(["git", "show", f"{a.base}:{p}"], check=True,
                             capture_output=True, text=True).stdout, old)
    for p in a.new:
        path = Path(p)
        files = sorted(path.rglob("*.rs")) if path.is_dir() else [path]
        for f in files:
            count(f.read_text(encoding="utf-8"), new)
    removed = old - new
    added = new - old
    for label, bag in (("only-in-old (LOST?)", removed), ("only-in-new (ADDED?)", added)):
        print(f"== {label}: {sum(bag.values())} lines")
        for line, n in sorted(bag.items()):
            print(f"{n:4} {line}")
    sys.exit(1 if removed else 0)


if __name__ == "__main__":
    main()
```

4. Create `tools/refactor/README.md`:

```markdown
<!-- SPDX-License-Identifier: GPL-3.0-only -->
# tools/refactor — checks for the docs/PLANS refactor programme

- `api_surface.py [roots…]` prints sorted `class X` / `func X::f` / `signal X::s` lines for
  the gdext crates. Diff it before and after a split; the diff must be empty.
- `move_check.py --base <rev> --old <files> --new <files/dirs>` is a multiset diff of
  normalized code lines. It ignores whitespace, visibility, `use`/`mod` lines, comments, lone
  braces and `impl` headers. `only-in-old` must be 0 for a pure move.

Both scripts are read-only and need no build. See docs/PLANS/01-RULES.md §6.
```

5. Sanity-run both scripts from the repo root:
   ```bash
   python3 tools/refactor/api_surface.py | wc -l            # 1888 at a clean 291e30a8 (1890 with the user's uncommitted render edits); more if main grew
   python3 tools/refactor/api_surface.py | grep -c '^func MindCampaign::'   # 104 at 291e30a8
   python3 tools/refactor/move_check.py --base HEAD \
       --old client/rust/mind-gdext/src/campaign.rs --new client/rust/mind-gdext/src/campaign.rs
   # expect: "only-in-old (LOST?): 0 lines" and exit 0
   ```
6. Take the baseline **in your fresh worktree**, after the user's in-flight work has been committed to `main`
   (00-README orchestrator step 1), so later diffs are not polluted. Commit the baseline: `mkdir -p docs/PLANS/baseline` and run
   `python3 tools/refactor/api_surface.py > docs/PLANS/baseline/api_surface.txt`. In the same
   commit, add empty `docs/PLANS/moves/.gitkeep` and `docs/PLANS/reports/.gitkeep`.
7. Record the test inventory baseline. These are slow, so run them through the queue, one at a time:
   ```bash
   for c in mind-core mind-headless; do
     tools/run-cpu-intensive-task.sh --cargo-test -p $c -- --list 2>/dev/null \
       | grep -E ': (test|bench)$' | sort > docs/PLANS/baseline/tests-$c.txt
   done
   tools/run-cpu-intensive-task.sh --cargo-test -p mind-gdext --lib -- --list 2>/dev/null \
       | grep -E ': (test|bench)$' | sort > docs/PLANS/baseline/tests-mind-gdext.txt
   wc -l docs/PLANS/baseline/tests-*.txt
   ```
   If a crate fails to build at the baseline, stop and report it; the programme needs a green start.
   If tests, goldens or `run-all` fail only because the fresh worktree lacks generated files,
   find the bootstrap step that fixes it (first `bash tools/sync_scenarios.sh`; asset packing is not
   needed for cargo tests). Write it into the report, and note it in RULES §1 for the orchestrator.
8. Run the golden checks once and paste the result into the report, so later drift is
   attributable:
   `tools/run-cpu-intensive-task.sh --cargo-run -q -p mind-headless -- parity goldens` and
   `tools/run-cpu-intensive-task.sh --goldens --check`.
9. Write `docs/PLANS/reports/F0.md` (RULES §7). Use `--gates fmt` for the CI entry; the scripts are Python only.

## Done when

- Both scripts exist, are executable and behave as in step 5.
- The `docs/PLANS/baseline/` files are committed.
- The report contains the golden-check output.

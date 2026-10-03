# `parity/golden/` — oracle golden artifacts (plan 23 M2)

Committed JVM/source-derived oracle goldens, pinned by sha256 in
`../golden_manifest.json` and verified by `parity goldens`.

| Domain | File | Source | Owner |
|---|---|---|---|
| `logic/` | `field_order.json` | `13_...` §6.2 / upstream `logic/LStatements.java` | 13 |
| `ui/` | `ui_keys.json` | upstream `ui/builder/UiKey.java` enum order | 14 |
| `io/` | `rules_fields.json` | upstream `game/Rules.java` public field order | 04 |
| `fx/` | `fx_order.json` | upstream `content/Fx.java` effect declaration order | 17 |

**Provenance.** The primary oracle is the one-off JVM dump set in
`../java/Dump{LogicIO,Ui,JsonIO,Fx}.java` (`NUD-10/A`). Running them requires a
Gradle-built upstream checkout (`:core` + `:tests`); CI never needs a JVM because
the outputs are frozen here.

**Source-derived fallback.** The lane that added these files could not complete
the timeboxed Gradle/JVM build, so each golden was produced by
`../java/extract_source_goldens.py`, which parses the exact upstream Java files at
`parity/upstream.lock` (commit `2cd7aeec…`) and records the source sha256. This is
the documented deferral: `Dump*.java` remain the primary regeneration path and the
field order matches the observed Java declaration order (plan 13 §6.2; `Fx` 267
matches the plan-17 audit).

Regenerate with `tools/regen_goldens.sh --only <id>` and update the manifest hash in
the same commit.

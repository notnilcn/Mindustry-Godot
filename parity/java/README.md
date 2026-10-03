# `parity/java/` — one-off JVM oracle dumps (plan 23 M2, NUD-10/A)

Standalone Java tools that dump upstream Mindustry data so the Rust port can be
pinned against it. They are **never linked into the port**; outputs are committed
under `parity/golden/`, so CI never needs a JVM.

| Dumper | Golden | Owner |
|---|---|---|
| `DumpContent.java` | `parity/golden_content.json` | 02 |
| `DumpWaves.java` | `client/rust/mind-core/tests/golden/units/wave_generate.json` | 11 |
| `DumpLogicIO.java` | `parity/golden/logic/field_order.json` | 13 |
| `DumpUi.java` | `parity/golden/ui/ui_keys.json` | 14 |
| `DumpJsonIO.java` | `parity/golden/io/rules_fields.json` | 04 |
| `DumpFx.java` | `parity/golden/fx/fx_order.json` | 17 |

Run `DumpContent.java` with `run.sh`; the newer dumpers compile the same way
(change the class name). Building the upstream checkout is a local, opt-in step.

**Current status (this lane).** The Gradle/JVM build was timeboxed and not
completed, so the logic/ui/io/fx goldens were produced by the reproducible
source-derived fallback `extract_source_goldens.py` (it parses the same upstream
Java files and records the source sha256). The `Dump*.java` sources are committed
and are the primary regeneration path once a built checkout is available:
`MIND_JAVA_PARITY=1 tools/regen_goldens.sh --only <id>`.

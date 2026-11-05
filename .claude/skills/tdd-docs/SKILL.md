---
name: tdd-docs
description: Generate or regenerate the Khvor' Gates technical design docs in docs/HUMAN/_TDD with an orchestrator and parallel per-system subagents — the JSON table map, CriticMarkup merge, orchestrator sweep, and final sync-embed pass. Use when asked to generate, regenerate, extend or review the per-system TDD docs or the 00 Table Map.
---

# Technical design docs (TDD) — orchestrator skill

This is the working plan for (re)generating the technical design docs in
`docs/HUMAN/_TDD/`. It is written for the **orchestrator**: the one agent that
owns the shared artifacts, spawns the per-system doc subagents, and performs the
final merge, sweep and sync-embed pass. Every subagent the orchestrator spawns
must read this file first.

The work is parallel by design: one subagent per per-system doc. Only the shared
artifacts serialize — the initial table map, pointers and flowcharts before the
fan-out; the table-map merge, orchestrator sweep, CriticMarkup resolution and
sync-embed application after it.

The docs are the narrative, learn-from-scratch companion to the terse maintainer
references: the root `AGENTS.md`, `client/AGENTS.md`,
`.claude/skills/godot-scenes/SKILL.md`, `server/AGENTS.md`,
`client/rust/kg_gdext/AGENTS.md`, `client/rust/kg_net/AGENTS.md`,
`client/rust/kg_client_model/AGENTS.md`, `client/rust/kg_editor/AGENTS.md`,
`server/modules/global/AGENTS.md`, `server/modules/region/AGENTS.md`,
`server/relay/AGENTS.md` and `server/audit/AGENTS.md`.

---

## 1. The orchestrator's job

The orchestrator runs the generation in six phases, numbered 0–5, in this order:

1. **Phase 0 — setup.** Refresh pointers and flowcharts, fix the doc set, write
   `01 Roadmap.md`, and write the initial `00 Table Map.json` in full.
2. **Phase 1 — fan out.** Spawn one doc subagent per per-system doc (02–22) in
   parallel. Each writes its own doc plus its own CriticMarkup version of the
   table map. No sync embeds are written in this phase.
3. **Phase 2 — merge.** Spawn one table-map merge subagent whose only job is to
   fold every per-agent table-map version into a single `00 Table Map.md` draft
   that still contains every CriticMarkup edit unresolved.
4. **Phase 3 — sweep.** Read every TDD doc end to end and repair doc-level
   problems. Do not resolve the table map yet and do not add sync embeds yet.
5. **Phase 4 — resolve and embed.** Resolve every CriticMarkup edit in the
   `00 Table Map.md` draft, then apply the sync-embed blocks to the per-system
   docs.
6. **Phase 5 — final verification.** Re-run pointers and flowcharts, fill the
   roadmap status table, run the whole-set checklist, and report.

The orchestrator **owns** the initial `00 Table Map.json` and the final
`00 Table Map.md`. The merge subagent writes the draft of the latter; the
orchestrator is accountable for its final resolved content.

**The orchestrator must never read git history.** Do not run `git log`,
`git show`, `git blame`, or diff old commits. Work from the current working
tree. (`git status` and the working-tree diff are fine.) Per-system doc
subagents **may** read git history; the merge subagent may not.

---

## 2. Roles, ownership and permissions

| Role | Count | Writes | May read git history? |
| --- | --- | --- | --- |
| Orchestrator | 1 | `00 Table Map.json`, final `00 Table Map.md`, `01 Roadmap.md`, `flows.json`, pointers, flowchart commands, `.tdd/{sweep,resolution,final}-report.md` | **No** |
| Per-system doc subagent | 1 per doc 02–22 | its own doc and, for 04+, its own `.tdd/table-map/<NN>-<slug>.json` | **Yes** |
| Table-map merge subagent | 1 | `00 Table Map.md` draft and `.tdd/table-map/_merge-report.md` | **No** |

Ownership rules:

- Only the orchestrator writes `00 Table Map.json`. It writes it once in
  Phase 0 and does not edit it afterwards.
- No doc subagent writes `00 Table Map.json` or `00 Table Map.md`. A doc
  subagent that wants a table entry changed records a CriticMarkup edit in its
  **own** table-map version.
- The merge subagent writes only the draft `00 Table Map.md` and its merge
  report. It never resolves edits, never edits a doc, never runs plugin
  commands and never reads git history.
- Doc 02 and doc 03 own no tables, so they do not write a table-map version.
  Their cross-doc or pointer concerns go in a `**Sweep note:**` at the end of
  their doc.
- The orchestrator is the only role allowed to run the Obsidian
  `vscode-editor` flowchart and pointer commands and the only role allowed to
  edit `flowcharts/flows.json`.

Parallel doc subagents must never see each other's unfinished docs as
authoritative; the base JSON and the code are the shared truth.

---

## 3. The repo these docs describe

- **Server tables** live in exactly two places: `server/shared/framework.rs`
  (spliced into both module roots with `include!`) and a system's `tables.rs`
  under `server/modules/global/src/` or `server/modules/region/src/`.
- `server/modules/global/` (`kg-global`, module id 0) has the systems `account`,
  `patch`, `registry` and `world`.
- `server/modules/region/` (`kg-region-1`, ...; module ids 1..=254) has the
  systems `ability`, `audit`, `clock`, `combat`, `config`, `enemy`, `health`,
  `movement`, `pass`, `patch`, `player`, `stats`, `weapon` and `world`.
  `src/aoi.rs` and `src/time.rs` are support files, not systems.
- The framework tables are defined once in `server/shared/framework.rs` but
  exist in **both** `kg-global` and every region database: `admin`,
  `relay_identity`, `globals`, `outbox`, `outbox_seq`, `inbox_watermark`,
  `inbox_result`, `admin_audit` and the scheduled `gc_schedule`. Its views are
  `relay_outbox` and `relay_results`.
- `server/crates/kg_intermod` owns `InterModuleMessage` and its BSATN codec.
  `server/relay` and `server/audit` are native sidecars, not modules; neither
  owns tables.
- **Client read path**: `client/rust/kg_net` owns `NetSession` and the generated
  bindings; it turns subscribed table/view rows into
  `kg_client_model::ModelEvent`s and session state.
  `client/rust/kg_client_model` owns `WorldModel` and the Godot-free combat
  model. `client/rust/kg_gdext` owns the runtime Godot-facing `Kg*` classes and
  drives the `.tscn` trees in `client/scenes/`. `client/rust/kg_editor` is the
  editor-only dev dock (it can read tables over HTTP, but never loads in an
  export).
- `tools/kg_bot` is a headless client that uses `kg_net`/`kg_client_model`
  directly; it consumes the same read path as the game (doc 03) but has no
  Godot scene.
- `crates/kg_*` (`kg_hex`, `kg_rng`, `kg_sim`, `kg_rules`, `kg_world`,
  `kg_content`, `kg_combat`, `kg_headless`) are pure crates: they compute values
  and define content, but they own no tables.

If a table or a view is not in `framework.rs` or a `*/tables.rs` /
`*/views.rs` file, it is not a table or a view in this repo. A file that writes
or reads one is a changer or a reader, not a schema owner.

Current-state notes in this skill (pointer granularity, `flows.json`, known
gaps) are a 2026-10-28 snapshot; re-verify them against the code before acting.

---

## 4. What these docs are

These docs are centered on **how the tables are viewed and changed** — not on
features, flows or code organization in the abstract. Every table has exactly
one story worth telling: **how it is changed** (which reducer, scheduled tick,
lifecycle hook, seed, inter-module handler or admin tool inserts/updates/deletes
rows) and **how it is viewed** (which raw public table or view carries it over
the wire, which `kg_net` subscription and callback consume it, which
`ModelEvent`/session state and `WorldModel` field it becomes, and which `Kg*`
node or UI element it ends up driving — plus the server-side reads that matter).
Each system doc is a group of tables told through those two lenses. During
generation the machine-readable spine is `00 Table Map.json`; the published
spine the final docs transclude from is `00 Table Map.md`. If a
mechanism doesn't change a table or explain how a table is viewed, it doesn't
get its own section — it belongs as a sentence inside the Changers/Readers
prose of the table(s) it touches.

The docs live in `docs/HUMAN/_TDD/`. **The Obsidian vault is the repo root**
(the `.obsidian` folder is there), not `docs/` — that is the only way the docs'
wikilinks to code and to the canvases resolve. All paths below are relative to
the repo root. The vault uses the `sync-embeds`, `vscode-editor` and
`advanced-canvas` community plugins. `sync-embeds` provides the `sync` block;
`vscode-editor` provides `[[file#symbol]]` links, pointer generation and the
canvas flowcharts; `advanced-canvas` is a hard dependency of every flowchart
command. There is no `block-link-plus` plugin in this vault, so do not use the
old `#function name#function index` link form.

`server/` is proprietary (`LicenseRef-KG-Proprietary`) while the root workspace
is GPL-3.0-only. These docs may name and link server code; never paste server
implementation into a doc. `docs/POLICY/licensing.md` is the authority.

`docs/HUMAN/_GDD/` and `docs/HUMAN/TODOS/` are forward-looking; these docs
describe the code that exists today. Where a doc touches an unimplemented edge,
it says so explicitly.

---

## 5. Hard rules for every session

1. **The code is the only source of truth.** The `AGENTS.md` files are useful
   maps, but some of their references predate the current layout; verify every
   claim against the actual code before writing it. Where they disagree, code
   wins.
2. **Never read `flowcharts/Subflowcharts/` canvas contents wholesale.** Use the
   composed canvas for the doc's layer (see §19) plus at most a handful of
   targeted subflowchart canvases. *Listing* directory paths is fine — that is
   how the orchestrator seeds and curates `flows.json`. Link canvases by **file
   path**, never deep-link to a node.
3. **Document what the code does, not what it is planned to do.** Flag TODOs,
   stubs, unwired paths and known bugs explicitly in the doc's **Known gaps /
   stubs** section and inline where relevant.
4. **Don't restate the AGENTS.md files or the implementation plans.** Link to
   them; these docs are the narrative complement. `docs/AI/PLANS/` is the place
   for task-level detail.
5. **Audience**: an intermediate programmer who is new to Godot 4.7, Rust,
   godot-rust/gdext, SpacetimeDB 2.13 and the project's fixed-point hex world.
   Explain unfamiliar concepts (reducers, views, subscriptions, `include!`,
   GDExtension, node trees, scenes, `.tscn` files) briefly inline or with an
   analogy. Don't explain general programming ideas (loops, classes,
   dictionaries).
6. **Style**: dense causal prose — "X does Y *because* Z". One step = one
   cause-and-effect beat. Every mechanism claim should name the file/function
   that implements it. For table content specifically: a **changer** claim names
   the exact reducer/helper/method that writes; a **client reader** claim names
   the read surface (raw table accessor or view), the subscription set, the
   `kg_net` callback, the `GlobalEvent`/`RegionEvent`, the `ModelEvent` or
   `NetSession` state, the `WorldModel` field/revision and the concrete `Kg*`
   node or UI element it feeds. Never stop at `kg_net` or `WorldModel`.
7. **The client is Rust, not C#.** Scenes are `client/scenes/**/*.tscn`; classes
   are `Kg*` in `client/rust/kg_gdext`; networking is `kg_net::NetSession`.
   Never cite `.cs` files, `client/Scripts`, `client/Scenes`, `client/sstdbsdk`,
   `server/spacetimedb`, `TableSubscriber`, `TableBinderComponent`,
   `LobbyComponent`, `LobbyGui`, `DebugOverlay.cs`, `game.tscn` or any old C#
   class/scene name.
8. **Generated bindings are not schema owners.**
   `client/rust/kg_net/src/bindings/{global,region}`,
   `server/relay/src/bindings/*` and `server/audit/src/bindings/*` are generated
   from the server modules and committed with a `.generated` marker. Never edit
   them and never cite them as the definition of a table or view; link to the
   server's `tables.rs` / `views.rs` instead.
9. **No sync embeds while the per-system docs are being written.** During
   Phase 1 every doc carries its table prose inline. Sync-embed blocks are
   applied only in Phase 4, by the orchestrator, after the sweep and after the
   table map is resolved.
10. **One writer per shared artifact.** `00 Table Map.json` has one writer
    (orchestrator). Each `.tdd/table-map/<NN>-<slug>.json` has one writer (its
    doc subagent). The draft `00 Table Map.md` has one writer (merge subagent);
    the final one has one writer (orchestrator). `01 Roadmap.md`, `flows.json`
    and the pointer files have one writer (orchestrator).
11. **The orchestrator does not read git history.** Doc subagents may; the
    merge subagent may not. Never document removed code as current state.

---

## 6. Artifacts and working directory

Inside `docs/HUMAN/_TDD/`:

```
00 Table Map.json                 orchestrator, initial structured spine
00 Table Map.md                   merge draft -> orchestrator final
01 Roadmap.md                     orchestrator
02 ... 22 ... .md                 one per doc, written by its subagent
.tdd/                             orchestrator working directory (Obsidian ignores dotfolders)
  previous/00 Table Map.md        previous generation, moved aside in Phase 0 (if present)
  table-map/
    04-framework.json             per-agent version
    05-global-account.json        per-agent version
    ...
    _merge-report.md              merge subagent
  sweep-report.md                 orchestrator
  resolution-report.md            orchestrator
  final-report.md                 orchestrator
```

- `.tdd/` is a dotfolder: Obsidian ignores it, so the draft and the working
  files never appear in the vault's link graph.
- Keep `.tdd/` after the run; it is the audit trail for the generation.
- The per-agent file name is `<NN>-<slug>.json`, where `NN` is the zero-padded
  doc number and `<slug>` is the doc's system slug (`framework`,
  `global-account`, `region-movement`, ...).
- If a previous generation left a `00 Table Map.md`, move it to
  `.tdd/previous/00 Table Map.md` in Phase 0 and do not use it as an input. The
  initial JSON is the only table-map input for this run. Other docs are
  overwritten in place by their subagents.
- A doc subagent treats any existing doc at its path as absent. It writes from
  the code and the base map, never from the previous generation's prose.

---

## 7. Phase 0 — orchestrator setup

Do all of this before spawning any doc subagent.

1. **Refresh the pointer files.** Regenerate every root at the granularity in
   §13, then regenerate again and confirm a second run produces no further
   changes. A stale pointer set sends doc subagents to the wrong files.
2. **Refresh the flowcharts.** Run "Regenerate all flowcharts", seed/repair
   `flowcharts/flows.json` with the required flows (§19), then run "Regenerate
   all flowcharts" again and verify every `main*.canvas` appears and
   `lastResult.missingCanvases` is empty.
3. **Fix the doc set from the folder listing.** One doc per system folder under
   `server/modules/global/src/` and `server/modules/region/src/`, plus one for
   `server/shared/framework.rs`, plus the fixed 00–03 docs. The folder listing
   at Phase-0 time is the authority. Existing doc numbers stay; a folder added
   later gets the next free number. Support files (`lib.rs`, `aoi.rs`,
   `time.rs`) are not docs. Write the resulting lineup into `01 Roadmap.md`
   (§16).
4. **Write `00 Table Map.json` in full.** Read every `tables.rs`, `views.rs` and
   `framework.rs` shallowly, plus the writers and client read paths needed to
   fill each entry. Every module, every system folder, every table, every
   `^table-*` slug, in definition order — not just headers. Follow §17
   exactly. The map is clean: no CriticMarkup, no sync blocks.
5. **Validate the base map.** It parses as JSON; every table has a unique
   `slug`; every system has a `doc` that exists in the lineup; every table's
   `owner_doc` matches its system's `doc`; docs 02 and 03 appear as tableless
   docs in the roadmap and own no entries.
6. **Create `.tdd/table-map/`.** Write nothing else there yet.
7. **Do not spawn doc subagents until steps 1–6 are complete.** Every doc
   subagent depends on the base map being complete and on the pointers being
   fresh.

Phase 0 is also the moment to note the current table set in `01 Roadmap.md`:
all statuses start blank.

---

## 8. Phase 1 — parallel per-system doc subagents

Spawn one subagent per doc 02–22, in parallel (one message with multiple Task
calls, or the platform's equivalent). Use a general-purpose subagent with
file-write access. One agent owns exactly one doc; never split a doc across
agents and never let two agents edit the same file. Wait for **every** report
before starting Phase 2.

Docs 02 and 03 are included: they cover machinery rather than tables, but they
are still part of the parallel fan-out. They do not write a table-map version.

### 8.1 The spawn brief

Each brief must contain all of the following (the doc subagent cannot see the
orchestrator's conversation):

```
You are <author-name>, the sole author of
docs/HUMAN/_TDD/<NN> <Title>.md in the Khvor' Gates repo.

Read .claude/skills/tdd-docs/SKILL.md in full first. Then read:
- the base table map: docs/HUMAN/_TDD/00 Table Map.json (read-only)
- every pointers.md under the roots that cover your scope
- the code in your system folder / scope, and the client read path it feeds
You may read git history; never document removed code as current state.

Your assignment:
- Doc: <NN> <Title> (owner: <author-name>)
- System folder: <path> (or "machinery: <scope>" for 02/03)
- Owned table slugs: <slug>, <slug>, ... (none for 02/03)
- 00 section: <module> > <system> (or "none" for 02/03)
- Flowchart links: <layer canvas> plus 1-3 deep-dive subflowcharts
- Prerequisite docs: <02, 03, 04 as applicable>

Write exactly two files (one for 02/03: only the doc):
1. docs/HUMAN/_TDD/.tdd/table-map/<NN>-<slug>.json
   A full copy of the base JSON with CriticMarkup edits for YOUR tables only.
   Follow section 8.3 of the skill exactly. Include your author name and a UTC
   timestamp in every edit comment. Do not edit another doc's tables; use the
   top-level "proposals" array for those.
2. docs/HUMAN/_TDD/<NN> <Title>.md
   The complete doc, following the system doc schema in section 15. Write all
   table prose inline. Do NOT write sync-embed blocks. Delimit the block that
   will later be replaced by the sync embed with
   <!-- tdd-embed-start: ^table-slug --> ... <!-- tdd-embed-end: ^table-slug -->
   around the Changers and Readers subsections.

Do not touch 00 Table Map.json, 00 Table Map.md, 01 Roadmap.md,
flowcharts/flows.json, any pointers.md, or another doc. Do not run Obsidian
plugin commands. Do not resolve another agent's CriticMarkup. Treat any
existing doc at your path as absent; overwrite it from the code and the base
map, not from the previous generation.

Before finishing, run the per-doc verification checklist in section 21
(items 1-5) and the structure check. Report back with: the doc path, the
table-map path, the number of tables covered, every sweep note, and anything
you could not verify.
```

Use the author names `doc-02-client-framework`, `doc-03-subscriptions`,
`doc-04-framework`, `doc-05-global-account`, ... matching the doc number and
system. The name is stable and appears in every CriticMarkup comment.

### 8.2 What each doc subagent reads

- **Docs 04–22:** deep code read starting from the pointer files matching the
  doc's scope (read through all `pointers.md` under the roots in §13 and pick
  entry points by scope), plus the server `tables.rs` / `views.rs` and
  `framework.rs` definitions, the writers, and the full client read path to the
  concrete `Kg*` node or UI element.
- **Doc 02:** the scene tree and node classes (`Kg*`, `*Component*`), exported
  wiring/signals, `KgMain`, `KgWorldModel`, `kg_client_model::WorldModel`,
  revisions, render frame, renderers. No tables.
- **Doc 03:** the `kg_net` session/state machine, both connections,
  subscriptions, generated bindings, callbacks, `ModelEvent`, view flavors, and
  the relay/audit/editor read paths. No tables. Doc 03 is the prerequisite for
  every later doc: it explains raw tables vs views, subscription sets, the
  `RawEvent` → `GlobalEvent`/`RegionEvent` → `ModelEvent` → `WorldModel` path,
  and the four view flavors (`my_*`, `nearby_*`, auditor-only, relay-only)
  once, so the 04+ docs can say "read via the `nearby_enemies` view in the
  region subscription" without re-teaching the mechanism. `kg_bot` consumes
  the same `kg_net`/`kg_client_model` path, so the reader trace also explains
  the bot.
- **Doc 04:** `framework.rs` tables/views, `include!`, module hooks,
  `kg_intermod`, relay, inter-module messaging, per-file deep dive.
- **Docs 05+:** the system folder's `tables.rs`, `views.rs`, reducers/methods,
  and every file listed by that folder's `pointers.md`.

### 8.3 CriticMarkup protocol (per-agent table-map version)

The per-agent file is a **full copy** of `00 Table Map.json` with these
top-level changes:

```json
{
  "format": "kg-tdd-table-map-edit",
  "version": 1,
  "base_generated_at": "<copy of the base generated_at>",
  "author": "doc-16-region-movement",
  "doc": "16 Region Movement Tables.md",
  "edited_at": "2026-10-28T14:32:05Z",
  "modules": [ ... the base modules tree ... ],
  "proposals": []
}
```

Rules:

- Change only the fields of tables whose `owner_doc` equals your `doc`. Never
  change `name`, `slug`, `definition`, `owner_doc`, module/system `title`,
  `folder`, `doc`, or another agent's entries.
- Copy every untouched table byte-for-byte (same values).
- Wrap **only the changed span** of a text field in CriticMarkup. Leave the
  unchanged parts of the string exactly as they were.
- Append a comment to every edit with your author name, the UTC timestamp of
  the edit, and a short reason:

  ```
  {>>doc-16-region-movement | 2026-10-28T14:32:05Z | guard added after check_report<<}
  ```

  This is the required "name and timestamp" record. One comment per edit, not
  one per file.
- Use the standard CriticMarkup operators:
  - addition: `{++new text++}`
  - deletion: `{--old text--}`
  - substitution: `{~~old text~>new text~~}`
  - highlight / verify: `{==text to verify==}`
  - comment only (proposal, no text change): `{>>author | ts | note<<}`
- Never put CriticMarkup inside a wikilink target (`[[path#Symbol]]`). Wrap
  the prose around the link.
- Never put CriticMarkup in `name`, `slug`, `definition`, `owner_doc` or
  `access`. If you believe one of those is wrong, use a `proposals` entry.
- **New table:** append the object at the position you want it, with
  `"status": "added"`, `"insert_after": "<slug of the previous table>"`, and
  `"critic": {"author": "...", "ts": "...", "note": "..."}`. Leave its text
  fields plain; the merge agent wraps the whole rendered entry.
- **Deleted table:** keep the object, set `"status": "deleted"` and
  `"critic": {...}`. Leave its fields as they were; the merge agent wraps the
  whole rendered entry.
- **Proposal for another doc's table:** do not edit it. Append to the top-level
  `proposals` array:

  ```json
  {
    "target": "table-player-position",
    "author": "doc-19-region-player",
    "ts": "2026-10-28T14:32:05Z",
    "change": "{++suggested text++}",
    "note": "why"
  }
  ```

- The strings are Markdown. Escape JSON quotes and backslashes normally; do not
  escape `{`, `}`, `[` or `]`.
- Validate the file parses as JSON before finishing. A malformed version is
  dropped by the merge agent.

Timestamp format: UTC ISO-8601 with seconds, `YYYY-MM-DDTHH:MM:SSZ`. Bash:
`date -u +%Y-%m-%dT%H:%M:%SZ`. PowerShell:
`(Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")`.

### 8.4 No sync embeds in Phase 1

A doc subagent must not write a `sync` code block anywhere. It writes the full
table prose inline, then brackets the part that will become the embed:

````markdown
## PlayerPosition

### Shape

...

<!-- tdd-embed-start: ^table-player-position -->

### Changers

...

### Readers

***Client*** — ...

***Server*** — ...

<!-- tdd-embed-end: ^table-player-position -->
````

The placeholder pair is the only sync-related text a doc may contain in Phase 1.
The orchestrator replaces the whole bracketed range with one sync block in
Phase 4. Keep doc-specific causal detail that the 00 entry does not carry
*outside* the bracketed range (usually in **Shape** or just before the start
marker).

### 8.5 Doc subagent validation and report

Before finishing, each doc subagent runs the per-doc checklist in §21 (items
1–5), checks that its table order matches the base JSON, and writes a
`**Sweep note:**` at the end of its doc for anything the orchestrator must
handle (stale pointers, flow-coverage problems, a table that seems missing from
the base map, an unverifiable claim). Table-entry changes belong in the
agent's table-map version, not in a sweep note.

The report back to the orchestrator is short: doc path, table-map path, table
count, sweep notes, unresolved questions.

---

## 9. Phase 2 — table-map merge subagent

Spawn exactly one merge subagent after **every** Phase-1 doc agent has finished
and every 04+ doc has a table-map version. If a doc or version is missing, fix
it or re-spawn that doc's subagent before merging. The merge subagent's only job
is the merge.

### 9.1 Inputs and outputs

Inputs:

- `docs/HUMAN/_TDD/00 Table Map.json` (base)
- every `docs/HUMAN/_TDD/.tdd/table-map/*.json` edit version

Outputs:

- `docs/HUMAN/_TDD/00 Table Map.md` — the draft with every CriticMarkup edit
  present and unresolved
- `docs/HUMAN/_TDD/.tdd/table-map/_merge-report.md` — conflicts, rejected
  files, proposals, counts

The merge agent must not write anything else.

### 9.2 Merge algorithm

1. Read the base JSON and every edit file. Reject (and report) any file whose
   `format`, `version` or `base_generated_at` does not match the base, or whose
   JSON does not parse. Ignore (and report) any edit to a table whose
   `owner_doc` does not equal the edit file's `doc`.
2. Walk the base `modules` → `systems` → `tables` order. Render each table entry
   as Markdown using §18. The base render is the clean text.
3. For each table slug, apply the edit files that changed it:
   - If exactly one agent changed a field, replace that field's base text with
     the agent's string (which already contains the CriticMarkup ops).
   - If two or more agents changed the **same field**, do not pick one. Render
     a conflict block instead. Do not nest one agent's CriticMarkup inside
     another operator; render each variant on its own line:

     ```
     {==CONFLICT — 2 agents edited `changers`; the orchestrator must choose:==}{>>merge | 2026-10-28T18:00:00Z | same-field conflict<<}

     - **base** — <base text>
     - **doc-05-global-account** — <that agent's edited field, with its own CriticMarkup>
     - **doc-18-region-patch** — <that agent's edited field, with its own CriticMarkup>
     ```

     Record the conflict in the merge report. The orchestrator resolves it
     against the code in Phase 4.
   - Agents that changed different fields are merged field by field.
   - If one agent deletes a table that another agent edited, record a conflict
     and render the deletion plus the edited entry together. Do not drop either
     side.
4. Render `status: "added"` entries at their `insert_after` position (or at the
   end of the system's table list when absent), wrapped:
   `{++<rendered entry>++}{>>author | ts | note<<}`. If an added slug already
   exists in the base or in another addition, record a conflict and render both
   entries; do not merge them silently.
5. Render `status: "deleted"` entries in their base position, wrapped:
   `{--<rendered entry>--}{>>author | ts | note<<}`.
6. Render each top-level proposal as a highlighted block directly after its
   target entry. Do not apply the change and do not nest it inside a comment:

   ```
   {==PROPOSAL for ^table-player-position — doc-19-region-player | 2026-10-28T14:32:05Z==}
   <the proposal's `change` text, with its own CriticMarkup>
   {>>doc-19-region-player | 2026-10-28T14:32:05Z | <note><<}
   ```

   The orchestrator accepts or rejects the block in Phase 4. If a proposal's
   target slug does not exist, report it and render the proposal at the end of
   the draft.
7. Keep module and system headings, notes and order exactly as the base. Do not
   reorder, rename or drop a section.
8. Write `00 Table Map.md` with this banner as the first line:

   ```html
   <!-- TDD TABLE MAP DRAFT — unresolved CriticMarkup. The orchestrator resolves this in Phase 4. Do not distribute. -->
   ```

9. Write `_merge-report.md` with: files read, files rejected and why, tables
   changed per agent, every conflict, every proposal, and a count of
   additions/deletions.

### 9.3 Merge agent prohibitions

The merge agent:

- never resolves an edit, accepts a change, deletes a deletion or strips a
  comment;
- never edits a per-system doc;
- never writes `00 Table Map.json`;
- never edits `flows.json`, pointers or canvases;
- never runs an Obsidian plugin command;
- never reads git history.

If the draft cannot be produced without a judgment call, it records the
conflict in the report and leaves the CriticMarkup in place. That is the correct
outcome.

---

## 10. Phase 3 — orchestrator sweep

After the merge report exists, sweep **every** TDD doc before touching the
table map or applying sync embeds. Read each file end to end: the draft
`00 Table Map.md`, `01 Roadmap.md`, and docs 02–22.

Check, and repair directly where the fix is local:

- **Coverage.** Every table in the base JSON appears exactly once in the draft;
  every 04+ doc gives every table it owns its own `## <TableName>` section, in
  the same order as the base map. A missing or duplicated table is a blocker.
- **Structure.** Docs 02–03 follow their machinery scope; every 04+ doc has the
  schema sections in order (§15), including **Files — pointer deep dive**.
- **No premature sync.** No doc contains a `sync` code block. Every 04+ table
  section has exactly one matching
  `<!-- tdd-embed-start: ^table-slug -->` / `<!-- tdd-embed-end: ^table-slug -->`
  pair, and the slug exists in the draft.
- **No CriticMarkup in a doc.** CriticMarkup belongs only in the per-agent
  table-map versions and the draft `00 Table Map.md`. A doc containing `{++`,
  `{--`, `{~~`, `{==` or `{>>` is a defect.
- **Client readers reach the visible end.** Every ***Client*** reader traces the
  row to a concrete `Kg*` node, UI panel, spawned node or visual — not just a
  callback or `WorldModel`.
- **Links.** Spot-check every `[[file#symbol]]`; the symbol must exist in that
  code file. Scene paths must exist. Flowchart links must resolve. Never cite a
  generated binding or an old C# name.
- **Cross-doc consistency.** The same table is described the same way in its
  own doc and in the draft; cross-doc links point at docs that exist; the
  roadmap's doc set matches the files on disk.
- **Sweep notes.** Read every `**Sweep note:**`; either fix the issue or turn
  it into a concrete orchestrator action.
- **No stale names.** No `server/spacetimedb`, `client/Scripts`,
  `client/Scenes`, `.cs`, `TableSubscriber`, `game.tscn`, `BuildingTile`,
  `PlayerPositionDebug` or `control_bullets`.

Repairs:

- Fix doc-level prose, structure, links and placeholders directly.
- If a doc needs substantial rework, spawn a focused replacement subagent with
  the same Phase-1 brief (it may read git history; it must not touch the table
  map). Re-run the per-doc checklist on its result.
- Do **not** resolve any CriticMarkup edit and do **not** add a sync block in
  this phase. Those are Phase 4 only.

Write `.tdd/sweep-report.md` with what you checked, what you fixed, what you
re-delegated, and every remaining conflict or open question.

---

## 11. Phase 4 — resolve the table map and apply sync embeds

Only start when the sweep is complete and `sweep-report.md` exists.

### 11.1 Resolve `00 Table Map.md`

Work through the draft top to bottom:

- `{++text++}` — accept: keep `text`, drop the markers and the trailing
  comment.
- `{--text--}` — accept the deletion: remove `text`.
- `{~~old~>new~~}` — accept the substitution: keep `new`.
- `{==text==}` — verify the highlighted claim against the code, then keep the
  verified text. If it is wrong, correct it and record why.
- `{>>author | ts | note<<}` — read it. A proposal block
  (`{==PROPOSAL for ^target — …==}`) is either accepted (apply its suggested
  text as if it were an edit) or rejected (drop it), with the decision recorded.
- **Conflicts** — choose one variant or combine them; verify the chosen claim
  against the code. Never leave two variants in the final file. Record the
  decision and the reason in `.tdd/resolution-report.md`.
- **Added / deleted tables** — decide each one: an accepted addition becomes a
  normal `^table-*` entry at its chosen position; an accepted deletion removes
  the entry; a rejected change drops the CriticMarkup and leaves the base entry.
- Remove the draft banner and every CriticMarkup operator from the final file.

After resolution:

- The file has no `{++`, `{--`, `{~~`, `{==` or `{>>` left.
- Every table slug is unique.
- Every `^table-*` referenced anywhere in the docs exists in this file.
- The section order and table order still match the base map, apart from
  accepted additions/deletions.

Record every acceptance/rejection, every conflict decision and every correction
in `.tdd/resolution-report.md`, with the edit's author and timestamp.

### 11.2 Apply the sync-embed blocks

Now apply the sync embeds, doc by doc (04–22 only; 02 and 03 have no tables):

1. For each table section, find the
   `<!-- tdd-embed-start: ^table-slug -->` …
   `<!-- tdd-embed-end: ^table-slug -->` range.
2. Replace the entire range with exactly one sync block:

   ````
   ```sync
   ![[00 Table Map#^table-slug{seamless:true,title:false,marker:01.}]]
   ```
   ````

3. The embedded entry supplies the **Changers**, **Client readers** and
   **Server readers** groups. The final table section is therefore:
   `## <TableName>` → `### Shape` → the embedded entry. Delete the duplicated
   `### Changers` / `### Readers` prose together with the placeholders; keep
   any doc-specific causal detail that the entry does not carry, above the
   embed.
4. `marker:01.` — each table section embeds exactly one entry, so every marker
   in a doc is `01.`. If a section deliberately embeds a second entry, number
   the blocks `01.`, `02.`, ... in reading order.
5. Do not add a sync block to `00 Table Map.md` (it is the source), to
   `01 Roadmap.md`, or to docs 02 and 03.
6. Do not edit the embedded text in the doc; the embeds are live. If a
   correction is needed later, edit `00 Table Map.md` and let the embed update.

Verify: every sync target resolves, every table section has exactly one embed,
no placeholder remains, and no `sync` block appears outside the 04–22 table
sections.

---

## 12. Phase 5 — final verification

1. Re-run "Regenerate all flowcharts"; verify every `main*.canvas` is fresh and
   `lastResult.missingCanvases` is empty.
2. Regenerate pointers on all roots again; confirm no `pointers.md` change.
3. Fill the **Status** column in `01 Roadmap.md` for every doc (`done`).
4. Run the whole-set verification checklist (§21).
5. Write `.tdd/final-report.md`: doc counts, table counts, conflicts resolved,
   links checked, remaining known gaps, and the final verification results.
6. Report to the user: what was generated, the one-line status, and any
   unresolved items. Do not silently ship a draft.

---

## 13. Pointer files (generated — the entry points)

Per-doc file lists rot: every rename silently invalidates them, and new folders
appear as the game grows. So **no lineup row names individual code files or
pointer files to read**. The 04+ rows name the stable system folders; Phase 0
fixes the doc set from the folder listing at that time, and a folder added later
gets a new doc number rather than changing the existing docs. Instead, each code
folder carries a generated `pointers.md` — a wikilink index of every code file
(and, at symbol granularity, every symbol and the vault files that use it)
under it, grouped by relative directory. Doc subagents read through all the
pointer files under a root for the entry points matching their scope.

Which pointer files to use (all paths vault-relative; regenerate, never
hand-edit — discover by listing, never assume any folder list is complete):

| Root | Granularity | Covers |
| --- | --- | --- |
| `server/modules` | symbol | both module roots, their `src/` roots and every system folder |
| `server/shared` | symbol | `framework.rs` |
| `server/crates` | symbol | `kg_intermod` |
| `server/relay` | symbol | the relay sidecar |
| `server/audit` | symbol | the audit sidecar |
| `client/rust` | symbol | `kg_net`, `kg_client_model`, `kg_gdext`, `kg_editor` and their subfolders |
| `client/scenes` | file | the `.tscn` trees (scene → class consumers) |
| `crates` | symbol | reference-only: the pure `kg_*` crates |

Regenerate with the `vscode-editor` plugin: right-click the root folder →
**Generate pointers in subfolders ▸ Symbol granularity** (or headless
`plugin.flowchart.generatePointers(folder, undefined, "symbol")`). For
`client/scenes` use **Generate pointers in subfolders ▸ File granularity**. The
palette command **"Generate pointers in subfolders…"** defaults to file
granularity; use the context-menu submenu when you need symbol granularity. The
plugin's pointer depth is 16, and its ignore list already excludes `target/`,
`server/target/`, `build/`, `bindings/`, `spikes/` and `client/addons/` — the
generated binding trees are deliberately not indexed.

The current pointer set is mixed: all server roots are symbol-level; the
`client/rust/kg_gdext` subfolder pointers and the `crates` roots are file-level;
and there are no pointers yet under `kg_net`, `kg_client_model`, `kg_editor` or
`client/scenes`. Phase 0 regenerates every root at the granularity above; a
`pointers.md` from the other granularity is overwritten. Use the pointer
header's `[[file#` symbol suggest for entry points. Symbol-level entry points
are included: they are the fastest way to jump to a function such as
`report_movement` or `apply_player_hit`.

Doc subagents never run the pointer command. If a pointer file is stale, they
leave a `**Sweep note:** regenerate pointers` at the end of the doc; the
orchestrator regenerates in Phase 0 and Phase 5.

---

## 14. Doc numbering and lineup

| Doc | Writes | 00 section | flows.json flow | Scope (match against all `pointers.md` — see §13) |
| --- | --- | --- | --- | --- |
| 00 | `00 Table Map.json` **in full**; final `00 Table Map.md`; seed/repair `flowcharts/flows.json` | — | `main` | All pointers, shallow pass; every `tables.rs` / `views.rs` + `framework.rs`; every writer and client read path |
| 01 | `01 Roadmap.md` | — | — | What these docs are, conventions, reading order, status table |
| 02 | `02 The Client Component Framework.md` | — | `client` | Scene tree and node classes (`Kg*`, `*Component*`), exported wiring/signals, `KgMain`, `KgWorldModel`, `kg_client_model::WorldModel`, revisions, render frame, renderers; no tables |
| 03 | `03 Connection, Subscriptions & Views.md` | — | `subscriptions` | `kg_net` session/state machine, both connections, subscriptions, generated bindings, callbacks, `ModelEvent`, view flavors, relay/audit/editor read paths; no tables |
| 04 | `04 Shared Framework Tables.md` | Shared framework | `framework`, `messaging` | `framework.rs` tables/views, `include!`, module hooks, `kg_intermod`, relay, inter-module messaging; per-file deep dive |
| 05 | `05 Global Account Tables.md` | kg-global › account | `global` | `server/modules/global/src/account/` — tables, views, changers, readers, per-file deep dive |
| 06 | `06 Global Patch Tables.md` | kg-global › patch | `global` | `server/modules/global/src/patch/` — tables, views, changers, readers, per-file deep dive |
| 07 | `07 Global Registry Tables.md` | kg-global › registry | `global` | `server/modules/global/src/registry/` — tables, views, changers, readers, per-file deep dive |
| 08 | `08 Global World Tables.md` | kg-global › world | `global` | `server/modules/global/src/world/` — tables, views, changers, readers, per-file deep dive |
| 09 | `09 Region Ability Tables.md` | kg-region › ability | `region` | `server/modules/region/src/ability/` — tables, views, changers, readers, per-file deep dive |
| 10 | `10 Region Audit Tables.md` | kg-region › audit | `region` | `server/modules/region/src/audit/` — tables, views, changers, readers, per-file deep dive |
| 11 | `11 Region Clock Tables.md` | kg-region › clock | `region` | `server/modules/region/src/clock/` — tables, views, changers, readers, per-file deep dive |
| 12 | `12 Region Combat Tables.md` | kg-region › combat | `region` | `server/modules/region/src/combat/` — tables, views, changers, readers, per-file deep dive |
| 13 | `13 Region Config Tables.md` | kg-region › config | `region` | `server/modules/region/src/config/` — tables, views, changers, readers, per-file deep dive |
| 14 | `14 Region Enemy Tables.md` | kg-region › enemy | `region` | `server/modules/region/src/enemy/` — tables, views, changers, readers, per-file deep dive |
| 15 | `15 Region Health Tables.md` | kg-region › health | `region` | `server/modules/region/src/health/` — tables, views, changers, readers, per-file deep dive |
| 16 | `16 Region Movement Tables.md` | kg-region › movement | `region` | `server/modules/region/src/movement/` — tables, views, changers, readers, per-file deep dive |
| 17 | `17 Region Pass Tables.md` | kg-region › pass | `region` | `server/modules/region/src/pass/` — tables, views, changers, readers, per-file deep dive |
| 18 | `18 Region Patch Tables.md` | kg-region › patch | `region` | `server/modules/region/src/patch/` — tables, views, changers, readers, per-file deep dive |
| 19 | `19 Region Player Tables.md` | kg-region › player | `region` | `server/modules/region/src/player/` — tables, views, changers, readers, per-file deep dive |
| 20 | `20 Region Stats Tables.md` | kg-region › stats | `region` | `server/modules/region/src/stats/` — tables, views, changers, readers, per-file deep dive |
| 21 | `21 Region Weapon Tables.md` | kg-region › weapon | `region` | `server/modules/region/src/weapon/` — tables, views, changers, readers, per-file deep dive |
| 22 | `22 Region World Tables.md` | kg-region › world | `region` | `server/modules/region/src/world/` — tables, views, changers, readers, per-file deep dive |

This table is the 2026-10-28 snapshot. Phase 0 re-derives the set from the
folder listing and keeps existing numbers; it may append rows, never renumber
them. Global systems are numbered before region systems, alphabetically within a
module.

The `flows.json` column names the composed canvas the doc links, **not** a
per-doc canvas: `main-global.canvas` covers docs 05–08, `main-region.canvas`
covers docs 09–22, `main-framework.canvas` and `main-messaging.canvas` cover doc
04, `main-client.canvas` covers doc 02 and `main-subscriptions.canvas` covers
doc 03. One composed canvas per layer is deliberate; a composed canvas per
system would duplicate the module canvas and go stale independently.

Docs 02 and 03 have no table entries because they cover machinery, not tables —
the client component/model framework (02) and the read path every table shares
(03). They omit **Tables** (system doc schema, item 5), replacing it with a main body organized
one section per piece of machinery. They keep **Find it fast** — it indexes
their machinery sections instead of table sections.

---

## 15. System doc schema (docs 02–22)

Every system doc is organized **around how its tables are viewed and changed** —
a reader asking "how does X manifest in the client?" or "what writes X?" should
be able to land on the exact table section that answers it without skimming
prose. The Tables body (item 5 below), including its final sync embeds, is the doc;
everything else exists only to index or illustrate those view/change paths.
Every system doc has exactly these sections, in order:

1. **Assumed knowledge** — wikilinks to the prerequisite docs (02 and 03 for
   every 04+ doc; 04 before the module docs that depend on the framework).
2. **The 30-second version** — a short paragraph: which tables this system owns
   and the one-sentence shape of their write/read paths, no detail.
3. **Find it fast** — the doc's navigation index: a lookup table mapping the
   questions a reader actually asks ("how does an enemy appear on the client?",
   "where is damage computed?", "what happens when I die?") to the place that
   answers each. Columns: *Question / concept* → *Table or view* → *Where* — an
   anchor link into this doc's table section, or a link to another doc / a
   `^table-*` entry in 00 when the answer lives elsewhere. Cover every table the
   doc owns plus the handful of cross-doc questions this system most often
   raises. Keep questions phrased the way a newcomer would ask them, not in
   code identifiers.
4. **Flowcharts** — wikilinks to the generated `.canvas` flowcharts for this
   layer: the module canvas (`flowcharts/main-global.canvas` or
   `flowcharts/main-region.canvas`; `main-framework.canvas`/`main-messaging.canvas`
   for 04; `main-client.canvas` for 02; `main-subscriptions.canvas` for 03) plus
   1–3 deep-dive subflowchart canvases from the doc's own folder under
   `flowcharts/Subflowcharts/`, all linked by path (never by node id).
5. **Tables** — the main body. **One `## <TableName>` section per table the doc
   owns**, in the order the tables appear in 00 — no free-floating prose blobs.
   During Phase 1 each table section has exactly these subsections, in order:
   - **Shape** — what a row *means* and why the table is shaped this way (key
     columns, constraints, relations to other tables). Only as much as the next
     two subsections need; this is not a schema dump.
   - **Changers** — every path that inserts, updates or deletes rows: the
     reducer, scheduled tick, lifecycle hook, inter-module handler, seed or
     admin tool, told as dense causal prose with its guards, write invariants
     and formulas, each claim naming the file/function. If an operation never
     happens (append-only, never deleted), say so — that is information.
   - **Readers** — how rows are consumed, split into ***Client*** and
     ***Server***:
     - ***Client*** — the full read path *to its visible end*: raw-table
       accessor or view → subscription set (`subscribe_global`/`subscribe_region`)
       → `kg_net` callback (`register_global_callbacks`/`register_region_callbacks`)
       → `GlobalEvent`/`RegionEvent` → `ModelEvent` (or `NetSession` state) →
       `WorldModel` field/revision → **the concrete thing on screen or in the
       scene it ends up driving** (which `Kg*` component, which UI panel, which
       spawned node, which visual). This subsection is where "how does X
       manifest in the client?" gets answered — never stop at the callback or
       the model.
     - ***Server*** — guards every reducer runs, AOI semijoins, full scans worth
       warning about, sidecar reads (`kg-relay`, `kg-audit`) and editor/CLI
       reads. Omit when unremarkable.

   Do **not** use sync embeds in Phase 1. Write the prose inline and bracket the
   Changers + Readers block with the `<!-- tdd-embed-start: ^table-slug -->` /
   `<!-- tdd-embed-end: ^table-slug -->` pair (§8.4). In Phase 4 the
   orchestrator replaces that range with one sync block; the final table section
   is `## <TableName>` → `### Shape` → the embedded entry, whose bullet groups
   are the Changers/Readers subsections. Keep **Shape**, and keep only the
   causal detail the entry does not carry.
6. **Files — pointer deep dive** (04+ module docs only) — one `### <file>`
   subsection per file (or tight file group) in the system folder, in the order
   that folder's `pointers.md` lists them. The pointers say *what* each file
   uses; this section says *how*: call sites with file/function names, call
   direction, guards/preconditions and what data is passed. Keep each
   subsection to the file's role in viewing/changing this module's tables —
   this is not a line-by-line walkthrough.
7. **Cross-table flows** (optional) — the few mechanisms that genuinely span
   several tables and can't be told under any one of them (e.g. the damage
   pipeline, joining the world, the patch swap, inter-module messaging). Keep
   short; link into the per-table sections rather than re-telling them. Omit
   when every mechanism fits under a single table.
8. **Known gaps / stubs** — explicit list of TODOs, stubs, unwired UI and known
   bugs in this system's code (§22 — each gap names its target doc). If none,
   omit the section.
9. **Where to go next** — 1–3 sentences pointing to the next doc(s) to read.

---

## 16. `01 Roadmap.md` (orchestrator, Phase 0)

Contains: what these docs are and who they are for; the conventions section
(copy the wikilink and canvas conventions from §20 into it, adapted as
observer-facing guidance); the reading-order/status
table (doc #, title, status, one-line coverage summary — mirror §14, statuses
all start blank and are filled by the orchestrator in Phase 5); a link to the
global `flowcharts/main.canvas` as the visual overview (**this link stays
unresolved until Phase 0 seeds `flows.json` and composes — that's expected,
don't "fix" it by hand-creating a canvas**); and the note about forward-looking
systems staying out. Status values: blank / `done`. Reading order:
02 → 03 → 04 → 05–08 → 09–22; the roadmap may also offer a server-first path
(04 → module docs). Link `docs/HUMAN/NETWORKING/Player Position Sync.md` as the
long-form movement companion, not as a substitute for the table story.

Mention in the conventions section that during generation the table map is
`00 Table Map.json` (machine-readable) and that the published doc is
`00 Table Map.md` (resolved Markdown with the `^table-*` anchors).

---

## 17. `00 Table Map.json` schema

The initial table map is a JSON document so each doc subagent can copy it and
edit its own version with CriticMarkup. The orchestrator writes it in full in
Phase 0.

Top-level shape:

```json
{
  "format": "kg-tdd-table-map",
  "version": 1,
  "generated_at": "2026-10-28T00:00:00Z",
  "generated_by": "orchestrator",
  "modules": []
}
```

A module:

```json
{
  "title": "kg-region",
  "note": null,
  "access_suffix": null,
  "systems": []
}
```

- `title` — the `#` heading text. `Shared framework`, `kg-global`, `kg-region`,
  in that order.
- `note` — an optional paragraph rendered under the heading. The Shared
  framework module states that its tables/views are included in both
  `kg-global` and every region database.
- `access_suffix` — optional text appended to every entry's access marker in
  this module (e.g. `global and every region database`).

A system:

```json
{
  "title": "movement",
  "folder": "server/modules/region/src/movement/",
  "doc": "16 Region Movement Tables.md",
  "tables": []
}
```

- `title` — the `##` heading. `null` for the Shared framework system, whose
  tables render directly under the module heading.
- `folder` — the code folder or file this system owns.
- `doc` — the exact filename of the doc that owns these tables. Docs 04+ only.

A table entry:

```json
{
  "name": "PlayerPosition",
  "slug": "table-player-position",
  "owner_doc": "16 Region Movement Tables.md",
  "access": "public",
  "definition": "server/modules/region/src/movement/tables.rs#PlayerPosition",
  "changers": "[[server/modules/region/src/movement/reducers.rs#report_movement|report_movement]] updates the row after `kg_rules::movement::check_report`; ...",
  "client_readers": "the `my_position` view [...] → `subscribe_region` → ... → `KgLocalMovementComponent`.",
  "server_readers": "`nearby_players` reads it directly; ..."
}
```

- `name` — the exact table struct name.
- `slug` — `table-` + the kebab-case table name (`table-player-position`).
  The renderer emits `^<slug>`. Unique across the repo; prefix with the module
  name only if two modules ever define the same table name.
- `owner_doc` — the exact filename of the doc that owns this table; must equal
  the system's `doc`. A doc subagent may edit only entries whose `owner_doc`
  equals its own doc. This is metadata; the renderer does not print it.
- `access` — `public`, `private` or `scheduled`. A `public` table is
  subscribable; it does **not** mean the client reads it. Scheduled-job tables
  get their own entry; their only "changer" is whoever arms/disarms the job row.
- `definition` — `path#Symbol`; the renderer emits a wikilink with the display
  text `definition`. Link the server's `tables.rs`/`views.rs`, never a
  generated binding.
- `changers` — required. Every insert/update/delete site, each with a code
  wikilink, grouped by operation. If an operation never happens, say so
  explicitly ("never updated", "append-only", "never deleted").
- `client_readers` — required. The exact path to the client *and to its visible
  end*: raw-table accessor or view (view flavor: `my_*`, `nearby_*`,
  auditor-only, relay-only) → subscription set (`subscribe_global` /
  `subscribe_region` in `client/rust/kg_net/src/lib.rs`) → `kg_net` callback
  (`register_global_callbacks` / `register_region_callbacks`) →
  `GlobalEvent`/`RegionEvent` → `ModelEvent` or `NetSession` state →
  `WorldModel` field/revision → the concrete `Kg*` node or UI element the row
  ends up driving. If the client never sees the table (private, public but
  unsubscribed, or subscribed but unmapped), say so — that is a first-class
  outcome, not an omission.
- `server_readers` — optional. Only where the read pattern matters (guards
  every reducer runs, AOI semijoins, full scans worth warning about,
  `kg-relay`/`kg-audit` subscriptions, the editor's HTTP SQL path). Use `null`
  when unremarkable.

Entries cross-refer by anchor name ("the patch row in ^table-player-patch"),
never by table name alone when an anchor exists. Where an entry's details belong
to another doc, say so and link it.

Do not add fields the renderer does not understand. The base file is clean: no
CriticMarkup and no sync blocks.

---

## 18. `00 Table Map.md` rendering conventions

The renderer (merge subagent, or the orchestrator when producing the final file)
turns the JSON into Markdown in this exact order:

- A module title becomes `# <title>`; its `note`, if present, becomes the
  paragraph directly below.
- A system `title` becomes `## <title>`; `null` emits no heading.
- Each table entry becomes a top-level list item:

  ```markdown
  - **PlayerPosition** — [[server/modules/region/src/movement/tables.rs#PlayerPosition|definition]] (public)
    - **Changers** — ...
    - **Client readers** — ...
    - **Server readers** — ...
  ^table-player-position
  ```

- Omit the **Server readers** bullet when the field is `null`/empty. **Changers**
  and **Client readers** are always present.
- If the module has an `access_suffix`, append it inside the parentheses:
  `(public; global and every region database)`.
- The `^table-*` anchor is the last line of the entry, unindented.
- Preserve the base order. The only allowed insertions are `insert_after`
  additions, and the only allowed removals are `status: "deleted"` entries.

The draft produced by the merge agent carries the same structure plus the
CriticMarkup operators and the draft banner. The final file has no banner and no
CriticMarkup.

Do not add a table of contents, frontmatter or sync blocks to
`00 Table Map.md`.

---

## 19. Flowchart integration (the `vscode-editor` fork's canvas flowcharts)

The plugin's flowchart feature (right-click a folder in Obsidian's file
explorer, the palette commands, or drive it headlessly via
`app.plugins.plugins['vscode-editor'].flowchart.*`) generates a deterministic
tree of `.canvas` files under `flowcharts/Subflowcharts/` at three granularity
levels (`_symbol`, `_codefile`, `_subfolder`), mirroring the `client/`,
`server/`, `crates/` and `tools/` folders, with the root aggregate at
`flowcharts/Subflowcharts/root_subfolder.canvas`. **Named main flowcharts** are
defined in **`flowcharts/flows.json`** — a manifest mapping flow names to lists
of subflowchart canvas paths:

```json
{ "flows": { "main": ["flowcharts/Subflowcharts/root_subfolder.canvas"], "region": ["…", "…"] } }
```

**Current state (2026-10-28):** `flows.json` exists with `main`, `messaging`,
`subscriptions`, `framework`, `global` and `region`; the `Subflowcharts/` tree
has **no `.canvas` files** (only a `tools_subfolder/` directory), so every
manifest member is dangling until Phase 0 regenerates. Phase 0 must add a
`client` flow (for doc 02) and verify the six existing flows after the
regenerate; do not assume the current members survive a changed tree.

**Exact palette command names**: "Regenerate all flowcharts" (full pipeline:
code model → subflowcharts → tag-driven aggregates → compose every flow in
flows.json, strictly serialized per discovered tree), "Generate flowcharts from
code…" (subflowcharts only), "Generate aggregate flowchart from subflowchart
metadata…" (aggregates + compose all flows; no code parse needed). Composition
output: the `"main"` flow → `flowcharts/main.canvas`, each other flow `<name>` →
`flowcharts/main-<name>.canvas` (flat in `flowcharts/`, no subfolder). One
invocation composes **all** flows at once. Membership lives in the manifest, not
in canvas frontmatter, so it survives code regeneration untouched. Key
mechanics:

- **Hard dependency: the Advanced Canvas plugin must be enabled** — every
  flowchart command aborts with a Notice otherwise.
- **Stable across regeneration**: canvas file **paths**, node ids (deterministic
  content hashes), and manually adjusted node positions (carried over on
  overwrite by matching id — but a canvas *renamed* by the collision ladder
  breaks the id match and loses its positions). Manually added text nodes in
  aggregate canvases are also preserved, and composed `main*.canvas` files keep
  any existing frontmatter.
- **Stale pruning is automatic**: a generation run trashes (**vault trash**
  `.trash/`, recoverable — not system trash) any previously generated,
  granularity-tagged `Subflowcharts/` canvas it didn't rewrite — controlled by
  the "Prune stale flowcharts" setting (default on). Canvases outside
  `Subflowcharts/` (mains, `flows.json`) are never pruned — and a **stale
  `main-<name>.canvas` is never cleaned up** if its flow is renamed or emptied;
  delete it manually.
- **Headless API** (`plugin.flowchart.*`, all return structured JSON and stash
  it on `plugin.flowchart.lastResult`): `generate(folder, extensions)`
  (subflowcharts only), `regenerate(scope)` (aggregates + compose),
  `regenerateAll()` (everything, every tree), `setFlowMembership(files,
  flowName, include)`, `setAggregateIncluded(Many)`,
  `generatePointers(folder, extensions?, granularity)` (pointer indexes — see
  §13).
- **Commands are queued in-process** — the plugin wraps every flowchart and
  pointer command in a `runExclusive` queue, so two commands in the same window
  run one at a time. Do not rely on that across agents or windows: only the
  orchestrator runs generation, and a long eval call can return before you
  observe completion — poll `lastResult` (or stash a global) rather than
  assuming completion.
- **`regenerateAll` regenerates with all configured extensions** (it ignores
  per-folder extension selection) and discovers only folders literally named
  `flowcharts` that have a `Subflowcharts/` child or a `flows.json` — the
  vault-root `flowcharts/` qualifies.
- **`flows.json` is hand-editable** — compose re-reads it fresh every run and
  reads are tolerant (missing/malformed → treated as empty). But API edits
  (`setFlowMembership`, the context menus) rewrite the whole file and **drop
  empty flows**, so every pre-defined flow needs at least one real member
  canvas. Keep members within their own tree — cross-tree members render as
  isolated nodes.
- **Dangling entries**: a renamed or pruned canvas leaves a dangling manifest
  entry — skipped at compose time and reported as `missingCanvases` in the
  result (a flow whose members are *all* missing produces no canvas at all).
  The manifest is deliberately not auto-edited; keep entries in sync when
  canvases move. Compose also reports `orphanedFlowCanvases` for
  `main-*.canvas` files whose flow no longer exists; they are **not** deleted,
  so remove them yourself.
- **Two curation layers — don't confuse them**: the canvas tag
  `includeInAggregateFlowchart` gates which nodes appear *inside*
  codefile/subfolder aggregate canvases (auto-added on create, survives
  regeneration); `flows.json` membership gates the `main*.canvas` compositions.
  These docs only curate the second.

### 19.1 Per-phase flowchart steps

**Phase 0 (orchestrator only):**

1. **Regenerate everything** with the palette command "Regenerate all
   flowcharts" (or `plugin.flowchart.regenerateAll()` via eval — strictly
   serialized; poll `lastResult`). This regenerates all subflowcharts, prunes
   stale ones, rebuilds aggregates and recomposes every flow in `flows.json`.
2. **Seed/repair `flows.json`:** after the regenerate, list the canvas paths
   under `flowcharts/Subflowcharts/` (listing paths is fine; rule 2 only bans
   reading canvas contents wholesale) and write `flowcharts/flows.json` by hand
   with the required flow set: `main` (root), `client` (doc 02),
   `subscriptions` (doc 03), `framework` and `messaging` (doc 04), `global`
   (docs 05–08) and `region` (docs 09–22), each seeded with the subflowchart
   canvases that obviously cover that layer — **at least one member per flow**
   (empty flows are dropped on the next API write). Add the `client` flow; the
   other six already exist. Then re-run "Regenerate all flowcharts" and verify
   every `main*.canvas` appears and `lastResult.missingCanvases` is empty.

**Phase 1 (doc subagents only):** each doc links the canvases in its
**Flowcharts** section (system doc schema, item 4): the layer canvas plus 1–3
deep-dive subflowcharts (`01 Roadmap.md` links the global
`flowcharts/main.canvas`). They **never** run plugin commands or edit
`flows.json`.

**Phase 5 (orchestrator only):**

1. Curate each doc's flow membership (batched; only where coverage changed or
   a doc subagent left a sweep note): add/remove member canvases via right-click
   → "Include in flow ▸ …" / "Exclude from flow ▸ …" (or `setFlowMembership`
   via eval).
2. Re-run "Regenerate all flowcharts" and verify every `main*.canvas` appears
   fresh and `lastResult.missingCanvases` is empty.

---

## 20. Mechanical conventions (exact syntax — follow precisely)

These conventions also bind hand-written docs added under `docs/HUMAN/_TDD/`
outside the numbered set; `docs/HUMAN/_TDD/AGENTS.md` is their standalone style
guide.

**Wikilinks to code** use the `vscode-editor` symbol subpath:

```
[[server/modules/region/src/movement/tables.rs#PlayerPosition|PlayerPosition]]
[[server/modules/region/src/movement/reducers.rs#report_movement|report_movement]]
[[client/rust/kg_gdext/src/player/position_sync_component.rs#KgPositionSyncComponent|KgPositionSyncComponent]]
```

- The symbol name after `#` is the exact struct/function/class name; it must
  exist in that file. The symbol-level pointer files contain ready-made links of
  this shape.
- Scenes are ordinary path links: `[[client/scenes/main.tscn]]`,
  `[[client/scenes/player/local_player.tscn]]`.
- Table-map anchors are block links: `[[00 Table Map#^table-player-position|PlayerPosition]]`.
- Do **not** use the old `[[filename#function name#function index|display text]]`
  form: this vault does not have `block-link-plus`, and the old index is not a
  current symbol path.
- Every table entry and every flowchart entry links to code wherever possible.
- **Inside a table cell, escape the alias pipe as `\|`.** An unescaped `|` ends
  the cell, so a link with display text in a table is written
  `| [[wikilink\|alias]] |`, e.g.
  `| [[server/modules/region/src/movement/tables.rs#PlayerPosition\|PlayerPosition]] |`.
  The backslash is Markdown source only; the rendered link is unchanged. This
  applies to every table in these docs (the **Find it fast** index, the roadmap
  status table, any ad-hoc table). A link without an alias needs no escape.

**Wikilinks to canvas flowcharts** are ordinary wikilinks including the folder
path.

**CriticMarkup** (per-agent table-map versions only; never in a doc, never in
the base JSON, never in the final `00 Table Map.md`):

| Purpose | Syntax | Required comment |
| --- | --- | --- |
| Add text | `{++new++}` | `{>>author \| ts \| why<<}` |
| Delete text | `{--old--}` | `{>>author \| ts \| why<<}` |
| Replace text | `{~~old~>new~~}` | `{>>author \| ts \| why<<}` |
| Verify text | `{==text==}` | `{>>author \| ts \| what to verify<<}` |
| Comment / proposal | `{>>author \| ts \| note<<}` | — |

Every edit carries its author name and UTC timestamp in exactly that comment
shape. Timestamps are `YYYY-MM-DDTHH:MM:SSZ`.

**Sync-embed blocks** (Phase 4 only, 04–22 docs only) — transclude the single
`^table-*` entry instead of re-typing its Changers/Readers prose:

````
```sync
![[00 Table Map#^table-player-position{seamless:true,title:false,marker:01.}]]
```
````

- `marker:NN.` — the entry number *as it should render in the embedding doc*,
  zero-padded to two digits. Each table section embeds exactly one entry, so
  its marker is `01.`; a section with extra embeds numbers them `01.`, `02.`,
  ... in reading order.
- One sync block may hold one or more embeds; keep each embed to a single
  `^table-*` entry.
- No sync block in `00 Table Map.md`, `01 Roadmap.md`, docs 02 or 03.

**Section references between docs** use Obsidian wikilinks, or with display
text.

---

## 21. Verification checklist

**Per-doc (each Phase-1 doc subagent, re-checked by the orchestrator during the
sweep):**

1. The doc follows the required section order (§15), with **Files — pointer deep
   dive** for every 04+ module doc.
2. Every table owned by the doc has its own `## <TableName>` section, in the
   same order as the base JSON, with **Shape** / **Changers** / **Readers**
   during Phase 1 and the `tdd-embed-start`/`tdd-embed-end` pair around the
   block that becomes the embed.
3. Spot-check every `[[file#symbol]]` code link: the symbol exists in that code
   file and the link resolves. Use the pointer files and the symbol suggest
   rather than guessing.
4. Every table, view, reducer, function and `Kg*` class named in the doc exists
   in the code today — grep for it. Link to the server's `tables.rs`/`views.rs`,
   never to generated bindings. Never cite `.cs` files, `client/Scripts`,
   `client/Scenes`, `server/spacetimedb` or the old
   `TableSubscriber`/`TableBinderComponent` pipeline — the client is
   `client/rust` + `client/scenes`.
5. **Cite the live wiring site.** The scene roots and inline component wiring
   are `client/scenes/main.tscn` (`Main/World/...`, `Main/Camera/...`,
   `Main/GUI/...`), `client/scenes/player/local_player.tscn`,
   `client/scenes/player/remote_player.tscn`, `client/scenes/enemy/enemy.tscn`
   and the UI scenes under `client/scenes/ui/`. The only standalone component
   scenes are `client/scenes/components/nameplate_component_2d.tscn` and
   `client/scenes/components/unit_sprites_component_2d.tscn`; every other
   component is declared inline. Cite the live scene and node path, never a
   component scene that does not exist. Every ***Client*** reader trace reaches
   a concrete on-screen or in-scene manifestation (not just a callback or the
   model), and the **Find it fast** index covers every owned table and resolves
   to real headers/anchors.

**Whole set (orchestrator, Phase 3–5):**

6. The final `00 Table Map.md` contains every base table exactly once, every
   accepted addition and no accepted deletion, with unique `^table-*` anchors;
   every `^table-*` referenced by any doc exists.
7. The final `00 Table Map.md` has no CriticMarkup left (`{++`, `{--`, `{~~`,
   `{==`, `{>>`), no draft banner, and the resolved section/table order matches
   the base map plus accepted changes.
8. Before Phase 4, no doc has a `sync` block. After Phase 4, every 04–22 table
   section has exactly one sync block, every target resolves, and no
   `tdd-embed-start`/`tdd-embed-end` placeholder remains.
9. Each doc's **Flowcharts** section links resolve; every linked canvas was
   regenerated in the final compose; each required flow exists in `flows.json`
   with at least one member; and the last compose result's `missingCanvases` was
   empty.
10. `01 Roadmap.md` status table updated for every doc.
11. Pointer files fresh: re-running **Generate pointers in subfolders** on all
    roots (`server/modules`, `server/shared`, `server/crates`, `server/relay`,
    `server/audit`, `client/rust`, `client/scenes`, `crates`) leaves no
    `pointers.md` in `git status --short` and no diff in
    `git diff -- '**/pointers.md'`.
12. Every `.tdd/table-map/*.json` parses; every edit version's
    `base_generated_at` matches the base; the merge report accounts for every
    file and every conflict; `.tdd/resolution-report.md` accounts for every
    conflict resolution.
13. No stale paths or schema names: no `server/spacetimedb`, no
    `client/Scripts`/`client/Scenes`, no `.cs`, no `TableSubscriber`, no
    `game.tscn`, no `BuildingTile`, no `PlayerPositionDebug`, no
    `control_bullets`. If a claim needs one of these, it is describing the old
    game, not this repo.

---

## 22. Known gaps to document

Verified against the code at the time this plan was written (2026-10-28). Each
gap names the doc that owns its **Known gaps / stubs** section; re-verify before
writing it. The doc numbers below are the 2026-10-28 snapshot; if Phase 0
appends or renumbers docs, map each gap by title.

**Server:**

- **Patch swaps have no undo.** `admin_swap_patches` is a two-phase saga; a
  failed step sets `patch_transfer.state = Failed` and leaves the ledger rows
  `InTransit`; repair is manual (`admin_resend_patch_arrive`), and a failed
  departure has no resend in the current implementation (`server/AGENTS.md`,
  `server/modules/global/src/patch/methods.rs`). → **06 Global Patch Tables**
  and **18 Region Patch Tables**.
- **Rerouting leaves an offline orphan.** `admin_reroute_account` moves the
  account and queues `RoutePlayer`, but there is no `PlayerLeave`; the old
  region keeps the player's rows (`player`, position, health, weapon, stats and
  loadout) as an offline orphan
  (`server/modules/global/src/account/reducers.rs`). The views hide them through
  `online_player`, so they are invisible but not deleted. → **05 Global Account
  Tables** and **19 Region Player Tables**; the stale health, weapon, stat and
  loadout rows are covered by **15 Region Health Tables**, **21 Region Weapon
  Tables**, **20 Region Stats Tables** and **09 Region Ability Tables**.
- **A patch swap does not carry health, weapons, stats or loadouts.**
  `PlayerBundle` carries identity, entity id, name, patch-local offset and
  facing only; the destination recreates health at full HP on arrival, and the
  next `enter_world` gives the Bow and creates fresh stat/loadout rows. A
  cross-region move therefore resets those rows. The source region also keeps
  `player_stat_allocation` and `player_loadout` (they are keyed by the global
  player id and `delete_player_rows` deliberately leaves them for an in-region
  swap), so a cross-region move leaves them stale there. → **18 Region Patch
  Tables** plus **15 Region Health Tables**, **21 Region Weapon Tables**,
  **20 Region Stats Tables**, **09 Region Ability Tables** and **19 Region
  Player Tables**.
- **The audit log has no removal reducer.** `admin_add_auditor` is the only
  audit reducer; revoking means rotating the identity. The replay audits
  player-taken hits; player→enemy hits are out of scope
  (`server/audit/AGENTS.md`, `server/modules/region/src/audit/reducers.rs`).
  → **10 Region Audit Tables**.

**Client / read path:**

- **`world_config` is subscribed but not consumed.** `NetSession::subscribe_global`
  adds `world_config`, but `register_global_callbacks` registers no callback and
  no `ModelEvent` carries it; the client uses `region_config` for the torus/speed
  today. → **08 Global World Tables** and **03 Connection, Subscriptions &
  Views**.
- **A build ghost can be green where the server says red.**
  `ModelPlacementWorld` only sees the AOI patch ring, so a player or building
  just outside it can make a placement look valid;
  `kg_client_model/AGENTS.md` calls this the one allowed mismatch. → **22 Region
  World Tables** and **03**.
- **Bullet sprites are a fallback today.** `bullets.ron` names the `"bullet"`
  region and the boss content names `"gorgon"`, neither of which is in
  `assets-raw/pack.ron`; `KgBulletLayerComponent2D` draws tinted hexagons on a
  1×1 white texture, and `KgEnemy` falls back to `nova`. Adding the atlas
  regions switches both with no code change. → **12 Region Combat Tables**,
  **14 Region Enemy Tables** and **02 The Client Component Framework**.
- **`player_health.defense` is not replicated to the client mirror.** `NetHealth`
  drops it (`BASE_DEFENSE = 0` today), so `WorldModel` stores 0; a future
  defense stat needs a new event field. → **15 Region Health Tables**.
- **`player_weapon.last_fire_tick` is not replicated.** `weapon_info`
  deliberately drops it (the implementation phase-2 README §13), so a client
  cannot reconstruct the server's exact fire-rate state from the mirror.
  → **21 Region Weapon Tables**.
- **`OfflineAuthority` is not the server.** It deliberately skips the server's
  travel bound (a `Reports` batch carries no shot origin) and uses a strength
  bonus of 0, so offline fights can differ from online ones at the edges.
  → **12 Region Combat Tables** and **02**.

**Design boundaries — state these as behavior, not as bugs:**

- `leave_world`/disconnect drop `online_player` (and the transient
  presence/charge rows) but keep `player` and position rows so a patch swap can
  carry a player; the AOI views hide them through `online_player`, and
  `delete_player_rows` removes them on a patch departure. → **19 Region Player
  Tables** and **16 Region Movement Tables**.
- Collision is client-side (`kg_sim`); `report_movement` never reads footprints,
  and `kg_bot --ignore-collision` is the proof. → **16 Region Movement Tables**
  and **02**.
- There is no timed respawn; the Respawn button calls `respawn`. → **15 Region
  Health Tables**.

The old C# client and `server/spacetimedb` layout are gone. Do not re-add their
gaps or classes; the current equivalents are `kg_net::NetSession`, `KgNet`,
`KgDebugHud`, `client/scenes/main.tscn`, the `player`/`building`/`enemy` tables
and the `report_*` hit reducers.

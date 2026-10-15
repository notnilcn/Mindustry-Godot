<!-- SPDX-License-Identifier: GPL-3.0-only -->
# I00 — Coordinator: close round H, merge the last session, apply the ledger, set the round-I baseline

Round I, written 2026-10-15. Read `docs/I-environment.md` fully first. I00 runs in the MAIN
checkout (`REPO`, session 1, bridge 6970, display `:10`) with NO other session or agent running. It
is the only plan that edits `chains.json` and commits on `main`.

State found by the plan writer (2026-10-15 ~11:20):
* `main` @ `b560e9b1` (merge of H01 pass 5). Baseline on that commit: `mind-core --lib --features msav-import`
  2543 passed / 0 failed / 4 ignored; `mind-headless --lib` 133; `mind-gdext --lib` 44; `parity goldens` 97 PASS.
* `parity/session-2`, `-3`, `-5` are BEHIND `main` (9, 20, 15 commits) with no unmerged work.
* `parity/session-4` is 6 commits AHEAD of `main` (and 16 behind): H03 pass 3, never summarized:
  ```
  e78dfda0 Gameplay H03 T17: toasts and announces render Mindustry markup; localized sector name in the loss toast
  51d36feb Gameplay H03 T16: FX light prims feed the light pass instead of drawing debug discs
  0a7967b3 Gameplay H03 T15: unit shield bubble only while shieldAlpha > 0 (ShieldComp.shieldAlpha)
  6cd5509f PAR-0129: animated shield and build-beam composites
  e8a4afb7 Gameplay H03 T10: status-effect particles, walk ripples and landing dust
  5b8de5bc Gameplay H03 T9: unit factory and reconstructor build preview (Drawf.construct)
  ```
  `git merge-tree --write-tree main parity/session-4` reports no conflicts.
* The session-4 worktree also has UNCOMMITTED H03 T13 work (launch/landing pods) in 5 files:
  `client/rust/mind-core/src/render/drawf.rs`, `client/rust/mind-core/src/world/behavior/campaign.rs`,
  `client/rust/mind-gdext/src/render.rs`, `client/rust/mind-gdext/src/render/canvas.rs`,
  `client/rust/mind-gdext/src/sim_host.rs` (+292 lines; last edited 10:22).
* `chains.json` `updated` = `2026-10-14T10:39`: only H00's ledger changes were applied. Every other
  round-H summary's "ledger changes" table is still pending (§T5 below lists them all).
* The main checkout shows modified tracked files under `.opencode/parity-session/` (session caches).
  Never stage them.

---

## T1 — Preconditions

```bash
cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
pgrep -af "godot --path|godot --editor" | grep -v open-godot-mcp     # must print nothing; otherwise ask the user before killing
pgrep -af "cargo|rustc" | head                                        # must print nothing
git status --short | grep -v '^??' | grep -v '^ M .opencode/'         # must print nothing
git log --oneline -1                                                  # b560e9b1 (or later: re-check everything below)
```
If `main` moved past `b560e9b1`, re-run the checks in the "State found" list before continuing.

## T2 — Park the uncommitted H03 T13 work on its own branch

```bash
cd ../Mindustry-Godot-session4
git status --short | grep -v '^??'          # expect exactly the 5 files above (M)
git switch -c wip/h03-t13-pods              # new branch at e78dfda0, working changes come along
git add client/rust/mind-core/src/render/drawf.rs client/rust/mind-core/src/world/behavior/campaign.rs \
        client/rust/mind-gdext/src/render.rs client/rust/mind-gdext/src/render/canvas.rs \
        client/rust/mind-gdext/src/sim_host.rs
git commit -m "WIP H03 T13: launch/landing pod draw math and pad rows (unfinished; continued by I02 T1)

Parked by I00 from the session-4 worktree (uncommitted at the end of H03 pass 3).

Co-Authored-By: <your model's co-author line>"
git switch parity/session-4                 # back to the clean branch
git status --short | grep -v '^??'          # must print nothing
```
Do NOT build or test the WIP here. I02 T1 merges it.

## T3 — Merge session 4 into main and gate it

```bash
cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
git merge --no-ff parity/session-4 -m "Merge branch 'parity/session-4' into main (H03 pass 3: T9,T10,T11/PAR-0129,T15,T16,T17)"
cd client/rust
L="flock /tmp/opencode/mg-build.lock"; export CARGO_BUILD_JOBS=6
$L cargo test -p mind-core --lib --features msav-import 2>&1 | grep -E "^test result|FAILED|panicked"
$L cargo test -p mind-headless --lib 2>&1 | grep -E "^test result|FAILED"
$L cargo test -p mind-gdext --lib 2>&1 | grep -E "^test result|FAILED"
$L cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt -p mind-core -p mind-gdext -- --check
$L cargo run -q -p mind-headless -- parity goldens | grep -vc PASS          # 0
$L cargo test -p mind-headless --test fx_golden 2>&1 | grep -E "^test result"
cd ../.. && bash tools/godot.sh --headless --editor --quit --path client 2>&1 | grep -E "SCRIPT ERROR|Parse Error"   # nothing
```
If anything fails, the failure came with session 4's commits: fix it in a separate commit
`I00: fix <what> after the session-4 merge` (smallest change; cite the failing test), or revert the
offending commit with `git revert -m 1 <merge>` and record which H03 task must be redone. If `cargo fmt`
prints a diff, run `cargo fmt -p mind-core -p mind-gdext` and commit `I00: cargo fmt`.

Then a short in-engine smoke (session 1):
```bash
CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh
tools/session-play.sh 1          # if it does not return in 5 min, kill it and check the bridge with gmcp godot_health
export OPEN_GODOT_MCP_PORT=6970
# Ground Zero via the Go path (I-environment §4.1), then:
mkdir -p runs/20261015-I00 && python3 tools/gmcp.py --screenshot runs/20261015-I00/gz-after-merge.png
grep -c panic $(ls -t ~/.local/share/godot/app_userdata/Mindustry-Godot/logs/* | head -1)    # 0
```
Look at the screenshot: units must NOT carry a permanent pale disc (H03 T15 fix), and the world must
render. Stop the game and editor afterwards (`pkill -f -- "--path $PWD/client --remote-debug"`, then the
editor pid).

## T4 — Bring every session branch to main

For N in 2 3 4 5:
```bash
cd ../Mindustry-Godot-sessionN
git status --short | grep -v '^??'      # must print nothing (session 4: clean after T2)
git merge --ff-only main                # session 4 is now behind main only (its commits are in main)
git log --oneline -1                    # equals main
```
If `--ff-only` fails for a session, stop and report (someone committed there).

## T5 — Apply round H's ledger changes to `chains.json`

`chains.json` entries look like `{"t": title, "s": surface, "st": status, "u": "YYYY-MM-DD", "run": "<runs dir>", ...}`
under `.ledger["PAR-XXXX"]` (one item per line, sometimes several per line), and notes are string arrays under
`.notes["PAR-XXXX"]` (one string per line). The file has a hand-made layout, so do NOT rewrite it with `jq` or a plain
`json.dump` (that reformats all ~4000 lines). Use this script: it re-serializes only the `ledger` and `notes`
sections in the same layout (it splits the few multi-item ledger lines) and the `updated` stamp, and keeps every
other byte. The plan writer dry-ran it on a copy on 2026-10-15: 1 update + 1 new item changed 107 diff lines and
`chains` (49 recipes) stayed intact. Save it OUTSIDE the repo as `/tmp/opencode/ledger_apply.py` and run it with
`python3 -I`. It never deletes anything.

```python
# Applies round-H ledger changes to chains.json, re-serializing only .ledger and .notes
# (one entry per line) and the "updated" stamp; every other byte of the file is kept.
# usage: python3 -I ledger_apply.py <chains.json> <changes.tsv> [--dry-run]
import json, re, sys

path, tsv = sys.argv[1], sys.argv[2]
dry = "--dry-run" in sys.argv
text = open(path, encoding="utf-8").read()
dec = json.JSONDecoder()

def section(key):
    m = re.search(r'\n"' + key + r'":\{', text)
    start = m.end() - 1
    obj, end = dec.raw_decode(text, start)
    return start, end, obj

ls, le, ledger = section("ledger")
ns, ne, notes = section("notes")
STAMP = "2026-10-15"

def next_id():
    return "PAR-%04d" % (max(int(k.split("-")[1]) for k in ledger) + 1)

made = upd = 0
for line in open(tsv, encoding="utf-8"):
    line = line.rstrip("\n")
    if not line.strip() or line.startswith("#"):
        continue
    kind, ident, status, surface, run, text_ = (line.split("\t") + [""] * 6)[:6]
    if kind == "update":
        item = ledger[ident]
        if status:
            item["st"] = status
        item["u"] = STAMP
        if run:
            item["run"] = run
        notes.setdefault(ident, []).append(STAMP + " round H: " + text_)
        upd += 1
    elif kind == "note":
        if ident not in ledger:
            print("unknown id, skipped:", ident); continue
        notes.setdefault(ident, []).append(STAMP + " round H: " + text_)
        upd += 1
    elif kind == "new":
        if any(v.get("t") == text_ for v in ledger.values()):
            print("exists, skipped:", text_); continue
        new = ident if ident.startswith("PAR-") and ident not in ledger else next_id()
        rec = {"t": text_, "s": surface or "world", "st": status, "u": STAMP}
        if run:
            rec["run"] = run
        ledger[new] = rec
        made += 1
        print("created", new, "|", text_)
    else:
        sys.exit("bad kind: " + kind)

def compact(v):
    return json.dumps(v, ensure_ascii=False, separators=(",", ":"))

def ser_ledger(d):
    return "{\n" + ",\n".join(json.dumps(k) + ":" + compact(d[k]) for k in sorted(d)) + "\n}"

def ser_notes(d):
    # keeps the original layout: one note string per line inside each array
    entries = []
    for k, v in d.items():
        if isinstance(v, list):
            entries.append(json.dumps(k) + ":[\n" + ",\n".join(compact(x) for x in v) + "\n]")
        else:
            entries.append(json.dumps(k) + ":" + compact(v))
    return "{\n" + ",\n".join(entries) + "\n}"

# replace the later section first so earlier offsets stay valid
parts = sorted([(ls, le, ser_ledger(ledger)), (ns, ne, ser_notes(notes))], reverse=True)
out = text
for s, e, rep in parts:
    out = out[:s] + rep + out[e:]
out = re.sub(r'\n"updated":"[^"]*"', '\n"updated":"2026-10-15T12:00:00+10:00"', out, count=1)
json.loads(out)  # must still parse
print(f"updates/notes: {upd}, new items: {made}")
if not dry:
    open(path, "w", encoding="utf-8").write(out)
```

Run it twice: first with `--dry-run` (prints what it would create; check that no "exists, skipped" line is a
surprise), then for real:
```bash
python3 -I /tmp/opencode/ledger_apply.py chains.json /tmp/opencode/changes.tsv --dry-run
python3 -I /tmp/opencode/ledger_apply.py chains.json /tmp/opencode/changes.tsv
git diff --stat chains.json                       # one file, a few hundred lines at most
jq '.chains | length' chains.json                 # 49 (unchanged)
```

Write `changes.tsv` (in `/tmp/opencode/`) with one line per row of the two tables below
(columns: kind, id, status, surface, run, text; TAB-separated).

### T5a — Updates to existing items

| kind | id | status | run | note text |
|---|---|---|---|---|
| update | PAR-0091 | verified | runs/20261014-s3-H09-T6/ | H09 T5/T6 (b069023d, 9ab724ea) + H01 T7 (ebe8c37d) |
| update | PAR-0104 | verified | runs/20261015-s2-H06-T8/ | H06 T2-T8 (Fixes 3c4284c2); conformance suite 0d8a92c5 |
| update | PAR-0106 | verified | | H06 T1 (0248a281): @tick readback in engine |
| update | PAR-0127 | verified | | H07 T6+T7 (7b6d93ec, c4f354be); the imported profile has showweather=false |
| update | PAR-0072 | verified | | H07 T10 (2c2c5975) |
| update | PAR-0203 | verified | | H07 T5 (961a26b0) |
| update | PAR-0202 | verified | runs/20261014-s5-H01-T4/ | H01 T4 (8f803021) velocity model incl. weather wind |
| update | PAR-0156 | verified | runs/20261015-s3-H01-T10/ | H01 T10 (802ffd71) |
| update | PAR-0145 | verified | runs/20261015-s4-H05-T8/ | H05 T8 (2f413cab) |
| update | PAR-0047 | verified | runs/20261015-s4-H05-T8/ | H05 T8 hover + spend |
| update | PAR-0157 | verified | runs/20261015-s5-H03-T3/ | H03 T3 + H08 wire fix 2b6d7cd1 |
| update | PAR-0095 | verified | runs/20261014-s3-H03-T3/ | H03 T3 (b38e9f68) command panel |
| update | PAR-0096 | verified | runs/20261014-s3-H03-T4/ | H03 T4 (7c3eaf2d) |
| update | PAR-0246 | verified | | H03 T6 (d62d763a) |
| update | PAR-0098 | verified | | H00 T2 + H02 T2-T4 + H08 T1-T3 |
| update | PAR-0213 | verified | | H08 T9 (104fa3c4) |
| update | PAR-0137 | verified | | H08 T7 (0879348e) boosting mech in engine |
| update | PAR-0190 | verified | | H02 T5 (3e46e8ca) + H03 T18 (9f042f98) sorter sprite |
| update | PAR-0188 | verified | runs/20261015-s5-H03-T18/ | H03 T18 (9f042f98) |
| update | PAR-0198 | verified | runs/20261014-s5-H04-T6/ | H04 T6/T7 sim; pod visuals in I02 T1 |
| update | PAR-0193 | verified | | pre-decided on the 2026-10-14 round-2 evidence (H04 T8) |
| update | PAR-0129 | fixing | | sim H02 T7 (66031b6a); render 6cd5509f + H03 T15 (0a7967b3) merged by I00; I02 T11 verifies |
| update | PAR-0112 | fixing | | H05 T1-T4, T6 landed; BaseGenerator remains (I04 T17) |
| update | PAR-0218 | fixing | | setprop H06 T6 (c6af4808); sensor tables remain (I03 T3) |
| note | PAR-0084 | | | re-verified after H04 T3 with fix 69820d5b |
| note | PAR-0220 | | | config_tap seam landed (H02 T6 e2f58037); LogicBlock arm = I03 T6 |
| note | PAR-0244 | | | bridge links unblocked by H02 T5/T6; check in I02 T11 |
| note | PAR-0138 | | | payload half verified by H08 T8 (3cee15d3); turret half = I01 T12 |
| note | PAR-0209 | | | re-check in I05 T5 |
| note | PAR-0089 | | | unchanged; needs an Erekir sector in engine |
| note | PAR-0229 | | | mount half fixed by H01 T2 (36e76496) |
| note | PAR-0237 | | | absorption semantics H01 T7 (ebe8c37d) |
| note | PAR-0175 | | | dock release H01 T3 (8a1c4a56) |
| note | PAR-0140 | | | H08 T6a/T6b; middle-click queue still dropped on the wire -> I01 T13 |
| note | PAR-0071 | | | H07 T12 in-engine notes (build-beam voice) |
| note | PAR-0205 | | | H07 T12 in-engine notes (payload pickup/drop, rain loop) |

### T5b — New items (all `world` unless stated; status as given)

| status | surface | run | title |
|---|---|---|---|
| verified | world | runs/20261014-s1-H00-T1/ | Planet Env bits used enum order (Serpulo rules.env 31, Java 233); the player never spawned |
| verified | world | runs/20261014-s2-H01-T1/ | Building and bullet tile coordinates were half a tile off (H01 T1 8e7b68b8) |
| verified | world | runs/20261014-s2-H01-T15/ | Pathfinder had no floor data or Java costs; wave daggers drowned (H01 T15 c6bcf17a) |
| verified | world | runs/20261014-s2-H01-T2/ | Weapons used one target per unit instead of per mount (H01 T2 36e76496) |
| verified | world | | Enemy AI units lived on captured sectors; drop-zone push on non-hasSpawns maps (H01 T5 cb9cb9e2) |
| verified | world | runs/20261015-s5-H01-T6/ | Dead flying units vanished instead of falling as wrecks (H01 T6 fcfa753e) |
| verified | world | | Flying target flags ignored working buildings without items (targetFlagActive, H01 T8 d8ab1c64) |
| verified | world | | DefenderAI/FlyingFollowAI/HugAI/NoAI never ran (H01 T9 8f3477e3) |
| verified | world | runs/20261015-s5-H01-T14/ | Ground Zero wave-1 fight acceptance (H01 T14 0e1f9b60) |
| verified | world | | Wave ground units had no stuck avoidance (H01 T11 c4002abc) |
| verified | world | runs/20261015-s3-H01-T12/ | Assist command / BuilderAI follow did nothing (H01 T12 43554571) |
| verified | world | | CargoAI/AssemblerAI never ran (H01 T13 cdd2133a) |
| verified | world | | Campaign cores never incinerated overflow; belts backed up (H02 T1 dbb2ca2b) |
| verified | world | | Turrets aimed at stale positions and never led targets (H02 T16 be80e18a) |
| verified | world | | Placement guard ignored team, deep water, rotation, derelicts, core radius, visibility, bans (H02 T2/T3) |
| verified | world | | Cores could not be upgraded; core-foundation placeable on free ground (H02 T4 0d1d4484) |
| verified | world | | Drill liquid boost/warmup ignored per-block values (H02 T8 655c2de5) |
| verified | world | | Bridges/nodes/drivers could not be linked by tapping (H02 T6 e2f58037) |
| verified | world | | Liquid modules had no current liquid; conduits mixed liquids (H02 T10 55032f91) |
| verified | world | | Overdrive affected canOverdrive=false blocks (H02 T9 3513060a) |
| verified | world | | Shock mines and conveyor unit transport were inert (H02 T11 158b7d1d) |
| verified | world | | Surge walls never arced lightning; phase walls never deflected (H02 T12 b7b79d3c) |
| verified | world | | RepairTurret and cargo unload point had no save body (H02 T13 e8e2ca2a) |
| seeded | world | | UnitCargoLoader has no save body (needs unit ids across saves) -> I01 T10 |
| seeded | world | | UnitAssembler has no save body and drones never arrange -> I01 T11 |
| verified | world | | Thermal generators never produced (floor heat attributes, H02 T14 9c4f0e80) |
| verified | world | | Unloaders ran 5.45x too fast (H02 T14 9c4f0e80) |
| verified | world | | Beam nodes connected nothing (H02 T15 e6e675c8) |
| verified | hud | runs/20261014-s3-H03-T4/ | Minimap attack indicators lived 15 min and aged while paused (H03 T4) |
| verified | hud | | Minimap was all black on sector entry (H03 T4) |
| verified | world | | Packed shader copies shadowed client/shaders edits (H03 T6) |
| verified | world | runs/20261014-s3-H03-T12/ | Erekir fog placeholder painted the screen black; real fog rendering (H03 T12) |
| verified | hud | | Overlay top missing: hover range, configure outline, drop zone, core radius (H03 T5) |
| verified | world | runs/20261015-s5-H03-T7/ | Fires and puddles were invisible (H03 T7 589e41aa) |
| verified | world | runs/20261015-s5-H03-T8/ | Crafter/generator DrawBlock chains never drawn (H03 T8 8a1eb257) |
| seeded | world | runs/20261015-s5-H03-T8/ | Power-link overlay draws large rings and lines around power buildings -> I02 T6 |
| seeded | world | | Power graph loses links when a building is re-placed -> I01 T14 |
| verified | world | runs/20261014-s3-H09-regression/ | fillItems refilled every core (player included) and exit saves panicked on resume (513eed69, 8a39f6fc) |
| verified | world | | Unit-command wire encoded EntitySeq as f32 (H08 2b6d7cd1) |
| verified | campaign | runs/20261014-s5-H04-T1/ | Campaign turn clock and offline production lost to a stale campaign copy (H04 T1) |
| verified | campaign | runs/20261014-s5-H04-T2/ | Sectors never produced while away (H04 T2) |
| verified | campaign | runs/20261014-s5-H04-T3/ | Resume ignored the sector record (H04 T3) |
| verified | campaign | | Invasions of the played sector never restarted waves (H04 T4, headless) |
| verified | campaign | runs/20261014-s5-H04-T9/ | Unit save chunk lost statuses/items/shields/commands (H04 T9) |
| verified | campaign | runs/20261015-s3-H04-T13/ | Loaded units came back with GroundAI instead of CommandAI (H04 T13) |
| verified | campaign | | Java save import dropped units, fires and puddles (H04 T10, headless) |
| verified | campaign | runs/20261015-s3-H04-V1/ | Prepared sector records never reached the live campaign (69820d5b) |
| seeded | campaign | | Interplanetary Accelerator is a stub |
| verified | campaign | | attackSectors never marked; numbered-sector threat 0 (H05 T1 ba24fe66) |
| verified | campaign | | Generated sectors ignored the sector polygon for building (H05 T3 ab9f87e8) |
| verified | campaign | | No toast for sector invasions (H05 T7 6781351f) |
| verified | ui | | Strings.autoFixed rounded instead of truncating (H05 T9 df856e3f) |
| fixing | ui | runs/20261015-s4-H05-T10a/ | Content info dialog shows no stats (T10a/T10b landed; families remain: I04 T4-T6) |
| seeded | ui | | Hint predicates/events: fungal pass, cores destroyed, cannotupgrade, factorycontrol -> I04 T3 |
| seeded | ui | | Planet globe cannot be dragged or zoomed; last sector not stored; no preset reveal -> I04 T8 |
| seeded | ui | | Planet page lacks rename/icon picker; stats dialog is a text dump -> I04 T9/T10 |
| seeded | ui | | Dialogs hide instantly (Java fades 0.1 s) -> I04 T12 |
| seeded | ui | runs/20261014-s3-verify3/ | Block palette keeps the previous planet's blocks after a sector switch -> I04 T1 |
| verified | world | | Item turrets other than duo had maxAmmo 0 (H06 T7 0d8a92c5) |
| verified | world | | Every gun overlapped: no voice policy (H07 T1 9604cf09) |
| verified | world | | Factory/reconstructor/mender/door/mass driver/force projector/payload-void sounds missing (H07 T2) |
| verified | world | | Mech step and floor walk sounds missing (H07 T3) |
| verified | world | | Bullet hit pitch never varied; heal bullets and abilities silent (H07 T4) |
| verified | world | | Snow wind-howl volume used the wrong noise (H07 T8) |
| verified | world | | Weather env attributes ignored by the water extractor (H07 T9) |
| verified | world | | Continuous-weapon beam loop sound missing (H07 T11) |
| verified | input | | Line drags pointed the wrong way vertically (H08 T1 29ebd1b3) |
| verified | input | | Line placement ignored block line flags (H08 T2/T3) |
| verified | input | | Tapping enemy buildings opened config; empty tap kept panels (H08 T4 42c91529) |
| verified | input | | Config/inventory panels drew over the pause menu (H08 T14 aec06cc3) |
| verified | world | runs/20261014-s3-H09-T1/ | Erekir fog of war was never simulated (H09 T1/T3) |
| verified | world | runs/20261014-s3-H09-T2/ | Turrets and units targeted enemies hidden in fog; building into fog allowed (H09 T2) |
| verified | world | runs/20261014-s3-H09-T4/ | Derelict and outside-map-area buildings kept running (H09 T4) |
| verified | world | runs/20261014-s3-H09-T5/ | Fires never threw fireballs or burned units (H09 T5) |
| verified | world | runs/20261014-s3-H09-T7/ | Building explosions were too weak; no lightning or fireballs (H09 T7) |
| verified | world | runs/20261014-s3-H09-T8/ | Enemy cores were never filled (fillItems, H09 T8) |
| verified | world | runs/20261014-s3-H09-T9/ | Erekir cores stored non-buildable items (H09 T9) |
| verified | world | runs/20261014-s3-H09-T10/ | Game-over and planet stats never counted builds/losses (H09 T10) |
| verified | world | runs/20261014-s3-H09-T11/ | Explored fog was lost on save/resume (H09 T11) |

For the `new` rows use `new` in the kind column and `-` as the id (the script assigns the next free id;
the fillItems row should get `PAR-0249` because commit `513eed69` is titled "Fixes PAR-0249": put
`PAR-0249` in its id column). Commit:
```bash
git add chains.json
git commit -m "chains: apply round-H ledger changes (H01-H09 summaries)

Co-Authored-By: <your model's co-author line>"
```

## T6 — Record the baseline and mark round I open

Write `REPO/docs/I00-coordinator_summary.md` (do not commit it): the merge commit, the test numbers from T3
(exact lines), the smoke screenshot path, the T4 branch heads, the number of ledger updates/new items,
and anything that failed. Then the marker commit (empty is fine):
```bash
git commit --allow-empty -m "I00: round-I baseline (session-4 merged, ledger applied)

Co-Authored-By: <your model's co-author line>"
```
Every round-I executor checks for this marker (`git log --oneline main | grep -m1 'I00'`).

<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L05 — Block UI, hover bars, content info, hints (wave 3: T2, T3 + stretch; T1 session B, T4–T5 wave 4)

Session A (session 2: worktree `Mindustry-Godot-session2`, branch `parity/session-2`, display `:11`, bridge 6980).
Owner of this summary; session B reports L05 T1 (hover bars, PAR-0379) for merging in.

## Per-task status

| Task | Status | Notes |
|---|---|---|
| T1 hover bars (PAR-0379) | session B, wave 4 | — |
| T2 bespoke config panels (PAR-0380) | **partial** | LightBlock ColorPicker and the MessageBlock edit dialog match Java. TargetDummy and CoreBlock team toggles are blocked on sim support (below). |
| T3 content info stat families (PAR-0301, PAR-0427) | **done** | The plan's eight contents plus copper are row-for-row with Java; layout residue is recorded. |
| T4 hint predicates (PAR-0302) | pending (wave 4) | |
| T5 | pending (wave 4) | |
| Stretch | PAR-0425, PAR-0428, PAR-0430 fixed; PAR-0426 and PAR-0435 root cause recorded | |

## Commits

```
aa675500 L05 T2: LightBlock ColorPicker and MessageBlock edit dialog (PAR-0380, part)
dd05c488 L05 T2: ColorPicker hex/rgba truncate like Arc; Tex.pane preview padding
dbb697ca L05 T3: content info stat families row-for-row with Java (PAR-0301, PAR-0427)
bbe7b8ec L05 stretch: clicking the selected palette block deselects it (PAR-0425)
896f1622 L05 stretch: Custom Rules rows use the category pad(5) (PAR-0430)
4325fba2 L05 stretch: Research items box and button row match Java (PAR-0428)
```

## T2 — bespoke config panels (`runs/20261018-s2-L05-T2/notes.md`)

### What changed
- **ColorPicker** (`ui/dialogs/color_picker.gd`, the existing `picker` manifest dialog, rewritten to
  `ColorPicker.java:27-170`):
  - a 200x200 preview over `Tex.alphaBg`, inside a `Tex.pane` frame with 12 px padding;
  - 370x44 slider rows, each stacked over its gradient: hue, saturation, value, and alpha only when requested
    (over `alphaBgLine`);
  - a 130x40 hex field;
  - Back plus `@ok` (`Icon.ok`);
  - `show_picker(color, alpha, consumer)`;
  - hex text and RGBA use Arc's truncation (`(int)(c * 255)`): `ffd37f7f`, where Godot rounds to `80`.
- **LightBlock** (`LightBlock.java:109-114`): a 40 px pencil (`cleari`) closes the panel and opens the picker with
  `color.a(0.5)` and no alpha row. OK configures `res.rgba()`.
- **MessageBlock** (`MessageBlock.java:109-154`, desktop): a 40 px pencil opens the floating `@editmessage` dialog
  (`UiRoot.show_message_edit`):
  - 380x160 TextArea, capped at `maxTextLength`;
  - a newline filter at `maxNewlines`;
  - a light-gray `n / max` counter;
  - `@ok` 130x60; an unchanged text is not reconfigured.

### Tests and evidence
- `ui_widgets_check.gd`:
  - the picker builds 3 rows without alpha and 4 with alpha;
  - opaque hex drops the alpha byte.
- In-engine, real input (blocks staged with `SimHost.place_block`):
  - illuminator → pencil → hue click → OK configures `0x8F7FFF80`;
  - message → pencil → `GT "hello world"` → OK stores the text.
- Java pairs (`java-ingame/`, sandbox with Lighting on, because the illuminator is `lightingOnly`):
  - picker: MATCH, rows about 8 px lower;
  - message dialog: MATCH, within 7 px.

### Blocked (exact needs)
- **TargetDummy** (`TargetDummy.java:240-262`) needs:
  1. an `int[]` config value. `ConfigValue` has no int-array variant, so this is a new determinism wire tag
     (`mind-core/src/determinism/command.rs`) and a `sim_host.rs configure_building` parse;
  2. `TargetDummyState` fields `boosting`, `dummy_size`, `unit_team` and the 5-int `config(int[])` handler;
  3. the tethered dummy unit (`UnitTetherBlock`) the panel's team/flying/hit-size options act on.
  The panel UI itself is small once those exist.
- **CoreBlock team toggles** (`CoreBlock.java:289-330`, only when `rules.coreBuildAndConfig`):
  - `configure(team.id)` changes the **player's** team (`configured`: `builder.team(next)`). The player team is
    `SimHost.default_team()`, in `sim_host.rs` (session B).
  - Needed: a configured-core hook that sets the player team when `core_build_and_config` is on, plus
    `config_spec` emitting the six base teams while the rule is on. I can do the `config_spec` part in
    `input/desktop.rs`.

## T3 — content info stat families (`runs/20261018-s2-L05-T3/notes.md`)

Method:
- **Java:** Core Database search, typed with `JT` (XSendEvent); first result; info page captured plus 12 wheel
  notches.
- **Godot:** the same flow with real input (GC, GT, pynput wheel).
- **Contents:** duo, lancer, ripple, force projector, mono, poly, cultivator, combustion generator, and copper for
  PAR-0427.

### Fixes (all cited in `dbb697ca`)
- **PAR-0427** (`0 %` → `0%`): the content-info stat line had a 6 px HBox gap between cells. Arc cells have no
  spacing (`ContentInfoDialog.java:89-97`). The page also opens at the top on every show.
- **Ammo rows** (`StatValues.java:727-900`):
  - armor pierce / weakness / partial / anti-armor, for units and buildings;
  - heal percent and amount; incendiary;
  - `statusText` is reactive (Blasted has no duration) with `autoFixed(chance, 4)`;
  - splash radius uses `Strings.fixed` truncation (4.6 tiles, not 4.7);
  - unit weapons are `compact` (no `ammo/item`).
- **Content data:**
  - lancer laser `ammoMultiplier = 1` (`Blocks.java:3615`);
  - hail and ripple pyratite apply burning for 12 s (`Blocks.java:3523-3524, 4030-4031`). This is a real sim
    change; the headless goldens still pass.
- **Consumers:**
  - optional (`.boost()`) items and liquids report under Booster (`ConsumeItems.java:56-58`), so the force
    projector's phase fabric moved out of Input;
  - force projector water/cryofluid coolant panels (`ForceProjector.java:120-140`);
  - the combustion and radioactive generator item lists with efficiency, Item Capacity and Production Time
    (`StatValues.itemEffMultiplier`, `StatValues.java:569-586`);
  - `items(timePeriod)` rows show the item name (`StatValues.java:157-163, 310-315`).
- **Units** (`UnitType.java:790-850`):
  - Item Capacity and Range carry no unit;
  - Payload Capacity only for `Payloadc` units;
  - Build Cost;
  - the Mine Tier drillables list;
  - `Ability.display` per class: repair, shield-regen, force, status and energy fields, and regen.

### Tests
- `cargo test -p mind-core --lib content_stats`: 20 passed. New tests:
  - `lancer_ammo_has_armor_weakness_and_no_ammo_per_item`
  - `ripple_status_rows_follow_java`
  - `force_projector_lists_coolant_boosters`
  - `mono_unit_stats_follow_java`
  - `combustion_generator_lists_flammable_items`
  - `graphite_press_stats` was updated for the item name.
- `cargo test -p mind-core`: 2763 passed, 3 failed. The failures are `asteroid` / `erekir` / `serpulo
  _generation_checksum`; they **also fail on main 4cd2419a** without these changes (session B's L02 generation
  work, golden batch pending).
- `cargo clippy -p mind-core -p mind-gdext --all-targets -D warnings`: clean. `cargo fmt --check`: clean.
- `mind-headless parity goldens`: PASS.

### Residual (recorded)
- Cultivator Affinities: the sim kind-data has `attribute = -1` (the port's `Attribute` has no spores).
- Status emoji glyphs, the weapon preview sprite, and panel spacing.
- Frag/interval panels render expanded instead of collapsible.
- `reloadWhileCharging`.
- Spawn/arc/suppression ability classes show name and description only.

## Stretch
- **PAR-0425:** clicking the selected palette block deselects it (`PlacementFragment.java:306-307`). Two real
  clicks: conveyor → air.
- **PAR-0428:** Research.
  - ItemsDisplay frame margins and toggle height (`ItemsDisplay.java:35-37`); box at (6,6).
  - Button row bottom inset 15 px (`ResearchDialog.java:110`). Back 569–632 vs Java 570–633.
- **PAR-0430:** Custom Rules category `pad(5)`, with `padTop(0)` only on table rows (`CustomRulesDialog.java:377-478`).
  Check pitch is 42 px, as in Java.
- **PAR-0426 / PAR-0435** (Core Database rows 10–16 px low; Settings Graphics list 9 px high): not fixed. Root
  cause: in both Java frames the title rule sits at y=27, not 30, and the search row is 6 px shorter. Arc shrinks
  rows toward their minimum height when a fill dialog's content overflows the viewport; `MindTable` always lays
  out at the minimum. That is a layout-engine change, best done with an Arc-style min/pref interpolation in
  `MindTable`.

## Ledger changes to apply

| Item | Change | Evidence |
|---|---|---|
| PAR-0380 | partial: LightBlock + MessageBlock done and verified; TargetDummy / CoreBlock blocked (needs above) | `runs/20261018-s2-L05-T2/` |
| PAR-0301 | close for the plan's eight contents (row-for-row), residual list above | `runs/20261018-s2-L05-T3/diff/info-*-side.png` |
| PAR-0427 | close | `runs/20261018-s2-L05-T3/diff/info-copper-side.png` |
| PAR-0425 | close (verified against Java source; in-engine real clicks) | commit `bbe7b8ec` |
| PAR-0428 | close | `runs/20261018-s2-L01-T3/godot/research.png` vs java |
| PAR-0430 | close | `runs/20261018-s2-L01-T3/godot/rules-banned.png` |
| PAR-0426, PAR-0435 | keep open; root cause = Arc min/pref row compression on overflow (layout engine) | this summary |
| new | Cultivator Affinities missing (sim attribute `-1`, no spores attribute) | `diff/info-cultivator-side.png` |
| new | TargetDummy config needs an int-array `ConfigValue` + dummy state | T2 above |

## Done criteria (wave 3 scope)
- T2: two of four panels match a Java capture; the other two have precise blockers.
- T3: the plan's pages match Java row for row, with the residue recorded.
- T1 is session B's in wave 4; T4–T5 are wave 4.

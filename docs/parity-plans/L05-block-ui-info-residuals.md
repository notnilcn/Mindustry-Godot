<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L05 — Block UI, hovered-block bars, content info and hints

**Goal.** Finish the per-block UI families that round J recorded but did not port.

**Build.** Mixed: T1 and any task that extends `config_spec` are Rust
(`mind-gdext/src/sim_host.rs`, `input/desktop.rs`) — `cargo check` while iterating, one
`tools/build.sh` before the in-engine hover check. The panels, stat rows, hints and icon
sizing are GDScript.

**Evidence.** PAR-0379, PAR-0380, PAR-0301, PAR-0302; `docs/J05-block-config-ui_summary.md`
(residual list); `docs/I04-campaign-ui_summary.md` deviations (round I, still open).

## T1 — Hovered-block bars for every family (PAR-0379) — HIGH

- Java: `Block.setBars()` / `addBar(name, build -> new Bar(...))` per block class, shown by
  `Building.display` in the `PlacementFragment` hover box. Enumerate them with
  `grep -rn "addBar(\|removeBar(" ../Mindustry/core/src/mindustry/world`.
- Port: `SimHost.building_bars_json` covers health, the core capacity and the J05 families.
  Missing:
  - turrets (ammo/reload, liquid, heat);
  - power nodes (`power` + `nodes`), diodes, batteries, generators (`poweroutput`), reactors
    (`heat`, `power`);
  - overdrive/force projector (`shield`, `boost`);
  - pumps (liquid);
  - payload loader/unloader;
  - unit assemblers, factories (the 3-arg `units` bar: `UnitFactory.java` `addBar("units", …)`),
    `HeatCrafter`'s `heatpercent` (2-arg label), `GenericCrafter` output-liquid bars;
  - campaign pads.
- Accept: hover each block with the real pointer (`GP`) in a sandbox and match Java's box
  (bar names, colours, fractions).

## T2 — Bespoke configuration panels (PAR-0380) — MEDIUM

| Block | Java | Panel |
|---|---|---|
| TargetDummy | `world/blocks/defense/TargetDummy.java:240-…` | enemy-team check, flying check, armor / reset time / hit size fields |
| CoreBlock | `world/blocks/storage/CoreBlock.java:289-…`, only when `rules.coreBuildAndConfig` | 32 px team-colour toggles |
| LightBlock | `world/blocks/power/LightBlock.java` | `ui.picker.show(color, …)`, the `ColorPicker` dialog (port the `ui/dialogs/ColorPicker.java` layout) |
| MessageBlock | `world/blocks/logic/MessageBlock.java` | `maxTextLength`, `maxNewlines` caps in the text dialog |

Accept: each panel matches a Java capture in a sandbox (`coreBuildAndConfig` on for the core).

## T3 — Content info stat families (PAR-0301) — MEDIUM

Round I recorded these as not ported (`docs/I04-campaign-ui_summary.md`, deviations T4/T6):
- `ForceProjector` coolant panel (`ForceProjector.java:117-140`);
- turret coolant booster panels (`ReloadTurret.java:17`, `LaserTurret.java:26`) — check what
  I04's later pass added;
- collapsible frag/interval bullet panels (`StatValues.java:906-945`);
- `statusText` emoji (`StatValues.java:229`);
- `reloadWhileCharging`;
- unit `mineTier` → `StatValues.drillables` list (`UnitType.java:824-833`);
- ability `display` per class (abilities render as name + knob);
- `AttributeCrafter` detailed efficiency/output (`mind-core/src/ui/content_stats/
  production.rs:18`);
- the `ConsumeItemEfficiency` `itemEffMultiplier` row (J04 T2 deviation 4).

Accept: Core Database → content info for duo, lancer, ripple, force projector, mono, poly,
cultivator and combustion generator matches Java row for row.

## T4 — Hint predicates (PAR-0302) — LOW

Check each `HintsFragment.DefaultHint` predicate (`ui/fragments/HintsFragment.java`) against the
port's producers. Round I landed `cannotupgrade`/`factorycontrol`/`cores_destroyed`; the
"fungal pass" hint and any others without a producer remain. List the hints the port never
fires (a headless table test over every `DefaultHint`), then port or record each one.

## T5 — `ItemSelection` icon size — LOW

Java sizes each icon `Mathf.clamp(item.selectionSize, 0, 40)` (`ItemSelection.java:70`;
`UnlockableContent.selectionSize = 24f` for items/liquids, `Block` 28, `UnitType` 30;
`UnlockableContent.java:44`, `Block.java:450`, `UnitType.java:534`). Godot's icons look
~32 px in a 40 px cell (`diff/22-srccfg-zoom.png`). Pin the size to `selectionSize`.

## Done criteria

T1 and T2 landed and matched. T3 rows landed or recorded with Java file:line. T4's table test
landed. Summary in `docs/L05-block-ui-info-residuals_summary.md`.

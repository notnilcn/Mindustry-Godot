# Mod content field ledger (plan 20 §7a/R4)

Tracks accepted JSON field names per content kind. Unknown fields warn and are
ignored (`ignoreUnknownFields = true`); this ledger records the implemented set
so drift from upstream Java fields is visible.

Status as of 2026-10-02 (lane/20-mods): **M1 item + block core fields implemented.**

## Item (`mindustry.type.Item`)

| JSON field | Rust target | Status |
|---|---|---|
| `name` | bundle `item.<name>.name` + `localized_name` | implemented |
| `description` | bundle `item.<name>.description` | implemented |
| `color` | `Item.color` (hex or `[r,g,b,a]`) | implemented |
| `hardness` | `Item.hardness` | implemented |
| `cost` | `Item.cost` | implemented |
| `explosiveness` | `Item.explosiveness` | implemented |
| `flammability` | `Item.flammability` | implemented |
| `radioactivity` | `Item.radioactivity` | implemented |
| `charge` | `Item.charge` | implemented |
| `healthScaling` | `Item.health_scaling` | implemented |
| `lowPriority` | `Item.low_priority` | implemented |
| `frames` | `Item.frames` | implemented |
| `transitionFrames` | `Item.transition_frames` | implemented |
| `frameTime` | `Item.frame_time` | implemented |
| `buildable` | `Item.buildable` | implemented |
| `hidden` | `Item.hidden` | implemented |
| (all other `UnlockableContent` fields) | `UnlockFields` | TODO (M2+) |

## Block (`mindustry.world.Block` and subclasses)

| JSON field | Rust target | Status |
|---|---|---|
| `name` / `description` | bundle + `UnlockFields` | implemented |
| `type` | `BlockKind` (ClassMap, M2) | partial aliases |
| `size`, `health`, `armor` | `BlockSpec` | implemented |
| `itemCapacity`, `liquidCapacity` | `BlockSpec` | implemented |
| `solid`, `floating`, `update`, `destructible` | `BlockSpec` | implemented |
| `saveData`, `saveConfig`, `configurable`, `inEditor` | `BlockSpec` | implemented |
| `hasItems`, `hasLiquids`, `hasPower` | `BlockSpec` | implemented |
| `placeablePlayer`, `placeableLiquid`, `placeableOn` | `BlockSpec` | implemented |
| `buildVisibility`, `category` | `BlockSpec` | implemented |
| `requirements` | `BlockSpec.requirements` | implemented |
| `consumes` | `BlockDef.consumes` | TODO (M3) |
| `unit_plans*`, `upgrades`, `assembler plans` | `BlockDef` | TODO (M2/M3) |

## Other kinds

`liquid`, `status`, `unit`, `weather`, `sector`, `planet`, `team` and all nested
definables (`bullet`/`effect`/`ability`/`draw`/`shoot`) are TODO in M2.

# Steam parity evaluation (plan 22 §3.9 — OD4)

> Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

**Status: deferred.** Locked 2026-10-01 (NUD-04/51=A): Steam is **not** implemented
in this phase. This document records exactly what full parity would require so the
decision can be revisited without re-deriving the surface.

## Upstream surface (`desktop/src/mindustry/desktop/steam/*`)

| Class | Responsibility | App id |
|---|---|---|
| `SVars` | Steam init/teardown, `appId = 1127400`, rich presence display keys | `1127400` |
| `SNet` | Steam Networking Sockets transport + lobbies | — |
| `SUser` | Steam account ↔ game identity | — |
| `SStats` | Achievements/stats backend for `GameService` | — |
| `SWorkshop` | UGC publish/subscribe for maps/schematics/mods | — |

## What this port ships now

- `mind_core::service::GameService` + `NullService`: the full trait shape from
  `DesktopLauncher`'s anonymous class. Every method is a no-op and
  `enabled()` is `false`.
- `mind_gdext::platform::workshop`: `workshop_maps`/`workshop_schematics`/
  `workshop_mods` return empty vectors; `workshop_available()` is `false`.
- `STEAM_APP_ID` and `DISCORD_APP_ID` are kept as constants for docs/tests.
- `+connect_lobby <id>` is parsed and stored, but never acted on without Steam.

## Full-parity checklist (for a future plan)

1. **Native SDK**: add the Steamworks SDK through a native GDExtension
   (e.g. GodotSteam) as a compiled-in or optional library; record its license in
   `THIRD_PARTY_NOTICES.md`.
2. **Identity**: map the Steam account id to a SpacetimeDB identity and store the
   link locally (`config/identity/`), then use it for `getUUID()` and
   `+connect_lobby`.
3. **Workshop**: map publish/subscribe onto plan 20's mod/map/schematic loaders;
   the `workshop_*` functions here are the seam.
4. **Lobbies / networking**: map Steam lobbies onto plan 21 matches; decide
   whether Steam Networking Sockets replaces the STDB relay transport (OD3).
5. **Rich presence**: port the exact display strings (`steam_display=#steam_status_raw`,
   `steam_status` = map/UI string) and per-frame throttle.
6. **Achievements/stats**: implement `GameService` over `SStats` and hand it to
   `MindPlatform`'s `ServiceRegistry` (replacing `NullService`).
7. **Build/modifier**: `Version.modifier` contains `steam`, so `BuildInfo::is_steam`
   flips and Steam-specific caps/tier defaults apply.

Until all seven land, `MindPlatform::service_enabled()` stays `false`.

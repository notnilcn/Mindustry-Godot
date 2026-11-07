# Steam parity evaluation (plan 22 §3.9 — OD4)

> Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

**Status: Steam is still deferred (OD4 / NUD-04/51=A); the platform-neutral
surface is now complete.** No Steamworks code is compiled and
`MindPlatform::service_enabled()` stays `false`, but the game no longer depends
on a Steam backend for achievements/stats, perf counters, GPU probes or the
update check. The Steam half is tracked as `seeded` in PAR-0591; the neutral
halves landed in PAR-0596/0598/0599 (2026-11-07, headless-verified).

## Upstream surface (`desktop/src/mindustry/desktop/steam/*`)

| Class | Responsibility | App id |
|---|---|---|
| `SVars` | Steam init/teardown, `appId = 1127400`, rich presence display keys | `1127400` |
| `SNet` | Steam Networking Sockets transport + lobbies | — |
| `SUser` | Steam account ↔ game identity | — |
| `SStats` | Achievements/stats backend for `GameService` | — |
| `SWorkshop` | UGC publish/subscribe for maps/schematics/mods | — |

## What this port ships now

### Platform services (complete, 2026-11-07)

- `mind_core::service::GameService`: the full trait shape from
  `DesktopLauncher`'s anonymous class plus the forward seams — `enabled`,
  `complete_achievement`/`clear_achievement`/`is_achieved`,
  `get_stat`/`set_stat`, `store_stats`, `submit_score` (leaderboard seam),
  `delete_achievement`/`has_achievement` aliases, guarded `open_link`/`open_file`
  (`http`/`https`/`mindustry` schemes only; regular files only) and logged
  `export_data`/`import_data`. `NullService` remains the default and is a
  complete no-op. (PAR-0596)
- `mind_core::game::achievements`: the `SStat` (21 stats) and `Achievement`
  (90 entries) tables in Java declaration order, with the
  `achievement-<name>` / `achievement-stat-<name>` persistence keys.
- `mind_core::game::service`: `LocalGameService` (real local stats/achievements
  through any `UnlockStore`) and `ServiceTracker` (the listener half:
  blocks-built/units-built sets and stat/achievement increments). `MindCampaign`
  owns one shared tracker, injects it into the live `CampaignRuntime`, and
  persists it on boot/autosave/exit. (PAR-0566/0579)
- UI: the Database achievements/statistics tabs read the persisted tables
  (`ui/kit/rows.rs`, `MindUi.database_rows_json`). (PAR-0595)
- **The Steam seam is still null:**
  `mind_gdext::platform::service::ServiceRegistry` holds `NullService`, so
  `service_enabled()` is false until a real backend is supplied (checklist
  item 6).

### Performance, GPU and update (complete, 2026-11-07)

- `mind_core::platform::perf`: PerfCounter port over `WindowedMean(120)` with
  the 100 ms value refresh, the 14 `PerfCounterKind` counters in upstream order
  and `PerfCounters::finish_frame` (`ClientLauncher.update` "other"
  aggregation). `MindPlatform` samples the frame delta and exposes
  `get_perf_stats()`. (PAR-0598)
- `mind_core::platform::gpu`: `IntelGpuCheck` marker init/`was_intel` and
  `NvGpuInfo` memory queries over an injected GL seam (None when
  `GL_NVX_gpu_memory_info` is absent). (PAR-0599)
- `mind_core::platform::update`: the offline BeControl disposition
  (`is_bleeding_edge`, release JSON + asset selection, `newBuild > build`,
  `version.properties` read). The network/dialog self-replacing download half
  stays intentionally unported.
- `mind_gdext::platform::crash`: GPU crash notice + report host glue.

### Steam-only surface (unchanged, PAR-0591)

- `mind_gdext::platform::workshop`: `workshop_maps`/`workshop_schematics`/
  `workshop_mods` return empty vectors; `workshop_available()` is `false`.
- `STEAM_APP_ID` (`1127400`) and `DISCORD_APP_ID` are kept as constants for
  docs/tests; Discord presence stays compile-gated behind the default-off
  `discord` feature (`docs/platform/discord.md`).
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
6. **Achievements/stats backend**: implement `GameService` over `SStats` and hand
   it to `MindPlatform`'s `ServiceRegistry` (replacing `NullService`). The local
   half already tracks and persists stats (`LocalGameService`/`ServiceTracker`).
7. **Build/modifier**: `Version.modifier` contains `steam`, so `BuildInfo::is_steam`
   flips and Steam-specific caps/tier defaults apply.

Until all seven land, `MindPlatform::service_enabled()` stays `false`.

## Decision record

- 2026-10-01: NUD-04/51=A — Steam is out of scope for this phase.
- 2026-11-07: the Steam half stays `seeded` (PAR-0591) with this rationale; the
  platform-neutral work (PAR-0596/0598/0599) is closed. Revisit only with a
  Steamworks-SDK GDExtension; the checklist above is the entry point.

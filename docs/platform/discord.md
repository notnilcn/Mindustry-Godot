# Discord Rich Presence (plan 22 §3.9 — OD4)

> Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

**Status: deferred, compile-gated feature off by default.** Discord is optional in
upstream and is only evaluated here; no RPC code is compiled or linked by default.

## Upstream behavior to port exactly

App id: `610508934456934412` (`DISCORD_APP_ID`). Opt-out when the `nodiscord`
environment variable is set. Presence is updated on the same per-frame cadence,
throttled to Discord's rate limit.

| Game state | `state` | `details` | `largeImageKey` | `largeImageText` |
|---|---|---|---|---|
| In game | `"<mode> | <N> Players"` | `"<map> | Wave <N>"` | `"logo"` | `"Wave <N>"` |
| In editor | `"In Editor"` | — | `"logo"` | — |
| Launch selection | `"In Launch Selection"` | — | `"logo"` | — |
| In menu | `"In Menu"` | — | `"logo"` | — |

`Vars.steam`/Discord presence strings are owned by the same "platform UI string"
source that feeds the Steam rich presence.

## Adoption checklist

1. Pick a Discord IPC binding (a GDExtension/addon, e.g. `discord-rpc` wrappers).
   Record its license in `THIRD_PARTY_NOTICES.md` before enabling.
2. Add a `discord` Cargo feature in `mind-gdext` (default **off**); the presence
   mapping lives in `platform/discord.rs` behind `#[cfg(feature = "discord")]`.
3. Route calls through the `GameService` seam (or a sibling presence trait) so
   gameplay code is unchanged.
4. Acceptance: enabling the feature **without** the plugin still starts cleanly
   and logs a single disabled notice; disabling it changes nothing.

## Current state

`mind_core::service::discord_presence` owns and unit-tests the mapping above;
`mind_gdext::platform::discord` re-exports it and `start()` is compile-gated
behind the default-off `discord` feature. With the feature enabled but no plugin
linked, the binary logs `RPC plugin not linked; presence disabled` and starts
cleanly — nothing opens a Discord IPC connection.

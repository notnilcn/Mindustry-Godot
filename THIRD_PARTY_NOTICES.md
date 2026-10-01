# Third-Party Notices

Mindustry-Godot is a **GPL-3.0 derivative work of Mindustry**. This file records the
third-party projects whose code, assets, or concepts are used in this repository, with the
license terms and upstream links. Versions are the pins in `client/rust/Cargo.toml` or the
vendored addon metadata.

| Name | Version | License | Upstream | Purpose |
|---|---|---|---|---|
| Mindustry (code + assets) | porting target (snapshot checked out alongside as `../Mindustry`) | GPL-3.0 | https://github.com/Anuken/Mindustry | Source of the behavior, content, asset set and test oracle being ported |
| Arc | bundled with the Mindustry source distribution (no standalone local checkout) | See Mindustry distribution | https://github.com/Anuken/Arc | Java game library Mindustry builds on; `Time`, `Rand`, collections and event/log semantics are ported |
| godot-rust `godot` crate | 0.5.5 (feature `api-4-7`) | MPL-2.0 | https://github.com/godot-rust/gdext | GDExtension bindings for the Godot 4.7 client |
| `bevy_ecs` | 0.19.1 (exact pin) | MIT OR Apache-2.0 | https://github.com/bevyengine/bevy | ECS backing the authoritative simulation |
| SpacetimeDB module crates (`spacetimedb`) | 2.10.1 (server module, lands with M5) | BSL-1.1 — see note below | https://github.com/clockworklabs/SpacetimeDB | Rust stored-module framework for `server/spacetimedb` |
| SpacetimeDB Rust SDK (`spacetimedb-sdk`) | 2.10.1 (exact pin; used by `mind-stdb` now) | BSL-1.1 — see note below | https://github.com/clockworklabs/SpacetimeDB | Client connection, subscriptions and reducer calls |
| Open Godot MCP addon | 0.1.10 | MIT | https://github.com/masteryee-labs/Open-Godot-MCP | Editor/game MCP bridge used for in-engine verification |
| BlastBullets2D | vendored addon (`client/addons/blastbullets2d`) | MIT | https://github.com/nikoladevelops/godot-blast-bullets-2d | Retained candidate for bulk bullet FX (evaluated in plans 10/17); unused at P0 |
| Phantom Camera | 0.11.0.3 | MIT | https://github.com/ramokz/phantom-camera | Retained candidate for the RTS camera (evaluated in plan 15); unused at P0 |

## SpacetimeDB license note

The locally cached `spacetimedb-sdk 2.10.1` and `spacetimedb 2.10.1` crates ship the
**SpacetimeDB Business Source License 1.1** (`license-file = "LICENSE"`), copyright
Clockwork Laboratories, Inc. Parameters (from the bundled license text):

- **Additional Use Grant:** production use of the Licensed Work with no more than one
  SpacetimeDB instance, provided it is not used for a "Database Service".
- **Change Date:** 2031-09-08.
- **Change License:** GNU Affero General Public License v3.0 with a linking exception.

The plan set's default assumption was Apache-2.0; the crates' bundled license files were
verified from the local cargo registry during M0 and say otherwise. This entry records the
actual terms. Anyone distributing a hosted/multi-instance service on this stack must review
these terms.

## Mindustry assets

Ported Mindustry assets keep their original names, keys and credits (Mindustry's bundled
credits file is the authoritative attribution list); they remain under GPL-3.0 as part of
this derivative work.

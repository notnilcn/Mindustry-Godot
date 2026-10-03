# 22 — PLATFORM, DISTRIBUTION & DEDICATED SERVER IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.` (Rust), `## Ported from Mindustry ... — GPL-3.0` (GDScript), or `; Ported from Mindustry ... — GPL-3.0` (`.tscn`/`.cfg`).
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Read order: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | 🟡 **M0 + M2 COMPLETE 2026-10-02 (lane/22-export); M3–M5 COMPLETE at the headless/parse-verifiable level 2026-10-03 (lane/f23-22); M1/M6/M7 gated.** M0 landed the pure-Rust platform foundation (`BuildInfo`, caps, args, `version`, `MindPlatform` skeleton). M2 landed the dedicated `mind-headless server` (config/console/commands/rules/autosave/logs/socket + local host). M3 added the plan-21 host/admin seam (`NetHost` + `config/{admins,bans,whitelist}.json` mirror + offline delegation tests); M4 the `GameService`/updater/Discord/Steam/workshop seams (Discord feature default-off); M5 the association/import/URI seams + `MindPlatform` wired into `spine.tscn`. M1 needs Godot 4.7.2 export templates (still absent, R10) + the single-editor MCP mutex; M6/M7 need Android/other export templates. The live `spacetime start` two-client gate (M3) and the in-engine association/drag-drop gates (M5) are deferred — no live STDB/editor/export in the lane environment. See the Changelog. Originally: Draft — 2026-10-01, not started. Plans 18/21 are on disk (the parallel-authorship notes are stale); the OD-P items were resolved 2026-10-01 (23 §8.1.1): pure-Rust server, Steam/Discord deferred, iOS cut, `mobile` renderer. |
| **Phase** | P7 — Editor, mods, export (HLP §3 row 22); the final platform surface. |
| **Depends on** | `14_UI_IMPLEMENTATION_PLAN.md` (FileChooser params/fallback, `MindUi.close_top_dialog`, safe areas, `Vars.mobile` source), `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` (touch/gesture parity, `MindCamera2D`, mobile buttons, native text input), `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (screenshot/capture APIs, renderer method, layer capture), `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (**not on disk**; host/join, admin/ban/whitelist, relay, transport OD3 — interface assumed in §3.12). Transitively `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (`mind-headless`, logging/`last_log.txt`, data dir, MCP rig), `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` (`mind-stdb` connector/token/relay), `03_ASSETS_IMPLEMENTATION_PLAN.md` (packed runtime assets shipped in exports), `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (`FileSystem`/`Paths`, `SettingsStore`, `SaveSlot::import_file/export_file`, OD2 importer), `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (`Platform`/`HeadlessPlatform` trait, `SimConfig`, fixed-step runner), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (save/autosave policy, `SAVE_EXTENSION`/`SCHEMATIC_EXTENSION`), `18_AUDIO_IMPLEMENTATION_PLAN.md` (**not on disk**; audio settings/bus, application-focus pause — assumed), `20_MODS_IMPLEMENTATION_PLAN.md` (mod directory loading, data patches, likely-mod detection input), `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (scenario catalog registration). |
| **Blocks** | Nothing — this is the last surface planned. It **feeds** `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` with the export/server/MCP scenario catalog and the boot/memory budgets. |
| **Sources** | Mindustry: `desktop/AGENTS.md`, `server/AGENTS.md`, `android/AGENTS.md`, `ios/AGENTS.md`, `core/src/mindustry/core/AGENTS.md` (all read in full); `desktop/src/mindustry/desktop/DesktopLauncher.java` (526 lines), `desktop/src/mindustry/desktop/steam/*` (SVars/SNet/SUser/SStats/SWorkshop — enumerated, not read), `server/src/mindustry/server/ServerLauncher.java` (85 lines), `server/src/mindustry/server/ServerControl.java` (1471 lines; command table, config, autosave, logs, socket read), `android/src/mindustry/android/AndroidLauncher.java` (330 lines), `android/AndroidRhinoContext.java` (enumerated), `ios/src/mindustry/ios/IOSLauncher.java` (242 lines), `core/src/mindustry/core/Platform.java`, `core/src/mindustry/core/Version.java`, `core/src/mindustry/net/CrashHandler.java`, `core/src/mindustry/net/BeControl.java`, `core/src/mindustry/net/Administration.java` (34 `Config` keys), `core/src/mindustry/Vars.java` (dirs/`checkLaunch`/`finishLaunch`/`loadLogger`), `core/src/mindustry/ClientLauncher.java` (`dataDir`, `fileDropped`, `handleFileImport`, `setup`), `core/src/mindustry/ui/FileChooser.java`, `core/src/mindustry/ui/Menus.java:369` (`openURI`), `core/assets/AGENTS.md`; `fastlane/` (metadata only). Repo: `mindustry-godot/HIGH_LEVEL_PLAN.md`, `PRELIMINARY_PLAN.md`, sibling plans 00/01/03/04/05/06/12/14/15/16/17; `client/project.godot` (current export-relevant state: dotnet block, `Mobile` feature, `rendering_method="mobile"`, d3d12 driver — all superseded by plan 00 §3.4). Skills: `godot-compositor-testing`, `playtest`. |
| **Extends spine** | (a) `mind-gdext` module `platform/` + autoload node `/root/Spine/MindPlatform` (appended; all plan-00 paths unchanged) exposing `#[func]` probes for MCP (`build_info`, `platform_caps`, `window_state`, `dev_file_drop`, `dev_open_uri`). (b) `mind-headless` gains a **lib target** (`mind_headless`) and a `server` mode (`mind-headless server [--config-dir …] [commands…]`) plus `mind-headless version`. (c) `client/export_presets.cfg` + `tools/export.sh|.ps1`, `tools/associate.sh|.ps1`, `tools/version.sh|.ps1` (bash primary). (d) `client/assets/version.properties` (generated, gitignored) and `mind_core::version::BuildInfo`. (e) MCP scenario family `platform_*`/`server_*` in the repo `playtest` skill (plan 00 creates the skill). |

---

## 2. Scope & parity definition

### 2.1 In scope

1. **Desktop distribution (Linux/WSL host; Windows and Linux targets, macOS on macOS).** Committed Godot export presets; window defaults 900×700 maximized (upstream `SdlConfig`); window title/icon; app version metadata; launch-arg compatibility (`-debug`, `-width/-height`, `-maximized`, `-testMobile`, data-dir override, connect args); fullscreen/borderless switching; single-instance/file-launch behavior (`launchid.dat` failed-launch detection); crash handling (`CrashHandler` equivalent: report text, likely-mod detection, `crashes/` + `last_log.txt`); native file dialogs via `DisplayServer.file_dialog_show` with plan-14 fallback; drag-and-drop `.msav`/`.msch` import.
2. **Platform abstraction.** Extension of plan 05's `mind_core::platform::Platform` (sim-visible subset) with a Godot-facing `mind_gdext::platform::ClientPlatform` trait: file chooser, `openURI`, clipboard, share, mobile virtual keyboard, orientation forcing, hide, UUID, workshop stubs. Platform caps (`mobile`, `ios`, `android`, `headless`, `test_mobile`, locale, performance tier) are boot data consumed by `mind-core`/plans 14/15.
3. **Version/build system.** 1:1 port of `Version.java` (`type`, `number`, `modifier`, `commitHash`, `buildDate`, `build`, `revision`, `isSteam`, `buildString()`, `combined()`, `isAtLeast(...)`) over a generated `version.properties` with the upstream key set; `MIND_VERSION` from plan 00 stays the crate constant.
4. **Dedicated server.** A pure-Rust, Godot-free headless host (`mind-headless server`) that boots `mind-core` + `mind-stdb`, runs the console (stdin), the command socket (TCP), autosave/rotation, map rotation on game over, log files with rotation, `config/` layout and `rules.hjson`; maps the full 43-command `ServerControl` table with documented replacements; hosts/browses matches through plan 21's relay (D2).
5. **Mods on platform.** `data/mods/` (client) / `config/mods/` (server) discovery is plan 20; this plan guarantees the platform directories, server `config/assets/` data-asset root, export packaging of `assets/`, and the Workshop (OD4) seam.
6. **Steam/Discord seams (OD4).** `GameService` trait (achievements/stats), Discord RPC presence mapping with the exact upstream strings and app ID, Steam evaluation document; Workshop/lobbies/networking deferred with stubs (`Platform::workshop_maps` etc. referenced by plans 06/16/19/20).
7. **Mobile exports (OD5).** Android export preset + storage root + orientation/back-button/safe-area handling + performance-tier defaults + store-packaging notes (keystore/Gradle via Godot); iOS preset authored and documented (macOS-only build; share sheet via a minimal iOS plugin). Touch parity itself is plan 15.
8. **Save-file associations and import/export.** `.msav`/`.msch` associations (user-run scripts, not installer registry writes), `ClientLauncher::fileDropped`/`handleFileImport` port, `mindustry://host:port` URI handling (desktop protocol + Android intent), import/export via plan 14 `FileChooser` + plan 04 slot/map APIs.
9. **Updater disposition.** `BeControl` is **not ported** (JVM/bleeding-edge-specific); own release-feed check is a disabled stub. Web/HTML5 is **not a target** (§2.4).

### 2.2 Definition of done

- `cargo test -p mind-headless` covers version parsing, launch-arg parsing, server command parsing/config/rules/autosave/log-rotation/socket round-trips, and is Godot-free.
- `mind-headless server --config-dir tmp/srv --commands "config name Demo,config autosave true,host groundZero survival,runwave,status,save 0,exit"` exits 0, writes `settings.json`/`rules.hjson`/`saves/0.msav`/`logs/log-0.txt`, and a second boot loads slot 0.
- `tools/export.sh --platform windows` (or `godot_export export`) produces a launchable Windows executable (run under WSL via interop; writes `%APPDATA%\Mindustry-Godot\last_log.txt` on a Windows host, `~/.local/share/Mindustry-Godot/last_log.txt` when built/run on the Linux host), honors `--data-dir` and `-maximized false`, and imports a dropped/passed `.msav`.
- A dedicated server and a Godot client connect through the plan-21 relay, the client sees the match, and the server console reports the player (MCP scenario §7c).
- All §7d budgets measured and recorded; §7e checklist ticked with evidence in the Changelog.

### 2.3 Related systems explicitly out of scope (owner)

| Area | Owner |
|---|---|
| Widgets/dialogs/FileChooserDialog/safe-area margins/back-button dialog stack | `14_UI_IMPLEMENTATION_PLAN.md` |
| Touch gestures, mobile input controller, on-screen buttons, camera rig | `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` |
| Screenshots/minimap textures/map capture pixels, renderer method internals | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| Audio playback/mixing/buses | `18_AUDIO_IMPLEMENTATION_PLAN.md` |
| Mod discovery/new-content loading/patch application/server asset loaders | `20_MODS_IMPLEMENTATION_PLAN.md` |
| Match schema, relay ordering, host/join protocol, admin/ban/whitelist enforcement, chat transport, transport OD3 | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |
| Map editor/`MapIO` import pixels | `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` |
| Golden scenarios/perf suite/CI matrix | `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` |

### 2.4 Deliberate deviations (each with reason)

| # | Upstream | Port | Reason |
|---|---|---|---|
| P22-1 | Desktop client ships a JVM (`Mindustry.jar`/Packr); dedicated server is `server-release.jar` with `java -jar`. | Single Godot executable per desktop platform; dedicated server is the Rust `mind-headless server` binary. | D1 (pure Rust), D7; no JVM. |
| P22-2 | SDL/OpenGL version negotiation list (`-gl`, `-coreGl`, `-compatibilityGl`, Intel GPU fallback). | Not ported; Godot owns the backend. Args are accepted and ignored with one `[W]` line so launchers do not break. | 16 D16-6; engine constraint. |
| P22-3 | `BeControl` bleeding-edge updater (GitHub releases, self-replace, `autoUpdate`). | Not ported. `config autoUpdate` is retained but only logs "updater not available in this build"; `mind-gdext::platform::update` is a disabled seam. | No JVM artifacts; Steam/own-release channels own updates (OD4). |
| P22-4 | `js <script>` console command runs Rhino. | Command kept; dispatches to plan 20's script hook (OD1) when present, otherwise returns a clear "script mods unavailable" message. | OD1; no JVM. |
| P22-5 | `gc` triggers `System.gc()`. | Retained as a no-op diagnostic that prints RSS/heap stats. | No GC in Rust; parity of the "testing only" command surface. |
| P22-6 | `settings.bin` (Arc JSON) holds server config + admin/ban/whitelist. | `config/settings.json` (04 `SettingsStore`, exact `Config` key names) holds local config; admin/ban/whitelist authority lives in plan 21's STDB tables with `config/{admins,bans,whitelist}.json` as the offline-mode mirror. | D2 + 04; STDB is the durable multiplayer state. |
| P22-7 | Mobile data root = Android `getExternalFilesDir` / iOS Documents; one-time `files_moved` migration. | Godot `user://` (Android internal app files, iOS Documents-equivalent sandbox) + SAF/document-picker import/export; no migration. | Scoped storage robustness; Godot's supported path; user-visible files still reachable via SAF. |
| P22-8 | `Platform.shareFile` iOS `UIActivityViewController`; Android no-op. | iOS share via a small Godot iOS plugin (objc) documented in `tools/ios/`; Android uses SAF create-document. | Godot has no share API; minimal plugin is the seam. |
| P22-9 | Save/game URI scheme `mindustry://`. | Registers both `mindustry://` (parity) and `mindustry-godot://` (disambiguation from Java installs); `.msav`/`.msch` associations are opt-in scripts. | Avoids hijacking Java Mindustry on the same machine. |
| P22-10 | Electron-like single-instance not used; multiple instances allowed, `launchid.dat` only detects failed launches. | Same: no single-instance enforcement; `checkLaunch`/`finishLaunch` ported verbatim; a second launch via file association is a second instance (parity). | `Vars.checkLaunch` semantics. |
| P22-11 | Upstream ships `icons/icon_64.png` + platform icon sets. | Godot export icons (`icon.svg` project icon + per-preset `.ico`/`.icns`/adaptive Android icons) generated from plan-03 migrated icon assets. | Godot pipeline. |

---

## 3. Target design

All names below are final unless marked otherwise. `mind-core` stays Godot-free and tokio-free (D1). `mind-headless` (and its new `server` mode) links no Godot.

### 3.1 Repository additions (per HLP §2.1)

```
mindustry-godot/
  client/
    export_presets.cfg                    # committed: Windows Desktop / Linux / macOS / Android / iOS presets
    assets/
      version.properties                  # GENERATED (gitignored), upstream key set
      icon.ico  icon.icns                 # generated from 03 migrated icons (tools/version + export)
    rust/
      mind-core/src/platform/
        caps.rs                           # PlatformCaps, PlatformKind, PerformanceTier
      mind-core/src/version.rs            # extended: BuildInfo (plan 00 keeps MIND_VERSION)
      mind-gdext/src/platform/
        mod.rs                            # MindPlatform autoload + #[func] probes
        args.rs                           # launch-arg parser (upstream flags + --data-dir)
        desktop.rs                        # window/fullscreen, launchid, file drop, fatal dialog, single-exe glue
        dialogs.rs                        # DisplayServer.file_dialog_show -> FileChooserParams bridge
        mobile.rs                         # orientation, back button, safe area push, hide, text input
        uri.rs                            # openURI / clipboard / protocol handlers
        service.rs                        # GameService trait, NullService
        discord.rs                        # optional Discord RPC presence (compile-gated)
        crash.rs                          # crash report + likely-mod detection + crashes/
        update.rs                         # own-feed stub (disabled)
        workshop.rs                       # Platform::workshop_* empty stubs (OD4 seam)
      mind-headless/src/
        lib.rs                            # NEW lib target re-exporting modules (binary keeps main.rs)
        server/
          mod.rs                          # server boot (port of ServerLauncher.init)
          config.rs                       # ServerConfig (34 Config keys) over 04 SettingsStore
          console.rs                      # CommandHandler port, Levenshtein suggestion, `yes`
          commands.rs                     # the 43 commands
          autosave.rs                     # cadence + rotation naming + autosavebe
          logs.rs                         # log file N rotation + socket fan-out
          rules_file.rs                   # rules.hjson load/save (04 JsonIO)
          socket.rs                       # TCP console socket (std::net, line-based)
          host.rs                         # HostControl trait -> plan 21 host/join/status
  tools/
    export.sh  export.ps1                 # export wrappers (platforms, clean, verify)
    version.sh  version.ps1               # generate version.properties from env/git
    associate.sh  associate.ps1           # opt-in .msav/.msch + mindustry:// registration
    server.sh  server.ps1                 # run mind-headless server with a config dir
    adb-smoke.sh  adb-smoke.ps1           # optional Android touch smoke (device required)
  docs/platform/
    steam-parity.md                       # OD4 evaluation: what full Steam parity requires
    discord.md                            # optional RPC plugin evaluation + presence mapping
    store-packaging.md                    # Godot gradle/Xcode signing/stores
    web.md                                # HTML5: not a target
```

New Cargo additions: `mind-headless` gains `[lib] name = "mind_headless"` (additive; binary `main.rs` keeps working); `mind-core/src/platform/caps.rs` and the extended `version.rs` are additive files in a plan-00/05 crate (orchestrator note in §8 R1). No new crates.

### 3.2 `mind_core::version` — `Version.java` port

```rust
// mind-core/src/version.rs (extends plan 00's constants module; plan 00 keeps MIND_VERSION)
pub struct BuildInfo {
    pub r#type: String,        // Version.type       ("unknown", "official", "bleeding-edge", ...)
    pub modifier: String,      // Version.modifier   ("unknown", "release", "steam", ...)
    pub commit_hash: String,   // Version.commitHash
    pub build_date: String,    // Version.buildDate
    pub number: u32,           // Version.number
    pub build: i32,            // Version.build      (-1 = custom)
    pub revision: u32,         // Version.revision
    pub is_steam: bool,        // modifier.contains("steam")  (Version.isSteam)
    pub enabled: bool,         // Version.enabled
}
impl BuildInfo {
    pub fn init_from(src: &str) -> Result<Self, VersionError>;   // PropertiesUtils.load port
    pub fn embedded() -> &'static BuildInfo;                     // include_str!(asset path), OnceLock parse
    pub fn is_at_least(&self, s: &str) -> bool;                  // Version.isAtLeast(String)
    pub fn is_at_least_nums(build: i32, revision: u32, s: &str) -> bool;
    pub fn build_string(&self) -> String;                        // "custom" | "build" or "build.revision"
    pub fn combined(&self) -> String;                            // menu string incl. commit hash
}
```

- `assets/version.properties` uses the upstream keys verbatim (`type`, `number`, `modifier`, `commitHash`, `buildDate`, `build`); values come from `MIND_BUILD_*` env vars set by `tools/version.sh` + CI (git commit/date), with `build=-1` when absent (custom build → version checks disabled, matching upstream).
- `tools/version.sh` accepts `--build-number`, `--revision`, `--type`, `--modifier`, `--commit-hash`, `--build-date` (`.ps1` twin uses `-BuildNumber` …); writes `client/assets/version.properties` (LF) and exports the same values for `Option<&str>`-embedded use in the server binary.
- `mind-headless version [--json]` prints `{type, modifier, commitHash, buildDate, number, build, revision, buildString, combined}` (the server startup line uses `[Mindustry] Version: <buildString>`).
- `BuildInfo` is boot data; the sim never reads wall clock. `combined()` is used by UI (14) and crash reports.

### 3.3 Platform caps + client platform trait

```rust
// mind-core/src/platform/caps.rs
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformKind { Desktop, Android, Ios, Headless }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PerformanceTier { Low, Medium, High }
#[derive(Clone, Debug)]
pub struct PlatformCaps {
    pub kind: PlatformKind,
    pub headless: bool,
    pub mobile: bool,          // Vars.mobile
    pub test_mobile: bool,     // Vars.testMobile (-testMobile / --mobile-preview)
    pub locale: String,        // Core.settings "locale" or OS default
    pub locale_default: String,
    pub tier: PerformanceTier, // graphics/particle defaults (16/17/18 consume)
    pub steam: bool,           // Vars.steam (false until OD4)
}
```

```rust
// mind-gdext/src/platform/mod.rs — Godot-facing, main-thread only (D1; 14 requested these names verbatim)
pub trait ClientPlatform {
    fn show_file_chooser(&mut self, params: FileChooserParams);        // 14 §3.11
    fn open_uri(&mut self, uri: &str);                                 // Menus.openURI
    fn set_clipboard(&mut self, text: &str);
    fn get_clipboard(&mut self) -> Option<String>;
    fn text_input(&mut self, params: TextInputParams);                 // 14 §3.11 (mobile IME / desktop dialog)
    fn share_file(&mut self, path: &Path);
    fn begin_force_landscape(&mut self);
    fn end_force_landscape(&mut self);
    fn hide(&mut self);                                                // Android moveTaskToBack
    fn uuid(&mut self) -> String;                                      // Platform.getUUID (8-byte base64)
    fn workshop_maps(&mut self) -> Vec<PathBuf>;                       // OD4 seam (06 §3 references this name)
    fn workshop_schematics(&mut self) -> Vec<PathBuf>;
    fn workshop_mods(&mut self) -> Vec<PathBuf>;
}
```

`MindPlatform` (gdext `Node`, appended at `/root/Spine/MindPlatform`) owns the concrete `DesktopPlatform`/`MobilePlatform` implementation, the caps struct, and the service registry. Plan 05's `mind_core::platform::Platform` is implemented by `mind_gdext::platform::GdPlatform` (client) and by `HeadlessPlatform` (headless/server); this plan does not change the trait shape except the additive `autosave_allowed()` reconciliation in §8 R3.

`#[func]` MCP probes (append-only, plan 00 §3.10 rule 4): `get_build_info() -> Dictionary`, `get_platform_caps() -> Dictionary`, `window_state() -> Dictionary`, `set_fullscreen(bool)`, `dev_file_drop(path) -> bool`, `dev_open_uri(uri) -> bool`, `dev_set_test_mobile(bool)`.

### 3.4 Desktop launch, window and args

**Window defaults (parity):** `project.godot` gets `display/window/size/viewport_width=900`, `viewport_height=700`, `mode=2` (maximized) — plan 00 sets the stretch; this plan adds size/mode and keeps `canvas_items`/`expand`. Window title = project name `Mindustry-Godot` (deviation: upstream title is "Mindustry"; documented, configurable via `--title`). `-maximized false` → windowed restore; F11 / `toggle_fullscreen` binding uses `DisplayServer.window_set_mode(FULLSCREEN)` (borderless windowed toggle is a settings option owned by 14).

**Arg parser (`args.rs`)** runs over `OS.get_cmdline_args()` (Godot-recognized args are skipped by an explicit allowlist); unknown tokens keep Mindustry's single-dash convention and preserve token order for `+connect_lobby`:

| Upstream flag | Port behavior |
|---|---|
| `-width N` / `-height N` | `DisplayServer.window_set_size(Vector2i(N, h))` at boot; `0`/invalid → default. |
| `-maximized true\|false` | window mode maximize/windowed. |
| `-debug` | log level `debug` (mirrors `Log.level = debug`); also enables the in-game debug overlay flag if 14/dev mode is on. |
| `-testMobile` (alias `--mobile-preview` from 14 §2.4) | `PlatformCaps.test_mobile = true`. |
| `-gl`, `-coreGl`, `-compatibilityGl`, `-antialias`, `-gltrace` | accepted, logged once at `[W]` as unsupported, ignored. |
| `-data-dir <path>` / `MINDUSTRY_DATA_DIR` / `MINDUSTRY_GODOT_DATA_DIR` | data root override (plan 00 OD-R9; §6.1 precedence). |
| `+connect_lobby <id>` | parsed and stored; consumed by the OD4 Steam seam only (never acts without Steam). |
| `-connect <host[:port]>` (new, our shim) | boot into the join flow (plan 21) after `ClientLoadEvent`; used by server/client smoke tools. |
| `--headless` | not a client arg; Godot headless export ignores it (server is `mind-headless`). |

**Failed-launch detection:** `Vars.checkLaunch`/`finishLaunch` ported to `mind-gdext::platform::desktop::{check_launch, finish_launch}` writing `<data-root>/launchid.dat` at boot and deleting it after a successful `ClientLoadEvent`; `lastBuild`/`lastBuildString` settings written on success. A leftover file ⇒ log a "previous launch may have crashed" warning (upstream silently sets `failedToLaunch`; the crash report path is the actionable surface).

**Crash handling (`crash.rs` + `mind_core::crash`):** panic hook + Godot `OS.alert`/`DisplayServer.dialog_show` fatal dialog; report text mirrors `CrashHandler.createReport` fields (build `combined()`, date, OS/arch, GPU/renderer via `RenderingServer.get_video_adapter_name`, RAM, cores, mods list, likely mod), saved to `<data-root>/crashes/crash-report-<MM_dd_yyyy_HH_mm_ss>.txt`; `log()` variant for the Android-style uncaught path (`OS.get_user_data_dir()` fallback). **Likely-mod detection** ports `getModCause`: stack/backtrace frames whose crate/file belongs to a loaded mod (registry from plan 20 exposes `LoadedMod { name, display_name, version, files: Vec<PathBuf>, main: Option<String> }`); non-vanilla prefix check is `mindcore|mind_|std|core|alloc` instead of `mindustry|arc|java`. Settings are force-saved before exit; network disposed through plan 21's API.

**Single-instance / file-drop:** drag-and-drop uses Godot's `DisplayServer` files-dropped signal (Windows/macOS/Linux) → `handle_file_import(path)`; a file path passed as a bare argv (`Open with` / association) is treated identically at boot; `ClientLauncher.fileDropped` port accepts only `.msav`/`.msch` (upstream rule) and routes: schematic → `Schematics.import_and_show` (12/14), map → `Maps.try_import_map` (06/19), save → `SaveSlot::import_file` + `LoadDialog.load_save` (04/14). Mobile drops are ignored (upstream). No second-instance forwarding (P22-10).

### 3.5 Native file dialogs

`dialogs.rs` bridges plan 14's `FileChooserParams` (`open`, `allow_multiple`, `title`, `file_name`, `extensions`, single/multi handlers):

- **Desktop:** `DisplayServer.file_dialog_show(title, current_dir, file_name, show_hidden, mode, filters, callback)` with the 4.7 Callable API (async). `last_directory` tracking lives in 14's `FileChooserDialog`; this plan reads/writes it through the 14 API. Save mode appends the first extension when missing (upstream `showFileChooser` fix), macOS single-extension quirk not ported (Godot handles patterns).
- **Fallback:** if `DisplayServer` reports the feature unavailable, call `FileChooser.showFallbackFileChooser` (14). Android SAF uses the same DisplayServer call; iOS uses the document browser plugin (§3.8).
- Native dialogs must never block the main loop: the callback posts back to the main thread and calls `FileChooserParams::handle_choose_result` in `Core.app.post`-equivalent order. Sandbox paths (macOS) are handled by the callback's native paths.

### 3.6 Dedicated server — `mind-headless server`

**Decision (default, `NEEDS USER DECISION` OD-P1):** the dedicated server is the **pure Rust `mind-headless server` mode**, not a Godot headless export. Justification: `mind-core` is the entire sim and is Godot-free/tokio-free by locked rule (D1/§2.2), so a Godot headless export adds a ~100 MB engine dependency, a GDExtension build, and a display-less Vulkan/GL context risk for zero gameplay capability; `mind-headless` is already the CI oracle (D4/D5) and can run with no GPU in any container; the server must be scriptable headlessly for plan 23. A Godot headless scene (`scenes/headless_server.tscn`) is explicitly **not** part of the server path; it exists nowhere.

Boot (port of `ServerLauncher.init`):

1. `--config-dir` (default `./config`, upstream `Core.files.local("config")`) becomes the data root; `headless = true`, locales skipped (UI-less; only the icon/bundle keys needed by server messages are loaded by 03's headless path).
2. `Vars.loadSettings` → 04 `SettingsStore` at `<config>/settings.json`; defaults registered with the exact `Config` key names + defaults from §6.3.
3. Content (02) → mods (20) → content init → map registry (06/19) → bases (12). Content errors print `| &ly[@]`-style lines and exit code 1 (upstream behavior).
4. Listeners in order: `AsyncCore.begin` (05 phase), `Logic`, `NetServer` equivalent (21 host side), `ServerControl` port, `AsyncCore.end`; then `mods.each_class(Mod::init)`; `ServerLoadEvent`.
5. `ServerControl::setup` task order: auto-update save load (disabled, P22-3) → argv `commands` joined by spaces split on `,` → `config startCommands` split on `,` → startup warnings → `rules.hjson` bootstrap → server data assets (`config/assets/`, plan 20) → events (`GameOver` rotation, `WorldLoad` autosave reset, `Trigger.update` autosave/auto-pause, `PlayEvent` rules apply, `ResetEvent` auto-pause clear) → mod commands → socket start → console thread on `ServerLoadEvent`.

**Console (`console.rs`).** `CommandHandler` port: empty prefix, `register(name, param_text, description, handler)`, `<required>`/`[optional]`/`...` parsing, response kinds `valid|unknownCommand|fewArguments|manyArguments`; commands are posted from the stdin thread to the tick owner and executed between ticks (never mid-tick). No JLine: a `std::io::stdin().lock().lines()` reader with a `> ` prompt and history is out of scope; line editing is the terminal's job (documented deviation). `mods.each_class(register_server_commands)` runs last (plan 20).

**Command socket (`socket.rs`).** `std::net::TcpListener` bound to `socketInputAddress:socketInputPort` (default `localhost:6859`), toggled by `config socketInput`; single active connection at a time (upstream); each received line runs through the same `handle_command_string`; all formatted log lines are echoed to the socket (colors stripped, upstream `socketOutput`). `config socketInput*` changes re-bind (upstream `Trigger.socketConfigChanged` → direct `toggle_socket(false); toggle_socket(on)` call in the config setter).

### 3.7 Server command surface (port of `ServerControl.registerCommands`)

43 commands. "Local" = implemented in `commands.rs` against `mind-core`/04/12; "21" = delegates to the plan-21 `Admins`/host API and STDB views; "OD1" = script hook.

| Command | Behavior / owner |
|---|---|
| `help [command]` | list / detail; param text and descriptions ported. Local. |
| `version` | `BuildInfo.combined()` + `Vars.headless` server suffix. Local. |
| `exit` | force full exit (`platform.exit()`), no save. Local. |
| `stop` | stop hosting: kick all (`KickReason::ServerClosed`), `state = Menu`, close relay/host. 21. |
| `host [mapname] [mode]` | `logic.reset()` → map lookup (`maps` registry, default random survival) → `world.load_map` → `logic.play()` → host/open through 21 (`open_server` equivalent); mode ∈ `survival|sandbox|attack|pvp|editor` (`Gamemode`). Local + 21. |
| `maps [all/custom/default]` | list registry with `author/width/height` columns. Local (12/19 registry). |
| `reloadassets` | re-run 20 data-asset load from `config/assets/`. 20. |
| `reloadmaps` | re-scan maps dir (06 `Maps::reload`). Local. |
| `status` | state, map, mode, wave, players, uptime, tick, FPS-ish tick budget. Local + 21 view. |
| `mods` / `mod <name...>` | list loaded mods / details (plan 20 registry). 20. |
| `js <script...>` | OD1 hook; otherwise `[E] script mods are not available in this build`. OD1. |
| `say <message...>` | relay server message to all players. 21. |
| `pause <on/off>` | `state.paused` toggle (host-side). Local (05 state). |
| `rules [remove/add] [name] [value...]` | list / edit `rules.hjson` in-memory + save; values parsed as JSON/HJSON via 04 `JsonIO` (12 `Rules`). Local. |
| `dumpsettings` | print every setting key/value (sorted). Local (04 store). |
| `fillitems [team]` | fill all cores for team (12/08 API). Local. |
| `playerlimit [off/number]` | `Config.playerlimit` (added from `NetServer` limits) → 21 connection gate. 21. |
| `config [name] [value...]` | get/set the 34 `Config` keys; coercion + description + validation; triggers side effects (port, debug log level, socket, autoUpdate no-op). Local (persistence) + 21 (enforcement). |
| `subnet-ban [add/remove] [address]` | 21 admin store. 21. |
| `name-ban [add/remove/clear] [regex]` | 21. |
| `whitelist [add/remove] [ID]` | 21 (+ `Config.whitelist` toggle). |
| `shuffle [none/all/custom/builtin]` | `maps.set_shuffle_mode`; next-map selection. Local (12/19). |
| `nextmap <mapname...>` | override next game-over map. Local. |
| `kick <username...>` | 21. |
| `ban [type-id/name/ip] <username/IP/ID...>` / `bans` / `unban <ip/ID>` / `pardon <ID>` | 21 admin store + `config/{bans,whitelist}.json` mirror. |
| `admin <add/remove> <username/ID...>` / `admins` | 21 (STDB authority). |
| `players` | 21 live view (name/id/IP/team/locale/mobile/modded). |
| `runwave` | force next wave (11 `WaveSpawner`). Local. |
| `loadautosave` | newest `saves/auto_*.msav` → 04 load + play (or `autosavebe` if present). Local. |
| `load <slot>` / `save <slot> [embedAssets]` / `saves` | 04 `SaveIo`/`SaveSlot`; `embedAssets` flag ported. Local. |
| `gameover` | force `GameOverEvent` path; triggers rotation/countdown (`roundExtraTime`). Local. |
| `info <IP/UUID/name...>` / `search <name...>` | 21 player DB; offline mirror scan fallback. |
| `gc` | no-op; prints allocator memory stats. Local (P22-5). |
| `yes` | re-run the suggested command from the last unknown-command response (Levenshtein < 3, `Strings.levenshtein` port). Local. |
| `dos-ban [add/remove] [ip]` | 21 admin store. 21. |

Responses/log lines match upstream formatting (`[D]/[I]/[W]/[E]` tags, `MM-dd-yyyy HH:mm:ss`, `&ly`→stripped), reused from plan 00's `MindLogger`.

### 3.8 Mobile (OD5; Android verified, iOS authored)

- **Android preset** (`export_presets.cfg`): package `io.anuke.mindustry`-equivalent `com.mindustrygodot.game` (default; flag), min SDK 21/target 36 notes documented, `screen/immersive_mode=true`, `screen/support_small/normal/large/xlarge=true`, orientation `sensor_landscape`, `rendering_method` inherits the project `mobile` renderer (OD-P4 resolved by NUD-06=B), keystore fields left empty (release signing via `--keystore` args/env, never committed). `config/features` keeps `"Mobile"` (project.godot already has it; plan 00 keeps/removes per §3.4 and 22 re-adds for this export).
- **Lifecycle:** `NOTIFICATION_APPLICATION_PAUSED/RESUMED` → 05 pause gate + 18 audio duck/pause + finish-launch behavior (`ClientLauncher.pause` mobile semantics: mark launch finished when backgrounded, P22 parity); `NOTIFICATION_WM_GO_BACK_REQUEST` → `MindUi.close_top_dialog()` (14) if any dialog, else `platform.hide()`; `application/config/quit_on_go_back=false`.
- **Orientation:** export-level sensor-landscape for gameplay. `beginForceLandscape`/`endForceLandscape` (editor) map to a native plugin call if present, else a no-op + the 14 portrait warning surface; deviation documented (editor is desktop-first).
- **Text/IME:** `text_input` uses Godot's virtual keyboard (`LineEdit`/`TextEdit` `virtual_keyboard_type`) driven by 14's `show_text_input`; mobile must use this path (upstream rule).
- **Safe areas:** 14's `DisplayServer.get_display_safe_area()` margins; plan 22 pushes `notch`-style insets via `size_changed` if Godot reports them.
- **Performance tiers:** `PerformanceTier` chosen from `OS.get_processor_count`/GPU class at boot, defaulting Android to `Low` (halved particle budgets in 17, `pixelate` on in 16, smaller FX/preview caps from 12/19); exposed as settings overrides (14).
- **iOS:** preset authored (`Info.plist` orientation landscape + all, document types `public.data` for `.msav`/`.msch`, `mindustry://` URL scheme); build/sign requires macOS + Xcode (cannot be verified on this machine). `share_file`/`show_file_chooser(save)` use the small `tools/ios/` plugin (UIDocumentBrowserViewController/UIActivityViewController ports); if the plugin is absent, fall back to writing into Documents + `OS.shell_open`. Store packaging notes in `docs/platform/store-packaging.md`.
- **Data root:** `user://` on both (P22-7), with `data/mods/`, `saves/`, `schematics/`, `maps/`, `screenshots/`, `crashes/`, `last_log.txt` under it.

### 3.9 Services: GameService / Discord / Steam (OD4)

```rust
// mind-gdext/src/service.rs
pub trait GameService: Send {
    fn enabled(&self) -> bool { false }
    fn complete_achievement(&mut self, _name: &str) {}
    fn clear_achievement(&mut self, _name: &str) {}
    fn is_achieved(&self, _name: &str) -> bool { false }
    fn get_stat(&self, _name: &str, _def: i32) -> i32 { _def }
    fn set_stat(&mut self, _name: &str, _amount: i32) {}
    fn store_stats(&mut self) {}
}
pub struct NullService; impl GameService for NullService {}
```

- `achievements`/stats call sites live in plan 12/19; they call `MindPlatform.service()` and are no-ops until OD4 lands. The trait shape is the full `DesktopLauncher` anonymous `GameService` (lines 218–256).
- **Discord RPC** (`discord.rs`, compile-gated `discord` feature, default off): presence mapping ported exactly (in-game: `state = mode + " | N Players"`, `details = map + " | Wave N"`, `largeImageKey = "logo"`, `largeImageText = "Wave N"`; out-of-game: `state = "In Editor" / "In Launch Selection" / "In Menu"`), app ID `610508934456934412`, `nodiscord` env opt-out, update throttle on the same cadence as upstream's per-frame call. Implementation requires a Discord IPC GDExtension/addon; evaluate `docs/platform/discord.md`, record license in `THIRD_PARTY_NOTICES.md` if adopted.
- **Steam** (deferred, seam only): `SVars` app ID `1127400`, `SNet` (Steam Networking Sockets + lobbies), `SWorkshop` (UGC), `SUser`/`SStats` (user/achievements), `getNet()`, `getUUID()` from account ID, `+connect_lobby`, rich presence (`steam_display=#steam_status_raw`, `steam_status` = map/UI string), `allowCustomClients`. `docs/platform/steam-parity.md` records exactly what full parity needs: the Steamworks SDK via a native GDExtension (e.g. GodotSteam), Steam identity → STDB identity linking, workshop publish/subscribe mapped onto plan 20 mod/map/schematic loading, lobbies mapped onto plan 21 matches, and (if chosen) UDP transport replacing STDB relay per OD3. `workshop.rs` returns empty vectors now so plans 06/16/19/20 compile against the final seam.

### 3.10 Updater, BE and web

- `update.rs`: disabled stub; `config autoUpdate` prints a one-line notice; no network calls at boot. If the project later ships its own launcher/updater, it is a new plan.
- Web/HTML5: **not a parity target** (`docs/platform/web.md` states why: no native file dialogs/drag-drop, STDB websocket + threading constraints, determinism/perf unproven, and ODs 1/3/4 unresolved). Godot can export web; it is explicitly out of scope unless the user changes scope.

### 3.11 STDB touchpoints

Plan 22 adds **no tables/reducers/views** (plan 21 owns the schema; D2 unchanged). It consumes:

| Interface | Use |
|---|---|
| `mind-stdb::Connector` + `tokens::FileTokenStore` (path `<config>/identity/`) | Dedicated server identity; `--pN` suffix not used server-side. |
| `21` host API (`HostController::open_server/close_server`, `MatchStatus`, `status()`, `players()`) assumed in §3.12 | `host`, `stop`, `status`, `players`, `playerlimit`. |
| `21` `Admins` API (`kick`, `ban`, `unban`, `pardon`, `admin_add/remove`, `whitelist_add/remove`, `subnet_ban`, `name_ban`, `dos_ban`, `search`, `info`) assumed | Admin commands + `config/{admins,bans,whitelist}.json` offline mirror. |
| `relay_config`/`audit_log` rows (01) | Server config audit (`logCommands`) and connection audits. |

### 3.12 Sibling reconciliation (by filename)

| Sibling | Interface this plan assumes/extends | State |
|---|---|---|
| `00_FOUNDATION_IMPLEMENTATION_PLAN.md` | Reuses `mind-headless` as the server (00 §3.10 says "22 reuses `mind-headless`"); adds a lib target + `server` mode (orchestrator note R1); uses `MindLogger`/`last_log.txt`, `--data-dir`, data dir `~/.local/share/Mindustry-Godot/` (OD-R9), MCP rig/node-path conventions. | On disk. |
| `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` | `Connector`, `TokenStore`, `relay_config`, `audit_log`; server connects like a headless peer. | On disk. |
| `03_ASSETS_IMPLEMENTATION_PLAN.md` | Packed `assets/` tree is what exports ship; `version.properties` is regenerated here (03 trim list excludes upstream's); headless server loads only the minimal bundle/icon path. | On disk. |
| `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` | `FileSystem`/`Paths`, `SettingsStore`, `SaveIo`/`SaveSlot::{import_file,export_file}`, `rules.hjson` via `JsonIO`; `.msav` extension stays; association opens of Java saves surface the OD2 "Unknown save version" message. **Data-root conflict:** 00 OD-R9 (XDG app-data) vs 04 R3 (`./data` portable, NEEDS USER DECISION) — §6.1 resolves per host and is flagged R4. | On disk. |
| `14_UI_IMPLEMENTATION_PLAN.md` | Exact hook names `platform.show_file_chooser/open_uri/set_clipboard/text_input`; `FileChooserParams` lifecycle; `MindUi.close_top_dialog()` back button; safe-area margins owned by 14; `is_mobile()` source; Discord dialog link via `open_uri`. | On disk. |
| `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` | Touch parity is 15's; 22 wires orientation/back/IME/runtime mobile flag; `Phantom Camera` untouched; `-testMobile`/`--mobile-preview` accepted. | On disk. |
| `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` | `capture_map(path)`/screenshot APIs consumed by FileChooser export; OD-R2 renderer method inherited on desktop; mobile export method override is OD-P4. | On disk. |
| `18_AUDIO_IMPLEMENTATION_PLAN.md` | **Not on disk.** Assumed: audio settings/bus persistence keys (14/04 own the dialog/store), `NOTIFICATION_APPLICATION_PAUSED` audio focus handling is 18's, this plan only forwards the notification; mobile tier selects 18's low-quality defaults. | Missing → reconcile. |
| `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` | **Not on disk.** Assumed host API (§3.11) + `Admins`; transport OD3 unchanged (STDB relay); dedicated server is a headless host peer, not an STDB module sim (D2 says authoritative server sim deferred). | Missing → reconcile. |
| `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` | `SAVE_EXTENSION="msav"`/`SCHEMATIC_EXTENSION="msch"`; autosave policy hook `platform.autosave_allowed()` (also 05 conflict, R3). | On disk. |
| `06`/`19`/`20` | `Platform::workshop_*` stubs; mod dirs/data assets; map/schematic import APIs. | On disk (06/20 not present in the listing at authoring were per-06's own header; current set includes 06). |

---

## 4. Port map

| Mindustry source | Target | Notes on adaptation |
|---|---|---|
| `desktop/src/mindustry/desktop/DesktopLauncher.java` | `client/export_presets.cfg` + `mind-gdext/src/platform/{args,desktop}.rs` | SDL `SdlConfig` → Godot project/export settings; GL version list/Intel checks dropped (P22-2); arg cases ported 1:1 with mapping table §3.4; Discord/Steam blocks → §3.9 seams. |
| `desktop/.../ErrorDialog.java` | `mind-gdext::platform::desktop::fatal_dialog` | Godot `OS.alert`/dialog; stderr fallback. |
| `desktop/.../steam/SVars|SNet|SUser|SStats|SWorkshop` | `mind-gdext/src/service/*` (`GameService`, `workshop.rs`) + `docs/platform/steam-parity.md` | Deferred OD4; trait + empty vectors + documented parity requirements; app ID `1127400` kept as a constant. |
| `android/src/mindustry/android/AndroidLauncher.java` | Android export preset + `mind-gdext/src/platform/mobile.rs` | Activity lifecycle → Godot notifications; SAF file dialog via `DisplayServer`; external-storage deviation P22-7; orientation/back/ime mapping §3.8. |
| `android/.../AndroidRhinoContext.java` | deleted | No JVM/DEX; OD1. |
| `ios/src/mindustry/ios/IOSLauncher.java` | iOS export preset + `mind-gdext/src/platform/mobile.rs` + `tools/ios/` plugin | macOS-only build; document browser/share sheet plugin; forced-landscape editor branch documented as deviation. |
| `core/src/mindustry/core/Platform.java` | plan 05 `mind_core::platform::Platform` + `mind_gdext::platform::ClientPlatform` | Sim-visible subset stays in core (05); Godot-facing methods in gdext with 14's exact names; `getUUID` → 04 settings key `uuid` + 8-byte base64 port. |
| `core/src/mindustry/core/Version.java` | `mind_core::version::BuildInfo` + `assets/version.properties` + `tools/version.*` | Full parse incl. `build.revision`, `isAtLeast`, `buildString`, `combined`; `isSteam` from modifier. |
| `server/src/mindustry/server/ServerLauncher.java` | `mind_headless::server::boot` (`mind-headless server`) | Pure Rust; `config/` data root; no renderer/UI/control modules ever constructed. |
| `server/src/mindustry/server/ServerControl.java` | `mind-headless/src/server/{console,commands,config,autosave,logs,rules_file,socket,host}.rs` | 43 commands (§3.7); 34 `Config` keys (§6.3); autosave naming/rotation; `log-N.txt` rotation; socket `localhost:6859`. |
| `core/src/mindustry/net/CrashHandler.java` | `mind-gdext::platform::crash` + `mind_core::crash` report builder | Report fields ported; likely-mod detection uses plan-20 registry; `crashes/` path. |
| `core/src/mindustry/net/BeControl.java` | not ported; `mind-gdext::platform::update` disabled stub | P22-3. |
| `core/src/mindustry/ClientLauncher.java` (`dataDir`, `fileDropped`, `handleFileImport`, `setup`) | `mind_gdext::platform::{args::data_dir, desktop::handle_file_import, file_drop}` + 04/06/12/19 APIs | `.msav`/`.msch` only; URI scheme path for `mindustry://`. |
| `core/src/mindustry/ui/FileChooser.java` | 14's `FileChooser` API + `mind-gdext/src/platform/dialogs.rs` | Native-first, 14 fallback; macOS pattern quirk dropped. |
| `core/src/mindustry/Vars.java` (dirs, `checkLaunch`, `loadLogger`, mobile flags) | 04 `Paths`/`FileSystem` + 22 §6.1 + `platform.rs` | `screenshots/ maps/ previews/ saves/ tmp/ mods/ assetCache/ schematics/` kept as subdir names. |
| `core/src/mindustry/ui/Menus.java:369` (`openURI`) | `ClientPlatform::open_uri` | `OS.shell_open`; Discord link uses this. |
| `core/assets/version.properties` (generated) | `client/assets/version.properties` + `mind-tools version gen` | Same keys/format. |
| Android manifest / iOS `Info.plist` | Godot export preset fields + `docs/platform/store-packaging.md` | Permissions/intents/document types/orientations; `mindustry://` + `.msav`/`.msch` document types. |
| `fastlane/` | not ported | Store metadata notes only. |

---

## 5. Milestones & task breakdown

Each milestone is independently verifiable; record evidence in the Changelog. The smallest vertical slice is M0 (headless version/args/tests) → M2 (headless server console) because both are pure Rust and fully automated; M1 can proceed in parallel once the MCP rig is connected.

### M0 — Platform foundation (headless, no Godot)

> **Status: ✅ COMPLETE 2026-10-02.** Implementation delta vs this plan: the pure `args` parser lives in `mind_core::platform::args` (Godot-free, shared by client + harness) instead of only `mind-gdext`, and `BuildInfo::embedded()` is generated by `mind-core/build.rs` from `client/assets/version.properties` (defaults when absent) rather than a raw `include_str!` of a gitignored file — both are required so `cargo test -p mind-core` stays Godot-free and a plain clone compiles. See the Changelog for evidence.

- `version.rs`: `BuildInfo` + `PropertiesUtils`-style parser + `is_at_least`/`build_string`/`combined`; `tools/version.sh|.ps1`; `mind-headless version [--json]`.
- `caps.rs`: `PlatformCaps`/`PlatformKind`/`PerformanceTier`.
- `args.rs`: flag parser (pure function over `&[String]` → `LaunchArgs { width, height, maximized, debug, test_mobile, data_dir, connect, connect_lobby, ignored_gl }`).
- `mind-gdext/src/platform/mod.rs` skeleton + `MindPlatform` registration (compiles, no behavior yet).
- Tests §7a.

**Verify:** `cargo test -p mind-headless -p mind-core version args`; `cargo run -p mind-headless -- version --json` matches the golden; `cargo tree -p mind-core | rg "godot|tokio"` empty.

### M1 — Desktop export (Windows preset, built from the WSL host)

- `export_presets.cfg` (Windows Desktop preset, `Mindustry-Godot.exe`, icon `.ico`, console wrapper `export.sh`), `tools/export.sh|.ps1` (`--platform windows --target release --clean --verify`).
- Window size/mode/title in `project.godot`; fullscreen toggle API; `-width/-height/-maximized/-debug` behavior.
- `crash.rs` + `crashes/` + fatal dialog; panic hook; `check_launch`/`finish_launch`.
- `dialogs.rs` native file chooser + 14 fallback call; `handle_file_import` + drop signal; URI handler on Windows (protocol registry opt-in via `tools/associate.sh --windows`, which drives `reg.exe`/`powershell.exe` from WSL).
- `MindPlatform` `#[func]` probes.

**Verify:** `tools/export.sh --platform windows --verify`; launch the exported exe from WSL via interop (`./build/export/Mindustry-Godot.exe -debug --data-dir out/clientdata &`) and assert `out/clientdata/last_log.txt` (build line `[Mindustry] Version: …`) with `test -f`/`grep`, `-data-dir` override honored, `-maximized false` windowed size, a passed `.msav` opens the load flow (log line), crash report generated by an injected panic (`dev_crash` test hook, debug builds only). Window liveness via `powershell.exe -NoProfile -Command "(Get-Process Mindustry-Godot).MainWindowTitle"`. MCP editor run probes `get_build_info()`/`window_state()`.

### M2 — Dedicated server (headless)

> **Status: ✅ COMPLETE (pure-Rust portion) 2026-10-02.** All pure-Rust modules landed and are tested; the plan-21 delegation points (`say`/`kick`/`ban`/admin/`players`/`info`/`search`/`dos-ban`/`playerlimit`/`subnet-ban`/`name-ban`/`whitelist`) return a clear "requires plan 21/STDB" line, and `host`/`save`/`load`/`runwave` run over the plan-04 synthetic world (`host synthetic survival`) until plan 06/07 supply real maps/buildings. `--commands` boots and exits; the same binary serves the console socket with `--socket-port`. Deviations: config persists through plan-04 `SettingsStore` (`<config>/config/settings.bin`, native `MGST`) **plus a human-readable `<config>/settings.json` mirror** (plan 04 landed the binary store; the plan's §6.3 `.json` was written before 04 M1). `rules.hjson` uses a tolerant flat parser + a typed 3-key `RulesState` until plan 12's `Rules`; unknown keys round-trip.

- `mind-headless` lib target; `server::{boot,config,console,commands,rules_file,autosave,logs,socket,host}`.
- Boot order per §3.6; 34 `Config` keys; `rules.hjson` default/apply; `maps/reloadmaps/reloadassets`; `host/stop/status`; save/load/saves; runwave; `yes` suggestion.
- Autosave cadence/rotation (`auto_<map>_<date>.msav`, keep `autosaveAmount`), `autoPause`, `roundExtraTime` rotation on game over.
- `logs/log-N.txt` rotation at `maxLogLength` + `logCommands`.
- Socket bind/toggle/echo.
- Offline mode (`--stdb offline`) for environments without SpacetimeDB; admin commands return "requires STDB" in that mode.

**Verify:** §7b scenarios; `mind-headless server --config-dir tmp/srv --commands "..."` exit 0 + artifacts; second boot `load 0`; socket round-trip via `tools/server.sh --socket "status"`.

### M3 — Host/admin integration with plan 21 (assumed interface)

> **Status: ✅ COMPLETE (offline/headless interface) 2026-10-03 (lane/f23-22).** `admin.rs` (`AdminMirror` over `config/{admins,bans,whitelist}.json`), `net_host.rs` (`NetHost`: `HostControl` + `Admins` over plan-21's connector/reducers with offline-mirror fallback; hex-id resolution; player-limit), `HostError::NeedsWorldLoader` for known-but-unwired maps, and the `Admins`/`HostControl` traits in `host.rs`. Delegation is unit-tested against an **offline connector** (`offline_grant_ban_whitelist_persist_to_mirror`, `hex_targets_resolve_without_a_connection`, `search_scans_live_and_mirror`, …). The live `spacetime start` + two-client gate is deferred — no live STDB/editor in the lane. See the Changelog.

- `host.rs` `HostControl` implementation against plan 21's host API; `players/status` views; admin command delegation + `config/{admins,bans,whitelist}.json` mirror; `playerlimit`; `say`; `kick/ban/…`; `info/search` (+ offline scan).
- Startup through `mind-stdb` connector; token path `<config>/identity/`.

**Verify:** local `spacetime start` + `server/build.sh`; two `--pN` clients (plan 01 MCP pattern) join; server console `status`/`players` reflect them; `kick` disconnects; `ban` persists across server restart via STDB (SQL evidence `spacetime sql`).

### M4 — Services seam + updater disposition

> **Status: ✅ COMPLETE 2026-10-03 (lane/f23-22).** `mind_core::service` (`GameService` trait + `NullService` + `discord_presence`/`discord_opted_out`, id constants) unit-tested; `mind-gdext::platform::{service,discord,workshop,update}`; the `discord` Cargo feature is **default-off** and, when enabled, `start()` only logs a disabled notice (no RPC linked); `docs/platform/{steam-parity,discord}.md` complete. `cargo check -p mind-gdext` clean with `--features discord` and `--no-default-features`. See the Changelog.

- `service.rs` (`GameService`/`NullService`) wired into `MindPlatform`; `discord.rs` compile-gated evaluation; `workshop.rs` stubs; `update.rs` stub; `docs/platform/{steam-parity,discord}.md` complete.
- Acceptance: no-op services do not affect gameplay; enabling the Discord feature without the plugin starts cleanly (feature off by default).

**Verify:** `cargo test` service stub test; feature-on/off `cargo check`; `godot_game play` with feature off has zero new errors; docs reviewed against `DesktopLauncher.java` line items.

### M5 — Save associations, import/export, drag-and-drop

> **Status: ✅ COMPLETE (headless/Godot-parse) 2026-10-03 (lane/f23-22).** `mind_core::platform::{assoc,file_import,uri}` (`.desktop`/`.reg` bodies, extension+meta import routing, dual-scheme `mindustry://` parser) unit-tested; `mind-gdext::platform::{desktop,dialogs,uri}` (window/args, `launchid.dat`, native-dialog bridge, `OS.shell_open`/clipboard); `tools/associate.{sh,ps1}` with `--print`/`--install`/`--uninstall` (Linux `.desktop` + Windows HKCU `.reg`). `MindPlatform` is wired into `client/scenes/spine.tscn` and verified by `godot4 --headless --editor --quit` exit 0 and a direct instantiation (`SPINE_OK class=MindPlatform`). The live `xdg-open`/`Start-Process`/drag-drop MCP gates need a built export + editor bridge and are deferred. See the Changelog.

- `tools/associate.sh|.ps1` (`.desktop` MimeType on Linux/WSL; HKCU `.msav`/`.msch` + `mindustry://`/`mindustry-godot://` on Windows — run the `.ps1` twin via `powershell.exe` from WSL; macOS documented only).
- Boot-time argv path import; drop-signal import; FileChooser export flows (save/map/schematic/image) smoke through 04/14 APIs.

**Verify:** `tools/associate.sh --install --all` then `xdg-open a.msav` (Linux) / `powershell.exe Start-Process .\a.msav` (Windows) opens the load flow; `--uninstall` removes registrations; drag-drop via MCP `dev_file_drop("/abs/path.msav")` triggers the import router and a screenshot shows the load/import surface.

### M6 — Mobile exports (OD5)

- Android preset + icons + keystore docs; `mobile.rs` (back, orientation, IME, hide, safe-area forward, app-pause); performance-tier defaults; `tools/adb-smoke.sh|.ps1` (install/launch/tap/screenshot) documented (`adb` is not installed in WSL — optional, device required); iOS preset + plugin design + `store-packaging.md`.
- **Target statement:** Android is implemented and smoke-tested when a device/emulator is available (`adb` must be installed in WSL; currently missing); iOS is preset-only on this WSL/Linux host (macOS build required) and marked NOT-RUN in the Changelog with reason (OD-P3).

**Verify:** `tools/export.sh --platform android --verify` produces an APK; if a device is attached, `adb install` + touch smoke (menu open, settings, start a sandbox map via taps) with `adb exec-out screencap` evidence; back button closes the top dialog; app background/foreground preserves state.

### M7 — Linux/macOS exports, web statement, budgets, exit gate

- Linux/macOS presets + doc; `docs/platform/web.md`; run §7d budget measurements; tick §7e; register scenarios with plan 23.

**Verify:** `tools/export.sh --platform linux --verify` (native on the WSL host); the Windows preset was cross-exported and interop-launched at M1; macOS preset present with a NOT-RUN note (needs macOS to build); all §7e boxes ticked with paths/commands.

---

## 6. Data & formats

### 6.1 Data-root resolution (`data_root()`)

```
1. MIND_DATA_DIR env (new)          -> absolute path
2. MINDUSTRY_GODOT_DATA_DIR env     -> absolute path          (plan 00 OD-R9)
3. MINDUSTRY_DATA_DIR env           -> absolute path          (upstream name parity)
4. --data-dir <path> (wins under all env? no: arg wins)       -> absolute path
5. platform defaults:
   desktop client : $XDG_DATA_HOME|~/.local/share/Mindustry-Godot (Linux/WSL default), %APPDATA%\Mindustry-Godot\ (Windows),
                    ~/Library/Application Support/Mindustry-Godot (macOS)
                    unless a `portable.dat` marker or a writable `data/` dir sits next to the executable -> ./data
   android/ios    : Godot `user://` (globalized by mind-gdext)
   dedicated      : ./config unless --config-dir (upstream `Core.files.local("config")`)
```

Precedence above is the resolution for R4 (reconciles 00 OD-R9 and 04 R3: app-data by default, portable opt-in, server `./config`). `client/rust/mind-core` never reads env/paths itself; `mind-gdext`/`mind-headless` resolve and inject the root through `Platform::data_dir`.

Subdirectories (upstream names kept): `screenshots/`, `maps/`, `previews/`, `saves/`, `tmp/`, `mods/`, `schematics/`, `assetCache/`, `server_list.json`, `launchid.dat`, `last_log.txt`, `crashes/`, `settings.json` (client), plus server-only `config/{settings.json, rules.hjson, logs/, saves/, maps/, mods/, schematics/, assets/patches/, assets/sprites/ (20), crashes/, identity/}`.

### 6.2 `version.properties` (generated; upstream keys)

```properties
type=official
number=4
modifier=release
commitHash=unknown
buildDate=unknown
build=-1
```

Rules: missing key → upstream default (`type/modifier` "unknown", `number` 4, `commitHash/buildDate` "unknown", `build` → -1 when non-numeric); `build` may be `N` or `N.R`; `modifier.contains("steam")` sets `is_steam`. Generated LF-only; file is gitignored.

### 6.3 Server config (`<config>/settings.json`, 04 `SettingsStore`)

The 34 upstream keys with exact names/defaults/coercion (from `Administration.java`): `name="Server"`, `desc="off"`, `port=6567`, `autoUpdate=false`, `showConnectMessages=true`, `enableVotekick=true`, `startCommands=""`, `logging=true`, `strict=true`, `antiSpam=!headless`, `interactRateWindow=6`, `interactRateLimit=25`, `interactRateKick=60`, `messageRateLimit=0`, `messageSpamKick=3`, `packetSpamLimit=300`, `uuidChangeLimit=10`, `uuidChangeTimePeriod=3`, `chatSpamLimit=20`, `socketInput=false` (alias key `socket`), `socketInputPort=6859`, `socketInputAddress="localhost"`, `allowCustomClients=!headless`, `whitelist=false`, `motd="off"`, `autosave=false`, `autosaveAmount=10`, `autosaveSpacing=300`, `debug=false`, `snapshotInterval=200`, `autoPause=false`, `roundExtraTime=12`, `maxLogLength=5242880`, `logCommands=true`. Keys used by plan 21's relay/validation are marked in §3.7; persistence is local, enforcement is 21. Never log secrets; `identity/` is gitignored.

### 6.4 `rules.hjson`

Default written when absent: `reactorExplosions: false\nlogicUnitBuild: false\nlogicUnitDeconstruct: false` (upstream `defaultRuleString`). Applied on `PlayEvent` by 12's `Rules` deserializer through 04 `JsonIO`; `rules [remove/add]` edits and rewrites. Parse errors log and continue with map rules (upstream behavior).

### 6.5 Autosave / logs naming

- Autosave: `saves/auto_<map>_<MM-dd-yyyy_HH-mm-ss>.msav`, oldest beyond `autosaveAmount` deleted; reset on `WorldLoad`.
- BE autosave (`autosavebe.msav`) is only read (never written; updater removed).
- Server logs: `logs/log-<N>.txt`, start at the first file with `len < maxLogLength`, append; `[End of log file. Date: …]` marker on rotation; color codes stripped; console timestamp `MM-dd-yyyy HH:mm:ss`; `last_log.txt` always written.

### 6.6 Command socket protocol

Line-delimited UTF-8. Client → server: one command per line (identical to stdin handling). Server → client: every formatted log line, colors stripped, one per `\n`. Default bind `localhost:6859`, disabled by default. One active client; a new connection replaces the output stream. No auth (loopback only; upstream parity).

### 6.7 Associations / URI

- Windows (opt-in `tools/associate.sh --windows`, or the `.ps1` twin run via `powershell.exe` from WSL): `HKCU\Software\Classes\.msav` → `MindustryGodot.Save`; shell `open` command `"<exe>" "%1"`; same for `.msch`; `HKCU\Software\Classes\mindustry-godot` (and `mindustry`) `URL Protocol` with `"%1"` passed as a normal arg (parser detects the scheme).
- Linux (`tools/associate.sh`): `~/.local/share/applications/mindustry-godot.desktop` with `MimeType=application/x-mindustry-save;application/x-mindustry-schematic;x-scheme-handler/mindustry;` and `%u`/`%f` handlers.
- iOS/Android: document types/intent filters in the presets (Android `VIEW` intent filters for `mindustry://` and file types).
- Opening a Java `.msav` (OD2 off) shows the "Unknown save version" error path (04) — expected, documented.

### 6.8 Export presets (committed `client/export_presets.cfg`)

Presets: `Windows Desktop` (primary), `Linux/X11`, `macOS`, `Android`, `iOS`. Required fields captured per preset: `name`, `platform`, `export_path`, `application/{product_name, company_name, file_version, product_version}` (from `BuildInfo`), `application/icon`, console wrapper, `application/modify_resources`, `binary_format/embed_pck`, `codesign/*` (macOS), `keystore/*` (Android, empty), `package/unique_name`, `screen/orientation`, permissions, `notarization/*`. Template requirement: Godot **4.7.2 export templates** installed at `~/.local/share/godot/export_templates/4.7.2.stable/`; `tools/export.sh` fails with an install hint if absent. **Status on the WSL host (checked 2026-10-01): templates are NOT installed yet** — download `Godot_v4.7.2-stable_export_templates.tpz` or use `godot4 --editor` → Editor → Manage Export Templates before plan 22 M1.

---

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests

**Inventory result: none applicable.** Upstream `tests/src/test/java/**` contains only `ApplicationTests`, `DataAssetTests`, `PatcherTests`, `LogicTests`, `GenericModTest`, `ModTestAllure`, `power/*` — none exercise `Version`, `Platform`, `DesktopLauncher`/`ServerControl` command parsing, or export paths; `desktop`/`server`/`android`/`ios` modules ship no tests. Platform behavior is verification-by-manual/MCP (§7c). New Rust tests (not ports, required by D5):

| Test | Asserts |
|---|---|
| `mind_core::version::tests::{parse_upstream_keys, parse_build_revision, missing_keys_default_custom, crlf_and_comments, is_at_least_build, is_at_least_build_revision, build_string_revision, combined_official_steam}` | `Version.java` semantics 1:1, incl. `build<0 → "custom"`, `isAtLeast` dot/no-dot branches. |
| `mind_core::platform::caps::tests::{mobile_implies_not_headless, tier_from_processor_count}` | caps invariants. |
| `mind_headless::server::config::tests::{defaults_match_upstream, set_coerce_bool_int_string, unknown_key_rejected, socket_alias_key, roundtrip_json}` | 34-key table. |
| `mind_headless::server::console::tests::{parse_required_optional_variadic, unknown_suggests_levenshtein, yes_replays_suggestion, few_many_arguments}` | `CommandHandler` + `yes`. |
| `mind_headless::server::rules_file::tests::{default_written, parse_and_apply, bad_json_logged_and_ignored}` | `rules.hjson`. |
| `mind_headless::server::autosave::tests::{filename_sanitizes_map, keep_amount_rotates_oldest, spacing_ticks}` | autosave naming/rotation. |
| `mind_headless::server::logs::tests::{rotates_at_max, strips_colors, next_index}` | log rotation. |
| `mind_headless::server::socket::tests::{line_roundtrip, disabled_is_noop}` | TCP console protocol (loopback, ephemeral port). |
| `mind_headless::args::tests::{width_height_maximized, data_dir_precedence, gl_flags_ignored, connect_lobby_pair}` | launch-arg parser. |
| `mind_core::crash::tests::{report_contains_build_os_mods, likely_mod_from_frame}` | report composition + likely-mod matching (plan-20 registry stub). |

### 7b. Headless harness scenarios

| Scenario / command | Setup | Assertions |
|---|---|---|
| `mind-headless version --json` | generated `version.properties` fixture | exact keys and `buildString`/`combined` golden. |
| `mind-headless server --config-dir tmp/srv1 --commands "config name Demo,config autosave true,config autosaveSpacing 1,host groundZero survival,runwave,status,save 0"` then `--commands "load 0,exit"` | no STDB (`--stdb offline`), fixed seed | exit 0; `settings.json` has exactly the two changed keys; `rules.hjson` exists with upstream default; `saves/0.msav` exists and re-loads with equal checksum (04 API); `logs/log-0.txt` non-empty; `status` prints map + wave. |
| `mind-headless server --config-dir tmp/srv2 --socket-port 0` (test hook reports the bound port) + `tools/server.sh --socket "status,exit"` | loopback | socket accepts, command runs, log lines echo back, clean exit. |
| `mind-headless server --config-dir tmp/srv3 --commands "host unknown-map-name"` | bad map | non-fatal error line, server stays up, exit via `exit`; exit code 0. |
| `mind-headless server --commands "config autoUpdate true"` then boot | updater stub | one `[I]`/`[W]` notice only; no network attempt (test asserts no outbound socket by binding all STDB/UDp disabled). |
| Autosave rotation | `autosaveSpacing 1`, `autosaveAmount 3`, 5 seconds of ticks (accelerated clock hook) | exactly 3 `auto_*` files, newest kept. |
| Log rotation | `maxLogLength 2048`, emit 5 KB of logs | `log-0.txt` ends with the marker, `log-1.txt` exists, no file exceeds the cap materially. |
| `mind-headless server --config-dir tmp/srv4 --commands "reloadmaps,maps all,mods,status,exit"` | fixture maps dir | sorted map list; mod list empty; exit 0. |

All scenarios register with plan 23 under names `platform_*`/`server_*`.

### 7c. MCP / manual playtest scenarios (concrete)

**Scenario A — export a Windows build and launch it (manual + scripted checks).**

Preconditions: `tools/build.sh` done; Godot 4.7.2 + **export templates installed**; `export_presets.cfg` committed.

1. `godot_health check`; if BRIDGE_NOT_CONNECTED, start the editor per the repo skill.
2. `godot_export presets` → expect a preset named `windows` with `platform` `Windows Desktop` and `export_path` `build/export/Mindustry-Godot.exe`.
3. `godot_export export {"preset": "windows", "dest_path": "build/export/Mindustry-Godot.exe"}` → wait for the headless export to finish; assert the exe exists (`test -f`).
4. From WSL (interop): `./build/export/Mindustry-Godot.exe -debug --data-dir out/clientdata &`; wait ≤ 10 s; assert `out/clientdata/last_log.txt` exists and contains `[Mindustry] Version:` and `[I]` startup lines; assert the process has a main window (`powershell.exe -NoProfile -Command "(Get-Process Mindustry-Godot).MainWindowTitle"` non-empty).
5. Cold-start budget: `time` from process start to the first `[Mindustry] Version:` log line; recorded (§7d).
6. Kill, relaunch with `-maximized false -width 1200 -height 800`; assert via a debug log line (`window: 1200x800 windowed`) — the exported build has no MCP bridge (autoload stripped), so assertions read the log/data dir, not `godot_exec`.
7. `dev_file_drop`/association check: `./build/export/Mindustry-Godot.exe out/fixtures/slot0.msav`; assert the log shows `importing save slot0.msav` and the load flow starts (screenshot optional/manual).
8. `godot_game stop` is not applicable to the exported exe; kill it (`powershell.exe -NoProfile -Command "Stop-Process -Name Mindustry-Godot"`).

**Scenario B — dedicated server + client connect (fully MCP-driven).**

Preconditions: `spacetime start`; `server/build.sh`; editor running with the bridge.

1. Start server in background: `./client/bin/rust/debug/mind-headless server --config-dir out/srv --commands 'host groundZero survival,status' >/tmp/mind-srv.log 2>&1 &`; assert `out/srv/last_log.txt` soon shows `Server loaded. Type 'help' for help.`
2. `godot_editor_edit open_scene res://scenes/spine.tscn`; `godot_game play` with the scene path.
3. Pid-stamp: `godot_exec eval {"code":"return {\"pid\": OS.get_process_id()}"}`; compare with `godot_game instances`.
4. Trigger join without UI clicks: `godot_exec call /root/Spine/MindPlatform dev_connect ["127.0.0.1:6567"]` (debug-only shim calling plan 21's join); wait; eval `get_node("/root/Spine/MindPlatform").platform_caps()` + a plan-01 connector state probe (`StdbConnector` state / match id).
5. Server side: `tools/server.sh --config-dir out/srv --socket "status,players"` → output shows the map and `1` player.
6. `godot_screenshot game` after load → screenshot path recorded; HUD/load surface visible.
7. Teardown: `tools/server.sh ... --socket "stop,exit"`; `godot_game stop`; `godot_log errors` clean.

**Scenario C — mobile touch smoke (only if a device is targeted/available; OD5).**

1. `tools/export.sh --platform android --verify` → APK path.
2. `adb install -r <apk>`; `adb shell am start -n com.mindustrygodot.game/com.godot.game.GodotApp` (exact activity from the preset).
3. `adb shell input tap <x> <y>` on the Play button (coordinates resolved from the first screenshot), start `groundZero`; `adb exec-out screencap -p > out/mobile_menu.png`, `out/mobile_hud.png`; assert non-blank and HUD panes visible.
4. Rotate (`adb shell settings put system user_rotation 1` or emulator controls); assert no crash; open a dialog and press BACK (`adb shell input keyevent 4`) → dialog closes, second BACK → app backgrounds.
5. If no device/emulator: record `NOT-RUN (no Android device)` with the export artifact as evidence.

### 7d. Performance budget + measurement

| Metric | Budget | Measurement |
|---|---|---|
| Client cold start to first frame (dev host, release export) | ≤ 5 s | `time`/`date +%s%N` from launch until the `[Mindustry] Version` + first-frame log marker (timestamps in `last_log.txt`). |
| Dedicated server boot to `ServerLoadEvent` (release, 18k-region content) | ≤ 3 s | `mind-headless server --boot-timing-json` (new debug flag) or log timestamps. |
| Dedicated server idle RSS (no players, groundZero) | ≤ 150 MB | `ps -o rss= -p <pid>` after 60 s; `--json` stats hook. |
| Server tick CPU with 0 players | ≤ 1 ms/tick p99 | `godot_profiler` is unavailable headless; use `mind-headless server --profile-json out/srv.json` (tick histogram over 60 s). |
| Version/args parse + boot data | ≤ 2 ms | `mind-headless version --bench`. |
| `config`/`status` command latency | ≤ 5 ms p99 (excluding STDB RTT) | socket round-trip timing in `socket.rs` test + `server --profile`. |
| Autosave (groundZero, 200 buildings) | ≤ 2 s; no frame stall > 100 ms | 04 `bench-save` + server autosave timing log. |
| Exported client size (Windows release, embedded PCK) | ≤ 250 MB | `stat -c '%s' build/export/Mindustry-Godot.exe` (engine dominates); PCK ≤ 150 MB. |
| Exported server binary size | ≤ 40 MB | `stat -c '%s' client/bin/rust/release/mind-headless`. |
| Log rotation overhead | ≤ 1 ms per 1 KB write batch | unit bench in `logs::tests`. |

Regressions > 20% warn; > 50% block per HLP §7.4 until plan 23 owns the suite.

### 7e. Exit criteria checklist

- [ ] `cargo test -p mind-core -p mind-headless` green (all §7a tests), boundary greps clean.
- [ ] `mind-headless version --json` matches the golden; `BuildInfo` parsing covers `build` and `build.revision`.
- [ ] §7b server scenarios pass; `settings.json`/`rules.hjson`/`saves`/`logs` artifacts verified; socket round-trip works.
- [ ] 43-command table implemented or explicitly delegated; `js`/`gc`/`autoUpdate` replacements behave as documented.
- [ ] `tools/export.sh --platform windows --verify` produces a launchable exe; Scenario A passes with `last_log.txt` evidence.
- [ ] Scenario B passes with screenshot + socket/status output attached.
- [ ] File associations install/uninstall cleanly (opt-in); dropped/passed `.msav`/`.msch` imports open the right flow; Java-save open shows the OD2 error path.
- [ ] Crash handler writes a report with likely-mod detection; `last_log.txt` never lost.
- [ ] Platform caps/tier drive 14/15/16/17/18 mobile defaults; no `Vars.mobile` divergence.
- [ ] Android export produced; touch smoke run **or** documented NOT-RUN with reason; iOS preset present with NOT-RUN note.
- [ ] Linux/macOS presets present; web statement written (`docs/platform/web.md`).
- [ ] `docs/platform/{steam-parity,discord,store-packaging}.md` complete; `GameService` no-op proven; OD4 decision recorded.
- [ ] §7d budgets measured and recorded; plan 23 scenario registry updated; Changelog evidence appended.

---

## 8. Risks & open decisions

Each item states the default this plan proceeds with; `NEEDS USER DECISION` items are load-bearing and not covered by `HIGH_LEVEL_PLAN.md`.

| # | Decision / risk | Default being planned against | Needs user? |
|---|---|---|---|
| R1 | `mind-headless` is a plan-00 binary crate; this plan adds a lib target + `server` mode (05 §3.2 pattern of additive modules). | Add `[lib] name = "mind_headless"` while keeping `main.rs`; note in plan 00's extension-contract changelog when execution starts. | no |
| R2 | Plans **18 and 21 are not on disk**; host/admin/audio interfaces are assumptions (§3.11/§3.12). | Implement against the assumed traits behind small local traits (`HostControl`, `Admins`, audio notification forward); swap to the real APIs at their kickoff without touching call sites. | no (process) |
| R3 | `Platform` trait mismatch: plan 05 defines `Platform` without `autosave_allowed()`, while plan 12 calls `platform.autosave_allowed()`/`allows_autosave()`. | Implement the method as an additive default on the client/server `Platform` impls now; orchestrator reconciles 05's trait with 12 at execution. | no |
| R4 | Data-root conflict: 00 OD-R9 (`~/.local/share/Mindustry-Godot/`, XDG) vs 04 R3 (portable `./data`, **NEEDS USER DECISION**). | §6.1 precedence: app-data by default; portable `./data` when a marker/writable dir is present; `--data-dir`/env override; server `./config`. Single `data_root()` function in `mind-gdext`/`mind-headless`. | no (04 already flags the user) |
| R5 | **Dedicated server binary form.** Godot headless export vs pure Rust `mind-headless server`. | Pure Rust (justification §3.6). | **locked 2026-10-01 (NUD-50=A)** |
| R6 | **OD4 Steam/Discord.** Steam parity is large (native SDK, lobby/workshop/networking, identity). | `GameService` trait + empty workshop stubs + disabled Discord feature with the full mapping documented; no Steam code. | **locked 2026-10-01 (NUD-04/51=A: deferred)** |
| R7 | **iOS.** | **Locked 2026-10-01 (NUD-52=B): iOS is cut from this phase entirely** — no preset, plugin design, or store notes; tracked as a platform deviation in HIGH_LEVEL_PLAN §9. Revisit only if scope changes. | locked |
| R8 | **Mobile renderer.** | **Locked 2026-10-01: `mobile` everywhere (NUD-06=B); Android inherits the project renderer, no per-platform override needed.** `gl_compatibility` remains the emergency fallback only. | locked |
| R9 | `.msav`/`mindustry://` associations collide with an installed Java Mindustry/other builds. | Opt-in scripts, dual scheme (`mindustry-godot://` preferred), never auto-installed by the game; uninstall supported. | no |
| R10 | Godot export templates (4.7.2) absent on the WSL dev machine/CI; headless export requires them. **Confirmed absent 2026-10-01** (`~/.local/share/godot/export_templates/` is empty). | `tools/export.sh` checks and fails with a download hint; install `Godot_v4.7.2-stable_export_templates.tpz` under `~/.local/share/godot/export_templates/4.7.2.stable/` before M1; CI exports are optional until templates are pinned in plan 23's CI matrix. | no |
| R11 | macOS signing/notarization and Android release signing (keys, Play metadata) cannot be done unattended. | Presets carry empty signing fields; `docs/platform/store-packaging.md` documents commands and env vars; unsigned local exports only. | no |
| R12 | Godot 4.7 Callable-based file dialog API and Android SAF behavior may differ on the pinned build. | Isolate in `dialogs.rs`; 14's `FileChooserDialog` is the always-available fallback; spike at M1. | no |
| R13 | `-debug` collides with Godot's own `--debug`; arg namespace ambiguity. | Parser skips Godot-reserved args via an explicit allowlist and documents `-debug` as ours; `--` user args also accepted. | no |
| R14 | Unattended `Browser`/web export scope creep. | Explicitly not a target (§3.10); revisit only if the user changes scope. | no |
| R15 | Server admin state split between STDB (21) and local JSON mirror can diverge. | STDB is authoritative when online; local mirror is read-only offline fallback; a `sync-admins` log line reports divergence; reconcile with 21. | no |

---

## 9. References

Repo-local: `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 D1–D9, §2.1/§2.2/§2.4, §3 row 22, §4 template, §5 P7 gate, §6.3/§6.5, §7.4, §9, §10 OD1–OD9); `PRELIMINARY_PLAN.md`; `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (spine, logging/data dir, MCP rig); `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md`; `03_ASSETS_IMPLEMENTATION_PLAN.md`; `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (R3 data root, slot import/export, OD2); `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (§3.3 `Platform`); `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (`Platform::workshop_maps` reference); `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (autosave policy, extensions, R12); `14_UI_IMPLEMENTATION_PLAN.md` (§2.3 boundary, §3.11 hook names); `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` (touch ownership); `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (capture API, OD-R2); `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (mobile FX budgets); `client/project.godot`; `client/addons/open_godot_mcp/handlers/export_handler.gd`; `client/addons/open_godot_mcp/AGENTS.md`.

Mindustry: `AGENTS.md`; `core/AGENTS.md`; `core/src/mindustry/AGENTS.md`; `core/src/mindustry/core/AGENTS.md`; `desktop/AGENTS.md` + `desktop/src/mindustry/desktop/DesktopLauncher.java` + `desktop/src/mindustry/desktop/steam/*` (paths only); `server/AGENTS.md` + `server/src/mindustry/server/{ServerLauncher,ServerControl}.java`; `android/AGENTS.md` + `android/src/mindustry/android/{AndroidLauncher,AndroidRhinoContext}.java`; `ios/AGENTS.md` + `ios/src/mindustry/ios/IOSLauncher.java`; `core/src/mindustry/core/{Platform,Version}.java`; `core/src/mindustry/net/{CrashHandler,BeControl,Administration}.java`; `core/src/mindustry/Vars.java`; `core/src/mindustry/ClientLauncher.java`; `core/src/mindustry/ui/FileChooser.java`; `core/src/mindustry/ui/Menus.java`; `tests/AGENTS.md`; `tests/src/test/java/**` (inventory); `fastlane/` (metadata only); `LICENSE` (GPL-3.0).

Tooling/reference: `/mnt/c/Users/Clinton/g/.opencode/skills/{godot-compositor-testing,playtest}/SKILL.md`; `godot4`/`godot4-mono` on PATH in WSL Ubuntu (standard `godot4` is the dev host per OD7); Godot 4.7.2 export-template docs; Godot `DisplayServer.file_dialog_show`/`OS.get_cmdline_args` API docs; `open-godot-mcp` `godot_export` tool docs.

## Changelog

> Append entries here when execution starts. Every “done” claim carries evidence (command, artifact path, log line, screenshot, checksum).

- (none yet — M0 not started)

- **2026-10-02 — M0 COMPLETE (lane/22-export @ 2d73179).** Landed the pure-Rust platform foundation:
  - `mind_core::version::BuildInfo` — full `Version.java` port (`init_from`, `is_at_least`/`is_at_least_nums`, `build_string`, `combined`, `is_steam`, `enabled`, `to_json`). `mind-core/build.rs` emits `$OUT_DIR/version.properties` from `client/assets/version.properties` (gitignored) or the upstream defaults, so `embedded()` always compiles. 9 tests (`version::tests::*`, §7a row).
  - `mind_core::platform::caps` — `PlatformKind`/`PerformanceTier`/`PlatformCaps` + `mobile_implies_not_headless`/`tier_from_processor_count`/`headless_caps`/`test_mobile_enables_mobile_ui` (4 tests).
  - `mind_core::platform::args` — pure `parse(&[String]) -> LaunchArgs` (upstream flags, `--data-dir`, `GL` ignored set, `+connect_lobby`, Godot-reserved skip, `--` separator) + `data_root` precedence; re-exported as `mind_headless::args`/`mind_gdext::platform::args`. 7 core tests + the 4 §7a-named harness aliases.
  - `mind-headless version [--json] [--file <path>]` with the §7b JSON shape (`type,modifier,commitHash,buildDate,number,build,revision,isSteam,buildString,combined`).
  - `tools/version.sh` + `tools/version.ps1` (flags `--build-number/--revision/--type/--modifier/--commit-hash/--build-date/--out`); `.gitignore` covers `client/assets/version.properties`.
  - `mind-gdext::platform::MindPlatform` (`Node` class, MCP probes `get_build_info/get_platform_caps/window_state/set_fullscreen/dev_file_drop/dev_open_uri/dev_set_test_mobile`) + arg re-export. **Scene insertion at `/root/Spine/MindPlatform` is deferred to the orchestrator (shared `spine.tscn`, HLP §5.2 rule 1); M0 does not touch scenes.**
  - Evidence: `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test -p mind-core` = **461 lib + 3 integration passed / 2 ignored**; `cargo test -p mind-headless --lib args` 4/4; `cargo check -p mind-gdext` clean; `bash tools/version.sh --build-number 43 --revision 2 --type official --modifier release --commit-hash deadbee --build-date 2026-10-02` then `cargo run -p mind-headless -- version --json` → `build 43.2`, `combined "release build 43.2 (deadbee)"`, exit 0. Plan deltas recorded in the M0 status note above.
  - Deviations from the plan text: (a) `args` parser owned by `mind-core` (Godot-free) with thin re-exports rather than `mind-gdext` only; (b) `embedded()` via build.rs instead of `include_str!` of a gitignored path. Both are recorded so M1 consumes a single implementation.

- **2026-10-02 — early-start scope audit (lane/22-export).** Confirmed the gating per HLP §5.1: M0/M2 are pure Rust and unblocked right after plan 00; M1 needs export templates (still absent, R10) + the MCP/editor mutex; M3–M7 need plans 14/15/16/21 (21 is on disk as a plan but not implemented; 14/15/16 not implemented). The `mind-headless` lib target required by HLP §12 C8 already landed under plan 00 (`lib.rs` + `main.rs` split); M2 completes the `server` mode only.

- **2026-10-02 — M2 COMPLETE (pure-Rust portion, lane/22-export).** New `client/rust/mind-headless/src/server/`:
  - `config.rs` — the 34 `Administration.Config` entries (names/keys/aliases/defaults, headless `antiSpam=true`/`allowCustomClients=false`), `get_*`/`set` coercion + `ConfigError`, persistence over plan-04 `SettingsStore`. Tests `defaults_match_upstream`/`set_coerce_bool_int_string`/`unknown_key_rejected`/`socket_alias_key`/`roundtrip_json`.
  - `console.rs` — `ParamSpec` grammar (`<required> [optional] ...`), `ArgError::{Few,Many}`, `levenshtein`, `suggest` (<3), `ConsoleState` for `yes`. Tests `parse_required_optional_variadic`/`few_many_arguments`/`unknown_suggests_levenshtein`/`yes_replays_suggestion`.
  - `commands.rs` — the 43-command `ServerControl` table (`COMMANDS` len == 43 test); local commands implemented, plan-21 admin/host commands delegate with a clear message.
  - `rules_file.rs` — `ServerControl.defaultRuleString` bootstrap (`rules.hjson`), tolerant flat parse + `RulesState` apply, bad input logged+ignored. Tests `default_written`/`parse_and_apply`/`bad_json_logged_and_ignored`.
  - `autosave.rs` — `auto_<map>_<MM-dd-yyyy_HH-mm-ss>.msav`, `spacing_ticks`, rotation keep-newest. Tests `filename_sanitizes_map`/`keep_amount_rotates_oldest`/`spacing_ticks_math`.
  - `logs.rs` — `logs/log-<N>.txt` rotation at `maxLogLength` + `[End of log file. Date: …]` marker, color stripping. Tests `strips_colors`/`next_index`/`rotates_at_max`.
  - `socket.rs` — loopback line socket, one active client, colored `send_log`. Tests `line_roundtrip`/`disabled_is_noop`.
  - `host.rs` — `HostControl` trait + `LocalHost` over the plan-04 `FixtureWorld` (`host`/`stop`/`status`/`run_wave`/`save`/`load`).
  - `mod.rs` — `ServerLauncher.init` boot order, `ServerOptions`/`ServerState::emit`, stdin/socket loops, `settings.json` mirror.
  - CLI: `mind-headless server --config-dir … --commands "a,b" --socket-port N --stdb online|offline --boot-timing-json …`; `tools/server.sh|.ps1` (run or drive the socket).
  - Evidence: `cargo test -p mind-headless --lib` **28 passed**; `cargo test -p mind-core` 464 lib + 5 integration passed / 2 ignored; fmt + workspace clippy `-D warnings` clean; `cargo check -p mind-gdext` clean; `cargo tree -p mind-core` free of godot/tokio. End-to-end (script `tmp/srv_test.sh`): `server --config-dir $SRV --commands "config name Demo,config autosave true,config autosaveSpacing 1,host synthetic survival,runwave,status,save 0,exit"` exits 0 and writes `settings.json` (812 B)/`rules.hjson`/`saves/0.msav` (6034 B)/`logs/log-0.txt`; a second boot `--commands "load 0,status,exit"` reloads slot 0 (wave 4). Socket round-trip (Python harness): `--socket-port 6863` + client `status`/`exit` receives `Hosting: no` + `Exiting server.`/`Server closed.`.
  - **Gated (not faked):** `host groundZero` (plan 06/19 map registry), multiplayer admin/host (plan 21), server-side mod content/assets (plan 20), real tick profiling (plan 23). Recorded in the M2 status note.

- **2026-10-03 — M3–M5 COMPLETE at the headless/parse-verifiable level (lane/f23-22; base `main` @ `53a22db`, WIP `95058dd` + the docs commit recorded in `HIGH_LEVEL_PLAN.md` §13).** Lane agent. Combined the committed M3 server/admin work with the uncommitted M4/M5 seams; Godot/STDB live gates are explicitly deferred.
  - **M3 (host/admin interface).** `server/admin.rs` — `AdminMirror` over `config/{admins,bans,whitelist}.json` (`AdminEntry`/`BanEntry`/`WhitelistEntry`, `is_admin`/`add_admin`/`add_ban`/`is_banned`/`is_whitelisted`/`name_for`/`search`; tolerate-missing/malformed). `server/net_host.rs` — `NetHost` implementing plan-21 `HostControl` + `Admins` over `mind-stdb::Connector`/`MatchSession`; `ensure_connected`/`pump`/`tick`; hex-id resolution; `playerlimit`; STDB-authoritative delegation with offline-mirror fallback. `server/host.rs` gains `HostError::NeedsWorldLoader` (known built-in/sector map vs genuinely unknown) and host round-trip tests. Delegation is covered against an **offline connector** (`net_host::tests::{offline_host_reports_empty_and_offline,offline_grant_ban_whitelist_persist_to_mirror,hex_targets_resolve_without_a_connection,offline_kick_say_and_ip_bans_are_unsupported,configured_player_limit_is_stored,search_scans_live_and_mirror}`).
  - **M4 (services seam + updater disposition).** `mind_core::service` — `GameService` trait (all no-op defaults) + `NullService`, `STEAM_APP_ID`/`DISCORD_APP_ID`, `discord_presence` (exact upstream `state`/`details`/`largeImageKey`/`largeImageText` mapping) + `discord_opted_out`, with 3 tests. `mind-gdext::platform::{service,workshop,update,discord}` — `ServiceRegistry` on `MindPlatform` (`service_enabled()` always `false`), empty `workshop_*` stubs, disabled updater (`NOT_AVAILABLE`, `update_available()==false`), and `discord::start()` compile-gated behind the new **default-off** `discord` Cargo feature. `docs/platform/{steam-parity,discord}.md` complete (Steam = deferred NUD-04/51=A; Discord = feature-off).
  - **M5 (associations / import / URI).** `mind_core::platform::assoc` (`.desktop` `MimeType` + Windows `.reg`/uninstaller bodies, `reg_escape`), `file_import` (`ImportKind` + `from_extension`/`route_save`/`is_droppable`), `uri` (dual-scheme `mindustry://`/`mindustry-godot://` parser). `mind-gdext::platform::{desktop,dialogs,uri}` — `data_root` precedence, `check_launch`/`finish_launch` (`launchid.dat`), `apply_window_args`, `handle_file_import` router, native-dialog `Feature::NATIVE_DIALOG_FILE` bridge, `OS.shell_open`/clipboard. `tools/associate.{sh,ps1}` (`--print`/`--install`/`--uninstall`, `--linux`/`--windows`, `--exe PATH|=PATH`). `MindPlatform` added to `client/scenes/spine.tscn` as `/root/Spine/MindPlatform`.
  - **Additive M1 groundwork (export still gated).** `mind_core::crash` (CrashHandler port: `create_report`, `get_mod_cause`/`get_matches`, `crashes/` naming) + `mind-gdext::platform::crash` (host context, panic hook chaining the log bridge). M1's desktop **export** remains gated on the absent 4.7.2 templates (R10).
  - **Fix.** `server::host::tests::synthetic_host_roundtrips_status` asserted an absolute wave; `FixtureWorld::synthetic` starts at wave 3 (save fixture), so it now asserts the `run_wave` increment.
  - **Evidence.** `cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check` clean; `cargo clippy --workspace --all-targets --manifest-path client/rust/Cargo.toml -- -D warnings` clean; `cargo test -p mind-core` **1558 lib + 13 integration passed / 3 ignored**; `cargo test -p mind-headless` **128 lib + 5 integration passed**; `cargo check -p mind-gdext` clean and clean with `--features discord` and `--no-default-features`; `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` clean; `godot4 --headless --editor --quit --path client` exit 0 and a direct `spine.tscn` instantiation prints `SPINE_OK class=MindPlatform`; `bash tools/associate.sh --print`, `--print --windows`, and `--exe PATH` all print the expected file bodies.
  - **Deferred (not faked).** M3 live gate (`spacetime start` + `server/build.sh` + two `--pN` clients + `kick`/`ban` SQL evidence) — no live STDB/editor in the lane; M5 live gate (`xdg-open`/`Start-Process` open flow, MCP `dev_file_drop` screenshot) — needs a built export + editor bridge; M1 desktop export (templates absent, R10); M6 Android/iOS and M7 Linux/macOS/web (M6 mobile.md/store-packaging.md status docs exist but exports need templates; web stays out of scope).

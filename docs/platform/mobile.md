# Mobile exports (plan 22 §3.8 — OD5)

> Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

**Status: Android preset + platform policy headless; touch parity is plan 15.**
Export templates are absent (R10), so no APK is produced on this host. iOS is cut
(NUD-52=B) — see `store-packaging.md`.

## Platform policy (implemented, Godot-free)

`mind_core::platform::caps` is the single source of boot data consumed by plans
14/15/16/17/18:

- `PlatformKind::{Desktop, Android, Ios, Headless}`.
- `PerformanceTier::{Low, Medium, High}`; `from_processor_count(kind, cores)`
  forces **Low** on Android/iOS regardless of core count.
- `PlatformCaps { kind, headless, mobile, test_mobile, locale, locale_default,
  tier, steam }`; `is_mobile_ui()` returns `mobile || test_mobile`.
- `-testMobile` / `--mobile-preview` set `test_mobile`, so desktop can exercise the
  mobile layout without a device.

Mobile default effects: halved particle budgets (plan 17), `pixelate` on (plan 16),
smaller FX/preview caps (plans 12/19), low-quality audio defaults (plan 18).

## Runtime behavior (Godot glue)

- `NOTIFICATION_APPLICATION_PAUSED/RESUMED` → sim pause gate + audio duck, and
  finish-launch when backgrounded (upstream `ClientLauncher.pause` mobile rule).
- `NOTIFICATION_WM_GO_BACK_REQUEST` → `MindUi.close_top_dialog()` (plan 14) if a
  dialog is open, else `platform.hide()`; `application/config/quit_on_go_back=false`.
- Orientation: export-level `sensor_landscape`; editor `beginForceLandscape` maps
  to a native plugin if present, else a no-op + portrait warning (documented
  deviation — the editor is desktop-first).
- Text/IME: `text_input` uses Godot's virtual keyboard driven by plan 14's
  `show_text_input`; mobile must use this path.
- Safe areas: plan 14 owns `DisplayServer.get_display_safe_area()` margins; plan 22
  forwards insets on `size_changed`.
- Data root: Godot `user://` (P22-7), no `files_moved` migration.

## Verification status

| Item | Status | Reason |
|---|---|---|
| Android preset | deferred | export templates absent (R10) |
| Android touch smoke | NOT-RUN | no `adb`/device in WSL; optional |
| iOS preset | **cut** | NUD-52=B |
| Caps/tier headless tests | ✅ | `mind_core::platform::caps::tests` |

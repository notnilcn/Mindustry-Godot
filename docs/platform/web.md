# Web / HTML5: not a target (plan 22 §3.10 — R14)

> Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

Godot can export to the web (WebAssembly), but **HTML5/Web is explicitly out of
scope** for this port. This is a deliberate deviation (R14), not an oversight.

## Reasons

1. **No native file dialogs / drag-and-drop.** `DisplayServer.file_dialog_show`
   and the files-dropped signal have no browser equivalent for arbitrary paths;
   the plan-22 §3.4/§3.5 import routes (association opens, dropped `.msav`) cannot
   work. The 14 `FileChooserDialog` fallback is not a substitute for the OS flows
   the parity checklist requires.
2. **SpacetimeDB transport + threading.** `mind-stdb` owns its runtime internally
   (OD-R4) and the SDK's websocket transport plus browser threading limits are
   unproven here; the server-authoritative path is deferred anyway (D2).
3. **Determinism/performance unproven.** The fixed 60 Hz sim (D8) budgets in §7d
   have never been measured under WASM; output sizes and CPU variance are unknown.
4. **Open decisions.** OD1 (script mods), OD3 (transport) and OD4 (Steam/Discord)
   are unresolved; shipping web before them would freeze a surface that may change.

## What this means

- No `Web` preset is authored in `export_presets.cfg`.
- No `docs/platform` web plugin, service worker, or JS shell is planned.
- Revisit only if the user changes scope; that would be a new plan, not a
  milestone of plan 22.

Desktop (Windows/Linux/macOS) is the supported distribution surface; Android is
OD5; iOS is cut (NUD-52=B).

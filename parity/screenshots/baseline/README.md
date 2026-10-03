# Screenshot baselines (deferred capture)

`../manifest.json` lists the fixed-pose screenshot oracles promised by plans
00/02/04–18. In-engine capture requires the Godot editor and is gated on the
single-editor MCP mutex (plan 23 §3.5, NUD-40/A), so every entry currently
stays `status = "deferred"` and this directory is empty.

Promoting a baseline:

1. Capture with `godot_screenshot game` (or `region`) at the pose recorded in
   the manifest, following `parity/README.md` and
   `.opencode/skills/playtest/SKILL.md`.
2. Save the PNG here as `parity/screenshots/baseline/<id>.png`.
3. Set the manifest entry's `status = "captured"` and record its `sha256`.
4. Run `mind-headless parity screenshots` — it diffs committed PNGs and fails a
   captured entry whose hash drifted.

`parity screenshots` also flags a committed PNG that is still marked `deferred`
so the manifest and the directory never silently diverge.

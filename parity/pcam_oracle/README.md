<!-- SPDX-License-Identifier: GPL-3.0-only -->

# Phantom Camera GDScript oracle (read-only)

This directory holds the **unmodified** Phantom Camera **0.11.0.3** GDScript tree
(`phantom_camera/`, MIT, Marcus Skov — see `phantom_camera/LICENSE` and
`phantom_camera/plugin.cfg`). It is retained read-only as the **parity oracle**
for the Rust GDExtension port in `client/addons/phantom_camera_rust/`
(crate: `client/rust/phantom-camera-rust/`).

**Do not edit, add to, or delete from `phantom_camera/`.** It must stay
byte-identical to the vendored upstream addon so a baseline capture is
reproducible. `git log --follow` on any file under it shows the original
`client/addons/phantom_camera/...` history.

## Why it lives outside `client/`

The Rust GDExtension registers native classes with the same names the GDScript
files declare as `class_name`:

`PhantomCamera2D`, `PhantomCamera3D`, `PhantomCameraHost`, `PhantomCameraManager`,
`PhantomCameraNoise2D`, `PhantomCameraNoise3D`, `PhantomCameraNoiseEmitter2D`,
`PhantomCameraNoiseEmitter3D`, `PhantomCameraTween`, `PhantomCameraTweenDirector`,
`TweenDirectorResource`, `Camera3DResource`.

GDScript **rejects** a `class_name` that hides a native class ("Class
'PhantomCamera3D' hides a global script class."), so the two trees cannot be
imported into one Godot project. Godot imports everything under the project root
(`client/`), so the oracle was moved to `parity/` — outside the project — where
it is never imported or parsed. See
`client/addons/phantom_camera_rust/PORT-PLAN.md` (§ "Why the oracle moved").

## What it is used for

| Use | How |
|---|---|
| Parity baseline | `tools/pcam_parity.sh --baseline` temporarily re-installs this tree into `client/addons/phantom_camera/` (and disables the Rust library), records camera telemetry goldens into `parity/pcam_golden/`, then restores. |
| ABI reference | `client/addons/phantom_camera_rust/dev/abi_dump.gd` is run against this tree to produce `parity/pcam_oracle/abi_gd.json`, the frozen property/signal/enum table the Rust classes must match exactly. |
| Port source | `client/addons/phantom_camera_rust/dev/rewrite_examples.py` reads `phantom_camera/examples/**` and `phantom_camera/fonts/**` from here and writes the ported, native-class copies into `client/addons/phantom_camera_rust/`. Run it to regenerate the demos; never hand-edit the copies' structure. |
| Line-anchored port docs | Every Rust module's `//!` header cites the `phantom_camera/scripts/**` file and line ranges it ports. Those anchors refer to this tree. |

`--baseline` is the only path that may copy this tree back into `client/`, and it
must remove it again when it is done: with both trees imported, every
`PhantomCamera*` `class_name` collides with the native class and the project
fails to load.

## Layout

```
parity/pcam_oracle/
  README.md              this file
  phantom_camera/        the untouched 0.11.0.3 addon tree
    LICENSE              MIT, Marcus Skov (kept with the oracle)
    plugin.cfg           name/author/version = 0.11.0.3
    plugin.gd            EditorPlugin (adds the manager autoload + custom types)
    scripts/             the implementation: 23 .gd files (the port source)
    examples/            the 18 demo scenes + demo helper scripts + assets
    fonts/               Nunito-Black / Nunito-Regular (used by the demos)
    assets/ icons/ inspector/ panel/ themes/   editor-side plugin UI
```

The `scripts/` subtree has no `class_name` on `managers/`, `gizmos/` or `panel/`;
those are editor plumbing and are **not** ported (the Rust side owns only the
runtime classes). The demo scenes in `examples/` are the ABI contract: they name
classes and store property names as strings.

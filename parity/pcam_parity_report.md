# Phantom Camera: Rust port vs GDScript oracle — parity report

Measured with `tools/pcam_parity.sh` (see [`../tools/AGENTS.md`](../tools/AGENTS.md)).
Both legs run the same 18 demo scenes, 240 frames each, at `--fixed-fps 60`, and are
diffed per frame across camera telemetry and the PCam signal stream.

- Oracle leg: the unmodified GDScript addon at `parity/pcam_oracle/phantom_camera/`,
  installed temporarily by `--baseline`, observed by a generated GDScript twin of
  the recorder (`pcam-record/v1` schema is the contract between the legs).
- Candidate leg: `client/addons/phantom_camera_rust/examples/`, observed by the
  crate's `PcamRecorder` (`client/rust/phantom-camera-rust/src/dev/recorder.rs`).

Tolerances: `pos`/`rot`/`zoom`/`transform`/`noise` 1e-4 absolute, `d` 1e-6;
`active`, `priority`, `has_noise`, `signals`, `frames` and the header are exact.

## Result

```
scenes: 7 pass, 9 fail, 2 skip - verdict: FAIL

channel     tolerance  max |delta|  mismatches
pos          1.00e-04            0           0
rot          1.00e-04     2.86e-05           0
zoom         1.00e-04            0           0
transform    1.00e-04     5.57e-05           0
noise        1.00e-04            0           0
active          exact            -           0
priority        exact            -           0
has_noise       exact            -           0
signals         exact            -           9
d              1.00e-06            0           0
header          exact            -           0
frames          exact            -           0
```

**Every numeric channel is exact over 18 scenes × 240 frames.** The only residual is
one signal on frame 1 in 9 scenes.

| scene | verdict |
|---|---|
| `2d_follow_group`, `2d_limit`, `3d_follow_glued`, `3d_follow_simple`, `3d_look_at`, `3d_follow_third_person`, `3d_follow_third_person_attribtues` | PASS (0 mismatches) |
| `2d_example`, `2d_follow_framed`, `2d_follow_path`, `2d_tweening`, `3d_example`, `3d_follow_framed`, `3d_follow_group`, `3d_follow_path`, `3d_tweening` | FAIL — oracle-only `tween_started` on frame 1, all numeric channels 0 |
| `2d_noise`, `3d_noise` | SKIP — nondeterministic source data, see below |

## Residual 1: one `tween_started` emission (9 scenes)

The oracle's `PhantomCameraHost._pcam_tween` emits `tween_started` when
`_tween_elapsed_time == 0` at the top of the first tween frame
(`phantom_camera_host.gd:973-978`); the port emits at the same point with the same
guard. In these 9 scenes the oracle emits it once on frame 1 and the port does not,
while the emitted `is_tweening`/`tween_completed` pair and the resulting transforms
are identical, frame for frame.

The pattern is reproducible and correlates with how the demo declares its tween:

| `tween_resource` | scenes | oracle emits `tween_started` |
|---|---|---|
| external `player_phantom_camera_{2,3}d_tween.tres` | 9 | **yes** |
| inline `[sub_resource type="PhantomCameraTween"]` | 8 | no |
| inline, but re-activated at runtime by `2d_room_limit_tween.gd` | 1 (`2d_limit`) | yes (port matches) |

So the divergence tracks a per-scene tween bookkeeping state (which of the
`_tween_skip` / `_tween_duration == 0` / `_tween_elapsed_time` branches activation
takes, `phantom_camera_host.gd:694-704`) that ends up producing identical motion.
Resolving it needs host-state instrumentation on the oracle leg (record
`_tween_duration`, `_tween_elapsed_time`, `_tween_skip`, `get_tween_skip()` at frame 1
in the generated observer), which the current recorder does not capture. This is a
follow-up item, not a code change to guess at: fabricating an unconditional emit
would diverge from the oracle.

## Residual 2: the two noise scenes are inherently nondeterministic

`tools/pcam_compare.py` reports them as SKIP with the reason. Both
`PhantomCameraNoise2D._init` and `PhantomCameraNoise3D._init` seed `FastNoiseLite`
with `randi()` (`phantom_camera_noise_2d.gd:88-90`, `phantom_camera_noise_3d.gd:88-90`),
and the scene's `randomize_noise_seed = false` is applied *after* `_init`, so the
seed is random on every run of *either* leg; `noise_seed` only shifts the sample's X
coordinate. Verified by recording the oracle twice in an isolated project: consecutive
runs differ while `pos`/`transform` stay bit-identical. The port reproduces the
resource faithfully, so this is an oracle property, not a port divergence. Making it
comparable requires seeding the resource in both demo scenes (a deliberate deviation
from the oracle's scenes) and is left as a follow-up.

## Bugs the harness caught (all fixed)

Five real port defects, none of which the "no errors on scene load" check would have
found, because the camera pipeline stayed silent instead of erroring:

1. `host.rs` — `host_pcam_added_to_scene` re-bound a `Gd<PhantomCamera3D>` that gdext
   already held `&mut` (the manager emits `pcam_added_to_scene` from inside the PCam's
   own callback), so the handler aborted and `PlayerPhantomCamera3D` never activated
   (active priority 0, static camera). Fixed by deferring to `host_pcam_added_check`,
   the non-blocking form of the oracle's `await pcam.ready` (`phantom_camera_host.gd:1272`).
2. `pcam2d.rs`/`pcam3d.rs` — `tree_exiting` connected a bare callable to a 1-arg
   handler; the oracle binds the target (`.bind`). Fixed with `Callable::bindv`.
3. `pcam3d.rs` — the runtime `SpringArm3D` was only published after the ready
   continuation, so `set_shape`/`set_spring_length`/`set_margin` never reached it
   (the oracle assigns `_follow_spring_arm` at creation, `:921`).
4. `pcam3d.rs` — the arm bootstrap read the raw `spring_length` field, which
   `set_spring_length` never writes (`:1955` writes `follow_distance`; `:1962` is an
   alias), so the arm was 1.0 m instead of 3.5 m and the camera sat 2.17 m off.
5. `pcam3d.rs` — `Quaternion::dot` routed through glam rounds differently from
   Godot's `x*x+y*y+z*z+w*w`; it feeds `is_zero_approx(sin(acos(dot)))` (`:1281`), a
   knife-edge when the damped look-at has nearly converged, so the blend stopped two
   frames early. Computing it in Godot's summation order in `f32` fixed it.

## Reproduce

```bash
bash tools/build.sh                       # builds the phantom-camera-rust cdylib too
bash tools/pcam_parity.sh --baseline      # installs the oracle, records goldens, restores
bash tools/pcam_parity.sh --candidate     # records the Rust build
bash tools/pcam_parity.sh --compare       # per-scene table + per-channel deltas
# one scene:  append  --scene 3d_follow_simple
```

The baseline leg rewrites `client/project.godot`'s `PhantomCameraManager` autoload and
disables `client/addons/phantom_camera_rust/phantom_camera_rust.gdextension` for the
duration, then restores both on exit (a `trap`; it also fails loudly when the working
tree is left dirty). Do not run it concurrently with anything else that needs the
oracle or the extension.

The 18 recordings under `parity/pcam_golden/` are registered in
`parity/golden_manifest.json` as `harness` goldens (`pcam_*`, `regen: bash
tools/pcam_parity.sh --baseline`), so `mind-headless parity goldens` hash-verifies them
alongside the rest (`parity check` → 6 checks, 0 failed; `golden_manifest` → 115 entries).

**`--compare` exits 1 today by design**: the nine `tween_started` deltas above are
counted as mismatches. Numeric telemetry — the thing the port actually computes —
matches bit-for-bit on every scene. Passing the gate requires either resolving
residual 1 or making it an explicit, reviewed exception in `tools/pcam_compare.py`;
neither is done here because both would hide a real (if tiny) behavioral difference.

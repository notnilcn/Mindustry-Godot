# Addon evaluation — bulk FX/particle rendering (plan 17 R-17-7 / OD-17-C)

## BlastBullets2D (`godot-blast-bullets-2d/`, `example-project-blastbullets2d/`)

| Field | Value |
|---|---|
| License | MIT © 2025 nikoladevelops (confirmed from `LICENSE.md`) |
| Nature | C++ GDExtension, precompiled `bin/`; a `MultiMesh2D`-based 2D bullet system |
| Runtime ownership | Owns bullet motion, homing, orbit, timers, collision callbacks |
| Decision | **Not adopted** |
| Reason | It would duplicate plan 10's authoritative Rust bullet simulation and couple the client to a godot-cpp ABI; plan 17 handles only bullet *trails/effects*, and the port needs deterministic headless-testable behavior. |
| Replacement | A custom `MultiMesh2D` batcher in `mind-gdext::fx` (threshold `MULTIMESH_THRESHOLD = 256`), with `GPUParticles2D` only above `GPUPARTICLES_THRESHOLD = 4000` instances. |
| `THIRD_PARTY_NOTICES.md` | Unchanged (no third-party code vendored). |

## `ParticleRenderer.java` (upstream `graphics/ParticleRenderer.java`)

Not ported: it is a WIP point-sprite renderer not wired into upstream's pipeline
(`graphics/AGENTS.md`); the port uses `MultiMesh2D`/`GPUParticles2D` instead
(plan 17 §2.4 #10).

## Open decision (OD-17-C)

No user input required; default (not adopted) recorded here and in plan 17 §8.

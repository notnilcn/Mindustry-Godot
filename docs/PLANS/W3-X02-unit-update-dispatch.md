<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W3-X02 — Break `unit_update_system` (948 lines) into per-AI-kind functions

**Wave:** W3. **Needs:** H08 merged (it owns these files in W2). It may run alongside X01.
**Owns:** `client/rust/mind-core/src/sim/unit_runtime/update.rs` and the new
`client/rust/mind-core/src/sim/unit_runtime/ai_arms/**` (declare it in `unit_runtime/mod.rs`; this
package owns that one line).
**Effort:** medium. **Risk:** high. This is the hottest determinism path: RULES §5, verify after every arm.

## Shape today (lines at `291e30a8`, the old `unit_runtime.rs`)

`unit_update_system` (2889–3838) loops over `runtime.units.clone()` (from 2949) and dispatches on the
controller kind with an `if slot.kind == … else if …` chain:
- Player 3007, Command 3011, Missile 3275, Suicide 3360, Builder 3430, Boost 3476, Defender 3500;
- FlyingFollow 3504, Hug 3516, Cargo 3528, Assembler 3556, Logic 3586, NoAi 3673;
- the final `else` (ground/flying) 3676.

After the chain come kinematics (3711), the weapons pass (3784) and shock mines (3794). The loop has
four `continue`s.

Upstream: `entities/comp/UnitComp.java` (`update`) and each `ai/types/*AI.java` `updateMovement`/`updateUnit`.

## Procedure

1. **Map the arms.** For each arm, list:
   - every local it reads or writes;
   - whether it contains `continue`, `break` or `return`;
   - whether it draws from an RNG.

   Put the table in your report **before** you change code.
2. **Extract one arm per commit**, smallest first (Defender, FlyingFollow, Hug, NoAi, Boost, …), into
   `ai_arms/<kind>.rs` as
   `pub(in crate::sim::unit_runtime) fn <kind>_arm(<explicit &/&mut params for exactly the locals it touches>)`.
   - If the arm uses `continue`, make the fn return a `bool` or an enum `ArmFlow { Continue, Fall }`,
     and do the `continue` at the call site.
   - Do not group locals into a context struct unless more than 6 parameters are needed. If you do,
     the struct holds references only; no copies.
   - The arm body moves **verbatim**, apart from the parameter renames.
3. **Leave in the loop:** the chain itself (its order is part of behaviour), the kinematics, the weapons
   pass, shock mines, and every RNG draw that happens outside an arm.
4. **Command arm (3011–3274, ~260 lines).** Extract it last; it calls `pop_live_command_queue`.
5. **Stop rule.** If an arm cannot be extracted without reordering statements or cloning data that is
   currently borrowed, leave it in place and record why.

## Verification after **every** arm commit

```bash
run --cargo-test -p mind-core --lib sim::unit_runtime
run --cargo-test -p mind-core --lib sim::tests::same_seed_same_checksums
run --cargo-run -q -p mind-headless -- parity goldens
run --cargo-run -q -p mind-headless -- run spine_determinism --json
```

Before the report, run the full RULES §6 sim set: `--cargo-test -p mind-core`,
`--features msav-import sim::unit_runtime`, `mind-headless` tests, `run-all --tier T0`, the three
`spine_*` scenarios, `--goldens --check`, and clippy.

Any drift: revert the commit. Do not bless it.

**Target:** `unit_update_system` under 250 lines, and each `ai_arms/*.rs` file cites its Java AI class.

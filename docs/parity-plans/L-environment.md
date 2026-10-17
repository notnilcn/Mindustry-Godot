<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Round L — environment additions

`J-environment.md` applies unchanged: work rules, gates, the "verified" definition and
parallelism. Round L adds these rules, learned in the round-L walk.

1. **Real input or it does not count.** A UI acceptance check must use real input for the
   interaction under test:
   - the real X pointer for hover (`tools/twin.sh` `GP`; injected `mouse_motion` does not move
     Godot's mouse position);
   - real wheel events for scrolling (pynput `mouse.scroll` over the target);
   - real clicks (`GC`/`GW`).

   Facade calls (`emit_signal`, `text_changed.emit`, `configure_building`) may stage state, and
   must be named as staging in the summary. Round K filed three findings (F-K1, F-K4, F-K7)
   that were artefacts of injected input.
2. **Text entry.** Until L01 T4 lands a `unicode` key path in `tools/gmcp.py`, Godot `LineEdit`
   tests can only use `text_changed.emit` (staging). pynput typing goes to the X-focused window,
   which on `:10` stays the Java client.
3. **Java focus.** Hover the target (`JM x y`, ~0.4 s) before `JW` after the pointer has been
   over another window; the first click is otherwise sometimes swallowed.
4. **Fresh build before any walk.** In round L, the main checkout's `libmind_gdext.so` predated
   all round-J merges. Run `tools/build.sh` (flock) and restart the editor
   (`tools/session-play.sh N`) before capturing.
5. **Profiles.** Profile state differs between the clients:
   - Map list order, sandbox map content and campaign sector state differ per profile (file
     mtimes, autosaves, unlocks). Do not file differences in those as findings without first
     normalising, or comparing like-for-like (built-in maps, fresh loads).
   - `blockstatus` is on in the Godot profile and off in Java's, so the status markers on Godot
     buildings are a settings difference.
6. **Goldens.** L08 T1 lands first. Every later golden change carries the Java citation of the
   behaviour that moved it.
7. **Java reference access, per session.** Each session may run the Java client for its own
   acceptance frames and captures:
   - its own data directory, seeded once from the pristine reference (the `parity-java-home`
     pattern); never the operator's default `~/.local/share/Mindustry` (the imported
     fully-unlocked profile) and never another session's copy;
   - its own X display (the `parity-session.sh` assignment). Java input is the real pointer and
     keyboard, so running it against another display types into the wrong window;
   - read-only oracle: no Game Data clearing, no research or save changes; `twin.sh`'s
     settings.bin backup/restore and save-snapshot teardown stay mandatory around every run;
     never rebuild or swap the pinned Java build — all prior evidence is captured against it;
   - captures land in the run dir with the recipe recorded per item.
8. **Close Java when idle.** The JVM and the Godot editor hog the CPU. Quit the Java client as
   soon as a capture batch ends (the `twin_stop` teardown already does this — keep it), and
   never leave an idle Java client running during cargo builds, test runs, or another
   session's captures.

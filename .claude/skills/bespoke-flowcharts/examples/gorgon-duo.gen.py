# Generates the Gorgon duo end-to-end spec (granular revision).
import json, sys

DOC = "docs/HUMAN/_TDD/Enemies and bullets/Gorgon Duo Fight - End to End.md"
PH = "docs/HUMAN/_TDD/Enemies and bullets/"
PHASE_FILES = [
    "Gorgon Phase 0 - Intro.canvas",
    "Gorgon Phase 1 - Tentacles and Shotguns.canvas",
    "Gorgon Phase 2 - Whirlpools.canvas",
    "Gorgon Phase 3 - Enemy Gun and Sunburst.canvas",
    "Gorgon Phase 4 - Orbit and Star Lines.canvas",
    "Gorgon Phase 5 - Wormy Loop.canvas",
    "Gorgon Phase 6 - Twins.canvas",
    "Gorgon Phase 7 - Interleaved Pulses.canvas",
    "Gorgon Phase 8 - Three Mounts.canvas",
    "Gorgon Phase 9 - Mirror Streams.canvas",
]
LANE = {"s": "L-server", "r": "L-relay", "p1": "L-p1", "p2": "L-p2"}
# The colour of the rules.rs groups: each wraps the server cards that are one
# `rules::rules` function (the claim's acceptance check), between the client's
# reducer call and the reducer's writes.
RULES_COLOR = "#8a5cf6"

nodes, edges, groups = [], [], []


def card(i, lane, gx, heading, brief, gy=0, main=True, group=None, color=None, section=None):
    n = {"id": i, "lane": LANE[lane], "gx": gx, "gy": gy, "heading": heading, "text": brief}
    if main:
        n["main"] = True
    if group:
        n["group"] = group
    if color:
        n["color"] = color
    if section:
        n["section"] = section
    nodes.append(n)


def embed(i, gx, f):
    nodes.append({"id": i, "lane": LANE["s"], "gx": gx, "gy": 0, "main": True, "file": PH + f})


def grp(i, lane, label, fork, color, parent=None):
    g = {"id": i, "label": label, "fork": fork, "color": color}
    if parent:
        g["parent"] = parent
    else:
        g["lane"] = LANE[lane]
    groups.append(g)


def rules_grp(i, label):
    """A rules.rs group around main-path server cards (no fork, no doc heading)."""
    groups.append({"id": i, "label": label, "lane": LANE["s"], "color": RULES_COLOR})


def e(a, b, label=None):
    x = {"from": a, "to": b}
    if label:
        x["label"] = label
    edges.append(x)


# ---------------------------------------------------------------- 1. spawn
grp("G-pre", "r", "Before t0 · both players routed (relay)", "Before the fight - routing players in", "5")
grp("G-pre-s", "s", "Before t0 · both players enter the world (server)", "Before the fight - routing players in", "5")
card("r-pre", "r", 0, "Relay routes both players into the region",
     "ensure_account on kg-global -> RoutePlayer outbox -> kg-relay -> handle_route_player writes the player row", main=False, group="G-pre", color="5")
card("s-enter", "s", 3, "Server puts both players in the world",
     "enter_world inserts online_player; health and weapon rows; the subscriptions start; players become aggro candidates", gy=3, main=False, group="G-pre-s", color="5")
card("s-spawn", "s", 6, "Server spawns the Gorgon",
     "admin_spawn_enemy(gorgon, 1, q, r): require_admin, resolve key at step 1, wrap anchor, slot_of_pos, mint_entity_id, seed derive")
card("s-rows", "s", 9, "Server inserts the Gorgon rows",
     "enemy row (template, difficulty, seed, anchor, slot, empty phases/targets, dead_tick None) + enemy_health hp=max_hp=1000; audit; fires nothing yet")
grp("G-spawnfail", "s", "Fork · spawn refused", "Fork - spawn refused", "1")
card("f-spawnfail", "s", 12, "Spawn refused", "every check before the first insert; Err rolls back", gy=3, main=False, group="G-spawnfail", color="1")
card("p1-rows", "p1", 12, "Both clients receive the Gorgon rows",
     "nearby_enemies / nearby_enemy_health in subscribe_region, AoI ring -> EnemySeen -> ModelEvent::Enemy -> WorldModel.enemies, combat_rev -> CombatRuntime::sync EnemyUpsert")
card("p2-rows", "p2", 12, "Both clients receive the Gorgon rows", "same heading as P1")
card("p1-see", "p1", 15, "P1 sees the Gorgon", "KgEnemySpawnerComponent2D instances enemy.tscn; position_at from the anchor, lerp by fraction; nova fallback; 1000/1000 bar")
card("p2-see", "p2", 15, "P2 sees the Gorgon", "same body on the same sub-unit, nearest torus copy to P2's focus; only while in P2's ring")
e("r-pre", "s-enter", "in world")
e("s-spawn", "s-rows")
e("s-spawn", "f-spawnfail")
e("s-rows", "p1-rows", "`nearby_enemies`")
e("s-rows", "p2-rows")
e("r-pre", "p1-rows", "routed")
e("r-pre", "p2-rows")
e("p1-rows", "p1-see")
e("p2-rows", "p2-see")

# ---------------------------------------------------------------- 2. engage
card("p1-move", "p1", 18, "Both clients report their movement",
     "KgPositionSyncComponent -> report_movement on a segment change or the 500 ms heartbeat")
card("p2-move", "p2", 18, "Both clients report their movement", "same heading")
rules_grp("G-rules-move", "rules.rs · movement claim")
card("s-accept-move", "s", 21, "Rule - accept_movement",
     "require_online_woke (Quiet), alive (Dead), judge_report: rate token (RateLimited), charge slow-down, grapple allowance, check_report (Rejected); Verdict MoveAccepted / MoveRefused", group="G-rules-move")
card("s-move", "s", 21, "Server records both player tracks",
     "report_movement applies the verdict: touch_presence, store_report (player_position), push_track_sample (player_track, movement_log), set_player_patch")
card("p1-tracks", "p1", 24, "Both clients receive the tracks",
     "my_track / nearby_tracks -> TrackSeen -> ModelEvent::Track -> sync_tracks; zero-velocity staircase (F1)")
card("p2-tracks", "p2", 24, "Both clients receive the tracks", "same heading")
card("s-pass", "s", 24, "Server runs the 1 Hz combat pass",
     "combat_pass order: quiet_pass (presence), aggro_pass, cleanup_dead; why presence goes first")
card("s-aggro", "s", 27, "Server aggro pass picks P1",
     "candidates: online, alive, in the ring, position predicted from player_track; next_target nearest anchor within 16 hexes, tie lower id")
card("s-stamp", "s", 30, "Server stamps the target and phase 0",
     "TargetSegment{P1, t+60}, locked_until t+1860, first engagement PhaseSegment{0, t+60}; one row update")
embed("s-ph0", 33, PHASE_FILES[0])
grp("G-nobody", "s", "Fork · nobody in range", "Fork - nobody in aggro range", "3")
card("f-nobody", "s", 36, "Nobody in aggro range", "no candidate: no change, no write; repeats next second", gy=3, main=False, group="G-nobody", color="3")
card("p1-pre", "p1", 36, "P1 receives the target and phase 0",
     "nearby_enemies update = insert(new)+delete(old); region_has_enemy keeps it; one upsert, combat_rev, re-derive")
card("p2-pre", "p2", 36, "P2 receives the same schedule", "same update on P2; P2 is not the target and that changes nothing")
card("p1-engage", "p1", 39, "Both clients wait out the preload and the rest",
     "target_at None until t+60; engaged_at true from t+60; phase 0 [rest 120][clip 528]; first shot >= t+60+120+33")
card("p2-engage", "p2", 39, "Both clients wait out the preload and the rest", "same heading")
e("p1-see", "p1-move")
e("p2-see", "p2-move")
e("p1-move", "s-accept-move", "`report_movement`")
e("p2-move", "s-accept-move")
e("s-accept-move", "s-move", "Accept")
e("s-move", "p1-tracks", "`nearby_tracks`")
e("s-move", "p2-tracks")
e("s-move", "s-pass")
e("s-enter", "s-aggro", "online_player")
e("s-pass", "s-aggro")
e("s-aggro", "s-stamp")
e("s-aggro", "f-nobody")
e("s-stamp", "s-ph0", "schedule")
e("s-stamp", "p1-pre", "+60 t lead")
e("s-stamp", "p2-pre")
e("p1-see", "p1-pre")
e("p2-see", "p2-pre")
e("p1-pre", "p1-engage")
e("p2-pre", "p2-engage")

# ---------------------------------------------------------------- 3. taken
card("p1-sched", "p1", 42, "P1 runs the phase 0 schedule each tick",
     "clip_tick_of (rest/clip), fires_at (window), fires_shot (cadence); engaged and alive gates; emitter (phase<<8)|index")
card("p2-derive", "p2", 42, "P2 derives the same volleys", "same row, seed and tick -> same BulletKeys; only aim input can differ")
card("p1-aim", "p1", 45, "P1 aims the targeted ring",
     "aim_angle: Fixed = base+spin; Target reads aim_point at fire_tick-15 (AIM_DELAY_TICKS) on the target's canonical path; no sample -> Fixed")
card("p1-expand", "p1", 48, "P1 expands each volley into bullets",
     "volley_rng_enemy(seed, emitter, fire_tick); three draws per bullet; BulletKey{fire_tick, source, emitter, ordinal}")
card("p1-step", "p1", 51, "P1 steps and draws the boss bullets",
     "Bullet::step order, remainder, spawn tick integrated, life cap 720; KgBulletLayerComponent2D MultiMesh per family")
card("p2-digest", "p2", 51, "Both clients print the same digest", "spawn_digest every 600 ticks recomputed from rows and tracks; HUD combat digest line")
card("p1-bodies", "p1", 54, "P1 builds a body for each player",
     "one Body per connected player: Track from reported samples, Life from health, canonical vs exact collision mode")
card("p1-contact", "p1", 57, "P1 resolver decides P1 was hit",
     "capsule_hits_circle radius 307; earliest (tick, player id) wins; immediate report mode decides own contact at once")
card("p2-contact", "p2", 57, "P2 resolver reaches the same verdict", "remote contact decided once no earlier one is possible; CONFIRM_MAX_WAIT 120")
card("p1-hold", "p1", 60, "P1 holds the contact, then consumes the bullet",
     "REPORT_HOLD_TICKS 12; a new BulletPath serial retracts; handed out -> Input::Consumed, bullet vanishes")
card("p1-send", "p1", 63, "P1 reports the hit", "take_reports, flush_reports while clock settled, HIT_BATCH_MAX 64, PlayerHitReport fields")
rules_grp("G-rules-taken", "rules.rs · enemy-bullet claim")
card("s-batch", "s", 66, "Rule - accept_player_damaged",
     "report_player_damaged calls it: caller is victim; require_online, batch > 64 Err, player_health row, empty Ok(None); sort order; TakenBatch", group="G-rules-taken")
card("s-taken", "s", 69, "Rule - TakenJudge.judge",
     "per entry: kg_rules::enemy_bullet::judge -> dead, duplicate, no_enemy, check_hit (window, enemy_shot, enemy_hit_window), off_path, rate_limited", group="G-rules-taken")
card("s-apply", "s", 72, "Server applies the hit to P1",
     "report_player_damaged: apply_player_hit: resolve 10 vs defence 0, hp 100->90, recent_hits; TakenJudge::accepted, hit_log, TakenJudge::save")
card("p1-hp", "p1", 75, "P1 HP bar drops to 110", "my_health -> HealthSeen -> ModelEvent::Health -> KgHealthBar")
card("p2-hp", "p2", 75, "P2 sees P1 at 110 HP", "nearby_health -> KgRemotePlayer::set_health; Life for P2's resolver")
e("p1-engage", "p1-sched")
e("p2-engage", "p2-derive")
e("s-ph0", "p1-sched", "phase 0 fires")
e("s-ph0", "p2-derive")
e("s-ph0", "s-taken", "same schedule")
e("p1-sched", "p1-aim")
e("p1-tracks", "p1-aim", "track -15 t")
e("p2-tracks", "p2-derive")
e("p1-aim", "p1-expand")
e("p1-expand", "p1-step")
e("p1-expand", "p2-digest", "same keys")
e("p2-derive", "p2-digest")
e("p1-step", "p1-bodies")
e("p1-tracks", "p1-bodies")
e("p1-bodies", "p1-contact")
e("p2-digest", "p2-contact")
e("p2-tracks", "p2-contact")
e("p1-contact", "p1-hold", "+12 t")
e("p1-hold", "p1-send")
e("p1-send", "s-batch", "`report_player_damaged`")
e("s-batch", "s-taken")
e("s-taken", "s-apply")
e("s-apply", "p1-hp", "`my_health`")
e("s-apply", "p2-hp", "`nearby_health`")

# forks of chapter 3
grp("G-cap", "p1", "Fork · bullet store full", "Fork - bullet store cap", "#7f9cff")
card("f-cap", "p1", 51, "P1 bullet store is full", "LIVE_BULLET_CAP 20000; rest of batch rejected in key order; HUD rej; the one place a corrected re-run differs",
     gy=6, main=False, group="G-cap", color="#7f9cff")
e("p1-expand", "f-cap")

grp("G-aoi", "p1", "Fork · P1 walks out of AoI / the leash (P1)", "Fork - P1 leaves the AoI ring or the leash", "3")
grp("G-aoi-s", "s", "Fork · P1 walks out of AoI / the leash (server)", "Fork - P1 leaves the AoI ring or the leash", "3")
card("f-a1", "p1", 54, "P1 walks out of the AoI ring", "view deletes the row -> EnemyRemove -> rederive away; P1 derives nothing", gy=3, main=False, group="G-aoi", color="3")
card("f-a2", "s", 57, "The Gorgon keeps P1 as its target", "aggro_pass keep rule: lock live, online, alive, within leash 32768", gy=3, main=False, group="G-aoi-s", color="3")
card("f-a4", "s", 60, "P1 leaves the leash", "keep fails; re-pick P2 (+60) or disengage (+12); ends", gy=3, main=False, group="G-aoi-s", color="3")
card("f-a3", "p1", 60, "P1 walks back into range", "row returns; body re-derived in place; bullets in flight appear", gy=3, main=False, group="G-aoi", color="3")
e("p1-step", "f-a1")
e("f-a1", "f-a2", "still targeted")
e("f-a2", "f-a4")
e("f-a2", "f-a3")
e("f-a3", "p1-hp", "back in")

grp("G-tie-p2", "p2", "Fork · clients disagree about a bullet", "Fork - clients disagree about a bullet", "6")
grp("G-tie", "s", "Fork · clients disagree (server)", "Fork - clients disagree about a bullet", "6")
card("f-c1", "p2", 60, "A black bullet on P2", "prediction vs undecided contact; black means might be gone", gy=3, main=False, group="G-tie-p2", color="6")
card("f-c2", "p2", 63, "P2 paths say P2 was hit first", "different samples, staircase F1, F3; P2 reports the same key", gy=3, main=False, group="G-tie-p2", color="6")
card("f-c3", "s", 66, "Server accepts both victims", "no cross-player rule; per-victim guard keys; F4 counter", gy=3, main=False, group="G-tie", color="6")
e("p2-contact", "f-c1")
e("f-c1", "f-c2")
e("f-c2", "f-c3", "`report_player_damaged`")
e("f-c3", "p2-hp", "both bars drop")

grp("G-taken", "s", "Fork · taken hits refused / too many", "Fork - taken hits refused", "1")
card("f-t1", "s", 75, "A taken hit is refused", "reason codes in order, hit_log, reducer Ok", gy=3, main=False, group="G-taken", color="1", section="S-taken")
card("f-t2", "s", 78, "Too many taken hits", "30 hits/s bucket; batch over 64 is Err", gy=3, main=False, group="G-taken", color="1", section="S-taken")
card("f-t3", "s", 81, "What a taken refusal costs", "no HP change, no event; bullet already consumed locally", gy=3, main=False, group="G-taken", color="1", section="S-taken")
e("s-taken", "f-t1")
e("f-t1", "f-t2")
e("f-t2", "f-t3")

grp("G-live", "s", "Fork · live hit check on", "Fork - live hit check", "6")
card("f-l1", "s", 75, "Live hit check is on", "admin switches; bullet_touches_path replays the bullet against movement_log", gy=6, main=False, group="G-live", color="6", section="S-taken")
card("f-l2", "s", 78, "Replay misses, off_path", "same function, no margin; F1 can refuse an honest victim", gy=6, main=False, group="G-live", color="6", section="S-taken")
e("s-taken", "f-l1")
e("f-l1", "f-l2")

# ---------------------------------------------------------------- 4. dealt
card("p2-fire", "p2", 78, "P2 fires the bow", "KgWeaponComponent fire action, refusals (build mode, GUI, control inactive), cursor -> binary angle")
card("p2-predict", "p2", 81, "P2 adds the arrows and sends report_fire",
     "KgCombatComponent::try_fire RTT <= 300 ms; CombatRuntime::try_fire cadence 15 ticks; arrows in P2's own world; NetSession::report_fire(fire_tick, origin, angle)")
card("p2-collide", "p2", 84, "P2 arrow hits the Gorgon", "CombatWorld collides player bullets with enemies alive at the tick; radius 768; non-pierce removed")
rules_grp("G-rules-fire", "rules.rs · fire claim")
card("s-fire", "s", 84, "Rule - accept_fire", "report_fire calls it: online, alive, weapon row and mode, position row; check_fire: window -30/+2, stamp (cadence), muzzle bound", group="G-rules-fire")
card("s-shotrec", "s", 87, "Server appends the ShotRec", "report_fire applies FireAccepted: ShotRec{fire_tick, weapon_id, mode, origin, angle} into recent_shots (16), last_fire_tick; volley never stored")
card("p2-hold", "p2", 87, "P2 holds and reports the dealt hit", "REPORT_HOLD 12; take_reports; flush_reports; EnemyHitReport{enemy, fire_tick, ordinal, hit_tick}")
card("p1-shots", "p1", 90, "P1 receives the shot ring", "nearby_weapons -> WeaponSeen -> ModelEvent::Weapon -> combat_rev -> sync_remote_shots -> Input::Shot; P2 skips its own row")
card("p1-expand-arrow", "p1", 93, "P1 expands the arrow from the shot ring", "volley_rng_player(P2, weapon, mode, fire_tick); same three arrows drawn")
rules_grp("G-rules-dealt", "rules.rs · player-bullet claim")
card("s-price", "s", 93, "Rule - accept_enemy_damaged", "report_enemy_damaged calls it: online, batch <= 64, alive, weapon row; sort; Strength, dealt_rate_ceiling, load_dealt_ledger; DealtBatch", group="G-rules-dealt")
card("p1-arrow", "p1", 96, "P1 collides the arrow without reporting", "P1's world collides P2's arrows too and removes them at contact, but only the shooter reports")
card("s-dealt", "s", 96, "Rule - DealtJudge.judge", "six checks: find_shot, score, dealt key, enemy, check_hit, dealt token; damage from the shot's mode", group="G-rules-dealt")
card("s-hp", "s", 99, "Server applies damage to the Gorgon", "report_enemy_damaged: DealtHit damage 0 -> apply_enemy_hit -> resolve 10-8=2 -> apply_enemy_hp 1000->998; recent_hits; DealtJudge::accepted / save; no shoot-back while locked")
card("p1-bar", "p1", 102, "P1 boss bar drops to 998", "nearby_enemy_health -> EnemyHealthSeen -> update_rows -> set_health + flash")
card("p2-bar", "p2", 102, "P2 boss bar drops to 998", "same bar; P2 skips its own recent_hits entries")
card("p1-consume", "p1", 105, "P1 consumes the arrow from recent_hits", "sync_consumed turns P2's DealtHit into Input::Consumed where P1's world missed the contact")
e("p2-hp", "p2-fire")
e("p2-fire", "p2-predict")
e("p2-predict", "s-fire", "`report_fire`")
e("p2-predict", "p2-collide")
e("s-fire", "s-shotrec")
e("s-shotrec", "p1-shots", "`nearby_weapons`")
e("p2-collide", "p2-hold", "+12 t")
e("p2-hold", "s-price", "`report_enemy_damaged`")
e("p1-shots", "p1-expand-arrow")
e("p1-expand-arrow", "p1-arrow")
e("s-price", "s-dealt")
e("s-shotrec", "s-dealt", "find_shot")
e("s-dealt", "s-hp")
e("s-hp", "p1-bar", "`nearby_enemy_health`")
e("s-hp", "p2-bar")
e("p1-bar", "p1-consume")
e("p1-arrow", "p1-consume")

grp("G-dc-p2", "p2", "Fork · P2 disconnects / times out", "Fork - P2 disconnects", "4")
grp("G-dc-s", "s", "Fork · P2 disconnects (server)", "Fork - P2 disconnects", "4")
grp("G-dc-p1", "p1", "Fork · P2 disconnects (P1)", "Fork - P2 disconnects", "4")
card("f-x1", "p2", 78, "P2 drops out", "closed socket -> client_disconnected; silent -> quiet_pass after 5 s", gy=3, main=False, group="G-dc-p2", color="4")
card("f-x2", "s", 84, "Server removes P2 and moves it to the spawn", "online_player deleted; disconnect nexus: a living P2 moves to the spawn slot (+ nexus mark if in danger); leaves aggro candidates", gy=6, main=False, group="G-dc-s", color="4")
card("f-x3", "p1", 87, "P2 puppet vanishes on P1", "nearby_players delete -> PlayerRemove -> puppet freed; not a body", gy=3, main=False, group="G-dc-p1", color="4")
card("f-x4", "p2", 90, "P2 reconnects", "Backoff; resubscribe replays rows; enter_world / wake_quiet", gy=3, main=False, group="G-dc-p2", color="4")
e("p2-hp", "f-x1")
e("f-x1", "f-x2")
e("f-x2", "f-x3")
e("f-x3", "f-x4")
e("f-x4", "p2-bar", "back in")

grp("G-phantom", "s", "Fork · refused report_fire (phantom arrow)", "Fork - refused report_fire", "1")
grp("G-phantom-p2", "p2", "Fork · phantom arrow (P2)", "Fork - refused report_fire", "1")
card("f-pa1", "s", 87, "Server refuses report_fire", "check_fire refusal codes; Err; ActionError on P2", gy=3, main=False, group="G-phantom", color="1")
card("f-pa2", "p2", 90, "P2 keeps a phantom arrow", "added locally before the call; hits refused no_shot", gy=6, main=False, group="G-phantom-p2", color="1")
e("s-fire", "f-pa1")
e("f-pa1", "f-pa2", "ActionError")

grp("G-lag", "p1", "Fork · P1 lags", "Fork - P1 lags", "4")
card("f-g1", "p1", 96, "P1 fire is held while the clock settles", "RTT gate; reports wait", gy=3, main=False, group="G-lag", color="4")
card("f-g2", "p1", 99, "Late rows trigger a correction", "rederive from tick T; held contacts retracted", gy=3, main=False, group="G-lag", color="4")
card("f-g3", "p1", 102, "A long stall restarts the world", "MAX_CATCHUP_TICKS; runtime restart", gy=3, main=False, group="G-lag", color="4")
e("p1-expand-arrow", "f-g1")
e("f-g1", "f-g2")
e("f-g2", "f-g3")
e("f-g3", "p1-consume", "caught up")

grp("G-dealt", "s", "Fork · enemy hits refused / too many", "Fork - dealt hits refused", "1")
card("f-d1", "s", 102, "A dealt hit is refused", "no_shot, unknown_weapon, duplicate, no_enemy, check_hit codes, rate_limited", gy=3, main=False, group="G-dealt", color="1")
card("f-d2", "s", 105, "What a dealt refusal costs", "arrow differs by screen; spectators keep it until life ends", gy=3, main=False, group="G-dealt", color="1")
e("s-dealt", "f-d1")
e("f-d1", "f-d2")

grp("G-lock", "s", "Fork · lock expiry and shoot-back", "Fork - lock expiry and shoot-back", "3")
grp("G-lock-p2", "p2", "Fork · shoot-back (P2)", "Fork - lock expiry and shoot-back", "3")
card("f-k1", "s", 102, "The target lock expires", "lock not renewed (F5); fight past t+1860", gy=6, main=False, group="G-lock", color="3")
card("f-k2", "s", 105, "Shoot-back retargets to P2", "apply_enemy_hit appends TargetSegment{P2} when lock expired", gy=6, main=False, group="G-lock", color="3")
card("f-k3", "p2", 108, "P2 becomes the target", "aimed emitters read P2's track", gy=3, main=False, group="G-lock-p2", color="3", section="S-dealt")
e("s-hp", "f-k1")
e("f-k1", "f-k2")
e("f-k2", "f-k3", "+60 t")

# ---------------------------------------------------------------- 5. phase change
grp("G-admin-hp", "s", "Fork · admin sets the HP", "Fork - admin sets the HP", "2")
card("f-adm", "s", 108, "An admin sets the Gorgon HP", "admin_set_enemy_hp: same sink apply_enemy_hp, can jump phases, refuses hp > max_hp",
     gy=9, main=False, group="G-admin-hp", color="2", section="S-phase")
card("s-ph1", "s", 111, "Server stamps phase 1", "hp <= 900: phase_for_hp, hp_floor, PhaseSegment{1} at max(now+12, last+1)")
card("p1-ph1", "p1", 114, "P1 sees phase 1 begin", "row update; segment_at answers phase 1 from the stamp; the timeline restarts with a rest")
card("p2-ph1", "p2", 114, "P2 sees phase 1 begin", "same stamp, same tick")
card("s-window", "s", 117, "Server refuses hits after the segment start", "enemy_hit_window: after_segment for phase 0 bullets hitting at/after the stamp")
card("p1-wipe", "p1", 117, "Both clients cull the bullets in flight", "CombatWorld cull at the segment start: every phase 0 bullet gone, no message")
card("p2-wipe", "p2", 117, "Both clients cull the bullets in flight", "same heading")
grp("G-skip", "s", "Fork · one hit skips phases", "Fork - one hit skips phases", "#7f9cff")
card("f-skip", "s", 114, "One hit crosses two thresholds", "one segment for the highest phase reached; hp_floor", gy=3, main=False, group="G-skip", color="#7f9cff")
e("s-hp", "s-ph1", "HP <= 900")
e("f-adm", "s-ph1", "same sink")
e("s-ph1", "p1-ph1", "+12 t lead")
e("s-ph1", "p2-ph1")
e("s-ph1", "s-window")
e("s-ph1", "f-skip")
e("p1-ph1", "p1-wipe")
e("p2-ph1", "p2-wipe")

# ---------------------------------------------------------------- 6. phases 1-9
card("s-phases", "s", 120, "Phases 1 to 9 in brief", "caption: each phase canvas, the HP thresholds, what repeats")
e("p1-wipe", "s-phases", "phase 1 running")
e("f-skip", "s-phases")
e("f-k3", "s-phases", "fight continues")
for k in range(1, 10):
    c = 123 + 6 * (k - 1)
    embed(f"s-p{k}", c, PHASE_FILES[k])
    h = f"Phase {k} at the players"
    card(f"p1-f{k}", "p1", c + 3, h, f"what phase {k} fires (gorgon.ron), aimed or not, movement; players take hits and hit back as in chapters 3 and 4")
    card(f"p2-f{k}", "p2", c + 3, h, "same heading")
    e(f"s-p{k}", f"p1-f{k}")
    e(f"s-p{k}", f"p2-f{k}")
    nxt = f"s-p{k + 1}" if k < 9 else "s-lethal"
    e(f"p1-f{k}", nxt, f"<= {900 - 100 * k}" if k < 9 else "HP 0")
    e(f"p2-f{k}", nxt)
e("s-phases", "s-p1")

grp("G-death-s", "s", "Fork · P1 dies and respawns (server)", "Fork - P1 dies and respawns", "2")
grp("G-death-p1", "p1", "Fork · P1 dies and respawns (P1)", "Fork - P1 dies and respawns", "2")
card("f-r1", "s", 138, "P1 takes the killing hit", "apply_player_hit kills (23rd hit at 115 HP, defence 5): dead_until DEAD_UNTIL_RESPAWN, died_tick, killed_by, log_death", gy=3, main=False, group="G-death-s", color="2")
card("f-r2", "p1", 141, "P1 sees the death panel", "Dead bar, You died panel", gy=3, main=False, group="G-death-p1", color="2")
card("f-r3", "s", 144, "The Gorgon retargets away from P1", "next pass drops P1; dead target aimed at up to 60 t", gy=3, main=False, group="G-death-s", color="2")
card("f-r4", "p1", 147, "P1 presses Respawn", "respawn reducer", gy=3, main=False, group="G-death-p1", color="2")
card("f-r5", "s", 150, "Server respawns P1", "respawn_player: starter kit again, recompute, full HP and mana at spawn slot", gy=3, main=False, group="G-death-s", color="2")
e("p1-f2", "f-r1", "killing hit")
e("f-r1", "f-r2")
e("f-r2", "f-r3")
e("f-r3", "f-r4")
e("f-r4", "f-r5", "`respawn`")
card("f-r6", "s", 141, "Server leaves P1 corpse", "corpse::on_death: corpse row with all gear at the death spot (300 s, loot_corpse); level, XP, stats, loadout wiped; CharacterDied to global", gy=6, main=False, group="G-death-s", color="2")
e("f-r1", "f-r6")
e("f-r6", "f-r5", "empty bag")
e("f-r5", "s-p6", "back in")

grp("G-patch-s", "s", "Fork · patch departure (server)", "Fork - patch departure", "5")
grp("G-patch-r", "r", "Fork · patch departure (relay)", "Fork - patch departure", "5")
grp("G-patch-p1", "p1", "Fork · patch departure (clients)", "Fork - patch departure", "5")
card("f-p1", "s", 156, "Global swaps the patch", "admin_swap_patches on kg-global", gy=3, main=False, group="G-patch-s", color="5")
card("f-p2", "r", 159, "Relay delivers PatchDepart", "kg-relay", main=False, group="G-patch-r", color="5")
card("f-p3", "s", 162, "Region handles the patch departure", "handle_patch_depart: delete_enemy, delete_player_rows", gy=3, main=False, group="G-patch-s", color="5")
card("f-p4", "r", 165, "Relay delivers PatchDeparted and PatchArrive", "kg-relay", main=False, group="G-patch-r", color="5")
card("f-p5", "p1", 171, "Clients transfer and lose the Gorgon", "Transferring; EnemyGone; fight ends without a death", gy=3, main=False, group="G-patch-p1", color="5")
e("s-p6", "f-p1")
e("f-p1", "f-p2")
e("f-p2", "f-p3")
e("f-p3", "f-p4")
e("f-p4", "f-p5")

grp("G-despawn", "s", "Fork · admin despawns the Gorgon", "Fork - admin despawns the Gorgon", "2")
card("f-ds", "s", 174, "An admin despawns the Gorgon", "admin_despawn_enemy: dead_tick = now + 12, refuses already dead; then the death path", gy=3, main=False, group="G-despawn", color="2", section="S-run")
e("s-p9", "f-ds")

# ---------------------------------------------------------------- 7. death
card("s-lethal", "s", 180, "Server stamps the death", "lethal hit: dead_tick = now + 12, once")
card("s-loot", "s", 183, "Server rolls loot for the contributors", "apply_enemy_hp died: loot::on_enemy_death (gorgon table, one roll per contributor >= 50 HP) -> loot_bag rows; kill XP")
card("p1-dead", "p1", 183, "P1 sees the Gorgon die", "dead_tick arrives; volleys culled at dead_tick; 600 ms fade")
card("p2-dead", "p2", 183, "P2 sees the Gorgon die", "same tick on P2")
card("s-late", "s", 186, "Server still accepts late reports", "row readable MAX_BULLET_LIFE_TICKS 720 so pre-death hit_ticks validate; after dead_tick refused")
card("s-clean", "s", 189, "Server deletes the Gorgon rows", "cleanup_dead in combat_pass: enemy_health then enemy")
card("p1-gone", "p1", 192, "P1 frees the Gorgon node", "view delete -> EnemyGone -> EnemyRemove -> node freed")
card("p2-gone", "p2", 192, "P2 frees the Gorgon node", "same")
e("f-ds", "p1-dead", "same death path")
e("f-ds", "p2-dead")
e("s-lethal", "p1-dead", "+12 t")
e("s-lethal", "p2-dead")
e("s-lethal", "s-loot")
e("s-loot", "s-late")
e("s-late", "s-clean", "+720 t")
e("s-clean", "p1-gone", "view delete")
e("s-clean", "p2-gone")
e("p1-dead", "p1-gone")
e("p2-dead", "p2-gone")

# accept_movement got its own column (21) in front of the track card: every
# later column moves right by one beat (3 units) so the rest keeps its spacing.
SHIFT_FROM, SHIFT = 21, 3
for n in nodes:
    if n["gx"] >= SHIFT_FROM and n["id"] != "s-accept-move":
        n["gx"] += SHIFT
SECTIONS = [
    {"id": "S-spawn", "label": "Spawning and observing", "until_gx": 15},
    {"id": "S-engage", "label": "Engagement: target and phase 0", "until_gx": 39},
    {"id": "S-taken", "label": "The boss fires and a player is hit", "until_gx": 75},
    {"id": "S-dealt", "label": "A player hits the boss", "until_gx": 105},
    {"id": "S-phase", "label": "The first phase change", "until_gx": 117},
    {"id": "S-run", "label": "Phases 1 to 9", "until_gx": 177},
    {"id": "S-death", "label": "Death and cleanup", "until_gx": 195},
]
for sec in SECTIONS:
    if sec["until_gx"] >= SHIFT_FROM:
        sec["until_gx"] += SHIFT

spec = {
    "doc": DOC,
    "lane_gap": 2,
    "align_lanes": True,
    "lanes": [
        {"id": "L-server", "label": "Server Table"},
        {"id": "L-relay", "label": "Relay"},
        {"id": "L-p1", "label": "Player Client 1"},
        {"id": "L-p2", "label": "Player Client 2"},
    ],
    "sections": SECTIONS,
    "groups": groups,
    "nodes": nodes,
    "edges": edges,
}
with open(sys.argv[1], "w", encoding="utf-8", newline="\n") as f:
    f.write(json.dumps(spec, ensure_ascii=False, indent=1) + "\n")
print(len(nodes), "nodes", len(edges), "edges", len(groups), "groups")

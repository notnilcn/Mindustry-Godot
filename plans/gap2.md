Campaign UI audit — NEW gaps (ledger EV-0001..0034 not re-reported)
Ranked, campaign blockers first.
GAP 1: Research purchase is a dead signal — tech tree cannot be advanced
- severity: S1
- port: client/ui/dialogs/research_dialog.gd:13 (signal research_requested), :90 (row.pressed.connect(_confirm)), :103-104 (_confirm emits); no listener anywhere in client/ (grep); backing action client/rust/mind-gdext/src/campaign.rs:238 (MindCampaign.research) is never called from GDScript
- upstream: core/src/mindustry/ui/dialogs/ResearchDialog.java:484-486 (canSpend && locked -> spend(node)), :567-606 (spend: removes sector items, accumulates finishedRequirements), :608-625 (unlock + ResearchEvent)
- symptom: the screen lists tech nodes for one root with requirement tooltips and lock state, but clicking any locked/ready node does nothing at all; no items are spent, nothing unlocks, no feedback. Nodes with unmet requirements are disabled (:88), so even upstream's partial-contribution spend is impossible. Node rows show raw content ids and no icons, and because MindDialog.campaign_views() caches once per dialog (mind_dialog.gd:206-215) the list would not refresh after an unlock anyway.
- campaign: blocks (research is the only block/unlock progression path)
- confidence: high
GAP 2: LaunchLoadoutDialog is unreachable, its signal is unconnected, and launches apply no loadout
- severity: S2
- port: client/ui/dialogs/launch_loadout_dialog.gd:12,48 (emits loadout_chosen, no listener); no open_dialog("launch_loadout") anywhere (grep); planet_dialog.gd:536-551 _launch() bypasses it and calls start_sector directly; client/rust/mind-gdext/src/campaign.rs:142-151 calls play_new_sector with no loadout input; client/rust/mind-core/src/game/play.rs:363-368 leaves the starting-loadout application as a no-op intent
- upstream: PlanetDialog.java:1395-1444 (for a base launch candidate opens loadouts.show(block, from, sector, confirm) and removes loadout requirements + getLaunchResources() on confirm), LaunchLoadoutDialog.java:36-221 (schematic grid, capacity launch.capacity, @resources.max toggle, per-item (already + adding) totals, @launch.text calling universe.updateLoadout(core, selected), @sector.missingresources gating)
- symptom: launching never asks which core schematic/resources to bring; no capacity or resource totals; no "missing resources" state; the choice could not be applied even if made. _selected_core() is also wrong (returns the planet name) and unused (launch_loadout_dialog.gd:30-32).
- campaign: degrades (removes a core launch mechanic; fresh-sector entry itself still works)
- confidence: high
GAP 3: Sector panel omits stats/objectives/attack state and can never Resume or Go
- severity: S2
- port: client/ui/dialogs/planet_dialog.gd:330-372 (_build_sector_panel renders only name, preset icon, accent divider, @sectors.threat, and a Launch button), :367-371 (label always @sectors.launch, disabled only by locked), :536-551 (_launch always calls start_sector, i.e. a fresh play_new_sector); the view model already carries attacked/frozen/shielded/has_enemy_base/captured/has_base/has_save/capture_wave (client/rust/mind-core/src/ui/campaign.rs:124-141) but none is read
- upstream: PlanetDialog.java:1284-1347 (locked -> incomplete objective list; @sectors.underattack / @sectors.vulnerable / @sectors.enemybase indicators; resource icons; action label select/resume/go/locked/nolaunchcandidate/launch with hasNoCandidate gating), :1055-1147 (showStats: playtime, attempts, wave, threat, production/export/import, stored items, abandon), :1368-1453 (playSelected resume/go paths)
- symptom: captured/attacked sectors are indistinguishable; no stats/abandon UI; the button says Launch even for a sector with a base and relaunches it fresh instead of resuming/go-to-base
- campaign: degrades
- confidence: high
GAP 4: Campaign rules dialog is inert and reads no current values
- severity: S2
- port: client/ui/dialogs/campaign_rules_dialog.gd:12 (rule_changed, no listener), :15 (only easy/normal/hard vs 5 upstream difficulties), :17 (vanilla toggle keys; missing rtsai, clearsectoronloss; Rust keys are sector_invasion/hide_spawns/random_wave_ai, not invasions/hidespawns/randomwaveai), :47-52 reads _context["difficulty"/"rules"] but the only opener passes "{}" (planet_dialog.gd:567-570); the Rust rules view (rules_view, client/rust/mind-core/src/ui/campaign.rs:630-662) is never consumed; gdext exposes no campaign-rules setter (only session set_rules_json, client/rust/mind-gdext/src/campaign.rs:220-234) though core CampaignRules::apply/write exist (client/rust/mind-core/src/game/campaign_rules.rs:222-274)
- upstream: CampaignRulesDialog.java:22-31 (hidden -> planet.saveRules() + planet.campaignRules.apply(planet, state.rules) + Call.setRules), :53-80 (all Difficulty.all; conditional invasion/rtsai/clearsectoronloss toggles)
- symptom: choosing a difficulty or toggling rules appears to do nothing; reopening resets to Normal/all-false because context is empty and nothing persists or applies
- campaign: degrades
- confidence: high
GAP 5: Campaign completion screen is never shown; capture never detects the last sector
- severity: S3
- port: client/ui/dialogs/campaign_complete_dialog.gd (shell only; no @menu/exit-save button, only @continue at :20); no code ever opens campaign_complete (grep: only dialogs_manifest.json:25 and pause tables); sector_capture (client/rust/mind-core/src/game/play.rs:488-513) never checks is_last, even though SectorView.is_last exists (ui/campaign.rs:139)
- upstream: core/src/mindustry/core/Control.java:177-185 (SectorCaptureEvent + sector.preset.isLastSector && initialCapture -> two-second delay -> ui.campaignComplete.show(e.sector.planet)), CampaignCompleteDialog.java:22-44 (@menu runExitSave + @continue, planet color markup, summed playtime)
- symptom: capturing the final sector produces no victory banner and no exit-to-menu path; the view model's complete data (campaign.rs:664-693) is never displayed
- campaign: degrades
- confidence: high (dialog reachability); medium (whether any ported planet currently reaches is_last)
GAP 6: ContentInfo dialog is unreachable; in-game block/unit info affordance absent
- severity: S3
- port: client/ui/dialogs/content_info_dialog.gd:16-45 exists and is registered as "content" (dialogs_manifest.json:10) but no open_dialog("content") call exists anywhere; fragments/placement_fragment.gd has no info hook; database_dialog.gd renders only a placeholder (EV-0028)
- upstream: PlacementFragment.java:262,268,392 (ui.content.show(unit.type()) / hovered block), ResearchDialog.java:668 (info button on selectable nodes), DatabaseDialog.java:212, CoreBlock.java:139; ContentInfoDialog.java:35-135 computes stats, purpose/category sections, details and patched indicator
- symptom: no way to open the info dialog for a block/unit from the HUD or from database/research rows; content stats/descriptions are not viewable in campaign
- campaign: degrades
- confidence: high (reachability); note port dialog is context-only and has no stats computation even if reached
GAP 7: LoadoutDialog is a summary list, not the capacity editor, and is dead
- severity: S4
- port: client/ui/dialogs/loadout_dialog.gd:33-51 lists loadout names/requirements plus a @loadout.select button that emits unconnected loadout_changed; no open_dialog("loadout") anywhere; _capacity computed but unused
- upstream: LoadoutDialog.java:82-123 per-item -/+/pencil stepping with capacity bounds, @max/@settings.reset buttons; opened from LaunchLoadoutDialog.java:131-142 and the schematics flow
- symptom: no way to edit a core loadout's item amounts from any screen
- campaign: degrades
- confidence: high
GAP 8: SectorSelectDialog is dead and does not filter to unlockable sectors
- severity: S4
- port: client/ui/dialogs/sector_select_dialog.gd:12,34-38 (emits sector_chosen, no listener; no open_dialog("sector_select") anywhere); layout/row_tree_layout.gd:31-35 lists every sector from the callback?
- upstream: SectorSelectDialog.java:64-82 is internal (LaunchPad.java:192, Accelerator.java:263 via PlanetDialog) and filters sector.planet == planet && sector.requireUnlock with search
- symptom: no sector-destination picker for launch-pad/accelerator style flows; if used as-is it would list all sectors rather than unlockable ones
- campaign: n-a / degrades
- confidence: high (dead); medium (player impact while launch pads are unported)
Coverage notes — what genuinely works
- Planet chooser (Play > Campaign) renders Serpulo/Erekir first-run cards from the vendored planet art, dynamic description, and OK-disabled-until-selected (verified under EV-0011).
- Sector view (EV-0010/EV-0016): planet_view.gd builds the SubViewport globe, grid/selection meshes, pick/highlight, in-world label; the planet rail switches Erekir/Serpulo and opens the rules dialog; the in-panel Launch genuinely calls SimHost.load_sector + MindCampaign.start_sector and enters the game with the core centered (EV-0016 fixed).
- Research screen is reachable (planet_dialog.gd:251,561-564) and renders real campaign_views().research rows per root with unlock/ready state and requirement tooltips; the Rust spend implementation (MindCampaign.research, tech_tree::spend) and objectives context are present and unit-tested — only the UI wiring is missing.
- Explicit answer: the research/tech-tree screen is NOT playable. Nodes are read-only text rows; the purchase path (research_requested) is connected to nothing, so no research can ever complete through the UI.
- Explicit answer: launch loadout selection is NOT playable. The dialog is never opened (no caller), its loadout_chosen signal has no listener, and the launch path has no loadout parameter at all (play_new_sector), so no selection could be applied.
- Sector panel does display selected-sector name, preset icon, threat band, and correctly disables Launch for locked sectors.
- Campaign rules dialog opens from the planet rail and renders difficulty toggles plus MindWidgets.check rows (key set is stale, values are ignored).
- Rust side has complete read models: SectorView carries attacked/frozen/enemy-base/captured/save fields and CampaignRules has apply/read/write with tests — the gaps above are UI wiring/consumption, not missing data.
- The campaign-complete shell and its CampaignCompleteView (planet, color, playtime, captured/waves, next planet) exist, so the missing piece is the trigger + opener.
- Sub-aspect of EV-0028 (not re-filed): the Database search field is a no-op — database_dialog.gd:65-74 computes query then ignores it with pass; tabs/planet filtering remain as filed.

// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MapObjectives` runtime executor (plan 12 M6).
//!
//! Ported from `core/src/mindustry/game/MapObjectives.java`. The **persistence
//! shape** of every objective (all 13 classes, their fields, camelCase JSON and
//! the class-tag registry) already lives in plan 04's
//! [`crate::io::json::objectives`]; this module owns the *runtime*:
//! `update()`/`done()`/`qualified()`/`dependency_finished()`, the transient
//! `Timer` accumulator, the `flagsAdded`/`flagsRemoved` application and the
//! `completionLogicCode` hook into plan 13's `run_logic_script`.
//!
//! Transient per-objective state (`completed`, `depFinished`, the timer
//! count-up) is kept in [`MapObjectivesRuntime`] rather than on the plan-04
//! structs: upstream marks those fields `transient`, and the plan-04 shape is
//! also the persisted `Rules.objectives` JSON, so completion state must not be
//! serialized. This is the plan §3.8 “`MapObjectives` resource”; the name
//! carries the `Runtime` suffix to avoid colliding with the plan-04 type.

use std::collections::BTreeSet;

use crate::io::json::objectives::{
    BuildCountObjective, CoreItemObjective, DestroyBlockObjective, DestroyBlocksObjective,
    DestroyUnitsObjective, FlagObjective, ItemObjective, MapObjective,
    MapObjectives as ObjectiveData, TimerObjective, UnitCountObjective,
};
use crate::logic::script::{SCRIPT_DEFAULT_MAX_INSTRUCTIONS, run_logic_script};

use super::rules::Rules;

/// Objective class tags in registration order (`MapObjectives.registerObjective`).
pub const ALL_OBJECTIVE_TYPE_NAMES: [&str; 13] = [
    "Research",
    "Produce",
    "Item",
    "CoreItem",
    "BuildCount",
    "UnitCount",
    "DestroyUnits",
    "Timer",
    "DestroyBlock",
    "DestroyBlocks",
    "DestroyCore",
    "CommandMode",
    "Flag",
];

/// Localization hooks for objective/marker text (plan 12 R9; plan 19 owns
/// `MapLocales`, plan 14 owns the bundle). The default is a pass-through.
pub trait ObjectiveLocale {
    /// `ObjectiveMarker.fetchText`: resolves `@`-prefixed bundle/map keys.
    fn fetch_text(&self, text: &str) -> String;
    /// `Core.bundle.format(key, args)`.
    fn format(&self, key: &str, args: &[&str]) -> String;
    /// `Core.bundle.get(key)`.
    fn get(&self, key: &str) -> String;
    /// `MapLocales.containsProperty(key)` → localized value (plan 19).
    fn map_locale(&self, _key: &str) -> Option<String> {
        None
    }
}

/// Pass-through locale used by headless tests and until plan 14 wires a bundle.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoLocale;

impl ObjectiveLocale for NoLocale {
    fn fetch_text(&self, text: &str) -> String {
        match text.strip_prefix('@') {
            Some(stripped) => stripped.to_owned(),
            None => text.to_owned(),
        }
    }

    fn format(&self, key: &str, args: &[&str]) -> String {
        if args.is_empty() {
            key.to_owned()
        } else {
            format!("{key} {}", args.join(" "))
        }
    }

    fn get(&self, key: &str) -> String {
        key.to_owned()
    }
}

/// The live state an objective reads (`state.stats`/`TeamData`/`world`).
///
/// Kept as a trait so objectives are testable without a world; the ECS-backed
/// implementation is the thin adapter joint with plans 06/07/08/11.
pub trait ObjectiveEnv {
    /// `UnlockableContent.unlocked()` by content name (Research/Produce).
    fn is_content_unlocked(&self, content: &str) -> bool;
    /// `state.rules.defaultTeam.items().has(item, amount)` (Item).
    fn team_has_item(&self, team: u8, item: &str, amount: i32) -> bool;
    /// `state.stats.coreItemCount.get(item)` (CoreItem).
    fn core_item_count(&self, item: &str) -> i32;
    /// `state.stats.placedBlockCount.get(block, 0)` (BuildCount).
    fn placed_block_count(&self, block: &str) -> i32;
    /// `state.rules.defaultTeam.data().countType(unit)` (UnitCount).
    fn unit_count(&self, team: u8, unit: &str) -> i32;
    /// `state.stats.enemyUnitsDestroyed` (DestroyUnits).
    fn enemy_units_destroyed(&self) -> i32;
    /// `state.rules.objectiveFlags.contains(flag)` (Flag).
    fn objective_flag(&self, flag: &str) -> bool;
    /// `state.rules.waveTeam.cores().size` (DestroyCore).
    fn core_count(&self, team: u8) -> usize;
    /// `world.build(x, y)` as `(block name, team)` (DestroyBlock(s)).
    fn block_at(&self, x: i32, y: i32) -> Option<(&str, u8)>;
    /// `headless` global (CommandMode always completes headless).
    fn headless(&self) -> bool;
    /// `control.input.selectedUnits` has a commandable, commanded unit
    /// (CommandMode client half; plan 15 wires the selector).
    fn command_mode_satisfied(&self) -> bool;
}

/// Rules-derived parameters passed to [`MapObjectivesRuntime::update`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectiveRunParams {
    /// `rules.defaultTeam`.
    pub default_team: u8,
    /// `rules.waveTeam`.
    pub wave_team: u8,
    /// `rules.objectiveTimerMultiplier`.
    pub timer_multiplier: f32,
}

impl Default for ObjectiveRunParams {
    fn default() -> Self {
        Self {
            default_team: 1,
            wave_team: 2,
            timer_multiplier: 1.0,
        }
    }
}

/// The runtime objective executor (plan §3.8 `MapObjectives` resource).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapObjectivesRuntime {
    /// Persisted objective data, in executor order (parents are indices).
    pub data: ObjectiveData,
    /// `MapObjective.completed`, indexed parallel to `data.all`.
    completed: Vec<bool>,
    /// `MapObjective.depFinished`, indexed parallel to `data.all`.
    dep_finished: Vec<bool>,
    /// `TimerObjective.countup`, indexed parallel to `data.all`.
    countup: Vec<f32>,
}

impl MapObjectivesRuntime {
    /// Empty executor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Wraps persisted objective data, allocating fresh transient state.
    pub fn from_data(data: ObjectiveData) -> Self {
        let len = data.all.len();
        Self {
            data,
            completed: vec![false; len],
            dep_finished: vec![false; len],
            countup: vec![0.0; len],
        }
    }

    /// Wraps the current `Rules.objectives` data.
    pub fn from_rules(rules: &Rules) -> Self {
        Self::from_data(rules.objectives.clone())
    }

    /// The persisted objective data (write back to `Rules.objectives`).
    pub fn data(&self) -> &ObjectiveData {
        &self.data
    }

    /// Mutable persisted objective data (editor/relay `setObjectives`).
    pub fn data_mut(&mut self) -> &mut ObjectiveData {
        &mut self.data
    }

    /// Consumes the runtime, returning the persisted data.
    pub fn into_data(self) -> ObjectiveData {
        self.data
    }

    /// Number of objectives.
    pub fn len(&self) -> usize {
        self.data.all.len()
    }

    /// Whether the executor is empty.
    pub fn is_empty(&self) -> bool {
        self.data.all.is_empty()
    }

    /// `MapObjectives.get(index)`.
    pub fn get(&self, index: usize) -> Option<&MapObjective> {
        self.data.all.get(index)
    }

    /// Whether an objective index has been completed.
    pub fn is_completed(&self, index: usize) -> bool {
        self.completed.get(index).copied().unwrap_or(false)
    }

    /// `MapObjective.qualified()`: not completed and all parents done.
    pub fn qualified(&self, index: usize) -> bool {
        !self.is_completed(index) && self.dependency_finished(index)
    }

    /// `MapObjective.dependencyFinished()`; caches `depFinished` on success.
    pub fn dependency_finished(&self, index: usize) -> bool {
        if self.dep_finished.get(index).copied().unwrap_or(false) {
            return true;
        }
        let Some(objective) = self.data.all.get(index) else {
            return false;
        };
        objective
            .common()
            .parents
            .iter()
            .all(|parent| self.is_completed(*parent))
    }

    /// `MapObjectives.any()`: any qualified objective exists.
    pub fn any(&self) -> bool {
        (0..self.len()).any(|index| self.qualified(index))
    }

    /// `MapObjectives.clear()` (`Call.clearObjectives`).
    pub fn clear(&mut self) {
        self.data.all.clear();
        self.completed.clear();
        self.dep_finished.clear();
        self.countup.clear();
    }

    /// Adds root objectives (upstream `add`; plan-04 data has no child links).
    pub fn add(&mut self, objectives: impl IntoIterator<Item = MapObjective>) {
        for objective in objectives {
            self.data.all.push(objective);
            self.completed.push(false);
            self.dep_finished.push(false);
            self.countup.push(0.0);
        }
    }

    /// Inserts the plan-04 objective at `index` with fresh transient state.
    pub fn insert(&mut self, index: usize, objective: MapObjective) {
        self.data.all.insert(index, objective);
        self.completed.insert(index, false);
        self.dep_finished.insert(index, false);
        self.countup.insert(index, 0.0);
    }

    /// Indices of every qualified objective, in order.
    pub fn running_indices(&self) -> Vec<usize> {
        (0..self.len())
            .filter(|index| self.qualified(*index))
            .collect()
    }

    /// Iterates qualified objectives (`eachRunning`).
    pub fn each_running(&self, mut cons: impl FnMut(usize, &MapObjective)) {
        for index in self.running_indices() {
            cons(index, &self.data.all[index]);
        }
    }

    /// `MapObjective.reset()` for every objective (timer state).
    pub fn reset(&mut self) {
        self.countup.iter_mut().for_each(|value| *value = 0.0);
    }

    /// Runs one update pass (`MapObjectives.update`).
    ///
    /// Returns the indices whose `update()` reported completion, in order. The
    /// **host** applies them through [`Self::complete`]; clients must wait for
    /// the relayed completion instead (plan §3.10 invariant 7).
    pub fn update(
        &mut self,
        env: &dyn ObjectiveEnv,
        params: &ObjectiveRunParams,
        delta_seconds: f32,
    ) -> Vec<usize> {
        let mut completed = Vec::new();
        for index in 0..self.len() {
            if !self.qualified(index) {
                continue;
            }
            let done = {
                let timer = &mut self.countup[index];
                objective_update(&self.data.all[index], timer, env, params, delta_seconds)
            };
            if done {
                completed.push(index);
            }
        }
        completed
    }

    /// `Call.completeObjective(index)` host function: applies `done()`.
    ///
    /// Ported from `NetClient.completeObjective`: a null/unknown or already
    /// completed index is ignored. Returns whether the objective transitioned.
    pub fn complete(&mut self, index: usize, rules: &mut Rules) -> bool {
        if index >= self.len() || self.is_completed(index) {
            return false;
        }
        let flags_removed = self.data.all[index].common().flags_removed.clone();
        let flags_added = self.data.all[index].common().flags_added.clone();
        let logic_code = self.data.all[index].common().completion_logic_code.clone();

        for flag in &flags_removed {
            rules.objective_flags.shift_remove(flag);
        }
        for flag in &flags_added {
            rules.objective_flags.insert(flag.clone());
        }
        self.completed[index] = true;
        self.dep_finished[index] = true;

        if let Some(code) = logic_code.as_deref() {
            let _ = run_logic_script(code, SCRIPT_DEFAULT_MAX_INSTRUCTIONS, false);
        }
        true
    }

    /// `complete_objective { index, rules_epoch }` idempotent relay target.
    ///
    /// Validates the epoch against `expected_epoch` before applying.
    pub fn complete_checked(
        &mut self,
        index: usize,
        expected_epoch: u32,
        command_epoch: u32,
        rules: &mut Rules,
    ) -> Result<bool, ObjectiveRelayError> {
        if command_epoch != expected_epoch {
            return Err(ObjectiveRelayError::EpochMismatch {
                expected: expected_epoch,
                got: command_epoch,
            });
        }
        Ok(self.complete(index, rules))
    }

    /// `Objective.typeName()` / `text()` / `details()` / `validate()` helpers.
    pub fn type_name(&self, index: usize) -> Option<String> {
        self.data
            .all
            .get(index)
            .map(|objective| objective_type_key(objective.class_tag()))
    }

    /// `Objective.text()` (`None` = fall back to the type name).
    pub fn text(&self, index: usize, locale: &dyn ObjectiveLocale) -> Option<String> {
        let objective = self.data.all.get(index)?;
        objective_text(objective, locale)
    }

    /// `Objective.details()`.
    pub fn details(&self, index: usize) -> Option<&str> {
        self.data
            .all
            .get(index)
            .and_then(|objective| objective.common().details.as_deref())
    }

    /// `Objective.validate()`: fills null content fields with defaults.
    pub fn validate(&mut self, index: usize) {
        if let Some(objective) = self.data.all.get_mut(index) {
            objective_validate(objective);
        }
    }

    /// Validates every objective.
    pub fn validate_all(&mut self) {
        for index in 0..self.len() {
            self.validate(index);
        }
    }
}

/// Relay validation failure for `complete_objective`/`clear_objectives`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ObjectiveRelayError {
    /// The client's rules epoch does not match the host's.
    #[error("rules epoch mismatch: expected {expected}, got {got}")]
    EpochMismatch {
        /// Host epoch.
        expected: u32,
        /// Command epoch.
        got: u32,
    },
    /// The objective index is out of range.
    #[error("objective index {0} out of range")]
    IndexOutOfRange(usize),
}

/// Evaluates one objective (`MapObjective.update()`).
fn objective_update(
    objective: &MapObjective,
    timer: &mut f32,
    env: &dyn ObjectiveEnv,
    params: &ObjectiveRunParams,
    delta_seconds: f32,
) -> bool {
    match objective {
        MapObjective::Research(o) => o
            .content
            .as_deref()
            .is_some_and(|content| env.is_content_unlocked(content)),
        MapObjective::Produce(o) => o
            .content
            .as_deref()
            .is_some_and(|content| env.is_content_unlocked(content)),
        MapObjective::Item(o) => item_objective(o, env, params),
        MapObjective::CoreItem(o) => core_item_objective(o, env),
        MapObjective::BuildCount(o) => build_count_objective(o, env),
        MapObjective::UnitCount(o) => unit_count_objective(o, env, params),
        MapObjective::DestroyUnits(o) => destroy_units_objective(o, env),
        MapObjective::Timer(o) => timer_objective(o, timer, params, delta_seconds),
        MapObjective::DestroyBlock(o) => destroy_block_objective(o, env),
        MapObjective::DestroyBlocks(o) => destroy_blocks_objective(o, env),
        MapObjective::CommandMode(_) => env.headless() || env.command_mode_satisfied(),
        MapObjective::Flag(o) => env.objective_flag(&o.flag),
        MapObjective::DestroyCore(_) => env.core_count(params.wave_team) == 0,
    }
}

fn item_objective(
    objective: &ItemObjective,
    env: &dyn ObjectiveEnv,
    params: &ObjectiveRunParams,
) -> bool {
    objective
        .item
        .as_deref()
        .is_some_and(|item| env.team_has_item(params.default_team, item, objective.amount.max(0)))
}

fn core_item_objective(objective: &CoreItemObjective, env: &dyn ObjectiveEnv) -> bool {
    objective
        .item
        .as_deref()
        .is_some_and(|item| env.core_item_count(item) >= objective.amount)
}

fn build_count_objective(objective: &BuildCountObjective, env: &dyn ObjectiveEnv) -> bool {
    objective
        .block
        .as_deref()
        .is_some_and(|block| env.placed_block_count(block) >= objective.count)
}

fn unit_count_objective(
    objective: &UnitCountObjective,
    env: &dyn ObjectiveEnv,
    params: &ObjectiveRunParams,
) -> bool {
    objective
        .unit
        .as_deref()
        .is_some_and(|unit| env.unit_count(params.default_team, unit) >= objective.count)
}

fn destroy_units_objective(objective: &DestroyUnitsObjective, env: &dyn ObjectiveEnv) -> bool {
    env.enemy_units_destroyed() >= objective.count
}

fn timer_objective(
    objective: &TimerObjective,
    timer: &mut f32,
    params: &ObjectiveRunParams,
    delta_seconds: f32,
) -> bool {
    *timer += delta_seconds;
    *timer >= objective.duration * params.timer_multiplier
}

fn destroy_block_objective(objective: &DestroyBlockObjective, env: &dyn ObjectiveEnv) -> bool {
    match env.block_at(objective.pos.x, objective.pos.y) {
        None => true,
        Some((block, team)) => team != objective.team || Some(block) != objective.block.as_deref(),
    }
}

fn destroy_blocks_objective(objective: &DestroyBlocksObjective, env: &dyn ObjectiveEnv) -> bool {
    destroy_blocks_progress(objective, env) >= objective.positions.len() as i32
}

/// `DestroyBlocksObjective.progress()`.
pub fn destroy_blocks_progress(objective: &DestroyBlocksObjective, env: &dyn ObjectiveEnv) -> i32 {
    objective
        .positions
        .iter()
        .filter(|pos| match env.block_at(pos.x, pos.y) {
            None => true,
            Some((block, team)) => {
                team != objective.team || Some(block) != objective.block.as_deref()
            }
        })
        .count() as i32
}

/// `Objective.typeName()`: bundle key used by plan 14/19.
pub fn objective_type_key(class_tag: &str) -> String {
    format!("objective.{}.name", class_tag.to_ascii_lowercase())
}

/// `Objective.text()` for every class, using the locale hooks.
fn objective_text(objective: &MapObjective, locale: &dyn ObjectiveLocale) -> Option<String> {
    match objective {
        MapObjective::Research(o) => {
            Some(locale.format("objective.research", &[o.content.as_deref().unwrap_or("")]))
        }
        MapObjective::Produce(o) => {
            Some(locale.format("objective.produce", &[o.content.as_deref().unwrap_or("")]))
        }
        MapObjective::Item(o) => Some(locale.format(
            "objective.item",
            &[o.item.as_deref().unwrap_or(""), &o.amount.to_string()],
        )),
        MapObjective::CoreItem(o) => Some(locale.format(
            "objective.coreitem",
            &[o.item.as_deref().unwrap_or(""), &o.amount.to_string()],
        )),
        MapObjective::BuildCount(o) => Some(locale.format(
            "objective.build",
            &[o.block.as_deref().unwrap_or(""), &o.count.to_string()],
        )),
        MapObjective::UnitCount(o) => Some(locale.format(
            "objective.buildunit",
            &[o.unit.as_deref().unwrap_or(""), &o.count.to_string()],
        )),
        MapObjective::DestroyUnits(o) => {
            Some(locale.format("objective.destroyunits", &[&o.count.to_string()]))
        }
        MapObjective::Timer(o) => timer_text(o, locale),
        MapObjective::DestroyBlock(o) => Some(locale.format(
            "objective.destroyblock",
            &[o.block.as_deref().unwrap_or("")],
        )),
        MapObjective::DestroyBlocks(o) => Some(locale.format(
            "objective.destroyblocks",
            &[
                o.block.as_deref().unwrap_or(""),
                &o.positions.len().to_string(),
            ],
        )),
        MapObjective::CommandMode(_) => Some(locale.get("objective.command")),
        MapObjective::Flag(o) => flag_text(o, locale),
        MapObjective::DestroyCore(_) => Some(locale.get("objective.destroycore")),
    }
}

/// `TimerObjective.text()` (exact minute/second formatting + locale lookup).
fn timer_text(o: &TimerObjective, locale: &dyn ObjectiveLocale) -> Option<String> {
    let text = o.text.as_deref()?;
    // `countup` is not available here; upstream shows remaining time using the
    // live accumulator, which plan 14 reads through its own state. We format
    // the full duration as the stable fallback (deterministic, UI-independent).
    let seconds = o.duration.max(0.0) as i32;
    let m = seconds / 60;
    let s = seconds % 60;
    let time_string = if m > 0 {
        format!("{m}:{s:02}")
    } else {
        format!("{s}")
    };
    if let Some(key) = text.strip_prefix('@') {
        if let Some(localized) = locale.map_locale(key) {
            return Some(localized);
        }
        // The 'escelating' typo was fixed in bundles+maps; existing saves keep
        // the old key, so it is remapped here.
        let key = if key == "objective.enemyescelating" {
            "objective.enemyescalating"
        } else {
            key
        };
        return Some(locale.format(key, &[&time_string]));
    }
    Some(locale.format(text, &[&time_string]))
}

/// `FlagObjective.text()`.
fn flag_text(o: &FlagObjective, locale: &dyn ObjectiveLocale) -> Option<String> {
    let text = o.text.as_deref()?;
    if let Some(key) = text.strip_prefix('@') {
        if let Some(localized) = locale.map_locale(key) {
            return Some(localized);
        }
        return Some(locale.get(key));
    }
    Some(text.to_owned())
}

/// `Objective.validate()` default-fills for null content references.
fn objective_validate(objective: &mut MapObjective) {
    match objective {
        MapObjective::Research(o) => {
            o.content.get_or_insert_with(|| "copper".to_owned());
        }
        MapObjective::Produce(o) => {
            o.content.get_or_insert_with(|| "copper".to_owned());
        }
        MapObjective::Item(o) => {
            o.item.get_or_insert_with(|| "copper".to_owned());
        }
        MapObjective::CoreItem(o) => {
            o.item.get_or_insert_with(|| "copper".to_owned());
        }
        MapObjective::BuildCount(o) => {
            o.block.get_or_insert_with(|| "conveyor".to_owned());
        }
        MapObjective::UnitCount(o) => {
            o.unit.get_or_insert_with(|| "dagger".to_owned());
        }
        MapObjective::DestroyBlock(o) => {
            o.block.get_or_insert_with(|| "router".to_owned());
        }
        MapObjective::DestroyBlocks(o) => {
            o.block.get_or_insert_with(|| "router".to_owned());
        }
        _ => {}
    }
}

/// `Call.completeObjective` relay ids already applied (idempotence guard).
#[derive(Debug, Default, Clone)]
pub struct CompletedObjectives(pub BTreeSet<usize>);

impl CompletedObjectives {
    /// Records `index`, returning `true` the first time.
    pub fn mark(&mut self, index: usize) -> bool {
        self.0.insert(index)
    }

    /// Clears the guard (`clear_objectives`).
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::io::json::objectives::{MapObjectives, Point2};

    /// Configurable env covering every objective predicate.
    #[derive(Default)]
    struct FakeEnv {
        unlocked: Vec<String>,
        default_items: BTreeSet<String>,
        core_items: BTreeMap<String, i32>,
        placed: BTreeMap<String, i32>,
        unit_counts: BTreeMap<String, i32>,
        enemy_units_destroyed: i32,
        flags: BTreeSet<String>,
        cores: usize,
        block: Option<(String, u8)>,
        headless: bool,
        command: bool,
    }

    use std::collections::BTreeMap;

    impl ObjectiveEnv for FakeEnv {
        fn is_content_unlocked(&self, content: &str) -> bool {
            self.unlocked.iter().any(|name| name == content)
        }
        fn team_has_item(&self, _team: u8, item: &str, amount: i32) -> bool {
            self.default_items.contains(item) && amount <= 1
        }
        fn core_item_count(&self, item: &str) -> i32 {
            self.core_items.get(item).copied().unwrap_or(0)
        }
        fn placed_block_count(&self, block: &str) -> i32 {
            self.placed.get(block).copied().unwrap_or(0)
        }
        fn unit_count(&self, _team: u8, unit: &str) -> i32 {
            self.unit_counts.get(unit).copied().unwrap_or(0)
        }
        fn enemy_units_destroyed(&self) -> i32 {
            self.enemy_units_destroyed
        }
        fn objective_flag(&self, flag: &str) -> bool {
            self.flags.contains(flag)
        }
        fn core_count(&self, _team: u8) -> usize {
            self.cores
        }
        fn block_at(&self, _x: i32, _y: i32) -> Option<(&str, u8)> {
            self.block
                .as_ref()
                .map(|(name, team)| (name.as_str(), *team))
        }
        fn headless(&self) -> bool {
            self.headless
        }
        fn command_mode_satisfied(&self) -> bool {
            self.command
        }
    }

    fn parse(json: &str) -> MapObjectives {
        MapObjectives::from_json(json).unwrap()
    }

    fn params() -> ObjectiveRunParams {
        ObjectiveRunParams::default()
    }

    #[test]
    fn item_and_content_objectives_track_env() {
        let data = parse(
            r#"[
                {"class":"Research","content":"alpha"},
                {"class":"Produce","content":"alpha"},
                {"class":"Item","item":"copper","amount":1},
                {"class":"CoreItem","item":"lead","amount":5}
            ]"#,
        );
        let mut runtime = MapObjectivesRuntime::from_data(data);
        let mut env = FakeEnv::default();
        env.unlocked.push("alpha".to_owned());
        env.default_items.insert("copper".to_owned());
        let done = runtime.update(&env, &params(), 0.0);
        assert_eq!(done, vec![0, 1, 2], "research/produce/item complete");
        for index in done {
            runtime.complete(index, &mut Rules::default());
        }
        env.core_items.insert("lead".to_owned(), 5);
        let done = runtime.update(&env, &params(), 0.0);
        assert_eq!(done, vec![3]);
    }

    #[test]
    fn build_unit_destroy_and_destroycore_objectives() {
        let data = parse(
            r#"[
                {"class":"BuildCount","block":"conveyor","count":2},
                {"class":"UnitCount","unit":"dagger","count":1},
                {"class":"DestroyUnits","count":3},
                {"class":"DestroyCore"}
            ]"#,
        );
        let mut runtime = MapObjectivesRuntime::from_data(data);
        let mut env = FakeEnv {
            cores: 5,
            ..FakeEnv::default()
        };
        assert!(runtime.update(&env, &params(), 0.0).is_empty());
        env.placed.insert("conveyor".to_owned(), 2);
        env.unit_counts.insert("dagger".to_owned(), 1);
        env.enemy_units_destroyed = 3;
        env.cores = 0;
        let done = runtime.update(&env, &params(), 0.0);
        assert_eq!(done, vec![0, 1, 2, 3]);
    }

    #[test]
    fn timer_accumulates_and_scales_by_multiplier() {
        let data = parse(r#"[{"class":"Timer","duration":10.0}]"#);
        let mut runtime = MapObjectivesRuntime::from_data(data);
        let env = FakeEnv::default();
        // 10 s at multiplier 1; 0.5 s delta needs 20 updates.
        for _ in 0..19 {
            assert!(runtime.update(&env, &params(), 0.5).is_empty());
        }
        assert_eq!(runtime.update(&env, &params(), 0.5), vec![0]);

        // Multiplier 2 halves the effective duration.
        let mut runtime =
            MapObjectivesRuntime::from_data(parse(r#"[{"class":"Timer","duration":10.0}]"#));
        let fast = ObjectiveRunParams {
            timer_multiplier: 0.5,
            ..ObjectiveRunParams::default()
        };
        let mut done_at = 0;
        for step in 1..=10 {
            if !runtime.update(&env, &fast, 1.0).is_empty() {
                done_at = step;
                break;
            }
        }
        assert_eq!(done_at, 5, "10s * 0.5 = 5 updates of 1s");
    }

    #[test]
    fn destroy_blocks_objective_counts_progress() {
        let data = parse(
            r#"[{"class":"DestroyBlocks","team":2,"block":"router",
                 "positions":[{"x":1,"y":1},{"x":2,"y":2}]}]"#,
        );
        let mut runtime = MapObjectivesRuntime::from_data(data);
        let env = FakeEnv {
            block: Some(("router".to_owned(), 2)),
            ..FakeEnv::default()
        };
        assert!(runtime.update(&env, &params(), 0.0).is_empty());
        // A missing block counts as destroyed.
        let destroyed = FakeEnv {
            block: None,
            ..FakeEnv::default()
        };
        assert_eq!(runtime.update(&destroyed, &params(), 0.0), vec![0]);
    }

    #[test]
    fn command_mode_and_flag_objectives() {
        let data = parse(r#"[{"class":"CommandMode"},{"class":"Flag","flag":"captured"}]"#);
        let mut runtime = MapObjectivesRuntime::from_data(data);
        let env = FakeEnv {
            headless: true,
            ..FakeEnv::default()
        };
        // CommandMode completes headless; Flag waits.
        assert_eq!(runtime.update(&env, &params(), 0.0), vec![0]);

        let mut env = FakeEnv::default();
        env.command = true;
        let data = parse(r#"[{"class":"CommandMode"},{"class":"Flag","flag":"captured"}]"#);
        let mut runtime = MapObjectivesRuntime::from_data(data);
        assert_eq!(runtime.update(&env, &params(), 0.0), vec![0]);
        env.flags.insert("captured".to_owned());
        assert_eq!(runtime.update(&env, &params(), 0.0), vec![0, 1]);
    }

    #[test]
    fn parent_dependency_gates_qualified_and_completion_applies_flags() {
        let data = parse(
            r#"[
                {"class":"Item","item":"copper","amount":1,
                 "flagsAdded":["one"],"flagsRemoved":["zero"]},
                {"class":"Item","item":"copper","amount":1,"parents":[0],
                 "flagsAdded":["two"]}
            ]"#,
        );
        let mut runtime = MapObjectivesRuntime::from_data(data);
        assert!(runtime.qualified(0));
        assert!(!runtime.qualified(1), "parent not done");
        assert!(!runtime.any() || runtime.qualified(0));

        let mut rules = Rules::default();
        rules.objective_flags.insert("zero".to_owned());
        assert!(runtime.complete(0, &mut rules));
        assert!(rules.objective_flags.contains("one"));
        assert!(!rules.objective_flags.contains("zero"));
        assert!(
            runtime.qualified(1),
            "dependency finished after parent done"
        );
        assert!(!runtime.complete(0, &mut rules), "idempotent");
        assert!(runtime.complete(1, &mut rules));
        assert!(rules.objective_flags.contains("two"));
    }

    #[test]
    fn clear_and_relay_epoch_checks() {
        let mut runtime =
            MapObjectivesRuntime::from_data(parse(r#"[{"class":"DestroyUnits","count":99}]"#));
        let mut rules = Rules::default();
        assert!(matches!(
            runtime.complete_checked(0, 4, 3, &mut rules),
            Err(ObjectiveRelayError::EpochMismatch { .. })
        ));
        assert!(runtime.complete_checked(0, 3, 3, &mut rules).unwrap());
        runtime.clear();
        assert!(runtime.is_empty());

        let mut guards = CompletedObjectives::default();
        assert!(guards.mark(0));
        assert!(!guards.mark(0));
        guards.clear();
        assert!(guards.mark(0));
    }

    #[test]
    fn type_names_keys_and_validate_defaults() {
        let data = parse(
            r#"[{"class":"Research","content":null},{"class":"DestroyBlock","block":null,"pos":{"x":1,"y":2}}]"#,
        );
        let mut runtime = MapObjectivesRuntime::from_data(data);
        assert_eq!(
            runtime.type_name(0).as_deref(),
            Some("objective.research.name")
        );
        assert_eq!(
            runtime.type_name(1).as_deref(),
            Some("objective.destroyblock.name")
        );
        runtime.validate_all();
        let MapObjective::Research(research) = runtime.get(0).unwrap() else {
            panic!("expected research");
        };
        assert_eq!(research.content.as_deref(), Some("copper"));
        let MapObjective::DestroyBlock(block) = runtime.get(1).unwrap() else {
            panic!("expected destroy block");
        };
        assert_eq!(block.block.as_deref(), Some("router"));
        assert_eq!(block.pos, Point2 { x: 1, y: 2 });
    }

    #[test]
    fn text_uses_locale_hooks() {
        let runtime = MapObjectivesRuntime::from_data(parse(
            r#"[{"class":"Flag","flag":"f","text":"@objective.capture"}]"#,
        ));
        let locale = NoLocale;
        let text = runtime.text(0, &locale).unwrap();
        assert_eq!(text, "objective.capture");
    }
}

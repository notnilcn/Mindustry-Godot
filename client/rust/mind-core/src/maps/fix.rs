// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MapFixer` port (`maps fix`, plan 19 M8/§3.13).
//!
//! Ported from `tools/src/mindustry/tools/MapFixer.java`. Upstream is a Gradle
//! tool; the port is `mind-headless maps fix --dir <dir> [--dry-run]`. The same
//! checks run against each editor-loadable map's `rules`/`wave` tags; `--dry-run`
//! reports the would-change list without writing.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::content::ContentRegistry;
use crate::editor::MapEditor;
use crate::editor::maps_glue::{editor_base_tags, save_editor_map};
use crate::io::FileSystem;
use crate::io::json::JsonIo;
use crate::io::json::objectives::MapObjective;
use crate::io::json::rules::Rules;
use crate::io::map::MapIo;
use crate::maps::{Map, MapError};
use crate::world::WorldGrid;

/// The result of fixing one map file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixReport {
    /// The map file.
    pub file: PathBuf,
    /// Human-readable change/warning lines (`MapFixer` log lines).
    pub changes: Vec<String>,
    /// Whether the map was (or would be) rewritten.
    pub changed: bool,
}

impl FixReport {
    /// A no-change report.
    fn clean(file: &Path) -> Self {
        Self {
            file: file.to_path_buf(),
            changes: Vec::new(),
            changed: false,
        }
    }
}

/// One loaded map's editable state used by the checks.
struct FixCandidate {
    file: PathBuf,
    editor: MapEditor,
    grid: WorldGrid,
    rules: Rules,
    wave: i32,
}

impl FixCandidate {
    fn load(
        fs: &dyn FileSystem,
        content: &mut ContentRegistry,
        file: &Path,
    ) -> Result<Self, MapError> {
        let mut editor = MapEditor::new();
        let mut grid = WorldGrid::new(0, 0);
        let header = MapIo::create_map(fs, file, true)?;
        let map = Map::from_header(&header, true);
        editor.begin_edit_map(&mut grid, content, fs, &map)?;
        let rules = match editor.tags.get("rules") {
            Some(json) if !json.trim().is_empty() => {
                JsonIo::read::<Rules>(json).unwrap_or_default()
            }
            _ => Rules::default(),
        };
        let wave = editor
            .tags
            .get("wave")
            .and_then(|wave| wave.parse::<i32>().ok())
            .unwrap_or(1);
        Ok(Self {
            file: file.to_path_buf(),
            editor,
            grid,
            rules,
            wave,
        })
    }
}

/// Runs the `MapFixer` checks on one file; writes back unless `dry_run`.
pub fn fix_file(
    fs: &dyn FileSystem,
    content: &mut ContentRegistry,
    file: &Path,
    dry_run: bool,
) -> Result<FixReport, MapError> {
    let is_hidden = file.to_string_lossy().contains("/hidden/");
    let mut candidate = FixCandidate::load(fs, content, file)?;
    let mut report = FixReport::clean(file);
    let rules = &mut candidate.rules;
    let mut changed = false;

    if !rules.banned_blocks.is_empty() {
        report
            .changes
            .push(format!("banned blocks found: {:?}", rules.banned_blocks));
        if is_hidden {
            rules.banned_blocks.clear();
            changed = true;
        }
    }
    if !rules.banned_units.is_empty() {
        report
            .changes
            .push(format!("banned units found: {:?}", rules.banned_units));
        if is_hidden {
            rules.banned_units.clear();
            changed = true;
        }
    }
    if rules.infinite_resources {
        report.changes.push("infinite resources enabled".to_owned());
        rules.infinite_resources = false;
        changed = true;
    }
    if rules.instant_build {
        report.changes.push("instant building enabled".to_owned());
        rules.instant_build = false;
        changed = true;
    }

    // Unlocalized + typo TimerObjectives.
    for objective in rules.objectives.all.iter_mut() {
        let MapObjective::Timer(timer) = objective else {
            continue;
        };
        if timer.common.hidden {
            continue;
        }
        let Some(text) = timer.text.clone() else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        if !text.contains('@') {
            report
                .changes
                .push(format!("unlocalized objective: '{text}'"));
            if is_hidden {
                timer.common.hidden = true;
                changed = true;
            }
        }
        if text.trim() == "@objective.enemyescelating" {
            report.changes.push(format!("typo in objective: '{text}'"));
            timer.text = Some("@objective.enemyescalating".to_owned());
            changed = true;
        }
    }

    if candidate.wave > 1 {
        report
            .changes
            .push(format!("wave is {}, but should be 1", candidate.wave));
        changed = true;
    }
    if !rules.revealed_blocks.is_empty() {
        report.changes.push(format!(
            "clearing revealed blocks: {:?}",
            rules.revealed_blocks
        ));
        rules.revealed_blocks.clear();
        changed = true;
    }
    if is_hidden && !rules.attack_mode && rules.win_wave <= 1 {
        report.changes.push("attack mode not enabled".to_owned());
        rules.attack_mode = true;
        changed = true;
    }

    report.changed = changed;
    if changed && !dry_run {
        write_back(fs, content, &mut candidate)?;
    }
    Ok(report)
}

/// Rewrites the map with the fixed rules + wave tags.
fn write_back(
    fs: &dyn FileSystem,
    content: &ContentRegistry,
    candidate: &mut FixCandidate,
) -> Result<(), MapError> {
    let rules_json = JsonIo::write(&candidate.rules)
        .map_err(|error| MapError::exception(candidate.file.clone(), error.to_string()))?;
    candidate.editor.tags.insert("rules".to_owned(), rules_json);
    candidate
        .editor
        .tags
        .insert("wave".to_owned(), "1".to_owned());
    let base = editor_base_tags(
        candidate.grid.tiles.width as u16,
        candidate.grid.tiles.height as u16,
        candidate
            .editor
            .tags
            .get("name")
            .map(String::as_str)
            .unwrap_or("map"),
    );
    let tags = candidate.editor.tags.clone();
    let file = candidate.file.clone();
    save_editor_map(fs, &file, &candidate.grid, content, base, tags, false)
}

/// Walks `dir` for `.msav` files and fixes each.
pub fn fix_dir(
    fs: &dyn FileSystem,
    content: &mut ContentRegistry,
    dir: &Path,
    dry_run: bool,
) -> Result<Vec<FixReport>, MapError> {
    let files = fs
        .walk(dir)
        .map_err(|error| MapError::exception(dir.to_path_buf(), error.to_string()))?;
    let mut reports = Vec::new();
    for file in files {
        let is_map = file
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("msav"));
        // The save engine writes a `<name>-backup.msav` alongside the rewrite;
        // never re-process it (it is a stale copy of the previous content).
        let is_backup = file
            .file_stem()
            .is_some_and(|stem| stem.to_string_lossy().ends_with("-backup"));
        if !is_map || is_backup {
            continue;
        }
        match fix_file(fs, content, &file, dry_run) {
            Ok(report) => reports.push(report),
            Err(error) => log::warn!("skipping `{}`: {error}", file.display()),
        }
    }
    reports.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(reports)
}

/// A tag map helper kept for parity with `MapFixer` unit fixtures.
pub fn with_wave_tag(mut tags: IndexMap<String, String>, wave: i32) -> IndexMap<String, String> {
    tags.insert("wave".to_owned(), wave.to_string());
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::io::fs::MockFs;
    use crate::io::save::fixture::FixtureWorld;
    use crate::io::save::versions::v1::base_meta_tags;
    use crate::io::save::{SaveIo, SaveOptions, WriteContext};

    fn write_map(
        fs: &dyn FileSystem,
        file: &Path,
        registry: &ContentRegistry,
        rules: &Rules,
        wave: i32,
    ) {
        let world = FixtureWorld::synthetic(registry, 32, 32);
        let mut tags = base_meta_tags(32, 32, wave, "fixme");
        tags.insert("name".to_owned(), "fixme".to_owned());
        tags.insert("rules".to_owned(), JsonIo::write(rules).unwrap());
        let mut ctx = WriteContext::meta_only(tags);
        ctx.content = Some(registry);
        ctx.map = Some(&world);
        ctx.entities = Some(&world);
        SaveIo::save(fs, file, &ctx, &SaveOptions::new()).unwrap();
    }

    #[test]
    fn dry_run_reports_then_write_is_noop() {
        let fs = MockFs::new();
        let mut content = test_registry();
        let dir = PathBuf::from("/data/maps/hidden");
        let file = dir.join("fixme.msav");
        #[allow(clippy::field_reassign_with_default)]
        let rules = {
            let mut rules = Rules::default();
            rules.infinite_resources = true;
            rules.instant_build = true;
            rules.objective_flags.insert("x".to_owned());
            rules.banned_blocks.insert("conveyor".to_owned());
            rules.revealed_blocks.insert("router".to_owned());
            rules
        };
        write_map(&fs, &file, &content, &rules, 5);

        let dry = fix_dir(&fs, &mut content, Path::new("/data/maps"), true).unwrap();
        let report = dry.iter().find(|r| r.file == file).expect("report");
        assert!(report.changed);
        assert!(report.changes.len() >= 4, "{:?}", report.changes);

        // Dry run must not modify the file.
        let before = fs.read(&file).unwrap();
        let _ = fix_dir(&fs, &mut content, Path::new("/data/maps"), true).unwrap();
        assert_eq!(before, fs.read(&file).unwrap());

        // Write mode fixes it; a second write is a no-op.
        let before_write = fs.read(&file).unwrap();
        let write = fix_dir(&fs, &mut content, Path::new("/data/maps"), false).unwrap();
        let changed_after = write.iter().filter(|r| r.changed).count();
        assert_eq!(changed_after, 1);
        assert_ne!(
            before_write,
            fs.read(&file).unwrap(),
            "fix write did not change the file"
        );
        let reloaded = FixCandidate::load(&fs, &mut content, &file).unwrap();
        assert!(
            !reloaded.rules.infinite_resources,
            "rules not persisted; rules tag = {:?}",
            reloaded.editor.tags.get("rules")
        );
        let second = fix_dir(&fs, &mut content, Path::new("/data/maps"), false).unwrap();
        let residual: Vec<(&PathBuf, &Vec<String>)> = second
            .iter()
            .filter(|r| r.changed)
            .map(|r| (&r.file, &r.changes))
            .collect();
        assert!(residual.is_empty(), "residual changes: {residual:?}");
    }
}

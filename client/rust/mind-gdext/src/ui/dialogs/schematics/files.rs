// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! `.msch` persistence helpers for the schematics dialogs. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/SchematicsDialog.java (.msch persistence helpers)

use super::schematics_dialog::*;
use crate::MindSimHost;
use crate::gdutil::variant::to_text_godot;
use crate::ui::dialog::MindDialog;
use crate::ui::dialogs::campaign::util::*;
use crate::ui::kit;
use godot::builtin::VarDictionary;
use mind_core::game::schematic::Schematic;
use mind_core::game::schematics;
use mind_core::io::{FileSystem, NativeFs, Paths};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// SchematicsDialog: `.msch` persistence
// (`Schematic.save` / `Schematics.saveChanges` / `Schematics.add` / `readBase64`)
// ---------------------------------------------------------------------------

/// Runs `f` against the live content registry (`Vars.content`). `None` when the
/// spine (and therefore the sim host) is absent, e.g. the dialog sweep scene.
pub(super) fn with_content_registry<T>(
    f: impl FnOnce(&mind_core::content::ContentRegistry) -> T,
) -> Option<T> {
    let node = kit::sim_host()?;
    let host = node.try_cast::<MindSimHost>().ok()?;
    let bound = host.bind();
    let registry = bound.content_registry()?;
    Some(f(registry))
}

/// `Vars.schematicDirectory` (`dataDirectory/schematics`): `MindCampaign` loads
/// the same root (`mind_core::config::set_default_data_dir` pins it to
/// `user://` at extension init).
pub(super) fn schematic_directory() -> PathBuf {
    Paths::resolve(None).root().join("schematics")
}

/// `Schematics.findFile`: `sanitizeFilename(name)[_N].msch`, first free.
pub(super) fn schematic_free_path(
    dir: &Path,
    name: &str,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    let base = mind_core::util::strings::sanitize_filename(name);
    // `Strings.sanitizeFilename("")` is empty; Java substitutes "empty".
    let base = if base.is_empty() {
        String::from("empty")
    } else {
        base
    };
    let extension = mind_core::game::rules::SCHEMATIC_EXTENSION;
    let mut index = 0usize;
    loop {
        let stem = if index == 0 {
            base.clone()
        } else {
            format!("{base}_{index}")
        };
        let path = dir.join(format!("{stem}.{extension}"));
        // `while(result == null || result.exists())`: bound the walk so a
        // broken filesystem cannot spin forever.
        if !exists(&path) || index > 1000 {
            return path;
        }
        index += 1;
    }
}

/// Reads a `.msch` (`Schematics.read`).
pub(super) fn schematic_read_file(path: &Path) -> Option<Schematic> {
    let bytes = NativeFs.read(path).ok()?;
    with_content_registry(|registry| schematics::read(&bytes, registry).ok())?
}

/// `Schematic.save` + `Schematics.saveChanges`: decode, mutate, re-encode,
/// write back. `false` on a decode/encode/IO failure (Java shows an exception).
pub(super) fn schematic_save_file(path: &Path, mutate: impl FnOnce(&mut Schematic)) -> bool {
    let Ok(bytes) = NativeFs.read(path) else {
        return false;
    };
    let encoded = with_content_registry(|registry| {
        let Ok(mut schematic) = schematics::read(&bytes, registry) else {
            return None;
        };
        mutate(&mut schematic);
        schematics::write(&schematic, registry).ok()
    })
    .flatten();
    let Some(encoded) = encoded else {
        return false;
    };
    NativeFs.write(path, &encoded).is_ok()
}

/// `Schematics.writeBase64` for a stored schematic (always `bXNjaAB...`).
pub(super) fn schematic_base64_file(path: &Path) -> Option<String> {
    let bytes = NativeFs.read(path).ok()?;
    with_content_registry(|registry| {
        schematics::read(&bytes, registry)
            .ok()
            .and_then(|schematic| schematics::write_base64(&schematic, registry).ok())
    })
    .flatten()
}

/// `Schematics.add`: decode, `removeSteamID`, write a fresh `.msch`.
pub(super) fn schematic_import_bytes(bytes: &[u8]) -> Option<PathBuf> {
    let mut schematic = with_content_registry(|registry| schematics::read(bytes, registry).ok())??;
    schematic.remove_steam_id();
    let dir = schematic_directory();
    let path = schematic_free_path(&dir, &schematic.name(), |candidate| {
        NativeFs.exists(candidate)
    });
    let encoded = with_content_registry(|registry| schematics::write(&schematic, registry).ok())??;
    NativeFs.write(&path, &encoded).ok()?;
    Some(path)
}

/// The view row for `index` (fresh read model; `None` when it disappeared).
pub(super) fn schematic_row(dialog: &MindDialog, index: i64) -> Option<VarDictionary> {
    for row in dialog.campaign_section("schematics").iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        if row_i64_or(&row, "index", -1) == index {
            return Some(row);
        }
    }
    None
}

/// The row list (`campaign_views_json.schematics`).
pub(super) fn schematic_rows(dialog: &MindDialog) -> Vec<VarDictionary> {
    dialog
        .campaign_section("schematics")
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// The absolute `.msch` path backing a schematic (`Schematic.file`).
pub(super) fn schematic_file_of(dialog: &MindDialog, index: i64) -> Option<PathBuf> {
    let row = schematic_row(dialog, index)?;
    let file = row_str(&row, "file");
    if file.is_empty() {
        None
    } else {
        Some(PathBuf::from(file))
    }
}

/// `s.mod != null`: mod schematics live in the mod's read-only data dir.
pub(super) fn schematic_is_mod(row: &VarDictionary) -> bool {
    !row_str(row, "mod_name").is_empty()
}

/// The index of the schematic stored at `path` (used after an import).
pub(super) fn schematic_index_by_file(dialog: &MindDialog, path: &Path) -> i64 {
    let wanted = path.to_string_lossy().to_string();
    for row in schematic_rows(dialog) {
        if row_str(&row, "file") == wanted {
            return row_i64_or(&row, "index", -1);
        }
    }
    -1
}

/// `s.labels.add(tag)` (`Seq.add`; false when already present or empty).
fn schematic_labels_add(labels: &mut Vec<String>, tag: &str) -> bool {
    if tag.is_empty() || labels.iter().any(|existing| existing == tag) {
        return false;
    }
    labels.push(tag.to_owned());
    true
}

/// `s.labels.remove(tag)`.
pub(super) fn schematic_labels_remove(labels: &mut Vec<String>, tag: &str) -> bool {
    let before = labels.len();
    labels.retain(|existing| existing != tag);
    labels.len() != before
}

/// `s.labels.replace(tag, result)` (`Seq.replace`: first occurrence).
pub(super) fn schematic_labels_replace(labels: &mut [String], from: &str, to: &str) -> bool {
    let Some(position) = labels.iter().position(|existing| existing == from) else {
        return false;
    };
    labels[position] = to.to_owned();
    true
}

/// `tags.swap(idx, idx ± 1)` for the `@editor.moveup`/`@editor.movedown` cells.
pub(super) fn schematics_tag_swap(tags: &mut [String], index: usize, delta: isize) -> bool {
    let Some(target) = index.checked_add_signed(delta) else {
        return false;
    };
    if index >= tags.len() || target >= tags.len() {
        return false;
    }
    tags.swap(index, target);
    true
}

/// The global tags the schematic does not carry yet (`buildTags` add panel;
/// order follows the global list).
pub(super) fn schematics_available_tags(tags: &[String], labels: &[String]) -> Vec<String> {
    tags.iter()
        .filter(|tag| !labels.iter().any(|label| label == *tag))
        .cloned()
        .collect()
}

/// `schematic.tagged`: how many schematics carry `tag`.
pub(super) fn schematics_tag_count(labels: &[Vec<String>], tag: &str) -> i64 {
    labels
        .iter()
        .filter(|row| row.iter().any(|label| label == tag))
        .count() as i64
}

/// The label lists of the library rows (`s.labels` for every schematic).
pub(super) fn schematic_label_lists(dialog: &MindDialog) -> Vec<Vec<String>> {
    schematic_rows(dialog)
        .iter()
        .map(|row| {
            row_arr(row, "labels")
                .iter_shared()
                .map(|value| to_text_godot(&value))
                .collect()
        })
        .collect()
}

/// `Core.app.getClipboardText().startsWith(schematicBaseStart)`.
pub(super) fn schematic_clipboard_importable(text: &str) -> bool {
    text.trim_start().starts_with(SCHEMATIC_BASE_START)
}

/// A single-character tag in the icon PUA range (`showNewIconTag` stores
/// `(char)Iconc.codes.get(icon) + ""`).
pub(super) fn schematic_icon_tag_char(tag: &str) -> Option<char> {
    let plain = schematics_tag_plain(tag);
    let mut characters = plain.chars();
    let character = characters.next()?;
    if characters.next().is_some() || (character as u32) < 0xE000 {
        return None;
    }
    Some(character)
}

/// `buttons.button("@ok", ...).disabled(b -> nameField.getText().isEmpty())`.
pub(super) fn schematic_edit_name_valid(name: &str) -> bool {
    !name.is_empty()
}

/// Adds `tag` to the global list, persists `schematic-tags` and rebuilds the
/// tag row (`showNewTag`/`showNewIconTag` + `tagsChanged`). `false` when the
/// tag already exists (Java `@schematic.tagexists`).
pub(super) fn schematics_add_global_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    tag: &str,
) -> bool {
    if tag.is_empty() {
        return false;
    }
    let exists = state.borrow().tags.iter().any(|existing| existing == tag);
    if exists {
        return false;
    }
    state.borrow_mut().tags.push(tag.to_owned());
    schematics_save_tags(state);
    schematics_rebuild_tags(dialog, state);
    true
}

/// `tagsChanged()`: rebuild the tag row, re-filter the pane when a tag filter is
/// active and persist the list.
pub(super) fn schematics_tags_changed(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
) {
    let has_filter = !state.borrow().selected_tags.is_empty();
    schematics_save_tags(state);
    schematics_rebuild_tags(dialog, state);
    if has_filter {
        schematics_rebuild_list(dialog, state);
    }
}

/// `addTag(s, tag)` / `removeTag(s, tag)`: rewrite the schematic's labels.
pub(super) fn schematic_set_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
    tag: &str,
    add: bool,
) -> bool {
    let Some(path) = schematic_file_of(dialog, index) else {
        return false;
    };
    let saved = schematic_save_file(&path, |schematic| {
        if add {
            schematic_labels_add(&mut schematic.labels, tag);
        } else {
            schematic_labels_remove(&mut schematic.labels, tag);
        }
    });
    if saved {
        schematics_rebuild_list(dialog, state);
    }
    saved
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn schematic_free_path_sanitizes_and_suffixes() {
        let dir = Path::new("/tmp/schematics");
        let taken = ["/tmp/schematics/a_b.msch", "/tmp/schematics/a_b_1.msch"];
        let exists = |path: &Path| taken.contains(&path.to_string_lossy().as_ref());
        // `sanitizeFilename("a/b")` -> `a_b`; `findFile` then adds `_N`.
        assert_eq!(
            schematic_free_path(dir, "a/b", exists),
            PathBuf::from("/tmp/schematics/a_b_2.msch")
        );
        // Spaces are legal in a file name (`Strings.sanitizeFilename` keeps them).
        assert_eq!(
            schematic_free_path(dir, "a b", |_path| false),
            PathBuf::from("/tmp/schematics/a b.msch")
        );
        // An empty name falls back to `empty` (`findFile`).
        assert_eq!(
            schematic_free_path(dir, "", |_path| false),
            PathBuf::from("/tmp/schematics/empty.msch")
        );
        // Reserved DOS names are prefixed (`sanitizeFilename`).
        assert_eq!(
            schematic_free_path(dir, "con", |_path| false),
            PathBuf::from("/tmp/schematics/_con.msch")
        );
    }

    #[test]
    fn schematic_label_list_operations_follow_seq_semantics() {
        let mut labels: Vec<String> = Vec::new();
        assert!(schematic_labels_add(&mut labels, "Power"));
        assert!(
            !schematic_labels_add(&mut labels, "Power"),
            "duplicate ignored"
        );
        assert!(!schematic_labels_add(&mut labels, ""), "empty ignored");
        assert!(schematic_labels_add(&mut labels, "[magenta]advanced-logic"));
        assert_eq!(labels.len(), 2);
        // `s.labels.replace(tag, result)` rewrites the first occurrence.
        assert!(schematic_labels_replace(
            &mut labels,
            "[magenta]advanced-logic",
            "logic"
        ));
        assert!(!schematic_labels_replace(&mut labels, "missing", "x"));
        assert_eq!(labels, vec!["Power", "logic"]);
        assert!(schematic_labels_remove(&mut labels, "Power"));
        assert!(!schematic_labels_remove(&mut labels, "Power"));
        assert_eq!(labels, vec!["logic"]);
    }

    #[test]
    fn schematic_tag_reorder_swaps_only_in_range() {
        let mut tags = vec![String::from("a"), String::from("b"), String::from("c")];
        // `idx > 0` / `idx < size - 1` guards from `showAllTags`.
        assert!(!schematics_tag_swap(&mut tags, 0, -1));
        assert!(schematics_tag_swap(&mut tags, 0, 1));
        assert_eq!(tags, vec!["b", "a", "c"]);
        assert!(schematics_tag_swap(&mut tags, 2, -1));
        assert_eq!(tags, vec!["b", "c", "a"]);
        assert!(!schematics_tag_swap(&mut tags, 2, 1));
        assert!(!schematics_tag_swap(&mut tags, 9, -1));
    }

    #[test]
    fn schematic_available_tags_keep_global_order() {
        let tags = vec![
            String::from("Power"),
            String::from("logic"),
            String::from("defense"),
        ];
        let labels = vec![String::from("logic")];
        assert_eq!(
            schematics_available_tags(&tags, &labels),
            vec!["Power", "defense"]
        );
        assert_eq!(schematics_available_tags(&tags, &[]).len(), 3);
    }

    #[test]
    fn schematic_icon_tags_are_single_pua_characters() {
        // Accessible icons sit in the private-use area (`Iconc.power` = 59408).
        let power = char::from_u32(59408).expect("PUA scalar");
        assert_eq!(schematic_icon_tag_char(&power.to_string()), Some(power));
        // Plain text tags are not icon tags.
        assert_eq!(schematic_icon_tag_char("Power"), None);
        assert_eq!(schematic_icon_tag_char(""), None);
        // Two characters never qualify.
        assert_eq!(schematic_icon_tag_char("ab"), None);
        // Arc color markup is stripped before the test (`[magenta]x` -> `x`).
        assert_eq!(schematic_icon_tag_char("[magenta]x"), None);
    }

    #[test]
    fn schematic_clipboard_import_gate_matches_prefix() {
        assert!(schematic_clipboard_importable("bXNjaAB4nGNg"));
        assert!(schematic_clipboard_importable("  bXNjaAB"));
        assert!(!schematic_clipboard_importable(""));
        // `FileChooser` output / arbitrary text is rejected.
        assert!(!schematic_clipboard_importable("hello"));
        assert!(!schematic_clipboard_importable("bXNjaA"));
    }

    #[test]
    fn schematic_edit_accept_requires_a_name() {
        assert!(schematic_edit_name_valid("Wall"));
        assert!(!schematic_edit_name_valid(""));
    }

    #[test]
    fn schematic_tagged_count_counts_rows() {
        let rows: Vec<Vec<String>> = vec![
            vec![String::from("Power"), String::from("logic")],
            vec![String::from("Power")],
        ];
        assert_eq!(schematics_tag_count(&rows, "Power"), 2);
        assert_eq!(schematics_tag_count(&rows, "logic"), 1);
        assert_eq!(schematics_tag_count(&rows, "missing"), 0);
    }
}

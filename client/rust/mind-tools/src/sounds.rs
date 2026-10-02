// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `annotations/src/main/java/mindustry/annotations/impl/AssetsProcess.java`
// (`processSounds`: sorted file walk, duplicate-name error, Java keyword
// mangling, dense append-only ids, `none`/`unset` dummies).

//! `sounds index` / `musics` generation (plan 03 §3.5 stage 8, §6.5).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

/// One generated sound entry (`Sounds.<field>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SoundEntry {
    /// Generated field name (keyword-mangled; parity ABI).
    pub name: String,
    /// Asset path (`sounds/<category>/<file>`), `null` for virtual dummies.
    pub file: Option<String>,
    /// Dense append-only id, `-1` for virtual dummies.
    pub id: i32,
    /// Top-level sound folder (`ui`, `weapons`, …), `null` when root/dummy.
    pub category: Option<String>,
}

/// One generated music entry (`Musics.<field>`; no ids upstream).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MusicEntry {
    /// Generated field name (keyword-mangled).
    pub name: String,
    /// Asset path (`music/<file>`).
    pub file: String,
}

/// Serialized `assets/sounds.index.json` (§6.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SoundsIndex {
    /// Format version.
    pub format: u32,
    /// Sounds (dummies first, then dense-id order).
    pub sounds: Vec<SoundEntry>,
    /// Musics in file-name order.
    pub musics: Vec<MusicEntry>,
}

/// Java `SourceVersion.isKeyword` (the 50 keywords + `true`/`false`/`null`).
pub fn is_java_keyword(name: &str) -> bool {
    matches!(
        name,
        "abstract"
            | "assert"
            | "boolean"
            | "break"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "class"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extends"
            | "final"
            | "finally"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "implements"
            | "import"
            | "instanceof"
            | "int"
            | "interface"
            | "long"
            | "native"
            | "new"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "short"
            | "static"
            | "strictfp"
            | "super"
            | "switch"
            | "synchronized"
            | "this"
            | "throw"
            | "throws"
            | "transient"
            | "try"
            | "void"
            | "volatile"
            | "while"
            | "true"
            | "false"
            | "null"
    )
}

/// `AssetsProcess`: keyword names get an `s` appended.
pub fn mangle_field(name: &str) -> String {
    if is_java_keyword(name) {
        format!("{name}s")
    } else {
        name.to_owned()
    }
}

/// Walks a directory recursively for audio files, sorted by file name
/// (upstream `files.sortComparing(Fi::name)`).
fn audio_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    collect(dir, &mut files)?;
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()).then_with(|| a.cmp(b)));
    Ok(files)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext == "ogg" || ext == "mp3")
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Builds the sounds+musics index for a repo root.
pub fn build_index(root: &Path) -> Result<SoundsIndex> {
    let sounds_dir = root.join("assets/sounds");
    let music_dir = root.join("assets/music");

    let mut names = BTreeSet::new();
    let mut sounds = vec![
        SoundEntry {
            name: String::from("none"),
            file: None,
            id: -1,
            category: None,
        },
        SoundEntry {
            name: String::from("unset"),
            file: None,
            id: -1,
            category: None,
        },
    ];
    for (id, path) in audio_files(&sounds_dir)?.into_iter().enumerate() {
        let stem = file_stem(&path)?;
        if !names.insert(stem.clone()) {
            bail!(
                "duplicate sound file name `{stem}` (AssetsProcess rejects this): {}",
                path.display()
            );
        }
        let asset_root = root.join("assets");
        let relative = path
            .strip_prefix(&asset_root)
            .with_context(|| format!("stripping {}", path.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        let category = relative
            .strip_prefix("sounds/")
            .and_then(|rest| rest.split_once('/').map(|(dir, _)| dir.to_owned()));
        sounds.push(SoundEntry {
            name: mangle_field(&stem),
            file: Some(relative),
            id: id as i32,
            category,
        });
    }

    let mut music_names = BTreeSet::new();
    let mut musics = Vec::new();
    for path in audio_files(&music_dir)? {
        let stem = file_stem(&path)?;
        if !music_names.insert(stem.clone()) {
            bail!("duplicate music file name `{stem}`: {}", path.display());
        }
        let asset_root = root.join("assets");
        let relative = path
            .strip_prefix(&asset_root)
            .with_context(|| format!("stripping {}", path.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        musics.push(MusicEntry {
            name: mangle_field(&stem),
            file: relative,
        });
    }

    Ok(SoundsIndex {
        format: 1,
        sounds,
        musics,
    })
}

fn file_stem(path: &Path) -> Result<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_owned)
        .with_context(|| format!("bad audio file name {}", path.display()))
}

/// Writes `assets/sounds.index.json` and returns the index.
pub fn write_index(root: &Path) -> Result<SoundsIndex> {
    let index = build_index(root)?;
    let path = root.join("assets/sounds.index.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&index)?),
    )
    .with_context(|| format!("writing {}", path.display()))?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("sounds-index-{name}-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(root.join("assets/sounds/ui")).unwrap();
        fs::create_dir_all(root.join("assets/sounds/weapons")).unwrap();
        fs::create_dir_all(root.join("assets/music")).unwrap();
        root
    }

    #[test]
    fn names_ids_categories_and_keyword_mangling() {
        let root = fixture("ok");
        fs::write(root.join("assets/sounds/ui/click.ogg"), b"x").unwrap();
        fs::write(root.join("assets/sounds/ui/new.ogg"), b"x").unwrap();
        fs::write(root.join("assets/sounds/weapons/shoot.ogg"), b"x").unwrap();
        fs::write(root.join("assets/music/game1.ogg"), b"x").unwrap();
        // sort by file name: click, new, shoot, static.
        fs::write(root.join("assets/sounds/ui/static.ogg"), b"x").unwrap();

        let index = build_index(&root).unwrap();
        assert_eq!(index.sounds[0].name, "none");
        assert_eq!(index.sounds[0].id, -1);
        assert_eq!(index.sounds[1].name, "unset");
        let real: Vec<&SoundEntry> = index.sounds.iter().filter(|s| s.id >= 0).collect();
        assert_eq!(
            real.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["click", "news", "shoot", "statics"]
        );
        assert_eq!(real[1].category.as_deref(), Some("ui"));
        assert_eq!(real[2].category.as_deref(), Some("weapons"));
        assert_eq!(index.musics.len(), 1);
        assert_eq!(index.musics[0].file, "music/game1.ogg");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn duplicates_are_rejected() {
        let root = fixture("dup");
        fs::write(root.join("assets/sounds/ui/beep.ogg"), b"x").unwrap();
        fs::create_dir_all(root.join("assets/sounds/extra")).unwrap();
        fs::write(root.join("assets/sounds/extra/beep.ogg"), b"x").unwrap();
        assert!(build_index(&root).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn keyword_list_matches_assets_process() {
        assert_eq!(mangle_field("new"), "news");
        assert_eq!(mangle_field("while"), "whiles");
        assert_eq!(mangle_field("uiBack"), "uiBack");
        assert!(is_java_keyword("char"));
        assert!(!is_java_keyword("charge"));
    }
}

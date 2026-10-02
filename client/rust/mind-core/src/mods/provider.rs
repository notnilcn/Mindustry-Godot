// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ModsContentProvider` (plan 02 `ModContentProvider`) and content-file
//! collection (`Mods.buildFiles`/`loadContent` order).

use std::path::Path;

use crate::content::parser_hooks::{ContentErrors, ModContentProvider};
use crate::content::{ContentError, ContentRegistry, ModId};
use crate::io::{FileSystem, SettingsStore};

use super::discovery::{ModRoot, find_meta, resolve_folder_root};
use super::json::{ContentJsonParser, content_type_for_folder};
use super::{LoadedMod, ModError, ModSource, ModState, Mods};

/// One content JSON file discovered inside an enabled mod.
#[derive(Debug, Clone)]
pub struct ModContentFile {
    /// Internal mod name.
    pub mod_name: String,
    /// Root-relative file path.
    pub path: String,
    /// File stem (pre-prefix content name).
    pub stem: String,
    /// Target content type.
    pub type_: crate::content::ContentType,
    /// Raw JSON text.
    pub json: String,
}

impl Mods {
    /// Treats `folder` itself as a single mod root (harness `--fixture <name>`).
    pub fn load_single(
        &mut self,
        fs: &dyn FileSystem,
        folder: &Path,
        settings: &SettingsStore,
    ) -> Result<(), ModError> {
        self.mod_directory = folder.parent().unwrap_or(folder).to_path_buf();
        self.mods.clear();
        self.last_ordered = None;
        let root = resolve_folder_root(fs, folder);
        let mod_root = ModRoot::Folder(root);
        let Some((_, meta)) = find_meta(fs, &mod_root) else {
            return Err(ModError::Invalid(format!(
                "`{}` is not a mod folder (no mod.json)",
                folder.display()
            )));
        };
        let mut loaded = LoadedMod::new(
            0,
            folder.to_path_buf(),
            mod_root.clone(),
            ModSource::LocalFolder,
            meta,
        );
        loaded.has_scripts = mod_root.exists(fs, "scripts");
        if !loaded.should_be_enabled(settings) {
            loaded.state = ModState::Disabled;
        }
        self.mods.push(loaded);
        self.sort_mods();
        self.reindex();
        self.requires_reload = false;
        Ok(())
    }

    /// `Mods.buildFiles` content half: enumerates `content/<kind>/**.json` for
    /// each enabled, non-hidden mod in dependency order, sorted by
    /// `contentOrder` first then file stem.
    pub fn collect_content_files(&mut self, fs: &dyn FileSystem) -> Vec<ModContentFile> {
        let mut out = Vec::new();
        let ordered = self.ordered_mods();
        for index in ordered {
            let Some(mod_) = self.mods.get(index) else {
                continue;
            };
            if mod_.meta.hidden {
                continue;
            }
            let Ok(dirs) = mod_.root.list(fs, "content") else {
                continue;
            };
            let mut per_mod: Vec<(usize, String, ModContentFile)> = Vec::new();
            for dir in dirs {
                if !dir.is_dir {
                    continue;
                }
                let Some(type_) = content_type_for_folder(&dir.name) else {
                    continue;
                };
                let prefix = format!("content/{}", dir.name);
                let Ok(files) = mod_.root.walk(fs, &prefix) else {
                    continue;
                };
                for path in files {
                    let extension = path.rsplit('.').next().unwrap_or("");
                    if !matches!(extension, "json" | "hjson" | "json5") {
                        continue;
                    }
                    let Some(stem) = file_stem(&path) else {
                        continue;
                    };
                    let Ok(json) = mod_.root.read_to_string(fs, &path) else {
                        continue;
                    };
                    let order = mod_
                        .meta
                        .content_order
                        .iter()
                        .position(|name| name == &stem)
                        .unwrap_or(usize::MAX);
                    per_mod.push((
                        order,
                        stem.clone(),
                        ModContentFile {
                            mod_name: mod_.name.clone(),
                            path,
                            stem,
                            type_,
                            json,
                        },
                    ));
                }
            }
            per_mod.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            out.extend(per_mod.into_iter().map(|(_, _, file)| file));
        }
        out
    }
}

/// File stem (final path component without extension).
fn file_stem(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name);
    (!stem.is_empty()).then(|| stem.to_owned())
}

/// `Mods.loadContent` provider: parses JSON content files into the registry.
pub struct ModsContentProvider {
    files: Vec<ModContentFile>,
    parser: ContentJsonParser,
    errors: Vec<ContentError>,
}

impl ModsContentProvider {
    /// Builds a provider from a collected file list.
    pub fn new(files: Vec<ModContentFile>) -> Self {
        Self {
            files,
            parser: ContentJsonParser::new(),
            errors: Vec::new(),
        }
    }

    /// The parser (bundle entries + warnings).
    pub fn parser(&self) -> &ContentJsonParser {
        &self.parser
    }

    /// Errors collected during the last `load_content`.
    pub fn errors(&self) -> &[ContentError] {
        &self.errors
    }
}

impl ModContentProvider for ModsContentProvider {
    fn load_content(&mut self, registry: &mut ContentRegistry) -> Result<(), ContentErrors> {
        let mut errors = Vec::new();
        let previous = registry.current_mod().cloned();
        for file in &self.files {
            registry.set_current_mod(Some(ModId(file.mod_name.clone())));
            if let Err(error) = self
                .parser
                .parse(registry, &file.path, &file.stem, &file.json, file.type_)
            {
                errors.push(ContentError::Parse(error.message));
            }
        }
        registry.set_current_mod(previous);
        self.errors = errors.clone();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::bundle::BundleView;
    use crate::content::create_base_content;
    use crate::content::{MemoryBundle, MemoryUnlockStore};
    use crate::io::MockFs;
    use std::path::PathBuf;

    fn fixture_fs() -> MockFs {
        let fs = MockFs::new();
        fs.write(
            Path::new("/mods/basic/mod.json"),
            br#"{"name":"Basic Mod","version":"1.0","minGameVersion":"146"}"#,
        )
        .expect("mod.json");
        fs.write(
            Path::new("/mods/basic/content/items/test-ingot.json"),
            br#"{"name":"Test Ingot","description":"A test ingot.","hardness":3,"cost":1.5}"#,
        )
        .expect("item");
        fs.write(
            Path::new("/mods/basic/content/blocks/test-wall.json"),
            br#"{"name":"Test Wall","size":2,"health":200,"requirements":["test-ingot/20"]}"#,
        )
        .expect("block");
        fs
    }

    fn boot() -> ContentRegistry {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        let mut registry = create_base_content(&bundle, &store, true).expect("base content");
        registry.init().expect("init");
        registry.post_init().expect("post init");
        registry
    }

    /// Plan 20 M1: `content::prefix_names` + `content::bundle_keys_registered`.
    #[test]
    fn prefix_names_and_bundle_keys() {
        let fs = fixture_fs();
        let settings = SettingsStore::new();
        let mut mods = Mods::new(true, "/mods");
        mods.load_single(&fs, Path::new("/mods/basic"), &settings)
            .expect("load single");
        let files = mods.collect_content_files(&fs);
        assert_eq!(files.len(), 2, "one item + one block");
        let mut provider = ModsContentProvider::new(files);

        let mut registry = boot();
        let items_before = registry.items().len();
        assert!(registry.create_mod_content(&mut provider).is_ok());

        assert_eq!(registry.items().len(), items_before + 1);
        assert!(registry.item_by_name("basic-mod-test-ingot").is_some());
        let wall = registry
            .block_by_name("basic-mod-test-wall")
            .expect("prefixed block");
        assert_eq!(wall.size, 2);
        assert_eq!(wall.health, 200);
        assert_eq!(wall.requirements.len(), 1);

        let bundle = provider.parser().bundle_entries();
        assert_eq!(
            bundle.get("item.basic-mod-test-ingot.name"),
            Some("Test Ingot")
        );
        assert_eq!(
            bundle.get("item.basic-mod-test-ingot.description"),
            Some("A test ingot.")
        );
        assert_eq!(
            bundle.get("block.basic-mod-test-wall.name"),
            Some("Test Wall")
        );
        let item = registry
            .item_by_name("basic-mod-test-ingot")
            .expect("item record");
        assert_eq!(item.unlock.localized_name, "Test Ingot");
    }

    /// Plan 20 M1: `content::content_order_first`.
    #[test]
    fn content_order_first() {
        let fs = MockFs::new();
        fs.write(
            Path::new("/mods/order/mod.json"),
            br#"{"name":"Order Mod","contentOrder":["zeta","alpha"],"minGameVersion":"146"}"#,
        )
        .expect("mod.json");
        for name in ["alpha", "beta", "zeta"] {
            fs.write(
                &PathBuf::from(format!("/mods/order/content/items/{name}.json")),
                br#"{"name":"X"}"#,
            )
            .expect("item");
        }
        let settings = SettingsStore::new();
        let mut mods = Mods::new(true, "/mods");
        mods.load_single(&fs, Path::new("/mods/order"), &settings)
            .expect("load");
        let names: Vec<String> = mods
            .collect_content_files(&fs)
            .into_iter()
            .map(|file| file.stem)
            .collect();
        // contentOrder first (zeta, alpha), then alphabetical remainder (beta).
        assert_eq!(names, vec!["zeta", "alpha", "beta"]);
    }

    /// Plan 20 M1: `content::legacy_folder_names` — singular and plural folders
    /// both resolve (`content/item/` and `content/items/`).
    #[test]
    fn legacy_folder_names() {
        assert_eq!(
            content_type_for_folder("item"),
            Some(crate::content::ContentType::Item)
        );
        assert_eq!(
            content_type_for_folder("items"),
            Some(crate::content::ContentType::Item)
        );
        assert_eq!(
            content_type_for_folder("block"),
            Some(crate::content::ContentType::Block)
        );
        assert_eq!(
            content_type_for_folder("weathers"),
            Some(crate::content::ContentType::Weather)
        );
    }

    /// Plan 20 M1: `content::bad_file_isolated` — a malformed file does not
    /// abort the sweep; good content still registers.
    #[test]
    fn bad_file_isolated() {
        let fs = fixture_fs();
        fs.write(
            Path::new("/mods/basic/content/blocks/bad-block.json"),
            br#"{"type":"NoSuchBlockType","name":"Bad"}"#,
        )
        .expect("bad block");
        let settings = SettingsStore::new();
        let mut mods = Mods::new(true, "/mods");
        mods.load_single(&fs, Path::new("/mods/basic"), &settings)
            .expect("load");
        let files = mods.collect_content_files(&fs);
        let mut provider = ModsContentProvider::new(files);
        let mut registry = boot();
        let errors = registry
            .create_mod_content(&mut provider)
            .expect_err("bad block reports an error");
        assert_eq!(errors.len(), 1, "one bad file isolated");
        assert!(registry.item_by_name("basic-mod-test-ingot").is_some());
        assert!(registry.block_by_name("basic-mod-test-wall").is_some());
        assert!(registry.block_by_name("basic-mod-bad-block").is_none());
    }
}

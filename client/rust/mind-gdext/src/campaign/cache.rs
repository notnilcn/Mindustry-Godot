// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Directory-mtime memo helpers shared by the schematic and save-slot caches.
//! Upstream: none (port-only; directory-mtime memo, findings #9)

/// The modification time of a directory used as a cache key (finding #9).
///
/// `None` means the directory (or its mtime) could not be read; the memo then
/// reloads instead of pinning a possibly empty result.
pub(super) fn dir_mtime_key(dir: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(dir).and_then(|meta| meta.modified()).ok()
}

/// A directory-mtime-keyed memo (finding #9).
///
/// Only populated when the directory is readable, so an unreadable directory
/// degrades to the uncached behaviour. The caller chooses what to load; the
/// helpers here only own the invalidation rule and are unit-tested directly.
#[derive(Debug, Default)]
pub(super) struct DirMemo<T> {
    /// The mtime the cached value was loaded under.
    key: Option<std::time::SystemTime>,
    /// The cached value; `Some` only alongside a `Some` key.
    value: Option<T>,
}

impl<T> DirMemo<T> {
    /// The cached value when `key` is still the one it was loaded under.
    pub(super) fn get(&self, key: Option<std::time::SystemTime>) -> Option<&T> {
        if key.is_some() && self.key == key {
            self.value.as_ref()
        } else {
            None
        }
    }

    /// Stores `value` under `key`; an unreadable directory is never cached.
    pub(super) fn store(&mut self, key: Option<std::time::SystemTime>, value: T) {
        if key.is_none() {
            return;
        }
        self.key = key;
        self.value = Some(value);
    }

    /// Drops the cached value (content boot / data-root change).
    pub(super) fn clear(&mut self) {
        self.key = None;
        self.value = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Finding #9: the directory-mtime memo only serves a value under the key
    /// it was stored with, never caches an unreadable directory, and clears.
    #[test]
    fn dir_memo_invalidates_on_key_change_and_clear() {
        use std::time::{Duration, SystemTime};

        let first = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        let second = first + Duration::from_secs(1);
        let mut memo = DirMemo::<u32>::default();
        assert!(memo.get(Some(first)).is_none(), "empty memo");
        assert!(memo.get(None).is_none(), "no key never hits");

        memo.store(Some(first), 7);
        assert_eq!(memo.get(Some(first)), Some(&7));
        assert!(memo.get(Some(second)).is_none(), "new mtime reloads");
        assert!(memo.get(None).is_none(), "unreadable dir bypasses");

        // A fresh key replaces the value.
        memo.store(Some(second), 9);
        assert_eq!(memo.get(Some(second)), Some(&9));
        assert!(memo.get(Some(first)).is_none());

        // An unreadable directory is never cached.
        memo.store(None, 11);
        assert_eq!(memo.get(Some(second)), Some(&9));

        memo.clear();
        assert!(memo.get(Some(second)).is_none());
    }

    /// Finding #9 key reader: an existing directory yields a key, a missing one
    /// yields `None` (so the caller reloads instead of pinning empty output).
    #[test]
    fn dir_mtime_key_reads_existing_and_missing_directories() {
        assert!(dir_mtime_key(&std::env::temp_dir()).is_some());
        assert!(
            dir_mtime_key(std::path::Path::new(
                "/definitely/not/a/mind-campaign-directory"
            ))
            .is_none()
        );
    }
}

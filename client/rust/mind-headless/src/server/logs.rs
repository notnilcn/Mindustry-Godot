// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server log file rotation (plan 22 §3.6/§6.5).
//!
//! Port of `ServerControl`'s log handling: `logs/log-<N>.txt` starts at the
//! first file under `maxLogLength` and rotates at the cap with an
//! `[End of log file. Date: …]` marker; color codes are stripped for the socket
//! and file output.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Rotation marker written at the end of a full log file.
pub const END_MARKER: &str = "[End of log file. Date: ";

/// Removes Mindustry color tags (`[...]`) and color codes (`&xy`).
pub fn strip_colors(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '[' => {
                i += 1;
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }
            '&' => {
                i += 1;
                let mut consumed = 0;
                while i < chars.len() && consumed < 2 && chars[i].is_ascii_alphabetic() {
                    i += 1;
                    consumed += 1;
                }
                if consumed == 0 && i < chars.len() {
                    // Not a color code; keep the ampersand + following char.
                    out.push('&');
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    out
}

/// The first `N` with no `logs/log-N.txt`, or one under `max_len`.
pub fn next_log_index(dir: &Path, max_len: u64) -> u32 {
    let mut index = 0u32;
    loop {
        let path = dir.join(format!("log-{index}.txt"));
        match std::fs::metadata(&path) {
            Ok(meta) if meta.len() < max_len => return index,
            Ok(_) => index += 1,
            Err(_) => return index,
        }
    }
}

/// A rotating log file (`logs/log-<N>.txt`).
#[derive(Debug)]
pub struct LogRotator {
    dir: PathBuf,
    max_len: u64,
    index: u32,
    size: u64,
    file: File,
}

impl LogRotator {
    /// Opens the current log file, rotating if the newest is full.
    pub fn new(dir: impl Into<PathBuf>, max_len: u64) -> std::io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        let index = next_log_index(&dir, max_len);
        let path = dir.join(format!("log-{index}.txt"));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        Ok(Self {
            dir,
            max_len,
            index,
            size,
            file,
        })
    }

    /// Path of the current log file.
    pub fn current_path(&self) -> PathBuf {
        self.dir.join(format!("log-{}.txt", self.index))
    }

    /// The current rotation index.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Appends a line (`\n` added), rotating at `max_len`.
    pub fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        let bytes = line.len() + 1;
        if self.size > 0 && self.size + bytes as u64 > self.max_len {
            self.rotate()?;
        }
        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.size += bytes as u64;
        Ok(())
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        let marker = format!("{END_MARKER}{}]\n", timestamp());
        self.file.write_all(marker.as_bytes())?;
        self.file.flush()?;
        self.index += 1;
        let path = self.dir.join(format!("log-{}.txt", self.index));
        self.file = OpenOptions::new().create(true).append(true).open(&path)?;
        self.size = 0;
        Ok(())
    }
}

fn timestamp() -> String {
    // A coarse UTC timestamp without a chrono dependency.
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("unix {seconds}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mind-srv-log-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create");
        dir
    }

    #[test]
    fn strips_colors() {
        assert_eq!(strip_colors("[red]hi &lythere&fr"), "hi there");
        assert_eq!(strip_colors("[white]plain"), "plain");
        assert_eq!(strip_colors("no codes"), "no codes");
    }

    #[test]
    fn next_index() {
        let dir = temp_dir("next");
        assert_eq!(next_log_index(&dir, 64), 0);
        std::fs::write(dir.join("log-0.txt"), b"short").expect("write");
        assert_eq!(next_log_index(&dir, 64), 0);
        std::fs::write(dir.join("log-0.txt"), vec![b'x'; 64]).expect("write");
        assert_eq!(next_log_index(&dir, 64), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotates_at_max() {
        let dir = temp_dir("rotate");
        let mut rotator = LogRotator::new(&dir, 32).expect("open");
        for i in 0..20 {
            rotator
                .write_line(&format!("line {i} padding"))
                .expect("write");
        }
        // At least one rotation happened.
        assert!(dir.join("log-1.txt").exists());
        assert!(rotator.index() >= 1);
        let content = std::fs::read_to_string(dir.join("log-0.txt")).expect("read");
        assert!(content.contains(END_MARKER));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

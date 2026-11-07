// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! GPU hardware probe seams (GAPS #70) — ports of
//! `core/src/mindustry/graphics/IntelGpuCheck.java` and
//! `core/src/mindustry/graphics/NvGpuInfo.java`.
//!
//! Both classes sniff driver capabilities the Godot host does not expose
//! directly, so the actual GL queries are injected:
//!
//! - [`IntelGpuCheck`] persists `was_intel_gpu` under the data root exactly like
//!   upstream (`ClientLauncher.setup` records the vendor for the *next* launch;
//!   `DesktopLauncher.main` reads it before creating the window so Intel drivers
//!   can be pinned to OpenGL 2). The host records the vendor it sees and reports
//!   the previous launch's answer.
//! - [`NvGpuInfo`] asks a [`GlQueries`] implementation for the
//!   `GL_NVX_gpu_memory_info` extension and the two NVX pnames. Godot has no GL
//!   pname query, so the desktop host's implementation reports "unsupported" and
//!   every value is `None`/absent — the seam is what matters, not a fake number.
//!
//! Nothing in the simulation reads these values; they only feed boot logging and
//! the platform diagnostics probe.

use std::path::Path;

/// Intel marker file name (`IntelGpuCheck`: `was_intel_gpu`).
pub const INTEL_GPU_MARKER: &str = "was_intel_gpu";

/// `GL_GPU_MEM_INFO_TOTAL_AVAILABLE_MEM_NVX`.
pub const NV_GPU_MEM_INFO_TOTAL_AVAILABLE_MEM_NVX: i32 = 0x9048;

/// `GL_GPU_MEM_INFO_CURRENT_AVAILABLE_MEM_NVX`.
pub const NV_GPU_MEM_INFO_CURRENT_AVAILABLE_MEM_NVX: i32 = 0x9049;

/// The extension `NvGpuInfo.hasMemoryInfo()` checks (`GL_NVX_gpu_memory_info`).
pub const NV_GPU_MEMORY_INFO_EXTENSION: &str = "GL_NVX_gpu_memory_info";

/// `NvGpuInfo`'s injected GL surface: extension support plus `Gl.getInt(pname)`.
pub trait GlQueries {
    /// Whether the active context supports `extension` (`Gl.supportsExtension`).
    fn supports_extension(&self, extension: &str) -> bool;
    /// Reads a GL integer pname (`Gl.getInt`).
    fn get_int(&self, pname: i32) -> i32;
}

/// Whether a driver vendor string names Intel (`IntelGpuCheck.init`'s check).
///
/// Java lower-cases and does a substring test; `"Intel(R) UHD Graphics"` and
/// `"Mesa Intel"` both match.
pub fn is_intel_vendor(vendor: &str) -> bool {
    vendor.to_lowercase().contains("intel")
}

/// Persistent "last launch used an Intel GPU" marker (`IntelGpuCheck`).
///
/// The struct caches the read result for the process lifetime
/// (`checkedLastLaunch`/`wasIntel` in Java). Construct one per host/process and
/// use the same instance for the boot read and the diagnostics probe.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IntelGpuCheck {
    checked_last_launch: bool,
    was_intel: bool,
}

impl IntelGpuCheck {
    /// A checker with no cached answer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records this launch's vendor for the next launch (`IntelGpuCheck.init`).
    ///
    /// Windows-only, like upstream. Writing `1` for an Intel vendor also creates
    /// the data root; a non-Intel vendor removes a stale marker. I/O errors are
    /// returned so the host can log them (Java catches and logs inside).
    pub fn init(
        &mut self,
        vendor: &str,
        is_windows: bool,
        data_root: &Path,
    ) -> std::io::Result<()> {
        if !is_windows {
            return Ok(());
        }
        let marker = data_root.join(INTEL_GPU_MARKER);
        if is_intel_vendor(vendor) {
            std::fs::create_dir_all(data_root)?;
            std::fs::write(&marker, b"1")
        } else if marker.exists() {
            std::fs::remove_file(&marker)
        } else {
            Ok(())
        }
    }

    /// Whether the previous launch used an Intel GPU (`IntelGpuCheck.wasIntel`).
    ///
    /// Windows-only; the first call reads and caches the marker, later calls
    /// return the cached answer even if the file changes.
    pub fn was_intel(&mut self, is_windows: bool, data_root: &Path) -> bool {
        if !is_windows {
            return false;
        }
        if self.checked_last_launch {
            return self.was_intel;
        }
        self.checked_last_launch = true;
        self.was_intel = match std::fs::read_to_string(data_root.join(INTEL_GPU_MARKER)) {
            // `IntelGpuCheck.wasIntel` compares the exact string `"1"`.
            Ok(contents) => contents == "1",
            Err(error) => {
                log::warn!(
                    "[gpu] failed to check whether the last launch used an Intel GPU: {error}"
                );
                false
            }
        };
        self.was_intel
    }

    /// Whether [`IntelGpuCheck::was_intel`] has cached an answer.
    pub fn checked(&self) -> bool {
        self.checked_last_launch
    }
}

/// NVX VRAM numbers in kilobytes (`NvGpuInfo.getMaxMemoryKB`/`getAvailableMemoryKB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvGpuMemory {
    /// Total available VRAM (`GL_GPU_MEM_INFO_TOTAL_AVAILABLE_MEM_NVX`).
    pub total_kb: i32,
    /// Currently available VRAM (`GL_GPU_MEM_INFO_CURRENT_AVAILABLE_MEM_NVX`).
    pub available_kb: i32,
}

/// NVIDIA VRAM probe (`NvGpuInfo`), with the GL query surface injected.
///
/// Upstream returns `0` when the extension is missing; this port returns `None`
/// instead (the packet's "return `Option`/`None` when unavailable"), so callers
/// can distinguish "unsupported" from "reported zero".
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NvGpuInfo {
    initialized: bool,
    supported: bool,
}

impl NvGpuInfo {
    /// A probe with no cached extension answer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the context supports `GL_NVX_gpu_memory_info`
    /// (`NvGpuInfo.hasMemoryInfo`); the query runs once and is cached.
    pub fn has_memory_info(&mut self, gl: &dyn GlQueries) -> bool {
        if !self.initialized {
            self.supported = gl.supports_extension(NV_GPU_MEMORY_INFO_EXTENSION);
            self.initialized = true;
        }
        self.supported
    }

    /// Total VRAM in KB (`NvGpuInfo.getMaxMemoryKB`), or `None` when the
    /// extension is unavailable.
    pub fn get_max_memory_kb(&mut self, gl: &dyn GlQueries) -> Option<i32> {
        self.has_memory_info(gl)
            .then(|| gl.get_int(NV_GPU_MEM_INFO_TOTAL_AVAILABLE_MEM_NVX))
    }

    /// Currently available VRAM in KB (`NvGpuInfo.getAvailableMemoryKB`), or
    /// `None` when the extension is unavailable.
    pub fn get_available_memory_kb(&mut self, gl: &dyn GlQueries) -> Option<i32> {
        self.has_memory_info(gl)
            .then(|| gl.get_int(NV_GPU_MEM_INFO_CURRENT_AVAILABLE_MEM_NVX))
    }

    /// Both NVX numbers in one call (`Gl.getInt` twice), or `None`.
    pub fn memory_info(&mut self, gl: &dyn GlQueries) -> Option<NvGpuMemory> {
        if !self.has_memory_info(gl) {
            return None;
        }
        Some(NvGpuMemory {
            total_kb: gl.get_int(NV_GPU_MEM_INFO_TOTAL_AVAILABLE_MEM_NVX),
            available_kb: gl.get_int(NV_GPU_MEM_INFO_CURRENT_AVAILABLE_MEM_NVX),
        })
    }

    /// Whether [`NvGpuInfo::has_memory_info`] has cached an answer.
    pub fn initialized(&self) -> bool {
        self.initialized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mind-gpu-{tag}-{}", std::process::id()))
    }

    #[test]
    fn intel_vendor_matching_is_case_insensitive() {
        assert!(is_intel_vendor("Intel(R) UHD Graphics 620"));
        assert!(is_intel_vendor("MESA INTEL"));
        assert!(!is_intel_vendor("NVIDIA GeForce RTX 3060"));
        assert!(!is_intel_vendor("AMD Radeon"));
        assert!(!is_intel_vendor(""));
    }

    #[test]
    fn intel_marker_round_trips_on_windows() {
        let root = temp_root("intel-roundtrip");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let mut check = IntelGpuCheck::new();
        check.init("Intel(R) UHD Graphics", true, &root).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join(INTEL_GPU_MARKER)).unwrap(),
            "1"
        );
        assert!(check.was_intel(true, &root));

        // A non-Intel launch clears the marker for the next start.
        check.init("NVIDIA GeForce", true, &root).unwrap();
        assert!(!root.join(INTEL_GPU_MARKER).exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn intel_check_is_windows_only() {
        let root = temp_root("intel-linux");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(INTEL_GPU_MARKER), b"1").unwrap();

        let mut check = IntelGpuCheck::new();
        assert!(!check.was_intel(false, &root));
        assert!(!check.checked());
        check.init("Intel", false, &root).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join(INTEL_GPU_MARKER)).unwrap(),
            "1"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn intel_answer_is_cached_for_the_process() {
        let root = temp_root("intel-cache");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(INTEL_GPU_MARKER), b"1").unwrap();

        let mut check = IntelGpuCheck::new();
        assert!(check.was_intel(true, &root));
        assert!(check.checked());

        // Deleting the marker does not change the cached answer.
        std::fs::remove_file(root.join(INTEL_GPU_MARKER)).unwrap();
        assert!(check.was_intel(true, &root));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn intel_marker_must_equal_exactly_one() {
        let root = temp_root("intel-exact");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(INTEL_GPU_MARKER), b"1\n").unwrap();

        let mut check = IntelGpuCheck::new();
        assert!(!check.was_intel(true, &root));

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Fake GL surface that counts queries.
    struct TestGl {
        supports: bool,
        total: i32,
        available: i32,
        extension_queries: Cell<u32>,
        int_queries: Cell<u32>,
    }

    impl TestGl {
        fn new(supports: bool) -> Self {
            Self {
                supports,
                total: 8_192,
                available: 4_096,
                extension_queries: Cell::new(0),
                int_queries: Cell::new(0),
            }
        }
    }

    impl GlQueries for TestGl {
        fn supports_extension(&self, extension: &str) -> bool {
            assert_eq!(extension, NV_GPU_MEMORY_INFO_EXTENSION);
            self.extension_queries.set(self.extension_queries.get() + 1);
            self.supports
        }

        fn get_int(&self, pname: i32) -> i32 {
            self.int_queries.set(self.int_queries.get() + 1);
            match pname {
                NV_GPU_MEM_INFO_TOTAL_AVAILABLE_MEM_NVX => self.total,
                NV_GPU_MEM_INFO_CURRENT_AVAILABLE_MEM_NVX => self.available,
                other => panic!("unexpected pname {other:#x}"),
            }
        }
    }

    #[test]
    fn nv_memory_is_none_when_the_extension_is_missing() {
        let gl = TestGl::new(false);
        let mut info = NvGpuInfo::new();
        assert!(!info.has_memory_info(&gl));
        assert!(info.initialized());
        assert_eq!(info.get_max_memory_kb(&gl), None);
        assert_eq!(info.get_available_memory_kb(&gl), None);
        assert_eq!(info.memory_info(&gl), None);
        // Extension probed once, no pname read.
        assert_eq!(gl.extension_queries.get(), 1);
        assert_eq!(gl.int_queries.get(), 0);
    }

    #[test]
    fn nv_memory_reads_both_pnames_when_supported() {
        let gl = TestGl::new(true);
        let mut info = NvGpuInfo::new();
        assert_eq!(info.get_max_memory_kb(&gl), Some(8_192));
        assert_eq!(info.get_available_memory_kb(&gl), Some(4_096));
        assert_eq!(
            info.memory_info(&gl),
            Some(NvGpuMemory {
                total_kb: 8_192,
                available_kb: 4_096,
            })
        );
        // Cached: supportsExtension called once for three reads.
        assert_eq!(gl.extension_queries.get(), 1);
        assert_eq!(gl.int_queries.get(), 4);
    }
}

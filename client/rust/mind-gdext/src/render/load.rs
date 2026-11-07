// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LoadRenderer` host (`graphics/LoadRenderer.java`, plan 16 §3.10/M7).
//!
//! Owns the loading-screen data: the rotating `PlanetGrid.create(2)` mesh
//! topology, the `colorRed` tint (`Pal.breakInvalid.lerp(black, 0.3)`), the ten
//! diagnostic bars and the layout constants the Java immediate-mode draw uses
//! (`LoadRenderer.draw`, `:82-317`). The Godot draw itself lives in the scene
//! consumer (the port's boot/loading fragment, owned by the `ui` pass); this
//! module is the Godot-free half, so the bar values, the fitted viewport, the
//! progress matrix and the asset log text are unit-testable.
//!
//! Deviation S16-11: the Java `Bar` array reads live engine facts (`OS.cores`,
//! `assets.getProgress()`, `graphics.getGLVersion()`, the Java heap, …); the
//! port takes the same facts as [`LoadMetrics`], so the formulas stay exact
//! while the host supplies the values.

// The whole `LoadRenderer` data surface is unread until the boot/loading-scene
// consumer (the `ui` pass's loading fragment) is wired to it: the renderer only
// re-exports `LoadRenderer` itself today. The ported bars/layout helpers are
// unit-tested below; drop this allow when the consumer lands.
#![allow(dead_code)]

use mind_core::render::drawf::pal;
use mind_core::render::g3d::PlanetGrid;
use mind_core::render::g3d::mesh_data::build_planet_grid;

/// `LoadRenderer.color = new Color(Pal.accent).lerp(Color.black, 0.5f)`.
pub fn load_color() -> [f32; 4] {
    mind_core::render::drawf::lerp(pal::ACCENT, [0.0, 0.0, 0.0, 1.0], 0.5)
}
/// `LoadRenderer.aspect` (`:157`), the fitted orthographic viewport's ratio.
pub const ASPECT: f32 = 1.94;
/// `LoadRenderer`'s matrix cell size (`space = s * 60`, `:122`).
pub const MATRIX_SPACE: f32 = 60.0;
/// `LoadRenderer`'s bar padding (`barpad = s * 8f`, `:245`).
pub const BAR_PAD: f32 = 8.0;
/// `LoadRenderer`'s bar row pitch factor (`font.getLineHeight() * 1.4f`,
/// `:246`).
pub const BAR_LINE_SCALE: f32 = 1.4;

/// The host OS the `s_osv` bar reports (`OS.isWindows`/`isLinux`/`isMac`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadOs {
    /// `OS.isWindows`: `0.35`, not red.
    Windows,
    /// `OS.isLinux`: `0.9`, not red.
    Linux,
    /// `OS.isMac`: `0.5`, red.
    Mac,
    /// Anything else: `0.2`, not red.
    Other,
}

impl LoadOs {
    /// `s_osv`'s value (`LoadRenderer.java:55`).
    pub fn value(self) -> f32 {
        match self {
            LoadOs::Windows => 0.35,
            LoadOs::Linux => 0.9,
            LoadOs::Mac => 0.5,
            LoadOs::Other => 0.2,
        }
    }

    /// `s_osv`'s red flag (`OS.isMac`).
    pub fn red(self) -> bool {
        self == LoadOs::Mac
    }
}

/// The live facts `LoadRenderer`'s bar array reads. Every field maps to one
/// Java expression in the `bars` initializer (`LoadRenderer.java:64-73`).
#[derive(Clone, Debug, PartialEq)]
pub struct LoadMetrics {
    /// `OS.cores`.
    pub cores: i32,
    /// `assets != null` (the `c_aprog` bar's `valid`).
    pub assets_ready: bool,
    /// `assets.getProgress()`.
    pub asset_progress: f32,
    /// `graphics.getGLVersion().type == GlType.GLES`.
    pub gl_es: bool,
    /// `graphics.getGLVersion().majorVersion`.
    pub gl_major: i32,
    /// `graphics.getGLVersion().minorVersion`.
    pub gl_minor: i32,
    /// `Core.app.getJavaHeap() / 1024f / 1024f` (MiB).
    pub java_heap_mb: f32,
    /// `Version.build`.
    pub version_build: i32,
    /// `Version.modifier` ("release" on a release build).
    pub version_modifier: String,
    /// `OS` family.
    pub os: LoadOs,
    /// `control.saves.getSaveSlots().size` (`-1` when saves are not loaded
    /// yet, the bar's `valid` gate).
    pub save_slots: i32,
    /// `settings.keySize()`.
    pub settings_keys: i32,
    /// `mods.list().size` (`-1` when mods are not loaded yet).
    pub mods: i32,
}

impl Default for LoadMetrics {
    fn default() -> Self {
        Self {
            cores: 4,
            assets_ready: true,
            asset_progress: 0.0,
            gl_es: false,
            gl_major: 3,
            gl_minor: 3,
            java_heap_mb: 0.0,
            version_build: 0,
            version_modifier: String::from("release"),
            os: LoadOs::Linux,
            save_slots: -1,
            settings_keys: 0,
            mods: -1,
        }
    }
}

/// One `LoadRenderer.Bar` (`:503-524`): a label key, a clamped value, the red
/// warning flag and the `valid` gate.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadBar {
    /// Bundle key (`"s_proc#"` upstream; the draw prefixes `@` for the locale).
    pub text: &'static str,
    /// `Mathf.clamp(value.get())`.
    pub value: f32,
    /// `red.get()` — draws with `colorRed`.
    pub red: bool,
    /// `valid.get()` — the row is skipped when false.
    pub valid: bool,
}

/// The ten diagnostic bars, in Java order (`LoadRenderer.java:64-73`).
pub const BAR_TEXTS: [&str; 10] = [
    "s_proc#",
    "c_aprog",
    "g_vtype",
    "s_mem#",
    "v_ver#",
    "s_osv",
    "v_worlds#",
    "c_datas#",
    "v_alterc",
    "g_vcomp#",
];

/// `LoadRenderer` host state.
#[derive(Clone, Debug)]
pub struct LoadRenderer {
    grid_size: usize,
    grid: PlanetGrid,
    color_red: u32,
    /// `assets.getLoadedAssets()` / `lastLength` (`:96-116`): the asset log is
    /// rebuilt only when the loaded count changes.
    last_asset_count: usize,
    /// The rebuilt `assetText` markup.
    asset_text: String,
    /// `renderTimes` (`WindowedMean(20)`, `:39`): frame times in seconds.
    render_times: Vec<f32>,
    /// `progress` of the last [`Self::frame`] (`assets.getProgress()`).
    progress: f32,
}

impl Default for LoadRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadRenderer {
    /// Builds the `PlanetGrid.create(2)` mesh and tint.
    pub fn new() -> Self {
        // `Pal.breakInvalid.lerp(black, 0.3)`.
        let red = mind_core::render::drawf::lerp(pal::BREAK_INVALID, [0.0, 0.0, 0.0, 1.0], 0.3);
        Self {
            grid_size: 2,
            grid: PlanetGrid::create(2),
            color_red: mind_core::render::drawf::to_bits(red),
            last_asset_count: 0,
            asset_text: String::new(),
            render_times: Vec::new(),
            progress: 0.0,
        }
    }

    /// The subdivision level (`LoadRenderer` uses `PlanetGrid.create(2)`).
    pub fn grid_size(&self) -> usize {
        self.grid_size
    }

    /// The `colorRed` tint used by `MeshBuilder.buildPlanetGrid`.
    pub fn color_red(&self) -> u32 {
        self.color_red
    }

    /// The loading-grid vertex count (for the mesh upload / stats).
    pub fn mesh_vertex_count(&self) -> usize {
        build_planet_grid(&self.grid, self.color_red, 1.0)
            .vertices
            .len()
    }

    /// `Draw.color(color)` (`:34`): `Pal.accent.lerp(black, 0.5)`.
    pub fn accent_color() -> [f32; 4] {
        load_color()
    }

    /// `LoadRenderer.bars` (`:64-73`): every row with the Java formula applied,
    /// value clamped like `Bar.value()` (`:521`).
    pub fn bars(&self, metrics: &LoadMetrics) -> Vec<LoadBar> {
        let mut bars = Vec::with_capacity(BAR_TEXTS.len());
        let mut push = |text: &'static str, value: f32, red: bool, valid: bool| {
            bars.push(LoadBar {
                text,
                value: value.clamp(0.0, 1.0),
                red,
                valid,
            });
        };
        // `new Bar("s_proc#", OS.cores / 16f, OS.cores < 4)`.
        push(
            BAR_TEXTS[0],
            metrics.cores as f32 / 16.0,
            metrics.cores < 4,
            true,
        );
        // `new Bar("c_aprog", () -> assets != null, () -> assets.getProgress(),
        //          () -> false)`.
        push(
            BAR_TEXTS[1],
            metrics.asset_progress,
            false,
            metrics.assets_ready,
        );
        // `new Bar("g_vtype", glType == GLES ? 0.5f : 1f, glType == GLES)`.
        push(
            BAR_TEXTS[2],
            if metrics.gl_es { 0.5 } else { 1.0 },
            metrics.gl_es,
            true,
        );
        // `new Bar("s_mem#", () -> true, javaHeap/200f, javaHeap > 110)`.
        push(
            BAR_TEXTS[3],
            metrics.java_heap_mb / 200.0,
            metrics.java_heap_mb > 110.0,
            true,
        );
        // `new Bar("v_ver#", build != 0, build == -1 ? 0.3f : (build - 103f)/10f,
        //          !modifier.equals("release"))`.
        push(
            BAR_TEXTS[4],
            if metrics.version_build == -1 {
                0.3
            } else {
                (metrics.version_build as f32 - 103.0) / 10.0
            },
            metrics.version_modifier != "release",
            metrics.version_build != 0,
        );
        // `new Bar("s_osv", isWindows ? 0.35 : isLinux ? 0.9 : isMac ? 0.5 : 0.2,
        //          isMac)`.
        push(BAR_TEXTS[5], metrics.os.value(), metrics.os.red(), true);
        // `new Bar("v_worlds#", control != null && control.saves != null,
        //          slots/30f, slots > 30)`.
        push(
            BAR_TEXTS[6],
            metrics.save_slots as f32 / 30.0,
            metrics.save_slots > 30,
            metrics.save_slots >= 0,
        );
        // `new Bar("c_datas#", settings.keySize() > 0, keySize/50f, keySize > 20)`.
        push(
            BAR_TEXTS[7],
            metrics.settings_keys as f32 / 50.0,
            metrics.settings_keys > 20,
            metrics.settings_keys > 0,
        );
        // `new Bar("v_alterc", mods != null, (mods.size + 1)/6f, mods.size > 0)`.
        push(
            BAR_TEXTS[8],
            (metrics.mods as f32 + 1.0) / 6.0,
            metrics.mods > 0,
            metrics.mods >= 0,
        );
        // `new Bar("g_vcomp#", (major + minor/10f)/4.6f, !atLeast(3, 2))`.
        push(
            BAR_TEXTS[9],
            (metrics.gl_major as f32 + metrics.gl_minor as f32 / 10.0) / 4.6,
            !(metrics.gl_major > 3 || (metrics.gl_major == 3 && metrics.gl_minor >= 2)),
            true,
        );
        bars
    }

    /// `assets.getProgress()` of the last [`Self::frame`] (the middle progress
    /// matrix, `:437-486`).
    pub fn progress(&self) -> f32 {
        self.progress
    }

    /// Pushes one frame's asset-log state (`:96-116`): rebuilds the text only
    /// when `assets.getLoadedAssets()` changed.
    pub fn update_assets(&mut self, loaded: usize, entries: &[(String, String)]) {
        if loaded == self.last_asset_count {
            return;
        }
        self.asset_text = asset_text(entries, "");
        self.last_asset_count = loaded;
    }

    /// The `assetText` markup (`:96-116`).
    pub fn asset_text(&self) -> &str {
        &self.asset_text
    }

    /// Records one frame time in seconds (`renderTimes.add(timespace)`, `:89`)
    /// and the frame's progress, returning the clamped progress.
    pub fn frame(&mut self, seconds: f32, progress: f32) -> f32 {
        self.render_times.push(seconds.max(0.0));
        // `WindowedMean(20)`: the last 20 frames (`:39`).
        if self.render_times.len() > 20 {
            self.render_times.remove(0);
        }
        self.progress = progress.clamp(0.0, 1.0);
        self.progress
    }

    /// `renderTimes.mean()` (`:298`): the frametime chart's scale.
    pub fn mean_frame_time(&self) -> f32 {
        if self.render_times.is_empty() {
            return 0.0;
        }
        self.render_times.iter().sum::<f32>() / self.render_times.len() as f32
    }

    /// `renderTimes.getWindowSize()` — live frame samples.
    pub fn frame_time_count(&self) -> usize {
        self.render_times.len()
    }

    /// The bar-list layout of the stencil pane (`:245-249`): at most `bars`
    /// rows fit, one row per `lineHeight * 1.4` with an 8 px pad.
    pub fn bar_layout(&self, height: f32, line_height: f32, bars: usize) -> BarLayout {
        bar_layout(height, line_height, bars)
    }
}

/// The bar column's vertical layout (`LoadRenderer.java:245-249`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarLayout {
    /// `barsUsed = min((int)((height - barpad) / (lineHeight * 1.4f)), bars.length)`.
    pub used: usize,
    /// `barspace = (height - barpad) / barsUsed`.
    pub space: f32,
    /// `barheight = barspace * 0.8f`.
    pub height: f32,
}

/// Computes the bar column layout for a pane `height`, with the Java
/// `barpad = s * 8f` and the `1.4 * lineHeight` row pitch.
pub fn bar_layout(height: f32, line_height: f32, bars: usize) -> BarLayout {
    if height <= BAR_PAD || line_height <= 0.0 || bars == 0 {
        return BarLayout {
            used: 0,
            space: 0.0,
            height: 0.0,
        };
    }
    let used = ((height - BAR_PAD) / (line_height * BAR_LINE_SCALE)) as usize;
    let used = used.min(bars);
    if used == 0 {
        return BarLayout {
            used: 0,
            space: 0.0,
            height: 0.0,
        };
    }
    let space = (height - BAR_PAD) / used as f32;
    BarLayout {
        used,
        space,
        height: space * 0.8,
    }
}

/// `Scaling.fit.apply(width, width / aspect, width, height)` (`:159-168`): the
/// fitted orthographic viewport `(x, y, w, h)`. A portrait screen uses the full
/// window instead (`:169-174`).
pub fn fit_viewport(width: f32, height: f32) -> (f32, f32, f32, f32) {
    if height > width {
        return (0.0, 0.0, width, height);
    }
    let target_h = width / ASPECT;
    // `Scaling.fit`: `scl = min(maxWidth / width, maxHeight / height)` with the
    // requested box already `(width, width / aspect)`.
    let scale = (1.0f32).min(height / target_h);
    let w = width * scale;
    let h = target_h * scale;
    (width / 2.0 - w / 2.0, height / 2.0 - h / 2.0, w, h)
}

/// The square-matrix extent (`dotw`/`doth`, `:124-125`):
/// `(int)(w / (s * 60)) / 2 + 1`.
pub fn matrix_extent(width: f32, height: f32, scale: f32) -> (i32, i32) {
    let space = (scale * MATRIX_SPACE).max(1e-3);
    (
        (width / space) as i32 / 2 + 1,
        (height / space) as i32 / 2 + 1,
    )
}

/// `LoadRenderer`'s progress matrix (`:441-447`): the bar cell count and the
/// per-cell progress span.
pub fn progress_matrix(width: f32, scale: f32) -> (i32, f32) {
    let bspace = scale * 100.0;
    let bars = ((width / bspace.max(1e-3)) as i32) / 2 + 1;
    let pscale = 1.0 / bars.max(1) as f32;
    (bars, pscale)
}

/// A colour as Arc's `Color.toString()` markup (`rrggbbaa`, lowercase): the
/// `red`/`orange` prefixes of the asset log (`:37-38`).
pub fn color_markup(color: [f32; 4]) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "[#{:02x}{:02x}{:02x}{:02x}]",
        channel(color[0]),
        channel(color[1]),
        channel(color[2]),
        channel(color[3])
    )
}

/// `LoadRenderer.assetText` (`:96-116`): one line per loaded asset, tinted red
/// for mods/previews and orange otherwise, with the username masked and path
/// separators doubled. `entries` are `(asset name, asset type simple name)`.
pub fn asset_text(entries: &[(String, String)], username: &str) -> String {
    // `colorRed = Pal.breakInvalid.lerp(black, 0.3)` and `color` (`:34-37`).
    let color_red = mind_core::render::drawf::lerp(pal::BREAK_INVALID, [0.0, 0.0, 0.0, 1.0], 0.3);
    let red = color_markup(color_red);
    let orange = color_markup(load_color());
    let mut out = String::new();
    for (name, kind) in entries {
        // `boolean isRed = name.toLowerCase().contains("mod") ||
        //  assets.getAssetType(name).getSimpleName().toLowerCase().contains("mod")
        //  || name.contains("preview")`.
        let lower_name = name.to_lowercase();
        let lower_kind = kind.to_lowercase();
        let is_red =
            lower_name.contains("mod") || lower_kind.contains("mod") || name.contains("preview");
        let masked = if username.is_empty() {
            name.clone()
        } else {
            name.replace(username, "<<host>>")
        };
        out.push_str(if is_red { &red } else { &orange });
        out.push_str(&masked.replace('/', "::"));
        out.push_str(&red);
        out.push_str("::[]");
        out.push_str(kind);
        out.push('\n');
    }
    out
}

/// `assets.getCurrentLoading()`'s category key (`:495-499`): `scripts`,
/// `content`, `mods`, `map`, `sound`, `image` or `system`.
pub fn current_asset_key(name: &str) -> &'static str {
    let name = name.to_lowercase();
    if name.contains("script") {
        "scripts"
    } else if name.contains("content") {
        "content"
    } else if name.contains("mod") {
        "mods"
    } else if name.contains("msav") || name.contains("maps") {
        "map"
    } else if name.contains("ogg") || name.contains("mp3") {
        "sound"
    } else if name.contains("png") {
        "image"
    } else {
        "system"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mindustry_red() -> [f32; 4] {
        mind_core::render::drawf::lerp(pal::BREAK_INVALID, [0.0, 0.0, 0.0, 1.0], 0.3)
    }

    #[test]
    fn bars_match_the_java_formulas() {
        let renderer = LoadRenderer::new();
        let values = LoadMetrics {
            cores: 2,
            asset_progress: 0.25,
            gl_es: true,
            java_heap_mb: 220.0,
            version_build: 146,
            version_modifier: String::from("beta"),
            os: LoadOs::Mac,
            save_slots: 45,
            settings_keys: 30,
            mods: 8,
            ..Default::default()
        };
        let bars = renderer.bars(&values);
        assert_eq!(bars.len(), BAR_TEXTS.len());
        assert_eq!(bars[0].text, "s_proc#");
        assert!((bars[0].value - 2.0 / 16.0).abs() < 1e-6);
        assert!(bars[0].red, "cores < 4 is red");
        assert!((bars[1].value - 0.25).abs() < 1e-6);
        assert_eq!(bars[2].value, 0.5, "GLES");
        assert!((bars[3].value - 1.0).abs() < 1e-6, "220 MiB clamps to 1");
        assert!(bars[3].red, "heap > 110 MiB");
        assert_eq!(bars[4].value, 1.0, "(146 - 103) / 10 clamps");
        assert!(bars[4].red, "non-release modifier");
        assert_eq!(bars[5].value, 0.5, "mac");
        assert!(bars[5].red, "mac is red");
        assert!((bars[6].value - 1.0).abs() < 1e-6, "45 slots clamp");
        assert!(bars[6].red, "slots > 30");
        assert!((bars[7].value - 0.6).abs() < 1e-6);
        assert_eq!(bars[8].value, 1.0, "(8 + 1) / 6 clamps");
        // (3 + 3/10) / 4.6 = 0.717…
        assert!((bars[9].value - (3.3 / 4.6)).abs() < 1e-6);
        assert!(!bars[9].red, "GL 3.3 is supported");
    }

    #[test]
    fn invalid_bars_are_skipped() {
        let renderer = LoadRenderer::new();
        let values = LoadMetrics {
            assets_ready: false,
            save_slots: -1,
            settings_keys: 0,
            mods: -1,
            ..Default::default()
        };
        let bars = renderer.bars(&values);
        assert!(!bars[1].valid, "assets bar hidden before assets load");
        assert!(!bars[6].valid, "saves bar hidden before saves load");
        assert!(!bars[7].valid, "settings bar hidden with no settings");
        assert!(!bars[8].valid, "mods bar hidden before mods load");
        assert!(bars[2].valid);
        assert!(bars[3].valid);
        assert!(!bars[4].valid, "build 0 is the version bar's invalid case");
    }

    #[test]
    fn bar_layout_uses_the_java_row_pitch() {
        // 400 px pane, 20 px line height: (400 - 8) / 28 = 14 rows, clamped to
        // the ten bars.
        let layout = bar_layout(400.0, 20.0, 10);
        assert_eq!(layout.used, 10);
        assert!((layout.space - 39.2).abs() < 1e-3);
        assert!((layout.height - 39.2 * 0.8).abs() < 1e-3);
        // A short pane fits fewer rows than the bar list.
        let layout = bar_layout(108.0, 20.0, 10);
        assert_eq!(layout.used, 3);
        // Degenerate panes fit none.
        assert_eq!(bar_layout(0.0, 20.0, 10).used, 0);
        assert_eq!(bar_layout(400.0, 0.0, 10).used, 0);
        assert_eq!(bar_layout(400.0, 20.0, 0).used, 0);
    }

    #[test]
    fn viewport_fits_the_194_aspect_and_centres() {
        // `Scaling.fit` keeps the 1.94 box on a 16:9 window (the fitted height
        // 989.7 is shorter than the window) and centres it.
        let (x, y, w, h) = fit_viewport(1920.0, 1080.0);
        assert!((w - 1920.0).abs() < 1e-2);
        assert!((h - 1920.0 / ASPECT).abs() < 1e-2);
        assert!((w / h - ASPECT).abs() < 1e-4);
        assert!((x - (1920.0 - w) / 2.0).abs() < 1e-2);
        assert!((y - (1080.0 - h) / 2.0).abs() < 1e-2);
        // A window shorter than the fitted height scales the pair down.
        let (x, y, w, h) = fit_viewport(1000.0, 400.0);
        assert!((w / h - ASPECT).abs() < 1e-4);
        assert!(w <= 1000.0 + 1e-3 && h <= 400.0 + 1e-3);
        assert!((x - (1000.0 - w) / 2.0).abs() < 1e-3);
        assert!((y - (400.0 - h) / 2.0).abs() < 1e-3);
        // A portrait screen uses the full window (`:169-174`).
        assert_eq!(fit_viewport(600.0, 1000.0), (0.0, 0.0, 600.0, 1000.0));
    }

    #[test]
    fn matrix_extent_matches_the_60px_cells() {
        // `(int)(1920 / 60) / 2 + 1 = 17`, `(int)(1080 / 60) / 2 + 1 = 10`.
        assert_eq!(matrix_extent(1920.0, 1080.0, 1.0), (17, 10));
        let (bars, pscale) = progress_matrix(1000.0, 1.0);
        assert_eq!(bars, 6, "(1000 / 100) / 2 + 1");
        assert!((pscale - 1.0 / 6.0).abs() < 1e-6);
    }

    #[test]
    fn asset_log_marks_mods_red_and_masks_the_host() {
        let entries = vec![
            (String::from("sprites/blocks/duo"), String::from("Texture")),
            (String::from("mods/my-mod/content"), String::from("Mod")),
            (String::from("preview/cache"), String::from("Texture")),
        ];
        let text = asset_text(&entries, "clint");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        let orange = color_markup(load_color());
        let red = color_markup(mindustry_red());
        assert!(lines[0].starts_with(&orange), "{}", lines[0]);
        assert!(lines[0].contains("sprites::blocks::duo"), "{}", lines[0]);
        assert!(lines[0].ends_with("::[]Texture"), "{}", lines[0]);
        assert!(lines[1].starts_with(&red), "{}", lines[1]);
        assert!(lines[2].starts_with(&red), "preview is red");
        // The username is masked (`:104`).
        let masked = asset_text(&[(String::from("c/clint/x"), String::from("I"))], "clint");
        assert!(masked.contains("c::<<host>>::x"), "{masked}");
        assert!(!masked.contains("clint"));
    }

    #[test]
    fn markup_is_arcs_rrggbbaa() {
        assert_eq!(color_markup([1.0, 1.0, 1.0, 1.0]), "[#ffffffff]");
        assert_eq!(color_markup([0.0, 0.0, 0.0, 1.0]), "[#000000ff]");
        assert_eq!(color_markup([1.0, 0.5, 0.0, 0.25]), "[#ff800040]");
    }

    #[test]
    fn current_asset_key_matches_java() {
        assert_eq!(current_asset_key("scripts/foo.js"), "scripts");
        assert_eq!(current_asset_key("content/blocks.json"), "content");
        assert_eq!(current_asset_key("mods/foo.jar"), "mods");
        assert_eq!(current_asset_key("maps/foo.msav"), "map");
        assert_eq!(current_asset_key("sounds/foo.ogg"), "sound");
        assert_eq!(current_asset_key("sprites/foo.png"), "image");
        assert_eq!(current_asset_key("bundle"), "system");
    }

    #[test]
    fn frame_times_keep_the_last_20() {
        let mut renderer = LoadRenderer::new();
        for i in 0..25 {
            renderer.frame(i as f32, i as f32 / 25.0);
        }
        assert_eq!(renderer.frame_time_count(), 20);
        // 5..24 seconds: mean 14.5.
        assert!((renderer.mean_frame_time() - 14.5).abs() < 1e-4);
        assert!((renderer.progress() - 24.0 / 25.0).abs() < 1e-6);
        renderer.frame(0.5, 2.0);
        assert_eq!(renderer.progress(), 1.0, "progress clamps");
    }

    #[test]
    fn asset_log_rebuilds_only_when_the_loaded_count_changes() {
        let mut renderer = LoadRenderer::new();
        let entries = vec![(String::from("a"), String::from("T"))];
        renderer.update_assets(1, &entries);
        assert!(renderer.asset_text().contains('a'));
        renderer.update_assets(1, &[]);
        assert!(
            renderer.asset_text().contains('a'),
            "unchanged count keeps the cached log"
        );
        renderer.update_assets(2, &[]);
        assert!(renderer.asset_text().is_empty());
    }

    #[test]
    fn load_renderer_keeps_the_planet_grid_mesh() {
        let renderer = LoadRenderer::new();
        assert_eq!(renderer.grid_size(), 2);
        assert!(renderer.mesh_vertex_count() > 0);
        assert_ne!(renderer.color_red(), 0);
        assert_eq!(LoadRenderer::accent_color(), load_color());
    }
}

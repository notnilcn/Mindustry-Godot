// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Behavioral reimplementation of Arc (Apache-2.0, revision 7445105cd2):
// `extensions/packer/src/arc/packer/{TexturePacker,ImageProcessor,MaxRectsPacker}.java`.

//! The texture packer (plan 03 §3.5 stage 6): image processing (ninepatch,
//! whitespace strip, aliases), maximal-rectangles bin packing and page
//! rendering (`duplicatePadding`, `bleed`).
//!
//! Determinism (§3.9): inputs are sorted by the digit-suffix comparator,
//! alias/region ordering is name-sorted, and every heuristic tie-break matches
//! Arc's evaluation order, so identical inputs produce identical pages.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::error::{AtlasError, Result};
use crate::ninepatch;
use crate::pack_json::PackSettings;
use crate::pixmaps::{self, Pixmap};
use crate::whitespace::{self, Strip};

/// One input image (relative, `/`-separated name without extension).
#[derive(Debug, Clone)]
pub struct InputImage {
    /// Staging-relative path without extension, e.g. `blocks/walls/copper-wall`
    /// (`.9` suffix kept for ninepatches).
    pub name: String,
    /// Decoded pixels (shared; the packer never copies image data).
    pub pixmap: Rc<Pixmap>,
}

/// A packed rectangle (`TexturePacker.Rect`).
#[derive(Debug, Clone)]
struct Rect {
    name: String,
    offset_x: i32,
    offset_y: i32,
    region_width: i32,
    region_height: i32,
    original_width: i32,
    original_height: i32,
    x: i32,
    y: i32,
    /// Page footprint including padding.
    width: i32,
    /// Page footprint including padding.
    height: i32,
    rotated: bool,
    splits: Option<[i32; 4]>,
    pads: Option<[i32; 4]>,
    can_rotate: bool,
    pixmap: Rc<Pixmap>,
    score1: i32,
    score2: i32,
    /// Alias entries keyed by name (deterministic order).
    aliases: BTreeMap<String, Alias>,
}

impl Rect {
    fn new(source: Rc<Pixmap>, strip: Strip, is_patch: bool) -> Rect {
        let (original_width, original_height) = (source.width as i32, source.height as i32);
        let pixmap = if strip.width == source.width
            && strip.height == source.height
            && strip.left == 0
            && strip.top == 0
        {
            source
        } else {
            Rc::new(source.crop(
                strip.left as i32,
                strip.top as i32,
                strip.width,
                strip.height,
            ))
        };
        Rect {
            name: String::new(),
            offset_x: strip.left as i32,
            offset_y: strip.top as i32,
            region_width: strip.width as i32,
            region_height: strip.height as i32,
            original_width,
            original_height,
            x: 0,
            y: 0,
            width: strip.width as i32,
            height: strip.height as i32,
            rotated: false,
            splits: None,
            pads: None,
            can_rotate: !is_patch,
            pixmap,
            score1: 0,
            score2: 0,
            aliases: BTreeMap::new(),
        }
    }

    fn blank() -> Rect {
        Rect::new(
            Rc::new(Pixmap::new(0, 0)),
            Strip {
                left: 0,
                top: 0,
                width: 0,
                height: 0,
            },
            false,
        )
    }

    fn geometry(&self) -> (i32, i32, i32, i32) {
        (self.x, self.y, self.width, self.height)
    }
}

/// An alias of a packed rect (`TexturePacker.Alias`).
#[derive(Debug, Clone)]
struct Alias {
    name: String,
    splits: Option<[i32; 4]>,
    pads: Option<[i32; 4]>,
    offset_x: i32,
    offset_y: i32,
    original_width: i32,
    original_height: i32,
}

/// A packed page (`TexturePacker.Page`).
#[derive(Debug)]
struct Page {
    output_rects: Vec<Rect>,
    remaining_rects: Vec<Rect>,
    occupancy: f32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    image_width: i32,
    image_height: i32,
}

/// One region in the finished atlas (pre-manifest form).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedRegion {
    /// Atlas name (flattened when `flattenPaths`).
    pub name: String,
    /// Page index.
    pub page: usize,
    /// X on the page image (y-down).
    pub x: i32,
    /// Y on the page image (y-down).
    pub y: i32,
    /// Region width.
    pub w: i32,
    /// Region height.
    pub h: i32,
    /// Ninepatch splits `[left, right, top, bottom]`.
    pub splits: Option<[i32; 4]>,
    /// Ninepatch pads `[left, right, top, bottom]`.
    pub pads: Option<[i32; 4]>,
    /// Whitespace-strip offsets `[offsetX, originalHeight - regionHeight - offsetY]`.
    pub offsets: [i32; 2],
    /// Original (unstripped) size.
    pub original: [i32; 2],
}

/// A finished page: rendered pixels plus metadata.
#[derive(Debug)]
pub struct PackedPage {
    /// Page index (file `sprites.png`, `sprites2.png`, ... by position).
    pub index: usize,
    /// Rendered page image.
    pub pixmap: Pixmap,
    /// Image width after pot/edge adjustments.
    pub width: i32,
    /// Image height after pot/edge adjustments.
    pub height: i32,
}

/// Pack result.
#[derive(Debug)]
pub struct PackResult {
    /// Rendered pages.
    pub pages: Vec<PackedPage>,
    /// All regions (rect + aliases), sorted by name.
    pub regions: Vec<PackedRegion>,
    /// Input names ignored as blank (deterministic order).
    pub ignored_blank: Vec<String>,
}

/// The maximal-rectangles packer entry point (`TexturePacker.pack`).
///
/// `inputs` must be decoded already; they are name-sorted with the Arc
/// digit-suffix comparator before processing.
pub fn pack_images(inputs: &[InputImage], settings: &PackSettings) -> Result<PackResult> {
    let mut sorted: Vec<&InputImage> = inputs.iter().collect();
    sorted.sort_by(|a, b| digit_suffix_cmp(&a.name, &b.name));

    // ImageProcessor pass.
    let mut rects: Vec<Rect> = Vec::new();
    let mut ignored_blank = Vec::new();
    let mut crcs: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for input in &sorted {
        match process_image(input, settings)? {
            None => ignored_blank.push(input.name.clone()),
            Some(rect) => {
                if settings.alias {
                    let crc = alias_hash(&rect.pixmap);
                    if let Some(&existing) = crcs.get(&crc) {
                        rects[existing].aliases.insert(
                            rect.name.clone(),
                            Alias {
                                name: rect.name.clone(),
                                splits: rect.splits,
                                pads: rect.pads,
                                offset_x: rect.offset_x,
                                offset_y: rect.offset_y,
                                original_width: rect.original_width,
                                original_height: rect.original_height,
                            },
                        );
                        continue;
                    }
                    crcs.insert(crc, rects.len());
                }
                rects.push(rect);
            }
        }
    }

    let mut packer = MaxRectsPacker::new(settings)?;
    let pages = packer.pack(rects)?;
    finish_pages(pages, settings, ignored_blank)
}

/// `ImageProcessor.processImage`: ninepatch strip, scale (unsupported — scale
/// is always 1 upstream), whitespace strip.
fn process_image(input: &InputImage, settings: &PackSettings) -> Result<Option<Rect>> {
    let image = &input.pixmap;
    let mut name = input.name.as_str();
    let is_patch = name.ends_with(".9");
    let mut splits = None;
    let mut pads = None;
    let mut working: Rc<Pixmap> = input.pixmap.clone();

    if is_patch {
        name = &name[..name.len() - 2];
        splits = ninepatch::get_splits(image, name)?;
        pads = ninepatch::get_pads(image, name, splits)?;
        let width = image.width - 2;
        let height = image.height - 2;
        let mut new_image = Pixmap::new(width, height);
        new_image.draw_region(image, 1, 1, width, height, 0, 0);
        working = Rc::new(new_image);
    }

    let rect = if is_patch {
        let mut rect = Rect::new(
            working.clone(),
            Strip {
                left: 0,
                top: 0,
                width: working.width,
                height: working.height,
            },
            true,
        );
        rect.splits = splits;
        rect.pads = pads;
        rect
    } else {
        match whitespace::strip_whitespace(&working, name, settings) {
            None => return Ok(None),
            Some(strip) => Rect::new(working, strip, false),
        }
    };
    let mut rect = rect;
    rect.name = name.to_owned();
    Ok(Some(rect))
}

/// sha256 over pixels + dimensions (`ImageProcessor.hash` — Arc uses SHA-1;
/// the hash only groups identical images and is not part of any ABI).
fn alias_hash(pixmap: &Pixmap) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&pixmap.pixels);
    hasher.update((pixmap.width as u32).to_be_bytes());
    hasher.update((pixmap.height as u32).to_be_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Arc's digit-suffix file comparator (`TexturePackerFileProcessor.processDir`).
pub fn digit_suffix_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn split(name: &str) -> (&str, u64) {
        let base = name.rsplit('/').next().unwrap_or(name);
        let no_ext = base.rsplit_once('.').map(|(n, _)| n).unwrap_or(base);
        let digits: String = no_ext
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if digits.is_empty() {
            (no_ext, 0)
        } else {
            (
                &no_ext[..no_ext.len() - digits.len()],
                digits.parse().unwrap_or(0),
            )
        }
    }
    let (name_a, num_a) = split(a);
    let (name_b, num_b) = split(b);
    name_a.cmp(name_b).then(num_a.cmp(&num_b))
}

/// `Rect.getAtlasName`.
fn atlas_name(name: &str, flatten: bool) -> String {
    if flatten {
        name.rsplit('/').next().unwrap_or(name).to_owned()
    } else {
        name.to_owned()
    }
}

// ---------------------------------------------------------------------------
// MaxRectsPacker
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Heuristic {
    BestShortSideFit,
    BestLongSideFit,
    BestAreaFit,
    BottomLeftRule,
    ContactPointRule,
}

const METHODS: [Heuristic; 5] = [
    Heuristic::BestShortSideFit,
    Heuristic::BestLongSideFit,
    Heuristic::BestAreaFit,
    Heuristic::BottomLeftRule,
    Heuristic::ContactPointRule,
];

struct MaxRectsPacker<'a> {
    settings: &'a PackSettings,
}

impl<'a> MaxRectsPacker<'a> {
    fn new(settings: &'a PackSettings) -> Result<Self> {
        if settings.min_width > settings.max_width {
            return Err(AtlasError::Invalid(
                "Page min width cannot be higher than max width.".into(),
            ));
        }
        if settings.min_height > settings.max_height {
            return Err(AtlasError::Invalid(
                "Page min height cannot be higher than max height.".into(),
            ));
        }
        Ok(Self { settings })
    }

    fn pack(&mut self, input_rects: Vec<Rect>) -> Result<Vec<Page>> {
        let mut input_rects = input_rects;
        for rect in &mut input_rects {
            rect.width += self.settings.padding_x;
            rect.height += self.settings.padding_y;
        }
        if self.settings.fast {
            if self.settings.rotation {
                input_rects.sort_by_key(|rect| -rect.width.max(rect.height));
            } else {
                input_rects.sort_by_key(|rect| -rect.width);
            }
        }

        let mut pages = Vec::new();
        while !input_rects.is_empty() {
            let result = self.pack_page(input_rects)?;
            input_rects = result.remaining_rects;
            pages.push(result.page);
        }
        Ok(pages)
    }

    fn pack_page(&mut self, input_rects: Vec<Rect>) -> Result<PageResult> {
        let settings = self.settings;
        let padding_x = settings.padding_x;
        let padding_y = settings.padding_y;
        let mut max_width = settings.max_width;
        let mut max_height = settings.max_height;
        if settings.edge_padding {
            if settings.duplicate_padding {
                max_width -= padding_x;
                max_height -= padding_y;
            } else {
                max_width -= padding_x * 2;
                max_height -= padding_y * 2;
            }
        }

        // Find min size.
        let mut min_width = i32::MAX;
        let mut min_height = i32::MAX;
        for rect in &input_rects {
            let width = rect.width - padding_x;
            let height = rect.height - padding_y;
            min_width = min_width.min(width);
            min_height = min_height.min(height);
            if settings.rotation {
                if (width > max_width || height > max_height)
                    && (width > max_height || height > max_width)
                {
                    return Err(AtlasError::Invalid(format!(
                        "Image does not fit with max page size {}x{} and edge padding {}*2,{}*2: {}[{},{}]",
                        settings.max_width,
                        settings.max_height,
                        padding_x,
                        padding_y,
                        rect.name,
                        width,
                        height
                    )));
                }
            } else {
                if width > max_width {
                    return Err(AtlasError::Invalid(format!(
                        "Image does not fit with max page width {}: {}[{},{}]",
                        settings.max_width, rect.name, width, height
                    )));
                }
                if height > max_height {
                    return Err(AtlasError::Invalid(format!(
                        "Image does not fit in max page height {}: {}[{},{}]",
                        settings.max_height, rect.name, width, height
                    )));
                }
            }
        }
        min_width = min_width.max(settings.min_width);
        min_height = min_height.max(settings.min_height);

        let mut adjust_x = padding_x;
        let mut adjust_y = padding_y;
        if settings.edge_padding {
            if settings.duplicate_padding {
                adjust_x -= padding_x;
                adjust_y -= padding_y;
            } else {
                adjust_x -= padding_x * 2;
                adjust_y -= padding_y * 2;
            }
        }

        let mut best_result: Option<Page> = None;
        if settings.square {
            let min_size = min_width.max(min_height);
            let max_size = settings.max_width.min(settings.max_height);
            let mut size_search = BinarySearch::new(
                min_size,
                max_size,
                if settings.fast { 25 } else { 15 },
                settings.pot,
                settings.multiple_of_four,
            );
            let mut size = size_search.reset();
            while size != -1 {
                let result =
                    self.pack_at_size(true, size + adjust_x, size + adjust_y, input_rects.clone());
                let was_none = result.is_none();
                best_result = get_best(best_result, result);
                size = size_search.next(was_none);
            }
            let mut best = match best_result {
                Some(page) => page,
                None => self
                    .pack_at_size(false, max_size + adjust_x, max_size + adjust_y, input_rects)
                    .ok_or_else(|| {
                        AtlasError::Invalid("pack: no rects fit on a max-size page".into())
                    })?,
            };
            best.output_rects.sort_by(|a, b| {
                atlas_name(&a.name, settings.flatten_paths)
                    .cmp(&atlas_name(&b.name, settings.flatten_paths))
            });
            best.width = best.width.max(best.height) - padding_x;
            best.height = best.width.max(best.height) - padding_y;
            let remaining = std::mem::take(&mut best.remaining_rects);
            Ok(PageResult {
                page: best,
                remaining_rects: remaining,
            })
        } else {
            let mut width_search = BinarySearch::new(
                min_width,
                settings.max_width,
                if settings.fast { 25 } else { 15 },
                settings.pot,
                settings.multiple_of_four,
            );
            let mut height_search = BinarySearch::new(
                min_height,
                settings.max_height,
                if settings.fast { 25 } else { 15 },
                settings.pot,
                settings.multiple_of_four,
            );
            let mut width = width_search.reset();
            let mut height = height_search.reset();
            loop {
                let mut best_width_result: Option<Page> = None;
                while width != -1 {
                    let result = self.pack_at_size(
                        true,
                        width + adjust_x,
                        height + adjust_y,
                        input_rects.clone(),
                    );
                    let was_none = result.is_none();
                    best_width_result = get_best(best_width_result, result);
                    width = width_search.next(was_none);
                }
                let had_none = best_width_result.is_none();
                best_result = get_best(best_result, best_width_result);
                height = height_search.next(had_none);
                if height == -1 {
                    break;
                }
                width = width_search.reset();
            }
            let mut best = match best_result {
                Some(page) => page,
                None => self
                    .pack_at_size(
                        false,
                        settings.max_width + adjust_x,
                        settings.max_height + adjust_y,
                        input_rects,
                    )
                    .ok_or_else(|| {
                        AtlasError::Invalid("pack: no rects fit on a max-size page".into())
                    })?,
            };
            best.output_rects.sort_by(|a, b| {
                atlas_name(&a.name, settings.flatten_paths)
                    .cmp(&atlas_name(&b.name, settings.flatten_paths))
            });
            best.width -= padding_x;
            best.height -= padding_y;
            let remaining = std::mem::take(&mut best.remaining_rects);
            Ok(PageResult {
                page: best,
                remaining_rects: remaining,
            })
        }
    }

    /// `packAtSize`: tries every heuristic; with `fully`, only results packing
    /// every rect count.
    fn pack_at_size(
        &mut self,
        fully: bool,
        width: i32,
        height: i32,
        input_rects: Vec<Rect>,
    ) -> Option<Page> {
        let mut best: Option<Page> = None;
        for method in METHODS {
            let mut max_rects = MaxRects::with_settings(width, height, self.settings);
            let result = if self.settings.fast {
                let mut remaining = Vec::new();
                let mut iter = input_rects.iter().enumerate();
                for (ii, rect) in &mut iter {
                    if max_rects.insert(rect, method).is_none() {
                        remaining.extend(input_rects.iter().skip(ii).cloned());
                        break;
                    }
                }
                let mut result = max_rects.get_result();
                result.remaining_rects = remaining;
                result
            } else {
                max_rects.pack_all(&input_rects, method)
            };
            if fully && !result.remaining_rects.is_empty() {
                continue;
            }
            if result.output_rects.is_empty() {
                continue;
            }
            best = get_best(best, Some(result));
        }
        best
    }
}

struct PageResult {
    page: Page,
    remaining_rects: Vec<Rect>,
}

fn get_best(a: Option<Page>, b: Option<Page>) -> Option<Page> {
    match (a, b) {
        (None, other) => other,
        (some, None) => some,
        (Some(x), Some(y)) => {
            if x.occupancy > y.occupancy {
                Some(x)
            } else {
                Some(y)
            }
        }
    }
}

struct BinarySearch {
    pot: bool,
    mod4: bool,
    min: i32,
    max: i32,
    fuzziness: i32,
    low: i32,
    high: i32,
    current: i32,
}

impl BinarySearch {
    fn new(min: i32, max: i32, fuzziness: i32, pot: bool, mod4: bool) -> BinarySearch {
        let (min_v, max_v, fuzz) = if pot {
            (
                log2(next_power_of_two(min)),
                log2(next_power_of_two(max)),
                0,
            )
        } else if mod4 {
            (round4(min), round4(max), fuzziness)
        } else {
            (min, max, fuzziness)
        };
        BinarySearch {
            pot,
            mod4,
            min: min_v,
            max: max_v,
            fuzziness: fuzz,
            low: 0,
            high: 0,
            current: 0,
        }
    }

    fn reset(&mut self) -> i32 {
        self.low = self.min;
        self.high = self.max;
        self.current = (self.low + self.high) >> 1;
        self.value()
    }

    fn next(&mut self, result: bool) -> i32 {
        if self.low >= self.high {
            return -1;
        }
        if result {
            self.low = self.current + 1;
        } else {
            self.high = self.current - 1;
        }
        self.current = (self.low + self.high) >> 1;
        if (self.low - self.high).abs() < self.fuzziness {
            return -1;
        }
        self.value()
    }

    fn value(&self) -> i32 {
        if self.pot {
            1 << self.current
        } else if self.mod4 {
            round4(self.current)
        } else {
            self.current
        }
    }
}

fn next_power_of_two(value: i32) -> i32 {
    if value <= 0 {
        return 1;
    }
    let mut v = value - 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v + 1
}

fn log2(pot: i32) -> i32 {
    31 - pot.leading_zeros() as i32
}

fn round4(value: i32) -> i32 {
    if value % 4 == 0 {
        value
    } else {
        value + 4 - (value % 4)
    }
}

/// A free rectangle in the bin.
#[derive(Debug, Clone, Copy)]
struct FreeRect {
    /// Unique id (Java compares candidates by reference; ids preserve that
    /// across Vec removals).
    id: u64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

struct MaxRects {
    bin_width: i32,
    bin_height: i32,
    used: Vec<Rect>,
    free: Vec<FreeRect>,
    /// Geometry snapshots of free rects added since the last prune
    /// (`rectanglesToCheckWhenPruning`; Java keeps object references, so
    /// candidates removed by a later split in the same insert still count).
    to_check: Vec<FreeRect>,
    next_id: u64,
    settings_rotation: bool,
    padding_x: i32,
    padding_y: i32,
}

impl MaxRects {
    fn new(width: i32, height: i32) -> MaxRects {
        MaxRects {
            bin_width: width,
            bin_height: height,
            used: Vec::new(),
            free: vec![FreeRect {
                id: 0,
                x: 0,
                y: 0,
                width,
                height,
            }],
            to_check: Vec::new(),
            next_id: 1,
            settings_rotation: false,
            padding_x: 0,
            padding_y: 0,
        }
    }

    fn with_settings(width: i32, height: i32, settings: &PackSettings) -> MaxRects {
        let mut rects = MaxRects::new(width, height);
        rects.settings_rotation = settings.rotation;
        rects.padding_x = settings.padding_x;
        rects.padding_y = settings.padding_y;
        rects
    }

    fn insert(&mut self, rect: &Rect, method: Heuristic) -> Option<Rect> {
        let new_node = self.score_rect(rect, method)?;
        // Literal port: only the original free nodes are visited; appends made
        // during a split are not re-split in this loop.
        let mut num = self.free.len() as i32;
        let mut i = 0i32;
        while i < num {
            if self.split_free_node(i as usize, &new_node) {
                self.free.remove(i as usize);
                i -= 1;
                num -= 1;
            }
            i += 1;
        }
        self.prune_free_list();

        let mut best = rect.clone();
        best.score1 = new_node.score1;
        best.score2 = new_node.score2;
        best.x = new_node.x;
        best.y = new_node.y;
        best.width = new_node.width;
        best.height = new_node.height;
        best.rotated = new_node.rotated;
        self.used.push(best.clone());
        Some(best)
    }

    fn pack_all(&mut self, rects: &[Rect], method: Heuristic) -> Page {
        let mut rects: Vec<Rect> = rects.to_vec();
        while !rects.is_empty() {
            let mut best_index = -1i32;
            let mut best_node: Option<Rect> = None;
            for (i, rect) in rects.iter().enumerate() {
                if let Some(node) = self.score_rect(rect, method) {
                    let better = match &best_node {
                        None => true,
                        Some(best) => {
                            node.score1 < best.score1
                                || (node.score1 == best.score1 && node.score2 < best.score2)
                        }
                    };
                    if better {
                        best_node = Some(node);
                        best_index = i as i32;
                    }
                }
            }
            if best_index == -1 {
                break;
            }
            let node = best_node.unwrap_or_else(Rect::blank);
            self.place_rect(&node);
            rects.remove(best_index as usize);
        }
        let mut result = self.get_result();
        result.remaining_rects = rects;
        result
    }

    fn get_result(&self) -> Page {
        let mut w = 0;
        let mut h = 0;
        for rect in &self.used {
            w = w.max(rect.x + rect.width);
            h = h.max(rect.y + rect.height);
        }
        Page {
            output_rects: self.used.clone(),
            remaining_rects: Vec::new(),
            occupancy: self.occupancy(),
            x: 0,
            y: 0,
            width: w,
            height: h,
            image_width: 0,
            image_height: 0,
        }
    }

    fn place_rect(&mut self, node: &Rect) {
        let mut num = self.free.len() as i32;
        let mut i = 0i32;
        while i < num {
            if self.split_free_node(i as usize, node) {
                self.free.remove(i as usize);
                i -= 1;
                num -= 1;
            }
            i += 1;
        }
        self.prune_free_list();
        self.used.push(node.clone());
    }

    fn score_rect(&self, rect: &Rect, method: Heuristic) -> Option<Rect> {
        let width = rect.width;
        let height = rect.height;
        let rotated_width = height - self.padding_y + self.padding_x;
        let rotated_height = width - self.padding_x + self.padding_y;
        let rotate = rect.can_rotate && self.settings_rotation;

        let mut node = match method {
            Heuristic::BestShortSideFit => {
                self.find_best_short_side_fit(width, height, rotated_width, rotated_height, rotate)
            }
            Heuristic::BottomLeftRule => {
                self.find_bottom_left(width, height, rotated_width, rotated_height, rotate)
            }
            Heuristic::ContactPointRule => {
                let mut n =
                    self.find_contact_point(width, height, rotated_width, rotated_height, rotate)?;
                n.score1 = -n.score1;
                Some(n)
            }
            Heuristic::BestLongSideFit => {
                self.find_best_long_side_fit(width, height, rotated_width, rotated_height, rotate)
            }
            Heuristic::BestAreaFit => {
                self.find_best_area_fit(width, height, rotated_width, rotated_height, rotate)
            }
        }?;

        if node.height == 0 {
            node.score1 = i32::MAX;
            node.score2 = i32::MAX;
            return None;
        }
        Some(node)
    }

    fn occupancy(&self) -> f32 {
        let used_area: i64 = self
            .used
            .iter()
            .map(|rect| rect.width as i64 * rect.height as i64)
            .sum();
        used_area as f32 / (self.bin_width * self.bin_height) as f32
    }

    fn blank_node(&self) -> Rect {
        Rect::blank()
    }

    fn find_bottom_left(
        &self,
        width: i32,
        height: i32,
        rotated_width: i32,
        rotated_height: i32,
        rotate: bool,
    ) -> Option<Rect> {
        let mut best: Option<Rect> = None;
        let mut best_score1 = i32::MAX;
        let mut best_score2 = i32::MAX;
        for free in &self.free {
            if free.width >= width && free.height >= height {
                let top_side_y = free.y + height;
                if top_side_y < best_score1 || (top_side_y == best_score1 && free.x < best_score2) {
                    let mut node = self.blank_node();
                    node.x = free.x;
                    node.y = free.y;
                    node.width = width;
                    node.height = height;
                    node.score1 = top_side_y;
                    node.score2 = free.x;
                    node.rotated = false;
                    best_score1 = top_side_y;
                    best_score2 = free.x;
                    best = Some(node);
                }
            }
            if rotate && free.width >= rotated_width && free.height >= rotated_height {
                let top_side_y = free.y + rotated_height;
                if top_side_y < best_score1 || (top_side_y == best_score1 && free.x < best_score2) {
                    let mut node = self.blank_node();
                    node.x = free.x;
                    node.y = free.y;
                    node.width = rotated_width;
                    node.height = rotated_height;
                    node.score1 = top_side_y;
                    node.score2 = free.x;
                    node.rotated = true;
                    best_score1 = top_side_y;
                    best_score2 = free.x;
                    best = Some(node);
                }
            }
        }
        best
    }

    fn find_best_short_side_fit(
        &self,
        width: i32,
        height: i32,
        rotated_width: i32,
        rotated_height: i32,
        rotate: bool,
    ) -> Option<Rect> {
        let mut best: Option<Rect> = None;
        let mut best_score1 = i32::MAX;
        let mut best_score2 = i32::MAX;
        for free in &self.free {
            if free.width >= width && free.height >= height {
                let leftover_h = (free.width - width).abs();
                let leftover_v = (free.height - height).abs();
                let short = leftover_h.min(leftover_v);
                let long = leftover_h.max(leftover_v);
                if short < best_score1 || (short == best_score1 && long < best_score2) {
                    let mut node = self.blank_node();
                    node.x = free.x;
                    node.y = free.y;
                    node.width = width;
                    node.height = height;
                    node.score1 = short;
                    node.score2 = long;
                    node.rotated = false;
                    best_score1 = short;
                    best_score2 = long;
                    best = Some(node);
                }
            }
            if rotate && free.width >= rotated_width && free.height >= rotated_height {
                let leftover_h = (free.width - rotated_width).abs();
                let leftover_v = (free.height - rotated_height).abs();
                let short = leftover_h.min(leftover_v);
                let long = leftover_h.max(leftover_v);
                if short < best_score1 || (short == best_score1 && long < best_score2) {
                    let mut node = self.blank_node();
                    node.x = free.x;
                    node.y = free.y;
                    node.width = rotated_width;
                    node.height = rotated_height;
                    node.score1 = short;
                    node.score2 = long;
                    node.rotated = true;
                    best_score1 = short;
                    best_score2 = long;
                    best = Some(node);
                }
            }
        }
        best
    }

    fn find_best_long_side_fit(
        &self,
        width: i32,
        height: i32,
        rotated_width: i32,
        rotated_height: i32,
        rotate: bool,
    ) -> Option<Rect> {
        let mut best: Option<Rect> = None;
        let mut best_score1 = i32::MAX;
        let mut best_score2 = i32::MAX;
        for free in &self.free {
            if free.width >= width && free.height >= height {
                let leftover_h = (free.width - width).abs();
                let leftover_v = (free.height - height).abs();
                let short = leftover_h.min(leftover_v);
                let long = leftover_h.max(leftover_v);
                if long < best_score2 || (long == best_score2 && short < best_score1) {
                    let mut node = self.blank_node();
                    node.x = free.x;
                    node.y = free.y;
                    node.width = width;
                    node.height = height;
                    node.score1 = short;
                    node.score2 = long;
                    node.rotated = false;
                    best_score1 = short;
                    best_score2 = long;
                    best = Some(node);
                }
            }
            if rotate && free.width >= rotated_width && free.height >= rotated_height {
                let leftover_h = (free.width - rotated_width).abs();
                let leftover_v = (free.height - rotated_height).abs();
                let short = leftover_h.min(leftover_v);
                let long = leftover_h.max(leftover_v);
                if long < best_score2 || (long == best_score2 && short < best_score1) {
                    let mut node = self.blank_node();
                    node.x = free.x;
                    node.y = free.y;
                    node.width = rotated_width;
                    node.height = rotated_height;
                    node.score1 = short;
                    node.score2 = long;
                    node.rotated = true;
                    best_score1 = short;
                    best_score2 = long;
                    best = Some(node);
                }
            }
        }
        best
    }

    fn find_best_area_fit(
        &self,
        width: i32,
        height: i32,
        rotated_width: i32,
        rotated_height: i32,
        rotate: bool,
    ) -> Option<Rect> {
        let mut best: Option<Rect> = None;
        let mut best_score1 = i32::MAX;
        let mut best_score2 = i32::MAX;
        for free in &self.free {
            let area_fit = free.width * free.height - width * height;
            if free.width >= width && free.height >= height {
                let leftover_h = (free.width - width).abs();
                let leftover_v = (free.height - height).abs();
                let short = leftover_h.min(leftover_v);
                if area_fit < best_score1 || (area_fit == best_score1 && short < best_score2) {
                    let mut node = self.blank_node();
                    node.x = free.x;
                    node.y = free.y;
                    node.width = width;
                    node.height = height;
                    node.score1 = area_fit;
                    node.score2 = short;
                    node.rotated = false;
                    best_score1 = area_fit;
                    best_score2 = short;
                    best = Some(node);
                }
            }
            if rotate && free.width >= rotated_width && free.height >= rotated_height {
                let leftover_h = (free.width - rotated_width).abs();
                let leftover_v = (free.height - rotated_height).abs();
                let short = leftover_h.min(leftover_v);
                if area_fit < best_score1 || (area_fit == best_score1 && short < best_score2) {
                    let mut node = self.blank_node();
                    node.x = free.x;
                    node.y = free.y;
                    node.width = rotated_width;
                    node.height = rotated_height;
                    node.score1 = area_fit;
                    node.score2 = short;
                    node.rotated = true;
                    best_score1 = area_fit;
                    best_score2 = short;
                    best = Some(node);
                }
            }
        }
        best
    }

    fn find_contact_point(
        &self,
        width: i32,
        height: i32,
        rotated_width: i32,
        rotated_height: i32,
        rotate: bool,
    ) -> Option<Rect> {
        let mut best: Option<Rect> = None;
        let mut best_score = -1;
        for free in &self.free {
            if free.width >= width && free.height >= height {
                let score = self.contact_score(free.x, free.y, width, height);
                if score > best_score {
                    let mut node = self.blank_node();
                    node.x = free.x;
                    node.y = free.y;
                    node.width = width;
                    node.height = height;
                    node.score1 = score;
                    node.rotated = false;
                    best_score = score;
                    best = Some(node);
                }
            }
            if rotate && free.width >= rotated_width && free.height >= rotated_height {
                let score = self.contact_score(free.x, free.y, rotated_width, rotated_height);
                if score > best_score {
                    let mut node = self.blank_node();
                    node.x = free.x;
                    node.y = free.y;
                    node.width = rotated_width;
                    node.height = rotated_height;
                    node.score1 = score;
                    node.rotated = true;
                    best_score = score;
                    best = Some(node);
                }
            }
        }
        best
    }

    fn contact_score(&self, x: i32, y: i32, width: i32, height: i32) -> i32 {
        let mut score = 0;
        if x == 0 || x + width == self.bin_width {
            score += height;
        }
        if y == 0 || y + height == self.bin_height {
            score += width;
        }
        for rect in &self.used {
            if rect.x == x + width || rect.x + rect.width == x {
                score += common_interval_length(rect.y, rect.y + rect.height, y, y + height);
            }
            if rect.y == y + height || rect.y + rect.height == y {
                score += common_interval_length(rect.x, rect.x + rect.width, x, x + width);
            }
        }
        score
    }

    fn split_free_node(&mut self, free_index: usize, used: &Rect) -> bool {
        let free_node = self.free[free_index];
        let (ux, uy, uw, uh) = used.geometry();
        if ux >= free_node.x + free_node.width
            || ux + uw <= free_node.x
            || uy >= free_node.y + free_node.height
            || uy + uh <= free_node.y
        {
            return false;
        }
        let mut added = Vec::new();
        if ux < free_node.x + free_node.width && ux + uw > free_node.x {
            // New node at the top side of the used node.
            if uy > free_node.y && uy < free_node.y + free_node.height {
                added.push(FreeRect {
                    id: self.fresh_id(),
                    x: free_node.x,
                    y: free_node.y,
                    width: free_node.width,
                    height: uy - free_node.y,
                });
            }
            // New node at the bottom side of the used node.
            if uy + uh < free_node.y + free_node.height {
                added.push(FreeRect {
                    id: self.fresh_id(),
                    x: free_node.x,
                    y: uy + uh,
                    width: free_node.width,
                    height: free_node.y + free_node.height - (uy + uh),
                });
            }
        }
        if uy < free_node.y + free_node.height && uy + uh > free_node.y {
            // New node at the left side of the used node.
            if ux > free_node.x && ux < free_node.x + free_node.width {
                added.push(FreeRect {
                    id: self.fresh_id(),
                    x: free_node.x,
                    y: free_node.y,
                    width: ux - free_node.x,
                    height: free_node.height,
                });
            }
            // New node at the right side of the used node.
            if ux + uw < free_node.x + free_node.width {
                added.push(FreeRect {
                    id: self.fresh_id(),
                    x: ux + uw,
                    y: free_node.y,
                    width: free_node.x + free_node.width - (ux + uw),
                    height: free_node.height,
                });
            }
        }
        for rect in &added {
            self.to_check.push(*rect);
        }
        self.free.extend(added);
        true
    }

    fn fresh_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn prune_free_list(&mut self) {
        // Arc `rectanglesToCheckWhenPruning`: only newly added candidates are
        // checked against the list (the O(n) per insert fast path).
        let mut to_remove: Vec<usize> = Vec::new();
        for checking in self.to_check.drain(..) {
            for (i, rect) in self.free.iter().enumerate() {
                if rect.id == checking.id {
                    continue;
                }
                if rect.x >= checking.x
                    && rect.y >= checking.y
                    && rect.x + rect.width <= checking.x + checking.width
                    && rect.y + rect.height <= checking.y + checking.height
                {
                    to_remove.push(i);
                }
            }
        }
        to_remove.sort_unstable_by(|a, b| b.cmp(a));
        to_remove.dedup();
        for index in to_remove {
            self.free.remove(index);
        }
    }
}

fn common_interval_length(i1start: i32, i1end: i32, i2start: i32, i2end: i32) -> i32 {
    if i1end < i2start || i2end < i1start {
        return 0;
    }
    i1end.min(i2end) - i1start.max(i2start)
}

// ---------------------------------------------------------------------------
// Page rendering (TexturePacker.writeImages / writePackFile)
// ---------------------------------------------------------------------------

fn finish_pages(
    pages: Vec<Page>,
    settings: &PackSettings,
    mut ignored_blank: Vec<String>,
) -> Result<PackResult> {
    let mut out_pages = Vec::with_capacity(pages.len());
    let mut out_regions = Vec::new();

    for (index, mut page) in pages.into_iter().enumerate() {
        let mut width = page.width;
        let mut height = page.height;
        if settings.edge_padding {
            let mut edge_pad_x = settings.padding_x;
            let mut edge_pad_y = settings.padding_y;
            if settings.duplicate_padding {
                edge_pad_x /= 2;
                edge_pad_y /= 2;
            }
            page.x = edge_pad_x;
            page.y = edge_pad_y;
            width += edge_pad_x * 2;
            height += edge_pad_y * 2;
        }
        if settings.pot {
            width = next_power_of_two(width);
            height = next_power_of_two(height);
        }
        if settings.multiple_of_four {
            width = round4(width);
            height = round4(height);
        }
        width = width.max(settings.min_width);
        height = height.max(settings.min_height);
        page.image_width = width;
        page.image_height = height;

        let mut canvas = Pixmap::new(width as usize, height as usize);
        for rect in &page.output_rects {
            let image = &rect.pixmap;
            let iw = image.width as i32;
            let ih = image.height as i32;
            let rect_x = page.x + rect.x;
            let rect_y = page.y + page.height - rect.y - (rect.height - settings.padding_y);
            if settings.duplicate_padding {
                let amount_x = settings.padding_x / 2;
                let amount_y = settings.padding_y / 2;
                if rect.rotated {
                    // Copy corner pixels to fill corners of the padding.
                    for i in 1..=amount_x {
                        for j in 1..=amount_y {
                            canvas.set(rect_x - j, rect_y + iw - 1 + i, image.get(0, 0));
                            canvas.set(
                                rect_x + ih - 1 + j,
                                rect_y + iw - 1 + i,
                                image.get(0, ih - 1),
                            );
                            canvas.set(rect_x - j, rect_y - i, image.get(iw - 1, 0));
                            canvas.set(rect_x + ih - 1 + j, rect_y - i, image.get(iw - 1, ih - 1));
                        }
                    }
                    for i in 1..=amount_y {
                        for j in 0..iw {
                            canvas.set(rect_x - i, rect_y + iw - 1 - j, image.get(j, 0));
                            canvas.set(
                                rect_x + ih - 1 + i,
                                rect_y + iw - 1 - j,
                                image.get(j, ih - 1),
                            );
                        }
                    }
                    for i in 1..=amount_x {
                        for j in 0..ih {
                            canvas.set(rect_x + j, rect_y - i, image.get(iw - 1, j));
                            canvas.set(rect_x + j, rect_y + iw - 1 + i, image.get(0, j));
                        }
                    }
                } else {
                    for i in 1..=amount_x {
                        for j in 1..=amount_y {
                            canvas.set(rect_x - i, rect_y - j, image.get(0, 0));
                            canvas.set(rect_x - i, rect_y + ih - 1 + j, image.get(0, ih - 1));
                            canvas.set(rect_x + iw - 1 + i, rect_y - j, image.get(iw - 1, 0));
                            canvas.set(
                                rect_x + iw - 1 + i,
                                rect_y + ih - 1 + j,
                                image.get(iw - 1, ih - 1),
                            );
                        }
                    }
                    for i in 1..=amount_y {
                        copy_pixels(
                            image,
                            0,
                            0,
                            iw,
                            1,
                            &mut canvas,
                            rect_x,
                            rect_y - i,
                            rect.rotated,
                        );
                        copy_pixels(
                            image,
                            0,
                            ih - 1,
                            iw,
                            1,
                            &mut canvas,
                            rect_x,
                            rect_y + ih - 1 + i,
                            rect.rotated,
                        );
                    }
                    for i in 1..=amount_x {
                        copy_pixels(
                            image,
                            0,
                            0,
                            1,
                            ih,
                            &mut canvas,
                            rect_x - i,
                            rect_y,
                            rect.rotated,
                        );
                        copy_pixels(
                            image,
                            iw - 1,
                            0,
                            1,
                            ih,
                            &mut canvas,
                            rect_x + iw - 1 + i,
                            rect_y,
                            rect.rotated,
                        );
                    }
                }
            }
            copy_pixels(
                image,
                0,
                0,
                iw,
                ih,
                &mut canvas,
                rect_x,
                rect_y,
                rect.rotated,
            );
        }

        if settings.bleed {
            pixmaps::bleed(&mut canvas, settings.bleed_iterations);
        }

        // Region entries (rect + aliases), Arc writeRect math.
        for rect in &page.output_rects {
            push_region(&mut out_regions, rect, None, index, &page, settings);
            for alias in rect.aliases.values() {
                push_region(&mut out_regions, rect, Some(alias), index, &page, settings);
            }
        }

        out_pages.push(PackedPage {
            index,
            pixmap: canvas,
            width,
            height,
        });
    }

    out_regions.sort_by(|a, b| a.name.cmp(&b.name));
    out_regions.shrink_to_fit();
    ignored_blank.sort();
    Ok(PackResult {
        pages: out_pages,
        regions: out_regions,
        ignored_blank,
    })
}

fn push_region(
    out: &mut Vec<PackedRegion>,
    rect: &Rect,
    alias: Option<&Alias>,
    page_index: usize,
    page: &Page,
    settings: &PackSettings,
) {
    let (name, splits, pads, offset_x, offset_y, original_width, original_height) = match alias {
        Some(alias) => (
            alias.name.clone(),
            alias.splits,
            alias.pads,
            alias.offset_x,
            alias.offset_y,
            alias.original_width,
            alias.original_height,
        ),
        None => (
            rect.name.clone(),
            rect.splits,
            rect.pads,
            rect.offset_x,
            rect.offset_y,
            rect.original_width,
            rect.original_height,
        ),
    };
    let offsets = if original_width != rect.region_width || original_height != rect.region_height {
        [offset_x, original_height - rect.region_height - offset_y]
    } else {
        [0, 0]
    };
    out.push(PackedRegion {
        name: atlas_name(&name, settings.flatten_paths),
        page: page_index,
        x: page.x + rect.x,
        y: page.y + page.height - rect.y - (rect.height - settings.padding_y),
        w: rect.region_width,
        h: rect.region_height,
        splits,
        pads,
        offsets,
        original: [original_width, original_height],
    });
}

#[allow(clippy::too_many_arguments)]
fn copy_pixels(
    src: &Pixmap,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    dst: &mut Pixmap,
    dx: i32,
    dy: i32,
    rotated: bool,
) {
    if rotated {
        for i in 0..w {
            for j in 0..h {
                let value = src.get(x + i, y + j);
                dst.set(dx + j, dy + w - i - 1, value);
            }
        }
    } else {
        dst.draw_full(
            src, x, y, w as usize, h as usize, dx, dy, w as usize, h as usize, false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixmaps::rgba8888;

    fn solid(name: &str, w: usize, h: usize, color: u32) -> InputImage {
        let mut pix = Pixmap::new(w, h);
        pix.fill(color);
        InputImage {
            name: name.to_owned(),
            pixmap: Rc::new(pix),
        }
    }

    fn root_settings() -> PackSettings {
        PackSettings {
            duplicate_padding: true,
            combine_subdirectories: true,
            flatten_paths: true,
            max_width: 4096,
            max_height: 4096,
            fast: true,
            strip_whitespace_center: true,
            ignored_whitespace_strings: vec!["effects/".into()],
            ..PackSettings::default()
        }
    }

    #[test]
    fn packs_one_image_deterministically() {
        let inputs = vec![solid(
            "blocks/walls/copper-wall",
            32,
            32,
            rgba8888(200, 120, 40, 255),
        )];
        let first = pack_images(&inputs, &root_settings()).unwrap();
        let second = pack_images(&inputs, &root_settings()).unwrap();
        assert_eq!(first.pages.len(), 1);
        assert_eq!(first.regions.len(), 1);
        let region = &first.regions[0];
        assert_eq!(region.name, "copper-wall");
        assert_eq!(region.w, 32);
        assert_eq!(region.h, 32);
        assert_eq!(region.page, 0);
        assert_eq!(first.pages[0].pixmap, second.pages[0].pixmap);
        assert_eq!(first.regions, second.regions);
    }

    #[test]
    fn digit_suffix_ordering() {
        let mut names = vec![
            "wall10".to_owned(),
            "wall2".to_owned(),
            "wall1".to_owned(),
            "wall".to_owned(),
        ];
        names.sort_by(|a, b| digit_suffix_cmp(a, b));
        assert_eq!(names, vec!["wall", "wall1", "wall2", "wall10"]);
    }

    #[test]
    fn aliases_deduplicate_identical_images() {
        let inputs = vec![
            solid("a-one", 8, 8, rgba8888(1, 2, 3, 255)),
            solid("b-two", 8, 8, rgba8888(1, 2, 3, 255)),
            solid("c-three", 8, 8, rgba8888(9, 9, 9, 255)),
        ];
        let result = pack_images(&inputs, &root_settings()).unwrap();
        let names: Vec<&str> = result.regions.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["a-one", "b-two", "c-three"]);
        // The alias shares the canonical rect's position.
        let a = &result.regions[0];
        let b = &result.regions[1];
        assert_eq!((a.x, a.y, a.page), (b.x, b.y, b.page));
    }

    #[test]
    fn ninepatch_splits_flow_through() {
        // 34x34 .9.png with a 1px border marking the split range.
        let mut pix = Pixmap::new(34, 34);
        for y in 1..33 {
            for x in 1..33 {
                pix.set_raw(x, y, rgba8888(60, 60, 60, 255));
            }
        }
        for x in 2..30 {
            pix.set_raw(x, 0, rgba8888(0, 0, 0, 255));
            pix.set_raw(0, x, rgba8888(0, 0, 0, 255));
        }
        let inputs = vec![InputImage {
            name: "ui/bar.9".to_owned(),
            pixmap: Rc::new(pix),
        }];
        let result = pack_images(&inputs, &root_settings()).unwrap();
        let region = &result.regions[0];
        assert_eq!(region.name, "bar");
        // left = 2-1 = 1; right = 34-2-(30-1) = 3 (inset from the right edge).
        let splits = region.splits.unwrap();
        assert_eq!(splits, [1, 3, 1, 3]);
    }

    #[test]
    fn blank_images_are_ignored() {
        // The blank-ignore path requires whitespace stripping without
        // duplicatePadding's 1px margins (upstream configs keep a tiny
        // transparent crop instead — that's how `blank.png` survives packing).
        let settings = PackSettings {
            strip_whitespace_x: true,
            strip_whitespace_y: true,
            duplicate_padding: false,
            flatten_paths: true,
            max_width: 4096,
            max_height: 4096,
            ..PackSettings::default()
        };
        let inputs = vec![
            solid("empty", 8, 8, 0),
            solid("full", 8, 8, rgba8888(5, 5, 5, 255)),
        ];
        let result = pack_images(&inputs, &settings).unwrap();
        assert_eq!(result.regions.len(), 1);
        assert_eq!(result.ignored_blank, vec!["empty".to_owned()]);
    }

    #[test]
    fn blank_image_under_upstream_settings_survives_as_tiny_region() {
        // Upstream quirk (Arc `ImageProcessor.stripWhitespace` +
        // duplicatePadding): a fully transparent image is cropped to a small
        // transparent rect and packed, never ignored.
        let inputs = vec![solid("shapes/blank", 32, 32, 0)];
        let result = pack_images(&inputs, &root_settings()).unwrap();
        assert_eq!(result.regions.len(), 1);
        assert_eq!(result.regions[0].name, "blank");
        assert_eq!(result.regions[0].w, 4);
        assert_eq!(result.regions[0].h, 4);
    }

    #[test]
    fn overflow_spills_to_second_page() {
        let settings = PackSettings {
            max_width: 64,
            max_height: 64,
            ..root_settings()
        };
        let inputs: Vec<InputImage> = (0..8)
            .map(|i| solid(&format!("big-{i}"), 30, 30, rgba8888(i * 30, 0, 0, 255)))
            .collect();
        let result = pack_images(&inputs, &settings).unwrap();
        assert!(result.pages.len() >= 2);
        for region in &result.regions {
            assert!(region.x + region.w <= result.pages[region.page].width);
            assert!(region.y + region.h <= result.pages[region.page].height);
        }
    }

    #[test]
    fn flatten_paths_strips_directories() {
        let inputs = vec![solid(
            "blocks/walls/copper-wall",
            32,
            32,
            rgba8888(1, 1, 1, 255),
        )];
        let result = pack_images(&inputs, &root_settings()).unwrap();
        assert_eq!(result.regions[0].name, "copper-wall");
        let mut no_flatten = root_settings();
        no_flatten.flatten_paths = false;
        let result = pack_images(&inputs, &no_flatten).unwrap();
        assert_eq!(result.regions[0].name, "blocks/walls/copper-wall");
    }
}

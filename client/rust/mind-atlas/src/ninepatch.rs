// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Arc (Apache-2.0) `ImageProcessor.getSplits/getPads/getSplitPoint`.

//! `.9.png` ninepatch split/pad detection (plan 03 §3.5 stage 6, R4).

use crate::error::{AtlasError, Result};
use crate::pixmaps::{Pixmap, ai, bi, gi, ri};

/// Splits for a ninepatch image *including* its 1 px border, or `None` when
/// there are no splits (or the split covers everything). Returns
/// `[left, right, top, bottom]` (Arc order).
pub fn get_splits(image: &Pixmap, name: &str) -> Result<Option<[i32; 4]>> {
    let start_x = get_split_point(image, name, 1, 0, true, true)?;
    let end_x = get_split_point(image, name, start_x, 0, false, true)?;
    let start_y = get_split_point(image, name, 0, 1, true, false)?;
    let end_y = get_split_point(image, name, 0, start_y, false, false)?;

    // Ensure pixels after the end are not invalid.
    get_split_point(image, name, end_x + 1, 0, true, true)?;
    get_split_point(image, name, 0, end_y + 1, true, false)?;

    if start_x == 0 && end_x == 0 && start_y == 0 && end_y == 0 {
        return Ok(None);
    }

    // -1 here is because the coordinates were computed before the 1px border
    // was stripped.
    let (mut sx, mut ex, mut sy, mut ey) = (start_x, end_x, start_y, end_y);
    if sx != 0 {
        sx -= 1;
        ex = image.width as i32 - 2 - (ex - 1);
    } else {
        ex = image.width as i32 - 2;
    }
    if sy != 0 {
        sy -= 1;
        ey = image.height as i32 - 2 - (ey - 1);
    } else {
        ey = image.height as i32 - 2;
    }

    Ok(Some([sx, ex, sy, ey]))
}

/// Pads for a ninepatch image, or `None` when absent or equal to the splits.
pub fn get_pads(image: &Pixmap, name: &str, splits: Option<[i32; 4]>) -> Result<Option<[i32; 4]>> {
    let bottom = image.height as i32 - 1;
    let right = image.width as i32 - 1;

    let mut start_x = get_split_point(image, name, 1, bottom, true, true)?;
    let mut start_y = get_split_point(image, name, right, 1, true, false)?;

    let mut end_x = 0;
    let mut end_y = 0;
    if start_x != 0 {
        end_x = get_split_point(image, name, start_x + 1, bottom, false, true)?;
    }
    if start_y != 0 {
        end_y = get_split_point(image, name, right, start_y + 1, false, false)?;
    }

    get_split_point(image, name, end_x + 1, bottom, true, true)?;
    get_split_point(image, name, right, end_y + 1, true, false)?;

    if start_x == 0 && end_x == 0 && start_y == 0 && end_y == 0 {
        return Ok(None);
    }

    if start_x == 0 && end_x == 0 {
        start_x = -1;
        end_x = -1;
    } else if start_x > 0 {
        start_x -= 1;
        end_x = image.width as i32 - 2 - (end_x - 1);
    } else {
        end_x = image.width as i32 - 2;
    }
    if start_y == 0 && end_y == 0 {
        start_y = -1;
        end_y = -1;
    } else if start_y > 0 {
        start_y -= 1;
        end_y = image.height as i32 - 2 - (end_y - 1);
    } else {
        end_y = image.height as i32 - 2;
    }

    let pads = [start_x, end_x, start_y, end_y];
    if splits == Some(pads) {
        return Ok(None);
    }
    Ok(Some(pads))
}

/// Hunts for the start or end of a sequence of split pixels
/// (`ImageProcessor.getSplitPoint`).
fn get_split_point(
    image: &Pixmap,
    name: &str,
    start_x: i32,
    start_y: i32,
    start_point: bool,
    x_axis: bool,
) -> Result<i32> {
    let mut next = if x_axis { start_x } else { start_y };
    let end = if x_axis { image.width } else { image.height } as i32;
    let break_a = if start_point { 255 } else { 0 };

    let mut x = start_x;
    let mut y = start_y;
    while next != end {
        if x_axis {
            x = next;
        } else {
            y = next;
        }
        let rgba = image.get(x, y);
        let (r, g, b, a) = (ri(rgba), gi(rgba), bi(rgba), ai(rgba));
        if a as i32 == break_a {
            return Ok(next);
        }
        if !start_point && (r != 0 || g != 0 || b != 0 || a != 255) {
            return Err(AtlasError::Invalid(format!(
                "Invalid {name} ninepatch split pixel at {x}, {y}, rgba: {r}, {g}, {b}, {a}"
            )));
        }
        next += 1;
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixmaps::rgba8888;

    /// Builds a 9-patch with the given split ranges marked on the border.
    fn ninepatch(
        w: usize,
        h: usize,
        xs: Option<(usize, usize)>,
        ys: Option<(usize, usize)>,
    ) -> Pixmap {
        let mut pix = Pixmap::new(w, h);
        // Content area: solid gray.
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                pix.set_raw(x, y, rgba8888(128, 128, 128, 255));
            }
        }
        // Top border: split marks (black, alpha 255).
        if let Some((a, b)) = xs {
            for x in a..=b {
                pix.set_raw(x, 0, rgba8888(0, 0, 0, 255));
            }
        }
        // Left border: split marks.
        if let Some((a, b)) = ys {
            for y in a..=b {
                pix.set_raw(0, y, rgba8888(0, 0, 0, 255));
            }
        }
        pix
    }

    #[test]
    fn split_detection() {
        // 10x10 image, x split 2..4, y split 3..5 (border coords).
        let pix = ninepatch(10, 10, Some((2, 4)), Some((3, 5)));
        let splits = get_splits(&pix, "test.9").unwrap().unwrap();
        // Arc semantics: left/top are insets from the left/top, right/bottom are
        // insets from the right/bottom edge of the border-stripped content.
        // startX=2 -> left = 1; endX=5 -> right = width-2-(5-1) = 10-2-4 = 4.
        assert_eq!(splits, [1, 4, 2, 3]);
    }

    #[test]
    fn no_splits_returns_none() {
        let pix = ninepatch(10, 10, None, None);
        assert_eq!(get_splits(&pix, "test.9").unwrap(), None);
    }

    #[test]
    fn invalid_split_pixel_errors() {
        let mut pix = ninepatch(10, 10, Some((2, 4)), None);
        // Non-black, non-transparent pixel inside the split range — invalid.
        pix.set_raw(3, 0, rgba8888(255, 0, 0, 255));
        assert!(get_splits(&pix, "test.9").is_err());
    }
}

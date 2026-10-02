// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Arc (Apache-2.0) `ImageProcessor.stripWhitespace` (center + X/Y variants).

//! Whitespace stripping for the packer (plan 03 §6.2 `stripWhitespaceCenter`).

use crate::pack_json::PackSettings;
use crate::pixmaps::Pixmap;

/// Result of a whitespace strip: source crop rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strip {
    /// Left crop offset in the source image.
    pub left: usize,
    /// Top crop offset in the source image.
    pub top: usize,
    /// Cropped width.
    pub width: usize,
    /// Cropped height.
    pub height: usize,
}

/// Strips whitespace per the settings; returns `None` when the image is blank
/// and `ignoreBlankImages` is set. Ported from `ImageProcessor.stripWhitespace`.
pub fn strip_whitespace(source: &Pixmap, name: &str, settings: &PackSettings) -> Option<Strip> {
    let thresh = settings.alpha_threshold;

    if settings
        .ignored_whitespace_strings
        .iter()
        .any(|s| name.contains(s.as_str()))
    {
        return Some(Strip {
            left: 0,
            top: 0,
            width: source.width,
            height: source.height,
        });
    }

    if settings.strip_whitespace_center && source.width > 3 && source.height > 3 {
        let mut crop_x = 0usize;
        let mut crop_y = 0usize;
        let max_crop = source.width.min(source.height) / 2 - 1;
        'outer: while crop_y < max_crop {
            for x in 0..source.width {
                if source.get_a(x, crop_y) as i32 > thresh {
                    break 'outer;
                }
                if source.get_a(x, source.height - 1 - crop_y) as i32 > thresh {
                    break 'outer;
                }
            }
            crop_y += 1;
        }
        'outer: while crop_x < max_crop {
            for y in 0..source.height {
                if source.get_a(crop_x, y) as i32 > thresh {
                    break 'outer;
                }
                if source.get_a(source.width - 1 - crop_x, y) as i32 > thresh {
                    break 'outer;
                }
            }
            crop_x += 1;
        }

        // Add a pixel of padding.
        let real_crop_x = crop_x.saturating_sub(1);
        let real_crop_y = crop_y.saturating_sub(1);
        if real_crop_x > 0 || real_crop_y > 0 {
            return Some(Strip {
                left: real_crop_x,
                top: real_crop_y,
                width: source.width - real_crop_x * 2,
                height: source.height - real_crop_y * 2,
            });
        }
    }

    if !settings.strip_whitespace_x && !settings.strip_whitespace_y {
        return Some(Strip {
            left: 0,
            top: 0,
            width: source.width,
            height: source.height,
        });
    }

    let mut top = 0usize;
    let mut bottom = source.height;
    if settings.strip_whitespace_y {
        'outer: for y in 0..source.height {
            for x in 0..source.width {
                if source.get_a(x, y) as i32 > thresh {
                    break 'outer;
                }
            }
            top += 1;
        }
        'outer: for y in (top..source.height).rev() {
            for x in 0..source.width {
                if source.get_a(x, y) as i32 > thresh {
                    break 'outer;
                }
            }
            bottom -= 1;
        }
        // Leave 1px so nothing is copied into padding.
        if settings.duplicate_padding {
            top = top.saturating_sub(1);
            if bottom < source.height {
                bottom += 1;
            }
        }
    }
    let mut left = 0usize;
    let mut right = source.width;
    if settings.strip_whitespace_x {
        'outer: for x in 0..source.width {
            for y in top..bottom {
                if source.get_a(x, y) as i32 > thresh {
                    break 'outer;
                }
            }
            left += 1;
        }
        'outer: for x in (left..source.width).rev() {
            for y in top..bottom {
                if source.get_a(x, y) as i32 > thresh {
                    break 'outer;
                }
            }
            right -= 1;
        }
        if settings.duplicate_padding {
            left = left.saturating_sub(1);
            if right < source.width {
                right += 1;
            }
        }
    }
    let new_width = right.saturating_sub(left);
    let new_height = bottom.saturating_sub(top);
    if new_width == 0 || new_height == 0 {
        if settings.ignore_blank_images {
            return None;
        }
        return Some(Strip {
            left: 0,
            top: 0,
            width: 1,
            height: 1,
        });
    }
    Some(Strip {
        left,
        top,
        width: new_width,
        height: new_height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixmaps::rgba8888;

    fn settings_center() -> PackSettings {
        PackSettings {
            strip_whitespace_center: true,
            duplicate_padding: true,
            ..PackSettings::default()
        }
    }

    #[test]
    fn center_strip_keeps_one_pixel_margin() {
        // 32x32 with a 16x16 opaque center.
        let mut pix = Pixmap::new(32, 32);
        for y in 8..24 {
            for x in 8..24 {
                pix.set_raw(x, y, rgba8888(255, 0, 0, 255));
            }
        }
        let strip = strip_whitespace(&pix, "test", &settings_center()).unwrap();
        // crop stops at 8, minus 1 padding -> 7 each side.
        assert_eq!(
            strip,
            Strip {
                left: 7,
                top: 7,
                width: 32 - 14,
                height: 32 - 14
            }
        );
    }

    #[test]
    fn ignored_paths_skip_stripping() {
        let settings = PackSettings {
            strip_whitespace_center: true,
            ignored_whitespace_strings: vec!["effects/".into()],
            ..PackSettings::default()
        };
        let pix = Pixmap::new(32, 32);
        let strip = strip_whitespace(&pix, "effects/fx-1", &settings).unwrap();
        assert_eq!(strip.width, 32);
        assert_eq!(strip.height, 32);
    }

    #[test]
    fn blank_image_ignored() {
        let pix = Pixmap::new(8, 8);
        let settings = PackSettings {
            strip_whitespace_x: true,
            strip_whitespace_y: true,
            ..PackSettings::default()
        };
        assert_eq!(strip_whitespace(&pix, "blank", &settings), None);
    }
}

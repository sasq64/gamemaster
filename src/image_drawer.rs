#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::RgbaImage;

use crate::draw::PixelCanvas;

#[derive(Default, Clone)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub palette: Vec<u32>,
    pub pixels: Vec<u8>,
}

pub struct ImageDrawer {
    pub pcanvas: PixelCanvas,
    pub colors: [u32; 8],
    pub palette: Vec<u32>,
    pub bitmaps: HashMap<u32, Bitmap>,
    pub left_status: String,
    pub right_status: String,
}

impl Default for ImageDrawer {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageDrawer {
    pub fn new() -> Self {
        Self {
            pcanvas: PixelCanvas::new(160, 96),
            colors: [
                0x000000, 0xFF0000, 0x30E830, 0xFFFF00, 0x0000FF, 0xA06800, 0x00FFFF, 0xFFFFFF,
            ],
            palette: vec![0; 64],
            bitmaps: HashMap::new(),
            left_status: "".into(),
            right_status: "".into(),
        }
    }

    pub fn get_statusbar(&self) -> (&str, &str) {
        (&self.left_status, &self.right_status)
    }

    /// Parse an integer literal the way Python's `int(s, 0)` does: accepts
    /// decimal, `0x`/`0o`/`0b` prefixes, and an optional leading sign.
    fn parse_int(s: &str) -> Option<i64> {
        let (neg, rest) = match s.as_bytes().first() {
            Some(b'-') => (true, &s[1..]),
            Some(b'+') => (false, &s[1..]),
            _ => (false, s),
        };
        let val = if let Some(r) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
            i64::from_str_radix(r, 16).ok()?
        } else if let Some(r) = rest.strip_prefix("0o").or_else(|| rest.strip_prefix("0O")) {
            i64::from_str_radix(r, 8).ok()?
        } else if let Some(r) = rest.strip_prefix("0b").or_else(|| rest.strip_prefix("0B")) {
            i64::from_str_radix(r, 2).ok()?
        } else {
            rest.parse::<i64>().ok()?
        };
        Some(if neg { -val } else { val })
    }

    /// Feed a text command line. Returns `true` if the visible canvas state
    /// changed (caller may want to re-render).
    pub fn add_text_command(&mut self, s: &str) -> bool {
        let mut parts = s.split_whitespace();
        let Some(cmd) = parts.next() else {
            return false;
        };
        let args: Vec<i64> = parts.filter_map(Self::parse_int).collect();

        match cmd {
            "status" => {
                let sides = s.split("::");
                let sides: Vec<_> = sides.collect();
                self.left_status = sides[1].into();
                self.right_status = (if sides.len() > 2 { sides[2] } else { "" }).into();
                false
            }
            "img" if args.len() == 4 => {
                let no = args[0] as u32;
                self.bitmaps.insert(
                    no,
                    Bitmap {
                        width: args[1] as u32,
                        height: args[2] as u32,
                        ..Default::default()
                    },
                );
                false
            }
            "pal" if !args.is_empty() => {
                let no = args[0] as u32;
                if let Some(b) = self.bitmaps.get_mut(&no) {
                    b.palette = args[1..].iter().map(|&n| n as u32).collect();
                }
                false
            }
            "pixels" if !args.is_empty() => {
                let no = args[0] as u32;
                if let Some(b) = self.bitmaps.get_mut(&no) {
                    b.pixels = args[1..].iter().map(|&n| n as u8).collect();
                }
                false
            }
            "imgsize" if args.len() >= 2 => {
                self.pcanvas = PixelCanvas::new(args[0] as u32, args[1] as u32);
                false
            }
            "line" if args.len() >= 5 => {
                let target = args.get(5).map(|&n| n as u8);
                self.pcanvas.draw_line(
                    args[0] as i32,
                    args[1] as i32,
                    args[2] as i32,
                    args[3] as i32,
                    args[4] as u8,
                    target,
                );
                true
            }
            "fill" if args.len() >= 4 => {
                self.pcanvas.flood_fill(
                    args[0] as i32,
                    args[1] as i32,
                    args[2] as u8,
                    args[3] as u8,
                );
                true
            }
            "clear" => {
                self.pcanvas.clear(0);
                true
            }
            "setcolor" if args.len() >= 2 => {
                let c_idx = args[1] as usize;
                let p_idx = args[0] as usize;
                if c_idx < self.colors.len() && p_idx < self.palette.len() {
                    self.palette[p_idx] = (self.colors[c_idx] << 8) | 0xFF;
                }
                true
            }
            "bitmap" if !args.is_empty() => {
                let no = args[0] as u32;
                let Some(bmp) = self.bitmaps.get(&no) else {
                    return false;
                };
                let mut canvas = PixelCanvas::new(bmp.width, bmp.height);
                canvas.set_pixels(&bmp.pixels);
                self.pcanvas = canvas;
                self.palette = bmp.palette.iter().map(|&c| (c << 8) | 0xFF).collect();
                true
            }
            _ => false,
        }
    }

    pub fn get_canvas_size(&self) -> (u32, u32) {
        (self.pcanvas.width, self.pcanvas.height)
    }

    pub fn get_image(&self) -> Result<RgbaImage> {
        let w = self.pcanvas.width;
        let h = self.pcanvas.height;
        let mut rgba = Vec::with_capacity((w as usize) * (h as usize) * 4);
        for &idx in &self.pcanvas.array {
            let p = self.palette.get(idx as usize).copied().unwrap_or(0);
            rgba.push(((p >> 24) & 0xFF) as u8);
            rgba.push(((p >> 16) & 0xFF) as u8);
            rgba.push(((p >> 8) & 0xFF) as u8);
            rgba.push((p & 0xFF) as u8);
        }
        Ok(RgbaImage::from_raw(w, h, rgba).unwrap())
    }

    /// Encode current canvas to a PNG at `path` (RGBA8).
    pub fn write_png(&self, path: &Path) -> Result<()> {
        let rgba = self.get_image()?;
        rgba.save(path)?;
        Ok(())
    }

    pub fn get_scaled_image(&self, scale: u32) -> Result<image::RgbaImage> {
        let src = self.get_image()?;
        let sw = src.width() * scale;
        let sh = src.height() * scale;

        let src_img = FirImage::from_vec_u8(
            src.width(),
            src.height(),
            src.as_raw().clone(),
            PixelType::U8x4,
        )
        .context("fir source image")?;
        let mut dst_img = FirImage::new(sw, sh, PixelType::U8x4);

        let mut resizer = Resizer::new();
        let opts = ResizeOptions::new().resize_alg(ResizeAlg::Nearest);
        resizer
            .resize(&src_img, &mut dst_img, &opts)
            .context("fir resize")?;

        image::RgbaImage::from_raw(sw, sh, dst_img.into_vec())
            .context("build RgbaImage from fir buffer")
    }

    /// Write `game.png` in the current working directory and return its path.
    pub fn get_png(&self) -> Result<PathBuf> {
        let path = PathBuf::from("game.png");
        self.write_png(&path)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_filled_box_and_write_png() {
        let mut d = ImageDrawer::new();

        // 32x32 canvas, palette: 0 black bg, 1 white outline, 2 red fill.
        let cmds = [
            "imgsize 32 32",
            "setcolor 0 0",
            "setcolor 1 7",
            "setcolor 2 1",
            "clear",
            "line 5 5 25 5 1",
            "line 25 5 25 25 1",
            "line 25 25 5 25 1",
            "line 5 25 5 5 1",
            "fill 15 15 2 0",
        ];
        for c in cmds {
            d.add_text_command(c);
        }

        // Canvas sanity: corners are outline, interior is fill, outside is bg.
        let at = |x: u32, y: u32| d.pcanvas.array[(y * d.pcanvas.width + x) as usize];
        assert_eq!(at(5, 5), 1, "top-left corner should be outline");
        assert_eq!(at(25, 25), 1, "bottom-right corner should be outline");
        assert_eq!(at(15, 15), 2, "interior should be flood-filled");
        assert_eq!(at(0, 0), 0, "outside box should remain background");

        let path = std::env::temp_dir().join("gamemaster_test_box.png");
        d.write_png(&path).expect("png write");

        let bytes = std::fs::read(&path).expect("read png");
        assert!(bytes.len() > 8, "png should be non-empty");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "png magic bytes");

        let _ = std::fs::remove_file(&path);
    }
}

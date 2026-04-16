//! Bitmap drawing utilities using a simple 1D pixel buffer.
//!
//! `PixelCanvas` treats `array` as a flat, mutable 1D buffer representing a
//! `w` by `h` bitmap in row-major order. Pixels are addressed at index
//! `y * w + x`.

pub struct PixelCanvas {
    pub array: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl PixelCanvas {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            array: vec![0; (w as usize) * (h as usize)],
            width: w,
            height: h,
        }
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && (x as u32) < self.width && y >= 0 && (y as u32) < self.height
    }

    fn index(&self, x: i32, y: i32) -> usize {
        (y as usize) * (self.width as usize) + (x as usize)
    }

    pub fn clear(&mut self, color: u8) {
        self.array.fill(color);
    }

    pub fn set_pixels(&mut self, pixels: &[u8]) {
        let n = self.array.len().min(pixels.len());
        self.array[..n].copy_from_slice(&pixels[..n]);
    }

    /// Stack-based scanline flood fill. Fills the region containing `(x, y)`
    /// with `col`, but only replaces pixels currently equal to `target_col`.
    pub fn flood_fill(&mut self, x: i32, y: i32, col: u8, target_col: u8) {
        if col == target_col {
            return;
        }
        if !self.in_bounds(x, y) {
            return;
        }
        if self.array[self.index(x, y)] != target_col {
            return;
        }

        let mut stack: Vec<(i32, i32)> = vec![(x, y)];

        while let Some((cx, cy)) = stack.pop() {
            if !self.in_bounds(cx, cy) {
                continue;
            }
            if self.array[self.index(cx, cy)] != target_col {
                continue;
            }

            let mut left = cx;
            while left >= 0 && self.array[self.index(left, cy)] == target_col {
                let i = self.index(left, cy);
                self.array[i] = col;
                left -= 1;
            }
            left += 1;

            let mut right = cx + 1;
            while (right as u32) < self.width
                && self.array[self.index(right, cy)] == target_col
            {
                let i = self.index(right, cy);
                self.array[i] = col;
                right += 1;
            }
            right -= 1;

            for i in left..=right {
                if cy - 1 >= 0 && self.array[self.index(i, cy - 1)] == target_col {
                    stack.push((i, cy - 1));
                }
                if (cy + 1) < self.height as i32
                    && self.array[self.index(i, cy + 1)] == target_col
                {
                    stack.push((i, cy + 1));
                }
            }
        }
    }

    /// Bresenham line. If `target_color` is `Some(c)`, only overwrites pixels
    /// currently equal to `c`. Writes only to in-bounds pixels.
    pub fn draw_line(
        &mut self,
        mut x0: i32,
        mut y0: i32,
        x1: i32,
        y1: i32,
        col: u8,
        target_color: Option<u8>,
    ) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            if self.in_bounds(x0, y0) {
                let i = self.index(x0, y0);
                let current = self.array[i];
                if target_color.is_none_or(|t| t == current) {
                    self.array[i] = col;
                }
            }
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }
}

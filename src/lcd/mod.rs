mod font;

pub const LCD_W: usize = 160;
pub const LCD_H: usize = 43;

/// A 160×43 1-bit framebuffer, row-major. Out-of-bounds writes are ignored so
/// rendering never panics.
pub struct Framebuffer {
    pixels: Vec<bool>,
}

impl Framebuffer {
    pub fn new() -> Self {
        Self { pixels: vec![false; LCD_W * LCD_H] }
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, on: bool) {
        if x < 0 || y < 0 || x as usize >= LCD_W || y as usize >= LCD_H {
            return;
        }
        self.pixels[y as usize * LCD_W + x as usize] = on;
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        if x >= LCD_W || y >= LCD_H {
            return false;
        }
        self.pixels[y * LCD_W + x]
    }

    pub fn draw_hline(&mut self, x: i32, y: i32, len: i32) {
        for i in 0..len {
            self.set_pixel(x + i, y, true);
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, on: bool) {
        for dy in 0..h {
            for dx in 0..w {
                self.set_pixel(x + dx, y + dy, on);
            }
        }
    }

    /// Pack into the 992-byte G13 LCD frame: 32-byte header (`[0]=0x03`) + 960
    /// bytes of 6-page column data. Pixel (x,y) -> bit y%8 of byte 32 + x + (y/8)*160.
    pub fn pack(&self) -> [u8; 992] {
        let mut frame = [0u8; 992];
        frame[0] = 0x03;
        for y in 0..LCD_H {
            for x in 0..LCD_W {
                if self.pixels[y * LCD_W + x] {
                    frame[32 + x + (y / 8) * LCD_W] |= 1 << (y % 8);
                }
            }
        }
        frame
    }

    /// Draw a 5-column glyph at (x,y); each set bit becomes a `scale`×`scale` block.
    pub fn draw_glyph(&mut self, x: i32, y: i32, glyph: &[u8; 5], scale: i32) {
        for (col, bits) in glyph.iter().enumerate() {
            for row in 0..8 {
                if bits & (1 << row) != 0 {
                    self.fill_rect(
                        x + col as i32 * scale,
                        y + row * scale,
                        scale, scale, true,
                    );
                }
            }
        }
    }

    pub fn draw_char(&mut self, x: i32, y: i32, ch: char, scale: i32) {
        self.draw_glyph(x, y, font::glyph(ch), scale);
    }

    /// Draw a string left-to-right; each cell is 6px wide (5 glyph + 1 gap) × scale.
    pub fn draw_text(&mut self, x: i32, y: i32, text: &str, scale: i32) {
        let mut cx = x;
        for ch in text.chars() {
            self.draw_char(cx, y, ch, scale);
            cx += 6 * scale;
        }
    }
}

/// Pixel width a string occupies: 6px (5 glyph + 1 gap) per char × scale.
pub fn text_width(text: &str, scale: i32) -> i32 {
    text.chars().count() as i32 * 6 * scale
}

impl Default for Framebuffer {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_pixel_is_bounds_safe() {
        let mut fb = Framebuffer::new();
        fb.set_pixel(-1, 0, true);      // no panic
        fb.set_pixel(0, 999, true);     // no panic
        fb.set_pixel(5, 6, true);
        assert!(fb.get(5, 6));
        assert!(!fb.get(6, 5));
    }

    #[test]
    fn pack_layout_is_992_bytes_with_header() {
        let fb = Framebuffer::new();
        let frame = fb.pack();
        assert_eq!(frame.len(), 992);
        assert_eq!(frame[0], 0x03);
        assert!(frame[1..32].iter().all(|&b| b == 0));
        assert!(frame[32..].iter().all(|&b| b == 0)); // blank fb -> zero body
    }

    #[test]
    fn pack_maps_pixel_to_correct_byte_and_bit() {
        let mut fb = Framebuffer::new();
        // (x=3, y=10): page = 10/8 = 1, bit = 10%8 = 2, byte = 32 + 3 + 1*160 = 195.
        fb.set_pixel(3, 10, true);
        let frame = fb.pack();
        assert_eq!(frame[195], 1 << 2);
        // (x=0, y=0): byte 32, bit 0.
        let mut fb2 = Framebuffer::new();
        fb2.set_pixel(0, 0, true);
        assert_eq!(fb2.pack()[32], 1 << 0);
        // (x=159, y=42): page 5, bit 2, byte 32 + 159 + 5*160 = 991.
        let mut fb3 = Framebuffer::new();
        fb3.set_pixel(159, 42, true);
        assert_eq!(fb3.pack()[991], 1 << 2);
    }

    #[test]
    fn draw_hline_and_fill_rect() {
        let mut fb = Framebuffer::new();
        fb.draw_hline(2, 4, 3); // (2,4),(3,4),(4,4)
        assert!(fb.get(2, 4) && fb.get(3, 4) && fb.get(4, 4));
        assert!(!fb.get(5, 4));
        fb.fill_rect(0, 0, 2, 2, true);
        assert!(fb.get(0, 0) && fb.get(1, 1));
        assert!(!fb.get(2, 2));
    }

    #[test]
    fn draw_glyph_places_bits_top_to_bottom() {
        let mut fb = Framebuffer::new();
        // Synthetic glyph: column 0 has row 0 and row 2 set; other columns empty.
        let g = [0b0000_0101u8, 0, 0, 0, 0];
        fb.draw_glyph(10, 20, &g, 1);
        assert!(fb.get(10, 20));       // col 0, row 0
        assert!(!fb.get(10, 21));      // row 1 clear
        assert!(fb.get(10, 22));       // col 0, row 2
        assert!(!fb.get(11, 20));      // col 1 empty
    }

    #[test]
    fn draw_glyph_scale_2_doubles_each_pixel() {
        let mut fb = Framebuffer::new();
        let g = [0b0000_0001u8, 0, 0, 0, 0]; // col 0, row 0 only
        fb.draw_glyph(0, 0, &g, 2);
        // one source pixel -> a 2x2 block
        assert!(fb.get(0, 0) && fb.get(1, 0) && fb.get(0, 1) && fb.get(1, 1));
        assert!(!fb.get(2, 0) && !fb.get(0, 2));
    }

    #[test]
    fn text_width_is_six_px_per_char_times_scale() {
        assert_eq!(text_width("AB", 1), 12);
        assert_eq!(text_width("AB", 2), 24);
        assert_eq!(text_width("", 1), 0);
    }

    #[test]
    fn draw_text_and_space_glyph_is_blank() {
        let mut fb = Framebuffer::new();
        fb.draw_text(0, 0, " ", 1); // space -> nothing set, no panic
        assert!((0..6).all(|x| (0..8).all(|y| !fb.get(x, y))));
        fb.draw_text(150, 0, "ABCDEFG", 1); // runs off the right edge -> no panic
    }
}

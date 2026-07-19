//! Renders "g13" (from the embedded console font) into an RGBA buffer for the
//! tray / window / exe icons.
use crate::g13_glyphs::G13_GLYPHS;

/// "g13" as RGBA on a **square** canvas, centered horizontally and vertically:
/// `fg` where a glyph bit is set, `bg` elsewhere. Each source pixel is a
/// `scale`×`scale` block. Square avoids aspect-ratio stretch when the OS drops it
/// into a square icon slot (tray / title-bar / taskbar).
pub fn render_g13_rgba(scale: u32, fg: [u8; 4], bg: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let scale = scale.max(1);
    let pad = 2 * scale;
    let cols = 3 * 6 - 1; // 3 glyphs of 5 cols + 1 gap between = 17
    let rows = 8u32;
    let text_w = cols * scale;
    let text_h = rows * scale;
    // Width-dominated square; text centered both axes.
    let side = text_w + pad * 2;
    let ox = (side - text_w) / 2;
    let oy = (side - text_h) / 2;
    let mut buf = vec![0u8; (side * side * 4) as usize];
    for px in buf.chunks_mut(4) {
        px.copy_from_slice(&bg);
    }
    for (gi, glyph) in G13_GLYPHS.iter().enumerate() {
        let gx = ox + gi as u32 * 6 * scale; // 5 cols + 1 gap per glyph
        for (col, bits) in glyph.iter().enumerate() {
            for row in 0..rows {
                if bits & (1 << row) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let x = gx + col as u32 * scale + dx;
                            let y = oy + row * scale + dy;
                            let idx = ((y * side + x) * 4) as usize;
                            buf[idx..idx + 4].copy_from_slice(&fg);
                        }
                    }
                }
            }
        }
    }
    (buf, side, side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_dimensions_and_colors() {
        let fg = [255, 0, 0, 255];
        let bg = [0, 0, 0, 255];
        let (buf, w, h) = render_g13_rgba(2, fg, bg);
        assert_eq!(buf.len() as u32, w * h * 4);
        assert_eq!(w, h, "icon must be square (no aspect-ratio stretch)");
        // A corner is padding → background.
        assert_eq!(&buf[0..4], &bg);
        // At least some foreground pixels were drawn (the glyphs).
        assert!(buf.chunks(4).any(|px| px == fg));
    }

    #[test]
    fn scale_grows_the_canvas() {
        let (_, w1, h1) = render_g13_rgba(1, [255; 4], [0; 4]);
        let (_, w2, h2) = render_g13_rgba(2, [255; 4], [0; 4]);
        assert!(w2 > w1 && h2 > h1);
    }
}

// The three glyphs for the "g13" app/tray/exe icon. Copied from src/lcd/font.rs
// (glcdfont) so build.rs (which can't use crate code) and src/icon.rs share one source.
// Each glyph is 5 columns; bit `b` of a column byte is row `b` (top→bottom).
//
// NOTE: use regular `//` comments here, not `//!` inner doc comments — build.rs
// `include!`s this file at module scope, and an inner doc comment must lead the whole
// enclosing scope (build.rs already has items above the include), so `//!` won't parse.
pub const G13_GLYPHS: [[u8; 5]; 3] = [
    // 'g' (font.rs index 71)
    [0x18, 0xA4, 0xA4, 0x9C, 0x78],
    // '1' (index 17)
    [0x00, 0x42, 0x7F, 0x40, 0x00],
    // '3' (index 19)
    [0x21, 0x41, 0x49, 0x4D, 0x33],
];

use std::fs;

// Included at module (item) scope rather than inside `fn main` — `include!` splices raw
// tokens at its call site, and the file's top-level `pub const` item is only valid Rust
// syntax where an item is expected, not in a function's statement position.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/g13_glyphs.rs"));

fn main() {
    let version = fs::read_to_string("version.txt")
        .expect("version.txt not found at crate root")
        .trim()
        .to_string();
    assert!(!version.is_empty(), "version.txt is empty");
    println!("cargo:rustc-env=G13_VERSION={version}");
    println!("cargo:rerun-if-changed=version.txt");

    // ---- exe icon: render "g13" and embed as a .ico (Windows target only) ----
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Render "g13" into a `size`×`size` RGBA (glyph centered, max integer scale).
        fn render(size: u32) -> ico::IconImage {
            let cols = 3 * 6 - 1; // 17
            let rows = 8u32;
            let scale = ((size.saturating_sub(2)) / cols).max(1).min((size.saturating_sub(2)) / rows).max(1);
            let tw = cols * scale;
            let th = rows * scale;
            // saturating: at size=16, "g13" needs 17 cols even at the minimum scale of 1,
            // so tw > size and centering would otherwise underflow.
            let ox = size.saturating_sub(tw) / 2;
            let oy = size.saturating_sub(th) / 2;
            let mut rgba = vec![0u8; (size * size * 4) as usize]; // transparent bg
            for (gi, glyph) in G13_GLYPHS.iter().enumerate() {
                let gx = ox + gi as u32 * 6 * scale;
                for (col, bits) in glyph.iter().enumerate() {
                    for row in 0..rows {
                        if bits & (1 << row) != 0 {
                            for dy in 0..scale {
                                for dx in 0..scale {
                                    let x = gx + col as u32 * scale + dx;
                                    let y = oy + row * scale + dy;
                                    if x >= size || y >= size {
                                        continue; // clip glyph pixels that overflow the canvas
                                    }
                                    let idx = ((y * size + x) * 4) as usize;
                                    rgba[idx..idx + 4].copy_from_slice(&[230, 230, 230, 255]);
                                }
                            }
                        }
                    }
                }
            }
            ico::IconImage::from_rgba_data(size, size, rgba)
        }

        let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
        for size in [16u32, 32, 48, 256] {
            icon_dir.add_entry(ico::IconDirEntry::encode(&render(size)).unwrap());
        }
        let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("g13.ico");
        icon_dir.write(std::fs::File::create(&out).unwrap()).unwrap();

        let mut res = winres::WindowsResource::new();
        res.set_icon(out.to_str().unwrap());
        if let Err(e) = res.compile() {
            println!("cargo:warning=exe icon embed failed (windres?): {e}");
        }
    }
    println!("cargo:rerun-if-changed=src/g13_glyphs.rs");
}

// Texture loading: PNG and JPEG (decoded by the image crate), downscaled at load time.
use super::mesh::Texture;

/// Most bytes a decoder may allocate: refuses absurd image headers (64 Mpx of RGBA).
const MAX_ALLOC: u64 = 256 * 1024 * 1024;

/// (w, h, RGBA8 pixels)
type Rgba = (usize, usize, Vec<u8>);

/// Box-filter down to at most `size` px on the long side. Color is averaged weighted by alpha
/// so cut-out edges don't pick up the (often black) color of transparent pixels.
fn downscale((iw, ih, px): Rgba, size: usize) -> Texture {
    let f = iw.max(ih).div_ceil(size).max(1);
    let (w, h) = ((iw / f).max(1), (ih / f).max(1));
    let n = w * h;
    let (mut r, mut g, mut b, mut a) = (vec![0f32; n], vec![0f32; n], vec![0f32; n], vec![0f32; n]);
    let mut opaque = true;
    for y in 0..h {
        for x in 0..w {
            let (mut sr, mut sg, mut sb, mut sa) = (0.0, 0.0, 0.0, 0.0);
            for yy in y * f..ih.min((y + 1) * f) {
                for xx in x * f..iw.min((x + 1) * f) {
                    let o = (yy * iw + xx) * 4;
                    let al = px[o + 3] as f64 / 255.0;
                    sr += px[o] as f64 * al;
                    sg += px[o + 1] as f64 * al;
                    sb += px[o + 2] as f64 * al;
                    sa += al;
                }
            }
            let i = y * w + x;
            if sa > 0.0 {
                r[i] = (sr / sa / 255.0) as f32;
                g[i] = (sg / sa / 255.0) as f32;
                b[i] = (sb / sa / 255.0) as f32;
            }
            a[i] = (sa / (f * f) as f64) as f32;
            if (a[i] as f64) < 0.99 {
                opaque = false;
            }
        }
    }
    Texture {
        w,
        h,
        r,
        g,
        b,
        a,
        alpha: !opaque,
    }
}

/// Load and downscale an image file.
pub fn load(path: &str, size: usize) -> Result<Texture, String> {
    let bad = |e: image::ImageError| format!("{e} ({path})");
    let mut reader = image::ImageReader::open(path)
        .map_err(|_| format!("cannot open {path}"))?
        .with_guessed_format()
        .map_err(|_| format!("cannot open {path}"))?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let img = reader.decode().map_err(bad)?.into_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    Ok(downscale((w, h, img.into_raw()), size))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_bad_files() {
        let dir = std::env::temp_dir().join(format!("tbloom-img-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, data) in [
            ("junk.png", b"not an image at all".to_vec()),
            ("cut.png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0".to_vec()),
        ] {
            let p = dir.join(name);
            std::fs::write(&p, data).unwrap();
            assert!(super::load(p.to_str().unwrap(), 16).is_err(), "{name}");
        }
        assert!(super::load("/nonexistent/x.png", 16).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

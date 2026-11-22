// Texture loading: PNG and JPEG, downscaled at load time.
use super::mesh::Texture;
use std::io::Cursor;

const MAX_PIXELS: usize = 64 * 1024 * 1024; // refuse absurd image headers

/// (w, h, RGBA8 pixels)
type Rgba = (usize, usize, Vec<u8>);

/// Expand 1-4 channel 8-bit pixels (gray, gray+alpha, rgb, rgba) to RGBA.
fn to_rgba(px: &[u8], channels: usize) -> Vec<u8> {
    px.chunks_exact(channels)
        .flat_map(|p| match channels {
            1 => [p[0], p[0], p[0], 255],
            2 => [p[0], p[0], p[0], p[1]],
            3 => [p[0], p[1], p[2], 255],
            _ => [p[0], p[1], p[2], p[3]],
        })
        .collect()
}

fn decode_png(bytes: &[u8]) -> Result<Rgba, String> {
    let bad = |e: png::DecodingError| format!("bad png: {e}");
    // its default limits cover only its own buffers (rows, metadata); the image size is checked below
    let mut dec = png::Decoder::new(Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(bad)?;
    let (w, h) = (reader.info().width as usize, reader.info().height as usize);
    if w * h > MAX_PIXELS {
        return Err("png too large".into());
    }
    let channels = match reader.output_color_type().0 {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("bad png: palette not expanded".into()),
    };
    let mut buf = vec![0; reader.output_buffer_size().ok_or("png too large")?];
    let info = reader.next_frame(&mut buf).map_err(bad)?;
    buf.truncate(info.buffer_size());
    Ok((w, h, to_rgba(&buf, channels)))
}

fn decode_jpeg(bytes: &[u8]) -> Result<Rgba, String> {
    use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(Cursor::new(bytes), opts);
    dec.decode_headers().map_err(|_| "bad jpeg header")?;
    let info = dec.info().ok_or("bad jpeg header")?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w * h > MAX_PIXELS {
        return Err("jpeg too large".into());
    }
    let px = dec.decode().map_err(|_| "bad jpeg data")?;
    match px.len().checked_div(w * h) {
        Some(c @ 1..=4) if px.len() == c * w * h => Ok((w, h, to_rgba(&px, c))),
        _ => Err("bad jpeg data".into()),
    }
}

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
    let bytes = std::fs::read(path).map_err(|_| format!("cannot open {path}"))?;
    let img = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(&bytes)
    } else if bytes.starts_with(b"\xff\xd8") {
        decode_jpeg(&bytes)
    } else {
        return Err(format!("unknown image format: {path}"));
    };
    Ok(downscale(img.map_err(|e| format!("{e} ({path})"))?, size))
}

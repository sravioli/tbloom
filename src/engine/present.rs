// Framebuffer -> terminal text. Half-block (1x2 px/cell) or sextant (2x3 px/cell, --nerd),
// truecolor or monochrome (--no-color). Frame mode only emits cells that changed.
use super::raster::Framebuffer;
use std::fmt::Write;

/// Sextant glyph per 6-bit mask; bit0=TL bit1=TR bit2=ML bit3=MR bit4=BL bit5=BR.
pub fn sext(mask: usize) -> char {
    match mask {
        0 => ' ',
        63 => '\u{2588}',
        21 => '\u{258C}',
        42 => '\u{2590}',
        _ => {
            // U+1FB00.. skips the masks that already exist as half blocks (21, 42)
            let i = mask - 1 - (mask > 21) as usize - (mask > 42) as usize;
            char::from_u32(0x1FB00 + i as u32).unwrap()
        }
    }
}

const RAMP: &[u8] = b" .:-=+*#%@";
/// Ramp character and its index for luminance l in [0, 1].
pub fn ramp(l: f64) -> (char, usize) {
    let i = (l * RAMP.len() as f64)
        .floor()
        .clamp(0.0, (RAMP.len() - 1) as f64) as usize;
    (RAMP[i] as char, i)
}

const BAYER: [f64; 16] = [
    0., 8., 2., 10., 12., 4., 14., 6., 3., 11., 1., 9., 15., 7., 13., 5.,
];
/// Dithered sextant glyph from 6 luminances (TL, TR, ML, MR, BL, BR) at pixel origin (x0, y0).
pub fn dither(lums: &[f64; 6], x0: usize, y0: usize) -> (char, usize) {
    let mut mask = 0;
    for (k, &l) in lums.iter().enumerate() {
        let (x, y) = (x0 + k % 2, y0 + k / 2);
        if l > (BAYER[(y % 4) * 4 + x % 4] + 0.5) / 16.0 {
            mask |= 1 << k;
        }
    }
    (sext(mask), mask)
}

const HALF: char = '\u{2580}';

// 6 bits per channel: invisible in practice, but neighbors match more often, so fewer cells
// change between frames and fewer color escapes are sent.
fn q(v: f64) -> i64 {
    let v = (v * 63.0 + 0.5).floor().clamp(0.0, 63.0);
    (v * 255.0 / 63.0 + 0.5).floor() as i64
}
fn rgb(r: f64, g: f64, b: f64) -> i64 {
    q(r) * 65536 + q(g) * 256 + q(b)
}
fn lum(r: f64, g: f64, b: f64) -> f64 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Monochrome/ASCII brightness lift: the scene is mostly near-black (chiaroscuro), so a linear
/// ramp would leave it blank. sqrt opens up the shadows; true black stays empty.
pub fn lift(l: f64) -> f64 {
    if l < 0.012 { 0.0 } else { l.sqrt() }
}

pub struct Present {
    cols: usize,
    rows: usize,
    nerd: bool,
    color: bool,
    ascii: bool,
    // last frame's (glyph id, fg) key and bg per cell
    pk: Vec<i64>,
    pb: Vec<i64>,
}

impl Present {
    pub fn new(cols: usize, rows: usize, nerd: bool, color: bool, ascii: bool) -> Self {
        let n = cols * rows;
        // i64::MIN matches no cell, so the first frame draws everything
        Present {
            cols,
            rows,
            nerd,
            color,
            ascii,
            pk: vec![i64::MIN; n],
            pb: vec![i64::MIN; n],
        }
    }
}

const GID_HALF: i64 = 64;
const GID_RAMP: i64 = 65;
/// Returns glyph, fg (24-bit int), bg (24-bit int, -1 = none), glyph id for cell (cx, cy).
/// Glyph ids: 0 space, 1..63 sextant/dither mask, 64 half block, 65+ ramp index.
pub fn cell(st: &Present, fb: &Framebuffer, cx: usize, cy: usize) -> (char, i64, i64, i64) {
    let (w, r, g, b) = (fb.w, &fb.r, &fb.g, &fb.b);
    if !st.nerd {
        let (i, j) = (cy * 2 * w + cx, (cy * 2 + 1) * w + cx);
        if st.ascii || !st.color {
            let (r, g, b) = (
                (r[i] + r[j]) / 2.0,
                (g[i] + g[j]) / 2.0,
                (b[i] + b[j]) / 2.0,
            );
            let l = lum(r, g, b);
            let ll = lift(l);
            let (ch, ci) = ramp(ll);
            if !st.color {
                return (ch, 0, 0, GID_RAMP + ci as i64);
            }
            let mut k = if l > 0.0 { ll / l } else { 0.0 }; // brighten the color to the lifted luminance, keep hue
            let mx = r.max(g).max(b) * k;
            if mx > 1.0 {
                k /= mx;
            }
            return (ch, rgb(r * k, g * k, b * k), -1, GID_RAMP + ci as i64);
        }
        let (top, bot) = (rgb(r[i], g[i], b[i]), rgb(r[j], g[j], b[j]));
        if top == bot {
            return (' ', bot, bot, 0); // background only: no fg escape needed
        }
        return (HALF, top, bot, GID_HALF);
    }
    let (x0, y0) = (cx * 2, cy * 3);
    let px = |k: usize| (y0 + k / 2) * w + x0 + k % 2;
    let ls: [f64; 6] = std::array::from_fn(|k| lum(r[px(k)], g[px(k)], b[px(k)]));
    let mean: f64 = ls.iter().map(|l| l / 6.0).sum();
    if !st.color {
        let (gl, mask) = dither(&ls.map(lift), x0, y0);
        return (gl, 0, 0, mask as i64);
    }
    let (mut mask, mut fg, mut nf, mut bg, mut nb) = (0, [0.0; 3], 0.0, [0.0; 3], 0.0);
    for (k, &l) in ls.iter().enumerate() {
        let i = px(k);
        if l > mean + 1e-6 {
            mask |= 1 << k;
            fg = [fg[0] + r[i], fg[1] + g[i], fg[2] + b[i]];
            nf += 1.0;
        } else {
            bg = [bg[0] + r[i], bg[1] + g[i], bg[2] + b[i]];
            nb += 1.0;
        }
    }
    let bgc = rgb(bg[0] / nb, bg[1] / nb, bg[2] / nb);
    if nf == 0.0 {
        return (' ', bgc, bgc, 0);
    }
    let fgc = rgb(fg[0] / nf, fg[1] / nf, fg[2] / nf);
    if fgc == bgc {
        return (' ', bgc, bgc, 0);
    }
    (sext(mask), fgc, bgc, mask as i64)
}

/// One escape setting the 24-bit foreground and/or background color.
fn sgr(out: &mut String, fg: Option<i64>, bg: Option<i64>) {
    out.push_str("\x1b[");
    let mut sep = "";
    for (code, c) in [(38, fg), (48, bg)] {
        if let Some(c) = c {
            let _ = write!(
                out,
                "{sep}{code};2;{};{};{}",
                c >> 16,
                (c >> 8) & 255,
                c & 255
            );
            sep = ";";
        }
    }
    out.push('m');
}

/// lines=true: full frame as newline-separated rows (snapshot). Otherwise diff vs last frame
/// using absolute cursor moves.
pub fn frame(st: &mut Present, fb: &Framebuffer, lines: bool) -> String {
    let mut out = String::new();
    let cols = st.cols;
    let (mut lastf, mut lastb, mut cur) = (-2i64, -2i64, -1i64);
    for cy in 0..st.rows {
        for cx in 0..cols {
            let (g, f, b, gid) = cell(st, fb, cx, cy);
            let (k, key) = (cy * cols + cx, gid * 16777216 + f);
            if lines || st.pk[k] != key || st.pb[k] != b {
                st.pk[k] = key;
                st.pb[k] = b;
                if !lines && cur != k as i64 {
                    let _ = write!(out, "\x1b[{};{}H", cy + 1, cx + 1);
                }
                if st.color {
                    let fg = (g != ' ' && f != lastf).then_some(f);
                    let bg = (b >= 0 && b != lastb).then_some(b); // -1: no bg
                    if fg.is_some() || bg.is_some() {
                        sgr(&mut out, fg, bg);
                    }
                    lastf = fg.unwrap_or(lastf);
                    lastb = bg.unwrap_or(lastb);
                }
                out.push(g);
                cur = k as i64 + 1;
            }
        }
        if lines {
            out.push_str(if st.color { "\x1b[0m\n" } else { "\n" });
            (lastf, lastb) = (-2, -2);
        }
        cur = -1; // don't trust the cursor across the right margin
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sextant_glyphs() {
        assert!(sext(0) == ' ' && sext(63) == '█' && sext(21) == '▌' && sext(42) == '▐');
        assert_eq!(sext(1), '\u{1FB00}');
        assert_eq!(sext(62), '\u{1FB3B}');
        let mut seen = std::collections::HashSet::new();
        for m in 0..64 {
            assert!(seen.insert(sext(m)), "duplicate glyph {m}");
        }
    }

    #[test]
    fn monochrome_mappings() {
        assert!(ramp(0.0).0 == ' ' && ramp(1.0).0 == '@' && ramp(0.5).0 == '+');
        assert!(
            lift(0.0) == 0.0 && lift(0.25) == 0.5,
            "ascii lift opens up shadows"
        );
        assert_eq!(dither(&[0.0; 6], 0, 0).0, ' ');
        assert_eq!(dither(&[1.0; 6], 0, 0).0, '█');
    }

    #[test]
    fn ascii_cell() {
        // plain-text cell: ASCII ramp glyph, averaged fg color, no background
        let st = Present::new(1, 1, false, true, true);
        let mut fb = Framebuffer::new(1, 2, 1.0, 1.0, 1);
        for c in [&mut fb.r, &mut fb.g, &mut fb.b] {
            c.fill(1.0);
        }
        let (g, f, b, _) = cell(&st, &fb, 0, 0);
        assert!(
            g == '@' && f == 0xFFFFFF && b == -1,
            "ascii cell {g} {f:x} {b}"
        );
    }
}

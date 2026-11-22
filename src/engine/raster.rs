// Software rasterizer: shadow-map pass from a directional light, then a visibility pass that
// stores only (triangle, barycentrics) per pixel, then one shading pass per visible pixel
// (ambient + wrapped Lambert + petal translucency + Blinn spec, bilinear PCF shadows,
// cool rim light, color grade) + light glow. Optional supersampling (ss x ss per pixel).
use super::math3d::{DMat3, DVec3, dvec3};
use super::mesh::{Mesh, Texture};

/// Camera rests at (0, 0, CAMZ) looking down -z, +y up; render() takes an offset.
pub const CAMZ: f64 = 5.0;
/// Shadow map resolution, half-extent in world units (covers the stems).
const SM: usize = 640;
const SE: f64 = 2.6;
const BIAS: f64 = 0.035;
// Chiaroscuro: almost no fill light, a strong warm key light, black surroundings.
const AMB: f64 = 0.07;
const KD: f64 = 1.9;
const WARM: [f64; 3] = [1.0, 0.86, 0.66]; // key light color (candle-ish)
const BG: [f64; 3] = [0.0, 0.0, 0.0];
const TRANS: f64 = 0.4; // light seen through thin petals lit from behind
const RIM: [f64; 3] = [0.34, 0.38, 0.48]; // cool back light that outlines the silhouette
const CUT: f64 = 0.5; // alpha below this is a hole (petal and leaf cards)
const EPS: f64 = -1e-9;

/// Light direction in camera space (= world space, camera is axis aligned): top-right, toward viewer.
fn light() -> DVec3 {
    dvec3(1.0, 1.0, 0.9).normalize_or(DVec3::Z)
}

pub struct Item {
    pub mesh: Mesh,
    /// follows the pose (false: fixed in camera space, like the backdrop)
    pub rotate: bool,
    pub caster: bool,
}

pub struct Framebuffer {
    pub w: usize,
    pub h: usize,
    pub ss: usize,
    pub iw: usize,
    pub ih: usize,
    pub pw: f64,
    pub ph: f64,
    pub f: f64,
    pub cx: f64,
    pub cy: f64,
    pub z: Vec<f64>,
    b0: Vec<f64>,
    b1: Vec<f64>,
    tk: Vec<u32>,
    tm: Vec<u32>,
    pub r: Vec<f64>,
    pub g: Vec<f64>,
    pub b: Vec<f64>,
    hr: Vec<f64>,
    hg: Vec<f64>,
    hb: Vec<f64>,
    sm: Vec<f64>,
    glow: Vec<f64>,
    /// pose the shadow map was drawn for (None: redraw it)
    pub sm_rot: Option<DMat3>,
    /// Closed surfaces are wound outward; a front face then has this sign of screen-space area
    /// (screen y points down). 0 disables back-face skipping (used to verify it).
    pub cull: f64,
}

impl Framebuffer {
    /// w x h output pixels, each pw x ph in cell-width units (half-block 1x1, sextant 0.5x0.667).
    /// ss > 1 renders ss x ss samples per output pixel and averages them (anti-aliasing).
    pub fn new(w: usize, h: usize, pw: f64, ph: f64, ss: usize) -> Self {
        let (iw, ih, ipw, iph) = (w * ss, h * ss, pw / ss as f64, ph / ss as f64); // internal sample grid
        let (n, ww, hh) = (iw * ih, w as f64 * pw, h as f64 * ph);
        Framebuffer {
            w,
            h,
            ss,
            iw,
            ih,
            pw: ipw,
            ph: iph,
            f: 0.5 * ww.min(hh) * CAMZ / 1.75,
            cx: iw as f64 / 2.0,
            cy: ih as f64 / 2.0,
            z: vec![0.0; n],
            b0: vec![0.0; n],
            b1: vec![0.0; n],
            tk: vec![0; n],
            tm: vec![0; n],
            r: vec![0.0; w * h],
            g: vec![0.0; w * h],
            b: vec![0.0; w * h],
            hr: vec![0.0; n],
            hg: vec![0.0; n],
            hb: vec![0.0; n],
            sm: vec![0.0; SM * SM],
            glow: glow(iw, ih, ipw, iph),
            sm_rot: None,
            cull: -1.0,
        }
    }
}

/// Largest supersampling factor (1..3) whose sample grid stays within a memory/time budget.
pub fn ss_for(w: usize, h: usize) -> usize {
    ((1.2e6 / (w * h) as f64).sqrt().floor() as usize).clamp(1, 3)
}

// Soft shoulder instead of hard clipping, so highlights near the light keep their hue.
fn tone(c: f64) -> f64 {
    if c <= 0.7 {
        c
    } else {
        0.7 + 0.3 * (1.0 - (-(c - 0.7) / 0.3).exp())
    }
}

fn transform(fb: &Framebuffer, m: &mut Mesh, rot: &DMat3, cam: [f64; 3]) {
    let l = light();
    let u = dvec3(0.0, 1.0, 0.0).cross(l).normalize_or(DVec3::Z);
    let vv = l.cross(u);
    let rl = dvec3(-1.0, 0.35, -0.6).normalize_or(DVec3::Z); // rim light direction: behind-left
    let (lx, ly, lz) = (l.x, l.y, l.z);
    let (ex, ey, ez) = (cam[0], cam[1], CAMZ + cam[2]); // eye position
    let (f, cx, cy, pw, ph) = (fb.f, fb.cx, fb.cy, fb.pw, fb.ph);
    let [r1, r2, r3, r4, r5, r6, r7, r8, r9] = rot.transpose().to_cols_array(); // rows
    let tscale = SM as f64 / (2.0 * SE);
    let t = &mut m.t;
    for i in 0..m.n {
        let (x0, y0, z0) = (m.px[i], m.py[i], m.pz[i]);
        let x = r1 * x0 + r2 * y0 + r3 * z0;
        let y = r4 * x0 + r5 * y0 + r6 * z0;
        let z = r7 * x0 + r8 * y0 + r9 * z0;
        let (a, b, c) = (m.nx[i], m.ny[i], m.nz[i]);
        let (mut nx, mut ny, mut nz) = (
            r1 * a + r2 * b + r3 * c,
            r4 * a + r5 * b + r6 * c,
            r7 * a + r8 * b + r9 * c,
        );
        let (mut vx, mut vy, mut vz) = (ex - x, ey - y, ez - z);
        let vl = 1.0 / (vx * vx + vy * vy + vz * vz).sqrt();
        (vx, vy, vz) = (vx * vl, vy * vl, vz * vl);
        // two-sided lighting: thin petals show whichever side faces the camera
        if nx * vx + ny * vy + nz * vz < 0.0 {
            (nx, ny, nz) = (-nx, -ny, -nz);
        }
        // pool of light: falls off with distance from the light along its axis
        // (reaches past the bouquet's middle; only the far side sinks into shadow)
        let mut fall = ((x * lx + y * ly + z * lz + 1.7) / 2.5).clamp(0.0, 1.0);
        fall *= fall.sqrt();
        let ndl = nx * lx + ny * ly + nz * lz;
        // wrapped diffuse on the lit side; on the far side thin petals glow with transmitted light
        t.d[i] = ((ndl + 0.25) / 1.25).max(-ndl * TRANS).max(0.0) * fall;
        // rim: strongest where the surface turns away from the viewer and faces the back light
        let fres = 1.0 - (nx * vx + ny * vy + nz * vz).max(0.0);
        t.rim[i] = fres * fres * fres * (0.3 + 0.7 * (nx * rl.x + ny * rl.y + nz * rl.z).max(0.0));
        let (hx, hy, hz) = (lx + vx, ly + vy, lz + vz);
        let hl = 1.0 / (hx * hx + hy * hy + hz * hz).sqrt();
        let mut nh = ((nx * hx + ny * hy + nz * hz) * hl).max(0.0);
        for _ in 0..5 {
            nh *= nh; // ^32
        }
        t.s[i] = m.sp[i] * nh;
        let (su, sv) = (x * u.x + y * u.y + z * u.z, x * vv.x + y * vv.y + z * vv.z);
        t.sd[i] = x * lx + y * ly + z * lz + 10.0; // +10: always > 0 (cleared map)
        t.tu[i] = (su + SE) * tscale;
        t.tv[i] = (SE - sv) * tscale;
        let zc = ez - z;
        let iz = if zc > 0.1 { 1.0 / zc } else { -1.0 };
        t.iz[i] = iz;
        t.sx[i] = cx + (x - ex) * f * iz / pw;
        t.sy[i] = cy - (y - ey) * f * iz / ph;
    }
}

/// Clip the row span [lo, hi] to where an edge weight w(px) = ws + a * (px - x0) stays >= EPS.
fn clip(ws: f64, a: f64, x0: f64, mut lo: f64, mut hi: f64) -> (f64, f64) {
    if a > 0.0 {
        lo = lo.max((x0 + (EPS - ws) / a).ceil());
    } else if a < 0.0 {
        hi = hi.min((x0 + (EPS - ws) / a).floor());
    } else if ws < EPS {
        hi = lo - 1.0;
    }
    (lo, hi)
}

/// Index of the texel at (u, v): wraps, and flips v (texture v runs up, image rows run down).
fn texel(tex: &Texture, u: f64, v: f64) -> usize {
    // u - floor(u) rounds to 1.0 for a tiny negative u, hence the clamps
    let x = (((u - u.floor()) * tex.w as f64).floor() as usize).min(tex.w - 1);
    let y = (((1.0 - (v - v.floor())) * tex.h as f64).floor() as usize).min(tex.h - 1);
    y * tex.w + x
}

/// Signed area (x2) of triangle p in screen space; its sign tells front from back faces.
fn area(p: &[f64; 6]) -> f64 {
    let [x0, y0, x1, y1, x2, y2] = *p;
    (x1 - x0) * (y2 - y0) - (y1 - y0) * (x2 - x0)
}

/// Back face of a closed surface: hidden behind its front, so it can be skipped.
fn culled(closed: bool, p: &[f64; 6], cull: f64) -> bool {
    closed && area(p) * cull < 0.0
}

/// (x, y) of corners v, interleaved.
fn corners(x: &[f64], y: &[f64], v: [usize; 3]) -> [f64; 6] {
    [x[v[0]], y[v[0]], x[v[1]], y[v[1]], x[v[2]], y[v[2]]]
}

/// Whether the alpha-cut texture is solid at barycentrics (w0, w1) of a triangle with uvs uv.
fn solid(tex: &Texture, uv: &[f64; 6], w0: f64, w1: f64) -> bool {
    let [u0, v0, u1, v1, u2, v2] = *uv;
    let w2 = 1.0 - w0 - w1;
    tex.a[texel(
        tex,
        w0 * u0 + w1 * u1 + w2 * u2,
        w0 * v0 + w1 * v1 + w2 * v2,
    )] as f64
        >= CUT
}

/// Edge-function setup of one triangle, clipped to a w x h pixel grid.
struct Tri {
    p: [f64; 6],
    ia: f64,
    /// d(w0)/dx, d(w1)/dx
    a0: f64,
    a1: f64,
    minx: f64,
    maxx: f64,
    rows: std::ops::RangeInclusive<usize>,
}

impl Tri {
    /// None when the triangle has no area or misses the grid.
    fn new(p: [f64; 6], w: usize, h: usize) -> Option<Tri> {
        let [x0, y0, x1, y1, x2, y2] = p;
        let a = area(&p);
        let (minx, maxx) = (
            x0.min(x1).min(x2).floor().max(0.0),
            x0.max(x1).max(x2).floor().min(w as f64 - 1.0),
        );
        let (miny, maxy) = (
            y0.min(y1).min(y2).floor().max(0.0),
            y0.max(y1).max(y2).floor().min(h as f64 - 1.0),
        );
        if a == 0.0 || minx > maxx || miny > maxy {
            return None;
        }
        let ia = 1.0 / a;
        Some(Tri {
            p,
            ia,
            a0: -(y2 - y1) * ia,
            a1: -(y0 - y2) * ia,
            minx,
            maxx,
            rows: miny as usize..=maxy as usize,
        })
    }

    /// Pixels [lo, hi] of row py inside the triangle and the barycentrics (w0, w1) at lo;
    /// each pixel to the right adds (a0, a1). Solving the three edge inequalities per row
    /// replaces a per-pixel inside test.
    fn span(&self, py: usize) -> Option<(usize, usize, f64, f64)> {
        let [x0, y0, x1, y1, x2, y2] = self.p;
        let (a0, a1, minx) = (self.a0, self.a1, self.minx);
        let (y, x) = (py as f64 + 0.5, minx + 0.5);
        let w0 = ((x2 - x1) * (y - y1) - (y2 - y1) * (x - x1)) * self.ia;
        let w1 = ((x0 - x2) * (y - y2) - (y0 - y2) * (x - x2)) * self.ia;
        let (lo, hi) = clip(w0, a0, minx, minx, self.maxx);
        let (lo, hi) = clip(w1, a1, minx, lo, hi);
        let (lo, hi) = clip(1.0 - w0 - w1, -a0 - a1, minx, lo, hi);
        (lo <= hi).then_some((
            lo as usize,
            hi as usize,
            w0 + a0 * (lo - minx),
            w1 + a1 * (lo - minx),
        ))
    }
}

/// Write light depths d of triangle p (shadow-map texel coordinates) into the map.
/// tex: alpha-cut texture and the triangle's uvs, so holes cast no shadow.
fn shadow_tri(
    sm: &mut [f64],
    p: [f64; 6],
    [d0, d1, d2]: [f64; 3],
    tex: Option<(&Texture, [f64; 6])>,
) {
    let Some(tri) = Tri::new(p, SM, SM) else {
        return;
    };
    let (e0, e1) = (d0 - d2, d1 - d2);
    let de = tri.a0 * e0 + tri.a1 * e1; // d(depth)/dx
    for py in tri.rows.clone() {
        let Some((lo, hi, mut w0, mut w1)) = tri.span(py) else {
            continue;
        };
        let row = py * SM;
        if let Some((tex, uv)) = tex {
            for s in &mut sm[row + lo..=row + hi] {
                let d = d2 + w0 * e0 + w1 * e1;
                if d > *s && solid(tex, &uv, w0, w1) {
                    *s = d;
                }
                w0 += tri.a0;
                w1 += tri.a1;
            }
        } else {
            let mut d = d2 + w0 * e0 + w1 * e1;
            for s in &mut sm[row + lo..=row + hi] {
                if d > *s {
                    *s = d;
                }
                d += de;
            }
        }
    }
}

/// Visibility only: nearest triangle and its barycentrics per pixel; shading happens once later.
fn camera_tris(fb: &mut Framebuffer, m: &Mesh, mi: u32) {
    let t = &m.t;
    let tex = m.tex.as_deref().filter(|t| t.alpha); // cut-out holes where the texture is transparent
    for k in 0..m.nt {
        let v = m.corners(k);
        let [z0, z1, z2] = v.map(|i| t.iz[i]);
        if !(z0 > 0.0 && z1 > 0.0 && z2 > 0.0) {
            continue;
        }
        let p = corners(&t.sx, &t.sy, v);
        if culled(m.cl[k], &p, fb.cull) {
            continue;
        }
        let Some(tri) = Tri::new(p, fb.iw, fb.ih) else {
            continue;
        };
        let uv = corners(&m.mu, &m.mv, v);
        let (e0, e1) = (z0 - z2, z1 - z2);
        for py in tri.rows.clone() {
            let Some((lo, hi, mut w0, mut w1)) = tri.span(py) else {
                continue;
            };
            let row = py * fb.iw;
            for i in row + lo..=row + hi {
                let d = z2 + w0 * e0 + w1 * e1;
                if d > fb.z[i] && tex.is_none_or(|tex| solid(tex, &uv, w0, w1)) {
                    fb.z[i] = d;
                    fb.b0[i] = w0;
                    fb.b1[i] = w1;
                    fb.tk[i] = k as u32;
                    fb.tm[i] = mi;
                }
                w0 += tri.a0;
                w1 += tri.a1;
            }
        }
    }
}

/// Fraction of light reaching a point at shadow-map texel (i, j) with light depth d.
fn tap(sm: &[f64], i: i64, j: i64, d: f64) -> f64 {
    if i < 0 || i >= SM as i64 || j < 0 || j >= SM as i64 {
        return 1.0;
    }
    if d < sm[j as usize * SM + i as usize] - BIAS {
        0.0
    } else {
        1.0
    }
}

/// Bilinear PCF: 4 taps blended by sub-texel position (smooth edges, cheaper than 3x3).
fn lit(sm: &[f64], tu: f64, tv: f64, d: f64) -> f64 {
    let (fu, fv) = (tu - 0.5, tv - 0.5);
    let (i, j) = (fu.floor(), fv.floor());
    let (ax, ay) = (fu - i, fv - j);
    let (i, j) = (i as i64, j as i64);
    let top = tap(sm, i, j, d) * (1.0 - ax) + tap(sm, i + 1, j, d) * ax;
    let bot = tap(sm, i, j + 1, d) * (1.0 - ax) + tap(sm, i + 1, j + 1, d) * ax;
    top * (1.0 - ay) + bot * ay
}

/// Glow of the light source in the top-right corner; depends only on size, so built once.
fn glow(w: usize, h: usize, pw: f64, ph: f64) -> Vec<f64> {
    let (ww, hh) = (w as f64 * pw, h as f64 * ph);
    let rad = 0.32 * ww.min(hh);
    let mut g = vec![0.0; w * h];
    for py in 0..h {
        for px in 0..w {
            let (dx, dy) = (
                (ww - (px as f64 + 0.5) * pw) / rad,
                (py as f64 + 0.5) * ph / rad,
            );
            let q = dx * dx + dy * dy;
            g[py * w + px] = 0.18 * (-q * 1.5).exp() + 0.45 * (-q * 14.0).exp(); // dim haze around a small bright source
        }
    }
    g
}

/// Shadow map of the casters as the light sees them. It depends only on the pose (light and
/// casters), not on zoom, so it is redrawn only when the pose changes.
fn shadow_map(fb: &mut Framebuffer, items: &[Item], rot: &DMat3) {
    if fb.sm_rot == Some(*rot) {
        return;
    }
    fb.sm_rot = Some(*rot);
    fb.sm.fill(0.0);
    for it in items.iter().filter(|it| it.caster) {
        let (m, t) = (&it.mesh, &it.mesh.t);
        let tex = m.tex.as_deref().filter(|t| t.alpha);
        for k in 0..m.nt {
            let v = m.corners(k);
            let p = corners(&t.tu, &t.tv, v);
            // the light sees closed surfaces from one side too: their far faces never set the depth
            if !culled(m.cl[k], &p, fb.cull) {
                shadow_tri(
                    &mut fb.sm,
                    p,
                    v.map(|i| t.sd[i]),
                    tex.map(|tx| (tx, corners(&m.mu, &m.mv, v))),
                );
            }
        }
    }
}

/// Light every sample the camera pass found (and the background) into the hr/hg/hb grid.
fn shade(fb: &mut Framebuffer, items: &[Item]) {
    for i in 0..fb.iw * fb.ih {
        let (mut r, mut gg, mut b);
        if fb.z[i] > 0.0 {
            let m = &items[fb.tm[i] as usize].mesh;
            let t = &m.t;
            let [a, bb, c] = m.corners(fb.tk[i] as usize);
            let (w0, w1) = (fb.b0[i], fb.b1[i]);
            let w2 = 1.0 - w0 - w1;
            let ip = |q: &[f64]| w0 * q[a] + w1 * q[bb] + w2 * q[c];
            let l = lit(&fb.sm, ip(&t.tu), ip(&t.tv), ip(&t.sd));
            let kd = KD * ip(&t.d) * l;
            let s = ip(&t.s) * l;
            let rim = 0.55 * ip(&t.rim);
            let (mut ar, mut ag, mut ab) = (ip(&m.cr), ip(&m.cg), ip(&m.cb));
            if let Some(tex) = &m.tex {
                // texture color times vertex (material) color
                let ti = texel(tex, ip(&m.mu), ip(&m.mv));
                ar *= tex.r[ti] as f64;
                ag *= tex.g[ti] as f64;
                ab *= tex.b[ti] as f64;
            }
            r = ar * (AMB + kd * WARM[0]) + s * WARM[0] + (ar + 0.15) * rim * RIM[0];
            gg = ag * (AMB + kd * WARM[1]) + s * WARM[1] + (ag + 0.15) * rim * RIM[1];
            b = ab * (AMB + kd * WARM[2]) + s * WARM[2] + (ab + 0.15) * rim * RIM[2];
            // grade: a little more saturation, shadows pushed cool, light stays warm
            let l = 0.2126 * r + 0.7152 * gg + 0.0722 * b;
            let cool = (1.0 - l * 4.0).max(0.0);
            r = ((l + (r - l) * 1.15) * (1.0 - 0.1 * cool)).max(0.0);
            gg = (l + (gg - l) * 1.15).max(0.0);
            b = ((l + (b - l) * 1.15) * (1.0 + 0.25 * cool)).max(0.0);
        } else {
            (r, gg, b) = (BG[0], BG[1], BG[2]);
        }
        let e = fb.glow[i];
        fb.hr[i] = tone(r + e);
        fb.hg[i] = tone(gg + e * 0.93);
        fb.hb[i] = tone(b + e * 0.75);
    }
}

/// Box-filter the sample grid down to output pixels (a copy without supersampling).
fn downsample(fb: &mut Framebuffer) {
    let ss = fb.ss;
    if ss == 1 {
        fb.r.copy_from_slice(&fb.hr);
        fb.g.copy_from_slice(&fb.hg);
        fb.b.copy_from_slice(&fb.hb);
        return;
    }
    let (iw, w, inv) = (fb.iw, fb.w, 1.0 / (ss * ss) as f64);
    for y in 0..fb.h {
        for x in 0..w {
            let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
            for sy in 0..ss {
                let row = (y * ss + sy) * iw + x * ss;
                for sx in 0..ss {
                    r += fb.hr[row + sx];
                    g += fb.hg[row + sx];
                    b += fb.hb[row + sx];
                }
            }
            let o = y * w + x;
            fb.r[o] = r * inv;
            fb.g[o] = g * inv;
            fb.b[o] = b * inv;
        }
    }
}

/// rot: pose of the items with rotate set; cam: eye offset from the rest position.
pub fn render(fb: &mut Framebuffer, items: &mut [Item], rot: &DMat3, cam: [f64; 3]) {
    fb.z.fill(0.0);
    for it in items.iter_mut() {
        transform(
            fb,
            &mut it.mesh,
            if it.rotate { rot } else { &DMat3::IDENTITY },
            cam,
        );
    }
    shadow_map(fb, items, rot);
    for (mi, it) in items.iter().enumerate() {
        camera_tris(fb, &it.mesh, mi as u32);
    }
    shade(fb, items);
    downsample(fb);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::math3d::pose;
    use crate::engine::mesh::Builder;
    use std::rc::Rc;

    #[test]
    fn shadow_coverage_matches_area() {
        let mut sm = vec![0.0; SM * SM];
        shadow_tri(
            &mut sm,
            [10.0, 10.0, 110.0, 10.0, 10.0, 110.0],
            [1.0; 3],
            None,
        );
        let cnt = sm.iter().filter(|&&d| d > 0.5).count() as i64;
        assert!((cnt - 5000).abs() < 150, "coverage {cnt}");
    }

    #[test]
    fn span_rasterizer_matches_brute_force() {
        // covers exactly the pixel centers a brute-force inside test accepts
        let mut r = crate::scene::Rng(99.0);
        let mut miss = 0;
        for _ in 0..200 {
            let p: [f64; 6] = std::array::from_fn(|_| r.rnd(0., 60.));
            let [x0, y0, x1, y1, x2, y2] = p;
            let mut m = vec![0.0; SM * SM];
            shadow_tri(&mut m, p, [1.0; 3], None);
            let area = (x1 - x0) * (y2 - y0) - (y1 - y0) * (x2 - x0);
            for py in 0..64 {
                for px in 0..64 {
                    let (x, y) = (px as f64 + 0.5, py as f64 + 0.5);
                    let w0 = ((x2 - x1) * (y - y1) - (y2 - y1) * (x - x1)) / area;
                    let w1 = ((x0 - x2) * (y - y2) - (y0 - y2) * (x - x2)) / area;
                    let inside = area != 0.0 && w0 >= 1e-7 && w1 >= 1e-7 && 1.0 - w0 - w1 >= 1e-7;
                    let edge = area != 0.0 && w0.min(w1).min(1.0 - w0 - w1) > -1e-7 && !inside;
                    if !edge && inside != (m[py * SM + px] > 0.5) {
                        miss += 1;
                    }
                }
            }
        }
        assert_eq!(miss, 0, "span coverage mismatches");
    }

    #[test]
    fn culling_changes_no_pixel() {
        // back-face skipping on closed surfaces must not change a single pixel (both passes)
        let mut scene = crate::scene::bouquet::build();
        let mut fb = Framebuffer::new(80, 80, 1.0, 1.0, 1);
        for ang in [[-0.079, 0.756, 0.393], [2.2, 0.35, 0.0], [0.5, -1.2, 0.3]] {
            let rot = pose(ang);
            (fb.cull, fb.sm_rot) = (0.0, None);
            render(&mut fb, &mut scene.items, &rot, [0.0; 3]);
            let reference: Vec<f64> = (0..fb.w * fb.h)
                .map(|i| fb.r[i] + fb.g[i] * 7.0 + fb.b[i] * 13.0)
                .collect();
            (fb.cull, fb.sm_rot) = (-1.0, None);
            render(&mut fb, &mut scene.items, &rot, [0.0; 3]);
            for (i, rf) in reference.iter().enumerate() {
                assert!(
                    (rf - (fb.r[i] + fb.g[i] * 7.0 + fb.b[i] * 13.0)).abs() < 1e-9,
                    "culling changed pixel {i}"
                );
            }
        }
    }

    fn quad(b: &mut Builder, cx: f64, cy: f64, cz: f64, s: f64) {
        let v = [
            b.v([cx - s, cy - s, cz, 1.0, 1.0, 1.0, 0.0], [0.0, 0.0]),
            b.v([cx + s, cy - s, cz, 1.0, 1.0, 1.0, 0.0], [1.0, 0.0]),
            b.v([cx + s, cy + s, cz, 1.0, 1.0, 1.0, 0.0], [1.0, 1.0]),
            b.v([cx - s, cy + s, cz, 1.0, 1.0, 1.0, 0.0], [0.0, 1.0]),
        ];
        b.t(v[0], v[1], v[2]);
        b.t(v[0], v[2], v[3]);
    }

    #[test]
    fn alpha_cutout() {
        // a square whose texture is transparent on its left half covers about half its pixels
        let mut b = Builder::new();
        quad(&mut b, 0.0, 0.0, 0.0, 1.0);
        let mut items = vec![Item {
            mesh: b.finish(),
            rotate: false,
            caster: true,
        }];
        let mut fb = Framebuffer::new(60, 60, 1.0, 1.0, 1);
        let covered = |fb: &mut Framebuffer, items: &mut Vec<Item>| {
            render(fb, items, &DMat3::IDENTITY, [0.0; 3]);
            fb.z.iter().filter(|&&z| z > 0.0).count()
        };
        let full = covered(&mut fb, &mut items);
        items[0].mesh.tex = Some(Rc::new(Texture {
            w: 2,
            h: 1,
            alpha: true,
            r: vec![1.0; 2],
            g: vec![1.0; 2],
            b: vec![1.0; 2],
            a: vec![0.0, 1.0],
        }));
        fb.sm_rot = None;
        let half = covered(&mut fb, &mut items);
        assert!(
            full > 100 && (half as f64 / full as f64 - 0.5).abs() < 0.05,
            "cutout covers {half} of {full}"
        );
    }

    #[test]
    fn shadow_falls_behind_occluder() {
        // a quad between light and backdrop darkens the backdrop behind it, not beside it
        let (mut occ, mut back) = (Builder::new(), Builder::new());
        quad(&mut occ, 0.0, 0.0, 0.0, 0.3);
        quad(&mut back, 0.0, 0.0, -1.0, 8.0);
        let mut items = vec![
            Item {
                mesh: occ.finish(),
                rotate: false,
                caster: true,
            },
            Item {
                mesh: back.finish(),
                rotate: false,
                caster: false,
            },
        ];
        let mut fb = Framebuffer::new(80, 80, 1.0, 1.0, 1);
        render(&mut fb, &mut items, &DMat3::IDENTITY, [0.0; 3]);
        let px = |wx: f64, wy: f64, wz: f64| {
            let iz = 1.0 / (CAMZ - wz);
            (fb.cy - wy * fb.f * iz).floor() as usize * fb.w
                + (fb.cx + wx * fb.f * iz).floor() as usize
        };
        let l = light();
        let t = 1.0 / l.z; // backdrop point the occluder center projects onto along the light
        let (shadowed, litp) = (px(-l.x * t, -l.y * t, -1.0), px(1.2, -1.2, -1.0));
        assert!(
            fb.r[shadowed] < fb.r[litp] - 0.2,
            "shadow {} vs lit {}",
            fb.r[shadowed],
            fb.r[litp]
        );
    }
}

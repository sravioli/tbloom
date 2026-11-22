// Florist wrapping: overlapping sheets of cream tissue inside, kraft outside, folded together
// around the flowers. They share one paper surface that clears everything inside it (so
// nothing pokes through) and each lies a paper thickness over the ones beneath, so overlaps
// show as edges, never gaps. The surface is a fan of flat facets between irregular creases
// running up from the tie, softly wrinkled and gathered into fine folds near the tie, where
// it is cinched onto the stems; below it, it carries on as a short bunched sleeve.
use super::Rng;
use crate::engine::mesh::{Builder, Mesh};
use std::f64::consts::TAU;

/// Columns around the axis: of the envelope and of every sheet, so sheets nest exactly.
const COLS: usize = 144;
/// Row heights shared by all sheets (so they nest exactly), from the deepest hem up to TOP.
const LEVELS: usize = 40;
/// Height levels of the envelope, from the tie up to TOP (the highest a sheet corner reaches).
const NZ: usize = 64;
const TOP: f64 = 0.5;
const MARGIN: f64 = 0.03; // clearance around the flowers
const SNUG: f64 = 0.008; // clearance at the tie, where the paper is pressed onto the stems
const CLEAR: f64 = 0.03; // the top edge stays this far below foliage hanging over it
const DIP: f64 = 2.0; // the edge rises back by DIP * angle^2 to either side of such a dip
const GATHER: f64 = 0.02; // the cone's lines meet this far below the tie
const THICK: f64 = 0.006; // paper thickness: how far each sheet lies over the one beneath
const SPREAD: f64 = 1.0; // a bulge falls off by SPREAD * angle^2 to either side
const GATHERS: f64 = 0.18; // depth of the gathered folds at the tie, relative to the radius
const WRINKLE: f64 = 0.01;
// Wrapping paper is soft: above the knot each panel billows out between its creases and the
// whole sheet crumples gently (both relative to the radius, both only outward).
const BILLOW: f64 = 0.06;
const CRUMPLE: f64 = 0.04;
/// How far the paper reaches below the tie (at most), and how much it bunches out there.
pub const SKIRT: f64 = 0.2;
const PUFF: f64 = 0.02;
/// Half height of the ribbon band around the tie.
pub const BAND: f64 = 0.03;

pub struct Paper {
    /// innermost first; each overlaps the ones before it
    pub sheets: Vec<Mesh>,
    /// radius of the ribbon band per column (column i at angle i * TAU / band.len())
    pub band: Vec<f64>,
    /// widest radius of the paper below the tie
    pub skirt: f64,
}

fn col_angle(i: usize) -> f64 {
    i as f64 * TAU / COLS as f64
}
fn angdist(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(TAU);
    d.min(TAU - d)
}
fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Radius the paper must stay beyond at column i and height z, spread to the neighboring
/// columns so bulges stay smooth.
struct Envelope {
    z0: f64,
    dz: f64,
    r: Vec<f64>, // COLS x NZ
}

impl Envelope {
    fn new(contents: &[&Mesh], z0: f64) -> Self {
        let dz = (TOP - z0) / (NZ - 1) as f64;
        let mut raw = vec![0.0f64; COLS * NZ];
        for m in contents {
            for k in 0..m.n {
                let (x, y, z) = (m.px[k], m.py[k], m.pz[k]);
                if z < z0 || z >= TOP {
                    continue; // below the tie or above every sheet
                }
                let i = (y.atan2(x).rem_euclid(TAU) / TAU * COLS as f64).round() as usize % COLS;
                let c = &mut raw[i * NZ + ((z - z0) / dz) as usize]; // level at or below z
                *c = c.max(x.hypot(y));
            }
        }
        // The paper only widens upward, along straight lines from where it is gathered just
        // below the tie: each level clears everything below it and, scaled back toward that
        // point, everything wider above it.
        let apex = z0 - GATHER;
        for col in raw.chunks_mut(NZ) {
            for j in 1..NZ {
                col[j] = col[j].max(col[j - 1]);
            }
            let mut slope = 0.0f64; // widest radius per height above the apex, at or above level j
            for j in (0..NZ).rev() {
                let h = z0 + j as f64 * dz - apex;
                slope = slope.max(col[j] / h);
                col[j] = slope * h;
            }
        }
        let r = (0..COLS * NZ)
            .map(|c| {
                let (i, j) = (c / NZ, c % NZ);
                (0..COLS)
                    .map(|i2| {
                        raw[i2 * NZ + j] - SPREAD * angdist(col_angle(i), col_angle(i2)).powi(2)
                    })
                    .fold(0.0, f64::max)
            })
            .collect();
        Envelope { z0, dz, r }
    }

    fn at(&self, i: usize, z: f64) -> f64 {
        let f = ((z - self.z0) / self.dz).clamp(0.0, (NZ - 1) as f64);
        let j = (f as usize).min(NZ - 2);
        let t = f - j as f64;
        self.r[i * NZ + j] * (1.0 - t) + self.r[i * NZ + j + 1] * t
    }
}

/// Highest the top edge may reach per column: below every point of `spill` that lies outside
/// the contents, so that foliage hangs over the edge instead of piercing the paper.
fn cap(env: &Envelope, spill: &Mesh) -> Vec<f64> {
    let mut low = vec![TOP; COLS];
    for k in 0..spill.n {
        let (x, y, z) = (spill.px[k], spill.py[k], spill.pz[k]);
        let i = (y.atan2(x).rem_euclid(TAU) / TAU * COLS as f64).round() as usize % COLS;
        if z > env.z0 && x.hypot(y) >= env.at(i, z) {
            low[i] = low[i].min(z - CLEAR);
        }
    }
    (0..COLS)
        .map(|i| {
            let dip = |i2: usize| low[i2] + DIP * angdist(col_angle(i), col_angle(i2)).powi(2);
            (0..COLS).map(dip).fold(TOP, f64::min).max(env.z0 + 0.3)
        })
        .collect()
}

/// The bare paper surface every sheet lies on: flat facets between irregular creases that run
/// up from the tie, softly wrinkled, gathered into fine folds near the tie, cinched onto the
/// stems there and bunched into a short sleeve below.
struct Surface {
    env: Envelope,
    /// crease columns, ascending
    creases: Vec<usize>,
    /// spacing of the row heights all sheets share, from the deepest hem up to TOP
    dz: f64,
    phase: [f64; 4],
}

impl Surface {
    fn new(env: Envelope, r: &mut Rng) -> Self {
        let mut creases = vec![0];
        while COLS - creases.last().unwrap() > 14 {
            let next = creases.last().unwrap() + (r.rnd(6.0, 12.0) as usize);
            creases.push(next);
        }
        let phase = [
            r.rnd(0.0, TAU),
            r.rnd(0.0, TAU),
            r.rnd(0.0, TAU),
            r.rnd(0.0, TAU),
        ];
        let dz = (TOP - (env.z0 - SKIRT)) / LEVELS as f64;
        Surface {
            env,
            creases,
            dz,
            phase,
        }
    }

    /// The facet around column i: its crease columns (the second may pass COLS, wrapping).
    fn facet(&self, i: usize) -> (usize, usize) {
        let cs = &self.creases;
        match cs.iter().rposition(|&c| c <= i) {
            Some(f) if f + 1 < cs.len() => (cs[f], cs[f + 1]),
            Some(f) => (cs[f], cs[0] + COLS),
            None => unreachable!("the first crease is column 0"),
        }
    }

    /// Radius a facet from column ca to cb needs at height z: everything inside it, one row
    /// ahead (radii only grow from the tie up, so a row clears what lies up to the next one).
    fn need(&self, ca: usize, cb: usize, z: f64) -> f64 {
        let z0 = self.env.z0;
        let margin = SNUG + (MARGIN - SNUG) * ((z - z0) / 0.3).clamp(0.0, 1.0);
        (ca..=cb)
            .map(|i| self.env.at(i % COLS, z + self.dz))
            .fold(0.0, f64::max)
            + margin
    }

    /// Radius of crease column c: a flat facet sags by cos(half its angle) between its
    /// creases, so they stand out by that much.
    fn crease(&self, c: usize, z: f64) -> f64 {
        let (prev, _) = self.facet((c + COLS - 1) % COLS);
        let (_, next) = self.facet(c);
        let half = |n: usize| (n as f64 * TAU / COLS as f64 / 2.0).cos();
        let left =
            self.need(prev, prev + (c + COLS - prev) % COLS, z) / half((c + COLS - prev) % COLS);
        (self.need(c, next, z) / half(next - c)).max(left)
    }

    fn radius(&self, i: usize, z: f64) -> f64 {
        let z0 = self.env.z0;
        if z < z0 {
            // the sleeve: puffs out under the knot and closes back onto the stems at the hem
            let d = (z0 - z) / SKIRT;
            return self.radius(i, z0) + 0.006 + PUFF * (std::f64::consts::PI * d.min(1.0)).sin();
        }
        let (ca, cb) = self.facet(i);
        let (ra, rb) = (self.crease(ca % COLS, z), self.crease(cb % COLS, z));
        let (al, be, a) = (col_angle(ca), col_angle(cb), col_angle(i));
        let flat = ra * rb * (be - al).sin() / (ra * (a - al).sin() + rb * (be - a).sin());
        let p = self.phase;
        // fine folds where the paper is gathered toward the tie (their phase always increases)
        let f = a * 30.0 / TAU + 0.3 * (2.0 * a + p[0]).sin() + 0.12 * (5.0 * a + p[1]).sin();
        let fold = 1.0 - (2.0 * (f - f.floor()) - 1.0).abs();
        let gathered = (1.0 - ((z - z0) / 0.6).clamp(0.0, 1.0)).powi(2);
        let wrinkle =
            0.5 + 0.5 * (7.0 * a + 11.0 * z + p[2]).sin() * (3.0 * a - 17.0 * z + p[3]).sin();
        // softness fades in above the knot, which stays exactly as tied
        let soft = smoothstep((z - z0 - 0.05) / 0.5);
        let across = (i - ca) as f64 / (cb - ca) as f64; // 0 and 1 at the creases
        let billow = (std::f64::consts::PI * across).sin();
        let crumple = 0.5
            + 0.5 * (5.0 * a + 6.0 * z + p[0] + 1.3).sin() * (3.0 * a - 9.0 * z + p[1] + 2.1).sin();
        flat * (1.0 + GATHERS * fold * gathered + soft * (BILLOW * billow + CRUMPLE * crumple))
            + WRINKLE * wrinkle
    }
}

struct Sheet {
    /// columns it covers (may pass COLS, wrapping), its top and bottom edge per column
    cols: std::ops::RangeInclusive<usize>,
    top: Vec<f64>,
    bottom: Vec<f64>,
    layer: usize,
    color: [f64; 3],
}

impl Sheet {
    /// A sheet over `span` columns from column c0, lying `layer` paper thicknesses out; its top
    /// edge rises in straight lines from about `rim` at the sides to a corner somewhere in the
    /// middle (a square sheet wrapped corner-up), kept below `cap`.
    fn new(
        r: &mut Rng,
        z0: f64,
        (c0, span, layer): (usize, usize, usize),
        rim: f64,
        color: [f64; 3],
        cap: &[f64],
    ) -> Self {
        let (peak, height, hem) = (
            span as f64 * r.rnd(0.35, 0.65),
            r.rnd(0.15, 0.32),
            r.rnd(0.0, TAU),
        );
        let top = (0..=span)
            .map(|k| {
                let k = k as f64;
                let side = if k < peak {
                    k / peak
                } else {
                    (span as f64 - k) / (span as f64 - peak)
                };
                (rim + height * side).min(cap[(c0 + k as usize) % COLS])
            })
            .collect();
        let bottom = (0..=span)
            .map(|k| z0 - SKIRT * (0.8 + 0.2 * (0.3 * k as f64 + hem).sin()))
            .collect();
        Sheet {
            cols: c0..=c0 + span,
            top,
            bottom,
            layer,
            color,
        }
    }

    fn mesh(&self, surf: &Surface, r: &mut Rng) -> Mesh {
        let tint = r.rnd(0.93, 1.05);
        let lo = surf.env.z0 - SKIRT;
        let mut b = Builder::new();
        // each column: its bottom edge, the shared row heights in between, its top edge
        let columns: Vec<Vec<(usize, f64)>> = self
            .cols
            .clone()
            .enumerate()
            .map(|(k, c)| {
                let (z_lo, z_hi) = (self.bottom[k], self.top[k]);
                let levels = (0..=LEVELS).map(|j| lo + surf.dz * j as f64);
                let zs = std::iter::once(z_lo)
                    .chain(levels.filter(|&z| z > z_lo + 1e-6 && z < z_hi - 1e-6))
                    .chain([z_hi]);
                zs.map(|z| {
                    let (i, a) = (c % COLS, col_angle(c));
                    let rr = surf.radius(i, z) + self.layer as f64 * THICK;
                    let edge = 1.0 + 0.25 * smoothstep((z - z_hi + 0.06) / 0.06); // light-catching top edge
                    let fiber = 0.95 + 0.05 * (a * 41.0 + z * 37.0).sin();
                    let col = self.color.map(|x| x * tint * fiber * edge);
                    (
                        b.v(
                            [rr * a.cos(), rr * a.sin(), z, col[0], col[1], col[2], 0.08],
                            [0.0, 0.0],
                        ),
                        z,
                    )
                })
                .collect()
            })
            .collect();
        // zip neighboring columns into triangles, always advancing the lower side
        for w in columns.windows(2) {
            let (pa, pb) = (&w[0], &w[1]);
            let (mut i, mut j) = (0, 0);
            while i + 1 < pa.len() || j + 1 < pb.len() {
                if j + 1 >= pb.len() || (i + 1 < pa.len() && pa[i + 1].1 <= pb[j + 1].1) {
                    b.t(pa[i].0, pb[j].0, pa[i + 1].0);
                    i += 1;
                } else {
                    b.t(pa[i].0, pb[j].0, pb[j + 1].0);
                    j += 1;
                }
            }
        }
        b.finish()
    }
}

/// Wrap the contents (meshes in model space, stems along -z) from the tie at height z0 up;
/// `spill` is foliage that hangs out over the paper's edge.
pub fn wrap(contents: &[&Mesh], spill: &Mesh, z0: f64) -> Paper {
    let env = Envelope::new(contents, z0);
    let cap = cap(&env, spill);
    let mut r = Rng(7654321.0);
    let surf = Surface::new(env, &mut r);
    let (tissue, kraft) = ([0.93, 0.88, 0.78], [0.82, 0.63, 0.42]);
    // (first column, columns covered, layer), rim, color: each sheet lies on the ones before it
    let layout = [
        ((10, 84, 0), 0.07, tissue),
        ((86, 76, 1), 0.07, tissue),
        ((30, 60, 2), -0.02, kraft),
        ((80, 60, 3), -0.02, kraft),
        ((128, 50, 4), -0.02, kraft),
    ];
    let sheets: Vec<Sheet> = layout
        .into_iter()
        .map(|(c, rim, col)| Sheet::new(&mut r, z0, c, rim, col, &cap))
        .collect();
    let outermost = (sheets.len() - 1) as f64 * THICK;
    // the ribbon hugs the paper column by column, over its height around the tie
    let band = (0..COLS)
        .map(|i| {
            (0..=6)
                .map(|k| surf.radius(i, z0 - BAND + BAND * k as f64 / 3.0))
                .fold(0.0, f64::max)
                + outermost
                + 0.004
        })
        .collect();
    let skirt = (0..COLS)
        .flat_map(|i| (0..=10).map(move |k| (i, z0 - SKIRT * k as f64 / 10.0)))
        .map(|(i, z)| surf.radius(i, z))
        .fold(0.0, f64::max)
        + outermost;
    Paper {
        sheets: sheets.iter().map(|s| s.mesh(&surf, &mut r)).collect(),
        band,
        skirt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type P3 = [f64; 3];
    fn sub(a: P3, b: P3) -> P3 {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }
    fn cross(a: P3, b: P3) -> P3 {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }
    fn dot(a: P3, b: P3) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
    /// Segment o..o+d crosses triangle abc (Möller-Trumbore).
    fn crosses(o: P3, d: P3, [a, b, c]: [P3; 3]) -> bool {
        let (e1, e2) = (sub(b, a), sub(c, a));
        let h = cross(d, e2);
        let det = dot(e1, h);
        if det.abs() < 1e-12 {
            return false;
        }
        let s = sub(o, a);
        let q = cross(s, e1);
        let (u, v, t) = (dot(s, h) / det, dot(d, q) / det, dot(e2, q) / det);
        u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0 && t < 1.0
    }
    fn points(m: &Mesh) -> Vec<P3> {
        (0..m.n).map(|k| [m.px[k], m.py[k], m.pz[k]]).collect()
    }

    /// Points outside any of the sheets: the segment from the axis to the point crosses one.
    fn outside(sheets: &[&Mesh], pts: &[P3]) -> usize {
        // triangles with the arc of angles they cover, to skip most of them cheaply
        let tris: Vec<([P3; 3], f64, f64)> = sheets
            .iter()
            .flat_map(|m| {
                let p = points(m);
                (0..m.nt).map(move |k| {
                    let t = m.corners(k).map(|i| p[i]);
                    let a: Vec<f64> = t.iter().map(|q| q[1].atan2(q[0])).collect();
                    let d = |x: f64| (x - a[0] + TAU / 2.0).rem_euclid(TAU) - TAU / 2.0; // relative to the first
                    let (lo, hi) = (d(a[1]).min(d(a[2])).min(0.0), d(a[1]).max(d(a[2])).max(0.0));
                    (t, a[0] + lo, hi - lo)
                })
            })
            .collect();
        pts.iter()
            .filter(|&&[x, y, z]| {
                let a = y.atan2(x);
                tris.iter().any(|&(t, lo, span)| {
                    (a - lo).rem_euclid(TAU) <= span && crosses([0.0, 0.0, z], [x, y, 0.0], t)
                })
            })
            .count()
    }

    #[test]
    fn nothing_pokes_through() {
        // items: flowers, spilling foliage, pine cones, ribbon, paper sheets (innermost first), backdrop
        let s = super::super::bouquet::build();
        let contents: Vec<P3> = s.items[..3]
            .iter()
            .flat_map(|it| points(&it.mesh))
            .collect();
        let sheets: Vec<&Mesh> = s.items[4..s.items.len() - 1]
            .iter()
            .map(|it| &it.mesh)
            .collect();
        assert_eq!(sheets.len(), 5);
        assert_eq!(
            outside(&sheets, &contents),
            0,
            "flowers or foliage through the paper"
        );
        for k in 0..sheets.len() - 1 {
            assert_eq!(
                outside(&sheets[k + 1..], &points(sheets[k])),
                0,
                "sheet {k} through the ones over it"
            );
        }
    }
}

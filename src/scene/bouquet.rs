// Procedural autumn bouquet: a sunflower framed by gerberas and roses, carnations, daisies,
// hypericum berries, lily buds, goldenrod filler, foliage, and wheat / eucalyptus / autumn
// leaves spilling out of folded kraft paper tied with twine and a satin bow.
//
// Every rnd() call sits in its own statement, in the order the shape was tuned with: the
// start pose in main is fitted to this exact geometry.
use super::{Rng, Scene, paper};
use crate::engine::math3d::{DVec3, Frame, dvec3, frame};
use crate::engine::mesh::Builder;
use crate::engine::raster::Item;
use std::f64::consts::PI;

fn hash(x: f64) -> f64 {
    let s = (x * 12.9898).sin() * 43758.5453;
    s - s.floor()
}
type C3 = [f64; 3];
fn lerp3(c1: C3, c2: C3, t: f64) -> C3 {
    [
        c1[0] + (c2[0] - c1[0]) * t,
        c1[1] + (c2[1] - c1[1]) * t,
        c1[2] + (c2[2] - c1[2]) * t,
    ]
}
const SPEC: f64 = 0.05;

#[derive(Default)]
struct Pet {
    count: usize,
    phase: f64,
    r0: f64,
    h0: f64,
    len: f64,
    wid: f64,
    tilt: f64,
    cup: f64,
    curl: f64,
    tip: f64,
    base: C3,
    tip_col: C3,
}

/// One petal at angle a around flower frame f.
fn petal_at(b: &mut Builder, r: &mut Rng, f: &Frame, a: f64, o: &Pet) {
    let d = f.t * a.cos() + f.b * a.sin();
    let tilt = o.tilt + r.rnd(-0.12, 0.12);
    let t = d * tilt.cos() + f.n * tilt.sin();
    let n = d * -tilt.sin() + f.n * tilt.cos();
    let p = Frame {
        o: f.o + d * o.r0 + f.n * o.h0,
        t,
        b: n.cross(t),
        n,
    };
    let j = r.rnd(0.85, 1.1); // brightness and slight hue shift per petal
    let h = r.rnd(-0.06, 0.06);
    let len = o.len * r.rnd(0.9, 1.1);
    let twist = r.rnd(-0.3, 0.3);
    let tip = o.tip * r.rnd(0.6, 1.3);
    let (base, tip_col) = (o.base, o.tip_col);
    b.petal(
        &p,
        (len, o.wid, o.cup, o.curl),
        |s, w| {
            let [cr, cg, cb] = lerp3(base, tip_col, s);
            let k = j * (0.72 + 0.28 * s) * (1.0 - 0.15 * (w * 2.0 - 1.0).abs()); // darker base and edges
            [cr * k * (1.0 + h), cg * k * (1.0 - h), cb * k, SPEC]
        },
        twist,
        tip,
    );
}

fn ring(b: &mut Builder, r: &mut Rng, f: &Frame, o: Pet) {
    for k in 0..o.count {
        let a = o.phase + 2.0 * PI * k as f64 / o.count as f64 + r.rnd(-0.08, 0.08);
        petal_at(b, r, f, a, &o);
    }
}

fn disk(b: &mut Builder, f: &Frame, rr: f64, rz: f64, inner: C3, outer: C3) {
    b.ellipsoid(
        f,
        (rr, rr, rz),
        20,
        10,
        |u, w| {
            let n = hash(u * 97.0 + w * 31.0) * 0.25 + 0.85;
            let [cr, cg, cb] = lerp3(inner, outer, (w * 2.2).min(1.0));
            [cr * n, cg * n, cb * n, 0.05]
        },
        None,
    );
}

fn sunflower(b: &mut Builder, r: &mut Rng, fr: &Frame, s: f64) {
    let (base, tip) = ([0.95, 0.6, 0.05], [1.0, 0.82, 0.14]);
    for (i, ph) in [(1.0, 0.0), (2.0, PI / 22.0)] {
        ring(
            b,
            r,
            fr,
            Pet {
                count: 22,
                phase: ph,
                r0: 0.15 * s,
                h0: -0.01 * i,
                len: 0.22 * s,
                wid: 0.05 * s,
                tilt: 0.05 + 0.1 * i,
                cup: 0.25,
                curl: -0.1,
                tip: -0.12,
                base,
                tip_col: tip,
            },
        );
    }
    // seed head: the two families of spirals (21 / 34) a real sunflower shows, as bumps + shading
    let seeds = |u: f64, w: f64| {
        let (th, rho) = (u * 2.0 * PI, w * 7.0);
        ((21.0 * th + 21.0 * rho).cos() * (34.0 * th - 34.0 * rho).cos()).max(0.0)
    };
    b.ellipsoid(
        fr,
        (0.18 * s, 0.18 * s, 0.07 * s),
        48,
        16,
        |u, w| {
            let t = (w * 2.2).min(1.0);
            let [cr, cg, cb] = if t < 0.55 {
                lerp3([0.07, 0.04, 0.02], [0.2, 0.1, 0.03], t / 0.55)
            } else if t < 0.8 {
                [0.12, 0.07, 0.03] // ring of dark florets
            } else {
                lerp3([0.3, 0.17, 0.04], [0.45, 0.24, 0.05], (t - 0.8) / 0.2)
            };
            let k = 0.7 + 0.6 * seeds(u, w);
            [cr * k, cg * k, cb * k, 0.08]
        },
        Some(&|u, w| 0.07 * seeds(u, w)),
    );
}

fn gerbera(b: &mut Builder, r: &mut Rng, fr: &Frame, s: f64) {
    ring(
        b,
        r,
        fr,
        Pet {
            count: 26,
            r0: 0.06 * s,
            len: 0.19 * s,
            wid: 0.024 * s,
            tilt: 0.12,
            cup: 0.4,
            curl: -0.05,
            tip: -0.08,
            base: [0.9, 0.3, 0.02],
            tip_col: [1.0, 0.48, 0.06],
            ..Pet::default()
        },
    );
    ring(
        b,
        r,
        fr,
        Pet {
            count: 18,
            phase: 0.1,
            r0: 0.05 * s,
            h0: 0.01,
            len: 0.11 * s,
            wid: 0.02 * s,
            tilt: 0.45,
            cup: 0.4,
            base: [0.85, 0.25, 0.02],
            tip_col: [0.98, 0.42, 0.05],
            ..Pet::default()
        },
    );
    // fringe of tiny florets around the eye, in a contrasting gold
    ring(
        b,
        r,
        fr,
        Pet {
            count: 34,
            r0: 0.052 * s,
            h0: 0.012 * s,
            len: 0.035 * s,
            wid: 0.007 * s,
            tilt: 0.75,
            base: [0.95, 0.55, 0.1],
            tip_col: [1.0, 0.8, 0.25],
            ..Pet::default()
        },
    );
    disk(
        b,
        fr,
        0.06 * s,
        0.03 * s,
        [0.04, 0.02, 0.01],
        [0.35, 0.06, 0.03],
    );
}

fn daisy(b: &mut Builder, r: &mut Rng, fr: &Frame, s: f64) {
    let base = if r.r() < 0.6 {
        [[0.98, 0.82, 0.05], [1.0, 0.9, 0.15]]
    } else {
        [[0.95, 0.55, 0.05], [1.0, 0.7, 0.1]]
    };
    let phase = r.rnd(0.0, 1.0);
    ring(
        b,
        r,
        fr,
        Pet {
            count: 16,
            phase,
            r0: 0.03 * s,
            len: 0.13 * s,
            wid: 0.025 * s,
            tilt: 0.25,
            cup: 0.5,
            curl: 0.08,
            tip: -0.1,
            base: base[0],
            tip_col: base[1],
            ..Pet::default()
        },
    );
    disk(
        b,
        fr,
        0.035 * s,
        0.022 * s,
        [0.45, 0.5, 0.1],
        [0.62, 0.62, 0.15],
    );
}

/// Rose: petals on a golden-angle spiral, tight and upright in the middle, opening and curling
/// back toward the outside, so they overlap like a real rose.
fn rose(b: &mut Builder, r: &mut Rng, fr: &Frame, s: f64) {
    let golden = PI * (3.0 - 5.0f64.sqrt());
    b.ellipsoid(
        &frame(fr.o + fr.n * (0.05 * s), fr.n, 0.0),
        (0.025 * s, 0.025 * s, 0.04 * s),
        10,
        6,
        |_, _| [0.9, 0.42, 0.04, 0.1],
        None,
    );
    let n = 18;
    for k in 0..n {
        let t = k as f64 / (n - 1) as f64;
        let [cr, cg, cb] = lerp3([0.92, 0.42, 0.03], [1.0, 0.82, 0.28], t);
        petal_at(
            b,
            r,
            fr,
            k as f64 * golden,
            &Pet {
                r0: (0.008 + 0.06 * t) * s,
                h0: (0.06 - 0.05 * t) * s,
                len: (0.05 + 0.08 * t) * s,
                wid: (0.04 + 0.05 * t) * s,
                tilt: 1.4 - 1.05 * t,
                cup: -0.8 + 0.3 * t,
                curl: -0.05 - 0.2 * t,
                tip: -0.2 * t,
                base: [cr * 0.85, cg * 0.85, cb],
                tip_col: [cr, cg, cb],
                ..Pet::default()
            },
        );
    }
}

fn carnation(b: &mut Builder, r: &mut Rng, fr: &Frame, s: f64) {
    let c = if r.r() < 0.5 {
        [0.78, 0.07, 0.05]
    } else {
        [0.95, 0.42, 0.35]
    };
    let fc = Frame {
        o: fr.o + fr.n * (0.04 * s),
        ..*fr
    };
    // irregular frills: the phase wanders with height so the folds don't line up into stripes
    let ruffle = |u: f64, w: f64| {
        0.12 * (u * 2.0 * PI * 17.0 + 4.0 * (w * 7.0 + u * 11.0).sin()).sin() * (w * PI * 9.0).sin()
            + 0.1 * hash(u * 71.0 + w * 13.0)
    };
    b.ellipsoid(
        &fc,
        (0.13 * s, 0.13 * s, 0.09 * s),
        40,
        14,
        |u, w| {
            let n = 0.75 + 1.4 * ruffle(u, w); // frilly edges catch light, folds go dark
            [c[0] * n, c[1] * n, c[2] * n, 0.05]
        },
        Some(&ruffle),
    );
}

fn berries(b: &mut Builder, r: &mut Rng, fr: &Frame, s: f64) {
    for k in 1..=9 {
        let (a, rr) = (k as f64 * 2.4, 0.05 * (k as f64).sqrt() * s);
        let lift = r.rnd(0.0, 0.06);
        let o = fr.o + (fr.t * (rr * a.cos()) + fr.b * (rr * a.sin())) + fr.n * (lift * s);
        let [cr, cg, cb] = lerp3([0.55, 0.03, 0.04], [0.9, 0.2, 0.06], r.r()); // ripeness varies
        b.ellipsoid(
            &frame(o, fr.n, 0.0),
            (0.034 * s, 0.034 * s, 0.045 * s),
            10,
            6,
            |_, w| {
                if w < 0.15 {
                    [0.22, 0.1, 0.04, 0.0]
                } else {
                    [cr, cg, cb, 0.9]
                }
            }, // dark tip
            None,
        );
        b.cylinder(o, o + fr.n * (-0.12 * s), 0.008 * s, 4, (0.25, 0.35, 0.12));
    }
}

fn bud(b: &mut Builder, _: &mut Rng, fr: &Frame, s: f64) {
    let o = fr.o + fr.n * (0.1 * s);
    b.ellipsoid(
        &frame(o, fr.n, 0.0),
        (0.045 * s, 0.045 * s, 0.15 * s),
        10,
        8,
        |_, w| {
            let mut c = lerp3([0.55, 0.72, 0.3], [0.35, 0.55, 0.2], w);
            if w < 0.2 {
                c = lerp3([0.75, 0.5, 0.4], c, w * 5.0);
            }
            [c[0], c[1], c[2], 0.2]
        },
        None,
    );
}

/// Leaf of (length, width) from o along dir, its face toward n; col: fixed color, else green.
fn leaf(
    b: &mut Builder,
    r: &mut Rng,
    o: DVec3,
    dir: DVec3,
    n: DVec3,
    (len, wid): (f64, f64),
    col: Option<C3>,
) {
    let d = dir.normalize_or(DVec3::Z);
    let pf = Frame {
        o,
        t: d,
        b: n.cross(d).normalize_or(DVec3::Z),
        n: n.normalize_or(DVec3::Z),
    };
    let j = r.rnd(0.8, 1.15);
    let olive = r.rnd(0.0, 0.08);
    let twist = r.rnd(-0.4, 0.4);
    let tip = r.rnd(-0.3, 0.0);
    b.petal(
        &pf,
        (len, wid, 0.4, -0.1),
        |s, w| {
            let k = j * (1.0 + 0.3 * (1.0 - (w * 2.0 - 1.0).abs() * 3.0).max(0.0)); // lighter midrib
            match col {
                Some(c) => [c[0] * k, c[1] * k, c[2] * k, 0.2],
                None => [(0.14 + olive) * k, (0.3 + 0.06 * s) * k, 0.1 * k, 0.25],
            }
        },
        twist,
        tip,
    );
}

fn fern(b: &mut Builder, r: &mut Rng, o: DVec3, dir: DVec3, n: DVec3) {
    let d = dir.normalize_or(DVec3::Z);
    let side = n.cross(d).normalize_or(DVec3::Z);
    for k in 1..=12 {
        let t = k as f64 / 12.0;
        let p = o + d * (0.6 * t);
        let l = 0.12 * (1.0 - t * 0.7);
        for sgn in [-1.0, 1.0] {
            leaf(b, r, p, side * sgn + d * 0.5, n, (l, 0.025), None);
        }
    }
    b.cylinder(o, o + d * 0.6, 0.006, 4, (0.15, 0.25, 0.08));
}

/// Wheat stalk: thin golden stem ending in a head of grains in two alternating rows.
fn wheat(b: &mut Builder, o: DVec3, dir: DVec3) {
    let d = dir.normalize_or(DVec3::Z);
    let side = dvec3(0.0, 0.0, 1.0).cross(d).normalize_or(DVec3::Z);
    let tipp = o + d * 0.6;
    b.cylinder(o, tipp, 0.008, 4, (0.75, 0.6, 0.3));
    for k in 0..=13 {
        let kf = k as f64;
        let sg = if k % 2 == 0 { 1.0 } else { -1.0 };
        let p = tipp + d * (-0.28 + 0.021 * kf);
        let g = p + side * (sg * 0.016 * (1.0 - kf / 20.0));
        let j = 0.9 + 0.2 * hash(kf * 7.3);
        b.ellipsoid(
            &frame(g, d + side * (sg * 0.5), 0.0),
            (0.017, 0.014, 0.034),
            6,
            4,
            |_, _| [0.88 * j, 0.68 * j, 0.32 * j, 0.15],
            None,
        );
    }
}

/// Eucalyptus sprig: pairs of round silver-green leaves along a curving stem.
fn eucalyptus(b: &mut Builder, r: &mut Rng, o: DVec3, dir: DVec3, n: DVec3) {
    let d = dir.normalize_or(DVec3::Z);
    let side = n.cross(d).normalize_or(DVec3::Z);
    b.cylinder(o, o + d * 0.6, 0.006, 4, (0.35, 0.3, 0.25));
    for k in 1..=6 {
        let p = o + d * (0.1 * k as f64);
        let sz = 0.075 * (1.0 - k as f64 * 0.08);
        for sg in [-1.0, 1.0] {
            let dir = side * sg + d * 0.25;
            leaf(
                b,
                r,
                p,
                dir,
                n + side * (-sg * 0.3),
                (sz, sz * 0.9),
                Some([0.42, 0.55, 0.5]),
            );
        }
    }
}

/// Goldenrod spray: a short stem with little branches of tiny gold florets (fills the gaps).
fn goldenrod(b: &mut Builder, r: &mut Rng, o: DVec3, n: DVec3) {
    let spin = r.rnd(0.0, 2.0 * PI);
    let f0 = frame(o, n, spin);
    for br in 0..=2 {
        let a = br as f64 * 2.1 + r.rnd(-0.3, 0.3);
        let bd = (f0.n * 0.8 + (f0.t * a.cos() + f0.b * a.sin())).normalize_or(DVec3::Z);
        for k in 1..=5 {
            let up = r.rnd(-0.01, 0.02);
            let p = o + bd * (0.03 * k as f64) + f0.n * up;
            let j = r.rnd(0.85, 1.1);
            b.ellipsoid(
                &frame(p, f0.n, 0.0),
                (0.016, 0.016, 0.014),
                5,
                3,
                |_, _| [0.95 * j, 0.72 * j, 0.14 * j, 0.1],
                None,
            );
        }
    }
}

// The dome is squashed along +z so the top doesn't bulge; helpers place things on it.
const FLAT: f64 = 0.6;
const TIE: f64 = -1.6; // z of the ribbon tie (narrow end of the paper)
const STEM_END: f64 = -2.8;
fn on_dome(d: DVec3, r: f64) -> DVec3 {
    dvec3(d.x * r, d.y * r, d.z * r * FLAT)
}
fn dome_normal(d: DVec3) -> DVec3 {
    dvec3(d.x, d.y, d.z / FLAT).normalize_or(DVec3::Z)
}
/// Unit direction at angle polar from the dome axis, turned az around it.
fn polar_dir(polar: f64, az: f64) -> DVec3 {
    dvec3(polar.sin() * az.cos(), polar.sin() * az.sin(), polar.cos())
}

fn pinecone(b: &mut Builder, o: DVec3, n: DVec3) {
    let scales = |u: f64, w: f64| (u * 2.0 * PI * 8.0 + w * PI * 10.0).sin().max(0.0);
    b.ellipsoid(
        &frame(o, n, 0.0),
        (0.065, 0.065, 0.11),
        24,
        12,
        |u, w| {
            let k = 0.6 + 0.5 * scales(u, w);
            [0.42 * k, 0.25 * k, 0.12 * k, 0.1]
        },
        Some(&|u, w| 0.18 * scales(u, w) * (w * PI).sin()),
    );
}

/// Twine wound a few times around the stems, from height z down.
fn twine(b: &mut Builder, z: f64) {
    let mut prev = None;
    for k in 0..=72 {
        let kf = k as f64;
        let (a, z) = (kf / 12.0 * 2.0 * PI, z - kf * 0.0028);
        let p = dvec3(0.078 * a.cos(), 0.078 * a.sin(), z);
        if let Some(q) = prev {
            b.cylinder(q, p, 0.008, 4, (0.78, 0.66, 0.45));
        }
        prev = Some(p);
    }
}

/// Satin bow on the -y side of the tie: two real loops of ribbon, a knot and two tails.
/// band: radius of the ribbon band the knot sits on; skirt: widest radius of the paper
/// below the tie, which the tails hang outside.
fn bow(b: &mut Builder, band: f64, skirt: f64, c: C3) {
    let col = |_: f64, u: f64| {
        let k = 0.8 + 0.4 * (1.0 - (u * 2.0 - 1.0).abs()); // satin sheen across the band
        [c[0] * k, c[1] * k, c[2] * k, 0.6]
    };
    let knot = dvec3(0.0, -(band + 0.03), TIE);
    let (out, up) = (dvec3(0.0, -1.0, 0.0), dvec3(0.0, 0.0, 1.0));
    b.ellipsoid(
        &frame(knot, out, 0.0),
        (0.045, 0.045, 0.04),
        12,
        6,
        |_, _| [c[0], c[1], c[2], 0.6],
        None,
    );
    for sx in [-1.0, 1.0] {
        let side = dvec3(sx, 0.0, 0.0);
        let (mut pts, mut dirs) = (Vec::new(), Vec::new());
        for k in 0..=16 {
            // loop: out from the knot and back, a flattened ellipse in the side/out plane
            let th = k as f64 / 16.0 * 2.0 * PI;
            let p = knot
                + (side * (0.17 * (1.0 - th.cos()))
                    + out * (0.07 * th.sin() + 0.03 * (1.0 - th.cos())));
            pts.push(p + up * (0.02 * (1.0 - th.cos())));
            dirs.push(up);
        }
        b.ribbon(&pts, &dirs, 0.07, col);
        (pts, dirs) = (Vec::new(), Vec::new());
        for k in 0..=10 {
            // tail hanging down with a gentle wave, cut at the end
            let t = k as f64 / 10.0;
            let p = knot
                + (side * (0.12 * t)
                    + (up * (-0.5 * t) + out * (0.05 * (t * 4.0).sin() + (skirt - band).max(0.0))));
            pts.push(p);
            dirs.push((side + up * 0.3).normalize_or(DVec3::Z));
        }
        b.ribbon(&pts, &dirs, 0.06, col);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Sunflower,
    Gerbera,
    Rose,
    Carnation,
    Daisy,
    Berries,
    Bud,
}

type Flower = fn(&mut Builder, &mut Rng, &Frame, f64);
impl Kind {
    /// Relative size and the function that builds one flower of this kind.
    fn spec(self) -> (f64, Flower) {
        match self {
            Kind::Sunflower => (1.2, sunflower),
            Kind::Gerbera => (1.3, gerbera),
            Kind::Rose => (1.0, rose),
            Kind::Carnation => (1.05, carnation),
            Kind::Daisy => (1.15, daisy),
            Kind::Berries => (1.0, berries),
            Kind::Bud => (1.0, bud),
        }
    }
}

pub fn build() -> Scene {
    use Kind::*;
    let mut r = Rng(1234567.0);
    let mut b = Builder::new();
    // foliage under the heads fills the gaps (no solid core: it showed as a blob)
    for _ in 0..80 {
        let a = r.rnd(0.0, 2.0 * PI);
        let z = r.rnd(0.05, 1.0);
        let rr = (1.0 - z * z).sqrt();
        let dir = dvec3(rr * a.cos(), rr * a.sin(), z);
        let n = dome_normal(dir);
        let spin = r.rnd(0.0, 2.0 * PI);
        let f0 = frame(on_dome(dir, 0.8), n, spin);
        let len = r.rnd(0.18, 0.3);
        let wid = r.rnd(0.06, 0.1);
        leaf(&mut b, &mut r, f0.o, f0.t + n * 0.3, n, (len, wid), None);
    }

    // Round florist layout: the sunflower in the middle, gerberas and roses around it, then
    // small clusters (2-3 of a kind) so it reads hand-made rather than strictly alternating.
    // polar = angle from the dome axis.
    let rings: [(f64, f64, &[Kind]); 5] = [
        (0.0, 0.0, &[Sunflower]),
        (0.6, 0.3, &[Gerbera, Rose, Gerbera, Rose, Gerbera, Rose]),
        (0.84, 0.82, &[Berries; 6]),
        (
            1.02,
            0.0,
            &[
                Carnation, Carnation, Daisy, Daisy, Daisy, Berries, Carnation, Bud, Berries,
            ],
        ),
        (
            1.28,
            0.26,
            &[
                Daisy, Daisy, Carnation, Bud, Gerbera, Berries, Berries, Daisy, Daisy, Carnation,
                Gerbera, Bud,
            ],
        ),
    ];
    for (rpolar, rphase, kinds) in rings {
        let n = kinds.len();
        for (k, &kind) in kinds.iter().enumerate() {
            let az = rphase + 2.0 * PI * k as f64 / n as f64 + r.rnd(-0.06, 0.06);
            let polar = if rpolar > 0.0 {
                rpolar + r.rnd(-0.04, 0.04)
            } else {
                0.0
            };
            let dir = polar_dir(polar, az);
            // heights vary; a few buds and daisies stand up out of the dome
            let mut rr = (if kind == Berries { 0.9 } else { 0.95 }) + r.rnd(-0.03, 0.04);
            if rpolar > 1.0 && (kind == Bud || kind == Daisy) && r.r() < 0.4 {
                rr += 0.13;
            }
            let o = on_dome(dir, rr);
            let tilt = if rpolar > 0.0 { 0.1 } else { 0.0 };
            let tx = r.rnd(-tilt, tilt);
            let ty = r.rnd(-tilt, tilt);
            let n0 = (dome_normal(dir) + dvec3(tx, ty, 0.25)).normalize_or(DVec3::Z);
            let spin = r.rnd(0.0, 2.0 * PI);
            let fr = frame(o, n0, spin);
            let (size, flower) = kind.spec();
            let s = size * r.rnd(0.93, 1.07);
            flower(&mut b, &mut r, &fr, s);
            // stems converge inside the paper cone at the tie
            let sx = r.rnd(-0.04, 0.04);
            let sy = r.rnd(-0.04, 0.04);
            b.cylinder(
                on_dome(dir, rr - 0.15),
                dvec3(sx, sy, TIE),
                0.015,
                5,
                (0.2, 0.32, 0.12),
            );
        }
    }

    // goldenrod filler in the gaps between rings
    for k in 0..=5 {
        let dir = polar_dir(0.84, 0.82 + PI / 6.0 + k as f64 * PI / 3.0);
        goldenrod(&mut b, &mut r, on_dome(dir, 0.93), dome_normal(dir));
    }
    for k in 0..=9 {
        let dir = polar_dir(1.16, 0.35 + k as f64 * 2.0 * PI / 10.0);
        goldenrod(&mut b, &mut r, on_dome(dir, 0.95), dome_normal(dir));
    }

    // Foliage spilling out past the paper (built separately: the paper wraps the flowers and
    // lets these hang over its edge). Leaves and ferns sticking out around the rim:
    let mut sb = Builder::new();

    for k in 0..=13 {
        let a = k as f64 / 14.0 * 2.0 * PI + r.rnd(-0.1, 0.1);
        let out = dvec3(a.cos(), a.sin(), 0.0);
        let dz = r.rnd(-0.1, 0.1);
        let o = out * 0.75 + dvec3(0.0, 0.0, dz);
        let up = r.rnd(0.1, 0.4);
        let dir = out + dvec3(0.0, 0.0, up);
        if k % 5 == 2 {
            fern(&mut sb, &mut r, o, dir, dvec3(0.0, 0.0, 1.0));
        } else {
            let len = r.rnd(0.4, 0.55);
            let wid = r.rnd(0.09, 0.13);
            leaf(
                &mut sb,
                &mut r,
                o,
                dir,
                dvec3(0.0, 0.0, 1.0) + out * -0.3,
                (len, wid),
                None,
            );
        }
    }
    // things spilling out past the paper break up the round silhouette
    for a in [0.4f64, 1.1, 2.6, 3.9, 5.3] {
        // wheat, standing tall
        let out = dvec3(a.cos(), a.sin(), 0.0);
        wheat(
            &mut sb,
            out * 0.55 + dvec3(0.0, 0.0, 0.1),
            out * 0.7 + dvec3(0.0, 0.0, 0.75),
        );
    }
    for a in [0.9f64, 2.2, 3.4, 4.7] {
        // eucalyptus, arching outward
        let out = dvec3(a.cos(), a.sin(), 0.0);
        let n = dvec3(0.0, 0.0, 1.0) + out * -0.4;
        eucalyptus(&mut sb, &mut r, out * 0.8, out + dvec3(0.0, 0.0, 0.25), n);
    }
    let autumn = [[0.6, 0.2, 0.05], [0.45, 0.08, 0.04], [0.7, 0.38, 0.08]];
    for (k, a) in [0.15f64, 1.5, 2.9, 4.2, 5.0, 5.8].into_iter().enumerate() {
        // autumn leaves drooping over the edge, in pairs, fanned
        let out = dvec3(a.cos(), a.sin(), 0.0);
        for j in [-1i32, 1] {
            let side = dvec3(-a.sin(), a.cos(), 0.0);
            let o = out * 0.9 + dvec3(0.0, 0.0, 0.02);
            let dir = out + side * (0.35 * j as f64) + dvec3(0.0, 0.0, -0.1);
            let col = autumn[(k as i32 + 1 + j).rem_euclid(3) as usize];
            leaf(
                &mut sb,
                &mut r,
                o,
                dir,
                dvec3(0.0, 0.0, 1.0) + out * -0.2,
                (0.22, 0.075),
                Some(col),
            );
        }
    }
    // back: collar of leaves hiding the core, stems gathered into a tied bundle
    for k in 0..=15 {
        let a = (k as f64 + 0.5) / 16.0 * 2.0 * PI + r.rnd(-0.1, 0.1);
        let out = dvec3(a.cos(), a.sin(), 0.0);
        let len = r.rnd(0.6, 0.75);
        let wid = r.rnd(0.18, 0.24);
        leaf(
            &mut b,
            &mut r,
            out * 0.3 + dvec3(0.0, 0.0, -0.2),
            out + dvec3(0.0, 0.0, 0.1),
            dvec3(0.0, 0.0, -1.0) + out,
            (len, wid),
            None,
        );
    }
    for k in 0..=8 {
        // stem bundle below the tie
        let a = k as f64 / 9.0 * 2.0 * PI;
        b.cylinder(
            dvec3(0.05 * a.cos(), 0.05 * a.sin(), TIE),
            dvec3(0.08 * a.cos(), 0.08 * a.sin(), STEM_END),
            0.018,
            5,
            (0.25, 0.36, 0.13),
        );
    }
    let (mut bouquet, mut spill) = (b.finish(), sb.finish());
    // baked occlusion: surfaces deep inside the dome get less light than the outer tips
    for m in [&mut bouquet, &mut spill] {
        for i in 0..m.n {
            let rr = (m.px[i].powi(2) + m.py[i].powi(2) + (m.pz[i] / FLAT).powi(2)).sqrt();
            let t = ((rr - 0.5) / 0.5).clamp(0.0, 1.0);
            let ao = 0.45 + 0.55 * t * t * (3.0 - 2.0 * t);
            m.cr[i] *= ao;
            m.cg[i] *= ao;
            m.cb[i] *= ao;
        }
    }

    // pine cones tucked in at the rim: outside the dome occlusion, inside the paper
    let mut cb = Builder::new();
    for k in 0..=2 {
        let a = 0.9 + k as f64 * 2.2;
        let out = dvec3(a.cos(), a.sin(), 0.0);
        pinecone(
            &mut cb,
            out * 0.92 + dvec3(0.0, 0.0, 0.05),
            out + dvec3(0.0, 0.0, 0.8),
        );
    }
    let cones = cb.finish();
    let paper = paper::wrap(&[&bouquet, &cones], &spill, TIE);
    // ribbon tying the paper, twine around the stems below it
    let mut rb = Builder::new();
    let n = paper.band.len();
    rb.grid(n, 1, true, |u, w| {
        let (a, rr) = (
            u * 2.0 * PI,
            paper.band[(u * n as f64).round() as usize % n],
        );
        let z = TIE - paper::BAND + 2.0 * paper::BAND * w;
        [rr * a.cos(), rr * a.sin(), z, 0.62, 0.12, 0.08, 0.0]
    });
    twine(&mut rb, TIE - paper::SKIRT - 0.03); // below the paper skirt
    let front = paper.band[n * 3 / 4]; // the knot sits on the band at -y
    bow(&mut rb, front, paper.skirt, [0.85, 0.16, 0.1]); // bright satin: it sits at the dim end of the light
    let ribbon = rb.finish();

    // Rotation happens about the origin; move the bouquet so its visual middle (between the
    // flower dome and the tie) sits there, otherwise dragging swings it around its base.
    let meshes = [bouquet, spill, cones, ribbon]
        .into_iter()
        .chain(paper.sheets);
    let mut items: Vec<Item> = meshes
        .map(|mut mesh| {
            for z in &mut mesh.pz {
                *z += 0.55;
            }
            Item {
                mesh,
                rotate: true,
                caster: true,
            }
        })
        .collect();
    items.push(super::backdrop::build());
    Scene {
        items,
        radius: 2.26, // farthest point from the pivot (stem ends / wheat tips)
    }
}

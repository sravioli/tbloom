// Mesh builder: primitives emit positions + colors, normals are computed on finish().
use super::math3d::{DVec3, Frame, frame};
use std::f64::consts::PI;
use std::rc::Rc;

/// Downscaled texture, channels in [0, 1]. alpha: some texels are not opaque (cut-outs).
pub struct Texture {
    pub w: usize,
    pub h: usize,
    pub r: Vec<f32>,
    pub g: Vec<f32>,
    pub b: Vec<f32>,
    pub a: Vec<f32>,
    pub alpha: bool,
}

/// Per-vertex values the rasterizer computes every frame.
pub struct Temps {
    pub sx: Vec<f64>,
    pub sy: Vec<f64>,
    pub iz: Vec<f64>,
    pub d: Vec<f64>,
    pub s: Vec<f64>,
    pub rim: Vec<f64>,
    pub sd: Vec<f64>,
    pub tu: Vec<f64>,
    pub tv: Vec<f64>,
}

pub struct Mesh {
    pub n: usize,
    pub nt: usize,
    pub idx: Vec<usize>,
    /// part of a closed surface: back faces can be skipped
    pub cl: Vec<bool>,
    pub px: Vec<f64>,
    pub py: Vec<f64>,
    pub pz: Vec<f64>,
    pub nx: Vec<f64>,
    pub ny: Vec<f64>,
    pub nz: Vec<f64>,
    pub cr: Vec<f64>,
    pub cg: Vec<f64>,
    pub cb: Vec<f64>,
    pub sp: Vec<f64>,
    pub mu: Vec<f64>,
    pub mv: Vec<f64>,
    pub tex: Option<Rc<Texture>>,
    pub t: Temps,
}

impl Mesh {
    /// Vertex indices of triangle k.
    pub fn corners(&self, k: usize) -> [usize; 3] {
        [self.idx[k * 3], self.idx[k * 3 + 1], self.idx[k * 3 + 2]]
    }
}

#[derive(Default)]
pub struct Builder {
    x: Vec<f64>,
    y: Vec<f64>,
    z: Vec<f64>,
    r: Vec<f64>,
    g: Vec<f64>,
    b: Vec<f64>,
    s: Vec<f64>,
    u: Vec<f64>,
    w: Vec<f64>,
    idx: Vec<usize>,
    closed: Vec<bool>,
}

fn at(f: &Frame, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let (o, t, b, n) = (f.o, f.t, f.b, f.n);
    (
        o.x + t.x * x + b.x * y + n.x * z,
        o.y + t.y * x + b.y * y + n.y * z,
        o.z + t.z * x + b.z * y + n.z * z,
    )
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// p = x, y, z, r, g, b, specular; uv = texture coordinates. Returns the vertex index.
    pub fn v(&mut self, p: [f64; 7], uv: [f64; 2]) -> usize {
        self.x.push(p[0]);
        self.y.push(p[1]);
        self.z.push(p[2]);
        self.r.push(p[3]);
        self.g.push(p[4]);
        self.b.push(p[5]);
        self.s.push(p[6]);
        self.u.push(uv[0]);
        self.w.push(uv[1]);
        self.x.len() - 1
    }

    pub fn t(&mut self, a: usize, b: usize, c: usize) {
        self.idx.extend([a, b, c]);
    }

    /// Grid of (nu x nv) quads; f(u, v) -> x, y, z, r, g, b, s with u, v in [0, 1].
    pub fn grid(
        &mut self,
        nu: usize,
        nv: usize,
        wrap: bool,
        mut f: impl FnMut(f64, f64) -> [f64; 7],
    ) {
        let (base, cols) = (self.x.len(), if wrap { nu } else { nu + 1 });
        for j in 0..=nv {
            for i in 0..cols {
                self.v(f(i as f64 / nu as f64, j as f64 / nv as f64), [0.0, 0.0]);
            }
        }
        for j in 0..nv {
            for i in 0..nu {
                let i2 = if wrap { (i + 1) % nu } else { i + 1 };
                let (a, b) = (base + j * cols + i, base + j * cols + i2);
                let (c, d) = (base + (j + 1) * cols + i2, base + (j + 1) * cols + i);
                self.t(a, b, c);
                self.t(a, c, d);
            }
        }
    }

    /// Mark triangles added since index i0 (into idx) as one closed surface: wind them outward
    /// (positive signed volume about center c) so the rasterizer can skip their back faces.
    fn close(&mut self, i0: usize, c: DVec3) {
        let (idx, x, y, z) = (&mut self.idx, &self.x, &self.y, &self.z);
        let mut vol = 0.0;
        for k in (i0..idx.len()).step_by(3) {
            let (a, b, d) = (idx[k], idx[k + 1], idx[k + 2]);
            let (ax, ay, az) = (x[a] - c.x, y[a] - c.y, z[a] - c.z);
            let (bx, by, bz) = (x[b] - c.x, y[b] - c.y, z[b] - c.z);
            let (dx, dy, dz) = (x[d] - c.x, y[d] - c.y, z[d] - c.z);
            vol = vol + ax * (by * dz - bz * dy) - ay * (bx * dz - bz * dx)
                + az * (bx * dy - by * dx);
        }
        self.closed.resize(idx.len() / 3, false);
        for k in (i0..idx.len()).step_by(3) {
            if vol < 0.0 {
                idx.swap(k + 1, k + 2);
            }
            self.closed[k / 3] = true;
        }
    }

    /// Ellipsoid in frame f (rz along f.n). col(u, v) -> r, g, b, s; disp(u, v) -> radial scale offset.
    pub fn ellipsoid(
        &mut self,
        f: &Frame,
        (rx, ry, rz): (f64, f64, f64),
        nu: usize,
        nv: usize,
        col: impl Fn(f64, f64) -> [f64; 4],
        disp: Option<&dyn Fn(f64, f64) -> f64>,
    ) {
        let i0 = self.idx.len();
        self.grid(nu, nv, true, |u, v| {
            let (th, ph) = (u * 2.0 * PI, v * PI);
            let k = 1.0 + disp.map_or(0.0, |d| d(u, v));
            let (x, y, z) = at(
                f,
                rx * k * ph.sin() * th.cos(),
                ry * k * ph.sin() * th.sin(),
                rz * k * ph.cos(),
            );
            let c = col(u, v);
            [x, y, z, c[0], c[1], c[2], c[3]]
        });
        self.close(i0, f.o);
    }

    /// Petal along f.t, width along f.b, bending toward f.n. col(s, v) with s along length, v across.
    /// twist turns the blade about its length toward the tip; tip curls the last part (negative =
    /// backward, away from f.n).
    pub fn petal(
        &mut self,
        f: &Frame,
        (len, wid, cup, curl): (f64, f64, f64, f64),
        col: impl Fn(f64, f64) -> [f64; 4],
        twist: f64,
        tip: f64,
    ) {
        self.grid(2, 4, false, |v, s| {
            let w = wid * (PI * (0.1 + 0.9 * s)).sin().powf(0.6);
            let y = (v * 2.0 - 1.0) * w;
            let z = cup * (y / wid).powi(2) * wid
                + curl * s * s * len
                + tip * len * (s - 0.55).max(0.0).powi(2) * 5.0;
            let a = twist * s;
            let (yy, zz) = (y * a.cos() - z * a.sin(), y * a.sin() + z * a.cos());
            let (x, y2, z2) = at(f, len * s, yy, zz);
            let c = col(s, v);
            [x, y2, z2, c[0], c[1], c[2], c[3]]
        });
    }

    /// Flat band through points[i] with its width along wdirs[i] (bow loops, ribbon tails).
    pub fn ribbon(
        &mut self,
        points: &[DVec3],
        wdirs: &[DVec3],
        width: f64,
        col: impl Fn(f64, f64) -> [f64; 4],
    ) {
        let n = points.len();
        self.grid(1, n - 1, false, |u, v| {
            let i = (v * (n - 1) as f64 + 0.5).floor() as usize;
            let (p, d, h) = (points[i], wdirs[i], (u - 0.5) * width);
            let c = col(v, u);
            [
                p.x + d.x * h,
                p.y + d.y * h,
                p.z + d.z * h,
                c[0],
                c[1],
                c[2],
                c[3],
            ]
        });
    }

    /// Open-ended tube: not closed, so seen end-on its inside is still drawn.
    pub fn cylinder(
        &mut self,
        a: DVec3,
        b: DVec3,
        r: f64,
        seg: usize,
        (cr, cg, cb): (f64, f64, f64),
    ) {
        let axis = b - a;
        let f = frame(a, axis, 0.0);
        let len = axis.dot(axis).sqrt();
        self.grid(seg, 1, true, |u, v| {
            let th = u * 2.0 * PI;
            let (x, y, z) = at(&f, r * th.cos(), r * th.sin(), len * v);
            [x, y, z, cr, cg, cb, 0.0]
        });
    }

    /// Freeze with area-weighted vertex normals.
    pub fn finish(mut self) -> Mesh {
        let (n, idx) = (self.x.len(), &self.idx);
        let (x, y, z) = (&self.x, &self.y, &self.z);
        let (mut nx, mut ny, mut nz) = (vec![0.0; n], vec![0.0; n], vec![0.0; n]);
        for k in (0..idx.len()).step_by(3) {
            let (a, b, c) = (idx[k], idx[k + 1], idx[k + 2]);
            let (ux, uy, uz) = (x[b] - x[a], y[b] - y[a], z[b] - z[a]);
            let (vx, vy, vz) = (x[c] - x[a], y[c] - y[a], z[c] - z[a]);
            let (fx, fy, fz) = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx);
            for i in [a, b, c] {
                nx[i] += fx;
                ny[i] += fy;
                nz[i] += fz;
            }
        }
        for i in 0..n {
            let l = (nx[i] * nx[i] + ny[i] * ny[i] + nz[i] * nz[i]).sqrt();
            if l > 0.0 {
                nx[i] /= l;
                ny[i] /= l;
                nz[i] /= l;
            } else {
                nz[i] = 1.0;
            }
        }
        let nt = idx.len() / 3;
        self.closed.resize(nt, false);
        let z = || vec![0.0; n];
        let t = Temps {
            sx: z(),
            sy: z(),
            iz: z(),
            d: z(),
            s: z(),
            rim: z(),
            sd: z(),
            tu: z(),
            tv: z(),
        };
        Mesh {
            n,
            nt,
            idx: self.idx,
            cl: self.closed,
            px: self.x,
            py: self.y,
            pz: self.z,
            nx,
            ny,
            nz,
            cr: self.r,
            cg: self.g,
            cb: self.b,
            sp: self.s,
            mu: self.u,
            mv: self.w,
            tex: None,
            t,
        }
    }
}

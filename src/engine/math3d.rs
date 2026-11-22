// Tiny vec3 helpers (used at build time, not in hot loops).
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

pub const fn v(x: f64, y: f64, z: f64) -> V3 {
    V3 { x, y, z }
}
pub fn add(a: V3, b: V3) -> V3 {
    v(a.x + b.x, a.y + b.y, a.z + b.z)
}
pub fn sub(a: V3, b: V3) -> V3 {
    v(a.x - b.x, a.y - b.y, a.z - b.z)
}
pub fn scale(a: V3, s: f64) -> V3 {
    v(a.x * s, a.y * s, a.z * s)
}
pub fn dot(a: V3, b: V3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
pub fn cross(a: V3, b: V3) -> V3 {
    v(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}
pub fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 0.0 {
        scale(a, 1.0 / l)
    } else {
        v(0.0, 0.0, 1.0)
    }
}
/// a*s + b*t
pub fn mix(a: V3, s: f64, b: V3, t: f64) -> V3 {
    v(a.x * s + b.x * t, a.y * s + b.y * t, a.z * s + b.z * t)
}

/// Orthonormal frame with normal n.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub o: V3,
    pub t: V3,
    pub b: V3,
    pub n: V3,
}

/// Frame at o with normal n, spun by angle around n.
pub fn frame(o: V3, n: V3, angle: f64) -> Frame {
    let n = norm(n);
    let r = if n.y.abs() < 0.9 {
        v(0.0, 1.0, 0.0)
    } else {
        v(1.0, 0.0, 0.0)
    };
    let t = norm(cross(r, n));
    let b = cross(n, t);
    let (c, s) = (angle.cos(), angle.sin());
    Frame {
        o,
        t: mix(t, c, b, s),
        b: mix(b, c, t, -s),
        n,
    }
}

/// Flat row-major 3x3 matrix.
pub type M3 = [f64; 9];
pub const IDENTITY: M3 = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// Rotation matrix = Rx(pitch) * Ry(yaw).
pub fn rot(yaw: f64, pitch: f64) -> M3 {
    let (cy, sy, cp, sp) = (yaw.cos(), yaw.sin(), pitch.cos(), pitch.sin());
    [cy, 0.0, sy, sp * sy, cp, -sp * cy, -cp * sy, sp, cp * cy]
}

/// Rotation about z.
pub fn rz(a: f64) -> M3 {
    let (c, s) = (a.cos(), a.sin());
    [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0]
}

pub fn mul(a: &M3, b: &M3) -> M3 {
    let mut r = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            r[i * 3 + j] = a[i * 3] * b[j] + a[i * 3 + 1] * b[j + 3] + a[i * 3 + 2] * b[j + 6];
        }
    }
    r
}

/// Turntable pose. The model's z axis (the stems) is first stood upright along screen y, then:
/// az spins it about that vertical axis, elev tilts it toward the viewer about screen x, and
/// lean rolls it about the view axis. Each control moves one angle.
pub fn pose([az, elev, lean]: [f64; 3]) -> M3 {
    let upright = rot(0.0, -PI / 2.0); // model +z (flowers) -> screen +y
    mul(
        &rz(lean),
        &mul(&rot(0.0, elev), &mul(&rot(az, 0.0), &upright)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition() {
        // two half turns about y = identity; mul matches rot's definition
        let i = mul(&rot(PI, 0.0), &rot(PI, 0.0));
        for k in 0..9 {
            assert!((i[k] - IDENTITY[k]).abs() < 1e-9);
        }
        let (a, b) = (mul(&rot(0.0, 0.7), &rot(0.4, 0.0)), rot(0.4, 0.7));
        for k in 0..9 {
            assert!((a[k] - b[k]).abs() < 1e-9);
        }
    }

    #[test]
    fn turntable_keeps_stems_vertical() {
        for az in [0.0, 1.3, -2.9] {
            let p = pose([az, 0.0, 0.0]);
            assert!(p[2].abs() < 1e-9 && (p[5] - 1.0).abs() < 1e-9 && p[8].abs() < 1e-9);
        }
    }
}

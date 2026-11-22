// 3D helpers on glam: local frames for placing parts, and the turntable pose.
pub use glam::{DMat3, DQuat, DVec3, dvec3};
use std::f64::consts::PI;

/// Orthonormal frame with normal n.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub o: DVec3,
    pub t: DVec3,
    pub b: DVec3,
    pub n: DVec3,
}

/// Frame at o with normal n, spun by angle around n.
pub fn frame(o: DVec3, n: DVec3, angle: f64) -> Frame {
    let n = n.normalize_or(DVec3::Z);
    let r = if n.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
    let t = r.cross(n).normalize_or(DVec3::Z);
    let b = n.cross(t);
    let (c, s) = (angle.cos(), angle.sin());
    Frame {
        o,
        t: t * c + b * s,
        b: b * c + t * -s,
        n,
    }
}

/// Turntable pose. The model's z axis (the stems) is first stood upright along screen y, then:
/// az spins it about that vertical axis, elev tilts it toward the viewer about screen x, and
/// lean rolls it about the view axis.
pub fn pose([az, elev, lean]: [f64; 3]) -> DMat3 {
    let upright = DMat3::from_rotation_x(-PI / 2.0); // model +z (flowers) -> screen +y
    DMat3::from_rotation_z(lean)
        * (DMat3::from_rotation_x(elev) * (DMat3::from_rotation_y(az) * upright))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turntable_keeps_stems_vertical() {
        for az in [0.0, 1.3, -2.9] {
            let up = pose([az, 0.0, 0.0]) * DVec3::Z;
            assert!((up - DVec3::Y).length() < 1e-9, "{up}");
        }
    }
}

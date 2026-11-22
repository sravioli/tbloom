// View controls: one rotation axis at a time, picked by key, always fixed on the screen, so a
// drag moves the picture the way the hand moves from any orientation.
use crate::engine::math3d::{DMat3, DQuat, pose};
use crate::engine::raster::CAMZ;
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseEventKind};

/// Turntable angles (az, elev, lean) and eye offset: where the view starts and resets to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub ang: [f64; 3],
    pub cam: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Axis {
    /// about the screen's vertical axis (horizontal drag)
    Turn,
    /// about the screen's horizontal axis (vertical drag)
    Tilt,
    /// about the view axis (horizontal drag)
    Roll,
}

impl Axis {
    fn next(self) -> Axis {
        match self {
            Axis::Turn => Axis::Tilt,
            Axis::Tilt => Axis::Roll,
            Axis::Roll => Axis::Turn,
        }
    }
    /// Rotation by a (radians) about this screen axis; positive follows a right or down drag.
    fn rot(self, a: f64) -> DMat3 {
        match self {
            Axis::Turn => DMat3::from_rotation_y(a),
            Axis::Tilt => DMat3::from_rotation_x(a),
            Axis::Roll => DMat3::from_rotation_z(-a),
        }
    }
}

pub struct View {
    pub rot: DMat3,
    pub cam: [f64; 3],
    pub axis: Axis,
    start: (DMat3, [f64; 3]),
    /// nearest eye offset: the eye (at CAMZ + zoom) stays outside the scene, plus the 0.1 near clip
    zoom_min: f64,
    /// last mouse cell while a button is held
    drag: Option<(u16, u16)>,
}

const STEP: f64 = 0.12; // per key press
const DRAG: f64 = 0.06; // per cell dragged (cells are about twice as tall as wide)

impl View {
    /// radius: farthest point of the scene from the pivot.
    pub fn new(start: Pose, radius: f64) -> Self {
        let rot = pose(start.ang);
        View {
            rot,
            cam: start.cam,
            axis: Axis::Turn,
            start: (rot, start.cam),
            zoom_min: radius + 0.15 - CAMZ,
            drag: None,
        }
    }

    fn rotate(&mut self, axis: Axis, a: f64) {
        // through a unit quaternion, so repeated small turns never skew the matrix
        let q = DQuat::from_mat3(&(axis.rot(a) * self.rot)).normalize();
        self.rot = DMat3::from_quat(q);
    }

    /// Footer text naming the axis a drag moves.
    pub fn label(&self) -> &'static str {
        match self.axis {
            Axis::Turn => " drag: TURN \u{2194}  Tab: axis ",
            Axis::Tilt => " drag: TILT \u{2195}  Tab: axis ",
            Axis::Roll => " drag: ROLL \u{21bb}  Tab: axis ",
        }
    }

    /// Apply one input event; true when the view (or the axis label) changed.
    pub fn apply(&mut self, e: &Event) -> bool {
        match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                let step = match k.code {
                    KeyCode::Tab => {
                        self.axis = self.axis.next();
                        return true;
                    }
                    KeyCode::Char(c @ '1'..='3') => {
                        self.axis = [Axis::Turn, Axis::Tilt, Axis::Roll][c as usize - '1' as usize];
                        return true;
                    }
                    KeyCode::Char('r' | 'R') => {
                        (self.rot, self.cam) = self.start;
                        return true;
                    }
                    KeyCode::Char('a') | KeyCode::Left => (Axis::Turn, -STEP),
                    KeyCode::Char('d') | KeyCode::Right => (Axis::Turn, STEP),
                    KeyCode::Char('w' | 'W') | KeyCode::Up => (Axis::Tilt, -STEP),
                    KeyCode::Char('s' | 'S') | KeyCode::Down => (Axis::Tilt, STEP),
                    KeyCode::Char('A') => (Axis::Roll, -STEP),
                    KeyCode::Char('D') => (Axis::Roll, STEP),
                    _ => return false,
                };
                self.rotate(step.0, step.1);
                true
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    let d = if m.kind == MouseEventKind::ScrollUp {
                        -0.25
                    } else {
                        0.25
                    };
                    self.cam[2] = (self.cam[2] + d).min(6.0).max(self.zoom_min);
                    true
                }
                MouseEventKind::Down(_) => {
                    self.drag = Some((m.column, m.row));
                    false
                }
                MouseEventKind::Drag(_) => {
                    let Some((px, py)) = self.drag.replace((m.column, m.row)) else {
                        return false;
                    };
                    let (dx, dy) = (m.column as f64 - px as f64, m.row as f64 - py as f64);
                    let d = match self.axis {
                        Axis::Tilt => dy * DRAG * 2.0,
                        _ => dx * DRAG,
                    };
                    if d == 0.0 {
                        return false;
                    }
                    self.rotate(self.axis, d);
                    true
                }
                MouseEventKind::Up(_) => {
                    self.drag = None;
                    false
                }
                _ => false,
            },
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::math3d::DVec3;
    use crossterm::event::{KeyEvent, KeyModifiers, MouseButton, MouseEvent};

    const START: Pose = Pose {
        ang: [-0.079, 0.756, 0.393],
        cam: [0.0, 0.0, -0.5],
    };
    fn key(c: KeyCode) -> Event {
        Event::Key(KeyEvent::new(c, KeyModifiers::NONE))
    }
    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
        let modifiers = KeyModifiers::NONE;
        Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers,
        })
    }
    fn drag(v: &mut View, (x0, y0): (u16, u16), (x1, y1): (u16, u16)) -> bool {
        v.apply(&mouse(MouseEventKind::Down(MouseButton::Left), x0, y0));
        let moved = v.apply(&mouse(MouseEventKind::Drag(MouseButton::Left), x1, y1));
        v.apply(&mouse(MouseEventKind::Up(MouseButton::Left), x1, y1));
        moved
    }
    /// The screen axis the change since `before` rotated about stays put.
    fn keeps(v: &View, before: DMat3, axis: DVec3) -> bool {
        let delta = v.rot * before.transpose();
        let moved = !delta.abs_diff_eq(DMat3::IDENTITY, 1e-9);
        moved && (delta * axis - axis).length() < 1e-9
    }

    #[test]
    fn each_axis_rotates_about_its_screen_axis() {
        let mut v = View::new(START, 2.0);
        for (k, axis, screen) in [
            (KeyCode::Char('1'), Axis::Turn, DVec3::Y),
            (KeyCode::Char('2'), Axis::Tilt, DVec3::X),
            (KeyCode::Char('3'), Axis::Roll, DVec3::Z),
        ] {
            assert!(v.apply(&key(k)) && v.axis == axis);
            let before = v.rot;
            assert!(drag(&mut v, (10, 10), (14, 13)));
            assert!(keeps(&v, before, screen), "{axis:?} drag");
        }
        // a drag right turns the front (+z, toward the viewer) to the right (+x)
        v.apply(&key(KeyCode::Char('1')));
        let before = v.rot;
        drag(&mut v, (10, 10), (12, 10));
        assert!((v.rot * before.transpose() * DVec3::Z).x > 0.0);
    }

    #[test]
    fn keys_tab_and_reset() {
        let mut v = View::new(START, 2.0);
        assert_eq!(v.rot, pose(START.ang), "starts at the fitted pose exactly");
        for (k, screen) in [
            (KeyCode::Char('d'), DVec3::Y),
            (KeyCode::Up, DVec3::X),
            (KeyCode::Char('A'), DVec3::Z),
        ] {
            let before = v.rot;
            assert!(v.apply(&key(k)));
            assert!(keeps(&v, before, screen), "{k:?}");
        }
        for want in [Axis::Tilt, Axis::Roll, Axis::Turn] {
            v.apply(&key(KeyCode::Tab));
            assert_eq!(v.axis, want);
        }
        assert!(!v.apply(&key(KeyCode::Char('x'))));
        assert!(v.apply(&key(KeyCode::Char('r'))));
        assert!(v.rot == pose(START.ang) && v.cam == START.cam);
        // motion without a press does nothing
        assert!(!v.apply(&mouse(MouseEventKind::Drag(MouseButton::Left), 3, 3)));
    }

    #[test]
    fn wheel_zoom_is_clamped() {
        let mut v = View::new(START, 2.0);
        for _ in 0..100 {
            v.apply(&mouse(MouseEventKind::ScrollUp, 1, 1));
        }
        assert!(
            (v.cam[2] - (2.0 + 0.15 - CAMZ)).abs() < 1e-12,
            "nearest zoom"
        );
        for _ in 0..100 {
            v.apply(&mouse(MouseEventKind::ScrollDown, 1, 1));
        }
        assert_eq!(v.cam[2], 6.0, "farthest zoom");
    }
}

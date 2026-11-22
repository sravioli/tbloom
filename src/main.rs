// Fullscreen TUI renderer for a 3D autumn bouquet. Drag with the mouse to rotate, q to quit.
mod engine;
mod scene;

use engine::input::{self, Event, Key};
use engine::math3d::pose;
use engine::present::{self, Present};
use engine::raster::{self, Framebuffer};
use engine::term;
use std::io::Write;

const USAGE: &str = "\
usage: tbloom [--model FILE.fbx] [--nerd | --ascii] [--no-color] [--aa] [--snapshot WxH]
  --model FILE    show a binary FBX model (textures looked up next to it or in ../textures)
  --nerd          sextant glyphs (2x3 px per cell); needs a font with Legacy Computing blocks
  --ascii         plain text: ASCII luminance ramp (still colored unless --no-color)
  --no-color      no color escapes: luminance ramp (or dithered sextants with --nerd)
  --aa            anti-aliasing: redraw a supersampled frame once input goes quiet
  --snapshot WxH  print one W x H frame to stdout and exit
controls (turntable; each rotates about one axis only):
  drag left/right, a/d, left/right      turn the bouquet about the vertical axis of its stems
  drag up/down, w/s, up/down            view from higher / lower
  shift/alt/ctrl+drag or right-drag, A/D (shift)   lean it sideways
  mouse wheel zooms, r resets the view, q or Ctrl-C quits";

#[derive(Default)]
struct Opt {
    nerd: bool,
    ascii: bool,
    no_color: bool,
    aa: bool,
    model: Option<String>,
    snap: Option<(usize, usize)>,
}

/// View to start from (and reset to): turntable angles (az, elev, lean) and eye offset.
#[derive(Clone, Copy)]
struct Pose {
    ang: [f64; 3],
    cam: [f64; 3],
}

fn usage_exit(msg: &str) -> ! {
    eprintln!("{msg}{USAGE}");
    std::process::exit(2)
}

fn parse_args() -> Opt {
    let mut opt = Opt::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--nerd" => opt.nerd = true,
            "--ascii" => opt.ascii = true,
            "--no-color" => opt.no_color = true,
            "--aa" => opt.aa = true,
            "--model" => opt.model = Some(args.next().unwrap_or_else(|| usage_exit(""))),
            "--snapshot" => {
                let s = args.next().unwrap_or_default();
                let wh = s
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
                opt.snap = Some(
                    wh.filter(|&(w, h)| w > 0 && h > 0)
                        .unwrap_or_else(|| usage_exit("")),
                );
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0)
            }
            _ => usage_exit(&format!("unknown option: {a}\n")),
        }
    }
    if opt.nerd && opt.ascii {
        eprintln!("--nerd and --ascii are exclusive");
        std::process::exit(2)
    }
    opt
}

/// Framebuffers of the same output size: a fast one for interaction and, with --aa, a
/// supersampled (anti-aliased) one drawn once the input goes quiet.
fn setup(opt: &Opt, cols: usize, rows: usize) -> (Framebuffer, Option<Framebuffer>, Present) {
    let (w, h, pw, ph) = if opt.nerd {
        (cols * 2, rows * 3, 0.5, 2.0 / 3.0)
    } else {
        (cols, rows * 2, 1.0, 1.0)
    };
    let fine = opt
        .aa
        .then(|| Framebuffer::new(w, h, pw, ph, raster::ss_for(w, h)));
    (
        Framebuffer::new(w, h, pw, ph, 1),
        fine,
        Present::new(cols, rows, opt.nerd, !opt.no_color, opt.ascii),
    )
}

const STEP: f64 = 0.12;
/// key -> (angle index: 0 az, 1 elev, 2 lean; step); uppercase A/D (shift) is the third axis
fn key_step(k: Key) -> Option<(usize, f64)> {
    Some(match k {
        Key::Byte(b'a') | Key::Left => (0, -STEP),
        Key::Byte(b'd') | Key::Right => (0, STEP),
        Key::Byte(b'w' | b'W') | Key::Up => (1, -STEP),
        Key::Byte(b's' | b'S') | Key::Down => (1, STEP),
        Key::Byte(b'A') => (2, STEP),
        Key::Byte(b'D') => (2, -STEP),
        _ => return None,
    })
}

fn run(opt: &Opt, scene: &mut scene::Scene, start: Pose) {
    let zoom_min = scene.radius + 0.15 - raster::CAMZ;
    let Pose { mut ang, mut cam } = start;
    let mut rot = pose(ang);
    let (mut cols, mut rows) = (0, 0);
    let mut bufs = None;
    let (mut dirty, mut pending, mut drag) = (true, Vec::new(), None::<(i64, i64)>);
    let (mut idle, mut refined) = (0, false);
    let mut out = std::io::stdout();
    loop {
        let (c, r) = term::size();
        if c != cols || r != rows {
            (cols, rows) = (c, r);
            bufs = Some(setup(opt, cols, rows));
            let _ = out.write_all(b"\x1b[2J");
            dirty = true;
        }
        let (fb, fine, st) = bufs.as_mut().unwrap();
        let mut draw = |target: &mut Framebuffer, rot: &[f64; 9]| {
            raster::render(target, &mut scene.items, rot, cam);
            // synchronized output (mode 2026): the terminal shows the frame at once, no tearing
            let frame = format!(
                "\x1b[?2026h{}\x1b[?2026l",
                present::frame(st, target, false)
            );
            let _ = out.write_all(frame.as_bytes());
            let _ = out.flush();
        };
        if dirty {
            rot = pose(ang);
            draw(fb, &rot);
            (dirty, refined, idle) = (false, false, 0);
        } else if let Some(fine) = fine.as_mut().filter(|_| !refined && idle >= 8) {
            // ~130 ms without input: show the smooth frame
            draw(fine, &rot);
            refined = true;
        }
        let data = match term::read(16) {
            term::Input::Closed => return, // terminal went away
            term::Input::Timeout => {
                idle += 1;
                continue;
            }
            term::Input::Data(d) => d,
        };
        idle = 0;
        pending.extend(data);
        let (events, rest) = input::parse(&pending);
        pending = rest;
        for e in events {
            match e {
                Event::Key(Key::Byte(b'q' | 3)) => return,
                Event::Key(k) => {
                    if let Some((i, step)) = key_step(k) {
                        ang[i] += step;
                        dirty = true;
                    } else if k == Key::Byte(b'r') || k == Key::Byte(b'R') {
                        (ang, cam, dirty) = (start.ang, start.cam, true);
                    }
                }
                Event::Mouse { b, down: true, .. } if (64..128).contains(&b) => {
                    // wheel: 64 up = zoom in, 65 down = zoom out. The eye (at CAMZ + zoom) must stay
                    // outside the scene's radius, plus the 0.1 near clip.
                    cam[2] = (cam[2] + if b % 2 == 0 { -0.25 } else { 0.25 })
                        .min(6.0)
                        .max(zoom_min);
                    dirty = true;
                }
                Event::Mouse { b, x, y, down } if b < 64 => {
                    // SGR button code: bits 0-1 button, 4 shift, 8 alt, 16 ctrl, 32 motion
                    let (btn, motion) = (b % 4, b % 64 >= 32);
                    let alt = btn == 2 || (b / 4) % 8 != 0;
                    if (btn == 0 || btn == 2) && down && !motion {
                        drag = Some((x, y));
                    } else if let Some((px, py)) = drag.filter(|_| down && motion) {
                        let (dx, dy) = ((x - px) as f64, (y - py) as f64);
                        if alt {
                            ang[2] -= dx * 0.06;
                        } else {
                            ang[0] += dx * 0.06;
                            ang[1] += dy * 0.12;
                        }
                        drag = Some((x, y));
                        dirty = true;
                    } else if !down {
                        drag = None;
                    }
                }
                Event::Mouse { .. } => {}
            }
        }
    }
}

fn main() {
    let opt = parse_args();
    let (mut scene, start) = match &opt.model {
        Some(path) => match scene::model::load(path) {
            // body fills the view; wheel out for the rest
            Ok(s) => (
                s,
                Pose {
                    ang: [0.0, 0.35, 0.0],
                    cam: [0.0, 0.0, -1.6],
                },
            ),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1)
            }
        },
        // start pose chosen by the user: flowers facing the viewer, paper cone and ribbon below
        // (fitted to the user's --nerd screenshot)
        None => (
            scene::bouquet::build(),
            Pose {
                ang: [-0.079, 0.756, 0.393],
                cam: [0.0, 0.0, -0.64],
            },
        ),
    };

    if let Some((w, h)) = opt.snap {
        let (fast, fine, mut st) = setup(&opt, w, h);
        let mut fb = fine.unwrap_or(fast);
        raster::render(&mut fb, &mut scene.items, &pose(start.ang), start.cam);
        let _ = std::io::stdout().write_all(present::frame(&mut st, &fb, true).as_bytes());
        return;
    }

    if let Err(e) = term::init() {
        eprintln!("{e}");
        std::process::exit(1)
    }
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        term::restore(); // leave the alternate screen first, or the message is lost with it
        hook(info)
    }));
    run(&opt, &mut scene, start);
    term::restore();
}

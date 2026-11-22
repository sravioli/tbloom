// Fullscreen TUI renderer for a 3D bouquet (or an FBX model). Drag with the mouse to rotate, q to quit.
mod cli;
mod engine;
mod scene;
mod view;

use clap::Parser;
use cli::Opt;
use engine::math3d::pose;
use engine::present::{self, Present};
use engine::raster::{self, Framebuffer};
use engine::term;
use std::io::Write;
use view::{Pose, View};

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
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

/// Quit on q or Ctrl-C.
fn quits(e: &term::Event) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};
    matches!(e, term::Event::Key(k) if k.code == KeyCode::Char('q')
        || (k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL)))
}

fn run(opt: &Opt, scene: &mut scene::Scene, start: Pose) {
    let mut view = View::new(start, scene.radius);
    let (mut cols, mut rows) = (0, 0);
    let mut bufs = None;
    let mut dirty = true;
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
        let (rot, cam, label) = (view.rot, view.cam, view.label());
        let mut draw = |target: &mut Framebuffer| {
            raster::render(target, &mut scene.items, &rot, cam);
            // the axis label over the bottom-left corner, redrawn with every frame
            let color = if opt.no_color {
                ""
            } else {
                "\x1b[38;2;170;170;170;48;2;0;0;0m"
            };
            // synchronized output (mode 2026): the terminal shows the frame at once, no tearing
            let frame = format!(
                "\x1b[?2026h{}\x1b[{rows};1H{color}{label}\x1b[0m\x1b[?2026l",
                present::frame(st, target, false)
            );
            let _ = out.write_all(frame.as_bytes());
            let _ = out.flush();
        };
        if dirty {
            draw(fb);
            (dirty, refined, idle) = (false, false, 0);
        } else if let Some(fine) = fine.as_mut().filter(|_| !refined && idle >= 8) {
            // ~130 ms without input: show the smooth frame
            draw(fine);
            refined = true;
        }
        match term::read(16) {
            term::Input::Closed => return,
            term::Input::Timeout => idle += 1,
            term::Input::Events(events) => {
                for e in &events {
                    if quits(e) {
                        return;
                    }
                    dirty |= view.apply(e);
                }
                // mouse moves without a button don't count as input: the smooth frame still comes
                if dirty {
                    idle = 0;
                }
            }
        }
    }
}

fn main() {
    let opt = Opt::parse();
    let (mut scene, start) = match &opt.model {
        // body fills the view; wheel out for the rest
        Some(path) => (
            scene::model::load(path).unwrap_or_else(|e| die(&e)),
            Pose {
                ang: [0.0, 0.35, 0.0],
                cam: [0.0, 0.0, -1.6],
            },
        ),
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

    term::init().unwrap_or_else(|e| die(&e));
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        term::restore(); // leave the alternate screen first, or the message is lost with it
        hook(info)
    }));
    run(&opt, &mut scene, start);
    term::restore();
}

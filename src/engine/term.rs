// Terminal control: raw mode, size, poll/read on stdin.
use std::io::Write;
use std::sync::Mutex;

static SAVED: Mutex<Option<libc::termios>> = Mutex::new(None);

pub fn size() -> (usize, usize) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) } == 0 && ws.ws_col > 0 {
        return (ws.ws_col as usize, ws.ws_row as usize);
    }
    (80, 24)
}

pub fn init() -> Result<(), String> {
    let mut t: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(0, &mut t) } != 0 {
        return Err("stdin is not a terminal (use --snapshot)".into());
    }
    let mut raw = t;
    unsafe {
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
    }
    *SAVED.lock().unwrap() = Some(t);
    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?1002h\x1b[?1006h\x1b[2J");
    let _ = out.flush();
    Ok(())
}

/// Leave raw mode and the alternate screen. Safe to call more than once (panic hook + exit).
pub fn restore() {
    let Some(t) = SAVED.lock().map_or(None, |mut s| s.take()) else {
        return;
    };
    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b[0m\x1b[?1002l\x1b[?1006l\x1b[?25h\x1b[?1049l");
    let _ = out.flush();
    unsafe { libc::tcsetattr(0, libc::TCSANOW, &t) };
}

pub enum Input {
    Data(Vec<u8>),
    Timeout,
    /// terminal gone: poll keeps firing but read returns 0
    Closed,
}

/// Wait up to ms for input.
pub fn read(ms: i32) -> Input {
    let mut pfd = libc::pollfd {
        fd: 0,
        events: libc::POLLIN,
        revents: 0,
    };
    if unsafe { libc::poll(&mut pfd, 1, ms) } > 0 {
        let mut buf = [0u8; 4096];
        let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
        if n > 0 {
            return Input::Data(buf[..n as usize].to_vec());
        }
        return Input::Closed;
    }
    Input::Timeout // also on EINTR (window resize): the caller re-checks the size
}

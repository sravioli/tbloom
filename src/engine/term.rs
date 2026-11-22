// Terminal control on crossterm: raw mode, alternate screen, mouse capture, input events.
use crossterm::{cursor, event, execute, style, terminal};
use std::io::stdout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub use crossterm::event::Event;

/// Set while the terminal is in raw mode, so restore() runs once (panic hook + exit).
static ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn size() -> (usize, usize) {
    // some ptys report 0 x 0
    match terminal::size() {
        Ok((c, r)) if c > 0 && r > 0 => (c as usize, r as usize),
        _ => (80, 24),
    }
}

pub fn init() -> Result<(), String> {
    terminal::enable_raw_mode()
        .map_err(|e| format!("cannot use the terminal: {e} (use --snapshot)"))?;
    ACTIVE.store(true, Ordering::SeqCst);
    execute!(
        stdout(),
        terminal::EnterAlternateScreen,
        cursor::Hide,
        event::EnableMouseCapture,
        terminal::Clear(terminal::ClearType::All)
    )
    .map_err(|e| e.to_string())
}

/// Leave raw mode and the alternate screen. Safe to call more than once.
pub fn restore() {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let _ = execute!(
        stdout(),
        style::ResetColor,
        event::DisableMouseCapture,
        cursor::Show,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}

pub enum Input {
    /// everything that arrived: handled together so a fast drag draws once, not per event
    Events(Vec<Event>),
    Timeout,
    /// the terminal went away
    Closed,
}

/// Wait up to ms for input, then take all that is queued.
pub fn read(ms: u64) -> Input {
    let ready = |ms| event::poll(Duration::from_millis(ms));
    match ready(ms) {
        Ok(false) => return Input::Timeout,
        Err(_) => return Input::Closed,
        Ok(true) => {}
    }
    let mut events = Vec::new();
    while let Ok(true) = ready(0) {
        match event::read() {
            Ok(e) => events.push(e),
            Err(_) => return Input::Closed,
        }
    }
    Input::Events(events)
}

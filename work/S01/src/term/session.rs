//! Owns the live terminal: raw mode, alternate screen, cursor visibility,
//! and the guarantees that all three are restored on exit, panic or signal.

use crossterm::terminal;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static TERMINATE: AtomicBool = AtomicBool::new(false);

pub struct Session {
    _private: (),
}

impl Session {
    /// Enter raw mode and the alternate screen. Fails with an actionable
    /// message when stdout is not a terminal (e.g. redirected to a file).
    pub fn open() -> Result<Session, String> {
        if !io::stdout().is_terminal() {
            return Err(
                "stdout is not a terminal. Run lcars inside a terminal emulator, or use \
                        `--snapshot WxH` to render one frame as text."
                    .into(),
            );
        }
        if !io::stdin().is_terminal() {
            return Err(
                "stdin is not a terminal, so keys cannot be read. Run lcars directly in a \
                        terminal (not through a pipe)."
                    .into(),
            );
        }
        terminal::enable_raw_mode().map_err(|e| format!("could not enable raw mode: {e}"))?;
        let mut out = io::stdout();
        // Alternate screen, hide cursor, clear.
        let _ = out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[2J");
        let _ = out.flush();
        ACTIVE.store(true, Ordering::SeqCst);
        install_handlers();
        Ok(Session { _private: () })
    }

    pub fn size() -> (usize, usize) {
        terminal::size()
            .map(|(w, h)| (w as usize, h as usize))
            .unwrap_or((80, 24))
    }

    /// True once SIGTERM/SIGHUP/SIGINT has been delivered.
    pub fn terminate_requested() -> bool {
        TERMINATE.load(Ordering::SeqCst)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        restore();
    }
}

/// Idempotent restore; safe to call from the panic hook and from Drop.
pub fn restore() {
    if ACTIVE.swap(false, Ordering::SeqCst) {
        let mut out = io::stdout();
        let _ = out.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
        let _ = out.flush();
        let _ = terminal::disable_raw_mode();
    }
}

extern "C" fn on_signal(_sig: libc::c_int) {
    // Only an atomic store: the main loop notices and unwinds normally so
    // the terminal is restored through the usual path.
    TERMINATE.store(true, Ordering::SeqCst);
}

fn install_handlers() {
    // Raw mode turns Ctrl-C into a key event, but SIGTERM/SIGHUP arrive
    // asynchronously (kill, closed window) and must still leave the terminal sane.
    unsafe {
        libc::signal(
            libc::SIGTERM,
            on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGHUP,
            on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
        );
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
}

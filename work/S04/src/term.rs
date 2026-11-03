use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static QUIT: AtomicBool = AtomicBool::new(false);
static RESIZE: AtomicBool = AtomicBool::new(false);
static ENTERED: AtomicBool = AtomicBool::new(false);
static ORIG: Mutex<Option<libc::termios>> = Mutex::new(None);

extern "C" fn on_signal(sig: libc::c_int) {
    if sig == libc::SIGWINCH {
        RESIZE.store(true, Ordering::Relaxed);
    } else {
        QUIT.store(true, Ordering::Relaxed);
    }
}

pub struct Term {
    _private: (),
}

impl Term {
    pub fn enter() -> Result<Self, String> {
        if !isatty(libc::STDOUT_FILENO) || !isatty(libc::STDIN_FILENO) {
            return Err(
                "stdout is not a terminal. Run lcars-monitor in an interactive terminal.".into(),
            );
        }
        let mut orig: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut orig) } != 0 {
            return Err(format!(
                "could not read terminal attributes: {}. The display was not changed.",
                io::Error::last_os_error()
            ));
        }
        if ORIG.lock().map(|mut g| g.replace(orig)).is_err() {
            return Err("could not store terminal state".into());
        }
        let mut raw = orig;
        raw.c_iflag &= !(libc::IGNBRK
            | libc::BRKINT
            | libc::PARMRK
            | libc::ISTRIP
            | libc::INLCR
            | libc::IGNCR
            | libc::ICRNL
            | libc::IXON);
        raw.c_oflag &= !libc::OPOST;
        raw.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON | libc::IEXTEN);
        raw.c_cflag &= !(libc::CSIZE | libc::PARENB);
        raw.c_cflag |= libc::CS8;
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } != 0 {
            return Err(format!(
                "could not enter raw mode: {}. The display was not changed.",
                io::Error::last_os_error()
            ));
        }
        install_signals();
        let mut out = io::stdout();
        let _ = out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[2J\x1b[H");
        let _ = out.flush();
        ENTERED.store(true, Ordering::Relaxed);
        set_panic_hook();
        Ok(Self { _private: () })
    }

    pub fn size(&self) -> (u16, u16) {
        window_size().unwrap_or((80, 24))
    }

    pub fn write(&self, bytes: &[u8]) {
        let mut out = io::stdout();
        let _ = out.write_all(bytes);
        let _ = out.flush();
    }

    pub fn quit_requested(&self) -> bool {
        QUIT.load(Ordering::Relaxed)
    }

    pub fn take_resize(&self) -> bool {
        RESIZE.swap(false, Ordering::Relaxed)
    }

    pub fn poll_key(&self, timeout_ms: i32) -> Option<Key> {
        if self.quit_requested() {
            return Some(Key::Quit);
        }
        if !wait_stdin(timeout_ms) {
            return None;
        }
        let mut buf = [0u8; 16];
        let n = match io::stdin().read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => return None,
            Err(_) => return None,
        };
        if n == 0 {
            return None;
        }
        decode_key(&buf[..n])
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        restore();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Quit,
    Help,
    Pause,
    Diag,
    Tab,
}

pub fn restore() {
    if !ENTERED.swap(false, Ordering::Relaxed) {
        return;
    }
    let mut out = io::stdout();
    let _ = out.write_all(b"\x1b[0m\x1b[?7h\x1b[?25h\x1b[?1049l");
    let _ = out.flush();
    if let Ok(guard) = ORIG.lock() {
        if let Some(orig) = guard.as_ref() {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, orig);
            }
        }
    }
}

fn set_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        prev(info);
    }));
}

fn install_signals() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_signal as *const () as usize;
        sa.sa_flags = 0;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGWINCH, &sa, std::ptr::null_mut());
    }
}

fn isatty(fd: libc::c_int) -> bool {
    unsafe { libc::isatty(fd) == 1 }
}

fn window_size() -> Option<(u16, u16)> {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == -1 {
            return None;
        }
        if ws.ws_col == 0 || ws.ws_row == 0 {
            return None;
        }
        Some((ws.ws_col, ws.ws_row))
    }
}

fn wait_stdin(timeout_ms: i32) -> bool {
    unsafe {
        let mut pfd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = libc::poll(&mut pfd, 1, timeout_ms);
        rc > 0
    }
}

fn decode_key(buf: &[u8]) -> Option<Key> {
    match buf[0] {
        3 | b'q' | b'Q' => Some(Key::Quit),
        b'?' => Some(Key::Help),
        b' ' => Some(Key::Pause),
        b'p' | b'P' => Some(Key::Diag),
        b'\t' => Some(Key::Tab),
        0x1b => {
            if buf.len() == 1 {
                Some(Key::Quit)
            } else {
                None
            }
        }
        _ => None,
    }
}

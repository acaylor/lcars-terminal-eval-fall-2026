//! Terminal lifecycle: raw mode, alternate screen, signals, and guaranteed
//! restoration on every exit path (normal return, signal, panic, suspend).

use std::io::{self, Write};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

const ENTER: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[0m\x1b[H\x1b[2J";
const LEAVE: &[u8] = b"\x1b[0m\x1b[?2026l\x1b[?7h\x1b[?25h\x1b[?1049l";

/// Termios saved at startup, shared with the panic hook.
static SAVED: Mutex<Option<libc::termios>> = Mutex::new(None);
/// Write end of the self-pipe used by the signal handler.
static SIG_PIPE: AtomicI32 = AtomicI32::new(-1);

/// Explain why this environment can't host the full-screen interface.
pub fn check_tty() -> Result<(), String> {
    // SAFETY: isatty has no preconditions.
    if unsafe { libc::isatty(libc::STDOUT_FILENO) } != 1 {
        return Err("stdout is not a terminal. Run lcarsmon in an interactive terminal, or use \
                    `lcarsmon --snapshot command` to print a single frame."
            .into());
    }
    if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
        return Err("stdin is not a terminal, so keys cannot be read. Run lcarsmon interactively \
                    (not from a pipe), or use `--snapshot`."
            .into());
    }
    match std::env::var("TERM") {
        Ok(t) if t == "dumb" => Err("TERM=dumb cannot position the cursor. Set TERM to a real terminal type \
                                     (e.g. xterm-256color) or use `--snapshot`."
            .into()),
        _ => Ok(()),
    }
}

pub fn size() -> (u16, u16) {
    // SAFETY: zeroed winsize is a valid out-parameter for TIOCGWINSZ.
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } == 0;
    if ok && ws.ws_col > 0 && ws.ws_row > 0 {
        (ws.ws_col, ws.ws_row)
    } else {
        (80, 24)
    }
}

fn write_all_fd(bytes: &[u8]) -> io::Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(bytes)?;
    out.flush()
}

fn apply_raw(saved: &libc::termios) -> io::Result<()> {
    let mut raw = *saved;
    raw.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
    raw.c_oflag &= !libc::OPOST;
    raw.c_cflag |= libc::CS8;
    // ISIG off: Ctrl-C / Ctrl-Z arrive as bytes and are handled in-loop, so
    // the terminal is always restored before we stop or exit.
    raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
    raw.c_cc[libc::VMIN] = 0;
    raw.c_cc[libc::VTIME] = 0;
    // SAFETY: valid termios pointer for stdin.
    if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Restore the saved terminal state. Safe to call repeatedly and from the
/// panic hook (it never blocks on the mutex).
pub fn restore() {
    if let Ok(guard) = SAVED.try_lock() {
        if let Some(t) = guard.as_ref() {
            // SAFETY: restoring a termios obtained from tcgetattr.
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, t) };
        }
    }
    let _ = write_all_fd(LEAVE);
}

/// RAII guard for raw mode + alternate screen.
pub struct Terminal {
    active: bool,
}

impl Terminal {
    pub fn enter() -> io::Result<Terminal> {
        // SAFETY: zeroed termios is a valid out-parameter.
        let mut t: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut t) } != 0 {
            return Err(io::Error::last_os_error());
        }
        *SAVED.lock().unwrap_or_else(|e| e.into_inner()) = Some(t);
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            prev(info);
        }));
        apply_raw(&t)?;
        write_all_fd(ENTER)?;
        Ok(Terminal { active: true })
    }

    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        write_all_fd(bytes)
    }

    /// Ctrl-Z / SIGTSTP: hand the terminal back, stop, and re-enter on SIGCONT.
    pub fn suspend(&mut self) -> io::Result<()> {
        restore();
        // SAFETY: temporarily use the default action so raise() really stops us.
        unsafe {
            libc::signal(libc::SIGTSTP, libc::SIG_DFL);
            libc::raise(libc::SIGTSTP);
        }
        // Execution resumes here after SIGCONT.
        install_handler(libc::SIGTSTP);
        let saved = SAVED.lock().unwrap_or_else(|e| e.into_inner()).as_ref().copied();
        if let Some(t) = saved {
            apply_raw(&t)?;
        }
        write_all_fd(ENTER)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if self.active {
            restore();
            self.active = false;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Resize,
    Terminate,
    Stop,
    Continue,
}

extern "C" fn on_signal(sig: libc::c_int) {
    let fd = SIG_PIPE.load(Ordering::Relaxed);
    if fd >= 0 {
        let b = sig as u8;
        // SAFETY: write(2) is async-signal-safe; a full pipe just drops the byte.
        unsafe { libc::write(fd, &b as *const u8 as *const libc::c_void, 1) };
    }
}

fn install_handler(sig: libc::c_int) {
    // SAFETY: installing a handler that only calls async-signal-safe write().
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_signal as *const () as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = libc::SA_RESTART;
        libc::sigaction(sig, &sa, std::ptr::null_mut());
    }
}

/// Self-pipe signal delivery, so signals become ordinary poll() events.
pub struct Signals {
    read_fd: libc::c_int,
}

impl Signals {
    pub fn install() -> io::Result<Signals> {
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: fds has room for two descriptors.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        for fd in fds {
            unsafe {
                libc::fcntl(fd, libc::F_SETFL, libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK);
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
        }
        SIG_PIPE.store(fds[1], Ordering::Relaxed);
        for sig in [libc::SIGWINCH, libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT, libc::SIGTSTP, libc::SIGCONT] {
            install_handler(sig);
        }
        Ok(Signals { read_fd: fds[0] })
    }

    pub fn fd(&self) -> libc::c_int {
        self.read_fd
    }

    pub fn drain(&self) -> Vec<Signal> {
        let mut buf = [0u8; 64];
        let mut out = Vec::new();
        loop {
            // SAFETY: reading into a valid buffer from our non-blocking pipe.
            let n = unsafe { libc::read(self.read_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n <= 0 {
                break;
            }
            for &b in &buf[..n as usize] {
                let s = b as libc::c_int;
                out.push(match s {
                    libc::SIGWINCH => Signal::Resize,
                    libc::SIGTSTP => Signal::Stop,
                    libc::SIGCONT => Signal::Continue,
                    _ => Signal::Terminate,
                });
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ready {
    pub input: bool,
    /// The terminal went away (e.g. window closed without SIGHUP).
    pub hangup: bool,
    pub signal: bool,
}

/// Wait for input, hangup, or a signal, at most `timeout_ms`.
pub fn wait(sig_fd: libc::c_int, timeout_ms: i32) -> Ready {
    let mut fds =
        [libc::pollfd { fd: libc::STDIN_FILENO, events: libc::POLLIN, revents: 0 }, libc::pollfd { fd: sig_fd, events: libc::POLLIN, revents: 0 }];
    // SAFETY: fds is a valid array of two pollfd structs.
    let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout_ms.max(0)) };
    if n <= 0 {
        // Timeout, or EINTR from a signal whose byte is already in the pipe.
        return Ready { signal: n < 0, ..Ready::default() };
    }
    Ready {
        input: fds[0].revents & libc::POLLIN != 0,
        hangup: fds[0].revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0,
        signal: fds[1].revents & libc::POLLIN != 0,
    }
}

/// Read pending input without blocking (VMIN=0); returns bytes read.
pub fn read_input(buf: &mut [u8]) -> io::Result<usize> {
    // SAFETY: reading into a valid buffer.
    let n = unsafe { libc::read(libc::STDIN_FILENO, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
    if n >= 0 {
        Ok(n as usize)
    } else {
        let e = io::Error::last_os_error();
        match e.kind() {
            io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => Ok(0),
            _ => Err(e),
        }
    }
}

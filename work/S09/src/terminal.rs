use std::io::{self, Write};
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, Ordering};

static TERMINATE_REQUESTED: AtomicBool = AtomicBool::new(false);
static RESIZE_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_termination_signal(_sig: libc::c_int) {
    TERMINATE_REQUESTED.store(true, Ordering::SeqCst);
}

extern "C" fn handle_winch_signal(_sig: libc::c_int) {
    RESIZE_REQUESTED.store(true, Ordering::SeqCst);
}

/// High-level user input event decoded from raw terminal bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Quit,
    ToggleHelp,
    TogglePause,
    ToggleProcesses,
    CycleViewOrFocus,
    CycleColorMode,
    ToggleGlyphMode,
    Resize,
    None,
}

/// Decodes a slice of raw terminal bytes into an `InputEvent`.
/// Exposed publicly so input decoding can be unit-tested without a live TTY.
pub fn decode_input_bytes(bytes: &[u8]) -> InputEvent {
    if bytes.is_empty() {
        return InputEvent::None;
    }
    match bytes[0] {
        b'q' | b'Q' | 0x03 | 0x04 => InputEvent::Quit, // q, Q, Ctrl-C, Ctrl-D
        0x1b => {
            // Standalone Esc (not an arrow/function escape sequence like `\x1b[A`)
            if bytes.len() == 1 {
                InputEvent::Quit
            } else {
                InputEvent::None
            }
        }
        b'?' => InputEvent::ToggleHelp,
        b' ' => InputEvent::TogglePause,
        b'p' | b'P' => InputEvent::ToggleProcesses,
        b'\t' => InputEvent::CycleViewOrFocus,
        b'c' | b'C' => InputEvent::CycleColorMode,
        b'u' | b'U' => InputEvent::ToggleGlyphMode,
        _ => InputEvent::None,
    }
}

/// Queries the current terminal dimensions `(cols, rows)` via `ioctl(TIOCGWINSZ)`.
pub fn query_terminal_size() -> (usize, usize) {
    unsafe {
        let mut ws = MaybeUninit::<libc::winsize>::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, ws.as_mut_ptr()) == 0 {
            let ws = ws.assume_init();
            if ws.ws_col > 0 && ws.ws_row > 0 {
                return (ws.ws_col as usize, ws.ws_row as usize);
            }
        }
    }
    (80, 24)
}

/// RAII guard that manages alternate screen, cursor visibility, raw `termios`, and signal handlers,
/// guaranteeing clean terminal restoration on normal exit, signal interruption, or panic.
pub struct TerminalSession {
    orig_termios: libc::termios,
    active: bool,
}

impl TerminalSession {
    pub fn enter() -> Result<Self, String> {
        unsafe {
            if libc::isatty(libc::STDIN_FILENO) == 0 || libc::isatty(libc::STDOUT_FILENO) == 0 {
                return Err(
                    "Standard input/output is not an interactive terminal (TTY). \
                     Run interactively in a terminal emulator, or pass `--capture-primary` / `--capture-secondary` for non-interactive output."
                        .to_string(),
                );
            }

            let mut orig = MaybeUninit::<libc::termios>::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, orig.as_mut_ptr()) != 0 {
                return Err(format!(
                    "Failed to read terminal attributes (tcgetattr): {}",
                    io::Error::last_os_error()
                ));
            }
            let orig_termios = orig.assume_init();
            let mut raw = orig_termios;

            // Disable canonical line buffering, echo, and software flow control
            raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ECHONL | libc::IEXTEN);
            raw.c_iflag &= !(libc::IXON | libc::ICRNL | libc::BRKINT | libc::INPCK | libc::ISTRIP);
            raw.c_cflag |= libc::CS8;
            raw.c_cc[libc::VMIN] = 0;
            raw.c_cc[libc::VTIME] = 0;

            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) != 0 {
                return Err(format!(
                    "Failed to enable non-canonical terminal mode (tcsetattr): {}",
                    io::Error::last_os_error()
                ));
            }

            install_signal_handlers();

            // Install panic hook to ensure terminal state is restored even if a panic occurs
            let orig_copy = orig_termios;
            let prev_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let _ = libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &orig_copy);
                let mut out = io::stdout();
                let _ = out.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
                let _ = out.flush();
                prev_hook(info);
            }));

            let mut stdout = io::stdout();
            stdout
                .write_all(b"\x1b[?1049h\x1b[?25l\x1b[0m\x1b[2J\x1b[H")
                .map_err(|e| format!("Failed to initialize terminal screen: {}", e))?;
            stdout
                .flush()
                .map_err(|e| format!("Failed to flush terminal screen: {}", e))?;

            Ok(Self {
                orig_termios,
                active: true,
            })
        }
    }

    pub fn should_terminate(&self) -> bool {
        TERMINATE_REQUESTED.load(Ordering::SeqCst)
    }

    pub fn take_resize_event(&self) -> bool {
        RESIZE_REQUESTED.swap(false, Ordering::SeqCst)
    }

    /// Waits up to `timeout_ms` for keyboard input or signal wakeup without spinning CPU.
    pub fn poll_event(&self, timeout_ms: u64) -> InputEvent {
        if self.should_terminate() {
            return InputEvent::Quit;
        }
        if self.take_resize_event() {
            return InputEvent::Resize;
        }

        unsafe {
            let mut pfd = libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            };
            let ret = libc::poll(&mut pfd, 1, timeout_ms.min(1000) as libc::c_int);
            if self.should_terminate() {
                return InputEvent::Quit;
            }
            if self.take_resize_event() {
                return InputEvent::Resize;
            }
            if ret > 0 && (pfd.revents & libc::POLLIN) != 0 {
                let mut buf = [0u8; 16];
                let n = libc::read(
                    libc::STDIN_FILENO,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                );
                if n > 0 {
                    return decode_input_bytes(&buf[..n as usize]);
                }
            }
        }
        InputEvent::None
    }

    pub fn restore(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        let mut stdout = io::stdout();
        let _ = stdout.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
        let _ = stdout.flush();
        unsafe {
            let _ = libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.orig_termios);
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.restore();
    }
}

unsafe fn install_signal_handlers() {
    TERMINATE_REQUESTED.store(false, Ordering::SeqCst);
    RESIZE_REQUESTED.store(false, Ordering::SeqCst);

    let mut sa_term = MaybeUninit::<libc::sigaction>::zeroed().assume_init();
    sa_term.sa_sigaction = handle_termination_signal as *const () as usize;
    libc::sigemptyset(&mut sa_term.sa_mask);
    sa_term.sa_flags = 0;

    for &sig in &[libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT] {
        let _ = libc::sigaction(sig, &sa_term, std::ptr::null_mut());
    }

    let mut sa_winch = MaybeUninit::<libc::sigaction>::zeroed().assume_init();
    sa_winch.sa_sigaction = handle_winch_signal as *const () as usize;
    libc::sigemptyset(&mut sa_winch.sa_mask);
    sa_winch.sa_flags = libc::SA_RESTART;
    let _ = libc::sigaction(libc::SIGWINCH, &sa_winch, std::ptr::null_mut());
}

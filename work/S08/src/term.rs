use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, DisableLineWrap, EnableLineWrap,
    EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
};
use crossterm::{execute, terminal};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static SIGNAL: AtomicI32 = AtomicI32::new(0);
static QUIET_PANIC: AtomicBool = AtomicBool::new(false);

pub struct Terminal {
    active: bool,
}

impl Terminal {
    pub fn enter() -> io::Result<Self> {
        if !io::stdin().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "stdin is not a terminal. Run lcars-mon from an interactive terminal, or use --snapshot for a one-frame capture.",
            ));
        }
        if !io::stdout().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "stdout is redirected. Use --snapshot --plain for text output, or run in an interactive terminal.",
            ));
        }
        enable_raw_mode()?;
        ACTIVE.store(true, Ordering::SeqCst);
        let mut out = io::stdout();
        let result = execute!(
            out,
            EnterAlternateScreen,
            Hide,
            DisableLineWrap,
            Clear(ClearType::All),
            SetTitle("LCARS 47 // Systems Monitor")
        )
        .and_then(|_| out.flush());
        if let Err(err) = result {
            emergency_restore();
            return Err(err);
        }
        Ok(Self { active: true })
    }

    pub fn restore(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        emergency_restore();
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.restore();
    }
}

fn emergency_restore() {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let _ = disable_raw_mode();
    let mut out = io::stdout();
    let _ = execute!(
        out,
        Show,
        EnableLineWrap,
        LeaveAlternateScreen,
        SetTitle("Terminal")
    );
    let _ = out.flush();
}

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if QUIET_PANIC.load(Ordering::SeqCst) {
            return;
        }
        emergency_restore();
        previous(info);
    }));
}

pub fn install_signal_handlers() {
    unsafe {
        libc::signal(
            libc::SIGINT,
            handle_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            handle_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGHUP,
            handle_signal as *const () as libc::sighandler_t,
        );
    }
}

extern "C" fn handle_signal(signum: libc::c_int) {
    SIGNAL.store(signum, Ordering::SeqCst);
}

pub fn set_quiet_panic(quiet: bool) {
    QUIET_PANIC.store(quiet, Ordering::SeqCst);
}

pub fn take_signal() -> Option<i32> {
    let sig = SIGNAL.swap(0, Ordering::SeqCst);
    if sig == 0 {
        None
    } else {
        Some(sig)
    }
}

pub fn size() -> (i32, i32) {
    match terminal::size() {
        Ok((w, h)) => (w as i32, h as i32),
        Err(_) => (80, 24),
    }
}

pub enum KeyAction {
    None,
    Quit,
    Escape,
    Help,
    Pause,
    Processes,
    Cycle,
}

pub fn key_action(event: &Event) -> KeyAction {
    let Event::Key(KeyEvent {
        code,
        modifiers,
        kind,
        ..
    }) = event
    else {
        return KeyAction::None;
    };
    if *kind != KeyEventKind::Press {
        return KeyAction::None;
    }
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') if !modifiers.contains(KeyModifiers::CONTROL) => {
            KeyAction::Quit
        }
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => KeyAction::Quit,
        KeyCode::Esc => KeyAction::Escape,
        KeyCode::Char('?') => KeyAction::Help,
        KeyCode::Char(' ') => KeyAction::Pause,
        KeyCode::Char('p') | KeyCode::Char('P') => KeyAction::Processes,
        KeyCode::Tab => KeyAction::Cycle,
        _ => KeyAction::None,
    }
}

pub fn poll_event(timeout: std::time::Duration) -> io::Result<Option<Event>> {
    if event::poll(timeout)? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(code, modifiers))
    }

    #[test]
    fn control_keys_are_recognized() {
        assert!(matches!(
            key_action(&key(KeyCode::Char('q'), KeyModifiers::NONE)),
            KeyAction::Quit
        ));
        assert!(matches!(
            key_action(&key(KeyCode::Esc, KeyModifiers::NONE)),
            KeyAction::Escape
        ));
        assert!(matches!(
            key_action(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            KeyAction::Quit
        ));
        assert!(matches!(
            key_action(&key(KeyCode::Char('?'), KeyModifiers::NONE)),
            KeyAction::Help
        ));
        assert!(matches!(
            key_action(&key(KeyCode::Char(' '), KeyModifiers::NONE)),
            KeyAction::Pause
        ));
        assert!(matches!(
            key_action(&key(KeyCode::Char('p'), KeyModifiers::NONE)),
            KeyAction::Processes
        ));
        assert!(matches!(
            key_action(&key(KeyCode::Tab, KeyModifiers::NONE)),
            KeyAction::Cycle
        ));
    }
}

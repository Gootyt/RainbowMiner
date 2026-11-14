//! Terminal helpers: color detection and non-blocking key reading (Read-ConsoleKeyGuarded)
#![allow(dead_code)]

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static COLORS: AtomicBool = AtomicBool::new(true);
static RAW: AtomicBool = AtomicBool::new(false);

pub fn init() {
    let tty = std::io::stdout().is_terminal();
    let no_color = std::env::var_os("NO_COLOR").is_some();
    COLORS.store(tty && !no_color, Ordering::Relaxed);
    #[cfg(windows)]
    {
        // enable ANSI escape sequences on Windows 10+ consoles
        let _ = crossterm::style::force_color_output(true);
    }
}

pub fn colors_enabled() -> bool {
    COLORS.load(Ordering::Relaxed)
}

pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// A pressed key, mapped to the command letters of the main loop
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Ctrl(char),
}

/// Wait up to `timeout` for a key press. Raw mode is only switched on for the duration of the poll,
/// so normal output keeps working.
pub fn poll_key(timeout: Duration) -> Option<Key> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    if !is_interactive() {
        std::thread::sleep(timeout);
        return None;
    }
    if crossterm::terminal::enable_raw_mode().is_err() {
        std::thread::sleep(timeout);
        return None;
    }
    RAW.store(true, Ordering::Relaxed);
    let mut result = None;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let now = std::time::Instant::now();
        if now >= deadline {
            break;
        }
        match event::poll(deadline - now) {
            Ok(true) => {
                if let Ok(Event::Key(k)) = event::read() {
                    if k.kind != KeyEventKind::Press {
                        continue;
                    }
                    if let KeyCode::Char(c) = k.code {
                        if k.modifiers.contains(KeyModifiers::CONTROL) {
                            result = Some(Key::Ctrl(c.to_ascii_uppercase()));
                        } else {
                            result = Some(Key::Char(c.to_ascii_uppercase()));
                        }
                        break;
                    }
                }
            }
            _ => break,
        }
    }
    let _ = crossterm::terminal::disable_raw_mode();
    RAW.store(false, Ordering::Relaxed);
    result
}

/// restore the terminal (called on exit)
pub fn restore() {
    if RAW.load(Ordering::Relaxed) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Window title (Windows console / xterm)
pub fn set_title(title: &str) {
    if is_interactive() {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::SetTitle(title));
    }
}

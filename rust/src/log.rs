//! Write-Log port: daily log file Logs/RainbowMiner_yyyy-MM-dd.txt plus colored console output.
//! Console lines are also kept in a ring buffer, the web console (/console) reads it.
#![allow(dead_code)]

use chrono::Local;
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Verbose,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn text(&self) -> &'static str {
        match self {
            Level::Error => "ERROR:",
            Level::Warn => "WARNING:",
            Level::Info => "INFO:",
            Level::Verbose => "VERBOSE:",
            Level::Debug => "DEBUG:",
        }
    }
}

/// Log level setting from config.txt: "Debug", "Info", "Warn", "Error", "Silent"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSetting {
    Debug,
    Info,
    Warn,
    Error,
    Silent,
}

impl LogSetting {
    pub fn parse(s: &str) -> LogSetting {
        match s.trim().to_lowercase().as_str() {
            "debug" => LogSetting::Debug,
            "warn" | "warning" => LogSetting::Warn,
            "error" => LogSetting::Error,
            "silent" => LogSetting::Silent,
            _ => LogSetting::Info,
        }
    }
}

struct LogState {
    dir: PathBuf,
    setting: LogSetting,
    debug_console: bool,
    quiet_console: bool,
}

static STATE: LazyLock<RwLock<LogState>> = LazyLock::new(|| {
    RwLock::new(LogState {
        dir: PathBuf::from("Logs"),
        setting: LogSetting::Info,
        debug_console: false,
        quiet_console: false,
    })
});

static FILE_LOCK: Mutex<()> = Mutex::new(());

const CONSOLE_LINES: usize = 1000;
static CONSOLE: LazyLock<Mutex<VecDeque<String>>> = LazyLock::new(|| Mutex::new(VecDeque::with_capacity(CONSOLE_LINES)));

pub fn init(dir: PathBuf, setting: LogSetting) {
    let _ = std::fs::create_dir_all(&dir);
    let mut st = STATE.write().unwrap();
    st.dir = dir;
    st.setting = setting;
}

pub fn set_level(setting: LogSetting) {
    STATE.write().unwrap().setting = setting;
}

pub fn set_debug_console(b: bool) {
    STATE.write().unwrap().debug_console = b;
}

/// suppress console echo (used while the setup runs)
pub fn set_quiet_console(b: bool) {
    STATE.write().unwrap().quiet_console = b;
}

pub fn log_dir() -> PathBuf {
    STATE.read().unwrap().dir.clone()
}

/// Write-Log
pub fn write(level: Level, msg: &str) {
    let (dir, setting, debug_console, quiet) = {
        let st = STATE.read().unwrap();
        (st.dir.clone(), st.setting, st.debug_console, st.quiet_console)
    };

    // console: warnings and errors always, info only in debug mode (like the original)
    if !quiet {
        match level {
            Level::Error => console_colored(&format!("{} {}", level.text(), msg), Color::Red),
            Level::Warn => console_colored(&format!("{} {}", level.text(), msg), Color::Yellow),
            Level::Info if debug_console => console_colored(&format!("{} {}", level.text(), msg), Color::DarkGray),
            _ => {}
        }
    }

    let no_log = match setting {
        LogSetting::Silent => true,
        LogSetting::Info => level == Level::Debug,
        LogSetting::Warn => matches!(level, Level::Info | Level::Debug | Level::Verbose),
        LogSetting::Error => matches!(level, Level::Warn | Level::Info | Level::Debug | Level::Verbose),
        LogSetting::Debug => false,
    };
    if no_log {
        return;
    }
    let now = Local::now();
    let file = dir.join(format!("RainbowMiner_{}.txt", now.format("%Y-%m-%d")));
    let line = format!("[{}] {} {}\n", now.format("%Y-%m-%d %H:%M:%S"), level.text(), msg);
    let _g = FILE_LOCK.lock();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = f.write_all(line.as_bytes());
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Color {
    Default,
    Red,
    Yellow,
    Green,
    Cyan,
    DarkGray,
    DarkYellow,
    Magenta,
}

fn color_code(c: Color) -> &'static str {
    match c {
        Color::Default => "",
        Color::Red => "\x1b[91m",
        Color::Yellow => "\x1b[93m",
        Color::Green => "\x1b[92m",
        Color::Cyan => "\x1b[96m",
        Color::DarkGray => "\x1b[90m",
        Color::DarkYellow => "\x1b[33m",
        Color::Magenta => "\x1b[95m",
    }
}

fn push_console(line: &str) {
    let mut c = CONSOLE.lock().unwrap();
    for l in line.split('\n') {
        if c.len() >= CONSOLE_LINES {
            c.pop_front();
        }
        c.push_back(l.trim_end_matches('\r').to_string());
    }
}

/// Write-Host
pub fn console(line: &str) {
    push_console(line);
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", line);
}

/// Write-Host -ForegroundColor
pub fn console_colored(line: &str, color: Color) {
    push_console(line);
    let mut out = std::io::stdout().lock();
    if crate::util_term::colors_enabled() && !matches!(color, Color::Default) {
        let _ = writeln!(out, "{}{}\x1b[0m", color_code(color), line);
    } else {
        let _ = writeln!(out, "{}", line);
    }
}

/// Write-Host -NoNewline
pub fn console_inline(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "{}", text);
    let _ = out.flush();
}

/// Clear-Host
pub fn clear_console() {
    CONSOLE.lock().unwrap().clear();
    if crate::util_term::colors_enabled() {
        let mut out = std::io::stdout().lock();
        let _ = write!(out, "\x1b[2J\x1b[H");
        let _ = out.flush();
    }
}

/// the last console lines (for the API)
pub fn console_lines() -> Vec<String> {
    CONSOLE.lock().unwrap().iter().cloned().collect()
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::log::write($crate::log::Level::Info, &format!($($arg)*)) };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::log::write($crate::log::Level::Warn, &format!($($arg)*)) };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { $crate::log::write($crate::log::Level::Error, &format!($($arg)*)) };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => { $crate::log::write($crate::log::Level::Debug, &format!($($arg)*)) };
}

#[macro_export]
macro_rules! log_verbose {
    ($($arg:tt)*) => { $crate::log::write($crate::log::Level::Verbose, &format!($($arg)*)) };
}

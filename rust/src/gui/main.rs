//! rainbowminer-gui: the graphical user interface of RainbowMiner (Rust port).
//! It shows and controls a running engine through its API (Config "APIport", default 4000)
//! and can start the engine.
//!
//!   rainbowminer-gui [--host 127.0.0.1] [--port 4000] [--user U --password P]
//!                    [--basedir DIR] [--interval 5] [--lang hu|en] [--tab console]

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod client;
mod fetch;
mod fmt;
mod i18n;

use client::Endpoint;
use eframe::egui;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};

#[derive(Default)]
struct Args {
    host: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    password: Option<String>,
    base: Option<String>,
    interval: Option<u64>,
    lang: Option<String>,
    tab: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args::default();
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let name = v[i].trim_start_matches('-').to_lowercase();
        let (name, inline) = match name.split_once('=') {
            Some((n, x)) => (n.to_string(), Some(x.to_string())),
            None => (name, None),
        };
        if ["h", "help", "?"].contains(&name.as_str()) {
            println!("rainbowminer-gui [--host H] [--port P] [--user U] [--password PW] [--basedir DIR] [--interval S] [--lang hu|en] [--tab dashboard|devices|miners|pools|balances|benchmarks|console|settings]");
            std::process::exit(0);
        }
        let val = match inline {
            Some(x) => x,
            None => {
                i += 1;
                v.get(i).cloned().ok_or_else(|| format!("missing value for --{}", name))?
            }
        };
        match name.as_str() {
            "host" => a.host = Some(val),
            "port" | "apiport" => a.port = Some(val.parse().map_err(|_| format!("invalid port {}", val))?),
            "user" | "apiuser" => a.user = Some(val),
            "password" | "apipassword" => a.password = Some(val),
            "basedir" | "base" => a.base = Some(val),
            "interval" => a.interval = val.parse().ok(),
            "lang" => a.lang = Some(val),
            "tab" => a.tab = Some(val),
            _ => return Err(format!("unknown parameter --{}", name)),
        }
        i += 1;
    }
    Ok(a)
}

/// the RainbowMiner folder (like the engine: Data/algorithms.json marks it)
fn find_base(arg: Option<&str>) -> Option<PathBuf> {
    let ok = |p: &Path| p.join("Data").join("algorithms.json").exists();
    if let Some(b) = arg {
        return Some(PathBuf::from(b));
    }
    if let Ok(cwd) = std::env::current_dir() {
        if ok(&cwd) {
            return Some(cwd);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let mut d = exe.parent().map(|p| p.to_path_buf());
    while let Some(p) = d {
        if ok(&p) {
            return Some(p);
        }
        d = p.parent().map(|x| x.to_path_buf());
    }
    None
}

/// the engine program: next to the GUI, else in the build folder of the Rust port
fn find_engine(base: Option<&Path>) -> Option<PathBuf> {
    let name = format!("rainbowminer{}", std::env::consts::EXE_SUFFIX);
    let mut cands = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            cands.push(d.join(&name));
        }
    }
    if let Some(b) = base {
        cands.push(b.join(&name));
        for p in ["release", "debug"] {
            cands.push(b.join("rust").join("target").join(p).join(&name));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// APIport, APIauth, APIuser, APIpassword of Config/config.txt ("$..." placeholders mean the default)
fn api_config(base: Option<&Path>) -> (u16, String, String) {
    let cfg: Value = base
        .and_then(|b| std::fs::read_to_string(b.join("Config").join("config.txt")).ok())
        .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
        .unwrap_or(Value::Null);
    let val = |k: &str| {
        let x = fmt::s(fmt::get(&cfg, k));
        if x.starts_with('$') {
            String::new()
        } else {
            x.trim().to_string()
        }
    };
    let port = val("APIport").parse().ok().filter(|p| *p > 0).unwrap_or(4000);
    let auth = ["1", "yes", "true", "on"].contains(&val("APIauth").to_lowercase().as_str());
    if auth {
        (port, val("APIuser"), val("APIpassword"))
    } else {
        (port, String::new(), String::new())
    }
}

fn main() -> eframe::Result<()> {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    let base = find_base(args.base.as_deref());
    let engine_exe = find_engine(base.as_deref());
    let (port, user, password) = api_config(base.as_deref());
    let endpoint = Endpoint {
        host: args.host.unwrap_or_else(|| "127.0.0.1".into()),
        port: args.port.unwrap_or(port),
        user: args.user.unwrap_or(user),
        password: args.password.unwrap_or(password),
    };
    let lang = match args.lang.as_deref().map(|l| l.to_lowercase()) {
        Some(l) if l.starts_with("hu") => i18n::Lang::Hu,
        Some(_) => i18n::Lang::En,
        None => i18n::Lang::detect(),
    };
    use fetch::Tab;
    let tab = match args.tab.as_deref().map(|t| t.to_lowercase()).as_deref() {
        Some("devices") => Tab::Devices,
        Some("miners") => Tab::Miners,
        Some("pools") => Tab::Pools,
        Some("balances") => Tab::Balances,
        Some("benchmarks") => Tab::Benchmarks,
        Some("console") => Tab::Console,
        Some("settings") => Tab::Settings,
        _ => Tab::Dashboard,
    };
    let opts = app::Options { endpoint: endpoint.clone(), base, engine_exe, lang, interval: args.interval.unwrap_or(5), tab };
    let shared = Arc::new(fetch::Shared { snap: Mutex::new(fetch::Snapshot::default()) });
    let (tx, rx) = mpsc::channel();
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("RainbowMiner").with_inner_size([1280.0, 800.0]).with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "RainbowMiner",
        native,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let sh = shared.clone();
            std::thread::spawn(move || fetch::run(endpoint, rx, sh, move || ctx.request_repaint()));
            Ok(Box::new(app::App::new(cc, tx, shared, opts)))
        }),
    )
}

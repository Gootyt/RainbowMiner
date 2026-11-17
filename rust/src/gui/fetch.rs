//! The background thread that polls the API and the commands the GUI sends to the engine

use crate::client::Endpoint;
use serde_json::Value;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// the tabs whose data is only loaded while they are shown
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Dashboard,
    Devices,
    Miners,
    Pools,
    Balances,
    Benchmarks,
    Console,
    Settings,
}

#[derive(Debug, Default, Clone)]
pub struct Snapshot {
    pub connected: bool,
    pub error: String,
    pub version: Value,
    pub status: Value,
    pub profit: Value,
    pub devices: Value,
    pub running: Value,
    pub active: Value,
    pub pools: Value,
    pub balances: Value,
    pub benchmarks: Value,
    pub console: Value,
    pub watchdog: Value,
    pub updated: Option<Instant>,
    /// the last command and its result
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum Cmd {
    Refresh,
    Pause,
    LockMiners,
    WatchdogReset,
    UpdateBalance,
    ClearCache,
    Stop,
    SetEndpoint(Endpoint),
    SetInterval(u64),
    SetTab(Tab),
}

pub struct Shared {
    pub snap: Mutex<Snapshot>,
}

fn load(ep: &Endpoint, tab: Tab, snap: &Arc<Shared>) {
    // the status endpoint decides whether the engine is reachable
    match ep.json("/status") {
        Ok(status) => {
            let get = |p: &str| ep.json(p).unwrap_or(Value::Null);
            let mut s = Snapshot { connected: true, status, ..Default::default() };
            s.version = get("/version");
            s.profit = get("/currentprofit");
            s.devices = get("/devices");
            s.running = get("/runningminers");
            s.watchdog = get("/watchdogtimers");
            match tab {
                Tab::Miners => s.active = get("/activeminers"),
                Tab::Pools => s.pools = get("/pools"),
                Tab::Balances => s.balances = get("/balances"),
                Tab::Benchmarks => s.benchmarks = get("/minersneedingbenchmark"),
                Tab::Console => s.console = get("/console"),
                Tab::Dashboard => s.balances = get("/balances"),
                _ => {}
            }
            s.updated = Some(Instant::now());
            let mut cur = snap.snap.lock().unwrap();
            // keep the data of the tabs that were not loaded this time
            if s.active.is_null() {
                s.active = std::mem::take(&mut cur.active);
            }
            if s.pools.is_null() {
                s.pools = std::mem::take(&mut cur.pools);
            }
            if s.balances.is_null() {
                s.balances = std::mem::take(&mut cur.balances);
            }
            if s.benchmarks.is_null() {
                s.benchmarks = std::mem::take(&mut cur.benchmarks);
            }
            if s.console.is_null() {
                s.console = std::mem::take(&mut cur.console);
            }
            s.message = std::mem::take(&mut cur.message);
            *cur = s;
        }
        Err(e) => {
            let mut cur = snap.snap.lock().unwrap();
            cur.connected = false;
            cur.error = e;
            cur.updated = Some(Instant::now());
        }
    }
}

/// runs until the GUI drops the command sender
pub fn run(mut ep: Endpoint, rx: Receiver<Cmd>, shared: Arc<Shared>, repaint: impl Fn() + Send + 'static) {
    let mut interval = Duration::from_secs(5);
    let mut tab = Tab::Dashboard;
    let mut next = Instant::now();
    loop {
        let wait = next.saturating_duration_since(Instant::now());
        let cmd = match rx.recv_timeout(wait) {
            Ok(c) => Some(c),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if let Some(c) = cmd {
            let path = match &c {
                Cmd::Pause => Some("/pause"),
                Cmd::LockMiners => Some("/lockminers"),
                Cmd::WatchdogReset => Some("/watchdogreset"),
                Cmd::UpdateBalance => Some("/updatebalance"),
                Cmd::ClearCache => Some("/clearcache"),
                Cmd::Stop => Some("/stop"),
                _ => None,
            };
            match c {
                Cmd::SetEndpoint(e) => ep = e,
                Cmd::SetInterval(s) => interval = Duration::from_secs(s.max(1)),
                Cmd::SetTab(t) => tab = t,
                _ => {}
            }
            if let Some(p) = path {
                let r = ep.command(p);
                shared.snap.lock().unwrap().message = match r {
                    Ok(()) => format!("{} OK", p),
                    Err(e) => format!("{}: {}", p, e),
                };
            }
        }
        load(&ep, tab, &shared);
        repaint();
        next = Instant::now() + interval;
    }
}

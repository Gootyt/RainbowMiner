//! The egui application: status bar, controls and one tab per data set of the API

use crate::client::Endpoint;
use crate::fetch::{Cmd, Shared, Snapshot, Tab};
use crate::fmt::{self, get, list, num, opt, path, s, truthy};
use crate::i18n::{tr, Lang};
use eframe::egui::{self, Color32, RichText};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

const GREEN: Color32 = Color32::from_rgb(80, 200, 120);
const ORANGE: Color32 = Color32::from_rgb(240, 170, 60);
const RED: Color32 = Color32::from_rgb(235, 90, 80);
const ACCENT: Color32 = Color32::from_rgb(120, 170, 255);
const DIM: Color32 = Color32::from_rgb(150, 150, 160);

/// a status light
fn dot(ui: &mut egui::Ui, c: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 5.0, c);
}

pub struct Options {
    pub endpoint: Endpoint,
    pub base: Option<PathBuf>,
    pub engine_exe: Option<PathBuf>,
    pub lang: Lang,
    pub interval: u64,
    pub tab: Tab,
}

pub struct App {
    tx: Sender<Cmd>,
    shared: Arc<Shared>,
    lang: Lang,
    tab: Tab,
    // the settings form
    host: String,
    port: String,
    user: String,
    password: String,
    interval: String,
    endpoint: Endpoint,
    base: Option<PathBuf>,
    engine_exe: Option<PathBuf>,
    engine_started: Option<Instant>,
    engine_error: String,
    confirm_stop: bool,
    miner_filter: String,
    only_best: bool,
    follow_console: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, tx: Sender<Cmd>, shared: Arc<Shared>, o: Options) -> App {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        cc.egui_ctx.set_style(style);
        let _ = tx.send(Cmd::SetInterval(o.interval));
        let _ = tx.send(Cmd::SetTab(o.tab));
        App {
            tx,
            shared,
            lang: o.lang,
            tab: o.tab,
            host: o.endpoint.host.clone(),
            port: o.endpoint.port.to_string(),
            user: o.endpoint.user.clone(),
            password: o.endpoint.password.clone(),
            interval: o.interval.to_string(),
            endpoint: o.endpoint,
            base: o.base,
            engine_exe: o.engine_exe,
            engine_started: None,
            engine_error: String::new(),
            confirm_stop: false,
            miner_filter: String::new(),
            only_best: true,
            follow_console: true,
        }
    }

    fn t(&self, k: &str) -> &'static str {
        tr(self.lang, k)
    }

    fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    fn set_tab(&mut self, t: Tab) {
        if self.tab != t {
            self.tab = t;
            self.send(Cmd::SetTab(t));
        }
    }

    /// starts the engine (rainbowminer[.exe]) in the RainbowMiner folder
    fn start_engine(&mut self) {
        let Some(exe) = self.engine_exe.clone() else { return };
        let mut cmd = std::process::Command::new(&exe);
        if let Some(b) = &self.base {
            cmd.current_dir(b).arg("-basedir").arg(b);
        }
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // its own console window, like a start of RainbowMiner.exe
            cmd.creation_flags(0x0000_0010);
        }
        match cmd.spawn() {
            Ok(_) => {
                self.engine_started = Some(Instant::now());
                self.engine_error.clear();
            }
            Err(e) => self.engine_error = format!("{}: {}", exe.display(), e),
        }
    }

    fn open_url(url: &str) {
        #[cfg(windows)]
        let r = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
        #[cfg(target_os = "macos")]
        let r = std::process::Command::new("open").arg(url).spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let r = std::process::Command::new("xdg-open").arg(url).spawn();
        let _ = r;
    }

    // --------------------------------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("🌈 RainbowMiner").size(20.0).strong().color(ACCENT));
            let ver = if snap.version.is_object() { s(get(&snap.version, "Version")) } else { s(&snap.version) };
            if !ver.is_empty() {
                ui.label(RichText::new(format!("v{}", ver.trim_start_matches('v'))).color(DIM));
            }
            ui.separator();
            if snap.connected {
                dot(ui, GREEN);
                ui.label(RichText::new(self.t("connected")).color(GREEN));
                let paused = truthy(get(&snap.status, "Pause"));
                let locked = truthy(path(&snap.status, &["LockMiners", "Locked"]));
                let (txt, col) = if paused { (self.t("paused"), ORANGE) } else { (self.t("running"), GREEN) };
                ui.label(RichText::new(txt).strong().color(col));
                if locked {
                    ui.label(RichText::new(self.t("locked")).color(ORANGE));
                }
                if truthy(get(&snap.status, "IsExclusiveRun")) {
                    ui.label(RichText::new(self.t("exclusive")).color(ACCENT));
                }
            } else {
                dot(ui, RED);
                ui.label(RichText::new(self.t("disconnected")).color(RED));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let hu = self.lang == Lang::Hu;
                if ui.selectable_label(hu, "HU").clicked() {
                    self.lang = Lang::Hu;
                }
                if ui.selectable_label(!hu, "EN").clicked() {
                    self.lang = Lang::En;
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            let c = snap.connected;
            let paused = truthy(get(&snap.status, "Pause"));
            let locked = truthy(path(&snap.status, &["LockMiners", "Locked"]));
            if ui.add_enabled(c, egui::Button::new(if paused { self.t("resume") } else { self.t("pause") })).clicked() {
                self.send(Cmd::Pause);
            }
            if ui.add_enabled(c, egui::Button::new(if locked { self.t("unlock") } else { self.t("lock") })).clicked() {
                self.send(Cmd::LockMiners);
            }
            if ui.add_enabled(c, egui::Button::new(self.t("watchdogreset"))).clicked() {
                self.send(Cmd::WatchdogReset);
            }
            if ui.add_enabled(c, egui::Button::new(self.t("updatebalance"))).clicked() {
                self.send(Cmd::UpdateBalance);
            }
            if ui.add_enabled(c, egui::Button::new(self.t("clearcache"))).clicked() {
                self.send(Cmd::ClearCache);
            }
            if ui.add_enabled(c, egui::Button::new(RichText::new(self.t("stop")).color(RED))).clicked() {
                self.confirm_stop = true;
            }
            if !c && self.engine_exe.is_some() && ui.button(RichText::new(self.t("start")).color(GREEN)).clicked() {
                self.start_engine();
            }
            if ui.button(format!("⟳ {}", self.t("refresh"))).clicked() {
                self.send(Cmd::Refresh);
            }
            if c && ui.button(self.t("web_ui")).clicked() {
                Self::open_url(&self.endpoint.url("/"));
            }
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            for (t, k) in [
                (Tab::Dashboard, "dashboard"),
                (Tab::Devices, "devices"),
                (Tab::Miners, "miners"),
                (Tab::Pools, "pools"),
                (Tab::Balances, "balances"),
                (Tab::Benchmarks, "benchmarks"),
                (Tab::Console, "console"),
                (Tab::Settings, "settings"),
            ] {
                if ui.selectable_label(self.tab == t, RichText::new(self.t(k)).size(15.0)).clicked() {
                    self.set_tab(t);
                }
            }
        });
    }

    fn status_line(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.horizontal(|ui| {
            if let Some(u) = snap.updated {
                ui.label(RichText::new(format!("{} {}s", self.t("last_update"), u.elapsed().as_secs())).color(DIM));
            }
            if !snap.connected && !snap.error.is_empty() {
                ui.label(RichText::new(&snap.error).color(RED));
            }
            if !self.engine_error.is_empty() {
                ui.label(RichText::new(&self.engine_error).color(RED));
            }
            if !snap.message.is_empty() {
                ui.label(RichText::new(&snap.message).color(DIM));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(self.endpoint.url("")).color(DIM));
            });
        });
    }

    // --------------------------------------------------------------------------------------
    // tabs

    fn card(ui: &mut egui::Ui, title: &str, value: String, sub: String) {
        egui::Frame::group(ui.style()).inner_margin(10.0).rounding(6.0).show(ui, |ui| {
            ui.set_min_width(190.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).color(DIM));
                ui.label(RichText::new(value).size(20.0).strong());
                if !sub.is_empty() {
                    ui.label(RichText::new(sub).color(DIM));
                }
            });
        });
    }

    /// BTC and the first configured currency
    fn money(snap: &Snapshot, btc: f64) -> (String, String) {
        let rates = get(&snap.profit, "Rates");
        let sub = match rates {
            Value::Object(m) => m.iter().filter(|(k, _)| k.as_str() != "BTC").map(|(k, r)| fmt::money(btc * num(r), k)).next().unwrap_or_default(),
            _ => String::new(),
        };
        (fmt::btc(btc), sub)
    }

    fn dashboard(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        if !snap.connected {
            ui.add_space(20.0);
            ui.label(RichText::new(self.t("start_hint")).size(16.0));
            if self.engine_started.map(|t| t.elapsed() < Duration::from_secs(120)).unwrap_or(false) {
                ui.label(RichText::new(self.t("started")).color(ORANGE));
            }
            ui.add_space(8.0);
            match &self.engine_exe {
                Some(exe) => {
                    ui.label(RichText::new(format!("{}: {}", self.t("engine_exe"), exe.display())).color(DIM));
                    if ui.button(RichText::new(self.t("start")).size(16.0).color(GREEN)).clicked() {
                        self.start_engine();
                    }
                }
                None => {
                    ui.label(RichText::new(format!("{}: {}", self.t("engine_exe"), self.t("not_found"))).color(RED));
                }
            }
            return;
        }
        let p = &snap.profit;
        ui.horizontal_wrapped(|ui| {
            let (a, b) = Self::money(snap, num(get(p, "ProfitBTC")));
            Self::card(ui, self.t("profit_day"), a, b);
            let (a, b) = Self::money(snap, num(get(p, "Earnings_1d")));
            Self::card(ui, self.t("earnings"), a, b);
            let pw = get(p, "Power");
            let watt = num(get(pw, "CPU")) + num(get(pw, "GPU")) + num(get(pw, "Offset"));
            Self::card(ui, self.t("power"), format!("{:.0} W", watt), format!("CPU {:.0} W · GPU {:.0} W", num(get(pw, "CPU")), num(get(pw, "GPU"))));
            let up = fmt::uptime(get(p, "Uptime"));
            Self::card(ui, self.t("uptime"), fmt::duration(up), String::new());
        });
        ui.add_space(6.0);
        ui.label(RichText::new(self.t("running_miners")).size(16.0).strong());
        let running = list(&snap.running);
        if running.is_empty() {
            ui.label(RichText::new(self.t("no_miners")).color(DIM));
        } else {
            self.running_grid(ui, &running);
        }
        ui.add_space(6.0);
        ui.label(RichText::new(self.t("devices")).size(16.0).strong());
        self.devices_grid(ui, snap, false);
        let wd = list(&snap.watchdog);
        if !wd.is_empty() {
            ui.add_space(6.0);
            egui::CollapsingHeader::new(format!("{} ({})", self.t("watchdog"), wd.len())).show(ui, |ui| {
                egui::Grid::new("wd").striped(true).show(ui, |ui| {
                    for h in ["MinerName", "DeviceModel", "PoolName", "Algorithm", "Kicked"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for w in wd {
                        for h in ["MinerName", "DeviceModel", "PoolName", "Algorithm", "Kicked"] {
                            ui.label(s(get(w, h)));
                        }
                        ui.end_row();
                    }
                });
            });
        }
    }

    fn running_grid(&self, ui: &mut egui::Ui, running: &[&Value]) {
        egui::Grid::new("running").striped(true).min_col_width(60.0).show(ui, |ui| {
            for k in ["name", "device", "algorithm", "coin", "pool", "speed_live", "profit", "runtime", "shares"] {
                ui.label(RichText::new(self.t(k)).strong());
            }
            ui.end_row();
            for m in running {
                let name = s(get(m, "Name"));
                let benchmarking = truthy(get(m, "NeedsBenchmark"));
                ui.label(RichText::new(&name).color(if benchmarking { ORANGE } else { Color32::WHITE }));
                ui.label(s(get(m, "DeviceName")));
                ui.label(s(get(m, "Algorithm")));
                ui.label(s(get(m, "CoinSymbol")));
                ui.label(s(get(m, "Pool")));
                let speeds: Vec<String> = list(get(m, "Speed_Live")).iter().map(|v| fmt::hash(num(v))).collect();
                ui.label(RichText::new(speeds.join(" + ")).monospace());
                ui.label(if benchmarking { RichText::new(self.t("benchmarked")).color(ORANGE) } else { RichText::new(opt(get(m, "Profit")).map(fmt::btc).unwrap_or_else(|| "-".into())) });
                ui.label(fmt::duration(fmt::timespan(get(m, "RunningTime"))));
                let st = list(get(m, "Stratum"));
                let shares: Vec<String> = st.iter().map(|x| format!("{}/{}", num(get(x, "Accepted")), num(get(x, "Rejected")))).collect();
                ui.label(shares.join(" "));
                ui.end_row();
            }
        });
    }

    fn devices_grid(&self, ui: &mut egui::Ui, snap: &Snapshot, full: bool) {
        let devs = list(&snap.devices);
        if devs.is_empty() {
            ui.label(RichText::new(self.t("no_data")).color(DIM));
            return;
        }
        egui::Grid::new(if full { "devices_full" } else { "devices" }).striped(true).min_col_width(50.0).show(ui, |ui| {
            let mut cols = vec!["name", "model", "temp", "fan", "power", "load"];
            if full {
                cols.splice(2..2, ["type", "vendor"]);
                cols.extend(["clock", "memory"]);
            }
            for k in &cols {
                ui.label(RichText::new(self.t(k)).strong());
            }
            ui.end_row();
            for d in devs {
                let data = get(d, "Data");
                let temp = num(get(data, "Temperature"));
                ui.label(s(get(d, "Name")));
                ui.label(s(get(d, "Model")));
                if full {
                    ui.label(s(get(d, "Type")));
                    ui.label(s(get(d, "Vendor")));
                }
                let tcol = if temp >= 85.0 {
                    RED
                } else if temp >= 75.0 {
                    ORANGE
                } else {
                    Color32::WHITE
                };
                ui.label(RichText::new(if temp > 0.0 { format!("{:.0} °C", temp) } else { "-".into() }).color(tcol));
                let fan = num(get(data, "FanSpeed"));
                ui.label(if fan > 0.0 { format!("{:.0} %", fan) } else { "-".into() });
                let pw = num(get(data, "PowerDraw"));
                ui.label(if pw > 0.0 { format!("{:.0} W", pw) } else { "-".into() });
                let load = num(get(data, "Utilization"));
                ui.label(if load > 0.0 { format!("{:.0} %", load) } else { "-".into() });
                if full {
                    let (c, m) = (num(get(data, "Clock")), num(get(data, "ClockMem")));
                    ui.label(if c > 0.0 { format!("{:.0}/{:.0} MHz", c, m) } else { "-".into() });
                    let mem = num(path(d, &["OpenCL", "GlobalMemSize"]));
                    ui.label(if mem > 0.0 { format!("{:.1} GB", mem / 1073741824.0) } else { "-".into() });
                }
                ui.end_row();
            }
        });
    }

    fn miners(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.horizontal(|ui| {
            ui.label(self.t("filter"));
            ui.add(egui::TextEdit::singleline(&mut self.miner_filter).desired_width(200.0));
            let l = self.t("only_best");
            ui.checkbox(&mut self.only_best, l);
        });
        let f = self.miner_filter.to_lowercase();
        let miners: Vec<&Value> = list(&snap.active)
            .into_iter()
            .filter(|m| !self.only_best || truthy(get(m, "Best")))
            .filter(|m| f.is_empty() || format!("{} {} {} {}", s(get(m, "Name")), s(get(m, "Algorithm")), s(get(m, "Pool")), s(get(m, "DeviceModel"))).to_lowercase().contains(&f))
            .collect();
        if miners.is_empty() {
            ui.label(RichText::new(self.t("no_data")).color(DIM));
            return;
        }
        egui::Grid::new("miners").striped(true).min_col_width(50.0).show(ui, |ui| {
            for k in ["name", "device", "algorithm", "pool", "speed", "profit", "power", "status"] {
                ui.label(RichText::new(self.t(k)).strong());
            }
            ui.end_row();
            for m in miners {
                let best = truthy(get(m, "Best"));
                ui.label(RichText::new(s(get(m, "Name"))).color(if best { GREEN } else { Color32::WHITE }));
                ui.label(s(get(m, "DeviceModel")));
                ui.label(s(get(m, "Algorithm")));
                ui.label(s(get(m, "Pool")));
                let sp: Vec<String> = list(get(m, "Speed")).iter().map(|v| opt(v).map(fmt::hash).unwrap_or_else(|| "?".into())).collect();
                ui.label(RichText::new(sp.join(" + ")).monospace());
                ui.label(opt(get(m, "Profit")).map(fmt::btc).unwrap_or_else(|| "-".into()));
                let pw = num(get(m, "PowerDraw"));
                ui.label(if pw > 0.0 { format!("{:.0} W", pw) } else { "-".into() });
                let st = s(get(m, "StatusText"));
                let col = match st.as_str() {
                    "Running" => GREEN,
                    "Failed" | "RunningFailed" => RED,
                    _ => DIM,
                };
                let label = match st.as_str() {
                    "Running" | "Idle" | "Failed" | "RunningFailed" => self.t(&format!("st_{}", st)).to_string(),
                    _ => st.clone(),
                };
                ui.label(RichText::new(label).color(col));
                ui.end_row();
            }
        });
    }

    fn pools(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        let mut pools = list(&snap.pools);
        if pools.is_empty() {
            ui.label(RichText::new(self.t("no_data")).color(DIM));
            return;
        }
        pools.sort_by_key(|p| s(get(p, "Algorithm")).to_lowercase());
        egui::Grid::new("pools").striped(true).min_col_width(50.0).show(ui, |ui| {
            for k in ["algorithm", "pool", "coin", "price", "host", "region", "hashrate", "tsl", "fee"] {
                ui.label(RichText::new(if k == "price" { format!("{} (BTC/GH/d)", self.t(k)) } else { self.t(k).to_string() }).strong());
            }
            ui.end_row();
            for p in pools {
                ui.label(s(get(p, "Algorithm")));
                let excl = truthy(get(p, "Exclusive"));
                ui.label(RichText::new(s(get(p, "Name"))).color(if excl { ACCENT } else { Color32::WHITE }));
                ui.label(s(get(p, "CoinSymbol")));
                ui.label(RichText::new(format!("{:.8}", num(get(p, "Price")) * 1e9)).monospace());
                ui.label(format!("{}:{}", s(get(p, "Host")), s(get(p, "Port"))));
                ui.label(s(get(p, "Region")));
                ui.label(opt(get(p, "Hashrate")).map(fmt::hash).unwrap_or_else(|| "-".into()));
                ui.label(opt(get(p, "TSL")).map(fmt::duration).unwrap_or_else(|| "-".into()));
                ui.label(format!("{:.2} %", num(get(p, "PoolFee"))));
                ui.end_row();
            }
        });
    }

    fn balances(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        let rows = list(&snap.balances);
        if rows.is_empty() {
            ui.label(RichText::new(self.t("no_data")).color(DIM));
            return;
        }
        // the value columns of the rows ("Value in USD", "Balance (BTC)", ...)
        let mut cols: Vec<String> = Vec::new();
        for r in &rows {
            if let Value::Object(m) = r {
                for k in m.keys() {
                    if (k.starts_with("Value in ") || k.starts_with("Balance (") || k.starts_with("Pending (")) && !cols.contains(k) {
                        cols.push(k.clone());
                    }
                }
            }
        }
        egui::Grid::new("balances").striped(true).min_col_width(60.0).show(ui, |ui| {
            ui.label(RichText::new(self.t("name")).strong());
            ui.label(RichText::new(self.t("currency")).strong());
            ui.label(RichText::new(self.t("total")).strong());
            for c in &cols {
                ui.label(RichText::new(c).strong());
            }
            ui.end_row();
            for r in rows {
                let caption = if s(get(r, "Caption")).is_empty() { s(get(r, "Name")) } else { s(get(r, "Caption")) };
                let total_row = caption == "*Total*";
                let txt = |t: String| if total_row { RichText::new(t).strong() } else { RichText::new(t) };
                ui.label(txt(caption.clone()));
                ui.label(txt(s(get(r, "Currency"))));
                ui.label(txt(opt(get(r, "Total")).map(|v| format!("{:.8}", v)).unwrap_or_default()));
                for c in &cols {
                    ui.label(txt(s(get(r, c))));
                }
                ui.end_row();
            }
        });
    }

    fn benchmarks(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        let rows = list(&snap.benchmarks);
        ui.label(RichText::new(format!("{} ({})", self.t("needs_benchmark"), rows.len())).size(16.0).strong());
        if rows.is_empty() {
            ui.label(RichText::new(self.t("no_benchmarks")).color(GREEN));
            return;
        }
        egui::Grid::new("bench").striped(true).show(ui, |ui| {
            for k in ["name", "device", "algorithm", "pool"] {
                ui.label(RichText::new(self.t(k)).strong());
            }
            ui.end_row();
            for m in rows {
                ui.label(s(get(m, "Name")));
                ui.label(s(get(m, "DeviceModel")));
                ui.label(s(get(m, "Algorithm")));
                ui.label(s(get(m, "Pool")));
                ui.end_row();
            }
        });
    }

    fn console(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        let l = self.t("autoscroll");
        ui.checkbox(&mut self.follow_console, l);
        let content = s(get(&snap.console, "Content"));
        let miners = list(get(&snap.console, "Miners"));
        let h = (ui.available_height() - if miners.is_empty() { 10.0 } else { 40.0 * miners.len().min(4) as f32 }).max(150.0);
        egui::ScrollArea::vertical().id_salt("console").max_height(h).stick_to_bottom(self.follow_console).auto_shrink([false, false]).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut content.as_str()).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
        });
        for m in miners {
            egui::CollapsingHeader::new(s(get(m, "Name"))).show(ui, |ui| {
                let c = s(get(m, "Content"));
                ui.add(egui::TextEdit::multiline(&mut c.as_str()).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
            });
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(self.t("api_host"));
            ui.add(egui::TextEdit::singleline(&mut self.host).desired_width(220.0));
            ui.end_row();
            ui.label(self.t("api_port"));
            ui.add(egui::TextEdit::singleline(&mut self.port).desired_width(80.0));
            ui.end_row();
            ui.label(self.t("api_user"));
            ui.add(egui::TextEdit::singleline(&mut self.user).desired_width(220.0));
            ui.end_row();
            ui.label(self.t("api_password"));
            ui.add(egui::TextEdit::singleline(&mut self.password).password(true).desired_width(220.0));
            ui.end_row();
            ui.label(self.t("interval"));
            ui.add(egui::TextEdit::singleline(&mut self.interval).desired_width(80.0));
            ui.end_row();
            ui.label(self.t("language"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.lang, Lang::En, "English");
                ui.radio_value(&mut self.lang, Lang::Hu, "Magyar");
            });
            ui.end_row();
            ui.label(self.t("base_dir"));
            ui.label(self.base.as_ref().map(|b| b.display().to_string()).unwrap_or_else(|| self.t("not_found").to_string()));
            ui.end_row();
            ui.label(self.t("engine_exe"));
            ui.label(self.engine_exe.as_ref().map(|b| b.display().to_string()).unwrap_or_else(|| self.t("not_found").to_string()));
            ui.end_row();
        });
        ui.add_space(8.0);
        if ui.button(self.t("apply")).clicked() {
            self.endpoint = Endpoint {
                host: self.host.trim().to_string(),
                port: self.port.trim().parse().unwrap_or(4000),
                user: self.user.trim().to_string(),
                password: self.password.clone(),
            };
            self.port = self.endpoint.port.to_string();
            self.send(Cmd::SetEndpoint(self.endpoint.clone()));
            self.send(Cmd::SetInterval(self.interval.trim().parse().unwrap_or(5)));
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let snap = self.shared.snap.lock().unwrap().clone();
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui, &snap);
            ui.add_space(2.0);
        });
        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| self.status_line(ui, &snap));
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Console => self.console(ui, &snap),
            Tab::Settings => self.settings(ui),
            tab => {
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| match tab {
                    Tab::Dashboard => self.dashboard(ui, &snap),
                    Tab::Devices => self.devices_grid(ui, &snap, true),
                    Tab::Miners => self.miners(ui, &snap),
                    Tab::Pools => self.pools(ui, &snap),
                    Tab::Balances => self.balances(ui, &snap),
                    Tab::Benchmarks => self.benchmarks(ui, &snap),
                    _ => {}
                });
            }
        });
        if self.confirm_stop {
            egui::Window::new(self.t("stop")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(self.t("confirm_stop"));
                ui.horizontal(|ui| {
                    if ui.button(RichText::new(self.t("yes")).color(RED)).clicked() {
                        self.send(Cmd::Stop);
                        self.confirm_stop = false;
                    }
                    if ui.button(self.t("no")).clicked() {
                        self.confirm_stop = false;
                    }
                });
            });
        }
        // the "updated .. s" counter
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

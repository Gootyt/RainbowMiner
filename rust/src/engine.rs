//! Start-Core / Invoke-Core / Stop-Core: the profit switching loop of RainbowMiner.
//!
//! One round: (re)load the config, update rates and stats, query the pools and select the best
//! pool per algorithm, ask the miner modules for candidates, calculate profits, choose the most
//! profitable miner combination, start/stop miners, show the status and wait for the next round
//! while the miner APIs are polled.

use crate::api;
use crate::balances;
use crate::combos::{self, ComboItem};
use crate::config::{self, Config, ConfigFiles, PoolConfig};
use crate::data;
use crate::device::{self, CpuInfo, Device, DeviceCache};
use crate::download::{self, DownloadItem, DownloaderState, SharedDownloader};
use crate::jsonx;
use crate::miner::{self, MinerArgs, MinerCandidate, MinerCtx, PoolsMap};
use crate::minerapi::{ActiveMiner, MinerStatus, RunCtx};
use crate::params::CommandLine;
use crate::pool::{self, Pool, PoolCtx};
use crate::pools;
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::util_term::{self, Key};
use crate::web;
use crate::{log_info, log_warn};
use chrono::{DateTime, Datelike, Duration, Utc};
use indexmap::IndexMap;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub const STRIKES: i64 = 3;
pub const SYNC_WINDOW: f64 = 10.0;
pub const OUTOFSYNC_WINDOW: f64 = 60.0;
pub const DECAY_PERIOD: f64 = 60.0;
pub const DECAY_BASE: f64 = 0.9;

#[derive(Debug, Clone)]
pub struct WatchdogTimer {
    pub miner_name: String,
    pub device_model: String,
    pub pool_name: String,
    pub algorithm: String,
    pub kicked: DateTime<Utc>,
    pub active: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Crash {
    pub timestamp: DateTime<Utc>,
    pub start: Option<DateTime<Utc>>,
    pub end: DateTime<Utc>,
    pub runtime: f64,
    pub name: String,
    pub miner_name: String,
    pub vendor: String,
    pub device: Vec<String>,
    pub algorithm: Vec<String>,
    pub pool: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SchedulerEntry {
    pub name: String,
    pub day_of_week: Vec<String>,
    pub month: Vec<String>,
    pub from: String,
    pub to: String,
    pub power_price: f64,
    pub enable: bool,
    pub pause: bool,
    pub enable_mining_heat_control: bool,
    pub mining_heat_control: f64,
    pub algorithm: Vec<String>,
    pub exclude_algorithm: Vec<String>,
    pub coin_symbol: Vec<String>,
    pub exclude_coin_symbol: Vec<String>,
    pub pool_name: Vec<String>,
    pub exclude_pool_name: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MinerSpeed {
    pub hashrate: f64,
    pub names: Vec<String>,
    pub miner: Vec<String>,
}

/// Get-HourMinStr
fn hour_min_str(s: &str, to: bool) -> String {
    let add = if s.to_lowercase().contains('p') { 12 } else { 0 };
    let s: String = s.chars().filter(|c| c.is_ascii_digit() || *c == ':').collect();
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
        return format!("{:02}:{}", (s.parse::<i64>().unwrap_or(0) + add) % 24, if to { "59:59" } else { "00:00" });
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() >= 2 {
        if let (Ok(h), Ok(m)) = (parts[0].parse::<i64>(), parts[1].parse::<i64>()) {
            return format!("{:02}:{:02}:{}", (h + add) % 24, m % 60, if to { "59" } else { "00" });
        }
    }
    if to {
        "23:59:59".into()
    } else {
        "00:00:00".into()
    }
}

/// read scheduler.config.txt
fn load_scheduler(path: &Path, config: &Config) -> Vec<SchedulerEntry> {
    let Ok(v) = config::read_config_content(path, &IndexMap::new(), true) else { return vec![] };
    let list: Vec<Value> = match &v {
        Value::Array(a) => a.clone(),
        Value::Object(_) if !jsonx::get(&v, "value").is_null() => jsonx::as_list(jsonx::get(&v, "value")).into_iter().cloned().collect(),
        Value::Object(_) => vec![v.clone()],
        _ => vec![],
    };
    let num = |x: &Value| -> Option<f64> {
        let s: String = jsonx::s(x).replace(',', ".").chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        s.parse::<f64>().ok()
    };
    let arr = |x: &Value, algo: bool| -> Vec<String> {
        let items: Vec<String> = match x {
            Value::Array(a) => a.iter().map(jsonx::s).collect(),
            _ => {
                let re = regex::Regex::new(r"(?i)[^A-Z0-9,;]+").unwrap();
                re.replace_all(&jsonx::s(x), "").split([',', ';']).map(|s| s.to_string()).collect()
            }
        };
        let mut v: Vec<String> = items.into_iter().filter(|s| !s.is_empty()).map(|s| if algo { data::get_algorithm(&s) } else { s }).collect();
        v.sort();
        v.dedup();
        v
    };
    let dw = |x: &Value| -> Vec<String> {
        let items: Vec<String> = match x {
            Value::Array(a) => a.iter().map(jsonx::s).collect(),
            _ => jsonx::s(x).chars().filter(|c| c.is_ascii_digit() || ",;*".contains(*c)).collect::<String>().split([',', ';']).map(|s| s.to_string()).collect(),
        };
        let mut v: Vec<String> = Vec::new();
        for i in items.into_iter().filter(|s| !s.is_empty()) {
            if !v.contains(&i) {
                v.push(i);
            }
        }
        if v.contains(&"*".to_string()) && v.len() > 1 {
            v = vec!["*".into()];
        }
        v
    };
    list.iter()
        .map(|e| SchedulerEntry {
            name: jsonx::s(jsonx::get(e, "Name")),
            day_of_week: dw(jsonx::get(e, "DayOfWeek")),
            month: dw(jsonx::get(e, "Month")),
            from: hour_min_str(&jsonx::s(jsonx::get(e, "From")), false),
            to: hour_min_str(&jsonx::s(jsonx::get(e, "To")), true),
            power_price: num(jsonx::get(e, "PowerPrice")).unwrap_or_else(|| config.f("PowerPrice")),
            enable: util::get_yes(jsonx::get(e, "Enable")),
            pause: util::get_yes(jsonx::get(e, "Pause")),
            enable_mining_heat_control: if jsonx::s(jsonx::get(e, "EnableMiningHeatControl")).is_empty() {
                config.b("EnableMiningHeatControl")
            } else {
                util::get_yes(jsonx::get(e, "EnableMiningHeatControl"))
            },
            mining_heat_control: util::round_to(num(jsonx::get(e, "MiningHeatControl")).unwrap_or_else(|| config.f("MiningHeatControl")).clamp(0.0, 5.0), 1),
            algorithm: arr(jsonx::get(e, "Algorithm"), true),
            exclude_algorithm: arr(jsonx::get(e, "ExcludeAlgorithm"), true),
            coin_symbol: arr(jsonx::get(e, "CoinSymbol"), false),
            exclude_coin_symbol: arr(jsonx::get(e, "ExcludeCoinSymbol"), false),
            pool_name: arr(jsonx::get(e, "PoolName"), false),
            exclude_pool_name: arr(jsonx::get(e, "ExcludePoolName"), false),
        })
        .collect()
}

/// the linux distribution codename (Get-LinuxDistroInfo)
fn distro_codename() -> String {
    for (f, keys) in [("/etc/os-release", &["VERSION_CODENAME", "UBUNTU_CODENAME"][..]), ("/etc/lsb-release", &["DISTRIB_CODENAME"][..])] {
        if let Ok(t) = std::fs::read_to_string(f) {
            for k in keys {
                for line in t.lines() {
                    if let Some(v) = line.strip_prefix(&format!("{}=", k)) {
                        let v = v.trim().trim_matches('"').trim_matches('\'').to_string();
                        if !v.is_empty() {
                            return v;
                        }
                    }
                }
            }
            return String::new();
        }
    }
    String::new()
}

/// $Session.IsVM: virt-what on Linux (as root), the computer system's manufacturer/model on Windows
fn detect_vm() -> bool {
    if cfg!(target_os = "linux") {
        // virt-what needs root
        let root = std::process::Command::new("id").arg("-u").output().map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0").unwrap_or(false);
        if !root {
            return false;
        }
        match std::process::Command::new("virt-what").output() {
            Ok(o) => String::from_utf8_lossy(&o.stdout).lines().any(|l| !l.trim().is_empty()),
            Err(_) => false,
        }
    } else if cfg!(windows) {
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "$c=Get-CimInstance -ClassName Win32_ComputerSystem; \"$($c.Manufacturer)`n$($c.Model)\""])
            .output();
        let re = regex::Regex::new(r"(?i)^Bochs|^KVM|^HVM|^QEMU|^UML|^Xen|ARAnyM|red hat|virtual|vmware|vmxnet").unwrap();
        match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout).lines().any(|l| re.is_match(l.trim())),
            Err(_) => false,
        }
    } else {
        false
    }
}

/// the glibc version (libc_version.sh)
fn libc_version() -> Option<util::Version> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let out = std::process::Command::new("ldd").arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let re = regex::Regex::new(r"(\d+\.\d+)").unwrap();
    let first = text.lines().next().unwrap_or("");
    re.captures(first).and_then(|c| util::get_version(&c[1]))
}

pub struct Core {
    pub base: PathBuf,
    pub version: String,
    #[allow(dead_code)]
    pub cl: CommandLine,
    pub defaults: IndexMap<String, Value>,
    pub files: ConfigFiles,
    pub config: Config,
    config_stamp: Vec<(PathBuf, Option<SystemTime>)>,
    pub scheduler: Vec<SchedulerEntry>,
    pub cpu: CpuInfo,
    pub all_devices: Vec<Device>,
    pub devices: DeviceCache,
    pub active_miners: Vec<ActiveMiner>,
    pub all_pools: Vec<Pool>,
    pub watchdog_timers: Vec<WatchdogTimer>,
    pub crash_counter: Vec<Crash>,
    pub miner_speeds: HashMap<String, MinerSpeed>,
    pub round_counter: u64,
    pub timer: DateTime<Utc>,
    pub round_start: Option<DateTime<Utc>>,
    pub decay_start: DateTime<Utc>,
    pub decay_fact: f64,
    pub current_interval: i64,
    pub watchdog_interval: f64,
    pub watchdog_reset: f64,
    pub skip_switching_prevention: bool,
    pub start_downloader: bool,
    pub stopp: bool,
    pub benchmarking: bool,
    pub is_benchmarking_run: bool,
    pub profitable: bool,
    pub is_exclusive_run: bool,
    pub restart_miners: bool,
    pub pause_ia: bool,
    pub pause_scheduler: bool,
    pub lock_miners: bool,
    pub lock_pools: Vec<String>,
    pub balances_updated: Option<DateTime<Utc>>,
    pub balances: Vec<Value>,
    pub earnings_avg: f64,
    pub earnings_1d: f64,
    pub downloader: SharedDownloader,
    pub downloader_prq: SharedDownloader,
    pub distro_codename: String,
    pub is_vm: bool,
    /// $Session.YiimpPools
    pub yiimp_pools: HashSet<String>,
    pub libc_version: Option<util::Version>,
    pub current_power_price: f64,
    pub current_power_price_btc: f64,
    pub fixed_cost_per_day_btc: f64,
    pub heat_control: (bool, f64),
    pub ui_full: bool,
    pub start_time: DateTime<Utc>,
    pub pool_alternates: HashMap<String, Vec<String>>,
    pub memory_skipped: Vec<Value>,
    pub api_running: bool,
}

impl Core {
    /// Start-Core
    pub fn start(base: PathBuf, version: String, cl: CommandLine) -> anyhow::Result<Core> {
        let defaults = crate::params::default_values(&cl);
        let config_file = match &cl.config_file {
            Some(f) => {
                let p = PathBuf::from(f);
                if p.is_absolute() {
                    p
                } else {
                    base.join(p)
                }
            }
            None => base.join("Config").join("config.txt"),
        };
        let files = ConfigFiles::new(&config_file);
        util_term::init();
        log_info!("Detecting the CPU and devices ..");
        let cpu = device::detect_cpu(&base);
        let mut core = Core {
            base: base.clone(),
            version,
            cl,
            defaults,
            files,
            config: Config::default(),
            config_stamp: Vec::new(),
            scheduler: Vec::new(),
            cpu,
            all_devices: Vec::new(),
            devices: DeviceCache::default(),
            active_miners: Vec::new(),
            all_pools: Vec::new(),
            watchdog_timers: Vec::new(),
            crash_counter: Vec::new(),
            miner_speeds: HashMap::new(),
            round_counter: 0,
            timer: Utc::now(),
            round_start: None,
            decay_start: Utc::now(),
            decay_fact: 1.0,
            current_interval: 0,
            watchdog_interval: 0.0,
            watchdog_reset: 0.0,
            skip_switching_prevention: false,
            start_downloader: false,
            stopp: false,
            benchmarking: false,
            is_benchmarking_run: false,
            profitable: true,
            is_exclusive_run: false,
            restart_miners: false,
            pause_ia: false,
            pause_scheduler: false,
            lock_miners: false,
            lock_pools: Vec::new(),
            balances_updated: None,
            balances: Vec::new(),
            earnings_avg: 0.0,
            earnings_1d: 0.0,
            downloader: Arc::new(Mutex::new(DownloaderState::default())),
            downloader_prq: Arc::new(Mutex::new(DownloaderState::default())),
            distro_codename: distro_codename(),
            is_vm: detect_vm(),
            yiimp_pools: config::yiimp_pools(&crate::pools::names()),
            libc_version: libc_version(),
            current_power_price: 0.0,
            current_power_price_btc: 0.0,
            fixed_cost_per_day_btc: 0.0,
            heat_control: (false, 0.0),
            ui_full: false,
            start_time: Utc::now(),
            pool_alternates: HashMap::new(),
            memory_skipped: Vec::new(),
            api_running: false,
        };
        if let Err(e) = config::ensure_config_file(&core.files.config, &core.version) {
            return Err(anyhow::anyhow!("{}", e));
        }
        core.load_config(true)?;
        core.detect_devices();
        core.load_sub_configs();
        web::set_cache_dir(core.base.join("Cache"));
        stat::set_max_error_ratio(core.config.f("MaxErrorRatio"));
        core.watchdog_levels(true, 0);
        let port = core.config.i("APIport");
        if port > 0 && port < 65536 {
            api::with(|s| {
                s.start_time = Some(core.start_time);
                s.machine_name = sysinfo::System::host_name().unwrap_or_default();
                s.rand_tag = util::md5_hex(&format!("{}{}", Utc::now(), std::process::id()));
                if core.config.b("APIauth") && !core.config.s("APIuser").is_empty() && !core.config.s("APIpassword").is_empty() {
                    s.auth = Some((core.config.s("APIuser"), core.config.s("APIpassword")));
                }
                s.allow_ips = core.config.arr("APIallowIPs");
                s.version = json!({"Version": core.version, "RemoteVersion": core.version, "ManualURI": "https://github.com/RainbowMiner/RainbowMiner/releases", "Rust": true});
            });
            core.api_running = api::start(core.base.clone(), port as u16, core.config.i("APIthreads").clamp(1, 8) as usize);
        }
        util_term::set_title(&format!("RainbowMiner v{} (Rust port)", core.version));
        {
            let empty = PoolsMap::default();
            let alt = HashMap::new();
            let yiimp = HashSet::new();
            let ctx = MinerCtx {
                base: &core.base,
                config: &core.config,
                devices: &core.devices,
                cpu: &core.cpu,
                pools: &empty,
                alternates: &alt,
                yiimp_pools: &yiimp,
                distro_codename: core.distro_codename.clone(),
                libc_version: core.libc_version.clone(),
                is_vm: core.is_vm,
            };
            let info = Value::Array(miner::get_miners_info(&ctx).iter().map(|m| m.to_json()).collect());
            api::with(|s| s.minerinfo = info);
        }
        stat::load_pool_stats();
        log_info!("Updating exchange rates ..");
        rates::update(&core.config.arr("Currency"), &[], &core.config.arr("ExcludeCurrencyRateFromCoinbase"));
        core.timer = Utc::now();
        core.decay_start = core.timer;
        Ok(core)
    }

    fn stamp(&self) -> Vec<(PathBuf, Option<SystemTime>)> {
        self.files.all().into_iter().map(|(_, p)| (p.clone(), config::file_mtime(&p))).collect()
    }

    /// load config.txt (and set the web/proxy settings)
    fn load_config(&mut self, first: bool) -> anyhow::Result<()> {
        let v = config::load_main_config(&self.files, &self.defaults, &self.cpu.facts()).map_err(|e| anyhow::anyhow!(e))?;
        self.config.v = v;
        web::configure(web::WebSettings {
            proxy: self.config.s("Proxy"),
            proxy_username: self.config.s("ProxyUsername"),
            proxy_password: self.config.s("ProxyPassword"),
            user_agent: self.config.s("UserAgent"),
        });
        crate::log::set_level(crate::log::LogSetting::parse(&self.config.s("LogLevel")));
        if first || self.current_interval == 0 {
            self.current_interval = self.config.i("Interval").max(10);
        }
        self.ui_full = self.config.s("UIstyle").eq_ignore_ascii_case("full");
        Ok(())
    }

    /// (re)detect the devices and build the device cache
    fn detect_devices(&mut self) {
        if self.all_devices.is_empty() {
            let opts = device::DetectOptions {
                base: self.base.clone(),
                opencl_platform_sorting: self.config.arr("OpenCLPlatformSorting"),
                nvsmi_path: self.config.s("NVSMIpath"),
            };
            self.all_devices = device::detect_devices(&opts, &self.cpu);
        }
        self.rebuild_device_cache();
    }

    fn rebuild_device_cache(&mut self) {
        let names = self.config.arr("DeviceName");
        let exclude = self.config.arr("ExcludeDeviceName");
        let selected = device::select_devices(&self.base, &self.all_devices, &names, &exclude);
        let combos = config::load_combos_config(&self.files.path("Combos"));
        self.devices = DeviceCache::build(self.all_devices.clone(), selected, &self.config.s("MiningMode").to_lowercase(), &combos);
        self.config.combos = combos;
        let mut models: Vec<String> = self.devices.devices.iter().map(|d| d.Model.clone()).collect();
        models.extend(self.devices.device_combos.iter().cloned());
        models.sort();
        models.dedup();
        self.config.device_model = models;
        // the CUDA version of the NVIDIA platform ("OpenCL 3.0 CUDA 12.2.140")
        let re = regex::Regex::new(r"CUDA\s+([\d\.]+)").unwrap();
        self.config.cuda_version = self
            .all_devices
            .iter()
            .filter(|d| d.Vendor == "NVIDIA")
            .find_map(|d| re.captures(&d.OpenCL.PlatformVersion).map(|c| c[1].to_string()))
            .unwrap_or_default();
    }

    /// all sub configs (pools, algorithms, coins, devices, miners, userpools, custom miners, scheduler)
    fn load_sub_configs(&mut self) {
        let pool_modules = pools::names();
        config::ensure_algorithms_config(&self.files.path("Algorithms"));
        let (algos, with_rules) = config::load_algorithms_config(&self.files.path("Algorithms"), &[]);
        self.config.algorithms = algos;
        self.config.algorithm_miner_name = with_rules;
        config::ensure_coins_config(&self.files.path("Coins"));
        self.config.coins = config::load_coins_config(&self.files.path("Coins"));
        config::ensure_userpools_config(&self.files.path("Userpools"));
        self.config.userpools = config::load_userpools_config(&self.files.path("Userpools"));
        let up_names: Vec<String> = self
            .config
            .userpools
            .iter()
            .map(|u| jsonx::s(jsonx::get(u, "Name")))
            .filter(|n| !n.is_empty())
            .collect();
        config::ensure_pools_config(&self.files.path("Pools"), &pool_modules, &up_names);
        let mut all_modules = pool_modules.clone();
        all_modules.extend(up_names.iter().cloned());
        match config::load_pools_config(&self.files.path("Pools"), &self.config.v, &self.config.coins, &all_modules) {
            Ok(p) => self.config.pools = p,
            Err(e) => log_warn!("{}", e),
        }
        let models: Vec<(String, String)> = {
            let mut v: Vec<(String, String)> = Vec::new();
            for d in &self.devices.devices {
                if !v.iter().any(|x| x.0 == d.Model) {
                    v.push((d.Model.clone(), d.type_key().to_string()));
                }
            }
            v
        };
        config::ensure_devices_config(&self.files.path("Devices"), &models);
        self.config.devices = config::load_devices_config(&self.files.path("Devices"), &self.config.s("WorkerName"), self.config.b("EnableAlgorithmVariants"));
        self.config.miners = config::load_miners_config(&self.files.path("Miners"), &self.config.device_model, &self.devices.full_combo_models, self.cpu.Threads);
        let builtin = crate::miners::names();
        self.config.custom_miners = config::load_custom_miners(&self.files.path("CustomMiners"), &builtin);
        config::ensure_combos_config(&self.files.path("Combos"), &self.devices.subsets);
        let sched = self.files.path("Scheduler");
        if !sched.exists() {
            let _ = config::write_json(&sched, &json!([serde_json::from_str::<Value>(include_str!("../assets/schedulerconfigdefault.json")).unwrap_or(json!({}))]));
        }
        self.scheduler = load_scheduler(&sched, &self.config);
        self.config_stamp = self.stamp();
    }

    /// reload all configs, if a file changed
    fn check_config(&mut self) {
        if self.stamp() == self.config_stamp {
            return;
        }
        log_info!("Config files changed, reloading");
        if let Err(e) = self.load_config(false) {
            log_warn!("{}", e);
            self.config_stamp = self.stamp();
            return;
        }
        self.rebuild_device_cache();
        self.load_sub_configs();
        stat::set_max_error_ratio(self.config.f("MaxErrorRatio"));
    }

    /// Update-WatchdogLevels
    fn watchdog_levels(&mut self, reset: bool, interval: i64) {
        let mut interval = interval.max(self.config.i("BenchmarkInterval"));
        if self.current_interval < 2 * interval {
            interval = interval.max(self.current_interval);
        }
        let s = STRIKES as f64;
        let keep = if reset { 0.0 } else { 1.0 };
        self.watchdog_interval = (self.watchdog_interval / s * (s - 1.0)) * keep + interval as f64;
        self.watchdog_reset = (self.watchdog_reset / (s * s * s) * (s * s * s - 1.0)) * keep + interval as f64;
    }

    pub fn run_ctx(&self) -> RunCtxOwned {
        RunCtxOwned {
            niceness: if self.config.b("EnableLinuxMinerNiceness") {
                self.config.s("LinuxMinerNiceness").parse::<i32>().ok()
            } else {
                None
            },
        }
    }

    /// the engine state for the pool modules (MiningRigRentals)
    fn engine_view(&self) -> pool::EngineView {
        let benchmarking: Vec<String> = api::with(|s| jsonx::pluck(&s.miners_needing_benchmark, "DeviceName").into_iter().map(jsonx::s).collect());
        let mut bd: Vec<String> = Vec::new();
        for d in benchmarking {
            if !bd.contains(&d) {
                bd.push(d);
            }
        }
        pool::EngineView {
            devices: self.devices.devices.iter().map(|d| (d.Name.clone(), d.Model.clone(), d.Type.clone())).collect(),
            miners: self
                .active_miners
                .iter()
                .map(|m| pool::EngineMiner {
                    enabled: m.enabled,
                    running: m.status == MinerStatus::Running,
                    device_model: m.device_model.clone(),
                    algorithm: m.algorithm.clone(),
                    pool: m.pool.clone(),
                    exclude_pool_name: m.exclude_pool_name.clone(),
                    last_accepted: m.get_last_accepted_seconds(),
                    difficulty: m.algorithm.iter().map(|a| (a.clone(), m.get_difficulty(a))).collect(),
                })
                .collect(),
            benchmarking_devices: bd,
            paused: self.is_paused(),
            paused_by_error: false,
            pause_rentals: false,
            interval: self.config.i("Interval"),
            current_interval: self.current_interval,
            config_dir: self.files.dir.clone(),
        }
    }

    pub fn is_paused(&self) -> bool {
        self.pause_ia || self.pause_scheduler || self.devices.devices.is_empty()
    }

    fn pause_status(&self) -> String {
        let mut v = Vec::new();
        if self.pause_ia {
            v.push("ByUser");
        }
        if self.pause_scheduler {
            v.push("ByScheduler");
        }
        if self.devices.devices.is_empty() {
            v.push("ByDevices");
        }
        v.join(",")
    }
}

/// the parts of RunCtx that are copied out of the core (to avoid borrow conflicts)
pub struct RunCtxOwned {
    pub niceness: Option<i32>,
}

impl RunCtxOwned {
    pub fn ctx<'a>(&self, base: &'a Path, devices: &'a [Device], cpu: &'a CpuInfo, dtv: &'a HashMap<String, String>) -> RunCtx<'a> {
        RunCtx {
            base,
            devices,
            cpu,
            devices_to_vendors: dtv,
            niceness: self.niceness,
        }
    }
}

/// Get-PoolPriorityTier
fn pool_tier(p: &Pool, config: &Config, lock: bool, lock_pools: &[String]) -> i32 {
    let mut t = 0;
    if p.Exclusive && !p.Idle {
        t += 8;
    }
    if let Some(pc) = config.pool(&p.Name) {
        if !pc.focus_wallet.is_empty() && util::contains_ci(&pc.focus_wallet, &p.Currency) {
            t += 4;
        }
    }
    if lock && util::contains_ci(lock_pools, &format!("{}-{}-{}", p.Name, p.Algorithm0, p.CoinSymbol)) {
        t += 2;
    }
    if p.PostBlockMining {
        t += 1;
    }
    t
}

fn ci_set(v: &[String]) -> HashSet<String> {
    v.iter().map(|s| s.to_lowercase()).collect()
}

fn overlaps(set: &HashSet<String>, list: &[String]) -> bool {
    list.iter().any(|x| set.contains(&x.to_lowercase()))
}

impl Core {
    /// the pool part of Invoke-Core: returns the selected pools ($Pools) and all pools
    fn select_pools(&mut self, round_span: Duration, disabled: &HashSet<String>) -> (PoolsMap, Vec<Pool>) {
        let mut new_pools: Vec<Pool> = Vec::new();
        let mut selected: HashSet<String> = HashSet::new();
        let pool_name = self.config.arr("PoolName");
        let exclude_pool_name = self.config.arr("ExcludePoolName");
        let speeds: HashMap<String, f64> = self.miner_speeds.iter().map(|(k, v)| (k.to_lowercase(), v.hashrate)).collect();
        let enable_error_ratio = self.config.b("EnableErrorRatio");
        let view = self.engine_view();
        let run_pool = |name: &str, pc: &PoolConfig, module: Option<&dyn pool::PoolModule>, config: &Config, cpu: &CpuInfo, combos: &[String]| -> Vec<Pool> {
            let worker = if pc.worker.is_empty() { config.s("WorkerName") } else { pc.worker.clone() };
            let ctx = PoolCtx {
                name: name.to_string(),
                cfg: pc,
                config,
                wallets: &pc.wallets,
                params: &pc.params,
                worker,
                stat_span: round_span,
                data_window: pc.data_window.clone(),
                info_only: false,
                allow_zero: pc.allow_zero,
                stat_average: if pc.stat_average.is_empty() { "Minute_10".into() } else { pc.stat_average.clone() },
                stat_average_stable: if pc.stat_average_stable.is_empty() { "Week".into() } else { pc.stat_average_stable.clone() },
                algorithm: pc.algorithm.clone(),
                exclude_algorithm: pc.exclude_algorithm.clone(),
                cpu_cores: cpu.Cores,
                cpu_mhz: cpu.MaxClockSpeed,
                engine: Some(&view),
            };
            let t0 = std::time::Instant::now();
            let list = match module {
                Some(m) => match m.get_pools(&ctx) {
                    Ok(l) => l,
                    Err(e) => {
                        log_warn!("Failed to load {}: {}", name, e);
                        vec![]
                    }
                },
                None => pools::userpools::get_pools(&ctx),
            };
            log_info!("{} loaded in {:.3}s ", name, t0.elapsed().as_secs_f64());
            let list = pool::post_process(list, pc, config, disabled, name, enable_error_ratio);
            pool::expand_solo(list, combos, &speeds)
        };
        if self.round_counter == 0 {
            log_info!("Loading pool modules ..");
        }
        for m in pools::registry() {
            let name = m.name();
            let Some(pc) = self.config.pool(name) else { continue };
            if (!pool_name.is_empty() && !util::contains_ci(&pool_name, name)) || util::contains_ci(&exclude_pool_name, name) {
                continue;
            }
            new_pools.extend(run_pool(name, pc, Some(m.as_ref()), &self.config, &self.cpu, &self.devices.device_combos));
            selected.insert(name.to_lowercase());
        }
        let up_names: Vec<String> = {
            let mut v: Vec<String> = Vec::new();
            for u in &self.config.userpools {
                let n = jsonx::s(jsonx::get(u, "Name"));
                if !n.is_empty() && !v.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
                    v.push(n);
                }
            }
            v
        };
        for n in up_names {
            if selected.contains(&n.to_lowercase()) {
                continue;
            }
            let default_pc = PoolConfig { name: n.clone(), ..Default::default() };
            let pc = self.config.pool(&n).cloned().unwrap_or(default_pc);
            new_pools.extend(run_pool(&n, &pc, None, &self.config, &self.cpu, &self.devices.device_combos));
            selected.insert(n.to_lowercase());
        }

        // re-add failed pools of the last round
        let new_names: HashSet<String> = new_pools.iter().map(|p| p.Name.to_lowercase()).collect();
        let readd: HashSet<String> = self.all_pools.iter().map(|p| p.Name.to_lowercase()).filter(|n| !new_names.contains(n)).collect();
        if !readd.is_empty() {
            let mut r: Vec<&String> = readd.iter().collect();
            r.sort();
            log_info!("Re-Adding currently failed pools: {}", r.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
            for p in self.all_pools.iter().filter(|p| readd.contains(&p.Name.to_lowercase())) {
                new_pools.push(p.clone());
            }
        }

        // filters
        let sch = self.active_scheduler().cloned();
        let mut t_algo = ci_set(&self.config.arr("Algorithm"));
        let mut t_ex_algo = ci_set(&self.config.arr("ExcludeAlgorithm"));
        let mut t_coin = ci_set(&self.config.arr("CoinSymbol"));
        let t_ex_coin = ci_set(&self.config.arr("ExcludeCoin"));
        let mut t_ex_coinsym = ci_set(&self.config.arr("ExcludeCoinSymbol"));
        let mut t_pool = ci_set(&pool_name);
        let mut t_ex_pool = ci_set(&exclude_pool_name);
        if let Some(s) = &sch {
            t_algo.extend(s.algorithm.iter().map(|x| x.to_lowercase()));
            t_ex_algo.extend(s.exclude_algorithm.iter().map(|x| x.to_lowercase()));
            t_coin.extend(s.coin_symbol.iter().map(|x| x.to_lowercase()));
            t_ex_coinsym.extend(s.exclude_coin_symbol.iter().map(|x| x.to_lowercase()));
            t_pool.extend(s.pool_name.iter().map(|x| x.to_lowercase()));
            t_ex_pool.extend(s.exclude_pool_name.iter().map(|x| x.to_lowercase()));
        }
        let unprof = data::unprofitable();
        let check_unprof_cfg = !self.config.b("DisableUnprofitableAlgolist");
        let config = &self.config;
        new_pools.retain(|p| {
            let Some(pc) = config.pool(&p.Name) else { return false };
            let mut algos = vec![p.Algorithm0.clone()];
            if !p.CoinSymbol.is_empty() {
                algos.push(format!("{}-{}", p.Algorithm0, p.CoinSymbol));
            }
            let check_unprof = check_unprof_cfg && !(p.Exclusive && !p.Idle);
            let up_pool = unprof.pools.iter().find(|(k, _)| k.eq_ignore_ascii_case(&p.Name)).map(|x| x.1);
            if !(p.SSL || pc.ssl != 2) {
                return false;
            }
            let cs = &p.CoinSymbol;
            let cn = &p.CoinName;
            let excluded = (!t_pool.is_empty() && !t_pool.contains(&p.Name.to_lowercase()))
                || t_ex_pool.contains(&p.Name.to_lowercase())
                || (!t_algo.is_empty() && !overlaps(&t_algo, &algos))
                || (!t_ex_algo.is_empty() && overlaps(&t_ex_algo, &algos))
                || (check_unprof && util::intersects_ci(&unprof.algorithms, &algos))
                || (check_unprof && up_pool.map(|u| util::intersects_ci(&u.0, &algos)).unwrap_or(false))
                || (check_unprof && !cs.is_empty() && util::contains_ci(&unprof.coins, cs))
                || (check_unprof && !cs.is_empty() && up_pool.map(|u| util::contains_ci(&u.1, cs)).unwrap_or(false))
                || (!t_coin.is_empty() && !cs.is_empty() && !t_coin.contains(&cs.to_lowercase()))
                || (!t_ex_coin.is_empty() && !cn.is_empty() && t_ex_coin.contains(&cn.to_lowercase()))
                || (!t_ex_coinsym.is_empty() && !cs.is_empty() && t_ex_coinsym.contains(&cs.to_lowercase()))
                || (!pc.algorithm.is_empty() && !util::intersects_ci(&pc.algorithm, &algos))
                || (!pc.exclude_algorithm.is_empty() && util::intersects_ci(&pc.exclude_algorithm, &algos))
                || (!cn.is_empty() && !pc.coin_name.is_empty() && !util::contains_ci(&pc.coin_name, cn))
                || (!cn.is_empty() && util::contains_ci(&pc.exclude_coin, cn))
                || (!cs.is_empty() && !pc.coin_symbol.is_empty() && !util::contains_ci(&pc.coin_symbol, cs))
                || (!cs.is_empty() && util::contains_ci(&pc.exclude_coin_symbol, cs));
            if excluded {
                return false;
            }
            if p.Exclusive && !p.Idle {
                return true;
            }
            let ac = config.algorithm(&p.Algorithm);
            let cc = if cs.is_empty() { None } else { config.coin(cs) };
            let lt = |v: Option<f64>, min: f64| v.map(|x| min > 0.0 && x < min).unwrap_or(false);
            let bad = p.Idle
                || (!p.SoloMining && lt(p.Hashrate, ac.map(|a| a.min_hashrate).unwrap_or(0.0)))
                || (p.SoloMining && lt(p.Hashrate, ac.map(|a| a.min_hashrate_solo).unwrap_or(0.0)))
                || lt(p.Workers, ac.map(|a| a.min_workers).unwrap_or(0.0))
                || lt(p.BLK, ac.map(|a| a.min_blk_rate).unwrap_or(0.0))
                || lt(p.BLK, pc.min_blk_rate)
                || (!p.SoloMining && lt(p.Hashrate, cc.map(|c| c.min_hashrate).unwrap_or(0.0)))
                || (p.SoloMining && lt(p.Hashrate, cc.map(|c| c.min_hashrate_solo).unwrap_or(0.0)))
                || lt(p.Workers, cc.map(|c| c.min_workers).unwrap_or(0.0))
                || lt(p.BLK, cc.map(|c| c.min_blk_rate).unwrap_or(0.0));
            !bad
        });
        let all_pools_before_wd = new_pools.len();

        // watchdog
        let wd_interval_time = self.timer - Duration::seconds(self.watchdog_interval as i64);
        let wd_reset_time = self.timer - Duration::seconds(self.watchdog_reset as i64);
        if self.watchdog_reset > self.watchdog_interval {
            self.watchdog_timers.retain(|t| t.kicked > wd_reset_time);
        }
        let names: HashSet<String> = new_pools.iter().map(|p| p.Name.to_lowercase()).collect();
        if names.len() > 1 {
            let timers = &self.watchdog_timers;
            new_pools.retain(|p| {
                if p.Disabled {
                    return false;
                }
                if p.Exclusive {
                    return true;
                }
                let t: Vec<&WatchdogTimer> = timers.iter().filter(|t| t.pool_name == p.Name && t.kicked < wd_interval_time && t.kicked > wd_reset_time).collect();
                if t.len() >= 3 {
                    return false;
                }
                t.iter().filter(|t| t.algorithm == p.Algorithm).count() < 2
            });
        }

        let mut pools_map = PoolsMap::default();
        self.pool_alternates.clear();
        if new_pools.is_empty() {
            self.all_pools = new_pools.clone();
            return (pools_map, new_pools);
        }

        // WhatToMine prices for the pools without own prices
        let wtm: Vec<&Pool> = new_pools.iter().filter(|p| p.WTM).collect();
        if !wtm.is_empty() {
            let wcfg = self.config.pool("WhatToMine");
            let avg = wcfg.map(|c| c.stat_average.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Minute_10".into());
            let avg_st = wcfg.map(|c| c.stat_average_stable.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Week".into());
            let t0 = std::time::Instant::now();
            let prices = pools::whattomine::get_prices(&wtm, round_span, &avg, &avg_st);
            for w in prices {
                for p in new_pools.iter_mut().filter(|p| p.WTM) {
                    if p.Price == 0.0 && p.Algorithm == w.algorithm && p.CoinSymbol == w.coin_symbol && (p.WTMMode.is_empty() || p.WTMMode == w.mode) {
                        p.Price_0 = w.price;
                        p.Price = w.price * p.PenaltyFactor;
                        p.StablePrice = w.stable_price * p.PenaltyFactor;
                        p.MarginOfError = w.margin_of_error;
                        p.Updated = w.updated;
                    }
                }
            }
            log_info!("WhatToMine loaded in {:.3}s ", t0.elapsed().as_secs_f64());
        }

        // compare prices
        let out_of_sync_timer = new_pools.iter().map(|p| p.Updated).max().unwrap_or(self.timer);
        let out_of_sync_time = out_of_sync_timer - Duration::minutes(OUTOFSYNC_WINDOW as i64);
        let oos_divisor = (OUTOFSYNC_WINDOW - SYNC_WINDOW).ln();
        let oos_limit = 1.0 / (OUTOFSYNC_WINDOW - SYNC_WINDOW);
        let sync_factor = |p: &Pool| -> f64 {
            let mins = (out_of_sync_timer - p.Updated).num_milliseconds() as f64 / 60000.0;
            ((((OUTOFSYNC_WINDOW - mins).max(oos_limit)).ln() / oos_divisor + 1.0) / 2.0).min(1.0)
        };
        let pool_switching_hysteresis = 1.0 + self.config.f("PoolSwitchingHysteresis") / 100.0;
        let pool_accuracy_weight = self.config.f("PoolAccuracyWeight") / 100.0;
        let hashrate_weight_strength = self.config.f("HashrateWeightStrength") / 100.0;
        let hashrate_weight = self.config.f("HashrateWeight") / 100.0;
        let mut pools_hashrates: HashMap<String, f64> = HashMap::new();
        for p in new_pools.iter().filter(|p| !p.SoloMining) {
            let k = format!("{}-{}", p.Algorithm0, p.CoinSymbol);
            let e = pools_hashrates.entry(k).or_insert(0.0);
            if p.StablePrice != 0.0 {
                if let Some(h) = p.Hashrate {
                    if h > *e {
                        *e = h;
                    }
                }
            }
        }
        for v in pools_hashrates.values_mut() {
            if *v == 0.0 {
                *v = 1.0;
            }
        }
        for p in new_pools.iter_mut() {
            if p.SoloMining || p.TSL.is_none() || p.CoinSymbol.is_empty() {
                continue;
            }
            let pbm = self.config.pool(&p.Name).map(|c| c.enable_post_block_mining).unwrap_or(false);
            let lim = self.config.coin(&p.CoinSymbol).map(|c| c.post_block_mining).unwrap_or(0) as f64;
            if pbm && p.TSL.unwrap() < lim {
                p.PostBlockMining = true;
            }
        }
        let mut pools_running: HashMap<String, i64> = HashMap::new();
        let mut pools_bench: HashMap<String, bool> = HashMap::new();
        for m in self.active_miners.iter().filter(|m| m.status == MinerStatus::Running) {
            for i in 0..m.pool.len() {
                let k = format!("{}-{}-{}", m.pool[i], m.base_algorithm.get(i).cloned().unwrap_or_default(), m.coin_symbol.get(i).cloned().unwrap_or_default());
                let e = pools_running.entry(k.clone()).or_insert(m.rounds);
                if *e > m.rounds {
                    *e = m.rounds;
                }
                let b = pools_bench.entry(k).or_insert(false);
                *b = *b || m.needs_benchmark;
            }
        }
        let sp = self.config.f("SwitchingPrevention");
        self.decay_fact = sp.min(1.0) * DECAY_BASE.powf((((self.timer - self.decay_start).num_seconds() as f64 / DECAY_PERIOD) as i64) as f64 / sp.max(1.0));
        log_info!("Calculating pool compare prices. ");
        let fast = self.config.b("EnableFastSwitching");
        let force_stable = self.config.b("ForceStablePrice");
        let min_intervals = self.config.i("MinimumMiningIntervals");
        let mut price_cmp: HashMap<String, f64> = HashMap::new();
        for p in new_pools.iter_mut() {
            let ix = format!("{}-{}-{}", p.Name, p.Algorithm0, p.CoinSymbol);
            if price_cmp.contains_key(&ix) {
                continue;
            }
            let mut pc = if !fast && (force_stable || !p.PaysLive) { p.StablePrice } else { p.Price };
            if !p.Exclusive {
                let rounds = pools_running.get(&ix).copied();
                let pcfg = self.config.pool(&p.Name);
                if rounds.is_some() && ((self.is_benchmarking_run && pools_bench.get(&ix).copied().unwrap_or(false)) || rounds.unwrap() < min_intervals) {
                    pc *= 100.0;
                } else if !p.PostBlockMining && !p.CoinSymbol.is_empty() && pcfg.map(|c| util::contains_ci(&c.coin_symbol_pbm, &p.CoinSymbol)).unwrap_or(false) {
                    pc = 0.0;
                    p.DisabledDueToCoinSymbolPBM = true;
                } else {
                    pc *= sync_factor(p);
                    if !(fast || self.skip_switching_prevention) {
                        if rounds.is_none() {
                            let mmoe = pcfg.map(|c| c.max_margin_of_error).unwrap_or(0.0);
                            if mmoe != 0.0 {
                                pc *= 1.0 - ((p.MarginOfError.min(mmoe / 100.0) * self.decay_fact * 100.0).floor() / 100.0) * pool_accuracy_weight;
                            }
                        } else if let Some(h) = pcfg.and_then(|c| c.switching_hysteresis) {
                            pc *= 1.0 + h / 100.0;
                        } else {
                            pc *= pool_switching_hysteresis;
                        }
                    }
                    if !p.SoloMining && self.config.f("HashrateWeightStrength") != 0.0 {
                        if let Some(h) = p.Hashrate {
                            let m = pools_hashrates.get(&format!("{}-{}", p.Algorithm0, p.CoinSymbol)).copied().unwrap_or(1.0);
                            pc *= 1.0 - (1.0 - (h / m).powf(hashrate_weight_strength)) * hashrate_weight;
                        }
                    }
                }
            }
            price_cmp.insert(ix, pc);
        }

        // sort and select
        log_info!("Selecting best pool for each algorithm. ");
        let region = self.config.s("Region");
        let default_regions = self.config.arr("DefaultPoolRegion");
        let lock = self.lock_miners && !self.is_exclusive_run;
        let lock_pools = self.lock_pools.clone();
        let config = &self.config;
        let key_of = |p: &Pool| format!("{}-{}-{}", p.Name, p.Algorithm0, p.CoinSymbol);
        let mut sorted: Vec<&Pool> = new_pools.iter().collect();
        sorted.sort_by(|a, b| {
            let fw = |p: &Pool| config.pool(&p.Name).map(|c| !c.focus_wallet.is_empty() && util::contains_ci(&c.focus_wallet, &p.Currency)).unwrap_or(false);
            let lk = |p: &Pool| lock && util::contains_ci(&lock_pools, &key_of(p));
            let reg_ix = |p: &Pool| default_regions.iter().position(|r| r.eq_ignore_ascii_case(&p.Region)).map(|i| 100 - i as i64).unwrap_or(0);
            let ssl_ok = |p: &Pool| config.pool(&p.Name).map(|c| (c.ssl != 0) == p.SSL && c.ssl != 2 || (c.ssl == 2 && p.SSL)).unwrap_or(!p.SSL);
            let pa = price_cmp.get(&key_of(a)).copied().unwrap_or(0.0);
            let pb = price_cmp.get(&key_of(b)).copied().unwrap_or(0.0);
            (b.Exclusive && !b.Idle)
                .cmp(&(a.Exclusive && !a.Idle))
                .then(fw(b).cmp(&fw(a)))
                .then(lk(b).cmp(&lk(a)))
                .then(b.PostBlockMining.cmp(&a.PostBlockMining))
                .then(pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal))
                .then(b.Region.eq_ignore_ascii_case(&region).cmp(&a.Region.eq_ignore_ascii_case(&region)))
                .then(reg_ix(b).cmp(&reg_ix(a)))
                .then(ssl_ok(b).cmp(&ssl_ok(a)))
                .then(a.Host.cmp(&b.Host))
                .then(a.Port.cmp(&b.Port))
        });
        let max_alt = if self.config.b("EnablePoolAlternates") { self.config.i("MaxPoolAlternates").max(0) as usize } else { 0 };
        let mut alt_count: HashMap<String, usize> = HashMap::new();
        let mut alt_names: HashMap<String, HashSet<String>> = HashMap::new();
        let mut alt_others: HashMap<String, usize> = HashMap::new();
        let mut alt_seen: HashMap<String, HashSet<String>> = HashMap::new();
        let mut alt_named_seen: HashMap<String, HashSet<String>> = HashMap::new();
        let mut alternates: HashMap<String, Vec<String>> = HashMap::new();
        for p in sorted {
            if p.DisabledDueToCoinSymbolPBM {
                continue;
            }
            let an = p.Algorithm.to_lowercase();
            if !pools_map.contains(&an) {
                pools_map.insert(&an, p.clone());
                if max_alt > 0 {
                    alt_count.insert(an.clone(), 0);
                    alt_names.insert(an.clone(), HashSet::new());
                    alt_others.insert(an.clone(), 0);
                    alt_named_seen.insert(an.clone(), HashSet::new());
                    let mut s = HashSet::new();
                    s.insert(key_of(p).to_lowercase());
                    alt_seen.insert(an.clone(), s);
                }
                continue;
            }
            if max_alt == 0 {
                continue;
            }
            let winner = pools_map.get(&an).unwrap().clone();
            let mut extra_named: Vec<String> = Vec::new();
            let cand_rules = config.pool(&p.Name);
            if cand_rules.map(|c| !c.miner_name.is_empty()).unwrap_or(false) || !p.MinerName.is_empty() {
                let win_rules = config.pool(&winner.Name);
                let mut named: Vec<String> = cand_rules.map(|c| c.miner_name.clone()).unwrap_or_default();
                named.extend(p.MinerName.iter().cloned());
                for nm in named {
                    if nm.is_empty() || extra_named.contains(&nm) || alt_named_seen[&an].contains(&nm.to_lowercase()) {
                        continue;
                    }
                    let allows = cand_rules.map(|c| c.miner_name.is_empty() || util::contains_ci(&c.miner_name, &nm)).unwrap_or(true)
                        && (p.MinerName.is_empty() || util::contains_ci(&p.MinerName, &nm))
                        && !cand_rules.map(|c| util::contains_ci(&c.exclude_miner_name, &nm)).unwrap_or(false)
                        && !util::contains_ci(&p.ExcludeMinerName, &nm);
                    let rejects = win_rules.map(|c| util::contains_ci(&c.exclude_miner_name, &nm)).unwrap_or(false)
                        || util::contains_ci(&winner.ExcludeMinerName, &nm)
                        || win_rules.map(|c| !c.miner_name.is_empty() && !util::contains_ci(&c.miner_name, &nm)).unwrap_or(false)
                        || (!winner.MinerName.is_empty() && !util::contains_ci(&winner.MinerName, &nm));
                    if allows && rejects {
                        extra_named.push(nm);
                    }
                }
            }
            if alt_count[&an] >= max_alt && extra_named.is_empty() && (alt_others[&an] >= max_alt || alt_names[&an].contains(&p.Name.to_lowercase())) {
                continue;
            }
            if p.BenchmarkOnly != winner.BenchmarkOnly {
                continue;
            }
            if pool_tier(p, config, lock, &lock_pools) != pool_tier(&winner, config, lock, &lock_pools) {
                continue;
            }
            if !alt_seen.get_mut(&an).unwrap().insert(key_of(p).to_lowercase()) {
                continue;
            }
            let mut alt_key = format!("{}-@{}", p.Algorithm, p.Name);
            if pools_map.contains(&alt_key) {
                alt_key = format!("{}_{}", alt_key, p.CoinSymbol);
            }
            if pools_map.contains(&alt_key) {
                continue;
            }
            pools_map.insert(&alt_key, p.clone());
            alternates.entry(an.clone()).or_default().push(alt_key);
            for nm in extra_named {
                alt_named_seen.get_mut(&an).unwrap().insert(nm.to_lowercase());
            }
            if alt_names.get_mut(&an).unwrap().insert(p.Name.to_lowercase()) && !p.Name.eq_ignore_ascii_case(&winner.Name) {
                *alt_others.get_mut(&an).unwrap() += 1;
            }
            *alt_count.get_mut(&an).unwrap() += 1;
        }
        self.pool_alternates = alternates;

        // out of sync info
        let winners: HashSet<String> = pools_map.iter().filter(|(k, _)| !miner::is_alternate_key(k)).map(|(_, p)| p.Name.clone()).collect();
        if winners.len() > 1 {
            let ups: Vec<DateTime<Utc>> = winners
                .iter()
                .filter_map(|n| new_pools.iter().filter(|p| &p.Name == n && p.Updated >= out_of_sync_time).map(|p| p.Updated).max())
                .collect();
            if let (Some(mn), Some(mx)) = (ups.iter().min(), ups.iter().max()) {
                let mins = (*mx - *mn).num_minutes();
                if mins as f64 > SYNC_WINDOW {
                    log_info!("Pool prices are out of sync ({} minutes). ", mins);
                }
            }
        }

        // bias prices
        let decay = self.decay_fact;
        for (_, p) in pools_map.iter_mut() {
            let mut price = p.Price;
            let bias;
            if !p.Exclusive {
                let mins = (out_of_sync_timer - p.Updated).num_milliseconds() as f64 / 60000.0;
                price *= ((((OUTOFSYNC_WINDOW - mins).max(oos_limit)).ln() / oos_divisor + 1.0) / 2.0).min(1.0);
                let mmoe = config.pool(&p.Name).map(|c| c.max_margin_of_error).unwrap_or(0.0);
                bias = if !fast && mmoe != 0.0 {
                    price * (1.0 - ((p.MarginOfError.min(mmoe / 100.0) * decay * 100.0).floor() / 100.0))
                } else {
                    price
                };
            } else {
                bias = price;
            }
            p.Price_Bias = bias;
            p.Price_Unbias = price;
            p.HasMinerExclusions = config.pool(&p.Name).map(|c| c.has_miner_rules()).unwrap_or(false) || !p.MinerName.is_empty() || !p.ExcludeMinerName.is_empty();
        }
        let _ = all_pools_before_wd;
        self.all_pools = new_pools.clone();
        (pools_map, new_pools)
    }

    /// the active scheduler profile
    fn active_scheduler(&self) -> Option<&SchedulerEntry> {
        let now = chrono::Local::now();
        let tod = now.format("%H:%M").to_string();
        let dow = now.weekday().num_days_from_sunday().to_string();
        let mon = now.month().to_string();
        self.scheduler.iter().filter(|s| {
            s.enable
                && (s.day_of_week.contains(&"*".to_string()) || s.day_of_week.contains(&dow))
                && (s.month.is_empty() || s.month.contains(&"*".to_string()) || s.month.contains(&mon))
                && tod.as_str() >= s.from.as_str()
                && tod.as_str() <= s.to.as_str()
        }).last()
    }
}

/// total physical RAM in GB (0 if unknown)
fn total_ram_gb() -> f64 {
    let mut s = sysinfo::System::new();
    s.refresh_memory();
    util::round_to(s.total_memory() as f64 / 1073741824.0, 1)
}

/// merge the miners.config.txt Params into the module's arguments (the Params block of Invoke-Core)
fn merge_params(args: &str, extra: &str) -> String {
    let mut current = args.to_string();
    let mut list: Vec<String> = Vec::new();
    let re_split = regex::Regex::new(r"\s+-").unwrap();
    let re_p = regex::Regex::new(r"^([-\w]+)(=|\s)?(.*)$").unwrap();
    for part in re_split.split(&format!(" {}", extra)) {
        if part.is_empty() {
            continue;
        }
        let p = format!("-{}", part);
        if let Some(c) = re_p.captures(&p) {
            let pcmd = c[1].to_string();
            let mut pdiv = c.get(2).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            if pdiv.is_empty() {
                pdiv = " ".into();
            }
            let parg = c[3].trim().to_string();
            let esc = regex::escape(&pcmd);
            let re1 = regex::Regex::new(&format!(r"{}(=|\s)[^-\s][^\s]*\s*", esc)).unwrap();
            let mut psub = re1.replace_all(&current, "").to_string();
            let re2 = regex::Regex::new(&format!(r"{}=?(\s+|$)", esc)).unwrap();
            if re2.is_match(&psub) {
                psub = re2.replace_all(&psub, "").to_string();
            }
            current = psub.trim_end().to_string();
            if parg != "#" {
                list.push(if parg.is_empty() { pcmd } else { format!("{}{}{}", pcmd, pdiv, parg) });
            }
        } else {
            list.push(p);
        }
    }
    format!("{}{}", current.trim(), if list.is_empty() { String::new() } else { format!(" {}", list.join(" ")) })
}

#[cfg(test)]
mod merge_tests {
    use super::merge_params;

    #[test]
    fn params_override() {
        assert_eq!(merge_params("-a ethash --intensity 20 -o pool:1", "--intensity 22"), "-a ethash -o pool:1 --intensity 22");
        assert_eq!(merge_params("-a x --lhr 1", "--lhr #"), "-a x");
        assert_eq!(merge_params("-a x", "--no-color"), "-a x --no-color");
    }
}

impl Core {
    /// the miner part of Invoke-Core: candidates with profits (sorted out: unavailable, disabled)
    fn select_miners(&mut self, pools_map: &PoolsMap, disabled: &HashSet<String>) -> (Vec<MinerCandidate>, usize, usize, usize) {
        let ctx = MinerCtx {
            base: &self.base,
            config: &self.config,
            devices: &self.devices,
            cpu: &self.cpu,
            pools: pools_map,
            alternates: &self.pool_alternates,
            yiimp_pools: &self.yiimp_pools,
            distro_codename: self.distro_codename.clone(),
            libc_version: self.libc_version.clone(),
            is_vm: self.is_vm,
        };
        if self.round_counter == 0 {
            log_info!("Loading miner modules ..");
        }
        let t0 = std::time::Instant::now();
        let candidates = if pools_map.is_empty() { vec![] } else { miner::get_miners_content(&ctx, None) };
        let dont_check_cpu = !self.devices.has_type("CPU") || self.config.b("DisableUnprofitableCpuAlgolist");
        let unprofitable_cpu = data::unprofitable_cpu();
        let min_free = self.config.f("MinFreeMemoryGB");
        let ram = total_ram_gb();
        let known: HashSet<String> = self.devices.devices_names.iter().map(|d| d.to_lowercase()).collect();
        let mut mem_skipped: IndexMap<String, Value> = IndexMap::new();
        let mut all: Vec<MinerCandidate> = Vec::new();
        'cand: for m in candidates {
            if m.device_name.is_empty() {
                continue;
            }
            if m.device_model.contains('-') {
                let want = self.devices.device_names.get(&m.device_model).cloned().unwrap_or_default();
                let mut have = m.device_name.clone();
                have.sort();
                if have != want {
                    continue;
                }
            }
            if !dont_check_cpu && m.device_model == "CPU" && util::contains_ci(&unprofitable_cpu, &m.base_algorithm) {
                continue;
            }
            if m.device_model == "CPU" {
                for a in m.base_algorithms() {
                    let need = data::get_algorithm_memory(&a);
                    if need > 0.0 && ram >= 0.5 && need + min_free > ram {
                        if !mem_skipped.contains_key(&a) {
                            mem_skipped.insert(a.clone(), json!({"Algorithm": a, "NeedGB": need, "MinFreeGB": min_free, "TotalGB": ram}));
                            miner::warn_once(
                                &format!("mem-{}", a),
                                &format!(
                                    "{} needs {} GB of RAM plus {} GB for the system, but this machine has {} GB. Skipping all CPU miners for {} - set MinFreeMemoryGB to 0 in config.txt to mine it anyway.",
                                    a, need, min_free, ram, a
                                ),
                            );
                        }
                        continue 'cand;
                    }
                }
            }
            if self.config.b("DisableDualMining") && m.hashrates.len() > 1 {
                continue;
            }
            let mut seen = HashSet::new();
            for d in &m.device_name {
                if !known.contains(&d.to_lowercase()) || !seen.insert(d.to_lowercase()) {
                    continue 'cand;
                }
            }
            if self.config.miner(&format!("{}-{}-{}", m.base_name, m.device_model, m.base_algorithm)).map(|x| x.disable()).unwrap_or(false) {
                continue;
            }
            for a in m.hashrates.keys() {
                if let Some(p) = pools_map.get(a) {
                    if p.HasMinerExclusions {
                        let rules = self.config.pool(&p.Name);
                        if rules.map(|r| (!r.miner_name.is_empty() && !util::contains_ci(&r.miner_name, &m.base_name)) || util::contains_ci(&r.exclude_miner_name, &m.base_name)).unwrap_or(false)
                            || (!p.MinerName.is_empty() && !util::contains_ci(&p.MinerName, &m.base_name))
                            || util::contains_ci(&p.ExcludeMinerName, &m.base_name)
                        {
                            continue 'cand;
                        }
                    }
                }
            }
            let base_algos = m.base_algorithms();
            for a in &base_algos {
                if util::contains_ci(&self.config.algorithm_miner_name, a) {
                    if let Some(ac) = self.config.algorithm(a) {
                        if (!ac.miner_name.is_empty() && !util::contains_ci(&ac.miner_name, &m.base_name)) || util::contains_ci(&ac.exclude_miner_name, &m.base_name) {
                            continue 'cand;
                        }
                    }
                }
            }
            for dev in m.device_model.split('-') {
                if self.config.miner(&format!("{}-{}-{}", m.base_name, dev, m.base_algorithm)).map(|x| x.disable()).unwrap_or(false) {
                    continue 'cand;
                }
                if let Some(dc) = self.config.device(dev) {
                    if (dc.disable_dual_mining && m.hashrates.len() > 1)
                        || (!dc.algorithm.is_empty() && !util::intersects_ci(&dc.algorithm, &base_algos))
                        || (!dc.exclude_algorithm.is_empty() && util::intersects_ci(&dc.exclude_algorithm, &base_algos))
                        || (!dc.miner_name.is_empty() && !util::contains_ci(&dc.miner_name, &m.base_name))
                        || util::contains_ci(&dc.exclude_miner_name, &m.base_name)
                    {
                        continue 'cand;
                    }
                }
            }
            all.push(m);
        }
        log_info!("Miner modules loaded in {:.3}s ", t0.elapsed().as_secs_f64());
        self.memory_skipped = mem_skipped.into_values().collect();

        // combo mode
        if self.config.s("MiningMode").eq_ignore_ascii_case("combo") {
            self.handle_combos(&mut all);
        }

        log_info!("Calculating profit for each miner. ");
        let hmf = if self.heat_control.0 { 3.0 - self.heat_control.1 } else { 1.0 };
        let ft_cpu = self.config.f("MinerFaultToleranceCPU") / 100.0;
        let ft_gpu = self.config.f("MinerFaultToleranceGPU") / 100.0;
        let power_price = (100.0 + self.config.f("PowerOffsetPercent")) * 24.0 / 100000.0 * self.current_power_price_btc;
        let ignore_fees = self.config.b("IgnoreFees");
        let use_power_price = self.config.b("UsePowerPrice");
        let combo_mode = self.config.s("MiningMode").eq_ignore_ascii_case("combo");
        let legacy = self.config.s("MiningMode").eq_ignore_ascii_case("legacy");
        let auto_bench = self.config.b("EnableAutoBenchmark");
        let mut version_check: HashMap<String, (bool, String, Option<SystemTime>)> = HashMap::new();
        let worker_name = self.config.s("WorkerName");
        let re_diff = regex::Regex::new(r"\{diff:(.+?)\}").unwrap();
        let re_wn = regex::Regex::new(r"\{workername:(.+?)\}").unwrap();
        for m in all.iter_mut() {
            let names = m.algo_names();
            for a in &names {
                if let Some(p) = pools_map.get(a) {
                    m.pools.insert(a.clone(), p.clone());
                }
            }
            m.benchmark_only = names.iter().any(|a| m.pools.get(a).map(|p| p.BenchmarkOnly).unwrap_or(false));
            let is_cpu = m.device_model == "CPU";
            let parts: Vec<String> = m.device_model.split('-').map(|s| s.to_string()).collect();
            for p in &parts {
                m.oc_profile.insert(p.clone(), String::new());
            }
            let ft = if is_cpu { ft_cpu } else { ft_gpu };
            m.fault_tolerance = Some(match m.fault_tolerance {
                Some(f) if f != 0.0 => f.max(ft),
                _ => ft,
            });
            // miners.config.txt
            let (mut args_cfg, mut diff_cfg) = (String::new(), String::new());
            let (mut msia, mut penalty, mut extend, mut fault, mut share_check) = (0i64, -1.0f64, -1i64, -1.0f64, -1i64);
            let (mut hash_adj, mut hash2_adj) = (-111.0f64, -111.0f64);
            let mut power_cfg: Option<f64> = None;
            let mut chain: Vec<String> = vec![m.base_name.clone(), m.device_model.clone()];
            chain.extend(m.base_algorithms());
            let mut found = false;
            for i in (1..=chain.len()).rev() {
                let key = chain[..i].join("-");
                let Some(mc) = self.config.miner(&key) else { continue };
                found = true;
                if mc.has("Params") && args_cfg.is_empty() {
                    args_cfg = mc.s("Params");
                }
                if mc.has("Difficulty") && diff_cfg.is_empty() {
                    diff_cfg = mc.s("Difficulty");
                }
                if mc.has("MSIAprofile") && msia == 0 {
                    msia = mc.s("MSIAprofile").parse().unwrap_or(0);
                }
                if mc.has("Penalty") && penalty == -1.0 {
                    penalty = util::parse_f64(&mc.s("Penalty")).unwrap_or(-1.0);
                }
                if mc.has("HashAdjust") && hash_adj == -111.0 {
                    hash_adj = util::parse_f64(&mc.s("HashAdjust")).unwrap_or(-111.0);
                }
                if mc.has("Hash2Adjust") && hash2_adj == -111.0 {
                    hash2_adj = util::parse_f64(&mc.s("Hash2Adjust")).unwrap_or(-111.0);
                }
                if mc.has("ShareCheck") && share_check == -1 {
                    share_check = mc.s("ShareCheck").parse().unwrap_or(-1);
                }
                if mc.has("ExtendInterval") && extend == -1 {
                    extend = mc.s("ExtendInterval").parse().unwrap_or(-1);
                }
                if mc.has("FaultTolerance") && fault == -1.0 {
                    fault = util::parse_f64(&mc.s("FaultTolerance")).unwrap_or(-1.0);
                }
                if mc.has("OCprofile") && i > 1 {
                    for p in &parts {
                        if m.oc_profile.get(p).map(|x| x.is_empty()).unwrap_or(true) {
                            m.oc_profile.insert(p.clone(), mc.s("OCprofile"));
                        }
                    }
                }
                if is_cpu && power_cfg.is_none() && mc.has("PowerDraw") {
                    power_cfg = util::parse_f64(&mc.s("PowerDraw"));
                }
            }
            if !found && combo_mode && m.device_model.contains('-') {
                for p in &parts {
                    let mut c = chain.clone();
                    c[1] = p.clone();
                    let Some(mc) = self.config.miner(&c.join("-")) else { continue };
                    if mc.has("Params") && args_cfg.is_empty() {
                        args_cfg = mc.s("Params");
                    }
                    if mc.has("Difficulty") && diff_cfg.is_empty() {
                        diff_cfg = mc.s("Difficulty");
                    }
                    if let Some(x) = util::parse_f64(&mc.s("Penalty")) {
                        if x > penalty {
                            penalty = x;
                        }
                    }
                    if let Ok(x) = mc.s("ExtendInterval").parse::<i64>() {
                        if x > extend {
                            extend = x;
                        }
                    }
                    if let Some(x) = util::parse_f64(&mc.s("FaultTolerance")) {
                        if x > fault {
                            fault = x;
                        }
                    }
                }
            }
            if !args_cfg.is_empty() {
                match &mut m.arguments {
                    MinerArgs::Str(s) => *s = merge_params(s, &args_cfg),
                    MinerArgs::Obj(o) => {
                        if jsonx::get(o, "Params").is_string() {
                            let cur = jsonx::s(jsonx::get(o, "Params"));
                            jsonx::set(o, "Params", json!(merge_params(&cur, &args_cfg)));
                        }
                    }
                }
            }
            if msia != 0 {
                m.msia_profile = msia;
            }
            if penalty != -1.0 {
                m.penalty = penalty;
            }
            if extend != -1 {
                m.extend_interval = extend;
            }
            if fault != -1.0 {
                m.fault_tolerance = Some(fault);
            }
            if share_check != -1 {
                m.share_check = share_check;
            }
            if let Some(p) = power_cfg {
                m.power_draw = p;
            }
            let root = m.base_algorithm.split('-').next().unwrap_or("").to_string();
            if m.msia_profile == 0 && names.len() == 1 {
                if let Some(ac) = self.config.algorithm(&root) {
                    if ac.msia_profile > 0 {
                        m.msia_profile = ac.msia_profile;
                    }
                }
            }
            for p in &parts {
                if m.oc_profile.get(p).map(|x| x.is_empty()).unwrap_or(true) {
                    let v = if names.len() == 1 {
                        self.config.algorithm(&root).map(|a| a.oc_profile.clone()).filter(|x| !x.is_empty())
                    } else {
                        None
                    };
                    let v = v.unwrap_or_else(|| self.config.device(p).map(|d| d.default_oc_profile.clone()).unwrap_or_default());
                    m.oc_profile.insert(p.clone(), v);
                }
            }
            // version check
            let vc = version_check
                .entry(m.base_name.clone())
                .or_insert_with(|| {
                    let ver = util::get_miner_version(&m.version).to_string();
                    let inst = download::miner_inst_path(&self.base, &m.path).join("_uri.json");
                    let ok = m.path.exists() && download::installed_uri(&self.base, &m.path).map(|u| u == m.uri).unwrap_or(false);
                    (ok, ver, if m.path.exists() { config::file_mtime(&inst) } else { None })
                })
                .clone();
            let mut needs_reset = false;
            if auto_bench && (legacy || !m.device_model.contains('-')) && vc.2.is_some() {
                let ba = m.base_algorithms();
                let key = format!("{}_{}_HashRate", m.name, ba[0]);
                if let Some(st) = stat::cached(&key) {
                    let date: DateTime<Utc> = vc.2.unwrap().into();
                    if (!st.version.is_empty() && st.version != vc.1) || (st.version.is_empty() && st.updated < date) {
                        let first = m.name.split('-').next().unwrap_or("").to_string();
                        for a in &ba {
                            let k = format!("{}_{}_HashRate", m.name, a);
                            stat::remove_cached(&k);
                            let pat = format!("*-{}-{}*_{}_HashRate.txt", first, m.device_name.join("*"), a);
                            if let Ok(rd) = std::fs::read_dir(self.base.join("Stats").join("Miners")) {
                                for e in rd.flatten() {
                                    if device::like(&e.file_name().to_string_lossy(), &pat) {
                                        let _ = std::fs::remove_file(e.path());
                                    }
                                }
                            }
                        }
                        log_info!("{}: miner version changed to {}, benchmarks reset", m.name, vc.1);
                        needs_reset = true;
                    }
                }
            }
            // profits
            let mut no_result = false;
            let mut bad_dual = false;
            let (mut p_sum, mut pb_sum, mut pu_sum) = (0.0f64, 0.0f64, 0.0f64);
            for (i, a) in names.iter().enumerate() {
                let fee = if ignore_fees { 0.0 } else { m.dev_fee.get(a).copied().unwrap_or(0.0) };
                m.dev_fee.insert(a.clone(), fee);
                let hr = m.hashrates.get(a).copied().flatten();
                if hr.is_none() || needs_reset {
                    m.hashrates.insert(a.clone(), None);
                    no_result = true;
                    continue;
                }
                let stat_key = format!("{}_{}_HashRate", m.name, miner::algorithm_base(a));
                let mut factor = 1.0 - fee / 100.0;
                if m.penalty != 0.0 {
                    factor = (factor - m.penalty / 100.0).max(0.0);
                }
                if !m.disabled && disabled.contains(&stat_key.to_lowercase()) {
                    m.disabled = true;
                }
                let mut h = hr.unwrap();
                let adj = if i > 0 { hash2_adj } else { hash_adj };
                if adj != -111.0 {
                    h *= 1.0 + adj / 100.0;
                }
                m.hashrates.insert(a.clone(), Some(h));
                let st = stat::cached(&stat_key);
                m.difficulties.insert(a.clone(), st.as_ref().map(|s| s.diff_average).unwrap_or(0.0));
                m.ratios.insert(a.clone(), st.as_ref().map(|s| s.ratio_live).unwrap_or(0.0));
                let p = m.pools.get(a);
                p_sum += h * p.map(|p| p.Price).unwrap_or(0.0) * factor;
                pb_sum += h * (p.map(|p| p.Price_Bias).unwrap_or(0.0) + 1e-32) * factor;
                pu_sum += h * (p.map(|p| p.Price_Unbias).unwrap_or(0.0) + 1e-32) * factor;
                if i > 0 && h <= 0.0 {
                    bad_dual = true;
                }
            }
            if no_result {
                m.profit = None;
                m.profit_bias = 0.0;
                m.profit_unbias = 0.0;
                m.profit_cost = None;
            } else if bad_dual {
                m.profit = Some(0.0);
                m.profit_bias = 0.0;
                m.profit_unbias = 0.0;
                m.profit_cost = Some(0.0);
            } else {
                m.profit = Some(p_sum);
                m.profit_bias = pb_sum * 1e15;
                m.profit_unbias = pu_sum * 1e15;
                m.profit_cost = Some(if is_cpu && (self.config.f("PowerOffset") > 0.0 || self.config.f("PowerOffsetPercent") > 0.0) { 0.0 } else { m.power_draw * power_price });
            }
            if let Some(cost) = m.profit_cost {
                if (use_power_price || (!is_cpu && self.heat_control.0 && m.power_draw > 0.0)) && cost > 0.0 {
                    if use_power_price {
                        if let Some(p) = m.profit.as_mut() {
                            *p -= cost;
                        }
                    }
                    m.profit_cost_bias = cost * hmf * 1e15;
                }
            }
            // final argument string
            let mut args = match &m.arguments {
                MinerArgs::Str(s) => util::collapse_ws(s),
                MinerArgs::Obj(o) => {
                    let mut o = o.clone();
                    if jsonx::get(&o, "Params").is_string() {
                        let p = util::collapse_ws(&jsonx::s(jsonx::get(&o, "Params")));
                        jsonx::set(&mut o, "Params", json!(p));
                    }
                    serde_json::to_string(&o).unwrap_or_default()
                }
            };
            let diff: f64 = diff_cfg.replace(',', ".").chars().filter(|c| c.is_ascii_digit() || *c == '.').collect::<String>().parse().unwrap_or(0.0);
            if !args.is_empty() {
                args = args.replace("$difficulty", &format!("{}", diff));
                args = re_diff.replace_all(&args, |c: &regex::Captures| if diff > 0.0 { c[1].to_string() } else { String::new() }).to_string();
                let mut workers: Vec<String> = Vec::new();
                for p in &parts {
                    let w = self.config.device(p).map(|d| d.worker.clone()).filter(|w| !w.is_empty()).unwrap_or_else(|| worker_name.clone());
                    if !workers.contains(&w) {
                        workers.push(w);
                    }
                }
                let wj = workers.join("_");
                args = args.replace("{workername}", &wj).replace(&format!("{{workername:{}}}", worker_name), &wj);
                args = re_wn.replace_all(&args, "$1").to_string();
            }
            m.arguments_str = args;
            if m.extend_interval == 0 || self.config.b("DisableExtendInterval") {
                m.extend_interval = 1;
            }
        }

        // availability and downloads
        let mut avail: Vec<MinerCandidate> = Vec::new();
        let mut dl: Vec<DownloadItem> = Vec::new();
        let mut dl_prq: Vec<DownloadItem> = Vec::new();
        let mut exists: HashMap<PathBuf, bool> = HashMap::new();
        let mut all_sorted: Vec<&MinerCandidate> = all.iter().collect();
        all_sorted.sort_by(|a, b| b.extend_interval.cmp(&a.extend_interval));
        for m in &all_sorted {
            let ok_path = *exists.entry(m.path.clone()).or_insert_with(|| m.path.exists());
            let ok_prq = match &m.prerequisite_path {
                Some(p) => *exists.entry(p.clone()).or_insert_with(|| p.exists()),
                None => true,
            };
            let vok = version_check.get(&m.base_name).map(|v| v.0).unwrap_or(false);
            if ok_path && ok_prq && vok {
                continue;
            }
            if !vok && !m.uri.is_empty() && !dl.iter().any(|d| d.uri == m.uri && d.path == m.path) {
                dl.push(DownloadItem { uri: m.uri.clone(), path: m.path.clone(), is_miner: true });
            }
            if let Some(p) = &m.prerequisite_path {
                if !ok_prq && !m.prerequisite_uri.is_empty() && !dl_prq.iter().any(|d| d.path == *p) {
                    dl_prq.push(DownloadItem { uri: m.prerequisite_uri.clone(), path: p.clone(), is_miner: false });
                }
            }
        }
        for m in all {
            let ok_path = exists.get(&m.path).copied().unwrap_or(false);
            let ok_prq = m.prerequisite_path.as_ref().map(|p| exists.get(p).copied().unwrap_or(false)).unwrap_or(true);
            if ok_path && ok_prq && version_check.get(&m.base_name).map(|v| v.0).unwrap_or(false) {
                avail.push(m);
            }
        }
        let running_paths: Vec<PathBuf> = self.active_miners.iter().filter(|m| m.status == MinerStatus::Running).map(|m| m.path.clone()).collect();
        let keep = self.config.b("EnableKeepDownloads");
        let backups = self.config.b("EnableMinerBackups");
        let downloading = dl.len();
        let downloading_prq = dl_prq.len();
        if !dl.is_empty() || self.start_downloader {
            if !dl.is_empty() && !self.downloader.lock().unwrap().running {
                log_info!("Starting download of {} miners.", dl.len());
                download::start(&self.downloader, self.base.clone(), dl.clone(), running_paths.clone(), keep, backups);
            } else if dl.is_empty() && !dl_prq.is_empty() && !self.downloader.lock().unwrap().running && !self.downloader_prq.lock().unwrap().running {
                log_info!("Starting download of {} pre-requisites.", dl_prq.len());
                download::start(&self.downloader_prq, self.base.clone(), dl_prq.clone(), running_paths, keep, backups);
            }
            self.start_downloader = false;
        }
        api::with(|s| {
            s.download_list = Value::Array(
                dl.iter()
                    .chain(dl_prq.iter())
                    .map(|d| json!({"URI": d.uri, "Path": d.path.to_string_lossy(), "IsMiner": d.is_miner}))
                    .collect(),
            );
        });

        if self.config.b("ExcludeMinersWithFee") {
            avail.retain(|m| m.dev_fee.values().sum::<f64>() == 0.0);
        }
        let before_wd = avail.len();

        // miner speeds
        let mut groups: IndexMap<String, Vec<usize>> = IndexMap::new();
        for (i, m) in avail.iter().enumerate() {
            groups.entry(format!("{}|{}", m.base_algorithm, m.device_model)).or_default().push(i);
        }
        for (k, ix) in groups {
            let (ba, model) = k.split_once('|').unwrap();
            let algo = ba.split('-').next().unwrap_or("");
            let key = format!("{}-{}", algo, model);
            let names: Vec<String> = util::unique_ci(ix.iter().map(|i| format!("{}_{}_Hashrate", avail[*i].name, algo)));
            let miners: Vec<String> = util::unique_ci(ix.iter().map(|i| avail[*i].base_name.clone()));
            let best = ix
                .iter()
                .max_by(|a, b| {
                    let (x, y) = (&avail[**a], &avail[**b]);
                    x.profit.is_none().cmp(&y.profit.is_none()).then(x.profit_bias.partial_cmp(&y.profit_bias).unwrap_or(std::cmp::Ordering::Equal))
                })
                .copied()
                .unwrap();
            match avail[best].hashrates.values().next().copied().flatten() {
                None => {
                    self.miner_speeds.remove(&key);
                }
                Some(h) => {
                    self.miner_speeds.insert(key, MinerSpeed { hashrate: util::round_to(h, 2), names, miner: miners });
                }
            }
        }

        // dual mining during exclusive pools, watchdog
        let wd_interval_time = self.timer - Duration::seconds(self.watchdog_interval as i64);
        let wd_reset_time = self.timer - Duration::seconds(self.watchdog_reset as i64);
        let dual_rentals = self.config.b("DisableDualMining") || self.config.b("EnableDualMiningDuringRentals");
        let timers = &self.watchdog_timers;
        avail.retain(|m| {
            if !(dual_rentals || m.hashrates.len() == 1 || !m.pools.values().any(|p| p.Exclusive)) {
                return false;
            }
            let keys: HashSet<String> = timers
                .iter()
                .filter(|t| t.miner_name == m.name && t.kicked < wd_interval_time && t.kicked > wd_reset_time)
                .map(|t| format!("{}|{}", t.pool_name, miner::strip_alternate(&t.algorithm)).to_lowercase())
                .collect();
            if keys.len() >= 2 {
                return false;
            }
            if !keys.is_empty() {
                for k in m.hashrates.keys() {
                    let pn = m.pools.get(k).map(|p| p.Name.clone()).unwrap_or_default();
                    if keys.contains(&format!("{}|{}", pn, miner::strip_alternate(k)).to_lowercase()) {
                        return false;
                    }
                }
            }
            true
        });
        let miners_json = Value::Array(avail.iter().map(|m| m.to_api_json()).collect());
        let _ = std::fs::write(self.base.join("Data").join("miners.json"), serde_json::to_vec(&miners_json).unwrap_or_default());
        api::with(|s| s.miners = miners_json);
        let zero_watt = self.config.b("DisableZeroWattMiners");
        avail.retain(|m| !(m.disabled || m.hashrates.values().any(|h| *h == Some(0.0)) || (zero_watt && m.power_draw == 0.0 && !m.needs_benchmark())));
        (avail, before_wd, downloading, downloading_prq)
    }

    /// the combo mode part of the miner selection
    fn handle_combos(&mut self, all: &mut Vec<MinerCandidate>) {
        log_info!("Start add missing combos");
        let mut remove_combos = false;
        let snapshot: Vec<(String, String, String, bool, bool)> = all
            .iter()
            .map(|m| (m.base_name.clone(), m.base_algorithm.clone(), m.device_model.clone(), m.needs_benchmark(), m.device_model.contains('-')))
            .collect();
        for (bn, ba, dm, nb, is_combo) in &snapshot {
            if dm != "CPU" && *nb && !is_combo {
                for cb in all.iter() {
                    if &cb.base_name == bn && &cb.base_algorithm == ba && cb.device_model.contains('-') && !cb.needs_benchmark() && cb.device_model.split('-').any(|x| x == dm) {
                        for a in ba.split('-') {
                            stat::remove_miner_stat_files(&format!("{}_{}_HashRate", cb.name, a));
                        }
                    }
                }
                remove_combos = true;
            }
        }
        if remove_combos {
            all.retain(|m| m.device_model == "CPU" || !m.device_model.contains('-'));
        } else {
            let cfg = &self.config;
            all.retain(|m| {
                if m.device_model == "CPU" || !m.device_model.contains('-') {
                    return true;
                }
                if cfg.miner(&format!("{}-{}-{}", m.base_name, m.device_model, m.base_algorithm)).is_some() {
                    return true;
                }
                let mut params: Vec<String> = m
                    .device_model
                    .split('-')
                    .filter_map(|d| cfg.miner(&format!("{}-{}-{}", m.base_name, d, m.base_algorithm)).map(|c| c.s("Params")))
                    .collect();
                params.sort();
                params.dedup();
                params.len() <= 1
            });
            log_info!("Gather mining statistics for fresh combos");
            let n = all.len();
            for i in 0..n {
                if all[i].device_model == "CPU" || !all[i].needs_benchmark() || !all[i].device_model.contains('-') {
                    continue;
                }
                let (bn, ba, dm) = (all[i].base_name.clone(), all[i].base_algorithm.clone(), all[i].device_model.clone());
                let mut members: Vec<usize> = Vec::new();
                let mut complete = true;
                for d in dm.split('-') {
                    match (0..n).find(|j| all[*j].base_name == bn && all[*j].base_algorithm == ba && all[*j].device_model == d) {
                        Some(j) if all[j].hashrates.values().all(|h| h.map(|x| x > 0.0).unwrap_or(false)) => members.push(j),
                        _ => {
                            complete = false;
                            break;
                        }
                    }
                }
                if !complete {
                    continue;
                }
                let mut sums: HashMap<String, f64> = HashMap::new();
                let mut power = 0.0;
                for j in &members {
                    for (k, v) in &all[*j].hashrates {
                        *sums.entry(miner::algorithm_base(k)).or_insert(0.0) += v.unwrap_or(0.0);
                    }
                    power += all[*j].power_draw;
                }
                all[i].power_draw = power;
                let keys: Vec<String> = all[i].hashrates.keys().cloned().collect();
                for k in keys {
                    let b = miner::algorithm_base(&k);
                    if let Some(s) = sums.get(&b) {
                        all[i].hashrates.insert(k, Some(s * 1.02));
                    }
                }
            }
        }
        all.retain(|m| m.device_model == "CPU" || !m.device_model.contains('-') || !m.needs_benchmark());
        log_info!("End add missing combos");
    }
}

/// Write-ActivityLog (+ crash counter and auto-disable)
#[allow(clippy::too_many_arguments)]
fn write_activity(base: &Path, config: &Config, crash_counter: &mut Vec<Crash>, m: &mut ActiveMiner, crashed: i32) {
    let Some(start) = m.get_active_start() else { return };
    let now = Utc::now();
    let last = m.get_active_last();
    if crashed != 0 {
        let runtime = m.get_running_time(false).num_milliseconds() as f64 / 1000.0;
        let c = Crash {
            timestamp: now,
            start: Some(start),
            end: last,
            runtime,
            name: m.base_name.clone(),
            miner_name: m.name.clone(),
            vendor: m.stats_sub.clone(),
            device: vec![m.device_model.clone()],
            algorithm: m.base_algorithm.clone(),
            pool: m.pool.clone(),
        };
        crash_counter.push(c.clone());
        if config.b("EnableAutoDisableMiners") && config.i("MaxCrashesBeforeDisable") > 0 {
            let window = config.i("CrashTrackingWindowMinutes").max(1);
            let ws = now - Duration::minutes(window);
            let akey = c.algorithm.join("-");
            let count = crash_counter.iter().filter(|x| x.miner_name == c.miner_name && x.algorithm.join("-") == akey && x.timestamp > ws).count() as i64;
            if count >= config.i("MaxCrashesBeforeDisable") {
                let mut new = false;
                for a in &c.algorithm {
                    if stat::write_disabled_marker(&c.vendor, &format!("{}_{}_HashRate", c.miner_name, a), &format!("crashed {} times within {} minutes", count, window), true) {
                        new = true;
                    }
                }
                if new {
                    log_warn!("Miner {} with {} auto-disabled: crashed {} times within {} minutes. ", c.miner_name, akey, count, window);
                }
            }
        }
    }
    let limit = now - Duration::minutes(config.i("CrashTrackingWindowMinutes").max(60));
    crash_counter.retain(|c| c.timestamp > limit);
    let line = json!({
        "ActiveStart": start.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string(),
        "ActiveLast": last.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string(),
        "Name": m.base_name,
        "Device": [m.device_model],
        "Algorithm": m.base_algorithm,
        "Pool": m.pool,
        "Speed": m.speed_live,
        "Profit": m.profit,
        "PowerDraw": m.power_draw,
        "Ratio": m.rejected_share_ratio,
        "Crashed": crashed,
        "OCmode": "off",
        "OCP": null,
        "Donation": false,
    });
    let f = base.join("Logs").join(format!("Activity_{}.txt", chrono::Local::now().format("%Y-%m-%d")));
    use std::io::Write;
    if let Ok(mut fh) = std::fs::OpenOptions::new().create(true).append(true).open(&f) {
        let _ = writeln!(fh, "{},", line);
    }
}

/// the sort order of the active miners for the best miner selection
fn active_order(a: &ActiveMiner, b: &ActiveMiner) -> std::cmp::Ordering {
    b.is_exclusive_miner
        .cmp(&a.is_exclusive_miner)
        .then(b.is_locked.cmp(&a.is_locked))
        .then(b.profit.is_none().cmp(&a.profit.is_none()))
        .then(b.is_focus_wallet_miner.cmp(&a.is_focus_wallet_miner))
        .then((b.post_block_mining > 0).cmp(&(a.post_block_mining > 0)))
        .then((b.is_running_first_rounds && !b.needs_benchmark).cmp(&(a.is_running_first_rounds && !a.needs_benchmark)))
        .then(b.profit_bias.partial_cmp(&a.profit_bias).unwrap_or(std::cmp::Ordering::Equal))
        .then(b.benchmarked.cmp(&a.benchmarked))
        .then(b.extend_interval.cmp(&a.extend_interval))
        .then((b.algorithm.first() == b.base_algorithm.first()).cmp(&(a.algorithm.first() == a.base_algorithm.first())))
}

fn devkey_sorted(d: &[String]) -> String {
    let mut v = d.to_vec();
    v.sort();
    v.join("|")
}

/// best miner per device set: indices into `sorted`, in first appearance order
fn best_per_device(miners: &[ActiveMiner], sorted: &[usize], filter: impl Fn(&ActiveMiner) -> bool) -> Vec<usize> {
    let mut first: HashMap<String, usize> = HashMap::new();
    for &i in sorted {
        if !filter(&miners[i]) {
            continue;
        }
        first.entry(devkey_sorted(&miners[i].device_name)).or_insert(i);
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for &i in sorted {
        if !seen.insert(miners[i].device_name.join("|")) {
            continue;
        }
        if let Some(&b) = first.get(&devkey_sorted(&miners[i].device_name)) {
            out.push(b);
        }
    }
    out
}

impl Core {
    /// the active miner part of Invoke-Core: sync the candidates, choose the best combination,
    /// stop and start miners. Returns the number of miners that need a benchmark
    fn update_and_switch(&mut self, mut miners: Vec<MinerCandidate>, pools_map: &PoolsMap, have_pools: bool) -> (usize, String) {
        let min_intervals = self.config.i("MinimumMiningIntervals");
        let lock = self.lock_miners && !self.is_exclusive_run;
        for m in self.active_miners.iter_mut() {
            m.profit = Some(0.0);
            m.profit_bias = 0.0;
            m.profit_unbias = 0.0;
            m.profit_cost = Some(0.0);
            m.profit_cost_bias = 0.0;
            m.best = false;
            m.stopped = false;
            m.enabled = false;
            m.is_focus_wallet_miner = false;
            m.is_exclusive_miner = false;
            m.is_locked = false;
            m.benchmark_only = false;
            m.post_block_mining = 0;
            m.is_running_first_rounds = m.status == MinerStatus::Running && m.rounds < min_intervals && !self.is_benchmarking_run;
        }
        if self.skip_switching_prevention || self.config.b("EnableFastSwitching") {
            for m in miners.iter_mut() {
                m.profit_bias = m.profit_unbias;
            }
        }
        let hyst = 1.0 + self.config.f("MinerSwitchingHysteresis") / 100.0;
        for a in self.active_miners.iter().filter(|a| a.status == MinerStatus::Running) {
            let a_algos: Vec<String> = a.algorithm.iter().map(|x| miner::strip_alternate(x)).collect();
            if let Some(m) = miners.iter_mut().find(|m| {
                m.name == a.name && m.path == a.path && m.arguments_str == a.arguments && m.api == a.api && {
                    let mut x: Vec<String> = m.hashrates.keys().map(|k| miner::strip_alternate(k)).collect();
                    let mut y = a_algos.clone();
                    x.sort();
                    y.sort();
                    x == y
                }
            }) {
                m.profit_bias = m.profit_unbias * hyst;
                if a.is_running_first_rounds {
                    m.profit_bias *= 100.0;
                }
            }
        }
        let prefer = self.config.arr("PreferMinerName");
        let margin = self.config.f("PreferMinerMargin");
        if !prefer.is_empty() && margin != 0.0 {
            for m in miners.iter_mut() {
                if !util::contains_ci(&prefer, &m.base_name) {
                    m.profit_bias *= 1.0 - margin / 100.0;
                }
            }
        }
        if self.config.b("FastestMinerOnly") {
            let mut groups: IndexMap<String, Vec<MinerCandidate>> = IndexMap::new();
            for m in miners.drain(..) {
                let k = format!("{}{}{}", m.device_name.join(""), m.base_algorithm, if m.needs_benchmark() { m.name.clone() } else { String::new() });
                groups.entry(k).or_default().push(m);
            }
            let mut keys: Vec<String> = groups.keys().cloned().collect();
            keys.sort();
            for k in keys {
                let mut g = groups.shift_remove(&k).unwrap();
                if g.len() == 1 {
                    miners.extend(g);
                    continue;
                }
                g.sort_by(|a, b| {
                    b.profit
                        .is_none()
                        .cmp(&a.profit.is_none())
                        .then((b.profit_bias - b.profit_cost_bias).partial_cmp(&(a.profit_bias - a.profit_cost_bias)).unwrap_or(std::cmp::Ordering::Equal))
                        .then((b.profit != Some(0.0)).cmp(&(a.profit != Some(0.0))))
                        .then(a.name.cmp(&b.name))
                });
                let top = g[0].base_name.clone();
                miners.extend(g.into_iter().filter(|m| m.base_name == top));
            }
        }
        let needing: Vec<&MinerCandidate> = miners.iter().filter(|m| m.needs_benchmark()).collect();
        let needing_count = needing.len();
        let fastest = Value::Array(miners.iter().map(|m| m.to_api_json()).collect());
        let mnb = Value::Array(needing.iter().map(|m| m.to_api_json()).collect());
        api::with(|s| {
            s.fastest_miners = fastest;
            s.miners_needing_benchmark = mnb;
        });

        // sync the candidates into the active miners
        let mut index: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, a) in self.active_miners.iter().enumerate() {
            index.entry(a.key()).or_default().push(i);
        }
        let now = Utc::now();
        let show_window = self.config.b("ShowMinerWindow") || cfg!(target_os = "linux");
        let max_rej_cfg = self.config.f("MaxRejectedShareRatio");
        let bench_interval = self.config.i("BenchmarkInterval");
        let auto_port = self.config.b("EnableAutoMinerPorts");
        let enable_oc = self.config.b("EnableOCProfiles");
        let re_manual = regex::Regex::new(r"^(.+?github.com/.+?/releases)").unwrap();
        for c in &miners {
            let key = ActiveMiner::index_key(&c.name, &c.path, &c.arguments_str, &c.api, &c.algo_names());
            let existing = index.get(&key).and_then(|v| {
                v.iter().copied().find(|i| {
                    let a = &self.active_miners[*i];
                    a.name == c.name && a.path == c.path && a.arguments == c.arguments_str && a.api == c.api
                })
            });
            let pools: Vec<&Pool> = c.pools.values().collect();
            let first = c.algo_names().first().cloned().unwrap_or_default();
            let fp = c.pools.get(&first);
            let is_locked = lock
                && !self.lock_pools.is_empty()
                && c.pools.values().all(|p| util::contains_ci(&self.lock_pools, &format!("{}-{}-{}", p.Name, p.Algorithm0, p.CoinSymbol)));
            let is_focus = pools.iter().any(|p| self.config.pool(&p.Name).map(|pc| !pc.focus_wallet.is_empty() && util::contains_ci(&pc.focus_wallet, &p.Currency)).unwrap_or(false));
            let is_excl = pools.iter().any(|p| p.Exclusive);
            let pbm = match fp {
                Some(p) if !p.SoloMining && p.TSL.is_some() && !p.CoinSymbol.is_empty() => {
                    let lim = self.config.coin(&p.CoinSymbol).map(|x| x.post_block_mining).unwrap_or(0);
                    let en = self.config.pool(&p.Name).map(|x| x.enable_post_block_mining).unwrap_or(false);
                    if en && lim > 0 && (p.TSL.unwrap() as i64) < lim {
                        lim - p.TSL.unwrap() as i64
                    } else {
                        0
                    }
                }
                _ => 0,
            };
            let manual = if c.manual_uri.is_empty() && !c.uri.contains("RainbowMiner") {
                re_manual.captures(&c.uri).map(|m| m[1].to_string()).unwrap_or_default()
            } else {
                c.manual_uri.clone()
            };
            let max_rej = c.max_rejected_share_ratio.unwrap_or(max_rej_cfg).clamp(0.0, 1.0);
            let a: &mut ActiveMiner = match existing {
                Some(i) => &mut self.active_miners[i],
                None => {
                    log_info!("New miner object {}: {}", c.base_name, c.algo_names().join("+"));
                    let mut a = ActiveMiner::from_candidate(c);
                    a.max_benchmark_rounds = STRIKES;
                    a.stats_sub = self.devices.vendor_of_model(&c.device_model);
                    self.active_miners.push(a);
                    let i = self.active_miners.len() - 1;
                    index.entry(key.clone()).or_default().push(i);
                    &mut self.active_miners[i]
                }
            };
            a.version = c.version.clone();
            a.profit = c.profit;
            a.profit_bias = c.profit_bias;
            a.profit_unbias = c.profit_unbias;
            a.profit_cost = c.profit_cost;
            a.profit_cost_bias = c.profit_cost_bias;
            a.power_draw = c.power_draw;
            a.speed = c.hashrates.values().cloned().collect();
            a.show_miner_window = show_window || c.show_miner_window;
            a.msia_profile = c.msia_profile;
            a.dev_fee = c.dev_fee.clone();
            a.oc_profile = c.oc_profile.clone();
            a.enable_oc_profile = enable_oc;
            a.extend_interval = c.extend_interval;
            a.skip_seconds = c.skip_seconds.unwrap_or(5);
            a.fault_tolerance = c.fault_tolerance.unwrap_or(0.1);
            a.penalty = c.penalty;
            a.manual_uri = manual;
            a.data_interval = bench_interval;
            a.enable_auto_port = auto_port;
            a.enabled = true;
            a.is_focus_wallet_miner = is_focus;
            a.is_exclusive_miner = is_excl;
            a.is_locked = is_locked;
            a.post_block_mining = pbm;
            a.min_samples = c.min_samples.unwrap_or(3);
            a.coin_name = pools.iter().map(|p| p.CoinName.clone()).collect();
            a.coin_symbol = pools.iter().map(|p| p.CoinSymbol.clone()).collect();
            a.pool_penalty = pools.iter().map(|p| p.Penalty).collect();
            a.blk = pools.iter().map(|p| p.BLK).collect();
            a.no_cpu_mining = c.no_cpu_mining;
            a.needs_benchmark = c.needs_benchmark();
            a.benchmark_only = c.benchmark_only;
            a.max_rejected_share_ratio = max_rej;
            a.mining_priority = c.mining_priority;
            a.mining_affinity = c.mining_affinity.clone();
            a.set_ld_library_path = c.set_ld_library_path.unwrap_or(true);
            a.share_check = c.share_check;
            a.hashrate_regex = c.hashrate_regex.clone();
            a.access_last = now;
            if a.stats_sub.is_empty() {
                a.stats_sub = self.devices.vendor_of_model(&c.device_model);
            }
        }
        for m in self.active_miners.iter_mut() {
            if m.profit_cost_bias > 0.0 {
                m.profit_bias -= m.profit_cost_bias;
            }
        }

        self.profitable = true;
        let mut message = String::new();
        if !miners.is_empty() {
            let mut sorted: Vec<usize> = (0..self.active_miners.len()).filter(|i| {
                let m = &self.active_miners[*i];
                m.enabled && (m.needs_benchmark || !m.benchmark_only)
            }).collect();
            let am = &self.active_miners;
            sorted.sort_by(|a, b| active_order(&am[*a], &am[*b]));
            let mut best = best_per_device(am, &sorted, |_| true);
            // post block mining: check the minimum profit
            let pbm: Vec<usize> = best
                .iter()
                .copied()
                .filter(|i| {
                    let m = &am[*i];
                    m.post_block_mining > 0
                        && !m.is_exclusive_miner
                        && !m.is_locked
                        && !m.is_focus_wallet_miner
                        && !m.needs_benchmark
                        && !m.is_running_first_rounds
                        && self.config.coin(m.coin_symbol.first().map(|s| s.as_str()).unwrap_or("")).map(|c| c.min_profit_percent > 0.0).unwrap_or(false)
                })
                .collect();
            if !pbm.is_empty() {
                let base_list = best_per_device(am, &sorted, |m| m.post_block_mining == 0);
                let use_pp = self.config.b("UsePowerPrice");
                let mut remove = Vec::new();
                for i in pbm {
                    let k = devkey_sorted(&am[i].device_name);
                    if let Some(&b) = base_list.iter().find(|b| devkey_sorted(&am[**b].device_name) == k) {
                        let prof = |m: &ActiveMiner| m.profit.unwrap_or(0.0) + if use_pp && m.profit_cost.unwrap_or(0.0) > 0.0 { m.profit_cost.unwrap_or(0.0) } else { 0.0 };
                        let pct = self.config.coin(&am[i].coin_symbol[0]).map(|c| c.min_profit_percent).unwrap_or(0.0);
                        if prof(&am[b]) * pct / 100.0 > prof(&am[i]) {
                            best.push(b);
                            remove.push(i);
                        }
                    }
                }
                best.retain(|i| !remove.contains(i));
            }
            let no_cpu_mining = self.config.b("EnableCheckMiningConflict")
                && needing_count == 0
                && best.iter().any(|i| am[*i].device_model == "CPU")
                && best.iter().any(|i| am[*i].no_cpu_mining);
            let mut best2: Vec<usize> = if no_cpu_mining { best_per_device(am, &sorted, |m| !m.no_cpu_mining) } else { vec![] };
            let mut check_profitability = false;
            if self.config.b("UsePowerPrice") && needing_count == 0 {
                if self.config.b("CheckProfitability") {
                    let lvl = self.config.f("ProfitabilityLevel");
                    best.retain(|i| am[*i].profit.unwrap_or(0.0) > lvl || am[*i].is_exclusive_miner || am[*i].is_locked);
                    best2.retain(|i| am[*i].profit.unwrap_or(0.0) > lvl || am[*i].is_exclusive_miner || am[*i].is_locked);
                }
                check_profitability = true;
            }
            let combo_mode = self.config.s("MiningMode").eq_ignore_ascii_case("combo");
            if combo_mode {
                for list in [&mut best, &mut best2] {
                    let excl: Vec<Vec<String>> = list.iter().filter(|i| am[**i].device_model != "CPU" && (am[**i].is_exclusive_miner || am[**i].is_locked)).map(|i| am[*i].device_name.clone()).collect();
                    for dn in excl {
                        list.retain(|i| {
                            let m = &am[*i];
                            !m.device_model.contains('-') || !util::intersects_ci(&dn, &m.device_name) || devkey_sorted(&dn) == devkey_sorted(&m.device_name)
                        });
                    }
                }
            }
            let items = |list: &[usize]| -> Vec<ComboItem> {
                list.iter()
                    .map(|i| ComboItem {
                        devices: am[*i].device_name.clone(),
                        profit_is_null: am[*i].profit.is_none(),
                        sort_value: am[*i].profit_bias,
                    })
                    .collect()
            };
            let mut combo: Vec<usize> = if no_cpu_mining {
                let l1: Vec<usize> = best.iter().copied().filter(|i| am[*i].device_model != "CPU").collect();
                let mut c1: Vec<usize> = combos::best_combo(&items(&l1)).into_iter().map(|k| l1[k]).collect();
                message = "CPU".into();
                if needing_count == 0 {
                    let c2: Vec<usize> = combos::best_combo(&items(&best2)).into_iter().map(|k| best2[k]).collect();
                    let s1: f64 = c1.iter().map(|i| am[*i].profit.unwrap_or(0.0)).sum();
                    let s2: f64 = c2.iter().map(|i| am[*i].profit.unwrap_or(0.0)).sum();
                    if s1 < s2 {
                        message = "GPU-only".into();
                        c1 = c2;
                    }
                }
                message = format!("GPU/CPU mining conflict: {} will not be started for best profit", message);
                c1
            } else {
                combos::best_combo(&items(&best)).into_iter().map(|k| best[k]).collect()
            };
            if combo_mode && needing_count == 0 && self.devices.device_combos.iter().any(|c| c.contains('-')) {
                let gkey = |m: &ActiveMiner| {
                    format!("{}{}{}{}{}{}", m.name.split('-').next().unwrap_or(""), m.vendor, m.pool.join(""), m.algorithm.join(""), m.currency.join(""), m.coin_symbol.join(""))
                };
                let ratio = self.config.f("MinComboOverSingleRatio");
                let mut groups: IndexMap<String, Vec<usize>> = IndexMap::new();
                for i in &combo {
                    groups.entry(gkey(&am[*i])).or_default().push(*i);
                }
                let mut out = Vec::new();
                for (k, g) in groups {
                    let multi = if g.len() > 1 {
                        let mut devs: Vec<String> = g.iter().flat_map(|i| am[*i].device_name.clone()).collect();
                        devs.sort();
                        devs.dedup();
                        best.iter()
                            .copied()
                            .filter(|i| gkey(&am[*i]) == k && devkey_sorted(&am[*i].device_name) == devs.join("|"))
                            .max_by(|a, b| am[*a].profit_bias.partial_cmp(&am[*b].profit_bias).unwrap_or(std::cmp::Ordering::Equal))
                    } else {
                        None
                    };
                    match multi {
                        Some(mi) if g.iter().map(|i| am[*i].profit_bias).sum::<f64>() * ratio < am[mi].profit_bias => out.push(mi),
                        _ => out.extend(g),
                    }
                }
                combo = out;
            }
            if have_pools && check_profitability {
                let po_cost = self.config.f("PowerOffset") * 24.0 / 1000.0 * self.current_power_price_btc + self.fixed_cost_per_day_btc;
                let total: f64 = combo.iter().map(|i| am[*i].profit.unwrap_or(0.0)).sum();
                if total - po_cost <= self.config.f("ProfitabilityLevel") {
                    if self.config.b("CheckProfitability") && !combo.iter().any(|i| am[*i].is_exclusive_miner || am[*i].is_locked) {
                        self.profitable = false;
                    }
                    log_warn!("No more miners are profitable. {}", if self.config.b("CheckProfitability") { " Waiting for profitability." } else { "" });
                }
            }
            let excl_count = combo.iter().filter(|i| am[**i].is_exclusive_miner).count();
            if self.pause_scheduler && (excl_count > 0 || self.is_exclusive_run) {
                self.pause_scheduler = false;
            }
            let paused = self.is_paused();
            if (!paused || excl_count > 0) && self.profitable {
                for i in combo {
                    if !paused || self.active_miners[i].is_exclusive_miner {
                        self.active_miners[i].best = true;
                    }
                }
            }
        }
        let _ = pools_map;
        (needing_count, message)
    }

    /// stop crashed and no longer profitable miners, start the best ones
    fn switch_miners(&mut self, pools_map: &PoolsMap) {
        let rco = self.run_ctx();
        let wd_interval_time = self.timer - Duration::seconds(self.watchdog_interval as i64);
        let wd_reset_time = self.timer - Duration::seconds(self.watchdog_reset as i64);
        let watchdog = self.config.b("Watchdog");
        let restart_miners = self.restart_miners;
        let Core { base, all_devices, cpu, devices, active_miners, config, crash_counter, watchdog_timers, .. } = self;
        let rc = rco.ctx(base, all_devices, cpu, &devices.devices_to_vendors);
        for m in active_miners.iter_mut() {
            if m.get_status() == MinerStatus::RunningFailed {
                log_info!("Stopping crashed miner ({}) ", m.name);
                m.crash_count += 1;
                write_activity(base, config, crash_counter, m, 1);
                m.set_status(MinerStatus::Idle, &rc);
            }
        }
        for m in active_miners.iter_mut() {
            if (!m.best || restart_miners || m.restart) && m.activated > 0 && m.status == MinerStatus::Running {
                log_info!("{} miner {} on pool {}.", if m.restart { "Restarting" } else { "Stopping" }, m.name, m.pool.join("/"));
                m.set_status(MinerStatus::Idle, &rc);
                m.stopped = true;
                m.restart = false;
                if watchdog && !watchdog_timers.is_empty() {
                    for (i, algo) in m.algorithm.clone().iter().enumerate() {
                        let pool = m.pool.get(i).cloned().unwrap_or_default();
                        if let Some(t) = watchdog_timers.iter().find(|t| t.miner_name == m.name && t.pool_name == pool && &t.algorithm == algo) {
                            if t.kicked < wd_interval_time && !restart_miners {
                                write_activity(base, config, crash_counter, m, 2);
                                m.set_status(MinerStatus::Failed, &rc);
                                log_warn!("Miner {} mining {} on pool {} temporarily disabled.", m.name, algo, pool);
                            } else {
                                let name = m.name.clone();
                                watchdog_timers.retain(|t| !(t.miner_name == name && t.pool_name == pool && &t.algorithm == algo));
                            }
                        }
                    }
                }
            }
        }
        // kill rogue processes of the miner binaries
        let tracked: HashSet<u32> = active_miners.iter().filter_map(|m| m.process_id()).collect();
        let mut exes: HashSet<String> = active_miners.iter().filter_map(|m| m.path.file_name().map(|f| f.to_string_lossy().to_string())).collect();
        exes.extend(active_miners.iter().flat_map(|m| m.executables.iter().cloned()));
        for (pid, name) in crate::proc::find_rogue_processes(&base.join("Bin"), &tracked, &exes) {
            log_warn!("Stop rogue process {} (pid {})", name, pid);
            crate::proc::kill_pid(pid);
        }
        let exclude_wd = config.arr("ExcludeFromWatchdog");
        let timer = Utc::now();
        let mut decay_start = None;
        for m in active_miners.iter_mut() {
            if !m.best || m.status == MinerStatus::Running {
                continue;
            }
            if m.device_model != "CPU" {
                m.set_static_port(config.i("StaticGPUMinerPort").clamp(0, 65535) as u16);
            } else {
                m.set_static_port(config.i("StaticCPUMinerPort").clamp(0, 65535) as u16);
            }
            if m.speed.iter().any(|s| s.is_none()) {
                log_info!("Benchmarking miner ({}): '{} {}' (Extend Interval {})", m.name, m.path.display(), m.arguments, m.extend_interval);
            } else {
                log_info!("Starting miner ({}): '{}'", m.name, m.path.display());
            }
            let cpu_prio = m.mining_priority.unwrap_or(config.i("MiningPriorityCPU") as i32);
            let gpu_prio = m.mining_priority.unwrap_or(config.i("MiningPriorityGPU") as i32);
            let aff = match &m.mining_affinity {
                Some(a) => a.clone(),
                None if m.device_model != "CPU" => config.s("GPUMiningAffinity"),
                None => String::new(),
            };
            m.set_priorities(cpu_prio, gpu_prio, &aff);
            decay_start = Some(Utc::now());
            m.set_status(MinerStatus::Running, &rc);
            if watchdog && m.profit.is_some() {
                for (i, algo) in m.algorithm.iter().enumerate() {
                    let mut pool = m.pool.get(i).cloned().unwrap_or_default();
                    if pool.is_empty() {
                        pool = pools_map.get(algo).map(|p| p.Name.clone()).unwrap_or_default();
                    }
                    let check = vec![m.name.clone(), miner::algorithm_base(algo), pool.clone()];
                    if util::intersects_ci(&check, &exclude_wd) {
                        continue;
                    }
                    match watchdog_timers.iter_mut().find(|t| t.miner_name == m.name && t.pool_name == pool && &t.algorithm == algo) {
                        None => watchdog_timers.push(WatchdogTimer {
                            miner_name: m.name.clone(),
                            device_model: m.device_model.clone(),
                            pool_name: pool,
                            algorithm: algo.clone(),
                            kicked: timer,
                            active: timer,
                        }),
                        Some(t) => {
                            if t.kicked <= wd_reset_time {
                                t.kicked = timer;
                                t.active = timer;
                            }
                        }
                    }
                }
            }
        }
        if let Some(d) = decay_start {
            self.decay_start = d;
        }
        self.is_exclusive_run = self.active_miners.iter().any(|m| m.is_exclusive_miner && m.status == MinerStatus::Running);
        // evict stale miner objects
        let max_hours = std::env::var("RBM_ACTIVEMINERS_MAXHOURS").ok().and_then(|v| v.parse::<i64>().ok()).unwrap_or(24);
        let expire = Utc::now() - Duration::hours(max_hours);
        let expire_ui = Utc::now() - Duration::seconds(5 * self.config.i("Interval"));
        let before = self.active_miners.len();
        self.active_miners.retain_mut(|m| {
            !(m.access_last < expire && m.status != MinerStatus::Running && !m.best && !m.enabled && !m.stopped && !m.new && m.process.is_none() && m.get_active_last() < expire_ui)
        });
        let evicted = before - self.active_miners.len();
        if evicted > 0 {
            log_info!("Evicted {} stale miner object{}, {} left in the active list.", evicted, if evicted > 1 { "s" } else { "" }, self.active_miners.len());
        }
    }
}

pub struct UpdateStatus {
    pub updated: usize,
    pub failed: usize,
    pub exclusive_failed: usize,
}

impl Core {
    pub fn pause_status_text(&self) -> String {
        self.pause_status()
    }

    /// Update-ActiveMiners
    fn update_active_miners(&mut self, first_round: bool, update_device_info: bool) -> UpdateStatus {
        if update_device_info {
            let mut names: Vec<String> = self.active_miners.iter().filter(|m| m.enabled).flat_map(|m| m.device_name.clone()).collect();
            names.sort();
            names.dedup();
            let adjust: HashMap<String, f64> = self.config.devices.iter().map(|(k, v)| (k.to_lowercase(), v.power_adjust)).collect();
            let nvsmi = self.config.s("NVSMIpath");
            let tdp = self.config.f("PowerCPUtdp");
            device::update_device_information(&self.base, &mut self.all_devices, &names, &adjust, &nvsmi, tdp);
        }
        let rco = self.run_ctx();
        let Core { base, all_devices, cpu, devices, active_miners, .. } = self;
        let rc = rco.ctx(base, all_devices, cpu, &devices.devices_to_vendors);
        let mut st = UpdateStatus { updated: 0, failed: 0, exclusive_failed: 0 };
        for m in active_miners.iter_mut().filter(|m| m.best) {
            let status = m.get_status();
            if status == MinerStatus::Running && (!first_round || m.rounds > 0) {
                m.update_miner_data(&rc);
                let all_stats = m.base_algorithm.iter().all(|a| stat::get_stat(&format!("{}_{}_HashRate", m.name, a), &m.stats_sub).is_some());
                if !m.check_share_ratio() && all_stats {
                    log_info!("Too many rejected shares for miner {}", m.name);
                    m.reset_miner_data();
                } else if m.share_check > 0 && m.share_check < m.get_last_accepted_seconds() {
                    log_info!("Miner {} will restart because last accepted share is older than {} seconds", m.name, m.share_check);
                    m.restart = true;
                }
            }
            match status {
                MinerStatus::Running => st.updated += 1,
                MinerStatus::RunningFailed => {
                    m.reset_miner_data();
                    st.failed += 1;
                    if m.is_exclusive_miner {
                        st.exclusive_failed += 1;
                    }
                }
                _ => {}
            }
        }
        st
    }

    /// Set-Total
    fn set_total(base: &Path, config: &Config, m: &mut ActiveMiner) {
        let dir = base.join("Stats").join("Totals");
        let pool = m.pool.first().cloned().unwrap_or_default();
        let path = dir.join(format!("{}_Total.txt", pool));
        let csv = dir.join(format!("Totals_{}.csv", chrono::Local::now().format("%Y-%m-%d")));
        let dur = m.get_running_time(true);
        let days = dur.num_milliseconds() as f64 / 86400000.0;
        let cost = m.profit_cost.unwrap_or(0.0);
        let total_profit = (m.profit.unwrap_or(0.0) + if config.b("UsePowerPrice") && cost > 0.0 { cost } else { 0.0 }) * days;
        let total_cost = cost * days;
        let total_power = m.power_draw * days;
        let penalty = m.pool_penalty.first().copied().unwrap_or(0.0);
        let pf = 1.0 - penalty / 100.0;
        let profit_api = if pf > 0.0 { total_profit / pf } else { 0.0 };
        let now = Utc::now();
        let _ = std::fs::create_dir_all(&dir);
        if total_profit > 0.0 {
            let exists = csv.exists();
            let mut s = String::new();
            if !exists {
                s.push_str("\"Date\",\"Date_UTC\",\"PoolName\",\"Algorithm\",\"Currency\",\"Rate\",\"Profit\",\"ProfitApi\",\"Cost\",\"Power\",\"Penalty\",\"Duration\",\"Donation\"\n");
            }
            let row = [
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                now.format("%Y-%m-%d %H:%M:%S").to_string(),
                pool.clone(),
                m.base_algorithm.first().cloned().unwrap_or_default(),
                m.currency.join("+"),
                format!("{}", util::round_to(rates::get("USD").unwrap_or(0.0), 2)),
                format!("{}", util::round_to(total_profit * 1e8, 4)),
                format!("{}", util::round_to(profit_api * 1e8, 4)),
                format!("{}", util::round_to(total_cost * 1e8, 4)),
                format!("{}", util::round_to(total_power, 3)),
                format!("{}", penalty),
                format!("{}", util::round_to(dur.num_milliseconds() as f64 / 60000.0, 3)),
                if m.donator { "1".into() } else { "0".into() },
            ];
            s.push_str(&row.iter().map(|v| format!("\"{}\"", v)).collect::<Vec<_>>().join(","));
            s.push('\n');
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&csv) {
                let _ = f.write_all(s.as_bytes());
            }
        }
        let mut st = util::read_text(&path).ok().and_then(|t| jsonx::parse(&t)).filter(|v| !jsonx::get(v, "Duration").is_null());
        if let Some(v) = st.as_mut() {
            let add = |v: &mut Value, k: &str, x: f64| {
                let c = jsonx::num(jsonx::get(v, k));
                jsonx::set(v, k, json!(c + x));
            };
            add(v, "Duration", dur.num_milliseconds() as f64 / 60000.0);
            add(v, "Cost", total_cost);
            add(v, "Profit", total_profit);
            add(v, "ProfitApi", profit_api);
            add(v, "Power", total_power);
            jsonx::set(v, "Updated", json!(util::format_datetime(&now)));
        }
        let st = st.unwrap_or_else(|| {
            json!({
                "Pool": pool,
                "Duration": dur.num_milliseconds() as f64 / 60000.0,
                "Cost": total_cost,
                "Profit": total_profit,
                "ProfitApi": profit_api,
                "Power": total_power,
                "Started": util::format_datetime(&now),
                "Updated": util::format_datetime(&now),
            })
        });
        let _ = std::fs::write(&path, serde_json::to_string_pretty(&st).unwrap_or_default());
    }

    /// Set-MinerStats: returns false if a miner failed (with the watchdog)
    fn set_miner_stats(&mut self, span: Duration, use_watchdog: bool, quiet: bool) -> bool {
        let rco = self.run_ctx();
        let max_crashes = self.config.i("MaxCrashesDuringBenchmark");
        let stall = self.config.b("EnableStallDetection");
        let wd_limit = self.timer - Duration::seconds(self.watchdog_interval as i64);
        let mut failed_total = 0;
        let Core { base, all_devices, cpu, devices, active_miners, config, crash_counter, watchdog_timers, .. } = self;
        let rc = rco.ctx(base, all_devices, cpu, &devices.devices_to_vendors);
        for m in active_miners.iter_mut() {
            if m.new {
                m.new = m.base_algorithm.iter().any(|a| stat::get_stat(&format!("{}_{}_HashRate", m.name, a), &m.stats_sub).is_none());
            }
            if m.new {
                if m.benchmarked == 0 {
                    m.crash_count = 0;
                }
                m.benchmarked += 1;
            }
            if !(m.status == MinerStatus::Running || m.new) {
                continue;
            }
            let mut power = m.get_power_draw();
            let mut power_idle = false;
            let mut stalled = false;
            let mut ps = None;
            if stall && m.status == MinerStatus::Running && !m.device_name.iter().any(|d| d.to_uppercase().starts_with("CPU")) && !m.base_algorithm.iter().any(|a| a == "ProgPowEpic") {
                let s = device::device_power_state(base, all_devices, &m.device_name);
                power_idle = s.is_idle;
                ps = Some(s);
            }
            let mut statset = 0;
            let mut failed = false;
            let algos = m.algorithm.clone();
            for (i, algo) in algos.iter().enumerate() {
                let mut speed = m.get_hash_rate(algo, true);
                let diff = m.get_difficulty(algo);
                let benchmarking = m.is_benchmarking();
                if i < m.speed_live.len() {
                    m.speed_live[i] = speed;
                }
                if i == 0 {
                    if power_idle && speed > 0.0 {
                        m.power_idle_rounds += 1;
                    } else {
                        power_idle = false;
                        m.power_idle_rounds = 0;
                    }
                    if m.power_idle_rounds >= 3 {
                        stalled = true;
                    }
                }
                log_info!(
                    "{} {} {} on {}: {} samples / round {}",
                    m.base_name,
                    if benchmarking { "benchmarking" } else { "mining" },
                    algo,
                    m.device_model,
                    m.get_miner_data_count(),
                    if benchmarking { format!("{} / variance {:.2}%", m.benchmarked, m.variance.get(i).copied().unwrap_or(0.0) * 100.0) } else { m.rounds.to_string() }
                );
                if power_idle && speed > 0.0 && benchmarking && m.crash_count >= max_crashes {
                    speed = 0.0;
                    if i < m.speed_live.len() {
                        m.speed_live[i] = 0.0;
                    }
                }
                let mut st = None;
                if (!power_idle || speed == 0.0) && (speed > 0.0 || !benchmarking || m.crash_count >= max_crashes) {
                    let mut p = SetStat::new(format!("{}_{}_HashRate", m.name, miner::algorithm_base(algo)), speed, span);
                    p.difficulty = diff;
                    p.ratio = m.rejected_share_ratio.get(i).copied().unwrap_or(0.0);
                    p.fault_detection = true;
                    p.fault_tolerance = m.fault_tolerance;
                    p.power_draw = power;
                    p.sub = m.stats_sub.clone();
                    p.device_model = m.device_model.clone();
                    p.start_time = Some(m.start_time);
                    p.version = util::get_miner_version(&m.version).to_string();
                    p.log_file = m.log_file.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                    p.quiet = quiet || m.get_running_time(false) < Duration::seconds(30) || m.is_wrapper();
                    st = Some(stat::set_stat(p));
                    statset += 1;
                }
                let pool = m.pool.get(i).cloned().unwrap_or_default();
                if let Some(t) = watchdog_timers.iter_mut().find(|t| t.miner_name == m.name && t.pool_name == pool && &t.algorithm == algo) {
                    t.active = Utc::now();
                    match &st {
                        Some(s) if s.updated > t.kicked => t.kicked = s.updated,
                        _ => {
                            if benchmarking {
                                t.kicked = Utc::now();
                            } else if use_watchdog && t.kicked < wd_limit {
                                failed = true;
                            }
                        }
                    }
                }
                power = 0.0;
            }
            if statset == m.algorithm.len() {
                m.benchmarked = 0;
            }
            m.end_of_round_cleanup();
            write_activity(base, config, crash_counter, m, if stalled { 1 } else if failed { 2 } else { 0 });
            if stalled {
                let s = ps.unwrap_or_default();
                log_warn!(
                    "Miner {} mining {} on pool {} stalled: the GPUs draw {}W of {}W since {} rounds while a hashrate is reported - treated as crashed. ",
                    m.name,
                    m.algorithm.join("/"),
                    m.pool.join("/"),
                    s.power_draw.round(),
                    s.reference.round(),
                    m.power_idle_rounds
                );
                m.crash_count += 1;
                m.power_idle_rounds = 0;
                m.set_status(MinerStatus::Idle, &rc);
                failed_total += 1;
            } else if failed {
                m.set_status(MinerStatus::Failed, &rc);
                m.stopped = true;
                log_warn!("Miner {} mining {} on pool {} temporarily disabled. ", m.name, m.algorithm.join("/"), m.pool.join("/"));
                failed_total += 1;
            } else if !m.donator {
                Core::set_total(base, config, m);
            }
        }
        // profit stats per device
        if !self.benchmarking && !self.is_benchmarking_run {
            let use_pp = self.config.b("UsePowerPrice");
            let spike = self.config.f("ProfitSpikeProtection");
            let mut gpu_profit = 0.0;
            let mut gpu_power = 0.0;
            let mut gpu_count = 0;
            for model in self.config.device_model.clone().iter().filter(|m| !m.contains('-')) {
                let best = self
                    .active_miners
                    .iter()
                    .filter(|m| m.profit.is_some() && &m.device_model == model && !m.pool.iter().any(|p| p == "MiningRigRentals"))
                    .max_by(|a, b| a.profit.partial_cmp(&b.profit).unwrap_or(std::cmp::Ordering::Equal));
                let Some(b) = best else { continue };
                let mut p = b.profit.unwrap_or(0.0);
                if use_pp && b.profit_cost.unwrap_or(0.0) != 0.0 {
                    p += b.profit_cost.unwrap_or(0.0);
                }
                if p <= 0.0 {
                    continue;
                }
                if model != "CPU" {
                    gpu_profit += p;
                    gpu_power += b.power_draw;
                    gpu_count = gpu_count.max(b.device_name.len());
                }
                let names = self.devices.device_names.get(model).cloned().unwrap_or_default();
                let mut s = SetStat::new(format!("Profit-{}", names.join("-")), p, span);
                s.power_draw = b.power_draw;
                s.uplim_protection = spike;
                stat::set_stat(s);
            }
            if gpu_profit > 0.0 {
                let mut names: Vec<String> = self.devices.devices.iter().filter(|d| d.is_gpu()).map(|d| d.Name.clone()).collect();
                names.sort_by_key(|n| n.to_lowercase());
                if names.len() > gpu_count {
                    let mut s = SetStat::new(format!("Profit-{}", names.join("-")), gpu_profit, span);
                    s.power_draw = gpu_power;
                    s.uplim_protection = spike;
                    stat::set_stat(s);
                }
            }
        }
        failed_total == 0
    }

    /// publish the state for the API
    fn publish_api(&mut self, pools_map: &PoolsMap, new_pools: &[Pool], current_profit: f64, power_cpu: f64, power_gpu: f64, menu: &[String]) {
        let mut active = Vec::new();
        let mut running = Vec::new();
        let mut failed = Vec::new();
        for m in self.active_miners.iter_mut() {
            let j = m.to_api_json();
            if m.status == MinerStatus::Running || m.profit.map(|p| p != 0.0).unwrap_or(false) || m.is_focus_wallet_miner {
                active.push(j.clone());
            }
            if m.status == MinerStatus::Running {
                running.push(j.clone());
            }
            if m.status == MinerStatus::Failed {
                failed.push(j);
            }
        }
        let pools: Vec<Value> = pools_map
            .iter()
            .filter(|(k, p)| !miner::is_alternate_key(k) && (!p.SoloMining || p.BLK.is_some()))
            .map(|(_, p)| serde_json::to_value(p).unwrap_or(Value::Null))
            .collect();
        let mut algos: Vec<String> = new_pools.iter().map(|p| p.Algorithm.clone()).collect();
        algos.sort();
        algos.dedup();
        // CPU devices carry no OpenCL block in the original (the dashboard shows the L3 cache then)
        let dev_json = |d: &Device| {
            let mut v = serde_json::to_value(d).unwrap_or(Value::Null);
            if d.is_cpu() {
                if let Value::Object(m) = &mut v {
                    m.remove("OpenCL");
                }
            }
            v
        };
        let selected: HashSet<String> = self.devices.devices.iter().map(|d| d.Name.clone()).collect();
        let devices: Vec<Value> = self.all_devices.iter().filter(|d| selected.contains(&d.Name)).map(dev_json).collect();
        let all_devices: Vec<Value> = self.all_devices.iter().map(dev_json).collect();
        let wd: Vec<Value> = self
            .watchdog_timers
            .iter()
            .map(|t| json!({"MinerName": t.miner_name, "DeviceModel": t.device_model, "PoolName": t.pool_name, "Algorithm": t.algorithm, "Kicked": util::format_datetime(&t.kicked), "Active": util::format_datetime(&t.active)}))
            .collect();
        let cc: Vec<Value> = self
            .crash_counter
            .iter()
            .map(|c| {
                json!({"Timestamp": util::format_datetime(&c.timestamp), "Start": c.start.map(|s| util::format_datetime(&s)), "End": util::format_datetime(&c.end), "Runtime": c.runtime, "Name": c.name, "MinerName": c.miner_name, "Vendor": c.vendor, "Device": c.device, "Algorithm": c.algorithm, "Pool": c.pool})
            })
            .collect();
        let mut actual = serde_json::Map::new();
        for c in self.config.arr("Currency") {
            if let Some(r) = rates::get(&c) {
                actual.insert(c, json!(r));
            }
        }
        let speeds: serde_json::Map<String, Value> = self.miner_speeds.iter().map(|(k, v)| (k.clone(), json!({"Hashrate": v.hashrate, "Names": v.names, "Miner": v.miner}))).collect();
        let offset = util::round_to((power_cpu + power_gpu) * (self.config.f("PowerOffsetPercent") / 100.0) + self.config.f("PowerOffset"), 2);
        let cpuinfo = serde_json::to_value(&self.cpu).unwrap_or(Value::Null);
        let config = self.config.v.clone();
        let paused = self.is_paused();
        let combos = json!(self.devices.device_combos);
        api::with(|s| {
            s.active_miners = Value::Array(active);
            s.running_miners = Value::Array(running);
            s.failed_miners = Value::Array(failed);
            s.pools = Value::Array(pools);
            s.all_pools = Value::Array(new_pools.iter().map(|p| serde_json::to_value(p).unwrap_or(Value::Null)).collect());
            s.algorithms = json!(algos);
            s.devices = Value::Array(devices);
            s.all_devices = Value::Array(all_devices);
            s.device_combos = combos;
            s.config = config.clone();
            s.user_config = config;
            s.watchdog_timers = Value::Array(wd);
            s.crash_counter = Value::Array(cc);
            s.actual_rates = Value::Object(actual);
            s.current_profit = current_profit;
            s.current_power = json!({"CPU": util::round_to(power_cpu, 2), "GPU": util::round_to(power_gpu, 2), "Offset": offset});
            s.current_power_price = self.current_power_price;
            s.cmd_menu = menu.to_vec();
            s.pause = paused;
            s.pause_ia = self.pause_ia;
            s.pause_ia_only = self.pause_ia && !self.pause_scheduler;
            s.lock_miners_state = self.lock_miners;
            s.is_exclusive_run = self.is_exclusive_run;
            s.minerspeeds = Value::Object(speeds);
            s.memory_skipped = Value::Array(self.memory_skipped.clone());
            s.cpuinfo = cpuinfo;
            s.balances = Value::Array(self.balances.clone());
            s.earnings_avg = self.earnings_avg;
            s.earnings_1d = self.earnings_1d;
        });
    }

    /// one round of Invoke-Core; false = stop
    /// the remote data updates (Get-UnprofitableAlgos, Get-EthDAGSizes -EnableRemoteUpdate)
    fn update_databases(&mut self) {
        let data_dir = self.base.join("Data");
        for (url, file, cpu) in [
            ("https://api.rbminer.net/data/unprofitable3.json", "unprofitable.json", false),
            ("https://api.rbminer.net/data/unprofitable-cpu.json", "unprofitable-cpu.json", true),
        ] {
            if let Ok(v) = web::Cached::new(url).jobkey(file).cycletime(3600).json() {
                if !v.is_null() {
                    if cpu {
                        data::set_unprofitable_cpu(&v);
                    } else {
                        data::set_unprofitable(&v);
                    }
                    let _ = config::write_json(&data_dir.join(file), &v);
                }
            }
        }
        if self.round_counter > 0 && self.round_counter % 40 == 0 {
            if let Ok(v) = web::Cached::new("https://api.rbminer.net/data/ethdagsizes.json").jobkey("ethdagsizes").cycletime(3600).json() {
                if jsonx::count(&v) > 10 {
                    data::set_eth_dag_sizes(&v);
                    let _ = config::write_json(&data_dir.join("ethdagsizes.json"), &v);
                }
            }
        }
    }

    pub fn run_round(&mut self) -> bool {
        self.check_config();
        if !self.config.b("DisableInternetCheck") {
            let sites = self.config.arr("WebsitesForOnlineCheck");
            if !sites.is_empty() && !web::test_internet(&sites) {
                log_warn!("Internet is down - mining continues with the last known pools. ");
            }
        }
        self.update_databases();
        let sched = self.active_scheduler().cloned();
        self.current_power_price = self.config.f("PowerPrice");
        self.heat_control = (self.config.b("EnableMiningHeatControl"), self.config.f("MiningHeatControl"));
        self.pause_scheduler = false;
        if let Some(s) = &sched {
            self.current_power_price = s.power_price;
            self.heat_control = (s.enable_mining_heat_control, s.mining_heat_control);
            self.pause_scheduler = s.pause && !self.is_exclusive_run;
            log_info!("Scheduler profile {} currently active: DayOfWeek={}, From={}, To={}, Month={}", s.name, s.day_of_week.join(","), s.from, s.to, s.month.join(","));
        }
        self.timer = Utc::now();
        let round_span = match self.round_start {
            Some(rs) => self.timer - rs,
            None => Duration::seconds(self.config.i("BenchmarkInterval")),
        };
        self.round_start = Some(self.timer);
        let mut round_end = self.timer + Duration::seconds(self.current_interval);

        log_info!("Updating exchange rates. ");
        rates::update(&self.config.arr("Currency"), &[], &self.config.arr("ExcludeCurrencyRateFromCoinbase"));
        let ppc = {
            let c = self.config.s("PowerPriceCurrency");
            if c.is_empty() {
                self.config.arr("Currency").first().cloned().unwrap_or_else(|| "USD".into())
            } else {
                c
            }
        };
        self.current_power_price_btc = 0.0;
        self.fixed_cost_per_day_btc = 0.0;
        match rates::get(&ppc).filter(|r| *r > 0.0) {
            Some(r) => {
                self.fixed_cost_per_day_btc = self.config.f("FixedCostPerDay") / r;
                self.current_power_price_btc = self.current_power_price / r;
            }
            None => {
                if self.current_power_price > 0.0 {
                    log_warn!("Powerprice currency {} not found. Cost of electricity will be ignored.", ppc);
                }
            }
        }

        log_info!("Loading saved statistics. ");
        stat::load_miner_stats(&self.devices.devices_names);
        stat::clear_expired_auto_disabled(self.config.i("AutoDisableResetHours"));
        let disabled = stat::get_disabled();
        self.miner_speeds.retain(|_, v| v.names.iter().all(|n| stat::cached(n).is_some()));

        log_info!("Loading pool information. ");
        let (pools_map, new_pools) = self.select_pools(round_span, &disabled);

        // balances
        if self.config.b("ShowPoolBalances") {
            let refresh = api::take(|s| &mut s.cmd_update_balance) || self.balances_updated.map(|t| t < self.timer - Duration::minutes(self.config.i("BalanceUpdateMinutes").max(1))).unwrap_or(true);
            if refresh {
                log_info!("Getting pool balances. ");
                self.balances_updated = Some(self.timer);
            }
            let (b, avg, d1) = balances::get_balance(&self.base, &self.config, &pools::names(), refresh);
            if b.is_empty() {
                self.balances_updated = None;
            }
            self.balances = b;
            self.earnings_avg = avg;
            self.earnings_1d = d1;
        }

        log_info!("Getting miner information. ");
        let (miners, _before_wd, downloading, downloading_prq) = self.select_miners(&pools_map, &disabled);
        log_info!("Update active miners.");
        let (needing, message) = self.update_and_switch(miners.clone(), &pools_map, !new_pools.is_empty());
        if self.round_counter == 0 {
            log_info!("Starting mining operation ..");
        }
        self.switch_miners(&pools_map);

        self.benchmarking = !self.is_exclusive_run && needing > 0;
        if self.benchmarking {
            self.is_benchmarking_run = true;
        }
        let wd_reset = self.watchdog_reset as i64;
        crate::display::show_round(self, &miners, needing, downloading, downloading_prq, &message, !new_pools.is_empty(), wd_reset);

        // profit line
        let (mut profit, mut without_cost, mut pcpu, mut pgpu) = (0.0, 0.0, 0.0, 0.0);
        let use_pp = self.config.b("UsePowerPrice");
        for m in self.active_miners.iter().filter(|m| m.status == MinerStatus::Running) {
            profit += m.profit.unwrap_or(0.0);
            if use_pp {
                without_cost += m.profit_cost.unwrap_or(0.0);
            }
            if m.device_model == "CPU" {
                pcpu += m.power_draw;
            } else {
                pgpu += m.power_draw;
            }
        }
        if use_pp {
            profit -= self.config.f("PowerOffset") * 24.0 / 1000.0 * self.current_power_price_btc + self.fixed_cost_per_day_btc;
        }
        let line = crate::display::profit_line(self, profit, without_cost);
        crate::log::console_colored(&line, crate::log::Color::Cyan);
        crate::log::console(" ");
        if !self.memory_skipped.is_empty() {
            crate::log::console_colored(
                &format!(
                    "Not enough RAM for {} - this machine has {}GB. Set MinFreeMemoryGB=0 in config.txt to mine anyway.",
                    self.memory_skipped.iter().map(|m| format!("{} ({}GB)", jsonx::s(jsonx::get(m, "Algorithm")), jsonx::num(jsonx::get(m, "NeedGB")))).collect::<Vec<_>>().join(", "),
                    total_ram_gb()
                ),
                crate::log::Color::Yellow,
            );
        }

        // the interval
        let running = !self.devices.devices.is_empty() && !new_pools.is_empty() && !miners.is_empty();
        let preset = if running {
            if self.benchmarking || self.is_benchmarking_run {
                self.config.i("BenchmarkInterval")
            } else {
                self.config.i("Interval")
            }
        } else {
            self.config.i("Interval").min(self.config.i("BenchmarkInterval"))
        };
        let next = preset.max(self.current_interval + (Utc::now() - (round_end - Duration::seconds(20))).num_seconds());
        if next != self.current_interval {
            log_info!("Runtime interval changed from {} to {} seconds. ", self.current_interval, next);
            round_end = round_end + Duration::seconds(next - self.current_interval);
            self.current_interval = next;
        }
        self.watchdog_levels(false, if next > preset { next } else { 0 });
        let mut menu: Vec<String> = ["E[x]it", "[R]estart", "[B]alance update", "[S]kip SP", "[W]D reset", "Clear cach[e]"].iter().map(|s| s.to_string()).collect();
        menu.push(format!("[V]erbose{}", if self.ui_full { " off" } else { "" }));
        menu.push(format!("[P]ause{}", if self.pause_ia { " off" } else { "" }));
        if !self.is_exclusive_run {
            menu.push(if self.lock_miners { "Un[l]ock".into() } else { "[L]ock".into() });
        }
        self.publish_api(&pools_map, &new_pools, profit, pcpu, pgpu, &menu);
        let wait = (round_end - Utc::now()).num_seconds().max(0);
        log_info!("Start waiting {} seconds before next run. ", wait);
        crate::log::console(&format!("Waiting {}s until next run: {}", wait, menu.join(", ")));

        let (restart, miner_start) = self.wait_loop(round_end, needing);
        self.timer = Utc::now();
        if self.stopp || restart {
            self.set_miner_stats(self.timer - miner_start, false, true);
            return false;
        }
        log_info!("Saving hash rates. ");
        self.set_miner_stats(self.timer - miner_start, false, false);
        for m in self.active_miners.iter_mut().filter(|m| m.stopped) {
            m.stop_mining_post_cleanup();
        }
        if self.is_benchmarking_run && !self.benchmarking {
            self.is_benchmarking_run = false;
        }
        self.round_counter += 1;
        let _ = pools_map;
        true
    }

    /// the wait loop between two rounds; returns (restart requested, start of the stats window)
    fn wait_loop(&mut self, mut round_end: DateTime<Utc>, needing: usize) -> (bool, DateTime<Utc>) {
        let mut wait_round = 0u64;
        let mut samples = 0;
        let mut some_failed = false;
        let mut miner_start = self.timer;
        let mut restart = false;
        self.skip_switching_prevention = false;
        loop {
            let t_backup = Utc::now();
            if wait_round % 3 == 0 {
                let st = self.update_active_miners(wait_round == 0, wait_round % 12 == 0);
                if (st.updated == 0 && st.failed > 0) || st.exclusive_failed > 0 {
                    round_end = Utc::now();
                    log_warn!("{} miners crashed. Immediately restarting loop. ", if st.updated == 0 { "All" } else { "Exclusive" });
                } else if st.failed > 0 && !some_failed {
                    if !self.benchmarking && !self.is_benchmarking_run {
                        let elapsed = (Utc::now() - self.round_start.unwrap_or(self.timer)).num_seconds();
                        let nre = Utc::now() + Duration::seconds((self.config.i("BenchmarkInterval") - elapsed).max(0));
                        if nre < round_end {
                            round_end = nre;
                        }
                    }
                    some_failed = true;
                    log_warn!("{} miner{} crashed. Restarting loop asap. ", st.failed, if st.failed > 1 { "s" } else { "" });
                }
                samples += 1;
            }
            self.timer = Utc::now();
            if needing == 0 && (self.timer - miner_start).num_seconds() >= self.config.i("BenchmarkInterval") {
                log_info!("Saving hash rates. ");
                if !self.set_miner_stats(self.timer - miner_start, true, true) {
                    round_end = Utc::now();
                }
                miner_start = self.timer;
            }
            // commands: files, API, keys
            let mut key: Option<String> = None;
            if self.base.join("stopp.txt").exists() || api::take(|s| &mut s.cmd_stop) {
                key = Some("X".into());
            } else if api::take(|s| &mut s.cmd_pause) {
                key = Some("P".into());
            } else if api::take(|s| &mut s.cmd_lock_miners) {
                key = Some("L".into());
            } else if api::take(|s| &mut s.cmd_watchdog_reset) {
                key = Some("W".into());
            } else if api::take(|s| &mut s.cmd_clear_cache) {
                key = Some("E".into());
            } else {
                let ck = api::with(|s| std::mem::take(&mut s.cmd_key));
                if !ck.is_empty() {
                    key = Some(ck.to_uppercase());
                }
            }
            if key.is_none() {
                let elapsed = (Utc::now() - t_backup).num_milliseconds();
                let to = std::time::Duration::from_millis((1000 - elapsed).clamp(0, 1000) as u64);
                key = match util_term::poll_key(to) {
                    Some(Key::Char(c)) => Some(c.to_ascii_uppercase().to_string()),
                    Some(Key::Ctrl('c')) | Some(Key::Ctrl('C')) => Some("X".into()),
                    _ => None,
                };
            }
            if crate::INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) {
                key = Some("X".into());
            }
            let mut pressed = false;
            if let Some(k) = key {
                pressed = true;
                match k.as_str() {
                    "S" => {
                        self.skip_switching_prevention = true;
                        crate::log::console("[S] pressed - skip switching prevention in next run. ");
                    }
                    "N" => crate::log::console("[N] pressed - next run will start immediatly. "),
                    "X" => {
                        self.stopp = true;
                        crate::log::console("[X] pressed - stopping script.");
                    }
                    "D" => {
                        self.start_downloader = true;
                        crate::log::console("[D] pressed - starting downloader in next run. ");
                    }
                    "V" => {
                        self.ui_full = !self.ui_full;
                        crate::log::console(&format!("[V] pressed - UI will be set to {} in next run. ", if self.ui_full { "full" } else { "lite" }));
                    }
                    "P" => {
                        self.pause_ia = !self.pause_ia;
                        crate::log::console(&format!("[P] pressed - mining will be {}", if self.is_paused() { "PAUSED" } else { "UNPAUSED" }));
                    }
                    "L" => {
                        if !self.is_exclusive_run {
                            self.lock_miners = !self.lock_miners;
                            if self.lock_miners {
                                let mut v: Vec<String> = Vec::new();
                                for m in self.active_miners.iter().filter(|m| m.status == MinerStatus::Running) {
                                    for i in 0..m.pool.len() {
                                        let k = format!("{}-{}-{}", m.pool[i], m.base_algorithm.get(i).cloned().unwrap_or_default(), m.coin_symbol.get(i).cloned().unwrap_or_default());
                                        if !v.contains(&k) {
                                            v.push(k);
                                        }
                                    }
                                }
                                self.lock_pools = v;
                            }
                            crate::log::console(&format!("[L] pressed - switching will be {}", if self.lock_miners { "LOCKED" } else { "UNLOCKED" }));
                        }
                    }
                    "R" => {
                        restart = true;
                        crate::log::console("[R] pressed - restarting RainbowMiner.");
                    }
                    "W" => {
                        self.watchdog_timers.clear();
                        self.watchdog_levels(true, 0);
                        crate::log::console("[W] pressed - resetting WatchDog.");
                    }
                    "B" => {
                        self.balances_updated = None;
                        crate::log::console("[B] pressed - updating Balance.");
                    }
                    "E" => {
                        web::clear_cache();
                        crate::log::console("[E] pressed - cache cleared.");
                    }
                    "C" | "U" | "Y" | "Q" => {
                        crate::log::console(&format!("[{}] is not available in the Rust port.", k));
                        pressed = false;
                    }
                    _ => pressed = false,
                }
            }
            wait_round += 1;
            self.timer = Utc::now();
            if pressed || self.skip_switching_prevention || self.start_downloader || self.stopp || restart || self.timer >= round_end {
                break;
            }
        }
        if samples == 0 {
            self.update_active_miners(false, true);
        }
        crate::log::console("Finished waiting - starting next run ");
        (restart, miner_start)
    }

    /// Stop-Core: stop all miners
    pub fn stop(&mut self) {
        log_info!("Stopping all miners. ");
        let rco = self.run_ctx();
        let Core { base, all_devices, cpu, devices, active_miners, .. } = self;
        let rc = rco.ctx(base, all_devices, cpu, &devices.devices_to_vendors);
        for m in active_miners.iter_mut() {
            if m.get_status() == MinerStatus::Running || m.process.is_some() {
                log_info!("Stopping miner {}", m.name);
                m.set_status(MinerStatus::Idle, &rc);
            }
        }
        util_term::restore();
    }
}

//! The active miner object and the miner API classes of Modules/MinerAPIs.psm1
//! (status, start/stop, data samples, hashrate variance, benchmarking, share counters,
//! and the per-miner API readers).

use crate::device::{self, CpuInfo, Device};
use crate::jsonx;
use crate::miner::MinerCandidate;
use crate::proc::{MinerProcess, StartOptions};
use crate::stat;
use crate::util;
use crate::web;
use crate::{log_info, log_warn};
use chrono::{DateTime, Duration, Utc};
use indexmap::IndexMap;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinerStatus {
    Running,
    Idle,
    Failed,
    RunningFailed,
}

impl MinerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MinerStatus::Running => "Running",
            MinerStatus::Idle => "Idle",
            MinerStatus::Failed => "Failed",
            MinerStatus::RunningFailed => "RunningFailed",
        }
    }
    /// the numeric value ConvertTo-Json emits for the enum
    pub fn as_int(&self) -> i32 {
        match self {
            MinerStatus::Running => 0,
            MinerStatus::Idle => 1,
            MinerStatus::Failed => 2,
            MinerStatus::RunningFailed => 3,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Stratum {
    pub accepted: f64,
    pub rejected: f64,
    pub stale: f64,
    pub last_accepted_time: Option<DateTime<Utc>>,
    pub last_rejected_time: Option<DateTime<Utc>>,
    pub last_stale_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Sample {
    pub hashrate: HashMap<String, f64>,
    pub difficulty: HashMap<String, f64>,
    pub date: DateTime<Utc>,
    pub power_draw: f64,
    pub round: i64,
}

/// SrbMinerMulti autotune state ($this.MinerInfo)
#[derive(Debug, Clone, Default)]
pub struct SrbInfo {
    pub config_name: String,
    pub algo_count: usize,
    pub gpu_count: usize,
    pub tuning_done: bool,
    pub failed: bool,
    pub autotune: bool,
    pub autotune_check: bool,
    pub intensities: BTreeMap<String, i64>,
}

/// what the miners need from the session while running
#[allow(dead_code)]
pub struct RunCtx<'a> {
    pub base: &'a Path,
    pub devices: &'a [Device],
    pub cpu: &'a CpuInfo,
    pub devices_to_vendors: &'a HashMap<String, String>,
    pub niceness: Option<i32>,
}

#[allow(dead_code)]
pub struct ActiveMiner {
    pub name: String,
    pub version: String,
    pub base_name: String,
    pub path: PathBuf,
    pub arguments: String,
    pub argument_list: String,
    pub log_file_args: String,
    pub api: String,
    pub port: u16,
    pub algorithm: Vec<String>,
    pub base_algorithm: Vec<String>,
    pub currency: Vec<String>,
    pub coin_name: Vec<String>,
    pub coin_symbol: Vec<String>,
    pub wallet: Vec<String>,
    pub pool_penalty: Vec<f64>,
    pub blk: Vec<Option<f64>>,
    pub pool: Vec<String>,
    pub device_name: Vec<String>,
    pub device_model: String,
    pub profit: Option<f64>,
    pub profit_bias: f64,
    pub profit_unbias: f64,
    pub profit_cost: Option<f64>,
    pub profit_cost_bias: f64,
    pub power_draw: f64,
    pub speed: Vec<Option<f64>>,
    pub speed_live: Vec<f64>,
    pub speed_last: [f64; 2],
    pub variance: Vec<f64>,
    pub best: bool,
    pub new: bool,
    pub benchmarked: i64,
    pub benchmarked_offset: i64,
    pub log_file: PathBuf,
    pub show_miner_window: bool,
    pub msia_profile: i64,
    pub oc_profile: IndexMap<String, String>,
    pub enable_oc_profile: bool,
    pub dev_fee: IndexMap<String, f64>,
    pub fault_tolerance: f64,
    pub extend_interval: i64,
    pub skip_seconds: i64,
    pub penalty: f64,
    pub post_block_mining: i64,
    pub rounds: i64,
    pub min_samples: usize,
    pub zero_rounds: i64,
    pub power_idle_rounds: i64,
    pub crash_count: i64,
    pub max_benchmark_rounds: i64,
    pub max_rejected_share_ratio: f64,
    pub mining_priority: Option<i32>,
    pub mining_affinity: Option<String>,
    pub share_check: i64,
    pub static_port: u16,
    pub mem_size_gb: f64,
    pub manual_uri: String,
    pub rejected_share_ratio: Vec<f64>,
    pub data_interval: i64,
    pub exclude_pool_name: String,
    pub vendor: String,
    /// DevicesToVendors of the device model (stats sub folder prefix)
    pub stats_sub: String,
    pub env_vars: Vec<String>,
    pub executables: Vec<String>,
    pub priority_cpu: i32,
    pub priority_gpu: i32,
    pub priority_affinity: String,
    pub stopped: bool,
    pub donator: bool,
    pub restart: bool,
    pub is_focus_wallet_miner: bool,
    pub is_exclusive_miner: bool,
    pub is_locked: bool,
    pub is_running_first_rounds: bool,
    pub enable_auto_port: bool,
    pub no_cpu_mining: bool,
    pub needs_benchmark: bool,
    pub benchmark_only: bool,
    pub set_ld_library_path: bool,
    pub skip_wait_on_shutdown: bool,
    pub start_time: DateTime<Utc>,
    pub active_last: DateTime<Utc>,
    pub access_last: DateTime<Utc>,
    pub running_time: Duration,
    pub status: MinerStatus,
    pub active: Duration,
    pub activated: i64,
    pub interval_begin: Option<DateTime<Utc>>,
    pub start_port: u16,
    pub stratum: Vec<Stratum>,
    pub hashrate_regex: String,
    pub enabled: bool,
    pub process: Option<MinerProcess>,
    data: Vec<Sample>,
    srb: Option<SrbInfo>,
    custom_regex: Option<Regex>,
    regex_failed: bool,
    /// $this.Difficulty_Value of the EthminerWrapper/RHWrapper classes
    pub api_difficulty: f64,
    /// $this.MaxPowerDraw of the TTminerWrapper class
    pub api_max_power_draw: f64,
}

impl ActiveMiner {
    /// New-Object $Miner.API -Property @{...}
    pub fn from_candidate(c: &MinerCandidate) -> ActiveMiner {
        let n = c.hashrates.len();
        let pools: Vec<&crate::pool::Pool> = c.pools.values().collect();
        let now = Utc::now();
        ActiveMiner {
            name: c.name.clone(),
            version: c.version.clone(),
            base_name: c.base_name.clone(),
            path: c.path.clone(),
            arguments: c.arguments_str.clone(),
            argument_list: String::new(),
            log_file_args: c.log_file_args.clone(),
            api: c.api.clone(),
            port: c.port,
            algorithm: c.hashrates.keys().cloned().collect(),
            base_algorithm: c.base_algorithm.split('-').map(|s| s.to_string()).collect(),
            currency: pools.iter().map(|p| p.Currency.clone()).collect(),
            coin_name: pools.iter().map(|p| p.CoinName.clone()).collect(),
            coin_symbol: pools.iter().map(|p| p.CoinSymbol.clone()).collect(),
            wallet: pools.iter().map(|p| p.Wallet.clone()).collect(),
            pool_penalty: pools.iter().map(|p| p.Penalty).collect(),
            blk: pools.iter().map(|p| p.BLK).collect(),
            pool: pools.iter().map(|p| p.Name.clone()).collect(),
            device_name: c.device_name.clone(),
            device_model: c.device_model.clone(),
            profit: c.profit,
            profit_bias: c.profit_bias,
            profit_unbias: c.profit_unbias,
            profit_cost: c.profit_cost,
            profit_cost_bias: c.profit_cost_bias,
            power_draw: c.power_draw,
            speed: c.hashrates.values().cloned().collect(),
            speed_live: vec![0.0; n],
            speed_last: [0.0, 0.0],
            variance: vec![0.0; n],
            best: false,
            new: false,
            benchmarked: 0,
            benchmarked_offset: 0,
            log_file: PathBuf::new(),
            show_miner_window: c.show_miner_window,
            msia_profile: c.msia_profile,
            oc_profile: c.oc_profile.clone(),
            enable_oc_profile: false,
            dev_fee: c.dev_fee.clone(),
            fault_tolerance: c.fault_tolerance.unwrap_or(0.1),
            extend_interval: c.extend_interval,
            skip_seconds: c.skip_seconds.unwrap_or(5),
            penalty: c.penalty,
            post_block_mining: 0,
            rounds: 0,
            min_samples: c.min_samples.unwrap_or(3),
            zero_rounds: 0,
            power_idle_rounds: 0,
            crash_count: 0,
            max_benchmark_rounds: 3,
            max_rejected_share_ratio: c.max_rejected_share_ratio.unwrap_or(0.3),
            mining_priority: c.mining_priority,
            mining_affinity: c.mining_affinity.clone(),
            share_check: c.share_check,
            static_port: 0,
            mem_size_gb: 0.0,
            manual_uri: c.manual_uri.clone(),
            rejected_share_ratio: vec![0.0; n],
            data_interval: 60,
            exclude_pool_name: c.exclude_pool_name.clone(),
            vendor: String::new(),
            stats_sub: String::new(),
            env_vars: c.env_vars.clone(),
            executables: c.executables.clone(),
            priority_cpu: -2,
            priority_gpu: -1,
            priority_affinity: String::new(),
            stopped: false,
            donator: false,
            restart: false,
            is_focus_wallet_miner: false,
            is_exclusive_miner: false,
            is_locked: false,
            is_running_first_rounds: false,
            enable_auto_port: false,
            no_cpu_mining: c.no_cpu_mining,
            needs_benchmark: c.needs_benchmark(),
            benchmark_only: c.benchmark_only,
            set_ld_library_path: c.set_ld_library_path.unwrap_or(true),
            skip_wait_on_shutdown: false,
            start_time: util::min_datetime(),
            active_last: util::min_datetime(),
            access_last: now,
            running_time: Duration::zero(),
            status: MinerStatus::Idle,
            active: Duration::zero(),
            activated: 0,
            interval_begin: None,
            start_port: 0,
            stratum: vec![Stratum::default(); n],
            hashrate_regex: c.hashrate_regex.clone(),
            enabled: true,
            process: None,
            data: Vec::new(),
            srb: None,
            custom_regex: None,
            regex_failed: false,
            api_difficulty: 0.0,
            api_max_power_draw: 0.0,
        }
    }

    /// the key of the active miners index (algorithms sorted, pool-alternate suffix stripped)
    pub fn index_key(name: &str, path: &Path, arguments: &str, api: &str, algos: &[String]) -> String {
        let mut a: Vec<String> = algos.iter().map(|x| crate::miner::strip_alternate(x).to_lowercase()).collect();
        a.sort();
        format!("{}|{}|{}|{}|{}", name.to_lowercase(), path.display(), arguments, api.to_lowercase(), a.join("|"))
    }

    pub fn key(&self) -> String {
        Self::index_key(&self.name, &self.path, &self.arguments, &self.api, &self.algorithm)
    }

    pub fn is_wrapper(&self) -> bool {
        self.api.to_lowercase().contains("wrapper") || self.api == "Miner"
    }

    pub fn get_shutdown_url(&self) -> String {
        if self.api == "Trex" && cfg!(target_os = "linux") {
            format!("http://127.0.0.1:{}/control?command=shutdown", self.port)
        } else {
            String::new()
        }
    }

    pub fn get_vendor(&mut self, devices: &[Device]) -> String {
        if self.vendor.is_empty() {
            let models: Vec<&str> = self.device_model.split('-').collect();
            if let Some(d) = devices.iter().find(|d| models.contains(&d.Model.as_str())) {
                self.vendor = d.Vendor.clone();
            }
        }
        self.vendor.clone()
    }

    // ------------------------------------------------------------------------------------
    // arguments
    // ------------------------------------------------------------------------------------

    fn base_arguments(&self) -> String {
        self.arguments.replace("$mport", &self.port.to_string()).replace("$memsizegb", &format!("{}", self.mem_size_gb))
    }

    /// GetArguments (with the config file handling of XMRig6, SrbMinerMulti and BzMiner)
    pub fn get_arguments(&mut self, rc: &RunCtx) -> String {
        if let Some(a) = self.get_arguments_more() {
            return a;
        }
        match self.api.as_str() {
            "XMRig6" => self.xmrig6_arguments(rc),
            "SrbMinerMulti" => self.srb_arguments(),
            "BzMiner" | "BzMinerWrapper" => {
                let a = self.base_arguments();
                let re = Regex::new(r"--config\s+(\S+)").unwrap();
                if let Some(c) = re.captures(&a) {
                    if let Some(dir) = self.path.parent() {
                        let f = dir.join(&c[1]);
                        if std::fs::metadata(&f).map(|m| m.len() == 0).unwrap_or(true) {
                            if let Err(e) = std::fs::write(&f, "{}") {
                                log_warn!("Could not create {} for {}: {}", f.display(), self.name, e);
                            }
                        }
                    }
                }
                a
            }
            _ => self.base_arguments(),
        }
    }

    fn srb_arguments(&mut self) -> String {
        let args = self.arguments.replace("$mport", &self.port.to_string());
        if !(args.starts_with('{') && args.ends_with('}')) {
            return args;
        }
        let Some(params) = jsonx::parse(&args) else { return args };
        let dir = self.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let hwsig = jsonx::s(jsonx::get(&params, "HwSig"));
        let config_name = format!("{}_{}", self.base_algorithm.join("-"), hwsig);
        let config_fn = format!("config_{}.json", config_name);
        let config_file = dir.join(&config_fn);
        let algo_count = self.algorithm.len();
        let gpu_count = jsonx::count(jsonx::get(&params, "Devices"));
        if self.srb.is_none() {
            self.srb = Some(SrbInfo {
                config_name: config_fn.clone(),
                algo_count,
                gpu_count,
                autotune: jsonx::truthy(jsonx::get(&params, "AutoTune")),
                ..Default::default()
            });
        }
        if config_file.exists() {
            let miner_new = self.base_algorithm.iter().any(|a| stat::get_stat(&format!("{}_{}_HashRate", self.name, a), &self.vendor_sub_cached()).is_none());
            let raw = util::read_text(&config_file).ok().and_then(|t| jsonx::parse(&t));
            let info = self.srb.as_mut().unwrap();
            let bad = match &raw {
                None => true,
                Some(r) => {
                    (miner_new && self.benchmarked == 0)
                        || jsonx::num(jsonx::get(r, "algo_count")) as usize != info.algo_count
                        || jsonx::num(jsonx::get(r, "gpu_count")) as usize != info.gpu_count
                        || jsonx::s(jsonx::get(r, "config_name")) != info.config_name
                }
            };
            if bad {
                let _ = std::fs::remove_file(&config_file);
            } else if let Some(r) = raw {
                if let Value::Object(m) = jsonx::get(&r, "intensities") {
                    for (k, v) in m {
                        info.intensities.insert(k.clone(), jsonx::num(v) as i64);
                    }
                }
                info.tuning_done = jsonx::truthy(jsonx::get(&r, "tuning_done"));
                info.failed = false;
            }
        }
        let device_model = self.device_model.clone();
        let info = self.srb.as_mut().unwrap();
        info.autotune = jsonx::truthy(jsonx::get(&params, "AutoTune"));
        info.autotune_check = info.algo_count == 1 && device_model != "CPU";
        let base_args = jsonx::s(jsonx::get(&params, "Arguments"));
        let mut add = String::new();
        if info.tuning_done && !base_args.contains("--gpu-intensity") {
            for i in 0..info.algo_count {
                let list: Vec<Option<i64>> = (0..info.gpu_count).map(|j| info.intensities.get(&format!("{}|{}", i, j)).copied()).collect();
                if list.iter().all(|x| x.is_some()) {
                    add = format!(" --gpu-intensity {}", list.iter().map(|x| x.unwrap().to_string()).collect::<Vec<_>>().join(","));
                }
            }
        }
        format!("{}{}", base_args.trim(), add)
    }

    /// the vendor sub folder of the stats (set when the miner object is synced)
    fn vendor_sub_cached(&self) -> String {
        self.stats_sub.clone()
    }

    fn xmrig6_arguments(&mut self, rc: &RunCtx) -> String {
        let args = self.arguments.replace("$mport", &self.port.to_string());
        if !(args.starts_with('{') && args.ends_with('}')) {
            return args;
        }
        let Some(mut params) = jsonx::parse(&args) else { return args };
        let dir = self.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let hwsig = jsonx::s(jsonx::get(&params, "HwSig"));
        let affinity = jsonx::get(&params, "Affinity").clone();
        let threads = jsonx::num(jsonx::get(&params, "Threads"));
        let config_name = format!(
            "{}_{}{}_{}",
            self.base_algorithm.join("-"),
            hwsig,
            if affinity.is_null() { String::new() } else { format!("_{}", jsonx::s(&affinity)) },
            threads as i64
        );
        let config_fn = format!("config_{}.json", config_name);
        let threads_fn = format!("threads_{}.json", hwsig);
        let pools_json = serde_json::to_string(jsonx::get(&params, "Pools")).unwrap_or_default();
        let run_fn = format!("run_{}.json", util::md5_hex(&format!("{}{}", config_name, pools_json)));
        let config_file = dir.join(&config_fn);
        let threads_file = dir.join(&threads_fn);
        let run_file = dir.join(&run_fn);
        let log_file = format!("log_{}.txt", config_name);
        let algo = jsonx::s(jsonx::get(&params, "Algorithm"));
        let algo0 = algo.split('/').next().unwrap_or("").to_string();
        let codec = jsonx::s(jsonx::get(&params, "Codec"));
        let device = if !codec.is_empty() && codec != "auto" {
            codec
        } else {
            match jsonx::s(jsonx::get(&params, "Vendor")).as_str() {
                "AMD" | "INTEL" => "opencl".to_string(),
                "NVIDIA" => "cuda".to_string(),
                _ => "cpu".to_string(),
            }
        };
        let read = |p: &Path| util::read_text(p).ok().and_then(|t| jsonx::parse(&t));
        let cnt = |v: &Value| match v {
            Value::Null => 0,
            Value::Bool(false) => 0,
            Value::Array(a) => a.len(),
            _ => 1,
        };
        let result: anyhow::Result<()> = (|| {
            let mut threads_config = read(&threads_file);
            let has_algo = |tc: &Option<Value>| tc.as_ref().map(|t| cnt(jsonx::get(t, &algo)) > 0 || cnt(jsonx::get(t, &algo0)) > 0).unwrap_or(false);
            if !has_algo(&threads_config) {
                let mut init = jsonx::get(&params, "Config").clone();
                jsonx::set(&mut init, "pools", jsonx::get(&params, "Pools").clone());
                std::fs::write(&threads_file, serde_json::to_string_pretty(&init)?)?;
                let arg_sp = util::collapse_ws(&format!(
                    "--algo={} --config={} {} {}",
                    algo,
                    threads_fn,
                    jsonx::s(jsonx::get(&params, "DeviceParams")),
                    jsonx::s(jsonx::get(&params, "Params"))
                ));
                let opts = StartOptions {
                    file_path: self.path.clone(),
                    arguments: arg_sp,
                    log_path: Some(dir.join(&log_file)),
                    working_dir: dir.clone(),
                    priority: self.process_priority(),
                    vendor: self.get_vendor(rc.devices),
                    set_ld_library_path: self.set_ld_library_path,
                    ld_library_dir: Some(rc.base.join("IncludesLinux").join("lib")),
                    niceness: rc.niceness,
                    ..Default::default()
                };
                threads_config = None;
                match MinerProcess::start(&opts) {
                    Ok(mut p) => {
                        let wait = if device == "cpu" { 30 } else { 90 };
                        let t0 = std::time::Instant::now();
                        while t0.elapsed().as_secs() < wait && p.is_running() {
                            if let Some(v) = read(&threads_file) {
                                let sec = jsonx::get(&v, &device).clone();
                                if cnt(&sec) > 0 && !sec.is_boolean() {
                                    std::fs::write(&threads_file, serde_json::to_string_pretty(&sec)?)?;
                                    threads_config = Some(sec);
                                    break;
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                        p.stop(&format!("Miner {} (prerun)", self.name), "", false);
                    }
                    Err(e) => log_warn!("Prerun of {} failed: {}", self.name, e),
                }
                if threads_file.exists() && !has_algo(&threads_config) {
                    let _ = std::fs::remove_file(&threads_file);
                }
                threads_config = read(&threads_file);
            }
            let mut config = read(&config_file);
            if let (Some(c), Some(tc)) = (&config, &threads_config) {
                if !jsonx::get(c, &device).is_null() && jsonx::get(tc, &algo).is_string() && cnt(jsonx::get(jsonx::get(c, &device), &algo)) == 1 {
                    config = None;
                }
            }
            let cfg_has_device = config.as_ref().map(|c| jsonx::truthy(jsonx::get(c, &device))).unwrap_or(false);
            if !cfg_has_device {
                let tc = threads_config.clone().unwrap_or(Value::Null);
                if jsonx::truthy(jsonx::get(&tc, &algo)) || jsonx::truthy(jsonx::get(&tc, &algo0)) {
                    let mut dev = serde_json::Map::new();
                    let re0 = Regex::new(r"/0$").unwrap();
                    if let Value::Object(m) = &tc {
                        for (k, v) in m {
                            if !re0.is_match(k) && !v.is_array() {
                                dev.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    let mut algo_source = algo.clone();
                    let mut hops = 0;
                    while jsonx::get(&tc, &algo_source).is_string() && hops < 3 {
                        algo_source = jsonx::s(jsonx::get(&tc, &algo_source));
                        hops += 1;
                    }
                    let src = jsonx::get(&tc, &algo_source);
                    if src.is_string() || src.is_boolean() || cnt(src) == 0 {
                        algo_source = algo0.clone();
                    }
                    let a = if jsonx::truthy(jsonx::get(&tc, &algo)) { algo.clone() } else { algo0.clone() };
                    let source: Vec<Value> = jsonx::as_list(jsonx::get(&tc, &algo_source)).into_iter().cloned().collect();
                    let mut cfg = jsonx::get(&params, "Config").clone();
                    if device == "cpu" {
                        if a == "ghostrider" && rc.cpu.Threads > 0 {
                            dev.insert("max-threads-hint".into(), json!((100.0 * threads / rc.cpu.Threads as f64) as i64));
                        }
                        let mut rank: HashMap<i64, usize> = HashMap::new();
                        for c in rc.cpu.RealCores.iter().chain(rc.cpu.ThreadList.iter()) {
                            let n = rank.len();
                            rank.entry(*c as i64).or_insert(n);
                        }
                        let sort_key = |x: i64| rank.get(&x).map(|r| *r as i64).unwrap_or(1_000_000 + x);
                        let mut cix: HashMap<i64, Value> = HashMap::new();
                        let mut thr: Vec<i64> = source
                            .iter()
                            .map(|t| match t {
                                Value::Array(p) if p.len() == 2 => {
                                    let cpu = jsonx::num(&p[1]) as i64;
                                    cix.insert(cpu, p[0].clone());
                                    cpu
                                }
                                _ => jsonx::num(t) as i64,
                            })
                            .collect();
                        thr.sort_by_key(|x| sort_key(*x));
                        let take = if threads > 0.0 && (threads as usize) < source.len() { threads as usize } else { source.len() };
                        let mut list: Vec<i64> = thr.into_iter().take(take).collect();
                        list.sort();
                        if !affinity.is_null() && !jsonx::s(&affinity).is_empty() {
                            let aff: Vec<i64> = util::convert_from_cpu_affinity(&jsonx::s(&affinity)).into_iter().map(|x| x as i64).collect();
                            if !aff.is_empty() {
                                let mut aff_threads: Vec<i64> = aff.iter().copied().filter(|x| list.contains(x)).collect();
                                let tcount = aff.len().min(list.len());
                                if aff_threads.len() < tcount {
                                    let mut rest: Vec<i64> = aff.iter().copied().filter(|x| !aff_threads.contains(x)).collect();
                                    rest.sort_by_key(|x| sort_key(*x));
                                    let need = tcount - aff_threads.len();
                                    aff_threads.extend(rest.into_iter().take(need));
                                }
                                aff_threads.sort();
                                list = aff_threads;
                            }
                        }
                        let arr: Vec<Value> = if cix.is_empty() {
                            list.iter().map(|x| json!(x)).collect()
                        } else {
                            list.iter().map(|x| json!([cix.get(x).cloned().unwrap_or(json!(1)), x])).collect()
                        };
                        dev.insert(a.clone(), Value::Array(arr));
                        jsonx::set(&mut cfg, "cuda", json!({"enabled": false}));
                        jsonx::set(&mut cfg, "opencl", json!({"enabled": false}));
                    } else {
                        let devices: Vec<i64> = jsonx::as_list(jsonx::get(&params, "Devices")).iter().map(|v| jsonx::num(v) as i64).collect();
                        let sel: Vec<Value> = source.iter().filter(|t| devices.contains(&(jsonx::num(jsonx::get(t, "index")) as i64))).cloned().collect();
                        let mut arr = Vec::new();
                        for _ in 0..(threads.max(1.0) as usize) {
                            arr.extend(sel.iter().cloned());
                        }
                        dev.insert(a.clone(), Value::Array(arr));
                        jsonx::set(&mut cfg, "cpu", json!({"enabled": false}));
                        jsonx::set(&mut cfg, if device == "cuda" { "opencl" } else { "cuda" }, json!({"enabled": false}));
                    }
                    jsonx::set(&mut cfg, &device, Value::Object(dev));
                    jsonx::set(&mut cfg, "autosave", json!(false));
                    std::fs::write(&config_file, serde_json::to_string_pretty(&cfg)?)?;
                    jsonx::set(&mut params, "Config", cfg);
                } else {
                    log_warn!(
                        "Error parsing threads config file - cannot create miner config file ({} {{{}@{}}})",
                        self.name,
                        self.base_algorithm.join("-"),
                        self.pool.join("-")
                    );
                }
            }
            let cfg_mtime = std::fs::metadata(&config_file).and_then(|m| m.modified()).ok();
            let run_mtime = std::fs::metadata(&run_file).and_then(|m| m.modified()).ok();
            let stale = match (run_mtime, cfg_mtime) {
                (None, _) => true,
                (Some(r), Some(c)) => r < c,
                _ => false,
            };
            if stale {
                let mut run = read(&config_file).ok_or_else(|| anyhow::anyhow!("cannot read {}", config_file.display()))?;
                jsonx::set(&mut run, "pools", jsonx::get(&params, "Pools").clone());
                let pc = jsonx::get(&params, "Config").clone();
                for par in ["algo-perf", "rebench-algo", "bench-algo-time", "algo-min-time"] {
                    if let Value::Object(m) = &pc {
                        if let Some(v) = m.get(par) {
                            jsonx::set(&mut run, par, v.clone());
                        }
                    }
                }
                util::write_file_atomic(&run_file, serde_json::to_string_pretty(&run)?.as_bytes())?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            log_warn!(
                "Creating miner config files failed ({} {}@{}) [Error: '{}'].",
                self.base_name,
                self.base_algorithm.join("-"),
                self.pool.join("-"),
                e
            );
        }
        util::collapse_ws(&format!(
            "--algo={} --config={} {} {} {}",
            jsonx::s(jsonx::get(&params, "Algorithm")),
            run_fn,
            jsonx::s(jsonx::get(&params, "DeviceParams")),
            jsonx::s(jsonx::get(&params, "APIParams")),
            jsonx::s(jsonx::get(&params, "Params"))
        ))
    }

    // ------------------------------------------------------------------------------------
    // start / stop / status
    // ------------------------------------------------------------------------------------

    fn process_priority(&self) -> i32 {
        self.device_name
            .iter()
            .map(|d| if d.to_uppercase().starts_with("CPU") { self.priority_cpu } else { self.priority_gpu })
            .max()
            .unwrap_or(self.priority_gpu)
    }

    fn start_mining(&mut self, rc: &RunCtx) {
        self.stop_mining();
        self.status = MinerStatus::Failed;
        self.new = true;
        self.activated += 1;
        self.rounds = 0;
        self.power_idle_rounds = 0;
        self.interval_begin = None;
        self.benchmarked_offset = 0;
        if self.start_port == 0 {
            self.start_port = self.port;
        }
        if self.process.is_some() {
            return;
        }
        let miner_port = if self.static_port > 0 { self.static_port } else { self.start_port };
        self.port = miner_port;
        if self.enable_auto_port {
            let used = crate::proc::ports_in_use();
            if !used.is_empty() {
                let max = (self.port as u32 + 9999).min(65535) as u16;
                while self.port <= max && used.contains(&self.port) {
                    self.port = self.port.saturating_add(20);
                    if self.port == u16::MAX {
                        break;
                    }
                }
                if self.port > max {
                    self.port = miner_port;
                }
            }
        }
        log_info!(
            "Start mining {} on {}{} with miner {} using API on port {}",
            self.base_algorithm.first().cloned().unwrap_or_default(),
            self.pool.first().cloned().unwrap_or_default(),
            if self.base_algorithm.len() == 2 {
                format!(" and {} on {}", self.base_algorithm[1], self.pool.get(1).cloned().unwrap_or_default())
            } else {
                String::new()
            },
            self.base_name,
            self.port
        );
        self.mem_size_gb = rc
            .devices
            .iter()
            .filter(|d| d.is_gpu() && self.device_name.contains(&d.Name))
            .map(|d| d.OpenCL.GlobalMemSizeGB as f64)
            .fold(0.0, f64::max);
        let vendor = self.get_vendor(rc.devices);
        self.argument_list = self.get_arguments(rc);
        let now = chrono::Local::now();
        self.start_time = Utc::now();
        self.log_file = rc.base.join("Logs").join(format!("{}-{}_{}.txt", self.name, self.port, now.format("%Y-%m-%d_%H-%M-%S")));
        let mut pipe_to_log = true;
        if cfg!(windows) && !self.log_file_args.is_empty() {
            let la = self.log_file_args.replace("$logfile", &self.log_file.display().to_string());
            self.argument_list = format!("{} {}", self.argument_list, la).trim().to_string();
            pipe_to_log = false;
            log_info!("Miner {} writes its log file itself: {}", self.name, la);
        }
        let opts = StartOptions {
            file_path: self.path.clone(),
            arguments: self.argument_list.clone(),
            log_path: if pipe_to_log { Some(self.log_file.clone()) } else { None },
            working_dir: self.path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| rc.base.to_path_buf()),
            priority: self.process_priority(),
            cpu_affinity: if self.priority_affinity.is_empty() { vec![] } else { util::convert_from_cpu_affinity(&self.priority_affinity) },
            env_vars: self.env_vars.clone(),
            vendor,
            set_ld_library_path: self.set_ld_library_path,
            ld_library_dir: Some(rc.base.join("IncludesLinux").join("lib")),
            niceness: rc.niceness,
            capture: self.is_wrapper(),
        };
        match MinerProcess::start(&opts) {
            Ok(p) => {
                self.process = Some(p);
                self.status = MinerStatus::Running;
            }
            Err(e) => {
                log_warn!("Failed to start miner {}: {}", self.name, e);
            }
        }
    }

    fn stop_mining(&mut self) {
        self.status = MinerStatus::Failed;
        self.reset_miner_data();
        if self.process.is_some() {
            let url = self.get_shutdown_url();
            let title = format!("Miner {}", self.name);
            let skip = self.skip_wait_on_shutdown;
            if let Some(p) = self.process.as_mut() {
                p.stop(&title, &url, skip);
            }
            self.active = self.get_active_time();
            self.process = None;
            self.status = MinerStatus::Idle;
        }
    }

    fn start_mining_pre_process(&mut self) {
        let n = self.algorithm.len();
        self.stratum = vec![Stratum::default(); n];
        self.rejected_share_ratio = vec![0.0; n];
        self.active_last = Utc::now();
    }

    fn stop_mining_post_process(&mut self) {
        self.new = false;
        self.argument_list.clear();
    }

    pub fn stop_mining_post_cleanup(&mut self) {
        self.stopped = false;
    }

    pub fn get_status(&mut self) -> MinerStatus {
        let running = self.process.as_mut().map(|p| p.is_running()).unwrap_or(false);
        if running {
            return MinerStatus::Running;
        }
        if self.status == MinerStatus::Running {
            self.status = MinerStatus::RunningFailed;
        }
        self.status
    }

    pub fn set_status(&mut self, status: MinerStatus, rc: &RunCtx) {
        if status == self.get_status() {
            return;
        }
        match status {
            MinerStatus::Running => {
                self.start_mining_pre_process();
                self.start_mining(rc);
            }
            MinerStatus::Idle => {
                self.stop_mining();
                self.stop_mining_post_process();
            }
            _ => {
                self.stop_mining();
                self.stop_mining_post_process();
                self.status = status;
            }
        }
    }

    pub fn set_priorities(&mut self, cpu: i32, gpu: i32, affinity: &str) {
        self.priority_cpu = cpu.clamp(-2, 3);
        self.priority_gpu = gpu.clamp(-2, 3);
        self.priority_affinity = affinity.to_string();
    }

    pub fn set_static_port(&mut self, port: u16) {
        self.static_port = port;
    }

    pub fn process_id(&self) -> Option<u32> {
        self.process.as_ref().map(|p| p.pid)
    }

    // ------------------------------------------------------------------------------------
    // times
    // ------------------------------------------------------------------------------------

    pub fn get_active_start(&self) -> Option<DateTime<Utc>> {
        self.process.as_ref().map(|p| p.start_time)
    }

    pub fn get_active_last(&mut self) -> DateTime<Utc> {
        let Some(p) = self.process.as_mut() else { return self.active_last };
        if p.is_running() {
            Utc::now()
        } else {
            p.exit_time.unwrap_or(self.active_last)
        }
    }

    pub fn get_active_time(&mut self) -> Duration {
        let active = self.active;
        match self.process.as_mut() {
            Some(p) => {
                let end = if p.is_running() { Utc::now() } else { p.exit_time.unwrap_or_else(Utc::now) };
                active + (end - p.start_time)
            }
            None => active,
        }
    }

    pub fn get_running_time(&mut self, measure_interval: bool) -> Duration {
        let Some(p) = self.process.as_mut() else { return Duration::zero() };
        let running = p.is_running();
        let begin = if measure_interval { self.interval_begin.unwrap_or(p.start_time) } else { p.start_time };
        let end = if running { Utc::now() } else { p.exit_time.unwrap_or_else(Utc::now) };
        if measure_interval {
            self.interval_begin = Some(end);
        }
        end - begin
    }

    pub fn end_of_round_cleanup(&mut self) {
        if !self.is_wrapper() {
            if let Some(p) = &self.process {
                let _ = p.drain_output();
            }
        }
        if self.speed_live.iter().sum::<f64>() > 0.0 {
            self.zero_rounds = 0;
        } else {
            self.zero_rounds += 1;
        }
        self.rounds += 1;
        self.running_time = Utc::now() - self.start_time;
    }

    // ------------------------------------------------------------------------------------
    // shares
    // ------------------------------------------------------------------------------------

    pub fn update_shares(&mut self, index: usize, accepted: f64, rejected: f64, stale: f64) {
        if index >= self.stratum.len() {
            return;
        }
        let now = Utc::now();
        let s = &mut self.stratum[index];
        if s.accepted != accepted {
            s.last_accepted_time = Some(now);
        }
        if s.rejected != rejected {
            s.last_rejected_time = Some(now);
        }
        if s.stale != stale {
            s.last_stale_time = Some(now);
        }
        s.accepted = accepted;
        s.rejected = rejected;
        s.stale = stale;
        if accepted + rejected > 0.0 && index < self.rejected_share_ratio.len() {
            self.rejected_share_ratio[index] = util::round_to(rejected / (accepted + rejected), 4);
        }
    }

    pub fn get_share_count(&self, index: usize) -> i64 {
        self.stratum.get(index).map(|s| (s.accepted + s.rejected) as i64).unwrap_or(0)
    }

    pub fn get_rejected_share_ratio(&self, index: usize, min_shares: i64) -> f64 {
        let c = self.get_share_count(index);
        if c >= min_shares && c > 0 {
            self.stratum[index].rejected / c as f64
        } else {
            0.0
        }
    }

    pub fn get_max_rejected_share_ratio(&self) -> f64 {
        (0..self.algorithm.len()).map(|i| self.get_rejected_share_ratio(i, 10)).fold(0.0, f64::max)
    }

    pub fn check_share_ratio(&self) -> bool {
        self.max_rejected_share_ratio <= 0.0 || self.get_max_rejected_share_ratio() <= self.max_rejected_share_ratio
    }

    pub fn get_last_accepted_seconds(&self) -> i64 {
        match self.stratum.first().and_then(|s| s.last_accepted_time) {
            Some(t) => (Utc::now() - t).num_seconds(),
            None => -1,
        }
    }

    // ------------------------------------------------------------------------------------
    // data samples
    // ------------------------------------------------------------------------------------

    pub fn add_miner_data(&mut self, hashrate: HashMap<String, f64>, difficulty: HashMap<String, f64>, power_draw: f64, rc: &RunCtx) {
        let pd = if power_draw > 0.0 { power_draw } else { device::device_power_draw(rc.devices, &self.device_name) };
        self.data.push(Sample {
            hashrate,
            difficulty,
            date: Utc::now(),
            power_draw: pd,
            round: self.rounds,
        });
        self.active_last = Utc::now();
    }

    pub fn get_miner_data_count(&self) -> usize {
        self.data.len()
    }

    pub fn cleanup_miner_data(&mut self) {
        if self.data.len() > self.min_samples {
            let min_time = Utc::now() - Duration::seconds(self.data_interval * self.extend_interval.max(1) * 2);
            let mut i = 0;
            while i < self.data.len() {
                if self.data[i].date >= min_time || (self.data.len() - i) <= self.min_samples {
                    break;
                }
                i += 1;
            }
            if i > 0 {
                self.data.drain(0..i);
            }
        }
    }

    pub fn reset_miner_data(&mut self) {
        self.data.clear();
    }

    fn sample_value(m: &HashMap<String, f64>, algo: &str) -> Option<f64> {
        let base = algo.split('-').next().unwrap_or("");
        let v = m.get(algo).copied().filter(|v| *v != 0.0);
        if v.is_some() {
            return v;
        }
        if algo.contains('-') {
            m.get(base).copied().filter(|v| *v != 0.0)
        } else {
            None
        }
    }

    pub fn get_difficulty(&self, algo: &str) -> f64 {
        let tf = Utc::now() - Duration::seconds(self.data_interval * self.extend_interval.max(1));
        let vals: Vec<f64> = self.data.iter().filter(|d| d.date >= tf).filter_map(|d| Self::sample_value(&d.difficulty, algo)).collect();
        if vals.is_empty() {
            0.0
        } else {
            vals.iter().sum::<f64>() / vals.len() as f64
        }
    }

    pub fn is_benchmarking(&self) -> bool {
        let base = self.new && (self.benchmarked - self.benchmarked_offset) < (self.max_benchmark_rounds + self.extend_interval.max(1) - 1);
        if self.api == "SrbMinerMulti" {
            if let Some(i) = &self.srb {
                return base || (i.autotune && i.algo_count == 1 && !i.failed && !i.tuning_done && self.running_time.num_minutes() < 10);
            }
        }
        base
    }

    /// GetHashRate: the average over the data window with the variance gate
    pub fn get_hash_rate(&mut self, algo: &str, safe: bool) -> f64 {
        let intervals = self.extend_interval.max(1);
        let mut timeframe = Utc::now() - Duration::seconds(self.data_interval * intervals);
        let tf_min = self.start_time + Duration::seconds(self.skip_seconds);
        if timeframe < tf_min {
            timeframe = tf_min;
        }
        let hash_data: Vec<f64> = self.data.iter().filter(|d| d.date >= timeframe).filter_map(|d| Self::sample_value(&d.hashrate, algo)).collect();
        let max_variance = if self.fault_tolerance != 0.0 { self.fault_tolerance } else { 0.075 };
        let min_hr = 1.0 - (max_variance / 2.0).min(0.1);
        let max_hr = 1.0 + (max_variance * 2.0).min(0.1);
        let (mut count, mut average, mut variance) = (0usize, 0.0f64, 0.0f64);
        let steps = if self.rounds >= 2 * intervals { 1 } else { 2 };
        let mut step = 0;
        while !hash_data.is_empty() && step < steps {
            let mut bucket: Option<Vec<f64>> = None;
            for hr in &hash_data {
                let is_above = *hr > average * min_hr;
                let is_below = average == 0.0 || *hr < average * max_hr;
                if bucket.is_some() || (is_above && is_below) {
                    bucket.get_or_insert_with(Vec::new).push(*hr);
                }
            }
            let b = bucket.unwrap_or_default();
            count = b.len();
            average = if b.is_empty() { 0.0 } else { b.iter().sum::<f64>() / b.len() as f64 };
            variance = if average > 0.0 && count > 2 { util::get_sigma(&b) / average } else { 1.0 };
            log_info!(
                "{}: GetHashrate {} #{} smpl:{}, avg:{}, var:{}",
                self.name,
                algo,
                step,
                count,
                util::round_to(average, 2),
                util::round_to(variance, 3) * 100.0
            );
            step += 1;
        }
        if let Some(i) = self.algorithm.iter().position(|a| a == algo) {
            if i < self.variance.len() {
                self.variance[i] = variance;
            }
        }
        if safe && self.is_benchmarking() && ((self.benchmarked - self.benchmarked_offset) < intervals || count < self.min_samples || variance > max_variance) {
            0.0
        } else {
            average
        }
    }

    pub fn get_power_draw(&self) -> f64 {
        let tf = Utc::now() - Duration::seconds(self.data_interval * self.extend_interval.max(1));
        let v: Vec<f64> = self.data.iter().filter(|d| d.date >= tf && d.power_draw > 0.0).map(|d| d.power_draw).collect();
        if v.is_empty() {
            0.0
        } else {
            (v.iter().sum::<f64>() / v.len() as f64).round()
        }
    }

    // ------------------------------------------------------------------------------------
    // the miner APIs
    // ------------------------------------------------------------------------------------

    fn local_json(&self, path: &str) -> Option<Value> {
        match web::get_json(&format!("http://127.0.0.1:{}{}", self.port, path), 10) {
            Ok(v) => Some(v),
            Err(_) => {
                log_info!("Failed to connect to miner {}. ", self.name);
                None
            }
        }
    }

    pub(crate) fn local_json_pub(&self, path: &str) -> Option<Value> {
        self.local_json(path)
    }

    /// UpdateMinerData
    pub fn update_miner_data(&mut self, rc: &RunCtx) {
        if self.update_miner_data_more(rc) {
            return;
        }
        let api = self.api.clone();
        match api.as_str() {
            "Trex" => self.api_trex(rc),
            "Gminer" => self.api_gminer(rc),
            "Lol" => self.api_lol(rc),
            "Rigel" => self.api_rigel(rc),
            "BzMiner" | "BzMinerWrapper" => self.api_bzminer(rc),
            "OneZeroMiner" => self.api_onezero(rc),
            "SrbMinerMulti" => self.api_srb(rc),
            "XMRig6" | "Xmrig" | "XMRig" | "WildRig" => self.api_xmrig(rc),
            "Ccminer" => self.api_ccminer(rc),
            "CustomWrapper" => self.api_custom_wrapper(rc),
            _ => self.api_wrapper(rc),
        }
    }

    fn running(&mut self) -> bool {
        self.get_status() == MinerStatus::Running
    }

    fn algo(&self, i: usize) -> Option<String> {
        self.algorithm.get(i).cloned()
    }

    fn api_trex(&mut self, rc: &RunCtx) {
        if !self.running() {
            return;
        }
        let Some(d) = self.local_json("/summary") else { return };
        let mut hr = HashMap::new();
        let mut diff = HashMap::new();
        let power = jsonx::sum_field(jsonx::get(&d, "gpus"), "power");
        if let Some(a) = self.algo(0) {
            let mut v = jsonx::num(jsonx::get(&d, "hashrate"));
            let g = jsonx::sum_field(jsonx::get(&d, "gpus"), "hashrate");
            if v <= g * 0.6 {
                v = g;
            }
            if v > 0.0 {
                hr.insert(a.clone(), v);
                diff.insert(a.clone(), util::convert_from_hash(&jsonx::s(jsonx::path(&d, &["active_pool", "difficulty"]))));
                self.update_shares(0, jsonx::num(jsonx::get(&d, "accepted_count")), jsonx::num(jsonx::get(&d, "rejected_count")), 0.0);
            }
        }
        if self.algorithm.len() == 2 {
            let a = self.algorithm[1].clone();
            let ds = jsonx::get(&d, "dual_stat");
            let mut v = jsonx::num(jsonx::get(ds, "hashrate"));
            let g = jsonx::sum_field(jsonx::get(ds, "gpus"), "hashrate");
            if v <= g * 0.6 {
                v = g;
            }
            if v > 0.0 {
                hr.insert(a.clone(), v);
                diff.insert(a, util::convert_from_hash(&jsonx::s(jsonx::path(ds, &["active_pool", "difficulty"]))));
                self.update_shares(1, jsonx::num(jsonx::get(ds, "accepted_count")), jsonx::num(jsonx::get(ds, "rejected_count")), 0.0);
            }
        }
        self.add_miner_data(hr, diff, power, rc);
        self.cleanup_miner_data();
    }

    fn api_gminer(&mut self, rc: &RunCtx) {
        if !self.running() {
            return;
        }
        let Some(d) = self.local_json("/stat") else { return };
        let devs = jsonx::get(&d, "devices");
        let mut hr = HashMap::new();
        let power = jsonx::sum_field(devs, "power_usage");
        if let Some(a) = self.algo(0) {
            let v = jsonx::sum_field(devs, "speed");
            if v > 0.0 {
                hr.insert(a, v);
                self.update_shares(0, jsonx::sum_field(devs, "accepted_shares"), jsonx::sum_field(devs, "rejected_shares"), jsonx::sum_field(devs, "stale_shares"));
                if let Some(a2) = self.algo(1) {
                    let v2 = jsonx::sum_field(devs, "speed2");
                    if v2 > 0.0 {
                        hr.insert(a2, v2);
                        self.update_shares(1, jsonx::sum_field(devs, "accepted_shares2"), jsonx::sum_field(devs, "rejected_shares2"), jsonx::sum_field(devs, "stale_shares2"));
                    }
                }
            }
        }
        self.add_miner_data(hr, HashMap::new(), power, rc);
        self.cleanup_miner_data();
    }

    fn api_lol(&mut self, rc: &RunCtx) {
        if !self.running() {
            return;
        }
        let Some(d) = self.local_json("/summary") else { return };
        let power = jsonx::sum_field(jsonx::get(&d, "Workers"), "Power");
        let mut hr = HashMap::new();
        let n = jsonx::num(jsonx::get(&d, "Num_Algorithms")) as usize;
        let algos = jsonx::get(&d, "Algorithms");
        for i in 0..n {
            let a = jsonx::idx(algos, i);
            let v = jsonx::num(jsonx::get(a, "Total_Performance")) * jsonx::num(jsonx::get(a, "Performance_Factor"));
            match self.algo(i) {
                Some(name) if v > 0.0 => {
                    hr.insert(name, v);
                    self.update_shares(i, jsonx::num(jsonx::get(a, "Total_Accepted")), jsonx::num(jsonx::get(a, "Total_Rejected")), jsonx::num(jsonx::get(a, "Total_Stales")));
                }
                _ => {
                    if i > 0 {
                        hr.clear();
                    }
                }
            }
        }
        self.add_miner_data(hr, HashMap::new(), power, rc);
        self.cleanup_miner_data();
    }

    fn api_rigel(&mut self, rc: &RunCtx) {
        if !self.running() {
            return;
        }
        let Some(d) = self.local_json("/stat") else { return };
        let re = Regex::new(r"^(Ethash|KawPOW)(\d+|low|NH)").unwrap();
        let fix = |s: &str| -> String {
            if let Some(c) = re.captures(s) {
                c[1].to_string()
            } else if s == "BlocxAutolykos2" {
                "Autolykos2".into()
            } else {
                s.to_string()
            }
        };
        let mut ix0 = fix(self.base_algorithm.first().map(|s| s.as_str()).unwrap_or(""));
        let mut ix1 = if self.algorithm.len() > 1 { fix(self.base_algorithm.get(1).map(|s| s.as_str()).unwrap_or("")) } else { String::new() };
        let algo_str = jsonx::s(jsonx::get(&d, "algorithm"));
        for a in algo_str.split('+') {
            let n = crate::data::get_algorithm(a);
            if ix0 == n {
                ix0 = a.to_string();
            } else if !ix1.is_empty() && ix1 == n {
                ix1 = a.to_string();
            }
        }
        let rates = jsonx::get(&d, "hashrate");
        let mut v = jsonx::num(jsonx::get(rates, &ix0));
        let power = jsonx::num(jsonx::get(&d, "power_usage"));
        let mut zil = 0.0;
        if algo_str.contains("+zil") {
            zil = jsonx::num(jsonx::get(rates, "zil"));
            if zil > 0.0 && v <= 1.0 {
                v = self.speed_last[0];
            }
        }
        let mut hr = HashMap::new();
        if let Some(a0) = self.algo(0) {
            if v > 0.0 {
                self.speed_last[0] = v;
                hr.insert(a0, v);
                let st = jsonx::path(&d, &["solution_stat", &ix0]);
                self.update_shares(0, jsonx::num(jsonx::get(st, "accepted")), jsonx::num(jsonx::get(st, "rejected")), jsonx::num(jsonx::get(st, "invalid")));
                if !ix1.is_empty() {
                    let mut v1 = jsonx::num(jsonx::get(rates, &ix1));
                    if zil > 0.0 && v1 <= 1.0 {
                        v1 = self.speed_last[1];
                    }
                    self.speed_last[1] = v1;
                    if let Some(a1) = self.algo(1) {
                        if v1 > 0.0 {
                            hr.insert(a1, v1);
                            let st = jsonx::path(&d, &["solution_stat", &ix1]);
                            self.update_shares(1, jsonx::num(jsonx::get(st, "accepted")), jsonx::num(jsonx::get(st, "rejected")), jsonx::num(jsonx::get(st, "invalid")));
                        }
                    }
                }
            }
        }
        self.add_miner_data(hr, HashMap::new(), power, rc);
        self.cleanup_miner_data();
    }

    fn api_bzminer(&mut self, rc: &RunCtx) {
        if !self.running() {
            return;
        }
        let Some(d) = self.local_json("/status") else {
            self.cleanup_miner_data();
            return;
        };
        let mut count = self.algorithm.len();
        let pools = jsonx::as_list(jsonx::get(&d, "pools"));
        if pools.len() > count && pools.last().map(|p| jsonx::s(jsonx::get(p, "algorithm")) == "zil").unwrap_or(false) {
            count += 1;
        }
        let devices: Vec<&Value> = jsonx::as_list(jsonx::get(&d, "devices"))
            .into_iter()
            .filter(|x| {
                let st = jsonx::get(x, "status");
                jsonx::count(st) == count && jsonx::count(jsonx::get(x, "hashrate")) == count && jsonx::num(jsonx::idx(st, 0)) == 3.0
            })
            .collect();
        let sum_ix = |k: &str, i: usize| devices.iter().map(|x| jsonx::num(jsonx::idx(jsonx::get(x, k), i))).sum::<f64>();
        let power: f64 = devices.iter().map(|x| jsonx::num(jsonx::get(x, "power"))).sum();
        let mut hr = HashMap::new();
        if let Some(a0) = self.algo(0) {
            let v = sum_ix("hashrate", 0);
            if v > 0.0 {
                hr.insert(a0, v);
                let (acc, rej, sta) = (sum_ix("valid_solutions", 0), sum_ix("rejected_solutions", 0), sum_ix("stale_solutions", 0));
                self.update_shares(0, acc, rej, sta);
                if let Some(a1) = self.algo(1) {
                    let v1 = sum_ix("hashrate", 1);
                    if v1 > 0.0 {
                        hr.insert(a1, v1);
                        let (acc, rej, sta) = (sum_ix("valid_solutions", 1), sum_ix("rejected_solutions", 1), sum_ix("stale_solutions", 1));
                        self.update_shares(1, acc, rej, sta);
                    }
                }
            }
        }
        self.add_miner_data(hr, HashMap::new(), power, rc);
        self.cleanup_miner_data();
    }

    fn api_onezero(&mut self, rc: &RunCtx) {
        if !self.running() {
            return;
        }
        let Some(d) = self.local_json("") else { return };
        let power = jsonx::sum_field(jsonx::get(&d, "devices"), "power");
        let algos = jsonx::as_list(jsonx::get(&d, "algos"));
        let find = |base: &str| algos.iter().find(|a| crate::data::get_algorithm(&jsonx::s(jsonx::get(a, "name"))) == base).copied();
        let mut hr = HashMap::new();
        if let (Some(a0), Some(b0)) = (self.algo(0), self.base_algorithm.first().cloned()) {
            if let Some(x) = find(&b0) {
                let v = jsonx::num(jsonx::get(x, "total_hashrate"));
                if v > 0.0 {
                    hr.insert(a0, v);
                    self.update_shares(0, jsonx::num(jsonx::get(x, "total_accepted_shares")), jsonx::num(jsonx::get(x, "total_rejected_shares")), 0.0);
                    if let (Some(a1), Some(b1)) = (self.algo(1), self.base_algorithm.get(1).cloned()) {
                        if let Some(y) = find(&b1) {
                            let v1 = jsonx::num(jsonx::get(y, "total_hashrate"));
                            if v1 > 0.0 {
                                hr.insert(a1, v1);
                                self.update_shares(1, jsonx::num(jsonx::get(y, "total_accepted_shares")), jsonx::num(jsonx::get(y, "total_rejected_shares")), 0.0);
                            }
                        }
                    }
                }
            }
        }
        self.add_miner_data(hr, HashMap::new(), power, rc);
        self.cleanup_miner_data();
    }

    fn srb_check_autotune(&mut self) {
        let Some(info) = self.srb.as_mut() else { return };
        let (mut auto, mut fixed, mut done) = (0usize, 0usize, false);
        if let Ok(text) = std::fs::read(&self.log_file) {
            let text = String::from_utf8_lossy(&text);
            let re = Regex::new(r"GPU\d+\s*\|A0\|[^|]*\|(.*)$").unwrap();
            let re_auto = Regex::new(r"INTENSITY:\s*AUTO").unwrap();
            for line in text.lines().take(500) {
                if let Some(c) = re.captures(line) {
                    if re_auto.is_match(&c[1]) {
                        auto += 1;
                    } else {
                        fixed += 1;
                    }
                } else if line.contains("Finished init of worker threads") {
                    done = true;
                    break;
                }
            }
        }
        if auto + fixed > 0 && (done || auto + fixed >= info.gpu_count) {
            info.autotune_check = false;
            let at = auto > 0;
            if at != info.autotune {
                log_info!(
                    "{}: miner log shows {} ({}).",
                    self.name,
                    if at { "INTENSITY AUTO, waiting for the autotune results" } else { "a fixed intensity, hashrate is not held back for autotune" },
                    self.base_algorithm.join("-")
                );
            }
            info.autotune = at;
        } else if (Utc::now() - self.start_time).num_minutes() >= 3 {
            info.autotune_check = false;
        }
    }

    fn api_srb(&mut self, rc: &RunCtx) {
        if !self.running() {
            return;
        }
        let dual = self.algorithm.len() == 2;
        let typ = if self.device_model == "CPU" { "cpu" } else { "gpu" };
        let Some(r) = self.local_json("") else {
            if let Some(i) = self.srb.as_mut() {
                i.failed = true;
            }
            return;
        };
        if self.srb.as_ref().map(|i| i.autotune_check).unwrap_or(false) {
            self.srb_check_autotune();
        }
        let re = Regex::new(r"^(Ethash|KawPOW)(\d+|low|NH)").unwrap();
        let mut b0 = self.base_algorithm.first().cloned().unwrap_or_default();
        if let Some(c) = re.captures(&b0.clone()) {
            b0 = c[1].to_string();
        } else if b0 == "SCCPow" {
            b0 = "FiroPow".into();
        }
        let algos = jsonx::as_list(jsonx::get(&r, "algorithms"));
        let find = |b: &str| algos.iter().find(|a| crate::data::get_algorithm(&jsonx::s(jsonx::get(a, "name"))) == b).copied();
        let d0 = find(&b0);
        let mut v = d0.map(|x| jsonx::num(jsonx::path(x, &["hashrate", typ, "total"]))).unwrap_or(0.0);
        let gpus = jsonx::as_list(jsonx::get(&r, "gpu_devices"));
        if typ == "gpu" {
            if jsonx::num(jsonx::get(&r, "mining_time")) < 20.0 {
                v = 0.0;
                if let Some(i) = self.srb.as_mut() {
                    i.failed = false;
                }
            } else {
                if v > 0.0 {
                    let missing = algos.iter().any(|a| {
                        gpus.iter().any(|g| {
                            let dev = jsonx::s(jsonx::get(g, "device"));
                            !jsonx::truthy(jsonx::get(jsonx::path(a, &["hashrate", "gpu"]), &dev))
                        })
                    });
                    if missing {
                        v = 0.0;
                    }
                }
                if let Some(i) = self.srb.as_mut() {
                    i.failed = v == 0.0;
                }
            }
        }
        let power = if typ == "gpu" { gpus.iter().map(|g| jsonx::num(jsonx::get(g, "asic_power"))).sum() } else { 0.0 };
        let path_dir = self.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let mut tuning_finished = false;
        if let Some(info) = self.srb.as_mut() {
            if !gpus.is_empty() && !info.failed && !info.tuning_done {
                for i in 0..info.algo_count {
                    if i >= algos.len() {
                        break;
                    }
                    for (j, g) in gpus.iter().enumerate() {
                        let dev = jsonx::s(jsonx::get(g, "device"));
                        if dev.is_empty() {
                            continue;
                        }
                        let val = jsonx::num(jsonx::get(jsonx::get(algos[i], "gpu_autotune_results"), &dev)) as i64;
                        info.intensities.insert(format!("{}|{}", i, j), val);
                    }
                }
                let was = info.tuning_done;
                let expected = info.algo_count * info.gpu_count;
                info.tuning_done = info.intensities.len() >= expected && !info.intensities.values().any(|x| *x == 0);
                if !was && info.tuning_done {
                    let f = path_dir.join(&info.config_name);
                    let j = json!({
                        "config_name": info.config_name,
                        "algo_count": info.algo_count,
                        "gpu_count": info.gpu_count,
                        "tuning_done": info.tuning_done,
                        "failed": info.failed,
                        "autotune": info.autotune,
                        "autotune_check": info.autotune_check,
                        "intensities": info.intensities,
                    });
                    let _ = std::fs::write(&f, serde_json::to_string_pretty(&j).unwrap_or_default());
                    tuning_finished = true;
                }
                if info.autotune && !info.tuning_done {
                    v = 0.0;
                }
            }
        }
        if tuning_finished {
            self.benchmarked_offset = self.benchmarked;
        }
        let mut hr = HashMap::new();
        let mut diff = HashMap::new();
        if let Some(a0) = self.algo(0) {
            if v > 0.0 {
                hr.insert(a0.clone(), v);
                if let Some(x) = d0 {
                    diff.insert(a0, jsonx::num(jsonx::path(x, &["pool", "difficulty"])));
                    self.update_shares(0, jsonx::num(jsonx::path(x, &["shares", "accepted"])), jsonx::num(jsonx::path(x, &["shares", "rejected"])), 0.0);
                }
                if dual {
                    let b1 = self.base_algorithm.get(1).cloned().unwrap_or_default();
                    if let (Some(x), Some(a1)) = (find(&b1), self.algo(1)) {
                        let v1 = jsonx::num(jsonx::path(x, &["hashrate", typ, "total"]));
                        if v1 > 0.0 {
                            hr.insert(a1.clone(), v1);
                            diff.insert(a1, jsonx::num(jsonx::path(x, &["pool", "difficulty"])));
                            self.update_shares(1, jsonx::num(jsonx::path(x, &["shares", "accepted"])), jsonx::num(jsonx::path(x, &["shares", "rejected"])), 0.0);
                        }
                    }
                }
            }
        }
        self.add_miner_data(hr, diff, power, rc);
        self.cleanup_miner_data();
    }

    fn api_xmrig(&mut self, rc: &RunCtx) {
        if !self.running() {
            return;
        }
        let path = if self.api == "WildRig" { "" } else { "/api.json" };
        let Some(d) = self.local_json(path) else { return };
        let total = jsonx::path(&d, &["hashrate", "total"]);
        let mut v = jsonx::num(jsonx::idx(total, 0));
        if v == 0.0 {
            v = jsonx::num(jsonx::idx(total, 1));
        }
        if v == 0.0 {
            v = jsonx::num(jsonx::idx(total, 2));
        }
        let mut hr = HashMap::new();
        let mut diff = HashMap::new();
        if let Some(a) = self.algo(0) {
            if v > 0.0 {
                hr.insert(a.clone(), v);
                let res = jsonx::get(&d, "results");
                diff.insert(a, jsonx::num(jsonx::get(res, "diff_current")));
                let good = jsonx::num(jsonx::get(res, "shares_good"));
                self.update_shares(0, good, jsonx::num(jsonx::get(res, "shares_total")) - good, 0.0);
            }
        }
        self.add_miner_data(hr, diff, 0.0, rc);
        self.cleanup_miner_data();
    }

    fn api_ccminer(&mut self, rc: &RunCtx) {
        if !self.running() {
            return;
        }
        let resp = match web::tcp_request("127.0.0.1", self.port, "summary", 10, false, false) {
            Ok(r) => r,
            Err(_) => {
                log_info!("Failed to connect to miner {}. ", self.name);
                return;
            }
        };
        let mut data: HashMap<String, String> = HashMap::new();
        for part in resp.split(';') {
            if let Some((k, v)) = part.split_once('=') {
                data.insert(k.trim().to_uppercase(), v.trim().trim_end_matches(['|', '\0']).to_string());
            }
        }
        let num = |k: &str| data.get(k).and_then(|v| util::parse_f64(v)).unwrap_or(0.0);
        let hs = num("HS");
        let khs = num("KHS");
        let v = if data.contains_key("HS") && hs > khs { hs } else { khs * 1000.0 };
        let mut hr = HashMap::new();
        if let Some(a) = self.algo(0) {
            if v > 0.0 {
                hr.insert(a, v);
                self.update_shares(0, num("ACC"), num("REJ"), 0.0);
            }
        }
        self.add_miner_data(hr, HashMap::new(), 0.0, rc);
        self.cleanup_miner_data();
    }

    /// the base Miner.UpdateMinerData: parse "<value> <unit>/s" from the console output
    fn api_wrapper(&mut self, rc: &RunCtx) {
        let lines = self.process.as_ref().map(|p| p.drain_output()).unwrap_or_default();
        let re_num = Regex::new(r"^((?:\d*[\.,])?\d+)(.*)$").unwrap();
        for line in lines {
            let simple = util::strip_ansi(&line.replace(['\n', '\r'], ""));
            if simple.is_empty() || !simple.contains("/s") {
                continue;
            }
            let words: Vec<&str> = simple.split_whitespace().collect();
            let mut sum = 0.0;
            for w in words.iter().filter(|w| w.ends_with("/s")) {
                let index = words.iter().position(|x| x == w).unwrap_or(0);
                let (val, unit) = if let Some(c) = re_num.captures(words[index]) {
                    (util::parse_f64(&c[1].replace(',', ".")).unwrap_or(0.0), c[2].to_string())
                } else {
                    let prev = if index == 0 { words.last().copied().unwrap_or("") } else { words[index - 1] };
                    (util::parse_f64(&prev.replace(',', ".")).unwrap_or(0.0), words[index].to_string())
                };
                let u = unit.to_lowercase();
                let mult = if u.starts_with("kh/s") {
                    1e3
                } else if u.starts_with("mh/s") {
                    1e6
                } else if u.starts_with("gh/s") {
                    1e9
                } else if u.starts_with("th/s") {
                    1e12
                } else if u.starts_with("ph/s") {
                    1e15
                } else {
                    1.0
                };
                sum += val * mult;
            }
            if sum > 0.0 {
                if let Some(a) = self.algo(0) {
                    let mut hr = HashMap::new();
                    hr.insert(a, sum);
                    self.add_miner_data(hr, HashMap::new(), 0.0, rc);
                }
            }
        }
        self.cleanup_miner_data();
    }

    fn api_custom_wrapper(&mut self, rc: &RunCtx) {
        if self.hashrate_regex.is_empty() || self.regex_failed {
            return self.api_wrapper(rc);
        }
        if self.custom_regex.as_ref().map(|r| r.as_str() != format!("(?i){}", self.hashrate_regex)).unwrap_or(true) {
            match Regex::new(&format!("(?i){}", self.hashrate_regex)) {
                Ok(r) => self.custom_regex = Some(r),
                Err(e) => {
                    log_warn!("Custom miner {}: invalid HashRateRegex ({}), using the generic wrapper parser instead", self.base_name, e);
                    self.regex_failed = true;
                    return self.api_wrapper(rc);
                }
            }
        }
        let lines = self.process.as_ref().map(|p| p.drain_output()).unwrap_or_default();
        let rx = self.custom_regex.clone().unwrap();
        for line in lines {
            let simple = util::strip_ansi(&line.replace(['\n', '\r'], ""));
            if simple.is_empty() {
                continue;
            }
            let Some(m) = rx.captures(&simple) else { continue };
            let g = m.name("hashrate").or_else(|| m.name("value")).or_else(|| m.get(1));
            let mut h = g.and_then(|g| util::parse_f64(&g.as_str().replace(',', "."))).unwrap_or(0.0);
            let unit = m.name("unit").or_else(|| m.get(2)).map(|u| u.as_str().trim().to_lowercase()).unwrap_or_default();
            h *= match unit.chars().next() {
                Some('k') => 1e3,
                Some('m') => 1e6,
                Some('g') => 1e9,
                Some('t') => 1e12,
                Some('p') => 1e15,
                _ => 1.0,
            };
            if let (Some(a), Some(r)) = (m.name("accepted"), m.name("rejected")) {
                if let (Ok(a), Ok(r)) = (a.as_str().parse::<f64>(), r.as_str().parse::<f64>()) {
                    if !self.stratum.is_empty() {
                        self.update_shares(0, a, r, 0.0);
                    }
                }
            }
            if h > 0.0 {
                if let Some(a) = self.algo(0) {
                    let mut hr = HashMap::new();
                    hr.insert(a, h);
                    self.add_miner_data(hr, HashMap::new(), 0.0, rc);
                }
            }
        }
        self.cleanup_miner_data();
    }

    // ------------------------------------------------------------------------------------
    // API output
    // ------------------------------------------------------------------------------------

    pub fn to_api_json(&mut self) -> Value {
        let status = self.status;
        let active_time = self.get_active_time();
        let running_time = self.running_time;
        let stratum: Vec<Value> = self
            .stratum
            .iter()
            .map(|s| {
                json!({
                    "Accepted": s.accepted,
                    "Rejected": s.rejected,
                    "Stale": s.stale,
                    "LastAcceptedTime": s.last_accepted_time.map(|t| util::format_datetime(&t)),
                    "LastRejectedTime": s.last_rejected_time.map(|t| util::format_datetime(&t)),
                    "LastStaleTime": s.last_stale_time.map(|t| util::format_datetime(&t)),
                })
            })
            .collect();
        let fee: serde_json::Map<String, Value> = self.dev_fee.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        let oc: serde_json::Map<String, Value> = self.oc_profile.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        json!({
            "Name": self.name,
            "Version": self.version,
            "Path": self.path.to_string_lossy(),
            "Arguments": self.arguments,
            "ArgumentList": self.argument_list,
            "API": self.api,
            "Port": self.port,
            "Algorithm": self.algorithm,
            "BaseAlgorithm": self.base_algorithm,
            "Currency": self.currency,
            "CoinName": self.coin_name,
            "CoinSymbol": self.coin_symbol,
            "DeviceName": self.device_name,
            "DeviceModel": self.device_model,
            "Enabled": self.enabled,
            "Pool": self.pool,
            "Wallet": self.wallet,
            "Profit": self.profit,
            "Profit_Bias": self.profit_bias,
            "Profit_Unbias": self.profit_unbias,
            "Profit_Cost": self.profit_cost,
            "Profit_Cost_Bias": self.profit_cost_bias,
            "PowerDraw": self.power_draw,
            "Stratum": stratum,
            "Speed": self.speed,
            "Speed_Live": self.speed_live,
            "BLK": self.blk,
            "Variance": self.variance,
            "Best": self.best,
            "New": self.new,
            "Benchmarked": self.benchmarked,
            "LogFile": self.log_file.to_string_lossy(),
            "ShowMinerWindow": self.show_miner_window,
            "MSIAprofile": self.msia_profile,
            "OCprofile": oc,
            "EnableOCprofile": self.enable_oc_profile,
            "DevFee": fee,
            "BaseName": self.base_name,
            "FaultTolerance": self.fault_tolerance,
            "ExtendInterval": self.extend_interval,
            "Penalty": self.penalty,
            "PoolPenalty": self.pool_penalty,
            "PostBlockMining": self.post_block_mining,
            "Rounds": self.rounds,
            "MinSamples": self.min_samples,
            "CrashCount": self.crash_count,
            "MaxRejectedShareRatio": self.max_rejected_share_ratio,
            "ManualUri": self.manual_uri,
            "RejectedShareRatio": self.rejected_share_ratio,
            "Donator": self.donator,
            "IsFocusWalletMiner": self.is_focus_wallet_miner,
            "IsExclusiveMiner": self.is_exclusive_miner,
            "IsLocked": self.is_locked,
            "IsRunningFirstRounds": self.is_running_first_rounds,
            "NoCPUMining": self.no_cpu_mining,
            "NeedsBenchmark": self.needs_benchmark,
            "BenchmarkOnly": self.benchmark_only,
            "StartTime": util::format_datetime(&self.start_time),
            "RunningTime": util::format_timespan(running_time),
            "Status": status.as_int(),
            "StatusText": status.as_str(),
            "Active": util::format_timespan(active_time),
            "Activated": self.activated,
            "Vendor": self.vendor,
        })
    }
}

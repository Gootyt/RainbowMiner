//! Configuration (Modules/ConfigLib.psm1 + the config part of Invoke-Core):
//! config.txt with $Variable placeholders, pools.config.txt, miners.config.txt,
//! devices.config.txt, algorithms.config.txt, coins.config.txt, userpools.config.txt,
//! customminers.config.txt and combos.config.txt.
#![allow(dead_code)]

use crate::data;
use crate::jsonx;
use crate::params::{self, PType};
use crate::util;
use crate::{log_info, log_warn};
use indexmap::IndexMap;
use regex::{NoExpand, Regex};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const POOLS_CONFIG_DEFAULT: &str = include_str!("../assets/poolsconfigdefault.json");
pub const ALGORITHMS_CONFIG_DEFAULT: &str = include_str!("../assets/algorithmsconfigdefault.json");
pub const COINS_CONFIG_DEFAULT: &str = include_str!("../assets/coinsconfigdefault.json");
pub const DEVICES_CONFIG_DEFAULT: &str = include_str!("../assets/devicesconfigdefault.json");
pub const USERPOOLS_CONFIG_DEFAULT: &str = include_str!("../assets/userpoolsconfigdefault.json");
pub const CONFIG_DEFAULT: &str = include_str!("../assets/configdefault.json");

/// the "System.Object" fields of the pools config that are no payout currencies
const POOL_FIELDS_FIXED: &[&str] = &[
    "Worker", "Penalty", "Algorithm", "ExcludeAlgorithm", "CoinName", "ExcludeCoin", "CoinSymbol", "ExcludeCoinSymbol",
    "MinerName", "ExcludeMinerName", "FocusWallet", "AllowZero", "EnableAutoCoin", "EnablePostBlockMining", "CoinSymbolPBM",
    "DataWindow", "StatAverage", "StatAverageStable", "MaxMarginOfError", "SwitchingHysteresis", "MaxAllowedLuck",
    "MaxTimeSinceLastBlock", "MaxTimeToFind", "Region", "SSL", "BalancesKeepAlive", "Wallets", "DefaultMinerSwitchCoinSymbol",
    "ETH-Paymentmode", "MiningMode", "Params",
];

/// CPU facts needed for some defaults
#[derive(Debug, Clone, Default)]
pub struct CpuFacts {
    pub cores: usize,
    pub threads: usize,
    pub real_cores: Vec<usize>,
    pub thread_list: Vec<usize>,
}

impl CpuFacts {
    /// Get-CPUAffinity $Threads: the cpus to use for the given number of threads
    pub fn affinity_for(&self, threads: usize) -> Vec<usize> {
        let mut out: Vec<usize> = if threads > 0 && threads != self.real_cores.len() {
            if cfg!(windows) {
                let total = self.threads.max(1);
                let b = (total / self.cores.max(1)).max(1);
                let mut a = 0usize;
                let mut r = 0usize;
                let mut v = Vec::new();
                for _ in 0..threads.min(total) {
                    v.push(a);
                    let c = (a + b) % total;
                    if c < a {
                        r += 1;
                        a = c + r;
                    } else {
                        a = c;
                    }
                }
                v
            } else {
                self.real_cores
                    .iter()
                    .chain(self.thread_list.iter())
                    .take(threads)
                    .copied()
                    .collect()
            }
        } else {
            self.real_cores.clone()
        };
        out.sort_unstable();
        out
    }
}

/// Paths of all config files ($Session.ConfigFiles)
#[derive(Debug, Clone)]
pub struct ConfigFiles {
    pub dir: PathBuf,
    pub config: PathBuf,
    names: Vec<(String, PathBuf)>,
}

impl ConfigFiles {
    pub fn new(config_file: &Path) -> ConfigFiles {
        let dir = config_file.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("Config"));
        let fname = config_file.file_name().and_then(|f| f.to_str()).unwrap_or("config.txt").to_string();
        let mut names = Vec::new();
        for n in [
            "Pools", "Miners", "Devices", "Algorithms", "Coins", "OCProfiles", "Scheduler", "Userpools", "CustomMiners",
            "Combos", "GpuGroups", "MRR", "MRRAlgorithms", "Autoexec",
        ] {
            names.push((n.to_string(), dir.join(format!("{}.{}", n.to_lowercase(), fname))));
        }
        ConfigFiles {
            dir,
            config: config_file.to_path_buf(),
            names,
        }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        if name.eq_ignore_ascii_case("Config") {
            return self.config.clone();
        }
        self.names
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| self.dir.join(format!("{}.txt", name.to_lowercase())))
    }

    pub fn all(&self) -> Vec<(String, PathBuf)> {
        let mut v = vec![("Config".to_string(), self.config.clone())];
        v.extend(self.names.iter().cloned());
        v
    }
}

pub fn file_mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Set-ContentJson: pretty JSON, UTF-8 without BOM
pub fn write_json(path: &Path, v: &Value) -> std::io::Result<()> {
    let s = serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".into());
    util::write_file_atomic(path, s.as_bytes())
}

/// read a JSON config file (with $Var substitution if `params` is not empty)
pub fn read_config_content(path: &Path, params: &IndexMap<String, String>, conserve_unknown: bool) -> Result<Value, String> {
    let text = util::read_text(path).map_err(|e| e.to_string())?;
    let text = if params.is_empty() {
        text
    } else {
        substitute_params(&text, params, conserve_unknown)
    };
    serde_json::from_str::<Value>(text.trim()).map_err(|e| e.to_string())
}

/// Get-ConfigContent -Parameters: replace "$Name" by the value (longest names first, case insensitive)
pub fn substitute_params(text: &str, params: &IndexMap<String, String>, conserve_unknown: bool) -> String {
    let mut keys: Vec<&String> = params.keys().collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
    let mut result = text.to_string();
    for k in keys {
        let pat = format!(r"(?i)\${}", regex::escape(k));
        if let Ok(re) = Regex::new(&pat) {
            if re.is_match(&result) {
                // the value lands inside a JSON string: escape it
                let v = serde_json::to_string(&params[k]).unwrap_or_default();
                let v = &v[1..v.len() - 1];
                result = re.replace_all(&result, NoExpand(v)).into_owned();
            }
        }
    }
    if !conserve_unknown {
        static RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| Regex::new(r"(?i)\$[A-Z0-9_]+").unwrap());
        result = RE.replace_all(&result, "").into_owned();
    }
    result
}

/// "$($Parameters.$_)" for a JSON value (arrays were joined by ",")
fn value_to_param_string(v: &Value) -> String {
    match v {
        Value::Array(a) => a.iter().map(jsonx::s).collect::<Vec<_>>().join(","),
        Value::Bool(b) => if *b { "True".into() } else { "False".into() },
        other => jsonx::s(other),
    }
}

/// Get-StatAverage
pub fn get_stat_average(avg: &str, default: &str) -> String {
    let a: String = avg.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let l = a.to_lowercase();
    for x in ["Live", "Minute_5", "Minute_10", "Hour", "Day", "ThreeDay", "Week"] {
        if l == x.to_lowercase() {
            return x.to_string();
        }
    }
    match l.as_str() {
        "minute5" | "min5" | "min_5" | "5minute" | "5_minute" | "5" => "Minute_5".into(),
        "minute10" | "min10" | "min_10" | "10minute" | "10_minute" | "10" => "Minute_10".into(),
        "3day" | "3_day" | "three_day" => "ThreeDay".into(),
        _ => {
            if default.is_empty() {
                "Minute_10".into()
            } else {
                default.to_string()
            }
        }
    }
}

/// Get-YiiMPDataWindow
pub fn get_yiimp_data_window(dw: &str, default: &str) -> String {
    let l: String = dw.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    let r = match l.as_str() {
        "1" | "e1" | "e" | "ec" | "ecurrent" | "current" | "default" | "estimatecurrent" => "estimate_current",
        "2" | "e2" | "e24" | "e24h" | "last24" | "estimate24h" | "24h" | "estimatelast24h" => "estimate_last24h",
        "3" | "a2" | "a" | "a24" | "a24h" | "actual" | "actual24h" | "actuallast24h" => "actual_last24h",
        "4" | "min" | "min2" | "minimum" | "minimum2" => "minimum-2",
        "5" | "max" | "max2" | "maximum" | "maximum2" => "maximum-2",
        "6" | "avg" | "avg2" | "average" | "average2" => "average-2",
        "7" | "min3" | "minimum3" | "minall" | "minimumall" => "minimum-3",
        "8" | "max3" | "maximum3" | "maxall" | "maximumall" => "maximum-3",
        "9" | "avg3" | "average3" | "avgall" | "averageall" => "average-3",
        "10" | "mine" | "min2e" | "minimume" | "minimum2e" => "minimum-2e",
        "11" | "maxe" | "max2e" | "maximume" | "maximum2e" => "maximum-2e",
        "12" | "avge" | "avg2e" | "averagee" | "average2e" => "average-2e",
        "13" | "minh" | "min2h" | "minimumh" | "minimum2h" => "minimum-2h",
        "14" | "maxh" | "max2h" | "maximumh" | "maximum2h" => "maximum-2h",
        "15" | "avgh" | "avg2h" | "averageh" | "average2h" => "average-2h",
        _ => {
            return if default.is_empty() {
                "estimate_current".into()
            } else {
                default.to_string()
            }
        }
    };
    r.to_string()
}

// ----------------------------------------------------------------------------------------
// typed sub configs
// ----------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct PoolConfig {
    pub name: String,
    pub raw: Value,
    pub wallets: IndexMap<String, String>,
    pub params: IndexMap<String, String>,
    pub worker: String,
    pub penalty: f64,
    pub algorithm: Vec<String>,
    pub exclude_algorithm: Vec<String>,
    pub coin_name: Vec<String>,
    pub exclude_coin: Vec<String>,
    pub coin_symbol: Vec<String>,
    pub coin_symbol_pbm: Vec<String>,
    pub exclude_coin_symbol: Vec<String>,
    pub miner_name: Vec<String>,
    pub exclude_miner_name: Vec<String>,
    pub focus_wallet: Vec<String>,
    pub allow_zero: bool,
    pub enable_auto_coin: bool,
    pub enable_post_block_mining: bool,
    pub data_window: String,
    pub stat_average: String,
    pub stat_average_stable: String,
    pub max_margin_of_error: f64,
    pub switching_hysteresis: Option<f64>,
    pub max_allowed_luck: Option<f64>,
    pub max_time_since_last_block: Option<i64>,
    pub max_time_to_find: Option<i64>,
    pub region: Option<String>,
    pub ssl: i64,
    pub balances_keep_alive: Option<i64>,
    pub min_blk_rate: f64,
}

impl PoolConfig {
    /// any field of the pool section as string (API keys, AECurrency, User, ...)
    pub fn get_str(&self, key: &str) -> String {
        jsonx::s(jsonx::get(&self.raw, key)).trim().to_string()
    }
    pub fn get(&self, key: &str) -> &Value {
        jsonx::get(&self.raw, key)
    }
    pub fn has_miner_rules(&self) -> bool {
        !self.miner_name.is_empty() || !self.exclude_miner_name.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct AlgoConfig {
    pub penalty: f64,
    pub min_hashrate: f64,
    pub min_hashrate_solo: f64,
    pub min_workers: f64,
    pub max_time_to_find: i64,
    pub msia_profile: i64,
    pub oc_profile: String,
    pub min_blk_rate: f64,
    pub miner_name: Vec<String>,
    pub exclude_miner_name: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CoinConfig {
    pub penalty: f64,
    pub min_hashrate: f64,
    pub min_hashrate_solo: f64,
    pub min_workers: f64,
    pub max_time_to_find: i64,
    pub wallet: String,
    pub enable_auto_pool: bool,
    pub post_block_mining: i64,
    pub min_profit_percent: f64,
    pub min_blk_rate: f64,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceConfig {
    pub algorithm: Vec<String>,
    pub exclude_algorithm: Vec<String>,
    pub miner_name: Vec<String>,
    pub exclude_miner_name: Vec<String>,
    pub disable_dual_mining: bool,
    pub default_oc_profile: String,
    pub power_adjust: f64,
    pub worker: String,
    pub enable_lhr: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct MinerConfig {
    pub raw: Value,
}

impl MinerConfig {
    pub fn s(&self, k: &str) -> String {
        jsonx::s(jsonx::get(&self.raw, k)).trim().to_string()
    }
    pub fn has(&self, k: &str) -> bool {
        let v = jsonx::get(&self.raw, k);
        !(v.is_null() || (v.is_string() && jsonx::s(v).trim().is_empty()))
    }
    pub fn disable(&self) -> bool {
        util::get_yes(jsonx::get(&self.raw, "Disable"))
    }
}

/// a custom miner definition (customminers.config.txt), see Get-CustomMinerDefinitions
#[derive(Debug, Clone, Default)]
pub struct CustomMinerCommand {
    pub main_algorithm: String,
    pub algo: String,
    pub params: String,
    pub fee: Option<f64>,
    pub extend_interval: Option<i64>,
    pub min_mem_gb: f64,
    pub dag: bool,
    pub vendors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CustomMinerDef {
    pub name: String,
    pub enable: bool,
    pub manual_uri: String,
    pub version: String,
    pub vendors: Vec<String>,
    pub windows: (String, String),
    pub linux: (String, String),
    pub linux_arm: (String, String),
    pub api: String,
    pub hashrate_regex: String,
    pub port: u16,
    pub dev_fee: f64,
    pub arguments: String,
    pub ssl_arguments: String,
    pub enable_ssl: bool,
    pub device_index_property: String,
    pub device_index_hex: bool,
    pub device_separator: String,
    pub env_vars: Vec<String>,
    pub exclude_pool_name: String,
    pub show_miner_window: bool,
    pub extend_interval: i64,
    pub commands: Vec<CustomMinerCommand>,
}

// ----------------------------------------------------------------------------------------
// the main config
// ----------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Config {
    /// the normalized config.txt
    pub v: Value,
    pub pools: IndexMap<String, PoolConfig>,
    pub miners: HashMap<String, MinerConfig>,
    pub devices: HashMap<String, DeviceConfig>,
    pub algorithms: HashMap<String, AlgoConfig>,
    pub coins: HashMap<String, CoinConfig>,
    pub combos: HashMap<String, HashMap<String, bool>>,
    pub userpools: Vec<Value>,
    pub custom_miners: Vec<CustomMinerDef>,
    pub oc_profiles: Value,
    /// detected device models (set after device detection)
    pub device_model: Vec<String>,
    pub cuda_version: String,
    pub algorithm_miner_name: Vec<String>,
}

impl Config {
    pub fn get(&self, k: &str) -> &Value {
        jsonx::get(&self.v, k)
    }
    pub fn s(&self, k: &str) -> String {
        jsonx::s(self.get(k))
    }
    pub fn i(&self, k: &str) -> i64 {
        jsonx::num(self.get(k)).round() as i64
    }
    pub fn f(&self, k: &str) -> f64 {
        jsonx::num(self.get(k))
    }
    pub fn b(&self, k: &str) -> bool {
        util::get_yes(self.get(k))
    }
    pub fn arr(&self, k: &str) -> Vec<String> {
        jsonx::str_list(self.get(k))
    }
    pub fn set(&mut self, k: &str, v: Value) {
        jsonx::set(&mut self.v, k, v);
    }
    pub fn pool(&self, name: &str) -> Option<&PoolConfig> {
        self.pools.get(name).or_else(|| self.pools.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v))
    }
    pub fn algorithm(&self, a: &str) -> Option<&AlgoConfig> {
        self.algorithms.get(&a.to_lowercase())
    }
    pub fn coin(&self, c: &str) -> Option<&CoinConfig> {
        self.coins.get(&c.to_uppercase())
    }
    pub fn device(&self, model: &str) -> Option<&DeviceConfig> {
        self.devices.get(&model.to_lowercase())
    }
    pub fn miner(&self, key: &str) -> Option<&MinerConfig> {
        self.miners.get(&key.to_lowercase())
    }
}

/// Make sure config.txt exists and contains every parameter (missing ones as "$Name")
pub fn ensure_config_file(path: &Path, version: &str) -> Result<(), String> {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
        let _ = std::fs::create_dir_all(d.join("Backup"));
    }
    if !path.exists() {
        let mut m = Map::new();
        m.insert("VersionCompatibility".into(), json!(version));
        for p in params::PARAMS {
            if p.name == "SetupOnly" {
                continue;
            }
            m.insert(p.name.into(), json!(format!("${}", p.name)));
        }
        write_json(path, &Value::Object(m)).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let text = util::read_text(path).map_err(|e| e.to_string())?;
    let mut v: Value = serde_json::from_str(text.trim()).map_err(|e| format!("The main config file contains JSON errors: {}", e))?;
    let Value::Object(ref mut m) = v else {
        return Err("config.txt is not a JSON object".into());
    };
    let mut changed = false;
    if let Some(port) = m.get("LocalAPIport").cloned() {
        m.insert("APIport".into(), port);
    }
    for p in params::PARAMS {
        if p.name == "SetupOnly" {
            continue;
        }
        if !m.keys().any(|k| k.eq_ignore_ascii_case(p.name)) {
            m.insert(p.name.into(), json!(format!("${}", p.name)));
            changed = true;
        }
    }
    for old in [
        "API_ID", "API_Key", "UserName", "LocalAPIport", "RemoteAPI", "ConfigFile", "ExcludeNegativeProfit", "DisableAutoUpdate",
        "Regin", "Debug", "Verbose", "ErrorAction", "WarningAction", "InformationAction", "ErrorVariable", "WarningVariable",
        "InformationVariable", "OutVariable", "OutBuffer", "PipelineVariable",
    ] {
        let k = m.keys().find(|k| k.eq_ignore_ascii_case(old)).cloned();
        if let Some(k) = k {
            m.shift_remove(&k);
            changed = true;
        }
    }
    if changed {
        write_json(path, &v).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Default values plus PresetDefault/ConfigDefault overrides (the $Parameters of Get-ChildItemContent)
pub fn config_parameters(defaults: &IndexMap<String, Value>, cpu: &CpuFacts) -> IndexMap<String, String> {
    let setup: Value = serde_json::from_str(CONFIG_DEFAULT).unwrap_or(Value::Null);
    let mut out = IndexMap::new();
    for (k, v) in defaults {
        if k == "SetupOnly" {
            continue;
        }
        let mut val = v.clone();
        let s = jsonx::get(&setup, k);
        if !s.is_null() {
            val = match k.as_str() {
                "CPUMiningThreads" => json!(cpu.cores),
                "CPUMiningAffinity" => json!(util::convert_to_cpu_affinity_hex(&cpu.affinity_for(cpu.real_cores.len()))),
                "LinuxXAuthority" => json!(""),
                _ => s.clone(),
            };
        }
        out.insert(k.clone(), value_to_param_string(&val));
    }
    out
}

/// Load config.txt and normalize it (the "if ($CheckConfig)" block of Invoke-Core)
pub fn load_main_config(
    files: &ConfigFiles,
    defaults: &IndexMap<String, Value>,
    cpu: &CpuFacts,
) -> Result<Value, String> {
    let params = config_parameters(defaults, cpu);
    let mut v = read_config_content(&files.config, &params, false)
        .map_err(|e| format!("Your {} seems to be corrupt. Check for correct JSON format or delete it. ({})", files.config.display(), e))?;
    if !v.is_object() {
        return Err(format!("{} is not a JSON object", files.config.display()));
    }

    // backwards compatibility
    if !jsonx::get(&v, "Type").is_null() {
        let t = jsonx::get(&v, "Type").clone();
        jsonx::set(&mut v, "DeviceName", t);
        jsonx::set(&mut v, "ExcludeDeviceName", json!([]));
    }
    let ssl = jsonx::s(jsonx::get(&v, "SSL"));
    if !ssl.is_empty() && !["0", "1", "2"].contains(&ssl.as_str()) {
        jsonx::set(&mut v, "SSL", json!(util::get_yes_str(&ssl) as i64));
    }

    // fill nulls / missing values with their defaults, convert to the parameter types
    for (k, def) in defaults {
        if k == "SetupOnly" {
            continue;
        }
        let cur = jsonx::get(&v, k).clone();
        if cur.is_null() {
            jsonx::set(&mut v, k, def.clone());
            continue;
        }
        let Some(pt) = params::ptype_of(k) else { continue };
        let new = match pt {
            PType::Array => match &cur {
                Value::String(s) => Value::Array(util::split_config_array(s).into_iter().map(Value::String).collect()),
                Value::Array(_) => cur.clone(),
                other => json!([jsonx::s(other)]),
            },
            PType::Bool => match &cur {
                Value::Bool(_) => cur.clone(),
                other => json!(util::get_yes(other)),
            },
            PType::Int => match &cur {
                Value::Number(n) if n.is_i64() => cur.clone(),
                other => match jsonx::num_opt(other) {
                    Some(f) => json!(util::round_to(f, 0) as i64),
                    None => {
                        if !jsonx::s(other).trim().is_empty() {
                            log_warn!("Config {} is not an integer ({})", k, jsonx::s(other));
                        }
                        json!(0)
                    }
                },
            },
            PType::Double => match &cur {
                Value::Number(_) => json!(jsonx::num(&cur)),
                other => match jsonx::num_opt(other) {
                    Some(f) => json!(f),
                    None => {
                        if !jsonx::s(other).trim().is_empty() {
                            log_warn!("Config {} is not a floating point number ({})", k, jsonx::s(other));
                        }
                        json!(0.0)
                    }
                },
            },
            PType::Str => match &cur {
                Value::String(_) => cur.clone(),
                other => json!(jsonx::s(other)),
            },
        };
        jsonx::set(&mut v, k, new);
    }

    normalize_main_config(&mut v, cpu);
    Ok(v)
}

fn clamp_num(v: &Value, min: f64, max: f64) -> f64 {
    let s = jsonx::s(v).replace(',', ".");
    let s: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    s.parse::<f64>().unwrap_or(0.0).clamp(min, max)
}

fn normalize_main_config(v: &mut Value, cpu: &CpuFacts) {
    let variants = util::get_yes(jsonx::get(v, "EnableAlgorithmVariants"));
    let mut algos = data::get_algorithms_list(&jsonx::str_list(jsonx::get(v, "Algorithm")));
    let mut ex_algos = data::get_algorithms_list(&jsonx::str_list(jsonx::get(v, "ExcludeAlgorithm")));
    if variants {
        data::expand_algo_variants(&mut algos);
        data::expand_algo_variants(&mut ex_algos);
    }
    jsonx::set(v, "Algorithm", json!(algos));
    jsonx::set(v, "ExcludeAlgorithm", json!(ex_algos));

    let mut region = jsonx::s(jsonx::get(v, "Region"));
    if region.is_empty() {
        region = "US".into();
    }
    let region = data::get_region(&region);
    jsonx::set(v, "Region", json!(region));

    let mut dpr: Vec<String> = Vec::new();
    for r in jsonx::str_list(jsonx::get(v, "DefaultPoolRegion")) {
        let g = data::get_region(&r);
        if !g.is_empty() && !dpr.contains(&g) {
            dpr.push(g);
        }
    }
    let wider = data::get_region2(&region);
    if !wider.is_empty() {
        let mut n = wider.clone();
        for r in dpr {
            if !n.contains(&r) {
                n.push(r);
            }
        }
        dpr = n;
    }
    for r in ["US", "CentralEurope", "Asia", "Russia"] {
        let g = data::get_region(r);
        if !dpr.iter().any(|x| x.eq_ignore_ascii_case(&g)) {
            dpr.push(g);
        }
    }
    jsonx::set(v, "DefaultPoolRegion", json!(dpr));

    let mut cur: Vec<String> = jsonx::str_list(jsonx::get(v, "Currency")).into_iter().map(|c| c.to_uppercase()).collect();
    cur.push("BTC".into());
    let cur = util::unique_ci(cur);
    jsonx::set(v, "Currency", json!(cur));

    let ui = jsonx::s(jsonx::get(v, "UIstyle")).to_lowercase();
    jsonx::set(v, "UIstyle", json!(if ui.starts_with('f') { "full" } else { "lite" }));
    let uis = jsonx::s(jsonx::get(v, "UIsorting")).to_lowercase();
    jsonx::set(v, "UIsorting", json!(if uis.starts_with('p') { "profit" } else { "biased" }));
    let ppc = jsonx::s(jsonx::get(v, "PowerPriceCurrency")).to_uppercase();
    jsonx::set(v, "PowerPriceCurrency", json!(ppc));
    let ocl: Vec<String> = jsonx::str_list(jsonx::get(v, "OpenCLPlatformSorting"))
        .into_iter()
        .map(|x| x.to_uppercase())
        .filter(|x| ["AMD", "INTEL", "NVIDIA"].contains(&x.as_str()))
        .collect();
    jsonx::set(v, "OpenCLPlatformSorting", json!(ocl));

    let mhc = util::round_to(clamp_num(jsonx::get(v, "MiningHeatControl"), 0.0, 5.0), 1);
    jsonx::set(v, "MiningHeatControl", json!(mhc));
    let psh = clamp_num(jsonx::get(v, "PoolSwitchingHysteresis"), 0.0, 100.0);
    jsonx::set(v, "PoolSwitchingHysteresis", json!(psh));
    let msh = clamp_num(jsonx::get(v, "MinerSwitchingHysteresis"), 0.0, 100.0);
    jsonx::set(v, "MinerSwitchingHysteresis", json!(msh));
    let mer = clamp_num(jsonx::get(v, "MaxErrorRatio"), 1.0, f64::MAX);
    jsonx::set(v, "MaxErrorRatio", json!(mer));
    let pmm = clamp_num(jsonx::get(v, "PreferMinerMargin"), 0.0, 100.0);
    jsonx::set(v, "PreferMinerMargin", json!(pmm));

    let psa = get_stat_average(&jsonx::s(jsonx::get(v, "PoolStatAverage")), "");
    jsonx::set(v, "PoolStatAverage", json!(psa));
    let psas = get_stat_average(&jsonx::s(jsonx::get(v, "PoolStatAverageStable")), "Week");
    jsonx::set(v, "PoolStatAverageStable", json!(psas));
    let mtslb = util::convert_from_time(&jsonx::s(jsonx::get(v, "MaxTimeSinceLastBlock")));
    jsonx::set(v, "MaxTimeSinceLastBlock", json!(mtslb));
    for k in ["FastlaneBenchmarkTypeCPU", "FastlaneBenchmarkTypeGPU"] {
        let t = jsonx::s(jsonx::get(v, k));
        let t = if ["avg", "min", "max"].contains(&t.as_str()) { t } else { "avg".into() };
        jsonx::set(v, k, json!(t));
    }
    let mut rt = util::convert_from_time(&jsonx::s(jsonx::get(v, "RestartRBMTimespan")));
    if rt < 3600 {
        rt = 0;
    }
    jsonx::set(v, "RestartRBMTimespan", json!(rt));
    let mut rm = util::convert_from_bytes(&jsonx::s(jsonx::get(v, "RestartRBMMemory")));
    if rm < 367001600.0 {
        rm = 0.0;
    }
    jsonx::set(v, "RestartRBMMemory", json!(rm));

    if jsonx::num(jsonx::get(v, "BenchmarkInterval")) < 60.0 {
        jsonx::set(v, "BenchmarkInterval", json!(60));
    }
    let oc = jsonx::num(jsonx::get(v, "OCResetInterval"));
    if oc > 0.0 && oc < 600.0 {
        jsonx::set(v, "OCResetInterval", json!(600));
    }
    if jsonx::num(jsonx::get(v, "APIport")) <= 0.0 {
        jsonx::set(v, "APIport", json!(4000));
    }
    let mrs = jsonx::num(jsonx::get(v, "MaxRejectedShareRatio")).clamp(0.0, 1.0);
    jsonx::set(v, "MaxRejectedShareRatio", json!(mrs));
    let lmt = jsonx::s(jsonx::get(v, "LinuxMinerTerminal"));
    if !["auto", "screen", "tmux"].contains(&lmt.as_str()) {
        jsonx::set(v, "LinuxMinerTerminal", json!("auto"));
    }
    if !jsonx::get(v, "LegacyMode").is_null() {
        let lm = util::get_yes(jsonx::get(v, "LegacyMode"));
        jsonx::set(v, "MiningMode", json!(if lm { "legacy" } else { "device" }));
    }
    let mm = jsonx::s(jsonx::get(v, "MiningMode")).to_lowercase();
    let mm = if ["legacy", "device", "combo"].contains(&mm.as_str()) { mm } else { "combo".into() };
    jsonx::set(v, "MiningMode", json!(mm));

    // crosscheck the CPU mining parameters to avoid system overload
    let dn = jsonx::str_list(jsonx::get(v, "DeviceName"));
    if dn.iter().any(|d| d.to_uppercase().starts_with("CPU")) && cpu.threads > 0 {
        let all_mask: Vec<usize> = (0..cpu.threads).collect();
        let mut aff: Vec<usize> = util::convert_from_cpu_affinity(&jsonx::s(jsonx::get(v, "CPUMiningAffinity")))
            .into_iter()
            .filter(|c| *c < cpu.threads)
            .collect();
        if aff.is_empty() {
            let t = jsonx::num(jsonx::get(v, "CPUMiningThreads")) as usize;
            let threads = if t > 0 { t.min(cpu.threads) } else { cpu.real_cores.len() };
            aff = cpu.affinity_for(threads);
            log_warn!(
                "Parameter CPUMiningAffinity (config.txt) is empty or contains errors. Falling back to {}",
                util::convert_to_cpu_affinity_hex(&aff)
            );
        }
        if util::get_yes(jsonx::get(v, "EnableAutoAdjustAffinity")) && cpu.threads > 1 && aff == all_mask {
            let threads = cpu.threads - 2usize.min(cpu.threads / 2);
            aff = cpu.affinity_for(threads);
            log_warn!(
                "All threads selected for CPU mining! This will overload your system, auto-adjusting affinity to {}",
                util::convert_to_cpu_affinity_hex(&aff)
            );
        }
        jsonx::set(v, "CPUMiningAffinity", json!(util::convert_to_cpu_affinity_hex(&aff)));
        jsonx::set(v, "CPUMiningThreads", json!(aff.len().max(1)));
    }
}

// ----------------------------------------------------------------------------------------
// pools.config.txt
// ----------------------------------------------------------------------------------------

pub fn pools_config_default() -> Value {
    serde_json::from_str(POOLS_CONFIG_DEFAULT).unwrap_or(Value::Null)
}

/// the field names of all pools that are no wallets ($Global:GlobalPoolFields)
fn pool_fields() -> Vec<String> {
    let def = pools_config_default();
    let mut v: Vec<String> = POOL_FIELDS_FIXED.iter().map(|s| s.to_string()).collect();
    for p in jsonx::values(&def) {
        for k in jsonx::keys(jsonx::get(p, "Fields")) {
            if !v.iter().any(|x| x.eq_ignore_ascii_case(&k)) {
                v.push(k);
            }
        }
    }
    v
}

/// Set-PoolsConfigDefault: create / update pools.config.txt so that every pool module has a section
pub fn ensure_pools_config(path: &Path, pool_modules: &[String], userpool_names: &[String]) {
    let def = pools_config_default();
    let preset: Option<Value> = if path.exists() {
        match read_config_content(path, &IndexMap::new(), true) {
            Ok(v) if v.is_object() => Some(v),
            Ok(_) => None,
            Err(e) => {
                log_warn!("Your {} seems to be corrupt. Check for correct JSON format or delete it. ({})", path.display(), e);
                return;
            }
        }
    } else {
        None
    };
    let default_fields = json!({"Worker": "$WorkerName","Penalty": "0","Algorithm": "","ExcludeAlgorithm": "","CoinName": "","ExcludeCoin": "","CoinSymbol": "","ExcludeCoinSymbol": "","MinerName": "","ExcludeMinerName": "","FocusWallet": "","AllowZero": "0","EnableAutoCoin": "0","EnablePostBlockMining": "0","CoinSymbolPBM": "","DataWindow": "","StatAverage": "","StatAverageStable": "","MaxMarginOfError": "100","SwitchingHysteresis":"","MaxAllowedLuck":"","MaxTimeSinceLastBlock":"","MaxTimeToFind":"","Region":"","SSL":"","BalancesKeepAlive":""});

    let mut names: Vec<String> = pool_modules.iter().filter(|p| !p.eq_ignore_ascii_case("Userpools")).cloned().collect();
    for u in userpool_names {
        if !names.iter().any(|n| n.eq_ignore_ascii_case(u)) {
            names.push(u.clone());
        }
    }
    names.sort_by_key(|a| a.to_lowercase());

    let mut done = preset.clone().unwrap_or_else(|| json!({}));
    let mut changed = preset.is_none();
    for name in &names {
        let existing = preset
            .as_ref()
            .and_then(|p| p.as_object())
            .and_then(|m| m.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone()));
        let mut content = match existing {
            Some(c) => c,
            None => {
                changed = true;
                let mut c = json!({});
                if !name.eq_ignore_ascii_case("WhatToMine") {
                    let pd = jsonx::get(&def, name);
                    let currencies: Vec<String> = if userpool_names.iter().any(|u| u.eq_ignore_ascii_case(name)) {
                        vec!["BTC".into()]
                    } else if !pd.is_null() {
                        if jsonx::get(pd, "Fields").is_object() {
                            c = jsonx::get(pd, "Fields").clone();
                        }
                        jsonx::str_list(jsonx::get(pd, "Currencies"))
                    } else {
                        vec!["BTC".into()]
                    };
                    for cur in currencies {
                        jsonx::set(&mut c, &cur, json!(if cur == "BTC" { "$Wallet" } else { "" }));
                        jsonx::set(&mut c, &format!("{}-Params", cur), json!(""));
                    }
                }
                c
            }
        };
        let pd = jsonx::get(&def, name);
        if let Value::Object(f) = jsonx::get(pd, "Fields") {
            for (k, val) in f {
                if jsonx::get(&content, k).is_null() {
                    jsonx::set(&mut content, k, val.clone());
                    changed = true;
                }
            }
        }
        if let Value::Object(f) = &default_fields {
            for (k, val) in f {
                if jsonx::get(&content, k).is_null() {
                    jsonx::set(&mut content, k, val.clone());
                    changed = true;
                }
            }
        }
        if jsonx::truthy(jsonx::get(pd, "Autoexchange")) && util::get_yes(jsonx::get(&content, "EnableAutoCoin")) {
            jsonx::set(&mut content, "EnableAutoCoin", json!("0"));
            changed = true;
        }
        jsonx::set(&mut done, name, content);
    }
    if changed {
        if let Err(e) = write_json(path, &done) {
            log_warn!("Could not write to {}: {}", path.display(), e);
        }
    }
}

/// Get-PoolPayoutCurrencies
fn pool_payout_currencies(pool: &Value, fields: &[String]) -> IndexMap<String, String> {
    let mut out = IndexMap::new();
    if let Value::Object(m) = pool {
        let mut items: Vec<(&String, &Value)> = m.iter().collect();
        items.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        for (k, v) in items {
            let Value::String(s) = v else { continue };
            let s = s.trim();
            if s.len() <= 2 {
                continue;
            }
            if fields.iter().any(|f| f.eq_ignore_ascii_case(k)) || k.to_lowercase().ends_with("-params") || k.starts_with('#') {
                continue;
            }
            out.insert(k.clone(), s.to_string());
        }
    }
    out
}

fn pool_list_field(v: &Value, algo: bool) -> Vec<String> {
    let list: Vec<String> = match v {
        Value::String(s) => s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == ',' || *c == ';')
            .collect::<String>()
            .split([',', ';'])
            .filter(|x| !x.is_empty())
            .map(|x| x.to_string())
            .collect(),
        other => jsonx::str_list(other),
    };
    let mut out: Vec<String> = Vec::new();
    for x in list {
        let x = if algo { data::get_algorithm(&x) } else { x };
        if !x.is_empty() && !out.iter().any(|y| y.eq_ignore_ascii_case(&x)) {
            out.push(x);
        }
    }
    out.sort_by_key(|a| a.to_lowercase());
    out
}

/// Load pools.config.txt and normalize every pool (the "if ($CheckPools)" block)
pub fn load_pools_config(
    path: &Path,
    main: &Value,
    coins: &HashMap<String, CoinConfig>,
    pool_modules: &[String],
) -> Result<IndexMap<String, PoolConfig>, String> {
    let mut params: IndexMap<String, String> = IndexMap::new();
    params.insert("Wallet".into(), jsonx::s(jsonx::get(main, "Wallet")));
    params.insert("UserName".into(), jsonx::s(jsonx::get(main, "UserName")));
    params.insert("WorkerName".into(), jsonx::s(jsonx::get(main, "WorkerName")));
    params.insert("API_ID".into(), jsonx::s(jsonx::get(main, "API_ID")));
    params.insert("API_Key".into(), jsonx::s(jsonx::get(main, "API_Key")));
    for (sym, c) in coins {
        if !c.wallet.is_empty() && !params.keys().any(|k| k.eq_ignore_ascii_case(sym)) {
            params.insert(sym.clone(), c.wallet.clone());
        }
    }
    let raw = read_config_content(path, &params, false)
        .map_err(|e| format!("Your {} seems to be corrupt. Check for correct JSON format or delete it. ({})", path.display(), e))?;
    let fields = pool_fields();
    let variants = util::get_yes(jsonx::get(main, "EnableAlgorithmVariants"));
    let default_ssl = jsonx::num(jsonx::get(main, "SSL")) as i64;
    let pool_data_window = jsonx::s(jsonx::get(main, "PoolDataWindow"));
    let pool_stat_avg = jsonx::s(jsonx::get(main, "PoolStatAverage"));
    let pool_stat_avg_stable = jsonx::s(jsonx::get(main, "PoolStatAverageStable"));
    let wallet = jsonx::s(jsonx::get(main, "Wallet"));
    let worker_name = jsonx::s(jsonx::get(main, "WorkerName"));

    let mut out: IndexMap<String, PoolConfig> = IndexMap::new();
    let mut sections: Vec<(String, Value)> = match &raw {
        Value::Object(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => Vec::new(),
    };
    // pool modules without a section get a minimal one
    for p in pool_modules {
        if !sections.iter().any(|(k, _)| k.eq_ignore_ascii_case(p)) {
            log_info!("Alas! Missing pool {} will be added to config manually", p);
            sections.push((p.clone(), json!({"BTC": wallet, "Worker": worker_name})));
        }
    }

    for (name, mut v) in sections {
        if !v.is_object() {
            continue;
        }
        let mut pc = PoolConfig {
            name: name.clone(),
            ..Default::default()
        };
        pc.algorithm = pool_list_field(jsonx::get(&v, "Algorithm"), true);
        pc.exclude_algorithm = pool_list_field(jsonx::get(&v, "ExcludeAlgorithm"), true);
        pc.coin_name = pool_list_field(jsonx::get(&v, "CoinName"), false);
        pc.exclude_coin = pool_list_field(jsonx::get(&v, "ExcludeCoin"), false);
        pc.coin_symbol = pool_list_field(jsonx::get(&v, "CoinSymbol"), false);
        pc.coin_symbol_pbm = pool_list_field(jsonx::get(&v, "CoinSymbolPBM"), false);
        pc.exclude_coin_symbol = pool_list_field(jsonx::get(&v, "ExcludeCoinSymbol"), false);
        pc.miner_name = pool_list_field(jsonx::get(&v, "MinerName"), false);
        pc.exclude_miner_name = pool_list_field(jsonx::get(&v, "ExcludeMinerName"), false);
        let keys = jsonx::keys(&v);
        pc.focus_wallet = pool_list_field(jsonx::get(&v, "FocusWallet"), false)
            .into_iter()
            .filter(|f| keys.iter().any(|k| k.eq_ignore_ascii_case(f)))
            .collect();
        if variants {
            data::expand_algo_variants(&mut pc.algorithm);
            data::expand_algo_variants(&mut pc.exclude_algorithm);
        }

        // Allow*/Enable* fields are bools
        if let Value::Object(m) = &mut v {
            for (k, val) in m.iter_mut() {
                let l = k.to_lowercase();
                if (l.starts_with("allow") || l.starts_with("enable")) && !val.is_boolean() {
                    *val = json!(util::get_yes(val));
                }
            }
        }
        pc.allow_zero = util::get_yes(jsonx::get(&v, "AllowZero"));
        pc.enable_auto_coin = util::get_yes(jsonx::get(&v, "EnableAutoCoin"));
        pc.enable_post_block_mining = util::get_yes(jsonx::get(&v, "EnablePostBlockMining"));

        if pc.enable_auto_coin {
            let mut syms: Vec<(&String, &CoinConfig)> = coins.iter().filter(|(_, c)| c.enable_auto_pool && !c.wallet.is_empty()).collect();
            syms.sort_by(|a, b| a.0.cmp(b.0));
            for (sym, c) in syms {
                if jsonx::s(jsonx::get(&v, sym)).is_empty() {
                    jsonx::set(&mut v, sym, json!(c.wallet));
                }
            }
        }

        if let Value::Object(m) = &v {
            for (k, val) in m {
                if k.starts_with('#') {
                    continue;
                }
                if let Some(cur) = k.strip_suffix("-Params").or_else(|| k.strip_suffix("-params")) {
                    pc.params.insert(cur.to_string(), jsonx::s(val));
                }
            }
        }
        pc.wallets = pool_payout_currencies(&v, &fields);
        pc.worker = {
            let w = jsonx::s(jsonx::get(&v, "Worker"));
            if w.is_empty() {
                worker_name.clone()
            } else {
                w
            }
        };
        let num_clean = |s: &str| -> String { s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect() };
        pc.data_window = get_yiimp_data_window(&jsonx::s(jsonx::get(&v, "DataWindow")), &pool_data_window);
        pc.penalty = util::round_to(num_clean(&jsonx::s(jsonx::get(&v, "Penalty"))).parse::<f64>().unwrap_or(0.0), 2);
        let mmoe = num_clean(&jsonx::s(jsonx::get(&v, "MaxMarginOfError")));
        pc.max_margin_of_error = if jsonx::get(&v, "MaxMarginOfError").is_null() {
            if name.eq_ignore_ascii_case("NiceHash") {
                0.0
            } else {
                100.0
            }
        } else {
            util::round_to(mmoe.parse::<f64>().unwrap_or(0.0), 2)
        };
        let sh = num_clean(&jsonx::s(jsonx::get(&v, "SwitchingHysteresis")));
        pc.switching_hysteresis = sh.parse::<f64>().ok().map(|x| x.clamp(0.0, 100.0));
        pc.stat_average = get_stat_average(&jsonx::s(jsonx::get(&v, "StatAverage")), &pool_stat_avg);
        pc.stat_average_stable = get_stat_average(&jsonx::s(jsonx::get(&v, "StatAverageStable")), &pool_stat_avg_stable);
        let mal: String = jsonx::s(jsonx::get(&v, "MaxAllowedLuck")).chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        pc.max_allowed_luck = mal.parse::<f64>().ok().map(|x| x.max(0.0));
        let tclean = |s: &str| -> String { s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || "mhdw".contains(*c)).collect() };
        let mtslb = tclean(&jsonx::s(jsonx::get(&v, "MaxTimeSinceLastBlock")));
        pc.max_time_since_last_block = if mtslb.is_empty() { None } else { Some(util::convert_from_time(&mtslb)) };
        let mttf = tclean(&jsonx::s(jsonx::get(&v, "MaxTimeToFind")));
        pc.max_time_to_find = if mttf.is_empty() { None } else { Some(util::convert_from_time(&mttf)) };
        pc.min_blk_rate = match pc.max_time_to_find {
            Some(t) if t > 0 => 86400.0 / t as f64,
            _ => 0.0,
        };
        let reg = jsonx::s(jsonx::get(&v, "Region"));
        pc.region = if reg.trim().is_empty() { None } else { Some(data::get_region(&reg)) };
        let ssl = jsonx::s(jsonx::get(&v, "SSL"));
        pc.ssl = if ssl.trim().is_empty() {
            default_ssl
        } else if ["0", "1", "2"].contains(&ssl.trim()) {
            ssl.trim().parse::<i64>().unwrap_or(0)
        } else {
            util::get_yes_str(&ssl) as i64
        };
        let bka: String = jsonx::s(jsonx::get(&v, "BalancesKeepAlive")).chars().filter(|c| c.is_ascii_digit()).collect();
        pc.balances_keep_alive = if bka.is_empty() { None } else { Some(util::convert_from_time(&bka)) };
        pc.raw = v;
        out.insert(name, pc);
    }
    Ok(out)
}

// ----------------------------------------------------------------------------------------
// algorithms / coins / devices / miners / userpools / custom miners / combos
// ----------------------------------------------------------------------------------------

fn clean_num(v: &Value) -> f64 {
    let s: String = jsonx::s(v).chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    s.parse::<f64>().unwrap_or(0.0)
}

/// Set-AlgorithmsConfigDefault (only creates a missing file)
pub fn ensure_algorithms_config(path: &Path) {
    if path.exists() {
        return;
    }
    let setup: Value = serde_json::from_str(ALGORITHMS_CONFIG_DEFAULT).unwrap_or(json!({}));
    let default = json!({"Penalty": "0","MinHashrate": "0","MinHashrateSolo": "0","MinWorkers": "0","MaxTimeToFind": "0","MSIAprofile": 0,"OCprofile":"","MinerName":"","ExcludeMinerName":""});
    let mut out = Map::new();
    for a in data::all_algorithms() {
        let mut e = jsonx::get(&setup, &a).clone();
        if !e.is_object() {
            e = json!({});
        }
        if let Value::Object(d) = &default {
            for (k, val) in d {
                if jsonx::get(&e, k).is_null() {
                    jsonx::set(&mut e, k, val.clone());
                }
            }
        }
        out.insert(a, e);
    }
    let _ = write_json(path, &Value::Object(out));
}

pub fn load_algorithms_config(path: &Path, only: &[String]) -> (HashMap<String, AlgoConfig>, Vec<String>) {
    let mut out = HashMap::new();
    let mut with_miner_rules = Vec::new();
    let Ok(Value::Object(m)) = read_config_content(path, &IndexMap::new(), true) else {
        return (out, with_miner_rules);
    };
    for (a, v) in m {
        if !only.is_empty() && !only.iter().any(|x| x.eq_ignore_ascii_case(&a)) {
            continue;
        }
        let mttf = util::convert_from_time(&jsonx::s(jsonx::get(&v, "MaxTimeToFind")));
        let ac = AlgoConfig {
            penalty: util::round_to(clean_num(jsonx::get(&v, "Penalty")), 2),
            min_hashrate: util::convert_from_hash(&jsonx::s(jsonx::get(&v, "MinHashrate"))),
            min_hashrate_solo: util::convert_from_hash(&jsonx::s(jsonx::get(&v, "MinHashrateSolo"))),
            min_workers: util::convert_from_hash(&jsonx::s(jsonx::get(&v, "MinWorkers"))),
            max_time_to_find: mttf,
            msia_profile: jsonx::num(jsonx::get(&v, "MSIAprofile")) as i64,
            oc_profile: jsonx::s(jsonx::get(&v, "OCProfile")).trim().to_string(),
            min_blk_rate: if mttf > 0 { 86400.0 / mttf as f64 } else { 0.0 },
            miner_name: jsonx::str_list(jsonx::get(&v, "MinerName")),
            exclude_miner_name: jsonx::str_list(jsonx::get(&v, "ExcludeMinerName")),
        };
        if !ac.miner_name.is_empty() || !ac.exclude_miner_name.is_empty() {
            with_miner_rules.push(a.clone());
        }
        out.insert(a.to_lowercase(), ac);
    }
    (out, with_miner_rules)
}

/// Set-CoinsConfigDefault (only creates a missing file)
pub fn ensure_coins_config(path: &Path) {
    if path.exists() {
        return;
    }
    let setup: Value = serde_json::from_str(COINS_CONFIG_DEFAULT).unwrap_or(json!({}));
    let default = json!({"Penalty": "0","MinHashrate": "0","MinHashrateSolo": "0","MinWorkers": "0","MaxTimeToFind":"0","PostBlockMining":"0","MinProfitPercent":"0","Wallet":"","EnableAutoPool":"0","Comment":""});
    let mut out = Map::new();
    let mut keys = jsonx::keys(&setup);
    keys.sort();
    for c in keys {
        let mut e = jsonx::get(&setup, &c).clone();
        if let Value::Object(d) = &default {
            for (k, val) in d {
                if jsonx::get(&e, k).is_null() {
                    jsonx::set(&mut e, k, val.clone());
                }
            }
        }
        out.insert(c, e);
    }
    let _ = write_json(path, &Value::Object(out));
}

pub fn load_coins_config(path: &Path) -> HashMap<String, CoinConfig> {
    let mut out = HashMap::new();
    let Ok(Value::Object(m)) = read_config_content(path, &IndexMap::new(), true) else {
        return out;
    };
    for (c, v) in m {
        let mttf = util::convert_from_time(&jsonx::s(jsonx::get(&v, "MaxTimeToFind")));
        let mpp: String = jsonx::s(jsonx::get(&v, "MinProfitPercent")).chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        out.insert(
            c.to_uppercase(),
            CoinConfig {
                penalty: util::round_to(clean_num(jsonx::get(&v, "Penalty")), 2),
                min_hashrate: util::convert_from_hash(&jsonx::s(jsonx::get(&v, "MinHashrate"))),
                min_hashrate_solo: util::convert_from_hash(&jsonx::s(jsonx::get(&v, "MinHashrateSolo"))),
                min_workers: util::convert_from_hash(&jsonx::s(jsonx::get(&v, "MinWorkers"))),
                max_time_to_find: mttf,
                wallet: jsonx::s(jsonx::get(&v, "Wallet")).split_whitespace().collect(),
                enable_auto_pool: util::get_yes(jsonx::get(&v, "EnableAutoPool")),
                post_block_mining: util::convert_from_time(&jsonx::s(jsonx::get(&v, "PostBlockMining"))),
                min_profit_percent: mpp.parse::<f64>().unwrap_or(0.0),
                min_blk_rate: if mttf > 0 { 86400.0 / mttf as f64 } else { 0.0 },
            },
        );
    }
    out
}

/// Set-DevicesConfigDefault: add every detected model to devices.config.txt
pub fn ensure_devices_config(path: &Path, models: &[(String, String)]) {
    let preset: Value = if path.exists() {
        match read_config_content(path, &IndexMap::new(), true) {
            Ok(v) if v.is_object() => v,
            Ok(_) => json!({}),
            Err(_) => return,
        }
    } else {
        json!({})
    };
    let setup: Value = serde_json::from_str(DEVICES_CONFIG_DEFAULT).unwrap_or(json!({}));
    let default = json!({"Algorithm":"","ExcludeAlgorithm":"","MinerName":"","ExcludeMinerName":"","DisableDualMining":"","DefaultOCprofile":"","PowerAdjust":"100","Worker":"","EnableLHR":""});
    let mut out = preset.clone();
    for (model, typ) in models {
        let mut e = jsonx::get(&out, model).clone();
        if !e.is_object() {
            e = jsonx::get(&setup, typ).clone();
            if !e.is_object() {
                e = json!({});
            }
        }
        if let Value::Object(d) = &default {
            for (k, val) in d {
                if jsonx::get(&e, k).is_null() {
                    jsonx::set(&mut e, k, val.clone());
                }
            }
        }
        jsonx::set(&mut out, model, e);
    }
    if let Value::Object(m) = &mut out {
        m.sort_keys();
    }
    if out != preset {
        let _ = write_json(path, &out);
    }
}

pub fn load_devices_config(path: &Path, worker_name: &str, variants: bool) -> HashMap<String, DeviceConfig> {
    let mut out = HashMap::new();
    let Ok(Value::Object(m)) = read_config_content(path, &IndexMap::new(), true) else {
        return out;
    };
    for (model, v) in m {
        let mut algos = data::get_algorithms_list(&jsonx::str_list(jsonx::get(&v, "Algorithm")));
        let mut ex = data::get_algorithms_list(&jsonx::str_list(jsonx::get(&v, "ExcludeAlgorithm")));
        if variants {
            data::expand_algo_variants(&mut algos);
            data::expand_algo_variants(&mut ex);
        }
        let lhr = jsonx::s(jsonx::get(&v, "EnableLHR"));
        let mut worker = jsonx::s(jsonx::get(&v, "Worker")).trim().to_string();
        if model.eq_ignore_ascii_case("CPU") && worker.is_empty() {
            worker = format!("{}cpu", worker_name);
        }
        let pa: String = jsonx::s(jsonx::get(&v, "PowerAdjust")).chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        out.insert(
            model.to_lowercase(),
            DeviceConfig {
                algorithm: algos,
                exclude_algorithm: ex,
                miner_name: jsonx::str_list(jsonx::get(&v, "MinerName")),
                exclude_miner_name: jsonx::str_list(jsonx::get(&v, "ExcludeMinerName")),
                disable_dual_mining: util::get_yes(jsonx::get(&v, "DisableDualMining")),
                default_oc_profile: jsonx::s(jsonx::get(&v, "DefaultOCprofile")),
                power_adjust: pa.parse::<f64>().unwrap_or(0.0),
                worker,
                enable_lhr: if lhr.trim().is_empty() { None } else { Some(util::get_yes_str(&lhr)) },
            },
        );
    }
    out
}

/// miners.config.txt -> map "Name-Model-Algo[-Algo2]" -> settings
pub fn load_miners_config(path: &Path, device_models: &[String], full_combo_models: &HashMap<String, String>, cpu_threads: usize) -> HashMap<String, MinerConfig> {
    let mut out = HashMap::new();
    if !path.exists() {
        let _ = write_json(path, &json!({}));
        return out;
    }
    let raw = match read_config_content(path, &IndexMap::new(), true) {
        Ok(v) => v,
        Err(e) => {
            log_warn!("Your {} seems to be corrupt. Check for correct JSON format or delete it. ({})", path.display(), e);
            return out;
        }
    };
    let Value::Object(m) = raw else { return out };
    for (name, entries) in m {
        let mut parts: Vec<String> = name.split('-').map(|s| s.to_string()).collect();
        // "Miner-NVIDIA" style names address the full combo of a vendor
        if parts.len() > 1 {
            if let Some(full) = full_combo_models.get(&parts[1].to_uppercase()) {
                if !full.is_empty() {
                    let n = format!("{}-{}", parts[0], full);
                    parts = n.split('-').map(|s| s.to_string()).collect();
                }
            }
        }
        let base_name = parts.join("-");
        if parts.iter().skip(1).any(|p| !device_models.iter().any(|d| d.eq_ignore_ascii_case(p))) {
            continue;
        }
        for e in jsonx::as_list(&entries) {
            if !e.is_object() {
                continue;
            }
            let disable = util::get_yes(jsonx::get(e, "Disable"));
            let tuning = util::get_yes(jsonx::get(e, "Tuning"));
            let mut relevant = disable || tuning;
            if let Value::Object(em) = e {
                for (k, val) in em {
                    if ["MainAlgorithm", "SecondaryAlgorithm", "Disable", "Tuning"].iter().any(|x| x.eq_ignore_ascii_case(k)) {
                        continue;
                    }
                    if !(val.is_string() && jsonx::s(val).trim().is_empty()) && !val.is_null() {
                        relevant = true;
                    }
                }
            }
            if !relevant {
                continue;
            }
            let mut key = base_name.clone();
            let main_algo = jsonx::s(jsonx::get(e, "MainAlgorithm"));
            let mut nm = e.clone();
            if let Value::Object(mm) = &mut nm {
                mm.shift_remove("MainAlgorithm");
                mm.shift_remove("SecondaryAlgorithm");
            }
            if main_algo != "*" && !main_algo.is_empty() {
                key = format!("{}-{}", key, data::get_algorithm(&main_algo));
                let sec = jsonx::s(jsonx::get(e, "SecondaryAlgorithm"));
                if !sec.is_empty() {
                    key = format!("{}-{}", key, data::get_algorithm(&sec));
                }
            }
            let msia = jsonx::s(jsonx::get(&nm, "MSIAprofile"));
            if !msia.is_empty() && !["1", "2", "3", "4", "5"].contains(&msia.as_str()) {
                log_warn!("Invalid MSIAprofile for {} in miners.config.txt: \"{}\" (empty or 1-5 allowed, only)", key, msia);
                jsonx::set(&mut nm, "MSIAprofile", json!(""));
            }
            let diff = jsonx::s(jsonx::get(&nm, "Difficulty"));
            if !jsonx::get(&nm, "Difficulty").is_null() {
                jsonx::set(&mut nm, "Difficulty", json!(diff.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect::<String>()));
            }
            let aff = jsonx::s(jsonx::get(&nm, "Affinity"));
            if !aff.is_empty() {
                let cpus: Vec<usize> = util::convert_from_cpu_affinity(&aff).into_iter().filter(|c| cpu_threads == 0 || *c < cpu_threads).collect();
                if cpus.is_empty() {
                    jsonx::set(&mut nm, "Affinity", json!(""));
                } else {
                    jsonx::set(&mut nm, "Affinity", json!(util::convert_to_cpu_affinity_hex(&cpus)));
                    if jsonx::num(jsonx::get(&nm, "Threads")) == 0.0 {
                        jsonx::set(&mut nm, "Threads", json!(cpus.len()));
                    }
                }
            }
            if !jsonx::get(&nm, "Threads").is_null() {
                let t: String = jsonx::s(jsonx::get(&nm, "Threads")).chars().filter(|c| c.is_ascii_digit()).collect();
                jsonx::set(&mut nm, "Threads", json!(t.parse::<i64>().unwrap_or(0)));
            }
            if !jsonx::get(&nm, "PowerDraw").is_null() {
                let p: String = jsonx::s(jsonx::get(&nm, "PowerDraw")).chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
                jsonx::set(&mut nm, "PowerDraw", p.parse::<f64>().map(|x| json!(util::round_to(x, 2))).unwrap_or(Value::Null));
            }
            let sc = jsonx::s(jsonx::get(&nm, "ShareCheck"));
            if !sc.is_empty() {
                jsonx::set(&mut nm, "ShareCheck", json!(util::convert_from_time(&sc)));
            }
            jsonx::set(&mut nm, "Disable", json!(disable));
            jsonx::set(&mut nm, "Tuning", json!(tuning));
            out.insert(key.to_lowercase(), MinerConfig { raw: nm });
        }
    }
    out
}

pub fn ensure_userpools_config(path: &Path) {
    if path.exists() {
        return;
    }
    let d: Value = serde_json::from_str(USERPOOLS_CONFIG_DEFAULT).unwrap_or(json!({}));
    let _ = write_json(path, &Value::Array(vec![d.clone(), d.clone(), d.clone(), d.clone(), d]));
}

/// userpools.config.txt (placeholders like $Wallet stay literal and are resolved per pool)
pub fn load_userpools_config(path: &Path) -> Vec<Value> {
    let Ok(v) = read_config_content(path, &IndexMap::new(), true) else {
        return Vec::new();
    };
    let list: Vec<Value> = match v {
        Value::Array(a) => a,
        Value::Object(ref o) if o.contains_key("value") => jsonx::get(&v, "value").as_array().cloned().unwrap_or_default(),
        other => vec![other],
    };
    list.into_iter()
        .filter(|u| !jsonx::s(jsonx::get(u, "Name")).trim().is_empty())
        .collect()
}

/// customminers.config.txt (Get-CustomMinerDefinitions)
pub fn load_custom_miners(path: &Path, builtin: &[String]) -> Vec<CustomMinerDef> {
    let mut out = Vec::new();
    if !path.exists() {
        let _ = write_json(path, &json!({}));
        return out;
    }
    let Ok(Value::Object(m)) = read_config_content(path, &IndexMap::new(), true) else {
        return out;
    };
    let vendors_all = ["AMD", "CPU", "INTEL", "NVIDIA"];
    let dev_props = ["Type_Vendor_Index", "Type_PlatformId_Index", "Type_Index", "Type_Mineable_Index", "Index", "BusId", "PCIBusId", "Vendor_Index"];
    let re_name = Regex::new(r"^[A-Za-z0-9_]+$").unwrap();
    for (name, d) in m {
        let name = name.trim().to_string();
        if !re_name.is_match(&name) || builtin.iter().any(|b| b.eq_ignore_ascii_case(&name)) {
            log_warn!("Custom miner \"{}\" skipped: the name may contain letters, digits and underscore only and must not equal a built-in miner name", name);
            continue;
        }
        if !d.is_object() {
            continue;
        }
        let vendors: Vec<String> = jsonx::str_list(jsonx::get(&d, "Vendors"))
            .into_iter()
            .map(|x| x.trim().to_uppercase())
            .filter(|x| vendors_all.contains(&x.as_str()))
            .collect();
        let mut commands = Vec::new();
        for c in jsonx::as_list(jsonx::get(&d, "Commands")) {
            let ma = jsonx::s(jsonx::get(c, "MainAlgorithm")).trim().to_string();
            if ma.is_empty() {
                continue;
            }
            let algo = jsonx::s(jsonx::get(c, "Algo")).trim().to_string();
            let num = |k: &str| -> Option<f64> {
                let s: String = jsonx::s(jsonx::get(c, k)).trim().replace(',', ".").chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
                s.parse::<f64>().ok()
            };
            commands.push(CustomMinerCommand {
                main_algorithm: data::get_algorithm(&ma),
                algo: if algo.is_empty() { ma.clone() } else { algo },
                params: jsonx::s(jsonx::get(c, "Params")).trim().to_string(),
                fee: num("Fee"),
                extend_interval: jsonx::s(jsonx::get(c, "ExtendInterval")).trim().parse::<i64>().ok(),
                min_mem_gb: num("MinMemGB").unwrap_or(0.0),
                dag: util::get_yes(jsonx::get(c, "DAG")),
                vendors: jsonx::str_list(jsonx::get(c, "Vendors"))
                    .into_iter()
                    .map(|x| x.trim().to_uppercase())
                    .filter(|x| vendors.contains(x))
                    .collect(),
            });
        }
        let os = |k: &str| -> (String, String) {
            let o = jsonx::get(&d, k);
            (
                jsonx::s(jsonx::get(o, "Uri")).trim().to_string(),
                jsonx::s(jsonx::get(o, "Path")).trim().trim_start_matches(['/', '\\']).replace('\\', "/"),
            )
        };
        let mut dev_prop = jsonx::s(jsonx::get(&d, "DeviceIndexProperty")).trim().to_string();
        if !dev_props.iter().any(|p| p.eq_ignore_ascii_case(&dev_prop)) {
            dev_prop = "Type_Vendor_Index".into();
        }
        let mut sep = jsonx::s(jsonx::get(&d, "DeviceSeparator"));
        if sep.is_empty() {
            sep = ",".into();
        }
        let version = jsonx::s(jsonx::get(&d, "Version")).trim().to_string();
        let api = jsonx::s(jsonx::get(&d, "API")).trim().to_string();
        out.push(CustomMinerDef {
            name: name.clone(),
            enable: util::get_yes(jsonx::get(&d, "Enable")),
            manual_uri: jsonx::s(jsonx::get(&d, "ManualUri")).trim().to_string(),
            version: if version.is_empty() { "1.0".into() } else { version },
            vendors,
            windows: os("Windows"),
            linux: os("Linux"),
            linux_arm: os("LinuxArm"),
            api: if api.is_empty() { "Wrapper".into() } else { api },
            hashrate_regex: jsonx::s(jsonx::get(&d, "HashRateRegex")).trim().to_string(),
            port: jsonx::s(jsonx::get(&d, "Port")).trim().parse::<u16>().unwrap_or(4000),
            dev_fee: jsonx::num(jsonx::get(&d, "DevFee")),
            arguments: jsonx::s(jsonx::get(&d, "Arguments")).trim().to_string(),
            ssl_arguments: jsonx::s(jsonx::get(&d, "SSLArguments")).trim().to_string(),
            enable_ssl: if jsonx::get(&d, "EnableSSL").is_null() { true } else { util::get_yes(jsonx::get(&d, "EnableSSL")) },
            device_index_property: dev_prop,
            device_index_hex: util::get_yes(jsonx::get(&d, "DeviceIndexHex")),
            device_separator: sep,
            env_vars: jsonx::str_list(jsonx::get(&d, "EnvVars")).into_iter().filter(|x| x.contains('=')).collect(),
            exclude_pool_name: jsonx::s(jsonx::get(&d, "ExcludePoolName")).trim().to_string(),
            show_miner_window: util::get_yes(jsonx::get(&d, "ShowMinerWindow")),
            extend_interval: jsonx::s(jsonx::get(&d, "ExtendInterval")).trim().parse::<i64>().unwrap_or(1),
            commands,
        });
    }
    out
}

/// combos.config.txt: {"NVIDIA": {"GTX1070-GTX1080": "1"}, ...}
pub fn load_combos_config(path: &Path) -> HashMap<String, HashMap<String, bool>> {
    let mut out = HashMap::new();
    if !path.exists() {
        let _ = write_json(path, &json!({}));
        return out;
    }
    let Ok(Value::Object(m)) = read_config_content(path, &IndexMap::new(), true) else {
        return out;
    };
    for (t, v) in m {
        let mut inner = HashMap::new();
        if let Value::Object(im) = v {
            for (k, b) in im {
                inner.insert(k.to_lowercase(), util::get_yes(&b));
            }
        }
        out.insert(t.to_uppercase(), inner);
    }
    out
}

/// write the combos config with all subsets (new subsets are disabled by default, full combos enabled)
pub fn ensure_combos_config(path: &Path, subsets: &HashMap<String, Vec<String>>) {
    let mut cur: Value = read_config_content(path, &IndexMap::new(), true).unwrap_or(json!({}));
    if !cur.is_object() {
        cur = json!({});
    }
    let before = cur.clone();
    for (typ, combos) in subsets {
        let mut sect = jsonx::get(&cur, typ).clone();
        if !sect.is_object() {
            sect = json!({});
        }
        for c in combos {
            if jsonx::get(&sect, c).is_null() {
                jsonx::set(&mut sect, c, json!("0"));
            }
        }
        jsonx::set(&mut cur, typ, sect);
    }
    if cur != before {
        let _ = write_json(path, &cur);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitution() {
        let mut p = IndexMap::new();
        p.insert("Wallet".to_string(), "1abc".to_string());
        p.insert("WalletBalances".to_string(), "".to_string());
        p.insert("MSIApath".to_string(), "C:\\Program Files\\x.exe".to_string());
        let r = substitute_params(r#"{"a":"$Wallet","b":"$WalletBalances","c":"$Unknown","d":"$MSIApath"}"#, &p, false);
        let v: Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["a"], "1abc");
        assert_eq!(v["b"], "");
        assert_eq!(v["c"], "");
        assert_eq!(v["d"], "C:\\Program Files\\x.exe");
    }

    #[test]
    fn stat_average() {
        assert_eq!(get_stat_average("min10", ""), "Minute_10");
        assert_eq!(get_stat_average("", "Week"), "Week");
        assert_eq!(get_stat_average("hour", ""), "Hour");
        assert_eq!(get_yiimp_data_window("avg", ""), "average-2");
        assert_eq!(get_yiimp_data_window("", ""), "estimate_current");
    }

    #[test]
    fn payout_currencies() {
        let v = json!({"BTC":"1abcdef","XMR":"","Worker":"rig","Penalty":"0","RVN-Params":"x","#comment":"long text"});
        let f = pool_fields();
        let w = pool_payout_currencies(&v, &f);
        assert_eq!(w.len(), 1);
        assert_eq!(w["BTC"], "1abcdef");
    }
}

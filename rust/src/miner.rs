//! Miner candidates: the objects the miner modules (Miners/*.ps1) emit, the context they get,
//! and the shared helpers of Modules/MinersLib.psm1 (Get-PoolAlgorithmKeys, Test-VRAM,
//! Confirm-Cuda, Get-MinersContent).

use crate::config::Config;
use crate::data;
use crate::device::{self, CpuInfo, Device, DeviceCache};
use crate::jsonx;
use crate::pool::Pool;
use crate::stat;
use crate::util;
use crate::{log_info, log_warn};
use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// ----------------------------------------------------------------------------------------
// $Pools as seen by the miner modules
// ----------------------------------------------------------------------------------------

/// The selected pools: keys are case insensitive ("ethash", "Ethash-GPU", "Ethash-@Zpool")
#[derive(Debug, Clone, Default)]
pub struct PoolsMap {
    map: IndexMap<String, (String, Pool)>,
}

#[allow(dead_code)]
impl PoolsMap {
    pub fn insert(&mut self, key: &str, p: Pool) {
        self.map.insert(key.to_lowercase(), (key.to_string(), p));
    }
    pub fn get(&self, key: &str) -> Option<&Pool> {
        self.map.get(&key.to_lowercase()).map(|x| &x.1)
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Pool> {
        self.map.get_mut(&key.to_lowercase()).map(|x| &mut x.1)
    }
    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(&key.to_lowercase())
    }
    /// $Pools.$key.Host is set
    pub fn has_host(&self, key: &str) -> bool {
        self.get(key).map(|p| !p.Host.is_empty()).unwrap_or(false)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Pool)> {
        self.map.values().map(|(k, p)| (k, p))
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&String, &mut Pool)> {
        self.map.values_mut().map(|(k, p)| (&*k, p))
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// a pool-alternate key ("Ethash-@Zpool")
pub fn is_alternate_key(key: &str) -> bool {
    key.contains("-@")
}

/// "Ethash-@Zpool" -> "Ethash", "Ethash-GPU" -> "Ethash"
pub fn algorithm_base(key: &str) -> String {
    key.split('-').next().unwrap_or("").to_string()
}

/// strip the pool-alternate suffix only ("Ethash-GPU-@Zpool" -> "Ethash-GPU")
pub fn strip_alternate(key: &str) -> String {
    match key.find("-@") {
        Some(i) => key[..i].to_string(),
        None => key.to_string(),
    }
}

// ----------------------------------------------------------------------------------------
// the candidate object
// ----------------------------------------------------------------------------------------

/// The Arguments of a miner: a command line, or an object for the miners that write config
/// files (Xmrig6, SrbMinerMulti); objects carry their extra parameters in "Params"/"Arguments".
#[derive(Debug, Clone)]
pub enum MinerArgs {
    Str(String),
    Obj(Value),
}

#[derive(Debug, Clone)]
pub struct MinerCandidate {
    pub name: String,
    pub base_name: String,
    pub device_name: Vec<String>,
    pub device_model: String,
    pub path: PathBuf,
    pub arguments: MinerArgs,
    /// the final argument string (set by the core)
    pub arguments_str: String,
    /// algorithm key -> hashrate (None = needs benchmark)
    pub hashrates: IndexMap<String, Option<f64>>,
    pub api: String,
    pub port: u16,
    pub uri: String,
    pub fault_tolerance: Option<f64>,
    pub extend_interval: i64,
    pub penalty: f64,
    pub dev_fee: IndexMap<String, f64>,
    pub manual_uri: String,
    pub version: String,
    pub power_draw: f64,
    pub base_algorithm: String,
    pub exclude_pool_name: String,
    pub no_cpu_mining: bool,
    pub env_vars: Vec<String>,
    pub mining_priority: Option<i32>,
    pub mining_affinity: Option<String>,
    pub max_rejected_share_ratio: Option<f64>,
    pub prerequisite_path: Option<PathBuf>,
    pub prerequisite_uri: String,
    pub prerequisite_msg: String,
    pub log_file_args: String,
    pub set_ld_library_path: Option<bool>,
    pub show_miner_window: bool,
    pub hashrate_regex: String,
    pub skip_seconds: Option<i64>,
    pub min_samples: Option<usize>,
    pub list_devices: String,
    // ---- set by the core (Invoke-Core "Calculating profit for each miner")
    pub pools: IndexMap<String, Pool>,
    pub difficulties: HashMap<String, f64>,
    pub ratios: HashMap<String, f64>,
    pub oc_profile: IndexMap<String, String>,
    pub profit: Option<f64>,
    pub profit_bias: f64,
    pub profit_unbias: f64,
    pub profit_cost: Option<f64>,
    pub profit_cost_bias: f64,
    pub disabled: bool,
    pub benchmark_only: bool,
    pub msia_profile: i64,
    pub share_check: i64,
}

impl MinerCandidate {
    pub fn new(base_name: &str, name: &str, devices: &[&Device], model: &str, path: PathBuf, api: &str, port: u16) -> MinerCandidate {
        let mut names: Vec<String> = devices.iter().map(|d| d.Name.clone()).collect();
        names.sort();
        names.dedup();
        MinerCandidate {
            name: name.to_string(),
            base_name: base_name.to_string(),
            device_name: names,
            device_model: model.to_string(),
            path,
            arguments: MinerArgs::Str(String::new()),
            arguments_str: String::new(),
            hashrates: IndexMap::new(),
            api: api.to_string(),
            port,
            uri: String::new(),
            fault_tolerance: None,
            extend_interval: 0,
            penalty: 0.0,
            dev_fee: IndexMap::new(),
            manual_uri: String::new(),
            version: String::new(),
            power_draw: 0.0,
            base_algorithm: String::new(),
            exclude_pool_name: String::new(),
            no_cpu_mining: false,
            env_vars: Vec::new(),
            mining_priority: None,
            mining_affinity: None,
            max_rejected_share_ratio: None,
            prerequisite_path: None,
            prerequisite_uri: String::new(),
            prerequisite_msg: String::new(),
            log_file_args: String::new(),
            set_ld_library_path: None,
            show_miner_window: false,
            hashrate_regex: String::new(),
            skip_seconds: None,
            min_samples: None,
            list_devices: String::new(),
            pools: IndexMap::new(),
            difficulties: HashMap::new(),
            ratios: HashMap::new(),
            oc_profile: IndexMap::new(),
            profit: None,
            profit_bias: 0.0,
            profit_unbias: 0.0,
            profit_cost: None,
            profit_cost_bias: 0.0,
            disabled: false,
            benchmark_only: false,
            msia_profile: 0,
            share_check: 0,
        }
    }

    /// add one algorithm: hashrate from the stats cache (Week, or the given duration)
    pub fn with_algo(mut self, key: &str, hashrate: Option<f64>, devfee: f64) -> Self {
        self.hashrates.insert(key.to_string(), hashrate);
        self.dev_fee.insert(key.to_string(), devfee);
        self
    }

    pub fn needs_benchmark(&self) -> bool {
        self.hashrates.values().any(|h| h.is_none())
    }

    pub fn algo_names(&self) -> Vec<String> {
        self.hashrates.keys().cloned().collect()
    }

    pub fn base_algorithms(&self) -> Vec<String> {
        self.base_algorithm.split('-').map(|s| s.to_string()).collect()
    }

    /// the candidate as JSON for the API (/miners, /fastestminers)
    pub fn to_api_json(&self) -> Value {
        let hr: serde_json::Map<String, Value> = self
            .hashrates
            .iter()
            .map(|(k, v)| (k.clone(), v.map(|x| serde_json::json!(x)).unwrap_or(Value::Null)))
            .collect();
        let fee: serde_json::Map<String, Value> = self.dev_fee.iter().map(|(k, v)| (k.clone(), serde_json::json!(v))).collect();
        let pools: serde_json::Map<String, Value> = self
            .pools
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or(Value::Null)))
            .collect();
        let diffs: serde_json::Map<String, Value> = self.difficulties.iter().map(|(k, v)| (k.clone(), serde_json::json!(v))).collect();
        let oc: serde_json::Map<String, Value> = self.oc_profile.iter().map(|(k, v)| (k.clone(), serde_json::json!(v))).collect();
        serde_json::json!({
            "Name": self.name,
            "BaseName": self.base_name,
            "DeviceName": self.device_name,
            "DeviceModel": self.device_model,
            "Path": self.path.to_string_lossy(),
            "Arguments": self.arguments_str,
            "HashRates": hr,
            "API": self.api,
            "Port": self.port,
            "Uri": self.uri,
            "FaultTolerance": self.fault_tolerance,
            "ExtendInterval": self.extend_interval,
            "Penalty": self.penalty,
            "DevFee": fee,
            "ManualUri": self.manual_uri,
            "Version": self.version,
            "PowerDraw": self.power_draw,
            "BaseAlgorithm": self.base_algorithm,
            "Pools": pools,
            "Difficulties": diffs,
            "OCprofile": oc,
            "Profit": self.profit,
            "Profit_Bias": self.profit_bias,
            "Profit_Unbias": self.profit_unbias,
            "Profit_Cost": self.profit_cost,
            "Profit_Cost_Bias": self.profit_cost_bias,
            "Disabled": self.disabled,
            "BenchmarkOnly": self.benchmark_only,
            "NoCPUMining": self.no_cpu_mining,
            "ExcludePoolName": self.exclude_pool_name,
        })
    }
}

/// what a miner module reports with -InfoOnly (miners.config.txt, Setup, API /minerinfo)
#[derive(Debug, Clone)]
pub struct MinerInfo {
    pub name: String,
    pub types: Vec<String>,
    pub path: PathBuf,
    pub uri: String,
    pub dev_fee: f64,
    pub manual_uri: String,
    pub commands: Value,
}

impl MinerInfo {
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "Name": self.name,
            "BaseName": self.name,
            "Type": self.types,
            "Path": self.path.to_string_lossy(),
            "Uri": self.uri,
            "DevFee": self.dev_fee,
            "ManualUri": self.manual_uri,
            "Commands": self.commands,
        })
    }
}

// ----------------------------------------------------------------------------------------
// the context of a miner module
// ----------------------------------------------------------------------------------------

pub struct MinerCtx<'a> {
    pub base: &'a Path,
    pub config: &'a Config,
    pub devices: &'a DeviceCache,
    pub cpu: &'a CpuInfo,
    pub pools: &'a PoolsMap,
    /// lower case algorithm -> alternate keys ($Global:PoolAlternates)
    pub alternates: &'a HashMap<String, Vec<String>>,
    pub yiimp_pools: &'a HashSet<String>,
    pub distro_codename: String,
    pub libc_version: Option<util::Version>,
    /// $Session.IsVM
    pub is_vm: bool,
}

#[derive(Default, Clone, Copy)]
pub struct KeyOpts<'b> {
    pub no_gpu: bool,
    pub exclude_pool_name: &'b str,
    pub pool_name: &'b str,
    pub coin_symbols: &'b [String],
    pub exclude_yiimp: bool,
}

thread_local! {
    static RE_CACHE: RefCell<HashMap<String, Option<Regex>>> = RefCell::new(HashMap::new());
}

/// PowerShell -match (case insensitive, an invalid pattern never matches)
pub fn ps_match(s: &str, pat: &str) -> bool {
    if pat.is_empty() {
        return true;
    }
    RE_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let re = c
            .entry(pat.to_string())
            .or_insert_with(|| Regex::new(&format!("(?i){}", pat)).ok());
        re.as_ref().map(|r| r.is_match(s)).unwrap_or(false)
    })
}

/// "316{0:d2}" -f Index
pub fn port_fmt(prefix: &str, index: usize) -> u16 {
    format!("{}{:02}", prefix, index).parse::<u32>().unwrap_or(0).min(65535) as u16
}

/// Confirm-Cuda
pub fn confirm_cuda(actual: &str, required: &str, warning: &str) -> bool {
    if required.is_empty() {
        return true;
    }
    let v1: Vec<i64> = actual.split('.').map(|x| x.trim().parse::<i64>().unwrap_or(0)).collect();
    let v2: Vec<i64> = required.split('.').map(|x| x.trim().parse::<i64>().unwrap_or(0)).collect();
    let max = v1.len().min(v2.len());
    for i in 0..max {
        if v1[i] < v2[i] {
            if !warning.is_empty() {
                log_info!(
                    "{} requires CUDA version {} or above (installed version is {}). Please update your Nvidia drivers.",
                    warning,
                    required,
                    actual
                );
            }
            return false;
        }
        if v1[i] > v2[i] {
            return true;
        }
    }
    true
}

impl<'a> MinerCtx<'a> {
    pub fn is_linux(&self) -> bool {
        cfg!(target_os = "linux")
    }
    pub fn is_windows(&self) -> bool {
        cfg!(windows)
    }
    pub fn is_arm(&self) -> bool {
        self.cpu.is_arm()
    }

    /// a miner path inside the RainbowMiner folder (".\Bin\NVIDIA-Trex\t-rex" -> absolute)
    pub fn path(&self, rel: &str) -> PathBuf {
        let mut p = util::rbm_path(self.base, rel);
        if cfg!(windows) && p.extension().is_none() {
            p.set_extension("exe");
        }
        p
    }

    /// $Global:DeviceCache.DevicesByTypes.$vendor
    pub fn by_type(&self, vendor: &str) -> &[Device] {
        self.devices.by_types.get(vendor).map(|v| v.as_slice()).unwrap_or(&[])
    }
    pub fn has_type(&self, vendor: &str) -> bool {
        !self.by_type(vendor).is_empty()
    }
    /// Select-Object Vendor, Model -Unique
    pub fn models(&self, vendor: &str) -> Vec<String> {
        self.devices.models_of(vendor)
    }
    pub fn devices_of(&self, vendor: &str, model: &str) -> Vec<&Device> {
        self.devices.devices_of(vendor, model)
    }

    pub fn pool(&self, key: &str) -> Option<&Pool> {
        self.pools.get(key)
    }

    /// Test-VRAM
    pub fn test_vram(&self, d: &Device, min_mem_gb: f64) -> bool {
        device::test_vram(d, min_mem_gb, &self.config.s("GPUReservedVRAMGB"))
    }

    /// the stats cache value of a miner ($Global:StatsCache."<name>_<algo>_HashRate".Week)
    pub fn hashrate(&self, miner_name: &str, algo0: &str) -> Option<f64> {
        stat::cached(&format!("{}_{}_HashRate", miner_name, algo0)).map(|s| s.week)
    }
    pub fn hashrate_dur(&self, miner_name: &str, algo0: &str, duration: &str) -> Option<f64> {
        let d = if duration.is_empty() { "Week" } else { duration };
        stat::cached(&format!("{}_{}_HashRate", miner_name, algo0)).map(|s| s.get_average(d))
    }
    pub fn ratio_live(&self, miner_name: &str, algo0: &str) -> f64 {
        stat::cached(&format!("{}_{}_HashRate", miner_name, algo0)).map(|s| s.ratio_live).unwrap_or(0.0)
    }

    /// Test-PoolConstraint
    pub fn pool_constraint(&self, pool: Option<&Pool>, o: &KeyOpts, miner_name: &str) -> bool {
        let Some(pool) = pool else { return false };
        if pool.Host.is_empty() {
            return false;
        }
        if !o.exclude_pool_name.is_empty() && (ps_match(&pool.Host, o.exclude_pool_name) || ps_match(&pool.Name, o.exclude_pool_name)) {
            return false;
        }
        if !o.pool_name.is_empty() && !(ps_match(&pool.Host, o.pool_name) || ps_match(&pool.Name, o.pool_name)) {
            return false;
        }
        if !o.coin_symbols.is_empty() && !util::contains_ci(o.coin_symbols, &pool.CoinSymbol) {
            return false;
        }
        if o.exclude_yiimp && self.yiimp_pools.contains(&pool.Name) {
            return false;
        }
        if !miner_name.is_empty() && pool.HasMinerExclusions {
            if let Some(rules) = self.config.pool(&pool.Name) {
                if (!rules.miner_name.is_empty() && !util::contains_ci(&rules.miner_name, miner_name))
                    || util::contains_ci(&rules.exclude_miner_name, miner_name)
                {
                    return false;
                }
            }
            if (!pool.MinerName.is_empty() && !util::contains_ci(&pool.MinerName, miner_name)) || util::contains_ci(&pool.ExcludeMinerName, miner_name) {
                return false;
            }
        }
        true
    }

    /// Get-PoolAlgorithmKeys
    pub fn algo_keys(&self, miner_name: &str, algo: &str, model: &str, o: KeyOpts) -> Vec<String> {
        let mut keys = vec![algo.to_string()];
        if !model.is_empty() {
            keys.push(format!("{}-{}", algo, model));
        }
        if !o.no_gpu {
            keys.push(format!("{}-GPU", algo));
        }
        let constrained = !o.exclude_pool_name.is_empty() || !o.pool_name.is_empty() || !o.coin_symbols.is_empty() || o.exclude_yiimp;
        if !miner_name.is_empty() && !constrained {
            let has_rules = keys.iter().any(|k| self.pools.get(k).map(|p| p.HasMinerExclusions).unwrap_or(false));
            if !has_rules {
                return keys;
            }
        }
        if !constrained && miner_name.is_empty() {
            return keys;
        }
        let max_alt = if self.config.b("EnablePoolAlternates") && !self.alternates.is_empty() {
            self.config.i("MaxPoolAlternates").max(0) as usize
        } else {
            0
        };
        let mut out = Vec::new();
        for k in keys {
            let pool = self.pools.get(&k);
            if pool.map(|p| p.Host.is_empty()).unwrap_or(true) {
                out.push(k);
                continue;
            }
            if self.pool_constraint(pool, &o, miner_name) {
                out.push(k);
                continue;
            }
            if max_alt == 0 {
                continue;
            }
            let Some(alts) = self.alternates.get(&k.to_lowercase()) else { continue };
            let mut found = 0;
            for a in alts {
                if found >= max_alt {
                    break;
                }
                if self.pool_constraint(self.pools.get(a), &o, miner_name) {
                    out.push(a.clone());
                    found += 1;
                }
            }
        }
        out
    }

    /// keys of a secondary algorithm: algo, algo-model, algo-GPU (CPU: no -GPU)
    pub fn second_keys(&self, algo2: &str, model: &str, gpu: bool) -> Vec<String> {
        let mut v = vec![algo2.to_string(), format!("{}-{}", algo2, model)];
        if gpu {
            v.push(format!("{}-GPU", algo2));
        }
        v
    }

    /// $MinMemGB of a command (DAG algorithms: the pool's DagSizeMax or Get-EthDAGSize)
    pub fn min_mem_gb(&self, cmd: &Value, pool: &Pool, algo0: &str) -> f64 {
        let min = jsonx::num(jsonx::get(cmd, "MinMemGB"));
        if jsonx::truthy(jsonx::get(cmd, "DAG")) {
            match pool.DagSizeMax {
                Some(d) if d > 0.0 => d,
                _ => data::get_eth_dag_size(&pool.CoinSymbol, algo0, if min > 0.0 { min } else { 1.0 }),
            }
        } else {
            min
        }
    }

    /// $Session.Config.Miners."<Name>-CPU-<Algo>".Threads etc.
    pub fn cpu_threads(&self, name: &str, algo: &str) -> Option<i64> {
        for k in [format!("{}-CPU-{}", name, algo), format!("{}-CPU", name)] {
            if let Some(m) = self.config.miner(&k) {
                let t = jsonx::num(jsonx::get(&m.raw, "Threads")) as i64;
                if t > 0 {
                    return Some(t);
                }
            }
        }
        let t = self.config.i("CPUMiningThreads");
        if t > 0 {
            Some(t)
        } else {
            None
        }
    }
    pub fn cpu_affinity(&self, name: &str, algo: &str) -> Option<String> {
        for k in [format!("{}-CPU-{}", name, algo), format!("{}-CPU", name)] {
            if let Some(m) = self.config.miner(&k) {
                let a = m.s("Affinity");
                if !a.is_empty() {
                    return Some(a);
                }
            }
        }
        let a = self.config.s("CPUMiningAffinity");
        if a.is_empty() {
            None
        } else {
            Some(a)
        }
    }

    /// $Session.Config.Miners."<Name>-<Model>-<Algo1>-<Algo2>".Intensity (array)
    pub fn intensities(&self, name: &str, model: &str, a1: &str, a2: &str) -> Vec<String> {
        let k = format!("{}-{}-{}-{}", name, model, a1, a2);
        let Some(m) = self.config.miner(&k) else { return vec![] };
        let v = jsonx::get(&m.raw, "Intensity");
        let mut out: Vec<String> = match v {
            Value::Array(a) => a.iter().map(jsonx::s).collect(),
            Value::Null => vec![],
            _ => util::split_config_array(&jsonx::s(v)),
        };
        out.retain(|s| !s.trim().is_empty());
        out
    }

    /// Confirm-Cuda against the detected CUDA version
    pub fn cuda_ok(&self, required: &str, warn_name: &str) -> bool {
        confirm_cuda(&self.config.cuda_version, required, warn_name)
    }

    /// the first CUDA build that fits (UriCuda lists, newest first); None if none fits
    pub fn pick_cuda<'b>(&self, list: &'b [(&'b str, &'b str)], warn_name: &str) -> Option<&'b str> {
        for (i, (uri, cuda)) in list.iter().enumerate() {
            let warn = if i + 1 < list.len() { "" } else { warn_name };
            if self.cuda_ok(cuda, warn) {
                return Some(uri);
            }
        }
        None
    }

    /// $Session.LibCVersion -and $Session.LibCVersion -lt (Get-Version <v>)
    pub fn libc_below(&self, v: &str) -> bool {
        match (&self.libc_version, util::get_version(v)) {
            (Some(l), Some(v)) => *l < v,
            _ => false,
        }
    }

    /// $Session.LinuxDistroInfo.distroCodename within a list, else the default
    pub fn distro(&self, allowed: &[&str], default: &str) -> String {
        if allowed.iter().any(|a| *a == self.distro_codename) {
            self.distro_codename.clone()
        } else {
            default.to_string()
        }
    }
}

/// $Miner_Name: <Name>[-<A1>-<A2>]-<devices sorted>
pub fn miner_name(base: &str, dual: Option<&str>, devices: &[&Device]) -> String {
    let mut names: Vec<&str> = devices.iter().map(|d| d.Name.as_str()).collect();
    names.sort();
    let mut parts = vec![base.to_string()];
    if let Some(d) = dual {
        parts.push(d.to_string());
    }
    parts.extend(names.iter().map(|s| s.to_string()));
    parts.join("-")
}

/// <prop> of the devices joined
pub fn device_ids(devices: &[&Device], prop: &str, sep: &str, hex: bool) -> String {
    devices
        .iter()
        .map(|d| {
            let v = d.prop(prop);
            if hex {
                v.parse::<u64>().map(|n| format!("{:x}", n)).unwrap_or(v)
            } else {
                v
            }
        })
        .collect::<Vec<_>>()
        .join(sep)
}

/// the command list of a miner module (assets/miners/<Name>.json)
pub fn commands(json: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Array(a)) => a,
        _ => vec![],
    }
}

/// cmd.Vendor contains the vendor (or no vendor list at all)
pub fn cmd_vendor(cmd: &Value, vendor: &str) -> bool {
    let v = jsonx::get(cmd, "Vendor");
    if v.is_null() {
        return true;
    }
    jsonx::str_list(v).iter().any(|x| x.eq_ignore_ascii_case(vendor))
}

pub fn cmd_s(cmd: &Value, k: &str) -> String {
    jsonx::s(jsonx::get(cmd, k))
}
pub fn cmd_f(cmd: &Value, k: &str) -> Option<f64> {
    jsonx::num_opt(jsonx::get(cmd, k))
}
pub fn cmd_b(cmd: &Value, k: &str) -> bool {
    jsonx::truthy(jsonx::get(cmd, k))
}
pub fn cmd_list(cmd: &Value, k: &str) -> Vec<String> {
    jsonx::str_list(jsonx::get(cmd, k))
}
pub fn cmd_i(cmd: &Value, k: &str) -> i64 {
    jsonx::num(jsonx::get(cmd, k)) as i64
}

/// the hashrate of a dual miner entry multiplied by the command's penalty factor
pub fn penalized(h: Option<f64>, cmd: &Value) -> Option<f64> {
    let p = cmd_f(cmd, "Penalty").unwrap_or(0.0);
    h.map(|x| if p != 0.0 { x * (1.0 - p / 100.0) } else { x })
}

/// Compare-Version a b >= 0 on device capabilities ("7.5" vs "5.0")
pub fn capability_ok(d: &Device, min: &str) -> bool {
    match &d.OpenCL.DeviceCapability {
        None => true,
        Some(c) if c.is_empty() => true,
        Some(c) => util::compare_version(c, min, 0) >= 0,
    }
}

/// a module of Miners/*.ps1
pub trait MinerModule: Send + Sync {
    fn name(&self) -> &'static str;
    /// the device types the miner can handle ($Global:MinerInfo, "ARMCPU" for ARM builds)
    fn types(&self) -> Vec<String>;
    /// InfoOnly; None if the module returns nothing on this system
    fn info(&self, ctx: &MinerCtx) -> Option<MinerInfo>;
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate>;
}

/// Get-MinersContent: run all modules (and the custom miners), post-process the candidates
pub fn get_miners_content(ctx: &MinerCtx, timers: Option<&mut HashMap<String, f64>>) -> Vec<MinerCandidate> {
    let mut possible: Vec<String> = ctx.devices.devices_to_vendors.values().cloned().collect();
    possible.sort();
    possible.dedup();
    if ctx.is_arm() {
        possible = possible.into_iter().map(|p| format!("ARM{}", p)).collect();
    }
    let miner_name = ctx.config.arr("MinerName");
    let exclude_miner_name = ctx.config.arr("ExcludeMinerName");
    let allowed = |name: &str, types: &[String]| -> bool {
        util::intersects_ci(&possible, types)
            && (miner_name.is_empty() || util::contains_ci(&miner_name, name))
            && (exclude_miner_name.is_empty() || !util::contains_ci(&exclude_miner_name, name))
    };
    let mut timers = timers;
    let mut out = Vec::new();
    let post = |mut m: MinerCandidate| -> Option<MinerCandidate> {
        if m.power_draw == 0.0 {
            let base = m.base_algorithm.split('-').next().unwrap_or("").to_string();
            m.power_draw = stat::cached(&format!("{}_{}_HashRate", m.name, base)).map(|s| s.power_draw_average).unwrap_or(0.0);
        }
        if let Some(full) = ctx.devices.full_combo_models.get(&m.device_model) {
            if !full.is_empty() {
                m.device_model = full.clone();
            }
        }
        Some(m)
    };
    for module in crate::miners::registry() {
        let name = module.name();
        if !allowed(name, &module.types()) {
            continue;
        }
        let t0 = std::time::Instant::now();
        for m in module.get_miners(ctx) {
            if let Some(m) = post(m) {
                out.push(m);
            }
        }
        if let Some(t) = timers.as_deref_mut() {
            t.insert(name.to_string(), (t0.elapsed().as_secs_f64() * 1000.0).round() / 1000.0);
        }
    }
    for def in &ctx.config.custom_miners {
        let types = crate::miners::custom::types(def);
        if !def.enable || !allowed(&def.name, &types) {
            continue;
        }
        for m in crate::miners::custom::get_miners(ctx, def) {
            if let Some(m) = post(m) {
                out.push(m);
            }
        }
    }
    out
}

/// InfoOnly of all modules
pub fn get_miners_info(ctx: &MinerCtx) -> Vec<MinerInfo> {
    let mut v: Vec<MinerInfo> = crate::miners::registry().iter().filter_map(|m| m.info(ctx)).collect();
    for def in &ctx.config.custom_miners {
        v.push(crate::miners::custom::info(ctx, def));
    }
    v
}

/// warn once per process
pub fn warn_once(key: &str, msg: &str) {
    static WARNED: std::sync::LazyLock<std::sync::Mutex<HashSet<String>>> = std::sync::LazyLock::new(|| std::sync::Mutex::new(HashSet::new()));
    if WARNED.lock().unwrap().insert(key.to_string()) {
        log_warn!("{}", msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuda_versions() {
        assert!(confirm_cuda("12.2", "11.8", ""));
        assert!(!confirm_cuda("11.2", "11.8", ""));
        assert!(confirm_cuda("11.8", "11.8", ""));
        assert!(confirm_cuda("", "", ""));
    }

    #[test]
    fn ports_and_keys() {
        assert_eq!(port_fmt("316", 3), 31603);
        assert_eq!(algorithm_base("Ethash-@Zpool"), "Ethash");
        assert_eq!(strip_alternate("Ethash-GPU-@Zpool"), "Ethash-GPU");
        assert!(is_alternate_key("KawPOW-@NiceHash"));
        assert!(ps_match("eu.nicehash.com", "nicehash|zpool"));
        assert!(!ps_match("eu.nicehash.com", "zpool"));
    }
}

//! Invoke-MinerFamily (Modules/MinersLib.psm1): the shared body of the Ccminer/Cpuminer/Sgminer
//! style modules. A module only fills a [`Family`] (header variables and the -Setup hashtable of
//! the PowerShell stub) and a `make_args` function, the driver builds the miner candidates.

use crate::device::Device;
use crate::jsonx;
use crate::miner::*;
use crate::pool::Pool;
use crate::util;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

/// $SuffixMode: which pool keys a command looks at
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Suffix {
    /// Get-PoolAlgorithmKeys (pool constraints, alternates)
    Keys,
    /// algo, algo-model, algo-GPU
    ListGPU,
    /// algo, algo-model
    ListCPU,
    /// Get-PoolAlgorithmKeys -NoGPU
    KeysNoGPU,
    /// Get-PoolAlgorithmKeys -CoinSymbols
    KeysCoin,
}

/// the values a MakeArgs script block sees
#[allow(dead_code)]
pub struct A<'x, 'c> {
    pub ctx: &'x MinerCtx<'c>,
    pub fam: &'x Family,
    /// the command ($_)
    pub cmd: &'x Value,
    /// $Pools.$Algorithm_Norm
    pub pool: &'x Pool,
    /// $Algorithm_Norm (pool key)
    pub key: &'x str,
    /// $Algorithm_Norm_0
    pub algo0: &'x str,
    /// $Pool_Port (GPU port of the pool)
    pub pool_port: u32,
    /// $DeviceIDsAll
    pub ids: &'x str,
    /// $Miner_Device
    pub devices: &'x [&'x Device],
    /// $Miner_Model
    pub model: &'x str,
    /// $DeviceParams (CpuParams)
    pub device_params: &'x str,
    pub threads: Option<i64>,
    pub affinity: Option<&'x str>,
    /// $PlatformId (NeedsPlatformId)
    pub platform_id: usize,
}

#[allow(dead_code)]
impl A<'_, '_> {
    /// a string field of the command
    pub fn c(&self, k: &str) -> String {
        cmd_s(self.cmd, k)
    }
    /// $_.Params
    pub fn params(&self) -> String {
        self.c("Params")
    }
    /// $_."Params$(if ($Miner_Model -match "(gfx900|vega)") {"Vega"})"
    pub fn params_vega(&self) -> String {
        self.c(if ps_match(self.model, "(gfx900|vega)") { "ParamsVega" } else { "Params" })
    }
    /// $_.MainAlgorithm
    pub fn main(&self) -> String {
        self.c("MainAlgorithm")
    }
    /// if ($_.Algorithm) {$_.Algorithm} else {$_.MainAlgorithm}
    pub fn algorithm(&self) -> String {
        let a = self.c("Algorithm");
        if a.is_empty() {
            self.main()
        } else {
            a
        }
    }
    /// $(if ($Pools.$Algorithm_Norm.Pass) {"<prefix><pass>"})
    pub fn pass_opt(&self, prefix: &str) -> String {
        if self.pool.Pass.is_empty() {
            String::new()
        } else {
            format!("{}{}", prefix, self.pool.Pass)
        }
    }
    /// <protocol>://<host>:<pool port>
    pub fn url(&self) -> String {
        format!("{}://{}:{}", self.pool.Protocol, self.pool.Host, self.pool_port)
    }
    /// <protocol>://<host>:<port> (the pool's main port, CPU miners)
    pub fn url_port(&self) -> String {
        format!("{}://{}:{}", self.pool.Protocol, self.pool.Host, self.pool.Port)
    }
    /// a value from the module's Vars
    pub fn var(&self, k: &str) -> &str {
        self.fam.vars.get(k).map(|s| s.as_str()).unwrap_or("")
    }
    pub fn var_b(&self, k: &str) -> bool {
        let v = self.var(k);
        !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
    }
}

pub type MakeArgs = fn(&A) -> String;
/// PerModel: the device filter of a model (replaces the Arch filters)
pub type ModelFilter = fn(&Family, &Device) -> bool;
/// PreKey: the device filter of a command
pub type CmdFilter = fn(&MinerCtx, &Device, &Value) -> bool;
/// PerKey: the device filter of a pool key (pool may be missing)
pub type KeyFilter = fn(&Family, &MinerCtx, &Device, &Value, Option<&Pool>, &str) -> bool;

/// the -Setup hashtable of a stub
pub struct Family {
    pub name: &'static str,
    /// the DevicesByTypes key: NVIDIA, AMD, INTEL or CPU
    pub vendor: &'static str,
    pub info_types: Vec<String>,
    /// path relative to the RainbowMiner folder (".\Bin\..." style is fine)
    pub path: String,
    pub uri: String,
    /// (Uri, Cuda, Version) newest first; the first one that fits the installed CUDA wins
    pub uri_cuda: Vec<(String, String, String)>,
    /// on Linux the chosen CUDA version without dots is appended to the path (CcminerMtp)
    pub path_append_cuda: bool,
    pub manual_uri: String,
    /// "102{0:d2}" -> "102"
    pub port: String,
    pub dev_fee: f64,
    pub version: String,
    pub commands: &'static LazyLock<Vec<Value>>,
    pub suffix: Suffix,
    pub check_ssl: bool,
    pub max_dev_count: usize,
    pub per_entry_vram: bool,
    pub arch_in: &'static [&'static str],
    pub arch_not_in: &'static [&'static str],
    pub use_algo_override: bool,
    pub cmd_filter: bool,
    pub cpu_params: bool,
    pub extend_default: i64,
    pub dev_fee_zero: bool,
    /// HashRateMode "DayPenalty"
    pub day_penalty: bool,
    pub max_rej_default: Option<f64>,
    pub mining_priority: Option<i32>,
    pub use_exclude_pool: bool,
    pub use_coin_symbols: bool,
    pub emits_no_cpu: bool,
    pub emits_max_rej: bool,
    pub api: &'static str,
    pub first_per_cmd: bool,
    pub needs_platform_id: bool,
    pub env_vars: Vec<String>,
    pub dev_id_prop: &'static str,
    pub dev_id_join: &'static str,
    pub dev_id_hex: bool,
    pub dev_fee_cmd: bool,
    pub win_or_single_dev: bool,
    pub vars: HashMap<&'static str, String>,
    pub make_args: MakeArgs,
    pub model_filter: Option<ModelFilter>,
    pub pre_key_filter: Option<CmdFilter>,
    pub per_key_filter: Option<KeyFilter>,
    /// PreKey hooks that rebuild $DeviceParams from threads/affinity
    pub device_params: Option<fn(&MinerCtx, Option<i64>, Option<&str>) -> String>,
    /// Prerequisite (path, uri, message) of a command
    pub prerequisite: Option<fn(&MinerCtx, &Value) -> Option<(std::path::PathBuf, String, String)>>,
}

fn no_args(_: &A) -> String {
    String::new()
}

impl Family {
    pub fn new(name: &'static str, vendor: &'static str, commands: &'static LazyLock<Vec<Value>>) -> Family {
        Family {
            name,
            vendor,
            info_types: vec![],
            path: String::new(),
            uri: String::new(),
            uri_cuda: vec![],
            path_append_cuda: false,
            manual_uri: String::new(),
            port: String::new(),
            dev_fee: 0.0,
            version: String::new(),
            commands,
            suffix: Suffix::Keys,
            check_ssl: false,
            max_dev_count: 0,
            per_entry_vram: false,
            arch_in: &[],
            arch_not_in: &[],
            use_algo_override: false,
            cmd_filter: false,
            cpu_params: false,
            extend_default: 0,
            dev_fee_zero: false,
            day_penalty: false,
            max_rej_default: None,
            mining_priority: None,
            use_exclude_pool: false,
            use_coin_symbols: false,
            emits_no_cpu: false,
            emits_max_rej: false,
            api: "Ccminer",
            first_per_cmd: false,
            needs_platform_id: false,
            env_vars: vec![],
            dev_id_prop: "Type_Vendor_Index",
            dev_id_join: ",",
            dev_id_hex: false,
            dev_fee_cmd: false,
            win_or_single_dev: false,
            vars: HashMap::new(),
            make_args: no_args,
            model_filter: None,
            pre_key_filter: None,
            per_key_filter: None,
            device_params: None,
            prerequisite: None,
        }
    }

    /// add (Uri, Cuda) entries, newest first
    pub fn cuda(mut self, list: &[(&str, &str)]) -> Self {
        self.uri_cuda = list.iter().map(|(u, c)| (u.to_string(), c.to_string(), String::new())).collect();
        self
    }
    /// add (Uri, Cuda, Version) entries, newest first
    pub fn cuda_v(mut self, list: &[(&str, &str, &str)]) -> Self {
        self.uri_cuda = list.iter().map(|(u, c, v)| (u.to_string(), c.to_string(), v.to_string())).collect();
        self
    }

    /// $Uri = if ($UriCuda) {$UriCuda.Uri} else {$Uri} (InfoOnly)
    pub fn info_uri(&self) -> String {
        match self.uri_cuda.first() {
            Some((u, _, _)) => u.clone(),
            None => self.uri.clone(),
        }
    }

    pub fn types(&self) -> Vec<String> {
        if self.info_types.is_empty() {
            vec![self.vendor.to_string()]
        } else {
            self.info_types.clone()
        }
    }

    /// the CUDA selection of the stub: false if no build fits the installed CUDA version
    fn select_cuda(&mut self, ctx: &MinerCtx) -> bool {
        if self.uri_cuda.is_empty() {
            return true;
        }
        let n = self.uri_cuda.len();
        for i in 0..n {
            let (uri, cuda, version) = self.uri_cuda[i].clone();
            if ctx.cuda_ok(&cuda, if i + 1 < n { "" } else { self.name }) {
                self.uri = uri;
                if !version.is_empty() {
                    self.version = version;
                }
                if self.path_append_cuda && ctx.is_linux() {
                    self.path = format!("{}{}", self.path, cuda.replace('.', ""));
                }
                return true;
            }
        }
        false
    }

    pub fn info(&self, ctx: &MinerCtx) -> MinerInfo {
        MinerInfo {
            name: self.name.into(),
            types: self.types(),
            path: if self.path.is_empty() { Default::default() } else { ctx.path(&self.path) },
            uri: self.info_uri(),
            dev_fee: self.dev_fee,
            manual_uri: self.manual_uri.clone(),
            commands: Value::Array((*self.commands).clone()),
        }
    }

    fn arch_ok(&self, d: &Device) -> bool {
        if !self.arch_in.is_empty() {
            self.arch_in.iter().any(|a| a.eq_ignore_ascii_case(&d.OpenCL.Architecture))
        } else if !self.arch_not_in.is_empty() {
            !self.arch_not_in.iter().any(|a| a.eq_ignore_ascii_case(&d.OpenCL.Architecture))
        } else {
            true
        }
    }

    /// the body of Invoke-MinerFamily
    pub fn miners(mut self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        if !ctx.has_type(self.vendor) || !self.select_cuda(ctx) {
            return out;
        }
        let is_gpu = self.vendor != "CPU";
        let path_default = self.path.clone();
        let neverprofitable = ctx.config.b("EnableNeverprofitableAlgos");
        let fam = &self;
        for model in ctx.models(fam.vendor) {
            let mut first = true;
            let all: Vec<&Device> = ctx.devices_of(fam.vendor, &model);
            if all.is_empty() {
                continue;
            }
            let mut platform_id = 0usize;
            if fam.needs_platform_id {
                let mut ids: Vec<usize> = all.iter().map(|d| d.PlatformId).collect();
                ids.sort();
                ids.dedup();
                if ids.len() != 1 {
                    continue;
                }
                platform_id = ids[0];
            }
            let mut miner_device: Vec<&Device> = if fam.per_entry_vram {
                vec![]
            } else {
                all.iter().copied().filter(|d| fam.arch_ok(d)).collect()
            };
            if let Some(f) = fam.model_filter {
                miner_device = all.iter().copied().filter(|d| f(fam, d)).collect();
            }
            let (mut miner_port, mut mname, mut ids) = (0u16, String::new(), String::new());
            for cmd in fam.commands.iter() {
                if fam.cmd_filter && ((cmd_b(cmd, "LinuxOnly") && !ctx.is_linux()) || (cmd_b(cmd, "NeverProfitable") && !neverprofitable)) {
                    continue;
                }
                if fam.first_per_cmd {
                    first = true;
                }
                let algo0 = if fam.use_algo_override {
                    let a = cmd_s(cmd, "Algorithm");
                    crate::data::get_algorithm(if a.is_empty() { cmd_s(cmd, "MainAlgorithm") } else { a }.as_str())
                } else {
                    crate::data::get_algorithm(&cmd_s(cmd, "MainAlgorithm"))
                };
                if fam.per_entry_vram {
                    let min = jsonx::num(jsonx::get(cmd, "MinMemGB"));
                    miner_device = all.iter().copied().filter(|d| ctx.test_vram(d, min) && fam.arch_ok(d)).collect();
                }
                let (mut threads, mut affinity) = (None, None);
                let mut device_params = String::new();
                if fam.cpu_params {
                    threads = ctx.cpu_threads(fam.name, &algo0);
                    affinity = ctx.cpu_affinity(fam.name, &algo0);
                    device_params = format!(
                        "{}{}",
                        threads.map(|t| format!(" -t {}", t)).unwrap_or_default(),
                        affinity.as_ref().map(|a| format!(" --cpu-affinity {}", a)).unwrap_or_default()
                    );
                }
                if let Some(dp) = fam.device_params {
                    device_params = dp(ctx, threads, affinity.as_deref());
                }
                if let Some(f) = fam.pre_key_filter {
                    miner_device = all.iter().copied().filter(|d| f(ctx, d, cmd)).collect();
                }
                let exclude = cmd_s(cmd, "ExcludePoolName");
                let coin_symbols = cmd_list(cmd, "CoinSymbols");
                let keys: Vec<String> = match fam.suffix {
                    Suffix::ListGPU => vec![algo0.clone(), format!("{}-{}", algo0, model), format!("{}-GPU", algo0)],
                    Suffix::ListCPU => vec![algo0.clone(), format!("{}-{}", algo0, model)],
                    Suffix::KeysNoGPU => ctx.algo_keys(fam.name, &algo0, &model, KeyOpts { no_gpu: true, exclude_pool_name: &exclude, ..Default::default() }),
                    Suffix::KeysCoin => ctx.algo_keys(
                        fam.name,
                        &algo0,
                        &model,
                        KeyOpts { exclude_pool_name: &exclude, coin_symbols: &coin_symbols, ..Default::default() },
                    ),
                    Suffix::Keys => ctx.algo_keys(fam.name, &algo0, &model, KeyOpts { exclude_pool_name: &exclude, ..Default::default() }),
                };
                let prereq = fam.prerequisite.and_then(|p| p(ctx, cmd));
                for key in keys {
                    let pool = ctx.pool(&key);
                    if let Some(f) = fam.per_key_filter {
                        miner_device = all.iter().copied().filter(|d| f(fam, ctx, d, cmd, pool, &algo0)).collect();
                    }
                    let Some(pool) = pool else { continue };
                    if (fam.check_ssl && pool.SSL)
                        || pool.Host.is_empty()
                        || miner_device.is_empty()
                        || (fam.max_dev_count > 0 && miner_device.len() > fam.max_dev_count)
                        || (fam.win_or_single_dev && !ctx.is_windows() && miner_device.len() != 1)
                        || (fam.use_exclude_pool && !exclude.is_empty() && ps_match(&pool.Host, &exclude))
                        || (fam.use_coin_symbols && !coin_symbols.is_empty() && !util::contains_ci(&coin_symbols, &pool.CoinSymbol))
                    {
                        continue;
                    }
                    if first {
                        miner_port = port_fmt(&fam.port, miner_device[0].Index);
                        mname = miner_name(fam.name, None, &miner_device);
                        if is_gpu {
                            ids = device_ids(&miner_device, fam.dev_id_prop, fam.dev_id_join, fam.dev_id_hex);
                        }
                        first = false;
                    }
                    let pool_port = if is_gpu { pool.port_gpu() } else { pool.Port };
                    let a = A {
                        ctx,
                        fam,
                        cmd,
                        pool,
                        key: &key,
                        algo0: &algo0,
                        pool_port,
                        ids: &ids,
                        devices: &miner_device,
                        model: &model,
                        device_params: &device_params,
                        threads,
                        affinity: affinity.as_deref(),
                        platform_id,
                    };
                    let args = (fam.make_args)(&a);
                    let hashrate = if fam.day_penalty {
                        let pen = cmd_f(cmd, "Penalty").unwrap_or(1.0);
                        ctx.hashrate_dur(&mname, &algo0, "Day").map(|h| h * pen)
                    } else {
                        ctx.hashrate_dur(&mname, &algo0, &cmd_s(cmd, "HashrateDuration"))
                    };
                    let devfee = if fam.dev_fee_zero {
                        0.0
                    } else if fam.dev_fee_cmd {
                        cmd_f(cmd, "DevFee").unwrap_or(fam.dev_fee)
                    } else {
                        fam.dev_fee
                    };
                    let cmd_path = cmd_s(cmd, "Path");
                    let path = ctx.path(if cmd_path.is_empty() { &path_default } else { &cmd_path });
                    let mut m = MinerCandidate::new(fam.name, &mname, &miner_device, &model, path, fam.api, miner_port).with_algo(&key, hashrate, devfee);
                    m.arguments = MinerArgs::Str(args);
                    m.uri = fam.uri.clone();
                    m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                    m.extend_interval = cmd_f(cmd, "ExtendInterval").map(|x| x as i64).unwrap_or(fam.extend_default);
                    m.manual_uri = fam.manual_uri.clone();
                    m.env_vars = fam.env_vars.clone();
                    m.version = fam.version.clone();
                    m.base_algorithm = algo0.clone();
                    m.mining_priority = fam.mining_priority;
                    if fam.use_exclude_pool {
                        m.exclude_pool_name = exclude.clone();
                    }
                    if fam.emits_no_cpu {
                        m.no_cpu_mining = cmd_b(cmd, "NoCPUMining");
                    }
                    if fam.emits_max_rej {
                        m.max_rejected_share_ratio = cmd_f(cmd, "MaxRejectedShareRatio").filter(|x| *x != 0.0).or(fam.max_rej_default);
                    }
                    if let Some((p, u, msg)) = &prereq {
                        m.prerequisite_path = Some(p.clone());
                        m.prerequisite_uri = u.clone();
                        m.prerequisite_msg = msg.clone();
                    }
                    out.push(m);
                }
            }
        }
        out
    }
}

/// a family module of the registry
pub struct FamilyModule {
    pub name: &'static str,
    pub types: &'static [&'static str],
    /// the stub: guards, header variables and Setup; None = the module returns nothing
    pub def: fn(&MinerCtx, bool) -> Option<Family>,
}

impl MinerModule for FamilyModule {
    fn name(&self) -> &'static str {
        self.name
    }
    fn types(&self) -> Vec<String> {
        self.types.iter().map(|s| s.to_string()).collect()
    }
    fn info(&self, ctx: &MinerCtx) -> Option<MinerInfo> {
        (self.def)(ctx, true).map(|f| f.info(ctx))
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        match (self.def)(ctx, false) {
            Some(f) => f.miners(ctx),
            None => vec![],
        }
    }
}

/// $(if($f.avx2 -and $f.sha -and $f.aes){'avx2-sha'}elseif(...)...else{'sse2'}): the value of the first
/// rule whose features are all present ("avx2 sha aes"; "vendor:AMD" tests the CPU vendor)
pub fn cpu_pick(ctx: &MinerCtx, rules: &[(&str, &'static str)], default: &'static str) -> &'static str {
    for (cond, v) in rules {
        let ok = cond.split_whitespace().all(|f| match f.strip_prefix("vendor:") {
            Some(vendor) => ctx.cpu.Vendor.eq_ignore_ascii_case(vendor),
            None => ctx.cpu.has(f),
        });
        if ok {
            return v;
        }
    }
    default
}

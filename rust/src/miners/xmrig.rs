//! Miners/Xmrig.ps1 (AMD, CPU, INTEL, NVIDIA): argument objects for the XMRig6 API class,
//! which writes the per hardware signature config files.

use crate::data;
use crate::device::Device;
use crate::jsonx;
use crate::miner::*;
use crate::util;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::LazyLock;

pub struct Xmrig;

const NAME: &str = "Xmrig";
const PORT: &str = "303";
const VERSION: &str = "6.26.0.3";
const DEVFEE: f64 = 0.0;
const MANUAL_URI: &str = "https://github.com/xmrig/xmrig/releases";

static COMMANDS: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Xmrig.json")));

const CUDA_LINUX: &[&str] = &[
    "12.9", "12.8", "12.6", "12.5", "12.4", "12.3", "12.2", "12.1", "12.0", "11.8", "11.7", "11.6", "11.5", "11.4", "11.3", "11.2", "11.1", "11.0", "10.2", "10.1", "10.0", "9.2",
];
const CUDA_WINDOWS: &[&str] = &[
    "12.9", "12.8", "12.6", "12.5", "12.4", "12.3", "12.2", "12.1", "12.0", "11.8", "11.7", "11.6", "11.5", "11.4", "11.3", "11.2", "11.1", "11.0", "10.2",
];

fn cuda_data() -> Vec<(String, String)> {
    let (list, os) = if cfg!(windows) { (CUDA_WINDOWS, "win64") } else { (CUDA_LINUX, "linux") };
    list.iter()
        .map(|c| {
            (
                format!(
                    "https://github.com/RainbowMiner/miner-binaries/releases/download/v6.22.1-xmrigcuda/xmrig-cuda-6.22.1-cuda{}-{}.7z",
                    c.replace('.', "_"),
                    os
                ),
                c.to_string(),
            )
        })
        .collect()
}

struct Setup {
    uri: String,
    path: PathBuf,
    path_mo: PathBuf,
    cuda_lib: &'static str,
}

fn setup(ctx: &MinerCtx) -> Option<Setup> {
    if cfg!(windows) {
        return Some(Setup {
            uri: "https://github.com/RainbowMiner/miner-binaries/releases/download/v6.26.0-xmrig/xmrig-6.26.0.3-msvc-win64.7z".into(),
            path: ctx.path("Bin/ANY-Xmrig/xmrig"),
            path_mo: ctx.path("Bin/ANY-Xmrig/xmrig-mo"),
            cuda_lib: "xmrig-cuda.dll",
        });
    }
    if !cfg!(target_os = "linux") {
        return None;
    }
    let uri = if ctx.is_arm() {
        if ctx.cpu.Architecture != "8" {
            return None;
        }
        let d = ctx.distro(&["jammy", "noble"], "jammy");
        format!("https://github.com/RainbowMiner/miner-binaries/releases/download/v6.26.0-xmrig/xmrig-6.26.0.3-{}-armv8.7z", d)
    } else {
        if let Some(v) = &ctx.libc_version {
            if *v < util::get_version("2.25").unwrap() {
                return None;
            }
        }
        let d = ctx.distro(&["focal", "jammy", "noble"], "focal");
        format!("https://github.com/RainbowMiner/miner-binaries/releases/download/v6.26.0-xmrig/xmrig-6.26.0.3-{}-x64.7z", d)
    };
    Some(Setup {
        uri,
        path: ctx.path("Bin/ANY-Xmrig/xmrig"),
        path_mo: ctx.path("Bin/ANY-Xmrig/xmrig-mo"),
        cuda_lib: "libxmrig-cuda.so",
    })
}

const MO_COINS: &[&str] = &["XMR", "SUMO", "ARQ", "GRFT", "KVA", "RVN", "WOW", "XEQ", "ZEPH", "Townforge"];

fn algo_perf() -> Value {
    let mut m = serde_json::Map::new();
    for a in [
        "cn/0", "cn/1", "cn/2", "cn/r", "cn/fast", "cn/half", "cn/xao", "cn/rto", "cn/rwz", "cn/zls", "cn/double", "cn/ccx", "cn-lite/0", "cn-lite/1", "cn-heavy/xhv", "cn-pico",
        "cn-pico/tlo", "cn/gpu", "flex", "rx/0", "rx/2", "rx/arq", "rx/graft", "rx/sfx", "rx/xeq", "rx/yada", "panthera", "argon2/chukwav2", "astrobwt", "astrobwt/v2", "kawpow",
        "ghostrider",
    ] {
        m.insert(a.into(), json!(0));
    }
    Value::Object(m)
}

impl MinerModule for Xmrig {
    fn name(&self) -> &'static str {
        NAME
    }
    fn types(&self) -> Vec<String> {
        vec!["AMD".into(), "CPU".into(), "INTEL".into(), "NVIDIA".into(), "ARMCPU".into()]
    }
    fn info(&self, ctx: &MinerCtx) -> MinerInfo {
        let s = setup(ctx);
        MinerInfo {
            name: NAME.into(),
            types: self.types(),
            path: s.as_ref().map(|s| s.path.clone()).unwrap_or_else(|| ctx.path("Bin/ANY-Xmrig/xmrig")),
            uri: s.map(|s| s.uri).unwrap_or_default(),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: Value::Array(COMMANDS.clone()),
        }
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        if !["AMD", "CPU", "INTEL", "NVIDIA"].iter().any(|v| ctx.has_type(v)) {
            return out;
        }
        let Some(s) = setup(ctx) else { return out };
        let cuda_list = cuda_data();
        let mut uri_cuda: Option<String> = None;
        let mut cuda: Option<String> = None;
        if !ctx.config.cuda_version.is_empty() {
            for (i, (u, c)) in cuda_list.iter().enumerate() {
                let warn = if i + 1 < cuda_list.len() || !ctx.has_type("NVIDIA") { "" } else { NAME };
                if ctx.cuda_ok(c, warn) {
                    uri_cuda = Some(u.clone());
                    cuda = Some(c.clone());
                    break;
                }
            }
        }
        if cfg!(windows) && cuda.is_none() {
            uri_cuda = cuda_list.last().map(|x| x.0.clone());
        }
        let re_cpu1 = regex::Regex::new(r"(?i)(\(R\)|\(TM\)|CPU|Processor)").unwrap();
        let re_cpu2 = regex::Regex::new(r"(?i)[^A-Z0-9]").unwrap();
        let mut path_cuda_lib: Option<PathBuf> = None;
        if let Some(uc) = &uri_cuda {
            let dir = ctx.base.join("Bin").join("ANY-Xmrig");
            let lib = dir.join(s.cuda_lib);
            let urilib = dir.join("_urilib.json");
            let mut update = true;
            if let Some(cur) = util::read_text(&urilib).ok().and_then(|t| jsonx::parse(&t)) {
                if jsonx::s(jsonx::get(&cur, "URI")) != *uc {
                    let _ = std::fs::remove_file(&lib);
                } else {
                    update = false;
                }
            }
            if update && dir.exists() {
                let _ = std::fs::write(&urilib, serde_json::to_string_pretty(&json!({"URI": uc})).unwrap_or_default());
            }
            path_cuda_lib = Some(lib);
        }
        for vendor in ["AMD", "CPU", "INTEL", "NVIDIA"] {
            if vendor == "NVIDIA" && cuda.is_none() {
                continue;
            }
            for model in ctx.models(vendor) {
                let device: Vec<&Device> = ctx.devices_of(vendor, &model);
                if device.is_empty() {
                    continue;
                }
                let mut platform_id = None;
                if vendor == "AMD" || vendor == "INTEL" {
                    let mut ids: Vec<usize> = device.iter().map(|d| d.PlatformId).collect();
                    ids.sort();
                    ids.dedup();
                    if ids.len() != 1 {
                        continue;
                    }
                    platform_id = Some(ids[0]);
                }
                for cmd in COMMANDS.iter().filter(|c| cmd_vendor(c, vendor)) {
                    let cmd_vendors = cmd_list(cmd, "Vendor");
                    if !(cmd_vendors.iter().any(|v| v != "NVIDIA") || cmd_s(cmd, "MainAlgorithm") != "rx/grft" || ps_match(&s.uri, r"\ddev")) {
                        continue;
                    }
                    let mut first = true;
                    let algorithm = {
                        let a = cmd_s(cmd, "Algorithm");
                        if a.is_empty() {
                            cmd_s(cmd, "MainAlgorithm")
                        } else {
                            a
                        }
                    };
                    let algo0 = data::get_algorithm(&cmd_s(cmd, "MainAlgorithm"));
                    let exclude = cmd_s(cmd, "ExcludePoolName");
                    let keys = ctx.algo_keys(NAME, &algo0, &model, KeyOpts { no_gpu: vendor == "CPU", exclude_pool_name: &exclude, ..Default::default() });
                    let by_params = cmd_b(cmd, "ByParameters");
                    let (mut miner_port, mut mname, mut device_params, mut codec, mut threads, mut affinity) = (0u16, String::new(), String::new(), "", None, None);
                    for key in keys {
                        let Some(pool) = ctx.pool(&key) else { continue };
                        if pool.Host.is_empty() {
                            continue;
                        }
                        let min_mem = ctx.min_mem_gb(cmd, pool, &algo0);
                        let miner_device: Vec<&Device> = device.iter().copied().filter(|d| vendor == "CPU" || ctx.test_vram(d, min_mem)).collect();
                        if miner_device.is_empty() || (!exclude.is_empty() && ps_match(&pool.Host, &exclude)) {
                            continue;
                        }
                        if first {
                            miner_port = port_fmt(PORT, miner_device[0].Index);
                            mname = miner_name(NAME, None, &miner_device);
                            first = false;
                            threads = None;
                            affinity = None;
                            device_params = if vendor == "CPU" {
                                codec = "cpu";
                                threads = ctx.cpu_threads(NAME, &algo0);
                                affinity = ctx.cpu_affinity(NAME, &algo0);
                                format!(
                                    "{}{}",
                                    if by_params { threads.map(|t| format!(" -t {}", t)).unwrap_or_default() } else { String::new() },
                                    if by_params { affinity.as_ref().map(|a| format!(" --cpu-affinity={}", a)).unwrap_or_default() } else { String::new() }
                                )
                            } else if vendor == "AMD" || vendor == "INTEL" || cmd_b(cmd, "UseOpenCL") {
                                codec = "opencl";
                                format!(
                                    "--opencl --opencl-devices={} --opencl-platform={} --no-cpu",
                                    device_ids(&miner_device, "Type_PlatformId_Index", ",", false),
                                    platform_id.unwrap_or(0)
                                )
                            } else {
                                codec = "cuda";
                                format!(
                                    "--cuda --cuda-loader={} --cuda-devices={} --no-nvml --no-cpu",
                                    s.cuda_lib,
                                    device_ids(&miner_device, "Type_Vendor_Index", ",", false)
                                )
                            };
                        }
                        let use_mo = (cmd_b(cmd, "UseMO") && ps_match(&pool.Host, "C3pool|MoneroOcean")) || cmd_b(cmd, "ForceMO");
                        let arguments = if by_params {
                            MinerArgs::Str(format!(
                                "-a {} -o {}:{} -u {}{}{}{} --donate-level={} --keepalive --http-enabled --http-host=127.0.0.1 --http-port=$mport{} {}",
                                algorithm,
                                pool.Host,
                                pool.Port,
                                pool.User,
                                if pool.Pass.is_empty() { String::new() } else { format!(" -p {}", pool.Pass) },
                                if pool.SSL { " --tls" } else { "" },
                                if ps_match(&pool.Host, "Nicehash") { " --nicehash" } else { "" },
                                if use_mo { 1 } else { 0 },
                                device_params,
                                cmd_s(cmd, "Params")
                            ))
                        } else {
                            let coin = if MO_COINS.iter().any(|c| c.eq_ignore_ascii_case(&pool.CoinSymbol)) { json!(pool.CoinSymbol) } else { Value::Null };
                            let hwsig = if vendor == "CPU" {
                                let n = re_cpu2.replace_all(&re_cpu1.replace_all(&ctx.cpu.Name, ""), "").to_string();
                                format!("{}x{}", ctx.by_type("CPU").len(), n)
                            } else {
                                let mut idx: Vec<usize> = miner_device.iter().map(|d| d.Type_Vendor_Index).collect();
                                idx.sort();
                                format!("{}-{}", model, idx.iter().map(|i| format!("{:x}", i)).collect::<String>())
                            };
                            let mut config = json!({
                                "api": {"id": null, "worker-id": null},
                                "background": false,
                                "colors": true,
                                "randomx": {"init": -1, "numa": true},
                                "donate-level": 0,
                                "log-file": null,
                                "print-time": 5,
                                "retries": 5,
                                "retry-pause": 1
                            });
                            let mut hwsig = hwsig;
                            if use_mo {
                                hwsig = format!("{}_mo", hwsig);
                                if let Value::Object(m) = &mut config {
                                    m.insert("rebench-algo".into(), json!(false));
                                    m.insert("bench-algo-time".into(), json!(0));
                                    m.insert("algo-min-time".into(), json!(86400));
                                    m.insert("donate-level".into(), json!(1));
                                    m.insert("algo-perf".into(), algo_perf());
                                }
                            }
                            MinerArgs::Obj(json!({
                                "Algorithm": algorithm,
                                "APIParams": "--http-enabled --http-host=127.0.0.1 --http-port=$mport",
                                "DeviceParams": device_params,
                                "Config": config,
                                "Pools": [{
                                    "algo": algorithm,
                                    "coin": coin,
                                    "url": format!("{}://{}:{}", pool.Protocol, pool.Host, pool.Port),
                                    "user": pool.User,
                                    "pass": if pool.Pass.is_empty() { "x".to_string() } else { pool.Pass.clone() },
                                    "rig-id": pool.Worker,
                                    "nicehash": ps_match(&pool.Host, "NiceHash"),
                                    "keepalive": true,
                                    "enabled": true,
                                    "tls": pool.SSL,
                                    "ip_version": 4
                                }],
                                "Vendor": vendor,
                                "Params": cmd_s(cmd, "Params"),
                                "HwSig": hwsig,
                                "Threads": if vendor == "CPU" { threads.unwrap_or(ctx.cpu.Cores as i64) } else { 1 },
                                "Devices": miner_device.iter().map(|d| d.Type_Vendor_Index).collect::<Vec<_>>(),
                                "Affinity": affinity,
                                "Codec": codec,
                            }))
                        };
                        let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, if use_mo { s.path_mo.clone() } else { s.path.clone() }, "XMRig6", miner_port)
                            .with_algo(&key, ctx.hashrate(&mname, &algo0), DEVFEE);
                        m.arguments = arguments;
                        m.uri = s.uri.clone();
                        m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                        m.extend_interval = cmd_i(cmd, "ExtendInterval");
                        m.manual_uri = MANUAL_URI.into();
                        m.version = VERSION.into();
                        m.base_algorithm = algo0.clone();
                        m.exclude_pool_name = exclude.clone();
                        m.prerequisite_path = path_cuda_lib.clone();
                        m.prerequisite_uri = uri_cuda.clone().unwrap_or_default();
                        m.prerequisite_msg = "Downloading Xmrig support files in the background, please wait!".into();
                        out.push(m);
                    }
                }
            }
        }
        out
    }
}

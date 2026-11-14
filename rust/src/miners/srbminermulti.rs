//! Miners/SrbMinerMulti.ps1 (AMD, CPU, INTEL, NVIDIA)
//!
//! GPU entries carry an argument object (HwSig, Devices, AutoTune), the SrbMinerMulti API class
//! keeps the autotune intensities per hardware signature in config files.

use super::opt;
use crate::data;
use crate::device::Device;
use crate::miner::*;
use crate::pool::Pool;
use crate::util;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::LazyLock;

pub struct SrbMinerMulti;

const NAME: &str = "SrbMinerMulti";
const PORT: &str = "349";
const DEVFEE: f64 = 0.85;
const VERSION: &str = "3.8.7";
const CUDA: &str = "11.7";
const MANUAL_URI: &str = "https://bitcointalk.org/index.php?topic=5190081.0";
const VALID_COMPUTE_AMD: &[&str] = &["RDNA4", "RDNA3", "RDNA2", "RDNA1", "GCN51", "GCN50"];
const VALID_COMPUTE_NVIDIA: &[&str] = &["Volta", "Pascal", "Turing", "Ampere", "Ada", "Hopper", "Blackwell"];

static COMMANDS: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SrbMinerMulti.json")));

fn uri() -> &'static str {
    if cfg!(windows) {
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.8.7-srbminermulti/SRBMiner-Multi-3-8-7-win64.zip"
    } else {
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.8.7-srbminermulti/SRBMiner-Multi-3-8-7-Linux.tar.gz"
    }
}

fn verthash_dat(base: &std::path::Path) -> Option<PathBuf> {
    if base.join("Bin/ANY-SRBMinerMulti/Cache/verthash.dat").exists() {
        return None;
    }
    let big = |p: &PathBuf| std::fs::metadata(p).map(|m| m.len() as f64 >= 1.19 * 1073741824.0).unwrap_or(false);
    let home = if cfg!(windows) {
        std::env::var("APPDATA").map(|a| PathBuf::from(a).join("Vertcoin").join("verthash.dat")).ok()
    } else {
        std::env::var("HOME").map(|h| PathBuf::from(h).join(".vertcoin").join("verthash.dat")).ok()
    };
    if let Some(h) = home {
        if big(&h) {
            return Some(h);
        }
    }
    let c = base.join("Bin/Common/verthash.dat");
    if big(&c) {
        Some(c)
    } else {
        None
    }
}

fn esc_pass(p: &str) -> String {
    p.replace(';', "#;").replace('!', "#!")
}

fn esm(p: &Pool) -> &'static str {
    match p.EthMode.as_str() {
        "ethproxy" | "qtminer" => " --esm 0",
        "minerproxy" => " --esm 1",
        "ethstratum" | "ethstratum1" | "ethstratumnh" => " --esm 2",
        _ => "",
    }
}

fn pool_port(p: &Pool, cpu: bool) -> u32 {
    if cpu {
        p.port_cpu()
    } else {
        p.port_gpu()
    }
}

fn nicehash(p: &Pool) -> &'static str {
    if ps_match(&p.Host, "Gteh|NiceHash") {
        "true"
    } else {
        "false"
    }
}

fn compute_of<'a>(d: &'a Device, param: &str) -> &'a str {
    if param == "Architecture" {
        d.OpenCL.Architecture.as_str()
    } else {
        d.OpenCL.DeviceCapability.as_deref().unwrap_or("")
    }
}

impl MinerModule for SrbMinerMulti {
    fn name(&self) -> &'static str {
        NAME
    }
    fn types(&self) -> Vec<String> {
        vec!["AMD".into(), "CPU".into(), "INTEL".into(), "NVIDIA".into()]
    }
    fn info(&self, ctx: &MinerCtx) -> MinerInfo {
        MinerInfo {
            name: NAME.into(),
            types: self.types(),
            path: ctx.path("Bin/ANY-SRBMinerMulti/SRBMiner-MULTI"),
            uri: uri().into(),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: Value::Array(COMMANDS.clone()),
        }
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        if !(ctx.is_linux() || ctx.is_windows()) || (ctx.is_linux() && ctx.is_arm()) {
            return out;
        }
        if !["AMD", "CPU", "INTEL", "NVIDIA"].iter().any(|v| ctx.has_type(v)) {
            return out;
        }
        let cuda = !ctx.has_type("NVIDIA") || ctx.cuda_ok(CUDA, NAME);
        let path = ctx.path("Bin/ANY-SRBMinerMulti/SRBMiner-MULTI");
        let verthash = verthash_dat(ctx.base);
        for vendor in ["AMD", "CPU", "INTEL", "NVIDIA"] {
            if vendor == "NVIDIA" && !cuda {
                continue;
            }
            let is_cpu = vendor == "CPU";
            for model in ctx.models(vendor) {
                let device: Vec<&Device> = ctx.devices_of(vendor, &model);
                if device.is_empty() {
                    continue;
                }
                for cmd in COMMANDS.iter().filter(|c| cmd_vendor(c, vendor) && (!cmd_b(c, "LinuxOnly") || cfg!(target_os = "linux"))) {
                    let mut first = true;
                    let main_algorithm = cmd_s(cmd, "MainAlgorithm");
                    let second_algorithm = cmd_s(cmd, "SecondaryAlgorithm");
                    let algo0 = data::get_algorithm(&main_algorithm);
                    let algo2_0 = if second_algorithm.is_empty() { None } else { Some(data::get_algorithm(&second_algorithm)) };
                    let exclude_compute = cmd_list(cmd, "ExcludeCompute");
                    let include_compute = cmd_list(cmd, "IncludeCompute");
                    let cpu_features = cmd_list(cmd, "CPUFeatures");
                    let mut compute: Vec<String> = Vec::new();
                    let mut compute_param = "";
                    let mut compute_avoid: Vec<String> = Vec::new();
                    let mut compute_avoid_param = "";
                    let mut device_params = match vendor {
                        "CPU" => {
                            let threads = ctx.cpu_threads(NAME, &algo0);
                            let affinity = ctx.cpu_affinity(NAME, &algo0);
                            let first_threads = device[0].Data.Threads.unwrap_or(ctx.cpu.Threads) as i64;
                            format!(
                                " --disable-gpu{}{}{}",
                                threads.map(|t| format!(" --cpu-threads {}", t)).unwrap_or_default(),
                                match &affinity {
                                    Some(a) if threads.map(|t| t <= 64).unwrap_or(true) => format!(" --cpu-affinity {}", a),
                                    _ => String::new(),
                                },
                                opt(threads.map(|t| first_threads <= t).unwrap_or(false), " --cpu-threads-priority 1".into())
                            )
                        }
                        "AMD" => {
                            compute = VALID_COMPUTE_AMD
                                .iter()
                                .map(|s| s.to_string())
                                .chain(include_compute.iter().cloned())
                                .filter(|c| !c.is_empty() && !util::contains_ci(&exclude_compute, c))
                                .collect();
                            compute_param = "DeviceCapability";
                            " --disable-cpu --disable-gpu-nvidia --disable-gpu-intel --enable-igpu".to_string()
                        }
                        "INTEL" => {
                            compute_param = "DeviceCapability";
                            " --disable-cpu --disable-gpu-amd --disable-gpu-nvidia --enable-igpu".to_string()
                        }
                        _ => {
                            compute = VALID_COMPUTE_NVIDIA.iter().map(|s| s.to_string()).filter(|c| !util::contains_ci(&exclude_compute, c)).collect();
                            compute_param = "Architecture";
                            compute_avoid = exclude_compute.iter().filter(|c| !VALID_COMPUTE_NVIDIA.iter().any(|v| v.eq_ignore_ascii_case(c))).cloned().collect();
                            if !compute_avoid.is_empty() {
                                compute_avoid_param = "DeviceCapability";
                            }
                            " --disable-cpu --disable-gpu-amd --disable-gpu-intel".to_string()
                        }
                    };
                    if main_algorithm.eq_ignore_ascii_case("verthash") {
                        if let Some(v) = &verthash {
                            device_params = format!(" --verthash-dat-path \"{}\"{}", v.display(), device_params);
                        }
                    }
                    if cmd_b(cmd, "Rx1GbPages") && ctx.config.b("EnableRandomX1GBPages") {
                        device_params.push_str(" --randomx-use-1gb-pages");
                    }
                    let exclude = cmd_s(cmd, "ExcludePoolName");
                    let coin_symbols = cmd_list(cmd, "CoinSymbols");
                    let exclude_yiimp = cmd_b(cmd, "ExcludeYiimp");
                    let keys = ctx.algo_keys(
                        NAME,
                        &algo0,
                        &model,
                        KeyOpts { no_gpu: is_cpu, exclude_pool_name: &exclude, coin_symbols: &coin_symbols, exclude_yiimp, ..Default::default() },
                    );
                    let second_keys = algo2_0.as_ref().map(|a2| ctx.second_keys(a2, &model, !is_cpu));
                    let yiimp_ok = |key0: &str| !exclude_yiimp || ctx.pool(key0).map(|p| !ctx.yiimp_pools.contains(&p.Name)).unwrap_or(true);
                    let (mut miner_port, mut mname, mut ids, mut busid_str, mut mallob, mut slow, mut miner_hr) =
                        (0u16, String::new(), String::new(), String::new(), String::new(), String::new(), None);
                    let algorithm_main = {
                        let a = cmd_s(cmd, "Algorithm");
                        if a.is_empty() {
                            main_algorithm.clone()
                        } else {
                            a
                        }
                    };
                    for key in keys {
                        let Some(pool) = ctx.pool(&key) else { continue };
                        if pool.Host.is_empty() {
                            continue;
                        }
                        let min_mem = ctx.min_mem_gb(cmd, pool, &algo0);
                        let miner_device: Vec<&Device> = device
                            .iter()
                            .copied()
                            .filter(|d| {
                                if is_cpu {
                                    cpu_features.is_empty() || cpu_features.iter().all(|f| ctx.cpu.has(f))
                                } else {
                                    (min_mem == 0.0 || ctx.test_vram(d, min_mem))
                                        && (compute.is_empty() || util::contains_ci(&compute, compute_of(d, compute_param)))
                                        && (compute_avoid.is_empty() || !util::contains_ci(&compute_avoid, compute_of(d, compute_avoid_param)))
                                }
                            })
                            .collect();
                        if miner_device.is_empty()
                            || (!coin_symbols.is_empty() && !util::contains_ci(&coin_symbols, &pool.CoinSymbol))
                            || (!exclude.is_empty() && ps_match(&pool.Host, &exclude))
                            || !yiimp_ok(&algo0)
                        {
                            continue;
                        }
                        if first {
                            miner_port = port_fmt(PORT, miner_device[0].Index);
                            mname = miner_name(NAME, algo2_0.as_ref().map(|a2| format!("{}-{}", algo0, a2)).as_deref(), &miner_device);
                            ids = device_ids(&miner_device, "BusId_Type_Vendor_Index", ",", false);
                            busid_str = if !is_cpu {
                                let re = regex::Regex::new(r"(?i)[^0-9a-f]").unwrap();
                                let mut s: String = miner_device.iter().map(|d| re.replace_all(d.BusId.as_deref().unwrap_or(""), "").to_string()).collect();
                                if s.is_empty() {
                                    s = miner_device.iter().map(|d| format!("{:x}", d.BusId_Type_Vendor_Index)).collect();
                                }
                                if s.is_empty() {
                                    s
                                } else {
                                    util::md5_hex(&s)[..4].to_lowercase()
                                }
                            } else {
                                String::new()
                            };
                            mallob = opt(!pool.Mallob.is_empty(), format!(" --mallob-endpoint {}", pool.Mallob));
                            slow = opt(
                                (cmd_b(cmd, "DAG") || main_algorithm.eq_ignore_ascii_case("verthash")) && miner_device.iter().any(|d| compute_of(d, compute_param) == "RDNA4"),
                                "  --gpu-table-slow-build".into(),
                            );
                            miner_hr = ctx.hashrate(&mname, &algo0);
                            if cmd_f(cmd, "MaxRejectedShareRatio").unwrap_or(0.0) != 0.0 {
                                miner_hr = miner_hr.map(|h| h * (1.0 - ctx.ratio_live(&mname, &algo0)));
                            }
                            first = false;
                        }
                        let proto = esm(pool);
                        let port1 = pool_port(pool, is_cpu);
                        let ext = cmd_f(cmd, "ExtendInterval").map(|x| x as i64).filter(|x| *x != 0).unwrap_or(if is_cpu { 2 } else { 3 });
                        let env = if vendor == "AMD" && cfg!(target_os = "linux") { vec!["GPU_MAX_WORKGROUP_SIZE=1024".to_string()] } else { vec![] };
                        let fee = cmd_f(cmd, "Fee").unwrap_or(DEVFEE);
                        let mut finish = |mut m: MinerCandidate, args: String, autotune: bool, fault: Option<f64>| {
                            m.arguments = if is_cpu {
                                MinerArgs::Str(args)
                            } else {
                                MinerArgs::Obj(json!({
                                    "Arguments": args,
                                    "Vendor": vendor,
                                    "HwSig": format!("{}-{}", model, busid_str),
                                    "Devices": miner_device.iter().map(|d| d.BusId_Type_Vendor_Index).collect::<Vec<_>>(),
                                    "AutoTune": autotune,
                                }))
                            };
                            m.uri = uri().into();
                            m.fault_tolerance = fault;
                            m.extend_interval = ext;
                            m.max_rejected_share_ratio = cmd_f(cmd, "MaxRejectedShareRatio").filter(|x| *x != 0.0);
                            m.manual_uri = MANUAL_URI.into();
                            m.env_vars = env.clone();
                            m.version = VERSION.into();
                            m.set_ld_library_path = Some(false);
                            m.list_devices = "--list-devices".into();
                            m.exclude_pool_name = exclude.clone();
                            m.log_file_args = "--log-file \"$logfile\" --log-file-mode 1".into();
                            out.push(m);
                        };
                        if let (Some(algo2_0), Some(skeys)) = (&algo2_0, &second_keys) {
                            let mut intens = ctx.intensities(NAME, &model, &algo0, algo2_0);
                            if intens.is_empty() {
                                intens = vec!["0".into()];
                            }
                            let algorithm2 = {
                                let a = cmd_s(cmd, "Algorithm2");
                                if a.is_empty() {
                                    second_algorithm.clone()
                                } else {
                                    a
                                }
                            };
                            for intensity in &intens {
                                let iv = util::parse_f64(intensity).unwrap_or(0.0);
                                let name_dual = if iv > 0.0 {
                                    miner_name(NAME, Some(&format!("{}-{}-{}", algo0, algo2_0, intensity)), &miner_device)
                                } else {
                                    mname.clone()
                                };
                                for key2 in skeys {
                                    let Some(pool2) = ctx.pool(key2) else { continue };
                                    if pool2.Host.is_empty()
                                        || (!coin_symbols.is_empty() && !util::contains_ci(&coin_symbols, &pool2.CoinSymbol))
                                        || (!exclude.is_empty() && ps_match(&pool2.Host, &exclude))
                                        || !yiimp_ok(algo2_0)
                                    {
                                        continue;
                                    }
                                    let passwords = opt(
                                        !pool.Pass.is_empty() || !pool2.Pass.is_empty(),
                                        format!(
                                            " --password {} --password {}",
                                            if pool.Pass.is_empty() { "x".into() } else { esc_pass(&pool.Pass) },
                                            if pool2.Pass.is_empty() { "x".into() } else { esc_pass(&pool2.Pass) }
                                        ),
                                    );
                                    let args = format!(
                                        "--gpu-id {} --gpu-dual-max-loss {} --algorithm {};{} --pool {}:{}{} --pool {}:{} --wallet {} --wallet {}{} --tls {} --tls {} --nicehash {} --nicehash {}{}{} --retry-time 10 --disable-startup-monitor{} --api-enable --api-port $mport {}",
                                        ids,
                                        intensity,
                                        algorithm_main,
                                        algorithm2,
                                        pool.Host,
                                        port1,
                                        proto,
                                        pool2.Host,
                                        pool_port(pool2, is_cpu),
                                        pool.User,
                                        pool2.User,
                                        passwords,
                                        pool.SSL,
                                        pool2.SSL,
                                        nicehash(pool),
                                        nicehash(pool2),
                                        mallob,
                                        slow,
                                        device_params,
                                        cmd_s(cmd, "Params")
                                    );
                                    let m = MinerCandidate::new(NAME, &name_dual, &miner_device, &model, path.clone(), "SrbMinerMulti", miner_port)
                                        .with_algo(&key, penalized(ctx.hashrate(&name_dual, &algo0), cmd), fee)
                                        .with_algo(key2, penalized(ctx.hashrate(&name_dual, algo2_0), cmd), 0.0);
                                    let mut m = m;
                                    m.base_algorithm = format!("{}-{}", algo0, algo2_0);
                                    finish(m, args, false, Some(0.7));
                                }
                            }
                        } else {
                            let passwords = opt(!pool.Pass.is_empty(), format!(" --password {}", esc_pass(&pool.Pass)));
                            let args = format!(
                                "--algorithm {} {} --pool {}:{}{} --wallet {}{} --tls {} --nicehash {}{}{} --retry-time 10 --disable-startup-monitor{} --autotune-no-load --api-enable --api-port $mport {}",
                                algorithm_main,
                                opt(!is_cpu, format!("--gpu-id {}", ids)),
                                pool.Host,
                                port1,
                                proto,
                                pool.User,
                                passwords,
                                pool.SSL,
                                nicehash(pool),
                                mallob,
                                slow,
                                device_params,
                                cmd_s(cmd, "Params")
                            );
                            let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, path.clone(), "SrbMinerMulti", miner_port).with_algo(&key, miner_hr, fee);
                            m.base_algorithm = algo0.clone();
                            finish(m, args, !cmd_b(cmd, "NoAutoTune"), cmd_f(cmd, "FaultTolerance"));
                        }
                    }
                }
            }
        }
        out
    }
}

//! Miners/SrbMinerMulti239.ps1, SrbMinerMulti244.ps1, SrbMinerMulti268.ps1, SrbMinerMulti292.ps1:
//! older SRBMiner-MULTI builds with plain argument strings (no autotune objects).

use super::opt;
use crate::data;
use crate::device::Device;
use crate::miner::*;
use crate::pool::Pool;
use crate::util;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum V {
    V239,
    V244,
    V268,
    V292,
}

pub struct SrbMinerMultiOld {
    v: V,
}

impl SrbMinerMultiOld {
    pub fn all() -> Vec<SrbMinerMultiOld> {
        vec![SrbMinerMultiOld { v: V::V239 }, SrbMinerMultiOld { v: V::V244 }, SrbMinerMultiOld { v: V::V268 }, SrbMinerMultiOld { v: V::V292 }]
    }
}

static CMD_239: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SrbMinerMulti239.json")));
static CMD_244: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SrbMinerMulti244.json")));
static CMD_268: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SrbMinerMulti268.json")));
static CMD_292: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SrbMinerMulti292.json")));

const MANUAL_URI: &str = "https://bitcointalk.org/index.php?topic=5190081.0";
const DEVFEE: f64 = 0.85;

struct Consts {
    name: &'static str,
    port: &'static str,
    version: &'static str,
    file_version: &'static str,
    linux_ext: &'static str,
    vendors: &'static [&'static str],
    valid_amd: &'static [&'static str],
    valid_nvidia: &'static [&'static str],
    commands: &'static LazyLock<Vec<Value>>,
}

fn consts(v: V) -> Consts {
    match v {
        V::V239 => Consts {
            name: "SrbMinerMulti239",
            port: "353",
            version: "2.3.9",
            file_version: "2-3-9",
            linux_ext: "tar.xz",
            vendors: &["AMD", "NVIDIA"],
            valid_amd: &["RDNA3", "RDNA2", "RDNA1", "GCN51", "GCN50", "GCN4"],
            valid_nvidia: &["Pascal", "Turing", "Ampere", "Ada", "Hopper"],
            commands: &CMD_239,
        },
        V::V244 => Consts {
            name: "SrbMinerMulti244",
            port: "334",
            version: "2.4.4",
            file_version: "2-4-4",
            linux_ext: "tar.xz",
            vendors: &["AMD", "CPU", "NVIDIA"],
            valid_amd: &["RDNA3", "RDNA2", "RDNA1", "GCN51", "GCN50", "GCN4"],
            valid_nvidia: &["Pascal", "Turing", "Ampere", "Ada", "Hopper"],
            commands: &CMD_244,
        },
        V::V268 => Consts {
            name: "SrbMinerMulti268",
            port: "377",
            version: "2.6.8",
            file_version: "2-6-8",
            linux_ext: "tar.gz",
            vendors: &["AMD", "CPU", "INTEL", "NVIDIA"],
            valid_amd: &["RDNA3", "RDNA2", "RDNA1", "GCN51", "GCN50"],
            valid_nvidia: &["Pascal", "Turing", "Ampere", "Ada", "Hopper", "Blackwell"],
            commands: &CMD_268,
        },
        V::V292 => Consts {
            name: "SrbMinerMulti292",
            port: "349",
            version: "2.9.2",
            file_version: "2-9-2",
            linux_ext: "tar.gz",
            vendors: &["AMD", "CPU", "INTEL", "NVIDIA"],
            valid_amd: &["RDNA4", "RDNA3", "RDNA2", "RDNA1", "GCN51", "GCN50"],
            valid_nvidia: &["Pascal", "Turing", "Ampere", "Ada", "Hopper", "Blackwell"],
            commands: &CMD_292,
        },
    }
}

fn uri(c: &Consts) -> String {
    format!(
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v{}-srbminermulti/SRBMiner-Multi-{}-{}",
        c.version,
        c.file_version,
        if cfg!(windows) { "win64.zip".to_string() } else { format!("Linux.{}", c.linux_ext) }
    )
}

fn esc_pass(p: &str) -> String {
    p.replace(';', "#;").replace('!', "#!")
}

fn esm(p: &Pool) -> &'static str {
    match p.EthMode.as_str() {
        "ethproxy" => " --esm 0",
        "minerproxy" => " --esm 1",
        "ethstratum" | "ethstratum1" | "ethstratumnh" => " --esm 2",
        _ => "",
    }
}

fn port_of(p: &Pool, cpu: bool) -> u32 {
    if cpu {
        p.port_cpu()
    } else {
        p.port_gpu()
    }
}

/// verthash.dat for 2.9.2 unless the miner keeps one in its Cache
fn verthash_dat(base: &std::path::Path) -> Option<PathBuf> {
    if base.join("Bin/ANY-SRBMinerMulti/Cache/verthash.dat").exists() {
        return None;
    }
    let home = if cfg!(windows) {
        std::env::var("APPDATA").map(|a| PathBuf::from(a).join("Vertcoin").join("verthash.dat")).ok()
    } else {
        std::env::var("HOME").map(|h| PathBuf::from(h).join(".vertcoin").join("verthash.dat")).ok()
    };
    match home {
        Some(h) if std::fs::metadata(&h).map(|m| m.len() as f64 >= 1.19 * 1073741824.0).unwrap_or(false) => Some(h),
        _ => Some(base.join("Bin").join("Common").join("verthash.dat")),
    }
}

impl MinerModule for SrbMinerMultiOld {
    fn name(&self) -> &'static str {
        consts(self.v).name
    }
    fn types(&self) -> Vec<String> {
        consts(self.v).vendors.iter().map(|s| s.to_string()).collect()
    }
    fn info(&self, ctx: &MinerCtx) -> Option<MinerInfo> {
        if !(ctx.is_linux() || ctx.is_windows()) || (ctx.is_linux() && ctx.is_arm()) {
            return None;
        }
        let c = consts(self.v);
        Some(MinerInfo {
            name: c.name.into(),
            types: self.types(),
            path: ctx.path(&format!("Bin/ANY-SRBMinerMulti{}/SRBMiner-MULTI", &c.name[13..])),
            uri: uri(&c),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: Value::Array((*c.commands).clone()),
        })
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        let v = self.v;
        let c = consts(v);
        let name = c.name;
        if !(ctx.is_linux() || ctx.is_windows()) || (ctx.is_linux() && ctx.is_arm()) || !c.vendors.iter().any(|x| ctx.has_type(x)) {
            return out;
        }
        let cuda = !ctx.has_type("NVIDIA") || ctx.cuda_ok("11.7", name);
        let path = ctx.path(&format!("Bin/ANY-SRBMinerMulti{}/SRBMiner-MULTI", &name[13..]));
        let uri = uri(&c);
        let verthash = if v == V::V292 { verthash_dat(ctx.base) } else { None };
        let nh_re = if v == V::V239 { "NiceHash" } else { "Gteh|NiceHash" };
        let nicehash = |p: &Pool| if ps_match(&p.Host, nh_re) { "true" } else { "false" };
        let tls = |p: &Pool| if p.SSL { "true" } else { "false" };
        let keepalive = if v <= V::V244 { " --keepalive" } else { "" };
        for vendor in c.vendors.iter().copied() {
            if vendor == "NVIDIA" && !cuda {
                continue;
            }
            let is_cpu = vendor == "CPU";
            for model in ctx.models(vendor) {
                let device: Vec<&Device> = ctx
                    .devices_of(vendor, &model)
                    .into_iter()
                    .filter(|d| {
                        // up to 2.4.4 the valid computes filter the devices of the model
                        v >= V::V268
                            || match vendor {
                                "AMD" => c.valid_amd.contains(&d.OpenCL.DeviceCapability.as_deref().unwrap_or("")),
                                "NVIDIA" => c.valid_nvidia.contains(&d.OpenCL.Architecture.as_str()),
                                _ => true,
                            }
                    })
                    .collect();
                if device.is_empty() {
                    continue;
                }
                let rig = |p: &Pool| ctx.config.pool(&p.Name).map(|x| x.worker.clone()).unwrap_or_default();
                for cmd in c.commands.iter().filter(|x| cmd_vendor(x, vendor)) {
                    let mut first = true;
                    let main_algorithm = cmd_s(cmd, "MainAlgorithm");
                    let second_algorithm = cmd_s(cmd, "SecondaryAlgorithm");
                    let algo0 = data::get_algorithm(&main_algorithm);
                    let algo2_0 = if second_algorithm.is_empty() { None } else { Some(data::get_algorithm(&second_algorithm)) };
                    let exclude_compute = cmd_list(cmd, "ExcludeCompute");
                    let cpu_features = cmd_list(cmd, "CPUFeatures");
                    let (compute, compute_nv): (Vec<String>, bool) = if v >= V::V268 {
                        match vendor {
                            "AMD" => (c.valid_amd.iter().map(|s| s.to_string()).filter(|x| !util::contains_ci(&exclude_compute, x)).collect(), false),
                            "NVIDIA" => (c.valid_nvidia.iter().map(|s| s.to_string()).filter(|x| !util::contains_ci(&exclude_compute, x)).collect(), true),
                            _ => (vec![], false),
                        }
                    } else {
                        (vec![], false)
                    };
                    let mut device_params = match vendor {
                        "CPU" => {
                            let threads = ctx.cpu_threads(name, &algo0);
                            let affinity = ctx.cpu_affinity(name, &algo0);
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
                        "AMD" => " --disable-cpu --disable-gpu-nvidia --disable-gpu-intel".to_string(),
                        "INTEL" => " --disable-cpu --disable-gpu-amd --disable-gpu-nvidia".to_string(),
                        _ => " --disable-cpu --disable-gpu-amd --disable-gpu-intel".to_string(),
                    };
                    if v >= V::V268 && cmd_b(cmd, "Rx1GbPages") && ctx.config.b("EnableRandomX1GBPages") {
                        device_params.push_str(" --randomx-use-1gb-pages");
                    }
                    if main_algorithm == "verthash" {
                        if let Some(dat) = &verthash {
                            device_params = format!(" --verthash-dat-path \"{}\"{}", dat.display(), device_params);
                        }
                    }
                    let exclude = cmd_s(cmd, "ExcludePoolName");
                    let coin_symbols = cmd_list(cmd, "CoinSymbols");
                    let exclude_yiimp = cmd_b(cmd, "ExcludeYiimp");
                    let keys = ctx.algo_keys(name, &algo0, &model, KeyOpts { no_gpu: is_cpu, exclude_pool_name: &exclude, coin_symbols: &coin_symbols, exclude_yiimp, ..Default::default() });
                    let yiimp_ok = |k0: &str| !exclude_yiimp || ctx.pool(k0).map(|p| !ctx.yiimp_pools.contains(&p.Name)).unwrap_or(true);
                    let algorithm_main = {
                        let a = cmd_s(cmd, "Algorithm");
                        if a.is_empty() {
                            main_algorithm.clone()
                        } else {
                            a
                        }
                    };
                    let (mut miner_port, mut mname, mut ids, mut mallob, mut miner_hr) = (0u16, String::new(), String::new(), String::new(), None);
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
                                    v < V::V268 || cpu_features.is_empty() || cpu_features.iter().all(|f| ctx.cpu.has(f))
                                } else {
                                    let comp = if compute_nv { d.OpenCL.Architecture.clone() } else { d.OpenCL.DeviceCapability.clone().unwrap_or_default() };
                                    (min_mem == 0.0 || ctx.test_vram(d, min_mem)) && (compute.is_empty() || util::contains_ci(&compute, &comp))
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
                            miner_port = port_fmt(c.port, miner_device[0].Index);
                            mname = miner_name(name, algo2_0.as_ref().map(|a2| format!("{}-{}", algo0, a2)).as_deref(), &miner_device);
                            ids = device_ids(&miner_device, "BusId_Type_Vendor_Index", ",", false);
                            mallob = opt(!pool.Mallob.is_empty(), format!(" --mallob-endpoint {}", pool.Mallob));
                            miner_hr = ctx.hashrate(&mname, &algo0);
                            if cmd_f(cmd, "MaxRejectedShareRatio").unwrap_or(0.0) != 0.0 {
                                miner_hr = miner_hr.map(|h| h * (1.0 - ctx.ratio_live(&mname, &algo0)));
                            }
                            first = false;
                        }
                        let p1 = port_of(pool, is_cpu);
                        let ext = cmd_f(cmd, "ExtendInterval").map(|x| x as i64).filter(|x| *x != 0).unwrap_or(if is_cpu {
                            2
                        } else if v >= V::V268 {
                            3
                        } else {
                            0
                        });
                        let env = if vendor == "AMD" && ctx.is_linux() { vec!["GPU_MAX_WORKGROUP_SIZE=1024".to_string()] } else { vec![] };
                        let fee = if v >= V::V268 { cmd_f(cmd, "Fee").unwrap_or(DEVFEE) } else { cmd_f(cmd, "Fee").unwrap_or(0.0) };
                        let finish = |m: &mut MinerCandidate| {
                            m.uri = uri.clone();
                            m.extend_interval = ext;
                            m.max_rejected_share_ratio = cmd_f(cmd, "MaxRejectedShareRatio").filter(|x| *x != 0.0);
                            m.manual_uri = MANUAL_URI.into();
                            m.env_vars = env.clone();
                            m.version = c.version.into();
                            m.set_ld_library_path = Some(false);
                            m.list_devices = "--list-devices".into();
                            m.exclude_pool_name = exclude.clone();
                        };
                        let rig_name = if v <= V::V268 { format!(" --api-rig-name {}", rig(pool)) } else { String::new() };
                        if let Some(algo2_0) = &algo2_0 {
                            let mut intens = ctx.intensities(name, &model, &algo0, algo2_0);
                            if intens.is_empty() {
                                intens = vec!["0".into()];
                            }
                            let skeys = ctx.second_keys(algo2_0, &model, !is_cpu);
                            for intensity in &intens {
                                let iv = util::parse_f64(intensity).unwrap_or(0.0);
                                let name_dual = if iv > 0.0 { miner_name(name, Some(&format!("{}-{}-{}", algo0, algo2_0, intensity)), &miner_device) } else { mname.clone() };
                                for key2 in &skeys {
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
                                    let pools = format!(
                                        "--pool {}:{}{} --pool {}:{} --wallet {} --wallet {}{} --tls {} --tls {} --nicehash {} --nicehash {}{}",
                                        pool.Host,
                                        p1,
                                        if v >= V::V268 { esm(pool) } else { "" },
                                        pool2.Host,
                                        port_of(pool2, is_cpu),
                                        pool.User,
                                        pool2.User,
                                        passwords,
                                        tls(pool),
                                        tls(pool2),
                                        nicehash(pool),
                                        nicehash(pool2),
                                        mallob
                                    );
                                    let autotune = opt(v <= V::V244 && vendor != "NVIDIA", " --gpu-auto-tune 1".into());
                                    let algos = format!("{};{}", algorithm_main, second_algorithm);
                                    let args = match v {
                                        V::V239 => format!(
                                            "--api-enable --api-port $mport{}{} --gpu-id {}{}{} --gpu-dual-max-loss {} --algorithm {} {}{} --retry-time 10 --disable-startup-monitor {}",
                                            rig_name,
                                            esm(pool),
                                            ids,
                                            device_params,
                                            autotune,
                                            intensity,
                                            algos,
                                            pools,
                                            keepalive,
                                            cmd_s(cmd, "Params")
                                        ),
                                        V::V244 => format!(
                                            "--api-enable --api-port $mport{}{} --gpu-id {}{} --gpu-dual-max-loss {} --algorithm {} {}{} --retry-time 10 --disable-startup-monitor{} {}",
                                            rig_name,
                                            esm(pool),
                                            ids,
                                            autotune,
                                            intensity,
                                            algos,
                                            pools,
                                            keepalive,
                                            device_params,
                                            cmd_s(cmd, "Params")
                                        ),
                                        _ => format!(
                                            "--gpu-id {} --gpu-dual-max-loss {} --algorithm {} {} --retry-time 10 --disable-startup-monitor{} --api-enable --api-port $mport{} {}",
                                            ids,
                                            intensity,
                                            algos,
                                            pools,
                                            device_params,
                                            rig_name,
                                            cmd_s(cmd, "Params")
                                        ),
                                    };
                                    let mut m = MinerCandidate::new(name, &name_dual, &miner_device, &model, path.clone(), "SrbMinerMulti", miner_port)
                                        .with_algo(&key, ctx.hashrate(&name_dual, &algo0), fee)
                                        .with_algo(key2, ctx.hashrate(&name_dual, algo2_0), 0.0);
                                    m.arguments = MinerArgs::Str(args);
                                    m.base_algorithm = format!("{}-{}", algo0, algo2_0);
                                    m.fault_tolerance = Some(0.7);
                                    m.show_miner_window = true;
                                    finish(&mut m);
                                    out.push(m);
                                }
                            }
                        } else {
                            let passwords = opt(!pool.Pass.is_empty(), format!(" --password {}", esc_pass(&pool.Pass)));
                            let gpu_id = opt(!is_cpu, format!("--gpu-id {}", ids));
                            let args = match v {
                                V::V239 => format!(
                                    "--algorithm {} --api-enable --api-port $mport{}{} {}{} --pool {}:{} --wallet {}{} --tls {} --nicehash {}{}{} --retry-time 10 --disable-startup-monitor {}",
                                    algorithm_main,
                                    rig_name,
                                    esm(pool),
                                    gpu_id,
                                    device_params,
                                    pool.Host,
                                    p1,
                                    pool.User,
                                    passwords,
                                    tls(pool),
                                    nicehash(pool),
                                    mallob,
                                    keepalive,
                                    cmd_s(cmd, "Params")
                                ),
                                V::V244 => format!(
                                    "--algorithm {} --api-enable --api-port $mport{}{} {} --pool {}:{} --wallet {}{} --tls {} --nicehash {}{}{} --retry-time 10 --disable-startup-monitor{} {}",
                                    algorithm_main,
                                    rig_name,
                                    esm(pool),
                                    gpu_id,
                                    pool.Host,
                                    p1,
                                    pool.User,
                                    passwords,
                                    tls(pool),
                                    nicehash(pool),
                                    mallob,
                                    keepalive,
                                    device_params,
                                    cmd_s(cmd, "Params")
                                ),
                                _ => format!(
                                    "--algorithm {} {} --pool {}:{}{} --wallet {}{} --tls {} --nicehash {}{} --retry-time 10 --disable-startup-monitor{} --api-enable --api-port $mport{} {}",
                                    algorithm_main,
                                    gpu_id,
                                    pool.Host,
                                    p1,
                                    esm(pool),
                                    pool.User,
                                    passwords,
                                    tls(pool),
                                    nicehash(pool),
                                    mallob,
                                    device_params,
                                    rig_name,
                                    cmd_s(cmd, "Params")
                                ),
                            };
                            let mut m = MinerCandidate::new(name, &mname, &miner_device, &model, path.clone(), "SrbMinerMulti", miner_port).with_algo(&key, miner_hr, fee);
                            m.arguments = MinerArgs::Str(args);
                            m.base_algorithm = algo0.clone();
                            m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                            finish(&mut m);
                            out.push(m);
                        }
                    }
                }
            }
        }
        out
    }
}

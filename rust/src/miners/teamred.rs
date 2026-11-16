//! Miners/Teamred.ps1 (TeamRedMiner, AMD, Xgminer API, dual mining with a second API port)

use super::opt;
use crate::data;
use crate::device::Device;
use crate::jsonx;
use crate::miner::*;
use crate::pool::Pool;
use crate::util;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::LazyLock;

pub struct Teamred;

const NAME: &str = "Teamred";
const PORT: &str = "409";
const DEVFEE: f64 = 3.0;
const VERSION: &str = "0.10.21";
const MANUAL_URI: &str = "https://bitcointalk.org/index.php?topic=5059817.0";

static COMMANDS: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Teamred.json")));

fn path_uri(ctx: &MinerCtx) -> (PathBuf, &'static str) {
    if ctx.is_linux() {
        (ctx.path("Bin/AMD-Teamred/teamredminer"), "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.10.21-teamred/teamredminer-v0.10.21-linux.tgz")
    } else {
        (ctx.path("Bin/AMD-Teamred/teamredminer.exe"), "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.10.21-teamred/teamredminer-v0.10.21-win.zip")
    }
}

/// the verthash.dat of the Vertcoin wallet, else Bin\Common\verthash.dat
fn verthash_dat(ctx: &MinerCtx) -> PathBuf {
    let home = if cfg!(windows) {
        std::env::var("APPDATA").map(|a| PathBuf::from(a).join("Vertcoin").join("verthash.dat")).ok()
    } else {
        std::env::var("HOME").map(|h| PathBuf::from(h).join(".vertcoin").join("verthash.dat")).ok()
    };
    if let Some(h) = home {
        if std::fs::metadata(&h).map(|m| m.len() as f64 >= 1.19 * 1073741824.0).unwrap_or(false) {
            return h;
        }
    }
    ctx.base.join("Bin").join("Common").join("verthash.dat")
}

/// host:port, or host:port/path
fn pool_host(p: &Pool) -> String {
    let port = p.port_gpu();
    if port == 0 {
        return p.Host.clone();
    }
    match p.Host.split_once('/') {
        Some((h, path)) if !path.is_empty() => format!("{}:{}/{}", h, port, path),
        _ => format!("{}:{}", p.Host, port),
    }
}

fn protocol(p: &Pool) -> String {
    if p.Protocol == "wss" {
        "stratum+tcp".into()
    } else {
        p.Protocol.clone()
    }
}

fn capability(d: &Device) -> String {
    d.OpenCL.DeviceCapability.clone().unwrap_or_default()
}

impl MinerModule for Teamred {
    fn name(&self) -> &'static str {
        NAME
    }
    fn types(&self) -> Vec<String> {
        vec!["AMD".into()]
    }
    fn info(&self, ctx: &MinerCtx) -> Option<MinerInfo> {
        if !(ctx.is_linux() || ctx.is_windows()) || (ctx.is_linux() && ctx.is_arm()) {
            return None;
        }
        let (path, uri) = path_uri(ctx);
        Some(MinerInfo {
            name: NAME.into(),
            types: self.types(),
            path,
            uri: uri.into(),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: Value::Array(COMMANDS.clone()),
        })
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        if !(ctx.is_linux() || ctx.is_windows()) || (ctx.is_linux() && ctx.is_arm()) || !ctx.has_type("AMD") {
            return out;
        }
        let (path, uri) = path_uri(ctx);
        let dat = verthash_dat(ctx);
        let zombie = ctx.config.b("EnableEthashZombieMode");
        for model in ctx.models("AMD") {
            let device: Vec<&Device> = ctx.devices_of("AMD", &model);
            if device.is_empty() {
                continue;
            }
            for cmd in COMMANDS.iter() {
                let mut first = true;
                let main_raw = cmd_s(cmd, "MainAlgorithm");
                let main_x = {
                    let x = cmd_s(cmd, "MainAlgorithmXlat");
                    if x.is_empty() {
                        main_raw.clone()
                    } else {
                        x
                    }
                };
                let second_raw = cmd_s(cmd, "SecondAlgorithm");
                let second_x = {
                    let x = cmd_s(cmd, "SecondAlgorithmXlat");
                    if x.is_empty() {
                        second_raw.clone()
                    } else {
                        x
                    }
                };
                let algo0 = data::get_algorithm(&main_x);
                let algo2_0 = if second_x.is_empty() { None } else { Some(data::get_algorithm(&second_x)) };
                let excl_compute = cmd_list(cmd, "ExcludeCompute");
                let compute = cmd_list(cmd, "Compute");
                let exclude = cmd_s(cmd, "ExcludePoolName");
                let pool_name = cmd_s(cmd, "PoolName");
                let (mut miner_port, mut mname, mut ids, mut ids_dual) = (0u16, String::new(), String::new(), String::new());
                for key in ctx.algo_keys(NAME, &algo0, &model, KeyOpts { exclude_pool_name: &exclude, pool_name: &pool_name, ..Default::default() }) {
                    let Some(pool) = ctx.pool(&key) else { continue };
                    if pool.Host.is_empty() {
                        continue;
                    }
                    let mut min_mem = ctx.min_mem_gb(cmd, pool, &algo0);
                    let cmd_min = jsonx::num(jsonx::get(cmd, "MinMemGB"));
                    if cmd_b(cmd, "DAG") && data::RE_ALGO_IS_ETHASH.is_match(&algo0) && min_mem > cmd_min && zombie {
                        min_mem = cmd_min;
                    }
                    let mut miner_device: Vec<&Device> = device
                        .iter()
                        .copied()
                        .filter(|d| {
                            let cap = capability(d);
                            ctx.test_vram(d, min_mem) && (excl_compute.is_empty() || !util::contains_ci(&excl_compute, &cap)) && (compute.is_empty() || util::contains_ci(&compute, &cap))
                        })
                        .collect();
                    let device_dual: Vec<&Device> = miner_device.iter().copied().filter(|d| compute.is_empty() || util::contains_ci(&compute, &capability(d))).collect();
                    let mut devfee = cmd_f(cmd, "DevFee").unwrap_or(0.0);
                    if algo2_0.is_some() && !compute.is_empty() && device_dual.is_empty() {
                        miner_device.clear();
                    } else if ps_match(&main_raw, "^ethash") && model.split('-').any(|m| !ps_match(m, r"(Baffin|Ellesmere|RX\d)")) {
                        devfee = 1.0;
                    } else if main_raw == "abel" {
                        let caps: Vec<String> = miner_device.iter().map(|d| capability(d)).collect();
                        if caps.iter().any(|c| c == "GCN50" || c == "RDNA1") {
                            devfee = 2.0;
                        } else if caps.iter().any(|c| c == "GCN51" || c == "CDNA1") {
                            devfee = 3.0;
                        }
                    }
                    if miner_device.is_empty() || (!exclude.is_empty() && ps_match(&pool.Host, &exclude)) || (!pool_name.is_empty() && !ps_match(&pool.Host, &pool_name)) {
                        continue;
                    }
                    if first {
                        miner_port = port_fmt(PORT, 2 * miner_device[0].Index);
                        mname = miner_name(NAME, algo2_0.as_ref().map(|a2| format!("{}-{}", algo0, a2)).as_deref(), &miner_device);
                        ids = device_ids(&miner_device, "BusId_Type_Vendor_Index", ",", false);
                        if algo2_0.is_some() {
                            ids_dual = device_ids(&device_dual, "BusId_Type_Vendor_Index", ",", false);
                        }
                        first = false;
                    }
                    let mut user = pool.User.clone();
                    let mut proto = protocol(pool);
                    let is_verthash = algo0 == "Verthash";
                    let mut extra = vec!["--watchdog_disabled".to_string()];
                    if ps_match(&pool.Host, "bsod") && algo0 == "x16rt" {
                        extra.push("--no_ntime_roll".into());
                    }
                    if ctx.is_linux() && ps_match(&algo0, "^cn") {
                        extra.push("--allow_large_alloc".into());
                    }
                    if main_raw == "nimiq" {
                        user = pool.Wallet.clone();
                        proto = "stratum+tcp".into();
                        extra.push(format!("--nimiq_worker={}", pool.Worker));
                    } else if is_verthash {
                        extra.push(format!("--verthash_file='{}'", dat.display()));
                    }
                    let pass = opt(!pool.Pass.is_empty(), format!(" -p {}", pool.Pass));
                    let finish = |m: &mut MinerCandidate| {
                        m.uri = uri.into();
                        m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                        m.extend_interval = cmd_i(cmd, "ExtendInterval");
                        m.manual_uri = MANUAL_URI.into();
                        m.version = VERSION.into();
                        m.exclude_pool_name = exclude.clone();
                        if is_verthash {
                            m.prerequisite_path = Some(dat.clone());
                            m.prerequisite_uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0-verthash/verthash.dat".into();
                            m.prerequisite_msg = "Downloading verthash.dat (1.2GB) in the background, please wait!".into();
                        }
                    };
                    if let Some(algo2_0) = &algo2_0 {
                        let mut intens = ctx.intensities(NAME, &model, &algo0, algo2_0);
                        if intens.is_empty() {
                            intens = vec!["0".into()];
                        }
                        for intensity in intens {
                            let iv = util::parse_f64(&intensity).unwrap_or(0.0);
                            let (name_dual, dual_int) = if iv > 0.0 {
                                (
                                    miner_name(NAME, Some(&format!("{}-{}-{}", algo0, algo2_0, (iv * 100.0).round() as i64)), &miner_device),
                                    vec![format!("{}", iv); miner_device.len()].join(","),
                                )
                            } else {
                                (mname.clone(), String::new())
                            };
                            for key2 in ctx.second_keys(algo2_0, &model, true) {
                                let Some(pool2) = ctx.pool(&key2) else { continue };
                                let ex2 = cmd_s(cmd, "SecondExcludePoolName");
                                let pn2 = cmd_s(cmd, "SecondPoolName");
                                if pool2.Host.is_empty() || (!ex2.is_empty() && ps_match(&pool2.Host, &ex2)) || (!pn2.is_empty() && !ps_match(&pool2.Host, &pn2)) {
                                    continue;
                                }
                                let ton_mode = if algo2_0 == "SHA256ton" && !pool2.EthMode.is_empty() { pool2.EthMode.clone() } else { String::new() };
                                let mut proto2 = protocol(pool2);
                                if proto2.is_empty() {
                                    proto2 = format!("stratum+{}", if pool2.SSL { "ssl" } else { "tcp" });
                                }
                                let second_args = format!(
                                    "--{} -d {} -o {}://{} -u {}.{}{}{} --api2_listen=$mport2 --{}_end",
                                    second_raw,
                                    ids_dual,
                                    proto2,
                                    pool_host(pool2),
                                    pool2.Wallet,
                                    pool2.Worker,
                                    opt(!pool2.Pass.is_empty(), format!(" -p {}", pool2.Pass)),
                                    opt(!ton_mode.is_empty(), format!(" --ton_pool_mode={}", ton_mode)),
                                    second_raw
                                );
                                let args = format!(
                                    "-a {} -d {} -o {}://{} -u {}{} {}{} --api_listen=$mport --bus_reorder {} {}",
                                    main_raw,
                                    ids,
                                    proto,
                                    pool_host(pool),
                                    user,
                                    pass,
                                    second_args,
                                    opt(!dual_int.is_empty(), format!("  --dual_intensity={}", dual_int)),
                                    extra.join(" "),
                                    cmd_s(cmd, "Params")
                                );
                                let mut m = MinerCandidate::new(NAME, &name_dual, &miner_device, &model, path.clone(), "Xgminer", miner_port)
                                    .with_algo(&key, ctx.hashrate(&name_dual, &algo0), devfee)
                                    .with_algo(&key2, ctx.hashrate(&name_dual, algo2_0), 0.0);
                                m.arguments = MinerArgs::Str(args);
                                m.base_algorithm = format!("{}-{}", algo0, algo2_0);
                                finish(&mut m);
                                out.push(m);
                            }
                        }
                    } else {
                        let ton_mode = if algo0 == "SHA256ton" && !pool.EthMode.is_empty() { pool.EthMode.clone() } else { String::new() };
                        let args = format!(
                            "-a {} -d {} -o {}://{} -u {}{}{} --api_listen=$mport --bus_reorder {} {}",
                            main_raw,
                            ids,
                            proto,
                            pool_host(pool),
                            user,
                            pass,
                            opt(!ton_mode.is_empty(), format!(" --ton_pool_mode={}", ton_mode)),
                            extra.join(" "),
                            cmd_s(cmd, "Params")
                        );
                        let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, path.clone(), "Xgminer", miner_port).with_algo(&key, ctx.hashrate(&mname, &algo0), devfee);
                        m.arguments = MinerArgs::Str(args);
                        m.base_algorithm = algo0.clone();
                        m.list_devices = "--list_devices".into();
                        finish(&mut m);
                        out.push(m);
                    }
                }
            }
        }
        out
    }
}

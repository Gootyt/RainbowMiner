//! Miners/Gminer.ps1 (AMD, NVIDIA)

use super::{host_port, opt};
use crate::data;
use crate::device::Device;
use crate::miner::*;
use crate::pool::Pool;
use crate::util;
use std::sync::LazyLock;

pub struct Gminer;

const NAME: &str = "Gminer";
const PORT: &str = "329";
const DEVFEE: f64 = 2.0;
const CUDA: &str = "9.0";
const VERSION: &str = "3.43";
const DEVICE_CAPABILITY: &str = "5.0";
const MANUAL_URI: &str = "https://github.com/develsoftware/GMinerRelease/releases";

static COMMANDS: LazyLock<Vec<serde_json::Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Gminer.json")));

fn uri() -> &'static str {
    if cfg!(windows) {
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.43-gminer/gminer_3_43_windows64.zip"
    } else {
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.43-gminer/gminer_3_43_linux64.tar.xz"
    }
}

fn coin_ok(cmd: &serde_json::Value, p: &Pool) -> bool {
    let cs = cmd_list(cmd, "CoinSymbol");
    let ex = cmd_list(cmd, "ExcludeCoinSymbol");
    (cs.is_empty() || util::contains_ci(&cs, &p.CoinSymbol)) && (ex.is_empty() || !util::contains_ci(&ex, &p.CoinSymbol))
}

impl MinerModule for Gminer {
    fn name(&self) -> &'static str {
        NAME
    }
    fn types(&self) -> Vec<String> {
        vec!["AMD".into(), "NVIDIA".into()]
    }
    fn info(&self, ctx: &MinerCtx) -> Option<MinerInfo> {
        Some(MinerInfo {
            name: NAME.into(),
            types: self.types(),
            path: ctx.path("Bin/GPU-Gminer/miner"),
            uri: uri().into(),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: serde_json::Value::Array(COMMANDS.clone()),
        })
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        if !(ctx.is_linux() || ctx.is_windows()) || (ctx.is_linux() && ctx.is_arm()) || !(ctx.has_type("AMD") || ctx.has_type("NVIDIA")) {
            return out;
        }
        let cuda = !ctx.has_type("NVIDIA") || ctx.cuda_ok(CUDA, NAME);
        let path = ctx.path("Bin/GPU-Gminer/miner");
        let version = util::get_miner_version(VERSION);
        for vendor in ["AMD", "NVIDIA"] {
            if vendor == "NVIDIA" && !cuda {
                continue;
            }
            for model in ctx.models(vendor) {
                let device: Vec<&Device> = ctx
                    .devices_of(vendor, &model)
                    .into_iter()
                    .filter(|d| d.is_gpu() && (vendor != "NVIDIA" || capability_ok(d, DEVICE_CAPABILITY)))
                    .collect();
                if device.is_empty() {
                    continue;
                }
                let vendor_params = if vendor == "NVIDIA" { " --nvml 1" } else { " --nvml 0" };
                for cmd in COMMANDS.iter().filter(|c| cmd_vendor(c, vendor)) {
                    let cv = cmd_s(cmd, "Version");
                    if !cv.is_empty() && util::get_miner_version(&cv) > version {
                        continue;
                    }
                    let mut first = true;
                    let algo0 = data::get_algorithm(&cmd_s(cmd, "MainAlgorithm"));
                    let algo2_raw = cmd_s(cmd, "SecondaryAlgorithm");
                    let algo2_0 = if algo2_raw.is_empty() { None } else { Some(data::get_algorithm(&algo2_raw)) };
                    let has_ethproxy = data::RE_ALGO_HAS_ETHPROXY.is_match(&algo0);
                    let exclude = cmd_s(cmd, "ExcludePoolName");
                    let (mut miner_port, mut mname, mut ids) = (0u16, String::new(), String::new());
                    for key in ctx.algo_keys(NAME, &algo0, &model, KeyOpts { exclude_pool_name: &exclude, ..Default::default() }) {
                        let Some(pool) = ctx.pool(&key) else { continue };
                        if pool.Host.is_empty() {
                            continue;
                        }
                        let min_mem = ctx.min_mem_gb(cmd, pool, &algo0);
                        let miner_device: Vec<&Device> = device.iter().copied().filter(|d| ctx.test_vram(d, min_mem)).collect();
                        if miner_device.is_empty() || (!exclude.is_empty() && ps_match(&pool.Host, &exclude)) || !coin_ok(cmd, pool) {
                            continue;
                        }
                        if first {
                            miner_port = port_fmt(PORT, miner_device[0].Index);
                            mname = miner_name(NAME, algo2_0.as_ref().map(|a2| format!("{}-{}", algo0, a2)).as_deref(), &miner_device);
                            ids = device_ids(&miner_device, "BusId_Type_Vendor_Index", " ", false);
                            first = false;
                        }
                        let pers = if key.to_lowercase().starts_with("equihash") {
                            data::get_equihash_coin_pers(&pool.CoinSymbol, "auto")
                        } else {
                            String::new()
                        };
                        let pers_arg = opt(!pers.is_empty() && (cmd_b(cmd, "AutoPers") || pers != "auto"), format!(" --pers {}", pers));
                        let proto_arg = opt(
                            has_ethproxy && !pool.EthMode.is_empty() && pool.EthMode != "ethproxy" && pool.EthMode != "qtminer",
                            " --proto stratum".into(),
                        );
                        let fee = cmd_f(cmd, "Fee").unwrap_or(DEVFEE);
                        let cuda_opencl = format!(" --cuda {} --opencl {}", (vendor == "NVIDIA") as i32, (vendor == "AMD") as i32);
                        if let Some(algo2_0) = &algo2_0 {
                            let mut intens = ctx.intensities(NAME, &model, &algo0, algo2_0);
                            if intens.is_empty() {
                                intens = vec!["0".into()];
                            }
                            for intensity in intens {
                                let iv = util::parse_f64(&intensity).unwrap_or(0.0);
                                let (name_dual, dual_int) = if iv > 0.0 {
                                    (
                                        miner_name(NAME, Some(&format!("{}-{}-{}", algo0, algo2_0, intensity)), &miner_device),
                                        format!(" --dual_intensity{}", format!(" {}", intensity).repeat(miner_device.len())),
                                    )
                                } else {
                                    (mname.clone(), String::new())
                                };
                                for key2 in ctx.second_keys(algo2_0, &model, true) {
                                    let Some(pool2) = ctx.pool(&key2) else { continue };
                                    if pool2.Host.is_empty() || pool2.User.is_empty() || (!exclude.is_empty() && ps_match(&pool2.Host, &exclude)) || !coin_ok(cmd, pool2) {
                                        continue;
                                    }
                                    let args = format!(
                                        "--api $mport --devices {}{} --server {}{} --user {}{}{}{}{}{} --dserver {}{} --duser {}{}{}{} --watchdog 0 --pec 0{} {}",
                                        ids,
                                        dual_int,
                                        host_port(pool, pool.port_gpu()),
                                        proto_arg,
                                        pool.User,
                                        opt(!pool.Pass.is_empty(), format!(" --pass {}", pool.Pass)),
                                        opt(!pool.Worker.is_empty(), format!(" --worker {}", pool.Worker)),
                                        pers_arg,
                                        if pool.SSL { " --ssl 1 --ssl_verification 0" } else { " --ssl 0" },
                                        cuda_opencl,
                                        opt(algo2_0 == "SHA256ton" && !pool2.Protocol.is_empty(), format!("{}://", pool2.Protocol)),
                                        host_port(pool2, pool2.port_gpu()),
                                        pool2.User,
                                        opt(!pool2.Pass.is_empty(), format!(" --dpass {}", pool2.Pass)),
                                        opt(!pool2.Worker.is_empty(), format!(" --dworker {}", pool2.Worker)),
                                        opt(algo2_0 != "SHA256ton", if pool2.SSL { " --dssl 1".into() } else { " --dssl 0".into() }),
                                        vendor_params,
                                        cmd_s(cmd, "Params")
                                    );
                                    let mut m = MinerCandidate::new(NAME, &name_dual, &miner_device, &model, path.clone(), "Gminer", miner_port)
                                        .with_algo(&key, penalized(ctx.hashrate(&name_dual, &algo0), cmd), fee)
                                        .with_algo(&key2, penalized(ctx.hashrate(&name_dual, algo2_0), cmd), 0.0);
                                    m.arguments = MinerArgs::Str(args);
                                    m.uri = uri().into();
                                    m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                                    m.extend_interval = cmd_i(cmd, "ExtendInterval");
                                    m.manual_uri = MANUAL_URI.into();
                                    m.no_cpu_mining = cmd_b(cmd, "NoCPUMining");
                                    m.version = VERSION.into();
                                    m.base_algorithm = format!("{}-{}", algo0, algo2_0);
                                    m.exclude_pool_name = exclude.clone();
                                    out.push(m);
                                }
                            }
                        } else {
                            let args = format!(
                                "--api $mport --devices {} --server {}{}{} --user {}{}{}{}{}{} --watchdog 0 --pec 0{} {}",
                                ids,
                                opt(algo0 == "SHA256ton" && !pool.Protocol.is_empty(), format!("{}://", pool.Protocol)),
                                host_port(pool, pool.port_gpu()),
                                proto_arg,
                                pool.User,
                                opt(!pool.Pass.is_empty(), format!(" --pass {}", pool.Pass)),
                                opt(!pool.Worker.is_empty(), format!(" --worker {}", pool.Worker)),
                                pers_arg,
                                opt(algo0 != "SHA256ton", if pool.SSL { " --ssl 1 --ssl_verification 0".into() } else { " --ssl 0".into() }),
                                cuda_opencl,
                                vendor_params,
                                cmd_s(cmd, "Params")
                            );
                            let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, path.clone(), "Gminer", miner_port).with_algo(&key, penalized(ctx.hashrate(&mname, &algo0), cmd), fee);
                            m.arguments = MinerArgs::Str(args);
                            m.uri = uri().into();
                            m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                            m.extend_interval = cmd_i(cmd, "ExtendInterval");
                            m.manual_uri = MANUAL_URI.into();
                            m.no_cpu_mining = cmd_b(cmd, "NoCPUMining");
                            m.version = VERSION.into();
                            m.base_algorithm = algo0.clone();
                            m.exclude_pool_name = exclude.clone();
                            out.push(m);
                        }
                    }
                }
            }
        }
        out
    }
}

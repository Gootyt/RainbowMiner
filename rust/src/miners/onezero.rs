//! Miners/OneZeroMiner.ps1 (AMD, NVIDIA)

use super::opt;
use crate::data;
use crate::device::Device;
use crate::miner::*;
use crate::pool::Pool;
use crate::util;
use serde_json::Value;
use std::sync::LazyLock;

pub struct OneZeroMiner;

const NAME: &str = "OneZeroMiner";
const PORT: &str = "370";
const DEVFEE: f64 = 3.0;
const CUDA: &str = "11.8";
const VERSION: &str = "1.8.6";
const MANUAL_URI: &str = "https://github.com/OneZeroMiner/onezerominer/releases";

static COMMANDS: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/OneZeroMiner.json")));

fn uri() -> &'static str {
    if cfg!(windows) {
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.8.6-onezerominer/onezerominer-win64-1.8.6.zip"
    } else {
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.8.6-onezerominer/onezerominer-linux-1.8.6.tar.gz"
    }
}

/// $_.Fee.$Miner_Vendor, the fee may be a per vendor object
fn fee_of(cmd: &Value, key: &str, vendor: &str) -> f64 {
    let f = crate::jsonx::get(cmd, key);
    if f.is_null() {
        DEVFEE
    } else if f.is_object() {
        crate::jsonx::num(crate::jsonx::get(f, vendor))
    } else {
        crate::jsonx::num(f)
    }
}

fn url(p: &Pool, port: u32) -> String {
    format!("{}{}:{}", opt(p.SSL, format!("{}://", p.Protocol)), p.Host, port)
}

impl MinerModule for OneZeroMiner {
    fn name(&self) -> &'static str {
        NAME
    }
    fn types(&self) -> Vec<String> {
        vec!["AMD".into(), "NVIDIA".into()]
    }
    fn info(&self, ctx: &MinerCtx) -> MinerInfo {
        MinerInfo {
            name: NAME.into(),
            types: self.types(),
            path: ctx.path("Bin/GPU-OneZero/onezerominer"),
            uri: uri().into(),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: Value::Array(COMMANDS.clone()),
        }
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        if !(ctx.is_linux() || ctx.is_windows()) || (ctx.is_linux() && ctx.is_arm()) || !(ctx.has_type("AMD") || ctx.has_type("NVIDIA")) {
            return out;
        }
        let cuda = !ctx.has_type("NVIDIA") || ctx.cuda_ok(CUDA, NAME);
        let path = ctx.path("Bin/GPU-OneZero/onezerominer");
        let re_user = regex::Regex::new(r"^([^/]+)/([^/]+)$").unwrap();
        for vendor in ["AMD", "NVIDIA"] {
            if vendor == "NVIDIA" && !cuda {
                continue;
            }
            let disable = if vendor == "NVIDIA" { "--disable-amd" } else { "--disable-nvidia" };
            for model in ctx.models(vendor) {
                let device: Vec<&Device> = ctx.devices_of(vendor, &model).into_iter().filter(|d| d.is_gpu()).collect();
                for cmd in COMMANDS.iter().filter(|c| cmd_vendor(c, vendor)) {
                    let mut first = true;
                    let excl_compute = cmd_list(cmd, "ExcludeCompute");
                    let miner_device: Vec<&Device> = device.iter().copied().filter(|d| excl_compute.is_empty() || !util::contains_ci(&excl_compute, &d.OpenCL.Architecture)).collect();
                    let main_raw = cmd_s(cmd, "MainAlgorithm");
                    let algorithm0 = {
                        let a = cmd_s(cmd, "Algorithm");
                        if a.is_empty() {
                            main_raw.clone()
                        } else {
                            a
                        }
                    };
                    let algo0 = data::get_algorithm(&algorithm0);
                    let algo2_raw = cmd_s(cmd, "SecondaryAlgorithm");
                    let algo2_0 = if algo2_raw.is_empty() { None } else { Some(data::get_algorithm(&algo2_raw)) };
                    let exclude = cmd_s(cmd, "ExcludePoolName");
                    let include = cmd_s(cmd, "IncludePoolName");
                    let (mut miner_port, mut mname, mut ids) = (0u16, String::new(), String::new());
                    for key in ctx.algo_keys(NAME, &algo0, &model, KeyOpts { exclude_pool_name: &exclude, ..Default::default() }) {
                        let Some(pool) = ctx.pool(&key) else { continue };
                        if pool.Host.is_empty() || miner_device.is_empty() || (!include.is_empty() && !ps_match(&pool.Host, &include)) {
                            continue;
                        }
                        if first {
                            miner_port = port_fmt(PORT, miner_device[0].Index);
                            mname = miner_name(NAME, algo2_0.as_ref().map(|a2| format!("{}-{}", algo0, a2)).as_deref(), &miner_device);
                            ids = device_ids(&miner_device, "Type_Vendor_Index", ",", false);
                            first = false;
                        }
                        let pool_port = pool.port_gpu();
                        let mut user = pool.User.clone();
                        let mut worker_arg = String::new();
                        if main_raw == "pearlhash" && ps_match(&pool.Host, "kryptex") && re_user.is_match(&pool.User) {
                            let c = re_user.captures(&pool.User).unwrap();
                            user = format!("{}.{}", &c[1], &c[2]);
                            worker_arg = format!(" --worker {}", &c[2]);
                        } else if !pool.Worker.is_empty() && pool.User == pool.Wallet {
                            worker_arg = format!(" --worker {}", pool.Worker);
                        }
                        if let Some(algo2_0) = &algo2_0 {
                            let mut intens = ctx.intensities(NAME, &model, &algo0, algo2_0);
                            if intens.is_empty() {
                                intens = vec![String::new()];
                            }
                            for intensity in intens {
                                let (name_dual, dual_int) = if !intensity.is_empty() {
                                    (
                                        miner_name(NAME, Some(&format!("{}-{}-{}", algo0, algo2_0, intensity)), &miner_device),
                                        vec![intensity.clone(); miner_device.len()].join(","),
                                    )
                                } else {
                                    (mname.clone(), String::new())
                                };
                                for key2 in ctx.second_keys(algo2_0, &model, true) {
                                    let Some(pool2) = ctx.pool(&key2) else { continue };
                                    let cs = cmd_list(cmd, "CoinSymbol");
                                    let ecs = cmd_list(cmd, "ExcludeCoinSymbol");
                                    if pool2.Host.is_empty()
                                        || pool2.User.is_empty()
                                        || (!exclude.is_empty() && ps_match(&pool2.Host, &exclude))
                                        || (!cs.is_empty() && !util::contains_ci(&cs, &pool2.CoinSymbol))
                                        || (!ecs.is_empty() && util::contains_ci(&ecs, &pool2.CoinSymbol))
                                    {
                                        continue;
                                    }
                                    let args = format!(
                                        "{} -d {} --d2 {}{} -a {} -w {}{} -o {}{} --a2 {} --w2 {}{} --o2 {}{}{} --api-port $mport --disable-telemetry{}",
                                        disable,
                                        ids,
                                        ids,
                                        opt(!dual_int.is_empty(), format!(" --dual-intensity {}", dual_int)),
                                        main_raw,
                                        user,
                                        opt(!pool.Pass.is_empty(), format!(" -p {}", pool.Pass)),
                                        url(pool, pool_port),
                                        worker_arg,
                                        algo2_raw,
                                        pool2.User,
                                        opt(!pool2.Pass.is_empty(), format!(" --p2 {}", pool2.Pass)),
                                        url(pool2, pool2.port_gpu()),
                                        opt(!pool2.Worker.is_empty() && pool2.User == pool2.Wallet, format!(" --worker2 {}", pool2.Worker)),
                                        opt((pool.SSL && pool.SSLSelfSigned) || (pool2.SSL && pool2.SSLSelfSigned), " --no-cert-validation".into()),
                                        cmd_s(cmd, "Params")
                                    );
                                    let mut m = MinerCandidate::new(NAME, &name_dual, &miner_device, &model, path.clone(), "OneZeroMiner", miner_port)
                                        .with_algo(&key, ctx.hashrate(&name_dual, &algo0), fee_of(cmd, "Fee", vendor))
                                        .with_algo(&key2, ctx.hashrate(&name_dual, algo2_0), fee_of(cmd, "SecondaryFee", vendor));
                                    m.arguments = MinerArgs::Str(args);
                                    m.uri = uri().into();
                                    m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                                    m.extend_interval = cmd_i(cmd, "ExtendInterval");
                                    m.manual_uri = MANUAL_URI.into();
                                    m.version = VERSION.into();
                                    m.base_algorithm = format!("{}-{}", algo0, algo2_0);
                                    m.exclude_pool_name = exclude.clone();
                                    out.push(m);
                                }
                            }
                        } else {
                            let args = format!(
                                "{} -d {} -a {} -w {}{} -o {}{}{} --api-port $mport --disable-telemetry{}",
                                disable,
                                ids,
                                main_raw,
                                user,
                                opt(!pool.Pass.is_empty(), format!(" -p {}", pool.Pass)),
                                url(pool, pool_port),
                                worker_arg,
                                opt(pool.SSL && pool.SSLSelfSigned, " --no-cert-validation".into()),
                                cmd_s(cmd, "Params")
                            );
                            let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, path.clone(), "OneZeroMiner", miner_port)
                                .with_algo(&key, ctx.hashrate(&mname, &algo0), fee_of(cmd, "Fee", vendor));
                            m.arguments = MinerArgs::Str(args);
                            m.uri = uri().into();
                            m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                            m.extend_interval = cmd_i(cmd, "ExtendInterval");
                            m.manual_uri = MANUAL_URI.into();
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

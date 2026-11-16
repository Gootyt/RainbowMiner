//! Miners/BzMiner.ps1 (AMD, CPU, INTEL, NVIDIA)

use super::{host_port, opt, ssl_word};
use crate::data;
use crate::device::Device;
use crate::miner::*;
use crate::pool::Pool;
use crate::util;
use std::sync::LazyLock;

pub struct BzMiner;

const NAME: &str = "BzMiner";
const PORT: &str = "332";
const DEVFEE: f64 = 0.5;
const CUDA: &str = "11.2";
const VERSION: &str = "100.80";
const MANUAL_URI: &str = "https://github.com/bzminer/bzminer/releases";
const EXCLUDE_POOL_NAME: &str = "prohashing|miningrigrentals";
const COMMON_PARAMS: &str = "--config config_$mport.txt --http_address 127.0.0.1 --http_port $mport --no-watchdog --output log --no-color --logfile bzminer_$mport.log --logfile-mode overwrite --disable_udp";

static COMMANDS: LazyLock<Vec<serde_json::Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/BzMiner.json")));

fn uri() -> &'static str {
    if cfg!(windows) {
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v100.80-bzminer/bzminer_v100.80_windows.zip"
    } else {
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v100.80-bzminer/bzminer_v100.80_linux.tar.gz"
    }
}

fn protocol(p: &Pool) -> String {
    match p.EthMode.as_str() {
        "ethproxy" => format!("ethproxy+{}", ssl_word(p)),
        "ethstratum1" | "ethstratumnh" => format!("ethstratum+{}", ssl_word(p)),
        "ethstratum2" => format!("ethstratum2+{}", ssl_word(p)),
        _ => p.Protocol.clone(),
    }
}

/// "01:00" -> "!1:0"
fn disable_busid(b: &str) -> String {
    format!("!{}", b.split(':').map(|x| u32::from_str_radix(x.trim(), 16).unwrap_or(0).to_string()).collect::<Vec<_>>().join(":"))
}

impl MinerModule for BzMiner {
    fn name(&self) -> &'static str {
        NAME
    }
    fn types(&self) -> Vec<String> {
        vec!["AMD".into(), "CPU".into(), "INTEL".into(), "NVIDIA".into()]
    }
    fn info(&self, ctx: &MinerCtx) -> Option<MinerInfo> {
        Some(MinerInfo {
            name: NAME.into(),
            types: self.types(),
            path: ctx.path("Bin/GPU-BzMiner/bzminer"),
            uri: uri().into(),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: serde_json::Value::Array(COMMANDS.clone()),
        })
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
        let path = ctx.path("Bin/GPU-BzMiner/bzminer");
        let version = util::get_miner_version(VERSION);
        for vendor in ["AMD", "CPU", "INTEL", "NVIDIA"] {
            if vendor == "NVIDIA" && !cuda {
                continue;
            }
            for model in ctx.models(vendor) {
                let device: Vec<&Device> = ctx.devices_of(vendor, &model);
                if device.is_empty() || (vendor == "NVIDIA" && model.contains('-') && device.iter().any(|d| d.IsLHR)) {
                    continue;
                }
                let mut busids: Vec<String> = Vec::new();
                for d in ctx.devices.all_devices.iter().filter(|d| d.is_gpu() && d.Vendor == vendor) {
                    if let Some(b) = &d.BusId {
                        if !busids.contains(b) {
                            busids.push(b.clone());
                        }
                    }
                }
                let vendor_params = match vendor {
                    "AMD" => "--amd 1 --intel 0 --nvidia 0",
                    "CPU" => "--amd 0 --intel 0 --nvidia 0 --cpu 1",
                    "INTEL" => "--amd 0 --intel 1 --nvidia 0",
                    _ => "--amd 0 --intel 0 --nvidia 1",
                };
                for cmd in COMMANDS.iter().filter(|c| cmd_vendor(c, vendor)) {
                    let cv = cmd_s(cmd, "Version");
                    if !cv.is_empty() && util::get_miner_version(&cv) > version {
                        continue;
                    }
                    let mut first = true;
                    let algorithm0 = {
                        let a = cmd_s(cmd, "Algorithm");
                        if a.is_empty() {
                            cmd_s(cmd, "MainAlgorithm")
                        } else {
                            a
                        }
                    };
                    let algo0 = data::get_algorithm(&cmd_s(cmd, "MainAlgorithm"));
                    let cpu_params = opt(vendor != "CPU", format!(" --cpu {}", if cmd_b(cmd, "WithCPU") { "1" } else { "0" }));
                    let affinity_params = if vendor == "CPU" {
                        ctx.cpu_affinity(NAME, &algo0)
                            .map(|a| {
                                format!(
                                    "--cpu_affinity {} ",
                                    util::convert_from_cpu_affinity(&a).iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
                                )
                            })
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let excl = {
                        let c = cmd_s(cmd, "ExcludePoolName");
                        if c.is_empty() {
                            EXCLUDE_POOL_NAME.to_string()
                        } else {
                            format!("{}|{}", EXCLUDE_POOL_NAME, c)
                        }
                    };
                    let (mut miner_port, mut mname) = (0u16, String::new());
                    for key in ctx.algo_keys(NAME, &algo0, &model, KeyOpts { exclude_pool_name: &excl, ..Default::default() }) {
                        let Some(pool) = ctx.pool(&key) else { continue };
                        if pool.Host.is_empty() {
                            continue;
                        }
                        let (miner_device, disable_devices): (Vec<&Device>, String) = if vendor != "CPU" {
                            let min_mem = ctx.min_mem_gb(cmd, pool, &algo0);
                            let md: Vec<&Device> = device.iter().copied().filter(|d| ctx.test_vram(d, min_mem)).collect();
                            let used: Vec<String> = md.iter().filter_map(|d| d.BusId.clone()).collect();
                            let dis = busids.iter().filter(|b| !used.contains(b)).map(|b| disable_busid(b)).collect::<Vec<_>>().join(",");
                            (md, dis)
                        } else {
                            (device.clone(), String::new())
                        };
                        let cs = cmd_list(cmd, "CoinSymbol");
                        let ecs = cmd_list(cmd, "ExcludeCoinSymbol");
                        if miner_device.is_empty()
                            || ps_match(&pool.Host, &excl)
                            || (!cs.is_empty() && !util::contains_ci(&cs, &pool.CoinSymbol))
                            || (!ecs.is_empty() && util::contains_ci(&ecs, &pool.CoinSymbol))
                            || pool.User.contains('@')
                        {
                            continue;
                        }
                        if first {
                            miner_port = port_fmt(PORT, miner_device[0].Index);
                            mname = miner_name(NAME, None, &miner_device);
                            first = false;
                        }
                        let user_args = if !pool.Worker.is_empty() && pool.User.contains(&pool.Worker) {
                            format!("-u {}", pool.User)
                        } else {
                            format!("-w {}{}", pool.User, opt(!pool.Worker.is_empty(), format!(" --worker {}", pool.Worker)))
                        };
                        let args = format!(
                            "-a {} {}{}{} -p {}://{} {}{} {}{} {}",
                            algorithm0,
                            vendor_params,
                            cpu_params,
                            opt(!disable_devices.is_empty(), format!(" --devices {}", disable_devices)),
                            protocol(pool),
                            host_port(pool, pool.port_gpu()),
                            user_args,
                            opt(!pool.Pass.is_empty(), format!(" --pass {}", pool.Pass)),
                            affinity_params,
                            COMMON_PARAMS,
                            cmd_s(cmd, "Params")
                        );
                        let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, path.clone(), "BzMinerWrapper", miner_port)
                            .with_algo(&key, penalized(ctx.hashrate(&mname, &algo0), cmd), cmd_f(cmd, "Fee").unwrap_or(DEVFEE));
                        m.arguments = MinerArgs::Str(args);
                        m.uri = uri().into();
                        m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                        m.extend_interval = cmd_i(cmd, "ExtendInterval");
                        m.manual_uri = MANUAL_URI.into();
                        m.no_cpu_mining = cmd_b(cmd, "NoCPUMining");
                        m.version = VERSION.into();
                        m.base_algorithm = algo0.clone();
                        m.exclude_pool_name = excl.clone();
                        out.push(m);
                    }
                }
            }
        }
        out
    }
}

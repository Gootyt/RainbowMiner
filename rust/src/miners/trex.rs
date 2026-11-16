//! Miners/Trex.ps1 (NVIDIA)

use super::{failover_args, opt, ssl_word};
use crate::data;
use crate::miner::*;
use crate::pool::Pool;
use std::sync::LazyLock;

pub struct Trex;

const NAME: &str = "Trex";
const PORT: &str = "316";
const DEVFEE: f64 = 1.0;
const VERSION: &str = "0.26.8";
const DEVICE_CAPABILITY: &str = "5.0";
const MANUAL_URI: &str = "https://bitcointalk.org/index.php?topic=4432704.0";

static COMMANDS: LazyLock<Vec<serde_json::Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Trex.json")));

fn uri_cuda() -> Vec<(&'static str, &'static str)> {
    if cfg!(windows) {
        vec![("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.26.8-trex/t-rex-0.26.8-win.zip", "9.2")]
    } else {
        vec![("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.26.8-trex/t-rex-0.26.8-linux.tar.gz", "9.2")]
    }
}

fn protocol(p: &Pool, algo0: &str, second: bool) -> String {
    if algo0 == "Octopus" {
        return p.Protocol.clone();
    }
    match p.EthMode.as_str() {
        "qtminer" => format!("stratum1+{}", ssl_word(p)),
        "ethstratum2" if !second => format!("stratum2+{}", ssl_word(p)),
        "ethstratumnh" => format!("stratum2+{}", ssl_word(p)),
        "ethlocalproxy" => "stratum+http".into(),
        _ => p.Protocol.clone(),
    }
}

impl MinerModule for Trex {
    fn name(&self) -> &'static str {
        NAME
    }
    fn types(&self) -> Vec<String> {
        vec!["NVIDIA".into()]
    }
    fn info(&self, ctx: &MinerCtx) -> Option<MinerInfo> {
        Some(MinerInfo {
            name: NAME.into(),
            types: self.types(),
            path: ctx.path("Bin/NVIDIA-Trex/t-rex"),
            uri: uri_cuda()[0].0.into(),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: serde_json::Value::Array(COMMANDS.clone()),
        })
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        if !(ctx.is_linux() || ctx.is_windows()) || (ctx.is_linux() && ctx.is_arm()) || !ctx.has_type("NVIDIA") {
            return out;
        }
        let list = uri_cuda();
        let Some(uri) = ctx.pick_cuda(&list, NAME) else { return out };
        let path = ctx.path("Bin/NVIDIA-Trex/t-rex");
        let show_window = ctx.config.b("ShowMinerWindow");
        for model in ctx.models("NVIDIA") {
            let device: Vec<&crate::device::Device> = ctx.devices_of("NVIDIA", &model).into_iter().filter(|d| capability_ok(d, DEVICE_CAPABILITY)).collect();
            if device.is_empty() {
                continue;
            }
            for cmd in COMMANDS.iter() {
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
                let second_raw = cmd_s(cmd, "SecondAlgorithm");
                let algo2_0 = if second_raw.is_empty() { None } else { Some(data::get_algorithm(&second_raw)) };
                let exclude = cmd_s(cmd, "ExcludePoolName");
                let (mut miner_port, mut mname, mut ids) = (0u16, String::new(), String::new());
                for key in ctx.algo_keys(NAME, &algo0, &model, KeyOpts { exclude_pool_name: &exclude, ..Default::default() }) {
                    let Some(pool) = ctx.pool(&key) else { continue };
                    if pool.Host.is_empty() {
                        continue;
                    }
                    let mut min_mem = ctx.min_mem_gb(cmd, pool, &algo0);
                    let cmd_min = cmd_f(cmd, "MinMemGB").unwrap_or(0.0);
                    if algo2_0.is_none() && cmd_b(cmd, "DAG") && data::RE_ALGO_IS_ETHASH.is_match(&algo0) && min_mem > cmd_min && ctx.config.b("EnableEthashZombieMode") {
                        min_mem = cmd_min;
                    }
                    let mut miner_device: Vec<&crate::device::Device> = device.iter().copied().filter(|d| ctx.test_vram(d, min_mem)).collect();
                    let mut is_lhr = true;
                    for d in &miner_device {
                        let enable_lhr = ctx.config.device(&d.Model_Base).and_then(|c| c.enable_lhr);
                        if (!d.IsLHR && enable_lhr != Some(true)) || enable_lhr == Some(false) {
                            is_lhr = false;
                            break;
                        }
                    }
                    if miner_device.is_empty() || (!exclude.is_empty() && ps_match(&pool.Host, &exclude)) {
                        continue;
                    }
                    if first {
                        miner_port = port_fmt(PORT, miner_device[0].Index);
                        mname = miner_name(NAME, algo2_0.as_ref().map(|a2| format!("{}_{}", algo0, a2)).as_deref(), &miner_device);
                        ids = device_ids(&miner_device, "Type_Vendor_Index", ",", false);
                        first = false;
                    }
                    let pool_port = pool.port_gpu();
                    let proto = protocol(pool, &algo0, false);
                    let main_args = format!(
                        "-N 10 -r 5 --api-bind-http 127.0.0.1:$mport -d {} -a {} -o {}://{}:{} -u {}{}{}{}{}",
                        ids,
                        algorithm,
                        proto,
                        pool.Host,
                        pool_port,
                        pool.User,
                        opt(!pool.Wallet.is_empty() && !pool.Worker.is_empty(), format!(" -w {}", pool.Worker)),
                        opt(!pool.Pass.is_empty(), format!(" -p {}", pool.Pass)),
                        failover_args(pool),
                        opt(pool.SSL, " --no-strict-ssl".into()),
                    );
                    let devfee = cmd_f(cmd, "DevFee").unwrap_or(DEVFEE);
                    let hr_dur = cmd_s(cmd, "HashrateDuration");
                    if let Some(algo2_0) = &algo2_0 {
                        if cmd_b(cmd, "DAG") {
                            let p2 = ctx.pool(algo2_0);
                            min_mem += match p2.and_then(|p| p.DagSizeMax) {
                                Some(d) if d > 0.0 => d,
                                _ => data::get_eth_dag_size(p2.map(|p| p.CoinSymbol.as_str()).unwrap_or(""), algo2_0, cmd_f(cmd, "MinMemGB2nd").unwrap_or(1.0)),
                            };
                            miner_device = device.iter().copied().filter(|d| ctx.test_vram(d, min_mem)).collect();
                        }
                        if (!miner_device.is_empty() && is_lhr) || cmd_b(cmd, "DualAll") {
                            for key2 in ctx.second_keys(algo2_0, &model, true) {
                                let Some(pool2) = ctx.pool(&key2) else { continue };
                                if pool2.Host.is_empty() || pool2.User.is_empty() || miner_device.is_empty() || (!exclude.is_empty() && ps_match(&pool2.Host, &exclude)) {
                                    continue;
                                }
                                let args = format!(
                                    "{} --dual-algo {} --url2 {}://{}:{} --user2 {}{}{}{} --no-watchdog --no-new-block-info {}",
                                    main_args,
                                    second_raw,
                                    protocol(pool2, &key2, true),
                                    pool2.Host,
                                    pool2.port_gpu(),
                                    pool2.User,
                                    opt(!pool2.Wallet.is_empty() && !pool2.Worker.is_empty(), format!(" --worker2 {}", pool2.Worker)),
                                    opt(!pool2.Pass.is_empty(), format!(" --pass2 {}", pool2.Pass)),
                                    opt(!show_window, " --no-color".into()),
                                    cmd_s(cmd, "Params")
                                );
                                let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, path.clone(), "Trex", miner_port)
                                    .with_algo(&key, ctx.hashrate_dur(&mname, &algo0, &hr_dur), devfee)
                                    .with_algo(&key2, ctx.hashrate_dur(&mname, algo2_0, &hr_dur), 0.0);
                                m.arguments = MinerArgs::Str(args);
                                m.uri = uri.into();
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
                            "{}{} --no-watchdog --no-new-block-info {}",
                            main_args,
                            opt(!show_window, " --no-color".into()),
                            cmd_s(cmd, "Params")
                        );
                        let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, path.clone(), "Trex", miner_port).with_algo(&key, ctx.hashrate_dur(&mname, &algo0, &hr_dur), devfee);
                        m.arguments = MinerArgs::Str(args);
                        m.uri = uri.into();
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
        out
    }
}

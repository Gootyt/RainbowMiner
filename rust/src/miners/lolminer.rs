//! Miners/lolminer.ps1 (AMD, NVIDIA)

use super::opt;
use crate::data;
use crate::device::Device;
use crate::miner::*;
use crate::pool::Pool;
use crate::util;
use std::sync::LazyLock;

pub struct LolMiner;

const NAME: &str = "lolminer";
const PORT: &str = "317";
const CUDA: &str = "10.0";
const DEVFEE: f64 = 1.0;
const VERSION: &str = "1.98a";
const MANUAL_URI: &str = "https://bitcointalk.org/index.php?topic=4724735.0";

static COMMANDS: LazyLock<Vec<serde_json::Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/lolminer.json")));

fn uri() -> &'static str {
    if cfg!(windows) {
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.98-lolminer/lolMiner_v1.98a_Win64.zip"
    } else {
        "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.98-lolminer/lolMiner_v1.98a_Lin64.tar.gz"
    }
}

fn pool_host(p: &Pool) -> String {
    let port = p.port_gpu();
    let re = regex::Regex::new(r"^[^/]+/.+$").unwrap();
    if port > 0 && !re.is_match(&p.Host) {
        format!("{}:{}", p.Host, port)
    } else {
        p.Host.clone()
    }
}

fn ton_mode(p: &Pool) -> Option<i32> {
    match p.EthMode.as_str() {
        "icemining" => Some(6),
        "toncoinpool" => Some(3),
        "tonpool" => Some(2),
        _ => None,
    }
}

impl MinerModule for LolMiner {
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
            path: ctx.path("Bin/GPU-lolMiner/lolMiner"),
            uri: uri().into(),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: serde_json::Value::Array(COMMANDS.clone()),
        }
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        if !(ctx.is_linux() || ctx.is_windows()) || (ctx.is_linux() && ctx.is_arm()) || !(ctx.has_type("AMD") || ctx.has_type("INTEL") || ctx.has_type("NVIDIA")) {
            return out;
        }
        let cuda = !ctx.has_type("NVIDIA") || ctx.cuda_ok(CUDA, NAME);
        let path = ctx.path("Bin/GPU-lolMiner/lolMiner");
        let watchdog = if ctx.config.b("RebootOnGPUFailure") && ctx.config.b("EnableRestartComputer") {
            format!(
                "--watchdog script --watchdogscript '{}'",
                ctx.base.join(if ctx.is_linux() { "reboot.sh" } else { "Reboot.bat" }).display()
            )
        } else {
            "--watchdog exit".to_string()
        };
        for vendor in ["AMD", "NVIDIA"] {
            if vendor == "NVIDIA" && !cuda {
                continue;
            }
            for model in ctx.models(vendor) {
                let device: Vec<&Device> = ctx.devices_of(vendor, &model).into_iter().filter(|d| d.is_gpu()).collect();
                if device.is_empty() {
                    continue;
                }
                for cmd in COMMANDS.iter().filter(|c| cmd_vendor(c, vendor)) {
                    let mut first = true;
                    let algo0 = data::get_algorithm(&cmd_s(cmd, "MainAlgorithm"));
                    let algo2_raw = cmd_s(cmd, "SecondAlgorithm");
                    let algo2_0 = if algo2_raw.is_empty() { None } else { Some(data::get_algorithm(&algo2_raw)) };
                    let device_params = opt(ctx.is_windows() && algo0 == "NexaPoW", " --keepfree 1024".into());
                    let exclude = cmd_s(cmd, "ExcludePoolName");
                    let (mut miner_port, mut mname, mut ids, mut lhrs) = (0u16, String::new(), String::new(), String::new());
                    for key in ctx.algo_keys(NAME, &algo0, &model, KeyOpts { exclude_pool_name: &exclude, ..Default::default() }) {
                        let Some(pool) = ctx.pool(&key) else { continue };
                        if pool.Host.is_empty() {
                            continue;
                        }
                        let mut min_mem = ctx.min_mem_gb(cmd, pool, &algo0);
                        let cmd_min = cmd_f(cmd, "MinMemGB").unwrap_or(0.0);
                        let zombie = cmd_list(cmd, "ZombieMode");
                        if cmd_b(cmd, "DAG")
                            && min_mem > cmd_min
                            && ((data::RE_ALGO_IS_ETHASH.is_match(&algo0) && ctx.config.b("EnableEthashZombieMode")) || util::contains_ci(&zombie, vendor))
                        {
                            min_mem = cmd_min;
                        }
                        let miner_device: Vec<&Device> = device.iter().copied().filter(|d| ctx.test_vram(d, min_mem)).collect();
                        if miner_device.is_empty() || (!exclude.is_empty() && ps_match(&pool.Host, &exclude)) {
                            continue;
                        }
                        if first {
                            miner_port = port_fmt(PORT, miner_device[0].Index);
                            mname = miner_name(NAME, algo2_0.as_ref().map(|a2| format!("{}-{}", algo0, a2)).as_deref(), &miner_device);
                            ids = device_ids(&miner_device, "BusId_Type_Mineable_Index", ",", false);
                            lhrs = if vendor == "NVIDIA" {
                                miner_device.iter().map(|d| if d.IsLHR { "auto" } else { "off" }).collect::<Vec<_>>().join(",")
                            } else {
                                String::new()
                            };
                            first = false;
                        }
                        let pers_cmd = cmd_b(cmd, "Pers");
                        let pers = if pers_cmd { data::get_equihash_coin_pers(&pool.CoinSymbol, "auto") } else { String::new() };
                        let eth_stratum = if data::RE_ALGO_HAS_ETHPROXY.is_match(&algo0) {
                            match pool.EthMode.as_str() {
                                "ethproxy" => "ETHPROXY",
                                "qtminer" => {
                                    if ps_match(&pool.Host, "miningrigrentals") {
                                        "ETHPROXY"
                                    } else {
                                        "ETHV1"
                                    }
                                }
                                _ => "ETHV1",
                            }
                        } else {
                            ""
                        };
                        let params_auto = cmd_s(cmd, "ParamsAutoPers");
                        if !(!pers_cmd || !pers.is_empty() || !params_auto.is_empty()) {
                            continue;
                        }
                        let user = if ps_match(&pool.Host, "miningrigrentals") { pool.User.replace('.', "*") } else { pool.User.clone() };
                        let pool_args = format!(
                            "--pool {}{} --user {}{}{}",
                            opt(algo0 == "SHA256ton" && !pool.Protocol.is_empty(), format!("{}://", pool.Protocol)),
                            pool_host(pool),
                            user,
                            opt(!pool.Pass.is_empty(), format!(" --pass {}", pool.Pass)),
                            opt(algo0 != "SHA256ton", format!(" --tls {}", if pool.SSL { "on" } else { "off" }))
                        );
                        let tail = format!(
                            " --devices {} --apiport $mport --digits 2 --longstats 60 --shortstats 5 --connectattempts 3 {}{}{}{}{} {}",
                            ids,
                            opt(!lhrs.is_empty(), format!("--lhrtune {} ", lhrs)),
                            opt(!eth_stratum.is_empty(), format!("--ethstratum {} ", eth_stratum)),
                            opt(!pers.is_empty() && pers != "auto", format!("--pers {} ", pers)),
                            watchdog,
                            device_params,
                            if pers == "auto" && !params_auto.is_empty() { params_auto.clone() } else { cmd_s(cmd, "Params") }
                        );
                        let fee = cmd_f(cmd, "Fee").unwrap_or(0.0);
                        if let Some(algo2_0) = &algo2_0 {
                            let mut intens = ctx.intensities(NAME, &model, &algo0, algo2_0);
                            if intens.is_empty() {
                                intens = vec!["2".into()];
                            }
                            for intensity in intens {
                                let iv = util::parse_f64(&intensity).unwrap_or(0.0);
                                let (name_dual, dual_factor) = if iv > 0.0 {
                                    (miner_name(NAME, Some(&format!("{}-{}-{}", algo0, algo2_0, intensity)), &miner_device), format!(" --dualfactor {}", intensity))
                                } else {
                                    (mname.clone(), String::new())
                                };
                                for key2 in ctx.second_keys(algo2_0, &model, true) {
                                    let Some(pool2) = ctx.pool(&key2) else { continue };
                                    if pool2.Host.is_empty() || (!exclude.is_empty() && ps_match(&pool2.Host, &exclude)) {
                                        continue;
                                    }
                                    let pool2_args = format!(
                                        "--dualpool {}{} --dualuser {}{}{}{}",
                                        opt(algo2_0 == "SHA256ton" && !pool2.Protocol.is_empty(), format!("{}://", pool2.Protocol)),
                                        pool_host(pool2),
                                        pool2.User,
                                        opt(!pool2.Pass.is_empty(), format!(" --dualpass {}", pool2.Pass)),
                                        opt(algo2_0 != "SHA256ton", format!(" --dualtls {}", if pool2.SSL { "on" } else { "off" })),
                                        dual_factor
                                    );
                                    let ton = if algo2_0 == "SHA256ton" { ton_mode(pool2) } else { None };
                                    let args = format!(
                                        "{}{} {}{}{}{}",
                                        pool_args,
                                        opt(!pool.Worker.is_empty(), format!(" --worker {}", pool.Worker)),
                                        pool2_args,
                                        ton.map(|t| format!(" --ton-mode {}", t)).unwrap_or_default(),
                                        opt(!pool2.Worker.is_empty(), format!(" --dualworker {}", pool2.Worker)),
                                        tail
                                    );
                                    let mut m = MinerCandidate::new(NAME, &name_dual, &miner_device, &model, path.clone(), "Lol", miner_port)
                                        .with_algo(&key, ctx.hashrate(&name_dual, &algo0), fee)
                                        .with_algo(&key2, ctx.hashrate(&name_dual, algo2_0), 0.0);
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
                            let ton = if algo0 == "SHA256ton" { ton_mode(pool) } else { None };
                            let args = format!(
                                "{}{}{}{}",
                                pool_args,
                                ton.map(|t| format!(" --ton-mode {}", t)).unwrap_or_default(),
                                opt(!pool.Worker.is_empty(), format!(" --worker {}", pool.Worker)),
                                tail
                            );
                            let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, path.clone(), "Lol", miner_port).with_algo(&key, ctx.hashrate(&mname, &algo0), fee);
                            m.arguments = MinerArgs::Str(args);
                            m.uri = uri().into();
                            m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                            m.extend_interval = cmd_i(cmd, "ExtendInterval");
                            m.manual_uri = MANUAL_URI.into();
                            m.version = VERSION.into();
                            m.base_algorithm = algo0.clone();
                            m.exclude_pool_name = exclude.clone();
                            m.list_devices = "--list-devices".into();
                            out.push(m);
                        }
                    }
                }
            }
        }
        out
    }
}

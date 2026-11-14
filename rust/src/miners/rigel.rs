//! Miners/Rigel.ps1 (NVIDIA)

use super::{opt, ssl_word};
use crate::data;
use crate::device::Device;
use crate::miner::*;
use crate::pool::Pool;
use crate::util;
use std::sync::LazyLock;

pub struct Rigel;

const NAME: &str = "Rigel";
const PORT: &str = "324";
const DEVFEE: f64 = 0.7;
const VERSION: &str = "1.23.2";
const MANUAL_URI: &str = "https://github.com/rigelminer/rigel/releases";

static COMMANDS: LazyLock<Vec<serde_json::Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Rigel.json")));

fn uri_cuda() -> Vec<(&'static str, &'static str)> {
    if cfg!(windows) {
        vec![("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.23.2-rigel/rigel-1.23.2-win.zip", "8.0")]
    } else {
        vec![("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.23.2-rigel/rigel-1.23.2-linux.tar.gz", "8.0")]
    }
}

fn protocol(p: &Pool) -> String {
    match p.EthMode.as_str() {
        "ethproxy" => format!("ethproxy+{}", ssl_word(p)),
        "ethstratum" | "ethstratum1" | "ethstratum2" | "ethstratumnh" => format!("ethstratum+{}", ssl_word(p)),
        _ => format!("stratum+{}", ssl_word(p)),
    }
}

/// the --coin parameter (mirrors the original switch, including its RVN branch order)
fn coin_params(algorithm: &str, p: &Pool) -> String {
    let c = p.CoinSymbol.as_str();
    if algorithm.eq_ignore_ascii_case("kawpow") {
        if c == "AIPG" {
            " --coin aipg".into()
        } else if c == "CLORE" {
            " --coin clore".into()
        } else if c == "NEOX" {
            " --coin neox".into()
        } else if c == "QUAI" {
            " --coin quai".into()
        } else if c != "RVN" {
            " --coin ravencoin".into()
        } else {
            String::new()
        }
    } else if algorithm.eq_ignore_ascii_case("ethash") {
        if c == "OCTA" {
            " --coin octa".into()
        } else if c == "XPB" {
            " --coin xpb".into()
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}

impl MinerModule for Rigel {
    fn name(&self) -> &'static str {
        NAME
    }
    fn types(&self) -> Vec<String> {
        vec!["NVIDIA".into()]
    }
    fn info(&self, ctx: &MinerCtx) -> MinerInfo {
        MinerInfo {
            name: NAME.into(),
            types: self.types(),
            path: ctx.path("Bin/NVIDIA-Rigel/rigel"),
            uri: uri_cuda()[0].0.into(),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: serde_json::Value::Array(COMMANDS.clone()),
        }
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        if !(ctx.is_linux() || ctx.is_windows()) || (ctx.is_linux() && ctx.is_arm()) || !ctx.has_type("NVIDIA") {
            return out;
        }
        let list = uri_cuda();
        let Some(uri) = ctx.pick_cuda(&list, NAME) else { return out };
        let path = ctx.path("Bin/NVIDIA-Rigel/rigel");
        for model in ctx.models("NVIDIA") {
            let device: Vec<&Device> = ctx.devices_of("NVIDIA", &model).into_iter().filter(|d| d.is_gpu()).collect();
            if device.is_empty() {
                continue;
            }
            for cmd in COMMANDS.iter().filter(|c| cmd_vendor(c, "NVIDIA")) {
                let mut first = true;
                let algo0 = data::get_algorithm(&cmd_s(cmd, "MainAlgorithm"));
                let algo2_raw = cmd_s(cmd, "SecondaryAlgorithm");
                let algo2_0 = if algo2_raw.is_empty() { None } else { Some(data::get_algorithm(&algo2_raw)) };
                let main_algorithm = {
                    let a = cmd_s(cmd, "Algorithm");
                    if a.is_empty() {
                        cmd_s(cmd, "MainAlgorithm")
                    } else {
                        a
                    }
                };
                let exclude = cmd_s(cmd, "ExcludePoolName");
                let pool_name = cmd_s(cmd, "PoolName");
                let (mut miner_port, mut mname, mut ids) = (0u16, String::new(), String::new());
                for key in ctx.algo_keys(NAME, &algo0, &model, KeyOpts { exclude_pool_name: &exclude, pool_name: &pool_name, ..Default::default() }) {
                    let Some(pool) = ctx.pool(&key) else { continue };
                    if pool.Host.is_empty() {
                        continue;
                    }
                    let mut min_mem = ctx.min_mem_gb(cmd, pool, &algo0);
                    let cmd_min = cmd_f(cmd, "MinMemGB").unwrap_or(0.0);
                    if cmd_b(cmd, "DAG") && min_mem > cmd_min && util::contains_ci(&cmd_list(cmd, "ZombieMode"), "NVIDIA") {
                        min_mem = cmd_min;
                    }
                    let miner_device: Vec<&Device> = device.iter().copied().filter(|d| ctx.test_vram(d, min_mem)).collect();
                    let coin = coin_params(&main_algorithm, pool);
                    if miner_device.is_empty() || (!exclude.is_empty() && ps_match(&pool.Name, &exclude)) || (!pool_name.is_empty() && !ps_match(&pool.Name, &pool_name)) {
                        continue;
                    }
                    if first {
                        miner_port = port_fmt(PORT, miner_device[0].Index);
                        mname = miner_name(NAME, algo2_0.as_ref().map(|a2| format!("{}-{}", algo0, a2)).as_deref(), &miner_device);
                        ids = device_ids(&miner_device, "Type_Vendor_Index", ",", false);
                        first = false;
                    }
                    let pool_port = pool.port_gpu();
                    let proto = protocol(pool);
                    let fee = cmd_f(cmd, "Fee").unwrap_or(DEVFEE);
                    if let Some(algo2_0) = &algo2_0 {
                        let mut intens = ctx.intensities(NAME, &model, &algo0, algo2_0);
                        if intens.is_empty() {
                            intens = vec!["0".into()];
                        }
                        let sec_excl = cmd_s(cmd, "SecondaryExcludePoolName");
                        let sec_pool = cmd_s(cmd, "SecondaryPoolName");
                        for intensity in intens {
                            let iv = util::parse_f64(&intensity).unwrap_or(0.0);
                            let (name_dual, int_params) = if iv > 0.0 {
                                (
                                    miner_name(NAME, Some(&format!("{}-{}-{}", algo0, algo2_0, intensity)), &miner_device),
                                    format!(" --dual-mode {}", vec![format!("a12:r{}", intensity); miner_device.len()].join(",")),
                                )
                            } else {
                                (mname.clone(), String::new())
                            };
                            for key2 in ctx.second_keys(algo2_0, &model, true) {
                                let Some(pool2) = ctx.pool(&key2) else { continue };
                                if pool2.Host.is_empty()
                                    || pool2.User.is_empty()
                                    || (!sec_excl.is_empty() && ps_match(&pool2.Name, &sec_excl))
                                    || (!sec_pool.is_empty() && !ps_match(&pool2.Name, &sec_pool))
                                {
                                    continue;
                                }
                                let args = format!(
                                    "--api-bind 127.0.0.1:$mport -d {}{} -a {}+{}{} -o [1]{}://{}:{} -u [1]{}{}{} -o [2]{}://{}:{} -u [2]{}{}{} --no-tui --no-watchdog --dag-reset-mclock off {}",
                                    ids,
                                    int_params,
                                    main_algorithm,
                                    algo2_raw,
                                    coin,
                                    proto,
                                    pool.Host,
                                    pool_port,
                                    pool.User,
                                    opt(!pool.Pass.is_empty(), format!(" -p [1]{}", pool.Pass)),
                                    opt(!pool.Worker.is_empty() && pool.User == pool.Wallet, format!(" -w [1]{}", pool.Worker)),
                                    pool2.Protocol,
                                    pool2.Host,
                                    pool2.port_gpu(),
                                    pool2.User,
                                    opt(!pool2.Pass.is_empty(), format!(" -p [2]{}", pool2.Pass)),
                                    opt(!pool2.Worker.is_empty() && pool2.User == pool2.Wallet, format!(" -w [2]{}", pool2.Worker)),
                                    cmd_s(cmd, "Params")
                                );
                                let mut m = MinerCandidate::new(NAME, &name_dual, &miner_device, &model, path.clone(), "Rigel", miner_port)
                                    .with_algo(&key, penalized(ctx.hashrate(&name_dual, &algo0), cmd), fee)
                                    .with_algo(&key2, penalized(ctx.hashrate(&name_dual, algo2_0), cmd), 0.0);
                                m.arguments = MinerArgs::Str(args);
                                m.uri = uri.into();
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
                            "--api-bind 127.0.0.1:$mport -d {} -a {}{} -o {}://{}:{} -u {}{}{} --no-tui --no-watchdog --dag-reset-mclock off {}",
                            ids,
                            main_algorithm,
                            coin,
                            proto,
                            pool.Host,
                            pool_port,
                            pool.User,
                            opt(!pool.Pass.is_empty(), format!(" -p {}", pool.Pass)),
                            opt(!pool.Worker.is_empty() && pool.User == pool.Wallet, format!(" -w {}", pool.Worker)),
                            cmd_s(cmd, "Params")
                        );
                        let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, path.clone(), "Rigel", miner_port).with_algo(&key, ctx.hashrate(&mname, &algo0), fee);
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

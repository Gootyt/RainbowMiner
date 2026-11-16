//! Miners/CpuminerJayddee.ps1 (cpuminer-opt, CPU) - an Invoke-MinerFamily module

use crate::data;
use crate::device::Device;
use crate::miner::*;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::LazyLock;

pub struct CpuminerJayddee;

const NAME: &str = "CpuminerJayddee";
const PORT: &str = "200";
const DEVFEE: f64 = 0.0;
const VERSION: &str = "26.1";
const MANUAL_URI: &str = "https://github.com/JayDDee/cpuminer-opt/releases";

static COMMANDS: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerJayddee.json")));

/// the binary that fits the CPU features, and the download
fn path_uri(ctx: &MinerCtx) -> Option<(PathBuf, String)> {
    let f = |k: &str| ctx.cpu.has(k);
    let amd = ctx.cpu.Vendor == "AMD";
    if cfg!(target_os = "linux") {
        if ctx.is_arm() {
            let (dist, file) = match ctx.cpu.Architecture.as_str() {
                "8" => {
                    let d = ctx.distro(&["jammy", "noble"], "jammy");
                    let sfx = if f("sha3") && f("sve2") && f("aes") {
                        ".5-crypto-sha3-sve2"
                    } else if f("sha3") && f("aes") {
                        ".4-crypto-sha3"
                    } else if f("sha2") && f("aes") {
                        "-crypto"
                    } else {
                        ""
                    };
                    (d, format!("cpuminer-armv8{}", sfx))
                }
                "9" => {
                    let sfx = if f("sha3") && f("aes") {
                        "-crypto-sha3"
                    } else if f("sha2") && f("aes") {
                        "-crypto"
                    } else {
                        ""
                    };
                    ("noble".to_string(), format!("cpuminer-armv9{}", sfx))
                }
                _ => return None,
            };
            return Some((
                ctx.path(&format!("Bin/CPU-JayDDee/{}", file)),
                format!("https://github.com/RainbowMiner/miner-binaries/releases/download/v26.1-jayddee/cpuminer-opt-26.1-{}-arm.7z", dist),
            ));
        }
        let d = ctx.distro(&["focal", "jammy", "noble"], "focal");
        let sfx = if f("avx512") && f("sha") && f("vaes") {
            if d == "noble" && amd {
                "zen4"
            } else {
                "avx512-sha-vaes"
            }
        } else if f("avx512") {
            "avx512"
        } else if f("avx2") && f("sha") && f("vaes") {
            if amd {
                "zen3"
            } else {
                "avx2-sha-vaes"
            }
        } else if f("avx2") && f("sha") && f("aes") {
            "avx2-sha"
        } else if f("avx2") && f("aes") {
            "avx2"
        } else if f("avx") && f("aes") {
            "avx"
        } else if f("sse42") && f("aes") && f("sha") {
            "sse42-aes-sha"
        } else if f("sse42") && f("aes") {
            "sse42-aes"
        } else if f("sse42") {
            "sse42"
        } else if f("ssse3") {
            "ssse3"
        } else {
            "sse2"
        };
        return Some((
            ctx.path(&format!("Bin/CPU-JayDDee/cpuminer-{}", sfx)),
            format!("https://github.com/RainbowMiner/miner-binaries/releases/download/v26.1-jayddee/cpuminer-opt-26.1-{}-x64.7z", d),
        ));
    }
    if cfg!(windows) {
        let sfx = if f("avx512") && f("sha") && f("vaes") {
            "avx512-sha-vaes"
        } else if f("avx512") {
            "avx512"
        } else if f("avx2") && f("sha") && f("vaes") {
            "avx2-sha-vaes"
        } else if f("avx2") && f("sha") && f("aes") {
            "avx2-sha"
        } else if f("avx2") && f("aes") {
            "avx2"
        } else if f("avx") && f("aes") {
            "avx"
        } else if f("sse42") && f("aes") {
            "aes-sse42"
        } else {
            "sse2"
        };
        return Some((
            ctx.path(&format!("Bin/CPU-JayDDee/cpuminer-{}.exe", sfx)),
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v26.1-jayddee/cpuminer-opt-26.1-windows.zip".into(),
        ));
    }
    None
}

fn verthash_file(ctx: &MinerCtx) -> PathBuf {
    let own = ctx.base.join("Bin/CPU-JayDDee/verthash.dat");
    if own.exists() {
        return own;
    }
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
    ctx.base.join("Bin/Common/verthash.dat")
}

impl MinerModule for CpuminerJayddee {
    fn name(&self) -> &'static str {
        NAME
    }
    fn types(&self) -> Vec<String> {
        vec!["CPU".into(), "ARMCPU".into()]
    }
    fn info(&self, ctx: &MinerCtx) -> Option<MinerInfo> {
        let pu = path_uri(ctx);
        Some(MinerInfo {
            name: NAME.into(),
            types: self.types(),
            path: pu.as_ref().map(|x| x.0.clone()).unwrap_or_default(),
            uri: pu.map(|x| x.1).unwrap_or_default(),
            dev_fee: DEVFEE,
            manual_uri: MANUAL_URI.into(),
            commands: Value::Array(COMMANDS.clone()),
        })
    }
    fn get_miners(&self, ctx: &MinerCtx) -> Vec<MinerCandidate> {
        let mut out = Vec::new();
        if !ctx.has_type("CPU") {
            return out;
        }
        let Some((path, uri)) = path_uri(ctx) else { return out };
        let verthash = verthash_file(ctx);
        let neverprofitable = ctx.config.b("EnableNeverprofitableAlgos");
        for model in ctx.models("CPU") {
            let mut first = true;
            let miner_device: Vec<&Device> = ctx.devices_of("CPU", &model);
            if miner_device.is_empty() {
                continue;
            }
            let (mut miner_port, mut mname) = (0u16, String::new());
            for cmd in COMMANDS.iter().filter(|c| (!cmd_b(c, "LinuxOnly") || cfg!(target_os = "linux")) && (!cmd_b(c, "NeverProfitable") || neverprofitable)) {
                let algo0 = data::get_algorithm(&cmd_s(cmd, "MainAlgorithm"));
                let threads = ctx.cpu_threads(NAME, &algo0);
                let affinity = ctx.cpu_affinity(NAME, &algo0);
                let is_verthash = algo0 == "Verthash";
                let device_params = format!(
                    "{}{}{}",
                    threads.map(|t| format!(" -t {}", t)).unwrap_or_default(),
                    affinity.as_ref().map(|a| format!(" --cpu-affinity {}", a)).unwrap_or_default(),
                    if is_verthash { format!(" --data-file \"{}\"", verthash.display()) } else { String::new() }
                );
                let algorithm = {
                    let a = cmd_s(cmd, "Algorithm");
                    if a.is_empty() {
                        cmd_s(cmd, "MainAlgorithm")
                    } else {
                        a
                    }
                };
                let exclude = cmd_s(cmd, "ExcludePoolName");
                for key in ctx.algo_keys(NAME, &algo0, &model, KeyOpts { no_gpu: true, exclude_pool_name: &exclude, ..Default::default() }) {
                    let Some(pool) = ctx.pool(&key) else { continue };
                    if pool.Host.is_empty() || (!exclude.is_empty() && ps_match(&pool.Host, &exclude)) {
                        continue;
                    }
                    if first {
                        miner_port = port_fmt(PORT, miner_device[0].Index);
                        mname = miner_name(NAME, None, &miner_device);
                        first = false;
                    }
                    let args = format!(
                        "-b 127.0.0.1:$mport -a {} -o {}://{}:{} -u {}{}{} -q {}",
                        algorithm,
                        pool.Protocol,
                        pool.Host,
                        pool.Port,
                        pool.User,
                        if pool.Pass.is_empty() { String::new() } else { format!(" -p {}", pool.Pass) },
                        device_params,
                        cmd_s(cmd, "Params")
                    );
                    let mut m = MinerCandidate::new(NAME, &mname, &miner_device, &model, path.clone(), "Ccminer", miner_port)
                        .with_algo(&key, ctx.hashrate_dur(&mname, &algo0, &cmd_s(cmd, "HashrateDuration")), DEVFEE);
                    m.arguments = MinerArgs::Str(args);
                    m.uri = uri.clone();
                    m.fault_tolerance = cmd_f(cmd, "FaultTolerance");
                    m.extend_interval = cmd_f(cmd, "ExtendInterval").map(|x| x as i64).unwrap_or(2);
                    m.manual_uri = MANUAL_URI.into();
                    m.version = VERSION.into();
                    m.base_algorithm = algo0.clone();
                    m.exclude_pool_name = exclude.clone();
                    if is_verthash {
                        m.prerequisite_path = Some(verthash.clone());
                        m.prerequisite_uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0-verthash/verthash.dat".into();
                        m.prerequisite_msg = "Downloading verthash.dat (1.2GB) in the background, please wait!".into();
                    }
                    out.push(m);
                }
            }
        }
        out
    }
}

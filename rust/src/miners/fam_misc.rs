//! Single algorithm modules that fit the family driver: CpuminerWyvern, CryptoDredge, CryptoDredge25,
//! Dynexsolve, Ethminer, MeowPowMiner, Multiminer, NinjaRig, Nsfminer, PhihashMiner, SgminerFancyIX,
//! VertHash.

use super::family::*;
use super::opt;
use crate::device::Device;
use crate::jsonx;
use crate::miner::*;
use crate::pool::Pool;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::LazyLock;

pub fn modules() -> Vec<FamilyModule> {
    vec![
        FamilyModule { name: "CpuminerWyvern", types: &["CPU"], def: cpuminer_wyvern },
        FamilyModule { name: "CryptoDredge", types: &["NVIDIA"], def: cryptodredge },
        FamilyModule { name: "CryptoDredge25", types: &["NVIDIA"], def: cryptodredge25 },
        FamilyModule { name: "Dynexsolve", types: &["NVIDIA"], def: dynexsolve },
        FamilyModule { name: "Ethminer", types: &["AMD", "NVIDIA"], def: ethminer },
        FamilyModule { name: "MeowPowMiner", types: &["AMD", "NVIDIA"], def: meowpowminer },
        FamilyModule { name: "Multiminer", types: &["AMD", "NVIDIA"], def: multiminer },
        FamilyModule { name: "NinjaRig", types: &["AMD", "CPU", "NVIDIA"], def: ninjarig },
        FamilyModule { name: "Nsfminer", types: &["AMD", "NVIDIA"], def: nsfminer },
        FamilyModule { name: "PhihashMiner", types: &["AMD", "NVIDIA"], def: phihashminer },
        FamilyModule { name: "SgminerFancyIX", types: &["AMD", "INTEL"], def: sgminer_fancyix },
        FamilyModule { name: "VertHash", types: &["AMD", "INTEL", "NVIDIA"], def: verthash },
    ]
}

macro_rules! cmds {
    ($c:ident, $f:literal) => {
        static $c: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!(concat!("../../assets/miners/", $f, ".json"))));
    };
}
cmds!(CMD_WYVERN, "CpuminerWyvern");
cmds!(CMD_CD, "CryptoDredge");
cmds!(CMD_CD25, "CryptoDredge25");
cmds!(CMD_DYNEX, "Dynexsolve");
cmds!(CMD_ETHMINER, "Ethminer");
cmds!(CMD_MEOW, "MeowPowMiner");
cmds!(CMD_MULTI, "Multiminer");
cmds!(CMD_NINJA, "NinjaRig");
cmds!(CMD_NSF, "Nsfminer");
cmds!(CMD_PHI, "PhihashMiner");
cmds!(CMD_FANCY, "SgminerFancyIX");
cmds!(CMD_VERTHASH, "VertHash");

fn win_linux_x64(ctx: &MinerCtx) -> bool {
    (ctx.is_windows() || ctx.is_linux()) && !(ctx.is_linux() && ctx.is_arm())
}

fn min_mem(cmd: &Value) -> f64 {
    jsonx::num(jsonx::get(cmd, "MinMemGB"))
}

/// Test-VRAM with the pool's DAG size (Get-EthDAGSize of the pool's coin)
fn dag_filter(_: &Family, ctx: &MinerCtx, d: &Device, cmd: &Value, pool: Option<&Pool>, algo0: &str) -> bool {
    let Some(pool) = pool else { return false };
    ctx.test_vram(d, dag_size(pool, algo0, "", min_mem(cmd)))
}

/// Test-VRAM with the command's MinMemGB (or DAG size)
fn vram_filter(_: &Family, ctx: &MinerCtx, d: &Device, cmd: &Value, pool: Option<&Pool>, algo0: &str) -> bool {
    let Some(pool) = pool else { return false };
    ctx.test_vram(d, ctx.min_mem_gb(cmd, pool, algo0))
}

/// Get-UrlEncode -ConvertDot
fn enc_dot(s: &str) -> String {
    crate::web::url_encode(s).replace('.', "%2e")
}

// ------------------------------------------------------------------------------------------
// CpuminerWyvern (cpuminer-gr, CPU)
// ------------------------------------------------------------------------------------------

fn cpuminer_wyvern(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("CpuminerWyvern", "CPU", &CMD_WYVERN);
    f.manual_uri = "https://github.com/WyvernTKC/cpuminer-gr-avx2/releases".into();
    f.port = "211".into();
    f.version = "1.2.4.1".into();
    let rules: &[(&str, &'static str)] = &[
        ("iszen3", "zen3"),
        ("iszen2", "zen2"),
        ("iszenplus", "zen"),
        ("iszen", "zen"),
        ("avx512 sha vaes", "avx512-sha-vaes"),
        ("avx512 sha", "avx512-sha"),
        ("avx512", "avx512"),
        ("avx2 sha vaes", "avx2-sha-vaes"),
        ("avx2 aes", "avx2"),
        ("avx aes", "avx"),
        ("sse42 aes", "aes-sse42"),
        ("sse42", "sse42"),
    ];
    if ctx.is_linux() {
        f.path = format!("Bin/CPU-Wyvern/cpuminer-{}", cpu_pick(ctx, rules, "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.4.1-wyvern/cpuminer-gr-1.2.4.1-x86_64_linux.7z".into();
    } else {
        f.path = format!("Bin/CPU-Wyvern/cpuminer-{}.exe", cpu_pick(ctx, rules, "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.4.1-wyvern/cpuminer-gr-1.2.4.1-x86_64_windows.7z".into();
    }
    f.suffix = Suffix::KeysNoGPU;
    f.use_algo_override = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.use_exclude_pool = true;
    f.emits_max_rej = true;
    f.cmd_ok = Some(|_, ctx, cmd| !cmd_b(cmd, "LinuxOnly") || ctx.is_linux());
    f.make_args = |a| {
        let (preset, tuning) = wyvern_tuning(a);
        format!(
            "-b $mport -a gr -o stratum+tcp{}://{}:{} -u {}{}{} {} {}",
            opt(a.pool.SSL, "s".into()),
            a.pool.Host,
            a.pool.Port,
            a.pool.User,
            a.pass_opt(" -p "),
            a.device_params,
            if preset || tuning { "--tune-config=tune_config" } else { "--no-tune" },
            a.params()
        )
    };
    f.post = Some(|a, m| {
        let (preset, tuning) = wyvern_tuning(a);
        // a configured tuning run without a tune_config yet: benchmark again
        if tuning && !preset {
            for v in m.hashrates.values_mut() {
                *v = None;
            }
        }
        if ps_match(&a.pool.Host, "FlockPool") {
            for v in m.dev_fee.values_mut() {
                *v = 1.5;
            }
        }
        m.max_rejected_share_ratio = cmd_f(a.cmd, "MaxRejectedShareRatio");
    });
    Some(f)
}

/// (tune_config present, $Session.Config.Miners."<Name>-CPU-<Algo>".Tuning)
fn wyvern_tuning(a: &A) -> (bool, bool) {
    let path = a.ctx.path(&a.fam.path);
    let preset = path.parent().map(|p| p.join("tune_config").exists()).unwrap_or(false);
    let tuning = a
        .ctx
        .config
        .miner(&format!("{}-CPU-{}", a.fam.name, a.algo0))
        .map(|m| jsonx::truthy(jsonx::get(&m.raw, "Tuning")))
        .unwrap_or(false);
    (preset, tuning)
}

// ------------------------------------------------------------------------------------------
// CryptoDredge, CryptoDredge25 (NVIDIA)
// ------------------------------------------------------------------------------------------

fn cryptodredge_args(a: &A) -> String {
    let algorithm = a.algorithm();
    let (proto, userpass) = if algorithm == "argon2d-nim" && ps_match(&a.pool.Host, "Icemining") {
        let re = regex::Regex::new(r"^([A-Z0-9]{4})\s*([A-Z0-9]{4})\s*([A-Z0-9]{4})\s*([A-Z0-9]{4})\s*([A-Z0-9]{4})\s*([A-Z0-9]{4})\s*([A-Z0-9]{4})\s*([A-Z0-9]{4})\s*([A-Z0-9]{4})$").unwrap();
        let mut user = a.pool.Wallet.clone();
        if let Some(c) = re.captures(&a.pool.Wallet) {
            user = (1..=9).map(|i| c[i].to_string()).collect::<Vec<_>>().join(" ");
        }
        ("wss".to_string(), format!("-u \"{}\" -p {}", user, a.pool.Worker))
    } else {
        (a.pool.Protocol.clone(), format!("-u {}{}", a.pool.User, a.pass_opt(" -p ")))
    };
    format!(
        "-r 10 -R 1 -b 127.0.0.1:$mport -d {} -a {} --no-watchdog -o {}://{}:{} {} --no-nvml {}",
        a.ids,
        algorithm,
        proto,
        a.pool.Host,
        a.pool_port,
        userpass,
        a.params()
    )
}

fn cryptodredge_base(name: &'static str, cmds: &'static LazyLock<Vec<Value>>) -> Family {
    let mut f = Family::new(name, "NVIDIA", cmds);
    f.manual_uri = "https://github.com/technobyl/CryptoDredge/releases".into();
    f.dev_fee = 1.0;
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.dev_fee_cmd = true;
    // $DeviceCapability = "5.0"
    f.model_filter = Some(|_, d| capability_ok(d, "5.0"));
    f.make_args = cryptodredge_args;
    f
}

fn cryptodredge(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) || ctx.is_vm {
        return None;
    }
    let mut f = cryptodredge_base("CryptoDredge", &CMD_CD);
    f.port = "313".into();
    f.version = "0.26.0".into();
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-CryptoDredge/CryptoDredge".into();
        f = f.cuda(&[("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.26.0-cryptodredge/CryptoDredge_0.26.0_cuda_11.2_linux.tar.gz", "11.2")]);
    } else {
        f.path = "Bin/NVIDIA-CryptoDredge/CryptoDredge.exe".into();
        f = f.cuda(&[("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.26.0-cryptodredge/CryptoDredge_0.26.0_cuda_11.2_windows.zip", "11.2")]);
    }
    f.cmd_ok = Some(|fam, _, cmd| {
        let v = cmd_s(cmd, "Version");
        v.is_empty() || crate::util::compare_version(&fam.version, &v, 0) >= 0
    });
    f.per_key_filter = Some(vram_filter);
    Some(f)
}

fn cryptodredge25(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) || ctx.is_vm {
        return None;
    }
    let mut f = cryptodredge_base("CryptoDredge25", &CMD_CD25);
    f.port = "363".into();
    f.version = "0.25.1".into();
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-CryptoDredge25/CryptoDredge".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.25.1-cryptodredge/CryptoDredge_0.25.1_cuda_10.2_linux.tar.gz", "10.2"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.25.1-cryptodredge/CryptoDredge_0.25.1_cuda_9.2_linux.tar.gz", "9.2"),
        ]);
    } else {
        f.path = "Bin/NVIDIA-CryptoDredge25/CryptoDredge.exe".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.25.1-cryptodredge/CryptoDredge_0.25.1_cuda_10.2_windows.zip", "10.2"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.25.1-cryptodredge/CryptoDredge_0.25.1_cuda_9.2_windows.zip", "9.2"),
        ]);
    }
    // $Miners_IsMaxCUDAVersion: only the legacy commands above CUDA 11.1
    f.cmd_ok = Some(|_, ctx, cmd| cmd_b(cmd, "Legacy") || crate::util::compare_version("11.1", &ctx.config.cuda_version, 0) >= 1);
    f.pre_key_filter = Some(|ctx, d, cmd| ctx.test_vram(d, min_mem(cmd)) && ["Other", "Pascal", "Turing"].contains(&d.OpenCL.Architecture.as_str()));
    Some(f)
}

// ------------------------------------------------------------------------------------------
// Dynexsolve (NVIDIA)
// ------------------------------------------------------------------------------------------

fn dynexsolve(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("Dynexsolve", "NVIDIA", &CMD_DYNEX);
    f.manual_uri = "https://github.com/dynexcoin/Dynex/releases/tag/DynexSolve".into();
    f.port = "352".into();
    f.version = "2.2.5".into();
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-Dynexsolve/dynexsolve".into();
        f = f.cuda(&[("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.2.5-dynexsolve/dynexsolve_ubuntu22_2.2.5.tar.xz", "11.2")]);
    } else {
        f.path = "Bin/NVIDIA-Dynexsolve/DynexSolveVS.225.exe".into();
        f = f.cuda(&[("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.2.5-dynexsolve/dynexsolve_windows2.2.5.7z", "11.2")]);
    }
    f.suffix = Suffix::Keys;
    f.use_exclude_pool = true;
    f.api = "DynexsolveWrapper";
    f.mining_priority = Some(2);
    f.emits_no_cpu = true;
    f.make_args = |a| {
        // all NVIDIA GPUs of the rig, the others than the model's are disabled
        let mut all_ids: Vec<usize> = a.ctx.devices.all_devices.iter().filter(|d| d.is_gpu() && d.Vendor == "NVIDIA").map(|d| d.Type_Vendor_Index).collect();
        all_ids.dedup();
        let params = if all_ids.len() == 1 {
            "-no-cpu -cpu-chips 0".to_string()
        } else {
            let mine: Vec<usize> = a.devices.iter().map(|d| d.Type_Vendor_Index).collect();
            let disable = all_ids.iter().filter(|i| !mine.contains(i)).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
            format!("-no-cpu -multi-gpu -disable-gpu {} -cpu-chips 0{}", disable, opt(a.devices.len() > 1, " -sync".into()))
        };
        format!(
            "-mining-address {} {} -stratum-url {} -stratum-port {} -stratum-password {}{} {}",
            a.pool.User,
            params,
            a.pool.Host,
            a.pool_port,
            a.pool.Pass,
            opt(!a.pool.Mallob.is_empty(), format!(" --mallob-endpoint {}", a.pool.Mallob)),
            a.params()
        )
    };
    f.post = Some(|_, m| m.list_devices = "-devices".into());
    Some(f)
}

// ------------------------------------------------------------------------------------------
// Ethminer, MeowPowMiner, Nsfminer, PhihashMiner (AMD, NVIDIA)
// ------------------------------------------------------------------------------------------

/// $Miner_Protocol of the ethminer modules, plus "auto" (the default case)
fn ethminer_protocol(p: &Pool) -> (String, bool) {
    let t = if p.SSL { "ssl" } else { "tcp" };
    match p.EthMode.as_str() {
        "stratum" => (format!("stratum+{}", t), false),
        "ethproxy" => (format!("stratum1+{}", t), false),
        "ethstratum2" | "ethstratumnh" => (format!("stratum2+{}", t), false),
        _ => (format!("stratum{}", if p.SSL { "s" } else { "" }), true),
    }
}

/// -P <proto>://<user>[:<pass>]@<host>:<port> (F2pool with an 0x wallet: port 8008)
fn ethminer_url(a: &A, f2pool_fix: bool) -> (String, bool) {
    let (proto, auto) = ethminer_protocol(a.pool);
    let mut port = a.pool_port;
    if f2pool_fix && ps_match(&a.pool.Host, "F2pool") && regex::Regex::new(r"(?i)^0x[0-9a-f]{40}").unwrap().is_match(&a.pool.User) {
        port = 8008;
    }
    let user = if a.pool.EthMode != "ethproxy" { enc_dot(&a.pool.User) } else { crate::web::url_encode(&a.pool.User) };
    (
        format!("{}://{}{}@{}:{}", proto, user, if a.pool.Pass.is_empty() { String::new() } else { format!(":{}", enc_dot(&a.pool.Pass)) }, a.pool.Host, port),
        auto,
    )
}

fn ethminer_base(ctx: &MinerCtx, info_only: bool, name: &'static str, cmds: &'static LazyLock<Vec<Value>>) -> Family {
    let mut f = Family::new(name, "AMD", cmds);
    f.info_types = vec!["AMD".into(), "NVIDIA".into()];
    f.suffix = Suffix::ListGPU;
    f.first_per_cmd = true;
    f.per_key_filter = Some(dag_filter);
    let _ = (ctx, info_only);
    f
}

/// the CUDA selection of Ethminer: without CUDA the first build (AMD only)
fn ethminer_cuda(f: &mut Family, ctx: &MinerCtx, info_only: bool) {
    let builds = std::mem::take(&mut f.uri_cuda);
    f.uri = builds.first().map(|b| b.uri.clone()).unwrap_or_default();
    let mut cuda = false;
    if !info_only && !ctx.config.cuda_version.is_empty() {
        let n = builds.len();
        for (i, b) in builds.iter().enumerate() {
            let warn = if i + 1 < n || !ctx.has_type("NVIDIA") { "" } else { f.name };
            if ctx.cuda_ok(&b.cuda, warn) {
                f.uri = b.uri.clone();
                if !b.version.is_empty() {
                    f.version = b.version.clone();
                }
                cuda = true;
                break;
            }
        }
    }
    if info_only {
        f.uri_cuda = builds;
    }
    f.vendors = if cuda { vec!["AMD", "NVIDIA"] } else { vec!["AMD"] };
}

fn ethminer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) || (ctx.is_linux() && ctx.libc_below("2.25")) {
        return None;
    }
    let mut f = ethminer_base(ctx, info_only, "Ethminer", &CMD_ETHMINER);
    f.version = "0.19.0-18".into();
    f.manual_uri = "https://github.com/ethereum-mining/ethminer/releases".into();
    f.port = "301".into();
    if ctx.is_linux() {
        f.path = "Bin/Ethash-Ethminer/ethminer".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.0-ethminer/ethminer-0.19.0-18-cuda11.6-linux-amd64.7z", "11.2"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.0-ethminer/ethminer-0.19.0-18-cuda10.2-linux-amd64.7z", "10.2"),
        ]);
    } else {
        f.path = "Bin/Ethash-Ethminer/ethminer.exe".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.0-ethminer/ethminer-0.19.0-18-cuda11.6-windows-vs2019-amd64.zip", "11.2"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.0-ethminer/ethminer-0.19.0-18-cuda10.0-windows-amd64.zip", "10.0"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.0-ethminer/ethminer-0.19.0-18-cuda9.1-windows-amd64.zip", "9.1"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.0-ethminer/ethminer-0.19.0-18-cuda8.0-windows-amd64.zip", "8.0"),
        ]);
    }
    ethminer_cuda(&mut f, ctx, info_only);
    f.api = "Ethminer";
    f.dev_id_prop = "BusId_Type_Vendor_Index";
    f.dev_id_join = " ";
    f.make_args = ethminer_args;
    f.post = Some(ethminer_post);
    Some(f)
}

fn meowpowminer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) || (ctx.is_linux() && ctx.libc_below("2.25")) {
        return None;
    }
    let mut f = ethminer_base(ctx, info_only, "MeowPowMiner", &CMD_MEOW);
    f.version = "2.0.0".into();
    f.manual_uri = "https://github.com/Meowcoin-Foundation/meowpowminer/releases".into();
    f.port = "336".into();
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-MeowPow/meowpowminer".into();
        f = f.cuda(&[("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.0.0-meowpowminer/meopowminer_2.0.0_linux.7z", "12.0")]);
    } else {
        f.path = "Bin/NVIDIA-MeowPow/meowpowminer.exe".into();
        f = f.cuda(&[("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.0.0-meowpowminer/meopowminer_2.0.0_win.7z", "12.0")]);
    }
    ethminer_cuda(&mut f, ctx, info_only);
    f.api = "Ethminer";
    f.dev_id_prop = "BusId_Type_Vendor_Index";
    f.dev_id_join = " ";
    f.make_args = ethminer_args;
    f.post = Some(ethminer_post);
    Some(f)
}

fn ethminer_args(a: &A) -> String {
    let dp = match a.vendor {
        "NVIDIA" => "--cuda --cuda-devices",
        "AMD" => "--opencl --opencl-devices",
        _ => "",
    };
    let (url, auto) = ethminer_url(a, true);
    format!(
        "--api-port $mport {} {} -P {} --HWMON 2{} --exit {}",
        dp,
        a.ids,
        url,
        opt(!auto, " --farm-recheck 3000 --farm-retries 20 --work-timeout 900 --response-timeout 180".into()),
        a.params()
    )
}

fn ethminer_post(a: &A, m: &mut MinerCandidate) {
    let mut env = vec![];
    if a.pool.SSL {
        env.push("SSL_NOVERIFY=1".to_string());
    }
    if a.vendor == "AMD" {
        env.push("GPU_FORCE_64BIT_PTR=0".to_string());
    }
    m.env_vars = env;
    m.dev_fee.values_mut().for_each(|v| *v = 0.0);
    m.list_devices = "--list-devices".into();
}

fn nsfminer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = ethminer_base(ctx, info_only, "Nsfminer", &CMD_NSF);
    f.version = "1.3.14".into();
    f.manual_uri = "https://github.com/no-fee-ethereum-mining/nsfminer/releases".into();
    f.port = "300".into();
    if ctx.is_linux() {
        f.path = "Bin/Ethash-Nsfminer/nsfminer".into();
        f = f.cuda_v(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.3.14-nsfminer/nsfminer_1.3.14-ubuntu_18.04-cuda_11.3-opencl.tgz", "11.3", ""),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.3.13-nsfminer/nsfminer_1.3.13-ubuntu_18.04-cuda_11.2-opencl.tgz", "11.2", "1.3.13"),
        ]);
    } else {
        f.path = "Bin/Ethash-Nsfminer/nsfminer.exe".into();
        f = f.cuda_v(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.3.14-nsfminer/nsfminer_1.3.14-windows_10-cuda_11.3-opencl.zip", "11.3", ""),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.3.13-nsfminer/nsfminer_1.3.13-windows_10-cuda_11.2-opencl.zip", "11.2", "1.3.13"),
        ]);
    }
    ethminer_cuda(&mut f, ctx, info_only);
    f.api = "Ethminer";
    f.dev_id_join = " ";
    f.make_args = |a| {
        let dp = match a.vendor {
            "NVIDIA" => "--cuda",
            "AMD" => "--opencl",
            _ => "",
        };
        let (url, auto) = ethminer_url(a, true);
        format!(
            "--api-port $mport {} --devices {} -P {} --HWMON 1{} --exit {}",
            dp,
            a.ids,
            url,
            opt(!auto, " --getwork-recheck 3000 --retry-max 20 --work-timeout 900 --response-timeout 180".into()),
            a.params()
        )
    };
    f.post = Some(|a, m| {
        m.env_vars = if a.vendor == "AMD" { vec!["GPU_FORCE_64BIT_PTR=0".into()] } else { vec![] };
        m.dev_fee.values_mut().for_each(|v| *v = 0.0);
    });
    Some(f)
}

fn phihashminer(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) || (ctx.is_linux() && ctx.libc_below("2.25")) {
        return None;
    }
    let mut f = Family::new("PhihashMiner", "AMD", &CMD_PHI);
    f.info_types = vec!["AMD".into(), "NVIDIA".into()];
    f.vendors = vec!["AMD", "NVIDIA"];
    f.version = "2.0.1".into();
    f.manual_uri = "https://github.com/PhicoinProject/phihashminer_v2/releases".into();
    f.port = "314".into();
    if ctx.is_linux() {
        f.path = "Bin/GPU-PhihashMiner/phihashminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2.0.1-phihashminer/phihashminer_v2.0.1_linux.zip".into();
    } else {
        f.path = "Bin/GPU-PhihashMiner/phihashminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2.0.1-phihashminer/phihashminer_v2.0.1_win.zip".into();
    }
    f.suffix = Suffix::ListGPU;
    f.first_per_cmd = true;
    f.per_key_filter = Some(dag_filter);
    f.api = "EthminerWrapper";
    f.dev_id_prop = "BusId_Type_Vendor_Index";
    f.dev_id_join = " ";
    f.make_args = |a| {
        let (url, auto) = ethminer_url(a, false);
        format!(
            "--api-port $mport --opencl --opencl-devices {} -P {} --HWMON 2{} --exit {}",
            a.ids,
            url,
            opt(!auto, " --farm-recheck 3000 --farm-retries 20 --work-timeout 900 --response-timeout 180".into()),
            a.params()
        )
    };
    f.post = Some(ethminer_post);
    Some(f)
}

// ------------------------------------------------------------------------------------------
// Multiminer (AMD, NVIDIA, Windows)
// ------------------------------------------------------------------------------------------

fn multiminer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("Multiminer", "AMD", &CMD_MULTI);
    f.info_types = vec!["AMD".into(), "NVIDIA".into()];
    f.path = "Bin/GPU-Multiminer/multiminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.0-multiminerrbm/multiminer-rbm-v1.2.0a-win64.7z".into();
    f.manual_uri = "https://github.com/RainbowMiner/multiminer/releases".into();
    f.port = "339".into();
    f.version = "1.2.0a".into();
    let cuda = !info_only && ctx.has_type("NVIDIA") && ctx.cuda_ok("10.2", "Multiminer");
    f.vendors = if cuda || !ctx.has_type("NVIDIA") { vec!["AMD", "NVIDIA"] } else { vec!["AMD"] };
    f.suffix = Suffix::ListGPU;
    f.first_per_cmd = true;
    f.pre_key_filter = Some(|ctx, d, cmd| cmd_vendor(cmd, &d.Vendor) && ctx.test_vram(d, min_mem(cmd)));
    f.ids_fn = Some(|vendor, devs| devs.iter().map(|d| (if vendor == "AMD" { d.Type_Index } else { d.Type_Vendor_Index }) + 1).map(|i| i.to_string()).collect::<Vec<_>>().join(","));
    f.make_args = |a| {
        let blocksize = 32.0 * jsonx::num(jsonx::get(a.cmd, "Blocksize")) / 0.865 / 1048576.0;
        let batch = a.devices.iter().map(|d| if blocksize > 0.0 { (d.OpenCL.GlobalMemSizeGB as f64 / blocksize).floor() } else { 0.0 }).fold(f64::INFINITY, f64::min);
        let batch = if batch.is_finite() { batch * 32.0 } else { 0.0 };
        format!(
            "-R 1 -b $mport -a {} -o {}://{}:{} -u {}{} --gpu-id={} --use-gpu={} -q --gpu-batchsize={} -t 1 {}",
            a.main(),
            a.pool.Protocol,
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            a.pass_opt(" -p "),
            a.ids,
            if a.vendor == "AMD" { "OpenCL" } else { "CUDA" },
            batch,
            a.params()
        )
    };
    Some(f)
}

// ------------------------------------------------------------------------------------------
// NinjaRig (AMD, CPU, NVIDIA)
// ------------------------------------------------------------------------------------------

fn ninjarig(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("NinjaRig", "AMD", &CMD_NINJA);
    f.info_types = vec!["AMD".into(), "CPU".into(), "NVIDIA".into()];
    if ctx.is_linux() {
        f.path = "Bin/ANY-NinjaRig/ninjarig".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.3-ninjarig/ninjarig-v1.0.3-linux.7z".into();
    } else {
        f.path = "Bin/ANY-NinjaRig/ninjarig.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.3-ninjarig/ninjarig-v1.0.3-win64.7z".into();
    }
    f.version = "1.0.3".into();
    f.manual_uri = "https://github.com/turtlecoin/ninjarig/releases".into();
    f.port = "348".into();
    f.dev_fee = 1.0;
    let cuda = !info_only && ctx.has_type("NVIDIA") && ctx.cuda_ok("10.1", "NinjaRig");
    f.vendors = if cuda || !ctx.has_type("NVIDIA") { vec!["AMD", "CPU", "NVIDIA"] } else { vec!["AMD", "CPU"] };
    f.suffix = Suffix::List;
    f.first_per_cmd = true;
    f.cpu_params = true;
    f.api = "XMRig";
    f.emits_no_cpu = true;
    // $MinMemGb is not set in the original: Test-VRAM with 0
    f.pre_key_filter = Some(|ctx, d, _| d.Model == "CPU" || ctx.test_vram(d, 0.0));
    f.ids_fn = Some(|_, devs| devs.iter().map(|d| format!("[{}]", d.Type_Vendor_Index + 1)).collect::<Vec<_>>().join(","));
    f.make_args = |a| {
        let cpu = a.vendor == "CPU";
        let global = match a.vendor {
            "CPU" => opt(a.ctx.cpu.has("avx2") && a.ctx.cpu.has("aes"), " --cpu-optimization AVX2".into()),
            "AMD" => format!(" -t 0 --use-gpu=OPENCL --gpu-filter={}", a.ids),
            "NVIDIA" => format!(" -t 0 --use-gpu=CUDA --gpu-filter={}", a.ids),
            _ => String::new(),
        };
        let port = if cpu { a.pool.port_cpu() } else { a.pool.port_gpu() };
        format!(
            "--api-port=$mport -a {} -o {}://{}:{} -u {}{}{}{}{}{} {} -c params.json --donate-level=1",
            a.main(),
            a.pool.Protocol,
            a.pool.Host,
            port,
            a.pool.User,
            a.pass_opt(" -p "),
            global,
            if cpu { a.device_params } else { "" },
            opt(ps_match(&a.pool.Host, "NiceHash"), " --nicehash".into()),
            opt(a.pool.SSL, " --tls".into()),
            a.params()
        )
    };
    f.post = Some(|a, m| {
        let p = cmd_f(a.cmd, "Penalty").unwrap_or(0.0);
        if p != 0.0 {
            for v in m.hashrates.values_mut() {
                *v = v.map(|h| h * (1.0 - p / 100.0));
            }
        }
    });
    Some(f)
}

// ------------------------------------------------------------------------------------------
// SgminerFancyIX (AMD, INTEL)
// ------------------------------------------------------------------------------------------

fn sgminer_fancyix(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("SgminerFancyIX", "AMD", &CMD_FANCY);
    f.info_types = vec!["AMD".into(), "INTEL".into()];
    f.vendors = vec!["AMD", "INTEL"];
    f.port = "409".into();
    f.manual_uri = "https://github.com/fancyIX/sgminer-phi2-branch/releases".into();
    f.version = "0.9.4".into();
    if ctx.is_linux() {
        f.path = "Bin/AMD-FancyIX/sgminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.9.4-fancyix/sgminer-fancyIX-linux-amd64-0.9.4.tar.gz".into();
    } else {
        f.path = "Bin/AMD-FancyIX/sgminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.9.4-fancyix/sgminer-fancyIX-win64-0.9.4.zip".into();
    }
    f.suffix = Suffix::Keys;
    f.use_exclude_pool = true;
    f.needs_platform_id = true;
    f.api = "Xgminer";
    f.dev_id_prop = "Type_PlatformId_Index";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    // the "<algo>_navi" kernels for Navi cards (gfx1010, RX5x00/RX6x00), the others for the rest.
    // The original compares with the device's (missing) MainAlgorithm and so selects the Navi
    // cards for every command; this is the intended device split.
    f.pre_key_filter = Some(|_, d, cmd| {
        let navi_cmd = cmd_s(cmd, "MainAlgorithm").ends_with("_navi");
        let navi_dev = d.Model.eq_ignore_ascii_case("gfx1010") || ps_match(&d.Model, r"^RX[56]\d00");
        navi_cmd == navi_dev
    });
    // $Algorithm_Norm_0 of "<algo>_navi" is the algorithm without the suffix
    f.algo_fn = Some(|cmd| cmd_s(cmd, "MainAlgorithm").trim_end_matches("_navi").to_string());
    f.make_args = |a| {
        format!(
            "--device {} --api-port $mport --api-listen -k {} -o {}://{}:{} -u {}{} --text-only --gpu-platform {} {}",
            a.ids,
            a.main(),
            a.pool.Protocol,
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            a.pass_opt(" -p "),
            a.platform_id,
            a.params()
        )
    };
    Some(f)
}

// ------------------------------------------------------------------------------------------
// VertHash (AMD, INTEL, NVIDIA)
// ------------------------------------------------------------------------------------------

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

fn verthash(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("VertHash", "AMD", &CMD_VERTHASH);
    f.info_types = vec!["AMD".into(), "INTEL".into(), "NVIDIA".into()];
    f.port = "364".into();
    f.manual_uri = "https://github.com/CryptoGraphics/VerthashMiner/releases".into();
    f.version = "0.7.2".into();
    if ctx.is_linux() {
        f.path = "Bin/GPU-Verthash/VerthashMiner".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.7.2-verthash/VerthashMiner-0.7.2-CUDA11-linux.tar.gz".into();
    } else {
        f.path = "Bin/GPU-Verthash/VerthashMiner.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.7.2-verthash/VerthashMiner-0.7.2-CUDA11-windows.zip".into();
    }
    let cuda = !info_only && ctx.has_type("NVIDIA") && ctx.cuda_ok("11.0", "VertHash");
    f.vendors = if cuda { vec!["AMD", "INTEL", "NVIDIA"] } else { vec!["AMD", "INTEL"] };
    f.suffix = Suffix::ListGPU;
    f.first_per_cmd = true;
    f.api = "VerthashWrapper";
    f.pre_key_filter = Some(|ctx, d, _| d.is_gpu() && ctx.test_vram(d, 0.0));
    f.ids_fn = Some(|vendor, devs| {
        devs.iter()
            .map(|d| if vendor == "NVIDIA" { d.Type_Vendor_Index } else { d.Type_Index })
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    });
    f.make_args = |a| {
        let dp = match a.vendor {
            "NVIDIA" => "--cu-devices",
            "AMD" | "INTEL" => "--cl-devices",
            _ => "",
        };
        format!(
            "{} {} -a {} -o stratum+tcp{}://{}:{} -u {} -p {} --verthash-data '{}' {}",
            dp,
            a.ids,
            a.main(),
            opt(a.pool.SSL, "s".into()),
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            a.pool.Pass,
            verthash_dat(a.ctx).display(),
            a.params()
        )
    };
    f.prerequisite = Some(|ctx, _| {
        Some((
            verthash_dat(ctx),
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0-verthash/verthash.dat".into(),
            "Downloading verthash.dat (1.2GB) in the background, please wait!".into(),
        ))
    });
    f.post = Some(|a, m| {
        m.env_vars = if a.vendor == "AMD" { vec!["GPU_FORCE_64BIT_PTR=0".into()] } else { vec![] };
        m.list_devices = "-l".into();
    });
    Some(f)
}

// ------------------------------------------------------------------------------------------
// Trex19, TrexGTX19 (NVIDIA, T-Rex 0.19.14 without dual mining)
// ------------------------------------------------------------------------------------------

cmds!(CMD_TREX19, "Trex19");
cmds!(CMD_TREXGTX19, "TrexGTX19");

pub fn modules_trex19() -> Vec<FamilyModule> {
    vec![
        FamilyModule { name: "Trex19", types: &["NVIDIA"], def: trex19 },
        FamilyModule { name: "TrexGTX19", types: &["NVIDIA"], def: trex_gtx19 },
        FamilyModule { name: "Xlarig", types: &["CPU", "ARMCPU"], def: xlarig },
    ]
}

/// (Uri, Cuda, IsGtx) newest first: the first build that fits the CUDA version
fn trex19_select(f: &mut Family, ctx: &MinerCtx, info_only: bool, builds: &[(&str, &str, bool)]) -> bool {
    f.uri = builds.first().map(|b| b.0.to_string()).unwrap_or_default();
    if info_only {
        return true;
    }
    for (i, (uri, cuda, gtx)) in builds.iter().enumerate() {
        if ctx.cuda_ok(cuda, if i + 1 < builds.len() { "" } else { f.name }) {
            f.uri = uri.to_string();
            f.vars.insert("IsGtx", if *gtx { "1".into() } else { String::new() });
            return true;
        }
    }
    false
}

fn trex19_base(name: &'static str, cmds: &'static LazyLock<Vec<Value>>) -> Family {
    let mut f = Family::new(name, "NVIDIA", cmds);
    f.manual_uri = "https://github.com/trexminer/T-Rex/releases".into();
    f.dev_fee = 1.0;
    f.version = "0.19.14".into();
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.api = "Trex";
    f.make_args = |a| {
        let p = a.pool;
        let proto = if a.algo0 == "Octopus" {
            p.Protocol.clone()
        } else {
            match p.EthMode.as_str() {
                "qtminer" => format!("stratum1+{}", if p.SSL { "ssl" } else { "tcp" }),
                "ethstratumnh" => format!("stratum2+{}", if p.SSL { "ssl" } else { "tcp" }),
                "ethlocalproxy" => "stratum+http".into(),
                _ => p.Protocol.clone(),
            }
        };
        format!(
            "-N 10 -r 5 --api-bind-http 127.0.0.1:$mport -d {} -a {} -o {}://{}:{} -u {}{}{}{}{}{} --watchdog-exit-mode 10:5:e -b 0 {}",
            a.ids,
            a.algorithm(),
            proto,
            p.Host,
            a.pool_port,
            p.User,
            opt(!p.Wallet.is_empty() && !p.Worker.is_empty(), format!(" -w {}", p.Worker)),
            a.pass_opt(" -p "),
            super::failover_args(p),
            opt(p.SSL, " --no-strict-ssl".into()),
            opt(!a.ctx.config.b("ShowMinerWindow"), " --no-color".into()),
            a.params()
        )
    };
    f.post = Some(|a, m| {
        let fee = cmd_f(a.cmd, "DevFee").filter(|x| *x != 0.0).unwrap_or(a.fam.dev_fee);
        m.dev_fee.values_mut().for_each(|v| *v = fee);
    });
    f
}

/// $MinMemGB with the zombie mode of the Ethash algorithms
fn trex19_min_mem(ctx: &MinerCtx, cmd: &Value, pool: &Pool, algo0: &str) -> f64 {
    let mut min = ctx.min_mem_gb(cmd, pool, algo0);
    let cmd_min = min_mem(cmd);
    if cmd_b(cmd, "DAG") && crate::data::RE_ALGO_IS_ETHASH.is_match(algo0) && min > cmd_min && ctx.config.b("EnableEthashZombieMode") {
        min = cmd_min;
    }
    min
}

fn trex19(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = trex19_base("Trex19", &CMD_TREX19);
    f.port = "319".into();
    let builds: &[(&str, &str, bool)] = if ctx.is_linux() {
        f.path = "Bin/NVIDIA-Trex19/t-rex".into();
        &[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.14-trex/t-rex-0.19.14-linux-cuda11.1.tar.gz", "11.1", false),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.14-trex/t-rex-0.19.14-linux-cuda10.0.tar.gz", "10.0", true),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.14-trex/t-rex-0.19.14-linux-cuda9.2.tar.gz", "9.2", true),
        ]
    } else {
        f.path = "Bin/NVIDIA-Trex19/t-rex.exe".into();
        &[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.14-trex/t-rex-0.19.14-win-cuda11.1.zip", "11.1", false),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.14-trex/t-rex-0.19.14-win-cuda10.0.zip", "10.0", true),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.14-trex/t-rex-0.19.14-win-cuda9.2.zip", "9.2", true),
        ]
    };
    if !trex19_select(&mut f, ctx, info_only, builds) {
        return None;
    }
    // $DeviceCapability = "5.0"
    f.model_filter = Some(|_, d| capability_ok(d, "5.0"));
    f.per_key_filter = Some(|fam, ctx, d, cmd, pool, algo0| {
        let Some(pool) = pool else { return false };
        let gtx_build = fam.vars.get("IsGtx").map(|s| !s.is_empty()).unwrap_or(false);
        let gtx_arch = ["Other", "Pascal", "Turing"].contains(&d.OpenCL.Architecture.as_str());
        (gtx_build || !cmd_b(cmd, "IsGtx") || !gtx_arch) && ctx.test_vram(d, trex19_min_mem(ctx, cmd, pool, algo0))
    });
    Some(f)
}

fn trex_gtx19(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = trex19_base("TrexGTX19", &CMD_TREXGTX19);
    f.port = "327".into();
    // the CUDA 10 build, if CUDA 11.1 is installed
    let builds: &[(&str, &str, bool)] = if ctx.is_linux() {
        f.path = "Bin/NVIDIA-TrexGTX19/t-rex".into();
        &[("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.14-trex/t-rex-0.19.14-linux-cuda10.0.tar.gz", "11.1", false)]
    } else {
        f.path = "Bin/NVIDIA-TrexGTX19/t-rex.exe".into();
        &[("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.19.14-trex/t-rex-0.19.14-win-cuda10.0.zip", "11.1", false)]
    };
    if !trex19_select(&mut f, ctx, info_only, builds) {
        return None;
    }
    f.model_filter = Some(|_, d| ["Other", "Pascal", "Turing"].contains(&d.OpenCL.Architecture.as_str()) && capability_ok(d, "5.0"));
    f.per_key_filter = Some(|_, ctx, d, cmd, pool, algo0| pool.map(|p| ctx.test_vram(d, trex19_min_mem(ctx, cmd, p, algo0))).unwrap_or(false));
    Some(f)
}

// ------------------------------------------------------------------------------------------
// Xlarig (CPU, XMRig6 API)
// ------------------------------------------------------------------------------------------

cmds!(CMD_XLARIG, "Xlarig");

fn xlarig(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("Xlarig", "CPU", &CMD_XLARIG);
    f.info_types = vec!["CPU".into(), "ARMCPU".into()];
    f.manual_uri = "https://github.com/scala-network/XLArig/releases".into();
    f.port = "241".into();
    f.version = "5.2.4".into();
    if ctx.is_linux() {
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v5.2.4-xlarig/xlarig-5.2.4-bionic-x64.7z".into();
        f.path = "Bin/CPU-Xlarig/xlarig".into();
    } else {
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v5.2.4-xlarig/xlarig-5.2.4-win64.7z".into();
        f.path = "Bin/CPU-Xlarig/xlarig.exe".into();
    }
    f.suffix = Suffix::ListCPU;
    f.api = "XMRig6";
    f.post = Some(|a, m| {
        let algorithm = a.algorithm();
        let re1 = regex::Regex::new(r"(?i)(\(R\)|\(TM\)|CPU|Processor)").unwrap();
        let re2 = regex::Regex::new(r"(?i)[^A-Z0-9]").unwrap();
        let hwsig = format!("{}x{}", a.ctx.by_type("CPU").len(), re2.replace_all(&re1.replace_all(&a.ctx.cpu.Name, ""), ""));
        let p = a.pool;
        m.arguments = MinerArgs::Obj(serde_json::json!({
            "Algorithm": algorithm,
            "APIParams": "--http-enabled --http-host=127.0.0.1 --http-port=$mport",
            "Config": {
                "api": {"id": null, "worker-id": null},
                "background": false,
                "colors": true,
                "donate-level": 0,
                "log-file": null,
                "print-time": 5,
                "retries": 5,
                "retry-pause": 1
            },
            "Pools": [{
                "algo": algorithm,
                "coin": null,
                "url": format!("{}://{}:{}", p.Protocol, p.Host, p.Port),
                "user": p.User,
                "pass": if p.Pass.is_empty() { "x".to_string() } else { p.Pass.clone() },
                "nicehash": ps_match(&p.Host, "NiceHash"),
                "keepalive": true,
                "enabled": true,
                "tls": p.SSL,
                "ip_version": 4
            }],
            "Params": a.params(),
            "HwSig": hwsig,
            "Threads": a.ctx.cpu_threads(a.fam.name, a.algo0).unwrap_or(a.ctx.cpu.Threads as i64),
            "Affinity": a.ctx.cpu_affinity(a.fam.name, a.algo0),
        }));
    });
    Some(f)
}

// ------------------------------------------------------------------------------------------
// Teamblack (TeamBlackMiner, AMD, INTEL, NVIDIA)
// ------------------------------------------------------------------------------------------

cmds!(CMD_TEAMBLACK, "Teamblack");

pub fn modules_teamblack() -> Vec<FamilyModule> {
    vec![FamilyModule { name: "Teamblack", types: &["AMD", "INTEL", "NVIDIA"], def: teamblack }]
}

fn teamblack(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("Teamblack", "AMD", &CMD_TEAMBLACK);
    f.info_types = vec!["AMD".into(), "INTEL".into(), "NVIDIA".into()];
    f.manual_uri = "https://github.com/sp-hash/TeamBlackMiner".into();
    f.port = "365".into();
    f.version = "2.46".into();
    let (uri, cuda) = if ctx.is_linux() {
        f.path = "Bin/GPU-Teamblack/TBMiner".into();
        ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.46-teamblack/TeamBlackMiner_2_46_Ubuntu_22_04_Cuda_12_9.tar.gz", "12.9")
    } else {
        f.path = "Bin/GPU-Teamblack/TBMiner.exe".into();
        ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.46-teamblack/TeamBlackMiner_2_46_cuda_12.9.7z", "12.9")
    };
    f.uri = uri.into();
    let cuda_ok = !info_only && !ctx.config.cuda_version.is_empty() && ctx.cuda_ok(cuda, if ctx.has_type("NVIDIA") { "Teamblack" } else { "" });
    f.vendors = if cuda_ok { vec!["AMD", "INTEL", "NVIDIA"] } else { vec!["AMD", "INTEL"] };
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.api = "TBMiner";
    f.cmd_ok = Some(|_, ctx, cmd| ctx.is_linux() || jsonx::get(cmd, "Xintensity").is_null());
    f.per_key_filter = Some(|_, ctx, d, cmd, pool, algo0| {
        let Some(pool) = pool else { return false };
        cmd_vendor(cmd, &d.Vendor) && ctx.test_vram(d, ctx.min_mem_gb(cmd, pool, algo0))
    });
    // --cuda-devices for NVIDIA (Type_Vendor_Index), --cl-devices for the others (Type_Index)
    f.ids_fn = Some(|vendor, devs| {
        devs.iter()
            .map(|d| if vendor == "NVIDIA" { d.Type_Vendor_Index } else { d.Type_Index })
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    });
    f.make_args = |a| {
        let p = a.pool;
        let wallet = if p.Wallet.is_empty() { &p.User } else { &p.Wallet };
        let devices = match a.vendor {
            "NVIDIA" => format!("--cuda-devices [{}]", a.ids),
            "AMD" => format!("--amd-only --cl-devices [{}]", a.ids),
            _ => format!("--cl-devices [{}]", a.ids),
        };
        let xint = cmd_f(a.cmd, "Xintensity").unwrap_or(-1.0);
        let mut dat = verthash_dat(a.ctx).display().to_string();
        if dat.contains(' ') {
            dat = format!("\"{}\"", dat);
        }
        format!(
            "--algo {}{} --hostname {} {} {} --wallet {} --worker-name {}{} {}{}{} --api --api-port $mport --no-ansi --no-cpu {}",
            a.algorithm(),
            opt(p.SSL, " --ssl --ssl-verify-none".into()),
            p.Host,
            if p.SSL { "--ssl-port" } else { "--port" },
            a.pool_port,
            wallet,
            p.Worker,
            opt(!p.Pass.is_empty(), format!(" --server-passwd {}", p.Pass)),
            devices,
            opt(a.main() == "verthash", format!(" --verthash-data {}", dat)),
            opt(a.vendor == "NVIDIA" && xint >= 1.0, format!(" --xintensity {}", xint)),
            a.params()
        )
    };
    f.post = Some(|a, m| {
        if a.algo0 == "PearlHash" {
            m.api = "TeamblackWrapper".into();
        }
        let fee = cmd_f(a.cmd, "DevFee").unwrap_or(0.0);
        m.dev_fee.values_mut().for_each(|v| *v = fee);
        // the original sets the verthash.dat prerequisite on every entry
        m.prerequisite_path = Some(verthash_dat(a.ctx));
        m.prerequisite_uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0-verthash/verthash.dat".into();
        m.prerequisite_msg = "Downloading verthash.dat (1.2GB) in the background, please wait!".into();
    });
    Some(f)
}

// ------------------------------------------------------------------------------------------
// MiniZ, Nanominer, Phoenix, SrbMiner, TTminer: no dual command in their tables, the dual
// branches of the originals are not reachable and left out
// ------------------------------------------------------------------------------------------

cmds!(CMD_MINIZ, "MiniZ");
cmds!(CMD_NANO, "Nanominer");
cmds!(CMD_PHOENIX, "Phoenix");
cmds!(CMD_SRBMINER, "SrbMiner");
cmds!(CMD_TTMINER, "TTminer");

pub fn modules_more() -> Vec<FamilyModule> {
    vec![
        FamilyModule { name: "MiniZ", types: &["AMD", "NVIDIA"], def: miniz },
        FamilyModule { name: "Nanominer", types: &["AMD", "CPU", "INTEL", "NVIDIA"], def: nanominer },
        FamilyModule { name: "Phoenix", types: &["AMD", "NVIDIA"], def: phoenix },
        FamilyModule { name: "SrbMiner", types: &["AMD"], def: srbminer },
        FamilyModule { name: "TTminer", types: &["AMD", "NVIDIA"], def: ttminer },
    ]
}

fn miniz(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("MiniZ", "AMD", &CMD_MINIZ);
    f.info_types = vec!["AMD".into(), "NVIDIA".into()];
    f.manual_uri = "https://bitcointalk.org/index.php?topic=4767892.0".into();
    f.port = "330".into();
    f.dev_fee = 2.0;
    f.version = "2.5e3".into();
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-MiniZ/miniZ".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2.5e3-miniz/miniZ_v2.5e3_linux-x64.tar.gz".into();
    } else {
        f.path = "Bin/NVIDIA-MiniZ/miniZ.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2.5e3-miniz/miniZ_v2.5e3_win-x64.7z".into();
    }
    let cuda = !info_only && ctx.has_type("NVIDIA") && ctx.cuda_ok("8.0", "MiniZ");
    f.vendors = if cuda { vec!["AMD", "NVIDIA"] } else { vec!["AMD"] };
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.api = "MiniZ";
    f.ids_fn = Some(|vendor, devs| device_ids(devs, if vendor == "NVIDIA" { "Type_Vendor_Index" } else { "BusId_Type_Vendor_Index" }, " ", false));
    f.per_key_filter = Some(|_, ctx, d, cmd, pool, algo0| {
        let Some(pool) = pool else { return false };
        if !cmd_vendor(cmd, &d.Vendor) || pool.User.contains('@') || !ctx.test_vram(d, ctx.min_mem_gb(cmd, pool, algo0)) {
            return false;
        }
        if d.Vendor == "AMD" {
            let cap = d.OpenCL.DeviceCapability.clone().unwrap_or_default();
            let ex = cmd_list(cmd, "ExcludeCompute");
            let inc = cmd_list(cmd, "Compute");
            if (!ex.is_empty() && crate::util::contains_ci(&ex, &cap)) || (!inc.is_empty() && !crate::util::contains_ci(&inc, &cap)) {
                return false;
            }
        }
        true
    });
    f.make_args = |a| {
        let p = a.pool;
        let pers = crate::data::get_equihash_coin_pers(&p.CoinSymbol, "auto");
        let mut stratum = vec![];
        if p.SSL {
            stratum.push("ssl");
        }
        if ps_match(&p.Host, "miningrigrentals") && ps_match(a.algo0, "^etc?hash") {
            stratum.push("stratum2");
        }
        format!(
            "--{} --telemetry=$mport --nonvml -cd {} --url={}{}@{}:{}{}{}{} --gpu-line --minimal --extra --latency{}{} {} --nohttpheaders",
            a.vendor.to_lowercase(),
            a.ids,
            opt(!stratum.is_empty(), format!("{}://", stratum.join("+"))),
            p.User.replace('@', "%40"),
            p.Host,
            a.pool_port,
            a.pass_opt(" -p "),
            opt(!p.Worker.is_empty() && p.User == p.Wallet, format!(" --worker={}", p.Worker)),
            opt(!pers.is_empty() && (cmd_b(a.cmd, "AutoPers") || pers != "auto"), format!(" --pers={}", pers)),
            opt(!a.ctx.config.b("ShowMinerWindow"), " --nocolor".into()),
            opt(!ps_match(&p.Host, "xxxMiningRigRentals") && pers != "auto", " --smart-pers".into()),
            a.params()
        )
    };
    f.post = Some(|a, m| {
        let fee = cmd_f(a.cmd, "Fee").unwrap_or(0.0);
        m.dev_fee.values_mut().for_each(|v| *v = fee);
        m.skip_seconds = Some(40);
    });
    Some(f)
}

fn nanominer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("Nanominer", "AMD", &CMD_NANO);
    f.info_types = vec!["AMD".into(), "CPU".into(), "INTEL".into(), "NVIDIA".into()];
    f.manual_uri = "https://github.com/nanopool/nanominer/releases".into();
    f.port = "234".into();
    f.dev_fee = 3.0;
    f.version = "3.10.0".into();
    if ctx.is_linux() {
        f.path = "Bin/ANY-Nanominer/nanominer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.10.0-nanominer/nanominer-linux-3.10.0.tar.gz".into();
    } else {
        f.path = "Bin/ANY-Nanominer/nanominer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.10.0-nanominer/nanominer-windows-3.10.0.zip".into();
    }
    let has_nv = ctx.has_type("NVIDIA");
    let cuda = !info_only && has_nv && ctx.cuda_ok("10.0", "Nanominer");
    f.vendors = if cuda || !has_nv { vec!["AMD", "CPU", "INTEL", "NVIDIA"] } else { vec!["AMD", "CPU", "INTEL"] };
    f.vars.insert("Cuda11", if has_nv && ctx.cuda_ok("11.0", "") { "1".into() } else { String::new() });
    f.suffix = Suffix::KeysCoin;
    f.exclude_yiimp = true;
    f.first_per_cmd = true;
    f.cpu_params = true;
    f.use_exclude_pool = true;
    f.api = "Nanominer";
    f.algo_fn = Some(|cmd| {
        let m = cmd_s(cmd, "MainAlgorithm");
        if m == "heavyhash" {
            "kHeavyHash".into()
        } else {
            m
        }
    });
    // the CPU features of a command must all be present
    f.cmd_ok = Some(|_, ctx, cmd| {
        let feats = cmd_list(cmd, "CPUFeatures");
        feats.is_empty() || feats.iter().all(|x| ctx.cpu.has(x))
    });
    f.per_key_filter = Some(|fam, ctx, d, cmd, pool, algo0| {
        let Some(pool) = pool else { return false };
        if !cmd_vendor(cmd, if d.is_cpu() { "CPU" } else { &d.Vendor }) {
            return false;
        }
        let coins = cmd_list(cmd, "Coins");
        if (!coins.is_empty() && (pool.CoinSymbol.is_empty() || !crate::util::contains_ci(&coins, &pool.CoinSymbol))) || (pool.SSL && pool.SSLSelfSigned) {
            return false;
        }
        if d.is_cpu() {
            return true;
        }
        let mut min = ctx.min_mem_gb(cmd, pool, algo0);
        let cmd_min = min_mem(cmd);
        if cmd_b(cmd, "ZombieMode") && !cmd_b(cmd, "NoMemCalcCheck") && min > cmd_min && ctx.config.b("EnableEthashZombieMode") {
            min = cmd_min;
        }
        let cuda11 = fam.vars.get("Cuda11").map(|s| !s.is_empty()).unwrap_or(false);
        (algo0 != "Cuckaroo30" || d.Model == "RX57016GB") && (d.Vendor != "NVIDIA" || cuda11 || !ps_match(&d.Model, "^RTX[345]0")) && ctx.test_vram(d, min)
    });
    f.post = Some(|a, m| {
        let p = a.pool;
        let cpu = a.vendor == "CPU";
        let wallet = if !p.Wallet.is_empty() {
            p.Wallet.clone()
        } else {
            regex::Regex::new(r"(?i)[\./]\{workername[^\}]*\}$").unwrap().replace(&p.User, "").to_string()
        };
        // $MainCoin_Data.Algo -ne $MainAlgorithm_Norm: compared with the pool key
        let mut coin = Value::String(p.CoinSymbol.clone());
        let coin_algo = crate::data::get_coin(&p.CoinSymbol, "").map(|c| c.algo).unwrap_or_default();
        if p.CoinSymbol.is_empty() || (coin_algo != a.key && p.CoinSymbol != "SCC") {
            coin = Value::Null;
        }
        let lhr = if a.vendor == "NVIDIA" && ps_match(a.key, "^Etc?hash") {
            a.devices.iter().map(|d| if d.IsLHR { "0" } else { "off" }).collect::<Vec<_>>().join(",")
        } else {
            String::new()
        };
        m.arguments = MinerArgs::Obj(serde_json::json!({
            "Algorithms": [{
                "Algo": a.algorithm(),
                "Coin": coin,
                "Host": p.Host,
                "Port": if cpu { p.Port } else { p.port_gpu() },
                "SSL": p.SSL,
                "Wallet": wallet,
                "PaymentId": null,
                "Worker": p.Worker,
                "Pass": p.Pass,
                "Email": p.Email,
            }],
            "Devices": if cpu { Value::Null } else { serde_json::json!(a.devices.iter().map(|d| d.BusId_Type_Mineable_Index).collect::<Vec<_>>()) },
            "Threads": if cpu { serde_json::json!(a.threads) } else { Value::Null },
            "LHR": lhr,
            "Params": a.params(),
        }));
        let fee = cmd_f(a.cmd, "DevFee").unwrap_or(0.0);
        m.dev_fee.values_mut().for_each(|v| *v = fee);
        m.mining_affinity = if cpu { a.affinity.map(|s| s.to_string()) } else { None };
    });
    Some(f)
}

fn phoenix(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("Phoenix", "AMD", &CMD_PHOENIX);
    f.info_types = vec!["AMD".into(), "NVIDIA".into()];
    f.manual_uri = "https://bitcointalk.org/index.php?topic=2647654.0".into();
    f.port = "308".into();
    f.dev_fee = 0.65;
    f.version = "6.2c".into();
    if ctx.is_linux() {
        f.path = "Bin/GPU-Phoenix/PhoenixMiner".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v6.2c-phoenix/PhoenixMiner_6.2c_Linux.tar.gz".into();
    } else {
        f.path = "Bin/GPU-Phoenix/PhoenixMiner.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v6.2c-phoenix/PhoenixMiner_6.2c_Windows.7z".into();
    }
    let cuda = !info_only && ctx.has_type("NVIDIA") && ctx.cuda_ok("8.0", "Phoenix");
    f.vendors = if cuda || !ctx.has_type("NVIDIA") { vec!["AMD", "NVIDIA"] } else { vec!["AMD"] };
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.api = "Claymore";
    f.extend_default = 2;
    f.dev_id_prop = "BusId_Type_Vendor_Index";
    f.per_key_filter = Some(|_, ctx, d, cmd, pool, algo0| pool.map(|p| cmd_vendor(cmd, &d.Vendor) && ctx.test_vram(d, dag_size(p, algo0, "", min_mem(cmd)))).unwrap_or(false));
    f.make_args = |a| {
        let p = a.pool;
        let proto = match p.EthMode.as_str() {
            "minerproxy" => "-proto 1",
            "ethproxy" => "-proto 2",
            "qtminer" => "-proto 3",
            "ethstratum" | "ethstratum1" | "ethstratumnh" => "-proto 4",
            "ethstratum2" => "-proto 5",
            _ => "-proto 1",
        };
        let mut port = a.pool_port;
        if ps_match(&p.Host, "F2pool") && regex::Regex::new(r"(?i)^0x[0-9a-f]{40}").unwrap().is_match(&p.User) {
            port = 8008;
        }
        let xlat = [
            "ATH", "B2G", "BCI", "CLO", "DBIX", "EGEM", "ELLA", "ESN", "ETC", "ETH", "ETHO", "ETHW", "ETP", "EXP", "MIX", "MOAC", "MUSIC", "NUKO", "PGC", "PIRL", "QKC", "REOSC", "UBQ", "VIC", "WHL", "YOC",
        ];
        let coin = if ps_match(a.key, "ProgPow") {
            "bci".to_string()
        } else if let Some(c) = xlat.iter().find(|c| c.eq_ignore_ascii_case(&p.CoinSymbol)) {
            if *c == "WHL" {
                "whale".into()
            } else {
                c.to_lowercase()
            }
        } else if a.algo0 == "EtcHash" {
            "etc".into()
        } else if a.algo0 == "UbqHash" {
            "ubq".into()
        } else {
            "auto".into()
        };
        let wal = if !p.Wallet.is_empty() && !ps_match(&p.Host, "gteh|miningrigrentals|nicehash") {
            format!("-wal {} -worker {}", p.Wallet, p.Worker)
        } else {
            format!("-wal {}", p.User)
        };
        let dp = match a.vendor {
            "NVIDIA" => "-nvidia -nvdo 1",
            "AMD" => "-amd",
            _ => "",
        };
        format!(
            "-cdmport $mport -coin {} -di {} -pool {}{}:{} {}{} -stales {}{} {} {} -allpools 0 -cdm 1 -leaveoc -log 0 -rmode 0 -wdog 1 -gbase 0 {}",
            coin,
            a.ids,
            opt(p.SSL, "ssl://".into()),
            p.Host,
            port,
            wal,
            a.pass_opt(" -pass "),
            if ps_match(&p.Host, "Nicehash") { 0 } else { 1 },
            opt(p.SSL && p.SSLSelfSigned, " -weakssl".into()),
            proto,
            dp,
            a.params()
        )
    };
    f.post = Some(|_, m| {
        m.extend_interval = 2;
        m.start_remove = vec!["*pools.txt".into()];
    });
    Some(f)
}

fn srbminer(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("SrbMiner", "AMD", &CMD_SRBMINER);
    f.path = "Bin/CryptoNight-SRBMiner/srbminer-cn.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.9.4-srbminer/SRBMiner-CN-V1-9-4.zip".into();
    f.manual_uri = "https://bitcointalk.org/index.php?topic=3167363.0".into();
    f.port = "315".into();
    f.dev_fee = 0.85;
    f.version = "1.9.4".into();
    f.suffix = Suffix::ListGPU;
    f.first_per_cmd = true;
    f.api = "SrbMiner";
    f.env_vars = vec!["GPU_MAX_SINGLE_ALLOC_PERCENT=100".into(), "GPU_FORCE_64BIT_PTR=0".into()];
    f.algo_fn = Some(|cmd| format!("cryptonight{}", cmd_s(cmd, "MainAlgorithm")));
    f.pre_key_filter = Some(|ctx, d, cmd| ctx.test_vram(d, min_mem(cmd)));
    f.post = Some(|a, m| {
        let p = a.pool;
        let threads = jsonx::num(jsonx::get(a.cmd, "Threads")) as i64;
        // $Miner_Name: <Name>-<Threads>-<devices>
        if threads > 0 {
            let mut parts = vec![a.fam.name.to_string(), threads.to_string()];
            let mut names: Vec<String> = a.devices.iter().map(|d| d.Name.clone()).collect();
            names.sort();
            parts.extend(names);
            m.name = parts.join("-");
            let algo0 = a.algo0.to_string();
            m.hashrates.values_mut().for_each(|v| *v = a.ctx.hashrate(&m.name, &algo0));
        }
        let rig = a.ctx.config.pool(&p.Name).map(|c| c.worker.clone()).unwrap_or_default();
        m.arguments = MinerArgs::Obj(serde_json::json!({
            "Config": {
                "cryptonight_type": a.main(),
                "intensity": 0,
                "double_threads": false,
                "timeout": 10,
                "retry_time": 10,
                "gpu_conf": a.devices.iter().map(|d| serde_json::json!({"id": d.BusId_Type_Vendor_Index, "intensity": 0, "threads": threads, "platform": "OpenCL"})).collect::<Vec<_>>(),
            },
            "Pools": {"pools": [{
                "pool": format!("{}:{}", p.Host, a.pool_port),
                "wallet": p.User,
                "password": p.Pass,
                "pool_use_tls": p.SSL,
                "nicehash": ps_match(&p.Host, "NiceHash"),
            }]},
            "Params": format!("--apienable --apiport $mport --apirigname {} --disabletweaking --disablegpuwatchdog --enablecoinforking --maxnosharesent 120 {}", rig, a.params()).trim(),
        }));
    });
    Some(f)
}

/// free space (GB) of the disk the path is on
fn disk_free_gb(path: &std::path::Path) -> Option<f64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let p = std::fs::canonicalize(path).ok()?;
    disks.iter().filter(|d| p.starts_with(d.mount_point())).max_by_key(|d| d.mount_point().as_os_str().len()).map(|d| d.available_space() as f64 / 1073741824.0)
}

fn ttminer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("TTminer", "AMD", &CMD_TTMINER);
    f.info_types = vec!["AMD".into(), "NVIDIA".into()];
    f.manual_uri = "https://bitcointalk.org/index.php?topic=5025783.0".into();
    f.port = "333".into();
    f.dev_fee = 1.0;
    f.version = "2024.3.2".into();
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-TTminer/TT-Miner".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2024.3.2-ttminer/TT-Miner-2024.3.2.tar.gz".into();
    } else {
        f.path = "Bin/NVIDIA-TTminer/TT-Miner.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2024.3.2-ttminer/TT-Miner-2024.3.2.zip".into();
    }
    // without a CUDA version $Cuda keeps the (truthy) required version
    let cuda = ctx.config.cuda_version.is_empty() || (!info_only && ctx.cuda_ok("11.8", "TTminer"));
    f.vendors = if cuda { vec!["AMD", "NVIDIA"] } else { vec!["AMD"] };
    let nvmax = if ctx.config.cuda_version.is_empty() { 0 } else { ctx.devices.all_devices.iter().filter(|d| d.is_gpu() && d.Codec == "CUDA").count() };
    f.vars.insert("NVmax", nvmax.to_string());
    let mut save_dag = ctx.config.b("EnableMinersToSaveDAG");
    if save_dag {
        let dag_dir = ctx.path(&f.path).parent().map(|p| p.join("DAGs"));
        let used = dag_dir
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|rd| rd.flatten().filter(|e| e.path().extension().map(|x| x == "dag").unwrap_or(false)).filter_map(|e| e.metadata().ok()).map(|m| m.len() as f64).sum::<f64>() / 1073741824.0)
            .unwrap_or(0.0);
        if let Some(free) = disk_free_gb(ctx.base) {
            if free < 100.0 - (used * 10.0).round() / 10.0 {
                save_dag = false;
            }
        }
    }
    f.vars.insert("SaveDAG", if save_dag { "1".into() } else { String::new() });
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.api = "TTminerWrapper";
    f.per_key_filter = Some(|_, ctx, d, cmd, pool, algo0| {
        let Some(pool) = pool else { return false };
        let coins = cmd_list(cmd, "Coins");
        let ex = cmd_list(cmd, "ExcludeCoins");
        cmd_vendor(cmd, &d.Vendor)
            && (coins.is_empty() || crate::util::contains_ci(&coins, &pool.CoinSymbol))
            && (ex.is_empty() || !crate::util::contains_ci(&ex, &pool.CoinSymbol))
            && ctx.test_vram(d, ctx.min_mem_gb(cmd, pool, algo0))
    });
    f.ids_fn = Some(|vendor, devs| {
        if vendor == "NVIDIA" {
            format!("-d {} -cuda-order", devs.iter().map(|d| d.Type_Vendor_Index.to_string()).collect::<Vec<_>>().join(" "))
        } else {
            // the AMD cards are counted after the CUDA devices: $NVmax added in make_args
            format!("-d {}", devs.iter().map(|d| d.BusId_Type_Codec_Index.to_string()).collect::<Vec<_>>().join(" "))
        }
    });
    f.make_args = |a| {
        let p = a.pool;
        let ids = if a.vendor == "NVIDIA" {
            a.ids.to_string()
        } else {
            let nvmax: usize = a.var("NVmax").parse().unwrap_or(0);
            format!(
                "-d {}",
                a.ids.trim_start_matches("-d ").split_whitespace().map(|i| (i.parse::<usize>().unwrap_or(0) + nvmax).to_string()).collect::<Vec<_>>().join(" ")
            )
        };
        let ssl_plus = if p.SSL { "+ssl" } else { "" };
        let mut proto = if p.CoinSymbol == "EPIC" {
            format!("epic{}://", ssl_plus)
        } else if p.EthMode == "ethproxy" && (!ps_match(&p.Host, "MiningRigRentals") || a.algo0 != "ProgPow") {
            format!("stratum1{}://", ssl_plus)
        } else {
            opt(p.SSL, "ssl://".into())
        };
        if a.algo0 == "SHA3Solidity" {
            proto.clear();
        }
        let mut pass = p.Pass.clone();
        if !pass.is_empty() && ps_match(&p.Host, "C3pool|MoneroOcean") {
            pass = regex::Regex::new(r":[^:]+~").unwrap().replace(&pass, "~").to_string();
        }
        let params = a.params();
        const COINS: &[&str] = &[
            "AKA", "ALPH", "ALT", "ARL", "AVS", "BBC", "BCH", "BLACK", "BNBTC", "BTC", "BTRM", "BUT", "CLO", "CLORE", "Coin", "EGAZ", "EGEM", "ELH", "EPIC", "ETC", "ETHF", "ETHO", "ETHW", "ETI", "ETP", "EVOX",
            "EVR", "EXP", "FIRO", "FITA", "FRENS", "GRAMS", "GSPC", "HVQ", "IRON", "JGC", "KAW", "KCN", "LAB", "LTR", "MEOW", "MEWC", "NAPI", "NEOX", "NOVO", "OCTA", "PAPRY", "PRCO", "REDE", "RTH", "RTM",
            "RVN", "RXD", "SATO", "SATOX", "SCC", "SERO", "SPR", "THOON", "TTM", "UBQ", "VBK", "VEIL", "VKAX", "VTE", "XEL", "XNA", "YERB", "ZANO", "ZELS", "ZKBTC",
        ];
        let dag = cmd_b(a.cmd, "DAG");
        // the original reads the Wallet of an unset key: the pool user, never -w
        format!(
            "-rep-i 10 {}{}{} -o {}{}:{} -u {}{}{} {}",
            ids,
            opt(dag && a.var_b("SaveDAG"), " -dag-2disk".into()),
            opt(dag, " -daginfo".into()),
            proto,
            p.Host,
            p.Port,
            p.User,
            opt(!pass.is_empty(), format!(" -p {}", pass)),
            opt(!ps_match(&params, "-c") && !p.CoinSymbol.is_empty() && COINS.iter().any(|c| c.eq_ignore_ascii_case(&p.CoinSymbol)), format!(" -c {}", p.CoinSymbol)),
            params
        )
    };
    f.post = Some(|a, m| {
        let fee = cmd_f(a.cmd, "DevFee").unwrap_or(a.fam.dev_fee);
        m.dev_fee.values_mut().for_each(|v| *v = fee);
    });
    Some(f)
}

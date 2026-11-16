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

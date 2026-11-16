//! The ethminer/progpowminer style modules (Miners/ProgPOW*.ps1, FiroPOW*.ps1, EvrProgPOWNvidia.ps1,
//! SccMiner.ps1): one algorithm, DAG sized device filter per pool, Claymore or EthminerWrapper API.

use super::family::*;
use crate::device::Device;
use crate::miner::*;
use crate::pool::Pool;
use serde_json::Value;
use std::sync::LazyLock;

pub fn modules() -> Vec<FamilyModule> {
    vec![
        FamilyModule { name: "EvrProgPOWNvidia", types: &["NVIDIA"], def: evr_progpow_nvidia },
        FamilyModule { name: "FiroPOWAmd", types: &["AMD"], def: firopow_amd },
        FamilyModule { name: "FiroPOWNvidia", types: &["NVIDIA"], def: firopow_nvidia },
        FamilyModule { name: "ProgPOWAmd", types: &["AMD"], def: progpow_amd },
        FamilyModule { name: "ProgPOWEthercoreAmd", types: &["AMD"], def: progpow_ethercore_amd },
        FamilyModule { name: "ProgPOWEthercoreNvidia", types: &["NVIDIA"], def: progpow_ethercore_nvidia },
        FamilyModule { name: "ProgPOWNvidia", types: &["NVIDIA"], def: progpow_nvidia },
        FamilyModule { name: "ProgPOWSeroAmd", types: &["AMD"], def: progpow_sero_amd },
        FamilyModule { name: "ProgPOWSeroNvidia", types: &["NVIDIA"], def: progpow_sero_nvidia },
        FamilyModule { name: "ProgPOWZAmd", types: &["AMD"], def: progpow_z_amd },
        FamilyModule { name: "ProgPOWZNvidia", types: &["NVIDIA"], def: progpow_z_nvidia },
        FamilyModule { name: "SccMiner", types: &["AMD", "NVIDIA"], def: sccminer },
    ]
}

macro_rules! cmds {
    ($c:ident, $f:literal) => {
        static $c: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!(concat!("../../assets/miners/", $f, ".json"))));
    };
}
cmds!(CMD_EVR, "EvrProgPOWNvidia");
cmds!(CMD_FIRO_AMD, "FiroPOWAmd");
cmds!(CMD_FIRO_NV, "FiroPOWNvidia");
cmds!(CMD_PP_AMD, "ProgPOWAmd");
cmds!(CMD_PPE_AMD, "ProgPOWEthercoreAmd");
cmds!(CMD_PPE_NV, "ProgPOWEthercoreNvidia");
cmds!(CMD_PP_NV, "ProgPOWNvidia");
cmds!(CMD_PPS_AMD, "ProgPOWSeroAmd");
cmds!(CMD_PPS_NV, "ProgPOWSeroNvidia");
cmds!(CMD_PPZ_AMD, "ProgPOWZAmd");
cmds!(CMD_PPZ_NV, "ProgPOWZNvidia");
cmds!(CMD_SCC, "SccMiner");

/// Test-VRAM with $MinMemGB = DagSizeMax or Get-EthDAGSize of the pool's coin (default: Vars.DagCoin)
fn dag_filter(fam: &Family, ctx: &MinerCtx, d: &Device, cmd: &Value, pool: Option<&Pool>, algo0: &str) -> bool {
    let Some(pool) = pool else { return false };
    let coin = fam.vars.get("DagCoin").map(|s| s.as_str()).unwrap_or("");
    ctx.test_vram(d, dag_size(pool, algo0, coin, crate::jsonx::num(crate::jsonx::get(cmd, "MinMemGB"))))
}

/// -P <protocol>://<user>[:<pass>]@<host>:<port>
fn pool_url(a: &A) -> String {
    format!("{}://{}@{}:{}", eth_protocol(a.pool), user_pass_enc(a.pool), a.pool.Host, a.pool.Port)
}

/// the common part of all modules
fn base(name: &'static str, vendor: &'static str, cmds: &'static LazyLock<Vec<Value>>, dag_coin: &str) -> Family {
    let mut f = Family::new(name, vendor, cmds);
    f.suffix = Suffix::ListGPU;
    f.api = "Claymore";
    f.dev_id_join = " ";
    f.per_key_filter = Some(dag_filter);
    f.vars.insert("DagCoin", dag_coin.into());
    if vendor == "AMD" {
        f.needs_platform_id = true;
        f.dev_id_prop = "Type_PlatformId_Index";
        f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    }
    f
}

fn win_linux_x64(ctx: &MinerCtx) -> bool {
    (ctx.is_windows() || ctx.is_linux()) && !(ctx.is_linux() && ctx.is_arm())
}

fn progpow_nvidia(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = base("ProgPOWNvidia", "NVIDIA", &CMD_PP_NV, "BCI");
    f.port = "331".into();
    f.manual_uri = "https://github.com/BitcoinInterestOfficial/BitcoinInterest/releases".into();
    f.version = "0.16".into();
    f = if ctx.is_linux() {
        f.cuda_p(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.16-progpowminer/progpow_linux_0.16_final.7z", "10.0", "Bin/NVIDIA-ProgPOW/progpowminer_cuda10"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.16-progpowminer/progpow_linux_0.16_final.7z", "9.2", "Bin/NVIDIA-ProgPOW/progpowminer_cuda9.2"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.16-progpowminer/progpow_linux_0.16_final.7z", "9.1", "Bin/NVIDIA-ProgPOW/progpowminer_cuda9.1"),
        ])
    } else {
        f.cuda_p(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.16-progpowminer/progpowminer-cuda10.0-windows-0.16_final.7z", "10.0", "Bin/NVIDIA-ProgPOW/progpowminer-cuda.exe"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.16-progpowminer/progpowminer-cuda9.2-windows-0.16_final.7z", "9.2", "Bin/NVIDIA-ProgPOW/progpowminer-cuda.exe"),
        ])
    };
    f.make_args = |a| format!("--api-port -$mport -P {} --cuda --cuda-devices {} --exit {}", pool_url(a), a.ids, a.params());
    Some(f)
}

fn progpow_ethercore_nvidia(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = base("ProgPOWEthercoreNvidia", "NVIDIA", &CMD_PPE_NV, "ERE");
    f.port = "357".into();
    f.manual_uri = "https://github.com/ethercore/ethcoreminer/releases".into();
    f.version = "1.0.0".into();
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-ProgPOWEthercore/ethcoreminer".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ethercore/ethcoreminer-1.0.0-nvidia-cuda-10-linux-x86_64.tar.gz", "10.0"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ethercore/ethcoreminer-1.0.0-nvidia-cuda-9-linux-x86_64.tar.gz", "9.0"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ethercore/ethcoreminer-1.0.0-nvidia-cuda-8-linux-x86_64.tar.gz", "8.0"),
        ]);
    } else {
        f.path = "Bin/NVIDIA-ProgPOWEthercore/ethcoreminer.exe".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ethercore/ethcoreminer-1.0.0-nvidia-cuda-10.0-windows-amd64.zip", "10.0"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ethercore/ethcoreminer-1.0.0-nvidia-cuda-9.2-windows-amd64.zip", "9.2"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ethercore/ethcoreminer-1.0.0-nvidia-cuda-8.0-windows-amd64.zip", "8.0"),
        ]);
    }
    f.make_args = |a| format!("--api-port -$mport -P {} --cuda --cu-devices {} --farm-retries 10 --exit {}", pool_url(a), a.ids, a.params());
    Some(f)
}

fn evr_progpow_nvidia(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = base("EvrProgPOWNvidia", "NVIDIA", &CMD_EVR, "BCI");
    f.port = "304".into();
    f.manual_uri = "https://github.com/EvrmoreOrg/evrprogpowminer".into();
    f.version = "1.3.0".into();
    f = if ctx.is_linux() {
        f.cuda_p(&[(
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.3.0-evrprogpow/evrprogpowminer-ubuntu20.0.4-v1p3p0-a66d921b.tar.gz",
            "11.2",
            "Bin/NVIDIA-EvrProgPOW/evrprogpowminer",
        )])
    } else {
        f.cuda_p(&[(
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.3.0-evrprogpow/evrprogpowminer-windows64-v1p3p0-a66d921b.7z",
            "11.2",
            "Bin/NVIDIA-EvrProgPOW/evrprogpowminer.exe",
        )])
    };
    f.make_args = |a| format!("--api-port -$mport -P {} --cuda --cuda-devices {} --exit {}", pool_url(a), a.ids, a.params());
    Some(f)
}

fn firopow_nvidia(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = base("FiroPOWNvidia", "NVIDIA", &CMD_FIRO_NV, "FIRO");
    f.port = "367".into();
    f.manual_uri = "https://github.com/firoorg/firominer/releases".into();
    f.version = "1.6.0".into();
    f = if ctx.is_linux() {
        f.cuda_p(&[(
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.6.0-firominer/firominer-linux-x86_64-cuda12.9-opencl-1.6.0.tar.gz",
            "12.9",
            "Bin/NVIDIA-FiroPOW/bin/firominer",
        )])
    } else {
        f.cuda_p(&[(
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.6.0-firominer/firominer-windows-x86_64-cuda12.9-opencl-1.6.0.zip",
            "12.9",
            "Bin/NVIDIA-FiroPOW/bin/firominer.exe",
        )])
    };
    f.make_args = |a| format!("--api-port -$mport -P {} --cuda --cuda-devices {} --exit {}", pool_url(a), a.ids, a.params());
    Some(f)
}

fn progpow_amd(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = base("ProgPOWAmd", "AMD", &CMD_PP_AMD, "BCI");
    if ctx.is_linux() {
        f.path = "Bin/AMD-ProgPOW/progpowminer_opencl_only".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.16-progpowminer/progpow_linux_0.16_final.7z".into();
    } else {
        f.path = "Bin/AMD-ProgPOW/progpowminer-amd.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.16-progpowminer/progpowminer-amd-windows-0.16_final.7z".into();
    }
    f.port = "409".into();
    f.manual_uri = "https://github.com/BitcoinInterestOfficial/BitcoinInterest/releases".into();
    f.version = "0.16".into();
    f.make_args = |a| format!("--api-port -$mport -P {} --opencl --opencl-platform {} --opencl-devices {} --exit {}", pool_url(a), a.platform_id, a.ids, a.params());
    Some(f)
}

fn progpow_ethercore_amd(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = base("ProgPOWEthercoreAmd", "AMD", &CMD_PPE_AMD, "ERE");
    if ctx.is_linux() {
        f.path = "Bin/AMD-ProgPOWEthercore/ethcoreminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ethercore/ethcoreminer-1.0.0-amd-opencl-linux-x86_64.tar.gz".into();
    } else {
        f.path = "Bin/AMD-ProgPOWEthercore/ethcoreminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ethercore/ethcoreminer-1.0.0-amd-opencl-windows-amd64.zip".into();
    }
    f.port = "415".into();
    f.manual_uri = "https://github.com/ethercore/ethcoreminer/releases".into();
    f.version = "1.0.0".into();
    f.make_args = |a| format!("--api-port -$mport -P {} --opencl --opencl-platform {} --cl-devices {} --farm-retries 10 --exit {}", pool_url(a), a.platform_id, a.ids, a.params());
    Some(f)
}

fn firopow_amd(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = base("FiroPOWAmd", "AMD", &CMD_FIRO_AMD, "FIRO");
    f.port = "418".into();
    f.manual_uri = "https://github.com/firoorg/firominer/releases".into();
    f.version = "1.6.0".into();
    if ctx.is_linux() {
        f.path = "Bin/AMD-FiroPOW/bin/firominer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.6.0-firominer/firominer-linux-x86_64-opencl-1.6.0.tar.gz".into();
    } else {
        f.path = "Bin/AMD-FiroPOW/bin/firominer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.6.0-firominer/firominer-windows-x86_64-opencl-1.6.0.zip".into();
    }
    f.make_args = |a| format!("--api-port -$mport -P {} --opencl --opencl-devices {} --exit {}", pool_url(a), a.ids, a.params());
    Some(f)
}

/// ProgPOWSero*/ProgPOWZ*: Windows only builds with the EthminerWrapper API
fn sero_z(name: &'static str, vendor: &'static str, cmds: &'static LazyLock<Vec<Value>>, coin: &str) -> Family {
    let mut f = base(name, vendor, cmds, coin);
    f.api = "EthminerWrapper";
    f.env_vars = vec![];
    f
}

fn progpow_sero_amd(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = sero_z("ProgPOWSeroAmd", "AMD", &CMD_PPS_AMD, "SERO");
    f.path = "Bin/AMD-ProgPOWSero/serominer-opencl.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.3.0-progpowsero/serominer-v0.3.0-windows-amd64.7z".into();
    f.port = "414".into();
    f.manual_uri = "https://github.com/sero-cash/serominer/releases".into();
    f.version = "0.3.0".into();
    f.make_args = |a| format!("--api-port -$mport -P {} --opencl --cl-devices {} --exit {}", pool_url(a), a.ids, a.params());
    Some(f)
}

fn progpow_z_amd(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = sero_z("ProgPOWZAmd", "AMD", &CMD_PPZ_AMD, "ZANO");
    f.path = "Bin/AMD-ProgPOWZ/progminer-zano-opencl.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.1.2-progminerzano/progminer-zano-win-1.1.2.7z".into();
    f.port = "413".into();
    f.manual_uri = "https://github.com/hyle-team/zano/releases".into();
    f.version = "1.1.2".into();
    f.make_args = |a| format!("--api-port -$mport -P {} --opencl --cl-devices {} --exit {}", pool_url(a), a.ids, a.params());
    Some(f)
}

fn progpow_sero_nvidia(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = sero_z("ProgPOWSeroNvidia", "NVIDIA", &CMD_PPS_NV, "SERO");
    f.path = "Bin/NVIDIA-ProgPOWSero/serominer-cuda.exe".into();
    f = f.cuda(&[("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.3.0-progpowsero/serominer-v0.3.0-windows-amd64.7z", "9.2")]);
    f.port = "355".into();
    f.manual_uri = "https://github.com/sero-cash/serominer/releases".into();
    f.version = "0.3.0".into();
    f.make_args = |a| format!("--api-port -$mport -P {} --cuda --cu-devices {} --exit {}", pool_url(a), a.ids, a.params());
    Some(f)
}

fn progpow_z_nvidia(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = sero_z("ProgPOWZNvidia", "NVIDIA", &CMD_PPZ_NV, "ZANO");
    f.path = "Bin/NVIDIA-ProgPOWZ/progminer-zano-cuda.exe".into();
    f = f.cuda(&[("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.1.2-progminerzano/progminer-zano-win-1.1.2.7z", "10.1")]);
    f.port = "340".into();
    f.manual_uri = "https://github.com/hyle-team/zano/releases".into();
    f.version = "1.1.2".into();
    f.make_args = |a| format!("--api-port -$mport -P {} --cuda --cu-devices {} --exit {}", pool_url(a), a.ids, a.params());
    Some(f)
}

/// SccMiner: AMD and NVIDIA (NVIDIA only with a fitting CUDA version), the DAG of SCC
fn sccminer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("SccMiner", "AMD", &CMD_SCC);
    f.info_types = vec!["AMD".into(), "NVIDIA".into()];
    f.suffix = Suffix::ListGPU;
    f.api = "Claymore";
    f.dev_id_join = " ";
    f.port = "374".into();
    f.manual_uri = "https://github.com/stakecube/sccminer/releases".into();
    f.version = "1.1.0".into();
    if ctx.is_linux() {
        f.path = "Bin/GPU-SccMiner/sccminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.1.0-sccminer/sccminer-1.1.0-linux.zip".into();
    } else {
        f.path = "Bin/GPU-SccMiner/sccminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.1.0-sccminer/sccminer-1.1.0b-Windows.7z".into();
    }
    let cuda = !info_only && ctx.has_type("NVIDIA") && ctx.cuda_ok("12.0", "SccMiner");
    f.vendors = if cuda { vec!["AMD", "NVIDIA"] } else { vec!["AMD"] };
    // the original computes $MinMemGB before the pool key loop: the DAG of SCC on the base algorithm's pool
    f.pre_key_filter = Some(|ctx, d, cmd| {
        let algo0 = crate::data::get_algorithm(&cmd_s(cmd, "MainAlgorithm"));
        let min = crate::jsonx::num(crate::jsonx::get(cmd, "MinMemGB"));
        let mem = match ctx.pool(&algo0).and_then(|p| p.DagSizeMax) {
            Some(x) if x > 0.0 => x,
            _ => crate::data::get_eth_dag_size("SCC", &algo0, min),
        };
        ctx.test_vram(d, mem)
    });
    f.make_args = |a| {
        let dp = match a.vendor {
            "NVIDIA" => "--cuda --cu-devices",
            "AMD" => "--opencl --cl-devices",
            _ => "",
        };
        format!("--api-port -$mport -P {} {} {} --exit {}", pool_url(a), dp, a.ids, a.params())
    };
    Some(f)
}

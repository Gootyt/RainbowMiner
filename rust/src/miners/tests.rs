//! A synthetic rig (one NVIDIA, one AMD GPU, a CPU) and a pool for every algorithm: all miner
//! modules must build their candidates without panicking. `cargo test miner_dump -- --nocapture`
//! prints the first command line of each module.

use crate::config::Config;
use crate::device::{CpuInfo, Device, DeviceCache};
use crate::miner::*;
use crate::pool::Pool;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::Path;

fn gpu(name: &str, vendor: &str, model: &str, idx: usize, arch: &str, mem_gb: u64, platform: usize) -> Device {
    let mut d = Device {
        Name: name.into(),
        Index: idx,
        Vendor: vendor.into(),
        Type: "Gpu".into(),
        Model: model.into(),
        Model_Name: model.into(),
        Type_Vendor_Index: 0,
        Type_Index: idx,
        PlatformId: platform,
        PlatformId_Index: 0,
        Type_PlatformId_Index: 0,
        ..Default::default()
    };
    d.OpenCL.GlobalMemSize = mem_gb << 30;
    d.OpenCL.GlobalMemSizeGB = mem_gb as i64;
    d.OpenCL.Architecture = arch.into();
    d.OpenCL.Name = if vendor == "AMD" { "Ellesmere".into() } else { model.into() };
    d.OpenCL.DeviceCapability = if vendor == "NVIDIA" { Some("8.6".into()) } else { None };
    d
}

pub struct Rig {
    pub config: Config,
    pub devices: DeviceCache,
    pub cpu: CpuInfo,
    pub pools: PoolsMap,
}

pub fn rig() -> Rig {
    crate::data::init(Path::new(".."));
    let mut config = Config { v: json!({"EnableNeverprofitableAlgos": false, "GPUReservedVRAMGB": ""}), ..Default::default() };
    config.cuda_version = "12.4".into();
    let cpu_dev = Device {
        Name: "CPU#00".into(),
        Index: 2,
        Vendor: "AMD".into(),
        Type: "Cpu".into(),
        Model: "CPU".into(),
        Model_Name: "Ryzen".into(),
        ..Default::default()
    };
    let all = vec![gpu("GPU#00", "NVIDIA", "RTX3080", 0, "Ampere", 10, 0), gpu("GPU#01", "AMD", "RX580", 1, "Polaris", 8, 1), cpu_dev];
    let devices = DeviceCache::build(all.clone(), all, "", &HashMap::new());
    let mut cpu = CpuInfo { Vendor: "AMD".into(), Cores: 8, Threads: 16, Architecture: "x86_64".into(), ..Default::default() };
    for f in ["avx", "avx2", "aes", "sha", "sse2", "sse42", "ssse3"] {
        cpu.Features.insert(f.into(), true);
    }
    let mut pools = PoolsMap::default();
    for algo in crate::data::all_algorithms() {
        let p = Pool {
            Algorithm: algo.clone(),
            Algorithm0: algo.clone(),
            Name: "TestPool".into(),
            Host: "stratum.example.org".into(),
            Port: 3333,
            Protocol: "stratum+tcp".into(),
            User: "WALLET.worker".into(),
            Wallet: "WALLET".into(),
            Worker: "worker".into(),
            Pass: "x".into(),
            Currency: "BTC".into(),
            ..Default::default()
        };
        pools.insert(&algo, p);
    }
    Rig { config, devices, cpu, pools }
}

pub fn with_ctx<R>(r: &Rig, f: impl FnOnce(&MinerCtx) -> R) -> R {
    let alt = HashMap::new();
    let yiimp = HashSet::new();
    let ctx = MinerCtx {
        base: Path::new(".."),
        config: &r.config,
        devices: &r.devices,
        cpu: &r.cpu,
        pools: &r.pools,
        alternates: &alt,
        yiimp_pools: &yiimp,
        distro_codename: "jammy".into(),
        libc_version: crate::util::get_version("2.35"),
        is_vm: false,
    };
    f(&ctx)
}

fn args(m: &MinerCandidate) -> String {
    match &m.arguments {
        MinerArgs::Str(s) => s.clone(),
        MinerArgs::Obj(v) => v.to_string(),
    }
}

#[test]
fn miner_dump() {
    let r = rig();
    with_ctx(&r, |ctx| {
        for module in crate::miners::registry() {
            let info = module.info(ctx);
            let miners = module.get_miners(ctx);
            let first = miners.first().map(|m| format!("{} [{}] {}", m.name, m.api, args(m))).unwrap_or_default();
            println!("{:<24} info={:<5} n={:<4} {}", module.name(), info.is_some(), miners.len(), first);
            for m in &miners {
                assert!(!m.name.is_empty());
                assert!(m.port > 0, "{}: no port", m.name);
                assert!(!args(m).contains("TODO"));
            }
        }
    });
}

#[test]
fn family_args() {
    let r = rig();
    with_ctx(&r, |ctx| {
        let find = |name: &str| crate::miners::registry().into_iter().find(|m| m.name() == name).unwrap().get_miners(ctx);
        let klaust = find("CcminerKlausT");
        assert!(!klaust.is_empty());
        let m = &klaust[0];
        assert_eq!(m.name, "CcminerKlausT-GPU#00");
        assert_eq!(m.port, 14000);
        assert_eq!(m.api, "Ccminer");
        assert!(args(m).starts_with("-R 1 -b $mport -d 0 -a "), "{}", args(m));
        assert!(args(m).contains(" -q -o stratum+tcp://stratum.example.org:3333 -u WALLET.worker -p x"), "{}", args(m));
        // Uri chosen by the CUDA version (12.4 -> the CUDA 11.8 build)
        assert!(m.uri.contains("cuda118"), "{}", m.uri);

        let rk = find("CpuminerRKZ");
        assert!(!rk.is_empty());
        assert!(rk[0].path.to_string_lossy().ends_with("cpuminer-avx2-sha"), "{}", rk[0].path.display());
        assert_eq!(rk[0].extend_interval, 2);
    });
}

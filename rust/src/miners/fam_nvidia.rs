//! generated from the Invoke-MinerFamily stubs, see family.rs

use super::family::*;
use super::opt;
use crate::miner::*;
use serde_json::Value;
use std::sync::LazyLock;

pub fn modules() -> Vec<FamilyModule> {
    vec![
        FamilyModule { name: "CcminerAlexis78", types: &["NVIDIA"], def: ccminer_alexis78 },
        FamilyModule { name: "CcminerAllium", types: &["NVIDIA"], def: ccminer_allium },
        FamilyModule { name: "CcminerBmw512", types: &["NVIDIA"], def: ccminer_bmw512 },
        FamilyModule { name: "CcminerDyn", types: &["NVIDIA"], def: ccminer_dyn },
        FamilyModule { name: "CcminerKlausT", types: &["NVIDIA"], def: ccminer_klaus_t },
        FamilyModule { name: "CcminerLyra2v3", types: &["NVIDIA"], def: ccminer_lyra2v3 },
        FamilyModule { name: "CcminerLyra2z330", types: &["NVIDIA"], def: ccminer_lyra2z330 },
        FamilyModule { name: "CcminerMtp", types: &["NVIDIA"], def: ccminer_mtp },
        FamilyModule { name: "CcminerMtp10", types: &["NVIDIA"], def: ccminer_mtp10 },
        FamilyModule { name: "CcminerRadiator", types: &["NVIDIA"], def: ccminer_radiator },
        FamilyModule { name: "CcminerSha3d", types: &["NVIDIA"], def: ccminer_sha3d },
        FamilyModule { name: "CcminerTpfuemp", types: &["NVIDIA"], def: ccminer_tpfuemp },
        FamilyModule { name: "CcminerTpruvot", types: &["NVIDIA"], def: ccminer_tpruvot },
        FamilyModule { name: "CcminerVerus", types: &["NVIDIA"], def: ccminer_verus },
        FamilyModule { name: "CcminerXaya", types: &["NVIDIA"], def: ccminer_xaya },
        FamilyModule { name: "CcminerXevan", types: &["NVIDIA"], def: ccminer_xevan },
        FamilyModule { name: "CcminerYescrypt", types: &["NVIDIA"], def: ccminer_yescrypt },
        FamilyModule { name: "CcminerYescryptR8", types: &["NVIDIA"], def: ccminer_yescrypt_r8 },
        FamilyModule { name: "CcminerZP", types: &["NVIDIA"], def: ccminer_zp },
        FamilyModule { name: "Suprminer", types: &["NVIDIA"], def: suprminer },
        FamilyModule { name: "CryptoDredge16", types: &["NVIDIA"], def: crypto_dredge16 },
        FamilyModule { name: "EnemyZealot", types: &["NVIDIA"], def: enemy_zealot },
        FamilyModule { name: "Ewbf", types: &["NVIDIA"], def: ewbf },
        FamilyModule { name: "EwbfZcash", types: &["NVIDIA"], def: ewbf_zcash },
        FamilyModule { name: "HtnNvidia", types: &["NVIDIA"], def: htn_nvidia },
    ]
}

// ---- Miners/CcminerAlexis78.ps1
static CMD_CCMINER_ALEXIS78: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerAlexis78.json")));

fn ccminer_alexis78(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CcminerAlexis78", "NVIDIA", &CMD_CCMINER_ALEXIS78);
    f.version = "1.5.1".into();
    f.port = "102".into();
    f.dev_fee = 0.0;
    f.path = "Bin/NVIDIA-Alexis78/ccminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.5.1-ccmineralexis/ccminerAlexis78cuda75x64.7z".into();
    let cuda = "7.5";
    if !info_only && !ctx.cuda_ok(cuda, "CcminerAlexis78") {
        return None;
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.make_args = |a| format!("-R 1 -b $mport -d {} -a {} -q -o {}://{}:{} -u {}{} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerAllium.ps1
static CMD_CCMINER_ALLIUM: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerAllium.json")));

fn ccminer_allium(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerAllium", "NVIDIA", &CMD_CCMINER_ALLIUM);
    f.manual_uri = "https://github.com/lenis0012/ccminer/releases".into();
    f.version = "2.3.0".into();
    f.port = "103".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-CcminerAllium/ccminer".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.3.0-ccminerallium/ccminerallium-v2.3.0-linux.7z", "10.0"),
        ]);
    } else {
        f.path = "Bin/NVIDIA-CcminerAllium/ccminer-x64.exe".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.3.0-ccminerallium/ccminerallium-v2.3.0-win.7z", "10.0"),
        ]);
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.dev_fee_zero = true;
    f.make_args = |a| format!("-R 1 -b $mport -d {} -a {} -q -o {}://{}:{} -u {} -p {} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pool.Pass, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerBmw512.ps1
static CMD_CCMINER_BMW512: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerBmw512.json")));

fn ccminer_bmw512(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CcminerBmw512", "NVIDIA", &CMD_CCMINER_BMW512);
    f.manual_uri = "https://github.com/Minerx117/ccminer-bmw512/releases".into();
    f.version = "2.2.5".into();
    f.port = "133".into();
    f.dev_fee = 0.0;
    f.path = "Bin/NVIDIA-CcminerBMW512/ccminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2.2.5-ccminerbmw512/ccminerbmw512x86.7z".into();
    let cuda = "9.1";
    if !info_only && !ctx.cuda_ok(cuda, "CcminerBmw512") {
        return None;
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.dev_fee_zero = true;
    f.make_args = |a| format!("-R 1 -b $mport -d {} -q -o {}://{}:{} -u {}{} {}", a.ids, a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerDyn.ps1
static CMD_CCMINER_DYN: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerDyn.json")));

fn ccminer_dyn(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CcminerDyn", "NVIDIA", &CMD_CCMINER_DYN);
    f.manual_uri = "https://github.com/nemosminer/Dynamic-GPU-Miner-Nvidia/releases".into();
    f.version = "1.0.2".into();
    f.port = "128".into();
    f.dev_fee = 0.0;
    f.path = "Bin/NVIDIA-CcminerDyn/ccminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.2-ccminerdyn/ccminerdyn-1.0.2.7z".into();
    let cuda = "10.0";
    if !info_only && !ctx.cuda_ok(cuda, "CcminerDyn") {
        return None;
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.dev_fee_zero = true;
    f.make_args = |a| format!("-R 1 -b $mport -d {} -q -o {}://{}:{} -u {}{} {}", a.ids, a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerKlausT.ps1
static CMD_CCMINER_KLAUS_T: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerKlausT.json")));

fn ccminer_klaus_t(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerKlausT", "NVIDIA", &CMD_CCMINER_KLAUS_T);
    f.manual_uri = "https://github.com/KlausT/ccminer/releases".into();
    f.version = "8.26x3".into();
    f.port = "140".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-KlausT/ccminer".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.26-klaust/ccminerklaust-826x3-cuda118-linux.7z", "11.8"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.26-klaust/ccminerklaust-826x3-cuda116-linux.7z", "11.6"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.26-klaust/ccminerklaust-826x3-cuda114-linux.7z", "11.4"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.26-klaust/ccminerklaust-826x3-cuda102-linux.7z", "10.2"),
        ]);
    } else {
        f.path = "Bin/NVIDIA-KlausT/ccminer.exe".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.26-klaust/ccminerklaust-826x3-cuda118-x64.7z", "11.8"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.26-klaust/ccminerklaust-826x3-cuda116-x64.7z", "11.6"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.26-klaust/ccminerklaust-826x3-cuda114-x64.7z", "11.4"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.26-klaust/ccminerklaust-826x2-cuda102-x64.7z", "10.2"),
        ]);
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.dev_fee_zero = true;
    f.mining_priority = Some(2);
    f.make_args = |a| format!("-R 1 -b $mport -d {} -a {} -q -o {}://{}:{} -u {} -p {} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pool.Pass, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerLyra2v3.ps1
static CMD_CCMINER_LYRA2V3: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerLyra2v3.json")));

fn ccminer_lyra2v3(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CcminerLyra2v3", "NVIDIA", &CMD_CCMINER_LYRA2V3);
    f.manual_uri = "https://github.com/nemosminer/ccminer-KlausT-8.21-mod-r18-src-fix/releases".into();
    f.version = "8.21-r18v3".into();
    f.port = "106".into();
    f.dev_fee = 0.0;
    f.path = "Bin/NVIDIA-Lyra2RE3/ccminer.exe".into();
    f = f.cuda(&[
        ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.21r18v3-ccminerklaust/ccminerKlausT-8.21-r18v3.7z", "10.0"),
    ]);
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.dev_fee_zero = true;
    f.mining_priority = Some(2);
    f.make_args = |a| format!("-R 1 -b $mport -d {} -a {} -q -o {}://{}:{} -u {} -p {} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pool.Pass, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerLyra2z330.ps1
static CMD_CCMINER_LYRA2Z330: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerLyra2z330.json")));

fn ccminer_lyra2z330(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CcminerLyra2z330", "NVIDIA", &CMD_CCMINER_LYRA2Z330);
    f.manual_uri = "https://github.com/Minerx117/ccminer8.21r9-lyra2z330/releases".into();
    f.version = "8.21r9".into();
    f.port = "138".into();
    f.dev_fee = 0.0;
    f.path = "Bin/NVIDIA-Lyra2z330/ccminer.exe".into();
    f = f.cuda(&[
        ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.21r9-ccminerlyra2z330v3/ccminerlyra2z330v3.zip", "10.0"),
    ]);
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.arch_not_in = &["Other"];
    f.day_penalty = true;
    f.max_rej_default = Some(0.5);
    f.mining_priority = Some(2);
    f.emits_max_rej = true;
    f.make_args = |a| format!("--no-cpu-verify -N 1 -R 1 -b $mport -a {} -d {} -q -o {}://{}:{} -u {}{} {}", a.main(), a.ids, a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerMtp.ps1
static CMD_CCMINER_MTP: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerMtp.json")));

fn ccminer_mtp(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerMtp", "NVIDIA", &CMD_CCMINER_MTP);
    f.manual_uri = "https://github.com/tecracoin/ccminer/releases".into();
    f.port = "126".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-CcminerMTP/ccminer_linux_cuda".into();
        f = f.cuda_v(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.11-ccminertcr/ccminertcr-v1.2.11-linux-cuda111.7z", "11.1", "1.2.11"),
        ]);
    } else {
        f.path = "Bin/NVIDIA-CcminerMTP/ccminer.exe".into();
        f = f.cuda_v(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.11-ccminertcr/ccminertcr-v1.2.11-win.7z", "11.1", "1.2.11"),
        ]);
    }
    f.path_append_cuda = true;
    f.suffix = Suffix::Keys;
    f.check_ssl = true;
    f.max_dev_count = 6;
    f.per_entry_vram = true;
    f.arch_not_in = &["Other", "Pascal", "Turing"];
    f.dev_fee_zero = true;
    f.mining_priority = Some(2);
    f.use_exclude_pool = true;
    f.vars.insert("UseCPUAffinity", if ctx.is_linux() { "1" } else { "" }.into());
    f.make_args = |a| format!("-R 1 -N 3 -b $mport -d {} -a {} -o {}://{}:{} -u {}{}{} --no-donation {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), opt(a.var_b("UseCPUAffinity"), " --cpu-affinity 1".to_string()), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerMtp10.ps1
static CMD_CCMINER_MTP10: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerMtp10.json")));

fn ccminer_mtp10(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerMtp10", "NVIDIA", &CMD_CCMINER_MTP10);
    f.manual_uri = "https://github.com/tecracoin/ccminer/releases".into();
    f.port = "127".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-CcminerMTP10/ccminer_linux_cuda".into();
        f = f.cuda_v(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.10-ccminertcr/ccminertcr-v1.2.10-linux-cuda102.7z", "10.2", "1.2.10"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.10-ccminertcr/ccminertcr-v1.2.10-linux-cuda100.7z", "10.0", "1.2.10"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.10-ccminertcr/ccminertcr-v1.2.10-linux-cuda92.7z", "9.2", "1.2.10"),
        ]);
    } else {
        f.path = "Bin/NVIDIA-CcminerMTP10/ccminer.exe".into();
        f = f.cuda_v(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.10-ccminertcr/ccminertcr-v1.2.10-win.7z", "10.2", "1.2.10"),
        ]);
    }
    f.path_append_cuda = true;
    f.suffix = Suffix::Keys;
    f.check_ssl = true;
    f.max_dev_count = 6;
    f.per_entry_vram = true;
    f.arch_in = &["Other", "Pascal", "Turing"];
    f.dev_fee_zero = true;
    f.mining_priority = Some(2);
    f.use_exclude_pool = true;
    f.vars.insert("UseCPUAffinity", if ctx.is_linux() { "1" } else { "" }.into());
    f.make_args = |a| format!("-R 1 -N 3 -b $mport -d {} -a {} -o {}://{}:{} -u {}{}{} --no-donation {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), opt(a.var_b("UseCPUAffinity"), " --cpu-affinity 1".to_string()), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerRadiator.ps1
static CMD_CCMINER_RADIATOR: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerRadiator.json")));

fn ccminer_radiator(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerRadiator", "NVIDIA", &CMD_CCMINER_RADIATOR);
    f.manual_uri = "https://github.com/xiaolin1579/radiator/releases".into();
    f.version = "1.0.0".into();
    f.port = "108".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-Radiator/ccminer".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ccminerradiator/ccminerradiator-1.0.0-cuda116-linux.7z", "11.6"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ccminerradiator/ccminerradiator-1.0.0-cuda112-linux.7z", "11.2"),
        ]);
    } else {
        f.path = "Bin/NVIDIA-Radiator/ccminer.exe".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ccminerradiator/Radiator1.0.0_cuda11.6_Win64.zip", "11.6"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-ccminerradiator/Radiator1.0.0_cuda11.2_Win64.zip", "11.2"),
        ]);
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.dev_fee_zero = true;
    f.mining_priority = Some(2);
    f.make_args = |a| format!("-R 1 -b $mport -d {} -a {} -q -o {}://{}:{} -u {} -p {} {}", a.ids, a.c("Algorithm"), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pool.Pass, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerSha3d.ps1
static CMD_CCMINER_SHA3D: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerSha3d.json")));

fn ccminer_sha3d(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerSha3d", "NVIDIA", &CMD_CCMINER_SHA3D);
    f.manual_uri = "https://github.com/brian112358/ccminer-bsha3/releases".into();
    f.version = "1.0.1".into();
    f.port = "143".into();
    f.dev_fee = 0.0;
    let cuda = "9.1";
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-CcminerSha3d/ccminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.1-ccminersha3d/ccminersha3d-v1.0.1-linux.7z".into();
    } else {
        f.path = "Bin/NVIDIA-CcminerSha3d/ccminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.1-ccminersha3d/ccminersha3d-v1.0.1-win.7z".into();
    }
    if !info_only && !ctx.cuda_ok(cuda, "CcminerSha3d") {
        return None;
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.make_args = |a| format!("-R 1 -b $mport -d {} -o {}://{}:{} -u {}{} {}", a.ids, a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerTpfuemp.ps1
static CMD_CCMINER_TPFUEMP: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerTpfuemp.json")));

fn ccminer_tpfuemp(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerTpfuemp", "NVIDIA", &CMD_CCMINER_TPFUEMP);
    f.manual_uri = "https://github.com/tpfuemp/ccminer-tpfuemp/releases".into();
    f.version = "2026.11".into();
    f.port = "147".into();
    f.dev_fee = 0.0;
    let cuda = "11.8";
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-CcminerTpfuemp/ccminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2026.11-ccminertpfuemp/ccminer-tpfuemp-2026.11-linux-x64.tar.gz".into();
    } else {
        f.path = "Bin/NVIDIA-CcminerTpfuemp/ccminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2026.11-ccminertpfuemp/ccminer-tpfuemp-2026.11-windows-x64.zip".into();
    }
    if !info_only && !ctx.cuda_ok(cuda, "CcminerTpfuemp") {
        return None;
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.dev_fee_zero = true;
    f.make_args = |a| format!("-R 1 -b $mport -d {} -q -o {}://{}:{} -u {}{} {} --cuda-schedule 4", a.ids, a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerTpruvot.ps1
static CMD_CCMINER_TPRUVOT: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerTpruvot.json")));

fn ccminer_tpruvot(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CcminerTpruvot", "NVIDIA", &CMD_CCMINER_TPRUVOT);
    f.manual_uri = "https://github.com/tpruvot/ccminer/releases".into();
    f.version = "2.3.1".into();
    f.port = "114".into();
    f.dev_fee = 0.0;
    f.path = "Bin/NVIDIA-TPruvot/ccminer-x64.exe".into();
    f = f.cuda(&[
        ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.3.1-tpruvot/ccminer-2.3.1-cuda10.1.7z", "10.1"),
    ]);
    f.suffix = Suffix::KeysCoin;
    f.check_ssl = true;
    f.use_exclude_pool = true;
    f.use_coin_symbols = true;
    f.emits_no_cpu = true;
    f.make_args = |a| format!("-T 50000 -R 1 -b $mport -d {} -a {} -q -o {}://{}:{} -u {}{} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerVerus.ps1
static CMD_CCMINER_VERUS: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerVerus.json")));

fn ccminer_verus(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerVerus", "NVIDIA", &CMD_CCMINER_VERUS);
    f.manual_uri = "https://github.com/monkins1010/ccminer/releases".into();
    f.version = "3.8.3".into();
    f.port = "139".into();
    f.dev_fee = 0.0;
    let cuda: &str;
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-CcminerVerus/ccminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.8.3-ccminerverus/ccminerverus22-3.8.3cuda-linux.7z".into();
        cuda = "10.2";
    } else {
        f.path = "Bin/NVIDIA-CcminerVerus/ccminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.8.3-ccminerverus/ccminerverus22-3.8.3cuda-win.7z".into();
        cuda = "8.0";
    }
    if !info_only && !ctx.cuda_ok(cuda, "CcminerVerus") {
        return None;
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.dev_fee_zero = true;
    f.make_args = |a| format!("-R 1 -b $mport -d {} -q -o {}://{}:{} -u {}{} {}", a.ids, a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerXaya.ps1
static CMD_CCMINER_XAYA: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerXaya.json")));

fn ccminer_xaya(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CcminerXaya", "NVIDIA", &CMD_CCMINER_XAYA);
    f.manual_uri = "https://github.com/xaya/ccminer/releases".into();
    f.version = "0.1".into();
    f.port = "145".into();
    f.dev_fee = 0.0;
    f.path = "Bin/NVIDIA-CcminerXaya/ccminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.2-ccminerxaya/neoscryptxayaV02.7z".into();
    let cuda = "11.1.1";
    if !info_only && !ctx.cuda_ok(cuda, "CcminerXaya") {
        return None;
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.dev_fee_zero = true;
    f.make_args = |a| {
        // PerModel: $Intensities per device by memory size
        let intensities = a.devices.iter().map(|d| if d.OpenCL.GlobalMemSize >= 2 << 30 { "21" } else { "20" }).collect::<Vec<_>>().join(",");
        format!(
            "-R 1 -b $mport -a {} -d {} -q -o {}://{}:{} -u {}{}{} {}",
            a.main(),
            a.ids,
            a.pool.Protocol,
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            a.pass_opt(" -p "),
            opt(!intensities.is_empty(), format!(" -i {}", intensities)),
            a.params()
        )
    };
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerXevan.ps1
static CMD_CCMINER_XEVAN: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerXevan.json")));

fn ccminer_xevan(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CcminerXevan", "NVIDIA", &CMD_CCMINER_XEVAN);
    f.version = "1.3".into();
    f.port = "118".into();
    f.dev_fee = 0.0;
    f.path = "Bin/NVIDIA-Xevan/ccminer_x86.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.3-ccminerxevan/ccminerxevan_1.3.7z".into();
    let cuda = "9.0";
    if !info_only && !ctx.cuda_ok(cuda, "CcminerXevan") {
        return None;
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.make_args = |a| format!("-R 1 -b $mport -d {} -a {} -q -o {}://{}:{} -u {}{} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerYescrypt.ps1
static CMD_CCMINER_YESCRYPT: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerYescrypt.json")));

fn ccminer_yescrypt(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerYescrypt", "NVIDIA", &CMD_CCMINER_YESCRYPT);
    f.port = "129".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.manual_uri = "https://github.com/KlausT/ccminer/releases".into();
        f.version = "8.25-yescryptv3".into();
        f.path = "Bin/NVIDIA-YesCrypt/ccminer".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.25-klaust/ccminer-825-yescrypt3-cuda101-linux.7z", "10.1"),
        ]);
    } else {
        f.manual_uri = "https://github.com/nemosminer/ccminer-KlausT-8.21-mod-r18-src-fix/releases".into();
        f.version = "8.21-r18v10".into();
        f.path = "Bin/NVIDIA-YesCrypt/ccminer.exe".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v10-ccmineryescrypt/ccminerKlausTyescryptv10.7z", "10.0"),
        ]);
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.day_penalty = true;
    f.max_rej_default = Some(0.5);
    f.mining_priority = Some(2);
    f.emits_max_rej = true;
    f.make_args = |a| format!("--no-cpu-verify -N 1 -R 1 -b $mport -d {} -a {} -q -o {}://{}:{} -u {}{} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerYescryptR8.ps1
static CMD_CCMINER_YESCRYPT_R8: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerYescryptR8.json")));

fn ccminer_yescrypt_r8(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerYescryptR8", "NVIDIA", &CMD_CCMINER_YESCRYPT_R8);
    f.port = "137".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.manual_uri = "https://github.com/Kudaraidee/ccminer-KlausT-8.21-mod-koto-sapling-2/releases".into();
        f.version = "8.21-r17kotov2".into();
        f.path = "Bin/NVIDIA-YesCryptR8/ccminer".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.21r17kotov2-ccminerklaust/ccminer-KlausT-8.21-mod-r17-koto-sapling-2.7z", "9.1"),
        ]);
    } else {
        f.manual_uri = "https://github.com/Minerx117/ccmineryescryptr8g/releases".into();
        f.version = "8.21-r18v5".into();
        f.path = "Bin/NVIDIA-YesCryptR8/ccminer.exe".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v8.21r18v5-ccminerklaust/ccmineryescryptrV5.7z", "10.1"),
        ]);
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.day_penalty = true;
    f.max_rej_default = Some(0.5);
    f.mining_priority = Some(2);
    f.emits_max_rej = true;
    f.make_args = |a| format!("--no-cpu-verify -N 1 -R 1 -b $mport -d {} -q -o {}://{}:{} -u {}{} {}", a.ids, a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CcminerZP.ps1
static CMD_CCMINER_ZP: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CcminerZP.json")));

fn ccminer_zp(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CcminerZP", "NVIDIA", &CMD_CCMINER_ZP);
    f.version = "8.23".into();
    f.port = "144".into();
    f.dev_fee = 0.0;
    let cuda = "9.0";
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-CcminerZP/ccminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v8.23-zp/ZenProtocol-ccminer-8.23-x64.tar.gz".into();
    } else {
        f.path = "Bin/NVIDIA-CcminerZP/ccminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v8.23-zp/ZenProtocol-ccminer-8.23-Winx64.zip".into();
    }
    if !info_only && !ctx.cuda_ok(cuda, "CcminerZP") {
        return None;
    }
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.arch_in = &["Other", "Pascal"];
    f.make_args = |a| format!("-R 1 -b $mport -d {} -a {} -q -o {}://{}:{} -u {}{} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/Suprminer.ps1
static CMD_SUPRMINER: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Suprminer.json")));

fn suprminer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("Suprminer", "NVIDIA", &CMD_SUPRMINER);
    f.manual_uri = "https://github.com/ocminer/suprminer-releases".into();
    f.version = "2.3.1".into();
    f.port = "107".into();
    f.dev_fee = 1.0;
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-Suprminer/suprminer".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.3.1-suprminer/suprminer-cuda-11-3.tar.gz", "11.3"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.3.1-suprminer/suprminer-cuda-11-2.tar.gz", "11.2"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.3.1-suprminer/suprminer-cuda-9.1.tar.gz", "9.1"),
        ]);
    } else {
        f.path = "Bin/NVIDIA-Suprminer/suprminer.exe".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.3.1-suprminer/suprminer-winx86_64_cuda11_1_v2.7z", "11.1"),
        ]);
    }
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.win_or_single_dev = true;
    f.dev_fee_zero = true;
    f.mining_priority = Some(2);
    f.make_args = |a| format!("-b $mport -d {} -o {}://{}:{} -u {}{} -R 1 -q {}", a.ids, a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.params());
    f.pre_key_filter = Some(|ctx, d, cmd| ctx.test_vram(d, crate::jsonx::num(crate::jsonx::get(cmd, "MinMemGB"))));
    let _ = info_only;
    Some(f)
}

// ---- Miners/CryptoDredge16.ps1
static CMD_CRYPTO_DREDGE16: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CryptoDredge16.json")));

fn crypto_dredge16(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() || ctx.is_vm {
        return None;
    }
    let mut f = Family::new("CryptoDredge16", "NVIDIA", &CMD_CRYPTO_DREDGE16);
    f.manual_uri = "https://bitcointalk.org/index.php?topic=4807821".into();
    f.version = "0.16.0".into();
    f.port = "338".into();
    f.dev_fee = 1.0;
    f.path = "Bin/NVIDIA-CryptoDredge16/CryptoDredge.exe".into();
    f = f.cuda(&[
        ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.16.0-cryptodredge/CryptoDredge_0.16.0_cuda_10.0_windows.zip", "10.0"),
        ("https://github.com/RainbowMiner/miner-binaries/releases/download/v0.16.0-cryptodredge/CryptoDredge_0.16.0_cuda_9.2_windows.zip", "9.2"),
    ]);
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.dev_fee_cmd = true;
    f.mining_priority = Some(-1);
    // $Enable_Logfile = $false: no "Enable_Logfile" var
    f.make_args = |a| format!("-r 10 -R 1 -b 127.0.0.1:$mport -d {} -a {} --no-watchdog -o {}://{}:{} -u {}{}{} {} --no-nvml", a.ids, a.algorithm(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), opt(a.var_b("Enable_Logfile"), " --log log_$mport.txt".to_string()), a.params());
    // $DeviceCapability = "5.0"
    f.model_filter = Some(|_, d| capability_ok(d, "5.0") && ["Other", "Pascal", "Turing"].contains(&d.OpenCL.Architecture.as_str()));
    let _ = info_only;
    Some(f)
}

// ---- Miners/EnemyZealot.ps1
static CMD_ENEMY_ZEALOT: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/EnemyZealot.json")));

fn enemy_zealot(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("EnemyZealot", "NVIDIA", &CMD_ENEMY_ZEALOT);
    f.manual_uri = "https://github.com/z-enemy/z-enemy/releases".into();
    f.version = "2.6.2".into();
    f.port = "302".into();
    f.dev_fee = 1.0;
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-enemyz/z-enemy".into();
        f = f.cuda(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.6.2-zenemy/z-enemy-2.6.2-cuda101-libcurl4.tar.gz", "10.1"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.6.2-zenemy/z-enemy-2.6.2-cuda100-libcurl4.tar.gz", "10.0"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.6.2-zenemy/z-enemy-2.6.2-cuda92-libcurl4.tar.gz", "9.2"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.6.2-zenemy/z-enemy-2.6.2-cuda91.tar.gz", "9.1"),
        ]);
    } else {
        f.path = "Bin/NVIDIA-enemyz/z-enemy.exe".into();
        f = f.cuda_v(&[
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.6.3-zenemy/z-enemy-2.6.3-win-cuda11.1.zip", "11.1", "2.6.3"),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.6.2-zenemy/z-enemy-2.6.2-win-cuda10.1.zip", "10.1", ""),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.6.2-zenemy/z-enemy-2.6.2-win-cuda10.0.zip", "10.0", ""),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.6.2-zenemy/z-enemy-2.6.2-win-cuda9.2.zip", "9.2", ""),
            ("https://github.com/RainbowMiner/miner-binaries/releases/download/v2.6.2-zenemy/z-enemy-2.6.2-win-cuda9.1.zip", "9.1", ""),
        ]);
    }
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.api = "EnemyZ";
    f.extend_default = 2;
    // MakeArgs with the failover pools of the pool
    f.make_args = |a| {
        format!(
            "-R 1 --api-bind=0 --api-bind-http=$mport -d {} -a {} -o {}://{}:{} -u {}{}{} --no-cert-verify {}",
            a.ids,
            a.algorithm(),
            a.pool.Protocol,
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            a.pass_opt(" -p "),
            super::failover_args(a.pool),
            a.params()
        )
    };
    // PerModel: $Miner_AllowAmpere = (Compare-Version $Version "2.6.3") -ge 0
    f.per_key_filter = Some(|fam, ctx, d, cmd, pool, algo0| {
        let Some(pool) = pool else { return false };
        let allow_ampere = crate::util::compare_version(&fam.version, "2.6.3", 0) >= 0;
        ctx.test_vram(d, ctx.min_mem_gb(cmd, pool, algo0)) && (allow_ampere || d.OpenCL.Architecture != "Ampere")
    });
    let _ = info_only;
    Some(f)
}

// ---- Miners/Ewbf.ps1
static CMD_EWBF: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Ewbf.json")));

fn ewbf(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("Ewbf", "NVIDIA", &CMD_EWBF);
    f.manual_uri = "https://bitcointalk.org/index.php?topic=4466962.0".into();
    f.version = "0.6".into();
    f.port = "311".into();
    f.dev_fee = 0.0;
    let cuda = "8.0";
    if ctx.is_linux() {
        f.path = "Bin/Equihash-EWBF/miner".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.6-ewbf/EWBF_Equihash_miner_v0.6.tar.gz".into();
    } else {
        f.path = "Bin/Equihash-EWBF/miner.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.6-ewbf/EWBF.Equihash.miner.v0.6.zip".into();
    }
    if !info_only && !ctx.cuda_ok(cuda, "Ewbf") {
        return None;
    }
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.api = "DSTM";
    f.dev_id_join = " ";
    f.extend_default = 2;
    f.make_args = |a| {
        format!(
            "--api 127.0.0.1:$mport --cuda_devices {} --server {} --port {} --fee 0 --eexit 1 --user {}{}{} {}",
            a.ids,
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            a.pass_opt(" --pass "),
            opt(ps_match(a.key, "^Equihash"), format!(" --pers {}", crate::data::get_equihash_coin_pers(&a.pool.CoinSymbol, "auto"))),
            a.params()
        )
    };
    f.pre_key_filter = Some(|ctx, d, cmd| ctx.test_vram(d, crate::jsonx::num(crate::jsonx::get(cmd, "MinMemGB"))));
    let _ = info_only;
    Some(f)
}

// ---- Miners/EwbfZcash.ps1
static CMD_EWBF_ZCASH: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/EwbfZcash.json")));

fn ewbf_zcash(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("EwbfZcash", "NVIDIA", &CMD_EWBF_ZCASH);
    f.manual_uri = "https://bitcointalk.org/index.php?topic=1707546.0".into();
    f.version = "0.3.4b".into();
    f.port = "351".into();
    f.dev_fee = 0.0;
    let cuda = "8.0";
    if ctx.is_linux() {
        f.path = "Bin/NVIDIA-EWBFZcash/miner".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.3.4b-ewbf/EWBF-nvidia-linux.0.3.4b.7z".into();
    } else {
        f.path = "Bin/NVIDIA-EWBFZcash/miner.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.3.4b-ewbf/EWBF-nvidia-win.0.3.4b.7z".into();
    }
    if !info_only && !ctx.cuda_ok(cuda, "EwbfZcash") {
        return None;
    }
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.api = "DSTM";
    f.dev_id_join = " ";
    f.extend_default = 2;
    f.make_args = |a| format!("--api 127.0.0.1:$mport --cuda_devices {} --server {} --port {} --fee 0 --eexit 1 --user {}{} {}", a.ids, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" --pass "), a.params());
    f.pre_key_filter = Some(|ctx, d, cmd| ctx.test_vram(d, crate::jsonx::num(crate::jsonx::get(cmd, "MinMemGB"))));
    let _ = info_only;
    Some(f)
}

// ---- Miners/HtnNvidia.ps1
static CMD_HTN_NVIDIA: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/HtnNvidia.json")));

fn htn_nvidia(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    if ctx.libc_below("2.30") {
        return None;
    }
    let mut f = Family::new("HtnNvidia", "NVIDIA", &CMD_HTN_NVIDIA);
    f.manual_uri = "https://htn.foztor.net/".into();
    f.version = "1.4.9".into();
    f.port = "379".into();
    f.dev_fee = 1.5;
    f.path = "Bin/NVIDIA-Htn/hoo_gpu".into();
    f = f.cuda(&[
        ("https://github.com/RainbowMiner/miner-binaries/releases/download/v1.4.9-htm/hoo_gpu-1.4.9.tar.gz", "12.0"),
    ]);
    f.suffix = Suffix::KeysNoGPU;
    f.use_exclude_pool = true;
    f.extend_default = 2;
    f.env_vars = vec!["LD_LIBRARY_PATH=./libs".into()];
    f.make_args = |a| format!("-b 127.0.0.1:$mport -d {} -o {}://{}:{} -u {}{}{} -q {}", a.ids, a.pool.Protocol, a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    let _ = info_only;
    Some(f)
}

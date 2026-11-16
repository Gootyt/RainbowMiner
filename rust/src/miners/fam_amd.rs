//! generated from the Invoke-MinerFamily stubs, see family.rs

use super::family::*;
use super::opt;
use crate::miner::*;
use serde_json::Value;
use std::sync::LazyLock;

pub fn modules() -> Vec<FamilyModule> {
    vec![
        FamilyModule { name: "Sgminer", types: &["AMD"], def: sgminer },
        FamilyModule { name: "SgminerBitcore", types: &["AMD"], def: sgminer_bitcore },
        FamilyModule { name: "SgminerDyn", types: &["AMD"], def: sgminer_dyn },
        FamilyModule { name: "SgminerKl", types: &["AMD"], def: sgminer_kl },
        FamilyModule { name: "SgminerLyra2z", types: &["AMD"], def: sgminer_lyra2z },
        FamilyModule { name: "SgminerMTP", types: &["AMD"], def: sgminer_mtp },
        FamilyModule { name: "SgminerTcr", types: &["AMD"], def: sgminer_tcr },
        FamilyModule { name: "SgminerXevan", types: &["AMD"], def: sgminer_xevan },
        FamilyModule { name: "Nsgminer", types: &["AMD"], def: nsgminer },
        FamilyModule { name: "Cast", types: &["AMD"], def: cast },
        FamilyModule { name: "ClaymoreNeoscryptAmd", types: &["AMD"], def: claymore_neoscrypt_amd },
    ]
}

// ---- Miners/Sgminer.ps1
static CMD_SGMINER: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Sgminer.json")));

fn sgminer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("Sgminer", "AMD", &CMD_SGMINER);
    f.manual_uri = "https://github.com/nicehash/sgminer/releases".into();
    f.version = "5.6.1".into();
    f.port = "400".into();
    f.dev_fee = 1.0;
    f.path = "Bin/AMD-NiceHash/sgminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v5.6.1-sgminer/sgminer-5.6.1-nicehash-51-windows-amd64.zip".into();
    f.suffix = Suffix::ListGPU;
    f.api = "Xgminer";
    f.needs_platform_id = true;
    f.dev_id_prop = "Type_PlatformId_Index";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    f.make_args = |a| format!("--device {} --api-port $mport --api-listen -k {} -o {}://{}:{} -u {}{} --text-only --gpu-platform {} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.platform_id, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/SgminerBitcore.ps1
static CMD_SGMINER_BITCORE: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SgminerBitcore.json")));

fn sgminer_bitcore(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("SgminerBitcore", "AMD", &CMD_SGMINER_BITCORE);
    f.version = "5.6.1.9".into();
    f.port = "401".into();
    f.dev_fee = 1.0;
    f.path = "Bin/AMD-Bitcore/sgminer-x64.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v5.6.1.9-sgminerbitcore/sgminer-bitcore-5.6.1.9.zip".into();
    f.suffix = Suffix::ListGPU;
    f.api = "Xgminer";
    f.needs_platform_id = true;
    f.dev_id_prop = "Type_PlatformId_Index";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    f.make_args = |a| format!("--device {} --api-port $mport --api-listen -k {} -o {}://{}:{} -u {}{} --text-only --gpu-platform {} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.platform_id, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/SgminerDyn.ps1
static CMD_SGMINER_DYN: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SgminerDyn.json")));

fn sgminer_dyn(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("SgminerDyn", "AMD", &CMD_SGMINER_DYN);
    f.manual_uri = "https://github.com/duality-solutions/Dynamic-GPU-Miner-AMD/releases".into();
    f.version = "1.0.0".into();
    f.port = "410".into();
    f.dev_fee = 0.0;
    f.path = "Bin/AMD-SgminerDyn/sgminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0-sgminerdyn/sgminer-dyn-1.0.0-windows-x64.zip".into();
    f.suffix = Suffix::ListGPU;
    f.api = "Xgminer";
    f.needs_platform_id = true;
    f.dev_id_prop = "Type_PlatformId_Index";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    f.make_args = |a| format!("--device {} --api-port $mport --api-listen -o {}://{}:{} -u {}{} --text-only --gpu-platform {} {}", a.ids, a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.platform_id, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/SgminerKl.ps1
static CMD_SGMINER_KL: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SgminerKl.json")));

fn sgminer_kl(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("SgminerKl", "AMD", &CMD_SGMINER_KL);
    f.manual_uri = "https://github.com/KL0nLutiy/sgminer-kl/releases".into();
    f.version = "1.0.9".into();
    f.port = "402".into();
    f.dev_fee = 1.0;
    f.path = "Bin/AMD-SgminerKl/sgminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.9-sgminerkl/sgminer-kl-1.0.9-windows.zip".into();
    f.suffix = Suffix::ListGPU;
    f.api = "Xgminer";
    f.needs_platform_id = true;
    f.dev_id_prop = "Type_PlatformId_Index";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    f.make_args = |a| format!("--device {} --api-port $mport --api-listen -k {} -o {}://{}:{} -u {}{} --text-only --gpu-platform {} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.platform_id, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/SgminerLyra2z.ps1
static CMD_SGMINER_LYRA2Z: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SgminerLyra2z.json")));

fn sgminer_lyra2z(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("SgminerLyra2z", "AMD", &CMD_SGMINER_LYRA2Z);
    f.manual_uri = "https://github.com/djm34/sgminer-msvc2015/releases".into();
    f.version = "0.3".into();
    f.port = "403".into();
    f.dev_fee = 1.0;
    f.path = "Bin/AMD-Lyra2z/sgminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.3-sgminerlyra2z/kernel.rar".into();
    f.suffix = Suffix::ListGPU;
    f.api = "Xgminer";
    f.needs_platform_id = true;
    f.dev_id_prop = "Type_PlatformId_Index";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    f.make_args = |a| format!("--device {} --api-port $mport --api-listen -k {} -o {}://{}:{} -u {}{} --text-only --gpu-platform {} {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.platform_id, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/SgminerMTP.ps1
static CMD_SGMINER_MTP: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SgminerMTP.json")));

fn sgminer_mtp(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("SgminerMTP", "AMD", &CMD_SGMINER_MTP);
    f.manual_uri = "https://github.com/zcoinofficial/sgminer/releases".into();
    f.version = "0.1.3".into();
    f.port = "411".into();
    f.dev_fee = 0.0;
    f.path = "Bin/AMD-SgminerMTP/sgminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.1.3-sgminermtp/sgminermtp-0.1.3.zip".into();
    f.suffix = Suffix::ListGPU;
    f.api = "Xgminer";
    f.needs_platform_id = true;
    f.dev_id_prop = "Type_PlatformId_Index";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    f.make_args = |a| {
        format!(
            "{} --device {} --api-port $mport --api-listen -o {}://{}:{} -u {}{} --gpu-platform {}",
            a.params_vega(),
            a.ids,
            a.pool.Protocol,
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            a.pass_opt(" -p "),
            a.platform_id
        )
    };
    let _ = info_only;
    Some(f)
}

// ---- Miners/SgminerTcr.ps1
static CMD_SGMINER_TCR: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SgminerTcr.json")));

fn sgminer_tcr(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("SgminerTcr", "AMD", &CMD_SGMINER_TCR);
    f.manual_uri = "https://github.com/tecracoin/sgminer/releases".into();
    f.port = "414".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.version = "0.1.4".into();
        f.path = "Bin/AMD-SgminerTCR/sgminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.1.4-sgminertcr/sgminertcr-v0.1.4-linux.7z".into();
    } else {
        f.version = "0.1.7".into();
        f.path = "Bin/AMD-SgminerTCR/sgminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.1.7-sgminertcr/sgminertcr-v0.1.7-win64.zip".into();
    }
    f.suffix = Suffix::ListGPU;
    f.api = "Xgminer";
    f.needs_platform_id = true;
    f.dev_id_prop = "Type_PlatformId_Index";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into(), "GPU_MAX_SINGLE_ALLOC_PERCENT=100".into()];
    f.make_args = |a| {
        format!(
            "{} --device {} --api-port $mport --api-listen -o {}://{}:{} -u {}{} --gpu-platform {}",
            a.params_vega(),
            a.ids,
            a.pool.Protocol,
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            a.pass_opt(" -p "),
            a.platform_id
        )
    };
    let _ = info_only;
    Some(f)
}

// ---- Miners/SgminerXevan.ps1
static CMD_SGMINER_XEVAN: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/SgminerXevan.json")));

fn sgminer_xevan(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("SgminerXevan", "AMD", &CMD_SGMINER_XEVAN);
    f.manual_uri = "https://github.com/LIMXTEC/Xevan-GPU-Miner/releases".into();
    f.version = "5.5.0".into();
    f.port = "405".into();
    f.dev_fee = 1.0;
    f.path = "Bin/Xevan-AMD/sgminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v5.5.0-sgminerxevan/sgminer-xevan-5.5.0-nicehash-1-windows-amd64.zip".into();
    f.suffix = Suffix::ListGPU;
    f.api = "Xgminer";
    f.needs_platform_id = true;
    f.dev_id_prop = "Type_PlatformId_Index";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    f.make_args = |a| format!("--device {} --api-port $mport --api-listen -k {} -o {}://{}:{} -u {}{} --text-only --gpu-platform {}  {}", a.ids, a.main(), a.pool.Protocol, a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.platform_id, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/Nsgminer.ps1
static CMD_NSGMINER: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Nsgminer.json")));

fn nsgminer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("Nsgminer", "AMD", &CMD_NSGMINER);
    f.manual_uri = "https://github.com/ghostlander/nsgminer/releases".into();
    f.version = "0.9.4".into();
    f.port = "406".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = "Bin/AMD-Nsgminer/nsgminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.9.4-nsgminer/nsgminer-lin64-0.9.4.7z".into();
    } else {
        f.path = "Bin/AMD-Nsgminer/nsgminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.9.4-nsgminer/nsgminer-win64-0.9.4.zip".into();
    }
    f.suffix = Suffix::ListGPU;
    f.api = "Xgminer";
    f.needs_platform_id = true;
    f.dev_id_prop = "Type_PlatformId_Index";
    f.dev_id_join = " -d ";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    f.make_args = |a| {
        format!(
            "--{} -d {} --api-port $mport --api-listen -o {}://{}:{} -u {}{} --gpu-platform {} {}",
            a.main(),
            a.ids,
            a.pool.Protocol,
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            a.pass_opt(" -p "),
            a.platform_id,
            a.params_vega()
        )
    };
    let _ = info_only;
    Some(f)
}

// ---- Miners/Cast.ps1
static CMD_CAST: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Cast.json")));

fn cast(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("Cast", "AMD", &CMD_CAST);
    f.port = "306".into();
    f.dev_fee = 1.0;
    if ctx.is_linux() {
        f.version = "1.7.1".into();
        f.path = "Bin/CryptoNight-Cast/cast_xmr-vega".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.71-cast/cast_xmr-vega-ubuntu_171.tar.gz".into();
    } else {
        f.version = "1.8.0".into();
        f.path = "Bin/CryptoNight-Cast/cast_xmr-vega.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.8.0-cast/cast_xmr-vega-win64_180.zip".into();
    }
    f.suffix = Suffix::ListGPU;
    f.first_per_cmd = true;
    f.api = "Cast";
    f.needs_platform_id = true;
    f.dev_id_prop = "Type_PlatformId_Index";
    f.dev_id_hex = true;
    f.make_args = |a| format!("--remoteaccess --remoteport $mport -S {}:{} -u {}{} --opencl {} -G {} --fastjobswitch --intensity -1{} {}", a.pool.Host, a.pool_port, a.pool.User, a.pass_opt(" -p "), a.platform_id, a.ids, opt(!ps_match(&a.pool.Host, "NiceHash"), " --nonicehash".to_string()), a.params());
    f.pre_key_filter = Some(|ctx, d, cmd| ctx.test_vram(d, crate::jsonx::num(crate::jsonx::get(cmd, "MinMemGB"))) && ps_match(&d.OpenCL.Name, "^(Ellesmere|Polaris|Vega|gfx900)"));
    let _ = info_only;
    Some(f)
}

// ---- Miners/ClaymoreNeoscryptAmd.ps1
static CMD_CLAYMORE_NEOSCRYPT_AMD: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/ClaymoreNeoscryptAmd.json")));

fn claymore_neoscrypt_amd(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("ClaymoreNeoscryptAmd", "AMD", &CMD_CLAYMORE_NEOSCRYPT_AMD);
    f.manual_uri = "https://bitcointalk.org/index.php?topic=3012600.0".into();
    f.version = "1.2".into();
    f.port = "202".into();
    f.dev_fee = 2.0;
    f.path = "Bin/NeoScrypt-Claymore/NeoScryptMiner.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2-claymoreneoscrypt/claymore_neoscrypt_1.2.zip".into();
    f.suffix = Suffix::ListGPU;
    f.check_ssl = true;
    f.api = "Claymore";
    f.dev_id_hex = true;
    f.dev_id_join = "";
    f.env_vars = vec!["GPU_FORCE_64BIT_PTR=0".into()];
    f.make_args = |a| format!("-r -1 -mport -$mport -pool {}://{}:{} -wal {} -psw {} -di {} {}", a.pool.Protocol, a.pool.Host, a.pool.Port, a.pool.User, a.pool.Pass, a.ids, a.params());
    f.model_filter = Some(|_, d| !d.Model.eq_ignore_ascii_case("gfx1010") && !ps_match(&d.Model, r"^RX[56]\d00"));
    let _ = info_only;
    Some(f)
}

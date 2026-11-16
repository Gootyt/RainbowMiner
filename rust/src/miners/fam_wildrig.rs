//! Miners/WildRig.ps1, WildRig0340.ps1, WildRig0445.ps1, WildrigNg.ps1 (AMD, INTEL, NVIDIA)

use super::family::*;
use super::opt;
use crate::device::Device;
use crate::miner::*;
use crate::pool::Pool;
use serde_json::Value;
use std::sync::LazyLock;

pub fn modules() -> Vec<FamilyModule> {
    vec![
        FamilyModule { name: "WildRig", types: &["AMD", "INTEL", "NVIDIA"], def: wildrig },
        FamilyModule { name: "WildRig0340", types: &["AMD", "NVIDIA"], def: wildrig0340 },
        FamilyModule { name: "WildRig0445", types: &["AMD", "INTEL", "NVIDIA"], def: wildrig0445 },
        FamilyModule { name: "WildrigNg", types: &["AMD", "INTEL", "NVIDIA"], def: wildrig_ng },
    ]
}

static CMD_WR: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/WildRig.json")));
static CMD_WR340: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/WildRig0340.json")));
static CMD_WR445: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/WildRig0445.json")));
static CMD_NG: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/WildrigNg.json")));

/// $_.Vendor -icontains $Miner_Vendor and the Version (and NeverProfitable) filter of the commands
fn cmd_ok(fam: &Family, ctx: &MinerCtx, cmd: &Value) -> bool {
    let v = cmd_s(cmd, "Version");
    if !v.is_empty() && crate::util::compare_version(&fam.version, &v, 0) < 0 {
        return false;
    }
    if fam.name == "WildrigNg" && cmd_b(cmd, "NeverProfitable") && !ctx.config.b("EnableNeverprofitableAlgos") {
        return false;
    }
    true
}

/// MinMemGB (DAG) and the compute capability filters of a command
fn key_filter(fam: &Family, ctx: &MinerCtx, d: &Device, cmd: &Value, pool: Option<&Pool>, algo0: &str) -> bool {
    let Some(pool) = pool else { return false };
    if !cmd_vendor(cmd, &d.Vendor) || !ctx.test_vram(d, ctx.min_mem_gb(cmd, pool, algo0)) {
        return false;
    }
    if fam.name == "WildRig0340" {
        return true;
    }
    let cap = d.OpenCL.DeviceCapability.clone().unwrap_or_default();
    let arch = &d.OpenCL.Architecture;
    let m = |v: &str, pat: &str| !pat.is_empty() && ps_match(v, pat);
    match d.Vendor.as_str() {
        "AMD" => {
            let amd_compute = cmd_s(cmd, "AmdCompute");
            if !amd_compute.is_empty() && !m(&cap, &amd_compute) {
                return false;
            }
            let caps = cmd_list(cmd, "AmdCapability");
            let amd_cap = if fam.name == "WildRig0445" { caps.first().cloned().unwrap_or_default() } else { caps.join("|") };
            if !amd_cap.is_empty() && !m(&cap, &amd_cap) {
                return false;
            }
            let ex = cmd_s(cmd, "ExcludeAmdCapability");
            if !ex.is_empty() && m(&cap, &ex) {
                return false;
            }
        }
        "NVIDIA" => {
            let nv = cmd_s(cmd, "NvCompute");
            if !nv.is_empty() && !m(arch, &nv) {
                return false;
            }
            let ex = cmd_s(cmd, "ExcludeNvCompute");
            if !ex.is_empty() && m(arch, &ex) {
                return false;
            }
        }
        _ => {}
    }
    true
}

/// $WatchdogParams + scratchpad + $_.Params
fn params(a: &A) -> String {
    let wd = if a.ctx.config.b("RebootOnGPUFailure") && a.ctx.config.b("EnableRestartComputer") {
        format!("--watchdog-script='{}' ", a.ctx.base.join(if cfg!(windows) { "Reboot.bat" } else { "reboot.sh" }).display())
    } else {
        String::new()
    };
    let sp = if a.pool.ScratchPadUrl.is_empty() {
        String::new()
    } else {
        format!("--scratchpad-url {} --scratchpad-file scratchpad-{}.bin ", a.pool.ScratchPadUrl, a.pool.CoinSymbol.to_lowercase())
    };
    format!("{}{}{}", wd, sp, a.params())
}

fn platforms(vendor: &str) -> &'static str {
    match vendor {
        "AMD" => "--opencl-platforms amd",
        "INTEL" => "--opencl-platforms intel",
        "NVIDIA" => "--opencl-platforms nvidia",
        _ => "",
    }
}

fn base(ctx: &MinerCtx, info_only: bool, name: &'static str, cmds: &'static LazyLock<Vec<Value>>, cuda: &str, vendors: &[&'static str]) -> Family {
    let mut f = Family::new(name, "AMD", cmds);
    f.info_types = vendors.iter().map(|v| v.to_string()).collect();
    let cuda_ok = !info_only && ctx.has_type("NVIDIA") && ctx.cuda_ok(cuda, name);
    f.vendors = vendors.iter().copied().filter(|v| *v != "NVIDIA" || cuda_ok || !ctx.has_type("NVIDIA")).collect();
    f.suffix = Suffix::KeysCoin;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.use_coin_symbols = true;
    f.api = "Xmrig";
    f.dev_id_prop = "BusId_Type_Vendor_Index";
    f.env_vars = vec!["GPU_MAX_WORKGROUP_SIZE=256".into()];
    f.cmd_ok = Some(cmd_ok);
    f.per_key_filter = Some(key_filter);
    f.post = Some(|a, m| {
        // DevFee = if ($_.DevFee) {$_.DevFee} else {$DevFee}
        let fee = cmd_f(a.cmd, "DevFee").filter(|x| *x != 0.0).unwrap_or(a.fam.dev_fee);
        m.dev_fee.values_mut().for_each(|v| *v = fee);
        if a.fam.name == "WildRig0340" {
            m.list_devices = "--print-devices".into();
        }
    });
    f
}

fn win_linux_x64(ctx: &MinerCtx) -> bool {
    (ctx.is_windows() || ctx.is_linux()) && !(ctx.is_linux() && ctx.is_arm())
}

fn wildrig(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = base(ctx, info_only, "WildRig", &CMD_WR, "11.0", &["AMD", "INTEL", "NVIDIA"]);
    f.manual_uri = "https://bitcointalk.org/index.php?topic=5023676.0".into();
    f.port = "407".into();
    f.version = "0.52.2".into();
    if ctx.is_linux() {
        f.path = "Bin/GPU-WildRig/wildrig-multi".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.52.2-wildrigmulti/wildrig-multi-linux-0.52.2.tar.gz".into();
    } else {
        f.path = "Bin/GPU-WildRig/wildrig.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.52.2-wildrigmulti/wildrig-multi-windows-0.52.2.zip".into();
    }
    // the --worker of the original needs a pool "Password" field, which no pool sets
    f.make_args = |a| {
        format!(
            "--api-port $mport -a {} -o stratum+tcp{}://{}:{} -u {}{} -r 4 -R 5 --max-rejects 10 --multiple-instance --opencl-devices {} {} --opencl-threads auto --gpu-temp-limit=95 {}",
            a.algorithm(),
            opt(a.pool.SSL, "s".into()),
            a.pool.Host,
            a.pool_port,
            strip_nexa(&a.pool.User),
            a.pass_opt(" -p "),
            a.ids,
            platforms(a.vendor),
            params(a)
        )
    };
    Some(f)
}

fn wildrig0445(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = base(ctx, info_only, "WildRig0445", &CMD_WR445, "11.0", &["AMD", "INTEL", "NVIDIA"]);
    f.manual_uri = "https://bitcointalk.org/index.php?topic=5023676.0".into();
    f.port = "420".into();
    f.version = "0.44.5".into();
    if ctx.is_linux() {
        f.path = "Bin/GPU-WildRig0445/wildrig-multi".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.44.5-wildrigmulti/wildrig-multi-linux-0.44.5.tar.xz".into();
    } else {
        f.path = "Bin/GPU-WildRig0445/wildrig.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.44.5-wildrigmulti/wildrig-multi-windows-0.44.5.zip".into();
    }
    f.make_args = |a| {
        // the CMP cards need the second qhash kernel (one device only, like -in on the device list)
        let qhash = a.vendor == "NVIDIA"
            && a.main() == "qhash"
            && a.devices.len() == 1
            && ["Volta", "Turing", "Ampere"].contains(&a.devices[0].OpenCL.Architecture.as_str())
            && ps_match(&a.devices[0].Model_Base, "^CMP");
        format!(
            "--api-port $mport -a {} -o stratum+tcp{}://{}:{} -u {}{} -r 4 -R 5 --max-rejects 10 --multiple-instance --opencl-devices {} {}{} --opencl-threads auto --gpu-temp-limit=95 {}",
            a.algorithm(),
            opt(a.pool.SSL, "s".into()),
            a.pool.Host,
            a.pool_port,
            strip_nexa(&a.pool.User),
            a.pass_opt(" -p "),
            a.ids,
            platforms(a.vendor),
            opt(qhash, " --qhash-kernel 2".into()),
            params(a)
        )
    };
    Some(f)
}

fn wildrig0340(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = base(ctx, info_only, "WildRig0340", &CMD_WR340, "8.0", &["AMD", "NVIDIA"]);
    f.manual_uri = "https://bitcointalk.org/index.php?topic=5023676.0".into();
    f.port = "419".into();
    f.dev_fee = 1.0;
    f.version = "0.34.0".into();
    f.api = "XMRig";
    if ctx.is_linux() {
        f.path = "Bin/GPU-WildRig0340/wildrig-multi".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.34.0-wildrigmulti/wildrig-multi-linux-0.34.0.tar.xz".into();
    } else {
        f.path = "Bin/GPU-WildRig0340/wildrig.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.34.0-wildrigmulti/wildrig-multi-windows-0.34.0.7z".into();
    }
    f.make_args = |a| {
        format!(
            "--api-port $mport -a {} -o stratum+tcp{}://{}:{} -u {}{} -r 4 -R 5 --max-rejects 10 --donate-level 1 --multiple-instance --opencl-devices {} {} --opencl-threads auto --opencl-launch auto --gpu-temp-limit=95 {}",
            a.algorithm(),
            opt(a.pool.SSL, "s".into()),
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            a.pass_opt(" -p "),
            a.ids,
            platforms(a.vendor),
            params(a)
        )
    };
    Some(f)
}

/// --gpu-list index: the vendor-local bus order without the Intel iGPUs (bus 00)
fn ng_index(ctx: &MinerCtx, d: &Device) -> Option<usize> {
    let igpu = |x: &Device| x.Vendor == "INTEL" && x.BusId.as_deref().unwrap_or("").starts_with("00:");
    if igpu(d) {
        return None;
    }
    let n = if d.Vendor == "INTEL" { ctx.devices.all_devices.iter().filter(|x| x.is_gpu() && igpu(x)).count() } else { 0 };
    Some(d.BusId_Type_Vendor_Index.saturating_sub(n))
}

fn wildrig_ng(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !(ctx.is_windows() || ctx.is_linux()) || (ctx.is_linux() && ctx.is_arm() && ctx.cpu.Architecture != "8") {
        return None;
    }
    let mut f = base(ctx, info_only, "WildrigNg", &CMD_NG, "11.0", &["AMD", "INTEL", "NVIDIA"]);
    f.manual_uri = "https://github.com/andru-kun/wildrig-ng".into();
    f.port = "421".into();
    f.version = "1.0.0u".into();
    if ctx.is_linux() {
        f.path = "Bin/GPU-WildrigNg/wildrig-ng".into();
        f.uri = if ctx.is_arm() {
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0u-wildrigng/wildrig-ng-linux-aarch64-1.0.0u.tar.gz"
        } else {
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0u-wildrigng/wildrig-ng-linux-1.0.0u.tar.gz"
        }
        .into();
    } else {
        f.path = "Bin/GPU-WildrigNg/wildrig-ng.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.0.0u-wildrigng/wildrig-ng-windows-1.0.0u.zip".into();
    }
    f.model_filter = Some(|_, d| d.Vendor != "INTEL" || !d.BusId.as_deref().unwrap_or("").starts_with("00:"));
    f.per_key_filter = Some(|fam, ctx, d, cmd, pool, algo0| ng_index(ctx, d).is_some() && key_filter(fam, ctx, d, cmd, pool, algo0));
    f.make_args = |a| {
        let mut idx: Vec<usize> = a.devices.iter().filter_map(|d| ng_index(a.ctx, d)).collect();
        idx.sort();
        let dp = match a.vendor {
            "AMD" => "--gpu-no-intel --gpu-no-nvidia",
            "INTEL" => "--gpu-no-amd --gpu-no-nvidia",
            "NVIDIA" => "--gpu-no-amd --gpu-no-intel",
            _ => "",
        };
        format!(
            "--api-port $mport -a {} -o stratum+{}://{}:{} -u {}{} --pool-retries 4 --pool-retry-pause 5 --pool-max-rejects 10 --gpu-list {} {} --gpu-temp-limit 95 --ui-plain --no-watchdog {}",
            a.algorithm(),
            if a.pool.SSL { "ssl" } else { "tcp" },
            a.pool.Host,
            a.pool_port,
            strip_nexa(&a.pool.User),
            a.pass_opt(" -p "),
            idx.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","),
            dp,
            a.params()
        )
    };
    Some(f)
}

/// $User -replace "^nexa:"
fn strip_nexa(u: &str) -> &str {
    if u.len() >= 5 && u[..5].eq_ignore_ascii_case("nexa:") {
        &u[5..]
    } else {
        u
    }
}

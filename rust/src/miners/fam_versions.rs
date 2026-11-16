//! Older miner versions kept for single algorithms: Gminer234/255/275/294 (Gminer API) and
//! SrbMinerMulti094 (GCN1-3 cards). None of their commands mines dual.

use super::family::*;
use super::opt;
use crate::device::Device;
use crate::miner::*;
use crate::pool::Pool;
use crate::util;
use serde_json::Value;
use std::sync::LazyLock;

pub fn modules() -> Vec<FamilyModule> {
    vec![
        FamilyModule { name: "Gminer234", types: &["AMD", "NVIDIA"], def: gminer234 },
        FamilyModule { name: "Gminer255", types: &["AMD", "NVIDIA"], def: gminer255 },
        FamilyModule { name: "Gminer275", types: &["AMD", "NVIDIA"], def: gminer275 },
        FamilyModule { name: "Gminer294", types: &["AMD", "NVIDIA"], def: gminer294 },
        FamilyModule { name: "SrbMinerMulti094", types: &["AMD"], def: srbminermulti094 },
    ]
}

macro_rules! cmds {
    ($c:ident, $f:literal) => {
        static $c: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!(concat!("../../assets/miners/", $f, ".json"))));
    };
}
cmds!(CMD_G234, "Gminer234");
cmds!(CMD_G255, "Gminer255");
cmds!(CMD_G275, "Gminer275");
cmds!(CMD_G294, "Gminer294");
cmds!(CMD_SRB094, "SrbMinerMulti094");

fn win_linux_x64(ctx: &MinerCtx) -> bool {
    (ctx.is_windows() || ctx.is_linux()) && !(ctx.is_linux() && ctx.is_arm())
}

/// CoinSymbol / ExcludeCoinSymbol of a command
fn coin_ok(cmd: &Value, p: &Pool) -> bool {
    let cs = cmd_list(cmd, "CoinSymbol");
    let ex = cmd_list(cmd, "ExcludeCoinSymbol");
    (cs.is_empty() || util::contains_ci(&cs, &p.CoinSymbol)) && (ex.is_empty() || !util::contains_ci(&ex, &p.CoinSymbol))
}

fn gminer_base(ctx: &MinerCtx, info_only: bool, name: &'static str, cmds: &'static LazyLock<Vec<Value>>, version: &str, port: &str, dir: &str) -> Family {
    let mut f = Family::new(name, "AMD", cmds);
    f.info_types = vec!["AMD".into(), "NVIDIA".into()];
    f.manual_uri = "https://github.com/develsoftware/GMinerRelease/releases".into();
    f.dev_fee = 2.0;
    f.version = version.into();
    f.port = port.into();
    let v = version.replace('.', "_");
    if ctx.is_linux() {
        f.path = format!("Bin/{}/miner", dir);
        f.uri = format!("https://github.com/RainbowMiner/miner-binaries/releases/download/v{}-gminer/gminer_{}_linux64.tar.xz", version, v);
    } else {
        f.path = format!("Bin/{}/miner.exe", dir);
        f.uri = format!("https://github.com/RainbowMiner/miner-binaries/releases/download/v{}-gminer/gminer_{}_windows64.zip", version, v);
    }
    let cuda = !info_only && ctx.has_type("NVIDIA") && ctx.cuda_ok("9.0", name);
    f.vendors = if cuda || !ctx.has_type("NVIDIA") { vec!["AMD", "NVIDIA"] } else { vec!["AMD"] };
    f.suffix = Suffix::Keys;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.emits_no_cpu = true;
    f.api = "Gminer";
    f.dev_id_prop = "BusId_Type_Vendor_Index";
    f.dev_id_join = " ";
    // $DeviceCapability = "5.0" for the NVIDIA cards
    f.model_filter = Some(|_, d| d.Vendor != "NVIDIA" || capability_ok(d, "5.0"));
    // $_.Vendor -icontains $Miner_Vendor -and (-not $_.Version -or [version]$_.Version -le [version]$Version)
    f.cmd_ok = Some(|fam, _, cmd| {
        let v = cmd_s(cmd, "Version");
        v.is_empty() || util::compare_version(&v, &fam.version, 0) <= 0
    });
    f.per_key_filter = Some(|_, ctx: &MinerCtx, d: &Device, cmd: &Value, pool: Option<&Pool>, algo0: &str| {
        let Some(pool) = pool else { return false };
        cmd_vendor(cmd, &d.Vendor) && coin_ok(cmd, pool) && ctx.test_vram(d, ctx.min_mem_gb(cmd, pool, algo0))
    });
    f.post = Some(|a, m| {
        let p = cmd_f(a.cmd, "Penalty").unwrap_or(0.0);
        if p != 0.0 {
            for v in m.hashrates.values_mut() {
                *v = v.map(|h| h * (1.0 - p / 100.0));
            }
        }
        let fee = cmd_f(a.cmd, "Fee").unwrap_or(a.fam.dev_fee);
        m.dev_fee.values_mut().for_each(|v| *v = fee);
    });
    f
}

/// the common start of the Gminer arguments up to the pool user
fn gminer_pers_proto(a: &A) -> (String, String) {
    let has_ethproxy = crate::data::RE_ALGO_HAS_ETHPROXY.is_match(a.algo0);
    let proto = opt(
        has_ethproxy && !a.pool.EthMode.is_empty() && a.pool.EthMode != "ethproxy" && a.pool.EthMode != "qtminer",
        " --proto stratum".into(),
    );
    let pers = if ps_match(a.key, "^Equihash") { crate::data::get_equihash_coin_pers(&a.pool.CoinSymbol, "auto") } else { String::new() };
    let pers = opt(!pers.is_empty() && (cmd_b(a.cmd, "AutoPers") || pers != "auto"), format!(" --pers {}", pers));
    (proto, pers)
}

fn cuda_opencl(a: &A) -> String {
    format!(" --cuda {} --opencl {}", (a.vendor == "NVIDIA") as i32, (a.vendor == "AMD") as i32)
}

/// Gminer 2.34: --server host --port port, no worker
fn gminer234(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = gminer_base(ctx, info_only, "Gminer234", &CMD_G234, "2.34", "375", "GPU-Gminer234");
    f.make_args = |a| {
        let (proto, pers) = gminer_pers_proto(a);
        format!(
            "--api $mport --devices {} --server {} --port {}{} --user {}{}{}{}{} --watchdog 0 --pec 0 --nvml 1 {}",
            a.ids,
            a.pool.Host,
            a.pool_port,
            proto,
            a.pool.User,
            a.pass_opt(" --pass "),
            pers,
            opt(a.pool.SSL, " --ssl 1 --ssl_verification 0".into()),
            cuda_opencl(a),
            a.params()
        )
    };
    Some(f)
}

/// Gminer 2.55 and 2.75: with --worker
fn gminer255_args(a: &A) -> String {
    let (proto, pers) = gminer_pers_proto(a);
    format!(
        "--api $mport --devices {} --server {} --port {}{} --user {}{}{}{}{}{} --watchdog 0 --pec 0 --nvml 1 {}",
        a.ids,
        a.pool.Host,
        a.pool_port,
        proto,
        a.pool.User,
        a.pass_opt(" --pass "),
        opt(!a.pool.Worker.is_empty(), format!(" --worker {}", a.pool.Worker)),
        pers,
        opt(a.pool.SSL, " --ssl 1 --ssl_verification 0".into()),
        cuda_opencl(a),
        a.params()
    )
}

fn gminer255(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = gminer_base(ctx, info_only, "Gminer255", &CMD_G255, "2.55", "347", "GPU-Gminer255");
    f.make_args = gminer255_args;
    Some(f)
}

fn gminer275(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    // $EnableContest is $false: no --contest_wallet
    let mut f = gminer_base(ctx, info_only, "Gminer275", &CMD_G275, "2.75", "346", "GPU-Gminer275");
    f.make_args = gminer255_args;
    Some(f)
}

/// Gminer 2.94: host:port, the SHA256ton protocol prefix
fn gminer294(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = gminer_base(ctx, info_only, "Gminer294", &CMD_G294, "2.94", "369", "GPU-Gminer294");
    f.make_args = |a| {
        let (proto, pers) = gminer_pers_proto(a);
        let ton = a.algo0 == "SHA256ton";
        format!(
            "--api $mport --devices {} --server {}{}{} --user {}{}{}{}{}{} --watchdog 0 --pec 0 --nvml 1 {}",
            a.ids,
            opt(ton && !a.pool.Protocol.is_empty(), format!("{}://", a.pool.Protocol)),
            super::host_port(a.pool, a.pool_port),
            proto,
            a.pool.User,
            a.pass_opt(" --pass "),
            opt(!a.pool.Worker.is_empty(), format!(" --worker {}", a.pool.Worker)),
            pers,
            opt(!ton, if a.pool.SSL { " --ssl 1 --ssl_verification 0".into() } else { " --ssl 0".into() }),
            cuda_opencl(a),
            a.params()
        )
    };
    Some(f)
}

// ------------------------------------------------------------------------------------------
// SrbMinerMulti 0.9.4 (AMD GCN1-3)
// ------------------------------------------------------------------------------------------

fn srbminermulti094(ctx: &MinerCtx, _info_only: bool) -> Option<Family> {
    if !win_linux_x64(ctx) {
        return None;
    }
    let mut f = Family::new("SrbMinerMulti094", "AMD", &CMD_SRB094);
    f.manual_uri = "https://bitcointalk.org/index.php?topic=5190081.0".into();
    f.port = "350".into();
    f.dev_fee = 0.85;
    f.version = "0.9.4".into();
    if ctx.is_linux() {
        f.path = "Bin/ANY-SRBMinerMulti094/SRBMiner-MULTI".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.9.4-srbminermulti/SRBMiner-Multi-0-9-4-Linux.tar.xz".into();
    } else {
        f.path = "Bin/ANY-SRBMinerMulti094/SRBMiner-MULTI.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.9.4-srbminermulti/SRBMiner-Multi-0-9-4-win64.zip".into();
    }
    f.suffix = Suffix::KeysCoin;
    f.exclude_yiimp = true;
    f.first_per_cmd = true;
    f.use_exclude_pool = true;
    f.use_coin_symbols = true;
    f.api = "SrbMinerMulti";
    f.ids_fn = Some(|_, devs| devs.iter().map(|d| d.BusId_Type_Vendor_Index.to_string()).collect::<Vec<_>>().join("!"));
    f.model_filter = Some(|_, d| ["GCN1", "GCN2", "GCN3"].contains(&d.OpenCL.DeviceCapability.as_deref().unwrap_or("")));
    f.per_key_filter = Some(|_, ctx: &MinerCtx, d: &Device, cmd: &Value, pool: Option<&Pool>, algo0: &str| {
        let Some(pool) = pool else { return false };
        // the original reads the DagSizeMax of an unset variable: the DAG size of the pool's coin
        let min = if cmd_b(cmd, "DAG") {
            crate::data::get_eth_dag_size(&pool.CoinSymbol, algo0, crate::jsonx::num(crate::jsonx::get(cmd, "MinMemGB")))
        } else {
            crate::jsonx::num(crate::jsonx::get(cmd, "MinMemGB"))
        };
        cmd_vendor(cmd, &d.Vendor) && ctx.test_vram(d, min)
    });
    f.make_args = |a| {
        let esm = match a.pool.EthMode.as_str() {
            "ethproxy" => "--esm 0 ",
            "minerproxy" => "--esm 1 ",
            _ => "",
        };
        let wd = if a.ctx.config.b("RebootOnGPUFailure") && a.ctx.config.b("EnableRestartComputer") {
            format!(" --reboot-script-gpu-watchdog '{}'", a.ctx.base.join(if cfg!(windows) { "Reboot.bat" } else { "reboot.sh" }).display())
        } else {
            String::new()
        };
        let intensity = a.devices.iter().map(|_| "0").collect::<Vec<_>>().join("!");
        let rig = a.ctx.config.pool(&a.pool.Name).map(|p| p.worker.clone()).unwrap_or_default();
        format!(
            "--algorithm {} --api-enable --api-port $mport --api-rig-name {} {}--gpu-id {} --gpu-intensity {} --disable-cpu {} --pool {}:{} --wallet {}{}{} --tls {} --nicehash {} --keepalive --retry-time 10 --disable-startup-monitor {}",
            a.algorithm(),
            rig,
            esm,
            a.ids,
            intensity,
            wd,
            a.pool.Host,
            a.pool_port,
            a.pool.User,
            opt(!a.pool.Worker.is_empty(), format!(" --worker {}", a.pool.Worker)),
            opt(!a.pool.Pass.is_empty(), format!(" --password {}", srb_escape(&a.pool.Pass))),
            if a.pool.SSL { "true" } else { "false" },
            if ps_match(&a.pool.Host, "NiceHash") { "true" } else { "false" },
            a.params()
        )
    };
    f.post = Some(|a, m| {
        let mrr = cmd_f(a.cmd, "MaxRejectedShareRatio").filter(|x| *x != 0.0);
        if mrr.is_some() {
            let r = a.ctx.ratio_live(&m.name, a.algo0);
            for v in m.hashrates.values_mut() {
                *v = v.map(|h| h * (1.0 - r));
            }
        }
        m.max_rejected_share_ratio = mrr;
        m.extend_interval = cmd_f(a.cmd, "ExtendInterval").filter(|x| *x != 0.0).map(|x| x as i64).unwrap_or(0);
        let fee = cmd_f(a.cmd, "Fee").unwrap_or(0.0);
        m.dev_fee.values_mut().for_each(|v| *v = fee);
        m.env_vars = if a.ctx.is_linux() { vec!["GPU_MAX_WORKGROUP_SIZE=1024".into()] } else { vec![] };
        m.set_ld_library_path = Some(false);
        m.list_devices = "--list-devices".into();
    });
    Some(f)
}

/// -replace "([;!])","#`$1"
pub fn srb_escape(p: &str) -> String {
    p.replace(';', "#;").replace('!', "#!")
}

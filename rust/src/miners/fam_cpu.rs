//! generated from the Invoke-MinerFamily stubs, see family.rs

use super::family::*;
use super::opt;
use crate::miner::*;
use serde_json::Value;
use std::sync::LazyLock;

pub fn modules() -> Vec<FamilyModule> {
    vec![
        FamilyModule { name: "CpuminerArctichash", types: &["CPU"], def: cpuminer_arctichash },
        FamilyModule { name: "CpuminerAurum", types: &["CPU"], def: cpuminer_aurum },
        FamilyModule { name: "CpuminerEasy", types: &["CPU"], def: cpuminer_easy },
        FamilyModule { name: "CpuminerKudaraidee", types: &["CPU", "ARMCPU"], def: cpuminer_kudaraidee },
        FamilyModule { name: "CpuminerMulti", types: &["CPU"], def: cpuminer_multi },
        FamilyModule { name: "CpuminerOptBF", types: &["CPU"], def: cpuminer_opt_bf },
        FamilyModule { name: "CpuminerPower", types: &["CPU"], def: cpuminer_power },
        FamilyModule { name: "CpuminerRKZ", types: &["CPU"], def: cpuminer_rkz },
        FamilyModule { name: "CpuminerRplant", types: &["CPU"], def: cpuminer_rplant },
        FamilyModule { name: "CpuminerTake2", types: &["CPU"], def: cpuminer_take2 },
        FamilyModule { name: "CpuminerVerium", types: &["CPU"], def: cpuminer_verium },
        FamilyModule { name: "CpuminerVerus", types: &["CPU", "ARMCPU"], def: cpuminer_verus },
        FamilyModule { name: "HtnCpu", types: &["CPU", "ARMCPU"], def: htn_cpu },
        FamilyModule { name: "Nheqminer", types: &["CPU"], def: nheqminer },
        FamilyModule { name: "RhminerCpu", types: &["CPU"], def: rhminer_cpu },
    ]
}

// ---- Miners/CpuminerArctichash.ps1
static CMD_CPUMINER_ARCTICHASH: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerArctichash.json")));

fn cpuminer_arctichash(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CpuminerArctichash", "CPU", &CMD_CPUMINER_ARCTICHASH);
    f.manual_uri = "".into();
    f.version = "1.3.7.1".into();
    f.port = "245".into();
    f.dev_fee = 0.0;
    f.path = "Bin/CPU-ArcticHash/cpuminer.exe".into();
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.3.7.1-cpuminerarctichash/cpuminer-arctichash-v1.3.7.1.7z".into();
    f.suffix = Suffix::ListCPU;
    f.check_ssl = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.make_args = |a| format!("-b $mport -a {} -o {}://{}:{} -u {}{}{} {}", a.main(), a.pool.Protocol, a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerAurum.ps1
static CMD_CPUMINER_AURUM: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerAurum.json")));

fn cpuminer_aurum(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CpuminerAurum", "CPU", &CMD_CPUMINER_AURUM);
    f.manual_uri = "https://github.com/bitnet-io/cpuminer-opt-aurum/releases".into();
    f.version = "3.23.1".into();
    f.port = "205".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = format!("Bin/CPU-Aurum/cpuminer-{}", cpu_pick(ctx, &[("avx512 sha vaes", "avx512-sha-vaes"), ("avx512", "avx512"), ("avx2 sha vaes", "avx2-sha-vaes"), ("avx2 sha aes", "avx2-sha"), ("avx2 aes", "avx2"), ("avx aes", "avx"), ("sse42 aes", "aes-sse42"), ("sse42", "sse42")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.23.1-jayddeeaurum/cpuminer-opt-aurum-3.23.1-linux.7z".into();
    } else {
        f.path = format!("Bin/CPU-Aurum/cpuminer-{}.exe", cpu_pick(ctx, &[("avx512 sha vaes", "avx512-sha-vaes"), ("avx512", "avx512"), ("avx2 sha vaes", "avx2-sha-vaes"), ("avx2 sha aes", "avx2-sha"), ("avx2 aes", "avx2"), ("avx aes", "avx"), ("sse42 aes", "aes-sse42")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.23.1-jayddeeaurum/cpuminer-opt-aurum-3.23.1-win64.7z".into();
    }
    f.suffix = Suffix::KeysNoGPU;
    f.cpu_params = true;
    f.extend_default = 2;
    f.use_exclude_pool = true;
    f.make_args = |a| {
        format!(
            "-b 127.0.0.1:$mport -a {} -o {}://{}:{} -u {}{}{} -q {}",
            a.algorithm(),
            a.pool.Protocol,
            a.pool.Host,
            a.pool.Port,
            a.pool.User,
            a.pass_opt(" -p "),
            a.device_params,
            a.params()
        )
    };
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerEasy.ps1
static CMD_CPUMINER_EASY: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerEasy.json")));

fn cpuminer_easy(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CpuminerEasy", "CPU", &CMD_CPUMINER_EASY);
    f.manual_uri = "https://github.com/bubasik/cpuminer-easy-binarium/releases".into();
    f.version = "3.7.0".into();
    f.port = "231".into();
    f.dev_fee = 0.0;
    f.path = format!("Bin/CPU-Easy/cpuminer-{}.exe", cpu_pick(ctx, &[("avx aes", "aes-avx"), ("sse42 aes", "aes-sse42")], "sse2"));
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.7.0-cpuminereasy/cpuminer-easy-binarium-win_x64.zip".into();
    f.suffix = Suffix::ListCPU;
    f.check_ssl = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.make_args = |a| format!("-b $mport -a {} -o {}://{}:{} -u {}{}{} {}", a.main(), a.pool.Protocol, a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerKudaraidee.ps1
static CMD_CPUMINER_KUDARAIDEE: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerKudaraidee.json")));

fn cpuminer_kudaraidee(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    let mut f = Family::new("CpuminerKudaraidee", "CPU", &CMD_CPUMINER_KUDARAIDEE);
    f.manual_uri = "https://github.com/Kudaraidee/cpuminer-opt-kudaraidee/releases".into();
    f.version = "1.2.4".into();
    f.port = "206".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        if ctx.is_arm() {
            let arm_uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.4-kudaraidee/cpuminer-opt-kudaraidee-1.2.4-arm.7z";
            match ctx.cpu.Architecture.as_str() {
                "8" => {
                    f.path = format!(
                        "Bin/CPU-Kudaraidee/cpuminer-armv8{}",
                        cpu_pick(ctx, &[("sha3 sve2 aes", ".5-crypto-sha3-sve2"), ("sha3 aes", ".4-crypto-sha3"), ("sha2 aes", "-crypto")], "")
                    );
                    f.uri = arm_uri.into();
                }
                "9" => {
                    f.path = format!("Bin/CPU-Kudaraidee/cpuminer-armv9{}", cpu_pick(ctx, &[("sha3 aes", "-crypto-sha3"), ("sha2 aes", "-crypto")], ""));
                    f.uri = arm_uri.into();
                }
                _ => return None,
            }
        } else {
            f.path = format!(
                "Bin/CPU-Kudaraidee/cpuminer-{}",
                cpu_pick(
                    ctx,
                    &[
                        ("avx512 sha vaes", "avx512-sha-vaes"),
                        ("avx512", "avx512"),
                        ("avx2 sha vaes", "avx2-sha-vaes"),
                        ("avx2 sha aes", "avx2-sha"),
                        ("avx2 aes", "avx2"),
                        ("avx aes", "aes-avx"),
                        ("avx", "avx"),
                        ("sse42 aes", "aes-sse42"),
                        ("sse42", "sse42"),
                        ("ssse3", "ssse3")
                    ],
                    "sse2"
                )
            );
            f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.4-kudaraidee/cpuminer-opt-kudaraidee-1.2.4-linux.7z".into();
        }
    } else {
        f.path = format!(
            "Bin/CPU-Kudaraidee/cpuminer-{}.exe",
            cpu_pick(
                ctx,
                &[
                    ("avx512 sha vaes", "avx512-sha-vaes"),
                    ("avx512", "avx512"),
                    ("avx2 sha vaes", "avx2-sha-vaes"),
                    ("avx2 sha aes", "avx2-sha"),
                    ("avx2 aes", "avx2"),
                    ("avx aes", "aes-avx"),
                    ("avx", "avx"),
                    ("sse42 aes", "aes-sse42")
                ],
                "sse2"
            )
        );
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.2.4-kudaraidee/cpuminer-opt-kudaraidee-1.2.4-win.7z".into();
    }
    f.suffix = Suffix::KeysNoGPU;
    f.cmd_filter = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.use_exclude_pool = true;
    f.info_types = vec!["CPU".into(), "ARMCPU".into()];
    f.make_args = |a| {
        format!(
            "-b 127.0.0.1:$mport -a {} -o {}://{}:{} -u {}{}{} -q {}",
            a.algorithm(),
            a.pool.Protocol,
            a.pool.Host,
            a.pool.Port,
            a.pool.User,
            a.pass_opt(" -p "),
            a.device_params,
            a.params()
        )
    };
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerMulti.ps1
static CMD_CPUMINER_MULTI: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerMulti.json")));

fn cpuminer_multi(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() {
        return None;
    }
    let mut f = Family::new("CpuminerMulti", "CPU", &CMD_CPUMINER_MULTI);
    f.manual_uri = "https://github.com/tpruvot/cpuminer-multi/releases".into();
    f.version = "1.3.6.3".into();
    f.port = "213".into();
    f.dev_fee = 0.0;
    f.path = format!("Bin/CPU-Multi/cpuminer-{}.exe", cpu_pick(ctx, &[("avx2 sha aes", "avx2-sha"), ("avx2 aes", "avx2"), ("avx aes", "avx"), ("sse42 aes", "aes-sse42"), ("sse42", "sse42")], "sse2"));
    f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.3.6.3-cpuminermulti/cpuminer-multi-1.3.6.3.7z".into();
    f.suffix = Suffix::ListCPU;
    f.check_ssl = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.make_args = |a| format!("-b $mport -a {} -o {}://{}:{} -u {}{}{} {}", a.main(), a.pool.Protocol, a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerOptBF.ps1
static CMD_CPUMINER_OPT_BF: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerOptBF.json")));

fn cpuminer_opt_bf(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CpuminerOptBF", "CPU", &CMD_CPUMINER_OPT_BF);
    f.manual_uri = "https://github.com/bellflower2015/cpuminer-opt/releases".into();
    f.version = "3.8.12".into();
    f.port = "204".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = format!("Bin/CPU-OptBF/cpuminer-{}", cpu_pick(ctx, &[("avx2 sha aes", "avx2-sha"), ("avx2 aes", "avx2"), ("avx aes", "aes-avx"), ("sse42 aes", "aes-sse42")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.8.12-cpumineropt/cpuminer-opt-v3.8.12-bf-linux.7z".into();
    } else {
        f.path = format!("Bin/CPU-OptBF/cpuminer-{}.exe", cpu_pick(ctx, &[("avx2 sha aes", "avx2-sha"), ("avx2 aes", "avx2"), ("avx aes", "avx"), ("sse42 aes", "aes-sse42")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.8.12-cpumineropt/cpuminer-opt-v3.8.12-bf-win64.zip".into();
    }
    f.suffix = Suffix::ListCPU;
    f.check_ssl = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.make_args = |a| format!("-b $mport -a {} -o {}://{}:{} -u {}{} -R 10 -r 4{} {}", a.main(), a.pool.Protocol, a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerPower.ps1
static CMD_CPUMINER_POWER: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerPower.json")));

fn cpuminer_power(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CpuminerPower", "CPU", &CMD_CPUMINER_POWER);
    f.manual_uri = "https://github.com/cpu-pool/cpuminer-opt-cpupower/releases".into();
    f.version = "1.4".into();
    f.port = "239".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = "Bin/CPU-Power/cpuminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.4-cpupool/Cpuminer-opt-cpu-pool-v1.4-linux64.tar.gz".into();
    } else {
        f.path = format!("Bin/CPU-Power/cpuminer-{}.exe", cpu_pick(ctx, &[("avx2 sha aes", "avx2-sha"), ("avx2 aes", "avx2"), ("avx aes", "avx"), ("sse42 aes", "aes-sse42")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.4-cpupool/Cpuminer-opt-cpu-pool-v1.4-win64.zip".into();
    }
    f.suffix = Suffix::ListCPU;
    f.check_ssl = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.make_args = |a| format!("-b $mport -a {} -o {}://{}:{} -u {}{}{} {}", a.main(), a.pool.Protocol, a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerRKZ.ps1
static CMD_CPUMINER_RKZ: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerRKZ.json")));

fn cpuminer_rkz(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CpuminerRKZ", "CPU", &CMD_CPUMINER_RKZ);
    f.manual_uri = "https://github.com/patrykwnosuch/cpuminer-RKZ/releases".into();
    f.version = "3.9.7c".into();
    f.port = "242".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = format!("Bin/CPU-rkz/cpuminer-{}", cpu_pick(ctx, &[("avx2 sha aes", "avx2-sha"), ("avx2 aes", "avx2"), ("sse2 aes", "aes-sse2")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.9.7c-cpuminerrkz/cpuminer-RKZ-3.9.7c-ubuntu18.7z".into();
    } else {
        f.path = format!("Bin/CPU-rkz/cpuminer-{}.exe", cpu_pick(ctx, &[("avx2 sha aes", "sha-avx2"), ("avx2 aes", "avx2"), ("sse42 aes", "aes-sse42")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.9.7c-cpuminerrkz/cpuminer-RKZ-3.9.7c-win64.7z".into();
    }
    f.suffix = Suffix::ListCPU;
    f.check_ssl = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.make_args = |a| format!("-b $mport -a {} -o {}://{}:{} -u {}{}{} {}", a.main(), a.pool.Protocol, a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerRplant.ps1
static CMD_CPUMINER_RPLANT: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerRplant.json")));

fn cpuminer_rplant(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CpuminerRplant", "CPU", &CMD_CPUMINER_RPLANT);
    f.manual_uri = "https://github.com/rplant8/cpuminer-opt-rplant/releases".into();
    f.version = "5.0.46".into();
    f.port = "232".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.version = "5.0.46u".into();
        f.path = format!("Bin/CPU-Rplant/cpuminer-{}", cpu_pick(ctx, &[("avx512 sha vaes", "avx512-sha-vaes"), ("avx512", "avx512"), ("avx2 sha aes", "ryzen"), ("avx2 aes", "avx2"), ("avx aes", "avx"), ("sse42 aes", "sse42-aes"), ("sse42", "sse42"), ("vendor:AMD", "sse2amd")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v5.0.46-rplant/cpuminer-opt-linux-5.0.46u.tar.gz".into();
    } else {
        f.version = "5.0.41".into();
        f.path = format!("Bin/CPU-Rplant/cpuminer-{}.exe", cpu_pick(ctx, &[("avx512", "avx512"), ("avx2 sha aes", "ryzen"), ("avx2 aes", "avx2"), ("avx aes", "avx"), ("sse42 aes", "sse42-aes"), ("sse42", "sse42"), ("vendor:AMD", "sse2amd")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v5.0.41-rplant/cpuminer-opt-win-5.0.41.zip".into();
    }
    f.suffix = Suffix::KeysNoGPU;
    f.use_algo_override = true;
    f.cmd_filter = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.use_exclude_pool = true;
    f.emits_max_rej = true;
    f.make_args = |a| {
        // PerKey: $Miner_Coin = the pool's coin, a command may name the algorithm per coin
        let coin = &a.pool.CoinSymbol;
        let per_coin = if coin.is_empty() { String::new() } else { a.c(coin) };
        format!(
            "-b $mport -a {} -o stratum+tcp{}://{}:{} -u {}{}{} --no-doh {}",
            if per_coin.is_empty() { a.main() } else { per_coin },
            opt(a.pool.SSL, "s".to_string()),
            a.pool.Host,
            a.pool.Port,
            a.pool.User,
            a.pass_opt(" -p "),
            a.device_params,
            a.params()
        )
    };
    f.device_params = Some(|ctx, threads, affinity| {
        format!(
            "{}{}",
            threads.map(|t| format!(" -t {}", t)).unwrap_or_default(),
            affinity.map(|a| format!(" --cpu-affinity {}{}", a, if ctx.is_windows() { " --no-smart" } else { "" })).unwrap_or_default()
        )
    });
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerTake2.ps1
static CMD_CPUMINER_TAKE2: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerTake2.json")));

fn cpuminer_take2(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CpuminerTake2", "CPU", &CMD_CPUMINER_TAKE2);
    f.manual_uri = "https://github.com/Raptor3um/cpuminer-opt/releases".into();
    f.version = "2.0".into();
    f.port = "212".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = format!("Bin/CPU-Take2/cpuminer-{}", cpu_pick(ctx, &[("avx512", "avx512"), ("avx2 sha aes", "zen"), ("avx2 aes", "avx2"), ("avx aes", "aes-avx"), ("sse42 aes", "aes-sse42"), ("sse42", "sse42")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2.0-take2/cpuminer-take2-ubuntu16-18.tar.gz".into();
    } else {
        f.path = format!("Bin/CPU-Take2/cpuminer-{}.exe", cpu_pick(ctx, &[("avx2 sha aes", "zen"), ("avx2 aes", "avx2"), ("avx aes", "avx"), ("sse42 aes", "aes-sse42")], "sse2"));
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2.0-take2/cpuminer-take2-windows.zip".into();
    }
    f.suffix = Suffix::KeysNoGPU;
    f.check_ssl = true;
    f.use_algo_override = true;
    f.cmd_filter = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.use_exclude_pool = true;
    f.emits_max_rej = true;
    f.make_args = |a| format!("-b $mport -a gr -o stratum+tcp{}://{}:{} -u {}{}{} {}", opt(a.pool.SSL, "s".to_string()), a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerVerium.ps1
static CMD_CPUMINER_VERIUM: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerVerium.json")));

fn cpuminer_verium(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("CpuminerVerium", "CPU", &CMD_CPUMINER_VERIUM);
    f.manual_uri = "https://github.com/fireworm71/veriumMiner/releases".into();
    f.version = "1.4".into();
    f.port = "244".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = "Bin/CPU-Verium/cpuminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.4-cpuminerverium/cpuminerverium_1.4_linux_x64_GCC7.7z".into();
    } else {
        f.path = "Bin/CPU-Verium/cpuminer.exe".into();
        f.uri = format!(
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.4-cpuminerverium/cpuminer_1.4_windows_x64_O2_GCC7{}.zip",
            if ctx.cpu.IsRyzen { "_RYZEN" } else { "" }
        );
    }
    f.suffix = Suffix::ListCPU;
    f.check_ssl = true;
    f.cpu_params = true;
    f.extend_default = 2;
    // the command's Params is "$(if ($Global:GlobalCPUInfo.IsRyzen) {"--ryzen"})"
    f.make_args = |a| {
        let params = a.params().replace("$(if ($Global:GlobalCPUInfo.IsRyzen) {\"--ryzen\"})", if a.ctx.cpu.IsRyzen { "--ryzen" } else { "" });
        format!("-R 2 -r 10 -b $mport -o stratum+tcp://{}:{} -u {}{}{} {}", a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, params)
    };
    let _ = info_only;
    Some(f)
}

// ---- Miners/CpuminerVerus.ps1
static CMD_CPUMINER_VERUS: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/CpuminerVerus.json")));

fn cpuminer_verus(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    let mut f = Family::new("CpuminerVerus", "CPU", &CMD_CPUMINER_VERUS);
    f.manual_uri = "https://github.com/monkins1010/ccminer/releases".into();
    f.version = "3.8.3".into();
    f.port = "235".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = "Bin/CPU-CcminerVerus/ccminer".into();
        f.uri = if ctx.is_arm() {
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.8.3-ccminerverus/ccminerverus22-3.8.3cpu-arm.7z"
        } else {
            "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.8.3-ccminerverus/ccminerverus22-3.8.3cpu-linux.7z"
        }
        .into();
    } else {
        f.path = "Bin/CPU-CcminerVerus/ccminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v3.8.3-ccminerverus/ccminerverus22-3.8.3cpu-win.7z".into();
    }
    f.suffix = Suffix::ListCPU;
    f.check_ssl = true;
    f.cpu_params = true;
    f.extend_default = 2;
    f.info_types = vec!["CPU".into(), "ARMCPU".into()];
    f.make_args = |a| format!("-N 1 -R 2 -r 10 -b $mport -a {} -o stratum+tcp://{}:{} -u {}{}{} {}", a.main(), a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/HtnCpu.ps1
static CMD_HTN_CPU: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/HtnCpu.json")));

fn htn_cpu(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_linux() {
        return None;
    }
    if !info_only && ctx.libc_below("2.30") {
        return None;
    }
    let mut f = Family::new("HtnCpu", "CPU", &CMD_HTN_CPU);
    f.manual_uri = "https://htn.foztor.net/".into();
    f.version = "1.4.9".into();
    f.port = "237".into();
    f.dev_fee = 1.5;
    if ctx.is_arm() {
        if ctx.cpu.Architecture != "8" {
            return None;
        }
        f.path = "Bin/CPU-Htn/hoo_cpu_arm".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.4.9-htm/hoo_cpu_arm-1.4.9.tar.gz".into();
    } else {
        // needs AVX2
        if !ctx.cpu.has("avx2") && !info_only {
            return None;
        }
        f.path = "Bin/CPU-Htn/hoo_cpu".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v1.4.9-htm/hoo_cpu-1.4.9.tar.gz".into();
    }
    f.suffix = Suffix::KeysNoGPU;
    f.cpu_params = true;
    f.use_exclude_pool = true;
    f.extend_default = 2;
    f.api = "Wrapper";
    f.info_types = vec!["CPU".into(), "ARMCPU".into()];
    f.make_args = |a| format!("-b 127.0.0.1:$mport -o {}://{}:{} -u {}{}{} -q {}", a.pool.Protocol, a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    Some(f)
}

// ---- Miners/Nheqminer.ps1
static CMD_NHEQMINER: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/Nheqminer.json")));

fn nheqminer(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("Nheqminer", "CPU", &CMD_NHEQMINER);
    f.manual_uri = "https://github.com/VerusCoin/nheqminer/releases".into();
    f.version = "0.8.2".into();
    f.port = "236".into();
    f.dev_fee = 0.0;
    if ctx.is_linux() {
        f.path = "Bin/CPU-Nheqminer/nheqminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.8.2-nheqminer/nheqminer-0.8.2-linux.7z".into();
    } else {
        f.path = "Bin/CPU-Nheqminer/nheqminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v0.8.2-nheqminer/nheqminer-0.8.2-win.7z".into();
    }
    f.suffix = Suffix::ListCPU;
    f.check_ssl = true;
    f.cpu_params = true;
    f.api = "Nheq";
    f.make_args = |a| format!("-a $mport -v -l {}:{} -u {}{}{} {}", a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -p "), a.device_params, a.params());
    let _ = info_only;
    Some(f)
}

// ---- Miners/RhminerCpu.ps1
static CMD_RHMINER_CPU: LazyLock<Vec<Value>> = LazyLock::new(|| commands(include_str!("../../assets/miners/RhminerCpu.json")));

fn rhminer_cpu(ctx: &MinerCtx, info_only: bool) -> Option<Family> {
    if !ctx.is_windows() && !ctx.is_linux() {
        return None;
    }
    if ctx.is_linux() && ctx.is_arm() {
        return None;
    }
    let mut f = Family::new("RhminerCpu", "CPU", &CMD_RHMINER_CPU);
    f.manual_uri = "https://github.com/polyminer1/rhminer/releases".into();
    f.version = "2.3".into();
    f.port = "131".into();
    f.dev_fee = 1.0;
    if ctx.is_linux() {
        f.path = "Bin/CPU-RHminer/rhminer".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2.3-rhminer/rhminer.2.3.Linux.CPU.zip".into();
    } else {
        f.path = "Bin/CPU-RHminer/rhminer.exe".into();
        f.uri = "https://github.com/RainbowMiner/miner-binaries/releases/download/v2.3-rhminer/rhminer.2.3.Windows.CPU.zip".into();
    }
    f.suffix = Suffix::ListCPU;
    f.cpu_params = true;
    f.api = "Claymore";
    f.make_args = |a| format!("-apiport $mport -s {}://{}:{} -su {}{} -cpu{} {}", a.pool.Protocol, a.pool.Host, a.pool.Port, a.pool.User, a.pass_opt(" -pw "), a.device_params, a.params());
    f.device_params = Some(|_, threads, affinity| {
        format!(
            "{}{}",
            threads.map(|t| format!(" -cputhreads {}", t)).unwrap_or_default(),
            affinity
                .map(|a| format!(" -processorsaffinity {}", crate::util::convert_from_cpu_affinity(a).iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")))
                .unwrap_or_default()
        )
    });
    let _ = info_only;
    Some(f)
}

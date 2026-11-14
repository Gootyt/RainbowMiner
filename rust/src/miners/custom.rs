//! Invoke-CustomMiner: the miners of Config/customminers.config.txt
//! (arguments from a %TOKEN% template, see Expand-CustomMinerArguments)

use crate::config::CustomMinerDef;
use crate::device::Device;
use crate::miner::*;
use crate::util;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;

/// Get-CustomMinerPlatform
pub fn platform(ctx: &MinerCtx) -> &'static str {
    if !cfg!(target_os = "linux") {
        "Windows"
    } else if ctx.is_arm() {
        "LinuxArm"
    } else {
        "Linux"
    }
}

fn os_block<'d>(ctx: &MinerCtx, def: &'d CustomMinerDef) -> &'d (String, String) {
    match platform(ctx) {
        "Windows" => &def.windows,
        "LinuxArm" => &def.linux_arm,
        _ => &def.linux,
    }
}

fn path_of(ctx: &MinerCtx, def: &CustomMinerDef) -> Option<PathBuf> {
    let (_, p) = os_block(ctx, def);
    if p.is_empty() {
        None
    } else {
        Some(ctx.base.join("Bin").join(format!("Custom-{}", def.name)).join(p))
    }
}

pub fn types(def: &CustomMinerDef) -> Vec<String> {
    let mut t = def.vendors.clone();
    if !def.linux_arm.0.is_empty() && !def.linux_arm.1.is_empty() {
        t.extend(def.vendors.iter().map(|v| format!("ARM{}", v)));
    }
    t
}

pub fn info(ctx: &MinerCtx, def: &CustomMinerDef) -> MinerInfo {
    MinerInfo {
        name: def.name.clone(),
        types: types(def),
        path: path_of(ctx, def).unwrap_or_default(),
        uri: os_block(ctx, def).0.clone(),
        dev_fee: def.dev_fee,
        manual_uri: def.manual_uri.clone(),
        commands: serde_json::Value::Array(
            def.commands
                .iter()
                .map(|c| serde_json::json!({"MainAlgorithm": c.main_algorithm, "Algo": c.algo, "Params": c.params, "Vendors": c.vendors}))
                .collect(),
        ),
    }
}

/// Get-CustomMinerPoolLogin: strip only the known worker suffix ("user/worker" or "user.worker")
pub fn pool_login(user: &str, worker: &str) -> String {
    if !worker.is_empty() {
        for sep in ["/", "."] {
            let suffix = format!("{}{}", sep, worker).to_lowercase();
            if user.to_lowercase().ends_with(&suffix) {
                return user[..user.len() - suffix.len()].to_string();
            }
        }
    }
    user.to_string()
}

/// Expand-CustomMinerArguments
pub fn expand_arguments(template: &str, values: &HashMap<&str, String>) -> String {
    if template.is_empty() {
        return String::new();
    }
    let t = template.replace("%MPORT%", "$mport");
    let re = Regex::new(r"%([A-Z_]+)%").unwrap();
    re.replace_all(&t, |c: &regex::Captures| values.get(&c[1]).cloned().unwrap_or_else(|| c[0].to_string()))
        .trim()
        .to_string()
}

pub fn get_miners(ctx: &MinerCtx, def: &CustomMinerDef) -> Vec<MinerCandidate> {
    let mut out = Vec::new();
    let Some(path) = path_of(ctx, def) else { return out };
    let uri = os_block(ctx, def).0.clone();
    if uri.is_empty() || def.commands.is_empty() {
        return out;
    }
    let api = if def.hashrate_regex.is_empty() { def.api.clone() } else { "CustomWrapper".to_string() };
    for vendor in &def.vendors {
        let is_gpu = vendor != "CPU";
        for model in ctx.models(vendor) {
            let all: Vec<&Device> = ctx.devices_of(vendor, &model);
            for cmd in def.commands.iter().filter(|c| c.vendors.is_empty() || util::contains_ci(&c.vendors, vendor)) {
                let mut first = true;
                let algo0 = cmd.main_algorithm.clone();
                let (threads, affinity) = if is_gpu { (None, None) } else { (ctx.cpu_threads(&def.name, &algo0), ctx.cpu_affinity(&def.name, &algo0)) };
                let (mut miner_port, mut mname, mut ids, mut count) = (0u16, String::new(), String::new(), 0usize);
                for key in ctx.algo_keys(&def.name, &algo0, &model, KeyOpts { no_gpu: !is_gpu, exclude_pool_name: &def.exclude_pool_name, ..Default::default() }) {
                    let Some(pool) = ctx.pool(&key) else { continue };
                    if pool.Host.is_empty() || (!def.exclude_pool_name.is_empty() && ps_match(&pool.Host, &def.exclude_pool_name)) || (pool.SSL && !def.enable_ssl) {
                        continue;
                    }
                    let miner_device: Vec<&Device> = if is_gpu {
                        let min_mem = if cmd.dag {
                            match pool.DagSizeMax {
                                Some(d) if d > 0.0 => d,
                                _ => crate::data::get_eth_dag_size(&pool.CoinSymbol, &algo0, if cmd.min_mem_gb > 0.0 { cmd.min_mem_gb } else { 1.0 }),
                            }
                        } else {
                            cmd.min_mem_gb
                        };
                        all.iter().copied().filter(|d| ctx.test_vram(d, min_mem)).collect()
                    } else {
                        all.clone()
                    };
                    if miner_device.is_empty() {
                        continue;
                    }
                    if first {
                        miner_port = def.port.saturating_add(miner_device[0].Index as u16);
                        mname = miner_name(&def.name, None, &miner_device);
                        ids = device_ids(&miner_device, &def.device_index_property, &def.device_separator, def.device_index_hex);
                        count = miner_device.len();
                        first = false;
                    }
                    let pool_port = if is_gpu { pool.port_gpu() } else { pool.port_cpu() };
                    let protocol = if pool.Protocol.is_empty() { format!("stratum+{}", if pool.SSL { "ssl" } else { "tcp" }) } else { pool.Protocol.clone() };
                    let worker = if pool.Worker.is_empty() { ctx.config.s("WorkerName") } else { pool.Worker.clone() };
                    let login = pool_login(&pool.User, &worker);
                    let mut v: HashMap<&str, String> = HashMap::new();
                    v.insert("ALGO", cmd.algo.clone());
                    v.insert("URL", format!("{}{}", pool.Host, if pool_port > 0 && !pool.Host.contains('/') { format!(":{}", pool_port) } else { String::new() }));
                    v.insert("HOST", pool.Host.clone());
                    v.insert("PORT", pool_port.to_string());
                    v.insert("PROTOCOL", protocol);
                    v.insert("WAL", pool.User.clone());
                    v.insert("USER", pool.User.clone());
                    v.insert("LOGIN", login.clone());
                    v.insert("POOL_LOGIN", login);
                    v.insert("WALLET", pool.Wallet.clone());
                    v.insert("PASS", pool.Pass.clone());
                    v.insert("WORKER_NAME", worker);
                    v.insert("COIN", pool.CoinSymbol.clone());
                    v.insert("ETHMODE", pool.EthMode.clone());
                    v.insert("DEVICES", ids.clone());
                    v.insert("DEVCOUNT", count.to_string());
                    v.insert("THREADS", threads.map(|t| t.to_string()).unwrap_or_default());
                    v.insert("AFFINITY", affinity.clone().unwrap_or_default());
                    v.insert(
                        "AFFINITY_LIST",
                        affinity.as_ref().map(|a| util::convert_from_cpu_affinity(a).iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")).unwrap_or_default(),
                    );
                    let template = format!(
                        "{} {}{}",
                        def.arguments,
                        cmd.params,
                        if pool.SSL && !def.ssl_arguments.is_empty() { format!(" {}", def.ssl_arguments) } else { String::new() }
                    );
                    let mut m = MinerCandidate::new(&def.name, &mname, &miner_device, &model, path.clone(), &api, miner_port)
                        .with_algo(&key, ctx.hashrate(&mname, &algo0), cmd.fee.unwrap_or(def.dev_fee));
                    m.arguments = MinerArgs::Str(expand_arguments(&template, &v));
                    m.uri = uri.clone();
                    m.extend_interval = cmd.extend_interval.unwrap_or(def.extend_interval);
                    m.manual_uri = def.manual_uri.clone();
                    m.env_vars = def.env_vars.clone();
                    m.version = def.version.clone();
                    m.base_algorithm = algo0.clone();
                    m.exclude_pool_name = def.exclude_pool_name.clone();
                    m.show_miner_window = def.show_miner_window;
                    m.hashrate_regex = def.hashrate_regex.clone();
                    out.push(m);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_and_template() {
        assert_eq!(pool_login("wallet.rig1", "rig1"), "wallet");
        assert_eq!(pool_login("wallet/rig1", "rig1"), "wallet");
        assert_eq!(pool_login("wallet", "rig1"), "wallet");
        let mut v = HashMap::new();
        v.insert("URL", "pool:3333".to_string());
        assert_eq!(expand_arguments("-o %URL% --api %MPORT% %UNKNOWN%", &v), "-o pool:3333 --api $mport %UNKNOWN%");
    }
}

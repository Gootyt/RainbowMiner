//! Pools/MoneroOcean.ps1 (algo switching pool, paid in XMR)

use crate::data;
use crate::jsonx;
use crate::pool::{Pool, PoolCtx, PoolModule, PoolPorts};
use crate::stat::{self, SetStat};
use crate::web;
use crate::log_warn;
use anyhow::Result;
use indexmap::IndexMap;
use serde_json::Value;

pub struct MoneroOcean;

impl PoolModule for MoneroOcean {
    fn name(&self) -> &'static str {
        "MoneroOcean"
    }

    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let password = ctx.field("Password");
        let wallet = ctx.wallet("XMR").unwrap_or("").to_string();
        if !ctx.info_only && (wallet.is_empty() || password.is_empty()) {
            return Ok(vec![]);
        }
        let req = match web::Cached::new("https://api.rbminer.net/data/moneroocean.json").tag(name).cycletime(120).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let pool_fee = 0.68;
        let cpu_test = ctx.cpu_cores as f64 * ctx.cpu_mhz as f64 * 4.0 / 1000.0;
        let cpu_base = if cpu_test > 0.0 { 2f64.powf(1.0 + cpu_test.log2().floor()).min(8192.0) as u32 } else { 4 };
        let gpu_base = 128u32;

        let mut groups: IndexMap<String, Vec<&Value>> = IndexMap::new();
        for v in jsonx::as_list(&req) {
            groups.entry(jsonx::s(jsonx::get(v, "algo"))).or_default().push(v);
        }
        let mut out = Vec::new();
        for (_algo, mut items) in groups {
            items.sort_by(|a, b| jsonx::num(jsonx::get(b, "profit")).partial_cmp(&jsonx::num(jsonx::get(a, "profit"))).unwrap_or(std::cmp::Ordering::Equal));
            let mut first = true;
            for v in items {
                let sym = jsonx::s(jsonx::get(v, "coin"));
                let coin = data::get_coin(&sym, "");
                let algo_norm = match &coin {
                    Some(c) => c.algo.clone(),
                    None => data::get_algorithm_coin(&jsonx::s(jsonx::get(v, "algo")), &sym),
                };
                let mut st = None;
                let mut tsl = None;
                if !ctx.info_only {
                    if !ctx.algo_allowed(&algo_norm) {
                        continue;
                    }
                    let t = jsonx::num(jsonx::get(v, "tsl"));
                    tsl = if t >= 0.0 { Some(t) } else { None };
                    let profit = jsonx::num(jsonx::get(v, "profit"));
                    if profit == 0.0 {
                        continue;
                    }
                    let ttf = jsonx::num(jsonx::get(v, "ttf"));
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, sym), profit, ctx.stat_span);
                    p.difficulty = jsonx::num(jsonx::get(v, "diff"));
                    p.hashrate = jsonx::num(jsonx::get(v, "hashrate"));
                    p.blockrate = if ttf > 0.0 { 86400.0 / ttf } else { 0.0 };
                    p.quiet = true;
                    let r = stat::set_stat(p);
                    if !first || (jsonx::num(jsonx::get(v, "hashrate")) == 0.0 && !ctx.allow_zero) {
                        continue;
                    }
                    st = Some(r);
                }
                first = false;
                let cucka = algo_norm.to_lowercase().starts_with("cucka");
                let cb = if cucka { 1 } else { cpu_base };
                let gb = if cucka { 1 } else { gpu_base };
                for ssl in [false, true] {
                    let port = if ssl { 20000u32 } else { 10000 };
                    let mut p = ctx.pool(&algo_norm).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| sym.clone());
                    p.CoinSymbol = sym.clone();
                    p.Currency = "XMR".into();
                    p.Protocol = super::protocol(ssl);
                    p.Host = "gulf.moneroocean.stream".into();
                    p.Port = port + cpu_base;
                    p.Ports = Some(PoolPorts { CPU: port + cb, GPU: port + gb, RIG: port + 8192 });
                    p.User = wallet.clone();
                    p.Pass = format!("{}:{}~{}", ctx.worker_tag(), password, jsonx::s(jsonx::get(v, "algo")));
                    p.Region = data::get_region("US");
                    p.SSL = ssl;
                    p.SSLSelfSigned = ssl;
                    p.PoolFee = pool_fee + jsonx::num(jsonx::get(v, "fee"));
                    p.Workers = jsonx::num_opt(jsonx::get(v, "worker"));
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.TSL = tsl;
                    p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    p.EthMode = if data::RE_ALGO_IS_ETHASH.is_match(&algo_norm) { "ethstratumnh".into() } else { String::new() };
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

//! Pools/2Miners.ps1

use crate::data;
use crate::jsonx;
use crate::pool::{wallet_with_payment_id, Pool, PoolCtx, PoolModule};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use crate::log_warn;
use anyhow::Result;
use serde_json::Value;

pub struct TwoMiners;

struct Coin {
    rpc: &'static str,
    symbol: &'static str,
    port: u32,
    fee: f64,
    divisor: f64,
    ssl: bool,
    cycles: f64,
}

const COINS: &[Coin] = &[
    Coin { rpc: "ae", symbol: "AE", port: 4040, fee: 1.0, divisor: 1e8, ssl: false, cycles: 0.0 },
    Coin { rpc: "beam", symbol: "BEAM", port: 5252, fee: 1.0, divisor: 1e8, ssl: true, cycles: 0.0 },
    Coin { rpc: "btg", symbol: "BTG", port: 4040, fee: 1.0, divisor: 1e8, ssl: false, cycles: 0.0 },
    Coin { rpc: "ckb", symbol: "CKB", port: 6464, fee: 1.0, divisor: 1e8, ssl: false, cycles: 0.0 },
    Coin { rpc: "ctxc", symbol: "CTXC", port: 2222, fee: 1.0, divisor: 1e18, ssl: false, cycles: 0.0 },
    Coin { rpc: "erg", symbol: "ERG", port: 8888, fee: 1.0, divisor: 1e9, ssl: false, cycles: 0.0 },
    Coin { rpc: "etc", symbol: "ETC", port: 1010, fee: 1.0, divisor: 1e18, ssl: false, cycles: 0.0 },
    Coin { rpc: "ethw", symbol: "ETHW", port: 2020, fee: 1.0, divisor: 1e18, ssl: false, cycles: 0.0 },
    Coin { rpc: "grin", symbol: "GRIN-PRI", port: 3030, fee: 1.0, divisor: 1e9, ssl: false, cycles: 42.0 },
    Coin { rpc: "kas", symbol: "KAS", port: 2020, fee: 1.0, divisor: 1e8, ssl: false, cycles: 0.0 },
    Coin { rpc: "mwc", symbol: "MWC-PRI", port: 7575, fee: 1.0, divisor: 1e9, ssl: false, cycles: 42.0 },
    Coin { rpc: "neox", symbol: "NEOX", port: 4040, fee: 1.0, divisor: 1e8, ssl: false, cycles: 0.0 },
    Coin { rpc: "nexa", symbol: "NEXA", port: 5050, fee: 1.0, divisor: 100.0, ssl: false, cycles: 0.0 },
    Coin { rpc: "rvn", symbol: "RVN", port: 6060, fee: 1.0, divisor: 1e8, ssl: false, cycles: 0.0 },
    Coin { rpc: "xna", symbol: "XNA", port: 6060, fee: 1.0, divisor: 1e8, ssl: false, cycles: 0.0 },
    Coin { rpc: "zec", symbol: "ZEC", port: 1010, fee: 1.0, divisor: 1e8, ssl: false, cycles: 0.0 },
    Coin { rpc: "zeph", symbol: "ZEPH", port: 2222, fee: 1.0, divisor: 1e12, ssl: false, cycles: 0.0 },
];

impl PoolModule for TwoMiners {
    fn name(&self) -> &'static str {
        "2Miners"
    }

    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let host_status = match web::Cached::new("https://status-api.2miners.com/").tag(name).cycletime(120).retry(5).json() {
            Ok(v) if jsonx::get(&v, "code").is_null() => v,
            _ => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let hosts: Vec<&Value> = jsonx::as_list(&host_status);
        let mut out = Vec::new();
        for c in COINS {
            let currency = c.symbol.split('-').next().unwrap_or(c.symbol);
            let Some(wallet) = ctx.wallet(currency).map(|s| s.to_string()).or(if ctx.info_only { Some(String::new()) } else { None }) else {
                continue;
            };
            let coin = data::get_coin(c.symbol, "").unwrap_or_default();
            let algo_norm = coin.algo.clone();
            if algo_norm.is_empty() || !ctx.algo_allowed(&algo_norm) {
                continue;
            }
            let host = format!("{}.2miners.com", c.rpc);
            let eth_mode = super::eth_mode_default(&algo_norm, "ethproxy");
            let mut ok = hosts.iter().any(|h| {
                let hs = jsonx::s(jsonx::get(h, "host"));
                !hs.contains("solo-") && hs.contains(&host)
            });
            let mut st = None;
            let mut tsl = None;
            let mut workers = None;
            if ok && !ctx.info_only {
                let blocks = web::Cached::new(&format!("https://{}/api/blocks", host)).tag(name).cycletime(120).delay(250).retry(5).json();
                let stats = web::Cached::new(&format!("https://{}/api/stats", host)).tag(name).cycletime(120).delay(250).retry(5).json();
                let (Ok(blocks), Ok(req)) = (blocks, stats) else {
                    log_warn!("Pool API ({}) for {} has failed. ", name, currency);
                    continue;
                };
                if !jsonx::get(&blocks, "code").is_null() || !jsonx::get(&req, "code").is_null() || !jsonx::truthy(jsonx::get(&req, "nodes")) {
                    log_warn!("Pool API ({}) for {} has failed. ", name, currency);
                    continue;
                }
                let timestamp = (jsonx::num(jsonx::get(&req, "now")) / 1000.0) as i64;
                let ts24 = timestamp - 86400;
                let mut bl: Vec<(i64, f64)> = Vec::new();
                for k in ["candidates", "immature", "matured"] {
                    for b in jsonx::as_list(jsonx::get(&blocks, k)) {
                        let t = jsonx::num(jsonx::get(b, "timestamp")) as i64;
                        if t > ts24 {
                            bl.push((t, jsonx::num(jsonx::get(b, "reward"))));
                        }
                    }
                }
                let avg_time = if bl.len() > 1 {
                    let mn = bl.iter().map(|b| b.0).min().unwrap() as f64;
                    let mx = bl.iter().map(|b| b.0).max().unwrap() as f64;
                    (mx - mn) / (bl.len() - 1) as f64
                } else {
                    timestamp as f64
                };
                let blk = if avg_time > 0.0 { (86400.0 / avg_time).trunc() } else { 0.0 };
                tsl = Some((timestamp as f64) - jsonx::num(jsonx::path(&req, &["stats", "lastBlockFound"])));
                let rewards: Vec<f64> = bl.iter().map(|b| b.1).filter(|r| *r > 0.0).collect();
                let reward = if rewards.is_empty() { 0.0 } else { rewards.iter().sum::<f64>() / rewards.len() as f64 } / c.divisor;
                let btc_price = rates::btc_price(currency);
                let nodes = jsonx::get(&req, "nodes");
                let (btc_live, hashrate) = if c.cycles > 0.0 {
                    let add: String = algo_norm.chars().filter(|x| x.is_ascii_digit()).collect();
                    let weight = if c.symbol.ends_with("-PRI") {
                        jsonx::num(jsonx::get(nodes, &format!("primaryWeight{}", add)))
                    } else {
                        jsonx::num(jsonx::get(nodes, "secondaryScale"))
                    };
                    let diff = jsonx::num(jsonx::get(nodes, "difficulty"));
                    let pbr = if diff > 0.0 { (86400.0 / c.cycles) * (weight / diff) } else { 0.0 };
                    (pbr * reward * btc_price, jsonx::num(jsonx::path(&req, &["hashrates", &add])))
                } else {
                    let hr = jsonx::num(jsonx::get(&req, "hashrate"));
                    (if hr > 0.0 { btc_price * reward * 86400.0 / avg_time / hr } else { 0.0 }, hr)
                };
                let mut p = SetStat::new(format!("{}_{}_Profit", name, c.symbol), btc_live, ctx.stat_span);
                p.hashrate = hashrate;
                p.blockrate = blk;
                p.quiet = true;
                let r = stat::set_stat(p);
                if r.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                workers = jsonx::num_opt(jsonx::get(&req, "workersTotal"));
                st = Some(r);
            }
            if !ok && !ctx.info_only {
                continue;
            }
            ok = true;
            let _ = ok;
            let (wallet, _, _) = wallet_with_payment_id(&wallet, ".");
            let mut sel: Vec<(String, u32)> = hosts
                .iter()
                .filter(|h| {
                    let hs = jsonx::s(jsonx::get(h, "host"));
                    !hs.contains("solo-") && hs.contains(&host)
                })
                .map(|h| (jsonx::s(jsonx::get(h, "host")), jsonx::num(jsonx::get(h, "port")) as u32))
                .collect();
            sel.sort_by_key(|h| h.1);
            let mut seen: Vec<String> = Vec::new();
            for (h, port) in sel {
                let ssl = c.ssl || port >= 10000;
                let k = format!("{}{}", h, ssl);
                if seen.contains(&k) {
                    continue;
                }
                seen.push(k);
                let region = if h.starts_with("asia-") {
                    "asia"
                } else if h.starts_with("us-") {
                    "us"
                } else {
                    "eu"
                };
                let mut p = ctx.pool(&algo_norm).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = coin.name.clone();
                p.CoinSymbol = currency.to_string();
                p.Currency = currency.to_string();
                p.Protocol = super::protocol(ssl);
                p.Host = h.clone();
                p.Port = if port > 0 { port } else { c.port };
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Region = data::get_region(region);
                p.SSL = ssl;
                p.PoolFee = c.fee;
                p.Workers = workers;
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.TSL = tsl;
                p.BLK = st.as_ref().map(|s| s.blockrate_average);
                p.EthMode = eth_mode.clone();
                p.Wallet = wallet.clone();
                out.push(p);
            }
        }
        let _ = util::unix_timestamp();
        Ok(out)
    }
}

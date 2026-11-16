//! Pools/CpuPool.ps1, Crazypool.ps1, Ethwmine.ps1

use crate::data;
use crate::jsonx;
use crate::log_warn;
use crate::pool::{Pool, PoolCtx, PoolModule};
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use crate::log_info;
use anyhow::Result;
use serde_json::Value;

/// 86400 / (newest - oldest) * count (one block: count), rounded
fn blk_of(times: &[f64]) -> f64 {
    let (mn, mx) = (times.iter().cloned().fold(f64::INFINITY, f64::min), times.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    ((if times.len() > 1 && mx - mn > 0.0 { 86400.0 / (mx - mn) } else { 1.0 }) * times.len() as f64).round()
}

pub struct CpuPool;

const CPUPOOL_PORTS: &[(&str, u32)] = &[("BELL", 63338), ("CPU", 63386), ("CRP", 63358), ("KOTO", 63318), ("LITB", 63398), ("MBC", 63408), ("SUGAR", 63418), ("URX", 63378), ("YTN", 63368)];

impl PoolModule for CpuPool {
    fn name(&self) -> &'static str {
        "CpuPool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let blocks = web::Cached::new("http://cpu-pool.com/api/blocks").tag(name).timeout(15).cycletime(120).json().unwrap_or_else(|_| {
            log_warn!("Pool blocks API ({}) has failed. ", name);
            Value::Null
        });
        // the stats contain a "workers" object that is no valid JSON for some parsers
        let req = match web::Cached::new("http://cpu-pool.com/api/stats").tag(name).timeout(15).cycletime(120).text() {
            Ok(t) => jsonx::parse(&t).or_else(|| jsonx::parse(&regex::Regex::new(r#""workers":\{".+?\}\},"#).unwrap().replace_all(&t, ""))),
            Err(_) => None,
        };
        let Some(req) = req else {
            log_warn!("Pool API ({}) has failed. ", name);
            return Ok(vec![]);
        };
        let mut out = Vec::new();
        for v in jsonx::values(jsonx::get(&req, "pools")) {
            let cur = jsonx::s(jsonx::get(v, "symbol"));
            let port = CPUPOOL_PORTS.iter().find(|(c, _)| *c == cur).map(|(_, p)| *p);
            if (ctx.wallet(&cur).is_none() || port.is_none()) && !ctx.info_only {
                continue;
            }
            let rpc = jsonx::s(jsonx::get(v, "name"));
            let algo = data::get_algorithm_coin(&jsonx::s(jsonx::get(v, "algorithm")), &cur);
            let user = ctx.wallet(&cur).unwrap_or("").to_string();
            let mut ts = util::unix_timestamp() as f64;
            let rt = jsonx::num(jsonx::get(&req, "time"));
            if ts < rt {
                ts = rt;
            }
            let tsl = jsonx::entries(&blocks)
                .into_iter()
                .find(|(k, _)| k.starts_with(&format!("{}-", rpc)))
                .and_then(|(_, b)| jsonx::s(b).split(':').nth(4).and_then(|x| x.parse::<f64>().ok()))
                .map(|t| ts - t / 1000.0);
            let mut st = None;
            if !ctx.info_only {
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.hashrate = jsonx::num(jsonx::get(v, "hashrate")).trunc();
                p.blockrate = jsonx::num(jsonx::get(v, "maxRoundTime"));
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
            p.CoinName = jsonx::s(jsonx::get(v, "namelong"));
            p.CoinSymbol = cur.clone();
            p.Currency = cur.clone();
            p.Protocol = "stratum+tcp".into();
            p.Host = "cpu-pool.com".into();
            p.Port = port.unwrap_or(0);
            p.User = format!("{}.{}", user, ctx.worker_tag());
            p.Pass = "x".into();
            p.Region = data::get_region("ru");
            p.PoolFee = 1.0;
            p.Workers = jsonx::num_opt(jsonx::get(v, "workerCount"));
            p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
            p.TSL = tsl;
            p.BLK = st.as_ref().map(|s| s.blockrate_average);
            p.EthMode = if crate::miner::ps_match(&algo, "^(Ethash|ProgPow|KawPow)") { "minerproxy".into() } else { String::new() };
            p.Wallet = user.clone();
            p.WTM = true;
            out.push(p);
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

/// Crazypool and Ethwmine: open-ethereum-pool style APIs
struct EthPoolCoin {
    rpc: &'static str,
    symbol: &'static str,
    ports: &'static [u32],
    fee: f64,
    regions: &'static [&'static str],
    host: fn(&str) -> String,
}

fn eth_style(ctx: &PoolCtx, coins: &[EthPoolCoin], eth_mode: &str, workers_key: &str, blocks_kinds: &[&str]) -> Result<Vec<Pool>> {
    let name = &ctx.name;
    let mut out = Vec::new();
    for c in coins {
        if ctx.wallet(c.symbol).is_none() && !ctx.info_only {
            continue;
        }
        let coin = data::get_coin(c.symbol, "").unwrap_or_default();
        let algo = coin.algo.clone();
        let eth = super::eth_mode_default(&algo, eth_mode);
        let (mut st, mut tsl, mut workers) = (None, None, None);
        if !ctx.info_only {
            let req = web::Cached::new(&format!("https://{}/api/stats", c.rpc)).tag(name).cycletime(120).json();
            let Ok(req) = req else {
                log_warn!("Pool API ({}) has failed. ", name);
                continue;
            };
            let blocks = match web::Cached::new(&format!("https://{}/api/blocks", c.rpc)).tag(name).cycletime(120).json() {
                Ok(b) => b,
                Err(_) => {
                    log_info!("Pool blocks API ({}) has failed. ", name);
                    Value::Null
                }
            };
            let ts = util::unix_timestamp() as f64;
            let times: Vec<f64> = blocks_kinds
                .iter()
                .flat_map(|k| jsonx::as_list(jsonx::get(&blocks, k)).into_iter().map(|b| jsonx::num(jsonx::get(b, "timestamp"))).collect::<Vec<_>>())
                .filter(|t| *t >= ts - 86400.0)
                .collect();
            tsl = if workers_key == "minersTotal" {
                Some(ts - jsonx::num(jsonx::path(&req, &["stats", "lastBlockFound"])))
            } else {
                Some((ts - times.iter().cloned().fold(f64::NEG_INFINITY, f64::max).max(0.0)).round())
            };
            workers = jsonx::num_opt(jsonx::get(&req, workers_key));
            let mut p = SetStat::new(format!("{}_{}_Profit", name, c.symbol), 0.0, ctx.stat_span);
            p.hashrate = jsonx::num(jsonx::get(&req, "hashrate"));
            p.blockrate = blk_of(&times);
            p.quiet = true;
            let s = stat::set_stat(p);
            if s.hashrate_live == 0.0 && !ctx.allow_zero {
                continue;
            }
            st = Some(s);
        }
        let wallet = ctx.wallet(c.symbol).unwrap_or("").to_string();
        for region in c.regions {
            for (i, port) in c.ports.iter().enumerate() {
                let ssl = i > 0;
                let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = coin.name.clone();
                p.CoinSymbol = c.symbol.into();
                p.Currency = c.symbol.into();
                p.Protocol = super::protocol(ssl);
                p.Host = (c.host)(region);
                p.Port = *port;
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x".into();
                p.Region = data::get_region(region);
                p.SSL = ssl;
                p.PoolFee = c.fee;
                p.Workers = workers;
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.TSL = tsl;
                p.BLK = st.as_ref().map(|s| s.blockrate_average);
                p.WTM = true;
                p.EthMode = eth.clone();
                p.Wallet = wallet.clone();
                out.push(p);
            }
        }
    }
    Ok(out)
}

pub struct Crazypool;

impl PoolModule for Crazypool {
    fn name(&self) -> &'static str {
        "Crazypool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        const R: &[&str] = &["sg", "eu", "us"];
        let coins = [
            EthPoolCoin { rpc: "etc.crazypool.org", symbol: "ETC", ports: &[7000, 7777], fee: 1.0, regions: R, host: |r| format!("etc-{}.crazypool.org", r) },
            EthPoolCoin { rpc: "octa.crazypool.org", symbol: "OCTA", ports: &[5225, 5885], fee: 1.0, regions: R, host: |r| format!("octa-{}.crazypool.org", r) },
        ];
        eth_style(ctx, &coins, "ethstratum", "minersTotal", &["matured", "immature"])
    }
}

pub struct Ethwmine;

impl PoolModule for Ethwmine {
    fn name(&self) -> &'static str {
        "Ethwmine"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let coins = [EthPoolCoin { rpc: "iceberg.ethwmine.com", symbol: "ETHW", ports: &[8008], fee: 1.0, regions: &["eu", "us", "asia"], host: |r| format!("{}.ethwmine.com", r) }];
        eth_style(ctx, &coins, "ethproxy", "workerTotal", &["candidates", "immature", "matured"])
    }
}

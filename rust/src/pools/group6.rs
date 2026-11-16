//! Pools/Abelpool.ps1, AccPool.ps1, Acepool.ps1

use crate::data;
use crate::jsonx;
use crate::log_warn;
use crate::pool::{Pool, PoolCtx, PoolModule};
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use anyhow::Result;

pub struct Abelpool;

impl PoolModule for Abelpool {
    fn name(&self) -> &'static str {
        "Abelpool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let cur = "ABEL";
        let w = ctx.wallet(cur).unwrap_or("").to_string();
        let (user, pass) = match w.split_once(':') {
            Some((u, p)) if !u.is_empty() && !p.is_empty() => (u.to_string(), p.to_string()),
            _ => (w.clone(), ctx.param(cur).unwrap_or("").to_string()),
        };
        if (user.is_empty() || pass.is_empty()) && !ctx.info_only {
            return Ok(vec![]);
        }
        let coin = data::get_coin(cur, "").unwrap_or_default();
        let (mut blk, mut tsl, mut st) = (None, None, None);
        if !ctx.info_only {
            match web::Cached::new("https://api.abelpool.io/api/v1/home/online/summary").tag(name).retry(3).timeout(15).cycletime(120).json() {
                Ok(r) if jsonx::num(jsonx::get(&r, "code")) == 200.0 => {
                    let d = jsonx::get(&r, "data");
                    let abt = jsonx::num(jsonx::get(d, "AvgBlockTime"));
                    blk = if abt > 0.0 { Some((86400.0 / abt).round()) } else { None };
                    tsl = Some((util::unix_timestamp() as f64 - jsonx::num(jsonx::get(d, "CreatedAt"))).round());
                }
                Ok(_) => {}
                Err(_) => log_warn!("Pool API ({}) has failed. ", name),
            }
            let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
            p.blockrate = blk.unwrap_or(0.0);
            st = Some(stat::set_stat(p));
        }
        let mut out = Vec::new();
        for region in ["us", "asia"] {
            let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
            p.CoinName = coin.name.clone();
            p.CoinSymbol = cur.into();
            p.Currency = cur.into();
            p.Protocol = "stratum+ssl".into();
            p.Host = format!("{}-service.abelpool.io", if region == "us" { "global" } else { region });
            p.Port = 27778;
            p.User = user.clone();
            p.Pass = pass.clone();
            p.Region = data::get_region(region);
            p.SSL = true;
            p.PoolFee = 1.0;
            p.BLK = if blk.is_some() { st.as_ref().map(|s| s.blockrate_average) } else { None };
            p.TSL = tsl;
            p.WTM = true;
            p.Wallet = user.clone();
            out.push(p);
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct AccPool;

const ACC: &[(&str, &str, &[u32], f64)] = &[("KAS", "kaspa.acc-pool.pw", &[16061, 16062], 0.8), ("NEXA", "nexa.acc-pool.pw", &[16011, 16012], 1.0)];
const ACC_REGIONS: &[&str] = &["eu", "ca", "us", "ru", "hk", "arg"];

impl PoolModule for AccPool {
    fn name(&self) -> &'static str {
        "AccPool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let re_series = regex::Regex::new(r"(?s)series:[^\[]+\[(\{.+\})\]").unwrap();
        let mut out = Vec::new();
        for (cur, web_host, ports, fee) in ACC {
            if ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            // the pool hashrate of the statistics page's chart
            let mut hr = 0.0;
            if let Ok(html) = web::Cached::new(&format!("https://{}/poolstatistics/", web_host)).tag(name).timeout(15).cycletime(120).text() {
                for part in html.split("<script>").filter(|p| p.contains("hashrate")) {
                    if let Some(c) = re_series.captures(part) {
                        if let Some(v) = jsonx::parse(&c[1]) {
                            let unit = jsonx::s(jsonx::get(&v, "name"));
                            let last = jsonx::as_list(jsonx::get(&v, "data")).last().map(|x| match x {
                                serde_json::Value::Array(a) => a.last().map(jsonx::s).unwrap_or_default(),
                                _ => jsonx::s(x),
                            });
                            if let Some(l) = last {
                                if !unit.is_empty() {
                                    hr = util::convert_from_hash(&format!("{}{}", l, unit));
                                }
                            }
                        }
                    }
                }
            }
            let mut st = None;
            if !ctx.info_only {
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.hashrate = hr;
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let wallet = ctx.wallet(cur).unwrap_or("").to_string();
            for (i, port) in ports.iter().enumerate() {
                let ssl = i > 0;
                for region in ACC_REGIONS {
                    let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = cur.to_string();
                    p.Currency = cur.to_string();
                    p.Protocol = super::protocol(ssl);
                    p.Host = format!("{}acc-pool.pw", if *region != "eu" { format!("{}.", region) } else { String::new() });
                    p.Port = *port;
                    p.User = format!("{}.{}", wallet, ctx.worker_tag());
                    p.Pass = "x".into();
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.PoolFee = *fee;
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.WTM = true;
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct Acepool;

impl PoolModule for Acepool {
    fn name(&self) -> &'static str {
        "Acepool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let email = ctx.field("Email");
        let mut out = Vec::new();
        for (cur, port, fee0, rpc) in [("BEAM", 3334u32, 1.0, "beam")] {
            let Some(wallet) = ctx.wallet(cur).map(|s| s.to_string()).or(if ctx.info_only { Some(String::new()) } else { None }) else {
                continue;
            };
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let mut fee = fee0;
            let (mut st, mut tsl, mut workers) = (None, None, None);
            let mut ok = true;
            if !ctx.info_only {
                match web::Cached::new(&format!("https://{}.acepool.top/pool-info.php?miningpoolstats", rpc)).tag(name).timeout(15).cycletime(120).json() {
                    Ok(r) if !jsonx::get(&r, "coin").is_null() => {
                        fee = jsonx::num(jsonx::get(&r, "feePercent"));
                        let newest = jsonx::as_list(jsonx::get(&r, "lastBlocksFound")).iter().map(|b| jsonx::num(jsonx::get(b, "timestampMilliseconds"))).fold(0.0, f64::max);
                        tsl = Some(((jsonx::num(jsonx::get(&r, "timestampMilliSeconds")) - newest) / 1000.0).round());
                        workers = jsonx::num_opt(jsonx::path(&r, &["pools", rpc, "workerCount"]));
                        let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                        p.hashrate = jsonx::num(jsonx::get(&r, "hashrate"));
                        p.blockrate = jsonx::num(jsonx::get(&r, "blocksFound24H"));
                        p.quiet = true;
                        let s = stat::set_stat(p);
                        if s.hashrate_live == 0.0 && !ctx.allow_zero {
                            continue;
                        }
                        st = Some(s);
                    }
                    _ => ok = false,
                }
                if !ok {
                    log_warn!("Pool API ({}) for {} has failed. ", name, cur);
                    continue;
                }
            }
            let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
            p.CoinName = coin.name.clone();
            p.CoinSymbol = cur.into();
            p.Currency = cur.into();
            p.Protocol = "stratum+ssl".into();
            p.Host = format!("{}.acepool.top", rpc);
            p.Port = port;
            p.User = format!("{}.{}", wallet, ctx.worker_tag());
            p.Pass = if email.is_empty() { "x".into() } else { email.clone() };
            p.Region = data::get_region("eu");
            p.SSL = true;
            p.PoolFee = fee;
            p.Workers = workers;
            p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
            p.TSL = tsl;
            p.BLK = st.as_ref().map(|s| s.blockrate_average);
            p.WTM = true;
            p.Wallet = wallet.clone();
            out.push(p);
        }
        Ok(out)
    }
}

//! Pools/Vipor.ps1, ViporSolo.ps1, WoolyPooly.ps1, WoolyPoolySolo.ps1, XdagOrg.ps1, XdagOrgSolo.ps1

use crate::data;
use crate::jsonx;
use crate::log_warn;
use crate::pool::{Pool, PoolCtx, PoolModule};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use crate::log_info;
use anyhow::Result;
use serde_json::Value;

fn age_secs(s: &str) -> Option<f64> {
    util::parse_datetime(s).map(|t| (chrono::Utc::now() - t).num_seconds() as f64)
}

/// 86400 / (newest - oldest) * count of a list of block times (one block: count)
fn blk_of(times: &[f64]) -> f64 {
    let (mn, mx) = (times.iter().cloned().fold(f64::INFINITY, f64::min), times.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    (if times.len() > 1 && mx - mn > 0.0 { 86400.0 / (mx - mn) } else { 1.0 }) * times.len() as f64
}

// ------------------------------------------------------------------------------------------

pub struct Vipor {
    pub solo: bool,
}

impl PoolModule for Vipor {
    fn name(&self) -> &'static str {
        if self.solo {
            "ViporSolo"
        } else {
            "Vipor"
        }
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut c = web::Cached::new("https://master.vipor.net/api/pools").tag(name);
        if !self.solo {
            c = c.cycletime(120);
        }
        let req = match c.json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let pools = jsonx::as_list(jsonx::get(&req, "pools"));
        if pools.len() <= 1 {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let regions = ["pl", "de", "ro", "fr", "ua", "fi", "ru", "ca", "usse", "us", "kz", "ussw", "usw", "sg", "sa", "cn", "tr", "ap"];
        let scheme = if self.solo { "SOLO" } else { "PPLNS" };
        let mut out = Vec::new();
        for v in pools {
            let cur = jsonx::s(jsonx::path(v, &["coin", "symbol"]));
            if jsonx::s(jsonx::path(v, &["paymentProcessing", "payoutScheme"])) != scheme || (ctx.wallet(&cur).is_none() && !ctx.info_only) {
                continue;
            }
            let coin = data::get_coin(&cur, "").unwrap_or_default();
            let fee = jsonx::num(jsonx::get(v, "poolFeePercent"));
            let port = jsonx::entries(jsonx::get(v, "ports")).into_iter().find(|(_, pv)| !jsonx::truthy(jsonx::get(pv, "tls"))).and_then(|(k, _)| k.parse::<u32>().ok()).unwrap_or(0);
            let mut st = None;
            let mut tsl = None;
            if !ctx.info_only {
                if self.solo {
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                    p.difficulty = jsonx::num(jsonx::path(v, &["networkStats", "networkDifficulty"]));
                    p.quiet = true;
                    st = Some(stat::set_stat(p));
                } else {
                    let phr = jsonx::num(jsonx::path(v, &["poolStats", "poolHashrate"]));
                    if phr == 0.0 && !ctx.allow_zero {
                        continue;
                    }
                    let id = jsonx::s(jsonx::get(v, "id"));
                    match web::Cached::new(&format!("https://master.vipor.net/api/v2/pools/{}/blocks?page=0&pageSize=20", id)).tag(name).timeout(15).cycletime(120).json() {
                        Ok(b) if jsonx::truthy(jsonx::get(&b, "success")) => {
                            let times: Vec<f64> = jsonx::as_list(jsonx::get(&b, "result")).iter().filter_map(|x| age_secs(&jsonx::s(jsonx::get(x, "created")))).filter(|a| *a < 86400.0).collect();
                            tsl = age_secs(&jsonx::s(jsonx::path(&b, &["stats", "lastPoolBlockTime"]))).map(|x| x.trunc());
                            let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                            p.hashrate = phr;
                            p.blockrate = blk_of(&times).round();
                            p.quiet = true;
                            st = Some(stat::set_stat(p));
                        }
                        _ => log_warn!("Pool blocks API ({}) for {} has failed. ", name, cur),
                    }
                }
            } else {
                tsl = age_secs(&jsonx::s(jsonx::get(v, "lastPoolBlockTime"))).map(|x| x.trunc());
            }
            let wallet = ctx.wallet(&cur).unwrap_or("").to_string();
            for region in regions {
                for ssl in [false, true] {
                    let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = if coin.symbol.is_empty() { cur.clone() } else { coin.symbol.clone() };
                    p.Currency = cur.clone();
                    p.Protocol = super::protocol(ssl);
                    p.Host = format!("{}.vipor.net", region);
                    p.Port = if ssl { port + 100 } else { port };
                    p.User = format!("{}.{}", wallet, ctx.worker_tag());
                    p.Pass = "x".into();
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.PoolFee = fee;
                    if self.solo {
                        p.Price = 0.0;
                        p.StablePrice = 0.0;
                        p.Difficulty = st.as_ref().map(|s| s.diff_average);
                    } else {
                        p.Workers = Some(jsonx::num(jsonx::path(v, &["poolStats", "connectedMiners"])).trunc());
                        p.BLK = st.as_ref().map(|s| s.blockrate_average);
                        p.TSL = tsl;
                        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    }
                    p.WTM = true;
                    p.ErrorRatio = st.as_ref().map(|s| s.error_ratio).unwrap_or(0.0);
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct WoolyPooly {
    pub solo: bool,
}

const WOOLY: &[(&str, u32, &str)] = &[
    ("AE", 20000, "aeternity-1"),
    ("ALPH", 3106, "alph-1"),
    ("CFX", 3094, "cfx-1"),
    ("CTXC", 40000, "cortex-1"),
    ("ERG", 3100, "ergo-1"),
    ("ETC", 35000, "etc-1"),
    ("ETHW", 3096, "ethw-1"),
    ("FIRO", 3104, "firo-1"),
    ("KAS", 3112, "kas-1"),
    ("KLS", 3132, "kls-1"),
    ("MEWC-MeowPow", 3116, "mewc-1"),
    ("MWC-Cuckatoo31", 11000, "mwc-1"),
    ("NEXA", 3124, "nexa-1"),
    ("NOVO", 3134, "novo-1"),
    ("OCTA", 3130, "octa-1"),
    ("RTM", 3110, "rtm-1"),
    ("RVN", 55555, "raven-1"),
    ("RXD", 3122, "rxd-1"),
    ("SDR", 3144, "sdr-1"),
    ("VTC", 3102, "vtc-1"),
    ("WART", 3140, "wart-1"),
    ("XEL", 3150, "xel-1"),
    ("XNA", 3128, "xna-1"),
    ("ZANO", 3146, "zano-1"),
];

impl PoolModule for WoolyPooly {
    fn name(&self) -> &'static str {
        if self.solo {
            "WoolyPoolySolo"
        } else {
            "WoolyPooly"
        }
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = match web::Cached::new("https://api.woolypooly.com/api/stats").tag(name).timeout(15).cycletime(120).delay(250).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let scheme = if self.solo { "SOLO" } else { "PPLNS" };
        let mode_of = |v: &Value| -> Value { jsonx::as_list(jsonx::get(v, "modes")).into_iter().find(|m| jsonx::s(jsonx::get(m, "payoutScheme")) == scheme).cloned().unwrap_or(Value::Null) };
        let mut out = Vec::new();
        for (symbol, port0, rpc) in WOOLY {
            let cur = symbol.split('-').next().unwrap_or(symbol).to_string();
            let rv = jsonx::get(&req, rpc);
            if rv.is_null() || (ctx.wallet(&cur).is_none() && !ctx.info_only) {
                continue;
            }
            let coin = data::get_coin(symbol, "").unwrap_or_default();
            let algo = coin.algo.clone();
            let port = if self.solo { port0 + 1 } else { *port0 };
            let eth = if data::RE_ALGO_HAS_ETHPROXY.is_match(&algo) { "qtminer".to_string() } else { String::new() };
            let algo_ok = |n: &str| n == "default" || data::get_algorithm(n) == algo;
            let mut st = None;
            let mut live = 0.0;
            let (mut tsl, mut workers) = (None, None);
            if !ctx.info_only {
                if self.solo {
                    let diff = jsonx::num(jsonx::get(rv, "netHashrate")) * jsonx::num(jsonx::get(rv, "blockTime")) / 4294967296.0;
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, symbol), 0.0, ctx.stat_span);
                    p.difficulty = diff;
                    p.quiet = true;
                    st = Some(stat::set_stat(p));
                } else {
                    let blocks = web::Cached::new(&format!("https://api.woolypooly.com/api/{}/blocks", rpc)).tag(name).timeout(20).cycletime(120).delay(250).json().map(|b| mode_of(&b)).unwrap_or_else(|_| {
                        log_info!("Pool blocks API ({}) for {} has failed. ", name, cur);
                        Value::Null
                    });
                    let stats = web::Cached::new(&format!("https://api.woolypooly.com/api/{}/stats?simple=false", rpc)).tag(name).timeout(20).cycletime(3600).delay(250).json().map(|b| mode_of(&b)).unwrap_or_else(|_| {
                        log_info!("Pool stats API ({}) for {} has failed. ", name, cur);
                        Value::Null
                    });
                    let data_m = mode_of(rv);
                    let algo_stats: Vec<&Value> = jsonx::entries(jsonx::get(&data_m, "algo_stats")).into_iter().filter(|(k, _)| algo_ok(k)).map(|(_, v)| v).collect();
                    let ts = util::unix_timestamp() as f64;
                    let ts24 = ts - 86400.0;
                    let completed: Vec<&Value> = jsonx::as_list(jsonx::get(&blocks, "matured"))
                        .into_iter()
                        .filter(|b| jsonx::num(jsonx::get(b, "timestamp")) > ts24 && !jsonx::truthy(jsonx::get(b, "orphan")) && algo_ok(&jsonx::s(jsonx::get(b, "algo"))))
                        .collect();
                    let times: Vec<f64> = completed.iter().map(|b| jsonx::num(jsonx::get(b, "timestamp"))).collect();
                    let rewards: Vec<f64> = completed.iter().map(|b| jsonx::num(jsonx::get(b, "reward"))).collect();
                    let reward = if rewards.is_empty() { 0.0 } else { rewards.iter().sum::<f64>() / rewards.len() as f64 };
                    let mut mn = times.iter().cloned().fold(f64::INFINITY, f64::min);
                    let mut mx = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let mut count = times.len() as f64;
                    let immature = jsonx::as_list(jsonx::get(&blocks, "immature"));
                    if !times.is_empty() && !immature.is_empty() {
                        let it: Vec<f64> = immature.iter().map(|b| jsonx::num(jsonx::get(b, "timestamp"))).filter(|t| *t > ts24).collect();
                        let imx = it.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        if imx > mx {
                            mx = imx;
                        }
                        if !mn.is_finite() || mn == 0.0 {
                            mn = it.iter().cloned().fold(f64::INFINITY, f64::min);
                        }
                        let orphan = jsonx::num(jsonx::get(jsonx::idx(jsonx::get(&blocks, "rate"), 0), "orphan"));
                        count += (1.0 - orphan) * it.len() as f64;
                    }
                    let blk = (if count > 1.0 && mx - mn > 0.0 { 86400.0 / (mx - mn) } else { 1.0 }) * count;
                    let newest = immature.iter().chain(jsonx::as_list(jsonx::get(&blocks, "matured")).iter()).map(|b| jsonx::num(jsonx::get(b, "timestamp"))).fold(f64::NEG_INFINITY, f64::max);
                    tsl = if newest.is_finite() { Some(ts - newest) } else { None };
                    let perf: Vec<&Value> = jsonx::as_list(jsonx::get(&stats, "perfomance")).into_iter().filter(|p| algo_ok(&jsonx::s(jsonx::get(p, "algo")))).collect();
                    let avg = |list: &[&Value]| -> f64 {
                        if list.is_empty() {
                            0.0
                        } else {
                            list.iter().map(|p| jsonx::num(jsonx::get(p, "poolHashrate"))).sum::<f64>() / list.len() as f64
                        }
                    };
                    let recent: Vec<&Value> = perf
                        .iter()
                        .copied()
                        .filter(|p| util::parse_datetime(&jsonx::s(jsonx::get(p, "created"))).map(|t| t.timestamp() as f64 >= mn).unwrap_or(false))
                        .collect();
                    let mut hr = avg(&recent);
                    if hr == 0.0 {
                        hr = avg(&perf);
                    }
                    live = if hr > 0.0 { rates::btc_price(&cur) * reward * blk / hr } else { 0.0 };
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, symbol), live, ctx.stat_span);
                    p.hashrate = algo_stats.first().map(|a| jsonx::num(jsonx::get(a, "hashrate"))).unwrap_or(0.0);
                    p.blockrate = blk;
                    p.quiet = true;
                    let s = stat::set_stat(p);
                    if s.hashrate_live == 0.0 && !ctx.allow_zero {
                        continue;
                    }
                    workers = algo_stats.first().and_then(|a| jsonx::num_opt(jsonx::get(a, "minersTotal")));
                    st = Some(s);
                }
            }
            let wallet = ctx.wallet(&cur).unwrap_or("").to_string();
            for ssl in [false, true] {
                let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = coin.name.clone();
                p.CoinSymbol = cur.clone();
                p.Currency = cur.clone();
                p.Protocol = super::protocol(ssl);
                p.Host = "pool.woolypooly.com".into();
                p.Port = port;
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x".into();
                p.Region = data::get_region("eu");
                p.SSL = ssl;
                p.PoolFee = jsonx::num(jsonx::get(rv, "fee"));
                if self.solo {
                    p.Price = 0.0;
                    p.StablePrice = 0.0;
                    p.MarginOfError = 0.0;
                    p.WTM = true;
                    p.Difficulty = st.as_ref().map(|s| s.diff_average);
                    p.SoloMining = true;
                } else {
                    p.WTM = live == 0.0;
                    p.Workers = workers;
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.TSL = tsl;
                    p.BLK = st.as_ref().map(|s| s.blockrate_average);
                }
                p.EthMode = eth.clone();
                p.Wallet = wallet.clone();
                out.push(p);
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct XdagOrg {
    pub solo: bool,
}

impl PoolModule for XdagOrg {
    fn name(&self) -> &'static str {
        if self.solo {
            "XdagOrgSolo"
        } else {
            "XdagOrg"
        }
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let pools = web::Cached::new("https://xdag.org/api/pools").tag(name).cycletime(86400).json().unwrap_or(Value::Null);
        let pat = if self.solo { "SOLO" } else { "EQUAL" };
        let mut status: Option<Value> = None;
        for p in jsonx::as_list(&pools).into_iter().filter(|p| crate::miner::ps_match(&jsonx::s(jsonx::get(p, "name")), pat)) {
            let id = jsonx::s(jsonx::get(p, "id"));
            if id.is_empty() {
                continue;
            }
            if let Ok(s) = web::Cached::new(&format!("https://xdag.org/api/status?pool_id={}", id)).tag(name).cycletime(120).json() {
                let ok = jsonx::s(jsonx::get(&s, "status")) == "online" && jsonx::num(jsonx::get(&s, "workers_count")) < jsonx::num(jsonx::get(&s, "workers_limit"));
                status = Some(s);
                if ok {
                    break;
                }
            }
        }
        let Some(req) = status else {
            log_warn!("Pool API ({}) has failed. ", name);
            return Ok(vec![]);
        };
        if jsonx::num(jsonx::get(&req, "workers_count")) >= jsonx::num(jsonx::get(&req, "workers_limit")) {
            log_warn!("Pool API ({}) all slots are full. Waiting for an empty slot. ", name);
            return Ok(vec![]);
        }
        let cur = "XDAG";
        let coin = data::get_coin(cur, "").unwrap_or_default();
        let bi = jsonx::num(jsonx::get(&req, "block_interval_seconds"));
        let mut st = None;
        if !ctx.info_only {
            let mut p = SetStat::new(format!("{}_{}_Profit", name, coin.algo), 0.0, ctx.stat_span);
            p.hashrate = jsonx::num(jsonx::idx(jsonx::path(&req, &["stats", "hashrate"]), 0));
            if !self.solo && bi > 0.0 {
                p.blockrate = 86400.0 / bi;
            }
            p.quiet = true;
            let s = stat::set_stat(p);
            if s.hashrate_live == 0.0 && !ctx.allow_zero {
                return Ok(vec![]);
            }
            st = Some(s);
        }
        let user = ctx.wallet(cur).map(|s| s.to_string());
        if user.is_none() && !ctx.info_only {
            return Ok(vec![]);
        }
        let params = ctx.param(cur).map(|p| format!(",{}", p)).unwrap_or_default();
        let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
        p.CoinName = coin.name.clone();
        p.CoinSymbol = cur.into();
        p.Currency = cur.into();
        p.Protocol = "stratum+tcp".into();
        p.Host = jsonx::s(jsonx::get(&req, "mining_stratum_domain"));
        p.Port = jsonx::num(jsonx::get(&req, "mining_stratum_port")) as u32;
        p.User = user.clone().unwrap_or_default();
        p.Pass = format!("{}{}", ctx.worker_tag(), params);
        p.Region = data::get_region("de");
        p.PoolFee = jsonx::num(jsonx::get(&req, "fee_percent"));
        p.Workers = jsonx::num_opt(jsonx::get(&req, "workersNum"));
        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
        if self.solo {
            p.Difficulty = st.as_ref().map(|s| s.diff_average);
            p.SoloMining = true;
        } else {
            p.BLK = st.as_ref().map(|s| s.blockrate_average);
        }
        p.WTM = true;
        p.ErrorRatio = st.as_ref().map(|s| s.error_ratio).unwrap_or(0.0);
        p.EthMode = "stratum".into();
        p.Wallet = user.unwrap_or_default();
        Ok(vec![p])
    }
}

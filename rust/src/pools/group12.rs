//! Pools/SuprNova.ps1, ViaBTC.ps1, YadaMiners.ps1, ZpoolCoins.ps1

use crate::data;
use crate::jsonx;
use crate::log_info;
use crate::log_warn;
use crate::pool::{Pool, PoolCtx, PoolModule, PoolPorts};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

pub struct SuprNova;

/// a plain port or a CPU/GPU port pair
#[derive(Clone, Copy)]
enum SnPort {
    P(u32),
    CpuGpu(u32, u32),
}
use SnPort::*;

/// symbol, ports, regions
const SUPRNOVA: &[(&str, &[SnPort], &[&str])] = &[
    ("BC3", &[P(7701), P(7704)], &["eu"]),
    ("BLOZ", &[P(7304)], &["eu"]),
    ("C64", &[P(6464), P(6469)], &["eu", "us-east", "asia"]),
    ("DCR", &[P(9332), P(9336)], &["eu", "us-east", "asia"]),
    ("DGB", &[P(8531)], &["eu"]),
    ("FAIR", &[P(3833), P(3834)], &["eu", "asia"]),
    ("GAP", &[P(2433)], &["eu"]),
    ("GRS", &[P(5544)], &["eu", "us-east"]),
    ("JUNO", &[P(8383)], &["eu", "us-east"]),
    ("KRX", &[CpuGpu(4401, 4404)], &["eu", "us-east"]),
    ("LPEPE", &[P(3633), P(3634)], &["eu", "asia"]),
    ("NOID", &[P(3337)], &["eu", "us-east"]),
    ("NPT", &[CpuGpu(3832, 3833)], &["eu"]),
    ("OBTC", &[CpuGpu(4074, 4075)], &["eu"]),
    ("PXC", &[P(2026), P(2027)], &["eu"]),
    ("QTC", &[P(5555), P(5557)], &["eu", "us-east", "asia"]),
    ("QUAN", &[P(7072), P(7074)], &["eu"]),
    ("PRL", &[P(3373), P(3374)], &["eu", "us-east", "asia"]),
    ("PROX", &[P(3355)], &["eu", "asia"]),
    ("RIC", &[P(5000)], &["eu"]),
    ("RTM", &[P(6273)], &["eu"]),
    ("RVN", &[P(8888), P(8889)], &["eu"]),
    ("TAPE", &[P(3351)], &["eu"]),
    ("TSC", &[P(3309)], &["eu"]),
    ("VELA", &[P(7601), P(7604)], &["eu"]),
    ("VTC", &[P(1777), P(1780)], &["eu"]),
    ("XEL", &[P(3333)], &["eu", "us-east", "asia"]),
    ("XNT", &[CpuGpu(3832, 3833)], &["eu"]),
    ("ZEC", &[P(3732)], &["eu"]),
    ("XMR", &[P(6665), P(6666)], &["eu", "us-east"]),
];

impl PoolModule for SuprNova {
    fn name(&self) -> &'static str {
        "SuprNova"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = match web::Cached::new("https://www.suprnova.cc/api.php").tag(name).cycletime(120).delay(750).timeout(30).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        if jsonx::s(jsonx::get(&req, "status")) != "ok" {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let stratums: HashMap<&str, &str> = [("us-east", "stratum-us"), ("asia", "stratum-apac")].into_iter().collect();
        let mut out = Vec::new();
        for v in jsonx::as_list(jsonx::get(&req, "pools")) {
            if jsonx::truthy(jsonx::get(v, "coming_soon")) {
                continue;
            }
            let id = jsonx::s(jsonx::get(v, "id"));
            let sym = if id == "quantus" { "QUAN".to_string() } else { jsonx::s(jsonx::path(v, &["coin", "symbol"])) };
            if ctx.wallet(&sym).is_none() && !ctx.info_only {
                continue;
            }
            let fee = jsonx::num_opt(jsonx::path(v, &["mining", "pool_fee_percent"])).unwrap_or(1.0);
            let Some(coin) = data::get_coin(&sym, &jsonx::s(jsonx::path(v, &["coin", "algorithm"]))) else {
                log_warn!("Pool {}: missing coin {} in db", name, sym);
                continue;
            };
            let cur = if coin.symbol.is_empty() { sym.clone() } else { coin.symbol.clone() };
            let Some((_, ports, regions)) = SUPRNOVA.iter().find(|(s, _, _)| *s == cur) else {
                log_warn!("Pool {}: missing coindata {}", name, cur);
                continue;
            };
            let eth = super::eth_mode_default(&coin.algo, "ethproxy");
            let hashrate = jsonx::num_opt(jsonx::path(v, &["stats", "hashrate", "pool"]));
            let workers = jsonx::num_opt(jsonx::path(v, &["stats", "workers"]));
            let mut tsl = None;
            let api = jsonx::s(jsonx::path(v, &["urls", "api"]));
            if !ctx.info_only && !api.is_empty() {
                match web::Cached::new(&api).tag(name).cycletime(120).delay(250).timeout(30).json() {
                    Ok(sub) => {
                        let lbf = jsonx::path(&sub, &["pool", "lastBlockFound", "foundAt"]);
                        if !lbf.is_null() {
                            tsl = Some((util::unix_timestamp() as f64 - jsonx::num(lbf).round()).round());
                        }
                    }
                    Err(_) => log_info!("Pool API ({}) has failed for pool {}. ", name, id),
                }
            }
            let wallet = ctx.wallet(&cur).unwrap_or("").to_string();
            for region in regions.iter() {
                let host = format!("{}.suprnova.cc", stratums.get(region).map(|s| s.to_string()).unwrap_or_else(|| id.clone()));
                for (i, port) in ports.iter().enumerate() {
                    let ssl = i > 0;
                    let mut p = ctx.pool(&coin.algo);
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = cur.clone();
                    p.Currency = cur.clone();
                    // the original sends a bare "ssl" for the SSL ports
                    p.Protocol = if ssl { "ssl".into() } else { "stratum+tcp".into() };
                    p.Host = host.clone();
                    match port {
                        P(x) => p.Port = *x,
                        CpuGpu(c, g) => {
                            p.Port = *c;
                            p.Ports = Some(PoolPorts { CPU: *c, GPU: *g, RIG: *g });
                        }
                    }
                    p.User = format!("{}.{}", wallet, ctx.worker_tag());
                    p.Pass = "x".into();
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.PoolFee = fee;
                    p.Workers = workers;
                    p.Hashrate = hashrate;
                    p.TSL = tsl;
                    p.WTM = true;
                    p.EthMode = eth.clone();
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct ViaBTC;

impl PoolModule for ViaBTC {
    fn name(&self) -> &'static str {
        "ViaBTC"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut out = Vec::new();
        for (stratum, cur, port) in [("mining.viabtc.io", "ETC", 3010u32)] {
            if ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let eth = super::eth_mode_default(&coin.algo, "ethstratumnh");
            // the original switches over the literal string "ETC-PaymentMode", the pool field of that name is what it means
            let (fee, solo) = match ctx.field(&format!("{}-PaymentMode", cur)).to_lowercase().as_str() {
                "pplns" => (2.0, false),
                "solo" => (1.0, true),
                _ => (4.0, false),
            };
            let (mut st, mut tsl, mut state) = (None, None, Value::Null);
            if !ctx.info_only {
                let get = |u: String| -> Option<Value> {
                    web::Cached::new(&u).tag(name).retry(5).cycletime(120).delay(250).json().ok().filter(|r| jsonx::s(jsonx::get(r, "message")) == "OK")
                };
                let (Some(s), Some(b)) = (
                    get(format!("https://www.viabtc.com/res/pool/{}/state", cur)),
                    get(format!("https://www.viabtc.com/res/pool/{}/block?page=1&limit=50", cur)),
                ) else {
                    log_warn!("Pool API ({}) for {} has failed. ", name, cur);
                    continue;
                };
                state = jsonx::as_list(jsonx::get(&s, "data")).into_iter().find(|d| jsonx::s(jsonx::get(d, "coin")) == cur).cloned().unwrap_or(Value::Null);
                let ts = util::unix_timestamp() as f64;
                let blocks: Vec<&Value> = jsonx::as_list(jsonx::path(&b, &["data", "data"]))
                    .into_iter()
                    .filter(|x| jsonx::s(jsonx::get(x, "coin")) == cur && !jsonx::truthy(jsonx::get(x, "solo_block")) && jsonx::num(jsonx::get(x, "time")) > ts - 86400.0)
                    .collect();
                let times: Vec<f64> = blocks.iter().map(|x| jsonx::num(jsonx::get(x, "time"))).collect();
                let mx = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let mn = times.iter().cloned().fold(f64::INFINITY, f64::min);
                let avg_time = if times.len() > 1 { (mx - mn) / (times.len() - 1) as f64 } else { ts };
                let blk = if avg_time != 0.0 { (86400.0 / avg_time).round() } else { 0.0 };
                tsl = Some(ts - if times.is_empty() { 0.0 } else { mx });
                let rewards: Vec<f64> = blocks.iter().map(|x| jsonx::num(jsonx::get(x, "reward"))).filter(|r| *r > 0.0).collect();
                let reward = if rewards.is_empty() { 0.0 } else { rewards.iter().sum::<f64>() / rewards.len() as f64 };
                let btc = match (rates::get(cur), rates::get("USD")) {
                    (Some(r), _) => 1.0 / r,
                    (None, Some(usd)) if jsonx::s(jsonx::get(&state, "pricing_currency")) == "USD" => jsonx::num(jsonx::get(&state, "coin_price")) / usd,
                    _ => 0.0,
                };
                let hr = jsonx::num(jsonx::get(&state, "pool_hashrate"));
                let live = if hr > 0.0 && avg_time != 0.0 { btc * reward * 86400.0 / avg_time / hr } else { 0.0 };
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), live, ctx.stat_span);
                p.hashrate = hr;
                p.blockrate = blk;
                p.difficulty = jsonx::num(jsonx::get(&state, "curr_diff")) / 4294967296.0;
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let wallet = ctx.wallet(cur).unwrap_or("").to_string();
            let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
            p.CoinName = coin.name.clone();
            p.CoinSymbol = cur.into();
            p.Currency = cur.into();
            p.Protocol = "stratum+tcp".into();
            p.Host = stratum.into();
            p.Port = port;
            p.User = format!("{}.{}", wallet, ctx.worker_tag());
            p.Pass = "x".into();
            p.Region = data::get_region("asia");
            p.PoolFee = fee;
            if !solo {
                p.Workers = Some(jsonx::num(jsonx::get(&state, "curr_connections")).round());
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.TSL = tsl;
                p.BLK = st.as_ref().map(|s| s.blockrate_average);
            }
            p.Difficulty = st.as_ref().map(|s| s.diff_average);
            p.SoloMining = solo;
            p.EthMode = eth;
            p.Wallet = wallet;
            out.push(p);
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct YadaMiners;

impl PoolModule for YadaMiners {
    fn name(&self) -> &'static str {
        "YadaMiners"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let cur = "YDA";
        if ctx.wallet(cur).is_none() && !ctx.info_only {
            return Ok(vec![]);
        }
        let coin = data::get_coin(cur, "").unwrap_or_default();
        let (mut st, mut blk, mut tsl, mut profit, mut req) = (None, None, None, 0.0, Value::Null);
        if !ctx.info_only {
            req = web::Cached::new("http://yadaminers.pl/pool-info").tag(name).retry(3).timeout(15).cycletime(120).json().unwrap_or(Value::Null);
            let pl = jsonx::get(&req, "pool");
            if jsonx::truthy(pl) {
                // "5 minutes 3 seconds": minutes, hours, days and years count (the seconds do not, like the original)
                let s = regex::Regex::new(r"(\d)\s+(\w)").unwrap().replace_all(&jsonx::s(jsonx::get(pl, "avg_block_time")), "$1-$2").into_owned();
                let mut secs = 0.0;
                for part in s.split_whitespace() {
                    let mut it = part.split('-');
                    let n = it.next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                    let unit = it.next().unwrap_or("");
                    let m = if unit.starts_with('m') {
                        60.0
                    } else if unit.starts_with('h') {
                        3600.0
                    } else if unit.starts_with('d') {
                        86400.0
                    } else if unit.starts_with('y') {
                        31536000.0
                    } else {
                        0.0
                    };
                    secs += n * m;
                }
                let info = web::Cached::new("http://yadaminers.pl/market-info").tag(name).retry(3).timeout(15).cycletime(120).json().unwrap_or(Value::Null);
                if secs > 0.0 {
                    let hps = jsonx::num(jsonx::get(pl, "hashes_per_second"));
                    profit = if hps != 0.0 { jsonx::num(jsonx::get(&info, "last_btc")) * jsonx::num(jsonx::path(&req, &["network", "reward"])) * 86400.0 / secs / hps } else { 0.0 };
                    blk = Some((86400.0 / secs).round());
                }
                let newest = jsonx::pluck(jsonx::get(pl, "last_five_blocks"), "timestamp").into_iter().map(jsonx::num).fold(0.0, f64::max);
                tsl = Some((util::unix_timestamp() as f64 - newest).round());
            }
            let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), profit, ctx.stat_span);
            p.hashrate = jsonx::num(jsonx::path(&req, &["pool", "hashes_per_second"]));
            p.blockrate = blk.unwrap_or(0.0);
            let s = stat::set_stat(p);
            if s.hashrate_live == 0.0 && !ctx.allow_zero {
                return Ok(vec![]);
            }
            st = Some(s);
        }
        let wallet = ctx.wallet(cur).unwrap_or("").to_string();
        let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
        if profit == 0.0 {
            p.Price = 0.0;
            p.StablePrice = 0.0;
            p.MarginOfError = 0.0;
        }
        p.CoinName = coin.name.clone();
        p.CoinSymbol = cur.into();
        p.Currency = cur.into();
        p.Protocol = "stratum+tcp".into();
        p.Host = "yadaminers.pl".into();
        p.Port = 3333;
        p.User = wallet.clone();
        p.Pass = "x".into();
        p.Region = data::get_region("eu");
        p.PoolFee = jsonx::num(jsonx::path(&req, &["pool", "pool_fee"])) * 100.0;
        p.Workers = Some(jsonx::num(jsonx::path(&req, &["pool", "worker_count"])).round());
        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
        p.BLK = if blk.is_some() { st.as_ref().map(|s| s.blockrate_average) } else { None };
        p.TSL = tsl;
        p.WTM = profit == 0.0;
        // the original sets the undefined $Pool_User here
        p.Wallet = wallet;
        Ok(vec![p])
    }
}

// ------------------------------------------------------------------------------------------

pub struct ZpoolCoins;

impl PoolModule for ZpoolCoins {
    fn name(&self) -> &'static str {
        "ZpoolCoins"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let coins = match web::Cached::new("https://www.zpool.ca/api/currencies").tag(name).cycletime(120).delay(750).timeout(30).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool currencies API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        if jsonx::keys(&coins).len() <= 1 {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let status = web::Cached::new("https://www.zpool.ca/api/status").tag(name).cycletime(120).delay(750).timeout(30).json().unwrap_or_else(|_| {
            log_warn!("Pool API ({}) has failed. ", name);
            Value::Null
        });
        let regions = ["na", "eu", "sea", "jp"];
        let mut currencies: Vec<String> = ["BTC", "LTC", "DASH", "DGB", "KMD", "RVN", "DOGE"].iter().map(|s| s.to_string()).collect();
        for (k, v) in jsonx::entries(&coins) {
            if jsonx::s(jsonx::get(v, "conversion_disabled")) != "1" {
                let s = jsonx::get(v, "symbol");
                currencies.push(if s.is_null() { k.clone() } else { jsonx::s(s) });
            }
        }
        let mut seen = std::collections::HashSet::new();
        currencies.retain(|c| seen.insert(c.clone()) && (ctx.wallet(c).is_some() || ctx.info_only));
        let mut ae = ctx.field("AECurrency");
        if ae.is_empty() {
            ae = currencies.first().cloned().unwrap_or_default();
        }
        let cs = &ctx.cfg.coin_symbol;
        let ecs = &ctx.cfg.exclude_coin_symbol;
        let mut algos: HashMap<String, String> = HashMap::new();
        let mut out = Vec::new();
        for (sym, v) in jsonx::entries(&coins) {
            let algo = jsonx::s(jsonx::get(v, "algo"));
            if algo.is_empty() {
                continue;
            }
            if !ctx.info_only && !((cs.is_empty() || cs.iter().any(|c| c.eq_ignore_ascii_case(&sym))) && (ecs.is_empty() || !ecs.iter().any(|c| c.eq_ignore_ascii_case(&sym)))) {
                continue;
            }
            let coin_name = jsonx::s(jsonx::get(v, "name"));
            let st_algo = jsonx::get(&status, &algo);
            let pool_fee = if jsonx::truthy(st_algo) { jsonx::num(jsonx::get(st_algo, "fees")) } else { 0.45 };
            let cur = if jsonx::truthy(jsonx::get(v, "symbol")) { jsonx::s(jsonx::get(v, "symbol")) } else { sym.clone() };
            let ex_cur = if ctx.wallet(&cur).is_some() || ctx.info_only { cur.clone() } else { ae.clone() };
            if !ctx.info_only && jsonx::num(jsonx::get(v, "estimate")) == 0.0 && ex_cur != sym {
                continue;
            }
            let algo_norm = if algo == "ethash" || algo == "kawpow" {
                data::get_algorithm_coin(&algo, &sym)
            } else {
                algos.entry(algo.clone()).or_insert_with(|| data::get_algorithm(&algo)).clone()
            };
            // the firopow port mines SCC with the Firo DAG profile, not Firo: SCC is offered as SCCPow
            if algo_norm == "FiroPoW" && cur == "SCC" {
                continue;
            }
            if !ctx.algo_allowed(&algo_norm) {
                continue;
            }
            let eth = if data::RE_ALGO_HAS_DAGSIZE.is_match(&algo_norm) {
                if data::RE_ALGO_IS_ETHASH.is_match(&algo_norm) {
                    "ethproxy".to_string()
                } else {
                    "stratum".to_string()
                }
            } else {
                String::new()
            };
            let factor = jsonx::num(jsonx::get(st_algo, "mbtc_mh_factor"));
            if factor <= 0.0 {
                log_info!("{}: Unable to determine divisor for algorithm {}. ", name, algo);
                continue;
            }
            let divisor = 1e6 * factor;
            let tsl = jsonx::num_opt(jsonx::get(v, "timesincelast"));
            let mut st = None;
            if !ctx.info_only {
                // a thin coin's estimate can sit far above what the port paid: cap an estimate above 10x at the port's actual_last24h
                let mut est = jsonx::num(jsonx::get(v, "estimate"));
                let a24 = jsonx::num(jsonx::get(st_algo, "actual_last24h")) / 1000.0;
                if a24 > 0.0 && est > a24 * 10.0 {
                    est = a24;
                }
                let mut p = SetStat::new(format!("{}_{}_Profit", name, sym), est / divisor, ctx.stat_span);
                p.change_detection = true;
                p.hashrate = jsonx::num(jsonx::get(v, "hashrate"));
                p.blockrate = jsonx::num(jsonx::get(v, "24h_blocks"));
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            if !(((!ex_cur.is_empty()) && ctx.wallet(&ex_cur).is_some()) || ctx.info_only) {
                continue;
            }
            let params = ctx
                .param(&format!("{}-{}", ex_cur, sym))
                .map(|p| format!(",{}", p))
                .or_else(|| ctx.param(&ex_cur).map(|p| format!(",{}", p)))
                .unwrap_or_default();
            for ssl in [false, true] {
                let port = if ssl {
                    let p = jsonx::num(jsonx::get(v, "ssl_port")) as u32;
                    if p == 0 {
                        continue;
                    }
                    p
                } else {
                    jsonx::num(jsonx::get(v, "port")) as u32
                };
                for region in regions {
                    let mut p = ctx.pool(&algo_norm).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin_name.clone();
                    p.CoinSymbol = cur.clone();
                    p.Currency = ex_cur.clone();
                    p.Protocol = super::protocol(ssl);
                    p.Host = format!("{}.{}.mine.zpool.ca", algo, region);
                    p.Port = port;
                    p.User = ctx.wallet(&ex_cur).unwrap_or("").to_string();
                    p.Pass = format!("{},c={},zap={}{{diff:,sd=$difficulty}}{}", ctx.worker_tag(), ex_cur, sym, params);
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.SSLSelfSigned = ssl;
                    p.PoolFee = pool_fee;
                    p.Workers = jsonx::num_opt(jsonx::get(v, "workers"));
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    p.TSL = tsl;
                    p.EthMode = eth.clone();
                    p.Wallet = ctx.wallet(&ex_cur).unwrap_or("").to_string();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

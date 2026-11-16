//! Pools/EpicMine.ps1, FlockPool.ps1, HashVault.ps1, Hashcryptos.ps1, Hashpool.ps1

use crate::data;
use crate::jsonx;
use crate::log_info;
use crate::log_warn;
use crate::pool::{self, Pool, PoolCtx, PoolModule, PoolPorts};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use anyhow::Result;
use chrono::Duration;
use serde_json::Value;

/// 86400 / (newest - oldest) * count (one block: count), [int]
pub(crate) fn blk_rate(times: &[f64], day: f64) -> f64 {
    if times.is_empty() {
        return 0.0;
    }
    let mn = times.iter().cloned().fold(f64::INFINITY, f64::min);
    let mx = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    ((if times.len() > 1 && mx - mn > 0.0 { day / (mx - mn) } else { 1.0 }) * times.len() as f64).round()
}

fn stat_file_exists(name: &str) -> bool {
    stat::get_stat(name, "").is_some()
}

pub struct EpicMine;

impl PoolModule for EpicMine {
    fn name(&self) -> &'static str {
        "EpicMine"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let cur = "EPIC";
        let ports = [3333u32, 3334];
        let alias = ctx.field("MiningAlias");
        let user = if !alias.is_empty() { alias } else { ctx.wallet(cur).unwrap_or("").to_string() };
        if !ctx.info_only && user.is_empty() {
            return Ok(vec![]);
        }
        let req = web::Cached::new("https://api.epicmine.io/pool/getstats").tag(name).cycletime(120).retry(5).json().unwrap_or(Value::Null);
        if !jsonx::truthy(jsonx::get(&req, "poolInfo")) || !jsonx::truthy(jsonx::get(&req, "poolStats")) {
            log_warn!("Pool API ({}) has failed. ", name);
            return Ok(vec![]);
        }
        let mut locations: Vec<(String, String)> = web::Cached::new("https://api.epicmine.io/pool/getlocations")
            .tag(name)
            .cycletime(86400)
            .retry(5)
            .json()
            .map(|v| jsonx::as_list(&v).into_iter().map(|l| (jsonx::s(jsonx::get(l, "location")), jsonx::s(jsonx::get(l, "host")))).collect())
            .unwrap_or_default();
        if locations.is_empty() || locations[0].1.is_empty() {
            locations = vec![
                ("Germany, Europe".into(), "de.epicmine.io".into()),
                ("United States, America".into(), "us.epicmine.io".into()),
                ("Singapore, Asia".into(), "sg.epicmine.io".into()),
            ];
        }
        let algos = ["RandomX", "ProgPow", "Cuckoo"];
        let mut weight = [0.48, 0.48, 0.04];
        let mut wtm = false;
        let mut blocks_per_day_avg = 0.0;
        let (mut nethash, mut reward) = (Value::Null, Value::Null);
        if !ctx.info_only {
            nethash = web::Cached::new("https://api.epicmine.io/explorer/getnetworkhashrate").tag(name).cycletime(120).retry(5).json().unwrap_or(Value::Null);
            reward = web::Cached::new("https://api.epicmine.io/explorer/getblockreward").tag(name).cycletime(7200).retry(5).json().unwrap_or(Value::Null);
            let netper = web::Cached::new("https://explorer.epiccash.com/epic_explorer/v1/blockchain_block/blockminedchart?Interval=1%20week&FromDate=&ToDate=")
                .tag(name)
                .cycletime(7200)
                .retry(5)
                .json()
                .unwrap_or(Value::Null);
            if !jsonx::truthy(&nethash) || !jsonx::truthy(&reward) || !jsonx::truthy(&netper) {
                wtm = true;
            }
            if !wtm && jsonx::num(jsonx::get(&netper, "status")) == 200.0 {
                let r = jsonx::get(&netper, "response");
                let days = jsonx::count(jsonx::get(r, "date")) as i64 - 1;
                let mut bpd = 0.0;
                weight = [0.0; 3];
                if days > 0 {
                    for i in 0..days as usize {
                        for (j, a) in algos.iter().enumerate() {
                            bpd += jsonx::num(jsonx::idx(jsonx::get(r, a), i));
                            weight[j] += jsonx::num(jsonx::idx(jsonx::get(r, &format!("{}per", a)), i));
                        }
                    }
                    blocks_per_day_avg = bpd / days as f64;
                    for w in weight.iter_mut() {
                        *w = (*w / days as f64).round() / 100.0;
                    }
                }
            }
        }
        let fee = jsonx::num(jsonx::path(&req, &["poolInfo", "fee"]));
        let ts = util::unix_timestamp() as f64;
        let mut out = Vec::new();
        for (j, a) in algos.iter().enumerate() {
            let coin = data::get_coin(cur, a).unwrap_or_default();
            let mut st = None;
            let mut tsl = None;
            if !ctx.info_only {
                let mut blocks: Vec<f64> = jsonx::as_list(jsonx::path(&req, &["poolBlocks", "latest"]))
                    .into_iter()
                    .filter(|b| jsonx::s(jsonx::get(b, "algo")) == *a && jsonx::num(jsonx::get(b, "time")) >= ts - 86400.0)
                    .map(|b| jsonx::num(jsonx::get(b, "time")))
                    .collect();
                blocks.sort_by(|x, y| y.partial_cmp(x).unwrap_or(std::cmp::Ordering::Equal));
                tsl = Some(ts - blocks.first().copied().unwrap_or(0.0).trunc());
                let live = if !wtm {
                    let btc_price = rates::btc_price(cur);
                    let nh = jsonx::num(jsonx::get(&nethash, a));
                    if nh > 0.0 { blocks_per_day_avg * weight[j] * jsonx::num(jsonx::get(&reward, "miners")) * btc_price / nh } else { 0.0 }
                } else {
                    0.0
                };
                let mut p = SetStat::new(format!("{}_{}_Profit", name, coin.algo), live, ctx.stat_span);
                p.hashrate = jsonx::num(jsonx::path(&req, &["poolStats", &format!("hashrate{}", a)]));
                p.blockrate = blk_rate(&blocks, 86400.0);
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    // a return inside the foreach of the script ends the module
                    return Ok(out);
                }
                st = Some(s);
            }
            for (i, port) in ports.iter().enumerate() {
                // the original sends "stratum+tsl" (sic) for the plain port
                let ssl = i > 0;
                for (location, host) in &locations {
                    let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = cur.into();
                    p.Currency = cur.into();
                    p.Protocol = super::protocol(ssl);
                    p.Host = host.clone();
                    p.Port = *port;
                    p.User = format!("{}.{}", user, ctx.worker_tag());
                    p.Region = data::get_region(location);
                    p.SSL = ssl;
                    p.PoolFee = fee;
                    p.Workers = jsonx::num_opt(jsonx::path(&req, &["poolStats", &format!("online{}", a)]));
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.TSL = tsl;
                    p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    p.WTM = wtm;
                    p.Wallet = user.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct FlockPool;

impl PoolModule for FlockPool {
    fn name(&self) -> &'static str {
        "FlockPool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = match web::Cached::new("https://flockpool.com/api/v1/pool-stats").tag(name).cycletime(120).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let regions = ["eu", "us", "us-west", "asia"];
        let cur = "RTM";
        let coin = data::get_coin(cur, "").unwrap_or_default();
        let mut st = None;
        if !ctx.info_only {
            // explorer.raptoreum.com retired its block list API, so there is no per-pool block data any more
            let mut hr: Option<f64> = None;
            for r in jsonx::as_list(jsonx::get(&req, "regions")) {
                let vals: Vec<f64> = jsonx::pluck(jsonx::get(r, "hashrate_graph"), "hashrate").into_iter().map(|x| jsonx::num(x).trunc()).collect();
                let last: Vec<f64> = vals.iter().rev().take(15).rev().cloned().collect();
                let used = &last[..last.len().saturating_sub(1)];
                let region_hr = if used.is_empty() { 0.0 } else { (used.iter().sum::<f64>() / used.len() as f64).round() };
                if region_hr > 0.0 {
                    hr = Some(hr.unwrap_or(0.0) + region_hr);
                }
            }
            let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
            p.hashrate = hr.unwrap_or(0.0);
            p.quiet = true;
            let s = stat::set_stat(p);
            if s.hashrate_live == 0.0 && !ctx.allow_zero {
                return Ok(vec![]);
            }
            st = Some(s);
        }
        let mut out = Vec::new();
        let wallet = ctx.wallet(cur).unwrap_or("").to_string();
        if wallet.is_empty() && !ctx.info_only {
            return Ok(out);
        }
        for (i, port) in [4444u32, 5555].iter().enumerate() {
            let ssl = i > 0;
            for region in regions {
                let mut p = ctx.pool(&coin.algo);
                if let Some(s) = &st {
                    p.Updated = s.updated;
                    p.ErrorRatio = s.error_ratio;
                }
                p.CoinName = coin.name.clone();
                p.CoinSymbol = cur.into();
                p.Currency = cur.into();
                p.Protocol = super::protocol(ssl);
                p.Host = format!("{}.flockpool.com", region);
                p.Port = *port;
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x".into();
                p.Region = data::get_region(if region == "us" { "us-central" } else { region });
                p.SSL = ssl;
                p.PoolFee = 1.0;
                p.Workers = jsonx::num_opt(jsonx::get(&req, "active_workers"));
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.WTM = true;
                p.Wallet = wallet.clone();
                out.push(p);
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct HashVault;

const HASHVAULT: &[(&str, &str)] = &[
    ("AEON", "aeon"),
    ("CCX", "conceal"),
    ("LTHN", "lethean"),
    ("MSR", "masari"),
    ("SAL", "salvium"),
    ("SUMO", "sumokoin"),
    ("XHV", "haven"),
    ("XMR", "monero"),
    ("ZEPH", "zephyr"),
];

impl PoolModule for HashVault {
    fn name(&self) -> &'static str {
        "HashVault"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut out = Vec::new();
        for (cur, rpc) in HASHVAULT {
            if ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let mut fee = 0.9;
            let ports = [PoolPorts { CPU: 80, GPU: 3333, RIG: 3333 }, PoolPorts { CPU: 80, GPU: 3333, RIG: 3333 }];
            let mut ok = true;
            let (mut st, mut tsl, mut req) = (None, None, Value::Null);
            if !ctx.info_only {
                let r = web::Cached::new(&format!("https://api.hashvault.pro/v3/{}/stats", rpc)).tag(name).timeout(15).cycletime(120).json();
                let ts = util::unix_timestamp() as f64 * 1000.0;
                let b = web::Cached::new(&format!("https://api.hashvault.pro/v3/{}/pool/blocks?limit=100&page=0&pooltype=collective", rpc)).tag(name).timeout(15).cycletime(120).json();
                match (r, b) {
                    (Ok(r), Ok(b)) => {
                        req = r;
                        fee = jsonx::num(jsonx::path(&req, &["config", "pplns_fee"]));
                        let units = jsonx::num(jsonx::path(&req, &["config", "sigDivisor"]));
                        let diff = jsonx::num(jsonx::path(&req, &["network_statistics", "difficulty"]));
                        let reward = jsonx::num(jsonx::path(&req, &["network_statistics", "value"]));
                        let profit = if diff > 0.0 && units > 0.0 { 86400.0 / diff * reward / units } else { 0.0 };
                        let btc = rates::get(cur).map(|r| 1.0 / r).unwrap_or_else(|| jsonx::num(jsonx::path(&req, &["market", "price_btc"])));
                        let blocks = jsonx::as_list(&b);
                        let times: Vec<f64> = blocks.iter().map(|x| jsonx::num(jsonx::get(x, "ts"))).filter(|t| *t >= ts - 86400000.0).collect();
                        tsl = Some((ts - blocks.first().map(|x| jsonx::num(jsonx::get(x, "ts"))).unwrap_or(0.0)) / 1000.0);
                        let stat_name = format!("{}_{}_Profit", name, cur);
                        let day_data = !stat_file_exists(&stat_name);
                        let mut p = SetStat::new(&stat_name, profit * btc, if day_data { Duration::days(1) } else { ctx.stat_span });
                        p.hashrate = jsonx::num(jsonx::path(&req, &["pool_statistics", "collective", if day_data { "avg24hashRate" } else { "hashRate" }]));
                        p.blockrate = blk_rate(&times, 86400000.0);
                        p.quiet = true;
                        let s = stat::set_stat(p);
                        if s.hashrate_live == 0.0 && !ctx.allow_zero {
                            continue;
                        }
                        st = Some(s);
                    }
                    _ => {
                        log_warn!("Pool API ({}) for {} has failed.", name, cur);
                        ok = false;
                    }
                }
            }
            if !ok && !ctx.info_only {
                continue;
            }
            let (wallet, _, difficulty) = pool::wallet_with_payment_id(ctx.wallet(cur).unwrap_or(""), ".");
            for (i, pp) in ports.iter().enumerate() {
                let ssl = i > 0;
                let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = coin.name.clone();
                p.CoinSymbol = cur.to_string();
                p.Currency = cur.to_string();
                p.Protocol = super::protocol(ssl);
                p.Host = "pool.hashvault.pro".into();
                p.Port = pp.CPU;
                p.Ports = Some(pp.clone());
                p.User = format!("{}{}", wallet, if difficulty.is_empty() { "" } else { "{diff:+$difficulty}" });
                p.Pass = ctx.worker_tag();
                p.Region = data::get_region("eu");
                p.SSL = ssl;
                p.PoolFee = fee;
                p.Workers = jsonx::num_opt(jsonx::path(&req, &["pool_statistics", "collective", "miners"]));
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.TSL = tsl;
                p.BLK = st.as_ref().map(|s| s.blockrate_average);
                p.Wallet = wallet.clone();
                out.push(p);
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct Hashcryptos;

/// algo, stratum host, divisor of the mbtc_mh_factor
const HASHCRYPTOS: &[(&str, &str, f64)] = &[
    ("blake2s", "stratum3", 0.0),
    ("equihash", "stratum4", 0.0),
    ("equihash192", "stratum4", 0.0),
    ("ghostrider", "stratum4", 0.0),
    ("groestl", "stratum3", 0.0),
    ("kawpow", "stratum4", 0.0),
    ("keccak", "stratum3", 0.0),
    ("lbry", "stratum4", 0.0),
    ("lyra2rev2", "stratum3", 0.0),
    ("memehash", "stratum4", 0.0),
    ("myrgro", "stratum3", 0.0),
    ("neoscrypt", "stratum1", 0.0),
    ("odocrypt", "stratum2", 0.0),
    ("qubit", "stratum3", 0.0),
    ("randomx", "stratum1", 1e-3),
    ("scrypt", "stratum2", 0.0),
    ("sha256", "stratum1", 0.0),
    ("verthash", "stratum3", 0.0),
    ("x11", "stratum1", 0.0),
    ("x11gost", "stratum3", 0.0),
    ("x13", "stratum3", 0.0),
    ("yescrypt", "stratum4", 0.0),
    ("yescryptr16", "stratum4", 0.0),
    ("yescryptr32", "stratum4", 0.0),
    ("yespower", "stratum4", 0.0),
    ("yespowerr16", "stratum4", 0.0),
];

const HASHCRYPTOS_CURRENCIES: &[&str] = &["BCH", "BSV", "BTC", "DASH", "DGB", "DOGE", "FTC", "GRS", "LTC", "MONA", "PEPEW", "RVN", "VTC", "XEC", "XMR", "XMY", "XVG", "ZEC"];

impl PoolModule for Hashcryptos {
    fn name(&self) -> &'static str {
        "Hashcryptos"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = match web::Cached::new("https://api.rbminer.net/data/hashcryptos.json").tag(name).retry(3).cycletime(120).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        if jsonx::keys(&req).len() <= 1 {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let currencies: Vec<&str> = HASHCRYPTOS_CURRENCIES.iter().copied().filter(|c| ctx.wallet(c).is_some() || ctx.info_only).collect();
        let mut out = Vec::new();
        for (_, v) in jsonx::entries(&req) {
            if !(jsonx::num(jsonx::get(v, "estimate_current")) > 0.0 || ctx.allow_zero || ctx.info_only) {
                continue;
            }
            let algo = jsonx::s(jsonx::get(v, "name"));
            let Some((_, stratum, div)) = HASHCRYPTOS.iter().find(|(a, _, _)| *a == algo) else {
                log_info!("{}: no data avail for algorithm {}. ", name, algo);
                continue;
            };
            let port = jsonx::num(jsonx::get(v, "port")) as u32;
            let coin_symbol = jsonx::s(jsonx::get(v, "coin"));
            let algo_norm = data::get_algorithm_coin(&algo, &coin_symbol);
            let fee = jsonx::num(jsonx::get(v, "fees"));
            let mut factor = jsonx::num(jsonx::get(v, "mbtc_mh_factor"));
            if *div != 0.0 {
                factor /= div;
            }
            let dag = if coin_symbol.is_empty() { None } else { Some(data::get_eth_dag_size(&coin_symbol, "", 1.0)) };
            if factor <= 0.0 {
                log_info!("{}: Unable to determine divisor for algorithm {}. ", name, algo);
                continue;
            }
            let mut st = None;
            if !ctx.info_only {
                let stat_name = format!("{}_{}_Profit", name, algo_norm);
                let old = stat_file_exists(&stat_name);
                let dw = if old { ctx.data_window.clone() } else { "actual_last24h".to_string() };
                let a24 = jsonx::num(jsonx::get(v, "actual_last24h"));
                let e24 = jsonx::num(jsonx::get(v, "estimate_last24h"));
                let price = if a24 > 0.0 && e24 > 0.0 { pool::yiimp_value_div(v, factor, &dw, 1.0) } else { 0.0 };
                let mut p = SetStat::new(&stat_name, price, if old { ctx.stat_span } else { Duration::days(1) });
                p.change_detection = old;
                p.actual24h = a24;
                p.estimate24h = e24;
                p.hashrate = jsonx::num(jsonx::get(v, "hashrate"));
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            for cur in &currencies {
                let mut p = ctx.pool(&algo_norm).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.Currency = cur.to_string();
                p.Protocol = "stratum+tcp".into();
                p.Host = format!("{}.hashcryptos.com", stratum);
                p.Port = port;
                p.User = format!("{}.{}", ctx.wallet(cur).unwrap_or(""), ctx.worker_tag());
                p.Pass = format!("n={}{{diff:,d=$difficulty}}", ctx.worker_tag());
                p.Region = data::get_region("us");
                p.PoolFee = fee;
                p.Workers = Some(jsonx::num(jsonx::get(v, "workers")).round());
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.CoinSymbolMax = coin_symbol.clone();
                p.DagSizeMax = dag;
                p.Wallet = ctx.wallet(cur).unwrap_or("").to_string();
                out.push(p);
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct Hashpool;

/// symbol, port, fee, rpc, algorithm
const HASHPOOL: &[(&str, u32, f64, &str, &str)] = &[
    ("BSHA3", 21879, 2.0, "bsha3", ""),
    ("CKB", 4300, 1.0, "ckb", ""),
    ("DGBODO", 11116, 1.0, "dgbodo", "Odocrypt"),
    ("HNS", 6000, 1.0, "hns", ""),
    ("KDA", 3700, 1.0, "kda", ""),
    ("NXS", 9012, 1.0, "nxs", ""),
    ("PMEER", 9966, 1.0, "meer", ""),
];

impl PoolModule for Hashpool {
    fn name(&self) -> &'static str {
        "Hashpool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let coins = web::Cached::new("https://hashpool.com/api/coins").tag(name).cycletime(120).json().unwrap_or(Value::Null);
        if jsonx::num(jsonx::get(&coins, "code")) != 0.0 || jsonx::get(&coins, "code").is_null() || jsonx::count(jsonx::get(&coins, "data")) == 0 {
            log_warn!("Pool API ({}) has failed. ", name);
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        for (sym, port, fee0, rpc, algo) in HASHPOOL {
            let cur = sym.replace("DGBODO", "DGB");
            if ctx.wallet(&cur).is_none() && !ctx.info_only {
                continue;
            }
            let Some(d) = jsonx::as_list(jsonx::get(&coins, "data")).into_iter().find(|c| jsonx::s(jsonx::get(c, "coin")) == *sym) else {
                continue;
            };
            let Some(coin) = data::get_coin(&cur, algo) else {
                log_warn!("Coin {} not found", cur);
                continue;
            };
            let fee = jsonx::num_opt(jsonx::get(d, "fee")).unwrap_or(*fee0);
            let (mut st, mut tsl) = (None, None);
            if !ctx.info_only {
                let mut blk = 0.0;
                if let Ok(b) = web::Cached::new(&format!("https://hashpool.com/api/blocks/{}?offset=0&limit=50", cur)).tag(name).cycletime(120).json() {
                    if !jsonx::get(&b, "code").is_null() && jsonx::num(jsonx::get(&b, "code")) == 0.0 {
                        let ts = util::unix_timestamp() as f64;
                        let mut blocks: Vec<f64> = jsonx::pluck(jsonx::path(&b, &["data", "data"]), "dateTime").into_iter().map(jsonx::num).collect();
                        blocks.sort_by(|x, y| y.partial_cmp(x).unwrap_or(std::cmp::Ordering::Equal));
                        if !blocks.is_empty() {
                            let recent: Vec<f64> = blocks.iter().cloned().filter(|t| *t > (ts - 86400.0) * 1000.0).collect();
                            tsl = Some(ts - blocks[0] / 1000.0);
                            blk = blk_rate(&recent, 86400000.0);
                        }
                    }
                }
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.hashrate = util::convert_from_hash(&format!("{}{}", jsonx::s(jsonx::get(d, "poolHashrate")), jsonx::s(jsonx::get(d, "poolHashrateUnit"))));
                p.blockrate = blk;
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let wallet = ctx.wallet(&cur).unwrap_or("").to_string();
            let mut p = ctx.pool(&coin.algo);
            p.CoinName = coin.name.clone();
            p.CoinSymbol = cur.clone();
            p.Currency = cur.clone();
            p.Protocol = "stratum+tcp".into();
            p.Host = format!("{}.stratum.hashpool.site", rpc);
            p.Port = *port;
            p.User = format!("{}.{}", wallet, ctx.worker_tag());
            p.Pass = "x{diff:,d=$difficulty}".into();
            p.Region = data::get_region("Asia");
            p.PoolFee = fee;
            p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
            p.BLK = st.as_ref().map(|s| s.blockrate_average);
            p.TSL = tsl;
            p.ErrorRatio = st.as_ref().map(|s| s.error_ratio).unwrap_or(p.ErrorRatio);
            p.WTM = true;
            // the original leaves Wallet empty ($Pool_User is undefined there)
            p.Wallet = wallet;
            out.push(p);
        }
        Ok(out)
    }
}

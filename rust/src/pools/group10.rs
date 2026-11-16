//! Pools/MinerRocks.ps1, Mintpond.ps1, Molepool.ps1, Neuropool.ps1, Pearlhash.ps1

use super::group8::blk_rate;
use crate::data;
use crate::jsonx;
use crate::log_info;
use crate::log_warn;
use crate::pool::{self, Pool, PoolCtx, PoolDataOpts, PoolModule, PoolPorts};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use anyhow::Result;
use chrono::Duration;
use serde_json::Value;

/// symbol, default port, fee, rpc, regions, solo
const MINERROCKS: &[(&str, u32, f64, &str, &[&str], bool)] = &[
    ("CCX", 30041, 0.9, "conceal", &["fr"], false),
    ("DERO", 30182, 0.9, "dero", &["fr", "ca", "sg"], true),
    ("XHV", 30031, 0.9, "haven", &["fr", "ca", "us-w", "br", "sg", "za"], false),
    ("RYO", 30172, 1.2, "ryo", &["fr", "ca", "us-w", "br", "sg", "za"], false),
    ("UPX", 30022, 0.9, "uplexa", &["fr"], false),
];

pub struct MinerRocks;

impl PoolModule for MinerRocks {
    fn name(&self) -> &'static str {
        "MinerRocks"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut out = Vec::new();
        for (cur, port0, fee0, rpc, regions, solo) in MINERROCKS {
            if ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let mut fee = *fee0;
            let mut divisor = 1.0;
            let mut ports: Vec<Option<PoolPorts>> = vec![None];
            let (mut st, mut pd) = (None, pool::PoolData::default());
            if !ctx.info_only {
                let req = match web::Cached::new(&format!("https://{}.miner.rocks/api/stats", rpc)).tag(name).cycletime(120).json() {
                    Ok(r) => r,
                    Err(_) => {
                        log_warn!("Pool API ({}) for {} has failed. ", name, cur);
                        continue;
                    }
                };
                let hm = jsonx::num(jsonx::path(&req, &["config", "hashrateMultiplier"]));
                if hm != 0.0 {
                    divisor = hm;
                }
                ports = pool::pool_ports_from_request(&req, "low", "modern", "farm", "PPS").to_vec();
                if ports.iter().all(|p| p.is_none()) {
                    continue;
                }
                fee = jsonx::num(jsonx::path(&req, &["config", "fee"]));
                let ts = util::unix_timestamp() as f64;
                let stat_name = format!("{}_{}_Profit", name, cur);
                let day = stat::get_stat(&stat_name, "").is_none();
                pd = pool::pool_data_from_request(
                    &req,
                    &PoolDataOpts { currency: cur.to_string(), divisor, timestamp: ts, add_day: day, add_block_data: true, chart_currency: "USD".into(), ..Default::default() },
                );
                let mut p = SetStat::new(&stat_name, pd.reward(day) / 1e8, if day { Duration::days(1) } else { ctx.stat_span });
                p.quiet = true;
                if *solo {
                    p.difficulty = jsonx::num(jsonx::path(&req, &["network", "difficulty"]));
                    st = Some(stat::set_stat(p));
                } else {
                    p.hashrate = pd.hashrate(day);
                    p.blockrate = pd.blk;
                    p.change_detection = day;
                    let s = stat::set_stat(p);
                    if s.hashrate_live == 0.0 && !ctx.allow_zero {
                        continue;
                    }
                    st = Some(s);
                }
            }
            let (wallet, _, difficulty) = pool::wallet_with_payment_id(ctx.wallet(cur).unwrap_or(""), ".");
            let mut ssl = false;
            for pp in &ports {
                if !ctx.info_only && pp.is_none() {
                    ssl = true;
                    continue;
                }
                for region in regions.iter() {
                    let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = cur.to_string();
                    p.Currency = cur.to_string();
                    p.Protocol = super::protocol(ssl);
                    p.Host = format!("{}.{}.miner.rocks", region, rpc);
                    p.Port = pp.as_ref().map(|x| x.CPU).unwrap_or(*port0);
                    p.Ports = pp.clone();
                    p.User = format!("{}{}", wallet, if difficulty.is_empty() { "{diff:.$difficulty}".to_string() } else { format!(".{}", difficulty) });
                    p.Pass = format!("w={}", ctx.worker_tag());
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.PoolFee = fee;
                    if !*solo {
                        p.Workers = pd.workers;
                        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                        p.TSL = pd.tsl;
                        p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    } else {
                        // the original reads the non-existing $Stat.Difficulty, the average difficulty is what it means
                        p.Difficulty = st.as_ref().map(|s| s.diff_average);
                        p.SoloMining = true;
                    }
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
                ssl = true;
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct Mintpond;

impl PoolModule for Mintpond {
    fn name(&self) -> &'static str {
        "Mintpond"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut out = Vec::new();
        for (cur, url, port, alt) in [("RVN", "ravencoin", 3010u32, ""), ("FIRO", "firo", 3000, "XZC")] {
            let wallet = ctx.wallet(cur).or(if alt.is_empty() { None } else { ctx.wallet(alt) }).unwrap_or("").to_string();
            if wallet.is_empty() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let eth = super::eth_mode_default(&coin.algo, "ethproxy");
            let base = format!("https://api.mintpond.com/v1/{}", url);
            let (mut st, mut tsl, mut req) = (None, None, Value::Null);
            if !ctx.info_only {
                let res: Result<(Value, Value, Vec<Value>)> = (|| {
                    let r = web::Cached::new(&format!("{}/pool/status", base)).tag(name).retry(5).cycletime(120).json()?;
                    if !jsonx::truthy(jsonx::get(&r, "pool")) {
                        anyhow::bail!("no pool");
                    }
                    if !(jsonx::truthy(jsonx::path(&r, &["pool", "hashrate"])) || ctx.allow_zero) {
                        return Ok((r, Value::Null, vec![]));
                    }
                    let bs = web::Cached::new(&format!("{}/pool/blockstats", base)).tag(name).retry(5).cycletime(120).delay(250).json()?;
                    if !jsonx::truthy(jsonx::path(&bs, &["pool", "blockStats"])) {
                        anyhow::bail!("no blockstats");
                    }
                    let rb = web::Cached::new(&format!("{}/pool/recentblocks", base)).tag(name).retry(5).cycletime(120).delay(250).json()?;
                    if !jsonx::truthy(jsonx::path(&rb, &["pool", "recentBlocks"])) {
                        anyhow::bail!("no recent blocks");
                    }
                    let mut last: Vec<Value> = jsonx::as_list(jsonx::path(&rb, &["pool", "recentBlocks"])).into_iter().cloned().collect();
                    last.sort_by(|a, b| jsonx::num(jsonx::get(a, "height")).partial_cmp(&jsonx::num(jsonx::get(b, "height"))).unwrap_or(std::cmp::Ordering::Equal));
                    let n = last.len();
                    Ok((r, bs, last.split_off(n.saturating_sub(3))))
                })();
                let (r, bs, last) = match res {
                    Ok((r, bs, _)) if bs.is_null() => {
                        let _ = r;
                        continue;
                    }
                    Ok(x) => x,
                    Err(_) => {
                        log_warn!("Pool API ({}) for {} has failed. ", name, cur);
                        continue;
                    }
                };
                req = r;
                let pl = jsonx::get(&req, "pool");
                let lt = last.last().map(|b| jsonx::num(jsonx::get(b, "time"))).unwrap_or(0.0);
                tsl = Some(util::unix_timestamp() as f64 - if lt != 0.0 { lt } else { jsonx::num(jsonx::get(pl, "lastBlockTime")) * 1000.0 });
                let reward = if last.is_empty() { 6.25 } else { last.iter().map(|b| jsonx::num(jsonx::get(b, "reward"))).sum::<f64>() / last.len() as f64 };
                let hr = jsonx::num(jsonx::get(pl, "hashrate"));
                let est = jsonx::num(jsonx::get(pl, "estTime"));
                let live = if hr > 0.0 && est != 0.0 { rates::btc_price(cur) * reward * 86400.0 / est / hr } else { 0.0 };
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), live, ctx.stat_span);
                p.change_detection = true;
                p.hashrate = hr;
                p.blockrate = jsonx::num(jsonx::path(&bs, &["pool", "blockStats", "valid24h"]));
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            for region in ["eu", "us"] {
                let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = coin.name.clone();
                p.CoinSymbol = cur.into();
                p.Currency = cur.into();
                p.Protocol = "stratum+tcp".into();
                p.Host = format!("{}-{}.mintpond.com", url, region);
                p.Port = port;
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x{diff:,d=$difficulty}".into();
                p.Region = data::get_region(region);
                p.PoolFee = 0.9;
                p.Workers = jsonx::num_opt(jsonx::path(&req, &["pool", "workerCount"]));
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.BLK = st.as_ref().map(|s| s.blockrate_average);
                p.TSL = tsl;
                p.EthMode = eth.clone();
                p.Wallet = wallet.clone();
                out.push(p);
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct Molepool;

impl PoolModule for Molepool {
    fn name(&self) -> &'static str {
        "Molepool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut out = Vec::new();
        for (cur, port, rpc) in [("ETHW", 3331u32, "ethw-pplns")] {
            if ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let eth = super::eth_mode_default(&coin.algo, "ethproxy");
            let (mut st, mut tsl, mut workers) = (None, None, None);
            if !ctx.info_only {
                let req = match web::Cached::new(&format!("https://{}.molepool.com/api/stats", rpc)).tag(name).cycletime(120).json() {
                    Ok(r) => r,
                    Err(_) => {
                        log_warn!("Pool API ({}) for {} has failed. ", name, cur);
                        continue;
                    }
                };
                let blocks = web::Cached::new(&format!("https://{}.molepool.com/api/blocks", rpc)).tag(name).cycletime(120).json().unwrap_or_else(|_| {
                    log_info!("Pool blocks API ({}) for {} has failed. ", name, cur);
                    Value::Null
                });
                let ts = util::unix_timestamp() as f64;
                let times: Vec<f64> = ["candidates", "immature", "matured"]
                    .iter()
                    .flat_map(|k| jsonx::as_list(jsonx::get(&blocks, k)).into_iter().map(|b| jsonx::num(jsonx::get(b, "timestamp"))).collect::<Vec<_>>())
                    .filter(|t| *t > ts - 86400.0)
                    .collect();
                workers = Some(jsonx::num(jsonx::path(&req, &["stats", "workerTotal"])).round());
                tsl = Some((ts - times.iter().cloned().fold(0.0, f64::max)).round());
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.hashrate = jsonx::num(jsonx::get(&req, "hashrate"));
                p.blockrate = blk_rate(&times, 86400.0);
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let wallet = ctx.wallet(cur).unwrap_or("").to_string();
            for ssl in [false, true] {
                let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.Price = 0.0;
                p.StablePrice = 0.0;
                p.CoinName = coin.name.clone();
                p.CoinSymbol = cur.into();
                p.Currency = cur.into();
                p.Protocol = super::protocol(ssl);
                p.Host = "eu.molepool.com".into();
                p.Port = if ssl { port + 10000 } else { port };
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x".into();
                p.Region = data::get_region("eu");
                p.SSL = ssl;
                p.PoolFee = 1.0;
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
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct Neuropool;

const NEUROPOOL_REGIONS: &[&str] = &["eu", "us", "in", "au", "as", "as2", "ca", "me"];

impl PoolModule for Neuropool {
    fn name(&self) -> &'static str {
        "Neuropool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut out = Vec::new();
        for (sym, port0, fee0, rpc, hostp, mallob) in [("DNX", 19330u32, 1.0, "dynex", "dnx", true)] {
            let cur = sym.split('-').next().unwrap_or(sym).to_string();
            if ctx.wallet(&cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(sym, "").unwrap_or_default();
            let eth = super::eth_mode_default(&coin.algo, "ethproxy");
            let mut fee = fee0;
            let mut ports: Vec<Option<PoolPorts>> = vec![None];
            let (mut st, mut tsl, mut workers, mut solo, mut req) = (None, None, None, false, Value::Null);
            if !ctx.info_only {
                req = match web::Cached::new(&format!("https://{}.neuropool.net:8119/stats", rpc)).tag(name).timeout(15).cycletime(120).json() {
                    Ok(r) => r,
                    Err(_) => {
                        log_warn!("Pool API ({}) for {} has failed. ", name, cur);
                        continue;
                    }
                };
                ports = pool::pool_ports_from_request(&req, "", "8khs", "25khs", "").to_vec();
                if ports.iter().all(|p| p.is_none()) {
                    continue;
                }
                fee = jsonx::num(jsonx::path(&req, &["config", "fee"]));
                if ctx.wallet(&cur).unwrap_or("").starts_with("solo:") {
                    solo = true;
                    let diff = jsonx::num(jsonx::path(&req, &["network", "difficulty"])) / 4294967296.0;
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, sym), 0.0, ctx.stat_span);
                    p.difficulty = diff;
                    p.quiet = true;
                    st = Some(stat::set_stat(p));
                } else {
                    let ts = util::unix_timestamp() as f64;
                    let re = regex::Regex::new(r"^[0-9a-z\.]+:.*?(\d{10}):").unwrap();
                    let times: Vec<f64> = jsonx::as_list(jsonx::path(&req, &["pool", "blocks"]))
                        .into_iter()
                        .filter_map(|b| re.captures(&jsonx::s(b)).and_then(|c| c[1].parse::<f64>().ok()))
                        .filter(|t| *t > ts - 86400.0)
                        .collect();
                    workers = Some(jsonx::num(jsonx::path(&req, &["pool", "workers"])).round());
                    tsl = Some((ts - jsonx::num(jsonx::path(&req, &["pool", "stats", "lastBlockFoundprop"])) / 1000.0).round());
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, sym), 0.0, ctx.stat_span);
                    p.hashrate = jsonx::num(jsonx::path(&req, &["pool", "hashrate"]));
                    p.blockrate = blk_rate(&times, 86400.0);
                    p.quiet = true;
                    let s = stat::set_stat(p);
                    if s.hashrate_live == 0.0 && !ctx.allow_zero {
                        continue;
                    }
                    st = Some(s);
                }
            }
            let (wallet, _, difficulty) = pool::wallet_with_payment_id(ctx.wallet(&cur).unwrap_or(""), "+");
            let fixed = jsonx::truthy(jsonx::path(&req, &["config", "fixedDiffEnabled"]));
            let sep = jsonx::s(jsonx::path(&req, &["config", "fixedDiffSeparator"]));
            let user = format!(
                "{}{}",
                wallet,
                if fixed {
                    if difficulty.is_empty() {
                        format!("{{diff:{}$difficulty}}", sep)
                    } else {
                        format!("{}{}", sep, difficulty)
                    }
                } else {
                    String::new()
                }
            );
            let mut ssl = false;
            for pp in &ports {
                if !ctx.info_only && pp.is_none() {
                    ssl = true;
                    continue;
                }
                for region in NEUROPOOL_REGIONS {
                    let mut p = ctx.pool(&coin.algo);
                    if let Some(s) = &st {
                        p.Updated = s.updated;
                        p.ErrorRatio = s.error_ratio;
                    }
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = cur.clone();
                    p.Currency = cur.clone();
                    p.Protocol = super::protocol(ssl);
                    p.Host = format!("{}.{}.neuropool.net", hostp, region);
                    p.Port = pp.as_ref().map(|x| x.CPU).unwrap_or(port0);
                    p.Ports = pp.clone();
                    p.User = user.clone();
                    p.Pass = ctx.worker_tag();
                    p.Region = data::get_region(if *region == "as2" { "hk" } else { region });
                    p.SSL = ssl;
                    p.PoolFee = fee;
                    if solo {
                        p.Difficulty = st.as_ref().map(|s| s.diff_average);
                        p.SoloMining = true;
                    } else {
                        p.Workers = workers;
                        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                        p.BLK = st.as_ref().map(|s| s.blockrate_average);
                        p.TSL = tsl;
                    }
                    p.WTM = true;
                    p.EthMode = eth.clone();
                    if mallob {
                        p.Mallob = format!(" http://mallob-ml.{}.neuropool.net/", region);
                    }
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
                ssl = true;
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct Pearlhash;

impl PoolModule for Pearlhash {
    fn name(&self) -> &'static str {
        "Pearlhash"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let cur = "PRL";
        let wallet = ctx.wallet(cur).unwrap_or("").to_string();
        let coin = data::get_coin(cur, "").unwrap_or_default();
        let (mut st, mut tsl, mut workers, mut live) = (None, None, None, None);
        if !ctx.info_only {
            let (req, net) = match (
                web::Cached::new("https://pearlhash.xyz/api/stats").tag(name).cycletime(120).json(),
                web::Cached::new("https://pearlhash.xyz/api/chain-info").tag(name).cycletime(120).json(),
            ) {
                (Ok(a), Ok(b)) => (a, b),
                _ => {
                    log_warn!("Pool API ({}) has failed. ", name);
                    return Ok(vec![]);
                }
            };
            let txs = web::Cached::new("https://pearlhash.xyz/api/pool-wallet-txs?page=1").tag(name).cycletime(120).json().unwrap_or_else(|_| {
                log_warn!("Pool blocks API ({}) has failed. ", name);
                Value::Null
            });
            let ts = util::unix_timestamp() as f64;
            let cb: Vec<&Value> = jsonx::as_list(jsonx::get(&txs, "transactions")).into_iter().filter(|t| jsonx::s(jsonx::get(t, "type")) == "coinbase").collect();
            let reward = cb.first().map(|t| jsonx::num(jsonx::get(t, "receivedGrains")) / 1e8).unwrap_or(0.0);
            let mut blocks: Vec<f64> = cb.iter().map(|t| (jsonx::num(jsonx::get(t, "timeMs")) / 1000.0).round()).collect();
            blocks.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
            let recent: Vec<f64> = blocks.iter().cloned().filter(|t| *t > ts - 86400.0).collect();
            tsl = Some((ts - blocks.first().copied().unwrap_or(0.0)).round());
            let nh = jsonx::num(jsonx::get(&net, "networkhashps"));
            let bt = jsonx::num(jsonx::get(&net, "avg_block_time_s"));
            let l = if nh != 0.0 && bt != 0.0 { 86400.0 * rates::btc_price(cur) * reward / nh / bt } else { 0.0 };
            live = Some(l);
            workers = jsonx::num_opt(jsonx::get(&req, "total_workers"));
            let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), l, ctx.stat_span);
            p.hashrate = jsonx::num(jsonx::get(&req, "hashrate"));
            p.blockrate = blk_rate(&recent, 86400.0);
            p.difficulty = jsonx::num(jsonx::get(&net, "difficulty"));
            p.quiet = true;
            let s = stat::set_stat(p);
            if s.hashrate_live == 0.0 && !ctx.allow_zero {
                return Ok(vec![]);
            }
            st = Some(s);
        }
        let mut out = Vec::new();
        if wallet.is_empty() && !ctx.info_only {
            return Ok(out);
        }
        for (region, host) in [("eu", "84.32.220.219"), ("asia", "129.226.55.135")] {
            let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
            p.CoinName = coin.name.clone();
            p.CoinSymbol = cur.into();
            p.Currency = cur.into();
            p.Protocol = "stratum+tcp".into();
            p.Host = host.into();
            p.Port = 9000;
            p.User = wallet.clone();
            p.Pass = "x".into();
            p.Region = data::get_region(region);
            p.PoolFee = 1.0;
            p.Workers = workers;
            p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
            p.BLK = st.as_ref().map(|s| s.blockrate_average);
            p.TSL = tsl;
            // in InfoOnly mode $btcRewardLive is $null, and $null -eq 0 is false
            p.WTM = live == Some(0.0);
            p.EthMode = "stratum".into();
            p.Wallet = wallet.clone();
            out.push(p);
        }
        Ok(out)
    }
}

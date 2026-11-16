//! Pools/Poolin.ps1, RaptoreumZone.ps1, SoloPool.ps1, Sunpool.ps1, SupportXmr.ps1

use super::group8::blk_rate;
use crate::data;
use crate::jsonx;
use crate::log_info;
use crate::log_warn;
use crate::pool::{Pool, PoolCtx, PoolModule};
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use anyhow::Result;
use serde_json::Value;

pub struct Poolin;

impl PoolModule for Poolin {
    fn name(&self) -> &'static str {
        "Poolin"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut out = Vec::new();
        for (cur, host, fee, divisor) in [("ETC", "etc.ss.poolin.one", 1.0, 1.0), ("ETHW", "ethw.ss.poolin.one", 1.0, 1.0)] {
            if ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let rpc = cur.to_lowercase();
            let eth = super::eth_mode_default(&coin.algo, "ethproxy");
            let (mut st, mut workers) = (None, None);
            if !ctx.info_only {
                let mut errno = "NF".to_string();
                let ok_resp = |v: &Value, errno: &mut String| -> bool {
                    let e = jsonx::s(jsonx::get(v, "err_no"));
                    if e != "0" || !jsonx::truthy(jsonx::get(v, "data")) {
                        if jsonx::truthy(jsonx::get(v, "err_no")) {
                            *errno = e;
                        }
                        false
                    } else {
                        true
                    }
                };
                let mut res: Option<(Value, Value)> = None;
                if let Ok(base) = web::Cached::new(&format!("https://api-prod.poolin.me/api/public/v2/basedata/coin/{}", rpc)).tag(name).cycletime(120).json() {
                    if ok_resp(&base, &mut errno) {
                        if let Ok(c) = web::Cached::new(&format!("https://api-prod.poolin.me/api/public/v1/pool/stats/merge?coin_type={}&show_pool_power=1", rpc)).tag(name).cycletime(120).json() {
                            if ok_resp(&c, &mut errno) {
                                res = Some((base, c));
                            }
                        }
                    }
                }
                let Some((base, c)) = res else {
                    log_warn!("Pool API ({}) has failed. (errno={})", name, errno);
                    continue;
                };
                let shares = jsonx::path(&c, &["data", "shares"]);
                let hr = util::convert_from_hash(&format!("{}{}", jsonx::s(jsonx::get(shares, "shares_5m")), jsonx::s(jsonx::get(shares, "shares_unit"))));
                let diff = jsonx::num(jsonx::path(&base, &["data", "difficulty"]));
                workers = jsonx::num_opt(jsonx::path(&c, &["data", "workers"]));
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.hashrate = hr;
                p.blockrate = if hr > 0.0 && diff > 0.0 { 86400.0 / diff * hr / divisor } else { 0.0 };
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let w = ctx.wallet(cur).unwrap_or("").to_string();
            let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
            p.Price = 0.0;
            p.StablePrice = 0.0;
            p.CoinName = coin.name.clone();
            p.CoinSymbol = cur.into();
            p.Currency = cur.into();
            p.Protocol = "stratum+tcp".into();
            p.Host = host.into();
            p.Port = 443;
            p.User = format!("{}{}", w, if w.contains('.') { "" } else { ".001" });
            p.Pass = "123".into();
            p.Region = data::get_region("ca");
            p.PoolFee = fee;
            p.Workers = workers;
            p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
            p.BLK = st.as_ref().map(|s| s.blockrate_average);
            p.WTM = true;
            p.EthMode = eth;
            p.Wallet = regex::Regex::new(r"\.[^.]+$").unwrap().replace(&w, "").into_owned();
            out.push(p);
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct RaptoreumZone;

impl PoolModule for RaptoreumZone {
    fn name(&self) -> &'static str {
        "RaptoreumZone"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = web::Cached::new("https://api.raptoreum.zone/v1/rzone/").tag(name).cycletime(120).retry(5).json().unwrap_or(Value::Null);
        let primary = jsonx::path(&req, &["result", "primary"]);
        if jsonx::truthy(jsonx::get(&req, "error")) || !jsonx::truthy(primary) {
            log_warn!("Pool API ({}) has failed. ", name);
            return Ok(vec![]);
        }
        let cur = "RTM";
        let coin = data::get_coin(cur, "").unwrap_or_default();
        let cfg = jsonx::get(primary, "config");
        let fee = if jsonx::truthy(cfg) && !jsonx::get(cfg, "recipientFee").is_null() { 100.0 * jsonx::num(jsonx::get(cfg, "recipientFee")) } else { 0.75 };
        let blocks = jsonx::get(primary, "blocks");
        let tsl = if jsonx::truthy(blocks) && !jsonx::get(blocks, "lastFound").is_null() {
            Some(((util::unix_timestamp() as f64 * 1000.0 - jsonx::num(jsonx::get(blocks, "lastFound"))) / 1000.0).round())
        } else {
            None
        };
        let blk = if jsonx::truthy(blocks) && !jsonx::get(blocks, "lastDayCount").is_null() { Some(jsonx::num(jsonx::get(blocks, "lastDayCount")).round()) } else { None };
        let wallet = ctx.wallet(cur).unwrap_or("").to_string();
        let mut st = None;
        if !ctx.info_only {
            let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
            p.hashrate = jsonx::num(jsonx::path(primary, &["hashrate", "shared"]));
            p.blockrate = blk.unwrap_or(0.0);
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
        for (i, port) in [3333u32, 4444].iter().enumerate() {
            let ssl = i > 0;
            for region in ["europe", "usa-east", "usa-west", "asia"] {
                let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.Price = 0.0;
                p.StablePrice = 0.0;
                p.CoinName = coin.name.clone();
                p.CoinSymbol = cur.into();
                p.Currency = cur.into();
                p.Protocol = super::protocol(ssl);
                p.Host = format!("{}.raptoreum.zone", region);
                p.Port = *port;
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x".into();
                p.Region = data::get_region(region);
                p.SSL = ssl;
                p.PoolFee = fee;
                p.Workers = jsonx::num_opt(jsonx::path(primary, &["status", "miners"]));
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.TSL = tsl;
                p.BLK = st.as_ref().map(|s| s.blockrate_average);
                p.WTM = true;
                p.Wallet = wallet.clone();
                out.push(p);
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct SoloPool;

impl PoolModule for SoloPool {
    fn name(&self) -> &'static str {
        "SoloPool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = match web::Cached::new("https://stats.solopool.org").tag(name).cycletime(120).json() {
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
        let re_meta = regex::Regex::new(r#"meta\s+name="\w+/config/environment"\s+content="(.+?)""#).unwrap();
        let mut out = Vec::new();
        for (key, v) in jsonx::entries(&req) {
            let cur = key.to_uppercase();
            if key.starts_with("dgb-") || !(ctx.wallet(&cur).is_some() || (cur == "FIRO" && ctx.wallet("XZC").is_some()) || ctx.info_only) {
                continue;
            }
            let coin = data::get_coin(&cur, "");
            let (mut algo, mut coin_name) = coin.as_ref().map(|c| (c.algo.clone(), c.name.clone())).unwrap_or_default();
            let wallet = ctx.wallet(&cur).or(ctx.wallet("XZC")).unwrap_or("").to_string();
            let mut meta = Value::Null;
            let mut ok = false;
            match web::Cached::new(&format!("https://{}.solopool.org/help", key)).tag(name).cycletime(86400).text() {
                Ok(html) => {
                    if let Some(c) = re_meta.captures(&html) {
                        if let Some(m) = jsonx::parse(&web::url_decode(&c[1])) {
                            meta = m;
                            ok = true;
                            if coin.is_none() {
                                algo = data::get_algorithm_coin(&jsonx::s(jsonx::path(&meta, &["TEMPLATE", "algoTitle"])), &cur);
                                coin_name = jsonx::s(jsonx::path(&meta, &["TEMPLATE", "title"]));
                            }
                        }
                    }
                }
                Err(_) => log_warn!("{}: {} help page not readable", name, cur),
            }
            if algo.is_empty() {
                log_warn!("Pool {} missing coin {}", name, cur);
                continue;
            }
            let eth = super::eth_mode_default(&algo, "ethproxy");
            let tpl = jsonx::get(&meta, "TEMPLATE");
            let port_key = if jsonx::truthy(jsonx::get(tpl, "stratumPortGpu")) { "stratumPortGpu" } else { "stratumPortLow" };
            let hosts: Vec<(&str, String, u32)> = ["eu", "us"]
                .into_iter()
                .filter_map(|r| {
                    let h = jsonx::s(jsonx::get(tpl, &format!("stratumHost{}", r.to_uppercase())));
                    if h.is_empty() {
                        None
                    } else {
                        Some((r, h, jsonx::num(jsonx::get(tpl, port_key)) as u32))
                    }
                })
                .collect();
            let mut st = None;
            if ok && !ctx.info_only {
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.difficulty = jsonx::num(jsonx::get(v, "difficulty"));
                p.quiet = true;
                st = Some(stat::set_stat(p));
            }
            if !ok && !ctx.info_only {
                continue;
            }
            for (region, host, port) in hosts {
                let mut p = ctx.pool(&algo);
                if let Some(s) = &st {
                    p.Updated = s.updated;
                    p.ErrorRatio = s.error_ratio;
                }
                p.CoinName = coin_name.clone();
                p.CoinSymbol = cur.clone();
                p.Currency = cur.clone();
                p.Protocol = "stratum+tcp".into();
                p.Host = host;
                p.Port = port;
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x".into();
                p.Region = data::get_region(region);
                p.PoolFee = jsonx::num(jsonx::get(v, "fee"));
                p.Difficulty = st.as_ref().map(|s| s.diff_average);
                p.SoloMining = true;
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

pub struct Sunpool;

impl PoolModule for Sunpool {
    fn name(&self) -> &'static str {
        "Sunpool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let email = ctx.field("Email");
        let mut out = Vec::new();
        for (cur, port, fee0, rpc) in [("BEAM", 3334u32, 1.0, "beam"), ("GRIMM", 3334, 1.0, "grimm")] {
            if ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let mut fee = fee0;
            let (mut st, mut tsl, mut workers) = (None, None, None);
            if !ctx.info_only {
                let req = match web::Cached::new(&format!("https://{}.sunpool.top/pool-info.php?miningpoolstats", rpc)).tag(name).timeout(15).cycletime(120).json() {
                    Ok(r) if jsonx::truthy(jsonx::get(&r, "coin")) => r,
                    _ => {
                        log_warn!("Pool API ({}) for {} has failed. ", name, cur);
                        continue;
                    }
                };
                fee = jsonx::num(jsonx::get(&req, "feePercent"));
                let newest = jsonx::pluck(jsonx::get(&req, "lastBlocksFound"), "timestampMilliseconds").into_iter().map(jsonx::num).fold(0.0, f64::max);
                tsl = Some(((jsonx::num(jsonx::get(&req, "timestampMilliSeconds")) - newest) / 1000.0).round());
                workers = jsonx::num_opt(jsonx::path(&req, &["pools", rpc, "workerCount"]));
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.hashrate = jsonx::num(jsonx::get(&req, "hashrate"));
                p.blockrate = jsonx::num(jsonx::get(&req, "blocksFound24H"));
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let wallet = ctx.wallet(cur).unwrap_or("").to_string();
            let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
            p.Price = 0.0;
            p.StablePrice = 0.0;
            p.CoinName = coin.name.clone();
            p.CoinSymbol = cur.into();
            p.Currency = cur.into();
            p.Protocol = "stratum+ssl".into();
            p.Host = format!("{}.sunpool.top", rpc);
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
            p.Wallet = wallet;
            out.push(p);
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct SupportXmr;

impl PoolModule for SupportXmr {
    fn name(&self) -> &'static str {
        "SupportXmr"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let cur = "XMR";
        if ctx.wallet(cur).is_none() && !ctx.info_only {
            return Ok(vec![]);
        }
        let req = web::Cached::new("https://supportxmr.com/api/pool/stats").tag(name).cycletime(120).json().unwrap_or(Value::Null);
        if !jsonx::truthy(jsonx::get(&req, "pool_statistics")) {
            log_warn!("Pool API ({}) has failed. ", name);
            return Ok(vec![]);
        }
        let ts = util::unix_timestamp() as f64;
        // the original filters with an undefined $Timestamp24, so all returned blocks count
        let times: Vec<f64> = match web::Cached::new("https://supportxmr.com/api/pool/blocks?limit=100").tag(name).cycletime(120).json() {
            Ok(b) => jsonx::as_list(&b).into_iter().map(|x| jsonx::num(jsonx::get(x, "ts"))).collect(),
            Err(_) => {
                log_info!("Pool Currency API ({}) has failed. ", name);
                vec![]
            }
        };
        let coin = data::get_coin(cur, "").unwrap_or_default();
        let blk = blk_rate(&times, 86400000.0);
        let tsl = if times.is_empty() { None } else { Some(ts - (times.iter().cloned().fold(0.0, f64::max) / 1000.0).round()) };
        let mut st = None;
        if !ctx.info_only {
            let mut p = SetStat::new(format!("{}_XMR_Profit", name), 0.0, ctx.stat_span);
            p.hashrate = jsonx::num(jsonx::path(&req, &["pool_statistics", "hashRate"]));
            p.blockrate = blk;
            p.quiet = true;
            let s = stat::set_stat(p);
            if s.hashrate_live == 0.0 && !ctx.allow_zero {
                return Ok(vec![]);
            }
            st = Some(s);
        }
        let wallet = ctx.wallet(cur).unwrap_or("").to_string();
        let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
        p.CoinName = coin.name.clone();
        p.CoinSymbol = cur.into();
        p.Currency = cur.into();
        p.Protocol = "stratum+tcp".into();
        p.Host = "pool.supportxmr.com".into();
        p.Port = 3333;
        p.User = format!("{}{{diff:+$difficulty}}", wallet);
        p.Pass = ctx.worker_tag();
        p.Region = data::get_region("us");
        p.PoolFee = 0.6;
        p.Workers = jsonx::num_opt(jsonx::path(&req, &["pool_statistics", "miners"]));
        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
        p.BLK = st.as_ref().map(|s| s.blockrate_average);
        p.TSL = tsl;
        p.WTM = true;
        p.EthMode = "stratum".into();
        p.Wallet = wallet;
        Ok(vec![p])
    }
}

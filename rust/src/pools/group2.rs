//! Pools/K1Pool.ps1, K1PoolSolo.ps1, Kryptex.ps1, KryptexSolo.ps1, Mining4people.ps1,
//! Mining4peopleSolo.ps1 (the solo variants are priced by WhatToMine)

use crate::data;
use crate::jsonx;
use crate::log_warn;
use crate::pool::{Pool, PoolCtx, PoolModule};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use anyhow::Result;
use serde_json::Value;

/// (Get-Culture).TextInfo.ToTitleCase
pub fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// a stat with value 0 for the WhatToMine priced pools (difficulty only)
fn solo_stat(ctx: &PoolCtx, name: String, diff: f64) -> stat::Stat {
    let mut p = SetStat::new(name, 0.0, ctx.stat_span);
    p.difficulty = diff;
    p.quiet = true;
    stat::set_stat(p)
}

/// the solo fields of a pool: no price, no pool hashrate, the difficulty
fn set_solo(p: &mut Pool, st: &Option<stat::Stat>) {
    p.Price = 0.0;
    p.StablePrice = 0.0;
    p.MarginOfError = 0.0;
    p.Workers = None;
    p.Hashrate = None;
    p.BLK = None;
    p.TSL = None;
    p.Difficulty = st.as_ref().map(|s| s.diff_average);
    p.SoloMining = true;
    p.WTM = true;
}

// ------------------------------------------------------------------------------------------

pub struct K1Pool {
    pub solo: bool,
}

impl PoolModule for K1Pool {
    fn name(&self) -> &'static str {
        if self.solo {
            "K1PoolSolo"
        } else {
            "K1Pool"
        }
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = match web::Cached::new("https://api.rbminer.net/data/k1pool.json").tag("K1Pool").timeout(30).cycletime(3600).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed.", name);
                return Ok(vec![]);
            }
        };
        let mut out = Vec::new();
        for v in jsonx::as_list(&req) {
            let pname = jsonx::s(jsonx::get(v, "name"));
            if pname.ends_with("solo") != self.solo {
                continue;
            }
            let cur = jsonx::s(jsonx::get(v, "symbol"));
            let user = ctx.wallet(&cur).map(|s| s.to_string());
            if user.is_none() && !ctx.info_only {
                continue;
            }
            let user = user.unwrap_or_default();
            let (algo, coin_name) = match data::get_coin(&cur, "") {
                Some(c) => (c.algo.clone(), c.name.clone()),
                None => (data::get_algorithm(&jsonx::s(jsonx::get(v, "algo"))), title_case(&pname)),
            };
            let eth = super::eth_mode_default(&algo, "ethstratum1");
            let url = format!("https://k1pool.com/api/{}/{}", if self.solo { "stats" } else { "blocks" }, pname);
            let sreq = match web::Cached::new(&url).tag(name).timeout(30).cycletime(120).json() {
                Ok(v) => v,
                Err(_) => {
                    log_warn!("Pool API ({}) for pool {} has failed.", name, pname);
                    continue;
                }
            };
            let mut st = None;
            let mut tsl = None;
            let mut workers = None;
            if !ctx.info_only {
                if self.solo {
                    st = Some(solo_stat(ctx, format!("{}_{}_Profit", name, cur), jsonx::num(jsonx::get(&sreq, "networkDiff"))));
                } else {
                    let pool = jsonx::get(&sreq, "pool");
                    let ts = util::unix_timestamp();
                    let bt = jsonx::num(jsonx::get(pool, "coinBlocktime"));
                    let ns = jsonx::num(jsonx::get(pool, "networkSpeed"));
                    let phr = jsonx::num(jsonx::get(pool, "poolHashrate"));
                    let blk = if bt > 0.0 && ns > 0.0 {
                        86400.0 / bt * phr / ns
                    } else {
                        let blocks = jsonx::get(&sreq, "blocks");
                        let mut t: Vec<f64> = Vec::new();
                        for k in ["candidates", "immature", "matured"] {
                            for b in jsonx::as_list(jsonx::get(blocks, k)) {
                                let x = jsonx::num(jsonx::get(b, "timestamp"));
                                if x >= (ts - 86400) as f64 {
                                    t.push(x);
                                }
                            }
                        }
                        let (mn, mx) = (t.iter().cloned().fold(f64::INFINITY, f64::min), t.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
                        let f = if t.len() > 1 && mx - mn > 0.0 { 86400.0 / (mx - mn) } else { 1.0 };
                        (f * t.len() as f64).round()
                    };
                    tsl = Some((ts as f64 - jsonx::num(jsonx::path(&sreq, &["blocks", "lastBlockFound"]))).round());
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                    p.hashrate = phr.trunc();
                    p.blockrate = blk;
                    p.quiet = true;
                    let s = stat::set_stat(p);
                    if s.hashrate_live == 0.0 && !ctx.allow_zero {
                        continue;
                    }
                    workers = Some(jsonx::num(jsonx::get(pool, "minersTotal")).trunc());
                    st = Some(s);
                }
            }
            for (region, hosts) in jsonx::entries(jsonx::get(v, "stratum")) {
                for h in jsonx::as_list(hosts) {
                    let ssl = jsonx::truthy(jsonx::get(h, "ssl"));
                    let zil = !self.solo && cur == "ZIL";
                    let a = if zil { "ZilliqaDual".to_string() } else { algo.clone() };
                    let mut p = ctx.pool(&a).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.Price = if zil { 3e-16 } else { 0.0 };
                    p.StablePrice = p.Price;
                    p.CoinName = coin_name.clone();
                    p.CoinSymbol = cur.clone();
                    p.Currency = cur.clone();
                    p.Protocol = super::protocol(ssl);
                    p.Host = jsonx::s(jsonx::get(h, "host"));
                    p.Port = jsonx::num(jsonx::get(h, "port")) as u32;
                    p.User = format!("{}.{}", user, ctx.worker_tag());
                    p.Pass = "x".into();
                    p.Region = data::get_region(&region);
                    p.SSL = ssl;
                    p.PoolFee = jsonx::num(jsonx::get(v, "fee"));
                    if self.solo {
                        set_solo(&mut p, &st);
                    } else {
                        p.Workers = workers;
                        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                        p.TSL = tsl;
                        p.BLK = st.as_ref().map(|s| s.blockrate_average);
                        p.WTM = cur != "ZIL";
                    }
                    p.EthMode = eth.clone();
                    p.Wallet = user.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct Kryptex {
    pub solo: bool,
}

fn is_email(s: &str) -> bool {
    regex::Regex::new(r"^\s*[^@\s]+@[^@\s]+\s*$").unwrap().is_match(s)
}

impl PoolModule for Kryptex {
    fn name(&self) -> &'static str {
        if self.solo {
            "KryptexSolo"
        } else {
            "Kryptex"
        }
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = match web::Cached::new("https://api.rbminer.net/data/kryptex.json").tag(name).cycletime(120).retry(5).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        if jsonx::as_list(&req).is_empty() {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let email = if self.solo { String::new() } else { ctx.field("Email") };
        let mining_user = if self.solo { String::new() } else { ctx.field("MiningUsername") };
        let regions = ["eu", "ru", "sg", "us"];
        let mut out = Vec::new();
        for v in jsonx::as_list(&req) {
            if self.solo && !jsonx::str_list(jsonx::get(v, "modes")).iter().any(|m| m == "solo") {
                continue;
            }
            let rpc = jsonx::s(jsonx::get(v, "rpc"));
            let (cur, algo_raw) = if rpc == "qtc" { ("QUAN".to_string(), "Quantus".to_string()) } else { (jsonx::s(jsonx::get(v, "symbol")), jsonx::s(jsonx::get(v, "algo"))) };
            let has_wallet = ctx.wallet(&cur).is_some();
            if !has_wallet && (self.solo || (email.is_empty() && mining_user.is_empty())) && !ctx.info_only {
                continue;
            }
            let algo = data::get_algorithm(&algo_raw);
            let stat_name = format!("{}{}", cur, if rpc != cur.to_lowercase() { format!("_{}", algo) } else { String::new() });
            let direct = jsonx::truthy(jsonx::get(v, "directMining"));
            let profit = jsonx::num(jsonx::get(v, "profit"));
            let wtm = !(profit > 0.0);
            let blk_v = jsonx::num(jsonx::get(v, "blk"));
            let hr_v = jsonx::num(jsonx::get(v, "hashrate"));
            let has_blk = blk_v > 0.0 || hr_v == 0.0;
            let tsl_v = jsonx::num(jsonx::get(v, "tsl"));
            let mut st = None;
            if !ctx.info_only {
                if self.solo {
                    let diff = if jsonx::truthy(jsonx::get(v, "diff_native")) { jsonx::num(jsonx::get(v, "diff")) } else { 0.0 };
                    st = Some(solo_stat(ctx, format!("{}_{}_Profit", name, stat_name), diff));
                } else {
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, stat_name), if wtm { 0.0 } else { profit }, ctx.stat_span);
                    p.hashrate = hr_v;
                    p.blockrate = if has_blk { blk_v.trunc() } else { 0.0 };
                    p.quiet = true;
                    let s = stat::set_stat(p);
                    if s.hashrate_live == 0.0 && !ctx.allow_zero {
                        continue;
                    }
                    st = Some(s);
                }
            }
            // the credential decides login and payout currency
            let (ex_cur, wallet) = if self.solo {
                (Some(cur.clone()), format!("solo:{}", ctx.wallet(&cur).unwrap_or("").trim_start_matches("solo:")))
            } else if let Some(w) = ctx.wallet(&cur) {
                (Some(if direct && is_email(w) { "BTC".to_string() } else { cur.clone() }), w.to_string())
            } else if !mining_user.is_empty() {
                if !direct {
                    continue;
                }
                (Some("BTC".into()), mining_user.clone())
            } else if !email.is_empty() {
                (if direct && is_email(&email) { Some("BTC".into()) } else { None }, email.clone())
            } else {
                (None, String::new())
            };
            let ex_cur = match ex_cur {
                Some(c) => c,
                None if ctx.info_only => cur.clone(),
                None => continue,
            };
            for region in regions {
                for ssl in ["", "ssl_"] {
                    let re = regex::Regex::new(&format!(r"^(.+?-{}.+?):(\d+)$", region)).unwrap();
                    for url in jsonx::as_list(jsonx::path(v, &["servers", &format!("{}urls", ssl)])) {
                        let u = jsonx::s(url);
                        let Some(c) = re.captures(&u) else { continue };
                        let is_ssl = !ssl.is_empty();
                        let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                        p.CoinName = jsonx::s(jsonx::get(v, "coin"));
                        p.CoinSymbol = cur.clone();
                        p.Currency = ex_cur.clone();
                        p.Protocol = super::protocol(is_ssl);
                        p.Host = c[1].to_string();
                        p.Port = c[2].parse().unwrap_or(0);
                        p.User = format!("{}/{}", wallet, ctx.worker_tag());
                        p.Pass = "x".into();
                        p.Region = data::get_region(region);
                        p.SSL = is_ssl;
                        if self.solo {
                            p.PoolFee = jsonx::num(jsonx::get(v, "fee_solo"));
                            set_solo(&mut p, &st);
                            p.Updated = chrono::Utc::now();
                            p.Wallet = wallet.clone();
                        } else {
                            if wtm {
                                p.Price = 0.0;
                                p.StablePrice = 0.0;
                                p.MarginOfError = 0.0;
                            }
                            p.PoolFee = jsonx::num(jsonx::get(v, "fee_pps"));
                            p.Workers = jsonx::num_opt(jsonx::get(v, "miners"));
                            p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                            p.TSL = if tsl_v >= 0.0 && !jsonx::get(v, "tsl").is_null() { Some(tsl_v) } else { None };
                            p.BLK = if has_blk { st.as_ref().map(|s| s.blockrate_average) } else { None };
                            p.PaysLive = jsonx::s(jsonx::get(v, "fee_type")) == "PPS+";
                            p.WTM = wtm;
                            p.Wallet = ctx.wallet(&cur).unwrap_or("").to_string();
                        }
                        out.push(p);
                    }
                }
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct Mining4people {
    pub solo: bool,
}

impl PoolModule for Mining4people {
    fn name(&self) -> &'static str {
        if self.solo {
            "Mining4peopleSolo"
        } else {
            "Mining4people"
        }
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = match web::Cached::new("https://api.rbminer.net/data/mining4people.json").tag(name).cycletime(120).retry(3).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        if jsonx::as_list(&req).is_empty() {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let regions = ["au", "br", "eu", "in", "jp", "us-west", "us-east", "us-cent"];
        let fee_type = if self.solo { "PPLNSBF70" } else { "PPLNSBF" };
        let re_id = regex::Regex::new(r"^\w+-\w+-pplns").unwrap();
        let mut out = Vec::new();
        for v in jsonx::as_list(&req) {
            let cur = jsonx::s(jsonx::get(v, "coin"));
            if jsonx::s(jsonx::get(v, "feeType")) != fee_type || (ctx.wallet(&cur).is_none() && !ctx.info_only) {
                continue;
            }
            let coin = data::get_coin(&cur, "");
            let (algo, coin_name) = match &coin {
                Some(c) if !re_id.is_match(&jsonx::s(jsonx::get(v, "id"))) && !c.multi => (c.algo.clone(), c.name.clone()),
                _ => (data::get_algorithm_coin(&jsonx::s(jsonx::get(v, "algorithm")), &cur), jsonx::s(jsonx::get(v, "name"))),
            };
            let eth = super::eth_mode_default(&algo, "ethstratumnh");
            let mut ports = [0u32, 0u32];
            let mut plist: Vec<&Value> = jsonx::as_list(jsonx::get(v, "ports"));
            plist.sort_by(|a, b| {
                let ka = (jsonx::num(jsonx::get(a, "difficulty")), jsonx::num(jsonx::get(a, "port")), jsonx::truthy(jsonx::get(a, "tls")));
                let kb = (jsonx::num(jsonx::get(b, "difficulty")), jsonx::num(jsonx::get(b, "port")), jsonx::truthy(jsonx::get(b, "tls")));
                ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
            });
            for p in plist {
                let i = if jsonx::truthy(jsonx::get(p, "tls")) { 1 } else { 0 };
                if ports[i] == 0 {
                    ports[i] = jsonx::num(jsonx::get(p, "port")) as u32;
                }
            }
            let mut st = None;
            if !ctx.info_only {
                let hashrate = jsonx::num(jsonx::get(v, "poolHashrate"));
                let mut p = if self.solo {
                    let mut p = SetStat::new(format!("{}_{}_{}_Profit", name, algo, cur), 0.0, ctx.stat_span);
                    p.difficulty = jsonx::num(jsonx::get(v, "diff"));
                    p
                } else {
                    let reward = jsonx::num(jsonx::get(v, "blockReward"));
                    let btc = rates::btc_price(&cur);
                    let nh = jsonx::num(jsonx::get(v, "networkHashrate"));
                    let nb = jsonx::num(jsonx::get(v, "networkBlocktime"));
                    let live = if nh > 0.0 && nb > 0.0 { btc * reward * 86400.0 / nb / nh } else { 0.0 };
                    let mut p = SetStat::new(format!("{}_{}_{}_Profit", name, algo, cur), live, ctx.stat_span);
                    p.blockrate = jsonx::num(jsonx::get(v, "blk"));
                    p
                };
                p.hashrate = hashrate;
                p.quiet = true;
                let s = stat::set_stat(p);
                if !self.solo && s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let user = ctx.wallet(&cur).unwrap_or("").to_string();
            for (i, port) in ports.iter().enumerate() {
                let tls = i == 1;
                if !((!user.is_empty() && *port > 0) || ctx.info_only) {
                    continue;
                }
                for region in regions {
                    let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin_name.clone();
                    p.CoinSymbol = cur.clone();
                    p.Currency = cur.clone();
                    p.Protocol = super::protocol(tls);
                    p.Host = format!("{}.mining4people.com", region);
                    p.Port = *port;
                    p.User = format!("{}.{}", user, ctx.worker_tag());
                    p.Pass = "x{diff:,d=$difficulty}".into();
                    p.Region = data::get_region(region);
                    p.SSL = tls;
                    p.PoolFee = jsonx::num(jsonx::get(v, "fee"));
                    p.Workers = jsonx::num_opt(jsonx::get(v, "workers"));
                    if self.solo {
                        set_solo(&mut p, &st);
                        p.Workers = jsonx::num_opt(jsonx::get(v, "workers"));
                        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                        p.TSL = jsonx::num_opt(jsonx::get(v, "tsl"));
                    } else {
                        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                        p.TSL = jsonx::num_opt(jsonx::get(v, "tsl"));
                        p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    }
                    p.EthMode = eth.clone();
                    p.Wallet = user.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

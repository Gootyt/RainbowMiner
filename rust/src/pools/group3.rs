//! Pools/BaikalMine.ps1, BaikalMinePPS.ps1, BaikalMineSolo.ps1, DeepMinerZ.ps1, DeepMinerZSolo.ps1

use crate::data;
use crate::jsonx;
use crate::log_warn;
use crate::pool::{Pool, PoolCtx, PoolModule};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;

fn solo_fields(p: &mut Pool, st: &Option<stat::Stat>) {
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

#[derive(Clone, Copy, PartialEq)]
pub enum BaikalType {
    Pplns,
    Pps,
    Solo,
}

pub struct BaikalMine {
    pub t: BaikalType,
}

impl PoolModule for BaikalMine {
    fn name(&self) -> &'static str {
        match self.t {
            BaikalType::Pplns => "BaikalMine",
            BaikalType::Pps => "BaikalMinePPS",
            BaikalType::Solo => "BaikalMineSolo",
        }
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let (ptype, fee) = match self.t {
            BaikalType::Pplns => ("pplns", 0.5),
            BaikalType::Pps => ("pps_plus", 0.75),
            BaikalType::Solo => ("solo", 1.0),
        };
        let body = json!({"props": ["_id","api","active","maintenance","type._id","type.name","type.identifier","coin.symbol","coin.name","coin.identifier","ports"]});
        let req = match web::Cached::new("https://baikalmine.com/api/Pool/GetEntities")
            .request(web::Request::get("https://baikalmine.com/api/Pool/GetEntities").post_json(&body))
            .tag("BaikalMine")
            .cycletime(3600)
            .retry(5)
            .json()
        {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let entities = jsonx::as_list(jsonx::get(&req, "entities"));
        if entities.is_empty() {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let mut coins: HashMap<String, String> = HashMap::new();
        for d in &entities {
            let s = jsonx::s(jsonx::path(d, &["coin", "symbol"]));
            coins.insert(s.clone(), if s == "PEARL" { "PRL".into() } else { s });
        }
        let regions = ["Netherlands", "France", "Moscow"];
        let mut out = Vec::new();
        for e in entities {
            let sym = jsonx::s(jsonx::path(e, &["coin", "symbol"]));
            let cur = coins.get(&sym).cloned().unwrap_or(sym.clone());
            if jsonx::s(jsonx::path(e, &["type", "identifier"])) != ptype || !jsonx::truthy(jsonx::get(e, "active")) || jsonx::truthy(jsonx::get(e, "maintenance")) {
                continue;
            }
            let wallet = ctx.wallet(&cur).map(|s| s.to_string());
            if wallet.is_none() && !ctx.info_only {
                continue;
            }
            let wallet = wallet.unwrap_or_default();
            let multi = if cur == "KRGN" {
                let a = jsonx::s(jsonx::path(e, &["coin", "algorithm"]));
                if a.is_empty() {
                    "KawPow".to_string()
                } else {
                    a
                }
            } else {
                String::new()
            };
            let Some(coin) = data::get_coin(&cur, &multi) else {
                log_warn!("Pool {}: missing coin {} in db", name, cur);
                continue;
            };
            let algo = coin.algo.clone();
            let eth = super::eth_mode_default(&algo, "qtminer");
            let ident = jsonx::s(jsonx::path(e, &["coin", "identifier"]));
            let ibody = json!({"type": ptype, "coin": ident});
            let info = match web::Cached::new("https://baikalmine.com/api/Engines/GetPoolStats")
                .request(web::Request::get("https://baikalmine.com/api/Engines/GetPoolStats").post_json(&ibody))
                .tag(name)
                .cycletime(120)
                .delay(250)
                .json()
            {
                Ok(v) if !jsonx::get(&v, "entity").is_null() => v,
                _ => {
                    log_warn!("Pool {}: Info API for {} has failed. ", name, cur);
                    continue;
                }
            };
            let ent = jsonx::get(&info, "entity");
            let mut st = None;
            let (mut workers, mut tsl) = (None, None);
            if !ctx.info_only {
                if self.t == BaikalType::Solo {
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                    p.difficulty = jsonx::num(jsonx::get(ent, "difficulty")) / 4294967296.0;
                    p.quiet = true;
                    st = Some(stat::set_stat(p));
                } else {
                    let phr = jsonx::num(jsonx::path(ent, &["hashrate", "pool"]));
                    let nhr = jsonx::num(jsonx::path(ent, &["hashrate", "network"]));
                    let bt = jsonx::num(jsonx::get(ent, "blocktime"));
                    workers = jsonx::num_opt(jsonx::get(ent, "miners"));
                    tsl = Some(util::unix_timestamp() as f64 - jsonx::num(jsonx::get(ent, "lastBlock")));
                    let blk = if bt > 0.0 && nhr > 0.0 { 86400.0 / bt * phr / nhr } else { 0.0 };
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                    p.hashrate = phr;
                    p.blockrate = blk;
                    p.quiet = true;
                    let s = stat::set_stat(p);
                    if s.hashrate_live == 0.0 && !ctx.allow_zero {
                        continue;
                    }
                    st = Some(s);
                }
            }
            let re_mallob = regex::Regex::new(r"(http.+?)$").unwrap();
            for pi in jsonx::as_list(jsonx::get(e, "ports")) {
                if jsonx::truthy(jsonx::get(pi, "asic")) {
                    continue;
                }
                let loc = jsonx::s(jsonx::get(pi, "location"));
                let region = regions.iter().find(|r| crate::miner::ps_match(&loc, r)).copied().unwrap_or("Moscow");
                let ssl = jsonx::truthy(jsonx::get(pi, "ssl"));
                let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = coin.name.clone();
                p.CoinSymbol = cur.clone();
                p.Currency = cur.clone();
                p.Protocol = super::protocol(ssl);
                p.Host = jsonx::s(jsonx::get(pi, "ip"));
                p.Port = jsonx::num(jsonx::get(pi, "port")) as u32;
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x".into();
                p.Region = data::get_region(region);
                p.SSL = ssl;
                p.PoolFee = fee;
                p.Mallob = re_mallob.captures(&jsonx::s(jsonx::get(pi, "additionally"))).map(|c| c[1].to_string()).unwrap_or_default();
                if self.t == BaikalType::Solo {
                    solo_fields(&mut p, &st);
                } else {
                    p.Price = 0.0;
                    p.StablePrice = 0.0;
                    p.Workers = workers;
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.TSL = tsl;
                    p.BLK = if self.t == BaikalType::Pps { None } else { st.as_ref().map(|s| s.blockrate_average) };
                    p.PaysLive = self.t == BaikalType::Pps;
                    p.WTM = true;
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

pub struct DeepMinerZ {
    pub solo: bool,
}

impl PoolModule for DeepMinerZ {
    fn name(&self) -> &'static str {
        if self.solo {
            "DeepMinerZSolo"
        } else {
            "DeepMinerZ"
        }
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let cur = "DNX";
        let user = ctx.wallet(cur).map(|s| s.to_string());
        if user.is_none() && !ctx.info_only {
            return Ok(vec![]);
        }
        let user = user.unwrap_or_default();
        let req = match web::Cached::new("https://pool.deepminerz.com:8071/live_stats").tag(name).cycletime(120).retry(5).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let regions = ["uk", "us", "us2", "sg", "uae", "kaunda", "xpone", "rondy"];
        let region_of = |r: &str| -> String {
            data::get_region(match r {
                "us" => "uswest",
                "us2" => "useast",
                "kaunda" => "eunorth",
                "pasteyy" => "fr",
                "redpanda" => "ca",
                "xpone" => "eueast",
                "rondy" => "us",
                x => x,
            })
        };
        let coin = data::get_coin(cur, "").unwrap_or_default();
        let algo = coin.algo.clone();
        let cfg = jsonx::get(&req, "config");
        let fee = jsonx::num(jsonx::get(cfg, if self.solo { "soloFee" } else { "fee" }));
        let factor = jsonx::num(jsonx::get(cfg, "coinUnits"));
        let last = jsonx::num(jsonx::path(&req, &["lastblock", "timestamp"]));
        let tsl = if last > 0.0 { Some(util::unix_timestamp() as f64 - last) } else { None };
        let mut st = None;
        if !ctx.info_only {
            let pool = jsonx::get(&req, "pool");
            let diff = jsonx::num(jsonx::path(&req, &["network", "difficulty"]));
            if self.solo {
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.hashrate = jsonx::num(jsonx::get(pool, "hashrateSolo"));
                p.difficulty = diff;
                p.quiet = true;
                st = Some(stat::set_stat(p));
            } else {
                let ts24 = (util::unix_timestamp() - 86400) as f64;
                let re_t = regex::Regex::new(r"^prop:.+?:.+?:(\d+)").unwrap();
                let re_r = regex::Regex::new(r"^prop:.+?:.+?:\d+:.+?:.+?:.+?:([\d\.]+)").unwrap();
                let blocks: Vec<String> = jsonx::as_list(jsonx::get(pool, "blocks")).iter().map(|b| jsonx::s(b)).collect();
                let times: Vec<f64> = blocks.iter().filter_map(|b| re_t.captures(b).and_then(|c| c[1].parse::<f64>().ok())).filter(|t| *t >= ts24).collect();
                let rewards: Vec<f64> = blocks.iter().filter_map(|b| re_r.captures(b).and_then(|c| c[1].parse::<f64>().ok())).filter(|r| *r > 0.0).collect();
                let reward = if rewards.is_empty() || factor == 0.0 { 0.0 } else { rewards.iter().sum::<f64>() / rewards.len() as f64 / factor };
                let (mn, mx) = (times.iter().cloned().fold(f64::INFINITY, f64::min), times.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
                let blk = (if times.len() > 1 && mx - mn > 0.0 { 86400.0 / (mx - mn) } else { 1.0 }) * times.len() as f64;
                let profit = match rates::get(cur) {
                    Some(r) if diff > 0.0 => (86400.0 / diff) * reward / r,
                    _ => 0.0,
                };
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), profit, ctx.stat_span);
                p.hashrate = jsonx::num(jsonx::get(pool, "hashrate"));
                p.blockrate = blk;
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    return Ok(vec![]);
                }
                st = Some(s);
            }
        }
        let mut out = Vec::new();
        for (i, port) in [4444u32, 9850].iter().enumerate() {
            let ssl = i == 1;
            for region in regions {
                let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = coin.name.clone();
                p.CoinSymbol = cur.into();
                p.Currency = cur.into();
                p.Protocol = super::protocol(ssl);
                p.Host = format!("pool.{}deepminerz.com", if region != "uk" { format!("{}.", region) } else { String::new() });
                p.Port = *port;
                p.User = format!("{}{}{{diff:+$difficulty}}", if self.solo { "SOLO:" } else { "" }, user);
                p.Pass = ctx.worker_tag();
                p.Region = region_of(region);
                p.SSL = ssl;
                p.PoolFee = fee;
                if self.solo {
                    solo_fields(&mut p, &st);
                } else {
                    p.Workers = jsonx::num_opt(jsonx::path(&req, &["pool", "miners"]));
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.TSL = tsl;
                    p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    p.ErrorRatio = st.as_ref().map(|s| s.error_ratio).unwrap_or(0.0);
                }
                p.Mallob = "mallob.deepminerz.com:9000,mallob.deepminerz.com:9001,http://mallob-ml.eu.neuropool.net/,http://mallob-ml.us.neuropool.net/,minenice.newpool.pw:1500".into();
                p.Wallet = user.clone();
                out.push(p);
            }
        }
        Ok(out)
    }
}

//! Pools/Hellominer.ps1, LeafPool.ps1, LuckPool.ps1, LuckyPool.ps1, Luxor.ps1

use crate::data;
use crate::jsonx;
use crate::log_info;
use crate::log_warn;
use crate::pool::{self, Pool, PoolCtx, PoolDataOpts, PoolModule};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use anyhow::Result;
use serde_json::Value;

pub struct Hellominer;

impl PoolModule for Hellominer {
    fn name(&self) -> &'static str {
        "Hellominer"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let regions = ["us", "eu", "asia"];
        let mut out = Vec::new();
        for (sym, port) in [("ETC", 5500u32), ("RVN", 6600), ("BTG", 8800)] {
            if ctx.wallet(sym).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(sym, "").unwrap_or_default();
            let wallet = ctx.wallet(sym).unwrap_or("").to_string();
            let eth = super::eth_mode_default(&coin.algo, "ethproxy");
            let mut req = Value::Null;
            let mut st = None;
            if !ctx.info_only {
                // the API answers with a JSON encoded JSON string
                if let Ok(t) = web::Cached::new(&format!("https://hellominer.com/api/v1?currency={}&command=PoolStats", sym)).tag(name).cycletime(300).timeout(20).text() {
                    req = match jsonx::parse(&t) {
                        Some(Value::String(inner)) => jsonx::parse(&inner).unwrap_or(Value::Null),
                        Some(v) => v,
                        None => Value::Null,
                    };
                }
                let mut p = SetStat::new(format!("{}_{}_Profit", name, sym), 0.0, ctx.stat_span);
                p.hashrate = jsonx::num(jsonx::get(&req, "Hashrate")) * jsonx::num(jsonx::get(&req, "HashrateMultiply"));
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            for region in regions {
                let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.Price = 0.0;
                p.StablePrice = 0.0;
                p.CoinName = coin.name.clone();
                p.CoinSymbol = sym.into();
                p.Currency = sym.into();
                p.Protocol = "stratum+tcp".into();
                p.Host = format!("{}1.hellominer.com", region);
                p.Port = port;
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x".into();
                p.Region = data::get_region(region);
                p.PoolFee = jsonx::num(jsonx::get(&req, "PoolFee"));
                p.Workers = jsonx::num_opt(jsonx::get(&req, "Miners"));
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.EthMode = eth.clone();
                p.WTM = true;
                p.Wallet = wallet.clone();
                out.push(p);
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct LeafPool;

/// symbol, ports, fee, rpc, regions, ssl
const LEAFPOOL: &[(&str, &[u32], f64, &str, &[&str], bool)] = &[
    ("BEAM", &[3333], 0.5, "beam", &["us", "eu", "asia"], true),
    ("ERG", &[1111, 2288], 0.0, "ergo", &["us-central", "us-west", "eu", "asia"], false),
    ("ZP", &[8811], 2.0, "zp", &["us-east", "eu", "asia"], false),
];

impl PoolModule for LeafPool {
    fn name(&self) -> &'static str {
        "LeafPool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut out = Vec::new();
        for (cur, ports, fee0, rpc, regions, ssl0) in LEAFPOOL {
            if ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let mut fee = *fee0;
            let (mut st, mut workers, mut tsl) = (None, None, None);
            if !ctx.info_only {
                // none of the coins has an own ports endpoint, so the pool list of api/stats is used
                let req = match web::Cached::new(&format!("https://api-{}.leafpool.com/api/stats", rpc)).tag(name).timeout(15).cycletime(120).json() {
                    Ok(r) => r,
                    Err(_) => {
                        log_warn!("Pool API ({}) for {} has failed. ", name, cur);
                        continue;
                    }
                };
                let pp = jsonx::values(jsonx::get(&req, "pools")).into_iter().find(|p| jsonx::s(jsonx::get(p, "symbol")) == *cur).cloned().unwrap_or(Value::Null);
                if let Some(f) = jsonx::num_opt(jsonx::get(&pp, "poolFee")) {
                    fee = f;
                }
                let mrt = jsonx::num(jsonx::get(&pp, "maxRoundTime"));
                workers = Some(jsonx::num(jsonx::get(&pp, "workerCount")).round());
                tsl = Some((jsonx::num(jsonx::get(&req, "time")) - jsonx::num(jsonx::path(&pp, &["poolStats", "blockFoundTime"])) / 1000.0).round());
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.hashrate = jsonx::num(jsonx::get(&pp, "hashrate")).round();
                p.blockrate = if mrt != 0.0 { (86400.0 / mrt).round() } else { 0.0 };
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let wallet = ctx.wallet(cur).unwrap_or("").to_string();
            // the original builds one port object from the whole port list, one pool per port is what it means
            let mut ssl = *ssl0;
            for port in ports.iter() {
                for region in regions.iter() {
                    let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = cur.to_string();
                    p.Currency = cur.to_string();
                    p.Protocol = super::protocol(ssl);
                    p.Host = format!("{}-{}.leafpool.com", rpc, region);
                    p.Port = *port;
                    p.User = format!("{}.{}", wallet, ctx.worker_tag());
                    p.Pass = "x".into();
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.WTM = true;
                    p.PoolFee = fee;
                    p.Workers = workers;
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.TSL = tsl;
                    p.BLK = st.as_ref().map(|s| s.blockrate_average);
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

pub struct LuckPool;

impl PoolModule for LuckPool {
    fn name(&self) -> &'static str {
        "LuckPool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let hybrid = util::get_yes(ctx.cfg.get("EnableHybridSoloMining")) && !ctx.field("EnableHybridSoloMining").is_empty();
        let mut out = Vec::new();
        for (cur, ports, rpc, regions) in [("VRSC", [3956u32, 3958], "verus", ["na", "eu", "ap"])] {
            if ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let mut req = Value::Null;
            let mut st = None;
            if !ctx.info_only {
                req = match web::Cached::new(&format!("https://luckpool.net/{}/stats", rpc)).tag(name).timeout(15).cycletime(120).json() {
                    Ok(r) => r,
                    Err(_) => {
                        log_warn!("Pool API ({}) for {} has failed. ", name, cur);
                        continue;
                    }
                };
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.hashrate = jsonx::num(jsonx::path(&req, &["poolStats", "hashrateSols"]));
                p.blockrate = jsonx::num(jsonx::path(&req, &["poolStats", "blocksLast24"]));
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let wallet = ctx.wallet(cur).unwrap_or("").to_string();
            for region in regions {
                for (i, port) in ports.iter().enumerate() {
                    let ssl = i > 0;
                    let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = cur.into();
                    p.Currency = cur.into();
                    p.Protocol = super::protocol(ssl);
                    p.Host = format!("{}.luckpool.net", region);
                    p.Port = *port;
                    p.User = format!("{}.{}", wallet, ctx.worker_tag());
                    p.Pass = format!("{}{{diff:,d=$difficulty}}", if hybrid { "hybrid" } else { "x" });
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.WTM = true;
                    p.PoolFee = 1.0;
                    p.Workers = jsonx::num_opt(jsonx::path(&req, &["poolStats", "workerCount"]));
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.TSL = jsonx::num_opt(jsonx::path(&req, &["poolStats", "currentRoundTimeMin"])).map(|m| m * 60.0);
                    p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct LuckyPool;

/// symbol, fee, rpc, user template, password template, algorithm
const LUCKYPOOL: &[(&str, f64, &str, &str, &str, &str)] = &[
    ("ALP", 0.9, "alp", "{wallet}.{worker}", "x", ""),
    ("BTXCHAIN", 0.9, "btx", "{wallet}", "x", ""),
    ("XCB", 0.9, "corecoin", "{wallet}{=diff}.{worker}", "x", ""),
    ("CSD", 0.9, "csd", "{wallet}.{worker}", "x", ""),
    ("EXFER", 0.9, "exfer", "{wallet}.{worker}", "LP01", ""),
    ("MDS", 1.0, "midstate", "{wallet}.{worker}", "x", ""),
    ("MCM", 0.9, "mochimo", "{wallet}{=diff}.{worker}", "x", ""),
    ("OGG", 0.9, "ogg", "{wallet}{=diff}.{worker}", "x", ""),
    ("PRL", 0.9, "pearl", "{wallet}", "x", ""),
    ("QTC", 0.9, "qtc", "{wallet}{=diff}.{worker}", "x", ""),
    ("QUAN", 1.0, "quantus", "{wallet}.{worker}", "x", ""),
    ("XTM", 0.9, "taric29", "{wallet}{=diff}.{worker}", "x", "Cuckaroo29"),
    ("XTM", 0.9, "tarirx", "{wallet}{=diff}.{worker}", "x", "RandomX"),
    ("XTM", 0.9, "tari", "{wallet}{=diff}.{worker}", "x", "SHA3x"),
    ("TSC", 3.0, "tensorcash", "{wallet}.{worker}", "x", ""),
    ("ZANO", 0.9, "zano", "{wallet}.{worker}", "x{diff}", ""),
];

impl PoolModule for LuckyPool {
    fn name(&self) -> &'static str {
        "LuckyPool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut out = Vec::new();
        for (cur, fee0, rpc, user_t, pass_t, algo) in LUCKYPOOL {
            if ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, algo).unwrap_or_default();
            if coin.algo.is_empty() {
                log_warn!("{}: {} not found in CoinsDB", name, cur);
                continue;
            }
            let eth = super::eth_mode_default(&coin.algo, "ethproxy");
            let req = match web::Cached::new(&format!("https://{}.luckypool.io/api/stats", rpc)).tag(name).timeout(15).cycletime(120).json() {
                Ok(r) => r,
                Err(_) => {
                    log_warn!("Pool API ({}) for {} has failed. ", name, cur);
                    Value::Null
                }
            };
            let has_stratum = jsonx::truthy(jsonx::path(&req, &["config", "stratum"]));
            let mut fee = *fee0;
            let (mut st, mut wtm, mut pd) = (None, false, pool::PoolData::default());
            if has_stratum && !ctx.info_only {
                fee = jsonx::num(jsonx::path(&req, &["config", "fee"]));
                let ts = util::unix_timestamp() as f64;
                let stat_name = format!("{}_{}{}_Profit", name, cur, if algo.is_empty() { String::new() } else { format!("_{}", coin.algo) });
                let nh = jsonx::num(jsonx::path(&req, &["network", "hashrate"]));
                let bt = jsonx::num(jsonx::path(&req, &["network", "blockTime"]));
                let rw = jsonx::num(jsonx::path(&req, &["network", "reward"]));
                let cu = jsonx::num(jsonx::path(&req, &["config", "coinUnits"]));
                let cu = if cu != 0.0 { cu } else { 1.0 };
                if nh != 0.0 && bt != 0.0 && rw != 0.0 {
                    // price in satoshi per coin like Get-PoolDataFromRequest, Set-Stat below divides by 1e8 for both paths
                    let amount = 86400.0 / bt * rw / nh / cu;
                    let mb = jsonx::num(jsonx::path(&req, &["markets", "btc"]));
                    let mp = jsonx::num(jsonx::path(&req, &["markets", "price"]));
                    let sat = match rates::get(cur) {
                        Some(r) => 1.0 / r * 1e8,
                        None if mb != 0.0 && mp != 0.0 => 1e8 * mp / mb,
                        None => 0.0,
                    };
                    let pool_ = jsonx::get(&req, "pool");
                    let blocks = pool::cn_block_times(jsonx::get(pool_, "blocks"));
                    let recent: Vec<f64> = blocks.iter().cloned().filter(|t| *t > ts - 86400.0).collect();
                    pd = pool::PoolData {
                        live_reward: amount * sat,
                        live_hashrate: jsonx::num(jsonx::get(pool_, "hashrate")),
                        workers: if jsonx::truthy(jsonx::get(pool_, "workers")) { jsonx::num_opt(jsonx::get(pool_, "workers")) } else { jsonx::num_opt(jsonx::get(pool_, "miners")) },
                        blk: super::group8::blk_rate(&recent, 86400.0),
                        tsl: blocks.first().map(|t| ts - t),
                        ..Default::default()
                    };
                } else {
                    pd = pool::pool_data_from_request(&req, &PoolDataOpts { currency: cur.to_string(), timestamp: ts, add_block_data: true, ..Default::default() });
                }
                wtm = pd.live_reward == 0.0;
                let mut p = SetStat::new(&stat_name, pd.live_reward / 1e8, ctx.stat_span);
                p.hashrate = pd.live_hashrate;
                p.blockrate = pd.blk;
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            if !has_stratum && !ctx.info_only {
                continue;
            }
            let wallet = ctx.wallet(cur).unwrap_or("").to_string();
            let user = user_t
                .replace("{wallet}", &wallet)
                .replace("{worker}", &ctx.worker_tag())
                .replace("{=diff}", "{diff:=$difficulty}")
                .replace("{.diff}", "{diff:.$difficulty}");
            let pass = pass_t.replace("{diff}", "{diff:,$difficulty}");
            let mut seen_tcp = false;
            let mut seen_ssl = false;
            for port in jsonx::as_list(jsonx::path(&req, &["config", "ports"])) {
                let tls = jsonx::truthy(jsonx::get(port, "tls"));
                let mut protos = Vec::new();
                if jsonx::truthy(jsonx::get(port, "tcp")) || !tls {
                    protos.push("tcp");
                }
                if tls {
                    protos.push("ssl");
                }
                for proto in protos {
                    let seen = if proto == "tcp" { &mut seen_tcp } else { &mut seen_ssl };
                    if *seen {
                        continue;
                    }
                    *seen = true;
                    for s in jsonx::as_list(jsonx::path(&req, &["config", "stratum"])) {
                        let host = if jsonx::truthy(jsonx::get(s, "server")) { jsonx::s(jsonx::get(s, "server")) } else { jsonx::s(s) };
                        let flag = jsonx::s(jsonx::get(s, "flag"));
                        let ssl = proto == "ssl";
                        let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                        p.CoinName = coin.name.clone();
                        p.CoinSymbol = cur.to_string();
                        p.Currency = cur.to_string();
                        p.Protocol = format!("stratum+{}", proto);
                        p.Host = host;
                        p.Port = jsonx::num(jsonx::get(port, "port")) as u32;
                        p.User = user.clone();
                        p.Pass = pass.clone();
                        p.Region = data::get_region(if flag.is_empty() { "eu" } else { &flag });
                        p.SSL = ssl;
                        p.SSLSelfSigned = ssl;
                        p.PoolFee = fee;
                        p.Workers = pd.workers;
                        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                        p.TSL = pd.tsl;
                        p.BLK = st.as_ref().map(|s| s.blockrate_average);
                        p.WTM = wtm;
                        p.EthMode = eth.clone();
                        p.Wallet = wallet.clone();
                        out.push(p);
                    }
                }
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct Luxor;

impl PoolModule for Luxor {
    fn name(&self) -> &'static str {
        "Luxor"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let api_key = ctx.field("API_Key");
        let user = ctx.field("User");
        if !ctx.info_only && api_key.is_empty() {
            log_warn!("{}: Please set an API_Key in pools.config.txt (on luxor.tech, sign in, then click \"API Keys\" and create)", name);
        }
        let mut out = Vec::new();
        for (cur, fee, rpc) in [("ARRR", 5.0, "arrr"), ("DASH", 3.0, "dash"), ("SC", 3.0, "sc"), ("ZEC", 3.0, "zec"), ("ZEN", 3.0, "zen")] {
            if user.is_empty() && ctx.wallet(cur).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(cur, "").unwrap_or_default();
            let mut hashrate: Option<f64> = None;
            if !ctx.info_only {
                if !api_key.is_empty() {
                    let body = format!("query={}", web::http_utility_encode(&format!("query getPoolHashrate {{ getPoolHashrate(mpn: {}, orgSlug: \"luxor\") }}", cur)));
                    let req = web::Request::get("https://api.beta.luxor.tech/graphql").timeout(15).header("x-lux-api-key", &api_key).post_form(&body);
                    match web::Cached::new("https://api.beta.luxor.tech/graphql").request(req).tag(name).cycletime(120).json() {
                        Ok(r) => hashrate = jsonx::num_opt(jsonx::path(&r, &["data", "getPoolHashrate"])),
                        Err(_) => log_info!("Pool API ({}) for {} has failed. ", name, cur),
                    }
                }
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
                p.hashrate = hashrate.unwrap_or(0.0);
                p.quiet = true;
                let s = stat::set_stat(p);
                if hashrate.is_some() && s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
            }
            let wallet = ctx.wallet(cur).map(|s| s.to_string()).unwrap_or_else(|| user.clone());
            let mut p = ctx.pool(&coin.algo);
            p.CoinName = coin.name.clone();
            p.CoinSymbol = cur.into();
            p.Currency = cur.into();
            p.Protocol = "stratum+tcp".into();
            p.Host = format!("{}.global.luxor.tech", rpc);
            p.Port = 700;
            p.User = format!("{}.{}", wallet, ctx.worker_tag());
            p.Pass = "123".into();
            p.Region = data::get_region("us");
            p.WTM = true;
            p.PoolFee = fee;
            p.Hashrate = hashrate;
            p.Wallet = wallet;
            out.push(p);
        }
        Ok(out)
    }
}

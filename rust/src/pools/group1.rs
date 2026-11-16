//! Pools/F2pool.ps1, Pools/C3pool.ps1, Pools/UUpool.ps1

use crate::data;
use crate::jsonx;
use crate::log_warn;
use crate::pool::{wallet_with_payment_id, Pool, PoolCtx, PoolModule};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use anyhow::Result;
use indexmap::IndexMap;
use serde_json::Value;

/// EthMode "ethproxy" for the Ethash like, "stratum" for ProgPow like algorithms
fn eth_proxy(algo: &str) -> String {
    super::eth_mode_default(algo, "ethproxy")
}

// ------------------------------------------------------------------------------------------

pub struct F2pool;

impl PoolModule for F2pool {
    fn name(&self) -> &'static str {
        "F2pool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = match web::Cached::new("http://api.rbminer.net/data/f2pool2022.json").tag(name).cycletime(300).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                Value::Null
            }
        };
        let mut out = Vec::new();
        for (_, v) in jsonx::entries(&req) {
            let cur = jsonx::s(jsonx::get(v, "currency"));
            let alt = jsonx::s(jsonx::get(v, "altsymbol"));
            let wallet = ctx.wallet(&cur).or_else(|| if alt.is_empty() { None } else { ctx.wallet(&alt) }).map(|s| s.to_string());
            if wallet.is_none() && !ctx.info_only {
                continue;
            }
            let wallet = wallet.unwrap_or_default();
            let mut algo = data::get_algorithm(&jsonx::s(jsonx::get(v, "algo")));
            let coin = data::get_coin(&cur, &algo);
            if let Some(c) = &coin {
                if !c.algo.is_empty() && c.algo != algo {
                    algo = c.algo.clone();
                }
            }
            let mut st = None;
            if !ctx.info_only {
                let divisor = util::convert_from_hash(&format!("1{}", jsonx::s(jsonx::get(v, "scale"))));
                let hr_unit = util::convert_from_hash(&format!("1{}", jsonx::s(jsonx::get(v, "hashrateunit"))));
                let mut rate = rates::get(&cur).unwrap_or(0.0);
                let price = jsonx::num(jsonx::get(v, "price"));
                if rate == 0.0 && price > 0.0 {
                    if let Some(usd) = rates::get("USD") {
                        rate = usd / price;
                    }
                }
                let id = jsonx::s(jsonx::get(v, "id"));
                let suffix: String = id.split('-').skip(1).collect::<Vec<_>>().join(" ");
                let value = if rate > 0.0 && divisor > 0.0 { jsonx::num(jsonx::get(v, "estimate")) / divisor / rate } else { 0.0 };
                let mut p = SetStat::new(format!("{}_{}{}_Profit", name, cur, suffix), value, ctx.stat_span);
                p.hashrate = jsonx::num(jsonx::get(v, "hashrate")) * hr_unit;
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            let (wallet, _, _) = wallet_with_payment_id(&wallet, ".");
            for strat in jsonx::as_list(jsonx::get(v, "stratum")) {
                let ssl = jsonx::truthy(jsonx::get(strat, "ssl"));
                let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = coin.as_ref().map(|c| c.name.clone()).unwrap_or_default();
                p.CoinSymbol = cur.clone();
                p.Currency = cur.clone();
                p.Protocol = super::protocol(ssl);
                p.Host = format!("{}.f2pool.com", jsonx::s(jsonx::get(strat, "host")));
                p.Port = if cur == "ETH" && regex::Regex::new(r"(?i)^0x[0-9a-f]{40}").unwrap().is_match(&wallet) { 8008 } else { jsonx::num(jsonx::get(strat, "port")) as u32 };
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x".into();
                p.Region = data::get_region(&jsonx::s(jsonx::get(strat, "region")));
                p.SSL = ssl;
                p.PoolFee = jsonx::num(jsonx::get(v, "fee"));
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.EthMode = eth_proxy(&algo);
                p.Wallet = wallet.clone();
                out.push(p);
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

/// C3pool: algo switching like MoneroOcean, paid in XMR
pub struct C3pool;

impl PoolModule for C3pool {
    fn name(&self) -> &'static str {
        "C3pool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let password = ctx.field("Password");
        let wallet = ctx.wallet("XMR").unwrap_or("").to_string();
        if !ctx.info_only && wallet.is_empty() && password.is_empty() {
            return Ok(vec![]);
        }
        let req = match web::Cached::new("https://api.rbminer.net/data/c3pool.json").tag(name).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let mut groups: IndexMap<String, Vec<&Value>> = IndexMap::new();
        for v in jsonx::as_list(&req) {
            groups.entry(jsonx::s(jsonx::get(v, "algo"))).or_default().push(v);
        }
        let mut out = Vec::new();
        for (_, mut items) in groups {
            items.sort_by(|a, b| jsonx::num(jsonx::get(b, "profit")).partial_cmp(&jsonx::num(jsonx::get(a, "profit"))).unwrap_or(std::cmp::Ordering::Equal));
            let mut first = true;
            for v in items {
                let sym = jsonx::s(jsonx::get(v, "coin"));
                let coin = data::get_coin(&sym, "");
                let algo = match &coin {
                    Some(c) => c.algo.clone(),
                    None => data::get_algorithm_coin(&jsonx::s(jsonx::get(v, "algo")), &sym),
                };
                let (mut st, mut tsl) = (None, None);
                if !ctx.info_only {
                    let t = jsonx::num(jsonx::get(v, "tsl"));
                    tsl = if t >= 0.0 { Some(t) } else { None };
                    let profit = jsonx::num(jsonx::get(v, "profit"));
                    if profit == 0.0 {
                        continue;
                    }
                    let ttf = jsonx::num(jsonx::get(v, "ttf"));
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, sym), profit, ctx.stat_span);
                    p.difficulty = jsonx::num(jsonx::get(v, "diff"));
                    p.hashrate = jsonx::num(jsonx::get(v, "hashrate"));
                    p.blockrate = if ttf > 0.0 { 86400.0 / ttf } else { 0.0 };
                    p.quiet = true;
                    let r = stat::set_stat(p);
                    if !first || (jsonx::num(jsonx::get(v, "hashrate")) == 0.0 && !ctx.allow_zero) {
                        continue;
                    }
                    st = Some(r);
                }
                first = false;
                for ssl in [false, true] {
                    let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| sym.clone());
                    p.CoinSymbol = sym.clone();
                    p.Currency = "XMR".into();
                    p.Protocol = super::protocol(ssl);
                    p.Host = "mine.c3pool.com".into();
                    p.Port = if ssl { 33333 } else { 15555 };
                    p.User = wallet.clone();
                    p.Pass = format!("{}:{}~{}", ctx.worker_tag(), password, jsonx::s(jsonx::get(v, "algo")));
                    p.Region = data::get_region("CN");
                    p.SSL = ssl;
                    p.SSLSelfSigned = ssl;
                    p.PoolFee = 0.68 + jsonx::num(jsonx::get(v, "fee"));
                    p.Workers = jsonx::num_opt(jsonx::get(v, "worker"));
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.TSL = tsl;
                    p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    p.EthMode = if data::RE_ALGO_IS_ETHASH.is_match(&algo) { "ethstratumnh".into() } else { String::new() };
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------------------------------

pub struct UUpool;

fn unit_mult(u: &str) -> f64 {
    match u {
        "K" => 1e3,
        "M" => 1e6,
        "G" => 1e9,
        "T" => 1e12,
        "P" => 1e15,
        _ => 1.0,
    }
}

impl PoolModule for UUpool {
    fn name(&self) -> &'static str {
        "UUpool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let req = match web::Cached::new("https://api.rbminer.net/data/uupool.json").tag(name).cycletime(120).retry(3).json() {
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
        let re_cur = regex::Regex::new(r"(29|31)").unwrap();
        let re_num = regex::Regex::new(r"[^\d\.]+").unwrap();
        let re_host = regex::Regex::new(r"^(.+?):(\d+)").unwrap();
        let mut out = Vec::new();
        for v in jsonx::as_list(&req) {
            let coin_raw = jsonx::s(jsonx::get(v, "coin"));
            let cur = {
                let c = re_cur.replace_all(&coin_raw, "").to_string();
                match c.as_str() {
                    "VDS" => "VOLLAR".to_string(),
                    "ULORD" => "UT".to_string(),
                    _ => c,
                }
            };
            let user = ctx.wallet(&cur).or_else(|| ctx.wallet(&coin_raw)).map(|s| s.to_string());
            if user.is_none() && !ctx.info_only {
                continue;
            }
            let user = user.unwrap_or_default();
            let algo = data::get_algorithm(&jsonx::s(jsonx::get(v, "algorithm")));
            let hr_s = jsonx::s(jsonx::get(v, "hr"));
            if hr_s.is_empty() {
                continue;
            }
            let hr: Vec<&str> = hr_s.split_whitespace().collect();
            let est_s = jsonx::s(jsonx::get(v, "est"));
            let est: Vec<&str> = est_s.split(|c: char| c.is_whitespace() || c == '/').filter(|s| !s.is_empty()).collect();
            let hr_value: f64 = re_num.replace_all(hr.first().copied().unwrap_or(""), "").parse().unwrap_or(0.0);
            let est_value: f64 = re_num.replace_all(est.first().copied().unwrap_or(""), "").parse().unwrap_or(0.0);
            let pool_hr = hr_value * unit_mult(hr.get(1).copied().unwrap_or(""));
            let pool_est = est_value / unit_mult(est.get(2).copied().unwrap_or(""));
            let btc = if let Some(r) = rates::get(&cur) {
                1.0 / r
            } else if let Some(r) = rates::get(&coin_raw) {
                1.0 / r
            } else {
                let usd = jsonx::num(jsonx::get(v, "usd"));
                let cny = jsonx::num(jsonx::get(v, "cny"));
                match (rates::get("USD"), rates::get("CNY")) {
                    (Some(u), _) if usd > 0.0 => usd / u,
                    (_, Some(c)) if cny > 0.0 => cny / c,
                    _ => 0.0,
                }
            };
            let mut st = None;
            if !ctx.info_only {
                let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), pool_est * btc, ctx.stat_span);
                p.hashrate = pool_hr;
                p.change_detection = true;
                p.quiet = true;
                let s = stat::set_stat(p);
                if s.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(s);
            }
            for h in jsonx::s(jsonx::get(v, "address")).split(',') {
                let Some(c) = re_host.captures(h) else { continue };
                let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = jsonx::s(jsonx::get(v, "name"));
                p.CoinSymbol = cur.clone();
                p.Currency = cur.clone();
                p.Protocol = "stratum+tcp".into();
                p.Host = c[1].trim().to_string();
                p.Port = c[2].trim().parse().unwrap_or(0);
                p.User = format!("{}.{}", user, ctx.worker_tag());
                p.Pass = "x".into();
                p.Region = data::get_region(if h.contains("(EU)") {
                    "EU"
                } else if h.contains("(US)") {
                    "US"
                } else {
                    "CN"
                });
                p.PoolFee = re_num.replace_all(&jsonx::s(jsonx::get(v, "rates")), "").parse().unwrap_or(0.0);
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.EthMode = eth_proxy(&algo);
                p.Wallet = user.clone();
                out.push(p);
            }
        }
        Ok(out)
    }
}

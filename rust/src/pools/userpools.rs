//! Pools/Userpools.ps1: pools defined in userpools.config.txt

use crate::data;
use crate::jsonx;
use crate::pool::{Pool, PoolCtx};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use crate::log_warn;
use indexmap::IndexMap;
use serde_json::Value;
use std::collections::HashMap;

/// Get-ValueFromRequest: "api1.data.price" / "api1.list[2].value" / with $Currency etc. placeholders
fn value_from_request(req: &HashMap<String, Value>, path: &str, params: &IndexMap<String, String>) -> Option<f64> {
    let mut path = path.to_string();
    for (k, v) in params {
        path = path.replace(&format!("${}", k), v);
    }
    let mut parts = path.split('.');
    let api = parts.next()?;
    let mut cur = req.get(api)?;
    let re_idx = regex::Regex::new(r"^(.*?)\[(-?\d+)\]$").unwrap();
    for p in parts {
        if let Some(c) = re_idx.captures(p) {
            if !c[1].is_empty() {
                cur = jsonx::get(cur, &c[1]);
            }
            let i: i64 = c[2].parse().ok()?;
            let arr = cur.as_array()?;
            let i = if i < 0 { arr.len() as i64 + i } else { i };
            cur = arr.get(i as usize)?;
        } else {
            cur = jsonx::get(cur, p);
        }
    }
    jsonx::num_opt(cur)
}

/// one userpool name may have several entries (currencies / coins)
pub fn get_pools(ctx: &PoolCtx) -> Vec<Pool> {
    let name = &ctx.name;
    let password = ctx.field("Password");
    let mut cache: HashMap<String, Value> = HashMap::new();
    let mut out = Vec::new();
    for up in &ctx.config.userpools {
        if !jsonx::s(jsonx::get(up, "Name")).eq_ignore_ascii_case(name) || !util::get_yes(jsonx::get(up, "Enable")) {
            continue;
        }
        let currency = {
            let c = jsonx::s(jsonx::get(up, "Currency")).trim().to_uppercase();
            if c.is_empty() {
                jsonx::s(jsonx::get(up, "CoinSymbol")).trim().to_uppercase()
            } else {
                c
            }
        };
        if ctx.wallet(&currency).is_none() && !ctx.info_only {
            continue;
        }
        let coin_symbol = jsonx::s(jsonx::get(up, "CoinSymbol")).trim().to_uppercase();
        let mut params: IndexMap<String, String> = IndexMap::new();
        params.insert("Wallet".into(), ctx.wallet(&currency).unwrap_or("").to_string());
        params.insert("WorkerName".into(), ctx.worker_tag());
        params.insert("Currency".into(), currency.clone());
        params.insert("CoinSymbol".into(), coin_symbol.clone());
        params.insert("Password".into(), password.clone());
        params.insert("Params".into(), ctx.param(&currency).unwrap_or("").to_string());
        let log = format!("Userpool {}, Currency {}{}", name, currency, if coin_symbol.is_empty() { String::new() } else { format!(", CoinSymbol {}", coin_symbol) });
        if currency.is_empty() {
            log_warn!("{}: no Currency set", log);
            continue;
        }
        let coin = if coin_symbol.is_empty() { None } else { data::get_coin(&coin_symbol, "") };
        let algo_raw = jsonx::s(jsonx::get(up, "Algorithm"));
        let algo_norm = match &coin {
            Some(c) => c.algo.clone(),
            None => {
                if coin_symbol.is_empty() {
                    data::get_algorithm(&algo_raw)
                } else {
                    data::get_algorithm_coin(&algo_raw, &coin_symbol)
                }
            }
        };
        if algo_norm.is_empty() {
            log_warn!("{}: no Algorithm set", log);
            continue;
        }
        let mut user = jsonx::s(jsonx::get(up, "User"));
        if user.is_empty() {
            user = "$Wallet.$WorkerName".into();
        }
        let mut pass = jsonx::s(jsonx::get(up, "Pass"));
        if pass.is_empty() {
            pass = "x".into();
        }
        let solo = util::get_yes(jsonx::get(up, "SoloMining"));
        let mut profit = 0.0;
        let mut profit_factor = 1.0;
        let mut vals: HashMap<&str, Option<f64>> = HashMap::new();
        let mut st = None;
        if !ctx.info_only {
            let mut req: HashMap<String, Value> = HashMap::new();
            for i in 1..=3 {
                let url = jsonx::s(jsonx::get(up, &format!("APIUrl{}", i))).trim().to_string();
                if url.is_empty() {
                    continue;
                }
                if !cache.contains_key(&url) {
                    match web::Cached::new(&url).tag(name).cycletime(120).text() {
                        Ok(t) => {
                            let v = jsonx::parse(&t).unwrap_or(Value::String(t));
                            cache.insert(url.clone(), v);
                        }
                        Err(e) => log_warn!("{}: {} {}", log, url, e),
                    }
                }
                if let Some(v) = cache.get(&url) {
                    req.insert(format!("api{}", i), v.clone());
                }
            }
            let pf = jsonx::s(jsonx::get(up, "Profit"));
            if pf == "#" || pf == "#1" || pf == "#2" || pf == "#3" {
                let k = if pf.len() == 2 { format!("api{}", &pf[1..]) } else { "api1".into() };
                profit = req.get(&k).map(jsonx::num).unwrap_or(0.0);
            } else {
                for fld in ["Profit", "ProfitFactor", "Hashrate", "Difficulty", "Workers", "TimeSinceLast", "Blocks24h", "PoolFee"] {
                    let apifld = jsonx::s(jsonx::get(up, fld)).trim().to_string();
                    if apifld.is_empty() {
                        continue;
                    }
                    let v = if apifld.chars().all(|c| c.is_ascii_digit() || "+-.,E".contains(c)) {
                        apifld.replace(',', ".").parse::<f64>().ok()
                    } else {
                        let re = regex::Regex::new(r"^(api1|api2|api3|1|2|3|apiurl1|apiurl2|apiurl3)\.").unwrap();
                        let path = if let Some(c) = re.captures(&apifld) {
                            let n = c[1].chars().last().unwrap();
                            format!("api{}{}", n, &apifld[c[1].len()..])
                        } else {
                            format!("api1.{}", apifld)
                        };
                        value_from_request(&req, &path, &params)
                    };
                    vals.insert(fld, v);
                }
                profit = vals.get("Profit").copied().flatten().unwrap_or(0.0);
                profit_factor = vals.get("ProfitFactor").copied().flatten().unwrap_or(1.0);
            }
            if solo {
                profit = 0.0;
            }
            if profit != 0.0 {
                if profit_factor != 0.0 {
                    if jsonx::s(jsonx::get(up, "ProfitFactor")).contains("mbtc_mh_factor") {
                        profit_factor *= 1e6;
                    }
                    profit /= profit_factor;
                } else {
                    profit = 0.0;
                }
                let pc = jsonx::s(jsonx::get(up, "ProfitCurrency"));
                let cur = if pc.is_empty() { currency.clone() } else { pc };
                if cur != "BTC" {
                    profit = rates::get(&cur).map(|r| profit / r).unwrap_or(0.0);
                }
            }
            let mut p = SetStat::new(
                format!("{}_{}{}_Profit", name, algo_norm, if coin_symbol.is_empty() { String::new() } else { format!("_{}", coin_symbol) }),
                profit,
                ctx.stat_span,
            );
            p.hashrate = vals.get("Hashrate").copied().flatten().unwrap_or(0.0);
            p.blockrate = vals.get("Blocks24h").copied().flatten().unwrap_or(0.0);
            p.difficulty = vals.get("Difficulty").copied().flatten().unwrap_or(0.0);
            p.quiet = true;
            st = Some(stat::set_stat(p));
        }
        for (k, v) in &params {
            user = user.replace(&format!("${}", k), v);
            pass = pass.replace(&format!("${}", k), v);
        }
        let ssl = util::get_yes(jsonx::get(up, "SSL"));
        let mut p = ctx.pool(&algo_norm);
        if profit != 0.0 {
            p = p.with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
        }
        p.Updated = chrono::Utc::now();
        p.CoinName = match &coin {
            Some(c) => c.name.clone(),
            None => {
                let n = jsonx::s(jsonx::get(up, "CoinName"));
                if n.is_empty() {
                    coin_symbol.clone()
                } else {
                    n
                }
            }
        };
        p.CoinSymbol = coin_symbol.clone();
        p.Currency = currency.clone();
        let proto = jsonx::s(jsonx::get(up, "Protocol"));
        p.Protocol = if proto.is_empty() { super::protocol(ssl) } else { proto };
        p.Host = jsonx::s(jsonx::get(up, "Host"));
        p.Port = jsonx::num(jsonx::get(up, "Port")) as u32;
        p.User = user;
        p.Pass = pass;
        let region = jsonx::s(jsonx::get(up, "Region"));
        p.Region = if region.is_empty() { "US".into() } else { data::get_region(&region) };
        p.SSL = ssl;
        p.WTM = profit == 0.0;
        p.PoolFee = vals.get("PoolFee").copied().flatten().unwrap_or_else(|| jsonx::num(jsonx::get(up, "PoolFee")));
        p.Workers = if solo { None } else { vals.get("Workers").copied().flatten() };
        p.Hashrate = if solo || vals.get("Hashrate").copied().flatten().is_none() { None } else { st.as_ref().map(|s| s.hashrate_live) };
        p.TSL = if solo { None } else { vals.get("TimeSinceLast").copied().flatten() };
        p.BLK = if solo || vals.get("Blocks24h").copied().flatten().is_none() { None } else { st.as_ref().map(|s| s.blockrate_average) };
        p.Difficulty = if vals.get("Difficulty").copied().flatten().is_none() { None } else { st.as_ref().map(|s| s.diff_average) };
        p.SoloMining = solo;
        let em = jsonx::s(jsonx::get(up, "EthMode"));
        p.EthMode = if em.is_empty() { super::eth_mode_default(&algo_norm, "ethproxy") } else { em };
        p.Wallet = params["Wallet"].clone();
        p.Worker = ctx.worker_tag();
        p.Email = ctx.field("Email");
        out.push(p);
    }
    out
}

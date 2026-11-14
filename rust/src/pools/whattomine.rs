//! Pools/WhatToMine.ps1 + Modules/WhatToMineLib.psm1: prices for pools that do not deliver them
//! (WTM = $true): hashrate.no and minerstat via api.rbminer.net, then whattomine.com

use crate::data;
use crate::jsonx;
use crate::pool::Pool;
use crate::rates;
use crate::stat::{self, SetStat, Stat};
use crate::util;
use crate::web;
use crate::log_warn;
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct WtmPrice {
    pub algorithm: String,
    pub coin_symbol: String,
    pub price: f64,
    pub stable_price: f64,
    pub margin_of_error: f64,
    pub updated: DateTime<Utc>,
    pub mode: String,
}

fn wtm_data() -> Vec<(String, String, f64)> {
    let v = data::read_data_json("wtmdata.json").unwrap_or(Value::Null);
    let mut out: Vec<(String, String, f64)> = jsonx::as_list(&v)
        .into_iter()
        .map(|x| (jsonx::s(jsonx::get(x, "algo")), jsonx::s(jsonx::get(x, "id")), jsonx::num(jsonx::get(x, "factor"))))
        .collect();
    if let Some(Value::Object(m)) = data::read_data_json("wtmfactors.json") {
        for (k, f) in m {
            if !out.iter().any(|o| o.0.eq_ignore_ascii_case(&k)) {
                out.push((k, String::new(), jsonx::num(&f)));
            }
        }
    }
    out
}

/// Get-WhatToMineUrl
fn wtm_url(factor: i64) -> String {
    let parts: Vec<String> = wtm_data()
        .into_iter()
        .filter(|d| !d.1.is_empty())
        .map(|d| format!("{0}=true&factor[{0}_hr]={1}&factor[{0}_p]=0", d.1, factor))
        .collect();
    format!("https://whattomine.com/coins.json?{}", parts.join("&"))
}

/// Get-WhatToMineFactor
fn wtm_factor(algo: &str, factor: f64) -> Option<f64> {
    wtm_data().into_iter().find(|d| d.0.eq_ignore_ascii_case(algo)).map(|d| d.2 * factor)
}

fn norm_algo(algo: &str, coin: &str) -> String {
    let a: String = algo.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if a.eq_ignore_ascii_case("ethash") || a.eq_ignore_ascii_case("kawpow") {
        data::get_algorithm_coin(&a, coin)
    } else {
        data::get_algorithm(&a)
    }
}

fn price_of(st: &Stat, avg: &str, avg_stable: &str, algo: &str, sym: &str, mode: &str) -> WtmPrice {
    WtmPrice {
        algorithm: algo.to_string(),
        coin_symbol: sym.to_string(),
        price: st.get_average(avg),
        stable_price: st.get_average(avg_stable),
        margin_of_error: st.week_fluctuation,
        updated: st.updated,
        mode: mode.to_string(),
    }
}

/// returns the prices for the (Algorithm, CoinSymbol[-Mode]) pairs of the given WTM pools
pub fn get_prices(pools: &[&Pool], stat_span: Duration, avg: &str, avg_stable: &str) -> Vec<WtmPrice> {
    let name = "WhatToMine";
    let mut wallets: Vec<(String, String)> = Vec::new();
    for p in pools {
        if p.Algorithm.contains('-') {
            continue;
        }
        let cs = if p.WTMMode.is_empty() { p.CoinSymbol.clone() } else { format!("{}-{}", p.CoinSymbol, p.WTMMode) };
        if !wallets.contains(&(p.Algorithm.clone(), cs.clone())) {
            wallets.push((p.Algorithm.clone(), cs));
        }
    }
    let mut out = Vec::new();
    if wallets.is_empty() {
        return out;
    }
    let take = |wallets: &mut Vec<(String, String)>, algo: &str, cur: &str, suffix: &str| -> bool {
        let before = wallets.len();
        wallets.retain(|(a, c)| !(a.eq_ignore_ascii_case(algo) && c.trim_end_matches(suffix) == cur));
        wallets.len() != before
    };

    // hashrate.no and minerstat
    for (url, mode, unit_div) in [
        ("https://api.rbminer.net/data/hashrateno.json", "HNO", true),
        ("https://api.rbminer.net/data/minerstat.json", "MST", false),
    ] {
        let req = match web::Cached::new(url).tag(name).cycletime(240).json() {
            Ok(v) if jsonx::count(&v) >= if unit_div { 10 } else { 1 } => v,
            Ok(_) => {
                log_warn!("Pool API {} ({}) returned nothing. ", mode, name);
                continue;
            }
            Err(_) => {
                log_warn!("Pool API {} ({}) has failed. ", mode, name);
                continue;
            }
        };
        let suffix = format!("-{}", mode);
        for x in jsonx::as_list(&req) {
            let coin = jsonx::s(jsonx::get(x, "coin"));
            if !wallets.iter().any(|(_, c)| *c == coin || *c == format!("{}{}", coin, suffix)) {
                continue;
            }
            let algo = norm_algo(&jsonx::s(jsonx::get(x, "algo")), &coin);
            if !wallets.iter().any(|(a, c)| a.eq_ignore_ascii_case(&algo) && c.trim_end_matches(suffix.as_str()) == coin) {
                continue;
            }
            let mut sat = rates::last_sat_price(&coin, 0.0);
            if sat == 0.0 {
                let usd = rates::get("USD").unwrap_or(0.0);
                let pr = jsonx::num(jsonx::get(x, "price"));
                sat = if usd > 0.0 && pr > 0.0 { pr / usd * 1e8 } else { 0.0 };
            }
            let mut value = jsonx::num(jsonx::get(x, "reward")) * sat / 1e8;
            if unit_div {
                let u = util::convert_from_hash(&format!("1{}", jsonx::s(jsonx::get(x, "unit"))));
                if u > 0.0 {
                    value /= u;
                }
            }
            let mut p = SetStat::new(format!("{}_{}_{}_{}_Profit", name, algo, coin, mode), value, stat_span);
            p.quiet = true;
            let st = stat::set_stat(p);
            out.push(price_of(&st, avg, avg_stable, &algo, &coin, mode));
            take(&mut wallets, &algo, &coin, &suffix);
        }
    }
    if wallets.is_empty() {
        return out;
    }

    // whattomine coins.json
    match web::Cached::new(&wtm_url(1000)).tag(name).cycletime(120).json() {
        Ok(req) if jsonx::count(jsonx::get(&req, "coins")) >= 10 => {
            if let Value::Object(m) = jsonx::get(&req, "coins") {
                for c in m.values() {
                    let tag = jsonx::s(jsonx::get(c, "tag"));
                    if !wallets.iter().any(|(_, w)| w.eq_ignore_ascii_case(&tag) || w.eq_ignore_ascii_case(&format!("{}-WTM", tag))) {
                        continue;
                    }
                    let algo_raw: String = jsonx::s(jsonx::get(c, "algorithm")).chars().filter(|x| x.is_ascii_alphanumeric()).collect();
                    let mut algo = norm_algo(&algo_raw, &tag);
                    let Some(divisor) = wtm_factor(&algo, 1000.0) else { continue };
                    if algo_raw.eq_ignore_ascii_case("ProgPow") {
                        algo = data::get_algorithm(&format!("ProgPow{}", tag));
                    }
                    if divisor == 0.0 || !wallets.iter().any(|(a, w)| a.eq_ignore_ascii_case(&algo) && w.trim_end_matches("-WTM") == tag) {
                        continue;
                    }
                    let sat = rates::last_sat_price(&tag, 0.0);
                    let stat_name = format!("{}_{}_{}_Profit", name, algo, tag);
                    let exists = stat::get_stat(&stat_name, "").is_some();
                    let (rew, btc_rev, exr) = if exists {
                        ("estimated_rewards", "btc_revenue", "exchange_rate")
                    } else {
                        ("estimated_rewards24", "btc_revenue24", "exchange_rate24")
                    };
                    let mut btc = if sat > 0.0 {
                        sat * jsonx::num(jsonx::get(c, rew)) / 1e8
                    } else {
                        jsonx::num(jsonx::get(c, btc_rev))
                    };
                    if btc == 0.0 {
                        let exc = jsonx::s(jsonx::get(c, "exchange_rate_curr"));
                        if let Some(r) = rates::get(&exc) {
                            let s2 = rates::last_sat_price(&tag, jsonx::num(jsonx::get(c, exr)) / r * 1e8);
                            btc = s2 * jsonx::num(jsonx::get(c, rew)) / 1e8;
                        }
                    }
                    let st = if exists {
                        let mut p = SetStat::new(&stat_name, btc / divisor, stat_span);
                        p.quiet = true;
                        stat::set_stat(p)
                    } else {
                        let mut p = SetStat::new(format!("{}_{}_{}_WTM_Profit", name, algo, tag), btc / divisor, Duration::days(1));
                        p.quiet = true;
                        stat::set_stat(p)
                    };
                    out.push(price_of(&st, avg, avg_stable, &algo, &tag, "WTM"));
                    take(&mut wallets, &algo, &tag, "-WTM");
                }
            }
        }
        Ok(_) => log_warn!("Pool API WTM-main ({}) returned nothing. ", name),
        Err(_) => log_warn!("Pool API WTM-main ({}) has failed. ", name),
    }
    if wallets.is_empty() {
        return out;
    }

    // whattomine calculators.json + per coin requests
    let calc = web::Cached::new("https://whattomine.com/calculators.json").tag(name).cycletime(12 * 3600).text();
    let calc = calc.ok().and_then(|t| jsonx::parse(&t.replace("Bitnet", "Bitnetmoney")));
    let Some(calc) = calc else {
        log_warn!("Pool API WTM-calc ({}) has failed. ", name);
        return out;
    };
    if let Value::Object(m) = jsonx::get(&calc, "coins") {
        for c in m.values() {
            if jsonx::s(jsonx::get(c, "status")) != "Active" {
                continue;
            }
            let tag = jsonx::s(jsonx::get(c, "tag"));
            if !wallets.iter().any(|(_, w)| w.eq_ignore_ascii_case(&tag) || w.eq_ignore_ascii_case(&format!("{}-WTM", tag))) {
                continue;
            }
            let algo_raw: String = jsonx::s(jsonx::get(c, "algorithm")).chars().filter(|x| x.is_ascii_alphanumeric()).collect();
            let mut algo = norm_algo(&algo_raw, &tag);
            let Some(divisor) = wtm_factor(&algo, 1000.0) else { continue };
            if algo_raw.eq_ignore_ascii_case("ProgPow") {
                algo = data::get_algorithm(&format!("ProgPow{}", tag));
            }
            if divisor == 0.0 || !wallets.iter().any(|(a, w)| a.eq_ignore_ascii_case(&algo) && w.trim_end_matches("-WTM") == tag) {
                continue;
            }
            let id = jsonx::s(jsonx::get(c, "id"));
            let url = format!("https://whattomine.com/coins/{}.json?hr=1000&p=0&fee=0.0&cost=0.0&hcost=0.0", id);
            let Ok(cr) = web::Cached::new(&url).tag(name).cycletime(120).json() else { continue };
            let ctag = jsonx::s(jsonx::get(&cr, "tag"));
            if ctag.is_empty() {
                continue;
            }
            let sat = rates::last_sat_price(&ctag, 0.0);
            let mut btc = if sat > 0.0 {
                sat * jsonx::num(jsonx::get(&cr, "estimated_rewards")) / 1e8
            } else {
                jsonx::num(jsonx::get(&cr, "btc_revenue"))
            };
            if btc == 0.0 {
                let exc = jsonx::s(jsonx::get(&cr, "exchange_rate_curr"));
                if let Some(r) = rates::get(&exc) {
                    let s2 = rates::last_sat_price(&ctag, jsonx::num(jsonx::get(&cr, "exchange_rate")) / r * 1e8);
                    btc = s2 * jsonx::num(jsonx::get(&cr, "estimated_rewards"));
                }
            }
            let stat_name = format!("{}_{}_{}_WTM_Profit", name, algo, tag);
            let st = if stat::get_stat(&stat_name, "").is_some() {
                let diff = jsonx::num(jsonx::get(&cr, "difficulty"));
                let diff24 = jsonx::num(jsonx::get(&cr, "difficulty24"));
                if diff24 > 0.0 && diff > 0.0 {
                    btc *= diff / diff24;
                }
                let mut p = SetStat::new(&stat_name, btc / divisor, stat_span);
                p.quiet = true;
                stat::set_stat(p)
            } else {
                let mut p = SetStat::new(&stat_name, btc / divisor, Duration::days(1));
                p.quiet = true;
                stat::set_stat(p)
            };
            out.push(price_of(&st, avg, avg_stable, &algo, &tag, "WTM"));
            take(&mut wallets, &algo, &tag, "-WTM");
        }
    }
    out
}

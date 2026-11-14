//! Exchange rates (Update-Rates): Rates[X] = amount of X for 1 BTC.
//! Coinbase first, api.rbminer.net as fallback and for the coins Coinbase does not list.

use crate::data;
use crate::jsonx;
use crate::web;
use crate::{log_info, log_warn};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

static RATES: LazyLock<RwLock<HashMap<String, f64>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("BTC".to_string(), 1.0);
    RwLock::new(m)
});

static TICKER: LazyLock<RwLock<Vec<String>>> = LazyLock::new(|| RwLock::new(Vec::new()));

pub fn get(sym: &str) -> Option<f64> {
    RATES.read().unwrap().get(&sym.to_uppercase()).copied().filter(|v| *v > 0.0)
}

pub fn set(sym: &str, v: f64) {
    RATES.write().unwrap().insert(sym.to_uppercase(), v);
}

pub fn all() -> HashMap<String, f64> {
    RATES.read().unwrap().clone()
}

/// Get-LastSatPrice: satoshi per coin (stores a rate from the given price if none is known)
pub fn last_sat_price(cur: &str, price: f64) -> f64 {
    match get(cur) {
        Some(r) if price == 0.0 => 1.0 / r * 1e8,
        Some(_) => price,
        None => {
            if price > 0.0 {
                set(cur, 1.0 / price * 1e8);
            }
            price
        }
    }
}

/// BTC value of 1 coin
pub fn btc_price(cur: &str) -> f64 {
    get(cur).map(|r| 1.0 / r).unwrap_or(0.0)
}

/// Update-Rates
pub fn update(currencies: &[String], extra_symbols: &[String], exclude_coinbase: &[String]) {
    let wc = data::world_currencies();
    let mut base: Vec<String> = currencies.iter().map(|c| c.to_uppercase()).collect();
    if !base.contains(&"USD".to_string()) {
        base.push("USD".into());
    }
    let mut want: Vec<String> = base.clone();
    for s in extra_symbols {
        let s = s.to_uppercase();
        if !s.is_empty() && !want.contains(&s) {
            want.push(s);
        }
    }
    for k in RATES.read().unwrap().keys() {
        if !want.contains(k) {
            want.push(k.clone());
        }
    }

    let mut new: HashMap<String, f64> = HashMap::new();
    if let Ok(v) = web::Cached::new("https://api.coinbase.com/v2/exchange-rates?currency=BTC").jobkey("coinbase").cycletime(60).json() {
        let r = jsonx::path(&v, &["data", "rates"]);
        if !jsonx::get(r, "BTC").is_null() {
            if let Value::Object(m) = r {
                for (k, x) in m {
                    let ku = k.to_uppercase();
                    if (!wc.contains(&ku) || base.contains(&ku)) && !exclude_coinbase.iter().any(|e| e.eq_ignore_ascii_case(&ku)) {
                        new.insert(ku, jsonx::num(x));
                    }
                }
            }
        }
    }
    if new.is_empty() {
        log_info!("Coinbase is down, using fallback. ");
        match web::get_json("https://api.rbminer.net/data/coinbase.json", 15) {
            Ok(v) => {
                if let Value::Object(m) = &v {
                    if m.contains_key("BTC") {
                        for (k, x) in m {
                            let ku = k.to_uppercase();
                            if !wc.contains(&ku) || base.contains(&ku) {
                                new.insert(ku, jsonx::num(x));
                            }
                        }
                    }
                }
            }
            Err(_) => log_warn!("Coinbase down. "),
        }
    }
    new.insert("BTC".into(), 1.0);
    {
        let mut rates = RATES.write().unwrap();
        rates.insert("BTC".into(), 1.0);
        let mut ticker = TICKER.write().unwrap();
        for s in &want {
            if let Some(v) = new.get(s) {
                rates.insert(s.clone(), *v);
            } else if !ticker.contains(s) {
                ticker.push(s.clone());
            }
        }
        for s in &wc {
            if let Some(v) = rates.get_mut(s) {
                *v = crate::util::round_to(*v, 3);
            }
        }
    }
    let ticker = TICKER.read().unwrap().clone();
    if !ticker.is_empty() {
        let mut t = ticker.clone();
        t.sort();
        let syms = t.join(",").to_uppercase();
        let url = format!("https://api.rbminer.net/cmc.php?symbols={}", syms);
        match web::Cached::new(&url).jobkey("morerates").cycletime(600).json() {
            Ok(v) => {
                if !jsonx::truthy(jsonx::get(&v, "status")) {
                    log_info!("api.rbminer.net/cmc failed for {}", syms);
                } else if let Value::Object(m) = jsonx::get(&v, "data") {
                    let mut rates = RATES.write().unwrap();
                    for (k, x) in m {
                        let p = jsonx::num(x);
                        rates.insert(k.to_uppercase(), if p > 0.0 { 1e8 / p } else { 0.0 });
                    }
                }
            }
            Err(_) => log_info!("api.rbminer.net/cmc API for {} has failed. ", syms),
        }
    }
}

//! Pools/Zpool.ps1 (YiiMP API)

use crate::data;
use crate::jsonx;
use crate::pool::{yiimp_value, Pool, PoolCtx, PoolModule};
use crate::stat::{self, SetStat};
use crate::web;
use crate::{log_info, log_warn};
use anyhow::Result;
use chrono::Duration;
use serde_json::Value;
use std::collections::HashMap;

pub struct Zpool;

impl PoolModule for Zpool {
    fn name(&self) -> &'static str {
        "Zpool"
    }

    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let status = match web::rest_async("https://www.zpool.ca/api/status", name, 120, 750, 30) {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        if jsonx::count(&status) <= 1 {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let currencies = web::rest_async("https://www.zpool.ca/api/currencies", name, 120, 750, 30).unwrap_or_else(|_| {
            log_warn!("Pool currencies API ({}) has failed. ", name);
            Value::Null
        });

        let regions = ["na", "eu", "sea", "jp"];
        let region_table: HashMap<&str, String> = regions.iter().map(|r| (*r, data::get_region(r))).collect();

        // payout currencies: BTC, DOGE and all currencies without disabled conversion
        let mut pool_currencies: Vec<String> = vec!["BTC".into(), "DOGE".into()];
        if let Value::Object(m) = &currencies {
            for (k, v) in m {
                if jsonx::s(jsonx::get(v, "conversion_disabled")) == "1" {
                    continue;
                }
                let sym = jsonx::get(v, "symbol");
                let s = if sym.is_null() { k.clone() } else { jsonx::s(sym) };
                if !pool_currencies.contains(&s) {
                    pool_currencies.push(s);
                }
            }
        }
        pool_currencies.retain(|c| ctx.wallet(c).is_some() || ctx.info_only);
        if !ctx.info_only && !pool_currencies.is_empty() {
            let mut ae = ctx.field("AECurrency");
            if ae.is_empty() || !pool_currencies.contains(&ae) {
                ae = pool_currencies[0].clone();
            }
            pool_currencies.retain(|c| *c == ae);
        }

        // algorithms with a single coin
        let mut coins: HashMap<String, (String, String)> = HashMap::new();
        let single_algos: Vec<String> = jsonx::values(&status)
            .into_iter()
            .filter(|v| jsonx::num(jsonx::get(v, "coins")) == 1.0)
            .map(|v| jsonx::s(jsonx::get(v, "name")))
            .collect();
        if let Value::Object(m) = &currencies {
            for (k, v) in m {
                let algo = jsonx::s(jsonx::get(v, "algo"));
                if single_algos.contains(&algo) {
                    coins.insert(algo, (jsonx::s(jsonx::get(v, "name")), k.split('-').next().unwrap_or(k).to_string()));
                }
            }
        }

        let mut out = Vec::new();
        let Value::Object(sm) = &status else { return Ok(out) };
        for (key, s) in sm {
            let algo = jsonx::s(jsonx::get(s, "name"));
            let host = "mine.zpool.ca";
            let (mut coin_name, mut coin_symbol) = coins.get(&algo).cloned().unwrap_or_default();
            let pool_fee = jsonx::num(jsonx::get(s, "fees"));
            let coin_results: Vec<&Value> = jsonx::values(&currencies)
                .into_iter()
                .filter(|c| jsonx::s(jsonx::get(c, "algo")) == algo && jsonx::num(jsonx::get(c, "estimate")) > 0.0)
                .collect();
            if coin_results.is_empty() && !ctx.info_only {
                continue;
            }
            if !coin_name.is_empty() && coin_symbol.is_empty() {
                coin_symbol = data::get_coin_symbol(&coin_name).unwrap_or_default();
            }
            let mut algo_norm = if (algo == "ethash" || algo == "kawpow") && !coin_symbol.is_empty() {
                data::get_algorithm_coin(&algo, &coin_symbol)
            } else {
                data::get_algorithm(&algo)
            };
            if algo_norm == "FiroPoW" && coin_symbol == "SCC" {
                continue;
            }
            if !ctx.algo_allowed(&algo_norm) || !ctx.coin_allowed(&coin_symbol) {
                continue;
            }
            let mut eth_mode = String::new();
            let mut sym_max = None;
            if data::RE_ALGO_HAS_DAGSIZE.is_match(&algo_norm) {
                eth_mode = if data::RE_ALGO_IS_ETHASH.is_match(&algo_norm) { "ethstratum2".into() } else { "stratum".into() };
                if coin_symbol.is_empty() {
                    let algo_coins: Vec<String> = match &currencies {
                        Value::Object(m) => m.iter().filter(|(_, v)| jsonx::s(jsonx::get(v, "algo")) == algo).map(|(k, _)| k.clone()).collect(),
                        _ => vec![],
                    };
                    if !algo_coins.is_empty() {
                        sym_max = data::get_eth_dag_size_max(&algo_coins, &algo_norm);
                        if let Some((_, ref sym)) = sym_max {
                            if algo == "ethash" || algo == "kawpow" {
                                algo_norm = data::get_algorithm_coin(&algo, sym);
                            }
                        }
                    }
                }
            }
            let factor = jsonx::num(jsonx::get(s, "mbtc_mh_factor"));
            if factor <= 0.0 {
                log_info!("{}: Unable to determine divisor for algorithm {}. ", name, algo);
                continue;
            }
            let tsl = coin_results.iter().map(|c| jsonx::num(jsonx::get(c, "timesincelast"))).fold(f64::MAX, f64::min);
            let blk = coin_results.iter().map(|c| jsonx::num(jsonx::get(c, "24h_blocks"))).fold(0.0, f64::max);

            let mut st = None;
            if !ctx.info_only {
                let stat_name = format!("{}_{}_Profit", name, algo_norm);
                let new_stat = stat::get_stat(&stat_name, "").is_none();
                let dw = if new_stat { "actual_last24h".to_string() } else { ctx.data_window.clone() };
                let price = yiimp_value(s, factor, &dw);
                let mut p = SetStat::new(&stat_name, price, if new_stat { Duration::days(1) } else { ctx.stat_span });
                p.change_detection = !new_stat;
                p.actual24h = jsonx::num(jsonx::get(s, "actual_last24h")) / 1000.0;
                p.estimate24h = jsonx::num(jsonx::get(s, "estimate_last24h"));
                p.hashrate = jsonx::num(jsonx::get(s, "hashrate"));
                p.blockrate = blk;
                p.quiet = true;
                let r = stat::set_stat(p);
                if r.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(r);
            }
            if coin_name.is_empty() {
                coin_name = String::new();
            }
            let _ = key;
            for ssl in [false, true] {
                let port = if ssl {
                    let p = jsonx::num(jsonx::get(s, "ssl_port")) as u32;
                    if p == 0 {
                        continue;
                    }
                    p
                } else {
                    jsonx::num(jsonx::get(s, "port")) as u32
                };
                for region in regions {
                    for cur in &pool_currencies {
                        let params = ctx.param(cur).map(|p| format!(",{}", p)).unwrap_or_default();
                        let mut p = ctx.pool(&algo_norm).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                        p.CoinName = coin_name.clone();
                        p.CoinSymbol = coin_symbol.clone();
                        p.Currency = cur.clone();
                        p.Protocol = super::protocol(ssl);
                        p.Host = format!("{}.{}.{}", algo, region, host);
                        p.Port = port;
                        p.User = ctx.wallet(cur).unwrap_or("").to_string();
                        p.Pass = format!("{},c={}{{diff:,sd=$difficulty}}{}", ctx.worker_tag(), cur, params);
                        p.Region = region_table[region].clone();
                        p.SSL = ssl;
                        p.SSLSelfSigned = ssl;
                        p.PoolFee = pool_fee;
                        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                        p.Workers = jsonx::num_opt(jsonx::get(s, "workers"));
                        p.BLK = st.as_ref().map(|s| s.blockrate_average);
                        p.TSL = if tsl == f64::MAX { None } else { Some(tsl) };
                        p.EthMode = eth_mode.clone();
                        if let Some((d, sym)) = &sym_max {
                            p.CoinSymbolMax = sym.clone();
                            p.DagSizeMax = Some(*d);
                        }
                        p.Wallet = ctx.wallet(cur).unwrap_or("").to_string();
                        out.push(p);
                    }
                }
            }
        }
        Ok(out)
    }
}

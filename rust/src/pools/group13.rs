//! Pools/Gtpool.ps1

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

pub struct Gtpool;

const GT_REGIONS: &[&str] = &["ru", "eu-de", "eu-fr", "eu-fi", "us-west", "us-east", "sg", "hk"];

/// POST api.gtpool.io/v2/<endpoint>?key=..
fn gt_request(ctx: &PoolCtx, endpoint: &str, key: &str, cycletime: u64) -> Option<Value> {
    let url = format!("https://api.gtpool.io/v2/{}?key={}", endpoint, key);
    let req = web::Request::get(&url).post_form("");
    web::Cached::new(&url).request(req).tag(&ctx.name).retry(3).cycletime(cycletime).json().ok()
}

/// the price of a coin in BTC per hash and day
fn gt_price(c: &Value) -> f64 {
    let p = jsonx::get(c, "profit");
    let hr = jsonx::num(jsonx::get(p, "hashrate"));
    let iv = jsonx::num(jsonx::get(p, "interval"));
    let usd = if hr != 0.0 && iv != 0.0 { jsonx::num(jsonx::get(p, "forecast")) / 1e8 / hr * 86400.0 / iv * jsonx::num(jsonx::get(c, "priceUsd")) } else { 0.0 };
    rates::get("USD").map(|r| usd / r).unwrap_or(0.0)
}

fn gt_stat(ctx: &PoolCtx, c: &Value, blk_default: f64) -> stat::Stat {
    let pool = jsonx::get(c, "pool");
    let chain = jsonx::get(c, "chain");
    let whr = jsonx::num(jsonx::get(pool, "workersHashrate"));
    let (chr, bt) = (jsonx::num(jsonx::get(chain, "hashrate")), jsonx::num(jsonx::get(chain, "blockTime")));
    let mut p = SetStat::new(format!("{}_{}_Profit", ctx.name, jsonx::s(jsonx::get(c, "ticker"))), gt_price(c), ctx.stat_span);
    p.hashrate = whr.round();
    p.blockrate = if chr != 0.0 && bt != 0.0 { whr / chr * 86400.0 / bt } else { blk_default };
    p.quiet = true;
    stat::set_stat(p)
}

impl PoolModule for Gtpool {
    fn name(&self) -> &'static str {
        "Gtpool"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let cs: Vec<&String> = ctx.cfg.coin_symbol.iter().filter(|c| !c.eq_ignore_ascii_case("ZIL")).collect();
        let ecs = &ctx.cfg.exclude_coin_symbol;
        let has_zil = ctx.cfg.coin_symbol.iter().any(|c| c.eq_ignore_ascii_case("ZIL")) && !ecs.iter().any(|c| c.eq_ignore_ascii_case("ZIL"));
        let api_key = if ctx.info_only { "XHJfbTPhG47UL1hFGU7orTR".to_string() } else { ctx.field("API_Key") };
        let account = ctx.field("Account_Id");
        let mut ok = true;
        if api_key.is_empty() {
            log_warn!("Pool {} API_Key not set", name);
            ok = false;
        }
        if account.is_empty() && !ctx.info_only {
            log_warn!("Pool {} Account_Id not set", name);
            ok = false;
        }
        if !ok {
            return Ok(vec![]);
        }
        let Some(coins) = gt_request(ctx, "coin/list", &api_key, 120) else {
            log_warn!("Pool Coins API ({}) has failed. ", name);
            return Ok(vec![]);
        };
        if !jsonx::truthy(jsonx::get(&coins, "result")) {
            log_warn!("Pool Coins API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let Some(mining) = gt_request(ctx, "mining/list", &api_key, 3600) else {
            log_warn!("Pool Mining API ({}) has failed. ", name);
            return Ok(vec![]);
        };
        if !jsonx::truthy(jsonx::get(&mining, "result")) {
            log_warn!("Pool Mining API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let payload: Vec<&Value> = jsonx::as_list(jsonx::get(&coins, "payload"));
        let eth = "ethstratumnh".to_string();
        let mut out = Vec::new();
        let emit = |out: &mut Vec<Pool>, algo: &str, coin_name: &str, sym: &str, st: &Option<stat::Stat>, c: &Value, worker: &str, host_domain: &str| {
            let tsl = util::unix_timestamp() as f64 - jsonx::num(jsonx::path(c, &["pool", "lastBlockTime"])).round();
            for ssl in [false, true] {
                for region in GT_REGIONS {
                    let mut p = ctx.pool(algo).with_stat(st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin_name.to_string();
                    p.CoinSymbol = sym.to_string();
                    p.Currency = sym.to_string();
                    p.Protocol = super::protocol(ssl);
                    p.Host = format!("{}.{}", region, host_domain);
                    p.Port = if ssl { 9009 } else { 9999 };
                    p.User = format!("{}.{{workername:{}}}", account, worker);
                    p.Pass = "x".into();
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.PoolFee = 1.0;
                    p.Workers = Some(jsonx::num(jsonx::path(c, &["pool", "workersCount"])).round());
                    p.TSL = Some(tsl);
                    p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    p.EthMode = eth.clone();
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.Wallet = account.clone();
                    out.push(p);
                }
            }
        };

        if ctx.info_only {
            for c in payload.iter().filter(|c| jsonx::truthy(jsonx::get(c, "status"))) {
                let sym = jsonx::s(jsonx::get(c, "ticker"));
                let coin = data::get_coin(&sym, "").unwrap_or_default();
                emit(&mut out, &coin.algo, &coin.name, &sym, &None, c, &ctx.worker, "gteh.org");
            }
            return Ok(out);
        }

        // the workers of the rig: per device model and the main worker
        let mut workers: Vec<String> = Vec::new();
        for m in &ctx.config.device_model {
            if let Some(w) = ctx.config.device(m).map(|d| d.worker.clone()).filter(|w| !w.is_empty()) {
                if !workers.contains(&w) {
                    workers.push(w);
                }
            }
        }
        if !workers.contains(&ctx.worker) {
            workers.push(ctx.worker.clone());
        }
        let split = |s: String| -> Vec<String> { s.split([',', ';', ' ']).filter(|x| !x.is_empty()).map(|x| x.to_string()).collect() };
        let use_wn = split(ctx.field("UseWorkerName"));
        let excl_wn = split(ctx.field("ExcludeWorkerName"));
        if !use_wn.is_empty() || !excl_wn.is_empty() {
            workers.retain(|w| (use_wn.is_empty() || use_wn.contains(w)) && (excl_wn.is_empty() || !excl_wn.contains(w)));
        }
        if workers.is_empty() {
            return Ok(out);
        }
        let mut all: Vec<&Value> = payload
            .iter()
            .copied()
            .filter(|c| {
                let t = jsonx::s(jsonx::get(c, "ticker"));
                t != "ZIL"
                    && jsonx::truthy(jsonx::get(c, "status"))
                    && (cs.is_empty() || cs.iter().any(|x| x.eq_ignore_ascii_case(&t)))
                    && (ecs.is_empty() || !ecs.iter().any(|x| x.eq_ignore_ascii_case(&t)))
            })
            .collect();
        let ratio = |c: &Value| {
            let p = jsonx::get(c, "profit");
            let hr = jsonx::num(jsonx::get(p, "hashrate"));
            if hr != 0.0 {
                jsonx::num(jsonx::get(p, "forecast")) / hr
            } else {
                0.0
            }
        };
        all.sort_by(|a, b| ratio(b).partial_cmp(&ratio(a)).unwrap_or(std::cmp::Ordering::Equal));
        if all.is_empty() {
            return Ok(out);
        }
        let zil = if has_zil { payload.iter().copied().find(|c| jsonx::s(jsonx::get(c, "ticker")) == "ZIL" && jsonx::truthy(jsonx::get(c, "status"))) } else { None };
        let Some(wreq) = gt_request(ctx, "worker/list", &api_key, 120) else {
            log_warn!("Pool Workers API ({}) has failed. ", name);
            return Ok(out);
        };
        if !jsonx::truthy(jsonx::get(&wreq, "result")) {
            log_warn!("Pool Workers API ({}) returned nothing. ", name);
            return Ok(out);
        }
        let enable_switch = util::get_yes_str(&ctx.field("EnableMiningSwitch"));
        let has_cpu = ctx.config.device_model.iter().any(|m| m == "CPU");
        let mut worker_count = 0;
        for w1 in &workers {
            let Some(cw) = jsonx::as_list(jsonx::get(&wreq, "payload")).into_iter().find(|w| jsonx::s(jsonx::get(w, "name")) == *w1) else {
                continue;
            };
            let model = if *w1 != ctx.worker {
                let mut ms: Vec<String> = ctx.config.device_model.iter().filter(|m| ctx.config.device(m).map(|d| d.worker == *w1).unwrap_or(false)).cloned().collect();
                ms.sort();
                ms.dedup();
                ms.join("-")
            } else if has_cpu {
                "GPU".to_string()
            } else {
                String::new()
            };
            let mut coin = data::get_coin(&jsonx::s(jsonx::path(cw, &["coin", "ticker"])), "").unwrap_or_default();
            let mut current = all.iter().copied().find(|c| jsonx::s(jsonx::get(c, "ticker")) == coin.symbol);
            let best = all[0];
            let best_ticker = jsonx::s(jsonx::get(best, "ticker"));
            if current.is_none() || (enable_switch && best_ticker != coin.symbol) {
                // switch the worker to the most profitable coin
                let bm = jsonx::as_list(jsonx::get(&mining, "payload"))
                    .into_iter()
                    .find(|m| jsonx::s(jsonx::path(m, &["coin", "ticker"])) == best_ticker && jsonx::s(jsonx::get(m, "config")).contains("PPLN"));
                let body = format!(
                    "{{\"workers\":[\"{}\"],\"mining\":\"{}\"}}",
                    jsonx::s(jsonx::get(cw, "uniq")),
                    bm.map(|m| jsonx::s(jsonx::get(m, "uniq"))).unwrap_or_default()
                );
                let r = web::fetch_json(&web::Request::get(&format!("https://api.gtpool.io/v2/mining/change?key={}", api_key)).post_form(&body));
                match r {
                    Err(_) => {
                        log_warn!("Pool Change Mining API ({}) has failed. ", name);
                        return Ok(out);
                    }
                    Ok(r) if !jsonx::truthy(jsonx::get(&r, "result")) => log_warn!("Pool Change Mining API ({}) returned nothing. ", name),
                    Ok(_) => {
                        current = Some(best);
                        coin = data::get_coin(&best_ticker, "").unwrap_or_default();
                    }
                }
            }
            let Some(cur_coin) = current else {
                return Ok(out);
            };
            let with_model = |a: &str| if model.is_empty() { a.to_string() } else { format!("{}-{}", a, model) };
            let st = if worker_count == 0 {
                // the first worker sets the stats of all coins
                let mut s = None;
                for c in &all {
                    let x = gt_stat(ctx, c, 0.0);
                    if jsonx::s(jsonx::get(c, "ticker")) == jsonx::s(jsonx::get(cur_coin, "ticker")) {
                        s = Some(x);
                    }
                }
                s
            } else {
                Some(gt_stat(ctx, cur_coin, 0.0))
            };
            worker_count += 1;
            if st.as_ref().map(|s| s.hashrate_live != 0.0).unwrap_or(false) || ctx.allow_zero {
                emit(&mut out, &with_model(&coin.algo), &coin.name, &coin.symbol, &st, cur_coin, w1, "gtpool.io");
            }
            if let Some(z) = zil {
                let st = Some(gt_stat(ctx, z, 26.624396348433418));
                if st.as_ref().map(|s| s.hashrate_live != 0.0).unwrap_or(false) || ctx.allow_zero {
                    emit(&mut out, &with_model("ZilliqaDual"), "Zilliqa", "ZIL", &st, z, w1, "gtpool.io");
                }
            }
        }
        Ok(out)
    }
}

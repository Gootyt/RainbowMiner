//! Pools/RPlant.ps1, RPlantSolo.ps1, Ravenminer.ps1, RavenminerSolo.ps1

use crate::data;
use crate::jsonx;
use crate::log_warn;
use crate::pool::{Pool, PoolCtx, PoolModule};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::web;
use anyhow::Result;
use serde_json::Value;

pub struct RPlant {
    pub solo: bool,
}

impl PoolModule for RPlant {
    fn name(&self) -> &'static str {
        if self.solo {
            "RPlantSolo"
        } else {
            "RPlant"
        }
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let dash = web::Cached::new("https://pool.rplant.xyz/api/dash").tag(name).timeout(30).cycletime(120).json();
        let curs = web::Cached::new("https://pool.rplant.xyz/api/currencies").tag(name).timeout(15).cycletime(120).delay(250).json();
        let (Ok(dash), Ok(curs)) = (dash, curs) else {
            log_warn!("Pool API ({}) has failed.", name);
            return Ok(vec![]);
        };
        let regions = ["eu", "asia", "na"];
        let xlat = |s: &str| -> String {
            match s {
                "SKY" => "SKYDOGE".into(),
                "BTX" => "BTXCHAIN".into(),
                "ALPHA" if !self.solo => "ALP".into(),
                x => x.to_string(),
            }
        };
        let mut out = Vec::new();
        for v in jsonx::values(jsonx::get(&dash, "tbs")) {
            let cur = jsonx::s(jsonx::get(v, "symbol"));
            let cx = xlat(&cur);
            if self.solo && !jsonx::truthy(jsonx::path(v, &["info", "solo"])) && !ctx.info_only {
                continue;
            }
            if ctx.wallet(&cx).is_none() && !ctx.info_only {
                continue;
            }
            let algo_raw = jsonx::s(jsonx::get(v, "algo"));
            let (algo, coin_name) = match data::get_coin(&cx, &algo_raw) {
                Some(c) => (c.algo.clone(), c.name.clone()),
                None => (data::get_algorithm_coin(&algo_raw, &cx), super::group2::title_case(&jsonx::s(jsonx::path(v, &["info", "coin"])))),
            };
            let cinfo = jsonx::get(&curs, &cur);
            let fee_v = jsonx::get(cinfo, if self.solo { "fee_solo" } else { "fee" });
            let fee = if fee_v.is_null() {
                if self.solo {
                    2.0
                } else {
                    1.0
                }
            } else if let Value::String(s) = fee_v {
                s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect::<String>().replace(',', ".").parse().unwrap_or(0.0)
            } else {
                jsonx::num(fee_v)
            };
            let user = ctx.wallet(&cx).unwrap_or("").to_string();
            let eth = super::eth_mode_default(&algo, "minerproxy");
            let stratums = jsonx::str_list(jsonx::path(v, &["info", "links", "stratums"]));
            let randomx = !stratums.is_empty();
            let mut st = None;
            let mut profit = 0.0;
            if !ctx.info_only {
                if self.solo {
                    let btc = jsonx::num(jsonx::path(v, &["marketStats", "btc"]));
                    let d = jsonx::num(jsonx::get(v, "d"));
                    profit = if btc > 0.0 && d > 0.0 { 86400.0 / 4294967296.0 / d * jsonx::num(jsonx::get(v, "r")) * btc } else { 0.0 };
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, cx), profit, ctx.stat_span);
                    p.difficulty = d;
                    p.quiet = true;
                    st = Some(stat::set_stat(p));
                } else {
                    let hr = jsonx::num(jsonx::get(cinfo, "hashrate"));
                    let blocks = jsonx::num(jsonx::get(cinfo, "24h_blocks"));
                    let reward = if hr > 0.0 { blocks * jsonx::num(jsonx::get(cinfo, "reward")) / hr } else { 0.0 };
                    let usd = jsonx::num(jsonx::path(v, &["marketStats", "usd"]));
                    let btc_price = match rates::get(&cx) {
                        Some(r) => 1.0 / r,
                        None => match rates::get("USD") {
                            Some(u) if usd > 0.0 => usd / u,
                            _ => 0.0,
                        },
                    };
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, cx), reward * btc_price, ctx.stat_span);
                    p.hashrate = hr.trunc();
                    p.blockrate = blocks;
                    p.quiet = true;
                    let s = stat::set_stat(p);
                    if s.hashrate_live == 0.0 && !ctx.allow_zero {
                        continue;
                    }
                    st = Some(s);
                }
            }
            // the lowest difficulty port per tls group
            let port_of = |ssl: bool| -> Option<String> {
                let mut ports: Vec<(String, f64)> = jsonx::entries(jsonx::path(v, &["info", "ports"]))
                    .into_iter()
                    .filter(|(_, pv)| jsonx::truthy(jsonx::get(pv, "tls")) == ssl)
                    .map(|(k, pv)| (k, jsonx::num(jsonx::get(pv, "diff"))))
                    .collect();
                ports.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                ports.first().map(|p| p.0.clone())
            };
            let ports = [port_of(false), port_of(true)];
            for region in regions {
                if randomx && !stratums.iter().any(|s| s == region) {
                    continue;
                }
                for (i, ssl) in [false, true].into_iter().enumerate() {
                    let Some(port) = &ports[i] else { continue };
                    let mut p = ctx.pool(&algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin_name.clone();
                    p.CoinSymbol = cx.clone();
                    p.Currency = cx.clone();
                    p.Protocol = super::protocol(ssl);
                    p.Host = format!("{}.rplant.xyz", if randomx { "randomx".to_string() } else { format!("stratum-{}", region) });
                    p.Port = port.parse().unwrap_or(0);
                    p.User = format!("{}.{}", user, ctx.worker_tag());
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.PoolFee = fee;
                    if self.solo {
                        if profit == 0.0 {
                            p.Price = 0.0;
                            p.StablePrice = 0.0;
                            p.MarginOfError = 0.0;
                        }
                        p.Pass = "m=solo".into();
                        p.Difficulty = st.as_ref().map(|s| s.diff_average);
                        p.SoloMining = true;
                        p.WTM = profit == 0.0;
                    } else {
                        p.Pass = "x".into();
                        p.Workers = Some(jsonx::num(jsonx::get(cinfo, "workers")).trunc());
                        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                        p.TSL = Some(jsonx::num(jsonx::get(cinfo, "timesincelast")).trunc());
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

// ------------------------------------------------------------------------------------------

pub struct Ravenminer {
    pub solo: bool,
}

impl PoolModule for Ravenminer {
    fn name(&self) -> &'static str {
        if self.solo {
            "RavenminerSolo"
        } else {
            "Ravenminer"
        }
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let cur = "RVN";
        let coin = data::get_coin(cur, "").unwrap_or_default();
        let req = match web::Cached::new("https://www.ravenminer.com/api/v1/dashboard").tag(name).cycletime(120).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let mut user: Option<String> = None;
        let mut ae = cur.to_string();
        let mut st = None;
        let mut tsl = None;
        if !ctx.info_only {
            let valid = ["RVN", "BTC", "ETH", "LTC", "BCH", "ADA", "DOGE", "MATIC"];
            let mut ae_cur = ctx.field("AECurrency");
            let prm = ctx.param(cur).unwrap_or("").to_string();
            if ae_cur.is_empty() && !prm.is_empty() {
                if let Some(v) = valid.iter().find(|v| crate::miner::ps_match(&prm, v)) {
                    ae_cur = v.to_string();
                }
            }
            if ae_cur.is_empty() || !valid.iter().any(|v| v.eq_ignore_ascii_case(&ae_cur)) {
                for v in valid {
                    if let Some(w) = ctx.wallet(v) {
                        user = Some(w.to_string());
                        ae = v.to_string();
                        break;
                    }
                }
            } else {
                user = ctx.wallet(&ae_cur).map(|s| s.to_string());
                ae = ae_cur;
            }
            let diff = jsonx::num(jsonx::path(&req, &["coin", "difficulty"]));
            let mut p = SetStat::new(format!("{}_{}_Profit", name, cur), 0.0, ctx.stat_span);
            p.difficulty = diff;
            p.quiet = true;
            if !self.solo {
                let blocks = web::Cached::new("https://www.ravenminer.com/api/v1/blocks").tag(name).cycletime(120).json().unwrap_or_else(|_| {
                    log_warn!("Pool blocks API ({}) has failed. ", name);
                    Value::Null
                });
                let ts = chrono::Utc::now().timestamp_millis() as f64;
                let mut times: Vec<f64> = jsonx::as_list(jsonx::get(&blocks, "blocks"))
                    .iter()
                    .filter(|b| jsonx::num(jsonx::get(b, "payment_type")) == 0.0)
                    .map(|b| jsonx::num(jsonx::get(b, "time")))
                    .collect();
                times.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
                let recent: Vec<f64> = times.iter().cloned().filter(|t| *t > ts - 86400000.0).collect();
                let (mn, mx) = (recent.iter().cloned().fold(f64::INFINITY, f64::min), recent.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
                let blk = ((if recent.len() > 1 && mx - mn > 0.0 { 86400000.0 / (mx - mn) } else { 1.0 }) * recent.len() as f64).round();
                tsl = Some(((ts - times.first().copied().unwrap_or(0.0)) / 1000.0).round());
                p.hashrate = jsonx::num(jsonx::get(&req, "hashrate"));
                p.blockrate = blk;
            }
            let s = stat::set_stat(p);
            if !self.solo && s.hashrate_live == 0.0 && !ctx.allow_zero {
                return Ok(vec![]);
            }
            st = Some(s);
        }
        if user.is_none() && !ctx.info_only {
            return Ok(vec![]);
        }
        let user = user.unwrap_or_default();
        let mut pass = ctx.param(&ae).unwrap_or("").to_string();
        let ael = ae.to_lowercase();
        if ael != "rvn" && !crate::miner::ps_match(&pass, &ael) {
            pass = if pass.is_empty() { ael.clone() } else { format!("{},{}", pass, ael) };
        }
        let (mode_solo, pps, fee) = if self.solo {
            if !crate::miner::ps_match(&pass, "solo") {
                pass = if pass.is_empty() { "solo".into() } else { format!("{},solo", pass) };
            }
            (true, false, jsonx::num(jsonx::get(&req, "solo_fees")))
        } else {
            if pass.is_empty() {
                pass = "x".into();
            }
            let mode = if crate::miner::ps_match(&pass, "pps") {
                "pps"
            } else if crate::miner::ps_match(&pass, "solo") {
                "solo"
            } else {
                "pplns"
            };
            (mode == "solo", mode == "pps", jsonx::num(jsonx::get(&req, &format!("{}_fees", mode))))
        };
        let mut out = Vec::new();
        for region in ["us", "eu", "asia"] {
            for (i, port) in [3838u32, 13838].iter().enumerate() {
                let ssl = i == 1;
                let mut p = ctx.pool(&coin.algo).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = coin.name.clone();
                p.CoinSymbol = cur.into();
                p.Currency = cur.into();
                p.Protocol = super::protocol(ssl);
                p.Host = format!("{}.ravenminer.com", region);
                p.Port = *port;
                p.User = format!("{}.{}", user, ctx.worker_tag());
                p.Pass = pass.clone();
                p.Region = data::get_region(region);
                p.SSL = ssl;
                p.PoolFee = fee;
                if mode_solo {
                    p.Difficulty = st.as_ref().map(|s| s.diff_average);
                    p.SoloMining = true;
                } else {
                    p.Workers = jsonx::num_opt(jsonx::get(&req, "workersNum"));
                    p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                    p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    p.TSL = tsl;
                }
                p.WTM = !self.solo;
                p.PaysLive = pps;
                p.ErrorRatio = st.as_ref().map(|s| s.error_ratio).unwrap_or(0.0);
                p.EthMode = "stratum".into();
                p.Wallet = ctx.wallet(cur).unwrap_or("").to_string();
                out.push(p);
            }
        }
        Ok(out)
    }
}

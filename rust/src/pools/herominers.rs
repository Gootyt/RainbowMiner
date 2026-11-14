//! Pools/HeroMiners.ps1 (cryptonote-pool API, prices from WhatToMine)

use crate::data;
use crate::jsonx;
use crate::pool::{pool_ports_from_request, wallet_with_payment_id, Pool, PoolCtx, PoolModule};
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use crate::log_warn;
use anyhow::Result;

pub struct HeroMiners;

struct Coin {
    symbol: &'static str,
    port: u32,
    fee: f64,
    rpc: &'static str,
    pass: Option<&'static str>,
    diff_factor: f64,
}

const COINS: &[Coin] = &[
    Coin { symbol: "ALPH", port: 1220, fee: 0.0, rpc: "alephium", pass: Some("x"), diff_factor: 1.0 },
    Coin { symbol: "BEAM", port: 1130, fee: 0.9, rpc: "beam", pass: None, diff_factor: 1.0 },
    Coin { symbol: "CFX", port: 1170, fee: 0.9, rpc: "conflux", pass: None, diff_factor: 1.0 },
    Coin { symbol: "ERG", port: 1180, fee: 0.9, rpc: "ergo", pass: None, diff_factor: 1.0 },
    Coin { symbol: "ETC", port: 1150, fee: 0.9, rpc: "etc", pass: None, diff_factor: 1.0 },
    Coin { symbol: "IRON", port: 1145, fee: 0.0, rpc: "ironfish", pass: Some("x"), diff_factor: 1.0 },
    Coin { symbol: "KAS", port: 1206, fee: 0.9, rpc: "kaspa", pass: None, diff_factor: 1.0 },
    Coin { symbol: "XMR", port: 1111, fee: 0.9, rpc: "monero", pass: None, diff_factor: 1.0 },
    Coin { symbol: "PRL", port: 1200, fee: 0.0, rpc: "pearl", pass: None, diff_factor: 1.0 },
    Coin { symbol: "QRL", port: 1166, fee: 0.9, rpc: "qrl", pass: None, diff_factor: 1.0 },
    Coin { symbol: "QUAI", port: 1185, fee: 0.0, rpc: "quai", pass: None, diff_factor: 1.0 },
    Coin { symbol: "RVN", port: 1140, fee: 0.9, rpc: "ravencoin", pass: None, diff_factor: 4294967296.0 },
    Coin { symbol: "SAL", port: 1230, fee: 0.9, rpc: "salvium", pass: None, diff_factor: 1.0 },
    Coin { symbol: "WART", port: 1143, fee: 0.9, rpc: "warthog", pass: None, diff_factor: 1.0 },
    Coin { symbol: "XEL", port: 1225, fee: 0.0, rpc: "xelis", pass: None, diff_factor: 1.0 },
    Coin { symbol: "ZANO", port: 1110, fee: 0.9, rpc: "zano", pass: None, diff_factor: 1.0 },
    Coin { symbol: "ZEPH", port: 1123, fee: 0.9, rpc: "zephyr", pass: None, diff_factor: 1.0 },
];

const REGIONS: &[&str] = &["de", "fr", "es", "fi", "ru", "ca", "us", "us2", "us3", "mx", "br", "kz", "hk", "kr", "in", "sg", "tr", "au"];

fn region_name(r: &str) -> String {
    let x = match r {
        "us" => "uswest",
        "us2" => "useast",
        "us3" => "ussouth",
        o => o,
    };
    data::get_region(x)
}

impl PoolModule for HeroMiners {
    fn name(&self) -> &'static str {
        "HeroMiners"
    }

    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let mut out = Vec::new();
        for c in COINS {
            let currency = c.symbol;
            if ctx.wallet(currency).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(c.symbol, "").unwrap_or_default();
            let algo_norm = coin.algo.clone();
            if algo_norm.is_empty() || !ctx.algo_allowed(&algo_norm) {
                continue;
            }
            let eth_mode = super::eth_mode_default(&algo_norm, "ethproxy");
            let password = c.pass.map(|p| p.to_string()).unwrap_or_else(|| ctx.worker_tag());
            let user_wn = if c.pass.is_some() { format!(".{}", ctx.worker_tag()) } else { String::new() };
            let req = match web::Cached::new(&format!("https://{}.herominers.com/api/stats", c.rpc)).tag(name).timeout(15).cycletime(120).json() {
                Ok(v) => v,
                Err(_) => {
                    log_warn!("Pool API ({}) for {} has failed. ", name, currency);
                    continue;
                }
            };
            let ports = pool_ports_from_request(&req, "", "(modern|mid)", "(higher|high-end|cloud|nicehash)", "");
            if ports.iter().all(|p| p.is_none()) {
                continue;
            }
            let cfg = jsonx::get(&req, "config");
            let mut fee = c.fee;
            let mut st = None;
            let mut solo = false;
            let mut workers = None;
            let mut blk_rate = None;
            let mut tsl = None;
            let wallet_raw = ctx.wallet(currency).unwrap_or("").to_string();
            if !ctx.info_only {
                fee = jsonx::num(jsonx::get(cfg, "fee"));
                let prefix = jsonx::s(jsonx::get(cfg, "soloMiningPrefix"));
                if !prefix.is_empty() && wallet_raw.starts_with(&prefix) {
                    let net = jsonx::get(&req, "network");
                    let _blk_time = if jsonx::truthy(jsonx::get(cfg, "avgDifficultyTargetEnabled")) {
                        jsonx::num(jsonx::get(net, "difficultyTarget"))
                    } else {
                        jsonx::num(jsonx::get(cfg, "coinDifficultyTarget"))
                    };
                    solo = true;
                    let diff = jsonx::num(jsonx::get(net, "difficulty")) * c.diff_factor / 4294967296.0;
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, c.symbol), 0.0, ctx.stat_span);
                    p.difficulty = diff;
                    p.quiet = true;
                    st = Some(stat::set_stat(p));
                } else {
                    let ts = util::unix_timestamp();
                    let ts24 = ts - 86400;
                    let pool = jsonx::get(&req, "pool");
                    workers = Some(jsonx::num(jsonx::get(pool, "workers")).trunc());
                    let hashrate = jsonx::num(jsonx::get(pool, "hashrate"));
                    let re = regex::Regex::new(r"^[^:]*:.*?(\d{10}):").unwrap();
                    let blocks: Vec<i64> = jsonx::as_list(jsonx::get(pool, "blocks"))
                        .into_iter()
                        .filter_map(|b| re.captures(&jsonx::s(b)).and_then(|c| c[1].parse::<i64>().ok()))
                        .filter(|t| *t > ts24)
                        .collect();
                    let blk = if blocks.len() > 1 {
                        let span = (blocks.iter().max().unwrap() - blocks.iter().min().unwrap()) as f64;
                        if span > 0.0 {
                            (86400.0 / span * blocks.len() as f64).trunc()
                        } else {
                            blocks.len() as f64
                        }
                    } else {
                        blocks.len() as f64
                    };
                    tsl = Some((ts as f64 - jsonx::num(jsonx::path(pool, &["stats", "lastBlockFound"])) / 1000.0).trunc());
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, c.symbol), 0.0, ctx.stat_span);
                    p.hashrate = hashrate;
                    p.blockrate = blk;
                    p.quiet = true;
                    let r = stat::set_stat(p);
                    if r.hashrate_live == 0.0 && !ctx.allow_zero {
                        continue;
                    }
                    blk_rate = Some(r.blockrate_average);
                    st = Some(r);
                }
            }
            let (wallet, _pid, wdiff) = wallet_with_payment_id(&wallet_raw, "+");
            let fixed = jsonx::truthy(jsonx::get(cfg, "fixedDiffEnabled"));
            let sep = jsonx::s(jsonx::get(cfg, "fixedDiffSeparator"));
            let diffpart = if fixed {
                if !wdiff.is_empty() {
                    format!("{}{}", sep, wdiff)
                } else {
                    format!("{{diff:{}$difficulty}}", sep)
                }
            } else {
                String::new()
            };
            for pp in ports.iter().take(1).flatten() {
                for region in REGIONS {
                    let mut p = ctx.pool(&algo_norm);
                    if let Some(s) = &st {
                        p.Updated = s.updated;
                    }
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = currency.into();
                    p.Currency = currency.into();
                    p.Protocol = "stratum+tcp".into();
                    p.Host = format!("{}.{}.herominers.com", region, c.rpc);
                    p.Port = if pp.CPU > 0 { pp.CPU } else { c.port };
                    p.Ports = if pp.CPU > 0 { Some(pp.clone()) } else { None };
                    p.User = format!("{}{}{}", wallet, diffpart, user_wn);
                    p.Pass = password.clone();
                    p.Region = region_name(region);
                    p.PoolFee = fee;
                    p.Workers = if solo { None } else { workers };
                    p.Hashrate = if solo { None } else { st.as_ref().map(|s| s.hashrate_live) };
                    p.BLK = if solo { None } else { blk_rate };
                    p.TSL = if solo { None } else { tsl };
                    p.Difficulty = if solo { st.as_ref().map(|s| s.diff_average) } else { None };
                    p.SoloMining = solo;
                    p.WTM = true;
                    p.EthMode = eth_mode.clone();
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

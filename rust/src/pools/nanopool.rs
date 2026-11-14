//! Pools/Nanopool.ps1

use crate::data;
use crate::jsonx;
use crate::pool::{wallet_with_payment_id, Pool, PoolCtx, PoolModule};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use crate::log_info;
use anyhow::Result;

pub struct Nanopool;

struct Coin {
    symbol: &'static str,
    rpc: &'static str,
    ports: [u32; 2],
    fee: f64,
    divisor: f64,
    use_email: bool,
}

const COINS: &[Coin] = &[
    Coin { symbol: "XMR", rpc: "xmr", ports: [10300, 10343], fee: 1.0, divisor: 1.0, use_email: true },
    Coin { symbol: "RVN", rpc: "rvn", ports: [10400, 10443], fee: 1.0, divisor: 1e6, use_email: true },
    Coin { symbol: "CFX", rpc: "cfx", ports: [10500, 10543], fee: 1.0, divisor: 1e6, use_email: true },
    Coin { symbol: "ERG", rpc: "ergo", ports: [10600, 10643], fee: 1.0, divisor: 1e6, use_email: true },
];

const REGIONS: &[&str] = &["asia", "eu", "jp", "us-east", "us-west", "au"];

impl PoolModule for Nanopool {
    fn name(&self) -> &'static str {
        "Nanopool"
    }

    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let email = ctx.field("Email");
        let mut out = Vec::new();
        for c in COINS {
            let currency = c.symbol;
            if ctx.wallet(currency).is_none() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(currency, "").unwrap_or_default();
            let algo_norm = coin.algo.clone();
            if algo_norm.is_empty() || !ctx.algo_allowed(&algo_norm) {
                continue;
            }
            let (wallet, _, _) = wallet_with_payment_id(ctx.wallet(currency).unwrap_or(""), ".");
            let eth_mode = super::eth_mode_default(&algo_norm, "ethproxy");
            let mut use_wtm = false;
            let mut st = None;
            let mut tsl = None;
            let mut workers = None;
            let mut hashrate = None;
            let mut blk = None;
            let mut updated = chrono::Utc::now();
            if !ctx.info_only {
                let api = |p: &str| web::Cached::new(&format!("https://api.nanopool.org/v1/{}/{}", c.rpc, p)).tag(name).cycletime(120).retry(5).json();
                let earn = api("approximated_earnings/1000");
                match &earn {
                    Ok(v) if jsonx::truthy(jsonx::get(v, "status")) => {}
                    _ => use_wtm = true,
                }
                if use_wtm {
                    log_info!("Pool API ({}) profitability for {} has failed. Using WhatToMine fallback. ", name, currency);
                }
                let w = api("pool/activeworkers").unwrap_or_default();
                let h = api("pool/hashrate").unwrap_or_default();
                let b = api("pool/count_blocks/24").unwrap_or_default();
                let l = api("pool/recentblocks/1").unwrap_or_default();
                let last = if jsonx::s(jsonx::get(&l, "status")).eq_ignore_ascii_case("true") && jsonx::count(jsonx::get(&l, "data")) == 1 {
                    jsonx::num(jsonx::get(jsonx::idx(jsonx::get(&l, "data"), 0), "date"))
                } else {
                    0.0
                };
                tsl = Some(util::unix_timestamp() as f64 - last);
                workers = jsonx::num_opt(jsonx::get(&w, "data"));
                let hr = jsonx::num(jsonx::get(&h, "data")) * c.divisor;
                let blocks = jsonx::num(jsonx::path(&b, &["data", "count"]));
                if use_wtm {
                    hashrate = Some(hr);
                    blk = Some(blocks);
                } else {
                    let e = earn.unwrap_or_default();
                    let day = jsonx::path(&e, &["data", "day"]);
                    let expected = match rates::get(currency) {
                        Some(r) => jsonx::num(jsonx::get(day, "coins")) / r,
                        None => jsonx::num(jsonx::get(day, "bitcoins")),
                    } / c.divisor
                        / 1000.0;
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, currency), expected, ctx.stat_span);
                    p.hashrate = hr;
                    p.blockrate = blocks;
                    p.change_detection = true;
                    p.quiet = true;
                    let r = stat::set_stat(p);
                    hashrate = Some(r.hashrate_live);
                    blk = Some(r.blockrate_average);
                    updated = r.updated;
                    st = Some(r);
                }
                if hashrate.unwrap_or(0.0) == 0.0 && !ctx.allow_zero {
                    continue;
                }
            }
            for region in REGIONS {
                for (i, port) in c.ports.iter().enumerate() {
                    let ssl = i > 0;
                    let mut p = ctx.pool(&algo_norm);
                    if !use_wtm {
                        p = p.with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    }
                    p.Updated = updated;
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = currency.into();
                    p.Currency = currency.into();
                    p.Protocol = super::protocol(ssl);
                    p.Host = format!("{}-{}1.nanopool.org", c.rpc, region);
                    p.Port = *port;
                    p.User = format!("{}.{}{}", wallet, ctx.worker_tag(), if c.use_email && !email.is_empty() { format!("/{}", email) } else { String::new() });
                    p.Pass = ctx.param(currency).unwrap_or("x").to_string();
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.PoolFee = c.fee;
                    p.Workers = workers;
                    p.Hashrate = hashrate;
                    p.TSL = tsl;
                    p.BLK = blk;
                    p.EthMode = eth_mode.clone();
                    p.WTM = use_wtm;
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

//! Pools/2Miners.ps1, 2MinersSolo.ps1, 2MinersAE.ps1

use crate::data;
use crate::jsonx;
use crate::log_warn;
use crate::pool::{wallet_with_payment_id, Pool, PoolCtx, PoolModule};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::web;
use anyhow::Result;
use serde_json::Value;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    /// 2Miners: PPLNS
    Pplns,
    /// 2MinersSolo: solo-<coin>.2miners.com, difficulty from the nodes
    Solo,
    /// 2MinersAE: auto exchange into one of the coin's currencies
    AutoExchange,
}

pub struct TwoMiners {
    pub mode: Mode,
}

struct Coin {
    rpc: &'static str,
    symbol: &'static str,
    port: u32,
    divisor: f64,
    ssl: bool,
    cycles: f64,
    currencies: &'static [&'static str],
}

const fn c(rpc: &'static str, symbol: &'static str, port: u32, divisor: f64) -> Coin {
    Coin { rpc, symbol, port, divisor, ssl: false, cycles: 0.0, currencies: &[] }
}

const COINS: &[Coin] = &[
    c("ae", "AE", 4040, 1e8),
    Coin { ssl: true, ..c("beam", "BEAM", 5252, 1e8) },
    c("btg", "BTG", 4040, 1e8),
    c("ckb", "CKB", 6464, 1e8),
    c("ctxc", "CTXC", 2222, 1e18),
    c("erg", "ERG", 8888, 1e9),
    c("etc", "ETC", 1010, 1e18),
    c("ethw", "ETHW", 2020, 1e18),
    Coin { cycles: 42.0, ..c("grin", "GRIN-PRI", 3030, 1e9) },
    c("kas", "KAS", 2020, 1e8),
    Coin { cycles: 42.0, ..c("mwc", "MWC-PRI", 7575, 1e9) },
    c("neox", "NEOX", 4040, 1e8),
    c("nexa", "NEXA", 5050, 100.0),
    c("rvn", "RVN", 6060, 1e8),
    c("xna", "XNA", 6060, 1e8),
    c("zec", "ZEC", 1010, 1e8),
    c("zeph", "ZEPH", 2222, 1e12),
];

const COINS_SOLO: &[Coin] = &[
    c("ae", "AE", 4141, 1e8),
    Coin { ssl: true, ..c("beam", "BEAM", 5454, 1e8) },
    c("btg", "BTG", 4040, 1e8),
    c("ckb", "CKB", 6767, 1e8),
    c("ctxc", "CTXC", 6666, 1e18),
    c("erg", "ERG", 9999, 1e9),
    c("etc", "ETC", 5050, 1e18),
    c("ethw", "ETHW", 6060, 1e18),
    Coin { cycles: 42.0, ..c("grin", "GRIN-PRI", 4040, 1e9) },
    c("kas", "KAS", 3030, 1e8),
    Coin { cycles: 42.0, ..c("mwc", "MWC-PRI", 8585, 1e9) },
    c("neox", "NEOX", 5050, 1e8),
    c("nexa", "NEXA", 7070, 100.0),
    c("rvn", "RVN", 7070, 1e8),
    c("xna", "XNA", 7070, 1e8),
    c("zec", "ZEC", 1010, 1e8),
    c("zeph", "ZEPH", 4444, 1e12),
];

const COINS_AE: &[Coin] = &[
    Coin { currencies: &["BTC", "TON", "ERG"], ..c("erg", "ERG", 8888, 1e9) },
    Coin { currencies: &["BTC", "TON", "ETC"], ..c("etc", "ETC", 1010, 1e18) },
    Coin { currencies: &["BTC", "TON", "ETHW"], ..c("ethw", "ETHW", 2020, 1e18) },
    Coin { currencies: &["BTC", "TON", "KAS"], ..c("kas", "KAS", 2020, 1e8) },
    Coin { currencies: &["BTC", "NEXA"], ..c("nexa", "NEXA", 5050, 100.0) },
    Coin { currencies: &["BTC", "TON", "RVN"], ..c("rvn", "RVN", 6060, 1e8) },
    Coin { currencies: &["BTC", "TON", "XNA"], ..c("xna", "XNA", 6060, 1e8) },
    Coin { currencies: &["BTC", "TON", "ZEC"], ..c("zec", "ZEC", 1010, 1e8) },
];

impl PoolModule for TwoMiners {
    fn name(&self) -> &'static str {
        match self.mode {
            Mode::Pplns => "2Miners",
            Mode::Solo => "2MinersSolo",
            Mode::AutoExchange => "2MinersAE",
        }
    }

    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let solo = self.mode == Mode::Solo;
        let host_status = match web::Cached::new("https://status-api.2miners.com/").tag(name).cycletime(120).retry(5).json() {
            Ok(v) if jsonx::get(&v, "code").is_null() => v,
            _ => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let hosts: Vec<&Value> = jsonx::as_list(&host_status);
        let coins = match self.mode {
            Mode::Pplns => COINS,
            Mode::Solo => COINS_SOLO,
            Mode::AutoExchange => COINS_AE,
        };
        let mut ae = ctx.field("AECurrency");
        let mut out = Vec::new();
        for c in coins {
            let coin_symbol = c.symbol.split('-').next().unwrap_or(c.symbol);
            // the currencies (and wallets) the pools are paid out in
            let currencies: Vec<String> = if self.mode == Mode::AutoExchange {
                let mut cur: Vec<String> = c.currencies.iter().filter(|x| ctx.wallet(x).is_some() || ctx.info_only).map(|x| x.to_string()).collect();
                if !ctx.info_only && cur.len() > 1 {
                    if ae.is_empty() || !cur.contains(&ae) {
                        ae = cur[0].clone();
                    }
                    cur.retain(|x| *x == ae);
                }
                cur
            } else if ctx.wallet(coin_symbol).is_some() || ctx.info_only {
                vec![coin_symbol.to_string()]
            } else {
                vec![]
            };
            if currencies.is_empty() && !ctx.info_only {
                continue;
            }
            let coin = data::get_coin(c.symbol, "").unwrap_or_default();
            let algo_norm = coin.algo.clone();
            if algo_norm.is_empty() || !ctx.algo_allowed(&algo_norm) {
                continue;
            }
            let host = if solo { format!("solo-{}.2miners.com", c.rpc) } else { format!("{}.2miners.com", c.rpc) };
            let eth_mode = super::eth_mode_default(&algo_norm, "ethproxy");
            let host_ok = |hs: &str| (solo || !hs.contains("solo-")) && hs.contains(&host);
            if !hosts.iter().any(|h| host_ok(&jsonx::s(jsonx::get(h, "host")))) {
                continue;
            }
            let mut st = None;
            let (mut tsl, mut workers) = (None, None);
            if !ctx.info_only {
                let blocks = web::Cached::new(&format!("https://{}.2miners.com/api/blocks", c.rpc)).tag(name).cycletime(120).delay(250).retry(5).json();
                let stats = web::Cached::new(&format!("https://{}/api/stats", host)).tag(name).cycletime(120).delay(250).retry(5).json();
                let (Ok(blocks), Ok(req)) = (blocks, stats) else {
                    log_warn!("Pool API ({}) for {} has failed. ", name, coin_symbol);
                    continue;
                };
                if !jsonx::get(&blocks, "code").is_null() || !jsonx::get(&req, "code").is_null() || !jsonx::truthy(jsonx::get(&req, "nodes")) {
                    log_warn!("Pool API ({}) for {} has failed. ", name, coin_symbol);
                    continue;
                }
                let timestamp = (jsonx::num(jsonx::get(&req, "now")) / 1000.0).round();
                let ts24 = timestamp - 86400.0;
                let mut bl: Vec<(f64, f64)> = Vec::new();
                for k in ["candidates", "immature", "matured"] {
                    for b in jsonx::as_list(jsonx::get(&blocks, k)) {
                        let t = jsonx::num(jsonx::get(b, "timestamp"));
                        if t > ts24 {
                            bl.push((t, jsonx::num(jsonx::get(b, "reward"))));
                        }
                    }
                }
                let avg_time = if bl.len() > 1 {
                    let mn = bl.iter().map(|b| b.0).fold(f64::INFINITY, f64::min);
                    let mx = bl.iter().map(|b| b.0).fold(f64::NEG_INFINITY, f64::max);
                    (mx - mn) / (bl.len() - 1) as f64
                } else {
                    timestamp
                };
                let blk = if avg_time != 0.0 { (86400.0 / avg_time).round() } else { 0.0 };
                let rewards: Vec<f64> = bl.iter().map(|b| b.1).filter(|r| *r > 0.0).collect();
                let reward = if rewards.is_empty() { 0.0 } else { rewards.iter().sum::<f64>() / rewards.len() as f64 } / c.divisor;
                let price_sym = if self.mode == Mode::AutoExchange { coin.symbol.as_str() } else { coin_symbol };
                let btc_price = rates::btc_price(price_sym);
                // 2MinersAE: a coin without any rate gets a zero rate, the stat starts over once it is known
                let reset_if_zero = self.mode == Mode::AutoExchange && btc_price == 0.0 && !rates::all().contains_key(&price_sym.to_uppercase());
                if reset_if_zero {
                    rates::set(price_sym, 0.0);
                }
                let nodes = jsonx::get(&req, "nodes");
                let node = jsonx::idx(nodes, 0);
                let mut hashrate = jsonx::num(jsonx::get(&req, "hashrate"));
                let btc_live = if c.cycles > 0.0 {
                    let add: String = algo_norm.chars().filter(|x| x.is_ascii_digit()).collect();
                    // $Pool_Request.nodes.<field>: the member enumeration of the (single) node
                    let weight = if c.symbol.ends_with("-PRI") {
                        jsonx::num(jsonx::get(node, &format!("primaryWeight{}", add)))
                    } else {
                        jsonx::num(jsonx::get(node, "secondaryScale"))
                    };
                    let diff = jsonx::num(jsonx::get(node, "difficulty"));
                    hashrate = jsonx::num(jsonx::path(&req, &["hashrates", &add]));
                    if diff != 0.0 {
                        (86400.0 / c.cycles) * (weight / diff) * reward * btc_price
                    } else {
                        0.0
                    }
                } else if hashrate > 0.0 {
                    btc_price * reward * 86400.0 / avg_time / hashrate
                } else {
                    0.0
                };
                let mut p = SetStat::new(format!("{}_{}_Profit", name, c.symbol), btc_live, ctx.stat_span);
                p.quiet = true;
                if solo {
                    p.difficulty = jsonx::num(jsonx::get(node, "networkhashps")) * jsonx::num(jsonx::get(node, "avgBlockTime")) / 4294967296.0;
                } else {
                    p.hashrate = hashrate;
                    p.blockrate = blk;
                    p.reset_if_zero = reset_if_zero;
                }
                let r = stat::set_stat(p);
                // like the original, 2MinersSolo needs AllowZero, since a solo stat has no hashrate
                if r.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                tsl = Some(timestamp - jsonx::num(jsonx::path(&req, &["stats", "lastBlockFound"])));
                workers = jsonx::num_opt(jsonx::get(&req, "workersTotal"));
                st = Some(r);
            }
            let mut sel: Vec<(String, u32)> = hosts
                .iter()
                .map(|h| (jsonx::s(jsonx::get(h, "host")), jsonx::num(jsonx::get(h, "port")) as u32))
                .filter(|(h, _)| host_ok(h))
                .collect();
            sel.sort_by_key(|h| h.1);
            if coin.symbol == "FIRO" {
                sel.reverse();
            }
            for cur in &currencies {
                let (wallet, _, _) = wallet_with_payment_id(ctx.wallet(cur).unwrap_or(""), ".");
                let mut seen: Vec<String> = Vec::new();
                for (h, port) in &sel {
                    let ssl = c.ssl || *port >= 10000;
                    let k = format!("{}{}", h, ssl);
                    if seen.contains(&k) {
                        continue;
                    }
                    seen.push(k);
                    let region = if h.starts_with("asia-") {
                        "asia"
                    } else if h.starts_with("us-") {
                        "us"
                    } else {
                        "eu"
                    };
                    let mut p = ctx.pool(&algo_norm).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                    p.CoinName = coin.name.clone();
                    p.CoinSymbol = if self.mode == Mode::AutoExchange { coin.symbol.clone() } else { coin_symbol.to_string() };
                    p.Currency = cur.clone();
                    p.Protocol = super::protocol(ssl);
                    p.Host = h.clone();
                    p.Port = if *port > 0 { *port } else { c.port };
                    p.User = format!("{}.{}", wallet, ctx.worker_tag());
                    p.Pass = "x".into();
                    p.Region = data::get_region(region);
                    p.SSL = ssl;
                    p.PoolFee = 1.0;
                    if solo {
                        p.Difficulty = st.as_ref().map(|s| s.diff_average);
                        p.SoloMining = true;
                    } else {
                        p.Workers = workers;
                        p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                        p.TSL = tsl;
                        p.BLK = st.as_ref().map(|s| s.blockrate_average);
                    }
                    p.EthMode = eth_mode.clone();
                    p.Wallet = wallet.clone();
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

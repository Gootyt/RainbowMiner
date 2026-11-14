//! Pools/unMineable.ps1

use crate::data;
use crate::jsonx;
use crate::pool::{Pool, PoolCtx, PoolModule};
use crate::rates;
use crate::stat::{self, SetStat};
use crate::web;
use crate::log_warn;
use anyhow::Result;
use serde_json::json;

pub struct UnMineable;

struct Algo {
    algo: &'static str,
    coin: &'static str,
    ethproxy: &'static str,
    rpc: &'static str,
    divisor: f64,
    mh: f64,
    regions: &'static [&'static str],
    rewardalgo: Option<&'static str>,
}

const R4: &[&str] = &["us", "ca", "eu", "asia"];
const R3: &[&str] = &["us", "eu", "asia"];

const ALGOS: &[Algo] = &[
    Algo { algo: "autolykos", coin: "ERG", ethproxy: "", rpc: "autolykos", divisor: 1e6, mh: 1e6, regions: R4, rewardalgo: None },
    Algo { algo: "beamhash", coin: "BEAM", ethproxy: "", rpc: "beamhash", divisor: 1.0, mh: 1e6, regions: R4, rewardalgo: None },
    Algo { algo: "blake3alephium", coin: "ALPH", ethproxy: "", rpc: "blake3", divisor: 1.0, mh: 1e15, regions: R3, rewardalgo: Some("blake3") },
    Algo { algo: "equihash", coin: "ZEC", ethproxy: "", rpc: "equihash", divisor: 1.0, mh: 1e9, regions: R3, rewardalgo: None },
    Algo { algo: "etchash", coin: "ETC", ethproxy: "ethstratumnh", rpc: "etchash", divisor: 1e6, mh: 1e6, regions: R4, rewardalgo: None },
    Algo { algo: "ethash", coin: "ETHW", ethproxy: "ethstratumnh", rpc: "ethash", divisor: 1e6, mh: 1e6, regions: R4, rewardalgo: None },
    Algo { algo: "fishhash", coin: "IRON", ethproxy: "stratum", rpc: "fishhash", divisor: 1.0, mh: 1e12, regions: R3, rewardalgo: None },
    Algo { algo: "kheavyhash", coin: "KAS", ethproxy: "", rpc: "kheavyhash", divisor: 1.0, mh: 1e15, regions: R3, rewardalgo: None },
    Algo { algo: "nexapow", coin: "NEXA", ethproxy: "", rpc: "nexapow", divisor: 1.0, mh: 1e12, regions: R3, rewardalgo: None },
    Algo { algo: "octopus", coin: "CFX", ethproxy: "", rpc: "octopus", divisor: 1.0, mh: 1e12, regions: R3, rewardalgo: None },
    Algo { algo: "pearlhash", coin: "PRL", ethproxy: "", rpc: "pearlpow", divisor: 1.0, mh: 1e18, regions: R3, rewardalgo: Some("pearlpow") },
    Algo { algo: "randomx", coin: "XMR", ethproxy: "", rpc: "rx", divisor: 1.0, mh: 1e9, regions: R3, rewardalgo: None },
    Algo { algo: "xelishashv3", coin: "XEL", ethproxy: "", rpc: "xelishash", divisor: 1.0, mh: 1e9, regions: R3, rewardalgo: Some("xelishash") },
];

/// the referral code of the original RainbowMiner module (lowers the pool fee to 0.75%)
const REFERRAL: &str = "U-TEMDPF";

impl PoolModule for UnMineable {
    fn name(&self) -> &'static str {
        "unMineable"
    }

    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let coins = match web::Cached::new("https://api.unmineable.com/v4/coin").tag(name).cycletime(21600).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        if !jsonx::truthy(jsonx::get(&coins, "success")) {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let user = ctx.field("User");
        let currencies: Vec<(String, String)> = jsonx::as_list(jsonx::get(&coins, "data"))
            .into_iter()
            .map(|c| (jsonx::s(jsonx::get(c, "symbol")), jsonx::s(jsonx::get(c, "name"))))
            .filter(|(s, _)| ctx.wallet(s).is_some() || (s == "EOS" && ctx.wallet("A").is_some() && ctx.wallet("EOS").is_none()) || ctx.info_only)
            .collect();
        let mut out = Vec::new();
        for a in ALGOS {
            let reward_algo = a.rewardalgo.unwrap_or(a.algo);
            let algo_norm = if (a.algo == "ethash" || a.algo == "kawpow") && !a.coin.is_empty() {
                data::get_algorithm_coin(a.algo, a.coin)
            } else {
                data::get_algorithm(a.algo)
            };
            if !ctx.algo_allowed(&algo_norm) {
                continue;
            }
            let (mut dag_max, mut sym_max) = (None, String::new());
            if data::RE_ALGO_HAS_DAGSIZE.is_match(&algo_norm) {
                dag_max = Some(data::get_eth_dag_size(a.coin, &algo_norm, 1.0));
                sym_max = a.coin.to_string();
            }
            let mk = |cur: &str, coin_name: &str, coin_sym: &str, wallet: &str, st: &Option<stat::Stat>, out: &mut Vec<Pool>| {
                for (i, port) in [3333u32, 4444].iter().enumerate() {
                    let ssl = i > 0;
                    for region in a.regions {
                        let mut p = ctx.pool(&algo_norm).with_stat(st, &ctx.stat_average, &ctx.stat_average_stable);
                        p.CoinName = coin_name.to_string();
                        p.CoinSymbol = coin_sym.to_string();
                        p.Currency = cur.to_string();
                        p.Protocol = super::protocol(ssl);
                        p.Host = format!("{}{}.unmineable.com", a.rpc, if a.regions.len() > 1 { format!("-{}", region) } else { String::new() });
                        p.Port = *port;
                        p.User = wallet.to_string();
                        p.Region = data::get_region(region);
                        p.SSL = ssl;
                        p.PoolFee = 0.75;
                        p.PaysLive = true;
                        p.EthMode = a.ethproxy.to_string();
                        p.CoinSymbolMax = sym_max.clone();
                        p.DagSizeMax = dag_max;
                        p.Wallet = wallet.to_string();
                        out.push(p);
                    }
                }
            };
            for (cur, cname) in &currencies {
                let mut st = None;
                if !ctx.info_only {
                    let req = web::Request::get("https://api.unminable.com/v3/calculate/reward").post_json(&json!({"algo": reward_algo, "coin": cur, "mh": a.mh}));
                    let r = web::Cached::new("").request(req).tag(name).cycletime(240).json();
                    let Ok(r) = r else {
                        log_warn!("Pool profit API ({}) has failed for coin {}. ", name, cur);
                        continue;
                    };
                    if jsonx::s(jsonx::get(&r, "algo")) != reward_algo {
                        continue;
                    }
                    let price = rates::btc_price(cur) * jsonx::num(jsonx::get(&r, "per_day")) / a.mh / a.divisor;
                    let mut p = SetStat::new(format!("{}_{}_{}_Profit", name, algo_norm, cur), price, ctx.stat_span);
                    p.change_detection = price > 0.0;
                    p.quiet = true;
                    st = Some(stat::set_stat(p));
                }
                let wallet = format!("{}:{}.{}#{}", cur, ctx.wallet(cur).unwrap_or(""), ctx.worker_tag(), REFERRAL);
                mk(cur, cname, cur, &wallet, &st, &mut out);
            }
            if !user.is_empty() || ctx.info_only {
                let mut st = None;
                if !ctx.info_only {
                    let hs = a.mh * a.divisor;
                    let body = json!({"mode":"advanced","coin":"BTC","algorithm":reward_algo,"hashrate_hs":format!("{}", hs),"scenario_set":"default","referral_discount":true});
                    let req = web::Request::get("https://api.unmineable.dev/v1/simulator").post_json(&body);
                    let Ok(r) = web::Cached::new("").request(req).tag(name).cycletime(240).json() else {
                        log_warn!("Pool profit API ({}) has failed for algorithm {}. ", name, algo_norm);
                        continue;
                    };
                    if !jsonx::truthy(jsonx::get(&r, "success")) {
                        continue;
                    }
                    let mut price = 0.0;
                    if let Some(al) = jsonx::as_list(jsonx::path(&r, &["data", "algorithms"])).into_iter().find(|x| jsonx::s(jsonx::get(x, "algorithm")) == reward_algo) {
                        let btc_usdt = jsonx::num(jsonx::path(al, &["assumptions", "btc_price_usdt"]));
                        if btc_usdt > 0.0 {
                            price = jsonx::num(jsonx::path(al, &["windows", "1d", "scenarios", "spot_now", "comparison", "auto_convert_usdt"])) / hs / btc_usdt;
                        }
                    }
                    let mut p = SetStat::new(format!("{}_{}_Profit", name, algo_norm), price, ctx.stat_span);
                    p.change_detection = price > 0.0;
                    p.quiet = true;
                    st = Some(stat::set_stat(p));
                }
                let wallet = format!("{}.{}#{}", user, ctx.worker_tag(), REFERRAL);
                mk("BTC", "", "", &wallet, &st, &mut out);
            }
        }
        Ok(out)
    }
}

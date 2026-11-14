//! Pools/NiceHash.ps1

use crate::data;
use crate::jsonx;
use crate::pool::{Pool, PoolCtx, PoolModule};
use crate::stat::{self, SetStat};
use crate::web;
use crate::log_warn;
use anyhow::Result;

pub struct NiceHash;

impl PoolModule for NiceHash {
    fn name(&self) -> &'static str {
        "NiceHash"
    }

    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let wallet = ctx.wallet("BTC").unwrap_or("").to_string();
        let (req, mining) = match (
            web::Cached::new("https://api2.nicehash.com/main/api/v2/public/stats/global/current").tag(name).timeout(20).cycletime(60).json(),
            web::Cached::new("https://api2.nicehash.com/main/api/v2/mining/algorithms/").tag(name).timeout(20).cycletime(3600).json(),
        ) {
            (Ok(a), Ok(b)) => (a, b),
            _ => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        let algos = jsonx::as_list(jsonx::get(&req, "algos"));
        let malgos = jsonx::as_list(jsonx::get(&mining, "miningAlgorithms"));
        if algos.len() <= 10 || malgos.len() <= 10 {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        for a in algos {
            let price = jsonx::num(jsonx::get(a, "p"));
            let speed = jsonx::num(jsonx::get(a, "s"));
            if !((price > 0.0 && speed > 0.0) || ctx.info_only) {
                continue;
            }
            let id = jsonx::num(jsonx::get(a, "a")) as i64;
            let Some(md) = malgos.iter().find(|m| jsonx::truthy(jsonx::get(m, "enabled")) && jsonx::num(jsonx::get(m, "order")) as i64 == id) else {
                continue;
            };
            let algo = jsonx::s(jsonx::get(md, "algorithm")).to_lowercase();
            let algo_norm = data::get_algorithm(&algo);
            if !ctx.algo_allowed(&algo_norm) {
                continue;
            }
            let mut st = None;
            if !ctx.info_only {
                let mut p = SetStat::new(format!("{}_{}_Profit", name, algo_norm), price / 1e8, ctx.stat_span);
                p.hashrate = speed;
                p.change_detection = true;
                p.quiet = true;
                st = Some(stat::set_stat(p));
            }
            if !((!wallet.is_empty() && jsonx::num(jsonx::get(a, "o")) > 0.0) || ctx.info_only) {
                continue;
            }
            let coin_symbol = match algo_norm.as_str() {
                "BeamHash3" => "BEAM",
                "Blake3Alephium" => "ALPH",
                "CuckooCycle" => "AE",
                "Eaglesong" => "CKB",
                "Fishhash" | "FishHash" => "IRON",
                "HeavyHashPyrin" => "PYI",
                "Lbry" => "LBC",
                "NexaPow" => "NEXA",
                "RandomX" => "XMR",
                "Octopus" => "CFX",
                "Verushash" | "VerusHash" => "VRSC",
                _ => "",
            }
            .to_string();
            let coin = if coin_symbol.is_empty() { None } else { data::get_coin(&coin_symbol, "") };
            let mut eth_mode = String::new();
            let mut sym_max = String::new();
            let mut dag_max = None;
            if data::RE_ALGO_HAS_DAGSIZE.is_match(&algo_norm) {
                eth_mode = if data::RE_ALGO_IS_ETHASH.is_match(&algo_norm) {
                    "ethstratumnh".into()
                } else if data::RE_ALGO_IS_PROGPOW.is_match(&algo_norm) {
                    "stratum".into()
                } else {
                    String::new()
                };
                if coin_symbol.is_empty() {
                    sym_max = match algo_norm.as_str() {
                        "Etchash" => "ETC",
                        "Ethash" => "ETHW",
                        "KawPOW" | "KawPow" => "RVN",
                        _ => "",
                    }
                    .to_string();
                    if !sym_max.is_empty() {
                        dag_max = Some(data::get_eth_dag_size(&sym_max, &algo, 1.0));
                    }
                }
            }
            let host = format!("{}.auto.nicehash.com", algo);
            for ssl in [false, true] {
                let mut p = ctx.pool(&algo_norm).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.CoinName = coin.as_ref().map(|c| c.name.clone()).unwrap_or_default();
                p.CoinSymbol = coin_symbol.clone();
                p.Currency = "BTC".into();
                p.Protocol = super::protocol(ssl);
                p.Host = host.clone();
                p.Port = if ssl { 443 } else { 9200 };
                p.User = format!("{}.{}", wallet, ctx.worker_tag());
                p.Pass = "x".into();
                p.SSL = ssl;
                p.PoolFee = 2.0;
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.PaysLive = true;
                p.EthMode = eth_mode.clone();
                p.CoinSymbolMax = sym_max.clone();
                p.DagSizeMax = dag_max;
                p.Wallet = wallet.clone();
                p.DataWindow = String::new();
                out.push(p);
            }
        }
        Ok(out)
    }
}

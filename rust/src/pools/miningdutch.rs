//! Pools/MiningDutch.ps1

use crate::data;
use crate::jsonx;
use crate::pool::{Pool, PoolCtx, PoolModule};
use crate::stat::{self, SetStat};
use crate::web;
use crate::{log_info, log_warn};
use anyhow::Result;

pub struct MiningDutch;

impl PoolModule for MiningDutch {
    fn name(&self) -> &'static str {
        "MiningDutch"
    }

    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        let user = ctx.field("User");
        if user.is_empty() && !ctx.info_only {
            return Ok(vec![]);
        }
        let req = match web::Cached::new("https://api.rbminer.net/data/miningdutch.json").tag(name).cycletime(120).retry(3).json() {
            Ok(v) => v,
            Err(_) => {
                log_warn!("Pool API ({}) has failed. ", name);
                return Ok(vec![]);
            }
        };
        if jsonx::count(&req) <= 5 {
            log_warn!("Pool API ({}) returned nothing. ", name);
            return Ok(vec![]);
        }
        let regions = ["americas", "asia", "brazil", "canada", "europe", "india", "singapore", "hongkong", "moscow", "kazakhstan"];
        let ae = ctx.field("AEcurrency");
        let currency = if ae.is_empty() { "BTC".to_string() } else { ae };
        let mut no_port = Vec::new();
        let mut out = Vec::new();
        for v in jsonx::values(&req) {
            let algo = jsonx::s(jsonx::get(v, "name"));
            let algo_norm = data::get_algorithm(&algo);
            let fee = jsonx::num(jsonx::get(v, "fees"));
            let port = jsonx::num(jsonx::get(v, "port")) as u32;
            if port == 0 {
                no_port.push(algo);
                continue;
            }
            if !ctx.algo_allowed(&algo_norm) {
                continue;
            }
            let eth_mode = super::eth_mode_default(&algo_norm, "ethstratumnh");
            let factor = jsonx::num(jsonx::get(v, "mbtc_mh_factor"));
            if factor <= 0.0 {
                log_info!("{}: Unable to determine divisor for algorithm {}. ", name, algo);
                continue;
            }
            let tsl = jsonx::num_opt(jsonx::get(v, "tsl"));
            let blk = jsonx::num_opt(jsonx::get(v, "blk"));
            let price = jsonx::num(jsonx::get(v, "estimate_current")) * 1e-6 / factor;
            let mut st = None;
            if !ctx.info_only {
                let mut p = SetStat::new(format!("{}_{}_Profit", name, algo_norm), price, ctx.stat_span);
                p.fault_detection = true;
                p.fault_tolerance = 5.0;
                p.hashrate = jsonx::num(jsonx::get(v, "hashrate_shared")) * 1e6;
                p.blockrate = blk.unwrap_or(0.0);
                p.quiet = true;
                let r = stat::set_stat(p);
                if r.hashrate_live == 0.0 && !ctx.allow_zero {
                    continue;
                }
                st = Some(r);
            }
            for region in regions {
                let mut p = ctx.pool(&algo_norm).with_stat(&st, &ctx.stat_average, &ctx.stat_average_stable);
                p.Currency = currency.clone();
                p.Host = format!("{}.mining-dutch.nl", region);
                p.Port = port;
                p.User = format!("{}.{}", user, ctx.worker_tag());
                p.Pass = "x{diff:,d=$difficulty}".into();
                p.Region = data::get_region(region);
                p.PoolFee = fee;
                p.Workers = jsonx::num_opt(jsonx::get(v, "workers_shared"));
                p.BLK = st.as_ref().map(|s| s.blockrate_average);
                p.TSL = tsl;
                p.WTM = price == 0.0 && blk.unwrap_or(0.0) > 0.0;
                p.Hashrate = st.as_ref().map(|s| s.hashrate_live);
                p.EthMode = eth_mode.clone();
                p.Wallet = user.clone();
                out.push(p);
            }
        }
        if !no_port.is_empty() {
            log_info!("{}: no stratum port for {} - skipped. ", name, no_port.join(", "));
        }
        Ok(out)
    }
}

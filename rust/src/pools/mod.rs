//! Pool modules (Pools/*.ps1)

pub mod group1;
pub mod group2;
pub mod group3;
pub mod group4;
pub mod group5;
pub mod group6;
pub mod group7;
pub mod group8;
pub mod herominers;
pub mod miningdutch;
pub mod moneroocean;
pub mod nanopool;
pub mod nicehash;
pub mod twominers;
pub mod unmineable;
pub mod userpools;
pub mod whattomine;
pub mod zpool;

use crate::pool::PoolModule;

/// all pool modules (without Userpools and WhatToMine, they are special)
pub fn registry() -> Vec<Box<dyn PoolModule>> {
    vec![
        Box::new(twominers::TwoMiners),
        Box::new(herominers::HeroMiners),
        Box::new(miningdutch::MiningDutch),
        Box::new(moneroocean::MoneroOcean),
        Box::new(nanopool::Nanopool),
        Box::new(nicehash::NiceHash),
        Box::new(unmineable::UnMineable),
        Box::new(zpool::Zpool),
        Box::new(group1::F2pool),
        Box::new(group1::C3pool),
        Box::new(group1::UUpool),
        Box::new(group2::K1Pool { solo: false }),
        Box::new(group2::K1Pool { solo: true }),
        Box::new(group2::Kryptex { solo: false }),
        Box::new(group2::Kryptex { solo: true }),
        Box::new(group2::Mining4people { solo: false }),
        Box::new(group2::Mining4people { solo: true }),
        Box::new(group3::BaikalMine { t: group3::BaikalType::Pplns }),
        Box::new(group3::BaikalMine { t: group3::BaikalType::Pps }),
        Box::new(group3::BaikalMine { t: group3::BaikalType::Solo }),
        Box::new(group3::DeepMinerZ { solo: false }),
        Box::new(group3::DeepMinerZ { solo: true }),
        Box::new(group4::RPlant { solo: false }),
        Box::new(group4::RPlant { solo: true }),
        Box::new(group4::Ravenminer { solo: false }),
        Box::new(group4::Ravenminer { solo: true }),
        Box::new(group5::Vipor { solo: false }),
        Box::new(group5::Vipor { solo: true }),
        Box::new(group5::WoolyPooly { solo: false }),
        Box::new(group5::WoolyPooly { solo: true }),
        Box::new(group5::XdagOrg { solo: false }),
        Box::new(group5::XdagOrg { solo: true }),
        Box::new(group6::Abelpool),
        Box::new(group6::AccPool),
        Box::new(group6::Acepool),
        Box::new(group7::CpuPool),
        Box::new(group7::Crazypool),
        Box::new(group7::Ethwmine),
        Box::new(group8::EpicMine),
        Box::new(group8::FlockPool),
        Box::new(group8::HashVault),
        Box::new(group8::Hashcryptos),
        Box::new(group8::Hashpool),
    ]
}

pub fn names() -> Vec<String> {
    let mut v: Vec<String> = registry().iter().map(|p| p.name().to_string()).collect();
    v.sort_by_key(|a| a.to_lowercase());
    v
}

/// "stratum+ssl" / "stratum+tcp"
pub fn protocol(ssl: bool) -> String {
    if ssl {
        "stratum+ssl".into()
    } else {
        "stratum+tcp".into()
    }
}

/// EthMode for most pools: ethproxy for Ethash-like, stratum for ProgPow-like
pub fn eth_mode_default(algo: &str, ethash: &str) -> String {
    if crate::data::RE_ALGO_HAS_ETHPROXY.is_match(algo) {
        ethash.to_string()
    } else if crate::data::RE_ALGO_IS_PROGPOW.is_match(algo) {
        "stratum".into()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, PoolConfig};
    use crate::pool::PoolCtx;
    use indexmap::IndexMap;

    /// `cargo test pools_live -- --ignored --nocapture`: every pool module against the live APIs
    /// with test wallets (RBM_TEST_POOLS=Name,Name limits the modules)
    #[test]
    #[ignore]
    fn pools_live() {
        let tmp = std::env::temp_dir().join(format!("rbm-pooltest-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("Stats")).unwrap();
        crate::data::init(std::path::Path::new(".."));
        crate::stat::set_stats_dir(tmp.join("Stats"));
        crate::web::set_cache_dir(tmp.join("Cache"));
        crate::rates::update(&["BTC".into(), "USD".into()], &[], &[]);
        let only: Vec<String> = std::env::var("RBM_TEST_POOLS").map(|s| s.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_default();
        let config = Config::default();
        let mut wallets = IndexMap::new();
        for c in crate::data::all_coin_symbols() {
            wallets.insert(c, "TESTWALLET".to_string());
        }
        for m in super::registry() {
            if !only.is_empty() && !only.iter().any(|o| o.eq_ignore_ascii_case(m.name())) {
                continue;
            }
            let pc = PoolConfig { name: m.name().into(), wallets: wallets.clone(), ..Default::default() };
            let ctx = PoolCtx {
                name: m.name().into(),
                cfg: &pc,
                config: &config,
                wallets: &pc.wallets,
                params: &pc.params,
                worker: "test".into(),
                stat_span: chrono::Duration::seconds(300),
                data_window: String::new(),
                info_only: false,
                allow_zero: true,
                stat_average: "Minute_10".into(),
                stat_average_stable: "Week".into(),
                algorithm: vec![],
                exclude_algorithm: vec![],
                cpu_cores: 8,
                cpu_mhz: 3000,
            };
            let t0 = std::time::Instant::now();
            match m.get_pools(&ctx) {
                Ok(l) => {
                    let first = l.first().map(|p| format!("{} {}:{} {} price={:e}", p.Algorithm, p.Host, p.Port, p.CoinSymbol, p.Price)).unwrap_or_default();
                    println!("{:<20} {:>4} pools {:>6.2}s  {}", m.name(), l.len(), t0.elapsed().as_secs_f64(), first);
                }
                Err(e) => println!("{:<20} ERROR {}", m.name(), e),
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

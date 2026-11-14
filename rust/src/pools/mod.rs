//! Pool modules (Pools/*.ps1)

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

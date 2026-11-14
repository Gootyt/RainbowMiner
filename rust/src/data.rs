//! The static databases from the Data folder (algorithms.json, coinsdb.json, regions.json, ...)
//! and the lookup functions built on them: Get-Algorithm, Get-Coin, Get-Region, Get-EthDAGSize, ...
#![allow(dead_code)]

use crate::jsonx;
use crate::util;
use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

pub static RE_ALGO_HAS_ETHPROXY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^Etc?hash|ProgPow|^Meraki|UbqHash").unwrap());
pub static RE_ALGO_HAS_DAGSIZE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^Etc?hash|^KawPow|ProgPow|^FiroPow|^MeowPow|^Meraki|^NexaPow|^SccPow|UbqHash|Octopus|^Abelian|Autolykos2|^FishHash|^PhiHash").unwrap()
});
pub static RE_ALGO_IS_ETHASH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^Etc?hash|UbqHash").unwrap());
pub static RE_ALGO_IS_PROGPOW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^KawPow|ProgPow|^FiroPow|^MeowPow|^Meraki|^NexaPow|^SccPow|Octopus").unwrap()
});

#[derive(Debug, Clone, Default)]
pub struct CoinInfo {
    pub algo: String,
    pub name: String,
    pub symbol: String,
    pub multi: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Unprofitable {
    pub algorithms: Vec<String>,
    pub coins: Vec<String>,
    pub pools: HashMap<String, (Vec<String>, Vec<String>)>,
}

#[derive(Debug, Default)]
pub struct Db {
    pub base: PathBuf,
    algorithms: HashMap<String, String>,
    algorithm_map: HashMap<String, String>,
    pub algo_variants: IndexMap<String, Vec<String>>,
    coins_db: HashMap<String, CoinInfo>,
    coin_names: HashMap<String, String>,
    equihash_coins: HashMap<String, String>,
    eth_dag_sizes: HashMap<String, f64>,
    algos_dag_sizes: HashMap<String, f64>,
    regions: Vec<(String, String)>,
    regions2: HashMap<String, Vec<String>>,
    algorithm_memory: HashMap<String, f64>,
    pub unprofitable: Unprofitable,
    pub unprofitable_cpu: Vec<String>,
    pub world_currencies: Vec<String>,
    memo: HashMap<String, String>,
}

static DB: LazyLock<RwLock<Db>> = LazyLock::new(|| RwLock::new(Db::default()));

fn read_json(path: &Path) -> Option<Value> {
    util::read_text(path).ok().and_then(|t| jsonx::parse(&t))
}

fn alnum_key(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()
}

impl Db {
    fn load(base: &Path) -> Db {
        let data = base.join("Data");
        let mut db = Db {
            base: base.to_path_buf(),
            ..Default::default()
        };

        if let Some(Value::Object(m)) = read_json(&data.join("algorithms.json")) {
            for (k, v) in m {
                db.algorithms.insert(k.to_lowercase(), jsonx::s(&v));
            }
        }
        if let Some(Value::Object(m)) = read_json(&data.join("algorithmmap.json")) {
            for (k, v) in m {
                db.algorithm_map.insert(k.to_lowercase(), jsonx::s(&v));
            }
        }
        if let Some(Value::Object(m)) = read_json(&data.join("algovariantsdb.json")) {
            for (k, v) in m {
                db.algo_variants.insert(k, jsonx::str_list(&v));
            }
        }
        if let Some(Value::Object(m)) = read_json(&data.join("coins.json")) {
            for (k, v) in m {
                db.coin_names.insert(k.to_lowercase(), jsonx::s(&v));
            }
        }
        if let Some(Value::Object(m)) = read_json(&data.join("equihashcoins.json")) {
            for (k, v) in m {
                db.equihash_coins.insert(k.to_uppercase(), jsonx::s(&v));
            }
        }
        if let Some(Value::Object(m)) = read_json(&data.join("regions.json")) {
            for (k, v) in m {
                db.regions.push((k.to_lowercase(), jsonx::s(&v)));
            }
        }
        if let Some(Value::Object(m)) = read_json(&data.join("regions2.json")) {
            for (k, v) in m {
                db.regions2.insert(k.to_lowercase(), jsonx::str_list(&v));
            }
        }
        if let Some(Value::Object(m)) = read_json(&data.join("algorithmmemory.json")) {
            for (k, v) in m {
                if !k.starts_with('_') {
                    db.algorithm_memory.insert(k.to_lowercase(), jsonx::num(&v));
                }
            }
        }
        if let Some(v) = read_json(&data.join("unprofitable.json")) {
            db.unprofitable = parse_unprofitable(&v);
        }
        if let Some(v) = read_json(&data.join("unprofitable-cpu.json")) {
            db.unprofitable_cpu = jsonx::str_list(&v);
        }
        db.world_currencies = read_json(&data.join("worldcurrencies.json"))
            .map(|v| jsonx::str_list(&v))
            .unwrap_or_else(|| ["USD", "INR", "RUB", "EUR", "GBP"].iter().map(|s| s.to_string()).collect());

        // coinsdb needs Get-Algorithm, so the algorithms are loaded first
        if let Some(Value::Object(m)) = read_json(&data.join("coinsdb.json")) {
            for (k, v) in m {
                let algo_raw = jsonx::s(jsonx::get(&v, "Algo"));
                let algo = db.get_algorithm_nomemo(&algo_raw);
                db.coins_db.insert(
                    k.to_uppercase(),
                    CoinInfo {
                        algo,
                        name: jsonx::s(jsonx::get(&v, "Name")),
                        symbol: jsonx::s(jsonx::get(&v, "Symbol")),
                        multi: jsonx::truthy(jsonx::get(&v, "Multi")),
                    },
                );
            }
        }
        if let Some(v) = read_json(&data.join("ethdagsizes.json")) {
            db.set_eth_dag_sizes(&v);
        }
        db
    }

    pub fn set_eth_dag_sizes(&mut self, v: &Value) {
        let Value::Object(m) = v else { return };
        if m.len() <= 10 {
            return;
        }
        self.eth_dag_sizes.clear();
        self.algos_dag_sizes.clear();
        for (k, val) in m {
            self.eth_dag_sizes.insert(k.to_uppercase(), jsonx::num(val) / 1073741824.0);
        }
        // algorithms that belong to a single coin (plus a few multi-coin exceptions) get a DAG size
        let mut algo_count: HashMap<String, usize> = HashMap::new();
        for c in self.coins_db.values() {
            *algo_count.entry(c.algo.to_lowercase()).or_default() += 1;
        }
        let single = |a: &str| -> bool {
            let l = a.to_lowercase();
            algo_count.get(&l).copied().unwrap_or(0) == 1 || l == "firopow" || l == "progpowepic" || l == "progpowz"
        };
        let mut result: HashMap<String, f64> = HashMap::new();
        for key in self.coins_db.keys() {
            let coin = key.split('-').next().unwrap_or(key).to_string();
            let Some(size) = self.eth_dag_sizes.get(&coin).copied() else { continue };
            if size <= 0.0 {
                continue;
            }
            let Some(info) = self.coins_db.get(&coin) else { continue };
            let algo = info.algo.clone();
            if !single(&algo) || !RE_ALGO_HAS_DAGSIZE.is_match(&algo) || info.name.to_lowercase().contains("testnet") {
                continue;
            }
            let e = result.entry(algo.clone()).or_insert(size);
            if *e < size {
                *e = size;
            }
        }
        self.algos_dag_sizes = result;
    }

    fn get_algorithm_nomemo(&self, algorithm: &str) -> String {
        if algorithm == "*" {
            return "*".to_string();
        }
        let key = alnum_key(algorithm);
        match self.algorithms.get(&key.to_lowercase()) {
            Some(r) => r.clone(),
            None => util::to_title_case(&key),
        }
    }

    fn get_eth_dag_size(&self, coin_symbol: &str, algorithm: &str, minimum: f64) -> f64 {
        if !coin_symbol.is_empty() {
            if let Some(v) = self.eth_dag_sizes.get(&coin_symbol.to_uppercase()) {
                return *v;
            }
        }
        if !algorithm.is_empty() {
            if let Some(v) = self
                .algos_dag_sizes
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(algorithm))
                .map(|(_, v)| *v)
            {
                return v;
            }
        }
        minimum
    }
}

fn parse_unprofitable(v: &Value) -> Unprofitable {
    let mut u = Unprofitable {
        algorithms: jsonx::str_list(jsonx::get(v, "Algorithms")),
        coins: jsonx::str_list(jsonx::get(v, "Coins")),
        pools: HashMap::new(),
    };
    if let Value::Object(m) = jsonx::get(v, "Pools") {
        for (k, p) in m {
            u.pools.insert(
                k.to_lowercase(),
                (
                    jsonx::str_list(jsonx::get(p, "Algorithms")),
                    jsonx::str_list(jsonx::get(p, "Coins")),
                ),
            );
        }
    }
    u
}

/// (Re-)load all databases from <base>/Data
pub fn init(base: &Path) {
    let db = Db::load(base);
    *DB.write().unwrap() = db;
}

pub fn reload() {
    let base = DB.read().unwrap().base.clone();
    init(&base);
}

pub fn base_dir() -> PathBuf {
    DB.read().unwrap().base.clone()
}

/// Get-Algorithm
pub fn get_algorithm(algorithm: &str) -> String {
    if algorithm == "*" {
        return algorithm.to_string();
    }
    {
        let db = DB.read().unwrap();
        if let Some(r) = db.memo.get(algorithm) {
            return r.clone();
        }
    }
    let mut db = DB.write().unwrap();
    let r = db.get_algorithm_nomemo(algorithm);
    db.memo.insert(algorithm.to_string(), r.clone());
    r
}

/// Get-Algorithm -CoinSymbol: Ethash/KawPOW with small DAGs become Ethash2g..5g / KawPOW2g..5g
pub fn get_algorithm_coin(algorithm: &str, coin_symbol: &str) -> String {
    let r = get_algorithm(algorithm);
    if coin_symbol.is_empty() || !(r == "Ethash" || r == "KawPOW") {
        return r;
    }
    let size = get_eth_dag_size(coin_symbol, "", 1.0);
    if size <= 2.0 {
        format!("{}2g", r)
    } else if size <= 3.0 {
        format!("{}3g", r)
    } else if size <= 4.0 {
        format!("{}4g", r)
    } else if size <= 5.0 {
        format!("{}5g", r)
    } else {
        r
    }
}

/// Get-Algorithm on a "a,b;c" list
pub fn get_algorithms_list(list: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in list {
        for p in util::split_config_array(a) {
            let g = get_algorithm(&p);
            if !g.is_empty() && !out.contains(&g) {
                out.push(g);
            }
        }
    }
    out
}

/// all known (normalized) algorithm names
pub fn all_algorithms() -> Vec<String> {
    let db = DB.read().unwrap();
    let mut v: Vec<String> = db.algorithms.values().cloned().collect();
    v.sort();
    v.dedup();
    v
}

/// Get-Coin
pub fn get_coin(coin_symbol: &str, algorithm: &str) -> Option<CoinInfo> {
    if coin_symbol.is_empty() {
        return None;
    }
    let sym: String = coin_symbol
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '$' || *c == '-')
        .collect::<String>()
        .to_uppercase();
    let db = DB.read().unwrap();
    let mut c = db.coins_db.get(&sym).cloned();
    if c.is_none() && !algorithm.is_empty() {
        c = db
            .coins_db
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&format!("{}-{}", sym, algorithm)))
            .map(|(_, v)| v.clone());
    }
    drop(db);
    if let Some(ref mut coin) = c {
        if coin.algo == "Ethash" || coin.algo == "KawPOW" {
            coin.algo = get_algorithm_coin(&coin.algo, &sym);
        }
    }
    c
}

/// Get-CoinSymbol (coin name -> symbol, from Data/coins.json)
pub fn get_coin_symbol(coin_name: &str) -> Option<String> {
    let key: String = coin_name.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    DB.read().unwrap().coin_names.get(&key).cloned()
}

/// Get-CoinName: title case, strip special chars
pub fn get_coin_name(name: &str) -> String {
    let t: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '$')
        .collect();
    util::to_title_case(&t).trim().to_string()
}

/// Get-MappedAlgorithm (EnableAlgorithmMapping)
pub fn get_mapped_algorithm(algorithm: &str, enabled: bool) -> String {
    if !enabled {
        return algorithm.to_string();
    }
    DB.read()
        .unwrap()
        .algorithm_map
        .get(&algorithm.to_lowercase())
        .cloned()
        .unwrap_or_else(|| algorithm.to_string())
}

/// Get-EquihashCoinPers
pub fn get_equihash_coin_pers(coin: &str, default: &str) -> String {
    if coin.is_empty() {
        return default.to_string();
    }
    DB.read()
        .unwrap()
        .equihash_coins
        .get(&coin.to_uppercase())
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

/// Get-EthDAGSize (GB)
pub fn get_eth_dag_size(coin_symbol: &str, algorithm: &str, minimum: f64) -> f64 {
    DB.read().unwrap().get_eth_dag_size(coin_symbol, algorithm, minimum)
}

/// Get-EthDAGSizeMax: the largest DAG among the coins
pub fn get_eth_dag_size_max(coin_symbols: &[String], algorithm: &str) -> Option<(f64, String)> {
    let db = DB.read().unwrap();
    let mut max = 0.0;
    let mut sym = String::new();
    for c in coin_symbols {
        let d = db.get_eth_dag_size(c, algorithm, 0.0);
        if d > max {
            max = d;
            sym = c.clone();
        }
    }
    if max > 0.0 {
        Some((max, sym))
    } else {
        None
    }
}

pub fn set_eth_dag_sizes(v: &Value) {
    DB.write().unwrap().set_eth_dag_sizes(v);
}

/// Get-Region
pub fn get_region(region: &str) -> String {
    let r = util::to_title_case(&region.replace(['-', '_'], " ")).replace(' ', "");
    let db = DB.read().unwrap();
    let key = r.to_lowercase();
    if let Some((_, v)) = db.regions.iter().find(|(k, _)| *k == key) {
        return v.clone();
    }
    // longest region key that prefixes the name
    let mut best: Option<&(String, String)> = None;
    for e in db.regions.iter() {
        if key.starts_with(&e.0) && best.map(|b| b.0.len() < e.0.len()).unwrap_or(true) {
            best = Some(e);
        }
    }
    match best {
        Some((_, v)) => v.clone(),
        None => r,
    }
}

/// Get-Region2 (wider regions)
pub fn get_region2(region: &str) -> Vec<String> {
    DB.read()
        .unwrap()
        .regions2
        .get(&region.to_lowercase())
        .cloned()
        .unwrap_or_default()
}

/// all region target names
pub fn all_regions() -> Vec<String> {
    let db = DB.read().unwrap();
    let mut v: Vec<String> = db.regions.iter().map(|(_, v)| v.clone()).collect();
    v.sort();
    v.dedup();
    v
}

/// Get-AlgorithmMemory (GB needed on CPU, 0 = unknown)
pub fn get_algorithm_memory(algorithm: &str) -> f64 {
    DB.read()
        .unwrap()
        .algorithm_memory
        .get(&algorithm.to_lowercase())
        .copied()
        .unwrap_or(0.0)
}

pub fn unprofitable() -> Unprofitable {
    DB.read().unwrap().unprofitable.clone()
}

pub fn set_unprofitable(v: &Value) {
    DB.write().unwrap().unprofitable = parse_unprofitable(v);
}

pub fn unprofitable_cpu() -> Vec<String> {
    DB.read().unwrap().unprofitable_cpu.clone()
}

pub fn set_unprofitable_cpu(v: &Value) {
    DB.write().unwrap().unprofitable_cpu = jsonx::str_list(v);
}

pub fn world_currencies() -> Vec<String> {
    DB.read().unwrap().world_currencies.clone()
}

pub fn algo_variants() -> IndexMap<String, Vec<String>> {
    DB.read().unwrap().algo_variants.clone()
}

/// expand a list of algorithms by their variants (EnableAlgorithmVariants)
pub fn expand_algo_variants(list: &mut Vec<String>) {
    let variants = algo_variants();
    let base: Vec<String> = list.clone();
    for (k, v) in variants.iter() {
        if base.iter().any(|a| a.eq_ignore_ascii_case(k)) {
            for x in v {
                if !list.iter().any(|a| a.eq_ignore_ascii_case(x)) {
                    list.push(x.clone());
                }
            }
        }
    }
}

/// Read a JSON file from the Data folder
pub fn read_data_json(name: &str) -> Option<Value> {
    read_json(&base_dir().join("Data").join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
    }

    #[test]
    fn algorithms() {
        init(&base());
        assert_eq!(get_algorithm("ethash"), "Ethash");
        assert_eq!(get_algorithm("alph"), "Blake3Alephium");
        assert_eq!(get_algorithm("rx/0"), get_algorithm("randomx"));
        assert_eq!(get_algorithm("unknownalgo"), "Unknownalgo");
        assert_eq!(get_region("eu"), "CentralEurope");
        assert_eq!(get_region("us-east"), "USEast");
        assert!(get_eth_dag_size("ETC", "", 1.0) > 1.0);
    }
}

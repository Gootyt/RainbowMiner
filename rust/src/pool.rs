//! Pools: the pool object, the context handed to the pool modules and the common post processing
//! of Get-PoolsContent (penalties, luck, time since last block, error ratio, disabled markers).

#![allow(non_snake_case)]

use crate::config::{Config, PoolConfig};
use crate::stat::Stat;
use chrono::{DateTime, Duration, Utc};
use indexmap::IndexMap;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct PoolPorts {
    pub CPU: u32,
    pub GPU: u32,
    pub RIG: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Failover {
    pub Protocol: String,
    pub Host: String,
    pub Port: u32,
    pub User: String,
    pub Pass: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pool {
    pub Algorithm: String,
    pub Algorithm0: String,
    pub CoinName: String,
    pub CoinSymbol: String,
    pub Currency: String,
    pub Price: f64,
    pub StablePrice: f64,
    pub MarginOfError: f64,
    pub Protocol: String,
    pub Host: String,
    pub Port: u32,
    pub Ports: Option<PoolPorts>,
    pub User: String,
    pub Pass: String,
    pub Region: String,
    pub SSL: bool,
    pub SSLSelfSigned: bool,
    pub Updated: DateTime<Utc>,
    pub PoolFee: f64,
    pub DataWindow: String,
    pub Workers: Option<f64>,
    pub Hashrate: Option<f64>,
    pub BLK: Option<f64>,
    pub TSL: Option<f64>,
    pub Difficulty: Option<f64>,
    pub EthMode: String,
    pub CoinSymbolMax: String,
    pub DagSizeMax: Option<f64>,
    pub ErrorRatio: f64,
    pub Name: String,
    pub Penalty: f64,
    pub PenaltyFactor: f64,
    pub Disabled: bool,
    pub HasMinerExclusions: bool,
    pub Price_0: f64,
    pub Price_Bias: f64,
    pub Price_Unbias: f64,
    pub Wallet: String,
    pub Worker: String,
    pub Email: String,
    pub PaysLive: bool,
    pub SoloMining: bool,
    pub Exclusive: bool,
    pub Idle: bool,
    pub BenchmarkOnly: bool,
    pub PostBlockMining: bool,
    pub DisabledDueToCoinSymbolPBM: bool,
    pub WTM: bool,
    pub WTMMode: String,
    pub Failover: Vec<Failover>,
    pub MinerName: Vec<String>,
    pub ExcludeMinerName: Vec<String>,
    pub Mallob: String,
}

impl Default for Pool {
    fn default() -> Self {
        Pool {
            Algorithm: String::new(),
            Algorithm0: String::new(),
            CoinName: String::new(),
            CoinSymbol: String::new(),
            Currency: String::new(),
            Price: 0.0,
            StablePrice: 0.0,
            MarginOfError: 0.0,
            Protocol: "stratum+tcp".into(),
            Host: String::new(),
            Port: 0,
            Ports: None,
            User: String::new(),
            Pass: "x".into(),
            Region: String::new(),
            SSL: false,
            SSLSelfSigned: false,
            Updated: Utc::now(),
            PoolFee: 0.0,
            DataWindow: String::new(),
            Workers: None,
            Hashrate: None,
            BLK: None,
            TSL: None,
            Difficulty: None,
            EthMode: String::new(),
            CoinSymbolMax: String::new(),
            DagSizeMax: None,
            ErrorRatio: 0.0,
            Name: String::new(),
            Penalty: 0.0,
            PenaltyFactor: 1.0,
            Disabled: false,
            HasMinerExclusions: false,
            Price_0: 0.0,
            Price_Bias: 0.0,
            Price_Unbias: 0.0,
            Wallet: String::new(),
            Worker: String::new(),
            Email: String::new(),
            PaysLive: false,
            SoloMining: false,
            Exclusive: false,
            Idle: false,
            BenchmarkOnly: false,
            PostBlockMining: false,
            DisabledDueToCoinSymbolPBM: false,
            WTM: false,
            WTMMode: String::new(),
            Failover: Vec::new(),
            MinerName: Vec::new(),
            ExcludeMinerName: Vec::new(),
            Mallob: String::new(),
        }
    }
}

impl Pool {
    /// fill Price, StablePrice, MarginOfError, Updated from a stat
    pub fn with_stat(mut self, st: &Option<Stat>, avg: &str, avg_stable: &str) -> Self {
        if let Some(s) = st {
            self.Price = s.get_average(avg);
            self.StablePrice = s.get_average(avg_stable);
            self.MarginOfError = s.week_fluctuation;
            self.Updated = s.updated;
            self.ErrorRatio = s.error_ratio;
        }
        self
    }
    /// the port for GPU / CPU miners
    pub fn port_gpu(&self) -> u32 {
        match &self.Ports {
            Some(p) if p.GPU > 0 => p.GPU,
            _ => self.Port,
        }
    }
    pub fn port_cpu(&self) -> u32 {
        match &self.Ports {
            Some(p) if p.CPU > 0 => p.CPU,
            _ => self.Port,
        }
    }
}

/// The parameters a pool module gets (the param block of Pools/*.ps1)
pub struct PoolCtx<'a> {
    pub name: String,
    pub cfg: &'a PoolConfig,
    pub config: &'a Config,
    pub wallets: &'a IndexMap<String, String>,
    pub params: &'a IndexMap<String, String>,
    pub worker: String,
    pub stat_span: Duration,
    pub data_window: String,
    pub info_only: bool,
    pub allow_zero: bool,
    pub stat_average: String,
    pub stat_average_stable: String,
    pub algorithm: Vec<String>,
    pub exclude_algorithm: Vec<String>,
    pub cpu_cores: usize,
    pub cpu_mhz: i64,
}

impl<'a> PoolCtx<'a> {
    pub fn wallet(&self, cur: &str) -> Option<&str> {
        self.wallets
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(cur))
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty())
    }
    pub fn param(&self, cur: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(cur))
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty())
    }
    /// the configured pool fields (User, API_Key, AECurrency, Password, Email, ...)
    pub fn field(&self, k: &str) -> String {
        self.cfg.get_str(k)
    }
    pub fn worker_tag(&self) -> String {
        format!("{{workername:{}}}", self.worker)
    }
    /// skip an algorithm that the config excludes anyway
    pub fn algo_allowed(&self, algo: &str) -> bool {
        if self.info_only {
            return true;
        }
        !((!self.algorithm.is_empty() && !self.algorithm.iter().any(|a| a.eq_ignore_ascii_case(algo)))
            || (!self.exclude_algorithm.is_empty() && self.exclude_algorithm.iter().any(|a| a.eq_ignore_ascii_case(algo))))
    }
    pub fn coin_allowed(&self, sym: &str) -> bool {
        if self.info_only || sym.is_empty() {
            return true;
        }
        !((!self.cfg.coin_symbol.is_empty() && !self.cfg.coin_symbol.iter().any(|c| c.eq_ignore_ascii_case(sym)))
            || (!self.cfg.exclude_coin_symbol.is_empty() && self.cfg.exclude_coin_symbol.iter().any(|c| c.eq_ignore_ascii_case(sym))))
    }
    /// a base pool object with the common fields set
    pub fn pool(&self, algo: &str) -> Pool {
        Pool {
            Algorithm: algo.to_string(),
            Algorithm0: algo.to_string(),
            Name: self.name.clone(),
            DataWindow: self.data_window.clone(),
            Worker: self.worker_tag(),
            Email: self.field("Email"),
            ..Default::default()
        }
    }
}

pub trait PoolModule: Send + Sync {
    fn name(&self) -> &'static str;
    fn get_pools(&self, ctx: &PoolCtx) -> anyhow::Result<Vec<Pool>>;
}

/// the post processing of Get-PoolsContent
pub fn post_process(mut pools: Vec<Pool>, cfg: &PoolConfig, config: &Config, disabled: &HashSet<String>, use_pool_name: &str, enable_error_ratio: bool) -> Vec<Pool> {
    if let Some(r) = &cfg.region {
        pools.retain(|p| p.Region.eq_ignore_ascii_case(r));
    }
    let ignore_fees = config.b("IgnoreFees");
    let max_luck_cfg = config.f("MaxAllowedLuck");
    let max_tslb_cfg = config.f("MaxTimeSinceLastBlock");
    for c in pools.iter_mut() {
        let mut penalty = cfg.penalty;
        penalty += config.algorithm(&c.Algorithm).map(|a| a.penalty).unwrap_or(0.0);
        penalty += config.coin(&c.CoinSymbol).map(|x| x.penalty).unwrap_or(0.0);
        c.Penalty = penalty;
        if !ignore_fees && c.PoolFee != 0.0 {
            penalty += c.PoolFee;
        }
        if !c.SoloMining {
            if let Some(tsl) = c.TSL {
                if let Some(blk) = c.BLK {
                    if blk <= 144.0 {
                        let max_luck = cfg.max_allowed_luck.unwrap_or(max_luck_cfg);
                        if max_luck > 0.0 {
                            let luck = if blk > 0.0 { tsl * blk / 86400.0 } else { 1.0 };
                            if luck > max_luck {
                                penalty += ((luck - max_luck).min(0.385) * 12.0).exp() - 1.0;
                            }
                        }
                    }
                }
                let max_tslb = cfg.max_time_since_last_block.map(|x| x as f64).unwrap_or(max_tslb_cfg);
                if max_tslb > 0.0 && tsl > max_tslb {
                    penalty += ((tsl - max_tslb).min(554.0) / 120.0).exp() - 1.0;
                }
            }
        }
        let mut factor = (1.0 - penalty / 100.0).max(0.0);
        if enable_error_ratio && c.ErrorRatio != 0.0 {
            factor *= c.ErrorRatio;
        }
        c.Price_0 = c.Price;
        c.Price *= factor;
        c.StablePrice *= factor;
        c.PenaltyFactor = factor;
        let dkey = format!("{}_{}_profit", use_pool_name, if c.CoinSymbol.is_empty() { &c.Algorithm } else { &c.CoinSymbol }).to_lowercase();
        if disabled.contains(&dkey) {
            c.Disabled = true;
        }
    }
    pools
}

/// solo mining pools with a difficulty are split per device model (Get-PoolsContent)
pub fn expand_solo(pools: Vec<Pool>, device_combos: &[String], miner_speeds: &std::collections::HashMap<String, f64>) -> Vec<Pool> {
    let diff_factor = 86400.0 / 4294967296.0;
    let mut out = Vec::new();
    for c in pools {
        if c.SoloMining && c.Difficulty.map(|d| d > 0.0).unwrap_or(false) {
            let blkf = diff_factor / c.Difficulty.unwrap();
            for m in device_combos {
                let mut d = c.clone();
                d.Algorithm = format!("{}-{}", d.Algorithm0, m);
                let hr = miner_speeds.get(&d.Algorithm.to_lowercase()).copied().unwrap_or(0.0);
                d.Hashrate = Some(hr);
                d.BLK = Some(hr * blkf);
                out.push(d);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Get-WalletWithPaymentId
pub fn wallet_with_payment_id(wallet: &str, pidchar: &str) -> (String, String, String) {
    let mut wallet = wallet.to_string();
    let mut paymentid = String::new();
    let mut difficulty = String::new();
    if !wallet.contains('@') && wallet.contains(['+', '.', '/']) {
        let re_pid = regex::Regex::new(r"[\+\./]([a-f0-9]{16,})").unwrap();
        if let Some(c) = re_pid.captures(&wallet) {
            paymentid = c[1].to_string();
            wallet = re_pid.replace(&wallet, "").to_string();
        }
        let re_diff = regex::Regex::new(r"[\+\./](\d{1,15})$").unwrap();
        if let Some(c) = re_diff.captures(&wallet) {
            difficulty = c[1].to_string();
            wallet = re_diff.replace(&wallet, "").to_string();
        }
    }
    let full = if !paymentid.is_empty() && !pidchar.is_empty() {
        format!("{}{}{}", wallet, pidchar, paymentid)
    } else {
        wallet
    };
    (full, paymentid, difficulty)
}

/// Get-YiiMPValue
pub fn yiimp_value(req: &serde_json::Value, factor: f64, data_window: &str) -> f64 {
    use crate::jsonx;
    let fields = ["estimate_current", "estimate_last24h", "actual_last24h"];
    let mut values: IndexMap<&str, f64> = IndexMap::new();
    let mut has_details = false;
    let mut contains_zero = false;
    for f in fields {
        let v = jsonx::get(req, f);
        if !v.is_null() {
            let det = jsonx::get(req, &format!("{}_in_btc_per_hash_per_day", f));
            let x = if !det.is_null() {
                has_details = true;
                jsonx::num(det)
            } else {
                jsonx::num(v)
            };
            if x == 0.0 {
                contains_zero = true;
            }
            values.insert(f, x);
        }
    }
    if !has_details {
        if let Some(a) = values.get_mut("actual_last24h") {
            *a /= 1000.0;
        }
    }
    if values.len() == 3 && !contains_zero {
        let min = values.values().cloned().fold(f64::MAX, f64::min);
        let max = values.values().cloned().fold(f64::MIN, f64::max);
        if min > 0.0 && max / min > 10.0 {
            for v in values.values_mut() {
                if *v / min > 10.0 {
                    *v = min;
                }
            }
        }
    }
    let mut value = 0.0;
    if let Some((kind, set)) = data_window.split_once('-') {
        let flds: Vec<&str> = match set {
            "2" => vec!["actual_last24h", "estimate_current"],
            "2e" => vec!["estimate_last24h", "estimate_current"],
            "2h" => vec!["actual_last24h", "estimate_last24h"],
            _ => fields.to_vec(),
        };
        let vals: Vec<f64> = flds.iter().filter_map(|f| values.get(f).copied()).collect();
        if !vals.is_empty() {
            value = match kind {
                "minimum" => vals.iter().cloned().fold(f64::MAX, f64::min),
                "maximum" => vals.iter().cloned().fold(f64::MIN, f64::max),
                _ => vals.iter().sum::<f64>() / vals.len() as f64,
            };
        }
    } else {
        let mut dw = data_window.to_string();
        if dw.is_empty() || !values.contains_key(dw.as_str()) {
            if let Some(f) = fields.iter().find(|f| values.contains_key(*f)) {
                dw = f.to_string();
            }
        }
        if let Some(v) = values.get(dw.as_str()) {
            value = *v;
        }
    }
    if !has_details {
        value *= 1e-6 / factor;
    }
    value
}

/// Get-PoolPortsFromRequest (cryptonote pool APIs): one PoolPorts for non-SSL and one for SSL
pub fn pool_ports_from_request(req: &serde_json::Value, m_cpu: &str, m_gpu: &str, m_rig: &str, m_avoid: &str) -> [Option<PoolPorts>; 2] {
    use crate::jsonx;
    let list = jsonx::path(req, &["config", "ports"]);
    let list: Vec<&serde_json::Value> = if list.is_null() { jsonx::as_list(req) } else { jsonx::as_list(list) };
    let re = |p: &str| if p.is_empty() { None } else { regex::Regex::new(&format!("(?i){}", p)).ok() };
    let (rc, rg, rr, ra) = (re(m_cpu), re(m_gpu), re(m_rig), re(m_avoid));
    let ports: Vec<&serde_json::Value> = list
        .into_iter()
        .filter(|p| {
            !jsonx::truthy(jsonx::get(p, "disabled"))
                && !jsonx::truthy(jsonx::get(p, "virtual"))
                && ra.as_ref().map(|r| !r.is_match(&jsonx::s(jsonx::get(p, "desc")))).unwrap_or(true)
        })
        .collect();
    let mut out = [None, None];
    for ssl in 0..2 {
        let sel: Vec<&&serde_json::Value> = ports.iter().filter(|p| (jsonx::truthy(jsonx::get(p, "ssl")) as usize) == ssl).collect();
        if sel.is_empty() {
            continue;
        }
        let pick = |r: &Option<regex::Regex>| -> u32 {
            let p = r.as_ref().and_then(|r| sel.iter().find(|p| r.is_match(&jsonx::s(jsonx::get(p, "desc"))))).unwrap_or(&sel[0]);
            jsonx::num(jsonx::get(p, "port")) as u32
        };
        out[ssl] = Some(PoolPorts {
            CPU: pick(&rc),
            GPU: pick(&rg),
            RIG: pick(&rr),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn yiimp() {
        let r = json!({"estimate_current": "0.0010", "estimate_last24h": "0.0012", "actual_last24h": "1.1"});
        let v = yiimp_value(&r, 1e3, "estimate_current");
        assert!((v - 0.0010 * 1e-6 / 1e3).abs() < 1e-18);
        let v = yiimp_value(&r, 1e3, "average-2");
        assert!((v - (0.0011 + 0.0010) / 2.0 * 1e-9).abs() < 1e-15);
    }

    #[test]
    fn payment_id() {
        let (w, pid, d) = wallet_with_payment_id("4abc.0123456789abcdef0123.5000", ".");
        assert_eq!(pid, "0123456789abcdef0123");
        assert_eq!(d, "5000");
        assert_eq!(w, "4abc.0123456789abcdef0123");
    }
}

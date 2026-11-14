//! Pool balances: the Balances/*.ps1 modules, Get-Balance (totals, conversions) and
//! Set-Balance (earnings tracking in Stats/Balances).

use crate::config::{Config, PoolConfig};
use crate::data;
use crate::jsonx;
use crate::pool::wallet_with_payment_id;
use crate::rates;
use crate::util;
use crate::web;
use crate::{log_debug, log_info, log_warn};
use chrono::{DateTime, Duration, Utc};
use indexmap::IndexMap;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone)]
pub struct Payout {
    pub date: DateTime<Utc>,
    pub amount: f64,
    pub txid: String,
}

#[derive(Debug, Clone)]
pub struct Balance {
    pub caption: String,
    pub name: String,
    pub base_name: String,
    pub info: Option<String>,
    pub currency: String,
    pub balance: f64,
    pub pending: f64,
    pub total: f64,
    pub paid: Option<f64>,
    pub paid24h: Option<f64>,
    pub earned: Option<f64>,
    pub payouts: Vec<Payout>,
    pub last_updated: DateTime<Utc>,
}

impl Balance {
    fn new(name: &str, base_name: &str, currency: &str) -> Balance {
        Balance {
            caption: format!("{} ({})", base_name, currency),
            name: name.to_string(),
            base_name: base_name.to_string(),
            info: None,
            currency: currency.to_string(),
            balance: 0.0,
            pending: 0.0,
            total: 0.0,
            paid: None,
            paid24h: None,
            earned: None,
            payouts: Vec::new(),
            last_updated: Utc::now(),
        }
    }
}

/// Get-BalancesPayouts
fn payouts(list: &Value, divisor: f64) -> Vec<Payout> {
    let mut out = Vec::new();
    for p in jsonx::as_list(list) {
        let dt = ["time", "date", "datetime", "timestamp", "createdAt", "pay_time"]
            .iter()
            .map(|k| jsonx::get(p, k))
            .find(|v| jsonx::truthy(v))
            .cloned()
            .unwrap_or(Value::Null);
        if dt.is_null() {
            continue;
        }
        let amount = if !jsonx::get(p, "amount").is_null() {
            jsonx::get(p, "amount")
        } else if !jsonx::get(p, "value").is_null() {
            jsonx::get(p, "value")
        } else {
            continue;
        };
        let ds = jsonx::s(&dt);
        let date = if ds.chars().all(|c| c.is_ascii_digit()) {
            let n: i64 = ds.parse().unwrap_or(0);
            let secs = if n > 100_000_000_000 { n / 1000 } else { n };
            DateTime::from_timestamp(secs, 0).unwrap_or_else(util::unix_epoch)
        } else {
            match util::parse_datetime(&ds) {
                Some(d) => d,
                None => continue,
            }
        };
        let txid = ["tx", "txid", "tx_id", "txHash", "transactionId", "hash"]
            .iter()
            .map(|k| jsonx::s(jsonx::get(p, k)))
            .find(|s| !s.is_empty())
            .unwrap_or_default();
        out.push(Payout {
            date,
            amount: jsonx::num(amount) / divisor,
            txid: txid.trim().to_string(),
        });
    }
    out
}

struct Ctx<'a> {
    config: &'a Config,
    cycle: u64,
    use_pools: Option<Vec<String>>,
    exclude: Vec<String>,
}

impl<'a> Ctx<'a> {
    fn pool(&self, name: &str) -> Option<&'a PoolConfig> {
        self.config.pool(name)
    }
    fn wallets(&self, name: &str, exts: &[&str]) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = Vec::new();
        for e in exts {
            let n = format!("{}{}", name, e);
            if let Some(u) = &self.use_pools {
                if !util::contains_ci(u, &n) {
                    continue;
                }
            }
            if let Some(pc) = self.pool(&n) {
                for (c, w) in &pc.wallets {
                    if !w.is_empty() && !v.iter().any(|(a, b)| a == c && b == w) {
                        v.push((c.clone(), w.clone()));
                    }
                }
            }
        }
        v.sort();
        v
    }
    fn excluded(&self, cur: &str) -> bool {
        util::contains_ci(&self.exclude, cur)
    }
    fn get(&self, url: &str, tag: &str, delay: u64) -> anyhow::Result<Value> {
        web::Cached::new(url).tag(tag).delay(delay).cycletime(self.cycle).json()
    }
}

fn zpool(c: &Ctx, name: &str) -> Vec<Balance> {
    let mut out = Vec::new();
    let wallets = c.wallets(name, &["", "Coins"]);
    if wallets.is_empty() {
        log_debug!("Cannot get balance on pool ({}) - no wallet address specified. ", name);
        return out;
    }
    for (cur, w) in wallets.iter().filter(|(cur, _)| !c.excluded(cur)) {
        match c.get(&format!("https://zpool.ca/api/walletEx?address={}", w), name, 750) {
            Ok(r) if jsonx::keys(&r).len() > 1 => {
                let currency = jsonx::s(jsonx::get(&r, "currency"));
                let mut b = Balance::new(name, name, &currency);
                b.balance = jsonx::num(jsonx::get(&r, "balance"));
                b.pending = jsonx::num(jsonx::get(&r, "unsold"));
                b.total = jsonx::num(jsonx::get(&r, "unpaid"));
                b.paid24h = Some(jsonx::num(jsonx::get(&r, "paid24h")));
                b.earned = Some(jsonx::num(jsonx::get(&r, "total")));
                b.payouts = payouts(jsonx::get(&r, "payouts"), 1.0);
                out.push(b);
            }
            Ok(_) => log_info!("Pool Balance API ({}) for {} returned nothing. ", name, cur),
            Err(_) => log_warn!("Pool Balance API ({}) for {} has failed. ", name, cur),
        }
    }
    out
}

/// Get-ReadableHex32 (kept as in the original)
fn readable_hex32(key: &str) -> String {
    if key.len() % 32 != 0 || key.is_empty() {
        return key.to_string();
    }
    let mut s = String::new();
    let mut i = 0;
    while i < key.len() {
        s.push_str(&format!(
            "{}-{}-{}-{}-{}",
            &key[i..i + 8],
            &key[i + 4..i + 8],
            &key[i + 8..i + 12],
            &key[i + 12..i + 16],
            &key[i + 16..i + 28]
        ));
        i += 32;
    }
    s
}

static NH_CACHE: LazyLock<Mutex<HashMap<String, (DateTime<Utc>, Value)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Invoke-NHRequest (GET, signed)
pub fn nh_request(endpoint: &str, key: &str, secret: &str, org: &str, cache: i64) -> Option<Value> {
    let (key, secret, org) = (readable_hex32(key), readable_hex32(secret), readable_hex32(org));
    let ck = util::md5_hex(endpoint);
    if cache > 0 {
        if let Some((t, v)) = NH_CACHE.lock().unwrap().get(&ck) {
            if *t >= Utc::now() - Duration::seconds(cache) {
                return Some(v.clone());
            }
        }
    }
    let base = "https://api2.nicehash.com";
    let mut ts = web::get_json(&format!("{}/api/v2/time", base), 3).map(|v| jsonx::s(jsonx::get(&v, "serverTime"))).unwrap_or_default();
    if ts.is_empty() || ts == "0" {
        ts = (Utc::now().timestamp_millis()).to_string();
    }
    let nonce = format!("{:032x}", rand_u128());
    let nonce = format!("{}-{}-{}-{}-{}", &nonce[0..8], &nonce[8..12], &nonce[12..16], &nonce[16..20], &nonce[20..32]);
    let s = format!("{}\0{}\0{}\0\0{}\0\0GET\0{}\0", key, ts, nonce, org, endpoint);
    let sig = web::hmac_sha256_hex(secret.as_bytes(), s.as_bytes());
    let req = web::Request::get(&format!("{}{}", base, endpoint))
        .timeout(15)
        .header("X-Time", &ts)
        .header("X-Nonce", &nonce)
        .header("X-Organization-Id", &org)
        .header("X-Auth", &format!("{}:{}", key, sig));
    match web::fetch_json(&req) {
        Ok(v) => {
            NH_CACHE.lock().unwrap().insert(ck, (Utc::now(), v.clone()));
            Some(v)
        }
        Err(e) => {
            log_info!("Nicehash API call: {}", e);
            NH_CACHE.lock().unwrap().get(&ck).map(|x| x.1.clone())
        }
    }
}

fn rand_u128() -> u128 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let h = util::md5_hex(&format!("{}{}", t, std::process::id()));
    u128::from_str_radix(&h, 16).unwrap_or(t)
}

fn nicehash(c: &Ctx, name: &str) -> Vec<Balance> {
    let mut out = Vec::new();
    let Some(pc) = c.pool(name) else { return out };
    let (k, s, o) = (pc.get_str("API_Key"), pc.get_str("API_Secret"), pc.get_str("OrganizationID"));
    if k.is_empty() || s.is_empty() || o.is_empty() {
        return out;
    }
    let cache = c.cycle as i64;
    let req = nh_request("/main/api/v2/accounting/account2/BTC/", &k, &s, &o, cache).unwrap_or(Value::Null);
    let unpaid = nh_request("/main/api/v2/mining/rigs2/", &k, &s, &o, cache).unwrap_or(Value::Null);
    if jsonx::truthy(jsonx::get(&req, "active")) {
        let cur = jsonx::s(jsonx::get(&req, "currency"));
        let mut b = Balance::new(name, name, &cur);
        b.balance = jsonx::num(jsonx::get(&req, "available"));
        b.pending = jsonx::num(jsonx::get(&unpaid, "unpaidAmount"));
        b.total = b.balance + b.pending;
        out.push(b);
    }
    out
}

fn twominers(c: &Ctx, name: &str) -> Vec<Balance> {
    let mut out = Vec::new();
    let wallets = c.wallets(name, &["", "AE"]);
    if wallets.is_empty() {
        return out;
    }
    // (rpc, symbol, divisor, aesymbol)
    let data: &[(&str, &str, f64, &str)] = &[
        ("ae", "AE", 1e8, ""),
        ("beam", "BEAM", 1e8, ""),
        ("btg", "BTG", 1e8, ""),
        ("ckb", "CKB", 1e8, ""),
        ("ctxc", "CTXC", 1e18, ""),
        ("erg", "ERG", 1e9, ""),
        ("etc", "ETC", 1e18, ""),
        ("ethw", "ETHW", 1e18, ""),
        ("grin", "GRIN-PRI", 1e9, ""),
        ("kas", "KAS", 1e8, ""),
        ("mwc", "MWC-PRI", 1e9, ""),
        ("neox", "NEOX", 1e8, ""),
        ("nexa", "NEXA", 100.0, ""),
        ("rvn", "RVN", 1e8, ""),
        ("xna", "XNA", 1e8, ""),
        ("zec", "ZEC", 1e8, ""),
        ("zeph", "ZEPH", 1e12, ""),
        ("erg", "BTC", 1e9, "ERG"),
        ("etc", "BTC", 1e9, "ETC"),
        ("ethw", "BTC", 1e9, "ETHW"),
        ("kas", "BTC", 1e8, "KAS"),
        ("nexa", "BTC", 100.0, "NEXA"),
        ("rvn", "BTC", 1e8, "RVN"),
        ("xna", "BTC", 1e8, "XNA"),
        ("zec", "BTC", 1e8, "ZEC"),
        ("erg", "TON", 1e9, "ERG"),
        ("etc", "TON", 1e9, "ETC"),
        ("ethw", "TON", 1e9, "ETHW"),
        ("kas", "TON", 1e8, "KAS"),
        ("rvn", "TON", 1e8, "RVN"),
        ("xna", "TON", 1e8, "XNA"),
        ("zec", "TON", 1e8, "ZEC"),
    ];
    let ae_name = format!("{}AE", name);
    let ae_cfg = c.pool(&ae_name);
    let ae_active = util::contains_ci(&c.config.arr("PoolName"), &ae_name);
    for (sym, wallet) in &wallets {
        if c.excluded(sym) {
            continue;
        }
        for (rpc, s, divisor, aesym) in data.iter().filter(|d| d.1.eq_ignore_ascii_case(sym)) {
            if !aesym.is_empty() && ae_active {
                if let Some(a) = ae_cfg {
                    if (!a.coin_symbol.is_empty() && !util::contains_ci(&a.coin_symbol, aesym)) || util::contains_ci(&a.exclude_coin_symbol, aesym) {
                        continue;
                    }
                }
            }
            let (pool_name, info, currency) = if aesym.is_empty() {
                (name.to_string(), None, s.to_string())
            } else {
                (ae_name.clone(), Some(format!("AE {}", sym)), aesym.to_string())
            };
            let (w, _, _) = wallet_with_payment_id(wallet, ".");
            let url = format!("https://{}.2miners.com/api/accounts/{}", rpc, w);
            match web::Cached::new(&url).cycletime(c.cycle).json() {
                Ok(r) if !jsonx::get(&r, "stats").is_null() => {
                    let st = jsonx::get(&r, "stats");
                    let mut b = Balance::new(&format!("{}{}", name, info.as_deref().unwrap_or("")), &pool_name, &currency);
                    b.caption = format!("{} ({})", pool_name, currency);
                    b.info = info.clone();
                    b.balance = jsonx::num(jsonx::get(st, "balance")) / divisor;
                    b.pending = jsonx::num(jsonx::get(st, "immature")) / divisor;
                    b.total = (jsonx::num(jsonx::get(st, "balance")) + jsonx::num(jsonx::get(st, "immature"))) / divisor;
                    b.paid = Some(jsonx::num(jsonx::get(st, "paid")) / divisor);
                    b.payouts = payouts(jsonx::get(&r, "payments"), *divisor);
                    out.push(b);
                }
                Ok(_) => log_info!("Pool Balance API ({}) for {} returned nothing. ", pool_name, currency),
                Err(e) => {
                    if e.to_string().contains("404") {
                        log_debug!("Pool Balance API ({}) for {} has no account yet. ", pool_name, currency);
                    } else {
                        log_debug!("Pool Balance API ({}) for {} has failed. ", pool_name, currency);
                    }
                }
            }
        }
    }
    out
}

fn herominers(c: &Ctx, name: &str) -> Vec<Balance> {
    let mut out = Vec::new();
    let Some(pc) = c.pool(name) else { return out };
    let data: &[(&str, &str)] = &[
        ("BEAM", "beam"),
        ("CFX", "conflux"),
        ("ERG", "ergo"),
        ("ETC", "etc"),
        ("IRON", "ironfish"),
        ("KAS", "kaspa"),
        ("XMR", "monero"),
        ("PRL", "pearl"),
        ("QRL", "qrl"),
        ("QUAI", "quai"),
        ("RVN", "ravencoin"),
        ("SAL", "salvium"),
        ("WART", "warthog"),
        ("XEL", "xelis"),
        ("ZANO", "zano"),
        ("ZEPH", "zephyr"),
    ];
    let re_wk = regex::Regex::new(r#""workers":\{".+\}\}"#).unwrap();
    let re_ch = regex::Regex::new(r#""charts":\{".+\]\]\}"#).unwrap();
    let re_blk = regex::Regex::new(r"^\d+?:\d+?:\d+?:\d+?:\d+?:(\d+?):").unwrap();
    let re_pay = regex::Regex::new(r"^(.+?):(\d+?):").unwrap();
    for (sym, rpc) in data {
        let Some(w) = pc.wallets.iter().find(|(k, _)| k.eq_ignore_ascii_case(sym)).map(|x| x.1.clone()).filter(|w| !w.is_empty()) else { continue };
        if c.excluded(sym) {
            continue;
        }
        let stats = match web::Cached::new(&format!("https://{}.herominers.com/api/stats", rpc)).tag(name).timeout(15).cycletime(120).json() {
            Ok(v) => v,
            Err(_) => {
                log_debug!("Pool Balance API ({}) for {} has failed. ", name, sym);
                continue;
            }
        };
        let units = jsonx::num(jsonx::path(&stats, &["config", "coinUnits"]));
        let (wid, _, _) = wallet_with_payment_id(w.trim_start_matches("solo:"), "+");
        let url = format!("https://{}.herominers.com/api/stats_address?address={}&longpoll=false", rpc, web::url_encode(&wid));
        let text = match web::Cached::new(&url).delay(100).timeout(15).cycletime(c.cycle).text() {
            Ok(t) => t,
            Err(_) => {
                log_debug!("Pool Balance API ({}) for {} has failed. ", name, sym);
                continue;
            }
        };
        let r = jsonx::parse(&text).or_else(|| {
            let t = re_ch.replace(&re_wk.replace(&text, r#""workers":{ }"#), r#""charts":{ }"#).to_string();
            jsonx::parse(&t)
        });
        let Some(r) = r else { continue };
        if jsonx::get(&r, "stats").is_null() || units == 0.0 {
            log_info!("Pool Balance API ({}) for {} returned nothing. ", name, sym);
            continue;
        }
        let pending: f64 = jsonx::as_list(jsonx::get(&r, "blocks"))
            .iter()
            .filter_map(|b| re_blk.captures(&jsonx::s(b)).and_then(|c| c[1].parse::<i64>().ok()))
            .sum::<i64>() as f64
            / units;
        let pays = jsonx::as_list(jsonx::get(&r, "payments"));
        let mut plist = Vec::new();
        let mut i = 0;
        while i < pays.len() {
            let s = jsonx::s(pays[i]);
            if let Some(c) = re_pay.captures(&s) {
                plist.push(json!({"time": jsonx::s(pays.get(i + 1).copied().unwrap_or(&Value::Null)), "amount": c[2].parse::<f64>().unwrap_or(0.0) / units, "txid": c[1].to_string()}));
            }
            i += 2;
        }
        let st = jsonx::get(&r, "stats");
        let mut b = Balance::new(name, name, sym);
        b.balance = jsonx::num(jsonx::get(st, "balance")) / units;
        b.pending = pending;
        b.total = b.balance + pending;
        b.paid = Some(jsonx::num(jsonx::get(st, "paid")) / units);
        b.payouts = payouts(&Value::Array(plist), 1.0);
        out.push(b);
    }
    out
}

fn nanopool(c: &Ctx, name: &str) -> Vec<Balance> {
    let mut out = Vec::new();
    let Some(pc) = c.pool(name) else { return out };
    let data: &[(&str, &str)] = &[("XMR", "xmr"), ("RVN", "rvn"), ("CFX", "cfx"), ("ERG", "ergo")];
    let mut count = 0;
    for (sym, rpc) in data {
        let Some(w) = pc.wallets.iter().find(|(k, _)| k.eq_ignore_ascii_case(sym)).map(|x| x.1.clone()).filter(|w| !w.is_empty()) else { continue };
        if c.excluded(sym) {
            continue;
        }
        // the plain wallet without payment id (Get-WalletWithPaymentId -asobject).wallet
        let (full, pid, _) = wallet_with_payment_id(&w, ".");
        let wallet = if pid.is_empty() { full } else { full.trim_end_matches(&format!(".{}", pid)).to_string() };
        let delay = if count > 0 { 500 } else { 0 };
        let url = format!("https://{}.nanopool.org/api/v1/load_account/{}", rpc, wallet);
        match web::Cached::new(&url).delay(delay).cycletime(c.cycle).retry(5).json() {
            Ok(r) => {
                count += 1;
                if !jsonx::truthy(jsonx::get(&r, "status")) {
                    log_info!("Pool Balance API ({}) for {} returned {}. ", name, sym, jsonx::s(jsonx::get(&r, "error")));
                    continue;
                }
                let up = jsonx::path(&r, &["data", "userParams"]);
                let balance = jsonx::num(jsonx::get(up, "balance")).max(0.0);
                let pending = jsonx::num(jsonx::get(up, "balance_uncomfirmed"));
                let pays = web::Cached::new(&format!("https://api.nanopool.org/v1/{}/payments/{}/0/50", rpc, wallet))
                    .delay(500)
                    .cycletime(c.cycle)
                    .retry(5)
                    .json()
                    .ok()
                    .filter(|p| jsonx::truthy(jsonx::get(p, "status")))
                    .map(|p| jsonx::get(&p, "data").clone())
                    .unwrap_or(Value::Null);
                let mut b = Balance::new(name, name, sym);
                b.balance = balance;
                b.pending = pending;
                b.total = balance + pending;
                b.paid = Some(jsonx::num(jsonx::get(up, "e_sum")));
                b.earned = Some(0.0);
                b.payouts = payouts(&pays, 1.0);
                out.push(b);
            }
            Err(_) => log_warn!("Pool Balance API ({}) for {} has failed. ", name, sym),
        }
    }
    out
}

fn moneroocean(c: &Ctx, name: &str) -> Vec<Balance> {
    let mut out = Vec::new();
    let Some(pc) = c.pool(name) else { return out };
    let Some(w) = pc.wallets.iter().find(|(k, _)| k.eq_ignore_ascii_case("XMR")).map(|x| x.1.clone()).filter(|w| !w.is_empty()) else {
        return out;
    };
    match web::Cached::new(&format!("https://api.moneroocean.stream/miner/{}/stats", w)).cycletime(c.cycle).json() {
        Ok(r) if jsonx::keys(&r).len() > 1 => {
            let mut b = Balance::new(name, name, "XMR");
            b.balance = jsonx::num(jsonx::get(&r, "amtDue")) / 1e12;
            b.total = b.balance;
            b.paid = Some(jsonx::num(jsonx::get(&r, "amtPaid")) / 1e12);
            out.push(b);
        }
        Ok(_) => log_info!("Pool Balance API ({}) returned nothing. ", name),
        Err(_) => log_warn!("Pool Balance API ({}) has failed. ", name),
    }
    out
}

fn unmineable(c: &Ctx, name: &str) -> Vec<Balance> {
    let mut out = Vec::new();
    let Some(pc) = c.pool(name) else { return out };
    let coins = match web::Cached::new("https://api.unmineable.com/v4/coin").tag(name).cycletime(21600).json() {
        Ok(v) if jsonx::truthy(jsonx::get(&v, "success")) => v,
        _ => {
            log_warn!("Pool API ({}) has failed. ", name);
            return out;
        }
    };
    let re = regex::Regex::new(r#","hashrate".+$"#).unwrap();
    for coin in jsonx::as_list(jsonx::get(&coins, "data")) {
        let sym = jsonx::s(jsonx::get(coin, "symbol"));
        let Some(w) = pc.wallets.iter().find(|(k, _)| k.eq_ignore_ascii_case(&sym)).map(|x| x.1.clone()).filter(|w| !w.is_empty()) else { continue };
        if c.excluded(&sym) {
            continue;
        }
        let url = format!("https://api.unminable.com/v4/address/{}?coin={}", w, sym);
        let Ok(t) = web::Cached::new(&url).cycletime(c.cycle).text() else {
            log_debug!("Pool Balance API ({}) for {} has failed. ", name, sym);
            continue;
        };
        let t = re.replace(&t, "}}").to_string();
        let Some(r) = jsonx::parse(&t) else { continue };
        if !jsonx::truthy(jsonx::get(&r, "success")) {
            log_info!("Pool Balance API ({}) for {} returned nothing. ", name, sym);
            continue;
        }
        let d = jsonx::get(&r, "data");
        let mut b = Balance::new(&format!("{} leg.", name), name, &sym);
        b.info = Some("leg.".into());
        b.balance = jsonx::num(jsonx::get(d, "balance"));
        b.total = b.balance;
        b.paid = Some(jsonx::num(jsonx::get(d, "total_paid")));
        b.paid24h = Some(jsonx::num(jsonx::get(d, "total_24h")));
        out.push(b);
    }
    out
}

type BalanceModule = fn(&Ctx, &str) -> Vec<Balance>;

fn modules() -> Vec<(&'static str, BalanceModule)> {
    vec![
        ("2Miners", twominers as BalanceModule),
        ("HeroMiners", herominers),
        ("MoneroOcean", moneroocean),
        ("Nanopool", nanopool),
        ("Nicehash", nicehash),
        ("unMineable", unmineable),
        ("Zpool", zpool),
    ]
}

/// Get-BalancesContent
pub fn get_balances_content(config: &Config, pool_modules: &[String]) -> Vec<Balance> {
    let pn = config.arr("PoolName");
    let xpn = config.arr("ExcludePoolName");
    let possible: Vec<String> = pool_modules
        .iter()
        .filter(|p| (pn.is_empty() || util::contains_ci(&pn, p)) && !util::contains_ci(&xpn, p))
        .cloned()
        .collect();
    let show_excluded = config.b("ShowPoolBalancesExcludedPools");
    let re_ext = |b: &str| regex::Regex::new(&format!(r"(?i)^{}(AE|Coins|CoinsSolo|CoinsParty|Party|PPS|Solo)?$", regex::escape(b))).unwrap();
    let mut out = Vec::new();
    for (name, f) in modules() {
        let re = re_ext(name);
        let matching: Vec<String> = possible.iter().filter(|p| re.is_match(p)).cloned().collect();
        if matching.is_empty() && !show_excluded {
            continue;
        }
        let ctx = Ctx {
            config,
            cycle: (config.i("BalanceUpdateMinutes").max(1) * 60) as u64,
            use_pools: if show_excluded { None } else { Some(matching) },
            exclude: config.arr("ExcludeCoinsymbolBalances"),
        };
        out.extend(f(&ctx, name));
    }
    out
}

// ----------------------------------------------------------------------------------------
// Set-Balance: earnings tracking
// ----------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct EarningsStat {
    pool_name: String,
    currency: String,
    balance: f64,
    paid: f64,
    earnings: f64,
    earnings_1h: f64,
    earnings_1d: f64,
    earnings_1w: f64,
    earnings_avg: f64,
    last: Vec<(DateTime<Utc>, f64)>,
    started: DateTime<Utc>,
    updated: DateTime<Utc>,
}

impl EarningsStat {
    fn to_json(&self) -> Value {
        json!({
            "PoolName": self.pool_name,
            "Currency": self.currency,
            "Balance": self.balance,
            "Paid": self.paid,
            "Earnings": self.earnings,
            "Earnings_1h": self.earnings_1h,
            "Earnings_1d": self.earnings_1d,
            "Earnings_1w": self.earnings_1w,
            "Earnings_Avg": self.earnings_avg,
            "Last_Earnings": self.last.iter().map(|(d, v)| json!({"Date": util::format_datetime(d), "Value": v})).collect::<Vec<_>>(),
            "Started": util::format_datetime(&self.started),
            "Updated": util::format_datetime(&self.updated),
        })
    }
}

static EARNINGS: LazyLock<Mutex<HashMap<String, EarningsStat>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn append_csv(path: &Path, header: &[&str], row: &[String]) {
    let exists = path.exists();
    let mut s = String::new();
    if !exists {
        s.push_str(&header.iter().map(|h| format!("\"{}\"", h)).collect::<Vec<_>>().join(","));
        s.push('\n');
    }
    s.push_str(&row.iter().map(|v| format!("\"{}\"", v.replace('"', "\"\""))).collect::<Vec<_>>().join(","));
    s.push('\n');
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(s.as_bytes());
    }
}

/// Set-Balance: returns (Earnings_1h, Earnings_1d, Earnings_1w, Earnings_Avg, Started)
fn set_balance(base: &Path, b: &Balance, updated: DateTime<Utc>) -> EarningsStat {
    let name = format!("{}_{}_Balance", b.name, b.currency);
    let dir = base.join("Stats").join("Balances");
    let path = dir.join(format!("{}.txt", name));
    let mut cache = EARNINGS.lock().unwrap();
    let mut write = false;
    if !cache.contains_key(&name) {
        let st = util::read_text(&path).ok().and_then(|t| jsonx::parse(&t)).map(|v| EarningsStat {
            pool_name: b.name.clone(),
            currency: b.currency.clone(),
            balance: jsonx::num(jsonx::get(&v, "Balance")),
            paid: jsonx::num(jsonx::get(&v, "Paid")),
            earnings: jsonx::num(jsonx::get(&v, "Earnings")),
            earnings_1h: jsonx::num(jsonx::get(&v, "Earnings_1h")),
            earnings_1d: jsonx::num(jsonx::get(&v, "Earnings_1d")),
            earnings_1w: jsonx::num(jsonx::get(&v, "Earnings_1w")),
            earnings_avg: jsonx::num(jsonx::get(&v, "Earnings_Avg")),
            last: jsonx::as_list(jsonx::get(&v, "Last_Earnings"))
                .iter()
                .filter_map(|e| util::parse_datetime(&jsonx::s(jsonx::get(e, "Date"))).map(|d| (d, jsonx::num(jsonx::get(e, "Value")))))
                .collect(),
            started: util::parse_datetime(&jsonx::s(jsonx::get(&v, "Started"))).unwrap_or(updated),
            updated: util::parse_datetime(&jsonx::s(jsonx::get(&v, "Updated"))).unwrap_or(updated),
        });
        let st = match st {
            Some(s) => s,
            None => {
                if path.exists() {
                    log_warn!("Balances file ({}) is corrupt and will be reset. ", name);
                }
                write = true;
                EarningsStat {
                    pool_name: b.name.clone(),
                    currency: b.currency.clone(),
                    balance: b.balance,
                    paid: b.paid.unwrap_or(0.0),
                    earnings: 0.0,
                    earnings_1h: 0.0,
                    earnings_1d: 0.0,
                    earnings_1w: 0.0,
                    earnings_avg: 0.0,
                    last: Vec::new(),
                    started: updated,
                    updated,
                }
            }
        };
        cache.insert(name.clone(), st);
    }
    let st = cache.get_mut(&name).unwrap();
    let total = b.balance;
    let paid = b.paid.unwrap_or(0.0);
    let earnings = if b.paid.is_some() {
        total - st.balance + paid - st.paid
    } else {
        let e = total - st.balance;
        if e < 0.0 {
            total
        } else {
            e
        }
    };
    if earnings > 0.0 {
        st.balance = total;
        st.paid = paid;
        st.earnings += earnings;
        st.updated = updated;
        st.last.push((updated, earnings));
        let _ = std::fs::create_dir_all(&dir);
        let rate = rates::get(&b.currency).unwrap_or(0.0);
        let sat = |v: f64| if rate > 0.0 { ((v / rate) * 1e8) as i64 } else { 0 };
        append_csv(
            &dir.join("Earnings.csv"),
            &["Date", "Date_UTC", "PoolName", "Currency", "Rate", "Balance", "Paid", "Earnings", "Value", "Balance_Sat", "Paid_Sat", "Earnings_Sat", "Value_Sat"],
            &[
                updated.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string(),
                updated.format("%Y-%m-%d %H:%M:%S").to_string(),
                b.name.clone(),
                b.currency.clone(),
                rate.to_string(),
                st.balance.to_string(),
                st.paid.to_string(),
                st.earnings.to_string(),
                earnings.to_string(),
                sat(st.balance).to_string(),
                sat(st.paid).to_string(),
                sat(st.earnings).to_string(),
                sat(earnings).to_string(),
            ],
        );
        write = true;
    }
    let cut_1w = updated - Duration::days(7);
    st.last.retain(|(d, _)| *d > cut_1w);
    let (c1h, c1d) = (updated - Duration::hours(1), updated - Duration::days(1));
    st.earnings_1w = st.last.iter().map(|x| x.1).sum();
    st.earnings_1d = st.last.iter().filter(|x| x.0 >= c1d).map(|x| x.1).sum();
    st.earnings_1h = st.last.iter().filter(|x| x.0 >= c1h).map(|x| x.1).sum();
    st.earnings_avg = if st.earnings_1w != 0.0 {
        let dur = (updated - st.last[0].0).num_seconds() as f64 / 86400.0;
        if dur > 1.0 {
            st.last.iter().map(|x| x.1).sum::<f64>() / dur
        } else {
            st.earnings_1d
        }
    } else {
        0.0
    };
    if write {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&path, serde_json::to_string_pretty(&st.to_json()).unwrap_or_default());
    }
    st.clone()
}

// ----------------------------------------------------------------------------------------
// Get-Balance
// ----------------------------------------------------------------------------------------

static CACHED: LazyLock<Mutex<Vec<Balance>>> = LazyLock::new(|| Mutex::new(Vec::new()));

fn fmt_digits(v: f64, digits: i32) -> String {
    let n = if v >= 10.0 && digits == 8 { (v.log10().ceil() as i32).min(8) } else { 1 };
    util::format_n(v, (digits - n + 1).max(0) as usize)
}

/// Get-Balance: the rows of the balance table (also the /balances API), plus Earnings_Avg / Earnings_1d in BTC
pub fn get_balance(base: &Path, config: &Config, pool_modules: &[String], refresh: bool) -> (Vec<Value>, f64, f64) {
    {
        let mut c = CACHED.lock().unwrap();
        if c.is_empty() || refresh {
            let exclude = config.arr("ExcludeCoinsymbolBalances");
            let mut groups: IndexMap<String, Balance> = IndexMap::new();
            for b in get_balances_content(config, pool_modules) {
                if util::contains_ci(&exclude, &b.currency) {
                    continue;
                }
                match groups.get_mut(&b.caption) {
                    Some(g) => {
                        g.balance += b.balance;
                        g.pending += b.pending;
                        g.total += b.total;
                        g.paid = match (g.paid, b.paid) {
                            (None, None) => None,
                            (a, b2) => Some(a.unwrap_or(0.0) + b2.unwrap_or(0.0)),
                        };
                        g.earned = match (g.earned, b.earned) {
                            (None, None) => None,
                            (a, b2) => Some(a.unwrap_or(0.0) + b2.unwrap_or(0.0)),
                        };
                        g.payouts.extend(b.payouts);
                    }
                    None => {
                        groups.insert(b.caption.clone(), b);
                    }
                }
            }
            *c = groups.into_values().collect();
        }
    }
    let balances: Vec<Balance> = CACHED.lock().unwrap().iter().filter(|b| config.b("ShowWalletBalances") || b.base_name != "Wallet").cloned().collect();
    if balances.is_empty() {
        return (vec![], 0.0, 0.0);
    }
    let mut currencies: Vec<String> = balances.iter().map(|b| b.currency.to_uppercase()).collect();
    currencies.sort();
    currencies.dedup();
    let mut to_ex: Vec<String> = std::iter::once("BTC".to_string()).chain(config.arr("Currency")).collect();
    to_ex.sort();
    to_ex.dedup();
    let missing: Vec<String> = currencies.iter().chain(to_ex.iter()).filter(|c| rates::get(c).is_none()).cloned().collect();
    if !missing.is_empty() {
        rates::update(&config.arr("Currency"), &missing, &config.arr("ExcludeCurrencyRateFromCoinbase"));
    }
    let wc = data::world_currencies();
    let digits = |c: &str| if util::contains_ci(&wc, c) { 2 } else { 8 };
    let mut rows: Vec<(Balance, serde_json::Map<String, Value>)> = balances.into_iter().map(|b| (b, serde_json::Map::new())).collect();
    let mut totals = serde_json::Map::new();
    for cur in &currencies {
        let mut sum = 0.0;
        let mut psum = 0.0;
        for (b, m) in rows.iter_mut().filter(|(b, _)| b.currency.eq_ignore_ascii_case(cur)) {
            m.insert(format!("Balance ({})", cur), json!(b.total));
            m.insert(format!("Pending ({})", cur), json!(b.pending));
            sum += b.total;
            psum += b.pending;
        }
        if sum != 0.0 {
            totals.insert(format!("Balance ({})", cur), json!(sum));
        }
        if psum != 0.0 {
            totals.insert(format!("Pending ({})", cur), json!(psum));
        }
    }
    let mut out_cur: Vec<String> = config.arr("Currency").iter().map(|c| c.to_uppercase()).collect();
    out_cur.sort();
    out_cur.dedup();
    for cur in &out_cur {
        let mut sum = 0.0;
        for (b, m) in rows.iter_mut() {
            let v = match (rates::get(&b.currency), rates::get(cur)) {
                (Some(rb), Some(rc)) if rb > 0.0 => Some(b.total * rc / rb),
                _ => None,
            };
            match v {
                Some(v) => {
                    m.insert(format!("Value in {}", cur), json!(v));
                    sum += v;
                }
                None => {
                    m.insert(format!("Value in {}", cur), json!("-"));
                }
            }
        }
        if sum != 0.0 {
            totals.insert(format!("Value in {}", cur), json!(sum));
        }
    }
    let now = Utc::now();
    let mut earn_avg = 0.0;
    let mut earn_1d = 0.0;
    let mut out = Vec::new();
    let details = config.b("ShowPoolBalancesDetails");
    let mut final_rows: Vec<Value> = Vec::new();
    if details {
        for (b, m) in rows.iter() {
            if b.total == 0.0 {
                continue;
            }
            let mut o = row_json(b, m);
            let st = set_balance(base, b, now);
            add_earnings(&mut o, &st);
            if let Some(r) = rates::get(&b.currency).filter(|r| *r > 0.0) {
                earn_avg += st.earnings_avg / r;
                earn_1d += st.earnings_1d / r;
            }
            final_rows.push(o);
        }
    } else {
        // consolidate per pool name (BTC first)
        let mut by_name: IndexMap<String, Vec<usize>> = IndexMap::new();
        for (i, (b, _)) in rows.iter().enumerate() {
            by_name.entry(b.name.clone()).or_default().push(i);
        }
        for (i, (b, _)) in rows.iter().enumerate() {
            if b.total != 0.0 {
                let st = set_balance(base, b, now);
                if let Some(r) = rates::get(&b.currency).filter(|r| *r > 0.0) {
                    earn_avg += st.earnings_avg / r;
                    earn_1d += st.earnings_1d / r;
                }
                let _ = i;
            }
        }
        for (name, idx) in by_name {
            let mut sorted = idx.clone();
            sorted.sort_by(|a, b| {
                let (x, y) = (&rows[*a].0, &rows[*b].0);
                (y.currency == "BTC").cmp(&(x.currency == "BTC")).then(x.caption.cmp(&y.caption))
            });
            let first = &rows[sorted[0]].0;
            if idx.iter().all(|i| rows[*i].0.total == 0.0) {
                continue;
            }
            let mut o = serde_json::Map::new();
            o.insert("Caption".into(), json!(first.caption));
            o.insert("Currency".into(), json!("BTC"));
            o.insert("Name".into(), json!(name));
            o.insert("BaseName".into(), json!(first.base_name));
            o.insert("Total".into(), json!(idx.iter().map(|i| rows[*i].0.total).sum::<f64>()));
            for cur in &out_cur {
                let k = format!("Value in {}", cur);
                let s: f64 = idx.iter().filter_map(|i| rows[*i].1.get(&k).and_then(|v| v.as_f64())).sum();
                o.insert(k, json!(s));
            }
            final_rows.push(Value::Object(o));
        }
    }
    let mut t = serde_json::Map::new();
    t.insert("Name".into(), json!("*Total*"));
    t.insert("BaseName".into(), json!("Total"));
    t.insert("Caption".into(), json!("*Total*"));
    t.extend(totals);
    final_rows.push(Value::Object(t));
    // format the value columns like the original ("{0:N<digits>}")
    let re = regex::Regex::new(r"^(Value in |Balance \(|Pending \()(\w+)").unwrap();
    for r in final_rows.iter_mut() {
        if let Value::Object(m) = r {
            let keys: Vec<String> = m.keys().cloned().collect();
            for k in keys {
                if let Some(c) = re.captures(&k) {
                    let cur = c[2].to_string();
                    let v = m.get(&k).cloned().unwrap_or(Value::Null);
                    let f = if v.is_string() && jsonx::s(&v) == "-" { 0.0 } else { jsonx::num(&v) };
                    m.insert(k.clone(), json!(fmt_digits(f, digits(&cur))));
                }
            }
        }
        out.push(r.clone());
    }
    (out, earn_avg, earn_1d)
}

fn row_json(b: &Balance, extra: &serde_json::Map<String, Value>) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("Caption".into(), json!(b.caption));
    o.insert("Name".into(), json!(b.name));
    o.insert("BaseName".into(), json!(b.base_name));
    o.insert("Info".into(), json!(b.info));
    o.insert("Currency".into(), json!(b.currency));
    o.insert("Balance".into(), json!(b.balance));
    o.insert("Pending".into(), json!(b.pending));
    o.insert("Total".into(), json!(b.total));
    o.insert("Paid".into(), json!(b.paid));
    o.insert("Paid24h".into(), json!(b.paid24h));
    o.insert("Earned".into(), json!(b.earned));
    o.insert(
        "Payouts".into(),
        Value::Array(b.payouts.iter().map(|p| json!({"Date": util::format_datetime(&p.date), "Amount": p.amount, "Txid": p.txid})).collect()),
    );
    o.insert("LastUpdated".into(), json!(util::format_datetime(&b.last_updated)));
    for (k, v) in extra {
        o.insert(k.clone(), v.clone());
    }
    Value::Object(o)
}

fn add_earnings(o: &mut Value, st: &EarningsStat) {
    jsonx::set(o, "Earnings_1h", json!(st.earnings_1h));
    jsonx::set(o, "Earnings_1d", json!(st.earnings_1d));
    jsonx::set(o, "Earnings_1w", json!(st.earnings_1w));
    jsonx::set(o, "Earnings_Avg", json!(st.earnings_avg));
    jsonx::set(o, "Earnings", json!(st.earnings));
    jsonx::set(o, "Started", json!(util::format_datetime(&st.started)));
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payouts_parse() {
        let v = json!([{"time": 1700000000, "amount": 150000000, "tx": "abc"}, {"date": "2024-01-01T00:00:00Z", "value": 1.5}]);
        let p = payouts(&v, 1e8);
        assert_eq!(p.len(), 2);
        assert!((p[0].amount - 1.5).abs() < 1e-9);
        assert_eq!(p[0].txid, "abc");
    }

    #[test]
    fn hex32() {
        let k = "0123456789abcdef0123456789abcdef";
        assert_eq!(readable_hex32(k), "01234567-4567-89ab-cdef-0123456789ab");
        assert_eq!(readable_hex32("abc"), "abc");
    }
}

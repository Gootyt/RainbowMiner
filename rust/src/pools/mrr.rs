//! Pools/MiningRigRentals.ps1 and the helpers of Modules/MiningRigRentals.psm1:
//! the rigs of the workers are enabled/disabled on miningrigrentals.com, a rented rig becomes an
//! exclusive pool, renters get the start/offline/extension/difficulty messages, rentals can be
//! extended automatically and the MRR servers are pinged while a rig waits for its renter's pool.
//! The automatic rig creation and price updates of the original (EnableAutoCreate,
//! EnableAutoUpdate, EnableAutoPrice, EnableAutoBenchmark) are not part of this port yet.

use crate::data;
use crate::jsonx;
use crate::log_info;
use crate::log_warn;
use crate::pool::{EngineView, Failover, Pool, PoolCtx, PoolModule};
use crate::stat::{self, SetStat};
use crate::util;
use crate::web;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write as _};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

const BASE: &str = "https://www.miningrigrentals.com/api/v2";
const POOL_FEE: f64 = 3.0;

// ------------------------------------------------------------------------------------------
// API (Invoke-MiningRigRentalRequest)
// ------------------------------------------------------------------------------------------

static CACHE: LazyLock<Mutex<HashMap<String, (DateTime<Utc>, Value)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn hmac_sha1_hex(secret: &str, msg: &str) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = <Hmac<sha1::Sha1> as Mac>::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(msg.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// a signed MRR API call; returns the "data" of a successful answer. With cache > 0 the last
/// successful answer is reused for that many seconds (and when a later call fails)
pub fn request(endpoint: &str, key: &str, secret: &str, params: Option<&Value>, method: &str, timeout: u64, cache: i64, force: bool) -> Option<Value> {
    let jobkey = util::md5_hex(&format!("{}{}{}{}", BASE, method, endpoint, params.map(|p| p.to_string()).unwrap_or_default()));
    let now = Utc::now();
    if cache > 0 && !force {
        if let Some((t, v)) = CACHE.lock().unwrap().get(&jobkey) {
            if *t >= now - Duration::seconds(cache) {
                return Some(v.clone());
            }
        }
    }
    let nonce = now.timestamp_millis();
    let sign = hmac_sha1_hex(secret, &format!("{}{}{}", key, nonce, endpoint));
    let mut url = format!("{}{}", BASE, endpoint);
    let mut req = web::Request::get(&url);
    match method {
        "POST" | "PUT" => {
            req = req.post_json(params.unwrap_or(&json!({})));
            req.method = method.to_string();
        }
        _ => {
            if let Some(Value::Object(m)) = params {
                if !m.is_empty() {
                    let q: Vec<String> = m.iter().map(|(k, v)| format!("{}={}", k, web::http_utility_encode(&jsonx::s(v)))).collect();
                    url = format!("{}?{}", url, q.join("&"));
                    req.url = url.clone();
                }
            }
            req.method = method.to_string();
        }
    }
    req = req.timeout(timeout).header("x-api-sign", &sign).header("x-api-key", key).header("x-api-nonce", &nonce.to_string());
    let data = match web::fetch_json(&req) {
        Ok(d) => Some(d),
        Err(e) => {
            log_info!("MiningRigRental call: {}", e);
            None
        }
    };
    let ok = data.as_ref().map(|d| jsonx::truthy(jsonx::get(d, "success"))).unwrap_or(false);
    if let Some(d) = &data {
        if !jsonx::get(d, "success").is_null() && !ok {
            let m = jsonx::s(jsonx::path(d, &["data", "message"]));
            log_warn!("MiningRigRental error: {}", if m.is_empty() { "unknown".to_string() } else { m });
        }
    }
    let mut c = CACHE.lock().unwrap();
    if ok {
        let v = jsonx::get(data.as_ref().unwrap(), "data").clone();
        if cache > 0 {
            c.insert(jobkey, (now, v.clone()));
        } else {
            c.remove(&jobkey);
        }
        // cleanup of old entries
        c.retain(|_, (t, _)| *t >= now - Duration::hours(24));
        return Some(v);
    }
    if cache > 0 {
        return c.get(&jobkey).map(|(_, v)| v.clone());
    }
    None
}

fn put(endpoint: &str, key: &str, secret: &str, params: Value) -> Option<Value> {
    request(endpoint, key, secret, Some(&params), "PUT", 60, 0, false)
}

// ------------------------------------------------------------------------------------------
// helpers
// ------------------------------------------------------------------------------------------

/// Get-MiningRigRentalAlgorithm
pub fn algorithm(name: &str) -> String {
    data::get_algorithm(match name {
        "cuckaroo24" => "Cuckaroom29",
        "cuckoocycle" => "Cuckarood29",
        "equihash1505" => "BeamHash3",
        "x16rt" => "X16rtVeil",
        "x16rtgin" => "X16rt",
        "xelishash" => "XelisHashV2",
        n => n,
    })
}

/// Get-MiningRigRentalCoin
pub fn coin(name: &str) -> &'static str {
    match name {
        "cuckaroo24" => "PMEER",
        "cuckoocycle" => "GRIN",
        "cuckoocycle29swap" => "SWAP",
        "equihash1505" => "BEAM",
        "equihash1505g" => "GRIMM",
        "blake2b" => "TNET",
        _ => "",
    }
}

/// Get-MiningRigRentalsDivisor
pub fn divisor(unit: &str) -> f64 {
    match unit.split('*').next().unwrap_or("") {
        "kh" => 1e3,
        "mh" => 1e6,
        "gh" => 1e9,
        "th" => 1e12,
        _ => 1.0,
    }
}

/// Get-MiningRigRentalsSubst
fn subst(text: &str, s: &[(&str, String)]) -> String {
    let mut t = text.to_string();
    for (k, v) in s {
        t = t.replace(&format!("%{}%", k), v);
    }
    regex::Regex::new(r"[\s+]").unwrap().replace_all(&t, " ").into_owned()
}

fn round_diff(d: f64) -> String {
    if d > 10.0 {
        format!("{}", d.round())
    } else {
        format!("{}", d)
    }
}

/// Get-MiningRigRentalRigs: the rigs of the workers (description contains "[worker]")
pub fn rigs(key: &str, secret: &str, workers: &[String]) -> Vec<Value> {
    let re = regex::Regex::new(&format!(r"\[({})\]", workers.iter().map(|w| regex::escape(w)).collect::<Vec<_>>().join("|"))).unwrap();
    request("/rig/mine", key, secret, None, "GET", 30, 60, false)
        .map(|d| jsonx::as_list(&d).into_iter().filter(|r| re.is_match(&jsonx::s(jsonx::get(r, "description")))).cloned().collect())
        .unwrap_or_default()
}

/// Get-MiningRigRentalServers (Data/mrrservers.json as fallback and copy)
pub fn servers() -> Vec<Value> {
    let path = data::base_dir().join("Data").join("mrrservers.json");
    let mut local: Vec<Value> = std::fs::read_to_string(&path).ok().and_then(|t| jsonx::parse(&t)).map(|v| jsonx::as_list(&v).into_iter().cloned().collect()).unwrap_or_default();
    if local.is_empty() && !path.exists() {
        log_warn!("mrrservers.json missing in Data folder! Cannot run MiningRigRentals");
    }
    if let Ok(r) = web::Cached::new(&format!("{}/info/servers", BASE)).tag("MiningRigRentals").cycletime(86400).json() {
        if jsonx::truthy(jsonx::get(&r, "success")) {
            let mut remote: Vec<Value> = jsonx::as_list(jsonx::get(&r, "data")).into_iter().cloned().collect();
            let names = |v: &[Value]| {
                let mut n: Vec<String> = v.iter().map(|x| jsonx::s(jsonx::get(x, "name"))).collect();
                n.sort();
                n
            };
            if names(&remote) != names(&local) || !path.exists() {
                remote.sort_by(|a, b| {
                    let ra = regex::Regex::new("^(eu|us)").unwrap().is_match(&jsonx::s(jsonx::get(a, "region")));
                    let rb = regex::Regex::new("^(eu|us)").unwrap().is_match(&jsonx::s(jsonx::get(b, "region")));
                    rb.cmp(&ra).then(jsonx::s(jsonx::get(a, "name")).cmp(&jsonx::s(jsonx::get(b, "name"))))
                });
                let _ = std::fs::write(&path, serde_json::to_string_pretty(&remote).unwrap_or_default());
                local = remote;
            }
        }
    }
    local
}

#[derive(Debug, Clone)]
struct RigInfo {
    rigid: String,
    port: String,
    server: String,
    updated: DateTime<Utc>,
}

static INFO_CACHE: LazyLock<Mutex<Option<HashMap<String, RigInfo>>>> = LazyLock::new(|| Mutex::new(None));

/// Get-MiningRigInfo: the MRR server and port of the rigs (Data/mrrinfo.json)
fn rig_info(ids: &[String], key: &str, secret: &str, cachetime: i64) -> Vec<RigInfo> {
    let path = data::base_dir().join("Data").join("mrrinfo.json");
    let mut guard = INFO_CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(|| {
        let mut m = HashMap::new();
        if let Some(v) = std::fs::read_to_string(&path).ok().and_then(|t| jsonx::parse(&t)) {
            for e in jsonx::as_list(&v) {
                let id = jsonx::s(jsonx::get(e, "rigid"));
                let updated = DateTime::parse_from_rfc3339(&jsonx::s(jsonx::get(e, "updated"))).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now() - Duration::days(365));
                m.insert(id.clone(), RigInfo { rigid: id, port: jsonx::s(jsonx::get(e, "port")), server: jsonx::s(jsonx::get(e, "server")), updated });
            }
        }
        m
    });
    let now = Utc::now();
    let mut need: Vec<String> = ids.iter().filter(|id| cache.get(*id).map(|i| i.port == "error" || i.updated < now - Duration::seconds(cachetime)).unwrap_or(true)).cloned().collect();
    need.sort();
    if !need.is_empty() {
        let mut updated = 0;
        if let Some(d) = request(&format!("/rig/{}/port", need.join(";")), key, secret, None, "GET", 120, 0, false) {
            for e in jsonx::as_list(&d) {
                let id = jsonx::s(jsonx::get(e, "rigid"));
                cache.insert(id.clone(), RigInfo { rigid: id, port: jsonx::s(jsonx::get(e, "port")), server: jsonx::s(jsonx::get(e, "server")), updated: now });
                updated += 1;
            }
        }
        if updated > 0 {
            let v: Vec<Value> = cache.values().map(|i| json!({"rigid": i.rigid, "port": i.port, "server": i.server, "updated": i.updated.to_rfc3339()})).collect();
            let _ = std::fs::write(&path, Value::Array(v).to_string());
        }
    }
    ids.iter().filter_map(|id| cache.get(id).cloned()).collect()
}

// ------------------------------------------------------------------------------------------
// rig status ($Session.MRRStatus, Set-/Get-MiningRigRentalStatus)
// ------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RigStatus {
    next: DateTime<Utc>,
    wait: bool,
    enable: bool,
    extended: bool,
    extensionmessagesent: bool,
    startmessagesent: bool,
    diffmessagesent: bool,
    diffisbadsince: Option<DateTime<Utc>>,
    poolofflinemessagesent: bool,
}

#[derive(Default)]
struct Session {
    status: HashMap<i64, RigStatus>,
    benchmark_status: HashMap<String, bool>,
    rental_timestamp: HashMap<String, DateTime<Utc>>,
    warned_auto: bool,
}

static SESSION: LazyLock<Mutex<Session>> = LazyLock::new(|| Mutex::new(Session::default()));

fn status_stop(rig: i64) {
    SESSION.lock().unwrap().status.remove(&rig);
}

fn status_get(rig: i64) -> Option<RigStatus> {
    SESSION.lock().unwrap().status.get(&rig).cloned()
}

fn status_set(rig: i64, status: &str, until_offline: i64, until_retry: i64) -> bool {
    let mut s = SESSION.lock().unwrap();
    let time = Utc::now();
    let (until_offline, until_retry) = (until_offline.max(180), until_retry.max(180));
    match s.status.get_mut(&rig) {
        Some(r) => {
            match status {
                "extended" => r.extended = true,
                "notextended" => r.extended = false,
                "extensionmessagesent" => r.extensionmessagesent = true,
                "startmessagesent" => r.startmessagesent = true,
                "diffmessagesent" => r.diffmessagesent = true,
                "diffisok" => {
                    r.diffisbadsince = None;
                    r.diffmessagesent = false;
                }
                "diffisbad" => {
                    if r.diffisbadsince.is_none() {
                        r.diffisbadsince = Some(time);
                    }
                }
                "poolofflinemessagesent" => r.poolofflinemessagesent = true,
                "online" => {
                    r.next = time;
                    r.wait = false;
                    r.enable = true;
                    r.poolofflinemessagesent = false;
                }
                _ => {
                    if time >= r.next {
                        if r.wait {
                            r.next = time + Duration::seconds(until_retry);
                            r.wait = false;
                            r.enable = false;
                        } else {
                            r.next = time + Duration::seconds(until_offline);
                            r.wait = true;
                            r.enable = true;
                        }
                    }
                }
            }
            r.enable
        }
        None => {
            s.status.insert(
                rig,
                RigStatus {
                    next: time,
                    wait: false,
                    enable: true,
                    extended: status == "extended",
                    extensionmessagesent: status == "extensionmessagesent",
                    startmessagesent: status == "startmessagesent",
                    diffmessagesent: status == "diffmessagesent",
                    diffisbadsince: None,
                    poolofflinemessagesent: status == "poolofflinemessagesent",
                },
            );
            true
        }
    }
}

// ------------------------------------------------------------------------------------------
// rental stats (Stats/MRR/<worker>_rental.txt) and rental options (Config/mrr-<id>.config.txt)
// ------------------------------------------------------------------------------------------

fn stats_dir() -> PathBuf {
    let p = data::base_dir().join("Stats").join("MRR");
    let _ = std::fs::create_dir_all(&p);
    p
}

fn rental_stat_get(worker: &str, rental_id: i64) -> Option<Value> {
    let path = stats_dir().join(format!("{}_rental.txt", worker));
    let v = std::fs::read_to_string(&path).ok().and_then(|t| jsonx::parse(&t));
    match v {
        Some(v) if rental_id == 0 || jsonx::num(jsonx::get(&v, "id")) as i64 == rental_id => Some(v),
        _ => {
            if path.exists() {
                log_warn!("Stat file {}_rental.txt is obsolete and will be removed. ", worker);
                let _ = std::fs::remove_file(&path);
            }
            None
        }
    }
}

fn rental_stat_set(worker: &str, v: &Value) {
    if std::fs::write(stats_dir().join(format!("{}_rental.txt", worker)), serde_json::to_string_pretty(v).unwrap_or_default()).is_err() {
        log_warn!("Could not write MRR rental stat file for worker {}, rental id {}", worker, jsonx::s(jsonx::get(v, "id")));
    }
}

fn mrr_time(s: &str) -> Option<DateTime<Utc>> {
    let t = regex::Regex::new(r"\s+UTC$").unwrap().replace(s.trim(), "Z").into_owned();
    DateTime::parse_from_rfc3339(&t.replace(' ', "T")).ok().map(|d| d.with_timezone(&Utc))
}

fn name_list(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => regex::Regex::new(r"[^A-Za-z0-9,;]+").unwrap().replace_all(s, "").split([',', ';']).filter(|x| !x.is_empty()).map(|x| x.to_string()).collect(),
        Value::Array(_) => jsonx::str_list(v),
        _ => vec![],
    }
}

fn hours_fmt(h: f64) -> String {
    let secs = (h * 3600.0).max(0.0) as i64;
    format!("{:02}h{:02}m{:02}s", secs / 3600, (secs / 60) % 60, secs % 60)
}

/// "1.5" / "30%" / "1,5" -> number
fn num_field(s: &str) -> f64 {
    regex::Regex::new(r"[^0-9\.]+").unwrap().replace_all(&s.replace(',', "."), "").parse().unwrap_or(0.0)
}

// ------------------------------------------------------------------------------------------
// MRR pings (Scripts/MRRPing.ps1, Invoke-PingStratum)
// ------------------------------------------------------------------------------------------

struct Ping {
    server: String,
    port: u16,
    user: String,
    worker: String,
    ethproxy: bool,
    wait: bool,
}

static PING_RUNNING: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

fn ping_stratum(p: &Ping, ethproxy: bool, version: &str) -> bool {
    use std::net::ToSocketAddrs;
    let req = if ethproxy {
        format!(r#"{{"id": 1, "method": "login", "params": {{"login": "{}", "pass": "x", "rigid": "{}", "agent": "RainbowMiner/{}"}}}}"#, p.user, p.worker, version)
    } else {
        format!(r#"{{"id": 1, "method": "mining.subscribe", "params": ["{}"]}}"#, p.user)
    };
    let wait = p.wait || ethproxy;
    let Some(addr) = (p.server.as_str(), p.port).to_socket_addrs().ok().and_then(|mut a| a.next()) else { return false };
    let Ok(mut s) = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(3)) else { return false };
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(3)));
    let _ = s.set_write_timeout(Some(std::time::Duration::from_secs(3)));
    if writeln!(s, "{}", req).is_err() {
        return false;
    }
    if !wait {
        return true;
    }
    let mut line = String::new();
    if BufReader::new(&s).read_line(&mut line).is_err() {
        return false;
    }
    jsonx::parse(&line).map(|r| jsonx::num(jsonx::get(&r, "id")) == 1.0 && !jsonx::truthy(jsonx::get(&r, "error"))).unwrap_or(false)
}

fn start_pings(pings: Vec<Ping>) {
    let mut running = PING_RUNNING.lock().unwrap();
    if *running || pings.is_empty() {
        return;
    }
    *running = true;
    let version = crate::VERSION.to_string();
    std::thread::spawn(move || {
        for p in pings.iter().filter(|p| !p.server.is_empty() && p.port > 0) {
            if !ping_stratum(p, p.ethproxy, &version) && p.ethproxy {
                ping_stratum(p, false, &version);
            }
        }
        *PING_RUNNING.lock().unwrap() = false;
    });
}

// ------------------------------------------------------------------------------------------
// the pool module
// ------------------------------------------------------------------------------------------

pub struct MiningRigRentals;

struct Opts {
    user: String,
    key: String,
    secret: String,
    enable_mining: bool,
    enable_auto_extend: bool,
    enable_auto_extend_diff_check: bool,
    allow_extensions: bool,
    allow_rental_during_pause: bool,
    maintenance: bool,
    exclude_rental_ids: Vec<i64>,
    start_message: String,
    diff_message: String,
    extension_message: String,
    pool_offline_message: String,
    extension_message_hours: f64,
    diff_message_secs: i64,
    pool_offline_secs: i64,
    pool_offline_retry_secs: i64,
    pause_between_rentals_secs: i64,
    auto_extend_target_percent: String,
    auto_extend_maximum_percent: String,
    auto_bonus_extend_for_hours: String,
    auto_bonus_extend_by_hours: String,
    auto_bonus_extend_times: String,
    diff_message_tolerancy_percent: String,
    use_host: String,
}

fn opts(ctx: &PoolCtx) -> Opts {
    let b = |k: &str| {
        let v = ctx.field(k);
        !v.is_empty() && util::get_yes_str(&v)
    };
    let s = |k: &str, d: &str| {
        let v = ctx.field(k);
        if v.is_empty() {
            d.to_string()
        } else {
            v
        }
    };
    let t = |k: &str, d: &str| util::convert_from_time(&s(k, d));
    Opts {
        user: ctx.field("User"),
        key: ctx.field("API_Key"),
        secret: ctx.field("API_Secret"),
        enable_mining: b("EnableMining"),
        enable_auto_extend: b("EnableAutoExtend"),
        enable_auto_extend_diff_check: b("EnableAutoExtendDifficultyCheck"),
        allow_extensions: b("AllowExtensions"),
        allow_rental_during_pause: b("AllowRentalDuringPause"),
        maintenance: b("EnableMaintenanceMode"),
        exclude_rental_ids: ctx.field("ExcludeRentalId").split([',', ';', ' ']).filter_map(|x| x.trim().parse().ok()).collect(),
        start_message: ctx.field("StartMessage").trim().to_string(),
        diff_message: ctx.field("DiffMessage").trim().to_string(),
        extension_message: ctx.field("ExtensionMessage").trim().to_string(),
        pool_offline_message: ctx.field("PoolOfflineMessage").trim().to_string(),
        extension_message_hours: t("ExtensionMessageTime", "") as f64 / 3600.0,
        diff_message_secs: t("DiffMessageTime", ""),
        pool_offline_secs: t("PoolOfflineTime", "10m"),
        pool_offline_retry_secs: t("PoolOfflineRetryTime", "15m"),
        pause_between_rentals_secs: t("PauseBetweenRentals", "0"),
        auto_extend_target_percent: s("AutoExtendTargetPercent", "100"),
        auto_extend_maximum_percent: s("AutoExtendMaximumPercent", "30"),
        auto_bonus_extend_for_hours: s("AutoBonusExtendForHours", "0"),
        auto_bonus_extend_by_hours: s("AutoBonusExtendByHours", "0"),
        auto_bonus_extend_times: s("AutoBonusExtendTimes", "0"),
        diff_message_tolerancy_percent: s("DiffMessageTolerancyPercent", "10"),
        use_host: ctx.field("UseHost"),
    }
}

/// a worker's override in mrr.config.txt, else the pool's value
fn wcfg(mrr: &Value, worker: &str, k: &str, default: &str) -> String {
    let v = jsonx::path(mrr, &[worker, k]);
    if v.is_null() || jsonx::s(v).is_empty() {
        default.to_string()
    } else {
        jsonx::s(v)
    }
}

fn read_json(p: &std::path::Path) -> Value {
    std::fs::read_to_string(p).ok().and_then(|t| jsonx::parse(&t)).unwrap_or(Value::Null)
}

impl PoolModule for MiningRigRentals {
    fn name(&self) -> &'static str {
        "MiningRigRentals"
    }
    fn get_pools(&self, ctx: &PoolCtx) -> Result<Vec<Pool>> {
        let name = &ctx.name;
        if ctx.info_only {
            let mut p = ctx.pool("");
            p.Algorithm0 = String::new();
            p.Currency = "BTC".into();
            p.Protocol = "stratum+tcp".into();
            p.PoolFee = POOL_FEE;
            p.Wallet = ctx.wallet("BTC").unwrap_or("").to_string();
            p.Worker = ctx.worker.clone();
            return Ok(vec![p]);
        }
        let o = opts(ctx);
        if o.key.is_empty() || o.secret.is_empty() {
            return Ok(vec![]);
        }
        let default_view = EngineView::default();
        let ev = ctx.engine.unwrap_or(&default_view);
        {
            let mut s = SESSION.lock().unwrap();
            if !s.warned_auto && ["EnableAutoCreate", "EnableAutoUpdate", "EnableAutoPrice", "EnableAutoBenchmark"].iter().any(|k| util::get_yes_str(&ctx.field(k)) && !ctx.field(k).is_empty()) {
                s.warned_auto = true;
                log_warn!("{}: the automatic rig creation, price update and benchmark of the original are not ported yet - manage the rigs on miningrigrentals.com", name);
            }
        }

        // the workers: per device model and the main worker
        let mut workers: Vec<String> = Vec::new();
        for m in &ctx.config.device_model {
            if let Some(w) = ctx.config.device(m).map(|d| d.worker.clone()).filter(|w| !w.is_empty()) {
                if !workers.contains(&w) {
                    workers.push(w);
                }
            }
        }
        if !workers.contains(&ctx.worker) {
            workers.push(ctx.worker.clone());
        }
        let split = |s: String| -> Vec<String> { s.split([',', ';', ' ']).filter(|x| !x.is_empty()).map(|x| x.to_string()).collect() };
        let (use_wn, excl_wn) = (split(ctx.field("UseWorkerName")), split(ctx.field("ExcludeWorkerName")));
        if !use_wn.is_empty() || !excl_wn.is_empty() {
            workers.retain(|w| (use_wn.is_empty() || use_wn.contains(w)) && (excl_wn.is_empty() || !excl_wn.contains(w)));
        }
        if workers.is_empty() {
            return Ok(vec![]);
        }
        let mut all_rigs = rigs(&o.key, &o.secret, &workers);
        let mut maintenance = o.maintenance;
        if maintenance {
            log_warn!("{}: Maintenance mode activated - all unrented rigs disabled.", name);
        } else if ev.pause_rentals {
            log_info!("{}: Paused by scheduler - all unrented rigs disabled.", name);
            maintenance = true;
        }
        let mut out = Vec::new();
        if all_rigs.is_empty() {
            return Ok(out);
        }
        let all_hosts = servers();
        let region_of = |r: &str| data::get_region(&regex::Regex::new("^eu-").unwrap().replace(r, ""));
        let is_rented = |r: &Value| jsonx::s(jsonx::path(r, &["status", "status"])) == "rented" || jsonx::truthy(jsonx::path(r, &["status", "rented"]));

        // sha256 rigs that are not rented are removed (like the original)
        let mut remove: Vec<String> = all_rigs.iter().filter(|r| jsonx::s(jsonx::get(r, "type")) == "sha256" && !is_rented(r)).map(|r| jsonx::s(jsonx::get(r, "id"))).collect();
        if !remove.is_empty() {
            remove.sort();
            request(&format!("/rig/{}", remove.join(";")), &o.key, &o.secret, None, "DELETE", 15, 0, false);
            all_rigs.retain(|r| !remove.contains(&jsonx::s(jsonx::get(r, "id"))));
        }
        let mrr_config = read_json(&ev.config_dir.join("mrr.config.txt"));
        let mrr_algos = read_json(&ev.config_dir.join("mrralgorithms.config.txt"));
        let algo_enabled = |a: &str| {
            let e = jsonx::path(&mrr_algos, &[a, "Enable"]);
            e.is_null() || util::get_yes(e)
        };
        let rigs_of = |all: &[Value], w: &str| -> Vec<usize> {
            let tag = format!("[{}]", w);
            all.iter().enumerate().filter(|(_, r)| jsonx::s(jsonx::get(r, "description")).contains(&tag)).map(|(i, _)| i).collect()
        };

        // devices and models per worker
        let mut workers_devices: HashMap<String, Vec<String>> = HashMap::new();
        let mut workers_models: HashMap<String, Vec<String>> = HashMap::new();
        let mut devices_rented: Vec<String> = Vec::new();
        for w1 in &workers {
            let idx = rigs_of(&all_rigs, w1);
            if idx.is_empty() {
                continue;
            }
            let models: Vec<String> = ctx.config.devices.iter().filter(|(_, d)| d.worker == *w1).map(|(m, _)| m.clone()).collect();
            let devs: Vec<&(String, String, String)> = ev.devices.iter().filter(|(_, m, t)| (*w1 == ctx.worker && t.eq_ignore_ascii_case("Gpu")) || (*w1 != ctx.worker && util::contains_ci(&models, m))).collect();
            let mut dn: Vec<String> = devs.iter().map(|d| d.0.clone()).collect();
            dn.dedup();
            let mut dm: Vec<String> = devs.iter().map(|d| d.1.clone()).collect();
            dm.sort();
            dm.dedup();
            if idx.iter().any(|i| is_rented(&all_rigs[*i])) {
                for d in &dn {
                    if !devices_rented.contains(d) {
                        devices_rented.push(d.clone());
                    }
                }
            }
            workers_devices.insert(w1.clone(), dn);
            workers_models.insert(w1.clone(), dm);
            let mut s = SESSION.lock().unwrap();
            s.benchmark_status.entry(w1.clone()).or_insert(false);
            s.rental_timestamp.entry(w1.clone()).or_insert(Utc::now() - Duration::days(7));
        }

        // the rental options files
        let mut rental_options: Vec<(i64, PathBuf, Value, bool)> = std::fs::read_dir(&ev.config_dir)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| {
                        let f = e.file_name().to_string_lossy().to_string();
                        let id: i64 = regex::Regex::new(r"^mrr-(\d+)").unwrap().captures(&f).filter(|_| f.ends_with(".config.txt"))?[1].parse().ok()?;
                        let v = read_json(&e.path());
                        if v.is_null() {
                            log_warn!("{}: Error in config file {}", name, f);
                        }
                        Some((id, e.path(), v, false))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut pings: Vec<Ping> = Vec::new();
        let interval = ev.interval.max(ev.current_interval).max(1);

        for w1 in &workers {
            let idx = rigs_of(&all_rigs, w1);
            if idx.is_empty() {
                continue;
            }
            let allow_pause = {
                let v = jsonx::path(&mrr_config, &[w1, "AllowRentalDuringPause"]);
                if v.is_null() || jsonx::s(v).is_empty() {
                    o.allow_rental_during_pause
                } else {
                    util::get_yes(v)
                }
            };
            let mut info_cachetime = 86400;
            let rented_count = idx.iter().filter(|i| is_rented(&all_rigs[**i])).count();
            let avail = |r: &Value| jsonx::s(jsonx::get(r, "available_status")) == "available";
            let set_avail = |ids: &[String], status: &str, all_rigs: &mut Vec<Value>| {
                if ids.is_empty() {
                    return;
                }
                put(&format!("/rig/{}", ids.join(";")), &o.key, &o.secret, json!({"status": status}));
                for r in all_rigs.iter_mut() {
                    if ids.contains(&jsonx::s(jsonx::get(r, "id"))) {
                        jsonx::set(r, "available_status", Value::String(status.to_string()));
                    }
                }
            };
            if rented_count > 0 {
                let mut dis: Vec<String> = idx.iter().map(|i| &all_rigs[*i]).filter(|r| !is_rented(r) && avail(r)).map(|r| jsonx::s(jsonx::get(r, "id"))).collect();
                dis.sort();
                set_avail(&dis, "disabled", &mut all_rigs);
                for d in &dis {
                    status_stop(d.parse().unwrap_or(0));
                }
                SESSION.lock().unwrap().rental_timestamp.insert(w1.clone(), Utc::now());
                info_cachetime = 2 * ev.interval.max(60);
            } else {
                let mut valid: Vec<String> = Vec::new();
                let wd = workers_devices.get(w1).cloned().unwrap_or_default();
                let benchmarking = ev.benchmarking_devices.iter().any(|d| wd.contains(d));
                let was_benchmarking = SESSION.lock().unwrap().benchmark_status.get(w1).copied().unwrap_or(false);
                if benchmarking {
                    SESSION.lock().unwrap().benchmark_status.insert(w1.clone(), true);
                } else if was_benchmarking {
                    SESSION.lock().unwrap().benchmark_status.insert(w1.clone(), false);
                } else if !maintenance && ((allow_pause && !ev.paused_by_error) || !ev.paused) {
                    let since = (Utc::now() - SESSION.lock().unwrap().rental_timestamp.get(w1).copied().unwrap_or(Utc::now())).num_seconds();
                    if o.pause_between_rentals_secs == 0 || o.pause_between_rentals_secs < since {
                        let models = workers_models.get(w1).cloned().unwrap_or_default();
                        let dev_algos: Vec<String> = models.iter().filter_map(|m| ctx.config.device(m)).flat_map(|d| d.algorithm.clone()).collect();
                        let dev_excl: Vec<String> = models.iter().filter_map(|m| ctx.config.device(m)).flat_map(|d| d.exclude_algorithm.clone()).collect();
                        let active: Vec<String> = ev
                            .miners
                            .iter()
                            .filter(|m| m.enabled && util::contains_ci(&models, &m.device_model) && (m.exclude_pool_name.is_empty() || !m.exclude_pool_name.to_lowercase().contains(&name.to_lowercase())))
                            .filter_map(|m| m.algorithm.first().map(|a| a.split('-').next().unwrap_or(a).to_string()))
                            .collect();
                        let cfg_algo = ctx.config.arr("Algorithm");
                        let cfg_excl = ctx.config.arr("ExcludeAlgorithm");
                        let rented_overlap = devices_rented.iter().any(|d| wd.contains(d));
                        for i in &idx {
                            let r = &all_rigs[*i];
                            let a = algorithm(&jsonx::s(jsonx::get(r, "type")));
                            if algo_enabled(&a)
                                && util::contains_ci(&active, &a)
                                && !((!cfg_algo.is_empty() && !util::contains_ci(&cfg_algo, &a))
                                    || util::contains_ci(&cfg_excl, &a)
                                    || (!ctx.cfg.algorithm.is_empty() && !util::contains_ci(&ctx.cfg.algorithm, &a))
                                    || util::contains_ci(&ctx.cfg.exclude_algorithm, &a)
                                    || rented_overlap
                                    || (!dev_algos.is_empty() && !util::contains_ci(&dev_algos, &a))
                                    || util::contains_ci(&dev_excl, &a))
                            {
                                valid.push(jsonx::s(jsonx::get(r, "id")));
                            }
                        }
                    } else {
                        log_warn!("{}: Wait {:.1} minutes for {} rigs to be re-enabled.", name, (o.pause_between_rentals_secs - since) as f64 / 60.0, w1);
                    }
                }
                let mut en: Vec<String> = idx.iter().map(|i| &all_rigs[*i]).filter(|r| !avail(r) && valid.contains(&jsonx::s(jsonx::get(r, "id")))).map(|r| jsonx::s(jsonx::get(r, "id"))).collect();
                en.sort();
                set_avail(&en, "available", &mut all_rigs);
                let mut dis: Vec<String> = idx.iter().map(|i| &all_rigs[*i]).filter(|r| avail(r) && !valid.contains(&jsonx::s(jsonx::get(r, "id")))).map(|r| jsonx::s(jsonx::get(r, "id"))).collect();
                dis.sort();
                set_avail(&dis, "disabled", &mut all_rigs);
                for i in &idx {
                    status_stop(jsonx::num(jsonx::get(&all_rigs[*i], "id")) as i64);
                }
            }
            let mut ids: Vec<String> = idx.iter().map(|i| &all_rigs[*i]).filter(|r| avail(r)).map(|r| jsonx::s(jsonx::get(r, "id"))).collect();
            ids.sort();
            if ids.is_empty() {
                continue;
            }
            let info = rig_info(&ids, &o.key, &o.secret, info_cachetime);
            if info.is_empty() {
                log_warn!("Pool API ({}) rig {} info request has failed. This is mostly a temporary problem with the {} API and RainbowMiner will retry automatically. If the warning persists, clear the cache to force fresh data. ", name, w1, name);
                return Ok(out);
            }
            let model = if *w1 != ctx.worker {
                let mut ms: Vec<String> = ctx.config.device_model.iter().filter(|m| ctx.config.device(m).map(|d| d.worker == *w1).unwrap_or(false)).cloned().collect();
                ms.sort();
                ms.dedup();
                ms.join("-")
            } else if ev.devices.iter().any(|d| d.2.eq_ignore_ascii_case("Cpu")) {
                "GPU".to_string()
            } else {
                String::new()
            };
            for i in idx.clone() {
                let r = all_rigs[i].clone();
                if !avail(&r) {
                    continue;
                }
                let rig_id = jsonx::num(jsonx::get(&r, "id")) as i64;
                let rtype = jsonx::s(jsonx::get(&r, "type"));
                let algo = algorithm(&rtype);
                let coin_symbol = coin(&rtype).to_string();
                let algo_m = if model.is_empty() { algo.clone() } else { format!("{}-{}", algo, model) };
                let od_min = jsonx::num(jsonx::path(&r, &["optimal_diff", "min"]));
                let od_max = jsonx::num(jsonx::path(&r, &["optimal_diff", "max"]));
                let div = divisor(&jsonx::s(jsonx::path(&r, &["price", "type"])));
                let btc_price = jsonx::num(jsonx::path(&r, &["price", "BTC", "price"]));
                let stat_name = format!("{}_{}{}_Profit", name, algo, if *w1 != ctx.worker && !model.is_empty() { format!("-{}", model) } else { String::new() });
                let mut sp = SetStat::new(&stat_name, btc_price / div, ctx.stat_span);
                sp.quiet = true;
                let st = stat::set_stat(sp);
                let Some(rig) = info.iter().find(|x| x.rigid == rig_id.to_string() && x.port != "error") else { continue };
                let mut price = st.get_average(&ctx.stat_average);
                let mut currency = "BTC".to_string();
                let mut rig_enable = false;
                let mut rented = is_rented(&r);
                let mut pool_status = jsonx::s(jsonx::get(&r, "poolstatus"));
                let mut miner_name = Vec::new();
                let mut exclude_miner_name = Vec::new();
                let mut use_host = o.use_host.clone();
                if rented {
                    let running: Option<&crate::pool::EngineMiner> = ev.miners.iter().find(|m| m.running && m.pool.iter().any(|p| p == name) && m.algorithm.contains(&algo_m));
                    let rental_id = jsonx::num(jsonx::get(&r, "rental_id")) as i64;
                    let ropt = match rental_options.iter_mut().find(|x| x.0 == rental_id) {
                        Some(x) => {
                            x.3 = true;
                            x.2.clone()
                        }
                        None => {
                            let v = json!({"UseHost": "", "PoolOfflineTime": "", "PoolOfflineRetryTime": "", "MinerName": "", "ExcludeMinerName": ""});
                            let _ = std::fs::write(ev.config_dir.join(format!("mrr-{}.config.txt", rental_id)), serde_json::to_string_pretty(&v).unwrap_or_default());
                            v
                        }
                    };
                    let tsec = |k: &str, d: i64| {
                        let v = jsonx::s(jsonx::get(&ropt, k));
                        if v.is_empty() {
                            d
                        } else {
                            util::convert_from_time(&v)
                        }
                    };
                    let offline_time = tsec("PoolOfflineTime", o.pool_offline_secs);
                    let retry_time = tsec("PoolOfflineRetryTime", o.pool_offline_retry_secs);
                    miner_name = name_list(jsonx::get(&ropt, "MinerName"));
                    exclude_miner_name = name_list(jsonx::get(&ropt, "ExcludeMinerName"));
                    if !jsonx::s(jsonx::get(&ropt, "UseHost")).is_empty() {
                        use_host = jsonx::s(jsonx::get(&ropt, "UseHost"));
                    }
                    let pool_was_offline = pool_status == "offline";
                    if pool_was_offline {
                        if let Some(m) = running {
                            if m.last_accepted >= 0 && m.last_accepted < offline_time {
                                pool_status = "online".into();
                            }
                        }
                    }
                    rig_enable = status_set(rig_id, &pool_status, offline_time, retry_time);
                    if rig_enable && o.exclude_rental_ids.contains(&rental_id) {
                        log_info!("{}: rig id #{} disabled, because it is on the ExcludeRentalId list in pools.config.txt", name, rig_id);
                        rig_enable = false;
                    }
                    let mut rs = status_get(rig_id);
                    if pool_was_offline {
                        let msg = if pool_status == "online" {
                            "ignored because we find shares".to_string()
                        } else {
                            let secs = rs.as_ref().map(|s| (s.next - Utc::now()).num_seconds().max(0)).unwrap_or(0);
                            if rs.as_ref().map(|s| s.wait).unwrap_or(false) {
                                format!("waiting {}s for online status", secs)
                            } else {
                                format!("rig offline for {}s", secs)
                            }
                        };
                        log_info!("{}: rig #{} renter's pool is offline - {}", name, rig_id, msg);
                    }
                    let hours = jsonx::num(jsonx::path(&r, &["status", "hours"]));
                    let check_auto_extend = hours < 0.25 && !rs.as_ref().map(|s| s.extended).unwrap_or(false);
                    let check_ext_msg = o.allow_extensions
                        && o.extension_message_hours > 0.0
                        && o.extension_message.len() > 3
                        && hours < o.extension_message_hours
                        && !rs.as_ref().map(|s| s.extensionmessagesent).unwrap_or(false);
                    let cache_time = ((hours * 3600.0) as i64 - 5 * interval).max(0);
                    let mut force = (o.enable_auto_extend && check_auto_extend) || check_ext_msg || cache_time == 0;
                    let saved = rental_stat_get(w1, rental_id);
                    if !force {
                        if let Some(end) = saved.as_ref().and_then(|s| mrr_time(&jsonx::s(jsonx::get(s, "end")))) {
                            if Utc::now() + Duration::seconds(cache_time) > end {
                                force = true;
                                log_info!("{}: Force status update for rental #{} on {}", name, rental_id, w1);
                            }
                        }
                    }
                    let result = match request(&format!("/rental/{}", rental_id), &o.key, &o.secret, None, "GET", 60, cache_time, force) {
                        Some(v) if jsonx::num(jsonx::get(&v, "id")) as i64 == rental_id => {
                            rental_stat_set(w1, &v);
                            Some(v)
                        }
                        Some(v) => Some(v),
                        None => {
                            log_warn!("{}: Status update for rental #{} went wrong", name, rental_id);
                            saved
                        }
                    };
                    let (mut adv_hr, mut avg_hr, mut rental_price) = (0.0, 0.0, 0.0);
                    if let Some(res) = &result {
                        let pc = jsonx::get(res, "price_converted");
                        rental_price = jsonx::num(jsonx::get(pc, "advertised")) / util::convert_from_hash(&format!("1{}", jsonx::s(jsonx::get(pc, "type"))));
                        if rental_price != 0.0 {
                            price = rental_price;
                            let cur = jsonx::s(jsonx::get(pc, "currency"));
                            if cur != "BTC" && !cur.is_empty() {
                                currency = cur;
                                let rc = jsonx::s(jsonx::path(res, &["price", "currency"]));
                                let other = jsonx::num(jsonx::path(&r, &["price", &rc, "price"]));
                                if other != 0.0 {
                                    price *= btc_price / other;
                                }
                            }
                        }
                        if force {
                            let hr = jsonx::get(res, "hashrate");
                            adv_hr = jsonx::num(jsonx::path(hr, &["advertised", "hash"])) * util::convert_from_hash(&format!("1{}", jsonx::s(jsonx::path(hr, &["advertised", "type"]))));
                            avg_hr = jsonx::num(jsonx::path(hr, &["average", "hash"])) * util::convert_from_hash(&format!("1{}", jsonx::s(jsonx::path(hr, &["average", "type"]))));
                        }
                    }
                    let algo_mapped = data::get_mapped_algorithm(&algo, true);
                    let diff_subst = |cur: Option<f64>| -> Vec<(&'static str, String)> {
                        let mut v = vec![
                            ("Algorithm", algo_mapped.clone()),
                            ("Coin", coin_symbol.clone()),
                            ("Type", rtype.clone()),
                            ("MinDiff", round_diff(od_min)),
                            ("MaxDiff", round_diff(od_max)),
                            ("MinDiffFmt", util::convert_to_float(od_min).replace(' ', "")),
                            ("MaxDiffFmt", util::convert_to_float(od_max).replace(' ', "")),
                        ];
                        if let Some(c) = cur {
                            v.push(("CurrentDiff", if c >= 10.0 { format!("{}", c.round()) } else { format!("{}", c) }));
                            v.push(("CurrentDiffFmt", util::convert_to_float(c).replace(' ', "")));
                        }
                        v
                    };
                    // start message
                    if !o.start_message.is_empty() && !rs.as_ref().map(|s| s.startmessagesent).unwrap_or(false) {
                        let mut sent = false;
                        if let Some(res) = &result {
                            let len = jsonx::num(jsonx::get(res, "length")) + jsonx::num(jsonx::get(res, "extended")) - jsonx::num(jsonx::path(res, &["rig", "status", "hours"]));
                            if jsonx::truthy(jsonx::get(res, "length")) && len < 5.0 / 60.0 {
                                let m = if od_min > 0.0 && od_max > 0.0 {
                                    regex::Regex::new("<[/]?diff>").unwrap().replace_all(&o.start_message, "").into_owned()
                                } else {
                                    regex::Regex::new("<diff>.+</diff>").unwrap().replace_all(&o.start_message, "").into_owned()
                                };
                                let m = subst(&m, &diff_subst(None));
                                sent = put(&format!("/rental/{}/message", rental_id), &o.key, &o.secret, json!({"message": m})).is_some();
                            }
                        }
                        log_info!("{}: Start message {}sent to rental #{} for {} on {}", name, if sent { "" } else { "NOT " }, rental_id, algo, w1);
                        status_set(rig_id, "startmessagesent", 180, 900);
                    }
                    // pool offline message
                    if !rig_enable && !o.pool_offline_message.is_empty() && pool_status == "offline" && !rs.as_ref().map(|s| s.poolofflinemessagesent).unwrap_or(false) {
                        let sent = put(&format!("/rental/{}/message", rental_id), &o.key, &o.secret, json!({"message": o.pool_offline_message})).is_some();
                        log_info!("{}: Pool offline message {}sent to rental #{} for {} on {}", name, if sent { "" } else { "NOT " }, rental_id, algo, w1);
                        status_set(rig_id, "poolofflinemessagesent", 180, 900);
                    }
                    // the difficulty of the renter's pool
                    let tolerancy = {
                        let a = jsonx::s(jsonx::path(&mrr_algos, &[&algo, "DiffMessageTolerancyPercent"]));
                        let v = if !a.is_empty() { a } else { wcfg(&mrr_config, w1, "DiffMessageTolerancyPercent", &o.diff_message_tolerancy_percent) };
                        num_field(&v) / 100.0
                    };
                    let pool_diff = || -> f64 {
                        let d = request(&format!("/rig/{}/threads", rig_id), &o.key, &o.secret, None, "GET", 15, 0, false)
                            .map(|t| jsonx::num(jsonx::path(jsonx::idx(jsonx::get(&t, "threads"), 0), &["difficulty", "share"])))
                            .unwrap_or(0.0);
                        if d != 0.0 {
                            d
                        } else {
                            running.and_then(|m| m.difficulty.iter().find(|(a, _)| *a == algo_m).map(|(_, d)| *d)).unwrap_or(0.0)
                        }
                    };
                    // auto extend
                    if o.enable_auto_extend {
                        if check_auto_extend {
                            let mut set_status = true;
                            if avg_hr > 0.0 && adv_hr > 0.0 && avg_hr < adv_hr {
                                let res = result.clone().unwrap_or(Value::Null);
                                let target = num_field(&wcfg(&mrr_config, w1, "AutoExtendTargetPercent", &o.auto_extend_target_percent)) / 100.0;
                                let maxp = num_field(&wcfg(&mrr_config, w1, "AutoExtendMaximumPercent", &o.auto_extend_maximum_percent)) / 100.0;
                                let extended = jsonx::num(jsonx::get(&res, "extended"));
                                let length = jsonx::num(jsonx::get(&res, "length"));
                                let mut extend_by = (length + extended) * (target * adv_hr / avg_hr - 1.0);
                                if extend_by > 0.0 && maxp > 0.0 {
                                    let mut bonus = 0.0;
                                    if extended > 0.0 {
                                        let for_h = num_field(&wcfg(&mrr_config, w1, "AutoBonusExtendForHours", &o.auto_bonus_extend_for_hours));
                                        let by_h = num_field(&wcfg(&mrr_config, w1, "AutoBonusExtendByHours", &o.auto_bonus_extend_by_hours));
                                        let mut times = num_field(&wcfg(&mrr_config, w1, "AutoBonusExtendTimes", &o.auto_bonus_extend_times));
                                        if times <= 0.0 {
                                            times = 1000.0;
                                        }
                                        if for_h > 0.0 && by_h > 0.0 {
                                            bonus = (length / for_h).floor().min(times) * by_h;
                                            if bonus > extended {
                                                bonus = 0.0;
                                            }
                                        }
                                    }
                                    extend_by = (length * maxp - extended + bonus).min(extend_by);
                                }
                                if o.enable_auto_extend_diff_check {
                                    let d = pool_diff();
                                    if d != 0.0 && od_min > 0.0 && od_max > 0.0 {
                                        let bad = if d < (1.0 - tolerancy) * od_min {
                                            Some(format!("{} < {}", round_diff(d), round_diff(od_min)))
                                        } else if d > (1.0 + tolerancy) * od_max {
                                            Some(format!("{} > {}", round_diff(d), round_diff(od_max)))
                                        } else {
                                            None
                                        };
                                        if let Some(b) = bad {
                                            extend_by = 0.0;
                                            log_info!("{}: No auto-extention due to renter pool difficulty {} for rental #{} for {} on {}", name, b, rental_id, algo, w1);
                                        }
                                    }
                                }
                                let extend_by = (extend_by * 100.0).round() / 100.0;
                                if extend_by >= 1.0 / 6.0 {
                                    let ok = put(&format!("/rig/{}/extend", rig_id), &o.key, &o.secret, json!({"hours": extend_by})).is_some();
                                    if ok {
                                        log_info!("{}: Extended rental #{} for {} on {} for {} hours.", name, rental_id, algo, w1, extend_by);
                                    } else {
                                        set_status = false;
                                    }
                                }
                            }
                            if set_status {
                                status_set(rig_id, "extended", 180, 900);
                            }
                        } else if hours > 0.25 && rs.as_ref().map(|s| s.extended).unwrap_or(false) {
                            status_set(rig_id, "notextended", 180, 900);
                        }
                    }
                    // extension message
                    if check_ext_msg {
                        let adv_p = adv_hr * rental_price;
                        let avg_p = avg_hr * rental_price;
                        let rig_p = jsonx::num(jsonx::path(&r, &["hashrate", "advertised", "hash"]))
                            * util::convert_from_hash(&format!("1{}", jsonx::s(jsonx::path(&r, &["hashrate", "advertised", "type"]))))
                            * (btc_price / util::convert_from_hash(&format!("1{}", jsonx::s(jsonx::path(&r, &["price", "type"])))));
                        let mut sent = false;
                        if adv_p - (adv_p - avg_p).abs() >= rig_p {
                            sent = put(&format!("/rental/{}/message", rental_id), &o.key, &o.secret, json!({"message": o.extension_message})).is_some();
                        }
                        log_info!("{}: Extension message {}sent to rental #{} for {} on {}", name, if sent { "" } else { "NOT " }, rental_id, algo, w1);
                        status_set(rig_id, "extensionmessagesent", 180, 900);
                    }
                    // difficulty message
                    if !o.diff_message.is_empty() {
                        let d = pool_diff();
                        let mut ok = true;
                        if d != 0.0 && ((od_min > 0.0 && od_max > 0.0 && d < (1.0 - tolerancy) * od_min) || d > (1.0 + tolerancy) * od_max) {
                            ok = false;
                            status_set(rig_id, "diffisbad", 180, 900);
                            rs = status_get(rig_id);
                            let since_ok = rs.as_ref().and_then(|s| s.diffisbadsince).map(|t| t < Utc::now() - Duration::seconds(o.diff_message_secs)).unwrap_or(false);
                            if !rs.as_ref().map(|s| s.diffmessagesent).unwrap_or(false) && since_ok {
                                let m = subst(&o.diff_message, &diff_subst(Some(d)));
                                let sent = put(&format!("/rental/{}/message", rental_id), &o.key, &o.secret, json!({"message": m})).is_some();
                                log_info!("{}: Difficulty message {}sent to rental #{} for {} on {}", name, if sent { "" } else { "NOT " }, rental_id, algo, w1);
                                status_set(rig_id, "diffmessagesent", 180, 900);
                            }
                        }
                        if ok {
                            status_set(rig_id, "diffisok", 180, 900);
                        }
                    }
                    // manual override to end rentals in case of server failure
                    if let Some(end) = result.as_ref().and_then(|x| mrr_time(&jsonx::s(jsonx::get(x, "end")))) {
                        if Utc::now() - Duration::minutes(15) > end {
                            rented = false;
                            rig_enable = false;
                            log_warn!("{}: cannot reach MRR, manually disable rental #{} on {} that ended {}.", name, jsonx::s(jsonx::get(result.as_ref().unwrap(), "id")), w1, end);
                        }
                    }
                }

                if rented || pool_status == "online" || o.enable_mining {
                    let mut server = rig.server.clone();
                    let mut port: u32 = rig.port.parse().unwrap_or(0);
                    if !use_host.is_empty() && rig_enable {
                        if let Some(h) = all_hosts.iter().find(|h| jsonx::s(jsonx::get(h, "name")).to_lowercase().starts_with(&use_host.to_lowercase())) {
                            let hn = jsonx::s(jsonx::get(h, "name"));
                            if hn != server {
                                server = hn;
                                if !(3300..3400).contains(&port) {
                                    port = 3333;
                                }
                            }
                        }
                    }
                    let is33 = (3300..3400).contains(&port);
                    // failover servers: same or near regions first
                    let my_region = region_of(&jsonx::s(jsonx::get(&r, "region")));
                    let alt = data::get_region2(&my_region);
                    let mut failover: Vec<String> = Vec::new();
                    if !alt.is_empty() {
                        let mut cand: Vec<&Value> = all_hosts.iter().filter(|h| jsonx::s(jsonx::get(h, "name")) != server).collect();
                        let pfx: String = server.chars().take(2).collect();
                        let key = |h: &Value| {
                            let rg = region_of(&jsonx::s(jsonx::get(h, "region")));
                            let ix = alt.iter().position(|x| *x == rg).map(|i| 100 - i as i64).unwrap_or(0);
                            (ix, jsonx::s(jsonx::get(h, "region")).starts_with(&pfx), 100 - jsonx::num(jsonx::get(h, "id")) as i64)
                        };
                        cand.sort_by(|a, b| key(b).cmp(&key(a)));
                        failover.extend(cand.into_iter().take(2).map(|h| jsonx::s(jsonx::get(h, "name"))));
                    }
                    if failover.is_empty() {
                        for p in ["us", "eu"] {
                            if let Some(h) = all_hosts.iter().find(|h| jsonx::s(jsonx::get(h, "name")) != server && jsonx::s(jsonx::get(h, "region")).starts_with(p)) {
                                failover.push(jsonx::s(jsonx::get(h, "name")));
                            }
                        }
                    }
                    if let Some(h) = all_hosts.iter().map(|h| jsonx::s(jsonx::get(h, "name"))).find(|n| *n != server && !failover.contains(n)) {
                        failover.push(h);
                    }
                    let sep = if ["ProgPowVeil", "ProgPowZ", "Ubqhash"].iter().any(|a| a.eq_ignore_ascii_case(&algo)) { "*" } else { "." };
                    let user = format!("{}{}{}", o.user, sep, rig_id);
                    let (mut wallet, mut worker) = (String::new(), w1.clone());
                    if algo == "XelisHash" {
                        worker = user.clone();
                        wallet = user.clone();
                    }
                    for ssl in [false, true] {
                        let mut p = ctx.pool(&algo_m).with_stat(&Some(st.clone()), &ctx.stat_average, &ctx.stat_average_stable);
                        p.Algorithm0 = algo.clone();
                        p.CoinName = if rented { hours_fmt(jsonx::num(jsonx::path(&r, &["status", "hours"]))) } else { String::new() };
                        p.CoinSymbol = coin_symbol.clone();
                        p.Currency = currency.clone();
                        p.Price = price;
                        p.Protocol = super::protocol(ssl);
                        p.Host = server.clone();
                        p.Port = port;
                        p.User = user.clone();
                        p.Pass = "x".into();
                        p.Region = region_of(&jsonx::s(jsonx::get(&r, "region")));
                        p.SSL = ssl;
                        p.PoolFee = POOL_FEE;
                        p.Exclusive = rented && rig_enable;
                        p.Idle = if rented && rig_enable { false } else { !o.enable_mining };
                        p.Failover = failover
                            .iter()
                            .map(|h| Failover { Protocol: "stratum+tcp".into(), Host: h.clone(), Port: if is33 { port } else { 3333 }, User: user.clone(), Pass: "x".into() })
                            .collect();
                        p.EthMode = if is33 && data::RE_ALGO_HAS_DAGSIZE.is_match(&algo) { "qtminer".into() } else { String::new() };
                        p.Wallet = wallet.clone();
                        p.Worker = worker.clone();
                        p.MinerName = miner_name.clone();
                        p.ExcludeMinerName = exclude_miner_name.clone();
                        out.push(p);
                    }
                }
                if !rig_enable {
                    pings.push(Ping {
                        server: rig.server.clone(),
                        port: rig.port.parse().unwrap_or(0),
                        user: format!("{}.{}", o.user, rig_id),
                        worker: w1.clone(),
                        ethproxy: rig.port.len() == 4 && rig.port.starts_with("33"),
                        wait: rented,
                    });
                }
            }
        }
        // the options of rentals that ended
        for (_, path, _, ok) in &rental_options {
            if !ok && path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
        start_pings(pings);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn helpers() {
        assert_eq!(super::hmac_sha1_hex("key", "The quick brown fox jumps over the lazy dog"), "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9");
        assert_eq!(super::divisor("mh*day"), 1e6);
        assert_eq!(super::hours_fmt(1.5), "01h30m00s");
        assert_eq!(super::subst("a %Coin% b", &[("Coin", "RVN".into())]), "a RVN b");
        assert_eq!(super::mrr_time("2026-11-16 10:00:00 UTC").map(|t| t.timestamp()), Some(1794823200));
        assert_eq!(super::name_list(&serde_json::json!("TRex, gminer;x")), vec!["TRex", "gminer", "x"]);
    }
}

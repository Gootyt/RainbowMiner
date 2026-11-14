//! The HTTP API and web dashboard (Modules/API.psm1 + Scripts/API.ps1): the core publishes its
//! state into `STATE`, the listener threads serve it next to the files of web/.

use crate::jsonx;
use crate::rates;
use crate::stat;
use crate::util;
use crate::{log_info, log_warn};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

#[derive(Debug, Default)]
pub struct ApiState {
    // published by the core
    pub version: Value,
    pub info: Value,
    pub active_miners: Value,
    pub running_miners: Value,
    pub failed_miners: Value,
    pub miners_needing_benchmark: Value,
    pub minerinfo: Value,
    pub minerspeeds: Value,
    pub pools: Value,
    pub all_pools: Value,
    pub new_pools: Value,
    pub algorithms: Value,
    pub miners: Value,
    pub fastest_miners: Value,
    pub devices: Value,
    pub all_devices: Value,
    pub device_combos: Value,
    pub config: Value,
    pub user_config: Value,
    pub download_list: Value,
    pub watchdog_timers: Value,
    pub crash_counter: Value,
    pub memory_skipped: Value,
    pub balances: Value,
    pub actual_rates: Value,
    pub earnings_avg: f64,
    pub earnings_1d: f64,
    pub current_profit: f64,
    pub current_power: Value,
    pub current_power_price: f64,
    pub cpuinfo: Value,
    pub sysinfo: Value,
    pub cmd_menu: Vec<String>,
    pub pause: bool,
    pub pause_ia: bool,
    pub pause_ia_only: bool,
    pub lock_miners_state: bool,
    pub is_exclusive_run: bool,
    pub remote_ip: String,
    pub machine_name: String,
    pub start_time: Option<DateTime<Utc>>,
    // commands from the API
    pub cmd_stop: bool,
    pub cmd_pause: bool,
    pub cmd_update_balance: bool,
    pub cmd_watchdog_reset: bool,
    pub cmd_clear_cache: bool,
    pub cmd_lock_miners: bool,
    pub cmd_key: String,
    // listener settings
    pub auth: Option<(String, String)>,
    pub allow_ips: Vec<String>,
    pub rand_tag: String,
}

pub static STATE: LazyLock<Mutex<ApiState>> = LazyLock::new(|| Mutex::new(ApiState::default()));

pub fn with<R>(f: impl FnOnce(&mut ApiState) -> R) -> R {
    let mut s = STATE.lock().unwrap();
    f(&mut s)
}

/// read and reset a command flag
pub fn take(f: impl FnOnce(&mut ApiState) -> &mut bool) -> bool {
    let mut s = STATE.lock().unwrap();
    let v = f(&mut s);
    let r = *v;
    *v = false;
    r
}

fn mime(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain",
        "xml" => "text/xml",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    }
}

/// Test-IPInRange: exact, wildcard ("192.168.0.*") or CIDR ("192.168.0.0/24")
pub fn ip_in_range(ip: &str, pat: &str) -> bool {
    let pat = pat.trim();
    if pat.is_empty() {
        return false;
    }
    if let Some((net, bits)) = pat.split_once('/') {
        let to_u32 = |s: &str| -> Option<u32> {
            let p: Vec<u32> = s.split('.').map(|x| x.parse::<u32>().ok()).collect::<Option<Vec<_>>>()?;
            if p.len() != 4 {
                return None;
            }
            Some((p[0] << 24) | (p[1] << 16) | (p[2] << 8) | p[3])
        };
        let (Some(a), Some(b), Ok(bits)) = (to_u32(ip), to_u32(net), bits.parse::<u32>()) else { return false };
        if bits == 0 {
            return true;
        }
        let mask = if bits >= 32 { u32::MAX } else { !((1u32 << (32 - bits)) - 1) };
        return a & mask == b & mask;
    }
    crate::device::like(ip, pat)
}

fn uptime(start: Option<DateTime<Utc>>) -> Value {
    let secs = start.map(|s| (Utc::now() - s).num_seconds()).unwrap_or(0).max(0);
    json!({"AsString": format!("{}.{:02}:{:02}:{:02}", secs / 86400, (secs / 3600) % 24, (secs / 60) % 60, secs % 60), "Seconds": secs})
}

fn system_uptime() -> Value {
    let secs = sysinfo::System::uptime() as i64;
    json!({"AsString": format!("{}.{:02}:{:02}:{:02}", secs / 86400, (secs / 3600) % 24, (secs / 60) % 60, secs % 60), "Seconds": secs})
}

fn parse_query(s: &str) -> HashMap<String, Vec<String>> {
    let mut m: HashMap<String, Vec<String>> = HashMap::new();
    for part in s.split('&') {
        if part.is_empty() {
            continue;
        }
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        let dec = |x: &str| url::form_urlencoded::parse(format!("a={}", x).as_bytes()).next().map(|(_, v)| v.to_string()).unwrap_or_default();
        m.entry(dec(k).to_lowercase().trim_end_matches("[]").to_string()).or_default().push(dec(v));
    }
    m
}

/// remove the stats files of one miner (resetonebenchmark.ps1)
fn reset_benchmark(base: &Path, name: &str, algorithm: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut algo = algorithm.split('-').next().unwrap_or("").to_string();
    let re_gpu = regex::Regex::new(r"-GPU.+$").unwrap();
    if re_gpu.replace(name, "").split('-').count() > 2 {
        algo = "*".into();
    }
    let first = name.split('-').next().unwrap_or("");
    let rest = name.split_once('-').map(|x| x.1.replace('-', "*")).unwrap_or_default();
    let pat = format!("*-{}-{}*_{}_HashRate.txt", first, rest, algo);
    if let Ok(rd) = std::fs::read_dir(base.join("Stats").join("Miners")) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if crate::device::like(&n, &pat) {
                let _ = std::fs::remove_file(e.path());
                stat::remove_cached(&regex::Regex::new(r"^(AMD|CPU|INTEL|NVIDIA)-").unwrap().replace(n.trim_end_matches(".txt"), "").to_string());
                out.push(n);
            }
        }
    }
    out
}

fn web_script(base: &Path, name: &str, q: &HashMap<String, Vec<String>>) -> Option<String> {
    let p = |k: &str| q.get(k).cloned().unwrap_or_default();
    let stats = base.join("Stats");
    let walk = |suffix: &str| -> Vec<PathBuf> {
        let mut v = Vec::new();
        let mut stack = vec![stats.clone()];
        while let Some(d) = stack.pop() {
            if let Ok(rd) = std::fs::read_dir(&d) {
                for e in rd.flatten() {
                    let path = e.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if e.file_name().to_string_lossy().to_lowercase().ends_with(suffix) {
                        v.push(path);
                    }
                }
            }
        }
        v
    };
    match name {
        "resetallbenchmarks.ps1" => {
            let files = walk("hashrate.txt");
            for f in &files {
                let _ = std::fs::remove_file(f);
            }
            for k in stat::cached_keys().into_iter().filter(|k| k.ends_with("_hashrate")) {
                stat::remove_cached(&k);
            }
            Some(format!("Removed {} stats files", files.len()))
        }
        "resettotals.ps1" => {
            let files = walk("total.txt");
            for f in &files {
                let _ = std::fs::remove_file(f);
            }
            Some(format!("Removed {} total files", files.len()))
        }
        "resetzerohashrateminers.ps1" => {
            let mut text = String::new();
            let mut count = 0;
            if let Ok(rd) = std::fs::read_dir(stats.join("Miners")) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    if !n.to_lowercase().ends_with("hashrate.txt") {
                        continue;
                    }
                    let v = util::read_text(&e.path()).ok().and_then(|t| jsonx::parse(&t));
                    if v.map(|v| jsonx::num(jsonx::get(&v, "Minute")) == 0.0).unwrap_or(true) {
                        let _ = std::fs::remove_file(e.path());
                        text.push_str(&n);
                        text.push('\n');
                        count += 1;
                    }
                }
            }
            Some(format!("Removed {} stat files:\n<pre>\n{}</pre>", count, text))
        }
        "resetonebenchmark.ps1" | "resetselectedbenchmark.ps1" => {
            let (names, algos, models) = (p("name"), p("algorithm"), p("devicemodel"));
            let mut text = String::new();
            let mut count = 0;
            for i in 0..names.len() {
                let model = models.get(i).cloned().unwrap_or_default();
                for f in reset_benchmark(base, &names[i], algos.get(i).map(|s| s.as_str()).unwrap_or("")) {
                    count += 1;
                    let stem = f.trim_end_matches(".txt");
                    text.push_str(&format!(
                        "{}/{}/{}\n",
                        regex::Regex::new(r"-(C|G)PU.+$").unwrap().replace(stem, ""),
                        model,
                        stem.split('_').nth(1).unwrap_or("")
                    ));
                }
            }
            Some(format!("Removed {} stat files:\n<pre>\n{}</pre>", count, text))
        }
        _ => None,
    }
}

struct Reply {
    status: u16,
    body: Vec<u8>,
    content_type: String,
    cache: Option<&'static str>,
    filename: Option<String>,
}

impl Reply {
    fn json(v: &Value) -> Reply {
        Reply {
            status: 200,
            body: serde_json::to_vec(v).unwrap_or_default(),
            content_type: "application/json".into(),
            cache: None,
            filename: None,
        }
    }
    fn text(s: &str, ct: &str, status: u16) -> Reply {
        Reply {
            status,
            body: s.as_bytes().to_vec(),
            content_type: ct.into(),
            cache: None,
            filename: None,
        }
    }
}

fn or_null(v: &Value) -> Option<Reply> {
    if v.is_null() {
        None
    } else {
        Some(Reply::json(v))
    }
}

fn console_json(base: &Path, s: &ApiState) -> Value {
    let lines = crate::log::console_lines();
    let content: Vec<String> = lines.iter().map(|l| util::strip_ansi(l)).collect();
    let miners: Vec<Value> = jsonx::as_list(&s.running_miners)
        .iter()
        .filter_map(|m| {
            let lf = jsonx::s(jsonx::get(m, "LogFile"));
            if lf.is_empty() {
                return None;
            }
            let text = std::fs::read(&lf).ok()?;
            let text = String::from_utf8_lossy(&text);
            let tail: Vec<&str> = text.lines().rev().take(20).collect::<Vec<_>>().into_iter().rev().collect();
            Some(json!({
                "Name": format!("{} {}", jsonx::s(jsonx::get(m, "DeviceModel")), jsonx::s(jsonx::get(m, "BaseName"))),
                "Content": tail.iter().map(|l| util::strip_ansi(l)).collect::<Vec<_>>().join("\n"),
            }))
        })
        .collect();
    let _ = base;
    json!({
        "Content": content.join("\n"),
        "Miners": miners,
        "Timestamp": util::unix_timestamp(),
        "CmdMenu": s.cmd_menu,
        "CmdKey": s.cmd_key,
    })
}

fn route(base: &Path, path: &str, q: &HashMap<String, Vec<String>>) -> Option<Reply> {
    let p1 = |k: &str| q.get(k).and_then(|v| v.first().cloned()).unwrap_or_default();
    let mut s = STATE.lock().unwrap();
    let r = match path {
        "/version" => or_null(&s.version),
        "/info" => or_null(&s.info),
        "/remoteip" => Some(Reply::json(&json!(s.remote_ip))),
        "/console" => Some(Reply::json(&console_json(base, &s))),
        "/cmdkey" => {
            s.cmd_key = p1("cmdkey");
            Some(Reply::json(&json!(s.cmd_key)))
        }
        "/cpuinfo" => or_null(&s.cpuinfo),
        "/sysinfo" => or_null(&s.sysinfo),
        "/uptime" => Some(Reply::json(&uptime(s.start_time))),
        "/systemuptime" => Some(Reply::json(&system_uptime())),
        "/isserver" => Some(Reply::json(&json!(false))),
        "/activeminers" => or_null(&s.active_miners),
        "/runningminers" => or_null(&s.running_miners),
        "/failedminers" => or_null(&s.failed_miners),
        "/remoteminers" => Some(Reply::json(&json!([]))),
        "/minersneedingbenchmark" => or_null(&s.miners_needing_benchmark),
        "/minerinfo" => or_null(&s.minerinfo),
        "/minerspeeds" => or_null(&s.minerspeeds),
        "/pools" => or_null(&s.pools),
        "/allpools" => or_null(&s.all_pools),
        "/newpools" => or_null(&s.new_pools),
        "/algorithms" => or_null(&s.algorithms),
        "/miners" => or_null(&s.miners),
        "/fastestminers" => or_null(&s.fastest_miners),
        "/disabled" => {
            let mut v: Vec<String> = stat::get_disabled().into_iter().collect();
            v.sort();
            Some(Reply::json(&json!(v)))
        }
        "/config" => or_null(&s.config),
        "/userconfig" => or_null(&s.user_config),
        "/downloadlist" => or_null(&s.download_list),
        "/alldevices" => or_null(&s.all_devices),
        "/devices" => or_null(&s.devices),
        "/devicecombos" => or_null(&s.device_combos),
        "/stats" => Some(Reply::json(&stat::api_stats())),
        "/totals" => {
            let dir = base.join("Stats").join("Totals");
            let mut v = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().ends_with("_Total.txt") {
                        if let Some(j) = util::read_text(&e.path()).ok().and_then(|t| jsonx::parse(&t)) {
                            v.push(j);
                        }
                    }
                }
            }
            Some(Reply::json(&Value::Array(v)))
        }
        "/watchdogtimers" => or_null(&s.watchdog_timers),
        "/memoryskipped" => or_null(&s.memory_skipped),
        "/crashcounter" => or_null(&s.crash_counter),
        "/balances" => or_null(&s.balances),
        "/rates" => {
            let r = rates::all();
            if p1("format") == "table" {
                let cur: Vec<String> = jsonx::keys(&s.actual_rates);
                let rows: Vec<Value> = r
                    .iter()
                    .map(|(sym, rate)| {
                        let mut o = serde_json::Map::new();
                        o.insert("symbol".into(), json!(sym));
                        for c in &cur {
                            let rc = r.get(c).copied().unwrap_or(0.0);
                            o.insert(format!("rate{}", c), json!(if *rate != 0.0 { rc / rate } else { 0.0 }));
                        }
                        Value::Object(o)
                    })
                    .collect();
                Some(Reply::json(&Value::Array(rows)))
            } else {
                Some(Reply::json(&json!(r)))
            }
        }
        "/asyncloaderjobs" => Some(Reply::json(&crate::web::jobs_json())),
        "/decsep" => Some(Reply::json(&json!("."))),
        "/currentprofit" => Some(Reply::json(&json!({
            "AllProfitBTC": s.current_profit,
            "ProfitBTC": s.current_profit,
            "Earnings_Avg": s.earnings_avg,
            "Earnings_1d": s.earnings_1d,
            "AllEarnings_Avg": s.earnings_avg,
            "AllEarnings_1d": s.earnings_1d,
            "Rates": s.actual_rates,
            "PowerPrice": s.current_power_price,
            "Power": s.current_power,
            "Uptime": uptime(s.start_time),
            "SysUptime": system_uptime(),
            "RemoteIP": s.remote_ip,
        }))),
        "/stop" => {
            s.cmd_stop = true;
            Some(Reply::text("Stopping", "text/html", 200))
        }
        "/reboot" => Some(Reply::text("Reboot is not supported by the Rust port.", "text/html", 200)),
        "/pause" => {
            let a = p1("action");
            s.cmd_pause = if ["set", "reset", "pause", "unpause"].contains(&a.as_str()) { (a == "set" || a == "pause") != s.pause_ia } else { true };
            Some(Reply::json(&json!(s.cmd_pause)))
        }
        "/lockminers" => {
            s.cmd_lock_miners = true;
            Some(Reply::json(&json!(!s.lock_miners_state)))
        }
        "/updatebalance" => {
            s.cmd_update_balance = true;
            Some(Reply::json(&json!(true)))
        }
        "/watchdogreset" => {
            s.cmd_watchdog_reset = true;
            Some(Reply::json(&json!(true)))
        }
        "/clearcache" => {
            s.cmd_clear_cache = true;
            Some(Reply::json(&json!(true)))
        }
        "/status" => Some(Reply::json(&json!({
            "Pause": s.pause,
            "PauseIAOnly": s.pause_ia_only,
            "LockMiners": {"Locked": s.lock_miners_state},
            "IsExclusiveRun": s.is_exclusive_run,
            "IsDonationRun": false,
        }))),
        "/clients" => Some(Reply::json(&json!([]))),
        _ => None,
    };
    drop(s);
    if r.is_some() {
        return r;
    }
    if let Some(name) = path.strip_prefix("/scripts/") {
        if let Some(t) = web_script(base, name, q) {
            let mut r = Reply::text(&t, "text/html", 200);
            r.cache = Some("no-store");
            return Some(r);
        }
    }
    if path == "/totalscsv" {
        let mut r = Reply::text("", "text/csv", 200);
        r.filename = Some(format!("totals_{}.txt", chrono::Local::now().format("%Y-%m-%d_%H%M%S")));
        return Some(r);
    }
    None
}

fn serve_file(base: &Path, path: &str) -> Reply {
    let rel = if path == "/" { "/index.html" } else { path };
    let web = base.join("web");
    let file = web.join(rel.trim_start_matches('/'));
    // no escape from web/
    let ok = file.canonicalize().ok().zip(web.canonicalize().ok()).map(|(f, w)| f.starts_with(w)).unwrap_or(false);
    if !ok || !file.is_file() {
        return Reply::text(&format!("URI '{}' is not a valid resource.", path), "text/html", 404);
    }
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
    let mut body = std::fs::read(&file).unwrap_or_default();
    if ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm") {
        let mut text = util::decode_text(&body);
        let re = regex::Regex::new(r#"<!-- *#include *file="(.*?)" *-->"#).unwrap();
        let caps: Vec<(String, String)> = re.captures_iter(&text).map(|c| (c[0].to_string(), c[1].to_string())).collect();
        for (all, inc) in caps {
            if let Ok(t) = util::read_text(&web.join(&inc)) {
                text = text.replace(&all, &t);
            }
        }
        body = text.into_bytes();
    }
    Reply {
        status: 200,
        body,
        content_type: mime(&ext).into(),
        cache: Some(if rel.starts_with("/vendor/") {
            "public, max-age=31536000, immutable"
        } else if ["js", "css", "png", "jpg", "jpeg", "gif", "svg", "ico", "woff", "woff2", "json"].contains(&ext.to_lowercase().as_str()) {
            "public, max-age=300"
        } else {
            "no-cache"
        }),
        filename: None,
    }
}

fn authorized(req: &tiny_http::Request) -> bool {
    let (auth, allow) = {
        let s = STATE.lock().unwrap();
        (s.auth.clone(), s.allow_ips.clone())
    };
    let ip = req.remote_addr().map(|a| a.ip().to_string()).unwrap_or_default();
    if !allow.is_empty() && !ip.is_empty() && ip != "127.0.0.1" && ip != "::1" && !allow.iter().any(|p| ip_in_range(&ip, p)) {
        return false;
    }
    if let Some((u, p)) = auth {
        let h = req.headers().iter().find(|h| h.field.equiv("Authorization")).map(|h| h.value.as_str().to_string()).unwrap_or_default();
        let Some(b64) = h.strip_prefix("Basic ") else { return false };
        let Ok(dec) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) else { return false };
        return String::from_utf8_lossy(&dec) == format!("{}:{}", u, p);
    }
    true
}

fn handle(base: &Path, mut req: tiny_http::Request) {
    let url = req.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let mut q = parse_query(query);
    if *req.method() == tiny_http::Method::Post {
        let mut body = String::new();
        let _ = req.as_reader().take(16 * 1024 * 1024).read_to_string(&mut body);
        for (k, v) in parse_query(&body) {
            q.entry(k).or_default().extend(v);
        }
    }
    let reply = if !authorized(&req) {
        Reply::text("Access denied", "text/html", 401)
    } else {
        let rand_tag = STATE.lock().unwrap().rand_tag.clone();
        let path = if !rand_tag.is_empty() && path.contains(&rand_tag) { "/stop" } else { path };
        route(base, path, &q).unwrap_or_else(|| serve_file(base, path))
    };
    let mut resp = tiny_http::Response::from_data(reply.body).with_status_code(reply.status);
    let mut add = |k: &str, v: &str| {
        if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
            resp.add_header(h);
        }
    };
    add("Content-Type", &reply.content_type);
    add("Server", "RainbowMiner API (Rust)");
    add("Access-Control-Allow-Origin", "*");
    if let Some(c) = reply.cache {
        add("Cache-Control", c);
    }
    if let Some(f) = &reply.filename {
        add("Content-Disposition", &format!("attachment; filename={}", f));
    }
    if reply.status == 401 {
        add("WWW-Authenticate", "Basic realm=\"RainbowMiner\"");
    }
    let _ = req.respond(resp);
}

/// Start-APIServer
pub fn start(base: PathBuf, port: u16, threads: usize) -> bool {
    let host = if cfg!(windows) { "127.0.0.1" } else { "0.0.0.0" };
    let server = match tiny_http::Server::http(format!("{}:{}", host, port)) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            log_warn!("Cannot start the API server on port {}: {}", port, e);
            return false;
        }
    };
    for _ in 0..threads.max(1) {
        let s = server.clone();
        let b = base.clone();
        std::thread::spawn(move || {
            for req in s.incoming_requests() {
                handle(&b, req);
            }
        });
    }
    log_info!("Started {} API threads on port {}", threads.max(1), port);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_ranges() {
        assert!(ip_in_range("192.168.1.7", "192.168.1.0/24"));
        assert!(!ip_in_range("192.168.2.7", "192.168.1.0/24"));
        assert!(ip_in_range("10.0.0.5", "10.0.0.*"));
        assert!(ip_in_range("10.0.0.5", "10.0.0.5"));
    }

    #[test]
    fn query() {
        let q = parse_query("a=1&name%5B%5D=x&name[]=y&b=hello%20world");
        assert_eq!(q["a"], vec!["1"]);
        assert_eq!(q["b"], vec!["hello world"]);
        assert_eq!(q["name"], vec!["x", "y"]);
    }
}

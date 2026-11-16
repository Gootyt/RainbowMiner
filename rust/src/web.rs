//! Web helpers (Modules/WebLib.psm1, TcpLib.psm1):
//! - one shared HTTP agent (proxy, user agent, timeouts)
//! - cached REST requests with a cycle time (the Asyncloader of the original: memory + Cache\*.asy)
//! - raw TCP requests for the miner APIs
#![allow(dead_code)]

use crate::jsonx;
use crate::util;
use crate::log_info;
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};

pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36";

/// Get-Unzip: base64 -> gunzip -> UTF-8
pub fn unzip_b64(s: &str) -> String {
    use base64::Engine;
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(s.trim()) else { return String::new() };
    let mut out = String::new();
    let _ = flate2::read::GzDecoder::new(&bytes[..]).read_to_string(&mut out);
    out
}

static RBM_DATA_SECRET: LazyLock<String> =
    LazyLock::new(|| unzip_b64("H4sIAAAAAAAEAAXBQQ6AMAgEwBeRbKEF+pzF4s2L8f9x5q1HDj/KPQ7W0PJItiXFmGbg4Wib2C0V2tkgZselvsSJ7Vq1gE1k/q8nexVMAAAA"));

/// Get-RBMDataAuth: HMAC-SHA256 of the current unix minute
pub fn rbm_data_auth() -> String {
    rbm_data_auth_at(util::unix_timestamp().div_euclid(60))
}

fn rbm_data_auth_at(window: i64) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(RBM_DATA_SECRET.as_bytes()).expect("hmac key");
    mac.update(window.to_string().as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

static RE_RBMINER_URL: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)^https?://(?:[\w-]+\.)*rbminer\.net(?::\d+)?(?:/|$)").unwrap());

/// requests to rbminer.net carry the client version and the data auth (and never the custom user agent)
pub fn is_rbminer_url(url: &str) -> bool {
    RE_RBMINER_URL.is_match(url)
}

fn rbm_headers(url: &str, given: &[(String, String)]) -> Vec<(&'static str, String)> {
    let mut v = Vec::new();
    if is_rbminer_url(url) {
        let has = |k: &str| given.iter().any(|(h, _)| h.eq_ignore_ascii_case(k));
        if !has("X-RBM-Client") {
            v.push(("X-RBM-Client", format!("v{}", crate::VERSION)));
        }
        if !has("X-RBM-Auth") {
            v.push(("X-RBM-Auth", rbm_data_auth()));
        }
        v.push(("User-Agent", DEFAULT_USER_AGENT.to_string()));
    }
    v
}

#[derive(Debug, Clone, Default)]
pub struct WebSettings {
    pub proxy: String,
    pub proxy_username: String,
    pub proxy_password: String,
    pub user_agent: String,
}

struct WebState {
    settings: WebSettings,
    agent: ureq::Agent,
    local_agent: ureq::Agent,
    cache_dir: PathBuf,
    quickstart: bool,
}

fn build_agent(s: &WebSettings, use_proxy: bool) -> ureq::Agent {
    let mut b = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .redirects(5)
        .user_agent(if s.user_agent.is_empty() { DEFAULT_USER_AGENT } else { &s.user_agent });
    if use_proxy && !s.proxy.trim().is_empty() {
        let mut p = s.proxy.trim().to_string();
        if !s.proxy_username.is_empty() {
            // ureq takes the credentials inside the proxy url
            if let Some(idx) = p.find("://") {
                let (scheme, rest) = p.split_at(idx + 3);
                p = format!("{}{}:{}@{}", scheme, s.proxy_username, s.proxy_password, rest);
            } else {
                p = format!("{}:{}@{}", s.proxy_username, s.proxy_password, p);
            }
        }
        match ureq::Proxy::new(&p) {
            Ok(px) => b = b.proxy(px),
            Err(e) => crate::log_warn!("Invalid proxy setting {}: {}", s.proxy, e),
        }
    } else if use_proxy {
        // like the .NET HttpClient of the original: honor HTTPS_PROXY / HTTP_PROXY / ALL_PROXY
        b = b.try_proxy_from_env(true);
    }
    b.build()
}

static STATE: LazyLock<RwLock<WebState>> = LazyLock::new(|| {
    let s = WebSettings::default();
    RwLock::new(WebState {
        agent: build_agent(&s, true),
        local_agent: build_agent(&s, false),
        settings: s,
        cache_dir: PathBuf::from("Cache"),
        quickstart: false,
    })
});

/// Set-Proxy / Initialize-HttpClient. Returns true if the settings changed
pub fn configure(settings: WebSettings) -> bool {
    let mut st = STATE.write().unwrap();
    let changed = st.settings.proxy != settings.proxy
        || st.settings.proxy_username != settings.proxy_username
        || st.settings.proxy_password != settings.proxy_password
        || st.settings.user_agent != settings.user_agent;
    if changed {
        st.agent = build_agent(&settings, true);
        st.local_agent = build_agent(&settings, false);
        st.settings = settings;
    }
    changed
}

pub fn set_cache_dir(p: PathBuf) {
    let _ = std::fs::create_dir_all(&p);
    STATE.write().unwrap().cache_dir = p;
}

pub fn set_quickstart(q: bool) {
    STATE.write().unwrap().quickstart = q;
}

fn agent_for(url: &str) -> ureq::Agent {
    let st = STATE.read().unwrap();
    let local = url.contains("://127.0.0.1") || url.contains("://localhost");
    if local {
        st.local_agent.clone()
    } else {
        st.agent.clone()
    }
}

/// Options of Invoke-GetUrl
#[derive(Debug, Clone)]
pub struct Request {
    pub url: String,
    pub method: String,
    pub body: Option<String>,
    pub headers: Vec<(String, String)>,
    pub timeout: u64,
}

impl Request {
    pub fn get(url: &str) -> Request {
        Request {
            url: url.to_string(),
            method: "GET".into(),
            body: None,
            headers: Vec::new(),
            timeout: 15,
        }
    }
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout = secs.max(1);
        self
    }
    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.to_string(), v.to_string()));
        self
    }
    pub fn post_json(mut self, body: &Value) -> Self {
        self.method = "POST".into();
        self.body = Some(body.to_string());
        self.headers.push(("Content-Type".into(), "application/json".into()));
        self
    }
    pub fn post_form(mut self, body: &str) -> Self {
        self.method = "POST".into();
        self.body = Some(body.to_string());
        self.headers.push(("Content-Type".into(), "application/x-www-form-urlencoded".into()));
        self
    }
    fn key(&self) -> String {
        let h: Vec<String> = self.headers.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        util::md5_hex(&format!("{}{}{}", self.url, self.body.clone().unwrap_or_default(), h.join("&")))
    }
}

/// Invoke-GetUrl: perform the request, return the body text
pub fn fetch_text(req: &Request) -> Result<String> {
    let agent = agent_for(&req.url);
    let mut r = agent
        .request(&req.method, &req.url)
        .timeout(Duration::from_secs(req.timeout))
        .set("Accept", "application/json, text/plain, */*")
        .set("Cache-Control", "no-cache");
    for (k, v) in &req.headers {
        r = r.set(k, v);
    }
    for (k, v) in rbm_headers(&req.url, &req.headers) {
        r = r.set(k, &v);
    }
    let resp = match &req.body {
        Some(b) => r.send_string(b),
        None => r.call(),
    };
    match resp {
        Ok(resp) => {
            let mut s = String::new();
            resp.into_reader().take(64 * 1024 * 1024).read_to_string(&mut s).context("reading response")?;
            Ok(s)
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Err(anyhow!("HTTP {} {}", code, body.chars().take(200).collect::<String>()))
        }
        Err(e) => Err(anyhow!("{}", e)),
    }
}

/// Invoke-GetUrl + ConvertFrom-Json
pub fn fetch_json(req: &Request) -> Result<Value> {
    let t = fetch_text(req)?;
    jsonx::parse(&t).ok_or_else(|| anyhow!("invalid JSON from {}", req.url))
}

pub fn get_json(url: &str, timeout: u64) -> Result<Value> {
    fetch_json(&Request::get(url).timeout(timeout))
}

pub fn get_text(url: &str, timeout: u64) -> Result<String> {
    fetch_text(&Request::get(url).timeout(timeout))
}

/// download a binary file
pub fn download_file(url: &str, dest: &std::path::Path, timeout: u64) -> Result<()> {
    let agent = agent_for(url);
    let mut r = agent.get(url).timeout(Duration::from_secs(timeout));
    for (k, v) in rbm_headers(url, &[]) {
        r = r.set(k, &v);
    }
    let resp = r.call().map_err(|e| anyhow!("{}", e))?;
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = dest.with_extension("part");
    {
        let mut f = std::fs::File::create(&tmp)?;
        let mut rd = resp.into_reader();
        std::io::copy(&mut rd, &mut f)?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

// ----------------------------------------------------------------------------------------
// Cached requests (Invoke-RestMethodAsync / Invoke-GetUrlAsync)
// ----------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CacheEntry {
    fetched: Instant,
    text: String,
    tag: String,
    url: String,
    success: u64,
    fail: u64,
    error: Option<String>,
}

static CACHE: LazyLock<Mutex<HashMap<String, CacheEntry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static HOST_LAST: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn host_of(url: &str) -> String {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(|h| h.to_string())).unwrap_or_default()
}

/// Options of Invoke-RestMethodAsync
#[derive(Debug, Clone)]
pub struct Cached {
    pub req: Request,
    pub tag: String,
    pub cycletime: u64,
    pub delay_ms: u64,
    pub jobkey: Option<String>,
    pub retry: u32,
}

impl Cached {
    pub fn new(url: &str) -> Cached {
        Cached {
            req: Request::get(url),
            tag: String::new(),
            cycletime: 0,
            delay_ms: 0,
            jobkey: None,
            retry: 0,
        }
    }
    pub fn tag(mut self, t: &str) -> Self {
        self.tag = t.to_string();
        self
    }
    pub fn cycletime(mut self, c: u64) -> Self {
        self.cycletime = c;
        self
    }
    pub fn delay(mut self, d: u64) -> Self {
        self.delay_ms = d.min(5000);
        self
    }
    pub fn timeout(mut self, t: u64) -> Self {
        self.req.timeout = t.max(1);
        self
    }
    pub fn jobkey(mut self, k: &str) -> Self {
        self.jobkey = Some(k.to_string());
        self
    }
    pub fn retry(mut self, r: u32) -> Self {
        self.retry = r.min(5);
        self
    }
    pub fn request(mut self, r: Request) -> Self {
        self.req = r;
        self
    }

    /// returns the body text (fresh, cached or stale)
    pub fn text(&self) -> Result<String> {
        let key = self.jobkey.clone().unwrap_or_else(|| self.req.key());
        let cycle = Duration::from_secs(self.cycletime.max(1));
        let (cache_dir, quickstart) = {
            let st = STATE.read().unwrap();
            (st.cache_dir.clone(), st.quickstart)
        };
        let file = cache_dir.join(format!("{}.asy", key));

        {
            let c = CACHE.lock().unwrap();
            if let Some(e) = c.get(&key) {
                if e.fetched.elapsed() < cycle && e.url == self.req.url {
                    return Ok(e.text.clone());
                }
            }
        }

        // first use: a cache file from the last run may be used, if it is young enough (or quickstart)
        let mem_known = CACHE.lock().unwrap().contains_key(&key);
        if !mem_known {
            if let Ok(meta) = std::fs::metadata(&file) {
                let age = meta
                    .modified()
                    .ok()
                    .and_then(|m| SystemTime::now().duration_since(m).ok())
                    .unwrap_or(Duration::MAX);
                if quickstart || age < cycle {
                    if let Ok(t) = util::read_text(&file) {
                        if !t.trim().is_empty() {
                            let fetched = Instant::now().checked_sub(age.min(cycle)).unwrap_or_else(Instant::now);
                            CACHE.lock().unwrap().insert(
                                key.clone(),
                                CacheEntry {
                                    fetched,
                                    text: t.clone(),
                                    tag: self.tag.clone(),
                                    url: self.req.url.clone(),
                                    success: 0,
                                    fail: 0,
                                    error: None,
                                },
                            );
                            return Ok(t);
                        }
                    }
                }
            }
        }

        // rate limit per host
        if self.delay_ms > 0 {
            let host = host_of(&self.req.url);
            let wait = {
                let mut hl = HOST_LAST.lock().unwrap();
                let now = Instant::now();
                let w = hl
                    .get(&host)
                    .map(|t| Duration::from_millis(self.delay_ms).saturating_sub(now.duration_since(*t)))
                    .unwrap_or(Duration::ZERO);
                hl.insert(host, now + w);
                w
            };
            if !wait.is_zero() {
                std::thread::sleep(wait);
            }
        }

        let mut result = fetch_text(&self.req);
        let mut tries = 0;
        while result.is_err() && tries < self.retry {
            std::thread::sleep(Duration::from_millis(250));
            result = fetch_text(&self.req);
            tries += 1;
        }
        let result = result.and_then(|t| {
            if t.trim().is_empty() {
                Err(anyhow!("Empty request"))
            } else {
                Ok(t)
            }
        });

        let mut c = CACHE.lock().unwrap();
        match result {
            Ok(t) => {
                let _ = util::write_file_atomic(&file, t.as_bytes());
                let e = c.entry(key).or_insert(CacheEntry {
                    fetched: Instant::now(),
                    text: String::new(),
                    tag: self.tag.clone(),
                    url: self.req.url.clone(),
                    success: 0,
                    fail: 0,
                    error: None,
                });
                e.fetched = Instant::now();
                e.text = t.clone();
                e.url = self.req.url.clone();
                e.success += 1;
                e.error = None;
                Ok(t)
            }
            Err(err) => {
                let msg = format!("Problem fetching {}: {}", self.req.url, err);
                if let Some(e) = c.get_mut(&key) {
                    e.fail += 1;
                    e.error = Some(msg.clone());
                    // retry earlier than a full cycle, but not every call
                    e.fetched = Instant::now().checked_sub(cycle.saturating_sub(Duration::from_secs(30))).unwrap_or_else(Instant::now);
                    if !e.text.is_empty() {
                        log_info!("{} (using cached data)", msg);
                        return Ok(e.text.clone());
                    }
                }
                drop(c);
                if let Ok(t) = util::read_text(&file) {
                    if !t.trim().is_empty() {
                        return Ok(t);
                    }
                }
                Err(anyhow!(msg))
            }
        }
    }

    pub fn json(&self) -> Result<Value> {
        let t = self.text()?;
        match jsonx::parse(&t) {
            Some(v) => Ok(v),
            None => {
                // clutter in the cache: drop it
                let key = self.jobkey.clone().unwrap_or_else(|| self.req.key());
                CACHE.lock().unwrap().remove(&key);
                let cache_dir = STATE.read().unwrap().cache_dir.clone();
                let _ = std::fs::remove_file(cache_dir.join(format!("{}.asy", key)));
                Err(anyhow!("Job {} contains clutter.", key))
            }
        }
    }
}

/// Invoke-RestMethodAsync shortcut
pub fn rest_async(url: &str, tag: &str, cycletime: u64, delay: u64, timeout: u64) -> Result<Value> {
    Cached::new(url).tag(tag).cycletime(cycletime).delay(delay).timeout(timeout).json()
}

/// Stop-AsyncJob -tag: forget the cached requests of a pool
pub fn forget_tag(tag: &str) {
    CACHE.lock().unwrap().retain(|_, e| !e.tag.eq_ignore_ascii_case(tag));
}

/// clear the request cache (key [E])
pub fn clear_cache() {
    CACHE.lock().unwrap().clear();
    let dir = STATE.read().unwrap().cache_dir.clone();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.path().extension().and_then(|x| x.to_str()) == Some("asy") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// /asyncloaderjobs
pub fn jobs_json() -> Value {
    let c = CACHE.lock().unwrap();
    Value::Array(
        c.iter()
            .map(|(k, e)| {
                serde_json::json!({
                    "Key": k, "Url": e.url, "Tag": e.tag, "Success": e.success, "Fail": e.fail,
                    "Error": e.error, "AgeSeconds": e.fetched.elapsed().as_secs()
                })
            })
            .collect(),
    )
}

// ----------------------------------------------------------------------------------------
// TCP
// ----------------------------------------------------------------------------------------

/// Invoke-TcpRequest (plain TCP request/response, used by the miner APIs)
pub fn tcp_request(server: &str, port: u16, request: &str, timeout: u64, read_to_end: bool, no_newline: bool) -> Result<String> {
    let server = if server == "localhost" { "127.0.0.1" } else { server };
    let addr = (server, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow!("cannot resolve {}", server))?;
    let t = Duration::from_secs(timeout.max(1));
    let mut stream = TcpStream::connect_timeout(&addr, t)?;
    stream.set_read_timeout(Some(t))?;
    stream.set_write_timeout(Some(t))?;
    if !request.is_empty() {
        if no_newline {
            stream.write_all(request.as_bytes())?;
        } else {
            stream.write_all(format!("{}\n", request).as_bytes())?;
        }
        stream.flush()?;
    }
    let mut reader = BufReader::new(stream);
    let mut out = String::new();
    if read_to_end {
        let mut buf = Vec::new();
        match reader.read_to_end(&mut buf) {
            Ok(_) => {}
            Err(e) if !buf.is_empty() && (e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.into()),
        }
        out = String::from_utf8_lossy(&buf).into_owned();
    } else {
        reader.read_line(&mut out)?;
    }
    Ok(out.trim_end_matches(['\r', '\n', '\0']).to_string())
}

/// Test-TcpServer: is something listening on host:port?
pub fn tcp_test(server: &str, port: u16, timeout_ms: u64) -> bool {
    let Ok(mut addrs) = (server, port).to_socket_addrs() else { return false };
    let Some(addr) = addrs.next() else { return false };
    TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)).is_ok()
}

/// Test-Internet: one of the websites must answer
pub fn test_internet(domains: &[String]) -> bool {
    for d in domains {
        let host = d.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
        if tcp_test(host, 443, 3000) || tcp_test(host, 80, 3000) {
            return true;
        }
    }
    false
}

/// HMAC-SHA256 hex signature (Get-HMACSignature)
pub fn hmac_sha256_hex(key: &[u8], msg: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key).expect("hmac key");
    mac.update(msg);
    hex::encode(mac.finalize().into_bytes())
}

/// [System.Web.HttpUtility]::UrlEncode: form encoding with lower case hex, "!()*" stay
fn http_utility_encode(s: &str) -> String {
    let e: String = url::form_urlencoded::byte_serialize(s.as_bytes()).collect();
    let mut out = String::with_capacity(e.len());
    let mut chars = e.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h: String = chars.by_ref().take(2).collect();
            match h.as_str() {
                "21" => out.push('!'),
                "28" => out.push('('),
                "29" => out.push(')'),
                _ => {
                    out.push('%');
                    out.push_str(&h.to_lowercase());
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Get-UrlEncode: "{...}" templates stay, the value of "{name:value}" is encoded
pub fn url_encode(s: &str) -> String {
    let re = regex::Regex::new(r"^(.*?)(\{[^}]+\})(.*?)$").unwrap();
    let re_t = regex::Regex::new(r"^\{(\w+):(.*?)\}$").unwrap();
    let mut rest = s.to_string();
    let mut out = String::new();
    loop {
        let Some(c) = re.captures(&rest) else { break };
        let (pre, tmp, post) = (c[1].to_string(), c[2].to_string(), c[3].to_string());
        if !pre.is_empty() {
            out.push_str(&http_utility_encode(&pre));
        }
        match re_t.captures(&tmp) {
            Some(t) => out.push_str(&format!("{{{}:{}}}", &t[1], http_utility_encode(&t[2].replace('$', "*dollar*")).replace("*dollar*", "$"))),
            None => out.push_str(&tmp),
        }
        rest = post;
    }
    if !rest.is_empty() {
        out.push_str(&http_utility_encode(&rest));
    }
    out
}

#[cfg(test)]
mod rbm_tests {
    #[test]
    fn rbm_auth() {
        assert_eq!(super::RBM_DATA_SECRET.len(), 76);
        assert_eq!(super::rbm_data_auth_at(29000000), "ba80eaa209189c992e6f5e41202b7d8422a63add7d836c633ec9eb88699287ca");
        assert!(super::is_rbminer_url("https://api.rbminer.net/data/coins.json"));
        assert!(super::is_rbminer_url("http://rbminer.net"));
        assert!(!super::is_rbminer_url("https://rbminer.net.evil.com/x"));
    }
}

#[cfg(test)]
mod dbg_tests {
    #[test]
    #[ignore]
    fn dbg_fetch() {
        for u in std::env::var("RBM_URLS").unwrap_or_default().split(',') {
            match super::get_text(u, 15) {
                Ok(t) => println!("{} OK {} {}", u, t.len(), t.chars().take(80).collect::<String>()),
                Err(e) => println!("{} ERR {}", u, e),
            }
        }
    }
}

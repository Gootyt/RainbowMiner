//! More miner API classes of Modules/MinerAPIs.psm1 (UpdateMinerData / GetArguments):
//! Cast, Claymore, CryptoDredge, Dstm, DynexsolveWrapper, EnemyZ, Ethminer, EthminerWrapper,
//! MiniZ, Nanominer, Nheq, RH, SrbMiner, TBMiner, TeamblackWrapper, TTminerWrapper,
//! VerthashWrapper and Xgminer.

use crate::jsonx;
use crate::minerapi::{ActiveMiner, MinerStatus, RunCtx};
use crate::util;
use crate::web;
use crate::{log_info, log_warn};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

static RE_ANSI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1B\[[0-?]*[ -/]*[@-~]").unwrap());

/// $Line -replace "`n|`r" -replace "\x1B\[[0-?]*[ -/]*[@-~]"
fn simple_line(line: &str) -> String {
    RE_ANSI.replace_all(&line.replace(['\n', '\r'], ""), "").to_string()
}

/// [Double]"1,5" style conversion
fn num_s(s: &str) -> f64 {
    util::parse_f64(&s.replace(',', ".")).unwrap_or(0.0)
}

/// Th/Gh/Mh/kh multipliers of the Ethminer style outputs
fn eth_unit(u: &str) -> f64 {
    match u {
        "Th" => 1e12,
        "Gh" => 1e9,
        "Mh" => 1e6,
        "kh" => 1e3,
        _ => 1.0,
    }
}

/// switch -Regex ($unit) {"k" ... "P"}: the last matching case wins in PowerShell, but the
/// units only ever contain one of the letters
fn regex_unit(u: &str) -> f64 {
    let mut m = 1.0;
    for (c, f) in [('k', 1e3), ('M', 1e6), ('G', 1e9), ('T', 1e12), ('P', 1e15)] {
        if u.to_lowercase().contains(c.to_ascii_lowercase()) {
            m = f;
        }
    }
    m
}

/// the JSON object between the first "{" and the last "}" with blanks turned to "_" (Xgminer)
fn xgminer_json(resp: &str) -> Option<Value> {
    let a = resp.find('{')?;
    let b = resp.rfind('}')?;
    if b < a {
        return None;
    }
    jsonx::parse(&resp[a..=b].replace(' ', "_"))
}

fn xgminer_hashrate(s: &Value) -> f64 {
    for (k, f) in [
        ("HS_5s", 1.0),
        ("KHS_5s", 1e3),
        ("MHS_5s", 1e6),
        ("GHS_5s", 1e9),
        ("THS_5s", 1e12),
        ("PHS_5s", 1e15),
        ("KHS_30s", 1e3),
        ("MHS_30s", 1e6),
        ("GHS_30s", 1e9),
        ("THS_30s", 1e12),
        ("PHS_30s", 1e15),
        ("HS_av", 1.0),
        ("KHS_av", 1e3),
        ("MHS_av", 1e6),
        ("GHS_av", 1e9),
        ("THS_av", 1e12),
        ("PHS_av", 1e15),
    ] {
        let v = jsonx::num(jsonx::get(s, k));
        if v != 0.0 {
            return v * f;
        }
    }
    0.0
}

/// ($list | Measure-Object -Sum).Sum over the values of an object's properties
fn sum_props(obj: &Value, field: &str) -> f64 {
    jsonx::values(obj).iter().map(|v| jsonx::num(jsonx::get(v, field))).sum()
}

impl ActiveMiner {
    fn tcp(&self, request: &str, read_to_end: bool, no_newline: bool) -> Option<String> {
        match web::tcp_request("127.0.0.1", self.port, request, 10, read_to_end, no_newline) {
            Ok(r) => Some(r),
            Err(_) => {
                log_info!("Failed to connect to miner {}. ", self.name);
                None
            }
        }
    }

    fn tcp_json(&self, request: &str, read_to_end: bool) -> Option<Value> {
        let r = self.tcp(request, read_to_end, false)?;
        match jsonx::parse(&r) {
            Some(v) => Some(v),
            None => {
                log_info!("Failed to connect to miner {}. ", self.name);
                None
            }
        }
    }

    fn one(&self, a: &str, v: f64) -> HashMap<String, f64> {
        let mut m = HashMap::new();
        m.insert(a.to_string(), v);
        m
    }

    /// the more API classes, false if the API is not one of them
    pub fn update_miner_data_more(&mut self, rc: &RunCtx) -> bool {
        let api = self.api.clone();
        match api.as_str() {
            "Cast" => self.api_cast(rc),
            "Claymore" => self.api_claymore(rc),
            "CryptoDredge" => self.api_cryptodredge(rc),
            "Dstm" | "DSTM" => self.api_dstm(rc),
            "DynexsolveWrapper" => self.api_dynexsolve(rc),
            "EnemyZ" => self.api_enemyz(rc),
            "Ethminer" => self.api_ethminer(rc),
            "EthminerWrapper" => self.api_ethminer_wrapper(rc),
            "MiniZ" => self.api_miniz(rc),
            "Nanominer" => self.api_nanominer(rc),
            "Nheq" => self.api_nheq(rc),
            "RH" => self.api_rh(rc),
            "RHWrapper" => self.api_rh_wrapper(rc),
            "SrbMiner" => self.api_srbminer(rc),
            "TBMiner" => self.api_tbminer(rc),
            "TeamblackWrapper" => self.api_teamblack_wrapper(rc),
            "TTminerWrapper" => self.api_ttminer_wrapper(rc),
            "VerthashWrapper" => self.api_verthash_wrapper(rc),
            "Xgminer" => self.api_xgminer(rc),
            _ => return false,
        }
        true
    }

    /// GetArguments of the more API classes, None if the API has no own GetArguments
    pub fn get_arguments_more(&mut self) -> Option<String> {
        match self.api.as_str() {
            "Xgminer" => Some(self.arguments.replace("$mport2", &(self.port as u32 + 1).to_string()).replace("$mport", &self.port.to_string())),
            "Nanominer" => Some(self.nanominer_arguments()),
            "SrbMiner" => Some(self.srbminer_arguments()),
            _ => None,
        }
    }

    fn is_running_now(&mut self) -> bool {
        self.get_status() == MinerStatus::Running
    }

    fn api_cast(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(d) = self.local_json_pub("/") else { return };
        let mut hr = HashMap::new();
        if let Some(a) = self.algorithm.first().cloned() {
            let v = jsonx::sum_field(jsonx::get(&d, "devices"), "hash_rate") / 1000.0;
            if v > 0.0 {
                hr.insert(a, v);
                let sh = jsonx::get(&d, "shares");
                let acc = jsonx::num(jsonx::get(sh, "num_accepted"));
                let rej = 2.0 * jsonx::num(jsonx::get(sh, "num_rejected")) + jsonx::num(jsonx::get(sh, "num_network_fail")) + jsonx::num(jsonx::get(sh, "num_outdated"));
                self.update_shares(0, acc.trunc(), rej.trunc(), 0.0);
            }
        }
        self.add_miner_data(hr, HashMap::new(), 0.0, rc);
        self.cleanup_miner_data();
    }

    fn api_claymore(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(d) = self.tcp_json(r#"{"id":1,"jsonrpc":"2.0","method":"miner_getstat1"}"#, false) else { return };
        let res = jsonx::get(&d, "result");
        let r2 = jsonx::s(jsonx::idx(res, 2));
        if r2.is_empty() {
            log_info!("Failed to connect to miner {}. ", self.name);
            return;
        }
        let tt = Regex::new("^TT-Miner").unwrap().is_match(&jsonx::s(jsonx::idx(res, 0)));
        let algos = self.algorithm.join(" ");
        let is_k = |re: &str| Regex::new(&format!("(?i){}", re)).unwrap().is_match(&algos) && !tt;
        let mut hr = HashMap::new();
        let parts: Vec<&str> = r2.split(';').collect();
        let mut v = num_s(parts.first().copied().unwrap_or(""));
        if is_k("^ethash|^etchash|^firopow|^kawpow|^neoscrypt|^ubqhash|progpow|^sccpow") {
            v *= 1000.0;
        }
        let v = v.trunc();
        let r9 = jsonx::s(jsonx::idx(res, 9));
        let power: f64 = if r9.is_empty() { 0.0 } else { r9.split(';').map(num_s).sum() };
        if let Some(a) = self.algorithm.first().cloned() {
            if v > 0.0 {
                hr.insert(a, v);
                let acc = num_s(parts.get(1).copied().unwrap_or("")).trunc();
                let rej = num_s(parts.get(2).copied().unwrap_or("")).trunc();
                self.update_shares(0, acc - rej, rej, 0.0);
            }
        }
        if let Some(a2) = self.algorithm.get(1).cloned() {
            let r4 = jsonx::s(jsonx::idx(res, 4));
            let p4: Vec<&str> = r4.split(';').collect();
            let mut v2 = num_s(p4.first().copied().unwrap_or(""));
            if is_k("^ethash|^etchash|^kawpow|^neoscrypt|progpow|^sccpow") {
                v2 *= 1000.0;
            }
            if v2 > 0.0 {
                hr.insert(a2, v2);
                let acc = num_s(p4.get(1).copied().unwrap_or("")).trunc();
                let rej = num_s(p4.get(2).copied().unwrap_or("")).trunc();
                self.update_shares(1, acc - rej, rej, 0.0);
            }
        }
        self.add_miner_data(hr, HashMap::new(), power, rc);
        self.cleanup_miner_data();
    }

    /// "KEY=value;KEY=value|" (ccminer style summary)
    fn string_data(resp: &str) -> HashMap<String, String> {
        let mut data = HashMap::new();
        for part in resp.split(';') {
            if let Some((k, v)) = part.split_once('=') {
                data.insert(k.trim().to_uppercase(), v.trim().trim_end_matches(['|', '\0']).to_string());
            }
        }
        data
    }

    fn api_cryptodredge(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(resp) = self.tcp("summary", false, true) else { return };
        let data = Self::string_data(&resp);
        let num = |k: &str| data.get(k).and_then(|v| util::parse_f64(v)).unwrap_or(0.0);
        let mut hr = HashMap::new();
        if let Some(a) = self.algorithm.first().cloned() {
            let v = num("KHS") * 1000.0;
            if v > 0.0 {
                hr.insert(a, v);
                self.update_shares(0, num("ACC"), num("REJ"), 0.0);
            }
        }
        self.add_miner_data(hr, HashMap::new(), 0.0, rc);
        self.cleanup_miner_data();
    }

    fn api_dstm(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(d) = self.tcp_json(r#"{"id":1,"method":"getstat"}"#, false) else { return };
        let res = jsonx::get(&d, "result");
        let mut hr = HashMap::new();
        if let Some(a) = self.algorithm.first().cloned() {
            let mut v = jsonx::sum_field(res, "sol_ps");
            if v == 0.0 {
                v = jsonx::sum_field(res, "speed_sps");
            }
            if v > 0.0 {
                hr.insert(a, v);
                self.update_shares(0, jsonx::sum_field(res, "accepted_shares"), jsonx::sum_field(res, "rejected_shares"), 0.0);
            }
        }
        self.add_miner_data(hr, HashMap::new(), 0.0, rc);
        self.cleanup_miner_data();
    }

    fn output_lines(&self) -> Vec<String> {
        self.process.as_ref().map(|p| p.drain_output()).unwrap_or_default()
    }

    fn api_dynexsolve(&mut self, rc: &RunCtx) {
        let Some(a) = self.algorithm.first().cloned() else { return };
        let re_hr = Regex::new(r"\[GPU \*\].+ HASHRATE ([\d\s\./hkMGTPs]+?) \|").unwrap();
        let re_sh = Regex::new(r"POOL.+?\((\d+)/(\d+)[/)]").unwrap();
        let re_to = Regex::new(r"GPU\s+(\d+):\s+has\s+timed\sout").unwrap();
        for line in self.output_lines() {
            let l = simple_line(&line);
            if let Some(c) = re_hr.captures(&l) {
                let words: Vec<&str> = c[1].split_whitespace().collect();
                let mut v = util::parse_f64(&words.first().copied().unwrap_or("").chars().filter(|c| c.is_ascii_digit() || *c == '.').collect::<String>()).unwrap_or(0.0);
                v *= regex_unit(words.get(1).copied().unwrap_or(""));
                let hr = if v > 0.0 { self.one(&a, v) } else { HashMap::new() };
                self.add_miner_data(hr, HashMap::new(), 0.0, rc);
            } else if let Some(c) = re_sh.captures(&l) {
                self.update_shares(0, num_s(&c[1]), num_s(&c[2]), 0.0);
            } else if let Some(c) = re_to.captures(&l) {
                log_warn!("{}: GPU {} has timed out - restarting miner", self.name, &c[1]);
                self.restart = true;
            }
        }
        self.cleanup_miner_data();
    }

    fn api_enemyz(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(d) = self.local_json_pub("/summary?gpuinfo=1") else { return };
        let gpus = jsonx::get(&d, "gpus");
        let power = jsonx::sum_field(gpus, "power") / 1e3;
        let mut v = jsonx::num(jsonx::get(&d, "hashrate"));
        let g = jsonx::sum_field(gpus, "hashrate");
        if v <= g * 0.6 {
            v = g;
        }
        let mut hr = HashMap::new();
        let mut diff = HashMap::new();
        if let Some(a) = self.algorithm.first().cloned() {
            if v > 0.0 {
                hr.insert(a.clone(), v);
                diff.insert(a, jsonx::num(jsonx::get(&d, "pool_difficulty")));
                self.update_shares(0, jsonx::num(jsonx::get(&d, "accepted_count")), jsonx::num(jsonx::get(&d, "rejected_count")), 0.0);
            }
        }
        self.add_miner_data(hr, diff, power, rc);
        self.cleanup_miner_data();
    }

    fn api_ethminer(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(d) = self.tcp_json(r#"{"id":1,"jsonrpc":"2.0","method":"miner_getstatdetail"}"#, false) else { return };
        let res = jsonx::get(&d, "result");
        if res.is_null() {
            log_info!("Failed to connect to miner {}. ", self.name);
            return;
        }
        let mining = jsonx::get(res, "mining");
        let v = jsonx::num(jsonx::get(mining, "hashrate")).trunc();
        let mut hr = HashMap::new();
        let mut diff = HashMap::new();
        let power: f64 = jsonx::as_list(jsonx::get(res, "devices")).iter().map(|dv| jsonx::num(jsonx::idx(jsonx::path(dv, &["hardware", "sensors"]), 2))).sum();
        if let Some(a) = self.algorithm.first().cloned() {
            diff.insert(a.clone(), jsonx::num(jsonx::get(mining, "difficulty")));
            if v > 0.0 {
                hr.insert(a, v);
                let sh = jsonx::get(mining, "shares");
                let acc = jsonx::num(jsonx::idx(sh, 0)).trunc();
                let rej = jsonx::num(jsonx::idx(sh, 1)).trunc();
                self.update_shares(0, acc - rej, rej, 0.0);
            }
        }
        self.add_miner_data(hr, diff, power, rc);
        self.cleanup_miner_data();
    }

    fn api_ethminer_wrapper(&mut self, rc: &RunCtx) {
        let Some(a) = self.algorithm.first().cloned() else { return };
        let re_diff = Regex::new(r"^\s+i\s+.+?Difficulty\s*:\s*([\d\.\,]+?)\s*([hkMGT]+)").unwrap();
        let re_m = Regex::new(r"^\s+m\s+(.+)$").unwrap();
        let re_a = Regex::new(r"(?i)A(\d+)").unwrap();
        let re_r = Regex::new(r"(?i)R(\d+)").unwrap();
        for line in self.output_lines() {
            let l = simple_line(&line);
            if l.is_empty() {
                continue;
            }
            if let Some(c) = re_diff.captures(&l) {
                self.api_difficulty = num_s(&c[1]) * eth_unit(&c[2]);
            } else if let Some(c) = re_m.captures(&l) {
                let cols: Vec<&str> = c[1].split_whitespace().collect();
                let v = num_s(cols.get(4).copied().unwrap_or("")) * eth_unit(cols.get(5).copied().unwrap_or(""));
                let mut hr = HashMap::new();
                let mut diff = HashMap::new();
                if v > 0.0 {
                    hr.insert(a.clone(), v);
                    diff.insert(a.clone(), self.api_difficulty);
                    let c3 = cols.get(3).copied().unwrap_or("");
                    let acc = re_a.captures(c3).map(|m| num_s(&m[1])).unwrap_or(0.0);
                    let rej = re_r.captures(c3).map(|m| num_s(&m[1])).unwrap_or(0.0);
                    self.update_shares(0, acc, rej, 0.0);
                }
                self.add_miner_data(hr, diff, 0.0, rc);
            }
        }
        self.cleanup_miner_data();
    }

    fn api_miniz(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(d) = self.tcp_json(r#"{ "id":"0", "method":"getstat" }"#, true) else { return };
        if jsonx::s(jsonx::get(&d, "pers")) != "zil" {
            let res = jsonx::get(&d, "result");
            let v = jsonx::sum_field(res, "speed_sps");
            let power = jsonx::sum_field(res, "gpu_power_usage");
            let mut hr = HashMap::new();
            let mut diff = HashMap::new();
            if let Some(a) = self.algorithm.first().cloned() {
                if v > 0.0 {
                    hr.insert(a.clone(), v);
                    diff.insert(a, jsonx::num(jsonx::path(&d, &["pool", "difficulty"])));
                    self.update_shares(0, jsonx::sum_field(res, "accepted_shares"), jsonx::sum_field(res, "rejected_shares"), 0.0);
                }
            }
            self.add_miner_data(hr, diff, power, rc);
        }
        self.cleanup_miner_data();
    }

    fn nanominer_arguments(&mut self) -> String {
        let args = self.arguments.replace("$mport", &self.port.to_string());
        if !(args.starts_with('{') && args.ends_with('}')) {
            return args;
        }
        let Some(p) = jsonx::parse(&args) else { return args };
        let ssl = jsonx::truthy(jsonx::get(&p, "SSL"));
        let config_file = format!("config_{}-{}-{}{}.txt", self.pool.join("-"), self.base_algorithm.join("-"), self.device_model, if ssl { "-ssl" } else { "" });
        if self.path.exists() {
            let mut c: Vec<String> = vec![
                ";Automatic config file created by RainbowMiner".into(),
                ";Do not edit!".into(),
                "mport=0".into(),
                format!("webPort={}", self.port),
                "Watchdog=false".into(),
                "noLog=true".into(),
            ];
            let devices = jsonx::get(&p, "Devices");
            let devs = if devices.is_null() { String::new() } else { jsonx::as_list(devices).iter().map(|v| jsonx::s(v)).collect::<Vec<_>>().join(",") };
            for algo in jsonx::as_list(jsonx::get(&p, "Algorithms")) {
                let g = |k: &str| jsonx::s(jsonx::get(algo, k));
                c.push(format!("[{}]", g("Algo")));
                c.push(format!("wallet={}", g("Wallet")));
                c.push(format!("rigName={}", g("Worker")));
                c.push(format!("pool1={}:{}", g("Host"), g("Port")));
                c.push(format!("devices={}", devs));
                c.push(format!("useSSL={}", if jsonx::truthy(jsonx::get(algo, "SSL")) { "true" } else { "false" }));
                if !jsonx::get(algo, "PaymentId").is_null() {
                    c.push(format!("paymentId={}", g("PaymentId")));
                }
                for (k, n) in [("Pass", "rigPassword"), ("Email", "email"), ("Coin", "coin"), ("Protocol", "protocol")] {
                    if !g(k).is_empty() {
                        c.push(format!("{}={}", n, g(k)));
                    }
                }
                if g("Algo") == "zil" {
                    c.push("zilEpoch=0".into());
                }
                if jsonx::truthy(jsonx::get(&p, "LHR")) {
                    c.push(format!("lhr={}", jsonx::s(jsonx::get(&p, "LHR"))));
                }
                if jsonx::truthy(jsonx::get(&p, "Threads")) {
                    c.push(format!("cpuThreads={}", jsonx::s(jsonx::get(&p, "Threads"))));
                }
            }
            if let Some(dir) = self.path.parent() {
                // Out-File -Encoding utf8 writes a BOM
                let text = format!("\u{feff}{}\r\n", c.join("\r\n"));
                if let Err(e) = std::fs::write(dir.join(&config_file), text) {
                    log_warn!("{}: could not write {}: {}", self.name, config_file, e);
                }
            }
        }
        let params = jsonx::s(jsonx::get(&p, "Params"));
        format!("{}{}", config_file, if params.is_empty() { String::new() } else { format!(" {}", params) })
    }

    fn api_nanominer(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let d = match web::get_json(&format!("http://127.0.0.1:{}/stats", self.port), 10) {
            Ok(v) => v,
            Err(_) => {
                log_info!("Failed to connect to miner {}. ", self.name);
                return;
            }
        };
        let re_ix = Regex::new(r"(?i)^(Ethash|KawPOW)(\d+|low|NH)").unwrap();
        let fix = |s: &str| -> String {
            if let Some(c) = re_ix.captures(s) {
                c[1].to_string()
            } else {
                s.to_string()
            }
        };
        let mut ix0 = fix(self.base_algorithm.first().map(|s| s.as_str()).unwrap_or(""));
        if ix0 == "SCCPow" {
            ix0 = "FiroPow".into();
        }
        let a0 = jsonx::idx(jsonx::get(&d, "Algorithms"), 0);
        let algos = jsonx::keys(a0);
        let (name1, mut ix1) = if self.algorithm.len() > 1 { (self.algorithm[1].clone(), fix(&self.base_algorithm.get(1).cloned().unwrap_or_default())) } else { (String::new(), String::new()) };
        for k in &algos {
            let norm = crate::data::get_algorithm(if k == "Heavyhash" { "kHeavyHash" } else { k });
            if ix0 == norm {
                ix0 = k.clone();
            } else if ix1 == norm {
                ix1 = k.clone();
            }
        }
        let total = |ix: &str, f: &str| jsonx::num(jsonx::path(a0, &[ix, "Total", f]));
        let power: f64 = jsonx::values(jsonx::idx(jsonx::get(&d, "Devices"), 0)).iter().map(|v| jsonx::num(jsonx::get(v, "Power"))).sum();
        let mut hr = HashMap::new();
        if let Some(a) = self.algorithm.first().cloned() {
            let v = total(&ix0, "Hashrate");
            if v > 0.0 {
                hr.insert(a, v);
                self.update_shares(0, total(&ix0, "Accepted").trunc(), total(&ix0, "Rejected").trunc(), 0.0);
                if !ix1.is_empty() {
                    let v1 = total(&ix1, "Hashrate");
                    if !name1.is_empty() && v1 > 0.0 {
                        hr.insert(name1.clone(), v1);
                        // the original reads Accepted twice
                        self.update_shares(1, total(&ix1, "Accepted").trunc(), total(&ix1, "Accepted").trunc(), 0.0);
                    }
                }
            }
        }
        self.add_miner_data(hr, HashMap::new(), power, rc);
        self.cleanup_miner_data();
    }

    fn api_nheq(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(d) = self.tcp_json("status", false) else { return };
        let res = jsonx::get(&d, "result");
        let v = jsonx::sum_field(res, "speed_ips") * 1e6;
        let mut hr = HashMap::new();
        if let Some(a) = self.algorithm.first().cloned() {
            if v > 0.0 {
                hr.insert(a, v);
                let mins = self.get_running_time(false).num_milliseconds() as f64 / 60000.0;
                self.update_shares(0, jsonx::sum_field(res, "accepted_per_minute") * mins, jsonx::sum_field(res, "rejected_per_minute") * mins, 0.0);
            }
        }
        self.add_miner_data(hr, HashMap::new(), 0.0, rc);
        self.cleanup_miner_data();
    }

    fn api_rh(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(d) = self.tcp_json(" ", false) else { return };
        let infos = jsonx::get(&d, "infos");
        let v = jsonx::sum_field(infos, "speed");
        let mut hr = HashMap::new();
        if let Some(a) = self.algorithm.first().cloned() {
            if v > 0.0 {
                hr.insert(a, v);
                self.update_shares(0, jsonx::sum_field(infos, "accepted"), jsonx::sum_field(infos, "rejected"), 0.0);
            }
        }
        self.add_miner_data(hr, HashMap::new(), 0.0, rc);
        self.cleanup_miner_data();
    }

    fn api_rh_wrapper(&mut self, rc: &RunCtx) {
        let Some(a) = self.algorithm.first().cloned() else { return };
        let re_diff = Regex::new(r"^Net\s+.+?Difficulty\s*is\s*([\d\.\,]+)").unwrap();
        let re_sh = Regex::new(r"^Miner\s+.+?Shares.+?Accepted.*?(\d+).*?Rejected.*?(\d+).*?Failed.*?(\d+)").unwrap();
        let re_sp = Regex::new(r"^Miner\s+.+?Speed.*?([\d\.\,]+).*?([hkMGT]+)").unwrap();
        for line in self.output_lines() {
            let l = simple_line(&line);
            if l.is_empty() {
                continue;
            }
            if let Some(c) = re_diff.captures(&l) {
                self.api_difficulty = num_s(&c[1]);
            } else if let Some(c) = re_sh.captures(&l) {
                self.update_shares(0, num_s(&c[1]), num_s(&c[2]), 0.0);
            } else if let Some(c) = re_sp.captures(&l) {
                let v = num_s(&c[1]) * eth_unit(&c[2]);
                let mut hr = HashMap::new();
                let mut diff = HashMap::new();
                if v > 0.0 {
                    hr.insert(a.clone(), v);
                    diff.insert(a.clone(), self.api_difficulty);
                }
                self.add_miner_data(hr, diff, 0.0, rc);
            }
        }
        self.cleanup_miner_data();
    }

    fn srbminer_arguments(&mut self) -> String {
        let args = self.arguments.replace("$mport", &self.port.to_string());
        if !(args.starts_with('{') && args.ends_with('}')) {
            return args;
        }
        let Some(p) = jsonx::parse(&args) else { return args };
        let Some(dir) = self.path.parent().map(|d| d.to_path_buf()) else { return args };
        let mut threads: Vec<String> = jsonx::as_list(jsonx::path(&p, &["Config", "gpu_conf"])).iter().map(|g| jsonx::s(jsonx::get(g, "threads"))).filter(|s| !s.is_empty()).collect();
        threads.dedup();
        let t = threads.first().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        let config_file = format!("config_{}-{}-{}{}.txt", self.name, self.base_algorithm.join("-"), self.device_model, if threads.len() == 1 && t > 1 { format!("-{}", t) } else { String::new() });
        if !dir.join(&config_file).exists() {
            let _ = std::fs::write(dir.join(&config_file), serde_json::to_string_pretty(jsonx::get(&p, "Config")).unwrap_or_default());
        }
        let pool_file = format!("pools_{}-{}.txt", self.pool.join(" "), self.base_algorithm.join("-"));
        let _ = std::fs::write(dir.join(&pool_file), serde_json::to_string_pretty(jsonx::get(&p, "Pools")).unwrap_or_default());
        let cpar = jsonx::s(jsonx::get(&p, "CPar"));
        let ppar = jsonx::s(jsonx::get(&p, "PPar"));
        format!(
            "{} {} {} {} {}",
            if cpar.is_empty() { "--config".to_string() } else { cpar },
            config_file,
            if ppar.is_empty() { "--pools".to_string() } else { ppar },
            pool_file,
            jsonx::s(jsonx::get(&p, "Params"))
        )
        .trim()
        .to_string()
    }

    fn api_srbminer(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(d) = self.local_json_pub("") else { return };
        let mut v = jsonx::num(jsonx::get(&d, "HashRate_total_5min"));
        if v == 0.0 {
            v = jsonx::num(jsonx::get(&d, "HashRate_total_now"));
        }
        let mut hr = HashMap::new();
        if let Some(a) = self.algorithm.first().cloned() {
            if v > 0.0 {
                hr.insert(a, v);
                let sh = jsonx::get(&d, "shares");
                self.update_shares(0, jsonx::num(jsonx::get(sh, "accepted")).trunc(), jsonx::num(jsonx::get(sh, "rejected")).trunc(), 0.0);
            }
        }
        self.add_miner_data(hr, HashMap::new(), 0.0, rc);
        self.cleanup_miner_data();
    }

    fn api_tbminer(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let Some(d) = self.local_json_pub("/summary") else { return };
        let devs = jsonx::get(&d, "devices");
        let v = sum_props(devs, "hashrate");
        let power = sum_props(devs, "watt");
        let mut hr = HashMap::new();
        let mut diff = HashMap::new();
        if let Some(a) = self.algorithm.first().cloned() {
            if v > 0.0 {
                hr.insert(a.clone(), v);
                let dv = jsonx::values(jsonx::get(&d, "pool")).first().map(|p| jsonx::num(jsonx::get(p, "diff"))).unwrap_or(0.0);
                if dv > 0.0 {
                    diff.insert(a, dv);
                }
                self.update_shares(0, sum_props(devs, "accepted").trunc(), sum_props(devs, "rejected").trunc(), sum_props(devs, "stale").trunc());
            }
        }
        self.add_miner_data(hr, diff, power, rc);
        self.cleanup_miner_data();
    }

    fn api_teamblack_wrapper(&mut self, rc: &RunCtx) {
        let Some(a) = self.algorithm.first().cloned() else { return };
        let re_pearl = Regex::new(r"^\s+[\d.,]+\s+[kMGTP]?H/W\s+([\d.,]+)\s*([kMGTP]?H/s)\s+[\d.,]+\s*[kMGTP]?H/s\s+(\d+)/(\d+)/(\d+)\s+[\d.,]+%").unwrap();
        let re_gpu = Regex::new(r"GPU\d").unwrap();
        let re_line = Regex::new(r"([\d\s\./hkMGTPs]+?)(\d+)/(\d+)[/\s].*\([\d\.]+\)$").unwrap();
        let re_acc = Regex::new(r"Accepted\s+\((\d+)/(\d+)[/)]").unwrap();
        let re_to = Regex::new(r"GPU\s+(\d+):\s+has\s+timed\sout").unwrap();
        for line in self.output_lines() {
            let l = simple_line(&line);
            if a == "PearlHash" {
                if let Some(c) = re_pearl.captures(&l) {
                    let mult = match c[2].to_uppercase().as_str() {
                        "KH/S" => 1e3,
                        "MH/S" => 1e6,
                        "GH/S" => 1e9,
                        "TH/S" => 1e12,
                        "PH/S" => 1e15,
                        _ => 0.0,
                    };
                    let v = num_s(&c[1]) * mult;
                    if v > 0.0 && v.is_finite() {
                        self.update_shares(0, num_s(&c[3]), num_s(&c[4]), num_s(&c[5]));
                        let hr = self.one(&a, v);
                        self.add_miner_data(hr, HashMap::new(), 0.0, rc);
                    }
                    continue;
                }
            }
            if !re_gpu.is_match(&l) {
                if let Some(c) = re_line.captures(&l) {
                    let words: Vec<&str> = c[1].split_whitespace().collect();
                    let mut v = util::parse_f64(&words.get(2).copied().unwrap_or("").chars().filter(|c| c.is_ascii_digit() || *c == '.').collect::<String>()).unwrap_or(0.0);
                    v *= regex_unit(words.get(3).copied().unwrap_or(""));
                    let mut hr = HashMap::new();
                    if v > 0.0 {
                        hr.insert(a.clone(), v);
                        self.update_shares(0, num_s(&c[2]), num_s(&c[3]), 0.0);
                    }
                    self.add_miner_data(hr, HashMap::new(), 0.0, rc);
                    continue;
                }
            }
            if let Some(c) = re_acc.captures(&l) {
                let acc = num_s(&c[1]);
                self.update_shares(0, acc, num_s(&c[2]) - acc, 0.0);
            } else if let Some(c) = re_to.captures(&l) {
                log_warn!("{}: GPU {} has timed out - restarting miner", self.name, &c[1]);
                self.restart = true;
            }
        }
        self.cleanup_miner_data();
    }

    fn api_ttminer_wrapper(&mut self, rc: &RunCtx) {
        let Some(a) = self.algorithm.first().cloned() else { return };
        // Epic Cash rotates its PoW per block, stale summary lines count as samples
        let stale_ok = self.base_algorithm.iter().any(|b| b == "ProgPowEpic");
        let re_all = Regex::new(r"All\[[^\]]*\]:\s+([\*-])((?:\d+[\.,])?\d+)\s*([kMGTP]?)H/s").unwrap();
        let re_pw = Regex::new(r"\]\s+((?:\d+[\.,])?\d+)\s+W\s").unwrap();
        let re_last = Regex::new(r"last:\s+(\d+):(\d+)\s*$").unwrap();
        let re_sh = Regex::new(r"\[A(\d+):R(\d+):S(\d+)").unwrap();
        for line in self.output_lines() {
            let l = simple_line(&line);
            let Some(c) = re_all.captures(&l) else { continue };
            let live = &c[1] == "*";
            let mut v = num_s(&c[2]);
            v *= match &c[3] {
                "k" => 1e3,
                "M" => 1e6,
                "G" => 1e9,
                "T" => 1e12,
                "P" => 1e15,
                _ => 1.0,
            };
            if v <= 0.0 {
                continue;
            }
            let power = re_pw.captures(&l).map(|m| num_s(&m[1]));
            let pw = power.unwrap_or(0.0);
            if pw > self.api_max_power_draw {
                self.api_max_power_draw = pw;
            }
            let last = re_last.captures(&l).map(|m| m[1].parse::<i64>().unwrap_or(0) * 60 + m[2].parse::<i64>().unwrap_or(0)).unwrap_or(-1);
            if last > 300 && pw > 0.0 && pw < self.api_max_power_draw * 0.25 {
                if !self.restart {
                    log_warn!(
                        "{}: appears crashed (no share for {}s, power {}W vs. peak {}W) - restarting miner",
                        self.name,
                        last,
                        pw,
                        self.api_max_power_draw
                    );
                    self.restart = true;
                }
            } else if live || stale_ok {
                if let Some(s) = re_sh.captures(&l) {
                    self.update_shares(0, num_s(&s[1]), num_s(&s[2]), num_s(&s[3]));
                }
                let hr = self.one(&a, v);
                self.add_miner_data(hr, HashMap::new(), pw, rc);
            }
        }
        self.cleanup_miner_data();
    }

    fn api_verthash_wrapper(&mut self, rc: &RunCtx) {
        let Some(a) = self.algorithm.first().cloned() else { return };
        let re = Regex::new(r"(?i)^.+?accepted[:\s]+(\d+)/(\d+).+hashrate[^\d]+(.+?)\s+(.+?)/s").unwrap();
        for line in self.output_lines() {
            let l = simple_line(&line);
            let Some(c) = re.captures(&l) else { continue };
            let mut v = num_s(&c[3]);
            v *= match &c[4] {
                "kH" => 1e3,
                "MH" => 1e6,
                "GH" => 1e9,
                "TH" => 1e12,
                "PH" => 1e15,
                _ => 1.0,
            };
            let hr = if v > 0.0 { self.one(&a, v) } else { HashMap::new() };
            let acc = num_s(&c[1]);
            self.update_shares(0, acc, num_s(&c[2]) - acc, 0.0);
            self.add_miner_data(hr, HashMap::new(), 0.0, rc);
        }
        self.cleanup_miner_data();
    }

    fn api_xgminer(&mut self, rc: &RunCtx) {
        if !self.is_running_now() {
            return;
        }
        let request = r#"{"command":"summary","parameter":""}"#;
        let Some(resp) = self.tcp(request, false, false) else { return };
        let Some(d) = xgminer_json(&resp) else {
            log_info!("Failed to connect to miner {}. ", self.name);
            return;
        };
        let mut hr = HashMap::new();
        let mut diff = HashMap::new();
        let s = jsonx::get(&d, "SUMMARY");
        let s = if s.is_array() { jsonx::idx(s, 0) } else { s };
        if let Some(a) = self.algorithm.first().cloned() {
            let v = xgminer_hashrate(s);
            if v > 0.0 {
                hr.insert(a.clone(), v);
                diff.insert(a, jsonx::num(jsonx::get(s, "Difficulty_Accepted")));
                self.update_shares(0, jsonx::num(jsonx::get(s, "accepted")).trunc(), jsonx::num(jsonx::get(s, "rejected")).trunc(), 0.0);
                if let Some(a2) = self.algorithm.get(1).cloned() {
                    match web::tcp_request("127.0.0.1", self.port + 1, request, 10, false, false).ok().and_then(|r| xgminer_json(&r)) {
                        Some(d2) => {
                            let s2 = jsonx::get(&d2, "SUMMARY");
                            let s2 = if s2.is_array() { jsonx::idx(s2, 0) } else { s2 };
                            let v2 = xgminer_hashrate(s2);
                            if v2 > 0.0 {
                                hr.insert(a2.clone(), v2);
                                diff.insert(a2, jsonx::num(jsonx::get(s2, "Difficulty_Accepted")));
                                self.update_shares(1, jsonx::num(jsonx::get(s2, "accepted")).trunc(), jsonx::num(jsonx::get(s2, "rejected")).trunc(), 0.0);
                            }
                        }
                        None => {
                            log_info!("Failed to connect to miner {}, second algorithm {}. ", self.name, a2);
                            return;
                        }
                    }
                }
            }
        }
        self.add_miner_data(hr, diff, 0.0, rc);
        self.cleanup_miner_data();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(simple_line("\x1b[32mHello\x1b[0m\r\n"), "Hello");
        assert_eq!(eth_unit("Mh"), 1e6);
        assert_eq!(regex_unit("MH/s"), 1e6);
        assert_eq!(regex_unit("kh/s"), 1e3);
        let v = xgminer_json("STATUS {\"SUMMARY\":[{\"KHS 5s\":12.5,\"Accepted\":3}]}\0").unwrap();
        assert_eq!(xgminer_hashrate(jsonx::idx(jsonx::get(&v, "SUMMARY"), 0)), 12500.0);
    }
}

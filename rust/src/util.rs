//! General helper functions, ported from Modules/Include.psm1
//! (Get-Yes, ConvertTo-Hash, ConvertFrom-Time, Get-Sigma, Get-Version, CPU affinity helpers, ...)
#![allow(dead_code)]

use chrono::{DateTime, Duration, Utc};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// Get-Yes: everything is "true", except a short list of negative words / numbers.
pub fn get_yes_str(s: &str) -> bool {
    let s = s.trim().to_lowercase();
    !matches!(
        s.as_str(),
        "" | "0" | "false" | "no" | "n" | "not" | "niet" | "non" | "nein" | "never" | "$false"
    )
}

/// Get-Yes for a JSON value
pub fn get_yes(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        serde_json::Value::String(s) => get_yes_str(s),
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(_) => true,
    }
}

/// .NET TextInfo.ToTitleCase (invariant culture): first letter of each word upper, rest lower.
/// Words that are completely upper case stay as they are (like .NET does with acronyms).
pub fn to_title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, word) in s.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let has_lower = word.chars().any(|c| c.is_lowercase());
        if !has_lower && word.chars().any(|c| c.is_uppercase()) {
            out.push_str(word);
            continue;
        }
        let mut first = true;
        for c in word.chars() {
            if first && c.is_alphabetic() {
                out.extend(c.to_uppercase());
                first = false;
            } else if first {
                out.push(c);
            } else {
                out.extend(c.to_lowercase());
            }
        }
    }
    out
}

/// Keep only the characters matching `keep` (used instead of `-replace "[^...]"`)
pub fn keep_chars(s: &str, keep: impl Fn(char) -> bool) -> String {
    s.chars().filter(|c| keep(*c)).collect()
}

/// Parse a number in a forgiving way ("1,5" -> 1.5, "  12 " -> 12)
pub fn parse_f64(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', ".");
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok()
}

/// [double]($x -replace "[^0-9.]") semantics
pub fn number_part(s: &str) -> f64 {
    let t: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    t.parse::<f64>().unwrap_or(0.0)
}

/// ConvertFrom-Hash: "12.5 MH" -> 12500000
pub fn convert_from_hash(s: &str) -> f64 {
    let num = number_part(s);
    let unit = s.chars().find(|c| "kMGHTPEZY".contains(*c));
    match unit {
        Some('k') => num * 1e3,
        Some('M') => num * 1e6,
        Some('G') => num * 1e9,
        Some('T') => num * 1e12,
        Some('P') => num * 1e15,
        Some('E') => num * 1e18,
        Some('Z') => num * 1e21,
        Some('Y') => num * 1e24,
        _ => num,
    }
}

/// ConvertFrom-Bytes: "16GB" -> 17179869184
pub fn convert_from_bytes(s: &str) -> f64 {
    let num = number_part(s);
    let unit = s.chars().find(|c| "kMGHTPEZY".contains(*c));
    match unit {
        Some('k') => num * 1024.0,
        Some('M') => num * 1048576.0,
        Some('G') => num * 1073741824.0,
        Some('T') => num * 1099511627776.0,
        Some('P') => num * 1.12589990684262e15,
        Some('E') => num * 1.15292150460684e18,
        Some('Z') => num * 1.18059162071741e21,
        Some('Y') => num * 1.20892581961462e24,
        _ => num,
    }
}

/// ConvertFrom-Time: "12h" -> 43200 (seconds), default unit is seconds
pub fn convert_from_time(s: &str) -> i64 {
    let num = number_part(s);
    let unit = s.chars().find(|c| "mhdw".contains(*c));
    let secs = match unit {
        Some('m') => num * 60.0,
        Some('h') => num * 3600.0,
        Some('d') => num * 86400.0,
        Some('w') => num * 604800.0,
        _ => num,
    };
    secs.round() as i64
}

/// Format a number with group separators and `decimals` decimals ("{0:n2}")
pub fn format_n(v: f64, decimals: usize) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let s = format!("{:.*}", decimals, v.abs());
    let (int_part, frac_part) = match s.find('.') {
        Some(p) => (&s[..p], &s[p..]),
        None => (s.as_str(), ""),
    };
    let mut grouped = String::new();
    let bytes = int_part.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(*b as char);
    }
    let sign = if v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') {
        "-"
    } else {
        ""
    };
    format!("{}{}{}", sign, grouped, frac_part)
}

/// ConvertTo-Float: 1234567 -> "1.23 M"
pub fn convert_to_float(num: f64) -> String {
    if num <= 0.0 || !num.is_finite() {
        return "0  ".to_string();
    }
    let e = (num.ln() / 1e3f64.ln()).floor() as i32;
    match e {
        i32::MIN..=-3 => "0  ".to_string(),
        -2 => format!("{} µ", format_n(num * 1e6, 2)),
        -1 => format!("{} m", format_n(num * 1e3, 2)),
        0 => format!("{}  ", format_n(num, 2)),
        1 => format!("{} k", format_n(num / 1e3, 2)),
        2 => format!("{} M", format_n(num / 1e6, 2)),
        3 => format!("{} G", format_n(num / 1e9, 2)),
        4 => format!("{} T", format_n(num / 1e12, 2)),
        5 => format!("{} P", format_n(num / 1e15, 2)),
        6 => format!("{} E", format_n(num / 1e18, 2)),
        7 => format!("{} Z", format_n(num / 1e21, 2)),
        _ => format!("{} Y", format_n(num / 1e24, 2)),
    }
}

/// ConvertTo-Hash: 1234567 -> "1.23 MH"
pub fn convert_to_hash(num: f64) -> String {
    format!("{}H", convert_to_float(num))
}

/// ConvertTo-TTF: seconds -> "1.5 h"
pub fn convert_to_ttf(secs: f64) -> String {
    if !secs.is_finite() || secs > 9.2e11 {
        return ">10 y".to_string();
    }
    let days = (secs / 86400.0).floor();
    let hours = ((secs % 86400.0) / 3600.0).floor();
    let minutes = ((secs % 3600.0) / 60.0).floor();
    if days > 0.0 {
        if days > 365.0 {
            ">1 y".into()
        } else if days > 182.0 {
            ">6 mo".into()
        } else if days > 30.0 {
            ">1 mo".into()
        } else if days > 7.0 {
            ">1 w".into()
        } else {
            format!("{} d", round_to(secs / 86400.0, 1))
        }
    } else if hours > 0.0 {
        format!("{} h", round_to(secs / 3600.0, 1))
    } else if minutes > 0.0 {
        format!("{} m", round_to(secs / 60.0, 1))
    } else {
        format!("{} s", round_to(secs, 1))
    }
}

/// [Math]::Round(x, digits) with banker's rounding like .NET
pub fn round_to(v: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    let x = v * f;
    let r = x.round();
    // .NET uses MidpointRounding.ToEven
    let r = if (x - x.trunc()).abs() == 0.5 {
        let t = x.trunc();
        if (t as i64) % 2 == 0 {
            t
        } else {
            t + x.signum()
        }
    } else {
        r
    };
    r / f
}

/// ConvertTo-LocalCurrency
pub fn convert_to_local_currency(number: f64, btc_rate: f64, offset: i32) -> String {
    let digits = if btc_rate > 0.0 {
        ((10 - offset) as f64 - btc_rate.log10()).trunc().clamp(0.0, 9.0) as usize
    } else {
        2
    };
    format_n(number * btc_rate, digits)
}

/// ConvertTo-BTC
pub fn convert_to_btc(number: f64, offset: i32) -> String {
    let mut num = number;
    let mut offset = offset;
    let mut currency = "BTC";
    if num != 0.0 {
        let e = (num.abs().ln() / 1000f64.ln()).trunc() as i32;
        match e {
            -1 => {
                currency = "mBTC";
                num *= 1e3;
                offset = 5
            }
            -2 => {
                currency = "µBTC";
                num *= 1e6;
                offset = 8
            }
            -3 => {
                currency = "sat";
                num *= 1e8;
                offset = 10
            }
            _ => {}
        }
    }
    format!("{} {}", convert_to_local_currency(num, 1.0, offset), currency)
}

/// Get-Sigma: bias-corrected standard deviation
pub fn get_sigma(data: &[f64]) -> f64 {
    if data.len() > 1 {
        let n = data.len() as f64;
        let mean = data.iter().sum::<f64>() / n;
        let bias = n - 1.5 + 1.0 / (8.0 * (n - 1.0));
        (data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / bias).sqrt()
    } else {
        0.0
    }
}

/// A System.Version like version with up to 4 components
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Version(pub Vec<u64>);

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() < 2 || parts.len() > 4 {
            return None;
        }
        let mut v = Vec::new();
        for p in parts {
            v.push(p.parse::<u64>().ok()?);
        }
        Some(Version(v))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: Vec<String> = self.0.iter().map(|x| x.to_string()).collect();
        write!(f, "{}", s.join("."))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // System.Version: missing components are -1 (smaller than 0)
        for i in 0..4 {
            let a = self.0.get(i).map(|x| *x as i64).unwrap_or(-1);
            let b = other.0.get(i).map(|x| *x as i64).unwrap_or(-1);
            match a.cmp(&b) {
                Ordering::Equal => continue,
                o => return o,
            }
        }
        Ordering::Equal
    }
}

/// Get-Version
pub fn get_version(s: &str) -> Option<Version> {
    let s = s.split('-').next().unwrap_or("");
    let s: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let parts: Vec<&str> = s.split('.').collect();
    let s = if parts.len() > 4 { parts[..4].join(".") } else { s };
    Version::parse(&s)
}

/// Get-MinerVersion: extract a comparable version from a version string or download URI
pub fn get_miner_version(version: &str) -> Version {
    use regex::Regex;
    use std::sync::LazyLock;
    static RE_TAG: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)/releases/download/v?([0-9][0-9a-z.]*)").unwrap());
    static RE_FILE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)[/_-]v?([0-9]+(?:\.[0-9]+){1,3}[a-z]?[0-9]*)").unwrap());
    static RE_V: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/v([0-9a-z.]+)-").unwrap());
    static RE_VV: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d)[a-z]*v(\d)").unwrap());
    static RE_RR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d)[a-z]*r(\d)").unwrap());
    static RE_ALPHA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([a-z])([0-9]|$)").unwrap());

    let mut v = version.to_string();
    if let Some(c) = RE_TAG.captures(&v) {
        v = c[1].to_string();
    } else if let Some(c) = RE_FILE.captures(&v) {
        v = c[1].to_string();
    }
    if let Some(c) = RE_V.captures(&v) {
        v = c[1].to_string();
    }
    let v = v.to_lowercase();
    let mut v: String = v
        .chars()
        .filter(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || *c == '.')
        .collect();
    v = v.trim_start_matches(|c: char| !c.is_ascii_digit()).to_string();
    v = RE_VV.replace_all(&v, "$1.$2").to_string();
    v = RE_RR.replace_all(&v, "$1.$2").to_string();
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) {
        v = format!("{}.0", v);
    }
    if !v.chars().all(|c| c.is_ascii_digit() || c == '.') {
        v = RE_ALPHA
            .replace_all(&v, |caps: &regex::Captures| {
                let ch = caps[1].chars().next().unwrap();
                let n = ch as u8 - b'a';
                let d = &caps[2];
                if d.is_empty() {
                    format!(".{}", n)
                } else {
                    format!(".{}.{}", n, d)
                }
            })
            .to_string();
    }
    let parts: Vec<u64> = v
        .split('.')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<u64>().ok())
        .collect();
    if parts.len() > 4 {
        // fold the extra components into the 4th one
        let mut arr = parts.clone();
        let mut x = 1000u64;
        let n = arr.len();
        let mut i = n as i64 - 2;
        while i > 2 {
            arr[i as usize] = arr[i as usize].saturating_mul(x);
            x = x.saturating_mul(1000);
            i -= 1;
        }
        let rest: u64 = arr[3..].iter().fold(0u64, |a, b| a.saturating_add(*b));
        return Version(vec![arr[0], arr[1], arr[2], rest]);
    }
    if parts.len() >= 2 {
        Version(parts)
    } else if parts.len() == 1 {
        Version(vec![parts[0], 0])
    } else {
        Version(vec![1, 0])
    }
}

/// Compare-Version
pub fn compare_version(v1: &str, v2: &str, revs: i32) -> i32 {
    let clean = |s: &str| -> Vec<i64> {
        let s = s.split('-').next().unwrap_or("");
        let s: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        s.split('.').map(|p| p.parse::<i64>().unwrap_or(0)).collect()
    };
    let a = clean(v1);
    let b = clean(v2);
    let mut max = a.len().min(b.len());
    if revs > 0 && (revs as usize) < max {
        max = revs as usize;
    }
    for i in 0..max {
        if a[i] < b[i] {
            return -1;
        }
        if a[i] > b[i] {
            return 1;
        }
    }
    0
}

/// ConvertFrom-CPUAffinity: "0x0F" -> [0,1,2,3]
pub fn convert_from_cpu_affinity(affinity: &str) -> Vec<usize> {
    let hex: String = affinity
        .chars()
        .filter(|c| c.is_ascii_hexdigit() || *c == 'x' || *c == 'X')
        .collect();
    let hex = hex.trim_start_matches(['0', 'x', 'X']);
    let mut out = Vec::new();
    let mut bit = 0usize;
    for c in hex.chars().rev() {
        let d = c.to_digit(16).unwrap_or(0);
        for b in 0..4 {
            if d & (1 << b) != 0 {
                out.push(bit + b);
            }
        }
        bit += 4;
    }
    out.sort_unstable();
    out
}

/// ConvertFrom-CPUAffinity -ToInt (limited to 128 cpus)
pub fn cpu_affinity_to_int(affinity: &str) -> u128 {
    convert_from_cpu_affinity(affinity)
        .into_iter()
        .filter(|b| *b < 128)
        .fold(0u128, |a, b| a | (1u128 << b))
}

/// ConvertTo-CPUAffinity -ToHex
pub fn convert_to_cpu_affinity_hex(threads: &[usize]) -> String {
    let v = threads
        .iter()
        .filter(|b| **b < 128)
        .fold(0u128, |a, b| a | (1u128 << b));
    if v > 0 {
        format!("0x{:x}", v)
    } else {
        "0x00".to_string()
    }
}

/// Get-ConfigArray: split "a, b;c" into ["a","b","c"]
pub fn split_config_array(s: &str) -> Vec<String> {
    s.split([',', ';'])
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

/// Format a duration like a .NET TimeSpan string: "[d.]hh:mm:ss[.fffffff]"
pub fn format_timespan(d: Duration) -> String {
    let neg = d < Duration::zero();
    let d = if neg { -d } else { d };
    let total_secs = d.num_seconds();
    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    let ticks = (d - Duration::seconds(total_secs)).num_nanoseconds().unwrap_or(0) / 100;
    let mut s = String::new();
    if neg {
        s.push('-');
    }
    if days > 0 {
        s.push_str(&format!("{}.", days));
    }
    s.push_str(&format!("{:02}:{:02}:{:02}", hours, minutes, seconds));
    if ticks > 0 {
        s.push_str(&format!(".{:07}", ticks));
    }
    s
}

/// Parse a .NET TimeSpan string "[-][d.]hh:mm:ss[.fffffff]"
pub fn parse_timespan(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (neg, s) = if let Some(r) = s.strip_prefix('-') { (true, r) } else { (false, s) };
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        // plain number of days?
        return s.parse::<i64>().ok().map(Duration::days);
    }
    let (days, hours) = match parts[0].split_once('.') {
        Some((d, h)) => (d.parse::<i64>().ok()?, h.parse::<i64>().ok()?),
        None => (0, parts[0].parse::<i64>().ok()?),
    };
    let minutes = parts[1].parse::<i64>().ok()?;
    let (secs, frac) = match parts[2].split_once('.') {
        Some((s, f)) => (s.parse::<i64>().ok()?, f),
        None => (parts[2].parse::<i64>().ok()?, ""),
    };
    let mut nanos = 0i64;
    if !frac.is_empty() {
        let f: String = frac.chars().take(9).collect();
        let len = f.len();
        nanos = f.parse::<i64>().ok()? * 10i64.pow((9 - len) as u32);
    }
    let d = Duration::days(days)
        + Duration::hours(hours)
        + Duration::minutes(minutes)
        + Duration::seconds(secs)
        + Duration::nanoseconds(nanos);
    Some(if neg { -d } else { d })
}

/// Parse a DateTime as written by ConvertTo-Json (PS7 ISO format or PS5 "/Date(ms)/")
pub fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix("/Date(").and_then(|x| x.strip_suffix(")/")) {
        let num: String = inner.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
        let ms = num.parse::<i64>().ok()?;
        return DateTime::from_timestamp_millis(ms);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // ISO without offset (interpreted as UTC)
    for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(DateTime::from_naive_utc_and_offset(ndt, Utc));
        }
    }
    None
}

/// Format a DateTime the way PS7 ConvertTo-Json does it for UTC times
pub fn format_datetime(dt: &DateTime<Utc>) -> String {
    use chrono::Timelike;
    format!("{}.{:07}Z", dt.format("%Y-%m-%dT%H:%M:%S"), dt.nanosecond() % 1_000_000_000 / 100)
}

/// "0001-01-01" (DateTime.MinValue)
pub fn min_datetime() -> DateTime<Utc> {
    DateTime::<Utc>::MIN_UTC
}

pub fn unix_epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).unwrap()
}

/// Split a command line into arguments (double and single quotes are respected)
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_dq = false;
    let mut in_sq = false;
    let mut has = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_dq {
            if c == '\\' && i + 1 < chars.len() && chars[i + 1] == '"' {
                cur.push('"');
                i += 2;
                continue;
            }
            if c == '"' {
                in_dq = false;
            } else {
                cur.push(c);
            }
        } else if in_sq {
            if c == '\'' {
                in_sq = false;
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_dq = true;
            has = true;
        } else if c == '\'' {
            in_sq = true;
            has = true;
        } else if c.is_whitespace() {
            if has || !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
                has = false;
            }
        } else {
            cur.push(c);
        }
        i += 1;
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Collapse whitespace runs into single spaces and trim (`-replace "\s+"," "`)
pub fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Join a relative RainbowMiner path (".\Bin\x\y.exe" or "./Bin/x/y") onto `base`
pub fn rbm_path(base: &Path, p: &str) -> PathBuf {
    let norm = p.replace('\\', "/");
    let norm = norm.trim_start_matches("./");
    let pb = PathBuf::from(norm);
    if pb.is_absolute() {
        pb
    } else {
        base.join(pb)
    }
}

/// Test-Intersect (case insensitive)
pub fn intersects_ci<A: AsRef<str>, B: AsRef<str>>(a: &[A], b: &[B]) -> bool {
    a.iter()
        .any(|x| b.iter().any(|y| x.as_ref().eq_ignore_ascii_case(y.as_ref())))
}

pub fn contains_ci<A: AsRef<str>>(list: &[A], s: &str) -> bool {
    list.iter().any(|x| x.as_ref().eq_ignore_ascii_case(s))
}

/// Unique (case-insensitive), keeps the first appearance
pub fn unique_ci(list: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in list {
        if !out.iter().any(|x| x.eq_ignore_ascii_case(&s)) {
            out.push(s);
        }
    }
    out
}

/// Strip ANSI escape sequences (`-replace "\x1B\[[0-?]*[ -/]*[@-~]"`)
pub fn strip_ansi(s: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1B\[[0-?]*[ -/]*[@-~]").unwrap());
    RE.replace_all(s, "").into_owned()
}

/// MD5 hex string (lower case)
pub fn md5_hex(s: &str) -> String {
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

/// SHA256 of a file (upper case hex, like Get-FileHash)
pub fn sha256_file(p: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let data = std::fs::read(p)?;
    let mut h = Sha256::new();
    h.update(&data);
    Ok(hex::encode_upper(h.finalize()))
}

/// Get-UnixTimestamp
pub fn unix_timestamp() -> i64 {
    Utc::now().timestamp()
}

/// Write a file atomically (tmp + rename) without BOM
pub fn write_file_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = path.with_extension(format!(
        "{}.tmp{}",
        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
        std::process::id()
    ));
    std::fs::write(&tmp, content)?;
    match std::fs::rename(&tmp, path) {
        Ok(_) => Ok(()),
        Err(_) => {
            let r = std::fs::write(path, content);
            let _ = std::fs::remove_file(&tmp);
            r
        }
    }
}

/// Read a text file like .NET's StreamReader: UTF-8 (BOM optional) or UTF-16 LE/BE with BOM
pub fn read_text(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(decode_text(&bytes))
}

pub fn decode_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let u: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let u: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    let b = if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        &bytes[3..]
    } else {
        bytes
    };
    String::from_utf8_lossy(b).into_owned()
}

/// Get-HourMinStr like "1h 5m"
pub fn hour_min_str(d: Duration) -> String {
    let h = d.num_hours();
    let m = d.num_minutes() % 60;
    if h > 0 {
        format!("{}h {}m", h, m)
    } else {
        format!("{}m", m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yes() {
        assert!(get_yes_str("1"));
        assert!(get_yes_str("yes"));
        assert!(!get_yes_str("0"));
        assert!(!get_yes_str("no"));
        assert!(!get_yes_str(""));
    }

    #[test]
    fn hashes() {
        assert_eq!(convert_from_hash("1.5 MH"), 1.5e6);
        assert_eq!(convert_to_hash(1234567.0), "1.23 MH");
        assert_eq!(convert_to_hash(999.0), "999.00  H");
        assert_eq!(convert_from_time("12h"), 43200);
        assert_eq!(convert_from_time("30m"), 1800);
        assert_eq!(convert_from_time("90"), 90);
    }

    #[test]
    fn titlecase() {
        assert_eq!(to_title_case("ethash"), "Ethash");
        assert_eq!(to_title_case("hello world"), "Hello World");
        assert_eq!(to_title_case("US"), "US");
    }

    #[test]
    fn timespan() {
        let d = parse_timespan("1.02:03:04.5000000").unwrap();
        assert_eq!(d.num_seconds(), 86400 + 7384);
        assert_eq!(format_timespan(Duration::seconds(3661)), "01:01:01");
        assert_eq!(format_timespan(Duration::seconds(90061)), "1.01:01:01");
    }

    #[test]
    fn versions() {
        assert_eq!(get_miner_version("0.26.8").to_string(), "0.26.8");
        assert_eq!(
            get_miner_version("https://github.com/x/y/releases/download/v0.26.8-trex/t-rex-0.26.8-linux.tar.gz")
                .to_string(),
            "0.26.8"
        );
        assert!(get_version("5.0.4.1").unwrap() > get_version("5.0.4").unwrap());
        assert_eq!(compare_version("9.2", "11.0", -1), -1);
    }

    #[test]
    fn affinity() {
        assert_eq!(convert_from_cpu_affinity("0x0F"), vec![0, 1, 2, 3]);
        assert_eq!(convert_to_cpu_affinity_hex(&[0, 1, 2, 3]), "0xf");
    }

    #[test]
    fn args() {
        assert_eq!(split_args(r#"-a x -p "c=BTC,d=1" --x='y z'"#), vec!["-a", "x", "-p", "c=BTC,d=1", "--x=y z"]);
    }

    #[test]
    fn sigma() {
        assert_eq!(get_sigma(&[1.0]), 0.0);
        assert!(get_sigma(&[1.0, 2.0, 3.0]) > 0.0);
    }
}

//! Statistics (Modules/StatLib.psm1): moving averages with fluctuation for pool prices,
//! miner hashrates and profits. The JSON format of the Stats folder is kept compatible.
#![allow(dead_code)]

use crate::jsonx;
use crate::util;
use crate::{log_info, log_warn};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

const SMALLEST_VALUE: f64 = 1e-20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatMode {
    Pools,
    Miners,
    Profit,
}

#[derive(Debug, Clone)]
pub struct Stat {
    pub live: f64,
    pub minute: f64,
    pub minute_fluctuation: f64,
    pub minute_5: f64,
    pub minute_5_fluctuation: f64,
    pub minute_10: f64,
    pub minute_10_fluctuation: f64,
    pub hour: f64,
    pub hour_fluctuation: f64,
    pub day: f64,
    pub day_fluctuation: f64,
    pub three_day: f64,
    pub three_day_fluctuation: f64,
    pub week: f64,
    pub week_fluctuation: f64,
    pub duration: Duration,
    pub updated: DateTime<Utc>,
    pub failed: i32,
    // miners part
    pub power_draw_live: f64,
    pub power_draw_average: f64,
    pub diff_live: f64,
    pub diff_average: f64,
    pub ratio_live: f64,
    pub benchmarked: DateTime<Utc>,
    pub version: String,
    pub log_file: String,
    pub is_fl: bool,
    pub failed_value: f64,
    pub failed_streak: i32,
    pub device_model: String,
    // pools part
    pub hashrate_live: f64,
    pub hashrate_average: f64,
    pub blockrate_live: f64,
    pub blockrate_average: f64,
    pub actual24h_week: f64,
    pub estimate24h_week: f64,
    pub error_ratio: f64,
    pub deviation_streak: i32,
}

impl Default for Stat {
    fn default() -> Self {
        Stat {
            live: 0.0,
            minute: 0.0,
            minute_fluctuation: 0.0,
            minute_5: 0.0,
            minute_5_fluctuation: 0.0,
            minute_10: 0.0,
            minute_10_fluctuation: 0.0,
            hour: 0.0,
            hour_fluctuation: 0.0,
            day: 0.0,
            day_fluctuation: 0.0,
            three_day: 0.0,
            three_day_fluctuation: 0.0,
            week: 0.0,
            week_fluctuation: 0.0,
            duration: Duration::zero(),
            updated: util::unix_epoch(),
            failed: 0,
            power_draw_live: 0.0,
            power_draw_average: 0.0,
            diff_live: 0.0,
            diff_average: 0.0,
            ratio_live: 0.0,
            benchmarked: util::unix_epoch(),
            version: String::new(),
            log_file: String::new(),
            is_fl: false,
            failed_value: 0.0,
            failed_streak: 0,
            device_model: String::new(),
            hashrate_live: 0.0,
            hashrate_average: 0.0,
            blockrate_live: 0.0,
            blockrate_average: 0.0,
            actual24h_week: 0.0,
            estimate24h_week: 0.0,
            error_ratio: 0.0,
            deviation_streak: 0,
        }
    }
}

impl Stat {
    /// $Stat."$StatAverage"
    pub fn get_average(&self, name: &str) -> f64 {
        match name.to_lowercase().as_str() {
            "live" => self.live,
            "minute" => self.minute,
            "minute_5" => self.minute_5,
            "minute_10" => self.minute_10,
            "hour" => self.hour,
            "day" => self.day,
            "threeday" => self.three_day,
            "week" => self.week,
            _ => self.minute_10,
        }
    }

    fn from_json(v: &Value) -> Option<Stat> {
        if !v.is_object() {
            return None;
        }
        let n = |k: &str| jsonx::num(jsonx::get(v, k));
        let i = |k: &str| jsonx::num(jsonx::get(v, k)) as i32;
        let dt = |k: &str| -> Option<DateTime<Utc>> {
            let x = jsonx::get(v, k);
            match x {
                Value::String(s) => util::parse_datetime(s),
                Value::Number(_) => DateTime::from_timestamp_millis(jsonx::num(x) as i64),
                _ => None,
            }
        };
        let dur = match jsonx::get(v, "Duration") {
            Value::String(s) => util::parse_timespan(s).unwrap_or_else(Duration::zero),
            Value::Object(_) => {
                Duration::milliseconds(jsonx::num(jsonx::get(jsonx::get(v, "Duration"), "TotalMilliseconds")) as i64)
            }
            _ => Duration::zero(),
        };
        let updated = dt("Updated").unwrap_or_else(util::unix_epoch);
        Some(Stat {
            live: n("Live"),
            minute: n("Minute"),
            minute_fluctuation: n("Minute_Fluctuation"),
            minute_5: n("Minute_5"),
            minute_5_fluctuation: n("Minute_5_Fluctuation"),
            minute_10: n("Minute_10"),
            minute_10_fluctuation: n("Minute_10_Fluctuation"),
            hour: n("Hour"),
            hour_fluctuation: n("Hour_Fluctuation"),
            day: n("Day"),
            day_fluctuation: n("Day_Fluctuation"),
            three_day: n("ThreeDay"),
            three_day_fluctuation: n("ThreeDay_Fluctuation"),
            week: n("Week"),
            week_fluctuation: n("Week_Fluctuation"),
            duration: dur,
            updated,
            failed: i("Failed"),
            power_draw_live: n("PowerDraw_Live"),
            power_draw_average: n("PowerDraw_Average"),
            diff_live: n("Diff_Live"),
            diff_average: n("Diff_Average"),
            ratio_live: n("Ratio_Live"),
            benchmarked: dt("Benchmarked").unwrap_or(updated - dur),
            version: jsonx::s(jsonx::get(v, "Version")),
            log_file: jsonx::s(jsonx::get(v, "LogFile")),
            is_fl: jsonx::truthy(jsonx::get(v, "IsFL")),
            failed_value: n("Failed_Value"),
            failed_streak: i("Failed_Streak"),
            device_model: jsonx::s(jsonx::get(v, "DeviceModel")),
            hashrate_live: n("HashRate_Live"),
            hashrate_average: n("HashRate_Average"),
            blockrate_live: n("BlockRate_Live"),
            blockrate_average: n("BlockRate_Average"),
            actual24h_week: n("Actual24h_Week"),
            estimate24h_week: n("Estimate24h_Week"),
            error_ratio: n("ErrorRatio"),
            deviation_streak: i("Deviation_Streak"),
        })
    }

    pub fn to_json(&self, mode: StatMode) -> Value {
        let common = json!({
            "Live": self.live,
            "Minute": self.minute,
            "Minute_Fluctuation": self.minute_fluctuation,
            "Minute_5": self.minute_5,
            "Minute_5_Fluctuation": self.minute_5_fluctuation,
            "Minute_10": self.minute_10,
            "Minute_10_Fluctuation": self.minute_10_fluctuation,
            "Hour": self.hour,
            "Hour_Fluctuation": self.hour_fluctuation,
            "Day": self.day,
            "Day_Fluctuation": self.day_fluctuation,
            "ThreeDay": self.three_day,
            "ThreeDay_Fluctuation": self.three_day_fluctuation,
            "Week": self.week,
            "Week_Fluctuation": self.week_fluctuation,
            "Duration": util::format_timespan(self.duration),
            "Updated": util::format_datetime(&self.updated),
            "Failed": self.failed,
        });
        let mut v = common;
        match mode {
            StatMode::Miners => {
                jsonx::set(&mut v, "PowerDraw_Live", json!(self.power_draw_live));
                jsonx::set(&mut v, "PowerDraw_Average", json!(self.power_draw_average));
                jsonx::set(&mut v, "Diff_Live", json!(self.diff_live));
                jsonx::set(&mut v, "Diff_Average", json!(self.diff_average));
                jsonx::set(&mut v, "Ratio_Live", json!(self.ratio_live));
                jsonx::set(&mut v, "Benchmarked", json!(util::format_datetime(&self.benchmarked)));
                jsonx::set(&mut v, "Version", json!(self.version));
                jsonx::set(&mut v, "LogFile", json!(self.log_file));
                jsonx::set(&mut v, "IsFL", json!(self.is_fl));
                jsonx::set(&mut v, "Failed_Value", json!(self.failed_value));
                jsonx::set(&mut v, "Failed_Streak", json!(self.failed_streak));
                jsonx::set(&mut v, "DeviceModel", json!(self.device_model));
            }
            StatMode::Pools => {
                jsonx::set(&mut v, "HashRate_Live", json!(self.hashrate_live));
                jsonx::set(&mut v, "HashRate_Average", json!(self.hashrate_average));
                jsonx::set(&mut v, "BlockRate_Live", json!(self.blockrate_live));
                jsonx::set(&mut v, "BlockRate_Average", json!(self.blockrate_average));
                jsonx::set(&mut v, "Diff_Live", json!(self.diff_live));
                jsonx::set(&mut v, "Diff_Average", json!(self.diff_average));
                jsonx::set(&mut v, "Actual24h_Week", json!(self.actual24h_week));
                jsonx::set(&mut v, "Estimate24h_Week", json!(self.estimate24h_week));
                jsonx::set(&mut v, "ErrorRatio", json!(self.error_ratio));
                jsonx::set(&mut v, "Deviation_Streak", json!(self.deviation_streak));
            }
            StatMode::Profit => {
                jsonx::set(&mut v, "PowerDraw_Live", json!(self.power_draw_live));
                jsonx::set(&mut v, "PowerDraw_Average", json!(self.power_draw_average));
                jsonx::set(&mut v, "Failed_Value", json!(self.failed_value));
                jsonx::set(&mut v, "Failed_Streak", json!(self.failed_streak));
                jsonx::set(&mut v, "Deviation_Streak", json!(self.deviation_streak));
            }
        }
        v
    }

    /// API view (what the web interface gets from /stats)
    pub fn to_api_json(&self) -> Value {
        let mut v = self.to_json(StatMode::Miners);
        jsonx::set(&mut v, "HashRate_Live", json!(self.hashrate_live));
        jsonx::set(&mut v, "BlockRate_Average", json!(self.blockrate_average));
        v
    }
}

/// Parameters of Set-Stat (with the defaults of the PowerShell function)
#[derive(Debug, Clone)]
pub struct SetStat {
    pub name: String,
    pub value: f64,
    pub actual24h: f64,
    pub estimate24h: f64,
    pub difficulty: f64,
    pub ratio: f64,
    pub updated: Option<DateTime<Utc>>,
    pub start_time: Option<DateTime<Utc>>,
    pub duration: Duration,
    pub fault_detection: bool,
    pub change_detection: bool,
    pub fault_tolerance: f64,
    pub power_draw: f64,
    pub hashrate: f64,
    pub blockrate: f64,
    pub uplim_protection: f64,
    pub sub: String,
    pub version: String,
    pub log_file: String,
    pub device_model: String,
    pub quiet: bool,
    pub is_fastlane_value: bool,
    pub reset: bool,
    pub reset_if_zero: bool,
    pub max_error_ratio: f64,
}

impl SetStat {
    pub fn new(name: impl Into<String>, value: f64, duration: Duration) -> SetStat {
        SetStat {
            name: name.into(),
            value,
            actual24h: 0.0,
            estimate24h: 0.0,
            difficulty: 0.0,
            ratio: 0.0,
            updated: None,
            start_time: None,
            duration,
            fault_detection: false,
            change_detection: false,
            fault_tolerance: 0.1,
            power_draw: 0.0,
            hashrate: 0.0,
            blockrate: 0.0,
            uplim_protection: 0.0,
            sub: String::new(),
            version: String::new(),
            log_file: String::new(),
            device_model: String::new(),
            quiet: false,
            is_fastlane_value: false,
            reset: false,
            reset_if_zero: false,
            max_error_ratio: 0.0,
        }
    }
}

struct StatsCache {
    map: HashMap<String, Stat>,
    max_error_ratio: f64,
    miner_stats_stamp: Option<String>,
}

static CACHE: LazyLock<Mutex<StatsCache>> = LazyLock::new(|| {
    Mutex::new(StatsCache {
        map: HashMap::new(),
        max_error_ratio: 0.0,
        miner_stats_stamp: None,
    })
});

static STATS_DIR: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

/// override the Stats folder (default: <base>/Stats)
pub fn set_stats_dir(p: PathBuf) {
    *STATS_DIR.lock().unwrap() = Some(p);
}

fn stats_dir() -> PathBuf {
    if let Some(p) = STATS_DIR.lock().unwrap().clone() {
        return p;
    }
    crate::data::base_dir().join("Stats")
}

pub fn set_max_error_ratio(v: f64) {
    CACHE.lock().unwrap().max_error_ratio = v;
}

fn mode_of(name: &str) -> StatMode {
    let l = name.to_lowercase();
    if l.ends_with("_profit") {
        StatMode::Pools
    } else if l.ends_with("_hashrate") {
        StatMode::Miners
    } else {
        StatMode::Profit
    }
}

fn path_of(name: &str, sub: &str) -> (PathBuf, PathBuf) {
    let l = name.to_lowercase();
    let dir = if l.ends_with("_profit") || l.ends_with("_poolstats") {
        stats_dir().join("Pools")
    } else if l.ends_with("_hashrate") {
        stats_dir().join("Miners")
    } else if l.ends_with("_total") || l.ends_with("_totalavg") {
        stats_dir().join("Totals")
    } else if l.ends_with("_balance") {
        stats_dir().join("Balances")
    } else {
        stats_dir()
    };
    let file = if sub.is_empty() {
        dir.join(format!("{}.txt", name))
    } else {
        dir.join(format!("{}-{}.txt", sub, name))
    };
    (dir, file)
}

fn read_stat_file(path: &PathBuf) -> Result<Option<Stat>, ()> {
    if !path.exists() {
        return Ok(None);
    }
    match util::read_text(path).ok().and_then(|t| jsonx::parse(&t)) {
        Some(v) => Ok(Stat::from_json(&v)),
        None => Err(()),
    }
}

/// Get-StatFromFile
fn get_stat_from_file(path: &PathBuf, name: &str, cached: bool) -> Option<Stat> {
    let key = name.to_lowercase();
    if cached {
        let c = CACHE.lock().unwrap();
        if let Some(s) = c.map.get(&key) {
            if path.exists() {
                return Some(s.clone());
            }
        }
    }
    let r = match read_stat_file(path) {
        Ok(s) => s,
        Err(_) => {
            log_warn!(
                "Stat file ({}) is corrupt and will be removed. ",
                path.file_name().and_then(|f| f.to_str()).unwrap_or("")
            );
            let _ = std::fs::remove_file(path);
            None
        }
    };
    if cached {
        let mut c = CACHE.lock().unwrap();
        match &r {
            Some(s) => {
                c.map.insert(key, s.clone());
            }
            None => {
                c.map.remove(&key);
            }
        }
    }
    r
}

/// Get-Stat -Name
pub fn get_stat(name: &str, sub: &str) -> Option<Stat> {
    let (dir, file) = path_of(name, sub);
    let _ = std::fs::create_dir_all(&dir);
    let cached = matches!(mode_of(name), StatMode::Pools | StatMode::Miners);
    get_stat_from_file(&file, name, cached)
}

/// $Global:StatsCache[$Name]
pub fn cached(name: &str) -> Option<Stat> {
    CACHE.lock().unwrap().map.get(&name.to_lowercase()).cloned()
}

/// Keys of the stats cache (lower case)
pub fn cached_keys() -> Vec<String> {
    CACHE.lock().unwrap().map.keys().cloned().collect()
}

pub fn remove_cached(name: &str) {
    CACHE.lock().unwrap().map.remove(&name.to_lowercase());
}

/// update the version stamp of a cached miner stat and write the file
pub fn update_version(name: &str, sub: &str, version: &str) {
    let key = name.to_lowercase();
    let mut c = CACHE.lock().unwrap();
    if let Some(s) = c.map.get_mut(&key) {
        s.version = version.to_string();
        let (_, file) = path_of(name, sub);
        let _ = util::write_file_atomic(&file, s.to_json(StatMode::Miners).to_string().as_bytes());
    }
}

/// Get-Stat -Miners -Quiet: (re)load all miner stats into the cache.
/// Only stats whose devices are present are kept (present_devices = names like "GPU#00").
pub fn load_miner_stats(present_devices: &[String]) {
    let dir = stats_dir().join("Miners");
    let _ = std::fs::create_dir_all(&dir);
    let present: HashSet<String> = present_devices.iter().map(|d| d.to_uppercase()).collect();

    // skip the pass if folder and devices are unchanged
    let stamp = std::fs::metadata(&dir).and_then(|m| m.modified()).ok().and_then(|t| {
        let age = std::time::SystemTime::now().duration_since(t).ok()?;
        if age.as_secs() > 3 {
            let mut d: Vec<&String> = present.iter().collect();
            d.sort();
            Some(format!(
                "{:?}|{}",
                t,
                d.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")
            ))
        } else {
            None
        }
    });
    {
        let mut c = CACHE.lock().unwrap();
        if stamp.is_some() && stamp == c.miner_stats_stamp {
            return;
        }
        c.miner_stats_stamp = stamp;
    }

    let re_dev = regex::Regex::new(r"(?i)(?:GPU|CPU)#\d+").unwrap();
    let re_vendor = regex::Regex::new(r"^(AMD|CPU|INTEL|NVIDIA)-").unwrap();
    let mut found: HashSet<String> = HashSet::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("txt") {
                continue;
            }
            let base = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
            if !base.to_lowercase().ends_with("_hashrate") {
                continue;
            }
            let key = re_vendor.replace(&base, "").to_string();
            if !present.is_empty() {
                let devpart = &key[..key.find('_').unwrap_or(0)];
                if re_dev
                    .find_iter(devpart)
                    .any(|m| !present.contains(&m.as_str().to_uppercase()))
                {
                    continue;
                }
            }
            if get_stat_from_file(&p, &key, true).is_some() {
                found.insert(key.to_lowercase());
            }
        }
    }
    let mut c = CACHE.lock().unwrap();
    c.map
        .retain(|k, _| !k.ends_with("_hashrate") || found.contains(k));
}

/// Get-Stat -Pools: load all pool stats into the cache
pub fn load_pool_stats() {
    let dir = stats_dir().join("Pools");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            let base = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
            if base.to_lowercase().ends_with("_profit") {
                get_stat_from_file(&p, &base, true);
            }
        }
    }
}

/// Get-Stat -Disabled: keys of the disabled markers (vendor prefix removed, lower case)
pub fn get_disabled() -> HashSet<String> {
    let dir = stats_dir().join("Disabled");
    let _ = std::fs::create_dir_all(&dir);
    let re_vendor = regex::Regex::new(r"^(AMD|CPU|INTEL|NVIDIA)-").unwrap();
    let mut out = HashSet::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            let base = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
            let l = base.to_lowercase();
            if l.ends_with("_hashrate") || l.ends_with("_profit") {
                out.insert(re_vendor.replace(&base, "").to_lowercase());
            }
        }
    }
    out
}

/// Clear-ExpiredAutoDisabled: auto-disable markers older than `hours` are removed
pub fn clear_expired_auto_disabled(hours: i64) {
    if hours <= 0 {
        return;
    }
    let dir = stats_dir().join("Disabled");
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            let Some(v) = util::read_text(&p).ok().and_then(|t| jsonx::parse(&t)) else { continue };
            if !jsonx::truthy(jsonx::get(&v, "AutoDisabled")) {
                continue;
            }
            if let Some(when) = util::parse_datetime(&jsonx::s(jsonx::get(&v, "DisabledWhen"))) {
                if Utc::now() - when > Duration::hours(hours) {
                    log_info!("Auto-disable marker {} expired, re-enabling", p.display());
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
    }
}

/// Set-Stat
pub fn set_stat(p: SetStat) -> Stat {
    let now = Utc::now();
    let mut updated = p.updated.unwrap_or(now);
    let start_time = p.start_time.unwrap_or(now);
    let mode = mode_of(&p.name);
    let cached = mode != StatMode::Profit;
    let value = p.value;
    let mut estimate24h = p.estimate24h;
    if mode == StatMode::Pools && p.actual24h > 0.0 && estimate24h > 0.0 && estimate24h / p.actual24h > 1000.0 {
        estimate24h = p.actual24h;
    }
    let (dir, path) = path_of(&p.name, &p.sub);
    let log_quiet = p.quiet;
    let mut is_fastlane = p.is_fastlane_value;

    let mut stat: Option<Stat> = None;
    let mut reset_reason: Option<String> = None;

    let old = if p.reset { None } else { get_stat_from_file(&path, &p.name, cached) };

    if let Some(mut st) = old {
        // validity checks that restart the history
        let mut throw = None;
        if mode != StatMode::Miners {
            if st.week_fluctuation >= 0.8 && st.duration >= Duration::hours(1) {
                throw = Some(format!(
                    "Stat file ({}) will be reset, because its values fluctuate too much (week fluctuation {}%). ",
                    p.name,
                    (st.week_fluctuation * 100.0).round()
                ));
            } else if st.deviation_streak >= 15 {
                throw = Some(format!(
                    "Stat file ({}) will be reset, because its values stayed more than 5x off the average for {} rounds. ",
                    p.name, st.deviation_streak
                ));
            }
        }
        if throw.is_none() && p.reset_if_zero && st.live == 0.0 {
            throw = Some(format!("Stat file ({}) will be reset, because its last value is zero. ", p.name));
        }

        if let Some(t) = throw {
            reset_reason = Some(t);
        } else {
            let mut tol_min = value;
            let mut tol_max = value;
            let mut fault_tolerance = p.fault_tolerance;

            if p.fault_detection {
                if fault_tolerance < 1.0 {
                    tol_min = st.week
                        * (1.0 - (st.week_fluctuation * 2.0).max(fault_tolerance + st.failed as f64 / 100.0).min(0.9));
                    tol_max = st.week
                        * (1.0
                            + (st.week_fluctuation * 2.0)
                                .max(fault_tolerance + ((st.failed * 3).max(10)) as f64 / 100.0)
                                .min(2.0));
                } else if st.hour > 0.0 {
                    if fault_tolerance < 2.0 {
                        fault_tolerance = 2.0;
                    }
                    tol_min = st.hour / fault_tolerance;
                    tol_max = st.hour * fault_tolerance;
                }
            } else if st.hour > 0.0 && p.uplim_protection > 1.0 {
                tol_max = st.hour * p.uplim_protection;
            }

            if p.change_detection
                && (value - st.live).abs() < 1e-28
                && (mode != StatMode::Pools || (p.hashrate - st.hashrate_live).abs() < 1e-28)
            {
                updated = st.updated;
            }

            if value > 0.0 && tol_max == 0.0 {
                tol_max = value;
            }

            let mut rejected = value < tol_min || value > tol_max;
            let mut keep_streak = false;

            if rejected && mode == StatMode::Profit && p.uplim_protection > 1.0 && value > 0.0 {
                let matchv = st.failed_value > 0.0 && (value / st.failed_value - 1.0).abs() <= 0.5;
                st.failed_streak = if matchv { st.failed_streak + 1 } else { 1 };
                st.failed_value = value;
                if st.failed_streak >= 3 {
                    if !log_quiet {
                        log_warn!(
                            "Stat file ({}) accepts the value {}, it held for {} rounds above the spike protection. ",
                            p.name,
                            value,
                            st.failed_streak
                        );
                    }
                    rejected = false;
                    keep_streak = true;
                }
            }

            if rejected {
                let mut reset_value: Option<f64> = None;
                let mut reset_streak: Option<i32> = None;
                let mut drop_stat = false;
                st.failed += 10;
                if st.failed >= 30 {
                    st.failed = 30;
                    if mode == StatMode::Miners && st.is_fl {
                        reset_value = Some(st.week);
                        is_fastlane = false;
                        drop_stat = true;
                    }
                }
                if !drop_stat && mode == StatMode::Miners && value > 0.0 {
                    let mut matchv = false;
                    if st.failed_value > 0.0 {
                        if fault_tolerance < 1.0 {
                            matchv = (value / st.failed_value - 1.0).abs() <= (2.0 * fault_tolerance.max(0.1)).min(0.5);
                        } else {
                            let f = fault_tolerance.max(2.0);
                            matchv = value >= st.failed_value / f && value <= st.failed_value * f;
                        }
                    }
                    st.failed_streak = if matchv { st.failed_streak + 1 } else { 1 };
                    st.failed_value = value;
                    if st.failed_streak >= 3 {
                        reset_value = Some(st.week);
                        reset_streak = Some(st.failed_streak);
                        is_fastlane = false;
                        drop_stat = true;
                    }
                }
                if !log_quiet {
                    if mode == StatMode::Miners {
                        if let Some(streak) = reset_streak {
                            log_warn!("Stat file ({}) will be reset, because the stored value {} is too far off of the last {} measured values (now {}). ", p.name, util::convert_to_hash(reset_value.unwrap_or(0.0)), streak, util::convert_to_hash(value));
                        } else if let Some(rv) = reset_value {
                            log_warn!("Stat file ({}) will be reset, because the seeded value {} (fastlane or derived from another device set) is too far off of {}. ", p.name, util::convert_to_hash(rv), util::convert_to_hash(value));
                        } else {
                            log_warn!("Stat file ({}) was not updated because the value {} is outside fault tolerance {} to {}. ", p.name, util::convert_to_hash(value), util::convert_to_hash(tol_min), util::convert_to_hash(tol_max));
                        }
                    } else if p.uplim_protection > 1.0 {
                        log_warn!("Stat file ({}) was not updated because the value {} is at least {} times above the hourly average. ", p.name, value, p.uplim_protection);
                    } else {
                        log_warn!("Stat file ({}) was not updated because the value {} is outside fault tolerance {} to {}. ", p.name, value, tol_min, tol_max);
                    }
                }
                if !drop_stat {
                    stat = Some(st);
                }
            } else {
                let mut deviation_streak = 0;
                if mode != StatMode::Miners && value > 0.0 && st.week > 0.0 {
                    let ratio = value / st.week;
                    if !(0.2..=5.0).contains(&ratio) {
                        deviation_streak = st.deviation_streak + 1;
                    }
                }
                let dmin = p.duration.num_milliseconds() as f64 / 60000.0;
                let smin = st.duration.num_milliseconds() as f64 / 60000.0;
                let span = |div: f64| -> f64 { ((dmin / div) / (smin / div).min(1.0)).min(1.0) };
                let span_minute = span(1.0);
                let span_minute_5 = span(5.0);
                let span_minute_10 = span(10.0);
                let span_hour = span(60.0);
                let span_day = span(1440.0);
                let span_three_day = span(4320.0);
                let span_week = span(10080.0);

                let avg = |old: f64, s: f64| old + s * (value - old);
                let fluct = |old_avg: f64, old_fl: f64, s: f64| {
                    old_fl + s * (((value - old_avg).abs() / old_avg.abs().max(SMALLEST_VALUE)).min(1.0) - old_fl)
                };

                let mut ns = Stat {
                    live: value,
                    minute: avg(st.minute, span_minute),
                    minute_fluctuation: fluct(st.minute, st.minute_fluctuation, span_minute),
                    minute_5: avg(st.minute_5, span_minute_5),
                    minute_5_fluctuation: fluct(st.minute_5, st.minute_5_fluctuation, span_minute_5),
                    minute_10: avg(st.minute_10, span_minute_10),
                    minute_10_fluctuation: fluct(st.minute_10, st.minute_10_fluctuation, span_minute_10),
                    hour: avg(st.hour, span_hour),
                    hour_fluctuation: fluct(st.hour, st.hour_fluctuation, span_hour),
                    day: avg(st.day, span_day),
                    day_fluctuation: fluct(st.day, st.day_fluctuation, span_day),
                    three_day: avg(st.three_day, span_three_day),
                    three_day_fluctuation: fluct(st.three_day, st.three_day_fluctuation, span_three_day),
                    week: avg(st.week, span_week),
                    week_fluctuation: fluct(st.week, st.week_fluctuation, span_week),
                    duration: st.duration + p.duration,
                    updated,
                    failed: (st.failed - 1).max(0),
                    ..Default::default()
                };
                match mode {
                    StatMode::Miners => {
                        ns.power_draw_live = p.power_draw;
                        ns.power_draw_average = if st.power_draw_average > 0.0 {
                            st.power_draw_average + span_week * (p.power_draw - st.power_draw_average)
                        } else {
                            p.power_draw
                        };
                        ns.diff_live = p.difficulty;
                        ns.diff_average = st.diff_average + span_hour * (p.difficulty - st.diff_average);
                        ns.ratio_live = p.ratio;
                        ns.benchmarked = st.benchmarked;
                        ns.version = p.version.clone();
                        ns.log_file = p.log_file.clone();
                        ns.is_fl = false;
                        ns.failed_value = 0.0;
                        ns.failed_streak = 0;
                        ns.device_model = if !p.device_model.is_empty() {
                            p.device_model.clone()
                        } else {
                            st.device_model.clone()
                        };
                    }
                    StatMode::Pools => {
                        ns.hashrate_live = p.hashrate;
                        ns.hashrate_average = if st.hashrate_average > 0.0 {
                            st.hashrate_average + span_hour * (p.hashrate - st.hashrate_average)
                        } else {
                            p.hashrate
                        };
                        ns.blockrate_live = p.blockrate;
                        ns.blockrate_average = if st.blockrate_average > 0.0 {
                            st.blockrate_average + span_hour * (p.blockrate - st.blockrate_average)
                        } else {
                            p.blockrate
                        };
                        ns.diff_live = p.difficulty;
                        ns.diff_average = st.diff_average + span_hour * (p.difficulty - st.diff_average);
                        ns.actual24h_week = st.actual24h_week + span_day * (p.actual24h - st.actual24h_week);
                        ns.estimate24h_week = st.estimate24h_week + span_day * (estimate24h - st.estimate24h_week);
                        ns.error_ratio = st.error_ratio;
                        ns.deviation_streak = deviation_streak;
                    }
                    StatMode::Profit => {
                        ns.failed_value = if keep_streak { st.failed_value } else { 0.0 };
                        ns.failed_streak = if keep_streak { st.failed_streak } else { 0 };
                        ns.deviation_streak = deviation_streak;
                        ns.power_draw_live = p.power_draw;
                        ns.power_draw_average = if st.power_draw_average > 0.0 {
                            st.power_draw_average + span_day * (p.power_draw - st.power_draw_average)
                        } else {
                            p.power_draw
                        };
                    }
                }
                for f in [
                    &mut ns.minute_fluctuation,
                    &mut ns.minute_5_fluctuation,
                    &mut ns.minute_10_fluctuation,
                    &mut ns.hour_fluctuation,
                    &mut ns.day_fluctuation,
                    &mut ns.three_day_fluctuation,
                    &mut ns.week_fluctuation,
                ] {
                    if *f > 1.0 {
                        *f = 0.0;
                    }
                }
                stat = Some(ns);
            }
        }
    }

    if let Some(r) = reset_reason {
        if !log_quiet && path.exists() {
            log_warn!("{}", r);
        }
    }

    let mut stat = stat.unwrap_or_else(|| Stat {
        live: value,
        minute: value,
        minute_5: value,
        minute_10: value,
        hour: value,
        day: value,
        three_day: value,
        week: value,
        duration: p.duration,
        updated,
        failed: 0,
        power_draw_live: p.power_draw,
        power_draw_average: p.power_draw,
        diff_live: p.difficulty,
        diff_average: p.difficulty,
        ratio_live: p.ratio,
        benchmarked: start_time,
        version: p.version.clone(),
        log_file: p.log_file.clone(),
        is_fl: is_fastlane,
        device_model: p.device_model.clone(),
        hashrate_live: p.hashrate,
        hashrate_average: p.hashrate,
        blockrate_live: p.blockrate,
        blockrate_average: p.blockrate,
        ..Default::default()
    });

    if mode == StatMode::Pools {
        let days = stat.duration.num_milliseconds() as f64 / 86400000.0;
        let corr = if stat.estimate24h_week != 0.0 {
            (stat.actual24h_week / stat.estimate24h_week - 1.0)
                * if days < 7.0 { days / 7.0 * (2.0 - days / 7.0) } else { 1.0 }
        } else {
            0.0
        };
        stat.error_ratio = (1.0 + corr).min(days.max(1.0));
        let max_er = if p.max_error_ratio > 0.0 {
            p.max_error_ratio
        } else {
            CACHE.lock().unwrap().max_error_ratio
        };
        if max_er > 0.0 && stat.error_ratio > max_er {
            stat.error_ratio = max_er;
        }
    }

    let _ = std::fs::create_dir_all(&dir);
    if stat.duration != Duration::zero() {
        if let Err(e) = util::write_file_atomic(&path, stat.to_json(mode).to_string().as_bytes()) {
            if !log_quiet {
                log_warn!("Could not write stat file ({}): {} ", p.name, e);
            }
        }
    }
    if cached {
        CACHE.lock().unwrap().map.insert(p.name.to_lowercase(), stat.clone());
    }
    stat
}

/// Remove-Item for stats files matching a miner stat key (used when combos are reset)
pub fn remove_miner_stat_files(stat_key: &str) {
    let dir = stats_dir().join("Miners");
    let suffix = format!("-{}.txt", stat_key).to_lowercase();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            if n.ends_with(&suffix) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    remove_cached(stat_key);
}

/// Write a disabled marker (Stats/Disabled/<Vendor>-<Name>.txt)
pub fn write_disabled_marker(vendor: &str, name: &str, reason: &str, auto: bool) -> bool {
    let dir = stats_dir().join("Disabled");
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join(format!("{}-{}.txt", vendor, name));
    if p.exists() {
        return false;
    }
    let v = json!({"DisabledWhen": util::format_datetime(&Utc::now()), "AutoDisabled": auto, "Reason": reason});
    util::write_file_atomic(&p, v.to_string().as_bytes()).is_ok()
}

/// all stats as JSON map for the API (/stats)
pub fn api_stats() -> Value {
    let c = CACHE.lock().unwrap();
    let mut m = serde_json::Map::new();
    let mut keys: Vec<&String> = c.map.keys().collect();
    keys.sort();
    for k in keys {
        m.insert(k.clone(), c.map[k].to_api_json());
    }
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir() {
        static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        let d = DIR.get_or_init(|| tempfile::tempdir().unwrap());
        set_stats_dir(d.path().join("Stats"));
    }

    #[test]
    fn moving_average() {
        test_dir();
        let s1 = set_stat(SetStat::new("Test_Ethash_Profit", 100.0, Duration::minutes(1)));
        assert_eq!(s1.live, 100.0);
        assert_eq!(s1.week, 100.0);
        // a young stat: the span is relative to its own duration
        let s2 = set_stat(SetStat::new("Test_Ethash_Profit", 200.0, Duration::minutes(1)));
        assert_eq!(s2.live, 200.0);
        assert_eq!(s2.minute, 200.0);
        assert_eq!(s2.hour, 200.0);
        let s3 = set_stat(SetStat::new("Test_Ethash_Profit", 300.0, Duration::minutes(1)));
        assert_eq!(s3.minute, 300.0);
        assert!((s3.hour - 250.0).abs() < 1e-9);
        assert!((s3.week - 250.0).abs() < 1e-9);
        let back = get_stat("Test_Ethash_Profit", "").unwrap();
        assert!((back.hour - s3.hour).abs() < 1e-9);
        assert_eq!(back.duration, Duration::minutes(3));
    }

    #[test]
    fn fault_detection() {
        test_dir();
        let mut p = SetStat::new("Miner-GPU#00_Ethash_HashRate", 1000.0, Duration::minutes(1));
        p.sub = "NVIDIA".into();
        p.fault_detection = true;
        set_stat(p.clone());
        let mut p2 = p.clone();
        p2.value = 5000.0;
        let s = set_stat(p2);
        // rejected, stays at 1000
        assert_eq!(s.week, 1000.0);
        assert_eq!(s.failed, 10);
    }
}
